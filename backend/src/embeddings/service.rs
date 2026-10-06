//! Embedding service: one interface, swappable backend.
//!
//! `embed(texts) -> Vec<Vec<f32>>` -- always [`DIM`]-dimensional,
//! order-preserving.
//!
//! Backend is selected by `settings.embeddings_backend` (env-only, never a
//! DB-backed setting -- see `config.rs`):
//!   - `"openai"` -- real OpenAI embeddings API call, model
//!     `text-embedding-3-small`, batched.
//!   - `"stub"`   -- deterministic SHA256-derived vector, zero network, for
//!     tests.
//!
//! Results are memoized in the `embed_cache` SurrealDB table, keyed by
//! HMAC(ENCRYPTION_KEY, "backend:model:text") -- ported from
//! `embeddings/service.py`'s `_hmac`.
//!
//! Ported from `embeddings/service.py`.

use std::collections::HashMap;

use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use surrealdb::RecordId;

use crate::config::Settings;
use crate::db::Db;
use crate::error::{AppError, AppResult};

pub const DIM: usize = 1536;
const BATCH: usize = 64;
const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256(key, s), hex-encoded. Mirrors `embeddings/service.py`'s
/// `_hmac` (`hmac.new(key, s.encode(), hashlib.sha256).hexdigest()`).
fn hmac_hex(key: &str, s: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC accepts a key of any length");
    mac.update(s.as_bytes());
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// Deterministic SHA256-derived vector: repeat the 32-byte digest to fill
/// `DIM`, then map each byte from `[0, 255]` to `[-1.0, 1.0)`. Mirrors
/// `embeddings/service.py`'s `_stub_vec`.
fn stub_vec(text: &str) -> Vec<f32> {
    let digest = Sha256::digest(text.as_bytes());
    let mut raw = Vec::with_capacity(DIM);
    while raw.len() < DIM {
        raw.extend_from_slice(&digest);
    }
    raw.truncate(DIM);
    raw.into_iter().map(|b| (b as f32 / 255.0) * 2.0 - 1.0).collect()
}

fn embed_stub(texts: &[String]) -> Vec<Vec<f32>> {
    texts.iter().map(|t| stub_vec(t)).collect()
}

#[derive(Debug, Deserialize)]
struct EmbeddingDatum {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingsResponse {
    #[serde(default)]
    data: Vec<EmbeddingDatum>,
}

/// Calls `{base_url}/embeddings`, batched `BATCH` at a time, re-sorting each
/// batch's response by `index` (the API doesn't guarantee response order
/// matches request order). Mirrors `embeddings/service.py`'s `_embed_openai`.
async fn embed_openai(texts: &[String], base_url: &str, api_key: &str) -> AppResult<Vec<Vec<f32>>> {
    let client = reqwest::Client::new();
    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
    let auth_key = if api_key.is_empty() { "not-needed" } else { api_key };

    let mut out = Vec::with_capacity(texts.len());
    for chunk in texts.chunks(BATCH) {
        let resp = client
            .post(&url)
            .bearer_auth(auth_key)
            .json(&json!({ "model": "text-embedding-3-small", "input": chunk }))
            .send()
            .await
            .map_err(|e| AppError::internal(format!("OpenAI embeddings request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::internal(format!("OpenAI embeddings HTTP {status}: {body}")));
        }

        let mut parsed: EmbeddingsResponse = resp
            .json()
            .await
            .map_err(|e| AppError::internal(format!("invalid OpenAI embeddings response: {e}")))?;
        parsed.data.sort_by_key(|d| d.index);
        out.extend(parsed.data.into_iter().map(|d| d.embedding));
    }

    for v in &out {
        if v.len() != DIM {
            return Err(AppError::internal(format!("OpenAI returned dim {}, expected {DIM}.", v.len())));
        }
    }
    Ok(out)
}

pub fn dim() -> usize {
    DIM
}

#[derive(Debug, Default, Deserialize)]
struct AppSettingsOpenaiRow {
    #[serde(default)]
    openai_api_key_encrypted: String,
    #[serde(default)]
    openai_base_url: String,
}

/// Resolve `(base_url, api_key)` for `owner`'s OpenAI-compatible backend.
///
/// Deferred: `connectors::service` (the Rust port of `connectors/service.py`)
/// only covers connector CRUD so far -- it explicitly doesn't yet expose
/// `get_app_settings`/`resolve_openai` (see that module's doc comment). Doing
/// a direct `app_settings` read here, rather than depending on a function
/// that doesn't exist yet, mirrors the same direct-read already done in
/// `routers/settings.rs::resolve_openai` (also not encrypting the stored key,
/// for the same reason noted there -- it's stored as-is today). Once
/// `connectors::service` grows a real `resolve_openai`, both call sites
/// should switch to it instead of duplicating this query.
pub(crate) async fn resolve_openai_for_owner(db: &Db, owner: &RecordId, env_api_key: &Option<String>) -> AppResult<(String, String)> {
    let rid = RecordId::from_table_key("app_settings", owner.key().clone());
    let row: Option<AppSettingsOpenaiRow> = db.select(rid).await?;
    let row = row.unwrap_or_default();

    let base_url = if row.openai_base_url.is_empty() { DEFAULT_OPENAI_BASE_URL.to_string() } else { row.openai_base_url };
    let key = if !row.openai_api_key_encrypted.is_empty() {
        row.openai_api_key_encrypted
    } else {
        env_api_key.clone().unwrap_or_default()
    };
    Ok((base_url, key))
}

#[derive(Debug, Deserialize)]
struct EmbedCacheRow {
    text_hmac: String,
    vector: Vec<f32>,
}

/// Embed `texts`, using the SurrealDB-backed memo (`embed_cache`) for ones
/// seen before.
///
/// `owner`, if given, resolves that user's OpenAI base_url/key (per-user
/// override of the env-level default) -- omit it only for owner-less call
/// sites, which fall back to the env settings exactly as before. Mirrors
/// `embeddings/service.py`'s `embed`.
pub async fn embed(db: &Db, settings: &Settings, texts: &[String], owner: Option<&RecordId>) -> AppResult<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let backend = settings.embeddings_backend.as_str();
    let model = if backend == "openai" { "text-embedding-3-small" } else { "stub" };
    let keys: Vec<String> =
        texts.iter().map(|t| hmac_hex(&settings.encryption_key, &format!("{backend}:{model}:{t}"))).collect();

    let mut res = db
        .query("SELECT text_hmac, vector FROM embed_cache WHERE text_hmac IN $keys")
        .bind(("keys", keys.clone()))
        .await?;
    let rows: Vec<EmbedCacheRow> = res.take(0)?;
    let mut cached: HashMap<String, Vec<f32>> = rows.into_iter().map(|r| (r.text_hmac, r.vector)).collect();

    let missing_idx: Vec<usize> = keys.iter().enumerate().filter(|(_, k)| !cached.contains_key(*k)).map(|(i, _)| i).collect();
    if !missing_idx.is_empty() {
        let fresh_texts: Vec<String> = missing_idx.iter().map(|&i| texts[i].clone()).collect();
        let vecs = if backend == "openai" {
            let (base_url, api_key) = match owner {
                Some(o) => resolve_openai_for_owner(db, o, &settings.openai_api_key).await?,
                None => (DEFAULT_OPENAI_BASE_URL.to_string(), settings.openai_api_key.clone().unwrap_or_default()),
            };
            embed_openai(&fresh_texts, &base_url, &api_key).await?
        } else {
            embed_stub(&fresh_texts)
        };

        for (i, v) in missing_idx.into_iter().zip(vecs.into_iter()) {
            let k = keys[i].clone();
            db.query("UPSERT $id SET text_hmac = $hmac, vector = $vector")
                .bind(("id", RecordId::from_table_key("embed_cache", k.clone())))
                .bind(("hmac", k.clone()))
                .bind(("vector", v.clone()))
                .await?;
            cached.insert(k, v);
        }
    }

    Ok(keys.iter().map(|k| cached.get(k).cloned().unwrap_or_default()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_is_1536() {
        assert_eq!(dim(), 1536);
    }

    #[test]
    fn hmac_hex_is_deterministic() {
        assert_eq!(hmac_hex("key", "value"), hmac_hex("key", "value"));
    }

    #[test]
    fn hmac_hex_changes_with_key_or_input() {
        let base = hmac_hex("key", "value");
        assert_ne!(base, hmac_hex("other-key", "value"));
        assert_ne!(base, hmac_hex("key", "other-value"));
    }

    #[test]
    fn hmac_hex_is_64_hex_chars() {
        let h = hmac_hex("key", "value");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn stub_vec_has_correct_dimension() {
        assert_eq!(stub_vec("hello world").len(), DIM);
    }

    #[test]
    fn stub_vec_is_deterministic_and_in_range() {
        let a = stub_vec("same text");
        let b = stub_vec("same text");
        assert_eq!(a, b);
        assert!(a.iter().all(|v| *v >= -1.0 && v < &1.0));
    }

    #[test]
    fn stub_vec_differs_for_different_text() {
        assert_ne!(stub_vec("a"), stub_vec("b"));
    }

    #[test]
    fn embed_stub_preserves_order_and_count() {
        let texts = vec!["one".to_string(), "two".to_string(), "three".to_string()];
        let out = embed_stub(&texts);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], stub_vec("one"));
        assert_eq!(out[1], stub_vec("two"));
        assert_eq!(out[2], stub_vec("three"));
    }
}

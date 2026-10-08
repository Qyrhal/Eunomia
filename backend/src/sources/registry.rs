//! Source discovery + the operations that run across all sources. Ported
//! from `sources/registry.py`.
//!
//! Python's `discover()` dynamically imports every `sources/<pkg>/` exposing
//! a module-level `SOURCE`. Rust has no runtime package discovery, so
//! [`all`] lists the four ported sources explicitly -- same registry shape
//! (`key -> Source`), a static list instead of filesystem scanning.
//!
//! `tool_registry()` (the per-source MCP tool surface) is not ported: see
//! `sources::base`'s module doc.

use surrealdb::types::SurrealValue;
use std::collections::HashSet;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use surrealdb::types::RecordId;
use crate::rid::RecordIdExt;

use crate::connectors::service;
use crate::pool::OrgDb;
use crate::state::OrgState;
use crate::store;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::sources::base::{owner_key_str, Source, SourceCtx};
use crate::sources::demo::DemoSource;
use crate::sources::discord::DiscordSource;
use crate::sources::example::ExampleSource;
use crate::sources::github::GitHubSource;
use crate::sources::gmail::GmailSource;
use crate::sources::google_calendar::GoogleCalendarSource;
use crate::sources::heypocket::HeyPocketSource;
use crate::sources::linear::LinearSource;
use crate::sources::notion::NotionSource;
use crate::sources::slack::SlackSource;
use crate::sources::spotify::SpotifySource;
use crate::sources::stripe::StripeSource;
use crate::sources::todoist::TodoistSource;
use crate::sources::up_bank::UpBankSource;

/// Every known source. A fresh `Vec` of trait-object handles each call --
/// cheap (small zero-sized structs) and avoids needing a lazily-initialized
/// global registry for what Python builds once at import time via
/// `discover()`.
pub fn all() -> Vec<Arc<dyn Source>> {
    vec![
        Arc::new(DemoSource) as Arc<dyn Source>,
        Arc::new(ExampleSource) as Arc<dyn Source>,
        Arc::new(HeyPocketSource) as Arc<dyn Source>,
        Arc::new(UpBankSource) as Arc<dyn Source>,
        Arc::new(GitHubSource) as Arc<dyn Source>,
        Arc::new(SlackSource) as Arc<dyn Source>,
        Arc::new(NotionSource) as Arc<dyn Source>,
        Arc::new(LinearSource) as Arc<dyn Source>,
        Arc::new(GmailSource) as Arc<dyn Source>,
        Arc::new(GoogleCalendarSource) as Arc<dyn Source>,
        Arc::new(DiscordSource) as Arc<dyn Source>,
        Arc::new(SpotifySource) as Arc<dyn Source>,
        Arc::new(TodoistSource) as Arc<dyn Source>,
        Arc::new(StripeSource) as Arc<dyn Source>,
    ]
}

pub fn get(key: &str) -> Option<Arc<dyn Source>> {
    all().into_iter().find(|s| s.key() == key)
}

/// The connector row that holds this source's credentials, scoped to `owner`.
pub async fn connector_for(db: &OrgDb, owner: &RecordId, src: &dyn Source) -> AppResult<Option<service::Connector>> {
    service::get_connector(db, owner, src.provider_key()).await
}

pub async fn credentials_for(db: &OrgDb, encryption_key: &str, owner: &RecordId, src: &dyn Source) -> AppResult<Value> {
    service::credentials_for(db, encryption_key, owner, src.provider_key()).await
}

#[derive(Debug, Deserialize, SurrealValue)]
struct ConnectorKindRow {
    kind: String,
    #[serde(default)]
    #[surreal(default)]
    config: Value,
}

/// Every registered source whose connector is enabled for `owner`, excluding
/// ones in demo mode (a missing `demo` key and an explicit `false` both count
/// as "not demo mode", filtered in Rust rather than the query, same as the
/// Python version's comment explains).
pub async fn enabled(db: &OrgDb, owner: &RecordId) -> AppResult<Vec<Arc<dyn Source>>> {
    let mut res = store::app::CONNECTOR_ENABLED
        .on(db)
        .bind(("owner", owner.clone()))
        .await?;
    let rows: Vec<ConnectorKindRow> = res.take(0)?;
    let on: HashSet<String> = rows
        .into_iter()
        .filter(|r| !r.config.get("demo").and_then(|v| v.as_bool()).unwrap_or(false))
        .map(|r| r.kind)
        .collect();
    Ok(all().into_iter().filter(|s| on.contains(s.provider_key())).collect())
}

/// A faithful-but-reduced port of `cache/ingest.py::ingest` for the
/// map -> upsert stage only. `cache/ingest.py` also runs an embedding stage
/// (`cache.search.set_embedding` + `embeddings.service.embed`) and an entity
/// extraction + consolidation stage (`entities.extract`, `entities.consolidate`)
/// after every write; neither `cache.search`'s embedding half nor `entities.*`
/// is ported to Rust yet, so both are skipped here rather than faked. Every
/// `cache_record` written by this path lands with `embedding = NONE`, exactly
/// the state the `embed` job reconciler (`jobs::handlers`) looks for and fixes.
pub struct IngestReport {
    pub source: String,
    pub written: i64,
    pub skipped: i64,
    pub failed: i64,
    pub errors: Vec<String>,
}

impl IngestReport {
    fn new(source: &str) -> Self {
        IngestReport { source: source.to_string(), written: 0, skipped: 0, failed: 0, errors: Vec::new() }
    }

    /// Mirrors Python's `IngestReport.as_dict()`: errors capped to the first 20.
    pub fn as_value(&self) -> Value {
        serde_json::json!({
            "source": self.source,
            "written": self.written,
            "skipped": self.skipped,
            "failed": self.failed,
            "errors": self.errors.iter().take(20).collect::<Vec<_>>(),
        })
    }
}

fn cache_record_id(owner: &RecordId, literal_id: &str) -> RecordId {
    RecordId::from_table_key("cache_record", format!("{}:{}", owner_key_str(owner), literal_id))
}

/// SHA-256 over the envelope fields that determine whether a record actually
/// changed, mirroring `cache/search.py::_hash_envelope`. Canonicalized via
/// `serde_json`'s key-sorted map so the hash is stable across calls within
/// this process (not required to match the Python implementation's hash
/// bytes -- only to be internally consistent for change detection).
/// Recursively sorts object keys so the hash below doesn't depend on
/// `serde_json`'s `Map` insertion-vs-sorted-order feature flag, or on the key
/// order a source's `map()` happened to build its `payload` object in.
fn canonicalize(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut out = serde_json::Map::new();
            for (k, val) in entries {
                out.insert(k.clone(), canonicalize(val));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

fn hash_envelope(env: &Value) -> String {
    use sha2::{Digest, Sha256};
    let keys = ["title", "body_text", "url", "occurred_at", "payload", "deleted"];
    let mut map = serde_json::Map::new();
    for k in keys {
        map.insert(k.to_string(), env.get(k).cloned().unwrap_or(Value::Null));
    }
    let blob = serde_json::to_string(&canonicalize(&Value::Object(map))).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(blob.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Deserialize, SurrealValue)]
struct ExistingRecord {
    #[serde(default)]
    #[surreal(default)]
    content_hash: String,
    #[serde(default)]
    #[surreal(default)]
    deleted: bool,
}

fn str_field(env: &Value, key: &str) -> String {
    env.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

async fn upsert_one(db: &OrgDb, owner: &RecordId, env: &Value) -> AppResult<bool> {
    let literal_id = str_field(env, "id");
    let rid = cache_record_id(owner, &literal_id);
    let h = hash_envelope(env);

    let existing: Option<ExistingRecord> = store::get(db, &rid).await?;
    let now = chrono::Utc::now();
    if let Some(existing) = &existing
        && existing.content_hash == h && !existing.deleted {
            store::app::SOURCES_RECORD_TOUCH
                .on(db)
                .bind(("id", rid))
                .bind(("now", surrealdb::types::Datetime::from(now)))
                .await?;
            return Ok(false);
        }

    let occurred_at: Option<surrealdb::types::Datetime> = env
        .get("occurred_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| surrealdb::types::Datetime::from(dt.with_timezone(&chrono::Utc)));

    store::app::SOURCES_RECORD_UPSERT
    .on(db)
    .bind(("id", rid))
    .bind(("owner", owner.clone()))
    .bind(("source", str_field(env, "source")))
    .bind(("type", str_field(env, "type")))
    .bind(("external_id", str_field(env, "external_id")))
    .bind(("title", str_field(env, "title")))
    .bind(("body_text", str_field(env, "body_text")))
    .bind(("occurred_at", occurred_at))
    .bind(("url", str_field(env, "url")))
    .bind(("payload", env.get("payload").cloned().unwrap_or(Value::Object(Default::default()))))
    .bind(("content_hash", h))
    .bind(("now", surrealdb::types::Datetime::from(now)))
    .bind(("deleted", env.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false)))
    .await?;

    // Deferred: `cache/search.py::_reconcile_links` (the `linked_to` edge
    // sync for `env["links"]`) is not ported -- none of the four ported
    // sources ever emit a non-empty `links` list, so there is nothing to
    // reconcile yet.
    Ok(true)
}

/// Map -> upsert every raw record for `source_key`, scoped to `owner`.
pub async fn ingest(state: &OrgState, owner: &RecordId, source_key: &str, raw_records: &[Value], src: &dyn Source) -> IngestReport {
    let db = &state.db;
    let mut report = IngestReport::new(source_key);

    for raw in raw_records {
        let Some(mut env) = src.map(raw) else {
            report.skipped += 1;
            continue;
        };
        if env.get("source").is_none()
            && let Some(obj) = env.as_object_mut() {
                obj.insert("source".to_string(), Value::String(source_key.to_string()));
            }

        match upsert_one(db, owner, &env).await {
            Ok(true) => {
                report.written += 1;
                if !env.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false) {
                    crate::jobs::handlers::enqueue_extract(state, owner, &str_field(&env, "id"), &hash_envelope(&env)).await;
                }
            }
            Ok(false) => report.skipped += 1,
            Err(e) => {
                report.failed += 1;
                report.errors.push(format!("{}: {}", str_field(&env, "id"), e.message));
            }
        }
    }

    // Enrichment is level-triggered: the `embed` reconciler finds records with no embedding,
    // and the `extract` job queued above feeds entity extraction and consolidation.
    report
}

/// Builds the per-source-call context. A thin constructor so call sites read
/// like the Python `registry.run_sync(owner, key, mode, cursor)` call.
pub fn ctx<'a>(db: &'a OrgDb, encryption_key: &'a str, owner: &'a RecordId) -> SourceCtx<'a> {
    SourceCtx { db, encryption_key, owner }
}

/// Sync one source through the (reduced) ingest pipeline, scoped to `owner`.
/// Returns `(report, next_cursor)`.
pub async fn run_sync(
    state: &OrgState,
    owner: &RecordId,
    key: &str,
    mode: &str,
    cursor: Option<String>,
) -> AppResult<(IngestReport, Option<String>)> {
    let src = get(key).ok_or_else(|| AppError::coded(ErrorCode::SourceNotFound, format!("no source {key:?}")))?;
    let source_ctx = ctx(&state.db, &state.settings.encryption_key, owner);
    let result = src.sync(&source_ctx, mode, cursor).await?;
    let report = ingest(state, owner, key, &result.records, src.as_ref()).await;
    Ok((report, result.cursor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn all_registers_every_ported_source_with_a_unique_key() {
        let keys: Vec<&str> = all().iter().map(|s| s.key()).collect();
        assert_eq!(keys.len(), 14);
        let unique: HashSet<&str> = keys.iter().copied().collect();
        assert_eq!(unique.len(), 14);
        for k in [
            "demo", "example", "heypocket", "up_bank", "github", "slack", "notion", "linear", "gmail",
            "google_calendar", "discord", "spotify", "todoist", "stripe",
        ] {
            assert!(keys.contains(&k), "missing {k}");
        }
    }

    #[test]
    fn get_returns_none_for_unknown_key() {
        assert!(get("nonexistent").is_none());
    }

    #[test]
    fn provider_key_defaults_match_python() {
        // demo.provider == "demo" (explicit); heypocket/up_bank set a
        // distinct provider; example has none, so falls back to its key.
        assert_eq!(get("demo").unwrap().provider_key(), "demo");
        assert_eq!(get("heypocket").unwrap().provider_key(), "pocketai");
        assert_eq!(get("up_bank").unwrap().provider_key(), "up_bank");
        assert_eq!(get("example").unwrap().provider_key(), "example");
    }

    #[test]
    fn hash_envelope_changes_when_a_tracked_field_changes() {
        let a = json!({"title": "A", "body_text": "x", "url": "", "occurred_at": null, "payload": {}, "deleted": false});
        let b = json!({"title": "B", "body_text": "x", "url": "", "occurred_at": null, "payload": {}, "deleted": false});
        assert_ne!(hash_envelope(&a), hash_envelope(&b));
    }

    #[test]
    fn hash_envelope_stable_regardless_of_key_order() {
        let a = json!({"title": "A", "body_text": "x", "url": "", "occurred_at": null, "payload": {"a": 1, "b": 2}, "deleted": false});
        let b = json!({"deleted": false, "payload": {"b": 2, "a": 1}, "occurred_at": null, "url": "", "body_text": "x", "title": "A"});
        assert_eq!(hash_envelope(&a), hash_envelope(&b));
    }

    #[test]
    fn hash_envelope_ignores_untracked_fields() {
        let a = json!({"title": "A", "body_text": "x", "url": "", "occurred_at": null, "payload": {}, "deleted": false, "id": "1", "source": "s"});
        let b = json!({"title": "A", "body_text": "x", "url": "", "occurred_at": null, "payload": {}, "deleted": false, "id": "2", "source": "t"});
        assert_eq!(hash_envelope(&a), hash_envelope(&b));
    }

    #[test]
    fn ingest_report_as_value_caps_errors_at_20() {
        let mut report = IngestReport::new("demo");
        for i in 0..25 {
            report.errors.push(format!("err{i}"));
        }
        let v = report.as_value();
        assert_eq!(v["errors"].as_array().unwrap().len(), 20);
    }
}

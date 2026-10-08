//! The source plug-in contract. Ported from `sources/base.py`.
//!
//! Python discovers sources dynamically (`pkgutil.iter_modules` over
//! `sources/<pkg>/` looking for a module-level `SOURCE`). Rust has no
//! equivalent runtime package discovery, so `sources::registry` lists the
//! known implementations explicitly instead -- same end result (a registry of
//! `Source` trait objects), different wiring mechanism.
//!
//! `ToolSpec`/`Source::tools()` (per-source MCP/REST tool registration) is
//! intentionally not ported: there is no MCP/tool-registry machinery in this
//! codebase yet (see `vaults::tools`'s module doc for the same call). The pure
//! logic behind each source's tools (finance summaries, recording lookups) is
//! still ported, as plain functions in each source's module.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;
use serde_json::Value;
use surrealdb::types::RecordId;
use crate::rid::RecordIdExt;

use crate::db::Db;
use crate::error::AppResult;

/// Raw records fetched from an origin, plus the opaque cursor to feed back
/// into the next sync.
#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    pub records: Vec<Value>,
    pub cursor: Option<String>,
}

/// Everything a `Source` method needs to reach the database and this owner's
/// credentials, bundled so trait methods don't grow an ever-longer parameter
/// list. Mirrors the Python source modules importing `app.db.db` /
/// `sources.registry.credentials_for` directly inside each method body.
pub struct SourceCtx<'a> {
    pub db: &'a Db,
    pub encryption_key: &'a str,
    pub owner: &'a RecordId,
}

#[async_trait]
pub trait Source: Send + Sync {
    /// Stable identifier, e.g. "up_bank" or "heypocket".
    fn key(&self) -> &'static str;

    /// The credential holder this source reads. Several sources can share one
    /// connector kind. Empty string means "defer to `key()`" -- see
    /// `provider_key()`. Matches a `connector` row's `kind`.
    fn provider(&self) -> &'static str {
        ""
    }

    /// Human label for the admin UI.
    fn label(&self) -> &'static str;

    /// Envelope `type` values this source emits, e.g. `["up.transaction"]`.
    fn record_types(&self) -> &'static [&'static str];

    /// "oauth" | "api_key" | "token".
    fn auth_kind(&self) -> &'static str {
        "token"
    }

    fn provider_key(&self) -> &'static str {
        let p = self.provider();
        if p.is_empty() {
            self.key()
        } else {
            p
        }
    }

    /// Fetch raw records for `ctx.owner`. `mode` is one of "poll", "webhook",
    /// "backfill".
    async fn sync(&self, ctx: &SourceCtx<'_>, mode: &str, cursor: Option<String>) -> AppResult<SyncResult>;

    /// One raw origin value -> a cache envelope, or `None` to skip.
    fn map(&self, raw: &Value) -> Option<Value>;

    /// Verify + translate a provider push into raw dicts (or `None`). Default:
    /// no webhook support.
    async fn webhook(&self, _ctx: &SourceCtx<'_>, _headers: &HeaderMap, _body: &[u8]) -> AppResult<Option<Vec<Value>>> {
        Ok(None)
    }
}

/// The caller-facing half of a user record id, e.g. `"abc123"` out of
/// `user:abc123`. Mirrors `cache/search.py`'s `_literal` partition -- the
/// owner-id prefix baked into cache/sync-status record ids is internal, never
/// shown to or passed by a caller.
pub fn owner_key_str(owner: &RecordId) -> String {
    let s = owner.to_string();
    match s.split_once(':') {
        Some((_, rest)) => rest.to_string(),
        None => s,
    }
}

/// Converts a `surrealdb::types::Datetime` to a `chrono::DateTime<Utc>`. The public
/// `surrealdb` crate's `Datetime` wrapper only offers `From<DateTime<Utc>>`
/// (writing), not the reverse (reading) -- so this round-trips through its
/// `Serialize` impl (a newtype-struct around `chrono::DateTime<Utc>`, which
/// `serde_json` serializes as a plain RFC3339 string) instead.
pub fn datetime_to_chrono(d: &surrealdb::types::Datetime) -> Option<DateTime<Utc>> {
    let value = serde_json::to_value(d).ok()?;
    let s = value.as_str()?;
    DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_key_str_strips_table_prefix() {
        let owner: RecordId = crate::rid::parse("user:abc123").unwrap();
        assert_eq!(owner_key_str(&owner), "abc123");
    }

    #[test]
    fn datetime_to_chrono_round_trips() {
        let now = Utc::now();
        // Truncate to milliseconds like a real RFC3339 round-trip would.
        let now = DateTime::parse_from_rfc3339(&now.to_rfc3339()).unwrap().with_timezone(&Utc);
        let sd: surrealdb::types::Datetime = now.into();
        assert_eq!(datetime_to_chrono(&sd), Some(now));
    }
}

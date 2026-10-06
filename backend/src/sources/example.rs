//! Reference source. Ported from `sources/example/source.py` -- copy this
//! file, rename, and fill in `sync` + `map`. (`tools()` isn't ported here; see
//! `sources::base`'s module doc for why.)

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::sources::base::{Source, SourceCtx, SyncResult};

pub struct ExampleSource;

#[async_trait]
impl Source for ExampleSource {
    fn key(&self) -> &'static str {
        "example"
    }

    fn label(&self) -> &'static str {
        "Example (reference stub)"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["example.note"]
    }

    async fn sync(&self, _ctx: &SourceCtx<'_>, _mode: &str, cursor: Option<String>) -> AppResult<SyncResult> {
        // A real source hits its API here, using `cursor` for delta pulls.
        Ok(SyncResult { records: vec![], cursor })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id")?;
        if id.is_null() {
            return None;
        }
        let id_str = match id {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let mut raw_keys: Vec<String> = raw.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
        raw_keys.sort();

        Some(json!({
            "id": format!("{}:example.note:{}", self.key(), id_str),
            "source": self.key(),
            "type": "example.note",
            "external_id": id_str,
            "title": raw.get("title").and_then(|v| v.as_str()).unwrap_or(""),
            "body_text": raw.get("text").and_then(|v| v.as_str()).unwrap_or(""),
            "occurred_at": raw.get("created_at").cloned().unwrap_or_else(|| json!(Utc::now().to_rfc3339())),
            "url": raw.get("url").and_then(|v| v.as_str()).unwrap_or(""),
            "payload": { "raw_keys": raw_keys },
            "links": [],
            "deleted": raw.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_skips_records_without_an_id() {
        let src = ExampleSource;
        assert!(src.map(&json!({"title": "no id"})).is_none());
    }

    #[test]
    fn map_builds_the_envelope_id_and_sorted_raw_keys() {
        let src = ExampleSource;
        let env = src.map(&json!({"id": 7, "title": "T", "text": "body", "zeta": 1, "alpha": 2})).unwrap();
        assert_eq!(env["id"], "example:example.note:7");
        assert_eq!(env["external_id"], "7");
        assert_eq!(env["source"], "example");
        assert_eq!(env["payload"]["raw_keys"], json!(["alpha", "id", "text", "title", "zeta"]));
    }

    #[test]
    fn map_defaults_deleted_to_false() {
        let src = ExampleSource;
        let env = src.map(&json!({"id": "x"})).unwrap();
        assert_eq!(env["deleted"], false);
    }
}

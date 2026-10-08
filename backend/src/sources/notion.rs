//! Notion source: pages/databases shared with the integration, via
//! `POST /v1/search`. Real API shape; not exercised against a live
//! integration in this environment -- see `connectors::clients::NotionClient`.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};

use crate::connectors::clients::NotionClient;
use crate::error::AppResult;
use crate::sources::base::{Source, SourceCtx, SyncResult};
use crate::sources::registry::credentials_for;

pub struct NotionSource;

/// Notion's page title lives at `properties.<some key>.title[*].plain_text`
/// -- the property holding it isn't always literally named "title" (it can
/// be renamed per-database), so this scans every property for the first
/// one shaped like a title.
fn extract_title(raw: &Value) -> String {
    let Some(props) = raw.get("properties").and_then(|v| v.as_object()) else { return String::new() };
    for prop in props.values() {
        if prop.get("type").and_then(|v| v.as_str()) == Some("title")
            && let Some(parts) = prop.get("title").and_then(|v| v.as_array()) {
                return parts.iter().filter_map(|p| p.get("plain_text").and_then(|v| v.as_str())).collect::<String>();
            }
    }
    String::new()
}

#[async_trait]
impl Source for NotionSource {
    fn key(&self) -> &'static str {
        "notion"
    }

    fn label(&self) -> &'static str {
        "Notion"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["notion.page"]
    }

    fn auth_kind(&self) -> &'static str {
        "token"
    }

    async fn sync(&self, ctx: &SourceCtx<'_>, _mode: &str, _cursor: Option<String>) -> AppResult<SyncResult> {
        let creds = credentials_for(ctx.db, ctx.encryption_key, ctx.owner, self).await?;
        let client = NotionClient::new(&creds);
        let resp = client.search().await?;
        let records = resp.get("results").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        Ok(SyncResult { records, cursor: Some(Utc::now().to_rfc3339()) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id").and_then(|v| v.as_str())?;
        let title = extract_title(raw);
        let object = raw.get("object").and_then(|v| v.as_str()).unwrap_or("page");
        Some(json!({
            "id": format!("notion:notion.page:{id}"),
            "source": "notion",
            "type": "notion.page",
            "external_id": id,
            "title": title,
            "body_text": title,
            "occurred_at": raw.get("last_edited_time"),
            "url": raw.get("url").cloned().unwrap_or(Value::String(String::new())),
            "payload": {
                "object": object,
                "archived": raw.get("archived"),
                "created_time": raw.get("created_time"),
            },
            "links": [],
            "deleted": raw.get("archived").and_then(|v| v.as_bool()).unwrap_or(false),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_title_finds_the_title_property_regardless_of_name() {
        let raw = json!({
            "properties": {
                "Tags": {"type": "multi_select", "multi_select": []},
                "Name": {"type": "title", "title": [{"plain_text": "Roadmap"}, {"plain_text": " Q1"}]},
            }
        });
        assert_eq!(extract_title(&raw), "Roadmap Q1");
    }

    #[test]
    fn map_marks_archived_pages_deleted() {
        let src = NotionSource;
        let raw = json!({"id": "p1", "object": "page", "archived": true, "url": "https://notion.so/p1", "properties": {}});
        let env = src.map(&raw).unwrap();
        assert_eq!(env["deleted"], true);
        assert_eq!(env["id"], "notion:notion.page:p1");
    }
}

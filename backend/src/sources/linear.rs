//! Linear source: issues assigned to the API key's owner, via a GraphQL
//! query. Real API shape; not exercised against a live workspace in this
//! environment -- see `connectors::clients::LinearClient`.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};

use crate::connectors::clients::LinearClient;
use crate::error::AppResult;
use crate::sources::base::{Source, SourceCtx, SyncResult};
use crate::sources::registry::credentials_for;

pub struct LinearSource;

#[async_trait]
impl Source for LinearSource {
    fn key(&self) -> &'static str {
        "linear"
    }

    fn label(&self) -> &'static str {
        "Linear"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["linear.issue"]
    }

    fn auth_kind(&self) -> &'static str {
        "api_key"
    }

    async fn sync(&self, ctx: &SourceCtx<'_>, _mode: &str, _cursor: Option<String>) -> AppResult<SyncResult> {
        let creds = credentials_for(ctx.db, ctx.encryption_key, ctx.owner, self).await?;
        let client = LinearClient::new(&creds);
        let resp = client.assigned_issues().await?;
        let records = resp
            .pointer("/data/viewer/assignedIssues/nodes")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(SyncResult { records, cursor: Some(Utc::now().to_rfc3339()) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id").and_then(|v| v.as_str())?;
        let identifier = raw.get("identifier").and_then(|v| v.as_str()).unwrap_or("");
        let title = raw.get("title").and_then(|v| v.as_str()).unwrap_or("");
        Some(json!({
            "id": format!("linear:linear.issue:{id}"),
            "source": "linear",
            "type": "linear.issue",
            "external_id": id,
            "title": format!("{identifier}: {title}"),
            "body_text": raw.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            "occurred_at": raw.get("updatedAt"),
            "url": raw.get("url").cloned().unwrap_or(Value::String(String::new())),
            "payload": {
                "identifier": identifier,
                "state": raw.pointer("/state/name"),
                "created_at": raw.get("createdAt"),
            },
            "links": [],
            "deleted": false,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_prefixes_title_with_issue_identifier() {
        let src = LinearSource;
        let raw = json!({
            "id": "abc", "identifier": "ENG-42", "title": "Fix flaky test",
            "description": "It flakes on CI", "url": "https://linear.app/acme/issue/ENG-42",
            "state": {"name": "In Progress"}, "updatedAt": "2024-01-01T00:00:00Z", "createdAt": "2023-12-01T00:00:00Z",
        });
        let env = src.map(&raw).unwrap();
        assert_eq!(env["title"], "ENG-42: Fix flaky test");
        assert_eq!(env["payload"]["state"], "In Progress");
    }
}

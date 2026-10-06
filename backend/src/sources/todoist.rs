//! Todoist source: active tasks, via `GET /tasks`. Real API shape; not
//! exercised against a live account in this environment -- see
//! `connectors::clients::TodoistClient`.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};

use crate::connectors::clients::TodoistClient;
use crate::error::AppResult;
use crate::sources::base::{Source, SourceCtx, SyncResult};
use crate::sources::registry::credentials_for;

pub struct TodoistSource;

#[async_trait]
impl Source for TodoistSource {
    fn key(&self) -> &'static str {
        "todoist"
    }

    fn label(&self) -> &'static str {
        "Todoist"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["todoist.task"]
    }

    fn auth_kind(&self) -> &'static str {
        "token"
    }

    async fn sync(&self, ctx: &SourceCtx<'_>, _mode: &str, _cursor: Option<String>) -> AppResult<SyncResult> {
        let creds = credentials_for(ctx.db, ctx.encryption_key, ctx.owner, self).await?;
        let client = TodoistClient::new(&creds);
        let resp = client.tasks().await?;
        let records = resp.as_array().cloned().unwrap_or_default();
        Ok(SyncResult { records, cursor: Some(Utc::now().to_rfc3339()) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id").and_then(|v| v.as_str())?;
        let content = raw.get("content").and_then(|v| v.as_str()).unwrap_or("");
        Some(json!({
            "id": format!("todoist:todoist.task:{id}"),
            "source": "todoist",
            "type": "todoist.task",
            "external_id": id,
            "title": content,
            "body_text": raw.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            "occurred_at": raw.pointer("/due/date"),
            "url": raw.get("url").cloned().unwrap_or(Value::String(String::new())),
            "payload": {
                "priority": raw.get("priority"),
                "is_completed": raw.get("is_completed"),
                "project_id": raw.get("project_id"),
            },
            "links": [],
            "deleted": raw.get("is_completed").and_then(|v| v.as_bool()).unwrap_or(false),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_marks_completed_tasks_deleted() {
        let src = TodoistSource;
        let raw = json!({"id": "1", "content": "Ship it", "priority": 4, "is_completed": true, "due": {"date": "2024-01-01"}});
        let env = src.map(&raw).unwrap();
        assert_eq!(env["title"], "Ship it");
        assert_eq!(env["deleted"], true);
        assert_eq!(env["occurred_at"], "2024-01-01");
    }
}

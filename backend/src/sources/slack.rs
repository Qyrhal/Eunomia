//! Slack source: channels/conversations the bot is a member of, via
//! `conversations.list`. Real API shape; not exercised against a live
//! workspace in this environment -- see `connectors::clients::SlackClient`.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};

use crate::connectors::clients::SlackClient;
use crate::error::AppResult;
use crate::sources::base::{Source, SourceCtx, SyncResult};
use crate::sources::registry::credentials_for;

pub struct SlackSource;

#[async_trait]
impl Source for SlackSource {
    fn key(&self) -> &'static str {
        "slack"
    }

    fn label(&self) -> &'static str {
        "Slack"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["slack.channel"]
    }

    fn auth_kind(&self) -> &'static str {
        "token"
    }

    async fn sync(&self, ctx: &SourceCtx<'_>, _mode: &str, _cursor: Option<String>) -> AppResult<SyncResult> {
        let creds = credentials_for(ctx.db, ctx.encryption_key, ctx.owner, self).await?;
        let client = SlackClient::new(&creds);
        let resp = client.conversations().await?;
        let records = resp.get("channels").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        Ok(SyncResult { records, cursor: Some(Utc::now().to_rfc3339()) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id").and_then(|v| v.as_str())?;
        let name = raw.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let topic = raw.pointer("/topic/value").and_then(|v| v.as_str()).unwrap_or("");
        Some(json!({
            "id": format!("slack:slack.channel:{id}"),
            "source": "slack",
            "type": "slack.channel",
            "external_id": id,
            "title": format!("#{name}"),
            "body_text": topic,
            "occurred_at": Value::Null,
            "url": format!("slack://channel?id={id}"),
            "payload": {
                "is_private": raw.get("is_private"),
                "num_members": raw.get("num_members"),
                "is_archived": raw.get("is_archived"),
            },
            "links": [],
            "deleted": raw.get("is_archived").and_then(|v| v.as_bool()).unwrap_or(false),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_prefixes_channel_name_with_hash() {
        let src = SlackSource;
        let raw = json!({"id": "C123", "name": "general", "is_private": false, "num_members": 12, "topic": {"value": "chat"}});
        let env = src.map(&raw).unwrap();
        assert_eq!(env["title"], "#general");
        assert_eq!(env["body_text"], "chat");
    }

    #[test]
    fn map_marks_archived_channels_deleted() {
        let src = SlackSource;
        let env = src.map(&json!({"id": "C1", "name": "old", "is_archived": true})).unwrap();
        assert_eq!(env["deleted"], true);
    }
}

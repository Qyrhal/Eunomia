//! Discord source: recent messages in one configured channel, via
//! `GET /channels/{id}/messages`. Needs `channel_id` set in the connector's
//! `config` (a bot has no "all channels" scope by default). Real API shape;
//! not exercised against a live bot in this environment -- see
//! `connectors::clients::DiscordClient`.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};

use crate::connectors::clients::DiscordClient;
use crate::error::AppResult;
use crate::sources::base::{Source, SourceCtx, SyncResult};
use crate::sources::registry::{connector_for, credentials_for};

pub struct DiscordSource;

#[async_trait]
impl Source for DiscordSource {
    fn key(&self) -> &'static str {
        "discord"
    }

    fn label(&self) -> &'static str {
        "Discord"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["discord.message"]
    }

    fn auth_kind(&self) -> &'static str {
        "token"
    }

    async fn sync(&self, ctx: &SourceCtx<'_>, _mode: &str, _cursor: Option<String>) -> AppResult<SyncResult> {
        let creds = credentials_for(ctx.db, ctx.encryption_key, ctx.owner, self).await?;
        let config = connector_for(ctx.db, ctx.owner, self).await?.map(|c| c.config).unwrap_or(json!({}));
        let client = DiscordClient::new(&creds, &config);
        let resp = client.messages().await?;
        let records = resp.as_array().cloned().unwrap_or_default();
        Ok(SyncResult { records, cursor: Some(Utc::now().to_rfc3339()) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id").and_then(|v| v.as_str())?;
        let content = raw.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let author = raw.pointer("/author/username").and_then(|v| v.as_str()).unwrap_or("unknown");
        Some(json!({
            "id": format!("discord:discord.message:{id}"),
            "source": "discord",
            "type": "discord.message",
            "external_id": id,
            "title": format!("{author}: {content}").chars().take(80).collect::<String>(),
            "body_text": content,
            "occurred_at": raw.get("timestamp"),
            "url": "",
            "payload": {
                "author": author,
                "channel_id": raw.get("channel_id"),
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
    fn map_truncates_title_to_80_chars() {
        let src = DiscordSource;
        let long = "x".repeat(200);
        let raw = json!({"id": "1", "content": long, "author": {"username": "bob"}, "timestamp": "2024-01-01T00:00:00Z"});
        let env = src.map(&raw).unwrap();
        assert!(env["title"].as_str().unwrap().chars().count() <= 80);
        assert_eq!(env["payload"]["author"], "bob");
    }
}

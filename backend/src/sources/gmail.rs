//! Gmail source: recent messages, via `GET .../messages` (list, ids only)
//! then `GET .../messages/{id}` (metadata) per id -- Gmail's list endpoint
//! never includes subject/snippet, only ids. Real API shape; not exercised
//! against a live account in this environment -- see
//! `connectors::clients::GmailClient`.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};

use crate::connectors::clients::GmailClient;
use crate::error::AppResult;
use crate::sources::base::{Source, SourceCtx, SyncResult};
use crate::sources::registry::credentials_for;

/// Gmail message list pages can be large; cap the per-message metadata
/// fetch (an extra round-trip each) to keep one sync tick bounded.
const MAX_MESSAGES_PER_SYNC: usize = 25;

fn header(headers: &[Value], name: &str) -> String {
    headers
        .iter()
        .find(|h| h.get("name").and_then(|v| v.as_str()).map(|n| n.eq_ignore_ascii_case(name)).unwrap_or(false))
        .and_then(|h| h.get("value").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string()
}

pub struct GmailSource;

#[async_trait]
impl Source for GmailSource {
    fn key(&self) -> &'static str {
        "gmail"
    }

    fn label(&self) -> &'static str {
        "Gmail"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["gmail.message"]
    }

    fn auth_kind(&self) -> &'static str {
        "oauth"
    }

    async fn sync(&self, ctx: &SourceCtx<'_>, _mode: &str, _cursor: Option<String>) -> AppResult<SyncResult> {
        let creds = credentials_for(ctx.db, ctx.encryption_key, ctx.owner, self).await?;
        let client = GmailClient::new(&creds);
        let list = client.messages(None).await?;
        let ids: Vec<String> = list
            .get("messages")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from)).collect())
            .unwrap_or_default();

        let mut records = Vec::new();
        for id in ids.into_iter().take(MAX_MESSAGES_PER_SYNC) {
            if let Ok(meta) = client.message(&id).await {
                records.push(meta);
            }
        }
        Ok(SyncResult { records, cursor: Some(Utc::now().to_rfc3339()) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id").and_then(|v| v.as_str())?;
        let headers = raw.pointer("/payload/headers").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let subject = header(&headers, "Subject");
        let from = header(&headers, "From");
        let date = header(&headers, "Date");
        let snippet = raw.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
        Some(json!({
            "id": format!("gmail:gmail.message:{id}"),
            "source": "gmail",
            "type": "gmail.message",
            "external_id": id,
            "title": subject,
            "body_text": snippet,
            "occurred_at": if date.is_empty() { Value::Null } else { Value::String(date) },
            "url": format!("https://mail.google.com/mail/u/0/#inbox/{id}"),
            "payload": {
                "from": from,
                "thread_id": raw.get("threadId"),
                "label_ids": raw.get("labelIds"),
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
    fn header_is_case_insensitive() {
        let headers = vec![json!({"name": "subject", "value": "Hello"})];
        assert_eq!(header(&headers, "Subject"), "Hello");
    }

    #[test]
    fn map_pulls_subject_from_from_and_snippet() {
        let src = GmailSource;
        let raw = json!({
            "id": "m1", "threadId": "t1", "snippet": "preview text", "labelIds": ["INBOX"],
            "payload": {"headers": [
                {"name": "Subject", "value": "Hi there"},
                {"name": "From", "value": "a@b.com"},
                {"name": "Date", "value": "Mon, 1 Jan 2024 00:00:00 +0000"},
            ]},
        });
        let env = src.map(&raw).unwrap();
        assert_eq!(env["title"], "Hi there");
        assert_eq!(env["payload"]["from"], "a@b.com");
        assert_eq!(env["body_text"], "preview text");
    }
}

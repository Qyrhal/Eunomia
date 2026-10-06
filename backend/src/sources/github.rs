//! GitHub source: notifications across every repo this token can see, via
//! `GET /notifications`. Real API shape; not exercised against a live token
//! in this environment -- see `connectors::clients::GitHubClient`.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};

use crate::connectors::clients::GitHubClient;
use crate::error::AppResult;
use crate::sources::base::{Source, SourceCtx, SyncResult};
use crate::sources::registry::credentials_for;

pub struct GitHubSource;

#[async_trait]
impl Source for GitHubSource {
    fn key(&self) -> &'static str {
        "github"
    }

    fn label(&self) -> &'static str {
        "GitHub"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["github.notification"]
    }

    fn auth_kind(&self) -> &'static str {
        "token"
    }

    async fn sync(&self, ctx: &SourceCtx<'_>, _mode: &str, cursor: Option<String>) -> AppResult<SyncResult> {
        let creds = credentials_for(ctx.db, ctx.encryption_key, ctx.owner, self).await?;
        let client = GitHubClient::new(&creds);
        let resp = client.notifications(cursor.as_deref()).await?;
        let records = resp.as_array().cloned().unwrap_or_default();
        Ok(SyncResult { records, cursor: Some(Utc::now().to_rfc3339()) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id").and_then(|v| v.as_str())?;
        let subject = &raw["subject"];
        let title = subject.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let repo = raw.pointer("/repository/full_name").and_then(|v| v.as_str()).unwrap_or("");
        let url = subject
            .get("url")
            .and_then(|v| v.as_str())
            .map(|api_url| api_url.replace("api.github.com/repos", "github.com"))
            .unwrap_or_default();
        Some(json!({
            "id": format!("github:github.notification:{id}"),
            "source": "github",
            "type": "github.notification",
            "external_id": id,
            "title": title,
            "body_text": format!("{title} ({repo})"),
            "occurred_at": raw.get("updated_at"),
            "url": url,
            "payload": {
                "reason": raw.get("reason"),
                "unread": raw.get("unread"),
                "subject_type": subject.get("type"),
                "repository": repo,
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
    fn map_builds_github_url_from_api_url() {
        let src = GitHubSource;
        let raw = json!({
            "id": "1", "updated_at": "2024-01-01T00:00:00Z", "reason": "mention", "unread": true,
            "subject": {"title": "Fix bug", "type": "Issue", "url": "https://api.github.com/repos/acme/widget/issues/5"},
            "repository": {"full_name": "acme/widget"},
        });
        let env = src.map(&raw).unwrap();
        assert_eq!(env["id"], "github:github.notification:1");
        assert_eq!(env["url"], "https://github.com/acme/widget/issues/5");
        assert_eq!(env["body_text"], "Fix bug (acme/widget)");
    }

    #[test]
    fn map_returns_none_without_id() {
        let src = GitHubSource;
        assert!(src.map(&json!({"subject": {}})).is_none());
    }
}

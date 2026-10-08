//! Gmail source: messages from the last 30 days, then everything newer than
//! the last one seen, via `users.messages.list` (ids only, page-token
//! paginated) + `users.messages.get?format=full` per message for headers and
//! the plain-text body.
//!
//! Google has no personal access tokens, so the user brings their own OAuth
//! client: `client_id` + `client_secret` + a `refresh_token` (scope
//! `gmail.readonly`, e.g. minted in the OAuth Playground -- see
//! docs/connectors.md). Each sync exchanges the refresh token for a fresh
//! access token ([`google_api`], shared with Google Calendar).

use async_trait::async_trait;
use base64::Engine;
use serde_json::{json, Value};

use crate::connectors::clients::{bearer, refresh_access_token, Api};
use crate::error::AppResult;
use crate::sources::base::{envelope, from_unix, items, s, Conn, Source, SyncResult, MAX_PAGES};

const BASE_URL: &str = "https://gmail.googleapis.com/gmail/v1";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// Each message costs one extra request; bound one sync.
const MAX_MESSAGES_PER_SYNC: usize = 500;
const MAX_BODY_CHARS: usize = 20_000;

/// A Google API authenticated with a freshly refreshed access token.
pub(crate) async fn google_api(conn: &Conn, base: &str) -> AppResult<Api> {
    let token = refresh_access_token(&conn.config, GOOGLE_TOKEN_URL, &conn.credentials).await?;
    Ok(Api::new(&conn.config, base, bearer(&token)))
}

fn header(raw: &Value, name: &str) -> String {
    items(raw, "/payload/headers")
        .iter()
        .find(|h| s(h, "/name").eq_ignore_ascii_case(name))
        .map(|h| s(h, "/value").to_string())
        .unwrap_or_default()
}

/// The first `text/plain` part anywhere in the MIME tree, decoded.
fn plain_text(part: &Value) -> Option<String> {
    if s(part, "/mimeType") == "text/plain" {
        let data = s(part, "/body/data").trim_end_matches('=');
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(data).ok()?;
        return Some(String::from_utf8_lossy(&bytes).to_string());
    }
    items(part, "/parts").iter().find_map(plain_text)
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

    async fn check(&self, conn: &Conn) -> AppResult<()> {
        google_api(conn, BASE_URL).await?.get("/users/me/profile", &[]).await.map(|_| ())
    }

    async fn fetch(&self, conn: &Conn, cursor: Option<String>) -> AppResult<SyncResult> {
        let api = google_api(conn, BASE_URL).await?;
        let q = cursor.as_deref().map(|c| format!("after:{c}")).unwrap_or_else(|| "newer_than:30d".to_string());

        let mut ids = Vec::new();
        let mut page_token = String::new();
        for _ in 0..MAX_PAGES {
            let mut query = vec![("q", q.clone()), ("maxResults", "100".to_string())];
            if !page_token.is_empty() {
                query.push(("pageToken", page_token.clone()));
            }
            let page = api.get("/users/me/messages", &query).await?;
            ids.extend(items(&page, "/messages").iter().map(|m| s(m, "/id").to_string()));
            page_token = s(&page, "/nextPageToken").to_string();
            if page_token.is_empty() || ids.len() >= MAX_MESSAGES_PER_SYNC {
                break;
            }
        }

        let mut records = Vec::new();
        for id in ids.iter().take(MAX_MESSAGES_PER_SYNC) {
            records.push(api.get(&format!("/users/me/messages/{id}"), &[("format", "full".to_string())]).await?);
        }
        let newest = records.iter().filter_map(|r| s(r, "/internalDate").parse::<i64>().ok()).max().map(|ms| (ms / 1000).to_string());
        Ok(SyncResult { records, cursor: newest.or(cursor) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id").and_then(|v| v.as_str())?;
        let subject = header(raw, "Subject");
        let from = header(raw, "From");
        let body: String = plain_text(&raw["payload"]).unwrap_or_else(|| s(raw, "/snippet").to_string()).chars().take(MAX_BODY_CHARS).collect();
        let millis = s(raw, "/internalDate").parse::<i64>().unwrap_or(0);
        Some(envelope(
            "gmail",
            "gmail.message",
            id,
            if subject.is_empty() { "(no subject)" } else { &subject },
            &format!("From: {from}\n{}", body.trim()),
            from_unix(millis / 1000),
            &format!("https://mail.google.com/mail/u/0/#all/{}", s(raw, "/threadId")),
            json!({
                "from": from,
                "to": header(raw, "To"),
                "thread_id": raw.get("threadId"),
                "label_ids": raw.get("labelIds"),
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::mock::{assert_fetch_fails, envelopes, route, serve};

    fn b64(s: &str) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s)
    }

    fn message(id: &str, internal: &str, subject: &str) -> Value {
        json!({
            "id": id, "threadId": format!("t-{id}"), "labelIds": ["INBOX", "UNREAD"], "snippet": "snippet only",
            "internalDate": internal,
            "payload": {
                "mimeType": "multipart/alternative",
                "headers": [{"name": "Subject", "value": subject}, {"name": "From", "value": "Ada <ada@example.com>"},
                            {"name": "To", "value": "me@example.com"}],
                "parts": [
                    {"mimeType": "text/plain", "body": {"size": 30, "data": b64("Lunch on Thursday works for me.")}},
                    {"mimeType": "text/html", "body": {"size": 40, "data": b64("<p>Lunch on Thursday works for me.</p>")}},
                ],
            },
        })
    }

    #[tokio::test]
    async fn fetch_refreshes_the_token_then_lists_and_reads_messages() {
        let mock = serve(vec![
            route("POST", "/oauth/token", json!({"access_token": "ya29.fresh", "expires_in": 3599, "token_type": "Bearer"})),
            route("GET", "/users/me/messages", json!({"messages": [{"id": "m2", "threadId": "t-m2"}], "resultSizeEstimate": 1}))
                .query("pageToken=p2"),
            route("GET", "/users/me/messages", json!({"messages": [{"id": "m1", "threadId": "t-m1"}], "nextPageToken": "p2"}))
                .query("q=after:1700000000"),
            route("GET", "/users/me/messages/m1", json!(message("m1", "1700000100000", "Lunch?"))).query("format=full"),
            route("GET", "/users/me/messages/m2", json!(message("m2", "1700000200000", "Re: Lunch?"))),
        ])
        .await;

        let creds = json!({"client_id": "cid", "client_secret": "csecret", "refresh_token": "1//rt"});
        let res = GmailSource.fetch(&mock.conn(creds), Some("1700000000".into())).await.unwrap();
        assert_eq!(res.records.len(), 2);
        assert_eq!(res.cursor.as_deref(), Some("1700000200"));

        let reqs = mock.requests();
        let token = &reqs[0];
        assert_eq!(token.path, "/oauth/token");
        assert!(token.body.contains("grant_type=refresh_token") && token.body.contains("refresh_token=1%2F%2Frt"));
        assert!(token.header("authorization").starts_with("Basic "));
        assert!(reqs[1..].iter().all(|r| r.header("authorization") == "Bearer ya29.fresh"));

        let envs = envelopes(&GmailSource, &res.records);
        assert_eq!(envs[0].title, "Lunch?");
        assert_eq!(envs[0].body_text, "From: Ada <ada@example.com>\nLunch on Thursday works for me.");
        assert!(envs[0].occurred_at.is_some());
    }

    #[tokio::test]
    async fn a_rejected_refresh_token_is_a_visible_error() {
        let mock = serve(vec![route("POST", "/oauth/token", json!({"error": "invalid_grant", "error_description": "Token has been expired or revoked."})).status(400)]).await;
        let creds = json!({"client_id": "cid", "client_secret": "s", "refresh_token": "old"});
        let err = GmailSource.fetch(&mock.conn(creds), None).await.unwrap_err();
        assert!(err.message.contains("HTTP 400") && err.message.contains("invalid_grant"), "{}", err.message);
    }

    #[tokio::test]
    async fn fetch_errors_are_visible() {
        let creds = json!({"client_id": "cid", "client_secret": "s", "refresh_token": "rt"});
        assert_fetch_fails(&GmailSource, 401, "/users/me/messages", creds.clone(), "HTTP 401").await;
        assert_fetch_fails(&GmailSource, 429, "/users/me/messages", creds.clone(), "HTTP 429").await;
        assert_fetch_fails(&GmailSource, 500, "/users/me/messages", creds, "HTTP 500").await;
        assert_fetch_fails(&GmailSource, 200, "/x", json!({"client_id": "cid"}), "missing client_secret").await;
    }
}

//! Google Calendar source: events on the primary calendar, via
//! `events.list` (page-token paginated, recurring events expanded). Same
//! bring-your-own-OAuth-client credentials as Gmail (scope
//! `calendar.readonly`), refreshed each sync.
//!
//! First sync: 30 days back to 180 days ahead. After that: every event
//! modified since the previous sync started (`updatedMin`), including
//! cancellations, which mark the cached event deleted.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::sources::base::{envelope, items, rfc3339, s, Conn, Source, SyncResult, MAX_PAGES};
use crate::sources::gmail::google_api;

const BASE_URL: &str = "https://www.googleapis.com/calendar/v3";

/// `start`/`end` carry `dateTime` for timed events, `date` for all-day ones.
fn when(v: &Value) -> Option<String> {
    let dt = s(v, "/dateTime");
    if !dt.is_empty() {
        return rfc3339(dt);
    }
    let d = s(v, "/date");
    (!d.is_empty()).then(|| rfc3339(&format!("{d}T00:00:00Z"))).flatten()
}

pub struct GoogleCalendarSource;

#[async_trait]
impl Source for GoogleCalendarSource {
    fn key(&self) -> &'static str {
        "google_calendar"
    }

    fn label(&self) -> &'static str {
        "Google Calendar"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["gcal.event"]
    }

    async fn check(&self, conn: &Conn) -> AppResult<()> {
        google_api(conn, BASE_URL).await?.get("/calendars/primary", &[]).await.map(|_| ())
    }

    async fn fetch(&self, conn: &Conn, cursor: Option<String>) -> AppResult<SyncResult> {
        let api = google_api(conn, BASE_URL).await?;
        let started = Utc::now();
        let mut base = vec![("singleEvents", "true".to_string()), ("maxResults", "250".to_string())];
        match &cursor {
            Some(c) => {
                base.push(("updatedMin", c.clone()));
                base.push(("showDeleted", "true".to_string()));
            }
            None => {
                base.push(("timeMin", (started - Duration::days(30)).to_rfc3339()));
                base.push(("timeMax", (started + Duration::days(180)).to_rfc3339()));
            }
        }

        let mut records = Vec::new();
        let mut page_token = String::new();
        for _ in 0..MAX_PAGES {
            let mut query = base.clone();
            if !page_token.is_empty() {
                query.push(("pageToken", page_token.clone()));
            }
            let page = api.get("/calendars/primary/events", &query).await?;
            records.extend(items(&page, "/items"));
            page_token = s(&page, "/nextPageToken").to_string();
            if page_token.is_empty() {
                break;
            }
        }
        Ok(SyncResult { records, cursor: Some(started.to_rfc3339()) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id").and_then(|v| v.as_str())?;
        let title = s(raw, "/summary");
        let attendees: Vec<String> = items(raw, "/attendees").iter().map(|a| s(a, "/email").to_string()).collect();
        let body = [title, s(raw, "/description"), s(raw, "/location")]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let mut env = envelope(
            "google_calendar",
            "gcal.event",
            id,
            if title.is_empty() { "(no title)" } else { title },
            &body,
            when(&raw["start"]),
            s(raw, "/htmlLink"),
            json!({
                "start": when(&raw["start"]),
                "end": when(&raw["end"]),
                "all_day": raw.pointer("/start/date").is_some(),
                "location": raw.get("location"),
                "organizer": raw.pointer("/organizer/email"),
                "attendees": attendees,
                "status": raw.get("status"),
            }),
        );
        env["deleted"] = json!(s(raw, "/status") == "cancelled");
        Some(env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::mock::{assert_fetch_fails, envelopes, route, serve};

    #[tokio::test]
    async fn fetch_uses_updated_min_after_the_first_sync_and_follows_page_tokens() {
        let mock = serve(vec![
            route("POST", "/oauth/token", json!({"access_token": "ya29.cal"})),
            route("GET", "/calendars/primary/events", json!({"kind": "calendar#events", "items": [
                {"id": "e2", "status": "cancelled", "summary": "Old sync", "start": {"date": "2024-03-05"}, "end": {"date": "2024-03-06"}},
            ]}))
            .query("pageToken=n2"),
            route("GET", "/calendars/primary/events", json!({"kind": "calendar#events", "nextPageToken": "n2", "items": [
                {"id": "e1", "status": "confirmed", "summary": "Design review", "description": "Walk through the sync UI",
                 "location": "Room 4", "htmlLink": "https://www.google.com/calendar/event?eid=e1",
                 "start": {"dateTime": "2024-03-04T10:00:00+11:00"}, "end": {"dateTime": "2024-03-04T11:00:00+11:00"},
                 "organizer": {"email": "ada@example.com"}, "attendees": [{"email": "ada@example.com"}, {"email": "me@example.com"}]},
            ]}))
            .query("updatedMin=2024-03-01T00:00:00Z")
            .query("showDeleted=true"),
        ])
        .await;

        let creds = json!({"client_id": "cid", "client_secret": "s", "refresh_token": "rt"});
        let res = GoogleCalendarSource.fetch(&mock.conn(creds), Some("2024-03-01T00:00:00Z".into())).await.unwrap();
        assert_eq!(res.records.len(), 2);
        assert!(mock.requests()[1..].iter().all(|r| r.header("authorization") == "Bearer ya29.cal"));

        let envs = envelopes(&GoogleCalendarSource, &res.records);
        assert_eq!(envs[0].title, "Design review");
        assert_eq!(envs[0].body_text, "Design review\nWalk through the sync UI\nRoom 4");
        assert_eq!(envs[0].payload["attendees"], json!(["ada@example.com", "me@example.com"]));
        assert!(envs[1].deleted, "cancelled events are deletions");
        assert!(envs[1].occurred_at.is_some(), "all-day dates become midnight UTC");
    }

    #[tokio::test]
    async fn fetch_errors_are_visible() {
        let creds = json!({"client_id": "cid", "client_secret": "s", "refresh_token": "rt"});
        assert_fetch_fails(&GoogleCalendarSource, 403, "/calendars/primary/events", creds.clone(), "HTTP 403").await;
        assert_fetch_fails(&GoogleCalendarSource, 429, "/calendars/primary/events", creds.clone(), "HTTP 429").await;
        assert_fetch_fails(&GoogleCalendarSource, 500, "/calendars/primary/events", creds, "HTTP 500").await;
    }
}

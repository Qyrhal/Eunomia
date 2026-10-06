//! Google Calendar source: upcoming/recent events on the primary calendar,
//! via `GET .../calendars/primary/events`. Real API shape; not exercised
//! against a live account in this environment -- see
//! `connectors::clients::GoogleCalendarClient`.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use serde_json::{json, Value};

use crate::connectors::clients::GoogleCalendarClient;
use crate::error::AppResult;
use crate::sources::base::{Source, SourceCtx, SyncResult};
use crate::sources::registry::credentials_for;

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

    fn auth_kind(&self) -> &'static str {
        "oauth"
    }

    async fn sync(&self, ctx: &SourceCtx<'_>, _mode: &str, cursor: Option<String>) -> AppResult<SyncResult> {
        let creds = credentials_for(ctx.db, ctx.encryption_key, ctx.owner, self).await?;
        let client = GoogleCalendarClient::new(&creds);
        let time_min = cursor.unwrap_or_else(|| (Utc::now() - Duration::days(7)).to_rfc3339());
        let resp = client.events(&time_min).await?;
        let records = resp.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        Ok(SyncResult { records, cursor: Some(Utc::now().to_rfc3339()) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let id = raw.get("id").and_then(|v| v.as_str())?;
        let summary = raw.get("summary").and_then(|v| v.as_str()).unwrap_or("(no title)");
        let start = raw.pointer("/start/dateTime").or_else(|| raw.pointer("/start/date")).and_then(|v| v.as_str());
        Some(json!({
            "id": format!("google_calendar:gcal.event:{id}"),
            "source": "google_calendar",
            "type": "gcal.event",
            "external_id": id,
            "title": summary,
            "body_text": raw.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            "occurred_at": start,
            "url": raw.get("htmlLink").cloned().unwrap_or(Value::String(String::new())),
            "payload": {
                "end": raw.pointer("/end/dateTime").or_else(|| raw.pointer("/end/date")),
                "location": raw.get("location"),
                "status": raw.get("status"),
                "attendees": raw.get("attendees"),
            },
            "links": [],
            "deleted": raw.get("status").and_then(|v| v.as_str()) == Some("cancelled"),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_prefers_datetime_over_all_day_date() {
        let src = GoogleCalendarSource;
        let raw = json!({
            "id": "e1", "summary": "Standup", "status": "confirmed",
            "start": {"dateTime": "2024-01-01T09:00:00Z"}, "end": {"dateTime": "2024-01-01T09:15:00Z"},
            "htmlLink": "https://calendar.google.com/event?eid=1",
        });
        let env = src.map(&raw).unwrap();
        assert_eq!(env["occurred_at"], "2024-01-01T09:00:00Z");
        assert_eq!(env["deleted"], false);
    }

    #[test]
    fn map_marks_cancelled_events_deleted() {
        let src = GoogleCalendarSource;
        let env = src.map(&json!({"id": "e2", "summary": "x", "status": "cancelled"})).unwrap();
        assert_eq!(env["deleted"], true);
    }
}

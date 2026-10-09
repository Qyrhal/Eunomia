//! heypocket source -- meeting recordings from heypocketai.com (connector
//! kind `pocketai`, API key, bearer auth). Poll + on-demand only (the public
//! API has no webhooks).
//!
//! Sync: `GET /public/recordings?start_date=` for recordings since the last
//! one seen, then `GET /public/recordings/{id}` for each one's transcript,
//! summary and notes -- the list endpoint carries only metadata.
//!
//! The tool helpers (`summary`, `list_recordings`, `search_recordings`) read
//! `cache_record` directly.

use surrealdb::types::SurrealValue;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::connectors::clients::PocketAIClient;
use crate::error::AppResult;
use crate::store;
use crate::sources::base::{datetime_to_chrono, envelope, items, rfc3339, s, Conn, Source, SourceCtx, SyncResult};

pub struct HeyPocketSource;

#[async_trait]
impl Source for HeyPocketSource {
    fn key(&self) -> &'static str {
        "heypocket"
    }

    fn provider(&self) -> &'static str {
        "pocketai"
    }

    fn label(&self) -> &'static str {
        "heypocket"
    }

    fn record_types(&self) -> &'static [&'static str] {
        &["heypocket.recording"]
    }

    async fn check(&self, conn: &Conn) -> AppResult<()> {
        PocketAIClient::new(&conn.credentials, &conn.config)?.ping().await
    }

    async fn fetch(&self, conn: &Conn, cursor: Option<String>) -> AppResult<SyncResult> {
        let client = PocketAIClient::new(&conn.credentials, &conn.config)?;
        // `start_date` is a calendar date; the cursor is the newest
        // recording's timestamp, so re-read that whole day (dedupe is free).
        let start = cursor
            .as_deref()
            .map(|c| c.chars().take(10).collect::<String>())
            .unwrap_or_else(|| (Utc::now() - Duration::days(30)).date_naive().to_string());

        let page = client.recordings(&[("start_date", start.clone()), ("limit", "200".to_string())]).await?;
        let mut data = items(&page, "/data");

        // The list endpoint has no transcript/summary -- fetch each
        // recording's detail so body_text carries real content.
        for r in data.iter_mut() {
            let rid = r.get("id").or_else(|| r.get("recording_id")).and_then(|v| v.as_str()).map(String::from);
            let Some(rid) = rid else { continue };
            let detail = client.recording(&rid).await?.get("data").cloned().unwrap_or(json!({}));
            let transcript_text = items(&detail, "/transcript/segments")
                .iter()
                .filter_map(|s| s.get("text").and_then(|t| t.as_str()))
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(obj) = r.as_object_mut() {
                obj.insert("transcript_text".to_string(), json!(transcript_text));
                obj.insert("summary".to_string(), detail.get("summary").cloned().unwrap_or(Value::Null));
                obj.insert("notes".to_string(), detail.get("notes").cloned().unwrap_or(Value::Null));
            }
        }

        let newest = data
            .iter()
            .filter_map(|r| r.get("recording_at").or_else(|| r.get("created_at")).and_then(|v| v.as_str()))
            .max()
            .map(String::from)
            .or(cursor)
            .unwrap_or(start);

        Ok(SyncResult { records: data, cursor: Some(newest) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let rid = raw.get("id").or_else(|| raw.get("recording_id")).and_then(|v| v.as_str())?.to_string();
        let tags: Vec<String> = items(raw, "/tags")
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect();

        let parts: Vec<&str> = [raw.get("summary"), raw.get("transcript_text"), raw.get("notes")]
            .into_iter()
            .filter_map(|v| v.and_then(|v| v.as_str()))
            .filter(|s| !s.is_empty())
            .collect();
        let title = s(raw, "/title");
        let body = if parts.is_empty() { title.to_string() } else { parts.join(" ") };
        let at = raw.get("recording_at").or_else(|| raw.get("created_at")).and_then(|v| v.as_str()).unwrap_or("");
        let url = raw.get("url").or_else(|| raw.get("share_url")).and_then(|v| v.as_str()).unwrap_or("");

        let mut env = envelope(
            "heypocket",
            "heypocket.recording",
            &rid,
            title,
            &body,
            rfc3339(at),
            url,
            json!({"duration_seconds": raw.get("duration").cloned().unwrap_or(json!(0)), "tags": tags}),
        );
        env["deleted"] = json!(raw.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false));
        Some(env)
    }
}

#[derive(Debug, Deserialize, SurrealValue)]
pub(crate) struct CachedRecording {
    #[serde(default)]
    #[surreal(default)]
    title: String,
    #[serde(default)]
    #[surreal(default)]
    occurred_at: Option<surrealdb::types::Datetime>,
    #[serde(default)]
    #[surreal(default)]
    url: String,
    #[serde(default)]
    #[surreal(default)]
    payload: Value,
}

async fn cached_recordings(ctx: &SourceCtx<'_>, days: i64) -> AppResult<Vec<CachedRecording>> {
    let since = Utc::now() - Duration::days(days);
    let mut res = store::app::SOURCES_HEYPOCKET_RECENT
        .on(ctx.db)
        .bind(("owner", ctx.owner.clone()))
        .bind(("since", surrealdb::types::Datetime::from(since)))
        .await?;
    Ok(res.take(0)?)
}

fn iso(dt: &Option<surrealdb::types::Datetime>) -> Option<String> {
    dt.as_ref().and_then(datetime_to_chrono).map(|d| d.to_rfc3339())
}

fn duration_minutes(payload: &Value) -> f64 {
    let secs = payload.get("duration_seconds").and_then(|v| v.as_f64()).unwrap_or(0.0);
    (secs / 60.0 * 10.0).round() / 10.0
}

fn tags_of(payload: &Value) -> Vec<String> {
    payload
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// Recording count/duration/tag breakdown over the last `days`, computed from
/// cached recordings (not a live API call) with no invented metrics.
pub(crate) fn compute_summary(recs: &[CachedRecording]) -> Value {
    let mut tag_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in recs {
        for tag in tags_of(&r.payload) {
            *tag_counts.entry(tag).or_insert(0) += 1;
        }
    }
    let mut tag_breakdown: Vec<Value> = tag_counts.into_iter().map(|(k, v)| json!({"tag": k, "count": v})).collect();
    tag_breakdown.sort_by(|a, b| b["count"].as_i64().unwrap_or(0).cmp(&a["count"].as_i64().unwrap_or(0)));

    let total_minutes: f64 = recs.iter().map(|r| r.payload.get("duration_seconds").and_then(|v| v.as_f64()).unwrap_or(0.0)).sum::<f64>() / 60.0;

    json!({
        "recordings_count": recs.len(),
        "total_duration_minutes": (total_minutes * 10.0).round() / 10.0,
        "tag_breakdown": tag_breakdown,
        "recent_recordings": recs.iter().take(10).map(|r| json!({
            "title": r.title,
            "duration_minutes": duration_minutes(&r.payload),
            "recorded_at": iso(&r.occurred_at),
            "tags": tags_of(&r.payload),
        })).collect::<Vec<_>>(),
    })
}

pub async fn summary(ctx: &SourceCtx<'_>, days: i64) -> AppResult<Value> {
    Ok(compute_summary(&cached_recordings(ctx, days).await?))
}

pub async fn list_recordings(ctx: &SourceCtx<'_>, days: i64, tag: Option<&str>, limit: i64) -> AppResult<Value> {
    let recs = cached_recordings(ctx, days).await?;
    let limit = limit.clamp(0, 200) as usize;
    let out: Vec<Value> = recs
        .iter()
        .filter(|r| tag.map(|t| tags_of(&r.payload).iter().any(|x| x == t)).unwrap_or(true))
        .take(limit)
        .map(|r| json!({
            "title": r.title,
            "duration_minutes": duration_minutes(&r.payload),
            "recorded_at": iso(&r.occurred_at),
            "tags": tags_of(&r.payload),
            "url": if r.url.is_empty() { Value::Null } else { json!(r.url) },
        }))
        .collect();
    Ok(json!(out))
}

/// A plain case-insensitive substring match over cached title/body_text.
pub async fn search_recordings(ctx: &SourceCtx<'_>, query: &str) -> AppResult<Value> {
    let needle = query.to_lowercase();
    let mut res = store::app::SOURCES_HEYPOCKET_SEARCH
        .on(ctx.db)
        .bind(("owner", ctx.owner.clone()))
        .bind(("q", needle))
        .await?;
    let recs: Vec<CachedRecording> = res.take(0)?;
    Ok(json!({
        "results": recs.iter().map(|r| json!({
            "title": r.title,
            "recorded_at": iso(&r.occurred_at),
            "tags": tags_of(&r.payload),
            "url": if r.url.is_empty() { Value::Null } else { json!(r.url) },
        })).collect::<Vec<_>>()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(title: &str, mins: f64, tags: Vec<&str>) -> CachedRecording {
        CachedRecording {
            title: title.to_string(),
            occurred_at: None,
            url: String::new(),
            payload: json!({"duration_seconds": mins * 60.0, "tags": tags}),
        }
    }

    #[test]
    fn compute_summary_tallies_tags_and_duration() {
        let recs = vec![rec("Standup", 10.0, vec!["work"]), rec("1:1", 30.0, vec!["work", "1:1"])];
        let summary = compute_summary(&recs);
        assert_eq!(summary["recordings_count"], 2);
        assert_eq!(summary["total_duration_minutes"], 40.0);
        assert_eq!(summary["tag_breakdown"][0]["tag"], "work");
        assert_eq!(summary["tag_breakdown"][0]["count"], 2);
    }

    #[test]
    fn compute_summary_empty_input_has_zeroed_fields() {
        let summary = compute_summary(&[]);
        assert_eq!(summary["recordings_count"], 0);
        assert_eq!(summary["total_duration_minutes"], 0.0);
        assert_eq!(summary["tag_breakdown"], json!([]));
    }

    use crate::sources::mock::{assert_fetch_fails, envelopes, route, serve};

    #[tokio::test]
    async fn fetch_lists_since_the_cursor_day_and_pulls_each_transcript() {
        let mock = serve(vec![
            route("GET", "/public/recordings", json!({"data": [
                {"id": "rec_1", "title": "Weekly standup", "duration": 900, "recording_at": "2024-05-02T09:00:00Z",
                 "tags": [{"id": "t1", "name": "work"}]},
            ]}))
            .query("start_date=2024-05-01"),
            route("GET", "/public/recordings/rec_1", json!({"data": {
                "id": "rec_1", "summary": "Agreed to ship the importer on Friday.",
                "transcript": {"segments": [{"speaker": "A", "text": "Morning all."}, {"speaker": "B", "text": "Importer is ready."}]},
            }})),
        ])
        .await;

        let res = HeyPocketSource
            .fetch(&mock.conn(json!({"api_key": "pk_test"})), Some("2024-05-01T08:00:00Z".into()))
            .await
            .unwrap();
        assert_eq!(res.cursor.as_deref(), Some("2024-05-02T09:00:00Z"));
        assert!(mock.requests().iter().all(|r| r.header("authorization") == "Bearer pk_test"));

        let envs = envelopes(&HeyPocketSource, &res.records);
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].title, "Weekly standup");
        assert_eq!(envs[0].body_text, "Agreed to ship the importer on Friday. Morning all. Importer is ready.");
        assert_eq!(envs[0].payload["tags"], json!(["work"]));
    }

    #[tokio::test]
    async fn fetch_errors_are_visible() {
        let creds = json!({"api_key": "bad"});
        assert_fetch_fails(&HeyPocketSource, 401, "/public/recordings", creds.clone(), "HTTP 401").await;
        assert_fetch_fails(&HeyPocketSource, 429, "/public/recordings", creds.clone(), "HTTP 429").await;
        assert_fetch_fails(&HeyPocketSource, 503, "/public/recordings", creds, "HTTP 503").await;
    }
}

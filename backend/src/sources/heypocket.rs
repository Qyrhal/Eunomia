//! heypocket source -- meeting recordings from heypocketai.com. Ported from
//! `sources/heypocket/source.py`. Wraps
//! `crate::connectors::clients::PocketAIClient`. Poll + on-demand only (the
//! public API has no webhooks), so `Source::webhook` keeps the trait default
//! (`Ok(None)`).
//!
//! The tool helpers (`summary`, `list_recordings`, `search_recordings`) read
//! from the cache (populated by the periodic sync) in Python, via
//! `cache.search`. That module isn't ported yet, so these read `cache_record`
//! directly here; `search_recordings` here is a plain case-insensitive
//! substring match rather than `cache.search`'s hybrid BM25+embedding search,
//! since the embedding half (`embeddings.service`) isn't ported either.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::connectors::clients::PocketAIClient;
use crate::error::AppResult;
use crate::store;
use crate::sources::base::{datetime_to_chrono, Source, SourceCtx, SyncResult};
use crate::sources::registry::{connector_for, credentials_for};

fn parse_dt(value: Option<&str>) -> Option<String> {
    // cache_record.occurred_at is a SurrealDB `option<datetime>` field; the
    // driver only coerces real datetime values, not arbitrary strings, so
    // every mapper normalizes to RFC3339 before building the envelope.
    let raw = value?;
    DateTime::parse_from_rfc3339(raw).ok().map(|dt| dt.with_timezone(&Utc).to_rfc3339())
}

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

    fn auth_kind(&self) -> &'static str {
        "api_key"
    }

    async fn sync(&self, ctx: &SourceCtx<'_>, _mode: &str, cursor: Option<String>) -> AppResult<SyncResult> {
        let start = cursor.clone().unwrap_or_else(|| (Utc::now() - Duration::days(30)).date_naive().to_string());
        let client = client_for(ctx, self).await?;

        let page = client.recordings(&[("start_date", start.clone()), ("limit", "200".to_string())]).await?;
        let mut data: Vec<Value> = page.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();

        // The list endpoint has no transcript/summary -- fetch each
        // recording's detail so body_text carries real content instead of
        // silently falling back to just the title.
        for r in data.iter_mut() {
            let rid = r.get("id").or_else(|| r.get("recording_id")).and_then(|v| v.as_str()).map(String::from);
            let Some(rid) = rid else { continue };
            let Ok(detail_resp) = client.recording(&rid).await else { continue };
            let detail = detail_resp.get("data").cloned().unwrap_or(json!({}));
            let segments = detail
                .pointer("/transcript/segments")
                .and_then(|s| s.as_array())
                .cloned()
                .unwrap_or_default();
            let transcript_text = segments
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
            .unwrap_or_else(|| cursor.unwrap_or(start));

        Ok(SyncResult { records: data, cursor: Some(newest) })
    }

    fn map(&self, raw: &Value) -> Option<Value> {
        let rid = raw.get("id").or_else(|| raw.get("recording_id")).and_then(|v| v.as_str())?.to_string();
        let tags: Vec<String> = raw
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|arr| arr.iter().filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from)).collect())
            .unwrap_or_default();

        let parts: Vec<&str> = [raw.get("summary"), raw.get("transcript_text"), raw.get("notes")]
            .into_iter()
            .filter_map(|v| v.and_then(|v| v.as_str()))
            .filter(|s| !s.is_empty())
            .collect();
        let body = if parts.is_empty() {
            raw.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string()
        } else {
            parts.join(" ")
        };

        Some(json!({
            "id": format!("heypocket:heypocket.recording:{rid}"),
            "source": "heypocket",
            "type": "heypocket.recording",
            "external_id": rid,
            "title": raw.get("title").and_then(|v| v.as_str()).unwrap_or(""),
            "body_text": body,
            "occurred_at": parse_dt(raw.get("recording_at").and_then(|v| v.as_str()).or_else(|| raw.get("created_at").and_then(|v| v.as_str()))),
            "url": raw.get("url").or_else(|| raw.get("share_url")).and_then(|v| v.as_str()).unwrap_or(""),
            "payload": {
                "duration_seconds": raw.get("duration").cloned().unwrap_or(json!(0)),
                "tags": tags,
            },
            "links": [],
            "deleted": raw.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false),
        }))
    }
}

async fn client_for(ctx: &SourceCtx<'_>, src: &HeyPocketSource) -> AppResult<PocketAIClient> {
    let conn = connector_for(ctx.db, ctx.owner, src).await?;
    let base = conn.as_ref().and_then(|c| c.config.get("base_url")).and_then(|v| v.as_str()).map(String::from);
    let creds = credentials_for(ctx.db, ctx.encryption_key, ctx.owner, src).await?;
    Ok(PocketAIClient::new(&creds, base.as_deref()))
}

#[derive(Debug, Deserialize)]
pub(crate) struct CachedRecording {
    #[serde(default)]
    title: String,
    #[serde(default)]
    occurred_at: Option<surrealdb::Datetime>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    payload: Value,
}

async fn cached_recordings(ctx: &SourceCtx<'_>, days: i64) -> AppResult<Vec<CachedRecording>> {
    let since = Utc::now() - Duration::days(days);
    let mut res = store::app::SOURCES_HEYPOCKET_RECENT
        .on(ctx.db)
        .bind(("owner", ctx.owner.clone()))
        .bind(("since", surrealdb::Datetime::from(since)))
        .await?;
    Ok(res.take(0)?)
}

fn iso(dt: &Option<surrealdb::Datetime>) -> Option<String> {
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
/// cached recordings (not a live API call) -- matches the "no invented
/// metrics" ethos of the Python tool.
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

/// Deferred: Python's `search_recordings` runs `cache.search`'s hybrid
/// BM25+embedding search; `embeddings.service` isn't ported, so this is a
/// plain case-insensitive substring match over cached title/body_text.
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

    #[test]
    fn parse_dt_normalizes_to_rfc3339() {
        assert!(parse_dt(Some("2024-01-02T03:04:05Z")).is_some());
        assert_eq!(parse_dt(None), None);
        assert_eq!(parse_dt(Some("not-a-date")), None);
    }
}

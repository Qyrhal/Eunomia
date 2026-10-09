//! Generic read-only tools over the cache. Ported from `tools/generic.py`;
//! the queries themselves live in `cache::search` (one read path shared with
//! recall), this module only validates arguments and shapes the JSON.
//!
//! Every lookup is owner-scoped: each `cache_record`'s key is
//! `"{owner_key}:{record_id}"`, so a record id is never valid across owners.
//! Tombstoned records (`deleted = true`) are invisible here: not returned by
//! `get`, not matched by `search`/`list`, and never a `links` endpoint.
//!
//! All strings may contain `[eunomia:*]` tokens -- callers treat them as
//! opaque handles.

use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::cache::search::{self as cs, CacheRecord, RecordFilter, SearchParams};
use crate::db::Db;
use crate::error::AppResult;

const SNIPPET_LEN: usize = 200;

fn hit(rec: &CacheRecord) -> Value {
    let snippet: String = if rec.body_text.chars().count() > SNIPPET_LEN {
        let truncated: String = rec.body_text.chars().take(SNIPPET_LEN).collect();
        format!("{truncated}\u{2026}")
    } else {
        rec.body_text.clone()
    };
    json!({
        "id": rec.id,
        "source": rec.source,
        "type": rec.type_,
        "title": rec.title,
        "snippet": snippet,
        "occurred_at": &rec.occurred_at,
        "url": if rec.url.is_empty() { Value::Null } else { Value::String(rec.url.clone()) },
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn search(
    db: &Db,
    settings: &crate::config::Settings,
    owner: &RecordId,
    query: &str,
    sources: Option<&[String]>,
    types: Option<&[String]>,
    since: Option<&str>,
    until: Option<&str>,
    mode: &str,
    limit: i64,
    offset: i64,
) -> AppResult<Value> {
    let (since, until) = cs::parse_range(since, until)?;
    let non_empty = |v: Option<&[String]>| v.filter(|v| !v.is_empty()).map(<[String]>::to_vec);
    let params = SearchParams {
        filter: RecordFilter {
            sources: non_empty(sources),
            types: non_empty(types),
            since: since.map(Into::into),
            until: until.map(Into::into),
        },
        mode: mode.to_string(),
        limit: limit.clamp(0, 100) as usize,
        offset: offset.max(0) as usize,
    };
    let (recs, has_more) = cs::search(db, settings, owner, query, &params).await?;
    let results: Vec<Value> = recs.iter().map(hit).collect();
    Ok(json!({ "results": results, "has_more": has_more }))
}

pub async fn get(db: &Db, owner: &RecordId, id: &str) -> AppResult<Value> {
    let Some(rec) = cs::get(db, owner, id).await? else {
        return Ok(json!({ "error": "not found" }));
    };
    let links = cs::links(db, owner, &rec.id, None).await?;
    // A Pocket recording or transcript chunk: attach the whole stored
    // recording -- full transcript, summary, action items, tags.
    let recording = match rec.source.as_str() {
        "heypocket" => {
            let rid = rec.payload.get("recording_id").and_then(Value::as_str).unwrap_or(&rec.external_id);
            crate::sources::heypocket::stored(db, owner, rid).await?
        }
        _ => None,
    };
    let mut out = json!({
        "id": rec.id,
        "source": rec.source,
        "type": rec.type_,
        "external_id": rec.external_id,
        "title": rec.title,
        "body_text": rec.body_text,
        "occurred_at": &rec.occurred_at,
        "url": if rec.url.is_empty() { Value::Null } else { Value::String(rec.url.clone()) },
        "payload": rec.payload,
        "links": { "links": links },
    });
    if let Some(recording) = recording {
        out["recording"] = recording;
    }
    Ok(out)
}

pub async fn list(
    db: &Db,
    owner: &RecordId,
    type_: Option<&str>,
    filters: Option<&serde_json::Map<String, Value>>,
    sort: &str,
    limit: i64,
    offset: i64,
) -> AppResult<Value> {
    let filters = match filters {
        Some(f) => cs::parse_filters(f)?,
        None => Vec::new(),
    };
    let limit = limit.clamp(0, 200) as usize;
    let offset = offset.max(0) as usize;
    let (recs, total) = cs::list(db, owner, type_, &filters, sort, limit, offset).await?;
    let results: Vec<Value> = recs.iter().map(hit).collect();
    let has_more = (offset + results.len()) < total as usize;
    Ok(json!({ "results": results, "total": total, "has_more": has_more }))
}

pub async fn links(db: &Db, owner: &RecordId, id: &str, rel: Option<&str>) -> AppResult<Value> {
    Ok(json!({ "links": cs::links(db, owner, id, rel).await? }))
}

/// The tool names these four functions are registered under in the
/// REST/MCP catalogue (see `tools::registry`): `"search"`, `"get"`,
/// `"list"`, `"links"`. All four are read-only.
pub const TOOL_NAMES: &[&str] = &["search", "get", "list", "links"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_truncates_long_bodies_into_a_snippet() {
        let rec = CacheRecord { id: "demo:x:1".into(), body_text: "y".repeat(500), ..Default::default() };
        let h = hit(&rec);
        assert_eq!(h["snippet"].as_str().unwrap().chars().count(), SNIPPET_LEN + 1);
        assert_eq!(h["url"], Value::Null);
    }

    #[test]
    fn tool_names_are_the_four_generic_cache_tools() {
        assert_eq!(TOOL_NAMES, &["search", "get", "list", "links"]);
    }
}

//! Generic read-only tools over the cache. Ported from `tools/generic.py`
//! (which calls into `cache/search.py`) -- rather than depending on a Rust
//! `cache` module that doesn't exist yet on this side of the port, the
//! handful of SurrealDB queries these tools need are inlined here directly,
//! scoped the same way `cache/search.py` scopes them: each `cache_record`'s
//! key is `"{owner_key}:{record_id}"`, so a record id is never valid across
//! owners and a lookup by a bare `record_id` is always owner-scoped.
//!
//! All strings may contain `[eunomia:*]` tokens -- callers treat them as
//! opaque handles.
//!
//! Semantic search is NOT ported here: it depends on an `embeddings` module
//! that hasn't landed in Rust yet. `mode = "semantic"` and the semantic leg
//! of `mode = "hybrid"` currently contribute no results (see `semantic_ids`);
//! keyword search still works standalone and hybrid degrades to keyword-only
//! until embeddings exist.

use std::collections::HashMap;

use serde_json::{json, Value};
use surrealdb::{Datetime, RecordId, RecordIdKey};

use crate::db::Db;
use crate::error::AppResult;

const SNIPPET_LEN: usize = 200;
const RRF_K: f64 = 60.0;

/// Columns an agent may sort/filter on -- anything else is a clean error,
/// not a 500.
const FIELDS: &[&str] =
    &["occurred_at", "ingested_at", "updated_at", "title", "type", "source", "id", "external_id"];

#[derive(Debug, serde::Deserialize)]
struct Row {
    id: RecordId,
    #[serde(default)]
    source: String,
    #[serde(rename = "type", default)]
    type_: String,
    #[serde(default)]
    external_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    body_text: String,
    #[serde(default)]
    occurred_at: Option<Datetime>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    payload: Value,
}

struct CacheRecord {
    id: String,
    source: String,
    type_: String,
    external_id: String,
    title: String,
    body_text: String,
    occurred_at: Option<Datetime>,
    url: String,
    payload: Value,
}

/// The owner-id prefix baked into the scoped `cache_record` key is internal
/// plumbing; callers never see or pass it.
fn owner_key(owner: &RecordId) -> String {
    String::try_from(owner.key().clone()).unwrap_or_else(|_| owner.to_string())
}

fn scoped_rid(owner: &RecordId, record_id: &str) -> RecordId {
    RecordId::from_table_key("cache_record", format!("{}:{record_id}", owner_key(owner)))
}

/// The caller-facing record id -- strips the owner-key prefix back off.
fn literal(key: &RecordIdKey) -> String {
    let raw = String::try_from(key.clone()).unwrap_or_default();
    raw.split_once(':').map(|(_, rest)| rest.to_string()).unwrap_or(raw)
}

fn row_to_record(row: Row) -> CacheRecord {
    CacheRecord {
        id: literal(row.id.key()),
        source: row.source,
        type_: row.type_,
        external_id: row.external_id,
        title: row.title,
        body_text: row.body_text,
        occurred_at: row.occurred_at,
        url: row.url,
        payload: row.payload,
    }
}

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

async fn full(db: &Db, owner: &RecordId, rec: CacheRecord) -> AppResult<Value> {
    let links_result = links(db, owner, &rec.id, None).await?;
    Ok(json!({
        "id": rec.id,
        "source": rec.source,
        "type": rec.type_,
        "external_id": rec.external_id,
        "title": rec.title,
        "body_text": rec.body_text,
        "occurred_at": &rec.occurred_at,
        "url": if rec.url.is_empty() { Value::Null } else { Value::String(rec.url.clone()) },
        "payload": rec.payload,
        "links": links_result,
    }))
}

/// Reciprocal Rank Fusion, k=60: sum of `1/(60+rank+1)` per id across any
/// number of ranked lists.
fn rrf(ranked_lists: &[Vec<String>]) -> Vec<String> {
    let mut scores: HashMap<&str, f64> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for list in ranked_lists {
        for (rank, id) in list.iter().enumerate() {
            let entry = scores.entry(id.as_str()).or_insert_with(|| {
                order.push(id.as_str());
                0.0
            });
            *entry += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
    }
    let mut ids: Vec<&str> = order;
    ids.sort_by(|a, b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal));
    ids.into_iter().map(str::to_string).collect()
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
    let limit = limit.clamp(0, 100) as usize;
    let offset = offset.max(0) as usize;
    let pool = (limit * 4).max(40);

    // Shared with recall (cache::search): per-term keyword matching, and real
    // embeddings when the server has them -- keyword-only otherwise.
    use crate::cache::search as cs;
    let semantic_ok = crate::embeddings::service::available(db, settings, owner).await;
    let ids = match mode {
        "semantic" if semantic_ok => cs::semantic_ids(db, settings, owner, query, pool).await?,
        "keyword" | "semantic" => cs::keyword_ids(db, owner, query, pool).await?,
        _ => {
            let kw = cs::keyword_ids(db, owner, query, pool).await?;
            if semantic_ok {
                let sem = cs::semantic_ids(db, settings, owner, query, pool).await.unwrap_or_default();
                rrf(&[kw, sem])
            } else {
                kw
            }
        }
    };

    if ids.is_empty() {
        return Ok(json!({ "results": [], "has_more": false }));
    }

    let mut conditions = vec!["id IN $ids".to_string(), "owner = $owner".to_string(), "deleted = false".to_string()];
    let scoped_ids: Vec<RecordId> = ids.iter().map(|i| scoped_rid(owner, i)).collect();

    // Build the query string once all optional filters are known, then bind.
    if let Some(sources) = sources {
        if !sources.is_empty() {
            conditions.push("source IN $sources".to_string());
        }
    }
    if let Some(types) = types {
        if !types.is_empty() {
            conditions.push("type IN $types".to_string());
        }
    }
    if since.is_some() {
        conditions.push("occurred_at >= $since".to_string());
    }
    if until.is_some() {
        conditions.push("occurred_at <= $until".to_string());
    }

    let sql = format!("SELECT * FROM cache_record WHERE {}", conditions.join(" AND "));
    let mut query = db
        .query(sql)
        .bind(("ids", scoped_ids))
        .bind(("owner", owner.clone()));
    if let Some(sources) = sources {
        if !sources.is_empty() {
            query = query.bind(("sources", sources.to_vec()));
        }
    }
    if let Some(types) = types {
        if !types.is_empty() {
            query = query.bind(("types", types.to_vec()));
        }
    }
    if let Some(since) = since {
        query = query.bind(("since", since.to_string()));
    }
    if let Some(until) = until {
        query = query.bind(("until", until.to_string()));
    }

    let mut res = query.await?;
    let rows: Vec<Row> = res.take(0)?;
    let order: HashMap<&str, usize> = ids.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    let mut recs: Vec<CacheRecord> = rows.into_iter().map(row_to_record).collect();
    recs.sort_by_key(|r| order.get(r.id.as_str()).copied().unwrap_or(usize::MAX));

    let total = recs.len();
    let page: Vec<Value> = recs
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|r| hit(&r))
        .collect();
    let has_more = offset + page.len() < total;
    Ok(json!({ "results": page, "has_more": has_more }))
}

pub async fn get(db: &Db, owner: &RecordId, id: &str) -> AppResult<Value> {
    let row: Option<Row> = db.select(scoped_rid(owner, id)).await?;
    match row {
        Some(row) => full(db, owner, row_to_record(row)).await,
        None => Ok(json!({ "error": "not found" })),
    }
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
    let field_name = sort.strip_prefix('-').unwrap_or(sort);
    if !FIELDS.contains(&field_name) {
        let mut allowed: Vec<&str> = FIELDS.to_vec();
        allowed.sort();
        return Ok(json!({ "error": format!("unknown sort field {sort:?}; use one of {allowed:?}") }));
    }

    let mut bad: Vec<String> = Vec::new();
    if let Some(filters) = filters {
        for key in filters.keys() {
            let fname = key.split("__").next().unwrap_or(key);
            if !FIELDS.contains(&fname) && fname != "payload" {
                bad.push(fname.to_string());
            }
        }
    }
    if !bad.is_empty() {
        bad.sort();
        bad.dedup();
        let mut allowed: Vec<&str> = FIELDS.to_vec();
        allowed.sort();
        return Ok(json!({
            "error": format!("unknown filter field(s) {bad:?}; use one of {allowed:?} or payload__*")
        }));
    }

    let limit = limit.clamp(0, 200);
    let offset = offset.max(0);

    let mut conditions = vec!["owner = $owner".to_string(), "deleted = false".to_string()];
    if type_.is_some() {
        conditions.push("type = $type".to_string());
    }
    let mut filter_binds: Vec<(String, Value)> = Vec::new();
    if let Some(filters) = filters {
        for (key, val) in filters.iter() {
            let fname = key.split("__").next().unwrap_or(key);
            let param = format!("filter_{fname}");
            conditions.push(format!("{fname} = ${param}"));
            filter_binds.push((param, val.clone()));
        }
    }

    let direction = if sort.starts_with('-') { "DESC" } else { "ASC" };
    let sql = format!(
        "SELECT * FROM cache_record WHERE {} ORDER BY {field_name} {direction} LIMIT $limit START $offset",
        conditions.join(" AND ")
    );

    let mut q = db.query(sql).bind(("owner", owner.clone())).bind(("limit", limit)).bind(("offset", offset));
    if let Some(type_) = type_ {
        q = q.bind(("type", type_.to_string()));
    }
    for (name, val) in filter_binds.clone() {
        q = q.bind((name, val));
    }
    let mut res = q.await?;
    let rows: Vec<Row> = res.take(0)?;
    let recs: Vec<CacheRecord> = rows.into_iter().map(row_to_record).collect();

    let count_sql = format!("SELECT count() FROM cache_record WHERE {} GROUP ALL", conditions.join(" AND "));
    let mut cq = db.query(count_sql).bind(("owner", owner.clone()));
    if let Some(type_) = type_ {
        cq = cq.bind(("type", type_.to_string()));
    }
    for (name, val) in filter_binds {
        cq = cq.bind((name, val));
    }
    #[derive(serde::Deserialize)]
    struct CountRow {
        count: i64,
    }
    let mut cres = cq.await?;
    let count_rows: Vec<CountRow> = cres.take(0)?;
    let total = count_rows.first().map(|r| r.count).unwrap_or(0);

    let results: Vec<Value> = recs.iter().map(hit).collect();
    let has_more = offset + (results.len() as i64) < total;
    Ok(json!({ "results": results, "total": total, "has_more": has_more }))
}

pub async fn links(db: &Db, owner: &RecordId, id: &str, rel: Option<&str>) -> AppResult<Value> {
    #[derive(serde::Deserialize)]
    struct FwdRow {
        rel: String,
        out: RecordId,
    }
    #[derive(serde::Deserialize)]
    struct BackRow {
        rel: String,
        #[serde(rename = "in")]
        in_: RecordId,
    }

    let rid = scoped_rid(owner, id);
    let mut out = Vec::new();

    let fwd_sql = if rel.is_some() {
        "SELECT rel, out FROM linked_to WHERE in = $id AND rel = $rel"
    } else {
        "SELECT rel, out FROM linked_to WHERE in = $id"
    };
    let mut fq = db.query(fwd_sql).bind(("id", rid.clone()));
    if let Some(rel) = rel {
        fq = fq.bind(("rel", rel.to_string()));
    }
    let mut fres = fq.await?;
    let fwd_rows: Vec<FwdRow> = fres.take(0)?;
    for row in fwd_rows {
        out.push(json!({ "rel": row.rel, "direction": "out", "target_id": literal(row.out.key()) }));
    }

    let back_sql = if rel.is_some() {
        "SELECT rel, in FROM linked_to WHERE out = $id AND rel = $rel"
    } else {
        "SELECT rel, in FROM linked_to WHERE out = $id"
    };
    let mut bq = db.query(back_sql).bind(("id", rid));
    if let Some(rel) = rel {
        bq = bq.bind(("rel", rel.to_string()));
    }
    let mut bres = bq.await?;
    let back_rows: Vec<BackRow> = bres.take(0)?;
    for row in back_rows {
        out.push(json!({ "rel": row.rel, "direction": "in", "target_id": literal(row.in_.key()) }));
    }

    Ok(json!({ "links": out }))
}

/// The tool names these four functions are registered under in the
/// REST/MCP catalogue (see `tools::registry`): `"search"`, `"get"`,
/// `"list"`, `"links"`. All four are read-only.
pub const TOOL_NAMES: &[&str] = &["search", "get", "list", "links"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_orders_by_combined_reciprocal_rank() {
        let a = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let b = vec!["y".to_string(), "x".to_string()];
        let fused = rrf(&[a, b]);
        // "x" and "y" both appear in both lists near the top; "z" only once
        // and last -- it must not outrank either.
        assert_eq!(fused.last().unwrap(), "z");
        assert!(fused.contains(&"x".to_string()));
        assert!(fused.contains(&"y".to_string()));
    }

    #[test]
    fn rrf_is_empty_for_no_lists() {
        assert!(rrf(&[]).is_empty());
    }

    #[test]
    fn rrf_single_list_preserves_order() {
        let a = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(rrf(&[a.clone()]), a);
    }

    #[test]
    fn literal_strips_owner_key_prefix() {
        let key = RecordIdKey::from("owner123:github:issue:42".to_string());
        assert_eq!(literal(&key), "github:issue:42");
    }

    #[test]
    fn literal_handles_key_without_colon() {
        let key = RecordIdKey::from("bare".to_string());
        assert_eq!(literal(&key), "bare");
    }

    #[test]
    fn allowed_sort_fields_match_python_fields_set() {
        for f in ["occurred_at", "ingested_at", "updated_at", "title", "type", "source", "id", "external_id"] {
            assert!(FIELDS.contains(&f));
        }
        assert_eq!(FIELDS.len(), 8);
    }

    #[test]
    fn tool_names_are_the_four_generic_cache_tools() {
        assert_eq!(TOOL_NAMES, &["search", "get", "list", "links"]);
    }
}

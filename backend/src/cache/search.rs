//! Cache write + query API. The only module that knows SurrealDB's BM25/MTREE
//! indexes exist -- this module's external signatures (`upsert`,
//! `set_embedding`, `search`, `get`, `list_records`, `count_records`,
//! `links`) are the swap point for any future backend, same principle as the
//! Python `cache/search.py`.
//!
//! Ported from `cache/search.py`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use surrealdb::{Datetime, RecordId};

use crate::db::Db;
use crate::embeddings::service::DIM;
use crate::error::{AppError, AppResult};

const RRF_K: f64 = 60.0;

/// One `linked_to` relation to create/keep when upserting a record, mirrors
/// the `{"target": ..., "rel": ...}` shape of `env["links"]` in the Python
/// envelope dict.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LinkSpec {
    pub target: String,
    pub rel: String,
}

/// The ingest envelope a mapper produces for one raw source record. Mirrors
/// the Python `env` dict `cache/search.py::upsert` and `cache/ingest.py`
/// expect.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Envelope {
    pub id: String,
    #[serde(default)]
    pub source: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub external_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body_text: String,
    #[serde(default)]
    pub occurred_at: Option<Datetime>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub links: Vec<LinkSpec>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CacheRecord {
    /// literal `"{source}:{type}:{external_id}"` (no table prefix, no owner
    /// prefix -- see `literal`).
    pub id: String,
    pub source: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub external_id: String,
    pub title: String,
    pub body_text: String,
    pub occurred_at: Option<Datetime>,
    pub url: String,
    pub payload: Value,
    pub content_hash: String,
    pub ingested_at: Option<Datetime>,
    pub updated_at: Option<Datetime>,
    pub deleted: bool,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Deserialize)]
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
    #[serde(default)]
    content_hash: String,
    #[serde(default)]
    ingested_at: Option<Datetime>,
    #[serde(default)]
    updated_at: Option<Datetime>,
    #[serde(default)]
    deleted: bool,
    #[serde(default)]
    embedding: Option<Vec<f32>>,
}

/// The owner-id prefix baked into the internal `cache_record` key -- callers
/// never see or pass it. Mirrors `cache/search.py`'s `_rid`.
pub(crate) fn owner_key(owner: &RecordId) -> String {
    String::try_from(owner.key().clone()).unwrap_or_else(|_| owner.to_string())
}

pub(crate) fn rid(owner: &RecordId, record_id: &str) -> RecordId {
    RecordId::from_table_key("cache_record", format!("{}:{record_id}", owner_key(owner)))
}

/// The caller-facing record id -- mirrors `cache/search.py`'s `_literal`.
pub(crate) fn literal(key: &RecordId) -> String {
    let raw = String::try_from(key.key().clone()).unwrap_or_default();
    raw.split_once(':').map(|(_, rest)| rest.to_string()).unwrap_or(raw)
}

fn row_to_record(row: Row) -> CacheRecord {
    CacheRecord {
        id: literal(&row.id),
        source: row.source,
        type_: row.type_,
        external_id: row.external_id,
        title: row.title,
        body_text: row.body_text,
        occurred_at: row.occurred_at,
        url: row.url,
        payload: row.payload,
        content_hash: row.content_hash,
        ingested_at: row.ingested_at,
        updated_at: row.updated_at,
        deleted: row.deleted,
        embedding: row.embedding,
    }
}

/// `json!({...})`'s default `Map` is a `BTreeMap` (no `preserve_order`
/// feature enabled), so keys come out sorted -- same effect as Python's
/// `json.dumps(..., sort_keys=True)`. Mirrors `cache/search.py`'s
/// `_hash_envelope`; the exact byte representation doesn't need to match the
/// Python backend's hash (this is a from-scratch SurrealDB-backed cache, not
/// a shared store), only be stable within this implementation for dedup.
fn hash_envelope(env: &Envelope) -> String {
    let occurred_at_str = env.occurred_at.as_ref().map(|d| d.to_string());
    let blob = json!({
        "title": env.title,
        "body_text": env.body_text,
        "url": env.url,
        "occurred_at": occurred_at_str,
        "payload": env.payload,
        "deleted": env.deleted,
    });
    let s = serde_json::to_string(&blob).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn reconcile_links(db: &Db, owner: &RecordId, record_rid: &RecordId, links_spec: &[LinkSpec]) -> AppResult<()> {
    db.query("DELETE linked_to WHERE in = $id AND origin = 'sync'").bind(("id", record_rid.clone())).await?;
    for link in links_spec {
        // (in, out, rel) has a unique index -- an error here means the edge
        // already exists; idempotent no-op, matching the old
        // get_or_create-style Python behavior.
        let _ = db
            .query("RELATE $in->linked_to->$out SET rel = $rel, origin = 'sync'")
            .bind(("in", record_rid.clone()))
            .bind(("out", rid(owner, &link.target)))
            .bind(("rel", link.rel.clone()))
            .await;
    }
    Ok(())
}

/// Insert or update one envelope, scoped to `owner`. Returns `(record,
/// changed)`.
pub async fn upsert(db: &Db, owner: &RecordId, env: &Envelope) -> AppResult<(CacheRecord, bool)> {
    let record_rid = rid(owner, &env.id);
    let h = hash_envelope(env);

    let existing: Option<Row> = db.select(record_rid.clone()).await?;
    if let Some(existing) = existing {
        if existing.content_hash == h && !existing.deleted {
            db.query("UPDATE $id SET ingested_at = time::now()").bind(("id", record_rid.clone())).await?;
            let mut rec = row_to_record(existing);
            rec.ingested_at = Some(Datetime::from(chrono::Utc::now()));
            return Ok((rec, false));
        }
    }

    let mut res = db
        .query(
            "UPSERT $id SET owner = $owner, source = $source, type = $type, external_id = $external_id, \
             title = $title, body_text = $body_text, occurred_at = $occurred_at, url = $url, \
             payload = $payload, content_hash = $content_hash, ingested_at = $ingested_at, \
             updated_at = $updated_at, deleted = $deleted RETURN AFTER",
        )
        .bind(("id", record_rid.clone()))
        .bind(("owner", owner.clone()))
        .bind(("source", env.source.clone()))
        .bind(("type", env.type_.clone()))
        .bind(("external_id", env.external_id.clone()))
        .bind(("title", env.title.clone()))
        .bind(("body_text", env.body_text.clone()))
        .bind(("occurred_at", env.occurred_at.clone()))
        .bind(("url", env.url.clone()))
        .bind(("payload", env.payload.clone()))
        .bind(("content_hash", h))
        .bind(("ingested_at", Datetime::from(chrono::Utc::now())))
        .bind(("updated_at", Datetime::from(chrono::Utc::now())))
        .bind(("deleted", env.deleted))
        .await?;
    let rows: Vec<Row> = res.take(0)?;
    let row = rows.into_iter().next().ok_or_else(|| AppError::internal("cache_record upsert returned no row"))?;
    let rec = row_to_record(row);

    reconcile_links(db, owner, &record_rid, &env.links).await?;
    Ok((rec, true))
}

pub async fn set_embedding(db: &Db, owner: &RecordId, record_id: &str, vector: Vec<f32>) -> AppResult<()> {
    if vector.len() != DIM {
        return Err(AppError::bad_request(format!("embedding dim {} != {DIM}", vector.len())));
    }
    db.query("UPDATE $id SET embedding = $embedding")
        .bind(("id", rid(owner, record_id)))
        .bind(("embedding", vector))
        .await?;
    Ok(())
}

/// `cache_record_fts_idx` is a composite BM25 index over `(title,
/// body_text)`, but this SurrealDB version only resolves the `@N@` match
/// operator against the FIRST field of a composite search index (title) --
/// body_text-only matches raise "no suitable index". Use the index for
/// title, then fall back to a plain substring scan for body_text so keyword
/// search still covers both fields (functionally correct; just not
/// BM25-ranked for body-only hits). Mirrors `cache/search.py`'s
/// `_keyword_ids`.
pub(crate) async fn keyword_ids(db: &Db, owner: &RecordId, q: &str, limit: usize) -> AppResult<Vec<String>> {
    #[derive(Deserialize)]
    struct IdRow {
        id: RecordId,
    }

    let mut res = db
        .query(
            "SELECT id, search::score(1) AS score FROM cache_record \
             WHERE owner = $owner AND title @1@ $q AND deleted = false ORDER BY score DESC LIMIT $limit",
        )
        .bind(("owner", owner.clone()))
        .bind(("q", q.to_string()))
        .bind(("limit", limit as i64))
        .await?;
    let rows: Vec<IdRow> = res.take(0)?;
    let mut ids: Vec<String> = rows.iter().map(|r| literal(&r.id)).collect();

    if ids.len() < limit {
        let seen: Vec<RecordId> = ids.iter().map(|i| rid(owner, i)).collect();
        let mut res = db
            .query(
                "SELECT id FROM cache_record WHERE owner = $owner AND \
                 string::contains(string::lowercase(body_text), string::lowercase($q)) \
                 AND deleted = false AND id NOT IN $seen LIMIT $limit",
            )
            .bind(("owner", owner.clone()))
            .bind(("q", q.to_string()))
            .bind(("seen", seen))
            .bind(("limit", (limit - ids.len()) as i64))
            .await?;
        let extra: Vec<IdRow> = res.take(0)?;
        ids.extend(extra.into_iter().map(|r| literal(&r.id)));
    }
    ids.truncate(limit);
    Ok(ids)
}

/// Mirrors `cache/search.py`'s `_semantic_ids`. The KNN `<|K|>` operator
/// requires a literal integer -- it cannot be a bound parameter -- so
/// `limit` is interpolated directly rather than passed as a bind.
pub(crate) async fn semantic_ids(
    db: &Db,
    settings: &crate::config::Settings,
    owner: &RecordId,
    q: &str,
    limit: usize,
) -> AppResult<Vec<String>> {
    let vec = crate::embeddings::service::embed(db, settings, &[q.to_string()], Some(owner))
        .await?
        .into_iter()
        .next()
        .unwrap_or_default();

    #[derive(Deserialize)]
    struct IdRow {
        id: RecordId,
    }
    let query = format!(
        "SELECT id FROM cache_record WHERE owner = $owner AND embedding <|{}|> $vec AND deleted = false",
        limit as i64
    );
    let mut res = db.query(query).bind(("owner", owner.clone())).bind(("vec", vec)).await?;
    let rows: Vec<IdRow> = res.take(0)?;
    Ok(rows.into_iter().map(|r| literal(&r.id)).collect())
}

/// Reciprocal Rank Fusion, k=60: sum of `1/(60+rank+1)` per id across any
/// number of ranked lists. Exposed (not just the fused order) so callers
/// that need the raw fused score to apply further boosts on top -- e.g.
/// `cache::recall`'s 4-arm pipeline -- don't reimplement this formula.
/// Mirrors `cache/search.py`'s `_rrf_scores`.
pub(crate) fn rrf_scores(ranked_lists: &[Vec<String>]) -> HashMap<String, f64> {
    let mut scores: HashMap<String, f64> = HashMap::new();
    for list in ranked_lists {
        for (rank, id) in list.iter().enumerate() {
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
    }
    scores
}

fn rrf(ranked_lists: &[Vec<String>]) -> Vec<String> {
    let scores = rrf_scores(ranked_lists);
    let mut ids: Vec<String> = scores.keys().cloned().collect();
    ids.sort_by(|a, b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal));
    ids
}

/// Options for [`search`], mirroring `cache/search.py::search`'s keyword
/// arguments (`sources`, `types`, `since`, `until`, `mode`, `limit`,
/// `offset`).
#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    pub sources: Option<Vec<String>>,
    pub types: Option<Vec<String>>,
    pub since: Option<Datetime>,
    pub until: Option<Datetime>,
    pub mode: String,
    pub limit: usize,
    pub offset: usize,
}

impl SearchParams {
    pub fn new() -> Self {
        SearchParams { mode: "hybrid".to_string(), limit: 20, offset: 0, ..Default::default() }
    }
}

pub async fn search(
    db: &Db,
    settings: &crate::config::Settings,
    owner: &RecordId,
    q: &str,
    params: &SearchParams,
) -> AppResult<Vec<CacheRecord>> {
    let pool = (params.limit * 4).max(40);
    // Without embeddings (no OpenAI key) every mode degrades to keyword search.
    let semantic_ok = crate::embeddings::service::available(db, settings, owner).await;
    let ids = match params.mode.as_str() {
        "keyword" => keyword_ids(db, owner, q, pool).await?,
        "semantic" if semantic_ok => semantic_ids(db, settings, owner, q, pool).await?,
        "semantic" => keyword_ids(db, owner, q, pool).await?,
        _ if semantic_ok => {
            rrf(&[keyword_ids(db, owner, q, pool).await?, semantic_ids(db, settings, owner, q, pool).await?])
        }
        _ => keyword_ids(db, owner, q, pool).await?,
    };

    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut conditions = vec!["id IN $ids".to_string(), "owner = $owner".to_string(), "deleted = false".to_string()];
    if params.sources.is_some() {
        conditions.push("source IN $sources".to_string());
    }
    if params.types.is_some() {
        conditions.push("type IN $types".to_string());
    }
    if params.since.is_some() {
        conditions.push("occurred_at >= $since".to_string());
    }
    if params.until.is_some() {
        conditions.push("occurred_at <= $until".to_string());
    }

    let sql = format!("SELECT * FROM cache_record WHERE {}", conditions.join(" AND "));
    let scoped_ids: Vec<RecordId> = ids.iter().map(|i| rid(owner, i)).collect();
    let mut query = db.query(sql).bind(("ids", scoped_ids)).bind(("owner", owner.clone()));
    if let Some(sources) = &params.sources {
        query = query.bind(("sources", sources.clone()));
    }
    if let Some(types) = &params.types {
        query = query.bind(("types", types.clone()));
    }
    if let Some(since) = &params.since {
        query = query.bind(("since", since.clone()));
    }
    if let Some(until) = &params.until {
        query = query.bind(("until", until.clone()));
    }

    let rows: Vec<Row> = query.await?.take(0)?;
    let order: HashMap<&str, usize> = ids.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    let mut recs: Vec<CacheRecord> = rows.into_iter().map(row_to_record).collect();
    recs.sort_by_key(|r| order.get(r.id.as_str()).copied().unwrap_or(usize::MAX));

    Ok(recs.into_iter().skip(params.offset).take(params.limit).collect())
}

pub async fn get(db: &Db, owner: &RecordId, record_id: &str) -> AppResult<Option<CacheRecord>> {
    let row: Option<Row> = db.select(rid(owner, record_id)).await?;
    Ok(row.map(row_to_record))
}

#[derive(Debug, Clone, Default)]
pub struct ListParams {
    pub type_: Option<String>,
    pub filters: HashMap<String, Value>,
    /// `-field` for DESC (the default, on `occurred_at`), `field` for ASC.
    pub sort: String,
    pub limit: usize,
    pub offset: usize,
}

impl ListParams {
    pub fn new() -> Self {
        ListParams { sort: "-occurred_at".to_string(), limit: 50, offset: 0, ..Default::default() }
    }
}

fn apply_filters(conditions: &mut Vec<String>, params: &HashMap<String, Value>, bound: &mut HashMap<String, Value>) {
    for (key, val) in params {
        // `__`-suffixed lookups (e.g. `occurred_at__gte`) aren't given
        // special operator handling -- same as the Python version, which
        // only strips the suffix for the field name and always compares
        // with `=`.
        let field_name = key.split("__").next().unwrap_or(key);
        let param_name = format!("filter_{field_name}");
        conditions.push(format!("{field_name} = ${param_name}"));
        bound.insert(param_name, val.clone());
    }
}

pub async fn list_records(db: &Db, owner: &RecordId, params: &ListParams) -> AppResult<Vec<CacheRecord>> {
    let mut conditions = vec!["owner = $owner".to_string(), "deleted = false".to_string()];
    let mut bound: HashMap<String, Value> = HashMap::new();
    if let Some(t) = &params.type_ {
        conditions.push("type = $type".to_string());
        bound.insert("type".to_string(), json!(t));
    }
    apply_filters(&mut conditions, &params.filters, &mut bound);

    let field_name = params.sort.trim_start_matches('-');
    let direction = if params.sort.starts_with('-') { "DESC" } else { "ASC" };
    let sql = format!(
        "SELECT * FROM cache_record WHERE {} ORDER BY {field_name} {direction} LIMIT $limit START $offset",
        conditions.join(" AND ")
    );

    let mut query = db.query(sql).bind(("owner", owner.clone())).bind(("limit", params.limit as i64)).bind((
        "offset",
        params.offset as i64,
    ));
    for (k, v) in bound {
        query = query.bind((k, v));
    }
    let rows: Vec<Row> = query.await?.take(0)?;
    Ok(rows.into_iter().map(row_to_record).collect())
}

/// Total `cache_record` rows matching `list_records`'s same `type`/`filters`
/// conditions, ignoring `limit`/`offset`.
pub async fn count_records(db: &Db, owner: &RecordId, type_: Option<&str>, filters: &HashMap<String, Value>) -> AppResult<i64> {
    let mut conditions = vec!["owner = $owner".to_string(), "deleted = false".to_string()];
    let mut bound: HashMap<String, Value> = HashMap::new();
    if let Some(t) = type_ {
        conditions.push("type = $type".to_string());
        bound.insert("type".to_string(), json!(t));
    }
    apply_filters(&mut conditions, filters, &mut bound);

    let sql = format!("SELECT count() FROM cache_record WHERE {} GROUP ALL", conditions.join(" AND "));
    let mut query = db.query(sql).bind(("owner", owner.clone()));
    for (k, v) in bound {
        query = query.bind((k, v));
    }

    #[derive(Deserialize)]
    struct CountRow {
        count: i64,
    }
    let rows: Vec<CountRow> = query.await?.take(0)?;
    Ok(rows.first().map(|r| r.count).unwrap_or(0))
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkEntry {
    pub rel: String,
    pub direction: &'static str,
    pub target_id: String,
}

pub async fn links(db: &Db, owner: &RecordId, record_id: &str, rel: Option<&str>) -> AppResult<Vec<LinkEntry>> {
    let record_rid = rid(owner, record_id);
    let mut out = Vec::new();

    #[derive(Deserialize)]
    struct FwdRow {
        rel: String,
        out: RecordId,
    }
    #[derive(Deserialize)]
    struct BackRow {
        rel: String,
        #[serde(rename = "in")]
        in_: RecordId,
    }

    let fwd_sql = format!("SELECT rel, out FROM linked_to WHERE in = $id{}", if rel.is_some() { " AND rel = $rel" } else { "" });
    let mut q = db.query(fwd_sql).bind(("id", record_rid.clone()));
    if let Some(r) = rel {
        q = q.bind(("rel", r.to_string()));
    }
    let fwd: Vec<FwdRow> = q.await?.take(0)?;
    for row in fwd {
        out.push(LinkEntry { rel: row.rel, direction: "out", target_id: literal(&row.out) });
    }

    let back_sql = format!("SELECT rel, in FROM linked_to WHERE out = $id{}", if rel.is_some() { " AND rel = $rel" } else { "" });
    let mut q = db.query(back_sql).bind(("id", record_rid));
    if let Some(r) = rel {
        q = q.bind(("rel", r.to_string()));
    }
    let back: Vec<BackRow> = q.await?.take(0)?;
    for row in back {
        out.push(LinkEntry { rel: row.rel, direction: "in", target_id: literal(&row.in_) });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(title: &str, body: &str) -> Envelope {
        Envelope {
            id: "x:y:z".to_string(),
            source: "demo".to_string(),
            type_: "note".to_string(),
            external_id: "z".to_string(),
            title: title.to_string(),
            body_text: body.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn hash_envelope_is_deterministic() {
        let e = env("hello", "world");
        assert_eq!(hash_envelope(&e), hash_envelope(&e));
    }

    #[test]
    fn hash_envelope_changes_with_content() {
        assert_ne!(hash_envelope(&env("a", "b")), hash_envelope(&env("a", "c")));
    }

    #[test]
    fn hash_envelope_ignores_fields_outside_the_hashed_set() {
        let mut a = env("a", "b");
        let mut b = env("a", "b");
        a.id = "one".to_string();
        b.id = "two".to_string();
        a.source = "s1".to_string();
        b.source = "s2".to_string();
        assert_eq!(hash_envelope(&a), hash_envelope(&b));
    }

    #[test]
    fn rrf_scores_sums_reciprocal_ranks_across_lists() {
        let a = vec!["x".to_string(), "y".to_string()];
        let b = vec!["y".to_string(), "x".to_string()];
        let scores = rrf_scores(&[a, b]);
        // x: rank0 in a (1/61) + rank1 in b (1/62); y: rank1 in a (1/62) + rank0 in b (1/61)
        assert!((scores["x"] - scores["y"]).abs() < 1e-12);
    }

    #[test]
    fn rrf_orders_items_appearing_in_more_lists_first() {
        let a = vec!["x".to_string()];
        let b = vec!["x".to_string(), "y".to_string()];
        let c = vec!["y".to_string()];
        let order = rrf(&[a, b, c]);
        // x appears at rank0 twice; y appears at rank0 once + rank1 once --
        // x's score must be strictly higher.
        assert_eq!(order[0], "x");
    }

    #[test]
    fn apply_filters_strips_dunder_suffix_for_field_name_but_keeps_equality() {
        let mut conditions = Vec::new();
        let mut bound = HashMap::new();
        let mut filters = HashMap::new();
        filters.insert("occurred_at__gte".to_string(), json!("2024-01-01"));
        apply_filters(&mut conditions, &filters, &mut bound);
        assert_eq!(conditions, vec!["occurred_at = $filter_occurred_at".to_string()]);
        assert_eq!(bound["filter_occurred_at"], json!("2024-01-01"));
    }
}

//! Cache write + query API. The only module that knows SurrealDB's BM25/MTREE
//! indexes exist -- this module's external signatures (`upsert`,
//! `set_embedding`, `search`, `get`, `list_records`, `count_records`,
//! `links`) are the swap point for any future backend, same principle as the
//! Python `cache/search.py`.
//!
//! Ported from `cache/search.py`.

use surrealdb::types::SurrealValue;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use surrealdb::types::{Datetime, RecordId};
use crate::rid::RecordIdExt;

use crate::pool::OrgDb;
use crate::store;
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

#[derive(Debug, Deserialize, SurrealValue)]
struct Row {
    id: RecordId,
    #[serde(default)]
    #[surreal(default)]
    source: String,
    #[serde(rename = "type", default)]
    #[surreal(rename = "type", default)]
    type_: String,
    #[serde(default)]
    #[surreal(default)]
    external_id: String,
    #[serde(default)]
    #[surreal(default)]
    title: String,
    #[serde(default)]
    #[surreal(default)]
    body_text: String,
    #[serde(default)]
    #[surreal(default)]
    occurred_at: Option<Datetime>,
    #[serde(default)]
    #[surreal(default)]
    url: String,
    #[serde(default)]
    #[surreal(default)]
    payload: Value,
    #[serde(default)]
    #[surreal(default)]
    content_hash: String,
    #[serde(default)]
    #[surreal(default)]
    ingested_at: Option<Datetime>,
    #[serde(default)]
    #[surreal(default)]
    updated_at: Option<Datetime>,
    #[serde(default)]
    #[surreal(default)]
    deleted: bool,
    #[serde(default)]
    #[surreal(default)]
    embedding: Option<Vec<f32>>,
}

/// The owner-id prefix baked into the internal `cache_record` key -- callers
/// never see or pass it. Mirrors `cache/search.py`'s `_rid`.
pub(crate) fn owner_key(owner: &RecordId) -> String {
    crate::rid::key_string(owner.key()).unwrap_or_else(|| owner.to_string())
}

pub(crate) fn rid(owner: &RecordId, record_id: &str) -> RecordId {
    RecordId::from_table_key("cache_record", format!("{}:{record_id}", owner_key(owner)))
}

/// The caller-facing record id -- mirrors `cache/search.py`'s `_literal`.
pub(crate) fn literal(key: &RecordId) -> String {
    let raw = crate::rid::key_string(key.key()).unwrap_or_default();
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
    // Stored hashes come from SurrealDB 2.x, whose `Datetime` displayed as `d'...'`; keep that
    // text so an upgraded install does not see every record as changed (and re-embed it).
    let occurred_at_str = env.occurred_at.as_ref().map(|d| format!("d'{d}'"));
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

async fn reconcile_links(db: &OrgDb, owner: &RecordId, record_rid: &RecordId, links_spec: &[LinkSpec]) -> AppResult<()> {
    store::cache::DELETE_SYNC_LINKS.on(db).bind(("id", record_rid.clone())).await?;
    for link in links_spec {
        // (in, out, rel) has a unique index -- an error here means the edge
        // already exists; idempotent no-op, matching the old
        // get_or_create-style Python behavior.
        let _ = store::cache::RELATE_SYNC_LINK
            .on(db)
            .bind(("in", record_rid.clone()))
            .bind(("out", rid(owner, &link.target)))
            .bind(("rel", link.rel.clone()))
            .await;
    }
    Ok(())
}

/// Insert or update one envelope, scoped to `owner`. Returns `(record,
/// changed)`.
pub async fn upsert(db: &OrgDb, owner: &RecordId, env: &Envelope) -> AppResult<(CacheRecord, bool)> {
    let record_rid = rid(owner, &env.id);
    let h = hash_envelope(env);

    let existing: Option<Row> = store::get(db, &record_rid).await?;
    if let Some(existing) = existing
        && existing.content_hash == h && !existing.deleted {
            store::cache::TOUCH_INGESTED.on(db).bind(("id", record_rid.clone())).await?;
            let mut rec = row_to_record(existing);
            rec.ingested_at = Some(Datetime::from(chrono::Utc::now()));
            return Ok((rec, false));
        }

    let mut res = store::cache::UPSERT_RECORD
        .on(db)
        .bind(("id", record_rid.clone()))
        .bind(("owner", owner.clone()))
        .bind(("source", env.source.clone()))
        .bind(("type", env.type_.clone()))
        .bind(("external_id", env.external_id.clone()))
        .bind(("title", env.title.clone()))
        .bind(("body_text", env.body_text.clone()))
        .bind(("occurred_at", env.occurred_at))
        .bind(("url", env.url.clone()))
        // 3.x rejects NULL for the `object` field; an envelope without a payload means "none".
        .bind(("payload", if env.payload.is_null() { json!({}) } else { env.payload.clone() }))
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

pub async fn set_embedding(db: &OrgDb, owner: &RecordId, record_id: &str, vector: Vec<f32>) -> AppResult<()> {
    if vector.len() != DIM {
        return Err(AppError::bad_request(format!("embedding dim {} != {DIM}", vector.len())));
    }
    store::cache::SET_EMBEDDING
        .on(db)
        .bind(("id", rid(owner, record_id)))
        .bind(("embedding", vector))
        .await?;
    Ok(())
}

/// Words worth matching on, from a natural-language query: lowercase, split
/// on non-alphanumerics, minus filler words, deduped, at most 8. SurrealDB's
/// `@@` full-text match (and a whole-query substring scan) only hit when
/// EVERY word matches, so a question like "how do updates work?" found
/// nothing; callers instead match each term and rank with [`rank_term_hits`].
pub(crate) fn search_terms(q: &str) -> Vec<String> {
    const FILLER: &[&str] = &[
        "a", "an", "the", "and", "or", "but", "if", "of", "to", "in", "on", "at", "by", "for", "from", "with", "about",
        "into", "as", "is", "are", "was", "were", "be", "been", "am", "do", "does", "did", "done", "have", "has", "had",
        "i", "me", "my", "we", "us", "our", "you", "your", "he", "she", "it", "its", "they", "them", "their", "this",
        "that", "these", "those", "what", "which", "who", "whom", "whose", "when", "where", "why", "how", "can",
        "could", "would", "should", "will", "shall", "may", "might", "must", "so", "than", "then", "there", "here",
        "any", "some", "all", "not", "no", "yes", "just", "also", "please", "tell", "know", "remember", "recall",
        "anything", "something", "thing", "things", "get", "got", "like", "up", "out", "s", "t",
    ];
    let mut out: Vec<String> = Vec::new();
    for w in q.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if !w.is_empty() && !FILLER.contains(&w) && !out.iter().any(|o| o == w) {
            out.push(w.to_string());
        }
    }
    out.truncate(8);
    out
}

/// Fuses per-term hit lists (`(id, score)`, one list per term) into one
/// ranking: items matching more of the terms first, then by summed score.
pub(crate) fn rank_term_hits(per_term: Vec<Vec<(String, f64)>>, limit: usize) -> Vec<String> {
    let mut acc: HashMap<String, (usize, f64)> = HashMap::new();
    for hits in per_term {
        let mut seen = std::collections::HashSet::new();
        for (id, score) in hits {
            if seen.insert(id.clone()) {
                let e = acc.entry(id).or_insert((0, 0.0));
                e.0 += 1;
                e.1 += score;
            }
        }
    }
    let mut ranked: Vec<(String, (usize, f64))> = acc.into_iter().collect();
    ranked.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(b.1 .1.partial_cmp(&a.1 .1).unwrap_or(std::cmp::Ordering::Equal)).then(a.0.cmp(&b.0)));
    ranked.into_iter().take(limit).map(|(id, _)| id).collect()
}

/// `title` and `body_text` each have their own FULLTEXT index (a two-field index only resolves
/// the first field). Per search term: the title index (BM25-scored, +1 so a title hit beats a
/// body-only one) plus the body index (flat score); fused by [`rank_term_hits`].
pub(crate) async fn keyword_ids(db: &OrgDb, owner: &RecordId, q: &str, limit: usize) -> AppResult<Vec<String>> {
    #[derive(Deserialize, SurrealValue)]
    struct ScoredRow {
        id: RecordId,
        #[serde(default)]
        #[surreal(default)]
        score: f64,
    }

    let mut per_term = Vec::new();
    for term in search_terms(q) {
        let mut res = store::cache::KEYWORD_IDS
            .on(db)
            .bind(("owner", owner.clone()))
            .bind(("t", term))
            .bind(("limit", limit as i64))
            .await?;
        let title: Vec<ScoredRow> = res.take(0)?;
        let body: Vec<ScoredRow> = res.take(1)?;
        per_term.push(
            title
                .into_iter()
                .map(|r| (literal(&r.id), r.score + 1.0))
                .chain(body.into_iter().map(|r| (literal(&r.id), 0.5)))
                .collect(),
        );
    }
    Ok(rank_term_hits(per_term, limit))
}

/// Mirrors `cache/search.py`'s `_semantic_ids`: embed the query, then [`nearest_ids`].
pub(crate) async fn semantic_ids(
    db: &OrgDb,
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
    nearest_ids(db, owner, vec, limit).await
}

/// The `limit` live records of `owner` closest to `vec`, best first. HNSW KNN (`<|K,EF|>`, both
/// literal integers: they cannot be bound parameters) answers first. The index is shared by every
/// owner and the owner filter is not part of the ANN walk, so a small owner among big ones can get
/// fewer than `limit` rows back; then an exact cosine scan over just this owner's records is the
/// answer (it is ground truth, and cheap exactly when the owner is small).
pub async fn nearest_ids(db: &OrgDb, owner: &RecordId, vec: Vec<f32>, limit: usize) -> AppResult<Vec<String>> {
    #[derive(Deserialize, SurrealValue)]
    struct IdRow {
        id: RecordId,
    }
    let ef = (limit * 2).max(64);
    let query = format!(
        "SELECT id FROM cache_record WHERE owner = $owner AND embedding <|{limit},{ef}|> $vec AND deleted = false"
    );
    // dynamic: the KNN `<|K,EF|>` operator needs literal integers, one SQL text per (K, EF).
    let mut res = store::dynamic(db, "cache.semantic_ids", query)
        .bind(("owner", owner.clone()))
        .bind(("vec", vec.clone()))
        .await?;
    let rows: Vec<IdRow> = res.take(0)?;
    if rows.len() >= limit {
        return Ok(rows.into_iter().map(|r| literal(&r.id)).collect());
    }
    let mut res = store::cache::SEMANTIC_EXACT
        .on(db)
        .bind(("owner", owner.clone()))
        .bind(("vec", vec))
        .bind(("limit", limit as i64))
        .await?;
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
    db: &OrgDb,
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
    // dynamic: 16 combinations of the four optional filters.
    let mut query = store::dynamic(db, "cache.search_filtered", sql).bind(("ids", scoped_ids)).bind(("owner", owner.clone()));
    if let Some(sources) = &params.sources {
        query = query.bind(("sources", sources.clone()));
    }
    if let Some(types) = &params.types {
        query = query.bind(("types", types.clone()));
    }
    if let Some(since) = &params.since {
        query = query.bind(("since", *since));
    }
    if let Some(until) = &params.until {
        query = query.bind(("until", *until));
    }

    let rows: Vec<Row> = query.await?.take(0)?;
    let order: HashMap<&str, usize> = ids.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    let mut recs: Vec<CacheRecord> = rows.into_iter().map(row_to_record).collect();
    recs.sort_by_key(|r| order.get(r.id.as_str()).copied().unwrap_or(usize::MAX));

    Ok(recs.into_iter().skip(params.offset).take(params.limit).collect())
}

pub async fn get(db: &OrgDb, owner: &RecordId, record_id: &str) -> AppResult<Option<CacheRecord>> {
    let row: Option<Row> = store::get(db, &rid(owner, record_id)).await?;
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

/// Field names are spliced into SQL text, so only plain identifiers pass.
fn is_ident(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn apply_filters(conditions: &mut Vec<String>, params: &HashMap<String, Value>, bound: &mut HashMap<String, Value>) {
    for (key, val) in params {
        // `__`-suffixed lookups (e.g. `occurred_at__gte`) aren't given
        // special operator handling -- same as the Python version, which
        // only strips the suffix for the field name and always compares
        // with `=`.
        let field_name = key.split("__").next().unwrap_or(key);
        if !is_ident(field_name) {
            continue;
        }
        let param_name = format!("filter_{field_name}");
        conditions.push(format!("{field_name} = ${param_name}"));
        bound.insert(param_name, val.clone());
    }
}

pub async fn list_records(db: &OrgDb, owner: &RecordId, params: &ListParams) -> AppResult<Vec<CacheRecord>> {
    let mut conditions = vec!["owner = $owner".to_string(), "deleted = false".to_string()];
    let mut bound: HashMap<String, Value> = HashMap::new();
    if let Some(t) = &params.type_ {
        conditions.push("type = $type".to_string());
        bound.insert("type".to_string(), json!(t));
    }
    apply_filters(&mut conditions, &params.filters, &mut bound);

    let field_name = params.sort.trim_start_matches('-');
    let field_name = if is_ident(field_name) { field_name } else { "occurred_at" };
    let direction = if params.sort.starts_with('-') { "DESC" } else { "ASC" };
    let sql = format!(
        "SELECT * FROM cache_record WHERE {} ORDER BY {field_name} {direction} LIMIT $limit START $offset",
        conditions.join(" AND ")
    );

    // dynamic: caller-chosen filter fields and sort column.
    let mut query = store::dynamic(db, "cache.list_records", sql).bind(("owner", owner.clone())).bind(("limit", params.limit as i64)).bind((
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
pub async fn count_records(db: &OrgDb, owner: &RecordId, type_: Option<&str>, filters: &HashMap<String, Value>) -> AppResult<i64> {
    let mut conditions = vec!["owner = $owner".to_string(), "deleted = false".to_string()];
    let mut bound: HashMap<String, Value> = HashMap::new();
    if let Some(t) = type_ {
        conditions.push("type = $type".to_string());
        bound.insert("type".to_string(), json!(t));
    }
    apply_filters(&mut conditions, filters, &mut bound);

    let sql = format!("SELECT count() FROM cache_record WHERE {} GROUP ALL", conditions.join(" AND "));
    // dynamic: caller-chosen filter fields.
    let mut query = store::dynamic(db, "cache.count_records", sql).bind(("owner", owner.clone()));
    for (k, v) in bound {
        query = query.bind((k, v));
    }

    #[derive(Deserialize, SurrealValue)]
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

pub async fn links(db: &OrgDb, owner: &RecordId, record_id: &str, rel: Option<&str>) -> AppResult<Vec<LinkEntry>> {
    let record_rid = rid(owner, record_id);
    let mut out = Vec::new();

    #[derive(Deserialize, SurrealValue)]
    struct FwdRow {
        rel: String,
        out: RecordId,
    }
    #[derive(Deserialize, SurrealValue)]
    struct BackRow {
        rel: String,
        #[serde(rename = "in")]
        #[surreal(rename = "in")]
        in_: RecordId,
    }

    let fwd = if rel.is_some() { &store::cache::LINKS_OUT_REL } else { &store::cache::LINKS_OUT };
    let mut q = fwd.on(db).bind(("id", record_rid.clone()));
    if let Some(r) = rel {
        q = q.bind(("rel", r.to_string()));
    }
    let fwd: Vec<FwdRow> = q.await?.take(0)?;
    for row in fwd {
        out.push(LinkEntry { rel: row.rel, direction: "out", target_id: literal(&row.out) });
    }

    let back = if rel.is_some() { &store::cache::LINKS_IN_REL } else { &store::cache::LINKS_IN };
    let mut q = back.on(db).bind(("id", record_rid));
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

    #[test]
    fn search_terms_drop_filler_and_keep_content_words() {
        assert_eq!(search_terms("How do updates work in Eunomia?"), vec!["updates", "work", "eunomia"]);
        assert_eq!(search_terms("What does Ada like?"), vec!["ada"]);
        assert_eq!(search_terms("the the THE"), Vec::<String>::new());
        assert_eq!(search_terms("v1.2.0 release notes"), vec!["v1", "2", "0", "release", "notes"]);
        assert_eq!(search_terms(&"word ".repeat(3)), vec!["word"]);
        assert_eq!(search_terms("a b c d e f g h i j k l m n o p").len(), 8);
    }

    #[test]
    fn rank_term_hits_prefers_items_matching_more_terms() {
        let per_term = vec![
            vec![("a".to_string(), 5.0), ("b".to_string(), 1.0)],
            vec![("b".to_string(), 1.0), ("c".to_string(), 9.0)],
        ];
        // b matched both terms, so it beats higher-scoring single-term hits
        assert_eq!(rank_term_hits(per_term, 10), vec!["b", "c", "a"]);
    }

    #[test]
    fn rank_term_hits_counts_a_term_once_per_item_and_truncates() {
        let per_term = vec![vec![("a".to_string(), 1.0), ("a".to_string(), 1.0), ("b".to_string(), 3.0)]];
        assert_eq!(rank_term_hits(per_term, 1), vec!["b"]);
        assert!(rank_term_hits(vec![], 5).is_empty());
    }

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

    /// The expected value was computed from the 2.x text form, so this fails if the hash input
    /// drifts (datetime format, field order, `serde_json` key order) and would orphan stored hashes.
    #[test]
    fn hash_envelope_matches_the_value_stored_by_2x() {
        let e = Envelope {
            title: "Woolworths".into(),
            body_text: "Groceries $42.50".into(),
            url: "https://x/y".into(),
            occurred_at: Some("2026-01-05T09:00:00Z".parse().unwrap()),
            payload: serde_json::from_str(r#"{"zeta":1,"alpha":{"b":2,"a":1}}"#).unwrap(),
            ..Default::default()
        };
        assert_eq!(hash_envelope(&e), "86d1d37b57ed8b728294c14bb1fad628e4c94df7acac2e2836a152f6bf18ac41");
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
    fn apply_filters_drops_non_identifier_fields() {
        let mut conditions = Vec::new();
        let mut bound = HashMap::new();
        let filters = HashMap::from([("x = 1 OR true; DELETE user; --".to_string(), json!(1))]);
        apply_filters(&mut conditions, &filters, &mut bound);
        assert!(conditions.is_empty() && bound.is_empty());
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

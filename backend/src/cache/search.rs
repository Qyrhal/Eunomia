//! Cache write + query API. The only module that knows SurrealDB's BM25/MTREE
//! indexes exist -- this module's external signatures (`upsert`,
//! `set_embedding`, `search`, `get`, `list`, `links`) are the swap point for
//! any future backend, same principle as the Python `cache/search.py`.
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

/// Most results one `search`/`list` call may page through (`offset + limit`);
/// past that, narrow the query rather than page further.
pub(crate) const MAX_WINDOW: usize = 1000;

/// An ISO 8601 / RFC 3339 timestamp, or a bare `YYYY-MM-DD` date (midnight
/// UTC). Anything else is a 400 naming `field`, not a silent string compare.
pub(crate) fn parse_datetime(field: &str, s: &str) -> AppResult<chrono::DateTime<chrono::Utc>> {
    let s = s.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    if let Some(dt) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)) {
        return Ok(dt.and_utc());
    }
    Err(AppError::bad_request(format!("{field}: {s:?} is not an ISO 8601 date or datetime")))
}

/// A `since`/`until` pair, each optional; a reversed range is an error.
pub(crate) fn parse_range(
    since: Option<&str>,
    until: Option<&str>,
) -> AppResult<(Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)> {
    let since = since.map(|s| parse_datetime("since", s)).transpose()?;
    let until = until.map(|s| parse_datetime("until", s)).transpose()?;
    if let (Some(a), Some(b)) = (since, until) {
        if a > b {
            return Err(AppError::bad_request("since must not be after until"));
        }
    }
    Ok((since, until))
}

/// Source/type/date restrictions, applied inside candidate generation rather
/// than to an already-truncated candidate list -- so a match in a small
/// source can't be crowded out by other sources' hits. Bound as a whole
/// (`.bind(filter)`); [`RecordFilter::sql`] only references the set fields.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RecordFilter {
    pub sources: Option<Vec<String>>,
    pub types: Option<Vec<String>>,
    pub since: Option<Datetime>,
    pub until: Option<Datetime>,
}

impl RecordFilter {
    fn sql(&self) -> String {
        let mut s = String::new();
        if self.sources.is_some() {
            s.push_str(" AND source IN $sources");
        }
        if self.types.is_some() {
            s.push_str(" AND type IN $types");
        }
        if self.since.is_some() {
            s.push_str(" AND occurred_at >= $since");
        }
        if self.until.is_some() {
            s.push_str(" AND occurred_at <= $until");
        }
        s
    }
}

/// Per search term: the title BM25 index (composite `cache_record_fts_idx`,
/// whose first field is title; +1 so a title hit beats a body-only one) and
/// the body_text BM25 index (`cache_record_body_fts_idx` -- SurrealDB only
/// resolves `@N@` against a composite index's FIRST field, hence the separate
/// one), both pre-filtered by `filter`; fused by [`rank_term_hits`].
pub(crate) async fn keyword_ids(
    db: &Db,
    owner: &RecordId,
    q: &str,
    filter: &RecordFilter,
    limit: usize,
) -> AppResult<Vec<String>> {
    #[derive(Deserialize)]
    struct ScoredRow {
        id: RecordId,
        #[serde(default)]
        score: f64,
    }

    // every term's two lookups in one round trip
    let cond = filter.sql();
    let terms = search_terms(q);
    let mut sql = String::new();
    for i in 0..terms.len() {
        sql.push_str(&format!(
            "SELECT id, search::score(1) AS score FROM cache_record \
             WHERE owner = $owner AND title @1@ $t{i} AND deleted = false{cond} ORDER BY score DESC, id LIMIT $limit; \
             SELECT id, search::score(2) AS score FROM cache_record \
             WHERE owner = $owner AND body_text @2@ $t{i} AND deleted = false{cond} ORDER BY score DESC, id LIMIT $limit;"
        ));
    }
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = db.query(sql).bind(filter.clone()).bind(("owner", owner.clone())).bind(("limit", limit as i64));
    for (i, t) in terms.into_iter().enumerate() {
        query = query.bind((format!("t{i}"), t));
    }
    let mut res = query.await?;
    let mut per_term = Vec::new();
    for i in 0..res.num_statements() / 2 {
        let title: Vec<ScoredRow> = res.take(2 * i)?;
        let body: Vec<ScoredRow> = res.take(2 * i + 1)?;
        per_term.push(
            title
                .into_iter()
                .map(|r| (literal(&r.id), r.score + 1.0))
                .chain(body.into_iter().map(|r| (literal(&r.id), r.score)))
                .collect(),
        );
    }
    Ok(rank_term_hits(per_term, limit))
}

/// Mirrors `cache/search.py`'s `_semantic_ids`. The KNN `<|K|>` operator
/// requires a literal integer -- it cannot be a bound parameter -- so
/// `limit` is interpolated directly rather than passed as a bind. `filter`
/// conditions are applied during the KNN search, not after it.
pub(crate) async fn semantic_ids(
    db: &Db,
    settings: &crate::config::Settings,
    owner: &RecordId,
    q: &str,
    filter: &RecordFilter,
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
        "SELECT id FROM cache_record WHERE owner = $owner AND embedding <|{}|> $vec AND deleted = false{}",
        limit as i64,
        filter.sql()
    );
    let mut res = db.query(query).bind(filter.clone()).bind(("owner", owner.clone())).bind(("vec", vec)).await?;
    let rows: Vec<IdRow> = res.take(0)?;
    Ok(rows.into_iter().map(|r| literal(&r.id)).collect())
}

/// Reciprocal Rank Fusion, k=60: sum of `1/(60+rank+1)` per id across any
/// number of ranked lists. Exposed (not just the fused order) so callers
/// that need the raw fused score to apply further boosts on top -- e.g.
/// `cache::recall`'s 5-arm pipeline -- don't reimplement this formula.
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

/// [`rrf_scores`] as an order; ties by id so paging is stable.
fn rrf(ranked_lists: &[Vec<String>]) -> Vec<String> {
    let scores = rrf_scores(ranked_lists);
    let mut ids: Vec<String> = scores.keys().cloned().collect();
    ids.sort_by(|a, b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(b)));
    ids
}

/// Options for [`search`].
#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    pub filter: RecordFilter,
    pub mode: String,
    pub limit: usize,
    pub offset: usize,
}

/// One page of records matching `q`, plus whether more exist. Filters are
/// pushed into every candidate query and candidates are fetched up to
/// `offset + limit + 1`, so later pages are reachable and `has_more` is
/// exact for the fused candidate list.
pub async fn search(
    db: &Db,
    settings: &crate::config::Settings,
    owner: &RecordId,
    q: &str,
    p: &SearchParams,
) -> AppResult<(Vec<CacheRecord>, bool)> {
    let window = p
        .offset
        .checked_add(p.limit)
        .filter(|w| *w <= MAX_WINDOW)
        .ok_or_else(|| AppError::bad_request(format!("offset + limit must be at most {MAX_WINDOW}")))?;
    let pool = window + 1;
    let f = &p.filter;
    // Without embeddings (no OpenAI key) every mode degrades to keyword search.
    let semantic_ok = crate::embeddings::service::available(db, settings, owner).await;
    let ids = match p.mode.as_str() {
        "semantic" if semantic_ok => semantic_ids(db, settings, owner, q, f, pool).await?,
        "keyword" | "semantic" => keyword_ids(db, owner, q, f, pool).await?,
        _ if semantic_ok => {
            let kw = keyword_ids(db, owner, q, f, pool).await?;
            rrf(&[kw, semantic_ids(db, settings, owner, q, f, pool).await.unwrap_or_default()])
        }
        _ => keyword_ids(db, owner, q, f, pool).await?,
    };

    let has_more = ids.len() > window;
    let page: Vec<String> = ids.into_iter().skip(p.offset).take(p.limit).collect();
    Ok((get_many(db, owner, &page).await?, has_more))
}

/// `owner`'s live records for `ids`, in `ids` order -- tombstoned
/// (`deleted = true`) and missing ids are dropped. One query for any number
/// of ids; the one read path behind `get`, `search` and recall hydration.
pub(crate) async fn get_many(db: &Db, owner: &RecordId, ids: &[String]) -> AppResult<Vec<CacheRecord>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rids: Vec<RecordId> = ids.iter().map(|i| rid(owner, i)).collect();
    let mut res = db
        .query("SELECT * FROM $ids WHERE owner = $owner AND deleted = false")
        .bind(("ids", rids))
        .bind(("owner", owner.clone()))
        .await?;
    let rows: Vec<Row> = res.take(0)?;
    let mut by_id: HashMap<String, CacheRecord> =
        rows.into_iter().map(row_to_record).map(|r| (r.id.clone(), r)).collect();
    Ok(ids.iter().filter_map(|i| by_id.remove(i)).collect())
}

/// One live record; a tombstoned record reads as not found.
pub async fn get(db: &Db, owner: &RecordId, record_id: &str) -> AppResult<Option<CacheRecord>> {
    Ok(get_many(db, owner, &[record_id.to_string()]).await?.pop())
}

/// Columns an agent may sort/filter `list` on -- anything else is a 400.
pub const FIELDS: &[&str] = &["occurred_at", "ingested_at", "updated_at", "title", "type", "source", "id", "external_id"];
const DATETIME_FIELDS: &[&str] = &["occurred_at", "ingested_at", "updated_at"];
/// `field__op` suffixes `list` filters understand; no suffix means equality.
const FILTER_OPS: &[(&str, &str)] = &[("ne", "!="), ("gt", ">"), ("gte", ">="), ("lt", "<"), ("lte", "<=")];

/// One validated `list` filter: `path` is a known column or a `payload.a.b`
/// path made only of `[A-Za-z0-9_]` segments, so it is safe to interpolate.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    path: String,
    op: &'static str,
    value: Value,
    datetime: bool,
}

/// Parses `{"field": v, "field__gte": v, "payload__a__b__lt": v, ...}`.
/// Datetime columns take ISO 8601 values and compare as datetimes;
/// `payload__<path>` targets that nested payload field; an unknown field or
/// operator is an error rather than a different query.
pub fn parse_filters(filters: &serde_json::Map<String, Value>) -> AppResult<Vec<Filter>> {
    let ops: Vec<&str> = FILTER_OPS.iter().map(|(name, _)| *name).collect();
    let mut out = Vec::new();
    for (key, value) in filters {
        let mut parts: Vec<&str> = key.split("__").collect();
        let op = match parts.last() {
            Some(last) if parts.len() > 1 => FILTER_OPS.iter().find(|(name, _)| name == last).map(|(_, sql)| *sql),
            _ => None,
        };
        if op.is_some() {
            parts.pop();
        }
        let (field, rest) = (parts[0], &parts[1..]);
        let path = if field == "payload" {
            let ok = |s: &&str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if rest.is_empty() || !rest.iter().all(ok) {
                return Err(AppError::bad_request(format!(
                    "filter {key:?}: payload filters name a field, e.g. payload__category or payload__amount_cents__lt"
                )));
            }
            format!("payload.{}", rest.join("."))
        } else if !FIELDS.contains(&field) {
            return Err(AppError::bad_request(format!(
                "unknown filter field {field:?}; use one of {FIELDS:?} or payload__<field>"
            )));
        } else if !rest.is_empty() {
            return Err(AppError::bad_request(format!(
                "filter {key:?}: unsupported operator {:?}; use one of {ops:?} (or none for equality)",
                rest.join("__")
            )));
        } else {
            field.to_string()
        };
        let datetime = DATETIME_FIELDS.contains(&field);
        let value = if datetime {
            let s = value
                .as_str()
                .ok_or_else(|| AppError::bad_request(format!("filter {key:?}: expected an ISO 8601 string")))?;
            Value::String(parse_datetime(key, s)?.to_rfc3339())
        } else {
            value.clone()
        };
        out.push(Filter { path, op: op.unwrap_or("="), value, datetime });
    }
    Ok(out)
}

/// One page of `owner`'s live records of `type_` matching `filters`, sorted
/// by `sort` (`-field` for DESC), plus the total match count.
pub async fn list(
    db: &Db,
    owner: &RecordId,
    type_: Option<&str>,
    filters: &[Filter],
    sort: &str,
    limit: usize,
    offset: usize,
) -> AppResult<(Vec<CacheRecord>, i64)> {
    let field = sort.strip_prefix('-').unwrap_or(sort);
    if !FIELDS.contains(&field) {
        return Err(AppError::bad_request(format!("unknown sort field {sort:?}; use one of {FIELDS:?}")));
    }
    let direction = if sort.starts_with('-') { "DESC" } else { "ASC" };

    let mut conditions = vec!["owner = $owner".to_string(), "deleted = false".to_string()];
    if type_.is_some() {
        conditions.push("type = $type".to_string());
    }
    for (i, f) in filters.iter().enumerate() {
        let param = if f.datetime { format!("<datetime>$f{i}") } else { format!("$f{i}") };
        conditions.push(format!("{} {} {param}", f.path, f.op));
    }
    let where_ = conditions.join(" AND ");
    let sql = format!(
        "SELECT * FROM cache_record WHERE {where_} ORDER BY {field} {direction} LIMIT $limit START $offset; \
         SELECT count() FROM cache_record WHERE {where_} GROUP ALL"
    );
    let mut q = db
        .query(sql)
        .bind(("owner", owner.clone()))
        .bind(("type", type_.map(str::to_string)))
        .bind(("limit", limit as i64))
        .bind(("offset", offset as i64));
    for (i, f) in filters.iter().enumerate() {
        q = q.bind((format!("f{i}"), f.value.clone()));
    }
    let mut res = q.await?;
    let rows: Vec<Row> = res.take(0)?;

    #[derive(Deserialize)]
    struct CountRow {
        count: i64,
    }
    let counts: Vec<CountRow> = res.take(1)?;
    Ok((rows.into_iter().map(row_to_record).collect(), counts.first().map(|r| r.count).unwrap_or(0)))
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkEntry {
    pub rel: String,
    pub direction: &'static str,
    pub target_id: String,
}

/// A record's links in both directions. Only links between two live records
/// count: a tombstoned (or never-synced) endpoint hides the link, and a
/// tombstoned record has no links at all.
pub async fn links(db: &Db, owner: &RecordId, record_id: &str, rel: Option<&str>) -> AppResult<Vec<LinkEntry>> {
    #[derive(Deserialize)]
    struct LinkRow {
        rel: String,
        other: RecordId,
    }

    let rel_cond = if rel.is_some() { " AND rel = $rel" } else { "" };
    let sql = format!(
        "SELECT rel, out AS other FROM linked_to WHERE in = $id AND in.deleted = false AND out.deleted = false{rel_cond}; \
         SELECT rel, in AS other FROM linked_to WHERE out = $id AND in.deleted = false AND out.deleted = false{rel_cond}"
    );
    let mut res = db.query(sql).bind(("id", rid(owner, record_id))).bind(("rel", rel.map(str::to_string))).await?;
    let fwd: Vec<LinkRow> = res.take(0)?;
    let back: Vec<LinkRow> = res.take(1)?;
    Ok(fwd
        .into_iter()
        .map(|r| LinkEntry { rel: r.rel, direction: "out", target_id: literal(&r.other) })
        .chain(back.into_iter().map(|r| LinkEntry { rel: r.rel, direction: "in", target_id: literal(&r.other) }))
        .collect())
}

/// For each live record among `sources`, its live linked records in either
/// direction -- one graph-traversal query for the whole batch (recall's graph
/// arm). Tombstoned sources and targets are skipped.
pub(crate) async fn live_neighbours(db: &Db, sources: &[RecordId]) -> AppResult<HashMap<RecordId, Vec<RecordId>>> {
    if sources.is_empty() {
        return Ok(HashMap::new());
    }
    #[derive(Deserialize)]
    struct NRow {
        id: RecordId,
        #[serde(default)]
        fwd: Vec<RecordId>,
        #[serde(default)]
        back: Vec<RecordId>,
    }
    let mut res = db
        .query(
            "SELECT id, ->linked_to->(cache_record WHERE deleted = false) AS fwd, \
             <-linked_to<-(cache_record WHERE deleted = false) AS back FROM $ids WHERE deleted = false",
        )
        .bind(("ids", sources.to_vec()))
        .await?;
    let rows: Vec<NRow> = res.take(0)?;
    Ok(rows.into_iter().map(|r| (r.id, r.fwd.into_iter().chain(r.back).collect())).collect())
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

    fn filters(v: Value) -> AppResult<Vec<Filter>> {
        parse_filters(v.as_object().unwrap())
    }

    #[test]
    fn parse_filters_maps_operators_and_compares_dates_as_datetimes() {
        let f = filters(json!({"occurred_at__gte": "2024-01-01"})).unwrap();
        assert_eq!(f[0].path, "occurred_at");
        assert_eq!(f[0].op, ">=");
        assert!(f[0].datetime);
        assert_eq!(f[0].value, json!("2024-01-01T00:00:00+00:00"));
        assert_eq!(filters(json!({"title__ne": "x"})).unwrap()[0].op, "!=");
        assert_eq!(filters(json!({"source": "demo"})).unwrap()[0].op, "=");
        assert!(filters(json!({"occurred_at__lt": "last tuesday"})).is_err());
        assert!(filters(json!({"occurred_at": 5})).is_err());
    }

    #[test]
    fn parse_filters_targets_the_named_payload_field() {
        let f = filters(json!({"payload__category": "Groceries", "payload__a__b__lt": 3})).unwrap();
        let paths: Vec<(&str, &str)> = f.iter().map(|f| (f.path.as_str(), f.op)).collect();
        assert!(paths.contains(&("payload.category", "=")));
        assert!(paths.contains(&("payload.a.b", "<")));
        assert!(filters(json!({"payload": {"x": 1}})).is_err());
        assert!(filters(json!({"payload__a-b": 1})).is_err());
        assert!(filters(json!({"payload__x;DELETE": 1})).is_err());
    }

    #[test]
    fn parse_filters_rejects_unknown_fields_and_operators() {
        assert!(filters(json!({"title__contains": "x"})).is_err());
        assert!(filters(json!({"title__gte__lt": "x"})).is_err());
        assert!(filters(json!({"bogus": "x"})).is_err());
        assert!(filters(json!({})).unwrap().is_empty());
    }

    #[test]
    fn parse_range_validates_dates_and_order() {
        assert!(parse_range(Some("2024-01-01"), Some("2024-02-01T10:00:00Z")).is_ok());
        assert!(parse_range(Some("2024-03-01"), Some("2024-02-01")).is_err());
        assert!(parse_range(Some("yesterday"), None).is_err());
        assert_eq!(parse_range(None, None).unwrap(), (None, None));
    }

    #[test]
    fn record_filter_sql_only_mentions_set_fields() {
        assert_eq!(RecordFilter::default().sql(), "");
        let f = RecordFilter { sources: Some(vec!["demo".into()]), until: Some(Datetime::default()), ..Default::default() };
        assert_eq!(f.sql(), " AND source IN $sources AND occurred_at <= $until");
    }

    #[test]
    fn rrf_breaks_ties_by_id_for_stable_pages() {
        let a = vec!["b".to_string(), "a".to_string()];
        let b = vec!["a".to_string(), "b".to_string()];
        assert_eq!(rrf(&[a, b]), vec!["a", "b"]);
    }
}

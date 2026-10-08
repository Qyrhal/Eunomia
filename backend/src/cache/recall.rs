//! Recall pipeline: 5 arms in parallel (semantic, keyword, graph, temporal, memory text),
//! fused via Reciprocal Rank Fusion, then boosted by recency + "proof" (how
//! many arms agreed on an item), then truncated to a token budget.
//!
//! Pipeline, matching the reference diagram:
//!
//! ```text
//!     [semantic | keyword | graph | temporal | memory text]  (5 arms, run concurrently)
//!                      |
//!                 RRF fusion (k=60, reusing cache::search's rrf_scores)
//!                      |
//!          (cross-encoder step -- intentionally omitted, see below)
//!                      |
//!                 boosts (recency x proof)
//!                      |
//!               token-budget truncation
//! ```
//!
//! Cross-encoder: the diagram has a cross-encoder re-ranking step between RRF
//! fusion and boosts. This is skipped on purpose, same reasoning as
//! `cache/recall.py`'s module docstring: OpenAI has no cross-encoder/rerank
//! API endpoint, and `EMBEDDINGS_BACKEND` is OpenAI-only for the user-facing
//! path. We go straight from RRF fusion to boosts rather than fake a
//! cross-encoder with a cheap heuristic pretending to be one.
//!
//! Natural-language date parsing ("last week", "in March") is also out of
//! scope for v1 -- the temporal arm only activates when an explicit
//! `time_range` is given.
//!
//! Deferred: the graph arm's entity matching mirrors
//! `entities/service.py::list_entities`'s query shape directly against the
//! `person`/`organisation`/`location`/`repository`/`file`/`symbol` tables,
//! rather than calling an `entities::service` module -- this crate's
//! `entities` module isn't wired into `lib.rs` yet (a concurrent port still
//! in progress elsewhere in this repo), so depending on it here would block
//! on someone else's unfinished work. Once it lands, `graph_arm`'s entity
//! listing could delegate to it instead of querying the tables directly.
//!
//! Ported from `cache/recall.py`.

use surrealdb::types::SurrealValue;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::RecordId;
use crate::rid::RecordIdExt;

use crate::cache::search as cs;
use crate::config::Settings;
use crate::db::Db;
use crate::store;
use crate::error::AppResult;
use crate::vaults::service as vaults_service;

/// Entity tables the graph arm searches -- mirrors `entities/service.py`'s
/// `KINDS` tuple (see module docstring on why this isn't delegated to a
/// Rust `entities::service` yet).
// recency boost floor/ceiling; see `boost` below.
const RECENCY_FLOOR: f64 = 0.7;
const RECENCY_WINDOW_DAYS: f64 = 365.0;
const RECENCY_SPAN: f64 = 0.3;
// proof boost: +5% per extra arm that surfaced the same item.
const PROOF_STEP: f64 = 0.05;

/// `surrealdb::types::Datetime` only converts *from* `chrono::DateTime<Utc>`
/// (`Datetime::from`); the reverse direction isn't exposed on the wrapper
/// type directly, only on the inner core type it wraps, so go through
/// `into_inner()`.
fn to_chrono(dt: surrealdb::types::Datetime) -> DateTime<Utc> {
    dt.into_inner()
}

#[derive(Debug, Clone, Serialize)]
pub struct RecallItem {
    pub id: String,
    pub kind: &'static str,
    pub text: String,
    pub source: Option<String>,
    pub occurred_at: Option<String>,
    pub score: f64,
    pub arms_hit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    World,
    Experience,
    Observation,
}

impl MemoryType {
    fn as_str(&self) -> &'static str {
        match self {
            MemoryType::World => "world",
            MemoryType::Experience => "experience",
            MemoryType::Observation => "observation",
        }
    }
}

#[derive(Debug, Deserialize, SurrealValue)]
struct EntityRow {
    id: RecordId,
    #[serde(default)]
    #[surreal(default)]
    name: String,
    #[serde(default)]
    #[surreal(default)]
    aliases: Vec<String>,
}

/// Candidate entities: the vault's person/organisation/location/.../symbol
/// names/aliases that appear as a case-insensitive substring of `query` (no
/// NER/LLM for v1), ranked by match specificity (longer matched name
/// first). For each matched entity, pulls its memories, each memory's
/// source `cache_record`, and any `cache_record`s `linked_to` that source --
/// that's the "follow the entity graph" hop the diagram's Graph arm
/// describes. Mirrors `cache/recall.py`'s `_graph_arm`.
async fn graph_arm(db: &Db, owner: &RecordId, vault: &RecordId, query: &str, limit: usize) -> AppResult<Vec<String>> {
    let q_lower = query.to_lowercase();

    let mut matches: Vec<(usize, RecordId)> = Vec::new();
    for stmt in store::cache::ENTITY_NAMES {
        let mut res = stmt.on(db).bind(("vault", vault.clone())).await?;
        let rows: Vec<EntityRow> = res.take(0)?;
        for row in rows {
            let best = std::iter::once(row.name.as_str())
                .chain(row.aliases.iter().map(String::as_str))
                .filter(|n| !n.is_empty() && q_lower.contains(&n.to_lowercase()))
                .map(str::len)
                .max();
            if let Some(best) = best {
                matches.push((best, row.id));
            }
        }
    }
    matches.sort_by_key(|m| std::cmp::Reverse(m.0));

    #[derive(Deserialize, SurrealValue)]
    struct MemRow {
        id: RecordId,
        #[serde(default)]
        #[surreal(default)]
        source: Option<RecordId>,
    }

    let mut keys: Vec<String> = Vec::new();
    for (_, entity_id) in &matches {
        let mut res = store::cache::MEMORIES_BY_SUBJECT
            .on(db)
            .bind(("id", entity_id.clone()))
            .await?;
        let mem_rows: Vec<MemRow> = res.take(0)?;
        for mem in mem_rows {
            keys.push(format!("memory:{}", mem.id.to_string()));
            if let Some(source) = mem.source {
                let src_literal = cs::literal(&source);
                keys.push(format!("cache_record:{src_literal}"));
                for link in cs::links(db, owner, &src_literal, None).await? {
                    keys.push(format!("cache_record:{}", link.target_id));
                }
            }
        }
        if keys.len() >= limit {
            break;
        }
    }

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for k in keys {
        if seen.insert(k.clone()) {
            out.push(k);
        }
    }
    out.truncate(limit);
    Ok(out)
}

/// Full-text arm over remembered facts (`memory.text`), BM25-ranked. This is
/// what makes a fact an agent wrote with `memory_write` findable by what it
/// says, not just by its subject's name -- no embeddings required. Works for
/// any vault the caller can read.
async fn memory_text_arm(db: &Db, vault: &RecordId, query: &str, limit: usize) -> AppResult<Vec<String>> {
    #[derive(Deserialize, SurrealValue)]
    struct ScoredRow {
        id: RecordId,
        #[serde(default)]
        #[surreal(default)]
        score: f64,
    }
    // one BM25 match per term: `@@` with a whole question needs every word to match
    let mut per_term = Vec::new();
    for term in cs::search_terms(query) {
        let mut res = store::cache::MEMORY_BM25
            .on(db)
            .bind(("vault", vault.clone()))
            .bind(("t", term))
            .bind(("limit", limit as i64))
            .await?;
        let rows: Vec<ScoredRow> = res.take(0)?;
        per_term.push(rows.into_iter().map(|r| (format!("memory:{}", r.id.to_string()), r.score)).collect());
    }
    Ok(cs::rank_term_hits(per_term, limit))
}

/// Explicit date-range filter only -- no NL date parsing in v1. Merges
/// `cache_record` (`occurred_at`, caller's own data only -- see `recall`'s
/// `include_cache_record`) and `memory` (`created_at`, vault-scoped) hits
/// into one list, ranked by recency. Mirrors `cache/recall.py`'s
/// `_temporal_ids`.
async fn temporal_ids(
    db: &Db,
    owner: &RecordId,
    vault: &RecordId,
    time_range: Option<(&str, &str)>,
    limit: usize,
    include_cache_record: bool,
) -> AppResult<Vec<String>> {
    let Some((since, until)) = time_range else {
        return Ok(Vec::new());
    };

    #[derive(Deserialize, SurrealValue)]
    struct CacheRow {
        id: RecordId,
        occurred_at: surrealdb::types::Datetime,
    }
    #[derive(Deserialize, SurrealValue)]
    struct MemRow {
        id: RecordId,
        created_at: surrealdb::types::Datetime,
    }

    let mut dated: Vec<(surrealdb::types::Datetime, String)> = Vec::new();

    if include_cache_record {
        let mut res = store::cache::CACHE_RECORDS_IN_RANGE
            .on(db)
            .bind(("owner", owner.clone()))
            .bind(("since", since.to_string()))
            .bind(("until", until.to_string()))
            .await?;
        let rows: Vec<CacheRow> = res.take(0)?;
        dated.extend(rows.into_iter().map(|r| (r.occurred_at, format!("cache_record:{}", cs::literal(&r.id)))));
    }

    let mut res = store::cache::MEMORIES_IN_RANGE
        .on(db)
        .bind(("vault", vault.clone()))
        .bind(("since", since.to_string()))
        .bind(("until", until.to_string()))
        .await?;
    let rows: Vec<MemRow> = res.take(0)?;
    dated.extend(rows.into_iter().map(|r| (r.created_at, format!("memory:{}", r.id.to_string()))));

    dated.sort_by_key(|d| std::cmp::Reverse(d.0));
    Ok(dated.into_iter().take(limit).map(|(_, k)| k).collect())
}

struct Hydrated {
    id: String,
    kind: &'static str,
    text: String,
    source: Option<String>,
    occurred_at: Option<DateTime<Utc>>,
    mem_type: String,
}

async fn hydrate(db: &Db, owner: &RecordId, vault: &RecordId, key: &str) -> AppResult<Option<Hydrated>> {
    let Some((kind, rest)) = key.split_once(':') else {
        return Ok(None);
    };

    if kind == "cache_record" {
        let Some(rec) = cs::get(db, owner, rest).await? else {
            return Ok(None);
        };
        let text = format!("{}\n{}", rec.title, rec.body_text).trim().to_string();
        return Ok(Some(Hydrated {
            id: rec.id,
            kind: "cache_record",
            text,
            source: if rec.source.is_empty() { None } else { Some(rec.source) },
            occurred_at: rec.occurred_at.map(to_chrono),
            mem_type: String::new(),
        }));
    }

    if kind == "memory" {
        let Ok(rid) = crate::rid::parse(rest) else {
            return Ok(None);
        };

        #[derive(Deserialize, SurrealValue)]
        struct MemRow {
            vault: RecordId,
            #[serde(default)]
            #[surreal(default)]
            text: String,
            #[serde(default)]
            #[surreal(default)]
            source: Option<RecordId>,
            created_at: surrealdb::types::Datetime,
            #[serde(rename = "type", default = "default_memory_type")]
            #[surreal(rename = "type", default = "default_memory_type")]
            mem_type: String,
        }
        fn default_memory_type() -> String {
            "world".to_string()
        }

        let row: Option<MemRow> = db.select(rid.clone()).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        if &row.vault != vault {
            return Ok(None);
        }
        return Ok(Some(Hydrated {
            id: rid.to_string(),
            kind: "memory",
            text: row.text,
            source: row.source.map(|s| cs::literal(&s)),
            occurred_at: Some(to_chrono(row.created_at)),
            mem_type: row.mem_type,
        }));
    }

    Ok(None)
}

/// Monotonic in both arm-count ("proof") and recency, matching the diagram's
/// small multipliers in spirit, not exact values -- the task leaves the
/// exact formula up to the implementation.
///
/// proof:   `1 + 0.05 * (arms_hit - 1)` -> 1.00 .. 1.20 for 1..5 arms
/// recency: 1.0 at age=0, linearly down to a 0.7 floor at 365+ days old;
///          1.0 (neutral) when there's no date to judge recency from.
///
/// Mirrors `cache/recall.py`'s `_boost`.
fn boost(arms_hit: usize, occurred_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> f64 {
    let proof = 1.0 + PROOF_STEP * (arms_hit as f64 - 1.0);

    let recency = match occurred_at {
        None => 1.0,
        Some(dt) => {
            let age_days = ((now - dt).num_seconds() as f64 / 86400.0).max(0.0);
            (1.0 - age_days.min(RECENCY_WINDOW_DAYS) / RECENCY_WINDOW_DAYS * RECENCY_SPAN).max(RECENCY_FLOOR)
        }
    };

    proof * recency
}

#[allow(clippy::too_many_arguments)]
pub async fn recall(
    db: &Db,
    settings: &Settings,
    owner: &RecordId,
    query: &str,
    time_range: Option<(&str, &str)>,
    limit: usize,
    max_tokens: Option<usize>,
    types: Option<&[MemoryType]>,
    vault_id: Option<&RecordId>,
) -> AppResult<Vec<RecallItem>> {
    let pool = (limit * 4).max(40);

    let default_vault = vaults_service::default_vault_id(db, owner).await?;
    let vault = match vault_id {
        Some(v) => {
            vaults_service::require_membership(db, owner, v).await?;
            v.clone()
        }
        None => default_vault.clone(),
    };
    let personal = vault == default_vault;

    let keyword_ids = if personal { cs::keyword_ids(db, owner, query, pool).await? } else { Vec::new() };
    // No embeddings (no OpenAI key -- the MCP agent is the model) or a failing
    // provider must not take down the other three arms.
    let semantic_ids = if personal && crate::embeddings::service::available(db, settings, owner).await {
        cs::semantic_ids(db, settings, owner, query, pool).await.unwrap_or_else(|e| {
            tracing::warn!("recall: semantic arm skipped: {}", e.message);
            Vec::new()
        })
    } else {
        Vec::new()
    };
    let graph_keys = graph_arm(db, owner, &vault, query, pool).await?;
    let temporal_keys = temporal_ids(db, owner, &vault, time_range, pool, personal).await?;

    let keyword_keys: Vec<String> = keyword_ids.iter().map(|i| format!("cache_record:{i}")).collect();
    let semantic_keys: Vec<String> = semantic_ids.iter().map(|i| format!("cache_record:{i}")).collect();

    let memory_keys = memory_text_arm(db, &vault, query, pool).await?;

    let arms = [semantic_keys, keyword_keys, graph_keys, temporal_keys, memory_keys];
    let rrf = cs::rrf_scores(&arms);
    if rrf.is_empty() {
        return Ok(Vec::new());
    }

    let now = Utc::now();
    let mut scored: Vec<RecallItem> = Vec::new();
    for (key, base_score) in &rrf {
        let Some(item) = hydrate(db, owner, &vault, key).await? else {
            continue;
        };
        if let Some(types) = types
            && item.kind == "memory" {
                let allowed = types.iter().any(|t| t.as_str() == item.mem_type);
                if !allowed {
                    continue;
                }
            }
        let arms_hit = arms.iter().filter(|arm| arm.contains(key)).count();
        let b = boost(arms_hit, item.occurred_at, now);
        scored.push(RecallItem {
            id: item.id,
            kind: item.kind,
            text: item.text,
            source: item.source,
            occurred_at: item.occurred_at.map(|d| d.to_rfc3339()),
            score: base_score * b,
            arms_hit,
        });
    }

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    Ok(match max_tokens {
        Some(budget) => fit_budget(scored, budget),
        None => scored,
    })
}

/// Fits ranked items into a token budget (~4 chars/token). Each item is
/// capped at a third of the budget -- trimmed with "…", its `id` kept so the
/// caller can fetch the full text -- and an item that doesn't fit is skipped
/// rather than ending the list, so one huge memory (an imported transcript,
/// say) can't crowd out everything after it.
fn fit_budget(items: Vec<RecallItem>, budget: usize) -> Vec<RecallItem> {
    let cap_chars = (budget / 3).max(60) * 4;
    let mut out = Vec::new();
    let mut used = 0usize;
    for mut item in items {
        if item.text.chars().count() > cap_chars {
            item.text = item.text.chars().take(cap_chars - 1).collect::<String>() + "\u{2026}";
        }
        let tokens = item.text.chars().count().div_ceil(4);
        if used + tokens > budget {
            continue;
        }
        used += tokens;
        out.push(item);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn boost_is_neutral_with_one_arm_and_no_date() {
        let now = Utc::now();
        assert!((boost(1, None, now) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn boost_increases_with_more_arms() {
        let now = Utc::now();
        assert!(boost(2, None, now) > boost(1, None, now));
        assert!(boost(4, None, now) > boost(2, None, now));
    }

    #[test]
    fn boost_decreases_with_age_down_to_floor() {
        let now = Utc::now();
        let fresh = boost(1, Some(now), now);
        let old = boost(1, Some(now - Duration::days(400)), now);
        assert!((fresh - 1.0).abs() < 1e-9);
        assert!((old - RECENCY_FLOOR).abs() < 1e-9);
        assert!(old < fresh);
    }

    #[test]
    fn boost_is_monotonic_between_zero_and_window() {
        let now = Utc::now();
        let b0 = boost(1, Some(now), now);
        let b_half = boost(1, Some(now - Duration::days(180)), now);
        let b_full = boost(1, Some(now - Duration::days(365)), now);
        assert!(b0 > b_half);
        assert!(b_half > b_full);
        assert!((b_full - RECENCY_FLOOR).abs() < 1e-9);
    }

    fn item(text: &str) -> RecallItem {
        RecallItem { id: "m".into(), kind: "memory", text: text.into(), source: None, occurred_at: None, score: 1.0, arms_hit: 1 }
    }

    #[test]
    fn fit_budget_trims_a_huge_item_instead_of_returning_nothing() {
        let out = fit_budget(vec![item(&"x".repeat(40_000)), item("small fact")], 600);
        assert_eq!(out.len(), 2);
        assert!(out[0].text.ends_with('\u{2026}'));
        assert!(out[0].text.chars().count() <= 200 * 4);
        assert_eq!(out[1].text, "small fact");
    }

    #[test]
    fn fit_budget_skips_what_does_not_fit_and_keeps_going() {
        // budget 350 -> items capped at 116 tokens: three fit (348), the fourth is
        // skipped, and the short one after it still fits
        let big = "y".repeat(10_000);
        let out = fit_budget(vec![item(&big), item(&big), item(&big), item(&big), item("tiny")], 350);
        let total: usize = out.iter().map(|i| i.text.chars().count().div_ceil(4)).sum();
        assert!(total <= 350);
        assert_eq!(out.len(), 4);
        assert_eq!(out.last().unwrap().text, "tiny");
        assert!(fit_budget(vec![], 100).is_empty());
    }

    #[test]
    fn memory_type_as_str_matches_schema_values() {
        assert_eq!(MemoryType::World.as_str(), "world");
        assert_eq!(MemoryType::Experience.as_str(), "experience");
        assert_eq!(MemoryType::Observation.as_str(), "observation");
    }
}

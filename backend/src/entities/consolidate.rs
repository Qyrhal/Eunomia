//! Observation consolidation: periodically synthesizes an entity's raw
//! `memory` facts ("world"/"experience" rows) into one durable belief
//! statement, stored as that subject's `type="observation"` memory row.
//!
//! Modeled on vectorize.io's Hindsight "observations" concept, with
//! deliberate scope cuts (see `entities/consolidate.py`'s module docstring
//! for the full rationale): scope is always one entity (`subject`), no
//! NLP-based world/experience classification, "update history" is just the
//! `version` counter, and the mission text is a single configurable string
//! rather than a full strategy-matching system.
//!
//! Staleness: handled eagerly, not at read time -- `service::add_memory`
//! sets `status="stale"` on a subject's existing observation the moment a
//! new raw fact is written for it. This module only ever sets `status="fresh"`
//! again, once consolidation has caught up.
//!
//! Best-effort throughout, same safety pattern as `extract.rs`: never fails
//! the caller, no-ops in stub-backend mode.
//!
//! Ported from `entities/consolidate.py`.

use surrealdb::types::SurrealValue;
use std::collections::HashSet;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::types::RecordId;
use crate::rid::RecordIdExt;

use crate::config::Settings;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::store::entities as q;
use crate::tx::with_retry;

use super::service::observation_rid;

use super::extract::{app_settings_row, resolve_openai};

pub const DEFAULT_MISSION: &str = "Observations are stable facts about people and relationships: preferences, skills, roles, \
recurring patterns, and how they change over time. Ignore ephemeral or one-off details.";

#[derive(Debug, Deserialize, SurrealValue)]
struct SubjectRow {
    vault: RecordId,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct RawMemoryRow {
    id: RecordId,
    text: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct ObservationRow {
    id: RecordId,
    #[serde(default)]
    #[surreal(default)]
    text: String,
    #[serde(default)]
    #[surreal(default)]
    source_memories: Option<Vec<RecordId>>,
    #[serde(default)]
    #[surreal(default)]
    version: i64,
}

/// The result of a successful consolidation -- just enough to let callers
/// (the `consolidate_observations` tool) report what happened; mirrors the
/// Python version's "truthy dict vs `None`" convention as `Option<..>`.
#[derive(Debug, Clone)]
pub struct ConsolidatedObservation {
    pub id: String,
    pub text: String,
}

fn build_prompt(mission: &str, current_belief: Option<&str>, facts: &[String]) -> String {
    let facts_block = facts.iter().map(|f| format!("- {f}")).collect::<Vec<_>>().join("\n");
    format!(
        "{mission}\n\n\
You maintain a single evolving belief statement about one subject, built from raw facts observed about them over time. \
Given the current belief (if any) and a batch of new raw facts, produce an updated belief statement that synthesizes \
all of it -- evolving the belief where facts have changed rather than just appending, and preserving genuinely \
still-true parts of the old belief. Return strict JSON, no prose, with this exact shape:\n\n\
{{\"belief\": str}}\n\n\
Current belief: {}\n\n\
New raw facts:\n{facts_block}\n",
        current_belief.unwrap_or("(none yet)")
    )
}

/// Which of `raw_rows` haven't already been folded into `existing`'s
/// `source_memories` lineage -- split out as a pure function so the "what's
/// new" decision is unit-testable without a database.
fn new_facts<'a>(raw_ids: &'a [(String, String)], already_consolidated: &HashSet<String>) -> Vec<&'a (String, String)> {
    raw_ids.iter().filter(|(id, _)| !already_consolidated.contains(id)).collect()
}

async fn call_llm(
    db: &Db,
    settings: &Settings,
    owner: &RecordId,
    mission: &str,
    current_belief: Option<&str>,
    facts: &[String],
) -> AppResult<String> {
    let (base_url, api_key) = resolve_openai(db, settings, owner).await?;
    let key = if api_key.is_empty() { "not-needed".to_string() } else { api_key };

    let body = json!({
        "model": "gpt-4o-mini",
        "response_format": {"type": "json_object"},
        "messages": [{"role": "user", "content": build_prompt(mission, current_belief, facts)}],
    });

    let resp = reqwest::Client::new()
        .post(format!("{}/chat/completions", base_url.trim_end_matches('/')))
        .bearer_auth(key)
        .json(&body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::internal(e.to_string()))?;

    let v: Value = resp.json().await.map_err(|e| AppError::internal(e.to_string()))?;
    let content = v
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .ok_or_else(|| AppError::internal("OpenAI response missing choices[0].message.content"))?;
    let parsed: Value =
        serde_json::from_str(content).map_err(|e| AppError::internal(format!("invalid consolidation JSON: {e}")))?;
    parsed
        .get("belief")
        .and_then(|b| b.as_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::internal("consolidation response missing 'belief'"))
}

/// The configured observations mission for `owner`, or `DEFAULT_MISSION` if
/// unset -- mirrors `tools.py::consolidate_observations` reading
/// `app_settings.observations_mission`.
pub async fn observations_mission(db: &Db, owner: &RecordId) -> AppResult<String> {
    let row = app_settings_row(db, owner).await?;
    Ok(if row.observations_mission.is_empty() { DEFAULT_MISSION.to_string() } else { row.observations_mission })
}

/// Consolidate one entity's raw facts into its observation row.
///
/// Returns the updated/created observation, or `None` if there was nothing
/// to do (no new raw facts beyond what's already consolidated) or the LLM
/// call failed / isn't available (stub backend).
#[allow(clippy::result_large_err)] // surrealdb::Error is large; boxing it would change the error type
pub async fn consolidate_subject(
    db: &Db,
    settings: &Settings,
    owner: &RecordId,
    subject_id: &RecordId,
    mission: Option<&str>,
) -> AppResult<Option<ConsolidatedObservation>> {
    let subject_row: Option<SubjectRow> = db.select(subject_id.clone()).await?;
    let Some(subject_row) = subject_row else { return Ok(None) };

    let mut raw_res = q::RAW_MEMORIES
        .on(db)
        .bind(("id", subject_id.clone()))
        .await?;
    let raw_rows: Vec<RawMemoryRow> = raw_res.take(0)?;

    let mut existing_res = q::OBSERVATION_OF
        .on(db)
        .bind(("id", subject_id.clone()))
        .await?;
    let existing_rows: Vec<ObservationRow> = existing_res.take(0)?;
    let existing = existing_rows.into_iter().next();

    let already_consolidated: HashSet<String> = existing
        .as_ref()
        .and_then(|e| e.source_memories.as_ref())
        .map(|ids| ids.iter().map(|r| r.to_string()).collect())
        .unwrap_or_default();

    let raw_ids: Vec<(String, String)> = raw_rows.iter().map(|r| (r.id.to_string(), r.text.clone())).collect();
    let fresh = new_facts(&raw_ids, &already_consolidated);
    if fresh.is_empty() {
        return Ok(None);
    }

    if settings.embeddings_backend == "stub" {
        return Ok(None);
    }

    let mission = mission.map(str::to_string).unwrap_or(DEFAULT_MISSION.to_string());
    let current_belief = existing.as_ref().map(|e| e.text.clone());
    let fact_texts: Vec<String> = fresh.iter().map(|(_, text)| text.clone()).collect();

    let belief = match call_llm(db, settings, owner, &mission, current_belief.as_deref(), &fact_texts).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("consolidation LLM call failed for {}: {}", subject_id.to_string(), e.message);
            return Ok(None);
        }
    };

    let mut all_source_ids: Vec<String> = already_consolidated.into_iter().collect();
    all_source_ids.extend(fresh.iter().map(|(id, _)| id.clone()));
    let source_memories: Vec<RecordId> =
        all_source_ids.iter().map(|s| crate::rid::parse(s)).collect::<Result<Vec<_>, _>>().map_err(|_| AppError::internal("source memory id did not round-trip"))?;

    // The LLM call above is too slow to hold a transaction open, so the write
    // is optimistic: it only lands if the observation is still the one we read
    // (same version, or still absent). Otherwise someone else revised it
    // meanwhile; skip, and the next consolidation recomputes from the new state.
    let (stmt, proof_count) = match &existing {
        None => (&q::CONSOLIDATE_CREATE, fresh.len() as i64),
        Some(_) => (&q::CONSOLIDATE_UPDATE, all_source_ids.len() as i64),
    };
    let row = with_retry(|| async {
        let mut res = stmt
            .on(db)
            .bind(("owner", owner.clone()))
            .bind(("vault", subject_row.vault.clone()))
            .bind(("subject", subject_id.clone()))
            .bind(("obs_id", observation_rid(subject_id)))
            .bind(("id", existing.as_ref().map(|e| e.id.clone())))
            .bind(("expected_version", existing.as_ref().map(|e| e.version)))
            .bind(("text", belief.clone()))
            .bind(("proof_count", proof_count))
            .bind(("source_memories", source_memories.clone()))
            .await?
            .check()?;
        let rows: Vec<ObservationRow> = res.take(stmt.slot)?;
        Ok(rows.into_iter().next())
    })
    .await?;
    let Some(row) = row else { return Ok(None) };

    Ok(Some(ConsolidatedObservation { id: row.id.to_string(), text: row.text }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_facts_filters_out_already_consolidated_ids() {
        let raw = vec![
            ("memory:a".to_string(), "fact a".to_string()),
            ("memory:b".to_string(), "fact b".to_string()),
        ];
        let mut already = HashSet::new();
        already.insert("memory:a".to_string());
        let fresh = new_facts(&raw, &already);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].0, "memory:b");
    }

    #[test]
    fn new_facts_empty_when_everything_already_consolidated() {
        let raw = vec![("memory:a".to_string(), "fact a".to_string())];
        let mut already = HashSet::new();
        already.insert("memory:a".to_string());
        assert!(new_facts(&raw, &already).is_empty());
    }

    #[test]
    fn new_facts_all_new_when_nothing_consolidated_yet() {
        let raw = vec![("memory:a".to_string(), "fact a".to_string())];
        let fresh = new_facts(&raw, &HashSet::new());
        assert_eq!(fresh.len(), 1);
    }

    #[test]
    fn build_prompt_includes_mission_belief_and_facts() {
        let facts = vec!["likes tea".to_string(), "works remote".to_string()];
        let prompt = build_prompt(DEFAULT_MISSION, Some("Alice drinks coffee."), &facts);
        assert!(prompt.contains(DEFAULT_MISSION));
        assert!(prompt.contains("Alice drinks coffee."));
        assert!(prompt.contains("- likes tea"));
        assert!(prompt.contains("- works remote"));
        assert!(prompt.contains("\"belief\""));
    }

    #[test]
    fn build_prompt_defaults_missing_belief_to_none_yet() {
        let prompt = build_prompt(DEFAULT_MISSION, None, &[]);
        assert!(prompt.contains("(none yet)"));
    }

    #[test]
    fn default_mission_matches_python_default() {
        assert!(DEFAULT_MISSION.contains("stable facts about people and relationships"));
    }
}

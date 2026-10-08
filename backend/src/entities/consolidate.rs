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

use std::collections::HashSet;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::config::Settings;
use crate::db::Db;
use crate::error::{AppError, AppResult};

use crate::embeddings::provider;

use super::extract::app_settings_row;

pub const DEFAULT_MISSION: &str = "Observations are stable facts about people and relationships: preferences, skills, roles, \
recurring patterns, and how they change over time. Ignore ephemeral or one-off details.";

#[derive(Debug, Deserialize)]
struct SubjectRow {
    vault: RecordId,
}

#[derive(Debug, Deserialize)]
struct RawMemoryRow {
    id: RecordId,
    text: String,
}

#[derive(Debug, Deserialize)]
struct ObservationRow {
    id: RecordId,
    #[serde(default)]
    text: String,
    #[serde(default)]
    source_memories: Option<Vec<RecordId>>,
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
    let p = provider::resolve(db, settings, owner).await?;

    let body = json!({
        "model": "gpt-4o-mini",
        "response_format": {"type": "json_object"},
        "messages": [{"role": "user", "content": build_prompt(mission, current_belief, facts)}],
    });

    let resp = provider::client()
        .post(p.url("chat/completions"))
        .bearer_auth(p.bearer())
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
pub async fn consolidate_subject(
    db: &Db,
    settings: &Settings,
    owner: &RecordId,
    subject_id: &RecordId,
    mission: Option<&str>,
) -> AppResult<Option<ConsolidatedObservation>> {
    if !super::service::is_entity_id(subject_id) {
        return Ok(None);
    }
    let subject_row: Option<SubjectRow> = db.select(subject_id.clone()).await?;
    let Some(subject_row) = subject_row else { return Ok(None) };
    // Not a member: same as not found -- the facts must never reach the
    // caller's model, nor the observation land in someone else's vault.
    if !crate::vaults::service::accessible_vault_ids(db, owner).await?.contains(&subject_row.vault) {
        return Ok(None);
    }

    let mut raw_res = db
        .query(r#"SELECT * FROM memory WHERE subject = $id AND type IN ["world","experience"] ORDER BY created_at"#)
        .bind(("id", subject_id.clone()))
        .await?;
    let raw_rows: Vec<RawMemoryRow> = raw_res.take(0)?;

    let mut existing_res = db
        .query(r#"SELECT * FROM memory WHERE subject = $id AND type = "observation" LIMIT 1"#)
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
            tracing::warn!("consolidation LLM call failed for {}: {}", subject_id, e.message);
            return Ok(None);
        }
    };

    let mut all_source_ids: Vec<String> = already_consolidated.into_iter().collect();
    all_source_ids.extend(fresh.iter().map(|(id, _)| id.clone()));
    let source_memories: Vec<RecordId> =
        all_source_ids.iter().map(|s| s.parse()).collect::<Result<Vec<_>, _>>().map_err(|_| AppError::internal("source memory id did not round-trip"))?;

    let row = match &existing {
        None => {
            let mut res = db
                .query(
                    "CREATE memory SET owner = $owner, vault = $vault, subject = $subject, text = $text, \
                     type = \"observation\", version = 1, proof_count = $proof_count, status = \"fresh\", \
                     source_memories = $source_memories, updated_at = time::now() RETURN AFTER",
                )
                .bind(("owner", owner.clone()))
                .bind(("vault", subject_row.vault.clone()))
                .bind(("subject", subject_id.clone()))
                .bind(("text", belief))
                .bind(("proof_count", fresh.len() as i64))
                .bind(("source_memories", source_memories))
                .await?;
            let rows: Vec<ObservationRow> = res.take(0)?;
            rows.into_iter().next().ok_or_else(|| AppError::internal("observation insert returned no row"))?
        }
        Some(existing) => {
            let mut res = db
                .query(
                    "UPDATE $id SET text = $text, version = version + 1, proof_count = $proof_count, \
                     status = \"fresh\", source_memories = $source_memories, updated_at = time::now() RETURN AFTER",
                )
                .bind(("id", existing.id.clone()))
                .bind(("text", belief))
                .bind(("proof_count", all_source_ids.len() as i64))
                .bind(("source_memories", source_memories))
                .await?;
            let rows: Vec<ObservationRow> = res.take(0)?;
            rows.into_iter().next().ok_or_else(|| AppError::internal("observation update returned no row"))?
        }
    };

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

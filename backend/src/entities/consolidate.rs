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
//! Staleness: handled eagerly, not at read time -- `service::add_memory`,
//! `update_memory` and `delete_memory` set `status="stale"` on a subject's
//! observation the moment one of its raw facts is written, edited or deleted
//! (a deletion also drops the fact from the observation's lineage). A fresh
//! observation is extended with just the facts not yet in its lineage; a
//! stale one is rebuilt from scratch out of the subject's surviving facts --
//! the old belief is not carried forward, since it may rest on a fact that
//! has since been corrected or deleted. Recall leaves stale observations out.
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

use super::extract::{app_settings_row, resolve_openai};

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
    #[serde(default)]
    text: String,
    #[serde(default)]
    source_memories: Option<Vec<RecordId>>,
    #[serde(default)]
    status: Option<String>,
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

/// What to consolidate given the subject's current raw facts `raw` (id,
/// text) and its observation (`None` if there isn't one): `(facts to send,
/// current belief to extend, lineage to store)`, or `None` when nothing
/// changed. A stale observation is rebuilt from every surviving fact, without
/// its old belief; a fresh one is extended with just the facts not yet in
/// its lineage. Lineage only ever names facts that still exist.
fn plan<'a>(
    raw: &'a [(String, String)],
    existing: Option<(&'a str, &HashSet<String>, bool)>,
) -> Option<(Vec<&'a (String, String)>, Option<&'a str>, Vec<String>)> {
    let all_ids: Vec<String> = raw.iter().map(|(id, _)| id.clone()).collect();
    match existing {
        Some((belief, seen, false)) => {
            let fresh: Vec<_> = raw.iter().filter(|(id, _)| !seen.contains(id)).collect();
            (!fresh.is_empty()).then_some((fresh, Some(belief), all_ids))
        }
        _ if raw.is_empty() => None,
        _ => Some((raw.iter().collect(), None, all_ids)),
    }
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

    let mut res = db
        .query(
            r#"SELECT id, text FROM memory WHERE subject = $id AND type IN ["world","experience"] ORDER BY created_at, id;
               SELECT text, source_memories, status FROM memory WHERE subject = $id AND type = "observation" LIMIT 1"#,
        )
        .bind(("id", subject_id.clone()))
        .await?;
    let raw_rows: Vec<RawMemoryRow> = res.take(0)?;
    let existing: Option<ObservationRow> = res.take::<Vec<ObservationRow>>(1)?.into_iter().next();

    let seen: HashSet<String> = existing
        .as_ref()
        .and_then(|e| e.source_memories.as_ref())
        .map(|ids| ids.iter().map(|r| r.to_string()).collect())
        .unwrap_or_default();
    let raw: Vec<(String, String)> = raw_rows.iter().map(|r| (r.id.to_string(), r.text.clone())).collect();
    let existing_plan = existing.as_ref().map(|e| (e.text.as_str(), &seen, e.status.as_deref() == Some("stale")));
    let Some((facts, current_belief, lineage)) = plan(&raw, existing_plan) else { return Ok(None) };

    if settings.embeddings_backend == "stub" {
        return Ok(None);
    }

    let mission = mission.map(str::to_string).unwrap_or(DEFAULT_MISSION.to_string());
    let fact_texts: Vec<String> = facts.iter().map(|(_, text)| text.clone()).collect();

    let belief = match call_llm(db, settings, owner, &mission, current_belief, &fact_texts).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("consolidation LLM call failed for {}: {}", subject_id, e.message);
            return Ok(None);
        }
    };

    let lineage: Vec<RecordId> = lineage
        .iter()
        .map(|s| s.parse())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::internal("source memory id did not round-trip"))?;
    let row = super::service::save_observation(db, owner, &subject_row.vault, subject_id, &belief, Some(lineage)).await?;

    Ok(Some(ConsolidatedObservation { id: row.id, text: row.text }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(ids: &[&str]) -> Vec<(String, String)> {
        ids.iter().map(|i| (format!("memory:{i}"), format!("fact {i}"))).collect()
    }

    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|i| format!("memory:{i}")).collect()
    }

    #[test]
    fn plan_without_an_observation_builds_from_every_fact() {
        let r = raw(&["a", "b"]);
        let (facts, belief, lineage) = plan(&r, None).unwrap();
        assert_eq!(facts.len(), 2);
        assert_eq!(belief, None);
        assert_eq!(lineage, vec!["memory:a", "memory:b"]);
        assert!(plan(&[], None).is_none());
    }

    #[test]
    fn plan_extends_a_fresh_observation_with_only_new_facts() {
        let seen = set(&["a"]);
        let r = raw(&["a", "b"]);
        let (facts, belief, lineage) = plan(&r, Some(("old", &seen, false))).unwrap();
        assert_eq!(facts.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), vec!["memory:b"]);
        assert_eq!(belief, Some("old"));
        assert_eq!(lineage, vec!["memory:a", "memory:b"]);
        assert!(plan(&raw(&["a"]), Some(("old", &seen, false))).is_none());
    }

    #[test]
    fn plan_rebuilds_a_stale_observation_from_surviving_facts_without_the_old_belief() {
        // "a" was edited (same id, so nothing looks new) and "x" was deleted
        let seen = set(&["a", "x"]);
        let r = raw(&["a"]);
        let (facts, belief, lineage) = plan(&r, Some(("lives in Paris", &seen, true))).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(belief, None);
        assert_eq!(lineage, vec!["memory:a"]);
        assert!(plan(&[], Some(("old", &seen, true))).is_none());
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

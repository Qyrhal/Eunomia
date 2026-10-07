//! The ingest pipeline: raw source record -> cache row.
//!
//! One entrypoint, [`ingest`], called by the scheduler, webhook endpoints, and
//! on-demand refresh. Stages, per record: map -> upsert (idempotent) ->
//! embed -> extract entities. Then, once per batch: consolidate observations
//! for every subject that got a new raw memory during the batch.
//!
//! Partial failure is isolated: a bad record is recorded and skipped, the
//! batch continues. Embedding failure is non-fatal (backfill retries).
//! Entity extraction and observation consolidation failures are likewise
//! non-fatal -- they're enrichment steps, not core pipeline.
//!
//! Deferred: entity extraction (`entities/extract.py::extract_entities`) and
//! observation consolidation (`entities/consolidate.py::consolidate_subject`)
//! aren't ported to Rust in this pass -- there's no `entities::extract` /
//! `entities::consolidate` module to call into yet. Both stages are wired up
//! as no-ops below (clearly marked) rather than blocked on; the write/embed
//! core of the pipeline is fully functional without them, same as the Python
//! version when those stages raise (non-fatal by design either way).
//!
//! Ported from `cache/ingest.py`.

use serde::Serialize;
use serde_json::Value;
use surrealdb::RecordId;

use crate::cache::search::{self, Envelope};
use crate::config::Settings;
use crate::db::Db;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize)]
pub struct IngestReport {
    pub source: String,
    pub written: u64,
    pub skipped: u64,
    pub failed: u64,
    pub errors: Vec<String>,
}

impl IngestReport {
    fn new(source: &str) -> Self {
        IngestReport { source: source.to_string(), written: 0, skipped: 0, failed: 0, errors: Vec::new() }
    }

    /// Mirrors `cache/ingest.py::IngestReport.as_dict` -- truncates `errors`
    /// to the first 20 for the response payload.
    pub fn as_dict(&self) -> Value {
        serde_json::json!({
            "source": self.source,
            "written": self.written,
            "skipped": self.skipped,
            "failed": self.failed,
            "errors": self.errors.iter().take(20).collect::<Vec<_>>(),
        })
    }
}

async fn embed_record(db: &Db, settings: &Settings, owner: &RecordId, rec: &search::CacheRecord) -> AppResult<()> {
    let text = format!("{}\n{}", rec.title, rec.body_text).trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    let vec = crate::embeddings::service::embed(db, settings, &[text], Some(owner))
        .await?
        .into_iter()
        .next()
        .unwrap_or_default();
    search::set_embedding(db, owner, &rec.id, vec).await
}

/// Deferred -- see module docstring. `entities::extract::extract_entities`
/// hasn't been ported, so this always reports no touched subjects.
fn extract_record_entities(_owner: &RecordId, _rec: &search::CacheRecord) -> Vec<String> {
    Vec::new()
}

/// Deferred -- see module docstring. `entities::consolidate::consolidate_subject`
/// hasn't been ported; `extract_record_entities` never returns any touched
/// subjects yet, so this is currently always a no-op, but kept as the
/// pipeline's named stage so wiring the real consolidation in later is a
/// one-function change.
async fn consolidate_touched_subjects(_owner: &RecordId, _subject_ids: &[String]) {}

/// `raw_records` are JSON values (mirrors Python's untyped `dict` raw
/// records); `map_fn` maps one raw record to `Some(Envelope)`, `None` to
/// skip it, or `Err(message)` on a mapping failure (mirrors a raised
/// exception from the Python mapper).
pub async fn ingest(
    db: &Db,
    settings: &Settings,
    owner: &RecordId,
    source_key: &str,
    raw_records: &[Value],
    map_fn: impl Fn(&Value) -> Result<Option<Envelope>, String>,
) -> AppResult<IngestReport> {
    let mut report = IngestReport::new(source_key);
    let mut touched_subjects: Vec<String> = Vec::new();

    for raw in raw_records {
        let env = match map_fn(raw) {
            Ok(Some(mut env)) => {
                if env.source.is_empty() {
                    env.source = source_key.to_string();
                }
                env
            }
            Ok(None) => {
                report.skipped += 1;
                continue;
            }
            Err(e) => {
                report.failed += 1;
                report.errors.push(format!("{}: {e}", raw_id(raw)));
                continue;
            }
        };

        match search::upsert(db, owner, &env).await {
            Ok((rec, changed)) => {
                if !changed {
                    report.skipped += 1;
                    continue;
                }
                report.written += 1;

                if !rec.deleted {
                    // No OpenAI key: skip embedding, keyword/graph retrieval still work.
                    if crate::embeddings::service::available(db, settings, owner).await {
                        if let Err(e) = embed_record(db, settings, owner, &rec).await {
                            report.errors.push(format!("embed {}: {}", rec.id, e.message));
                        }
                    }
                    touched_subjects.extend(extract_record_entities(owner, &rec));
                }
            }
            Err(e) => {
                report.failed += 1;
                report.errors.push(format!("{}: {}", env.id, e.message));
            }
        }
    }

    consolidate_touched_subjects(owner, &touched_subjects).await;
    Ok(report)
}

fn raw_id(raw: &Value) -> String {
    raw.get("id").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_else(|| "?".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_dict_truncates_errors_to_20() {
        let mut report = IngestReport::new("demo");
        for i in 0..30 {
            report.errors.push(format!("err {i}"));
        }
        let d = report.as_dict();
        assert_eq!(d["errors"].as_array().unwrap().len(), 20);
    }

    #[test]
    fn as_dict_reports_counters() {
        let mut report = IngestReport::new("demo");
        report.written = 3;
        report.skipped = 1;
        report.failed = 2;
        let d = report.as_dict();
        assert_eq!(d["source"], "demo");
        assert_eq!(d["written"], 3);
        assert_eq!(d["skipped"], 1);
        assert_eq!(d["failed"], 2);
    }

    #[test]
    fn raw_id_falls_back_to_question_mark() {
        assert_eq!(raw_id(&serde_json::json!({"no_id": true})), "?");
        assert_eq!(raw_id(&serde_json::json!({"id": "abc"})), "abc");
    }
}

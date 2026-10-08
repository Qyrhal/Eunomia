//! Admin and REST-only reads that sit beside the agent tools.

use serde_json::{json, Value};
use surrealdb::types::{RecordId, SurrealValue};

use crate::error::AppResult;
use crate::pool::OrgDb;
use crate::rid::RecordIdExt;
use crate::store;

/// The owner's own audit log, newest first -- backs `GET /api/audit` (an
/// admin/REST concern, not an agent-facing tool; an agent auditing its own
/// writes isn't a real use case this codebase needs yet).
pub async fn list_audit(db: &OrgDb, owner: &RecordId, limit: i64, offset: i64) -> AppResult<Value> {
    #[derive(serde::Deserialize, SurrealValue)]
    struct AuditRow {
        id: RecordId,
        tool_name: String,
        args_summary: String,
        outcome: String,
        created_at: Option<surrealdb::types::Datetime>,
    }
    #[derive(serde::Deserialize, SurrealValue)]
    struct CountRow {
        count: i64,
    }

    let mut res = store::cache::AUDIT_PAGE
        .on(db)
        .bind(("owner", owner.clone()))
        .bind(("limit", limit))
        .bind(("offset", offset))
        .await?;
    let rows: Vec<AuditRow> = res.take(0)?;

    let mut tres = store::cache::AUDIT_COUNT.on(db).bind(("owner", owner.clone())).await?;
    let total_rows: Vec<CountRow> = tres.take(0)?;
    let total = total_rows.first().map(|r| r.count).unwrap_or(0);

    let results: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id.to_string(),
                "tool_name": r.tool_name,
                "args_summary": r.args_summary,
                "outcome": r.outcome,
                "created_at": &r.created_at,
            })
        })
        .collect();
    let has_more = offset + (results.len() as i64) < total;
    Ok(json!({ "results": results, "total": total, "has_more": has_more }))
}

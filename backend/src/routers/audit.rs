//! Audit-log REST surface: the owner's own mutating-tool-call history.
//! Ported from `app/routers/audit.py` (which delegates to
//! `tools/registry.py::list_audit`); since `tools/registry.py` hasn't been
//! ported to Rust, the query is written directly here against the
//! `audit_log` table.

use axum::{extract::{Query, State}, routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::{Datetime, RecordId};

use crate::error::AppResult;
use crate::models_user::User;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/audit", get(get_audit))
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize)]
struct AuditQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

#[derive(Debug, Deserialize)]
struct AuditRow {
    id: RecordId,
    tool_name: String,
    #[serde(default)]
    args_summary: String,
    outcome: String,
    created_at: Option<Datetime>,
}

#[derive(Debug, Deserialize)]
struct CountRow {
    count: i64,
}

/// Pure pagination check, factored out for testing: are there more rows
/// beyond what this page already returned?
fn has_more(offset: i64, returned: usize, total: i64) -> bool {
    offset + (returned as i64) < total
}

async fn get_audit(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<AuditQuery>,
) -> AppResult<Json<Value>> {
    let mut res = state
        .db
        .query("SELECT * FROM audit_log WHERE owner = $owner ORDER BY created_at DESC LIMIT $limit START $offset")
        .bind(("owner", user.id.clone()))
        .bind(("limit", q.limit))
        .bind(("offset", q.offset))
        .await?;
    let rows: Vec<AuditRow> = res.take(0)?;

    let mut count_res = state
        .db
        .query("SELECT count() FROM audit_log WHERE owner = $owner GROUP ALL")
        .bind(("owner", user.id.clone()))
        .await?;
    let counts: Vec<CountRow> = count_res.take(0)?;
    let total = counts.first().map(|c| c.count).unwrap_or(0);

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

    Ok(Json(json!({
        "results": results,
        "total": total,
        "has_more": has_more(q.offset, results.len(), total),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_is_fifty() {
        assert_eq!(default_limit(), 50);
    }

    #[test]
    fn has_more_true_when_page_does_not_cover_total() {
        assert!(has_more(0, 50, 120));
    }

    #[test]
    fn has_more_false_when_page_reaches_total() {
        assert!(!has_more(50, 50, 100));
        assert!(!has_more(0, 10, 10));
    }

    #[test]
    fn has_more_false_when_page_overshoots_total() {
        assert!(!has_more(90, 50, 100));
    }
}

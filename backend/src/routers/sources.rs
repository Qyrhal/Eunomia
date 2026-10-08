//! Sources dashboard data: the registered sources, each one's sync health,
//! and whether the caller has an enabled connector backing it. Ported from
//! `app/routers/sources.py`.
//!
//! Also the webhook intake route ([`webhook_router`]) -- deliberately a
//! *separate* router with no `User` extractor, since an external provider
//! can't send our session cookie or bearer token. Owner is resolved from the
//! URL itself (`{owner_id}`, a `user:...` record id) and the request is only
//! trusted once `Source::webhook()` verifies it against that owner's stored
//! connector credentials -- mirrors the Python module's `webhook_router` and
//! its docstring on why the owner id lives in the URL. Matches
//! `routers::connectors`'s dual-router pattern (`router()` + a second
//! function the caller mounts separately).

use surrealdb::types::SurrealValue;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use surrealdb::types::RecordId;
use crate::rid::RecordIdExt;

use crate::error::{AppError, AppResult, ErrorCode};
use crate::models_user::User;
use crate::sources::registry;
use crate::sources::scheduler::sync_source;
use crate::state::{AppState, OrgState};
use crate::store;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sources", get(list_sources))
        .route("/sources/status", get(sources_status))
        .route("/sources/{key}/sync", post(sync_now))
}

/// No `User` extractor: see this module's doc comment.
pub fn webhook_router() -> Router<AppState> {
    Router::new().route("/sources/{key}/webhook/{owner_id}", post(source_webhook))
}

#[derive(Debug, Deserialize, SurrealValue)]
struct SyncStatusRow {
    id: RecordId,
    #[serde(default)]
    #[surreal(default)]
    cursor: String,
    #[serde(default)]
    #[surreal(default)]
    last_run: Option<surrealdb::types::Datetime>,
    #[serde(default)]
    #[surreal(default)]
    last_ok: Option<surrealdb::types::Datetime>,
    #[serde(default)]
    #[surreal(default)]
    last_error: String,
    #[serde(default)]
    #[surreal(default)]
    consecutive_failures: i64,
}

async fn sync_status_rows(state: &OrgState, owner: &RecordId) -> AppResult<std::collections::HashMap<String, SyncStatusRow>> {
    let mut res = store::app::SOURCES_SYNC_STATUS_LIST
        .on(&state.db)
        .bind(("owner", owner.clone()))
        .await?;
    let rows: Vec<SyncStatusRow> = res.take(0)?;
    Ok(rows
        .into_iter()
        .map(|r| {
            // The record key is `{owner_key}:{source_key}`; partition off the
            // owner-key prefix to recover the source key, same as the Python
            // router's `row["id"].id.partition(":")`.
            let raw_key = crate::rid::key_string(r.id.key()).unwrap_or_default();
            let source_key = raw_key.split_once(':').map(|(_, k)| k.to_string()).unwrap_or(raw_key);
            (source_key, r)
        })
        .collect())
}

/// Cached-record count per source key, for the dashboard's totals -- a single
/// grouped count, not a per-source query.
async fn record_counts(state: &OrgState, owner: &RecordId) -> AppResult<std::collections::HashMap<String, i64>> {
    #[derive(Deserialize, SurrealValue)]
    struct Row {
        source: String,
        count: i64,
    }
    let mut res = store::app::SOURCES_RECORD_COUNTS
        .on(&state.db)
        .bind(("owner", owner.clone()))
        .await?;
    let rows: Vec<Row> = res.take(0)?;
    Ok(rows.into_iter().map(|r| (r.source, r.count)).collect())
}

#[derive(Serialize, utoipa::ToSchema)]
struct SyncStatusOut {
    cursor: String,
    #[schema(value_type = Option<String>)]
    last_run: Option<surrealdb::types::Datetime>,
    #[schema(value_type = Option<String>)]
    last_ok: Option<surrealdb::types::Datetime>,
    last_error: String,
    consecutive_failures: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
struct SourceOut {
    key: &'static str,
    label: &'static str,
    provider: &'static str,
    record_types: &'static [&'static str],
    connected: bool,
    sync_status: SyncStatusOut,
    record_count: i64,
}

/// Outcome of a sync or webhook ingest: an ingest report, or `source` + `error`
/// when the sync failed. Schema only: the handlers pass the report through.
#[derive(Serialize, utoipa::ToSchema)]
#[allow(dead_code)]
struct SyncReport {
    source: String,
    written: Option<i64>,
    skipped: Option<i64>,
    failed: Option<i64>,
    errors: Option<Vec<String>>,
    error: Option<String>,
}

/// Body of the webhook route: `{"status": "ignored"}` or an ingest report with
/// `status: "ok"`. Schema only.
#[derive(Serialize, utoipa::ToSchema)]
#[allow(dead_code)]
struct WebhookOut {
    status: String,
    source: Option<String>,
    written: Option<i64>,
    skipped: Option<i64>,
    failed: Option<i64>,
    errors: Option<Vec<String>>,
}

fn status_out(row: Option<&SyncStatusRow>) -> SyncStatusOut {
    match row {
        None => SyncStatusOut { cursor: String::new(), last_run: None, last_ok: None, last_error: String::new(), consecutive_failures: 0 },
        Some(row) => SyncStatusOut {
            cursor: row.cursor.clone(),
            last_run: row.last_run,
            last_ok: row.last_ok,
            last_error: row.last_error.clone(),
            consecutive_failures: row.consecutive_failures,
        },
    }
}

#[utoipa::path(
    operation_id = "listSources",
    get,
    path = "/api/sources",
    tag = "sources",
    summary = "List sources with sync status",
    responses((status = 200, body = Vec<SourceOut>), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn list_sources(State(state): State<AppState>, user: User) -> AppResult<Json<Vec<SourceOut>>> {
    let state = state.org(&user.org).await?;
    let statuses = sync_status_rows(&state, &user.id).await?;
    let enabled_keys: std::collections::HashSet<&'static str> =
        registry::enabled(&state.db, &user.id).await?.iter().map(|s| s.key()).collect();
    let counts = record_counts(&state, &user.id).await?;

    Ok(Json(
        registry::all()
            .iter()
            .map(|src| {
                SourceOut {
                    key: src.key(),
                    label: src.label(),
                    provider: src.provider_key(),
                    record_types: src.record_types(),
                    connected: enabled_keys.contains(src.key()),
                    sync_status: status_out(statuses.get(src.key())),
                    record_count: counts.get(src.key()).copied().unwrap_or(0),
                }
            })
            .collect(),
    ))
}

#[utoipa::path(
    operation_id = "getSourcesStatus",
    get,
    path = "/api/sources/status",
    tag = "sources",
    summary = "Overall sync status",
    responses((status = 200, body = std::collections::HashMap<String, SyncStatusOut>), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn sources_status(State(state): State<AppState>, user: User) -> AppResult<Json<std::collections::HashMap<String, SyncStatusOut>>> {
    let state = state.org(&user.org).await?;
    let statuses = sync_status_rows(&state, &user.id).await?;
    Ok(Json(statuses.iter().map(|(k, v)| (k.clone(), status_out(Some(v)))).collect()))
}

#[utoipa::path(
    operation_id = "syncSource",
    post,
    path = "/api/sources/{key}/sync",
    tag = "sources",
    summary = "Sync a source now",
    params(("key" = String, Path)),
    responses((status = 200, body = SyncReport), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn sync_now(State(state): State<AppState>, user: User, Path(key): Path<String>) -> AppResult<Json<Value>> {
    let state = state.org(&user.org).await?;
    if registry::get(&key).is_none() {
        return Err(AppError::coded(ErrorCode::SourceNotFound, format!("no source {key:?}")));
    }
    let report = sync_source(&state, &user.id, &key, "poll").await;
    Ok(Json(report))
}

#[utoipa::path(
    operation_id = "receiveSourceWebhook",
    post,
    path = "/api/sources/{key}/webhook/{owner_id}",
    tag = "sources",
    summary = "Source webhook (authenticated by the source's own signature)",
    params(("key" = String, Path), ("owner_id" = String, Path)),
    responses((status = 200, body = WebhookOut), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(()),
)]
async fn source_webhook(
    State(state): State<AppState>,
    Path((key, owner_id)): Path<(String, String)>,
    request: axum::extract::Request,
) -> AppResult<Json<Value>> {
    let Some(src) = registry::get(&key) else {
        return Err(AppError::coded(ErrorCode::SourceNotFound, format!("no source {key:?}")));
    };

    let owner: RecordId = crate::rid::parse(&owner_id).map_err(|_| AppError::not_found("unknown owner"))?;
    // the owner's org decides whose database the delivery lands in; an unknown owner has none
    let org = crate::models_user::org_of(&state.control, &owner).await.map_err(|_| AppError::not_found("unknown owner"))?;
    let state = state.org(&org).await?;

    let (parts, body) = request.into_parts();
    let body_bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?;

    let ctx = registry::ctx(&state.db, &state.settings.encryption_key, &owner);
    let raw_records = src.webhook(&ctx, &parts.headers, &body_bytes).await?;

    let Some(raw_records) = raw_records.filter(|r| !r.is_empty()) else {
        // Covers both "signature didn't verify" and "nothing worth
        // ingesting" -- `Source::webhook` doesn't distinguish the two (see
        // its doc comment), and responding 200 either way avoids the
        // provider retrying a delivery it has no reason to believe failed.
        tracing::info!(source = %key, owner = %owner.to_string(), "webhook: ignored");
        return Ok(Json(json!({"status": "ignored"})));
    };

    let report = registry::ingest(&state, &owner, &key, &raw_records, src.as_ref()).await;
    tracing::info!(source = %key, owner = %owner.to_string(), report = ?report.as_value(), "webhook: processed");

    let mut out = report.as_value();
    out.as_object_mut().unwrap().insert("status".to_string(), json!("ok"));
    Ok(Json(out))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(
    list_sources,
    sources_status,
    sync_now,
    source_webhook,
))]
pub struct Doc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_out_defaults_for_a_missing_row() {
        let v = serde_json::to_value(status_out(None)).unwrap();
        assert_eq!(v["cursor"], "");
        assert_eq!(v["consecutive_failures"], 0);
        assert!(v["last_run"].is_null());
    }
}

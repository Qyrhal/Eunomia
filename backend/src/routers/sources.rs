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

use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::error::{AppError, AppResult};
use crate::models_user::User;
use crate::sources::registry;
use crate::sources::scheduler::sync_source;
use crate::state::AppState;

/// Provider deliveries are a few KiB; anything bigger is rejected unread.
const MAX_WEBHOOK_BYTES: usize = 1 << 20;
/// A slow or stalled upload is cut off rather than holding a connection.
const WEBHOOK_READ_TIMEOUT: Duration = Duration::from_secs(10);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sources", get(list_sources))
        .route("/sources/status", get(sources_status))
        .route("/sources/:key/sync", post(sync_now))
}

/// No `User` extractor: see this module's doc comment.
pub fn webhook_router() -> Router<AppState> {
    Router::new().route("/sources/:key/webhook/:owner_id", post(source_webhook))
}

#[derive(Debug, Deserialize)]
struct SyncStatusRow {
    id: RecordId,
    #[serde(default)]
    cursor: String,
    #[serde(default)]
    last_run: Option<surrealdb::Datetime>,
    #[serde(default)]
    last_ok: Option<surrealdb::Datetime>,
    #[serde(default)]
    last_error: String,
    #[serde(default)]
    consecutive_failures: i64,
}

async fn sync_status_rows(state: &AppState, owner: &RecordId) -> AppResult<std::collections::HashMap<String, SyncStatusRow>> {
    let mut res = state
        .db
        .query("SELECT * FROM sync_status WHERE owner = $owner")
        .bind(("owner", owner.clone()))
        .await?;
    let rows: Vec<SyncStatusRow> = res.take(0)?;
    Ok(rows
        .into_iter()
        .map(|r| {
            // The record key is `{owner_key}:{source_key}`; partition off the
            // owner-key prefix to recover the source key, same as the Python
            // router's `row["id"].id.partition(":")`.
            let raw_key: String = r.id.key().clone().try_into().unwrap_or_default();
            let source_key = raw_key.split_once(':').map(|(_, k)| k.to_string()).unwrap_or(raw_key);
            (source_key, r)
        })
        .collect())
}

/// Cached-record count per source key, for the dashboard's totals -- a single
/// grouped count, not a per-source query.
async fn record_counts(state: &AppState, owner: &RecordId) -> AppResult<std::collections::HashMap<String, i64>> {
    #[derive(Deserialize)]
    struct Row {
        source: String,
        count: i64,
    }
    let mut res = state
        .db
        .query("SELECT source, count() AS count FROM cache_record WHERE owner = $owner AND deleted = false GROUP BY source")
        .bind(("owner", owner.clone()))
        .await?;
    let rows: Vec<Row> = res.take(0)?;
    Ok(rows.into_iter().map(|r| (r.source, r.count)).collect())
}

fn status_out(row: Option<&SyncStatusRow>) -> Value {
    match row {
        None => json!({"cursor": "", "last_run": Value::Null, "last_ok": Value::Null, "last_error": "", "consecutive_failures": 0}),
        Some(row) => json!({
            "cursor": row.cursor,
            "last_run": row.last_run,
            "last_ok": row.last_ok,
            "last_error": row.last_error,
            "consecutive_failures": row.consecutive_failures,
        }),
    }
}

async fn list_sources(State(state): State<AppState>, user: User) -> AppResult<Json<Vec<Value>>> {
    let statuses = sync_status_rows(&state, &user.id).await?;
    let enabled_keys: std::collections::HashSet<&'static str> =
        registry::enabled(&state.db, &user.id).await?.iter().map(|s| s.key()).collect();
    let counts = record_counts(&state, &user.id).await?;

    Ok(Json(
        registry::all()
            .iter()
            .map(|src| {
                json!({
                    "key": src.key(),
                    "label": src.label(),
                    "provider": src.provider_key(),
                    "record_types": src.record_types(),
                    "connected": enabled_keys.contains(src.key()),
                    "sync_status": status_out(statuses.get(src.key())),
                    "record_count": counts.get(src.key()).copied().unwrap_or(0),
                })
            })
            .collect(),
    ))
}

async fn sources_status(State(state): State<AppState>, user: User) -> AppResult<Json<Value>> {
    let statuses = sync_status_rows(&state, &user.id).await?;
    let out: serde_json::Map<String, Value> = statuses.iter().map(|(k, v)| (k.clone(), status_out(Some(v)))).collect();
    Ok(Json(Value::Object(out)))
}

async fn sync_now(State(state): State<AppState>, user: User, Path(key): Path<String>) -> AppResult<Json<Value>> {
    if registry::get(&key).is_none() {
        return Err(AppError::not_found(format!("no source {key:?}")));
    }
    let report = sync_source(&state.db, &state.settings, &user.id, &key).await;
    Ok(Json(report))
}

async fn source_webhook(
    State(state): State<AppState>,
    Path((key, owner_id)): Path<(String, String)>,
    request: axum::extract::Request,
) -> AppResult<Json<Value>> {
    let Some(src) = registry::get(&key) else {
        return Err(AppError::not_found(format!("no source {key:?}")));
    };

    let owner: RecordId = owner_id.parse().map_err(|_| AppError::not_found("unknown owner"))?;

    // Bounded before any database or signature work: this route is public.
    let (parts, body) = request.into_parts();
    let body_bytes = match tokio::time::timeout(WEBHOOK_READ_TIMEOUT, axum::body::to_bytes(body, MAX_WEBHOOK_BYTES)).await {
        Err(_) => return Err(AppError::new(StatusCode::REQUEST_TIMEOUT, "webhook body not received in time")),
        Ok(Err(_)) => return Err(AppError::new(StatusCode::PAYLOAD_TOO_LARGE, "webhook body over 1 MiB")),
        Ok(Ok(bytes)) => bytes,
    };

    let conn = registry::conn_for(&state.db, &state.settings.encryption_key, &owner, src.as_ref()).await?;
    let raw_records = src.webhook(&conn, &parts.headers, &body_bytes).await?;

    let Some(raw_records) = raw_records.filter(|r| !r.is_empty()) else {
        // Covers both "signature didn't verify" and "nothing worth
        // ingesting" -- `Source::webhook` doesn't distinguish the two (see
        // its doc comment), and responding 200 either way avoids the
        // provider retrying a delivery it has no reason to believe failed.
        tracing::info!(source = %key, owner = %owner, "webhook: ignored");
        return Ok(Json(json!({"status": "ignored"})));
    };

    let report = registry::ingest(&state.db, &state.settings, &owner, &raw_records, src.as_ref()).await?;
    tracing::info!(source = %key, owner = %owner, report = ?report.as_dict(), "webhook: processed");
    if report.failed > 0 {
        // Not stored: a 5xx makes the provider redeliver.
        return Err(AppError::new(StatusCode::SERVICE_UNAVAILABLE, format!("{} records failed to save; retry", report.failed)));
    }

    let mut out = report.as_dict();
    out.as_object_mut().unwrap().insert("status".to_string(), json!("ok"));
    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    /// A state whose database is never connected: these requests must be
    /// rejected before anything touches it.
    fn offline_state() -> AppState {
        let db = surrealdb::Surreal::<surrealdb::engine::remote::ws::Client>::init();
        AppState(std::sync::Arc::new(crate::state::AppStateInner { db, settings: crate::config::Settings::load() }))
    }

    fn webhook(body: Body) -> axum::http::Request<Body> {
        axum::http::Request::post("/sources/up_bank/webhook/user:abc").body(body).unwrap()
    }

    #[tokio::test]
    async fn an_oversized_webhook_is_rejected_unread() {
        let app = webhook_router().with_state(offline_state());
        let resp = app.oneshot(webhook(Body::from(vec![b'x'; MAX_WEBHOOK_BYTES + 1]))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test(start_paused = true)]
    async fn a_stalled_webhook_upload_times_out() {
        let app = webhook_router().with_state(offline_state());
        let stalled = Body::from_stream(futures::stream::pending::<Result<Vec<u8>, std::io::Error>>());
        let resp = app.oneshot(webhook(stalled)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::REQUEST_TIMEOUT);
    }

    #[test]
    fn status_out_defaults_for_a_missing_row() {
        let v = status_out(None);
        assert_eq!(v["cursor"], "");
        assert_eq!(v["consecutive_failures"], 0);
        assert!(v["last_run"].is_null());
    }
}

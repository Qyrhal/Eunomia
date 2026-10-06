//! Connector CRUD + per-connector data endpoints (finance summary, PocketAI
//! recordings) + the cross-connector dashboard snapshot. Ported from the old
//! Django/FastAPI `app/routers/connectors.py`, owner-scoped, no
//! Google/Twenty/demo-mode.
//!
//! The Python module exposes two routers mounted separately by `main.py`:
//! `router` (prefixed `/connectors`) and `snapshot_router` (mounted bare, so
//! its one route lands at `/snapshot`). This file mirrors that split as
//! [`router`] and [`snapshot_router`] -- the caller is responsible for
//! merging both into the app (matching main.py's `.include_router` calls).

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{Datelike, Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::connectors::clients::{OpenConnectorClient, PocketAIClient, UpBankClient};
use crate::connectors::service::{self, Connector, CONNECTOR_KINDS};
use crate::error::{AppError, AppResult};
use crate::models_user::User;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/connectors", get(list_all))
        .route("/connectors/up_bank/finance-summary", get(up_bank_finance_summary))
        .route("/connectors/pocketai/summary", get(pocketai_summary))
        .route("/connectors/pocketai/all", get(pocketai_all))
        .route("/connectors/pocketai/search", get(pocketai_search))
        .route("/connectors/pocketai/detail/:recording_id", get(pocketai_detail))
        .route("/connectors/:kind", get(get_one).put(put_one))
        .route("/connectors/:kind/test", axum::routing::post(test_one))
}

pub fn snapshot_router() -> Router<AppState> {
    Router::new().route("/snapshot", get(snapshot))
}

fn connector_out(row: &Connector) -> Value {
    json!({
        "kind": row.kind,
        "enabled": row.enabled,
        "config": row.config,
        "credentials_set": !row.credentials_encrypted.is_empty(),
        "updated_at": row.updated_at,
    })
}

enum AnyClient {
    UpBank(UpBankClient),
    PocketAI(PocketAIClient),
    OpenConnector(OpenConnectorClient),
}

impl AnyClient {
    async fn ping(&self) -> AppResult<bool> {
        match self {
            AnyClient::UpBank(c) => c.ping().await,
            AnyClient::PocketAI(c) => c.ping().await,
            AnyClient::OpenConnector(c) => c.ping().await,
        }
    }
}

fn client_for(kind: &str, config: &Value, credentials: &Value) -> Option<AnyClient> {
    let base_url = config.get("base_url").and_then(|v| v.as_str());
    match kind {
        "up_bank" => Some(AnyClient::UpBank(UpBankClient::new(credentials))),
        "pocketai" => Some(AnyClient::PocketAI(PocketAIClient::new(credentials, base_url))),
        "open_connector" => Some(AnyClient::OpenConnector(OpenConnectorClient::new(credentials, base_url))),
        _ => None,
    }
}

fn require_known_kind(kind: &str) -> AppResult<()> {
    if CONNECTOR_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(AppError::not_found("unknown connector"))
    }
}

#[derive(Deserialize)]
struct ConnectorUpdateBody {
    enabled: Option<bool>,
    config: Option<Value>,
    credentials: Option<Value>,
}

#[derive(Deserialize)]
struct DaysQuery {
    #[serde(default = "default_days")]
    days: i64,
}

fn default_days() -> i64 {
    30
}

#[derive(Deserialize)]
struct LimitQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Deserialize)]
struct SearchQuery {
    query: String,
}

async fn list_all(State(state): State<AppState>, user: User) -> AppResult<Json<Vec<Value>>> {
    let rows = service::list_connectors(&state.db, &user.id).await?;
    Ok(Json(rows.iter().map(connector_out).collect()))
}

async fn up_bank_finance_summary(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Value>> {
    let row = service::get_or_create_connector(&state.db, &user.id, "up_bank").await?;
    let creds = service::credentials_for(&state.db, &state.settings.encryption_key, &user.id, "up_bank").await?;
    let has_token = creds.get("personal_access_token").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
    if !row.enabled || !has_token {
        return Err(AppError::bad_request("Up Bank is not connected"));
    }
    let since = Utc::now() - Duration::days(q.days);
    let summary = UpBankClient::new(&creds).finance_summary(&since.to_rfc3339()).await?;
    Ok(Json(summary))
}

async fn pocketai_client_or_400(state: &AppState, user: &User) -> AppResult<PocketAIClient> {
    let row = service::get_or_create_connector(&state.db, &user.id, "pocketai").await?;
    let creds = service::credentials_for(&state.db, &state.settings.encryption_key, &user.id, "pocketai").await?;
    let has_key = creds.get("api_key").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
    if !row.enabled || !has_key {
        return Err(AppError::bad_request("PocketAI is not connected"));
    }
    let base_url = row.config.get("base_url").and_then(|v| v.as_str());
    Ok(PocketAIClient::new(&creds, base_url))
}

async fn pocketai_summary(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Value>> {
    let client = pocketai_client_or_400(&state, &user).await?;
    let since = (Utc::now() - Duration::days(q.days)).date_naive().to_string();
    Ok(Json(client.summary(&since).await?))
}

async fn pocketai_all(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<LimitQuery>,
) -> AppResult<Json<Value>> {
    let client = pocketai_client_or_400(&state, &user).await?;
    Ok(Json(client.recordings(&[("limit", q.limit.to_string())]).await?))
}

async fn pocketai_search(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    let client = pocketai_client_or_400(&state, &user).await?;
    Ok(Json(client.search(&q.query).await?))
}

async fn pocketai_detail(
    State(state): State<AppState>,
    user: User,
    Path(recording_id): Path<String>,
) -> AppResult<Json<Value>> {
    let client = pocketai_client_or_400(&state, &user).await?;
    Ok(Json(client.recording(&recording_id).await?))
}

async fn get_one(State(state): State<AppState>, user: User, Path(kind): Path<String>) -> AppResult<Json<Value>> {
    require_known_kind(&kind)?;
    let row = service::get_or_create_connector(&state.db, &user.id, &kind).await?;
    Ok(Json(connector_out(&row)))
}

async fn put_one(
    State(state): State<AppState>,
    user: User,
    Path(kind): Path<String>,
    Json(body): Json<ConnectorUpdateBody>,
) -> AppResult<Json<Value>> {
    require_known_kind(&kind)?;
    let row = service::upsert_connector(
        &state.db,
        &state.settings.encryption_key,
        &user.id,
        &kind,
        body.enabled,
        body.config,
        body.credentials,
    )
    .await?;
    Ok(Json(connector_out(&row)))
}

async fn test_one(State(state): State<AppState>, user: User, Path(kind): Path<String>) -> AppResult<Json<Value>> {
    require_known_kind(&kind)?;
    let row = service::get_or_create_connector(&state.db, &user.id, &kind).await?;
    let creds = service::credentials_for(&state.db, &state.settings.encryption_key, &user.id, &kind).await?;
    let client = client_for(&kind, &row.config, &creds).expect("kind already validated against CONNECTOR_KINDS");
    match client.ping().await {
        Ok(ok) => Ok(Json(json!({ "ok": ok }))),
        Err(err) => Ok(Json(json!({ "ok": false, "error": err.message }))),
    }
}

/// Best-effort figures for each connected account, read live from each
/// client. A connector that isn't connected, or whose call fails, comes back
/// as null rather than failing the whole request.
async fn snapshot(State(state): State<AppState>, user: User) -> AppResult<Json<Value>> {
    let mut result = json!({ "up_bank": Value::Null, "pocketai": Value::Null });

    let now = Utc::now();
    let week_start_date = now.date_naive() - Duration::days(now.weekday().num_days_from_monday() as i64);
    let week_start = week_start_date.and_hms_opt(0, 0, 0).expect("midnight is always valid").and_utc();

    let up_bank = service::get_or_create_connector(&state.db, &user.id, "up_bank").await?;
    let up_bank_creds =
        service::credentials_for(&state.db, &state.settings.encryption_key, &user.id, "up_bank").await?;
    let has_token =
        up_bank_creds.get("personal_access_token").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
    if up_bank.enabled && has_token {
        if let Ok(summary) = UpBankClient::new(&up_bank_creds).week_summary(&week_start.to_rfc3339()).await {
            result["up_bank"] = summary;
        }
    }

    let pocketai = service::get_or_create_connector(&state.db, &user.id, "pocketai").await?;
    let pocketai_creds =
        service::credentials_for(&state.db, &state.settings.encryption_key, &user.id, "pocketai").await?;
    let has_key = pocketai_creds.get("api_key").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
    if pocketai.enabled && has_key {
        let since = (now - Duration::days(7)).date_naive().to_string();
        let base_url = pocketai.config.get("base_url").and_then(|v| v.as_str());
        let client = PocketAIClient::new(&pocketai_creds, base_url);
        if let Ok(summary) = client.summary(&since).await {
            result["pocketai"] = json!({ "recordings_count": summary["recordings_count"] });
        }
    }

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_out_reports_credentials_set_without_leaking_them() {
        let row = Connector {
            id: "connector:abc".parse().unwrap(),
            owner: "user:abc".parse().unwrap(),
            kind: "up_bank".to_string(),
            enabled: true,
            config: json!({}),
            credentials_encrypted: "ciphertext".to_string(),
            updated_at: chrono::Utc::now().into(),
        };
        let out = connector_out(&row);
        assert_eq!(out["credentials_set"], true);
        assert!(out.get("credentials_encrypted").is_none());
        assert!(out.get("credentials").is_none());
    }

    #[test]
    fn require_known_kind_rejects_demo_and_unknown() {
        assert!(require_known_kind("up_bank").is_ok());
        assert!(require_known_kind("demo").is_err());
        assert!(require_known_kind("something_else").is_err());
    }

    #[test]
    fn client_for_builds_a_client_for_every_known_kind() {
        let creds = json!({});
        let config = json!({});
        assert!(matches!(client_for("up_bank", &config, &creds), Some(AnyClient::UpBank(_))));
        assert!(matches!(client_for("pocketai", &config, &creds), Some(AnyClient::PocketAI(_))));
        assert!(matches!(client_for("open_connector", &config, &creds), Some(AnyClient::OpenConnector(_))));
        assert!(client_for("demo", &config, &creds).is_none());
    }
}

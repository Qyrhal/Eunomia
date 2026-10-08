//! Auth routes: register/login issue the session cookie, `me`/`token` require
//! it (or a Bearer API token, via the `User` extractor). Ported from
//! `app/routers/auth.py`.

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::{Datetime, RecordId};

use crate::audit::{self, Event};
use crate::auth::{self, SESSION_COOKIE};
use crate::authz;
use crate::scopes;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::store;
use crate::models_user::{self, User};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/tokens", post(create_token).get(get_tokens))
        .route("/auth/tokens/{token_id}", delete(delete_token))
        .route("/auth/sessions", get(get_sessions))
        .route("/auth/sessions/{session_id}", delete(revoke_session_route))
        .route("/auth/bootstrap", get(bootstrap))
}

#[derive(Serialize, utoipa::ToSchema)]
struct AuthOut {
    id: String,
    email: String,
    onboarded: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
struct TokenCreated {
    id: String,
    name: String,
    /// Shown once, never retrievable again.
    token: String,
    scopes: Vec<String>,
    vault_id: Option<String>,
    #[schema(value_type = Option<String>)]
    expires_at: Option<Datetime>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct BootstrapOut {
    has_users: bool,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct Credentials {
    email: String,
    password: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
struct TokenCreate {
    name: String,
    /// Any of `memory:read`, `memory:write`, `vaults:admin`, `connectors`. Defaults
    /// to all of them. Cannot exceed the creating credential's own scopes.
    #[serde(default)]
    scopes: Option<Vec<String>>,
    /// Restrict the token to one vault you belong to.
    #[serde(default)]
    vault_id: Option<String>,
    /// RFC 3339 timestamp in the future. Omit for a token that never expires.
    #[serde(default)]
    expires_at: Option<String>,
}

fn user_agent_from(headers: &HeaderMap) -> Option<String> {
    headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(str::to_string)
}

async fn onboarded(state: &AppState, user: &User) -> AppResult<bool> {
    #[derive(Deserialize)]
    struct Row {
        onboarded_at: Option<Datetime>,
    }
    let row: Option<Row> = state.db.select(user.id.clone()).await?;
    Ok(row.map(|r| r.onboarded_at.is_some()).unwrap_or(false))
}

fn session_cookie_header(token: &str) -> String {
    format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/")
}

#[utoipa::path(
    operation_id = "register",
    post,
    path = "/api/auth/register",
    tag = "auth",
    summary = "Create an account and start a session",
    request_body = Credentials,
    responses((status = 200, body = AuthOut), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(()),
)]
async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> AppResult<Response> {
    let user = models_user::register_user(&state.db, &body.email, &body.password)
        .await
        .map_err(|e| {
            if e.code == ErrorCode::DbDuplicate {
                AppError::coded(ErrorCode::AuthEmailTaken, "A user with that email already exists.")
            } else {
                e
            }
        })?;

    let token = auth::start_session(&state.db, &state.settings.jwt_secret, &user, user_agent_from(&headers).as_deref())
        .await?;
    audit::record_as_caller(&state.db, &user.id, "auth.register", "", "ok").await;

    let body = AuthOut { id: user.id.to_string(), email: user.email, onboarded: false };
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, session_cookie_header(&token))],
        Json(body),
    )
        .into_response())
}

#[utoipa::path(
    operation_id = "login",
    post,
    path = "/api/auth/login",
    tag = "auth",
    summary = "Log in and start a session",
    request_body = Credentials,
    responses((status = 200, body = AuthOut), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(()),
)]
async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> AppResult<Response> {
    let Some(user) = models_user::authenticate(&state.db, &body.email, &body.password).await? else {
        let email = models_user::normalize_email(&body.email);
        let event = Event {
            user: None,
            actor: &audit::anonymous(),
            action: "auth.login",
            target: "",
            outcome: ErrorCode::AuthUnauthorized.as_str(),
            detail: &email,
        };
        audit::record(&state.db, event).await;
        return Err(AppError::unauthorized("Invalid email or password."));
    };

    let token = auth::start_session(&state.db, &state.settings.jwt_secret, &user, user_agent_from(&headers).as_deref())
        .await?;
    audit::record_as_caller(&state.db, &user.id, "auth.login", "", "ok").await;
    let onboarded = onboarded(&state, &user).await?;

    let body = AuthOut { id: user.id.to_string(), email: user.email, onboarded };
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, session_cookie_header(&token))],
        Json(body),
    )
        .into_response())
}

#[utoipa::path(
    operation_id = "logout",
    post,
    path = "/api/auth/logout",
    tag = "auth",
    summary = "End the current session",
    responses((status = 200, body = crate::openapi::OkBody), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(()),
)]
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(cookie_header) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok())
        && let Some(token) = cookie_header.split(';').find_map(|p| {
            let p = p.trim();
            p.strip_prefix(&format!("{SESSION_COOKIE}=")).map(str::to_string)
        }) {
            auth::revoke_session_by_jwt(&state.db, &state.settings.jwt_secret, &token).await;
        }
    let expired = format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    (StatusCode::OK, [(header::SET_COOKIE, expired)], Json(json!({ "ok": true }))).into_response()
}

#[utoipa::path(
    operation_id = "getMe",
    get,
    path = "/api/auth/me",
    tag = "auth",
    summary = "The signed in user",
    responses((status = 200, body = AuthOut), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn me(State(state): State<AppState>, user: User) -> AppResult<Json<AuthOut>> {
    let onboarded = onboarded(&state, &user).await?;
    Ok(Json(AuthOut { id: user.id.to_string(), email: user.email, onboarded }))
}

#[utoipa::path(
    operation_id = "createToken",
    post,
    path = "/api/auth/tokens",
    tag = "auth",
    summary = "Create an API token",
    request_body = TokenCreate,
    responses((status = 200, body = TokenCreated), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn create_token(
    State(state): State<AppState>,
    user: User,
    Json(body): Json<TokenCreate>,
) -> AppResult<Json<TokenCreated>> {
    let name = {
        let trimmed = body.name.trim();
        if trimmed.is_empty() { "API token".to_string() } else { trimmed.to_string() }
    };
    let scopes: Vec<String> = match body.scopes {
        None => scopes::ALL.iter().map(|s| s.to_string()).collect(),
        Some(s) if s.is_empty() => return Err(AppError::bad_request("Choose at least one scope.")),
        Some(mut s) => {
            if let Some(bad) = s.iter().find(|n| !scopes::is_known(n)) {
                return Err(AppError::bad_request(format!("Unknown scope {bad:?}.")));
            }
            s.sort();
            s.dedup();
            s
        }
    };
    // a token cannot hand out more than it holds
    for s in &scopes {
        authz::require_scope(s)?;
    }
    let vault = match body.vault_id.as_deref().filter(|v| !v.is_empty()) {
        None => None,
        Some(v) => {
            let rid: RecordId = v
                .parse()
                .ok()
                .filter(|r: &RecordId| r.table() == "vault")
                .ok_or_else(|| AppError::coded(ErrorCode::VaultNotFound, "Vault not found."))?;
            authz::ensure_member(&state.db, &user.id, &rid).await?;
            Some(rid)
        }
    };
    let expires_at = match body.expires_at.as_deref().filter(|v| !v.is_empty()) {
        None => None,
        Some(v) => {
            let at = chrono::DateTime::parse_from_rfc3339(v)
                .map_err(|_| AppError::bad_request("expires_at must be an RFC 3339 timestamp."))?
                .with_timezone(&chrono::Utc);
            if at <= chrono::Utc::now() {
                return Err(AppError::bad_request("expires_at must be in the future."));
            }
            Some(Datetime::from(at))
        }
    };
    let result = models_user::create_api_token_with(&state.db, &user.id, &name, &scopes, vault.as_ref(), expires_at).await?;
    audit::record_as_caller(&state.db, &user.id, "auth.token_create", &result.id.to_string(), "ok").await;
    Ok(Json(TokenCreated {
        id: result.id.to_string(),
        name: result.name,
        token: result.token,
        scopes: result.scopes,
        vault_id: result.vault.map(|v| v.to_string()),
        expires_at: result.expires_at,
    }))
}

#[derive(Serialize, utoipa::ToSchema)]
struct TokenOut {
    id: String,
    name: String,
    #[schema(value_type = Option<String>)]
    created_at: Option<Datetime>,
    #[schema(value_type = Option<String>)]
    last_used_at: Option<Datetime>,
    scopes: Vec<String>,
    vault_id: Option<String>,
    /// `null` for a token that never expires.
    #[schema(value_type = Option<String>)]
    expires_at: Option<Datetime>,
}

#[utoipa::path(
    operation_id = "listTokens",
    get,
    path = "/api/auth/tokens",
    tag = "auth",
    summary = "List API tokens",
    responses((status = 200, body = Vec<TokenOut>), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn get_tokens(State(state): State<AppState>, user: User) -> AppResult<Json<Vec<TokenOut>>> {
    let rows = models_user::list_api_tokens(&state.db, &user.id).await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| TokenOut {
                id: r.id.to_string(),
                name: r.name,
                created_at: Some(r.created_at),
                last_used_at: r.last_used_at,
                scopes: r.scopes.into_iter().filter(|s| scopes::is_known(s)).collect(),
                vault_id: r.vault.map(|v| v.to_string()),
                expires_at: r.expires_at,
            })
            .collect(),
    ))
}

#[utoipa::path(
    operation_id = "deleteToken",
    delete,
    path = "/api/auth/tokens/{token_id}",
    tag = "auth",
    summary = "Revoke an API token",
    params(("token_id" = String, Path)),
    responses((status = 200, body = crate::openapi::OkBody), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn delete_token(
    State(state): State<AppState>,
    user: User,
    Path(token_id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let rid: RecordId = token_id.parse().map_err(|_| AppError::coded(ErrorCode::AuthNotFound, "Token not found."))?;
    let ok = models_user::revoke_api_token(&state.db, &user.id, &rid).await?;
    if !ok {
        return Err(AppError::coded(ErrorCode::AuthNotFound, "Token not found."));
    }
    audit::record_as_caller(&state.db, &user.id, "auth.token_revoke", &rid.to_string(), "ok").await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct SessionRow {
    id: RecordId,
    #[serde(default)]
    user_agent: String,
    created_at: Datetime,
    last_seen_at: Datetime,
}

#[derive(Serialize, utoipa::ToSchema)]
struct SessionOut {
    id: String,
    user_agent: String,
    #[schema(value_type = String)]
    created_at: Datetime,
    #[schema(value_type = String)]
    last_seen_at: Datetime,
}

#[utoipa::path(
    operation_id = "listSessions",
    get,
    path = "/api/auth/sessions",
    tag = "auth",
    summary = "List active sessions",
    responses((status = 200, body = Vec<SessionOut>), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn get_sessions(State(state): State<AppState>, user: User) -> AppResult<Json<Vec<SessionOut>>> {
    let mut res = store::app::AUTH_SESSION_LIST
        .on(&state.db)
        .bind(("owner", user.id.clone()))
        .await?;
    let rows: Vec<SessionRow> = res.take(0)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| SessionOut {
                id: r.id.to_string(),
                user_agent: r.user_agent,
                created_at: r.created_at,
                last_seen_at: r.last_seen_at,
            })
            .collect(),
    ))
}

#[utoipa::path(
    operation_id = "revokeSession",
    delete,
    path = "/api/auth/sessions/{session_id}",
    tag = "auth",
    summary = "Revoke a session",
    params(("session_id" = String, Path)),
    responses((status = 200, body = crate::openapi::OkBody), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn revoke_session_route(
    State(state): State<AppState>,
    user: User,
    Path(session_id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    #[derive(Deserialize)]
    struct Row {
        owner: RecordId,
    }
    let rid: RecordId = session_id.parse().map_err(|_| AppError::coded(ErrorCode::AuthNotFound, "Session not found."))?;
    let row: Option<Row> = state.db.select(rid.clone()).await?;
    match row {
        Some(r) if r.owner == user.id => {}
        _ => return Err(AppError::coded(ErrorCode::AuthNotFound, "Session not found.")),
    }
    store::app::AUTH_SESSION_REVOKE.on(&state.db).bind(("id", rid)).await?;
    Ok(Json(json!({ "ok": true })))
}

#[utoipa::path(
    operation_id = "getBootstrap",
    get,
    path = "/api/auth/bootstrap",
    tag = "auth",
    summary = "Whether any user exists yet",
    responses((status = 200, body = BootstrapOut), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(()),
)]
async fn bootstrap(State(state): State<AppState>) -> AppResult<Json<BootstrapOut>> {
    #[derive(Deserialize)]
    struct CountRow {
        count: i64,
    }
    let mut res = store::app::AUTH_USER_COUNT.on(&state.db).await?;
    let rows: Vec<CountRow> = res.take(0)?;
    let has_users = rows.first().map(|r| r.count > 0).unwrap_or(false);
    Ok(Json(BootstrapOut { has_users }))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(
    register,
    login,
    logout,
    me,
    create_token,
    get_tokens,
    delete_token,
    get_sessions,
    revoke_session_route,
    bootstrap,
))]
pub struct Doc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cookie_header_includes_name_and_flags() {
        let header = session_cookie_header("abc.def.ghi");
        assert!(header.starts_with("eunomia_session=abc.def.ghi;"));
        assert!(header.contains("HttpOnly"));
        assert!(header.contains("SameSite=Lax"));
    }

    #[test]
    fn empty_token_name_defaults_to_api_token() {
        let name = {
            let trimmed = "   ".trim();
            if trimmed.is_empty() { "API token".to_string() } else { trimmed.to_string() }
        };
        assert_eq!(name, "API token");
    }
}

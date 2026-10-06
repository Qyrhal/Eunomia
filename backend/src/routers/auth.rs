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

use crate::auth::{self, SESSION_COOKIE};
use crate::error::{AppError, AppResult};
use crate::models_user::{self, User};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/tokens", post(create_token).get(get_tokens))
        .route("/auth/tokens/:token_id", delete(delete_token))
        .route("/auth/sessions", get(get_sessions))
        .route("/auth/sessions/:session_id", delete(revoke_session_route))
        .route("/auth/bootstrap", get(bootstrap))
}

#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct TokenCreate {
    name: String,
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

async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> AppResult<Response> {
    let user = models_user::register_user(&state.db, &body.email, &body.password)
        .await
        .map_err(|e| {
            if e.message.contains("already contains") || e.message.to_lowercase().contains("already exist") {
                AppError::new(StatusCode::CONFLICT, "A user with that email already exists.")
            } else {
                e
            }
        })?;

    let token = auth::start_session(&state.db, &state.settings.jwt_secret, &user, user_agent_from(&headers).as_deref())
        .await?;

    let body = json!({ "id": user.id.to_string(), "email": user.email, "onboarded": false });
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, session_cookie_header(&token))],
        Json(body),
    )
        .into_response())
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> AppResult<Response> {
    let user = models_user::authenticate(&state.db, &body.email, &body.password)
        .await?
        .ok_or_else(|| AppError::unauthorized("Invalid email or password."))?;

    let token = auth::start_session(&state.db, &state.settings.jwt_secret, &user, user_agent_from(&headers).as_deref())
        .await?;
    let onboarded = onboarded(&state, &user).await?;

    let body = json!({ "id": user.id.to_string(), "email": user.email, "onboarded": onboarded });
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, session_cookie_header(&token))],
        Json(body),
    )
        .into_response())
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(cookie_header) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        if let Some(token) = cookie_header.split(';').find_map(|p| {
            let p = p.trim();
            p.strip_prefix(&format!("{SESSION_COOKIE}=")).map(str::to_string)
        }) {
            auth::revoke_session_by_jwt(&state.db, &state.settings.jwt_secret, &token).await;
        }
    }
    let expired = format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    (StatusCode::OK, [(header::SET_COOKIE, expired)], Json(json!({ "ok": true }))).into_response()
}

async fn me(State(state): State<AppState>, user: User) -> AppResult<Json<serde_json::Value>> {
    let onboarded = onboarded(&state, &user).await?;
    Ok(Json(json!({ "id": user.id.to_string(), "email": user.email, "onboarded": onboarded })))
}

async fn create_token(
    State(state): State<AppState>,
    user: User,
    Json(body): Json<TokenCreate>,
) -> AppResult<Json<serde_json::Value>> {
    let name = {
        let trimmed = body.name.trim();
        if trimmed.is_empty() { "API token".to_string() } else { trimmed.to_string() }
    };
    let result = models_user::create_api_token(&state.db, &user.id, &name).await?;
    Ok(Json(json!({ "id": result.id.to_string(), "name": result.name, "token": result.token })))
}

#[derive(Serialize)]
struct TokenOut {
    id: String,
    name: String,
    created_at: Option<Datetime>,
    last_used_at: Option<Datetime>,
}

async fn get_tokens(State(state): State<AppState>, user: User) -> AppResult<Json<Vec<TokenOut>>> {
    let rows = models_user::list_api_tokens(&state.db, &user.id).await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| TokenOut {
                id: r.id.to_string(),
                name: r.name,
                created_at: Some(r.created_at),
                last_used_at: r.last_used_at,
            })
            .collect(),
    ))
}

async fn delete_token(
    State(state): State<AppState>,
    user: User,
    Path(token_id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let rid: RecordId = token_id.parse().map_err(|_| AppError::not_found("Token not found."))?;
    let ok = models_user::revoke_api_token(&state.db, &user.id, &rid).await?;
    if !ok {
        return Err(AppError::not_found("Token not found."));
    }
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

#[derive(Serialize)]
struct SessionOut {
    id: String,
    user_agent: String,
    created_at: Datetime,
    last_seen_at: Datetime,
}

async fn get_sessions(State(state): State<AppState>, user: User) -> AppResult<Json<Vec<SessionOut>>> {
    let mut res = state
        .db
        .query(
            "SELECT id, user_agent, created_at, last_seen_at FROM session \
             WHERE owner = $owner AND revoked = false ORDER BY last_seen_at DESC",
        )
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

async fn revoke_session_route(
    State(state): State<AppState>,
    user: User,
    Path(session_id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    #[derive(Deserialize)]
    struct Row {
        owner: RecordId,
    }
    let rid: RecordId = session_id.parse().map_err(|_| AppError::not_found("Session not found."))?;
    let row: Option<Row> = state.db.select(rid.clone()).await?;
    match row {
        Some(r) if r.owner == user.id => {}
        _ => return Err(AppError::not_found("Session not found.")),
    }
    state.db.query("UPDATE $id SET revoked = true").bind(("id", rid)).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn bootstrap(State(state): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    #[derive(Deserialize)]
    struct CountRow {
        count: i64,
    }
    let mut res = state.db.query("SELECT count() FROM user GROUP ALL").await?;
    let rows: Vec<CountRow> = res.take(0)?;
    let has_users = rows.first().map(|r| r.count > 0).unwrap_or(false);
    Ok(Json(json!({ "has_users": has_users })))
}

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

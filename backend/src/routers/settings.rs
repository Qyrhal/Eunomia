//! Per-user app settings: embedding model, sync intervals, theme, and the
//! OpenAI(-compatible) base URL + optional API key. Ported from
//! `app/routers/settings.py`.
//!
//! `openai_api_key` is stored AES-GCM-encrypted (`connectors::crypto`) in
//! `openai_api_key_encrypted`; it is never returned, only `openai_api_key_set`.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::db::Db;
use crate::store;
use crate::error::{AppError, AppResult};
use crate::models_user::User;
use crate::state::AppState;

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_OBSERVATIONS_MISSION: &str = "Observations are stable facts about people and relationships: preferences, skills, roles, recurring patterns, and how they change over time. Ignore ephemeral or one-off details.";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(read_settings).patch(patch_settings))
        .route("/settings/complete-onboarding", post(complete_onboarding))
        .route("/settings/openai-models", get(openai_models))
}

#[derive(Debug, Deserialize)]
struct AppSettingsRow {
    #[serde(default)]
    embedding_model: String,
    #[serde(default)]
    sync_intervals: Value,
    #[serde(default)]
    theme: Value,
    #[serde(default)]
    openai_api_key_encrypted: String,
    #[serde(default)]
    openai_base_url: String,
    #[serde(default)]
    observations_mission: String,
    #[serde(default)]
    memory_skill: String,
}

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
struct SettingsUpdate {
    embedding_model: Option<String>,
    sync_intervals: Option<Value>,
    theme: Option<Value>,
    openai_api_key: Option<String>,
    openai_base_url: Option<String>,
    observations_mission: Option<String>,
    /// "" resets to the built-in skill
    memory_skill: Option<String>,
}

fn app_settings_id(owner: &RecordId) -> RecordId {
    RecordId::from_table_key("app_settings", owner.key().clone())
}

#[derive(Serialize, utoipa::ToSchema)]
struct SettingsOut {
    embedding_model: String,
    /// Open map of source key to sync interval seconds.
    #[schema(value_type = Object)]
    sync_intervals: Value,
    /// Open UI theme preferences.
    #[schema(value_type = Object)]
    theme: Value,
    openai_api_key_set: bool,
    openai_base_url: String,
    observations_mission: String,
    memory_skill: String,
    memory_skill_custom: bool,
}

fn out(row: &AppSettingsRow) -> SettingsOut {
    SettingsOut {
        embedding_model: row.embedding_model.clone(),
        sync_intervals: row.sync_intervals.clone(),
        theme: row.theme.clone(),
        openai_api_key_set: !row.openai_api_key_encrypted.is_empty(),
        openai_base_url: row.openai_base_url.clone(),
        observations_mission: row.observations_mission.clone(),
        memory_skill: crate::docs::effective_skill(&row.memory_skill).to_string(),
        memory_skill_custom: !row.memory_skill.trim().is_empty(),
    }
}

/// The `app_settings:<owner_id>` row, creating it with defaults if missing.
/// The skill text for `owner` (theirs, or the built-in default).
pub async fn memory_skill(db: &Db, owner: &RecordId) -> AppResult<String> {
    let row = get_app_settings(db, owner).await?;
    Ok(crate::docs::effective_skill(&row.memory_skill).to_string())
}

async fn get_app_settings(db: &Db, owner: &RecordId) -> AppResult<AppSettingsRow> {
    let rid = app_settings_id(owner);
    let row: Option<AppSettingsRow> = db.select(rid.clone()).await?;
    if let Some(row) = row {
        return Ok(row);
    }

    let mut res = store::app::SETTINGS_UPSERT_DEFAULTS
        .on(db)
        .bind(("id", rid))
        .bind(("owner", owner.clone()))
        .bind(("embedding_model", "text-embedding-3-small"))
        .bind(("sync_intervals", json!({ "heypocket": 86400 })))
        .bind(("theme", json!({})))
        .bind(("observations_mission", DEFAULT_OBSERVATIONS_MISSION))
        .bind(("openai_base_url", DEFAULT_OPENAI_BASE_URL))
        .await?;
    let rows: Vec<AppSettingsRow> = res.take(0)?;
    rows.into_iter()
        .next()
        .ok_or_else(|| AppError::internal("app_settings upsert returned no row"))
}

/// Partial update: each provided field replaces its current value outright
/// (no deep merge), matching the Python router's semantics.
async fn update_app_settings(db: &Db, owner: &RecordId, body: &SettingsUpdate, encryption_key: &str) -> AppResult<AppSettingsRow> {
    get_app_settings(db, owner).await?; // ensure the row exists

    let mut set_parts: Vec<&str> = Vec::new();
    if body.embedding_model.is_some() {
        set_parts.push("embedding_model = $embedding_model");
    }
    if body.sync_intervals.is_some() {
        set_parts.push("sync_intervals = $sync_intervals");
    }
    if body.theme.is_some() {
        set_parts.push("theme = $theme");
    }
    if body.openai_api_key.is_some() {
        set_parts.push("openai_api_key_encrypted = $openai_api_key_encrypted");
    }
    if body.openai_base_url.is_some() {
        set_parts.push("openai_base_url = $openai_base_url");
    }
    if body.observations_mission.is_some() {
        set_parts.push("observations_mission = $observations_mission");
    }
    if body.memory_skill.is_some() {
        set_parts.push("memory_skill = $memory_skill");
    }

    if set_parts.is_empty() {
        return get_app_settings(db, owner).await;
    }

    let query_str = format!("UPDATE $id SET {}, updated_at = time::now() RETURN AFTER", set_parts.join(", "));
    let rid = app_settings_id(owner);
    // dynamic: the SET clause list depends on which fields the patch carries
    let mut q = store::dynamic(db, "app.settings_update", query_str).bind(("id", rid));
    if let Some(v) = &body.embedding_model {
        q = q.bind(("embedding_model", v.clone()));
    }
    if let Some(v) = &body.sync_intervals {
        q = q.bind(("sync_intervals", v.clone()));
    }
    if let Some(v) = &body.theme {
        q = q.bind(("theme", v.clone()));
    }
    if let Some(v) = &body.openai_api_key {
        q = q.bind(("openai_api_key_encrypted", crate::connectors::crypto::encrypt(encryption_key, v)));
    }
    if let Some(v) = &body.openai_base_url {
        q = q.bind(("openai_base_url", v.clone()));
    }
    if let Some(v) = &body.observations_mission {
        q = q.bind(("observations_mission", v.clone()));
    }
    if let Some(v) = &body.memory_skill {
        // saving the built-in text unchanged keeps following future defaults
        let v = if v.trim() == crate::docs::DEFAULT_SKILL.trim() { String::new() } else { v.clone() };
        q = q.bind(("memory_skill", v));
    }

    let mut res = q.await?;
    let rows: Vec<AppSettingsRow> = res.take(0)?;
    rows.into_iter()
        .next()
        .ok_or_else(|| AppError::internal("app_settings update returned no row"))
}

/// Resolve `(base_url, api_key)` for `owner`'s OpenAI-compatible backend.
async fn resolve_openai(db: &Db, owner: &RecordId, env_api_key: &Option<String>, encryption_key: &str) -> AppResult<(String, String)> {
    let row = get_app_settings(db, owner).await?;
    let base_url = if row.openai_base_url.is_empty() {
        DEFAULT_OPENAI_BASE_URL.to_string()
    } else {
        row.openai_base_url
    };
    let key = if !row.openai_api_key_encrypted.is_empty() {
        crate::connectors::crypto::decrypt_or_plaintext(encryption_key, &row.openai_api_key_encrypted)
    } else {
        env_api_key.clone().unwrap_or_default()
    };
    Ok((base_url, key))
}

#[utoipa::path(
    operation_id = "getSettings",
    get,
    path = "/api/settings",
    tag = "settings",
    summary = "Read app settings",
    responses((status = 200, body = SettingsOut), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn read_settings(State(state): State<AppState>, user: User) -> AppResult<Json<SettingsOut>> {
    let row = get_app_settings(&state.db, &user.id).await?;
    Ok(Json(out(&row)))
}

#[utoipa::path(
    operation_id = "updateSettings",
    patch,
    path = "/api/settings",
    tag = "settings",
    summary = "Update app settings",
    request_body = SettingsUpdate,
    responses((status = 200, body = SettingsOut), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn patch_settings(
    State(state): State<AppState>,
    user: User,
    Json(body): Json<SettingsUpdate>,
) -> AppResult<Json<SettingsOut>> {
    let row = update_app_settings(&state.db, &user.id, &body, &state.settings.encryption_key).await?;
    Ok(Json(out(&row)))
}

#[utoipa::path(
    operation_id = "completeOnboarding",
    post,
    path = "/api/settings/complete-onboarding",
    tag = "settings",
    summary = "Mark onboarding done",
    responses((status = 200, body = crate::openapi::OkBody), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn complete_onboarding(State(state): State<AppState>, user: User) -> AppResult<Json<Value>> {
    store::app::AUTH_USER_ONBOARDED
        .on(&state.db)
        .bind(("id", user.id.clone()))
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct ModelsListResponse {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ModelsOut {
    models: Vec<String>,
    /// Null on success.
    error: Option<String>,
}

/// Lists models from the caller's configured OpenAI-compatible base URL.
/// Never errors out to the caller -- an unreachable base URL or auth
/// failure comes back as `{"models": [], "error": "..."}`.
#[utoipa::path(
    operation_id = "listOpenaiModels",
    get,
    path = "/api/settings/openai-models",
    tag = "settings",
    summary = "List models the configured OpenAI endpoint offers",
    responses((status = 200, body = ModelsOut), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn openai_models(State(state): State<AppState>, user: User) -> AppResult<Json<ModelsOut>> {
    let (base_url, api_key) = resolve_openai(&state.db, &user.id, &state.settings.openai_api_key, &state.settings.encryption_key).await?;
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let auth_key = if api_key.is_empty() { "not-needed" } else { api_key.as_str() };

    let client = reqwest::Client::new();
    let resp = match client.get(&url).bearer_auth(auth_key).send().await {
        Ok(r) => r,
        Err(e) => return Ok(Json(ModelsOut { models: vec![], error: Some(e.to_string()) })),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        return Ok(Json(ModelsOut { models: vec![], error: Some(format!("HTTP {status}")) }));
    }

    match resp.json::<ModelsListResponse>().await {
        Ok(body) => {
            let mut ids: Vec<String> = body.data.into_iter().map(|m| m.id).collect();
            ids.sort();
            Ok(Json(ModelsOut { models: ids, error: None }))
        }
        Err(e) => Ok(Json(ModelsOut { models: vec![], error: Some(e.to_string()) })),
    }
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(
    read_settings,
    patch_settings,
    complete_onboarding,
    openai_models,
))]
pub struct Doc;

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key_set: bool) -> AppSettingsRow {
        AppSettingsRow {
            embedding_model: "text-embedding-3-small".to_string(),
            sync_intervals: json!({ "heypocket": 86400 }),
            theme: json!({}),
            openai_api_key_encrypted: if key_set { "sk-very-secret-123".to_string() } else { String::new() },
            openai_base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
            observations_mission: DEFAULT_OBSERVATIONS_MISSION.to_string(),
            memory_skill: String::new(),
        }
    }

    #[test]
    fn out_reports_key_set_true_when_encrypted_value_present() {
        let v = serde_json::to_value(out(&row(true))).unwrap();
        assert_eq!(v["openai_api_key_set"], json!(true));
        assert!(v.get("openai_api_key_encrypted").is_none());
    }

    #[test]
    fn out_reports_key_set_false_when_empty() {
        let v = serde_json::to_value(out(&row(false))).unwrap();
        assert_eq!(v["openai_api_key_set"], json!(false));
    }

    #[test]
    fn out_never_leaks_the_raw_secret() {
        let v = serde_json::to_value(out(&row(true))).unwrap();
        let dumped = v.to_string();
        assert!(!dumped.contains("sk-very-secret-123"));
    }
}

//! Per-user app settings: embedding model, sync intervals, theme, and the
//! OpenAI(-compatible) base URL + optional API key. Ported from
//! `app/routers/settings.py`.
//!
//! `openai_api_key` is stored AES-GCM-encrypted (`connectors::crypto`) in
//! `openai_api_key_encrypted`; it is never returned, only `openai_api_key_set`.
//! An empty `openai_base_url` means the server's `OPENAI_BASE_URL`; which key
//! goes where is decided by `embeddings::provider`.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::config::Settings;
use crate::db::Db;
use crate::embeddings::{provider, service as embeddings};
use crate::error::{AppError, AppResult};
use crate::models_user::User;
use crate::state::AppState;

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

#[derive(Debug, Default, Deserialize)]
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

fn out(row: &AppSettingsRow, server_base_url: &str) -> Value {
    json!({
        "embedding_model": row.embedding_model,
        "sync_intervals": row.sync_intervals,
        "theme": row.theme,
        "openai_api_key_set": !row.openai_api_key_encrypted.is_empty(),
        "openai_base_url": provider::effective_base_url(server_base_url, &row.openai_base_url),
        "observations_mission": row.observations_mission,
        "memory_skill": crate::docs::effective_skill(&row.memory_skill),
        "memory_skill_custom": !row.memory_skill.trim().is_empty(),
    })
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

    let mut res = db
        .query(
            "UPSERT $id SET owner = $owner, embedding_model = $embedding_model, \
             sync_intervals = $sync_intervals, theme = $theme, \
             observations_mission = $observations_mission, openai_base_url = $openai_base_url, \
             updated_at = time::now() RETURN AFTER",
        )
        .bind(("id", rid))
        .bind(("owner", owner.clone()))
        .bind(("embedding_model", embeddings::DEFAULT_MODEL))
        .bind(("sync_intervals", json!({ "heypocket": 86400 })))
        .bind(("theme", json!({})))
        .bind(("observations_mission", DEFAULT_OBSERVATIONS_MISSION))
        .bind(("openai_base_url", "")) // "" = the server's
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
    let mut q = db.query(query_str).bind(("id", rid));
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

/// A base URL must be an http(s) URL ("" = the server's).
fn check_base_url(v: &str) -> Result<(), String> {
    let v = v.trim();
    if v.is_empty() {
        return Ok(());
    }
    match reqwest::Url::parse(v) {
        Ok(u) if (u.scheme() == "http" || u.scheme() == "https") && u.host_str().is_some() => Ok(()),
        _ => Err(format!("base URL must be an http(s) URL, got `{v}`")),
    }
}

/// Rejects a base URL or embedding model the update would make unusable,
/// before anything is written.
async fn validate_update(db: &Db, settings: &Settings, owner: &RecordId, body: &SettingsUpdate) -> AppResult<()> {
    if let Some(v) = &body.openai_base_url {
        check_base_url(v).map_err(AppError::bad_request)?;
    }
    if body.openai_base_url.is_none() && body.embedding_model.is_none() {
        return Ok(());
    }
    let row = get_app_settings(db, owner).await?;
    let base = body.openai_base_url.as_deref().unwrap_or(&row.openai_base_url);
    let model = body.embedding_model.as_deref().unwrap_or(&row.embedding_model).trim();
    let model = if model.is_empty() { embeddings::DEFAULT_MODEL } else { model };
    embeddings::check_model(provider::effective_base_url(&settings.openai_base_url, base), model).map_err(AppError::bad_request)
}

async fn read_settings(State(state): State<AppState>, user: User) -> AppResult<Json<Value>> {
    let row = get_app_settings(&state.db, &user.id).await?;
    Ok(Json(out(&row, &state.settings.openai_base_url)))
}

async fn patch_settings(
    State(state): State<AppState>,
    user: User,
    Json(body): Json<SettingsUpdate>,
) -> AppResult<Json<Value>> {
    validate_update(&state.db, &state.settings, &user.id, &body).await?;
    let row = update_app_settings(&state.db, &user.id, &body, &state.settings.encryption_key).await?;
    Ok(Json(out(&row, &state.settings.openai_base_url)))
}

async fn complete_onboarding(State(state): State<AppState>, user: User) -> AppResult<Json<Value>> {
    state
        .db
        .query("UPDATE $id SET onboarded_at = time::now()")
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

/// Lists models from the caller's configured OpenAI-compatible base URL.
/// Never errors out to the caller -- an unreachable base URL or auth
/// failure comes back as `{"models": [], "error": "..."}`.
async fn openai_models(State(state): State<AppState>, user: User) -> AppResult<Json<Value>> {
    let p = match provider::resolve(&state.db, &state.settings, &user.id).await {
        Ok(p) => p,
        Err(e) => return Ok(Json(json!({ "models": [], "error": e.message }))),
    };
    let resp = match provider::client().get(p.url("models")).bearer_auth(p.bearer()).send().await {
        Ok(r) => r,
        Err(e) => return Ok(Json(json!({ "models": [], "error": e.to_string() }))),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        return Ok(Json(json!({ "models": [], "error": format!("HTTP {status}") })));
    }

    match resp.json::<ModelsListResponse>().await {
        Ok(body) => {
            let mut ids: Vec<String> = body.data.into_iter().map(|m| m.id).collect();
            ids.sort();
            Ok(Json(json!({ "models": ids, "error": Value::Null })))
        }
        Err(e) => Ok(Json(json!({ "models": [], "error": e.to_string() }))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key_set: bool) -> AppSettingsRow {
        AppSettingsRow {
            embedding_model: embeddings::DEFAULT_MODEL.to_string(),
            sync_intervals: json!({ "heypocket": 86400 }),
            theme: json!({}),
            openai_api_key_encrypted: if key_set { "sk-very-secret-123".to_string() } else { String::new() },
            openai_base_url: String::new(),
            observations_mission: DEFAULT_OBSERVATIONS_MISSION.to_string(),
            memory_skill: String::new(),
        }
    }

    #[test]
    fn out_reports_key_set_true_when_encrypted_value_present() {
        let v = out(&row(true), SERVER);
        assert_eq!(v["openai_api_key_set"], json!(true));
        assert!(v.get("openai_api_key_encrypted").is_none());
    }

    #[test]
    fn out_reports_key_set_false_when_empty() {
        let v = out(&row(false), SERVER);
        assert_eq!(v["openai_api_key_set"], json!(false));
    }

    const SERVER: &str = "http://llm.internal/v1";

    #[test]
    fn out_shows_the_servers_base_url_when_the_user_has_none() {
        assert_eq!(out(&row(false), SERVER)["openai_base_url"], json!(SERVER));
        let mut r = row(false);
        r.openai_base_url = "http://mine/v1".into();
        assert_eq!(out(&r, SERVER)["openai_base_url"], json!("http://mine/v1"));
    }

    #[test]
    fn base_urls_must_be_http_urls() {
        assert!(check_base_url("").is_ok());
        assert!(check_base_url("http://localhost:11434/v1").is_ok());
        assert!(check_base_url("https://api.openai.com/v1").is_ok());
        assert!(check_base_url("file:///etc/passwd").is_err());
        assert!(check_base_url("not a url").is_err());
        assert!(check_base_url("ftp://x/v1").is_err());
    }

    #[test]
    fn out_never_leaks_the_raw_secret() {
        let v = out(&row(true), SERVER);
        let dumped = v.to_string();
        assert!(!dumped.contains("sk-very-secret-123"));
    }
}

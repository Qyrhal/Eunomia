//! Per-user app settings: embedding model, sync intervals, theme, and the
//! OpenAI(-compatible) base URL + optional API key. Ported from
//! `app/routers/settings.py`.
//!
//! `connectors/service.py`'s Fernet-based credential encryption hasn't been
//! ported to Rust yet (no cipher crate in this crate's dependencies), so the
//! `openai_api_key` value is stored as-is in the `openai_api_key_encrypted`
//! column rather than actually encrypted. The read side (`openai_api_key_set`)
//! and all other behavior match the Python router exactly.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::db::Db;
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
}

#[derive(Debug, Default, Deserialize)]
struct SettingsUpdate {
    embedding_model: Option<String>,
    sync_intervals: Option<Value>,
    theme: Option<Value>,
    openai_api_key: Option<String>,
    openai_base_url: Option<String>,
    observations_mission: Option<String>,
}

fn app_settings_id(owner: &RecordId) -> RecordId {
    RecordId::from_table_key("app_settings", owner.key().clone())
}

fn out(row: &AppSettingsRow) -> Value {
    json!({
        "embedding_model": row.embedding_model,
        "sync_intervals": row.sync_intervals,
        "theme": row.theme,
        "openai_api_key_set": !row.openai_api_key_encrypted.is_empty(),
        "openai_base_url": row.openai_base_url,
        "observations_mission": row.observations_mission,
    })
}

/// The `app_settings:<owner_id>` row, creating it with defaults if missing.
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
async fn update_app_settings(db: &Db, owner: &RecordId, body: &SettingsUpdate) -> AppResult<AppSettingsRow> {
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
        // Deferred: should be Fernet-encrypted (see module docstring).
        q = q.bind(("openai_api_key_encrypted", v.clone()));
    }
    if let Some(v) = &body.openai_base_url {
        q = q.bind(("openai_base_url", v.clone()));
    }
    if let Some(v) = &body.observations_mission {
        q = q.bind(("observations_mission", v.clone()));
    }

    let mut res = q.await?;
    let rows: Vec<AppSettingsRow> = res.take(0)?;
    rows.into_iter()
        .next()
        .ok_or_else(|| AppError::internal("app_settings update returned no row"))
}

/// Resolve `(base_url, api_key)` for `owner`'s OpenAI-compatible backend.
async fn resolve_openai(db: &Db, owner: &RecordId, env_api_key: &Option<String>) -> AppResult<(String, String)> {
    let row = get_app_settings(db, owner).await?;
    let base_url = if row.openai_base_url.is_empty() {
        DEFAULT_OPENAI_BASE_URL.to_string()
    } else {
        row.openai_base_url
    };
    let key = if !row.openai_api_key_encrypted.is_empty() {
        row.openai_api_key_encrypted
    } else {
        env_api_key.clone().unwrap_or_default()
    };
    Ok((base_url, key))
}

async fn read_settings(State(state): State<AppState>, user: User) -> AppResult<Json<Value>> {
    let row = get_app_settings(&state.db, &user.id).await?;
    Ok(Json(out(&row)))
}

async fn patch_settings(
    State(state): State<AppState>,
    user: User,
    Json(body): Json<SettingsUpdate>,
) -> AppResult<Json<Value>> {
    let row = update_app_settings(&state.db, &user.id, &body).await?;
    Ok(Json(out(&row)))
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
    let (base_url, api_key) = resolve_openai(&state.db, &user.id, &state.settings.openai_api_key).await?;
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let auth_key = if api_key.is_empty() { "not-needed" } else { api_key.as_str() };

    let client = reqwest::Client::new();
    let resp = match client.get(&url).bearer_auth(auth_key).send().await {
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
            embedding_model: "text-embedding-3-small".to_string(),
            sync_intervals: json!({ "heypocket": 86400 }),
            theme: json!({}),
            openai_api_key_encrypted: if key_set { "secret".to_string() } else { String::new() },
            openai_base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
            observations_mission: DEFAULT_OBSERVATIONS_MISSION.to_string(),
        }
    }

    #[test]
    fn out_reports_key_set_true_when_encrypted_value_present() {
        let v = out(&row(true));
        assert_eq!(v["openai_api_key_set"], json!(true));
        assert!(v.get("openai_api_key_encrypted").is_none());
    }

    #[test]
    fn out_reports_key_set_false_when_empty() {
        let v = out(&row(false));
        assert_eq!(v["openai_api_key_set"], json!(false));
    }

    #[test]
    fn out_never_leaks_the_raw_secret() {
        let v = out(&row(true));
        let dumped = v.to_string();
        assert!(!dumped.contains("secret"));
    }
}

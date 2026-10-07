//! Self-update check/trigger -- reads/writes files in
//! `settings.update_status_dir` rather than running `git`/`docker` itself.
//! The privileged work (git, docker) happens in the separate `updater`
//! service, never in this web-facing process.
//!
//! Not configured (the directory doesn't exist) is a normal, expected
//! state, surfaced as `{"configured": false}` rather than an error.

use std::path::PathBuf;

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::models_user::User;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/update/status", get(get_status))
        .route("/update/request", post(request_update))
        .route("/update/check", post(check_now))
}

fn status_dir(state: &AppState) -> PathBuf {
    PathBuf::from(&state.settings.update_status_dir)
}

/// Merges the on-disk status JSON into `{"configured": true, ...}`, mirroring
/// the Python handler's `{"configured": True, **data}`.
fn merge_status(data: Value) -> Value {
    let mut merged = json!({ "configured": true });
    if let (Some(obj), Some(data_obj)) = (merged.as_object_mut(), data.as_object()) {
        for (k, v) in data_obj {
            obj.insert(k.clone(), v.clone());
        }
    }
    merged
}

async fn get_status(State(state): State<AppState>, _user: User) -> AppResult<Json<Value>> {
    let status_file = status_dir(&state).join("status.json");
    if !status_file.exists() {
        return Ok(Json(json!({ "configured": false })));
    }

    let text = match std::fs::read_to_string(&status_file) {
        Ok(t) => t,
        Err(_) => return Ok(Json(json!({ "configured": false }))),
    };
    let data: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Ok(Json(json!({ "configured": false }))),
    };

    Ok(Json(merge_status(data)))
}

/// Drops a marker file for `scripts/auto-update.sh` (run every 20s by the
/// `updater` service) -- acted on at its next run, not instantly.
fn drop_marker(state: &AppState, name: &str) -> AppResult<Json<Value>> {
    let dir = status_dir(state);
    if !dir.exists() {
        return Ok(Json(json!({ "configured": false })));
    }
    std::fs::File::create(dir.join(name)).map_err(|e| crate::error::AppError::internal(e.to_string()))?;
    Ok(Json(json!({ "configured": true, "requested": true })))
}

/// "Update now": apply the newest release.
async fn request_update(State(state): State<AppState>, _user: User) -> AppResult<Json<Value>> {
    drop_marker(&state, "requested")
}

/// "Check now": ask GitHub for the newest release without waiting for the
/// 10-minute throttle.
async fn check_now(State(state): State<AppState>, _user: User) -> AppResult<Json<Value>> {
    drop_marker(&state, "check")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_status_adds_configured_true_alongside_data() {
        let merged = merge_status(json!({
            "current_version": "v1.0.0",
            "latest_version": "v1.1.0",
            "update_available": true,
            "checked_at": "now"
        }));
        assert_eq!(merged["configured"], json!(true));
        assert_eq!(merged["current_version"], json!("v1.0.0"));
        assert_eq!(merged["update_available"], json!(true));
    }

    #[test]
    fn merge_status_with_empty_object_is_just_configured() {
        let merged = merge_status(json!({}));
        assert_eq!(merged, json!({ "configured": true }));
    }
}

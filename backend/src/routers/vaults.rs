//! Vault REST routes: thin wrappers over `vaults::service`, same pattern as
//! `routers/auth.rs`. Ported from `app/routers/vaults.py`.

use axum::{
    extract::{Path, State},
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::error::{AppError, AppResult};
use crate::models_user::User;
use crate::state::AppState;
use crate::vaults::service;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/vaults", get(list_vaults).post(create_vault))
        .route("/vaults/invitations", get(list_invitations))
        .route("/vaults/merge", post(merge_vaults))
        .route("/vaults/:vault_id", patch(rename_vault).delete(delete_vault))
        .route("/vaults/:vault_id/clone", post(clone_vault))
        .route("/vaults/:vault_id/members", get(list_members).post(invite_member))
        .route("/vaults/:vault_id/members/:email", axum::routing::delete(remove_member))
        .route("/vaults/:vault_id/leave", post(leave_vault))
        .route("/vaults/:vault_id/invitations/accept", post(accept_invitation))
        .route("/vaults/:vault_id/invitations/decline", post(decline_invitation))
}

fn default_kind() -> String {
    "org".to_string()
}

fn default_role() -> String {
    "member".to_string()
}

#[derive(Deserialize)]
struct VaultCreate {
    name: String,
    #[serde(default = "default_kind")]
    kind: String,
}

#[derive(Deserialize)]
struct VaultClone {
    #[serde(default)]
    name: Option<String>,
    #[serde(default = "default_kind")]
    kind: String,
}

#[derive(Deserialize)]
struct VaultRename {
    name: String,
}

#[derive(Deserialize)]
struct InviteRequest {
    email: String,
    #[serde(default = "default_role")]
    role: String,
}

/// Mirrors the Python router's inline `kind` validation on create/clone.
fn valid_vault_kind(kind: &str) -> bool {
    matches!(kind, "org" | "personal")
}

/// Path params are plain strings (like `app/routers/vaults.py`'s `vault_id:
/// str`); parse here, same convention as `routers/auth.rs`'s token/session
/// id parsing.
fn parse_vault_id(vault_id: &str) -> AppResult<RecordId> {
    vault_id.parse().map_err(|_| AppError::not_found("Vault not found."))
}

async fn list_vaults(State(state): State<AppState>, user: User) -> AppResult<Json<Value>> {
    let results = service::list_my_vaults(&state.db, &user.id).await?;
    Ok(Json(json!({ "results": results })))
}

async fn create_vault(
    State(state): State<AppState>,
    user: User,
    Json(body): Json<VaultCreate>,
) -> AppResult<Json<service::VaultOut>> {
    if !valid_vault_kind(&body.kind) {
        return Err(AppError::bad_request("kind must be 'org' or 'personal'"));
    }
    Ok(Json(service::create_vault(&state.db, &user.id, &body.name, &body.kind).await?))
}

async fn clone_vault(
    State(state): State<AppState>,
    user: User,
    Path(vault_id): Path<String>,
    Json(body): Json<VaultClone>,
) -> AppResult<Json<service::CloneOut>> {
    if !valid_vault_kind(&body.kind) {
        return Err(AppError::bad_request("kind must be 'org' or 'personal'"));
    }
    let rid = parse_vault_id(&vault_id)?;
    Ok(Json(service::clone_vault(&state.db, &user.id, &rid, body.name.as_deref(), &body.kind).await?))
}

#[derive(Deserialize)]
struct VaultMerge {
    vault_ids: [String; 2],
    #[serde(default)]
    name: Option<String>,
    #[serde(default = "default_kind")]
    kind: String,
}

async fn merge_vaults(
    State(state): State<AppState>,
    user: User,
    Json(body): Json<VaultMerge>,
) -> AppResult<Json<service::MergeOut>> {
    if !valid_vault_kind(&body.kind) {
        return Err(AppError::bad_request("kind must be 'org' or 'personal'"));
    }
    let a = parse_vault_id(&body.vault_ids[0])?;
    let b = parse_vault_id(&body.vault_ids[1])?;
    Ok(Json(service::merge_vaults(&state.db, &user.id, &a, &b, body.name.as_deref(), &body.kind).await?))
}

async fn rename_vault(
    State(state): State<AppState>,
    user: User,
    Path(vault_id): Path<String>,
    Json(body): Json<VaultRename>,
) -> AppResult<Json<service::VaultOut>> {
    let rid = parse_vault_id(&vault_id)?;
    Ok(Json(service::rename_vault(&state.db, &user.id, &rid, &body.name).await?))
}

async fn delete_vault(
    State(state): State<AppState>,
    user: User,
    Path(vault_id): Path<String>,
) -> AppResult<Json<Value>> {
    let rid = parse_vault_id(&vault_id)?;
    service::delete_vault(&state.db, &user.id, &rid).await?;
    Ok(Json(json!({ "deleted": true })))
}

async fn list_members(
    State(state): State<AppState>,
    user: User,
    Path(vault_id): Path<String>,
) -> AppResult<Json<Value>> {
    let rid = parse_vault_id(&vault_id)?;
    let results = service::list_members(&state.db, &user.id, &rid).await?;
    Ok(Json(json!({ "results": results })))
}

async fn invite_member(
    State(state): State<AppState>,
    user: User,
    Path(vault_id): Path<String>,
    Json(body): Json<InviteRequest>,
) -> AppResult<Json<service::InviteOut>> {
    let rid = parse_vault_id(&vault_id)?;
    Ok(Json(service::invite_member(&state.db, &user.id, &rid, &body.email, &body.role).await?))
}

async fn remove_member(
    State(state): State<AppState>,
    user: User,
    Path((vault_id, email)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let rid = parse_vault_id(&vault_id)?;
    service::remove_member(&state.db, &user.id, &rid, &email).await?;
    Ok(Json(json!({ "removed": true })))
}

async fn leave_vault(
    State(state): State<AppState>,
    user: User,
    Path(vault_id): Path<String>,
) -> AppResult<Json<Value>> {
    let rid = parse_vault_id(&vault_id)?;
    service::leave_vault(&state.db, &user.id, &rid).await?;
    Ok(Json(json!({ "left": true })))
}

async fn list_invitations(State(state): State<AppState>, user: User) -> AppResult<Json<Value>> {
    let results = service::list_my_invitations(&state.db, &user.id).await?;
    Ok(Json(json!({ "results": results })))
}

async fn accept_invitation(
    State(state): State<AppState>,
    user: User,
    Path(vault_id): Path<String>,
) -> AppResult<Json<service::VaultWithRole>> {
    let rid = parse_vault_id(&vault_id)?;
    Ok(Json(service::accept_invitation(&state.db, &user.id, &rid).await?))
}

async fn decline_invitation(
    State(state): State<AppState>,
    user: User,
    Path(vault_id): Path<String>,
) -> AppResult<Json<Value>> {
    let rid = parse_vault_id(&vault_id)?;
    service::decline_invitation(&state.db, &user.id, &rid).await?;
    Ok(Json(json!({ "declined": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_vault_kind_accepts_only_org_and_personal() {
        assert!(valid_vault_kind("org"));
        assert!(valid_vault_kind("personal"));
        assert!(!valid_vault_kind("team"));
        assert!(!valid_vault_kind(""));
    }

    #[test]
    fn defaults_match_python_pydantic_models() {
        assert_eq!(default_kind(), "org");
        assert_eq!(default_role(), "member");
    }

    #[test]
    fn parse_vault_id_rejects_garbage() {
        assert!(parse_vault_id("not-a-record-id!!").is_err());
    }

    #[test]
    fn parse_vault_id_accepts_well_formed_id() {
        assert!(parse_vault_id("vault:abc123").is_ok());
    }
}

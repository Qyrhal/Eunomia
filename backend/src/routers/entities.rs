//! Entity-memory REST surface: list/get/graph plus create/edit/delete over
//! the person/organisation/location + memory + relates_to graph,
//! vault-scoped like `routers/vaults.rs`. Thin wrappers over
//! `entities::service` -- the same functions `entities::tools`'s functions
//! call -- so the manual add/edit UI and a future agent's tool calls stay in
//! lockstep.
//!
//! Ported from `app/routers/entities.py`.
//!
//! `GET /entities/graph`'s `kinds` filter matches Python's repeated-query-param
//! form (`?kinds=a&kinds=b`) via a manual `RawQuery` parse (see `parse_kinds`),
//! since `axum::extract::Query`'s `serde_urlencoded` backing doesn't collect
//! repeated keys into a `Vec` without `axum-extra`. Comma-separated
//! (`?kinds=a,b`) is also accepted. Every other behavior (status codes,
//! response shapes, field names, vault membership checks) matches exactly.

use axum::{
    extract::{Path, Query, RawQuery, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::entities::service;
use crate::error::{AppError, AppResult};
use crate::models_user::User;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/entities", get(list_entities).post(create_entity))
        .route("/entities/graph", get(entity_graph))
        .route("/entities/memory/:memory_id", axum::routing::delete(delete_memory))
        .route("/entities/:entity_id", get(get_entity).patch(update_entity).delete(delete_entity))
        .route("/entities/:entity_id/memory", post(add_memory))
        .route("/entities/:entity_id/relations", post(add_relation))
        .route("/entities/:entity_id/merge", post(merge_entities))
}

fn default_memory_type() -> String {
    "world".to_string()
}

#[derive(Debug, Deserialize)]
struct EntityCreate {
    kind: String,
    name: String,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    #[serde(default)]
    vault_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct EntityUpdate {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    #[serde(default)]
    summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryCreate {
    text: String,
    #[serde(rename = "type", default = "default_memory_type")]
    mem_type: String,
}

#[derive(Debug, Deserialize)]
struct RelationCreate {
    to_id: String,
    label: String,
}

#[derive(Debug, Deserialize)]
struct MergeRequest {
    loser_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    kind: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
    vault_id: Option<String>,
}

fn default_limit() -> usize {
    50
}

#[derive(Debug, Default, Deserialize)]
struct GraphQuery {
    vault_id: Option<String>,
}

/// `kinds` can arrive either as a repeated query param (`?kinds=a&kinds=b`,
/// the browser/fetch convention this frontend uses) or comma-separated
/// (`?kinds=a,b`) -- accept both rather than pick one.
fn parse_kinds(raw_query: Option<&str>) -> Vec<String> {
    let Some(raw) = raw_query else { return Vec::new() };
    let pairs: Vec<(String, String)> = serde_urlencoded::from_str(raw).unwrap_or_default();
    pairs
        .into_iter()
        .filter(|(k, _)| k == "kinds")
        .flat_map(|(_, v)| v.split(',').map(str::to_string).collect::<Vec<_>>())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Path params are plain strings (like `app/routers/entities.py`'s
/// `entity_id: str`); parse here, same convention as `routers/vaults.rs`'s
/// `parse_vault_id`.
fn parse_record_id(id: &str) -> AppResult<RecordId> {
    id.parse().map_err(|_| AppError::not_found("not found"))
}

fn known_kind_or_400(kind: &str) -> AppResult<()> {
    if !service::KINDS.contains(&kind) {
        return Err(AppError::bad_request(format!("unknown kind {kind:?}; use one of {:?}", service::KINDS)));
    }
    Ok(())
}

async fn list_entities(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<service::ListEntitiesOut>> {
    if let Some(k) = &q.kind {
        known_kind_or_400(k)?;
    }
    let vault_rid = q.vault_id.as_deref().map(parse_record_id).transpose()?;
    Ok(Json(
        service::list_entities(&state.db, &user.id, q.kind.as_deref(), vault_rid.as_ref(), Some(q.limit), q.offset)
            .await?,
    ))
}

async fn entity_graph(
    State(state): State<AppState>,
    user: User,
    Query(q): Query<GraphQuery>,
    RawQuery(raw): RawQuery,
) -> AppResult<Json<service::GraphOut>> {
    let kinds_vec = parse_kinds(raw.as_deref());
    let kinds: Option<Vec<String>> = if kinds_vec.is_empty() { None } else { Some(kinds_vec) };
    if let Some(ks) = &kinds {
        let bad: Vec<&String> = ks.iter().filter(|k| !service::KINDS.contains(&k.as_str())).collect();
        if !bad.is_empty() {
            return Err(AppError::bad_request(format!("unknown kind(s) {bad:?}; use one of {:?}", service::KINDS)));
        }
    }
    let vault_rid = q.vault_id.as_deref().map(parse_record_id).transpose()?;
    Ok(Json(service::graph(&state.db, &user.id, kinds.as_deref(), vault_rid.as_ref()).await?))
}

async fn create_entity(
    State(state): State<AppState>,
    user: User,
    Json(body): Json<EntityCreate>,
) -> AppResult<Json<service::EntityOut>> {
    known_kind_or_400(&body.kind)?;
    let vault_rid = body.vault_id.as_deref().map(parse_record_id).transpose()?;
    Ok(Json(
        service::upsert_entity(&state.db, &user.id, &body.kind, &body.name, body.aliases, vault_rid.as_ref()).await?,
    ))
}

async fn delete_memory(
    State(state): State<AppState>,
    user: User,
    Path(memory_id): Path<String>,
) -> AppResult<Json<Value>> {
    let rid = parse_record_id(&memory_id)?;
    let deleted = service::delete_memory(&state.db, &user.id, &rid).await?;
    if !deleted {
        return Err(AppError::not_found("not found"));
    }
    Ok(Json(json!({ "deleted": true })))
}

async fn get_entity(
    State(state): State<AppState>,
    user: User,
    Path(entity_id): Path<String>,
) -> AppResult<Json<service::EntityDetail>> {
    let rid = parse_record_id(&entity_id)?;
    let entity = service::get_entity(&state.db, &user.id, &rid).await?;
    entity.map(Json).ok_or_else(|| AppError::not_found("not found"))
}

async fn update_entity(
    State(state): State<AppState>,
    user: User,
    Path(entity_id): Path<String>,
    Json(body): Json<EntityUpdate>,
) -> AppResult<Json<service::EntityOut>> {
    let rid = parse_record_id(&entity_id)?;
    let entity =
        service::update_entity(&state.db, &user.id, &rid, body.name.as_deref(), body.aliases, body.summary.as_deref())
            .await?;
    entity.map(Json).ok_or_else(|| AppError::not_found("not found"))
}

async fn delete_entity(
    State(state): State<AppState>,
    user: User,
    Path(entity_id): Path<String>,
) -> AppResult<Json<Value>> {
    let rid = parse_record_id(&entity_id)?;
    let deleted = service::delete_entity(&state.db, &user.id, &rid).await?;
    if !deleted {
        return Err(AppError::not_found("not found"));
    }
    Ok(Json(json!({ "deleted": true })))
}

async fn add_memory(
    State(state): State<AppState>,
    user: User,
    Path(entity_id): Path<String>,
    Json(body): Json<MemoryCreate>,
) -> AppResult<Json<service::MemoryOut>> {
    let rid = parse_record_id(&entity_id)?;
    if service::get_entity(&state.db, &user.id, &rid).await?.is_none() {
        return Err(AppError::not_found("not found"));
    }
    Ok(Json(service::add_memory(&state.db, &user.id, &rid, &body.text, None, &body.mem_type).await?))
}

async fn add_relation(
    State(state): State<AppState>,
    user: User,
    Path(entity_id): Path<String>,
    Json(body): Json<RelationCreate>,
) -> AppResult<Json<service::RelationOut>> {
    let rid = parse_record_id(&entity_id)?;
    if service::get_entity(&state.db, &user.id, &rid).await?.is_none() {
        return Err(AppError::not_found("not found"));
    }
    let to_rid = parse_record_id(&body.to_id)?;
    if service::get_entity(&state.db, &user.id, &to_rid).await?.is_none() {
        return Err(AppError::not_found("target entity not found"));
    }
    Ok(Json(service::add_relation(&state.db, &user.id, &rid, &to_rid, &body.label, None).await?))
}

async fn merge_entities(
    State(state): State<AppState>,
    user: User,
    Path(entity_id): Path<String>,
    Json(body): Json<MergeRequest>,
) -> AppResult<Json<service::EntityOut>> {
    let rid = parse_record_id(&entity_id)?;
    let loser_rid = parse_record_id(&body.loser_id)?;
    Ok(Json(service::merge_entities(&state.db, &user.id, &rid, &loser_rid).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_matches_python_default() {
        assert_eq!(default_limit(), 50);
    }

    #[test]
    fn default_memory_type_matches_python_default() {
        assert_eq!(default_memory_type(), "world");
    }

    #[test]
    fn known_kind_or_400_accepts_valid_kinds() {
        assert!(known_kind_or_400("person").is_ok());
        assert!(known_kind_or_400("symbol").is_ok());
    }

    #[test]
    fn known_kind_or_400_rejects_unknown_kind() {
        assert!(known_kind_or_400("alien").is_err());
    }

    #[test]
    fn parse_record_id_rejects_garbage() {
        assert!(parse_record_id("not-a-record-id!!").is_err());
    }

    #[test]
    fn parse_record_id_accepts_well_formed_id() {
        assert!(parse_record_id("person:abc123").is_ok());
    }
}

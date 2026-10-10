//! REST surface for the tool registry -- the catalogue is public (schema
//! only, no data); calling a tool requires auth since it reads the caller's
//! owned data.
//!
//! `all_tools()` and `call()` pick up new registry entries automatically.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;

use crate::error::AppResult;
use crate::models_user::User;
use crate::state::AppState;
use crate::tools::registry;

pub fn router() -> Router<AppState> {
    Router::new().route("/tools", get(catalogue)).route("/tools/{name}", post(invoke))
}

#[utoipa::path(
    operation_id = "listTools",
    get,
    path = "/api/tools",
    tag = "tools",
    summary = "Names of the available tools",
    responses((status = 200, body = Vec<String>), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(()),
)]
async fn catalogue() -> Json<Value> {
    // Names only: the tools' JSON schemas are served by the MCP `tools/list` call.
    let names: Vec<&str> = registry::all_tools().keys().copied().collect();
    Json(serde_json::json!(names))
}

#[utoipa::path(
    operation_id = "invokeTool",
    post,
    path = "/api/tools/{name}",
    tag = "tools",
    summary = "Invoke a tool",
    params(("name" = String, Path)),
    request_body(content = Object, description = "Tool arguments"),
    // open body: each tool defines its own result shape
    responses((status = 200, body = Object), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn invoke(
    State(state): State<AppState>,
    user: User,
    Path(name): Path<String>,
    body: Option<Json<Value>>,
) -> AppResult<Json<Value>> {
    if !registry::all_tools().contains_key(name.as_str()) {
        return Err(crate::error::AppError::coded(crate::error::ErrorCode::ToolNotFound, format!("unknown tool {name:?}")));
    }
    let args = body.map(|Json(v)| v).unwrap_or(Value::Object(serde_json::Map::new()));
    let result = registry::call(&state, &user, &name, args).await?;
    Ok(Json(result))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(
    catalogue,
    invoke,
))]
pub struct Doc;

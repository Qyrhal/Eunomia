//! REST surface for the tool registry -- the catalogue is public (schema
//! only, no data); calling a tool requires auth since it reads the caller's
//! owned data. Ported from `app/routers/tools.py`.
//!
//! The catalogue currently reports `{}` for every caller: `tools::registry`
//! is stubbed pending `entities::tools`, `vaults::tools`, `cache::tools` and
//! `sources::registry` landing (see `tools::registry::build_registry`'s doc
//! comment). This router itself needs no changes once those wire up --
//! `all_tools()` and `call()` pick the new entries up automatically.

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

async fn catalogue() -> Json<Value> {
    // The Python version returns each tool's JSON schema alongside its name;
    // there's no Rust equivalent of that schema literal yet (see
    // `tools::registry`'s doc comment), so this reports names only until a
    // schema representation exists.
    let names: Vec<&str> = registry::all_tools().keys().copied().collect();
    Json(serde_json::json!(names))
}

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
    let result = registry::call(&state, &user.id, &name, args).await?;
    Ok(Json(result))
}

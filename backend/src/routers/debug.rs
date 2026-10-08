//! Agent-debugging surface: fetch a failure capsule by trace id. Admin only (docs/debugging.md).

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};

use crate::capsules::{self, Capsule};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::models_user::User;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/debug/capsules/{trace_id}", get(get_capsule))
}

#[utoipa::path(
    operation_id = "getFailureCapsule",
    get,
    path = "/api/debug/capsules/{trace_id}",
    tag = "debug",
    summary = "Fetch the redacted failure capsule for a trace id (instance admin only)",
    params(("trace_id" = String, Path)),
    responses((status = 200, body = Capsule), (status = "default", description = "Error", body = crate::openapi::Problem, content_type = "application/problem+json")),
    security(("cookie" = []), ("bearer" = [])),
)]
async fn get_capsule(State(state): State<AppState>, user: User, Path(trace_id): Path<String>) -> AppResult<Json<Capsule>> {
    if !capsules::is_admin(&state.db, &user).await? {
        return Err(AppError::coded(ErrorCode::AuthForbidden, "Only an instance admin can read failure capsules."));
    }
    capsules::get(&state.db, &trace_id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::not_found(format!("No failure capsule for trace {trace_id}.")))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(get_capsule), components(schemas(Capsule)))]
pub struct Doc;

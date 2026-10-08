pub mod auth;
pub mod cache;
pub mod chat;
pub mod config;
pub mod connectors;
pub mod db;
pub mod docs;
pub mod embeddings;
pub mod entities;
pub mod error;
pub mod models_user;
pub mod openapi;
pub mod routers;
pub mod sources;
pub mod state;
pub mod telemetry;
pub mod tools;
pub mod vaults;

use axum::http::{header, HeaderName, HeaderValue, Method};
use tower_http::cors::CorsLayer;

use crate::state::AppState;

/// The full HTTP app (CORS included), shared by `main` and the tests.
pub fn app(state: AppState) -> axum::Router {
    let allowed_origins: Vec<HeaderValue> = state
        .settings
        .cors_allowed_origins
        .split(',')
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .filter_map(|o| o.parse().ok())
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::PUT, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, HeaderName::from_static("traceparent")])
        .expose_headers([HeaderName::from_static("x-trace-id")]);

    let api = axum::Router::new()
        .route("/openapi.json", axum::routing::get(openapi::openapi_json))
        .merge(routers::auth::router())
        .merge(routers::chat::router())
        .merge(routers::settings::router())
        .merge(routers::audit::router())
        .merge(routers::export::router())
        .merge(routers::update::router())
        .merge(routers::vaults::router())
        .merge(routers::connectors::router())
        .merge(routers::connectors::snapshot_router())
        .merge(routers::entities::router())
        .merge(routers::tools::router())
        .merge(routers::sources::router())
        .merge(routers::sources::webhook_router());

    axum::Router::new()
        .route("/healthz", axum::routing::get(healthz))
        .merge(routers::mcp::router())
        .nest("/api", api)
        .layer(axum::middleware::from_fn(telemetry::trace_request))
        .layer(cors)
        .with_state(state)
}

async fn healthz() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

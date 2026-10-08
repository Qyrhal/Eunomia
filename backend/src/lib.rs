pub mod audit;
pub mod auth;
pub mod authz;
pub mod cache;
pub mod capsules;
pub mod chat;
pub mod config;
pub mod connectors;
pub mod db;
pub mod docs;
pub mod embeddings;
pub mod entities;
pub mod error;
pub mod gate;
pub mod jobs;
pub mod migrate;
pub mod models_user;
pub mod oauth;
pub mod openapi;
pub mod rid;
pub mod ratelimit;
pub mod replay;
pub mod routers;
pub mod scopes;
pub mod sources;
pub mod state;
pub mod store;
pub mod telemetry;
pub mod tools;
pub mod tx;
pub mod vaults;

use axum::http::{header, HeaderName, HeaderValue, Method};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::state::AppState;

/// The full HTTP app (CORS included), shared by `main` and the tests.
pub fn app(state: AppState) -> axum::Router {
    app_with(state, ratelimit::RateConfig::from_env())
}

/// [`app`] with explicit rate limits (the tests use tiny ones).
pub fn app_with(state: AppState, limits: ratelimit::RateConfig) -> axum::Router {
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
        .merge(routers::debug::router())
        .merge(routers::update::router())
        .merge(routers::vaults::router())
        .merge(routers::connectors::router())
        .merge(routers::connectors::snapshot_router())
        .merge(routers::entities::router())
        .merge(routers::tools::router())
        .merge(routers::sources::router())
        .merge(routers::sources::webhook_router())
        .merge(routers::oauth::router());

    // Browser-based MCP clients (for example MCP Inspector) run on any origin and
    // authenticate with a bearer token only, so these routes answer CORS for any
    // origin and never allow credentials: no cookie is ever sent or honoured here.
    // The ambient-credential routes (`/api`, which accept the session cookie) keep
    // the single-origin credentialed CORS above.
    let open_cors = CorsLayer::new()
        .allow_origin(AllowOrigin::any())
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
            HeaderName::from_static("mcp-protocol-version"),
            HeaderName::from_static("mcp-session-id"),
            HeaderName::from_static("traceparent"),
        ])
        .expose_headers([header::WWW_AUTHENTICATE, HeaderName::from_static("x-trace-id"), HeaderName::from_static("mcp-session-id")]);

    // CORS wraps the gate so a preflight is answered before authentication.
    let gate = gate::Gate::new(state.clone(), limits);
    let capture_state = state.clone();
    let guarded = |routes: axum::Router<AppState>, cors: CorsLayer| {
        routes
            .layer(axum::middleware::from_fn_with_state(capture_state.clone(), capsules::capture))
            .layer(axum::middleware::from_fn_with_state(gate.clone(), gate::gate))
            .layer(axum::middleware::from_fn(telemetry::trace_request))
            .layer(cors)
    };

    let open = axum::Router::new().route("/healthz", axum::routing::get(healthz)).merge(routers::mcp::router()).merge(oauth::router());
    guarded(open, open_cors).merge(guarded(axum::Router::new().nest("/api", api), cors)).with_state(state)
}

async fn healthz() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

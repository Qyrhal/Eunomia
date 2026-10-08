use std::sync::Arc;

use axum::http::{header, HeaderValue, Method};
use tower_http::cors::CorsLayer;

use eunomia_backend::config::Settings;
use eunomia_backend::db;
use eunomia_backend::routers;
use eunomia_backend::state::{AppState, AppStateInner};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let settings = Settings::load();
    let conn = db::connect(&settings).await.expect("failed to connect to SurrealDB");
    db::ensure_schema(&conn, &settings).await.expect("failed to apply schema");

    let state = AppState(Arc::new(AppStateInner { db: conn, settings: settings.clone() }));

    let _scheduler_handles = eunomia_backend::sources::scheduler::spawn(state.clone()).await;

    let allowed_origins: Vec<HeaderValue> = settings
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
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let api = axum::Router::new()
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

    let app = axum::Router::new()
        .route("/healthz", axum::routing::get(healthz))
        .merge(routers::mcp::router())
        .nest("/api", api)
        .layer(cors)
        .with_state(state);

    // PORT: lets several dev/test backends run side by side; containers keep 8001
    let port = std::env::var("PORT").unwrap_or_else(|_| "8001".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn healthz() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

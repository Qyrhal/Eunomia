use std::sync::Arc;

use eunomia_backend::config::Settings;
use eunomia_backend::db;
use eunomia_backend::state::{AppState, AppStateInner};

#[tokio::main]
async fn main() {
    let settings = Settings::load();
    let _otel = eunomia_backend::telemetry::init(&settings.log_level);
    let conn = db::connect(&settings).await.expect("failed to connect to SurrealDB");
    eunomia_backend::migrate::migrate(&conn, &settings).await.expect("failed to apply migrations");

    let state = AppState(Arc::new(AppStateInner { db: conn, settings: settings.clone() }));

    let _scheduler_handles = eunomia_backend::sources::scheduler::spawn(state.clone()).await;

    let app = eunomia_backend::app(state);

    let listener = tokio::net::TcpListener::bind(&settings.bind_addr).await.expect("failed to bind BIND_ADDR");
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

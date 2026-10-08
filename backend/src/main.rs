use std::sync::Arc;
use std::time::Duration;

use eunomia_backend::config::Settings;
use eunomia_backend::db;
use eunomia_backend::jobs::{self, Role, WorkerConfig};
use eunomia_backend::state::{AppState, AppStateInner};
use tokio::sync::watch;

#[tokio::main]
async fn main() {
    let settings = Settings::load();
    let _otel = eunomia_backend::telemetry::init(&settings.log_level);
    let role = Role::from_env();
    let conn = db::connect(&settings).await.expect("failed to connect to SurrealDB");
    eunomia_backend::migrate::migrate(&conn, &settings).await.expect("failed to apply migrations");

    let state = AppState(Arc::new(AppStateInner { db: conn, settings: settings.clone() }));

    let (stop, stopped) = watch::channel(false);
    let mut background = Vec::new();
    let cfg = WorkerConfig::from_env();
    if role.runs_jobs() {
        background.push(tokio::spawn(jobs::worker::run(state.clone(), jobs::handlers::registry(), cfg.clone(), stopped.clone())));
        background.push(tokio::spawn(jobs::leader::run(state.clone(), cfg.clone(), stopped.clone())));
    }

    if role.serves_http() {
        let app = eunomia_backend::app(state);
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8001").await.unwrap();
        tracing::info!(?role, "listening on {}", listener.local_addr().unwrap());
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).with_graceful_shutdown(shutdown_signal()).await.unwrap();
    } else {
        tracing::info!(?role, worker = %cfg.id, "worker running (no HTTP)");
        shutdown_signal().await;
    }

    // Tell the job loops to finish or release their leases, and give them the grace period.
    let _ = stop.send(true);
    let _ = tokio::time::timeout(cfg.grace + Duration::from_secs(5), futures::future::join_all(background)).await;
}

/// Resolves on SIGTERM (docker stop) or Ctrl-C.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}

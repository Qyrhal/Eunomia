use std::time::Duration;

use eunomia_backend::config::Settings;
use eunomia_backend::jobs::{self, Role, WorkerConfig};
use eunomia_backend::state::AppState;
use tokio::sync::watch;

#[tokio::main]
async fn main() {
    let settings = Settings::load();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("replay") {
        std::process::exit(eunomia_backend::replay::cli(&args[1..], &settings).await);
    }
    let _otel = eunomia_backend::telemetry::init(&settings.log_level);
    let role = Role::from_env();
    let state = AppState::build(&settings, surrealdb::opt::Config::new()).await.expect("failed to set up the databases");

    let (stop, stopped) = watch::channel(false);
    let mut background = Vec::new();
    let cfg = WorkerConfig::from_env();
    if role.runs_jobs() {
        background.push(tokio::spawn(jobs::worker::run(state.clone(), jobs::handlers::registry(), cfg.clone(), stopped.clone())));
        background.push(tokio::spawn(jobs::leader::run(state.clone(), cfg.clone(), stopped.clone())));
    }

    if role.serves_http() {
        let app = eunomia_backend::app(state);
        let listener = tokio::net::TcpListener::bind(&settings.bind_addr).await.expect("failed to bind BIND_ADDR");
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

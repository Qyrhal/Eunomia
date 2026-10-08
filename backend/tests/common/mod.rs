//! Shared harness: a fresh in-memory SurrealDB per test, the real schema, the
//! real axum router, and helpers to call registry tools and HTTP routes.
//! Nothing here touches the network.
#![allow(dead_code)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use eunomia_backend::config::Settings;
use eunomia_backend::models_user::{self, User};
use eunomia_backend::state::{AppState, AppStateInner};
use eunomia_backend::tools::registry;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

pub const PASSWORD: &str = "correct-horse-battery";

pub struct TestApp {
    pub state: AppState,
    pub user: User,
    pub token: String,
    pub router: Router,
}

/// In-memory SurrealDB by default. `TEST_SURREAL_URL=ws://127.0.0.1:8231/rpc` (with
/// `TEST_SURREAL_USER` / `TEST_SURREAL_PASS`) runs the same suite against a real server, e.g. one
/// started with the hardened flags from docker-compose.yml; each call then gets a fresh database.
pub fn test_settings() -> Settings {
    let url = std::env::var("TEST_SURREAL_URL").unwrap_or_else(|_| "mem://".into());
    let db = if url == "mem://" { "test".to_string() } else { format!("t{}", uuid::Uuid::new_v4().simple()) };
    Settings {
        jwt_secret: "test-jwt-secret".into(),
        surreal_url: url,
        surreal_user: std::env::var("TEST_SURREAL_USER").unwrap_or_else(|_| "root".into()),
        surreal_pass: std::env::var("TEST_SURREAL_PASS").unwrap_or_else(|_| "root".into()),
        surreal_ns: "test".into(),
        surreal_db: db,
        openai_api_key: None,
        openai_base_url: "http://127.0.0.1:9/v1".into(), // unroutable: never reached without a key
        encryption_key: "test-encryption-key".into(),
        embeddings_backend: "openai".into(),
        cors_allowed_origins: "http://localhost:3000".into(),
        log_level: "WARN".into(),
        update_status_dir: "/nonexistent".into(),
        bind_addr: "127.0.0.1:0".into(),
        public_url: "http://localhost:8001".into(),
    }
}

/// Fresh DB + schema + app state, no users yet.
pub async fn bare_state() -> AppState {
    let settings = test_settings();
    let db = eunomia_backend::db::connect(&settings).await.expect("mem db");
    eunomia_backend::migrate::migrate(&db, &settings).await.expect("schema");
    AppState(Arc::new(AppStateInner { db, settings }))
}

impl TestApp {
    /// Fresh DB plus one registered user (with their personal vault, exactly
    /// as signup creates it) and an API token for Bearer auth.
    pub async fn new() -> Self {
        let state = bare_state().await;
        let user = models_user::register_user(&state.db, "tester@example.com", PASSWORD).await.expect("register");
        let token = models_user::create_api_token(&state.db, &user.id, "test").await.expect("token").token;
        let router = eunomia_backend::app(state.clone());
        TestApp { state, user, token, router }
    }

    /// Call a registry tool as the test user (the same choke point REST and MCP use).
    pub async fn tool(&self, name: &str, args: Value) -> Value {
        registry::call(&self.state, &self.user.id, name, args).await.expect("tool call")
    }

    /// One HTTP request against the real router. `auth` adds the Bearer token.
    pub async fn http(&self, method: &str, path: &str, body: Option<Value>, auth: bool) -> (StatusCode, Value) {
        http(&self.router, method, path, body, auth.then_some(self.token.as_str()), None).await.0
    }
}

/// Returns ((status, json body), set-cookie header if any).
pub async fn http(
    router: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    bearer: Option<&str>,
    cookie: Option<&str>,
) -> ((StatusCode, Value), Option<String>) {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(t) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let req = match body {
        Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let resp = router.clone().oneshot(req).await.expect("router");
    let status = resp.status();
    let set_cookie = resp.headers().get(header::SET_COOKIE).and_then(|v| v.to_str().ok()).map(str::to_string);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    ((status, json), set_cookie)
}

/// Turns run-specific values (record ids, timestamps, hashes) into stable
/// placeholders, numbered by first appearance so equal values stay equal in
/// the snapshot.
#[derive(Default)]
pub struct Normalizer {
    ids: std::collections::HashMap<String, String>,
}

impl Normalizer {
    pub fn apply(&mut self, v: &Value) -> Value {
        use regex::Regex;
        let ts = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})?").unwrap();
        let hash = Regex::new(r"\b[0-9a-f]{64}\b").unwrap();
        // surreal random ids are 20 lowercase alphanumerics, e.g. entity:abc...
        let rid = Regex::new(r"\b[0-9a-z]{20}\b").unwrap();
        let mut s = v.to_string();
        s = ts.replace_all(&s, "<ts>").into_owned();
        s = hash.replace_all(&s, "<hash>").into_owned();
        // trace ids are random per request
        s = Regex::new(r"\b[0-9a-f]{32}\b").unwrap().replace_all(&s, "<trace>").into_owned();
        let found: Vec<String> = rid.find_iter(&s).map(|m| m.as_str().to_string()).collect();
        for id in found {
            let n = self.ids.len() + 1;
            let ph = self.ids.entry(id.clone()).or_insert_with(|| format!("<id{n}>")).clone();
            s = s.replace(&id, &ph);
        }
        serde_json::from_str(&s).unwrap()
    }
}

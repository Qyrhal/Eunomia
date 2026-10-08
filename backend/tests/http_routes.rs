//! HTTP route tests against the real router and an in-memory SurrealDB.

mod common;

use axum::http::StatusCode;
use common::{http, TestApp, PASSWORD};
use serde_json::json;

#[tokio::test]
async fn signup_then_login_issues_a_session_cookie() {
    let state = common::bare_state().await;
    let router = eunomia_backend::app(state);
    let creds = json!({"email": "new@example.com", "password": PASSWORD});

    let ((status, body), cookie) = http(&router, "POST", "/api/auth/register", Some(creds.clone()), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["email"], "new@example.com");
    assert!(cookie.unwrap().starts_with("eunomia_session="));

    let ((status, _), _) = http(&router, "POST", "/api/auth/register", Some(creds.clone()), None, None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let bad = json!({"email": "new@example.com", "password": "wrong-password"});
    let ((status, _), _) = http(&router, "POST", "/api/auth/login", Some(bad), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let ((status, body), cookie) = http(&router, "POST", "/api/auth/login", Some(creds), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["onboarded"], false);
    let session = cookie.unwrap();
    let session = session.split(';').next().unwrap().to_string();

    let ((status, body), _) = http(&router, "GET", "/api/auth/me", None, None, Some(&session)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["email"], "new@example.com");
}

#[tokio::test]
async fn protected_routes_return_401_without_auth() {
    let app = TestApp::new().await;
    for (method, path) in [("GET", "/api/auth/me"), ("GET", "/api/vaults"), ("POST", "/api/tools/vault_list")] {
        let (status, _) = app.http(method, path, None, false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
    let (status, body) = app.http("GET", "/healthz", None, false).await;
    assert_eq!((status, body["status"].as_str()), (StatusCode::OK, Some("ok")));
}

#[tokio::test]
async fn vault_routes_list_and_create() {
    let app = TestApp::new().await;
    let (status, body) = app.http("GET", "/api/vaults", None, true).await;
    assert_eq!(status, StatusCode::OK);
    let vaults = body.as_array().or(body["results"].as_array()).expect("vault list");
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0]["kind"], "personal");

    let (status, created) = app.http("POST", "/api/vaults", Some(json!({"name": "Team"})), true).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["name"], "Team");

    let (_, body) = app.http("GET", "/api/vaults", None, true).await;
    let n = body.as_array().or(body["results"].as_array()).unwrap().len();
    assert_eq!(n, 2);
}

#[tokio::test]
async fn tool_route_runs_a_registry_tool() {
    let app = TestApp::new().await;
    let (status, body) = app.http("POST", "/api/tools/docs", Some(json!({})), true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["docs"].as_array().is_some_and(|d| !d.is_empty()));
    let (status, _) = app.http("POST", "/api/tools/not_a_tool", Some(json!({})), true).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

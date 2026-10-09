//! A per-user connector `base_url` is ignored unless the operator sets EUNOMIA_ALLOW_CONNECTOR_BASE_URL=1,
//! and even then every request goes through the outbound guard. One test, because it edits the environment.

use axum::http::HeaderMap;
use eunomia_backend::connectors::clients::{base_url_override_allowed, Api};
use eunomia_backend::sources::mock::{route, serve};
use serde_json::json;

#[tokio::test]
async fn base_url_override_is_off_by_default_and_still_guarded_when_on() {
    // SAFETY: this binary has a single test, so nothing else reads the environment concurrently.
    unsafe { std::env::remove_var("EUNOMIA_ALLOW_CONNECTOR_BASE_URL") };
    assert!(!base_url_override_allowed());

    // Off: the user's base_url is ignored, the vendor default is used (here a host the guard refuses
    // outright, so nothing leaves the process), and the mock is never called.
    let mock = serve(vec![route("GET", "/x", json!({}))]).await;
    let api = Api::new(&json!({"base_url": mock.base}), "http://metadata", HeaderMap::new());
    let err = api.get("/x", &[]).await.unwrap_err();
    assert!(err.message.contains("not allowed"), "{}", err.message);
    assert!(mock.requests().is_empty());

    // On: the override is honoured...
    unsafe { std::env::set_var("EUNOMIA_ALLOW_CONNECTOR_BASE_URL", "1") };
    assert!(base_url_override_allowed());
    let api = Api::new(&json!({"base_url": mock.base}), "http://metadata", HeaderMap::new());
    api.get("/x", &[]).await.unwrap();
    assert_eq!(mock.requests().len(), 1);

    // ...but cloud metadata and the database stay unreachable.
    for target in ["http://169.254.169.254", "http://surrealdb:8000"] {
        let api = Api::new(&json!({"base_url": target}), "https://api.example.com", HeaderMap::new());
        let err = api.get("/latest/meta-data", &[]).await.unwrap_err();
        assert!(err.message.contains("not allowed"), "{target}: {}", err.message);
    }
}

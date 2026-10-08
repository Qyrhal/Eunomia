//! Failure capsules and `eunomia replay` (docs/debugging.md): capture with redaction, replay
//! against a scratch database, retention, and the admin-only route.

mod common;

use common::{http, TestApp, PASSWORD};
use eunomia_backend::capsules::{self, Failure};
use eunomia_backend::error::ErrorCode;
use eunomia_backend::models_user;
use eunomia_backend::replay;
use eunomia_backend::telemetry::with_trace_id;
use eunomia_backend::tools::registry;
use serde_json::{json, Value};

fn trace(n: u32) -> String {
    format!("{n:032x}")
}

/// A tool call that fails with a non-validation code (`entity.not_found`) and carries secrets.
async fn failing_tool(app: &TestApp, trace_id: &str) -> Value {
    let args = json!({
        "entity_id": "person:nobody", "name": "x", "api_key": "sk-live-SECRET1", "note": "sent Bearer abcDEF123456 to host",
        "nested": { "password": "hunter2-SECRET2" }
    });
    with_trace_id(trace_id.to_string(), registry::call(&app.state, &app.user.id, "entity_update", args)).await.unwrap()
}

#[tokio::test]
async fn forced_tool_failure_records_a_redacted_capsule() {
    let app = TestApp::new().await;
    let out = failing_tool(&app, &trace(1)).await;
    assert_eq!(out["code"], "entity.not_found");

    let c = capsules::get(&app.state.db, &trace(1)).await.unwrap().expect("capsule recorded");
    assert_eq!((c.kind.as_str(), c.name.as_str(), c.code.as_str()), ("tool", "entity_update", "entity.not_found"));
    assert_eq!(c.user.as_deref(), Some(app.user.id.to_string().as_str()));
    assert_eq!(c.version, eunomia_backend::config::APP_VERSION);
    assert_eq!(c.args["entity_id"], "person:nobody");
    let text = serde_json::to_string(&c).unwrap();
    for secret in ["sk-live-SECRET1", "hunter2-SECRET2", "abcDEF123456"] {
        assert!(!text.contains(secret), "{secret} leaked into the capsule");
    }
    assert_eq!(c.args["api_key"], "***");
    assert!(c.args["note"].as_str().unwrap().contains("Bearer ***"));
}

#[tokio::test]
async fn validation_errors_leave_no_capsule() {
    let app = TestApp::new().await;
    let out = with_trace_id(trace(2), registry::call(&app.state, &app.user.id, "entity_update", json!({ "wrong": 1 }))).await.unwrap();
    assert_eq!(out["code"], "validation.invalid");
    assert!(capsules::get(&app.state.db, &trace(2)).await.unwrap().is_none());
}

#[tokio::test]
async fn capsules_are_capped_at_16kb() {
    let app = TestApp::new().await;
    let args = json!({ "entity_id": "person:nobody", "summary": "y ".repeat(40_000) });
    with_trace_id(trace(3), registry::call(&app.state, &app.user.id, "entity_update", args)).await.unwrap();
    let c = capsules::get(&app.state.db, &trace(3)).await.unwrap().unwrap();
    assert!(c.truncated);
    assert!(c.args.is_null(), "a cut capsule keeps no half-parsed args");
    assert!(replay::replay(&app.state.db, &app.state.settings, &trace(3)).await.is_err(), "a cut capsule is not replayable");
}

#[tokio::test]
async fn replay_reproduces_the_same_error_code_in_a_scratch_database() {
    let app = TestApp::new().await;
    failing_tool(&app, &trace(4)).await;
    let r = replay::replay(&app.state.db, &app.state.settings, &trace(4)).await.unwrap();
    assert_eq!(r.replayed.code, "entity.not_found");
    assert!(r.reproduced);
    let test = replay::emit_test(&r.capsule);
    assert!(test.contains("registry::call") && test.contains("entity_update") && !test.contains("SECRET"), "{test}");
}

#[tokio::test]
async fn replay_seeds_the_scratch_database_with_the_callers_data() {
    let app = TestApp::new().await;
    app.tool("memory_write", json!({ "subject_name": "Ada", "subject_kind": "person", "text": "likes engines" })).await;
    let found = app.tool("entities_search", json!({ "query": "Ada" })).await;
    let id = found["results"][0]["id"].as_str().expect("entity id").to_string();

    // a capsule for a call that would succeed: the replay differs, and shows the copied data
    with_trace_id(
        trace(5),
        capsules::record(
            &app.state.db,
            Failure {
                kind: "tool",
                name: "entities_get".into(),
                user: Some(app.user.id.to_string()),
                args: json!({ "id": id }),
                code: ErrorCode::Internal,
                status: 500,
                source: "boom".into(),
            },
        ),
    )
    .await;
    let r = replay::replay(&app.state.db, &app.state.settings, &trace(5)).await.unwrap();
    assert!(!r.reproduced);
    assert_eq!(r.replayed.code, "ok");
    assert!(r.replayed.body.to_string().contains("likes engines"), "{}", r.replayed.body);
}

#[tokio::test]
async fn a_5xx_route_records_a_route_capsule_that_replays() {
    let app = TestApp::new().await;
    // a row the typed reader cannot decode: the route fails with a database error
    let bad = "REMOVE TABLE audit_log; DEFINE TABLE audit_log SCHEMALESS; CREATE audit_log SET owner = $u, tool_name = 'x', outcome = 5;";
    app.state.db.query(bad).bind(("u", app.user.id.clone())).await.unwrap().check().unwrap();
    let ((status, body), _) = http(&app.router, "GET", "/api/audit?limit=5", None, Some(&app.token), None).await;
    assert_eq!(status, 500);
    let trace_id = body["trace_id"].as_str().unwrap();
    let c = capsules::get(&app.state.db, trace_id).await.unwrap().expect("route capsule");
    assert_eq!((c.kind.as_str(), c.name.as_str(), c.code.as_str()), ("route", "GET /api/audit", "internal"));
    assert!(!c.source.is_empty());
    assert!(!serde_json::to_string(&c).unwrap().contains(&app.token));

    let r = replay::replay(&app.state.db, &app.state.settings, trace_id).await.unwrap();
    assert_eq!(r.replayed.status, Some(200), "the scratch database has clean data");
    assert!(!r.reproduced);
}

#[tokio::test]
async fn retention_keeps_seven_days_and_the_newest_rows() {
    let app = TestApp::new().await;
    let db = &app.state.db;
    for n in 10..15 {
        failing_tool(&app, &trace(n)).await;
    }
    capsules::prune(db, "7d", 3).await.unwrap();
    let left = |n| async move { capsules::get(db, &trace(n)).await.unwrap().is_some() };
    let kept = [left(10).await, left(11).await, left(12).await, left(13).await, left(14).await];
    assert_eq!(kept.iter().filter(|k| **k).count(), 3, "{kept:?}");
    assert!(kept[4] && kept[3] && kept[2], "the newest three survive: {kept:?}");

    db.query("UPDATE failure_capsule SET created_at = time::now() - 8d WHERE trace_id = $t").bind(("t", trace(14))).await.unwrap().check().unwrap();
    capsules::prune_default(db).await.unwrap();
    assert!(!left(14).await && left(13).await);
}

#[tokio::test]
async fn capsule_route_is_admin_only() {
    let app = TestApp::new().await; // first user: the admin
    failing_tool(&app, &trace(20)).await;
    let other = models_user::register_user(&app.state.db, "other@example.com", PASSWORD).await.unwrap();
    let other_token = models_user::create_api_token(&app.state.db, &other.id, "t").await.unwrap().token;
    let path = format!("/api/debug/capsules/{}", trace(20));

    let ((status, body), _) = http(&app.router, "GET", &path, None, Some(&other_token), None).await;
    assert_eq!((status.as_u16(), body["code"].as_str()), (403, Some("auth.forbidden")));

    let ((status, body), _) = http(&app.router, "GET", &path, None, Some(&app.token), None).await;
    assert_eq!(status, 200);
    assert_eq!(body["code"], "entity.not_found");
    assert!(!body.to_string().contains("SECRET"));

    let ((status, _), _) = http(&app.router, "GET", &format!("/api/debug/capsules/{}", trace(99)), None, Some(&app.token), None).await;
    assert_eq!(status, 404);
}

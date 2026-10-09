//! Defects found by the hands-on click-through: blank entity names, vault name rules.

mod common;

use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn entity_update_rejects_a_blank_name() {
    let app = TestApp::new().await;
    let w = app.tool("memory_write", json!({"subject_name": "Ada", "subject_kind": "person", "text": "Likes tea."})).await;
    let id = w["memory"]["entity_id"].as_str().or_else(|| w["entity"]["id"].as_str()).unwrap_or_else(|| panic!("{w}")).to_string();

    for bad in ["", "   "] {
        let (status, body) = app.http("PATCH", &format!("/api/entities/{id}"), Some(json!({"name": bad})), true).await;
        assert_eq!(status, 400, "{body}");
        assert_eq!(body["code"], "validation.invalid");
        let t = app.tool("entity_update", json!({"entity_id": id, "name": bad})).await;
        assert!(t["error"].as_str().unwrap().contains("can't be empty"), "{t}");
    }
    let (status, body) = app.http("PATCH", &format!("/api/entities/{id}"), Some(json!({"name": "  Ada L  "})), true).await;
    assert_eq!(status, 200);
    assert_eq!(body["name"], "Ada L");
}

#[tokio::test]
async fn vault_names_are_trimmed_bounded_and_unique() {
    let app = TestApp::new().await;
    for bad in ["", "   "] {
        let (s, b) = app.http("POST", "/api/vaults", Some(json!({"name": bad})), true).await;
        assert_eq!((s.as_u16(), b["code"].as_str()), (400, Some("validation.invalid")), "{b}");
    }
    let (s, _) = app.http("POST", "/api/vaults", Some(json!({"name": "x".repeat(81)})), true).await;
    assert_eq!(s, 400);

    // the personal vault already holds "Personal"
    let (s, b) = app.http("POST", "/api/vaults", Some(json!({"name": " personal "})), true).await;
    assert_eq!((s.as_u16(), b["code"].as_str()), (409, Some("vault.name_taken")), "{b}");

    let (s, b) = app.http("POST", "/api/vaults", Some(json!({"name": "  Acme "})), true).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["name"], "Acme");
    let acme = b["id"].as_str().unwrap().to_string();
    let (s, _) = app.http("POST", "/api/vaults", Some(json!({"name": "ACME"})), true).await;
    assert_eq!(s, 409);

    let (s, b) = app.http("POST", "/api/vaults", Some(json!({"name": "Other"})), true).await;
    assert_eq!(s, 200, "{b}");
    let other = b["id"].as_str().unwrap().to_string();
    // rename: blank and duplicate refused, own name (new case) and a fresh name accepted
    let (s, _) = app.http("PATCH", &format!("/api/vaults/{other}"), Some(json!({"name": " "})), true).await;
    assert_eq!(s, 400);
    let (s, b) = app.http("PATCH", &format!("/api/vaults/{other}"), Some(json!({"name": "acme"})), true).await;
    assert_eq!((s.as_u16(), b["code"].as_str()), (409, Some("vault.name_taken")), "{b}");
    let (s, _) = app.http("PATCH", &format!("/api/vaults/{acme}"), Some(json!({"name": "ACME"})), true).await;
    assert_eq!(s, 200);

    // the tools share the rule
    let t = app.tool("vault_create", json!({"name": "other"})).await;
    assert_eq!(t["code"], "vault.name_taken", "{t}");

    // a default-named clone twice gets distinct names
    let (s, a) = app.http("POST", &format!("/api/vaults/{acme}/clone"), Some(json!({})), true).await;
    assert_eq!(s, 200, "{a}");
    let (s, b) = app.http("POST", &format!("/api/vaults/{acme}/clone"), Some(json!({})), true).await;
    assert_eq!(s, 200, "{b}");
    assert_ne!(a["name"], b["name"]);
}

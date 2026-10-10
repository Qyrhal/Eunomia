//! Every tool's `vault_id` also accepts a vault's name (ported from main's mcp.spec.ts).

mod common;

use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn vaults_keep_separate_areas_apart_and_can_be_used_by_name() {
    let app = TestApp::new().await;
    assert_eq!(app.tool("vault_create", json!({"name": "Garden Project"})).await["name"], "Garden Project");
    app.tool("vault_create", json!({"name": "Book Club"})).await;

    // write and read by name, case-insensitively, no vault ids needed
    let w = app.tool("memory_write", json!({"subject_name": "Tomatoes", "subject_kind": "organisation", "text": "Tomatoes need staking by June.", "vault_id": "garden project"})).await;
    assert!(w["memory"].is_object(), "{w}");
    app.tool("memory_write", json!({"subject_name": "Dune", "subject_kind": "organisation", "text": "The club reads Dune in June.", "vault_id": "Book Club"})).await;

    let garden = app.tool("recall", json!({"query": "what happens in June?", "vault_id": "Garden Project"})).await;
    let texts: Vec<&str> = garden["results"].as_array().unwrap().iter().map(|r| r["text"].as_str().unwrap()).collect();
    assert_eq!(texts, ["Tomatoes need staking by June."]); // nothing from the other vault
    // nor the personal one
    assert!(app.tool("recall", json!({"query": "what happens in June?", "vault_id": "personal"})).await["results"].as_array().unwrap().is_empty());
    // no vault given: every org vault, each hit labelled with its vault
    let everywhere = app.tool("recall", json!({"query": "what happens in June?"})).await;
    let pairs: Vec<(String, String)> = everywhere["results"].as_array().unwrap().iter().map(|r| (r["vault_name"].as_str().unwrap().to_string(), r["text"].as_str().unwrap().to_string())).collect();
    assert!(pairs.contains(&("Garden Project".into(), "Tomatoes need staking by June.".into())), "{everywhere}");
    assert!(pairs.contains(&("Book Club".into(), "The club reads Dune in June.".into())), "{everywhere}");

    assert!(app.tool("recall", json!({"query": "June", "vault_id": "Nope"})).await["error"].as_str().unwrap().contains("no vault named"));
    assert_eq!(app.tool("vault_rename", json!({"vault_id": "Book Club", "name": "Reading Group"})).await["name"], "Reading Group");
    // "personal" names the personal vault; an id passes through unchanged
    assert!(app.tool("entities_search", json!({"query": "Tomatoes", "vault_id": "personal"})).await["results"].as_array().unwrap().is_empty());
}

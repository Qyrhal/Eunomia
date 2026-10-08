//! Hardening fixes from the foundation review: encryption-key guard, entity id kinds, payload filters.

mod common;

use common::{bare_state, register, test_settings, TestApp};
use serde_json::json;

#[tokio::test]
async fn empty_encryption_key_refuses_to_boot_on_a_fresh_install_but_not_over_existing_data() {
    let state = bare_state().await;
    let empty = eunomia_backend::config::Settings { encryption_key: String::new(), ..test_settings() };
    let err = eunomia_backend::connectors::crypto::guard_key(&empty, &state.control).await.unwrap_err();
    assert!(err.message.contains("ENCRYPTION_KEY is not set"), "{}", err.message);
    // a real key always passes
    eunomia_backend::connectors::crypto::guard_key(&test_settings(), &state.control).await.unwrap();
    // an install that already has an org was provisioned under the empty key: keep it reachable
    register(&state, "owner@example.com").await;
    eunomia_backend::connectors::crypto::guard_key(&empty, &state.control).await.unwrap();
}

#[tokio::test]
async fn entity_tools_refuse_ids_from_other_tables() {
    let app = TestApp::new().await;
    let w = app.tool("memory_write", json!({"subject_name": "Alice", "subject_kind": "person", "text": "likes tea"})).await;
    let entity = w["entity"]["id"].as_str().unwrap().to_string();
    let memory = w["memory"]["id"].as_str().unwrap().to_string();

    // a memory id where an entity id belongs, and the other way round
    for (tool, args) in [
        ("entity_update", json!({"entity_id": memory, "summary": "x"})),
        ("entity_delete", json!({"entity_id": memory})),
        ("entities_get", json!({"id": memory})),
        ("memory_update", json!({"memory_id": entity, "text": "x"})),
        ("memory_delete", json!({"memory_id": entity})),
        ("entity_update", json!({"entity_id": "vault_member:abc", "summary": "x"})),
    ] {
        let out = app.tool(tool, args).await;
        assert!(out["error"].is_string(), "{tool}: {out}");
    }
    // nothing changed
    let got = app.tool("entities_get", json!({"id": entity})).await;
    assert_eq!(got["memory"].as_array().unwrap().len(), 1, "{got}");
    assert_eq!(got["summary"], "");
}

#[tokio::test]
async fn payload_filters_use_a_nested_path_and_a_bind_each() {
    let app = TestApp::new().await;
    let db = app.db().await;
    for (n, a, b) in [("1", "x", "p"), ("2", "x", "q"), ("3", "y", "p")] {
        let env: eunomia_backend::cache::search::Envelope = serde_json::from_value(json!({
            "id": format!("demo:note:{n}"), "source": "demo", "type": "note", "external_id": n,
            "title": format!("note {n}"), "body_text": "body", "payload": {"a": a, "b": b},
        }))
        .unwrap();
        eunomia_backend::cache::search::upsert(&db, &app.user.id, &env).await.unwrap();
    }
    let ids = |v: &serde_json::Value| -> Vec<String> {
        let mut ids: Vec<String> = v["results"].as_array().unwrap_or(&vec![]).iter().map(|r| r["id"].as_str().unwrap_or_default().trim_start_matches("demo:note:").to_string()).collect();
        ids.sort();
        ids
    };
    let both = app.tool("list", json!({"filters": {"payload__a": "x", "payload__b": "q"}})).await;
    assert_eq!(ids(&both), ["2"], "{both}");
    let one = app.tool("list", json!({"filters": {"payload__a": "x"}})).await;
    assert_eq!(ids(&one), ["1", "2"], "{one}");
    // a path segment that is not an identifier never reaches the query text
    let bad = app.tool("list", json!({"filters": {"payload__a); DELETE cache_record; --": "x"}})).await;
    assert!(bad["error"].is_string(), "{bad}");
    let odd = app.tool("list", json!({"filters": {"title__x": "x"}})).await;
    assert!(odd["error"].is_string(), "{odd}");
}

#[tokio::test]
async fn org_passwords_written_under_the_empty_key_move_to_the_real_one() {
    use eunomia_backend::connectors::crypto::{decrypt_exact, encrypt, rotate_tenant_passwords_with};
    let state = bare_state().await;
    register(&state, "owner@example.com").await;
    let key = &state.settings.encryption_key;
    let mut res = state.control.test_raw().query("SELECT VALUE db_pass_enc FROM tenant").await.unwrap();
    let pass = decrypt_exact(key, &res.take::<Vec<String>>(0).unwrap()[0]).unwrap();
    // as an install from before the key was required would have stored it
    state.control.test_raw().query("UPDATE tenant SET db_pass_enc = $e").bind(("e", encrypt("", &pass))).await.unwrap();

    assert_eq!(rotate_tenant_passwords_with(&state.settings, &state.control, false).await.unwrap(), 0);
    assert_eq!(rotate_tenant_passwords_with(&state.settings, &state.control, true).await.unwrap(), 1);
    let mut res = state.control.test_raw().query("SELECT VALUE db_pass_enc FROM tenant").await.unwrap();
    assert_eq!(decrypt_exact(key, &res.take::<Vec<String>>(0).unwrap()[0]).unwrap(), pass);
    // idempotent
    assert_eq!(rotate_tenant_passwords_with(&state.settings, &state.control, true).await.unwrap(), 0);
}

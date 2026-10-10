//! Hardening fixes from the foundation review: encryption-key guard, entity id kinds, payload filters.

mod common;

use common::{bare_state, register, test_settings, TestApp};
use serde_json::json;

#[tokio::test]
async fn empty_encryption_key_refuses_to_boot_even_over_existing_data() {
    let state = bare_state().await;
    let empty = eunomia_backend::config::Settings { encryption_key: String::new(), ..test_settings() };
    let err = eunomia_backend::connectors::crypto::guard_key(&empty, &state.control).await.unwrap_err();
    assert!(err.message.contains("ENCRYPTION_KEY is not set"), "{}", err.message);
    // a real key always passes
    eunomia_backend::connectors::crypto::guard_key(&test_settings(), &state.control).await.unwrap();
    // an install that already has an org is refused too: under the empty key its control database
    // password is HMAC("", label) and its org passwords are encrypted with the all-zero key, both public
    register(&state, "owner@example.com").await;
    let err = eunomia_backend::connectors::crypto::guard_key(&empty, &state.control).await.unwrap_err();
    assert!(err.message.contains("ENCRYPTION_KEY_LEGACY_EMPTY=1"), "the refusal says how to keep old values readable: {}", err.message);
    // boot stops before it defines any database user with a password derived from the empty key
    let Err(err) = eunomia_backend::state::AppState::build(&empty, common::engine_config()).await else { panic!("booted on an empty key") };
    assert!(err.message.contains("ENCRYPTION_KEY is not set"), "{}", err.message);
}

/// The tenant row's database name and encrypted password (one org).
async fn tenant_row(state: &eunomia_backend::state::AppState) -> (String, String) {
    let mut res = state.control.test_raw().query("SELECT VALUE db FROM tenant; SELECT VALUE db_pass_enc FROM tenant").await.unwrap();
    (res.take::<Vec<String>>(0).unwrap().remove(0), res.take::<Vec<String>>(1).unwrap().remove(0))
}

/// Store the org's password under the empty key, as an install that ran without one did.
async fn store_under_empty_key(state: &eunomia_backend::state::AppState, pass: &str) {
    let enc = eunomia_backend::connectors::crypto::encrypt("", pass);
    state.control.test_raw().query("UPDATE tenant SET db_pass_enc = $e").bind(("e", enc)).await.unwrap().check().unwrap();
}

/// An org database password that sat under the empty key was readable by anyone who could reach the
/// control database: once a real key is set it is replaced, so the exposed one stops working.
#[tokio::test]
async fn org_passwords_stored_under_the_empty_key_are_replaced_not_just_re_encrypted() {
    use eunomia_backend::connectors::crypto::decrypt_exact;
    let state = bare_state().await;
    let user = register(&state, "owner@example.com").await;
    let key = state.settings.encryption_key.clone();
    let p = state.provisioner.as_ref().unwrap();
    let (db, enc) = tenant_row(&state).await;
    let old = decrypt_exact(&key, &enc).unwrap();
    store_under_empty_key(&state, &old).await;

    assert_eq!(p.replace_exposed_db_passwords(&state.control).await.unwrap(), 1);
    let new = decrypt_exact(&key, &tenant_row(&state).await.1).unwrap();
    assert_ne!(new, old);
    assert!(state.pool.test_session(user.org, &db, "app", &old).await.is_err(), "the exposed password still signs in");
    state.pool.test_session(user.org, &db, "app", &new).await.unwrap();
    state.pool.evict(&user.org);
    state.pool.for_org(&user.org).await.unwrap();
    assert_eq!(p.replace_exposed_db_passwords(&state.control).await.unwrap(), 0, "idempotent");

    // a crash after the user was redefined but before the row was written leaves the row under the
    // empty key: the next boot derives the same password, so the org converges instead of locking out
    store_under_empty_key(&state, &old).await;
    assert_eq!(p.replace_exposed_db_passwords(&state.control).await.unwrap(), 1);
    assert_eq!(decrypt_exact(&key, &tenant_row(&state).await.1).unwrap(), new);
    state.pool.evict(&user.org);
    state.pool.for_org(&user.org).await.unwrap();
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
    for filters in [json!({"payload__a); DELETE cache_record; --": "x"}), json!({"title__x": "x"})] {
        let out = common::sys(eunomia_backend::tools::registry::call(&app.state, &app.user, "list", json!({"filters": filters}))).await;
        assert_eq!(out.unwrap_err().code, eunomia_backend::error::ErrorCode::ValidationInvalid, "{filters}");
    }
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

//! Recall's semantic arm over remembered facts (issue #75, part 1): a fact is found by meaning when
//! no word of the query appears in it, in the personal vault and every shared vault the caller reads,
//! never in a vault they are not in, and only when its vector was made by the caller's current
//! provider and model. Without a provider, recall is unchanged.
//!
//! The stub backend's vectors carry no meaning, so the fact arm returns every embedded fact in a
//! vault: what these tests prove is that the arm runs, where it looks, and what it compares.

mod common;

use common::{test_settings, TestApp};
use eunomia_backend::config::Settings;
use eunomia_backend::embeddings::service as embeddings;
use eunomia_backend::sources::mock::{route, serve, Mock};
use eunomia_backend::tools::registry;
use serde_json::{json, Value};

/// No word in common with any fact below, nor with a subject name.
const QUERY: &str = "which profession does she practise";
const FACT: &str = "drafted analytical engine notes";

fn stub() -> Settings {
    Settings { embeddings_backend: "stub".into(), ..test_settings() }
}

async fn call(app: &TestApp, user: &eunomia_backend::models_user::User, name: &str, args: Value) -> Value {
    common::sys(registry::call(&app.state, user, name, args)).await.expect(name)
}

fn texts(recall: &Value) -> Vec<String> {
    recall["results"].as_array().unwrap().iter().map(|i| i["text"].as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn a_paraphrase_finds_a_fact_with_no_word_in_common() {
    let app = TestApp::with_settings(stub()).await;
    app.tool("memory_write", json!({"subject_name": "Byron Household", "subject_kind": "person", "text": FACT})).await;
    let r = app.tool("recall", json!({"query": QUERY})).await;
    assert_eq!(texts(&r), vec![FACT.to_string()], "{r}");
}

#[tokio::test]
async fn shared_vault_facts_are_found_and_other_vaults_never_leak() {
    let app = TestApp::with_settings(stub()).await;
    let alice = app.user.clone();
    let bob = common::register(&app.state, "bob@example.com").await;
    let carol = common::register(&app.state, "carol@example.com").await;

    // alice shares a vault with bob; carol keeps a vault and a personal fact bob is not in
    let shared = call(&app, &alice, "vault_create", json!({"name": "Shared"})).await["id"].as_str().unwrap().to_string();
    call(&app, &alice, "vault_invite", json!({"vault_id": shared, "email": bob.email})).await;
    common::sys(eunomia_backend::vaults::service::accept_invitation(&app.db().await, &bob.id, &eunomia_backend::rid::parse(&shared).unwrap()))
        .await
        .unwrap();
    call(&app, &alice, "memory_write", json!({"subject_name": "Byron Household", "subject_kind": "person", "text": FACT, "vault_id": shared})).await;
    let hidden = call(&app, &carol, "vault_create", json!({"name": "Hidden"})).await["id"].as_str().unwrap().to_string();
    call(&app, &carol, "memory_write", json!({"subject_name": "Hidden Person", "subject_kind": "person", "text": "CAROLVAULT secret plans", "vault_id": hidden})).await;
    call(&app, &carol, "memory_write", json!({"subject_name": "Hidden Person", "subject_kind": "person", "text": "CAROLPERSONAL secret plans"})).await;

    let r = call(&app, &bob, "recall", json!({"query": QUERY})).await;
    assert!(texts(&r).contains(&FACT.to_string()), "the shared fact is found: {r}");
    assert!(r["results"].as_array().unwrap().iter().all(|i| i["vault"] == shared.as_str()), "only the shared vault: {r}");
    assert!(!r.to_string().contains("CAROL"), "nothing from carol's vaults: {r}");
    // narrowed to the shared vault, the same
    let narrowed = call(&app, &bob, "recall", json!({"query": QUERY, "vault_id": shared})).await;
    assert_eq!(texts(&narrowed), vec![FACT.to_string()], "{narrowed}");
}

async fn embedder() -> Mock {
    serve(vec![route("POST", "/embeddings", json!({"data": [{"index": 0, "embedding": vec![0.01_f32; embeddings::DIM]}]}))]).await
}

async fn use_provider(app: &TestApp, base: &str) {
    let (status, body) = app.http("PATCH", "/api/settings", Some(json!({"openai_base_url": base})), true).await;
    assert_eq!(status, 200, "{body}");
}

#[tokio::test]
async fn a_fact_embedded_by_another_provider_is_not_compared() {
    let (a, b) = (embedder().await, embedder().await);
    let app = TestApp::new().await;
    use_provider(&app, &a.base).await;
    let old = app.tool("memory_write", json!({"subject_name": "Byron Household", "subject_kind": "person", "text": FACT})).await;
    assert_eq!(a.requests().len(), 1, "the fact was embedded by provider a");

    use_provider(&app, &b.base).await;
    let later = "keeps bees on weekends";
    app.tool("memory_write", json!({"subject_name": "Mary Shelley", "subject_kind": "person", "text": later})).await;
    let r = app.tool("recall", json!({"query": QUERY})).await;
    assert_eq!(texts(&r), vec![later.to_string()], "only provider b's vector is compared: {r}");

    // editing the old fact re-embeds it under provider b, and then it is found
    app.tool("memory_update", json!({"memory_id": old["memory"]["id"], "text": FACT.to_uppercase()})).await;
    let r = app.tool("recall", json!({"query": QUERY})).await;
    assert_eq!(texts(&r).len(), 2, "{r}");
}

#[tokio::test]
async fn without_a_provider_recall_is_unchanged() {
    let app = TestApp::new().await; // no key, OpenAI's URL: nothing is configured
    app.tool("memory_write", json!({"subject_name": "Byron Household", "subject_kind": "person", "text": FACT})).await;
    let r = app.tool("recall", json!({"query": QUERY})).await;
    assert!(texts(&r).is_empty(), "no semantic arm without a provider: {r}");
    let r = app.tool("recall", json!({"query": "analytical engine"})).await;
    assert_eq!(texts(&r), vec![FACT.to_string()], "{r}");
    assert_eq!(r["results"][0]["arms_hit"], 1, "found by the memory text arm alone: {r}");
}

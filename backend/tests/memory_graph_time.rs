//! Recall, issue #75 part 2: the graph arm follows one `relates_to` hop from the entities a query
//! names, never into a vault the caller cannot read; a fact is dated by when it happened
//! (`occurred_at`), not when it was stored; a superseded fact is left out of recall but kept.
//!
//! No embedding provider here, so the semantic arms are off: a hit comes from the graph, temporal or
//! memory text arm only, and no fact below shares a word with the query that should not find it.

mod common;

use common::TestApp;
use eunomia_backend::models_user::User;
use eunomia_backend::rid;
use eunomia_backend::tools::registry;
use serde_json::{json, Value};

async fn call(app: &TestApp, user: &User, name: &str, args: Value) -> Value {
    common::sys(registry::call(&app.state, user, name, args)).await.expect(name)
}

fn texts(recall: &Value) -> Vec<String> {
    recall["results"].as_array().unwrap().iter().map(|i| i["text"].as_str().unwrap().to_string()).collect()
}

/// Writes `text` about a person `name` and returns the person's entity id.
async fn fact(app: &TestApp, user: &User, name: &str, text: &str) -> String {
    call(app, user, "memory_write", json!({"subject_name": name, "subject_kind": "person", "text": text})).await["entity"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn relate(app: &TestApp, from: &str, to: &str) {
    let r = app.tool("code_relate", json!({"from_id": from, "to_id": to, "label": "knows"})).await;
    assert!(r.get("error").is_none(), "{r}");
}

/// An edge written straight to the database, as an older install could hold one across vaults.
async fn forge_edge(app: &TestApp, from: &str, to: &str) {
    app.db()
        .await
        .test_raw()
        .query("RELATE $a->relates_to->$b SET label = 'forged', owner = $o")
        .bind(("a", rid::parse(from).unwrap()))
        .bind(("b", rid::parse(to).unwrap()))
        .bind(("o", app.user.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn a_related_entitys_evidence_is_found_through_one_hop() {
    let app = TestApp::new().await;
    let user = app.user.clone();
    let ada = fact(&app, &user, "Ada Lovelace", "wrote the first program").await;
    let charles = fact(&app, &user, "Charles Babbage", "designed the difference engine").await;
    relate(&app, &ada, &charles).await;

    let r = app.tool("recall", json!({"query": "Ada Lovelace"})).await;
    let got = texts(&r);
    assert!(got.contains(&"wrote the first program".to_string()), "{r}");
    assert!(got.contains(&"designed the difference engine".to_string()), "the related entity's fact: {r}");
    assert_eq!(got[0], "wrote the first program", "the named entity ranks first: {r}");

    // the edge is followed either way round
    let r = app.tool("recall", json!({"query": "Charles Babbage"})).await;
    assert!(texts(&r).contains(&"wrote the first program".to_string()), "{r}");
}

#[tokio::test]
async fn the_hop_is_one_edge_deep() {
    let app = TestApp::new().await;
    let user = app.user.clone();
    let a = fact(&app, &user, "Ada Lovelace", "wrote the first program").await;
    let b = fact(&app, &user, "Charles Babbage", "designed the difference engine").await;
    let c = fact(&app, &user, "Mary Somerville", "translated Laplace into English").await;
    relate(&app, &a, &b).await;
    relate(&app, &b, &c).await;

    let got = texts(&app.tool("recall", json!({"query": "Ada Lovelace"})).await);
    assert!(got.contains(&"designed the difference engine".to_string()), "{got:?}");
    assert!(!got.contains(&"translated Laplace into English".to_string()), "two hops away: {got:?}");
}

#[tokio::test]
async fn an_edge_into_an_unreadable_vault_is_never_followed() {
    let app = TestApp::new().await;
    let alice = app.user.clone();
    let carol = common::register(&app.state, "carol@example.com").await;
    let hidden = call(&app, &carol, "vault_create", json!({"name": "Hidden"})).await["id"].as_str().unwrap().to_string();
    let ada = fact(&app, &alice, "Ada Lovelace", "wrote the first program").await;
    let secret = call(
        &app,
        &carol,
        "memory_write",
        json!({"subject_name": "Charles Babbage", "subject_kind": "person", "text": "CAROLVAULT plans", "vault_id": hidden}),
    )
    .await["entity"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let personal = fact(&app, &carol, "Mary Somerville", "CAROLPERSONAL plans").await;
    for other in [&secret, &personal] {
        forge_edge(&app, &ada, other).await;
        forge_edge(&app, other, &ada).await;
    }

    let r = app.tool("recall", json!({"query": "Ada Lovelace"})).await;
    assert_eq!(texts(&r), vec!["wrote the first program".to_string()], "{r}");
    assert!(!r.to_string().contains("CAROL"), "{r}");
}

fn days_ago(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339()
}

#[tokio::test]
async fn a_fact_is_dated_by_when_it_happened() {
    let app = TestApp::new().await;
    let last_year = days_ago(365);
    let w = app
        .tool(
            "memory_write",
            json!({"subject_name": "Ada Lovelace", "subject_kind": "person", "text": "moved to Marylebone", "occurred_at": last_year}),
        )
        .await;
    assert!(w["memory"]["occurred_at"].is_string(), "{w}");
    // stored today, without an event time
    app.tool("memory_write", json!({"subject_name": "Ada Lovelace", "subject_kind": "person", "text": "joined a chess club"})).await;

    // a query no arm but the temporal one answers
    let in_range = |since: String, until: String| {
        let app = &app;
        async move { texts(&app.tool("recall", json!({"query": "zzqx", "time_range": [since, until]})).await) }
    };
    assert_eq!(in_range(days_ago(400), days_ago(300)).await, vec!["moved to Marylebone".to_string()], "last year's event");
    assert_eq!(in_range(days_ago(7), days_ago(-1)).await, vec!["joined a chess club".to_string()], "this week's fact only");

    let r = app.tool("recall", json!({"query": "Marylebone"})).await;
    assert_eq!(r["results"][0]["occurred_at"].as_str().unwrap()[..10], last_year[..10], "recall shows the event date: {r}");

    let bad = app
        .tool("memory_write", json!({"subject_name": "Ada Lovelace", "subject_kind": "person", "text": "x", "occurred_at": "last tuesday"}))
        .await;
    assert!(bad.get("error").is_some(), "{bad}");
}

#[tokio::test]
async fn a_superseded_fact_is_left_out_of_recall_but_kept() {
    let app = TestApp::new().await;
    let old = app.tool("memory_write", json!({"subject_name": "Ada Lovelace", "subject_kind": "person", "text": "lives in Marylebone"})).await;
    app.tool("memory_write", json!({"subject_name": "Ada Lovelace", "subject_kind": "person", "text": "lives in Ockham"})).await;
    app.db()
        .await
        .test_raw()
        .query("UPDATE $id SET status = 'superseded'")
        .bind(("id", rid::parse(old["memory"]["id"].as_str().unwrap()).unwrap()))
        .await
        .unwrap()
        .check()
        .unwrap();

    let got = texts(&app.tool("recall", json!({"query": "Ada Lovelace lives"})).await);
    assert_eq!(got, vec!["lives in Ockham".to_string()]);
    let entity = app.tool("entities_get", json!({"id": old["entity"]["id"]})).await;
    assert!(entity.to_string().contains("lives in Marylebone"), "history stays reachable: {entity}");
}

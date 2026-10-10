//! A clone or merge that fails part way leaves no target vault and no copied rows; one a crash left
//! behind is deleted by the stale-copy cleanup; the normal results keep the merge rules. Its own test
//! binary: the failure switch (`FAIL_COPY_AT`) is process-global.

mod common;

use std::sync::atomic::Ordering;

use common::TestApp;
use eunomia_backend::pool::OrgDb;
use eunomia_backend::vaults::service::{self, FAIL_COPY_AT};
use serde_json::{json, Value};
use surrealdb::types::RecordId;

const TABLES: [&str; 10] = ["vault", "vault_member", "memory", "person", "organisation", "location", "repository", "file", "symbol", "relates_to"];

fn s(v: &Value, ptr: &str) -> String {
    v.pointer(ptr).and_then(Value::as_str).unwrap_or_else(|| panic!("no string at {ptr} in {v}")).to_string()
}

fn rid(s: &str) -> RecordId {
    eunomia_backend::rid::parse(s).unwrap()
}

/// The rows of `sql` (one `SELECT ... FROM ONLY` row as itself, NONE as null).
async fn query(db: &OrgDb, sql: &str, vault: &RecordId) -> Value {
    let mut res = db.test_raw().query(sql).bind(("vault", vault.clone())).await.unwrap();
    if sql.contains("FROM ONLY") {
        res.take::<Option<Value>>(0).unwrap().unwrap_or(Value::Null)
    } else {
        Value::Array(res.take::<Vec<Value>>(0).unwrap())
    }
}

/// Rows in every table a copy writes.
async fn totals(db: &OrgDb) -> Vec<(&'static str, i64)> {
    let mut out = Vec::new();
    for t in TABLES {
        let mut res = db.test_raw().query(format!("SELECT count() FROM {t} GROUP ALL")).await.unwrap();
        out.push((t, res.take::<Option<Value>>(0).unwrap().and_then(|v| v["count"].as_i64()).unwrap_or(0)));
    }
    out
}

async fn vault_names(app: &TestApp) -> Vec<String> {
    let mut names: Vec<String> = app.tool("vault_list", json!({})).await["results"].as_array().map(|vs| vs.iter().map(|v| s(v, "/name")).collect()).unwrap_or_default();
    names.sort();
    names
}

async fn write(app: &TestApp, vault: &str, name: &str, kind: &str, text: &str, mem_type: &str) -> String {
    let out = app.tool("memory_write", json!({"vault_id": vault, "subject_name": name, "subject_kind": kind, "text": text, "type": mem_type})).await;
    s(&out, "/entity/id")
}

/// Alice (two facts, one observation in each vault, one fact shared), Acme, and Alice works_at Acme in both.
async fn seed(app: &TestApp) -> (String, String) {
    let a = s(&app.tool("vault_create", json!({"name": "Alpha", "kind": "org"})).await, "/id");
    let b = s(&app.tool("vault_create", json!({"name": "Beta", "kind": "org"})).await, "/id");
    for (vault, alias, fact, obs, acme_fact) in
        [(&a, "Al", "Alice paints", "Alice is calm", "Acme builds anvils"), (&b, "Ally", "Alice runs", "Alice is fast", "Acme is in Sydney")]
    {
        let alice = write(app, vault, "Alice", "person", "Alice likes tea", "world").await;
        write(app, vault, "Alice", "person", fact, "world").await;
        write(app, vault, "Alice", "person", obs, "observation").await;
        let acme = write(app, vault, "Acme", "organisation", acme_fact, "world").await;
        app.tool("entity_update", json!({"entity_id": alice, "aliases": [alias]})).await;
        let rel = app.tool("code_relate", json!({"from_id": alice, "to_id": acme, "label": "works_at"})).await;
        assert!(rel.get("error").is_none(), "{rel}");
    }
    (a, b)
}

#[tokio::test]
async fn failed_copies_leave_nothing_and_finished_ones_keep_the_merge_rules() {
    let app = TestApp::new().await;
    let db = app.db().await;
    let (a, b) = seed(&app).await;
    let before = (totals(&db).await, vault_names(&app).await);

    // clone fails at its second memory (entities and a memory already written)
    FAIL_COPY_AT.store(2, Ordering::SeqCst);
    let out = app.tool("vault_clone", json!({"vault_id": a, "name": "Alpha Copy"})).await;
    assert!(out.get("error").is_some(), "{out}");
    assert_eq!(FAIL_COPY_AT.load(Ordering::SeqCst), 0, "the injected failure fired");
    assert_eq!((totals(&db).await, vault_names(&app).await), before, "a failed clone leaves no vault and no rows");

    // merge fails in its second source, after the first was copied whole and folding began
    FAIL_COPY_AT.store(5, Ordering::SeqCst);
    let out = app.tool("vault_merge", json!({"vault_id_a": a, "vault_id_b": b, "name": "Merged"})).await;
    assert!(out.get("error").is_some(), "{out}");
    assert_eq!(FAIL_COPY_AT.load(Ordering::SeqCst), 0, "the injected failure fired");
    assert_eq!((totals(&db).await, vault_names(&app).await), before, "a failed merge leaves no vault and no rows");

    // the same calls without the failure: a ready vault the caller owns, with everything in it
    let clone = app.tool("vault_clone", json!({"vault_id": a, "name": "Alpha Copy"})).await;
    assert_eq!(clone["entities_copied"], 2, "{clone}");
    let c = rid(&s(&clone, "/id"));
    assert_eq!(query(&db, "SELECT VALUE text FROM memory WHERE vault = $vault ORDER BY text", &c).await,
        json!(["Acme builds anvils", "Alice is calm", "Alice likes tea", "Alice paints"]));
    assert_eq!(query(&db, "SELECT VALUE label FROM relates_to WHERE in.vault = $vault", &c).await, json!(["works_at"]));
    assert_eq!(query(&db, "SELECT VALUE status FROM ONLY $vault", &c).await, Value::Null, "the copying marker is gone");

    let merged = app.tool("vault_merge", json!({"vault_id_a": a, "vault_id_b": b, "name": "Merged"})).await;
    assert_eq!(merged["entities"], 2, "{merged}");
    let m = rid(&s(&merged, "/id"));
    let alice = query(&db, "SELECT aliases, (SELECT VALUE text FROM memory WHERE subject = $parent.id AND type != 'observation' ORDER BY text) AS facts, \
        (SELECT VALUE text FROM memory WHERE subject = $parent.id AND type = 'observation') AS obs FROM person WHERE vault = $vault", &m).await;
    let alice = &alice[0];
    let mut aliases: Vec<String> = serde_json::from_value(alice["aliases"].clone()).unwrap();
    aliases.sort();
    assert_eq!(aliases, ["Al", "Ally"], "aliases unioned");
    assert_eq!(alice["facts"], json!(["Alice likes tea", "Alice paints", "Alice runs"]), "identical facts kept once");
    let obs = alice["obs"].as_array().unwrap();
    assert_eq!(obs.len(), 1, "one observation per subject: {alice}");
    let obs = obs[0].as_str().unwrap();
    assert!(obs.contains("Alice is calm") && obs.contains("Alice is fast"), "observations joined: {obs}");
    assert_eq!(query(&db, "SELECT VALUE label FROM relates_to WHERE in.vault = $vault", &m).await, json!(["works_at"]), "relations deduplicated");
    let mut names = vault_names(&app).await;
    names.retain(|n| n != "Personal");
    assert_eq!(names, ["Alpha", "Alpha Copy", "Beta", "Merged"]);

    // a copy a crash left behind is deleted once stale; one still in progress is not
    let staged = |age: &str| {
        let db = db.clone();
        let sql = format!(
            "LET $v = (CREATE ONLY vault SET name = 'half', kind = 'org', status = 'copying', created_at = time::now() - {age}).id; \
             LET $p = (CREATE ONLY person SET owner = $owner, vault = $v, name = 'Zed').id; \
             CREATE memory SET owner = $owner, vault = $v, subject = $p, text = 'Zed exists'; RETURN $v;"
        );
        let owner = app.user.id.clone();
        async move { db.test_raw().query(sql).bind(("owner", owner)).await.unwrap().take::<Option<RecordId>>(3).unwrap().unwrap() }
    };
    let (stale, fresh) = (staged("2h").await, staged("1m").await);
    assert_eq!(service::discard_stale_copies(&db).await.unwrap(), 1);
    assert_eq!(query(&db, "SELECT VALUE id FROM memory WHERE vault = $vault", &stale).await, json!([]));
    assert_eq!(query(&db, "SELECT VALUE id FROM person WHERE vault = $vault", &stale).await, json!([]));
    assert_eq!(query(&db, "SELECT * FROM $vault", &stale).await, json!([]));
    assert_eq!(query(&db, "SELECT VALUE status FROM ONLY $vault", &fresh).await, json!("copying"), "a copy in progress stays");
}

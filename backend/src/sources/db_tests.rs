//! Sync pipeline tests against a real SurrealDB and mock providers: the
//! scheduler, the shared ingest + enrichment path, cursors, leases and the
//! webhook route. Skipped unless `EUNOMIA_TEST_SURREAL_URL` (e.g.
//! `ws://127.0.0.1:8000/rpc`) is set; each test gets its own database.

use std::sync::Arc;

use axum::body::Body;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::RecordId;
use tower::ServiceExt;

use crate::cache::search;
use crate::config::Settings;
use crate::connectors::service;
use crate::db::Db;
use crate::sources::base::owner_key_str;
use crate::sources::mock::{route, serve, Mock};
use crate::sources::registry;
use crate::sources::scheduler::{poll_all, sync_source};
use crate::sources::heypocket::HeyPocketSource;
use crate::sources::up_bank::UpBankSource;
use crate::state::{AppState, AppStateInner};

async fn test_db() -> Option<(Db, Settings)> {
    let Ok(url) = std::env::var("EUNOMIA_TEST_SURREAL_URL") else {
        eprintln!("skipped: set EUNOMIA_TEST_SURREAL_URL to run");
        return None;
    };
    let mut settings = Settings::load();
    settings.surreal_url = url;
    settings.surreal_ns = "connectors_test".into();
    settings.surreal_db = format!("t{}", uuid::Uuid::new_v4().simple());
    settings.encryption_key = "test-key".into();
    settings.embeddings_backend = "openai".into();
    settings.openai_api_key = None;
    let db = crate::db::connect(&settings).await.unwrap();
    crate::db::ensure_schema(&db, &settings).await.unwrap();
    Some((db, settings))
}

async fn new_user(db: &Db) -> RecordId {
    #[derive(Deserialize)]
    struct Id {
        id: RecordId,
    }
    let email = format!("{}@example.com", uuid::Uuid::new_v4().simple());
    let mut res = db.query("CREATE user SET email = $e, password_hash = 'x' RETURN id").bind(("e", email)).await.unwrap();
    let owner = res.take::<Vec<Id>>(0).unwrap().remove(0).id;
    crate::vaults::service::create_personal_vault(db, &owner).await.unwrap();
    owner
}

async fn connect_github(db: &Db, settings: &Settings, owner: &RecordId, base: &str) {
    service::upsert_connector(
        db,
        &settings.encryption_key,
        owner,
        "github",
        Some(true),
        Some(json!({"base_url": base})),
        Some(json!({"personal_access_token": "ghp_test"})),
    )
    .await
    .unwrap();
}

async fn status(db: &Db, owner: &RecordId, key: &str) -> Value {
    let id = RecordId::from_table_key("sync_status", format!("{}:{key}", owner_key_str(owner)));
    let mut res = db.query("SELECT last_error, cursor, consecutive_failures FROM ONLY $id").bind(("id", id)).await.unwrap();
    res.take::<Option<Value>>(0).unwrap().unwrap_or(Value::Null)
}

async fn count(db: &Db, sql: &str, owner: &RecordId) -> i64 {
    let mut res = db.query(format!("{sql} GROUP ALL")).bind(("owner", owner.clone())).await.unwrap();
    res.take::<Option<Value>>(0).unwrap().and_then(|v| v["count"].as_i64()).unwrap_or(0)
}

fn issue(number: i64, title: &str, updated: &str) -> Value {
    json!({
        "number": number, "title": title, "state": "open",
        "body": "Ada Lovelace reported that the quarterly importer drops the last CSV row.",
        "user": {"login": "ada"}, "labels": [], "assignees": [], "comments": 0, "repository": {"full_name": "acme/widget"},
        "html_url": format!("https://github.com/acme/widget/issues/{number}"),
        "created_at": "2024-01-01T00:00:00Z", "updated_at": updated,
    })
}

const ISSUE_7: &str = "github:github.issue:acme/widget#7";

#[tokio::test]
async fn scheduled_sync_lands_searchable_records_then_surfaces_a_401() {
    let Some((db, settings)) = test_db().await else { return };
    let owner = new_user(&db).await;
    let mock = serve(vec![route("GET", "/issues", json!([issue(7, "Importer drops the last row", "2024-02-01T00:00:00Z")]))]).await;
    connect_github(&db, &settings, &owner, &mock.base).await;

    assert_eq!(poll_all(&db, &settings).await.unwrap().len(), 1, "a never-synced connector is due at once");
    let st = status(&db, &owner, "github").await;
    assert_eq!(st["last_error"], "");
    assert_eq!(st["cursor"], "2024-02-01T00:00:00Z");
    assert_eq!(mock.requests()[0].header("authorization"), "Bearer ghp_test");

    let params = search::SearchParams { mode: "keyword".into(), limit: 10, ..Default::default() };
    let (hits, _more) = search::search(&db, &settings, &owner, "importer quarterly", &params).await.unwrap();
    assert_eq!(hits.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), vec![ISSUE_7]);
    assert!(poll_all(&db, &settings).await.unwrap().is_empty(), "a healthy source is not polled again within its interval");

    let bad = serve(vec![route("GET", "/issues", json!({"message": "Bad credentials"})).status(401)]).await;
    service::upsert_connector(&db, &settings.encryption_key, &owner, "github", None, Some(json!({"base_url": bad.base})), None)
        .await
        .unwrap();
    let out = sync_source(&db, &settings, &owner, "github").await;
    assert!(out["error"].as_str().unwrap().contains("HTTP 401"));
    let st = status(&db, &owner, "github").await;
    assert!(st["last_error"].as_str().unwrap().contains("Bad credentials"));
    assert_eq!(st["consecutive_failures"], 1);
    assert_eq!(st["cursor"], "2024-02-01T00:00:00Z", "a failed sync keeps the cursor");
}

/// #67: disabling stops scheduled runs without a restart; two overlapping
/// runs make one provider sync.
#[tokio::test]
async fn disabled_connectors_are_not_polled_and_concurrent_runs_do_not_overlap() {
    let Some((db, settings)) = test_db().await else { return };
    let owner = new_user(&db).await;
    let mock = serve(vec![route("GET", "/issues", json!([issue(7, "Slow", "2024-02-01T00:00:00Z")])).delay(300)]).await;
    connect_github(&db, &settings, &owner, &mock.base).await;

    service::upsert_connector(&db, &settings.encryption_key, &owner, "github", Some(false), None, None).await.unwrap();
    assert!(poll_all(&db, &settings).await.unwrap().is_empty());
    assert!(mock.requests().is_empty(), "disabled: no provider request");

    let run = || {
        let (db, settings, owner) = (db.clone(), settings.clone(), owner.clone());
        tokio::spawn(async move { sync_source(&db, &settings, &owner, "github").await })
    };
    let (a, b) = tokio::join!(run(), run());
    let (a, b) = (a.unwrap(), b.unwrap());
    let statuses = [a["status"].as_str(), b["status"].as_str()];
    assert!(statuses.contains(&Some("already_running")), "{a} {b}");
    assert_eq!(mock.requests().len(), 1, "one provider sync");
    assert_eq!(status(&db, &owner, "github").await["cursor"], "2024-02-01T00:00:00Z");
    // The lease is released afterwards.
    assert!(sync_source(&db, &settings, &owner, "github").await.get("status").is_none());

    // A lease held by another replica blocks this one until it expires.
    let lease = RecordId::from_table_key("sync_lease", format!("{}:github", owner_key_str(&owner)));
    db.query("CREATE $id SET until = time::now() + 5m").bind(("id", lease.clone())).await.unwrap().check().unwrap();
    assert_eq!(sync_source(&db, &settings, &owner, "github").await["status"], "already_running");
    db.query("UPDATE $id SET until = time::now() - 1s").bind(("id", lease)).await.unwrap().check().unwrap();
    assert!(sync_source(&db, &settings, &owner, "github").await.get("status").is_none(), "a stale lease is reclaimed");
    assert_eq!(mock.requests().len(), 3);
}

/// #66: a failed write keeps the previous cursor; the retry recovers the
/// record without duplicating the ones already stored; a checkpoint write
/// error is reported, not a clean sync.
#[tokio::test]
async fn a_failed_write_keeps_the_cursor_and_the_retry_recovers_without_duplicates() {
    let Some((db, settings)) = test_db().await else { return };
    let owner = new_user(&db).await;
    let mock = serve(vec![route(
        "GET",
        "/issues",
        json!([issue(7, "Fine", "2024-02-01T00:00:00Z"), issue(8, "POISON", "2024-02-02T00:00:00Z")]),
    )])
    .await;
    connect_github(&db, &settings, &owner, &mock.base).await;
    db.query("DEFINE FIELD OVERWRITE title ON cache_record TYPE string DEFAULT '' ASSERT $value != 'POISON'").await.unwrap();

    let out = sync_source(&db, &settings, &owner, "github").await;
    assert!(out["error"].as_str().unwrap().contains("1 of 2 records failed to save"), "{out}");
    let st = status(&db, &owner, "github").await;
    assert_eq!(st["cursor"], "", "cursor not advanced past the failed record");
    assert_eq!(st["consecutive_failures"], 1);

    db.query("DEFINE FIELD OVERWRITE title ON cache_record TYPE string DEFAULT ''").await.unwrap();
    let out = sync_source(&db, &settings, &owner, "github").await;
    assert_eq!(out["written"], 1, "{out}");
    assert_eq!(out["skipped"], 1, "the stored record is replayed, not duplicated");
    assert_eq!(count(&db, "SELECT count() FROM cache_record WHERE owner = $owner", &owner).await, 2);
    assert_eq!(status(&db, &owner, "github").await["cursor"], "2024-02-02T00:00:00Z");

    // Checkpoint write failure: visible, not a clean sync.
    db.query("DEFINE FIELD OVERWRITE cursor ON sync_status TYPE string DEFAULT '' ASSERT $value != '2024-02-02T00:00:00Z'").await.unwrap();
    db.query("UPDATE sync_status SET cursor = ''").await.unwrap();
    let out = sync_source(&db, &settings, &owner, "github").await;
    assert!(out["error"].as_str().unwrap().contains("checkpoint"), "{out}");
    assert!(status(&db, &owner, "github").await["last_error"].as_str().unwrap().contains("checkpoint"));
}

fn openai_mock_routes() -> Vec<crate::sources::mock::Route> {
    let extraction = json!({"people": [{"name": "Ada Lovelace", "facts": ["Ada reported the importer bug"]}]});
    vec![
        route("POST", "/embeddings", json!({"data": [{"index": 0, "embedding": vec![0.01_f32; crate::embeddings::service::DIM]}]})),
        route("POST", "/chat/completions", json!({"choices": [{"message": {"content": extraction.to_string()}}]})).body("Extract entities"),
        route("POST", "/chat/completions", json!({"choices": [{"message": {"content": "{\"belief\": \"Ada reports importer bugs.\"}"}}]})),
    ]
}

async fn point_openai_at(db: &Db, owner: &RecordId, base: &str) {
    db.query("UPSERT type::thing('app_settings', $k) SET owner = $owner, openai_base_url = $base")
        .bind(("k", owner_key_str(owner)))
        .bind(("owner", owner.clone()))
        .bind(("base", base.to_string()))
        .await
        .unwrap()
        .check()
        .unwrap();
}

/// #63: one ingested record gets an embedding, a source-linked extracted
/// fact and an observation, via the same path every sync uses.
#[tokio::test]
async fn a_synced_record_is_embedded_and_its_entities_extracted_and_consolidated() {
    let Some((db, mut settings)) = test_db().await else { return };
    settings.openai_api_key = Some("sk-test".into());
    let owner = new_user(&db).await;
    let mut routes = openai_mock_routes();
    routes.push(route("GET", "/issues", json!([issue(7, "Importer drops the last row", "2024-02-01T00:00:00Z")])));
    let mock = serve(routes).await;
    point_openai_at(&db, &owner, &mock.base).await;
    connect_github(&db, &settings, &owner, &mock.base).await;

    let out = sync_source(&db, &settings, &owner, "github").await;
    assert_eq!(out["written"], 1, "{out}");
    assert_eq!(count(&db, "SELECT count() FROM cache_record WHERE owner = $owner AND embedding != NONE", &owner).await, 1);
    let rec = search::rid(&owner, ISSUE_7);
    let mut res = db.query("SELECT VALUE text FROM memory WHERE source = $rec AND type = 'world'").bind(("rec", rec)).await.unwrap();
    assert_eq!(res.take::<Vec<String>>(0).unwrap(), vec!["Ada reported the importer bug".to_string()]);
    let mut res = db.query("SELECT VALUE text FROM memory WHERE owner = $owner AND type = 'observation'").bind(("owner", owner.clone())).await.unwrap();
    assert_eq!(res.take::<Vec<String>>(0).unwrap(), vec!["Ada reports importer bugs.".to_string()]);
}

/// #63: with no model configured raw data is stored and nothing calls out;
/// an embedding provider outage leaves work that replaying the unchanged
/// record repairs.
#[tokio::test]
async fn no_model_stores_raw_data_and_a_failed_embedding_is_repaired_on_replay() {
    let Some((db, mut settings)) = test_db().await else { return };
    let owner = new_user(&db).await;
    let github = serve(vec![route("GET", "/issues", json!([issue(7, "Importer", "2024-02-01T00:00:00Z")]))]).await;
    connect_github(&db, &settings, &owner, &github.base).await;

    let out = sync_source(&db, &settings, &owner, "github").await;
    assert_eq!(out["written"], 1);
    assert_eq!(out["errors"], json!([]), "no model: no provider call was attempted");
    assert_eq!(count(&db, "SELECT count() FROM cache_record WHERE owner = $owner AND embedding = NONE", &owner).await, 1);

    // Embedding provider down: the record is still stored, the sync is ok.
    settings.openai_api_key = Some("sk-test".into());
    let down = serve(vec![route("POST", "/embeddings", json!({"error": "overloaded"})).status(503)]).await;
    point_openai_at(&db, &owner, &down.base).await;
    let out = sync_source(&db, &settings, &owner, "github").await;
    assert!(out["errors"][0].as_str().unwrap().starts_with("embed"), "{out}");
    assert_eq!(status(&db, &owner, "github").await["last_error"], "");

    // Provider back: replaying the unchanged record repairs its embedding.
    let up: Mock = serve(openai_mock_routes()).await;
    point_openai_at(&db, &owner, &up.base).await;
    let out = sync_source(&db, &settings, &owner, "github").await;
    assert_eq!(out["skipped"], 1, "{out}");
    assert_eq!(count(&db, "SELECT count() FROM cache_record WHERE owner = $owner AND embedding != NONE", &owner).await, 1);
}

/// Pocket: everything a recording carries is stored whole and readable
/// through `get`; the cache side is chunked, embedded and entity-extracted
/// with the server's own chat model; a shorter re-sync tombstones the
/// stale chunks.
#[tokio::test]
async fn a_pocket_recording_is_stored_whole_chunked_embedded_and_extracted() {
    let Some((db, mut settings)) = test_db().await else { return };
    settings.openai_api_key = Some("sk-test".into());
    let owner = new_user(&db).await;
    let mut routes = openai_mock_routes();
    routes.push(route("GET", "/models", json!({"data": [{"id": "nomic-embed-text"}, {"id": "llama3.1"}]})));
    let mock = serve(routes).await;
    point_openai_at(&db, &owner, &mock.base).await;

    let mut rec = json!({
        "id": "rec_1", "title": "Importer review", "recording_at": "2024-05-02T09:00:00Z", "duration": 900,
        "tags": [{"id": "t1", "name": "work"}], "speakers": {"s1": {"name": "Ada Lovelace", "speakerId": "s1"}},
        "transcript": {"segments": [
            {"speaker": "s1", "start": 0, "end": 1000, "text": "Ada here, the importer drops the last row."},
            {"speaker": "s2", "speakerName": "Grace", "start": 1000, "end": 9000, "text": "word ".repeat(1500)},
        ]},
        "summarizations": {"sm1": {"v2": {"summary": {"markdown": "Agreed to fix the importer."},
                                          "actionItems": {"items": [{"title": "Ship the fix"}]}}}},
    });
    let report = registry::ingest(&db, &settings, &owner, &[rec.clone()], &HeyPocketSource).await.unwrap();
    assert_eq!((report.written, report.failed), (4, 0), "recording + 3 chunks: {:?}", report.errors);

    let got = crate::tools::generic::get(&db, &owner, "heypocket:heypocket.transcript_chunk:rec_1:2").await.unwrap();
    let stored = &got["recording"];
    assert_eq!(stored["summary"], "Agreed to fix the importer.", "{got}");
    assert_eq!(stored["action_items"], json!(["Ship the fix"]));
    assert_eq!(stored["tags"], json!(["work"]));
    assert_eq!(stored["speakers"], json!(["Ada Lovelace", "Grace"]));
    let transcript = stored["transcript"].as_str().unwrap();
    assert!(transcript.starts_with("Ada Lovelace: Ada here, the importer drops the last row.\nGrace: word word"), "{transcript}");
    assert_eq!(transcript.matches("word").count(), 1500);
    assert_eq!(count(&db, "SELECT count() FROM pocket_recording WHERE owner = $owner AND raw.summarizations.sm1.v2 != NONE", &owner).await, 1);

    assert_eq!(count(&db, "SELECT count() FROM cache_record WHERE owner = $owner AND embedding != NONE", &owner).await, 4);
    assert!(count(&db, "SELECT count() FROM memory WHERE owner = $owner AND source != NONE", &owner).await >= 1);
    let chats: Vec<_> = mock.requests().into_iter().filter(|r| r.path == "/chat/completions").collect();
    assert!(!chats.is_empty() && chats.iter().all(|r| r.body.contains("\"llama3.1\"")), "the server's own chat model draws the relations");

    // Another user's `get` sees nothing.
    let other = new_user(&db).await;
    assert_eq!(crate::tools::generic::get(&db, &other, "heypocket:heypocket.recording:rec_1").await.unwrap()["error"], "not found");

    rec["transcript"]["segments"] = json!([{"speaker": "s1", "text": "Short now."}]);
    registry::ingest(&db, &settings, &owner, &[rec.clone()], &HeyPocketSource).await.unwrap();
    assert_eq!(count(&db, "SELECT count() FROM cache_record WHERE owner = $owner AND deleted = false", &owner).await, 2);

    // Deleting the connector's data takes everything derived from it, and
    // nothing from another source or another user.
    registry::ingest(&db, &settings, &other, &[rec], &HeyPocketSource).await.unwrap();
    let up = json!({"type": "transactions", "id": "t1",
        "attributes": {"description": "Coles", "createdAt": "2024-03-01T00:00:00+11:00", "status": "SETTLED",
                       "amount": {"value": "-5.00", "valueInBaseUnits": -500, "currencyCode": "AUD"}},
        "relationships": {"account": {"data": {"id": "acc"}}}});
    registry::ingest(&db, &settings, &owner, &[up], &UpBankSource).await.unwrap();
    let up_memories = count(&db, "SELECT count() FROM memory WHERE owner = $owner AND source.source = 'up_bank'", &owner).await;
    let out = registry::delete_data(&db, &owner, &HeyPocketSource).await.unwrap();
    assert_eq!(out["records"], 4, "{out}");
    assert!(out["memories"].as_i64().unwrap() >= 1, "{out}");
    assert_eq!(count(&db, "SELECT count() FROM cache_record WHERE owner = $owner AND source = 'heypocket'", &owner).await, 0);
    assert_eq!(count(&db, "SELECT count() FROM pocket_recording WHERE owner = $owner", &owner).await, 0);
    assert_eq!(count(&db, "SELECT count() FROM memory WHERE owner = $owner AND source.source = 'heypocket'", &owner).await, 0);
    assert_eq!(count(&db, "SELECT count() FROM linked_to WHERE in.source = 'heypocket' AND in.owner = $owner", &owner).await, 0);
    assert_eq!(count(&db, "SELECT count() FROM cache_record WHERE owner = $owner AND source = 'up_bank'", &owner).await, 1);
    assert_eq!(count(&db, "SELECT count() FROM memory WHERE owner = $owner AND source.source = 'up_bank'", &owner).await, up_memories);
    assert_eq!(count(&db, "SELECT count() FROM pocket_recording WHERE owner = $owner", &other).await, 1);
    assert_eq!(count(&db, "SELECT count() FROM cache_record WHERE owner = $owner AND source = 'heypocket'", &other).await, 2);
}

/// #63: changed links are reconciled without duplicating the raw record.
#[tokio::test]
async fn link_changes_are_reconciled_without_duplicating_records() {
    let Some((db, settings)) = test_db().await else { return };
    let owner = new_user(&db).await;
    let txn = |cat: &str| {
        json!({"type": "transactions", "id": "t1",
               "attributes": {"description": "Coles", "createdAt": "2024-03-01T00:00:00+11:00", "status": "SETTLED",
                              "amount": {"value": "-5.00", "valueInBaseUnits": -500, "currencyCode": "AUD"}},
               "relationships": {"account": {"data": {"id": "acc"}}, "category": {"data": {"id": cat}}}})
    };
    registry::ingest(&db, &settings, &owner, &[txn("groceries")], &UpBankSource).await.unwrap();
    registry::ingest(&db, &settings, &owner, &[txn("takeaway")], &UpBankSource).await.unwrap();
    assert_eq!(count(&db, "SELECT count() FROM cache_record WHERE owner = $owner", &owner).await, 1);
    let links = search::links(&db, &owner, "up_bank:up.transaction:t1", Some("category")).await.unwrap();
    let json = serde_json::to_string(&serde_json::to_value(&links).unwrap()).unwrap();
    assert!(json.contains("takeaway") && !json.contains("groceries"), "{json}");
}

/// #66 + #76: a signed webhook is ingested; one whose records can't be
/// stored gets a retryable 503.
#[tokio::test]
async fn signed_webhooks_ingest_and_failed_persistence_is_retryable() {
    let Some((db, settings)) = test_db().await else { return };
    let owner = new_user(&db).await;
    let txn = |desc: &str| {
        json!({"data": {"type": "transactions", "id": "t9",
               "attributes": {"description": desc, "createdAt": "2024-03-01T00:00:00+11:00", "status": "HELD",
                              "amount": {"value": "-5.00", "valueInBaseUnits": -500, "currencyCode": "AUD"}},
               "relationships": {}}})
    };
    let mock = serve(vec![route("GET", "/transactions/t9", txn("POISON")).query("v=1"), route("GET", "/transactions/t9", txn("Coles"))]).await;
    service::upsert_connector(
        &db,
        &settings.encryption_key,
        &owner,
        "up_bank",
        Some(true),
        None,
        Some(json!({"personal_access_token": "up:yeah:x", "webhook_secret_key": "shh"})),
    )
    .await
    .unwrap();
    let state = AppState(Arc::new(AppStateInner { db: db.clone(), settings: settings.clone() }));
    let deliver = |related: String| {
        let body = json!({"data": {"attributes": {"eventType": "TRANSACTION_CREATED"},
            "relationships": {"transaction": {"data": {"id": "t9"}, "links": {"related": related}}}}})
        .to_string();
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"shh").unwrap();
        mac.update(body.as_bytes());
        axum::http::Request::post(format!("/sources/up_bank/webhook/{owner}"))
            .header("X-Up-Authenticity-Signature", hex::encode(mac.finalize().into_bytes()))
            .body(Body::from(body))
            .unwrap()
    };

    db.query("DEFINE FIELD OVERWRITE title ON cache_record TYPE string DEFAULT '' ASSERT $value != 'POISON'").await.unwrap();
    let app = crate::routers::sources::webhook_router().with_state(state.clone());
    let resp = app.oneshot(deliver(format!("{}/transactions/t9?v=1", mock.base))).await.unwrap();
    assert_eq!(resp.status(), 503);

    let app = crate::routers::sources::webhook_router().with_state(state);
    let resp = app.oneshot(deliver(format!("{}/transactions/t9", mock.base))).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(count(&db, "SELECT count() FROM cache_record WHERE owner = $owner AND title = 'Coles'", &owner).await, 1);
}

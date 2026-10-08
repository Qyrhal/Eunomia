//! Versioned schema runner: fresh apply, upgrade from the legacy replay, dedupe, idempotence, tamper.
mod common;

use common::test_settings;
use eunomia_backend::db::{self, Db};
use eunomia_backend::migrate;
use serde_json::Value;

/// Every statement the pre-ledger `ensure_schema` replayed on boot (198, incl. the 6 unique indexes).
const LEGACY_SCHEMA: &str = include_str!("legacy_schema.txt");

async fn fresh() -> Db {
    db::connect(&test_settings()).await.expect("mem db")
}

async fn info(db: &Db) -> Value {
    let mut r = db.query("INFO FOR DB STRUCTURE").await.expect("info");
    let mut v: Value = r.take::<Option<Value>>(0).expect("take").expect("row");
    // The ledger is bookkeeping, not schema: legacy-then-migrate has it, a raw legacy DB does not.
    for k in ["tables"] {
        if let Some(a) = v.get_mut(k).and_then(Value::as_array_mut) {
            a.retain(|t| t["name"] != "_migration");
        }
    }
    v
}

/// Row count for a `SELECT` tail, e.g. `count(&db, "person WHERE vault = vault:v")`.
async fn count(db: &Db, from: &str) -> usize {
    let v: Option<usize> =
        db.query(format!("RETURN array::len((SELECT VALUE id FROM {from}))")).await.unwrap().take(0).unwrap();
    v.unwrap()
}

#[tokio::test]
async fn fresh_schema_matches_snapshot() {
    let db = fresh().await;
    migrate::migrate(&db, &test_settings()).await.unwrap();
    insta::assert_json_snapshot!(info(&db).await);
}

#[tokio::test]
async fn upgraded_equals_fresh() {
    let settings = test_settings();
    let legacy = fresh().await;
    legacy.query(LEGACY_SCHEMA).await.unwrap().check().unwrap();
    legacy
        .query(
            "CREATE user:u SET email = 'a@b.c', password_hash = 'x';
             CREATE vault:v SET name = 'v';
             CREATE person:a SET owner = user:u, vault = vault:v, name = 'Ann';
             CREATE memory:m SET owner = user:u, vault = vault:v, subject = person:a, text = 'hi';",
        )
        .await
        .unwrap()
        .check()
        .unwrap();
    migrate::migrate(&legacy, &settings).await.unwrap();
    assert_eq!(count(&legacy, "memory").await, 1);

    let new = fresh().await;
    migrate::migrate(&new, &settings).await.unwrap();
    assert_eq!(info(&legacy).await, info(&new).await);
}

#[tokio::test]
async fn duplicates_are_merged_into_oldest() {
    let db = fresh().await;
    migrate::apply_up_to(&db, 1).await.unwrap();
    db.query(
        "CREATE user:u SET email = 'a@b.c', password_hash = 'x';
         CREATE vault:v SET name = 'v';
         CREATE vault:w SET name = 'w';
         CREATE organisation:o SET owner = user:u, vault = vault:v, name = 'Acme';
         CREATE person:a SET owner = user:u, vault = vault:v, name = 'Ann', created_at = d'2024-01-01T00:00:00Z';
         CREATE person:b SET owner = user:u, vault = vault:v, name = 'Ann', aliases = ['A', 'Ann'], created_at = d'2024-02-01T00:00:00Z';
         CREATE person:c SET owner = user:u, vault = vault:v, name = 'Ann', created_at = d'2024-03-01T00:00:00Z';
         CREATE person:other SET owner = user:u, vault = vault:w, name = 'Ann';
         CREATE memory:m1 SET owner = user:u, vault = vault:v, subject = person:a, text = 'one';
         CREATE memory:m2 SET owner = user:u, vault = vault:v, subject = person:b, text = 'two';
         CREATE memory:m3 SET owner = user:u, vault = vault:v, subject = person:c, text = 'three';
         RELATE person:b->relates_to->organisation:o SET label = 'works_at';
         RELATE person:c->relates_to->person:a SET label = 'knows';",
    )
    .await
    .unwrap()
    .check()
    .unwrap();

    migrate::migrate(&db, &test_settings()).await.unwrap();

    assert_eq!(count(&db, "person WHERE vault = vault:v").await, 1);
    assert_eq!(count(&db, "person:a").await, 1, "oldest survives");
    assert_eq!(count(&db, "person:other").await, 1, "other vault untouched");
    assert_eq!(count(&db, "memory WHERE subject = person:a").await, 3);
    assert_eq!(count(&db, "relates_to WHERE in = person:a AND out = organisation:o").await, 1);
    assert_eq!(count(&db, "relates_to").await, 1, "self-loop dropped");
    let aliases: Option<Vec<String>> = db.query("SELECT VALUE aliases FROM person:a").await.unwrap().take(0).unwrap();
    assert_eq!(aliases.unwrap(), vec!["A".to_string()]);
    // The unique index now exists and bites.
    assert!(db
        .query("CREATE person SET owner = user:u, vault = vault:v, name = 'Ann'")
        .await
        .unwrap()
        .check()
        .is_err());
}

#[tokio::test]
async fn rerun_is_noop() {
    let db = fresh().await;
    let s = test_settings();
    migrate::migrate(&db, &s).await.unwrap();
    let before = info(&db).await;
    migrate::migrate(&db, &s).await.unwrap();
    assert_eq!(before, info(&db).await);
    assert_eq!(count(&db, "_migration").await, 2);
}

#[tokio::test]
async fn tampered_checksum_is_a_hard_error() {
    let db = fresh().await;
    let s = test_settings();
    migrate::migrate(&db, &s).await.unwrap();
    db.query("UPDATE _migration SET checksum = 'tampered' WHERE version = 1").await.unwrap().check().unwrap();
    let err = migrate::migrate(&db, &s).await.unwrap_err().to_string();
    assert!(err.contains("edited after it was applied"), "{err}");
}

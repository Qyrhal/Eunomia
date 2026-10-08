//! Versioned schema runner: fresh apply, upgrade from a 2.x export, dedupe, idempotence, tamper.
mod common;

use common::test_settings;
use eunomia_backend::migrate;
use serde_json::Value;

type Db = surrealdb::Surreal<surrealdb::engine::any::Any>;

/// A root session on an empty, non-strict scratch database: the migration runner's own view of a
/// database (tenancy aside), the same way a legacy single-database install is seen.
async fn fresh() -> Db {
    let state = common::bare_state().await;
    state.provisioner.as_ref().expect("provisioning feature").scratch("scratch").await.unwrap()
}

async fn info(db: &Db) -> Value {
    let mut r = db.query("INFO FOR DB STRUCTURE").await.expect("info");
    let mut v: Value = r.take::<Option<Value>>(0).expect("take").expect("row");
    // The ledger is bookkeeping, not schema: legacy-then-migrate has it, a raw legacy DB does not.
    if let Some(a) = v.get_mut("tables").and_then(Value::as_array_mut) {
        a.retain(|t| t["name"] != "_migration");
        for t in a.iter_mut() {
            // Table ids follow creation order, which differs between a fresh and an imported database.
            t.as_object_mut().unwrap().remove("id");
            // Indexes live on the table, not in the database overview.
            let name = t["name"].as_str().unwrap().to_string();
            let mut r = db.query(format!("INFO FOR TABLE `{name}` STRUCTURE")).await.expect("table info");
            let info: Value = r.take::<Option<Value>>(0).expect("take").expect("row");
            t["indexes"] = info["indexes"].clone();
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
async fn control_schema_matches_snapshot_and_reapplies_cleanly() {
    let db = fresh().await;
    migrate::migrate_control(&db).await.unwrap();
    let first = info(&db).await;
    migrate::migrate_control(&db).await.unwrap();
    assert_eq!(first, info(&db).await, "re-running the control migrations changes nothing");
    assert_eq!(count(&db, "_migration").await, migrate::CONTROL_MIGRATIONS.len());
    insta::assert_json_snapshot!(first);
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
    assert_eq!(count(&db, "_migration").await, migrate::MIGRATIONS.len());
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

#[tokio::test]
async fn existing_tokens_and_sessions_survive_0003() {
    let db = fresh().await;
    migrate::apply_up_to(&db, 2).await.unwrap();
    db.query(
        "CREATE user:u SET email = 'a@b.c', password_hash = 'x';
         CREATE api_token:t SET owner = user:u, token_hash = 'h';
         CREATE session:s SET owner = user:u, sid = 'sid';",
    )
    .await
    .unwrap()
    .check()
    .unwrap();
    // the legacy database is only ever taken this far: 0009 drops the tables the move reads
    migrate::apply_up_to(&db, migrate::LEGACY_TENANT_VERSION).await.unwrap();
    let scopes: Option<Vec<String>> = db.query("SELECT VALUE scopes FROM api_token:t").await.unwrap().take(0).unwrap();
    assert_eq!(scopes.unwrap(), ["memory:read", "memory:write", "vaults:admin", "connectors"]);
    assert_eq!(count(&db, "api_token WHERE expires_at = NONE AND vault = NONE").await, 1, "non-expiring, unrestricted");
    assert_eq!(count(&db, "session WHERE expires_at > time::now()").await, 1, "existing sessions get an expiry");
}

// ---- SurrealDB 2.x to 3.x upgrade -------------------------------------------------------------
//
// An upgraded install is `surreal v2 export --v3` of the 2.x database, imported into a fresh 3.x
// one. The fixture below has that shape: the schema 2.x had after migrations 1 to 7, the
// `_migration` ledger rows 2.x recorded (0001 and 0005 carry their old 2.x checksums), the
// converter's index output, and seed rows. Regenerate it with
//   cargo test --test migrations -- --ignored regenerate_v2_fixture
// NOTE: it is produced by the embedded 3.x engine's own export from a database built to look like
// the converted state; the index lines are our reading of what the converter emits, not a capture
// of a real 2.7 export. See docs/surrealdb-3-upgrade.md for the real export commands.

const V2_FIXTURE: &str = include_str!("fixtures/v2_export_converted.surql");
const LEGACY_SUMS: [(u32, &str); 2] = [
    (1, "31199597d8ffdb899e26dd741ec3886f6af03467f65710f4c7a1db2bfc3ce5a5"),
    (5, "7227cfd0e1a0f672f6938c4bada0b9d7c720e42ff06ff7d8d93bfd11f231acd2"),
];

fn unit_vector(i: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; 1536];
    v[i] = 1.0;
    v
}

#[tokio::test]
#[ignore = "writes tests/fixtures/v2_export_converted.surql"]
async fn regenerate_v2_fixture() {
    let db = fresh().await;
    // Everything 2.x had applied: all migrations except the 3.x index one (the highest version).
    let last_2x = migrate::MIGRATIONS.iter().map(|m| m.0).filter(|v| *v < 8).max().unwrap();
    migrate::apply_up_to(&db, last_2x).await.unwrap();
    for (v, sum) in LEGACY_SUMS {
        db.query("UPDATE _migration SET checksum = $c WHERE version = $v").bind(("c", sum)).bind(("v", v)).await.unwrap().check().unwrap();
    }
    db.query(
        "DEFINE INDEX cache_record_embedding_idx ON cache_record FIELDS embedding HNSW DIMENSION 1536 DIST COSINE TYPE F32;
         DEFINE INDEX cache_record_fts_idx ON cache_record FIELDS title FULLTEXT ANALYZER cache_text_analyzer BM25 HIGHLIGHTS;
         DEFINE INDEX memory_text_fts_idx ON memory FIELDS text FULLTEXT ANALYZER cache_text_analyzer BM25;
         CREATE user:u SET email = 'a@b.c', password_hash = 'x';
         CREATE vault:v SET name = 'Personal', kind = 'personal';
         CREATE vault_member:vm SET vault = vault:v, user = user:u, role = 'owner';
         CREATE user:u2 SET email = 'Zed@Example.com', password_hash = 'x', created_at = d'2026-10-08T13:41:00Z';
         CREATE user:u3 SET email = 'zed@example.COM', password_hash = 'x', created_at = d'2026-10-08T13:42:00Z';
         CREATE vault_member:vm2 SET vault = vault:v, user = user:u2, role = 'member';
         CREATE vault_member:vm3 SET vault = vault:v, user = user:u3, role = 'member', status = 'pending';
         CREATE person:ann SET owner = user:u, vault = vault:v, name = 'Ann';
         CREATE organisation:acme SET owner = user:u, vault = vault:v, name = 'Acme';
         RELATE person:ann->relates_to->organisation:acme SET label = 'works_at';
         CREATE memory:m1 SET owner = user:u, vault = vault:v, subject = person:ann, text = 'Ann likes green tea';
         CREATE memory:m2 SET owner = user:u, vault = vault:v, subject = person:ann, text = 'Ann works remotely';",
    )
    .bind(("e1", unit_vector(0)))
    .await
    .unwrap()
    .check()
    .unwrap();
    for (i, (id, title, body)) in [("r1", "Woolworths groceries", "weekly shop"), ("r2", "Netflix subscription", "monthly streaming")].iter().enumerate() {
        db.query("CREATE $rid SET owner = user:u, source = 'demo', type = 'note', external_id = $id, title = $t, body_text = $b, content_hash = 'h', ingested_at = time::now(), updated_at = time::now(), embedding = $e")
            .bind(("rid", surrealdb::types::RecordId::new("cache_record", *id)))
            .bind(("id", *id))
            .bind(("t", *title))
            .bind(("b", *body))
            .bind(("e", unit_vector(i)))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v2_export_converted.surql");
    db.export(path).await.unwrap();
}

#[tokio::test]
async fn v2_export_upgrades_to_the_fresh_schema() {
    let settings = test_settings();
    let db = fresh().await;
    db.query(V2_FIXTURE).await.unwrap().check().unwrap(); // what `surreal import` does
    assert_eq!(count(&db, "_migration").await, 7, "arrives carrying the 2.x ledger");

    // Old checksums for 0001 and 0005 are accepted; the 3.x index migration then applies.
    migrate::migrate(&db, &settings).await.unwrap();
    assert_eq!(count(&db, "_migration").await, migrate::MIGRATIONS.len());
    assert_eq!(count(&db, "memory").await, 2);
    assert_eq!(count(&db, "cache_record").await, 2);
    assert_eq!(count(&db, "relates_to").await, 1);

    // Same schema, indexes included, as a database that never ran 2.x.
    let new = fresh().await;
    migrate::migrate(&new, &settings).await.unwrap();
    assert_eq!(info(&db).await, info(&new).await);

    // The re-defined indexes answer queries over the imported rows (the vector index is exercised
    // end to end by tenancy.rs, which moves this same fixture into an org database).
    let hits: Vec<surrealdb::types::RecordId> =
        db.query("SELECT VALUE id FROM cache_record WHERE title @1@ 'netflix'").await.unwrap().take(0).unwrap();
    assert_eq!(hits.len(), 1);
    let mems: Vec<surrealdb::types::RecordId> = db.query("SELECT VALUE id FROM memory WHERE text @1@ 'tea'").await.unwrap().take(0).unwrap();
    assert_eq!(mems.len(), 1);
}

/// Two replicas booting together both run the control and tenant migrations on the same empty
/// database. The loser used to hit the unique `_migration.version` index inside the migration
/// transaction and the boot panicked; now it sees the other process applied the version and moves on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_migrations_on_one_database_both_succeed() {
    let state = common::bare_state().await;
    let p = state.provisioner.as_ref().expect("provisioning feature");
    for round in 0..6 {
        let ctl = p.scratch(&format!("race_ctl_{round}")).await.unwrap();
        let (a, b) = (ctl.clone(), ctl.clone());
        let (ra, rb) = tokio::join!(tokio::spawn(async move { migrate::migrate_control(&a).await }), tokio::spawn(async move { migrate::migrate_control(&b).await }));
        ra.unwrap().unwrap_or_else(|e| panic!("round {round}, first control migrator: {e}"));
        rb.unwrap().unwrap_or_else(|e| panic!("round {round}, second control migrator: {e}"));
        assert_eq!(count(&ctl, "_migration").await, migrate::CONTROL_MIGRATIONS.len());

        let ten = p.scratch(&format!("race_ten_{round}")).await.unwrap();
        let (a, b, settings) = (ten.clone(), ten.clone(), test_settings());
        let s2 = settings.clone();
        let (ra, rb) = tokio::join!(tokio::spawn(async move { migrate::migrate(&a, &settings).await }), tokio::spawn(async move { migrate::migrate(&b, &s2).await }));
        ra.unwrap().unwrap_or_else(|e| panic!("round {round}, first tenant migrator: {e}"));
        rb.unwrap().unwrap_or_else(|e| panic!("round {round}, second tenant migrator: {e}"));
        assert_eq!(count(&ten, "_migration").await, migrate::MIGRATIONS.len());
    }
}

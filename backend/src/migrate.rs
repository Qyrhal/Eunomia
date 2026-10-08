//! Versioned schema runner. Ordered `migrations/tenant/*.surql` files are applied once each
//! and recorded in the `_migration` ledger (version, name, sha256 checksum, applied_at).

use sha2::{Digest, Sha256};
use surrealdb::sql::Datetime;
use surrealdb::RecordId;
use serde::Deserialize;

use crate::config::Settings;
use crate::db::Db;

const MIGRATIONS: &[(u32, &str, &str)] = &[
    (1, "baseline", include_str!("../migrations/tenant/0001_baseline.surql")),
    (2, "entity_name_unique", include_str!("../migrations/tenant/0002_entity_name_unique.surql")),
    (4, "oauth", include_str!("../migrations/tenant/0004_oauth.surql")),
];

const LEDGER: &str = "DEFINE TABLE IF NOT EXISTS _migration SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS version ON _migration TYPE int;
DEFINE FIELD IF NOT EXISTS name ON _migration TYPE string;
DEFINE FIELD IF NOT EXISTS checksum ON _migration TYPE string;
DEFINE FIELD IF NOT EXISTS applied_at ON _migration TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS _migration_version_unique ON _migration FIELDS version UNIQUE;";

const ENTITY_TABLES: [&str; 6] = ["person", "organisation", "location", "repository", "file", "symbol"];

#[derive(Deserialize)]
struct Applied {
    version: u32,
    checksum: String,
}

#[derive(Deserialize)]
struct Entity {
    id: RecordId,
    vault: RecordId,
    name: String,
    aliases: Vec<String>,
}

#[derive(Deserialize)]
struct Edge {
    #[serde(rename = "in")]
    in_: RecordId,
    out: RecordId,
    label: String,
    owner: Option<RecordId>,
    source: Option<RecordId>,
    created_at: Datetime,
}

fn checksum(sql: &str) -> String {
    hex::encode(Sha256::digest(sql.as_bytes()))
}

/// Apply every pending migration, then the settings-dependent field default.
pub async fn migrate(db: &Db, settings: &Settings) -> surrealdb::Result<()> {
    apply_up_to(db, u32::MAX).await?;
    // The one config-dependent definition: re-applied each boot, outside the ledger.
    db.query(format!(
        "DEFINE FIELD OVERWRITE openai_base_url ON app_settings TYPE string DEFAULT \"{}\";",
        settings.openai_base_url.replace('"', "\\\"")
    ))
    .await?
    .check()?;
    Ok(())
}

/// Apply pending migrations with `version <= max`. Public for tests that need a half-migrated DB.
pub async fn apply_up_to(db: &Db, max: u32) -> surrealdb::Result<()> {
    db.query(LEDGER).await?.check()?;
    let applied: Vec<Applied> = db.query("SELECT version, checksum FROM _migration").await?.take(0)?;
    for &(version, name, sql) in MIGRATIONS.iter().filter(|m| m.0 <= max) {
        let sum = checksum(sql);
        if let Some(a) = applied.iter().find(|a| a.version == version) {
            if a.checksum != sum {
                return Err(surrealdb::Error::Db(surrealdb::error::Db::Thrown(format!(
                    "migration {version:04}_{name} was edited after it was applied (checksum {} != {sum}). \
                     Applied migrations are immutable: revert the file and add a new migration instead.",
                    a.checksum
                ))));
            }
            continue;
        }
        if version == 2 {
            dedupe_entity_names(db).await?;
        }
        // DEFINE is allowed inside a transaction, so schema and ledger row commit together.
        db.query(format!(
            "BEGIN TRANSACTION;\n{sql}\nCREATE _migration SET version = $v, name = $n, checksum = $c;\nCOMMIT TRANSACTION;"
        ))
        .bind(("v", version))
        .bind(("n", name))
        .bind(("c", sum))
        .await?
        .check()?;
        tracing::info!(version, name, "applied migration");
    }
    Ok(())
}

/// 0002 pre-step: merge entities sharing (vault, name) into the oldest, so the UNIQUE index can build.
/// Mirrors `entities::service::merge_entities`. Idempotent: a crash midway just resumes next boot.
async fn dedupe_entity_names(db: &Db) -> surrealdb::Result<()> {
    for table in ENTITY_TABLES {
        let rows: Vec<Entity> = db
            .query(format!("SELECT id, vault, name, aliases, created_at FROM {table} ORDER BY created_at, id"))
            .await?
            .take(0)?;
        let mut groups: Vec<Vec<Entity>> = Vec::new();
        for e in rows {
            match groups.iter_mut().find(|g| g[0].vault == e.vault && g[0].name == e.name) {
                Some(g) => g.push(e),
                None => groups.push(vec![e]),
            }
        }
        // ponytail: O(n * groups) grouping, fine for a one-off pass; use a map if installs get huge.
        for group in groups.into_iter().filter(|g| g.len() > 1) {
            let mut it = group.into_iter();
            let mut winner = it.next().expect("non-empty group");
            for loser in it {
                merge_into(db, &mut winner, &loser).await?;
            }
        }
    }
    Ok(())
}

async fn merge_into(db: &Db, winner: &mut Entity, loser: &Entity) -> surrealdb::Result<()> {
    db.query("UPDATE memory SET subject = $w WHERE subject = $l")
        .bind(("w", winner.id.clone()))
        .bind(("l", loser.id.clone()))
        .await?
        .check()?;
    // Relation endpoints are immutable: re-create each edge on the winner (the (in, out, label)
    // UNIQUE index drops ones it already has, so errors here are expected), then drop the loser's.
    let edges: Vec<Edge> = db
        .query("SELECT * FROM relates_to WHERE in = $l OR out = $l")
        .bind(("l", loser.id.clone()))
        .await?
        .take(0)?;
    for e in edges {
        let from = if e.in_ == loser.id { winner.id.clone() } else { e.in_.clone() };
        let to = if e.out == loser.id { winner.id.clone() } else { e.out.clone() };
        if from == to {
            continue;
        }
        let _ = db
            .query("RELATE $from->relates_to->$to SET label = $label, owner = $owner, source = $source, created_at = $at")
            .bind(("from", from))
            .bind(("to", to))
            .bind(("label", e.label))
            .bind(("owner", e.owner))
            .bind(("source", e.source))
            .bind(("at", e.created_at))
            .await;
    }
    for a in &loser.aliases {
        if *a != winner.name && !winner.aliases.contains(a) {
            winner.aliases.push(a.clone());
        }
    }
    db.query("UPDATE $w SET aliases = $a; DELETE relates_to WHERE in = $l OR out = $l; DELETE $l;")
        .bind(("w", winner.id.clone()))
        .bind(("a", winner.aliases.clone()))
        .bind(("l", loser.id.clone()))
        .await?
        .check()?;
    Ok(())
}

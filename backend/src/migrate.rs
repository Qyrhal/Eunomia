//! Versioned schema runner. Ordered `migrations/{control,tenant}/*.surql` files are applied once each
//! and recorded in the `_migration` ledger (version, name, sha256 checksum, applied_at) of the database
//! they run in: the control database once at boot, each org database when it is provisioned and by the
//! `migrate_tenant` job after an upgrade. These functions take a root session (see `provisioning/`).

use sha2::{Digest, Sha256};
use surrealdb::types::{Datetime, RecordId, SurrealValue};

use crate::config::Settings;
use crate::db::Db;
use crate::store::root;

pub const MIGRATIONS: &[(u32, &str, &str)] = &[
    (1, "baseline", include_str!("../migrations/tenant/0001_baseline.surql")),
    (2, "entity_name_unique", include_str!("../migrations/tenant/0002_entity_name_unique.surql")),
    (3, "auth", include_str!("../migrations/tenant/0003_auth.surql")),
    (4, "oauth", include_str!("../migrations/tenant/0004_oauth.surql")),
    (5, "jobs", include_str!("../migrations/tenant/0005_jobs.surql")),
    (6, "oauth_code_redeemed", include_str!("../migrations/tenant/0006_oauth_code_redeemed.surql")),
    (7, "capsules", include_str!("../migrations/tenant/0007_capsules.surql")),
    (8, "v3_indexes", include_str!("../migrations/tenant/0008_v3_indexes.surql")),
    (9, "drop_control_tables", include_str!("../migrations/tenant/0009_drop_control_tables.surql")),
];

/// The tenant schema version this code writes. An org database is current at this version.
pub const LATEST_TENANT: u32 = MIGRATIONS[MIGRATIONS.len() - 1].0;

/// The oldest tenant schema this code still serves (N-1), so a rolling upgrade can run the new code
/// while the per-org migration jobs catch up. Below it, `tenant.schema_behind`.
pub const MIN_SUPPORTED_TENANT: u32 = LATEST_TENANT - 1;

/// Migrations for the control database.
pub const CONTROL_MIGRATIONS: &[(u32, &str, &str)] = &[
    (1, "control", include_str!("../migrations/control/0001_control.surql")),
    (2, "oauth_token_scope", include_str!("../migrations/control/0002_oauth_token_scope.surql")),
    (3, "audit_actor_system", include_str!("../migrations/control/0003_audit_actor_system.surql")),
    (4, "email_lc_and_owner_slot", include_str!("../migrations/control/0004_email_lc_and_owner_slot.surql")),
];

/// The last tenant migration the legacy single database may be brought to before its data moves out
/// (0009 drops the tables the move is about to read).
pub const LEGACY_TENANT_VERSION: u32 = 8;

/// Checksums of migration files as they were applied on SurrealDB 2.x, before being rewritten to
/// 3.x-valid syntax (0001: MTREE and SEARCH ANALYZER indexes moved to the v3 indexes migration;
/// 0005: `FLEXIBLE TYPE` became `TYPE ... FLEXIBLE`). An install upgraded with
/// `surreal v2 export --v3` arrives carrying the ledger rows recorded by the old files, so accept
/// them; the v3 indexes migration redefines whatever the converter produced.
const LEGACY_CHECKSUMS: &[(u32, &str)] = &[
    (1, "31199597d8ffdb899e26dd741ec3886f6af03467f65710f4c7a1db2bfc3ce5a5"),
    (5, "7227cfd0e1a0f672f6938c4bada0b9d7c720e42ff06ff7d8d93bfd11f231acd2"),
];

const LEDGER: &str = "DEFINE TABLE IF NOT EXISTS _migration SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS version ON _migration TYPE int;
DEFINE FIELD IF NOT EXISTS name ON _migration TYPE string;
DEFINE FIELD IF NOT EXISTS checksum ON _migration TYPE string;
DEFINE FIELD IF NOT EXISTS applied_at ON _migration TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS _migration_version_unique ON _migration FIELDS version UNIQUE;";

const ENTITY_TABLES: [&str; 6] = ["person", "organisation", "location", "repository", "file", "symbol"];

#[derive(SurrealValue)]
struct Applied {
    version: u32,
    checksum: String,
}

#[derive(SurrealValue)]
struct Entity {
    id: RecordId,
    vault: RecordId,
    name: String,
    aliases: Vec<String>,
}

#[derive(SurrealValue)]
struct Edge {
    #[surreal(rename = "in")]
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

/// Bring an org database to the latest tenant schema, then the settings-dependent field default.
pub async fn migrate(db: &Db, settings: &Settings) -> surrealdb::Result<()> {
    apply_up_to(db, u32::MAX).await?;
    // The one config-dependent definition: re-applied on every migration pass, outside the ledger.
    let sql = format!(
        "DEFINE FIELD OVERWRITE openai_base_url ON app_settings TYPE string DEFAULT \"{}\";",
        settings.openai_base_url.replace('"', "\\\"")
    );
    crate::tx::with_retry(|| async { root(db, "migrate.openai_base_url", &sql).await?.check().map(|_| ()) }).await?;
    Ok(())
}

/// Bring the control database to the latest control schema.
pub async fn migrate_control(db: &Db) -> surrealdb::Result<()> {
    apply(db, CONTROL_MIGRATIONS, u32::MAX).await
}

/// Apply pending tenant migrations with `version <= max`. Public for tests that need a half-migrated DB.
pub async fn apply_up_to(db: &Db, max: u32) -> surrealdb::Result<()> {
    apply(db, MIGRATIONS, max).await
}

async fn apply(db: &Db, set: &[(u32, &str, &str)], max: u32) -> surrealdb::Result<()> {
    // Two replicas booting together race on everything below; a lost race is a commit conflict or a
    // unique violation on `_migration.version`, and means the other one did the work.
    crate::tx::with_retry(|| async { root(db, "migrate.ledger", LEDGER).await?.check().map(|_| ()) }).await?;
    let applied: Vec<Applied> = root(db, "migrate.applied", "SELECT version, checksum FROM _migration").await?.take(0)?;
    for &(version, name, sql) in set.iter().filter(|m| m.0 <= max) {
        let sum = checksum(sql);
        if let Some(a) = applied.iter().find(|a| a.version == version) {
            if a.checksum != sum && !LEGACY_CHECKSUMS.contains(&(version, a.checksum.as_str())) {
                return Err(surrealdb::Error::thrown(format!(
                    "migration {version:04}_{name} was edited after it was applied (checksum {} != {sum}). \
                     Applied migrations are immutable: revert the file and add a new migration instead.",
                    a.checksum
                )));
            }
            continue;
        }
        // a pre-step of the tenant set only: control 0002 is unrelated
        if version == 2 && std::ptr::eq(set, MIGRATIONS) {
            dedupe_entity_names(db).await?;
        }
        // DEFINE is allowed inside a transaction, so schema and ledger row commit together.
        let mut attempt = 1;
        loop {
            let run = root(
                db,
                "migrate.apply",
                format!("BEGIN TRANSACTION;\n{sql}\nCREATE _migration SET version = $v, name = $n, checksum = $c;\nCOMMIT TRANSACTION;"),
            )
            .bind(("v", version))
            .bind(("n", name))
            .bind(("c", sum.clone()))
            .await
            .and_then(|r| r.check());
            match run {
                Ok(_) => {
                    tracing::info!(version, name, "applied migration");
                    break;
                }
                Err(e) if attempt < crate::tx::MAX_ATTEMPTS && (crate::tx::is_conflict(&e) || crate::tx::is_duplicate(&e)) => {
                    if is_applied(db, version).await? {
                        tracing::info!(version, name, "migration was applied by another process");
                        break;
                    }
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

async fn is_applied(db: &Db, version: u32) -> surrealdb::Result<bool> {
    let rows: Vec<Applied> = root(db, "migrate.is_applied", "SELECT version, checksum FROM _migration WHERE version = $v").bind(("v", version)).await?.take(0)?;
    Ok(!rows.is_empty())
}

/// 0002 pre-step: merge entities sharing (vault, name) into the oldest, so the UNIQUE index can build.
/// Mirrors `entities::service::merge_entities`. Idempotent: a crash midway just resumes next boot.
async fn dedupe_entity_names(db: &Db) -> surrealdb::Result<()> {
    for table in ENTITY_TABLES {
        let rows: Vec<Entity> = root(db, "migrate.dedupe_list", format!("SELECT id, vault, name, aliases, created_at FROM {table} ORDER BY created_at, id"))
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
    root(db, "migrate.dedupe_memory", "UPDATE memory SET subject = $w WHERE subject = $l")
        .bind(("w", winner.id.clone()))
        .bind(("l", loser.id.clone()))
        .await?
        .check()?;
    // Relation endpoints are immutable: re-create each edge on the winner (the (in, out, label)
    // UNIQUE index drops ones it already has, so errors here are expected), then drop the loser's.
    let edges: Vec<Edge> = root(db, "migrate.dedupe_edges", "SELECT * FROM relates_to WHERE in = $l OR out = $l")
        .bind(("l", loser.id.clone()))
        .await?
        .take(0)?;
    for e in edges {
        let from = if e.in_ == loser.id { winner.id.clone() } else { e.in_.clone() };
        let to = if e.out == loser.id { winner.id.clone() } else { e.out.clone() };
        if from == to {
            continue;
        }
        let _ = root(db, "migrate.dedupe_relate", "RELATE $from->relates_to->$to SET label = $label, owner = $owner, source = $source, created_at = $at")
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
    root(db, "migrate.dedupe_finish", "UPDATE $w SET aliases = $a; DELETE relates_to WHERE in = $l OR out = $l; DELETE $l;")
        .bind(("w", winner.id.clone()))
        .bind(("a", winner.aliases.clone()))
        .bind(("l", loser.id.clone()))
        .await?
        .check()?;
    Ok(())
}

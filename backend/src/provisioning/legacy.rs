//! The self-host move: a pre-tenancy install keeps everything in one database. On the first boot of
//! this version, if that database has users and `control.tenant` has no rows, copy it into the new
//! layout: one org (`Default`), every existing user a member (the first user the owner), an
//! `org_<uuid>` database holding the org tables, and the control database holding accounts,
//! credentials, OAuth, the job queue and capsules.
//!
//! Resumable: the org's `tenant` row says `moving` until the end, every copy is `INSERT IGNORE` by
//! record id, so a crash re-runs the same steps and skips what is already there. Verified: per-table
//! row counts must match before the org is marked `ready`. Invisible to users: record ids, sessions,
//! tokens and OAuth grants carry over unchanged. The old database is never deleted; the log line at
//! the end says how to remove it once you have checked.

use surrealdb::types::{RecordId, SurrealValue, Value};

use super::Provisioner;
use crate::config::Settings;
use crate::connectors::crypto;
use crate::error::{AppError, AppResult};
use crate::pool::{self, ControlDb, OrgId};
use crate::store::{self, root};

/// Control tables to copy, in dependency-free order (no table has a hard reference to another).
/// `job_leader` is not copied: the control migration seeds its one row.
const CONTROL_COPY: &[&str] =
    &["user", "api_token", "session", "oauth_client", "oauth_grant", "oauth_code", "oauth_token", "audit_event", "job", "failure_capsule"];

/// Org tables to copy. Edges come last.
const TENANT_COPY: &[&str] = &[
    "app_settings", "vault", "vault_member", "connector", "sync_status", "cache_record", "person", "organisation", "location", "repository", "file",
    "symbol", "memory", "chat_thread", "chat_message", "audit_log", "embed_cache",
];
const TENANT_EDGES: &[&str] = &["linked_to", "relates_to"];

const BATCH: usize = 100;

#[derive(serde::Deserialize, SurrealValue)]
struct Count {
    count: i64,
}

/// Does the move if there is one to do. A no-op on a fresh install and on every boot after the move.
pub async fn move_if_needed(p: &Provisioner, control: &ControlDb, settings: &Settings) -> AppResult<()> {
    let mut res = store::dynamic_control(control, "legacy.tenants", "SELECT org, status FROM tenant").await?;
    #[derive(serde::Deserialize, SurrealValue)]
    struct T {
        org: RecordId,
        status: String,
    }
    let tenants: Vec<T> = res.take(0)?;
    if let Some(t) = tenants.iter().find(|t| t.status == "moving") {
        let org = org_of_record(&t.org)?;
        tracing::warn!(%org, "resuming an interrupted data move");
        return run(p, control, settings, org).await;
    }
    if !tenants.is_empty() || !p.legacy_has_users(&settings.surreal_db).await? {
        return Ok(());
    }
    tracing::warn!(db = %settings.surreal_db, "moving this install's single database into the org layout");
    run(p, control, settings, OrgId::new()).await
}

fn org_of_record(r: &RecordId) -> AppResult<OrgId> {
    use crate::rid::RecordIdExt;
    crate::rid::key_string(r.key()).and_then(|k| OrgId::parse(&k)).ok_or_else(|| AppError::internal("tenant row has a bad org id"))
}

async fn run(p: &Provisioner, control: &ControlDb, settings: &Settings, org: OrgId) -> AppResult<()> {
    let legacy = p.session(&settings.surreal_db).await?;
    // Bring a 2.x export or an older schema to the last shape the legacy database may have.
    crate::migrate::apply_up_to(&legacy, crate::migrate::LEGACY_TENANT_VERSION).await?;
    let ctrl = p.session(pool::CONTROL_DB).await?;

    // The org and its (still `moving`) routing row first, so a crash from here on resumes.
    store::tenant::ORG_CREATE.on(control).bind(("id", org.record())).bind(("name", "Default")).await?.check()?;
    let t = p.tenant_state(control, &org).await?;
    let (db, pass) = match t {
        Some(t) => (t.db, crypto::decrypt(&settings.encryption_key, &t.db_pass_enc)?),
        None => {
            let (db, pass) = (org.db_name(), pool::generate_db_password());
            p.write_tenant(control, &org, &db, &pass, 0, "moving").await?;
            (db, pass)
        }
    };
    p.build_database(&db, &pass).await?;
    let dst = p.session(&db).await?;

    // Accounts and credentials, then membership: oldest user owns.
    for table in CONTROL_COPY {
        copy_table(&legacy, &ctrl, table, false).await?;
    }
    add_memberships(&ctrl, &org).await?;
    for table in TENANT_COPY {
        copy_table(&legacy, &dst, table, false).await?;
    }
    for table in TENANT_EDGES {
        copy_table(&legacy, &dst, table, true).await?;
    }

    // Verify before declaring done.
    let mut mismatched = Vec::new();
    for (table, to) in CONTROL_COPY.iter().map(|t| (t, &ctrl)).chain(TENANT_COPY.iter().chain(TENANT_EDGES).map(|t| (t, &dst))) {
        let (a, b) = (count(&legacy, table).await?, count(to, table).await?);
        if a != b {
            mismatched.push(format!("{table}: {a} in the old database, {b} in the new"));
        }
    }
    if !mismatched.is_empty() {
        return Err(AppError::internal(format!("the data move did not verify: {}", mismatched.join("; "))));
    }
    p.write_tenant(control, &org, &db, &pass, crate::migrate::LATEST_TENANT, "ready").await?;
    let (users, mems) = (count(&ctrl, "user").await?, count(&ctrl, "membership").await?);
    tracing::warn!(
        %org, users, memberships = mems, db = %db,
        "data move verified and complete. The old database `{old}` is untouched; once you have checked the app, remove it with: \
         surreal sql --user <root> --pass <pass> --ns {ns} --hide-welcome <<< 'REMOVE DATABASE `{old}`;'",
        old = settings.surreal_db, ns = settings.surreal_ns
    );
    Ok(())
}

/// Every copied user becomes a member of the org; the oldest is the owner. Re-runs skip existing rows.
/// Copied jobs and capsules are stamped with the org too.
async fn add_memberships(ctrl: &crate::db::Db, org: &OrgId) -> AppResult<()> {
    root(
        ctrl,
        "legacy.memberships",
        "LET $users = (SELECT VALUE id FROM user ORDER BY created_at, id);
         FOR $u IN $users {
             IF array::len((SELECT id FROM membership WHERE user = $u AND org = $org)) = 0 {
                 CREATE membership SET user = $u, org = $org,
                     role = IF $u = $users[0] THEN 'owner' ELSE 'member' END;
             };
         };
         UPDATE job SET org = $key WHERE org = NONE;
         UPDATE failure_capsule SET org = $key WHERE org = NONE;",
    )
    .bind(("org", org.record()))
    .bind(("key", org.key()))
    .await?
    .check()?;
    Ok(())
}

async fn count(db: &crate::db::Db, table: &str) -> AppResult<i64> {
    let mut res = root(db, "legacy.count", format!("SELECT count() FROM {table} GROUP ALL")).await?;
    Ok(res.take::<Vec<Count>>(0)?.first().map_or(0, |c| c.count))
}

/// Copy every row of `table` in id order, `BATCH` at a time.
async fn copy_table(src: &crate::db::Db, dst: &crate::db::Db, table: &str, edge: bool) -> AppResult<()> {
    let mut last: Option<RecordId> = None;
    loop {
        let ids: Vec<RecordId> = match &last {
            Some(l) => root(src, "legacy.ids", format!("SELECT VALUE id FROM {table} WHERE id > $last ORDER BY id LIMIT {BATCH}")).bind(("last", l.clone())),
            None => root(src, "legacy.ids", format!("SELECT VALUE id FROM {table} ORDER BY id LIMIT {BATCH}")),
        }
        .await?
        .take(0)?;
        let Some(tail) = ids.last().cloned() else { break };
        let rows: Vec<Value> = root(src, "legacy.rows", "SELECT * FROM $ids").bind(("ids", ids)).await?.take(0)?;
        let relation = if edge { "RELATION " } else { "" };
        root(dst, "legacy.insert", format!("INSERT {relation}IGNORE INTO {table} $rows RETURN NONE")).bind(("rows", rows)).await?.check()?;
        last = Some(tail);
    }
    Ok(())
}

impl Provisioner {
    /// Is there an old single database holding at least one user?
    pub(super) async fn legacy_has_users(&self, db: &str) -> AppResult<bool> {
        if db == pool::CONTROL_DB {
            return Ok(false);
        }
        let ns = self.session_ns().await?;
        let info: Option<serde_json::Value> = root(&ns, "legacy.info_ns", "INFO FOR NS STRUCTURE").await?.take(0)?;
        let exists = info
            .as_ref()
            .and_then(|v| v.get("databases"))
            .and_then(|d| d.as_array())
            .is_some_and(|dbs| dbs.iter().any(|d| d.get("name").and_then(|n| n.as_str()) == Some(db)));
        if !exists {
            return Ok(false);
        }
        let s = self.session(db).await?;
        let info: Option<serde_json::Value> = root(&s, "legacy.info_db", "INFO FOR DB STRUCTURE").await?.take(0)?;
        let has_user_table = info
            .as_ref()
            .and_then(|v| v.get("tables"))
            .and_then(|t| t.as_array())
            .is_some_and(|ts| ts.iter().any(|t| t.get("name").and_then(|n| n.as_str()) == Some("user")));
        Ok(has_user_table && count(&s, "user").await? > 0)
    }

    async fn session_ns(&self) -> surrealdb::Result<crate::db::Db> {
        let s = (*self.root).clone();
        s.use_ns(&self.settings.surreal_ns).await?;
        Ok(s)
    }
}


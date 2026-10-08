//! Sync scheduler. One background loop wakes every minute and syncs every
//! enabled source whose interval (Settings -> sync intervals, default 15
//! minutes, heypocket 24 hours) has elapsed since its last run -- or, after
//! failures, whose backoff has. A connector saved after startup is picked up
//! on the next tick, and its first sync runs right away. [`sync_source`] is
//! also what the on-demand `POST /sources/{key}/sync` endpoint calls.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};
use std::time::Duration as StdDuration;

use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::config::Settings;
use crate::db::Db;
use crate::error::AppResult;
use crate::sources::base::{datetime_to_chrono, owner_key_str};
use crate::sources::registry;
use crate::state::AppState;

const BACKOFF: [u64; 5] = [900, 1800, 3600, 7200, 21600];
const TICK_SECS: u64 = 60;
/// How long a sync lease outlives a crashed run (SurrealQL duration).
const LEASE: &str = "30m";

/// Seconds to wait before retrying after `failures` consecutive failures.
pub fn backoff_seconds(failures: i64) -> u64 {
    let idx = failures.max(0) as usize;
    BACKOFF[idx.min(BACKOFF.len() - 1)]
}

fn sync_status_id(owner: &RecordId, key: &str) -> RecordId {
    RecordId::from_table_key("sync_status", format!("{}:{}", owner_key_str(owner), key))
}

async fn all_user_ids(db: &Db) -> AppResult<Vec<RecordId>> {
    #[derive(Deserialize)]
    struct Row {
        id: RecordId,
    }
    let mut res = db.query("SELECT id FROM user").await?;
    let rows: Vec<Row> = res.take(0)?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SyncStatusRow {
    #[serde(default)]
    cursor: String,
    #[serde(default)]
    last_run: Option<surrealdb::Datetime>,
    #[serde(default)]
    consecutive_failures: i64,
}

async fn get_sync_status(db: &Db, owner: &RecordId, key: &str) -> AppResult<SyncStatusRow> {
    let rid = sync_status_id(owner, key);
    if let Some(row) = db.select::<Option<SyncStatusRow>>(rid.clone()).await? {
        return Ok(row);
    }
    let mut res = db
        .query(
            "UPSERT $id SET owner = $owner, cursor = '', consecutive_failures = 0, last_error = '', \
             last_report = {} RETURN AFTER",
        )
        .bind(("id", rid))
        .bind(("owner", owner.clone()))
        .await?;
    let rows: Vec<SyncStatusRow> = res.take(0)?;
    Ok(rows.into_iter().next().unwrap_or_default())
}

/// `(owner, source)` syncs running in this process. Checked before the
/// database lease: it is exact within one process (manual + scheduled runs),
/// while the lease covers other replicas.
static RUNNING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

struct RunningGuard(String);

impl RunningGuard {
    fn claim(owner: &RecordId, key: &str) -> Option<Self> {
        let id = format!("{}:{key}", owner_key_str(owner));
        RUNNING.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone()).then(|| RunningGuard(id))
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.0);
    }
}

fn lease_id(owner: &RecordId, key: &str) -> RecordId {
    RecordId::from_table_key("sync_lease", format!("{}:{}", owner_key_str(owner), key))
}

/// Atomically claims the `(owner, source)` sync lease: `CREATE` fails when
/// the record already exists, so of two concurrent runs -- scheduled and
/// manual, or two backend replicas -- exactly one gets it. A lease left by a
/// crashed run expires after `LEASE`.
async fn claim_lease(db: &Db, owner: &RecordId, key: &str) -> AppResult<bool> {
    let id = lease_id(owner, key);
    db.query("DELETE $id WHERE until < time::now()").bind(("id", id.clone())).await?.check()?;
    match db.query(format!("CREATE $id SET until = time::now() + {LEASE}")).bind(("id", id)).await?.check() {
        Ok(_) => Ok(true),
        Err(e) if e.to_string().contains("already exists") => Ok(false),
        Err(e) => Err(e.into()),
    }
}

async fn release_lease(db: &Db, owner: &RecordId, key: &str) {
    if let Err(e) = db.query("DELETE $id").bind(("id", lease_id(owner, key))).await {
        tracing::warn!(source = %key, error = %e, "could not release sync lease (it expires on its own)");
    }
}

/// Runs one source sync for `owner` and records its health on the
/// `sync_status` row. Never returns an `Err`: any failure (missing or bad
/// credentials, a provider error, records that failed to save, the
/// checkpoint write itself) is stored as `last_error` -- shown on the
/// Connectors and dashboard pages -- and returned as `{"source", "error"}`.
/// The cursor only advances when every record was stored, so a failed
/// record is fetched again next time (the replay is idempotent). If another
/// run for the same source holds the lease, returns
/// `{"status": "already_running"}` without calling the provider.
pub async fn sync_source(db: &Db, settings: &Settings, owner: &RecordId, key: &str) -> Value {
    let Some(_running) = RunningGuard::claim(owner, key) else {
        return json!({"source": key, "status": "already_running"});
    };
    match claim_lease(db, owner, key).await {
        Ok(true) => {}
        Ok(false) => return json!({"source": key, "status": "already_running"}),
        Err(e) => return json!({"source": key, "error": e.message}),
    }
    let out = sync_locked(db, settings, owner, key).await;
    release_lease(db, owner, key).await;
    out
}

async fn sync_locked(db: &Db, settings: &Settings, owner: &RecordId, key: &str) -> Value {
    let st = match get_sync_status(db, owner, key).await {
        Ok(st) => st,
        Err(e) => return json!({"source": key, "error": e.message}),
    };
    let now = surrealdb::Datetime::from(Utc::now());
    let cursor = if st.cursor.is_empty() { None } else { Some(st.cursor.clone()) };

    let error = match registry::run_sync(db, settings, owner, key, cursor).await {
        Ok((report, _)) if report.failed > 0 => format!(
            "{} of {} records failed to save; will retry: {}",
            report.failed,
            report.failed + report.written + report.skipped,
            report.errors.first().map(String::as_str).unwrap_or("")
        ),
        Ok((report, next_cursor)) => {
            let report_value = report.as_dict();
            let saved = async {
                db.query(
                    "UPDATE $id SET last_run = $now, cursor = $cursor, last_ok = $now, \
                     last_error = '', consecutive_failures = 0, last_report = $report",
                )
                .bind(("id", sync_status_id(owner, key)))
                .bind(("now", now.clone()))
                .bind(("cursor", next_cursor.unwrap_or(st.cursor.clone())))
                .bind(("report", report_value.clone()))
                .await?
                .check()
            }
            .await;
            match saved {
                Ok(_) => return report_value,
                Err(e) => format!("records saved, but the sync checkpoint could not be: {e}"),
            }
        }
        Err(e) => e.message,
    };

    tracing::warn!(source = %key, owner = %owner, error = %error, "sync failed");
    if let Err(e) = db
        .query("UPDATE $id SET last_run = $now, consecutive_failures = $failures, last_error = $error")
        .bind(("id", sync_status_id(owner, key)))
        .bind(("now", now))
        .bind(("failures", st.consecutive_failures + 1))
        .bind(("error", error.clone()))
        .await
        .and_then(|r| r.check())
    {
        tracing::warn!(source = %key, error = %e, "could not record sync failure");
    }
    json!({"source": key, "error": error})
}

/// Whether a source last run `since_last` seconds ago (None = never) is due.
fn is_due(since_last: Option<i64>, interval: u64, failures: i64) -> bool {
    let wait = if failures > 0 { interval.max(backoff_seconds(failures - 1)) } else { interval };
    since_last.map(|s| s >= wait as i64).unwrap_or(true)
}

/// Syncs every enabled source, for every user, that is due.
pub async fn poll_all(db: &Db, settings: &Settings) -> AppResult<Vec<Value>> {
    let mut out = Vec::new();
    let now = Utc::now();
    for owner in all_user_ids(db).await? {
        let intervals = sync_intervals_for(db, &owner).await.unwrap_or_default();
        for src in registry::enabled(db, &owner).await? {
            let row: Option<SyncStatusRow> = db.select(sync_status_id(&owner, src.key())).await?;
            let row = row.unwrap_or_default();
            let since_last = row.last_run.as_ref().and_then(datetime_to_chrono).map(|t| (now - t).num_seconds());
            if is_due(since_last, interval_for(&intervals, src.key()), row.consecutive_failures) {
                out.push(sync_source(db, settings, &owner, src.key()).await);
            }
        }
    }
    Ok(out)
}

/// Spawns the scheduler loop. Returns its handle so the caller can hold or
/// abort it.
pub async fn spawn(state: AppState) -> Vec<tokio::task::JoinHandle<()>> {
    vec![tokio::spawn(async move {
        let mut ticker = tokio::time::interval(StdDuration::from_secs(TICK_SECS));
        loop {
            ticker.tick().await;
            if let Err(e) = poll_all(&state.db, &state.settings).await {
                tracing::warn!(error = %e.message, "sync sweep failed");
            }
        }
    })]
}

async fn sync_intervals_for(db: &Db, owner: &RecordId) -> AppResult<Value> {
    #[derive(Deserialize)]
    struct Row {
        #[serde(default)]
        sync_intervals: Value,
    }
    let mut res = db.query("SELECT sync_intervals FROM app_settings WHERE owner = $owner LIMIT 1").bind(("owner", owner.clone())).await?;
    let rows: Vec<Row> = res.take(0)?;
    Ok(rows.into_iter().next().map(|r| r.sync_intervals).unwrap_or(json!({})))
}

/// The user's interval for `key`, else heypocket's 24h plan-mandated minimum
/// or the 15-minute default.
fn interval_for(sync_intervals: &Value, key: &str) -> u64 {
    if let Some(v) = sync_intervals.get(key).and_then(|v| v.as_u64()) {
        return v.max(1);
    }
    if key == "heypocket" {
        86400
    } else {
        900
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_seconds_follows_the_fixed_schedule_and_clamps() {
        assert_eq!(backoff_seconds(0), 900);
        assert_eq!(backoff_seconds(1), 1800);
        assert_eq!(backoff_seconds(4), 21600);
        assert_eq!(backoff_seconds(100), 21600);
    }

    #[test]
    fn sync_status_id_embeds_owner_key_and_source() {
        let owner: RecordId = "user:abc123".parse().unwrap();
        let id = sync_status_id(&owner, "up_bank");
        assert_eq!(id.table(), "sync_status");
        let key_str: String = id.key().clone().try_into().expect("string key");
        assert_eq!(key_str, "abc123:up_bank");
    }

    #[test]
    fn interval_for_defers_to_configured_value_when_present() {
        let intervals = json!({"up_bank": 120, "heypocket": 3600});
        assert_eq!(interval_for(&intervals, "up_bank"), 120);
        assert_eq!(interval_for(&intervals, "heypocket"), 3600);
        assert_eq!(interval_for(&json!({}), "heypocket"), 86400);
        assert_eq!(interval_for(&json!({}), "github"), 900);
    }

    #[test]
    fn is_due_honours_interval_and_backoff() {
        assert!(is_due(None, 900, 0), "never synced -> sync now");
        assert!(!is_due(Some(60), 900, 0));
        assert!(is_due(Some(900), 900, 0));
        // after 2 failures wait at least 30 minutes, even with a short interval
        assert!(!is_due(Some(1000), 300, 2));
        assert!(is_due(Some(1800), 300, 2));
    }
}

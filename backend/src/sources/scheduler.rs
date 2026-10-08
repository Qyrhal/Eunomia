//! Sync scheduling. [`sync_source`] runs one source for one user (called by the `sync` job
//! handler and the on-demand `/sources/{key}/sync` endpoint). [`due_syncs`] says which
//! `(owner, source)` pairs should sync now; the scheduler leader (`jobs::leader`) turns those
//! into `sync:<owner>:<source>:<window>` jobs. There are no per-user timer loops.

use surrealdb::types::SurrealValue;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::types::RecordId;
use crate::rid::RecordIdExt;

use crate::db::Db;
use crate::store;
use crate::error::AppResult;
use crate::sources::base::{datetime_to_chrono, owner_key_str};
use crate::sources::registry;

const BACKOFF: [u64; 5] = [900, 1800, 3600, 7200, 21600];

/// Seconds to wait before retrying after `failures` consecutive failures.
pub fn backoff_seconds(failures: i64) -> u64 {
    let idx = failures.max(0) as usize;
    BACKOFF[idx.min(BACKOFF.len() - 1)]
}

fn sync_status_id(owner: &RecordId, key: &str) -> RecordId {
    RecordId::from_table_key("sync_status", format!("{}:{}", owner_key_str(owner), key))
}

async fn all_user_ids(db: &Db) -> AppResult<Vec<RecordId>> {
    #[derive(Deserialize, SurrealValue)]
    struct Row {
        id: RecordId,
    }
    let mut res = store::app::SOURCES_USER_IDS.on(db).await?;
    let rows: Vec<Row> = res.take(0)?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

#[derive(Debug, Clone, Deserialize, SurrealValue)]
struct SyncStatusRow {
    #[serde(default)]
    #[surreal(default)]
    cursor: String,
    #[serde(default)]
    #[surreal(default)]
    last_run: Option<surrealdb::types::Datetime>,
    #[serde(default)]
    #[surreal(default)]
    consecutive_failures: i64,
}

async fn get_sync_status(db: &Db, owner: &RecordId, key: &str) -> AppResult<SyncStatusRow> {
    let rid = sync_status_id(owner, key);
    if let Some(row) = db.select::<Option<SyncStatusRow>>(rid.clone()).await? {
        return Ok(row);
    }
    let mut res = store::app::SOURCES_SYNC_STATUS_UPSERT
        .on(db)
        .bind(("id", rid))
        .bind(("owner", owner.clone()))
        .await?;
    let rows: Vec<SyncStatusRow> = res.take(0)?;
    Ok(rows.into_iter().next().unwrap_or(SyncStatusRow { cursor: String::new(), last_run: None, consecutive_failures: 0 }))
}

/// Runs one source sync for `owner`, records health, applies backoff on
/// failure. Never returns an `Err` to the caller -- any failure (bad
/// credentials, a network error, an unknown source key) is recorded on the
/// `sync_status` row and reported back as `{"source": key, "error": ...}`,
/// matching Python's blanket `except Exception` here.
pub async fn sync_source(db: &Db, encryption_key: &str, owner: &RecordId, key: &str, mode: &str) -> Value {
    let st = match get_sync_status(db, owner, key).await {
        Ok(st) => st,
        Err(e) => return json!({"source": key, "error": e.message}),
    };
    let now = Utc::now();
    let cursor = if st.cursor.is_empty() { None } else { Some(st.cursor.clone()) };

    match registry::run_sync(db, encryption_key, owner, key, mode, cursor).await {
        Ok((report, next_cursor)) => {
            let report_value = report.as_value();
            let _ = store::app::SOURCES_SYNC_OK
                .on(db)
                .bind(("id", sync_status_id(owner, key)))
                .bind(("now", surrealdb::types::Datetime::from(now)))
                .bind(("cursor", next_cursor.unwrap_or_default()))
                .bind(("report", report_value.clone()))
                .await;
            report_value
        }
        Err(e) => {
            let error = e.message.clone();
            let failures = st.consecutive_failures + 1;
            let _ = store::app::SOURCES_SYNC_FAILED
                .on(db)
                .bind(("id", sync_status_id(owner, key)))
                .bind(("now", surrealdb::types::Datetime::from(now)))
                .bind(("failures", failures))
                .bind(("error", error.clone()))
                .await;
            json!({"source": key, "error": error})
        }
    }
}

/// One source that should sync now. `period` (the source's interval) sizes the idempotency window.
pub struct DueSync {
    pub owner: RecordId,
    pub source: String,
    pub period: u64,
}

/// Whether a source last run at `last_run` should run again: its interval has passed, and after
/// failures the longer backoff has too. A source that never ran is due.
pub fn is_due(last_run: Option<chrono::DateTime<Utc>>, failures: i64, interval: u64, now: chrono::DateTime<Utc>) -> bool {
    let Some(last_run) = last_run else { return true };
    let wait = if failures > 0 { interval.max(backoff_seconds(failures)) } else { interval };
    (now - last_run).num_seconds() >= wait as i64
}

/// Every enabled source, for every user, whose interval and backoff window have elapsed.
pub async fn due_syncs(db: &Db) -> AppResult<Vec<DueSync>> {
    let mut out = Vec::new();
    let now = Utc::now();
    for owner in all_user_ids(db).await? {
        let intervals = sync_intervals_for(db, &owner).await.unwrap_or_default();
        for src in registry::enabled(db, &owner).await? {
            let row: Option<SyncStatusRow> = db.select(sync_status_id(&owner, src.key())).await?;
            let last_run = row.as_ref().and_then(|r| r.last_run.as_ref()).and_then(datetime_to_chrono);
            let failures = row.map_or(0, |r| r.consecutive_failures);
            let period = interval_for(&intervals, src.key());
            if is_due(last_run, failures, period, now) {
                out.push(DueSync { owner: owner.clone(), source: src.key().to_string(), period });
            }
        }
    }
    Ok(out)
}

async fn sync_intervals_for(db: &Db, owner: &RecordId) -> AppResult<Value> {
    #[derive(Deserialize, SurrealValue)]
    struct Row {
        #[serde(default)]
        #[surreal(default)]
        sync_intervals: Value,
    }
    let mut res = store::app::SOURCES_SYNC_INTERVALS.on(db).bind(("owner", owner.clone())).await?;
    let rows: Vec<Row> = res.take(0)?;
    Ok(rows.into_iter().next().map(|r| r.sync_intervals).unwrap_or(json!({})))
}

/// `sync_intervals.get(key, default)`, where the default is heypocket's
/// 24h plan-mandated minimum (Python: `sync_intervals.setdefault("heypocket",
/// 86400)`) and everything else's fallback is 15 minutes.
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
    fn is_due_follows_interval_and_backoff() {
        let now = Utc::now();
        let ago = |s: i64| Some(now - chrono::Duration::seconds(s));
        assert!(is_due(None, 0, 900, now));
        assert!(!is_due(ago(100), 0, 900, now));
        assert!(is_due(ago(901), 0, 900, now));
        // one failure backs off 1800s, longer than the 900s interval
        assert!(!is_due(ago(901), 1, 900, now));
        assert!(is_due(ago(1801), 1, 900, now));
    }

    #[test]
    fn backoff_seconds_follows_the_fixed_schedule_and_clamps() {
        assert_eq!(backoff_seconds(0), 900);
        assert_eq!(backoff_seconds(1), 1800);
        assert_eq!(backoff_seconds(4), 21600);
        assert_eq!(backoff_seconds(100), 21600); // clamps to the last tier
    }

    #[test]
    fn sync_status_id_embeds_owner_key_and_source() {
        let owner: RecordId = crate::rid::parse("user:abc123").unwrap();
        let id = sync_status_id(&owner, "up_bank");
        assert_eq!(id.table(), "sync_status");
        let key_str = crate::rid::key_string(id.key()).expect("string key");
        assert_eq!(key_str, "abc123:up_bank");
    }

    #[test]
    fn interval_for_defers_to_configured_value_when_present() {
        let intervals = json!({"up_bank": 120, "heypocket": 3600});
        assert_eq!(interval_for(&intervals, "up_bank"), 120);
        assert_eq!(interval_for(&intervals, "heypocket"), 3600);
    }

    #[test]
    fn interval_for_defaults_heypocket_to_24h_and_others_to_15m() {
        let intervals = json!({});
        assert_eq!(interval_for(&intervals, "heypocket"), 86400);
        assert_eq!(interval_for(&intervals, "up_bank"), 900);
    }
}

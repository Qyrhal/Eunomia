//! Sync scheduler. Ported from `sources/scheduler.py`.
//!
//! Python drives these jobs from an APScheduler `AsyncIOScheduler` wired into
//! the FastAPI app's lifespan; each job is also directly callable, which is
//! what its tests and the on-demand `/sources/{key}/sync` endpoint use. This
//! port keeps that same shape -- [`sync_source`] and [`poll_all`] are plain
//! async functions callable on demand -- and replaces APScheduler with
//! `tokio::time::interval` tasks spawned by [`spawn`], per the task's "keep it
//! simple" instruction: one task per (owner, source) plus one `poll_all`
//! sweep, no generic scheduler library.
//!
//! `backfill_embeddings` is stubbed: it depends on `cache.search.set_embedding`
//! and `embeddings.service.embed`, neither of which is ported to Rust yet
//! (see `sources::registry::ingest`'s doc comment for the same gap).

use std::time::Duration as StdDuration;

use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use surrealdb::RecordId;

use crate::db::Db;
use crate::store;
use crate::error::AppResult;
use crate::sources::base::{datetime_to_chrono, owner_key_str};
use crate::sources::registry;
use crate::state::AppState;

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
    #[derive(Deserialize)]
    struct Row {
        id: RecordId,
    }
    let mut res = store::app::SOURCES_USER_IDS.on(db).await?;
    let rows: Vec<Row> = res.take(0)?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

#[derive(Debug, Clone, Deserialize)]
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
                .bind(("now", surrealdb::Datetime::from(now)))
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
                .bind(("now", surrealdb::Datetime::from(now)))
                .bind(("failures", failures))
                .bind(("error", error.clone()))
                .await;
            json!({"source": key, "error": error})
        }
    }
}

/// Polls every enabled source, for every user, whose backoff window has
/// elapsed.
pub async fn poll_all(db: &Db, encryption_key: &str) -> AppResult<Vec<Value>> {
    let mut out = Vec::new();
    let now = Utc::now();
    for owner in all_user_ids(db).await? {
        for src in registry::enabled(db, &owner).await? {
            let rid = sync_status_id(&owner, src.key());
            let row: Option<SyncStatusRow> = db.select(rid).await?;
            if let Some(row) = &row
                && row.consecutive_failures > 0
                    && let Some(last_run) = row.last_run.as_ref().and_then(datetime_to_chrono) {
                        let wait = backoff_seconds(row.consecutive_failures);
                        if (now - last_run).num_seconds() < wait as i64 {
                            continue;
                        }
                    }
            out.push(sync_source(db, encryption_key, &owner, src.key(), "poll").await);
        }
    }
    Ok(out)
}

/// Deferred: re-embeds cache records missing an embedding. Depends on
/// `cache.search.set_embedding` + `embeddings.service.embed`, neither ported
/// to Rust yet. Kept as a callable stub (rather than omitted) so the public
/// surface this module exposes still matches Python's, and so wiring it up
/// later is a one-function change.
pub async fn backfill_embeddings(_db: &Db, _limit: i64) -> AppResult<i64> {
    Ok(0)
}

/// Spawns the periodic sync tasks: one `tokio::time::interval` loop per
/// `(owner, enabled source)` pair (mirroring Python's `sched.add_job(..., id=
/// f"sync:{owner.id}:{src.key}")`), plus one `poll_all` sweep every 5 minutes.
/// Read-light by design per the task brief -- this is not a generic
/// scheduler, just enough to keep sources syncing. Returns the spawned
/// task handles so the caller (wired up separately, outside this module) can
/// hold or abort them.
pub async fn spawn(state: AppState) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = Vec::new();

    let owners = all_user_ids(&state.db).await.unwrap_or_default();
    for owner in owners {
        let sync_intervals = sync_intervals_for(&state.db, &owner).await.unwrap_or_default();
        let enabled_sources = registry::enabled(&state.db, &owner).await.unwrap_or_default();
        for src in enabled_sources {
            let interval_secs = interval_for(&sync_intervals, src.key());
            let state = state.clone();
            let owner = owner.clone();
            let key = src.key().to_string();
            handles.push(tokio::spawn(async move {
                let mut ticker = tokio::time::interval(StdDuration::from_secs(interval_secs));
                ticker.tick().await; // first tick is immediate; skip it to mirror an "interval" trigger's first run
                loop {
                    ticker.tick().await;
                    let _ = sync_source(&state.db, &state.settings.encryption_key, &owner, &key, "poll").await;
                }
            }));
        }
    }

    {
        let state = state.clone();
        handles.push(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(StdDuration::from_secs(300));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let _ = poll_all(&state.db, &state.settings.encryption_key).await;
            }
        }));
    }

    handles
}

async fn sync_intervals_for(db: &Db, owner: &RecordId) -> AppResult<Value> {
    #[derive(Deserialize)]
    struct Row {
        #[serde(default)]
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
    fn backoff_seconds_follows_the_fixed_schedule_and_clamps() {
        assert_eq!(backoff_seconds(0), 900);
        assert_eq!(backoff_seconds(1), 1800);
        assert_eq!(backoff_seconds(4), 21600);
        assert_eq!(backoff_seconds(100), 21600); // clamps to the last tier
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
    }

    #[test]
    fn interval_for_defaults_heypocket_to_24h_and_others_to_15m() {
        let intervals = json!({});
        assert_eq!(interval_for(&intervals, "heypocket"), 86400);
        assert_eq!(interval_for(&intervals, "up_bank"), 900);
    }
}

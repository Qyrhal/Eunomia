//! Sync scheduler. One background loop wakes every minute and syncs every
//! enabled source whose interval (Settings -> sync intervals, default 15
//! minutes, heypocket 24 hours) has elapsed since its last run -- or, after
//! failures, whose backoff has. A connector saved after startup is picked up
//! on the next tick, and its first sync runs right away. [`sync_source`] is
//! also what the on-demand `POST /sources/{key}/sync` endpoint calls.

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

/// Runs one source sync for `owner` and records its health on the
/// `sync_status` row. Never returns an `Err`: any failure (missing or bad
/// credentials, a provider error, an unknown source key) is stored as
/// `last_error` -- shown on the Connectors and dashboard pages -- and
/// returned as `{"source": key, "error": ...}`.
pub async fn sync_source(db: &Db, settings: &Settings, owner: &RecordId, key: &str) -> Value {
    let st = match get_sync_status(db, owner, key).await {
        Ok(st) => st,
        Err(e) => return json!({"source": key, "error": e.message}),
    };
    let now = surrealdb::Datetime::from(Utc::now());
    let cursor = if st.cursor.is_empty() { None } else { Some(st.cursor.clone()) };

    match registry::run_sync(db, settings, owner, key, cursor).await {
        Ok((report, next_cursor)) => {
            let report_value = report.as_dict();
            let _ = db
                .query(
                    "UPDATE $id SET last_run = $now, cursor = $cursor, last_ok = $now, \
                     last_error = '', consecutive_failures = 0, last_report = $report",
                )
                .bind(("id", sync_status_id(owner, key)))
                .bind(("now", now))
                .bind(("cursor", next_cursor.unwrap_or(st.cursor)))
                .bind(("report", report_value.clone()))
                .await;
            report_value
        }
        Err(e) => {
            tracing::warn!(source = %key, owner = %owner, error = %e.message, "sync failed");
            let _ = db
                .query("UPDATE $id SET last_run = $now, consecutive_failures = $failures, last_error = $error")
                .bind(("id", sync_status_id(owner, key)))
                .bind(("now", now))
                .bind(("failures", st.consecutive_failures + 1))
                .bind(("error", e.message.clone()))
                .await;
            json!({"source": key, "error": e.message})
        }
    }
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

    /// End to end against a real SurrealDB (skipped unless
    /// `EUNOMIA_TEST_SURREAL_URL`, e.g. `ws://127.0.0.1:8000/rpc`, is set):
    /// connector saved with encrypted credentials -> scheduler sync -> mock
    /// GitHub API -> ingest -> records findable by `search`; then a 401 lands
    /// in `sync_status.last_error` instead of vanishing.
    #[tokio::test]
    async fn sync_source_end_to_end_against_surrealdb() {
        use crate::cache::search;
        use crate::connectors::service;
        use crate::sources::mock::{route, serve};

        let Ok(url) = std::env::var("EUNOMIA_TEST_SURREAL_URL") else {
            eprintln!("skipped: set EUNOMIA_TEST_SURREAL_URL to run");
            return;
        };
        let mut settings = Settings::load();
        settings.surreal_url = url;
        settings.surreal_ns = "connectors_test".into();
        settings.surreal_db = format!("t{}", uuid::Uuid::new_v4().simple());
        settings.encryption_key = "test-key".into();
        settings.embeddings_backend = "stub".into();
        let db = crate::db::connect(&settings).await.unwrap();
        crate::db::ensure_schema(&db, &settings).await.unwrap();
        let mut res = db.query("CREATE user SET email = 'e2e@example.com', password_hash = 'x' RETURN id").await.unwrap();
        #[derive(Deserialize)]
        struct Id {
            id: RecordId,
        }
        let owner = res.take::<Vec<Id>>(0).unwrap().remove(0).id;

        let mock = serve(vec![route("GET", "/issues", json!([{
            "number": 7, "title": "Importer drops the last row", "state": "open", "body": "Seen with the quarterly CSV export.",
            "user": {"login": "octocat"}, "labels": [], "assignees": [], "comments": 0, "repository": {"full_name": "acme/widget"},
            "html_url": "https://github.com/acme/widget/issues/7", "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-02-01T00:00:00Z",
        }]))])
        .await;
        service::upsert_connector(
            &db,
            &settings.encryption_key,
            &owner,
            "github",
            Some(true),
            Some(json!({"base_url": mock.base})),
            Some(json!({"personal_access_token": "ghp_e2e"})),
        )
        .await
        .unwrap();

        assert_eq!(poll_all(&db, &settings).await.unwrap().len(), 1, "a never-synced connector is due at once");
        let row: Value = db.query("SELECT last_error, cursor, consecutive_failures FROM ONLY $id").bind(("id", sync_status_id(&owner, "github"))).await.unwrap().take::<Option<Value>>(0).unwrap().unwrap();
        assert_eq!(row["last_error"], "");
        assert_eq!(row["cursor"], "2024-02-01T00:00:00Z");
        assert_eq!(mock.requests()[0].header("authorization"), "Bearer ghp_e2e");

        let mut params = search::SearchParams::new();
        params.mode = "keyword".into();
        let hits = search::search(&db, &settings, &owner, "importer quarterly", &params).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "github:github.issue:acme/widget#7");
        assert!(poll_all(&db, &settings).await.unwrap().is_empty(), "not due again within the interval");

        // Revoked token: the error is recorded, not swallowed.
        let bad = serve(vec![route("GET", "/issues", json!({"message": "Bad credentials"})).status(401)]).await;
        service::upsert_connector(&db, &settings.encryption_key, &owner, "github", None, Some(json!({"base_url": bad.base})), None)
            .await
            .unwrap();
        let out = sync_source(&db, &settings, &owner, "github").await;
        assert!(out["error"].as_str().unwrap().contains("HTTP 401"));
        let row: Value = db.query("SELECT last_error, cursor, consecutive_failures FROM ONLY $id").bind(("id", sync_status_id(&owner, "github"))).await.unwrap().take::<Option<Value>>(0).unwrap().unwrap();
        assert!(row["last_error"].as_str().unwrap().contains("Bad credentials"));
        assert_eq!(row["consecutive_failures"], 1);
    }
}

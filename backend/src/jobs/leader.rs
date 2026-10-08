//! The scheduler leader. One process at a time holds the `job_leader:scheduler` lease and, each
//! tick, re-derives periodic work from state and enqueues it with idempotency keys. Losing or
//! doubling the leader is harmless: keys dedupe, and a missed tick only delays work.

use tokio::sync::watch;

use super::{kind, NewJob, WorkerConfig};
use crate::error::AppResult;
use crate::sources::scheduler;
use crate::state::AppState;
use crate::store;

/// Embedding and stale-observation probes run every this many ticks (they scan more than they return).
const SLOW_EVERY: u64 = 10;
/// Finished jobs are kept this long (they are the idempotency record for their window).
const KEEP_DONE: &str = "7d";
/// Enqueue at most this many consolidations per owner per tick.
const STALE_BATCH: i64 = 100;

pub async fn run(state: AppState, cfg: WorkerConfig, mut shutdown: watch::Receiver<bool>) {
    let ttl = cfg.tick * 3;
    let mut ticks = 0u64;
    let mut leading = false;
    while !*shutdown.borrow() {
        match super::acquire_leader(&state.db, &cfg.id, ttl).await {
            Ok(true) => {
                if !leading {
                    tracing::info!(worker = %cfg.id, "became scheduler leader");
                }
                leading = true;
                tick(&state, ticks).await;
                ticks += 1;
            }
            Ok(false) => leading = false,
            Err(e) => tracing::warn!(error = %e.message, "scheduler lease check failed"),
        }
        tokio::select! {
            _ = tokio::time::sleep(cfg.tick) => {}
            _ = shutdown.changed() => {}
        }
    }
    if leading {
        let _ = super::release_leader(&state.db, &cfg.id).await; // else the lease just expires
    }
}

/// One scheduler pass. `n` counts the passes this leader has made.
pub async fn tick(state: &AppState, n: u64) {
    let db = &state.db;
    reconcile_syncs(state).await;
    reconcile_observations(state).await;
    if n.is_multiple_of(SLOW_EVERY) {
        reconcile_embeddings(state).await;
        if let Err(e) = housekeeping(db).await {
            tracing::warn!(error = %e.message, "job housekeeping failed");
        }
    }
}

/// Dead-letter crash-looping jobs and prune old done ones.
async fn housekeeping(db: &crate::db::Db) -> AppResult<()> {
    store::jobs::REAP_POISONED.on(db).await?.check()?;
    store::jobs::PRUNE_DONE.on(db).bind(("age", KEEP_DONE)).await?.check()?;
    Ok(())
}

/// Connector sync for every source whose interval and backoff have elapsed.
pub async fn reconcile_syncs(state: &AppState) {
    let due = match scheduler::due_syncs(&state.db).await {
        Ok(d) => d,
        Err(e) => return tracing::warn!(error = %e.message, "listing due syncs failed"),
    };
    for d in due {
        let key = format!("{}:{}:{}:{}", kind::SYNC, crate::sources::base::owner_key_str(&d.owner), d.source, super::window(d.period));
        let job = NewJob::new(kind::SYNC, d.owner, key).payload(serde_json::json!({ "source": d.source }));
        super::enqueue_lossy(&state.db, job).await;
    }
}

/// Cache records still missing an embedding, for owners that can embed.
pub async fn reconcile_embeddings(state: &AppState) {
    let Ok(owners) = owners(state).await else { return };
    for owner in owners {
        if !crate::embeddings::service::available(&state.db, &state.settings, &owner).await {
            continue;
        }
        if has_embed_backlog(&state.db, &owner).await.unwrap_or(false) {
            let key = super::periodic_key(kind::EMBED, &owner, 300);
            super::enqueue_lossy(&state.db, NewJob::new(kind::EMBED, owner, key)).await;
        }
    }
}

async fn has_embed_backlog(db: &crate::db::Db, owner: &surrealdb::RecordId) -> AppResult<bool> {
    let mut res = store::jobs::EMBED_BACKLOG.on(db).bind(("owner", owner.clone())).bind(("limit", 1)).await?;
    Ok(!res.take::<Vec<surrealdb::RecordId>>("id")?.is_empty())
}

#[derive(serde::Deserialize)]
struct StaleRow {
    owner: surrealdb::RecordId,
    subject: surrealdb::RecordId,
}

async fn stale_observations(db: &crate::db::Db) -> AppResult<Vec<StaleRow>> {
    let mut res = store::jobs::STALE_OBSERVATIONS.on(db).bind(("limit", STALE_BATCH)).await?;
    Ok(res.take(0)?)
}

/// Entities whose observation was marked stale by a fact write.
pub async fn reconcile_observations(state: &AppState) {
    let rows = match stale_observations(&state.db).await {
        Ok(r) => r,
        Err(e) => return tracing::warn!(error = %e.message, "listing stale observations failed"),
    };
    let mut can_chat: std::collections::HashMap<String, bool> = Default::default();
    for row in rows {
        let ok = match can_chat.get(&row.owner.to_string()) {
            Some(ok) => *ok,
            None => {
                let ok = crate::embeddings::service::chat_available(&state.db, &state.settings, &row.owner).await;
                can_chat.insert(row.owner.to_string(), ok);
                ok
            }
        };
        if ok {
            super::enqueue_lossy(&state.db, consolidate_job(&row.owner, &row.subject)).await;
        }
    }
}

/// `consolidate:<subject>:<10 minute window>`: one consolidation per entity per window.
pub fn consolidate_job(owner: &surrealdb::RecordId, subject: &surrealdb::RecordId) -> NewJob {
    let key = format!("{}:{}:{}", kind::CONSOLIDATE, subject, super::window(600));
    NewJob::new(kind::CONSOLIDATE, owner.clone(), key).payload(serde_json::json!({ "subject": subject.to_string() }))
}

async fn owners(state: &AppState) -> Result<Vec<surrealdb::RecordId>, ()> {
    #[derive(serde::Deserialize)]
    struct Row {
        id: surrealdb::RecordId,
    }
    async fn list(db: &crate::db::Db) -> AppResult<Vec<Row>> {
        let mut res = store::app::SOURCES_USER_IDS.on(db).await?;
        Ok(res.take(0)?)
    }
    list(&state.db).await.map(|r| r.into_iter().map(|u| u.id).collect()).map_err(|e| tracing::warn!(error = %e.message, "listing users failed"))
}

# Background jobs

One page: how work that must not block a request (syncs, embeddings, entity extraction, consolidation) is queued, claimed and recovered. Code: `backend/src/jobs/`, SQL: `backend/src/store/jobs.rs`, schema: `backend/migrations/tenant/0005_jobs.surql`. Plan: foundation-plan section 3.6.

## Table

`job` (in the current single database; `org` is NONE until the `control` database exists, so the later move is a data copy, not a redesign).

| Field | Meaning |
|-------|---------|
| `kind` | Handler name: `sync`, `embed`, `extract`, `consolidate`. |
| `owner` | `record<user>`; the fairness unit. |
| `payload` | Ids only (`{"source": "up_bank"}`, `{"record": "..."}`, `{"subject": "person:x"}`). Never content. |
| `idempotency_key` | UNIQUE. The record id is derived from it too, so a second enqueue is a no-op. |
| `run_at` | Not claimable before this time (also the retry backoff). |
| `attempts`, `max_attempts` | Attempts used (counted at claim) and the limit (default 5). |
| `locked_by`, `locked_until` | The lease: worker id and expiry. |
| `status` | `ready`, `running`, `done`, `dead`. |
| `last_error_code` | The `ErrorCode` of the last failure (see `docs/errors.md`, `job.*` rows). |
| `traceparent` | W3C trace of the request that enqueued it. |

`job_leader` holds one row, `job_leader:scheduler` (`holder`, `until`).

## States

```
ready --claim--> running --handler ok--> done
  ^                 |
  |                 +--handler error, attempts left--> ready (run_at = now + backoff)
  |                 +--permanent error, or attempts used up--> dead
  +--lease expired (crash)--(claimed again, attempts+1)
```

A job whose lease expired `max_attempts` times (a crash loop) is dead with `job.lease_expired`. Done jobs are kept 7 days (they are the idempotency record for their window) and then pruned.

## Claim

One transaction (`jobs.claim`): find owners at their running cap, select up to `n` candidates (expired leases first, then ready jobs with `run_at <= now`), and run a conditional `UPDATE` that re-checks the claim condition and sets `status = running`, `locked_by`, `locked_until`, `attempts += 1`. Two workers racing for a row conflict on commit; `tx::with_retry` re-runs the transaction, which re-reads, and the loser gets other rows or none. A row is claimed once.

The per-owner cap (`EUNOMIA_JOB_OWNER_CAP`, default 4) is soft: one claim batch can overshoot it by less than a batch.

## Leases

A lease lasts `EUNOMIA_JOB_LEASE_SECS` (default 30). While a handler runs, the worker extends the lease every third of that. Every finish (`complete`, `fail`, heartbeat) is conditional on `locked_by = me`, so a worker that lost its lease can never overwrite the new owner's result; on a lost lease the handler future is dropped. A crashed worker stops heartbeating, the lease expires, and the next claim takes the job. So a handler can run twice only after a lease expiry, and must be safe to repeat.

## Workers and roles

`EUNOMIA_ROLE=api|worker|all` (default `all`, so the one-container install keeps working). `api` serves HTTP only, `worker` runs the job loop and the scheduler leader candidate without HTTP, `all` does both. Run several `worker` containers against one database if you want more throughput; correctness does not depend on how many.

| Env | Default | Meaning |
|-----|---------|---------|
| `EUNOMIA_WORKER_CONCURRENCY` | 4 | Jobs in flight per process. |
| `EUNOMIA_JOB_POLL_MS` | 1000 | Poll interval when idle. Polling is the guarantee. |
| `EUNOMIA_JOB_LEASE_SECS` | 30 | Lease length. |
| `EUNOMIA_JOB_OWNER_CAP` | 4 | Soft cap on running jobs per owner, all workers together. |
| `EUNOMIA_JOB_GRACE_SECS` | 8 | On SIGTERM, wait this long for running jobs, then abort them. |
| `EUNOMIA_SCHEDULER_TICK_SECS` | 30 | Scheduler leader period. |

A `LIVE SELECT` on `job` wakes idle workers when a job is created. It is node-local and best effort; if it drops, polling still picks everything up within the poll interval.

On SIGTERM the worker stops claiming, waits for running jobs up to the grace period, aborts the rest and releases their leases (`status = ready`, the attempt refunded).

Failures: retry after `5s * 2^(attempt-1)` capped at 15 minutes, scaled by a random 50 to 100% (jitter). A handler returns `JobError::permanent` for input that will never work (dead at once); a panic is `job.panicked` and retried. Each attempt runs in a `job.attempt` span with an OpenTelemetry link to the enqueuing request's span, and logs and store queries carry that request's trace id.

## Scheduler leader and reconcilers

Every process with the worker role tries `UPDATE job_leader:scheduler ... WHERE holder = me OR until < now` each tick; the one that gets the row is the leader for three ticks and renews every tick. The leader runs the reconcilers, which are **level-triggered**: they look at current state and enqueue what is missing, with idempotency keys. A lost or doubled enqueue costs latency, never correctness; two leaders during a handover just produce the same keys.

| Reconciler | Looks at | Enqueues | Key | Cadence |
|------------|----------|----------|-----|---------|
| connector sync | `sync_status.last_run`, `sync_intervals`, failure backoff | `sync` per due source | `sync:<owner>:<source>:<window>` (window = now / interval) | every tick |
| embeddings | `cache_record` with `embedding` NONE, when an embeddings key exists | `embed` per owner | `embed:<owner>:<5 min window>` | every 10 ticks |
| consolidation | `memory` observations with `status = 'stale'` (set by fact writes), when an LLM key exists | `consolidate` per entity | `consolidate:<subject>:<10 min window>` | every tick |
| extraction | queued by ingest for each changed record (not polled) | `extract` per record | `extract:<owner>:<record>:<content hash>` | on ingest |

The leader also dead-letters crash-looping jobs and prunes old done jobs (every 10 ticks). The sync schedule is the old one: a source runs every `sync_intervals[source]` seconds (default 900, `heypocket` 86400); after failures it waits the longer of the interval and the backoff table (900 s doubling-ish to 6 h). A source that never ran is due immediately. `sync_status` is written exactly as before.

Without an LLM or embeddings key the `extract`, `consolidate` and `embed` handlers log `... skipped: no ... key configured` and finish.

## Add a job kind

1. Write `async fn my_job(state: AppState, job: Job) -> Result<(), JobError>` in `jobs/handlers.rs`. Re-derive what to do from the database using the ids in `job.payload`; it must be safe to run twice. Return `JobError::permanent(..)` for hopeless input, any `AppError` (via `?`) otherwise.
2. Register it in `handlers::registry()`: `.register("my_kind", my_job)`, and add the name to `jobs::kind`.
3. Enqueue with `jobs::enqueue(db, NewJob::new("my_kind", owner, "my_kind:<stable key>").payload(json!({...})))`, or `enqueue_lossy` where a reconciler would re-derive it anyway. Put a window in the key for periodic work.
4. For periodic or recovery work add a reconciler in `jobs/leader.rs` that finds the state needing work and enqueues. Add a test in `backend/tests/jobs.rs`.

## Inspect and retry dead jobs

In Surreal Studio or `surreal sql` against the Eunomia namespace and database:

```sql
-- what is stuck
SELECT kind, status, count() FROM job GROUP BY kind, status;
SELECT id, kind, owner, attempts, last_error_code, updated_at, payload FROM job WHERE status = 'dead' ORDER BY updated_at DESC LIMIT 50;

-- retry every dead job of one kind (or drop the kind filter)
UPDATE job SET status = 'ready', attempts = 0, run_at = time::now(), last_error_code = NONE
  WHERE status = 'dead' AND kind = 'sync';

-- give up on one
DELETE job:xxxx;
```

The `last_error_code` is a stable code from `docs/errors.md`; the matching log line has `job.id`, `job.kind` and the enqueuing request's `trace_id`.

## Spike S3 result

`cd backend && cargo test --release --test jobs -- --ignored job_claim_throughput --nocapture`

50,000 jobs, 6 worker loops of 16 slots each in one process, 19 random worker kills (task abort: no completion, no lease release), 3 s lease, **in-memory engine (SurrealMX via `kv-mem`), not RocksDB**:

```
S3: jobs=50000 workers=6x16 kills=19 elapsed=24.8s executions=50004 lost=0 dead=0 reclaims_after_lease_expiry=4 duplicates_before_expiry=0 claims_per_sec=2018
```

Zero lost jobs, 4 reruns (all after the first lease had expired, checked against the stored `locked_until`), no early duplicates. About 2,000 claim-and-complete cycles per second end to end, against a 200 to 500 target. Not yet measured: RocksDB, and several processes over `ws://` (S3 should be rerun on that setup before the numbers are quoted for self-host).

## Known engine caveat

The in-memory engine (SurrealMX 0.27, SurrealDB 2.7) loses about one in ten tightly synchronised write-write races: two transactions that update the same record both commit and the later one wins, instead of one failing with a retryable conflict. We reproduced it with a bare `UPDATE t:1 SET ... WHERE s = 'ready'` from 8 tasks. The claim therefore also takes an in-process lock (`tx::lock("job.claim")`), the same pattern `entities` uses. That makes several workers in one process exact. Across processes the guarantee is the storage engine's commit-time conflict check (RocksDB, which self-host uses). Rerun S3 against RocksDB with 2 API and 2 worker containers before relying on it for scale-out.

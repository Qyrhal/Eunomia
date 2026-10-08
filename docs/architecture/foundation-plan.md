# Eunomia foundation plan

Status: implemented on branch `foundation` (self-host first), 2026-10-09; see "Implementation status" below. Proposed 2026-10-08. Owner decisions that bind this plan: **Eunomia keeps SurrealDB** and **Eunomia keeps Rust**. Goal: a multi-tenant, stress-tested, well-scaling, bug-free and debuggable product that agents build and agents debug.

How this was made: six independent architects (two each on Opus, Fable and Sonnet) read the codebase and researched primary sources; a judge on a different model scored them; the result below is candidate 4 as the base with named grafts from the others. The synthesis note is at the end.

## Implementation status (2026-10-09, branch `foundation`)

Owner decisions since the plan: **self-host first**; embedding dimension stays 1536; self-host upgrades are scripted and automatic but always export first; no restricted vaults.

| Phase | State | Where |
|---|---|---|
| 0 Safety net | Done: real-SurrealDB harness, goldens for all 29 tools, axum 0.8, edition 2024, clippy `-D warnings`, CI with e2e and upgrade jobs | `backend/tests/`, `.github/workflows/ci.yml` |
| 1 Errors and tracing | Done: `ErrorCode` + problem+json, JSON logs, OTel (opt-in), SigNoz profile, trace id in UI, transactions with `with_retry`; concurrent-write test passes | `backend/src/{error,telemetry,tx}.rs`, `docs/errors.md`, `docs/observability.md` |
| 2 Contracts and store seam | Done: utoipa (all routes typed except 7 open bodies), hey-api client, TanStack Query, every statement named in `store/`, `every_query_executes` | `backend/openapi.json`, `frontend/src/lib/{gen,queries}`, `backend/src/store/` |
| 3 Versioned schema | Done: migration runner with checksum ledger, control and tenant sets, nightly encrypted backups, restore drill | `backend/migrations/`, `backend/src/migrate.rs`, `backend/scripts/backup/` |
| 4 SurrealDB 3.3 | Done in code: 3.3.1, HNSW, per-field FULLTEXT, hybrid KNN fallback, hardened `--deny-all` function list, upgrade script with verify and rollback | `docs/upgrading-to-surrealdb-3.md`, `scripts/upgrade-surreal-v3.sh` |
| 5 Org tenancy | Done in code: control database, database per org with its own user, `OrgDb`/`ControlDb` types, verified self-host move, isolation suite in every mode, mutation check | `docs/architecture/tenancy.md`, `backend/tests/isolation.rs` |
| 6 Jobs | Done: job table, leases, leader, reconcilers, extraction and consolidation wired | `docs/architecture/jobs.md` |
| 7 Team auth | Done: `authorize()`, scoped and expiring PATs, session expiry, `audit_event`, rate limits, MCP OAuth 2.1 (PKCE, CIMD, DCR, RFC 8707) | `backend/src/{authz,gate,ratelimit}.rs`, `backend/src/oauth/` |
| 8 Stress | Tooling done: k6, fuzz targets, chaos script, failure capsules and `eunomia-backend replay` | `docs/testing.md`, `docs/debugging.md` |

Spike results so far (in-memory engine, not RocksDB, so indicative only):
- S1, 200 org databases: about 7.4 MB RSS per org, cold `for_org` p50 217 ms (password check), first query 0.6 ms, KNN p50 66 ms. Fine for self-host; database per org does **not** yet look viable at 5k to 10k orgs on one node. Revisit before any SaaS launch (fallback in section 3.3).
- S3, job claims: 50k jobs, 6 workers, 19 kills, zero lost, duplicates only after lease expiry, about 2,000 claims per second. In-process claims use a lock because the in-memory engine misses some write conflicts; multi-process safety on RocksDB is still unproven.

Still to verify on real containers (Docker was unavailable while this was built): the 3.3.1 server end to end with the hardened flags, a real `export --v3` fixture, per-org backups and the restore drill on 3.3, multi-process job claims, two API plus two worker replicas under chaos, k6 and Playwright against a built stack, and spikes S2 (recall quality) and S4 (trace propagation into SurrealDB).

## 1. The target, in one table

| Layer | Choice | Runner-up | Switch trigger |
|---|---|---|---|
| Backend | Rust, Axum 0.8, Tokio, `surrealdb` SDK 3.x | Go | Agent PRs fail CI on borrow/lifetime errors more than 1 in 4 for a quarter, or the compile + test loop passes 10 minutes |
| Database | SurrealDB latest 3.3 patch, RocksDB single node for self-host, SurrealDB Cloud Scale for SaaS | Self-managed TiKV | Cloud misses RPO/RTO in a restore drill, or residency needs our own cluster |
| Tenant isolation | **Database per org** inside one namespace, each with its own database user; the app never holds root at runtime | Namespace per org (when an org needs several databases); shared DB + `org` on every row if spike S1 fails | See section 3.3 |
| In-org access | One typed `authorize()` module over vault roles | Cedar (in-process) | More than ~5 roles or customer-defined roles |
| Agent tokens | MCP OAuth 2.1 (built-in authorization server, CIMD, audience-bound, scoped, 15-minute tokens); scoped expiring PATs kept | External IdP | Enterprise SSO/SCIM |
| Background work | `job` table in the control database: transactional claim, lease, idempotency key, level-triggered reconcilers; LIVE only as a wake-up | Restate | Workflows with 3+ durable steps, human waits, or claim throughput fails spike S3 |
| Observability | `tracing` + OpenTelemetry OTLP to SigNoz (agents query it through its MCP server); SurrealDB 3.1+ OTLP telemetry; RFC 9457 errors with stable codes and `trace_id` | Grafana LGTM | An existing Grafana estate |
| Contracts | utoipa OpenAPI, `@hey-api/openapi-ts` client, CI drift check, Schemathesis | Hand-written zod | n/a |
| Testing | Real-SurrealDB integration tests, isolation proof suite, golden snapshots, migration and upgrade tests, then proptest, k6, fuzz, chaos | Deterministic simulation | A concurrency bug escapes twice |
| Frontend data | TanStack Query v5 over the generated client | Server Components for static pages | n/a |
| **Effect.ts** | **No, frontend or backend** | Only for a future TypeScript service | That service exists and needs real orchestration |
| Deploy | One backend image with `api`/`worker` roles; Compose for self-host; managed containers + SurrealDB Cloud for SaaS; no Kubernetes yet | Kubernetes + Helm | Self-run TiKV, or more than ~5 always-on services |

Licence check (verified 2026-10-08): SurrealDB is Business Source License 1.1. Production use is allowed except as a "Database Service" that lets third parties create, manage or control schemas or tables. Eunomia's users never touch schemas, so this is fine; never expose raw SurrealQL or schema control to customers. The licence converts to Apache 2.0 on 2030-01-01.

## 2. What the codebase gets right, and what blocks the goal

Keep: one tool registry for UI, chat and MCP (`backend/src/tools/registry.rs`) with one audit choke point; a stateless Streamable HTTP MCP server with Origin and version checks (`routers/mcp.rs`); parameterised queries; SCHEMAFULL tables; AES-GCM secrets and hashed tokens; recall as pure RRF fusion (`cache/recall.rs`); a real self-host story.

Blockers, by blast radius:
1. **No tenant wall in the database.** The backend signs in as root (`db.rs::connect`), so isolation is hand-written `WHERE owner/vault` clauses. Two scope models (`cache_record`, `connector`, `chat_*`, `audit_log` by owner; entities and memories by vault); `embed_cache` is global.
2. **No transactions anywhere** (verified: no `BEGIN` in `backend/src`). Read-modify-write paths such as consolidation and the memory proof-count bump can lose updates when two agents write at once.
3. **Pinned to SurrealDB 2.3.10.** `MTREE` and `SEARCH ANALYZER` in `db.rs` do not exist in 3.x; 3.x security and vector-correctness fixes never reach 2.3.
4. **No migrations.** `ensure_schema` replays `DEFINE ... IF NOT EXISTS`, so changed definitions silently never apply.
5. **Untyped errors and almost no logs.** Raw DB text leaks as 500s; no codes, request ids, spans or metrics.
6. **In-process scheduler** (`sources/scheduler.rs`): two replicas double-sync, crashes lose work; ingest extraction and consolidation are stubs (`cache/ingest.rs`).
7. **No database in CI; hand-typed frontend contract** (`frontend/src/lib/api.ts`).
8. **Thin auth for teams:** sessions without `exp`, unscoped never-expiring PATs, no MCP OAuth, no rate limits.

## 3. Per-layer design

### 3.1 Backend
Rust stays: the compiler is the cheapest reviewer of agent-written code, 15.6k working lines stay, and SurrealDB's first-class SDK is Rust. Upgrade to Axum 0.8 (`/{id}` paths) and edition 2024; add `sccache` and `cargo nextest` for loop speed.

### 3.2 SurrealDB 2.3.10 to 3.3
1. In place to the latest 2.x (2.6.1+ for Studio migration diagnostics). Run diagnostics on a copy of a real install.
2. `surreal v2 export --v3` (3.0.3+ CLI), import into a **fresh** 3.3 volume; never in place. Keep the 2.x volume for one release as rollback. 3.3+ refuses downgrades, so the pre-upgrade export is mandatory and scripted in the updater.
3. Define vector indexes explicitly rather than trusting the auto-conversion: `HNSW DIMENSION 1536 DIST COSINE TYPE F32` built `CONCURRENTLY`, queried with `<|K,EF|>`. One `FULLTEXT ANALYZER ... BM25` index per field (the two-field index splits), which also replaces the unindexed `string::contains` body scan.
4. Fix 3.x behaviour changes: SCHEMAFULL errors on extra fields, missing tables error, `SET` reads the pre-statement record, `LET` for params, `DEFINE DATABASE ... STRICT`.
5. SDK to 3.x with `#[derive(SurrealValue)]` row structs collected in one `rows.rs`.
6. Run the latest 3.3 patch (it fixes HNSW/DiskANN losing records when many share an identical vector; rebuild vector indexes after that upgrade). Pin exact patches; upgrade within 7 days of a security release, behind the upgrade suite.
7. Harden the server: `--deny-all` plus an explicit `--allow-funcs` list from a grep of our queries, `--deny-guests`, request size and concurrency limits, DB port never exposed.

### 3.3 Tenant isolation (the key decision)
**Database per org inside namespace `eunomia`**, plus a `control` database for users, orgs, memberships, sessions, tokens, the job queue and the tenant routing table. Provisioning (the only root path, compiled behind a feature flag) runs `DEFINE DATABASE org_<uuid> STRICT`, applies migrations and `DEFINE USER app ON DATABASE ... ROLES EDITOR` with a generated secret stored encrypted in `control`. Request code gets an `OrgDb` only from `pool.for_org(org_id)`; the raw client is private to the pool module and `Surreal::query` is a disallowed method elsewhere. A self-hosted single user is one org, so there is one code path.

Why, with evidence:
- SurrealDB's own benchmark (blog, 2026-08-21 measurements on 3.2.4): a small tenant's plain KNN through a shared index with a PERMISSIONS filter returns **0 rows**; filtered KNN is correct but ~175 ms; an exact scan scoped to the tenant is ~0.2 ms. A per-tenant database gives each org its own HNSW index, so the filter never fights the ANN walk.
- PERMISSIONS do not bind system users, so a shared-database wall would require a record-user session per request; the Rust SDK's cost for that is unverified.
- 2026 bypasses landed exactly in the permission path (CVE-2026-63733; the 3.2.1 KNN SELECT permission fix).
- Per-org export, restore, deletion and moving a whale to a dedicated instance become one database operation.

Inside an org, vault scope is the typed `VaultScope` built only by `authorize()`, bound into every store query including the KNN `WHERE`, with a hybrid exact-scan fallback when KNN returns fewer than K rows (from candidate 1).

**Spike S1 (2 days) decides it:** 5,000 to 10,000 org databases on 3.3, each with HNSW(1536) + two FULLTEXT indexes + 1k records. Must hold memory, startup time, recall p95 under 150 ms with a cold pool, and a one-field migration fan-out under 30 minutes. Also confirm that 3.x SDK session cloning isolates database and auth state. **If it fails:** shared `main` database, `org` on every row, compound `(org, vault)` index, filter inside the KNN `WHERE`, hybrid fallback, and database per org only as an enterprise tier.

**Proving no leaks (every PR):**
1. Isolation property test: two orgs with canary strings in every table; call every registry tool and route as A with B's ids, names and vault ids; assert no B data in any response, error, log line, span or side effect.
2. Credential wall test: org A's `OrgDb` attempting org B's database gets an auth error.
3. Layer-removal runs (from candidate 6): run the suite with app filters disabled (the database wall must hold) and with the wall replaced by a shared test database (the app filters must hold), proving each layer stands alone.
4. Harness mutation check (from candidate 3): delete one filter in a throwaway build; the suite must go red.
5. Static gates: no root sign-in outside `provisioning/`, no `Surreal::query` outside `store/`, no `USE` inside query strings.
6. Runtime: a "query without org context" counter that alerts at any non-zero value (from candidate 5).

### 3.4 Schema and migrations
Numbered files in `backend/migrations/{control,tenant}/NNNN_name.surql`, embedded with `include_str!`; a `_migration` ledger with version and checksum (a changed checksum fails boot); `DEFINE ... OVERWRITE` for real changes; expand then contract for destructive changes; backfills are jobs. Tenant fan-out is a resumable job per org; code supports schema N and N-1, and the API answers `tenant.schema_behind` for orgs below that (from candidate 1). Tests: fresh-apply equals upgraded (`INFO FOR DB STRUCTURE`), a committed schema snapshot, upgrade-with-data goldens. The runner is owned code (~150 lines); third-party crates call themselves not production-ready.

Compile-time safety without sqlx: every query is a typed constant in `store/` with a params struct and a row type, and an `every_query_executes` test runs each one against a freshly migrated 3.3 database (from candidate 1). A `docs/surrealql-3.md` cheat sheet keeps agents off 2.x syntax.

### 3.5 Transactions (from candidate 2)
Wrap every read-modify-write path (consolidation, the memory proof-count bump, invitation accept, job claim) in `BEGIN ... COMMIT` with a `with_retry(3)` helper on SurrealDB's commit-time conflict error. Snapshot isolation does not stop write skew, so invariants (one personal vault per user, one `sync_status` per source) are `UNIQUE` indexes. Gate: two agents write memory for one entity 200 times concurrently and the proof counts must sum exactly. This test fails today.

### 3.6 Background work
`job` table in `control`: `org, kind, payload (ids only), idempotency_key UNIQUE, run_at, attempts, max_attempts, locked_by, locked_until, status, last_error_code, traceparent`. Claim with a conditional `UPDATE ... WHERE status = 'ready' OR locked_until < now` inside a transaction; a conflict or empty result means another worker won. Leases make crashes safe. Fairness caps running jobs per org.

Because tenant writes and the job row live in different databases, jobs are **level-triggered**: each re-derives its work from state ("embed records where `embedding` is NONE", "consolidate entities marked stale"), and a per-org reconciler re-enqueues anything missed. A lost enqueue costs latency, never correctness. `DEFINE EVENT ... ASYNC` only for in-database derivations such as marking a memory stale. One scheduler leader via a lease row replaces the per-user interval loops. LIVE SELECT wakes workers early; polling stays the guarantee (LIVE is node-local in Community and has had stability fixes).

Spike S3 (2 days): 4 to 8 workers, 50k to 100k jobs, random kills; zero lost jobs, double execution only after a lease expiry, claim throughput target 200 to 500 jobs per second.

### 3.7 Observability and agent-debuggability
- Spans on every route, registry tool, store query (statement name, org, rows, duration, retries) and job attempt, exported over OTLP. Every store query carries a `-- trace:<id> op:<name>` comment so SurrealDB's slow-query log joins the trace even if the server does not propagate W3C context (spike S4 checks).
- `AppError { code, status, message, source }` as RFC 9457 `application/problem+json` with `code` and `trace_id`; DB errors mapped to codes (conflict is retryable, permission error is a high-severity `tenant.denied` alert); raw DB text only on the span. MCP tool errors carry the same code and trace id. A generated `docs/errors.md` maps each code to its module.
- Browser `traceparent` through the Next proxy, Axum, MCP span, job row and outbound calls; the UI shows the trace id with a copy button.
- SigNoz in an optional Compose profile; agents query traces and logs through its MCP server.
- Replay: redacted failure capsules (from candidate 6) for 5xx and tool errors; `eunomia replay <trace_id>` reruns through the registry against a scratch org database restored from that org's export (database per org makes this one import).

### 3.8 Testing, ranked by bugs caught per agent-hour
1. Integration tests on real SurrealDB (in-memory engine per test plus one job on the release server image). Build this harness **first**, before any refactor (from candidate 3).
2. The isolation proof suite (3.3).
3. Golden snapshots of every tool's output, which guard the upgrade and the tenant split.
4. `every_query_executes`, the schema snapshot, migration and 2.x-to-3.x upgrade tests, and a monthly restore drill.
5. Contracts: OpenAPI drift gate, Schemathesis, MCP `tools/list` and error-shape snapshots per protocol version.
6. proptest: RRF fusion, ingest idempotency, consolidation versioning, the job state machine, conflict retries.
7. The concurrent memory-write test (3.5).
8. k6 nightly with Zipf-sized orgs and a fairness assertion; cargo-fuzz on JSON-RPC and connector mappers; chaos (kill workers, restart SurrealDB mid-transaction).
9. Deterministic simulation deferred.
Rule: a bug-fix PR includes a test that fails without the fix.

### 3.9 Frontend and Effect.ts
Generated client and TanStack Query hooks from the Rust OpenAPI spec replace `frontend/src/lib/api.ts` page by page; errors surface `code` and `trace_id`. **Effect.ts: no.** The backend is Rust, which already gives typed errors, resource safety and structured concurrency. The frontend's real problems are contract drift and caching, which codegen and TanStack Query solve in the style agents know best. Effect 4 shipped about 2026-09-30 as a ground-up rebuild with renamed core APIs, so agents' training data is mostly the old dialect. Revisit only for a future TypeScript service that needs real orchestration.

### 3.10 Deployment, HA and scaling
- Self-host: Compose with pinned 3.3 on RocksDB, `backend` (`--role all`), `frontend`, a `backup` service (nightly per-database export plus volume snapshot, encrypted, rotated) and optional observability. State plainly that a single node has no HA and RPO equals the backup interval.
- SaaS: managed containers plus SurrealDB Cloud Scale (three-node minimum, launched July 2026, treat its HA as unverified until a failover drill). Cloud snapshots are not downloadable, so also run our own per-org exports to our bucket.
- Scaling: stateless API replicas, workers scaled on queue depth, per-org connection pool; move a whale org to a dedicated instance by exporting one database.
- Noisy neighbours: `tower_governor` per org and token, per-org quotas and job caps, `TIMEOUT` on recall statements, per-org embedding spend caps.

## 4. The agent-debugging loop
1. The report carries `code` and `trace_id` (UI toast, MCP error, alert).
2. The agent pulls the trace through the SigNoz MCP server: route, tool, store spans with org and statement names, the linked job, the outbound call.
3. The code maps to one module through `docs/errors.md`.
4. `eunomia replay <trace_id>` reproduces it against a scratch org database; the agent turns that into a failing test.
5. The agent fixes the root cause at the shared point; the isolation suite, `every_query_executes`, goldens, OpenAPI diff and MCP snapshots all pass.
6. The PR carries the trace link and the new test.
7. After deploy, the agent watches the code's rate per org drop, retries dead jobs (safe, they are idempotent), and records the lesson with `memory_write` on the repository entity.

## 5. Migration plan
Each phase ships on `main` and keeps the one-curl install working. Spikes come before the phases they de-risk. Effort is engineer-weeks with agents doing most of the typing.

| Phase | Scope | Done when | Effort |
|---|---|---|---|
| 0. Safety net | Real-SurrealDB integration harness on today's 2.x, golden snapshots of every tool, Playwright in CI, Axum 0.8, clippy `-D warnings` | DB-backed CI green; goldens cover every registry tool | 1 to 1.5 w |
| 1. Errors and tracing | `ErrorCode` + problem+json, request ids, JSON logs, spans, OTLP, SigNoz profile, trace id in the UI, `with_retry` transactions on the four read-modify-write paths | Every error has a code and a resolvable trace; the concurrent memory-write test passes | 1.5 w |
| 2. Contracts and store seam | utoipa + generated client + TanStack Query; all queries into `store/` as typed constants; `every_query_executes` | Hand-typed `api.ts` gone; no `db.query` outside `store/` | 1.5 to 2 w |
| 3. Versioned schema on 2.x | Migration runner, ledger, snapshot test, backup service and restore drill, move to latest 2.x and run diagnostics | Fresh and existing installs reach the same snapshot; restore drill green | 1 w |
| Spikes S2, S4 | 3.3 recall quality and latency vs the 2.3 golden set; trace propagation into SurrealDB | Recall equal or better, p95 within budget | 2.5 d |
| 4. SurrealDB 3.3 | 3.x migrations (HNSW, per-field FULLTEXT, STRICT), SDK 3.x, hardened flags, installer export/import/verify/rollback | A real 1.2.x install upgrades in CI with identical counts and goldens | 2 w |
| Spike S1 | Database per org at 5k to 10k orgs | Section 3.3 targets met, or the fallback is chosen | 2 d |
| 5. Org tenancy | `control` database, org model, provisioning, per-org users, `OrgDb` pool, per-org data move (resumable, verified by counts), per-org `embed_cache`, the full isolation proof suite | Isolation suite green in every mode; no request path holds root | 3 to 4 w |
| Spike S3 | Job claim correctness and throughput | Section 3.6 targets met | 2 d |
| 6. Jobs | Job table, worker role, leader lease, reconcilers, ASYNC events for staleness, wire the extraction and consolidation stubs, delete the interval loops | Two API + two worker replicas, zero duplicate syncs under chaos | 1.5 to 2 w |
| 7. Team auth | `authorize()` matrix, MCP OAuth AS, scoped expiring PATs, session `exp`, append-only `audit_event` with trace ids, rate limits and quotas | Claude Code and Cursor connect via OAuth without a pasted token; a vault-scoped token gets 403 elsewhere | 2 to 3 w |
| 8. Stress and SaaS | k6 nightly, fuzz, chaos, replay CLI, Cloud Scale staging with a failover drill | Two weeks of green nightlies; failover drill documented | 1 to 2 w, then ongoing |

Total: about 15 to 19 engineer-weeks plus about 9 spike-days. Phases 0 to 3 deliver most of the debuggability without touching the engine version. Owner go-ahead is needed before phases 4 and 5.

Deferred until their triggers fire: record-level PERMISSIONS inside an org (restricted vaults), DiskANN, TiKV, Restate, Cedar, Kubernetes, deterministic simulation.

## 6. Rejected
Postgres or another primary database (owner decision); staying on 2.x; shared database with PERMISSIONS as the main wall (empty or slow filtered KNN, system users bypass, recent bypass CVEs); app-only scope with a root connection (today); namespace per org as the default (no gain over database per org until an org needs several databases); LIVE queries as the queue; Temporal, Redis, NATS (extra stateful services in a one-curl install); Apalis (no stable SurrealDB backend); OpenFGA/SpiceDB now; rewrites in Go, TypeScript + Effect or Elixir; Effect.ts; tRPC; Kubernetes now; deterministic simulation now.

## 7. Open questions for the owner
1. SaaS on SurrealDB Cloud first, self-host first, or both equal? It orders phases 5 to 7.
2. Expected orgs, largest org (records, vectors) and recall p95 budget in 12 months? These are spike S1's targets.
3. One embedding model and dimension per org (1536 today, 384 in `docs/research`)? Must be fixed before phase 4.
4. Must existing self-host installs upgrade automatically (export, import, move into an org database), or is a guided upgrade acceptable?
5. Will orgs need restricted vaults that some members cannot see? That triggers record-level PERMISSIONS.

## Synthesis note
- **Base:** candidate 4 (Opus). Cross-judge (Fable) scores: C1 28, C4 28, C2 27, C5 27, C3 22, C6 22; it recommended C4 for the best agent loop and full grounding. My own read favoured C2 until the judge showed C2 wrongly called 3.3 beta; the judge's pick stands.
- **Isolation verdict:** per-tenant database with a per-tenant user (4 of 6 candidates; judge agrees). Database rather than namespace per org (C2, C5, judge). Verified against SurrealDB's filtered-KNN benchmark before adopting.
- **Grafts:** C1 (3.3 target with upgrade spike, hybrid KNN fallback, N/N-1 tenant migrations, `every_query_executes`); C2 (transactions with retry and the concurrent-write test, verified by grep that no transactions exist today; server hardening flags); C3 (harness mutation check; DB-backed harness first); C5 (`OrgDb` type-state, no-org-context metric, 3.3 identical-vector fix); C6 (layer-removal runs, failure capsules).
- **Rejected from candidates:** C6's per-request JWT PERMISSIONS as the main wall (unverified SDK cost, evidence against filtered KNN); C3's app scope as the main wall; C2's 3.2.5 target.
- **Convergence (no graft needed):** Rust + Axum 0.8, 3.x via 2.6 diagnostics and v3 export, owned migration runner, SurrealDB job table, OTel + RFC 9457, OpenAPI codegen + TanStack Query, no Effect.ts, Cedar later, no Kubernetes, Cloud Scale for SaaS.
- **Verification:** licence text read from the SurrealDB repo; no `BEGIN` anywhere in `backend/src` (lost-update risk confirmed); filtered-KNN numbers read from SurrealDB's own post; 3.3.0 confirmed stable on GitHub releases. Not yet verified, and gated by spikes: per-org scale (S1), 3.3 recall (S2), job claim behaviour (S3), trace propagation (S4).
- **Dropouts:** none. Candidates 1, 3 and 6 first recommended Postgres and were revised after the owner's decision.

# Org tenancy

One page: how an org's data is kept apart from every other org's, how that is created and moved, and how it is proved. Code: `backend/src/pool.rs`, `backend/src/provisioning/`, `backend/src/store/`, `backend/src/migrate.rs`. Proof: `backend/tests/isolation.rs`. Plan: foundation-plan section 3.3 and phase 5. Bench: `backend/tests/spike_s1.rs`.

## Layout

Namespace `eunomia` (`SURREAL_NS`) holds one `control` database and one `org_<32 hex>` database per org.

| Database | Tables | Migrations |
|----------|--------|------------|
| `control` | `user`, `org`, `membership` (user, org, role), `tenant` (org to database name, schema version, status, encrypted database password), `session`, `api_token`, `oauth_*`, `audit_event`, `job`, `job_leader`, `failure_capsule` | `backend/migrations/control/` |
| `org_<uuid>` (`STRICT`) | `vault`, `vault_member`, entity tables, `memory`, `relates_to`, `cache_record`, `linked_to`, `connector`, `sync_status`, `app_settings`, `chat_*`, `audit_log`, `embed_cache` | `backend/migrations/tenant/` |

`store::CONTROL_TABLES` and `store::TENANT_TABLES` list them; a test fails if either list disagrees with the migrations. A record in an org database may name a control record (`owner = user:abc`), but the link is just an id: nothing joins across databases. Where a response needs a user's email (vault members, who wrote a memory) the service reads the ids from the org database and the emails from `control`.

A user's home org is their oldest `membership`. A session, token or OAuth grant resolves to the user and then to that org; `User.org` rides on the request. Signup puts a new user in the install's one org (the first user of a fresh install creates it and owns it); `EUNOMIA_SIGNUP_ORG=personal` gives every signup an org of their own. Inviting a user to a vault looks the email up inside the inviter's org only, so an email cannot be probed across orgs.

## Handles: who may query what

- `ControlDb` and `OrgDb` (in `pool.rs`) wrap a SurrealDB session. The raw client is a private field; `.raw()` is `pub(crate)` and a static test allows it only in `store/`, `pool.rs` and `provisioning/`.
- A statement is `Stmt<Tenant>` or `Stmt<Control>` (`ControlStmt`). `Stmt<Tenant>::on` takes `&OrgDb`, `Stmt<Control>::on` takes `&ControlDb`, so the compiler decides which database it runs in. `store::dynamic(&OrgDb, ..)` and `store::dynamic_control(&ControlDb, ..)` are the escape hatches for SQL built at runtime.
- `Pool::for_org(&OrgId)` is the only way to get an `OrgDb`. On a miss it reads the org's `tenant` row, decrypts the database password (AES-GCM, `connectors/crypto.rs`) and signs a fresh session in as that org's **database-level** user (`app`, `ROLES EDITOR`). Handles live in an LRU cache (`EUNOMIA_ORG_POOL_CAP`, default 256). SurrealDB 3.x gives every `clone()` of the client its own session (own namespace, database and credentials), so one connection serves every org and nothing the pool does to one org's session touches another's.
- A database user cannot `USE` another database, so even a statement with no `WHERE owner = ...` can only see its own org. The app-level owner and vault filters are the second layer, for people inside one org.
- `AppState` has `control` and `pool` and no database of its own. A handler does `let state = state.org(&user.org).await?;` and gets an `OrgState` (`state.db` is the `OrgDb`; `state.control`, `state.settings` and the rest still work).
- Counter `pool::NO_ORG_CONTEXT`: bumped (and logged at error level) when an org with no ready database is asked for, or when a statement names a table of the other database (`store::crossing_table` looks at the word after FROM, INTO, UPDATE, DELETE, CREATE, UPSERT and the edge in RELATE). Exposed at `GET /api/debug/metrics` (instance admin) as `queries_without_org_context`. It must stay 0.

## Provisioning (the only root path)

`backend/src/provisioning/` is the only code that signs in as the SurrealDB root user. It is compiled behind the cargo feature `provisioning`, on by default (`default = ["provisioning"]`). A binary built with `--no-default-features` cannot sign in as root: boot skips setup, the control database must already exist, and signup answers `tenant.provisioning_disabled`. The `Provisioner` (which owns the root session) lives in `AppState` for the process because signup (a new org) and the `migrate_tenant` job call it; no query a request runs for a user goes through it, and a `--no-default-features` build has none. A static test (`tests/store.rs`, `raw_database_access_stays_in_its_modules`) fails if `signin(`, `auth::Root`, `.use_db(` or a `USE ...` statement appears anywhere else.

At boot (`AppState::build`): root signs in, the namespace and `control` database are defined, `DEFINE USER OVERWRITE app ON DATABASE control ... ROLES EDITOR` with a password derived from `ENCRYPTION_KEY` (HMAC, nothing stored), control migrations run, a pre-tenancy install is moved (below), and the root session stays inside the `Provisioner`. Creating an org (`Provisioner::provision_org`):

1. `org` record, and a `tenant` row in state `provisioning` holding the generated password, encrypted. It is written first so a crash resumes with the same password.
2. `DEFINE DATABASE IF NOT EXISTS org_<uuid> STRICT`. STRICT means a query naming a table the migrations did not define fails instead of reading nothing; migrations define everything, so provisioning needs nothing non-strict.
3. Tenant migrations (ledger `_migration`, checksums, same rules as before) and `DEFINE USER OVERWRITE app ON DATABASE ... ROLES EDITOR`.
4. `tenant` row to `ready` with `schema_version`.

Schema N and N-1: `Pool::for_org` serves an org whose `schema_version` is `LATEST_TENANT - 1` or newer and answers `tenant.schema_behind` (503) below that. After an upgrade the scheduler leader queues one `migrate_tenant` job per org behind (key `migrate_tenant:<org>:<version>`), the worker runs `Provisioner::migrate_org`, which is idempotent, so a crash just reruns it.

## The self-host move

A pre-tenancy install keeps everything in one database (`SURREAL_DB`, default `eunomia`). On the first boot of this version, if that database has users and `control.tenant` has no rows, `provisioning::legacy::move_if_needed` runs:

1. Bring the old database to tenant schema 8 (an install that arrived by the 2.x to 3.x export has its old ledger; 0009 would drop the tables being read, so it is never applied there).
2. Create the org `Default`, its `tenant` row in state `moving`, and the `org_<uuid>` database.
3. Copy accounts, tokens, sessions, OAuth, audit events, jobs and capsules to `control` and every org table to the org database, 100 rows at a time in id order, `INSERT IGNORE` by record id (so a re-run skips what is there). Record ids are unchanged: sessions, tokens and OAuth grants keep working. Every user becomes a member; the oldest is the owner.
4. Verify per-table row counts, old against new. Any mismatch fails the boot and leaves the org `moving` (not served).
5. Mark the org `ready`. The old database is **never deleted**: the log line at the end gives the `REMOVE DATABASE` command to run once you have checked.

Re-running is a no-op (a `ready` tenant exists), and an interrupted run resumes (a `moving` tenant exists). Tests: `backend/tests/tenancy.rs`.

## How isolation is proved

`backend/tests/isolation.rs`, every PR (`cargo test --test isolation`):

1. **Canaries.** Two orgs; each has a unique canary string in every org table and in every control table that has per-org rows (tables with no free text carry it in the record id). The test fails if any table lacks one.
2. **Everything is called.** As org A, using B's ids, vault ids and names: every registry tool (29), directly and over `/mcp`, and every route in the OpenAPI spec (72) with generated path, query and body values. No B canary or B org key may appear in a response body, an error, a captured log line or span, or in an audit or capsule row written for A. B's rows are counted before and after: unchanged. Then the same with B attacking A. A control group checks each org does see its own canary through the same tools, so the detector is not blind.
3. **Credential wall.** A's password does not sign in to B's database; A's live handle cannot `USE DB` B's database or the control database, by the client or inside a query; an org user cannot create database users; the control handle cannot read an org database.
4. **Layers apart.** Mode `NoAppFilters` (feature `test-support`, `isolation::set_no_app_filters` or `ISOLATION_TEST_NO_APP_FILTERS=1`) rewrites every owner, vault and user predicate in org-database statements to `true`: the database wall alone must hold. Mode `SharedDb` points every org at one database (`Pool::set_shared_database`): the app filters alone must hold. A third test turns both off at once and expects the leak, so the suite measures the layers and not the data.
5. **Mutation check.** `isolation::mutate("app.chat_thread_list", "WHERE owner = $owner", "WHERE true")` (and two more statements) in shared-database mode must make the suite report a leak through the matching route.
6. **Static gates** (`tests/store.rs`): no `signin(` / root outside `provisioning/` and `pool.rs`; no `USE` in statement text; no `.query(` / `.raw()` outside `store/` and `pool.rs` (`provisioning/` and `migrate.rs` go through `store::root`); every statement names only its own database's tables.
7. **Runtime metric**: `queries_without_org_context` is 0 at the end of every mode and at `/api/debug/metrics`.

The test-only switches live in `src/isolation.rs`, compiled only with the `test-support` feature (the dev-dependency on the crate itself turns it on for `cargo test`); a release build has no way to turn a filter off.

## Add a table to an org

1. Add `migrations/tenant/NNNN_name.surql` (next number after 0009), `DEFINE ... OVERWRITE`, register it in `MIGRATIONS` in `src/migrate.rs`. Org databases are `STRICT`: define the table, every field and index there.
2. Add the table name to `store::TENANT_TABLES` (a test checks it against the schema).
3. Write statements in `store/` as `Stmt` (tenant) and call them with the `OrgDb`. Filter by `owner` or `vault` as the existing ones do; the shared-database mode will fail the isolation suite if you forget.
4. Plant a canary row for it in `plant()` in `tests/isolation.rs` (the suite fails until you do), regenerate the schema snapshot (`INSTA_UPDATE=always cargo test --test migrations`) and review it.

A control table is the same with `migrations/control/`, `store::CONTROL_TABLES` and `ControlStmt`.

## Operating notes and limits

- Backups must cover every database in the namespace (control and each `org_*`); see the backup service notes in `docs/deployment.md`.
- `ENCRYPTION_KEY` now also guards the per-org database passwords and derives the control user's password. Losing or changing it makes org databases unreachable until their users are redefined; keep it in the backup. The backend refuses to boot with an empty key on a fresh install; an install that already has orgs (provisioned under the empty key) still boots, with an error in the log, and moves to a real key as described in [deployment.md](../deployment.md).
- Signup holds a per-process lock while it creates an org (rare, and keeps "the first user creates the instance org" true).
- Spike S1, `cargo test --test spike_s1 -- --ignored --nocapture` (`S1_N=200`): 200 org databases on the in-memory engine, debug build, 100 records of 1536 dims each, HNSW and two FULLTEXT indexes. Provisioning one org (database, 9 migrations, user, routing row): p50 367 ms. Loading 100 records: p50 676 ms. Memory: 78 MB at start, 1,564 MB after 200 orgs (about 7.4 MB per org with its data). Cold `pool.for_org` (sign in as the org's database user): p50 217 ms, p95 218 ms, almost all of it the password hash check; first query after it 0.6 ms; a k=10 vector search on the cold handle p50 66 ms, p95 67 ms. A one-field migration took 0.5 ms per org (it adds an empty option field; a field that rewrites rows or rebuilds an index will cost more). These are a debug build on one machine against the embedded engine, not the plan's 5,000 to 10,000 orgs on a server: rerun it there before choosing the SaaS shape, and rerun it over `ws://` once Docker is available (the pool relies on several sessions sharing one websocket, which only the embedded engine has been exercised with).

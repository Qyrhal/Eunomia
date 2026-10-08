# Testing

Every test layer, from cheapest to most destructive. Run the first six on every change. The rest need a stack or special tooling.

| Layer | What it proves | Where | Run |
| --- | --- | --- | --- |
| Unit | Pure logic in each module | `#[cfg(test)]` blocks in `backend/src/**` | `cd backend && cargo test --lib` (or `cargo nextest run --lib`) |
| Real-DB harness and goldens | Tools, routes and concurrent writes against a real in-memory SurrealDB; tool output shapes pinned by `insta` snapshots | `backend/tests/` (`common/`, `golden_tools.rs`, `http_routes.rs`, `concurrent_memory.rs`, `snapshots/`) | `cd backend && cargo test` ; review snapshot changes with `cargo insta review` |
| Isolation proof | Two orgs, canaries in every table, every tool and route called as one org with the other's ids: no leak with both layers, with the app filters off, or with a shared database; the harness fails when a filter is removed; credential wall; static gates | `backend/tests/isolation.rs`, `backend/tests/store.rs`, `docs/architecture/tenancy.md` | `cd backend && cargo test --test isolation` |
| Tenancy and the self-host move | Provisioning, the pool, schema N-1 gating, the verified resumable move of a single-database install (the 2.x export fixture) | `backend/tests/tenancy.rs` | `cd backend && cargo test --test tenancy` |
| Script tests | Shell scripts (auto-update, agent connect, backup and restore of every database) against a mock server | `scripts/tests/` | `bash scripts/tests/auto-update.test.sh`, `bash scripts/tests/backup.test.sh` and `bash scripts/tests/connect-agents.test.sh` |
| Playwright e2e | The real UI and `/mcp` through the frontend proxy | `frontend/tests/` | Start backend and SurrealDB (`docker compose up -d surrealdb backend`), then `cd frontend && bun run test` (`E2E_BASE_URL=...` to target a running stack) |
| k6 load | Latency, error rate and noisy-neighbour fairness over MCP | `loadtest/k6/` | `k6 run loadtest/k6/noisy.js`, see `loadtest/k6/README.md` |
| Fuzz | Parsers never panic on hostile input (MCP request parsing, every connector `map`, summary functions) | `backend/fuzz/` | `cd backend && cargo +nightly fuzz run source_map` (also `jsonrpc_parse`, `summaries`). Needs `cargo install cargo-fuzz` and a nightly toolchain. Without them, `cd backend/fuzz && cargo check --bins` still type-checks the targets |
| Chaos | Killing the backend and DB mid-load loses no acknowledged records and `/healthz` recovers | `scripts/chaos.sh` | `CHAOS_CONFIRM=yes scripts/chaos.sh` |
| Restore drill | Backups actually restore (seed, back up, wipe, restore, compare counts) | `scripts/restore-drill.sh` | `scripts/restore-drill.sh` |

## Safety

- k6 registers users and writes memories. Point it at a test stack only.
- Chaos kills containers. It only touches compose project `fw-chaos` (own volumes, port `18101`), refuses ports used by a normal install, and needs `CHAOS_CONFIRM=yes`. Never run it against a real install.
- The restore drill uses its own throwaway database container `fw-backup-db`, never the live stack.
- Do not run any of these against the containers or volumes of a real `eunomia` compose project.

## Seeing a failure

For a failure in a running instance (a trace id from the UI or an MCP error), see [debugging.md](debugging.md): `eunomia replay <trace_id>` re-runs it and `--emit-test` writes the failing test.


Fuzz crashes are saved under `backend/fuzz/artifacts/<target>/`; replay with `cargo +nightly fuzz run <target> <file>`. Chaos leaves k6 output in `/tmp/fw-chaos-k6.log`. k6 prints per-threshold pass or fail at the end of every run.

## Spike S1: database per org at scale

`cd backend && S1_N=200 cargo test --test spike_s1 -- --ignored --nocapture` (add `--release` for numbers closer to a server). Provisions N org databases on the in-memory engine, each migrated (HNSW 1536 and two FULLTEXT indexes) and loaded with 100 records, then prints provisioning time per org, resident memory, cold `pool.for_org` and first-query latency, vector-search latency on a cold handle, and how long a one-field migration takes to fan out over every org. A script, not a gate.

# Testing

Every test layer, from cheapest to most destructive. Run the first four on every change. The last four need a stack or special tooling.

| Layer | What it proves | Where | Run |
| --- | --- | --- | --- |
| Unit | Pure logic in each module | `#[cfg(test)]` blocks in `backend/src/**` | `cd backend && cargo test --lib` (or `cargo nextest run --lib`) |
| Real-DB harness and goldens | Tools, routes and concurrent writes against a real in-memory SurrealDB; tool output shapes pinned by `insta` snapshots | `backend/tests/` (`common/`, `golden_tools.rs`, `http_routes.rs`, `concurrent_memory.rs`, `snapshots/`) | `cd backend && cargo test` ; review snapshot changes with `cargo insta review` |
| Script tests | Shell scripts (auto-update, agent connect) against a mock server | `scripts/tests/` | `bash scripts/tests/auto-update.test.sh` and `bash scripts/tests/connect-agents.test.sh` |
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

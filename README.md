<div align="center">

# Eunomia

**A Super Intelligence Agent AI memory system, built for enterprise scale in Rust.**

Vaults · an entity/memory graph · external connectors · a tool-calling chat agent —
all on one [SurrealDB](https://surrealdb.com/) schema, all exposed identically to the UI and the agent.

[![CI](https://github.com/Qyrhal/Eunomia/actions/workflows/ci.yml/badge.svg)](https://github.com/Qyrhal/Eunomia/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/Qyrhal/Eunomia?label=release)](https://github.com/Qyrhal/Eunomia/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/backend-Rust%20%2F%20Axum-dea584)](backend)
[![Next.js](https://img.shields.io/badge/frontend-Next.js%20%2F%20TypeScript-000000)](frontend)

```bash
curl -fsSL https://midhunkumar05.github.io/eunomia/install.sh | bash
```

[Install](#install) · [Quickstart](docs/quickstart.md) · [What's in it](#whats-in-it) · [Docs](docs/README.md) · [Install page](https://midhunkumar05.github.io/eunomia/)

</div>

---

## Install

```bash
curl -fsSL https://midhunkumar05.github.io/eunomia/install.sh | bash
```

It asks a few questions (Enter accepts each default), then starts SurrealDB +
backend + frontend, turns on one-click updates (Settings → Updates), creates
your account, and connects your AI agents (Claude Code, Codex, Hermes, Gemini
CLI, Cursor, Windsurf, OpenCode, VS Code, Claude Desktop) over MCP. Run by an agent, it connects that
agent. No OpenAI key needed: the agent is the model. See the
[Quickstart](docs/quickstart.md) and [Installation](docs/installation.md).

## How it fits together

```
 sources ──sync──▶ ingest pipeline ──▶ cache (SurrealDB: rows + FTS + vector index)
 (Up Bank,                │  embed + extract entities    │
  heypocket, demo)        └───────────────────────────────┼──▶ tools: search / get / list / links
                                                            │    + per-source tools
                                                            ▼
                                     REST /api/* (:8001)  ◀── frontend (:3000)
```

- **Sources** are plug-ins under `backend/src/sources/`; each registers
  itself (`auth` / `sync` / `map`).
- **Auth** is per-user: register/login issues a signed session cookie for
  the browser; a personal long-lived API token (minted in Settings or via
  `POST /api/auth/tokens`) authenticates other clients instead.

## What's in it

<table>
<tr><td width="50%" valign="top">

**Entity-memory graph**
People, organisations, locations — auto-extracted from your synced data,
rendered as a draggable/zoomable force-directed graph with kind-filter
pills and avatars.

**Code-entity graph**
A separate, cross-linkable graph of repositories/files/symbols, populated
by an agent calling `code_entity_upsert`/`code_relate` as it works in a
codebase — not static analysis.

**`recall()`**
A 4-arm parallel retrieval pipeline (semantic / keyword / graph /
temporal), fused with Reciprocal Rank Fusion and boosted by recency and
proof count.

**Observation consolidation**
Raw memory facts synthesized into evolving "belief" observations per
entity, with freshness, proof-count, and versioning.

</td><td width="50%" valign="top">

**One shared tool registry**
`search` / `get` / `list` / `links`, `recall`, `memory_write`,
`entities_search` / `entities_get` / `entities_graph`,
`consolidate_observations`, `code_entity_upsert` / `code_relate`, vault
management, plus per-source tools — called by both the chat agent and
`POST /api/tools/:name`.

**`/chat`**
Talk to the configured OpenAI-compatible model, streamed over SSE, with
full tool access to recall/write/modify memory through the same shared
registry.

**Sources & connectors**
Up Bank, HeyPocket (meeting recordings/transcripts, synced every 24h), a
generic Open Connector bridge, and a demo source for trying the app
without creds. Marketplace-style grid with per-connector setup pages.

**Multi-user accounts**
bcrypt password hashing, JWT session cookies, personal API tokens, vaults
scoped per-user or per-team.

</td></tr>
</table>

## Stack

| | |
|---|---|
| **Backend** | Rust, [Axum](https://github.com/tokio-rs/axum), the official SurrealDB SDK |
| **Database** | [SurrealDB](https://surrealdb.com/) — graph relations, full-text search, and vector search (MTREE) in one engine |
| **Frontend** | Next.js, TypeScript |
| **Deployment** | Docker Compose |

Inspired by [Hindsight](https://vectorize.io/) (vectorize.io) for the
observation-consolidation model, and built on [SurrealDB](https://surrealdb.com/).

## Connecting an agent (MCP)

Eunomia is an MCP server (Streamable HTTP) at `<your Eunomia URL>/mcp`,
exposing the same tools the built-in chat agent uses (`recall`, `reflect`,
memory CRUD, the entity/code-graph and vault tools, `docs`), with
read-only/destructive hints. Calls are scoped to the token's owner and
mutating calls are audit-logged. The installer wires up your agents;
afterwards use `./scripts/connect-agents.sh` (see
[AI agents & MCP](docs/agents.md)). Plain REST works too: `POST
/api/tools/:name` with the same bearer token.

## Connectors

Up Bank: a personal access token from api.up.com.au. HeyPocket: an API key.
Both are entered on the Connectors page and stored encrypted at rest (AES-
256-GCM); Open Connector is optional and only needed to broker other apps.

Up Bank also supports push-based sync: register
`POST /api/sources/up_bank/webhook/<your-user-id>` (shown on the Up Bank
setup page once you're logged in) as a webhook URL at
[api.up.com.au](https://api.up.com.au), paste the `secretKey` it gives you
back into the "Webhook secret key" field, and transaction events arrive
immediately instead of waiting for the next poll. The route verifies every
delivery's `X-Up-Authenticity-Signature` against that secret before
trusting it. HeyPocket's API has no webhook support, so it stays poll-only;
the underlying webhook route/dispatch (`backend/src/routers/sources.rs`) is
generic for any future source whose provider does.

## Tests

```bash
cd backend && cargo test --release     # includes MCP protocol tests (no DB needed)
bash scripts/tests/connect-agents.test.sh && bash scripts/tests/auto-update.test.sh
cd frontend && bunx tsc --noEmit && bun run build
cd frontend && bun run test           # Playwright, starts `bun run dev`; needs backend + DB (./run.sh)
E2E_BASE_URL=http://localhost:3000 bun run test   # or against an already-running stack
```

Backend tests, the script tests, a frontend typecheck+build and the whole Playwright suite (against a
production build, the release backend and an in-memory SurrealDB v2.3, no OpenAI key) run on every
push/PR to `main` via `.github/workflows/ci.yml`. Set `E2E_UPDATE_STATUS_DIR` to the backend's
`UPDATE_STATUS_DIR` to include the Settings → HTTPS tests.

`tests/recall-eval.spec.ts` is a small retrieval evaluation: it seeds the labeled corpus in
`tests/fixtures/recall-eval.json` (a personal and a shared vault, plus another user's decoys), prints
Recall@5 and MRR, writes them to `test-results/**/recall-eval.json` (a CI artifact), fails on any
cross-vault or other-user result, and fails if the metrics drop below the floors in the spec. Compare
retrieval changes on this same corpus.

## Backups

The SurrealDB data lives in the `eunomia-surreal-data` Docker volume. Back
it up with `surreal export` (the only backup mechanism SurrealDB v2.x's CLI
offers), via the wrapper scripts below — both run inside the `surrealdb`
container, so nothing needs installing on the host, and both need the
`surrealdb` service already running (`docker compose up -d`):

```bash
backend/scripts/backup.sh                              # writes backups/eunomia-<timestamp>.surql
backend/scripts/restore.sh backups/eunomia-<timestamp>.surql
```

`restore.sh` replays the dump's `CREATE`/`DEFINE` statements against the
live database rather than wiping it first — for a guaranteed-clean restore,
restore into a fresh volume. **Never run `docker compose down -v`** to get a
clean slate; it deletes the volume (and any real data in it) outright.

## Production deployment

See [`docs/deployment.md`](docs/deployment.md) for reverse-proxy/TLS setup
(Caddy example), which env vars need real (non-`localhost`, non-default)
values in production, and how the frontend proxies `/api` to the backend
so the browser only ever needs the frontend's address.

## Local dev (without Docker)

```bash
./run.sh   # SurrealDB in Docker (localhost only) + `cargo run` + `bun run dev`
```

Open http://localhost:3000 -- the dev server proxies `/api` to the backend on
:8001, same as the production image.

## License

[MIT](LICENSE) © [Midhun Kumar](https://midhunkumar05.github.io/)

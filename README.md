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

[Install](#install) · [What's in it](#whats-in-it) · [Docs](docs/deployment.md) · [Install page](https://midhunkumar05.github.io/eunomia/)

</div>

---

## Install

```bash
curl -fsSL https://midhunkumar05.github.io/eunomia/install.sh | bash
```

Clones the latest release, generates fresh secrets, and brings up SurrealDB +
backend + frontend with Docker Compose. Or do it by hand:

```bash
git clone https://github.com/Qyrhal/Eunomia.git && cd Eunomia
cp .env.example .env    # fill in ENCRYPTION_KEY, JWT_SECRET, OPENAI_API_KEY
docker compose up --build
```

Then visit **http://localhost:3000** and register — the first account becomes
the only account unless you add more.

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

## Calling tools from another client

1. Mint a personal API token: Settings → API tokens, or
   `POST /api/auth/tokens` while logged in (shown once — store it).
2. Call `GET /api/tools` for the list of registered tools, then
   `POST /api/tools/:name` with `Authorization: Bearer <your-token>` and a
   JSON body matching that tool's schema.

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
cd backend && cargo test --release
cd frontend && bunx tsc --noEmit && bun run build
cd frontend && bun run test           # Playwright, needs the stack running
```

Backend tests and a frontend typecheck+build run on every push/PR to `main`
via `.github/workflows/ci.yml`.

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
values in production, and why changing `NEXT_PUBLIC_API_URL` requires
rebuilding the frontend image, not just restarting it.

## Local dev (without Docker)

```bash
cd backend
cargo run   # needs a local SurrealDB (see docker-compose.yml)

cd frontend
bun install
bun run dev
```

## License

[MIT](LICENSE) © [Midhun Kumar](https://midhunkumar05.github.io/)

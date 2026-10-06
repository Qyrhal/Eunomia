# Eunomia

A personal AI memory hub. Eunomia pulls your data (Up Bank, HeyPocket meeting
transcripts, anything else behind a `Source` plug-in) into one searchable
store, extracts a people/organisation/location memory graph from it, and
exposes everything identically through a web dashboard and an MCP server —
so an agent (Claude Desktop/Code) and the UI have the same read/write power.

## How it fits together

```
 sources ──sync──▶ ingest pipeline ──▶ cache (SurrealDB: rows + FTS + vector index)
 (Up Bank,                │  embed + extract entities    │
  heypocket, demo)        └───────────────────────────────┼──▶ tools: search / get / list / links
                                                            │    + per-source tools
                                                            ▼
                                     MCP server (stdio + HTTP :8765)  ◀── Claude Desktop/Code
                                     REST /api/* (:8001)  ◀── frontend (:3000)
```

- **Sources** are plug-ins under `backend/sources/`; each registers itself
  (`auth` / `sync` / `map` / `tools`).
- **Auth** is per-user: register/login issues a signed session cookie for
  the browser; a personal long-lived API token (minted in Settings or via
  `POST /api/auth/token`) authenticates MCP/agent connections instead.

## What's in it

- **Multi-user accounts** — bcrypt password hashing, JWT session cookies,
  and personal API tokens for MCP/agent connections.
- **Sources** — Up Bank (transactions/accounts), HeyPocket (meeting
  recordings/transcripts, synced incrementally every 24h), a generic Open
  Connector bridge, and a demo source for trying the app without creds.
- **One shared tool registry** — `search` / `get` / `list` / `links`,
  `recall`, `memory_write`, `entities_search` / `entities_get` /
  `entities_graph`, `consolidate_observations`, `code_entity_upsert` /
  `code_relate`, plus per-source tools — implemented once and exposed
  identically over REST and MCP. Nothing the UI can do is hidden from an
  agent, and nothing an agent can do is hidden from the UI.
- **Entity-memory graph** — people, organisations, and locations are
  auto-extracted from your synced data via OpenAI, rendered as a
  draggable/zoomable force-directed graph with kind-filter pills and
  Blobatar avatars for people.
- **Code-entity graph** — a separate but cross-linkable graph of
  repositories/files/symbols, populated not by static analysis but by an
  agent (e.g. Claude Code) calling `code_entity_upsert`/`code_relate` as it
  works in a codebase — Eunomia's own code graph was built this way, by an
  agent mapping this repository.
- **`recall()`** — a 4-arm parallel retrieval pipeline (semantic / keyword /
  graph / temporal), fused with Reciprocal Rank Fusion and boosted by
  recency and proof count.
- **Observation consolidation** — raw memory facts synthesized into
  evolving "belief" observations per entity (modeled on vectorize.io's
  Hindsight), with freshness, proof-count, and versioning.
- **`/chat`** — talk to the configured OpenAI model, with full tool access
  to recall/write/modify memory through the same shared registry.
- **`/connectors`** — a marketplace-style grid (My Connectors / Discover
  tabs, search/filter, per-connector setup pages with sync-interval
  controls).
- **The dashboard** — sync health for every connected source, total
  records, entities tracked, and what's still not connected.

## Run it

```bash
cp .env.example .env    # fill in ENCRYPTION_KEY, JWT_SECRET, OPENAI_API_KEY
docker compose up --build
```

This brings up four services: `surrealdb` (`:8000`), `backend` (`:8001`),
`frontend` (`:3000`), and `mcp` (`:8765`).

1. Visit **http://localhost:3000** and register an account (the first
   registered user becomes the only user, unless you add more).
2. Follow the onboarding wizard: it asks for an OpenAI API key if one
   isn't already configured server-wide (skipped entirely if it is).
3. Connect a source from the **Connectors** page (Up Bank / HeyPocket /
   Open Connector, or just use the seeded demo source) — sync status and
   the entity graph fill in once a source has synced.

## Connecting an MCP client

1. Mint a personal API token: Settings → "Generate API token", or
   `POST /api/auth/token` while logged in (shown once — store it).
2. Point your client at the `mcp` service. For Claude Desktop/Code, add to
   its MCP config:
   ```json
   {
     "mcpServers": {
       "eunomia": {
         "url": "http://localhost:8765/mcp",
         "headers": { "Authorization": "Bearer <your-token>" }
       }
     }
   }
   ```
   Or run it directly over stdio instead of the `mcp` container:
   ```bash
   cd backend && EUNOMIA_API_TOKEN=<your-token> uv run mcp_server.py
   ```

## Connectors

Up Bank: a personal access token from api.up.com.au. HeyPocket: an API key.
Both are entered on the Connectors page and stored encrypted at rest; Open
Connector is optional and only needed to broker other apps.

Up Bank also supports push-based sync: register `POST /api/sources/up_bank/webhook/<your-user-id>`
(shown on the Up Bank setup page once you're logged in) as a webhook URL at
[api.up.com.au](https://api.up.com.au), paste the `secretKey` it gives you
back into the "Webhook secret key" field, and transaction events arrive
immediately instead of waiting for the next poll. The route verifies every
delivery's `X-Up-Authenticity-Signature` against that secret before
trusting it. HeyPocket's API has no webhook support, so it stays poll-only;
the underlying webhook route/dispatch is generic (`Source.webhook()`,
`backend/app/routers/sources.py`) for any future source whose provider does.

## Tests

```bash
cd backend && uv run pytest tests -q
cd frontend && bunx tsc --noEmit && bun run build
cd frontend && bun run test           # Playwright, needs the stack running

# Live smoke test against a running docker-compose stack:
cd backend && uv run python scripts/smoke_live.py --base http://localhost:8001
```

Backend tests and a frontend typecheck+build run on every push/PR to `main`
via `.github/workflows/ci.yml`.

## Backups

The SurrealDB data lives in the `eunomia-surreal-data` Docker volume. Back
it up with `surreal export` (the only backup mechanism SurrealDB v2.3's CLI
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
values in production, and why changing `NEXT_PUBLIC_API_URL`/
`NEXT_PUBLIC_MCP_URL` requires rebuilding the frontend image, not just
restarting it.

## Local dev (without Docker)

```bash
cd backend
uv sync
uv run uvicorn app.main:app --reload --port 8001   # needs a local SurrealDB (see docker-compose.yml)

cd frontend
bun install
bun run dev
```

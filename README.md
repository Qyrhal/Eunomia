# Eunomia

A personal data hub with real multi-user accounts. Eunomia pulls your Up
Bank and heypocket data (and anything else behind a `Source` plug-in) into
one searchable store, extracts a people/organisation/location memory graph
from it, and exposes everything identically through a web dashboard and an
MCP server — so an agent (Claude Desktop/Code) and the UI have the same
read/write power.

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

## Run it

```bash
cp .env.example .env    # fill in ENCRYPTION_KEY, JWT_SECRET, OPENAI_API_KEY
docker compose up --build
```

This brings up four services: `surrealdb` (`:8000`), `backend` (`:8001`),
`frontend` (`:3000`), and `mcp` (`:8765`).

1. Visit **http://localhost:3000** and register an account (the first
   registered user becomes the only user, unless you add more).
2. Follow the onboarding wizard: confirm your password, connect one data
   source (Up Bank / heypocket / Open Connector), and set an OpenAI API key
   if one isn't already configured server-wide.
3. The dashboard shows sync health for connected sources and an entity
   network graph (people/organisations/locations) extracted from your data.

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

Up Bank: a personal access token from api.up.com.au. heypocket: an API key.
Both are entered on the Connectors page and stored encrypted at rest; Open
Connector is optional and only needed to broker other apps.

## Tests

```bash
cd backend && uv run pytest tests -q
cd frontend && bun run test           # Playwright, needs the stack running

# Live smoke test against a running docker-compose stack:
cd backend && uv run python scripts/smoke_live.py --base http://localhost:8001
```

## Local dev (without Docker)

```bash
cd backend
uv sync
uv run uvicorn app.main:app --reload --port 8001   # needs a local SurrealDB (see docker-compose.yml)

cd frontend
bun install
bun run dev
```

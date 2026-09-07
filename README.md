# Eunomia

A **collection of tools exposed over MCP**, for [Hermes](https://github.com/nousresearch/hermes-agent)
or Claude Code. Eunomia pulls your Up Bank and heypocket data
into one local store, indexes everything for keyword + semantic search, and
exposes it — plus task management and any app reachable through Nango,
Composio or Open Connector — to Hermes/Claude over MCP.

There is no dashboard. Hermes/Claude is the interface. A small admin frontend
exists for inspecting the cache, sources and connectors.

## How it fits together

```
 sources ──sync──▶ ingest pipeline ──▶ cache (sqlite: rows + FTS5 + sqlite-vec)
 (Up Bank,                │  embed                  │
  heypocket, demo)        └─────────────────────────┼──▶ tools: search / get / list / links
                                                      │    + per-source + task + managed-connector
                                                      ▼
                                     MCP server (stdio + HTTP)  ◀── Hermes / Claude Code
                                     REST /api/tools ───────────────────────┘
```

- **Sources** are plug-ins: drop a folder in `backend/sources/`, it registers
  itself (`auth` / `sync` / `map` / `tools` / `webhook`).
- **Managed connectors** — Nango, Composio and Open Connector are broker
  platforms that hold OAuth to many third-party apps; connect one and its
  `managed_connector_call` / `managed_connector_list_connections` tools let
  Hermes/Claude drive any app that broker supports.

## Layout

- `backend/` — Django + DRF, uv-managed. Apps: `sources`, `cache`,
  `embeddings`, `tools`, plus `tasks` / `connectors` / `analytics`.
  `mcp_server.py` is the MCP entrypoint.
- `frontend/` — Next.js admin UI, bun-managed.

## Run it (dev)

```bash
./run.sh
```

Starts, together: the API (`:8000`), the sync **worker**, the **MCP server**
(`:8765/mcp`), and the frontend (`:3000`). Stop with ctrl-c.

By hand:

```bash
cd backend
uv sync
python -c "from cryptography.fernet import Fernet; print(Fernet.generate_key().decode())"  # -> ENCRYPTION_KEY
cat > .env <<EOF
SECRET_KEY=dev-secret
ENCRYPTION_KEY=<paste>
# EUNOMIA_API_TOKEN=<long random string>   # leave unset for open access on a trusted LAN
EOF
uv run manage.py migrate
uv run manage.py runserver 8000        # API
uv run manage.py run_worker            # sync (separate process)
uv run mcp_server.py --http            # MCP over HTTP on 127.0.0.1:8765
```

Local semantic search uses an embedding backend chosen in Settings:
`api` (any OpenAI-compatible `/embeddings`, incl. Ollama's — no extra deps, the
default), `local` (`uv sync --extra local-embeddings` for `bge-small`), or
`stub`.

## Connecting Hermes

Eunomia and Hermes are meant to run on the **same host**, reachable over
tailscale / netbird — **never public**.

1. **Set `EUNOMIA_API_TOKEN`** and bind services to the tailscale/netbird
   interface, not `0.0.0.0`.
2. **Point Hermes at the MCP server** — in `~/.hermes/config.yaml`:
   ```yaml
   mcp_servers:
     eunomia:
       url: http://127.0.0.1:8765/mcp
       headers: { Authorization: "Bearer ${EUNOMIA_API_TOKEN}" }
   ```
   A webhook-triggered Hermes run gets a constrained toolset by default — add
   `toolsets: [...]` to the route so Hermes may call Eunomia's tools in reply.

## Connectors

Up Bank: a personal access token from api.up.com.au. heypocket: an API key.
Both are entered on the Connectors page and stored encrypted at rest; Open
Connector is optional and only needed if you want to broker other apps.

## Demo data

```bash
cd backend
uv run manage.py seed_demo            # bank + recordings + tasks, synced into the cache
uv run manage.py seed_demo --clear
```

## Tests

```bash
cd backend && uv run manage.py test
cd frontend && bun run test           # needs the backend on :8000
```

## Docker

```bash
cp .env.example .env    # fill SECRET_KEY, ENCRYPTION_KEY, EUNOMIA_API_TOKEN
docker compose up --build
```

Brings up `backend`, `worker`, `mcp` (`:8765`) and `frontend`.

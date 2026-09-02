# Eunomia

An **agentic data layer for [Hermes](https://github.com/nousresearch/hermes-agent)**.
Eunomia pulls your Google Workspace, Up Bank and heypocket data into one local
store, **masks every secret and matched PII behind reversible tokens**, indexes
everything for keyword + semantic search, and exposes it to Hermes over MCP.
Hermes can also register watches that fire a webhook back to it.

There is no dashboard. Hermes is the interface. A small admin frontend exists for
inspecting the cache, the vault, sources and triggers.

## How it fits together

```
 sources ──sync──▶ ingest pipeline ──▶ cache (sqlite: rows + FTS5 + sqlite-vec)
 (Google, Up Bank,   │  mask secrets          │
  heypocket, demo)   │  detect + tokenise PII  ├──▶ tools:  search / get / list / links
                     │  embed                  │            + per-source + task + trigger
                     └─ evaluate rules ─┐      │
                                        ▼      ▼
 Hermes ◀── webhook (HMAC) ── triggers   MCP server (stdio + HTTP)  ◀── Hermes
 Hermes ──────────────────── REST /api/tools ──────────────────────────┘
```

- **Masking** — a declared credential or a detected email / phone / card / bank
  number / API-key becomes `[eunomia:<type>:<n>]`. The real value lives in a
  Fernet-encrypted vault; it is re-inserted only when Eunomia itself calls an
  external API, and revealed to a human only through one audited endpoint. Every
  crossing is logged (token strings + who, never the value).
- **Sources** are plug-ins: drop a folder in `backend/sources/`, it registers
  itself (`auth` / `sync` / `map` / `tools` / `webhook`).
- **Triggers** — record rules, time-relative schedules, and crons; each fires an
  HMAC-signed POST to the Hermes gateway webhook adapter.

## Layout

- `backend/` — Django + DRF, uv-managed. Apps: `sources`, `cache`, `masking`,
  `embeddings`, `triggers`, `tools`, plus `tasks` / `connectors` / `analytics`.
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
uv run manage.py run_worker            # sync + triggers (separate process)
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
3. **Notifications** — set the Hermes gateway webhook base URL and shared secret
   in Eunomia's Settings (`hermes_webhook_url`, `hermes_webhook_secret`). Enable
   or add triggers; each POSTs `X-Webhook-Signature-V2` (HMAC-SHA256 of
   `<ts>.<body>`) to that route.

Google Calendar / Drive **push** needs a public CA-trusted HTTPS callback, which
a tailscale-only box doesn't have — Eunomia polls those instead (Gmail can use a
Pub/Sub pull). See `docs/research/google-workspace-push.md`.

## Connectors

Google OAuth needs a Google Cloud OAuth client (Calendar + Gmail + Drive APIs,
redirect URI `http://localhost:8000/api/connectors/google/callback`). Paste the
client id/secret into the Google card on the Connectors page, or set
`GOOGLE_OAUTH_CLIENT_ID` / `GOOGLE_OAUTH_CLIENT_SECRET` in `backend/.env`.
Up Bank: a personal access token. heypocket: an API key.

## Demo data

```bash
cd backend
uv run manage.py seed_demo            # bank + calendar + email + recordings + tasks, synced into the cache
uv run manage.py seed_demo --clear
```

## Tests

```bash
cd backend && uv run manage.py test
cd frontend && bun run test           # needs the backend on :8000
```

## Docker

```bash
cp .env.example .env    # fill SECRET_KEY, ENCRYPTION_KEY, EUNOMIA_API_TOKEN, Google creds
docker compose up --build
```

Brings up `backend`, `worker`, `mcp` (`:8765`) and `frontend`.

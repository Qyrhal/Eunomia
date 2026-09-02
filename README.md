# Eunomia

Personal AI dashboard: tasks/reminders, an AI assistant (bring your own OpenAI-compatible
endpoint) with tool-calling over your tasks/calendar/bank data, and connectors for Google
(Calendar + Gmail), [Up Bank](https://developer.up.com.au), and
[PocketAI](https://docs.heypocketai.com/docs/api).

On the Tasks page, hitting "Generate" next to a task title asks the model to draft notes —
it can call read-only tools (calendar, email search, recent transactions) to pull in real,
specific context (e.g. a "Picnic with Sam" task can end up referencing the actual email
thread or the venue booking transaction) rather than inventing detail. That generation path
never touches your data — it can look things up but can't create or edit tasks itself.

No auth — this is meant to run on your own machine/network, not be exposed publicly.

## Structure

- `backend/` — Django + DRF API, uv-managed. Also ships `mcp_server.py`, a standalone
  MCP server (stdio) exposing the same task/calendar tools to Claude or other MCP clients.
- `frontend/` — Next.js (App Router) + Tailwind, bun-managed.

## Quick start

```bash
./run.sh
```

Migrates the backend and runs both servers together. See "Local dev" below to do it by hand.

## Local dev

```bash
# backend
cd backend
uv sync
python -c "from cryptography.fernet import Fernet; print(Fernet.generate_key().decode())"  # -> ENCRYPTION_KEY
cat > .env <<EOF
SECRET_KEY=dev-secret
ENCRYPTION_KEY=<paste generated key>
EOF
uv run manage.py migrate
uv run manage.py runserver 8000

# frontend, in another shell
cd frontend
bun install
echo "NEXT_PUBLIC_API_URL=http://localhost:8000" > .env.local
bun run dev
```

Open http://localhost:3000, go to Settings for the AI endpoint/theme, and Connectors for
Google/Up Bank/PocketAI.

Google OAuth needs a Google Cloud OAuth client (Calendar + Gmail APIs enabled, redirect URI
`http://localhost:8000/api/connectors/google/callback`) — paste its client ID/secret into
the Google card on the Connectors page, no `.env` editing needed. (They can also be set via
`GOOGLE_OAUTH_CLIENT_ID` / `GOOGLE_OAUTH_CLIENT_SECRET` in `backend/.env` as a deployment-wide
fallback if you'd rather not store them per-instance.)

## Demo data

Nothing to look at yet? Seed ~50 realistic fake tasks across 4 demo projects (all prefixed
"Demo — ", so they're easy to tell apart and clear):

```bash
cd backend
uv run manage.py seed_demo_data          # add --seed N for reproducible data
uv run manage.py seed_demo_data --clear  # remove it again
```

Or from the app: Settings → Demo data → Seed/Clear. Re-seeding replaces the previous batch;
clearing only ever touches those prefixed projects, never real data.

## MCP server

```bash
cd backend
uv run mcp_server.py
```

Point Claude Desktop (or another MCP client) at that command — it reads/writes the same
sqlite database as the Django app.

## Tests

```bash
cd backend
uv run manage.py test

# frontend UI tests (needs the backend running on :8000 — real API, no mocks)
cd frontend
bun run test
```

## Docker

```bash
cp .env.example .env   # fill in SECRET_KEY, ENCRYPTION_KEY, Google OAuth creds
docker compose up --build
```

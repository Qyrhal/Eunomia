# Installing Eunomia (for an agent doing the install)

You're an agent installing Eunomia onto the box you're running on. Execute
this top to bottom. Where it says "ask the operator," stop and ask.

Keep it private: reachable over localhost or tailscale/netbird only. Never
bind anything to a public interface.

## Step 1 — install

Requires git and Docker with the Compose plugin.

```bash
curl -fsSL https://midhunkumar05.github.io/eunomia/install.sh | bash -s -- --yes
```

This clones the latest release into `./eunomia`, writes `.env` with fresh
secrets (JWT, encryption key, DB password), pulls the prebuilt images, and
starts SurrealDB + backend + frontend. Useful flags:

| Flag | Default |
|---|---|
| `--dir <path>` | `./eunomia` |
| `--ref <tag>` | latest release |
| `--frontend-port <port>` | `3000` |
| `--backend-port <port>` | `8001` |
| `--api-key <key>` | none — ask the operator; can be added later in Settings |
| `--openai-base-url <url>` | `https://api.openai.com/v1` (any OpenAI-compatible endpoint) |

If a port is taken the installer stops before changing anything — ask the
operator which port to use.

Manual alternative (same result):

```bash
git clone https://github.com/Qyrhal/Eunomia.git && cd Eunomia
cp .env.example .env   # then set JWT_SECRET, ENCRYPTION_KEY, SURREAL_PASS (openssl rand -base64 32)
docker compose pull && docker compose up -d
```

## Step 2 — verify

```bash
curl -fsS http://localhost:8001/healthz
docker compose ps        # from the install dir; all three services "Up"
```

The UI is at `http://localhost:3000` (or `http://<host-ip>:3000` from other
devices — the browser only talks to the frontend, which proxies `/api` and
`/mcp`).

## Step 3 — connect yourself over MCP

Ask the operator to register at the UI and generate a token (Dashboard →
"Connect an MCP client" → Generate a token). Eunomia is an MCP server
(Streamable HTTP) at `http://localhost:3000/mcp`; register it with that
token as a bearer header, e.g. for Claude Code:

```bash
claude mcp add --transport http eunomia http://localhost:3000/mcp \
  --header "Authorization: Bearer $EUNOMIA_API_TOKEN"
```

Check it with a `tools/list` — you should see 25 tools (`recall`,
`memory_write`, `search`, vault and code-graph tools). `recall` works without
an OpenAI key (keyword/graph/recency only); `reflect` and semantic search
need one.

## Step 4 — hand back to the operator

Tell them where the UI is, that connectors (Up Bank, PocketAI, …) are empty
until they add credentials on the Connectors page, and that chat/extraction
need an OpenAI key in Settings if one wasn't passed at install.

Production (TLS, reverse proxy, which env vars need real values):
[`docs/deployment.md`](docs/deployment.md).

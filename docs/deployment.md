# Production deployment

`docker compose up` (see the main [README](../README.md)) is fine for local
dev, where every service talks to `localhost` over plain HTTP. Putting this
in front of real users needs three more things: TLS, env vars that aren't
`localhost`/default secrets, and knowing that the frontend's `NEXT_PUBLIC_*`
vars are baked in at *build* time, not read at runtime.

## 1. Reverse proxy + TLS

Put something in front of the `frontend` (`:3000`) and `backend` (`:8001`)
containers that terminates TLS and proxies to them. [Caddy](https://caddyserver.com/)
gets automatic HTTPS (it provisions and renews a Let's Encrypt cert itself)
in a handful of lines, which is why it's recommended here over hand-rolling
nginx + certbot:

```caddyfile
# Caddyfile
eunomia.example.com {
    reverse_proxy /api/* localhost:8001
    reverse_proxy /mcp localhost:8001
    reverse_proxy /healthz localhost:8001
    reverse_proxy /* localhost:3000
}
```

Run it alongside the compose stack (`caddy run` on the host, or as one more
service in `docker-compose.yml` with ports 80/443 published). An nginx
equivalent needs its own `server { listen 443 ssl; ... }` block plus a
certbot container/cron to issue and renew the cert — more moving parts for
the same result.

## 2. Env vars that need real values

`.env.example` documents every variable; these specifically must NOT keep
their local-dev defaults in production:

| Var | Local default | Production |
|---|---|---|
| `JWT_SECRET` | random per-process if unset | a fixed secret (`openssl rand -base64 32`) — sessions won't survive a restart otherwise, and every process must agree on one value |
| `ENCRYPTION_KEY` | a static fallback key | `openssl rand -base64 32` — the fallback is non-secret and ships in this repo |
| `SURREAL_PASS` | `root` | a real password |
| `OPENAI_API_KEY` | blank (settable per-user instead) | set it server-wide, or rely on each user setting their own in Settings |
| `CORS_ALLOWED_ORIGINS` | `http://localhost:3000` | only needed for browsers calling the backend port directly; the bundled frontend goes through its own `/api` proxy |
| `FRONTEND_URL` | `http://localhost:3000` | same as above (this is what `CORS_ALLOWED_ORIGINS` defaults from in `docker-compose.yml`) |

## 3. The browser only ever talks to the frontend

The frontend calls same-origin `/api/*`, and `frontend/next.config.ts`
rewrites that to `BACKEND_INTERNAL_URL` (`http://backend:8001` in the
image, the compose service name). The browser never needs the backend's
host, so one prebuilt image works whether you open it as `localhost`, a LAN
IP, or a domain -- no rebuild, no CORS. Put your reverse proxy / TLS in
front of the frontend port only.

`NEXT_PUBLIC_API_URL` still exists as a build-time override for running the
frontend somewhere it can't reach the backend over a private network; it is
inlined at `bun run build`, so changing it requires rebuilding the image.

## Auto-update

Settings → Updates shows whether `main` on GitHub has moved past what's
deployed, and an "Update now" button. It's opt-in and off by default (no
cron job, no effect) because the actual `git pull` + rebuild runs on the
**host**, not in a container: this backend never gets a docker socket or a
git credential, since a web-facing process (webhooks, agent tool calls)
holding either would turn any RCE in it into a host compromise.

To enable it, run `scripts/auto-update.sh` on the host periodically --
cron:

```cron
* * * * * cd /path/to/eunomia && ./scripts/auto-update.sh >> update.log 2>&1
```

or a systemd timer hitting the same script every minute. It writes
`update-status/status.json` (polled by the UI) every run, and only
`git pull --ff-only` + `docker compose up -d --build backend frontend`
when `update-status/requested` exists -- i.e. only after someone clicks
"Update now" in the UI, never on its own. `surrealdb` is deliberately never
rebuilt by it (no application code to update there) and the script never
touches itself.

Expect up to ~1 minute of latency between clicking "Update now" and the
restart landing, and a few seconds of downtime on `backend`/`frontend`
while they rebuild -- plan for a quiet window, same as any manual
`docker compose up -d --build`.

## Backups

See the [README's Backups section](../README.md#backups).

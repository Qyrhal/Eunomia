# Production deployment

`docker compose up` (see the main [README](../README.md)) is fine for local
dev, where every service talks to `localhost` over plain HTTP. Putting this
in front of real users needs three more things: TLS, env vars that aren't
`localhost`/default secrets, and knowing that the frontend's `NEXT_PUBLIC_*`
vars are baked in at *build* time, not read at runtime.

## 1. HTTPS (Let's Encrypt)

The stack ships an optional `caddy` service that serves Eunomia at your own
domain with a free Let's Encrypt certificate and renews it on its own.

You need:

- a domain (or subdomain) whose DNS A/AAAA record points at this machine;
- ports 80 and 443 reachable from the internet and not used by anything else
  on the machine (Let's Encrypt checks port 80 when issuing).

Turn it on in any of three ways:

- **Settings → HTTPS**: enter the domain and an email for Let's Encrypt,
  click **Enable HTTPS**. The status reads *Pending* until the certificate is
  issued (usually under a minute), then *Active*. Problems (a port already in
  use, DNS not pointing here yet) show up there. **Disable** stops it.
- **The installer**: `--domain eunomia.example.com --acme-email you@example.com`
  (or answer yes to the HTTPS question). Agents are then connected to
  `https://eunomia.example.com/mcp`.
- **By hand**: add to `.env`, then `docker compose up -d`:

  ```bash
  COMPOSE_PROFILES=https
  EUNOMIA_DOMAIN=eunomia.example.com
  EUNOMIA_ACME_EMAIL=you@example.com
  ```

`./Caddyfile` proxies everything to the frontend, which forwards `/api` and
`/mcp` to the backend, so the web app, the API and MCP all live at
`https://<domain>`. Certificates are kept in the `eunomia-caddy-data`
volume. Plain HTTP on port 80 redirects to HTTPS. The old addresses
(`:3000`, `:8001`) keep working; to make the domain the only way in, firewall
those ports or remove their `ports:` entries.

How Settings applies it: like updates (below), the backend never touches
docker or `.env`. It validates the domain and email and writes
`update-status/https.json`. Within 20 seconds the `updater` service validates
them again, sets the three `.env` values, starts (or removes) `caddy`, and
reports in `update-status/https-status.json`, probing the certificate until
it answers. Watch it with `docker compose logs caddy`.

Using your own reverse proxy instead (nginx, Traefik, an existing Caddy):
point it at the frontend port only, as in section 3. Leave HTTPS off here
so the two don't both want ports 80/443.

## 2. Env vars that need real values

`.env.example` documents every variable; these specifically must NOT keep
their local-dev defaults in production:

| Var | Local default | Production |
|---|---|---|
| `JWT_SECRET` | random per-process if unset | a fixed secret (`openssl rand -base64 32`) — sessions won't survive a restart otherwise, and every process must agree on one value |
| `ENCRYPTION_KEY` | none: the backend refuses to start without one (min. 16 characters) | `openssl rand -base64 32` (the installer generates one) |
| `SURREAL_PASS` | `root` | a real password |
| `OPENAI_API_KEY` | blank (settable per-user instead) | set it server-wide, or rely on each user setting their own in Settings. It is only ever sent to `OPENAI_BASE_URL`; a user who points Settings at another endpoint uses their own key there |
| `CORS_ALLOWED_ORIGINS` | `http://localhost:3000` | only needed for browsers calling the backend port directly; the bundled frontend goes through its own `/api` proxy |
| `FRONTEND_URL` | `http://localhost:3000` | same as above (this is what `CORS_ALLOWED_ORIGINS` defaults from in `docker-compose.yml`) |

Saved credentials are stored as `enc:v1:…` ciphertext; older rows without
the prefix still decrypt. If a value can't be decrypted (a changed
`ENCRYPTION_KEY`, corrupted data, or a deployment that used to run without
a key), it is never sent anywhere: the feature reports an error and the
credential has to be entered again in Settings or Connectors. Keep
`ENCRYPTION_KEY` with your backups.

## 3. The browser only ever talks to the frontend

The frontend calls same-origin `/api/*`, and `frontend/next.config.ts`
rewrites that to `BACKEND_INTERNAL_URL` (`http://backend:8001` in the
image, the compose service name). The browser never needs the backend's
host, so one prebuilt image works whether you open it as `localhost`, a LAN
IP, or a domain -- no rebuild, no CORS. The built-in HTTPS (section 1), or
your own reverse proxy, goes in front of the frontend port only.

`NEXT_PUBLIC_API_URL` still exists as a build-time override for running the
frontend somewhere it can't reach the backend over a private network; it is
inlined at `bun run build`, so changing it requires rebuilding the image.

## Updates

Settings → Updates shows the running release and, when a newer one is
published, **Update now**. A sidebar badge appears when an update is waiting.
**Check now** asks GitHub immediately instead of waiting for the 10-minute
check. After you click, the page shows progress, Eunomia restarts on the new
release's prebuilt images (a few seconds of downtime, no build), and the page
reloads itself. Data lives in the `eunomia-surreal-data` volume and isn't
touched.

How: the stack includes a small `updater` service (`scripts/updater.sh`) that
runs `scripts/auto-update.sh` every 20 seconds. It works the same on every
install and OS, with nothing to schedule on the host. The web-facing backend
never gets git or the docker socket: it only drops a marker file in
`update-status/`. The updater has the socket but no ports, and its only input
is that marker. The worst a compromised backend can do is ask for the newest
release tag. The script checks out the tag, pins `EUNOMIA_IMAGE_TAG` in `.env`,
pulls and restarts, and appends to `update-status/history.log`. It refuses
(and says so in Settings) if tracked files like `docker-compose.yml` have
local edits. A lock in `update-status/` keeps an old host cron job and the
container from ever running at once.

Installs from before the updater existed: run the installer once more from
the folder that contains your install. It updates in place and keeps your
data.

## Backups

See the [README's Backups section](../README.md#backups).

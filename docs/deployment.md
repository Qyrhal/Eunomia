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
| `ENCRYPTION_KEY` | none: the backend refuses to boot without one | `openssl rand -base64 32`. It encrypts stored connector credentials and OpenAI keys, protects the per-org database passwords and derives the control database password. Keep a copy off the machine. `EUNOMIA_ALLOW_EMPTY_ENCRYPTION_KEY=1` skips the check for local development only (an empty key is a public, all-zero AES key). An install that already holds data written under an empty key still boots with a loud error in the log; see "Rotating ENCRYPTION_KEY" below. |
| `SURREAL_PASS` | `root` | a real password |
| `OPENAI_API_KEY` | blank (settable per-user instead) | set it server-wide, or rely on each user setting their own in Settings |
| `CORS_ALLOWED_ORIGINS` | `http://localhost:3000` | only needed for browsers calling the backend port directly; the bundled frontend goes through its own `/api` proxy |
| `FRONTEND_URL` | `http://localhost:3000` | same as above (this is what `CORS_ALLOWED_ORIGINS` defaults from in `docker-compose.yml`) |
| `SESSION_TTL_DAYS` | `30` | Browser sessions expire this many days after last use (extended at most once an hour). The session cookie itself also carries a hard 90 day limit. |
| `RATE_LIMIT_USER_PER_MIN` | `1200` | Requests per minute per signed-in user. `0` turns the limit off. Over the limit the API answers 429 `rate.limited` with `Retry-After`. |
| `RATE_LIMIT_TOKEN_PER_MIN` | `600` | Requests per minute per API token (a user's limit applies as well). |
| `RATE_LIMIT_AUTH_PER_MIN` | `20` | Login, signup, OAuth token and failed-credential attempts per minute per client address (read from `X-Forwarded-For`, but only when the connecting peer is in `TRUSTED_PROXIES`). |
| `RATE_LIMIT_WEBHOOK_PER_MIN` | `120` | Source webhook deliveries per minute per client address. |
| `MAX_REQUEST_BODY_BYTES` | `1048576` | Largest request body any route accepts (413 over it). Raise it only if a route you use needs more. |
| `TRUSTED_PROXIES` | loopback only (`127.0.0.0/8`, `::1/128`); `docker-compose.yml` sets the frontend's pinned address `172.30.77.10` | Comma-separated CIDRs of proxies whose `X-Forwarded-For` is believed; the right-most address that is not itself trusted is the client. A LAN or bridge peer that is not listed cannot spoof its address. Compose gives the network a fixed subnet (`172.30.77.0/24`) and the frontend the address `172.30.77.10`; change both if that range clashes with yours. Putting your own reverse proxy in front of the backend: add its address (`TRUSTED_PROXIES=172.30.77.10,203.0.113.5`) or a range. `none` trusts nobody (every client is its peer address). |
| `PUBLIC_URL` | `http://localhost:8001` | The address MCP clients reach Eunomia at, no trailing slash, e.g. `https://eunomia.example.com`. Used in OAuth discovery documents and as the audience of OAuth tokens, so it must match what clients connect to. |
| `EUNOMIA_SIGNUP_ORG` | `join` | `join`: a new user joins the install's one org (the first user of a fresh install creates it and owns it). `personal`: every signup gets an org of their own. |
| `EUNOMIA_ORG_POOL_CAP` | `256` | How many org database sessions one process keeps open (least recently used are dropped and signed in again on demand). |
| `BACKUP_ENCRYPTION_KEY` | none (the `backup` service generates one, see below) | Encrypts nightly backups. Recommended: generate it before the first `docker compose up` (`openssl rand -base64 32`), put it in `.env`, and keep a copy off the machine, since backups cannot be read without it. The auto-updater adds one when it updates an existing install. If it is still empty, the `backup` service creates a key on first start, saves it to `/backups/.backup-key` in its own volume and logs a one-time "COPY IT SOMEWHERE SAFE" notice. Tradeoff: that key lives in the same volume as the backups, so someone who copies the volume gets both and the encryption protects nothing against them; losing the volume loses the key too. It only guards against someone who sees the backup files without the volume's key file. Setting the key in `.env` keeps the two apart. |

Backup files are encrypted with AES-256-CBC and authenticated with an HMAC-SHA256 over the ciphertext, checked before anything is decrypted or imported, so a modified file is refused. Backups written by earlier versions (no authentication) still restore.

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

## Org databases and the one-time move

Each org's data lives in its own database, `org_<uuid>`, in the `SURREAL_NS` namespace; accounts, credentials and the job queue live in the `control` database. See [architecture/tenancy.md](architecture/tenancy.md). The first boot of a release with tenancy on an existing install moves the old single database (`SURREAL_DB`, default `eunomia`) into that layout automatically, copying every table in batches and refusing to finish unless the row counts match. Nothing in the old database is changed or deleted; the backend logs the `REMOVE DATABASE` command to use once you have checked the app. If the boot stops with "the data move did not verify", nothing is served from the half-moved org; fix the cause and restart (the move resumes). Take an export first, as the upgrade scripts do.

`ENCRYPTION_KEY` now also protects the per-org database passwords: keep it.

### Rotating ENCRYPTION_KEY

Older installs ran with an empty `ENCRYPTION_KEY`, so their stored connector credentials, OpenAI keys and org database passwords were encrypted under a public all-zero key. The backend does not refuse to boot over that data (it would lock you out of it); it logs an error on every start until you move to a real key:

1. Set `ENCRYPTION_KEY` (`openssl rand -base64 32`) **and** `ENCRYPTION_KEY_LEGACY_EMPTY=1` in `.env`, then restart the backend. "Update now" does both for you when the key is blank. With the flag on, a value that does not decrypt under the new key is tried under the old empty one, and the log notes each time that happens. Every new write uses the new key.
2. Org database passwords are re-encrypted under the new key automatically at that boot. Connector credentials and OpenAI keys are re-encrypted when you save them again: reconnect each connector (or re-enter its credentials) and re-enter the OpenAI key in Settings.
3. When the log no longer shows the "decrypted a value written with the empty ENCRYPTION_KEY" line over a few days of normal use, remove `ENCRYPTION_KEY_LEGACY_EMPTY` from `.env`.

Changing a real key later has no such fallback: values written under the old key stop decrypting, so reconnect the connectors, re-enter the OpenAI key, and expect org databases to be unreachable until their users are redefined (see [architecture/tenancy.md](architecture/tenancy.md)). Do not change it casually.

## Backups

See the [README's Backups section](../README.md#backups).

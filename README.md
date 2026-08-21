# Eunomia

Personal work/uni/business dashboard. Reads an Obsidian vault off disk, buckets notes
by frontmatter tag / folder / inline hashtag, serves a dashboard. See [ABOUT.md](ABOUT.md)
for what it's for and why.

## Install

Requires [uv](https://docs.astral.sh/uv/) (manages the Python 3.14 install and venv for you).

```
git clone https://github.com/Qyrhal/Eunomia.git
cd Eunomia
uv sync --extra dev
```

## Configure

Environment variables:

| Variable                        | Required for                | Meaning                                                    |
|----------------------------------|------------------------------|--------------------------------------------------------------|
| `EUNOMIA_VAULT_PATH`             | dashboard (default `./vault`) | Fallback vault path — the Settings page's "Vault" block overrides this once set |
| `EUNOMIA_DB_PATH`                | dashboard (default `eunomia.db`) | Path to the SQLite file Eunomia writes to                |
| `EUNOMIA_MASTER_KEY`             | Settings page                | Encrypts stored credentials at rest. Generate one:<br>`uv run python -c "from cryptography.fernet import Fernet; print(Fernet.generate_key().decode())"` |
| `EUNOMIA_GITHUB_CLIENT_ID` / `_SECRET` | GitHub sign-in         | From a GitHub OAuth App: https://github.com/settings/developers |
| `EUNOMIA_LINEAR_CLIENT_ID` / `_SECRET` | Linear sign-in         | From a Linear OAuth application: https://linear.app/settings/api/applications |
| `EUNOMIA_SLACK_CLIENT_ID` / `_SECRET`  | Slack sign-in          | From a Slack app: https://api.slack.com/apps               |

The vault path just needs to be a folder of `.md` files kept in sync by whatever you
already use (Obsidian Sync, Syncthing, iCloud) — Eunomia only reads from disk, it
doesn't talk to Obsidian directly.

The OAuth client id/secret pairs are only needed if you want the "Sign in" buttons
on the Settings page to work for that service — set the redirect/callback URL on
each platform's app to `http(s)://<your-host>/oauth/<service>/callback`. Without
them, that connection's Settings row still works via a pasted API key instead.

## Run locally

```
EUNOMIA_VAULT_PATH=/path/to/vault EUNOMIA_MASTER_KEY=<generated key> uv run uvicorn eunomia.api:app --reload
```

- `GET /` — dashboard; "+ New note" is disabled until a vault is set in Settings
- `POST /notes` — create a note (`title`, `bucket`) directly in the vault; 400s if no vault is configured yet
- `POST /sync` — rescan the vault and refresh the DB
- `GET /api/notes?bucket=work` — JSON, `bucket` optional
- `GET /settings` — set the vault path, manage buckets (add your own beyond Uni/Work/Business/Other), connect accounts (sign-in or API key), and manage LLM provider keys
- `GET /health` — liveness check

## Deploy (self-hosted, always-on)

Runs as a single process — no containers or reverse proxy required, though you can
put one in front for TLS if you expose it beyond localhost.

1. On the server: install [uv](https://docs.astral.sh/uv/getting-started/installation/),
   clone the repo into `/opt/eunomia` (or wherever), then `cd /opt/eunomia && uv sync --extra dev`.
   uv downloads and pins Python 3.14 itself — no system Python version to manage.
2. Create a systemd unit at `/etc/systemd/system/eunomia.service`:

   ```ini
   [Unit]
   Description=Eunomia dashboard
   After=network.target

   [Service]
   User=YOUR_USER
   WorkingDirectory=/opt/eunomia
   Environment=EUNOMIA_VAULT_PATH=/path/to/vault
   Environment=EUNOMIA_DB_PATH=/opt/eunomia/eunomia.db
   Environment=EUNOMIA_MASTER_KEY=<generated key, keep it secret>
   # Optional, only needed for the Settings page's "Sign in" buttons:
   # Environment=EUNOMIA_GITHUB_CLIENT_ID=...
   # Environment=EUNOMIA_GITHUB_CLIENT_SECRET=...
   ExecStart=/usr/local/bin/uv run uvicorn eunomia.api:app --host 0.0.0.0 --port 8000
   Restart=on-failure

   [Install]
   WantedBy=multi-user.target
   ```

   Adjust the `uv` path to wherever it installed (`which uv` on the server).

3. Enable and start it:

   ```
   sudo systemctl daemon-reload
   sudo systemctl enable --now eunomia
   ```

4. `curl http://localhost:8000/health` to confirm it's up. Point a cron/systemd timer
   or the vault sync tool's hook at `POST /sync` to keep the dashboard current, since
   nothing polls the filesystem automatically yet.

To update: `git pull`, `uv sync --extra dev`, `sudo systemctl restart eunomia`.

## Test

```
uv run pytest
```

## Status

Vault reader + rule classifier + SQLite + dashboard, note creation gated on a
configured vault, user-defined buckets beyond the built-in four, and a Settings
page (laid out as independent blocks so new sections drop in cleanly) for the
vault path, buckets, connected accounts (OAuth sign-in where configured, API key
as fallback), and LLM provider keys. No LLM classification fallback yet, and
Slack/Linear/GitHub/HeyPocket/Reminders don't pull data yet — the credentials
just sit ready for when those integrations land.

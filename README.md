# Eunomia

Personal work/uni/business dashboard. Reads an Obsidian vault off disk, buckets notes
by frontmatter tag / folder / inline hashtag, serves a dashboard.

## Install

Requires Python 3.11+.

```
git clone https://github.com/Qyrhal/Eunomia.git
cd Eunomia
python3 -m venv .venv
.venv/bin/pip install -e ".[dev]"
```

## Configure

Environment variables (both optional, shown with defaults):

| Variable            | Default       | Meaning                                   |
|---------------------|---------------|--------------------------------------------|
| `EUNOMIA_VAULT_PATH` | `./vault`     | Path to your Obsidian vault folder         |
| `EUNOMIA_DB_PATH`    | `eunomia.db`  | Path to the SQLite file Eunomia writes to  |

The vault path just needs to be a folder of `.md` files kept in sync by whatever you
already use (Obsidian Sync, Syncthing, iCloud) — Eunomia only reads from disk, it
doesn't talk to Obsidian directly.

## Run locally

```
EUNOMIA_VAULT_PATH=/path/to/vault .venv/bin/uvicorn eunomia.api:app --reload
```

- `GET /` — dashboard
- `POST /sync` — rescan the vault and refresh the DB
- `GET /api/notes?bucket=work` — JSON, `bucket` optional
- `GET /health` — liveness check

## Deploy (self-hosted, always-on)

Runs as a single process — no containers or reverse proxy required, though you can
put one in front for TLS if you expose it beyond localhost.

1. On the server: clone the repo and install as above into `/opt/eunomia` (or wherever).
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
   ExecStart=/opt/eunomia/.venv/bin/uvicorn eunomia.api:app --host 0.0.0.0 --port 8000
   Restart=on-failure

   [Install]
   WantedBy=multi-user.target
   ```

3. Enable and start it:

   ```
   sudo systemctl daemon-reload
   sudo systemctl enable --now eunomia
   ```

4. `curl http://localhost:8000/health` to confirm it's up. Point a cron/systemd timer
   or the vault sync tool's hook at `POST /sync` to keep the dashboard current, since
   nothing polls the filesystem automatically yet.

To update: `git pull`, `.venv/bin/pip install -e ".[dev]"`, `sudo systemctl restart eunomia`.

## Test

```
.venv/bin/pytest
```

## Status

Step 1 of the plan: vault reader + rule classifier + SQLite + dashboard. No LLM fallback,
no Slack/Linear/GitHub/HeyPocket/Reminders integrations yet — those are next.

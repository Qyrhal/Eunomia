# Installing Eunomia

Moved to [docs/installation.md](docs/installation.md). Agents should read its
"Installing as an AI agent" section.

## Backups

A self-hosted Eunomia is one node. There is no high availability: if the
machine or its disk dies, you are down until you restore. The data you can lose
equals the backup interval (nightly backups mean up to about 24 hours).

The `backup` service takes an encrypted SurrealDB export every night at 03:00
(container time, set `TZ` in `.env` for local time) into the `eunomia-backups`
volume. It keeps the last 14 nightly and 8 weekly files and exposes no ports.

1. Set the key in `.env`, then `docker compose up -d`:
   `BACKUP_ENCRYPTION_KEY=$(openssl rand -base64 32)`. Keep a copy somewhere
   off this machine. Without the key the backups cannot be read. With no key
   the service refuses to start and logs why.
2. Back up now: `backend/scripts/backup.sh` (add a directory to also copy the
   file to the host). List files: `backend/scripts/backup.sh list`. Also copy
   the volume off the machine regularly, a backup on the same disk is not enough.
3. Restore: `backend/scripts/restore.sh <file> --wipe`. `--wipe` deletes the
   current database first; without it the dump is replayed over existing data.
   Wrong key gives a clear "could not decrypt" error.
4. Prove it works: `scripts/restore-drill.sh` starts a throwaway SurrealDB
   (`fw-backup-db`, port 8211), seeds records, backs up, wipes, restores and
   compares counts. It exits non-zero on mismatch and always cleans up. It never
   touches your live stack.

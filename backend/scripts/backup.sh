#!/usr/bin/env bash
# Takes an encrypted SurrealDB backup now, via the `backup` compose service.
# The file lands in the `eunomia-backups` volume (same place as the nightly
# ones, prefix "manual-", never rotated). Pass a directory to also copy it
# to the host.
#
#   backend/scripts/backup.sh [host-output-dir]
#   backend/scripts/backup.sh list          # show what is in the volume
#
# Needs the stack running (`docker compose up -d`) and BACKUP_ENCRYPTION_KEY
# set in .env.
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ "${1:-}" = list ]; then exec docker compose exec -T backup eunomia-backup list; fi

docker compose exec -T backup eunomia-backup now manual || {
  echo "backup failed. Is the stack up (docker compose up -d) and BACKUP_ENCRYPTION_KEY set in .env?" >&2
  exit 1
}
if [ -n "${1:-}" ]; then
  mkdir -p "$1"
  latest="$(docker compose exec -T backup sh -c 'ls -1d /backups/manual-* | sort | tail -1' | tr -d '\r')"
  docker compose cp "backup:$latest" "$1/"   # a file, or a directory with one file per database
  echo "copied to $1/$(basename "$latest")"
fi

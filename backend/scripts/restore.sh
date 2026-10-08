#!/usr/bin/env bash
# Restores an encrypted backup made by the `backup` service or backup.sh.
#
#   backend/scripts/restore.sh <file> [--wipe]
#
# <file> is a name in the eunomia-backups volume (see `backup.sh list`) or a
# path on the host (copied in first). Without --wipe the dump is replayed over
# the existing database, which fails or merges if data is already there. With
# --wipe the database is deleted first, then restored: use that to roll back.
# Uses the key in .env, so a different key means a clear "could not decrypt".
set -euo pipefail
cd "$(dirname "$0")/../.."

file="${1:?usage: restore.sh <backup-file> [--wipe]}"
if [ -f "$file" ]; then
  docker compose cp "$file" "backup:/tmp/$(basename "$file")"
  file="/tmp/$(basename "$file")"
fi
exec docker compose exec -T backup eunomia-backup restore "$file" ${2:+"$2"}

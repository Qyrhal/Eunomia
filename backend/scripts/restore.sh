#!/usr/bin/env bash
# Restores an encrypted backup made by the `backup` service or backup.sh.
#
#   backend/scripts/restore.sh <file> [--wipe]
#
# <file> is a name in the eunomia-backups volume (see `backup.sh list`) or a
# path on the host (copied in first). A backup of a tenancy install is a
# directory holding one file per database; it is restored database by database. Without --wipe the dump is replayed over
# the existing database, which fails or merges if data is already there. With
# --wipe the database is deleted first, then restored: use that to roll back.
# Uses the key in .env, so a different key means a clear "could not decrypt".
# The import runs against a SurrealDB started without the 60 s query and transaction limits
# (docker-compose.import.yml layered over docker-compose.yml): a big database cannot load inside
# them. The backend is stopped meanwhile (the app is down), and the hardened server and the backend
# are started again at the end, even if the restore fails.
set -euo pipefail
cd "$(dirname "$0")/../.."

file="${1:?usage: restore.sh <backup-file> [--wipe]}"
C=(docker compose -f docker-compose.yml -f docker-compose.import.yml)
was_running="$(docker compose ps -q --status running backend 2>/dev/null | head -1)"
back_to_hardened() {
  docker compose up -d --wait surrealdb backup >/dev/null || echo "COULD NOT restart the hardened SurrealDB: run docker compose up -d" >&2
  [ -z "$was_running" ] || docker compose up -d backend >/dev/null
}
trap back_to_hardened EXIT
docker compose stop backend >/dev/null
"${C[@]}" up -d --wait surrealdb backup >/dev/null
if [ -e "$file" ]; then
  docker compose cp "$file" "backup:/tmp/$(basename "$file")"
  file="/tmp/$(basename "$file")"
fi
docker compose exec -T backup eunomia-backup restore "$file" ${2:+"$2"}

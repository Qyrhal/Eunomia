#!/usr/bin/env bash
# Restores a .surql dump produced by backup.sh, via `surreal import` run
# inside the `surrealdb` container. The dump file is copied into the
# container first (SurrealDB v2.3's `surreal import` takes a file path, not
# stdin), then imported. The copy is left under the container's own /tmp
# (not the `eunomia-surreal-data` volume) rather than cleaned up afterwards
# -- the image is minimal and ships no shell utilities (no `rm`) to clean up
# with, and /tmp there is container-scoped ephemeral storage anyway, gone on
# the container's next restart.
#
# `surreal import` replays the dump's own `CREATE`/`DEFINE` statements
# against the live database -- it does not wipe existing data first, so
# restoring into a database that already has rows merges/overwrites by
# record id rather than giving you a guaranteed-clean copy. For a clean
# restore, start from a fresh `eunomia-surreal-data` volume first.
#
# Usage (from the repo root, or anywhere):
#   backend/scripts/restore.sh backups/eunomia-20260101T000000Z.surql
#
# Needs the `surrealdb` service already running: `docker compose up -d`.
# Never `docker compose down -v` to "start clean" -- that destroys the volume
# and any real data in it; use a fresh volume/environment instead.
set -euo pipefail

cd "$(dirname "$0")/../.."  # repo root, so `docker compose` finds docker-compose.yml

file="${1:?usage: restore.sh <path-to-.surql-dump>}"
[ -f "$file" ] || { echo "no such file: $file" >&2; exit 1; }

SURREAL_USER="${SURREAL_USER:-root}"
SURREAL_PASS="${SURREAL_PASS:-root}"
SURREAL_NS="${SURREAL_NS:-eunomia}"
SURREAL_DB="${SURREAL_DB:-eunomia}"

tmp="/tmp/$(basename "$file")"
docker compose cp "$file" "surrealdb:$tmp"
docker compose exec -T surrealdb /surreal import \
  --conn http://localhost:8000 \
  --user "$SURREAL_USER" --pass "$SURREAL_PASS" \
  --ns "$SURREAL_NS" --db "$SURREAL_DB" \
  "$tmp"

echo "restored $file"

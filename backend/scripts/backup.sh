#!/usr/bin/env bash
# Dumps the SurrealDB data (everything in the eunomia-surreal-data volume)
# to a timestamped SurrealQL script, via `surreal export` -- the only backup
# mechanism SurrealDB v2.3's CLI offers. Run inside the `surrealdb` container
# (the `surreal` binary is Docker-only here, see backend/tests/conftest.py's
# docstring) and streamed to a file on the host via `docker compose exec`'s
# stdout, so nothing needs installing on the host.
#
# Usage (from the repo root, or anywhere -- it cd's to the repo root itself):
#   backend/scripts/backup.sh [output-dir]      # default output-dir: backups/
#
# Needs the `surrealdb` service already running: `docker compose up -d`.
set -euo pipefail

cd "$(dirname "$0")/../.."  # repo root, so `docker compose` finds docker-compose.yml

out_dir="${1:-backups}"
mkdir -p "$out_dir"

SURREAL_USER="${SURREAL_USER:-root}"
SURREAL_PASS="${SURREAL_PASS:-root}"
SURREAL_NS="${SURREAL_NS:-eunomia}"
SURREAL_DB="${SURREAL_DB:-eunomia}"

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
out="${out_dir}/eunomia-${stamp}.surql"

docker compose exec -T surrealdb /surreal export \
  --conn http://localhost:8000 \
  --user "$SURREAL_USER" --pass "$SURREAL_PASS" \
  --ns "$SURREAL_NS" --db "$SURREAL_DB" \
  - > "$out"

echo "wrote $out"

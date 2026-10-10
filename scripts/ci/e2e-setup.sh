#!/usr/bin/env bash
# Prepares a running stack for the Playwright suite (used by the CI e2e job, usable locally):
#   1. registers and onboards the install's first user (the instance admin) with a generated password,
#   2. finds the org database that user was given (org_<uuid>).
# Prints KEY=VALUE lines for E2E_ADMIN_EMAIL, E2E_ADMIN_PASSWORD, E2E_SURREAL_NS and E2E_SURREAL_DB
# (append them to $GITHUB_ENV). Needs the frontend at $E2E_BASE_URL and SurrealDB's HTTP port at
# $E2E_SURREAL_HTTP (docker-compose.ci.yml publishes it on 127.0.0.1).
set -euo pipefail
base="${E2E_BASE_URL:-http://localhost:3000}"
db_http="${E2E_SURREAL_HTTP:-http://127.0.0.1:8000}"
ns="${SURREAL_NS:-eunomia}"
auth="${SURREAL_USER:-root}:${SURREAL_PASS:-root}"
email="e2e-admin-$(date +%s)@example.com"
password="$(openssl rand -hex 16)"
jar="$(mktemp)"; trap 'rm -f "$jar"' EXIT

curl -fsS -c "$jar" -H 'Content-Type: application/json' \
  -d "{\"email\":\"$email\",\"password\":\"$password\"}" "$base/api/auth/register" >/dev/null
curl -fsS -b "$jar" -X POST "$base/api/settings/complete-onboarding" >/dev/null

# The org databases live in the same namespace as `control`; one user, one org database.
dbs="$(curl -fsS -u "$auth" -H 'Accept: application/json' -H "surreal-ns: $ns" -H 'surreal-db: control' \
  --data 'INFO FOR NS;' "$db_http/sql" | jq -r '.[0].result.databases | keys[] | select(startswith("org_"))')"
[ "$(printf '%s\n' "$dbs" | wc -l | tr -d ' ')" = 1 ] || { echo "expected exactly one org database, got: $dbs" >&2; exit 1; }

printf 'E2E_ADMIN_EMAIL=%s\nE2E_ADMIN_PASSWORD=%s\nE2E_SURREAL_NS=%s\nE2E_SURREAL_DB=%s\n' "$email" "$password" "$ns" "$dbs"

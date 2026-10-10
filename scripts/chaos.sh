#!/usr/bin/env bash
# CHAOS TEST. DESTRUCTIVE. NEVER POINT THIS AT A REAL INSTALL.
# Starts its own throwaway compose project (-p fw-chaos, own volumes and port),
# then kills and restarts the backend and SurrealDB containers at random
# intervals while k6 load runs. Afterwards: /healthz must recover and the
# acknowledged memory count must not be lower than before.
#   CHAOS_CONFIRM=yes scripts/chaos.sh
# Env: CHAOS_PORT (default 18101), CHAOS_SECONDS (default 90), K6_IMAGE.
set -euo pipefail
cd "$(dirname "$0")/.."

[ "${CHAOS_CONFIRM:-}" = yes ] || { echo "refusing: set CHAOS_CONFIRM=yes (throwaway stack only)"; exit 2; }
PROJECT=fw-chaos
PORT=${CHAOS_PORT:-18101}
case "$PORT" in 8001|8101|3000) echo "refusing: port $PORT looks like a real install"; exit 2;; esac
SECS=${CHAOS_SECONDS:-90}
K6_IMAGE=${K6_IMAGE:-grafana/k6:0.54.0}
BASE=http://localhost:$PORT

export COMPOSE_PROJECT_NAME=$PROJECT BACKEND_PORT=$PORT FRONTEND_PORT=13999
export JWT_SECRET=$(openssl rand -hex 32) ENCRYPTION_KEY=$(openssl rand -hex 32)
dc() { docker compose -p "$PROJECT" "$@"; }
cleanup() { dc down -v >/dev/null 2>&1 || true; }
trap cleanup EXIT

dc up -d --build surrealdb backend
wait_health() { for _ in $(seq "${1:-60}"); do curl -fsS "$BASE/healthz" >/dev/null 2>&1 && return 0; sleep 1; done; return 1; }
wait_health 120 || { echo "FAIL: stack never became healthy"; exit 1; }

# A chaos user with a known set of acknowledged memories.
EMAIL="chaos-$(openssl rand -hex 4)@chaos.invalid"
JAR=$(mktemp)
curl -fsS -c "$JAR" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL\",\"password\":\"chaos-test-password-1\"}" "$BASE/api/auth/register" >/dev/null
TOKEN=$(curl -fsS -b "$JAR" -H 'content-type: application/json' -d '{"name":"chaos"}' "$BASE/api/auth/tokens" | sed 's/.*"token":"\([^"]*\)".*/\1/')
mcp() { curl -fsS -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' -H 'accept: application/json' -d "$1" "$BASE/mcp"; }
for i in $(seq 25); do
  mcp "{\"jsonrpc\":\"2.0\",\"id\":$i,\"method\":\"tools/call\",\"params\":{\"name\":\"memory_write\",\"arguments\":{\"subject_name\":\"Chaos Subject $i\",\"subject_kind\":\"person\",\"text\":\"chaos fact $i\"}}}" >/dev/null
done
count() { mcp '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"entities_search","arguments":{"query":"Chaos Subject","limit":100}}}' | grep -o 'Chaos Subject [0-9]*' | sort -u | wc -l | tr -d ' '; }
BEFORE=$(count)
echo "records before chaos: $BEFORE"
[ "$BEFORE" -gt 0 ] || { echo "FAIL: could not count seed records"; exit 1; }

# Load in the background (note: k6 setup registers its own users in this throwaway stack).
docker run --rm -i --network host -v "$PWD/loadtest/k6:/k6" -e BASE_URL="$BASE" -e ORGS=5 -e DURATION="${SECS}s" "$K6_IMAGE" run /k6/steady.js \
  >/tmp/fw-chaos-k6.log 2>&1 &
K6_PID=$!

# Random kills: backend or database, restart after a short pause.
END=$((SECONDS + SECS))
while [ $SECONDS -lt $END ]; do
  sleep $((RANDOM % 10 + 5))
  svc=$([ $((RANDOM % 2)) -eq 0 ] && echo backend || echo surrealdb)
  echo "chaos: killing $svc"; dc kill "$svc"
  sleep $((RANDOM % 4 + 1))
  dc start "$svc"
done
wait "$K6_PID" || echo "note: k6 thresholds failing during chaos is expected, see /tmp/fw-chaos-k6.log"

dc start surrealdb backend >/dev/null
wait_health 120 || { echo "FAIL: /healthz did not recover"; exit 1; }
sleep 3
AFTER=$(count)
echo "records after chaos: $AFTER"
[ "$AFTER" -ge "$BEFORE" ] || { echo "FAIL: records lost ($BEFORE -> $AFTER)"; exit 1; }
echo "PASS: recovered, no acknowledged records lost"

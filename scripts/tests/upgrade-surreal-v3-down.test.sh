#!/usr/bin/env bash
# Mock-based test of the "surrealdb is not running" path of scripts/upgrade-surreal-v3.sh
# (an install broken by an old updater that pulled a 3.x release over 2.x data). No Docker needed.
# The real recovery (temp 2.x server on a copy, backup, export, import) is covered only by
# scripts/tests/upgrade-surreal-v3.test.sh with Docker, which has not been run for this path.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PASS=0; FAIL=0
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
check() { local d="$1"; shift; if "$@" >/dev/null 2>&1; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); echo "FAIL: $d"; fi; }
mkdir -p "$T/bin" "$T/repo"; : > "$T/repo/.env"; touch "$T/repo/docker-compose.import.yml"
cat > "$T/bin/docker" <<SHIM
#!/bin/sh
echo "\$@" >> "$T/docker.log"
case "\$*" in
  "compose ps -q"*) ;;                                     # nothing running
  "compose config --images") echo surrealdb/surrealdb:v3.3.1 ;;
  "volume ls"*) cat "$T/volume" 2>/dev/null ;;
  "network inspect"*) exit 1 ;;
esac
exit 0
SHIM
chmod +x "$T/bin/docker"
run() { EUNOMIA_DIR="$T/repo" COMPOSE_PROJECT_NAME=proj PATH="$T/bin:$PATH" bash "$HERE/../upgrade-surreal-v3.sh" 2>&1; }

: > "$T/docker.log"; rm -f "$T/volume"
out="$(run)"; rc=$?
check "down, no data volume: exits 0, nothing to upgrade" test "$rc" -eq 0
check "down, no data volume: says so" grep -q 'nothing to upgrade' <<<"$out"

echo proj_eunomia-surreal-data > "$T/volume"; : > "$T/docker.log"
out="$(run)"; rc=$?
check "down, volume found, no network: fails with a hint" test "$rc" -ne 0
check "down: the original volume is never mounted" bash -c "! grep -q 'proj_eunomia-surreal-data:/data' '$T/docker.log'"
check "down: no container was started" bash -c "! grep -q '^run -d' '$T/docker.log'"

echo SURREAL_DATA_VOLUME=eunomia-surreal-data-v3 > "$T/repo/.env"; : > "$T/docker.log"
out="$(run)"; rc=$?
check "down but already on the v3 volume: exits 0" test "$rc" -eq 0
echo "upgrade-down: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]

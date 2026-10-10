#!/usr/bin/env bash
# Mock-docker test of backend/scripts/restore.sh: the import runs on a server started with
# docker-compose.import.yml (no query/transaction timeouts), and the hardened server and the backend
# come back afterwards, also when the restore fails. No Docker needed.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$HERE/../.."
PASS=0; FAIL=0
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
check() { local d="$1"; shift; if "$@" >/dev/null 2>&1; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); echo "FAIL: $d"; fi; }
mkdir -p "$T/bin" "$T/repo/backend/scripts"; cp "$ROOT/backend/scripts/restore.sh" "$T/repo/backend/scripts/"
cat > "$T/bin/docker" <<SHIM
#!/bin/sh
echo "\$@" >> "$T/docker.log"
case "\$*" in
  *"ps -q --status running backend"*) echo "\${MOCK_BACKEND:-}" ;;
  *"eunomia-backup restore"*) [ -z "\${MOCK_FAIL:-}" ] || exit 1 ;;
esac
exit 0
SHIM
chmod +x "$T/bin/docker"
run() { PATH="$T/bin:$PATH" bash "$T/repo/backend/scripts/restore.sh" "$@" >/dev/null 2>&1; }
line() { grep -n "$1" "$T/docker.log" | head -1 | cut -d: -f1; }

: > "$T/docker.log"; MOCK_BACKEND=abc run manual-1 --wipe; rc=$?
check "restore succeeds" test "$rc" -eq 0
check "the import server is started with the import override" grep -q '^compose -f docker-compose.yml -f docker-compose.import.yml up -d --wait surrealdb backup' "$T/docker.log"
check "the override is up before the restore runs" test "$(line 'docker-compose.import.yml up')" -lt "$(line 'eunomia-backup restore manual-1 --wipe')"
check "the plain hardened compose file is started after the restore" test "$(line '^compose up -d --wait surrealdb backup')" -gt "$(line 'eunomia-backup restore')"
check "the backend is stopped first and started last" bash -c "[ \$(grep -n '^compose stop backend' '$T/docker.log' | head -1 | cut -d: -f1) -lt \$(grep -n 'import.yml up' '$T/docker.log' | head -1 | cut -d: -f1) ] && tail -1 '$T/docker.log' | grep -q '^compose up -d backend'"

: > "$T/docker.log"; MOCK_BACKEND=abc MOCK_FAIL=1 run manual-1; rc=$?
check "a failed restore exits non-zero" test "$rc" -ne 0
check "a failed restore still brings the hardened server back" grep -q '^compose up -d --wait surrealdb backup' "$T/docker.log"

: > "$T/docker.log"; MOCK_BACKEND= run manual-1
check "a backend that was not running is not started" bash -c "! grep -q '^compose up -d backend' '$T/docker.log'"

echo "restore.test.sh: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]

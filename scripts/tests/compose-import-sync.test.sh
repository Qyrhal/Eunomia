#!/usr/bin/env bash
# The surrealdb entrypoint in docker-compose.import.yml must equal the one in docker-compose.yml except
# for the two timeout flags. No Docker needed.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
entrypoint() { # the "- arg" lines of the first `entrypoint:` list, comments dropped
  awk '/^    entrypoint:/ {on=1; next} on && /^      - / {sub(/^      - /, ""); print; next} on && /^      #/ {next} on {exit}' "$1"
}
main="$(entrypoint "$ROOT/docker-compose.yml" | grep -v -e '^--query-timeout=' -e '^--transaction-timeout=')"
imp="$(entrypoint "$ROOT/docker-compose.import.yml")"
if [ -n "$imp" ] && [ "$main" = "$imp" ]; then echo "compose-import-sync: ok"; exit 0; fi
echo "FAIL: docker-compose.import.yml entrypoint differs from docker-compose.yml (besides the timeouts):"
diff <(echo "$main") <(echo "$imp")
exit 1

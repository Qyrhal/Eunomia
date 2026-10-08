#!/usr/bin/env bash
# Tests scripts/auto-update.sh: a local git repo stands in for GitHub and a
# `docker` shim records what would have run.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REAL_SCRIPT="$HERE/../auto-update.sh"
PASS=0; FAIL=0
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
check() { local d="$1"; shift; if "$@" >/dev/null 2>&1; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); echo "FAIL: $d"; fi; }
field() { python3 -c 'import json,sys;print(str(json.load(open(sys.argv[1]))[sys.argv[2]]).lower())' "$TMP/repo/update-status/status.json" "$1"; }
git_() { git -c user.email=t@t -c user.name=t "$@"; }

# "GitHub": v1.0.0 -> v1.1.0
mkdir -p "$TMP/src/scripts" "$TMP/bin"
cp "$REAL_SCRIPT" "$TMP/src/scripts/auto-update.sh"
(cd "$TMP/src" && git init -q -b main && echo one > compose.yml && git_ add -A && git_ commit -qm v1 && git tag v1.0.0 \
  && echo two > compose.yml && git_ commit -qam v1.1 && git tag v1.1.0)
git clone -q --branch v1.0.0 "file://$TMP/src" "$TMP/repo" 2>/dev/null
printf 'JWT_SECRET=x\nEUNOMIA_IMAGE_TAG=v1.0.0\n' > "$TMP/repo/.env"
# the shim answers `compose config --services` from $TMP/services (default: no caddy)
printf 'surrealdb\nbackend\nfrontend\nupdater\n' > "$TMP/services"
printf '#!/bin/sh\necho "$@" >> "%s/docker.log"\n[ "$1 $2" = "compose config" ] && cat "%s/services"\nexit 0\n' "$TMP" "$TMP" > "$TMP/bin/docker"; chmod +x "$TMP/bin/docker"
update() { EUNOMIA_DIR="$TMP/repo" PATH="$TMP/bin:$PATH" bash "$TMP/repo/scripts/auto-update.sh"; }
S="$TMP/repo/update-status"

# 1. a check alone reports, never applies
update
check "status says an update is available" test "$(field update_available)" = true
check "status names both versions" test "$(field current_version)" = v1.0.0 -a "$(field latest_version)" = v1.1.0
check "a check alone doesn't touch the checkout" test "$(cat "$TMP/repo/compose.yml")" = one
check "a check alone doesn't run docker" test ! -e "$TMP/docker.log"

# 1b. "Check now": a check marker forces a GitHub check inside the 10-minute window
(cd "$TMP/src" && git_ tag v1.0.5 HEAD~1)
update; check "throttled: still sees v1.1.0 as newest" test "$(field latest_version)" = v1.1.0
git -C "$TMP/src" tag -d v1.0.5 >/dev/null
touch "$S/check"; update
check "check marker consumed" test ! -e "$S/check"

# 1c. a second runner while one holds the lock does nothing
mkdir "$S/.lock"; touch "$S/requested"; update
check "locked: nothing applied" test "$(cat "$TMP/repo/compose.yml")" = one
check "locked: marker left for the lock holder" test -e "$S/requested"
rmdir "$S/.lock"; rm -f "$S/requested"

# 2. the button (marker file) applies it
touch "$S/requested"
update
check "checked out the new release" test "$(cat "$TMP/repo/compose.yml")" = two
check ".env is pinned to the new release" grep -q '^EUNOMIA_IMAGE_TAG=v1.1.0$' "$TMP/repo/.env"
check ".env keeps its other settings" grep -q '^JWT_SECRET=x$' "$TMP/repo/.env"
check "images were pulled" grep -q 'compose pull backend frontend' "$TMP/docker.log"
check "stack was restarted" grep -q 'compose up -d' "$TMP/docker.log"
check "the updater service is left out of the restart" bash -c "! grep -q 'up -d --remove-orphans.*updater' '$TMP/docker.log'"
check "lock released" test ! -e "$S/.lock"
check "never recreates the updater from inside itself" bash -c "! grep -q 'up -d updater' '$TMP/docker.log'"
check "status now up to date" test "$(field current_version)" = v1.1.0 -a "$(field update_available)" = false
check "marker file consumed" test ! -e "$S/requested"
check "history recorded" grep -q 'updated v1.0.0 -> v1.1.0' "$S/history.log"

# 3. a request with nothing newer is a no-op
: > "$TMP/docker.log"; touch "$S/requested"; update
check "nothing newer: no docker run" test ! -s "$TMP/docker.log"
check "nothing newer: marker consumed" test ! -e "$S/requested"

# 3b. without HTTPS no caddy reload; with HTTPS on, the update restarts and
#     reloads caddy (a changed Caddyfile isn't noticed by compose)
check "no HTTPS: no caddy reload" bash -c "! grep -q 'caddy reload' '$TMP/docker.log'"
(cd "$TMP/src" && echo https > compose.yml && git_ commit -qam v1.1.1 && git tag v1.1.1)
printf 'surrealdb\nbackend\nfrontend\ncaddy\nupdater\n' > "$TMP/services"
: > "$TMP/docker.log"; touch "$S/requested"; update
check "HTTPS on: caddy restarted with the stack" grep -q 'up -d --remove-orphans .*caddy' "$TMP/docker.log"
check "HTTPS on: Caddyfile reloaded" grep -q 'exec -T caddy caddy reload --config /etc/caddy/Caddyfile' "$TMP/docker.log"
check "HTTPS on: updater still not restarted by itself" bash -c "! grep -q 'up -d --remove-orphans.*updater' '$TMP/docker.log'"
printf 'surrealdb\nbackend\nfrontend\nupdater\n' > "$TMP/services"

# 4. local edits to tracked files are reported, not clobbered
(cd "$TMP/src" && echo three > compose.yml && git_ commit -qam v1.2 && git tag v1.2.0)
echo mine > "$TMP/repo/compose.yml"
touch "$S/requested"; update
check "refuses over local changes" test "$(field error)" != none
check "error says why" grep -q 'local changes' "$S/status.json"
check "local edit survives" test "$(cat "$TMP/repo/compose.yml")" = mine
(cd "$TMP/repo" && git checkout -q compose.yml)

# 5. GitHub unreachable is reported
rm -f "$S/status.json"; (cd "$TMP/repo" && git remote set-url origin "file://$TMP/nowhere")
update
check "unreachable origin is reported" grep -q 'could not reach GitHub' "$S/status.json"

echo "auto-update: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]

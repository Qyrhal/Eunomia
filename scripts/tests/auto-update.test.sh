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
# Besides logging, the shim answers the queries the SurrealDB 3 hook makes (the compose file's
# images in $TMP/images, the running surrealdb image in $TMP/running) and the service list
# ($TMP/services; default: no caddy).
printf 'surrealdb\nbackend\nfrontend\nupdater\n' > "$TMP/services"
cat > "$TMP/bin/docker" <<SHIM
#!/bin/sh
echo "\$@" >> "$TMP/docker.log"
case "\$*" in
  "compose config --images") cat "$TMP/images" 2>/dev/null ;;
  "compose config --services") cat "$TMP/services" ;;
  "compose ps -q surrealdb") [ -f "$TMP/running" ] && echo fakecid ;;
  inspect*) cat "$TMP/running" 2>/dev/null ;;
  "compose exec -T backup cat /backups/.backup-key") cat "$TMP/volkey" 2>/dev/null ;;
esac
exit 0
SHIM
chmod +x "$TMP/bin/docker"
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
check ".env gains a backup key" grep -Eq '^BACKUP_ENCRYPTION_KEY=.{20,}$' "$TMP/repo/.env"
check "exactly one backup key line" test "$(grep -c '^BACKUP_ENCRYPTION_KEY=' "$TMP/repo/.env")" = 1
check "images were pulled, the backup image too" grep -q 'compose pull backend frontend backup' "$TMP/docker.log"
check "no SurrealDB upgrade for a 2.x release" bash -c "! grep -q UPGRADE '$TMP/docker.log'"
check "stack was restarted" grep -q 'compose up -d' "$TMP/docker.log"
check "the updater service is left out of the restart" bash -c "! grep -q 'up -d --remove-orphans.*updater' '$TMP/docker.log'"
check "lock released" test ! -e "$S/.lock"
check "never recreates the updater from inside itself" bash -c "! grep -q 'up -d updater' '$TMP/docker.log'"
check "status now up to date" test "$(field current_version)" = v1.1.0 -a "$(field update_available)" = false
check "marker file consumed" test ! -e "$S/requested"
check "history recorded" grep -q 'updated v1.0.0 -> v1.1.0' "$S/history.log"
# the fixture .env has no ENCRYPTION_KEY: the update must add one (the backend
# now refuses to start without a key of 16+ characters)
key1="$(sed -n 's/^ENCRYPTION_KEY=//p' "$TMP/repo/.env")"
check "missing ENCRYPTION_KEY generated before restart" test "${#key1}" -ge 32
check "key generation logged" grep -q 'generated ENCRYPTION_KEY' "$S/history.log"
check "exactly one ENCRYPTION_KEY line" test "$(grep -c '^ENCRYPTION_KEY=' "$TMP/repo/.env")" = 1

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
check "an existing valid key is never replaced" test "$(sed -n 's/^ENCRYPTION_KEY=//p' "$TMP/repo/.env")" = "$key1"
check "key generated only once" test "$(grep -c 'generated ENCRYPTION_KEY' "$S/history.log")" = 1

# 4. local edits to tracked files are reported, not clobbered
(cd "$TMP/src" && echo three > compose.yml && git_ commit -qam v1.2 && git tag v1.2.0)
echo mine > "$TMP/repo/compose.yml"
touch "$S/requested"; update
check "refuses over local changes" test "$(field error)" != none
check "error says why" grep -q 'local changes' "$S/status.json"
check "local edit survives" test "$(cat "$TMP/repo/compose.yml")" = mine
(cd "$TMP/repo" && git checkout -q compose.yml)

# 4a. a backup service that generated its own key (no key in .env): .env adopts that key
echo 'VolumeKeyVolumeKeyVolumeKey0123456789abcd=' > "$TMP/volkey"
sed -i.bak '/^BACKUP_ENCRYPTION_KEY=/d' "$TMP/repo/.env" && rm -f "$TMP/repo/.env.bak"
(cd "$TMP/src" && echo threeb > compose.yml && git_ commit -qam v1.2.1 && git tag v1.2.1)
touch "$S/requested"; update
check "the backup service's own key is adopted" grep -qx 'BACKUP_ENCRYPTION_KEY=VolumeKeyVolumeKeyVolumeKey0123456789abcd=' "$TMP/repo/.env"
echo 'not a key; rm -rf /' > "$TMP/volkey"
sed -i.bak '/^BACKUP_ENCRYPTION_KEY=/d' "$TMP/repo/.env" && rm -f "$TMP/repo/.env.bak"
(cd "$TMP/src" && echo threec > compose.yml && git_ commit -qam v1.2.2 && git tag v1.2.2)
touch "$S/requested"; update
check "a malformed volume key is not copied into .env" bash -c "grep -Eq '^BACKUP_ENCRYPTION_KEY=[A-Za-z0-9+/=]{40,}$' '$TMP/repo/.env' && ! grep -q 'rm -rf' '$TMP/repo/.env'"
rm -f "$TMP/volkey"

# 4b. SurrealDB 3 hook: a release pinning 3.x over a running 2.x runs the upgrade
# script before pulling, and a failed upgrade leaves the install on the old release
BK="$(sed -n 's/^BACKUP_ENCRYPTION_KEY=//p' "$TMP/repo/.env")"
(cd "$TMP/src" && mkdir -p scripts && printf '#!/bin/sh\necho UPGRADE >> "%s/docker.log"\nexit $(cat "%s/upgrade_rc")\n' "$TMP" "$TMP" > scripts/upgrade-surreal-v3.sh \
  && echo four > compose.yml && echo '      - FRONTEND_TRUST_FORWARDED=${FRONTEND_TRUST_FORWARDED:-}' > docker-compose.yml \
  && git_ add -A && git_ commit -qm v2.0 && git tag v2.0.0)
echo BACKEND_PORT=8911 >> "$TMP/repo/.env"
echo surrealdb/surrealdb:v3.3.1 > "$TMP/images"; echo surrealdb/surrealdb:v2.3 > "$TMP/running"
echo 1 > "$TMP/upgrade_rc"; : > "$TMP/docker.log"; touch "$S/requested"; update
check "failed upgrade: ran before anything else" grep -qx UPGRADE "$TMP/docker.log"
check "failed upgrade: nothing pulled" bash -c "! grep -q 'compose pull' '$TMP/docker.log'"
check "failed upgrade: checkout put back" test "$(cat "$TMP/repo/compose.yml")" = threec
check "failed upgrade: .env back on the old release" grep -q '^EUNOMIA_IMAGE_TAG=v1.2.2$' "$TMP/repo/.env"
check "failed upgrade: status says why" grep -q 'upgrade failed' "$S/status.json"
check "failed upgrade: no 2.x settings written" bash -c "! grep -q '^PUBLIC_URL=' '$TMP/repo/.env'"
# same failure on an install whose .env has no EUNOMIA_IMAGE_TAG: the line must not be left behind
sed -i.bak '/^EUNOMIA_IMAGE_TAG=/d' "$TMP/repo/.env" && rm -f "$TMP/repo/.env.bak"
echo 1 > "$TMP/upgrade_rc"; : > "$TMP/docker.log"; touch "$S/requested"; update
check "failed upgrade without a tag in .env: none written back" bash -c "! grep -q '^EUNOMIA_IMAGE_TAG=' '$TMP/repo/.env'"
echo 0 > "$TMP/upgrade_rc"; : > "$TMP/docker.log"; touch "$S/requested"; update
check "upgrade runs once, before the pull" bash -c "[ \$(grep -c UPGRADE '$TMP/docker.log') = 1 ] && [ \$(grep -n UPGRADE '$TMP/docker.log' | cut -d: -f1) -lt \$(grep -n 'compose pull' '$TMP/docker.log' | cut -d: -f1) ]"
check "upgraded release is applied" test "$(cat "$TMP/repo/compose.yml")" = four
check "upgraded release is pinned in .env" grep -q '^EUNOMIA_IMAGE_TAG=v2.0.0$' "$TMP/repo/.env"
check "moving to 2.x sets PUBLIC_URL to the API port" grep -qx 'PUBLIC_URL=http://localhost:8911' "$TMP/repo/.env"
check "no HTTPS: the frontend keeps listening on the network" bash -c "! grep -q '^FRONTEND_BIND=' '$TMP/repo/.env'"
check "the backup key is kept across updates" test "$(sed -n 's/^BACKUP_ENCRYPTION_KEY=//p' "$TMP/repo/.env")" = "$BK"
echo surrealdb/surrealdb:v3.3.1 > "$TMP/running"
(cd "$TMP/src" && echo five > compose.yml && git_ commit -qam v2.0.1 && git tag v2.0.1)
: > "$TMP/docker.log"; touch "$S/requested"; update
check "already on 3.x: no upgrade" bash -c "! grep -q UPGRADE '$TMP/docker.log'"
check "already on 3.x: still updates" test "$(cat "$TMP/repo/compose.yml")" = five
# 2.x stopped (an install an old updater broke): the script must still be called.
# It also had HTTPS on: the move to 2.x points PUBLIC_URL at the domain and keeps
# the frontend on loopback behind Caddy.
sed -i.bak '/^PUBLIC_URL=/d' "$TMP/repo/.env" && rm -f "$TMP/repo/.env.bak"
printf 'COMPOSE_PROFILES=https\nEUNOMIA_DOMAIN=eunomia.example.com\n' >> "$TMP/repo/.env"
(cd "$TMP/src" && echo six > compose.yml && git_ commit -qam v2.0.2 && git tag v2.0.2)
rm -f "$TMP/running"
: > "$TMP/docker.log"; touch "$S/requested"; update
check "surrealdb not running: upgrade script still runs" grep -qx UPGRADE "$TMP/docker.log"
check "surrealdb not running: release applied after the upgrade" test "$(cat "$TMP/repo/compose.yml")" = six
check "HTTPS install: PUBLIC_URL is the domain" grep -qx 'PUBLIC_URL=https://eunomia.example.com' "$TMP/repo/.env"
check "HTTPS install: frontend trusts Caddy and listens on loopback" bash -c "grep -qx 'FRONTEND_TRUST_FORWARDED=1' '$TMP/repo/.env' && grep -qx 'FRONTEND_BIND=127.0.0.1' '$TMP/repo/.env'"
rm -f "$TMP/images" "$TMP/running"

# 5. GitHub unreachable is reported
rm -f "$S/status.json"; (cd "$TMP/repo" && git remote set-url origin "file://$TMP/nowhere")
update
check "unreachable origin is reported" grep -q 'could not reach GitHub' "$S/status.json"

echo "auto-update: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]

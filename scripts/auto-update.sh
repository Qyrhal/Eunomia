#!/usr/bin/env bash
# Self-update check/apply. Run every 20s by the `updater` compose service
# (scripts/updater.sh), or by hand / cron on the host -- a lock keeps two
# runners from overlapping. See docs/deployment.md's "Updates" section.
#
# The web-facing backend never gets git or the docker socket: it only drops
# marker files in update-status/ ("check" = check GitHub now, "requested" =
# apply the newest release), and this script does the rest.
#
# Writes $REPO/update-status/status.json (polled by GET /api/update/status)
# at most every CHECK_EVERY_MIN minutes, comparing the running release
# (EUNOMIA_IMAGE_TAG in .env) with the newest v* tag on GitHub. Applies an
# update -- check out that tag, pull its prebuilt images, restart -- only when
# update-status/requested exists (written by POST /api/update/request, i.e.
# someone clicked "Update now"). Never auto-applies on its own.
#
# Everything runs inside main() so bash has parsed the whole script before
# `git checkout` replaces this file with the new release's copy.
#
# Every run also applies Settings → HTTPS (see https_apply below).
set -uo pipefail
# cron/launchd start with a bare PATH; appended so an existing PATH still wins
export PATH="$PATH:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin"

CHECK_EVERY_MIN=10

main() {
  local repo status_dir current latest
  repo="${EUNOMIA_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
  status_dir="$repo/update-status"
  mkdir -p "$status_dir"
  cd "$repo" || exit 1

  # One runner at a time (container + an old host cron job may both exist).
  # A lock older than 30 minutes is from a crashed run.
  find "$status_dir/.lock" -maxdepth 0 -mmin +30 -exec rmdir {} \; 2>/dev/null
  mkdir "$status_dir/.lock" 2>/dev/null || exit 0
  trap "rmdir '$status_dir/.lock' 2>/dev/null" EXIT

  https_apply "$status_dir"

  # Nothing requested and checked recently: skip the network round-trip.
  if [ ! -f "$status_dir/requested" ] && [ ! -f "$status_dir/check" ] \
    && [ -n "$(find "$status_dir/status.json" -mmin -$CHECK_EVERY_MIN 2>/dev/null)" ]; then
    exit 0
  fi
  rm -f "$status_dir/check"

  current="$(sed -n 's/^EUNOMIA_IMAGE_TAG=//p' .env 2>/dev/null | tail -1)"
  [ -n "$current" ] || current="$(git describe --tags --exact-match 2>/dev/null || echo unknown)"

  if ! latest="$(git ls-remote --tags --refs origin 'v*' 2>"$status_dir/.last-error" | sed 's#.*refs/tags/##' | sort -V | tail -1)" || [ -z "$latest" ]; then
    write_status "$status_dir" "$current" "$current" false "could not reach GitHub to check for releases: $(cat "$status_dir/.last-error")"
    exit 1
  fi

  if [ ! -f "$status_dir/requested" ] || [ "$current" = "$latest" ]; then
    rm -f "$status_dir/requested"
    write_status "$status_dir" "$current" "$latest" false ""
    exit 0
  fi

  write_status "$status_dir" "$current" "$latest" true ""
  rm -f "$status_dir/requested"

  # Local edits to tracked files (e.g. docker-compose.yml) can't be swapped
  # for the new release's copy safely -- report rather than clobber them.
  if ! git diff --quiet || ! git diff --cached --quiet; then
    write_status "$status_dir" "$current" "$latest" false "local changes in $repo -- commit or stash them, then click Update again"
    exit 1
  fi

  if ! { git fetch --depth 1 origin tag "$latest" --quiet && git checkout --quiet "$latest"; } 2>"$status_dir/.last-error"; then
    write_status "$status_dir" "$current" "$latest" false "$(cat "$status_dir/.last-error")"
    exit 1
  fi

  set_env EUNOMIA_IMAGE_TAG "$latest"
  ensure_encryption_key "$status_dir"

  # Everything except the updater itself (recreating it here would kill this
  # run); it is refreshed last, after the status is written.
  local services
  services="$(docker compose config --services 2>/dev/null | grep -vx updater | tr '\n' ' ')"
  if ! { docker compose pull backend frontend && docker compose up -d --remove-orphans $services; } >/dev/null 2>"$status_dir/.last-error"; then
    write_status "$status_dir" "$latest" "$latest" false "restart failed: $(tail -5 "$status_dir/.last-error")"
    exit 1
  fi

  # HTTPS on: the new release may ship a changed Caddyfile, which compose
  # doesn't notice (it's a mounted file) -- reload it, no downtime.
  if printf '%s\n' $services | grep -qx caddy; then
    docker compose exec -T caddy caddy reload --config /etc/caddy/Caddyfile >/dev/null 2>&1 || true
  fi

  rm -f "$status_dir/.last-error"
  write_status "$status_dir" "$latest" "$latest" false ""
  echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) updated $current -> $latest" >> "$status_dir/history.log"
  # Pick up a changed updater definition. Not from in here: recreating the
  # container this runs in would kill the command halfway and leave no
  # updater. A short-lived sibling container does it a few seconds later
  # (a no-op when nothing changed). The repo is at the same path on the host.
  if [ -f /.dockerenv ] && docker compose config --services 2>/dev/null | grep -qx updater; then
    docker run -d --rm -v /var/run/docker.sock:/var/run/docker.sock -v "$PWD:$PWD" -w "$PWD" docker:27-cli \
      sh -c "sleep 5 && docker compose up -d updater" >/dev/null 2>&1
  fi
}

# set_env <KEY> <value>: replace or append KEY=value in .env. Callers pass
# validated values only (no '#', '&' or newlines).
# Since v1.3 the backend refuses to start without an ENCRYPTION_KEY of at
# least 16 characters (it used to fall back to a known all-zero key). An
# install that never set one would crash-loop after this update, so give it
# one first. Credentials saved under the old fallback must be re-entered;
# the app says so for each one.
ensure_encryption_key() {
  local key
  key="$(sed -n 's/^ENCRYPTION_KEY=//p' .env 2>/dev/null | tail -1)"
  [ "${#key}" -ge 16 ] && return 0
  set_env ENCRYPTION_KEY "$(head -c 32 /dev/urandom | base64 | tr -d '\n')"
  echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) generated ENCRYPTION_KEY (none was set; re-enter saved connector credentials and API keys)" >> "$1/history.log"
}

set_env() {
  if grep -q "^$1=" .env 2>/dev/null; then
    sed -i.bak "s#^$1=.*#$1=$2#" .env && rm -f .env.bak
  else
    echo "$1=$2" >> .env
  fi
}

# --- HTTPS (Settings → HTTPS) ----------------------------------------------
# POST /api/https writes update-status/https.json ({"enabled", "domain",
# "email"}). This validates it again (untrusted input), points .env at the
# domain, starts or removes the `caddy` service, and reports progress in
# update-status/https-status.json: off, pending (until the certificate
# answers), active, or error.
https_apply() {
  local dir="$1" req="$1/https.json" enabled domain email
  if [ -f "$req" ]; then
    enabled="$(json_field enabled "$req")"
    domain="$(json_field domain "$req")"
    email="$(json_field email "$req")"
    rm -f "$req"
    if [ "$enabled" != true ]; then
      sed -i.bak '/^COMPOSE_PROFILES=/d' .env && rm -f .env.bak
      if docker compose rm -sf caddy >/dev/null 2>"$dir/.https-error"; then
        write_https_status "$dir" off "" ""
      else
        write_https_status "$dir" error "" "could not stop caddy: $(tail -3 "$dir/.https-error")"
      fi
      return
    fi
    if ! valid_domain "$domain" || ! valid_email "$email"; then
      write_https_status "$dir" error "" "refused: invalid domain or email"
      return
    fi
    set_env EUNOMIA_DOMAIN "$domain"
    set_env EUNOMIA_ACME_EMAIL "$email"
    set_env COMPOSE_PROFILES https
    if ! docker compose up -d caddy >/dev/null 2>"$dir/.https-error"; then
      write_https_status "$dir" error "$domain" "could not start caddy: $(tail -3 "$dir/.https-error")"
      return
    fi
    write_https_status "$dir" pending "$domain" ""
  fi

  # Waiting for the certificate (after the request above, or an install with
  # --domain): probe until it answers. Through the caddy container itself, so
  # a router without NAT loopback doesn't make a working setup look broken.
  grep -qx 'COMPOSE_PROFILES=https' .env 2>/dev/null || return 0
  case "$(json_field state "$dir/https-status.json" 2>/dev/null)" in ""|pending) ;; *) return 0 ;; esac
  domain="$(sed -n 's/^EUNOMIA_DOMAIN=//p' .env | tail -1)"
  local target=127.0.0.1
  [ -f /.dockerenv ] && target=caddy
  command -v curl >/dev/null || apk add --no-cache -q curl >/dev/null 2>&1 # updater containers started before curl was added
  if curl -sS -o /dev/null --max-time 10 --connect-to "$domain:443:$target:443" "https://$domain/" 2>"$dir/.https-error"; then
    write_https_status "$dir" active "$domain" ""
  else
    write_https_status "$dir" pending "$domain" "no certificate yet. Check that $domain points at this machine and ports 80 and 443 are open; Caddy keeps retrying. ($(tail -1 "$dir/.https-error"))"
  fi
}

# json_field <key> <file>: a string/bool value from one-key-per-line JSON.
json_field() {
  sed -n "s/^ *\"$1\": *\"\{0,1\}\([^\",]*\)\"\{0,1\},\{0,1\} *$/\1/p" "$2" | head -1
}

# A public DNS name Let's Encrypt can issue for: dot-separated labels of
# letters/digits/hyphens ending in an alphabetic TLD (so no IPs).
valid_domain() {
  local label='[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?'
  [ "${#1}" -le 253 ] && [[ "$1" =~ ^($label\.)+[A-Za-z]{2,63}$ ]]
}

valid_email() {
  [ "${#1}" -le 254 ] && [[ "$1" =~ ^[A-Za-z0-9._%+-]{1,64}@(.+)$ ]] && valid_domain "${BASH_REMATCH[1]}"
}

# write_https_status <dir> <state> <domain> <message>
write_https_status() {
  local message=null
  if [ -n "$4" ]; then
    message="\"$(printf '%s' "$4" | tr '\n\r\t' '   ' | sed 's/\\/\\\\/g; s/"/\\"/g')\""
  fi
  cat > "$1/https-status.json.tmp" <<EOF
{
  "state": "$2",
  "domain": "$3",
  "message": $message,
  "checked_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF
  mv "$1/https-status.json.tmp" "$1/https-status.json"
}

# write_status <dir> <current> <latest> <applying> <error>
write_status() {
  local error=null
  if [ -n "$5" ]; then
    error="\"$(printf '%s' "$5" | tr '\n\r\t' '   ' | sed 's/\\/\\\\/g; s/"/\\"/g')\""
  fi
  cat > "$1/status.json.tmp" <<EOF
{
  "current_version": "$2",
  "latest_version": "$3",
  "update_available": $([ "$2" != "$3" ] && echo true || echo false),
  "checked_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "applying": $4,
  "error": $error
}
EOF
  mv "$1/status.json.tmp" "$1/status.json"
}

main "$@"
exit

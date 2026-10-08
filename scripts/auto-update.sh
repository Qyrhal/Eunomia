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

  local prev_ref
  prev_ref="$(git rev-parse HEAD 2>/dev/null)"
  if ! { git fetch --depth 1 origin tag "$latest" --quiet && git checkout --quiet "$latest"; } 2>"$status_dir/.last-error"; then
    write_status "$status_dir" "$current" "$latest" false "$(cat "$status_dir/.last-error")"
    exit 1
  fi

  local had_tag=false
  if grep -q '^EUNOMIA_IMAGE_TAG=' .env; then
    had_tag=true
    sed -i.bak "s#^EUNOMIA_IMAGE_TAG=.*#EUNOMIA_IMAGE_TAG=$latest#" .env && rm -f .env.bak
  else
    echo "EUNOMIA_IMAGE_TAG=$latest" >> .env
  fi
  # Installs from before nightly backups have no backup key yet.
  if ! grep -q '^BACKUP_ENCRYPTION_KEY=.' .env; then
    sed -i.bak '/^BACKUP_ENCRYPTION_KEY=/d' .env && rm -f .env.bak
    echo "BACKUP_ENCRYPTION_KEY=$(head -c 32 /dev/urandom | base64 | tr -d '\n')" >> .env
  fi
  # A release that pins SurrealDB 3.x over a running 2.x needs its data moved
  # first (export, fresh volume, verify; see docs/upgrading-to-surrealdb-3.md).
  # The script rolls itself back on failure, so here we only undo the checkout
  # to let "Update now" be retried. It can run for a long time: keep the lock fresh.
  local new_major old_major
  new_major="$(docker compose config --images 2>/dev/null | sed -n 's#^surrealdb/surrealdb:v\{0,1\}\([0-9][0-9]*\)\..*#\1#p' | head -1)"
  old_major="$(docker inspect -f '{{.Config.Image}}' "$(docker compose ps -q surrealdb 2>/dev/null | head -1)" 2>/dev/null | sed -n 's#.*:v\{0,1\}\([0-9][0-9]*\)\..*#\1#p')"
  if [ "$new_major" = 3 ] && [ "$old_major" = 2 ]; then
    ( while sleep 60; do touch "$status_dir/.lock"; done ) & local keepalive=$!; disown "$keepalive"
    bash scripts/upgrade-surreal-v3.sh >> "$status_dir/upgrade.log" 2>&1; local rc=$?
    kill "$keepalive" 2>/dev/null
    if [ "$rc" -ne 0 ]; then
      git checkout --quiet "$prev_ref" 2>/dev/null
      # only write the old tag back if .env had one: "unknown" or a git tag would break the next pull
      if $had_tag; then sed -i.bak "s#^EUNOMIA_IMAGE_TAG=.*#EUNOMIA_IMAGE_TAG=$current#" .env; else sed -i.bak '/^EUNOMIA_IMAGE_TAG=/d' .env; fi
      rm -f .env.bak
      write_status "$status_dir" "$current" "$latest" false "SurrealDB 3 upgrade failed and was rolled back, nothing changed: $(tail -3 "$status_dir/upgrade.log")"
      exit 1
    fi
  fi

  # Installs from before the key was required ran with an empty ENCRYPTION_KEY, so their stored
  # credentials and org database passwords were written under it. Give them a real key and let the
  # backend still read the old values (ENCRYPTION_KEY_LEGACY_EMPTY=1); new writes use the new key.
  # See docs/deployment.md, "Rotating ENCRYPTION_KEY". An existing key is never replaced.
  # Done only after the SurrealDB upgrade step: a rolled-back update must leave .env able to run the old release.
  if ! grep -q '^ENCRYPTION_KEY=.' .env; then
    sed -i.bak -e '/^ENCRYPTION_KEY=/d' -e '/^ENCRYPTION_KEY_LEGACY_EMPTY=/d' .env && rm -f .env.bak
    echo "ENCRYPTION_KEY=$(head -c 32 /dev/urandom | base64 | tr -d '\n')" >> .env
    echo "ENCRYPTION_KEY_LEGACY_EMPTY=1" >> .env
  fi

  if ! grep -q '^JWT_SECRET=.' .env; then
    sed -i.bak '/^JWT_SECRET=/d' .env && rm -f .env.bak
    echo "JWT_SECRET=$(head -c 32 /dev/urandom | base64 | tr -d '\n')" >> .env
  fi

  # Everything except the updater itself (recreating it here would kill this
  # run); it is refreshed last, after the status is written.
  local services
  services="$(docker compose config --services 2>/dev/null | grep -vx updater | tr '\n' ' ')"
  if ! { docker compose pull backend frontend backup && docker compose up -d --remove-orphans $services; } >/dev/null 2>"$status_dir/.last-error"; then
    write_status "$status_dir" "$latest" "$latest" false "restart failed: $(tail -5 "$status_dir/.last-error")"
    exit 1
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

#!/bin/sh
# Entrypoint of the one-shot `surreal-upgrade` compose service. `surrealdb` waits for it to
# finish (service_completed_successfully) on every `docker compose up`, so the SurrealDB 2.x ->
# 3.x data move happens whoever runs `up`: an old release's updater that jumped straight to this
# release (it only checks out the tag and runs `docker compose up -d`), the installer, or a person.
#
# Nearly always a no-op that exits within a second: .env already records the data volume
# (SURREAL_DATA_VOLUME, written by scripts/upgrade-surreal-v3.sh on success), or there is no
# 2.x volume (a fresh install). Otherwise it runs scripts/upgrade-surreal-v3.sh in here, the
# same script the host-side hook and the installer run: it stops the old backend, takes the
# encrypted pre-v3 backup, copies the old volume (never modified), exports it with 2.7, imports
# into 3.3 on eunomia-surreal-data-v3, compares every table's row count and records the volume.
# Whichever of the two paths runs first wins; the other then finds the record and does nothing.
#
# Exit 0 lets surrealdb (3.3, on the -v3 volume) and the rest of the stack start. Any failure
# exits 1, so `up` stops before 3.3 or the new backend start: the script has already put the
# old SurrealDB and backend back, the old volume is untouched, and the log says what to do.
# It holds the docker socket (like the updater), publishes nothing and reads only .env.
set -u
log() { echo "[surreal-upgrade $(date -u +%FT%TZ)] $*"; }
envget() { sed -n "s/^$1=//p" /eunomia/.env 2>/dev/null | tail -1; }
envset() { # in place (keeps the file's owner): replace or append KEY=VALUE
  if grep -q "^$1=" /eunomia/.env 2>/dev/null; then
    tmp="$(sed "s#^$1=.*#$1=$2#" /eunomia/.env)" && printf '%s\n' "$tmp" > /eunomia/.env
  else
    printf '%s=%s\n' "$1" "$2" >> /eunomia/.env
  fi
}
label() { docker inspect "$HOSTNAME" --format "{{index .Config.Labels \"$1\"}}" 2>/dev/null; }

project="$(label com.docker.compose.project)"
[ -n "$project" ] || { log "ERROR: cannot read this container's compose project (is the docker socket mounted?)"; exit 1; }

# Moved already (here or by scripts/upgrade-surreal-v3.sh), or chosen by hand.
if [ -n "$(envget SURREAL_DATA_VOLUME)" ]; then log "data volume recorded in .env: nothing to do"; exit 0; fi
old="$(docker volume ls -q --filter "label=com.docker.compose.project=$project" --filter label=com.docker.compose.volume=eunomia-surreal-data | head -1)"
[ -n "$old" ] || { log "no 2.x data volume: nothing to do"; exit 0; }

host="$(label com.docker.compose.project.working_dir)"
[ -n "$host" ] || host="$(docker inspect "$HOSTNAME" --format '{{range .Mounts}}{{if eq .Destination "/eunomia"}}{{.Source}}{{end}}{{end}}')"
files="$(label com.docker.compose.project.config_files | tr , :)"
# The `docker compose` this `up` was started with (same files, same project), re-run in a few
# seconds by a short-lived sibling: for .env changes, which this `up` read before we ran. Then
# the status file goes, so the updater reports afresh instead of this run's "restart failed".
reup() {
  docker run -d --rm -v /var/run/docker.sock:/var/run/docker.sock -v "$host:$host" -w "$host" \
    -e COMPOSE_PROJECT_NAME="$project" -e COMPOSE_FILE="$files" docker:27-cli \
    sh -c "sleep 20 && docker compose up -d --remove-orphans && rm -f update-status/status.json" >/dev/null 2>&1 \
    && log "starting the stack again by itself in 20 seconds (or run: docker compose up -d)" \
    || log "run: docker compose up -d"
  exit 1
}

# This release refuses to move old data under an empty ENCRYPTION_KEY (v1.2 updaters never
# set one). Give it one that still reads the old values (see docs/deployment.md, "Rotating
# ENCRYPTION_KEY"); the backend of this `up` was configured before, hence the second run.
if [ -z "$(envget ENCRYPTION_KEY)" ]; then
  envset ENCRYPTION_KEY "$(head -c 32 /dev/urandom | base64 | tr -d '\n')"
  envset ENCRYPTION_KEY_LEGACY_EMPTY 1
  log "ENCRYPTION_KEY was empty: generated one in .env (old values stay readable); nothing else changed yet"
  reup
fi

# Never overwrite: a -v3 volume that already holds data without a record in .env is a state
# this step does not understand (a lost .env line?). Stop and say how to choose.
v3="$(docker volume ls -q --filter "label=com.docker.compose.project=$project" --filter label=com.docker.compose.volume=eunomia-surreal-data-v3 | head -1)"
if [ -n "$v3" ] && docker run --rm -v "$v3:/d:ro" busybox:1.36 sh -c 'ls -A /d | grep -q .'; then
  log "ERROR: both $old (2.x) and $v3 hold data, and .env does not say which is in use. Nothing was changed."
  log "If $v3 is the moved data, add SURREAL_DATA_VOLUME=eunomia-surreal-data-v3 to .env; to move again from $old, empty $v3 first. Then: docker compose up -d"
  exit 1
fi

log "found $old and no record of a move: moving SurrealDB 2.x data to 3.x (the app is offline meanwhile)"
apk add --no-cache -q bash coreutils >/dev/null 2>&1 || { log "ERROR: could not install bash (no network?). Nothing was changed."; exit 1; }
# compose resolves this project's paths (./update-status, the compose files) on the host, so the
# repo must sit at its host path in here too (as in scripts/updater.sh)
if [ "$host" != /eunomia ] && [ ! -e "$host" ]; then mkdir -p "$(dirname "$host")" && ln -s /eunomia "$host"; fi
owner="$(stat -c %u:%g /eunomia)"
mkdir -p /eunomia/update-status
( cd "$host" && SURREAL_UPGRADE_ONESHOT=1 EUNOMIA_DIR="$host" COMPOSE_PROJECT_NAME="$project" COMPOSE_FILE="$files" bash scripts/upgrade-surreal-v3.sh; echo $? > /tmp/rc ) 2>&1 \
  | tee -a /eunomia/update-status/upgrade.log
rc="$(cat /tmp/rc 2>/dev/null || echo 1)"
chown "$owner" /eunomia/.env /eunomia/update-status/upgrade.log 2>/dev/null
if [ "$rc" -ne 0 ]; then
  prev="$(sed -n 's/.*updated \(v[^ ]*\) -> .*/\1/p' /eunomia/update-status/history.log 2>/dev/null | tail -1)"
  log "ERROR: the move to SurrealDB 3 failed. Your data is untouched on $old, but the app is DOWN until you act."
  log "To fix the cause shown above and retry: docker compose up -d   (or click Update now)."
  log "To go back to the release you were on${prev:+ ($prev)}: git checkout ${prev:-<previous tag>} && sed -i 's/^EUNOMIA_IMAGE_TAG=.*/EUNOMIA_IMAGE_TAG=${prev:-<previous tag>}/' .env && docker compose up -d --remove-orphans"
  log "Log: update-status/upgrade.log"
  exit 1
fi
if [ -z "$(envget SURREAL_DATA_VOLUME)" ]; then
  # The script found nothing to move: 3.x already runs, or the old volume holds data a 2.x
  # server cannot open (a pre-release 3.x install). Record the volume 3.x is using, so it
  # stays in use (the default is the -v3 volume, which may be empty here).
  cur="$(docker ps -q --filter "label=com.docker.compose.project=$project" --filter label=com.docker.compose.service=surrealdb | head -1)"
  cur="$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/data"}}{{.Name}}{{end}}{{end}}' "$cur" 2>/dev/null)"
  case "$cur" in
    *_eunomia-surreal-data-v3) envset SURREAL_DATA_VOLUME eunomia-surreal-data-v3; log "already on the -v3 volume: recorded it in .env" ;;
    *) envset SURREAL_DATA_VOLUME eunomia-surreal-data
       log "$old does not hold 2.x data: recorded SURREAL_DATA_VOLUME=eunomia-surreal-data in .env so it stays in use"
       reup ;;
  esac
fi
log "done"

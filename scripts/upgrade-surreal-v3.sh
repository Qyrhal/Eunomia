#!/usr/bin/env bash
# Moves a self-hosted Eunomia stack from SurrealDB 2.x to 3.x. Idempotent: it
# exits 0 when the stack already runs 3.x (or the compose file does not pin 3.x).
# Called by scripts/auto-update.sh before `docker compose up`, or run by hand
# from the repo root: bash scripts/upgrade-surreal-v3.sh
#
# 3.x cannot read 2.x data and refuses downgrades, so the data is never
# upgraded in place:
#   1. stop the backend (nothing writes from here on)
#   2. encrypted backup (kept, "pre-v3-*"), taken by a one-off container of the NEW release's
#      backup image that runs the RUNNING server's own `surreal` binary (copied out of its
#      image), so no `backup` service has to exist yet and a 2.3 server is never exported
#      by a 3.x CLI
#   3. stop the old server and COPY its volume to a scratch volume; start a 2.7 server on the
#      copy (the old volume is never opened by a newer version, because 2.6 -> 2.7 cannot be
#      reverted in place); `surreal export --v3` from that copy, then rewrite the two
#      index kinds 3.x dropped (MTREE -> HNSW, multi-field FULLTEXT -> one per field)
#   4. start the 3.x image from the compose file on a NEW
#      volume (SURREAL_DATA_VOLUME=eunomia-surreal-data-v3), import
#   5. compare per-table record counts and the _migration ledger with 2.x
#   6. match: record the new volume in .env. Mismatch or any error: delete the
#      half-built new volume, start the old image on the OLD volume again, restart
#      the backend, exit 1.
# The old volume (eunomia-surreal-data) is never deleted. The unencrypted export
# lives in a temp dir that is shredded on exit.
#
# Environment: EUNOMIA_DIR (repo root), COMPOSE_PROJECT_NAME / COMPOSE_FILE (as
# docker compose), EUNOMIA_SURREAL_EXPORT_IMAGE (2.x server run on the copy of the data,
# and its CLI for the export; default surrealdb/surrealdb:v2.7.0, with `export --v3`).
set -uo pipefail
export PATH="$PATH:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin"

V3_VOLUME=eunomia-surreal-data-v3
EXPORT_IMAGE="${EUNOMIA_SURREAL_EXPORT_IMAGE:-surrealdb/surrealdb:v2.7.0}"
log() { echo "[upgrade-surreal-v3 $(date -u +%FT%TZ)] $*"; }
die() { log "ERROR: $*" >&2; exit 1; }

cd "${EUNOMIA_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}" || exit 1
compose() { docker compose "$@"; }

# value of $1: the environment first, then .env, then the default $2
envval() {
  local v="${!1:-}"
  [ -n "$v" ] || v="$(sed -n "s/^$1=//p" .env 2>/dev/null | tail -1)"
  echo "${v:-$2}"
}
setenv() { # setenv KEY VALUE (.env)
  touch .env
  if grep -q "^$1=" .env; then sed -i.bak "s#^$1=.*#$1=$2#" .env && rm -f .env.bak; else echo "$1=$2" >> .env; fi
}
major_of() { sed -n 's/.*:v\{0,1\}\([0-9][0-9]*\)\..*/\1/p' <<<"$1"; }

export SURREAL_USER SURREAL_PASS
SURREAL_USER="$(envval SURREAL_USER root)"; SURREAL_PASS="$(envval SURREAL_PASS root)"
NS="$(envval SURREAL_NS eunomia)"; DB="$(envval SURREAL_DB eunomia)"
ENDPOINT=http://surrealdb:8000
SCRATCH_HOST=v2scratch   # the 2.7 server on the copy of the data

# --- 1. what is running, what does the compose file want ---
cid="$(compose ps -q surrealdb 2>/dev/null | head -1)"
running=false
[ -n "$cid" ] && [ "$(docker inspect -f '{{.State.Running}}' "$cid" 2>/dev/null)" = true ] && running=true

target_image="$(compose config --images 2>/dev/null | grep '^surrealdb/surrealdb:' | head -1)"
[ -n "$target_image" ] || die "no surrealdb/surrealdb image found in the compose file"
if [ "$(major_of "$target_image")" != 3 ]; then
  log "compose file pins $target_image, not 3.x: nothing to do"; exit 0
fi
if ! $running; then
  [ "$(envval SURREAL_DATA_VOLUME '')" = "$V3_VOLUME" ] && { log "already on $V3_VOLUME: nothing to do"; exit 0; }
  die "surrealdb is not running. Start the stack on the old release (docker compose up -d), then re-run."
fi
old_image="$(docker inspect -f '{{.Config.Image}}' "$cid")"
old_major="$(major_of "$old_image")"
[ -n "$old_major" ] || old_major="$(docker exec "$cid" /surreal version 2>/dev/null | sed -n 's/^\([0-9][0-9]*\)\..*/\1/p')"
case "$old_major" in
  3) log "SurrealDB already runs 3.x ($old_image): nothing to do"; exit 0 ;;
  2) ;;
  *) die "cannot tell the running SurrealDB version (image $old_image)" ;;
esac
NET="$(docker inspect -f '{{range $k,$v := .NetworkSettings.Networks}}{{$k}}{{"\n"}}{{end}}' "$cid" | head -1)"
OLD_VOLUME="$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/data"}}{{.Name}}{{end}}{{end}}' "$cid")"
[ -n "$OLD_VOLUME" ] || die "cannot find the data volume of the running SurrealDB"
PROJECT="$(docker inspect -f '{{index .Config.Labels "com.docker.compose.project"}}' "$cid")"
bid="$(compose ps -q backend 2>/dev/null | head -1)"
base_compose="${COMPOSE_FILE:-docker-compose.yml}"
log "upgrading SurrealDB $old_image -> $target_image (network $NET)"

T="$(mktemp -d)"
phase=0   # 1 backend stopped, 2 old surrealdb stopped (3.x may be running)
DONE=0

shred_tmp() {
  find "$T" -type f -exec sh -c 'shred -u -f "$1" 2>/dev/null || rm -f "$1"' _ {} \;
  rm -rf "$T"
}
v3_volume_names() {
  docker volume ls -q --filter "label=com.docker.compose.volume=$V3_VOLUME" --filter "label=com.docker.compose.project=$PROJECT"
}
SCRATCH_VOLUME="${PROJECT}_eunomia-surreal-v2copy"; BIN_VOLUME="${PROJECT}_eunomia-surreal-oldbin"
SCRATCH_CTR="${PROJECT}-v2scratch"
cleanup_scratch() {
  docker rm -f "$SCRATCH_CTR" >/dev/null 2>&1
  docker volume rm "$SCRATCH_VOLUME" "$BIN_VOLUME" >/dev/null 2>&1
}
rollback() {
  log "ROLLING BACK: the old volume was not modified"
  if [ "$phase" -ge 2 ]; then
    compose rm -sf surrealdb >/dev/null 2>&1
    v3_volume_names | while read -r v; do docker volume rm "$v" >/dev/null 2>&1 && log "removed partial volume $v"; done
    printf 'services:\n  surrealdb:\n    image: %s\n' "$old_image" > "$T/old-image.yml"
    unset SURREAL_DATA_VOLUME
    COMPOSE_FILE="$base_compose:$T/old-image.yml" compose up -d --wait surrealdb >/dev/null 2>&1 \
      && log "old SurrealDB ($old_image) is running again on the old volume" \
      || log "COULD NOT restart the old SurrealDB. Run: docker compose up -d surrealdb  (with the old image $old_image)"
  fi
  [ -z "$bid" ] || docker start "$bid" >/dev/null 2>&1 && log "backend restarted"
}
on_exit() {
  if [ "$DONE" != 1 ] && [ "$phase" -ge 1 ]; then rollback; fi
  cleanup_scratch
  shred_tmp
}
trap on_exit EXIT
trap 'exit 1' INT TERM

# $1 image, stdin: SurrealQL, stdout: --json results
sqlq() { docker run --rm -i --network "$NET" -e SURREAL_USER -e SURREAL_PASS "$1" sql \
           --endpoint "$ENDPOINT" --ns "$NS" --db "$DB" --hide-welcome --json 2>/dev/null; }
# $1 image, rest: tables -> sorted "table:count" lines
count_tables() {
  local img="$1" q="RETURN {" t; shift
  for t; do q="$q \`$t\`: (SELECT count() FROM \`$t\` GROUP ALL)[0].count ?? 0,"; done
  echo "$q };" | sqlq "$img" | tr -d '[]{}" \n' | tr ',' '\n' | grep . | sort
}
ledger() { echo 'SELECT version, name, checksum FROM _migration ORDER BY version;' | sqlq "$1" | tr -d ' \n'; }

# --- 2. quiesce, then the encrypted backup ---
phase=1
if [ -n "$bid" ]; then compose stop backend >/dev/null || die "could not stop the backend"; fi
log "backend stopped; taking the encrypted pre-upgrade backup"
# The backup image comes from the NEW compose file (its tag is already in .env); no `backup` service
# needs to be running, or to have ever existed. It runs the old server's own CLI via SURREAL_BIN.
cleanup_scratch
bc="$(docker create -v "$BIN_VOLUME:/o" busybox:1.36 true)" || die "could not create the CLI volume"
oc="$(docker create "$old_image")" && docker cp "$oc:/surreal" "$T/surreal-old" >/dev/null && docker rm "$oc" >/dev/null \
  || die "could not copy the surreal binary out of $old_image"
docker cp "$T/surreal-old" "$bc:/o/surreal" >/dev/null; docker rm "$bc" >/dev/null
compose pull -q backup >/dev/null 2>&1 || compose build -q backup >/dev/null || die "could not get the backup image (pull and build both failed)"
compose run --rm --no-deps -T -v "$BIN_VOLUME:/opt/oldbin:ro" -e SURREAL_BIN=/opt/oldbin/surreal backup now pre-v3 \
  || die "backup failed (is BACKUP_ENCRYPTION_KEY set in .env?)"

# --- 3. copy the data, run 2.7 on the copy, export for 3.x and convert what 3.x dropped ---
phase=2
compose stop surrealdb >/dev/null || die "could not stop surrealdb"
log "copying volume $OLD_VOLUME to $SCRATCH_VOLUME and starting $EXPORT_IMAGE on the copy"
docker volume create "$SCRATCH_VOLUME" >/dev/null
docker run --rm -v "$OLD_VOLUME:/from:ro" -v "$SCRATCH_VOLUME:/to" busybox:1.36 cp -a /from/. /to/ || die "could not copy the data volume"
docker run -d --name "$SCRATCH_CTR" --network "$NET" --network-alias "$SCRATCH_HOST" --user root -v "$SCRATCH_VOLUME:/data" \
  --entrypoint /surreal "$EXPORT_IMAGE" start --user "$SURREAL_USER" --pass "$SURREAL_PASS" rocksdb:/data/eunomia.db >/dev/null \
  || die "could not start $EXPORT_IMAGE on the copy"
ENDPOINT="http://$SCRATCH_HOST:8000"
for i in $(seq 1 150); do
  docker run --rm --network "$NET" "$EXPORT_IMAGE" isready --endpoint "$ENDPOINT" >/dev/null 2>&1 && break
  [ "$i" -lt 150 ] || die "$EXPORT_IMAGE did not come up on the copy of the data (see: docker logs $SCRATCH_CTR)"
  sleep 2
done
log "exporting with $EXPORT_IMAGE export --v3"
# create + start -a instead of `run --rm`: with --rm, docker can drop the tail of a large stdout.
# A truncated export (last statement not closed by `;`) is retried, never imported.
export_once() {
  local c rc
  c="$(docker create --network "$NET" -e SURREAL_USER -e SURREAL_PASS "$EXPORT_IMAGE" export --log none \
       --endpoint "$ENDPOINT" --ns "$NS" --db "$DB" --v3 -)" || return 1
  docker start -a "$c" > "$T/export.surql"; rc=$?
  docker rm "$c" >/dev/null 2>&1
  [ "$rc" -eq 0 ] && tail -c 4096 "$T/export.surql" | grep . | tail -1 | grep -q ';$'
}
for attempt in 1 2 3; do export_once && break; log "export attempt $attempt was incomplete"; [ "$attempt" -lt 3 ] || die "export failed"; done
grep -q '^OPTION IMPORT;' "$T/export.surql" || die "export looks wrong (no OPTION IMPORT header)"
tables=($(sed -n 's/^DEFINE TABLE \([^ ]*\) .*/\1/p' "$T/export.surql" | tr -d '`'))
[ "${#tables[@]}" -gt 0 ] || die "export defines no tables"
awk '
  / MTREE DIMENSION / { sub(/ MTREE DIMENSION /, " HNSW DIMENSION "); sub(/ CAPACITY [0-9]+.*;$/, ";") }
  /^DEFINE INDEX [^ ]+ ON [^ ]+ FIELDS [^;]*, [^;]* FULLTEXT / {
    idx=$3; tbl=$5; rest=$0; sub(/^.* FULLTEXT /, "", rest)
    f=$0; sub(/^.* FIELDS /, "", f); sub(/ FULLTEXT .*$/, "", f)
    n=split(f, a, /, */)
    for (i=1; i<=n; i++) print "DEFINE INDEX " idx "_" a[i] " ON " tbl " FIELDS " a[i] " FULLTEXT " rest
    next
  }
  { print }' "$T/export.surql" > "$T/import.surql"

# --- 4. the 2.x truth: counts and ledger, taken from the copy with the backend stopped ---
counts2="$(count_tables "$EXPORT_IMAGE" "${tables[@]}")"
[ "$(wc -l <<<"$counts2")" -eq "${#tables[@]}" ] || die "could not count every table on 2.x"
has_ledger=false; printf '%s\n' "${tables[@]}" | grep -qx _migration && has_ledger=true
ledger2=""; $has_ledger && ledger2="$(ledger "$EXPORT_IMAGE")"
log "2.x counts: $(tr '\n' ' ' <<<"$counts2")"

# --- 5. swap in 3.x on a new volume and import ---
docker rm -f "$SCRATCH_CTR" >/dev/null 2>&1
ENDPOINT=http://surrealdb:8000
v3_volume_names | while read -r v; do docker volume rm "$v" >/dev/null 2>&1 && log "removed leftover volume $v from an earlier attempt"; done
export SURREAL_DATA_VOLUME="$V3_VOLUME"
compose up -d --wait surrealdb >/dev/null || die "SurrealDB $target_image did not start on the new volume"
echo "DEFINE NAMESPACE IF NOT EXISTS \`$NS\`; USE NS \`$NS\`; DEFINE DATABASE IF NOT EXISTS \`$DB\`;" | docker run --rm -i --network "$NET" \
  -e SURREAL_USER -e SURREAL_PASS "$target_image" sql --endpoint "$ENDPOINT" --hide-welcome >/dev/null 2>&1
log "importing into $target_image"
ic="$(docker create --network "$NET" -e SURREAL_USER -e SURREAL_PASS "$target_image" import \
      --endpoint "$ENDPOINT" --ns "$NS" --db "$DB" /tmp/import.surql)" || die "could not create the import container"
docker cp "$T/import.surql" "$ic:/tmp/import.surql" >/dev/null && docker start -a "$ic"
rc=$?; docker rm "$ic" >/dev/null 2>&1
[ "$rc" -eq 0 ] || die "import failed"
# test hook: SurrealQL run on 3.x after the import (used to fake a mismatch)
[ -z "${UPGRADE_TEST_SQL:-}" ] || echo "$UPGRADE_TEST_SQL" | sqlq "$target_image" >/dev/null

# --- 6. verify ---
counts3="$(count_tables "$target_image" "${tables[@]}")"
log "3.x counts: $(tr '\n' ' ' <<<"$counts3")"
[ "$counts2" = "$counts3" ] || die "record counts differ between 2.x and 3.x: $(diff <(echo "$counts2") <(echo "$counts3") | tr '\n' ' ')"
if $has_ledger; then
  [ -n "$ledger2" ] && [ "$ledger2" = "$(ledger "$target_image")" ] || die "_migration ledger differs between 2.x and 3.x"
fi

setenv SURREAL_DATA_VOLUME "$V3_VOLUME"
DONE=1
log "OK: ${#tables[@]} tables match; SurrealDB $target_image runs on volume $V3_VOLUME (recorded in .env)."
log "The old volume eunomia-surreal-data is untouched (rollback = keep it). Pre-upgrade backup: pre-v3-* in the eunomia-backups volume."
log "Start the rest of the stack: docker compose up -d"

#!/bin/sh
# Runs inside the `backup` compose service (and the restore drill).
#   eunomia-backup loop              nightly at 03:00 local time (TZ), forever
#   eunomia-backup now [prefix]      one encrypted export now (prefix: manual|daily|weekly)
#   eunomia-backup list              list backup files
#   eunomia-backup restore FILE [--wipe]   FILE is a name in /backups or a path
# Files are `surreal export` output encrypted with AES-256-CBC (PBKDF2) using
# BACKUP_ENCRYPTION_KEY. Decrypt by hand:
#   openssl enc -d -aes-256-cbc -pbkdf2 -pass env:BACKUP_ENCRYPTION_KEY -in X.surql.enc
# With org tenancy (a `control` database exists) one backup is a directory, NAME/, holding one
# file per database of the namespace (control.surql.enc, org_<uuid>.surql.enc, and the old
# single database until you remove it). Without it (a pre-tenancy install, the restore drill) a
# backup is the single file NAME.surql.enc as before.
set -eu

ENDPOINT="${SURREAL_ENDPOINT:-http://surrealdb:8000}"
USER_="${SURREAL_USER:-root}"
PASS_="${SURREAL_PASS:-root}"
NS="${SURREAL_NS:-eunomia}"
DB="${SURREAL_DB:-eunomia}"
DIR="${BACKUP_DIR:-/backups}"
KEEP_DAILY="${KEEP_DAILY:-14}"
KEEP_WEEKLY="${KEEP_WEEKLY:-8}"

log() { echo "[backup $(date -u +%FT%TZ)] $*"; }
die() { log "ERROR: $*" >&2; exit 1; }

need_key() {
  [ -n "${BACKUP_ENCRYPTION_KEY:-}" ] || die "BACKUP_ENCRYPTION_KEY is empty. Set it in .env (generate one with: openssl rand -base64 32), then restart the backup service. Refusing to write unencrypted backups."
}


export_db() { # export one database to stdout
  surreal export --log none --endpoint "$ENDPOINT" --user "$USER_" --pass "$PASS_" --ns "$NS" --db "$1" -
}

# The databases to back up: control, every org_<uuid> and $DB (the old single database) when the
# tenancy layout is there; just $DB otherwise.
list_dbs() {
  info="$(echo 'INFO FOR NS;' | surreal sql --endpoint "$ENDPOINT" --user "$USER_" --pass "$PASS_" --ns "$NS" --hide-welcome --json 2>/dev/null || true)"
  if ! echo "$info" | grep -q '"control"'; then echo "$DB"; return; fi
  { echo "$info" | grep -oE '"(control|org_[0-9a-f]{32})"[[:space:]]*:' | sed -E 's/^"([^"]+)".*/\1/'
    echo "$info" | grep -q "\"$DB\"[[:space:]]*:" && echo "$DB"; } | sort -u
}

encrypt_to() { # stdin -> $1, atomically; refuses an empty export
  tmp="$1.part"
  openssl enc -aes-256-cbc -pbkdf2 -salt -pass env:BACKUP_ENCRYPTION_KEY > "$tmp" || { rm -f "$tmp"; return 1; }
  # Guard against pipe failures that sh cannot see: an empty export still encrypts to ~32 bytes.
  [ "$(wc -c < "$tmp")" -gt 64 ] || { rm -f "$tmp"; return 1; }
  mv "$tmp" "$1"
}

do_backup() {
  prefix="${1:-manual}"
  need_key
  mkdir -p "$DIR"
  name="$prefix-$(date -u +%Y%m%dT%H%M%SZ)"
  dbs="$(list_dbs)"
  # export | encrypt in one pipe: the plaintext dump never touches disk. --log none keeps log lines out of stdout (they would corrupt the dump).
  if [ "$dbs" = "$DB" ]; then
    out="$DIR/$name.surql.enc"
    export_db "$DB" | encrypt_to "$out" || { rm -f "$out"; die "export failed (is SurrealDB reachable at $ENDPOINT and are the credentials right?)"; }
  else
    out="$DIR/$name"
    mkdir -p "$out.part"
    for d in $dbs; do
      export_db "$d" | encrypt_to "$out.part/$d.surql.enc" || { rm -rf "$out.part"; die "export of database $d failed (is SurrealDB reachable at $ENDPOINT and are the credentials right?)"; }
    done
    mv "$out.part" "$out"
  fi
  log "wrote $out ($(du -sk "$out" | cut -f1) KB, databases: $(echo $dbs))"
  rotate daily "$KEEP_DAILY"
  rotate weekly "$KEEP_WEEKLY"
}

rotate() { # keep the newest $2 backups (files or directories) named $1-*
  ls -1d "$DIR"/"$1"-* 2>/dev/null | grep -v '\.part$' | sort -r | tail -n +"$(($2 + 1))" | while read -r f; do
    rm -rf "$f"; log "rotated out $f"
  done
}

restore_one() { # restore_one FILE DB [--wipe]
  plain="$(mktemp)"; trap 'rm -f "$plain"' EXIT
  openssl enc -d -aes-256-cbc -pbkdf2 -pass env:BACKUP_ENCRYPTION_KEY -in "$1" > "$plain" 2>/dev/null \
    || die "could not decrypt $1: wrong BACKUP_ENCRYPTION_KEY or corrupt file"
  if [ "${3:-}" = "--wipe" ]; then
    log "wiping database $NS/$2"
    echo "REMOVE DATABASE IF EXISTS \`$2\`;" | surreal sql --endpoint "$ENDPOINT" --user "$USER_" --pass "$PASS_" --ns "$NS" --hide-welcome >/dev/null \
      || die "wipe failed"
  fi
  echo "DEFINE DATABASE IF NOT EXISTS \`$2\`;" | surreal sql --endpoint "$ENDPOINT" --user "$USER_" --pass "$PASS_" --ns "$NS" --hide-welcome >/dev/null \
    || die "could not create database $2"
  surreal import --endpoint "$ENDPOINT" --user "$USER_" --pass "$PASS_" --ns "$NS" --db "$2" "$plain" \
    || die "import of $2 failed. If the database already has data, retry with --wipe (this deletes it first)."
  rm -f "$plain"
}

do_restore() {
  need_key
  f="${1:-}"; [ -n "$f" ] || die "usage: restore NAME [--wipe]"
  [ -e "$f" ] || f="$DIR/$f"
  [ -e "$f" ] || die "no such backup: $1 (try: list)"
  if [ -d "$f" ]; then
    for file in "$f"/*.surql.enc; do
      d="$(basename "$file" .surql.enc)"
      restore_one "$file" "$d" "${2:-}"
    done
  else
    restore_one "$f" "$DB" "${2:-}"
  fi
  log "restored $f"
}

secs_to_next_3am() {
  now="$(date +%s)"; t="$(date -d 03:00 +%s)"
  [ "$t" -gt "$now" ] || t="$(date -d 'tomorrow 03:00' +%s)"
  echo $((t - now))
}

case "${1:-}" in
  loop)
    need_key
    log "nightly backups to $DIR at 03:00 (TZ=${TZ:-UTC}), keeping $KEEP_DAILY daily + $KEEP_WEEKLY weekly"
    while :; do
      sleep "$(secs_to_next_3am)"
      do_backup daily || log "nightly backup failed, will retry tomorrow"
      [ "$(date +%u)" = 7 ] && { ls -1dt "$DIR"/daily-* | head -1 | while read -r f; do
        cp -R "$f" "$DIR/weekly-${f##*/daily-}"; done; rotate weekly "$KEEP_WEEKLY"; }
    done ;;
  now) do_backup "${2:-manual}" ;;
  list) ls -lh "$DIR" ;;
  restore) shift; do_restore "$@" ;;
  *) die "usage: eunomia-backup loop | now [prefix] | list | restore NAME [--wipe]" ;;
esac

#!/bin/sh
# Runs inside the `backup` compose service (and the restore drill).
#   eunomia-backup loop              nightly at 03:00 local time (TZ), forever
#   eunomia-backup now [prefix]      one encrypted export now (prefix: manual|daily|weekly)
#   eunomia-backup list              list backup files
#   eunomia-backup restore FILE [--wipe]   FILE is a name in /backups or a path
# Files are `surreal export` output encrypted with AES-256-CBC (PBKDF2) using
# BACKUP_ENCRYPTION_KEY. Decrypt by hand:
#   openssl enc -d -aes-256-cbc -pbkdf2 -pass env:BACKUP_ENCRYPTION_KEY -in X.surql.enc
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


do_backup() {
  prefix="${1:-manual}"
  need_key
  mkdir -p "$DIR"
  out="$DIR/$prefix-$(date -u +%Y%m%dT%H%M%SZ).surql.enc"
  tmp="$out.part"
  # export | encrypt in one pipe: the plaintext dump never touches disk. --log none keeps log lines out of stdout (they would corrupt the dump).
  if ! { surreal export --log none --endpoint "$ENDPOINT" --user "$USER_" --pass "$PASS_" --ns "$NS" --db "$DB" - \
         | openssl enc -aes-256-cbc -pbkdf2 -salt -pass env:BACKUP_ENCRYPTION_KEY > "$tmp"; }; then
    rm -f "$tmp"; die "export failed (is SurrealDB reachable at $ENDPOINT and are the credentials right?)"
  fi
  # Guard against pipe failures that sh cannot see: an empty export still encrypts to ~32 bytes.
  [ "$(wc -c < "$tmp")" -gt 64 ] || { rm -f "$tmp"; die "export looks empty, discarding"; }
  mv "$tmp" "$out"
  log "wrote $out ($(wc -c < "$out") bytes)"
  rotate daily "$KEEP_DAILY"
  rotate weekly "$KEEP_WEEKLY"
}

rotate() { # keep the newest $2 files named $1-*.surql.enc
  ls -1 "$DIR"/"$1"-*.surql.enc 2>/dev/null | sort -r | tail -n +"$(($2 + 1))" | while read -r f; do
    rm -f "$f"; log "rotated out $f"
  done
}

do_restore() {
  need_key
  f="${1:-}"; [ -n "$f" ] || die "usage: restore FILE [--wipe]"
  [ -f "$f" ] || f="$DIR/$f"
  [ -f "$f" ] || die "no such backup: $1 (try: list)"
  plain="$(mktemp)"; trap 'rm -f "$plain"' EXIT
  openssl enc -d -aes-256-cbc -pbkdf2 -pass env:BACKUP_ENCRYPTION_KEY -in "$f" > "$plain" 2>/dev/null \
    || die "could not decrypt $f: wrong BACKUP_ENCRYPTION_KEY or corrupt file"
  if [ "${2:-}" = "--wipe" ]; then
    log "wiping database $NS/$DB"
    echo "REMOVE DATABASE IF EXISTS $DB;" | surreal sql --endpoint "$ENDPOINT" --user "$USER_" --pass "$PASS_" --ns "$NS" --hide-welcome >/dev/null \
      || die "wipe failed"
  fi
  surreal import --endpoint "$ENDPOINT" --user "$USER_" --pass "$PASS_" --ns "$NS" --db "$DB" "$plain" \
    || die "import failed. If the database already has data, retry with --wipe (this deletes it first)."
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
      [ "$(date +%u)" = 7 ] && { ls -1t "$DIR"/daily-*.surql.enc | head -1 | while read -r f; do
        cp "$f" "$DIR/weekly-${f##*/daily-}"; done; rotate weekly "$KEEP_WEEKLY"; }
    done ;;
  now) do_backup "${2:-manual}" ;;
  list) ls -lh "$DIR" ;;
  restore) shift; do_restore "$@" ;;
  *) die "usage: eunomia-backup loop | now [prefix] | list | restore FILE [--wipe]" ;;
esac

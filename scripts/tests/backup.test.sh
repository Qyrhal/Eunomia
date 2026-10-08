#!/usr/bin/env bash
# Tests backend/scripts/backup/eunomia-backup.sh against a `surreal` shim: a pre-tenancy install is
# backed up as one file, a tenancy install (a `control` database exists) as a directory with one file
# per database, restores replay every file into its own database, and rotation keeps the newest N of both.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$HERE/../../backend/scripts/backup/eunomia-backup.sh"
PASS=0; FAIL=0
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
check() { local d="$1"; shift; if "$@" >/dev/null 2>&1; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); echo "FAIL: $d"; fi; }

mkdir -p "$TMP/bin" "$TMP/backups"
cat > "$TMP/bin/surreal" <<'SHIM'
#!/bin/sh
cmd="$1"; shift
db=""; file=""
while [ $# -gt 0 ]; do case "$1" in --db) db="$2"; shift ;; --ns|--endpoint|--user|--pass|--log) shift ;; --*) ;; *) file="$1" ;; esac; shift; done
case "$cmd" in
  export) echo "OPTION IMPORT; -- dump of $db -- padding padding padding padding padding padding padding padding" ;;
  import) echo "import $db: $(cat "$file")" >> "$SHIM_LOG" ;;
  sql) in="$(cat)"; echo "sql: $in" >> "$SHIM_LOG"
       case "$in" in
         "INFO FOR NS;") if [ -n "${MOCK_TENANCY:-}" ]; then
              echo '[{"databases":{"control":"DEFINE DATABASE control","eunomia":"DEFINE DATABASE eunomia","org_0123456789abcdef0123456789abcdef":"DEFINE DATABASE org_0123456789abcdef0123456789abcdef"}}]'
            else echo '[{"databases":{"eunomia":"DEFINE DATABASE eunomia"}}]'; fi ;;
       esac ;;
esac
SHIM
chmod +x "$TMP/bin/surreal"
export PATH="$TMP/bin:$PATH" SHIM_LOG="$TMP/shim.log" BACKUP_DIR="$TMP/backups" BACKUP_ENCRYPTION_KEY=testkey SURREAL_NS=eunomia SURREAL_DB=eunomia
bk() { sh "$SCRIPT" "$@"; }
# decrypt a new-format file by restoring it through the script (the shim logs the import)
dump_of() { : > "$SHIM_LOG"; sh "$SCRIPT" restore "$1" >/dev/null 2>&1; cat "$SHIM_LOG"; }

# 1. a pre-tenancy install: one file, as before
bk now manual >/dev/null
f="$(ls "$TMP"/backups/manual-*.surql.enc | head -1)"
check "single-database install writes one file" test -f "$f"
check "that file holds the database export" sh -c "grep -q 'dump of eunomia' <<< \"\$(sh '$SCRIPT' restore '$f' >/dev/null 2>&1; cat '$SHIM_LOG')\""
check "new files carry the authenticated header" sh -c "head -n1 '$f' | grep -q '^EUNOMIA-BK2 '"
check "round trip restores the exact dump" sh -c ": > '$SHIM_LOG'; sh '$SCRIPT' restore '$f' && grep -q 'import eunomia: OPTION IMPORT; -- dump of eunomia' '$SHIM_LOG'"
cp "$f" "$TMP/tampered.surql.enc"; printf 'X' | dd of="$TMP/tampered.surql.enc" bs=1 seek=60 conv=notrunc 2>/dev/null
check "a tampered file is refused before import" sh -c ": > '$SHIM_LOG'; sh '$SCRIPT' restore '$TMP/tampered.surql.enc' 2>&1 | grep -q 'integrity check' && ! grep -q import '$SHIM_LOG'"
printf 'legacy dump of eunomia\n' | openssl enc -aes-256-cbc -pbkdf2 -salt -pass env:BACKUP_ENCRYPTION_KEY > "$TMP/legacy.surql.enc"
check "an old-format file still restores" sh -c ": > '$SHIM_LOG'; sh '$SCRIPT' restore '$TMP/legacy.surql.enc' && grep -q 'legacy dump of eunomia' '$SHIM_LOG'"

# 2. a tenancy install: a directory with a file per database (control, each org, the old database)
rm -rf "$TMP/backups"/*; sleep 1
MOCK_TENANCY=1 bk now manual >/dev/null
d="$(ls -d "$TMP"/backups/manual-* | head -1)"
check "tenancy install writes a directory" test -d "$d"
check "control, the org and the old database are each backed up" test "$(ls "$d" | sort | tr '\n' ' ')" = "control.surql.enc eunomia.surql.enc org_0123456789abcdef0123456789abcdef.surql.enc "
check "no half-written .part is left" test -z "$(ls -d "$TMP"/backups/*.part 2>/dev/null)"

# 3. restoring the directory imports every database into its own database
: > "$SHIM_LOG"
bk restore "$(basename "$d")" --wipe >/dev/null
check "each database is wiped, created and imported" sh -c "grep -c 'import ' '$SHIM_LOG' | grep -qx 3 && grep -q 'REMOVE DATABASE IF EXISTS .control.' '$SHIM_LOG' && grep -q 'import control: OPTION IMPORT; -- dump of control' '$SHIM_LOG'"
check "a wrong key refuses clearly" sh -c "BACKUP_ENCRYPTION_KEY=nope sh '$SCRIPT' restore '$(basename "$d")' 2>&1 | grep -q 'integrity check'"

# 3b. no BACKUP_ENCRYPTION_KEY: a key is generated once into the backup volume and reused
rm -rf "$TMP/backups"/*
out1="$(unset BACKUP_ENCRYPTION_KEY; bk now manual 2>&1)"
case "$out1" in *"COPY IT SOMEWHERE SAFE"*) PASS=$((PASS + 1)) ;; *) FAIL=$((FAIL + 1)); echo "FAIL: first start generates a key and says so";; esac
check "the key file exists and is private" test -s "$TMP/backups/.backup-key" -a "$(ls -l "$TMP/backups/.backup-key" | cut -c1-10)" = "-rw-------"
sleep 1
out2="$(unset BACKUP_ENCRYPTION_KEY; bk now manual 2>&1)"
case "$out2" in *"COPY IT"*) FAIL=$((FAIL + 1)); echo "FAIL: the notice is printed only once";; *) PASS=$((PASS + 1));; esac
g="$(ls "$TMP"/backups/manual-*.surql.enc | head -1)"
check "generated-key backups restore with the key file" sh -c "unset BACKUP_ENCRYPTION_KEY; sh '$SCRIPT' restore '$g'"

# 4. rotation keeps the newest N, files and directories alike
rm -rf "$TMP/backups"/*
for s in 1 2 3 4; do mkdir "$TMP/backups/daily-2026010${s}T000000Z"; done; touch "$TMP/backups/daily-20260105T000000Z.surql.enc"
KEEP_DAILY=2 MOCK_TENANCY=1 bk now daily >/dev/null
check "only the newest two daily backups stay" test "$(ls -1d "$TMP"/backups/daily-* | wc -l | tr -d ' ')" = 2

echo "backup.test.sh: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]

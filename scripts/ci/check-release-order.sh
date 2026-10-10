#!/usr/bin/env bash
# Refuses a release whose docker-compose.yml pins SurrealDB 3.x unless 2.x data cannot reach a
# 3.x server: either the tag migrates by itself at `docker compose up` (its compose file has the
# one-shot surreal-upgrade service that surrealdb waits for), or the PREVIOUS v* tag already ships
# the updater hook (a bridge release, still pinning 2.x). "Update now" runs the OLD release's
# updater, which only checks out the tag and runs `docker compose up`, so without either a 3.x
# pin starts 3.x on 2.x data. Usage: scripts/ci/check-release-order.sh <tag>   (run in a full clone).
# Also refuses a tag whose compose pin and backend SDK majors differ (SDK 3.x refuses a 2.x
# server and the reverse), which is what a bridge cut from `foundation` would be.
# Exit 0: fine. Exit 1: refuse to tag. See docs/releasing.md.
set -uo pipefail
tag="${1:?usage: $0 <tag>}"
pins() { sed -n 's#.*image: surrealdb/surrealdb:v\{0,1\}\([0-9][0-9]*\)\..*#\1#p' | head -1; }
pin="$(git show "$tag:docker-compose.yml" | pins)"
sdk="$(git show "$tag:backend/Cargo.toml" 2>/dev/null | sed -n 's#^surrealdb *=.*version = "=\{0,1\}\([0-9][0-9]*\)\..*#\1#p' | head -1)"
if [ -n "$sdk" ] && [ -n "$pin" ] && [ "$sdk" != "$pin" ]; then
  echo "REFUSED: $tag pins SurrealDB $pin.x in docker-compose.yml but backend/Cargo.toml needs the $sdk.x SDK: the backend would crash-loop."
  echo "A bridge release must be cut from the last 2.x-compatible line, not from foundation (docs/releasing.md)."
  exit 1
fi
[ "$pin" = 3 ] || { echo "$tag does not pin SurrealDB 3.x: no bridge needed"; exit 0; }
if git show "$tag:docker-compose.yml" | grep -q '^  surreal-upgrade:' \
  && git show "$tag:docker-compose.yml" | grep -A2 '^      surreal-upgrade:' | grep -q 'condition: service_completed_successfully' \
  && git cat-file -e "$tag:scripts/surreal-upgrade-service.sh" 2>/dev/null; then
  echo "ok: $tag moves 2.x data itself at docker compose up (surreal-upgrade service): no bridge needed"; exit 0
fi
# newest v* tag strictly below $tag in version order
prev="$( { git tag -l 'v*' | grep -vx "$tag"; echo "$tag"; } | sort -V | grep -x -B1 "$tag" | head -1 | grep -vx "$tag")"
[ -n "$prev" ] || { echo "REFUSED: $tag pins SurrealDB 3.x and has no previous release tag to act as the bridge"; exit 1; }
if git cat-file -e "$prev:scripts/upgrade-surreal-v3.sh" 2>/dev/null \
  && git show "$prev:scripts/auto-update.sh" 2>/dev/null | grep -q 'upgrade-surreal-v3.sh'; then
  echo "ok: previous release $prev already ships the upgrade hook"; exit 0
fi
echo "REFUSED: $tag pins SurrealDB 3.x but the previous release $prev has no SurrealDB 3 upgrade hook."
echo "Ship a bridge release first (hook, still pinning 2.x), let installs take it, then tag the 3.x release."
exit 1

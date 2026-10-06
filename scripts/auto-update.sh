#!/usr/bin/env bash
# Self-update check/apply, run on the HOST (never inside a container) via
# cron or a systemd timer -- see docs/deployment.md's "Auto-update" section.
#
# Why host-side: the backend container has no git, no docker CLI, and no
# access to the docker socket. Giving a web-facing container control of the
# docker socket would mean any RCE in it compromises the whole host; running
# this on the host instead keeps that privileged step at the same trust
# level the operator already has (they're the one who runs `docker compose`
# by hand today).
#
# Writes $REPO/update-status/status.json every run (polled by
# GET /api/update/status), and applies an update -- `git pull --ff-only` +
# rebuild -- only when $REPO/update-status/requested exists (written by
# POST /api/update). Never auto-applies on its own; that was a deliberate
# product choice, not a technical limitation.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATUS_DIR="$REPO/update-status"
mkdir -p "$STATUS_DIR"

cd "$REPO"
git fetch origin main --quiet

LOCAL_SHA=$(git rev-parse HEAD)
REMOTE_SHA=$(git rev-parse origin/main)

write_status() {
  local applying="$1" error="${2:-}"
  cat > "$STATUS_DIR/status.json" <<EOF
{
  "local_sha": "$LOCAL_SHA",
  "remote_sha": "$REMOTE_SHA",
  "update_available": $([ "$LOCAL_SHA" != "$REMOTE_SHA" ] && echo true || echo false),
  "checked_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "applying": $applying,
  "error": $([ -n "$error" ] && printf '%s' "$error" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))' || echo null)
}
EOF
}

if [ ! -f "$STATUS_DIR/requested" ]; then
  write_status false
  exit 0
fi

write_status true
rm -f "$STATUS_DIR/requested"

# A dirty working tree (local edits, not just a stale checkout) can't be
# fast-forwarded safely -- fail loudly into status.json rather than let a
# mid-pull error (or an operator's global `pull.rebase` config turning this
# into a rebase, which also refuses on dirty trees) leave `applying: true`
# stuck forever with no explanation.
if ! git diff --quiet || ! git diff --cached --quiet; then
  write_status false "working tree has local changes -- commit or stash them, then click Update again"
  exit 1
fi

if ! git pull --ff-only --no-rebase origin main 2>"$STATUS_DIR/.last-error"; then
  write_status false "$(cat "$STATUS_DIR/.last-error")"
  exit 1
fi

if ! docker compose up -d --build backend frontend mcp 2>"$STATUS_DIR/.last-error"; then
  write_status false "rebuild failed: $(cat "$STATUS_DIR/.last-error")"
  exit 1
fi

rm -f "$STATUS_DIR/.last-error"
LOCAL_SHA=$(git rev-parse HEAD)
write_status false
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) updated to $LOCAL_SHA" >> "$STATUS_DIR/history.log"

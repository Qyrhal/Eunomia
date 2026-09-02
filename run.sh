#!/usr/bin/env bash
# Boots Eunomia: migrates the backend, then runs backend + frontend together.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

BLUE='\033[38;5;33m'
DIM='\033[2m'
BOLD='\033[1m'
GREEN='\033[38;5;35m'
RESET='\033[0m'

echo -e "${BLUE}${BOLD}"
cat <<'EOF'
  ______                           _
 |  ____|                         (_)
 | |__   _   _ _ __   ___  _ __ ___  _  __ _
 |  __| | | | | '_ \ / _ \| '_ ` _ \| |/ _` |
 | |____| |_| | | | | (_) | | | | | | | (_| |
 |______|\__,_|_| |_|\___/|_| |_| |_|_|\__,_|
EOF
echo -e "${RESET}${DIM}  your personal AI dashboard${RESET}\n"

cd backend

if [ ! -f .env ]; then
  echo -e "${DIM}no backend/.env — generating one with a fresh ENCRYPTION_KEY${RESET}"
  KEY=$(uv run python -c "from cryptography.fernet import Fernet; print(Fernet.generate_key().decode())")
  cat > .env <<EOF
SECRET_KEY=dev-secret-$(date +%s)
ENCRYPTION_KEY=${KEY}
EOF
fi

echo -e "${BOLD}==>${RESET} syncing backend deps (uv)"
uv sync --quiet

echo -e "${BOLD}==>${RESET} making migrations"
uv run manage.py makemigrations

echo -e "${BOLD}==>${RESET} applying migrations"
uv run manage.py migrate

cd ../frontend
if [ ! -d node_modules ]; then
  echo -e "${BOLD}==>${RESET} installing frontend deps (bun)"
  bun install
fi
if [ ! -f .env.local ]; then
  echo "NEXT_PUBLIC_API_URL=http://localhost:8000" > .env.local
fi

cd ..

pids=()
cleanup() {
  echo -e "\n${DIM}shutting down...${RESET}"
  kill "${pids[@]}" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo -e "${BOLD}==>${RESET} starting backend  ${DIM}http://localhost:8000${RESET}"
(cd backend && uv run manage.py runserver 8000) &
pids+=($!)

echo -e "${BOLD}==>${RESET} starting frontend ${DIM}http://localhost:3000${RESET}"
(cd frontend && bun run dev) &
pids+=($!)

echo -e "\n${GREEN}${BOLD}Eunomia is running${RESET} — ${BOLD}http://localhost:3000${RESET}  ${DIM}(ctrl-c to stop)${RESET}\n"

wait

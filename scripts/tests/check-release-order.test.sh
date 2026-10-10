#!/usr/bin/env bash
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; S="$HERE/../ci/check-release-order.sh"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT; cd "$T" && git init -q -b main
g() { git -c user.email=t@t -c user.name=t "$@"; }
PASS=0; FAIL=0; check() { local d="$1"; shift; if "$@" >/dev/null 2>&1; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); echo "FAIL: $d"; fi; }
mkdir scripts; echo '    image: surrealdb/surrealdb:v2.3' > docker-compose.yml; echo x > scripts/auto-update.sh
g add -A; g commit -qm a; g tag v1.0.0
echo '    image: surrealdb/surrealdb:v3.3.1' > docker-compose.yml; g commit -qam b; g tag v1.1.0
check "2.x pin passes" bash "$S" v1.0.0
check "3.x pin straight after a hookless release is refused" bash -c "! bash '$S' v1.1.0"
echo '    image: surrealdb/surrealdb:v2.3' > docker-compose.yml; echo 'bash scripts/upgrade-surreal-v3.sh' > scripts/auto-update.sh; echo y > scripts/upgrade-surreal-v3.sh
g add -A; g commit -qm c; g tag v1.2.0
echo '    image: surrealdb/surrealdb:v3.3.1' > docker-compose.yml; g commit -qam d; g tag v1.3.0
check "3.x pin after a bridge release passes" bash "$S" v1.3.0
# compose pin and backend SDK must agree
mkdir backend
cdep() { echo "surrealdb = { version = \"$1\", features = [\"kv-mem\"] }" > backend/Cargo.toml; }
echo '    image: surrealdb/surrealdb:v2.3' > docker-compose.yml; cdep '=3.3.1'; g add -A; g commit -qm e; g tag v1.4.0
check "2.x pin with a 3.x SDK backend is refused" bash -c "! bash '$S' v1.4.0"
echo '    image: surrealdb/surrealdb:v3.3.1' > docker-compose.yml; cdep '2.3'; g commit -qam f; g tag v1.5.0
check "3.x pin with a 2.x SDK backend is refused" bash -c "! bash '$S' v1.5.0"
cdep '=3.3.1'; g commit -qam h; g tag v1.6.0
check "3.x pin with a 3.x SDK backend passes (hook already in v1.2.0)" bash "$S" v1.6.0
echo '    image: surrealdb/surrealdb:v2.3' > docker-compose.yml; cdep '2.3'; g commit -qam i; g tag v1.7.0
check "2.x pin with a 2.x SDK backend passes" bash "$S" v1.7.0
# a 3.x release that migrates by itself at `up` needs no bridge before it
g checkout -q v1.0.0 2>/dev/null; g checkout -q -b selfup
printf 'services:\n  surrealdb:\n    image: surrealdb/surrealdb:v3.3.1\n    depends_on:\n      surreal-upgrade:\n        condition: service_completed_successfully\n  surreal-upgrade:\n    image: docker:27-cli\n' > docker-compose.yml
echo z > scripts/surreal-upgrade-service.sh; g add -A; g commit -qm j; g tag v2.0.0
check "3.x pin with its own surreal-upgrade step passes without a bridge" bash "$S" v2.0.0
sed -i.bak '/condition:/d' docker-compose.yml && rm -f docker-compose.yml.bak; g commit -qam k; g tag v2.0.1
check "a surreal-upgrade service surrealdb does not wait for is not enough" bash -c "! bash '$S' v2.0.1 | grep -q 'moves 2.x data itself'"
echo "release-order: $PASS passed, $FAIL failed"; [ "$FAIL" -eq 0 ]

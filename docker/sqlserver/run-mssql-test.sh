#!/usr/bin/env bash
# Avvia SQL Server "usa e getta", applica il seed e lancia il test d'integrazione
# `core/tests/sqlserver.rs` (path puro-Rust: nessun client SQL Server sull'host).
#
# Uso:   ./run-mssql-test.sh [--keep]
#          --keep   lascia il container attivo a fine test
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
SA_PW="Charon_Pw_2026"
KEEP=0
[ "${1:-}" = "--keep" ] && KEEP=1

step() { printf '\n\033[1;36m== %s\033[0m\n' "$1"; }
cleanup() { [ "$KEEP" = 1 ] || docker compose -f "$HERE/docker-compose.yml" down -v >/dev/null 2>&1 || true; }
trap cleanup EXIT

step "Avvio container SQL Server"
docker compose -f "$HERE/docker-compose.yml" up -d

step "Attendo che il server sia healthy"
for i in $(seq 1 40); do
  st=$(docker inspect -f '{{.State.Health.Status}}' charon-mssql 2>/dev/null || echo "?")
  [ "$st" = "healthy" ] && break
  sleep 3
done
[ "$st" = "healthy" ] || { echo "SQL Server non pronto in tempo"; docker logs charon-mssql | tail -20; exit 1; }

step "Seed dei database (charon_src + charon_full)"
docker exec -i charon-mssql /opt/mssql-tools18/bin/sqlcmd \
  -S localhost -U sa -P "$SA_PW" -C -b < "$HERE/init.sql"

step "Test d'integrazione (cargo test)"
CHARON_TEST_MSSQL=1 cargo test -p charon-core --manifest-path "$ROOT/Cargo.toml" \
  --test sqlserver -- --nocapture

step "OK"

#!/usr/bin/env bash
# Prova di Charon contro un Oracle "usa e getta" (Docker).
#
# ATTENZIONE alle differenze di portabilità rispetto a Postgres/SQL Server:
#   • Il path puro-Rust di Oracle richiede l'Oracle Instant Client A RUNTIME
#     (il crate `oracle`/ODPI-C lo carica via dlopen). NON è un binario unico.
#   • `sqlldr` (per `oracle-load`) fa parte dell'Instant Client "Tools".
# Vedi ../../PORTABILITY.md per i dettagli.
#
# Lo script è pensato per ESSERE LETTO ed eseguito quando hai a disposizione:
#   - Docker (per il DB)                          → sempre
#   - Oracle Instant Client (Basic + Tools)       → per le operazioni REALI
# Senza Instant Client puoi comunque vedere il DRY-RUN di oracle-load (legge i
# pacchetti e stampa i comandi sqlldr, senza toccare nulla).
#
# Uso:   ./run-oracle-test.sh [--keep]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
KEEP="${1:-}"

ORAHOST=127.0.0.1
ORAPORT=1521
ORASVC=FREEPDB1
export CHARON_PASSWORD=jform_pw
COMMON=(--engine oracle --host "$ORAHOST" --port "$ORAPORT" --db "$ORASVC" --user JFORM_DEV)

step() { printf '\n\033[1;36m== %s ==\033[0m\n' "$1"; }

# --- build: serve la feature oracle per il path puro-Rust (test/clone) --------
step "Build della CLI charon (--features oracle)"
cargo build -q -p charon-cli --features oracle --manifest-path "$ROOT/Cargo.toml"
CHARON="$ROOT/target/debug/charon"

# --- il DRY-RUN di oracle-load funziona SEMPRE (nessun DB, nessun client) -----
step "oracle-load — DRY RUN (pacchetto di esempio, nessun Oracle necessario)"
"$CHARON" "${COMMON[@]}" oracle-load --dir "$HERE/sample-package" --dry-run

# --- container ---------------------------------------------------------------
step "Avvio del container Oracle (primo avvio LENTO: scarica ed inizializza)"
docker compose -f "$HERE/docker-compose.yml" up -d
echo "Attendo 'DATABASE IS READY TO USE!' nei log del container…"
READY=0
for _ in $(seq 1 120); do
  if docker logs charon-ora 2>&1 | grep -q "DATABASE IS READY TO USE!"; then READY=1; break; fi
  sleep 5
done
[ "$READY" = 1 ] || { echo "Oracle non pronto in tempo"; docker logs charon-ora | tail -30; exit 1; }

cleanup() {
  if [ "$KEEP" != "--keep" ]; then
    step "Teardown"; docker compose -f "$HERE/docker-compose.yml" down -v
  else
    echo "(--keep) container attivo. Stop: docker compose -f $HERE/docker-compose.yml down -v"
  fi
}
trap cleanup EXIT

# --- da qui in poi serve l'Oracle Instant Client sull'host --------------------
if ! "$CHARON" "${COMMON[@]}" test >/dev/null 2>&1; then
  step "Instant Client assente → salto le operazioni reali"
  cat <<'EOF'
Il path puro-Rust di Oracle non ha trovato l'Instant Client (o la connessione è
fallita). Installa l'Instant Client "Basic" (+ "Tools" per sqlldr) e riprova:
  https://www.oracle.com/database/technologies/instant-client/downloads.html
Poi assicurati che le librerie siano in LD_LIBRARY_PATH (Linux) / PATH (Windows).
EOF
  exit 0
fi

step "1) charon test (pure-Rust, Instant Client presente)"
"$CHARON" "${COMMON[@]}" test

step "2) oracle-load — ESECUZIONE (sqlldr) + riallineo sequenze"
# Richiede sqlldr nel PATH (Instant Client Tools). Carica CUSTOMERS e ORDERS
# nell'ordine di load_order.txt, poi riallinea le sequenze identity.
"$CHARON" "${COMMON[@]}" oracle-load --dir "$HERE/sample-package"

step "3) verifica conteggi"
docker exec charon-ora bash -lc \
  "echo 'SELECT COUNT(*) FROM customers; SELECT COUNT(*) FROM orders;' | \
   sqlplus -s JFORM_DEV/jform_pw@localhost:1521/FREEPDB1"

step "FATTO ✅  (verifica i conteggi sopra: 3 customers, 4 orders)"

#!/usr/bin/env bash
# Prova end-to-end di Charon contro un Postgres "usa e getta" (Docker), usando
# SOLO il path puro-Rust: nessun client Postgres installato sull'host. Dimostra:
#   1. test connessione
#   2. clone "pieno"  (DROP+CREATE+dati)         src → charon_full
#   3. clone data-only + mascheramento + riallineo sequenze   src → charon_dst
# Ogni operazione viene prima mostrata in --dry-run e poi eseguita davvero.
#
# Uso:   ./run-postgres-test.sh [--keep]
#   --keep   non fermare il container al termine (per ispezionarlo a mano)
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
KEEP="${1:-}"

PGHOST=127.0.0.1
PGPORT=55432
export CHARON_PASSWORD=app_pw          # letta dal core, non finisce negli argomenti
COMMON=(--engine pg --host "$PGHOST" --port "$PGPORT" --user app --prefer rust)

step() { printf '\n\033[1;36m== %s ==\033[0m\n' "$1"; }
psql_dst() { docker exec charon-pg psql -U app -d "$1" -tAc "$2" | tr -d '[:space:]'; }

# --- build (una volta) -------------------------------------------------------
step "Build della CLI charon"
cargo build -q -p charon-cli --manifest-path "$ROOT/Cargo.toml"
CHARON="$ROOT/target/debug/charon"

# --- container ---------------------------------------------------------------
step "Avvio del container Postgres"
docker compose -f "$HERE/docker-compose.yml" up -d
# NB: pg_isready DENTRO il container passa già durante l'init (quando Postgres
# ascolta solo sul socket locale e sta ancora ricreando i DB): non basta. Il
# controllo affidabile è connettersi DA FUORI col vero client Charon.
echo -n "Attendo che il DB accetti connessioni TCP esterne"
READY=0
for _ in $(seq 1 40); do
  if "$CHARON" "${COMMON[@]}" --db charon_src test >/dev/null 2>&1; then READY=1; break; fi
  echo -n "."; sleep 1
done
echo ""
[ "$READY" = 1 ] || { echo "DB non pronto in tempo"; docker logs charon-pg | tail -20; exit 1; }

cleanup() {
  if [ "$KEEP" != "--keep" ]; then
    step "Teardown del container"
    docker compose -f "$HERE/docker-compose.yml" down -v
  else
    echo "(--keep) container lasciato attivo: docker compose -f $HERE/docker-compose.yml down -v"
  fi
}
trap cleanup EXIT

# --- 1) test connessione -----------------------------------------------------
step "1) charon test (sorgente)"
"$CHARON" "${COMMON[@]}" --db charon_src test

# --- 2) clone pieno ----------------------------------------------------------
step "2a) clone pieno — DRY RUN (src → charon_full)"
"$CHARON" "${COMMON[@]}" --db charon_src \
  --dst-host "$PGHOST" --dst-port "$PGPORT" --dst-user app --dst-db charon_full \
  clone --dry-run

step "2b) clone pieno — ESECUZIONE"
CHARON_DST_PASSWORD=app_pw "$CHARON" "${COMMON[@]}" --db charon_src \
  --dst-host "$PGHOST" --dst-port "$PGPORT" --dst-user app --dst-db charon_full \
  clone

C_SRC=$(psql_dst charon_src  "SELECT count(*) FROM customers")
C_FULL=$(psql_dst charon_full "SELECT count(*) FROM customers")
O_FULL=$(psql_dst charon_full "SELECT count(*) FROM orders")
echo "  customers: src=$C_SRC  full=$C_FULL   orders(full)=$O_FULL"
[ "$C_SRC" = "$C_FULL" ] || { echo "FALLITO: conteggi diversi"; exit 1; }

# --- 3) clone data-only + mask ----------------------------------------------
step "3a) clone data-only + mask email — DRY RUN (src → charon_dst)"
CHARON_DST_PASSWORD=app_pw "$CHARON" "${COMMON[@]}" --db charon_src \
  --dst-host "$PGHOST" --dst-port "$PGPORT" --dst-user app --dst-db charon_dst \
  clone --data-only --mask customers.email=email --dry-run

step "3b) clone data-only + mask email — ESECUZIONE"
CHARON_DST_PASSWORD=app_pw "$CHARON" "${COMMON[@]}" --db charon_src \
  --dst-host "$PGHOST" --dst-port "$PGPORT" --dst-user app --dst-db charon_dst \
  clone --data-only --mask customers.email=email

C_DST=$(psql_dst charon_dst "SELECT count(*) FROM customers")
EMAIL0=$(psql_dst charon_dst "SELECT email FROM customers ORDER BY id LIMIT 1")
echo "  customers(dst)=$C_DST   email[0] mascherata = $EMAIL0"
[ "$C_DST" = "$C_SRC" ] || { echo "FALLITO: data-only conteggi diversi"; exit 1; }
case "$EMAIL0" in
  *@example.com) [ "$EMAIL0" != "alice.rossi@example.com" ] || { echo "FALLITO: email non mascherata"; exit 1; } ;;
  *) echo "FALLITO: email non nel formato atteso"; exit 1 ;;
esac

# La sequenza identity deve essere stata riallineata: un nuovo insert SENZA id
# esplicito non deve violare la PK (id parte da max+1, non da 1).
step "4) verifica riallineo sequenza (insert senza id esplicito su charon_dst)"
# CTE: l'output è solo l'id (niente tag "INSERT 0 1" da filtrare).
NEWID=$(psql_dst charon_dst "WITH x AS (INSERT INTO customers(name) VALUES ('Nuovo Cliente') RETURNING id) SELECT id FROM x")
echo "  nuovo id assegnato = $NEWID (atteso > $C_SRC)"
[ "$NEWID" -gt "$C_SRC" ] || { echo "FALLITO: sequenza non riallineata (id=$NEWID)"; exit 1; }

# --- 5) round-trip dump → file → import -------------------------------------
DUMPFILE="$(mktemp -t charon-dump-XXXX.sql)"
step "5a) dump sorgente su file — DRY RUN"
"$CHARON" "${COMMON[@]}" --db charon_src dump --out "$DUMPFILE" --dry-run
step "5b) dump sorgente su file — ESECUZIONE"
"$CHARON" "${COMMON[@]}" --db charon_src dump --out "$DUMPFILE"
echo "  file: $DUMPFILE ($(wc -l < "$DUMPFILE") righe)"

step "5c) import del dump in charon_full — DRY RUN"
"$CHARON" "${COMMON[@]}" --db charon_full import --in "$DUMPFILE" --dry-run
step "5d) import del dump in charon_full — ESECUZIONE"
"$CHARON" "${COMMON[@]}" --db charon_full import --in "$DUMPFILE"
C_RT=$(psql_dst charon_full "SELECT count(*) FROM customers")
echo "  customers dopo re-import = $C_RT (atteso $C_SRC)"
rm -f "$DUMPFILE"
[ "$C_RT" = "$C_SRC" ] || { echo "FALLITO: round-trip conteggi diversi"; exit 1; }

step "TUTTI I CONTROLLI SUPERATI ✅"

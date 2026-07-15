# Charon

Strumento desktop per **fare il dump, importare e clonare database**, con una UI
moderna. Stessa impostazione tecnica di GlyphBox/Oxiterm: **Rust + Tauri 2 + Svelte 5**,
workspace Cargo con `core` puro Rust riutilizzabile.

Motori supportati: **PostgreSQL**, **Oracle**, **SQL Server**.

> Charon è il traghettatore: porta i tuoi dati da una sponda all'altra. 🛶

## Cosa fa

- **Dump** — esporta un database su file.
- **Importa** — carica un dump dentro un database.
- **Clona** — copia schema + dati da un database sorgente a uno di destinazione.
  - **Solo dati** (data-only) — preserva lo schema della destinazione (TRUNCATE +
    dati, sequenze riallineate): ideale per schemi gestiti da migration
    (Liquibase/Flyway). *PostgreSQL.*
  - **Mascheramento** — anonimizza colonne sensibili durante il travaso
    (hash, email fittizia, offuscamento, NULL, valore fisso): sicuro per copiare
    **prod→test**. *PostgreSQL, via metodo puro Rust.*
- **Strumenti** — mostra quali tool nativi sono installati e quale metodo verrà usato.

Ogni connessione (dump/import/clone/test) può passare da un **tunnel SSH** integrato
(bastion), in **puro Rust** (crate `russh`, nessun binario `ssh` esterno): host e porta
del DB sono risolti dal lato del server SSH. Auth con **password** o **chiave privata**.

## Strategia ibrida (nativo + puro Rust)

Per ogni operazione Charon sceglie automaticamente (oppure puoi forzare dal selettore
**Metodo**: Auto / Nativo / Puro Rust):

| Motore | Tool nativi (consigliati) | Fallback puro Rust |
|---|---|---|
| **PostgreSQL** | `pg_dump`, `pg_restore`, `psql` | ✅ `tokio-postgres` — dump SQL best-effort |
| **SQL Server** | `mssql-scripter`, `sqlcmd`, `bcp` | ✅ `tiberius` — dump SQL best-effort (via `FOR JSON`) |
| **Oracle** | `expdp`, `impdp`, `sqlplus` (Data Pump) | ✅ crate `oracle` — dump/clone client-side (richiede **Instant Client** a runtime) |

Il pannello **Console** indica sempre **quale metodo è stato usato** (tool nativi o
fallback puro Rust) e mostra i comandi eseguiti e l'output del database.

### Note importanti

- Il **fallback puro Rust** è *best-effort*: esporta lo schema essenziale (colonne,
  NOT NULL) e i dati come `INSERT`. Per fedeltà completa (indici, vincoli, sequenze,
  permessi, tipi custom) usa i **tool nativi**.
- **Oracle Data Pump** (`expdp`/`impdp`) opera **lato server**: il dumpfile viene
  creato nella directory logica `DATA_PUMP_DIR` del server, non in locale.
- Il driver Oracle (crate `oracle`/ODPI-C) è **incluso nei pacchetti di release**:
  compilarlo non richiede l'Instant Client. Serve però l'**Oracle Instant Client**
  *a runtime* (sole librerie, nessun compilatore). Se manca, la schermata
  **Strumenti** mostra il pulsante **"i"** con le istruzioni per installarlo. Il
  path Oracle puro-Rust lavora **lato client** (SELECT→INSERT): ideale per la
  migrazione src→dst, senza `DATA_PUMP_DIR` né accesso DBA.

## Struttura

```
charon/
├─ core/            # logica dump/import/clone, Rust puro (no Tauri)
│  └─ src/
│     ├─ model.rs   # tipi condivisi (Engine, Connection, Method, OpResult…)
│     ├─ tools.rs   # rilevamento tool nativi nel PATH + esecuzione comandi
│     ├─ postgres.rs / mssql.rs / oracle.rs   # implementazioni per motore
│     └─ ops.rs     # orchestratore: sceglie il metodo e instrada
├─ cli/             # binario `charon` headless (stessa logica, senza GUI)
├─ src-tauri/       # backend Tauri (comandi sottili sopra al core)
├─ src/             # UI Svelte 5 (top-bar + tab + area form/console)
└─ docker/          # DB "usa e getta" per prove end-to-end (Postgres, Oracle)
```

## CLI (`charon`)

Oltre all'app desktop c'è una **CLI headless** con la stessa identica logica del
core: ideale su server, in CI, e su sistemi **senza GUI o senza diritti di
amministrazione**. È un **singolo binario** (per PostgreSQL/SQL Server/SSH non
serve installare alcun client).

Il binario CLI si chiama **`charon`**; l'app desktop (Tauri) è **`charon-desktop`**
(nome-prodotto "Charon"): così i due eseguibili convivono nello stesso `target/`.

```bash
cargo build --release -p charon-cli          # senza Oracle: binario unico
cargo build --release -p charon-cli --features oracle   # con driver Oracle

charon tools                                  # cosa è disponibile sulla macchina
CHARON_PASSWORD=… charon test  --engine pg --host db --db app --user app
CHARON_PASSWORD=… charon dump  --db app --user app --out app.sql --prefer rust
CHARON_PASSWORD=… CHARON_DST_PASSWORD=… charon clone \
    --db prod --user app --dst-host stage --dst-db stage --dst-user app \
    --data-only --mask users.email=email
charon help                                   # elenco completo di comandi e flag
```

Le **password** si passano da variabile d'ambiente (`CHARON_PASSWORD`,
`CHARON_DST_PASSWORD`), non come argomenti (non finiscono nella lista processi).

### Dry-run (anteprima)

Ogni operazione che modifica dati — **dump, import, clone, oracle-load** —
accetta **`--dry-run`** (nella GUI: interruttore **Dry-run** accanto al Metodo):
mostra esattamente **cosa verrebbe fatto** (comandi, tabelle, righe, ordine)
**senza toccare nulla**. Consigliato prima di ogni operazione distruttiva.

### Oracle senza privilegi DBA: pacchetti SQL\*Loader

`charon oracle-load --dir <pacchetto>` importa un pacchetto **SQL\*Loader**
(`.ctl`/`.ldr`, come esportato da SQL Developer in "formato Loader") lanciando
`sqlldr` per ogni tabella nell'ordine di `load_order.txt` (rispetta i vincoli
FK). Gestisce i **BLOB** (via `LOBFILE`), che gli `INSERT` non trasferiscono, e al
termine **riallinea le sequenze** identity (passaggio obbligatorio dopo un load
diretto). È il metodo no-admin descritto nella documentazione Oracle. Vedi
`docker/oracle/sample-package/` per un esempio e `docker/README.md` per provarlo.

**Instant Client senza installazione.** Il path Oracle richiede l'Oracle Instant
Client a runtime (solo librerie, no-admin). Charon lo gestisce da sé: scarichi UNA
volta lo `.zip` ufficiale per il tuo OS/arch e lo agganci con

```bash
charon oracle-setup --zip instantclient-basiclite-<os>-<arch>.zip
```

(oppure dalla GUI: **Strumenti → "Configura Instant Client"**). Charon lo
scompatta in una cartella dell'utente, verifica la libreria giusta per il sistema
(`oci.dll` / `libclntsh.dylib` / `libclntsh.so`) e la ricorda.

Oppure, **zero-config**: lascia lo `.zip` (o la cartella estratta) in
**`vendor/oracle/`** e Charon lo aggancia da solo al primo uso di Oracle (i file
lì restano solo in locale, non vengono committati). Serve una build
`--features oracle`. Dettagli e limiti in **[PORTABILITY.md](PORTABILITY.md)**.

## Prove con Docker

`docker/` contiene ambienti pronti per esercitare Charon **senza client DB
installati**:

```bash
./docker/postgres/run-postgres-test.sh   # clone pieno + data-only + mask + sequenze
./docker/oracle/run-oracle-test.sh       # dry-run sempre; reale con Instant Client
```

## Portabilità (Windows / senza admin)

I limiti di portabilità sono documentati in **[PORTABILITY.md](PORTABILITY.md)**.
In breve: la CLI puro-Rust verso **PostgreSQL/SQL Server** (anche via tunnel SSH)
è un **binario unico, no-admin, cross-platform**; **Oracle** è l'unico motore che
richiede una dipendenza a runtime (l'**Instant Client**, solo librerie, comunque
senza admin).

## Sviluppo

```bash
npm install
npm run tauri dev      # avvia l'app in sviluppo
npm run tauri build    # genera i pacchetti
```

## Release

I pacchetti per macOS, Linux (.deb/.rpm/.AppImage), Windows e Arch (.pkg.tar.zst)
vengono generati dalla CI quando spingi un tag:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

Repo: https://github.com/russus87/charon

## Licenza

MIT.

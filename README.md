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
├─ src-tauri/       # backend Tauri (comandi sottili sopra al core)
└─ src/             # UI Svelte 5 (top-bar + tab + area form/console)
```

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

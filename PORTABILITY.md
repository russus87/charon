# Portabilità di Charon

> Requisito: girare su **Windows** e su **sistemi senza diritti di
> amministrazione**, con la **massima portabilità**. Questo documento spiega
> cosa funziona ovunque come singolo binario, cosa richiede dipendenze esterne,
> e quali sono i limiti reali — motore per motore e sistema per sistema.

## TL;DR

| Cosa | Portabile / no-admin? | Note |
|---|---|---|
| **CLI `charon`** (PostgreSQL, SQL Server, SSH) | ✅ **Sì**, binario unico | Nessun client DB, nessun installer, nessun admin. Basta copiare l'eseguibile. |
| **CLI `charon`** con Oracle | ⚠️ Parziale | Serve l'**Oracle Instant Client** a runtime (solo librerie, no admin). |
| **App desktop** (Tauri) | ⚠️ Quasi | Usa la webview di sistema; su Windows serve **WebView2** (di norma già presente). |
| **Tool nativi** (`pg_dump`, `expdp`, `sqlcmd`, …) | ❌ Opzionali | Massima fedeltà ma vanno installati (spesso grandi / con admin). Charon li usa solo se presenti. |

La configurazione **massimamente portabile** è: **CLI `charon`** + metodo **puro
Rust** (`--prefer rust`) verso **PostgreSQL** o **SQL Server**, eventualmente
attraverso il **tunnel SSH** integrato. È un unico file eseguibile, copiabile su
chiavetta, che non tocca il sistema e non richiede alcun privilegio.
*(Testato in questo repo con Docker e **zero** client DB installati: vedi
`docker/postgres/run-postgres-test.sh`.)*

---

## 1. Il cuore è Rust puro

`charon-core` non dipende da Tauri e implementa dump/import/clone con **driver in
Rust puro**:

- **PostgreSQL** → `tokio-postgres`
- **SQL Server** → `tiberius` (protocollo TDS, TLS via **rustls**: niente OpenSSL
  di sistema)
- **Tunnel SSH** → `russh` (nessun binario `ssh` esterno)

Questi tre non richiedono **nulla** di preinstallato: si compilano dentro il
binario. È ciò che rende la CLI un singolo file autosufficiente su Windows,
Linux e macOS.

## 2. Il caso Oracle (il vero limite)

**Oracle non ha un driver di rete in Rust puro**: il protocollo TNS è
proprietario. Charon usa quindi il crate `oracle` (ODPI-C), che **carica a
runtime l'Oracle Instant Client** (`libclntsh.so` / `oci.dll`) via `dlopen`.

Conseguenze:

- **Compilare** Charon con Oracle **non** richiede l'Instant Client (ODPI-C è
  vendored): perciò le release CI lo includono già.
- **Eseguire** operazioni Oracle **sì**: serve l'Instant Client sulla macchina.
  - È solo un set di **librerie** (pacchetto *Basic*, ~200 MB estratti; *Basic
    Lite* molto più piccolo e con UTF-8): **niente compilatore, niente admin**.
  - `sqlldr` (per `charon oracle-load`) è nel pacchetto *Tools* dell'Instant
    Client, stessa logica.
- **Charon lo gestisce per te** (nessuna installazione a sistema): scarichi UNA
  volta lo `.zip` ufficiale per il tuo OS/architettura e lo agganci con
  **`charon oracle-setup --zip <file.zip>`** (o dalla GUI: **Strumenti →
  "Configura Instant Client"**). Charon lo **scompatta in una cartella
  dell'utente**, verifica che contenga la libreria giusta **per il sistema
  corrente** (`oci.dll` su Windows, `libclntsh.dylib` su macOS, `libclntsh.so`
  su Linux) e ricorda il percorso; a runtime punta ODPI-C lì
  (`InitParams::oracle_client_lib_dir`, cross-platform, senza toccare
  `PATH`/`LD_LIBRARY_PATH`). In alternativa: `--dir <cartella_estratta>` o la
  variabile `CHARON_ORACLE_CLIENT`.
- La schermata **Strumenti** (GUI) e `charon tools` (CLI) rilevano se l'Instant
  Client c'è (anche quello gestito da Charon) e, in caso contrario, mostrano le
  istruzioni.

> Perché non un unico zip incluso in Charon? Perché Oracle pubblica **zip diversi
> per ogni OS/arch** (non "adattabili") e la ridistribuzione dei binari Oracle in
> un repo pubblico è una scelta di licenza da evitare. Il flusso `oracle-setup`
> dà la stessa comodità ("Charon scompatta lo zip giusto") senza ridistribuire
> nulla e funziona **offline** su macchine isolate.

#### Cosa fa Charon "dietro le quinte" (e i vincoli residui)

Far funzionare l'Instant Client da una cartella qualsiasi, senza installarlo,
richiede due accorgimenti che Charon gestisce da solo — ma che è bene conoscere:

- **Symlink nello zip**: gli archivi Oracle contengono symlink
  (`libclntsh.so → libclntsh.so.23.1`). Charon li **ricrea** in fase di
  estrazione (se venissero copiati come file normali, il client non si
  caricherebbe: *"file too short"*).
- **Nessun RUNPATH** negli Instant Client recenti: `libclntsh` non "sa" dove
  cercare le sue dipendenze (`libnnz.so`, `libclntshcore.so`). Su Linux/macOS la
  cartella va messa in `LD_LIBRARY_PATH`/`DYLD_LIBRARY_PATH`, che il loader legge
  **solo all'avvio**: Charon quindi **rilancia sé stesso una volta** con quella
  variabile impostata (trasparente, idempotente). Su Windows non serve (il loader
  cerca le dipendenze accanto alla DLL caricata).
- **Dipendenze di sistema**:
  - **Linux**: serve **`libaio`** (`libaio.so.1`), quasi sempre già presente; se
    manca va installata (pacchetto `libaio`). Nessun altro requisito.
  - **Windows**: l'Instant Client richiede il **Visual C++ Redistributable**.
    Se non è già installato, installarlo di norma **richiede privilegi di
    amministratore** → è l'unico punto in cui il requisito "no-admin" può saltare
    su Windows per il solo Oracle. Su molte postazioni aziendali è già presente.

Perché non Data Pump (`expdp`/`impdp`)? Perché opera **lato server**, scrive il
dump nella directory logica `DATA_PUMP_DIR` e richiede in pratica privilegi da
**DBA** — l'opposto del requisito no-admin. Per questo la migrazione Oracle
no-admin usa il workflow **SQL\*Loader** (vedi sotto), lato client.

> **In sintesi:** l'unico motore che rompe la portabilità "binario unico" è
> **Oracle**, e solo a runtime (librerie Instant Client). Rimane comunque
> **no-admin**.

## 3. Workflow Oracle no-admin: SQL\*Loader

Ricalca la procedura della documentazione (`Analisi DUMP DB ORACLE`), pensata
proprio per un cliente **senza privilegi DBA**:

1. **Export** (a monte, con SQL Developer): un pacchetto `.zip` per schema con un
   `.ctl` + `.ldr` per tabella (i BLOB come `LOBFILE`). Gli `INSERT` semplici non
   trasferiscono i BLOB: SQL\*Loader sì.
2. **Import** con `charon oracle-load --dir <pacchetto>`: lancia `sqlldr` per ogni
   tabella nell'ordine di `load_order.txt` (rispetta i vincoli FK). È l'equivalente
   portabile e cross-platform degli script `script_load_*.sh` della zip — ma
   funziona anche su Windows, senza shell POSIX.
3. **Riallineo sequenze**: dopo un load diretto le colonne *identity* non
   avanzano; Charon riallinea automaticamente le sequenze a `MAX(id)+1`
   (`user_tab_identity_cols` → `ALTER SEQUENCE … RESTART`). Senza il driver Oracle
   compilato, stampa comunque le query SQL da eseguire a mano.

Il **dry-run** di `oracle-load` funziona **ovunque, senza Instant Client e senza
DB**: legge il pacchetto e stampa i comandi `sqlldr` che verrebbero eseguiti.

## 4. App desktop (Tauri) vs CLI

- **Tauri** non impacchetta un browser: usa la **webview del sistema operativo**.
  - **Windows**: **WebView2** (runtime Evergreen). Su Windows 10/11 è quasi sempre
    già presente; se manca, si installa **per-utente senza admin**. L'installer
    **NSIS** di Charon supporta l'installazione per-utente.
  - **Linux**: **WebKitGTK** (`libwebkit2gtk-4.1`). In genere presente sui desktop;
    l'`.AppImage` è il formato più portabile (nessuna installazione).
  - **macOS**: **WKWebView**, sempre presente.
- La **CLI** non ha nessuna di queste dipendenze: è la scelta giusta per server,
  container, CI e desktop "bloccati". Stessa identica logica dell'app.

## 5. Dettagli Windows

- Ricerca dei tool nativi: Charon aggiunge automaticamente `.exe` (`pg_dump.exe`…).
- **Password**: passale via variabile d'ambiente (`CHARON_PASSWORD`,
  `CHARON_DST_PASSWORD`), **non** come argomento — sulla riga di comando finirebbero
  nella lista processi. La GUI le tiene solo in memoria.
- Percorsi con spazi: usa le virgolette (`--out "C:\Temp\dump.sql"`).
- Nessuna dipendenza da `ssh.exe`, `bash` o cygwin: il tunnel è in Rust puro.

## 6. Limiti noti / cose da sapere

- **PostgreSQL puro-Rust e TLS**: la connessione usa `NoTls`. Va benissimo in
  LAN, dietro il **tunnel SSH** (consigliato) o verso `localhost`; **non** cifra
  di per sé il traffico. Per TLS diretto servirebbe abilitare `rustls` su
  `tokio-postgres` (miglioria futura). SQL Server invece cifra già via rustls.
- **Fallback puro-Rust = best-effort**: esporta schema essenziale (colonne, NOT
  NULL) + dati come `INSERT`. **Non** replica indici, vincoli, trigger, sequenze
  custom, permessi, tipi definiti dall'utente. Per fedeltà completa servono i tool
  nativi (o, per Oracle, il workflow SQL\*Loader che preserva i BLOB).
- **BLOB/LOB grandi via INSERT**: il path puro-Rust li tratta come testo/valori —
  adatto a dati "normali", non a colonne binarie grandi. Per quelli, SQL\*Loader
  (Oracle) o i tool nativi.
- **Ordine FK nel clone puro-Rust**: il clone "pieno" ricrea le tabelle senza
  vincoli, quindi l'ordine non blocca; il data-only usa
  `TRUNCATE … CASCADE` + `session_replication_role=replica` (best-effort: se non
  hai i permessi per disabilitare i trigger, alcune FK potrebbero ostacolare).
- **`session_replication_role`/`disable-triggers`**: alcune ottimizzazioni del
  data-only richiedono privilegi elevati; Charon prosegue comunque, ma con quei
  permessi il travaso è più pulito.

## 7. Raccomandazione operativa

Per un ambiente Windows **senza admin**:

1. Copia il **binario CLI `charon`** (nessuna installazione).
2. PostgreSQL / SQL Server → usa il **puro Rust** (`--prefer rust`), eventualmente
   con **`--ssh-host`** per passare da un bastion.
3. Oracle → estrai l'**Instant Client** (Basic [+ Tools per `sqlldr`]) in una
   cartella dell'utente e aggiungila al `PATH`; poi usa `oracle-load` per i
   pacchetti SQL\*Loader, oppure il clone puro-Rust src→dst.
4. Prima di ogni operazione distruttiva, lancia lo stesso comando con **`--dry-run`**.

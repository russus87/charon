# Oracle Instant Client "in dote" (drop-in)

Charon aggancia **automaticamente** un Oracle Instant Client lasciato in questa
cartella, senza installarlo a sistema e senza privilegi di amministrazione.

## Come fare (una volta)

1. Scarica il pacchetto **Basic** o **Basic Lite** per il TUO sistema/architettura da:
   https://www.oracle.com/database/technologies/instant-client/downloads.html
   - Linux x86-64:   `instantclient-basiclite-linux.x64-<versione>.zip`
   - Windows x64:     `instantclient-basiclite-windows.x64-<versione>.zip`
   - macOS ARM64:     `instantclient-basiclite-macos.arm64-<versione>.zip`
   - Per usare anche `sqlldr`/`sqlplus` (comando `oracle-load`) serve il pacchetto **Tools**.

2. Copia lo **.zip** così com'è in questa cartella (`vendor/oracle/`).
   In alternativa, estrailo qui (va bene anche la cartella `instantclient_.../`).

3. Fatto. Al primo uso di Oracle, Charon:
   - scompatta lo zip in una cartella dell'utente,
   - verifica che contenga la libreria giusta per il sistema
     (`libclntsh.so` Linux · `oci.dll` Windows · `libclntsh.dylib` macOS),
   - la ricorda e ci punta ODPI-C a runtime.

> Serve una **build con Oracle**: `--features oracle`
> (es. `cargo build --release -p charon-cli --features oracle`,
>  oppure `npm run tauri dev -- --features oracle` per l'app).

## Note

- I file qui dentro **non vengono committati** (vedi `.gitignore`): restano solo
  sul tuo computer. Le librerie sono di proprietà Oracle.
- Puoi anche saltare questa cartella e agganciare il client a mano:
  `charon oracle-setup --zip <percorso>.zip`  ·  `--dir <cartella_estratta>`
  ·  oppure la variabile d'ambiente `CHARON_ORACLE_CLIENT=<cartella>`.
- Lo zip è **specifico per OS/architettura**: usa quello giusto per la macchina
  su cui gira Charon.

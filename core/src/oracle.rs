//! Oracle: dump/import/clone.
//!
//! - **Nativo**: Oracle Data Pump (`expdp`/`impdp`) e `sqlplus` per il test.
//!   ATTENZIONE: Data Pump scrive/legge i file lato **server**, dentro la
//!   directory logica `DATA_PUMP_DIR`. Il nome file scelto in Charon viene usato
//!   come `dumpfile`; il file fisico resta sul server del database.
//! - **Puro Rust**: disponibile solo compilando con la feature `oracle-driver`,
//!   che richiede l'Oracle Instant Client in fase di link (non nelle build CI).
//!   Senza di essa, per Oracle servono i tool nativi.

use crate::compare::{ColumnDiff, DbDiff, RowDelta, Status, TableDataDiff, TableDiff};
use crate::model::*;
use crate::schema::{AbstractType, Column, ForeignKey, Index, SchemaModel, Table};
use crate::tools::{find_tool, has_tool, plan_or_run, run};
use crate::{Error, Result};
use std::path::Path;
use std::process::Command;

/// Data-only e masking non sono ancora implementati per Oracle: lo diciamo
/// chiaramente invece di ignorare l'opzione in silenzio.
fn reject_unsupported_opts(opts: &CloneOptions) -> Result<()> {
    if opts.data_only || opts.has_mask() {
        return Err(Error::Unsupported(
            "data-only e mascheramento sono al momento disponibili solo per PostgreSQL".into(),
        ));
    }
    Ok(())
}

/// Stringa di connessione Oracle `user/pass@host:port/service`.
fn conn_str(conn: &Connection) -> String {
    format!(
        "{}/{}@{}:{}/{}",
        conn.user, conn.password, conn.host, conn.port, conn.database
    )
}
fn conn_display(conn: &Connection) -> String {
    format!("{}@{}:{}/{}", conn.user, conn.host, conn.port, conn.database)
}

/// Solo il nome del file (Data Pump lavora dentro DATA_PUMP_DIR sul server).
fn basename(p: &str) -> String {
    Path::new(p)
        .file_name()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string())
}

pub fn native_available() -> bool {
    has_tool("expdp") && has_tool("impdp")
}

/// Il path puro-Rust e' utilizzabile solo se (a) Charon e' compilato con la
/// feature `oracle-driver` E (b) l'Oracle Instant Client e' presente a runtime
/// (ODPI-C lo carica con dlopen alla prima connessione). Compilare la feature
/// NON richiede l'Instant Client; usarla si'.
pub fn rust_available() -> bool {
    cfg!(feature = "oracle-driver") && client_lib_present()
}

/// Cerca la libreria client Oracle (Instant Client) nei percorsi tipici del
/// dynamic loader, senza tentare una connessione. Serve per dare un report
/// onesto in UI: "puro Rust ✓" solo se il client c'e' davvero.
pub fn client_lib_present() -> bool {
    // Priorità: una cartella client "gestita" da Charon (configurata con
    // `oracle-setup`) vale come client presente, ovunque sia.
    if managed_client_dir().is_some() {
        return true;
    }
    // Nome della libreria per piattaforma.
    let (base, exts) = lib_names();

    // Directory candidate: variabili d'ambiente + percorsi comuni di installazione.
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    let env_path_vars = if cfg!(target_os = "windows") {
        &["PATH"][..]
    } else if cfg!(target_os = "macos") {
        &["DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH", "PATH"][..]
    } else {
        &["LD_LIBRARY_PATH", "PATH"][..]
    };
    for var in env_path_vars {
        if let Ok(val) = std::env::var(var) {
            for p in std::env::split_paths(&val) {
                dirs.push(p);
            }
        }
    }
    if let Ok(home) = std::env::var("ORACLE_HOME") {
        let h = std::path::PathBuf::from(home);
        dirs.push(h.join("lib"));
        dirs.push(h.clone());
        dirs.push(h.join("bin"));
    }
    for common in [
        "/usr/lib", "/usr/local/lib", "/usr/lib/oracle", "/opt/oracle",
        "/usr/lib64", "/lib", "/lib64",
    ] {
        dirs.push(std::path::PathBuf::from(common));
    }

    // Una directory va bene se contiene un file il cui nome inizia con `base` e
    // contiene una delle estensioni (es. libclntsh.so, libclntsh.so.21.1).
    dirs.iter().any(|dir| dir_has_lib(dir, base, exts))
}

fn dir_has_lib(dir: &Path, base: &str, exts: &[&str]) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if name.starts_with(base) && exts.iter().any(|ext| name.contains(ext)) {
            return true;
        }
    }
    false
}

/// Nome base + estensioni della libreria client Oracle per la piattaforma corrente.
fn lib_names() -> (&'static str, &'static [&'static str]) {
    if cfg!(target_os = "windows") {
        ("oci", &[".dll"])
    } else if cfg!(target_os = "macos") {
        ("libclntsh", &[".dylib"])
    } else {
        ("libclntsh", &[".so"])
    }
}

/// Vero se la cartella contiene la libreria client per QUESTO sistema operativo.
fn dir_has_client_lib(dir: &Path) -> bool {
    let (base, exts) = lib_names();
    dir_has_lib(dir, base, exts)
}

/// Breve descrizione della libreria attesa (per messaggi d'errore chiari).
#[cfg(feature = "oracle-driver")]
fn os_client_hint() -> &'static str {
    if cfg!(target_os = "windows") {
        "oci.dll (Windows)"
    } else if cfg!(target_os = "macos") {
        "libclntsh.dylib (macOS)"
    } else {
        "libclntsh.so (Linux)"
    }
}

// ------------------------------------------- Instant Client "gestito" da Charon ---
// Charon può agganciare un Oracle Instant Client estratto in una cartella
// dell'utente (nessun admin), senza doverlo installare a sistema. La cartella si
// configura con `oracle-setup` (da uno .zip ufficiale o da una cartella già
// estratta) e viene ricordata in un file puntatore. A runtime, ODPI-C viene
// puntato lì via `InitParams::oracle_client_lib_dir` (funziona su Win/Linux/mac,
// senza dover impostare LD_LIBRARY_PATH/PATH a processo già avviato).

/// Cartella di configurazione per-utente di Charon (nessun privilegio richiesto).
fn config_base() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("charon"))
    } else if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(PathBuf::from(x).join("charon"))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("charon"))
    }
}

/// Cartella dati per-utente dove estraiamo l'Instant Client.
#[cfg(feature = "oracle-driver")]
fn data_base() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("charon"))
    } else if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        Some(PathBuf::from(x).join("charon"))
    } else {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".local").join("share").join("charon"))
    }
}

/// File che ricorda la cartella dell'Instant Client agganciato.
fn pointer_file() -> Option<std::path::PathBuf> {
    config_base().map(|d| d.join("oracle_client_dir.txt"))
}

/// Cartella client attualmente configurata e VALIDA, se c'è.
/// Precedenza: `CHARON_ORACLE_CLIENT` → file puntatore → zip/cartella "in dote"
/// (auto-provisioning da `vendor/oracle/`, vedi [`auto_provision_bundled`]).
pub fn managed_client_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    // 1) override esplicito via variabile d'ambiente.
    if let Some(env) = std::env::var_os("CHARON_ORACLE_CLIENT") {
        let p = PathBuf::from(env);
        if dir_has_client_lib(&p) {
            return Some(p);
        }
    }
    // 2) percorso ricordato da un precedente oracle-setup.
    if let Some(ptr) = pointer_file() {
        if let Ok(content) = std::fs::read_to_string(&ptr) {
            let dir = PathBuf::from(content.trim());
            if dir_has_client_lib(&dir) {
                return Some(dir);
            }
        }
    }
    // 3) zip/cartella lasciati in vendor/oracle: Charon li aggancia da solo.
    #[cfg(feature = "oracle-driver")]
    {
        return auto_provision_bundled();
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        None
    }
}

/// Aggiunge le candidate `vendor/oracle` (e `Resources/vendor/oracle` per il
/// bundle macOS) risalendo alcuni livelli da `start`. Risalire serve perché in
/// sviluppo la working dir è `src-tauri/` mentre l'exe sta in `target/debug/`:
/// la cartella `vendor/oracle` del progetto è qualche livello più su.
#[cfg(feature = "oracle-driver")]
fn push_vendor_ancestors(dirs: &mut Vec<std::path::PathBuf>, start: Option<std::path::PathBuf>) {
    if let Some(mut d) = start {
        for _ in 0..4 {
            dirs.push(d.join("vendor").join("oracle"));
            dirs.push(d.join("Resources").join("vendor").join("oracle"));
            match d.parent() {
                Some(p) => d = p.to_path_buf(),
                None => break,
            }
        }
    }
}

/// Cartelle dove Charon cerca un Instant Client "in dote" (una `vendor/oracle/`),
/// risalendo da eseguibile e working dir. Percorsi ristretti apposta, per non
/// scandagliare cartelle enormi.
#[cfg(feature = "oracle-driver")]
fn client_search_dirs() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut dirs = Vec::new();
    // 1) Override esplicito (l'app può impostarlo con la resource dir del bundle).
    if let Some(v) = std::env::var_os("CHARON_ORACLE_VENDOR") {
        dirs.push(PathBuf::from(v));
    }
    // 2) Risorse impacchettate da Tauri (bundle.resources): percorsi tipici
    //    relativi all'eseguibile, per Windows/Linux/macOS.
    if let Ok(exe) = std::env::current_exe() {
        let bin = exe.file_name().map(|n| n.to_os_string());
        if let Some(d) = exe.parent() {
            dirs.push(d.join("oracle")); // Windows: risorse accanto all'exe
            dirs.push(d.join("resources").join("oracle"));
            if let Some(up) = d.parent() {
                dirs.push(up.join("Resources").join("resources").join("oracle")); // macOS bundle
                dirs.push(up.join("Resources").join("oracle"));
                if let Some(name) = &bin {
                    // Linux deb/AppImage/pkg: <prefix>/lib/<binario>/[resources/]oracle
                    dirs.push(up.join("lib").join(name).join("resources").join("oracle"));
                    dirs.push(up.join("lib").join(name).join("oracle"));
                }
            }
        }
    }
    // 3) vendor/oracle risalendo da eseguibile e working dir (comodo in sviluppo).
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf()));
    push_vendor_ancestors(&mut dirs, exe_dir);
    push_vendor_ancestors(&mut dirs, std::env::current_dir().ok());
    dirs
}

/// Primo file `instantclient*.zip` in una cartella (non ricorsivo).
#[cfg(feature = "oracle-driver")]
fn zip_in_dir(dir: &Path) -> Option<std::path::PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if name.starts_with("instantclient") && name.ends_with(".zip") {
            return Some(p);
        }
    }
    None
}

/// Aggancia (una sola volta per processo) un Instant Client lasciato in
/// `vendor/oracle/`: una cartella già estratta oppure uno `instantclient*.zip`
/// da scompattare. Al successo scrive il puntatore, così le chiamate successive
/// passano dal ramo (2) di [`managed_client_dir`].
#[cfg(feature = "oracle-driver")]
fn auto_provision_bundled() -> Option<std::path::PathBuf> {
    use std::sync::OnceLock;
    static DONE: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    DONE.get_or_init(|| {
        let mut log = Vec::new();
        for dir in client_search_dirs() {
            if !dir.is_dir() {
                continue;
            }
            // (a) client già estratto dentro vendor/oracle?
            if let Some(found) = find_client_dir(&dir) {
                if persist_pointer(&found).is_ok() {
                    return Some(found);
                }
            }
            // (b) uno zip da scompattare?
            if let Some(zip) = zip_in_dir(&dir) {
                if let Ok(d) = provision(&zip.to_string_lossy(), &mut log) {
                    return Some(d);
                }
            }
        }
        None
    })
    .clone()
}

/// Scrive il file puntatore con la cartella client scelta.
#[cfg(feature = "oracle-driver")]
fn persist_pointer(dir: &Path) -> Result<()> {
    let ptr = pointer_file()
        .ok_or_else(|| Error::Msg("impossibile determinare la cartella di configurazione".into()))?;
    if let Some(parent) = ptr.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&ptr, dir.display().to_string())?;
    Ok(())
}

/// Cerca ricorsivamente la sottocartella che contiene la libreria client.
#[cfg(feature = "oracle-driver")]
fn find_client_dir(root: &Path) -> Option<std::path::PathBuf> {
    if dir_has_client_lib(root) {
        return Some(root.to_path_buf());
    }
    for e in std::fs::read_dir(root).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(found) = find_client_dir(&p) {
                return Some(found);
            }
        }
    }
    None
}

/// Estrae uno .zip nella cartella `dest` (solo `zip`+deflate: gli Instant Client
/// ufficiali usano questo formato).
#[cfg(feature = "oracle-driver")]
fn extract_zip(zip_path: &Path, dest: &Path, log: &mut Vec<String>) -> Result<()> {
    use std::io::Read;
    let f = std::fs::File::open(zip_path)?;
    let mut ar = zip::ZipArchive::new(f).map_err(|e| Error::Msg(format!("zip non valido: {e}")))?;
    log.push(format!("Archivio: {} voci.", ar.len()));
    let mut symlinks = 0u32;
    for i in 0..ar.len() {
        let mut file = ar.by_index(i).map_err(|e| Error::Msg(format!("zip: {e}")))?;
        // enclosed_name protegge dagli zip-slip (percorsi ../ malevoli).
        let Some(rel) = file.enclosed_name() else { continue };
        let out = dest.join(rel);
        if file.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Gli zip dell'Instant Client contengono SYMLINK (es. libclntsh.so →
        // libclntsh.so.23.1): il contenuto della voce è il percorso di
        // destinazione. Se li scrivessimo come file normali, ODPI-C troverebbe un
        // libclntsh.so di pochi byte ("file too short"). Vanno ricreati come link.
        #[cfg(unix)]
        {
            let is_symlink = file.unix_mode().map(|m| m & 0o170000 == 0o120000).unwrap_or(false);
            if is_symlink {
                let mut target = String::new();
                file.read_to_string(&mut target)?;
                let _ = std::fs::remove_file(&out); // sovrascrivi un eventuale residuo
                std::os::unix::fs::symlink(target.trim(), &out)?;
                symlinks += 1;
                continue;
            }
        }
        let mut w = std::fs::File::create(&out)?;
        std::io::copy(&mut file, &mut w)?;
        #[cfg(unix)]
        if let Some(mode) = file.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode));
        }
    }
    if symlinks > 0 {
        log.push(format!("Ricreati {symlinks} symlink.", ));
    }
    Ok(())
}

/// Configura l'Instant Client a partire da `path` (uno .zip ufficiale oppure una
/// cartella già estratta) e ricorda la scelta. Ritorna la cartella agganciata.
#[cfg(feature = "oracle-driver")]
pub fn provision(path: &str, log: &mut Vec<String>) -> Result<std::path::PathBuf> {
    let src = Path::new(path);
    if !src.exists() {
        return Err(Error::Msg(format!("percorso inesistente: {path}")));
    }
    let dir = if src.is_dir() {
        log.push(format!("Uso la cartella client indicata: {}", src.display()));
        find_client_dir(src)
            .ok_or_else(|| Error::Msg(format!("in {} non trovo le librerie client", src.display())))?
    } else if src.extension().map(|e| e.eq_ignore_ascii_case("zip")).unwrap_or(false) {
        let dest = data_base()
            .ok_or_else(|| Error::Msg("impossibile determinare la cartella dati utente".into()))?
            .join("instantclient");
        std::fs::create_dir_all(&dest)?;
        log.push(format!("Estrazione di {} in {} …", src.display(), dest.display()));
        extract_zip(src, &dest, log)?;
        find_client_dir(&dest).ok_or_else(|| {
            Error::Msg(format!(
                "nell'archivio non ho trovato le librerie client attese ({})",
                os_client_hint()
            ))
        })?
    } else {
        return Err(Error::Msg(
            "fornisci uno .zip dell'Instant Client oppure una cartella già estratta".into(),
        ));
    };
    if !dir_has_client_lib(&dir) {
        return Err(Error::Msg(format!(
            "la cartella {} non contiene la libreria attesa per questo sistema ({}). \
             Hai scaricato lo zip giusto per il tuo OS/architettura?",
            dir.display(),
            os_client_hint()
        )));
    }
    persist_pointer(&dir)?;
    log.push(format!("✅ Instant Client agganciato: {}", dir.display()));
    log.push("La configurazione è ricordata: le prossime operazioni Oracle lo useranno.".into());
    Ok(dir)
}

/// Se è configurato un Instant Client "gestito" ma la sua cartella non è ancora
/// nel path del loader dinamico, **rilancia il processo** con quella cartella in
/// `LD_LIBRARY_PATH` (Linux) / `DYLD_LIBRARY_PATH` (macOS).
///
/// Serve perché gli Instant Client recenti NON hanno il RUNPATH `$ORIGIN`: senza
/// la cartella nel path del loader, `libclntsh` non trova le dipendenze
/// (`libnnz.so`, che per giunta è senza soname → un dlopen mirato non basta),
/// dando l'errore DPI-1047. E quella variabile va impostata PRIMA dell'avvio
/// (glibc la legge una sola volta). È idempotente (guardia anti-loop) e va
/// chiamata il prima possibile in `main`. No-op se non serve o su Windows
/// (là il loader cerca le dipendenze accanto alla DLL caricata).
pub fn ensure_client_env() {
    #[cfg(all(feature = "oracle-driver", unix))]
    {
        // Già rilanciati una volta: evita loop infiniti.
        if std::env::var_os("CHARON_ORACLE_REEXEC").is_some() {
            return;
        }
        let Some(dir) = managed_client_dir() else {
            return;
        };
        let var = if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        let dir_s = dir.to_string_lossy().to_string();
        let current = std::env::var(var).unwrap_or_default();
        if current.split(':').any(|p| p == dir_s) {
            return; // la cartella è già nel path: nessun re-exec.
        }
        let newval = if current.is_empty() {
            dir_s.clone()
        } else {
            format!("{dir_s}:{current}")
        };
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        use std::os::unix::process::CommandExt;
        // exec() sostituisce il processo: se ritorna, è fallita e proseguiamo
        // (ODPI-C darà comunque un messaggio chiaro).
        let _ = std::process::Command::new(exe)
            .args(std::env::args_os().skip(1))
            .env(var, newval)
            .env("CHARON_ORACLE_REEXEC", "1")
            .exec();
    }
}

/// Punta ODPI-C alla cartella client gestita (una sola volta per processo).
/// Se non c'è una cartella gestita, non fa nulla: si usa il loader di sistema.
#[cfg(feature = "oracle-driver")]
fn ensure_client_init() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if let Some(dir) = managed_client_dir() {
            let mut params = oracle::InitParams::new();
            if let Ok(p) = params.oracle_client_lib_dir(&dir) {
                // init() fallisce solo se un contesto è già stato creato: in tal
                // caso ricadiamo sul loader di default, nessun danno.
                let _ = p.init();
            }
        }
    });
}

fn tool(name: &str, purpose: &str) -> ToolInfo {
    let p = find_tool(name);
    ToolInfo {
        name: name.into(),
        found: p.is_some(),
        path: p.map(|x| x.display().to_string()),
        purpose: purpose.into(),
    }
}

pub fn report() -> EngineReport {
    let tools = vec![
        tool("expdp", "export Data Pump (dump)"),
        tool("impdp", "import Data Pump"),
        tool("sqlplus", "test connessione / script"),
    ];
    let native = native_available();
    // Tre stati distinti, per dare il suggerimento giusto:
    //  - compiled: il driver Oracle e' incluso in questa build?
    //  - client:   l'Instant Client e' presente a runtime?
    //  - rust:     entrambi -> il path puro-Rust e' davvero usabile.
    let compiled = cfg!(feature = "oracle-driver");
    let client = client_lib_present();
    let rust = compiled && client;
    let note = if native {
        "Tool nativi trovati. Nota: Data Pump opera lato server (DATA_PUMP_DIR): \
         il dumpfile viene creato sul server del database."
            .into()
    } else if rust {
        "Driver Oracle pronto (Instant Client rilevato): migrazione client-side senza tool nativi.".into()
    } else if compiled {
        "Manca l'Oracle Instant Client (librerie runtime): premi \"i\" per le istruzioni.".into()
    } else {
        "Questa build non include il driver Oracle: usa una release ufficiale o i tool nativi.".into()
    };
    let mut hints = Vec::new();
    if !compiled {
        // Charon e' stato compilato SENZA il supporto Oracle puro-Rust.
        // (Le release ufficiali lo includono: questo accade solo in build custom.)
        hints.push(FixHint::new(
            "Supporto Oracle non incluso in questa build",
            "Questa copia di Charon è stata compilata senza il driver Oracle. Le release \
             ufficiali lo includono già: scarica un pacchetto dalla pagina Releases, oppure \
             — se compili da te — abilita la feature 'oracle-driver'.",
            Some("cargo build --release --features charon-core/oracle-driver"),
        ));
    } else if !client {
        // Build con supporto Oracle: serve solo l'Instant Client a RUNTIME.
        // Niente compilatore: sono librerie gratuite caricate all'avvio della connessione.
        hints.push(FixHint::new(
            "Configura l'Oracle Instant Client (da .zip)",
            "Oracle non ha un driver puro-Rust (il protocollo TNS è proprietario): Charon usa \
             le librerie ufficiali Oracle, caricate a runtime — sono solo librerie gratuite, \
             NON serve alcun compilatore né admin. Scarica UNA volta lo zip \"Basic\" (o \"Basic \
             Lite\") per il tuo OS/architettura, poi fai fare a Charon lo scompattamento e \
             l'aggancio: nessuna installazione a sistema.\n\
             • GUI: schermata Strumenti → \"Configura Instant Client\" e scegli lo .zip.\n\
             • CLI: charon oracle-setup --zip <percorso_instantclient.zip>\n\
             Download: https://www.oracle.com/database/technologies/instant-client/downloads.html",
            Some(
                "charon oracle-setup --zip ./instantclient-basiclite-linux.x64-21.zip\n\
                 # in alternativa, una cartella già estratta:\n\
                 charon oracle-setup --dir /opt/oracle/instantclient_21_12",
            ),
        ));
    }
    if rust && !native {
        // Ha il driver Rust ma non i tool nativi: chiarisci la differenza.
        hints.push(FixHint::new(
            "Tool nativi opzionali (Data Pump)",
            "Per la migrazione src→dst il driver puro-Rust è sufficiente e lavora lato client \
             (SELECT→INSERT, file in locale). I tool nativi expdp/impdp servono solo se vuoi \
             Data Pump (più fedele ma scrive il dumpfile sul SERVER, in DATA_PUMP_DIR).",
            Some("yay -S oracle-instantclient-tools oracle-instantclient-sqlplus"),
        ));
    }
    EngineReport {
        engine: Engine::Oracle,
        label: Engine::Oracle.label().into(),
        tools,
        native_available: native,
        rust_available: rust,
        note,
        hints,
    }
}

// ------------------------------------------------------------------ nativo ---

pub fn native_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("expdp").ok_or_else(|| Error::ToolMissing("expdp".into()))?;
    let file = basename(out);
    log.push(format!(
        "Nota: Data Pump crea '{file}' nel DATA_PUMP_DIR del server (non in {out})."
    ));
    let mut cmd = Command::new(&exe);
    cmd.arg(conn_str(conn))
        .arg(format!("schemas={}", conn.user.to_uppercase()))
        .arg("directory=DATA_PUMP_DIR")
        .arg(format!("dumpfile={file}"))
        .arg(format!("logfile={file}.log"))
        .arg("reuse_dumpfiles=yes");
    let display = format!(
        "expdp {} schemas={} directory=DATA_PUMP_DIR dumpfile={} reuse_dumpfiles=yes",
        conn_display(conn),
        conn.user.to_uppercase(),
        file
    );
    if plan_or_run(log, &display, &mut cmd, dry)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("expdp ha segnalato un errore (vedi log)".into()))
    }
}

pub fn native_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("impdp").ok_or_else(|| Error::ToolMissing("impdp".into()))?;
    let file = basename(input);
    let mut cmd = Command::new(&exe);
    cmd.arg(conn_str(conn))
        .arg("directory=DATA_PUMP_DIR")
        .arg(format!("dumpfile={file}"))
        .arg(format!("logfile=import-{file}.log"))
        .arg("table_exists_action=replace");
    let display = format!(
        "impdp {} directory=DATA_PUMP_DIR dumpfile={} table_exists_action=replace",
        conn_display(conn),
        file
    );
    if plan_or_run(log, &display, &mut cmd, dry)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("impdp ha segnalato un errore (vedi log)".into()))
    }
}

pub fn native_clone(
    src: &Connection,
    dst: &Connection,
    opts: &CloneOptions,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    reject_unsupported_opts(opts)?;
    let file = format!("charon-clone-{}.dmp", std::process::id());
    log.push(format!("Dump Data Pump della sorgente ({file})"));
    native_dump(src, &file, dry, log)?;

    // Import sul target, rimappando lo schema se l'utente è diverso.
    let exe = find_tool("impdp").ok_or_else(|| Error::ToolMissing("impdp".into()))?;
    let mut cmd = Command::new(&exe);
    cmd.arg(conn_str(dst))
        .arg("directory=DATA_PUMP_DIR")
        .arg(format!("dumpfile={file}"))
        .arg(format!("logfile=clone-{file}.log"))
        .arg("table_exists_action=replace");
    let src_schema = src.user.to_uppercase();
    let dst_schema = dst.user.to_uppercase();
    if src_schema != dst_schema {
        cmd.arg(format!("remap_schema={src_schema}:{dst_schema}"));
    }
    let display = format!(
        "impdp {} directory=DATA_PUMP_DIR dumpfile={} table_exists_action=replace{}",
        conn_display(dst),
        file,
        if src_schema != dst_schema {
            format!(" remap_schema={src_schema}:{dst_schema}")
        } else {
            String::new()
        }
    );
    if plan_or_run(log, &display, &mut cmd, dry)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("impdp (clone) ha segnalato un errore (vedi log)".into()))
    }
}

// ------------------------------------------------------ SQL*Loader (no-admin) ---
// Workflow descritto nella documentazione Oracle: importare pacchetti .ctl/.ldr
// (esportati da SQL Developer in formato Loader) con `sqlldr`, tabella per tabella
// nell'ordine dei vincoli FK. Gestisce anche i BLOB (via LOBFILE), che gli INSERT
// non trasferiscono. È il metodo consigliato quando NON si hanno privilegi DBA né
// accesso a DATA_PUMP_DIR.

/// Elenca i control file `.ctl` di un pacchetto, nell'ordine di caricamento.
///
/// Se nella cartella esiste `load_order.txt`, l'ordine (una voce per riga, con o
/// senza estensione `.ctl`, righe vuote e `#commenti` ignorati) viene rispettato:
/// serve a soddisfare i vincoli di primary/foreign key come da documentazione.
/// Altrimenti i file vengono ordinati alfabeticamente (con un avviso nel log).
fn control_files(dir: &Path, log: &mut Vec<String>) -> Result<Vec<std::path::PathBuf>> {
    let order_file = dir.join("load_order.txt");
    if order_file.is_file() {
        log.push("Ordine di caricamento da load_order.txt (rispetta i vincoli FK).".into());
        let text = std::fs::read_to_string(&order_file)?;
        let mut files = Vec::new();
        for raw in text.lines() {
            let name = raw.trim();
            if name.is_empty() || name.starts_with('#') {
                continue;
            }
            let fname = if name.to_lowercase().ends_with(".ctl") {
                name.to_string()
            } else {
                format!("{name}.ctl")
            };
            let p = dir.join(&fname);
            if p.is_file() {
                files.push(p);
            } else {
                log.push(format!("  ⚠ {fname} elencato ma non trovato: saltato."));
            }
        }
        return Ok(files);
    }
    // Nessun ordine esplicito: raccogliamo tutti i .ctl in ordine alfabetico.
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("ctl")).unwrap_or(false))
        .collect();
    files.sort();
    log.push(
        "⚠ Nessun load_order.txt: i .ctl sono ordinati alfabeticamente. Se ci sono vincoli FK, \
         fornisci load_order.txt con l'ordine corretto (vedi documentazione)."
            .into(),
    );
    Ok(files)
}

/// Importa un pacchetto SQL\*Loader con `sqlldr`, un control file alla volta.
pub fn sqlldr_import(conn: &Connection, package_dir: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    let dir = Path::new(package_dir);
    if !dir.is_dir() {
        return Err(Error::Msg(format!(
            "pacchetto SQL*Loader non trovato: '{package_dir}' non è una cartella"
        )));
    }
    let files = control_files(dir, log)?;
    if files.is_empty() {
        return Err(Error::Msg(format!(
            "nessun file .ctl trovato in '{package_dir}'"
        )));
    }
    // sqlldr fa parte dell'Instant Client. In dry-run non è obbligatorio (mostriamo
    // solo i comandi); in esecuzione reale invece serve.
    let exe = find_tool("sqlldr");
    if exe.is_none() {
        if dry {
            log.push("⚠ 'sqlldr' non nel PATH: in esecuzione reale serve l'Oracle Instant Client (Tools).".into());
        } else {
            return Err(Error::ToolMissing(
                "sqlldr non trovato: installa l'Oracle Instant Client (pacchetto 'Tools')".into(),
            ));
        }
    }
    crate::progress::note(log, format!("{} tabelle da caricare da {package_dir}.", files.len()));
    for ctl in &files {
        let stem = ctl.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ctl_name = ctl.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let display = format!(
            "sqlldr {} control={ctl_name} log={stem}.log",
            conn_display(conn)
        );
        // sqlldr vuole userid=user/pass@//host:port/service; il control file è
        // relativo alla cartella del pacchetto (impostiamo lì la current dir).
        let mut cmd = Command::new(exe.clone().unwrap_or_else(|| "sqlldr".into()));
        cmd.current_dir(dir)
            .arg(format!("userid={}", conn_str(conn)))
            .arg(format!("control={ctl_name}"))
            .arg(format!("log={stem}.log"));
        let outcome = plan_or_run(log, &display, &mut cmd, dry)?;
        if !dry && !outcome.success {
            return Err(Error::Cmd(format!(
                "sqlldr ha segnalato un errore su {ctl_name} (vedi {stem}.log)"
            )));
        }
    }
    // Passo obbligatorio da documentazione: riallineare le sequenze identity.
    realign_sequences_after_load(conn, dry, log);
    Ok(())
}

/// Dopo un load diretto le sequenze identity NON avanzano: vanno riportate a
/// MAX(col)+1 o i nuovi INSERT violeranno il vincolo. Col driver Oracle
/// compilato lo facciamo in automatico; altrimenti stampiamo le query da eseguire.
fn realign_sequences_after_load(conn: &Connection, dry: bool, log: &mut Vec<String>) {
    if dry {
        log.push("Dry-run: al termine verrebbero riallineate le sequenze identity (MAX(ID)+1).".into());
        return;
    }
    #[cfg(feature = "oracle-driver")]
    {
        match rustimpl::realign_sequences(conn, log) {
            Ok(n) => log.push(format!("Sequenze identity riallineate: {n}.")),
            Err(e) => log.push(format!("⚠ Riallineamento sequenze non riuscito: {e} (vedi istruzioni sotto).")),
        }
        return;
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = conn;
        log.push(
            "NB: riallinea manualmente le sequenze (questa build non ha il driver Oracle). \
             Per ogni tabella caricata, con l'utenza dello schema:"
                .into(),
        );
        log.push("  SELECT table_name, column_name, sequence_name FROM user_tab_identity_cols WHERE sequence_name IS NOT NULL;".into());
        log.push("  poi:  ALTER SEQUENCE \"<seq>\" RESTART START WITH <MAX(ID)+1>;".into());
    }
}

pub fn native_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("sqlplus").ok_or_else(|| Error::ToolMissing("sqlplus".into()))?;
    let mut cmd = Command::new(&exe);
    // -L: non richiede credenziali in caso di errore; -S: silenzioso.
    cmd.arg("-L").arg("-S").arg(conn_str(conn))
        .stdin(std::process::Stdio::null());
    let display = format!("sqlplus -L -S {}", conn_display(conn));
    let outcome = run(log, &display, &mut cmd)?;
    if outcome.success {
        Ok(())
    } else {
        Err(Error::Conn("connessione/sqlplus falliti (vedi log)".into()))
    }
}

// --------------------------------------------------------------- puro Rust ---
// Disponibile solo con la feature `oracle-driver`. Compilare NON richiede
// l'Instant Client (ODPI-C fa dlopen a runtime): le funzioni qui sotto falliscono
// con un messaggio chiaro se il client non e' installato sulla macchina.

/// Messaggio comune quando il driver Oracle non e' compilato in questa build.
#[cfg(not(feature = "oracle-driver"))]
fn no_driver() -> Error {
    Error::Unsupported(
        "questa build di Charon non include il driver Oracle (feature 'oracle-driver'): \
         usa una release ufficiale, oppure i tool nativi expdp/impdp."
            .into(),
    )
}

pub fn rust_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::dump(conn, out, dry, log);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (conn, out, dry, log);
        Err(no_driver())
    }
}
pub fn rust_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::import(conn, input, dry, log);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (conn, input, dry, log);
        Err(no_driver())
    }
}
pub fn rust_clone(
    src: &Connection,
    dst: &Connection,
    opts: &CloneOptions,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    reject_unsupported_opts(opts)?;
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::clone(src, dst, dry, log);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (src, dst, dry, log);
        Err(no_driver())
    }
}
/// Confronta due database Oracle (schema + conteggio righe). Solo puro Rust:
/// il confronto legge i cataloghi via driver, i tool nativi non c'entrano.
pub fn rust_compare(src: &Connection, dst: &Connection) -> Result<DbDiff> {
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::compare(src, dst);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (src, dst);
        Err(no_driver())
    }
}

/// Esporta i dati di tutte le tabelle dello schema Oracle in file CSV/JSON,
/// uno per tabella, dentro `out_dir`. Sola lettura: nessuna scrittura sul
/// database. Ritorna i percorsi dei file scritti.
pub fn rust_export(conn: &Connection, out_dir: &str, format: &str) -> Result<Vec<String>> {
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::export(conn, out_dir, format);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (conn, out_dir, format);
        Err(no_driver())
    }
}

/// Anteprima (read-only) delle prime `limit` righe di una tabella: stessa
/// lettura di `rust_export` (colonne + valori grezzi come `Option<String>`),
/// ma limitata e senza scrivere alcun file. Sincrona come `rust_export`.
pub fn rust_peek(conn: &Connection, table: &str, limit: u32) -> Result<(Vec<String>, Vec<Vec<Option<String>>>)> {
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::peek(conn, table, limit);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (conn, table, limit);
        Err(no_driver())
    }
}

/// Esegue una query SQL libera (SELECT o comando DML/DDL) e ne restituisce
/// l'esito in forma neutra: result set (colonne + righe, lettura grezza come
/// `Option<String>`) per le query, righe modificate per gli altri statement.
/// Sincrona come le altre `rust_*`.
pub fn rust_query(conn: &Connection, sql: &str) -> Result<QueryResult> {
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::run_query(conn, sql);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (conn, sql);
        Err(no_driver())
    }
}

/// Confronto DATI riga-per-riga di una tabella Oracle: righe accoppiate per
/// chiave primaria (o per riga intera in assenza di PK), classificate come
/// solo-sorgente / solo-destinazione / cambiate / uguali.
pub fn rust_data_diff(src: &Connection, dst: &Connection, table: &str) -> Result<TableDataDiff> {
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::data_diff(src, dst, table);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (src, dst, table);
        Err(no_driver())
    }
}

pub fn rust_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::test(conn, log);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (conn, log);
        Err(no_driver())
    }
}

/// Legge lo schema **neutro** (tabelle, colonne mappate su [`AbstractType`],
/// chiavi primarie) via driver puro-Rust: base per il confronto e la
/// clonazione cross-motore.
pub fn rust_read_schema(conn: &Connection) -> Result<SchemaModel> {
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::read_schema(conn);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = conn;
        Err(no_driver())
    }
}

/// Implementazione puro-Rust basata sul crate `oracle` (ODPI-C). Best-effort:
/// esporta schema essenziale (colonne, tipi, NOT NULL) e dati come INSERT, e
/// clona src→dst lato client (SELECT→INSERT) senza DATA_PUMP_DIR.
#[cfg(feature = "oracle-driver")]
mod rustimpl {
    use super::*;

    /// Categoria di un tipo Oracle, per scegliere SELECT e letterale SQL.
    #[derive(Clone, Copy, PartialEq)]
    enum Cat {
        Num,
        Date,
        Ts,
        Text,
    }

    /// Metadati di una colonna utili alla riscrittura.
    struct Col {
        name: String,
        dtype: String,
        len: Option<i64>,
        prec: Option<i64>,
        scale: Option<i64>,
        not_null: bool,
        cat: Cat,
    }

    fn category(dtype: &str) -> Cat {
        let d = dtype.to_uppercase();
        if d.starts_with("TIMESTAMP") {
            Cat::Ts
        } else if d == "DATE" {
            Cat::Date
        } else if d.starts_with("NUMBER")
            || d == "FLOAT"
            || d == "BINARY_FLOAT"
            || d == "BINARY_DOUBLE"
        {
            Cat::Num
        } else {
            Cat::Text
        }
    }

    fn connect_string(conn: &Connection) -> String {
        format!("{}:{}/{}", conn.host, conn.port, conn.database)
    }

    /// Apre una connessione e normalizza i formati di numero/data della sessione,
    /// cosi' i TO_CHAR producono valori riscrivibili in modo deterministico.
    fn connect(conn: &Connection) -> Result<oracle::Connection> {
        // Aggancia l'Instant Client gestito da Charon, se configurato (no-op
        // altrimenti). Va fatto PRIMA della prima connessione.
        super::ensure_client_init();
        let c = oracle::Connection::connect(&conn.user, &conn.password, connect_string(conn))
            .map_err(|e| Error::Conn(e.to_string()))?;
        let _ = c.execute("ALTER SESSION SET NLS_NUMERIC_CHARACTERS = '.,'", &[]);
        let _ = c.execute("ALTER SESSION SET NLS_DATE_FORMAT = 'YYYY-MM-DD HH24:MI:SS'", &[]);
        Ok(c)
    }

    fn list_tables(c: &oracle::Connection) -> Result<Vec<String>> {
        let rows = c
            .query(
                "SELECT table_name FROM user_tables \
                 WHERE table_name NOT LIKE 'BIN$%' ORDER BY table_name",
                &[],
            )
            .map_err(|e| Error::Msg(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            let r = r.map_err(|e| Error::Msg(e.to_string()))?;
            out.push(r.get::<usize, String>(0).map_err(|e| Error::Msg(e.to_string()))?);
        }
        Ok(out)
    }

    fn columns(c: &oracle::Connection, table: &str) -> Result<Vec<Col>> {
        // table proviene da user_tables: niente apici nel nome.
        let sql = format!(
            "SELECT column_name, data_type, data_length, data_precision, data_scale, nullable \
             FROM user_tab_columns WHERE table_name = '{table}' ORDER BY column_id"
        );
        let rows = c.query(&sql, &[]).map_err(|e| Error::Msg(e.to_string()))?;
        let mut cols = Vec::new();
        for r in rows {
            let r = r.map_err(|e| Error::Msg(e.to_string()))?;
            let g = |i: usize| r.get::<usize, String>(i).map_err(|e| Error::Msg(e.to_string()));
            let gi = |i: usize| r.get::<usize, Option<i64>>(i).map_err(|e| Error::Msg(e.to_string()));
            let name = g(0)?;
            let dtype = g(1)?;
            let len = gi(2)?;
            let prec = gi(3)?;
            let scale = gi(4)?;
            let nullable = g(5)?;
            let cat = category(&dtype);
            cols.push(Col {
                name,
                dtype,
                len,
                prec,
                scale,
                not_null: nullable == "N",
                cat,
            });
        }
        Ok(cols)
    }

    // ------------------------------------------------------------- compare ---

    /// Definizione leggibile di una colonna, usata per confrontare gli schemi.
    fn col_def(c: &Col) -> String {
        format!("{}{}", ddl_type(c), if c.not_null { " NOT NULL" } else { "" })
    }

    /// Conta le righe di una tabella. Best-effort: se non è contabile (permessi,
    /// tabella in stato strano) restituisce None invece di far fallire il diff.
    fn count_rows(c: &oracle::Connection, table: &str) -> Option<i64> {
        c.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), &[])
            .ok()
            .and_then(|r| r.get::<usize, i64>(0).ok())
    }

    /// Confronta schema e volume dati di due database Oracle.
    pub fn compare(src: &Connection, dst: &Connection) -> Result<DbDiff> {
        let s = connect(src)?;
        let d = connect(dst)?;
        let mut log = Vec::new();

        let stables = list_tables(&s)?;
        let dtables = list_tables(&d)?;
        crate::progress::note(
            &mut log,
            format!(
                "Tabelle: {} nella sorgente, {} nella destinazione",
                stables.len(),
                dtables.len()
            ),
        );

        // Unione ordinata dei nomi visti da almeno una parte.
        let mut names: Vec<String> = stables.iter().chain(dtables.iter()).cloned().collect();
        names.sort();
        names.dedup();

        let mut tables = Vec::new();
        for name in names {
            let in_s = stables.contains(&name);
            let in_d = dtables.contains(&name);

            // Tabella presente da un solo lato: tutte le colonne sono "nuove".
            if in_s != in_d {
                let (c, status) = if in_s {
                    (&s, Status::OnlySource)
                } else {
                    (&d, Status::OnlyTarget)
                };
                let cols = columns(c, &name)?;
                let columns_diff = cols
                    .iter()
                    .map(|col| {
                        let def = Some(col_def(col));
                        ColumnDiff {
                            name: col.name.clone(),
                            status,
                            source: if in_s { def.clone() } else { None },
                            target: if in_s { None } else { def },
                        }
                    })
                    .collect();
                tables.push(TableDiff {
                    name: name.clone(),
                    status,
                    columns: columns_diff,
                    source_rows: if in_s { count_rows(&s, &name) } else { None },
                    target_rows: if in_s { None } else { count_rows(&d, &name) },
                });
                continue;
            }

            // Presente da entrambe le parti: confronto colonna per colonna.
            let scols = columns(&s, &name)?;
            let dcols = columns(&d, &name)?;
            let mut cnames: Vec<String> = scols
                .iter()
                .chain(dcols.iter())
                .map(|c| c.name.clone())
                .collect();
            cnames.sort();
            cnames.dedup();

            let mut columns_diff = Vec::new();
            for cn in cnames {
                let sc = scols.iter().find(|c| c.name == cn).map(col_def);
                let dc = dcols.iter().find(|c| c.name == cn).map(col_def);
                let status = match (&sc, &dc) {
                    (Some(a), Some(b)) if a == b => Status::Same,
                    (Some(_), Some(_)) => Status::Changed,
                    (Some(_), None) => Status::OnlySource,
                    (None, Some(_)) => Status::OnlyTarget,
                    (None, None) => continue, // impossibile: il nome viene da un lato
                };
                // Le colonne identiche non entrano nel diff: su tabelle larghe
                // renderebbero illeggibile ciò che conta davvero.
                if status != Status::Same {
                    columns_diff.push(ColumnDiff {
                        name: cn,
                        status,
                        source: sc,
                        target: dc,
                    });
                }
            }

            let status = if columns_diff.is_empty() {
                Status::Same
            } else {
                Status::Changed
            };
            tables.push(TableDiff {
                name: name.clone(),
                status,
                columns: columns_diff,
                source_rows: count_rows(&s, &name),
                target_rows: count_rows(&d, &name),
            });
        }

        let diff = DbDiff {
            tables,
            source_label: connect_string(src),
            target_label: connect_string(dst),
            log,
        };
        Ok(diff)
    }

    // --------------------------------------------------------------- dati ---

    /// Colonne che compongono la chiave primaria della tabella, nell'ordine di
    /// posizione. Vuoto se la tabella non ha vincolo PK.
    fn primary_key_cols(c: &oracle::Connection, table: &str) -> Result<Vec<String>> {
        let rows = c
            .query(
                "SELECT cc.column_name FROM user_constraints c \
                 JOIN user_cons_columns cc ON cc.constraint_name = c.constraint_name \
                 WHERE c.constraint_type = 'P' AND c.table_name = :1 ORDER BY cc.position",
                &[&table],
            )
            .map_err(|e| Error::Msg(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            let r = r.map_err(|e| Error::Msg(e.to_string()))?;
            out.push(r.get::<usize, String>(0).map_err(|e| Error::Msg(e.to_string()))?);
        }
        Ok(out)
    }

    /// Legge tutte le righe di una tabella e le indicizza per chiave: la mappa
    /// associa la chiave "cruda" (valori delle colonne-chiave uniti da un
    /// separatore di controllo, per evitare ambiguità con valori che
    /// contengono ", ") alla coppia (rappresentazione delle colonne non-chiave,
    /// chiave leggibile "col=val, col=val" per il campione mostrato in UI).
    /// Se `key_cols` copre tutte le colonne (nessuna PK), la parte "valore" è
    /// vuota: si confronta l'intera riga come chiave.
    fn read_keyed_rows(
        c: &oracle::Connection,
        table: &str,
        all_cols: &[Col],
        key_names: &[String],
    ) -> Result<std::collections::HashMap<String, (String, String)>> {
        let sel = select_list(all_cols);
        let rows = c
            .query(&format!("SELECT {sel} FROM \"{table}\""), &[])
            .map_err(|e| Error::Msg(e.to_string()))?;
        let key_idx: Vec<usize> = key_names
            .iter()
            .filter_map(|k| all_cols.iter().position(|c| &c.name == k))
            .collect();

        let mut map = std::collections::HashMap::new();
        for r in rows {
            let r = r.map_err(|e| Error::Msg(e.to_string()))?;
            let mut vals = Vec::with_capacity(all_cols.len());
            for i in 0..all_cols.len() {
                let v = r
                    .get::<usize, Option<String>>(i)
                    .map_err(|e| Error::Msg(e.to_string()))?;
                // Marcatore di controllo per NULL, distinto da qualunque valore reale.
                vals.push(v.unwrap_or_else(|| "\u{2}".into()));
            }
            let key_str = key_idx
                .iter()
                .map(|&i| vals[i].as_str())
                .collect::<Vec<_>>()
                .join("\u{1}");
            let key_display = key_idx
                .iter()
                .map(|&i| format!("{}={}", all_cols[i].name, vals[i]))
                .collect::<Vec<_>>()
                .join(", ");
            let val_str = (0..all_cols.len())
                .filter(|i| !key_idx.contains(i))
                .map(|i| vals[i].as_str())
                .collect::<Vec<_>>()
                .join("\u{1}");
            map.insert(key_str, (val_str, key_display));
        }
        Ok(map)
    }

    // -------------------------------------------------------- schema neutro ---

    /// Mappa un tipo Oracle (data_type + length/precision/scale grezzi di
    /// `user_tab_columns`) sul tipo neutro [`AbstractType`], per il confronto e
    /// la clonazione cross-motore.
    fn abstract_type(dtype: &str, len: Option<i64>, prec: Option<i64>, scale: Option<i64>) -> AbstractType {
        let d = dtype.to_uppercase();
        if d.starts_with("TIMESTAMP") {
            return AbstractType::Timestamp { tz: d.contains("WITH TIME ZONE") };
        }
        match d.as_str() {
            "VARCHAR2" | "NVARCHAR2" | "CHAR" | "NCHAR" => {
                AbstractType::Text { max: len.map(|n| n as u32) }
            }
            "CLOB" | "NCLOB" | "LONG" => AbstractType::Text { max: None },
            "NUMBER" => match scale {
                // Scala positiva: numero con decimali.
                Some(s) if s > 0 => AbstractType::Decimal {
                    precision: prec.map(|p| p as u32),
                    scale: scale.map(|s| s as u32),
                },
                // Scala 0 o assente: intero, se conosciamo la precisione
                // (l'ampiezza in bit dipende dal numero di cifre).
                _ => match prec {
                    Some(p) => AbstractType::Integer {
                        bits: if p <= 4 {
                            16
                        } else if p <= 9 {
                            32
                        } else {
                            64
                        },
                    },
                    None => AbstractType::Decimal { precision: None, scale: None },
                },
            },
            "FLOAT" | "BINARY_FLOAT" => AbstractType::Float { double: false },
            "BINARY_DOUBLE" => AbstractType::Float { double: true },
            "DATE" => AbstractType::Date,
            "BLOB" => AbstractType::Binary { max: None },
            "RAW" | "LONG RAW" => AbstractType::Binary { max: len.map(|n| n as u32) },
            _ => AbstractType::Unknown { raw: dtype.to_string() },
        }
    }

    /// Auto-increment (IDENTITY, Oracle 12c+) e default grezzo delle colonne di
    /// una tabella, in una query separata da `columns()`: `IDENTITY_COLUMN` non
    /// esiste nelle versioni pre-12c e `DATA_DEFAULT` è di tipo LONG (scomodo/
    /// fragile da leggere). Se la query fallisce (versione vecchia o problemi nel
    /// leggere il LONG) ripieghiamo su "nessuna colonna auto-increment, nessun
    /// default" invece di far fallire tutta la lettura dello schema.
    fn column_extras(
        c: &oracle::Connection,
        table: &str,
    ) -> std::collections::HashMap<String, (bool, Option<String>)> {
        let sql = format!(
            "SELECT column_name, identity_column, data_default FROM user_tab_columns \
             WHERE table_name = '{table}' ORDER BY column_id"
        );
        let mut out = std::collections::HashMap::new();
        let rows = match c.query(&sql, &[]) {
            Ok(r) => r,
            Err(_) => return out, // es. IDENTITY_COLUMN assente: nessun extra, tutto a default
        };
        for row in rows {
            let Ok(row) = row else { continue };
            let Ok(name) = row.get::<usize, String>(0) else { continue };
            let auto_increment = row
                .get::<usize, Option<String>>(1)
                .ok()
                .flatten()
                .map(|v| v.trim().eq_ignore_ascii_case("YES"))
                .unwrap_or(false);
            // Se la colonna è auto-increment il default (di solito la sequenza
            // implicita dell'IDENTITY) non è significativo per il modello neutro.
            let default = if auto_increment {
                None
            } else {
                row.get::<usize, Option<String>>(2)
                    .ok()
                    .flatten()
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            };
            out.insert(name, (auto_increment, default));
        }
        out
    }

    /// Indici non-PK/non-UNIQUE-constraint di una tabella (quelli generati per i
    /// vincoli PK/UNIQUE sono già rappresentati da `primary_key`/vincoli, non
    /// vanno duplicati qui). Raggruppati per nome indice, colonne in ordine.
    fn indexes(c: &oracle::Connection, table: &str) -> Result<Vec<Index>> {
        let rows = c
            .query(
                "SELECT i.index_name, i.uniqueness, c.column_name \
                 FROM user_indexes i JOIN user_ind_columns c ON c.index_name = i.index_name \
                 WHERE i.table_name = :1 \
                   AND i.index_name NOT IN (SELECT index_name FROM user_constraints \
                     WHERE constraint_type IN ('P','U') AND index_name IS NOT NULL) \
                 ORDER BY i.index_name, c.column_position",
                &[&table],
            )
            .map_err(|e| Error::Msg(e.to_string()))?;
        let mut order: Vec<String> = Vec::new();
        let mut map: std::collections::HashMap<String, (bool, Vec<String>)> = std::collections::HashMap::new();
        for r in rows {
            let r = r.map_err(|e| Error::Msg(e.to_string()))?;
            let name = r.get::<usize, String>(0).map_err(|e| Error::Msg(e.to_string()))?;
            let uniqueness = r.get::<usize, String>(1).map_err(|e| Error::Msg(e.to_string()))?;
            let col = r.get::<usize, String>(2).map_err(|e| Error::Msg(e.to_string()))?;
            if !map.contains_key(&name) {
                order.push(name.clone());
            }
            let entry = map.entry(name).or_insert_with(|| (uniqueness == "UNIQUE", Vec::new()));
            entry.1.push(col);
        }
        Ok(order
            .into_iter()
            .map(|name| {
                let (unique, columns) = map.remove(&name).unwrap();
                Index { name, columns, unique }
            })
            .collect())
    }

    /// Foreign key di una tabella, raggruppate per nome vincolo, con le colonne
    /// locali/riferite in ordine di posizione.
    fn foreign_keys(c: &oracle::Connection, table: &str) -> Result<Vec<ForeignKey>> {
        let rows = c
            .query(
                "SELECT c.constraint_name, cc.column_name, rc.table_name AS ref_table, \
                        rcc.column_name AS ref_col \
                 FROM user_constraints c \
                 JOIN user_cons_columns cc ON cc.constraint_name = c.constraint_name \
                 JOIN user_constraints rc ON rc.constraint_name = c.r_constraint_name \
                 JOIN user_cons_columns rcc ON rcc.constraint_name = rc.constraint_name \
                   AND rcc.position = cc.position \
                 WHERE c.constraint_type = 'R' AND c.table_name = :1 \
                 ORDER BY c.constraint_name, cc.position",
                &[&table],
            )
            .map_err(|e| Error::Msg(e.to_string()))?;
        let mut order: Vec<String> = Vec::new();
        let mut map: std::collections::HashMap<String, (String, Vec<String>, Vec<String>)> =
            std::collections::HashMap::new();
        for r in rows {
            let r = r.map_err(|e| Error::Msg(e.to_string()))?;
            let name = r.get::<usize, String>(0).map_err(|e| Error::Msg(e.to_string()))?;
            let col = r.get::<usize, String>(1).map_err(|e| Error::Msg(e.to_string()))?;
            let ref_table = r.get::<usize, String>(2).map_err(|e| Error::Msg(e.to_string()))?;
            let ref_col = r.get::<usize, String>(3).map_err(|e| Error::Msg(e.to_string()))?;
            if !map.contains_key(&name) {
                order.push(name.clone());
            }
            let entry = map.entry(name).or_insert_with(|| (ref_table, Vec::new(), Vec::new()));
            entry.1.push(col);
            entry.2.push(ref_col);
        }
        Ok(order
            .into_iter()
            .map(|name| {
                let (ref_table, columns, ref_columns) = map.remove(&name).unwrap();
                ForeignKey { name, columns, ref_table, ref_columns }
            })
            .collect())
    }

    /// Legge lo schema neutro dell'intero database: tutte le tabelle
    /// dell'utente, con colonne mappate su [`AbstractType`], chiavi primarie,
    /// auto-increment/default, indici non-PK e foreign key (riusa le stesse
    /// query di `columns`/`primary_key_cols` usate dal diff).
    pub fn read_schema(conn: &Connection) -> Result<SchemaModel> {
        let c = connect(conn)?;
        let table_names = list_tables(&c)?;
        let mut tables = Vec::with_capacity(table_names.len());
        for name in table_names {
            let cols = columns(&c, &name)?;
            let pk = primary_key_cols(&c, &name)?;
            let extras = column_extras(&c, &name);
            let columns_neutre = cols
                .into_iter()
                .map(|col| {
                    let ty = abstract_type(&col.dtype, col.len, col.prec, col.scale);
                    let primary_key = pk.iter().any(|p| *p == col.name);
                    let (auto_increment, default) =
                        extras.get(&col.name).cloned().unwrap_or((false, None));
                    Column {
                        name: col.name,
                        ty,
                        nullable: !col.not_null,
                        primary_key,
                        auto_increment,
                        default,
                    }
                })
                .collect();
            let idx = indexes(&c, &name)?;
            let fks = foreign_keys(&c, &name)?;
            tables.push(Table { name, columns: columns_neutre, indexes: idx, foreign_keys: fks });
        }
        Ok(SchemaModel { tables })
    }

    /// Confronto DATI riga-per-riga di una tabella: righe accoppiate per
    /// chiave primaria (o per riga intera in assenza di PK), classificate
    /// come solo-sorgente / solo-destinazione / cambiate / uguali.
    pub fn data_diff(src: &Connection, dst: &Connection, table: &str) -> Result<TableDataDiff> {
        let s = connect(src)?;
        let d = connect(dst)?;

        // La tabella in user_tables e' in MAIUSCOLO: usiamo il nome cosi' com'e'
        // arrivato (il chiamante lo prende dalla lista tabelle, gia' coerente).
        let all_cols = columns(&s, table)?;
        if all_cols.is_empty() {
            return Err(Error::Msg(format!(
                "tabella '{table}' non trovata o senza colonne leggibili"
            )));
        }

        let pk_cols = primary_key_cols(&s, table)?;
        let no_pk = pk_cols.is_empty();
        let (key_names, note) = if no_pk {
            // Nessuna chiave primaria: la riga intera fa da chiave, e non resta
            // nulla da confrontare come "valore".
            (
                all_cols.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
                Some("nessuna chiave primaria: confronto per riga intera".to_string()),
            )
        } else {
            (pk_cols, None)
        };

        let smap = read_keyed_rows(&s, table, &all_cols, &key_names)?;
        let dmap = read_keyed_rows(&d, table, &all_cols, &key_names)?;

        let mut only_source = 0i64;
        let mut only_target = 0i64;
        let mut changed = 0i64;
        let mut same = 0i64;
        let mut sample = Vec::new();

        for (k, (sval, kdisp)) in &smap {
            match dmap.get(k) {
                None => {
                    only_source += 1;
                    if sample.len() < 50 {
                        sample.push(RowDelta { key: kdisp.clone(), kind: Status::OnlySource });
                    }
                }
                Some((dval, _)) => {
                    if dval == sval {
                        same += 1;
                    } else {
                        changed += 1;
                        if sample.len() < 50 {
                            sample.push(RowDelta { key: kdisp.clone(), kind: Status::Changed });
                        }
                    }
                }
            }
        }
        for (k, (_, kdisp)) in &dmap {
            if !smap.contains_key(k) {
                only_target += 1;
                if sample.len() < 50 {
                    sample.push(RowDelta { key: kdisp.clone(), kind: Status::OnlyTarget });
                }
            }
        }

        Ok(TableDataDiff {
            table: table.to_string(),
            // Coerente col contratto in compare.rs: vuoto se si e' confrontata
            // la riga intera (key_names qui contiene tutte le colonne, usate
            // solo internamente per l'hashing).
            key: if no_pk { Vec::new() } else { key_names },
            only_source,
            only_target,
            changed,
            same,
            sample,
            note,
        })
    }

    /// Tipo da usare in CREATE TABLE (target Oracle: riusa il tipo originale).
    fn ddl_type(c: &Col) -> String {
        let d = c.dtype.to_uppercase();
        match d.as_str() {
            "VARCHAR2" | "NVARCHAR2" | "CHAR" | "NCHAR" | "RAW" => {
                format!("{d}({})", c.len.unwrap_or(255))
            }
            "NUMBER" => match (c.prec, c.scale) {
                (Some(p), Some(s)) if s != 0 => format!("NUMBER({p},{s})"),
                (Some(p), _) => format!("NUMBER({p})"),
                _ => "NUMBER".into(),
            },
            // DATE, TIMESTAMP(..), CLOB, BLOB, FLOAT, ...: data_type e' gia' completo.
            _ => c.dtype.clone(),
        }
    }

    fn create_sql(table: &str, cols: &[Col]) -> String {
        let defs: Vec<String> = cols
            .iter()
            .map(|c| {
                let mut d = format!("  \"{}\" {}", c.name, ddl_type(c));
                if c.not_null {
                    d.push_str(" NOT NULL");
                }
                d
            })
            .collect();
        format!("CREATE TABLE \"{table}\" (\n{}\n)", defs.join(",\n"))
    }

    /// Espressione SELECT che converte la colonna in testo riscrivibile.
    fn select_expr(c: &Col) -> String {
        match c.cat {
            Cat::Num => format!("TO_CHAR(\"{}\")", c.name),
            Cat::Date => format!("TO_CHAR(\"{}\", 'YYYY-MM-DD HH24:MI:SS')", c.name),
            Cat::Ts => format!("TO_CHAR(\"{}\", 'YYYY-MM-DD HH24:MI:SS.FF')", c.name),
            Cat::Text => format!("\"{}\"", c.name),
        }
    }

    fn esc(s: &str) -> String {
        s.replace('\'', "''")
    }

    /// Letterale SQL Oracle per un valore (gia' convertito a stringa via TO_CHAR).
    fn literal(cat: Cat, v: Option<String>) -> String {
        match v {
            None => "NULL".into(),
            Some(s) => match cat {
                Cat::Num => {
                    if s.is_empty() {
                        "NULL".into()
                    } else {
                        s
                    }
                }
                Cat::Date => format!("TO_DATE('{}', 'YYYY-MM-DD HH24:MI:SS')", esc(&s)),
                Cat::Ts => format!("TO_TIMESTAMP('{}', 'YYYY-MM-DD HH24:MI:SS.FF')", esc(&s)),
                Cat::Text => format!("'{}'", esc(&s)),
            },
        }
    }

    fn col_list(cols: &[Col]) -> String {
        cols.iter()
            .map(|c| format!("\"{}\"", c.name))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn select_list(cols: &[Col]) -> String {
        cols.iter().map(select_expr).collect::<Vec<_>>().join(", ")
    }

    /// Costruisce la lista di valori di una riga (gia' fetchata come testo).
    fn row_values(row: &oracle::Row, cols: &[Col]) -> Result<String> {
        let mut vals = Vec::with_capacity(cols.len());
        for (i, c) in cols.iter().enumerate() {
            let v = row
                .get::<usize, Option<String>>(i)
                .map_err(|e| Error::Msg(e.to_string()))?;
            vals.push(literal(c.cat, v));
        }
        Ok(vals.join(", "))
    }

    // -------------------------------------------------------------- operazioni ---

    pub fn test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
        log.push(format!("Connessione Oracle a {} …", connect_string(conn)));
        let c = connect(conn)?;
        let rows = c
            .query("SELECT 1 FROM dual", &[])
            .map_err(|e| Error::Conn(e.to_string()))?;
        for r in rows {
            r.map_err(|e| Error::Conn(e.to_string()))?;
        }
        log.push("SELECT 1 FROM dual riuscito.".into());
        Ok(())
    }

    fn dump_sql(conn: &Connection, log: &mut Vec<String>) -> Result<String> {
        let c = connect(conn)?;
        let tables = list_tables(&c)?;
        crate::progress::note(log, format!("Trovate {} tabelle nello schema {}.", tables.len(), conn.user));

        let mut out = String::new();
        out.push_str("-- Dump generato da Charon — driver Oracle puro-Rust (best-effort).\n");
        out.push_str("-- Schema essenziale (colonne + NOT NULL) e dati come INSERT.\n");
        out.push_str("-- Per fedeltà completa (indici, vincoli, sequenze) usa Data Pump.\n\n");

        for table in &tables {
            let cols = columns(&c, table)?;
            if cols.is_empty() {
                continue;
            }
            out.push_str(&format!("DROP TABLE \"{table}\" CASCADE CONSTRAINTS;\n"));
            out.push_str(&create_sql(table, &cols));
            out.push_str(";\n");

            let collist = col_list(&cols);
            let sel = select_list(&cols);
            let rows = c
                .query(&format!("SELECT {sel} FROM \"{table}\""), &[])
                .map_err(|e| Error::Msg(e.to_string()))?;
            let mut n = 0u64;
            for r in rows {
                let r = r.map_err(|e| Error::Msg(e.to_string()))?;
                let vals = row_values(&r, &cols)?;
                out.push_str(&format!("INSERT INTO \"{table}\" ({collist}) VALUES ({vals});\n"));
                n += 1;
            }
            out.push('\n');
            crate::progress::note(log, format!("  {table}: {n} righe."));
        }
        Ok(out)
    }

    pub fn dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
        let sql = dump_sql(conn, log)?;
        let tables = sql.matches("CREATE TABLE ").count();
        let rows = sql.matches("INSERT INTO ").count();
        if dry {
            log.push(format!(
                "Dry-run: pronto un dump di {tables} tabelle / {rows} INSERT ({} byte). File {out} NON scritto.",
                sql.len()
            ));
            return Ok(());
        }
        std::fs::write(out, sql)?;
        log.push(format!("Dump SQL scritto in {out}"));
        Ok(())
    }

    /// Esporta i dati di ogni tabella dello schema in un file CSV/JSON separato
    /// dentro `out_dir` (uno per tabella, nome `<tabella>.<estensione>`). Sola
    /// lettura: nessuna scrittura sul database. I formati NLS di sessione sono
    /// già normalizzati da [`connect`], così i valori numerici/data letti come
    /// testo sono deterministici. Ritorna i percorsi dei file scritti.
    pub fn export(conn: &Connection, out_dir: &str, format: &str) -> Result<Vec<String>> {
        let fmt = crate::export::DataFormat::from_str(format)
            .ok_or_else(|| Error::Unsupported("formato non supportato".into()))?;
        std::fs::create_dir_all(out_dir)?;

        let c = connect(conn)?;
        let tables = list_tables(&c)?;

        let mut files = Vec::new();
        for table in &tables {
            let cols = columns(&c, table)?;
            if cols.is_empty() {
                continue;
            }
            let colnames: Vec<String> = cols.iter().map(|col| col.name.clone()).collect();
            // select_list converte NUMBER/DATE/TIMESTAMP in testo (TO_CHAR) così
            // il valore letto è già una rappresentazione stabile; niente letterali
            // SQL qui, solo il valore grezzo per riga/colonna.
            let sel = select_list(&cols);
            let query_rows = c
                .query(&format!("SELECT {sel} FROM \"{table}\""), &[])
                .map_err(|e| Error::Msg(e.to_string()))?;

            let mut rows: Vec<Vec<Option<String>>> = Vec::new();
            for r in query_rows {
                let r = r.map_err(|e| Error::Msg(e.to_string()))?;
                let mut vals = Vec::with_capacity(cols.len());
                for i in 0..cols.len() {
                    let v = r
                        .get::<usize, Option<String>>(i)
                        .map_err(|e| Error::Msg(e.to_string()))?;
                    vals.push(v);
                }
                rows.push(vals);
            }

            let rendered = crate::export::render(fmt, &colnames, &rows);
            let path = std::path::Path::new(out_dir).join(format!("{table}.{}", fmt.ext()));
            std::fs::write(&path, rendered)?;
            files.push(path.display().to_string());
        }
        Ok(files)
    }

    /// Anteprima read-only delle prime `limit` righe di una tabella: stessa
    /// lettura di `export` (`columns()` + `select_list`, valori come
    /// `Option<String>` grezzi), ma con `FETCH FIRST ... ROWS ONLY` (Oracle
    /// 12c+) al posto del ciclo su tutte le righe, e senza scrivere file.
    pub fn peek(conn: &Connection, table: &str, limit: u32) -> Result<(Vec<String>, Vec<Vec<Option<String>>>)> {
        let c = connect(conn)?;
        let cols = columns(&c, table)?;
        let colnames: Vec<String> = cols.iter().map(|col| col.name.clone()).collect();

        let sel = select_list(&cols);
        let query_rows = c
            .query(
                &format!("SELECT {sel} FROM \"{table}\" FETCH FIRST {limit} ROWS ONLY"),
                &[],
            )
            .map_err(|e| Error::Msg(format!("{table}: {e}")))?;

        let mut rows: Vec<Vec<Option<String>>> = Vec::new();
        for r in query_rows {
            let r = r.map_err(|e| Error::Msg(format!("{table}: {e}")))?;
            let mut vals = Vec::with_capacity(cols.len());
            for i in 0..cols.len() {
                let v = r
                    .get::<usize, Option<String>>(i)
                    .map_err(|e| Error::Msg(format!("{table}: {e}")))?;
                vals.push(v);
            }
            rows.push(vals);
        }
        Ok((colnames, rows))
    }

    /// Esegue una query SQL libera. Prepariamo lo statement per capire se e'
    /// una query (`is_query`, in base al tipo rilevato dal parser Oracle):
    /// per un SELECT leggiamo il result set (nomi colonna da `column_info`,
    /// valori grezzi come `Option<String>`, come `peek`/`export`); per un
    /// DML/DDL eseguiamo e leggiamo le righe modificate da `row_count`, poi
    /// facciamo il commit esplicito (il driver Oracle non e' in autocommit).
    pub fn run_query(conn: &Connection, sql: &str) -> Result<QueryResult> {
        use crate::model::QueryResult;

        let c = connect(conn)?;
        let mut stmt = c.statement(sql).build().map_err(|e| Error::Msg(e.to_string()))?;
        if stmt.is_query() {
            let rows = stmt.query(&[]).map_err(|e| Error::Msg(e.to_string()))?;
            let columns: Vec<String> = rows
                .column_info()
                .iter()
                .map(|ci| ci.name().to_string())
                .collect();
            let mut data: Vec<Vec<Option<String>>> = Vec::new();
            for row in rows {
                let row = row.map_err(|e| Error::Msg(e.to_string()))?;
                let mut vals = Vec::with_capacity(columns.len());
                for i in 0..columns.len() {
                    let v = row
                        .get::<usize, Option<String>>(i)
                        .map_err(|e| Error::Msg(e.to_string()))?;
                    vals.push(v);
                }
                data.push(vals);
            }
            let message = format!("{} righe", data.len());
            Ok(QueryResult { columns, rows: data, affected: None, message })
        } else {
            let n = c.execute(sql, &[]).map_err(|e| Error::Msg(e.to_string()))?;
            let aff = n.row_count().unwrap_or(0);
            c.commit().map_err(|e| Error::Msg(e.to_string()))?;
            Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                affected: Some(aff),
                message: format!("Eseguito · {aff} righe modificate"),
            })
        }
    }

    pub fn clone(src: &Connection, dst: &Connection, dry: bool, log: &mut Vec<String>) -> Result<()> {
        let s = connect(src)?;
        let tables = list_tables(&s)?;
        if dry {
            log.push(format!(
                "Dry-run: verrebbero clonate {} tabelle (SELECT→INSERT, lato client) su {}. Destinazione non modificata:",
                tables.len(),
                connect_string(dst)
            ));
            for table in &tables {
                let n = s
                    .query_row_as::<i64>(&format!("SELECT COUNT(*) FROM \"{table}\""), &[])
                    .unwrap_or(0);
                log.push(format!("  {table}: {n} righe da copiare"));
            }
            log.push("Al termine verrebbero riallineate le sequenze identity (MAX+1).".into());
            return Ok(());
        }
        let d = connect(dst)?;
        crate::progress::note(log, format!("Clonazione di {} tabelle (SELECT→INSERT, lato client).", tables.len()));

        for table in &tables {
            let cols = columns(&s, table)?;
            if cols.is_empty() {
                continue;
            }
            // Best-effort: elimina la tabella di destinazione se esiste.
            let _ = d.execute(&format!("DROP TABLE \"{table}\" CASCADE CONSTRAINTS"), &[]);
            d.execute(&create_sql(table, &cols), &[])
                .map_err(|e| Error::Msg(format!("CREATE {table}: {e}")))?;

            let collist = col_list(&cols);
            let sel = select_list(&cols);
            let rows = s
                .query(&format!("SELECT {sel} FROM \"{table}\""), &[])
                .map_err(|e| Error::Msg(e.to_string()))?;
            let mut n = 0u64;
            for r in rows {
                let r = r.map_err(|e| Error::Msg(e.to_string()))?;
                let vals = row_values(&r, &cols)?;
                d.execute(
                    &format!("INSERT INTO \"{table}\" ({collist}) VALUES ({vals})"),
                    &[],
                )
                .map_err(|e| Error::Msg(format!("INSERT {table}: {e}")))?;
                n += 1;
            }
            d.commit().map_err(|e| Error::Msg(e.to_string()))?;
            crate::progress::note(log, format!("  {table}: {n} righe copiate."));
        }
        // Il clone ricrea le tabelle: se la destinazione usa sequenze/identity,
        // riallineale (best-effort, non blocca se non applicabile).
        match realign_sequences(dst, log) {
            Ok(n) if n > 0 => log.push(format!("Sequenze identity riallineate: {n}.")),
            _ => {}
        }
        Ok(())
    }

    pub fn import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
        let sql = std::fs::read_to_string(input)?;
        let stmts = split_statements(&sql);
        if dry {
            log.push(format!(
                "Dry-run: {} statement da {input} verrebbero eseguiti. Nessuna modifica applicata.",
                stmts.len()
            ));
            return Ok(());
        }
        log.push(format!("Esecuzione di {} statement da {input}…", stmts.len()));
        let c = connect(conn)?;
        let mut errs = 0u64;
        for stmt in &stmts {
            let trimmed = stmt.trim();
            if trimmed.is_empty() {
                continue;
            }
            match c.execute(trimmed, &[]) {
                Ok(_) => {}
                Err(e) => {
                    // I DROP su tabelle inesistenti sono attesi: non bloccano.
                    if trimmed.to_uppercase().starts_with("DROP") {
                        log.push(format!("  (ignorato) {e}"));
                    } else {
                        errs += 1;
                        log.push(format!("  ERRORE: {e}"));
                    }
                }
            }
        }
        c.commit().map_err(|e| Error::Msg(e.to_string()))?;
        if errs > 0 {
            return Err(Error::Cmd(format!("import completato con {errs} errori (vedi log)")));
        }
        log.push("Import completato.".into());
        Ok(())
    }

    /// Riporta ogni sequenza identity dello schema a `MAX(colonna)+1`, come
    /// prescritto dalla documentazione dopo un load diretto (SQL\*Loader o
    /// INSERT): senza questo passaggio i nuovi inserimenti violerebbero il
    /// vincolo di unicità sulla colonna ID. Ritorna quante sequenze ha aggiornato.
    ///
    /// Best-effort: le tabelle vuote vengono saltate; un ALTER non riuscito viene
    /// registrato ma non interrompe le altre.
    pub fn realign_sequences(conn: &Connection, log: &mut Vec<String>) -> Result<usize> {
        let c = connect(conn)?;
        // Query dalla documentazione: sequenza + colonna identity per tabella.
        let rows = c
            .query(
                "SELECT table_name, column_name, sequence_name FROM user_tab_identity_cols \
                 WHERE sequence_name IS NOT NULL",
                &[],
            )
            .map_err(|e| Error::Msg(e.to_string()))?;
        let mut targets: Vec<(String, String, String)> = Vec::new();
        for r in rows {
            let r = r.map_err(|e| Error::Msg(e.to_string()))?;
            let g = |i: usize| r.get::<usize, String>(i).map_err(|e| Error::Msg(e.to_string()));
            targets.push((g(0)?, g(1)?, g(2)?));
        }
        let mut n = 0usize;
        for (table, col, seq) in targets {
            let max: Option<i64> = c
                .query_row_as::<Option<i64>>(&format!("SELECT MAX(\"{col}\") FROM \"{table}\""), &[])
                .ok()
                .flatten();
            let Some(max) = max else {
                continue; // tabella vuota: la sequenza resta com'è
            };
            let next = max + 1;
            // Oracle 12.2+: RESTART riporta la sequenza al valore indicato.
            match c.execute(&format!("ALTER SEQUENCE \"{seq}\" RESTART START WITH {next}"), &[]) {
                Ok(_) => {
                    log.push(format!("  seq {seq} → {next} (da {table}.{col})"));
                    n += 1;
                }
                Err(e) => log.push(format!("  ⚠ ALTER SEQUENCE {seq}: {e}")),
            }
        }
        Ok(n)
    }

    /// Divide uno script SQL in statement sui ';', rispettando le stringhe tra
    /// apici singoli e ignorando i commenti di riga `--`. Niente PL/SQL.
    fn split_statements(sql: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = String::new();
        let mut in_str = false;
        let mut chars = sql.chars().peekable();
        while let Some(ch) = chars.next() {
            if in_str {
                cur.push(ch);
                if ch == '\'' {
                    // '' = apice escapato dentro la stringa.
                    if chars.peek() == Some(&'\'') {
                        cur.push(chars.next().unwrap());
                    } else {
                        in_str = false;
                    }
                }
                continue;
            }
            match ch {
                '\'' => {
                    in_str = true;
                    cur.push(ch);
                }
                '-' if chars.peek() == Some(&'-') => {
                    // commento di riga: salta fino a fine riga.
                    chars.next();
                    for c2 in chars.by_ref() {
                        if c2 == '\n' {
                            break;
                        }
                    }
                }
                ';' => {
                    if !cur.trim().is_empty() {
                        out.push(cur.trim().to_string());
                    }
                    cur.clear();
                }
                _ => cur.push(ch),
            }
        }
        if !cur.trim().is_empty() {
            out.push(cur.trim().to_string());
        }
        out
    }
}

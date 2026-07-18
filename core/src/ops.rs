//! Orchestratore: sceglie il metodo (nativo/puro Rust) e instrada l'operazione
//! verso il motore giusto. E' l'unico punto che la UI deve conoscere.

use crate::model::*;
use crate::{mssql, mysql, oracle, postgres, sqlite, tunnel, Error, Result};

/// Riepilogo di cosa e' disponibile sulla macchina, per tutti i motori.
pub fn detect_all() -> Vec<EngineReport> {
    vec![
        postgres::report(),
        oracle::report(),
        mssql::report(),
        sqlite::report(),
        mysql::report(),
    ]
}

/// (nativo_disponibile, rust_disponibile) per un motore.
fn availability(engine: Engine) -> (bool, bool) {
    match engine {
        Engine::Postgres => (postgres::native_available(), postgres::rust_available()),
        Engine::Sqlserver => (mssql::native_available(), mssql::rust_available()),
        Engine::Oracle => (oracle::native_available(), oracle::rust_available()),
        Engine::Sqlite => (sqlite::native_available(), sqlite::rust_available()),
        Engine::Mysql => (mysql::native_available(), mysql::rust_available()),
    }
}

/// Decide il metodo da usare in base alla preferenza e a cosa e' disponibile.
fn choose(prefer: Prefer, native: bool, rust: bool) -> Result<Method> {
    match prefer {
        Prefer::Native => {
            if native {
                Ok(Method::Native)
            } else {
                Err(Error::ToolMissing(
                    "i tool nativi richiesti non sono installati".into(),
                ))
            }
        }
        Prefer::Rust => {
            if rust {
                Ok(Method::Rust)
            } else {
                Err(Error::Unsupported(
                    "fallback puro Rust non disponibile per questo motore".into(),
                ))
            }
        }
        Prefer::Auto => {
            if native {
                Ok(Method::Native)
            } else if rust {
                Ok(Method::Rust)
            } else {
                Err(Error::ToolMissing(
                    "nessun metodo disponibile: installa i tool client del database".into(),
                ))
            }
        }
    }
}

/// Esegue il blocco scelto e impacchetta l'esito in un [`OpResult`].
fn finalize(
    engine: Engine,
    prefer: Prefer,
    success_msg: String,
    f: impl FnOnce(Method, &mut Vec<String>) -> Result<()>,
) -> OpResult {
    let mut log = Vec::new();
    let (native, rust) = availability(engine);
    let method = match choose(prefer, native, rust) {
        Ok(m) => m,
        Err(e) => {
            return OpResult {
                ok: false,
                method: Method::Native,
                message: e.to_string(),
                artifact: None,
                log,
            }
        }
    };
    crate::progress::note(&mut log, format!("Metodo selezionato: {}", method.label()));
    match f(method, &mut log) {
        Ok(()) => OpResult::ok(method, success_msg, log),
        Err(e) => OpResult {
            ok: false,
            method,
            message: e.to_string(),
            artifact: None,
            log,
        },
    }
}

fn dispatch_dump(
    conn: &Connection,
    out: &str,
    m: Method,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    match (conn.engine, m) {
        (Engine::Postgres, Method::Native) => postgres::native_dump(conn, out, dry, log),
        (Engine::Postgres, Method::Rust) => postgres::rust_dump(conn, out, dry, log),
        (Engine::Sqlserver, Method::Native) => mssql::native_dump(conn, out, dry, log),
        (Engine::Sqlserver, Method::Rust) => mssql::rust_dump(conn, out, dry, log),
        (Engine::Oracle, Method::Native) => oracle::native_dump(conn, out, dry, log),
        (Engine::Oracle, Method::Rust) => oracle::rust_dump(conn, out, dry, log),
        (Engine::Sqlite, Method::Native) => sqlite::native_dump(conn, out, dry, log),
        (Engine::Sqlite, Method::Rust) => sqlite::rust_dump(conn, out, dry, log),
        (Engine::Mysql, Method::Native) => mysql::native_dump(conn, out, dry, log),
        (Engine::Mysql, Method::Rust) => mysql::rust_dump(conn, out, dry, log),
    }
}

fn dispatch_import(
    conn: &Connection,
    input: &str,
    m: Method,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    match (conn.engine, m) {
        (Engine::Postgres, Method::Native) => postgres::native_import(conn, input, dry, log),
        (Engine::Postgres, Method::Rust) => postgres::rust_import(conn, input, dry, log),
        (Engine::Sqlserver, Method::Native) => mssql::native_import(conn, input, dry, log),
        (Engine::Sqlserver, Method::Rust) => mssql::rust_import(conn, input, dry, log),
        (Engine::Oracle, Method::Native) => oracle::native_import(conn, input, dry, log),
        (Engine::Oracle, Method::Rust) => oracle::rust_import(conn, input, dry, log),
        (Engine::Sqlite, Method::Native) => sqlite::native_import(conn, input, dry, log),
        (Engine::Sqlite, Method::Rust) => sqlite::rust_import(conn, input, dry, log),
        (Engine::Mysql, Method::Native) => mysql::native_import(conn, input, dry, log),
        (Engine::Mysql, Method::Rust) => mysql::rust_import(conn, input, dry, log),
    }
}

fn dispatch_clone(
    src: &Connection,
    dst: &Connection,
    m: Method,
    opts: &CloneOptions,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    match (src.engine, m) {
        (Engine::Postgres, Method::Native) => postgres::native_clone(src, dst, opts, dry, log),
        (Engine::Postgres, Method::Rust) => postgres::rust_clone(src, dst, opts, dry, log),
        (Engine::Sqlserver, Method::Native) => mssql::native_clone(src, dst, opts, dry, log),
        (Engine::Sqlserver, Method::Rust) => mssql::rust_clone(src, dst, opts, dry, log),
        (Engine::Oracle, Method::Native) => oracle::native_clone(src, dst, opts, dry, log),
        (Engine::Oracle, Method::Rust) => oracle::rust_clone(src, dst, opts, dry, log),
        (Engine::Sqlite, Method::Native) => sqlite::native_clone(src, dst, opts, dry, log),
        (Engine::Sqlite, Method::Rust) => sqlite::rust_clone(src, dst, opts, dry, log),
        (Engine::Mysql, Method::Native) => mysql::native_clone(src, dst, opts, dry, log),
        (Engine::Mysql, Method::Rust) => mysql::rust_clone(src, dst, opts, dry, log),
    }
}

fn dispatch_test(conn: &Connection, m: Method, log: &mut Vec<String>) -> Result<()> {
    match (conn.engine, m) {
        (Engine::Postgres, Method::Native) => postgres::native_test(conn, log),
        (Engine::Postgres, Method::Rust) => postgres::rust_test(conn, log),
        (Engine::Sqlserver, Method::Native) => mssql::native_test(conn, log),
        (Engine::Sqlserver, Method::Rust) => mssql::rust_test(conn, log),
        (Engine::Oracle, Method::Native) => oracle::native_test(conn, log),
        (Engine::Oracle, Method::Rust) => oracle::rust_test(conn, log),
        (Engine::Sqlite, Method::Native) => sqlite::native_test(conn, log),
        (Engine::Sqlite, Method::Rust) => sqlite::rust_test(conn, log),
        (Engine::Mysql, Method::Native) => mysql::native_test(conn, log),
        (Engine::Mysql, Method::Rust) => mysql::rust_test(conn, log),
    }
}

/// Se la connessione usa un tunnel SSH, lo apre e restituisce una connessione
/// "effettiva" che punta al forward locale (più la guardia da tenere viva).
/// Senza SSH, ritorna la connessione invariata.
fn prepare(conn: &Connection) -> Result<(Connection, Option<tunnel::TunnelGuard>)> {
    if conn.ssh.is_none() {
        return Ok((conn.clone(), None));
    }
    // Un motore su file è locale: inoltrare una porta TCP non lo raggiungerebbe.
    if conn.engine.is_file_based() {
        return Err(Error::Unsupported(format!(
            "{} è un file locale: il tunnel SSH non si applica",
            conn.engine.label()
        )));
    }
    #[cfg(feature = "ssh-tunnel")]
    {
        let guard = tunnel::open(conn)?;
        let mut eff = conn.clone();
        eff.host = guard.local_host.clone();
        eff.port = guard.local_port;
        eff.ssh = None;
        Ok((eff, Some(guard)))
    }
    #[cfg(not(feature = "ssh-tunnel"))]
    {
        Err(Error::Unsupported(
            "supporto SSH non compilato in questa build".into(),
        ))
    }
}

/// OpResult di errore "precoce" (es. tunnel non apribile), prima di scegliere il metodo.
fn early_error(e: Error) -> OpResult {
    OpResult {
        ok: false,
        method: Method::Native,
        message: e.to_string(),
        artifact: None,
        log: Vec::new(),
    }
}

/// Prefissa il log con la modalità dry-run, quando attiva.
fn note_dry(dry: bool, log: &mut Vec<String>) {
    if dry {
        crate::progress::note(log, "── DRY-RUN: anteprima, nessuna modifica verrà applicata ──");
    }
}

/// Crea un dump del database in `out`. Con `dry`, mostra solo cosa verrebbe fatto.
pub fn dump(conn: &Connection, out: &str, prefer: Prefer, dry: bool) -> OpResult {
    let (conn, _guard) = match prepare(conn) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    let msg = if dry {
        format!("Dry-run dump → {out} (nessun file scritto)")
    } else {
        format!("Dump completato → {out}")
    };
    let res = finalize(conn.engine, prefer, msg, |m, log| {
        note_dry(dry, log);
        dispatch_dump(&conn, out, m, dry, log)
    });
    if res.ok && !dry {
        res.with_artifact(out)
    } else {
        res
    }
}

/// Importa un dump `input` nel database. Con `dry`, mostra solo cosa verrebbe fatto.
pub fn import(conn: &Connection, input: &str, prefer: Prefer, dry: bool) -> OpResult {
    let (conn, _guard) = match prepare(conn) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    let msg = if dry {
        "Dry-run import (nessuna modifica applicata)".into()
    } else {
        "Import completato".into()
    };
    finalize(conn.engine, prefer, msg, |m, log| {
        note_dry(dry, log);
        dispatch_import(&conn, input, m, dry, log)
    })
}

/// Clona il database `src` su `dst` (devono essere dello stesso motore).
/// Con `dry`, ispeziona la sorgente e mostra il piano senza toccare la destinazione.
pub fn clone(
    src: &Connection,
    dst: &Connection,
    prefer: Prefer,
    opts: &CloneOptions,
    dry: bool,
) -> OpResult {
    if src.engine != dst.engine {
        return OpResult {
            ok: false,
            method: Method::Native,
            message: "sorgente e destinazione devono usare lo stesso motore di database".into(),
            artifact: None,
            log: Vec::new(),
        };
    }
    // Apre i tunnel SSH (se configurati) per sorgente e destinazione.
    let (src, _gs) = match prepare(src) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    let (dst, _gd) = match prepare(dst) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    // Il mascheramento riscrive i valori riga per riga: possibile solo col puro
    // Rust. Anche il data-only per SQL Server è implementato solo nel fallback puro
    // Rust (i tool nativi non lo gestiscono). In questi casi forziamo quel metodo a
    // prescindere dalla preferenza.
    let force_rust = opts.has_mask() || (src.engine == Engine::Sqlserver && opts.data_only);
    let effective = if force_rust { Prefer::Rust } else { prefer };
    let msg = if dry {
        "Dry-run clonazione (destinazione non modificata)".into()
    } else {
        "Clonazione completata".into()
    };
    finalize(src.engine, effective, msg, |m, log| {
        note_dry(dry, log);
        if opts.has_mask() && m != Method::Rust {
            return Err(Error::Unsupported(
                "il mascheramento richiede il metodo puro Rust".into(),
            ));
        }
        dispatch_clone(&src, &dst, m, opts, dry, log)
    })
}

/// Importa un **pacchetto SQL\*Loader** (cartella con file `.ctl`/`.ldr`, come
/// esportato da SQL Developer in formato Loader) invocando `sqlldr` per ogni
/// tabella nell'ordine dato. È il workflow no-admin descritto nella
/// documentazione Oracle: gestisce anche i BLOB (via LOBFILE) che gli `INSERT`
/// non possono trasferire. Con `dry`, stampa i comandi `sqlldr` senza eseguirli.
///
/// Al termine (se non in dry-run e col driver Oracle disponibile) riallinea le
/// sequenze identity, passaggio obbligatorio dopo un load diretto.
pub fn oracle_load(conn: &Connection, package_dir: &str, dry: bool) -> OpResult {
    if conn.engine != Engine::Oracle {
        return OpResult {
            ok: false,
            method: Method::Native,
            message: "oracle-load è specifico del motore Oracle".into(),
            artifact: None,
            log: Vec::new(),
        };
    }
    let (conn, _guard) = match prepare(conn) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    let mut log = Vec::new();
    note_dry(dry, &mut log);
    let msg: String = if dry {
        "Dry-run SQL*Loader (nessun caricamento eseguito)".into()
    } else {
        "Caricamento SQL*Loader completato".into()
    };
    match oracle::sqlldr_import(&conn, package_dir, dry, &mut log) {
        Ok(()) => OpResult::ok(Method::Native, msg, log),
        Err(e) => OpResult {
            ok: false,
            method: Method::Native,
            message: e.to_string(),
            artifact: None,
            log,
        },
    }
}

/// Configura l'**Oracle Instant Client** partendo da uno `.zip` ufficiale (o da
/// una cartella già estratta): Charon lo scompatta in una cartella dell'utente
/// (nessun admin), verifica che contenga le librerie per il sistema corrente e
/// ricorda il percorso. Da lì in poi le operazioni Oracle puro-Rust lo usano.
pub fn oracle_setup(path: &str) -> OpResult {
    // `mut` serve solo nel ramo con la feature Oracle (provision scrive nel log).
    #[allow(unused_mut)]
    let mut log = Vec::new();
    #[cfg(feature = "oracle-driver")]
    {
        match oracle::provision(path, &mut log) {
            Ok(dir) => OpResult::ok(
                Method::Rust,
                format!("Instant Client configurato: {}", dir.display()),
                log,
            )
            .with_artifact(dir.display().to_string()),
            Err(e) => OpResult {
                ok: false,
                method: Method::Rust,
                message: e.to_string(),
                artifact: None,
                log,
            },
        }
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = path;
        OpResult {
            ok: false,
            method: Method::Rust,
            message: "questa build non include il driver Oracle: usa una release ufficiale \
                      (o compila con --features oracle)"
                .into(),
            artifact: None,
            log,
        }
    }
}

/// Confronta due database dello stesso motore e restituisce il diff (schema +
/// conteggio righe). **Sola lettura**: non modifica nessuno dei due lati.
///
/// Implementato per tutti i motori (PostgreSQL, Oracle, SQL Server, SQLite,
/// MySQL/MariaDB) via il fallback puro Rust: legge i cataloghi, i tool nativi
/// non c'entrano.
pub fn compare(src: &Connection, dst: &Connection) -> Result<crate::compare::DbDiff> {
    if src.engine != dst.engine {
        return Err(Error::Unsupported(
            "il confronto richiede due database dello stesso motore".into(),
        ));
    }
    let (src, _gs) = prepare(src)?;
    let (dst, _gd) = prepare(dst)?;
    match src.engine {
        Engine::Postgres => postgres::rust_compare(&src, &dst),
        Engine::Oracle => oracle::rust_compare(&src, &dst),
        Engine::Sqlserver => mssql::rust_compare(&src, &dst),
        Engine::Sqlite => sqlite::rust_compare(&src, &dst),
        Engine::Mysql => mysql::rust_compare(&src, &dst),
    }
}

/// Confronto **dati** (riga per riga, per chiave primaria) di una singola
/// tabella fra due database dello stesso motore. Sola lettura. Più pesante del
/// confronto di schema: si esegue su richiesta per la tabella scelta.
pub fn compare_data(
    src: &Connection,
    dst: &Connection,
    table: &str,
) -> Result<crate::compare::TableDataDiff> {
    if src.engine != dst.engine {
        return Err(Error::Unsupported(
            "il confronto dati richiede due database dello stesso motore".into(),
        ));
    }
    let (src, _gs) = prepare(src)?;
    let (dst, _gd) = prepare(dst)?;
    match src.engine {
        Engine::Postgres => postgres::rust_data_diff(&src, &dst, table),
        Engine::Oracle => oracle::rust_data_diff(&src, &dst, table),
        Engine::Sqlserver => mssql::rust_data_diff(&src, &dst, table),
        Engine::Sqlite => sqlite::rust_data_diff(&src, &dst, table),
        Engine::Mysql => mysql::rust_data_diff(&src, &dst, table),
    }
}

/// Verifica la connessione al database.
pub fn test_connection(conn: &Connection, prefer: Prefer) -> OpResult {
    let (conn, _guard) = match prepare(conn) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    finalize(
        conn.engine,
        prefer,
        "Connessione riuscita".into(),
        |m, log| dispatch_test(&conn, m, log),
    )
}

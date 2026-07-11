//! Orchestratore: sceglie il metodo (nativo/puro Rust) e instrada l'operazione
//! verso il motore giusto. E' l'unico punto che la UI deve conoscere.

use crate::model::*;
use crate::{mssql, oracle, postgres, Error, Result};

/// Riepilogo di cosa e' disponibile sulla macchina, per tutti i motori.
pub fn detect_all() -> Vec<EngineReport> {
    vec![postgres::report(), oracle::report(), mssql::report()]
}

/// (nativo_disponibile, rust_disponibile) per un motore.
fn availability(engine: Engine) -> (bool, bool) {
    match engine {
        Engine::Postgres => (postgres::native_available(), postgres::rust_available()),
        Engine::Sqlserver => (mssql::native_available(), mssql::rust_available()),
        Engine::Oracle => (oracle::native_available(), oracle::rust_available()),
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
    log.push(format!("Metodo selezionato: {}", method.label()));
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

fn dispatch_dump(conn: &Connection, out: &str, m: Method, log: &mut Vec<String>) -> Result<()> {
    match (conn.engine, m) {
        (Engine::Postgres, Method::Native) => postgres::native_dump(conn, out, log),
        (Engine::Postgres, Method::Rust) => postgres::rust_dump(conn, out, log),
        (Engine::Sqlserver, Method::Native) => mssql::native_dump(conn, out, log),
        (Engine::Sqlserver, Method::Rust) => mssql::rust_dump(conn, out, log),
        (Engine::Oracle, Method::Native) => oracle::native_dump(conn, out, log),
        (Engine::Oracle, Method::Rust) => oracle::rust_dump(conn, out, log),
    }
}

fn dispatch_import(conn: &Connection, input: &str, m: Method, log: &mut Vec<String>) -> Result<()> {
    match (conn.engine, m) {
        (Engine::Postgres, Method::Native) => postgres::native_import(conn, input, log),
        (Engine::Postgres, Method::Rust) => postgres::rust_import(conn, input, log),
        (Engine::Sqlserver, Method::Native) => mssql::native_import(conn, input, log),
        (Engine::Sqlserver, Method::Rust) => mssql::rust_import(conn, input, log),
        (Engine::Oracle, Method::Native) => oracle::native_import(conn, input, log),
        (Engine::Oracle, Method::Rust) => oracle::rust_import(conn, input, log),
    }
}

fn dispatch_clone(
    src: &Connection,
    dst: &Connection,
    m: Method,
    opts: &CloneOptions,
    log: &mut Vec<String>,
) -> Result<()> {
    match (src.engine, m) {
        (Engine::Postgres, Method::Native) => postgres::native_clone(src, dst, opts, log),
        (Engine::Postgres, Method::Rust) => postgres::rust_clone(src, dst, opts, log),
        (Engine::Sqlserver, Method::Native) => mssql::native_clone(src, dst, opts, log),
        (Engine::Sqlserver, Method::Rust) => mssql::rust_clone(src, dst, opts, log),
        (Engine::Oracle, Method::Native) => oracle::native_clone(src, dst, opts, log),
        (Engine::Oracle, Method::Rust) => oracle::rust_clone(src, dst, opts, log),
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
    }
}

/// Crea un dump del database in `out`.
pub fn dump(conn: &Connection, out: &str, prefer: Prefer) -> OpResult {
    let res = finalize(
        conn.engine,
        prefer,
        format!("Dump completato → {out}"),
        |m, log| dispatch_dump(conn, out, m, log),
    );
    if res.ok {
        res.with_artifact(out)
    } else {
        res
    }
}

/// Importa un dump `input` nel database.
pub fn import(conn: &Connection, input: &str, prefer: Prefer) -> OpResult {
    finalize(
        conn.engine,
        prefer,
        "Import completato".into(),
        |m, log| dispatch_import(conn, input, m, log),
    )
}

/// Clona il database `src` su `dst` (devono essere dello stesso motore).
pub fn clone(src: &Connection, dst: &Connection, prefer: Prefer, opts: &CloneOptions) -> OpResult {
    if src.engine != dst.engine {
        return OpResult {
            ok: false,
            method: Method::Native,
            message: "sorgente e destinazione devono usare lo stesso motore di database".into(),
            artifact: None,
            log: Vec::new(),
        };
    }
    // Il mascheramento riscrive i valori riga per riga: possibile solo col puro
    // Rust. Se richiesto, forziamo quel metodo a prescindere dalla preferenza.
    let effective = if opts.has_mask() { Prefer::Rust } else { prefer };
    finalize(
        src.engine,
        effective,
        "Clonazione completata".into(),
        |m, log| {
            if opts.has_mask() && m != Method::Rust {
                return Err(Error::Unsupported(
                    "il mascheramento richiede il metodo puro Rust".into(),
                ));
            }
            dispatch_clone(src, dst, m, opts, log)
        },
    )
}

/// Verifica la connessione al database.
pub fn test_connection(conn: &Connection, prefer: Prefer) -> OpResult {
    finalize(
        conn.engine,
        prefer,
        "Connessione riuscita".into(),
        |m, log| dispatch_test(conn, m, log),
    )
}

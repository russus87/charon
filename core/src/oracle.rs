//! Oracle: dump/import/clone.
//!
//! - **Nativo**: Oracle Data Pump (`expdp`/`impdp`) e `sqlplus` per il test.
//!   ATTENZIONE: Data Pump scrive/legge i file lato **server**, dentro la
//!   directory logica `DATA_PUMP_DIR`. Il nome file scelto in Charon viene usato
//!   come `dumpfile`; il file fisico resta sul server del database.
//! - **Puro Rust**: disponibile solo compilando con la feature `oracle-driver`,
//!   che richiede l'Oracle Instant Client in fase di link (non nelle build CI).
//!   Senza di essa, per Oracle servono i tool nativi.

use crate::model::*;
use crate::tools::{find_tool, has_tool, run};
use crate::{Error, Result};
use std::path::Path;
use std::process::Command;

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

pub fn rust_available() -> bool {
    cfg!(feature = "oracle-driver")
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
    let rust = rust_available();
    let note = if native {
        "Tool nativi trovati. Nota: Data Pump opera lato server (DATA_PUMP_DIR): \
         il dumpfile viene creato sul server del database."
            .into()
    } else if rust {
        "Userà il driver Oracle (feature oracle-driver). Richiede l'Instant Client.".into()
    } else {
        "Tool nativi non trovati. Per Oracle installa il client (expdp/impdp/sqlplus); \
         il fallback puro Rust è disponibile solo compilando con la feature 'oracle-driver'."
            .into()
    };
    EngineReport {
        engine: Engine::Oracle,
        label: Engine::Oracle.label().into(),
        tools,
        native_available: native,
        rust_available: rust,
        note,
    }
}

// ------------------------------------------------------------------ nativo ---

pub fn native_dump(conn: &Connection, out: &str, log: &mut Vec<String>) -> Result<()> {
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
    if run(log, &display, &mut cmd)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("expdp ha segnalato un errore (vedi log)".into()))
    }
}

pub fn native_import(conn: &Connection, input: &str, log: &mut Vec<String>) -> Result<()> {
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
    if run(log, &display, &mut cmd)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("impdp ha segnalato un errore (vedi log)".into()))
    }
}

pub fn native_clone(src: &Connection, dst: &Connection, log: &mut Vec<String>) -> Result<()> {
    let file = format!("charon-clone-{}.dmp", std::process::id());
    log.push(format!("Dump Data Pump della sorgente ({file})"));
    native_dump(src, &file, log)?;

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
    if run(log, &display, &mut cmd)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("impdp (clone) ha segnalato un errore (vedi log)".into()))
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
// Disponibile solo con la feature `oracle-driver` (richiede Instant Client).

pub fn rust_dump(conn: &Connection, out: &str, log: &mut Vec<String>) -> Result<()> {
    let _ = (conn, out, log);
    Err(Error::Unsupported(
        "fallback puro Rust per Oracle non disponibile: compila con la feature 'oracle-driver' \
         (richiede l'Instant Client) oppure usa i tool nativi expdp/impdp."
            .into(),
    ))
}
pub fn rust_import(conn: &Connection, input: &str, log: &mut Vec<String>) -> Result<()> {
    let _ = (conn, input, log);
    Err(Error::Unsupported(
        "fallback puro Rust per Oracle non disponibile (vedi feature 'oracle-driver').".into(),
    ))
}
pub fn rust_clone(src: &Connection, dst: &Connection, log: &mut Vec<String>) -> Result<()> {
    let _ = (src, dst, log);
    Err(Error::Unsupported(
        "fallback puro Rust per Oracle non disponibile (vedi feature 'oracle-driver').".into(),
    ))
}
pub fn rust_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    let _ = (conn, log);
    Err(Error::Unsupported(
        "fallback puro Rust per Oracle non disponibile (vedi feature 'oracle-driver').".into(),
    ))
}

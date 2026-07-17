//! SQLite: dump/import/clone.
//!
//! SQLite non è un server ma un **file**: per questo motore
//! [`Connection::database`] contiene il **percorso del file .db**, mentre
//! host/porta/utente/password non si applicano (né il tunnel SSH).
//!
//! - **Nativo**: il CLI `sqlite3` (`.dump` per esportare, stdin per importare,
//!   `.backup` per clonare — copia esatta del file).
//! - **Puro Rust** (`sqlite-driver`): `rusqlite`. A differenza degli altri
//!   motori il dump è ad **alta fedeltà** anche in puro Rust: SQLite conserva
//!   il DDL originale in `sqlite_master`, quindi lo riusiamo così com'è invece
//!   di ricostruire i tipi. I BLOB diventano letterali `X'..'`.

use crate::model::*;
use crate::tools::{find_tool, has_tool};
use crate::{Error, Result};
use std::process::Command;

/// Per SQLite il "database" è il percorso del file.
fn db_path(conn: &Connection) -> &str {
    &conn.database
}

/// Data-only e masking non sono ancora implementati per SQLite: meglio dirlo
/// che ignorare l'opzione in silenzio.
fn reject_unsupported_opts(opts: &CloneOptions) -> Result<()> {
    if opts.data_only || opts.has_mask() {
        return Err(Error::Unsupported(
            "per SQLite data-only e mascheramento non sono ancora disponibili".into(),
        ));
    }
    Ok(())
}

/// Clonare un file su se stesso lo distruggerebbe: meglio fermarsi prima.
fn reject_same_file(src: &Connection, dst: &Connection) -> Result<()> {
    let (a, b) = (db_path(src), db_path(dst));
    let same = std::fs::canonicalize(a)
        .ok()
        .zip(std::fs::canonicalize(b).ok())
        .map(|(x, y)| x == y)
        .unwrap_or(a == b);
    if same {
        return Err(Error::Unsupported(
            "sorgente e destinazione sono lo stesso file".into(),
        ));
    }
    Ok(())
}

pub fn native_available() -> bool {
    has_tool("sqlite3")
}

pub fn rust_available() -> bool {
    cfg!(feature = "sqlite-driver")
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
    let tools = vec![tool("sqlite3", "dump (.dump), import e clone (.backup)")];
    let native = native_available();
    let rust = rust_available();
    let note = if native {
        "CLI sqlite3 trovato. È comunque disponibile il fallback puro Rust (fedeltà equivalente: \
         SQLite conserva il DDL originale)."
            .into()
    } else if rust {
        "CLI sqlite3 non trovato: verrà usato il fallback puro Rust, che per SQLite è \
         pienamente equivalente (SQLite è compilato dentro Charon)."
            .into()
    } else {
        "Nessun metodo disponibile.".into()
    };
    let mut hints = Vec::new();
    if !native {
        hints.push(FixHint::new(
            "Installa il CLI sqlite3 (opzionale)",
            "Non è necessario: Charon include SQLite al proprio interno. Serve solo se preferisci \
             il metodo nativo:",
            Some(
                "Arch:           sudo pacman -S sqlite\n\
                 Debian/Ubuntu:  sudo apt install sqlite3\n\
                 macOS:          già presente\n\
                 Windows:        https://sqlite.org/download.html",
            ),
        ));
    }
    EngineReport {
        engine: Engine::Sqlite,
        label: Engine::Sqlite.label().into(),
        tools,
        native_available: native,
        rust_available: rust,
        note,
        hints,
    }
}

// ------------------------------------------------------------------ nativo ---

pub fn native_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("sqlite3").ok_or_else(|| Error::ToolMissing("sqlite3".into()))?;
    let path = db_path(conn);
    let display = format!("sqlite3 {path} .dump > {out}");
    crate::progress::note(log, format!("$ {display}"));
    if dry {
        crate::progress::note(log, "  (dry-run: comando non eseguito)");
        return Ok(());
    }
    // Non usiamo tools::run: riverserebbe l'intero dump SQL nel log.
    let o = Command::new(&exe)
        .arg(path)
        .arg(".dump")
        .output()
        .map_err(|e| Error::Cmd(format!("{display}: {e}")))?;
    if !o.status.success() {
        return Err(Error::Cmd(format!(
            "sqlite3 .dump: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )));
    }
    std::fs::write(out, &o.stdout)?;
    crate::progress::note(log, format!("Dump scritto in {out} ({} byte)", o.stdout.len()));
    Ok(())
}

pub fn native_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("sqlite3").ok_or_else(|| Error::ToolMissing("sqlite3".into()))?;
    let path = db_path(conn);
    let display = format!("sqlite3 {path} < {input}");
    crate::progress::note(log, format!("$ {display}"));
    if dry {
        crate::progress::note(log, "  (dry-run: comando non eseguito)");
        return Ok(());
    }
    let f = std::fs::File::open(input)?;
    let o = Command::new(&exe)
        .arg(path)
        .stdin(f)
        .output()
        .map_err(|e| Error::Cmd(format!("{display}: {e}")))?;
    if !o.status.success() {
        return Err(Error::Cmd(format!(
            "sqlite3 import: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )));
    }
    crate::progress::note(log, "Import completato.");
    Ok(())
}

/// Clone nativo: `.backup` produce una copia esatta del file sorgente,
/// **sostituendo** la destinazione. È il modo idiomatico per SQLite.
pub fn native_clone(
    src: &Connection,
    dst: &Connection,
    opts: &CloneOptions,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    reject_unsupported_opts(opts)?;
    reject_same_file(src, dst)?;
    let exe = find_tool("sqlite3").ok_or_else(|| Error::ToolMissing("sqlite3".into()))?;
    let (s, d) = (db_path(src), db_path(dst));
    let display = format!("sqlite3 {s} \".backup '{d}'\"");
    crate::progress::note(log, format!("$ {display}"));
    crate::progress::note(
        log,
        "  .backup sostituisce integralmente il file di destinazione",
    );
    if dry {
        crate::progress::note(log, "  (dry-run: comando non eseguito)");
        return Ok(());
    }
    let o = Command::new(&exe)
        .arg(s)
        .arg(format!(".backup '{d}'"))
        .output()
        .map_err(|e| Error::Cmd(format!("{display}: {e}")))?;
    if !o.status.success() {
        return Err(Error::Cmd(format!(
            "sqlite3 .backup: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )));
    }
    crate::progress::note(log, format!("Copia completata in {d}"));
    Ok(())
}

pub fn native_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("sqlite3").ok_or_else(|| Error::ToolMissing("sqlite3".into()))?;
    let path = db_path(conn);
    let o = Command::new(&exe)
        .arg(path)
        .arg("SELECT sqlite_version();")
        .output()
        .map_err(|e| Error::Cmd(e.to_string()))?;
    if !o.status.success() {
        return Err(Error::Conn(
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        ));
    }
    crate::progress::note(
        log,
        format!("SQLite {}", String::from_utf8_lossy(&o.stdout).trim()),
    );
    Ok(())
}

// --------------------------------------------------------------- puro Rust ---

#[cfg(not(feature = "sqlite-driver"))]
fn no_driver() -> Error {
    Error::Unsupported("fallback SQLite non disponibile in questa build".into())
}

pub fn rust_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::dump(conn, out, dry, log);
    }
    #[cfg(not(feature = "sqlite-driver"))]
    {
        let _ = (conn, out, dry, log);
        Err(no_driver())
    }
}

pub fn rust_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::import(conn, input, dry, log);
    }
    #[cfg(not(feature = "sqlite-driver"))]
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
    reject_same_file(src, dst)?;
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::clone(src, dst, dry, log);
    }
    #[cfg(not(feature = "sqlite-driver"))]
    {
        let _ = (src, dst, dry, log);
        Err(no_driver())
    }
}

pub fn rust_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::test(conn, log);
    }
    #[cfg(not(feature = "sqlite-driver"))]
    {
        let _ = (conn, log);
        Err(no_driver())
    }
}

#[cfg(feature = "sqlite-driver")]
mod rustimpl {
    use super::*;
    use rusqlite::types::ValueRef;
    use rusqlite::{Connection as Sql, OpenFlags};

    fn open_ro(path: &str) -> Result<Sql> {
        Sql::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| Error::Conn(format!("{path}: {e}")))
    }

    fn open_rw(path: &str) -> Result<Sql> {
        Sql::open(path).map_err(|e| Error::Conn(format!("{path}: {e}")))
    }

    /// Letterale SQL di un valore. I BLOB diventano `X'aabb..'`, quindi il dump
    /// regge i binari (a differenza dei motori dove passiamo per JSON).
    fn lit(v: ValueRef) -> String {
        match v {
            ValueRef::Null => "NULL".into(),
            ValueRef::Integer(i) => i.to_string(),
            ValueRef::Real(f) => f.to_string(),
            ValueRef::Text(t) => {
                format!("'{}'", String::from_utf8_lossy(t).replace('\'', "''"))
            }
            ValueRef::Blob(b) => {
                let mut s = String::with_capacity(b.len() * 2 + 3);
                s.push_str("X'");
                for byte in b {
                    s.push_str(&format!("{byte:02x}"));
                }
                s.push('\'');
                s
            }
        }
    }

    /// Oggetti dello schema (`sqlite_master`) di un certo tipo, col DDL originale.
    fn objects(c: &Sql, kind: &str) -> Result<Vec<(String, String)>> {
        let mut st = c
            .prepare(
                "SELECT name, sql FROM sqlite_master \
                 WHERE type = ?1 AND sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .map_err(|e| Error::Msg(e.to_string()))?;
        let rows = st
            .query_map([kind], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| Error::Msg(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| Error::Msg(e.to_string()))?);
        }
        Ok(out)
    }

    /// Genera il dump completo: schema (DDL originale) + dati + indici/trigger/viste.
    pub fn dump_sql(path: &str) -> Result<String> {
        let c = open_ro(path)?;
        let mut out = String::new();
        out.push_str("-- Dump generato da Charon (SQLite, puro Rust).\n");
        out.push_str("-- Il DDL è quello originale conservato da SQLite in sqlite_master.\n");
        out.push_str("PRAGMA foreign_keys=OFF;\nBEGIN TRANSACTION;\n\n");

        for (name, sql) in objects(&c, "table")? {
            out.push_str(&format!("DROP TABLE IF EXISTS \"{name}\";\n"));
            out.push_str(&sql);
            out.push_str(";\n");

            // Dati della tabella.
            let mut st = c
                .prepare(&format!("SELECT * FROM \"{name}\""))
                .map_err(|e| Error::Msg(format!("{name}: {e}")))?;
            let ncol = st.column_count();
            let cols = st
                .column_names()
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(", ");
            let mut rows = st.query([]).map_err(|e| Error::Msg(e.to_string()))?;
            while let Some(r) = rows.next().map_err(|e| Error::Msg(e.to_string()))? {
                let mut vals = Vec::with_capacity(ncol);
                for i in 0..ncol {
                    let v = r.get_ref(i).map_err(|e| Error::Msg(e.to_string()))?;
                    vals.push(lit(v));
                }
                out.push_str(&format!(
                    "INSERT INTO \"{name}\" ({cols}) VALUES ({});\n",
                    vals.join(", ")
                ));
            }
            out.push('\n');
        }

        // Indici, trigger e viste dopo i dati: più veloce e senza dipendenze.
        for kind in ["index", "trigger", "view"] {
            for (_, sql) in objects(&c, kind)? {
                out.push_str(&sql);
                out.push_str(";\n");
            }
        }

        out.push_str("\nCOMMIT;\n");
        Ok(out)
    }

    pub fn dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
        let sql = dump_sql(db_path(conn))?;
        let tables = sql.matches("CREATE TABLE").count();
        let rows = sql.matches("INSERT INTO ").count();
        if dry {
            crate::progress::note(
                log,
                format!(
                    "Dry-run: pronto un dump di {tables} tabelle / {rows} INSERT ({} byte). File {out} NON scritto.",
                    sql.len()
                ),
            );
            return Ok(());
        }
        std::fs::write(out, sql)?;
        crate::progress::note(
            log,
            format!("Dump SQL ({tables} tabelle, {rows} INSERT) scritto in {out}"),
        );
        Ok(())
    }

    pub fn import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
        let sql = std::fs::read_to_string(input)?;
        if dry {
            crate::progress::note(
                log,
                format!(
                    "Dry-run: {input} verrebbe eseguito ({} INSERT). Nessuna modifica applicata.",
                    sql.matches("INSERT INTO ").count()
                ),
            );
            return Ok(());
        }
        let c = open_rw(db_path(conn))?;
        c.execute_batch(&sql).map_err(|e| Error::Msg(e.to_string()))?;
        crate::progress::note(log, "Import completato.");
        Ok(())
    }

    pub fn clone(src: &Connection, dst: &Connection, dry: bool, log: &mut Vec<String>) -> Result<()> {
        crate::progress::note(log, "Lettura schema+dati dalla sorgente…");
        let sql = dump_sql(db_path(src))?;
        if dry {
            crate::progress::note(
                log,
                format!(
                    "Dry-run: verrebbero ricreate {} tabelle e inserite {} righe in {}. Destinazione non modificata.",
                    sql.matches("CREATE TABLE").count(),
                    sql.matches("INSERT INTO ").count(),
                    db_path(dst)
                ),
            );
            return Ok(());
        }
        crate::progress::note(log, "Scrittura sul database di destinazione…");
        let c = open_rw(db_path(dst))?;
        c.execute_batch(&sql).map_err(|e| Error::Msg(e.to_string()))?;
        crate::progress::note(log, "Clonazione completata.");
        Ok(())
    }

    pub fn test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
        let path = db_path(conn);
        let c = open_ro(path)?;
        let v: String = c
            .query_row("SELECT sqlite_version()", [], |r| r.get(0))
            .map_err(|e| Error::Conn(e.to_string()))?;
        let n: i64 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        crate::progress::note(log, format!("SQLite {v} — {n} tabelle in {path}"));
        Ok(())
    }
}

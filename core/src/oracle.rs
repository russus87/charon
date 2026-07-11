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
    // Nome della libreria per piattaforma.
    let (base, exts): (&str, &[&str]) = if cfg!(target_os = "windows") {
        ("oci", &[".dll"])
    } else if cfg!(target_os = "macos") {
        ("libclntsh", &[".dylib"])
    } else {
        ("libclntsh", &[".so"])
    };

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
            "Installa l'Oracle Instant Client",
            "Oracle non ha un driver puro-Rust (il protocollo TNS è proprietario): Charon usa \
             le librerie ufficiali Oracle, caricate a runtime. Ti basta installare l'Instant \
             Client — sono solo librerie gratuite, NON serve alcun compilatore. Poi premi \
             \"Aggiorna\".",
            Some(
                "Arch (AUR):  yay -S oracle-instantclient-basic\n\
                 macOS:       brew install instantclient-basic\n\
                 Windows/Linux generico: scarica il pacchetto \"Basic\" da\n\
                 https://www.oracle.com/database/technologies/instant-client/downloads.html\n\
                 ed estrailo in una cartella nel PATH (Windows) o in LD_LIBRARY_PATH (Linux).",
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

pub fn native_clone(
    src: &Connection,
    dst: &Connection,
    opts: &CloneOptions,
    log: &mut Vec<String>,
) -> Result<()> {
    reject_unsupported_opts(opts)?;
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

pub fn rust_dump(conn: &Connection, out: &str, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::dump(conn, out, log);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (conn, out, log);
        Err(no_driver())
    }
}
pub fn rust_import(conn: &Connection, input: &str, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::import(conn, input, log);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (conn, input, log);
        Err(no_driver())
    }
}
pub fn rust_clone(
    src: &Connection,
    dst: &Connection,
    opts: &CloneOptions,
    log: &mut Vec<String>,
) -> Result<()> {
    reject_unsupported_opts(opts)?;
    #[cfg(feature = "oracle-driver")]
    {
        return rustimpl::clone(src, dst, log);
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = (src, dst, log);
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
        log.push(format!("Trovate {} tabelle nello schema {}.", tables.len(), conn.user));

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
            log.push(format!("  {table}: {n} righe."));
        }
        Ok(out)
    }

    pub fn dump(conn: &Connection, out: &str, log: &mut Vec<String>) -> Result<()> {
        let sql = dump_sql(conn, log)?;
        std::fs::write(out, sql)?;
        log.push(format!("Dump SQL scritto in {out}"));
        Ok(())
    }

    pub fn clone(src: &Connection, dst: &Connection, log: &mut Vec<String>) -> Result<()> {
        let s = connect(src)?;
        let d = connect(dst)?;
        let tables = list_tables(&s)?;
        log.push(format!("Clonazione di {} tabelle (SELECT→INSERT, lato client).", tables.len()));

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
            log.push(format!("  {table}: {n} righe copiate."));
        }
        Ok(())
    }

    pub fn import(conn: &Connection, input: &str, log: &mut Vec<String>) -> Result<()> {
        let sql = std::fs::read_to_string(input)?;
        let stmts = split_statements(&sql);
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

//! MySQL/MariaDB: dump/import/clone.
//!
//! - **Nativo**: `mysqldump` (dump schema+dati, via `--result-file`) e `mysql`
//!   (import via stdin, test di connessione). Stessi tool per MySQL e MariaDB.
//! - **Puro Rust** (`mysql-driver`): crate `mysql` (client sincrono, come
//!   `rusqlite` per SQLite: niente runtime async). Il dump ricostruisce lo
//!   schema con `SHOW CREATE TABLE` (DDL originale del server) e i dati come
//!   `INSERT`.

use crate::compare::{ColumnDiff, DbDiff, RowDelta, Status, TableDataDiff, TableDiff};
use crate::model::*;
use crate::schema::{AbstractType, Column, ForeignKey, Index, SchemaModel, Table};
use crate::tools::{find_tool, has_tool, plan_or_run, run};
use crate::{Error, Result};
use std::process::Command;

/// Data-only e mascheramento non sono ancora implementati per MySQL/MariaDB:
/// meglio dirlo che ignorare l'opzione in silenzio (come per SQLite).
fn reject_unsupported_opts(opts: &CloneOptions) -> Result<()> {
    if opts.data_only || opts.has_mask() {
        return Err(Error::Unsupported(
            "per MySQL/MariaDB data-only e mascheramento non sono ancora disponibili".into(),
        ));
    }
    Ok(())
}

pub fn native_available() -> bool {
    has_tool("mysqldump") && has_tool("mysql")
}

pub fn rust_available() -> bool {
    cfg!(feature = "mysql-driver")
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
        tool("mysqldump", "genera il dump (schema + dati)"),
        tool("mysql", "esegue/importa script SQL e test di connessione"),
    ];
    let native = native_available();
    let rust = rust_available();
    let note = if native {
        "Tool nativi trovati: dump con mysqldump, import con mysql.".into()
    } else if has_tool("mysql") {
        "Trovato mysql ma manca mysqldump per il dump: verrà usato il fallback puro Rust \
         se necessario."
            .into()
    } else if rust {
        "Tool nativi non trovati: verrà usato il fallback puro Rust (best-effort).".into()
    } else {
        "Nessun metodo disponibile.".into()
    };
    let mut hints = Vec::new();
    if !native {
        hints.push(FixHint::new(
            "Installa i client MySQL/MariaDB (mysqldump, mysql)",
            "Servono per dump e import ad alta fedeltà. Fanno parte dei pacchetti client ufficiali:",
            Some(
                "Arch:           sudo pacman -S mariadb-clients\n\
                 Debian/Ubuntu:  sudo apt install mysql-client\n\
                 macOS:          brew install mysql-client\n\
                 Windows:        https://dev.mysql.com/downloads/mysql/",
            ),
        ));
    }
    if !native && !rust {
        hints.push(FixHint::new(
            "Nessun metodo disponibile",
            "Mancano i tool nativi e il fallback puro Rust. Ricompila con la feature \
             'mysql-driver' (attiva di default) oppure installa i tool sopra.",
            None,
        ));
    }
    EngineReport {
        engine: Engine::Mysql,
        label: Engine::Mysql.label().into(),
        tools,
        native_available: native,
        rust_available: rust,
        note,
        hints,
    }
}

// ------------------------------------------------------------------ nativo ---

/// `mysqldump` scrive direttamente su file con `--result-file`: evita di far
/// transitare l'intero dump per `tools::run` (che lo riverserebbe nel log).
pub fn native_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("mysqldump").ok_or_else(|| Error::ToolMissing("mysqldump".into()))?;
    let mut cmd = Command::new(&exe);
    cmd.arg(format!("--host={}", conn.host))
        .arg(format!("--port={}", conn.port))
        .arg(format!("--user={}", conn.user))
        .arg(format!("--password={}", conn.password))
        .arg("--routines")
        .arg("--triggers")
        .arg("--single-transaction")
        .arg(format!("--result-file={out}"))
        .arg(&conn.database);
    let display = format!(
        "mysqldump --host={} --port={} --user={} --routines --triggers --single-transaction \
         --result-file={out} {}",
        conn.host, conn.port, conn.user, conn.database
    );
    if plan_or_run(log, &display, &mut cmd, dry)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("mysqldump ha segnalato un errore (vedi log)".into()))
    }
}

/// Import via stdin (`mysql database < file`): niente flag nativo per un file
/// di input, quindi costruiamo lo stdin a mano come fa SQLite.
pub fn native_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("mysql").ok_or_else(|| Error::ToolMissing("mysql".into()))?;
    let display = format!(
        "mysql --host={} --port={} --user={} {} < {input}",
        conn.host, conn.port, conn.user, conn.database
    );
    crate::progress::note(log, format!("$ {display}"));
    if dry {
        crate::progress::note(log, "  (dry-run: comando non eseguito)");
        return Ok(());
    }
    let f = std::fs::File::open(input)?;
    let o = Command::new(&exe)
        .arg(format!("--host={}", conn.host))
        .arg(format!("--port={}", conn.port))
        .arg(format!("--user={}", conn.user))
        .arg(format!("--password={}", conn.password))
        .arg(&conn.database)
        .stdin(f)
        .output()
        .map_err(|e| Error::Cmd(format!("{display}: {e}")))?;
    if !o.status.success() {
        return Err(Error::Cmd(format!(
            "mysql import: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )));
    }
    crate::progress::note(log, "Import completato.");
    Ok(())
}

/// Clone nativo: dump della sorgente su file temporaneo, poi import sulla
/// destinazione (stesso schema usato da SQL Server).
pub fn native_clone(
    src: &Connection,
    dst: &Connection,
    opts: &CloneOptions,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    reject_unsupported_opts(opts)?;
    let tmp = std::env::temp_dir().join(format!("charon-mysql-{}.sql", std::process::id()));
    let tmp_s = tmp.display().to_string();
    crate::progress::note(log, format!("Dump temporaneo della sorgente in {tmp_s}"));
    native_dump(src, &tmp_s, dry, log)?;
    crate::progress::note(log, "Import sul database di destinazione".to_string());
    let res = native_import(dst, &tmp_s, dry, log);
    let _ = std::fs::remove_file(&tmp);
    res
}

pub fn native_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("mysql").ok_or_else(|| Error::ToolMissing("mysql".into()))?;
    let mut cmd = Command::new(&exe);
    cmd.arg(format!("--host={}", conn.host))
        .arg(format!("--port={}", conn.port))
        .arg(format!("--user={}", conn.user))
        .arg(format!("--password={}", conn.password))
        .arg(&conn.database)
        .arg("-e")
        .arg("SELECT VERSION()");
    let display = format!(
        "mysql --host={} --port={} --user={} {} -e 'SELECT VERSION()'",
        conn.host, conn.port, conn.user, conn.database
    );
    if run(log, &display, &mut cmd)?.success {
        Ok(())
    } else {
        Err(Error::Conn("connessione/mysql falliti (vedi log)".into()))
    }
}

// --------------------------------------------------------------- puro Rust ---

#[cfg(not(feature = "mysql-driver"))]
fn no_driver() -> Error {
    Error::Unsupported("fallback MySQL non disponibile in questa build".into())
}

pub fn rust_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "mysql-driver")]
    {
        return rustimpl::dump(conn, out, dry, log);
    }
    #[cfg(not(feature = "mysql-driver"))]
    {
        let _ = (conn, out, dry, log);
        Err(no_driver())
    }
}

pub fn rust_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "mysql-driver")]
    {
        return rustimpl::import(conn, input, dry, log);
    }
    #[cfg(not(feature = "mysql-driver"))]
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
    #[cfg(feature = "mysql-driver")]
    {
        return rustimpl::clone(src, dst, dry, log);
    }
    #[cfg(not(feature = "mysql-driver"))]
    {
        let _ = (src, dst, dry, log);
        Err(no_driver())
    }
}

pub fn rust_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "mysql-driver")]
    {
        return rustimpl::test(conn, log);
    }
    #[cfg(not(feature = "mysql-driver"))]
    {
        let _ = (conn, log);
        Err(no_driver())
    }
}

/// Confronta due database MySQL/MariaDB (schema + conteggio righe). Sincrona:
/// il crate `mysql` non richiede un runtime async.
pub fn rust_compare(src: &Connection, dst: &Connection) -> Result<DbDiff> {
    #[cfg(feature = "mysql-driver")]
    {
        return rustimpl::compare(src, dst);
    }
    #[cfg(not(feature = "mysql-driver"))]
    {
        let _ = (src, dst);
        Err(no_driver())
    }
}

/// Legge lo schema neutro (indipendente dal motore) dell'intero database:
/// tabelle, colonne con tipo astratto/nullabilità e chiave primaria. Sincrona
/// come `rust_compare`: nessun runtime async necessario.
pub fn rust_read_schema(conn: &Connection) -> Result<SchemaModel> {
    #[cfg(feature = "mysql-driver")]
    {
        return rustimpl::read_schema(conn);
    }
    #[cfg(not(feature = "mysql-driver"))]
    {
        let _ = conn;
        Err(no_driver())
    }
}

/// Confronta i dati di una singola tabella riga per riga (per chiave primaria,
/// o per riga intera se la tabella non ne ha una). Sincrona come `rust_compare`.
pub fn rust_data_diff(src: &Connection, dst: &Connection, table: &str) -> Result<TableDataDiff> {
    #[cfg(feature = "mysql-driver")]
    {
        return rustimpl::data_diff(src, dst, table);
    }
    #[cfg(not(feature = "mysql-driver"))]
    {
        let _ = (src, dst, table);
        Err(no_driver())
    }
}

/// Esporta i dati di tutte le tabelle in CSV/JSON, un file per tabella dentro
/// `out_dir`. Sincrona come `rust_compare`: nessun runtime async necessario.
pub fn rust_export(conn: &Connection, out_dir: &str, format: &str) -> Result<Vec<String>> {
    #[cfg(feature = "mysql-driver")]
    {
        return rustimpl::export(conn, out_dir, format);
    }
    #[cfg(not(feature = "mysql-driver"))]
    {
        let _ = (conn, out_dir, format);
        Err(no_driver())
    }
}

/// Anteprima (read-only) delle prime `limit` righe di una tabella: stessa
/// lettura di `rust_export`, ma limitata e senza scrivere file. Sincrona come
/// `rust_export`: nessun runtime async necessario.
pub fn rust_peek(conn: &Connection, table: &str, limit: u32) -> Result<(Vec<String>, Vec<Vec<Option<String>>>)> {
    #[cfg(feature = "mysql-driver")]
    {
        return rustimpl::peek(conn, table, limit);
    }
    #[cfg(not(feature = "mysql-driver"))]
    {
        let _ = (conn, table, limit);
        Err(no_driver())
    }
}

/// Esegue una query SQL libera (SELECT o comando DML/DDL) e ne restituisce
/// l'esito in forma neutra: result set con colonne/righe per le query di
/// lettura, righe modificate per le altre. Sincrona come le altre funzioni
/// `rust_*`: nessun runtime async necessario.
pub fn rust_query(conn: &Connection, sql: &str) -> Result<QueryResult> {
    #[cfg(feature = "mysql-driver")]
    {
        return rustimpl::run_query(conn, sql);
    }
    #[cfg(not(feature = "mysql-driver"))]
    {
        let _ = (conn, sql);
        Err(no_driver())
    }
}

#[cfg(feature = "mysql-driver")]
mod rustimpl {
    use super::*;
    use crate::model::QueryResult;
    use mysql::prelude::Queryable;
    use mysql::{Conn, OptsBuilder, Value};
    use std::collections::HashMap;

    fn connect(conn: &Connection) -> Result<Conn> {
        let opts = OptsBuilder::default()
            .ip_or_hostname(Some(conn.host.clone()))
            .tcp_port(conn.port)
            .user(Some(conn.user.clone()))
            .pass(Some(conn.password.clone()))
            .db_name(Some(conn.database.clone()));
        Conn::new(opts).map_err(|e| Error::Conn(e.to_string()))
    }

    /// Semplice escaping per i letterali inseriti nelle query di catalogo
    /// (es. il nome del database in `WHERE table_schema = '...'`).
    fn esc(s: &str) -> String {
        s.replace('\'', "''")
    }

    /// Letterale SQL di un valore. Con il protocollo testuale usato da
    /// `query_iter` (query semplici, non preparate) tutto ciò che non è NULL
    /// arriva come [`Value::Bytes`]: se i byte sono UTF-8 valido li trattiamo
    /// da stringa (escapata), altrimenti da BLOB binario (`X'..'`).
    fn lit(v: &Value) -> String {
        match v {
            Value::NULL => "NULL".into(),
            Value::Int(i) => i.to_string(),
            Value::UInt(u) => u.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Double(f) => f.to_string(),
            Value::Bytes(b) => match std::str::from_utf8(b) {
                Ok(s) => format!(
                    "'{}'",
                    s.replace('\\', "\\\\").replace('\'', "''")
                ),
                Err(_) => {
                    let mut out = String::with_capacity(b.len() * 2 + 3);
                    out.push_str("X'");
                    for byte in b {
                        out.push_str(&format!("{byte:02x}"));
                    }
                    out.push('\'');
                    out
                }
            },
            // Non compaiono con il protocollo testuale, ma li gestiamo per
            // completezza (es. se in futuro si passasse a query preparate).
            Value::Date(y, mo, d, h, mi, s, us) => {
                if *us > 0 {
                    format!("'{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}.{us:06}'")
                } else {
                    format!("'{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}'")
                }
            }
            Value::Time(neg, days, h, mi, s, us) => {
                let sign = if *neg { "-" } else { "" };
                let hh = *days as u64 * 24 + *h as u64;
                if *us > 0 {
                    format!("'{sign}{hh:02}:{mi:02}:{s:02}.{us:06}'")
                } else {
                    format!("'{sign}{hh:02}:{mi:02}:{s:02}'")
                }
            }
        }
    }

    // ------------------------------------------------------------- compare ---

    /// Nomi delle tabelle base (esclude viste) del database.
    fn list_tables(client: &mut Conn, db: &str) -> Result<Vec<String>> {
        client
            .query(format!(
                "SELECT TABLE_NAME FROM information_schema.tables \
                 WHERE table_schema = '{}' AND table_type = 'BASE TABLE' ORDER BY TABLE_NAME",
                esc(db)
            ))
            .map_err(|e| Error::Msg(e.to_string()))
    }

    /// Colonne di una tabella (nome, definizione leggibile): `COLUMN_TYPE` più
    /// `NOT NULL` quando `IS_NULLABLE='NO'`.
    fn columns_def(client: &mut Conn, db: &str, table: &str) -> Result<Vec<(String, String)>> {
        let rows: Vec<(String, String, String)> = client
            .query(format!(
                "SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE FROM information_schema.columns \
                 WHERE table_schema = '{}' AND table_name = '{}' ORDER BY ORDINAL_POSITION",
                esc(db),
                esc(table)
            ))
            .map_err(|e| Error::Msg(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|(name, ctype, nullable)| {
                let def = if nullable == "NO" {
                    format!("{ctype} NOT NULL")
                } else {
                    ctype
                };
                (name, def)
            })
            .collect())
    }

    /// Conta le righe di una tabella. Best-effort: `None` se non contabile.
    fn count_rows(client: &mut Conn, table: &str) -> Option<i64> {
        client
            .query_first(format!("SELECT COUNT(*) FROM `{table}`"))
            .ok()
            .flatten()
    }

    /// Confronta schema e volume dati di due database MySQL/MariaDB.
    pub fn compare(src: &Connection, dst: &Connection) -> Result<DbDiff> {
        let mut sc = connect(src)?;
        let mut dc = connect(dst)?;
        let mut log = Vec::new();

        let stables = list_tables(&mut sc, &src.database)?;
        let dtables = list_tables(&mut dc, &dst.database)?;
        crate::progress::note(
            &mut log,
            format!(
                "Tabelle: {} nella sorgente, {} nella destinazione",
                stables.len(),
                dtables.len()
            ),
        );

        // Unione ordinata+dedup dei nomi visti da almeno una parte.
        let mut names: Vec<String> = stables.iter().chain(dtables.iter()).cloned().collect();
        names.sort();
        names.dedup();

        let mut tables = Vec::new();
        for name in names {
            let in_s = stables.contains(&name);
            let in_d = dtables.contains(&name);

            // Tabella presente da un solo lato: tutte le colonne sono "nuove".
            if in_s != in_d {
                let status = if in_s { Status::OnlySource } else { Status::OnlyTarget };
                let cols = if in_s {
                    columns_def(&mut sc, &src.database, &name)?
                } else {
                    columns_def(&mut dc, &dst.database, &name)?
                };
                let rows = if in_s {
                    count_rows(&mut sc, &name)
                } else {
                    count_rows(&mut dc, &name)
                };
                let columns_diff = cols
                    .into_iter()
                    .map(|(cname, def)| ColumnDiff {
                        name: cname,
                        status,
                        source: if in_s { Some(def.clone()) } else { None },
                        target: if in_s { None } else { Some(def) },
                    })
                    .collect();
                tables.push(TableDiff {
                    name: name.clone(),
                    status,
                    columns: columns_diff,
                    source_rows: if in_s { rows } else { None },
                    target_rows: if in_s { None } else { rows },
                });
                continue;
            }

            // Presente da entrambe le parti: confronto colonna per colonna.
            let scols = columns_def(&mut sc, &src.database, &name)?;
            let dcols = columns_def(&mut dc, &dst.database, &name)?;
            let mut cnames: Vec<String> = scols
                .iter()
                .chain(dcols.iter())
                .map(|(n, _)| n.clone())
                .collect();
            cnames.sort();
            cnames.dedup();

            let mut columns_diff = Vec::new();
            for cn in cnames {
                let sdef = scols.iter().find(|(n, _)| *n == cn).map(|(_, d)| d.clone());
                let ddef = dcols.iter().find(|(n, _)| *n == cn).map(|(_, d)| d.clone());
                let status = match (&sdef, &ddef) {
                    (Some(a), Some(b)) if a == b => Status::Same,
                    (Some(_), Some(_)) => Status::Changed,
                    (Some(_), None) => Status::OnlySource,
                    (None, Some(_)) => Status::OnlyTarget,
                    (None, None) => continue, // impossibile: il nome viene da un lato
                };
                // Le colonne identiche non entrano nel diff: su tabelle larghe
                // renderebbero illeggibile ciò che conta davvero.
                if status != Status::Same {
                    columns_diff.push(ColumnDiff { name: cn, status, source: sdef, target: ddef });
                }
            }

            let status = if columns_diff.is_empty() { Status::Same } else { Status::Changed };
            let source_rows = count_rows(&mut sc, &name);
            let target_rows = count_rows(&mut dc, &name);
            tables.push(TableDiff {
                name,
                status,
                columns: columns_diff,
                source_rows,
                target_rows,
            });
        }

        Ok(DbDiff {
            tables,
            source_label: format!("{}:{}/{}", src.host, src.port, src.database),
            target_label: format!("{}:{}/{}", dst.host, dst.port, dst.database),
            log,
        })
    }

    // --------------------------------------------------------------- schema ---

    /// Colonne della chiave primaria di una tabella, nell'ordine dichiarato
    /// (stessa query usata da `data_diff`).
    fn pk_columns(client: &mut Conn, db: &str, table: &str) -> Result<Vec<String>> {
        client
            .query(format!(
                "SELECT COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE \
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}' AND CONSTRAINT_NAME = 'PRIMARY' \
                 ORDER BY ORDINAL_POSITION",
                esc(db),
                esc(table)
            ))
            .map_err(|e| Error::Msg(e.to_string()))
    }

    /// Mappa `DATA_TYPE`/`COLUMN_TYPE` (in minuscolo) di `information_schema`
    /// sul tipo astratto neutro. `Unknown` conserva il `DATA_TYPE` originale
    /// per i tipi non riconosciuti.
    fn map_type(
        data_type: &str,
        column_type: &str,
        char_max_len: Option<i64>,
        num_precision: Option<i64>,
        num_scale: Option<i64>,
    ) -> AbstractType {
        let dt = data_type.to_lowercase();
        let ct = column_type.to_lowercase();
        match dt.as_str() {
            "varchar" | "char" => AbstractType::Text { max: char_max_len.map(|n| n as u32) },
            "text" | "tinytext" | "mediumtext" | "longtext" => AbstractType::Text { max: None },
            "tinyint" if ct == "tinyint(1)" => AbstractType::Boolean,
            "tinyint" | "smallint" => AbstractType::Integer { bits: 16 },
            "mediumint" | "int" => AbstractType::Integer { bits: 32 },
            "bigint" => AbstractType::Integer { bits: 64 },
            "decimal" | "numeric" => AbstractType::Decimal {
                precision: num_precision.map(|n| n as u32),
                scale: num_scale.map(|n| n as u32),
            },
            "float" => AbstractType::Float { double: false },
            "double" => AbstractType::Float { double: true },
            "date" => AbstractType::Date,
            "time" => AbstractType::Time,
            "datetime" | "timestamp" => AbstractType::Timestamp { tz: false },
            "binary" | "varbinary" => AbstractType::Binary { max: char_max_len.map(|n| n as u32) },
            "blob" | "tinyblob" | "mediumblob" | "longblob" => AbstractType::Binary { max: None },
            "json" => AbstractType::Json,
            _ => AbstractType::Unknown { raw: data_type.to_string() },
        }
    }

    /// Indici non-PK di una tabella (`information_schema.statistics`), raggruppati
    /// per nome indice, colonne nell'ordine `SEQ_IN_INDEX`.
    fn table_indexes(client: &mut Conn, db: &str, table: &str) -> Result<Vec<Index>> {
        let rows: Vec<(String, i64, String)> = client
            .query(format!(
                "SELECT INDEX_NAME, NON_UNIQUE, COLUMN_NAME FROM information_schema.statistics \
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}' AND INDEX_NAME <> 'PRIMARY' \
                 ORDER BY INDEX_NAME, SEQ_IN_INDEX",
                esc(db),
                esc(table)
            ))
            .map_err(|e| Error::Msg(e.to_string()))?;

        // Ordine di prima apparizione + raggruppamento per nome indice.
        let mut order: Vec<String> = Vec::new();
        let mut grouped: HashMap<String, (bool, Vec<String>)> = HashMap::new();
        for (idx_name, non_unique, col_name) in rows {
            let entry = grouped.entry(idx_name.clone()).or_insert_with(|| {
                order.push(idx_name.clone());
                (non_unique == 0, Vec::new())
            });
            entry.1.push(col_name);
        }
        Ok(order
            .into_iter()
            .map(|name| {
                let (unique, columns) = grouped.remove(&name).unwrap();
                Index { name, columns, unique }
            })
            .collect())
    }

    /// Foreign key di una tabella (`information_schema.KEY_COLUMN_USAGE`),
    /// raggruppate per nome vincolo, colonne nell'ordine `ORDINAL_POSITION`.
    fn table_foreign_keys(client: &mut Conn, db: &str, table: &str) -> Result<Vec<ForeignKey>> {
        let rows: Vec<(String, String, String, String)> = client
            .query(format!(
                "SELECT CONSTRAINT_NAME, COLUMN_NAME, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME \
                 FROM information_schema.KEY_COLUMN_USAGE \
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}' AND REFERENCED_TABLE_NAME IS NOT NULL \
                 ORDER BY CONSTRAINT_NAME, ORDINAL_POSITION",
                esc(db),
                esc(table)
            ))
            .map_err(|e| Error::Msg(e.to_string()))?;

        let mut order: Vec<String> = Vec::new();
        let mut grouped: HashMap<String, (Vec<String>, String, Vec<String>)> = HashMap::new();
        for (cname, col, ref_table, ref_col) in rows {
            let entry = grouped.entry(cname.clone()).or_insert_with(|| {
                order.push(cname.clone());
                (Vec::new(), ref_table.clone(), Vec::new())
            });
            entry.0.push(col);
            entry.2.push(ref_col);
        }
        Ok(order
            .into_iter()
            .map(|name| {
                let (columns, ref_table, ref_columns) = grouped.remove(&name).unwrap();
                ForeignKey { name, columns, ref_table, ref_columns }
            })
            .collect())
    }

    /// Legge lo schema neutro dell'intero database: per ogni tabella, le
    /// colonne (tipo astratto, nullabilità, auto-increment, default), la
    /// chiave primaria, gli indici non-PK e le foreign key, ricavate da
    /// `information_schema`.
    pub fn read_schema(conn: &Connection) -> Result<SchemaModel> {
        let mut client = connect(conn)?;
        let table_names = list_tables(&mut client, &conn.database)?;

        let mut tables = Vec::with_capacity(table_names.len());
        for tname in &table_names {
            let pk = pk_columns(&mut client, &conn.database, tname)?;

            let rows: Vec<(
                String,
                String,
                String,
                Option<i64>,
                Option<i64>,
                Option<i64>,
                String,
                String,
                Option<String>,
            )> = client
                .query(format!(
                    "SELECT COLUMN_NAME, DATA_TYPE, COLUMN_TYPE, CHARACTER_MAXIMUM_LENGTH, \
                     NUMERIC_PRECISION, NUMERIC_SCALE, IS_NULLABLE, EXTRA, COLUMN_DEFAULT \
                     FROM information_schema.columns \
                     WHERE table_schema = '{}' AND table_name = '{}' ORDER BY ORDINAL_POSITION",
                    esc(&conn.database),
                    esc(tname)
                ))
                .map_err(|e| Error::Msg(e.to_string()))?;

            let mut columns = Vec::with_capacity(rows.len());
            for (
                name,
                data_type,
                column_type,
                char_max_len,
                num_precision,
                num_scale,
                is_nullable,
                extra,
                column_default,
            ) in rows
            {
                let ty = map_type(&data_type, &column_type, char_max_len, num_precision, num_scale);
                let primary_key = pk.iter().any(|k| k == &name);
                let auto_increment = extra.to_lowercase().contains("auto_increment");
                let default = if auto_increment { None } else { column_default };
                columns.push(Column {
                    name,
                    ty,
                    nullable: is_nullable == "YES",
                    primary_key,
                    auto_increment,
                    default,
                });
            }

            let indexes = table_indexes(&mut client, &conn.database, tname)?;
            let foreign_keys = table_foreign_keys(&mut client, &conn.database, tname)?;

            tables.push(Table { name: tname.clone(), columns, indexes, foreign_keys });
        }

        Ok(SchemaModel { tables })
    }

    // -------------------------------------------------------------- dati ---

    /// Legge tutte le righe di una tabella e le indicizza per chiave: la mappa
    /// associa la chiave "cruda" (valori delle colonne-chiave uniti da un
    /// separatore di controllo, per evitare ambiguità con valori che
    /// contengono ", ") a una coppia (rappresentazione delle colonne non-chiave,
    /// chiave leggibile "col=val, col=val" per il campione mostrato in UI).
    fn read_keyed_rows(
        client: &mut Conn,
        table: &str,
        key_cols: &[String],
    ) -> Result<HashMap<String, (String, String)>> {
        let qr = client
            .query_iter(format!("SELECT * FROM `{table}`"))
            .map_err(|e| Error::Msg(format!("{table}: {e}")))?;
        let col_names: Vec<String> = qr
            .columns()
            .as_ref()
            .iter()
            .map(|c| c.name_str().into_owned())
            .collect();
        // Indici delle colonne-chiave nell'ordine con cui compaiono nel resultset.
        let key_idx: Vec<usize> = key_cols
            .iter()
            .filter_map(|k| col_names.iter().position(|c| c == k))
            .collect();

        let mut map = HashMap::new();
        for row in qr {
            let row = row.map_err(|e| Error::Msg(format!("{table}: {e}")))?;
            let mut vals = Vec::with_capacity(row.len());
            for i in 0..row.len() {
                let v = row.as_ref(i).cloned().unwrap_or(Value::NULL);
                vals.push(lit(&v));
            }
            let key_str = key_idx
                .iter()
                .map(|&i| vals[i].as_str())
                .collect::<Vec<_>>()
                .join("\u{1}");
            let key_display = key_idx
                .iter()
                .map(|&i| format!("{}={}", col_names[i], vals[i]))
                .collect::<Vec<_>>()
                .join(", ");
            let val_str = col_names
                .iter()
                .enumerate()
                .filter(|(i, _)| !key_idx.contains(i))
                .map(|(i, _)| vals[i].as_str())
                .collect::<Vec<_>>()
                .join("\u{1}");
            map.insert(key_str, (val_str, key_display));
        }
        Ok(map)
    }

    /// Confronto dati riga-per-riga di una tabella: righe accoppiate per
    /// chiave primaria (o per riga intera in assenza di PK), classificate
    /// come solo-sorgente / solo-destinazione / cambiate / uguali.
    pub fn data_diff(src: &Connection, dst: &Connection, table: &str) -> Result<TableDataDiff> {
        let mut sc = connect(src)?;
        let mut dc = connect(dst)?;

        // Colonne della chiave primaria, nell'ordine dichiarato.
        let mut key_cols: Vec<String> = sc
            .query(format!(
                "SELECT COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE \
                 WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}' AND CONSTRAINT_NAME = 'PRIMARY' \
                 ORDER BY ORDINAL_POSITION",
                esc(&src.database),
                esc(table)
            ))
            .map_err(|e| Error::Msg(e.to_string()))?;

        let no_pk = key_cols.is_empty();
        let note = if no_pk {
            // Nessuna chiave primaria: si confronta la riga intera (tutte le
            // colonne diventano "chiave" e non resta nulla da confrontare come
            // "valore").
            key_cols = columns_def(&mut sc, &src.database, table)?
                .into_iter()
                .map(|(n, _)| n)
                .collect();
            Some("nessuna chiave primaria: confronto per riga intera".to_string())
        } else {
            None
        };

        let smap = read_keyed_rows(&mut sc, table, &key_cols)?;
        let dmap = read_keyed_rows(&mut dc, table, &key_cols)?;

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
            // Coerente col contratto in compare.rs: vuoto se si è confrontata
            // la riga intera (key_cols qui contiene tutte le colonne, usate
            // solo internamente per l'hashing).
            key: if no_pk { Vec::new() } else { key_cols },
            only_source,
            only_target,
            changed,
            same,
            sample,
            note,
        })
    }

    // --------------------------------------------------------------- dump ---

    /// DDL originale della tabella (`SHOW CREATE TABLE`).
    fn create_table_sql(client: &mut Conn, table: &str) -> Result<String> {
        let row: Option<(String, String)> = client
            .query_first(format!("SHOW CREATE TABLE `{table}`"))
            .map_err(|e| Error::Msg(format!("{table}: {e}")))?;
        row.map(|(_, ddl)| ddl)
            .ok_or_else(|| Error::Msg(format!("{table}: SHOW CREATE TABLE senza risultato")))
    }

    fn dump_sql(conn: &Connection) -> Result<String> {
        let mut client = connect(conn)?;
        let tables = list_tables(&mut client, &conn.database)?;

        let mut out = String::new();
        out.push_str("-- Dump generato da Charon (MySQL/MariaDB, puro Rust).\n");
        out.push_str("-- Lo schema è il DDL originale restituito da SHOW CREATE TABLE.\n");
        out.push_str("SET FOREIGN_KEY_CHECKS=0;\nSET NAMES utf8mb4;\n\n");

        for table in &tables {
            let ddl = create_table_sql(&mut client, table)?;
            out.push_str(&format!("DROP TABLE IF EXISTS `{table}`;\n"));
            out.push_str(&ddl);
            out.push_str(";\n");

            // Dati: colonne dalla risposta della query, valori riga per riga.
            let qr = client
                .query_iter(format!("SELECT * FROM `{table}`"))
                .map_err(|e| Error::Msg(format!("{table}: {e}")))?;
            let col_names: Vec<String> = qr
                .columns()
                .as_ref()
                .iter()
                .map(|c| c.name_str().into_owned())
                .collect();
            let collist = col_names
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ");
            for row in qr {
                let row = row.map_err(|e| Error::Msg(format!("{table}: {e}")))?;
                let mut vals = Vec::with_capacity(row.len());
                for i in 0..row.len() {
                    let v = row.as_ref(i).cloned().unwrap_or(Value::NULL);
                    vals.push(lit(&v));
                }
                out.push_str(&format!(
                    "INSERT INTO `{table}` ({collist}) VALUES ({});\n",
                    vals.join(", ")
                ));
            }
            out.push('\n');
        }

        out.push_str("SET FOREIGN_KEY_CHECKS=1;\n");
        Ok(out)
    }

    pub fn dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
        let sql = dump_sql(conn)?;
        let tables = sql.matches("DROP TABLE IF EXISTS").count();
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

    /// Esegue lo script separando gli statement sui `;` di fine riga: euristica
    /// ragionevole per un dump generato da noi stessi (nessuna stringa contiene
    /// `;\n` non terminale in pratica, e le DDL non hanno mai corpo multi-statement).
    fn split_statements(sql: &str) -> Vec<String> {
        sql.split(";\n")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn run_script(conn: &Connection, sql: &str, log: &mut Vec<String>) -> Result<()> {
        let mut client = connect(conn)?;
        let statements = split_statements(sql);
        let mut failures = 0usize;
        for stmt in &statements {
            if let Err(e) = client.query_drop(stmt) {
                failures += 1;
                let head: String = stmt.trim().chars().take(140).collect();
                crate::progress::note(
                    log,
                    format!("⚠ statement non applicato: {} | causa: {e}", head.replace('\n', " ")),
                );
            }
        }
        if failures > 0 {
            return Err(Error::Msg(format!(
                "{failures} statement su {} non applicati (gli altri sì): vedi il log per le cause.",
                statements.len()
            )));
        }
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
        crate::progress::note(log, format!("Esecuzione di {input}…"));
        run_script(conn, &sql, log)?;
        crate::progress::note(log, "Import completato.");
        Ok(())
    }

    pub fn clone(src: &Connection, dst: &Connection, dry: bool, log: &mut Vec<String>) -> Result<()> {
        crate::progress::note(log, "Lettura schema+dati dalla sorgente…");
        let sql = dump_sql(src)?;
        if dry {
            let tables = sql.matches("DROP TABLE IF EXISTS").count();
            let rows = sql.matches("INSERT INTO ").count();
            crate::progress::note(
                log,
                format!(
                    "Dry-run: verrebbero ricreate {tables} tabelle e inserite {rows} righe su {}:{}/{}. Destinazione non modificata.",
                    dst.host, dst.port, dst.database
                ),
            );
            return Ok(());
        }
        crate::progress::note(log, "Scrittura sul database di destinazione…");
        run_script(dst, &sql, log)?;
        crate::progress::note(log, "Clonazione completata.");
        Ok(())
    }

    // ------------------------------------------------------------- export ---

    /// Valore grezzo (non un letterale SQL) di una cella: `NULL` → `None`,
    /// altrimenti la stringa "naturale" del valore. Col protocollo testuale di
    /// `query_iter` tutto ciò che non è NULL arriva quasi sempre come
    /// [`Value::Bytes`], che qui rendiamo come UTF-8 (con sostituzione dei
    /// byte non validi, non dovrebbe capitare su colonne testuali/numeriche).
    fn raw_value(v: &Value) -> Option<String> {
        match v {
            Value::NULL => None,
            Value::Int(i) => Some(i.to_string()),
            Value::UInt(u) => Some(u.to_string()),
            Value::Float(f) => Some(f.to_string()),
            Value::Double(f) => Some(f.to_string()),
            Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
            Value::Date(y, mo, d, h, mi, s, us) => {
                if *us > 0 {
                    Some(format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}.{us:06}"))
                } else {
                    Some(format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}"))
                }
            }
            Value::Time(neg, days, h, mi, s, us) => {
                let sign = if *neg { "-" } else { "" };
                let hh = *days as u64 * 24 + *h as u64;
                if *us > 0 {
                    Some(format!("{sign}{hh:02}:{mi:02}:{s:02}.{us:06}"))
                } else {
                    Some(format!("{sign}{hh:02}:{mi:02}:{s:02}"))
                }
            }
        }
    }

    /// Esporta i dati di ogni tabella del database in un file CSV/JSON dentro
    /// `out_dir` (un file per tabella, nome `<tabella>.<estensione>`). Ritorna
    /// i percorsi scritti.
    pub fn export(conn: &Connection, out_dir: &str, format: &str) -> Result<Vec<String>> {
        let fmt = crate::export::DataFormat::from_str(format)
            .ok_or_else(|| Error::Unsupported("formato non supportato".into()))?;
        std::fs::create_dir_all(out_dir)?;

        let mut client = connect(conn)?;
        let tables = list_tables(&mut client, &conn.database)?;

        let mut files = Vec::new();
        for table in &tables {
            let qr = client
                .query_iter(format!("SELECT * FROM `{table}`"))
                .map_err(|e| Error::Msg(format!("{table}: {e}")))?;
            let columns: Vec<String> = qr
                .columns()
                .as_ref()
                .iter()
                .map(|c| c.name_str().into_owned())
                .collect();

            let mut rows: Vec<Vec<Option<String>>> = Vec::new();
            for row in qr {
                let row = row.map_err(|e| Error::Msg(format!("{table}: {e}")))?;
                let mut vals = Vec::with_capacity(row.len());
                for i in 0..row.len() {
                    let v = row.as_ref(i).cloned().unwrap_or(Value::NULL);
                    vals.push(raw_value(&v));
                }
                rows.push(vals);
            }

            let content = crate::export::render(fmt, &columns, &rows);
            let path = format!("{out_dir}/{table}.{}", fmt.ext());
            std::fs::write(&path, content)?;
            files.push(path);
        }
        Ok(files)
    }

    /// Anteprima read-only delle prime `limit` righe di una tabella: stessa
    /// lettura di `export` (stessa `raw_value`), ma con `LIMIT` e senza
    /// scrivere alcun file.
    pub fn peek(conn: &Connection, table: &str, limit: u32) -> Result<(Vec<String>, Vec<Vec<Option<String>>>)> {
        let mut client = connect(conn)?;
        let qr = client
            .query_iter(format!("SELECT * FROM `{table}` LIMIT {limit}"))
            .map_err(|e| Error::Msg(format!("{table}: {e}")))?;
        let columns: Vec<String> = qr
            .columns()
            .as_ref()
            .iter()
            .map(|c| c.name_str().into_owned())
            .collect();

        let mut rows: Vec<Vec<Option<String>>> = Vec::new();
        for row in qr {
            let row = row.map_err(|e| Error::Msg(format!("{table}: {e}")))?;
            let mut vals = Vec::with_capacity(row.len());
            for i in 0..row.len() {
                let v = row.as_ref(i).cloned().unwrap_or(Value::NULL);
                vals.push(raw_value(&v));
            }
            rows.push(vals);
        }
        Ok((columns, rows))
    }

    /// Esegue una query SQL libera. Se il result set restituito ha colonne
    /// (query di lettura, es. SELECT/SHOW) le righe vengono lette con
    /// `raw_value` (stessa lettura di `export`/`peek`); altrimenti (comando
    /// DML/DDL senza result set) si consuma l'iteratore e si legge il numero
    /// di righe modificate da `affected_rows`.
    pub fn run_query(conn: &Connection, sql: &str) -> Result<QueryResult> {
        let mut client = connect(conn)?;
        let qr = client
            .query_iter(sql)
            .map_err(|e| Error::Msg(e.to_string()))?;
        let columns: Vec<String> = qr
            .columns()
            .as_ref()
            .iter()
            .map(|c| c.name_str().into_owned())
            .collect();

        if !columns.is_empty() {
            let mut rows: Vec<Vec<Option<String>>> = Vec::new();
            for row in qr {
                let row = row.map_err(|e| Error::Msg(e.to_string()))?;
                let mut vals = Vec::with_capacity(row.len());
                for i in 0..row.len() {
                    let v = row.as_ref(i).cloned().unwrap_or(Value::NULL);
                    vals.push(raw_value(&v));
                }
                rows.push(vals);
            }
            let message = format!("{} righe", rows.len());
            Ok(QueryResult { columns, rows, affected: None, message })
        } else {
            // Nessun result set (query non-SELECT): consuma comunque
            // l'iteratore (necessario per finalizzare lo statement) prima di
            // leggere il numero di righe modificate.
            for row in qr {
                row.map_err(|e| Error::Msg(e.to_string()))?;
            }
            let n = client.affected_rows();
            Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                affected: Some(n),
                message: format!("Eseguito · {n} righe modificate"),
            })
        }
    }

    pub fn test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
        let mut client = connect(conn)?;
        let version: String = client
            .query_first("SELECT VERSION()")
            .map_err(|e| Error::Conn(e.to_string()))?
            .unwrap_or_default();
        let n: i64 = client
            .query_first(format!(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = '{}' \
                 AND table_type = 'BASE TABLE'",
                esc(&conn.database)
            ))
            .ok()
            .flatten()
            .unwrap_or(0);
        crate::progress::note(log, format!("MySQL/MariaDB {version} — {n} tabelle in {}", conn.database));
        Ok(())
    }
}

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

#[cfg(feature = "mysql-driver")]
mod rustimpl {
    use super::*;
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

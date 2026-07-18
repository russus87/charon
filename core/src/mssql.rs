//! SQL Server: dump/import/clone.
//!
//! - **Nativo**: `mssql-scripter` (dump schema+dati in SQL), `sqlcmd` (import).
//!   `mssql-scripter` e' un tool a parte (si installa con `pip install mssql-scripter`).
//! - **Puro Rust** (`mssql-driver`): driver TDS `tiberius`, dump best-effort via
//!   `FOR JSON` e import eseguendo i batch separati da `GO`.

use crate::compare::{ColumnDiff, DbDiff, RowDelta, Status, TableDataDiff, TableDiff};
use crate::model::*;
use crate::tools::{find_tool, has_tool, plan_or_run, run};
use crate::{Error, Result};
use std::process::Command;

/// Il metodo **nativo** (mssql-scripter/sqlcmd) non gestisce ancora data-only né
/// masking: lo diciamo chiaramente invece di ignorare l'opzione in silenzio. Il
/// data-only è invece supportato dal fallback puro Rust (vedi [`reject_mask`]).
fn reject_unsupported_opts(opts: &CloneOptions) -> Result<()> {
    if opts.data_only || opts.has_mask() {
        return Err(Error::Unsupported(
            "con i tool nativi SQL Server data-only e mascheramento non sono disponibili: \
             usa il metodo puro Rust per il data-only"
                .into(),
        ));
    }
    Ok(())
}

/// Il mascheramento non è ancora implementato per SQL Server (nemmeno in puro
/// Rust); il data-only invece sì. Rifiuta solo il masking.
fn reject_mask(opts: &CloneOptions) -> Result<()> {
    if opts.has_mask() {
        return Err(Error::Unsupported(
            "il mascheramento delle colonne è al momento disponibile solo per PostgreSQL".into(),
        ));
    }
    Ok(())
}

/// `host,port` come vuole sqlcmd/mssql-scripter.
fn server_arg(conn: &Connection) -> String {
    format!("{},{}", conn.host, conn.port)
}

pub fn native_available() -> bool {
    has_tool("sqlcmd") && has_tool("mssql-scripter")
}

pub fn rust_available() -> bool {
    cfg!(feature = "mssql-driver")
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
        tool("mssql-scripter", "genera il dump (schema + dati)"),
        tool("sqlcmd", "esegue/importa script SQL"),
        tool("bcp", "copia massiva di dati (opzionale)"),
    ];
    let native = native_available();
    let rust = rust_available();
    let note = if native {
        "Tool nativi trovati: dump con mssql-scripter, import con sqlcmd.".into()
    } else if has_tool("sqlcmd") {
        "Trovato sqlcmd ma manca mssql-scripter (pip install mssql-scripter) per il dump: \
         verrà usato il fallback puro Rust se necessario."
            .into()
    } else if rust {
        "Tool nativi non trovati: verrà usato il fallback puro Rust (best-effort).".into()
    } else {
        "Nessun metodo disponibile.".into()
    };
    let mut hints = Vec::new();
    if !has_tool("mssql-scripter") {
        hints.push(FixHint::new(
            "Installa mssql-scripter (per il dump)",
            "Genera il dump schema+dati ad alta fedeltà. È un tool Python ufficiale Microsoft:",
            Some("pip install mssql-scripter"),
        ));
    }
    if !has_tool("sqlcmd") {
        hints.push(FixHint::new(
            "Installa sqlcmd (per l'import)",
            "Esegue gli script SQL sul database. Fa parte degli 'mssql-tools' di Microsoft:",
            Some(
                "Arch (AUR):     yay -S mssql-tools\n\
                 Debian/Ubuntu:  segui https://learn.microsoft.com/sql/tools/sqlcmd-utility\n\
                 macOS:          brew install microsoft/mssql-release/mssql-tools\n\
                 Windows:        incluso in \"SQL Server Command Line Utilities\"",
            ),
        ));
    }
    if !native && !rust {
        hints.push(FixHint::new(
            "Nessun metodo disponibile",
            "Mancano i tool nativi e il fallback puro Rust. Ricompila con la feature \
             'mssql-driver' (attiva di default) oppure installa i tool sopra.",
            None,
        ));
    }
    EngineReport {
        engine: Engine::Sqlserver,
        label: Engine::Sqlserver.label().into(),
        tools,
        native_available: native,
        rust_available: rust,
        note,
        hints,
    }
}

// ------------------------------------------------------------------ nativo ---

pub fn native_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("mssql-scripter")
        .ok_or_else(|| Error::ToolMissing("mssql-scripter".into()))?;
    let server = server_arg(conn);
    let mut cmd = Command::new(&exe);
    cmd.arg("-S").arg(&server)
        .arg("-d").arg(&conn.database)
        .arg("-U").arg(&conn.user)
        .arg("-P").arg(&conn.password)
        .arg("--schema-and-data")
        .arg("-f").arg(out);
    let display = format!(
        "mssql-scripter -S {} -d {} -U {} --schema-and-data -f {}",
        server, conn.database, conn.user, out
    );
    if plan_or_run(log, &display, &mut cmd, dry)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("mssql-scripter ha segnalato un errore (vedi log)".into()))
    }
}

pub fn native_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("sqlcmd").ok_or_else(|| Error::ToolMissing("sqlcmd".into()))?;
    let server = server_arg(conn);
    let mut cmd = Command::new(&exe);
    cmd.arg("-S").arg(&server)
        .arg("-d").arg(&conn.database)
        .arg("-U").arg(&conn.user)
        .arg("-P").arg(&conn.password)
        .arg("-b")
        .arg("-i").arg(input);
    let display = format!(
        "sqlcmd -S {} -d {} -U {} -b -i {}",
        server, conn.database, conn.user, input
    );
    if plan_or_run(log, &display, &mut cmd, dry)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("sqlcmd ha segnalato un errore (vedi log)".into()))
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
    let tmp = std::env::temp_dir().join(format!("charon-mssql-{}.sql", std::process::id()));
    let tmp_s = tmp.display().to_string();
    log.push(format!("Dump temporaneo della sorgente in {tmp_s}"));
    native_dump(src, &tmp_s, dry, log)?;
    log.push("Import sul database di destinazione".into());
    let res = native_import(dst, &tmp_s, dry, log);
    let _ = std::fs::remove_file(&tmp);
    res
}

pub fn native_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("sqlcmd").ok_or_else(|| Error::ToolMissing("sqlcmd".into()))?;
    let server = server_arg(conn);
    let mut cmd = Command::new(&exe);
    cmd.arg("-S").arg(&server)
        .arg("-d").arg(&conn.database)
        .arg("-U").arg(&conn.user)
        .arg("-P").arg(&conn.password)
        .arg("-b")
        .arg("-Q").arg("SELECT @@VERSION");
    let display = format!("sqlcmd -S {} -d {} -U {} -Q 'SELECT @@VERSION'", server, conn.database, conn.user);
    if run(log, &display, &mut cmd)?.success {
        Ok(())
    } else {
        Err(Error::Conn("connessione/sqlcmd falliti (vedi log)".into()))
    }
}

// --------------------------------------------------------------- puro Rust ---

pub fn rust_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "mssql-driver")]
    {
        return rustimpl::dump(conn, out, dry, log);
    }
    #[cfg(not(feature = "mssql-driver"))]
    {
        let _ = (conn, out, dry, log);
        Err(Error::Unsupported("fallback SQL Server non disponibile in questa build".into()))
    }
}

pub fn rust_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "mssql-driver")]
    {
        return rustimpl::import(conn, input, dry, log);
    }
    #[cfg(not(feature = "mssql-driver"))]
    {
        let _ = (conn, input, dry, log);
        Err(Error::Unsupported("fallback SQL Server non disponibile in questa build".into()))
    }
}

pub fn rust_clone(
    src: &Connection,
    dst: &Connection,
    opts: &CloneOptions,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    reject_mask(opts)?;
    #[cfg(feature = "mssql-driver")]
    {
        return rustimpl::clone(src, dst, opts.data_only, dry, log);
    }
    #[cfg(not(feature = "mssql-driver"))]
    {
        let _ = (src, dst, dry, log);
        Err(Error::Unsupported("fallback SQL Server non disponibile in questa build".into()))
    }
}

pub fn rust_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "mssql-driver")]
    {
        return rustimpl::test(conn, log);
    }
    #[cfg(not(feature = "mssql-driver"))]
    {
        let _ = (conn, log);
        Err(Error::Unsupported("fallback SQL Server non disponibile in questa build".into()))
    }
}

/// Confronta due database SQL Server (schema + conteggio righe). Solo puro
/// Rust: il confronto legge i cataloghi via tiberius, i tool nativi non c'entrano.
pub fn rust_compare(src: &Connection, dst: &Connection) -> Result<DbDiff> {
    #[cfg(feature = "mssql-driver")]
    {
        return rustimpl::runtime()?.block_on(rustimpl::compare(src, dst));
    }
    #[cfg(not(feature = "mssql-driver"))]
    {
        let _ = (src, dst);
        Err(Error::Unsupported("fallback SQL Server non disponibile in questa build".into()))
    }
}

/// Confronta i **dati** di una singola tabella fra sorgente e destinazione,
/// riga per riga (per chiave primaria, o per riga intera in sua assenza).
/// `table` è nel formato `schema.tabella`, come lo produce [`rust_compare`].
pub fn rust_data_diff(src: &Connection, dst: &Connection, table: &str) -> Result<TableDataDiff> {
    #[cfg(feature = "mssql-driver")]
    {
        return rustimpl::runtime()?.block_on(rustimpl::data_diff(src, dst, table));
    }
    #[cfg(not(feature = "mssql-driver"))]
    {
        let _ = (src, dst, table);
        Err(Error::Unsupported("fallback SQL Server non disponibile in questa build".into()))
    }
}

#[cfg(feature = "mssql-driver")]
mod rustimpl {
    use super::*;
    use serde_json::Value;
    use tiberius::{AuthMethod, Client, Config, Row};
    use tokio::net::TcpStream;
    use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

    type Conn = Client<Compat<TcpStream>>;

    /// Una foreign key raccolta dal catalogo, con le colonne (potenzialmente più
    /// d'una, per le chiavi composte) già in ordine.
    struct Fk {
        name: String,
        child: String,
        referenced: String,
        child_cols: Vec<String>,
        ref_cols: Vec<String>,
    }

    pub(super) fn runtime() -> Result<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::Msg(format!("runtime tokio: {e}")))
    }

    async fn connect(conn: &Connection) -> Result<Conn> {
        let mut config = Config::new();
        config.host(&conn.host);
        config.port(conn.port);
        config.database(&conn.database);
        config.authentication(AuthMethod::sql_server(&conn.user, &conn.password));
        config.trust_cert();
        let tcp = TcpStream::connect(config.get_addr())
            .await
            .map_err(|e| Error::Conn(e.to_string()))?;
        tcp.set_nodelay(true).ok();
        Client::connect(config, tcp.compat_write())
            .await
            .map_err(|e| Error::Conn(e.to_string()))
    }

    async fn query(client: &mut Conn, sql: &str) -> Result<Vec<Row>> {
        client
            .simple_query(sql)
            .await
            .map_err(|e| Error::Msg(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| Error::Msg(e.to_string()))
    }

    fn s(row: &Row, idx: usize) -> String {
        row.try_get::<&str, _>(idx)
            .ok()
            .flatten()
            .unwrap_or("")
            .to_string()
    }

    /// Legge una colonna intera (le query di catalogo la fanno via `CAST(... AS INT)`
    /// per non dipendere dal tipo esatto — bit/tinyint/smallint — restituito).
    fn i(row: &Row, idx: usize) -> Option<i32> {
        row.try_get::<i32, _>(idx).ok().flatten()
    }

    /// Ricostruisce il tipo SQL Server con lunghezza/precisione/scala.
    /// `max_len` è in *byte* come da `sys.columns`: per nchar/nvarchar va dimezzato
    /// per ottenere la lunghezza in caratteri (`-1` = `(max)`).
    fn map_type(dtype: &str, max_len: Option<i32>, prec: Option<i32>, scale: Option<i32>) -> String {
        let len_str = |halve: bool| match max_len {
            Some(-1) => "(max)".to_string(),
            Some(n) => format!("({})", if halve { n / 2 } else { n }),
            None => String::new(),
        };
        match dtype {
            "varchar" | "char" | "binary" | "varbinary" => format!("{dtype}{}", len_str(false)),
            "nvarchar" | "nchar" => format!("{dtype}{}", len_str(true)),
            "decimal" | "numeric" => match (prec, scale) {
                (Some(p), Some(sc)) => format!("{dtype}({p},{sc})"),
                (Some(p), None) => format!("{dtype}({p})"),
                _ => dtype.to_string(),
            },
            "datetime2" | "time" | "datetimeoffset" => match scale {
                Some(sc) => format!("{dtype}({sc})"),
                None => dtype.to_string(),
            },
            other => other.to_string(),
        }
    }

    // ------------------------------------------------------------- compare ---

    /// Elenca (schema, tabella) di tutte le BASE TABLE.
    async fn list_tables(client: &mut Conn) -> Result<Vec<(String, String)>> {
        let rows = query(
            client,
            "SELECT TABLE_SCHEMA, TABLE_NAME FROM INFORMATION_SCHEMA.TABLES \
             WHERE TABLE_TYPE='BASE TABLE' ORDER BY TABLE_SCHEMA, TABLE_NAME",
        )
        .await?;
        Ok(rows.iter().map(|r| (s(r, 0), s(r, 1))).collect())
    }

    /// Colonne di `schema.table` con la loro definizione leggibile (tipo +
    /// eventuale `NOT NULL`), usata per confrontare gli schemi.
    async fn columns_def(client: &mut Conn, schema: &str, table: &str) -> Result<Vec<(String, String)>> {
        let full = format!("[{schema}].[{table}]");
        let rows = query(
            client,
            &format!(
                "SELECT c.name, tp.name, CAST(c.max_length AS INT), CAST(c.precision AS INT), \
                 CAST(c.scale AS INT), CAST(c.is_nullable AS INT) \
                 FROM sys.columns c \
                 JOIN sys.types tp ON tp.user_type_id = c.user_type_id \
                 WHERE c.object_id = OBJECT_ID('{full}') ORDER BY c.column_id"
            ),
        )
        .await?;
        let mut out = Vec::new();
        for r in &rows {
            let name = s(r, 0);
            let dtype = s(r, 1);
            let max_len = i(r, 2);
            let prec = i(r, 3);
            let scale = i(r, 4);
            let nullable = i(r, 5).unwrap_or(1) == 1;
            let mut def = map_type(&dtype, max_len, prec, scale);
            if !nullable {
                def.push_str(" NOT NULL");
            }
            out.push((name, def));
        }
        Ok(out)
    }

    /// Conta le righe di `schema.table`. Best-effort: se non è contabile
    /// (permessi, tabella in stato strano) restituisce `None` invece di far
    /// fallire il diff.
    async fn count_rows(client: &mut Conn, schema: &str, table: &str) -> Option<i64> {
        let full = format!("[{schema}].[{table}]");
        let rows = query(client, &format!("SELECT COUNT(*) FROM {full}")).await.ok()?;
        rows.first().and_then(|r| i(r, 0)).map(|n| n as i64)
    }

    /// Confronta schema e volume dati di due database SQL Server.
    pub async fn compare(src: &Connection, dst: &Connection) -> Result<DbDiff> {
        let mut sc = connect(src).await?;
        let mut dc = connect(dst).await?;
        let mut log = Vec::new();

        let stables = list_tables(&mut sc).await?;
        let dtables = list_tables(&mut dc).await?;
        log.push(format!(
            "Tabelle: {} nella sorgente, {} nella destinazione",
            stables.len(),
            dtables.len()
        ));

        // Nome nel diff = "schema.tabella", così due schemi diversi non si confondono.
        let sfull: Vec<String> = stables.iter().map(|(sch, t)| format!("{sch}.{t}")).collect();
        let dfull: Vec<String> = dtables.iter().map(|(sch, t)| format!("{sch}.{t}")).collect();

        // Unione ordinata+dedup dei nomi visti da almeno una parte.
        let mut names: Vec<String> = sfull.iter().chain(dfull.iter()).cloned().collect();
        names.sort();
        names.dedup();

        let mut tables = Vec::new();
        for name in names {
            let in_s = sfull.contains(&name);
            let in_d = dfull.contains(&name);
            let (schema, table) = name.split_once('.').unwrap_or(("dbo", name.as_str()));
            let (schema, table) = (schema.to_string(), table.to_string());

            // Tabella presente da un solo lato: tutte le colonne sono "nuove".
            if in_s != in_d {
                let status = if in_s { Status::OnlySource } else { Status::OnlyTarget };
                let cols = if in_s {
                    columns_def(&mut sc, &schema, &table).await?
                } else {
                    columns_def(&mut dc, &schema, &table).await?
                };
                let rows = if in_s {
                    count_rows(&mut sc, &schema, &table).await
                } else {
                    count_rows(&mut dc, &schema, &table).await
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
            let scols = columns_def(&mut sc, &schema, &table).await?;
            let dcols = columns_def(&mut dc, &schema, &table).await?;
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
            let source_rows = count_rows(&mut sc, &schema, &table).await;
            let target_rows = count_rows(&mut dc, &schema, &table).await;
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

    /// Confronta i dati di `schema.tabella` fra sorgente e destinazione, riga
    /// per riga. Usa la chiave primaria se presente (stessa query su
    /// `sys.indexes`/`sys.index_columns` di `dump_sql`); altrimenti confronta
    /// la riga intera (nessuna chiave = ogni riga è la propria chiave).
    pub async fn data_diff(src: &Connection, dst: &Connection, table: &str) -> Result<TableDataDiff> {
        let (schema, name) = table.split_once('.').unwrap_or(("dbo", table));
        let full = format!("[{schema}].[{name}]");

        let mut sc = connect(src).await?;
        let mut dc = connect(dst).await?;

        // Colonne chiave: la PK (se esiste) letta dal lato sorgente. Assumiamo
        // che, se le due tabelle sono davvero "la stessa tabella", la PK sia
        // la stessa da entrambe le parti.
        let pk_rows = query(
            &mut sc,
            &format!(
                "SELECT c.name FROM sys.indexes i \
                 JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
                 JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
                 WHERE i.is_primary_key = 1 AND i.object_id = OBJECT_ID('{full}') \
                 ORDER BY ic.key_ordinal"
            ),
        )
        .await?;
        let key_cols: Vec<String> = pk_rows.iter().map(|r| s(r, 0)).collect();
        let note = if key_cols.is_empty() {
            Some("nessuna chiave primaria: confronto per riga intera".into())
        } else {
            None
        };

        // Legge tutte le righe di un lato come mappa key_str -> val_str.
        async fn rows_map(client: &mut Conn, full: &str, key_cols: &[String]) -> Result<std::collections::HashMap<String, String>> {
            let json_rows = query(
                client,
                &format!("SELECT * FROM {full} FOR JSON PATH, INCLUDE_NULL_VALUES"),
            )
            .await?;
            let mut json = String::new();
            for r in &json_rows {
                json.push_str(&s(r, 0));
            }
            let mut out = std::collections::HashMap::new();
            if json.trim().is_empty() {
                return Ok(out);
            }
            let parsed: Value = serde_json::from_str(&json).map_err(|e| Error::Msg(e.to_string()))?;
            if let Value::Array(items) = parsed {
                for item in items {
                    if let Value::Object(map) = &item {
                        if key_cols.is_empty() {
                            // Nessuna PK: la riga intera è la chiave, nessun valore da confrontare a parte.
                            out.insert(item.to_string(), String::new());
                        } else {
                            let key_str = key_cols
                                .iter()
                                .map(|k| map.get(k).unwrap_or(&Value::Null).to_string())
                                .collect::<Vec<_>>()
                                .join("\u{1}");
                            let val_str = map
                                .iter()
                                .filter(|(k, _)| !key_cols.contains(k))
                                .map(|(_, v)| v.to_string())
                                .collect::<Vec<_>>()
                                .join("\u{1}");
                            out.insert(key_str, val_str);
                        }
                    }
                }
            }
            Ok(out)
        }

        let smap = rows_map(&mut sc, &full, &key_cols).await?;
        let dmap = rows_map(&mut dc, &full, &key_cols).await?;

        let mut only_source = 0i64;
        let mut only_target = 0i64;
        let mut changed = 0i64;
        let mut same = 0i64;
        let mut sample: Vec<RowDelta> = Vec::new();

        // Rende leggibile la chiave per il campione: "col=val, col2=val2" quando
        // c'è una PK, altrimenti la riga intera troncata.
        let key_label = |key_str: &str| -> String {
            if key_cols.is_empty() {
                let mut s = key_str.to_string();
                if s.len() > 120 {
                    s.truncate(120);
                    s.push('…');
                }
                s
            } else {
                key_cols
                    .iter()
                    .zip(key_str.split('\u{1}'))
                    .map(|(c, v)| format!("{c}={v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        };

        for (k, sv) in &smap {
            match dmap.get(k) {
                None => {
                    only_source += 1;
                    if sample.len() < 50 {
                        sample.push(RowDelta { key: key_label(k), kind: Status::OnlySource });
                    }
                }
                Some(dv) if dv != sv => {
                    changed += 1;
                    if sample.len() < 50 {
                        sample.push(RowDelta { key: key_label(k), kind: Status::Changed });
                    }
                }
                Some(_) => same += 1,
            }
        }
        for k in dmap.keys() {
            if !smap.contains_key(k) {
                only_target += 1;
                if sample.len() < 50 {
                    sample.push(RowDelta { key: key_label(k), kind: Status::OnlyTarget });
                }
            }
        }

        Ok(TableDataDiff {
            table: table.into(),
            key: key_cols,
            only_source,
            only_target,
            changed,
            same,
            sample,
            note,
        })
    }

    fn lit(v: &Value) -> String {
        match v {
            Value::Null => "NULL".into(),
            Value::Bool(b) => if *b { "1" } else { "0" }.into(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => format!("N'{}'", s.replace('\'', "''")),
            other => format!("N'{}'", other.to_string().replace('\'', "''")),
        }
    }

    async fn dump_sql(conn: &Connection) -> Result<String> {
        let mut client = connect(conn).await?;

        let trows = query(
            &mut client,
            "SELECT TABLE_SCHEMA, TABLE_NAME FROM INFORMATION_SCHEMA.TABLES \
             WHERE TABLE_TYPE='BASE TABLE' ORDER BY TABLE_SCHEMA, TABLE_NAME",
        )
        .await?;

        let mut out = String::new();
        out.push_str("-- Dump generato da Charon — fallback puro Rust (best-effort).\n");
        out.push_str("-- Include schemi, PK, IDENTITY, UNIQUE, CHECK, DEFAULT e foreign key.\n");
        out.push_str("-- Le FK (e gli UNIQUE che ne fanno da bersaglio) sono aggiunte in coda,\n");
        out.push_str("-- dopo i dati, così l'ordine di creazione delle tabelle è irrilevante.\n");
        out.push_str("-- Per fedeltà completa (indici non-unici, trigger…) usa mssql-scripter.\n\n");

        // Fase 0: rimuove tutte le foreign key già presenti sulla destinazione,
        // altrimenti i DROP TABLE seguenti falliscono per le dipendenze.
        out.push_str("-- Rimozione difensiva delle foreign key preesistenti\n");
        out.push_str(
            "DECLARE @drop_fk NVARCHAR(MAX)=N'';\n\
             SELECT @drop_fk += 'ALTER TABLE '+QUOTENAME(SCHEMA_NAME(schema_id))+'.'\
             +QUOTENAME(OBJECT_NAME(parent_object_id))+' DROP CONSTRAINT '+QUOTENAME(name)+';'\n\
             FROM sys.foreign_keys;\n\
             IF LEN(@drop_fk) > 0 EXEC sp_executesql @drop_fk;\nGO\n\n",
        );

        // Fase 0b: crea gli schemi non-dbo prima delle tabelle. `CREATE SCHEMA` deve
        // essere la prima istruzione del batch, quindi lo eseguiamo via EXEC.
        let mut schemas: Vec<String> = Vec::new();
        for tr in &trows {
            let sc = s(tr, 0);
            if sc != "dbo" && !schemas.contains(&sc) {
                schemas.push(sc);
            }
        }
        if !schemas.is_empty() {
            out.push_str("-- Schemi non-dbo\n");
            for sc in &schemas {
                out.push_str(&format!(
                    "IF SCHEMA_ID(N'{sc}') IS NULL EXEC(N'CREATE SCHEMA [{sc}]');\nGO\n"
                ));
            }
            out.push('\n');
        }

        for tr in &trows {
            let schema = s(tr, 0);
            let table = s(tr, 1);
            let full = format!("[{schema}].[{table}]");

            // Colonne: tipo, precisione/scala, nullabilità, IDENTITY e computed.
            let cols = query(
                &mut client,
                &format!(
                    "SELECT c.name, tp.name, CAST(c.max_length AS INT), CAST(c.precision AS INT), \
                     CAST(c.scale AS INT), CAST(c.is_nullable AS INT), CAST(c.is_identity AS INT), \
                     CAST(c.is_computed AS INT), cc.definition \
                     FROM sys.columns c \
                     JOIN sys.types tp ON tp.user_type_id = c.user_type_id \
                     LEFT JOIN sys.computed_columns cc \
                       ON cc.object_id = c.object_id AND cc.column_id = c.column_id \
                     WHERE c.object_id = OBJECT_ID('{full}') ORDER BY c.column_id"
                ),
            )
            .await?;

            // Chiave primaria (colonne in ordine di chiave).
            let pk = query(
                &mut client,
                &format!(
                    "SELECT i.name, c.name FROM sys.indexes i \
                     JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
                     JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
                     WHERE i.is_primary_key = 1 AND i.object_id = OBJECT_ID('{full}') \
                     ORDER BY ic.key_ordinal"
                ),
            )
            .await?;

            let mut insert_cols: Vec<String> = Vec::new();
            let mut has_identity = false;

            out.push_str(&format!("IF OBJECT_ID('{full}','U') IS NOT NULL DROP TABLE {full};\nGO\n"));
            out.push_str(&format!("CREATE TABLE {full} (\n"));
            let mut defs = Vec::new();
            for c in &cols {
                let name = s(c, 0);
                let dtype = s(c, 1);
                let max_len = i(c, 2);
                let prec = i(c, 3);
                let scale = i(c, 4);
                let nullable = i(c, 5).unwrap_or(1) == 1;
                let identity = i(c, 6).unwrap_or(0) == 1;
                let computed = i(c, 7).unwrap_or(0) == 1;
                let comp_def = s(c, 8);

                // Le colonne computed non si inseriscono: le ricreiamo (se abbiamo
                // la definizione) e le escludiamo dagli INSERT.
                if computed {
                    if !comp_def.is_empty() {
                        defs.push(format!("  [{name}] AS {comp_def}"));
                    }
                    continue;
                }
                insert_cols.push(name.clone());
                let mut def = format!("  [{name}] {}", map_type(&dtype, max_len, prec, scale));
                if identity {
                    def.push_str(" IDENTITY(1,1)");
                    has_identity = true;
                }
                def.push_str(if nullable { " NULL" } else { " NOT NULL" });
                defs.push(def);
            }
            if !pk.is_empty() {
                let raw = s(&pk[0], 0);
                let pk_name = if raw.is_empty() { format!("PK_{schema}_{table}") } else { raw };
                let pk_cols = pk
                    .iter()
                    .map(|r| format!("[{}]", s(r, 1)))
                    .collect::<Vec<_>>()
                    .join(", ");
                defs.push(format!("  CONSTRAINT [{pk_name}] PRIMARY KEY ({pk_cols})"));
            }
            out.push_str(&defs.join(",\n"));
            out.push_str("\n);\nGO\n");

            // Dati: FOR JSON restituisce un'unica stringa JSON (a pezzi su piu' righe).
            let json_rows = query(
                &mut client,
                &format!("SELECT * FROM {full} FOR JSON PATH, INCLUDE_NULL_VALUES"),
            )
            .await?;
            let mut json = String::new();
            for r in &json_rows {
                json.push_str(&s(r, 0));
            }
            if !json.trim().is_empty() {
                let parsed: Value =
                    serde_json::from_str(&json).map_err(|e| Error::Msg(e.to_string()))?;
                if let Value::Array(items) = parsed {
                    if !items.is_empty() {
                        let collist = insert_cols
                            .iter()
                            .map(|n| format!("[{n}]"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        // IDENTITY_INSERT è di sessione: on/off in batch separati, così
                        // se gli INSERT falliscono l'OFF viene comunque eseguito e la
                        // tabella successiva non eredita lo stato.
                        if has_identity {
                            out.push_str(&format!("SET IDENTITY_INSERT {full} ON;\nGO\n"));
                        }
                        for item in &items {
                            let vals = insert_cols
                                .iter()
                                .map(|n| lit(item.get(n).unwrap_or(&Value::Null)))
                                .collect::<Vec<_>>()
                                .join(", ");
                            out.push_str(&format!("INSERT INTO {full} ({collist}) VALUES ({vals});\n"));
                        }
                        out.push_str("GO\n");
                        if has_identity {
                            out.push_str(&format!("SET IDENTITY_INSERT {full} OFF;\nGO\n"));
                        }
                    }
                }
            }
            out.push('\n');
        }

        // Vincoli/indici UNIQUE (non-PK). Servono anche come bersaglio valido per le
        // FK che non riferiscono la chiave primaria, quindi vanno PRIMA delle FK.
        // Escludiamo gli indici filtrati (has_filter): non possono essere bersaglio
        // di FK e ricrearli senza filtro cambierebbe la semantica.
        let urows = query(
            &mut client,
            "SELECT SCHEMA_NAME(t.schema_id), t.name, i.name, c.name, \
             CAST(i.is_unique_constraint AS INT) \
             FROM sys.indexes i \
             JOIN sys.tables t ON t.object_id = i.object_id \
             JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
             JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
             WHERE i.is_unique = 1 AND i.is_primary_key = 0 AND i.has_filter = 0 \
               AND ic.is_included_column = 0 \
             ORDER BY SCHEMA_NAME(t.schema_id), t.name, i.name, ic.key_ordinal",
        )
        .await?;

        struct Uniq {
            parent: String,
            name: String,
            is_constraint: bool,
            cols: Vec<String>,
        }
        let mut uniqs: Vec<Uniq> = Vec::new();
        for r in &urows {
            let parent = format!("[{}].[{}]", s(r, 0), s(r, 1));
            let name = s(r, 2);
            let col = s(r, 3);
            let is_con = i(r, 4).unwrap_or(0) == 1;
            if matches!(uniqs.last(), Some(u) if u.parent == parent && u.name == name) {
                uniqs.last_mut().unwrap().cols.push(col);
            } else {
                uniqs.push(Uniq { parent, name, is_constraint: is_con, cols: vec![col] });
            }
        }
        if !uniqs.is_empty() {
            out.push_str("-- Vincoli e indici UNIQUE\n");
            for u in &uniqs {
                let cols = u.cols.iter().map(|c| format!("[{c}]")).collect::<Vec<_>>().join(", ");
                if u.is_constraint {
                    out.push_str(&format!(
                        "ALTER TABLE {} ADD CONSTRAINT [{}] UNIQUE ({cols});\nGO\n",
                        u.parent, u.name
                    ));
                } else {
                    out.push_str(&format!(
                        "CREATE UNIQUE INDEX [{}] ON {} ({cols});\nGO\n",
                        u.name, u.parent
                    ));
                }
            }
            out.push('\n');
        }

        // Vincoli CHECK. `definition` è già racchiusa tra parentesi (es. `([eta]>(0))`).
        let crows = query(
            &mut client,
            "SELECT SCHEMA_NAME(t.schema_id), t.name, cc.name, cc.definition \
             FROM sys.check_constraints cc \
             JOIN sys.tables t ON t.object_id = cc.parent_object_id \
             ORDER BY SCHEMA_NAME(t.schema_id), t.name, cc.name",
        )
        .await?;
        if !crows.is_empty() {
            out.push_str("-- Vincoli CHECK\n");
            for r in &crows {
                let parent = format!("[{}].[{}]", s(r, 0), s(r, 1));
                let name = s(r, 2);
                let def = s(r, 3);
                out.push_str(&format!(
                    "ALTER TABLE {parent} ADD CONSTRAINT [{name}] CHECK {def};\nGO\n"
                ));
            }
            out.push('\n');
        }

        // Vincoli DEFAULT. Non influiscono sugli INSERT (inseriamo valori espliciti):
        // servono per i futuri inserimenti sulla destinazione. `definition` è già
        // racchiusa tra parentesi (es. `((0))`, `(getdate())`).
        let drows = query(
            &mut client,
            "SELECT SCHEMA_NAME(t.schema_id), t.name, dc.name, c.name, dc.definition \
             FROM sys.default_constraints dc \
             JOIN sys.tables t ON t.object_id = dc.parent_object_id \
             JOIN sys.columns c ON c.object_id = dc.parent_object_id AND c.column_id = dc.parent_column_id \
             ORDER BY SCHEMA_NAME(t.schema_id), t.name, dc.name",
        )
        .await?;
        if !drows.is_empty() {
            out.push_str("-- Vincoli DEFAULT\n");
            for r in &drows {
                let parent = format!("[{}].[{}]", s(r, 0), s(r, 1));
                let name = s(r, 2);
                let col = s(r, 3);
                let def = s(r, 4);
                out.push_str(&format!(
                    "ALTER TABLE {parent} ADD CONSTRAINT [{name}] DEFAULT {def} FOR [{col}];\nGO\n"
                ));
            }
            out.push('\n');
        }

        // Fase finale: foreign key. Una riga per (fk, colonna); le raggruppiamo per
        // nome di vincolo mantenendo l'ordine delle colonne.
        let fkrows = query(
            &mut client,
            "SELECT fk.name, SCHEMA_NAME(fk.schema_id), OBJECT_NAME(fk.parent_object_id), \
             COL_NAME(fkc.parent_object_id, fkc.parent_column_id), \
             SCHEMA_NAME(rt.schema_id), OBJECT_NAME(fk.referenced_object_id), \
             COL_NAME(fkc.referenced_object_id, fkc.referenced_column_id) \
             FROM sys.foreign_keys fk \
             JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id \
             JOIN sys.tables rt ON rt.object_id = fk.referenced_object_id \
             ORDER BY fk.name, fkc.constraint_column_id",
        )
        .await?;

        let mut fks: Vec<Fk> = Vec::new();
        for r in &fkrows {
            let name = s(r, 0);
            let pcol = s(r, 3);
            let rcol = s(r, 6);
            if matches!(fks.last(), Some(l) if l.name == name) {
                let l = fks.last_mut().unwrap();
                l.child_cols.push(pcol);
                l.ref_cols.push(rcol);
            } else {
                fks.push(Fk {
                    name,
                    child: format!("[{}].[{}]", s(r, 1), s(r, 2)),
                    referenced: format!("[{}].[{}]", s(r, 4), s(r, 5)),
                    child_cols: vec![pcol],
                    ref_cols: vec![rcol],
                });
            }
        }
        if !fks.is_empty() {
            out.push_str("-- Foreign key (aggiunte dopo i dati)\n");
            for fk in &fks {
                let pc = fk.child_cols.iter().map(|c| format!("[{c}]")).collect::<Vec<_>>().join(", ");
                let rc = fk.ref_cols.iter().map(|c| format!("[{c}]")).collect::<Vec<_>>().join(", ");
                out.push_str(&format!(
                    "ALTER TABLE {} ADD CONSTRAINT [{}] FOREIGN KEY ({pc}) REFERENCES {} ({rc});\nGO\n",
                    fk.child, fk.name, fk.referenced
                ));
            }
        }
        Ok(out)
    }

    /// Genera uno script **solo dati (append)** da eseguire sulla *destinazione*:
    /// legge le righe dalla sorgente e produce solo gli INSERT, **senza** toccare lo
    /// schema. Le foreign key/CHECK della destinazione vengono disabilitate prima del
    /// travaso (così l'ordine di inserimento è irrilevante) e riattivate — con
    /// rivalidazione — alla fine. Modalità *append*: non svuota le tabelle esistenti.
    async fn data_only_sql(conn: &Connection) -> Result<String> {
        let mut client = connect(conn).await?;

        let trows = query(
            &mut client,
            "SELECT TABLE_SCHEMA, TABLE_NAME FROM INFORMATION_SCHEMA.TABLES \
             WHERE TABLE_TYPE='BASE TABLE' ORDER BY TABLE_SCHEMA, TABLE_NAME",
        )
        .await?;

        let mut out = String::new();
        out.push_str("-- Dump SOLO DATI (append) generato da Charon — fallback puro Rust.\n");
        out.push_str("-- Lo schema della destinazione NON viene toccato. Le FK/CHECK sono\n");
        out.push_str("-- disabilitate durante il travaso e riattivate (rivalidate) alla fine.\n\n");

        // Disabilita FK e CHECK su tutte le tabelle della destinazione. La query
        // dinamica gira sulla destinazione, quindi copre le sue tabelle a runtime.
        out.push_str("-- Disabilita i vincoli FK/CHECK sulla destinazione\n");
        out.push_str(
            "DECLARE @nocheck NVARCHAR(MAX)=N'';\n\
             SELECT @nocheck += 'ALTER TABLE '+QUOTENAME(SCHEMA_NAME(schema_id))+'.'\
             +QUOTENAME(name)+' NOCHECK CONSTRAINT ALL;'\n\
             FROM sys.tables;\n\
             IF LEN(@nocheck) > 0 EXEC sp_executesql @nocheck;\nGO\n\n",
        );

        for tr in &trows {
            let schema = s(tr, 0);
            let table = s(tr, 1);
            let full = format!("[{schema}].[{table}]");

            // Colonne inseribili (escluse le computed) + presenza di IDENTITY.
            let cols = query(
                &mut client,
                &format!(
                    "SELECT c.name, CAST(c.is_identity AS INT), CAST(c.is_computed AS INT) \
                     FROM sys.columns c WHERE c.object_id = OBJECT_ID('{full}') ORDER BY c.column_id"
                ),
            )
            .await?;
            let mut insert_cols: Vec<String> = Vec::new();
            let mut has_identity = false;
            for c in &cols {
                let computed = i(c, 2).unwrap_or(0) == 1;
                if computed {
                    continue;
                }
                if i(c, 1).unwrap_or(0) == 1 {
                    has_identity = true;
                }
                insert_cols.push(s(c, 0));
            }
            if insert_cols.is_empty() {
                continue;
            }

            let json_rows = query(
                &mut client,
                &format!("SELECT * FROM {full} FOR JSON PATH, INCLUDE_NULL_VALUES"),
            )
            .await?;
            let mut json = String::new();
            for r in &json_rows {
                json.push_str(&s(r, 0));
            }
            if json.trim().is_empty() {
                continue;
            }
            let parsed: Value =
                serde_json::from_str(&json).map_err(|e| Error::Msg(e.to_string()))?;
            if let Value::Array(items) = parsed {
                if items.is_empty() {
                    continue;
                }
                let collist = insert_cols
                    .iter()
                    .map(|n| format!("[{n}]"))
                    .collect::<Vec<_>>()
                    .join(", ");
                if has_identity {
                    out.push_str(&format!("SET IDENTITY_INSERT {full} ON;\nGO\n"));
                }
                for item in &items {
                    let vals = insert_cols
                        .iter()
                        .map(|n| lit(item.get(n).unwrap_or(&Value::Null)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("INSERT INTO {full} ({collist}) VALUES ({vals});\n"));
                }
                out.push_str("GO\n");
                if has_identity {
                    out.push_str(&format!("SET IDENTITY_INSERT {full} OFF;\nGO\n"));
                }
                out.push('\n');
            }
        }

        // Riattiva e rivalida i vincoli. Se un CHECK/FK fallisce la rivalidazione
        // (dati incoerenti dopo l'append) run_script lo registra e prosegue.
        out.push_str("-- Riattiva e rivalida i vincoli FK/CHECK\n");
        out.push_str(
            "DECLARE @recheck NVARCHAR(MAX)=N'';\n\
             SELECT @recheck += 'ALTER TABLE '+QUOTENAME(SCHEMA_NAME(schema_id))+'.'\
             +QUOTENAME(name)+' WITH CHECK CHECK CONSTRAINT ALL;'\n\
             FROM sys.tables;\n\
             IF LEN(@recheck) > 0 EXEC sp_executesql @recheck;\nGO\n",
        );

        Ok(out)
    }

    async fn run_script(conn: &Connection, sql: &str, log: &mut Vec<String>) -> Result<()> {
        let mut client = connect(conn).await?;
        // sqlcmd separa i batch con righe contenenti solo "GO".
        let mut batches: Vec<String> = Vec::new();
        let mut cur = String::new();
        for line in sql.lines() {
            if line.trim().eq_ignore_ascii_case("go") {
                if !cur.trim().is_empty() {
                    batches.push(std::mem::take(&mut cur));
                }
                cur.clear();
            } else {
                cur.push_str(line);
                cur.push('\n');
            }
        }
        if !cur.trim().is_empty() {
            batches.push(cur);
        }

        // Continua anche se un batch fallisce: registra la causa e va avanti, così
        // un singolo INSERT/tabella problematica non lascia la destinazione con una
        // sola tabella. I batch già eseguiti restano (autocommit).
        let mut failures = 0usize;
        for b in &batches {
            let outcome: std::result::Result<(), String> = match client.simple_query(b.as_str()).await {
                Ok(stream) => stream.into_results().await.map(|_| ()).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            if let Err(msg) = outcome {
                failures += 1;
                let head: String = b.trim().chars().take(140).collect();
                log.push(format!(
                    "⚠ batch non applicato: {} | causa: {msg}",
                    head.replace('\n', " ")
                ));
            }
        }
        if failures > 0 {
            return Err(Error::Msg(format!(
                "{failures} batch su {} non applicati (gli altri sì): vedi il log per le cause.",
                batches.len()
            )));
        }
        Ok(())
    }

    pub fn dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
        log.push("Connessione con tiberius…".into());
        let sql = runtime()?.block_on(dump_sql(conn))?;
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
        log.push(format!("Dump SQL ({tables} tabelle, {rows} INSERT) scritto in {out}"));
        Ok(())
    }

    pub fn import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
        let sql = std::fs::read_to_string(input)?;
        if dry {
            let inserts = sql.matches("INSERT INTO ").count();
            log.push(format!(
                "Dry-run: {input} verrebbe eseguito ({inserts} INSERT, {} righe). Nessuna modifica applicata.",
                sql.lines().count()
            ));
            return Ok(());
        }
        log.push(format!("Esecuzione di {input} (batch separati da GO)…"));
        runtime()?.block_on(run_script(conn, &sql, log))?;
        log.push("Import completato.".into());
        Ok(())
    }

    pub fn clone(
        src: &Connection,
        dst: &Connection,
        data_only: bool,
        dry: bool,
        log: &mut Vec<String>,
    ) -> Result<()> {
        let rt = runtime()?;
        if data_only {
            return rt.block_on(clone_data_only(src, dst, dry, log));
        }
        log.push("Lettura schema+dati dalla sorgente…".into());
        let sql = rt.block_on(dump_sql(src))?;
        if dry {
            let tables = sql.matches("CREATE TABLE ").count();
            let rows = sql.matches("INSERT INTO ").count();
            log.push(format!(
                "Dry-run: verrebbero ricreate {tables} tabelle e inserite {rows} righe su {}:{}/{}. Destinazione non modificata.",
                dst.host, dst.port, dst.database
            ));
            return Ok(());
        }
        log.push("Scrittura sul database di destinazione…".into());
        rt.block_on(run_script(dst, &sql, log))?;
        log.push("Clonazione completata.".into());
        Ok(())
    }

    /// Clone **solo dati (append)**: preserva lo schema della destinazione e vi
    /// riversa i dati della sorgente, disabilitando le FK/CHECK durante il travaso.
    async fn clone_data_only(
        src: &Connection,
        dst: &Connection,
        dry: bool,
        log: &mut Vec<String>,
    ) -> Result<()> {
        log.push("Modalità solo-dati (append): lo schema della destinazione resta intatto.".into());
        log.push("Lettura dati dalla sorgente…".into());
        let sql = data_only_sql(src).await?;
        if dry {
            let rows = sql.matches("INSERT INTO ").count();
            log.push(format!(
                "Dry-run: verrebbero inserite (append) {rows} righe su {}:{}/{}, con FK/CHECK \
                 disabilitate durante il travaso e riattivate dopo. Destinazione non modificata.",
                dst.host, dst.port, dst.database
            ));
            return Ok(());
        }
        log.push("Inserimento dati sulla destinazione (FK/CHECK disabilitate durante il travaso)…".into());
        run_script(dst, &sql, log).await?;
        log.push("Copia dei dati completata (vincoli riattivati).".into());
        Ok(())
    }

    pub fn test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
        runtime()?.block_on(async {
            let mut client = connect(conn).await?;
            let rows = query(&mut client, "SELECT @@VERSION").await?;
            if let Some(r) = rows.first() {
                log.push(s(r, 0));
            }
            Ok::<(), Error>(())
        })
    }
}

//! SQL Server: dump/import/clone.
//!
//! - **Nativo**: `mssql-scripter` (dump schema+dati in SQL), `sqlcmd` (import).
//!   `mssql-scripter` e' un tool a parte (si installa con `pip install mssql-scripter`).
//! - **Puro Rust** (`mssql-driver`): driver TDS `tiberius`, dump best-effort via
//!   `FOR JSON` e import eseguendo i batch separati da `GO`.

use crate::model::*;
use crate::tools::{find_tool, has_tool, run};
use crate::{Error, Result};
use std::process::Command;

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

pub fn native_dump(conn: &Connection, out: &str, log: &mut Vec<String>) -> Result<()> {
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
    if run(log, &display, &mut cmd)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("mssql-scripter ha segnalato un errore (vedi log)".into()))
    }
}

pub fn native_import(conn: &Connection, input: &str, log: &mut Vec<String>) -> Result<()> {
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
    if run(log, &display, &mut cmd)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("sqlcmd ha segnalato un errore (vedi log)".into()))
    }
}

pub fn native_clone(src: &Connection, dst: &Connection, log: &mut Vec<String>) -> Result<()> {
    let tmp = std::env::temp_dir().join(format!("charon-mssql-{}.sql", std::process::id()));
    let tmp_s = tmp.display().to_string();
    log.push(format!("Dump temporaneo della sorgente in {tmp_s}"));
    native_dump(src, &tmp_s, log)?;
    log.push("Import sul database di destinazione".into());
    let res = native_import(dst, &tmp_s, log);
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

pub fn rust_dump(conn: &Connection, out: &str, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "mssql-driver")]
    {
        return rustimpl::dump(conn, out, log);
    }
    #[cfg(not(feature = "mssql-driver"))]
    {
        let _ = (conn, out, log);
        Err(Error::Unsupported("fallback SQL Server non disponibile in questa build".into()))
    }
}

pub fn rust_import(conn: &Connection, input: &str, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "mssql-driver")]
    {
        return rustimpl::import(conn, input, log);
    }
    #[cfg(not(feature = "mssql-driver"))]
    {
        let _ = (conn, input, log);
        Err(Error::Unsupported("fallback SQL Server non disponibile in questa build".into()))
    }
}

pub fn rust_clone(src: &Connection, dst: &Connection, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "mssql-driver")]
    {
        return rustimpl::clone(src, dst, log);
    }
    #[cfg(not(feature = "mssql-driver"))]
    {
        let _ = (src, dst, log);
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

#[cfg(feature = "mssql-driver")]
mod rustimpl {
    use super::*;
    use serde_json::Value;
    use tiberius::{AuthMethod, Client, Config, Row};
    use tokio::net::TcpStream;
    use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

    type Conn = Client<Compat<TcpStream>>;

    fn runtime() -> Result<tokio::runtime::Runtime> {
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

    fn map_type(dtype: &str, maxlen: Option<i32>) -> String {
        match dtype {
            "varchar" | "nvarchar" | "char" | "nchar" => match maxlen {
                Some(-1) => format!("{dtype}(max)"),
                Some(n) => format!("{dtype}({n})"),
                None => dtype.into(),
            },
            other => other.to_string(),
        }
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
        out.push_str("-- Per fedeltà completa usa mssql-scripter.\n\n");

        for tr in &trows {
            let schema = s(tr, 0);
            let table = s(tr, 1);
            let full = format!("[{schema}].[{table}]");

            let cols = query(
                &mut client,
                &format!(
                    "SELECT COLUMN_NAME, DATA_TYPE, CHARACTER_MAXIMUM_LENGTH, IS_NULLABLE \
                     FROM INFORMATION_SCHEMA.COLUMNS \
                     WHERE TABLE_SCHEMA='{schema}' AND TABLE_NAME='{table}' \
                     ORDER BY ORDINAL_POSITION"
                ),
            )
            .await?;

            let col_names: Vec<String> = cols.iter().map(|c| s(c, 0)).collect();

            out.push_str(&format!("IF OBJECT_ID('{full}','U') IS NOT NULL DROP TABLE {full};\nGO\n"));
            out.push_str(&format!("CREATE TABLE {full} (\n"));
            let mut defs = Vec::new();
            for c in &cols {
                let name = s(c, 0);
                let dtype = s(c, 1);
                let maxlen = c.try_get::<i32, _>(2).ok().flatten();
                let nullable = s(c, 3);
                let mut def = format!("  [{}] {}", name, map_type(&dtype, maxlen));
                def.push_str(if nullable == "NO" { " NOT NULL" } else { " NULL" });
                defs.push(def);
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
                    let collist = col_names
                        .iter()
                        .map(|n| format!("[{n}]"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    for item in &items {
                        let vals = col_names
                            .iter()
                            .map(|n| lit(item.get(n).unwrap_or(&Value::Null)))
                            .collect::<Vec<_>>()
                            .join(", ");
                        out.push_str(&format!("INSERT INTO {full} ({collist}) VALUES ({vals});\n"));
                    }
                    out.push_str("GO\n");
                }
            }
            out.push('\n');
        }
        Ok(out)
    }

    async fn run_script(conn: &Connection, sql: &str) -> Result<()> {
        let mut client = connect(conn).await?;
        // sqlcmd separa i batch con righe contenenti solo "GO".
        let mut batch = String::new();
        for line in sql.lines() {
            if line.trim().eq_ignore_ascii_case("go") {
                if !batch.trim().is_empty() {
                    client
                        .simple_query(batch.as_str())
                        .await
                        .map_err(|e| Error::Msg(e.to_string()))?
                        .into_results()
                        .await
                        .map_err(|e| Error::Msg(e.to_string()))?;
                }
                batch.clear();
            } else {
                batch.push_str(line);
                batch.push('\n');
            }
        }
        if !batch.trim().is_empty() {
            client
                .simple_query(batch.as_str())
                .await
                .map_err(|e| Error::Msg(e.to_string()))?
                .into_results()
                .await
                .map_err(|e| Error::Msg(e.to_string()))?;
        }
        Ok(())
    }

    pub fn dump(conn: &Connection, out: &str, log: &mut Vec<String>) -> Result<()> {
        log.push("Connessione con tiberius…".into());
        let sql = runtime()?.block_on(dump_sql(conn))?;
        std::fs::write(out, sql)?;
        log.push(format!("Dump SQL scritto in {out}"));
        Ok(())
    }

    pub fn import(conn: &Connection, input: &str, log: &mut Vec<String>) -> Result<()> {
        let sql = std::fs::read_to_string(input)?;
        log.push(format!("Esecuzione di {input} (batch separati da GO)…"));
        runtime()?.block_on(run_script(conn, &sql))?;
        log.push("Import completato.".into());
        Ok(())
    }

    pub fn clone(src: &Connection, dst: &Connection, log: &mut Vec<String>) -> Result<()> {
        let rt = runtime()?;
        log.push("Lettura schema+dati dalla sorgente…".into());
        let sql = rt.block_on(dump_sql(src))?;
        log.push("Scrittura sul database di destinazione…".into());
        rt.block_on(run_script(dst, &sql))?;
        log.push("Clonazione completata.".into());
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

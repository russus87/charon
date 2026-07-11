//! PostgreSQL: dump/import/clone.
//!
//! - **Nativo**: `pg_dump` (dump SQL), `psql` (import/esecuzione script).
//! - **Puro Rust** (`pg-driver`): si connette con `tokio-postgres` e genera un
//!   dump SQL best-effort (schema essenziale + dati come INSERT), reimportabile.

use crate::model::*;
use crate::tools::{find_tool, has_tool, run};
use crate::{Error, Result};
use std::process::Command;

/// I tool nativi minimi (pg_dump + psql) sono presenti.
pub fn native_available() -> bool {
    has_tool("pg_dump") && has_tool("psql")
}

/// Il fallback puro Rust e' compilato in questa build.
pub fn rust_available() -> bool {
    cfg!(feature = "pg-driver")
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

/// Riepilogo per la UI.
pub fn report() -> EngineReport {
    let tools = vec![
        tool("pg_dump", "crea il dump del database"),
        tool("pg_restore", "ripristina dump in formato custom"),
        tool("psql", "esegue/importa script SQL"),
    ];
    let native = native_available();
    let rust = rust_available();
    let note = if native {
        "Tool nativi trovati: Charon usa pg_dump/psql (fedeltà massima).".into()
    } else if rust {
        "Tool nativi non trovati: verrà usato il fallback puro Rust (best-effort). \
         Installa il pacchetto 'postgresql-client' per la fedeltà massima."
            .into()
    } else {
        "Nessun metodo disponibile.".into()
    };
    let mut hints = Vec::new();
    if !native {
        hints.push(FixHint::new(
            "Installa il client PostgreSQL",
            "I tool nativi (pg_dump, pg_restore, psql) danno la fedeltà massima \
             (indici, vincoli, sequenze, permessi). Senza, Charon usa il fallback \
             puro Rust che esporta solo schema essenziale + dati. Installa il client:",
            Some(
                "Arch:           sudo pacman -S postgresql\n\
                 Debian/Ubuntu:  sudo apt install postgresql-client\n\
                 Fedora:         sudo dnf install postgresql\n\
                 macOS:          brew install libpq && brew link --force libpq\n\
                 Windows:        installa \"PostgreSQL\" da enterprisedb.com",
            ),
        ));
    }
    if !native && !rust {
        hints.push(FixHint::new(
            "Nessun metodo disponibile",
            "Mancano sia i tool nativi sia il fallback puro Rust. Ricompila Charon \
             con la feature 'pg-driver' (attiva di default) oppure installa il client.",
            None,
        ));
    }
    EngineReport {
        engine: Engine::Postgres,
        label: Engine::Postgres.label().into(),
        tools,
        native_available: native,
        rust_available: rust,
        note,
        hints,
    }
}

// ------------------------------------------------------------------ nativo ---

fn pg_dump_path() -> Result<std::path::PathBuf> {
    find_tool("pg_dump").ok_or_else(|| Error::ToolMissing("pg_dump".into()))
}
fn psql_path() -> Result<std::path::PathBuf> {
    find_tool("psql").ok_or_else(|| Error::ToolMissing("psql".into()))
}

pub fn native_dump(conn: &Connection, out: &str, log: &mut Vec<String>) -> Result<()> {
    let exe = pg_dump_path()?;
    let port = conn.port.to_string();
    let mut cmd = Command::new(&exe);
    cmd.env("PGPASSWORD", &conn.password)
        .arg("-h").arg(&conn.host)
        .arg("-p").arg(&port)
        .arg("-U").arg(&conn.user)
        .arg("-d").arg(&conn.database)
        .arg("--no-owner")
        .arg("--no-privileges")
        .arg("-f").arg(out);
    let display = format!(
        "pg_dump -h {} -p {} -U {} -d {} --no-owner --no-privileges -f {}",
        conn.host, port, conn.user, conn.database, out
    );
    if run(log, &display, &mut cmd)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("pg_dump ha segnalato un errore (vedi log)".into()))
    }
}

pub fn native_import(conn: &Connection, input: &str, log: &mut Vec<String>) -> Result<()> {
    let exe = psql_path()?;
    let port = conn.port.to_string();
    let mut cmd = Command::new(&exe);
    cmd.env("PGPASSWORD", &conn.password)
        .arg("-h").arg(&conn.host)
        .arg("-p").arg(&port)
        .arg("-U").arg(&conn.user)
        .arg("-d").arg(&conn.database)
        .arg("-v").arg("ON_ERROR_STOP=1")
        .arg("-f").arg(input);
    let display = format!(
        "psql -h {} -p {} -U {} -d {} -v ON_ERROR_STOP=1 -f {}",
        conn.host, port, conn.user, conn.database, input
    );
    if run(log, &display, &mut cmd)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("psql ha segnalato un errore (vedi log)".into()))
    }
}

pub fn native_clone(src: &Connection, dst: &Connection, log: &mut Vec<String>) -> Result<()> {
    let tmp = std::env::temp_dir().join(format!("charon-pg-{}.sql", std::process::id()));
    let tmp_s = tmp.display().to_string();
    log.push(format!("Dump temporaneo della sorgente in {tmp_s}"));
    native_dump(src, &tmp_s, log)?;
    log.push("Import sul database di destinazione".into());
    let res = native_import(dst, &tmp_s, log);
    let _ = std::fs::remove_file(&tmp);
    res
}

pub fn native_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    let exe = psql_path()?;
    let port = conn.port.to_string();
    let mut cmd = Command::new(&exe);
    cmd.env("PGPASSWORD", &conn.password)
        .arg("-h").arg(&conn.host)
        .arg("-p").arg(&port)
        .arg("-U").arg(&conn.user)
        .arg("-d").arg(&conn.database)
        .arg("-tAc").arg("SELECT version()");
    let display = format!(
        "psql -h {} -p {} -U {} -d {} -tAc 'SELECT version()'",
        conn.host, port, conn.user, conn.database
    );
    if run(log, &display, &mut cmd)?.success {
        Ok(())
    } else {
        Err(Error::Conn("connessione/psql falliti (vedi log)".into()))
    }
}

// --------------------------------------------------------------- puro Rust ---

pub fn rust_dump(conn: &Connection, out: &str, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "pg-driver")]
    {
        return rustimpl::dump(conn, out, log);
    }
    #[cfg(not(feature = "pg-driver"))]
    {
        let _ = (conn, out, log);
        Err(Error::Unsupported("fallback Postgres non disponibile in questa build".into()))
    }
}

pub fn rust_import(conn: &Connection, input: &str, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "pg-driver")]
    {
        return rustimpl::import(conn, input, log);
    }
    #[cfg(not(feature = "pg-driver"))]
    {
        let _ = (conn, input, log);
        Err(Error::Unsupported("fallback Postgres non disponibile in questa build".into()))
    }
}

pub fn rust_clone(src: &Connection, dst: &Connection, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "pg-driver")]
    {
        return rustimpl::clone(src, dst, log);
    }
    #[cfg(not(feature = "pg-driver"))]
    {
        let _ = (src, dst, log);
        Err(Error::Unsupported("fallback Postgres non disponibile in questa build".into()))
    }
}

pub fn rust_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "pg-driver")]
    {
        return rustimpl::test(conn, log);
    }
    #[cfg(not(feature = "pg-driver"))]
    {
        let _ = (conn, log);
        Err(Error::Unsupported("fallback Postgres non disponibile in questa build".into()))
    }
}

#[cfg(feature = "pg-driver")]
mod rustimpl {
    use super::*;
    use serde_json::Value;

    fn runtime() -> Result<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::Msg(format!("runtime tokio: {e}")))
    }

    async fn connect(conn: &Connection) -> Result<tokio_postgres::Client> {
        let mut cfg = tokio_postgres::Config::new();
        cfg.host(&conn.host)
            .port(conn.port)
            .user(&conn.user)
            .password(&conn.password)
            .dbname(&conn.database);
        let (client, connection) = cfg
            .connect(tokio_postgres::NoTls)
            .await
            .map_err(|e| Error::Conn(e.to_string()))?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok(client)
    }

    /// Mappa il `data_type` di information_schema in un tipo riscrivibile.
    fn map_type(dtype: &str, maxlen: Option<i32>) -> String {
        match dtype {
            "character varying" | "varchar" => maxlen
                .map(|n| format!("varchar({n})"))
                .unwrap_or_else(|| "text".into()),
            "character" | "bpchar" => maxlen
                .map(|n| format!("char({n})"))
                .unwrap_or_else(|| "char".into()),
            // tipi non riscrivibili tali e quali: ripieghiamo su text (best-effort).
            "ARRAY" | "USER-DEFINED" => "text".into(),
            other => other.to_string(),
        }
    }

    /// Converte un valore JSON in un letterale SQL.
    fn lit(v: &Value) -> String {
        match v {
            Value::Null => "NULL".into(),
            Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => format!("'{}'", s.replace('\'', "''")),
            other => format!("'{}'", other.to_string().replace('\'', "''")),
        }
    }

    async fn dump_sql(conn: &Connection) -> Result<String> {
        let client = connect(conn).await?;
        let qerr = |e: tokio_postgres::Error| Error::Msg(e.to_string());

        let tables = client
            .query(
                "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema='public' AND table_type='BASE TABLE' \
                 ORDER BY table_name",
                &[],
            )
            .await
            .map_err(qerr)?;

        let mut out = String::new();
        out.push_str("-- Dump generato da Charon — fallback puro Rust (best-effort).\n");
        out.push_str("-- Schema essenziale (colonne + NOT NULL) e dati come INSERT.\n");
        out.push_str("-- Per fedeltà completa (indici, vincoli, sequenze, tipi) usa pg_dump.\n\n");
        out.push_str("SET client_encoding = 'UTF8';\n\n");

        for trow in &tables {
            let table: String = trow.get(0);

            let cols = client
                .query(
                    "SELECT column_name, data_type, character_maximum_length, is_nullable \
                     FROM information_schema.columns \
                     WHERE table_schema='public' AND table_name=$1 ORDER BY ordinal_position",
                    &[&table],
                )
                .await
                .map_err(qerr)?;

            let col_names: Vec<String> = cols.iter().map(|c| c.get::<_, String>(0)).collect();

            out.push_str(&format!("DROP TABLE IF EXISTS \"{table}\" CASCADE;\n"));
            out.push_str(&format!("CREATE TABLE \"{table}\" (\n"));
            let mut defs = Vec::new();
            for c in &cols {
                let name: String = c.get(0);
                let dtype: String = c.get(1);
                let maxlen: Option<i32> = c.get(2);
                let nullable: String = c.get(3);
                let mut def = format!("  \"{}\" {}", name, map_type(&dtype, maxlen));
                if nullable == "NO" {
                    def.push_str(" NOT NULL");
                }
                defs.push(def);
            }
            out.push_str(&defs.join(",\n"));
            out.push_str("\n);\n");

            let collist = col_names
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(", ");
            let rows = client
                .query(format!("SELECT to_json(t) FROM \"{table}\" t").as_str(), &[])
                .await
                .map_err(qerr)?;
            for r in &rows {
                let row_json: Value = r.get(0);
                let vals = col_names
                    .iter()
                    .map(|n| lit(row_json.get(n).unwrap_or(&Value::Null)))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("INSERT INTO \"{table}\" ({collist}) VALUES ({vals});\n"));
            }
            out.push('\n');
        }
        Ok(out)
    }

    pub fn dump(conn: &Connection, out: &str, log: &mut Vec<String>) -> Result<()> {
        log.push("Connessione con tokio-postgres…".into());
        let sql = runtime()?.block_on(dump_sql(conn))?;
        std::fs::write(out, sql)?;
        log.push(format!("Dump SQL scritto in {out}"));
        Ok(())
    }

    pub fn import(conn: &Connection, input: &str, log: &mut Vec<String>) -> Result<()> {
        let sql = std::fs::read_to_string(input)?;
        log.push(format!("Esecuzione di {input} (batch_execute)…"));
        runtime()?.block_on(async {
            let client = connect(conn).await?;
            client
                .batch_execute(&sql)
                .await
                .map_err(|e| Error::Msg(e.to_string()))
        })?;
        log.push("Import completato.".into());
        Ok(())
    }

    pub fn clone(src: &Connection, dst: &Connection, log: &mut Vec<String>) -> Result<()> {
        let rt = runtime()?;
        log.push("Lettura schema+dati dalla sorgente…".into());
        let sql = rt.block_on(dump_sql(src))?;
        log.push("Scrittura sul database di destinazione…".into());
        rt.block_on(async {
            let client = connect(dst).await?;
            client
                .batch_execute(&sql)
                .await
                .map_err(|e| Error::Msg(e.to_string()))
        })?;
        log.push("Clonazione completata.".into());
        Ok(())
    }

    pub fn test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
        runtime()?.block_on(async {
            let client = connect(conn).await?;
            let row = client
                .query_one("SELECT version()", &[])
                .await
                .map_err(|e| Error::Msg(e.to_string()))?;
            let v: String = row.get(0);
            log.push(v);
            Ok::<(), Error>(())
        })
    }
}

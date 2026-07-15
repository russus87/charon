//! PostgreSQL: dump/import/clone.
//!
//! - **Nativo**: `pg_dump` (dump SQL), `psql` (import/esecuzione script).
//! - **Puro Rust** (`pg-driver`): si connette con `tokio-postgres` e genera un
//!   dump SQL best-effort (schema essenziale + dati come INSERT), reimportabile.

use crate::model::*;
use crate::tools::{find_tool, has_tool, plan_or_run, run};
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

pub fn native_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
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
    if plan_or_run(log, &display, &mut cmd, dry)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("pg_dump ha segnalato un errore (vedi log)".into()))
    }
}

pub fn native_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
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
    if plan_or_run(log, &display, &mut cmd, dry)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("psql ha segnalato un errore (vedi log)".into()))
    }
}

pub fn native_clone(
    src: &Connection,
    dst: &Connection,
    opts: &CloneOptions,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    if opts.has_mask() {
        return Err(Error::Unsupported(
            "il mascheramento non è disponibile con i tool nativi: usa il metodo puro Rust".into(),
        ));
    }
    if opts.data_only {
        return native_clone_data_only(src, dst, dry, log);
    }
    let tmp = std::env::temp_dir().join(format!("charon-pg-{}.sql", std::process::id()));
    let tmp_s = tmp.display().to_string();
    log.push(format!("Dump temporaneo della sorgente in {tmp_s}"));
    native_dump(src, &tmp_s, dry, log)?;
    log.push("Import sul database di destinazione".into());
    let res = native_import(dst, &tmp_s, dry, log);
    let _ = std::fs::remove_file(&tmp);
    res
}

/// Clone "solo dati" con i tool nativi: `pg_dump --data-only --disable-triggers`
/// della sorgente, poi sulla destinazione `TRUNCATE` + caricamento in un'unica
/// transazione (lo schema della destinazione resta intatto). Massima fedeltà.
fn native_clone_data_only(
    src: &Connection,
    dst: &Connection,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    let pgd = pg_dump_path()?;
    let psql = psql_path()?;
    if dry {
        // In anteprima non tocchiamo i DB: descriviamo la sequenza di passi.
        log.push(format!(
            "$ pg_dump -h {} -p {} -U {} -d {} --data-only --disable-triggers -f <tmp>",
            src.host, src.port, src.user, src.database
        ));
        log.push("  (dry-run: dump data-only non eseguito)".into());
        log.push("  Poi, sulla destinazione, in un'unica transazione:".into());
        log.push("    SET session_replication_role = replica;".into());
        log.push("    TRUNCATE TABLE <tabelle public> RESTART IDENTITY CASCADE;".into());
        log.push(format!(
            "    \\i <tmp>   (caricamento dati su {}:{}/{})",
            dst.host, dst.port, dst.database
        ));
        return Ok(());
    }
    let pid = std::process::id();
    let dump = std::env::temp_dir().join(format!("charon-pg-data-{pid}.sql"));
    let load = std::env::temp_dir().join(format!("charon-pg-load-{pid}.sql"));
    let cleanup = || {
        let _ = std::fs::remove_file(&dump);
        let _ = std::fs::remove_file(&load);
    };

    // 1) Dump dei soli dati della sorgente.
    let mut cmd = Command::new(&pgd);
    cmd.env("PGPASSWORD", &src.password)
        .arg("-h").arg(&src.host).arg("-p").arg(src.port.to_string())
        .arg("-U").arg(&src.user).arg("-d").arg(&src.database)
        .arg("--data-only").arg("--disable-triggers")
        .arg("-f").arg(&dump);
    let display = format!(
        "pg_dump -h {} -p {} -U {} -d {} --data-only --disable-triggers -f {}",
        src.host, src.port, src.user, src.database, dump.display()
    );
    if !run(log, &display, &mut cmd)?.success {
        cleanup();
        return Err(Error::Cmd("pg_dump (data-only) ha segnalato un errore".into()));
    }

    // 2) Elenco delle tabelle public della sorgente (quote_ident le rende sicure).
    let mut lc = Command::new(&psql);
    lc.env("PGPASSWORD", &src.password)
        .arg("-h").arg(&src.host).arg("-p").arg(src.port.to_string())
        .arg("-U").arg(&src.user).arg("-d").arg(&src.database)
        .arg("-tAc")
        .arg("SELECT string_agg(format('%I', tablename), ',') FROM pg_tables WHERE schemaname='public'");
    let listed = run(log, "psql (elenco tabelle public)", &mut lc)?;
    if !listed.success {
        cleanup();
        return Err(Error::Cmd("psql non è riuscito a elencare le tabelle".into()));
    }
    let tables = listed.stdout.trim().to_string();

    // 3) File di caricamento: disabilita i vincoli, TRUNCATE, poi i dati del dump.
    let dump_sql = std::fs::read_to_string(&dump)?;
    let mut load_sql = String::from("SET session_replication_role = replica;\n");
    if !tables.is_empty() {
        load_sql.push_str(&format!("TRUNCATE TABLE {tables} RESTART IDENTITY CASCADE;\n"));
    }
    load_sql.push_str(&dump_sql);
    std::fs::write(&load, &load_sql)?;

    // 4) Caricamento sulla destinazione in un'unica transazione (rollback se fallisce).
    let mut ic = Command::new(&psql);
    ic.env("PGPASSWORD", &dst.password)
        .arg("-h").arg(&dst.host).arg("-p").arg(dst.port.to_string())
        .arg("-U").arg(&dst.user).arg("-d").arg(&dst.database)
        .arg("--single-transaction").arg("-v").arg("ON_ERROR_STOP=1")
        .arg("-f").arg(&load);
    let display2 = format!(
        "psql -h {} -p {} -U {} -d {} --single-transaction -f {} (TRUNCATE + dati)",
        dst.host, dst.port, dst.user, dst.database, load.display()
    );
    let res = if run(log, &display2, &mut ic)?.success {
        Ok(())
    } else {
        Err(Error::Cmd("psql (caricamento data-only) ha segnalato un errore".into()))
    };
    cleanup();
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

pub fn rust_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "pg-driver")]
    {
        return rustimpl::dump(conn, out, dry, log);
    }
    #[cfg(not(feature = "pg-driver"))]
    {
        let _ = (conn, out, dry, log);
        Err(Error::Unsupported("fallback Postgres non disponibile in questa build".into()))
    }
}

pub fn rust_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "pg-driver")]
    {
        return rustimpl::import(conn, input, dry, log);
    }
    #[cfg(not(feature = "pg-driver"))]
    {
        let _ = (conn, input, dry, log);
        Err(Error::Unsupported("fallback Postgres non disponibile in questa build".into()))
    }
}

pub fn rust_clone(
    src: &Connection,
    dst: &Connection,
    opts: &CloneOptions,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    #[cfg(feature = "pg-driver")]
    {
        return rustimpl::clone(src, dst, opts, dry, log);
    }
    #[cfg(not(feature = "pg-driver"))]
    {
        let _ = (src, dst, opts, dry, log);
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
    use std::collections::HashMap;

    /// Mappa (tabella, colonna) → strategia di mascheramento.
    type MaskMap = HashMap<(String, String), MaskStrategy>;

    fn build_mask_map(opts: &CloneOptions) -> MaskMap {
        opts.mask
            .iter()
            .map(|r| ((r.table.clone(), r.column.clone()), r.strategy.clone()))
            .collect()
    }

    fn perr(e: tokio_postgres::Error) -> Error {
        Error::Msg(e.to_string())
    }

    /// Hash deterministico (non crittografico) → esadecimale a 16 cifre.
    fn hash_hex(s: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut h);
        format!("{:016x}", h.finish())
    }

    fn value_repr(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        }
    }

    /// Applica una strategia di mascheramento a un valore. I NULL restano NULL
    /// (non si fabbricano dati dove non ce n'erano).
    fn mask_value(strategy: &MaskStrategy, v: &Value) -> Value {
        if v.is_null() {
            return Value::Null;
        }
        match strategy {
            MaskStrategy::Null => Value::Null,
            MaskStrategy::Fixed { value } => Value::String(value.clone()),
            MaskStrategy::Hash => Value::String(hash_hex(&value_repr(v))),
            MaskStrategy::Email => Value::String(format!("user_{}@example.com", hash_hex(&value_repr(v)))),
            MaskStrategy::Redact => {
                let n = value_repr(v).chars().count().max(1);
                Value::String("*".repeat(n))
            }
        }
    }

    /// Restituisce il valore, mascherato se c'è una regola per (tabella, colonna).
    fn mask_col(mask: &MaskMap, table: &str, col: &str, v: &Value) -> Value {
        match mask.get(&(table.to_string(), col.to_string())) {
            Some(strategy) => mask_value(strategy, v),
            None => v.clone(),
        }
    }

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

    async fn dump_sql(conn: &Connection, mask: &MaskMap) -> Result<String> {
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
            crate::progress::emit(&format!("  tabella {table}…"));

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
                    .map(|n| {
                        let v = row_json.get(n).cloned().unwrap_or(Value::Null);
                        lit(&mask_col(mask, &table, n, &v))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("INSERT INTO \"{table}\" ({collist}) VALUES ({vals});\n"));
            }
            out.push('\n');
        }
        Ok(out)
    }

    pub fn dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
        log.push("Connessione con tokio-postgres…".into());
        let sql = runtime()?.block_on(dump_sql(conn, &MaskMap::new()))?;
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
            let creates = sql.matches("CREATE TABLE ").count();
            log.push(format!(
                "Dry-run: {input} verrebbe eseguito ({creates} CREATE, {inserts} INSERT, {} righe). Nessuna modifica applicata.",
                sql.lines().count()
            ));
            return Ok(());
        }
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

    pub fn clone(
        src: &Connection,
        dst: &Connection,
        opts: &CloneOptions,
        dry: bool,
        log: &mut Vec<String>,
    ) -> Result<()> {
        let rt = runtime()?;
        let mask = build_mask_map(opts);
        if !mask.is_empty() {
            log.push(format!("Mascheramento attivo su {} colonna/e.", mask.len()));
        }
        if opts.data_only {
            log.push("Modalità data-only: preservo lo schema della destinazione.".into());
            if dry {
                return rt.block_on(plan_data_only(src, log));
            }
            rt.block_on(clone_data_only(src, dst, &mask, log))?;
        } else {
            log.push("Lettura schema+dati dalla sorgente…".into());
            let sql = rt.block_on(dump_sql(src, &mask))?;
            let tables = sql.matches("CREATE TABLE ").count();
            let rows = sql.matches("INSERT INTO ").count();
            if dry {
                log.push(format!(
                    "Dry-run: verrebbero ricreate {tables} tabelle e inserite {rows} righe su \
                     {}:{}/{} (DROP + CREATE + dati). Destinazione non modificata.",
                    dst.host, dst.port, dst.database
                ));
                return Ok(());
            }
            log.push("Scrittura sul database di destinazione (DROP + CREATE + dati)…".into());
            rt.block_on(async {
                let client = connect(dst).await?;
                client.batch_execute(&sql).await.map_err(perr)
            })?;
        }
        log.push("Clonazione completata.".into());
        Ok(())
    }

    /// Anteprima (dry-run) del clone data-only: elenca tabelle e conteggi della
    /// sorgente senza toccare la destinazione.
    async fn plan_data_only(src: &Connection, log: &mut Vec<String>) -> Result<()> {
        let s = connect(src).await?;
        let trows = s
            .query(
                "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema='public' AND table_type='BASE TABLE' ORDER BY table_name",
                &[],
            )
            .await
            .map_err(perr)?;
        let tables: Vec<String> = trows.iter().map(|r| r.get::<_, String>(0)).collect();
        log.push(format!(
            "Dry-run: verrebbe eseguito TRUNCATE ... RESTART IDENTITY CASCADE su {} tabelle, poi il travaso dati:",
            tables.len()
        ));
        for t in &tables {
            let n = s
                .query_one(&format!("SELECT count(*) FROM \"{t}\""), &[])
                .await
                .map(|r| r.get::<_, i64>(0))
                .unwrap_or(0);
            crate::progress::note(log, format!("  {t}: {n} righe da copiare"));
        }
        log.push("Destinazione non modificata (anteprima).".into());
        Ok(())
    }

    /// Clone "solo dati": preserva lo schema della destinazione (gestito magari
    /// da migration). Disabilita i vincoli, TRUNCATE le tabelle, reinserisce i
    /// dati (mascherati se richiesto) e riallinea le sequenze. Tutto in una
    /// transazione: se qualcosa fallisce, la destinazione resta invariata.
    async fn clone_data_only(
        src: &Connection,
        dst: &Connection,
        mask: &MaskMap,
        log: &mut Vec<String>,
    ) -> Result<()> {
        let s = connect(src).await?;
        let mut d = connect(dst).await?;

        let trows = s
            .query(
                "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema='public' AND table_type='BASE TABLE' ORDER BY table_name",
                &[],
            )
            .await
            .map_err(perr)?;
        let tables: Vec<String> = trows.iter().map(|r| r.get::<_, String>(0)).collect();
        if tables.is_empty() {
            return Err(Error::Msg("nessuna tabella 'public' nella sorgente".into()));
        }

        let tx = d.transaction().await.map_err(|e| Error::Conn(e.to_string()))?;
        // Disabilita i trigger/vincoli FK per il travaso (richiede superuser).
        // Best-effort: se non abbiamo il permesso proseguiamo comunque.
        let _ = tx.batch_execute("SET session_replication_role = replica").await;

        let tlist = tables
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(", ");
        tx.batch_execute(&format!("TRUNCATE TABLE {tlist} RESTART IDENTITY CASCADE"))
            .await
            .map_err(|e| Error::Msg(format!("TRUNCATE: {e}")))?;

        let mut total = 0usize;
        for table in &tables {
            let cols = s
                .query(
                    "SELECT column_name FROM information_schema.columns \
                     WHERE table_schema='public' AND table_name=$1 ORDER BY ordinal_position",
                    &[table],
                )
                .await
                .map_err(perr)?;
            let col_names: Vec<String> = cols.iter().map(|c| c.get::<_, String>(0)).collect();
            if col_names.is_empty() {
                continue;
            }
            let collist = col_names
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(", ");
            let rows = s
                .query(format!("SELECT to_json(t) FROM \"{table}\" t").as_str(), &[])
                .await
                .map_err(perr)?;
            if rows.is_empty() {
                continue;
            }
            let mut sql = String::new();
            for r in &rows {
                let row_json: Value = r.get(0);
                let vals = col_names
                    .iter()
                    .map(|n| {
                        let v = row_json.get(n).cloned().unwrap_or(Value::Null);
                        lit(&mask_col(mask, table, n, &v))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                sql.push_str(&format!("INSERT INTO \"{table}\" ({collist}) VALUES ({vals});\n"));
            }
            tx.batch_execute(&sql)
                .await
                .map_err(|e| Error::Msg(format!("INSERT in {table}: {e}")))?;
            total += rows.len();
            crate::progress::note(log, format!("  {table}: {} righe", rows.len()));
        }

        // Riallinea le sequenze identity/serial al massimo valore inserito.
        let seqrows = tx
            .query(
                "SELECT table_name, column_name, pg_get_serial_sequence(quote_ident(table_name), column_name) \
                 FROM information_schema.columns \
                 WHERE table_schema='public' AND table_name = ANY($1)",
                &[&tables],
            )
            .await
            .map_err(perr)?;
        for r in &seqrows {
            let seq: Option<String> = r.get(2);
            if let Some(seq) = seq {
                let table: String = r.get(0);
                let col: String = r.get(1);
                let q = format!(
                    "SELECT setval('{seq}', (SELECT COALESCE(MAX(\"{col}\"), 1) FROM \"{table}\"))"
                );
                tx.batch_execute(&q)
                    .await
                    .map_err(|e| Error::Msg(format!("setval {seq}: {e}")))?;
            }
        }

        tx.commit().await.map_err(|e| Error::Msg(format!("commit: {e}")))?;
        log.push(format!("Totale righe copiate: {total}"));
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

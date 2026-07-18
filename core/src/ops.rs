//! Orchestratore: sceglie il metodo (nativo/puro Rust) e instrada l'operazione
//! verso il motore giusto. E' l'unico punto che la UI deve conoscere.

use crate::model::*;
use crate::{mssql, mysql, oracle, postgres, sqlite, tunnel, Error, Result};

/// Riepilogo di cosa e' disponibile sulla macchina, per tutti i motori.
pub fn detect_all() -> Vec<EngineReport> {
    vec![
        postgres::report(),
        oracle::report(),
        mssql::report(),
        sqlite::report(),
        mysql::report(),
    ]
}

/// (nativo_disponibile, rust_disponibile) per un motore.
fn availability(engine: Engine) -> (bool, bool) {
    match engine {
        Engine::Postgres => (postgres::native_available(), postgres::rust_available()),
        Engine::Sqlserver => (mssql::native_available(), mssql::rust_available()),
        Engine::Oracle => (oracle::native_available(), oracle::rust_available()),
        Engine::Sqlite => (sqlite::native_available(), sqlite::rust_available()),
        Engine::Mysql => (mysql::native_available(), mysql::rust_available()),
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
    crate::progress::note(&mut log, format!("Metodo selezionato: {}", method.label()));
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

fn dispatch_dump(
    conn: &Connection,
    out: &str,
    m: Method,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    match (conn.engine, m) {
        (Engine::Postgres, Method::Native) => postgres::native_dump(conn, out, dry, log),
        (Engine::Postgres, Method::Rust) => postgres::rust_dump(conn, out, dry, log),
        (Engine::Sqlserver, Method::Native) => mssql::native_dump(conn, out, dry, log),
        (Engine::Sqlserver, Method::Rust) => mssql::rust_dump(conn, out, dry, log),
        (Engine::Oracle, Method::Native) => oracle::native_dump(conn, out, dry, log),
        (Engine::Oracle, Method::Rust) => oracle::rust_dump(conn, out, dry, log),
        (Engine::Sqlite, Method::Native) => sqlite::native_dump(conn, out, dry, log),
        (Engine::Sqlite, Method::Rust) => sqlite::rust_dump(conn, out, dry, log),
        (Engine::Mysql, Method::Native) => mysql::native_dump(conn, out, dry, log),
        (Engine::Mysql, Method::Rust) => mysql::rust_dump(conn, out, dry, log),
    }
}

fn dispatch_import(
    conn: &Connection,
    input: &str,
    m: Method,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    match (conn.engine, m) {
        (Engine::Postgres, Method::Native) => postgres::native_import(conn, input, dry, log),
        (Engine::Postgres, Method::Rust) => postgres::rust_import(conn, input, dry, log),
        (Engine::Sqlserver, Method::Native) => mssql::native_import(conn, input, dry, log),
        (Engine::Sqlserver, Method::Rust) => mssql::rust_import(conn, input, dry, log),
        (Engine::Oracle, Method::Native) => oracle::native_import(conn, input, dry, log),
        (Engine::Oracle, Method::Rust) => oracle::rust_import(conn, input, dry, log),
        (Engine::Sqlite, Method::Native) => sqlite::native_import(conn, input, dry, log),
        (Engine::Sqlite, Method::Rust) => sqlite::rust_import(conn, input, dry, log),
        (Engine::Mysql, Method::Native) => mysql::native_import(conn, input, dry, log),
        (Engine::Mysql, Method::Rust) => mysql::rust_import(conn, input, dry, log),
    }
}

fn dispatch_clone(
    src: &Connection,
    dst: &Connection,
    m: Method,
    opts: &CloneOptions,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    match (src.engine, m) {
        (Engine::Postgres, Method::Native) => postgres::native_clone(src, dst, opts, dry, log),
        (Engine::Postgres, Method::Rust) => postgres::rust_clone(src, dst, opts, dry, log),
        (Engine::Sqlserver, Method::Native) => mssql::native_clone(src, dst, opts, dry, log),
        (Engine::Sqlserver, Method::Rust) => mssql::rust_clone(src, dst, opts, dry, log),
        (Engine::Oracle, Method::Native) => oracle::native_clone(src, dst, opts, dry, log),
        (Engine::Oracle, Method::Rust) => oracle::rust_clone(src, dst, opts, dry, log),
        (Engine::Sqlite, Method::Native) => sqlite::native_clone(src, dst, opts, dry, log),
        (Engine::Sqlite, Method::Rust) => sqlite::rust_clone(src, dst, opts, dry, log),
        (Engine::Mysql, Method::Native) => mysql::native_clone(src, dst, opts, dry, log),
        (Engine::Mysql, Method::Rust) => mysql::rust_clone(src, dst, opts, dry, log),
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
        (Engine::Sqlite, Method::Native) => sqlite::native_test(conn, log),
        (Engine::Sqlite, Method::Rust) => sqlite::rust_test(conn, log),
        (Engine::Mysql, Method::Native) => mysql::native_test(conn, log),
        (Engine::Mysql, Method::Rust) => mysql::rust_test(conn, log),
    }
}

/// Se la connessione usa un tunnel SSH, lo apre e restituisce una connessione
/// "effettiva" che punta al forward locale (più la guardia da tenere viva).
/// Senza SSH, ritorna la connessione invariata.
fn prepare(conn: &Connection) -> Result<(Connection, Option<tunnel::TunnelGuard>)> {
    if conn.ssh.is_none() {
        return Ok((conn.clone(), None));
    }
    // Un motore su file è locale: inoltrare una porta TCP non lo raggiungerebbe.
    if conn.engine.is_file_based() {
        return Err(Error::Unsupported(format!(
            "{} è un file locale: il tunnel SSH non si applica",
            conn.engine.label()
        )));
    }
    #[cfg(feature = "ssh-tunnel")]
    {
        let guard = tunnel::open(conn)?;
        let mut eff = conn.clone();
        eff.host = guard.local_host.clone();
        eff.port = guard.local_port;
        eff.ssh = None;
        Ok((eff, Some(guard)))
    }
    #[cfg(not(feature = "ssh-tunnel"))]
    {
        Err(Error::Unsupported(
            "supporto SSH non compilato in questa build".into(),
        ))
    }
}

/// OpResult di errore "precoce" (es. tunnel non apribile), prima di scegliere il metodo.
fn early_error(e: Error) -> OpResult {
    OpResult {
        ok: false,
        method: Method::Native,
        message: e.to_string(),
        artifact: None,
        log: Vec::new(),
    }
}

/// Prefissa il log con la modalità dry-run, quando attiva.
fn note_dry(dry: bool, log: &mut Vec<String>) {
    if dry {
        crate::progress::note(log, "── DRY-RUN: anteprima, nessuna modifica verrà applicata ──");
    }
}

/// Crea un dump del database in `out`. Con `dry`, mostra solo cosa verrebbe fatto.
pub fn dump(conn: &Connection, out: &str, prefer: Prefer, dry: bool) -> OpResult {
    let (conn, _guard) = match prepare(conn) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    let msg = if dry {
        format!("Dry-run dump → {out} (nessun file scritto)")
    } else {
        format!("Dump completato → {out}")
    };
    let res = finalize(conn.engine, prefer, msg, |m, log| {
        note_dry(dry, log);
        dispatch_dump(&conn, out, m, dry, log)
    });
    if res.ok && !dry {
        res.with_artifact(out)
    } else {
        res
    }
}

/// Importa un dump `input` nel database. Con `dry`, mostra solo cosa verrebbe fatto.
pub fn import(conn: &Connection, input: &str, prefer: Prefer, dry: bool) -> OpResult {
    let (conn, _guard) = match prepare(conn) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    let msg = if dry {
        "Dry-run import (nessuna modifica applicata)".into()
    } else {
        "Import completato".into()
    };
    finalize(conn.engine, prefer, msg, |m, log| {
        note_dry(dry, log);
        dispatch_import(&conn, input, m, dry, log)
    })
}

/// Clona il database `src` su `dst` (devono essere dello stesso motore).
/// Con `dry`, ispeziona la sorgente e mostra il piano senza toccare la destinazione.
pub fn clone(
    src: &Connection,
    dst: &Connection,
    prefer: Prefer,
    opts: &CloneOptions,
    dry: bool,
) -> OpResult {
    // Apre i tunnel SSH (se configurati) per sorgente e destinazione.
    let (src, _gs) = match prepare(src) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    let (dst, _gd) = match prepare(dst) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    // Motori diversi → migrazione cross-motore (best-effort, puro Rust).
    if src.engine != dst.engine {
        if opts.has_mask() {
            return early_error(Error::Unsupported(
                "il mascheramento non è disponibile nel clone cross-motore".into(),
            ));
        }
        if opts.data_only {
            return cross_data_only(&src, &dst, dry);
        }
        return cross_clone(&src, &dst, dry);
    }
    // Il mascheramento riscrive i valori riga per riga: possibile solo col puro
    // Rust. Anche il data-only per SQL Server è implementato solo nel fallback puro
    // Rust (i tool nativi non lo gestiscono). In questi casi forziamo quel metodo a
    // prescindere dalla preferenza.
    let force_rust = opts.has_mask() || (src.engine == Engine::Sqlserver && opts.data_only);
    let effective = if force_rust { Prefer::Rust } else { prefer };
    let msg = if dry {
        "Dry-run clonazione (destinazione non modificata)".into()
    } else {
        "Clonazione completata".into()
    };
    finalize(src.engine, effective, msg, |m, log| {
        note_dry(dry, log);
        if opts.has_mask() && m != Method::Rust {
            return Err(Error::Unsupported(
                "il mascheramento richiede il metodo puro Rust".into(),
            ));
        }
        dispatch_clone(&src, &dst, m, opts, dry, log)
    })
}

/// Importa un **pacchetto SQL\*Loader** (cartella con file `.ctl`/`.ldr`, come
/// esportato da SQL Developer in formato Loader) invocando `sqlldr` per ogni
/// tabella nell'ordine dato. È il workflow no-admin descritto nella
/// documentazione Oracle: gestisce anche i BLOB (via LOBFILE) che gli `INSERT`
/// non possono trasferire. Con `dry`, stampa i comandi `sqlldr` senza eseguirli.
///
/// Al termine (se non in dry-run e col driver Oracle disponibile) riallinea le
/// sequenze identity, passaggio obbligatorio dopo un load diretto.
pub fn oracle_load(conn: &Connection, package_dir: &str, dry: bool) -> OpResult {
    if conn.engine != Engine::Oracle {
        return OpResult {
            ok: false,
            method: Method::Native,
            message: "oracle-load è specifico del motore Oracle".into(),
            artifact: None,
            log: Vec::new(),
        };
    }
    let (conn, _guard) = match prepare(conn) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    let mut log = Vec::new();
    note_dry(dry, &mut log);
    let msg: String = if dry {
        "Dry-run SQL*Loader (nessun caricamento eseguito)".into()
    } else {
        "Caricamento SQL*Loader completato".into()
    };
    match oracle::sqlldr_import(&conn, package_dir, dry, &mut log) {
        Ok(()) => OpResult::ok(Method::Native, msg, log),
        Err(e) => OpResult {
            ok: false,
            method: Method::Native,
            message: e.to_string(),
            artifact: None,
            log,
        },
    }
}

/// Configura l'**Oracle Instant Client** partendo da uno `.zip` ufficiale (o da
/// una cartella già estratta): Charon lo scompatta in una cartella dell'utente
/// (nessun admin), verifica che contenga le librerie per il sistema corrente e
/// ricorda il percorso. Da lì in poi le operazioni Oracle puro-Rust lo usano.
pub fn oracle_setup(path: &str) -> OpResult {
    // `mut` serve solo nel ramo con la feature Oracle (provision scrive nel log).
    #[allow(unused_mut)]
    let mut log = Vec::new();
    #[cfg(feature = "oracle-driver")]
    {
        match oracle::provision(path, &mut log) {
            Ok(dir) => OpResult::ok(
                Method::Rust,
                format!("Instant Client configurato: {}", dir.display()),
                log,
            )
            .with_artifact(dir.display().to_string()),
            Err(e) => OpResult {
                ok: false,
                method: Method::Rust,
                message: e.to_string(),
                artifact: None,
                log,
            },
        }
    }
    #[cfg(not(feature = "oracle-driver"))]
    {
        let _ = path;
        OpResult {
            ok: false,
            method: Method::Rust,
            message: "questa build non include il driver Oracle: usa una release ufficiale \
                      (o compila con --features oracle)"
                .into(),
            artifact: None,
            log,
        }
    }
}

/// Confronta due database dello stesso motore e restituisce il diff (schema +
/// conteggio righe). **Sola lettura**: non modifica nessuno dei due lati.
///
/// Implementato per tutti i motori (PostgreSQL, Oracle, SQL Server, SQLite,
/// MySQL/MariaDB) via il fallback puro Rust: legge i cataloghi, i tool nativi
/// non c'entrano.
pub fn compare(src: &Connection, dst: &Connection) -> Result<crate::compare::DbDiff> {
    let (src, _gs) = prepare(src)?;
    let (dst, _gd) = prepare(dst)?;
    if src.engine == dst.engine {
        // Stesso motore: confronto pieno (schema + conteggio righe).
        match src.engine {
            Engine::Postgres => postgres::rust_compare(&src, &dst),
            Engine::Oracle => oracle::rust_compare(&src, &dst),
            Engine::Sqlserver => mssql::rust_compare(&src, &dst),
            Engine::Sqlite => sqlite::rust_compare(&src, &dst),
            Engine::Mysql => mysql::rust_compare(&src, &dst),
        }
    } else {
        // Cross-motore: leggi i due schemi neutri e confrontali sui tipi
        // normalizzati (senza conteggio righe).
        let s = read_schema(&src)?;
        let d = read_schema(&dst)?;
        Ok(crate::schema::diff_schemas(&s, &d, conn_label(&src), conn_label(&dst)))
    }
}

/// Letterale SQL di un valore (stringa grezza dal JSON di export) per il motore
/// **target**, in base al tipo astratto della colonna. Cuore della conversione
/// dati nella migrazione cross-motore.
fn cross_literal(v: Option<&str>, ty: &crate::schema::AbstractType, target: Engine) -> String {
    use crate::schema::AbstractType::*;
    let s = match v {
        None => return "NULL".into(),
        Some(s) => s,
    };
    match ty {
        Boolean => {
            let truthy = matches!(s.to_ascii_lowercase().as_str(), "true" | "t" | "1" | "yes" | "y");
            match target {
                Engine::Postgres => if truthy { "true" } else { "false" }.into(),
                _ => if truthy { "1" } else { "0" }.into(),
            }
        }
        Integer { .. } | Decimal { .. } | Float { .. } => {
            if s.trim().is_empty() { "NULL".into() } else { s.to_string() }
        }
        // Testo/data/uuid/json/binario: come stringa quotata (best-effort).
        _ => format!("'{}'", s.replace('\'', "''")),
    }
}

/// `DROP TABLE` idempotente nel dialetto target (dove supportato).
fn drop_if_exists(engine: Engine, table: &str) -> String {
    let q = crate::schema::quote_ident(engine, table);
    match engine {
        // Oracle < 23c non ha IF EXISTS: si conta sul continue-on-error dell'import.
        Engine::Oracle => String::new(),
        _ => format!("DROP TABLE IF EXISTS {q};\n"),
    }
}

/// **Clone cross-motore** (migrazione best-effort): legge lo schema della
/// sorgente nel modello neutro, esporta i dati in JSON temporaneo, genera
/// CREATE TABLE + INSERT nel dialetto della destinazione e li esegue lì.
/// Riusa read_schema + export + import: nessun percorso dati nuovo per motore.
pub fn cross_clone(src: &Connection, dst: &Connection, dry: bool) -> OpResult {
    let mut log = Vec::new();
    note_dry(dry, &mut log);
    crate::progress::note(
        &mut log,
        format!(
            "Migrazione cross-motore {} → {} (best-effort)",
            src.engine.label(),
            dst.engine.label()
        ),
    );

    let model = match read_schema(src) {
        Ok(m) => m,
        Err(e) => return early_error(e),
    };
    let tmpdir = std::env::temp_dir().join(format!("charon-xclone-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmpdir);
    let files = match export_data(src, &tmpdir.display().to_string(), "json") {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmpdir);
            return early_error(e);
        }
    };
    let file_of: std::collections::HashMap<String, String> = files
        .iter()
        .filter_map(|f| {
            std::path::Path::new(f)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|stem| (stem.to_string(), f.clone()))
        })
        .collect();

    let mut script = String::new();
    let mut nrows = 0usize;
    for t in &model.tables {
        script.push_str(&drop_if_exists(dst.engine, &t.name));
        script.push_str(&crate::schema::create_table_ddl(t, dst.engine));
        script.push('\n');
        if let Some(path) = file_of.get(&t.name) {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(serde_json::Value::Array(rows)) =
                    serde_json::from_str::<serde_json::Value>(&text)
                {
                    let collist = t
                        .columns
                        .iter()
                        .map(|c| crate::schema::quote_ident(dst.engine, &c.name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let tname = crate::schema::quote_ident(dst.engine, &t.name);
                    for row in &rows {
                        let vals = t
                            .columns
                            .iter()
                            .map(|c| {
                                let v = row
                                    .get(&c.name)
                                    .and_then(|x| if x.is_null() { None } else { x.as_str() });
                                cross_literal(v, &c.ty, dst.engine)
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        script.push_str(&format!("INSERT INTO {tname} ({collist}) VALUES ({vals});\n"));
                        nrows += 1;
                    }
                }
            }
        }
        // Indici della tabella, dopo i dati.
        for idx in &t.indexes {
            script.push_str(&crate::schema::index_ddl(&t.name, idx, dst.engine));
            script.push('\n');
        }
        script.push('\n');
    }
    // Foreign key in coda: tutte le tabelle esistono e sono popolate, quindi la
    // validazione referenziale non fallisce per ordine di creazione.
    let mut nfk = 0usize;
    for t in &model.tables {
        for fk in &t.foreign_keys {
            script.push_str(&crate::schema::fk_ddl(&t.name, fk, dst.engine));
            script.push('\n');
            nfk += 1;
        }
    }
    crate::progress::note(
        &mut log,
        format!(
            "{} tabelle, {} righe, {} FK da migrare",
            model.tables.len(),
            nrows,
            nfk
        ),
    );

    if dry {
        let _ = std::fs::remove_dir_all(&tmpdir);
        crate::progress::note(&mut log, "Dry-run: destinazione non modificata.");
        return OpResult::ok(Method::Rust, "Dry-run migrazione cross-motore", log);
    }

    let tmp = tmpdir.join("migrate.sql");
    if let Err(e) = std::fs::write(&tmp, &script) {
        let _ = std::fs::remove_dir_all(&tmpdir);
        return early_error(Error::Io(e));
    }
    let mut res = import(dst, &tmp.display().to_string(), Prefer::Rust, false);
    let _ = std::fs::remove_dir_all(&tmpdir);
    let mut full = log;
    full.append(&mut res.log);
    OpResult {
        message: if res.ok {
            format!("Migrazione cross-motore completata ({nrows} righe)")
        } else {
            res.message
        },
        log: full,
        ..res
    }
}

/// **Data-only cross-motore** (append): non tocca lo schema della destinazione
/// (che si assume già esistente, es. gestito da migration). Per ogni tabella
/// presente da entrambe le parti, trasferisce i dati delle sole colonne comuni
/// (per nome), convertendo i valori per il target. Non svuota la destinazione.
pub fn cross_data_only(src: &Connection, dst: &Connection, dry: bool) -> OpResult {
    let mut log = Vec::new();
    note_dry(dry, &mut log);
    crate::progress::note(
        &mut log,
        format!(
            "Solo-dati cross-motore {} → {} (append, colonne comuni)",
            src.engine.label(),
            dst.engine.label()
        ),
    );
    let smodel = match read_schema(src) {
        Ok(m) => m,
        Err(e) => return early_error(e),
    };
    let dmodel = match read_schema(dst) {
        Ok(m) => m,
        Err(e) => return early_error(e),
    };
    let tmpdir = std::env::temp_dir().join(format!("charon-xdata-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmpdir);
    let files = match export_data(src, &tmpdir.display().to_string(), "json") {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmpdir);
            return early_error(e);
        }
    };
    let file_of: std::collections::HashMap<String, String> = files
        .iter()
        .filter_map(|f| {
            std::path::Path::new(f)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|stem| (stem.to_string(), f.clone()))
        })
        .collect();

    let mut script = String::new();
    let mut nrows = 0usize;
    let mut skipped = 0usize;
    for st in &smodel.tables {
        let Some(dt) = dmodel.table(&st.name) else {
            skipped += 1;
            continue; // tabella assente nella destinazione
        };
        // Colonne del target che esistono anche nella sorgente (abbinamento per nome).
        let cols: Vec<&crate::schema::Column> = dt
            .columns
            .iter()
            .filter(|c| st.columns.iter().any(|sc| sc.name == c.name))
            .collect();
        if cols.is_empty() {
            continue;
        }
        let collist = cols
            .iter()
            .map(|c| crate::schema::quote_ident(dst.engine, &c.name))
            .collect::<Vec<_>>()
            .join(", ");
        let tname = crate::schema::quote_ident(dst.engine, &st.name);
        if let Some(path) = file_of.get(&st.name) {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(serde_json::Value::Array(rows)) =
                    serde_json::from_str::<serde_json::Value>(&text)
                {
                    for row in &rows {
                        let vals = cols
                            .iter()
                            .map(|c| {
                                let v = row
                                    .get(&c.name)
                                    .and_then(|x| if x.is_null() { None } else { x.as_str() });
                                cross_literal(v, &c.ty, dst.engine)
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        script.push_str(&format!("INSERT INTO {tname} ({collist}) VALUES ({vals});\n"));
                        nrows += 1;
                    }
                }
            }
        }
    }
    crate::progress::note(
        &mut log,
        format!("{nrows} righe da inserire; {skipped} tabelle saltate (assenti nel target)"),
    );
    if dry {
        let _ = std::fs::remove_dir_all(&tmpdir);
        return OpResult::ok(Method::Rust, "Dry-run solo-dati cross-motore", log);
    }
    let tmp = tmpdir.join("data.sql");
    if let Err(e) = std::fs::write(&tmp, &script) {
        let _ = std::fs::remove_dir_all(&tmpdir);
        return early_error(Error::Io(e));
    }
    let mut res = import(dst, &tmp.display().to_string(), Prefer::Rust, false);
    let _ = std::fs::remove_dir_all(&tmpdir);
    let mut full = log;
    full.append(&mut res.log);
    OpResult {
        message: if res.ok {
            format!("Solo-dati cross-motore completato ({nrows} righe)")
        } else {
            res.message
        },
        log: full,
        ..res
    }
}

/// Legge lo schema neutro di un database (dispatch per motore).
fn read_schema(conn: &Connection) -> Result<crate::schema::SchemaModel> {
    match conn.engine {
        Engine::Postgres => postgres::rust_read_schema(conn),
        Engine::Oracle => oracle::rust_read_schema(conn),
        Engine::Sqlserver => mssql::rust_read_schema(conn),
        Engine::Sqlite => sqlite::rust_read_schema(conn),
        Engine::Mysql => mysql::rust_read_schema(conn),
    }
}

/// Etichetta leggibile di una connessione (col motore, utile nel diff cross-motore).
fn conn_label(conn: &Connection) -> String {
    if conn.engine.is_file_based() {
        format!("{} · {}", conn.engine.label(), conn.database)
    } else {
        format!("{} · {}:{}/{}", conn.engine.label(), conn.host, conn.port, conn.database)
    }
}

/// Confronto **dati** (riga per riga, per chiave primaria) di una singola
/// tabella fra due database dello stesso motore. Sola lettura. Più pesante del
/// confronto di schema: si esegue su richiesta per la tabella scelta.
pub fn compare_data(
    src: &Connection,
    dst: &Connection,
    table: &str,
) -> Result<crate::compare::TableDataDiff> {
    if src.engine != dst.engine {
        return Err(Error::Unsupported(
            "il confronto dati richiede due database dello stesso motore".into(),
        ));
    }
    let (src, _gs) = prepare(src)?;
    let (dst, _gd) = prepare(dst)?;
    match src.engine {
        Engine::Postgres => postgres::rust_data_diff(&src, &dst, table),
        Engine::Oracle => oracle::rust_data_diff(&src, &dst, table),
        Engine::Sqlserver => mssql::rust_data_diff(&src, &dst, table),
        Engine::Sqlite => sqlite::rust_data_diff(&src, &dst, table),
        Engine::Mysql => mysql::rust_data_diff(&src, &dst, table),
    }
}

/// Genera lo **script di allineamento** (DDL) che porterebbe la destinazione a
/// somigliare alla sorgente, dal diff di schema. **Sola lettura**: non modifica
/// niente, serve per l'anteprima da rivedere prima di applicare.
pub fn sync_plan(src: &Connection, dst: &Connection) -> Result<String> {
    if src.engine != dst.engine {
        return Err(Error::Unsupported(
            "l'allineamento richiede due database dello stesso motore".into(),
        ));
    }
    let diff = compare(src, dst)?;
    Ok(crate::sync::sync_script(&diff, src.engine))
}

/// Genera lo script di allineamento e lo **applica** alla destinazione (o, con
/// `dry`, mostra soltanto cosa verrebbe eseguito). L'esecuzione passa dal
/// percorso import puro-Rust, statement per statement.
pub fn sync_apply(src: &Connection, dst: &Connection, dry: bool) -> OpResult {
    let script = match sync_plan(src, dst) {
        Ok(s) => s,
        Err(e) => return early_error(e),
    };
    // Niente DDL effettive (solo commenti/intestazione): non c'è nulla da fare.
    let has_ddl = script
        .lines()
        .any(|l| !l.trim_start().starts_with("--") && !l.trim().is_empty());
    if !has_ddl {
        return OpResult::ok(
            Method::Rust,
            "Nessuna differenza di schema da applicare.",
            vec!["Le due schemi risultano già allineati.".into()],
        );
    }
    let tmp = std::env::temp_dir().join(format!("charon-sync-{}.sql", std::process::id()));
    if let Err(e) = std::fs::write(&tmp, &script) {
        return early_error(Error::Io(e));
    }
    let msg = if dry {
        "Dry-run allineamento (destinazione non modificata)".into()
    } else {
        "Allineamento applicato alla destinazione".into()
    };
    // Forziamo il puro Rust: lo script è pensato per l'esecuzione statement-based.
    let res = import(dst, &tmp.display().to_string(), Prefer::Rust, dry);
    let _ = std::fs::remove_file(&tmp);
    // Sostituiamo il messaggio generico dell'import con uno specifico dell'allineamento.
    OpResult { message: if res.ok { msg } else { res.message }, ..res }
}

/// Esporta i **dati** di tutte le tabelle in `out_dir`, un file per tabella nel
/// formato scelto ("csv" o "json"). Sola lettura. Ritorna i file scritti.
pub fn export_data(conn: &Connection, out_dir: &str, format: &str) -> Result<Vec<String>> {
    let (conn, _guard) = prepare(conn)?;
    match conn.engine {
        Engine::Postgres => postgres::rust_export(&conn, out_dir, format),
        Engine::Oracle => oracle::rust_export(&conn, out_dir, format),
        Engine::Sqlserver => mssql::rust_export(&conn, out_dir, format),
        Engine::Sqlite => sqlite::rust_export(&conn, out_dir, format),
        Engine::Mysql => mysql::rust_export(&conn, out_dir, format),
    }
}

/// Anteprima **read-only** delle prime `limit` righe di una tabella. Non modifica
/// nulla; serve a sbirciare i dati durante confronto/migrazione.
pub fn preview_table(conn: &Connection, table: &str, limit: u32) -> Result<TablePreview> {
    let (conn, _guard) = prepare(conn)?;
    let (columns, rows) = match conn.engine {
        Engine::Postgres => postgres::rust_peek(&conn, table, limit)?,
        Engine::Oracle => oracle::rust_peek(&conn, table, limit)?,
        Engine::Sqlserver => mssql::rust_peek(&conn, table, limit)?,
        Engine::Sqlite => sqlite::rust_peek(&conn, table, limit)?,
        Engine::Mysql => mysql::rust_peek(&conn, table, limit)?,
    };
    let truncated = rows.len() as u32 >= limit;
    Ok(TablePreview { columns, rows, truncated })
}

/// Verifica la connessione al database.
pub fn test_connection(conn: &Connection, prefer: Prefer) -> OpResult {
    let (conn, _guard) = match prepare(conn) {
        Ok(x) => x,
        Err(e) => return early_error(e),
    };
    finalize(
        conn.engine,
        prefer,
        "Connessione riuscita".into(),
        |m, log| dispatch_test(&conn, m, log),
    )
}

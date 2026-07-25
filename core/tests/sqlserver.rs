//! Test d'integrazione su **SQL Server reale** (path puro-Rust `tiberius`) di
//! `core::ops`.
//!
//! Non gira nel normale `cargo test`: richiede un server. Il modo comodo è lo
//! script che avvia il container, applica il seed e lancia il test:
//!
//! ```bash
//! docker/sqlserver/run-mssql-test.sh
//! ```
//!
//! Oppure a mano, con `CHARON_TEST_MSSQL=1` e i database già popolati. Parametri
//! sovrascrivibili via env (default = quelli del compose): `CHARON_MSSQL_HOST`
//! (localhost), `CHARON_MSSQL_PORT` (11433), `CHARON_MSSQL_USER` (sa),
//! `CHARON_MSSQL_PASSWORD` (Charon_Pw_2026).

use charon_core::model::*;
use charon_core::ops;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Connessione a un database del container di prova. `Prefer::Rust` forza il
/// path puro-Rust (nessun `sqlcmd`/`bcp` richiesto sull'host).
fn mssql(db: &str) -> Connection {
    Connection {
        engine: Engine::Sqlserver,
        host: env_or("CHARON_MSSQL_HOST", "localhost"),
        port: env_or("CHARON_MSSQL_PORT", "11433").parse().unwrap_or(11433),
        database: db.to_string(),
        user: env_or("CHARON_MSSQL_USER", "sa"),
        password: env_or("CHARON_MSSQL_PASSWORD", "Charon_Pw_2026"),
        ssh: None,
    }
}

fn ok(r: OpResult, what: &str) {
    assert!(r.ok, "{what} fallito: {} | log: {:?}", r.message, r.log);
}

fn count(conn: &Connection, table: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM {table}");
    let r = ops::run_query(conn, &sql).expect("count query");
    r.rows[0][0]
        .as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("count non numerico: {:?}", r.rows))
}

#[test]
fn end_to_end_sqlserver_puro_rust() {
    if std::env::var("CHARON_TEST_MSSQL").is_err() {
        eprintln!("CHARON_TEST_MSSQL non impostata: test SQL Server saltato");
        return;
    }

    let src = mssql("charon_src");
    let full = mssql("charon_full");

    // --- 1) test connessione: ok sulla sorgente, KO con password sbagliata ---
    ok(ops::test_connection(&src, Prefer::Rust), "test connessione sorgente");

    let mut bad = src.clone();
    bad.password = "password-sbagliata".into();
    assert!(
        !ops::test_connection(&bad, Prefer::Rust).ok,
        "una password errata è stata accettata"
    );

    // Conteggi attesi dal seed (docker/sqlserver/init.sql).
    assert_eq!(count(&src, "customers"), 3, "seed sorgente inatteso");
    assert_eq!(count(&src, "orders"), 4, "seed sorgente inatteso");

    // --- 2) clone PIENO (schema + dati) src → charon_full --------------------
    ok(
        ops::clone(&src, &full, Prefer::Rust, &CloneOptions::default(), false),
        "clone pieno",
    );
    assert_eq!(count(&full, "customers"), 3, "clone pieno: customers non travasati");
    assert_eq!(count(&full, "orders"), 4, "clone pieno: orders non travasati");

    let diff = ops::compare(&src, &full).expect("compare src/full");
    assert!(diff.identical(), "src e full divergono dopo il clone pieno: {diff:?}");

    // NB: il mascheramento colonne è al momento supportato SOLO su PostgreSQL
    // (vedi ops::clone → Unsupported per gli altri motori), quindi qui non lo
    // esercitiamo: su SQL Server verifichiamo connessione, clone pieno e confronto.
}

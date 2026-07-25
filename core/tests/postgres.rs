//! Test d'integrazione su **PostgreSQL reale** (path puro-Rust) di `core::ops`.
//!
//! Non gira nel normale `cargo test`: richiede un server. Si attiva impostando
//! `CHARON_TEST_PG=1` e serve il Postgres "usa e getta" di `docker/postgres`:
//!
//! ```bash
//! docker compose -f docker/postgres/docker-compose.yml up -d   # attendi healthy
//! CHARON_TEST_PG=1 cargo test -p charon-core --test postgres -- --nocapture
//! ```
//!
//! Parametri sovrascrivibili via env (default = quelli del compose):
//! `CHARON_PG_HOST` (localhost), `CHARON_PG_PORT` (55432), `CHARON_PG_USER`
//! (app), `CHARON_PG_PASSWORD` (app_pw).
//!
//! Tutto il flusso vive in **un solo test**: le operazioni di clone modificano i
//! database di destinazione condivisi, quindi vanno eseguite in sequenza.

use charon_core::model::*;
use charon_core::ops;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Connessione a un database del container di prova. `Prefer::Rust` forza il
/// path puro-Rust (nessun `pg_dump`/`psql` richiesto sull'host).
fn pg(db: &str) -> Connection {
    Connection {
        engine: Engine::Postgres,
        host: env_or("CHARON_PG_HOST", "localhost"),
        port: env_or("CHARON_PG_PORT", "55432").parse().unwrap_or(55432),
        database: db.to_string(),
        user: env_or("CHARON_PG_USER", "app"),
        password: env_or("CHARON_PG_PASSWORD", "app_pw"),
        ssh: None,
    }
}

fn ok(r: OpResult, what: &str) {
    assert!(r.ok, "{what} fallito: {} | log: {:?}", r.message, r.log);
}

/// Conteggio righe di una tabella via query libera (path puro-Rust).
fn count(conn: &Connection, table: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM {table}");
    let r = ops::run_query(conn, &sql).expect("count query");
    r.rows[0][0]
        .as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("count non numerico: {:?}", r.rows))
}

fn first_email(conn: &Connection) -> Option<String> {
    let r = ops::run_query(conn, "SELECT email FROM customers ORDER BY id LIMIT 1").expect("email query");
    r.rows[0][0].clone()
}

#[test]
fn end_to_end_postgres_puro_rust() {
    if std::env::var("CHARON_TEST_PG").is_err() {
        eprintln!("CHARON_TEST_PG non impostata: test Postgres saltato");
        return;
    }

    let src = pg("charon_src");
    let full = pg("charon_full");
    let dst = pg("charon_dst");

    // --- 1) test connessione: ok sulla sorgente, KO con password sbagliata ---
    ok(ops::test_connection(&src, Prefer::Rust), "test connessione sorgente");

    let mut bad = src.clone();
    bad.password = "password-sbagliata".into();
    assert!(
        !ops::test_connection(&bad, Prefer::Rust).ok,
        "una password errata è stata accettata"
    );

    // Conteggi attesi dal seed (docker/postgres/init.sql).
    assert_eq!(count(&src, "customers"), 3, "seed sorgente inatteso");
    assert_eq!(count(&src, "orders"), 4, "seed sorgente inatteso");

    // --- 2) clone PIENO (schema + dati) src → charon_full --------------------
    ok(
        ops::clone(&src, &full, Prefer::Rust, &CloneOptions::default(), false),
        "clone pieno",
    );
    assert_eq!(count(&full, "customers"), 3, "clone pieno: customers non travasati");
    assert_eq!(count(&full, "orders"), 4, "clone pieno: orders non travasati");

    // Dopo un clone pieno i due database devono risultare identici.
    let diff = ops::compare(&src, &full).expect("compare src/full");
    assert!(diff.identical(), "src e full divergono dopo il clone pieno: {diff:?}");

    // --- 3) clone DATA-ONLY + mascheramento email src → charon_dst -----------
    let email_originale = first_email(&src).expect("la sorgente ha un'email");
    let opts = CloneOptions {
        data_only: true,
        mask: vec![MaskRule {
            table: "customers".into(),
            column: "email".into(),
            strategy: MaskStrategy::Email,
        }],
    };
    ok(
        ops::clone(&src, &dst, Prefer::Rust, &opts, false),
        "clone data-only + mask",
    );
    assert_eq!(count(&dst, "customers"), 3, "data-only: righe non travasate");

    // L'email deve essere stata mascherata: presente ma diversa dall'originale.
    let email_mascherata = first_email(&dst).expect("email presente dopo il mask");
    assert_ne!(
        email_mascherata, email_originale,
        "l'email NON è stata mascherata: {email_mascherata}"
    );
    assert!(
        !email_mascherata.is_empty(),
        "il mask email ha prodotto un valore vuoto"
    );
}

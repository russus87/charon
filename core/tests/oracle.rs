//! Test d'integrazione su **Oracle reale** (path puro-Rust, crate `oracle`/
//! ODPI-C) di `core::ops`.
//!
//! Non gira nel normale `cargo test`: richiede sia il server sia l'Oracle
//! Instant Client a runtime, ed è compilato solo con la feature `oracle-driver`
//! (attiva di default in `charon-core`). Si abilita con `CHARON_TEST_ORACLE=1`.
//!
//! Server di prova: `docker/oracle` (service `FREEPDB1`, schema `JFORM_DEV`).
//! Attenzione: quel compose pubblica la porta 1521; se hai già un Oracle su 1521
//! avvialo su un'altra porta e passala con `CHARON_ORA_PORT`.
//!
//! NB: le immagini `gvenzl/oracle-free:*slim` hanno il database GIÀ creato, per
//! cui gli script in `initdb.d` NON vengono eseguiti all'avvio: lo schema di
//! prova va applicato a mano una volta (esempio con container `charon-ora-test`):
//!
//! ```bash
//! docker exec -i charon-ora-test bash -lc \
//!   'sqlplus -s JFORM_DEV/jform_pw@localhost/FREEPDB1' < docker/oracle/init/01_schema.sql
//! ```
//!
//! L'Instant Client va nel path di ricerca del loader, es.:
//!
//! ```bash
//! export LD_LIBRARY_PATH="$HOME/.local/share/charon/instantclient/instantclient_23_26:$LD_LIBRARY_PATH"
//! CHARON_TEST_ORACLE=1 CHARON_ORA_PORT=1526 \
//!   cargo test -p charon-core --features oracle-driver --test oracle -- --nocapture
//! ```
//!
//! Parametri sovrascrivibili via env (default = quelli del compose):
//! `CHARON_ORA_HOST` (localhost), `CHARON_ORA_PORT` (1521), `CHARON_ORA_SERVICE`
//! (FREEPDB1), `CHARON_ORA_USER` (JFORM_DEV), `CHARON_ORA_PASSWORD` (jform_pw).
//!
//! Il seed Oracle crea le tabelle SENZA dati (arrivano da `oracle-load`/clone),
//! quindi il test è volutamente read-only: verifica connessione e lettura dello
//! schema, senza dipendere da conteggi righe né da commit.

#![cfg(feature = "oracle-driver")]

use charon_core::model::*;
use charon_core::ops;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Connessione al database di prova. Per Oracle `database` è il *service name*.
fn oracle() -> Connection {
    Connection {
        engine: Engine::Oracle,
        host: env_or("CHARON_ORA_HOST", "localhost"),
        port: env_or("CHARON_ORA_PORT", "1521").parse().unwrap_or(1521),
        database: env_or("CHARON_ORA_SERVICE", "FREEPDB1"),
        user: env_or("CHARON_ORA_USER", "JFORM_DEV"),
        password: env_or("CHARON_ORA_PASSWORD", "jform_pw"),
        ssh: None,
    }
}

#[test]
fn connessione_e_schema_oracle_puro_rust() {
    if std::env::var("CHARON_TEST_ORACLE").is_err() {
        eprintln!("CHARON_TEST_ORACLE non impostata: test Oracle saltato");
        return;
    }

    let conn = oracle();

    // --- 1) test connessione: ok, e KO con password sbagliata ---------------
    let r = ops::test_connection(&conn, Prefer::Rust);
    assert!(r.ok, "test connessione fallito: {} | {:?}", r.message, r.log);

    let mut bad = conn.clone();
    bad.password = "password-sbagliata".into();
    assert!(
        !ops::test_connection(&bad, Prefer::Rust).ok,
        "una password errata è stata accettata"
    );

    // --- 2) lettura schema: devono comparire le tabelle del seed ------------
    let model = ops::schema(&conn).expect("lettura schema");
    let has = |name: &str| {
        model
            .tables
            .iter()
            .any(|t| t.name.eq_ignore_ascii_case(name))
    };
    assert!(has("customers"), "tabella CUSTOMERS assente dallo schema letto");
    assert!(has("orders"), "tabella ORDERS assente dallo schema letto");
}

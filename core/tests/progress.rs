//! Verifica il canale di avanzamento **strutturato** (done/total): durante un
//! dump SQLite di più tabelle il sink deve ricevere passi monotoni fino al
//! totale delle tabelle. Nessun DB esterno: SQLite è compilato dentro il core.

use charon_core::model::*;
use charon_core::{ops, progress};
use std::sync::{Arc, Mutex};

fn conn(path: &std::path::Path) -> Connection {
    Connection {
        engine: Engine::Sqlite,
        host: String::new(),
        port: 0,
        database: path.display().to_string(),
        user: String::new(),
        password: String::new(),
        ssh: None,
    }
}

#[test]
fn dump_sqlite_riporta_avanzamento_per_tabella() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();

    // Tre tabelle: il totale atteso degli step è 3.
    let sql = d.join("seed.sql");
    std::fs::write(
        &sql,
        "CREATE TABLE a(id INTEGER PRIMARY KEY);\n\
         CREATE TABLE b(id INTEGER PRIMARY KEY);\n\
         CREATE TABLE c(id INTEGER PRIMARY KEY);\n\
         INSERT INTO a VALUES (1);\n",
    )
    .unwrap();
    let db = conn(&d.join("x.db"));
    assert!(
        ops::import(&db, &sql.display().to_string(), Prefer::Rust, false).ok,
        "import seed fallito"
    );

    // Cattura gli eventi di avanzamento emessi durante il dump.
    let events = Arc::new(Mutex::new(Vec::<(u64, u64)>::new()));
    let sink = events.clone();
    progress::set_progress_sink(Some(Box::new(move |done, total| {
        sink.lock().unwrap().push((done, total));
    })));
    let out = d.join("dump.sql");
    assert!(
        ops::dump(&db, &out.display().to_string(), Prefer::Rust, false).ok,
        "dump fallito"
    );
    progress::set_progress_sink(None);

    let ev = events.lock().unwrap();
    assert!(!ev.is_empty(), "nessun evento di avanzamento ricevuto");
    assert!(ev.iter().all(|(_, t)| *t == 3), "totale non costante a 3 tabelle: {ev:?}");

    let dones: Vec<u64> = ev.iter().map(|(d, _)| *d).collect();
    assert!(
        dones.windows(2).all(|w| w[0] <= w[1]),
        "avanzamento non monotono: {dones:?}"
    );
    assert_eq!(
        *dones.last().unwrap(),
        3,
        "l'ultimo passo deve toccare il totale: {dones:?}"
    );
}

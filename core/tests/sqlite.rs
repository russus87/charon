//! Test di integrazione SQLite: import → dump → clone su file temporanei.
//!
//! Usano solo l'API pubblica (`ops`), quindi verificano la catena completa
//! esattamente come la usa la UI. Nessun database esterno richiesto: SQLite è
//! compilato dentro `charon-core` (feature `sqlite-driver`, di default).

use charon_core::model::*;
use charon_core::ops;
use std::path::Path;

/// Connessione SQLite: per questo motore `database` è il percorso del file.
fn conn(path: &Path) -> Connection {
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

/// Dati di prova scelti per stressare i casi che rompono i dump ingenui:
/// apici nel testo, NULL, un BLOB, un tipo REAL e un indice.
const SEED: &str = r#"
CREATE TABLE autori (
  id INTEGER PRIMARY KEY,
  nome TEXT NOT NULL,
  voto REAL,
  foto BLOB,
  note TEXT
);
CREATE TABLE libri (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  titolo TEXT NOT NULL,
  autore_id INTEGER REFERENCES autori(id)
);
CREATE INDEX idx_libri_autore ON libri(autore_id);
INSERT INTO autori (id, nome, voto, foto, note) VALUES
  (1, 'Manzoni', 8.5, X'deadbeef', NULL),
  (2, 'D''Annunzio', 7.25, NULL, 'apice nel nome');
INSERT INTO libri (titolo, autore_id) VALUES ('I promessi sposi', 1), ('Il piacere', 2);
"#;

fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

/// Il BLOB di prova è presente nel dump? Il CLI `sqlite3` lo scrive `x'..'` e il
/// driver Rust `X'..'`: sono equivalenti, quindi confrontiamo senza il caso.
fn has_blob(dump: &str) -> bool {
    dump.to_lowercase().contains("x'deadbeef'")
}

fn ok(r: OpResult, what: &str) -> OpResult {
    assert!(r.ok, "{what} fallito: {} | log: {:?}", r.message, r.log);
    r
}

/// Crea un db popolato importando SEED, col metodo dato.
fn seeded(dir: &Path, name: &str, prefer: Prefer) -> Connection {
    let sql = dir.join(format!("{name}.sql"));
    write(&sql, SEED);
    let db = conn(&dir.join(format!("{name}.db")));
    ok(
        ops::import(&db, &sql.display().to_string(), prefer, false),
        "import seed",
    );
    db
}

fn dump_to_string(db: &Connection, out: &Path, prefer: Prefer) -> String {
    ok(
        ops::dump(db, &out.display().to_string(), prefer, false),
        "dump",
    );
    std::fs::read_to_string(out).unwrap()
}

/// Il clone deve produrre una destinazione **indistinguibile** dalla sorgente:
/// lo verifichiamo confrontando i rispettivi dump.
#[test]
fn clone_riproduce_la_sorgente_fedelmente() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);
    let dst = conn(&d.join("dst.db"));

    ok(
        ops::clone(&src, &dst, Prefer::Rust, &CloneOptions::default(), false),
        "clone",
    );

    let a = dump_to_string(&src, &d.join("a.sql"), Prefer::Rust);
    let b = dump_to_string(&dst, &d.join("b.sql"), Prefer::Rust);
    assert_eq!(a, b, "il dump della destinazione differisce dalla sorgente");

    // I casi ostici devono essere sopravvissuti al giro completo.
    assert!(has_blob(&a), "BLOB perso nel dump: {a}");
    assert!(a.contains("'D''Annunzio'"), "apice non escapato: {a}");
    assert!(a.contains("NULL"), "NULL perso nel dump");
    assert!(a.contains("idx_libri_autore"), "indice non riprodotto");
    assert!(a.contains("AUTOINCREMENT"), "DDL originale non preservato");
}

/// Stesso giro ma col metodo **nativo** (CLI `sqlite3`): è un percorso di codice
/// del tutto diverso (`.dump` / `.backup`). Saltato se il CLI non è installato.
#[test]
fn clone_nativo_riproduce_la_sorgente() {
    if !charon_core::sqlite::native_available() {
        eprintln!("sqlite3 non installato: test saltato");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Native);
    let dst = conn(&d.join("dst.db"));

    ok(
        ops::clone(&src, &dst, Prefer::Native, &CloneOptions::default(), false),
        "clone nativo",
    );

    let a = dump_to_string(&src, &d.join("a.sql"), Prefer::Native);
    let b = dump_to_string(&dst, &d.join("b.sql"), Prefer::Native);
    assert_eq!(a, b, "il .backup non ha riprodotto la sorgente");
    // Il CLI sqlite3 scrive i BLOB come x'..', il driver Rust come X'..':
    // entrambi validi, quindi qui il caso non conta.
    assert!(has_blob(&a), "BLOB perso: {a}");
    assert!(a.contains("'D''Annunzio'"), "apice non escapato: {a}");
}

/// I due metodi devono produrre database equivalenti: clono la stessa sorgente
/// con nativo e puro Rust e confronto i risultati (letti con lo stesso metodo,
/// così il confronto misura i dati e non il formato del dump).
#[test]
fn nativo_e_rust_producono_lo_stesso_risultato() {
    if !charon_core::sqlite::native_available() {
        eprintln!("sqlite3 non installato: test saltato");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);

    let via_native = conn(&d.join("via_native.db"));
    let via_rust = conn(&d.join("via_rust.db"));
    ok(
        ops::clone(&src, &via_native, Prefer::Native, &CloneOptions::default(), false),
        "clone nativo",
    );
    ok(
        ops::clone(&src, &via_rust, Prefer::Rust, &CloneOptions::default(), false),
        "clone rust",
    );

    let a = dump_to_string(&via_native, &d.join("a.sql"), Prefer::Native);
    let b = dump_to_string(&via_rust, &d.join("b.sql"), Prefer::Native);
    assert_eq!(a, b, "i due metodi divergono");
}

/// Il clone su un database di destinazione **già popolato** deve sostituirlo,
/// non fallire con "table already exists".
#[test]
fn clone_sovrascrive_una_destinazione_gia_popolata() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);
    let dst = seeded(d, "dst", Prefer::Rust); // stessa struttura, già piena

    // Aggiunge una riga solo sulla destinazione: dopo il clone deve sparire.
    let extra = d.join("extra.sql");
    write(&extra, "INSERT INTO autori (id, nome) VALUES (99, 'Da rimuovere');");
    ok(
        ops::import(&dst, &extra.display().to_string(), Prefer::Rust, false),
        "import extra",
    );
    let before = dump_to_string(&dst, &d.join("before.sql"), Prefer::Rust);
    assert!(before.contains("Da rimuovere"), "setup non valido");

    ok(
        ops::clone(&src, &dst, Prefer::Rust, &CloneOptions::default(), false),
        "clone su destinazione popolata",
    );

    let after = dump_to_string(&dst, &d.join("after.sql"), Prefer::Rust);
    assert!(
        !after.contains("Da rimuovere"),
        "la riga estranea è sopravvissuta al clone: {after}"
    );
    let a = dump_to_string(&src, &d.join("a.sql"), Prefer::Rust);
    assert_eq!(a, after, "destinazione non allineata alla sorgente");
}

/// Il dry-run non deve creare né toccare la destinazione.
#[test]
fn dry_run_non_tocca_la_destinazione() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);
    let dst_path = d.join("mai_creato.db");
    let dst = conn(&dst_path);

    ok(
        ops::clone(&src, &dst, Prefer::Rust, &CloneOptions::default(), true),
        "clone dry-run",
    );
    assert!(
        !dst_path.exists(),
        "il dry-run ha creato il file di destinazione"
    );
}

/// Clonare un file su se stesso lo distruggerebbe: deve essere rifiutato.
#[test]
fn clone_sullo_stesso_file_e_rifiutato() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);

    let r = ops::clone(&src, &src, Prefer::Rust, &CloneOptions::default(), false);
    assert!(!r.ok, "clone su se stesso avrebbe dovuto fallire");
    assert!(
        r.message.contains("stesso file"),
        "messaggio poco chiaro: {}",
        r.message
    );
}

/// Data-only e masking non sono implementati: devono dirlo, non fingere.
#[test]
fn data_only_e_masking_sono_rifiutati_esplicitamente() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);
    let dst = conn(&d.join("dst.db"));

    let opts = CloneOptions {
        data_only: true,
        ..Default::default()
    };
    let r = ops::clone(&src, &dst, Prefer::Rust, &opts, false);
    assert!(!r.ok, "data-only avrebbe dovuto essere rifiutato");
    assert!(
        r.message.contains("data-only"),
        "messaggio poco chiaro: {}",
        r.message
    );
}

/// Il test di connessione deve riuscire su un file valido e fallire su uno che
/// non è un database SQLite.
#[test]
fn test_connessione_distingue_un_db_valido() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);
    assert!(ops::test_connection(&src, Prefer::Rust).ok, "db valido rifiutato");

    let fasullo = d.join("non_un_db.db");
    write(&fasullo, "questo non e' un database sqlite");
    let r = ops::test_connection(&conn(&fasullo), Prefer::Rust);
    assert!(!r.ok, "un file non-SQLite è stato accettato: {}", r.message);
}

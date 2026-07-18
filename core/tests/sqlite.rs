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

/// Clonare verso un file **inesistente** deve crearlo: è il caso normale quando
/// si prepara un database nuovo dalla UI con "Nuovo…".
#[test]
fn clone_crea_il_file_di_destinazione_se_manca() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);
    let dst_path = d.join("nuovo.db");
    assert!(!dst_path.exists(), "il file non deve esistere prima");

    ok(
        ops::clone(&src, &conn(&dst_path), Prefer::Rust, &CloneOptions::default(), false),
        "clone verso file nuovo",
    );
    assert!(dst_path.exists(), "il clone non ha creato il file");

    let a = dump_to_string(&src, &d.join("a.sql"), Prefer::Rust);
    let b = dump_to_string(&conn(&dst_path), &d.join("b.sql"), Prefer::Rust);
    assert_eq!(a, b, "il db appena creato non riproduce la sorgente");
}

/// Anche l'import verso un file inesistente lo crea (SQLite genera il file al
/// primo accesso in scrittura).
#[test]
fn import_crea_il_file_se_manca() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let sql = d.join("seed.sql");
    write(&sql, SEED);
    let db_path = d.join("da_zero.db");
    assert!(!db_path.exists());

    ok(
        ops::import(&conn(&db_path), &sql.display().to_string(), Prefer::Rust, false),
        "import su file nuovo",
    );
    assert!(db_path.exists(), "l'import non ha creato il file");
    assert!(ops::test_connection(&conn(&db_path), Prefer::Rust).ok);
}

/// "Prova connessione" non deve MAI creare il file: creerebbe un db vuoto come
/// effetto collaterale di una semplice verifica. Vale per entrambi i metodi.
#[test]
fn test_connessione_non_crea_il_file() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();

    for prefer in [Prefer::Rust, Prefer::Native] {
        if prefer == Prefer::Native && !charon_core::sqlite::native_available() {
            continue;
        }
        let p = d.join(format!("mai_creato_{prefer:?}.db"));
        let r = ops::test_connection(&conn(&p), prefer);
        assert!(!r.ok, "{prefer:?}: un file inesistente è stato accettato");
        assert!(
            r.message.contains("non esiste ancora"),
            "{prefer:?}: messaggio poco chiaro: {}",
            r.message
        );
        assert!(!p.exists(), "{prefer:?}: la prova ha CREATO il file");
    }
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

/// Due database con lo stesso schema (clonati dalla stessa sorgente) devono
/// risultare identici per il confronto; introducendo una divergenza di schema
/// e una di dati, il diff deve segnalarle correttamente.
#[test]
fn compare_rileva_tabelle_e_colonne_divergenti() {
    use charon_core::compare::Status;

    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);
    let dst = conn(&d.join("dst.db"));
    ok(
        ops::clone(&src, &dst, Prefer::Rust, &CloneOptions::default(), false),
        "clone per compare",
    );

    // Appena clonati: nessuna differenza.
    let diff = charon_core::sqlite::rust_compare(&src, &dst).expect("compare (identici)");
    assert!(diff.identical(), "db appena clonati risultano diversi: {diff:?}");

    // Una tabella solo sulla destinazione, una colonna in più su 'autori', una
    // riga in più su 'libri': il diff deve vederle tutte.
    let extra_sql = d.join("extra.sql");
    write(
        &extra_sql,
        "CREATE TABLE solo_dst (id INTEGER PRIMARY KEY);\n\
         ALTER TABLE autori ADD COLUMN paese TEXT;\n\
         INSERT INTO libri (titolo, autore_id) VALUES ('Extra', 1);\n",
    );
    ok(
        ops::import(&dst, &extra_sql.display().to_string(), Prefer::Rust, false),
        "import divergenze",
    );

    let diff2 = charon_core::sqlite::rust_compare(&src, &dst).expect("compare (divergenti)");
    assert!(!diff2.identical(), "il diff non ha visto le divergenze");

    let solo_dst = diff2
        .tables
        .iter()
        .find(|t| t.name == "solo_dst")
        .expect("tabella solo_dst assente dal diff");
    assert_eq!(solo_dst.status, Status::OnlyTarget);

    let autori = diff2
        .tables
        .iter()
        .find(|t| t.name == "autori")
        .expect("tabella autori assente dal diff");
    assert_eq!(autori.status, Status::Changed);
    assert!(
        autori.columns.iter().any(|c| c.name == "paese" && c.status == Status::OnlyTarget),
        "colonna 'paese' non rilevata come OnlyTarget: {:?}",
        autori.columns
    );

    let libri = diff2
        .tables
        .iter()
        .find(|t| t.name == "libri")
        .expect("tabella libri assente dal diff");
    assert!(libri.rows_differ(), "conteggio righe di 'libri' non divergente");
}

/// Confronto dati riga-per-riga su 'autori' (chiave primaria 'id'): una riga
/// resta identica, una viene modificata solo sulla destinazione e una viene
/// aggiunta solo sulla destinazione. Il diff deve distinguere i tre casi.
#[test]
fn data_diff_rileva_righe_modificate_e_aggiunte() {
    use charon_core::compare::Status;

    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);
    let dst = conn(&d.join("dst.db"));
    ok(
        ops::clone(&src, &dst, Prefer::Rust, &CloneOptions::default(), false),
        "clone per data_diff",
    );

    // Appena clonati: dati identici.
    let diff = charon_core::sqlite::rust_data_diff(&src, &dst, "autori").expect("data_diff (identici)");
    assert_eq!(diff.key, vec!["id".to_string()], "chiave primaria non rilevata");
    assert_eq!(diff.only_source, 0);
    assert_eq!(diff.only_target, 0);
    assert_eq!(diff.changed, 0);
    assert_eq!(diff.same, 2, "le due righe seed devono risultare uguali");
    assert!(diff.note.is_none());

    // Modifica la riga id=2 e ne aggiunge una nuova (id=3), solo sulla dest.
    let extra_sql = d.join("extra.sql");
    write(
        &extra_sql,
        "UPDATE autori SET voto = 9.9 WHERE id = 2;\n\
         INSERT INTO autori (id, nome, voto) VALUES (3, 'Leopardi', 6.0);\n",
    );
    ok(
        ops::import(&dst, &extra_sql.display().to_string(), Prefer::Rust, false),
        "import modifica+aggiunta",
    );

    let diff2 = charon_core::sqlite::rust_data_diff(&src, &dst, "autori").expect("data_diff (divergenti)");
    assert_eq!(diff2.only_source, 0, "nessuna riga dovrebbe mancare in dst");
    assert_eq!(diff2.only_target, 1, "la riga id=3 è solo in dst");
    assert_eq!(diff2.changed, 1, "la riga id=2 è stata modificata");
    assert_eq!(diff2.same, 1, "la riga id=1 resta identica");
    assert!(
        diff2.sample.iter().any(|r| r.key.contains("id=3") && r.kind == Status::OnlyTarget),
        "campione senza la riga aggiunta: {:?}",
        diff2.sample
    );
    assert!(
        diff2.sample.iter().any(|r| r.key.contains("id=2") && r.kind == Status::Changed),
        "campione senza la riga modificata: {:?}",
        diff2.sample
    );
}

/// Export dei dati in CSV: un file per tabella, con header e valori grezzi
/// (non letterali SQL: niente apici attorno al testo, niente X'..' pei BLOB).
#[test]
fn export_csv_scrive_un_file_per_tabella() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);
    let out_dir = d.join("export_csv");

    let files = charon_core::sqlite::rust_export(&src, &out_dir.display().to_string(), "csv")
        .expect("export csv");
    assert_eq!(files.len(), 2, "attese 2 tabelle esportate: {files:?}");

    let autori_csv = out_dir.join("autori.csv");
    assert!(autori_csv.exists(), "manca autori.csv");
    let content = std::fs::read_to_string(&autori_csv).unwrap();
    assert!(content.starts_with("id,nome,voto,foto,note\r\n"), "header CSV inatteso: {content}");
    assert!(content.contains("Manzoni"), "valore atteso assente: {content}");
    assert!(content.contains("deadbeef"), "BLOB non esadecimale: {content}");
    // Il valore grezzo non ha apici SQL attorno al testo.
    assert!(!content.contains("'Manzoni'"), "il CSV non deve contenere letterali SQL: {content}");

    let libri_csv = out_dir.join("libri.csv");
    assert!(libri_csv.exists(), "manca libri.csv");
}

/// Stesso giro in JSON: valori come stringhe (o null), array di oggetti.
#[test]
fn export_json_produce_json_valido_con_null() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let src = seeded(d, "src", Prefer::Rust);
    let out_dir = d.join("export_json");

    let files = charon_core::sqlite::rust_export(&src, &out_dir.display().to_string(), "json")
        .expect("export json");
    assert_eq!(files.len(), 2, "attese 2 tabelle esportate: {files:?}");

    let autori_json = out_dir.join("autori.json");
    let content = std::fs::read_to_string(&autori_json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("json valido");
    assert!(parsed.is_array());
    assert!(content.contains("\"note\": null"), "NULL non reso come null: {content}");
    assert!(content.contains("Manzoni"), "valore atteso assente: {content}");
}

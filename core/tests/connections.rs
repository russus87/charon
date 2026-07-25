//! Test del deposito profili di connessione: verificano l'invariante di
//! sicurezza — **nessun segreto finisce nel file su disco** — usando le varianti
//! `*_in` con una cartella temporanea e un [`MemoryStore`] (nessun portachiavi
//! reale, nessuna variabile d'ambiente: i test sono isolati e paralleli).

use charon_core::connections::{
    delete_in, list_in, migrate_in, save_in, ConnectionProfile, MemoryStore, SecretStore,
};
use charon_core::model::{Connection, Engine, SshAuth, SshTunnel};
use std::path::Path;

fn pg(password: &str) -> Connection {
    Connection {
        engine: Engine::Postgres,
        host: "db.example.com".into(),
        port: 5432,
        database: "app".into(),
        user: "app".into(),
        password: password.into(),
        ssh: None,
    }
}

fn profile(id: &str, conn: Connection) -> ConnectionProfile {
    ConnectionProfile {
        id: id.into(),
        name: format!("profilo {id}"),
        connection: conn,
    }
}

/// Contenuto grezzo del file su disco (per controllare che i segreti non ci siano).
fn raw_file(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("connections.json")).unwrap_or_default()
}

#[test]
fn save_non_scrive_la_password_sul_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new();

    save_in(dir.path(), &store, profile("a", pg("super-segreta-123"))).unwrap();

    let raw = raw_file(dir.path());
    assert!(!raw.is_empty(), "il file avrebbe dovuto essere scritto");
    assert!(
        !raw.contains("super-segreta-123"),
        "la password è finita in chiaro nel file:\n{raw}"
    );
    // Il segreto è invece nel portachiavi, sotto la chiave stabile del profilo.
    assert_eq!(store.get("db:a").as_deref(), Some("super-segreta-123"));
}

#[test]
fn list_riempie_la_password_dal_portachiavi() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new();
    save_in(dir.path(), &store, profile("a", pg("pw1"))).unwrap();

    let listed = list_in(dir.path(), &store);
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].connection.password, "pw1",
        "la password non è stata risolta dal portachiavi"
    );
}

#[test]
fn save_ritorna_la_lista_gia_risolta() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new();
    let out = save_in(dir.path(), &store, profile("a", pg("pw1"))).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].connection.password, "pw1");
}

#[test]
fn segreto_ssh_password_va_nel_portachiavi_non_sul_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new();

    let mut conn = pg("db-pw");
    conn.ssh = Some(SshTunnel {
        host: "bastion".into(),
        port: 22,
        user: "deploy".into(),
        auth: SshAuth::Password {
            password: "ssh-pw-xyz".into(),
        },
    });
    save_in(dir.path(), &store, profile("a", conn)).unwrap();

    let raw = raw_file(dir.path());
    assert!(!raw.contains("db-pw"), "password DB in chiaro nel file:\n{raw}");
    assert!(!raw.contains("ssh-pw-xyz"), "password SSH in chiaro nel file:\n{raw}");
    assert_eq!(store.get("db:a").as_deref(), Some("db-pw"));
    assert_eq!(store.get("ssh:a").as_deref(), Some("ssh-pw-xyz"));

    // Round-trip: la lista risolta rimette la password SSH al posto giusto.
    let listed = list_in(dir.path(), &store);
    match &listed[0].connection.ssh.as_ref().unwrap().auth {
        SshAuth::Password { password } => assert_eq!(password, "ssh-pw-xyz"),
        other => panic!("auth SSH inattesa: {other:?}"),
    }
}

#[test]
fn passphrase_di_una_chiave_ssh_va_nel_portachiavi() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new();

    let mut conn = pg("");
    conn.ssh = Some(SshTunnel {
        host: "bastion".into(),
        port: 22,
        user: "deploy".into(),
        auth: SshAuth::Key {
            path: "/home/x/.ssh/id_ed25519".into(),
            passphrase: "frase-segreta".into(),
        },
    });
    save_in(dir.path(), &store, profile("k", conn)).unwrap();

    let raw = raw_file(dir.path());
    assert!(!raw.contains("frase-segreta"), "passphrase in chiaro nel file:\n{raw}");
    // Il percorso della chiave NON è un segreto: resta (giustamente) sul file.
    assert!(raw.contains("id_ed25519"), "il percorso chiave doveva restare sul file");
    assert_eq!(store.get("ssh:k").as_deref(), Some("frase-segreta"));
}

#[test]
fn auth_ssh_agent_non_crea_segreti() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new();

    let mut conn = pg("db-pw");
    conn.ssh = Some(SshTunnel {
        host: "bastion".into(),
        port: 22,
        user: "deploy".into(),
        auth: SshAuth::Agent,
    });
    save_in(dir.path(), &store, profile("ag", conn)).unwrap();

    assert_eq!(store.get("db:ag").as_deref(), Some("db-pw"));
    assert!(store.get("ssh:ag").is_none(), "l'auth via agent non deve creare un segreto SSH");
}

#[test]
fn aggiornare_stesso_id_sostituisce_e_aggiorna_il_segreto() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new();
    save_in(dir.path(), &store, profile("a", pg("vecchia"))).unwrap();

    // Stesso id, password nuova.
    let mut p = profile("a", pg("nuova"));
    p.name = "rinominato".into();
    let out = save_in(dir.path(), &store, p).unwrap();

    assert_eq!(out.len(), 1, "un id ripetuto non deve duplicare il profilo");
    assert_eq!(out[0].name, "rinominato");
    assert_eq!(store.get("db:a").as_deref(), Some("nuova"));
}

#[test]
fn salvare_con_password_vuota_rimuove_il_segreto() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new();
    save_in(dir.path(), &store, profile("a", pg("c'era"))).unwrap();
    assert!(store.get("db:a").is_some());

    // Ri-salvataggio con password vuota: il segreto va rimosso, non lasciato orfano.
    save_in(dir.path(), &store, profile("a", pg(""))).unwrap();
    assert!(store.get("db:a").is_none(), "la password vuota doveva purgare il segreto");
}

#[test]
fn delete_rimuove_profilo_e_purga_i_segreti() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new();

    let mut conn = pg("db-pw");
    conn.ssh = Some(SshTunnel {
        host: "bastion".into(),
        port: 22,
        user: "deploy".into(),
        auth: SshAuth::Password { password: "ssh-pw".into() },
    });
    save_in(dir.path(), &store, profile("a", conn)).unwrap();
    save_in(dir.path(), &store, profile("b", pg("altra"))).unwrap();

    let out = delete_in(dir.path(), &store, "a").unwrap();
    assert_eq!(out.len(), 1, "doveva restare solo il profilo 'b'");
    assert_eq!(out[0].id, "b");
    assert!(store.get("db:a").is_none(), "segreto DB non purgato");
    assert!(store.get("ssh:a").is_none(), "segreto SSH non purgato");
    // Il profilo rimasto è intatto.
    assert_eq!(store.get("db:b").as_deref(), Some("altra"));
}

#[test]
fn migrazione_sposta_le_password_in_chiaro_nel_portachiavi() {
    let dir = tempfile::tempdir().unwrap();

    // Scrive a mano un file *legacy* con la password in chiaro (formato v0.3.x).
    let legacy = r#"[
      {
        "id": "old",
        "name": "vecchio profilo",
        "connection": {
          "engine": "postgres",
          "host": "db",
          "port": 5432,
          "database": "app",
          "user": "app",
          "password": "in-chiaro-legacy"
        }
      }
    ]"#;
    std::fs::write(dir.path().join("connections.json"), legacy).unwrap();

    let store = MemoryStore::new();
    migrate_in(dir.path(), &store).unwrap();

    // Dopo la migrazione: niente segreto sul file, tutto nel portachiavi.
    let raw = raw_file(dir.path());
    assert!(!raw.contains("in-chiaro-legacy"), "la migrazione non ha ripulito il file:\n{raw}");
    assert_eq!(store.get("db:old").as_deref(), Some("in-chiaro-legacy"));

    // Idempotente: una seconda migrazione non cambia nulla e non rompe.
    migrate_in(dir.path(), &store).unwrap();
    assert_eq!(store.get("db:old").as_deref(), Some("in-chiaro-legacy"));

    // E la lista continua a risolvere correttamente la password.
    let listed = list_in(dir.path(), &store);
    assert_eq!(listed[0].connection.password, "in-chiaro-legacy");
}

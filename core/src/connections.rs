//! Profili di connessione salvati.
//!
//! Il file di configurazione (`~/.config/charon/connections.json`, oppure
//! `%LOCALAPPDATA%\charon\` su Windows, o la cartella indicata da
//! `CHARON_CONFIG_DIR`) contiene **solo dati non segreti**: la password del
//! database e il segreto del tunnel SSH (password o passphrase) vivono nel
//! portachiavi del sistema operativo tramite un [`SecretStore`] — su disco
//! restano sempre stringhe vuote.
//!
//! Le funzioni pubbliche di alto livello ([`list`], [`save`], [`delete`],
//! [`migrate_plaintext`]) usano il portachiavi reale passato dal chiamante e la
//! cartella di configurazione del sistema. Le varianti `*_in` prendono cartella
//! e store espliciti: sono usate dai test (con una cartella temporanea e un
//! [`MemoryStore`]) e non toccano né il portachiavi né la configurazione reale.

use crate::model::{Connection, SshAuth};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Deposito dei segreti del sistema: il portachiavi in produzione, una mappa in
/// memoria nei test. Le chiavi (`account`) sono stabili per id di profilo.
pub trait SecretStore {
    /// Ritorna il segreto per `account`, o `None` se assente/vuoto.
    fn get(&self, account: &str) -> Option<String>;
    /// Salva (o sovrascrive) il segreto per `account`.
    fn set(&self, account: &str, value: &str) -> Result<()>;
    /// Rimuove il segreto per `account` (no-op se assente).
    fn delete(&self, account: &str);
}

/// Deposito segreti **in memoria**: usato dai test e come ripiego. Thread-safe.
#[derive(Default)]
pub struct MemoryStore {
    map: Mutex<HashMap<String, String>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Option<String> {
        self.map
            .lock()
            .unwrap()
            .get(account)
            .filter(|s| !s.is_empty())
            .cloned()
    }
    fn set(&self, account: &str, value: &str) -> Result<()> {
        self.map
            .lock()
            .unwrap()
            .insert(account.to_string(), value.to_string());
        Ok(())
    }
    fn delete(&self, account: &str) {
        self.map.lock().unwrap().remove(account);
    }
}

/// Chiave del portachiavi per la password del database di un profilo.
fn db_account(id: &str) -> String {
    format!("db:{id}")
}
/// Chiave del portachiavi per il segreto SSH (password o passphrase) di un profilo.
fn ssh_account(id: &str) -> String {
    format!("ssh:{id}")
}

/// Una connessione salvata: un [`Connection`] con id stabile e nome leggibile.
///
/// Su disco i campi segreti (`connection.password` e il segreto SSH) sono
/// sempre vuoti; vengono riempiti dal portachiavi solo quando il profilo è
/// restituito al chiamante (vedi [`list`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    /// Identificativo stabile (generato dalla UI).
    pub id: String,
    /// Nome mostrato all'utente.
    pub name: String,
    /// I parametri di connessione veri e propri.
    pub connection: Connection,
}

// --------------------------------------------------------------- segreti ---

/// Estrae e **azzera** il segreto SSH (password o passphrase) dal `Connection`.
/// Ritorna `None` se non c'è tunnel, se l'auth è via agent, o se è vuoto.
fn take_ssh_secret(c: &mut Connection) -> Option<String> {
    let ssh = c.ssh.as_mut()?;
    let secret = match &mut ssh.auth {
        SshAuth::Password { password } => std::mem::take(password),
        SshAuth::Key { passphrase, .. } => std::mem::take(passphrase),
        SshAuth::Agent => return None,
    };
    (!secret.is_empty()).then_some(secret)
}

/// Reinserisce un segreto SSH nel punto giusto dell'enum (password o passphrase).
fn put_ssh_secret(c: &mut Connection, secret: String) {
    if let Some(ssh) = c.ssh.as_mut() {
        match &mut ssh.auth {
            SshAuth::Password { password } => *password = secret,
            SshAuth::Key { passphrase, .. } => *passphrase = secret,
            SshAuth::Agent => {}
        }
    }
}

/// Imposta il segreto se non vuoto, altrimenti lo rimuove dal portachiavi.
fn set_or_clear(store: &dyn SecretStore, account: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        store.delete(account);
        Ok(())
    } else {
        store.set(account, value)
    }
}

/// Riempie i segreti di un profilo dal portachiavi. Se il portachiavi non ha
/// nulla per quell'id, lascia intatto il valore presente nel profilo (così un
/// file *legacy* con password in chiaro resta usabile finché non lo si migra).
fn resolve(mut p: ConnectionProfile, store: &dyn SecretStore) -> ConnectionProfile {
    if let Some(pw) = store.get(&db_account(&p.id)) {
        p.connection.password = pw;
    }
    if let Some(secret) = store.get(&ssh_account(&p.id)) {
        put_ssh_secret(&mut p.connection, secret);
    }
    p
}

// ----------------------------------------------------------- file su disco ---

/// Cartella di configurazione per-utente (nessun privilegio richiesto).
/// `CHARON_CONFIG_DIR` ha la precedenza (comodo per i test e per profili portabili).
fn config_dir() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("CHARON_CONFIG_DIR") {
        return Some(PathBuf::from(x));
    }
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("charon"))
    } else if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(PathBuf::from(x).join("charon"))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("charon"))
    }
}

fn file_in(dir: &Path) -> PathBuf {
    dir.join("connections.json")
}

/// Legge i profili dal file (vuoto se non esiste o è illeggibile). I segreti
/// restano come sono sul file (vuoti in un file nuovo, in chiaro in uno legacy).
fn read_dir(dir: &Path) -> Vec<ConnectionProfile> {
    let path = file_in(dir);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_dir(dir: &Path, profiles: &[ConnectionProfile]) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = file_in(dir);
    let text = serde_json::to_string_pretty(profiles).map_err(|e| Error::Msg(e.to_string()))?;
    std::fs::write(&path, text)?;
    // Igiene: il file non contiene segreti, ma resta comunque privato all'utente.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn upsert(profiles: &mut Vec<ConnectionProfile>, profile: ConnectionProfile) {
    match profiles.iter_mut().find(|p| p.id == profile.id) {
        Some(existing) => *existing = profile,
        None => profiles.push(profile),
    }
}

// ------------------------------------------------- API a cartella esplicita ---

/// Come [`list`], ma su una cartella e uno store espliciti (per i test).
pub fn list_in(dir: &Path, store: &dyn SecretStore) -> Vec<ConnectionProfile> {
    read_dir(dir)
        .into_iter()
        .map(|p| resolve(p, store))
        .collect()
}

/// Come [`save`], ma su una cartella e uno store espliciti (per i test).
pub fn save_in(
    dir: &Path,
    store: &dyn SecretStore,
    mut profile: ConnectionProfile,
) -> Result<Vec<ConnectionProfile>> {
    // Sposta i segreti nel portachiavi e azzerali nel profilo persistito.
    let db = std::mem::take(&mut profile.connection.password);
    set_or_clear(store, &db_account(&profile.id), &db)?;

    match take_ssh_secret(&mut profile.connection) {
        Some(secret) => store.set(&ssh_account(&profile.id), &secret)?,
        None => store.delete(&ssh_account(&profile.id)),
    }

    let mut profiles = read_dir(dir);
    upsert(&mut profiles, profile);
    write_dir(dir, &profiles)?;

    Ok(profiles.into_iter().map(|p| resolve(p, store)).collect())
}

/// Come [`delete`], ma su una cartella e uno store espliciti (per i test).
pub fn delete_in(
    dir: &Path,
    store: &dyn SecretStore,
    id: &str,
) -> Result<Vec<ConnectionProfile>> {
    store.delete(&db_account(id));
    store.delete(&ssh_account(id));
    let mut profiles = read_dir(dir);
    profiles.retain(|p| p.id != id);
    write_dir(dir, &profiles)?;
    Ok(profiles.into_iter().map(|p| resolve(p, store)).collect())
}

/// Come [`migrate_plaintext`], ma su una cartella e uno store espliciti.
///
/// Sposta nel portachiavi ogni segreto ancora in chiaro nel file e riscrive il
/// file senza. Idempotente: una seconda esecuzione non trova nulla da spostare.
pub fn migrate_in(dir: &Path, store: &dyn SecretStore) -> Result<()> {
    let mut profiles = read_dir(dir);
    let mut changed = false;
    for p in profiles.iter_mut() {
        let db = std::mem::take(&mut p.connection.password);
        if !db.is_empty() {
            store.set(&db_account(&p.id), &db)?;
            changed = true;
        }
        if let Some(secret) = take_ssh_secret(&mut p.connection) {
            store.set(&ssh_account(&p.id), &secret)?;
            changed = true;
        }
    }
    if changed {
        write_dir(dir, &profiles)?;
    }
    Ok(())
}

// ---------------------------------------------------- API di produzione ---

/// Elenco delle connessioni salvate, coi segreti riempiti dal portachiavi
/// (vuoto se il file non esiste o la cartella di configurazione è indisponibile).
pub fn list(store: &dyn SecretStore) -> Vec<ConnectionProfile> {
    match config_dir() {
        Some(dir) => list_in(&dir, store),
        None => Vec::new(),
    }
}

/// Inserisce o aggiorna (per `id`) una connessione: i segreti finiscono nel
/// portachiavi, il file resta senza. Ritorna la lista aggiornata (coi segreti).
pub fn save(profile: ConnectionProfile, store: &dyn SecretStore) -> Result<Vec<ConnectionProfile>> {
    let dir = config_dir()
        .ok_or_else(|| Error::Msg("impossibile determinare la cartella di configurazione".into()))?;
    save_in(&dir, store, profile)
}

/// Elimina la connessione con quell'`id`, purgando anche i suoi segreti dal
/// portachiavi. Ritorna la lista aggiornata.
pub fn delete(id: &str, store: &dyn SecretStore) -> Result<Vec<ConnectionProfile>> {
    let dir = config_dir()
        .ok_or_else(|| Error::Msg("impossibile determinare la cartella di configurazione".into()))?;
    delete_in(&dir, store, id)
}

/// Migra un eventuale file legacy con password in chiaro: sposta i segreti nel
/// portachiavi e riscrive il file senza. Da chiamare una volta all'avvio.
pub fn migrate_plaintext(store: &dyn SecretStore) -> Result<()> {
    match config_dir() {
        Some(dir) => migrate_in(&dir, store),
        None => Ok(()),
    }
}

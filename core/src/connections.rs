//! Profili di connessione salvati, persistiti in
//! `~/.config/charon/connections.json` (Windows: `%LOCALAPPDATA%\charon\`).
//!
//! NB: la password è salvata **in chiaro** nel file (comodità per un tool
//! personale). La cifratura con master password è in roadmap.

use crate::model::Connection;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Una connessione salvata: un [`Connection`] con id stabile e nome leggibile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    /// Identificativo stabile (generato dalla UI).
    pub id: String,
    /// Nome mostrato all'utente.
    pub name: String,
    /// I parametri di connessione veri e propri.
    pub connection: Connection,
}

/// Cartella di configurazione per-utente (nessun privilegio richiesto).
fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("charon"))
    } else if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(PathBuf::from(x).join("charon"))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("charon"))
    }
}

fn file() -> Result<PathBuf> {
    config_dir()
        .map(|d| d.join("connections.json"))
        .ok_or_else(|| Error::Msg("impossibile determinare la cartella di configurazione".into()))
}

/// Elenco delle connessioni salvate (vuoto se il file non esiste o è illeggibile).
pub fn list() -> Vec<ConnectionProfile> {
    let Ok(path) = file() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write(profiles: &[ConnectionProfile]) -> Result<()> {
    let path = file()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(profiles).map_err(|e| Error::Msg(e.to_string()))?;
    std::fs::write(&path, text)?;
    Ok(())
}

/// Inserisce o aggiorna (per `id`) una connessione. Ritorna la lista aggiornata.
pub fn save(profile: ConnectionProfile) -> Result<Vec<ConnectionProfile>> {
    let mut profiles = list();
    match profiles.iter_mut().find(|p| p.id == profile.id) {
        Some(existing) => *existing = profile,
        None => profiles.push(profile),
    }
    write(&profiles)?;
    Ok(profiles)
}

/// Elimina la connessione con quell'`id`. Ritorna la lista aggiornata.
pub fn delete(id: &str) -> Result<Vec<ConnectionProfile>> {
    let mut profiles = list();
    profiles.retain(|p| p.id != id);
    write(&profiles)?;
    Ok(profiles)
}

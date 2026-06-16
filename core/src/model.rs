//! Tipi condivisi tra core e UI (tutti serializzabili in JSON per Tauri).

use serde::{Deserialize, Serialize};

/// Motore di database supportato.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Postgres,
    Oracle,
    Sqlserver,
}

impl Engine {
    /// Porta TCP di default del motore.
    pub fn default_port(self) -> u16 {
        match self {
            Engine::Postgres => 5432,
            Engine::Sqlserver => 1433,
            Engine::Oracle => 1521,
        }
    }

    /// Nome leggibile.
    pub fn label(self) -> &'static str {
        match self {
            Engine::Postgres => "PostgreSQL",
            Engine::Oracle => "Oracle",
            Engine::Sqlserver => "SQL Server",
        }
    }
}

/// Parametri di connessione a un database.
///
/// Per Oracle `database` rappresenta il *service name* (es. `XEPDB1`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub engine: Engine,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    #[serde(default)]
    pub password: String,
}

/// Come e' stata (o deve essere) eseguita l'operazione.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    /// Tool client ufficiali del database.
    Native,
    /// Fallback con driver puro Rust (best-effort).
    Rust,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::Native => "tool nativi",
            Method::Rust => "puro Rust (best-effort)",
        }
    }
}

/// Preferenza del chiamante su quale metodo usare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Prefer {
    /// Nativo se disponibile, altrimenti puro Rust.
    Auto,
    /// Forza i tool nativi (errore se mancano).
    Native,
    /// Forza il fallback puro Rust.
    Rust,
}

/// Stato di un singolo tool nativo cercato nel PATH.
#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub found: bool,
    pub path: Option<String>,
    /// A cosa serve (mostrato in UI).
    pub purpose: String,
}

/// Riepilogo, per un motore, di cosa e' disponibile sulla macchina.
#[derive(Debug, Clone, Serialize)]
pub struct EngineReport {
    pub engine: Engine,
    pub label: String,
    pub tools: Vec<ToolInfo>,
    /// I tool nativi sono sufficienti per dump+import.
    pub native_available: bool,
    /// E' disponibile il fallback puro Rust.
    pub rust_available: bool,
    /// Nota esplicativa per l'utente.
    pub note: String,
}

/// Esito di un'operazione (dump/import/clone/test).
#[derive(Debug, Clone, Serialize)]
pub struct OpResult {
    pub ok: bool,
    /// Metodo effettivamente usato.
    pub method: Method,
    pub message: String,
    /// Percorso del file prodotto (per il dump).
    pub artifact: Option<String>,
    /// Righe di log da mostrare nel pannello laterale.
    pub log: Vec<String>,
}

impl OpResult {
    pub fn ok(method: Method, message: impl Into<String>, log: Vec<String>) -> Self {
        OpResult {
            ok: true,
            method,
            message: message.into(),
            artifact: None,
            log,
        }
    }

    pub fn with_artifact(mut self, path: impl Into<String>) -> Self {
        self.artifact = Some(path.into());
        self
    }
}

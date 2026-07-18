//! Tipi condivisi tra core e UI (tutti serializzabili in JSON per Tauri).

use serde::{Deserialize, Serialize};

/// Motore di database supportato.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Postgres,
    Oracle,
    Sqlserver,
    /// SQLite: non è un server ma un **file**. Per questo motore il campo
    /// `Connection::database` contiene il **percorso del file .db**, e
    /// host/porta/utente/password non si applicano.
    Sqlite,
    /// MySQL/MariaDB: stesso protocollo di rete, stesso client (mysqldump/mysql).
    Mysql,
}

impl Engine {
    /// Il motore è un file locale (nessun server, nessuna autenticazione)?
    pub fn is_file_based(self) -> bool {
        matches!(self, Engine::Sqlite)
    }

    /// Porta TCP di default del motore. Per i motori su file non ha senso: 0.
    pub fn default_port(self) -> u16 {
        match self {
            Engine::Postgres => 5432,
            Engine::Sqlserver => 1433,
            Engine::Oracle => 1521,
            Engine::Sqlite => 0,
            Engine::Mysql => 3306,
        }
    }

    /// Nome leggibile.
    pub fn label(self) -> &'static str {
        match self {
            Engine::Postgres => "PostgreSQL",
            Engine::Oracle => "Oracle",
            Engine::Sqlserver => "SQL Server",
            Engine::Sqlite => "SQLite",
            Engine::Mysql => "MySQL",
        }
    }
}

/// Come autenticarsi al server SSH del tunnel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SshAuth {
    /// Password dell'utente SSH.
    Password { password: String },
    /// Chiave privata su file (con eventuale passphrase).
    Key {
        path: String,
        #[serde(default)]
        passphrase: String,
    },
    /// Agent SSH del sistema (ssh-agent).
    Agent,
}

impl Default for SshAuth {
    fn default() -> Self {
        SshAuth::Agent
    }
}

fn default_ssh_port() -> u16 {
    22
}

/// Tunnel SSH: Charon apre un port-forward locale verso il DB passando dal
/// server SSH (bastion). `host`/`port` del [`Connection`] sono risolti **dal
/// lato del server SSH** (tipicamente `localhost:5432` sul bastion).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTunnel {
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub auth: SshAuth,
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
    /// Se presente, la connessione passa da un tunnel SSH.
    #[serde(default)]
    pub ssh: Option<SshTunnel>,
}

/// Strategia di mascheramento di una colonna, per il clone sicuro prod→test.
///
/// Il masking si applica solo lato **puro Rust** (SELECT→trasforma→INSERT):
/// i tool nativi non possono riscrivere i valori al volo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum MaskStrategy {
    /// Sostituisce il valore con NULL.
    Null,
    /// Sostituisce con un valore fisso.
    Fixed { value: String },
    /// Pseudonimizza in modo deterministico (hash → esadecimale).
    Hash,
    /// Rimpiazza con un indirizzo email fittizio ma deterministico.
    Email,
    /// Offusca mantenendo la lunghezza (stringa di asterischi).
    Redact,
}

/// Regola di mascheramento: quale colonna di quale tabella trasformare, e come.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskRule {
    pub table: String,
    pub column: String,
    pub strategy: MaskStrategy,
}

/// Opzioni per la clonazione.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloneOptions {
    /// Se `true`, preserva lo schema della destinazione: non fa DROP/CREATE ma
    /// TRUNCATE + inserimento dati (adatto a schemi gestiti da migration).
    #[serde(default)]
    pub data_only: bool,
    /// Regole di mascheramento da applicare durante il travaso (forza il puro Rust).
    #[serde(default)]
    pub mask: Vec<MaskRule>,
}

impl CloneOptions {
    /// `true` se ci sono regole di mascheramento attive.
    pub fn has_mask(&self) -> bool {
        !self.mask.is_empty()
    }
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

/// Suggerimento per risolvere un problema rilevato su un motore.
///
/// Mostrato in UI dietro al pulsante "i" quando qualcosa non e' disponibile.
#[derive(Debug, Clone, Serialize)]
pub struct FixHint {
    /// Titolo breve del problema.
    pub title: String,
    /// Spiegazione e come risolverlo.
    pub body: String,
    /// Comando/i consigliati da copiare (opzionale), reso come blocco monospace.
    pub command: Option<String>,
}

impl FixHint {
    pub fn new(title: impl Into<String>, body: impl Into<String>, command: Option<&str>) -> Self {
        FixHint {
            title: title.into(),
            body: body.into(),
            command: command.map(|s| s.to_string()),
        }
    }
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
    /// Problemi rilevati + come risolverli (vuoto se va tutto bene).
    pub hints: Vec<FixHint>,
}

/// Anteprima (sola lettura) delle prime righe di una tabella.
#[derive(Debug, Clone, Serialize)]
pub struct TablePreview {
    pub columns: Vec<String>,
    /// Righe: una cella `None` è un NULL.
    pub rows: Vec<Vec<Option<String>>>,
    /// `true` se la tabella potrebbe avere più righe di quelle mostrate.
    pub truncated: bool,
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

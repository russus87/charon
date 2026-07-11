//! # charon-core
//!
//! Logica di **dump**, **import** e **clonazione** di database, in Rust puro
//! (niente Tauri). Motori supportati: PostgreSQL, Oracle, SQL Server.
//!
//! ## Strategia ibrida
//! Ogni operazione puo' essere eseguita in due modi:
//! - **Nativo** (`Method::Native`): Charon invoca i tool client ufficiali del
//!   database (`pg_dump`/`pg_restore`/`psql`, `mssql-scripter`/`sqlcmd`,
//!   `expdp`/`impdp`/`sqlplus`). E' la via consigliata: massima fedelta'.
//! - **Puro Rust** (`Method::Rust`): se i tool nativi non ci sono, Charon usa i
//!   driver del database per generare/applicare un dump SQL *best-effort*
//!   (schema essenziale + dati). Utile come ripiego, non sostituisce i tool.
//!
//! Il chiamante sceglie con [`model::Prefer`] (Auto/Native/Rust) e il risultato
//! ([`model::OpResult`]) riporta sempre **quale metodo** e' stato usato, cosi' la
//! UI puo' informare l'utente.

pub mod model;
pub mod mssql;
pub mod ops;
pub mod oracle;
pub mod postgres;
pub mod tools;
pub mod tunnel;

pub use model::*;

/// Errori della libreria.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("errore di I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Msg(String),
    #[error("tool nativo non trovato: {0}")]
    ToolMissing(String),
    #[error("comando fallito: {0}")]
    Cmd(String),
    #[error("connessione fallita: {0}")]
    Conn(String),
    #[error("operazione non supportata: {0}")]
    Unsupported(String),
}

/// Alias comodo.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Scorciatoia per costruire un errore generico con messaggio.
    pub fn msg(s: impl Into<String>) -> Self {
        Error::Msg(s.into())
    }
}

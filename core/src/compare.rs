//! Confronto ("compare") fra due database dello stesso motore, in stile diff:
//! quali tabelle/colonne esistono solo da una parte, quali differiscono, e di
//! quanto divergono i dati (conteggio righe).
//!
//! Il modello è volutamente **indipendente dal motore**: la UI lo rende come un
//! diff git-like, e l'applicazione selettiva (portare una voce da una parte
//! all'altra) userà le stesse voci come unità di scelta.
//!
//! Stato attuale: **sola lettura** (fase 1). Il confronto non modifica nulla.

use serde::{Deserialize, Serialize};

/// Come si colloca un oggetto rispetto ai due database confrontati.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Presente solo nella sorgente: andrebbe creato sulla destinazione.
    OnlySource,
    /// Presente solo nella destinazione: la sorgente non ce l'ha.
    OnlyTarget,
    /// Presente in entrambi, ma con definizione diversa.
    Changed,
    /// Identico da entrambe le parti.
    Same,
}

/// Differenza su una singola colonna.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDiff {
    pub name: String,
    pub status: Status,
    /// Definizione nella sorgente (es. `VARCHAR2(50) NOT NULL`), `None` se assente.
    pub source: Option<String>,
    /// Definizione nella destinazione, `None` se assente.
    pub target: Option<String>,
}

/// Differenza su una tabella: schema (colonne) e volume dati (conteggio righe).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDiff {
    pub name: String,
    pub status: Status,
    /// Solo le colonne che **non** coincidono: le identiche non sono incluse,
    /// altrimenti il diff diventa illeggibile su tabelle larghe.
    pub columns: Vec<ColumnDiff>,
    /// Numero righe nella sorgente (`None` se non contabile o tabella assente).
    pub source_rows: Option<i64>,
    /// Numero righe nella destinazione.
    pub target_rows: Option<i64>,
}

impl TableDiff {
    /// I conteggi righe divergono? Indizio economico di dati diversi: non prova
    /// che le righe siano uguali quando i conteggi coincidono (serve il diff
    /// per chiave, previsto nella fase dati).
    pub fn rows_differ(&self) -> bool {
        matches!((self.source_rows, self.target_rows), (Some(a), Some(b)) if a != b)
    }

    /// La tabella è allineata sia come schema sia come numero di righe?
    pub fn aligned(&self) -> bool {
        self.status == Status::Same && !self.rows_differ()
    }
}

/// Esito completo del confronto fra due database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbDiff {
    /// Tutte le tabelle viste da almeno una delle due parti, ordinate per nome.
    pub tables: Vec<TableDiff>,
    /// Etichette leggibili dei due lati (es. `localhost:1521/FREEPDB1`).
    pub source_label: String,
    pub target_label: String,
    /// Diagnostica del confronto (tabelle non contabili, permessi mancanti…).
    pub log: Vec<String>,
}

impl DbDiff {
    /// Quante tabelle risultano disallineate (schema o numero righe).
    pub fn diff_count(&self) -> usize {
        self.tables.iter().filter(|t| !t.aligned()).count()
    }

    /// I due database risultano allineati per quanto il confronto sa vedere.
    pub fn identical(&self) -> bool {
        self.diff_count() == 0
    }
}

//! Modello di **schema neutro** rispetto al motore: la base per il confronto e
//! la clonazione **cross-motore**.
//!
//! L'idea: ogni motore sa (a) leggere il proprio catalogo in un [`SchemaModel`]
//! mappando i tipi nativi su [`AbstractType`], e (b) generare il DDL nel proprio
//! dialetto a partire da un `AbstractType`. Così il confronto diventa un diff di
//! due modelli neutri, e la clonazione diventa «leggi il modello sorgente →
//! genera nel dialetto del target».
//!
//! Questo modulo definisce il modello e la direzione **AbstractType → DDL** per
//! tutti i motori (pura formattazione, nessun accesso al DB). La direzione
//! inversa (**catalogo → AbstractType**) vive nei moduli dei singoli motori.

use crate::model::Engine;
use serde::{Deserialize, Serialize};

/// Tipo di colonna normalizzato, indipendente dal motore. `Unknown` conserva il
/// testo originale come ripiego, così non si perde informazione sui tipi esotici.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AbstractType {
    /// Testo. `max = None` = illimitato (text/clob).
    Text { max: Option<u32> },
    /// Intero con ampiezza in bit (16/32/64).
    Integer { bits: u8 },
    /// Numerico a precisione fissa.
    Decimal { precision: Option<u32>, scale: Option<u32> },
    /// Virgola mobile (double = doppia precisione).
    Float { double: bool },
    Boolean,
    Date,
    Time,
    /// Timestamp; `tz` = con fuso orario.
    Timestamp { tz: bool },
    /// Binario. `max = None` = illimitato (blob/bytea).
    Binary { max: Option<u32> },
    Uuid,
    Json,
    /// Tipo non riconosciuto: si conserva il testo originale del motore.
    Unknown { raw: String },
}

impl AbstractType {
    /// Forma canonica per il **confronto** cross-motore: due colonne hanno lo
    /// «stesso tipo» se le loro forme canoniche coincidono. Volutamente lasca su
    /// dettagli che i motori non preservano in modo uniforme.
    pub fn canonical(&self) -> String {
        match self {
            AbstractType::Text { max: Some(n) } => format!("text({n})"),
            AbstractType::Text { max: None } => "text".into(),
            AbstractType::Integer { bits } => format!("int{bits}"),
            AbstractType::Decimal { precision, scale } => match (precision, scale) {
                (Some(p), Some(s)) => format!("decimal({p},{s})"),
                (Some(p), None) => format!("decimal({p})"),
                _ => "decimal".into(),
            },
            AbstractType::Float { double } => if *double { "float64" } else { "float32" }.into(),
            AbstractType::Boolean => "bool".into(),
            AbstractType::Date => "date".into(),
            AbstractType::Time => "time".into(),
            AbstractType::Timestamp { tz } => if *tz { "timestamptz" } else { "timestamp" }.into(),
            AbstractType::Binary { max: Some(n) } => format!("binary({n})"),
            AbstractType::Binary { max: None } => "binary".into(),
            AbstractType::Uuid => "uuid".into(),
            AbstractType::Json => "json".into(),
            AbstractType::Unknown { raw } => format!("raw:{}", raw.to_lowercase()),
        }
    }

    /// Genera il tipo DDL nel dialetto del motore dato. Best-effort per i tipi
    /// che un motore non ha nativamente (es. bool su Oracle → NUMBER(1)).
    pub fn to_ddl(&self, engine: Engine) -> String {
        use AbstractType::*;
        match engine {
            Engine::Postgres => match self {
                Text { max: Some(n) } => format!("varchar({n})"),
                Text { max: None } => "text".into(),
                Integer { bits: 16 } => "smallint".into(),
                Integer { bits: 64 } => "bigint".into(),
                Integer { .. } => "integer".into(),
                Decimal { precision: Some(p), scale: Some(s) } => format!("numeric({p},{s})"),
                Decimal { precision: Some(p), .. } => format!("numeric({p})"),
                Decimal { .. } => "numeric".into(),
                Float { double: true } => "double precision".into(),
                Float { .. } => "real".into(),
                Boolean => "boolean".into(),
                Date => "date".into(),
                Time => "time".into(),
                Timestamp { tz: true } => "timestamptz".into(),
                Timestamp { .. } => "timestamp".into(),
                Binary { .. } => "bytea".into(),
                Uuid => "uuid".into(),
                Json => "jsonb".into(),
                Unknown { raw } => raw.clone(),
            },
            Engine::Mysql => match self {
                Text { max: Some(n) } if *n <= 16000 => format!("varchar({n})"),
                Text { .. } => "text".into(),
                Integer { bits: 16 } => "smallint".into(),
                Integer { bits: 64 } => "bigint".into(),
                Integer { .. } => "int".into(),
                Decimal { precision: Some(p), scale: Some(s) } => format!("decimal({p},{s})"),
                Decimal { precision: Some(p), .. } => format!("decimal({p})"),
                Decimal { .. } => "decimal".into(),
                Float { double: true } => "double".into(),
                Float { .. } => "float".into(),
                Boolean => "tinyint(1)".into(),
                Date => "date".into(),
                Time => "time".into(),
                Timestamp { .. } => "datetime".into(),
                Binary { max: Some(n) } if *n <= 16000 => format!("varbinary({n})"),
                Binary { .. } => "blob".into(),
                Uuid => "char(36)".into(),
                Json => "json".into(),
                Unknown { raw } => raw.clone(),
            },
            Engine::Sqlite => match self {
                // SQLite usa affinità di tipo: pochi tipi "base".
                Integer { .. } | Boolean => "INTEGER".into(),
                Float { .. } => "REAL".into(),
                Decimal { .. } => "NUMERIC".into(),
                Binary { .. } => "BLOB".into(),
                _ => "TEXT".into(),
            },
            Engine::Oracle => match self {
                Text { max: Some(n) } if *n <= 4000 => format!("VARCHAR2({n})"),
                Text { .. } => "CLOB".into(),
                Integer { bits: 16 } => "NUMBER(5)".into(),
                Integer { bits: 64 } => "NUMBER(19)".into(),
                Integer { .. } => "NUMBER(10)".into(),
                Decimal { precision: Some(p), scale: Some(s) } => format!("NUMBER({p},{s})"),
                Decimal { precision: Some(p), .. } => format!("NUMBER({p})"),
                Decimal { .. } => "NUMBER".into(),
                Float { double: true } => "BINARY_DOUBLE".into(),
                Float { .. } => "BINARY_FLOAT".into(),
                Boolean => "NUMBER(1)".into(),
                Date => "DATE".into(),
                Time => "VARCHAR2(16)".into(),
                Timestamp { .. } => "TIMESTAMP".into(),
                Binary { .. } => "BLOB".into(),
                Uuid => "VARCHAR2(36)".into(),
                Json => "CLOB".into(),
                Unknown { raw } => raw.clone(),
            },
            Engine::Sqlserver => match self {
                Text { max: Some(n) } if *n <= 4000 => format!("nvarchar({n})"),
                Text { .. } => "nvarchar(max)".into(),
                Integer { bits: 16 } => "smallint".into(),
                Integer { bits: 64 } => "bigint".into(),
                Integer { .. } => "int".into(),
                Decimal { precision: Some(p), scale: Some(s) } => format!("decimal({p},{s})"),
                Decimal { precision: Some(p), .. } => format!("decimal({p})"),
                Decimal { .. } => "decimal".into(),
                Float { double: true } => "float".into(),
                Float { .. } => "real".into(),
                Boolean => "bit".into(),
                Date => "date".into(),
                Time => "time".into(),
                Timestamp { .. } => "datetime2".into(),
                Binary { max: Some(n) } if *n <= 8000 => format!("varbinary({n})"),
                Binary { .. } => "varbinary(max)".into(),
                Uuid => "uniqueidentifier".into(),
                Json => "nvarchar(max)".into(),
                Unknown { raw } => raw.clone(),
            },
        }
    }
}

/// Una colonna nel modello neutro.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub ty: AbstractType,
    pub nullable: bool,
    pub primary_key: bool,
}

impl Column {
    /// Definizione DDL della colonna nel dialetto dato (tipo + nullabilità).
    pub fn to_ddl(&self, engine: Engine) -> String {
        let null = if self.nullable { "" } else { " NOT NULL" };
        format!("{}{}", self.ty.to_ddl(engine), null)
    }
}

/// Una tabella nel modello neutro. `name` è il nome semplice (senza schema);
/// il confronto cross-motore avviene per nome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
}

impl Table {
    /// Le colonne che compongono la chiave primaria, in ordine di dichiarazione.
    pub fn pk_columns(&self) -> Vec<&str> {
        self.columns.iter().filter(|c| c.primary_key).map(|c| c.name.as_str()).collect()
    }
}

/// Lo schema neutro di un intero database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaModel {
    pub tables: Vec<Table>,
}

impl SchemaModel {
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonico_uguale_tra_motori() {
        // Uno stesso AbstractType ha la stessa forma canonica ovunque: è ciò che
        // permette al confronto cross-motore di dire «stesso tipo».
        let t = AbstractType::Text { max: Some(50) };
        assert_eq!(t.canonical(), "text(50)");
        assert_eq!(AbstractType::Integer { bits: 32 }.canonical(), "int32");
        assert_eq!(
            AbstractType::Decimal { precision: Some(10), scale: Some(2) }.canonical(),
            "decimal(10,2)"
        );
    }

    #[test]
    fn ddl_per_dialetto() {
        let t = AbstractType::Text { max: Some(50) };
        assert_eq!(t.to_ddl(Engine::Postgres), "varchar(50)");
        assert_eq!(t.to_ddl(Engine::Oracle), "VARCHAR2(50)");
        assert_eq!(t.to_ddl(Engine::Sqlserver), "nvarchar(50)");
        assert_eq!(t.to_ddl(Engine::Sqlite), "TEXT");

        let b = AbstractType::Boolean;
        assert_eq!(b.to_ddl(Engine::Postgres), "boolean");
        assert_eq!(b.to_ddl(Engine::Mysql), "tinyint(1)");
        assert_eq!(b.to_ddl(Engine::Oracle), "NUMBER(1)");
        assert_eq!(b.to_ddl(Engine::Sqlserver), "bit");

        let big = AbstractType::Integer { bits: 64 };
        assert_eq!(big.to_ddl(Engine::Postgres), "bigint");
        assert_eq!(big.to_ddl(Engine::Oracle), "NUMBER(19)");
    }
}

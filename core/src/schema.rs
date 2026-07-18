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

use crate::compare::{ColumnDiff, DbDiff, Status, TableDiff};
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

/// Definizione «logica» di una colonna per il confronto: forma canonica del tipo
/// (uguale fra motori diversi) + nullabilità.
fn logical_def(c: &Column) -> String {
    format!("{}{}", c.ty.canonical(), if c.nullable { "" } else { " NOT NULL" })
}

fn table_one_side(t: &Table, status: Status) -> TableDiff {
    let on_source = status == Status::OnlySource;
    let columns = t
        .columns
        .iter()
        .map(|c| {
            let def = Some(logical_def(c));
            ColumnDiff {
                name: c.name.clone(),
                status,
                source: if on_source { def.clone() } else { None },
                target: if on_source { None } else { def },
            }
        })
        .collect();
    TableDiff {
        name: t.name.clone(),
        status,
        columns,
        source_rows: None,
        target_rows: None,
    }
}

/// Confronta due schemi **neutri** e produce un [`DbDiff`] usando la forma
/// canonica dei tipi: così due colonne equivalenti su motori diversi (es.
/// `VARCHAR2(50)` Oracle e `varchar(50)` PostgreSQL) risultano «uguali».
/// I conteggi righe sono `None` (il confronto cross-motore è di solo schema).
pub fn diff_schemas(
    src: &SchemaModel,
    dst: &SchemaModel,
    src_label: String,
    dst_label: String,
) -> DbDiff {
    let mut names: Vec<String> = src
        .tables
        .iter()
        .chain(dst.tables.iter())
        .map(|t| t.name.clone())
        .collect();
    names.sort();
    names.dedup();

    let mut tables = Vec::new();
    for name in names {
        match (src.table(&name), dst.table(&name)) {
            (Some(st), None) => tables.push(table_one_side(st, Status::OnlySource)),
            (None, Some(dt)) => tables.push(table_one_side(dt, Status::OnlyTarget)),
            (Some(st), Some(dt)) => {
                let mut cnames: Vec<String> = st
                    .columns
                    .iter()
                    .chain(dt.columns.iter())
                    .map(|c| c.name.clone())
                    .collect();
                cnames.sort();
                cnames.dedup();

                let mut columns = Vec::new();
                for cn in cnames {
                    let sc = st.columns.iter().find(|c| c.name == cn);
                    let dc = dt.columns.iter().find(|c| c.name == cn);
                    let status = match (sc, dc) {
                        (Some(a), Some(b)) if logical_def(a) == logical_def(b) => Status::Same,
                        (Some(_), Some(_)) => Status::Changed,
                        (Some(_), None) => Status::OnlySource,
                        (None, Some(_)) => Status::OnlyTarget,
                        (None, None) => continue,
                    };
                    if status != Status::Same {
                        columns.push(ColumnDiff {
                            name: cn,
                            status,
                            source: sc.map(logical_def),
                            target: dc.map(logical_def),
                        });
                    }
                }
                let status = if columns.is_empty() { Status::Same } else { Status::Changed };
                tables.push(TableDiff { name, status, columns, source_rows: None, target_rows: None });
            }
            (None, None) => {}
        }
    }
    DbDiff { tables, source_label: src_label, target_label: dst_label, log: Vec::new() }
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

    fn col(name: &str, ty: AbstractType, nullable: bool) -> Column {
        Column { name: name.into(), ty, nullable, primary_key: false }
    }

    #[test]
    fn diff_cross_motore_normalizza_i_tipi() {
        // "Oracle": id NUMBER(10) → Integer(32), nome VARCHAR2(50) → Text(50)
        let oracle = SchemaModel {
            tables: vec![Table {
                name: "clienti".into(),
                columns: vec![
                    col("id", AbstractType::Integer { bits: 32 }, false),
                    col("nome", AbstractType::Text { max: Some(50) }, true),
                    col("solo_oracle", AbstractType::Date, true),
                ],
            }],
        };
        // "Postgres": stessi tipi logici (int/varchar(50)) → devono risultare uguali
        let pg = SchemaModel {
            tables: vec![
                Table {
                    name: "clienti".into(),
                    columns: vec![
                        col("id", AbstractType::Integer { bits: 32 }, false),
                        col("nome", AbstractType::Text { max: Some(50) }, true),
                    ],
                },
                Table { name: "solo_pg".into(), columns: vec![] },
            ],
        };
        let d = diff_schemas(&oracle, &pg, "ora".into(), "pg".into());
        let clienti = d.tables.iter().find(|t| t.name == "clienti").unwrap();
        // id e nome combaciano (normalizzati) → nel diff resta solo la colonna extra
        assert_eq!(clienti.status, Status::Changed);
        assert_eq!(clienti.columns.len(), 1);
        assert_eq!(clienti.columns[0].name, "solo_oracle");
        assert_eq!(clienti.columns[0].status, Status::OnlySource);
        // tabelle presenti da un lato solo
        assert_eq!(d.tables.iter().find(|t| t.name == "solo_pg").unwrap().status, Status::OnlyTarget);
    }
}

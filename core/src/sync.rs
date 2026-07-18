//! Generazione dello **script di allineamento** dal diff di schema: le DDL che
//! rendono la destinazione uguale alla sorgente (CREATE/DROP TABLE, ADD/DROP/
//! ALTER COLUMN). È best-effort e pensato per essere **riletto prima di
//! applicarlo**: la sintassi ALTER COLUMN e la nullabilità variano per motore,
//! quindi alcune istruzioni sono marcate da rivedere.
//!
//! Sfrutta il fatto che [`crate::compare::DbDiff`] porta già le definizioni di
//! colonna nel dialetto del motore, quindi non serve codice per-motore per
//! leggere i cataloghi: qui si formattano solo le DDL.

use crate::compare::{DbDiff, Status, TableDiff};
use crate::model::Engine;

/// Delimitatori per citare gli identificatori del motore.
struct Dialect {
    open: char,
    close: char,
    engine: Engine,
}

fn dialect(engine: Engine) -> Dialect {
    match engine {
        Engine::Sqlserver => Dialect { open: '[', close: ']', engine },
        Engine::Mysql => Dialect { open: '`', close: '`', engine },
        // PostgreSQL, Oracle, SQLite: virgolette doppie ANSI.
        _ => Dialect { open: '"', close: '"', engine },
    }
}

impl Dialect {
    fn ident(&self, name: &str) -> String {
        format!("{}{}{}", self.open, name, self.close)
    }
    /// Cita un nome tabella che può essere `schema.tabella` (SQL Server) o
    /// semplice: ogni segmento è citato separatamente.
    fn table(&self, name: &str) -> String {
        name.split('.')
            .map(|p| self.ident(p))
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// Estrae la sola parte "tipo" da una definizione tipo `varchar(80) NOT NULL`.
fn type_only(def: &str) -> &str {
    let upper = def.to_uppercase();
    for marker in [" NOT NULL", " NULL"] {
        if let Some(pos) = upper.find(marker) {
            return def[..pos].trim();
        }
    }
    def.trim()
}

/// `ALTER TABLE t ADD ...` per aggiungere una colonna, nel dialetto giusto.
fn add_column(d: &Dialect, table: &str, col: &str, def: &str) -> String {
    let t = d.table(table);
    let c = d.ident(col);
    match d.engine {
        Engine::Oracle => format!("ALTER TABLE {t} ADD ({c} {def});"),
        Engine::Sqlserver => format!("ALTER TABLE {t} ADD {c} {def};"),
        // PostgreSQL, MySQL, SQLite accettano ADD COLUMN.
        _ => format!("ALTER TABLE {t} ADD COLUMN {c} {def};"),
    }
}

/// `ALTER TABLE t DROP ...` per rimuovere una colonna.
fn drop_column(d: &Dialect, table: &str, col: &str) -> String {
    let t = d.table(table);
    let c = d.ident(col);
    match d.engine {
        Engine::Oracle => format!("ALTER TABLE {t} DROP COLUMN {c};"),
        _ => format!("ALTER TABLE {t} DROP COLUMN {c};"),
    }
}

/// Modifica il tipo/definizione di una colonna. La sintassi cambia molto per
/// motore; dove è ambiguo, si emette un commento «da rivedere».
fn alter_column(d: &Dialect, table: &str, col: &str, def: &str) -> String {
    let t = d.table(table);
    let c = d.ident(col);
    match d.engine {
        Engine::Mysql => format!("ALTER TABLE {t} MODIFY COLUMN {c} {def};"),
        Engine::Sqlserver => format!("ALTER TABLE {t} ALTER COLUMN {c} {def};"),
        Engine::Oracle => format!("ALTER TABLE {t} MODIFY ({c} {def});"),
        Engine::Postgres => format!(
            "ALTER TABLE {t} ALTER COLUMN {c} TYPE {};  -- rivedere: nullabilità non inclusa",
            type_only(def)
        ),
        Engine::Sqlite => format!(
            "-- SQLite non supporta ALTER COLUMN {c} su {t}: ricreare la tabella per cambiare «{def}»"
        ),
    }
}

/// CREATE TABLE per una tabella presente solo nella sorgente, dalle colonne del
/// diff (che portano le definizioni della sorgente).
fn create_table(d: &Dialect, t: &TableDiff) -> String {
    let cols: Vec<String> = t
        .columns
        .iter()
        .filter_map(|c| c.source.as_ref().map(|def| format!("  {} {}", d.ident(&c.name), def)))
        .collect();
    if cols.is_empty() {
        return format!("-- CREATE TABLE {} saltata: nessuna colonna nota", d.table(&t.name));
    }
    format!("CREATE TABLE {} (\n{}\n);", d.table(&t.name), cols.join(",\n"))
}

/// Genera lo script di allineamento che porta la **destinazione** a somigliare
/// alla **sorgente**, dal diff di schema. Ordina: prima le nuove tabelle, poi le
/// modifiche di colonna, infine i DROP (più distruttivi).
pub fn sync_ddl(diff: &DbDiff, engine: Engine) -> Vec<String> {
    let d = dialect(engine);
    let mut creates = Vec::new();
    let mut alters = Vec::new();
    let mut drops = Vec::new();

    for t in &diff.tables {
        match t.status {
            Status::OnlySource => creates.push(create_table(&d, t)),
            Status::OnlyTarget => drops.push(format!("DROP TABLE {};", d.table(&t.name))),
            Status::Changed => {
                for c in &t.columns {
                    match c.status {
                        Status::OnlySource => {
                            if let Some(def) = &c.source {
                                alters.push(add_column(&d, &t.name, &c.name, def));
                            }
                        }
                        Status::OnlyTarget => drops.push(drop_column(&d, &t.name, &c.name)),
                        Status::Changed => {
                            if let Some(def) = &c.source {
                                alters.push(alter_column(&d, &t.name, &c.name, def));
                            }
                        }
                        Status::Same => {}
                    }
                }
            }
            Status::Same => {}
        }
    }

    let mut out = Vec::new();
    if !creates.is_empty() {
        out.push("-- Nuove tabelle (presenti solo nella sorgente)".to_string());
        out.extend(creates);
    }
    if !alters.is_empty() {
        out.push("-- Modifiche di colonna".to_string());
        out.extend(alters);
    }
    if !drops.is_empty() {
        out.push("-- Rimozioni (DISTRUTTIVE: presenti solo nella destinazione)".to_string());
        out.extend(drops);
    }
    out
}

/// Script completo come singola stringa (con intestazione esplicativa).
pub fn sync_script(diff: &DbDiff, engine: Engine) -> String {
    let stmts = sync_ddl(diff, engine);
    let mut s = String::new();
    s.push_str("-- Script di allineamento generato da Database Studio.\n");
    s.push_str("-- Porta la DESTINAZIONE a somigliare alla SORGENTE (solo schema).\n");
    s.push_str("-- RILEGGERE prima di applicare: le righe ALTER/DROP possono perdere dati.\n\n");
    if stmts.is_empty() {
        s.push_str("-- Nessuna differenza di schema da applicare.\n");
    } else {
        s.push_str(&stmts.join("\n"));
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::{ColumnDiff, DbDiff, TableDiff};

    fn col(name: &str, st: Status, src: Option<&str>, tgt: Option<&str>) -> ColumnDiff {
        ColumnDiff {
            name: name.into(),
            status: st,
            source: src.map(|s| s.into()),
            target: tgt.map(|s| s.into()),
        }
    }
    fn diff(tables: Vec<TableDiff>) -> DbDiff {
        DbDiff { tables, source_label: "s".into(), target_label: "t".into(), log: vec![] }
    }

    #[test]
    fn genera_create_add_drop_alter() {
        let d = diff(vec![
            TableDiff {
                name: "nuova".into(),
                status: Status::OnlySource,
                source_rows: Some(0),
                target_rows: None,
                columns: vec![col("id", Status::OnlySource, Some("integer NOT NULL"), None)],
            },
            TableDiff {
                name: "clienti".into(),
                status: Status::Changed,
                source_rows: Some(1),
                target_rows: Some(1),
                columns: vec![
                    col("email", Status::OnlySource, Some("varchar(100)"), None),
                    col("vecchia", Status::OnlyTarget, None, Some("text")),
                    col("nome", Status::Changed, Some("varchar(80) NOT NULL"), Some("varchar(50) NOT NULL")),
                ],
            },
            TableDiff {
                name: "obsoleta".into(),
                status: Status::OnlyTarget,
                source_rows: None,
                target_rows: Some(3),
                columns: vec![],
            },
        ]);
        let s = sync_script(&d, Engine::Postgres);
        assert!(s.contains("CREATE TABLE \"nuova\""));
        assert!(s.contains("ALTER TABLE \"clienti\" ADD COLUMN \"email\" varchar(100);"));
        assert!(s.contains("ALTER TABLE \"clienti\" DROP COLUMN \"vecchia\";"));
        assert!(s.contains("ALTER COLUMN \"nome\" TYPE varchar(80)"));
        assert!(s.contains("DROP TABLE \"obsoleta\";"));

        // Dialetto diverso: SQL Server quota con [] e usa MODIFY/ALTER propri.
        let ms = sync_script(&d, Engine::Sqlserver);
        assert!(ms.contains("ALTER TABLE [clienti] ADD [email] varchar(100);"));
        assert!(ms.contains("DROP TABLE [obsoleta];"));

        // MySQL: backtick + MODIFY COLUMN.
        let my = sync_script(&d, Engine::Mysql);
        assert!(my.contains("ALTER TABLE `clienti` MODIFY COLUMN `nome` varchar(80) NOT NULL;"));
    }

    #[test]
    fn nessuna_differenza() {
        let d = diff(vec![TableDiff {
            name: "t".into(),
            status: Status::Same,
            source_rows: Some(1),
            target_rows: Some(1),
            columns: vec![],
        }]);
        assert!(sync_script(&d, Engine::Postgres).contains("Nessuna differenza"));
    }
}

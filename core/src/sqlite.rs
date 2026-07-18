//! SQLite: dump/import/clone.
//!
//! SQLite non è un server ma un **file**: per questo motore
//! [`Connection::database`] contiene il **percorso del file .db**, mentre
//! host/porta/utente/password non si applicano (né il tunnel SSH).
//!
//! - **Nativo**: il CLI `sqlite3` (`.dump` per esportare, stdin per importare,
//!   `.backup` per clonare — copia esatta del file).
//! - **Puro Rust** (`sqlite-driver`): `rusqlite`. A differenza degli altri
//!   motori il dump è ad **alta fedeltà** anche in puro Rust: SQLite conserva
//!   il DDL originale in `sqlite_master`, quindi lo riusiamo così com'è invece
//!   di ricostruire i tipi. I BLOB diventano letterali `X'..'`.

use crate::compare::{ColumnDiff, DbDiff, RowDelta, Status, TableDataDiff, TableDiff};
use crate::model::*;
use crate::schema::{AbstractType, Column, ForeignKey, Index, SchemaModel, Table};
use crate::tools::{find_tool, has_tool};
use crate::{Error, Result};
use std::process::Command;

/// Per SQLite il "database" è il percorso del file.
fn db_path(conn: &Connection) -> &str {
    &conn.database
}

/// Data-only e masking non sono ancora implementati per SQLite: meglio dirlo
/// che ignorare l'opzione in silenzio.
fn reject_unsupported_opts(opts: &CloneOptions) -> Result<()> {
    if opts.data_only || opts.has_mask() {
        return Err(Error::Unsupported(
            "per SQLite data-only e mascheramento non sono ancora disponibili".into(),
        ));
    }
    Ok(())
}

/// "Prova connessione" non deve **creare** nulla: il CLI `sqlite3` invece
/// genererebbe un file vuoto al primo accesso, e il driver Rust (read-only)
/// fallirebbe con un errore oscuro. Diciamo la verità: il file non c'è ancora,
/// ma come destinazione va benissimo perché lo crea il clone/import.
fn require_existing(conn: &Connection) -> Result<()> {
    let path = db_path(conn);
    if path.trim().is_empty() {
        return Err(Error::Conn("nessun file indicato".into()));
    }
    if !std::path::Path::new(path).exists() {
        return Err(Error::Conn(format!(
            "il file {path} non esiste ancora: va bene come destinazione (lo creerà \
             il clone/import), ma non c'è niente da leggere"
        )));
    }
    Ok(())
}

/// Clonare un file su se stesso lo distruggerebbe: meglio fermarsi prima.
fn reject_same_file(src: &Connection, dst: &Connection) -> Result<()> {
    let (a, b) = (db_path(src), db_path(dst));
    let same = std::fs::canonicalize(a)
        .ok()
        .zip(std::fs::canonicalize(b).ok())
        .map(|(x, y)| x == y)
        .unwrap_or(a == b);
    if same {
        return Err(Error::Unsupported(
            "sorgente e destinazione sono lo stesso file".into(),
        ));
    }
    Ok(())
}

pub fn native_available() -> bool {
    has_tool("sqlite3")
}

pub fn rust_available() -> bool {
    cfg!(feature = "sqlite-driver")
}

fn tool(name: &str, purpose: &str) -> ToolInfo {
    let p = find_tool(name);
    ToolInfo {
        name: name.into(),
        found: p.is_some(),
        path: p.map(|x| x.display().to_string()),
        purpose: purpose.into(),
    }
}

pub fn report() -> EngineReport {
    let tools = vec![tool("sqlite3", "dump (.dump), import e clone (.backup)")];
    let native = native_available();
    let rust = rust_available();
    let note = if native {
        "CLI sqlite3 trovato. È comunque disponibile il fallback puro Rust (fedeltà equivalente: \
         SQLite conserva il DDL originale)."
            .into()
    } else if rust {
        "CLI sqlite3 non trovato: verrà usato il fallback puro Rust, che per SQLite è \
         pienamente equivalente (SQLite è compilato dentro Charon)."
            .into()
    } else {
        "Nessun metodo disponibile.".into()
    };
    let mut hints = Vec::new();
    if !native {
        hints.push(FixHint::new(
            "Installa il CLI sqlite3 (opzionale)",
            "Non è necessario: Charon include SQLite al proprio interno. Serve solo se preferisci \
             il metodo nativo:",
            Some(
                "Arch:           sudo pacman -S sqlite\n\
                 Debian/Ubuntu:  sudo apt install sqlite3\n\
                 macOS:          già presente\n\
                 Windows:        https://sqlite.org/download.html",
            ),
        ));
    }
    EngineReport {
        engine: Engine::Sqlite,
        label: Engine::Sqlite.label().into(),
        tools,
        native_available: native,
        rust_available: rust,
        note,
        hints,
    }
}

// ------------------------------------------------------------------ nativo ---

pub fn native_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("sqlite3").ok_or_else(|| Error::ToolMissing("sqlite3".into()))?;
    let path = db_path(conn);
    let display = format!("sqlite3 {path} .dump > {out}");
    crate::progress::note(log, format!("$ {display}"));
    if dry {
        crate::progress::note(log, "  (dry-run: comando non eseguito)");
        return Ok(());
    }
    // Non usiamo tools::run: riverserebbe l'intero dump SQL nel log.
    let o = Command::new(&exe)
        .arg(path)
        .arg(".dump")
        .output()
        .map_err(|e| Error::Cmd(format!("{display}: {e}")))?;
    if !o.status.success() {
        return Err(Error::Cmd(format!(
            "sqlite3 .dump: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )));
    }
    std::fs::write(out, &o.stdout)?;
    crate::progress::note(log, format!("Dump scritto in {out} ({} byte)", o.stdout.len()));
    Ok(())
}

pub fn native_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("sqlite3").ok_or_else(|| Error::ToolMissing("sqlite3".into()))?;
    let path = db_path(conn);
    let display = format!("sqlite3 {path} < {input}");
    crate::progress::note(log, format!("$ {display}"));
    if dry {
        crate::progress::note(log, "  (dry-run: comando non eseguito)");
        return Ok(());
    }
    let f = std::fs::File::open(input)?;
    let o = Command::new(&exe)
        .arg(path)
        .stdin(f)
        .output()
        .map_err(|e| Error::Cmd(format!("{display}: {e}")))?;
    if !o.status.success() {
        return Err(Error::Cmd(format!(
            "sqlite3 import: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )));
    }
    crate::progress::note(log, "Import completato.");
    Ok(())
}

/// Clone nativo: `.backup` produce una copia esatta del file sorgente,
/// **sostituendo** la destinazione. È il modo idiomatico per SQLite.
pub fn native_clone(
    src: &Connection,
    dst: &Connection,
    opts: &CloneOptions,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    reject_unsupported_opts(opts)?;
    reject_same_file(src, dst)?;
    let exe = find_tool("sqlite3").ok_or_else(|| Error::ToolMissing("sqlite3".into()))?;
    let (s, d) = (db_path(src), db_path(dst));
    let display = format!("sqlite3 {s} \".backup '{d}'\"");
    crate::progress::note(log, format!("$ {display}"));
    crate::progress::note(
        log,
        "  .backup sostituisce integralmente il file di destinazione",
    );
    if dry {
        crate::progress::note(log, "  (dry-run: comando non eseguito)");
        return Ok(());
    }
    let o = Command::new(&exe)
        .arg(s)
        .arg(format!(".backup '{d}'"))
        .output()
        .map_err(|e| Error::Cmd(format!("{display}: {e}")))?;
    if !o.status.success() {
        return Err(Error::Cmd(format!(
            "sqlite3 .backup: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )));
    }
    crate::progress::note(log, format!("Copia completata in {d}"));
    Ok(())
}

pub fn native_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    let exe = find_tool("sqlite3").ok_or_else(|| Error::ToolMissing("sqlite3".into()))?;
    // Senza questo il CLI creerebbe un db vuoto solo per "provare" la connessione.
    require_existing(conn)?;
    let path = db_path(conn);
    let o = Command::new(&exe)
        .arg(path)
        .arg("SELECT sqlite_version();")
        .output()
        .map_err(|e| Error::Cmd(e.to_string()))?;
    if !o.status.success() {
        return Err(Error::Conn(
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        ));
    }
    crate::progress::note(
        log,
        format!("SQLite {}", String::from_utf8_lossy(&o.stdout).trim()),
    );
    Ok(())
}

// --------------------------------------------------------------- puro Rust ---

#[cfg(not(feature = "sqlite-driver"))]
fn no_driver() -> Error {
    Error::Unsupported("fallback SQLite non disponibile in questa build".into())
}

pub fn rust_dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::dump(conn, out, dry, log);
    }
    #[cfg(not(feature = "sqlite-driver"))]
    {
        let _ = (conn, out, dry, log);
        Err(no_driver())
    }
}

pub fn rust_import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::import(conn, input, dry, log);
    }
    #[cfg(not(feature = "sqlite-driver"))]
    {
        let _ = (conn, input, dry, log);
        Err(no_driver())
    }
}

pub fn rust_clone(
    src: &Connection,
    dst: &Connection,
    opts: &CloneOptions,
    dry: bool,
    log: &mut Vec<String>,
) -> Result<()> {
    reject_unsupported_opts(opts)?;
    reject_same_file(src, dst)?;
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::clone(src, dst, dry, log);
    }
    #[cfg(not(feature = "sqlite-driver"))]
    {
        let _ = (src, dst, dry, log);
        Err(no_driver())
    }
}

pub fn rust_test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::test(conn, log);
    }
    #[cfg(not(feature = "sqlite-driver"))]
    {
        let _ = (conn, log);
        Err(no_driver())
    }
}

/// Confronta due database SQLite (schema + conteggio righe). Sincrona: SQLite
/// non ha rete, quindi niente runtime async serve qui.
pub fn rust_compare(src: &Connection, dst: &Connection) -> Result<DbDiff> {
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::compare(src, dst);
    }
    #[cfg(not(feature = "sqlite-driver"))]
    {
        let _ = (src, dst);
        Err(no_driver())
    }
}

/// Confronta i **dati** di una tabella riga-per-riga (per chiave primaria, o
/// per riga intera in assenza di chiave). Sincrona come `rust_compare`.
pub fn rust_data_diff(src: &Connection, dst: &Connection, table: &str) -> Result<TableDataDiff> {
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::data_diff(src, dst, table);
    }
    #[cfg(not(feature = "sqlite-driver"))]
    {
        let _ = (src, dst, table);
        Err(no_driver())
    }
}

/// Esporta i dati di tutte le tabelle in CSV/JSON, un file per tabella, in
/// `out_dir`. Ritorna i percorsi dei file scritti. Sincrona come le altre
/// operazioni SQLite.
pub fn rust_export(conn: &Connection, out_dir: &str, format: &str) -> Result<Vec<String>> {
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::export(conn, out_dir, format);
    }
    #[cfg(not(feature = "sqlite-driver"))]
    {
        let _ = (conn, out_dir, format);
        Err(no_driver())
    }
}

/// Legge lo **schema neutro** (indipendente dal motore) di un database SQLite:
/// tabelle utente e colonne, con i tipi dichiarati mappati su [`AbstractType`]
/// secondo le regole di affinità di SQLite. Sincrona come le altre operazioni.
pub fn rust_read_schema(conn: &Connection) -> Result<SchemaModel> {
    #[cfg(feature = "sqlite-driver")]
    {
        return rustimpl::read_schema(conn);
    }
    #[cfg(not(feature = "sqlite-driver"))]
    {
        let _ = conn;
        Err(no_driver())
    }
}

#[cfg(feature = "sqlite-driver")]
mod rustimpl {
    use super::*;
    use rusqlite::types::ValueRef;
    use rusqlite::{Connection as Sql, OpenFlags};
    use std::collections::HashMap;

    fn open_ro(path: &str) -> Result<Sql> {
        Sql::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| Error::Conn(format!("{path}: {e}")))
    }

    fn open_rw(path: &str) -> Result<Sql> {
        Sql::open(path).map_err(|e| Error::Conn(format!("{path}: {e}")))
    }

    /// Letterale SQL di un valore. I BLOB diventano `X'aabb..'`, quindi il dump
    /// regge i binari (a differenza dei motori dove passiamo per JSON).
    fn lit(v: ValueRef) -> String {
        match v {
            ValueRef::Null => "NULL".into(),
            ValueRef::Integer(i) => i.to_string(),
            ValueRef::Real(f) => f.to_string(),
            ValueRef::Text(t) => {
                format!("'{}'", String::from_utf8_lossy(t).replace('\'', "''"))
            }
            ValueRef::Blob(b) => {
                let mut s = String::with_capacity(b.len() * 2 + 3);
                s.push_str("X'");
                for byte in b {
                    s.push_str(&format!("{byte:02x}"));
                }
                s.push('\'');
                s
            }
        }
    }

    /// Oggetti dello schema (`sqlite_master`) di un certo tipo, col DDL originale.
    fn objects(c: &Sql, kind: &str) -> Result<Vec<(String, String)>> {
        let mut st = c
            .prepare(
                "SELECT name, sql FROM sqlite_master \
                 WHERE type = ?1 AND sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .map_err(|e| Error::Msg(e.to_string()))?;
        let rows = st
            .query_map([kind], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| Error::Msg(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| Error::Msg(e.to_string()))?);
        }
        Ok(out)
    }

    // ------------------------------------------------------------- compare ---

    /// Nomi delle tabelle utente (esclude le tabelle di sistema `sqlite_%`).
    fn list_tables(c: &Sql) -> Result<Vec<String>> {
        let mut st = c
            .prepare(
                "SELECT name FROM sqlite_master \
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .map_err(|e| Error::Msg(e.to_string()))?;
        let rows = st
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| Error::Msg(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| Error::Msg(e.to_string()))?);
        }
        Ok(out)
    }

    /// Colonne di una tabella (nome, definizione leggibile) via `PRAGMA table_info`:
    /// il tipo dichiarato (può essere vuoto in SQLite) più `NOT NULL` se presente.
    fn columns(c: &Sql, table: &str) -> Result<Vec<(String, String)>> {
        let mut st = c
            .prepare(&format!("PRAGMA table_info(\"{table}\")"))
            .map_err(|e| Error::Msg(e.to_string()))?;
        let rows = st
            .query_map([], |r| {
                let name: String = r.get(1)?;
                let ctype: String = r.get(2)?;
                let notnull: i64 = r.get(3)?;
                Ok((name, ctype, notnull))
            })
            .map_err(|e| Error::Msg(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            let (name, ctype, notnull) = r.map_err(|e| Error::Msg(e.to_string()))?;
            let def = format!("{ctype}{}", if notnull == 1 { " NOT NULL" } else { "" });
            out.push((name, def));
        }
        Ok(out)
    }

    // --------------------------------------------------------- schema neutro ---

    /// Estrae gli interi tra parentesi in una dichiarazione di tipo, es.
    /// `"VARCHAR(255)"` → `[255]`, `"DECIMAL(10,2)"` → `[10, 2]`. Nessuna
    /// parentesi o contenuto non numerico → lista vuota.
    fn parse_parens(raw: &str) -> Vec<u32> {
        let Some(start) = raw.find('(') else { return Vec::new() };
        let Some(end) = raw[start..].find(')') else { return Vec::new() };
        raw[start + 1..start + end]
            .split(',')
            .filter_map(|s| s.trim().parse::<u32>().ok())
            .collect()
    }

    /// Mappa il tipo dichiarato di SQLite (stringa libera, spesso assente) su
    /// [`AbstractType`], seguendo le regole di **affinità di tipo** di SQLite:
    /// non è un vero sistema di tipi, ma un'euristica sul nome dichiarato.
    fn map_type(raw: &str) -> AbstractType {
        let upper = raw.to_uppercase();
        let nums = parse_parens(&upper);

        if upper.contains("INT") {
            let bits = if upper.contains("BIG") { 64 } else { 32 };
            AbstractType::Integer { bits }
        } else if upper.contains("CHAR") || upper.contains("CLOB") || upper.contains("TEXT") {
            AbstractType::Text { max: nums.first().copied() }
        } else if upper.contains("BOOL") {
            AbstractType::Boolean
        } else if upper.contains("REAL") || upper.contains("FLOA") || upper.contains("DOUB") {
            AbstractType::Float { double: true }
        } else if upper.contains("DEC") || upper.contains("NUMERIC") {
            let precision = nums.first().copied();
            let scale = nums.get(1).copied();
            AbstractType::Decimal { precision, scale }
        } else if upper.contains("BLOB") || upper.trim().is_empty() {
            AbstractType::Binary { max: None }
        } else if upper.contains("DATETIME") || upper.contains("TIMESTAMP") {
            AbstractType::Timestamp { tz: false }
        } else if upper.contains("DATE") {
            AbstractType::Date
        } else if upper.contains("TIME") {
            AbstractType::Time
        } else {
            // Affinità di default di SQLite per i tipi non riconosciuti.
            AbstractType::Text { max: None }
        }
    }

    /// Indici non-PK di una tabella via `PRAGMA index_list` + `PRAGMA index_info`.
    /// L'indice implicito generato da SQLite per la chiave primaria (`origin =
    /// 'pk'`) viene scartato: è già rappresentato da `Column::primary_key`.
    fn read_indexes(c: &Sql, table: &str) -> Result<Vec<Index>> {
        let mut st = c
            .prepare(&format!("PRAGMA index_list(\"{table}\")"))
            .map_err(|e| Error::Msg(e.to_string()))?;
        let rows = st
            .query_map([], |r| {
                let name: String = r.get(1)?;
                let unique: i64 = r.get(2)?;
                let origin: String = r.get(3)?;
                Ok((name, unique, origin))
            })
            .map_err(|e| Error::Msg(e.to_string()))?;
        let mut idx_rows = Vec::new();
        for r in rows {
            idx_rows.push(r.map_err(|e| Error::Msg(e.to_string()))?);
        }

        let mut indexes = Vec::new();
        for (name, unique, origin) in idx_rows {
            if origin == "pk" {
                continue;
            }
            let mut st2 = c
                .prepare(&format!("PRAGMA index_info(\"{name}\")"))
                .map_err(|e| Error::Msg(e.to_string()))?;
            let cols = st2
                .query_map([], |r| r.get::<_, String>(2))
                .map_err(|e| Error::Msg(e.to_string()))?;
            let mut columns = Vec::new();
            for cr in cols {
                columns.push(cr.map_err(|e| Error::Msg(e.to_string()))?);
            }
            indexes.push(Index { name, columns, unique: unique == 1 });
        }
        Ok(indexes)
    }

    /// Foreign key di una tabella via `PRAGMA foreign_key_list`: le righe con lo
    /// stesso `id` compongono una chiave composta, ordinate per `seq`.
    fn read_foreign_keys(c: &Sql, table: &str) -> Result<Vec<ForeignKey>> {
        let mut st = c
            .prepare(&format!("PRAGMA foreign_key_list(\"{table}\")"))
            .map_err(|e| Error::Msg(e.to_string()))?;
        let rows = st
            .query_map([], |r| {
                let id: i64 = r.get(0)?;
                let seq: i64 = r.get(1)?;
                let ref_table: String = r.get(2)?;
                let from: String = r.get(3)?;
                let to: String = r.get(4)?;
                Ok((id, seq, ref_table, from, to))
            })
            .map_err(|e| Error::Msg(e.to_string()))?;

        let mut raw = Vec::new();
        for r in rows {
            raw.push(r.map_err(|e| Error::Msg(e.to_string()))?);
        }
        // Ordina per id e poi per seq: raggruppa le chiavi composte mantenendo
        // l'ordine originale delle colonne.
        raw.sort_by_key(|(id, seq, ..)| (*id, *seq));

        let mut groups: Vec<(i64, String, Vec<String>, Vec<String>)> = Vec::new();
        for (id, _seq, ref_table, from, to) in raw {
            if let Some(g) = groups.iter_mut().find(|(gid, ..)| *gid == id) {
                g.2.push(from);
                g.3.push(to);
            } else {
                groups.push((id, ref_table, vec![from], vec![to]));
            }
        }

        Ok(groups
            .into_iter()
            .map(|(id, ref_table, columns, ref_columns)| ForeignKey {
                name: format!("fk_{table}_{id}"),
                columns,
                ref_table,
                ref_columns,
            })
            .collect())
    }

    /// Legge lo schema neutro: tabelle utenti, colonne (con default/auto-increment),
    /// indici non-PK e foreign key, via `PRAGMA table_info`/`index_list`/`index_info`/
    /// `foreign_key_list`.
    pub fn read_schema(conn: &Connection) -> Result<SchemaModel> {
        let c = open_ro(db_path(conn))?;
        let mut tables = Vec::new();
        for name in list_tables(&c)? {
            let mut st = c
                .prepare(&format!("PRAGMA table_info(\"{name}\")"))
                .map_err(|e| Error::Msg(e.to_string()))?;
            let rows = st
                .query_map([], |r| {
                    let cname: String = r.get(1)?;
                    let ctype: String = r.get(2)?;
                    let notnull: i64 = r.get(3)?;
                    let dflt: Option<String> = r.get(4)?;
                    let pk: i64 = r.get(5)?;
                    Ok((cname, ctype, notnull, dflt, pk))
                })
                .map_err(|e| Error::Msg(e.to_string()))?;
            let mut raw_columns = Vec::new();
            for r in rows {
                raw_columns.push(r.map_err(|e| Error::Msg(e.to_string()))?);
            }
            // Numero di colonne che compongono la PK: serve a distinguere una
            // singola colonna INTEGER PRIMARY KEY (alias di rowid, auto-increment)
            // da una PK composta o non intera.
            let pk_count = raw_columns.iter().filter(|(_, _, _, _, pk)| *pk > 0).count();

            let mut columns = Vec::new();
            for (cname, ctype, notnull, dflt, pk) in raw_columns {
                let upper = ctype.to_uppercase();
                let auto_increment = pk == 1 && pk_count == 1 && upper.contains("INT");
                columns.push(Column {
                    name: cname,
                    ty: map_type(&ctype),
                    nullable: notnull == 0,
                    primary_key: pk > 0,
                    auto_increment,
                    default: dflt,
                });
            }

            let indexes = read_indexes(&c, &name)?;
            let foreign_keys = read_foreign_keys(&c, &name)?;

            tables.push(Table { name, columns, indexes, foreign_keys });
        }
        Ok(SchemaModel { tables })
    }

    /// Conta le righe di una tabella. Best-effort: `None` se non contabile.
    fn count_rows(c: &Sql, table: &str) -> Option<i64> {
        c.query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| r.get(0))
            .ok()
    }

    /// Confronta schema e volume dati di due database SQLite.
    pub fn compare(src: &Connection, dst: &Connection) -> Result<DbDiff> {
        let s = open_ro(db_path(src))?;
        let d = open_ro(db_path(dst))?;
        let mut log = Vec::new();

        let stables = list_tables(&s)?;
        let dtables = list_tables(&d)?;
        crate::progress::note(
            &mut log,
            format!(
                "Tabelle: {} nella sorgente, {} nella destinazione",
                stables.len(),
                dtables.len()
            ),
        );

        // Unione ordinata dei nomi visti da almeno una parte.
        let mut names: Vec<String> = stables.iter().chain(dtables.iter()).cloned().collect();
        names.sort();
        names.dedup();

        let mut tables = Vec::new();
        for name in names {
            let in_s = stables.contains(&name);
            let in_d = dtables.contains(&name);

            // Tabella presente da un solo lato: tutte le colonne sono "nuove".
            if in_s != in_d {
                let (c, status) = if in_s {
                    (&s, Status::OnlySource)
                } else {
                    (&d, Status::OnlyTarget)
                };
                let cols = columns(c, &name)?;
                let columns_diff = cols
                    .iter()
                    .map(|(cname, def)| ColumnDiff {
                        name: cname.clone(),
                        status,
                        source: if in_s { Some(def.clone()) } else { None },
                        target: if in_s { None } else { Some(def.clone()) },
                    })
                    .collect();
                tables.push(TableDiff {
                    name: name.clone(),
                    status,
                    columns: columns_diff,
                    source_rows: if in_s { count_rows(&s, &name) } else { None },
                    target_rows: if in_s { None } else { count_rows(&d, &name) },
                });
                continue;
            }

            // Presente da entrambe le parti: confronto colonna per colonna.
            let scols = columns(&s, &name)?;
            let dcols = columns(&d, &name)?;
            let mut cnames: Vec<String> = scols
                .iter()
                .chain(dcols.iter())
                .map(|(n, _)| n.clone())
                .collect();
            cnames.sort();
            cnames.dedup();

            let mut columns_diff = Vec::new();
            for cn in cnames {
                let sc = scols.iter().find(|(n, _)| *n == cn).map(|(_, def)| def.clone());
                let dc = dcols.iter().find(|(n, _)| *n == cn).map(|(_, def)| def.clone());
                let status = match (&sc, &dc) {
                    (Some(a), Some(b)) if a == b => Status::Same,
                    (Some(_), Some(_)) => Status::Changed,
                    (Some(_), None) => Status::OnlySource,
                    (None, Some(_)) => Status::OnlyTarget,
                    (None, None) => continue, // impossibile: il nome viene da un lato
                };
                // Le colonne identiche non entrano nel diff: su tabelle larghe
                // renderebbero illeggibile ciò che conta davvero.
                if status != Status::Same {
                    columns_diff.push(ColumnDiff {
                        name: cn,
                        status,
                        source: sc,
                        target: dc,
                    });
                }
            }

            let status = if columns_diff.is_empty() {
                Status::Same
            } else {
                Status::Changed
            };
            tables.push(TableDiff {
                name: name.clone(),
                status,
                columns: columns_diff,
                source_rows: count_rows(&s, &name),
                target_rows: count_rows(&d, &name),
            });
        }

        let diff = DbDiff {
            tables,
            source_label: db_path(src).to_string(),
            target_label: db_path(dst).to_string(),
            log,
        };
        Ok(diff)
    }

    // ---------------------------------------------------------- data_diff ---

    /// Colonne che compongono la chiave primaria di una tabella, nell'ordine
    /// della chiave composta (`PRAGMA table_info`, colonna `pk`: 0 = non chiave,
    /// altrimenti la posizione 1-based nella chiave).
    fn pk_columns(c: &Sql, table: &str) -> Result<Vec<String>> {
        let mut st = c
            .prepare(&format!("PRAGMA table_info(\"{table}\")"))
            .map_err(|e| Error::Msg(e.to_string()))?;
        let rows = st
            .query_map([], |r| {
                let name: String = r.get(1)?;
                let pk: i64 = r.get(5)?;
                Ok((pk, name))
            })
            .map_err(|e| Error::Msg(e.to_string()))?;
        let mut pairs = Vec::new();
        for r in rows {
            let (pk, name) = r.map_err(|e| Error::Msg(e.to_string()))?;
            if pk > 0 {
                pairs.push((pk, name));
            }
        }
        pairs.sort_by_key(|(pk, _)| *pk);
        Ok(pairs.into_iter().map(|(_, name)| name).collect())
    }

    /// Legge tutte le righe di una tabella, indicizzate per chiave. Se `pk` è
    /// vuoto si confronta la riga intera: la "chiave" diventa tutta la riga e
    /// non resta nulla per la parte "valore" (val_str vuota, come da spec).
    ///
    /// Ritorna una mappa `key_str -> (val_str, display)`:
    /// - `key_str`/`val_str` sono i letterali delle colonne unite da `\u{1}`,
    ///   usati solo per il confronto (separatore che non compare nei dati veri);
    /// - `display` è la forma leggibile `col=val, col2=val2` per il campione UI.
    fn read_rows(c: &Sql, table: &str, pk: &[String]) -> Result<HashMap<String, (String, String)>> {
        let mut st = c
            .prepare(&format!("SELECT * FROM \"{table}\""))
            .map_err(|e| Error::Msg(format!("{table}: {e}")))?;
        let colnames: Vec<String> = st.column_names().into_iter().map(|s| s.to_string()).collect();
        let ncol = colnames.len();
        let whole_row = pk.is_empty();
        let key_idx: Vec<usize> = if whole_row {
            (0..ncol).collect()
        } else {
            pk.iter()
                .filter_map(|k| colnames.iter().position(|c| c == k))
                .collect()
        };

        let mut rows = st.query([]).map_err(|e| Error::Msg(e.to_string()))?;
        let mut map = HashMap::new();
        while let Some(r) = rows.next().map_err(|e| Error::Msg(e.to_string()))? {
            let mut key_parts = Vec::with_capacity(key_idx.len());
            let mut display_parts = Vec::with_capacity(key_idx.len());
            let mut val_parts = Vec::new();
            for i in 0..ncol {
                let rendered = lit(r.get_ref(i).map_err(|e| Error::Msg(e.to_string()))?);
                if key_idx.contains(&i) {
                    display_parts.push(format!("{}={}", colnames[i], rendered));
                    key_parts.push(rendered);
                } else {
                    val_parts.push(rendered);
                }
            }
            let key_str = key_parts.join("\u{1}");
            let val_str = val_parts.join("\u{1}");
            let display = display_parts.join(", ");
            map.insert(key_str, (val_str, display));
        }
        Ok(map)
    }

    /// Confronta i dati di una tabella riga per riga fra sorgente e destinazione.
    pub fn data_diff(src: &Connection, dst: &Connection, table: &str) -> Result<TableDataDiff> {
        let s = open_ro(db_path(src))?;
        let d = open_ro(db_path(dst))?;

        let pk = pk_columns(&s, table)?;
        let note = if pk.is_empty() {
            Some("nessuna chiave primaria: confronto per riga intera".to_string())
        } else {
            None
        };

        // Per il confronto per riga intera usiamo tutte le colonne come chiave
        // (nessuna colonna "valore" resta fuori); `read_rows` gestisce già
        // questo caso quando riceve `pk` vuoto.
        let rows_s = read_rows(&s, table, &pk)?;
        let rows_d = read_rows(&d, table, &pk)?;

        let mut only_source = 0i64;
        let mut only_target = 0i64;
        let mut changed = 0i64;
        let mut same = 0i64;
        let mut sample = Vec::new();

        for (k, (v, disp)) in &rows_s {
            match rows_d.get(k) {
                None => {
                    only_source += 1;
                    if sample.len() < 50 {
                        sample.push(RowDelta {
                            key: disp.clone(),
                            kind: Status::OnlySource,
                        });
                    }
                }
                Some((v2, _)) => {
                    if v2 != v {
                        changed += 1;
                        if sample.len() < 50 {
                            sample.push(RowDelta {
                                key: disp.clone(),
                                kind: Status::Changed,
                            });
                        }
                    } else {
                        same += 1;
                    }
                }
            }
        }
        for (k, (_, disp)) in &rows_d {
            if !rows_s.contains_key(k) {
                only_target += 1;
                if sample.len() < 50 {
                    sample.push(RowDelta {
                        key: disp.clone(),
                        kind: Status::OnlyTarget,
                    });
                }
            }
        }

        Ok(TableDataDiff {
            table: table.to_string(),
            key: pk,
            only_source,
            only_target,
            changed,
            same,
            sample,
            note,
        })
    }

    /// Genera il dump completo: schema (DDL originale) + dati + indici/trigger/viste.
    pub fn dump_sql(path: &str) -> Result<String> {
        let c = open_ro(path)?;
        let mut out = String::new();
        out.push_str("-- Dump generato da Charon (SQLite, puro Rust).\n");
        out.push_str("-- Il DDL è quello originale conservato da SQLite in sqlite_master.\n");
        out.push_str("PRAGMA foreign_keys=OFF;\nBEGIN TRANSACTION;\n\n");

        for (name, sql) in objects(&c, "table")? {
            out.push_str(&format!("DROP TABLE IF EXISTS \"{name}\";\n"));
            out.push_str(&sql);
            out.push_str(";\n");

            // Dati della tabella.
            let mut st = c
                .prepare(&format!("SELECT * FROM \"{name}\""))
                .map_err(|e| Error::Msg(format!("{name}: {e}")))?;
            let ncol = st.column_count();
            let cols = st
                .column_names()
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(", ");
            let mut rows = st.query([]).map_err(|e| Error::Msg(e.to_string()))?;
            while let Some(r) = rows.next().map_err(|e| Error::Msg(e.to_string()))? {
                let mut vals = Vec::with_capacity(ncol);
                for i in 0..ncol {
                    let v = r.get_ref(i).map_err(|e| Error::Msg(e.to_string()))?;
                    vals.push(lit(v));
                }
                out.push_str(&format!(
                    "INSERT INTO \"{name}\" ({cols}) VALUES ({});\n",
                    vals.join(", ")
                ));
            }
            out.push('\n');
        }

        // Indici, trigger e viste dopo i dati: più veloce e senza dipendenze.
        for kind in ["index", "trigger", "view"] {
            for (_, sql) in objects(&c, kind)? {
                out.push_str(&sql);
                out.push_str(";\n");
            }
        }

        out.push_str("\nCOMMIT;\n");
        Ok(out)
    }

    pub fn dump(conn: &Connection, out: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
        let sql = dump_sql(db_path(conn))?;
        let tables = sql.matches("CREATE TABLE").count();
        let rows = sql.matches("INSERT INTO ").count();
        if dry {
            crate::progress::note(
                log,
                format!(
                    "Dry-run: pronto un dump di {tables} tabelle / {rows} INSERT ({} byte). File {out} NON scritto.",
                    sql.len()
                ),
            );
            return Ok(());
        }
        std::fs::write(out, sql)?;
        crate::progress::note(
            log,
            format!("Dump SQL ({tables} tabelle, {rows} INSERT) scritto in {out}"),
        );
        Ok(())
    }

    pub fn import(conn: &Connection, input: &str, dry: bool, log: &mut Vec<String>) -> Result<()> {
        let sql = std::fs::read_to_string(input)?;
        if dry {
            crate::progress::note(
                log,
                format!(
                    "Dry-run: {input} verrebbe eseguito ({} INSERT). Nessuna modifica applicata.",
                    sql.matches("INSERT INTO ").count()
                ),
            );
            return Ok(());
        }
        let c = open_rw(db_path(conn))?;
        c.execute_batch(&sql).map_err(|e| Error::Msg(e.to_string()))?;
        crate::progress::note(log, "Import completato.");
        Ok(())
    }

    pub fn clone(src: &Connection, dst: &Connection, dry: bool, log: &mut Vec<String>) -> Result<()> {
        crate::progress::note(log, "Lettura schema+dati dalla sorgente…");
        let sql = dump_sql(db_path(src))?;
        if dry {
            crate::progress::note(
                log,
                format!(
                    "Dry-run: verrebbero ricreate {} tabelle e inserite {} righe in {}. Destinazione non modificata.",
                    sql.matches("CREATE TABLE").count(),
                    sql.matches("INSERT INTO ").count(),
                    db_path(dst)
                ),
            );
            return Ok(());
        }
        crate::progress::note(log, "Scrittura sul database di destinazione…");
        let c = open_rw(db_path(dst))?;
        c.execute_batch(&sql).map_err(|e| Error::Msg(e.to_string()))?;
        crate::progress::note(log, "Clonazione completata.");
        Ok(())
    }

    /// Esporta i dati di tutte le tabelle utente in CSV/JSON, un file per
    /// tabella, dentro `out_dir`. A differenza del dump SQL qui i valori sono
    /// **grezzi** (non letterali SQL): servono a scambiare dati con altri
    /// strumenti, non a essere rieseguiti come SQL.
    pub fn export(conn: &Connection, out_dir: &str, format: &str) -> Result<Vec<String>> {
        let fmt = crate::export::DataFormat::from_str(format)
            .ok_or_else(|| Error::Unsupported("formato non supportato".into()))?;
        std::fs::create_dir_all(out_dir)?;

        let c = open_ro(db_path(conn))?;
        let tables = list_tables(&c)?;

        let mut files = Vec::with_capacity(tables.len());
        for table in tables {
            let mut st = c
                .prepare(&format!("SELECT * FROM \"{table}\""))
                .map_err(|e| Error::Msg(format!("{table}: {e}")))?;
            let columns: Vec<String> = st.column_names().into_iter().map(|s| s.to_string()).collect();
            let ncol = columns.len();

            let mut sql_rows = st.query([]).map_err(|e| Error::Msg(e.to_string()))?;
            let mut rows: Vec<Vec<Option<String>>> = Vec::new();
            while let Some(r) = sql_rows.next().map_err(|e| Error::Msg(e.to_string()))? {
                let mut row = Vec::with_capacity(ncol);
                for i in 0..ncol {
                    let v = r.get_ref(i).map_err(|e| Error::Msg(e.to_string()))?;
                    let cell = match v {
                        ValueRef::Null => None,
                        ValueRef::Integer(i) => Some(i.to_string()),
                        ValueRef::Real(f) => Some(f.to_string()),
                        ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
                        ValueRef::Blob(b) => {
                            let mut s = String::with_capacity(b.len() * 2);
                            for byte in b {
                                s.push_str(&format!("{byte:02x}"));
                            }
                            Some(s)
                        }
                    };
                    row.push(cell);
                }
                rows.push(row);
            }

            let rendered = crate::export::render(fmt, &columns, &rows);
            let path = format!("{out_dir}/{table}.{}", fmt.ext());
            std::fs::write(&path, rendered)?;
            files.push(path);
        }
        Ok(files)
    }

    pub fn test(conn: &Connection, log: &mut Vec<String>) -> Result<()> {
        // Messaggio esplicito invece dell'oscuro "unable to open database file".
        super::require_existing(conn)?;
        let path = db_path(conn);
        let c = open_ro(path)?;
        let v: String = c
            .query_row("SELECT sqlite_version()", [], |r| r.get(0))
            .map_err(|e| Error::Conn(e.to_string()))?;
        let n: i64 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        crate::progress::note(log, format!("SQLite {v} — {n} tabelle in {path}"));
        Ok(())
    }
}

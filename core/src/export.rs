//! Serializzazione dei dati di una tabella in formati di scambio (CSV, JSON).
//!
//! I motori leggono le righe come `Vec<Vec<Option<String>>>` (una cella `None`
//! è un NULL) e qui le si rende nel formato scelto. Tenere la formattazione in
//! un solo posto garantisce lo stesso identico output per tutti i motori.

/// Formato di export dei dati.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Csv,
    Json,
}

impl DataFormat {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "csv" => Some(DataFormat::Csv),
            "json" => Some(DataFormat::Json),
            _ => None,
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            DataFormat::Csv => "csv",
            DataFormat::Json => "json",
        }
    }
}

/// Rende una tabella nel formato scelto.
pub fn render(fmt: DataFormat, columns: &[String], rows: &[Vec<Option<String>>]) -> String {
    match fmt {
        DataFormat::Csv => csv_string(columns, rows),
        DataFormat::Json => json_string(columns, rows),
    }
}

/// CSV secondo RFC 4180: campo quotato se contiene virgola, virgolette o
/// a-capo; le virgolette interne si raddoppiano; NULL → campo vuoto.
pub fn csv_string(columns: &[String], rows: &[Vec<Option<String>>]) -> String {
    let esc = |s: &str| -> String {
        if s.contains(['"', ',', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let mut out = String::new();
    out.push_str(&columns.iter().map(|c| esc(c)).collect::<Vec<_>>().join(","));
    out.push_str("\r\n");
    for row in rows {
        let line = row
            .iter()
            .map(|c| esc(c.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&line);
        out.push_str("\r\n");
    }
    out
}

/// JSON: array di oggetti `{colonna: valore}`, NULL → `null`. I valori restano
/// stringhe (fedeltà testuale garantita, nessuna re-inferenza di tipo).
pub fn json_string(columns: &[String], rows: &[Vec<Option<String>>]) -> String {
    let esc = |s: &str| -> String {
        let mut r = String::with_capacity(s.len() + 2);
        for ch in s.chars() {
            match ch {
                '"' => r.push_str("\\\""),
                '\\' => r.push_str("\\\\"),
                '\n' => r.push_str("\\n"),
                '\r' => r.push_str("\\r"),
                '\t' => r.push_str("\\t"),
                c if (c as u32) < 0x20 => r.push_str(&format!("\\u{:04x}", c as u32)),
                c => r.push(c),
            }
        }
        r
    };
    let mut out = String::from("[\n");
    for (i, row) in rows.iter().enumerate() {
        out.push_str("  {");
        let fields = columns
            .iter()
            .enumerate()
            .map(|(j, col)| {
                let val = row.get(j).and_then(|c| c.as_ref());
                match val {
                    Some(v) => format!("\"{}\": \"{}\"", esc(col), esc(v)),
                    None => format!("\"{}\": null", esc(col)),
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&fields);
        out.push('}');
        if i + 1 < rows.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Vec<String>, Vec<Vec<Option<String>>>) {
        (
            vec!["id".into(), "nome".into(), "note".into()],
            vec![
                vec![Some("1".into()), Some("Rossi".into()), None],
                vec![Some("2".into()), Some("a,b\"c".into()), Some("riga\ndue".into())],
            ],
        )
    }

    #[test]
    fn csv_escape_e_null() {
        let (c, r) = sample();
        let s = csv_string(&c, &r);
        assert!(s.starts_with("id,nome,note\r\n"));
        assert!(s.contains("1,Rossi,\r\n")); // NULL → vuoto
        assert!(s.contains("\"a,b\"\"c\"")); // virgola + virgolette raddoppiate
        assert!(s.contains("\"riga\ndue\"")); // a-capo quotato
    }

    #[test]
    fn json_escape_e_null() {
        let (c, r) = sample();
        let s = json_string(&c, &r);
        assert!(s.contains("\"note\": null"));
        assert!(s.contains("\\\"c")); // virgoletta escapata
        assert!(s.contains("riga\\ndue")); // a-capo escapato
        // dev'essere JSON valido
        let _: serde_json::Value = serde_json::from_str(&s).expect("json valido");
    }
}

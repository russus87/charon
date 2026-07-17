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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DbDiff {
        DbDiff {
            source_label: "a:5432/src".into(),
            target_label: "b:5432/dst".into(),
            log: vec![],
            tables: vec![
                TableDiff {
                    name: "clienti".into(),
                    status: Status::Changed,
                    source_rows: Some(2),
                    target_rows: Some(1),
                    columns: vec![ColumnDiff {
                        name: "email".into(),
                        status: Status::OnlySource,
                        source: Some("varchar(100)".into()),
                        target: None,
                    }],
                },
                TableDiff {
                    name: "allineata".into(),
                    status: Status::Same,
                    source_rows: Some(5),
                    target_rows: Some(5),
                    columns: vec![],
                },
            ],
        }
    }

    #[test]
    fn conteggio_e_identita() {
        let d = sample();
        assert_eq!(d.diff_count(), 1); // solo "clienti" diverge
        assert!(!d.identical());
    }

    #[test]
    fn json_contiene_le_differenze() {
        let j = sample().to_json();
        assert!(j.contains("clienti"));
        assert!(j.contains("only_source")); // Status serde snake_case
        assert!(j.contains("\"target_rows\""));
    }

    #[test]
    fn html_mostra_solo_cio_che_diverge_ed_e_escapato() {
        let mut d = sample();
        d.tables[0].name = "a<b>&".into(); // deve essere escapato
        let h = d.to_html();
        assert!(h.contains("a&lt;b&gt;&amp;"), "nome non escapato");
        assert!(h.contains("email"));
        assert!(!h.contains(">allineata<"), "la tabella allineata non va nel report");
        assert!(h.contains("righe: 2 ≠ 1"));
    }
}

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

    /// Serializza il diff in JSON leggibile (per allegarlo a un ticket o a una CI).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    /// Report HTML autonomo (nessuna dipendenza esterna), in stile diff.
    pub fn to_html(&self) -> String {
        let esc = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        };
        let mark = |st: Status| match st {
            Status::OnlySource => ("+", "src", "solo nella sorgente"),
            Status::OnlyTarget => ("−", "dst", "solo nella destinazione"),
            Status::Changed => ("~", "chg", "diversa"),
            Status::Same => ("=", "same", "uguale"),
        };
        let rows = |n: Option<i64>| n.map(|v| v.to_string()).unwrap_or_else(|| "—".into());

        let mut body = String::new();
        for t in &self.tables {
            if t.aligned() {
                continue; // il report mostra ciò che diverge, come `git status`
            }
            let (m, cls, lbl) = mark(t.status);
            let rowbadge = if t.rows_differ() {
                format!(
                    "<span class=\"rows warn\">righe: {} ≠ {}</span>",
                    rows(t.source_rows),
                    rows(t.target_rows)
                )
            } else {
                String::new()
            };
            body.push_str(&format!(
                "<div class=\"tbl {cls}\"><div class=\"th\"><span class=\"m\">{m}</span>\
                 <b>{}</b> <span class=\"lbl\">{lbl}</span> {rowbadge}</div>",
                esc(&t.name)
            ));
            for c in &t.columns {
                let (cm, ccls, _) = mark(c.status);
                body.push_str(&format!(
                    "<div class=\"col {ccls}\"><span class=\"m\">{cm}</span><span class=\"cn\">{}</span>\
                     <code>{}</code> → <code>{}</code></div>",
                    esc(&c.name),
                    esc(c.source.as_deref().unwrap_or("—")),
                    esc(c.target.as_deref().unwrap_or("—")),
                ));
            }
            body.push_str("</div>");
        }
        if body.is_empty() {
            body.push_str("<p class=\"aligned\">✓ I due database risultano allineati.</p>");
        }

        format!(
            r#"<!doctype html><html lang="it"><head><meta charset="utf-8">
<title>Charon — Compare</title><style>
body{{font-family:system-ui,-apple-system,Segoe UI,sans-serif;margin:24px;color:#16211c;background:#f4f6f5}}
h1{{font-size:20px}} .sides{{font-family:monospace;font-size:13px;color:#5c6b63;margin-bottom:16px}}
.tbl{{border:1px solid #e7eae8;border-left-width:3px;border-radius:8px;margin:6px 0;background:#fff;overflow:hidden}}
.tbl.src{{border-left-color:#1f8a4c}} .tbl.dst{{border-left-color:#c0392b}} .tbl.chg{{border-left-color:#b7791f}}
.th{{padding:9px 12px;display:flex;gap:8px;align-items:center;flex-wrap:wrap}}
.col{{padding:5px 12px 5px 30px;display:flex;gap:8px;align-items:center;border-top:1px solid #f0f2f0;font-size:13px}}
.m{{font-family:monospace;font-weight:700;width:12px}}
.src>.th .m,.col.src .m{{color:#1f8a4c}} .dst>.th .m,.col.dst .m{{color:#c0392b}} .chg>.th .m,.col.chg .m{{color:#b7791f}}
.cn{{font-family:monospace;min-width:150px}} code{{font-family:monospace;font-size:12px;background:#f2f4f8;padding:2px 6px;border-radius:5px}}
.lbl{{font-size:12px;color:#55645c;background:#eef1ef;padding:2px 8px;border-radius:999px}}
.rows{{font-size:12px;padding:2px 8px;border-radius:999px}} .warn{{background:#fbf0dc;color:#9a6512}}
.aligned{{background:#e6f6ec;color:#1f8a4c;padding:12px;border-radius:8px}}
</style></head><body>
<h1>Charon · Confronto database</h1>
<div class="sides">{} &nbsp;↔&nbsp; {}</div>
<p><b>{}</b> tabell{} con differenze su {} totali.</p>
{}
</body></html>"#,
            esc(&self.source_label),
            esc(&self.target_label),
            self.diff_count(),
            if self.diff_count() == 1 { "a" } else { "e" },
            self.tables.len(),
            body,
        )
    }
}

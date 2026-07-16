//! `charon` — interfaccia a riga di comando di Charon.
//!
//! Espone la stessa logica del `core` (dump/import/clone/test, tunnel SSH,
//! mascheramento, data-only, SQL*Loader per Oracle) senza bisogno della GUI:
//! ideale su server/CI e su sistemi senza diritti di amministrazione.
//!
//! Le password NON si passano sulla riga di comando (finirebbero nella lista
//! processi): si leggono da variabili d'ambiente. Vedi `charon help`.

use charon_core::model::{
    CloneOptions, Connection, Engine, MaskRule, MaskStrategy, OpResult, Prefer, SshAuth, SshTunnel,
};
use charon_core::ops;
use std::process::exit;

/// Argomenti già scomposti in coppie `--chiave [valore]` e in posizionali (il
/// subcomando). L'ordine delle coppie è preservato, così le opzioni ripetibili
/// come `--mask` non vanno perse.
struct Args {
    pairs: Vec<(String, Option<String>)>,
    positionals: Vec<String>,
}

/// Flag booleane note: non consumano il token successivo (che resta un
/// posizionale, es. il nome del comando).
const BOOL_FLAGS: &[&str] = &["dry-run", "data-only", "help", "h"];

impl Args {
    /// Scompone TUTTI gli argomenti: i token con prefisso `-`/`--` diventano
    /// coppie chiave/valore; i token liberi (non consumati come valore di un
    /// flag) diventano posizionali. Così il comando può stare in qualsiasi
    /// punto della riga (`charon --db x test` == `charon test --db x`), senza
    /// confondersi con un valore (`--db test` tiene "test" come valore di --db).
    fn parse(raw: &[String]) -> Args {
        let mut pairs = Vec::new();
        let mut positionals = Vec::new();
        let mut i = 0;
        while i < raw.len() {
            let a = &raw[i];
            if let Some(key) = a.strip_prefix("--").or_else(|| a.strip_prefix('-')) {
                if let Some((k, v)) = key.split_once('=') {
                    pairs.push((k.to_string(), Some(v.to_string())));
                } else if BOOL_FLAGS.contains(&key) {
                    pairs.push((key.to_string(), None));
                } else if i + 1 < raw.len() && !raw[i + 1].starts_with('-') {
                    pairs.push((key.to_string(), Some(raw[i + 1].clone())));
                    i += 1;
                } else {
                    pairs.push((key.to_string(), None));
                }
            } else {
                positionals.push(a.clone());
            }
            i += 1;
        }
        Args { pairs, positionals }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.as_deref())
    }

    fn get_or(&self, key: &str, default: &str) -> String {
        self.get(key).unwrap_or(default).to_string()
    }

    fn has(&self, key: &str) -> bool {
        self.pairs.iter().any(|(k, _)| k == key)
    }

    fn all(&self, key: &str) -> Vec<String> {
        self.pairs
            .iter()
            .filter(|(k, _)| k == key)
            .filter_map(|(_, v)| v.clone())
            .collect()
    }
}

fn parse_engine(s: &str) -> Engine {
    match s.to_lowercase().as_str() {
        "postgres" | "postgresql" | "pg" => Engine::Postgres,
        "oracle" | "ora" => Engine::Oracle,
        "mssql" | "sqlserver" | "sql-server" | "ss" => Engine::Sqlserver,
        other => {
            eprintln!("motore sconosciuto '{other}' (usa: postgres | oracle | mssql)");
            exit(2);
        }
    }
}

fn parse_prefer(s: &str) -> Prefer {
    match s.to_lowercase().as_str() {
        "auto" => Prefer::Auto,
        "native" | "nativo" => Prefer::Native,
        "rust" => Prefer::Rust,
        other => {
            eprintln!("metodo sconosciuto '{other}' (usa: auto | native | rust)");
            exit(2);
        }
    }
}

/// Costruisce un tunnel SSH da flag con prefisso (`` o `dst-`). La password/
/// passphrase SSH arriva dall'ambiente per non finire negli argomenti.
fn build_ssh(a: &Args, prefix: &str, env_prefix: &str) -> Option<SshTunnel> {
    let host = a.get(&format!("{prefix}ssh-host"))?;
    let auth = if let Some(key) = a.get(&format!("{prefix}ssh-key")) {
        SshAuth::Key {
            path: key.to_string(),
            passphrase: std::env::var(format!("{env_prefix}SSH_KEYPASS")).unwrap_or_default(),
        }
    } else if let Ok(pw) = std::env::var(format!("{env_prefix}SSH_PASSWORD")) {
        SshAuth::Password { password: pw }
    } else {
        SshAuth::Agent
    };
    Some(SshTunnel {
        host: host.to_string(),
        port: a.get_or(&format!("{prefix}ssh-port"), "22").parse().unwrap_or(22),
        user: a.get_or(&format!("{prefix}ssh-user"), "root"),
        auth,
    })
}

/// Costruisce una connessione dai flag. `prefix` è "" per la sorgente e "dst-"
/// per la destinazione del clone; `env_prefix` seleziona la variabile d'ambiente
/// della password (`CHARON_PASSWORD` o `CHARON_DST_PASSWORD`).
fn build_conn(a: &Args, prefix: &str, env_prefix: &str) -> Connection {
    let engine = parse_engine(&a.get_or(&format!("{prefix}engine"), "postgres"));
    let password = a
        .get(&format!("{prefix}password"))
        .map(|s| s.to_string())
        .or_else(|| std::env::var(format!("{env_prefix}PASSWORD")).ok())
        .unwrap_or_default();
    Connection {
        engine,
        host: a.get_or(&format!("{prefix}host"), "localhost"),
        port: a
            .get(&format!("{prefix}port"))
            .and_then(|p| p.parse().ok())
            .unwrap_or_else(|| engine.default_port()),
        database: a.get_or(&format!("{prefix}db"), ""),
        user: a.get_or(&format!("{prefix}user"), ""),
        password,
        ssh: build_ssh(a, prefix, env_prefix),
    }
}

/// Parsa `tab.col=strategia` (ripetibile). Strategie: null|hash|email|redact|fixed:VAL.
fn parse_masks(specs: &[String]) -> Vec<MaskRule> {
    let mut rules = Vec::new();
    for entry in specs {
        let Some((target, strat)) = entry.split_once('=') else { continue };
        let Some((table, column)) = target.split_once('.') else { continue };
        let strategy = match strat {
            "null" => MaskStrategy::Null,
            "hash" => MaskStrategy::Hash,
            "email" => MaskStrategy::Email,
            "redact" => MaskStrategy::Redact,
            other => match other.strip_prefix("fixed:") {
                Some(v) => MaskStrategy::Fixed { value: v.to_string() },
                None => {
                    eprintln!("strategia mask sconosciuta: '{strat}'");
                    exit(2);
                }
            },
        };
        rules.push(MaskRule {
            table: table.to_string(),
            column: column.to_string(),
            strategy,
        });
    }
    rules
}

/// Stampa un OpResult e restituisce il codice di uscita (0 = ok).
fn report(r: &OpResult) -> i32 {
    println!("── {} ──", if r.ok { "OK" } else { "ERRORE" });
    println!("metodo:    {}", r.method.label());
    println!("messaggio: {}", r.message);
    if !r.log.is_empty() {
        println!("log:");
        for line in &r.log {
            println!("  {line}");
        }
    }
    if r.ok {
        0
    } else {
        1
    }
}

fn require<'a>(a: &'a Args, key: &str, cmd: &str) -> &'a str {
    match a.get(key) {
        Some(v) => v,
        None => {
            eprintln!("manca --{key} (obbligatorio per '{cmd}'). Vedi 'charon help'.");
            exit(2);
        }
    }
}

const HELP: &str = r#"charon — dump/import/clone di database (PostgreSQL, Oracle, SQL Server)

USO:
  charon <comando> [opzioni]

COMANDI:
  tools                    Elenca i tool nativi rilevati e i metodi disponibili
  test                     Verifica la connessione al database
  dump   --out FILE        Esporta il database su file
  import --in FILE         Importa un dump nel database
  clone  --dst-... FLAGS   Copia sorgente → destinazione (stesso motore)
  oracle-load --dir PKG    Importa un pacchetto SQL*Loader (.ctl/.ldr) via sqlldr
  oracle-setup --zip ZIP   Scompatta e aggancia l'Oracle Instant Client (no-admin)
  help                     Mostra questo aiuto

OPZIONI DI CONNESSIONE (sorgente / connessione principale):
  --engine pg|oracle|mssql   Motore (default: pg)
  --host HOST                (default: localhost)
  --port N                   (default: porta del motore)
  --db NOME                  Database / service name Oracle
  --user UTENTE
  --password PW              SCONSIGLIATO: preferisci la variabile d'ambiente
  --ssh-host / --ssh-port / --ssh-user / --ssh-key   Tunnel SSH opzionale

DESTINAZIONE DEL CLONE: stessi flag col prefisso --dst- (es. --dst-host, --dst-db).

OPZIONI OPERATIVE:
  --prefer auto|native|rust  Metodo (default: auto)
  --dry-run                  Anteprima: non modifica nulla, mostra solo il piano
  --data-only                Clone: preserva lo schema destinazione e copia i dati
                             (PostgreSQL: sostituisce con TRUNCATE; SQL Server: append)
  --mask tab.col=STRAT       Clone: maschera una colonna (ripetibile)
                             STRAT: null|hash|email|redact|fixed:VALORE

VARIABILI D'AMBIENTE (password — non compaiono nella lista processi):
  CHARON_PASSWORD            Password della connessione principale/sorgente
  CHARON_DST_PASSWORD        Password della destinazione del clone
  CHARON_SSH_PASSWORD / CHARON_SSH_KEYPASS           Credenziali SSH sorgente
  CHARON_DST_SSH_PASSWORD / CHARON_DST_SSH_KEYPASS   Credenziali SSH destinazione

ESEMPI:
  charon tools
  CHARON_PASSWORD=secret charon test --engine pg --host db --db app --user app
  CHARON_PASSWORD=s charon dump --db app --user app --out app.sql --prefer rust
  CHARON_PASSWORD=s CHARON_DST_PASSWORD=d charon clone \
      --db prod --user app --dst-host stage --dst-db stage --dst-user app \
      --data-only --mask users.email=email --dry-run
  CHARON_PASSWORD=s charon oracle-load --engine oracle --db XEPDB1 \
      --user JFORM_DEV --dir ./pacchetto_writer --dry-run
  charon oracle-setup --zip ./instantclient-basiclite-linux.x64.zip

NOTE ORACLE:
  Il path puro-Rust di Oracle richiede l'Instant Client (solo librerie, no-admin).
  Scaricalo UNA volta per il tuo OS/arch e aggancialo con 'oracle-setup' (scompatta
  lo .zip in una cartella utente); poi 'test'/'clone'/'oracle-load' lo useranno.
  In alternativa punta a una cartella con la variabile CHARON_ORACLE_CLIENT.
"#;

fn main() {
    // Se è configurato un Oracle Instant Client, assicura che il loader dinamico
    // lo trovi (può rilanciare il processo una volta). No-op se non serve.
    charon_core::oracle::ensure_client_env();

    let raw: Vec<String> = std::env::args().skip(1).collect();
    let a = Args::parse(&raw);
    let cmd = a.positionals.first().cloned().unwrap_or_else(|| "help".into());

    if a.has("help") || a.has("h") || cmd == "help" {
        print!("{HELP}");
        return;
    }

    let prefer = parse_prefer(&a.get_or("prefer", "auto"));
    let dry = a.has("dry-run");

    let code = match cmd.as_str() {
        "tools" => {
            print_tools();
            0
        }
        "test" => {
            let conn = build_conn(&a, "", "CHARON_");
            report(&ops::test_connection(&conn, prefer))
        }
        "dump" => {
            let conn = build_conn(&a, "", "CHARON_");
            let out = require(&a, "out", "dump");
            report(&ops::dump(&conn, out, prefer, dry))
        }
        "import" => {
            let conn = build_conn(&a, "", "CHARON_");
            let input = require(&a, "in", "import");
            report(&ops::import(&conn, input, prefer, dry))
        }
        "clone" => {
            let src = build_conn(&a, "", "CHARON_");
            let dst = build_conn(&a, "dst-", "CHARON_DST_");
            let opts = CloneOptions {
                data_only: a.has("data-only"),
                mask: parse_masks(&a.all("mask")),
            };
            report(&ops::clone(&src, &dst, prefer, &opts, dry))
        }
        "oracle-load" => {
            let mut conn = build_conn(&a, "", "CHARON_");
            conn.engine = Engine::Oracle; // il comando è specifico di Oracle
            let dir = require(&a, "dir", "oracle-load");
            report(&ops::oracle_load(&conn, dir, dry))
        }
        "oracle-setup" => match a.get("zip").or_else(|| a.get("dir")) {
            Some(path) => report(&ops::oracle_setup(path)),
            None => {
                eprintln!("uso: charon oracle-setup --zip <file.zip> | --dir <cartella>");
                2
            }
        },
        other => {
            eprintln!("comando sconosciuto: '{other}'\n");
            print!("{HELP}");
            2
        }
    };
    exit(code);
}

fn print_tools() {
    let reports = ops::detect_all();
    // Piccola tabella leggibile, senza dipendenze esterne.
    for r in &reports {
        println!("\n=== {} ===", r.label);
        println!(
            "  metodi: nativo={} · puro-Rust={}",
            yesno(r.native_available),
            yesno(r.rust_available)
        );
        for t in &r.tools {
            let where_ = t.path.clone().unwrap_or_else(|| "—".into());
            println!("  [{}] {:<16} {}  ({})", yesno(t.found), t.name, where_, t.purpose);
        }
        println!("  nota: {}", r.note);
        for h in &r.hints {
            println!("  ⚠ {}: {}", h.title, h.body);
            if let Some(cmd) = &h.command {
                for line in cmd.lines() {
                    println!("      {line}");
                }
            }
        }
    }
}

fn yesno(b: bool) -> &'static str {
    if b {
        "sì"
    } else {
        "no"
    }
}

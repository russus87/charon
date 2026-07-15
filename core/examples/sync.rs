//! Piccolo harness CLI per esercitare charon-core da riga di comando.
//! Utile per dogfooding del clone e primo abbozzo della futura CLI.
//!
//! Le connessioni si passano via variabili d'ambiente (così le password non
//! finiscono su file né negli argomenti):
//!   SRC_HOST SRC_PORT SRC_DB SRC_USER SRC_PASS   (sorgente)
//!   DST_HOST DST_PORT DST_DB DST_USER DST_PASS   (destinazione)
//!
//! Uso:
//!   cargo run -p charon-core --example sync -- test-src
//!   cargo run -p charon-core --example sync -- test-dst
//!   cargo run -p charon-core --example sync -- dump-src /tmp/out.sql
//!   cargo run -p charon-core --example sync -- clone
//!
//! Il metodo è sempre Prefer::Auto (nativo se presente, altrimenti puro Rust).

use charon_core::model::{
    CloneOptions, Connection, Engine, MaskRule, MaskStrategy, Prefer, SshAuth, SshTunnel,
};
use charon_core::ops;

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn opt_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

/// Costruisce il tunnel SSH da {prefix}_SSH_HOST/PORT/USER + PASS oppure KEY.
fn ssh(prefix: &str) -> Option<SshTunnel> {
    let host = opt_env(&format!("{prefix}_SSH_HOST"))?;
    let auth = if let Some(key) = opt_env(&format!("{prefix}_SSH_KEY")) {
        SshAuth::Key {
            path: key,
            passphrase: env(&format!("{prefix}_SSH_KEYPASS"), ""),
        }
    } else {
        SshAuth::Password {
            password: env(&format!("{prefix}_SSH_PASS"), ""),
        }
    };
    Some(SshTunnel {
        host,
        port: env(&format!("{prefix}_SSH_PORT"), "22").parse().unwrap_or(22),
        user: env(&format!("{prefix}_SSH_USER"), "root"),
        auth,
    })
}

fn conn(prefix: &str) -> Connection {
    Connection {
        engine: Engine::Postgres,
        host: env(&format!("{prefix}_HOST"), "localhost"),
        port: env(&format!("{prefix}_PORT"), "5432").parse().unwrap_or(5432),
        database: env(&format!("{prefix}_DB"), "postgres"),
        user: env(&format!("{prefix}_USER"), "postgres"),
        password: env(&format!("{prefix}_PASS"), ""),
        ssh: ssh(prefix),
    }
}

/// Parsa "tab.col=strategia,tab2.col2=fixed:VAL" in regole di mascheramento.
fn parse_mask(spec: &str) -> Vec<MaskRule> {
    let mut rules = Vec::new();
    for entry in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (target, strat) = match entry.split_once('=') {
            Some(x) => x,
            None => continue,
        };
        let (table, column) = match target.split_once('.') {
            Some(x) => x,
            None => continue,
        };
        let strategy = match strat {
            "null" => MaskStrategy::Null,
            "hash" => MaskStrategy::Hash,
            "email" => MaskStrategy::Email,
            "redact" => MaskStrategy::Redact,
            other => match other.strip_prefix("fixed:") {
                Some(v) => MaskStrategy::Fixed { value: v.to_string() },
                None => continue,
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

fn print_result(title: &str, r: &charon_core::model::OpResult) {
    println!("\n=== {title} ===");
    println!("ok: {}   metodo: {}", r.ok, r.method.label());
    println!("messaggio: {}", r.message);
    if !r.log.is_empty() {
        println!("--- log ---");
        for line in &r.log {
            println!("  {line}");
        }
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    match mode.as_str() {
        "test-src" => {
            let r = ops::test_connection(&conn("SRC"), Prefer::Auto);
            print_result("TEST sorgente", &r);
            std::process::exit(if r.ok { 0 } else { 1 });
        }
        "test-dst" => {
            let r = ops::test_connection(&conn("DST"), Prefer::Auto);
            print_result("TEST destinazione", &r);
            std::process::exit(if r.ok { 0 } else { 1 });
        }
        "dump-src" => {
            let out = std::env::args().nth(2).expect("uso: dump-src <file>");
            let dry = env("DRY_RUN", "") == "1";
            let r = ops::dump(&conn("SRC"), &out, Prefer::Auto, dry);
            print_result("DUMP sorgente", &r);
            std::process::exit(if r.ok { 0 } else { 1 });
        }
        "clone" => {
            // Opzioni via env:
            //   DATA_ONLY=1                          → preserva lo schema destinazione
            //   MASK="tab.col=hash,tab.col=email"    → regole di mascheramento
            //        strategie: null | hash | email | redact | fixed:VALORE
            let opts = CloneOptions {
                data_only: env("DATA_ONLY", "") == "1",
                mask: parse_mask(&env("MASK", "")),
            };
            let dry = env("DRY_RUN", "") == "1";
            let r = ops::clone(&conn("SRC"), &conn("DST"), Prefer::Auto, &opts, dry);
            print_result("CLONE sorgente → destinazione", &r);
            std::process::exit(if r.ok { 0 } else { 1 });
        }
        other => {
            eprintln!("modo sconosciuto: '{other}'. Usa: test-src | test-dst | dump-src <file> | clone");
            std::process::exit(2);
        }
    }
}

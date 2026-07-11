//! Rilevamento dei tool nativi nel PATH ed esecuzione di comandi esterni.

use crate::{Error, Result};
use std::path::PathBuf;
use std::process::Command;

/// Cerca un eseguibile nel PATH (aggiunge `.exe` su Windows).
pub fn find_tool(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(&exe);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// `true` se il tool e' presente nel PATH.
pub fn has_tool(name: &str) -> bool {
    find_tool(name).is_some()
}

/// Esito grezzo di un comando esterno.
pub struct CmdOutcome {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Esegue un comando, accodando una descrizione e l'output al log.
///
/// `display` e' la riga "umana" da mostrare in UI: NON deve contenere la
/// password (la costruisce il chiamante senza segreti).
pub fn run(log: &mut Vec<String>, display: &str, cmd: &mut Command) -> Result<CmdOutcome> {
    log.push(format!("$ {display}"));
    let out = cmd
        .output()
        .map_err(|e| Error::Cmd(format!("{display}: {e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    for line in stdout.lines().take(300) {
        log.push(line.to_string());
    }
    for line in stderr.lines().take(300) {
        log.push(line.to_string());
    }
    Ok(CmdOutcome {
        success: out.status.success(),
        stdout,
        stderr,
    })
}

//! Backend Tauri di Charon: espone alla UI i comandi del crate `charon-core`.
//!
//! Ogni comando e' un sottile adattatore: riceve i parametri dalla UI, chiama
//! l'orchestratore del core e restituisce un `OpResult` (gia' serializzabile),
//! che riporta SEMPRE quale metodo (nativo o puro Rust) e' stato usato.

use charon_core::model::{Connection, EngineReport, OpResult, Prefer};
use charon_core::ops;

/// Elenco, per ogni motore, dei tool nativi trovati e dei metodi disponibili.
#[tauri::command]
fn detect_tools() -> Vec<EngineReport> {
    ops::detect_all()
}

/// Verifica la connessione al database.
#[tauri::command]
fn test_connection(conn: Connection, prefer: Prefer) -> OpResult {
    ops::test_connection(&conn, prefer)
}

/// Crea un dump del database nel percorso `out`.
#[tauri::command]
fn dump_database(conn: Connection, out: String, prefer: Prefer) -> OpResult {
    ops::dump(&conn, &out, prefer)
}

/// Importa un dump `input` nel database.
#[tauri::command]
fn import_dump(conn: Connection, input: String, prefer: Prefer) -> OpResult {
    ops::import(&conn, &input, prefer)
}

/// Clona il database `source` su `target`.
#[tauri::command]
fn clone_database(source: Connection, target: Connection, prefer: Prefer) -> OpResult {
    ops::clone(&source, &target, prefer)
}

/// Punto di ingresso dell'app Tauri.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            detect_tools,
            test_connection,
            dump_database,
            import_dump,
            clone_database,
        ])
        .run(tauri::generate_context!())
        .expect("errore irreversibile all'avvio di Charon");
}

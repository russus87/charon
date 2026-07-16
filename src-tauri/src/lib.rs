//! Backend Tauri di Charon: espone alla UI i comandi del crate `charon-core`.
//!
//! Ogni comando e' un sottile adattatore: riceve i parametri dalla UI, chiama
//! l'orchestratore del core e restituisce un `OpResult` (gia' serializzabile),
//! che riporta SEMPRE quale metodo (nativo o puro Rust) e' stato usato.

use charon_core::connections::{self, ConnectionProfile};
use charon_core::model::{CloneOptions, Connection, EngineReport, Method, OpResult, Prefer};
use charon_core::ops;
use tauri::{AppHandle, Emitter};

/// Nome dell'evento con cui il backend invia le righe di avanzamento alla UI.
const PROGRESS_EVENT: &str = "charon://progress";

/// Esegue un'operazione BLOCCANTE del core fuori dal main thread (così la webview
/// resta reattiva) e inoltra l'**avanzamento live** alla UI: imposta un sink che
/// emette ogni riga come evento `charon://progress`, poi lo rimuove.
/// I comandi sono `async`: senza questo, Tauri eseguirebbe il lavoro sul thread
/// principale bloccando l'interfaccia fino al termine.
async fn run_blocking<F>(app: AppHandle, f: F) -> OpResult
where
    F: FnOnce() -> OpResult + Send + 'static,
{
    let result = tauri::async_runtime::spawn_blocking(move || {
        let sink_app = app.clone();
        charon_core::progress::set_sink(Some(Box::new(move |line: &str| {
            let _ = sink_app.emit(PROGRESS_EVENT, line.to_string());
        })));
        let res = f();
        charon_core::progress::set_sink(None); // ripulisce il sink del thread
        res
    })
    .await;
    match result {
        Ok(res) => res,
        Err(e) => OpResult {
            ok: false,
            method: Method::Native,
            message: format!("operazione interrotta: {e}"),
            artifact: None,
            log: Vec::new(),
        },
    }
}

/// Elenco, per ogni motore, dei tool nativi trovati e dei metodi disponibili.
#[tauri::command]
fn detect_tools() -> Vec<EngineReport> {
    ops::detect_all()
}

/// Verifica la connessione al database.
#[tauri::command]
async fn test_connection(app: AppHandle, conn: Connection, prefer: Prefer) -> OpResult {
    run_blocking(app, move || ops::test_connection(&conn, prefer)).await
}

/// Crea un dump del database nel percorso `out`. Con `dry_run` mostra solo il piano.
#[tauri::command]
async fn dump_database(
    app: AppHandle,
    conn: Connection,
    out: String,
    prefer: Prefer,
    dry_run: bool,
) -> OpResult {
    run_blocking(app, move || ops::dump(&conn, &out, prefer, dry_run)).await
}

/// Importa un dump `input` nel database. Con `dry_run` mostra solo il piano.
#[tauri::command]
async fn import_dump(
    app: AppHandle,
    conn: Connection,
    input: String,
    prefer: Prefer,
    dry_run: bool,
) -> OpResult {
    run_blocking(app, move || ops::import(&conn, &input, prefer, dry_run)).await
}

/// Clona il database `source` su `target` con le opzioni date (data-only, masking).
/// Con `dry_run` ispeziona la sorgente e mostra il piano senza toccare la destinazione.
#[tauri::command]
async fn clone_database(
    app: AppHandle,
    source: Connection,
    target: Connection,
    prefer: Prefer,
    options: CloneOptions,
    dry_run: bool,
) -> OpResult {
    run_blocking(app, move || ops::clone(&source, &target, prefer, &options, dry_run)).await
}

/// Importa un pacchetto SQL*Loader (cartella .ctl/.ldr) in Oracle via `sqlldr`,
/// riallineando le sequenze al termine. Con `dry_run` stampa solo i comandi.
#[tauri::command]
async fn oracle_load(app: AppHandle, conn: Connection, package_dir: String, dry_run: bool) -> OpResult {
    run_blocking(app, move || ops::oracle_load(&conn, &package_dir, dry_run)).await
}

/// Scompatta e aggancia l'Oracle Instant Client da uno `.zip` (o cartella).
#[tauri::command]
async fn oracle_setup(app: AppHandle, path: String) -> OpResult {
    run_blocking(app, move || ops::oracle_setup(&path)).await
}

/// Elenco delle connessioni salvate.
#[tauri::command]
fn list_connections() -> Vec<ConnectionProfile> {
    connections::list()
}

/// Inserisce/aggiorna una connessione salvata; ritorna la lista aggiornata.
#[tauri::command]
fn save_connection(profile: ConnectionProfile) -> std::result::Result<Vec<ConnectionProfile>, String> {
    connections::save(profile).map_err(|e| e.to_string())
}

/// Elimina una connessione salvata; ritorna la lista aggiornata.
#[tauri::command]
fn delete_connection(id: String) -> std::result::Result<Vec<ConnectionProfile>, String> {
    connections::delete(&id).map_err(|e| e.to_string())
}

/// Punto di ingresso dell'app Tauri.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Se è configurato un Oracle Instant Client, assicura che il loader dinamico
    // lo trovi (può rilanciare il processo una volta, prima di avviare la UI).
    charon_core::oracle::ensure_client_env();

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
            oracle_load,
            oracle_setup,
            list_connections,
            save_connection,
            delete_connection,
        ])
        .run(tauri::generate_context!())
        .expect("errore irreversibile all'avvio di Charon");
}

//! Canale di **avanzamento**: le operazioni emettono righe man mano che
//! procedono, così una UI (Tauri) può mostrarle in tempo reale invece di
//! aspettare la fine. Il sink è **per-thread**: ogni operazione (che gira su un
//! proprio thread, vedi `spawn_blocking` lato Tauri) ha il suo, senza mescolarsi
//! con le altre. Se nessun sink è impostato, `emit`/`note` non fanno nulla: il
//! core resta utilizzabile identico da CLI e test.

use std::cell::RefCell;

thread_local! {
    static SINK: RefCell<Option<Box<dyn FnMut(&str)>>> = const { RefCell::new(None) };
    static PROGRESS: RefCell<Option<Box<dyn FnMut(u64, u64)>>> = const { RefCell::new(None) };
}

/// Imposta (o azzera con `None`) il sink di avanzamento per il thread corrente.
/// Va impostato all'inizio dell'operazione e rimosso alla fine.
pub fn set_sink(sink: Option<Box<dyn FnMut(&str)>>) {
    SINK.with(|s| *s.borrow_mut() = sink);
}

/// Imposta (o azzera) il sink di avanzamento **strutturato** (done/total) per il
/// thread corrente: alimenta una barra a percentuale, in parallelo alle righe di
/// testo. No-op se non impostato, così CLI e test restano identici.
pub fn set_progress_sink(sink: Option<Box<dyn FnMut(u64, u64)>>) {
    PROGRESS.with(|s| *s.borrow_mut() = sink);
}

/// Segnala l'avanzamento a `done` passi su `total` (no-op se nessun sink).
/// L'unità è per-operazione (di norma le tabelle travasate); `total` = 0 va
/// interpretato dalla UI come "indeterminato".
pub fn report(done: u64, total: u64) {
    PROGRESS.with(|s| {
        if let Some(f) = s.borrow_mut().as_mut() {
            f(done, total);
        }
    });
}

/// Invia una riga di avanzamento al sink corrente (no-op se non impostato).
pub fn emit(line: &str) {
    SINK.with(|s| {
        if let Some(f) = s.borrow_mut().as_mut() {
            f(line);
        }
    });
}

/// Scrive una riga **sia** nel log persistente dell'operazione **sia** sul canale
/// di avanzamento live. Usata al posto di `log.push(...)` nei punti "interessanti"
/// (es. per-tabella), così l'utente li vede scorrere durante il lavoro.
pub fn note(log: &mut Vec<String>, line: impl Into<String>) {
    let s = line.into();
    emit(&s);
    log.push(s);
}

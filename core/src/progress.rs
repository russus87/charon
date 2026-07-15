//! Canale di **avanzamento**: le operazioni emettono righe man mano che
//! procedono, così una UI (Tauri) può mostrarle in tempo reale invece di
//! aspettare la fine. Il sink è **per-thread**: ogni operazione (che gira su un
//! proprio thread, vedi `spawn_blocking` lato Tauri) ha il suo, senza mescolarsi
//! con le altre. Se nessun sink è impostato, `emit`/`note` non fanno nulla: il
//! core resta utilizzabile identico da CLI e test.

use std::cell::RefCell;

thread_local! {
    static SINK: RefCell<Option<Box<dyn FnMut(&str)>>> = const { RefCell::new(None) };
}

/// Imposta (o azzera con `None`) il sink di avanzamento per il thread corrente.
/// Va impostato all'inizio dell'operazione e rimosso alla fine.
pub fn set_sink(sink: Option<Box<dyn FnMut(&str)>>) {
    SINK.with(|s| *s.borrow_mut() = sink);
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

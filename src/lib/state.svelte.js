// Stato globale dell'app (runes Svelte 5). Un solo oggetto reattivo condiviso.
import { listen } from "@tauri-apps/api/event";
import {
  detectTools,
  testConnection,
  dumpDatabase,
  importDump,
  cloneDatabase,
  compareDatabases,
  syncApply,
  listConnections,
  saveConnection,
  deleteConnection,
} from "./api.js";

// Porte di default per motore. SQLite è un file: nessuna porta (0).
export const PORTS = { postgres: 5432, mysql: 3306, oracle: 1521, sqlserver: 1433, sqlite: 0 };

// Motori che sono un file locale invece di un server: per questi il campo
// `database` contiene il percorso del file e host/utente/password non servono.
export const FILE_ENGINES = ["sqlite"];
export const isFileEngine = (e) => FILE_ENGINES.includes(e);

export function blankConn(engine = "postgres") {
  return {
    engine,
    host: isFileEngine(engine) ? "" : "localhost",
    port: PORTS[engine],
    database: "",
    user: "",
    password: "",
    ssh: null, // tunnel SSH opzionale (vedi toggleSsh)
  };
}

// Attiva/disattiva il tunnel SSH su una connessione. L'oggetto auth tiene tutti
// i campi: il backend (serde) ignora quelli non pertinenti alla strategia scelta.
export function toggleSsh(conn) {
  conn.ssh = conn.ssh
    ? null
    : {
        host: "",
        port: 22,
        user: "",
        auth: { kind: "password", password: "", path: "", passphrase: "" },
      };
}

export const app = $state({
  view: "connection", // connection | dump | import | clone | tools
  reports: [], // EngineReport[] dei tool rilevati
  connections: [], // ConnectionProfile[] salvate ({id, name, connection})
  editing: null, // profilo in modifica nella vista Connessioni (o null)
  // Connessione selezionata per ciascuna operazione (id del profilo).
  sel: { dump: null, import: null, cloneSrc: null, cloneDst: null, cmpSrc: null, cmpDst: null },
  theme: "auto", // 'auto' | 'light' | 'dark' (vedi initTheme/cycleTheme)
  diff: null, // ultimo DbDiff del confronto (o null)
  diffErr: null, // errore del confronto, se fallito
  comparing: false, // confronto in corso
  prefer: "auto", // auto | native | rust
  dryRun: false, // anteprima: non modifica nulla, mostra solo il piano
  dumpPath: "", // file di destinazione del dump
  importPath: "", // file da importare
  busy: false, // operazione in corso
  liveLog: [], // righe di avanzamento in tempo reale (evento charon://progress)
  result: null, // ultimo OpResult
  showLog: false, // console (log dettagliato) espansa: l'utente la apre se serve
  resultModal: false, // popup di riepilogo a fine operazione
  lastOp: null, // 'dump' | 'import' | 'clone': quale operazione ha prodotto result
  confirm: null, // popup di conferma PRIMA di un'operazione (o null)
  // Opzioni del clone. dataOnly: preserva lo schema destinazione (TRUNCATE+dati).
  // mask: regole {table, column, kind, value} (value solo per kind='fixed').
  cloneOpts: { dataOnly: false, mask: [] },
});

// ------------------------------------------------------------- connessioni ---

const ENGINE_LABELS = {
  postgres: "PostgreSQL",
  mysql: "MySQL",
  oracle: "Oracle",
  sqlserver: "SQL Server",
  sqlite: "SQLite",
};
export const engineLabel = (e) => ENGINE_LABELS[e] ?? e;

// Genera un id stabile per un nuovo profilo (fallback se randomUUID non c'è).
function newId() {
  if (typeof crypto !== "undefined" && crypto.randomUUID) return crypto.randomUUID();
  return `c_${Date.now().toString(16)}${Math.random().toString(16).slice(2, 8)}`;
}

// Se la connessione in modifica non ha ancora un nome, propone quello del file
// (senza estensione): per un motore su file è l'etichetta naturale, ed evita di
// ritrovarsi il pulsante Salva disabilitato senza capire perché.
export function suggestNameFromFile(path) {
  if (!app.editing || app.editing.name.trim()) return;
  const base = String(path).split(/[\\/]/).pop() ?? "";
  const name = base.replace(/\.(db|sqlite3?|db3)$/i, "");
  if (name) app.editing.name = name;
}

// Riepilogo leggibile di una connessione. Per i motori su file è il percorso:
// mostrare "utente@host:0/percorso" sarebbe fuorviante.
export function connTarget(c) {
  if (!c) return "";
  if (isFileEngine(c.engine)) return c.database || "(nessun file scelto)";
  const base = `${c.user ? c.user + "@" : ""}${c.host}:${c.port}/${c.database}`;
  return c.ssh ? `${base} (via SSH ${c.ssh.host})` : base;
}

// Ritorna l'oggetto Connection di un profilo dato il suo id (o null).
export function connById(id) {
  const p = app.connections.find((c) => c.id === id);
  return p ? p.connection : null;
}

// Carica le connessioni salvate e imposta selezioni di default sensate.
export async function loadConnections() {
  try {
    app.connections = await listConnections();
  } catch (e) {
    console.error(e);
    app.connections = [];
  }
  const first = app.connections[0]?.id ?? null;
  const valid = (id) => (app.connections.some((c) => c.id === id) ? id : first);
  for (const k of ["dump", "import", "cloneSrc", "cloneDst", "cmpSrc", "cmpDst"]) {
    app.sel[k] = valid(app.sel[k]);
  }
}

// ---------------------------------------------------------------- confronto ---

// Confronta i due database selezionati. Sola lettura: non modifica nulla.
export async function runCompare() {
  const src = connById(app.sel.cmpSrc);
  const dst = connById(app.sel.cmpDst);
  if (!src || !dst) {
    app.diffErr = "Seleziona i due database da confrontare.";
    app.diff = null;
    return;
  }
  app.comparing = true;
  app.diff = null;
  app.diffErr = null;
  app.liveLog = [];
  try {
    app.diff = await compareDatabases($state.snapshot(src), $state.snapshot(dst));
  } catch (e) {
    app.diffErr = String(e);
  } finally {
    app.comparing = false;
  }
}

// I conteggi righe divergono? (solo se entrambi noti: null = non contabile)
export const rowsDiffer = (t) =>
  t.source_rows != null && t.target_rows != null && t.source_rows !== t.target_rows;

// Una tabella è allineata se ha schema uguale e stesso numero di righe.
export const tableAligned = (t) => t.status === "same" && !rowsDiffer(t);

// Applica l'allineamento (schema) alla destinazione. Passa dal popup di conferma.
const runSyncApply = () => {
  const src = connById(app.sel.cmpSrc);
  const dst = connById(app.sel.cmpDst);
  if (!src || !dst) return needConn("Seleziona i due database.", "sync");
  return withBusy(() => syncApply($state.snapshot(src), $state.snapshot(dst), false), "sync");
};

export function requestSyncApply() {
  const src = connById(app.sel.cmpSrc);
  const dst = connById(app.sel.cmpDst);
  if (!src || !dst) return needConn("Seleziona i due database.", "sync");
  app.confirm = {
    title: "Applicare l'allineamento?",
    cta: "Applica alla destinazione",
    danger: true,
    dry: false,
    rows: [
      { label: "Modello (sorgente)", value: `${engineLabel(src.engine)} · ${connTarget(src)}` },
      { label: "Verrà modificata", value: `${engineLabel(dst.engine)} · ${connTarget(dst)}`, danger: true },
    ],
    notes: [
      "Esegue le DDL di allineamento SULLA destinazione, statement per statement.",
      "Le righe DROP possono perdere dati: rileggi lo script generato prima di procedere.",
    ],
  };
  pendingRun = runSyncApply;
}

// Apre l'editor su una NUOVA connessione.
export function newConnection() {
  app.editing = { id: newId(), name: "", connection: blankConn(), isNew: true };
}

// Apre l'editor su una connessione esistente (copia profonda, per annullare).
export function editConnection(profile) {
  app.editing = structuredClone($state.snapshot(profile));
}

// Nome libero per una copia: "X (copia)", poi "X (copia 2)", "X (copia 3)"…
// Se il nome è già una copia, riparte dalla radice invece di annidare i suffissi.
function nextCopyName(base) {
  const root = (base || "").trim().replace(/\s*\(copia(\s+\d+)?\)$/, "") || "(senza nome)";
  const taken = new Set(app.connections.map((c) => c.name));
  let name = `${root} (copia)`;
  for (let n = 2; taken.has(name); n++) name = `${root} (copia ${n})`;
  return name;
}

// Trasforma l'editor corrente in una NUOVA connessione copia: stessi parametri,
// id e nome nuovi. L'originale non viene toccato — la copia diventa un profilo a
// sé solo quando si preme Salva. Comodo per puntare un DB di prova.
export function duplicateEditing() {
  const p = app.editing;
  if (!p) return;
  app.editing = {
    id: newId(),
    name: nextCopyName(p.name),
    connection: structuredClone($state.snapshot(p.connection)),
    isNew: true,
  };
}

export function cancelEdit() {
  app.editing = null;
}

// Salva il profilo in modifica; aggiorna la lista e chiude l'editor.
export async function saveEditing() {
  const p = app.editing;
  if (!p || !p.name.trim()) return;
  const profile = { id: p.id, name: p.name.trim(), connection: $state.snapshot(p.connection) };
  app.connections = await saveConnection(profile);
  if (!app.sel.dump) await loadConnections(); // primo salvataggio: seleziona default
  app.editing = null;
}

// Elimina il profilo in modifica; aggiorna la lista e chiude l'editor.
export async function deleteEditing() {
  const p = app.editing;
  if (!p) return;
  app.connections = await deleteConnection(p.id);
  // Ripulisce le selezioni che puntavano al profilo eliminato.
  const first = app.connections[0]?.id ?? null;
  for (const k of ["dump", "import", "cloneSrc", "cloneDst"]) {
    if (!app.connections.some((c) => c.id === app.sel[k])) app.sel[k] = first;
  }
  app.editing = null;
}

// ------------------------------------------------------------------ masking ---

// Strategie di mascheramento disponibili (kind = tag serde lato Rust).
export const MASK_KINDS = [
  { kind: "hash", label: "Hash (pseudonimo)" },
  { kind: "email", label: "Email fittizia" },
  { kind: "redact", label: "Offusca (asterischi)" },
  { kind: "null", label: "NULL" },
  { kind: "fixed", label: "Valore fisso" },
];

export function addMaskRule() {
  app.cloneOpts.mask.push({ table: "", column: "", kind: "hash", value: "" });
}

// Preset per i dati sensibili comuni: aggiungono una regola con la strategia
// già scelta e un nome-colonna tipico (da adeguare alla propria tabella).
export const MASK_PRESETS = [
  { label: "Email", column: "email", kind: "email" },
  { label: "Codice fiscale", column: "codice_fiscale", kind: "hash" },
  { label: "IBAN", column: "iban", kind: "redact" },
  { label: "Telefono", column: "telefono", kind: "redact" },
  { label: "Nome", column: "nome", kind: "hash" },
  { label: "Password", column: "password", kind: "null" },
];
export function addMaskPreset(p) {
  app.cloneOpts.mask.push({ table: "", column: p.column, kind: p.kind, value: "" });
}

export function removeMaskRule(i) {
  app.cloneOpts.mask.splice(i, 1);
}

// Costruisce l'oggetto CloneOptions come lo attende il backend Rust (serde).
function buildCloneOptions() {
  const mask = app.cloneOpts.mask
    .filter((m) => m.table.trim() && m.column.trim())
    .map((m) => ({
      table: m.table.trim(),
      column: m.column.trim(),
      strategy: m.kind === "fixed" ? { kind: "fixed", value: m.value } : { kind: m.kind },
    }));
  return { data_only: app.cloneOpts.dataOnly, mask };
}

// ------------------------------------------------------------------- tema ---

// 'auto' segue il sistema; 'light'/'dark' forzano. Persistito in localStorage.
const THEME_KEY = "charon-theme";
export function initTheme() {
  try {
    app.theme = localStorage.getItem(THEME_KEY) || "auto";
  } catch {
    app.theme = "auto";
  }
}
// Risolve 'auto' nella preferenza di sistema; ritorna 'light' o 'dark'.
export function resolvedTheme() {
  if (app.theme === "light" || app.theme === "dark") return app.theme;
  const dark = typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches;
  return dark ? "dark" : "light";
}
// Cicla auto → light → dark → auto.
export function cycleTheme() {
  const next = { auto: "light", light: "dark", dark: "auto" };
  app.theme = next[app.theme] ?? "auto";
  try {
    localStorage.setItem(THEME_KEY, app.theme);
  } catch {
    /* storage non disponibile: resta solo in memoria */
  }
}

// Cambia motore e adegua la porta di default. Passando a un motore su file
// azzeriamo i campi di rete: resterebbero valori senza senso in `connections.json`
// (e il tunnel SSH non si applica a un file locale).
export function setEngine(conn, engine) {
  conn.engine = engine;
  conn.port = PORTS[engine];
  if (isFileEngine(engine)) {
    conn.host = "";
    conn.user = "";
    conn.password = "";
    conn.ssh = null;
  } else if (!conn.host) {
    conn.host = "localhost";
  }
}

// Carica (una volta) il riepilogo dei tool installati.
export async function loadReports() {
  try {
    app.reports = await detectTools();
  } catch (e) {
    console.error(e);
  }
}

export function reportFor(engine) {
  return app.reports.find((r) => r.engine === engine);
}

// Si iscrive (una volta) all'avanzamento live emesso dal backend.
let progressReady = false;
export async function initProgress() {
  if (progressReady) return;
  progressReady = true;
  await listen("charon://progress", (e) => {
    if (app.busy) app.liveLog.push(String(e.payload));
  });
}

// Titoli del popup di riepilogo, per operazione ed esito.
export const OP_TITLES = {
  dump: { ok: "Dump completato", err: "Dump non riuscito" },
  import: { ok: "Import completato", err: "Import non riuscito" },
  clone: { ok: "Clonazione completata", err: "Clonazione non riuscita" },
  sync: { ok: "Allineamento applicato", err: "Allineamento non riuscito" },
};

export function closeResultModal() {
  app.resultModal = false;
}

// Chiude il popup e apre la console: "Mostra log" dal riepilogo.
export function openLogFromModal() {
  app.resultModal = false;
  app.showLog = true;
}

export function toggleLog() {
  app.showLog = !app.showLog;
}

// `op` (dump|import|clone) attiva il popup di riepilogo a fine operazione; le
// operazioni senza op (es. prova connessione) restano solo inline nella console.
async function withBusy(fn, op = null) {
  app.busy = true;
  app.result = null;
  app.liveLog = []; // azzera il log live a ogni nuova operazione
  app.lastOp = op;
  try {
    app.result = await fn();
  } catch (e) {
    app.result = { ok: false, method: "native", message: String(e), artifact: null, log: [] };
  } finally {
    app.busy = false;
    if (op) app.resultModal = true;
  }
}

// Risultato d'errore "connessione non selezionata", senza chiamare il backend.
// Passa anche dal popup: altrimenti l'utente non si accorge dell'errore.
function needConn(msg, op) {
  app.lastOp = op;
  app.result = { ok: false, method: "native", message: msg, artifact: null, log: [] };
  app.resultModal = true;
}

// Testa una connessione (oggetto Connection già risolto, es. dall'editor/picker).
export const runTest = (conn) => withBusy(() => testConnection($state.snapshot(conn), app.prefer));

export const runDump = () => {
  const c = connById(app.sel.dump);
  if (!c) return needConn("Seleziona una connessione salvata.", "dump");
  return withBusy(
    () => dumpDatabase($state.snapshot(c), app.dumpPath, app.prefer, app.dryRun),
    "dump",
  );
};

export const runImport = () => {
  const c = connById(app.sel.import);
  if (!c) return needConn("Seleziona una connessione salvata.", "import");
  return withBusy(
    () => importDump($state.snapshot(c), app.importPath, app.prefer, app.dryRun),
    "import",
  );
};

export const runClone = () => {
  const src = connById(app.sel.cloneSrc);
  const dst = connById(app.sel.cloneDst);
  if (!src || !dst) return needConn("Seleziona sorgente e destinazione.", "clone");
  return withBusy(
    () =>
      cloneDatabase(
        $state.snapshot(src),
        $state.snapshot(dst),
        app.prefer,
        buildCloneOptions(),
        app.dryRun,
      ),
    "clone",
  );
};

// -------------------------------------------------- conferma pre-operazione ---

const PREFER_LABELS = { auto: "Auto", native: "Tool nativi", rust: "Puro Rust" };
const preferLabel = (p) => PREFER_LABELS[p] ?? p;

// Operazione da eseguire quando l'utente conferma il popup di riepilogo.
let pendingRun = null;

export function cancelConfirm() {
  app.confirm = null;
  pendingRun = null;
}

export function confirmProceed() {
  const run = pendingRun;
  app.confirm = null;
  pendingRun = null;
  if (run) run();
}

// Dump: legge dal DB (sicuro) e scrive un file. Rischio basso.
export function requestDump() {
  const c = connById(app.sel.dump);
  if (!c) return needConn("Seleziona una connessione salvata.", "dump");
  app.confirm = {
    title: app.dryRun ? "Anteprima del dump" : "Confermi il dump?",
    cta: app.dryRun ? "Esegui anteprima" : "Crea dump",
    danger: false,
    dry: app.dryRun,
    rows: [
      { label: "Esporta da", value: `${engineLabel(c.engine)} · ${connTarget(c)}` },
      { label: "File di destinazione", value: app.dumpPath, mono: true },
      { label: "Metodo", value: preferLabel(app.prefer) },
    ],
    notes: app.dryRun
      ? ["Anteprima: nessun file verrà scritto."]
      : [
          "Il dump legge soltanto dal database: operazione sicura per la sorgente.",
          "Se il file esiste già verrà sovrascritto.",
        ],
  };
  pendingRun = runDump;
}

// Import: ESEGUE lo script sul database. Modifica dati → conferma "danger".
export function requestImport() {
  const c = connById(app.sel.import);
  if (!c) return needConn("Seleziona una connessione salvata.", "import");
  app.confirm = {
    title: app.dryRun ? "Anteprima dell'import" : "Confermi l'import?",
    cta: app.dryRun ? "Esegui anteprima" : "Importa dump",
    danger: !app.dryRun,
    dry: app.dryRun,
    rows: [
      { label: "Scrive su", value: `${engineLabel(c.engine)} · ${connTarget(c)}`, danger: !app.dryRun },
      { label: "File dump", value: app.importPath, mono: true },
      { label: "Metodo", value: preferLabel(app.prefer) },
    ],
    notes: app.dryRun
      ? ["Anteprima: nessuna modifica verrà applicata."]
      : ["L'import esegue lo script SUL database di destinazione, modificandolo."],
  };
  pendingRun = runImport;
}

// Clone: la destinazione viene modificata. Il testo si adatta a data-only,
// append (SQL Server) e masking, così sai esattamente cosa succederà.
export function requestClone() {
  const src = connById(app.sel.cloneSrc);
  const dst = connById(app.sel.cloneDst);
  if (!src || !dst) return needConn("Seleziona sorgente e destinazione.", "clone");
  const dataOnly = app.cloneOpts.dataOnly;
  const maskN = app.cloneOpts.mask.filter((m) => m.table.trim() && m.column.trim()).length;

  const notes = [];
  if (app.dryRun) {
    notes.push("Anteprima: la destinazione non verrà modificata.");
  } else if (dataOnly) {
    notes.push(
      src.engine === "sqlserver"
        ? "Solo dati (append): lo schema resta, i dati vengono AGGIUNTI a quelli esistenti."
        : "Solo dati: lo schema della destinazione resta, i dati vengono SOSTITUITI.",
    );
  } else {
    notes.push("La destinazione verrà RISCRITTA: le tabelle esistenti vengono ricreate.");
  }
  if (maskN) notes.push(`${maskN} regol${maskN === 1 ? "a" : "e"} di mascheramento (forza il metodo puro Rust).`);

  app.confirm = {
    title: app.dryRun ? "Anteprima della clonazione" : "Confermi la clonazione?",
    cta: app.dryRun ? "Esegui anteprima" : dataOnly ? "Sincronizza dati" : "Clona database",
    danger: !app.dryRun,
    dry: app.dryRun,
    rows: [
      { label: "Sorgente", value: `${engineLabel(src.engine)} · ${connTarget(src)}` },
      { label: "Destinazione", value: `${engineLabel(dst.engine)} · ${connTarget(dst)}`, danger: !app.dryRun },
      { label: "Modalità", value: dataOnly ? "solo dati" : "schema + dati" },
      { label: "Metodo", value: preferLabel(app.prefer) },
    ],
    notes,
  };
  pendingRun = runClone;
}

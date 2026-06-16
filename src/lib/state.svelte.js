// Stato globale dell'app (runes Svelte 5). Un solo oggetto reattivo condiviso.
import {
  detectTools,
  testConnection,
  dumpDatabase,
  importDump,
  cloneDatabase,
} from "./api.js";

// Porte di default per motore.
export const PORTS = { postgres: 5432, oracle: 1521, sqlserver: 1433 };

function blankConn(engine = "postgres") {
  return {
    engine,
    host: "localhost",
    port: PORTS[engine],
    database: "",
    user: "",
    password: "",
  };
}

export const app = $state({
  view: "connection", // connection | dump | import | clone | tools
  reports: [], // EngineReport[] dei tool rilevati
  conn: blankConn(), // connessione principale (dump/import + sorgente clone)
  target: blankConn(), // destinazione del clone
  prefer: "auto", // auto | native | rust
  dumpPath: "", // file di destinazione del dump
  importPath: "", // file da importare
  busy: false, // operazione in corso
  result: null, // ultimo OpResult
});

// Cambia motore e adegua la porta di default.
export function setEngine(conn, engine) {
  conn.engine = engine;
  conn.port = PORTS[engine];
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

async function withBusy(fn) {
  app.busy = true;
  app.result = null;
  try {
    app.result = await fn();
  } catch (e) {
    app.result = { ok: false, method: "native", message: String(e), artifact: null, log: [] };
  } finally {
    app.busy = false;
  }
}

export const runTest = (conn) => withBusy(() => testConnection(conn, app.prefer));
export const runDump = () =>
  withBusy(() => dumpDatabase(app.conn, app.dumpPath, app.prefer));
export const runImport = () =>
  withBusy(() => importDump(app.conn, app.importPath, app.prefer));
export const runClone = () =>
  withBusy(() => cloneDatabase(app.conn, app.target, app.prefer));

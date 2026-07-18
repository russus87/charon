// Sottile strato sopra ai comandi Tauri del backend Rust.
import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";

export const detectTools = () => invoke("detect_tools");

// Connessioni salvate (persistite dal backend in ~/.config/charon/connections.json).
export const listConnections = () => invoke("list_connections");
export const saveConnection = (profile) => invoke("save_connection", { profile });
export const deleteConnection = (id) => invoke("delete_connection", { id });

export const testConnection = (conn, prefer) =>
  invoke("test_connection", { conn, prefer });

export const dumpDatabase = (conn, out, prefer, dryRun) =>
  invoke("dump_database", { conn, out, prefer, dryRun });

export const importDump = (conn, input, prefer, dryRun) =>
  invoke("import_dump", { conn, input, prefer, dryRun });

export const cloneDatabase = (source, target, prefer, options, dryRun) =>
  invoke("clone_database", { source, target, prefer, options, dryRun });

// Import di un pacchetto SQL*Loader (.ctl/.ldr) in Oracle via sqlldr.
// Confronto fra due database (schema + conteggio righe). Sola lettura.
export const compareDatabases = (source, target) =>
  invoke("compare_databases", { source, target });

// Confronto DATI (riga per riga, per chiave) di una singola tabella.
export const compareTableData = (source, target, table) =>
  invoke("compare_table_data", { source, target, table });

// Esporta i dati di tutte le tabelle in una cartella (CSV o JSON).
export const exportData = (conn, outDir, format) =>
  invoke("export_data", { conn, outDir, format });

// Sceglie una cartella (per l'export dati, che scrive un file per tabella).
export const pickDirectory = () => open({ directory: true, multiple: false });

// Ri-esegue il confronto e ne scrive il report su file (format: "html" | "json").
export const exportDiff = (source, target, format, out) =>
  invoke("export_diff", { source, target, format, out });

// Genera lo script di allineamento (DDL) dal diff di schema. Sola lettura.
export const syncPlan = (source, target) => invoke("sync_plan", { source, target });

// Applica lo script di allineamento alla destinazione (dryRun = anteprima).
export const syncApply = (source, target, dryRun) =>
  invoke("sync_apply", { source, target, dryRun });

// Percorso di salvataggio per il report del confronto.
export const pickReportPath = (format) =>
  save({
    defaultPath: `compare.${format}`,
    filters: [{ name: format.toUpperCase(), extensions: [format] }],
  });

export const oracleLoad = (conn, packageDir, dryRun) =>
  invoke("oracle_load", { conn, packageDir, dryRun });

// Scompatta e aggancia l'Oracle Instant Client da uno .zip (o cartella).
export const oracleSetup = (path) => invoke("oracle_setup", { path });

// Dialogo per scegliere lo .zip dell'Instant Client.
export const pickOracleZip = () =>
  open({ multiple: false, filters: [{ name: "Instant Client (zip)", extensions: ["zip"] }] });

// Dialoghi nativi per scegliere i file di dump.
export const pickSavePath = (defaultName) =>
  save({ defaultPath: defaultName, filters: [{ name: "Dump SQL", extensions: ["sql", "dmp"] }] });

export const pickOpenPath = () =>
  open({ multiple: false, filters: [{ name: "Dump", extensions: ["sql", "dmp"] }] });

// Sceglie il file di un database SQLite (per SQLite la "connessione" è un file).
const SQLITE_FILTER = [{ name: "SQLite", extensions: ["db", "sqlite", "sqlite3", "db3"] }];

export const pickSqliteFile = () => open({ multiple: false, filters: SQLITE_FILTER });

// Sceglie il percorso di un database SQLite **nuovo**: il dialogo di apertura
// mostra solo file esistenti, quindi per crearne uno serve quello di salvataggio.
// Il file vero lo crea poi il clone/import (SQLite lo genera al primo accesso).
export const pickSqliteNewFile = () =>
  save({ defaultPath: "nuovo.db", filters: SQLITE_FILTER });

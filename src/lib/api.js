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

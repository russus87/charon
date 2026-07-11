// Sottile strato sopra ai comandi Tauri del backend Rust.
import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";

export const detectTools = () => invoke("detect_tools");

export const testConnection = (conn, prefer) =>
  invoke("test_connection", { conn, prefer });

export const dumpDatabase = (conn, out, prefer) =>
  invoke("dump_database", { conn, out, prefer });

export const importDump = (conn, input, prefer) =>
  invoke("import_dump", { conn, input, prefer });

export const cloneDatabase = (source, target, prefer, options) =>
  invoke("clone_database", { source, target, prefer, options });

// Dialoghi nativi per scegliere i file di dump.
export const pickSavePath = (defaultName) =>
  save({ defaultPath: defaultName, filters: [{ name: "Dump SQL", extensions: ["sql", "dmp"] }] });

export const pickOpenPath = () =>
  open({ multiple: false, filters: [{ name: "Dump", extensions: ["sql", "dmp"] }] });

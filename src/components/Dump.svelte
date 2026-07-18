<script>
  import { app, requestDump, connById } from "../lib/state.svelte.js";
  import { pickSavePath, pickDirectory, exportData } from "../lib/api.js";
  import ConnPicker from "./ConnPicker.svelte";
  import PreferPicker from "./PreferPicker.svelte";

  async function choose() {
    const def = `${connById(app.sel.dump)?.database || "dump"}.sql`;
    const p = await pickSavePath(def);
    if (p) app.dumpPath = p;
  }

  let ready = $derived(!!app.dumpPath && !!app.sel.dump);

  // --- Export dati CSV/JSON (un file per tabella in una cartella) ---
  let exportFormat = $state("csv");
  let exportDir = $state("");
  let exporting = $state(false);
  let exportMsg = $state(null); // { ok, text }
  let canExport = $derived(!!exportDir && !!app.sel.dump && !exporting);

  async function chooseDir() {
    const d = await pickDirectory();
    if (d) exportDir = d;
  }

  async function runExport() {
    const c = connById(app.sel.dump);
    if (!c) return;
    exporting = true;
    exportMsg = null;
    try {
      const files = await exportData($state.snapshot(c), exportDir, exportFormat);
      exportMsg = { ok: true, text: `${files.length} file ${exportFormat.toUpperCase()} scritti in ${exportDir}` };
    } catch (e) {
      exportMsg = { ok: false, text: String(e) };
    } finally {
      exporting = false;
    }
  }
</script>

<div class="workspace">
  <div class="left">
    <ConnPicker bind:selectedId={app.sel.dump} title="Database da esportare" />

    <div class="card box">
      <h3>Dump SQL su file</h3>
      <div class="filebox">
        <div class="path">{app.dumpPath || "Nessun file scelto"}</div>
        <button class="btn ghost" onclick={choose}>Scegli…</button>
      </div>

      <div class="actions-row">
        <PreferPicker />
        <button class="btn primary" disabled={app.busy || !ready} onclick={requestDump}>
          Crea dump
        </button>
      </div>
    </div>

    <div class="card box">
      <h3>Esporta dati (CSV / JSON)</h3>
      <p class="sub">Un file per tabella, con i soli dati — per analisi o scambio, non un dump ripristinabile.</p>

      <div class="fmt">
        <span class="lbl">Formato</span>
        <div class="seg">
          <button class="opt" class:on={exportFormat === "csv"} onclick={() => (exportFormat = "csv")}>CSV</button>
          <button class="opt" class:on={exportFormat === "json"} onclick={() => (exportFormat = "json")}>JSON</button>
        </div>
      </div>

      <div class="filebox">
        <div class="path">{exportDir || "Nessuna cartella scelta"}</div>
        <button class="btn ghost" onclick={chooseDir}>Cartella…</button>
      </div>

      {#if exportMsg}
        <p class="msg" class:err={!exportMsg.ok}>{exportMsg.text}</p>
      {/if}

      <div class="actions-row">
        <span class="hint">I file esistenti con lo stesso nome verranno sovrascritti.</span>
        <button class="btn primary" disabled={!canExport} onclick={runExport}>
          {exporting ? "Esportazione…" : "Esporta dati"}
        </button>
      </div>
    </div>
  </div>
</div>

<style>
  .box {
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h3 {
    margin: 0;
    font-size: 16px;
  }
  .sub {
    margin: -6px 0 0;
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .fmt {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .lbl {
    font-size: 13px;
    color: var(--text-dim);
    font-weight: 600;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .opt {
    border: 0;
    background: var(--surface);
    padding: 7px 16px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-dim);
    cursor: pointer;
  }
  .opt.on {
    background: var(--accent);
    color: #fff;
  }
  .actions-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .hint {
    font-size: 12px;
    color: var(--text-faint);
  }
  .msg {
    margin: 0;
    font-size: 13px;
    color: var(--ok, #1f8a4c);
  }
  .msg.err {
    color: var(--err);
  }
</style>

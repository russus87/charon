<script>
  import { app, runDump } from "../lib/state.svelte.js";
  import { pickSavePath } from "../lib/api.js";
  import ConnForm from "./ConnForm.svelte";
  import ResultPanel from "./ResultPanel.svelte";
  import PreferPicker from "./PreferPicker.svelte";

  async function choose() {
    const def = `${app.conn.database || "dump"}.sql`;
    const p = await pickSavePath(def);
    if (p) app.dumpPath = p;
  }

  let ready = $derived(!!app.dumpPath && !!app.conn.database);
</script>

<div class="workspace">
  <div class="left">
    <ConnForm conn={app.conn} title="Database da esportare" />

    <div class="card box">
      <h3>File di destinazione</h3>
      <div class="filebox">
        <div class="path">{app.dumpPath || "Nessun file scelto"}</div>
        <button class="btn ghost" onclick={choose}>Scegli…</button>
      </div>

      <div class="actions-row">
        <PreferPicker />
        <button class="btn primary" disabled={app.busy || !ready} onclick={runDump}>
          Crea dump
        </button>
      </div>
    </div>
  </div>

  <ResultPanel />
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
</style>

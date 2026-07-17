<script>
  import { app, requestDump, connById } from "../lib/state.svelte.js";
  import { pickSavePath } from "../lib/api.js";
  import ConnPicker from "./ConnPicker.svelte";
  import PreferPicker from "./PreferPicker.svelte";

  async function choose() {
    const def = `${connById(app.sel.dump)?.database || "dump"}.sql`;
    const p = await pickSavePath(def);
    if (p) app.dumpPath = p;
  }

  let ready = $derived(!!app.dumpPath && !!app.sel.dump);
</script>

<div class="workspace">
  <div class="left">
    <ConnPicker bind:selectedId={app.sel.dump} title="Database da esportare" />

    <div class="card box">
      <h3>File di destinazione</h3>
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
  .actions-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
</style>

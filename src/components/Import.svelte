<script>
  import { app, requestImport } from "../lib/state.svelte.js";
  import { pickOpenPath } from "../lib/api.js";
  import ConnPicker from "./ConnPicker.svelte";
  import PreferPicker from "./PreferPicker.svelte";

  async function choose() {
    const p = await pickOpenPath();
    if (p) app.importPath = p;
  }

  let ready = $derived(!!app.importPath && !!app.sel.import);
</script>

<div class="workspace">
  <div class="left">
    <ConnPicker bind:selectedId={app.sel.import} title="Database di destinazione" />

    <div class="card box">
      <h3>Dump da importare</h3>
      <div class="filebox">
        <div class="path">{app.importPath || "Nessun file scelto"}</div>
        <button class="btn ghost" onclick={choose}>Scegli…</button>
      </div>

      <p class="warn-note">
        ⚠️ L'import scrive sul database selezionato: assicurati che sia quello giusto.
      </p>

      <div class="actions-row">
        <PreferPicker />
        <button class="btn primary" disabled={app.busy || !ready} onclick={requestImport}>
          Importa dump
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
  .warn-note {
    margin: 0;
    font-size: 12.5px;
    color: var(--warn);
    background: var(--warn-soft);
    padding: 9px 12px;
    border-radius: 10px;
  }
</style>

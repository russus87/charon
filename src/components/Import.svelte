<script>
  import { app, runImport, runTest } from "../lib/state.svelte.js";
  import { pickOpenPath } from "../lib/api.js";
  import ConnForm from "./ConnForm.svelte";
  import ResultPanel from "./ResultPanel.svelte";
  import PreferPicker from "./PreferPicker.svelte";

  async function choose() {
    const p = await pickOpenPath();
    if (p) app.importPath = p;
  }

  let ready = $derived(!!app.importPath && !!app.conn.database);
</script>

<div class="workspace">
  <div class="left">
    <ConnForm conn={app.conn} title="Database di destinazione" />

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
        <div class="btns">
          <button class="btn ghost" disabled={app.busy || !app.conn.database}
                  onclick={() => runTest(app.conn)}>
            Prova connessione
          </button>
          <button class="btn primary" disabled={app.busy || !ready} onclick={runImport}>
            Importa dump
          </button>
        </div>
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
  .actions-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .btns {
    display: flex;
    gap: 10px;
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

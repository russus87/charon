<script>
  import {
    app,
    runTest,
    saveEditing,
    deleteEditing,
    cancelEdit,
  } from "../lib/state.svelte.js";
  import ConnForm from "./ConnForm.svelte";
  import ResultPanel from "./ResultPanel.svelte";
  import PreferPicker from "./PreferPicker.svelte";

  let e = $derived(app.editing);
  let canSave = $derived(!!e && e.name.trim().length > 0);
</script>

<div class="workspace">
  <div class="left">
    <button class="btn ghost sm back" onclick={cancelEdit}>← Tutte le connessioni</button>

    <div class="card name-card">
      <label class="name-field">
        <span>Nome connessione</span>
        <input bind:value={app.editing.name} placeholder="es. Prod RDS (Oracle)" autocomplete="off" />
      </label>
    </div>

    <ConnForm conn={app.editing.connection} title="Parametri" />

    <p class="warn-note">
      ⚠️ La password viene salvata <b>in chiaro</b> nel file di configurazione locale
      (<code>~/.config/charon/connections.json</code>).
    </p>

    <div class="card bar">
      <PreferPicker />
      <div class="btns">
        <button class="btn ghost" disabled={app.busy || !app.editing.connection.database}
                onclick={() => runTest(app.editing.connection)}>
          Prova connessione
        </button>
        {#if !app.editing.isNew}
          <button class="btn danger" disabled={app.busy} onclick={deleteEditing}>Elimina</button>
        {/if}
        <button class="btn primary" disabled={app.busy || !canSave} onclick={saveEditing}>
          Salva
        </button>
      </div>
    </div>
  </div>

  <ResultPanel />
</div>

<style>
  .back {
    align-self: flex-start;
  }
  .name-card {
    padding: 16px 18px;
  }
  .name-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .name-field span {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-dim);
  }
  .name-field input {
    border: 1px solid var(--border-strong);
    background: var(--surface);
    border-radius: var(--radius-sm);
    padding: 10px 12px;
    font-size: 14px;
    color: var(--text);
  }
  .name-field input:focus {
    outline: none;
    border-color: var(--green-500);
    box-shadow: 0 0 0 3px var(--green-100);
  }
  .warn-note {
    margin: 0;
    font-size: 12.5px;
    color: var(--warn);
    background: var(--warn-soft);
    padding: 9px 12px;
    border-radius: var(--radius-sm);
  }
  .warn-note code {
    font-family: var(--mono);
    font-size: 11.5px;
  }
  .bar {
    padding: 14px 16px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .btns {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
</style>

<script>
  import { app, engineLabel, connById, runTest, connTarget } from "../lib/state.svelte.js";

  let { selectedId = $bindable(), title = "Connessione" } = $props();

  let conn = $derived(connById(selectedId));
</script>

<div class="card picker">
  <div class="head">
    <h3>{title}</h3>
    {#if conn}
      <button class="btn ghost sm" disabled={app.busy} onclick={() => runTest(conn)}>Prova</button>
    {/if}
  </div>

  {#if app.connections.length === 0}
    <p class="hint">
      Nessuna connessione salvata. Vai in <b>Connessioni</b> per aggiungerne una.
    </p>
  {:else}
    <select class="sel" bind:value={selectedId}>
      {#each app.connections as c (c.id)}
        <option value={c.id}>{c.name || "(senza nome)"} · {engineLabel(c.connection.engine)}</option>
      {/each}
    </select>

    {#if conn}
      <div class="summary">
        <span class="badge brand">{engineLabel(conn.engine)}</span>
        <code>{connTarget(conn)}</code>
        {#if conn.ssh}<span class="badge grey">SSH</span>{/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .picker {
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  h3 {
    margin: 0;
    font-size: 16px;
  }
  .hint {
    margin: 0;
    font-size: 13px;
    color: var(--text-dim);
  }
  .sel {
    border: 1px solid var(--border-strong);
    background: var(--surface);
    border-radius: var(--radius-sm);
    padding: 10px 12px;
    font-size: 14px;
    color: var(--text);
  }
  .sel:focus {
    outline: none;
    border-color: var(--green-500);
    box-shadow: 0 0 0 3px var(--green-100);
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .summary code {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
</style>

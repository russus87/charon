<script>
  import { app, engineLabel, newConnection, editConnection } from "../lib/state.svelte.js";
  import ConnEditor from "./ConnEditor.svelte";

  // Riepilogo host per la card della lista.
  function target(c) {
    const s = c.connection;
    const base = `${s.user ? s.user + "@" : ""}${s.host}:${s.port}/${s.database}`;
    return s.ssh ? `${base} (via SSH ${s.ssh.host})` : base;
  }
</script>

{#if app.editing}
  <ConnEditor />
{:else}
  <div class="conns">
    <div class="head">
      <p class="lead">Le connessioni salvate qui sono riutilizzabili in Dump, Importa e Clona.</p>
      <button class="btn primary" onclick={newConnection}>+ Nuova connessione</button>
    </div>

    {#if app.connections.length === 0}
      <div class="empty card">
        <span class="em-icon">🔌</span>
        <p>Nessuna connessione salvata.</p>
        <button class="btn primary" onclick={newConnection}>Crea la prima connessione</button>
      </div>
    {:else}
      <div class="list">
        {#each app.connections as c (c.id)}
          <button class="conn card" onclick={() => editConnection(c)}>
            <div class="row1">
              <span class="name">{c.name || "(senza nome)"}</span>
              <span class="badge brand">{engineLabel(c.connection.engine)}</span>
              {#if c.connection.ssh}<span class="badge grey">SSH</span>{/if}
            </div>
            <code class="target">{target(c)}</code>
            <span class="edit">Modifica →</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .conns {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }
  .lead {
    margin: 0;
    color: var(--text-dim);
    font-size: 13.5px;
  }
  .list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 14px;
  }
  .conn {
    text-align: left;
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    cursor: pointer;
    transition: box-shadow 0.14s ease, border-color 0.14s ease, transform 0.05s ease;
  }
  .conn:hover {
    box-shadow: var(--shadow);
    border-color: var(--border-strong);
  }
  .conn:active {
    transform: translateY(1px);
  }
  .row1 {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .name {
    font-size: 15px;
    font-weight: 700;
    color: var(--text);
    margin-right: auto;
  }
  .target {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .edit {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--accent);
  }
  .empty {
    display: grid;
    place-items: center;
    gap: 12px;
    padding: 56px 20px;
    text-align: center;
    color: var(--text-dim);
  }
  .empty .em-icon {
    font-size: 34px;
  }
  .empty p {
    margin: 0;
  }
</style>

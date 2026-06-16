<script>
  import { setEngine } from "../lib/state.svelte.js";

  // `conn` e' l'oggetto reattivo dello stato: mutarne i campi aggiorna tutto.
  let { conn, title = "Connessione" } = $props();

  const engines = [
    { id: "postgres", label: "PostgreSQL" },
    { id: "oracle", label: "Oracle" },
    { id: "sqlserver", label: "SQL Server" },
  ];

  // Per Oracle il campo "database" e' il service name.
  let dbLabel = $derived(conn.engine === "oracle" ? "Service name" : "Database");
</script>

<div class="form card">
  <div class="head">
    <h3>{title}</h3>
    <div class="engines">
      {#each engines as e}
        <button
          class="eng"
          class:active={conn.engine === e.id}
          onclick={() => setEngine(conn, e.id)}
        >
          {e.label}
        </button>
      {/each}
    </div>
  </div>

  <div class="grid">
    <label class="wide">
      <span>Host</span>
      <input bind:value={conn.host} placeholder="localhost" autocomplete="off" />
    </label>
    <label>
      <span>Porta</span>
      <input type="number" bind:value={conn.port} />
    </label>
    <label>
      <span>{dbLabel}</span>
      <input bind:value={conn.database} placeholder={conn.engine === "oracle" ? "XEPDB1" : "miodb"} />
    </label>
    <label>
      <span>Utente</span>
      <input bind:value={conn.user} autocomplete="off" />
    </label>
    <label>
      <span>Password</span>
      <input type="password" bind:value={conn.password} autocomplete="off" />
    </label>
  </div>
</div>

<style>
  .form {
    padding: 20px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 16px;
    flex-wrap: wrap;
  }
  h3 {
    margin: 0;
    font-size: 16px;
  }
  .engines {
    display: flex;
    gap: 4px;
    background: var(--surface-2);
    padding: 4px;
    border-radius: 12px;
  }
  .eng {
    border: none;
    background: transparent;
    color: var(--ink-soft);
    font-size: 13px;
    font-weight: 600;
    padding: 7px 12px;
    border-radius: 9px;
    transition: all 0.15s ease;
  }
  .eng.active {
    background: var(--surface);
    color: var(--brand);
    box-shadow: var(--shadow-sm);
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  label.wide {
    grid-column: span 2;
  }
  label span {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--ink-soft);
  }
  input {
    border: 1px solid var(--stroke);
    background: var(--bg-2);
    border-radius: 10px;
    padding: 10px 12px;
    font-size: 14px;
    color: var(--ink);
    transition: border 0.15s ease, box-shadow 0.15s ease;
  }
  input:focus {
    outline: none;
    border-color: var(--brand);
    box-shadow: 0 0 0 3px var(--brand-soft);
  }
</style>

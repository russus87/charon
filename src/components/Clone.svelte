<script>
  import { app, runClone, setEngine } from "../lib/state.svelte.js";
  import ConnForm from "./ConnForm.svelte";
  import ResultPanel from "./ResultPanel.svelte";
  import PreferPicker from "./PreferPicker.svelte";

  // Sorgente e destinazione devono avere lo stesso motore: lo allineo.
  $effect(() => {
    if (app.target.engine !== app.conn.engine) {
      setEngine(app.target, app.conn.engine);
    }
  });

  let ready = $derived(!!app.conn.database && !!app.target.database);
</script>

<div class="workspace">
  <div class="left">
    <ConnForm conn={app.conn} title="Sorgente" />

    <div class="arrow">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 5v14M6 13l6 6 6-6" />
      </svg>
      <span>copia schema + dati</span>
    </div>

    <ConnForm conn={app.target} title="Destinazione" />

    <div class="card bar">
      <PreferPicker />
      <button class="btn primary" disabled={app.busy || !ready} onclick={runClone}>
        Clona database
      </button>
    </div>
  </div>

  <ResultPanel />
</div>

<style>
  .arrow {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--brand);
    font-size: 12.5px;
    font-weight: 600;
  }
  .arrow svg {
    width: 20px;
    height: 20px;
  }
  .bar {
    padding: 14px 16px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
</style>

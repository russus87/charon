<script>
  import { app, runTest, reportFor } from "../lib/state.svelte.js";
  import ConnForm from "./ConnForm.svelte";
  import ResultPanel from "./ResultPanel.svelte";
  import PreferPicker from "./PreferPicker.svelte";

  let report = $derived(reportFor(app.conn.engine));
</script>

<div class="workspace">
  <div class="left">
    <ConnForm conn={app.conn} title="Connessione al database" />

    {#if report}
      <div class="hint card">
        <span class="badge {report.native_available ? 'ok' : report.rust_available ? 'warn' : 'err'}">
          {report.native_available
            ? "tool nativi pronti"
            : report.rust_available
              ? "userà il fallback puro Rust"
              : "nessun metodo"}
        </span>
        <p>{report.note}</p>
      </div>
    {/if}

    <div class="card bar">
      <PreferPicker />
      <button class="btn primary" disabled={app.busy} onclick={() => runTest(app.conn)}>
        Prova connessione
      </button>
    </div>
  </div>

  <ResultPanel />
</div>

<style>
  .hint {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .hint p {
    margin: 0;
    font-size: 13px;
    color: var(--ink-soft);
    line-height: 1.5;
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

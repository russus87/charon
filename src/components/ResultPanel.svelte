<script>
  import { app, toggleLog } from "../lib/state.svelte.js";

  // Pannello "console": esito + metodo usato dell'ultima operazione. Il log
  // dettagliato è collassato di default: l'utente lo apre solo se gli serve.
  let r = $derived(app.result);
  let methodLabel = $derived(
    r?.method === "rust" ? "puro Rust (best-effort)" : "tool nativi",
  );
  // Numero di righe di log disponibili, per invogliare l'apertura quando servono.
  let lines = $derived(app.busy ? app.liveLog.length : (r?.log?.length ?? 0));
</script>

<div class="panel card" class:collapsed={!app.showLog}>
  <div class="head">
    <h3>Console</h3>
    {#if r}
      <span class="badge {r.ok ? 'ok' : 'err'}">{r.ok ? "OK" : "Errore"}</span>
    {/if}
    <button class="btn ghost sm toggle" onclick={toggleLog} aria-expanded={app.showLog}>
      {app.showLog ? "Nascondi log ▾" : `Mostra log${lines ? ` (${lines})` : ""} ▸`}
    </button>
  </div>

  {#if r}
    <div class="meta">
      <span class="badge brand">metodo: {methodLabel}</span>
      {#if r.artifact}
        <span class="path" title={r.artifact}>📄 {r.artifact}</span>
      {/if}
    </div>
    <p class="msg" class:err={!r.ok}>{r.message}</p>
  {/if}

  <!-- Con la console chiusa serve comunque un segnale che qualcosa sta girando. -->
  {#if app.busy && !app.showLog}
    <div class="mini spinner">Operazione in corso…</div>
  {/if}

  {#if app.showLog}
    <div class="log">
      {#if app.busy}
        <div class="spinner">Operazione in corso…</div>
        {#each app.liveLog as line}
          <div class="line" class:cmd={line.startsWith("$")}>{line}</div>
        {/each}
      {:else if r && r.log && r.log.length}
        {#each r.log as line}
          <div class="line" class:cmd={line.startsWith("$")}>{line}</div>
        {/each}
      {:else}
        <div class="empty">
          Qui compariranno il metodo usato (nativo o puro Rust), i comandi eseguiti
          e l'output del database.
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .panel {
    padding: 18px;
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
  }
  /* Con il log chiuso il pannello non deve occupare tutta la colonna. */
  .panel.collapsed {
    height: auto;
    align-self: flex-start;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
  }
  h3 {
    margin: 0;
    font-size: 16px;
  }
  .toggle {
    margin-left: auto;
    flex: none;
  }
  .mini {
    font-size: 13px;
    color: var(--text-dim, var(--ink-soft));
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 8px;
  }
  .path {
    font-size: 12px;
    color: var(--ink-soft);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
  .msg {
    margin: 0 0 12px;
    font-size: 13.5px;
    color: var(--ink);
  }
  .msg.err {
    color: var(--err);
  }
  .log {
    flex: 1;
    min-height: 160px;
    overflow: auto;
    background: #0e1220;
    color: #cdd4e6;
    border-radius: 12px;
    padding: 14px;
    font-family: var(--mono);
    font-size: 12.5px;
    line-height: 1.6;
  }
  .line {
    white-space: pre-wrap;
    word-break: break-word;
  }
  .line.cmd {
    color: #8bd5a0;
  }
  .empty,
  .spinner {
    color: #6b7390;
    font-family: var(--font);
    font-size: 13px;
  }
  .spinner::before {
    content: "";
    display: inline-block;
    width: 10px;
    height: 10px;
    margin-right: 8px;
    border-radius: 50%;
    border: 2px solid #4b5575;
    border-top-color: #9aa6d8;
    animation: spin 0.7s linear infinite;
    vertical-align: middle;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>

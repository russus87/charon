<script>
  import { app, toggleLog } from "../lib/state.svelte.js";

  // Console ancorata in basso: barra sottile che si apre verso l'alto quando
  // serve. Il log dettagliato resta collassato di default.
  let r = $derived(app.result);
  let methodLabel = $derived(
    r?.method === "rust" ? "puro Rust (best-effort)" : "tool nativi",
  );
  let lines = $derived(app.busy ? app.liveLog.length : (r?.log?.length ?? 0));
</script>

<div class="console" class:open={app.showLog}>
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

  <div class="bar">
    <span class="title">Console</span>
    {#if r}
      <span class="badge {r.ok ? 'ok' : 'err'}">{r.ok ? "OK" : "Errore"}</span>
      <span class="badge brand">{methodLabel}</span>
      <span class="msg" class:err={!r.ok} title={r.message}>{r.message}</span>
    {:else if !app.busy}
      <span class="idle">Nessuna operazione ancora eseguita.</span>
    {/if}
    {#if app.busy}
      <span class="mini spinner">Operazione in corso…</span>
    {/if}
    <button class="btn ghost sm toggle" onclick={toggleLog} aria-expanded={app.showLog}>
      {app.showLog ? "Nascondi log ▾" : `Mostra log${lines ? ` (${lines})` : ""} ▴`}
    </button>
  </div>
</div>

<style>
  .console {
    position: fixed;
    left: var(--sidebar-w);
    right: 0;
    bottom: 0;
    z-index: 30;
    display: flex;
    flex-direction: column; /* log sopra, barra sotto → si apre verso l'alto */
    max-height: 72vh;
    background: var(--surface);
    border-top: 1px solid var(--border-strong);
    box-shadow: 0 -8px 26px rgba(16, 40, 30, 0.12);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 18px 10px 30px;
    flex: none;
  }
  .title {
    font-size: 14px;
    font-weight: 700;
    flex: none;
  }
  .msg {
    font-size: 13px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .msg.err {
    color: var(--err);
  }
  .idle {
    font-size: 12.5px;
    color: var(--text-faint);
  }
  .mini {
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .toggle {
    margin-left: auto;
    flex: none;
  }
  .log {
    flex: 1;
    min-height: 0;
    height: 46vh;
    overflow: auto;
    background: #0e1220;
    color: #cdd4e6;
    padding: 14px 30px;
    font-family: var(--mono);
    font-size: 12.5px;
    line-height: 1.6;
    border-bottom: 1px solid #1c2233;
  }
  .line {
    white-space: pre-wrap;
    word-break: break-word;
  }
  .line.cmd {
    color: #8bd5a0;
  }
  .empty {
    color: #6b7390;
    font-family: var(--font);
    font-size: 13px;
  }
  .spinner {
    color: #9aa6d8;
  }
  .mini.spinner {
    color: var(--text-dim);
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
  @media (prefers-reduced-motion: reduce) {
    .spinner::before {
      animation: none;
    }
  }
  @media (max-width: 980px) {
    .console {
      left: 0;
    }
  }
</style>

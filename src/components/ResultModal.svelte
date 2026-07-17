<script>
  import {
    app,
    OP_TITLES,
    closeResultModal,
    openLogFromModal,
  } from "../lib/state.svelte.js";

  // Popup di riepilogo mostrato a fine operazione (dump/import/clone).
  let r = $derived(app.result);
  let titles = $derived(OP_TITLES[app.lastOp] ?? { ok: "Operazione completata", err: "Operazione non riuscita" });
  let title = $derived(r?.ok ? titles.ok : titles.err);
  let methodLabel = $derived(r?.method === "rust" ? "puro Rust (best-effort)" : "tool nativi");

  // Il backend segnala i problemi non bloccanti con righe che iniziano per "⚠"
  // (es. un batch SQL non applicato): vale la pena evidenziarli nel riepilogo.
  let warnings = $derived((r?.log ?? []).filter((l) => l.trimStart().startsWith("⚠")));
  // Il dry-run è annotato nel log dall'orchestratore.
  let isDry = $derived((r?.log ?? []).some((l) => l.includes("DRY-RUN")));

  function onKeydown(e) {
    if (e.key === "Escape") closeResultModal();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if app.resultModal && r}
  <!-- Il backdrop chiude il popup; il click interno non deve propagarsi. -->
  <div
    class="backdrop"
    role="button"
    tabindex="-1"
    aria-label="Chiudi"
    onclick={closeResultModal}
    onkeydown={onKeydown}
  >
    <div
      class="modal card"
      role="dialog"
      aria-modal="true"
      aria-labelledby="res-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="top">
        <span class="icon {r.ok ? 'ok' : 'err'}">{r.ok ? "✓" : "✕"}</span>
        <div class="titles">
          <h2 id="res-title">{title}</h2>
          <div class="badges">
            <span class="badge brand">metodo: {methodLabel}</span>
            {#if isDry}<span class="badge grey">anteprima (dry-run)</span>{/if}
            {#if warnings.length}
              <span class="badge warn">{warnings.length} avvis{warnings.length === 1 ? "o" : "i"}</span>
            {/if}
          </div>
        </div>
      </div>

      <p class="msg" class:err={!r.ok}>{r.message}</p>

      {#if r.artifact}
        <p class="artifact" title={r.artifact}>📄 {r.artifact}</p>
      {/if}

      {#if warnings.length}
        <div class="warns">
          <p class="warns-head">
            Alcune istruzioni non sono state applicate — il resto sì:
          </p>
          <ul>
            {#each warnings.slice(0, 3) as w}
              <li>{w}</li>
            {/each}
          </ul>
          {#if warnings.length > 3}
            <p class="more">…e altri {warnings.length - 3}. Aprili nel log.</p>
          {/if}
        </div>
      {/if}

      <div class="actions">
        <button class="btn ghost" onclick={openLogFromModal}>Mostra log</button>
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn primary" autofocus onclick={closeResultModal}>OK</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(8, 10, 18, 0.55);
    backdrop-filter: blur(2px);
    display: grid;
    place-items: center;
    padding: 24px;
    z-index: 50;
    cursor: default;
    animation: fade 0.12s ease;
  }
  .modal {
    width: min(560px, 100%);
    max-height: 80vh;
    overflow: auto;
    padding: 22px 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    cursor: default;
    animation: pop 0.14s ease;
  }
  .top {
    display: flex;
    align-items: flex-start;
    gap: 14px;
  }
  .icon {
    flex: none;
    width: 38px;
    height: 38px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 19px;
    font-weight: 700;
  }
  .icon.ok {
    background: var(--green-100, #e6f6ec);
    color: var(--ok, #1f8a4c);
  }
  .icon.err {
    background: var(--err-soft, #fdeaea);
    color: var(--err, #c0392b);
  }
  .titles {
    min-width: 0;
  }
  h2 {
    margin: 0 0 6px;
    font-size: 18px;
    letter-spacing: -0.01em;
  }
  .badges {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .msg {
    margin: 0;
    font-size: 13.5px;
    color: var(--text, var(--ink));
    white-space: pre-wrap;
    word-break: break-word;
  }
  .msg.err {
    color: var(--err);
  }
  .artifact {
    margin: 0;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-dim, var(--ink-soft));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .warns {
    background: var(--warn-soft);
    border-radius: var(--radius-sm);
    padding: 10px 12px;
  }
  .warns-head {
    margin: 0 0 6px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--warn);
  }
  .warns ul {
    margin: 0;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .warns li {
    font-family: var(--mono);
    font-size: 11.5px;
    color: var(--text-dim, var(--ink-soft));
    word-break: break-word;
  }
  .more {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--text-dim, var(--ink-soft));
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.99);
    }
  }
</style>

<script>
  import { app, cancelConfirm, confirmProceed } from "../lib/state.svelte.js";

  // Popup di conferma mostrato PRIMA di dump/import/clone: riepiloga cosa sta
  // per succedere, così un'operazione distruttiva non parte per un click distratto.
  let c = $derived(app.confirm);

  function onKeydown(e) {
    if (!app.confirm) return;
    if (e.key === "Escape") cancelConfirm();
    // Enter conferma solo le operazioni non distruttive: per quelle danger
    // vogliamo un click esplicito, niente scorciatoie.
    else if (e.key === "Enter" && !c.danger) confirmProceed();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if c}
  <div
    class="backdrop"
    role="button"
    tabindex="-1"
    aria-label="Annulla"
    onclick={cancelConfirm}
    onkeydown={onKeydown}
  >
    <div
      class="modal card"
      class:danger={c.danger}
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="top">
        <span class="icon {c.danger ? 'danger' : 'safe'}">{c.danger ? "!" : "→"}</span>
        <h2 id="confirm-title">{c.title}</h2>
        {#if c.dry}<span class="badge grey">anteprima (dry-run)</span>{/if}
      </div>

      <dl class="rows">
        {#each c.rows as r}
          {#if r.value}
            <div class="row" class:danger={r.danger}>
              <dt>{r.label}</dt>
              <dd class:mono={r.mono} title={r.value}>{r.value}</dd>
            </div>
          {/if}
        {/each}
      </dl>

      {#if c.notes?.length}
        <ul class="notes" class:danger={c.danger}>
          {#each c.notes as n}
            <li>{n}</li>
          {/each}
        </ul>
      {/if}

      <div class="actions">
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn ghost" autofocus={c.danger} onclick={cancelConfirm}>Annulla</button>
        <button
          class="btn {c.danger ? 'danger-solid' : 'primary'}"
          autofocus={!c.danger}
          onclick={confirmProceed}
        >
          {c.cta}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(8, 10, 18, 0.5);
    backdrop-filter: blur(2px);
    display: grid;
    place-items: center;
    padding: 24px;
    z-index: 60;
    cursor: default;
    animation: fade 0.12s ease;
  }
  .modal {
    width: min(520px, 100%);
    max-height: 82vh;
    overflow: auto;
    padding: 22px 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    cursor: default;
    animation: pop 0.16s cubic-bezier(0.2, 0.9, 0.3, 1.2);
  }
  .modal.danger {
    border-color: #eecac6;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .icon {
    flex: none;
    width: 34px;
    height: 34px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 18px;
    font-weight: 800;
  }
  .icon.safe {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .icon.danger {
    background: var(--err-soft);
    color: var(--err);
  }
  h2 {
    margin: 0;
    font-size: 17px;
    margin-right: auto;
  }
  .rows {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    background: var(--border);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .row {
    display: grid;
    grid-template-columns: 130px 1fr;
    gap: 12px;
    padding: 9px 12px;
    background: var(--surface);
    align-items: baseline;
  }
  .row.danger {
    background: var(--err-soft);
  }
  dt {
    font-size: 12px;
    color: var(--text-dim);
    font-weight: 600;
  }
  dd {
    margin: 0;
    font-size: 13px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.danger dd {
    color: var(--err);
    font-weight: 600;
  }
  dd.mono {
    font-family: var(--mono);
    font-size: 11.5px;
  }
  .notes {
    margin: 0;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .notes li {
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .notes.danger li {
    color: var(--warn);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 2px;
  }
  /* Pulsante distruttivo pieno: più deciso del .danger (bordato) dell'editor. */
  .btn.danger-solid {
    background: var(--red);
    border-color: var(--red);
    color: #fff;
  }
  .btn.danger-solid:hover {
    background: #bf3a30;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .backdrop,
    .modal {
      animation: none;
    }
  }
</style>

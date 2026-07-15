<script>
  import { app } from "../lib/state.svelte.js";

  // Sceglie come eseguire: Auto (nativo se c'e', altrimenti Rust), o forzato.
  const opts = [
    { id: "auto", label: "Auto", hint: "Nativo se disponibile, altrimenti puro Rust" },
    { id: "native", label: "Nativo", hint: "Forza i tool client del database" },
    { id: "rust", label: "Puro Rust", hint: "Forza il fallback best-effort" },
  ];
</script>

<div class="picker">
  <span class="lbl">Metodo</span>
  <div class="seg">
    {#each opts as o}
      <button
        class="seg-btn"
        class:active={app.prefer === o.id}
        title={o.hint}
        onclick={() => (app.prefer = o.id)}
      >
        {o.label}
      </button>
    {/each}
  </div>

  <!-- Dry-run: anteprima senza modifiche. Vale per dump/import/clone. -->
  <label class="dry" class:on={app.dryRun}
         title="Anteprima: non modifica nulla, mostra solo cosa verrebbe fatto">
    <input type="checkbox" bind:checked={app.dryRun} />
    <span>Dry-run</span>
  </label>
</div>

<style>
  .picker {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .lbl {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--ink-soft);
  }
  .seg {
    display: flex;
    gap: 3px;
    background: var(--surface-2);
    padding: 4px;
    border-radius: 11px;
  }
  .seg-btn {
    border: none;
    background: transparent;
    color: var(--ink-soft);
    font-size: 13px;
    font-weight: 600;
    padding: 7px 13px;
    border-radius: 8px;
    transition: all 0.15s ease;
  }
  .seg-btn.active {
    background: var(--surface);
    color: var(--brand);
    box-shadow: var(--shadow-sm);
  }
  .dry {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--ink-soft);
    padding: 6px 10px;
    border-radius: 9px;
    background: var(--surface-2);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .dry.on {
    color: var(--brand);
    background: var(--surface);
    box-shadow: var(--shadow-sm);
  }
  .dry input {
    margin: 0;
  }
</style>

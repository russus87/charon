<script>
  import {
    app,
    requestClone,
    connById,
    engineLabel,
    addMaskRule,
    removeMaskRule,
    MASK_KINDS,
  } from "../lib/state.svelte.js";
  import ConnPicker from "./ConnPicker.svelte";
  import PreferPicker from "./PreferPicker.svelte";

  let src = $derived(connById(app.sel.cloneSrc));
  let dst = $derived(connById(app.sel.cloneDst));
  let opts = $derived(app.cloneOpts);
  let masking = $derived(opts.mask.length > 0);
  // Il masking richiede il puro Rust: solo PostgreSQL (in base alla sorgente).
  let pgOnly = $derived(src?.engine === "postgres");
  // Sorgente e destinazione devono usare lo stesso motore.
  let mismatch = $derived(!!src && !!dst && src.engine !== dst.engine);
  let ready = $derived(!!app.sel.cloneSrc && !!app.sel.cloneDst && !mismatch);
</script>

<div class="workspace">
  <div class="left">
    <ConnPicker bind:selectedId={app.sel.cloneSrc} title="Sorgente" />

    <div class="arrow">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 5v14M6 13l6 6 6-6" />
      </svg>
      <span>
        {#if opts.dataOnly}solo dati (preserva schema){:else}copia schema + dati{/if}
        {#if masking} · masking{/if}
      </span>
    </div>

    <ConnPicker bind:selectedId={app.sel.cloneDst} title="Destinazione" />

    {#if mismatch}
      <p class="warn-note">
        ⚠️ Sorgente ({engineLabel(src.engine)}) e destinazione ({engineLabel(dst.engine)})
        devono usare lo <b>stesso motore</b>.
      </p>
    {/if}

    <!-- Opzioni del clone: data-only + mascheramento -->
    <div class="card opts">
      <label class="check">
        <input type="checkbox" bind:checked={opts.dataOnly} />
        <span>
          <strong>Solo dati</strong> — preserva lo schema della destinazione
          (TRUNCATE + dati, ideale per schemi gestiti da migration).
        </span>
      </label>

      <div class="mask">
        <div class="mask-head">
          <span class="mask-title">Mascheramento colonne</span>
          <button class="btn ghost sm" onclick={addMaskRule}>+ Regola</button>
        </div>

        {#if !pgOnly && masking}
          <p class="warn">Il mascheramento è disponibile solo per PostgreSQL.</p>
        {/if}

        {#if opts.mask.length === 0}
          <p class="hint">
            Nessuna regola. Aggiungine per anonimizzare colonne sensibili durante
            il clone prod→test (forza il metodo puro Rust).
          </p>
        {/if}

        {#each opts.mask as rule, i (i)}
          <div class="rule">
            <input class="in tbl" placeholder="tabella" bind:value={rule.table} />
            <span class="dot">.</span>
            <input class="in col" placeholder="colonna" bind:value={rule.column} />
            <select class="in strat" bind:value={rule.kind}>
              {#each MASK_KINDS as k}
                <option value={k.kind}>{k.label}</option>
              {/each}
            </select>
            {#if rule.kind === "fixed"}
              <input class="in val" placeholder="valore" bind:value={rule.value} />
            {/if}
            <button class="btn ghost sm rm" title="Rimuovi" onclick={() => removeMaskRule(i)}>✕</button>
          </div>
        {/each}
      </div>
    </div>

    <div class="card bar">
      <PreferPicker />
      <button class="btn primary" disabled={app.busy || !ready} onclick={requestClone}>
        {opts.dataOnly ? "Sincronizza dati" : "Clona database"}
      </button>
    </div>
  </div>
</div>

<style>
  .arrow {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--accent);
    font-size: 12.5px;
    font-weight: 600;
  }
  .arrow svg {
    width: 20px;
    height: 20px;
  }
  .warn-note {
    margin: 0;
    font-size: 12.5px;
    color: var(--warn);
    background: var(--warn-soft);
    padding: 9px 12px;
    border-radius: var(--radius-sm);
  }
  .opts {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .check {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    font-size: 13px;
    cursor: pointer;
  }
  .check input {
    margin-top: 2px;
  }
  .check span strong {
    color: var(--accent);
  }
  .mask {
    display: flex;
    flex-direction: column;
    gap: 8px;
    border-top: 1px solid var(--border);
    padding-top: 12px;
  }
  .mask-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .mask-title {
    font-size: 13px;
    font-weight: 600;
  }
  .hint,
  .warn {
    font-size: 12px;
    margin: 0;
    color: var(--text-faint);
  }
  .warn {
    color: var(--amber);
  }
  .rule {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .in {
    padding: 7px 9px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong);
    background: var(--surface);
    color: var(--text);
    font-size: 12.5px;
  }
  .in:focus {
    outline: none;
    border-color: var(--green-500);
    box-shadow: 0 0 0 3px var(--green-100);
  }
  .tbl,
  .col {
    width: 110px;
  }
  .strat {
    flex: 0 0 auto;
  }
  .val {
    width: 120px;
  }
  .dot {
    opacity: 0.6;
    font-weight: 700;
  }
  .rm {
    margin-left: auto;
  }
  .btn.sm {
    padding: 4px 9px;
    font-size: 12px;
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

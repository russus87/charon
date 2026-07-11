<script>
  import {
    app,
    runClone,
    setEngine,
    addMaskRule,
    removeMaskRule,
    MASK_KINDS,
  } from "../lib/state.svelte.js";
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
  let opts = $derived(app.cloneOpts);
  let masking = $derived(opts.mask.length > 0);
  // Il masking richiede il puro Rust: solo PostgreSQL, e forza il metodo.
  let pgOnly = $derived(app.conn.engine === "postgres");
</script>

<div class="workspace">
  <div class="left">
    <ConnForm conn={app.conn} title="Sorgente" />

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

    <ConnForm conn={app.target} title="Destinazione" />

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
      <button class="btn primary" disabled={app.busy || !ready} onclick={runClone}>
        {opts.dataOnly ? "Sincronizza dati" : "Clona database"}
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
    color: var(--brand);
  }
  .mask {
    display: flex;
    flex-direction: column;
    gap: 8px;
    border-top: 1px solid var(--border, #2a2a2a);
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
    color: var(--muted, #888);
  }
  .warn {
    color: #d98b3a;
  }
  .rule {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .in {
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--border, #333);
    background: var(--input-bg, #1b1b1b);
    color: inherit;
    font-size: 12.5px;
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

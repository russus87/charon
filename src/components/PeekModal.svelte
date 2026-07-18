<script>
  import { app, closePeek } from "../lib/state.svelte.js";

  // Anteprima read-only delle prime righe di una tabella (griglia scrollabile).
  let p = $derived(app.peek);

  function onKeydown(e) {
    if (e.key === "Escape") closePeek();
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if p}
  <div
    class="backdrop"
    role="button"
    tabindex="-1"
    aria-label="Chiudi"
    onclick={closePeek}
    onkeydown={onKeydown}
  >
    <div
      class="modal card"
      role="dialog"
      aria-modal="true"
      aria-label={`Anteprima ${p.table}`}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="top">
        <div class="titles">
          <h3><span class="eye">👁</span> {p.table}</h3>
          {#if !p.loading && !p.error}
            <span class="sub">
              {p.rows.length} righ{p.rows.length === 1 ? "a" : "e"}
              {#if p.truncated}<span class="badge grey">prime 100 · troncato</span>{/if}
            </span>
          {/if}
        </div>
        <button class="btn ghost sm" onclick={closePeek}>Chiudi</button>
      </div>

      {#if p.loading}
        <div class="state spinner">Lettura righe…</div>
      {:else if p.error}
        <div class="state err">{p.error}</div>
      {:else if p.rows.length === 0}
        <div class="state">Nessuna riga.</div>
      {:else}
        <div class="grid-wrap">
          <table class="grid">
            <thead>
              <tr>
                <th class="rn">#</th>
                {#each p.columns as c}<th>{c}</th>{/each}
              </tr>
            </thead>
            <tbody>
              {#each p.rows as row, i}
                <tr>
                  <td class="rn">{i + 1}</td>
                  {#each row as cell}
                    {#if cell === null}
                      <td class="null">NULL</td>
                    {:else}
                      <td title={cell}>{cell}</td>
                    {/if}
                  {/each}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
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
    z-index: 55;
    cursor: default;
    animation: fade 0.12s ease;
  }
  .modal {
    width: min(920px, 96vw);
    max-height: 84vh;
    display: flex;
    flex-direction: column;
    padding: 18px 20px;
    gap: 12px;
    cursor: default;
    animation: pop 0.14s ease;
  }
  .top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  h3 {
    margin: 0;
    font-size: 16px;
    font-family: var(--mono);
  }
  .eye {
    font-family: var(--font);
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-dim);
    margin-top: 4px;
  }
  .state {
    padding: 28px 8px;
    color: var(--text-dim);
    font-size: 13.5px;
  }
  .state.err {
    color: var(--err);
  }
  .grid-wrap {
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    min-height: 0;
  }
  .grid {
    border-collapse: collapse;
    font-size: 12.5px;
    font-family: var(--mono);
    white-space: nowrap;
    width: 100%;
  }
  .grid th,
  .grid td {
    border-bottom: 1px solid var(--border);
    border-right: 1px solid var(--border);
    padding: 6px 10px;
    text-align: left;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .grid thead th {
    position: sticky;
    top: 0;
    background: var(--surface-2);
    color: var(--text-dim);
    font-weight: 700;
    z-index: 1;
  }
  .grid tbody tr:hover td {
    background: var(--surface-2);
  }
  .grid .rn {
    color: var(--text-faint);
    text-align: right;
    background: var(--surface-2);
    position: sticky;
    left: 0;
  }
  .grid td.null {
    color: var(--text-faint);
    font-style: italic;
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
  @media (prefers-reduced-motion: reduce) {
    .backdrop,
    .modal {
      animation: none;
    }
  }
</style>

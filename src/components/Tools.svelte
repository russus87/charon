<script>
  import { app, loadReports } from "../lib/state.svelte.js";
</script>

<div class="tools">
  <div class="intro">
    <div>
      <h2>Strumenti rilevati</h2>
      <p>
        Charon usa i <strong>tool nativi</strong> del database se li trova nel PATH
        (massima fedeltà); altrimenti ripiega su un <strong>fallback puro Rust</strong>
        best-effort. Qui vedi cosa è disponibile sulla tua macchina.
      </p>
    </div>
    <button class="btn ghost" onclick={loadReports}>Aggiorna</button>
  </div>

  <div class="cards">
    {#each app.reports as r}
      <div class="card eng">
        <div class="eng-head">
          <h3>{r.label}</h3>
          <div class="flags">
            <span class="badge {r.native_available ? 'ok' : 'err'}">
              nativo {r.native_available ? "✓" : "✗"}
            </span>
            <span class="badge {r.rust_available ? 'brand' : 'err'}">
              puro Rust {r.rust_available ? "✓" : "✗"}
            </span>
          </div>
        </div>

        <ul class="tlist">
          {#each r.tools as t}
            <li>
              <span class="tdot" class:on={t.found}></span>
              <div class="tinfo">
                <code>{t.name}</code>
                <span class="purpose">{t.purpose}</span>
                {#if t.found && t.path}<span class="tpath" title={t.path}>{t.path}</span>{/if}
              </div>
              <span class="tstate {t.found ? 'ok' : 'no'}">{t.found ? "trovato" : "assente"}</span>
            </li>
          {/each}
        </ul>

        <p class="note">{r.note}</p>
      </div>
    {/each}
  </div>
</div>

<style>
  .tools {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .intro {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  .intro h2 {
    margin: 0 0 6px;
    font-size: 20px;
  }
  .intro p {
    margin: 0;
    max-width: 720px;
    font-size: 13.5px;
    color: var(--ink-soft);
    line-height: 1.55;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 16px;
  }
  .eng {
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .eng-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .eng-head h3 {
    margin: 0;
    font-size: 16px;
  }
  .flags {
    display: flex;
    gap: 6px;
  }
  .tlist {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .tlist li {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .tdot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--ink-faint);
    flex-shrink: 0;
  }
  .tdot.on {
    background: var(--ok);
  }
  .tinfo {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .tinfo code {
    font-family: var(--mono);
    font-size: 13px;
    color: var(--ink);
  }
  .purpose {
    font-size: 12px;
    color: var(--ink-soft);
  }
  .tpath {
    font-size: 11px;
    color: var(--ink-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tstate {
    font-size: 11.5px;
    font-weight: 600;
  }
  .tstate.ok {
    color: var(--ok);
  }
  .tstate.no {
    color: var(--ink-faint);
  }
  .note {
    margin: 0;
    font-size: 12.5px;
    color: var(--ink-soft);
    line-height: 1.5;
    background: var(--surface-2);
    padding: 10px 12px;
    border-radius: 10px;
  }
</style>

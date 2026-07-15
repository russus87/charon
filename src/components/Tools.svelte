<script>
  import { app, loadReports } from "../lib/state.svelte.js";
  import { oracleSetup, pickOracleZip } from "../lib/api.js";

  // Indice della card con il pannello "come risolvere" aperto (null = nessuno).
  let openHelp = $state(null);
  const toggleHelp = (i) => (openHelp = openHelp === i ? null : i);

  // Esito dell'ultimo "Configura Instant Client" (per la card Oracle).
  let oraSetup = $state({ busy: false, msg: "" });

  async function setupOracle() {
    const zip = await pickOracleZip();
    if (!zip) return;
    oraSetup.busy = true;
    oraSetup.msg = "";
    try {
      const r = await oracleSetup(zip);
      oraSetup.msg = (r.ok ? "✅ " : "⚠️ ") + r.message;
      if (r.ok) await loadReports(); // aggiorna lo stato dei motori
    } catch (e) {
      oraSetup.msg = "⚠️ " + String(e);
    } finally {
      oraSetup.busy = false;
    }
  }
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
    {#each app.reports as r, i}
      <div class="card eng" class:hasproblem={r.hints?.length}>
        <div class="eng-head">
          <h3>
            {r.label}
            {#if r.hints?.length}
              <button
                class="ibtn"
                aria-label="Come risolvere"
                aria-expanded={openHelp === i}
                title="Come risolvere ({r.hints.length})"
                onclick={() => toggleHelp(i)}
              >i</button>
            {/if}
          </h3>
          <div class="flags">
            <span class="badge {r.native_available ? 'ok' : 'err'}">
              nativo {r.native_available ? "✓" : "✗"}
            </span>
            <span class="badge {r.rust_available ? 'brand' : 'err'}">
              puro Rust {r.rust_available ? "✓" : "✗"}
            </span>
          </div>
        </div>

        {#if openHelp === i && r.hints?.length}
          <div class="help">
            <div class="help-head">
              <strong>Come risolvere</strong>
              <button class="xbtn" aria-label="Chiudi" onclick={() => toggleHelp(i)}>✕</button>
            </div>
            {#each r.hints as h}
              <div class="hint">
                <div class="hint-title">{h.title}</div>
                <p class="hint-body">{h.body}</p>
                {#if h.command}
                  <pre class="hint-cmd">{h.command}</pre>
                {/if}
              </div>
            {/each}
          </div>
        {/if}

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

        {#if r.engine === "oracle"}
          <div class="ora-setup">
            <button class="btn ghost" disabled={oraSetup.busy} onclick={setupOracle}>
              {oraSetup.busy ? "Configuro…" : "Configura Instant Client…"}
            </button>
            <span class="ora-hint">
              Scegli lo <code>.zip</code> "Basic/Basic Lite" scaricato da Oracle: Charon
              lo scompatta in una cartella utente (nessun admin) e lo aggancia.
            </span>
            {#if oraSetup.msg}<p class="ora-msg">{oraSetup.msg}</p>{/if}
          </div>
        {/if}
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
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .eng.hasproblem {
    border-color: color-mix(in srgb, var(--err) 45%, transparent);
  }
  .ibtn {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: none;
    background: var(--err);
    color: #fff;
    font-size: 11px;
    font-weight: 700;
    font-style: italic;
    line-height: 18px;
    text-align: center;
    cursor: pointer;
    padding: 0;
    flex-shrink: 0;
  }
  .ibtn:hover {
    filter: brightness(1.1);
  }
  .help {
    background: var(--surface-2);
    border: 1px solid color-mix(in srgb, var(--err) 35%, transparent);
    border-radius: 12px;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .help-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
  }
  .xbtn {
    border: none;
    background: transparent;
    color: var(--ink-soft);
    cursor: pointer;
    font-size: 13px;
    padding: 2px 4px;
  }
  .hint-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--ink);
    margin-bottom: 4px;
  }
  .hint-body {
    margin: 0 0 8px;
    font-size: 12.5px;
    color: var(--ink-soft);
    line-height: 1.5;
  }
  .hint-cmd {
    margin: 0;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--ink);
    background: var(--surface);
    border: 1px solid var(--border, rgba(127, 127, 127, 0.18));
    border-radius: 8px;
    padding: 10px 12px;
    overflow-x: auto;
    white-space: pre;
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
  .ora-setup {
    display: flex;
    flex-direction: column;
    gap: 8px;
    border-top: 1px solid var(--border, rgba(127, 127, 127, 0.18));
    padding-top: 12px;
  }
  .ora-hint {
    font-size: 12px;
    color: var(--ink-soft);
    line-height: 1.5;
  }
  .ora-hint code {
    font-family: var(--mono);
    font-size: 11.5px;
  }
  .ora-msg {
    margin: 0;
    font-size: 12.5px;
    color: var(--ink);
    background: var(--surface-2);
    padding: 8px 10px;
    border-radius: 8px;
    word-break: break-word;
  }
</style>

<script>
  import { app, connById, engineLabel } from "../lib/state.svelte.js";
  import { runQuery } from "../lib/api.js";
  import ConnPicker from "./ConnPicker.svelte";

  // Editor SQL minimale (scratch): scrivi una query, eseguila, vedi il risultato.
  let conn = $derived(connById(app.sel.sql));
  let sql = $state("");
  let running = $state(false);
  let result = $state(null); // QueryResult o null
  let error = $state(null);

  let canRun = $derived(!!app.sel.sql && sql.trim().length > 0 && !running);

  async function exec() {
    if (!canRun) return;
    running = true;
    error = null;
    result = null;
    try {
      result = await runQuery($state.snapshot(conn), sql);
    } catch (e) {
      error = String(e);
    } finally {
      running = false;
    }
  }

  // Ctrl/Cmd+Enter esegue.
  function onKeydown(e) {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      exec();
    }
  }
</script>

<div class="sql">
  <div class="bar card">
    <ConnPicker bind:selectedId={app.sel.sql} title="Connessione" />
  </div>

  <div class="editor card">
    <div class="ehead">
      <span class="lbl">Query SQL</span>
      <span class="warn">⚠ eseguita direttamente sul database: può modificarne i dati</span>
      <button class="btn primary sm" disabled={!canRun} onclick={exec}>
        {running ? "Esecuzione…" : "Esegui"}
      </button>
    </div>
    <textarea
      bind:value={sql}
      onkeydown={onKeydown}
      spellcheck="false"
      placeholder={"SELECT * FROM ...  —  Ctrl/Cmd+Invio per eseguire"}
    ></textarea>
  </div>

  {#if error}
    <div class="card res err">
      <b>Errore</b>
      <pre>{error}</pre>
    </div>
  {:else if result}
    <div class="card res">
      {#if result.columns.length === 0}
        <div class="ok">✓ {result.message}</div>
      {:else}
        <div class="rhead">
          <b>{result.rows.length} righ{result.rows.length === 1 ? "a" : "e"}</b>
          {#if conn}<span class="badge brand">{engineLabel(conn.engine)}</span>{/if}
        </div>
        {#if result.rows.length === 0}
          <div class="ok">Nessuna riga.</div>
        {:else}
          <div class="grid-wrap">
            <table class="grid">
              <thead>
                <tr><th class="rn">#</th>{#each result.columns as c}<th>{c}</th>{/each}</tr>
              </thead>
              <tbody>
                {#each result.rows as row, i}
                  <tr>
                    <td class="rn">{i + 1}</td>
                    {#each row as cell}
                      {#if cell === null}<td class="null">NULL</td>{:else}<td title={cell}>{cell}</td>{/if}
                    {/each}
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .sql {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-height: 0;
  }
  .bar {
    padding: 14px 16px;
  }
  .editor {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .ehead {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .lbl {
    font-size: 13px;
    font-weight: 600;
  }
  .warn {
    font-size: 12px;
    color: var(--warn);
    margin-right: auto;
  }
  textarea {
    width: 100%;
    min-height: 160px;
    resize: vertical;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: #0e1220;
    color: #cdd4e6;
    padding: 12px 14px;
    font-family: var(--mono);
    font-size: 13px;
    line-height: 1.5;
    tab-size: 2;
  }
  textarea:focus {
    outline: none;
    border-color: var(--green-500);
  }
  .res {
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }
  .res.err {
    color: var(--err);
  }
  .res.err pre {
    margin: 6px 0 0;
    white-space: pre-wrap;
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .ok {
    color: var(--ok, #1f8a4c);
    font-size: 13.5px;
    font-weight: 600;
  }
  .rhead {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .grid-wrap {
    overflow: auto;
    max-height: 52vh;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
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
    max-width: 340px;
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
</style>

<script>
  import { app, connById } from "../lib/state.svelte.js";
  import { browseSchema, previewTable } from "../lib/api.js";
  import ConnPicker from "./ConnPicker.svelte";

  // Browser dati read-only: scegli una connessione, vedi le tabelle e i dati.
  const LIMIT = 200;

  let conn = $derived(connById(app.sel.browse));

  let tables = $state(null); // [{name, columns:[...]}] o null
  let loadingTables = $state(false);
  let tablesErr = $state(null);

  let selected = $state(null); // nome tabella selezionata
  let data = $state(null); // { columns, rows, truncated }
  let loadingData = $state(false);
  let dataErr = $state(null);
  let filter = $state(""); // filtro dell'elenco tabelle

  // Filtro righe + ordinamento colonna (client-side, sulle righe già caricate).
  let rowFilter = $state("");
  let sortCol = $state(null); // indice colonna di ordinamento (o null)
  let sortDir = $state("asc");

  let shownTables = $derived(
    (tables ?? []).filter((t) => !filter || t.name.toLowerCase().includes(filter.toLowerCase())),
  );

  function toggleSort(i) {
    if (sortCol === i) sortDir = sortDir === "asc" ? "desc" : "asc";
    else {
      sortCol = i;
      sortDir = "asc";
    }
  }

  // Confronto celle: NULL in fondo; numerico se entrambe sono numeri "puliti".
  function cmpCells(a, b) {
    if (a == null && b == null) return 0;
    if (a == null) return 1;
    if (b == null) return -1;
    const na = Number(a);
    const nb = Number(b);
    if (!Number.isNaN(na) && !Number.isNaN(nb) && a.trim() !== "" && b.trim() !== "") return na - nb;
    return a < b ? -1 : a > b ? 1 : 0;
  }

  let displayRows = $derived.by(() => {
    let rs = data?.rows ?? [];
    const f = rowFilter.trim().toLowerCase();
    if (f) rs = rs.filter((r) => r.some((c) => c != null && String(c).toLowerCase().includes(f)));
    if (sortCol != null) {
      const dir = sortDir === "asc" ? 1 : -1;
      rs = [...rs].sort((a, b) => cmpCells(a[sortCol], b[sortCol]) * dir);
    }
    return rs;
  });

  async function loadTables() {
    if (!conn) return;
    loadingTables = true;
    tablesErr = null;
    tables = null;
    selected = null;
    data = null;
    try {
      const schema = await browseSchema($state.snapshot(conn));
      tables = schema.tables;
    } catch (e) {
      tablesErr = String(e);
    } finally {
      loadingTables = false;
    }
  }

  async function openTable(name) {
    if (!conn) return;
    selected = name;
    loadingData = true;
    dataErr = null;
    data = null;
    rowFilter = "";
    sortCol = null;
    sortDir = "asc";
    try {
      data = await previewTable($state.snapshot(conn), name, LIMIT);
    } catch (e) {
      dataErr = String(e);
    } finally {
      loadingData = false;
    }
  }
</script>

<div class="sbircia">
  <div class="bar card">
    <ConnPicker bind:selectedId={app.sel.browse} title="Connessione" />
    <button class="btn primary" disabled={loadingTables || !app.sel.browse} onclick={loadTables}>
      {loadingTables ? "Carico…" : "Carica tabelle"}
    </button>
  </div>

  {#if tablesErr}
    <div class="card err-box"><b>Impossibile leggere lo schema</b><p>{tablesErr}</p></div>
  {/if}

  {#if tables}
    <div class="browser card">
      <aside class="tlist">
        <input class="filter" placeholder="Filtra tabelle…" bind:value={filter} />
        <div class="tscroll">
          {#if shownTables.length === 0}
            <p class="empty">Nessuna tabella.</p>
          {/if}
          {#each shownTables as t (t.name)}
            <button class="titem" class:on={selected === t.name} onclick={() => openTable(t.name)}>
              <span class="tn">{t.name}</span>
              <span class="tc">{t.columns.length}</span>
            </button>
          {/each}
        </div>
        <div class="tfoot">{tables.length} tabell{tables.length === 1 ? "a" : "e"}</div>
      </aside>

      <section class="tdata">
        {#if !selected}
          <div class="hint">Scegli una tabella a sinistra per vederne i dati.</div>
        {:else if loadingData}
          <div class="hint spinner">Lettura di {selected}…</div>
        {:else if dataErr}
          <div class="hint err">{dataErr}</div>
        {:else if data}
          <div class="dhead">
            <b class="dtitle">{selected}</b>
            <span class="dsub">
              {displayRows.length}{#if rowFilter.trim()} / {data.rows.length}{/if}
              righ{displayRows.length === 1 ? "a" : "e"}
              {#if data.truncated}<span class="badge grey">prime {LIMIT} · troncato</span>{/if}
            </span>
            <input class="rowfilter" placeholder="Filtra righe…" bind:value={rowFilter} />
          </div>
          {#if data.rows.length === 0}
            <div class="hint">Tabella vuota.</div>
          {:else}
            <div class="grid-wrap">
              <table class="grid">
                <thead>
                  <tr>
                    <th class="rn">#</th>
                    {#each data.columns as c, i}
                      <th class="sortable" class:sorted={sortCol === i} onclick={() => toggleSort(i)}>
                        {c}
                        <span class="sarrow">{sortCol === i ? (sortDir === "asc" ? "▲" : "▼") : "⇅"}</span>
                      </th>
                    {/each}
                  </tr>
                </thead>
                <tbody>
                  {#each displayRows as row, i}
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
      </section>
    </div>
  {/if}
</div>

<style>
  .sbircia {
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: 100%;
    min-height: 0;
  }
  .bar {
    padding: 14px 16px;
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 14px;
    flex-wrap: wrap;
  }
  .bar :global(.picker) {
    flex: 1;
    min-width: 260px;
  }
  .err-box {
    padding: 14px 16px;
    color: var(--err);
  }
  .err-box p {
    margin: 6px 0 0;
    font-size: 13px;
    white-space: pre-wrap;
  }
  .browser {
    display: grid;
    grid-template-columns: 260px 1fr;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }
  .tlist {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    min-height: 0;
  }
  .filter {
    margin: 10px;
    padding: 8px 10px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    color: var(--text);
    border-radius: var(--radius-sm);
    font-size: 13px;
  }
  .filter:focus {
    outline: none;
    border-color: var(--green-500);
  }
  .tscroll {
    overflow: auto;
    flex: 1;
    min-height: 0;
    padding: 0 6px;
  }
  .titem {
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    border-radius: 8px;
    padding: 8px 10px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    cursor: pointer;
    font-size: 13px;
  }
  .titem:hover {
    background: var(--surface-2);
  }
  .titem.on {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .tn {
    font-family: var(--mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tc {
    font-size: 11px;
    color: var(--text-faint);
    flex: none;
  }
  .tfoot {
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    font-size: 12px;
    color: var(--text-faint);
  }
  .tdata {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .hint {
    padding: 28px;
    color: var(--text-dim);
    font-size: 13.5px;
  }
  .hint.err {
    color: var(--err);
  }
  .dhead {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    flex-wrap: wrap;
  }
  .dtitle {
    font-family: var(--mono);
    font-size: 14px;
  }
  .dsub {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .rowfilter {
    margin-left: auto;
    padding: 6px 10px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    color: var(--text);
    border-radius: var(--radius-sm);
    font-size: 12.5px;
    width: 200px;
  }
  .rowfilter:focus {
    outline: none;
    border-color: var(--green-500);
  }
  .grid th.sortable {
    cursor: pointer;
    user-select: none;
  }
  .grid th.sortable:hover {
    color: var(--text);
  }
  .grid th .sarrow {
    font-size: 9px;
    opacity: 0.5;
    margin-left: 4px;
  }
  .grid th.sorted .sarrow {
    opacity: 1;
    color: var(--accent);
  }
  .grid-wrap {
    overflow: auto;
    flex: 1;
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
  .empty {
    padding: 12px;
    color: var(--text-faint);
    font-size: 12.5px;
  }
  .spinner::before {
    content: "";
    display: inline-block;
    width: 10px;
    height: 10px;
    margin-right: 8px;
    border-radius: 50%;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent);
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
</style>

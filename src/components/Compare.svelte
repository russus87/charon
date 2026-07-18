<script>
  import { app, runCompare, rowsDiffer, tableAligned, connById, requestSyncApply } from "../lib/state.svelte.js";
  import { exportDiff, pickReportPath, compareTableData, syncPlan } from "../lib/api.js";
  import ConnPicker from "./ConnPicker.svelte";

  // Vista "Compare": diff fra due database, in stile git.
  // FASE 1 — sola lettura: mostra le differenze, non applica nulla.

  let diff = $derived(app.diff);
  // Di default nascondiamo le tabelle allineate: il diff deve mostrare ciò che
  // NON torna, come farebbe `git status`.
  let showAligned = $state(false);

  let shown = $derived(
    (diff?.tables ?? []).filter((t) => showAligned || !tableAligned(t)),
  );
  let diffCount = $derived((diff?.tables ?? []).filter((t) => !tableAligned(t)).length);

  // Simbolo e classe per stato, in stile diff.
  const MARK = { only_source: "+", only_target: "−", changed: "~", same: "=" };
  const LABEL = {
    only_source: "solo nella sorgente",
    only_target: "solo nella destinazione",
    changed: "diversa",
    same: "uguale",
  };

  let open = $state({}); // nome tabella → dettaglio colonne espanso
  const toggle = (n) => (open[n] = !open[n]);
  const rows = (n) => (n == null ? "—" : n.toLocaleString("it-IT"));

  // Motori dei due lati: data-diff e allineamento sono solo per lo stesso motore
  // (il confronto cross-motore è di solo schema, sui tipi normalizzati).
  let cmpSrc = $derived(connById(app.sel.cmpSrc));
  let cmpDst = $derived(connById(app.sel.cmpDst));
  let sameEngine = $derived(!!cmpSrc && !!cmpDst && cmpSrc.engine === cmpDst.engine);

  // Confronto dati per-tabella (su richiesta): nome tabella → risultato/stato.
  let dataDiffs = $state({});
  // Una tabella esiste da entrambe le parti (quindi il data-diff ha senso)?
  const inBoth = (t) => t.status === "same" || t.status === "changed";

  async function runDataDiff(table) {
    const src = connById(app.sel.cmpSrc);
    const dst = connById(app.sel.cmpDst);
    if (!src || !dst) return;
    dataDiffs[table] = { loading: true };
    try {
      dataDiffs[table] = await compareTableData($state.snapshot(src), $state.snapshot(dst), table);
    } catch (e) {
      dataDiffs[table] = { error: String(e) };
    }
  }

  // --- Allineamento (sync): genera lo script DDL, poi applica con conferma ---
  let syncScript = $state(null);
  let syncing = $state(false);
  let syncErr = $state(null);
  async function generatePlan() {
    const src = connById(app.sel.cmpSrc);
    const dst = connById(app.sel.cmpDst);
    if (!src || !dst) return;
    syncing = true;
    syncErr = null;
    try {
      syncScript = await syncPlan($state.snapshot(src), $state.snapshot(dst));
    } catch (e) {
      syncErr = String(e);
    } finally {
      syncing = false;
    }
  }

  let exporting = $state(false);
  // Ri-esegue il confronto lato backend e ne salva il report (html/json).
  async function saveReport(format) {
    const src = connById(app.sel.cmpSrc);
    const dst = connById(app.sel.cmpDst);
    if (!src || !dst) return;
    const out = await pickReportPath(format);
    if (!out) return;
    exporting = true;
    try {
      await exportDiff($state.snapshot(src), $state.snapshot(dst), format, out);
    } catch (e) {
      app.diffErr = String(e);
    } finally {
      exporting = false;
    }
  }
</script>

<div class="cmp">
  <div class="pickers">
    <ConnPicker bind:selectedId={app.sel.cmpSrc} title="Sorgente" />
    <ConnPicker bind:selectedId={app.sel.cmpDst} title="Destinazione" />
  </div>

  <div class="card bar">
    <p class="note">
      Confronto <b>in sola lettura</b>: nessuno dei due database viene modificato.
      Disponibile per PostgreSQL, Oracle, SQL Server e SQLite.
    </p>
    <button class="btn primary" disabled={app.comparing} onclick={runCompare}>
      {app.comparing ? "Confronto in corso…" : "Confronta"}
    </button>
  </div>

  {#if app.diffErr}
    <div class="card err-box">
      <b>Confronto non riuscito</b>
      <p>{app.diffErr}</p>
    </div>
  {/if}

  {#if diff}
    <div class="card result">
      <div class="res-head">
        <div class="sides">
          <code class="side src">{diff.source_label}</code>
          <span class="arrow">↔</span>
          <code class="side dst">{diff.target_label}</code>
        </div>
        <div class="res-actions">
          <label class="toggle-aligned">
            <input type="checkbox" bind:checked={showAligned} />
            Mostra anche le tabelle allineate
          </label>
          <div class="exports">
            <button class="btn ghost sm" disabled={exporting} onclick={() => saveReport("html")}>
              Esporta HTML
            </button>
            <button class="btn ghost sm" disabled={exporting} onclick={() => saveReport("json")}>
              Esporta JSON
            </button>
          </div>
        </div>
      </div>

      {#if diffCount === 0}
        <div class="aligned-msg">
          ✓ I due database risultano allineati: stesse tabelle, stesse colonne,
          stesso numero di righe.
        </div>
      {:else}
        <p class="count">{diffCount} tabell{diffCount === 1 ? "a" : "e"} con differenze</p>
      {/if}

      <div class="tables">
        {#each shown as t (t.name)}
          <div class="tbl {t.status}" class:ok={tableAligned(t)}>
            <button class="tbl-head" onclick={() => toggle(t.name)}>
              <span class="mark">{MARK[t.status]}</span>
              <span class="tname">{t.name}</span>
              <span class="badge grey">{LABEL[t.status]}</span>
              {#if rowsDiffer(t)}
                <span class="badge warn">righe: {rows(t.source_rows)} ≠ {rows(t.target_rows)}</span>
              {:else if t.source_rows != null || t.target_rows != null}
                <span class="rowcount">righe: {rows(t.source_rows ?? t.target_rows)}</span>
              {/if}
              {#if t.columns.length}
                <span class="chev">{open[t.name] ? "▾" : "▸"} {t.columns.length} colonn{t.columns.length === 1 ? "a" : "e"}</span>
              {/if}
            </button>

            {#if inBoth(t) && sameEngine}
              <div class="data-row">
                {#if !dataDiffs[t.name]}
                  <button class="btn ghost sm" onclick={() => runDataDiff(t.name)}>
                    Confronta dati →
                  </button>
                {:else if dataDiffs[t.name].loading}
                  <span class="ddim">Confronto dati in corso…</span>
                {:else if dataDiffs[t.name].error}
                  <span class="derr" title={dataDiffs[t.name].error}>Errore nel confronto dati</span>
                {:else}
                  {@const dd = dataDiffs[t.name]}
                  {#if dd.only_source === 0 && dd.only_target === 0 && dd.changed === 0}
                    <span class="dd-ok">✓ dati identici ({rows(dd.same)} righe)</span>
                  {:else}
                    <span class="dd-badge add" title="solo nella sorgente">+{dd.only_source}</span>
                    <span class="dd-badge del" title="solo nella destinazione">−{dd.only_target}</span>
                    <span class="dd-badge chg" title="stessa chiave, valori diversi">~{dd.changed}</span>
                    <span class="dd-same">{rows(dd.same)} uguali</span>
                  {/if}
                  <span class="dd-key">
                    {dd.key.length ? `chiave: ${dd.key.join(", ")}` : dd.note}
                  </span>
                  <button class="btn ghost sm" title="Ricalcola" onclick={() => runDataDiff(t.name)}>↻</button>
                  {#if dd.sample?.length}
                    <details class="dd-sample">
                      <summary>campione ({dd.sample.length})</summary>
                      {#each dd.sample as s}
                        <div class="ds {s.kind}"><span class="mark">{MARK[s.kind]}</span><code>{s.key}</code></div>
                      {/each}
                    </details>
                  {/if}
                {/if}
              </div>
            {/if}

            {#if open[t.name] && t.columns.length}
              <div class="cols">
                {#each t.columns as c (c.name)}
                  <div class="col {c.status}">
                    <span class="mark">{MARK[c.status]}</span>
                    <span class="cname">{c.name}</span>
                    <span class="defs">
                      <code class="src-def">{c.source ?? "—"}</code>
                      <span class="arrow">→</span>
                      <code class="dst-def">{c.target ?? "—"}</code>
                    </span>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      </div>

      <p class="phase-note">
        ⓘ Il data-diff e l'allineamento dello schema confrontano/generano; le DROP
        nell'allineamento sono distruttive: lo script va sempre riletto.
      </p>

      {#if !sameEngine}
        <p class="xnote">
          Confronto <b>cross-motore</b> ({cmpSrc?.engine} → {cmpDst?.engine}): solo schema, con
          i tipi normalizzati. Il confronto dati e l'allineamento sono disponibili fra database
          dello stesso motore.
        </p>
      {/if}

      {#if diffCount > 0 && sameEngine}
        <div class="sync">
          <div class="sync-head">
            <div>
              <b>Allineamento schema</b>
              <span class="sync-sub">genera le DDL per portare la destinazione al livello della sorgente</span>
            </div>
            <button class="btn ghost sm" disabled={syncing} onclick={generatePlan}>
              {syncing ? "Generazione…" : syncScript ? "Rigenera" : "Genera script"}
            </button>
          </div>

          {#if syncErr}
            <p class="sync-err">{syncErr}</p>
          {/if}

          {#if syncScript}
            <pre class="sync-pre">{syncScript}</pre>
            <div class="sync-actions">
              <span class="warn-inline">⚠ Rileggi lo script: le DROP possono perdere dati.</span>
              <button class="btn danger-solid sm" disabled={app.busy} onclick={requestSyncApply}>
                Applica alla destinazione
              </button>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .cmp {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .pickers {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .bar {
    padding: 14px 16px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    flex-wrap: wrap;
  }
  .note {
    margin: 0;
    font-size: 13px;
    color: var(--text-dim, var(--ink-soft));
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
  .result {
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .res-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    flex-wrap: wrap;
  }
  .sides {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .side {
    font-family: var(--mono);
    font-size: 12px;
    padding: 3px 8px;
    border-radius: 6px;
    background: var(--surface-2, #f2f4f8);
  }
  .arrow {
    color: var(--text-faint);
  }
  .res-actions {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .exports {
    display: flex;
    gap: 6px;
  }
  .toggle-aligned {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    color: var(--text-dim, var(--ink-soft));
    cursor: pointer;
  }
  .count {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  .aligned-msg {
    background: var(--green-100, #e6f6ec);
    color: var(--ok, #1f8a4c);
    padding: 12px 14px;
    border-radius: var(--radius-sm);
    font-size: 13.5px;
  }
  .tables {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .tbl {
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
    border-left-width: 3px;
  }
  .tbl.only_source {
    border-left-color: var(--ok, #1f8a4c);
  }
  .tbl.only_target {
    border-left-color: var(--err, #c0392b);
  }
  .tbl.changed {
    border-left-color: var(--warn, #b7791f);
  }
  .tbl.ok {
    border-left-color: var(--border-strong);
    opacity: 0.65;
  }
  .tbl-head {
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    padding: 9px 12px;
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
    flex-wrap: wrap;
  }
  .tbl-head:hover {
    background: var(--surface-2, #f7f8fb);
  }
  .mark {
    font-family: var(--mono);
    font-weight: 700;
    width: 12px;
    flex: none;
  }
  .only_source > .mark,
  .col.only_source .mark {
    color: var(--ok, #1f8a4c);
  }
  .only_target > .mark,
  .col.only_target .mark {
    color: var(--err, #c0392b);
  }
  .changed > .mark,
  .col.changed .mark {
    color: var(--warn, #b7791f);
  }
  .tname {
    font-family: var(--mono);
    font-size: 13px;
    font-weight: 600;
  }
  .rowcount {
    font-size: 12px;
    color: var(--text-faint);
  }
  .chev {
    margin-left: auto;
    font-size: 12px;
    color: var(--text-dim, var(--ink-soft));
  }
  .data-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 7px 12px 9px 30px;
    border-top: 1px solid var(--line-soft, var(--border));
    font-size: 12.5px;
  }
  .ddim {
    color: var(--text-faint);
  }
  .derr {
    color: var(--err);
  }
  .dd-ok {
    color: var(--ok, #1f8a4c);
    font-weight: 600;
  }
  .dd-badge {
    font-family: var(--mono);
    font-weight: 700;
    font-size: 12px;
    padding: 2px 8px;
    border-radius: 999px;
  }
  .dd-badge.add {
    color: var(--ok, #1f8a4c);
    background: var(--ok-soft, #e6f6ec);
  }
  .dd-badge.del {
    color: var(--err, #c0392b);
    background: var(--err-soft, #fdeaea);
  }
  .dd-badge.chg {
    color: var(--warn, #b7791f);
    background: var(--warn-soft, #fbf0dc);
  }
  .dd-same {
    color: var(--text-faint);
  }
  .dd-key {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--text-dim);
    margin-left: auto;
  }
  .dd-sample {
    flex-basis: 100%;
    margin-top: 4px;
  }
  .dd-sample summary {
    cursor: pointer;
    font-size: 12px;
    color: var(--text-dim);
  }
  .ds {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 2px 0 2px 14px;
  }
  .ds code {
    font-family: var(--mono);
    font-size: 11.5px;
  }
  .ds.only_source .mark {
    color: #1f8a4c;
  }
  .ds.only_target .mark {
    color: #c0392b;
  }
  .ds.changed .mark {
    color: #b7791f;
  }

  .cols {
    border-top: 1px solid var(--border);
    background: var(--surface-2, #fafbfd);
    padding: 8px 12px 10px 30px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .col {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .cname {
    font-family: var(--mono);
    font-size: 12px;
    min-width: 150px;
  }
  .defs {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .defs code {
    font-family: var(--mono);
    font-size: 11.5px;
    padding: 2px 6px;
    border-radius: 5px;
    background: var(--surface);
    border: 1px solid var(--border);
  }
  .sync {
    border-top: 1px solid var(--border);
    padding-top: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .sync-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .sync-sub {
    font-size: 12.5px;
    color: var(--text-dim);
    margin-left: 8px;
  }
  .sync-err {
    margin: 0;
    font-size: 13px;
    color: var(--err);
  }
  .sync-pre {
    margin: 0;
    max-height: 320px;
    overflow: auto;
    background: #0e1220;
    color: #cdd4e6;
    border-radius: 10px;
    padding: 14px;
    font-family: var(--mono);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre;
  }
  .sync-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .warn-inline {
    font-size: 12.5px;
    color: var(--warn);
  }
  .btn.danger-solid {
    background: var(--red);
    border-color: var(--red);
    color: #fff;
  }
  .btn.danger-solid:hover {
    background: #bf3a30;
  }
  .phase-note {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-faint);
  }
  .xnote {
    margin: 0;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    font-size: 12.5px;
    color: var(--text-dim);
  }
</style>

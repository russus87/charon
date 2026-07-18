<script>
  import { onMount } from "svelte";
  import {
    app,
    loadReports,
    loadConnections,
    initProgress,
    initTheme,
    resolvedTheme,
  } from "./lib/state.svelte.js";
  import Sidebar from "./components/Sidebar.svelte";
  import Connection from "./components/Connection.svelte";
  import Dump from "./components/Dump.svelte";
  import Import from "./components/Import.svelte";
  import Clone from "./components/Clone.svelte";
  import Compare from "./components/Compare.svelte";
  import Sbircia from "./components/Sbircia.svelte";
  import Sql from "./components/Sql.svelte";
  import Tools from "./components/Tools.svelte";
  import ResultModal from "./components/ResultModal.svelte";
  import ConfirmModal from "./components/ConfirmModal.svelte";
  import ResultPanel from "./components/ResultPanel.svelte";
  import PeekModal from "./components/PeekModal.svelte";

  onMount(() => {
    initTheme(); // legge la preferenza salvata (auto/light/dark)
    loadReports();
    loadConnections(); // profili di connessione salvati
    initProgress(); // avanzamento live delle operazioni

    // Quando il tema è "auto", segui i cambi di preferenza del sistema.
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    const onSys = () => {
      if (app.theme === "auto") document.documentElement.dataset.theme = resolvedTheme();
    };
    mq?.addEventListener?.("change", onSys);
    return () => mq?.removeEventListener?.("change", onSys);
  });

  // Applica il tema risolto (light/dark) all'elemento radice a ogni cambio.
  $effect(() => {
    // dipende da app.theme
    document.documentElement.dataset.theme = ((_) => resolvedTheme())(app.theme);
  });

  // Metadati di ogni vista: titolo + sottotitolo mostrati nell'intestazione.
  const views = {
    connection: { comp: Connection, title: "Connessioni", sub: "Gestisci le connessioni ai database riutilizzabili nelle operazioni." },
    dump: { comp: Dump, title: "Dump", sub: "Esporta un database su file." },
    import: { comp: Import, title: "Importa", sub: "Carica un dump dentro un database." },
    clone: { comp: Clone, title: "Clona", sub: "Copia schema e dati da una sorgente a una destinazione." },
    compare: { comp: Compare, title: "Compare", sub: "Confronta due database e vedi cosa differisce, in stile diff." },
    sbircia: { comp: Sbircia, title: "Sbircia", sub: "Sfoglia tabelle e dati di una connessione, in sola lettura." },
    sql: { comp: Sql, title: "SQL", sub: "Scrivi ed esegui query SQL su una connessione." },
    tools: { comp: Tools, title: "Strumenti", sub: "Cosa è disponibile sulla macchina e come si connette Database Studio." },
  };

  let current = $derived(views[app.view] ?? views.connection);

  // ---- Navigazione da tastiera ----
  // Tasti 1–6 per le viste, "?" per la legenda. Disattivi quando si scrive in un
  // campo o quando è aperto un popup/editor, per non rubare i tasti.
  let showShortcuts = $state(false);
  const NAV_KEYS = { 1: "connection", 2: "dump", 3: "import", 4: "clone", 5: "compare", 6: "sbircia", 7: "sql", 8: "tools" };
  const NAV_HELP = [
    ["1", "Connessioni"], ["2", "Dump"], ["3", "Importa"], ["4", "Clona"],
    ["5", "Compare"], ["6", "Sbircia"], ["7", "SQL"], ["8", "Strumenti"], ["?", "Questa legenda"],
  ];

  function onKeydown(e) {
    if (e.key === "Escape") {
      showShortcuts = false;
      return;
    }
    const t = e.target;
    const typing =
      t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable);
    if (typing || e.metaKey || e.ctrlKey || e.altKey) return;
    // Non rubare i tasti mentre un popup o l'editor connessione è aperto.
    if (app.confirm || app.resultModal || app.editing) return;
    if (e.key === "?") {
      showShortcuts = !showShortcuts;
      e.preventDefault();
      return;
    }
    const v = NAV_KEYS[e.key];
    if (v) {
      app.view = v;
      e.preventDefault();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="shell">
  <Sidebar />

  <main class="content">
    {#key app.view}
      {@const Comp = current.comp}
      <div class="page fade-in">
        <div class="page-head">
          <div>
            <div class="breadcrumb">Database Studio <span class="sep">/</span> <span class="cur">{current.title}</span></div>
            <h1 class="title">{current.title}</h1>
            <p class="subtitle">{current.sub}</p>
          </div>
        </div>
        <div class="view">
          <Comp />
        </div>
      </div>
    {/key}
  </main>

  <!-- Console ancorata in basso, comune a tutte le viste. -->
  <ResultPanel />

  {#if showShortcuts}
    <div
      class="kbd-overlay"
      role="button"
      tabindex="-1"
      aria-label="Chiudi"
      onclick={() => (showShortcuts = false)}
      onkeydown={(e) => e.key === "Escape" && (showShortcuts = false)}
    >
      <div class="kbd card" role="dialog" aria-label="Scorciatoie da tastiera" onclick={(e) => e.stopPropagation()} onkeydown={() => {}} tabindex="-1">
        <h3>Scorciatoie</h3>
        <dl>
          {#each NAV_HELP as [k, label]}
            <div class="krow"><kbd>{k}</kbd><span>{label}</span></div>
          {/each}
        </dl>
        <p class="kbd-foot">Esc per chiudere</p>
      </div>
    </div>
  {/if}

  <!-- Anteprima dati (peek) read-only. -->
  <PeekModal />

  <!-- Conferma PRIMA dell'operazione, riepilogo DOPO: entrambi sopra tutto. -->
  <ConfirmModal />
  <ResultModal />
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: var(--sidebar-w) 1fr;
    height: 100vh;
    overflow: hidden;
  }
  .content {
    overflow-y: auto;
    /* spazio in fondo per la barra Console ancorata (collassata ~52px) */
    padding: 22px 30px 64px;
    min-width: 0;
  }
  .page {
    display: flex;
    flex-direction: column;
    gap: 18px;
    min-height: 100%;
  }
  .page-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  .breadcrumb {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-faint);
    font-size: 13px;
    margin-bottom: 6px;
  }
  .breadcrumb .sep {
    opacity: 0.6;
  }
  .breadcrumb .cur {
    color: var(--accent);
    font-weight: 600;
  }
  .title {
    font-size: 24px;
    font-weight: 700;
    margin: 0;
    letter-spacing: -0.02em;
  }
  .subtitle {
    color: var(--text-dim);
    margin: 4px 0 0;
    font-size: 13.5px;
  }
  .view {
    flex: 1;
    min-height: 0;
  }

  /* Legenda scorciatoie (tasto ?) */
  .kbd-overlay {
    position: fixed;
    inset: 0;
    background: rgba(8, 10, 18, 0.5);
    backdrop-filter: blur(2px);
    display: grid;
    place-items: center;
    z-index: 70;
    cursor: default;
    animation: fade 0.12s ease;
  }
  .kbd {
    width: min(340px, 92vw);
    padding: 20px 22px;
    cursor: default;
  }
  .kbd h3 {
    margin: 0 0 14px;
    font-size: 16px;
  }
  .kbd dl {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .krow {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13.5px;
    color: var(--text-dim);
  }
  .krow kbd {
    font-family: var(--mono);
    font-size: 12px;
    min-width: 26px;
    text-align: center;
    padding: 4px 0;
    border: 1px solid var(--border-strong);
    border-bottom-width: 2px;
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
  }
  .kbd-foot {
    margin: 14px 0 0;
    font-size: 12px;
    color: var(--text-faint);
  }
  @media (prefers-reduced-motion: reduce) {
    .kbd-overlay {
      animation: none;
    }
  }
</style>

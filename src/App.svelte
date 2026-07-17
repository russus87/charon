<script>
  import { onMount } from "svelte";
  import { app, loadReports, loadConnections, initProgress } from "./lib/state.svelte.js";
  import Sidebar from "./components/Sidebar.svelte";
  import Connection from "./components/Connection.svelte";
  import Dump from "./components/Dump.svelte";
  import Import from "./components/Import.svelte";
  import Clone from "./components/Clone.svelte";
  import Compare from "./components/Compare.svelte";
  import Tools from "./components/Tools.svelte";
  import ResultModal from "./components/ResultModal.svelte";
  import ConfirmModal from "./components/ConfirmModal.svelte";
  import ResultPanel from "./components/ResultPanel.svelte";

  onMount(() => {
    loadReports();
    loadConnections(); // profili di connessione salvati
    initProgress(); // avanzamento live delle operazioni
  });

  // Metadati di ogni vista: titolo + sottotitolo mostrati nell'intestazione.
  const views = {
    connection: { comp: Connection, title: "Connessioni", sub: "Gestisci le connessioni ai database riutilizzabili nelle operazioni." },
    dump: { comp: Dump, title: "Dump", sub: "Esporta un database su file." },
    import: { comp: Import, title: "Importa", sub: "Carica un dump dentro un database." },
    clone: { comp: Clone, title: "Clona", sub: "Copia schema e dati da una sorgente a una destinazione." },
    compare: { comp: Compare, title: "Compare", sub: "Confronta due database e vedi cosa differisce, in stile diff." },
    tools: { comp: Tools, title: "Strumenti", sub: "Cosa è disponibile sulla macchina e come si connette Database Studio." },
  };

  let current = $derived(views[app.view] ?? views.connection);
</script>

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
</style>

<script>
  import { onMount } from "svelte";
  import { app, loadReports } from "./lib/state.svelte.js";
  import TopBar from "./components/TopBar.svelte";
  import Connection from "./components/Connection.svelte";
  import Dump from "./components/Dump.svelte";
  import Import from "./components/Import.svelte";
  import Clone from "./components/Clone.svelte";
  import Tools from "./components/Tools.svelte";

  onMount(loadReports);

  // Le viste sono tab orizzontali (layout diverso dal rail verticale di GlyphBox).
  const tabs = [
    { id: "connection", label: "Connessione" },
    { id: "dump", label: "Dump" },
    { id: "import", label: "Importa" },
    { id: "clone", label: "Clona" },
    { id: "tools", label: "Strumenti" },
  ];

  const views = {
    connection: Connection,
    dump: Dump,
    import: Import,
    clone: Clone,
    tools: Tools,
  };

  let Current = $derived(views[app.view] ?? Connection);
</script>

<div class="shell">
  <TopBar />

  <nav class="tabs">
    {#each tabs as t}
      <button
        class="tab"
        class:active={app.view === t.id}
        onclick={() => (app.view = t.id)}
      >
        {t.label}
      </button>
    {/each}
  </nav>

  <main class="content">
    {#key app.view}
      <div class="view fade-in">
        <Current />
      </div>
    {/key}
  </main>
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 0 26px;
    margin-top: 6px;
  }
  .tab {
    border: none;
    background: transparent;
    color: var(--ink-soft);
    font-size: 14px;
    font-weight: 600;
    padding: 12px 18px;
    border-radius: 12px 12px 0 0;
    position: relative;
    transition: color 0.15s ease, background 0.15s ease;
  }
  .tab:hover {
    color: var(--ink);
    background: rgba(255, 255, 255, 0.5);
  }
  .tab.active {
    color: var(--brand);
    background: var(--surface);
  }
  .tab.active::after {
    content: "";
    position: absolute;
    left: 18px;
    right: 18px;
    bottom: 6px;
    height: 3px;
    border-radius: 3px;
    background: var(--brand-grad);
  }
  .content {
    flex: 1;
    min-height: 0;
    padding: 18px 26px 26px;
    overflow: auto;
  }
  .view {
    height: 100%;
    min-height: 0;
  }
</style>

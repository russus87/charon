<script>
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app, reportFor, cycleTheme } from "../lib/state.svelte.js";

  // Etichetta/icona del tema corrente (ciclo auto → chiaro → scuro).
  const THEME_UI = {
    auto: { icon: "◐", label: "Tema: auto" },
    light: { icon: "☀", label: "Tema: chiaro" },
    dark: { icon: "☾", label: "Tema: scuro" },
  };

  const VERSION = "0.3.1"; // versione app (allineata a Cargo/tauri.conf)

  // Voci di navigazione = le viste dell'app. Icone come path SVG 24×24.
  const nav = [
    { id: "connection", label: "Connessioni",
      icon: "M12 3C7 3 4 4.6 4 6.5S7 10 12 10s8-1.6 8-3.5S17 3 12 3zM4 9.5v4c0 1.9 3 3.5 8 3.5s8-1.6 8-3.5v-4C20 11.4 17 13 12 13s-8-1.6-8-3.5zm0 7V19c0 1.9 3 3.5 8 3.5s8-1.6 8-3.5v-2.5C20 18.4 17 20 12 20s-8-1.6-8-3.5z" },
    { id: "dump", label: "Dump",
      icon: "M12 3v10m0 0l3-3m-3 3l-3-3M5 14v3a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-3" },
    { id: "import", label: "Importa",
      icon: "M12 14V4m0 0l3 3m-3-3l-3 3M5 14v3a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-3" },
    { id: "clone", label: "Clona",
      icon: "M9 8h10v11H9zM5 5h10v2H7v9H5z" },
    // Compare: due rami che divergono, come un diff/branch.
    { id: "compare", label: "Compare",
      icon: "M6 4v9a3 3 0 0 0 3 3h6m0 0l-3-3m3 3l-3 3M6 4a2 2 0 1 0 0-.1zM18 20a2 2 0 1 0 0-.1z" },
    // Sbircia: una lente (browse dati read-only).
    { id: "sbircia", label: "Sbircia",
      icon: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zm5.5 12.5L21 21" },
    // SQL: parentesi angolari (editor query).
    { id: "sql", label: "SQL",
      icon: "M8 8l-4 4 4 4M16 8l4 4-4 4M13 5l-2 14" },
    { id: "tools", label: "Strumenti",
      icon: "M14.7 6.3a4 4 0 0 0-5.4 5.4L4 17v3h3l5.3-5.3a4 4 0 0 0 5.4-5.4l-2.6 2.6-2-2 2.6-2.6z" },
  ];

  const engines = [
    { id: "postgres", short: "PostgreSQL" },
    { id: "mysql", short: "MySQL" },
    { id: "oracle", short: "Oracle" },
    { id: "sqlserver", short: "SQL Server" },
    { id: "sqlite", short: "SQLite" },
  ];

  // Stato sintetico per il footer: verde = nativo, ambra = solo puro-Rust, off = niente.
  let status = $derived(
    engines.map((e) => {
      const r = reportFor(e.id);
      const state = r?.native_available ? "native" : r?.rust_available ? "rust" : "none";
      return { short: e.short, state };
    }),
  );
</script>

<aside class="sidebar">
  <div class="brand">
    <div class="logo" title="Database Studio">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
           stroke-linecap="round" stroke-linejoin="round">
        <path d="M3 15c1.5 1 3 1 4.5 0s3-1 4.5 0 3 1 4.5 0 3-1 4.5 0" />
        <path d="M5 12l1.6-4.2a2 2 0 0 1 1.9-1.3h7a2 2 0 0 1 1.9 1.3L20 12" />
        <path d="M12 6.5V3" />
      </svg>
    </div>
    <div class="brand-text">
      <strong>Database Studio</strong>
      <span>dump · clona · confronta</span>
    </div>
  </div>

  <nav>
    {#each nav as item}
      <button
        class="nav-item"
        class:active={app.view === item.id}
        onclick={() => (app.view = item.id)}
      >
        <svg viewBox="0 0 24 24" width="19" height="19" fill="none" stroke="currentColor"
             stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d={item.icon} />
        </svg>
        {item.label}
      </button>
    {/each}
  </nav>

  <div class="side-foot">
    <button class="foot-card" onclick={() => (app.view = "tools")} title="Vai a Strumenti">
      <span class="foot-title">Motori</span>
      <div class="engs">
        {#each status as s}
          <span class="eng {s.state}">
            <span class="dot"></span>{s.short}
          </span>
        {/each}
      </div>
    </button>

    <button class="theme-toggle" onclick={cycleTheme} title="Cambia tema (auto / chiaro / scuro)">
      <span class="ti">{THEME_UI[app.theme].icon}</span>
      {THEME_UI[app.theme].label}
    </button>

    <div class="app-meta">
      <span class="ver">Database Studio v{VERSION}</span>
      <button class="site" onclick={() => openUrl("https://russus.it")} title="Apri russus.it">
        russus.it ↗
      </button>
    </div>
  </div>
</aside>

<style>
  .sidebar {
    background: linear-gradient(180deg, var(--green-800), var(--green-900));
    color: #d9e9e0;
    display: flex;
    flex-direction: column;
    padding: 18px 14px;
    gap: 8px;
    overflow-y: auto;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 4px 6px 14px;
  }
  .logo {
    width: 40px;
    height: 40px;
    border-radius: 12px;
    background: #fff;
    color: var(--green-800);
    display: grid;
    place-items: center;
    flex: 0 0 auto;
  }
  .logo svg {
    width: 24px;
    height: 24px;
  }
  .brand-text {
    display: flex;
    flex-direction: column;
    line-height: 1.15;
  }
  .brand-text strong {
    font-size: 16px;
    color: #fff;
    letter-spacing: -0.01em;
  }
  .brand-text span {
    font-size: 11.5px;
    color: #9dc4b1;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-top: 6px;
  }
  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    background: transparent;
    border: none;
    color: #c4ddd0;
    padding: 10px 13px;
    border-radius: 10px;
    font-size: 13.5px;
    font-weight: 500;
    text-align: left;
    width: 100%;
    transition: background 0.13s ease, color 0.13s ease, transform 0.12s ease;
  }
  /* Barretta d'accento a sinistra: cresce quando la voce è attiva. */
  .nav-item::before {
    content: "";
    position: absolute;
    left: 4px;
    top: 50%;
    width: 3px;
    height: 0;
    border-radius: 3px;
    background: var(--green-500);
    transform: translateY(-50%);
    transition: height 0.18s cubic-bezier(0.2, 0.9, 0.3, 1.2);
  }
  .nav-item:hover {
    background: rgba(255, 255, 255, 0.07);
    color: #fff;
  }
  .nav-item:not(.active):hover {
    transform: translateX(2px);
  }
  .nav-item.active {
    background: rgba(255, 255, 255, 0.14);
    color: #fff;
    font-weight: 600;
  }
  .nav-item.active::before {
    height: 18px;
  }
  .nav-item svg {
    opacity: 0.9;
    flex: 0 0 auto;
    transition: transform 0.12s ease;
  }
  .nav-item:hover svg {
    transform: scale(1.08);
  }
  @media (prefers-reduced-motion: reduce) {
    .nav-item,
    .nav-item::before,
    .nav-item svg {
      transition: none;
    }
  }

  .side-foot {
    margin-top: auto;
    padding-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .theme-toggle {
    display: flex;
    align-items: center;
    gap: 9px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: #c4ddd0;
    padding: 8px 12px;
    border-radius: 10px;
    font-size: 12.5px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.13s ease;
  }
  .theme-toggle:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
  }
  .theme-toggle .ti {
    font-size: 15px;
    line-height: 1;
  }
  .app-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 0 4px;
    font-size: 11.5px;
    color: #9dc4b1;
  }
  .site {
    background: transparent;
    border: none;
    color: #bfe0cf;
    font-size: 11.5px;
    font-weight: 600;
    padding: 2px 4px;
    border-radius: 6px;
  }
  .site:hover {
    color: #fff;
    background: rgba(255, 255, 255, 0.08);
  }
  .foot-card {
    width: 100%;
    text-align: left;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 10px;
    padding: 11px 12px;
    color: inherit;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .foot-card:hover {
    background: rgba(255, 255, 255, 0.09);
  }
  .foot-title {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #93bda9;
  }
  .engs {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .eng {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: #bcdac9;
  }
  .eng .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #6f9a84; /* none */
    flex: 0 0 auto;
  }
  .eng.native .dot {
    background: #57d99a;
  }
  .eng.rust .dot {
    background: var(--amber);
  }
  .eng.none {
    color: #7fae96;
  }
</style>

<script>
  import { app, reportFor } from "../lib/state.svelte.js";

  const engines = [
    { id: "postgres", short: "PostgreSQL" },
    { id: "oracle", short: "Oracle" },
    { id: "sqlserver", short: "SQL Server" },
  ];

  // Stato sintetico dei tool nativi per la pill in alto a destra.
  let summary = $derived(
    engines.map((e) => {
      const r = reportFor(e.id);
      return { short: e.short, native: r?.native_available ?? false };
    }),
  );
</script>

<header class="bar">
  <div
    class="brand"
    onclick={() => (app.view = "connection")}
    onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (app.view = "connection")}
    role="button"
    tabindex="0"
  >
    <div class="logo">
      <!-- Charon: il traghettatore. Una piccola barca che attraversa. -->
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
           stroke-linecap="round" stroke-linejoin="round">
        <path d="M3 15c1.5 1 3 1 4.5 0s3-1 4.5 0 3 1 4.5 0 3-1 4.5 0" />
        <path d="M5 12l1.6-4.2a2 2 0 0 1 1.9-1.3h7a2 2 0 0 1 1.9 1.3L20 12" />
        <path d="M12 6.5V3" />
      </svg>
    </div>
    <div class="title">
      <h1>Charon</h1>
      <span>dump · import · clonazione di database</span>
    </div>
  </div>

  <button class="tools-pill" onclick={() => (app.view = "tools")} title="Vai a Strumenti">
    {#each summary as s}
      <span class="chip" class:on={s.native}>
        <span class="dot"></span>{s.short}
      </span>
    {/each}
  </button>
</header>

<style>
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 26px 0;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 14px;
    cursor: pointer;
  }
  .logo {
    width: 46px;
    height: 46px;
    border-radius: 14px;
    display: grid;
    place-items: center;
    color: #fff;
    background: var(--brand-grad);
    box-shadow: 0 8px 20px rgba(99, 102, 241, 0.35);
  }
  .logo svg {
    width: 26px;
    height: 26px;
  }
  .title h1 {
    margin: 0;
    font-size: 21px;
    letter-spacing: -0.3px;
  }
  .title span {
    font-size: 12.5px;
    color: var(--ink-faint);
  }
  .tools-pill {
    display: flex;
    gap: 6px;
    align-items: center;
    background: var(--surface);
    border: 1px solid var(--stroke);
    border-radius: 999px;
    padding: 6px 8px;
    box-shadow: var(--shadow-sm);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-faint);
    padding: 3px 8px;
    border-radius: 999px;
  }
  .chip.on {
    color: var(--ink);
    background: var(--surface-2);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ink-faint);
  }
  .chip.on .dot {
    background: var(--ok);
  }
</style>

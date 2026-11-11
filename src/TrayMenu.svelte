<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import StudioIcon from "./components/StudioIcon.svelte";
  import "./studio.css";

  // Same key the main window writes its theme to (App.svelte). Both windows share localStorage.
  const THEME_KEY = "stellars-theme";
  const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  let allMuted = $state(false);
  let micMuted = $state(false);
  let hasScenes = $state(false);
  let root = $state<HTMLElement | null>(null);

  function applyTheme() {
    let saved: string | null = null;
    try { saved = localStorage.getItem(THEME_KEY); } catch {}
    const theme = saved === "light" ? "light" : "dark";
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme;
  }

  async function refresh() {
    applyTheme();
    if (!isTauri) return;
    try {
      const state = await invoke<{ allMuted: boolean; micMuted: boolean; hasScenes: boolean }>("tray_menu_state");
      allMuted = state.allMuted;
      micMuted = state.micMuted;
      hasScenes = state.hasScenes;
    } catch {}
  }

  function replayEntrance() {
    if (!root) return;
    root.classList.remove("enter");
    void root.offsetWidth;
    root.classList.add("enter");
    root.querySelector<HTMLButtonElement>("button.item")?.focus({ preventScroll: true });
  }

  async function run(action: string) {
    if (!isTauri) return;
    try { await invoke("tray_menu_action", { action }); } catch {}
  }

  function hide() {
    if (isTauri) invoke("tray_menu_hide").catch(() => {});
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      hide();
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const items = Array.from(root?.querySelectorAll<HTMLButtonElement>("button.item") ?? []);
    if (!items.length) return;
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = e.key === "ArrowDown" ? (at + 1) % items.length : (at - 1 + items.length) % items.length;
    items[next].focus();
  }

  onMount(() => {
    refresh();
    const onStorage = (e: StorageEvent) => { if (e.key === THEME_KEY) applyTheme(); };
    window.addEventListener("storage", onStorage);
    window.addEventListener("keydown", onKeydown);
    let unlisten: (() => void) | undefined;
    if (isTauri) {
      listen("tray-menu-shown", () => { refresh(); replayEntrance(); }).then((fn) => { unlisten = fn; });
    }
    return () => {
      window.removeEventListener("storage", onStorage);
      window.removeEventListener("keydown", onKeydown);
      unlisten?.();
    };
  });
</script>

<div class="stage">
  <div class="card enter" bind:this={root} role="menu" aria-label="Stellars">
    <header class="head">
      <img class="logo" src="/favicon.svg" alt="" width="22" height="22" />
      <span class="name">Stellars</span>
      {#if allMuted}<span class="pill">All muted</span>{/if}
    </header>

    <button class="item" role="menuitem" onclick={() => run("open")}>
      <StudioIcon name="external" size={16} /><span>Open Stellars</span>
    </button>
    <button class="item" role="menuitem" onclick={() => run("toggle_mute_all")}>
      <StudioIcon name={allMuted ? "volume" : "mute"} size={16} /><span>{allMuted ? "Unmute all channels" : "Mute all channels"}</span>
    </button>
    <button class="item" role="menuitem" onclick={() => run("toggle_mic_mute")}>
      <StudioIcon name={micMuted ? "volume" : "mute"} size={16} /><span>{micMuted ? "Unmute microphone" : "Mute microphone"}</span>
    </button>
    <button class="item" role="menuitem" disabled={!hasScenes} onclick={() => run("next_scene")}>
      <StudioIcon name="mix" size={16} /><span>{hasScenes ? "Next scene" : "Next scene (none saved)"}</span>
    </button>
    <button class="item" role="menuitem" onclick={() => run("reset_endpoints")}>
      <StudioIcon name="reset" size={16} /><span>Reset app audio endpoints</span>
    </button>
    <button class="item" role="menuitem" onclick={() => run("restore_audio")} title="Stops routing and puts Windows audio back as it was before Stellars">
      <StudioIcon name="headphones" size={16} /><span>Restore Windows audio</span>
    </button>

    <div class="divider" role="separator"></div>

    <button class="item danger" role="menuitem" onclick={() => run("quit")}>
      <StudioIcon name="power" size={16} /><span>Quit Stellars</span>
    </button>
  </div>
</div>

<style>
  /* The window is exactly the card (see tray.rs); no shadow or margin. */
  :global(html), :global(body) {
    background: transparent !important;
    margin: 0;
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
    cursor: default;
  }
  .stage { box-sizing: border-box; width: 100vw; height: 100vh; padding: 0; }
  .card {
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    padding: 6px;
    border-radius: 14px;
    background: var(--panel-raised);
    border: 1px solid var(--line-strong);
    color: var(--text);
    font-size: 12.5px;
  }
  .card.enter { animation: tray-in 0.16s var(--ease, cubic-bezier(0.22, 1, 0.36, 1)); }
  @keyframes tray-in { from { opacity: 0; transform: translateY(4px) scale(0.98); } to { opacity: 1; transform: none; } }

  .head { display: flex; align-items: center; gap: 9px; height: 44px; padding: 0 10px; }
  .logo { border-radius: 6px; flex-shrink: 0; }
  .name { flex: 1; font-size: 13px; font-weight: 700; letter-spacing: 0.01em; }
  .pill {
    padding: 2px 8px; border-radius: 999px; font-size: 10px; font-weight: 600;
    color: var(--violet); background: color-mix(in srgb, var(--violet) 16%, transparent);
  }

  .item {
    display: flex; align-items: center; gap: 10px; width: 100%; height: 36px; padding: 0 10px;
    border: 0; border-radius: 8px; background: transparent; color: var(--text-secondary);
    font: inherit; font-weight: 500; text-align: left; cursor: pointer;
    transition: background-color 0.12s ease, color 0.12s ease;
  }
  .item :global(svg) { flex-shrink: 0; color: var(--text-muted); transition: color 0.12s ease; }
  .item:hover, .item:focus-visible { background: var(--control); color: var(--text); outline: none; }
  .item:hover :global(svg), .item:focus-visible :global(svg) { color: var(--violet); }
  .item:active { background: var(--control-hover); }
  .item:disabled { opacity: 0.45; cursor: default; }
  .item:disabled:hover { background: transparent; color: var(--text-secondary); }
  .item.danger:hover, .item.danger:focus-visible { background: color-mix(in srgb, #e5484d 14%, transparent); color: #e5484d; }
  .item.danger:hover :global(svg), .item.danger:focus-visible :global(svg) { color: #e5484d; }
  :global(:root[data-theme="light"]) .item.danger:hover, :global(:root[data-theme="light"]) .item.danger:focus-visible { color: #c42b31; }
  :global(:root[data-theme="light"]) .item.danger:hover :global(svg), :global(:root[data-theme="light"]) .item.danger:focus-visible :global(svg) { color: #c42b31; }

  .divider { height: 1px; margin: 4px 6px; background: var(--line); }

  @media (prefers-reduced-motion: reduce) { .card.enter { animation: none; } }
</style>

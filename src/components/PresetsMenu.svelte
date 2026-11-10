<script lang="ts">
  import { onMount } from "svelte";
  import StudioIcon from "./StudioIcon.svelte";
  import type { RouterConfig } from "../types";
  import { applyOutputProfile, applyScene, loadPresets, presetStore } from "../presets.svelte";

  interface Props {
    native: boolean;
    onApplied: (config: RouterConfig) => void;
    onNotify: (msg: string) => void;
    onManage: () => void;
  }

  let { native, onApplied, onNotify, onManage }: Props = $props();

  let open = $state(false);
  let busyId = $state<string | null>(null);
  let rootEl = $state<HTMLDivElement>();
  let triggerEl = $state<HTMLButtonElement>();

  const activeScene = $derived(presetStore.scenes.find((s) => s.id === presetStore.activeSceneId));

  async function toggle() {
    open = !open;
    if (open && native) await loadPresets();
  }

  function close(refocus = false) {
    open = false;
    if (refocus) triggerEl?.focus();
  }

  async function choose(kind: "scene" | "profile", id: string, name: string) {
    if (busyId) return;
    busyId = id;
    try {
      onApplied(kind === "scene" ? await applyScene(id) : await applyOutputProfile(id));
      onNotify(kind === "scene" ? `Scene: ${name}` : `Output: ${name}`);
      close(true);
    } catch (err) {
      onNotify(String(err));
    } finally {
      busyId = null;
    }
  }

  onMount(() => {
    const onPointerDown = (e: PointerEvent) => {
      if (open && rootEl && !rootEl.contains(e.target as Node)) close();
    };
    const onKeydown = (e: KeyboardEvent) => {
      if (open && e.key === "Escape") {
        e.stopPropagation();
        close(true);
      }
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeydown, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeydown, true);
    };
  });
</script>

<div class="presets-menu" bind:this={rootEl}>
  <button
    bind:this={triggerEl}
    type="button"
    class="hardware-button presets-trigger"
    aria-haspopup="menu"
    aria-expanded={open}
    data-tooltip="Switch scene or output profile"
    onclick={toggle}
  >
    <StudioIcon name="mix" size={16} />
    <span>{activeScene?.name ?? "Presets"}</span>
    <StudioIcon name="down" size={14} />
  </button>

  {#if open}
    <div class="menu" role="menu" aria-label="Presets">
      {#if !native}
        <p class="empty">Scenes and output profiles need the desktop app.</p>
      {:else}
        <div class="group-label">Scenes</div>
        {#each presetStore.scenes as scene (scene.id)}
          <button type="button" role="menuitemradio" aria-checked={presetStore.activeSceneId === scene.id} class="item" disabled={busyId !== null} onclick={() => choose("scene", scene.id, scene.name)}>
            <span class="check">{#if presetStore.activeSceneId === scene.id}<StudioIcon name="check" size={13} />{/if}</span>
            <span class="label">{scene.name}</span>
            {#if scene.autoApps.length}<span class="auto" data-tooltip={`Auto-applies for ${scene.autoApps.join(", ")}`}>auto</span>{/if}
          </button>
        {:else}
          <p class="empty">No scenes yet.</p>
        {/each}

        <div class="group-label">Output</div>
        {#each presetStore.outputProfiles as profile (profile.id)}
          <button type="button" role="menuitemradio" aria-checked={presetStore.activeProfileId === profile.id} class="item" disabled={busyId !== null} onclick={() => choose("profile", profile.id, profile.name)}>
            <span class="check">{#if presetStore.activeProfileId === profile.id}<StudioIcon name="check" size={13} />{/if}</span>
            <span class="label">{profile.name}</span>
          </button>
        {:else}
          <p class="empty">No output profiles yet.</p>
        {/each}
      {/if}
      <div class="divider"></div>
      <button type="button" role="menuitem" class="item manage" onclick={() => { close(); onManage(); }}>
        <span class="check"><StudioIcon name="settings" size={13} /></span>
        <span class="label">Manage presets…</span>
      </button>
    </div>
  {/if}
</div>

<style>
  .presets-menu { position: relative; }
  .presets-trigger span { max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .menu {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    z-index: 60;
    width: 260px;
    max-height: 360px;
    overflow-y: auto;
    padding: 8px;
    border-radius: 12px;
    background: var(--panel-raised);
    border: 1px solid var(--line-strong);
    box-shadow: 0 18px 44px rgba(0, 0, 0, 0.45);
  }
  .group-label { padding: 8px 10px 4px; font-size: 9.5px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-muted); }
  .item { display: flex; align-items: center; gap: 8px; width: 100%; padding: 8px 10px; border: 0; border-radius: 8px; background: transparent; color: var(--text); font: inherit; font-size: 12px; text-align: left; cursor: pointer; }
  .item:hover:not(:disabled), .item:focus-visible { background: var(--control); outline: none; }
  .item:disabled { opacity: 0.6; cursor: progress; }
  .check { display: grid; place-items: center; width: 16px; color: var(--violet); flex-shrink: 0; }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .auto { padding: 1px 6px; border-radius: 5px; font-size: 9px; color: var(--teal); background: color-mix(in srgb, var(--teal) 14%, transparent); }
  .empty { margin: 0; padding: 6px 10px 8px; font-size: 11px; color: var(--text-muted); }
  .divider { height: 1px; margin: 6px 4px; background: var(--line); }
  .manage { color: var(--text-secondary); }
</style>

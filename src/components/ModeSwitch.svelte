<script lang="ts">
  import { appSettings, updateAppSettings } from "../appSettings.svelte";

  let { label = "Interface mode" }: { label?: string } = $props();

  async function set(mode: "simple" | "advanced") {
    if (appSettings.interfaceMode === mode) return;
    try {
      await updateAppSettings({ interfaceMode: mode, interfaceModeChosen: true });
    } catch (err) {
      console.error("Could not change interface mode:", err);
    }
  }
</script>

<div class="mode-switch" role="group" aria-label={label}>
  <button type="button" aria-pressed={appSettings.interfaceMode === "simple"} data-tooltip="Presets and a few simple controls" onclick={() => set("simple")}>Simple</button>
  <button type="button" aria-pressed={appSettings.interfaceMode === "advanced"} data-tooltip="Every control, graph and number" onclick={() => set("advanced")}>Advanced</button>
</div>

<style>
  .mode-switch { display: inline-flex; padding: 2px; gap: 2px; border-radius: 8px; background: color-mix(in srgb, var(--text) 7%, transparent); }
  button { border: 0; background: transparent; color: var(--text-muted); font: inherit; font-size: 10px; font-weight: 600; padding: 4px 10px; border-radius: 6px; cursor: pointer; min-height: 0; }
  button:hover { color: var(--text); }
  button[aria-pressed="true"] { background: color-mix(in srgb, var(--violet) 26%, transparent); color: var(--text); }
  button:focus-visible { outline: 2px solid var(--violet); outline-offset: 1px; }
</style>

<script lang="ts">
  import StudioSelect from "./StudioSelect.svelte";
  import type { BusId } from "../types";
  import {
    bundleMatches,
    recipeBundle,
    recipesFor,
    type SectionBundle,
  } from "../finePresets";
  import { deleteUserPreset, presetStore, saveUserPreset, userPresetsFor } from "../presets.svelte";

  interface Props {
    bus: BusId;
    current: SectionBundle;
    simple: boolean;
    onApply: (bundle: SectionBundle, label: string, eqPresetId?: string, eqOff?: boolean) => void;
    onNotify?: (msg: string) => void;
  }

  let { bus, current, simple, onApply, onNotify }: Props = $props();

  let naming = $state(false);
  let name = $state("");

  const builtIn = $derived(recipesFor(bus));
  const saved = $derived((presetStore.userPresets, userPresetsFor("recipe", bus)));

  const activeValue = $derived.by(() => {
    const own = saved.find((p) => bundleMatches(current, p.data as SectionBundle));
    if (own) return "user:" + own.id;
    const match = builtIn.find((r) => bundleMatches(current, recipeBundle(r, bus)));
    return match ? "recipe:" + match.id : "custom";
  });
  const activeDescription = $derived(
    activeValue.startsWith("recipe:") ? builtIn.find((r) => "recipe:" + r.id === activeValue)?.description : undefined
  );

  function choose(value: string) {
    if (value.startsWith("recipe:")) {
      const recipe = builtIn.find((r) => "recipe:" + r.id === value);
      if (recipe) onApply(recipeBundle(recipe, bus), "Setup: " + recipe.name, recipe.eq, recipe.eqOff);
    } else if (value.startsWith("user:")) {
      const own = saved.find((p) => "user:" + p.id === value);
      if (own) onApply(own.data as SectionBundle, "Setup: " + own.name);
    }
  }

  async function saveCurrent() {
    const clean = name.trim();
    if (!clean) return;
    try {
      await saveUserPreset("recipe", bus, clean, current);
      onNotify?.("Setup saved as \u201c" + clean + "\u201d");
      naming = false;
      name = "";
    } catch (err) {
      onNotify?.(String(err));
    }
  }

  async function removeActive() {
    const own = saved.find((p) => "user:" + p.id === activeValue);
    if (!own) return;
    try {
      await deleteUserPreset(own.id);
      onNotify?.("Deleted \u201c" + own.name + "\u201d");
    } catch (err) {
      onNotify?.(String(err));
    }
  }
</script>

<div class="quick-setup">
<div class="quick">
  <label class="label" for="quick-setup">Quick setup</label>
  <StudioSelect id="quick-setup" value={activeValue} onchange={(e) => choose(e.currentTarget.value)}>
    <option value="custom" disabled>Custom</option>
    <optgroup label="Ready-made">
      {#each builtIn as r (r.id)}<option value={"recipe:" + r.id}>{r.name}</option>{/each}
    </optgroup>
    {#if saved.length}
      <optgroup label="My setups">
        {#each saved as p (p.id)}<option value={"user:" + p.id}>{p.name}</option>{/each}
      </optgroup>
    {/if}
  </StudioSelect>
  {#if activeValue.startsWith("user:")}
    <button type="button" class="link" onclick={removeActive}>Delete</button>
  {:else if !naming}
    <button type="button" class="link" onclick={() => (naming = true)} data-tooltip="Keep all the settings below as your own setup">Save as&hellip;</button>
  {:else}
    <form class="name-row" onsubmit={(e) => { e.preventDefault(); void saveCurrent(); }}>
      <input type="text" bind:value={name} maxlength="40" placeholder="Name" aria-label="Setup name" />
      <button type="submit" class="link">Save</button>
      <button type="button" class="link" onclick={() => { naming = false; name = ""; }}>Cancel</button>
    </form>
  {/if}
</div>
{#if activeDescription}<p class="note">{activeDescription}</p>{/if}
</div>

<style>
  .quick-setup { grid-column: 1 / -1; display: flex; flex-direction: column; gap: 8px; }
  .quick { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 12px; }
  .label { font-size: 11px; color: var(--text-muted); }
  .quick :global(.studio-select) { min-width: 210px; }
  .note { margin: 0; font-size: 11px; line-height: 1.5; color: var(--text-muted); max-width: none; }
  .link { min-height: 0; padding: 0; border: 0; background: transparent; color: var(--violet); font: inherit; font-size: 11px; cursor: pointer; }
  .link:hover { text-decoration: underline; }
  .name-row { display: flex; gap: 10px; align-items: center; }
  .name-row input { width: 150px; padding: 5px 9px; border-radius: 7px; border: 1px solid var(--line-strong); background: transparent; color: var(--text); font: inherit; font-size: 11px; }
</style>

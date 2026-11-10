<script lang="ts">
  import type { BusId } from "../types";
  import {
    OFF_PRESET_ID,
    SECTION_TITLES,
    sectionMatches,
    sectionPresetsFor,
    type SectionId,
    type SectionValues,
  } from "../finePresets";
  import { deleteUserPreset, presetStore, saveUserPreset, userPresetsFor } from "../presets.svelte";

  interface Props {
    section: SectionId;
    bus: BusId;
    current: SectionValues;
    /** Simple mode shows a "Customize" toggle instead of always-visible sliders. */
    simple: boolean;
    open: boolean;
    onToggleOpen: () => void;
    onApply: (values: SectionValues, label: string) => void;
    onNotify?: (msg: string) => void;
  }

  let { section, bus, current, simple, open, onToggleOpen, onApply, onNotify }: Props = $props();

  let naming = $state(false);
  let name = $state("");
  let busy = $state(false);

  const builtIn = $derived(sectionPresetsFor(section, bus));
  const saved = $derived((presetStore.userPresets, userPresetsFor(section, bus)));
  const title = $derived(SECTION_TITLES[section]);

  interface Chip { id: string; label: string; values: SectionValues; description: string; user: boolean }
  const chips = $derived<Chip[]>([
    { id: OFF_PRESET_ID, label: "Off", values: { enabled: false }, description: "", user: false },
    ...builtIn.map((p) => ({ id: p.id, label: p.name, values: p.values, description: p.description, user: false })),
    ...saved.map((p) => ({ id: p.id, label: p.name, values: p.data as SectionValues, description: "Your saved setting.", user: true })),
  ]);
  const active = $derived(chips.find((c) => sectionMatches(current, c.values)));

  async function saveCurrent() {
    const clean = name.trim();
    if (!clean || busy) return;
    busy = true;
    try {
      await saveUserPreset(section, bus, clean, { ...current, enabled: true });
      onNotify?.(title + " saved as \u201c" + clean + "\u201d");
      naming = false;
      name = "";
    } catch (err) {
      onNotify?.(String(err));
    } finally {
      busy = false;
    }
  }

  async function remove(chip: Chip) {
    try {
      await deleteUserPreset(chip.id);
      onNotify?.("Deleted \u201c" + chip.label + "\u201d");
    } catch (err) {
      onNotify?.(String(err));
    }
  }
</script>

<div class="bar" role="group" aria-label={title + " presets"}>
  <div class="chips">
    {#each chips as chip (chip.id)}
      <span class="chip-wrap">
        <button type="button" class="chip" class:on={active?.id === chip.id} aria-pressed={active?.id === chip.id}
          data-tooltip={chip.description}
          onclick={() => onApply(chip.values, title + ": " + chip.label)}>{chip.label}</button>
        {#if chip.user}
          <button type="button" class="del" aria-label={"Delete " + chip.label} data-tooltip="Delete this saved setting" onclick={() => remove(chip)}>&times;</button>
        {/if}
      </span>
    {/each}
    {#if !active}<span class="chip custom" aria-live="polite">Custom</span>{/if}
  </div>

  {#if !active || active.description}<p class="desc">{active ? active.description : "Your own settings. Pick a preset to go back."}</p>{/if}

  <div class="tools">
    {#if simple}
      <button type="button" class="link" aria-expanded={open} onclick={onToggleOpen}>{open ? "Hide controls" : "Customize"}</button>
    {/if}
    {#if !naming}
      {#if !active || !active.user}<button type="button" class="link" onclick={() => (naming = true)} disabled={current.enabled === false} data-tooltip="Keep these settings as your own preset">Save as&hellip;</button>{/if}
    {:else}
      <form class="name-row" onsubmit={(e) => { e.preventDefault(); void saveCurrent(); }}>
        <input type="text" bind:value={name} maxlength="40" placeholder="Name" aria-label={title + " preset name"} />
        <button type="submit" class="link" disabled={busy}>Save</button>
        <button type="button" class="link" onclick={() => { naming = false; name = ""; }}>Cancel</button>
      </form>
    {/if}
  </div>
</div>

<style>
  .bar { grid-column: 1 / -1; display: flex; flex-direction: column; gap: 8px; margin: 2px 0 2px; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip-wrap { display: inline-flex; align-items: center; }
  .chip { min-height: 0; padding: 6px 12px; border-radius: 999px; border: 1px solid var(--line); background: var(--control); color: var(--text-secondary); font: inherit; font-size: 11px; cursor: pointer; transition: background .15s, border-color .15s, color .15s; }
  .chip:hover { border-color: var(--line-strong); color: var(--text); }
  .chip.on { background: color-mix(in srgb, var(--violet) 22%, var(--control)); border-color: var(--violet); color: var(--text); font-weight: 600; }
  .chip:focus-visible, .link:focus-visible, .del:focus-visible { outline: 2px solid var(--violet); outline-offset: 2px; }
  .chip.custom { cursor: default; border-style: dashed; background: transparent; color: var(--amber, #edc990); }
  .del { margin-left: -6px; min-height: 0; width: 18px; height: 18px; padding: 0; display: grid; place-items: center; border: 0; border-radius: 50%; background: transparent; color: var(--text-muted); font-size: 13px; cursor: pointer; }
  .del:hover { color: var(--text); background: var(--control); }
  .desc { margin: 0; font-size: 11px; line-height: 1.5; color: var(--text-muted); max-width: none; }
  .tools { display: flex; flex-wrap: wrap; gap: 14px; align-items: center; }
  .link { min-height: 0; padding: 0; border: 0; background: transparent; color: var(--violet); font: inherit; font-size: 11px; cursor: pointer; }
  .link:hover:not(:disabled) { text-decoration: underline; }
  .link:disabled { opacity: .4; cursor: not-allowed; }
  .name-row { display: flex; gap: 10px; align-items: center; }
  .name-row input { width: 150px; padding: 5px 9px; border-radius: 7px; border: 1px solid var(--line-strong); background: transparent; color: var(--text); font: inherit; font-size: 11px; }
</style>

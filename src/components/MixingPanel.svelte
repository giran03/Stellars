<script lang="ts">
  import ToggleRow from "./ToggleRow.svelte";
  import { BUS_COLORS, type BusId, type DuckingConfig } from "../types";

  interface Props {
    ducking: DuckingConfig;
    /** Current ducking gain from the engine (1 = not ducking). */
    duckGain: number;
    onChange: (next: DuckingConfig) => void;
  }

  let { ducking, duckGain, onChange }: Props = $props();

  const TRIGGER_BUSES: BusId[] = ["Game", "Chat", "Media", "Aux", "Mic"];
  const TARGET_BUSES: BusId[] = ["Game", "Chat", "Media", "Aux"];

  const SLIDERS = [
    { key: "amountDb", label: "Reduce by", min: -30, max: 0, step: 1, unit: "dB", hint: "How far the target channels are lowered." },
    { key: "thresholdDb", label: "Trigger level", min: -70, max: -10, step: 1, unit: "dB", hint: "How loud a trigger channel must be to start ducking." },
    { key: "attackMs", label: "Attack", min: 5, max: 500, step: 5, unit: "ms", hint: "How quickly the targets lower." },
    { key: "releaseMs", label: "Release", min: 50, max: 3000, step: 50, unit: "ms", hint: "How slowly they come back." },
  ] as const;

  const reductionDb = $derived(duckGain > 0.001 ? 20 * Math.log10(duckGain) : -60);
  const isDucking = $derived(ducking.enabled && duckGain < 0.97);

  function toggleBus(list: "triggers" | "targets", bus: BusId) {
    const current = ducking[list];
    const next = current.includes(bus) ? current.filter((b) => b !== bus) : [...current, bus];
    const patch: DuckingConfig = { ...ducking, [list]: next };
    // A channel can't both trigger and be ducked.
    if (list === "triggers" && next.includes(bus)) patch.targets = patch.targets.filter((b) => b !== bus);
    if (list === "targets" && next.includes(bus)) patch.triggers = patch.triggers.filter((b) => b !== bus);
    onChange(patch);
  }
</script>

<div class="mixing">
  <ToggleRow
    title="Auto-ducking"
    desc="Lower game and music automatically while someone is talking, then bring them back."
    checked={ducking.enabled}
    onchange={(enabled) => onChange({ ...ducking, enabled })}
  />

  <div class="status" class:active={isDucking} role="status" aria-live="polite">
    <span class="dot"></span>
    {#if !ducking.enabled}
      Ducking is off.
    {:else if isDucking}
      Ducking now: {reductionDb.toFixed(1)} dB
    {:else}
      Waiting for a trigger channel to get loud.
    {/if}
  </div>

  <div class="bus-pickers" class:is-off={!ducking.enabled}>
    <fieldset>
      <legend>When these are active…</legend>
      <div class="chips">
        {#each TRIGGER_BUSES as bus}
          <button
            type="button"
            class="chip"
            aria-pressed={ducking.triggers.includes(bus)}
            style={`--chip-color:${BUS_COLORS[bus]}`}
            onclick={() => toggleBus("triggers", bus)}
          >{bus}</button>
        {/each}
      </div>
    </fieldset>
    <fieldset>
      <legend>…lower these</legend>
      <div class="chips">
        {#each TARGET_BUSES as bus}
          <button
            type="button"
            class="chip"
            aria-pressed={ducking.targets.includes(bus)}
            style={`--chip-color:${BUS_COLORS[bus]}`}
            onclick={() => toggleBus("targets", bus)}
          >{bus}</button>
        {/each}
      </div>
    </fieldset>
  </div>

  <div class="sliders" class:is-off={!ducking.enabled}>
    {#each SLIDERS as s}
      <label class="slider">
        <span class="slider-head"><strong>{s.label}</strong><output>{ducking[s.key]} {s.unit}</output></span>
        <input
          type="range"
          class="horizontal-range"
          min={s.min}
          max={s.max}
          step={s.step}
          value={ducking[s.key]}
          disabled={!ducking.enabled}
          aria-label={s.label}
          oninput={(e) => onChange({ ...ducking, [s.key]: Number(e.currentTarget.value) })}
        />
        <small>{s.hint}</small>
      </label>
    {/each}
  </div>
</div>

<style>
  .mixing { display: flex; flex-direction: column; gap: 14px; }
  .status { display: flex; align-items: center; gap: 9px; padding: 10px 14px; border-radius: 10px; font-size: 11.5px; color: var(--text-muted); background: var(--panel-inset); border: 1px solid var(--line); }
  .status .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--text-muted); }
  .status.active { color: var(--teal); border-color: color-mix(in srgb, var(--teal) 35%, transparent); }
  .status.active .dot { background: var(--teal); }

  .bus-pickers { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .bus-pickers fieldset { margin: 0; padding: 12px 14px; border-radius: 11px; border: 1px solid var(--line); background: var(--panel-raised); min-width: 0; }
  .bus-pickers legend { padding: 0 6px; font-size: 10px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-muted); }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { padding: 6px 12px; border-radius: 999px; border: 1px solid var(--line-strong); background: transparent; color: var(--text-muted); font: inherit; font-size: 11px; font-weight: 500; cursor: pointer; transition: background-color 140ms ease, color 140ms ease, border-color 140ms ease; }
  .chip:hover { color: var(--text); }
  .chip[aria-pressed="true"] { color: var(--chip-color); border-color: var(--chip-color); background: color-mix(in srgb, var(--chip-color) 16%, transparent); }
  .chip:focus-visible { outline: 2px solid var(--violet); outline-offset: 2px; }

  .sliders { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .slider { display: flex; flex-direction: column; gap: 6px; padding: 12px 14px; border-radius: 11px; border: 1px solid var(--line); background: var(--panel-raised); }
  .slider-head { display: flex; justify-content: space-between; align-items: baseline; font-size: 12px; }
  .slider-head output { font-variant-numeric: tabular-nums; color: var(--text-secondary); font-size: 11px; }
  .slider small { font-size: 10px; color: var(--text-muted); line-height: 1.4; }
  .is-off { opacity: 0.55; }
  @media (max-width: 760px) { .bus-pickers, .sliders { grid-template-columns: 1fr; } }
</style>

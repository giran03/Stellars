<script lang="ts">
  import StudioIcon from "./StudioIcon.svelte";
  import type { InterfaceMode } from "../appSettings.svelte";

  interface Props {
    value: InterfaceMode;
    onchoose: (mode: InterfaceMode) => void;
    /** Show only the radio cards without the bullet points and footnote. */
    compact?: boolean;
  }

  let { value, onchoose, compact = false }: Props = $props();

  const OPTIONS: { mode: InterfaceMode; title: string; icon: string; desc: string; points: string[] }[] = [
    {
      mode: "simple",
      title: "Simple",
      icon: "headphones",
      desc: "Ready-made sound settings. Pick a preset and go.",
      points: ["One-click presets for voice, game and music", "Bass, voice and treble sliders", "No technical jargon"],
    },
    {
      mode: "advanced",
      title: "Advanced",
      icon: "wrench",
      desc: "Every control, graph and number.",
      points: ["Full 10-band EQ graph", "Gate, compressor and limiter sliders", "Peak, LUFS and diagnostics"],
    },
  ];
</script>

<div class="mode-choice" role="radiogroup" aria-label="Interface mode">
  {#each OPTIONS as o (o.mode)}
    <button type="button" role="radio" aria-checked={value === o.mode} class="card" class:chosen={value === o.mode} onclick={() => onchoose(o.mode)}>
      <span class="head"><StudioIcon name={o.icon} size={18} /><strong>{o.title}</strong>{#if o.mode === "simple"}<em>Recommended</em>{/if}</span>
      <span class="desc">{o.desc}</span>
      {#if !compact}
        <ul>{#each o.points as p}<li>{p}</li>{/each}</ul>
      {/if}
    </button>
  {/each}
</div>
{#if !compact}<p class="foot">Switching never changes your sound. It only shows or hides controls, and you can change it any time in Settings → Appearance.</p>{/if}

<style>
  .mode-choice { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .card { display: flex; flex-direction: column; gap: 8px; align-items: flex-start; padding: 16px; text-align: left; border-radius: 12px; border: 1px solid var(--line); background: var(--panel-raised); color: var(--text); font: inherit; cursor: pointer; transition: border-color .15s, background .15s; }
  .card:hover { border-color: var(--line-strong); }
  .card:focus-visible { outline: 2px solid var(--violet); outline-offset: 2px; }
  .card.chosen { border-color: var(--violet); background: color-mix(in srgb, var(--violet) 12%, var(--panel-raised)); }
  .head { display: flex; align-items: center; gap: 8px; color: var(--violet); }
  .head strong { color: var(--text); font-size: 14px; }
  .head em { font-style: normal; font-size: 9px; padding: 2px 7px; border-radius: 6px; color: var(--teal); background: color-mix(in srgb, var(--teal) 14%, transparent); }
  .desc { font-size: 12px; color: var(--text-secondary); line-height: 1.5; }
  ul { margin: 4px 0 0; padding: 0 0 0 16px; font-size: 11px; color: var(--text-muted); line-height: 1.7; }
  .foot { margin: 12px 0 0; font-size: 11px; color: var(--text-muted); line-height: 1.6; }
  @media (max-width: 560px) { .mode-choice { grid-template-columns: 1fr; } }
</style>

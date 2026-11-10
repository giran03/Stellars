<script lang="ts" module>
  export type StageId = "noise" | "eq" | "deEsser" | "gate" | "comp" | "limiter";
</script>

<script lang="ts">
  import type { BusConfig, BusId, StereoMeterSnapshot } from "../types";
  import { compressorOutputDb, gateOutputDb } from "../dspMath";

  interface Props {
    bus: BusId;
    config: BusConfig;
    meter: StereoMeterSnapshot;
    accentColor: string;
    native: boolean;
    onToggle: (stage: StageId) => void;
  }

  let { bus, config, meter, accentColor, native, onToggle }: Props = $props();

  const MAX_GR_DB = 18;
  const CURVE_MIN = -60;
  const CURVE_MAX = 6;
  const SIZE = 120;

  const reduction = $derived(meter.reduction ?? { gateDb: 0, compDb: 0, deEsserDb: 0, limiterDb: 0 });
  const eqOn = $derived(config.eqBands.some((b) => b.enabled));
  const noise = $derived(config.noiseSuppression);
  const deEsser = $derived(config.deEsser);
  const limiter = $derived(config.limiter);
  const hasNoise = $derived(bus === "Mic" || bus === "Chat");

  const toDb = (linear: number) => 20 * Math.log10(Math.max(linear, 1e-6));
  const fmtDb = (db: number) => `${db >= 0 ? "+" : "−"}${Math.abs(db).toFixed(1)} dB`;
  const faderLabel = $derived(config.muted ? "Muted" : config.volume <= 0.001 ? "−∞ dB" : fmtDb(toDb(config.volume)));

  interface Stage {
    id: StageId | "pre" | "fader";
    name: string;
    on: boolean;
    toggle: boolean;
    detail: string;
    gr?: number;
  }

  const stages = $derived<Stage[]>([
    { id: "pre", name: "Pre-gain", on: true, toggle: false, detail: fmtDb(config.preGainDb) },
    ...(hasNoise
      ? [{ id: "noise" as const, name: "Noise cut", on: !!noise?.enabled, toggle: true, detail: noise?.enabled ? `${Math.round((noise.strength ?? 1) * 100)}%` : "Off" }]
      : []),
    { id: "eq", name: "EQ", on: eqOn, toggle: true, detail: eqOn ? `${config.eqBands.filter((b) => b.enabled).length} bands` : "Bypassed" },
    { id: "deEsser", name: "De-esser", on: !!deEsser?.enabled, toggle: true, detail: deEsser?.enabled ? "" : "Off", gr: deEsser?.enabled ? reduction.deEsserDb : undefined },
    { id: "gate", name: "Gate", on: config.noiseGate.enabled, toggle: true, detail: config.noiseGate.enabled ? "" : "Off", gr: config.noiseGate.enabled ? reduction.gateDb : undefined },
    { id: "comp", name: "Compressor", on: config.compressor.enabled, toggle: true, detail: config.compressor.enabled ? "" : "Off", gr: config.compressor.enabled ? reduction.compDb : undefined },
    { id: "fader", name: "Fader", on: !config.muted, toggle: false, detail: faderLabel },
    { id: "limiter", name: "Limiter", on: !!limiter?.enabled, toggle: true, detail: limiter?.enabled ? "" : "Off", gr: limiter?.enabled ? reduction.limiterDb : undefined },
  ]);

  const peakDb = $derived(toDb(Math.max(meter.peakLeft, meter.peakRight)));

  const x = (db: number) => ((Math.min(CURVE_MAX, Math.max(CURVE_MIN, db)) - CURVE_MIN) / (CURVE_MAX - CURVE_MIN)) * SIZE;
  const y = (db: number) => SIZE - ((Math.min(CURVE_MAX, Math.max(CURVE_MIN, db)) - CURVE_MIN) / (CURVE_MAX - CURVE_MIN)) * SIZE;

  function path(fn: (db: number) => number): string {
    const pts: string[] = [];
    for (let db = CURVE_MIN; db <= CURVE_MAX; db += 1) pts.push(`${x(db).toFixed(1)},${y(fn(db)).toFixed(1)}`);
    return pts.join(" ");
  }

  const compPath = $derived(path((db) => compressorOutputDb(db, config.compressor)));
  const gatePath = $derived(path((db) => gateOutputDb(db, config.noiseGate)));
  const dotX = $derived(x(peakDb));
  const gridDbs = [-48, -36, -24, -12, 0];

  const curves = $derived([
    { key: "gate", title: "Gate", on: config.noiseGate.enabled, d: gatePath, thr: config.noiseGate.thresholdDb, a: `Threshold ${config.noiseGate.thresholdDb} dB`, b: `Ratio ${config.noiseGate.ratio}:1`, gr: reduction.gateDb, out: (db: number) => gateOutputDb(db, config.noiseGate) },
    { key: "comp", title: "Compressor", on: config.compressor.enabled, d: compPath, thr: config.compressor.thresholdDb, a: `Threshold ${config.compressor.thresholdDb} dB`, b: `Ratio ${config.compressor.ratio}:1 · Knee ${config.compressor.kneeDb} dB`, gr: reduction.compDb, out: (db: number) => compressorOutputDb(db, config.compressor) },
  ]);
</script>

<section class="chain panel" aria-label={`${bus} channel processing`} style={`--accent:${accentColor}`}>
  <div class="section-heading"><h2>Channel processing</h2><span style={`color:${accentColor}`}>{bus}</span></div>

  <ol class="stages" aria-label="Signal flow, left to right">
    {#each stages as stage, i (stage.id)}
      <li class="stage-wrap">
        {#if stage.toggle}
          <button type="button" class="stage" class:on={stage.on} aria-pressed={stage.on}
            data-tooltip={`${stage.name}: ${stage.on ? "on" : "off"} · click to ${stage.on ? "turn off" : "turn on"}`}
            onclick={() => onToggle(stage.id as StageId)}>
            <span class="name">{stage.name}</span>
            {#if stage.gr !== undefined}
              <span class="gr" class:active={stage.gr > 0.1}>{stage.gr > 0.05 ? `−${stage.gr.toFixed(1)} dB` : "0 dB"}</span>
              <span class="bar" aria-hidden="true"><i style:width={`${Math.min(100, (stage.gr / MAX_GR_DB) * 100)}%`}></i></span>
            {:else}
              <span class="detail">{stage.detail}</span>
            {/if}
          </button>
        {:else}
          <div class="stage fixed" class:on={stage.on}>
            <span class="name">{stage.name}</span>
            <span class="detail">{stage.detail}</span>
          </div>
        {/if}
        {#if i < stages.length - 1}<span class="arrow" aria-hidden="true">›</span>{/if}
      </li>
    {/each}
  </ol>

  <div class="curves">
    {#each curves as c (c.key)}
      <figure class="curve" class:off={!c.on}>
        <svg viewBox={`0 0 ${SIZE} ${SIZE}`} role="img" aria-label={`${c.title} transfer curve, ${c.a}, ${c.b}`}>
          {#each gridDbs as g}
            <line x1={x(g)} y1="0" x2={x(g)} y2={SIZE} class="grid" />
            <line x1="0" y1={y(g)} x2={SIZE} y2={y(g)} class="grid" />
          {/each}
          <line x1={x(CURVE_MIN)} y1={y(CURVE_MIN)} x2={x(CURVE_MAX)} y2={y(CURVE_MAX)} class="unity" />
          <line x1={x(c.thr)} y1="0" x2={x(c.thr)} y2={SIZE} class="threshold" />
          <polyline points={c.d} class="trace" />
          {#if c.on && native}<circle cx={dotX} cy={y(c.out(peakDb))} r="3.4" class="live" />{/if}
        </svg>
        <figcaption>
          <strong>{c.title}</strong>
          <span>{c.on ? c.a : "Off · turn it on above"}</span>
          {#if c.on}<span>{c.b}</span>{/if}
          {#if c.on && native}<em class:active={c.gr > 0.1}>{c.gr > 0.05 ? `Reducing ${c.gr.toFixed(1)} dB` : "Not reducing"}</em>{/if}
        </figcaption>
      </figure>
    {/each}
    <p class="note">Curves plot input level (across) against output level (up). The dot follows this channel's live peak.</p>
  </div>
</section>

<style>
  .chain { display: flex; flex-direction: column; gap: 18px; padding: 22px 23px; min-width: 0; }
  .chain :global(.section-heading) { margin-bottom: 0; }
  .stages { list-style: none; margin: 0; padding: 0 0 4px; display: flex; align-items: stretch; gap: 0; overflow-x: auto; }
  .stage-wrap { display: flex; align-items: center; flex: 1 1 0; min-width: 84px; }
  .arrow { color: var(--text-muted); padding: 0 4px; font-size: 16px; flex-shrink: 0; }
  .stage {
    flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; align-items: flex-start;
    padding: 10px 11px; border-radius: 10px; border: 1px solid var(--line); background: var(--control);
    color: var(--text-muted); font: inherit; text-align: left; transition: border-color .15s, background .15s;
  }
  button.stage { cursor: pointer; }
  button.stage:hover { border-color: var(--line-strong); }
  button.stage:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .stage.on { color: var(--text); border-color: color-mix(in srgb, var(--accent) 45%, var(--line)); background: color-mix(in srgb, var(--accent) 9%, var(--control)); }
  .stage.fixed { cursor: default; }
  .name { font-size: 11px; font-weight: 600; letter-spacing: -.01em; white-space: nowrap; }
  .detail, .gr { font-size: 10px; font-variant-numeric: tabular-nums; color: var(--text-muted); white-space: nowrap; }
  .stage.on .detail { color: var(--text-secondary); }
  .gr.active { color: var(--amber, #edc990); }
  .bar { width: 100%; height: 3px; border-radius: 3px; background: color-mix(in srgb, var(--text) 10%, transparent); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--amber, #edc990); transition: width 90ms linear; }

  .curves { display: grid; grid-template-columns: 1fr 1fr; gap: 14px 20px; }
  .curve { margin: 0; display: grid; grid-template-columns: 112px minmax(0, 1fr); gap: 14px; align-items: center; padding: 14px; border-radius: 12px; border: 1px solid var(--line); background: color-mix(in srgb, var(--text) 3%, transparent); }
  .curve.off { opacity: .55; }
  svg { width: 112px; height: 112px; border-radius: 8px; background: color-mix(in srgb, var(--text) 5%, transparent); overflow: visible; }
  .grid { stroke: color-mix(in srgb, var(--text) 8%, transparent); stroke-width: .6; }
  .unity { stroke: color-mix(in srgb, var(--text) 25%, transparent); stroke-width: .8; stroke-dasharray: 3 3; }
  .threshold { stroke: color-mix(in srgb, var(--accent) 55%, transparent); stroke-width: .8; stroke-dasharray: 2 2; }
  .trace { fill: none; stroke: var(--accent); stroke-width: 1.8; stroke-linejoin: round; stroke-linecap: round; }
  .live { fill: var(--text); stroke: var(--accent); stroke-width: 1.5; }
  figcaption { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  figcaption strong { font-size: 12px; }
  figcaption span, figcaption em { font-size: 10px; color: var(--text-muted); font-style: normal; }
  figcaption em.active { color: var(--amber, #edc990); }
  .note { grid-column: 1 / -1; margin: 0; font-size: 10px; color: var(--text-muted); }

  :global(:root[data-mode="simple"]) .curves { display: none; }
  @media (max-width: 1100px) { .curves { grid-template-columns: 1fr; } }
  @media (prefers-reduced-motion: reduce) { .bar i { transition: none; } }
</style>

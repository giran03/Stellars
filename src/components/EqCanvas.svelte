<script lang="ts">
  import StudioSelect from './StudioSelect.svelte';
  import EditableValue from './EditableValue.svelte';
  import StudioIcon from './StudioIcon.svelte';
  import { onMount } from "svelte";
  import ModeSwitch from "./ModeSwitch.svelte";
  import { appSettings } from "../appSettings.svelte";
  import { deleteUserPreset, presetStore, saveUserPreset, userPresetsFor } from "../presets.svelte";
  import type { BusId, EqBandConfig, FilterType } from "../types";
  import {
    computeBiquadCoeffs,
    evaluateBandMagnitudeDb,
    getEqPresetsForBus,
    type EqPreset,
  } from "../dspMath";

  interface Props {
    bus: BusId;
    bands: EqBandConfig[];
    accentColor?: string;
    onUpdateBand: (bandIndex: number, updated: EqBandConfig) => void;
    onResetFlat: () => void;
    onResetDefaults?: () => void;
    onToggleGlobalEq?: (enabled: boolean) => void;
    onApplyPreset?: (preset: EqPreset) => void;
  }

  let {
    bus,
    bands,
    accentColor = "#00F0FF",
    onUpdateBand,
    onResetFlat,
    onResetDefaults,
    onToggleGlobalEq,
    onApplyPreset,
  }: Props = $props();

  let canvasEl: HTMLCanvasElement | undefined = $state();
  let selectedBand = $state(0);
  let hoveredBand: number | null = $state(null);
  let draggingBand: number | null = $state(null);
  let cursorHud: { freq: number; db: number } | null = $state(null);

  // If at least one band is active, EQ is considered enabled; otherwise bypassed.
  let isEqEnabled = $derived(bands.some((b) => b.enabled));
  let availablePresets = $derived(getEqPresetsForBus(bus));
  let savedBandStates: boolean[] = $state([]);

  function toggleGlobalEq() {
    const next = !isEqEnabled;
    if (!next) {
      savedBandStates = bands.map((b) => b.enabled);
    }
    if (onToggleGlobalEq) {
      onToggleGlobalEq(next);
    } else {
      bands.forEach((b, idx) => {
        const shouldEnable = next ? (savedBandStates[idx] ?? true) : false;
        onUpdateBand(idx, { ...b, enabled: shouldEnable });
      });
    }
  }

  function handleResetDefaults() {
    if (onResetDefaults) {
      onResetDefaults();
    } else {
      onResetFlat();
    }
  }

  // ── A/B compare + custom presets ──────────────────────────────────────────
  interface UserEqPreset { id: string; name: string; bands: EqBandConfig[] }
  const MAX_USER_PRESETS = 30;
  const cloneBands = (list: EqBandConfig[]): EqBandConfig[] => list.map((b) => ({ ...b }));
  const sameBands = (a: EqBandConfig[], b: EqBandConfig[]) =>
    a.length === b.length && a.every((x, i) => x.enabled === b[i].enabled && x.type === b[i].type
      && Math.abs(x.freq - b[i].freq) < 0.05 && Math.abs(x.gainDb - b[i].gainDb) < 0.01 && Math.abs(x.q - b[i].q) < 0.005);

  /** Saved EQ presets for this channel (stored with the other presets, so backups include them). */
  const userPresets = $derived<UserEqPreset[]>(
    (presetStore.userPresets, userPresetsFor("eq", bus))
      .map((p) => ({ id: p.id, name: p.name, bands: (p.data as { bands?: EqBandConfig[] })?.bands ?? [] }))
      .filter((p) => p.bands.length === 10)
  );
  let abSlots = $state<[EqBandConfig[] | null, EqBandConfig[] | null]>([null, null]);
  let abActive = $state<0 | 1>(0);
  let namingPreset = $state(false);
  let presetName = $state("");
  let presetNotice = $state("");

  // A/B memory restarts when another channel is selected.
  $effect(() => {
    void bus;
    abSlots = [null, null];
    abActive = 0;
    namingPreset = false;
    presetNotice = "";
  });

  // One-time move of presets saved by an earlier build in this browser profile.
  $effect(() => {
    const key = "stellars.eqPresets.v1." + bus;
    let legacy: { name?: string; bands?: EqBandConfig[] }[] = [];
    try {
      legacy = JSON.parse(localStorage.getItem(key) ?? "[]");
    } catch {
      return;
    }
    if (!Array.isArray(legacy) || legacy.length === 0) return;
    void (async () => {
      try {
        for (const old of legacy) {
          if (old?.name && Array.isArray(old.bands) && old.bands.length === 10) await saveUserPreset("eq", bus, old.name, { bands: old.bands });
        }
        localStorage.removeItem(key);
      } catch {
        /* keep the old copy and try again next time */
      }
    })();
  });

  function applyBands(next: EqBandConfig[]) {
    next.forEach((band, idx) => {
      if (bands[idx] && !sameBands([bands[idx]], [band])) onUpdateBand(idx, { ...band });
    });
  }

  function switchAB(target: 0 | 1) {
    if (target === abActive || !isEqEnabled) return;
    abSlots[abActive] = cloneBands(bands);
    const destination = abSlots[target] ?? cloneBands(bands);
    abSlots[target] = cloneBands(destination);
    abActive = target;
    applyBands(destination);
  }

  const activeUserPreset = $derived(userPresets.find((p) => sameBands(p.bands, bands)));
  const presetValue = $derived(
    activeUserPreset
      ? `user:${activeUserPreset.id}`
      : availablePresets.find((p) => bands.length === p.gains.length && bands.every((b, i) => b.enabled && Math.abs(b.gainDb - p.gains[i]) < 0.01))?.id ?? "custom");

  function handlePresetChange(value: string) {
    if (value.startsWith("user:")) {
      const preset = userPresets.find((p) => `user:${p.id}` === value);
      if (preset) applyBands(preset.bands);
      return;
    }
    const builtin = availablePresets.find((p) => p.id === value);
    if (builtin) onApplyPreset?.(builtin);
  }

  async function saveEqPreset() {
    const name = presetName.trim().slice(0, 32);
    if (!name) { presetNotice = "Give the preset a name."; return; }
    const exists = userPresets.some((p) => p.name.toLowerCase() === name.toLowerCase());
    if (!exists && userPresets.length >= MAX_USER_PRESETS) { presetNotice = "You can keep up to " + MAX_USER_PRESETS + " presets per channel."; return; }
    try {
      await saveUserPreset("eq", bus, name, { bands: cloneBands(bands) });
      presetNotice = exists ? "Updated \u201c" + name + "\u201d" : "Saved \u201c" + name + "\u201d";
      namingPreset = false;
      presetName = "";
    } catch (err) {
      presetNotice = String(err);
    }
  }

  async function deleteEqPreset() {
    const target = activeUserPreset;
    if (!target) return;
    try {
      await deleteUserPreset(target.id);
      presetNotice = "Deleted \u201c" + target.name + "\u201d";
    } catch (err) {
      presetNotice = String(err);
    }
  }

  // ── Simple mode: three plain tone sliders ────────────────────────────────────
  const simple = $derived(appSettings.interfaceMode === "simple");
  const TONE_GROUPS: { id: string; label: string; hint: string; bands: number[] }[] = [
    { id: "bass", label: "Bass", hint: "Deep sounds: explosions, music, the rumble of a voice", bands: [0, 1, 2] },
    { id: "voice", label: "Voice clarity", hint: "Makes voices and footsteps easier to pick out", bands: [5, 6, 7] },
    { id: "treble", label: "Treble", hint: "Crisp, bright sounds: cymbals, \u201cs\u201d sounds, air", bands: [8, 9] },
  ];
  const TONE_RANGE_DB = 9;
  const groupGain = (group: number[]) =>
    Math.round((group.reduce((sum, i) => sum + (bands[i]?.gainDb ?? 0), 0) / group.length) * 2) / 2;

  function setToneGroup(group: number[], target: number) {
    if (!isEqEnabled) return;
    const delta = target - group.reduce((sum, i) => sum + (bands[i]?.gainDb ?? 0), 0) / group.length;
    for (const i of group) {
      const band = bands[i];
      if (band) onUpdateBand(i, { ...band, gainDb: Math.max(-12, Math.min(12, Math.round((band.gainDb + delta) * 10) / 10)) });
    }
  }

  const fmtTone = (db: number) => (db > 0 ? "+" + db : db < 0 ? "\u2212" + Math.abs(db) : "0") + " dB";

  const MIN_FREQ = 20.0;
  const MAX_FREQ = 20000.0;
  const MAX_DB = 15.0;
  const PAD_LEFT = 46;
  const PAD_RIGHT = 18;
  const PAD_TOP = 22;
  const PAD_BOTTOM = 24;

  const GRID_FREQS = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000];
  const GRID_DBS = [-12, -6, 0, 6, 12];
  const FILTER_TYPES: FilterType[] = ["LowShelf", "Peaking", "HighShelf", "LowPass", "HighPass"];

  const SPECTRUM_BANDS = [
    { label: "SUB", fStart: 20, fEnd: 60 },
    { label: "BASS", fStart: 60, fEnd: 250 },
    { label: "LOW MID", fStart: 250, fEnd: 500 },
    { label: "MID", fStart: 500, fEnd: 2000 },
    { label: "PRESENCE", fStart: 2000, fEnd: 6000 },
    { label: "AIR", fStart: 6000, fEnd: 20000 },
  ];

  function freqToX(freq: number, width: number): number {
    const plotW = width - PAD_LEFT - PAD_RIGHT;
    const norm = Math.log10(freq / MIN_FREQ) / Math.log10(MAX_FREQ / MIN_FREQ);
    return PAD_LEFT + Math.max(0, Math.min(1, norm)) * plotW;
  }

  function xToFreq(x: number, width: number): number {
    const plotW = Math.max(1, width - PAD_LEFT - PAD_RIGHT);
    const norm = Math.max(0, Math.min(1, (x - PAD_LEFT) / plotW));
    return MIN_FREQ * Math.pow(MAX_FREQ / MIN_FREQ, norm);
  }

  function dbToY(db: number, height: number): number {
    const plotH = height - PAD_TOP - PAD_BOTTOM;
    const clamped = Math.max(-MAX_DB, Math.min(MAX_DB, db));
    const norm = (MAX_DB - clamped) / (2.0 * MAX_DB);
    return PAD_TOP + norm * plotH;
  }

  function yToDb(y: number, height: number): number {
    const plotH = Math.max(1, height - PAD_TOP - PAD_BOTTOM);
    const norm = Math.max(0, Math.min(1, (y - PAD_TOP) / plotH));
    const db = MAX_DB - norm * (2.0 * MAX_DB);
    return Math.max(-12.0, Math.min(12.0, db));
  }

  function formatFreq(f: number): string {
    if (f >= 1000) return `${(f / 1000).toFixed(f >= 10000 ? 1 : 2)} kHz`;
    return `${Math.round(f)} Hz`;
  }

  function drawEqGraph() {
    if (!canvasEl) return;
    const ctx = canvasEl.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const rect = canvasEl.getBoundingClientRect();
    const width = Math.max(1, rect.width);
    const height = Math.max(132, rect.height);

    if (canvasEl.width !== Math.round(width * dpr) || canvasEl.height !== Math.round(height * dpr)) {
      canvasEl.width = Math.round(width * dpr);
      canvasEl.height = Math.round(height * dpr);
    }

    ctx.save();
    ctx.scale(dpr, dpr);

    const isLight = typeof document !== "undefined" && document.documentElement.dataset.theme === "light";

    // Recessed Clean Laboratory (Light) or Obsidian (Dark) Background
    ctx.fillStyle = isLight ? "#fbfafc" : "#1c1c20";
    ctx.fillRect(0, 0, width, height);

    // Acoustic Spectrum Zone Labels at top
    ctx.font = "600 9px 'Plus Jakarta Sans', sans-serif";
    ctx.textAlign = "center";
    for (const sb of SPECTRUM_BANDS) {
      const x1 = freqToX(sb.fStart, width);
      const x2 = freqToX(sb.fEnd, width);
      ctx.fillStyle = isLight ? "rgba(90, 78, 115, 0.58)" : "rgba(175, 161, 192, 0.42)";
      ctx.fillText(sb.label, (x1 + x2) * 0.5, 13);
    }

    // Logarithmic Frequency Grid Lines
    ctx.lineWidth = 1;
    ctx.font = "500 10px 'Plus Jakarta Sans', sans-serif";
    ctx.textAlign = "center";

    for (const gf of GRID_FREQS) {
      const x = Math.round(freqToX(gf, width)) + 0.5;
      ctx.strokeStyle = gf === 1000
        ? (isLight ? "rgba(90, 78, 115, 0.22)" : "rgba(175, 161, 192, 0.28)")
        : (isLight ? "rgba(90, 78, 115, 0.08)" : "rgba(175, 161, 192, 0.09)");
      ctx.beginPath();
      ctx.moveTo(x, PAD_TOP);
      ctx.lineTo(x, height - PAD_BOTTOM);
      ctx.stroke();

      ctx.fillStyle = isLight ? "#5e586d" : "#aaa0b8";
      const label = gf >= 1000 ? `${gf / 1000}k` : `${gf}`;
      ctx.fillText(label, x, height - 8);
    }

    // dB Horizontal Rules & 0dB Unity Reference
    ctx.textAlign = "right";
    for (const gdb of GRID_DBS) {
      const y = Math.round(dbToY(gdb, height)) + 0.5;
      ctx.strokeStyle = gdb === 0
        ? (isLight ? "rgba(60, 48, 85, 0.35)" : "rgba(226, 232, 240, 0.36)")
        : (isLight ? "rgba(90, 78, 115, 0.08)" : "rgba(175, 161, 192, 0.09)");
      ctx.beginPath();
      ctx.moveTo(PAD_LEFT, y);
      ctx.lineTo(width - PAD_RIGHT, y);
      ctx.stroke();

      ctx.fillStyle = gdb === 0 ? (isLight ? "#2a2438" : "#e2e8f0") : (isLight ? "#5e586d" : "#aaa0b8");
      const label = gdb > 0 ? `+${gdb}dB` : `${gdb}dB`;
      ctx.fillText(label, PAD_LEFT - 6, y + 3.5);
    }

    const coeffsList = bands.map((b) => computeBiquadCoeffs(b));
    const plotW = Math.max(1, Math.floor(width - PAD_LEFT - PAD_RIGHT));
    const zeroY = dbToY(0, height);

    // Selected Band Individual Response Curve
    if (bands[selectedBand] && bands[selectedBand].enabled) {
      const selCoeffs = coeffsList[selectedBand];
      ctx.strokeStyle = `${accentColor}77`;
      ctx.lineWidth = 1.4;
      ctx.setLineDash([4, 3]);
      ctx.beginPath();
      for (let px = 0; px <= plotW; px += 2) {
        const x = PAD_LEFT + px;
        const f = xToFreq(x, width);
        const db = evaluateBandMagnitudeDb(selCoeffs, f);
        const y = dbToY(db, height);
        if (px === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
      ctx.setLineDash([]);
    }

    // Composite 10-Band Response Curve: Total_dB(f) = sum_{k=0}^9 H_k(omega)
    const points: Array<[number, number]> = [];
    for (let px = 0; px <= plotW; px += 2) {
      const x = PAD_LEFT + px;
      const f = xToFreq(x, width);
      let totalDb = 0;
      for (let k = 0; k < coeffsList.length; k++) {
        totalDb += evaluateBandMagnitudeDb(coeffsList[k], f);
      }
      const y = dbToY(totalDb, height);
      points.push([x, y]);
    }

    if (points.length > 1) {
      const grad = ctx.createLinearGradient(0, PAD_TOP, 0, height - PAD_BOTTOM);
      if (isEqEnabled) {
        grad.addColorStop(0, `${accentColor}48`);
        grad.addColorStop(0.5, `${accentColor}0A`);
        grad.addColorStop(1, `${accentColor}48`);
      } else {
        grad.addColorStop(0, "rgba(255, 255, 255, 0.02)");
        grad.addColorStop(1, "rgba(255, 255, 255, 0.005)");
      }

      ctx.fillStyle = grad;
      ctx.beginPath();
      ctx.moveTo(points[0][0], zeroY);
      for (const [x, y] of points) {
        ctx.lineTo(x, y);
      }
      ctx.lineTo(points[points.length - 1][0], zeroY);
      ctx.closePath();
      ctx.fill();

      // Composite transfer spline stroke
      ctx.strokeStyle = isEqEnabled ? accentColor : "#5e5768";
      ctx.lineWidth = isEqEnabled ? 2.4 : 1.8;
      if (!isEqEnabled) ctx.setLineDash([6, 5]);
      ctx.beginPath();
      points.forEach(([x, y], idx) => {
        if (idx === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      });
      ctx.stroke();
      if (!isEqEnabled) ctx.setLineDash([]);
    }

    if (!isEqEnabled) {
      ctx.save();
      ctx.fillStyle = isLight ? "rgba(120, 105, 145, 0.75)" : "rgba(202, 189, 219, 0.55)";
      ctx.font = "600 11px 'Plus Jakarta Sans', sans-serif";
      ctx.textAlign = "center";
      ctx.fillText("BYPASS · 10-BAND EQ FILTERING INACTIVE", width / 2, height / 2 - 12);
      ctx.restore();
    }

    // Draw 10 Constellation Band Handles + Stems to 0 dB Unity Line
    bands.forEach((band, idx) => {
      const nx = freqToX(band.freq, width);
      const ny = dbToY(band.gainDb, height);
      const isSel = idx === selectedBand;
      const isHov = idx === hoveredBand;

      // Vertical stem from 0dB axis to node
      if (band.enabled && Math.abs(band.gainDb) > 0.15) {
        ctx.strokeStyle = isSel ? `${accentColor}AA` : `${accentColor}44`;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(nx, zeroY);
        ctx.lineTo(nx, ny);
        ctx.stroke();
      }

      // Outer constellation Q-ring & spectroscopic crosshair on selected or hovered band
      if (isSel || isHov) {
        ctx.strokeStyle = `${accentColor}30`;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(nx, PAD_TOP);
        ctx.lineTo(nx, height - PAD_BOTTOM);
        ctx.moveTo(PAD_LEFT, ny);
        ctx.lineTo(width - PAD_RIGHT, ny);
        ctx.stroke();

        const ringRadius = Math.max(12, Math.min(24, 20 / Math.sqrt(band.q)));
        ctx.beginPath();
        ctx.arc(nx, ny, ringRadius, 0, Math.PI * 2);
        ctx.strokeStyle = `${accentColor}66`;
        ctx.lineWidth = 1.3;
        ctx.stroke();
      }

      ctx.beginPath();
      ctx.arc(nx, ny, isSel ? 8.5 : 7, 0, Math.PI * 2);
      ctx.fillStyle = band.enabled
        ? (isSel ? accentColor : (isLight ? "#ffffff" : "#0d1322"))
        : (isLight ? "#e8e5ef" : "#090d16");
      ctx.fill();
      ctx.lineWidth = isSel ? 2.2 : 1.6;
      ctx.strokeStyle = band.enabled ? accentColor : (isLight ? "#94a3b8" : "#475569");
      ctx.stroke();

      ctx.fillStyle = isSel && band.enabled
        ? (isLight ? "#ffffff" : "#1c1c20")
        : (isLight ? "#241f32" : "#f1f5f9");
      ctx.font = "700 9px 'Plus Jakarta Sans', sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(`${idx + 1}`, nx, ny + 3);
    });

    ctx.restore();
  }

  function getPointerBandIndex(e: PointerEvent | MouseEvent): number | null {
    if (!canvasEl) return null;
    const rect = canvasEl.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    let closestIdx: number | null = null;
    let closestDist = 20;
    bands.forEach((b, idx) => {
      const bx = freqToX(b.freq, rect.width);
      const by = dbToY(b.gainDb, rect.height);
      const dist = Math.hypot(mx - bx, my - by);
      if (dist < closestDist) {
        closestDist = dist;
        closestIdx = idx;
      }
    });
    return closestIdx;
  }

  // While the EQ is bypassed the graph is read-only: no dragging, selecting, resetting or Q scroll.
  function onPointerDown(e: PointerEvent) {
    if (!isEqEnabled || simple) return;
    const hit = getPointerBandIndex(e);
    if (hit !== null) {
      selectedBand = hit;
      draggingBand = hit;
      canvasEl?.setPointerCapture(e.pointerId);
    }
  }

  function onPointerMove(e: PointerEvent) {
    if (!canvasEl) return;
    const rect = canvasEl.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    cursorHud = {
      freq: Math.max(20, Math.min(20000, xToFreq(mx, rect.width))),
      db: Math.max(-12, Math.min(12, yToDb(my, rect.height))),
    };

    if (draggingBand === null) {
      hoveredBand = isEqEnabled && !simple ? getPointerBandIndex(e) : null;
      return;
    }

    const current = bands[draggingBand];
    if (!current) return;

    const newFreq = Math.round(xToFreq(mx, rect.width) * 10) / 10;
    const newGain = Math.round(yToDb(my, rect.height) * 10) / 10;

    onUpdateBand(draggingBand, {
      ...current,
      enabled: true,
      freq: Math.max(20, Math.min(20000, newFreq)),
      gainDb: Math.max(-12, Math.min(12, newGain)),
    });
  }

  function onPointerUp(e: PointerEvent) {
    if (draggingBand !== null) {
      canvasEl?.releasePointerCapture(e.pointerId);
      draggingBand = null;
    }
  }

  function onPointerLeave() {
    hoveredBand = null;
    cursorHud = null;
  }

  function onDoubleClickCanvas(e: MouseEvent) {
    if (!isEqEnabled || simple) return;
    const hit = getPointerBandIndex(e);
    if (hit !== null && bands[hit]) {
      selectedBand = hit;
      onUpdateBand(hit, {
        ...bands[hit],
        gainDb: 0.0,
      });
    }
  }

  function onWheel(e: WheelEvent) {
    if (!isEqEnabled || simple) return; // let the page scroll
    e.preventDefault();
    const current = bands[selectedBand];
    if (!current) return;
    const step = e.shiftKey ? 0.05 : 0.15;
    const delta = e.deltaY < 0 ? step : -step;
    const nextQ = Math.round(Math.max(0.1, Math.min(10.0, current.q + delta)) * 100) / 100;
    onUpdateBand(selectedBand, {
      ...current,
      q: nextQ,
    });
  }

  $effect(() => {
    void bus;
    void bands;
    void selectedBand;
    void hoveredBand;
    void accentColor;
    drawEqGraph();
  });

  onMount(() => {
    drawEqGraph();
    const ro = new ResizeObserver(() => drawEqGraph());
    if (canvasEl) ro.observe(canvasEl);
    const mo = new MutationObserver(() => drawEqGraph());
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => {
      ro.disconnect();
      mo.disconnect();
    };
  });
</script>

<div class="eq-panel" style="--eq-accent: {accentColor}">
  <div class="eq-header">
    <div class="eq-title-group">
      <div class="eq-title-meta">
        <span
          class="bus-badge"
          style="background: {accentColor}1F; color: {accentColor}; border-color: {accentColor}66"
        >
          {bus} · 10-band EQ
        </span>
        <ModeSwitch />
        <span class="eq-status-indicator" class:active={isEqEnabled}>
          {isEqEnabled ? "Active" : "Bypassed"}
        </span>
      </div>
      <h2>Shape your sound</h2>
    </div>

    <div class="eq-actions">
      <!-- Global toggle switch for the EQ -->
      <div class="eq-master-toggle" data-tooltip={isEqEnabled ? `EQ is Active · Click to bypass ${bus} EQ` : `EQ is Bypassed · Click to enable ${bus} EQ`}>
        <span class="eq-toggle-label">{isEqEnabled ? "EQ" : "BYP"}</span>
        <button
          type="button"
          class="switch"
          class:on={isEqEnabled}
          aria-pressed={isEqEnabled}
          aria-label={`${bus} Equalizer master bypass toggle (${isEqEnabled ? "Enabled" : "Bypassed"})`}
          onclick={toggleGlobalEq}
        >
          <span></span>
        </button>
      </div>

      <div class="ab-toggle" role="group" aria-label="Compare two EQ settings">
        {#each [0, 1] as slot}
          <button type="button" aria-pressed={abActive === slot} disabled={!isEqEnabled}
            data-tooltip={abActive === slot ? `Setting ${slot ? "B" : "A"} is active` : `Switch to setting ${slot ? "B" : "A"} (your current EQ is kept in ${abActive ? "B" : "A"})`}
            onclick={() => switchAB(slot as 0 | 1)}>{slot ? "B" : "A"}</button>
        {/each}
      </div>

      {#if onApplyPreset}
        <StudioSelect
          class="eq-preset"
          disabled={!isEqEnabled}
          aria-label={`${bus} EQ preset`}
          value={presetValue}
          onchange={e => handlePresetChange(e.currentTarget.value)}
        >
          <option value="custom" disabled>Custom</option>
          <optgroup label="Built-in">
            {#each availablePresets as p}
              <option value={p.id}>{p.name}</option>
            {/each}
          </optgroup>
          {#if userPresets.length}
            <optgroup label="My presets">
              {#each userPresets as p (p.id)}
                <option value={`user:${p.id}`}>{p.name}</option>
              {/each}
            </optgroup>
          {/if}
        </StudioSelect>
      {/if}

      {#if activeUserPreset}
        <button type="button" class="icon-button" disabled={!isEqEnabled} onclick={deleteEqPreset}
          aria-label={`Delete preset ${activeUserPreset.name}`} data-tooltip={`Delete "${activeUserPreset.name}"`}>
          <StudioIcon name="close" size={14} />
        </button>
      {:else}
        <button type="button" class="btn-flat" disabled={!isEqEnabled} onclick={() => { namingPreset = !namingPreset; presetNotice = ""; }}
          aria-expanded={namingPreset} data-tooltip="Save the current EQ as your own preset">Save as…</button>
      {/if}

      <!-- Reset to defaults button icon -->
      <button
        type="button"
        class="icon-button eq-reset-btn"
        onclick={handleResetDefaults}
        disabled={!isEqEnabled}
        aria-label={`Reset ${bus} EQ to default profile`}
        data-tooltip={`Reset ${bus} EQ to defaults`}
      >
        <StudioIcon name="reset" size={15} />
      </button>

      <button
        type="button"
        class="btn-flat"
        onclick={onResetFlat}
        disabled={!isEqEnabled}
        data-tooltip="Reset all 10 bands to 0.0 dB flat response"
      >
        Flatten 0 dB
      </button>
    </div>
  </div>

  {#if namingPreset}
    <form class="save-row" onsubmit={(e) => { e.preventDefault(); void saveEqPreset(); }}>
      <input type="text" bind:value={presetName} maxlength="32" placeholder="Preset name" aria-label="Preset name" />
      <button type="submit" class="btn-flat">Save</button>
      <button type="button" class="btn-flat" onclick={() => { namingPreset = false; presetNotice = ""; }}>Cancel</button>
    </form>
  {/if}
  {#if presetNotice}<p class="preset-notice" role="status">{presetNotice}</p>{/if}

  <div class="eq-subbar">
    {#if !isEqEnabled}
      <span class="hud-bypass-warning">
        Equalizer is bypassed · Filter processing paused
      </span>
    {:else if bands[selectedBand] && !simple}
      {@const sel = bands[selectedBand]}
      <span class="hud-active-band" style="color: {accentColor}">
        Band B{selectedBand + 1} · {formatFreq(sel.freq)} · {sel.gainDb >= 0 ? `+${sel.gainDb.toFixed(1)}` : sel.gainDb.toFixed(1)} dB · Q {sel.q.toFixed(2)} ({sel.type})
      </span>
    {/if}
    {#if cursorHud}
      <span class="hud-cursor">
        Cursor: {formatFreq(cursorHud.freq)} ({cursorHud.db >= 0 ? `+${cursorHud.db.toFixed(1)}` : cursorHud.db.toFixed(1)} dB)
      </span>
    {:else}
      <span class="hud-cursor">
        {!isEqEnabled || simple ? "" : "Drag node to sculpt · Double-click node for 0dB · Scroll wheel for Q"}
      </span>
    {/if}
  </div>

  <div class="canvas-wrap">
    <canvas
      bind:this={canvasEl}
      class:locked={!isEqEnabled || simple}
      aria-readonly={!isEqEnabled || simple}
      aria-label="{bus} Bus 10-Band Parametric Equalizer Graph"
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointerleave={onPointerLeave}
      ondblclick={onDoubleClickCanvas}
      onwheel={onWheel}
    ></canvas>
  </div>

  {#if simple}
    <div class="tone-panel" class:locked={!isEqEnabled} inert={!isEqEnabled}>
      {#each TONE_GROUPS as group (group.id)}
        <label class="tone">
          <span class="tone-head"><strong>{group.label}</strong><em>{fmtTone(groupGain(group.bands))}</em></span>
          <input class="horizontal-range" type="range" min={-TONE_RANGE_DB} max={TONE_RANGE_DB} step="0.5" value={groupGain(group.bands)}
            aria-label={bus + " " + group.label} aria-valuetext={fmtTone(groupGain(group.bands))}
            ondblclick={() => setToneGroup(group.bands, 0)}
            oninput={(e) => setToneGroup(group.bands, parseFloat(e.currentTarget.value))} />
          <small>{group.hint}</small>
        </label>
      {/each}
    </div>
  {:else if bands[selectedBand]}
    {@const active = bands[selectedBand]}
    <div class="band-inspector">
      <div class="band-tabs" role="tablist" aria-label="Select Equalizer Band">
        {#each bands as b, idx}
          <button
            type="button"
            role="tab" data-tooltip={`Band ${idx+1}: ${formatFreq(b.freq)}, ${b.gainDb.toFixed(2)} dB, Q ${b.q.toFixed(2)}. Select to edit this band.`}
            aria-selected={idx === selectedBand}
            class="band-tab {idx === selectedBand ? 'active' : ''} {!b.enabled ? 'muted' : ''}"
            style={idx === selectedBand ? `border-color: ${accentColor}; color: ${accentColor}` : ""}
            onclick={() => (selectedBand = idx)}
          >
            <span>B{idx + 1}</span>
            <small>{b.gainDb >= 0 ? `+${b.gainDb.toFixed(1)}` : b.gainDb.toFixed(1)}</small>
          </button>
        {/each}
      </div>

      <div class="band-controls" class:locked={!isEqEnabled} inert={!isEqEnabled} aria-disabled={!isEqEnabled}>
        <label class="ctrl-item toggle-col">
          <span>Band {selectedBand + 1}</span>
          <div class="band-enable-row">
            <input
              type="checkbox"
              checked={active.enabled}
              onchange={(e) =>
                onUpdateBand(selectedBand, { ...active, enabled: e.currentTarget.checked })}
            />
            <strong>{active.enabled ? "ON" : "BYP"}</strong>
          </div>
        </label>

        <label class="ctrl-item">
          <span>Filter Topology</span>
          <StudioSelect
            value={active.type}
            onchange={(e) =>
              onUpdateBand(selectedBand, {
                ...active,
                type: e.currentTarget.value as FilterType,
              })}
          >
            {#each FILTER_TYPES as ft}
              <option value={ft}>{ft}</option>
            {/each}
          </StudioSelect>
        </label>

        <label class="ctrl-item">
          <div class="ctrl-label-row">
            <span>Frequency</span>
            <strong><EditableValue value={active.freq} min={20} max={20000} step={1} unit="Hz" label={bus+' EQ frequency'} oncommit={v=>onUpdateBand(selectedBand,{...active,freq:v})}/></strong>
          </div>
          <input
            class="horizontal-range"
            type="range"
            aria-label={`${bus} EQ frequency`} min="20"
            max="20000"
            step="1"
            value={active.freq}
            oninput={(e) =>
              onUpdateBand(selectedBand, {
                ...active,
                freq: parseFloat(e.currentTarget.value),
              })}
          />
        </label>

        <label class="ctrl-item" title="Double-click to reset band gain to 0.0 dB">
          <div class="ctrl-label-row">
            <span>Gain</span>
            <strong><EditableValue value={active.gainDb} min={-12} max={12} step={.1} unit="dB" label={bus+' EQ gain'} oncommit={v=>onUpdateBand(selectedBand,{...active,gainDb:v})}/></strong>
          </div>
          <input
            class="horizontal-range"
            type="range"
            aria-label={`${bus} EQ gain`} min="-12"
            max="12"
            step="0.1"
            value={active.gainDb}
            ondblclick={() =>
              onUpdateBand(selectedBand, {
                ...active,
                gainDb: 0.0,
              })}
            oninput={(e) =>
              onUpdateBand(selectedBand, {
                ...active,
                gainDb: parseFloat(e.currentTarget.value),
              })}
          />
        </label>

        <label class="ctrl-item" title="Double-click to reset Q factor to 1.41">
          <div class="ctrl-label-row">
            <span>Q Bandwidth</span>
            <strong><EditableValue value={active.q} min={.1} max={10} step={.001} label={bus+' EQ Q'} oncommit={v=>onUpdateBand(selectedBand,{...active,q:v})}/></strong>
          </div>
          <input
            class="horizontal-range"
            type="range"
            aria-label={`${bus} EQ Q`} min="0.1"
            max="10.0"
            step="0.05"
            value={active.q}
            ondblclick={() =>
              onUpdateBand(selectedBand, {
                ...active,
                q: selectedBand === 0 || selectedBand === 9 ? 0.707 : 1.414,
              })}
            oninput={(e) =>
              onUpdateBand(selectedBand, {
                ...active,
                q: parseFloat(e.currentTarget.value),
              })}
          />
        </label>
      </div>
    </div>
  {/if}
</div>

<style>
 .eq-panel{background:var(--bg-surface);border-radius:16px;padding:22px 23px;display:flex;flex-direction:column;gap:18px;min-width:0;height:100%}.eq-header{display:flex;align-items:center;justify-content:space-between;gap:15px;flex-wrap:wrap}.eq-title-group{display:flex;flex-direction:column-reverse;gap:9px}.eq-title-group h2{font-size:15px;font-weight:600;letter-spacing:-.025em}.eq-title-meta{display:flex;align-items:center;gap:8px}.bus-badge{font-size:10px;background:transparent!important;border:0!important;padding:0;color:var(--eq-accent)}.eq-status-indicator{font-size:9px;font-weight:600;text-transform:uppercase;letter-spacing:.05em;color:#857c92;padding:2px 6px;border-radius:4px;background:#ffffff06}.eq-status-indicator.active{color:var(--eq-accent);background:#b8a1fa12}.eq-actions{display:flex;align-items:center;gap:9px;flex-wrap:wrap}.eq-master-toggle{display:inline-flex;align-items:center;gap:8px;background:#25222b;padding:5px 9px 5px 11px;border-radius:7px;border:1px solid #ffffff0d}.eq-toggle-label{font-size:10px;font-weight:600;letter-spacing:.04em;color:#c0b3cf}.eq-master-toggle .switch{width:30px;height:17px;min-width:30px;border-radius:20px;background:#48414e;padding:3px;display:flex;align-items:center;cursor:pointer;border:0;transition:background 200ms ease,box-shadow 200ms ease}.eq-master-toggle .switch span{width:11px;height:11px;border-radius:50%;background:#c0b4ca;transition:transform 220ms var(--spring-bounce, cubic-bezier(.22,1,.36,1)),background-color 200ms ease}.eq-master-toggle .switch.on{background:var(--eq-accent)}.eq-master-toggle .switch.on span{transform:translateX(13px);background:#18141f}:global(.eq-preset),.btn-flat{background:#29262f;border:0;border-radius:7px;padding:9px 11px;font-size:10px;color:#cfc3df;max-width:100%}.btn-flat{cursor:pointer}.btn-flat:hover{background:#39323f}.eq-actions .eq-reset-btn{background:#29262f;border:0;border-radius:7px;width:32px;height:32px;display:grid;place-items:center;color:#cfc3df;cursor:pointer;transition:background 150ms ease,color 150ms ease,transform 150ms ease}.eq-actions .eq-reset-btn:hover{background:#39323f;color:#f4f2fa}.eq-actions .eq-reset-btn:active{transform:scale(0.94)}.hud-bypass-warning{color:#e89ab0;font-weight:500}.eq-subbar{font-size:10px;display:flex;justify-content:space-between;gap:12px;flex-wrap:wrap;color:#b5a4c6;font-variant-numeric:tabular-nums}.hud-cursor{color:#b5a4c6}.canvas-wrap{height:210px;min-width:0;width:100%;overflow:hidden;background:var(--bg-surface);border-radius:8px}.canvas-wrap canvas{width:100%;height:100%;display:block;touch-action:none;cursor:crosshair}.band-inspector{display:flex;flex-direction:column;gap:20px;min-width:0;margin-top:8px}.band-tabs{display:grid;grid-template-columns:repeat(10,minmax(0,1fr));gap:5px}.band-tab{padding:8px 1px;min-width:0;min-height:42px;background:#ffffff03;border:0;border-bottom:1px solid transparent!important;border-radius:5px;font-variant-numeric:tabular-nums;display:flex;flex-direction:column;align-items:center;gap:8px;color:#bcaece;font-size:10px;cursor:pointer}.band-tab.active{background:#b8a1fa12;color:var(--eq-accent)}.band-tab.muted{opacity:.45}.band-tab small{font-size:9px}.band-controls{display:grid;grid-template-columns:.7fr 1fr 1.2fr 1fr .8fr;gap:14px;min-width:0}.ctrl-item{display:flex;flex-direction:column;justify-content:center;gap:11px;min-width:0;font-size:10px;color:#baadca}.ctrl-label-row{display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap}.ctrl-label-row strong{font-size:10px;color:#e0d1ee;font-weight:500;font-variant-numeric:tabular-nums}.ctrl-item :global(.studio-select-trigger){width:100%;min-width:0;background:#2c2634;color:#e0d1ee;border:0;border-radius:6px;padding:8px;font-size:10px}.band-enable-row{display:flex;align-items:center;gap:8px}.band-enable-row strong{font-size:10px;font-weight:500}.band-enable-row input{accent-color:var(--eq-accent)}.ctrl-item input[type=range]{margin:8px 0}button:focus-visible,:global(.studio-select-trigger:focus-visible),input:focus-visible{outline:2px solid var(--focus-ring);outline-offset:3px}
 @media(max-width:1200px){.band-controls{grid-template-columns:1fr 1fr 1fr}.eq-subbar{font-size:9px}}
 @media(max-width:720px){.eq-panel{padding:19px 14px}.canvas-wrap{height:180px}.band-controls{grid-template-columns:1fr 1fr;gap:17px}.band-tab{font-size:8px}.band-tab small{font-size:8px}:global(.eq-preset),.btn-flat{font-size:9px;padding:8px}.eq-header{gap:15px}}
  .ab-toggle{display:inline-flex;padding:2px;border-radius:8px;background:color-mix(in srgb,var(--text,#fff) 6%,transparent);gap:2px}
  .ab-toggle button{min-width:28px;padding:6px 9px;border:0;border-radius:6px;background:transparent;color:inherit;font:inherit;font-size:11px;font-weight:600;cursor:pointer;opacity:.7}
  .ab-toggle button:hover:not(:disabled){opacity:1}
  .ab-toggle button[aria-pressed=true]{opacity:1;background:color-mix(in srgb,var(--eq-accent) 28%,transparent);color:var(--eq-accent)}
  .ab-toggle button:disabled{cursor:not-allowed;opacity:.35}
  .save-row{display:flex;gap:8px;align-items:center;margin:0}
  .save-row input{flex:1;max-width:260px;min-width:0;padding:8px 10px;border-radius:8px;border:1px solid var(--line,#ffffff1a);background:transparent;color:inherit;font:inherit;font-size:11px}
  .preset-notice{margin:0;font-size:10px;color:var(--eq-accent)}
  .tone-panel{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:22px}
  .tone-panel.locked{opacity:.45;filter:saturate(.6)}
  .tone{display:flex;flex-direction:column;gap:8px;min-width:0}
  .tone-head{display:flex;justify-content:space-between;align-items:baseline}
  .tone-head strong{font-size:12px}
  .tone-head em{font-style:normal;font-size:11px;color:var(--eq-accent);font-variant-numeric:tabular-nums}
  .tone small{font-size:10px;color:var(--text-muted);line-height:1.5}
  @media(max-width:720px){.tone-panel{grid-template-columns:1fr}}
  .band-controls.locked{opacity:.45;filter:saturate(.6)}
  .canvas-wrap:has(canvas.locked){cursor:not-allowed}
  canvas.locked{cursor:not-allowed}
  .btn-flat:disabled,.eq-reset-btn:disabled{opacity:.4;cursor:not-allowed}
  :global(:root[data-theme="light"]) .eq-panel {
    background: #ffffff;
    border: 1px solid rgba(35, 28, 56, 0.08);
    box-shadow: 0 4px 18px rgba(35, 28, 56, 0.05);
  }
  :global(:root[data-theme="light"]) .eq-title-group h2 {
    color: #1a1824;
  }
  :global(:root[data-theme="light"]) .eq-master-toggle {
    background: #f0edf6;
    border: 1px solid rgba(35, 28, 56, 0.12);
  }
  :global(:root[data-theme="light"]) .eq-toggle-label {
    color: #383446;
  }
  :global(:root[data-theme="light"]) .eq-master-toggle .switch {
    background: #d8d3e2;
    border: 1px solid rgba(35, 28, 56, 0.1);
  }
  :global(:root[data-theme="light"]) .eq-master-toggle .switch span {
    background: #ffffff;
    box-shadow: 0 1px 3px rgba(35, 28, 56, 0.2);
  }
  :global(:root[data-theme="light"]) .eq-master-toggle .switch.on {
    background: var(--eq-accent);
  }
  :global(:root[data-theme="light"]) .eq-master-toggle .switch.on span {
    background: #ffffff;
  }
  :global(:root[data-theme="light"]) .hud-bypass-warning {
    color: #c62828;
  }
  :global(:root[data-theme="light"]) .eq-status-indicator {
    background: rgba(35, 28, 56, 0.06);
    color: #6e6980;
  }
  :global(:root[data-theme="light"]) .eq-status-indicator.active {
    background: rgba(124, 77, 255, 0.12);
    color: var(--eq-accent);
  }
  :global(:root[data-theme="light"]) :global(.eq-preset) {
    background: #f0edf6;
    color: #1a1824;
    border: 1px solid rgba(35, 28, 56, 0.1);
  }
  :global(:root[data-theme="light"]) .btn-flat,
  :global(:root[data-theme="light"]) .eq-actions .eq-reset-btn {
    background: #f0edf6;
    color: #383446;
    border: 1px solid rgba(35, 28, 56, 0.08);
  }
  :global(:root[data-theme="light"]) .btn-flat:hover,
  :global(:root[data-theme="light"]) .eq-actions .eq-reset-btn:hover {
    background: #e4e0ee;
    color: #1a1824;
  }
  :global(:root[data-theme="light"]) .canvas-wrap {
    background: #fbfafc;
    border: 1px solid rgba(35, 28, 56, 0.08);
  }
  :global(:root[data-theme="light"]) .band-tab {
    background: #f2eef8;
    color: #5a5468;
    border: 1px solid rgba(35, 28, 56, 0.08);
  }
  :global(:root[data-theme="light"]) .band-tab:hover {
    background: #e8e3f2;
    color: #1a1824;
  }
  :global(:root[data-theme="light"]) .band-tab.active {
    background: rgba(124, 77, 255, 0.15);
    color: var(--eq-accent);
    border-color: rgba(124, 77, 255, 0.45);
    font-weight: 600;
  }
  :global(:root[data-theme="light"]) .eq-subbar,
  :global(:root[data-theme="light"]) .hud-cursor,
  :global(:root[data-theme="light"]) .ctrl-item {
    color: #5a5468;
  }
  :global(:root[data-theme="light"]) .ctrl-label-row strong {
    color: #1a1824;
  }
  :global(:root[data-theme="light"]) .ctrl-item :global(.studio-select-trigger) {
    background: #f0edf6;
    color: #1a1824;
    border: 1px solid rgba(35, 28, 56, 0.1);
  }
</style>



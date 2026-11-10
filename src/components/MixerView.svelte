<script lang="ts">
  import StudioSelect from './StudioSelect.svelte';
  import EditableValue from './EditableValue.svelte';
  import AppIcon from './AppIcon.svelte';
  import { onMount, untrack, type Snippet } from "svelte";
  import { masonry } from "../masonry";
  import StudioIcon from "./StudioIcon.svelte";
  import type {
    ActiveAudioProcess,
    AllBusMetersPayload,
    AudioEndpointInfo,
    BusId,
    CompressorConfig,
    DeEsserConfig,
    EqBandConfig,
    LimiterConfig,
    NoiseGateConfig,
    NoiseSuppressionConfig,
    PlaybackBusId,
    RouterConfig,
    StereoMeterSnapshot,
  } from "../types";
  import { DEFAULT_DE_ESSER, DEFAULT_LIMITER, DEFAULT_NOISE_SUPPRESSION } from "../types";
  import { computeChatMixGains, getEqPresetsForBus, linearToDbString, type EqPreset } from "../dspMath";
  import EqCanvas from "./EqCanvas.svelte";
  import ChannelChain, { type StageId } from "./ChannelChain.svelte";
  import ModeSwitch from "./ModeSwitch.svelte";
  import FinePresetBar from "./FinePresetBar.svelte";
  import QuickSetup from "./QuickSetup.svelte";
  import { appSettings } from "../appSettings.svelte";
  import { applyBundleToConfig, readAllSections, type SectionBundle, type SectionId } from "../finePresets";
  import CategoryAppsModal from "./CategoryAppsModal.svelte";

  interface Props {
    native?: boolean;
    streamOutput?: Snippet;
    config: RouterConfig;
    endpoints: AudioEndpointInfo[];
    processes: ActiveAudioProcess[];
    meters: AllBusMetersPayload;
    selectedBus: BusId;
    soloedBuses: Record<BusId, boolean>;
    streamMonitoring?: boolean;
    onSelectBus: (bus: BusId) => void;
    onToggleSolo: (bus: BusId) => void;
    onChangeBusVolumeMute: (bus: BusId, volume: number, muted: boolean, preGainDb: number) => void;
    onChangeStreamChannel: (bus: BusId, volume: number, muted: boolean) => void;
    streamSaving?: boolean;
    onChangeChatMix: (balance: number) => void;
    onChangeMasterVolume: (volume: number) => void;
    onChangeEqBand: (bus: BusId, bandIndex: number, band: EqBandConfig) => void;
    onResetEqFlat: (bus: BusId) => void;
    onResetEqDefaults?: (bus: BusId) => void;
    onToggleEqGlobal?: (bus: BusId, enabled: boolean) => void;
    onApplyEqPreset: (bus: BusId, preset: EqPreset) => void;
    onToggleMicToMonitor?: () => void;
    onChangeDynamics: (bus: BusId, gate: NoiseGateConfig, comp: CompressorConfig) => void;
    onRouteProcess: (pid: number, exeName: string, bus: PlaybackBusId) => void;
    onUnrouteProcess: (pid: number) => void;
    onRemoveRoutingRule: (exeName: string) => void;
    onRefreshProcesses: () => void;
    onNotify: (msg: string, action?: { label: string; run: () => void }) => void;
    onChangeExtras?: (bus: BusId, limiter: LimiterConfig, deEsser: DeEsserConfig, noise: NoiseSuppressionConfig) => void;
    onChangeProcessVolume?: (exeName: string, volume: number) => void;
  }

  let {
    native = false,
    streamOutput,
    config,
    endpoints,
    processes,
    meters,
    selectedBus,
    soloedBuses,
    streamMonitoring = false,
    onSelectBus,
    onToggleSolo,
    onChangeBusVolumeMute,
    onChangeStreamChannel,
    streamSaving = false,
    onChangeChatMix,
    onChangeMasterVolume,
    onChangeEqBand,
    onResetEqFlat,
    onResetEqDefaults,
    onToggleEqGlobal,
    onApplyEqPreset,
    onToggleMicToMonitor,
    onChangeDynamics,
    onRouteProcess,
    onUnrouteProcess,
    onRemoveRoutingRule,
    onRefreshProcesses,
    onNotify,
    onChangeExtras,
    onChangeProcessVolume,
  }: Props = $props();

  const BUS_LIST: BusId[] = ["Game", "Chat", "Media", "Aux", "Mic"];
  const PLAYBACK_BUSES: PlaybackBusId[] = ["Game", "Chat", "Media", "Aux"];

  const BUS_COLORS: Record<BusId, string> = {
    Game: "#b7a3ff",
    Chat: "#7be1d1",
    Media: "#a1baff",
    Aux: "#edc990",
    Mic: "#f0a7c6",
  };

  let draggedProc: ActiveAudioProcess | null = $state(null);
  let dragOverTarget: PlaybackBusId | "unroute" | "mic-locked" | null = $state(null);
  let dragCursorX: number = $state(0);
  let dragCursorY: number = $state(0);
  let dragOffsetX: number = $state(0);
  let dragOffsetY: number = $state(0);
  let dragTilt: number = $state(0);
  let dragSource: "pool" | "chip" = $state("pool");
  let recentlyRoutedPid: number | null = $state(null);
  let recentlyRoutedBus: PlaybackBusId | "unroute" | null = $state(null);
  let lastDragX: number = 0;
  let targetTilt: number = 0;
  let hasLeftRoutingPanel: boolean = false;
  let dragRafId: number | null = null;
  let dropFeedbackTimer: ReturnType<typeof setTimeout> | null = null;
  let pendingDrag: {
    proc: ActiveAudioProcess;
    source: "pool" | "chip";
    startX: number;
    startY: number;
    offsetX: number;
    offsetY: number;
  } | null = null;
  let processSearch: string = $state("");
  let poolTab: "unrouted" | "all" | "rules" = $state("all");
  let categoryModalBus: PlaybackBusId | null = $state(null);

  function triggerDropFeedback(pid: number, target: PlaybackBusId | "unroute") {
    recentlyRoutedPid = pid;
    recentlyRoutedBus = target;
    if (dropFeedbackTimer) clearTimeout(dropFeedbackTimer);
    dropFeedbackTimer = setTimeout(() => {
      recentlyRoutedPid = null;
      recentlyRoutedBus = null;
    }, 650);
  }

  function updateDropTargetAt(clientX: number, clientY: number) {
    if (!draggedProc) return;
    const els = document.elementsFromPoint(clientX, clientY);
    for (const el of els) {
      if (!(el instanceof HTMLElement)) continue;
      const channelEl = el.closest<HTMLElement>(".channel[data-bus]");
      if (channelEl) {
        hasLeftRoutingPanel = true;
        const bus = channelEl.dataset.bus as BusId | undefined;
        if (bus === "Mic") {
          dragOverTarget = "mic-locked";
        } else if (bus) {
          dragOverTarget = bus as PlaybackBusId;
        }
        return;
      }
      if (el.closest(".unroute-shelf")) {
        dragOverTarget = "unroute";
        return;
      }
      if (el.closest("#routing")) {
        if (dragSource === "chip" || (hasLeftRoutingPanel && !isAutoAuxProcess(draggedProc))) {
          dragOverTarget = "unroute";
        } else {
          dragOverTarget = null;
        }
        return;
      }
    }
    hasLeftRoutingPanel = true;
    dragOverTarget = null;
  }

  function stepDragPhysics() {
    if (!draggedProc) {
      dragRafId = null;
      return;
    }
    dragOffsetX *= 0.76;
    dragOffsetY *= 0.76;
    if (Math.abs(dragOffsetX) < 0.15) dragOffsetX = 0;
    if (Math.abs(dragOffsetY) < 0.15) dragOffsetY = 0;

    dragTilt += (targetTilt - dragTilt) * 0.3;
    targetTilt *= 0.78;
    if (Math.abs(dragTilt) < 0.08 && Math.abs(targetTilt) < 0.08) {
      dragTilt = 0;
      targetTilt = 0;
    }
    dragRafId = requestAnimationFrame(stepDragPhysics);
  }

  function startDragPhysicsLoop() {
    if (dragRafId !== null) cancelAnimationFrame(dragRafId);
    dragRafId = requestAnimationFrame(stepDragPhysics);
  }

  function clearDragState() {
    pendingDrag = null;
    draggedProc = null;
    dragOverTarget = null;
    dragTilt = 0;
    targetTilt = 0;
    dragOffsetX = 0;
    dragOffsetY = 0;
    if (dragRafId !== null) {
      cancelAnimationFrame(dragRafId);
      dragRafId = null;
    }
    document.body.classList.remove("is-dragging-app");
  }

  onMount(() => {
    const onWindowPointerMove = (e: PointerEvent) => {
      if (!draggedProc && pendingDrag) {
        const dist = Math.hypot(e.clientX - pendingDrag.startX, e.clientY - pendingDrag.startY);
        if (dist < 4) return;
        draggedProc = pendingDrag.proc;
        dragSource = pendingDrag.source;
        hasLeftRoutingPanel = pendingDrag.source === "chip";
        dragCursorX = e.clientX;
        dragCursorY = e.clientY;
        dragOffsetX = pendingDrag.offsetX;
        dragOffsetY = pendingDrag.offsetY;
        lastDragX = e.clientX;
        dragTilt = 0;
        targetTilt = 0;
        document.body.classList.add("is-dragging-app");
        startDragPhysicsLoop();
      }
      if (!draggedProc) return;
      const dx = e.clientX - lastDragX;
      lastDragX = e.clientX;
      dragCursorX = e.clientX;
      dragCursorY = e.clientY;
      targetTilt = Math.max(-14, Math.min(14, dx * 0.95));
      updateDropTargetAt(e.clientX, e.clientY);
    };

    const onWindowPointerUp = (e: PointerEvent) => {
      if (draggedProc) {
        if (e.clientX !== 0 || e.clientY !== 0) {
          updateDropTargetAt(e.clientX, e.clientY);
        }
        if (dragOverTarget === "unroute") {
          handleDropToUnroute();
        } else if (dragOverTarget && dragOverTarget !== "mic-locked") {
          handleDropOnBus(dragOverTarget);
        }
      }
      clearDragState();
    };

    const onWindowKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && (draggedProc || pendingDrag)) {
        clearDragState();
      }
    };

    const onWindowDragOver = (e: DragEvent) => {
      if (!draggedProc) return;
      e.preventDefault();
      if (e.clientX !== 0 || e.clientY !== 0) {
        const dx = e.clientX - lastDragX;
        lastDragX = e.clientX;
        dragCursorX = e.clientX;
        dragCursorY = e.clientY;
        targetTilt = Math.max(-14, Math.min(14, dx * 0.95));
        updateDropTargetAt(e.clientX, e.clientY);
      }
    };

    window.addEventListener("pointermove", onWindowPointerMove, { passive: true });
    window.addEventListener("pointerup", onWindowPointerUp);
    window.addEventListener("pointercancel", clearDragState);
    window.addEventListener("blur", clearDragState);
    window.addEventListener("keydown", onWindowKeyDown);
    window.addEventListener("dragover", onWindowDragOver);
    window.addEventListener("drop", clearDragState);
    window.addEventListener("dragend", clearDragState);

    return () => {
      clearDragState();
      if (dropFeedbackTimer) clearTimeout(dropFeedbackTimer);
      window.removeEventListener("pointermove", onWindowPointerMove);
      window.removeEventListener("pointerup", onWindowPointerUp);
      window.removeEventListener("pointercancel", clearDragState);
      window.removeEventListener("blur", clearDragState);
      window.removeEventListener("keydown", onWindowKeyDown);
      window.removeEventListener("dragover", onWindowDragOver);
      window.removeEventListener("drop", clearDragState);
      window.removeEventListener("dragend", clearDragState);
    };
  });

  // Peak hold decay tracking per bus
  let peakHoldLeft: Record<BusId, number> = $state({
    Game: 0,
    Chat: 0,
    Media: 0,
    Aux: 0,
    Mic: 0,
  });
  let peakHoldRight: Record<BusId, number> = $state({
    Game: 0,
    Chat: 0,
    Media: 0,
    Aux: 0,
    Mic: 0,
  });

  $effect(() => {
    for (const bus of BUS_LIST) {
      const m = meterForBus(bus);
      untrack(() => {
        if (m.peakLeft >= peakHoldLeft[bus]) {
          peakHoldLeft[bus] = m.peakLeft;
        } else {
          peakHoldLeft[bus] = Math.max(0, peakHoldLeft[bus] - 0.012);
        }
        if (m.peakRight >= peakHoldRight[bus]) {
          peakHoldRight[bus] = m.peakRight;
        } else {
          peakHoldRight[bus] = Math.max(0, peakHoldRight[bus] - 0.012);
        }
      });
    }
  });

  // Clip indicator: latches when a channel reaches full scale, until clicked.
  const CLIP_PEAK = 0.995;
  let clipLatched: Record<BusId, boolean> = $state({ Game: false, Chat: false, Media: false, Aux: false, Mic: false });

  $effect(() => {
    for (const bus of BUS_LIST) {
      const m = meterForBus(bus);
      if (Math.max(m.peakLeft, m.peakRight) >= CLIP_PEAK) {
        untrack(() => {
          if (!clipLatched[bus]) clipLatched[bus] = true;
        });
      }
    }
  });

  function formatLufs(value: number | undefined): string {
    if (value === undefined || value <= -99) return "−∞";
    return value.toFixed(1);
  }

  /** Channels currently lowered by auto-ducking. */
  const duckedBuses = $derived.by(() => {
    const d = config.ducking;
    const gain = meters.duckGain ?? 1;
    return d?.enabled && gain < 0.97 ? new Set<BusId>(d.targets) : new Set<BusId>();
  });

  const selLimiter = $derived<LimiterConfig>(selectedConfig.limiter ?? DEFAULT_LIMITER);
  const selDeEsser = $derived<DeEsserConfig>(selectedConfig.deEsser ?? DEFAULT_DE_ESSER);
  const selNoise = $derived<NoiseSuppressionConfig>(selectedConfig.noiseSuppression ?? DEFAULT_NOISE_SUPPRESSION);

  function changeExtras(patch: { limiter?: Partial<LimiterConfig>; deEsser?: Partial<DeEsserConfig>; noise?: Partial<NoiseSuppressionConfig> }) {
    onChangeExtras?.(
      selectedBus,
      { ...selLimiter, ...patch.limiter },
      { ...selDeEsser, ...patch.deEsser },
      { ...selNoise, ...patch.noise }
    );
  }

  const simple = $derived(appSettings.interfaceMode === "simple");
  let openSections = $state<Record<string, boolean>>({});
  const showParams = (id: SectionId) => !simple || !!openSections[id];
  const allSections = $derived(readAllSections(selectedConfig, selectedBus));

  /** Sends a bundle of section values to the engine with the existing per-section commands. */
  function commitBundle(bus: BusId, bundle: SectionBundle) {
    const next = applyBundleToConfig(config.buses[bus], bundle);
    if (bundle.gate || bundle.comp) onChangeDynamics(bus, next.gate, next.comp);
    if (bundle.limiter || bundle.deEsser || bundle.noise) onChangeExtras?.(bus, next.limiter, next.deEsser, next.noise);
  }

  /** Applies presets or a whole recipe, with a one-click Undo in the toast. */
  function applyBundle(bundle: SectionBundle, label: string, eqPresetId?: string, eqOff?: boolean) {
    const bus = selectedBus;
    const before = readAllSections(config.buses[bus], bus);
    const eqBefore = eqPresetId || eqOff ? config.buses[bus].eqBands.map((b) => ({ ...b })) : null;
    commitBundle(bus, bundle);
    if (eqOff) onToggleEqGlobal?.(bus, false);
    if (eqPresetId) {
      const preset = getEqPresetsForBus(bus).find((p) => p.id === eqPresetId);
      if (preset) onApplyEqPreset(bus, preset);
    }
    onNotify?.(label, {
      label: "Undo",
      run: () => {
        commitBundle(bus, before);
        eqBefore?.forEach((band, idx) => onChangeEqBand(bus, idx, band));
      },
    });
  }

  function toggleStage(stage: StageId) {
    switch (stage) {
      case "noise": return changeExtras({ noise: { enabled: !selNoise.enabled } });
      case "eq": return onToggleEqGlobal?.(selectedBus, !selectedConfig.eqBands.some((b) => b.enabled));
      case "deEsser": return changeExtras({ deEsser: { enabled: !selDeEsser.enabled } });
      case "gate": return onChangeDynamics(selectedBus, { ...selectedConfig.noiseGate, enabled: !selectedConfig.noiseGate.enabled }, selectedConfig.compressor);
      case "comp": return onChangeDynamics(selectedBus, selectedConfig.noiseGate, { ...selectedConfig.compressor, enabled: !selectedConfig.compressor.enabled });
      case "limiter": return changeExtras({ limiter: { enabled: !selLimiter.enabled } });
    }
  }

  const limiterParams = [
    { key: "ceilingDb", label: "Ceiling", min: -12, max: 0, step: 0.5, unit: "dB" },
    { key: "releaseMs", label: "Release", min: 10, max: 500, step: 10, unit: "ms" },
  ] as const;
  const deEsserParams = [
    { key: "frequency", label: "Frequency", min: 3000, max: 10000, step: 100, unit: "Hz" },
    { key: "thresholdDb", label: "Threshold", min: -60, max: 0, step: 1, unit: "dB" },
    { key: "maxReductionDb", label: "Max reduction", min: 0, max: 18, step: 0.5, unit: "dB" },
  ] as const;

  function appVolume(exeName: string): number {
    return config.processVolumes?.[exeName.toLowerCase()] ?? 1;
  }

  function volumeLabel(volume: number): string {
    return volume <= 0.001 ? "Muted" : `${Math.round(volume * 100)}%`;
  }

  function meterForBus(bus: BusId): StereoMeterSnapshot {
    switch (bus) {
      case "Game":
        return meters.game;
      case "Chat":
        return meters.chat;
      case "Media":
        return meters.media;
      case "Aux":
        return meters.aux;
      case "Mic":
        return meters.mic;
    }
  }

  function meterPercent(val: number): number {
    if (val <= 0.0001) return 0;
    const db = 20 * Math.log10(Math.min(1.5, val));
    return Math.max(0, Math.min(100, ((db + 60) / 60) * 100));
  }

  function meterPeakDbString(l: number, r: number): string {
    const max = Math.max(l, r);
    if (max <= 0.0005) return "-∞";
    const db = 20 * Math.log10(max);
    return `${db >= 0 ? "+" : ""}${db.toFixed(1)}`;
  }

  function endpointLabel(id: string | null, fallback: string): string {
    if (!id) return fallback;
    const found = endpoints.find((e) => e.id === id);
    return found ? found.name : fallback;
  }

  function processesForBus(bus: PlaybackBusId): ActiveAudioProcess[] {
    return processes.filter((p) => (p.assignedBus ?? "Aux") === bus);
  }

  function isAutoAuxProcess(proc: ActiveAudioProcess): boolean {
    return (proc.assignedBus ?? "Aux") === "Aux" && !config.processRoutingRules[proc.exeName.toLowerCase()];
  }

  function filteredPoolProcesses(): ActiveAudioProcess[] {
    const q = processSearch.trim().toLowerCase();
    return processes.filter((p) => {
      if (poolTab === "unrouted" && !isAutoAuxProcess(p)) return false;
      if (!q) return true;
      return (
        p.exeName.toLowerCase().includes(q) ||
        p.displayName.toLowerCase().includes(q) ||
        (p.pid === 0 ? ("system".includes(q) || "sounds".includes(q)) : String(p.pid).includes(q))
      );
    });
  }

  function onPointerDownProcess(
    e: PointerEvent,
    proc: ActiveAudioProcess,
    source: "pool" | "chip"
  ) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement | null;
    if (
      target?.closest(
        "button, select, input, a, .route-select, .studio-select, .chip-remove"
      )
    ) {
      return;
    }
    const currentEl = e.currentTarget as HTMLElement | null;
    let offsetX = 0;
    let offsetY = 0;
    if (currentEl) {
      const rect = currentEl.getBoundingClientRect();
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      offsetX = Math.max(-56, Math.min(56, (centerX - e.clientX) * 0.4));
      offsetY = Math.max(-22, Math.min(22, (centerY - e.clientY) * 0.4));
    }
    pendingDrag = {
      proc,
      source,
      startX: e.clientX,
      startY: e.clientY,
      offsetX,
      offsetY,
    };
  }

  function onDragStartProcess(
    e: DragEvent,
    proc: ActiveAudioProcess,
    source: "pool" | "chip" = "pool"
  ) {
    // Prevent native OS OLE drag loop when pointer drag is handling interaction
    if (pendingDrag || draggedProc) {
      e.preventDefault();
      return;
    }
    draggedProc = proc;
    dragSource = source;
    hasLeftRoutingPanel = source === "chip";
    dragOverTarget = null;
    dragCursorX = e.clientX || 0;
    dragCursorY = e.clientY || 0;
    dragOffsetX = 0;
    dragOffsetY = 0;
    lastDragX = e.clientX || 0;
    dragTilt = 0;
    targetTilt = 0;
    document.body.classList.add("is-dragging-app");
    startDragPhysicsLoop();
  }

  function onDragEndProcess() {
    clearDragState();
  }

  function handleDropOnBus(bus: PlaybackBusId) {
    if (draggedProc) {
      const proc = draggedProc;
      triggerDropFeedback(proc.pid, bus);
      if (proc.assignedBus !== bus || isAutoAuxProcess(proc)) {
        onRouteProcess(proc.pid, proc.exeName, bus);
      }
      clearDragState();
    }
  }

  function handleDropToUnroute() {
    if (draggedProc) {
      const proc = draggedProc;
      triggerDropFeedback(proc.pid, "unroute");
      onUnrouteProcess(proc.pid);
    }
    clearDragState();
  }

  const LINKED_FADERS_KEY = "stellars_linked_faders";
  function loadLinkedFaders(): Record<BusId, boolean> {
    try {
      const raw = localStorage.getItem(LINKED_FADERS_KEY);
      if (raw) return JSON.parse(raw);
    } catch {}
    return { Game: false, Chat: false, Media: false, Aux: false, Mic: false };
  }

  let linkedFaders = $state<Record<BusId, boolean>>(loadLinkedFaders());

  function toggleLinkFaders(bus: BusId) {
    const next = !linkedFaders[bus];
    linkedFaders[bus] = next;
    try {
      localStorage.setItem(LINKED_FADERS_KEY, JSON.stringify(linkedFaders));
    } catch {}
    if (next) {
      const currentVol = config.buses[bus]?.volume ?? 1.0;
      const send = config.stream.channels[bus];
      if (send && !streamSaving) {
        onChangeStreamChannel(bus, currentVol, send.muted);
      }
      onNotify?.(`${bus} faders linked (${linearToDbString(currentVol)})`);
    } else {
      onNotify?.(`${bus} faders unlinked`);
    }
  }

  function handleMonitorFaderInput(bus: BusId, volume: number) {
    const c = config.buses[bus];
    if (!c) return;
    onChangeBusVolumeMute(bus, volume, c.muted, c.preGainDb);
    if (config.stream.enabled && linkedFaders[bus]) {
      const send = config.stream.channels[bus];
      if (send && !streamSaving) {
        onChangeStreamChannel(bus, volume, send.muted);
      }
    }
  }

  function handleStreamFaderInput(bus: BusId, volume: number) {
    const send = config.stream.channels[bus];
    if (!send || streamSaving) return;
    onChangeStreamChannel(bus, volume, send.muted);
    if (config.stream.enabled && linkedFaders[bus]) {
      const c = config.buses[bus];
      if (c) {
        onChangeBusVolumeMute(bus, volume, c.muted, c.preGainDb);
      }
    }
  }

  function handleResetMonitorFader(bus: BusId) {
    handleMonitorFaderInput(bus, 1.0);
    onNotify?.(`${bus} reset to 0 dB`);
  }

  function handleResetStreamFader(bus: BusId) {
    handleStreamFaderInput(bus, 1.0);
    onNotify?.(`${bus} Stream reset to 0 dB`);
  }

  /** Selects the channel and brings its Fine-tune section into view. */
  function editChannel(bus: BusId) {
    onSelectBus(bus);
    const reduced = document.documentElement.dataset.motion === "reduced";
    document.getElementById("finetune")?.scrollIntoView({ behavior: reduced ? "auto" : "smooth", block: "start" });
  }

  function handleFaderWheel(e: WheelEvent, bus: BusId) {
    e.preventDefault();
    e.stopPropagation();
    const busCfg = config.buses[bus];
    if (!busCfg) return;
    const step = e.shiftKey ? 0.01 : 0.03;
    const delta = e.deltaY < 0 ? step : -step;
    const newVol = Math.max(0, Math.min(1.5, busCfg.volume + delta));
    handleMonitorFaderInput(bus, newVol);
  }

  function handleStreamFaderWheel(e: WheelEvent, bus: BusId) {
    e.preventDefault();
    e.stopPropagation();
    if (streamSaving) return;
    const send = config.stream.channels[bus];
    if (!send) return;
    const step = e.shiftKey ? 0.01 : 0.03;
    const delta = e.deltaY < 0 ? step : -step;
    const newVol = Math.max(0, Math.min(1.5, send.volume + delta));
    handleStreamFaderInput(bus, newVol);
  }

  function handlePreGainWheel(e: WheelEvent, bus: BusId) {
    e.preventDefault();
    e.stopPropagation();
    const busCfg = config.buses[bus];
    if (!busCfg) return;
    const step = e.shiftKey ? 0.2 : 0.5;
    const delta = e.deltaY < 0 ? step : -step;
    onChangeBusVolumeMute(bus, busCfg.volume, busCfg.muted, busCfg.preGainDb + delta);
  }

  function handleChatMixWheel(e: WheelEvent) {
    e.preventDefault();
    const step = e.shiftKey ? 0.02 : 0.05;
    const delta = e.deltaY < 0 ? step : -step;
    onChangeChatMix(config.chatMixBalance + delta);
  }

  function handleMasterWheel(e: WheelEvent) {
    e.preventDefault();
    const step = e.shiftKey ? 0.01 : 0.03;
    const delta = e.deltaY < 0 ? step : -step;
    onChangeMasterVolume(config.masterVolume + delta);
  }

  const anySoloActive = $derived(Object.values(soloedBuses).some(Boolean));

  const icons: Record<BusId, string> = { Game: "game", Chat: "chat", Media: "music", Aux: "aux", Mic: "mic" };
  const gateParams = [
    { key: "thresholdDb", label: "Threshold", min: -80, max: 0, step: 1, unit: "dB" },
    { key: "ratio", label: "Ratio", min: 1, max: 20, step: 0.5, unit: ":1" },
    { key: "attackMs", label: "Attack", min: 0.5, max: 50, step: 0.5, unit: "ms" },
    { key: "releaseMs", label: "Release", min: 10, max: 500, step: 5, unit: "ms" },
  ] as const;
  const compParams = [
    { key: "thresholdDb", label: "Threshold", min: -60, max: 0, step: 1, unit: "dB" },
    { key: "ratio", label: "Ratio", min: 1, max: 20, step: 0.5, unit: ":1" },
    { key: "attackMs", label: "Attack", min: 0.5, max: 100, step: 0.5, unit: "ms" },
    { key: "releaseMs", label: "Release", min: 10, max: 1000, step: 5, unit: "ms" },
    { key: "makeupGainDb", label: "Makeup gain", min: 0, max: 18, step: 0.5, unit: "dB" },
    { key: "kneeDb", label: "Soft knee", min: 0, max: 12, step: 0.5, unit: "dB" },
  ] as const;

  function routeSelection(proc: ActiveAudioProcess, value: string) {
    if (value === "Unrouted") onUnrouteProcess(proc.pid);
    else onRouteProcess(proc.pid, proc.exeName, value as PlaybackBusId);
  }

  /** Left half of the columns shows the left channel, right half the right; the last column of each half is the peak. */
  function dotValue(m: StereoMeterSnapshot, col: number, cols = 12) {
    const half = cols / 2;
    const side = col < half ? 0 : 1;
    const index = col % half;
    const rmsColumns = Math.ceil((half * 2) / 3);
    const value = index < rmsColumns ? (side ? m.rmsRight : m.rmsLeft) : side ? m.peakRight : m.peakLeft;
    return meterPercent(value);
  }

  const EMPTY_METER: StereoMeterSnapshot = { peakLeft: 0, peakRight: 0, rmsLeft: 0, rmsRight: 0 };
  /** What this channel currently adds to the stream mix. */
  function streamMeterForBus(bus: BusId): StereoMeterSnapshot {
    return meters.streamSends?.[bus.toLowerCase() as "game" | "chat" | "media" | "aux" | "mic"] ?? EMPTY_METER;
  }
  /** Two matrices (monitor + stream) share the card, so each gets fewer columns. */
  const STREAM_COLS = 8;
  const meterCols = $derived(config.stream.enabled ? STREAM_COLS : 12);

  const selectedConfig = $derived(config.buses[selectedBus]);
</script>

<div class="console-grid production-grid">
  <section id="mixer" class="mixer-area" aria-label="Five-bus mixer">
    <div class="section-heading"><h2>Mixer <span class="small-count">5 channels</span></h2></div>
    <div class="channel-grid">
      {#each BUS_LIST as bus,i}
        {@const c=config.buses[bus]}{@const m=meterForBus(bus)}{@const muted=c.muted||(anySoloActive&&!soloedBuses[bus])}
        <section
          class="channel"
          data-bus={bus}
          class:active-channel={selectedBus===bus}
          class:muted-channel={muted}
          class:drop-active={dragOverTarget===bus}
          class:drop-locked={bus==='Mic'&&dragOverTarget==='mic-locked'}
          class:drop-just-landed={recentlyRoutedBus===bus}
          style={`--channel-color:${BUS_COLORS[bus]};--channel-index:${i}`}
          aria-label={`${bus} channel`}
          ondragover={e=>{if(draggedProc){e.preventDefault();dragOverTarget=bus==='Mic'?'mic-locked':(bus as PlaybackBusId);}}}
          ondrop={e=>{e.preventDefault();if(bus!=='Mic')handleDropOnBus(bus as PlaybackBusId);}}
        >
          <button class="channel-title" data-tooltip={`Select ${bus} to edit EQ and dynamics. Keyboard shortcut: ${i+1}.`} aria-pressed={selectedBus===bus} onclick={()=>onSelectBus(bus)}><span class="channel-symbol"><StudioIcon name={icons[bus]}/></span><span>{bus}</span><span class="selected-dot"></span></button>
          
          <!-- Expandable Channel App Shelf with Physics -->
          <div class="channel-app-shelf" class:shelf-expanded={draggedProc !== null} class:shelf-drop-active={dragOverTarget === bus}>
            {#if bus === 'Mic'}
              <div class="channel-mic-shelf-content">
                <div class="channel-mic-source" title={endpointLabel(config.physicalMicDeviceId, 'Default microphone')}>
                  <span class="channel-mic-dot"></span>
                  <span class="channel-mic-text">{endpointLabel(config.physicalMicDeviceId, 'Default microphone')}</span>
                </div>
                <button
                  type="button"
                  class="channel-mic-monitor-btn"
                  class:active={config.micToMonitor}
                  aria-pressed={config.micToMonitor}
                  aria-label={config.micToMonitor ? "Stop playback of mic audio to monitor output" : "Playback mic audio to monitor output (Headphones)"}
                  data-tooltip={config.micToMonitor ? "Mic monitoring ON · Playing to monitor output (Headphones). Click to mute." : "Playback mic audio to monitor output (Headphones)"}
                  onclick={e => { e.stopPropagation(); onToggleMicToMonitor?.(); }}
                >
                  <StudioIcon name="headphones" size={15} />
                </button>
              </div>
              {#if draggedProc !== null}
                <div class="channel-mic-locked" class:locked-active={dragOverTarget === 'mic-locked'}>
                  <span>Hardware Mic only</span>
                </div>
              {/if}
            {:else}
              {@const busProcs = processesForBus(bus as PlaybackBusId)}
              {#if busProcs.length > 0}
                {@const MAX_VISIBLE = 3}
                {@const hasOverflow = busProcs.length > MAX_VISIBLE}
                {@const visibleProcs = hasOverflow ? busProcs.slice(0, MAX_VISIBLE) : busProcs}
                <div class="channel-chips-row">
                  {#each visibleProcs as proc (proc.pid)}
                    <div
                      class="channel-app-chip"
                      class:is-dragged={draggedProc?.pid === proc.pid}
                      class:just-routed={recentlyRoutedPid === proc.pid}
                      draggable="true"
                      role="button"
                      tabindex="0"
                      data-placement="top"
                      data-tooltip={`${proc.displayName} (${proc.exeName})`}
                      onpointerdown={e => onPointerDownProcess(e, proc, 'chip')}
                      ondragstart={e => onDragStartProcess(e, proc, 'chip')}
                      ondragend={onDragEndProcess}
                    >
                      <AppIcon {proc} size={18} color={BUS_COLORS[bus]}/>
                      <button
                        type="button"
                        class="chip-remove"
                        aria-label={`Unroute ${proc.displayName}`}
                        data-tooltip={`Unroute ${proc.displayName}`}
                        onclick={e => { e.stopPropagation(); onUnrouteProcess(proc.pid); }}
                      >
                        <svg viewBox="0 0 10 10" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true">
                          <path d="M2.5 2.5l5 5M7.5 2.5l-5 5" />
                        </svg>
                      </button>
                    </div>
                  {/each}

                  {#if hasOverflow}
                    <button
                      type="button"
                      class="channel-apps-more-btn"
                      data-placement="top"
                      data-tooltip={`Show all ${busProcs.length} ${bus} apps`}
                      aria-label={`Show all ${busProcs.length} ${bus} apps`}
                      onclick={e => { e.stopPropagation(); categoryModalBus = bus as PlaybackBusId; }}
                    >
                      <StudioIcon name="grid" size={13}/>
                      <span class="more-count">+{busProcs.length - MAX_VISIBLE}</span>
                    </button>
                  {/if}
                </div>
              {/if}

              <!-- Animated Drop Well Target (only when dragging an application) -->
              {#if draggedProc !== null}
                {@const isSameExplicitBus = (draggedProc.assignedBus ?? 'Aux') === bus && !isAutoAuxProcess(draggedProc)}
                <div class="shelf-drop-well" class:active-drop={dragOverTarget === bus} class:same-bus={isSameExplicitBus}>
                  <StudioIcon name={dragOverTarget === bus ? "check" : "plus"} size={13}/>
                  <span>
                    {#if dragOverTarget === bus}
                      {isSameExplicitBus ? `Keep on ${bus}` : `Route to ${bus}`}
                    {:else}
                      {isSameExplicitBus ? `Current (${bus})` : `+ Drop for ${bus}`}
                    {/if}
                  </span>
                </div>
              {:else if busProcs.length === 0}
                <div class="channel-sources-empty">Drop an app here</div>
              {/if}
            {/if}
          </div>
          <div class="meter-pair" class:dual={config.stream.enabled}>
          <div class="meter-matrix" role="img" aria-label={`${bus} stereo peak ${meterPeakDbString(m.peakLeft,m.peakRight)} dBFS; columns show left and right RMS and peak`}>
            {#each Array(meterCols) as _,col}<div class="dot-column">{#each Array(18) as _,row}<i class:lit={(17-row)/18*100<dotValue(m,col,meterCols)} class:peak={row<2} class:held={(col===meterCols/2-1||col===meterCols-1)&&Math.abs((17-row)/18*100-meterPercent(col===meterCols/2-1?peakHoldLeft[bus]:peakHoldRight[bus]))<3}></i>{/each}</div>{/each}
          </div>
          {#if config.stream.enabled}
            <div class="meter-matrix stream-matrix" role="img" aria-label={`${bus} level in the stream mix; columns show left and right RMS and peak`}>
              {#each Array(STREAM_COLS) as _,col}<div class="dot-column">{#each Array(18) as _,row}<i class:lit={(17-row)/18*100<dotValue(streamMeterForBus(bus),col,STREAM_COLS)} class:peak={row<2}></i>{/each}</div>{/each}
            </div>
          {/if}
          </div>
          <div class="meter-readout">
            <span class="readout" data-tooltip="Peak hold (dBFS)"><b>{meterPeakDbString(peakHoldLeft[bus],peakHoldRight[bus])}</b><small>peak</small></span>
            <span class="readout" data-tooltip="Momentary loudness, 400 ms (LUFS)"><b>{formatLufs(m.lufs)}</b><small>LUFS</small></span>
            <button type="button" class="clip-led" class:lit={clipLatched[bus]} aria-label={clipLatched[bus] ? `${bus} clipped. Click to clear` : `${bus} clip indicator`} data-tooltip={clipLatched[bus] ? 'Reached full scale. Click to clear' : 'Lights up if this channel reaches full scale'} onclick={()=>{clipLatched[bus]=false;}}>CLIP</button>
          </div>
          <label class="pre-gain" onwheel={e=>handlePreGainWheel(e,bus)}>Pre-gain<span><input type="number" min="-12" max="12" step=".5" aria-label={`${bus} pre-gain`} value={c.preGainDb} oninput={e=>{if(e.currentTarget.value!=='')onChangeBusVolumeMute(bus,c.volume,c.muted,Number(e.currentTarget.value));}}/>dB</span></label>
          <div class="channel-faders" class:dual-faders={config.stream.enabled} class:faders-linked={config.stream.enabled && linkedFaders[bus]}>
            {#if config.stream.enabled}
              <div class="dual-fader-header">
                <span class="fader-label">{bus === 'Mic' ? 'Mic out' : 'Monitor'}</span>
                <button
                  type="button"
                  class="fader-link-btn"
                  class:linked={linkedFaders[bus]}
                  aria-pressed={linkedFaders[bus]}
                  aria-label={`${linkedFaders[bus] ? 'Unlink' : 'Link'} ${bus} Monitor and Stream volume sliders`}
                  data-tooltip={linkedFaders[bus] ? `${bus} sliders linked · Click to unlink` : `Link ${bus} Monitor & Stream sliders`}
                  onclick={() => toggleLinkFaders(bus)}
                >
                  <StudioIcon name={linkedFaders[bus] ? 'link' : 'unlink'} size={12} />
                </button>
                <span class="fader-label stream-label" data-tooltip="Independent send to Stream. Double-click the slider to reset to 0 dB. Shift + scroll for fine adjustment. Unassigned apps use Aux.">Stream</span>
              </div>
            {/if}
            <div class="fader-column">
              <div class="channel-level">
                <EditableValue
                  value={c.volume > 0 ? 20 * Math.log10(c.volume) : -60}
                  min={-60}
                  max={3.521825}
                  step={0.1}
                  label={bus + ' level'}
                  unit="dB"
                  display={c.volume > 0 ? (20 * Math.log10(c.volume)).toFixed(2) + " dB" : "−∞ dB"}
                  allowSilence
                  oncommit={v => handleMonitorFaderInput(bus, Math.pow(10, v / 20))}
                  onsilence={() => handleMonitorFaderInput(bus, 0)}
                />
                <span class="activity-label">{muted ? 'Muted' : soloedBuses[bus] ? 'Solo' : duckedBuses.has(bus) ? 'Ducked' : 'Level'}</span>
              </div>
              <div class="fader-area">
                <div class="fader-ticks" aria-hidden="true">
                  <span style="top:calc(8px + (100% - 16px)*.0583)">+3</span>
                  <span style="top:calc(8px + (100% - 16px)/3)">0</span>
                  <span style="top:calc(100% - 8px)">−∞</span>
                </div>
                <div class="fader-track">
                  <div class="fader-fill" style={`height:${c.volume/1.5*100}%`}></div>
                  <div class="fader-unity"></div>
                  <input
                    type="range"
                    min="0"
                    max="1.5"
                    step=".01"
                    value={c.volume}
                    aria-label={`${bus} Volume Fader`}
                    oninput={e => handleMonitorFaderInput(bus, Number(e.currentTarget.value))}
                    ondblclick={() => handleResetMonitorFader(bus)}
                    onwheel={e => handleFaderWheel(e, bus)}
                  />
                </div>
                <div class="stereo-bars" aria-hidden="true">
                  <i style={`height:${meterPercent(m.peakLeft)}%`}></i>
                  <i style={`height:${meterPercent(m.peakRight)}%`}></i>
                </div>
              </div>
              <div class="channel-buttons">
                {#if bus === 'Mic'}
                  <button
                    type="button"
                    class="mic-sidetone-btn"
                    class:engaged={config.micToMonitor}
                    aria-pressed={config.micToMonitor}
                    aria-label={config.micToMonitor ? "Mute mic monitor playback" : "Playback mic audio to monitor output"}
                    data-tooltip={config.micToMonitor ? "Mic monitor playback ON (Headphones)" : "Playback mic audio to monitor output (Headphones)"}
                    onclick={() => onToggleMicToMonitor?.()}
                  >
                    <StudioIcon name="headphones" size={15} />
                  </button>
                {:else}
                  <button aria-label={`Solo ${bus}`} aria-pressed={soloedBuses[bus]} class:engaged={soloedBuses[bus]} onclick={() => onToggleSolo(bus)}>S</button>
                {/if}
                <button aria-label={`Mute ${bus}`} aria-pressed={c.muted} class:engaged={c.muted} onclick={() => onChangeBusVolumeMute(bus, c.volume, !c.muted, c.preGainDb)}>
                  <StudioIcon name={c.muted ? 'mute' : 'volume'} size={16} />
                </button>
              </div>
            </div>
            {#if config.stream.enabled}
              {@const send = config.stream.channels[bus]}
              <div class="fader-column stream-fader-column">
                <div class="channel-level">
                  <EditableValue
                    value={send.volume > 0 ? 20 * Math.log10(send.volume) : -60}
                    min={-60}
                    max={3.521825}
                    step={0.1}
                    label={bus + ' stream level'}
                    unit="dB"
                    display={send.volume > 0 ? (20 * Math.log10(send.volume)).toFixed(2) + ' dB' : '−∞ dB'}
                    disabled={streamSaving}
                    allowSilence
                    oncommit={v => handleStreamFaderInput(bus, Math.pow(10, v / 20))}
                    onsilence={() => handleStreamFaderInput(bus, 0)}
                  />
                </div>
                <div class="fader-area">
                  <div class="fader-ticks" aria-hidden="true">
                    <span style="top:calc(8px + (100% - 16px)*.0583)">+3</span>
                    <span style="top:calc(8px + (100% - 16px)/3)">0</span>
                    <span style="top:calc(100% - 8px)">−∞</span>
                  </div>
                  <div class="fader-track">
                    <div class="fader-fill" style={`height:${send.volume/1.5*100}%`}></div>
                    <div class="fader-unity"></div>
                    <input
                      type="range"
                      min="0"
                      max="1.5"
                      step=".01"
                      value={send.volume}
                      disabled={streamSaving}
                      aria-label={`${bus} Stream Volume Fader`}
                      aria-valuetext={send.muted ? 'Muted' : linearToDbString(send.volume) + ' dB'}
                      oninput={e => handleStreamFaderInput(bus, Number(e.currentTarget.value))}
                      ondblclick={() => handleResetStreamFader(bus)}
                      onwheel={e => handleStreamFaderWheel(e, bus)}
                    />
                  </div>
                </div>
                <div class="channel-buttons stream-send-buttons">
                  <button aria-label={`${send.muted ? 'Unmute' : 'Mute'} ${bus} stream send`} aria-pressed={send.muted} class:engaged={send.muted} disabled={streamSaving} onclick={() => onChangeStreamChannel(bus, send.volume, !send.muted)}>
                    <StudioIcon name={send.muted ? 'mute' : 'volume'} size={16} />
                  </button>
                </div>
                <span class="send-status">{send.muted ? 'Muted' : config.stream.categories.includes(bus) ? 'Included' : processes.some(p => (p.assignedBus ?? 'Aux') === bus && config.stream.appOverrides[p.exeName.toLowerCase()] === true) ? 'Overrides' : 'Excluded'}</span>
              </div>
            {/if}
          </div>
          <button class="channel-edit" onclick={()=>editChannel(bus)}>{selectedBus===bus?'Editing channel':'Edit channel'}<StudioIcon name="chevron" size={12}/></button>
        </section>
      {/each}
    </div>
    <div class="mix-bottom">
      <section class="chatmix panel"><div class="compact-title"><StudioIcon name="mix" size={17}/><h3>ChatMix</h3><button class="text-button" aria-label="Reset ChatMix" onclick={()=>onChangeChatMix(0)}>Reset</button></div><div class="balance-labels"><span>Game</span><span class="balance-value"><EditableValue value={config.chatMixBalance*100} min={-100} max={100} step={1} unit="%" label="ChatMix balance" display={config.chatMixBalance===0?'Perfect balance':Math.abs(Math.round(config.chatMixBalance*100))+'% toward '+(config.chatMixBalance<0?'Game':'Chat')} oncommit={v=>onChangeChatMix(v/100)}/></span><span>Chat</span></div><input class="horizontal-range chat-range" type="range" min="-1" max="1" step=".01" value={config.chatMixBalance} aria-label="Game and Chat balance" oninput={e=>onChangeChatMix(Number(e.currentTarget.value))} ondblclick={()=>onChangeChatMix(0)} onwheel={handleChatMixWheel}/></section>
      <section class="master-panel"><div class="master-heading"><span><StudioIcon name="headphones" size={18}/>Monitor output{#if streamMonitoring}<span class="stream-solo-pill">Stream Solo</span>{/if}</span><strong><EditableValue value={config.masterVolume*100} min={0} max={150} step={1} unit="%" label="Master monitor level" oncommit={v=>onChangeMasterVolume(v/100)}/></strong></div><input class="horizontal-range" type="range" min="0" max="1.5" step=".01" value={config.masterVolume} aria-label="Master monitor volume" oninput={e=>onChangeMasterVolume(Number(e.currentTarget.value))} ondblclick={()=>onChangeMasterVolume(1)} onwheel={handleMasterWheel}/><div class="master-footer"><span title={endpointLabel(config.masterOutputDeviceId,'System default')}>{endpointLabel(config.masterOutputDeviceId,'System default')}</span><span>Limiter −0.1 dBFS</span></div><div class="master-stereo" role="meter" aria-label="Master stereo peak" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(meterPercent(Math.max(meters.master.peakLeft,meters.master.peakRight)))}><i style={`width:${meterPercent(meters.master.peakLeft)}%`}></i><i style={`width:${meterPercent(meters.master.peakRight)}%`}></i></div></section>
    </div>
  </section>

  <section id="routing" class="routing panel" aria-label="App routing" ondragover={e=>{if(draggedProc){e.preventDefault();dragOverTarget='unroute';}}} ondrop={e=>{e.preventDefault();handleDropToUnroute();}} class:unroute-target={dragOverTarget==='unroute'}>
    <div class="routing-heading"><h2>App routing</h2><span class="count-badge">{processes.length}</span><button class="icon-button" aria-label="Refresh audio apps" onclick={onRefreshProcesses}><StudioIcon name="reset" size={16}/></button></div>
    <label class="search"><StudioIcon name="search" size={16}/><input type="search" bind:value={processSearch} aria-label="Search audio apps" placeholder="Find an app or PID…"/></label>
    <div class="filter-row">{#each [{id:'all',name:'All apps'},{id:'unrouted',name:'Auto (Aux)'},{id:'rules',name:'Rules'}] as tab}<button class:filter-active={poolTab===tab.id} aria-pressed={poolTab===tab.id} onclick={()=>poolTab=tab.id as typeof poolTab}>{tab.name}{#if tab.id==='unrouted'}<span>{processes.filter(p=>isAutoAuxProcess(p)).length}</span>{/if}</button>{/each}</div>
    <div class="app-list-wrap">
      {#if draggedProc && (dragSource === 'chip' || !isAutoAuxProcess(draggedProc))}
        <div class="unroute-shelf" class:shelf-active={dragOverTarget==='unroute'}>
          <StudioIcon name="reset" size={16}/>
          <div class="unroute-shelf-text">
            <strong>Drop to reset {draggedProc.displayName}</strong>
            <small>Restores system playback (auto-routed to Aux)</small>
          </div>
        </div>
      {/if}
      <div class="app-list">{#if poolTab==='rules'}{#each Object.entries(config.processRoutingRules).filter(([exe])=>exe.toLowerCase().includes(processSearch.toLowerCase())) as [exe,bus]}<div class="app-row rule-row"><StudioIcon name="route" size={17}/><div class="app-details"><strong>{exe}</strong><span>Auto-route · {bus}</span></div><button class="icon-button" aria-label={`Remove ${exe} routing rule`} onclick={()=>onRemoveRoutingRule(exe)}><StudioIcon name="close" size={14}/></button></div>{:else}<div class="empty-state"><strong>No saved routes</strong><p>All apps are currently auto-routed to Aux.</p></div>{/each}
      {:else}{#each filteredPoolProcesses() as proc (proc.pid)}{@const effectiveBus = proc.assignedBus ?? 'Aux'}<div class="app-row" class:is-being-dragged={draggedProc?.pid===proc.pid} class:just-routed={recentlyRoutedPid===proc.pid} role="group" aria-label={proc.displayName} draggable="true" onpointerdown={e=>onPointerDownProcess(e,proc,'pool')} ondragstart={e=>onDragStartProcess(e,proc,'pool')} ondragend={onDragEndProcess}><AppIcon {proc} size={30} color={BUS_COLORS[effectiveBus]} /><div class="app-details"><strong title={proc.displayName}>{proc.displayName}</strong><span title={proc.exeName}>{proc.exeName} · {proc.pid === 0 ? 'System' : proc.pid}</span>{#if proc.pid !== 0}<label class="app-volume" onpointerdown={e=>e.stopPropagation()} draggable="true" ondragstart={e=>{e.preventDefault();e.stopPropagation();}}><span class="sr-only">{proc.displayName} volume</span><input class="horizontal-range" type="range" min="0" max="1.5" step="0.01" value={appVolume(proc.exeName)} aria-label={`${proc.displayName} volume`} aria-valuetext={volumeLabel(appVolume(proc.exeName))} data-tooltip={`App volume: ${volumeLabel(appVolume(proc.exeName))}. Double-click to reset.`} oninput={e=>onChangeProcessVolume?.(proc.exeName,Number(e.currentTarget.value))} ondblclick={()=>onChangeProcessVolume?.(proc.exeName,1)}/><em>{volumeLabel(appVolume(proc.exeName))}</em></label>{/if}</div><label class="route-select" style={`color:${BUS_COLORS[effectiveBus]}`}><span class="sr-only">Route {proc.displayName}</span><StudioSelect value={effectiveBus} onchange={e=>routeSelection(proc,e.currentTarget.value)}>{#each PLAYBACK_BUSES as bus}<option>{bus}</option>{/each}</StudioSelect><StudioIcon name="down" size={11}/></label></div>{:else}<div class="empty-state"><StudioIcon name="search" size={25}/><strong>{processSearch?'No apps found':'No auto-Aux apps'}</strong><p>{processSearch?'Try another name or executable.':'New audio apps automatically route to Aux.'}</p><button class="quiet-button" onclick={()=>{processSearch='';poolTab='all';}}>Show all apps</button></div>{/each}{/if}</div>
    </div>
    <div class="routing-tip"><StudioIcon name="info" size={15}/><p>New apps automatically route to Aux. Drag an app onto a channel or choose a bus to reassign.</p></div><div class="routing-footer"><span><span class="status-dot"></span>{processes.length} apps routed</span><span>{native?'Live sessions':'Preview data'}</span></div>
  </section>

  <div id="stream" class="stream-slot">{@render streamOutput?.()}</div>
  <div id="equalizer" class="live-eq-slot"><EqCanvas bus={selectedBus} bands={selectedConfig.eqBands} accentColor={BUS_COLORS[selectedBus]} onUpdateBand={(idx,band)=>onChangeEqBand(selectedBus,idx,band)} onResetFlat={()=>onResetEqFlat(selectedBus)} onResetDefaults={()=>onResetEqDefaults?.(selectedBus)} onToggleGlobalEq={(enabled)=>onToggleEqGlobal?.(selectedBus,enabled)} onApplyPreset={p=>onApplyEqPreset(selectedBus,p)}/><ChannelChain bus={selectedBus} config={selectedConfig} meter={meterForBus(selectedBus)} accentColor={BUS_COLORS[selectedBus]} {native} onToggle={toggleStage}/></div>
  <section id="finetune" class="dynamics panel" aria-label={`${selectedBus} dynamics`} style={`--card-accent:${BUS_COLORS[selectedBus]}`}>
    <div class="section-heading"><h2>Fine-tune</h2><ModeSwitch/><span style={`color:${BUS_COLORS[selectedBus]}`}>{selectedBus}</span></div>
    <QuickSetup bus={selectedBus} current={allSections} {simple} onApply={applyBundle} {onNotify}/>
    <div class="fine-grid" use:masonry>
    <div class="fine-card" class:on={selectedConfig.noiseGate.enabled}>
    <div class="dynamics-row"><span class="dynamics-icon"><StudioIcon name="wave" size={18}/></span><div><h3>Noise gate</h3><p>Keep the quiet parts quiet.</p></div><button class="switch" class:on={selectedConfig.noiseGate.enabled} aria-pressed={selectedConfig.noiseGate.enabled} aria-label={`${selectedBus} noise gate`} onclick={()=>onChangeDynamics(selectedBus,{...selectedConfig.noiseGate,enabled:!selectedConfig.noiseGate.enabled},selectedConfig.compressor)}><span></span></button></div>
    <FinePresetBar section="gate" bus={selectedBus} current={allSections.gate!} {simple} open={!!openSections.gate} onToggleOpen={() => (openSections.gate = !openSections.gate)} onApply={(v, l) => applyBundle({ gate: v }, l)} {onNotify}/>
    {#if showParams("gate")}<div class="dynamics-params">{#each gateParams as param}<label class="dynamics-control"><span>{param.label}<strong><EditableValue value={selectedConfig.noiseGate[param.key]} min={param.min} max={param.max} step={param.step} unit={param.unit} label={selectedBus+' gate '+param.label} oncommit={v=>onChangeDynamics(selectedBus,{...selectedConfig.noiseGate,[param.key]:v},selectedConfig.compressor)}/></strong></span><input class="horizontal-range" type="range" min={param.min} max={param.max} step={param.step} value={selectedConfig.noiseGate[param.key]} aria-label={`${selectedBus} gate ${param.label}`} oninput={e=>onChangeDynamics(selectedBus,{...selectedConfig.noiseGate,[param.key]:Number(e.currentTarget.value)},selectedConfig.compressor)}/></label>{/each}</div>{/if}
    </div>
    <div class="fine-card" class:on={selectedConfig.compressor.enabled}>
    <div class="dynamics-row"><span class="dynamics-icon"><StudioIcon name="settings" size={18}/></span><div><h3>Compressor</h3><p>Smooth out the unexpected.</p></div><button class="switch" class:on={selectedConfig.compressor.enabled} aria-pressed={selectedConfig.compressor.enabled} aria-label={`${selectedBus} compressor`} onclick={()=>onChangeDynamics(selectedBus,selectedConfig.noiseGate,{...selectedConfig.compressor,enabled:!selectedConfig.compressor.enabled})}><span></span></button></div>
    <FinePresetBar section="comp" bus={selectedBus} current={allSections.comp!} {simple} open={!!openSections.comp} onToggleOpen={() => (openSections.comp = !openSections.comp)} onApply={(v, l) => applyBundle({ comp: v }, l)} {onNotify}/>
    {#if showParams("comp")}<div class="dynamics-params">{#each compParams as param}<label class="dynamics-control"><span>{param.label}<strong><EditableValue value={selectedConfig.compressor[param.key]} min={param.min} max={param.max} step={param.step} unit={param.unit} label={selectedBus+' compressor '+param.label} oncommit={v=>onChangeDynamics(selectedBus,selectedConfig.noiseGate,{...selectedConfig.compressor,[param.key]:v})}/></strong></span><input class="horizontal-range" type="range" min={param.min} max={param.max} step={param.step} value={selectedConfig.compressor[param.key]} aria-label={`${selectedBus} compressor ${param.label}`} oninput={e=>onChangeDynamics(selectedBus,selectedConfig.noiseGate,{...selectedConfig.compressor,[param.key]:Number(e.currentTarget.value)})}/></label>{/each}</div>{/if}
    </div>
    <div class="fine-card" class:on={selLimiter.enabled}>
    <div class="dynamics-row"><span class="dynamics-icon"><StudioIcon name="volume" size={18}/></span><div><h3>Limiter</h3><p>Never let this channel clip.</p></div><button class="switch" class:on={selLimiter.enabled} aria-pressed={selLimiter.enabled} aria-label={`${selectedBus} limiter`} onclick={()=>changeExtras({limiter:{enabled:!selLimiter.enabled}})}><span></span></button></div>
    <FinePresetBar section="limiter" bus={selectedBus} current={allSections.limiter!} {simple} open={!!openSections.limiter} onToggleOpen={() => (openSections.limiter = !openSections.limiter)} onApply={(v, l) => applyBundle({ limiter: v }, l)} {onNotify}/>
    {#if showParams("limiter")}<div class="dynamics-params">{#each limiterParams as param}<label class="dynamics-control"><span>{param.label}<strong><EditableValue value={selLimiter[param.key]} min={param.min} max={param.max} step={param.step} unit={param.unit} label={selectedBus+' limiter '+param.label} oncommit={v=>changeExtras({limiter:{[param.key]:v}})}/></strong></span><input class="horizontal-range" type="range" min={param.min} max={param.max} step={param.step} value={selLimiter[param.key]} aria-label={`${selectedBus} limiter ${param.label}`} oninput={e=>changeExtras({limiter:{[param.key]:Number(e.currentTarget.value)}})}/></label>{/each}</div>{/if}
    </div>
    <div class="fine-card" class:on={selDeEsser.enabled}>
    <div class="dynamics-row"><span class="dynamics-icon"><StudioIcon name="wave" size={18}/></span><div><h3>De-esser</h3><p>Soften harsh “s” and “t” sounds.</p></div><button class="switch" class:on={selDeEsser.enabled} aria-pressed={selDeEsser.enabled} aria-label={`${selectedBus} de-esser`} onclick={()=>changeExtras({deEsser:{enabled:!selDeEsser.enabled}})}><span></span></button></div>
    <FinePresetBar section="deEsser" bus={selectedBus} current={allSections.deEsser!} {simple} open={!!openSections.deEsser} onToggleOpen={() => (openSections.deEsser = !openSections.deEsser)} onApply={(v, l) => applyBundle({ deEsser: v }, l)} {onNotify}/>
    {#if showParams("deEsser")}<div class="dynamics-params">{#each deEsserParams as param}<label class="dynamics-control"><span>{param.label}<strong><EditableValue value={selDeEsser[param.key]} min={param.min} max={param.max} step={param.step} unit={param.unit} label={selectedBus+' de-esser '+param.label} oncommit={v=>changeExtras({deEsser:{[param.key]:v}})}/></strong></span><input class="horizontal-range" type="range" min={param.min} max={param.max} step={param.step} value={selDeEsser[param.key]} aria-label={`${selectedBus} de-esser ${param.label}`} oninput={e=>changeExtras({deEsser:{[param.key]:Number(e.currentTarget.value)}})}/></label>{/each}</div>{/if}
    </div>
    {#if selectedBus === 'Mic' || selectedBus === 'Chat'}
    <div class="fine-card" class:on={selNoise.enabled}>
      <div class="dynamics-row"><span class="dynamics-icon"><StudioIcon name="mic" size={18}/></span><div><h3>Noise suppression</h3><p>AI voice cleanup (adds about 10 ms of delay).</p></div><button class="switch" class:on={selNoise.enabled} aria-pressed={selNoise.enabled} aria-label={`${selectedBus} noise suppression`} onclick={()=>changeExtras({noise:{enabled:!selNoise.enabled}})}><span></span></button></div>
      <div class="noise-engine" role="group" aria-label={`${selectedBus} noise suppression engine`}>
        <span class="noise-engine-label">Engine</span>
        <div class="noise-engine-options">
          <button type="button" class:active={(selNoise.engine ?? "fast") === "fast"} aria-pressed={(selNoise.engine ?? "fast") === "fast"} data-tooltip="RNNoise: light on the CPU, about 10 ms delay" onclick={() => changeExtras({ noise: { engine: "fast" } })}>Fast</button>
          <button type="button" class:active={selNoise.engine === "high"} aria-pressed={selNoise.engine === "high"} data-tooltip="DeepFilterNet3: cleaner voice, more CPU, about 45 ms delay" onclick={() => changeExtras({ noise: { engine: "high" } })}>High quality</button>
        </div>
        {#if selNoise.engine === "high"}<span class="noise-engine-note">More CPU · about 45 ms delay</span>{/if}
      </div>
      <FinePresetBar section="noise" bus={selectedBus} current={allSections.noise!} {simple} open={!!openSections.noise} onToggleOpen={() => (openSections.noise = !openSections.noise)} onApply={(v, l) => applyBundle({ noise: v }, l)} {onNotify}/>
    {#if showParams("noise")}<div class="dynamics-params"><label class="dynamics-control"><span>Strength<strong>{Math.round(selNoise.strength*100)}%</strong></span><input class="horizontal-range" type="range" min="0" max="1" step="0.05" value={selNoise.strength} aria-label={`${selectedBus} noise suppression strength`} oninput={e=>changeExtras({noise:{strength:Number(e.currentTarget.value)}})}/></label></div>{/if}
    </div>
    {/if}
    </div>
  </section>
</div>

{#if draggedProc}
  {@const activeColor = dragOverTarget === 'unroute' ? '#87ddcf' : dragOverTarget === 'mic-locked' ? '#f0a7c6' : dragOverTarget ? BUS_COLORS[dragOverTarget as BusId] : (draggedProc.assignedBus ? BUS_COLORS[draggedProc.assignedBus] : '#b8a1fa')}
  <div
    class="drag-cursor-follower"
    class:over-target={dragOverTarget !== null && dragOverTarget !== 'mic-locked'}
    class:over-locked={dragOverTarget === 'mic-locked'}
    style={`--cursor-x:${dragCursorX}px; --cursor-y:${dragCursorY}px; --offset-x:${dragOffsetX.toFixed(1)}px; --offset-y:${dragOffsetY.toFixed(1)}px; --drag-tilt:${dragTilt.toFixed(2)}deg; --drag-accent:${activeColor};`}
    aria-hidden="true"
  >
    <div class="drag-cursor-badge">
      <AppIcon proc={draggedProc} size={22} color={activeColor} />
      <div class="drag-cursor-meta">
        <strong class="drag-cursor-name">{draggedProc.displayName}</strong>
        <span class="drag-cursor-exe">{draggedProc.pid === 0 ? 'System' : draggedProc.exeName}</span>
      </div>
      <span
        class="drag-cursor-target"
        class:is-unroute={dragOverTarget === 'unroute'}
        class:is-locked={dragOverTarget === 'mic-locked'}
      >
        {#if dragOverTarget === 'unroute'}
          ↺ Reset to Aux
        {:else if dragOverTarget === 'mic-locked'}
          ✕ Mic locked
        {:else if dragOverTarget}
          → {dragOverTarget}
        {:else}
          Drop on channel
        {/if}
      </span>
    </div>
  </div>
{/if}

<CategoryAppsModal
  bus={categoryModalBus}
  {processes}
  accentColor={categoryModalBus ? BUS_COLORS[categoryModalBus] : '#b8a1fa'}
  onClose={() => (categoryModalBus = null)}
  {onRouteProcess}
  {onUnrouteProcess}
/>

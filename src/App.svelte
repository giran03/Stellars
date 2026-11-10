<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, Channel } from "@tauri-apps/api/core";
  import StudioIcon from "./components/StudioIcon.svelte";
  import StudioTooltips from "./components/StudioTooltips.svelte";
  import "./studio.css";
  import MixerView from "./components/MixerView.svelte";
  import StreamPanel from "./components/StreamPanel.svelte";
  import ConfirmModal from "./components/ConfirmModal.svelte";
  import SettingsModal, { type SettingsTabId } from "./components/SettingsModal.svelte";
  import HardwareSettings from "./components/HardwareSettings.svelte";
  import SetupWizard from "./components/SetupWizard.svelte";
  import ModePrompt from "./components/ModePrompt.svelte";
  import Rail from "./components/Rail.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { installBackgroundSaver, type BackgroundSaver } from "./backgroundSaver";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import PresetsMenu from "./components/PresetsMenu.svelte";
  import { appSettings, isNativeApp, loadAppSettings, updateAppSettings } from "./appSettings.svelte";
  import { loadPresets } from "./presets.svelte";
  import { ALL_EQ_PRESETS, getEqPresetsForBus, type EqPreset } from "./dspMath";
  import {
    BUS_COLORS,
    DEFAULT_DUCKING,
    type DeEsserConfig,
    type DuckingConfig,
    type LimiterConfig,
    type NoiseSuppressionConfig,
    type ActiveAudioProcess,
    type AllBusMetersPayload,
    type AudioEndpointInfo,
    type BusId,
    type CompressorConfig,
    type DiagnosticsSnapshot,
    type EqBandConfig,
    type FullRouterStatePayload,
    type NoiseGateConfig,
    type PlaybackBusId,
    type RouteSlotStatus,
    type RouterConfig,
    type StreamConfig,
    type VirtualCableEndpointPair,
    type VirtualCableSetupResult,
    type VirtualCableStatusPayload,
  } from "./types";

  type Theme = "dark" | "light";
  const THEME_KEY = "stellars-theme";
  let theme = $state<Theme>("dark");
  function setTheme(next: Theme) {
    theme = next;
    document.documentElement.dataset.theme = next;
    document.documentElement.style.colorScheme = next;
    try { localStorage.setItem(THEME_KEY, next); } catch {}
  }
  onMount(() => {
    let saved: string | null = null;
    try { saved = localStorage.getItem(THEME_KEY); } catch {}
    setTheme(saved === "light" ? "light" : "dark");
    const handleKeydown = (e: KeyboardEvent) => {
      if (e.altKey && (e.key === "t" || e.key === "T")) {
        e.preventDefault();
        toggleTheme();
      }
    };
    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  });
  function toggleTheme() { setTheme(theme === "dark" ? "light" : "dark"); }

  const DEFAULT_FREQS = [31.25, 62.5, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
  const BUS_ORDER: BusId[] = ["Game", "Chat", "Media", "Aux", "Mic"];

  function createDefaultBands(): EqBandConfig[] {
    return DEFAULT_FREQS.map((freq, idx) => ({
      enabled: true,
      type: idx === 0 ? "LowShelf" : idx === 9 ? "HighShelf" : "Peaking",
      freq,
      gainDb: 0.0,
      q: idx === 0 || idx === 9 ? 0.707 : 1.414,
    }));
  }

  function createFallbackConfig(): RouterConfig {
    const makeBus = (defaultPresetId?: string) => {
      const bands = createDefaultBands();
      if (defaultPresetId) {
        const preset = ALL_EQ_PRESETS.find((p) => p.id === defaultPresetId);
        if (preset) {
          preset.gains.forEach((g, i) => {
            bands[i].gainDb = g;
          });
        }
      }
      return {
        preGainDb: 0,
        volume: 1.0,
        muted: false,
        noiseGate: { enabled: false, thresholdDb: -48, ratio: 4, attackMs: 2, releaseMs: 80 },
        compressor: {
          enabled: false,
          thresholdDb: -18,
          ratio: 3,
          attackMs: 10,
          releaseMs: 120,
          makeupGainDb: 0,
          kneeDb: 6,
        },
        eqBands: bands,
      };
    };

    const gameBus = makeBus("fps");
    gameBus.compressor.enabled = true;

    const chatBus = makeBus("voice");
    chatBus.noiseGate.enabled = true;

    const micBus = makeBus("broadcast");
    micBus.noiseGate.enabled = true;
    micBus.compressor.enabled = true;

    return {
      stream: {
        enabled: false,
        outputDeviceId: "ep-render-stream",
        volume: 1,
        muted: false,
        categories: ["Game", "Chat", "Mic"],
        micSource: "virtual",
        appOverrides: {},
        channels: Object.fromEntries(["Game", "Chat", "Media", "Aux", "Mic"].map(bus => [bus, { volume: 1, muted: false }])) as StreamConfig["channels"],
      },
      masterOutputDeviceId: "ep-render-headphones",
      dummySinkDeviceId: "ep-render-vbcable",
      physicalMicDeviceId: "ep-capture-shure",
      virtualMicOutputDeviceId: "ep-render-virtmic",
      chatMixBalance: 0.0,
      masterVolume: 1.0,
      micToMonitor: false,
      processRoutingRules: {
        "cs2.exe": "Game",
        "discord.exe": "Chat",
        "spotify.exe": "Media",
        "obs64.exe": "Aux",
      },
      buses: {
        Game: gameBus,
        Chat: chatBus,
        Media: makeBus("cinema"),
        Aux: makeBus("flat"),
        Mic: micBus,
      },
    };
  }

  function createFallbackEndpoints(): AudioEndpointInfo[] {
    return [
      {
        id: "ep-render-stream",
        name: "Hi-Fi Cable Input (VB-Audio Hi-Fi Cable)",
        flow: "Render",
        isDefault: false,
        isDummySinkCandidate: true,
        isVirtualMicCandidate: true,
      },
      {
        id: "ep-capture-stream",
        name: "Hi-Fi Cable Output (VB-Audio Hi-Fi Cable)",
        flow: "Capture",
        isDefault: false,
        isDummySinkCandidate: false,
        isVirtualMicCandidate: false,
      },
      {
        id: "ep-render-headphones",
        name: "RME ADI-2 DAC FS (48kHz 32-bit Float)",
        flow: "Render",
        isDefault: true,
        isDummySinkCandidate: false,
        isVirtualMicCandidate: false,
      },
      {
        id: "ep-render-vbcable",
        name: "Steam Streaming Speakers (Silent Sink)",
        flow: "Render",
        isDefault: false,
        isDummySinkCandidate: true,
        isVirtualMicCandidate: false,
      },
      {
        id: "ep-render-virtmic",
        name: "CABLE Input (VB-Audio Virtual Cable)",
        flow: "Render",
        isDefault: false,
        isDummySinkCandidate: true,
        isVirtualMicCandidate: true,
      },
      {
        id: "ep-capture-virtmic",
        name: "CABLE Output (VB-Audio Virtual Cable)",
        flow: "Capture",
        isDefault: false,
        isDummySinkCandidate: false,
        isVirtualMicCandidate: false,
      },
      {
        id: "ep-capture-shure",
        name: "Shure SM7B · Audient iD14 MKII Loop",
        flow: "Capture",
        isDefault: true,
        isDummySinkCandidate: false,
        isVirtualMicCandidate: false,
      },
    ];
  }

  function classifyEndpointName(name: string): { kind: string; group: string; watermarkFree: boolean } | null {
    const lower = name.toLowerCase();
    if (
      lower.includes("hi-fi cable") ||
      lower.includes("stellars hi-fi") ||
      lower.includes("hi-fi virtual cable") ||
      lower.includes("stream mix")
    ) {
      return { kind: "hifi-cable", group: "vbcable-hifi", watermarkFree: true };
    }
    if (lower.includes("cable-a") || lower.includes("stellars cable-a")) return { kind: "vb-cable-abcd", group: "vbcable-a", watermarkFree: true };
    if (lower.includes("cable-b") || lower.includes("stellars cable-b")) return { kind: "vb-cable-abcd", group: "vbcable-b", watermarkFree: true };
    if (lower.includes("cable-c") || lower.includes("stellars cable-c")) return { kind: "vb-cable-abcd", group: "vbcable-c", watermarkFree: true };
    if (lower.includes("cable-d") || lower.includes("stellars cable-d")) return { kind: "vb-cable-abcd", group: "vbcable-d", watermarkFree: true };
    if (
      lower.includes("vb-audio virtual cable") ||
      lower.includes("stellars virtual audio cable") ||
      lower.includes("vbaudiovacwdm") ||
      lower.includes("aux in 16") ||
      lower.includes("virtual mic in") ||
      lower.includes("stellars - microphone") ||
      lower.startsWith("cable input") ||
      lower.startsWith("cable output") ||
      lower.startsWith("cable in 16")
    ) {
      return { kind: "vb-cable", group: "vbcable-main", watermarkFree: true };
    }
    if (lower.includes("steam streaming microphone")) return { kind: "steam", group: "steam-mic", watermarkFree: true };
    if (lower.includes("steam streaming speakers")) return { kind: "steam", group: "steam-speakers", watermarkFree: true };
    if (lower.includes("voicemeeter") || lower.includes("vaio")) {
      const sub = lower.includes("aux") ? "voicemeeter-aux" : "voicemeeter-main";
      return { kind: "voicemeeter", group: sub, watermarkFree: true };
    }
    if (lower.includes("oculus virtual audio device")) return { kind: "oculus", group: "oculus-vad", watermarkFree: true };
    if (lower.includes("sonar") && lower.includes("virtual")) {
      return { kind: "sonar", group: `sonar:${lower}`, watermarkFree: true };
    }
    if (lower.includes("virtual audio cable") || lower.includes("virtual cable") || lower.includes("vad sink")) {
      const isEugene = lower.includes("(virtual audio cable)");
      return { kind: "other-virtual", group: `other:${lower}`, watermarkFree: !isEugene };
    }
    return null;
  }

  function computeLocalCableStatus(cfg: RouterConfig, eps: AudioEndpointInfo[]): VirtualCableStatusPayload {
    const captures = eps.filter((e) => e.flow === "Capture");
    const detectedCables: VirtualCableEndpointPair[] = [];

    for (const ep of eps.filter((e) => e.flow === "Render")) {
      const info = classifyEndpointName(ep.name);
      if (!info) continue;
      const paired = captures.find((cap) => {
        const capInfo = classifyEndpointName(cap.name);
        return capInfo && (capInfo.group === info.group || (info.group === "steam-speakers" && capInfo.group === "steam-mic"));
      });
      detectedCables.push({
        renderId: ep.id,
        renderName: ep.name,
        pairedCaptureId: paired?.id ?? null,
        pairedCaptureName: paired?.name ?? null,
        driverKind: info.kind,
        groupKey: info.group,
        isWatermarkFree: info.watermarkFree,
      });
    }

    const masterId =
      cfg.masterOutputDeviceId ??
      eps.find((e) => e.flow === "Render" && e.isDefault)?.id ??
      null;

    const groupForId = (id: string) => {
      const found = eps.find((e) => e.id === id);
      if (!found) return `id:${id}`;
      return classifyEndpointName(found.name)?.group ?? `id:${id}`;
    };

    const evalSlot = (
      slot: "silentSink" | "virtualMic" | "streamMix",
      deviceId: string | null,
      others: (string | null)[],
      emptyHint: string,
      readyPrefix: string
    ): RouteSlotStatus => {
      if (!deviceId || !deviceId.trim()) {
        return {
          slot,
          deviceId: null,
          deviceName: null,
          pairedCaptureName: null,
          status: "unassigned",
          hint: emptyHint,
        };
      }
      const ep = eps.find((e) => e.flow === "Render" && e.id === deviceId);
      if (!ep) {
        return {
          slot,
          deviceId,
          deviceName: null,
          pairedCaptureName: null,
          status: "missing",
          hint: "Selected endpoint is disconnected or disabled. Click Repair or choose another cable.",
        };
      }
      const pair = detectedCables.find((c) => c.renderId === ep.id);
      const pairedCaptureName = pair?.pairedCaptureName ?? null;
      const myGroup = groupForId(deviceId);

      if (masterId === deviceId) {
        return {
          slot,
          deviceId,
          deviceName: ep.name,
          pairedCaptureName,
          status: "conflict",
          hint: "Conflicts with Monitor Output (Headphones). Choose a separate virtual cable.",
        };
      }

      for (const other of others) {
        if (other && (other === deviceId || groupForId(other) === myGroup)) {
          return {
            slot,
            deviceId,
            deviceName: ep.name,
            pairedCaptureName,
            status: "conflict",
            hint: "Shares the same virtual cable pipe with another Stellars route. Assign a separate cable.",
          };
        }
      }

      const hint =
        slot === "silentSink"
          ? "Silences raw dry app audio for Game, Chat, Media, and Aux."
          : pairedCaptureName
            ? `${readyPrefix}: ${pairedCaptureName}`
            : `Sending audio to ${ep.name}`;

      return {
        slot,
        deviceId,
        deviceName: ep.name,
        pairedCaptureName,
        status: "ready",
        hint,
      };
    };

    const silentSink = evalSlot(
      "silentSink",
      cfg.dummySinkDeviceId,
      [cfg.virtualMicOutputDeviceId, cfg.stream.outputDeviceId],
      "Unassigned · Routed apps will still play dry audio on default speakers until assigned.",
      "Silent sink active"
    );
    const virtualMic = evalSlot(
      "virtualMic",
      cfg.virtualMicOutputDeviceId,
      [cfg.dummySinkDeviceId, cfg.stream.outputDeviceId],
      "Unassigned · Assign a virtual cable to feed your EQ/gated Mic into Discord or games.",
      "In Discord / Game Voice, select Mic"
    );
    const streamMix = evalSlot(
      "streamMix",
      cfg.stream.outputDeviceId,
      [cfg.dummySinkDeviceId, cfg.virtualMicOutputDeviceId],
      "Unassigned · Assign a dedicated virtual cable to capture your Stream Mix in OBS.",
      "In OBS Audio Input Capture, select"
    );

    const vbInstalled = detectedCables.some((c) => c.driverKind === "vb-cable") ||
      eps.some((e) => classifyEndpointName(e.name)?.kind === "vb-cable");
    const hifiInstalled = detectedCables.some((c) => c.driverKind === "hifi-cable") ||
      eps.some((e) => classifyEndpointName(e.name)?.kind === "hifi-cable");
    return {
      vbCableInstalled: vbInstalled,
      hifiCableInstalled: hifiInstalled,
      steamAudioAvailable: detectedCables.some((c) => c.driverKind === "steam"),
      detectedCables,
      silentSink,
      virtualMic,
      streamMix,
      allRoutesHealthy:
        silentSink.status === "ready" &&
        virtualMic.status === "ready" &&
        streamMix.status === "ready",
    };
  }

  function createFallbackProcesses(): ActiveAudioProcess[] {
    return [
      {
        pid: 4812,
        exeName: "cs2.exe",
        exePath: "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Counter-Strike Global Offensive\\game\\bin\\win64\\cs2.exe",
        displayName: "Counter-Strike 2",
        assignedBus: "Game",
        isRouted: true,
      },
      {
        pid: 9104,
        exeName: "discord.exe",
        exePath: "C:\\Users\\Operator\\AppData\\Local\\Discord\\app-1.0.9168\\Discord.exe",
        displayName: "Discord Voice Comms",
        assignedBus: "Chat",
        isRouted: true,
      },
      {
        pid: 2210,
        exeName: "spotify.exe",
        exePath: "C:\\Users\\Operator\\AppData\\Roaming\\Spotify\\Spotify.exe",
        displayName: "Spotify Desktop",
        assignedBus: "Media",
        isRouted: true,
      },
      {
        pid: 7740,
        exeName: "obs64.exe",
        exePath: "C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe",
        displayName: "OBS Studio Monitor",
        assignedBus: "Aux",
        isRouted: true,
      },
      {
        pid: 6328,
        exeName: "chrome.exe",
        exePath: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
        displayName: "Google Chrome Audio",
        assignedBus: "Aux",
        isRouted: true,
      },
      {
        pid: 11420,
        exeName: "eldenring.exe",
        exePath: "C:\\Program Files (x86)\\Steam\\steamapps\\common\\ELDEN RING\\Game\\eldenring.exe",
        displayName: "Elden Ring",
        assignedBus: "Aux",
        isRouted: true,
      },
      {
        pid: 5096,
        exeName: "vlc.exe",
        exePath: "C:\\Program Files\\VideoLAN\\VLC\\vlc.exe",
        displayName: "VLC Media Player",
        assignedBus: "Aux",
        isRouted: true,
      },
    ];
  }

  const zeroMeter = { peakLeft: 0, peakRight: 0, rmsLeft: 0, rmsRight: 0 };

  let config: RouterConfig = $state(createFallbackConfig());
  let endpoints: AudioEndpointInfo[] = $state(createFallbackEndpoints());
  let processes: ActiveAudioProcess[] = $state(createFallbackProcesses());
  let diagnostics: DiagnosticsSnapshot = $state({
    masterRunning: true,
    micRunning: true,
    callbackCount: 14820,
    underrunCount: 0,
  });
  let meters: AllBusMetersPayload = $state({
    stream: zeroMeter,
    streamRunning: false,
    streamError: null,
    game: { peakLeft: 0.72, peakRight: 0.68, rmsLeft: 0.48, rmsRight: 0.45 },
    chat: { peakLeft: 0.54, peakRight: 0.52, rmsLeft: 0.34, rmsRight: 0.32 },
    media: { peakLeft: 0.62, peakRight: 0.65, rmsLeft: 0.44, rmsRight: 0.46 },
    aux: { peakLeft: 0.24, peakRight: 0.22, rmsLeft: 0.14, rmsRight: 0.12 },
    mic: { peakLeft: 0.58, peakRight: 0.58, rmsLeft: 0.36, rmsRight: 0.36 },
    master: { peakLeft: 0.81, peakRight: 0.79, rmsLeft: 0.56, rmsRight: 0.54 },
  });

  let selectedBus: BusId = $state("Game");
  let soloedBuses: Record<BusId, boolean> = $state({
    Game: false,
    Chat: false,
    Media: false,
    Aux: false,
    Mic: false,
  });
  let streamMonitoring: boolean = $state(false);
  let settingsOpen: boolean = $state(false);
  let settingsTab: SettingsTabId = $state("general");
  let wizardOpen: boolean = $state(false);
  let appSettingsLoaded = $state(false);
  function openSettings(tab?: SettingsTabId) {
    if (tab) settingsTab = tab;
    settingsOpen = true;
  }
  let simulateTelemetry: boolean = $state(false);
  let isTauriConnected: boolean = $state(false);
  let statusMessage: string = $state(
    "WASAPI Shared 48kHz Float32 · 10ms Event Clock · Ring-3 Anti-Cheat Safe"
  );
  let toastText: string | null = $state(null);
  let streamSaving = $state(false);
  let streamError = $state<string | null>(null);
  let backendCableStatus = $state<VirtualCableStatusPayload | null>(null);
  let cableInstalling = $state(false);
  let cableAutoRouting = $state(false);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  const cableStatus: VirtualCableStatusPayload = $derived(
    (isTauriConnected && backendCableStatus)
      ? backendCableStatus
      : computeLocalCableStatus(config, endpoints)
  );

  type ToastAction = { label: string; run: () => void };
  let toastAction: ToastAction | null = $state(null);

  /** Toasts that offer an action (e.g. Undo) are shown even when notifications are off. */
  function showToast(msg: string, action?: ToastAction) {
    if (!appSettings.showNotifications && !action) return;
    toastText = msg;
    toastAction = action ?? null;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toastText = null;
      toastAction = null;
    }, action ? 7000 : 3200);
  }

  function runToastAction() {
    const action = toastAction;
    toastText = null;
    toastAction = null;
    action?.run();
  }

  async function restoreEqBands(bus: BusId, bands: EqBandConfig[]) {
    config.buses[bus].eqBands = bands;
    showToast(`${bus.toUpperCase()} EQ restored`);
    for (let i = 0; i < bands.length; i++) {
      try {
        await invoke("set_eq_band", { bus, bandIndex: i, band: bands[i] });
      } catch {}
    }
  }

  async function restoreRoutes(rules: Record<string, BusId>) {
    let restored = 0;
    for (const p of processes) {
      const bus = rules[p.exeName.toLowerCase()];
      if (bus && bus !== "Mic") {
        await handleRouteProcess(p.pid, p.exeName, bus);
        restored++;
      }
    }
    showToast(restored ? `Restored ${restored} route${restored === 1 ? "" : "s"}` : "No running apps to restore routes for");
  }

  function ensureDefaultAuxRouting(list: ActiveAudioProcess[], rules: Record<string, BusId>): ActiveAudioProcess[] {
    return list.map((p) => {
      const ruleBus = rules[p.exeName.toLowerCase()];
      const assignedBus: BusId = p.assignedBus ?? (ruleBus && ruleBus !== "Mic" ? ruleBus : "Aux");
      return {
        ...p,
        assignedBus,
        isRouted: true,
      };
    });
  }

  async function loadRouterState() {
    try {
      const payload = await invoke<FullRouterStatePayload>("get_router_state");
      config = payload.config;
      endpoints = payload.endpoints;
      processes = ensureDefaultAuxRouting(payload.processes, payload.config.processRoutingRules);
      diagnostics = payload.diagnostics;
      if (payload.cableStatus) {
        backendCableStatus = payload.cableStatus;
      }
      isTauriConnected = true;
    } catch {
      isTauriConnected = false;
      simulateTelemetry = true;
      statusMessage = "Preview workspace · Connect the desktop app to route live audio";
    }
  }

  async function connectMeterStream() {
    try {
      const onMeter = new Channel<AllBusMetersPayload>();
      onMeter.onmessage = (payload) => {
        meters = payload;
      };
      await invoke("subscribe_peak_meters", { onMeter });
    } catch {
      // Handled by optional local telemetry simulation in preview mode
    }
  }

  // Coalesce drag events and serialize writes so rapid moves cannot save out of order.
  const pendingStreamChannels = new Map<BusId, { volume: number; muted: boolean }>();
  let streamChannelFlush: Promise<void> | null = null;
  const savedStreamChannels = new Map<BusId, { volume: number; muted: boolean }>();
  function handleStreamChannel(bus: BusId, volume: number, muted: boolean) {
    if (streamSaving || !Number.isFinite(volume)) return;
    const channel = { volume: Math.max(0, Math.min(1.5, volume)), muted };
    if (!savedStreamChannels.has(bus)) savedStreamChannels.set(bus, { ...config.stream.channels[bus] });
    config.stream.channels[bus] = channel;
    if (streamMonitoring) {
      void syncEffectiveBusMutes();
    }
    if (!isTauriConnected) return;
    pendingStreamChannels.set(bus, channel);
    if (streamChannelFlush) return;
    streamChannelFlush = flushStreamChannels().finally(() => { streamChannelFlush = null; });
  }

  async function flushStreamChannels() {
    while (pendingStreamChannels.size) {
      const [bus, channel] = pendingStreamChannels.entries().next().value!;
      pendingStreamChannels.delete(bus);
      try {
        await invoke("set_stream_channel", { bus, ...channel });
        savedStreamChannels.set(bus, channel);
      } catch (err) {
        if (!pendingStreamChannels.has(bus)) config.stream.channels[bus] = { ...savedStreamChannels.get(bus)! };
        streamError = `Could not save ${bus} stream level: ${String(err)}`;
        showToast(streamError);
      }
    }
  }

  // Dragging fires many updates; run one pass at a time and re-run once so the newest values win.
  let monitorSyncRunning = false;
  let monitorSyncQueued = false;
  async function syncEffectiveBusMutes() {
    if (monitorSyncRunning) {
      monitorSyncQueued = true;
      return;
    }
    monitorSyncRunning = true;
    try {
      do {
        monitorSyncQueued = false;
        await applyEffectiveBusMutes();
      } while (monitorSyncQueued);
    } finally {
      monitorSyncRunning = false;
    }
  }

  async function applyEffectiveBusMutes() {
    const anySolo = Object.values(soloedBuses).some(Boolean);
    const stream = config.stream;
    for (const b of BUS_ORDER) {
      const bCfg = config.buses[b];
      if (!bCfg) continue;
      if (streamMonitoring) {
        // Stream playback audition: headphones follow the stream send faders, not the
        // monitor faders. Live-only, so the saved monitor mix is restored untouched.
        const inStream =
          stream.enabled &&
          !stream.muted &&
          stream.categories.includes(b) &&
          !stream.channels[b]?.muted;
        const sendLevel = (stream.channels[b]?.volume ?? 1) * stream.volume;
        if (isTauriConnected) {
          try {
            await invoke("set_bus_audition", { bus: b, volume: sendLevel, muted: !inStream });
          } catch (err) {
            console.error(`Stream playback level for ${b} failed:`, err);
          }
        }
        continue;
      }
      if (isTauriConnected) {
        try {
          await invoke("set_bus_volume_mute", {
            bus: b,
            volume: bCfg.volume,
            muted: bCfg.muted || (anySolo && !soloedBuses[b]),
            preGainDb: bCfg.preGainDb,
          });
        } catch {}
      }
    }

    // Mic monitor handling
    if (streamMonitoring) {
      const micInStream =
        stream.enabled &&
        !stream.muted &&
        stream.categories.includes("Mic") &&
        !stream.channels["Mic"]?.muted;
      const micLevel = (stream.channels["Mic"]?.volume ?? 1) * stream.volume;
      if (isTauriConnected) {
        try {
          await invoke("set_bus_audition", { bus: "Mic", volume: micLevel, muted: !micInStream });
        } catch (err) {
          console.error("Failed to audition mic stream level:", err);
        }
      }
      return;
    }
    if (isTauriConnected) {
      try {
        await invoke("set_mic_to_monitor", { enabled: config.micToMonitor });
      } catch (err) {
        console.error("Failed to set mic to monitor:", err);
      }
    }
  }

  async function handleToggleStreamMonitoring() {
    streamMonitoring = !streamMonitoring;
    if (streamMonitoring) {
      showToast("Stream playback active · Solo headphones listening");
    } else {
      showToast("Stream playback disabled · Monitor mix restored");
    }
    await syncEffectiveBusMutes();
  }

  async function handleBusVolumeMute(
    bus: BusId,
    volume: number,
    muted: boolean,
    preGainDb: number
  ) {
    const clampedVol = Math.max(0, Math.min(1.5, Math.round(volume * 1e6) / 1e6));
    const clampedPre = Math.max(-12, Math.min(12, Math.round(preGainDb * 1e6) / 1e6));
    config.buses[bus].volume = clampedVol;
    config.buses[bus].muted = muted;
    config.buses[bus].preGainDb = clampedPre;
    await syncEffectiveBusMutes();
  }

  async function handleToggleSolo(bus: BusId) {
    if (streamMonitoring) {
      streamMonitoring = false;
    }
    soloedBuses[bus] = !soloedBuses[bus];
    const anySolo = Object.values(soloedBuses).some(Boolean);
    if (anySolo) {
      showToast(`${bus.toUpperCase()} Bus Solo Active`);
    } else {
      showToast("Solo cleared across all buses");
    }
    await syncEffectiveBusMutes();
  }

  async function handleRemoveRoutingRule(exeName: string) {
    const nextRules = { ...config.processRoutingRules };
    delete nextRules[exeName];
    delete nextRules[exeName.toLowerCase()];
    config.processRoutingRules = nextRules;
    const matchingProc = processes.find(
      (p) => p.exeName.toLowerCase() === exeName.toLowerCase()
    );
    if (matchingProc && matchingProc.isRouted) {
      await handleUnrouteProcess(matchingProc.pid);
    } else {
      showToast(`Removed saved routing rule for ${exeName}`);
    }
  }

  async function handleChatMix(balance: number) {
    const clamped = Math.max(-1.0, Math.min(1.0, Math.round(balance * 1e6) / 1e6));
    config.chatMixBalance = clamped;
    try {
      await invoke("set_chat_mix", { balance: clamped });
    } catch {}
  }

  async function handleMasterVolume(volume: number) {
    const clamped = Math.max(0, Math.min(1.5, Math.round(volume * 1e6) / 1e6));
    config.masterVolume = clamped;
    try {
      await invoke("set_master_volume", { volume: clamped });
    } catch {}
  }

  async function handleEqBand(bus: BusId, bandIndex: number, band: EqBandConfig) {
    config.buses[bus].eqBands[bandIndex] = band;
    try {
      await invoke("set_eq_band", { bus, bandIndex, band });
    } catch {}
  }

  async function handleResetEqFlat(bus: BusId) {
    const before = config.buses[bus].eqBands.map((b) => ({ ...b }));
    const flatBands = createDefaultBands();
    config.buses[bus].eqBands = flatBands;
    showToast(`${bus.toUpperCase()} 10-Band EQ flattened to 0.0 dB`, {
      label: "Undo",
      run: () => void restoreEqBands(bus, before),
    });
    for (let i = 0; i < flatBands.length; i++) {
      try {
        await invoke("set_eq_band", { bus, bandIndex: i, band: flatBands[i] });
      } catch {}
    }
  }

  function getBusDefaultBands(bus: BusId): EqBandConfig[] {
    const bands = createDefaultBands();
    let defaultPresetId: string | undefined;
    switch (bus) {
      case "Game":
        defaultPresetId = "fps";
        break;
      case "Chat":
        defaultPresetId = "voice";
        break;
      case "Media":
        defaultPresetId = "cinema";
        break;
      case "Mic":
        defaultPresetId = "broadcast";
        break;
      case "Aux":
      default:
        defaultPresetId = "flat";
        break;
    }
    if (defaultPresetId) {
      const preset = getEqPresetsForBus(bus).find((p) => p.id === defaultPresetId);
      if (preset) {
        preset.gains.forEach((g, i) => {
          bands[i].gainDb = g;
        });
      }
    }
    if (bus === "Chat") {
      bands[0].type = "HighPass";
      bands[0].freq = 85.0;
    } else if (bus === "Mic") {
      bands[0].type = "HighPass";
      bands[0].freq = 80.0;
    }
    return bands;
  }

  async function handleToggleEqGlobal(bus: BusId, enabled: boolean) {
    const current = config.buses[bus].eqBands;
    const updated = current.map((b) => ({ ...b, enabled }));
    config.buses[bus].eqBands = updated;
    showToast(`${bus.toUpperCase()} 10-Band EQ ${enabled ? "enabled" : "bypassed"}`);
    for (let i = 0; i < updated.length; i++) {
      try {
        await invoke("set_eq_band", { bus, bandIndex: i, band: updated[i] });
      } catch {}
    }
  }

  async function handleResetEqDefaults(bus: BusId) {
    const before = config.buses[bus].eqBands.map((b) => ({ ...b }));
    const defaultBands = getBusDefaultBands(bus);
    config.buses[bus].eqBands = defaultBands;
    showToast(`${bus.toUpperCase()} EQ reset to default profile`, {
      label: "Undo",
      run: () => void restoreEqBands(bus, before),
    });
    for (let i = 0; i < defaultBands.length; i++) {
      try {
        await invoke("set_eq_band", { bus, bandIndex: i, band: defaultBands[i] });
      } catch {}
    }
  }

  async function handleApplyEqPreset(bus: BusId, preset: EqPreset) {
    const updatedBands = config.buses[bus].eqBands.map((b, idx) => ({
      ...b,
      enabled: true,
      gainDb: preset.gains[idx] ?? 0,
      q: preset.qs ? preset.qs[idx] : idx === 0 || idx === 9 ? 0.707 : 1.414,
    }));
    config.buses[bus].eqBands = updatedBands;
    showToast(`Applied "${preset.name}" EQ curve to ${bus.toUpperCase()}`);
    for (let i = 0; i < updatedBands.length; i++) {
      try {
        await invoke("set_eq_band", { bus, bandIndex: i, band: updatedBands[i] });
      } catch {}
    }
  }

  async function handleToggleMicToMonitor() {
    const next = !config.micToMonitor;
    config.micToMonitor = next;
    showToast(next ? "Mic playback to monitor enabled (Headphones)" : "Mic playback to monitor muted");
    await syncEffectiveBusMutes();
  }

  async function handleDynamics(bus: BusId, noiseGate: NoiseGateConfig, compressor: CompressorConfig) {
    config.buses[bus].noiseGate = noiseGate;
    config.buses[bus].compressor = compressor;
    try {
      await invoke("set_bus_dynamics", { bus, noiseGate, compressor });
    } catch {}
  }

  async function handleExtras(
    bus: BusId,
    limiter: LimiterConfig,
    deEsser: DeEsserConfig,
    noiseSuppression: NoiseSuppressionConfig
  ) {
    config.buses[bus] = { ...config.buses[bus], limiter, deEsser, noiseSuppression };
    try {
      await invoke("set_bus_extras", { bus, limiter, deEsser, noiseSuppression });
    } catch {}
  }

  async function handleProcessVolume(exeName: string, volume: number) {
    const key = exeName.toLowerCase();
    const next = { ...(config.processVolumes ?? {}) };
    if (Math.abs(volume - 1) < 0.005) delete next[key];
    else next[key] = volume;
    config.processVolumes = next;
    try {
      await invoke("set_process_volume", { exeName, volume });
    } catch {}
  }

  async function handleDucking(ducking: DuckingConfig) {
    config.ducking = ducking;
    try {
      await invoke("set_ducking", { ducking });
    } catch {}
  }

  async function handlePresetApplied(applied: RouterConfig) {
    config = applied;
    await loadRouterState();
  }

  async function handleRouteProcess(pid: number, exeName: string, bus: PlaybackBusId) {
    processes = processes.map((p) =>
      p.pid === pid ? { ...p, assignedBus: bus, isRouted: true } : p
    );
    config.processRoutingRules = {
      ...config.processRoutingRules,
      [exeName.toLowerCase()]: bus,
    };
    const label = pid === 0 ? "System Sounds" : `${exeName} (PID ${pid})`;
    showToast(`Routed ${label} → ${bus.toUpperCase()}`);

    try {
      await invoke("route_process_to_bus", { pid, exeName, bus });
      await handleRefreshProcesses();
    } catch (err) {
      if (isTauriConnected) {
        statusMessage = `Route error: ${String(err)}`;
      }
    }
  }

  async function handleUnrouteProcess(pid: number) {
    const target = processes.find((p) => p.pid === pid);
    processes = processes.map((p) =>
      p.pid === pid ? { ...p, assignedBus: "Aux", isRouted: true } : p
    );
    if (target) {
      const nextRules = { ...config.processRoutingRules };
      delete nextRules[target.exeName.toLowerCase()];
      delete nextRules[target.exeName];
      config.processRoutingRules = nextRules;
      const label = pid === 0 ? "System Sounds" : `${target.exeName} (PID ${pid})`;
      showToast(`Auto-routed ${label} → AUX`);
    }

    try {
      await invoke("unroute_process", { pid });
      await handleRefreshProcesses();
    } catch (err) {
      if (isTauriConnected) {
        statusMessage = `Unroute error: ${String(err)}`;
      }
    }
  }

  async function handleRefreshProcesses() {
    try {
      const refreshed = await invoke<ActiveAudioProcess[]>("refresh_audio_processes");
      processes = ensureDefaultAuxRouting(refreshed, config.processRoutingRules);
    } catch {}
  }

  async function handleDevices(
    masterOutputDeviceId: string | null,
    dummySinkDeviceId: string | null,
    physicalMicDeviceId: string | null,
    virtualMicOutputDeviceId: string | null,
    streamOutputDeviceId?: string | null
  ) {
    config.masterOutputDeviceId = masterOutputDeviceId;
    config.dummySinkDeviceId = dummySinkDeviceId;
    config.physicalMicDeviceId = physicalMicDeviceId;
    config.virtualMicOutputDeviceId = virtualMicOutputDeviceId;
    if (streamOutputDeviceId !== undefined) {
      config.stream.outputDeviceId = streamOutputDeviceId;
    }
    showToast("Updated Hardware I/O & Virtual Cable endpoints");
    try {
      config = await invoke<RouterConfig>("set_audio_devices", {
        masterOutputDeviceId,
        dummySinkDeviceId,
        physicalMicDeviceId,
        virtualMicOutputDeviceId,
        streamOutputDeviceId: streamOutputDeviceId !== undefined ? streamOutputDeviceId : undefined,
      });
      await handleRefreshProcesses();
    } catch (err) {
      if (isTauriConnected) {
        statusMessage = `Device switch error: ${String(err)}`;
        showToast(String(err));
      }
    }
  }

  async function handleInstallOrRepairCables(forceRepair: boolean = false) {
    if (cableInstalling) return;
    cableInstalling = true;
    statusMessage = "Installing / repairing WHQL VB-CABLE + Hi-Fi Cable drivers (approve Administrator UAC prompt)…";
    try {
      if (isTauriConnected) {
        const result = await invoke<VirtualCableSetupResult>("install_or_repair_virtual_cables", {
          installHifiCable: true,
          forceRepair,
        });
        config = result.config;
        endpoints = result.endpoints;
        if (result.cableStatus) {
          backendCableStatus = result.cableStatus;
        }
        await handleRefreshProcesses();
        const summary =
          result.actionsTaken.length > 0
            ? result.actionsTaken[result.actionsTaken.length - 1]
            : "VB-CABLE drivers verified and routes assigned";
        statusMessage = result.rebootRecommended
          ? `${summary} · Restart Windows if endpoints do not appear immediately.`
          : `Virtual cables active · ${summary}`;
        showToast(
          result.rebootRecommended
            ? "Drivers installed! Restart Windows to finalize audio endpoints."
            : "VB-CABLE & routes configured for Silent Sink, Mic, and Stream!"
        );
      } else {
        endpoints = createFallbackEndpoints();
        config.dummySinkDeviceId = "ep-render-vbcable";
        config.virtualMicOutputDeviceId = "ep-render-virtmic";
        config.stream.outputDeviceId = "ep-render-stream";
        statusMessage = "Preview: Simulated WHQL VB-CABLE + Hi-Fi Cable setup and 3-slot auto-routing";
        showToast("Simulated VB-CABLE install & auto-routed Silent Sink, Mic, and Stream");
      }
    } catch (err) {
      statusMessage = `Virtual cable setup error: ${String(err)}`;
      showToast(String(err));
    } finally {
      cableInstalling = false;
    }
  }

  async function handleAutoRouteCables() {
    if (cableAutoRouting) return;
    cableAutoRouting = true;
    try {
      if (isTauriConnected) {
        const result = await invoke<VirtualCableSetupResult>("auto_route_virtual_cables", {
          overwriteExisting: true,
        });
        config = result.config;
        endpoints = result.endpoints;
        if (result.cableStatus) {
          backendCableStatus = result.cableStatus;
        }
        await handleRefreshProcesses();
        showToast(
          result.actionsTaken.length > 0
            ? `Auto-routed ${result.actionsTaken.length} virtual cable slot(s)`
            : "Verified virtual cable routes"
        );
      } else {
        config.dummySinkDeviceId = "ep-render-vbcable";
        config.virtualMicOutputDeviceId = "ep-render-virtmic";
        config.stream.outputDeviceId = "ep-render-stream";
        showToast("Auto-routed Silent Sink, Virtual Mic, and Stream Mix");
      }
    } catch (err) {
      statusMessage = `Auto-route error: ${String(err)}`;
      showToast(String(err));
    } finally {
      cableAutoRouting = false;
    }
  }

  async function handleStreamChange(stream: StreamConfig): Promise<boolean> {
    if (streamSaving) return false;
    streamError = null;
    const conflicts = [config.masterOutputDeviceId, config.dummySinkDeviceId, config.virtualMicOutputDeviceId];
    if ((stream.enabled && !stream.outputDeviceId) || (stream.outputDeviceId && conflicts.includes(stream.outputDeviceId))) {
      streamError = "Choose a separate streaming cable, distinct from your other outputs.";
      return false;
    }
    streamSaving = true;
    try {
      await streamChannelFlush;
      stream = { ...stream, channels: structuredClone($state.snapshot(config.stream.channels)) };
      if (isTauriConnected) {
        config = await invoke<RouterConfig>("set_stream_config", { stream });
      } else {
        config.stream = { ...stream, categories: [...stream.categories], appOverrides: { ...stream.appOverrides } };
      }
      savedStreamChannels.clear();
      showToast(isTauriConnected ? "Stream mix saved" : "Preview stream mix updated");
      if (streamMonitoring) {
        void syncEffectiveBusMutes();
      }
      return true;
    } catch (err) {
      streamError = String(err);
      return false;
    } finally {
      streamSaving = false;
    }
  }

  async function refreshStreamDevices() {
    if (!isTauriConnected) return;
    try {
      const state = await invoke<FullRouterStatePayload>("get_router_state");
      endpoints = state.endpoints;
      if (state.cableStatus) {
        backendCableStatus = state.cableStatus;
      }
      streamError = null;
    } catch (err) {
      streamError = `Could not refresh devices: ${String(err)}`;
    }
  }

  type ConfirmState = {
    open: boolean;
    title: string;
    subtitle?: string;
    message: string;
    details?: string[];
    confirmLabel?: string;
    cancelLabel?: string;
    tone?: "danger" | "warning" | "info";
    icon?: string;
    accentColor?: string;
    action: () => void | Promise<void>;
  };

  let confirmState = $state<ConfirmState | null>(null);

  function promptResetAllRoutes() {
    if (!appSettings.confirmBeforeReset) {
      void handleResetAllEndpoints();
      return;
    }
    confirmState = {
      open: true,
      title: "Reset All Routed Applications?",
      subtitle: "Routing reset",
      message: "This will clear all saved routing rules and restore all active applications back to default system playback (Aux).",
      details: [
        "All custom application-to-bus routes will be cleared",
        "Active apps will revert to Windows default playback (Aux)",
        "Faders, volumes, ChatMix, and EQ settings will remain saved",
      ],
      confirmLabel: "Reset All Routes",
      cancelLabel: "Keep My Routes",
      tone: "warning",
      icon: "reset",
      accentColor: "#edc990",
      action: async () => {
        confirmState = null;
        await handleResetAllEndpoints();
      },
    };
  }

  function promptResetEqFlat(bus: BusId) {
    const isAlreadyFlat = config.buses[bus]?.eqBands?.every((b) => Math.abs(b.gainDb) < 0.05);
    if (isAlreadyFlat) {
      handleResetEqFlat(bus);
      return;
    }
    confirmState = {
      open: true,
      title: `Flatten ${bus} Equalizer?`,
      subtitle: `${bus} EQ reset`,
      message: `This will reset all 10 frequency bands for the ${bus} channel back to a completely flat (0.0 dB) response.`,
      confirmLabel: "Flatten EQ",
      cancelLabel: "Keep Settings",
      tone: "warning",
      icon: "wave",
      accentColor: BUS_COLORS[bus],
      action: async () => {
        confirmState = null;
        await handleResetEqFlat(bus);
      },
    };
  }

  function promptResetEqDefaults(bus: BusId) {
    confirmState = {
      open: true,
      title: `Reset ${bus} EQ to Default Profile?`,
      subtitle: `${bus} acoustic profile`,
      message: `This will restore the tuned factory EQ profile for ${bus}, overwriting any custom band adjustments.`,
      confirmLabel: "Reset to Defaults",
      cancelLabel: "Keep Settings",
      tone: "info",
      icon: "reset",
      accentColor: BUS_COLORS[bus],
      action: async () => {
        confirmState = null;
        await handleResetEqDefaults(bus);
      },
    };
  }

  async function handleResetAllEndpoints() {
    const rulesBefore = { ...config.processRoutingRules };
    processes = processes.map((p) => ({ ...p, assignedBus: "Aux", isRouted: true }));
    config.processRoutingRules = {};
    showToast(
      "Cleared custom rules · Auto-routed all apps to AUX",
      Object.keys(rulesBefore).length ? { label: "Undo", run: () => void restoreRoutes(rulesBefore) } : undefined
    );
    try {
      await invoke("reset_all_endpoints");
      await handleRefreshProcesses();
      statusMessage = "Cleared custom routing rules · All apps auto-routed to AUX.";
    } catch (err) {
      if (isTauriConnected) {
        statusMessage = `Reset error: ${String(err)}`;
      }
    }
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement | null)?.tagName;
    if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA" || (e.target as HTMLElement)?.getAttribute("role") === "combobox") return;

    if (e.key >= "1" && e.key <= "5") {
      const idx = parseInt(e.key, 10) - 1;
      if (BUS_ORDER[idx]) {
        selectedBus = BUS_ORDER[idx];
      }
    } else if (e.key.toLowerCase() === "m") {
      const b = config.buses[selectedBus];
      if (b) {
        void handleBusVolumeMute(selectedBus, b.volume, !b.muted, b.preGainDb);
        showToast(`${selectedBus.toUpperCase()} ${!b.muted ? "Muted" : "Unmuted"}`);
      }
    } else if (e.key.toLowerCase() === "s") {
      handleToggleSolo(selectedBus);
    } else if (e.key.toLowerCase() === "i") {
      openSettings("audio");
    } else if (e.key === ",") {
      openSettings();
    }
  }

  // Events from the Rust side: device hot-plug and changes made from the tray menu.
  onMount(() => {
    if (!isNativeApp()) return;
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    const track = (pending: Promise<() => void>) =>
      void pending.then((off) => (disposed ? off() : unlisteners.push(off))).catch(() => {});

    track(
      listen<{ added: string[]; removed: string[]; reconnected: boolean }>("devices-changed", (event) => {
        const { added, removed, reconnected } = event.payload;
        const parts: string[] = [];
        if (removed.length) parts.push(`Unplugged: ${removed.join(", ")}`);
        if (added.length) parts.push(`Connected: ${added.join(", ")}`);
        if (reconnected) parts.push("routing restored");
        if (parts.length) showToast(parts.join(" · "));
        void loadRouterState();
      })
    );
    track(
      listen<{ id: string; name: string }>("scene-applied", (event) => {
        showToast(`Scene “${event.payload.name}” applied automatically`);
        void loadRouterState();
        void loadPresets();
      })
    );
    track(
      listen("config-changed", () => {
        void loadRouterState();
        showToast("Channel mutes updated from the tray");
      })
    );
    return () => {
      disposed = true;
      unlisteners.forEach((off) => off());
    };
  });

  let backgroundSaver: BackgroundSaver | null = null;
  $effect(() => {
    // Read first: with the saver still null the optional call would skip reading it, and the
    // effect would never re-run when the setting loads or changes.
    const enabled = appSettings.saveResourcesInBackground;
    backgroundSaver?.setEnabled(enabled);
  });

  onMount(() => {
    void loadAppSettings().then(() => {
      appSettingsLoaded = true;
      if (isNativeApp() && !appSettings.onboardingCompleted) wizardOpen = true;
    });
    void loadRouterState();
    void loadPresets();
    void connectMeterStream();
    backgroundSaver = installBackgroundSaver();
    backgroundSaver.setEnabled(appSettings.saveResourcesInBackground);

    const procTimer = setInterval(() => {
      if (isTauriConnected && !document.hidden) {
        void handleRefreshProcesses();
      }
    }, 4000);

    let phase = 0;
    const simTimer = setInterval(() => {
      if (!simulateTelemetry || isTauriConnected || document.hidden) return;
      phase += 0.22;
      const anySolo = Object.values(soloedBuses).some(Boolean);
      const makeWave = (base: number, freq: number, bus: BusId) => {
        const busCfg = config.buses[bus];
        let isMuted = false;
        if (streamMonitoring) {
          const inStream =
            config.stream.enabled &&
            !config.stream.muted &&
            config.stream.categories.includes(bus) &&
            !config.stream.channels[bus]?.muted;
          isMuted = !inStream;
        } else {
          isMuted = !busCfg || busCfg.muted || (anySolo && !soloedBuses[bus]) || busCfg.volume <= 0.01;
        }
        if (isMuted) {
          return { peakLeft: 0, peakRight: 0, rmsLeft: 0, rmsRight: 0 };
        }
        const fader = streamMonitoring ? (config.stream.channels[bus]?.volume ?? 1) * config.stream.volume : busCfg.volume;
        const scale = Math.min(1.35, fader * Math.pow(10, busCfg.preGainDb / 20));
        const modL = Math.abs(Math.sin(phase * freq) * 0.22 + Math.cos(phase * 1.7) * 0.1);
        const modR = Math.abs(Math.cos(phase * freq * 0.95) * 0.22 + Math.sin(phase * 1.3) * 0.1);
        const pL = Math.min(1.15, (base + modL) * scale);
        const pR = Math.min(1.15, (base + modR) * scale);
        return {
          peakLeft: pL,
          peakRight: pR,
          rmsLeft: pL * 0.68,
          rmsRight: pR * 0.68,
        };
      };

      const g = makeWave(0.52, 1.4, "Game");
      const c = makeWave(0.38, 0.9, "Chat");
      const m = makeWave(0.48, 1.1, "Media");
      const a = makeWave(0.18, 0.6, "Aux");
      const mc = makeWave(0.42, 1.25, "Mic");

      const masterScale = config.masterVolume;
      const masterPeakL = Math.min(
        0.99,
        Math.max(g.peakLeft, c.peakLeft, m.peakLeft, a.peakLeft) * masterScale
      );
      const masterPeakR = Math.min(
        0.99,
        Math.max(g.peakRight, c.peakRight, m.peakRight, a.peakRight) * masterScale
      );

      meters = {
        stream: (() => {
          const stream = config.stream;
          if (!stream.enabled || !stream.outputDeviceId || stream.muted) return zeroMeter;
          const count = processes.filter(
            (p) =>
              stream.appOverrides[p.exeName.toLowerCase()] ??
              (p.assignedBus !== null && stream.categories.includes(p.assignedBus))
          ).reduce((sum, p) => {
            const channel = stream.channels[p.assignedBus ?? "Aux"];
            return sum + (channel.muted ? 0 : channel.volume);
          }, 0);
          const micSend = stream.channels.Mic;
          const mic = stream.categories.includes("Mic") && !micSend.muted ? mc.peakLeft * micSend.volume : 0;
          const peak = Math.min(
            0.98,
            (count * (0.08 + Math.abs(Math.sin(phase)) * 0.05) + mic * 0.5) * stream.volume
          );
          return { peakLeft: peak, peakRight: peak * 0.95, rmsLeft: peak * 0.7, rmsRight: peak * 0.66 };
        })(),
        streamSends: (() => {
          const stream = config.stream;
          const send = (snap: { peakLeft: number; peakRight: number; rmsLeft: number; rmsRight: number }, bus: BusId) => {
            const channel = stream.channels?.[bus];
            const on = stream.enabled && !stream.muted && !channel?.muted && stream.categories.includes(bus);
            const k = on ? (channel?.volume ?? 1) * stream.volume : 0;
            return { peakLeft: snap.peakLeft * k, peakRight: snap.peakRight * k, rmsLeft: snap.rmsLeft * k, rmsRight: snap.rmsRight * k };
          };
          return { game: send(g, "Game"), chat: send(c, "Chat"), media: send(m, "Media"), aux: send(a, "Aux"), mic: send(mc, "Mic") };
        })(),
        streamRunning: false,
        streamError: null,
        game: g,
        chat: c,
        media: m,
        aux: a,
        mic: mc,
        master: {
          peakLeft: masterPeakL,
          peakRight: masterPeakR,
          rmsLeft: masterPeakL * 0.72,
          rmsRight: masterPeakR * 0.72,
        },
      };
      diagnostics.callbackCount += 6;
    }, 65);

    if (isTauriConnected) {
      void checkMaximized();
      void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
        const win = getCurrentWindow();
        void win.isMaximized().then((m) => (isMaximized = m));
        win.onResized(() => {
          void win.isMaximized().then((m) => (isMaximized = m));
        });
      });
    }

    return () => {
      clearInterval(procTimer);
      clearInterval(simTimer);
      backgroundSaver?.dispose();
    };
  });

  let isMaximized = $state(false);

  async function checkMaximized() {
    if (!isTauriConnected) return;
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      isMaximized = await getCurrentWindow().isMaximized();
    } catch {
      try {
        isMaximized = await invoke<boolean>("window_is_maximized");
      } catch {}
    }
  }

  async function handleMinimize() {
    if (!isTauriConnected) {
      showToast("Window minimize is available in the desktop app");
      return;
    }
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().minimize();
    } catch {
      try {
        await invoke("window_minimize");
      } catch (err) {
        console.error("Minimize error:", err);
      }
    }
  }

  async function handleToggleMaximize() {
    if (!isTauriConnected) {
      showToast("Window maximize is available in the desktop app");
      return;
    }
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().toggleMaximize();
      await checkMaximized();
    } catch {
      try {
        isMaximized = await invoke<boolean>("window_toggle_maximize");
      } catch (err) {
        console.error("Toggle maximize error:", err);
      }
    }
  }

  async function handleClose() {
    if (!isTauriConnected) {
      showToast("Window close is available in the desktop app");
      return;
    }
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().close();
    } catch {
      try {
        await invoke("window_close");
      } catch (err) {
        console.error("Close error:", err);
      }
    }
  }

  function handleHeaderPointerDown(e: PointerEvent) {
    if (e.button !== 0 || e.detail > 1) return; // double-click toggles maximize instead
    if ((e.target as HTMLElement)?.closest("button, a, input, select, textarea, label, [role='menu'], [role='switch']")) return;
    if (!isTauriConnected) return;
    // Must be called synchronously inside the press: Windows ignores a drag started after an await.
    try {
      void getCurrentWindow().startDragging().catch(() => {});
    } catch {}
  }

  function scrollToTop(e?: MouseEvent) {
    if (e) e.preventDefault();
    window.scrollTo({ top: 0, left: 0, behavior: "smooth" });
    document.documentElement.scrollTo({ top: 0, left: 0, behavior: "smooth" });
    document.body.scrollTo({ top: 0, left: 0, behavior: "smooth" });
    const workspace = document.querySelector(".workspace");
    if (workspace) workspace.scrollTo({ top: 0, left: 0, behavior: "smooth" });
    if (window.location.hash) {
      history.replaceState(null, "", window.location.pathname + window.location.search);
    }
  }

  function handleHeaderDblClick(e: MouseEvent) {
    if ((e.target as HTMLElement)?.closest("button, a, input, select")) return;
    void handleToggleMaximize();
  }
</script>

<svelte:window onkeydown={handleGlobalKeydown} />
<StudioTooltips/>

<div class="studio live-studio">
  <Rail
    connected={isTauriConnected}
    engineRunning={diagnostics.masterRunning}
    streamEnabled={config.stream.enabled}
    streamRunning={meters.streamRunning}
    appCount={processes.length}
    {settingsOpen}
    onOpenSettings={(tab) => openSettings(tab as typeof settingsTab | undefined)}
    onScrollTop={scrollToTop}
  />
  <main class="workspace">
    <header
      id="top"
      class="console-header"
      role="region"
      aria-label="Console header"
      onpointerdown={handleHeaderPointerDown}
      ondblclick={handleHeaderDblClick}
    >
      <a href="#top" class="wordmark" data-tauri-drag-region="false" onclick={scrollToTop}>stellars</a>
      <div class="header-center">
        <span class="status-dot" class:standby={!isTauriConnected||!diagnostics.masterRunning}></span>
        {isTauriConnected?(diagnostics.masterRunning?'Audio engine connected':'Engine standby'):'Browser preview'}
        <span class="header-separator"></span>
        <span>48 kHz / Stereo</span>
        <span class="header-separator"></span>
        <span>{cableStatus.vbCableInstalled?'VB-CABLE ready':'Virtual cable setup available'}</span>
      </div>
      <div class="header-actions">
        <button
          type="button"
          class="theme-toggle-switch"
          class:is-light={theme === "light"}
          role="switch"
          aria-checked={theme === "light"}
          aria-label={theme === "dark" ? "Switch to light theme (Alt+T)" : "Switch to dark theme (Alt+T)"}
          data-tooltip={theme === "dark" ? "Switch to light theme (Alt+T)" : "Switch to dark theme (Alt+T)"}
          onclick={toggleTheme}
        >
          <span class="theme-switch-track">
            <span class="theme-switch-icon sun" aria-hidden="true">
              <StudioIcon name="sun" size={12}/>
            </span>
            <span class="theme-switch-icon moon" aria-hidden="true">
              <StudioIcon name="moon" size={12}/>
            </span>
            <span class="theme-switch-thumb" aria-hidden="true">
              <span class="thumb-icon sun-icon">
                <StudioIcon name="sun" size={12}/>
              </span>
              <span class="thumb-icon moon-icon">
                <StudioIcon name="moon" size={12}/>
              </span>
            </span>
          </span>
        </button>
        <PresetsMenu native={isTauriConnected} onApplied={handlePresetApplied} onNotify={showToast} onManage={() => openSettings("presets")} />
        <button class="header-reset-button" aria-label="Reset routes" data-tooltip="Reset all routed apps to system playback and clear saved routing rules." onclick={promptResetAllRoutes}>
          <StudioIcon name="reset" size={15}/>
        </button>
        <div class="window-controls" role="group" aria-label="Window controls">
          <button
            type="button"
            class="win-btn win-minimize"
            aria-label="Minimize"
            data-tooltip="Minimize"
            onclick={handleMinimize}
          >
            <svg width="10" height="2" viewBox="0 0 10 2" fill="currentColor" aria-hidden="true">
              <rect width="10" height="1.5" rx="0.75" />
            </svg>
          </button>
          <button
            type="button"
            class="win-btn win-maximize"
            aria-label={isMaximized ? "Restore" : "Maximize"}
            data-tooltip={isMaximized ? "Restore" : "Maximize"}
            onclick={handleToggleMaximize}
          >
            {#if isMaximized}
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.1" aria-hidden="true">
                <path d="M3 4.5V2h5v5H5.5m-3 0v4h5v-4z" />
              </svg>
            {:else}
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.1" aria-hidden="true">
                <rect x="1" y="1" width="8" height="8" rx="1" />
              </svg>
            {/if}
          </button>
          <button
            type="button"
            class="win-btn win-close"
            aria-label="Close"
            data-tooltip="Close"
            onclick={handleClose}
          >
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" aria-hidden="true">
              <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" />
            </svg>
          </button>
        </div>
      </div>
    </header>
    {#if !isTauriConnected}<div class="page-heading"><div class="heading-actions"><span class="preview-label">Interactive preview</span><button class="quiet-button" aria-pressed={simulateTelemetry} onclick={()=>{simulateTelemetry=!simulateTelemetry;showToast(simulateTelemetry?'Preview meters playing':'Preview meters paused');}}><StudioIcon name={simulateTelemetry?'pause':'play'} size={13}/>{simulateTelemetry?'Pause preview':'Play preview'}</button></div></div>{/if}
    <div class="engine-status" role="status"><span>{statusMessage}</span><span>Mic {isTauriConnected?(diagnostics.micRunning?'active':'standby'):'demo'} · Underruns {diagnostics.underrunCount}</span></div>
  <MixerView
    native={isTauriConnected}
    {config}
    {endpoints}
    {processes}
    {meters}
    {selectedBus}
    {soloedBuses}
    {streamMonitoring}
    onSelectBus={(b) => (selectedBus = b)}
    onToggleSolo={handleToggleSolo}
    onChangeBusVolumeMute={handleBusVolumeMute}
    onToggleMicToMonitor={handleToggleMicToMonitor}
    onChangeStreamChannel={handleStreamChannel}
    {streamSaving}
    onChangeChatMix={handleChatMix}
    onChangeMasterVolume={handleMasterVolume}
    onChangeEqBand={handleEqBand}
    onResetEqFlat={promptResetEqFlat}
    onResetEqDefaults={promptResetEqDefaults}
    onToggleEqGlobal={handleToggleEqGlobal}
    onApplyEqPreset={handleApplyEqPreset}
    onChangeDynamics={handleDynamics}
    onRouteProcess={handleRouteProcess}
    onUnrouteProcess={handleUnrouteProcess}
    onRemoveRoutingRule={handleRemoveRoutingRule}
    onRefreshProcesses={handleRefreshProcesses}
    onNotify={showToast}
    onChangeExtras={handleExtras}
    onChangeProcessVolume={handleProcessVolume}
  >
    {#snippet streamOutput()}
      <StreamPanel stream={config.stream} {endpoints} {processes}
        meter={meters.stream} running={meters.streamRunning} native={isTauriConnected}
        saving={streamSaving} error={streamError ?? meters.streamError}
        reservedOutputs={[config.masterOutputDeviceId, config.dummySinkDeviceId, config.virtualMicOutputDeviceId]}
        cableInstalling={cableInstalling}
        {streamMonitoring}
        onToggleStreamMonitoring={handleToggleStreamMonitoring}
        onInstallOrRepairCables={() => handleInstallOrRepairCables(false)}
        onSave={handleStreamChange} onRefreshDevices={refreshStreamDevices} />
    {/snippet}
  </MixerView>
    <footer class="workspace-footer"><button type="button" class="to-top" aria-label="Scroll to top" data-tooltip="Back to top" onclick={scrollToTop}><StudioIcon name="chevron" size={13}/></button><span>{isTauriConnected?'Connected to the desktop audio engine':'Browser preview · Open the desktop app to route audio'}</span></footer>
  </main>
</div>
<SettingsModal
  open={settingsOpen}
  bind:tab={settingsTab}
  native={isTauriConnected}
  {theme}
  onSetTheme={setTheme}
  onClose={() => (settingsOpen = false)}
  onNotify={showToast}
  onRestored={() => void loadRouterState()}
  ducking={config.ducking ?? DEFAULT_DUCKING}
  duckGain={meters.duckGain ?? 1}
  onChangeDucking={handleDucking}
  onPresetApplied={handlePresetApplied}
  onRunWizard={() => {
    settingsOpen = false;
    wizardOpen = true;
  }}
>
  {#snippet audio()}
    {@render hardware(settingsOpen && settingsTab === "audio")}
  {/snippet}
</SettingsModal>
<SetupWizard open={wizardOpen} native={isTauriConnected} onFinish={() => (wizardOpen = false)}>
  {#snippet devices()}
    {@render hardware(wizardOpen)}
  {/snippet}
</SetupWizard>
<ModePrompt
  open={appSettingsLoaded && !appSettings.interfaceModeChosen && !wizardOpen && !settingsOpen && (appSettings.onboardingCompleted || !isNativeApp())}
  onchoose={(mode) => {
    void updateAppSettings({ interfaceMode: mode, interfaceModeChosen: true });
    showToast(mode === "simple" ? "Simple mode on" : "Advanced mode on");
  }}
  ondismiss={() => void updateAppSettings({ interfaceModeChosen: true })}
/>
{#snippet hardware(active: boolean)}
  <HardwareSettings
    native={isTauriConnected}
    {active}
    {config}
    {endpoints}
    {cableStatus}
    {cableInstalling}
    {cableAutoRouting}
    onChangeDevices={handleDevices}
    onInstallOrRepairCables={handleInstallOrRepairCables}
    onAutoRouteCables={handleAutoRouteCables}
    onRefreshDevices={refreshStreamDevices}
    onNotify={showToast}
  />
{/snippet}
{#if toastText}<div class="toast" role="status" aria-live="polite"><StudioIcon name="check" size={17}/>{toastText}{#if toastAction}<button type="button" class="toast-action" onclick={runToastAction}>{toastAction.label}</button>{/if}</div>{/if}
{#if confirmState}
  <ConfirmModal
    open={confirmState.open}
    title={confirmState.title}
    subtitle={confirmState.subtitle}
    message={confirmState.message}
    details={confirmState.details}
    confirmLabel={confirmState.confirmLabel}
    cancelLabel={confirmState.cancelLabel}
    tone={confirmState.tone}
    icon={confirmState.icon}
    accentColor={confirmState.accentColor}
    onConfirm={confirmState.action}
    onCancel={() => (confirmState = null)}
  />
{/if}


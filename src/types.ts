export type BusId = "Game" | "Chat" | "Media" | "Aux" | "Mic";
export type PlaybackBusId = "Game" | "Chat" | "Media" | "Aux";
export type FilterType = "Peaking" | "LowShelf" | "HighShelf" | "LowPass" | "HighPass";

export const BUS_COLORS: Record<BusId, string> = {
  Game: "#b7a3ff",
  Chat: "#7be1d1",
  Media: "#a1baff",
  Aux: "#edc990",
  Mic: "#f0a7c6",
};

export interface EqBandConfig {
  enabled: boolean;
  type: FilterType;
  freq: number;
  gainDb: number;
  q: number;
}

export interface CompressorConfig {
  enabled: boolean;
  thresholdDb: number;
  ratio: number;
  attackMs: number;
  releaseMs: number;
  makeupGainDb: number;
  kneeDb: number;
}

export interface NoiseGateConfig {
  enabled: boolean;
  thresholdDb: number;
  ratio: number;
  attackMs: number;
  releaseMs: number;
}

export interface LimiterConfig {
  enabled: boolean;
  ceilingDb: number;
  releaseMs: number;
}

export interface DeEsserConfig {
  enabled: boolean;
  frequency: number;
  thresholdDb: number;
  maxReductionDb: number;
}

export interface NoiseSuppressionConfig {
  enabled: boolean;
  /** 0 = dry, 1 = fully cleaned. */
  strength: number;
  /** `fast` = RNNoise (light), `high` = DeepFilterNet3 (cleaner, more CPU and delay). */
  engine?: "fast" | "high";
}

export interface DuckingConfig {
  enabled: boolean;
  triggers: BusId[];
  targets: BusId[];
  amountDb: number;
  thresholdDb: number;
  attackMs: number;
  releaseMs: number;
}

export const DEFAULT_LIMITER: LimiterConfig = { enabled: false, ceilingDb: -1, releaseMs: 80 };
export const DEFAULT_DE_ESSER: DeEsserConfig = { enabled: false, frequency: 6000, thresholdDb: -30, maxReductionDb: 6 };
export const DEFAULT_NOISE_SUPPRESSION: NoiseSuppressionConfig = { enabled: false, strength: 1, engine: "fast" };
export const DEFAULT_DUCKING: DuckingConfig = {
  enabled: false,
  triggers: ["Chat"],
  targets: ["Game", "Media"],
  amountDb: -12,
  thresholdDb: -42,
  attackMs: 30,
  releaseMs: 600,
};

export interface BusConfig {
  /** Optional processors; absent in configs saved before they existed. */
  limiter?: LimiterConfig;
  deEsser?: DeEsserConfig;
  noiseSuppression?: NoiseSuppressionConfig;
  preGainDb: number;
  volume: number;
  muted: boolean;
  noiseGate: NoiseGateConfig;
  compressor: CompressorConfig;
  eqBands: EqBandConfig[];
}

export interface RouterConfig {
  stream: StreamConfig;
  masterOutputDeviceId: string | null;
  dummySinkDeviceId: string | null;
  physicalMicDeviceId: string | null;
  virtualMicOutputDeviceId: string | null;
  chatMixBalance: number;
  masterVolume: number;
  micToMonitor: boolean;
  processRoutingRules: Record<string, BusId>;
  /** Per-app volume (0–2) keyed by lower-case exe name; missing = unity. */
  processVolumes?: Record<string, number>;
  ducking?: DuckingConfig;
  buses: Record<BusId, BusConfig>;
}

export interface StreamChannelConfig {
  volume: number;
  muted: boolean;
}

export interface StreamConfig {
  enabled: boolean;
  outputDeviceId: string | null;
  volume: number;
  muted: boolean;
  categories: BusId[];
  appOverrides: Record<string, boolean>;
  channels: Record<BusId, StreamChannelConfig>;
  /** Where the mic is tapped for the stream mix; absent means "virtual". */
  micSource?: "virtual" | "raw";
}

export interface AudioEndpointInfo {
  id: string;
  name: string;
  flow: "Render" | "Capture";
  isDefault: boolean;
  isDummySinkCandidate: boolean;
  isVirtualMicCandidate: boolean;
}

export interface ActiveAudioProcess {
  pid: number;
  exeName: string;
  exePath: string;
  displayName: string;
  assignedBus: BusId | null;
  isRouted: boolean;
}

export interface DiagnosticsSnapshot {
  masterRunning: boolean;
  micRunning: boolean;
  callbackCount: number;
  underrunCount: number;
}

export interface VirtualCableEndpointPair {
  renderId: string;
  renderName: string;
  pairedCaptureId: string | null;
  pairedCaptureName: string | null;
  driverKind: string;
  groupKey: string;
  isWatermarkFree: boolean;
}

export interface RouteSlotStatus {
  slot: "silentSink" | "virtualMic" | "streamMix" | string;
  deviceId: string | null;
  deviceName: string | null;
  pairedCaptureName: string | null;
  status: "ready" | "unassigned" | "missing" | "conflict";
  hint: string;
}

export interface VirtualCableStatusPayload {
  vbCableInstalled: boolean;
  hifiCableInstalled: boolean;
  steamAudioAvailable: boolean;
  detectedCables: VirtualCableEndpointPair[];
  silentSink: RouteSlotStatus;
  virtualMic: RouteSlotStatus;
  streamMix: RouteSlotStatus;
  allRoutesHealthy: boolean;
}

export interface VirtualCableSetupResult {
  config: RouterConfig;
  endpoints: AudioEndpointInfo[];
  cableStatus: VirtualCableStatusPayload;
  actionsTaken: string[];
  rebootRecommended: boolean;
}

export interface FullRouterStatePayload {
  config: RouterConfig;
  endpoints: AudioEndpointInfo[];
  processes: ActiveAudioProcess[];
  diagnostics: DiagnosticsSnapshot;
  cableStatus?: VirtualCableStatusPayload;
}

export interface StereoMeterSnapshot {
  peakLeft: number;
  peakRight: number;
  rmsLeft: number;
  rmsRight: number;
  /** Momentary loudness in LUFS (-100 = silence). */
  lufs?: number;
  /** Live gain reduction (dB, >= 0) per processor. */
  reduction?: ProcessingReduction;
}

export interface ProcessingReduction {
  gateDb: number;
  compDb: number;
  deEsserDb: number;
  limiterDb: number;
}

export interface BusMeterSet {
  game: StereoMeterSnapshot;
  chat: StereoMeterSnapshot;
  media: StereoMeterSnapshot;
  aux: StereoMeterSnapshot;
  mic: StereoMeterSnapshot;
}

export interface AllBusMetersPayload {
  /** Level each channel contributes to the stream mix. */
  streamSends?: BusMeterSet;
  /** Current auto-ducking gain (1 = not ducking). */
  duckGain?: number;
  stream: StereoMeterSnapshot;
  streamRunning: boolean;
  streamError: string | null;
  game: StereoMeterSnapshot;
  chat: StereoMeterSnapshot;
  media: StereoMeterSnapshot;
  aux: StereoMeterSnapshot;
  mic: StereoMeterSnapshot;
  master: StereoMeterSnapshot;
}

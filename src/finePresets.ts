import type {
  BusConfig,
  BusId,
  CompressorConfig,
  DeEsserConfig,
  LimiterConfig,
  NoiseGateConfig,
  NoiseSuppressionConfig,
} from "./types";
import { DEFAULT_DE_ESSER, DEFAULT_LIMITER, DEFAULT_NOISE_SUPPRESSION } from "./types";

/** The five Fine-tune sections that can have presets. */
export type SectionId = "noise" | "gate" | "comp" | "deEsser" | "limiter";
export const SECTION_IDS: SectionId[] = ["noise", "gate", "comp", "deEsser", "limiter"];

export type SectionValues = Record<string, number | boolean | string>;
export type SectionBundle = Partial<Record<SectionId, SectionValues>>;

export interface SectionPreset {
  id: string;
  name: string;
  /** One plain-language sentence shown under the chips. */
  description: string;
  /** Channels this preset suits; absent means every channel that has the section. */
  buses?: BusId[];
  values: SectionValues;
}

export interface Recipe {
  id: string;
  name: string;
  description: string;
  buses: BusId[];
  /** Section preset id per section; "off" turns the section off. Unlisted sections are turned off. */
  sections: Partial<Record<SectionId, string>>;
  /** Optional EQ preset id (from the EQ presets of the channel). */
  eq?: string;
  /** Bypass the channel EQ (used by the "Plain" setups). */
  eqOff?: boolean;
}

const VOICE: BusId[] = ["Mic", "Chat"];
const PLAYBACK: BusId[] = ["Game", "Media", "Aux"];

/** Which channels have which section (noise suppression is voice-only). */
export function sectionAvailable(section: SectionId, bus: BusId): boolean {
  return section !== "noise" || bus === "Mic" || bus === "Chat";
}

export const OFF_PRESET_ID = "off";

export const SECTION_TITLES: Record<SectionId, string> = {
  noise: "Noise suppression",
  gate: "Noise gate",
  comp: "Compressor",
  deEsser: "De-esser",
  limiter: "Limiter",
};

export const SECTION_PRESETS: Record<SectionId, SectionPreset[]> = {
  noise: [
    { id: "noise_light", name: "Light", description: "Gently trims background hiss. Keeps your voice natural.", buses: VOICE, values: { enabled: true, strength: 0.5 } },
    { id: "noise_normal", name: "Normal", description: "Removes fans, keyboards and room noise. Good for most rooms.", buses: VOICE, values: { enabled: true, strength: 0.8 } },
    { id: "noise_strong", name: "Strong", description: "Maximum cleanup for noisy rooms. Can sound slightly processed.", buses: VOICE, values: { enabled: true, strength: 1 } },
  ],
  gate: [
    { id: "gate_gentle", name: "Gentle", description: "Quiets the pauses between words without cutting anything off.", buses: VOICE, values: { enabled: true, thresholdDb: -52, ratio: 2, attackMs: 5, releaseMs: 200 } },
    { id: "gate_normal", name: "Normal", description: "Mutes background noise while you are not talking.", buses: VOICE, values: { enabled: true, thresholdDb: -44, ratio: 4, attackMs: 3, releaseMs: 150 } },
    { id: "gate_strong", name: "Strong", description: "Cuts everything below your voice. Best with a noisy room.", buses: VOICE, values: { enabled: true, thresholdDb: -18, ratio: 8, attackMs: 2, releaseMs: 120 } },
  ],
  comp: [
    { id: "comp_voice_gentle", name: "Gentle", description: "Evens out your volume a little. Sounds natural.", buses: VOICE, values: { enabled: true, thresholdDb: -18, ratio: 2, attackMs: 15, releaseMs: 150, makeupGainDb: 2, kneeDb: 6 } },
    { id: "comp_voice_balanced", name: "Broadcast", description: "Steady, radio-style voice. Quiet and loud words sit closer together.", buses: VOICE, values: { enabled: true, thresholdDb: -20, ratio: 3.5, attackMs: 8, releaseMs: 120, makeupGainDb: 4, kneeDb: 6 } },
    { id: "comp_voice_tamer", name: "Loud-voice tamer", description: "Holds back shouts and laughs so nobody gets a fright.", buses: VOICE, values: { enabled: true, thresholdDb: -24, ratio: 6, attackMs: 4, releaseMs: 100, makeupGainDb: 6, kneeDb: 4 } },
    { id: "comp_game_even", name: "Even out loud moments", description: "Softens explosions and gunfire so footsteps stay audible.", buses: ["Game"], values: { enabled: true, thresholdDb: -16, ratio: 3, attackMs: 10, releaseMs: 200, makeupGainDb: 2, kneeDb: 8 } },
    { id: "comp_smooth", name: "Smooth", description: "Keeps volume steady between quiet and loud scenes.", buses: ["Media", "Aux"], values: { enabled: true, thresholdDb: -18, ratio: 2.5, attackMs: 15, releaseMs: 250, makeupGainDb: 2, kneeDb: 8 } },
    { id: "comp_night", name: "Night mode", description: "Makes loud parts much quieter so you will not wake anyone.", buses: PLAYBACK, values: { enabled: true, thresholdDb: -28, ratio: 6, attackMs: 5, releaseMs: 250, makeupGainDb: 6, kneeDb: 6 } },
  ],
  deEsser: [
    { id: "deess_light", name: "Light", description: "Takes the edge off sharp \u201cs\u201d sounds.", buses: ["Mic", "Chat", "Media"], values: { enabled: true, frequency: 6500, thresholdDb: -26, maxReductionDb: 4 } },
    { id: "deess_normal", name: "Normal", description: "Softens harsh \u201cs\u201d and \u201ct\u201d sounds. Good for most voices.", buses: ["Mic", "Chat", "Media"], values: { enabled: true, frequency: 6000, thresholdDb: -30, maxReductionDb: 6 } },
    { id: "deess_strong", name: "Strong", description: "For very sharp voices or bright microphones.", buses: ["Mic", "Chat", "Media"], values: { enabled: true, frequency: 5500, thresholdDb: -36, maxReductionDb: 9 } },
  ],
  limiter: [
    { id: "limiter_safe", name: "Safety", description: "Stops sudden peaks from distorting. You will not hear it working.", values: { enabled: true, ceilingDb: -1, releaseMs: 80 } },
    { id: "limiter_strict", name: "Strict", description: "Keeps this channel well below clipping, with extra headroom.", values: { enabled: true, ceilingDb: -3, releaseMs: 60 } },
  ],
};

export const RECIPES: Recipe[] = [
  { id: "mic_stream", name: "Streaming voice", description: "Clean, steady and polished. A good starting point for most people.", buses: ["Mic"], sections: { noise: "noise_normal", gate: "gate_normal", comp: "comp_voice_balanced", deEsser: "deess_normal", limiter: "limiter_safe" }, eq: "mic_clarity" },
  { id: "mic_quiet", name: "Quiet room, light touch", description: "Barely processed. Best when your room and microphone are already good.", buses: ["Mic"], sections: { noise: "noise_light", gate: "gate_gentle", comp: "comp_voice_gentle", limiter: "limiter_safe" } },
  { id: "mic_noisy", name: "Noisy room", description: "Strong cleanup for fans, keyboards and street noise.", buses: ["Mic"], sections: { noise: "noise_strong", gate: "gate_strong", comp: "comp_voice_balanced", deEsser: "deess_normal", limiter: "limiter_strict" } },
  { id: "mic_plain", name: "Plain (no effects)", description: "", buses: ["Mic"], sections: {}, eqOff: true },
  { id: "chat_clear", name: "Clear voice chat", description: "Makes friends easier to hear without hurting your ears.", buses: ["Chat"], sections: { noise: "noise_normal", comp: "comp_voice_gentle", limiter: "limiter_safe" } },
  { id: "chat_tamed", name: "Loud friends tamed", description: "Keeps shouting and laughing under control.", buses: ["Chat"], sections: { noise: "noise_light", comp: "comp_voice_tamer", limiter: "limiter_strict" } },
  { id: "chat_plain", name: "Plain (no effects)", description: "", buses: ["Chat"], sections: {}, eqOff: true },
  { id: "game_footsteps", name: "Hear footsteps", description: "Evens out explosions and boosts the sounds that matter in shooters.", buses: ["Game"], sections: { comp: "comp_game_even", limiter: "limiter_safe" }, eq: "fps" },
  { id: "game_night", name: "Night mode", description: "Loud games, quiet house.", buses: ["Game"], sections: { comp: "comp_night", limiter: "limiter_strict" } },
  { id: "game_plain", name: "Plain (no effects)", description: "", buses: ["Game"], sections: {}, eqOff: true },
  { id: "media_smooth", name: "Smooth listening", description: "Steady volume for music, videos and streams.", buses: ["Media", "Aux"], sections: { comp: "comp_smooth", limiter: "limiter_safe" } },
  { id: "media_night", name: "Night mode", description: "Loud parts much quieter.", buses: ["Media", "Aux"], sections: { comp: "comp_night", limiter: "limiter_strict" } },
  { id: "media_plain", name: "Plain (no effects)", description: "", buses: ["Media", "Aux"], sections: {}, eqOff: true },
];

export const sectionPresetsFor = (section: SectionId, bus: BusId): SectionPreset[] =>
  SECTION_PRESETS[section].filter((p) => !p.buses || p.buses.includes(bus));

export const recipesFor = (bus: BusId): Recipe[] => RECIPES.filter((r) => r.buses.includes(bus));

/** Current values of one section, as a flat record (always includes `enabled`). */
export function readSection(section: SectionId, cfg: BusConfig): SectionValues {
  switch (section) {
    case "gate": return { ...(cfg.noiseGate as NoiseGateConfig) };
    case "comp": return { ...(cfg.compressor as CompressorConfig) };
    case "limiter": return { ...((cfg.limiter ?? DEFAULT_LIMITER) as LimiterConfig) };
    case "deEsser": return { ...((cfg.deEsser ?? DEFAULT_DE_ESSER) as DeEsserConfig) };
    case "noise": return { ...((cfg.noiseSuppression ?? DEFAULT_NOISE_SUPPRESSION) as NoiseSuppressionConfig) };
  }
}

export function readAllSections(cfg: BusConfig, bus: BusId): SectionBundle {
  const out: SectionBundle = {};
  for (const id of SECTION_IDS) if (sectionAvailable(id, bus)) out[id] = readSection(id, cfg);
  return out;
}

const close = (a: number | boolean | string, b: number | boolean | string) =>
  typeof a === "number" && typeof b === "number" ? Math.abs(a - b) < 0.011 : a === b;

/** True when `current` is what `wanted` would produce (Off only needs `enabled` to be false). */
export function sectionMatches(current: SectionValues, wanted: SectionValues): boolean {
  if (wanted.enabled === false) return current.enabled === false;
  return Object.entries(wanted).every(([key, value]) => key in current && close(current[key], value));
}

/** Resolves a preset id to its values ("off" turns the section off). */
export function presetValues(section: SectionId, bus: BusId, id: string, saved: { id: string; data: unknown }[] = []): SectionValues | null {
  if (id === OFF_PRESET_ID) return { enabled: false };
  const builtIn = sectionPresetsFor(section, bus).find((p) => p.id === id);
  if (builtIn) return builtIn.values;
  const user = saved.find((p) => p.id === id);
  return user && typeof user.data === "object" && user.data ? (user.data as SectionValues) : null;
}

/** Turns a recipe into concrete values for every section the channel has. */
export function recipeBundle(recipe: Recipe, bus: BusId): SectionBundle {
  const bundle: SectionBundle = {};
  for (const section of SECTION_IDS) {
    if (!sectionAvailable(section, bus)) continue;
    const id = recipe.sections[section];
    bundle[section] = (id && presetValues(section, bus, id)) || { enabled: false };
  }
  return bundle;
}

export function bundleMatches(current: SectionBundle, wanted: SectionBundle): boolean {
  return Object.entries(wanted).every(([section, values]) => {
    const now = current[section as SectionId];
    return !!now && !!values && sectionMatches(now, values);
  });
}

/** Maps a section bundle to the three engine calls' inputs. */
export function applyBundleToConfig(cfg: BusConfig, bundle: SectionBundle): {
  gate: NoiseGateConfig;
  comp: CompressorConfig;
  limiter: LimiterConfig;
  deEsser: DeEsserConfig;
  noise: NoiseSuppressionConfig;
} {
  return {
    gate: { ...cfg.noiseGate, ...(bundle.gate ?? {}) } as NoiseGateConfig,
    comp: { ...cfg.compressor, ...(bundle.comp ?? {}) } as CompressorConfig,
    limiter: { ...(cfg.limiter ?? DEFAULT_LIMITER), ...(bundle.limiter ?? {}) } as LimiterConfig,
    deEsser: { ...(cfg.deEsser ?? DEFAULT_DE_ESSER), ...(bundle.deEsser ?? {}) } as DeEsserConfig,
    noise: { ...(cfg.noiseSuppression ?? DEFAULT_NOISE_SUPPRESSION), ...(bundle.noise ?? {}) } as NoiseSuppressionConfig,
  };
}

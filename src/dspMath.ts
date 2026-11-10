import type { EqBandConfig } from "./types";

export const SAMPLE_RATE = 48000.0;

export interface BiquadCoeffs {
  b0: number;
  b1: number;
  b2: number;
  a1: number;
  a2: number;
}

/**
 * RBJ Audio EQ Cookbook coefficient calculator matching `src-tauri/src/dsp/biquad.rs`.
 */
export function computeBiquadCoeffs(band: EqBandConfig, fs = SAMPLE_RATE): BiquadCoeffs {
  if (!band.enabled) {
    return { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 };
  }

  const f0 = Math.max(20.0, Math.min(20000.0, band.freq));
  const q = Math.max(0.1, Math.min(10.0, band.q));
  const gainDb = Math.max(-12.0, Math.min(12.0, band.gainDb));

  const A = Math.pow(10.0, gainDb / 40.0);
  const omega0 = (2.0 * Math.PI * f0) / fs;
  const cosW0 = Math.cos(omega0);
  const sinW0 = Math.sin(omega0);
  const alpha = sinW0 / (2.0 * q);

  let b0 = 1,
    b1 = 0,
    b2 = 0,
    a0 = 1,
    a1 = 0,
    a2 = 0;

  switch (band.type) {
    case "Peaking": {
      b0 = 1.0 + alpha * A;
      b1 = -2.0 * cosW0;
      b2 = 1.0 - alpha * A;
      a0 = 1.0 + alpha / A;
      a1 = -2.0 * cosW0;
      a2 = 1.0 - alpha / A;
      break;
    }
    case "LowShelf": {
      const twoSqrtAAlpha = 2.0 * Math.sqrt(A) * alpha;
      b0 = A * (A + 1.0 - (A - 1.0) * cosW0 + twoSqrtAAlpha);
      b1 = 2.0 * A * (A - 1.0 - (A + 1.0) * cosW0);
      b2 = A * (A + 1.0 - (A - 1.0) * cosW0 - twoSqrtAAlpha);
      a0 = A + 1.0 + (A - 1.0) * cosW0 + twoSqrtAAlpha;
      a1 = -2.0 * (A - 1.0 + (A + 1.0) * cosW0);
      a2 = A + 1.0 + (A - 1.0) * cosW0 - twoSqrtAAlpha;
      break;
    }
    case "HighShelf": {
      const twoSqrtAAlpha = 2.0 * Math.sqrt(A) * alpha;
      b0 = A * (A + 1.0 + (A - 1.0) * cosW0 + twoSqrtAAlpha);
      b1 = -2.0 * A * (A - 1.0 + (A + 1.0) * cosW0);
      b2 = A * (A + 1.0 + (A - 1.0) * cosW0 - twoSqrtAAlpha);
      a0 = A + 1.0 - (A - 1.0) * cosW0 + twoSqrtAAlpha;
      a1 = 2.0 * (A - 1.0 - (A + 1.0) * cosW0);
      a2 = A + 1.0 - (A - 1.0) * cosW0 - twoSqrtAAlpha;
      break;
    }
    case "LowPass": {
      b1 = 1.0 - cosW0;
      b0 = b1 * 0.5;
      b2 = b0;
      a0 = 1.0 + alpha;
      a1 = -2.0 * cosW0;
      a2 = 1.0 - alpha;
      break;
    }
    case "HighPass": {
      const onePlusCos = 1.0 + cosW0;
      b0 = onePlusCos * 0.5;
      b1 = -onePlusCos;
      b2 = b0;
      a0 = 1.0 + alpha;
      a1 = -2.0 * cosW0;
      a2 = 1.0 - alpha;
      break;
    }
  }

  const invA0 = 1.0 / a0;
  return {
    b0: b0 * invA0,
    b1: b1 * invA0,
    b2: b2 * invA0,
    a1: a1 * invA0,
    a2: a2 * invA0,
  };
}

/**
 * Evaluates single-band dB magnitude response H_k(omega) at `freqHz` (Section 3.2).
 */
export function evaluateBandMagnitudeDb(c: BiquadCoeffs, freqHz: number, fs = SAMPLE_RATE): number {
  const omega = (2.0 * Math.PI * freqHz) / fs;
  const cos1 = Math.cos(omega);
  const sin1 = Math.sin(omega);
  const cos2 = Math.cos(2.0 * omega);
  const sin2 = Math.sin(2.0 * omega);

  const numRe = c.b0 + c.b1 * cos1 + c.b2 * cos2;
  const numIm = c.b1 * sin1 + c.b2 * sin2;
  const denRe = 1.0 + c.a1 * cos1 + c.a2 * cos2;
  const denIm = c.a1 * sin1 + c.a2 * sin2;

  const num = numRe * numRe + numIm * numIm;
  const den = Math.max(1e-18, denRe * denRe + denIm * denIm);
  return 10.0 * Math.log10(Math.max(1e-18, num / den));
}

/**
 * Computes ChatMix crossfader gains [G_Game(m), G_Chat(m)] (Section 3.4).
 */
export function computeChatMixGains(m: number): [number, number] {
  const clamped = Math.max(-1.0, Math.min(1.0, m));
  const gGame = clamped <= 0.0 ? 1.0 : Math.pow(1.0 - clamped, 2);
  const gChat = clamped < 0.0 ? Math.pow(1.0 + clamped, 2) : 1.0;
  return [gGame, gChat];
}

/**
 * Converts linear volume (0.0 .. 1.5) to decibels for console display.
 */
export function linearToDbString(volume: number): string {
  if (volume <= 0.001) return "-∞ dB";
  const db = 20.0 * Math.log10(volume);
  const sign = db > 0.04 ? "+" : "";
  return `${sign}${db.toFixed(1)} dB`;
}

export interface EqPreset {
  id: string;
  name: string;
  description: string;
  gains: [number, number, number, number, number, number, number, number, number, number];
  qs?: [number, number, number, number, number, number, number, number, number, number];
}

export const PLAYBACK_EQ_PRESETS: EqPreset[] = [
  {
    id: "flat",
    name: "Flat Reference",
    description: "0.0 dB unity across all 10 biquad stages",
    gains: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  },
  {
    id: "fps",
    name: "FPS Footsteps",
    description: "Sub-rumble cut + 2kHz–4kHz tactical transient lift",
    gains: [-4.5, -2.5, -1.0, -2.0, 0.5, 2.5, 4.5, 4.0, 1.5, 0.0],
  },
  {
    id: "voice",
    name: "Voice Clarity",
    description: "Low-cut mud removal + 2.5kHz–5kHz vocal articulation",
    gains: [-6.0, -3.5, -1.0, -2.5, 0.5, 2.0, 3.5, 3.0, 2.0, 1.0],
  },
  {
    id: "cinema",
    name: "Bass & Cinema",
    description: "Deep sub-shelf warmth + crisp harmonic highs",
    gains: [5.0, 4.0, 1.5, -1.0, -1.5, 0.0, 1.5, 2.5, 3.5, 3.0],
  },
  {
    id: "music",
    name: "Music & Hi-Fi",
    description: "Harman-tuned curve with punchy low-end and silky high shimmer",
    gains: [3.5, 2.5, 1.0, -0.5, -1.0, 0.5, 1.5, 2.5, 3.0, 2.5],
  },
];

export const MIC_EQ_PRESETS: EqPreset[] = [
  {
    id: "broadcast",
    name: "Warm Broadcast",
    description: "Studio proximity warmth + de-boxiness + smooth air shelf",
    gains: [-7.0, 1.5, 3.0, -3.0, -1.0, 1.0, 2.5, 3.5, 2.5, 2.0],
  },
  {
    id: "mic_clarity",
    name: "Voice Clarity",
    description: "Rumble filter, low-mid mud scoop + 2.5kHz–4kHz vocal intelligibility",
    gains: [-8.0, -5.0, -1.0, -2.5, -0.5, 1.0, 2.5, 4.0, 2.0, 1.0],
  },
  {
    id: "mic_deep",
    name: "Deep Voice (Proximity)",
    description: "Rich 60Hz-125Hz chest resonance and bottom-end authority",
    gains: [-5.0, 3.0, 4.5, -2.0, -1.5, 0.0, 1.0, 2.0, 1.0, 0.0],
  },
  {
    id: "mic_podcast",
    name: "Podcast Host",
    description: "Natural, fatigue-free studio vocal curve for long discussions",
    gains: [-6.0, 0.5, 2.0, -2.0, 0.0, 1.5, 3.0, 3.5, 2.0, 1.5],
  },
  {
    id: "mic_desk_tamer",
    name: "Desk Tamer (Anti-Boom)",
    description: "Steep sub/low-cut to eliminate mechanical desk thumps and keyboard vibration",
    gains: [-12.0, -9.0, -5.0, -3.5, 0.0, 1.0, 2.0, 3.0, 1.5, 0.0],
  },
  {
    id: "mic_de_sibilant",
    name: "Smooth & De-Sibilant",
    description: "Tames harsh piercing sibilance (6k–10kHz) for mellow condenser tones",
    gains: [-8.0, -2.0, 1.5, 0.5, 0.0, 1.0, -1.5, -3.0, -4.5, -2.5],
  },
  {
    id: "mic_asmr",
    name: "Crisp & Airy (ASMR)",
    description: "High-sensitivity ultra-clean air boost for whispers and intimate details",
    gains: [-10.0, -5.0, -1.0, -1.0, 0.0, 1.0, 2.0, 4.5, 5.5, 4.5],
  },
  {
    id: "mic_radio",
    name: "Vintage AM Radio",
    description: "Lo-fi bandpass radio filter with peaked telephone midrange",
    gains: [-14.0, -14.0, -10.0, -2.0, 2.5, 6.0, 4.5, -2.0, -12.0, -14.0],
  },
  {
    id: "mic_walkie",
    name: "Walkie-Talkie (Comm)",
    description: "Tactical military comms / aviation intercom band-limited profile",
    gains: [-15.0, -15.0, -12.0, -4.0, 1.5, 7.5, 6.0, 0.5, -10.0, -15.0],
  },
  {
    id: "mic_megaphone",
    name: "Megaphone (Bullhorn)",
    description: "Gutted low-end with piercing resonant mid horn flare",
    gains: [-15.0, -15.0, -14.0, -6.0, 0.0, 4.5, 8.5, 3.5, -8.0, -15.0],
  },
  {
    id: "mic_late_night",
    name: "Late Night FM (Smooth DJ)",
    description: "Velvety bass warmth with crisp top-end sheen for midnight radio hosts",
    gains: [-3.0, 3.5, 5.0, -2.5, -3.0, -1.5, 1.0, 2.0, 3.0, 2.5],
  },
  {
    id: "flat",
    name: "Flat Reference",
    description: "0.0 dB transparent flat response across all frequencies",
    gains: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
  },
];

export const EQ_PRESETS = PLAYBACK_EQ_PRESETS;
export const ALL_EQ_PRESETS: EqPreset[] = [...PLAYBACK_EQ_PRESETS, ...MIC_EQ_PRESETS];

export function getEqPresetsForBus(bus: string): EqPreset[] {
  return bus === "Mic" ? MIC_EQ_PRESETS : PLAYBACK_EQ_PRESETS;
}


/** Static transfer curve of the downward soft-knee compressor (matches the Rust DSP). */
export function compressorOutputDb(inputDb: number, c: { thresholdDb: number; ratio: number; kneeDb: number; makeupGainDb: number }): number {
  const ratio = Math.max(1, c.ratio);
  const overshoot = inputDb - c.thresholdDb;
  const halfKnee = c.kneeDb * 0.5;
  let reduction = 0;
  if (c.kneeDb > 0.01 && overshoot > -halfKnee && overshoot < halfKnee) {
    const x = overshoot + halfKnee;
    reduction = ((1 - 1 / ratio) * x * x) / (2 * c.kneeDb);
  } else if (overshoot >= halfKnee) {
    reduction = (1 - 1 / ratio) * overshoot;
  }
  return inputDb - reduction + c.makeupGainDb;
}

/** Static transfer curve of the downward-expander noise gate (matches the Rust DSP). */
export function gateOutputDb(inputDb: number, g: { thresholdDb: number; ratio: number }): number {
  if (inputDb >= g.thresholdDb) return inputDb;
  const reduction = Math.min(72, (g.thresholdDb - inputDb) * (1 - 1 / Math.max(1, g.ratio)));
  return inputDb - reduction;
}

use crate::dsp::biquad::{BiquadCoeffs, StereoBiquad};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Floor reported for silence (LUFS).
pub const SILENCE_LUFS: f32 = -100.0;
/// `-100.0_f32.to_bits()`, spelled out so `AtomicStereoMeter::new` can stay `const`.
const SILENCE_LUFS_BITS: u32 = 0xC2C8_0000;
/// `1.0_f32.to_bits()`
const UNITY_BITS: u32 = 0x3F80_0000;

/// Live gain reduction (dB, >= 0) applied by each optional processor of a channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ProcessingReduction {
    #[serde(rename = "gateDb")]
    pub gate_db: f32,
    #[serde(rename = "compDb")]
    pub comp_db: f32,
    #[serde(rename = "deEsserDb")]
    pub de_esser_db: f32,
    #[serde(rename = "limiterDb")]
    pub limiter_db: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StereoMeterSnapshot {
    #[serde(rename = "peakLeft")]
    pub peak_left: f32,
    #[serde(rename = "peakRight")]
    pub peak_right: f32,
    #[serde(rename = "rmsLeft")]
    pub rms_left: f32,
    #[serde(rename = "rmsRight")]
    pub rms_right: f32,
    /// Momentary loudness (400 ms, K-weighted) in LUFS; `-100` is silence.
    #[serde(default = "silence_lufs")]
    pub lufs: f32,
    #[serde(default)]
    pub reduction: ProcessingReduction,
}

fn silence_lufs() -> f32 {
    SILENCE_LUFS
}

impl Default for StereoMeterSnapshot {
    fn default() -> Self {
        Self {
            peak_left: 0.0,
            peak_right: 0.0,
            rms_left: 0.0,
            rms_right: 0.0,
            lufs: SILENCE_LUFS,
            reduction: ProcessingReduction::default(),
        }
    }
}

/// One meter snapshot per channel (Game, Chat, Media, Aux, Mic).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct BusMeterSet {
    pub game: StereoMeterSnapshot,
    pub chat: StereoMeterSnapshot,
    pub media: StereoMeterSnapshot,
    pub aux: StereoMeterSnapshot,
    pub mic: StereoMeterSnapshot,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AllBusMetersPayload {
    /// Level each channel contributes to the stream mix (after its stream send and master level).
    #[serde(rename = "streamSends", default)]
    pub stream_sends: BusMeterSet,
    pub stream: StereoMeterSnapshot,
    #[serde(rename = "streamRunning")]
    pub stream_running: bool,
    #[serde(rename = "streamError")]
    pub stream_error: Option<String>,
    pub game: StereoMeterSnapshot,
    pub chat: StereoMeterSnapshot,
    pub media: StereoMeterSnapshot,
    pub aux: StereoMeterSnapshot,
    pub mic: StereoMeterSnapshot,
    pub master: StereoMeterSnapshot,
    /// Current auto-ducking gain applied to duck targets (1.0 = not ducking).
    #[serde(rename = "duckGain", default)]
    pub duck_gain: f32,
}

/// Lock-free AtomicU32 (f32::to_bits) RMS & Peak tracker for a single stereo bus.
pub struct AtomicStereoMeter {
    peak_left: AtomicU32,
    peak_right: AtomicU32,
    rms_left: AtomicU32,
    rms_right: AtomicU32,
    lufs: AtomicU32,
    /// Gate, compressor, de-esser, limiter reduction in dB (f32 bits).
    reduction: [AtomicU32; 4],
}

impl Default for AtomicStereoMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicStereoMeter {
    pub const fn new() -> Self {
        Self {
            peak_left: AtomicU32::new(0),
            peak_right: AtomicU32::new(0),
            rms_left: AtomicU32::new(0),
            rms_right: AtomicU32::new(0),
            lufs: AtomicU32::new(SILENCE_LUFS_BITS),
            reduction: [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)],
        }
    }

    /// Updates peak and RMS from an interleaved stereo frame block on the real-time thread.
    /// Zero allocations, zero locks.
    #[inline]
    pub fn update_from_stats(&self, peak_left: f32, peak_right: f32, rms_left: f32, rms_right: f32) {
        self.apply_block(peak_left, peak_right, rms_left, rms_right);
    }

    #[inline]
    pub fn update_from_interleaved(&self, interleaved_stereo: &[f32]) {
        if interleaved_stereo.len() < 2 {
            return;
        }
        let mut max_l = 0.0_f32;
        let mut max_r = 0.0_f32;
        let mut sum_sq_l = 0.0_f32;
        let mut sum_sq_r = 0.0_f32;
        let mut count = 0usize;

        for frame in interleaved_stereo.chunks_exact(2) {
            let l = frame[0];
            let r = frame[1];
            let abs_l = l.abs();
            let abs_r = r.abs();
            if abs_l > max_l {
                max_l = abs_l;
            }
            if abs_r > max_r {
                max_r = abs_r;
            }
            sum_sq_l += l * l;
            sum_sq_r += r * r;
            count += 1;
        }

        let inv_n = 1.0 / (count.max(1) as f32);
        let rms_l = (sum_sq_l * inv_n).sqrt();
        let rms_r = (sum_sq_r * inv_n).sqrt();
        self.apply_block(max_l, max_r, rms_l, rms_r);
    }

    /// Applies one block's measurements with the shared ballistics (instant rise, smooth fall).
    #[inline]
    fn apply_block(&self, max_l: f32, max_r: f32, rms_l: f32, rms_r: f32) {

        // Smooth ballistic decay with existing reading
        let prev_pl = f32::from_bits(self.peak_left.load(Ordering::Relaxed));
        let prev_pr = f32::from_bits(self.peak_right.load(Ordering::Relaxed));
        let prev_rl = f32::from_bits(self.rms_left.load(Ordering::Relaxed));
        let prev_rr = f32::from_bits(self.rms_right.load(Ordering::Relaxed));

        let next_pl = max_l.max(prev_pl * 0.86);
        let next_pr = max_r.max(prev_pr * 0.86);
        let next_rl = rms_l * 0.35 + prev_rl * 0.65;
        let next_rr = rms_r * 0.35 + prev_rr * 0.65;

        self.peak_left.store(next_pl.to_bits(), Ordering::Relaxed);
        self.peak_right.store(next_pr.to_bits(), Ordering::Relaxed);
        self.rms_left.store(next_rl.to_bits(), Ordering::Relaxed);
        self.rms_right.store(next_rr.to_bits(), Ordering::Relaxed);
    }

    #[inline]
    pub fn set_lufs(&self, lufs: f32) {
        self.lufs.store(lufs.to_bits(), Ordering::Relaxed);
    }

    /// `[gate, compressor, de-esser, limiter]` reduction in dB. Rises instantly, falls smoothly
    /// so brief reductions stay readable at the UI's poll rate.
    #[inline]
    pub fn set_reductions(&self, now: [f32; 4]) {
        for (slot, value) in self.reduction.iter().zip(now) {
            let previous = f32::from_bits(slot.load(Ordering::Relaxed));
            let next = if value >= previous { value } else { previous * 0.85 };
            slot.store((if next < 0.05 { 0.0 } else { next }).to_bits(), Ordering::Relaxed);
        }
    }

    #[inline]
    pub fn read(&self) -> StereoMeterSnapshot {
        StereoMeterSnapshot {
            peak_left: f32::from_bits(self.peak_left.load(Ordering::Relaxed)),
            peak_right: f32::from_bits(self.peak_right.load(Ordering::Relaxed)),
            rms_left: f32::from_bits(self.rms_left.load(Ordering::Relaxed)),
            rms_right: f32::from_bits(self.rms_right.load(Ordering::Relaxed)),
            lufs: f32::from_bits(self.lufs.load(Ordering::Relaxed)),
            reduction: ProcessingReduction {
                gate_db: f32::from_bits(self.reduction[0].load(Ordering::Relaxed)),
                comp_db: f32::from_bits(self.reduction[1].load(Ordering::Relaxed)),
                de_esser_db: f32::from_bits(self.reduction[2].load(Ordering::Relaxed)),
                limiter_db: f32::from_bits(self.reduction[3].load(Ordering::Relaxed)),
            },
        }
    }
}

const LUFS_WINDOW_FRAMES: u32 = 19_200; // 400 ms at 48 kHz
const LUFS_MAX_BLOCKS: usize = 256;

/// ITU-R BS.1770 K-weighting at 48 kHz: stage 1 high-shelf, stage 2 high-pass (RLB).
const K_STAGE1: BiquadCoeffs = BiquadCoeffs {
    b0: 1.535_124_9,
    b1: -2.691_696_2,
    b2: 1.198_392_8,
    a1: -1.690_659_3,
    a2: 0.732_480_8,
};
const K_STAGE2: BiquadCoeffs = BiquadCoeffs {
    b0: 1.0,
    b1: -2.0,
    b2: 1.0,
    a1: -1.990_047_5,
    a2: 0.990_072_25,
};

/// Momentary loudness (400 ms rectangular window) for a stereo signal. Real-time safe.
pub struct LufsMeter {
    stage1: StereoBiquad,
    stage2: StereoBiquad,
    sum_sq: [f32; LUFS_MAX_BLOCKS],
    frames: [u32; LUFS_MAX_BLOCKS],
    head: usize,
    len: usize,
}

impl Default for LufsMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl LufsMeter {
    pub fn new() -> Self {
        Self {
            stage1: StereoBiquad::new(),
            stage2: StereoBiquad::new(),
            sum_sq: [0.0; LUFS_MAX_BLOCKS],
            frames: [0; LUFS_MAX_BLOCKS],
            head: 0,
            len: 0,
        }
    }

    /// Feeds one block and returns the current momentary loudness in LUFS.
    pub fn process(&mut self, interleaved_stereo: &[f32]) -> f32 {
        let mut block_sum = 0.0_f32;
        let mut block_frames = 0u32;
        for frame in interleaved_stereo.chunks_exact(2) {
            let (mut l, mut r) = (frame[0], frame[1]);
            self.stage1.process_frame(&mut l, &mut r, &K_STAGE1);
            self.stage2.process_frame(&mut l, &mut r, &K_STAGE2);
            block_sum += l * l + r * r;
            block_frames += 1;
        }
        if block_frames == 0 {
            return self.current();
        }

        if self.len == LUFS_MAX_BLOCKS {
            self.head = (self.head + 1) % LUFS_MAX_BLOCKS;
            self.len -= 1;
        }
        let tail = (self.head + self.len) % LUFS_MAX_BLOCKS;
        self.sum_sq[tail] = block_sum;
        self.frames[tail] = block_frames;
        self.len += 1;

        // Drop the oldest blocks while the remaining window still covers 400 ms.
        loop {
            let total: u32 = (0..self.len).map(|i| self.frames[(self.head + i) % LUFS_MAX_BLOCKS]).sum();
            if self.len > 1 && total - self.frames[self.head] >= LUFS_WINDOW_FRAMES {
                self.head = (self.head + 1) % LUFS_MAX_BLOCKS;
                self.len -= 1;
            } else {
                break;
            }
        }
        self.current()
    }

    fn current(&self) -> f32 {
        let mut sum = 0.0_f32;
        let mut frames = 0u32;
        for i in 0..self.len {
            let idx = (self.head + i) % LUFS_MAX_BLOCKS;
            sum += self.sum_sq[idx];
            frames += self.frames[idx];
        }
        if frames == 0 || sum <= 1e-12 {
            return SILENCE_LUFS;
        }
        (-0.691 + 10.0 * (sum / frames as f32).log10()).max(SILENCE_LUFS)
    }
}

pub struct SharedMeterBank {
    pub stream: AtomicStereoMeter,
    pub stream_running: AtomicBool,
    /// Per-channel contribution to the stream mix.
    pub stream_sends: [AtomicStereoMeter; 5],
    pub buses: [AtomicStereoMeter; 5],
    pub master: AtomicStereoMeter,
    duck_gain: AtomicU32,
    /// Live-only level of the mic in the monitor mix (stream playback audition); never saved.
    monitor_mic_gain: AtomicU32,
}

impl Default for SharedMeterBank {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedMeterBank {
    pub const fn new() -> Self {
        Self {
            buses: [
                AtomicStereoMeter::new(),
                AtomicStereoMeter::new(),
                AtomicStereoMeter::new(),
                AtomicStereoMeter::new(),
                AtomicStereoMeter::new(),
            ],
            master: AtomicStereoMeter::new(),
            stream: AtomicStereoMeter::new(),
            stream_running: AtomicBool::new(false),
            stream_sends: [
                AtomicStereoMeter::new(),
                AtomicStereoMeter::new(),
                AtomicStereoMeter::new(),
                AtomicStereoMeter::new(),
                AtomicStereoMeter::new(),
            ],
            duck_gain: AtomicU32::new(UNITY_BITS),
            monitor_mic_gain: AtomicU32::new(UNITY_BITS),
        }
    }

    #[inline]
    pub fn set_monitor_mic_gain(&self, gain: f32) {
        self.monitor_mic_gain.store(gain.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    #[inline]
    pub fn monitor_mic_gain(&self) -> f32 {
        f32::from_bits(self.monitor_mic_gain.load(Ordering::Relaxed))
    }

    #[inline]
    pub fn set_duck_gain(&self, gain: f32) {
        self.duck_gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> AllBusMetersPayload {
        AllBusMetersPayload {
            stream_sends: BusMeterSet {
                game: self.stream_sends[0].read(),
                chat: self.stream_sends[1].read(),
                media: self.stream_sends[2].read(),
                aux: self.stream_sends[3].read(),
                mic: self.stream_sends[4].read(),
            },
            stream: self.stream.read(),
            stream_running: self.stream_running.load(Ordering::Acquire),
            stream_error: None,
            game: self.buses[0].read(),
            chat: self.buses[1].read(),
            media: self.buses[2].read(),
            aux: self.buses[3].read(),
            mic: self.buses[4].read(),
            master: self.master.read(),
            duck_gain: f32::from_bits(self.duck_gain.load(Ordering::Relaxed)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_constants_match() {
        assert_eq!(SILENCE_LUFS_BITS, SILENCE_LUFS.to_bits());
        assert_eq!(UNITY_BITS, 1.0_f32.to_bits());
    }

    #[test]
    fn full_scale_stereo_1khz_sine_reads_about_zero_lufs() {
        let mut meter = LufsMeter::new();
        let block: Vec<f32> = (0..480)
            .flat_map(|n| {
                let v = (2.0 * std::f32::consts::PI * 1000.0 * n as f32 / 48_000.0).sin();
                [v, v]
            })
            .collect();
        let mut lufs = SILENCE_LUFS;
        for _ in 0..80 {
            lufs = meter.process(&block);
        }
        assert!(lufs.abs() < 0.6, "expected ~0 LUFS, got {lufs}");
    }

    #[test]
    fn silence_reads_floor() {
        let mut meter = LufsMeter::new();
        assert_eq!(meter.process(&[0.0; 960]), SILENCE_LUFS);
    }
}

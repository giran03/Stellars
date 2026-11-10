//! Optional per-bus processors: peak limiter, split-band de-esser and RNNoise voice noise
//! suppression. Parameters are lock-free atomics written by the control thread; the processors
//! themselves live on the audio thread and never allocate or lock inside `process_*`.

use crate::dsp::biquad::{BiquadCoeffs, StereoBiquad, DEFAULT_SAMPLE_RATE};
use crate::state::config::{
    DeEsserConfig, EqBandConfig, FilterType, LimiterConfig, NoiseEngine, NoiseSuppressionConfig,
};
use crate::dsp::deepfilter::{self, DfBridge, DfShared, DfWorkerEnds};
use nnnoiseless::DenoiseState;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[inline(always)]
fn db_to_lin(db: f32) -> f32 {
    10.0_f32.powf(db * 0.05)
}

#[inline(always)]
fn one_pole_alpha(time_ms: f32) -> f32 {
    (-1.0 / ((time_ms * 0.001).max(0.0001) * DEFAULT_SAMPLE_RATE)).exp()
}

#[inline(always)]
fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

/// Parameter block shared between the Tauri control thread and the audio thread.
pub struct AtomicExtrasParams {
    limiter_enabled: AtomicBool,
    limiter_ceiling_lin: AtomicU32,
    limiter_release_alpha: AtomicU32,
    de_esser_enabled: AtomicBool,
    de_esser_freq: AtomicU32,
    de_esser_threshold_lin: AtomicU32,
    de_esser_min_gain: AtomicU32,
    noise_enabled: AtomicBool,
    noise_strength: AtomicU32,
    noise_high_quality: AtomicBool,
    /// DeepFilterNet3 state shared with the audio thread and its worker.
    pub df_shared: Arc<DfShared>,
    df_ends: Mutex<Option<DfWorkerEnds>>,
}

impl AtomicExtrasParams {
    pub fn new(
        limiter: &LimiterConfig,
        de_esser: &DeEsserConfig,
        noise: &NoiseSuppressionConfig,
    ) -> Arc<Self> {
        let params = Arc::new(Self {
            limiter_enabled: AtomicBool::new(false),
            limiter_ceiling_lin: AtomicU32::new(1.0_f32.to_bits()),
            limiter_release_alpha: AtomicU32::new(0),
            de_esser_enabled: AtomicBool::new(false),
            de_esser_freq: AtomicU32::new(6000.0_f32.to_bits()),
            de_esser_threshold_lin: AtomicU32::new(1.0_f32.to_bits()),
            de_esser_min_gain: AtomicU32::new(1.0_f32.to_bits()),
            noise_enabled: AtomicBool::new(false),
            noise_strength: AtomicU32::new(1.0_f32.to_bits()),
            noise_high_quality: AtomicBool::new(false),
            df_shared: Arc::new(DfShared::default()),
            df_ends: Mutex::new(None),
        });
        params.update_limiter(limiter);
        params.update_de_esser(de_esser);
        params.update_noise_suppression(noise);
        params
    }

    pub fn update_limiter(&self, cfg: &LimiterConfig) {
        let cfg = cfg.normalized();
        self.limiter_ceiling_lin
            .store(db_to_lin(cfg.ceiling_db).to_bits(), Ordering::Relaxed);
        self.limiter_release_alpha
            .store(one_pole_alpha(cfg.release_ms).to_bits(), Ordering::Relaxed);
        self.limiter_enabled.store(cfg.enabled, Ordering::Relaxed);
    }

    pub fn update_de_esser(&self, cfg: &DeEsserConfig) {
        let cfg = cfg.normalized();
        self.de_esser_freq.store(cfg.frequency.to_bits(), Ordering::Relaxed);
        self.de_esser_threshold_lin
            .store(db_to_lin(cfg.threshold_db).to_bits(), Ordering::Relaxed);
        self.de_esser_min_gain
            .store(db_to_lin(-cfg.max_reduction_db).to_bits(), Ordering::Relaxed);
        self.de_esser_enabled.store(cfg.enabled, Ordering::Relaxed);
    }

    pub fn update_noise_suppression(&self, cfg: &NoiseSuppressionConfig) {
        let cfg = cfg.normalized();
        self.noise_strength.store(cfg.strength.to_bits(), Ordering::Relaxed);
        self.noise_enabled.store(cfg.enabled, Ordering::Relaxed);
        let high = cfg.engine == NoiseEngine::High;
        self.noise_high_quality.store(high, Ordering::Relaxed);
        let wants_model = cfg.enabled && high;
        if wants_model {
            // Start the worker the first time it is needed; it loads the model off the audio thread.
            if let Ok(mut ends) = self.df_ends.lock() {
                if let Some(ends) = ends.take() {
                    deepfilter::spawn_worker(&self.df_shared, ends);
                }
            }
        }
        self.df_shared.active.store(wants_model, Ordering::Release);
    }

    /// Hands the worker's ring ends to the params so the worker can start on demand.
    fn install_worker_ends(&self, ends: DfWorkerEnds) {
        if let Ok(mut slot) = self.df_ends.lock() {
            *slot = Some(ends);
        }
    }
}

/// Peak limiter: instant attack (never exceeds the ceiling), exponential release.
struct Limiter {
    gain: f32,
    /// Deepest gain reduction (dB, >= 0) during the last block.
    reduction_db: f32,
}

impl Limiter {
    fn process(&mut self, buf: &mut [f32], ceiling: f32, release_alpha: f32) {
        let mut min_gain = 1.0_f32;
        for frame in buf.chunks_exact_mut(2) {
            let peak = frame[0].abs().max(frame[1].abs());
            let required = if peak > ceiling { ceiling / peak } else { 1.0 };
            self.gain = if required < self.gain {
                required
            } else {
                required + (self.gain - required) * release_alpha
            };
            min_gain = min_gain.min(self.gain);
            frame[0] = (frame[0] * self.gain).clamp(-ceiling, ceiling);
            frame[1] = (frame[1] * self.gain).clamp(-ceiling, ceiling);
        }
        self.reduction_db = -20.0 * min_gain.max(1e-6).log10();
    }
}

/// Split-band de-esser: `out = in - (1 - gain) * highpass(in)`, so only sibilance is reduced.
struct DeEsser {
    /// Linkwitz-Riley 4th-order split (two cascaded Butterworth biquads per band) so the low
    /// and high bands sum back to a flat response when the gain is 1.
    low_pass: [StereoBiquad; 2],
    high_pass: [StereoBiquad; 2],
    low_coeffs: BiquadCoeffs,
    high_coeffs: BiquadCoeffs,
    coeffs_freq: f32,
    envelope: f32,
    attack_alpha: f32,
    release_alpha: f32,
    /// Deepest sibilance reduction (dB, >= 0) during the last block.
    reduction_db: f32,
}

impl DeEsser {
    fn new() -> Self {
        Self {
            low_pass: [StereoBiquad::new(); 2],
            high_pass: [StereoBiquad::new(); 2],
            low_coeffs: BiquadCoeffs::bypass(),
            high_coeffs: BiquadCoeffs::bypass(),
            coeffs_freq: 0.0,
            envelope: 0.0,
            attack_alpha: one_pole_alpha(1.0),
            release_alpha: one_pole_alpha(40.0),
            reduction_db: 0.0,
        }
    }

    fn crossover(filter_type: FilterType, freq: f32) -> BiquadCoeffs {
        BiquadCoeffs::from_config(
            &EqBandConfig { enabled: true, filter_type, freq, gain_db: 0.0, q: 0.707 },
            DEFAULT_SAMPLE_RATE,
        )
    }

    fn process(&mut self, buf: &mut [f32], freq: f32, threshold: f32, min_gain: f32) {
        if (freq - self.coeffs_freq).abs() > 1.0 {
            self.low_coeffs = Self::crossover(FilterType::LowPass, freq);
            self.high_coeffs = Self::crossover(FilterType::HighPass, freq);
            self.coeffs_freq = freq;
        }
        let mut deepest_gain = 1.0_f32;
        for frame in buf.chunks_exact_mut(2) {
            let (mut ll, mut lr) = (frame[0], frame[1]);
            let (mut hl, mut hr) = (frame[0], frame[1]);
            for stage in &mut self.low_pass {
                stage.process_frame(&mut ll, &mut lr, &self.low_coeffs);
            }
            for stage in &mut self.high_pass {
                stage.process_frame(&mut hl, &mut hr, &self.high_coeffs);
            }
            let level = hl.abs().max(hr.abs());
            let alpha = if level > self.envelope { self.attack_alpha } else { self.release_alpha };
            self.envelope = alpha * self.envelope + (1.0 - alpha) * level;
            let gain = if self.envelope > threshold {
                (threshold / self.envelope).max(min_gain)
            } else {
                1.0
            };
            deepest_gain = deepest_gain.min(gain);
            frame[0] = ll + gain * hl;
            frame[1] = lr + gain * hr;
        }
        self.reduction_db = -20.0 * deepest_gain.max(1e-6).log10();
    }
}

const NS_FRAME: usize = nnnoiseless::FRAME_SIZE;
const NS_RING: usize = 1024;
/// nnnoiseless expects 16-bit-range sample values.
const NS_SCALE: f32 = 32768.0;

/// Fixed-capacity FIFO of f32 (no allocation).
struct Ring {
    buf: [f32; NS_RING],
    head: usize,
    len: usize,
}

impl Ring {
    fn primed() -> Self {
        Self { buf: [0.0; NS_RING], head: 0, len: NS_FRAME }
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.head = 0;
        self.len = NS_FRAME;
    }

    #[inline(always)]
    fn push(&mut self, v: f32) {
        if self.len < NS_RING {
            self.buf[(self.head + self.len) % NS_RING] = v;
            self.len += 1;
        }
    }

    #[inline(always)]
    fn pop(&mut self) -> f32 {
        if self.len == 0 {
            return 0.0;
        }
        let v = self.buf[self.head];
        self.head = (self.head + 1) % NS_RING;
        self.len -= 1;
        v
    }
}

struct DenoiseChannel {
    state: Box<DenoiseState<'static>>,
    input: [f32; NS_FRAME],
    input_len: usize,
    wet: Ring,
    dry: Ring,
}

impl DenoiseChannel {
    fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            input: [0.0; NS_FRAME],
            input_len: 0,
            wet: Ring::primed(),
            dry: Ring::primed(),
        }
    }

    fn reset(&mut self) {
        self.state = DenoiseState::new();
        self.input_len = 0;
        self.wet.reset();
        self.dry.reset();
    }

    #[inline]
    fn process_sample(&mut self, x: f32, strength: f32) -> f32 {
        self.input[self.input_len] = x * NS_SCALE;
        self.input_len += 1;
        if self.input_len == NS_FRAME {
            let mut cleaned = [0.0_f32; NS_FRAME];
            self.state.process_frame(&mut cleaned, &self.input);
            for v in cleaned {
                self.wet.push(v / NS_SCALE);
            }
            self.input_len = 0;
        }
        self.dry.push(x);
        let wet = self.wet.pop();
        let dry = self.dry.pop();
        dry * (1.0 - strength) + wet * strength
    }
}

/// RNNoise on each channel. Adds one 10 ms frame of latency while enabled.
struct NoiseSuppressor {
    channels: [DenoiseChannel; 2],
    was_enabled: bool,
}

impl NoiseSuppressor {
    fn new() -> Self {
        Self { channels: [DenoiseChannel::new(), DenoiseChannel::new()], was_enabled: false }
    }

    fn process(&mut self, buf: &mut [f32], strength: f32) {
        if !self.was_enabled {
            for ch in &mut self.channels {
                ch.reset();
            }
            self.was_enabled = true;
        }
        for frame in buf.chunks_exact_mut(2) {
            frame[0] = self.channels[0].process_sample(frame[0], strength);
            frame[1] = self.channels[1].process_sample(frame[1], strength);
        }
    }
}

/// All optional processors of one bus, owned by the audio thread.
pub struct BusExtras {
    params: Arc<AtomicExtrasParams>,
    limiter: Limiter,
    de_esser: DeEsser,
    noise: NoiseSuppressor,
    deep_filter: DfBridge,
}

impl BusExtras {
    pub fn new(
        limiter: &LimiterConfig,
        de_esser: &DeEsserConfig,
        noise: &NoiseSuppressionConfig,
    ) -> (Self, Arc<AtomicExtrasParams>) {
        let params = AtomicExtrasParams::new(limiter, de_esser, noise);
        let (deep_filter, worker_ends) = deepfilter::create(Arc::clone(&params.df_shared));
        params.install_worker_ends(worker_ends);
        // The config may already ask for DeepFilterNet3 (loaded from disk): start it now.
        params.update_noise_suppression(noise);
        (
            Self {
                params: Arc::clone(&params),
                limiter: Limiter { gain: 1.0, reduction_db: 0.0 },
                de_esser: DeEsser::new(),
                noise: NoiseSuppressor::new(),
                deep_filter,
            },
            params,
        )
    }

    /// Runs before the EQ.
    #[inline]
    pub fn process_noise_suppression(&mut self, buf: &mut [f32]) {
        if !self.params.noise_enabled.load(Ordering::Relaxed) {
            self.noise.was_enabled = false;
            self.deep_filter.process(&mut [], 0.0);
            return;
        }
        let strength = load(&self.params.noise_strength);
        if self.params.noise_high_quality.load(Ordering::Relaxed) && self.deep_filter.process(buf, strength) {
            // Let RNNoise start from a clean state if it is needed again.
            self.noise.was_enabled = false;
        } else {
            self.noise.process(buf, strength);
        }
    }

    /// Runs after the EQ, before gate/compressor.
    #[inline]
    pub fn process_de_esser(&mut self, buf: &mut [f32]) {
        if self.params.de_esser_enabled.load(Ordering::Relaxed) {
            self.de_esser.process(
                buf,
                load(&self.params.de_esser_freq),
                load(&self.params.de_esser_threshold_lin),
                load(&self.params.de_esser_min_gain),
            );
        } else {
            self.de_esser.envelope = 0.0;
            self.de_esser.reduction_db = 0.0;
        }
    }

    /// Runs after the output fader.
    #[inline]
    pub fn process_limiter(&mut self, buf: &mut [f32]) {
        if self.params.limiter_enabled.load(Ordering::Relaxed) {
            self.limiter.process(
                buf,
                load(&self.params.limiter_ceiling_lin),
                load(&self.params.limiter_release_alpha),
            );
        } else {
            self.limiter.gain = 1.0;
            self.limiter.reduction_db = 0.0;
        }
    }

    /// `(de-esser, limiter)` gain reduction in dB (>= 0) from the last block.
    #[inline]
    pub fn reductions_db(&self) -> (f32, f32) {
        (self.de_esser.reduction_db, self.limiter.reduction_db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_stereo(freq: f32, amp: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|n| {
                let v = amp * (2.0 * std::f32::consts::PI * freq * n as f32 / DEFAULT_SAMPLE_RATE).sin();
                [v, v]
            })
            .collect()
    }

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt()
    }

    #[test]
    fn limiter_never_exceeds_ceiling() {
        let (mut extras, _) = BusExtras::new(
            &LimiterConfig { enabled: true, ceiling_db: -6.0, release_ms: 50.0 },
            &DeEsserConfig::default(),
            &NoiseSuppressionConfig::default(),
        );
        let mut buf = sine_stereo(440.0, 1.5, 4800);
        extras.process_limiter(&mut buf);
        let ceiling = db_to_lin(-6.0);
        assert!(buf.iter().all(|s| s.abs() <= ceiling + 1e-5));
    }

    #[test]
    fn disabled_processors_leave_audio_untouched() {
        let (mut extras, _) = BusExtras::new(
            &LimiterConfig::default(),
            &DeEsserConfig::default(),
            &NoiseSuppressionConfig::default(),
        );
        let original = sine_stereo(1000.0, 0.5, 960);
        let mut buf = original.clone();
        extras.process_noise_suppression(&mut buf);
        extras.process_de_esser(&mut buf);
        extras.process_limiter(&mut buf);
        assert_eq!(buf, original);
    }

    #[test]
    fn de_esser_reduces_sibilance_more_than_low_frequencies() {
        let cfg = DeEsserConfig { enabled: true, frequency: 5000.0, threshold_db: -40.0, max_reduction_db: 12.0 };
        let run = |freq: f32| {
            let (mut extras, _) = BusExtras::new(&LimiterConfig::default(), &cfg, &NoiseSuppressionConfig::default());
            let mut buf = sine_stereo(freq, 0.5, 9600);
            let before = rms(&buf[9600..]);
            extras.process_de_esser(&mut buf);
            rms(&buf[9600..]) / before
        };
        let low = run(300.0);
        let high = run(8000.0);
        assert!(low > 0.95, "low band should be untouched, ratio {low}");
        assert!(high < 0.6, "sibilance should be reduced, ratio {high}");
    }

    #[test]
    fn noise_suppressor_keeps_length_stays_finite_and_attenuates_noise() {
        let (mut extras, _) = BusExtras::new(
            &LimiterConfig::default(),
            &DeEsserConfig::default(),
            &NoiseSuppressionConfig { enabled: true, strength: 1.0, engine: NoiseEngine::Fast },
        );
        // Deterministic pseudo-noise, low level.
        let mut seed = 0x1234_5678_u32;
        let mut buf: Vec<f32> = (0..48_000 * 2)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.1
            })
            .collect();
        let before = rms(&buf[48_000..]);
        extras.process_noise_suppression(&mut buf);
        assert_eq!(buf.len(), 96_000);
        assert!(buf.iter().all(|s| s.is_finite()));
        assert!(rms(&buf[48_000..]) < before * 0.8, "broadband noise should be reduced: before {before} after {}", rms(&buf[48_000..]));
    }

    #[test]
    fn reports_limiter_and_idle_reduction() {
        let limiter = LimiterConfig { enabled: true, ceiling_db: -6.0, release_ms: 80.0 };
        let (mut extras, _) = BusExtras::new(&limiter, &DeEsserConfig::default(), &NoiseSuppressionConfig::default());
        let mut loud = sine_stereo(1000.0, 0.95, 960);
        extras.process_limiter(&mut loud);
        assert!(extras.reductions_db().1 > 3.0, "limiter should report reduction");
        let mut off = BusExtras::new(&LimiterConfig::default(), &DeEsserConfig::default(), &NoiseSuppressionConfig::default()).0;
        let mut buf = sine_stereo(1000.0, 0.95, 960);
        off.process_limiter(&mut buf);
        off.process_de_esser(&mut buf);
        assert_eq!(off.reductions_db(), (0.0, 0.0));
    }

    #[test]
    fn deep_filter_engine_cleans_noise_after_loading_and_never_drops_audio() {
        let (mut extras, _params) = BusExtras::new(
            &LimiterConfig::default(),
            &DeEsserConfig::default(),
            &NoiseSuppressionConfig { enabled: true, strength: 1.0, engine: NoiseEngine::High },
        );
        let mut seed = 0x9e37_79b9_u32;
        let mut block_rms = Vec::new();
        let mut in_rms = Vec::new();
        // 10 ms blocks paced in real time so the worker keeps up like it would in the app.
        for _ in 0..400 {
            let mut buf: Vec<f32> = (0..480 * 2)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.1
                })
                .collect();
            in_rms.push(rms(&buf));
            extras.process_noise_suppression(&mut buf);
            assert_eq!(buf.len(), 960);
            assert!(buf.iter().all(|s| s.is_finite()));
            block_rms.push(rms(&buf));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let tail_in: f32 = in_rms[300..].iter().sum::<f32>() / 100.0;
        let tail_out: f32 = block_rms[300..].iter().sum::<f32>() / 100.0;
        assert!(tail_out < tail_in * 0.8, "noise should be reduced: in {tail_in} out {tail_out}");
    }
}

use crate::dsp::biquad::DEFAULT_SAMPLE_RATE;
use crate::state::config::{CompressorConfig, NoiseGateConfig};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

#[inline(always)]
fn lin_to_db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

#[inline(always)]
fn db_to_lin(db: f32) -> f32 {
    10.0_f32.powf(db * 0.05)
}

#[inline(always)]
fn time_constant_coeff(time_ms: f32, sample_rate: f32) -> f32 {
    let t_sec = (time_ms * 0.001).max(0.0001);
    (-1.0 / (t_sec * sample_rate)).exp()
}

/// Peak/RMS Envelope Follower with exponential attack & release coefficients:
/// alpha_A = exp(-1 / (t_attack * f_s)), alpha_R = exp(-1 / (t_release * f_s))
#[derive(Debug, Clone, Copy)]
pub struct EnvelopeFollower {
    envelope: f32,
}

impl Default for EnvelopeFollower {
    fn default() -> Self {
        Self { envelope: 0.0 }
    }
}

impl EnvelopeFollower {
    #[inline(always)]
    pub fn process(&mut self, input_abs: f32, alpha_attack: f32, alpha_release: f32) -> f32 {
        let coeff = if input_abs > self.envelope {
            alpha_attack
        } else {
            alpha_release
        };
        self.envelope = coeff * self.envelope + (1.0 - coeff) * input_abs;
        if self.envelope < 1e-20 {
            self.envelope = 0.0;
        }
        self.envelope
    }
}

/// Lock-free atomic parameter block shared between Tauri control thread and audio render thread.
pub struct AtomicDynamicsParams {
    gate_enabled: AtomicBool,
    gate_threshold_db: AtomicU32,
    gate_ratio: AtomicU32,
    gate_alpha_attack: AtomicU32,
    gate_alpha_release: AtomicU32,

    comp_enabled: AtomicBool,
    comp_threshold_db: AtomicU32,
    comp_ratio: AtomicU32,
    comp_knee_db: AtomicU32,
    comp_makeup_lin: AtomicU32,
    comp_alpha_attack: AtomicU32,
    comp_alpha_release: AtomicU32,
}

impl AtomicDynamicsParams {
    pub fn new(gate: &NoiseGateConfig, comp: &CompressorConfig, sample_rate: f32) -> Arc<Self> {
        let params = Arc::new(Self {
            gate_enabled: AtomicBool::new(false),
            gate_threshold_db: AtomicU32::new(0),
            gate_ratio: AtomicU32::new(0),
            gate_alpha_attack: AtomicU32::new(0),
            gate_alpha_release: AtomicU32::new(0),
            comp_enabled: AtomicBool::new(false),
            comp_threshold_db: AtomicU32::new(0),
            comp_ratio: AtomicU32::new(0),
            comp_knee_db: AtomicU32::new(0),
            comp_makeup_lin: AtomicU32::new(0),
            comp_alpha_attack: AtomicU32::new(0),
            comp_alpha_release: AtomicU32::new(0),
        });
        params.update_gate(gate, sample_rate);
        params.update_compressor(comp, sample_rate);
        params
    }

    pub fn update_gate(&self, cfg: &NoiseGateConfig, sample_rate: f32) {
        let fs = sample_rate.max(8000.0);
        self.gate_enabled.store(cfg.enabled, Ordering::Relaxed);
        self.gate_threshold_db.store(
            cfg.threshold_db.clamp(-80.0, 0.0).to_bits(),
            Ordering::Relaxed,
        );
        self.gate_ratio
            .store(cfg.ratio.clamp(1.0, 20.0).to_bits(), Ordering::Relaxed);
        self.gate_alpha_attack.store(
            time_constant_coeff(cfg.attack_ms, fs).to_bits(),
            Ordering::Relaxed,
        );
        self.gate_alpha_release.store(
            time_constant_coeff(cfg.release_ms, fs).to_bits(),
            Ordering::Relaxed,
        );
    }

    pub fn update_compressor(&self, cfg: &CompressorConfig, sample_rate: f32) {
        let fs = sample_rate.max(8000.0);
        self.comp_enabled.store(cfg.enabled, Ordering::Relaxed);
        self.comp_threshold_db.store(
            cfg.threshold_db.clamp(-60.0, 0.0).to_bits(),
            Ordering::Relaxed,
        );
        self.comp_ratio
            .store(cfg.ratio.clamp(1.0, 20.0).to_bits(), Ordering::Relaxed);
        self.comp_knee_db
            .store(cfg.knee_db.clamp(0.0, 24.0).to_bits(), Ordering::Relaxed);
        self.comp_makeup_lin.store(
            db_to_lin(cfg.makeup_gain_db.clamp(-12.0, 24.0)).to_bits(),
            Ordering::Relaxed,
        );
        self.comp_alpha_attack.store(
            time_constant_coeff(cfg.attack_ms, fs).to_bits(),
            Ordering::Relaxed,
        );
        self.comp_alpha_release.store(
            time_constant_coeff(cfg.release_ms, fs).to_bits(),
            Ordering::Relaxed,
        );
    }
}

/// Real-time Noise Gate + Soft-Knee Downward Compressor stage.
pub struct DynamicsProcessor {
    gate_env: EnvelopeFollower,
    comp_env: EnvelopeFollower,
    params: Arc<AtomicDynamicsParams>,
    /// Largest gain reduction (dB, >= 0) applied during the last processed block.
    gate_reduction_db: f32,
    comp_reduction_db: f32,
}

impl DynamicsProcessor {
    pub fn new(gate: &NoiseGateConfig, comp: &CompressorConfig) -> (Self, Arc<AtomicDynamicsParams>) {
        let params = AtomicDynamicsParams::new(gate, comp, DEFAULT_SAMPLE_RATE);
        (
            Self {
                gate_env: EnvelopeFollower::default(),
                comp_env: EnvelopeFollower::default(),
                params: Arc::clone(&params),
                gate_reduction_db: 0.0,
                comp_reduction_db: 0.0,
            },
            params,
        )
    }

    /// `(gate, compressor)` gain reduction in dB (>= 0) from the last block.
    #[inline]
    pub fn reductions_db(&self) -> (f32, f32) {
        (self.gate_reduction_db, self.comp_reduction_db)
    }

    #[inline]
    pub fn process_interleaved(&mut self, interleaved_stereo: &mut [f32]) {
        let gate_enabled = self.params.gate_enabled.load(Ordering::Relaxed);
        let gate_thresh_db = f32::from_bits(self.params.gate_threshold_db.load(Ordering::Relaxed));
        let gate_ratio = f32::from_bits(self.params.gate_ratio.load(Ordering::Relaxed));
        let gate_att = f32::from_bits(self.params.gate_alpha_attack.load(Ordering::Relaxed));
        let gate_rel = f32::from_bits(self.params.gate_alpha_release.load(Ordering::Relaxed));

        let comp_enabled = self.params.comp_enabled.load(Ordering::Relaxed);
        let comp_thresh_db = f32::from_bits(self.params.comp_threshold_db.load(Ordering::Relaxed));
        let comp_ratio = f32::from_bits(self.params.comp_ratio.load(Ordering::Relaxed));
        let comp_knee_db = f32::from_bits(self.params.comp_knee_db.load(Ordering::Relaxed));
        let comp_makeup = f32::from_bits(self.params.comp_makeup_lin.load(Ordering::Relaxed));
        let comp_att = f32::from_bits(self.params.comp_alpha_attack.load(Ordering::Relaxed));
        let comp_rel = f32::from_bits(self.params.comp_alpha_release.load(Ordering::Relaxed));

        self.gate_reduction_db = 0.0;
        self.comp_reduction_db = 0.0;
        if !gate_enabled && !comp_enabled {
            return;
        }

        for frame in interleaved_stereo.chunks_exact_mut(2) {
            let mut l = frame[0];
            let mut r = frame[1];
            let peak = l.abs().max(r.abs());

            // 1. Noise Gate stage
            if gate_enabled {
                let env = self.gate_env.process(peak, gate_att, gate_rel);
                let env_db = lin_to_db(env);
                if env_db < gate_thresh_db {
                    let under_db = gate_thresh_db - env_db;
                    let gain_reduction_db = -(under_db * (1.0 - 1.0 / gate_ratio.max(1.0))).max(-72.0);
                    let g = db_to_lin(gain_reduction_db);
                    self.gate_reduction_db = self.gate_reduction_db.max(-gain_reduction_db);
                    l *= g;
                    r *= g;
                }
            }

            // 2. Soft-Knee Downward Compressor stage
            if comp_enabled {
                let post_gate_peak = l.abs().max(r.abs());
                let env = self.comp_env.process(post_gate_peak, comp_att, comp_rel);
                let env_db = lin_to_db(env);
                let overshoot = env_db - comp_thresh_db;
                let half_knee = comp_knee_db * 0.5;

                let gain_reduction_db = if comp_knee_db > 0.01 && overshoot > -half_knee && overshoot < half_knee {
                    let x = overshoot + half_knee;
                    -((1.0 - 1.0 / comp_ratio) * (x * x) / (2.0 * comp_knee_db))
                } else if overshoot >= half_knee {
                    -((1.0 - 1.0 / comp_ratio) * overshoot)
                } else {
                    0.0
                };

                self.comp_reduction_db = self.comp_reduction_db.max(-gain_reduction_db);
                let total_gain = db_to_lin(gain_reduction_db) * comp_makeup;
                l *= total_gain;
                r *= total_gain;
            }

            frame[0] = l;
            frame[1] = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compressor_reduces_loud_signal() {
        let gate = NoiseGateConfig {
            enabled: false,
            ..Default::default()
        };
        let comp = CompressorConfig {
            enabled: true,
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 0.5,
            release_ms: 50.0,
            makeup_gain_db: 0.0,
            knee_db: 3.0,
        };
        let (mut proc, _params) = DynamicsProcessor::new(&gate, &comp);
        let mut signal = vec![0.8_f32; 480 * 2];
        proc.process_interleaved(&mut signal);
        let last_sample = signal[signal.len() - 1].abs();
        assert!(last_sample < 0.5, "Expected compressed signal < 0.5, got {last_sample}");
    }

    #[test]
    fn reports_compressor_gain_reduction() {
        let gate = NoiseGateConfig { enabled: false, ..Default::default() };
        let comp = CompressorConfig {
            enabled: true,
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 0.5,
            release_ms: 50.0,
            makeup_gain_db: 0.0,
            knee_db: 0.0,
        };
        let (mut proc, _params) = DynamicsProcessor::new(&gate, &comp);
        let mut signal = vec![0.8_f32; 480 * 2];
        proc.process_interleaved(&mut signal);
        let (gate_db, comp_db) = proc.reductions_db();
        assert_eq!(gate_db, 0.0);
        assert!(comp_db > 6.0, "expected clear reduction, got {comp_db}");
        let mut quiet = vec![0.001_f32; 480 * 2];
        for _ in 0..50 { proc.process_interleaved(&mut quiet); }
        assert!(proc.reductions_db().1 < 0.5, "reduction should release on quiet input");
    }
}

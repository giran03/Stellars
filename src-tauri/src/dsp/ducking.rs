//! Auto-ducking: lowers a set of target buses while a trigger bus (e.g. voice chat) is active.
//! The gain follows block-rate attack/release smoothing and is interpolated per sample by the
//! caller, so there are no zipper artifacts.

use crate::dsp::biquad::DEFAULT_SAMPLE_RATE;
use crate::state::config::DuckingConfig;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

#[inline(always)]
fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

/// Parameters written by the control thread, read by the audio thread.
pub struct AtomicDuckParams {
    enabled: AtomicBool,
    trigger_mask: AtomicU32,
    target_mask: AtomicU32,
    amount_lin: AtomicU32,
    threshold_lin: AtomicU32,
    attack_ms: AtomicU32,
    release_ms: AtomicU32,
}

impl AtomicDuckParams {
    pub fn new(cfg: &DuckingConfig) -> Arc<Self> {
        let params = Arc::new(Self {
            enabled: AtomicBool::new(false),
            trigger_mask: AtomicU32::new(0),
            target_mask: AtomicU32::new(0),
            amount_lin: AtomicU32::new(1.0_f32.to_bits()),
            threshold_lin: AtomicU32::new(1.0_f32.to_bits()),
            attack_ms: AtomicU32::new(30.0_f32.to_bits()),
            release_ms: AtomicU32::new(600.0_f32.to_bits()),
        });
        params.update(cfg);
        params
    }

    pub fn update(&self, cfg: &DuckingConfig) {
        let cfg = cfg.clone().normalized();
        self.trigger_mask.store(cfg.trigger_mask(), Ordering::Relaxed);
        self.target_mask.store(cfg.target_mask(), Ordering::Relaxed);
        self.amount_lin
            .store(10.0_f32.powf(cfg.amount_db * 0.05).to_bits(), Ordering::Relaxed);
        self.threshold_lin
            .store(10.0_f32.powf(cfg.threshold_db * 0.05).to_bits(), Ordering::Relaxed);
        self.attack_ms.store(cfg.attack_ms.to_bits(), Ordering::Relaxed);
        self.release_ms.store(cfg.release_ms.to_bits(), Ordering::Relaxed);
        self.enabled.store(cfg.enabled, Ordering::Relaxed);
    }

    #[inline]
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn trigger_mask(&self) -> u32 {
        self.trigger_mask.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn target_mask(&self) -> u32 {
        self.target_mask.load(Ordering::Relaxed)
    }
}

/// RMS of an interleaved stereo block (0.0 for empty input).
#[inline]
pub fn block_rms(interleaved_stereo: &[f32]) -> f32 {
    if interleaved_stereo.is_empty() {
        return 0.0;
    }
    let sum: f32 = interleaved_stereo.iter().map(|s| s * s).sum();
    (sum / interleaved_stereo.len() as f32).sqrt()
}

/// Smoothed duck gain owned by the audio thread.
pub struct DuckState {
    gain: f32,
}

impl Default for DuckState {
    fn default() -> Self {
        Self::new()
    }
}

impl DuckState {
    pub const fn new() -> Self {
        Self { gain: 1.0 }
    }

    /// Advances by one block of `frames` and returns `(previous_gain, new_gain)`; the caller
    /// interpolates linearly between them across the block.
    pub fn advance(&mut self, params: &AtomicDuckParams, trigger_level: f32, frames: usize) -> (f32, f32) {
        let previous = self.gain;
        let active = params.enabled() && trigger_level > load(&params.threshold_lin);
        let target = if active { load(&params.amount_lin) } else { 1.0 };
        let tau_ms = if target < self.gain { load(&params.attack_ms) } else { load(&params.release_ms) };
        let dt = frames as f32 / DEFAULT_SAMPLE_RATE;
        let coeff = 1.0 - (-dt / (tau_ms * 0.001).max(0.001)).exp();
        self.gain += (target - self.gain) * coeff;
        if (1.0 - self.gain).abs() < 1e-4 && target >= 1.0 {
            self.gain = 1.0;
        }
        (previous, self.gain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::config::BusId;

    fn params() -> Arc<AtomicDuckParams> {
        AtomicDuckParams::new(&DuckingConfig {
            enabled: true,
            triggers: vec![BusId::Chat],
            targets: vec![BusId::Game],
            amount_db: -12.0,
            threshold_db: -40.0,
            attack_ms: 20.0,
            release_ms: 300.0,
        })
    }

    #[test]
    fn ducks_toward_amount_then_recovers() {
        let p = params();
        let mut state = DuckState::new();
        let amount = 10.0_f32.powf(-12.0 / 20.0);
        let mut gain = 1.0;
        for _ in 0..100 {
            gain = state.advance(&p, 0.2, 480).1;
        }
        assert!((gain - amount).abs() < 0.01, "ducked gain {gain}");
        for _ in 0..300 {
            gain = state.advance(&p, 0.0, 480).1;
        }
        assert!((gain - 1.0).abs() < 1e-3, "recovered gain {gain}");
    }

    #[test]
    fn disabled_never_ducks() {
        let p = params();
        p.update(&DuckingConfig { enabled: false, ..DuckingConfig::default() });
        let mut state = DuckState::new();
        for _ in 0..50 {
            assert_eq!(state.advance(&p, 1.0, 480).1, 1.0);
        }
    }

    #[test]
    fn masks_exclude_mic_targets_and_triggers_from_targets() {
        let cfg = DuckingConfig {
            triggers: vec![BusId::Chat],
            targets: vec![BusId::Chat, BusId::Mic, BusId::Game],
            ..DuckingConfig::default()
        }
        .normalized();
        assert_eq!(cfg.targets, vec![BusId::Game]);
        assert_eq!(cfg.target_mask(), 1 << BusId::Game.index());
        assert_eq!(cfg.trigger_mask(), 1 << BusId::Chat.index());
    }
}

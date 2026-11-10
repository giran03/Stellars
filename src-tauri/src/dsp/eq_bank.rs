use crate::dsp::biquad::{BiquadCoeffs, StereoBiquad, DEFAULT_SAMPLE_RATE};
use crate::state::config::EqBandConfig;
use rtrb::{Consumer, Producer, RingBuffer};

pub const NUM_EQ_BANDS: usize = 10;
pub const RAMP_FRAMES: u32 = 128;

#[derive(Debug, Clone, Copy)]
pub struct CoeffsUpdate {
    pub band_index: usize,
    pub target: BiquadCoeffs,
}

#[derive(Debug, Clone, Copy)]
struct BandRampState {
    current: BiquadCoeffs,
    step: BiquadCoeffs,
    target: BiquadCoeffs,
    frames_remaining: u32,
}

impl Default for BandRampState {
    fn default() -> Self {
        let bypass = BiquadCoeffs::bypass();
        Self {
            current: bypass,
            step: BiquadCoeffs {
                b0: 0.0,
                b1: 0.0,
                b2: 0.0,
                a1: 0.0,
                a2: 0.0,
            },
            target: bypass,
            frames_remaining: 0,
        }
    }
}

impl BandRampState {
    #[inline]
    fn set_target(&mut self, target: BiquadCoeffs) {
        let inv = 1.0 / (RAMP_FRAMES as f32);
        self.target = target;
        self.step = BiquadCoeffs {
            b0: (target.b0 - self.current.b0) * inv,
            b1: (target.b1 - self.current.b1) * inv,
            b2: (target.b2 - self.current.b2) * inv,
            a1: (target.a1 - self.current.a1) * inv,
            a2: (target.a2 - self.current.a2) * inv,
        };
        self.frames_remaining = RAMP_FRAMES;
    }

    #[inline(always)]
    fn advance_frame(&mut self) -> &BiquadCoeffs {
        if self.frames_remaining > 0 {
            self.frames_remaining -= 1;
            if self.frames_remaining == 0 {
                self.current = self.target;
            } else {
                self.current.b0 += self.step.b0;
                self.current.b1 += self.step.b1;
                self.current.b2 += self.step.b2;
                self.current.a1 += self.step.a1;
                self.current.a2 += self.step.a2;
            }
        }
        &self.current
    }
}

/// Producer handle held by the Tauri control thread to push lock-free EQ updates.
pub struct EqBankControl {
    tx: Producer<CoeffsUpdate>,
}

impl EqBankControl {
    pub fn update_band(&mut self, band_index: usize, cfg: &EqBandConfig, sample_rate: f32) {
        if band_index < NUM_EQ_BANDS {
            let target = BiquadCoeffs::from_config(cfg, sample_rate);
            let _ = self.tx.push(CoeffsUpdate {
                band_index,
                target,
            });
        }
    }

    pub fn update_all_bands(&mut self, bands: &[EqBandConfig], sample_rate: f32) {
        for (idx, cfg) in bands.iter().enumerate().take(NUM_EQ_BANDS) {
            self.update_band(idx, cfg, sample_rate);
        }
    }
}

/// Real-time 10-band cascaded Parametric EQ with 128-sample coefficient smoothing.
pub struct EqBank {
    filters: [StereoBiquad; NUM_EQ_BANDS],
    ramps: [BandRampState; NUM_EQ_BANDS],
    rx: Consumer<CoeffsUpdate>,
}

impl EqBank {
    pub fn new(initial_bands: &[EqBandConfig], sample_rate: f32) -> (Self, EqBankControl) {
        let (tx, rx) = RingBuffer::new(128);
        let mut ramps = [BandRampState::default(); NUM_EQ_BANDS];
        for (idx, cfg) in initial_bands.iter().enumerate().take(NUM_EQ_BANDS) {
            let c = BiquadCoeffs::from_config(cfg, sample_rate);
            ramps[idx].current = c;
            ramps[idx].target = c;
            ramps[idx].frames_remaining = 0;
        }
        (
            Self {
                filters: [StereoBiquad::new(); NUM_EQ_BANDS],
                ramps,
                rx,
            },
            EqBankControl { tx },
        )
    }

    #[inline(always)]
    fn poll_updates(&mut self) {
        while let Ok(update) = self.rx.pop() {
            if update.band_index < NUM_EQ_BANDS {
                self.ramps[update.band_index].set_target(update.target);
            }
        }
    }

    /// Processes an interleaved stereo buffer (`[L0, R0, L1, R1, ...]`) in place.
    /// Zero allocations, zero locks.
    #[inline]
    pub fn process_interleaved(&mut self, interleaved_stereo: &mut [f32]) {
        self.poll_updates();

        for frame in interleaved_stereo.chunks_exact_mut(2) {
            let mut l = frame[0];
            let mut r = frame[1];
            for band_idx in 0..NUM_EQ_BANDS {
                let coeffs = self.ramps[band_idx].advance_frame();
                self.filters[band_idx].process_frame(&mut l, &mut r, coeffs);
            }
            frame[0] = l;
            frame[1] = r;
        }
    }
}

/// Evaluates the composite 10-band frequency response curve (dB) across `frequencies_hz`.
pub fn evaluate_composite_response_db(
    bands: &[EqBandConfig],
    frequencies_hz: &[f32],
    sample_rate: f32,
) -> Vec<f32> {
    let fs = if sample_rate > 0.0 {
        sample_rate
    } else {
        DEFAULT_SAMPLE_RATE
    };
    let coeffs: Vec<BiquadCoeffs> = bands
        .iter()
        .take(NUM_EQ_BANDS)
        .map(|b| BiquadCoeffs::from_config(b, fs))
        .collect();

    frequencies_hz
        .iter()
        .map(|&f| coeffs.iter().map(|c| c.magnitude_db(f, fs)).sum())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::config::FilterType;

    #[test]
    fn test_eq_bank_128_frame_ramp() {
        let bands: Vec<EqBandConfig> = (0..10).map(EqBandConfig::default_for_band).collect();
        let (mut eq, mut ctrl) = EqBank::new(&bands, 48000.0);

        let boosted = EqBandConfig {
            enabled: true,
            filter_type: FilterType::Peaking,
            freq: 1000.0,
            gain_db: 6.0,
            q: 1.0,
        };
        ctrl.update_band(5, &boosted, 48000.0);

        let mut buffer = vec![0.1_f32; 256 * 2]; // 256 stereo frames
        eq.process_interleaved(&mut buffer);

        // After 256 frames (> 128 frame ramp), ramp should have converged to target
        assert_eq!(eq.ramps[5].frames_remaining, 0);
        let expected = BiquadCoeffs::from_config(&boosted, 48000.0);
        assert!((eq.ramps[5].current.b0 - expected.b0).abs() < 1e-5);
    }
}

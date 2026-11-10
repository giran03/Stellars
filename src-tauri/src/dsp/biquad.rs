use crate::state::config::{EqBandConfig, FilterType};
use std::f32::consts::PI;

pub const DEFAULT_SAMPLE_RATE: f32 = 48000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Default for BiquadCoeffs {
    fn default() -> Self {
        Self::bypass()
    }
}

impl BiquadCoeffs {
    #[inline]
    pub const fn bypass() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    /// Computes normalized 2nd-order Biquad coefficients using the RBJ Audio EQ Cookbook.
    pub fn from_config(cfg: &EqBandConfig, sample_rate: f32) -> Self {
        if !cfg.enabled {
            return Self::bypass();
        }

        let fs = sample_rate.max(8000.0);
        let f0 = cfg.freq.clamp(20.0, (fs * 0.495).min(20000.0));
        let q = cfg.q.clamp(0.1, 10.0);
        let gain_db = cfg.gain_db.clamp(-12.0, 12.0);

        let a = 10.0_f32.powf(gain_db / 40.0);
        let omega0 = 2.0 * PI * (f0 / fs);
        let cos_w0 = omega0.cos();
        let sin_w0 = omega0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let (b0, b1, b2, a0, a1, a2) = match cfg.filter_type {
            FilterType::Peaking => {
                let b0 = 1.0 + alpha * a;
                let b1 = -2.0 * cos_w0;
                let b2 = 1.0 - alpha * a;
                let a0 = 1.0 + alpha / a;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha / a;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::LowShelf => {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
                let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
                let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
                let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
                let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
                let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::HighShelf => {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
                let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
                let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
                let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
                let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
                let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::LowPass => {
                let b1 = 1.0 - cos_w0;
                let b0 = b1 * 0.5;
                let b2 = b0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::HighPass => {
                let one_plus_cos = 1.0 + cos_w0;
                let b0 = one_plus_cos * 0.5;
                let b1 = -one_plus_cos;
                let b2 = b0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
        };

        let inv_a0 = 1.0 / a0;
        Self {
            b0: b0 * inv_a0,
            b1: b1 * inv_a0,
            b2: b2 * inv_a0,
            a1: a1 * inv_a0,
            a2: a2 * inv_a0,
        }
    }

    /// Evaluates the dB magnitude response H_k(omega) at frequency `freq_hz`.
    #[inline]
    pub fn magnitude_db(&self, freq_hz: f32, sample_rate: f32) -> f32 {
        let omega = 2.0 * PI * (freq_hz / sample_rate);
        let cos1 = omega.cos();
        let sin1 = omega.sin();
        let cos2 = (2.0 * omega).cos();
        let sin2 = (2.0 * omega).sin();

        let num_re = self.b0 + self.b1 * cos1 + self.b2 * cos2;
        let num_im = self.b1 * sin1 + self.b2 * sin2;
        let den_re = 1.0 + self.a1 * cos1 + self.a2 * cos2;
        let den_im = self.a1 * sin1 + self.a2 * sin2;

        let num = num_re * num_re + num_im * num_im;
        let den = (den_re * den_re + den_im * den_im).max(1e-18);
        10.0 * (num / den).max(1e-18).log10()
    }
}

/// Stereo 2nd-order Biquad filter using Direct Form II Transposed difference equations.
#[derive(Debug, Clone, Copy, Default)]
pub struct StereoBiquad {
    z1: [f32; 2],
    z2: [f32; 2],
}

impl StereoBiquad {
    #[inline]
    pub const fn new() -> Self {
        Self {
            z1: [0.0; 2],
            z2: [0.0; 2],
        }
    }

    #[inline(always)]
    pub fn process_sample(&mut self, x: f32, channel: usize, c: &BiquadCoeffs) -> f32 {
        let ch = channel & 1;
        let y = c.b0 * x + self.z1[ch];
        let mut z1_next = c.b1 * x - c.a1 * y + self.z2[ch];
        let mut z2_next = c.b2 * x - c.a2 * y;

        // Flush denormals on real-time audio path
        if z1_next.abs() < 1e-30 {
            z1_next = 0.0;
        }
        if z2_next.abs() < 1e-30 {
            z2_next = 0.0;
        }

        self.z1[ch] = z1_next;
        self.z2[ch] = z2_next;
        y
    }

    #[inline(always)]
    pub fn process_frame(&mut self, left: &mut f32, right: &mut f32, c: &BiquadCoeffs) {
        *left = self.process_sample(*left, 0, c);
        *right = self.process_sample(*right, 1, c);
    }

    pub fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peaking_eq_center_gain() {
        let cfg = EqBandConfig {
            enabled: true,
            filter_type: FilterType::Peaking,
            freq: 1000.0,
            gain_db: 6.0,
            q: 1.414,
        };
        let coeffs = BiquadCoeffs::from_config(&cfg, 48000.0);
        let mag_center = coeffs.magnitude_db(1000.0, 48000.0);
        let mag_dc = coeffs.magnitude_db(20.0, 48000.0);
        assert!((mag_center - 6.0).abs() < 0.05, "mag_center = {mag_center}");
        assert!(mag_dc.abs() < 0.05, "mag_dc = {mag_dc}");
    }

    #[test]
    fn test_bypass_unity_response() {
        let coeffs = BiquadCoeffs::bypass();
        let mut biquad = StereoBiquad::new();
        let mut l = 0.75;
        let mut r = -0.25;
        biquad.process_frame(&mut l, &mut r, &coeffs);
        assert!((l - 0.75).abs() < 1e-6);
        assert!((r + 0.25).abs() < 1e-6);
    }
}

use crate::audio::ring_stream::PidRingConsumer;

/// Target ring buffer fill level: L_target = 960 stereo frames (20 ms at 48 kHz)
pub const TARGET_FILL_FRAMES: f32 = 960.0;
pub const KP: f32 = 1.0e-6;
pub const KI: f32 = 1.0e-9;
pub const MAX_RATIO_DEVIATION: f32 = 0.002;

/// 4-point Hermite cubic interpolation at fractional offset `mu` in [0.0, 1.0).
#[inline(always)]
pub fn hermite4(y0: f32, y1: f32, y2: f32, y3: f32, mu: f32) -> f32 {
    let c0 = y1;
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * mu + c2) * mu + c1) * mu + c0
}

/// Adaptive Fractional Resampler with PI Clock-Drift Controller and 4-point Hermite Interpolation.
pub struct AdaptiveResampler {
    /// Sliding 4-frame stereo history window: hist[0] = y_{-1}, hist[1] = y_0, hist[2] = y_1, hist[3] = y_2
    hist: [[f32; 2]; 4],
    /// Fractional read phase in [0.0, 1.0)
    phase: f32,
    /// Accumulated error integral for PI controller (with anti-windup clamp)
    error_integral: f32,
    /// Last computed resampling ratio `r`
    current_ratio: f32,
    /// Whether the stream has primed enough initial frames
    primed: bool,
}

impl Default for AdaptiveResampler {
    fn default() -> Self {
        Self::new()
    }
}

impl AdaptiveResampler {
    pub const fn new() -> Self {
        Self {
            hist: [[0.0; 2]; 4],
            phase: 0.0,
            error_integral: 0.0,
            current_ratio: 1.0,
            primed: false,
        }
    }

    #[inline(always)]
    pub fn current_ratio(&self) -> f32 {
        self.current_ratio
    }

    /// Updates the PI controller from current buffer fill level `l_current` (in frames).
    /// e = L_current - L_target
    /// r = 1.0 + clamp(K_p * e + K_i * sum(e), -0.002, +0.002)
    #[inline]
    pub fn update_pi_ratio(&mut self, current_fill_frames: usize) -> f32 {
        let e = (current_fill_frames as f32) - TARGET_FILL_FRAMES;
        // Anti-windup clamp on integral accumulator
        self.error_integral = (self.error_integral + e).clamp(-1.5e6, 1.5e6);
        let delta = (KP * e + KI * self.error_integral).clamp(-MAX_RATIO_DEVIATION, MAX_RATIO_DEVIATION);
        self.current_ratio = 1.0 + delta;
        self.current_ratio
    }

    #[inline(always)]
    fn push_history_frame(&mut self, frame: [f32; 2]) {
        self.hist[0] = self.hist[1];
        self.hist[1] = self.hist[2];
        self.hist[2] = self.hist[3];
        self.hist[3] = frame;
    }

    /// Pulls and resamples `out_interleaved.len() / 2` stereo frames from `consumer` and
    /// accumulates (`+=`) them into `out_interleaved`.
    /// Real-time safe: zero allocations, zero locks.
    pub fn process_and_accumulate(
        &mut self,
        consumer: &mut PidRingConsumer,
        out_interleaved: &mut [f32],
    ) {
        let avail_frames = consumer.available_frames();
        let needed_frames = out_interleaved.len() / 2;

        // If ring buffer has severely overflowed (> 4096 frames ~85ms), drop down to target fill
        if avail_frames > 4096 {
            consumer.discard_frames(avail_frames - (TARGET_FILL_FRAMES as usize));
        }

        let avail_after = consumer.available_frames();
        if !self.primed {
            if avail_after < (TARGET_FILL_FRAMES as usize) / 2 {
                // Wait until half of target fill is buffered to avoid startup clicks
                return;
            }
            for _ in 0..4 {
                if let Some(f) = consumer.pop_stereo_frame() {
                    self.push_history_frame(f);
                }
            }
            self.primed = true;
        }

        if avail_after < 4 {
            // Stream went silent/paused: decay history smoothly to zero and unprime
            self.primed = false;
            self.error_integral = 0.0;
            self.current_ratio = 1.0;
            self.hist = [[0.0; 2]; 4];
            return;
        }

        let r = self.update_pi_ratio(avail_after);

        for out_frame in out_interleaved.chunks_exact_mut(2) {
            while self.phase >= 1.0 {
                self.phase -= 1.0;
                if let Some(next_frame) = consumer.pop_stereo_frame() {
                    self.push_history_frame(next_frame);
                } else {
                    self.push_history_frame([0.0, 0.0]);
                }
            }

            let mu = self.phase;
            let l = hermite4(
                self.hist[0][0],
                self.hist[1][0],
                self.hist[2][0],
                self.hist[3][0],
                mu,
            );
            let r_ch = hermite4(
                self.hist[0][1],
                self.hist[1][1],
                self.hist[2][1],
                self.hist[3][1],
                mu,
            );

            out_frame[0] += l;
            out_frame[1] += r_ch;

            self.phase += r;
        }

        let _ = needed_frames;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::ring_stream::create_pid_ring_stream;
    use crate::state::config::BusId;

    #[test]
    fn test_pi_controller_ratio_bounds() {
        let mut resampler = AdaptiveResampler::new();
        let r_high = resampler.update_pi_ratio(5000);
        assert!(r_high > 1.0 && r_high <= 1.0 + MAX_RATIO_DEVIATION + 1e-6);

        let mut resampler_low = AdaptiveResampler::new();
        let r_low = resampler_low.update_pi_ratio(100);
        assert!(r_low < 1.0 && r_low >= 1.0 - MAX_RATIO_DEVIATION - 1e-6);
    }

    #[test]
    fn test_hermite_resampler_dc_unity() {
        let (mut prod, mut cons) = create_pid_ring_stream(1234, BusId::Game);
        let input = vec![0.5_f32; 1200 * 2];
        prod.push_interleaved(&input);

        let mut resampler = AdaptiveResampler::new();
        let mut output = vec![0.0_f32; 480 * 2];
        resampler.process_and_accumulate(&mut cons, &mut output);

        for &sample in &output[16..] {
            assert!((sample - 0.5).abs() < 1e-4, "Expected ~0.5, got {sample}");
        }
    }
}

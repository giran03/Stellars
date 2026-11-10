//! Decides how often the meter stream pushes snapshots to the UI.
//!
//! The UI asks for a rate (full speed while focused, slower while unfocused). Independently, the
//! window tells us when it is minimized or hidden, which stops the stream entirely. The audio
//! engine is never affected: this only paces what is sent to the webview.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

pub const MAX_HZ: u32 = 60;

pub struct MeterPacing {
    requested_hz: AtomicU32,
    window_hidden: AtomicBool,
    /// When false ("save resources in the background" is off) the stream always runs at full rate.
    saving_enabled: AtomicBool,
}

impl Default for MeterPacing {
    fn default() -> Self {
        Self::new()
    }
}

impl MeterPacing {
    pub const fn new() -> Self {
        Self {
            requested_hz: AtomicU32::new(MAX_HZ),
            window_hidden: AtomicBool::new(false),
            saving_enabled: AtomicBool::new(true),
        }
    }

    pub fn set_requested_hz(&self, hz: u32) {
        self.requested_hz.store(hz.min(MAX_HZ), Ordering::Relaxed);
    }

    pub fn set_window_hidden(&self, hidden: bool) {
        self.window_hidden.store(hidden, Ordering::Relaxed);
    }

    pub fn set_saving_enabled(&self, enabled: bool) {
        self.saving_enabled.store(enabled, Ordering::Relaxed);
    }

    /// Snapshots per second to send right now; 0 means do not send.
    pub fn effective_hz(&self) -> u32 {
        if !self.saving_enabled.load(Ordering::Relaxed) {
            return MAX_HZ;
        }
        if self.window_hidden.load(Ordering::Relaxed) {
            return 0;
        }
        self.requested_hz.load(Ordering::Relaxed)
    }
}

/// Time between snapshots at `hz` (clamped to 1..=MAX_HZ).
pub fn interval_for(hz: u32) -> Duration {
    Duration::from_micros(1_000_000 / u64::from(hz.clamp(1, MAX_HZ)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_full_rate() {
        assert_eq!(MeterPacing::new().effective_hz(), MAX_HZ);
    }

    #[test]
    fn follows_the_requested_rate_and_clamps_it() {
        let pacing = MeterPacing::new();
        pacing.set_requested_hz(20);
        assert_eq!(pacing.effective_hz(), 20);
        pacing.set_requested_hz(500);
        assert_eq!(pacing.effective_hz(), MAX_HZ);
    }

    #[test]
    fn a_hidden_window_stops_the_stream() {
        let pacing = MeterPacing::new();
        pacing.set_window_hidden(true);
        assert_eq!(pacing.effective_hz(), 0);
        pacing.set_window_hidden(false);
        assert_eq!(pacing.effective_hz(), MAX_HZ);
    }

    #[test]
    fn turning_saving_off_ignores_hidden_and_requested_rate() {
        let pacing = MeterPacing::new();
        pacing.set_requested_hz(20);
        pacing.set_window_hidden(true);
        pacing.set_saving_enabled(false);
        assert_eq!(pacing.effective_hz(), MAX_HZ);
    }

    #[test]
    fn interval_matches_the_rate() {
        assert_eq!(interval_for(60), Duration::from_micros(16_666));
        assert_eq!(interval_for(20), Duration::from_micros(50_000));
        assert_eq!(interval_for(0), Duration::from_micros(1_000_000));
    }
}

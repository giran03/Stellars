//! DeepFilterNet3 voice denoiser.
//!
//! The network allocates while it runs, so it never executes on the audio thread. The audio
//! thread only copies samples through two lock-free rings to a worker thread and mixes the
//! result back. Until the worker has loaded the model (or if it cannot), the caller keeps using
//! the light RNNoise engine, so audio never stops.

use deep_filter_rt::tract::{DfParams, DfTract, RuntimeParams};
use ndarray::Array2;
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, Thread};
use std::time::Duration;

/// DeepFilterNet3 works on 10 ms hops of 48 kHz audio.
pub const HOP: usize = 480;
const CHANNELS: usize = 2;
/// Interleaved samples of slack between worker and audio thread (a 15 ms cushion).
const PRIME_SAMPLES: usize = 720 * CHANNELS;
/// The model delays its output by 30 ms (1440 samples) against its input.
const MODEL_DELAY_FRAMES: usize = 1440;
const RING_SAMPLES: usize = 24_000 * CHANNELS;
const IDLE_WAIT: Duration = Duration::from_millis(20);

/// Flags shared by the control thread, the audio thread and the worker.
#[derive(Default)]
pub struct DfShared {
    /// The user wants DeepFilterNet3 on this channel.
    pub active: AtomicBool,
    /// The worker has loaded the model and primed its output.
    ready: AtomicBool,
    /// The model could not be loaded; the RNNoise fallback stays in use.
    failed: AtomicBool,
    shutdown: AtomicBool,
    worker: OnceLock<Thread>,
}

impl DfShared {
    pub fn is_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

/// The worker's ends of the two rings, handed to the thread when it is first needed.
pub struct DfWorkerEnds {
    input: Consumer<f32>,
    output: Producer<f32>,
}

/// Audio-thread side. All buffers are allocated up front.
pub struct DfBridge {
    shared: Arc<DfShared>,
    input: Producer<f32>,
    output: Consumer<f32>,
    /// Circular delay line that keeps the dry signal aligned with the cleaned one.
    dry: Vec<f32>,
    dry_pos: usize,
    running: bool,
    /// Cleaned samples that were late and must be skipped when they finally arrive.
    owed: usize,
}

pub fn create(shared: Arc<DfShared>) -> (DfBridge, DfWorkerEnds) {
    let (input_tx, input_rx) = RingBuffer::<f32>::new(RING_SAMPLES);
    let (output_tx, output_rx) = RingBuffer::<f32>::new(RING_SAMPLES);
    let dry_len = (PRIME_SAMPLES / CHANNELS + MODEL_DELAY_FRAMES) * CHANNELS;
    (
        DfBridge {
            shared,
            input: input_tx,
            output: output_rx,
            dry: vec![0.0; dry_len],
            dry_pos: 0,
            running: false,
            owed: 0,
        },
        DfWorkerEnds { input: input_rx, output: output_tx },
    )
}

/// Starts the worker thread; called on the control thread when the engine is first selected.
pub fn spawn_worker(shared: &Arc<DfShared>, ends: DfWorkerEnds) {
    let worker_shared = Arc::clone(shared);
    match thread::Builder::new()
        .name("stellars-deepfilter".into())
        .spawn(move || run_worker(worker_shared, ends))
    {
        Ok(handle) => {
            let _ = shared.worker.set(handle.thread().clone());
        }
        Err(_) => shared.failed.store(true, Ordering::Relaxed),
    }
}

impl DfBridge {
    /// Cleans `buf` (interleaved stereo) in place. Returns `false` when the model is not
    /// available, leaving `buf` untouched so the caller can use another engine.
    pub fn process(&mut self, buf: &mut [f32], strength: f32) -> bool {
        let usable = self.shared.active.load(Ordering::Acquire) && self.shared.ready.load(Ordering::Acquire);
        if !usable {
            self.running = false;
            while self.output.pop().is_ok() {}
            return false;
        }
        if !self.running {
            self.running = true;
            self.owed = 0;
            self.dry.fill(0.0);
            self.dry_pos = 0;
            // Keep exactly the primed cushion; anything older is stale audio.
            for _ in 0..self.output.slots().saturating_sub(PRIME_SAMPLES) {
                let _ = self.output.pop();
            }
        }

        for &sample in buf.iter() {
            let _ = self.input.push(sample);
        }
        if let Some(worker) = self.shared.worker.get() {
            worker.unpark();
        }

        let dry_len = self.dry.len();
        for sample in buf.iter_mut() {
            let dry = std::mem::replace(&mut self.dry[self.dry_pos], *sample);
            self.dry_pos = if self.dry_pos + 1 == dry_len { 0 } else { self.dry_pos + 1 };
            while self.owed > 0 && self.output.pop().is_ok() {
                self.owed -= 1;
            }
            let wet = match self.output.pop() {
                Ok(value) => value,
                Err(_) => {
                    self.owed += 1;
                    dry
                }
            };
            *sample = dry * (1.0 - strength) + wet * strength;
        }
        true
    }
}

impl Drop for DfBridge {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.shared.worker.get() {
            worker.unpark();
        }
    }
}

fn load_model() -> Result<DfTract, String> {
    DfTract::new(DfParams::default(), &RuntimeParams::default_with_ch(CHANNELS)).map_err(|e| e.to_string())
}

fn run_worker(shared: Arc<DfShared>, mut ends: DfWorkerEnds) {
    let mut model: Option<DfTract> = None;
    let mut noisy = Array2::<f32>::zeros((CHANNELS, HOP));
    let mut enhanced = Array2::<f32>::zeros((CHANNELS, HOP));

    while !shared.shutdown.load(Ordering::Acquire) {
        if !shared.active.load(Ordering::Acquire) {
            if model.take().is_some() {
                shared.ready.store(false, Ordering::Release);
            }
            while ends.input.pop().is_ok() {}
            thread::park_timeout(IDLE_WAIT);
            continue;
        }

        if model.is_none() {
            while ends.input.pop().is_ok() {}
            match load_model() {
                Ok(loaded) => {
                    model = Some(loaded);
                    for _ in 0..PRIME_SAMPLES {
                        let _ = ends.output.push(0.0);
                    }
                    shared.failed.store(false, Ordering::Relaxed);
                    shared.ready.store(true, Ordering::Release);
                }
                Err(_) => {
                    shared.failed.store(true, Ordering::Relaxed);
                    thread::park_timeout(Duration::from_secs(2));
                }
            }
            continue;
        }

        if ends.input.slots() < HOP * CHANNELS {
            thread::park_timeout(IDLE_WAIT);
            continue;
        }
        for frame in 0..HOP {
            for ch in 0..CHANNELS {
                noisy[[ch, frame]] = ends.input.pop().unwrap_or(0.0);
            }
        }
        let Some(df) = model.as_mut() else { continue };
        if df.process(noisy.view(), enhanced.view_mut()).is_err() {
            // Keep the timeline intact: pass the audio through rather than leave a gap.
            enhanced.assign(&noisy);
        }
        for frame in 0..HOP {
            for ch in 0..CHANNELS {
                let value = enhanced[[ch, frame]];
                let _ = ends.output.push(if value.is_finite() { value } else { 0.0 });
            }
        }
    }
}

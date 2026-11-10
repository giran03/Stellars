use crate::state::config::BusId;
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;

/// Capacity: 16384 interleaved f32 samples = 8192 stereo frames (~170.6 ms at 48kHz)
pub const PID_RING_CAPACITY_SAMPLES: usize = 16384;

/// Shared lock-free routing & lifecycle state for a single captured PID stream.
pub struct PidStreamMeta {
    pub pid: u32,
    bus_index: AtomicU8,
    pub active: AtomicBool,
    /// Per-app volume (linear), applied by the render thread before the bus DSP.
    gain: AtomicU32,
}

impl PidStreamMeta {
    pub fn new(pid: u32, initial_bus: BusId) -> Arc<Self> {
        Arc::new(Self {
            pid,
            bus_index: AtomicU8::new(initial_bus.index() as u8),
            active: AtomicBool::new(true),
            gain: AtomicU32::new(1.0_f32.to_bits()),
        })
    }

    #[inline(always)]
    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain.load(Ordering::Relaxed))
    }

    #[inline]
    pub fn set_gain(&self, gain: f32) {
        self.gain.store(gain.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn bus(&self) -> BusId {
        match self.bus_index.load(Ordering::Relaxed) {
            0 => BusId::Game,
            1 => BusId::Chat,
            2 => BusId::Media,
            3 => BusId::Aux,
            _ => BusId::Aux,
        }
    }

    #[inline]
    pub fn set_bus(&self, bus: BusId) {
        self.bus_index.store(bus.index() as u8, Ordering::Relaxed);
    }

    #[inline]
    pub fn stop(&self) {
        self.active.store(false, Ordering::Release);
    }

    #[inline(always)]
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}

pub struct PidRingProducer {
    tx: Producer<f32>,
    pub meta: Arc<PidStreamMeta>,
}

impl PidRingProducer {
    /// Pushes interleaved stereo f32 samples into the lock-free SPSC ring buffer.
    /// Zero allocations, zero locks. Drops excess frames gracefully on overflow.
    #[inline]
    pub fn push_interleaved(&mut self, samples: &[f32]) -> usize {
        let avail = self.tx.slots();
        let to_write = samples.len().min(avail) & !1; // Ensure whole stereo pairs
        if to_write == 0 {
            return 0;
        }
        if let Ok( mut chunk) = self.tx.write_chunk_uninit(to_write) {
            let (first, second) = chunk.as_mut_slices();
            let first_len = first.len();
            for (dst, &src) in first.iter_mut().zip(&samples[..first_len]) {
                dst.write(src);
            }
            for (dst, &src) in second.iter_mut().zip(&samples[first_len..to_write]) {
                dst.write(src);
            }
            unsafe {
                chunk.commit_all();
            }
            to_write / 2
        } else {
            0
        }
    }
}

pub struct PidRingConsumer {
    pub rx: Consumer<f32>,
    pub meta: Arc<PidStreamMeta>,
}

impl PidRingConsumer {
    #[inline(always)]
    pub fn available_frames(&self) -> usize {
        self.rx.slots() / 2
    }

    #[inline(always)]
    pub fn pop_stereo_frame(&mut self) -> Option<[f32; 2]> {
        if self.rx.slots() < 2 {
            return None;
        }
        let l = self.rx.pop().ok()?;
        let r = self.rx.pop().ok()?;
        Some([l, r])
    }

    /// Discards excess frames if buffer accumulated too much latency.
    #[inline]
    pub fn discard_frames(&mut self, frames: usize) {
        let samples = (frames * 2).min(self.rx.slots() & !1);
        if let Ok(chunk) = self.rx.read_chunk(samples) {
            chunk.commit_all();
        }
    }
}

pub fn create_pid_ring_stream(pid: u32, initial_bus: BusId) -> (PidRingProducer, PidRingConsumer) {
    let (tx, rx) = RingBuffer::new(PID_RING_CAPACITY_SAMPLES);
    let meta = PidStreamMeta::new(pid, initial_bus);
    (
        PidRingProducer {
            tx,
            meta: Arc::clone(&meta),
        },
        PidRingConsumer { rx, meta },
    )
}

//! Coalesces rapid "please save" requests (fader drags, EQ edits) into a single write after a
//! short quiet period, so the disk is not rewritten hundreds of times per second.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

pub struct DebouncedSaver {
    tx: Sender<()>,
}

impl DebouncedSaver {
    /// `save` runs on a background thread once no request has arrived for `quiet`.
    pub fn spawn(quiet: Duration, save: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        let _ = thread::Builder::new().name("stellars-config-saver".into()).spawn(move || {
            // Block until something asks for a save, then wait for the burst to settle.
            while rx.recv().is_ok() {
                loop {
                    match rx.recv_timeout(quiet) {
                        Ok(()) => continue,
                        Err(RecvTimeoutError::Timeout) => {
                            save();
                            break;
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            save();
                            return;
                        }
                    }
                }
            }
        });
        Self { tx }
    }

    pub fn request(&self) {
        let _ = self.tx.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn a_burst_of_requests_causes_one_save() {
        let saves = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&saves);
        let saver = DebouncedSaver::spawn(Duration::from_millis(60), move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        for _ in 0..50 {
            saver.request();
            thread::sleep(Duration::from_millis(2));
        }
        thread::sleep(Duration::from_millis(250));
        assert_eq!(saves.load(Ordering::SeqCst), 1);
        saver.request();
        thread::sleep(Duration::from_millis(250));
        assert_eq!(saves.load(Ordering::SeqCst), 2, "a later change is saved again");
    }
}

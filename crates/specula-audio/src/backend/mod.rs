//! Output backends: WASAPI (Windows) and a null backend (no device, headless, other platforms).
//!
//! Each real-time backend owns a dedicated audio thread that owns the [`Mixer`]. The game thread
//! never touches the mixer; it sends commands through the ring in [`crate::mixer::MixerHandle`].

use crate::mixer::{BackendState, Mixer, SharedStats};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) mod null;
#[cfg(windows)]
pub(crate) mod com;
#[cfg(windows)]
pub(crate) mod wasapi;

/// Paces mixing to the wall clock when no device does: renders `block` frames, then sleeps until
/// that much time has passed. Used by the null backend and while a lost device reconnects, so
/// voices keep their timing (a 2 s sound started while unplugged is over 2 s later).
pub(crate) struct Clock {
    next: Instant,
    period: Duration,
    scratch: Vec<f32>,
}

impl Clock {
    pub fn new(rate: u32, block_ms: u32) -> Clock {
        let block = (rate as u64 * block_ms as u64 / 1000).max(1) as usize;
        Clock {
            next: Instant::now(),
            period: Duration::from_nanos(block as u64 * 1_000_000_000 / rate.max(1) as u64),
            scratch: vec![0.0; block * 2],
        }
    }

    /// Renders one block and waits for its deadline (or returns early if `stop` is set).
    pub fn tick(&mut self, mixer: &mut Mixer, stats: &SharedStats, stop: &AtomicBool) {
        mixer.render(&mut self.scratch);
        self.next += self.period;
        let now = Instant::now();
        if now > self.next + self.period * 10 {
            // Fell far behind (debugger, suspended laptop): resynchronise instead of bursting.
            stats.underruns.fetch_add(1, Ordering::Relaxed);
            self.next = now;
        }
        while !stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            if now >= self.next {
                break;
            }
            std::thread::sleep((self.next - now).min(Duration::from_millis(5)));
        }
    }
}

pub(crate) fn set_state(stats: &SharedStats, state: BackendState) {
    stats.backend_state.store(state.as_u32(), Ordering::Relaxed);
}

/// Handle to a running backend thread.
pub(crate) struct ThreadBackend {
    pub stop: Arc<AtomicBool>,
    pub join: Option<std::thread::JoinHandle<()>>,
}

impl ThreadBackend {
    /// Signals the thread and waits for it. Returns `false` if it panicked.
    pub fn shutdown(&mut self) -> bool {
        self.stop.store(true, Ordering::Relaxed);
        match self.join.take() {
            Some(j) => j.join().is_ok(),
            None => true,
        }
    }
}

impl Drop for ThreadBackend {
    fn drop(&mut self) {
        self.shutdown();
    }
}

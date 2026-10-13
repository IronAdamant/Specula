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

/// Lives on the audio thread's stack: if the thread unwinds from a panic, publishes `Stopped` and
/// the panic flag so callers see the failure instead of a silently dead "device" backend.
pub(crate) struct PanicGuard(pub std::sync::Arc<SharedStats>);

impl Drop for PanicGuard {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.thread_panicked.store(true, Ordering::Relaxed);
            set_state(&self.0, BackendState::Stopped);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A panicking audio thread publishes `Stopped` and the panic flag instead of leaving the
    /// stats claiming a live device.
    #[test]
    fn panic_guard_publishes_the_failure() {
        let stats = Arc::new(SharedStats::default());
        set_state(&stats, BackendState::Device);
        let s2 = stats.clone();
        let joined = std::thread::spawn(move || {
            let _guard = PanicGuard(s2);
            panic!("simulated audio thread panic (expected by this test)");
        })
        .join();
        assert!(joined.is_err());
        let st = crate::Stats::read(&stats);
        assert!(st.audio_thread_panicked);
        assert_eq!(st.backend, BackendState::Stopped);
        // A normal exit leaves the flag alone.
        let fresh = Arc::new(SharedStats::default());
        let s3 = fresh.clone();
        std::thread::spawn(move || drop(PanicGuard(s3))).join().unwrap();
        assert!(!fresh.thread_panicked.load(Ordering::Relaxed));
    }
}

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
        if self.advance(Instant::now()) {
            stats.underruns.fetch_add(1, Ordering::Relaxed);
        }
        while !stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            if now >= self.next {
                break;
            }
            std::thread::sleep((self.next - now).min(Duration::from_millis(5)));
        }
    }

    /// Moves the deadline on by one block, as of `now`; `true` when it had fallen more than ten
    /// blocks behind (debugger, suspended laptop) and was resynchronised to `now` instead of
    /// bursting: an underrun. A few blocks behind, it catches up block by block.
    fn advance(&mut self, now: Instant) -> bool {
        self.next += self.period;
        if now > self.next + self.period * 10 {
            self.next = now;
            return true;
        }
        false
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

    /// The null clock's pace, held by its own state and never by how fast this machine ran
    /// (what the null backend's test held from below until 2026-11-24, by the wall): a block's
    /// period is exact; a clock a few blocks behind catches up one block a tick, counting no
    /// underrun; one twenty blocks behind counts one and starts again from now.
    #[test]
    fn the_clock_keeps_its_period_catches_up_and_counts_an_underrun() {
        for (rate, ms) in [(48_000, 10), (44_100, 10), (48_000, 5)] {
            let c = Clock::new(rate, ms);
            assert_eq!(c.period, Duration::from_millis(ms as u64), "{rate} Hz, {ms} ms blocks");
            assert_eq!(c.scratch.len(), (rate * ms / 1000 * 2) as usize, "a stereo block's frames");
        }
        let mut c = Clock::new(48_000, 10);
        let (start, period) = (c.next, c.period);
        // Five blocks behind: each tick moves the deadline by exactly one block.
        let now = start + period * 5;
        for k in 1..=5 {
            assert!(!c.advance(now), "{k} blocks on, four or fewer behind: no underrun");
            assert_eq!(c.next, start + period * k);
        }
        // Now on time: the next deadline is one block ahead of now.
        assert!(!c.advance(now));
        assert_eq!(c.next, now + period);
        // Twenty blocks behind: one underrun, and the clock starts again from now.
        let late = c.next + period * 20;
        assert!(c.advance(late), "twenty blocks behind is an underrun");
        assert_eq!(c.next, late);
        assert!(!c.advance(late), "and the next tick is on time again");
        assert_eq!(c.next, late + period);
    }
}

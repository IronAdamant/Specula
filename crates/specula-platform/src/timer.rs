//! The system timer's resolution.
//!
//! Windows ends timed waits (`Receiver::recv_timeout`, `Condvar::wait_timeout`, `park_timeout`)
//! on its timer tick, 15.6 ms by default: a 2 ms wait takes up to 16 ms. A loop that serves
//! requests between frames needs its waits to end within about a millisecond, so while the engine's
//! window is open it asks for 1 ms (`timeBeginPeriod`, what games and media players do). Since
//! Windows 10 2004 that affects only the calling process, and Windows 11 ignores it while the
//! process's windows are minimized or hidden. Other platforms' timers are already fine.

/// Holds the system timer at 1 ms resolution until dropped (where the platform lets it).
#[derive(Debug)]
pub struct FineTimer {
    active: bool,
}

#[cfg(windows)]
mod imp {
    /// `TIMERR_NOERROR`.
    const NO_ERROR: u32 = 0;

    #[link(name = "winmm", kind = "raw-dylib")]
    extern "system" {
        fn timeBeginPeriod(period_ms: u32) -> u32;
        fn timeEndPeriod(period_ms: u32) -> u32;
    }

    pub fn begin() -> bool {
        // SAFETY: plain call with a constant argument; each success is matched by `end`.
        unsafe { timeBeginPeriod(1) == NO_ERROR }
    }

    pub fn end() {
        // SAFETY: matches a successful `timeBeginPeriod(1)`.
        unsafe {
            timeEndPeriod(1);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn begin() -> bool {
        false
    }

    pub fn end() {}
}

impl FineTimer {
    /// Asks for 1 ms timer resolution. Never fails: without it, timed waits are only coarser.
    pub fn request() -> FineTimer {
        FineTimer { active: imp::begin() }
    }

    /// Whether this guard raised the resolution (always false off Windows, where it isn't needed).
    pub fn active(&self) -> bool {
        self.active
    }
}

impl Drop for FineTimer {
    fn drop(&mut self) {
        if self.active {
            imp::end();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A copy of `specula_core::speed_budget` (this crate doesn't depend on it): a test's speed
    /// bound, tight (times the gate's load factor) when it runs alone, 5 s or more in every other run.
    fn speed_budget(tight: std::time::Duration) -> std::time::Duration {
        match std::env::var("SPECULA_SPEED_TESTS") {
            Ok(factor) => tight.mul_f64(factor.trim().parse::<f64>().ok().filter(|f| (1.0..=1000.0).contains(f)).unwrap_or(1.0)),
            Err(std::env::VarError::NotPresent) => tight.max(std::time::Duration::from_secs(5)),
            Err(std::env::VarError::NotUnicode(_)) => tight,
        }
    }

    #[test]
    fn a_short_timed_wait_ends_within_a_few_milliseconds() {
        let timer = FineTimer::request();
        assert_eq!(timer.active(), cfg!(windows));
        let (_tx, rx) = std::sync::mpsc::channel::<()>();
        // The best of several: a busy machine may delay any one wake-up. Under 12 ms still tells
        // a 1 ms timer from the 15.6 ms default.
        let best = (0..5)
            .map(|_| {
                let t = Instant::now();
                let _ = rx.recv_timeout(Duration::from_millis(2));
                t.elapsed()
            })
            .min()
            .unwrap();
        assert!(best >= Duration::from_millis(2) && best < speed_budget(Duration::from_millis(12)), "a 2 ms wait took {best:?}");
    }
}

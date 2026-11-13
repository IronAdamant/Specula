//! How long the calling thread has waited for locks it shares with worker threads.
//!
//! The engine thread and the workers share a few locks, the model library's above all (a
//! worker holds it while it notes a finished build). A stall whose thread ran for a fraction of
//! its time waited for something (`cpu_ms`), and this says whether that was such a lock: its
//! holder switched out on a busy machine, or holding it too long. Each thread counts its own
//! waits; the engine thread reads its count around every piece of work it reports
//! (`trace_engine`), and a stall carries the difference as `lock_ms`.

use std::cell::Cell;
use std::time::Duration;

thread_local! {
    /// Nanoseconds this thread spent getting the locks that are timed.
    static WAITED: Cell<u64> = const { Cell::new(0) };
}

/// The calling thread waited `took` for a lock (the time to get it, held or not).
pub fn waited(took: Duration) {
    WAITED.with(|w| w.set(w.get().saturating_add(took.as_nanos().min(u64::MAX as u128) as u64)));
}

/// What the calling thread has waited for timed locks so far, in milliseconds.
pub fn total_ms() -> f64 {
    WAITED.with(Cell::get) as f64 / 1e6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_thread_counts_its_own_waits() {
        std::thread::spawn(|| {
            assert_eq!(total_ms(), 0.0, "a new thread has waited for nothing");
            waited(Duration::from_micros(1500));
            waited(Duration::from_micros(500));
            assert!((total_ms() - 2.0).abs() < 1e-9, "{}", total_ms());
            // Another thread's waits are its own.
            std::thread::spawn(|| {
                waited(Duration::from_millis(40));
                assert!((total_ms() - 40.0).abs() < 1e-9);
            })
            .join()
            .unwrap();
            assert!((total_ms() - 2.0).abs() < 1e-9, "{}", total_ms());
        })
        .join()
        .unwrap();
    }
}

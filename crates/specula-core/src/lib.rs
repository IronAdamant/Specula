//! Specula core: std-only building blocks shared by every crate.
//!
//! Zero third-party crates (see `AGENTS.md`). Everything here is pure Rust and unit-tested.

// No unsafe code, except the counting allocator (`GlobalAlloc` is an unsafe trait).
#![deny(unsafe_code)]

#[allow(unsafe_code)]
pub mod alloc;
pub mod base64;
pub mod bytes;
pub mod checksum;
pub mod deflate;
pub mod dmath;
pub mod hash;
pub mod jpeg;
pub mod json;
pub mod math;
pub mod png;
pub mod profile;
pub mod rng;
pub mod toml;
pub mod trace;

pub use json::{Json, JsonError};

/// The bound a test puts on how long something took, when the speed is the point (a budget).
/// It is `tight` times a factor when the test runs alone (`SPECULA_SPEED_TESTS` is set: the
/// `speed` step of the gate, `tools/examen`, one test at a time), and at least 5 s in every other
/// run: a full test run has sixteen test threads and several engines on one GPU, where "under
/// 20 ms" holds most of the time, and a gate must not pass by luck (AGENTS.md, "The gate").
///
/// The variable's value is how many times slower the machine is now than at its best, as the
/// gate measured it with a reference workload just before the run (`tools/examen/src/mensura.rs`):
/// `1` on a quiet machine, `3.40` on one three and a half times slower. A number from 1 to 1000
/// scales the budget by it; anything else (below 1, not a number, past 1000) counts as 1, so a
/// value that is set always gives at least the tight bound. A bare bound shorter than 5 s on a
/// time fails `crates/specula/tests/clock_guard.rs`, which also keeps the copies of this
/// function (in crates that don't depend on this one) identical.
#[doc(hidden)]
pub fn speed_budget(tight: std::time::Duration) -> std::time::Duration {
    match std::env::var("SPECULA_SPEED_TESTS") {
        Ok(factor) => tight.mul_f64(factor.trim().parse::<f64>().ok().filter(|f| (1.0..=1000.0).contains(f)).unwrap_or(1.0)),
        Err(std::env::VarError::NotPresent) => tight.max(std::time::Duration::from_secs(5)),
        Err(std::env::VarError::NotUnicode(_)) => tight,
    }
}

/// How long a test waits for work it is owed (a model build, a GPU fence, a worker's answer)
/// before it calls the wait a hang and says what it was waiting for. It never decides a
/// result: a test waits on the thing itself (a join, a channel, the build's own completion),
/// and this bound only turns a hang into a failure that names itself, well inside the gate's
/// time limit for a step (45 minutes and more). Ten minutes: on a loaded machine a whole GPU
/// test has taken eight, never one wait of this length (2026-11-24). It is the one bound for
/// this, so that no test picks a short one of its own: 10, 20, 30 and 60 s stood before it,
/// and the 60 ran out once.
#[doc(hidden)]
pub const HANG_BOUND: std::time::Duration = std::time::Duration::from_secs(600);

/// Tests only: ends the test process when a wait with no bound of its own (a join on a GPU
/// pipeline build: a hung driver never returns) has not ended in [`HANG_BOUND`], saying what
/// it was waiting for, so that the hang fails by name and not by the gate's time limit for a
/// whole step. Armed before the wait, dropped after it. The message names the thread that armed
/// it (libtest names a test's thread after the test) and is written straight to standard
/// error, which the test harness does not capture: a panic's message would go to the arming
/// test's capture buffer (a spawned thread inherits it) and be lost when the process aborts,
/// and libtest names a test only when it ends. Then the process aborts: a panic on the
/// watchdog's thread alone would end only it, and the waiting thread would go on hanging.
#[doc(hidden)]
pub struct Watchdog {
    _disarm: std::sync::mpsc::Sender<()>,
}

impl Watchdog {
    /// Arms a watchdog for the wait named `what`, of [`HANG_BOUND`].
    pub fn arm(what: &str) -> Watchdog {
        Watchdog::arm_for(what, HANG_BOUND)
    }

    /// Arms a watchdog of `bound` (its own tests use a short one).
    pub fn arm_for(what: &str, bound: std::time::Duration) -> Watchdog {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let what = what.to_owned();
        let armed_by = std::thread::current().name().unwrap_or("an unnamed thread").to_owned();
        // (A watchdog that cannot start a thread watches nothing: the wait is as it was.)
        let _ = std::thread::Builder::new().name("specula-watchdog".into()).spawn(move || {
            if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(bound) {
                use std::io::Write as _;
                // The one console write of this crate's sources (print_guard allows it): a test
                // aid's last words before the abort, which no trace sink would live to show.
                let _ = writeln!(std::io::stderr(), "hung: {what} has not ended in {bound:?} (armed by {armed_by})");
                std::process::abort();
            }
        });
        Watchdog { _disarm: tx }
    }
}

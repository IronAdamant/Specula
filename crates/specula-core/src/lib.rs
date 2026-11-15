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
/// It is `tight` when the test runs alone (`SPECULA_SPEED_TESTS` is set: the `speed` step of
/// the gate, `tools/examen`, one test at a time), and at least 5 s in every other run: a full test run
/// has sixteen test threads and several engines on one GPU, where "under 20 ms" holds most of
/// the time, and a gate must not pass by luck (AGENTS.md, "The gate"). A bare bound shorter
/// than 5 s on a time fails `crates/specula/tests/clock_guard.rs`, which also keeps the copies
/// of this function (in crates that don't depend on this one) identical.
#[doc(hidden)]
pub fn speed_budget(tight: std::time::Duration) -> std::time::Duration {
    if std::env::var_os("SPECULA_SPEED_TESTS").is_some() {
        tight
    } else {
        tight.max(std::time::Duration::from_secs(5))
    }
}

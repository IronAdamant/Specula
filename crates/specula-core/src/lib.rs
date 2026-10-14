//! Specula core: std-only building blocks shared by every crate.
//!
//! Zero third-party crates (see `AGENTS.md`). Everything here is pure Rust and unit-tested.

pub mod base64;
pub mod checksum;
pub mod deflate;
pub mod dmath;
pub mod hash;
pub mod jpeg;
pub mod json;
pub mod math;
pub mod png;
pub mod rng;

pub use json::{Json, JsonError};

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

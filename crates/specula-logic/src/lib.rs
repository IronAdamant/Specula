//! Specula module host, the engine's side of `specula-sdk`:
//!
//! - [`loader`]: load a module library (from a versioned copy, so Cargo can rebuild the
//!   original while it runs), check its ABI table, and call it with byte messages; requests the
//!   module makes go to a [`loader::Handler`]. Panics never cross the boundary.
//! - [`build`]: run Cargo for a module, one build at a time, merging duplicate requests from a
//!   swarm ([`build::BuildService`]).
//! - [`diag`]: Cargo's JSON messages as data (diagnostics with spans and suggested fixes).
//! - [`fix`]: apply rustc's machine-applicable suggestions inside a module's folder.
//! - [`paths`]: compile-time paths (`#[path]`, `include_str!`...) must stay in a module's own folder.
//!
//! The engine (`specula::logic`) owns the world, the modules' state and the tick; this crate
//! knows nothing about either. All of the module host's `unsafe` lives in [`loader`].

pub mod build;
pub mod diag;
pub mod fix;
pub mod lint;
pub mod paths;
#[doc(hidden)]
pub mod fixtures;
pub mod loader;

pub use build::{BuildReport, BuildRequest, BuildService};
pub use loader::{CallOutcome, Handler, LoadError, ModuleCode};
pub use specula_sdk::{abi, Value};

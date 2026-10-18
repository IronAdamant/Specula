//! Lex (Latin "law"): gameplay rules as typed text data.
//!
//! Agents write a game's rules as a JSON document of items (archetypes with behaviours,
//! GAS-style effects, waves, commands, rules, outcomes...). This crate parses it ([`Document`]),
//! validates and compiles it with diagnostics that say exactly what to change ([`compile`],
//! [`Diagnostic`]), and runs it deterministically on a fixed tick ([`Sim`]): integers only,
//! entities in id order, named seeded RNG streams, so the same rules, seed and commands give the
//! same [`Sim::hash`] on every machine (foundation F2). Design: `docs/design/LEX.md`.
//!
//! ```text
//! let doc = specula_lex::Document::parse(text)?;
//! let compiled = specula_lex::compile(&doc, &Default::default())?;      // or the diagnostics
//! let mut sim = Sim::new(Arc::new(compiled.rules), seed, &Natives::new())?;
//! let place = sim.parse_command("place", &json)?;
//! sim.step(&[place]);
//! ```

#![forbid(unsafe_code)]
// Errors are the product here: they carry the path, expected, got and suggestion an agent acts
// on, and they're on the cold path, so their size doesn't matter.
#![allow(clippy::result_large_err)]

pub mod compile;
pub mod diag;
pub mod doc;
pub mod expr;
pub mod rules;
pub mod run;
pub mod schema;
pub mod sim;
pub mod units;

pub use compile::{compile, validate, Compiled, Options};
pub use diag::{Diagnostic, Severity};
pub use doc::{Document, FORMAT_VERSION};
pub use rules::Rules;
pub use sim::{CommandCall, Event, NativeArgs, NativeCtx, NativeHook, NativeHost, Natives, Rejection, Sim, Undo, Val, RESERVED_DETAILS};
pub use units::Unit;

/// Bumped when the runtime's semantics change in a way that changes results (like
/// `dmath::VERSION`): replays recorded with another version aren't expected to match.
///
/// 2 (2026-10-18): spawns that join a wave get its scale (summons, on_death and on_spawn spawns),
/// and the state hash covers phase order keys and wave groups' `with` values (only for rules that use them).
pub const RUNTIME_VERSION: u32 = 2;

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
pub use sim::{CommandCall, Event, NativeCtx, NativeHook, Natives, Rejection, Sim, Val};
pub use units::Unit;

/// Bumped when the runtime's semantics change in a way that changes results (like
/// `dmath::VERSION`): replays recorded with another version aren't expected to match.
pub const RUNTIME_VERSION: u32 = 1;

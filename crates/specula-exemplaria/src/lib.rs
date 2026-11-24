//! Specula Exemplaria: the library of sample assets (docs/design/EXEMPLARIA.md).
//!
//! Three layers, so that "a wolf" is never one model:
//!
//! - **A plan** ([`Plan`], Rust, in [`plans`]): a skeleton's bones, the table of numbers it takes
//!   (each with a unit, a default share of the size and how it grows with size), its traits, its
//!   roles, its options and its motions, and how a specimen of it is written.
//! - **A kind** ([`Kind`], data: `<name>.kind.json`): a plan with its numbers set, each with its
//!   `source` (where it was measured) or `own` (why the engine set it), and how far each may
//!   vary. A kind may name another kind of its plan as its `base` (one level).
//! - **A specimen** ([`Specimen`]): one seed of a kind, every varied number drawn
//!   ([`Resolved::draw`], the same bytes on every machine), written out as an ordinary recipe
//!   and anims file ([`expand`]) that the modeller builds and the anim library plays as any
//!   others.
//!
//! The look is the project's: a kind names **roles** (`coat`, `claw`), resolved at build to the
//! project style's presets ([`Roles`]; `specula_model::Style::role`). The library's own kinds are
//! the files under `exemplaria/` at the repository's root ([`library`]); a project takes a copy.
//!
//! Everything here is pure and bounded: a kind file is at most [`kind::MAX_KIND_BYTES`], every
//! table at most [`kind::MAX_ENTRIES`], and a specimen's writing is a few dozen bones and parts.

#![forbid(unsafe_code)]

pub mod draw;
pub mod error;
pub mod gaits;
pub mod kind;
pub mod library;
pub mod normalis;
pub mod plan;
pub mod plans;
pub mod words;
pub mod write;

pub use draw::Specimen;
pub use error::Error;
pub use kind::{Kind, Why};
pub use plan::{plan, resolve, resolve_among, Plan, Resolved};
pub use write::{expand, AsNamed, Expansion, Roles};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_build;
#[cfg(test)]
mod tests_table;

//! Specula Corpus (*corpus*: a body): the human body as numbers.
//!
//! An agent that models a figure describes it in numbers and cannot see whether an arm is too
//! long. This crate holds what a figure can be held to, written down from published
//! measurements of people, each number with its source and none from a model's memory
//! (docs/research/library/ has the research and its second checker's reports; the roadmap's
//! "Form within an agent's reach" says why).
//!
//! - [`ansur`]: 1,986 women and 4,082 men on 114 dimensions (93 measured, their age, 20 links
//!   made of those): how each is spread, how it follows stature and weight, and how it moves
//!   with fourteen named traits. Computed from the survey's public data by the repository's
//!   own tooling.
//! - [`body`]: a body from a sex, a stature, a weight and traits, and the frame it stands in.
//! - [`frame`]: where a figure's joints are; any figure's, in any pose.
//! - [`inertia`]: what each part weighs and where its weight sits, and so a posed figure's
//!   centre of mass.
//! - [`check`]: a figure's proportions against the people measured, and whether it stands.
//!
//! Pure data and pure functions, deterministic (only IEEE basic operations and square roots),
//! so a body is the same body on every machine.
//!
//! **What is not here yet**: a body under 17 or over 58, a face, fingers and toes, the range
//! each joint turns through, and the surface (girths are in the rows; the mesh that wears
//! them is the modelling crate's).
#![forbid(unsafe_code)]

pub mod ansur;
mod ansur_data;
pub mod body;
pub mod check;
pub mod frame;
pub mod inertia;

pub use ansur::{Row, Sex};
pub use body::{Body, BodyError};
pub use check::{balance, proportions, Balance, Proportion};
pub use frame::{Frame, Joint, Side};
pub use inertia::{centre_of_mass, parts, Part};

//! The balance lab (docs/design/BALANCE.md): designer targets as data, headless sweeps of Lex
//! params, and a seeded Bayesian optimiser that tunes params toward the targets.
//!
//! - **Targets** ([`targets`]): a validated JSON document of what the game should feel like in
//!   numbers (win rate, win rate per wave, leaks per wave, the dust curve, time to kill, how
//!   long waves take, any Lex expression at the end), each with a tolerance and a weight, plus
//!   the scripted **policy** that plays (a build order as data) and the seeds. Errors are Lex
//!   [`specula_lex::Diagnostic`]s: the target, the field, what was expected, a suggestion.
//! - **Runs** ([`run`]): one rule set, one seed, the policy playing it; the metrics as numbers.
//! - **Sweeps** ([`sweep`]): many param points × many seeds across threads. Every run is
//!   independent and results are gathered by index, so the numbers don't depend on the thread
//!   count or timing (F2); a wall-clock budget stops a sweep between runs and says where.
//! - **Tuning** ([`tune`]): Bayesian optimisation over chosen `param` items: a Gaussian process
//!   with a fixed squared-exponential kernel on the normalised params, expected improvement
//!   over a seeded candidate set, integer (stored-unit) points. Resumable: its whole state is
//!   data. The best point is checked on held-out seeds it never saw.
//!
//! **Simulated balance can mislead** (agents don't play like people; the policy is a
//! heuristic). Every result says so: `needs_human_playtest: true`.
//!
//! The lab knows nothing about any one game: it reads a rule set's params, resources, waves,
//! archetypes and commands from the compiled rules, so it works on any Lex rule set.

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

pub mod gp;
pub mod run;
pub mod sweep;
pub mod targets;
pub mod tune;

pub use run::{RunResult, Runner};
pub use sweep::{sweep, Point, PointResult, SweepOptions, SweepResult};
pub use targets::{Metric, Target, Targets};
pub use tune::{TuneJob, TuneOptions, TuneParam};

/// What every result carries: simulated balance is a hypothesis for people to test.
pub const PLAYTEST_NOTE: &str = "simulated with a scripted policy, not people: confirm in a human playtest before shipping a change";

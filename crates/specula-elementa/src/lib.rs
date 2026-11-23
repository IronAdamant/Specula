//! Specula Elementa: fire that spreads, cold that freezes, wet that resists, wind that leans the
//! fire, as deterministic simulation (docs/design/EFFECTUS.md, "Cosmetic and behaviour").
//!
//! - **Nodes** ([`Node`]): the things tracked, each an entity's id, a material and a box, with
//!   its state: `heat` (−1 frozen solid .. 0 ambient .. 1 white hot), `fuel` (seconds of burning
//!   left), `wet` (0..1) and a [`Phase`] (intact, burning, burnt, frozen).
//! - **Materials** ([`Material`], [`Config`]): what burns, how hot and how long, what cools,
//!   wets and freezes; ten built in, a project's `elements.json` overriding and adding
//!   ([`Config::parse`], every number range-checked, every fault an [`ElementaError`]).
//! - **The step** ([`Elementa::step`]): every tick; every `tick_hz / spread_hz` ticks a spread
//!   step heats the neighbours of burning things (falling off over the reach, leaning downwind and
//!   upward), applies wetness first, changes phases, burns fuel down, cools and dries, and queues
//!   [`Event`]s in a fixed order.
//! - **Acts** ([`Elementa::act`]): ignite, chill and wet, on a node or a sphere, at once.
//! - **Near** is a sorted grid ([`grid`]): no hashing, every walk in a fixed order.
//! - **Bounded**: nodes tracked, nodes burning and work per step are limits ([`Limits`]),
//!   checked when given and said before a run ([`Config::worst_case`]); a step never passes its
//!   work limit (it resumes where it stopped at the next step; heat it could not give is not
//!   given, so a limit only slows a fire); under the burning limit the longest waiting catch
//!   first. Only a spread step updates the grid, by a merge of what changed.
//! - **Predicted** ([`Elementa::check`]): a scenario run on a copy, bounded, for a worker thread.
//! - **Saved and hashed** ([`Elementa::save_state`], [`Elementa::restore_state`],
//!   [`Elementa::hash`]): a load continues bit for bit; a damaged section is an error.
//!
//! **Determinism (F2):** only IEEE `+ − × ÷`, `sqrt`, `min`/`max`/`abs` and
//! `specula_core::dmath`; ordered containers; embers drawn from a hash of the run's
//! seeded key, the node's id and the tick (never in the order the budget reached
//! nodes). Nothing reads a clock or spawns a thread.
//!
//! **What it does not do:** it writes nothing in the world (it tells the rules); blasts, strikes,
//! wind zones and pushes are later stages (9 and 10).

#![forbid(unsafe_code)]

pub mod check;
pub mod config;
pub mod error;
pub mod grid;
pub mod save;
pub mod sim;

pub use check::{Prediction, Scenario, MAX_CHECK_OPS, MAX_CHECK_TICKS};
pub use config::{built_in, Config, Limits, Material, WorstCase};
pub use error::ElementaError;
pub use save::SECTION_VERSION;
pub use sim::{
    Act, ActKind, ActReport, AddReport, Cause, Cursor, Elementa, Event, EventKind, Events, Node, NodeDesc, Phase, StepReport, Target,
};

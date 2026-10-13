//! *Sepulcra*: a tower defence on a shattered slab, as a deterministic headless simulation.
//!
//! This crate is the whole rules engine with no rendering: the slab ([`map`]), the content
//! files ([`content`]), pathing ([`path`]), the tick ([`sim`]) and replay tools ([`script`]).
//! A presentation layer reads [`State`] each tick (positions via `world_pos_f32`), feeds player
//! input as [`Command`]s and turns [`Event`]s into visuals and sound. Balance swarms run it
//! headlessly (`examples/headless.rs`).
//!
//! **Determinism (foundation F2).** Fixed 30 Hz tick; the tick number is the only clock. No
//! floats in simulation state: millimetres, milli-HP and permille ([`fixed`]). Randomness only
//! from named, seeded [`specula_core::rng::Rng`] streams kept in the state (`waves`,
//! `lane_jump`, `drops`). Ordered containers and lowest-id tie-breaks everywhere (`HashMap` and
//! `HashSet` are banned by `clippy.toml`). [`State::hash`] covers the full state, so the same
//! seed and commands give the same hash on every tick and every machine.
//!
//! ```text
//! let content = Arc::new(Content::builtin());
//! let mut state = State::new(content.clone(), 1);
//! let cell = content.map.cell_ref("top:12,9").unwrap();
//! let warding = content.stone_kind("warding").unwrap();
//! let events = sepulcra::step(&mut state, &[Command::PlaceStone { cell, stone: warding }]);
//! println!("{}", state.ascii());
//! ```

pub mod content;
pub mod error;
pub mod fixed;
pub mod map;
pub mod path;
pub mod script;
pub mod sim;

pub use content::{Content, EnemyKind, StoneKind};
pub use error::{DataError, ErrorCode};
pub use fixed::{Pos, TICK_HZ};
pub use map::{CellId, CellKind, Face, Map};
pub use script::{run, Player, RunReport, Script};
pub use sim::{step, Command, Enemy, Event, Motion, Outcome, Projectile, Rejection, State};

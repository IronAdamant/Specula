//! *Sepulcra* in the Specula engine: the deterministic simulation (`sepulcra`) hosted in-process,
//! drawn into the shared world, played with mouse and keyboard, and fully playable by agents over
//! MCP (`play_*` tools).
//!
//! ```text
//! cargo run -p sepulcra-app --release -- --port 7486 -- --seed 7     # window + MCP
//! cargo run -p sepulcra-app --release -- --headless --port 7486
//! ```
//!
//! The game's own arguments go after `--`: `--seed N` (the simulation's seed, else
//! `SEPULCRA_SEED`, else 1) and `--mute` (no audio device). Assets (recipes, anim sets, sounds,
//! screens) come from `--assets DIR`, else `assets/` next to the executable, else the source
//! tree's `games/sepulcra/assets`.
//!
//! Modules: [`game`] (the [`specula::game::Game`] host), [`scene`] (state → world), [`slab`]
//! (the level geometry and picking), [`looks`] (which recipe and clip draws each thing),
//! [`controls`] (all input), [`hud`] (the `specula-ui` HUD on the GPU overlay), [`audio`] (sound),
//! [`tools`] (agents).

pub mod audio;
pub mod controls;
mod ents;
pub mod game;
pub mod hud;
pub mod looks;
pub mod scene;
pub mod slab;
pub mod tools;

pub use game::SepulcraGame;

use sepulcra::Content;
use std::sync::Arc;

/// Runs the game in the engine with the engine's command-line flags. The seed is `--seed` (after
/// `--`), else `SEPULCRA_SEED`, else 1.
pub fn run() -> Result<(), String> {
    let seed = match std::env::var("SEPULCRA_SEED") {
        Ok(s) => s.trim().parse::<u64>().map_err(|_| format!("SEPULCRA_SEED must be a u64, got {s:?}"))?,
        Err(_) => 1,
    };
    specula::run_game(Box::new(SepulcraGame::new(Arc::new(Content::builtin()), seed)))
}

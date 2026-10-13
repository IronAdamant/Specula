//! *Sepulcra* in the Specula engine: the deterministic simulation (`sepulcra`) hosted in-process,
//! drawn into the shared world, played with mouse and keyboard, and fully playable by agents over
//! MCP (`play_*` tools).
//!
//! ```text
//! cargo run -p sepulcra-app --release -- --port 7486          # window + MCP
//! cargo run -p sepulcra-app --release -- --headless --port 7486
//! ```
//!
//! Modules: [`game`] (the [`specula::game::Game`] host), [`scene`] (state → world), [`slab`]
//! (the level geometry and picking), [`looks`] (one model function per kind of thing: the seam
//! for real assets), [`controls`] (all input), [`hud`] (the title-bar HUD), [`tools`] (agents).

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

/// Runs the game in the engine with the engine's command-line flags. The seed comes from
/// `SEPULCRA_SEED` (the engine owns the command line), else 1.
pub fn run() -> Result<(), String> {
    let seed = match std::env::var("SEPULCRA_SEED") {
        Ok(s) => s.trim().parse::<u64>().map_err(|_| format!("SEPULCRA_SEED must be a u64, got {s:?}"))?,
        Err(_) => 1,
    };
    specula::run_game(Box::new(SepulcraGame::new(Arc::new(Content::builtin()), seed)))
}

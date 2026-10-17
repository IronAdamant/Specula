//! Save games for Sepulcra (docs/design/SAVES.md): the game's part of a save, and the pause
//! menu's Save and Load.
//!
//! The game's section holds what the engine can't see: the simulation [`State`] (its own
//! versioned codec in `sepulcra::sim`), the run's recording (every applied command and its
//! tick), the build order being followed, and the game's own pause and speed (they decide how
//! many simulation steps a tick takes). The world, the clock and physics are the engine's part.
//! Presentation is rebuilt from the state, adopting the entities the saved world holds.
//!
//! Players save and load from the pause menu (Escape or the pad's Start, then Save game / Load
//! game: menus work with a pad, a mouse, touch or keys, F6), into one slot,
//! [`PLAYER_SLOT`]. Agents use `save_write` / `save_read` with any name, or press the same
//! buttons with `play_ui`.

use super::*;
use sepulcra::script::{Recording, Script};
use specula::game::GameRequest;
use specula_core::bytes::{ByteReader, ByteWriter, DecodeError};

/// The game section's own layout version.
pub const SECTION_VERSION: u32 = 1;
/// The save the pause menu writes and reads.
pub const PLAYER_SLOT: &str = "sepulcra_slot_1";

/// A parsed game section, waiting for [`SepulcraGame::apply_saved`].
pub(crate) struct Loaded {
    state: State,
    journal: Journal,
    autoplay: Option<(String, Player)>,
    paused: bool,
    speed: u32,
    pres_ticks: u64,
}

fn bad(e: DecodeError) -> String {
    format!("the game section is damaged: {e}")
}

impl SepulcraGame {
    /// The game's section of a save.
    pub(crate) fn save_bytes(&self) -> Vec<u8> {
        let content = self.state.content();
        let mut w = ByteWriter::new();
        w.u32(SECTION_VERSION);
        w.blob(&self.state.to_bytes());
        // The run's recording, in the format play_record writes and replays read.
        let rec = Recording { seed: self.state.seed(), content_hash: content.hash, journal: self.journal.clone(), end: None };
        w.str(&rec.to_json(content).to_string());
        match &self.autoplay {
            Some((name, player)) => {
                w.bool(true);
                w.str(name);
                w.str(&player.script().to_json(content).to_string());
                w.u64(player.position() as u64);
            }
            None => w.bool(false),
        }
        w.bool(self.paused);
        w.u32(self.speed);
        w.u64(self.pres_ticks);
        w.into_bytes()
    }

    /// Reads [`SepulcraGame::save_bytes`] against this game's content, changing nothing.
    pub(crate) fn parse_saved(&self, data: &[u8]) -> Result<Loaded, String> {
        let content = self.content();
        let mut r = ByteReader::new(data);
        let version = r.u32("version").map_err(bad)?;
        if version != SECTION_VERSION {
            return Err(format!("game section version {version}; this build reads {SECTION_VERSION}"));
        }
        let state = State::from_bytes(Arc::clone(&content), r.blob("state", 64 << 20).map_err(bad)?).map_err(|e| e.to_string())?;
        let rec = r.str("recording", 64 << 20).map_err(bad)?;
        let journal = Recording::from_json(&rec, &content, "save").map_err(|e| format!("the run's recording: {e}"))?.journal;
        let autoplay = if r.bool("autoplay").map_err(bad)? {
            let name = r.str("autoplay.name", 256).map_err(bad)?;
            let text = r.str("autoplay.script", 1 << 20).map_err(bad)?;
            let script = Script::parse(&text, &content, "save").map_err(|e| format!("the build order being followed: {e}"))?;
            let at = r.u64("autoplay.next").map_err(bad)? as usize;
            Some((name, Player::resume(script, at)))
        } else {
            None
        };
        let paused = r.bool("paused").map_err(bad)?;
        let speed = r.u32("speed").map_err(bad)?;
        if !SPEEDS.contains(&speed) {
            return Err(format!("speed {speed} isn't one of {SPEEDS:?}"));
        }
        let pres_ticks = r.u64("pres_ticks").map_err(bad)?;
        r.finish().map_err(bad)?;
        Ok(Loaded { state, journal, autoplay, paused, speed, pres_ticks })
    }

    /// Takes a parsed section over the loaded world: the state, then the scene adopting what
    /// the world holds (rebuilt only if it can't be adopted).
    pub(crate) fn apply_saved(&mut self, world: &mut World, agent: &str, l: Loaded) {
        self.state = l.state;
        self.journal = l.journal;
        self.autoplay = l.autoplay;
        self.paused = l.paused;
        self.speed = l.speed;
        self.pres_ticks = l.pres_ticks;
        self.now = self.pres_ticks as f32 / TICK_HZ as f32;
        self.events.clear();
        self.pending.clear();
        self.inspect = None;
        self.radial_cell = None;
        self.confirm = None;
        self.hover_key = None;
        self.sfx.reset();
        for screen in ["victory", "defeat", "radial", "confirm"] {
            self.hud.show(screen, false);
        }
        match self.state.outcome() {
            Outcome::Won => self.hud.show("victory", true),
            Outcome::Lost => self.hud.show("defeat", true),
            Outcome::Running => {}
        }
        let mut scene = match Scene::adopt(world, agent, &self.state) {
            Ok(s) => s,
            Err(why) => {
                eprintln!("sepulcra: the loaded world's scene couldn't be adopted ({why}); rebuilding it");
                Scene::build(world, agent, &self.state)
            }
        };
        scene.set_pace(world, self.pace());
        scene.sync(world, &self.state, &[], self.now);
        scene.animate(world, 1.0, self.now);
        self.scene = Some(scene);
    }

    /// The pause menu's Save game and Load game.
    pub(crate) fn request(&mut self, req: GameRequest) {
        self.requests.push(req);
    }

    /// How a menu request went, for the player.
    pub(crate) fn request_result(&mut self, world: &mut World, req: &GameRequest, result: Result<Json, Json>) {
        let msg = match (req, &result) {
            (GameRequest::Save { .. }, Ok(j)) => format!("Game saved (tick {})", j.get("tick").and_then(Json::as_u64).unwrap_or(0)),
            (GameRequest::Load { .. }, Ok(_)) => format!("Game loaded: wave {} of {}", self.state.wave(), self.state.waves_total()),
            (GameRequest::Load { .. }, Err(e)) if e.get("error").and_then(Json::as_str) == Some("not_found") => "No saved game yet".into(),
            (_, Err(e)) => format!("Couldn't {}: {}", if matches!(req, GameRequest::Save { .. }) { "save" } else { "load" }, e.get("message").and_then(Json::as_str).unwrap_or("unknown error")),
            (GameRequest::Delete { .. }, Ok(_)) => "Save deleted".into(),
            (GameRequest::List, Ok(_)) => return,
        };
        let failed = result.is_err();
        self.say(msg);
        self.sfx.ui(if failed { "ui_error" } else { "ui_click" }, self.now);
        if !failed && matches!(req, GameRequest::Load { .. }) && self.state.outcome() == Outcome::Running {
            // Loaded from the menu: paused under it, so the player looks before it runs.
            self.set_paused(world, true);
            self.hud.show("pause", true);
        }
    }
}

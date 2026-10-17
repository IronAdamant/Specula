//! Save games for Sepulcra (docs/design/SAVES.md): the game's part of a save, and the pause
//! menu's Save and Load.
//!
//! The game's section holds what the engine can't see: the simulation [`State`] (its own
//! versioned codec in `sepulcra::sim`), the run's recording (every applied command and its
//! tick), the build order being followed, and the game's own pause and speed (they decide how
//! many simulation steps a tick takes). The world, the clock and physics are the engine's part.
//! Presentation is rebuilt from the state, adopting the entities the saved world holds.
//!
//! Players save and load from the pause menu's Save / Load screen (Escape or the pad's Start:
//! menus work with a pad, a mouse, touch or keys, F6): three slots (`sepulcra_slot_1..3`) with
//! what each holds (wave, tombstones, time played, when), saving over, loading and deleting each
//! asking first, plus two autosaves (`sepulcra_autosave_1..2`) written in turn as each wave
//! starts (on by default with a window, off headless; Settings switches it). Saves and loads run
//! in the background: the screen says "Saving..." and a toast says how it went. Agents use
//! `save_write` / `save_read` with any name, or press the same buttons with `play_ui`.

use super::*;
use sepulcra::script::{Recording, Script};
use specula::game::GameRequest;
use specula_core::bytes::{ByteReader, ByteWriter, DecodeError};

/// The game section's own layout version.
pub const SECTION_VERSION: u32 = 1;
/// The player's save slots and autosaves on disk: `<prefix><n>`.
pub const PLAYER_PREFIX: &str = "sepulcra_slot_";
pub const AUTO_PREFIX: &str = "sepulcra_autosave_";
pub const PLAYER_SLOTS: u8 = 3;
pub const AUTO_SLOTS: u8 = 2;

/// A slot on the saves screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Player(u8),
    Auto(u8),
}

/// A used slot, as the saves screen shows it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SlotInfo {
    pub slot: Slot,
    /// What it holds (its label: wave and tombstones).
    pub title: String,
    /// Time played and when it was saved.
    pub detail: String,
}

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

    /// A request for the engine (served after the frame, a game tool or the tick).
    pub(crate) fn request(&mut self, req: GameRequest) {
        self.requests.push(req);
    }

    /// The saves screen: opened from the pause menu; asks for the list of saves.
    pub(crate) fn open_saves(&mut self) {
        // In place of the pause menu (its title would show through), which Back brings back.
        self.hud.show("pause", false);
        self.hud.show("saves", true);
        self.request(GameRequest::List);
    }

    /// Back from the saves screen: to the pause menu.
    pub(crate) fn close_saves(&mut self) {
        self.hud.show("saves", false);
        if self.paused && self.state.outcome() == Outcome::Running {
            self.hud.show("pause", true);
        }
    }

    /// A slot's name on disk.
    pub(crate) fn slot_name(slot: Slot) -> String {
        match slot {
            Slot::Player(n) => format!("{PLAYER_PREFIX}{n}"),
            Slot::Auto(n) => format!("{AUTO_PREFIX}{n}"),
        }
    }

    /// What the save's label says (shown in the load screen).
    fn save_label(&self) -> String {
        let s = &self.state;
        format!("Wave {}/{}, {}/{} tombstones", s.wave(), s.waves_total(), s.tombstones().max(0), s.content().rules.tombstones)
    }

    /// Saves to a slot (the engine writes it in the background; a toast says how it went).
    pub(crate) fn save_to(&mut self, slot: Slot) {
        self.saves_status = Some("Saving...".into());
        let label = match slot {
            Slot::Auto(_) => format!("Autosave, {}", self.save_label()),
            Slot::Player(_) => self.save_label(),
        };
        self.request(GameRequest::Save { slot: Self::slot_name(slot), label: Some(label) });
    }

    pub(crate) fn load_from(&mut self, slot: Slot) {
        self.saves_status = Some("Loading...".into());
        self.request(GameRequest::Load { slot: Self::slot_name(slot) });
    }

    /// Starting a wave: the rotating autosave, when it's on.
    pub(crate) fn autosave(&mut self, wave: u16) {
        if self.autosave_on {
            self.save_to(Slot::Auto(if wave % 2 == 1 { 1 } else { 2 }));
        }
    }

    /// The saves screen's buttons (`slot` from the action's `:N`): saving over a used slot,
    /// loading (the run in play is lost) and deleting ask first.
    pub(crate) fn saves_action(&mut self, action: &str, arg: u8) {
        let used = |list: &[SlotInfo], slot: Slot| list.iter().any(|s| s.slot == slot);
        match action {
            "save_slot" if (1..=PLAYER_SLOTS).contains(&arg) => {
                if used(&self.slots, Slot::Player(arg)) {
                    self.ask(Confirm::Overwrite(arg));
                } else {
                    self.save_to(Slot::Player(arg));
                }
            }
            "load_slot" if (1..=PLAYER_SLOTS).contains(&arg) && used(&self.slots, Slot::Player(arg)) => self.ask(Confirm::Load(Slot::Player(arg))),
            "load_auto" if (1..=AUTO_SLOTS).contains(&arg) && used(&self.slots, Slot::Auto(arg)) => self.ask(Confirm::Load(Slot::Auto(arg))),
            "delete_slot" if (1..=PLAYER_SLOTS).contains(&arg) && used(&self.slots, Slot::Player(arg)) => self.ask(Confirm::Delete(arg)),
            _ => {}
        }
    }

    fn ask(&mut self, c: Confirm) {
        self.confirm = Some(c);
        self.hud.show("confirm", true);
    }

    /// A confirmed save-screen question.
    pub(crate) fn confirmed(&mut self, c: Confirm) {
        match c {
            Confirm::Overwrite(n) => self.save_to(Slot::Player(n)),
            Confirm::Load(slot) => self.load_from(slot),
            Confirm::Delete(n) => self.request(GameRequest::Delete { slot: Self::slot_name(Slot::Player(n)) }),
            Confirm::Restart => {}
        }
    }

    /// The confirm dialog's words for a save-screen question.
    pub(crate) fn confirm_text(&self, c: Confirm) -> Option<(String, String)> {
        let what = |slot: Slot| self.slots.iter().find(|s| s.slot == slot).map_or_else(|| "this save".into(), |s| s.title.clone());
        Some(match c {
            Confirm::Overwrite(n) => (format!("Save over slot {n}?"), format!("{} will be replaced by this run.", what(Slot::Player(n)))),
            Confirm::Load(slot) => ("Load this save?".into(), format!("{}. The run in play is lost unless you saved it.", what(slot))),
            Confirm::Delete(n) => (format!("Delete slot {n}?"), format!("{} will be gone for good.", what(Slot::Player(n)))),
            Confirm::Restart => return None,
        })
    }

    /// The saves screen's state (every frame while it's open; cheap).
    pub(crate) fn fill_saves(&mut self) {
        let s = &mut self.hud.state;
        if let Some(t) = &self.saves_status {
            s.set_str("saves_status", t);
        } else {
            s.remove("saves_status");
        }
        let rows = (1..=PLAYER_SLOTS).map(|n| ("slot", Slot::Player(n))).chain((1..=AUTO_SLOTS).map(|n| ("auto", Slot::Auto(n))));
        for (prefix, slot) in rows {
            let n = match slot {
                Slot::Player(n) | Slot::Auto(n) => n,
            };
            let info = self.slots.iter().find(|i| i.slot == slot);
            let name = if prefix == "slot" { format!("Slot {n}") } else { format!("Autosave {n}") };
            let (what, detail) = match info {
                Some(i) => (i.title.clone(), i.detail.clone()),
                None => ("Empty".into(), String::new()),
            };
            s.set_str(&format!("{prefix}{n}_name"), &name)
                .set_str(&format!("{prefix}{n}_title"), &what)
                .set_str(&format!("{prefix}{n}_detail"), &detail)
                .set_bool(&format!("{prefix}{n}_used"), info.is_some());
        }
    }

    /// How a request went, for the player.
    pub(crate) fn request_result(&mut self, world: &mut World, req: &GameRequest, result: Result<Json, Json>) {
        if let (GameRequest::List, Ok(list)) = (req, &result) {
            self.slots = slots_of(list);
            return;
        }
        self.saves_status = None;
        let autosave = matches!(req, GameRequest::Save { slot, .. } if slot.starts_with(AUTO_PREFIX));
        let error = |e: &Json| e.get("message").and_then(Json::as_str).unwrap_or("unknown error").chars().take(160).collect::<String>();
        let msg = match (req, &result) {
            (GameRequest::Save { .. }, Ok(_)) if autosave => "Autosaved".into(),
            (GameRequest::Save { .. }, Ok(_)) => "Game saved".into(),
            (GameRequest::Load { .. }, Ok(_)) => format!("Game loaded: wave {} of {}", self.state.wave(), self.state.waves_total()),
            (GameRequest::Delete { .. }, Ok(_)) => "Save deleted".into(),
            (GameRequest::Load { .. }, Err(e)) if e.get("error").and_then(Json::as_str) == Some("not_found") => "That save is gone".into(),
            (GameRequest::Save { .. }, Err(e)) if autosave => format!("Autosave skipped: {}", error(e)),
            (GameRequest::Save { .. }, Err(e)) => format!("Couldn't save: {}", error(e)),
            (GameRequest::Load { .. }, Err(e)) => format!("Couldn't load: {}", error(e)),
            (GameRequest::Delete { .. }, Err(e)) => format!("Couldn't delete: {}", error(e)),
            (GameRequest::List, _) => return,
        };
        let failed = result.is_err();
        self.say(msg);
        if !autosave || failed {
            self.sfx.ui(if failed { "ui_error" } else { "ui_click" }, self.now);
        }
        // The list changed: ask again (the screen shows it when it's open).
        if !failed && !matches!(req, GameRequest::Load { .. }) {
            self.request(GameRequest::List);
        }
        if !failed && matches!(req, GameRequest::Load { .. }) {
            self.hud.show("saves", false);
            if self.state.outcome() == Outcome::Running {
                // Loaded from the menu: paused under it, so the player looks before it runs.
                self.set_paused(world, true);
                self.hud.show("pause", true);
            }
            self.request(GameRequest::List);
        }
    }
}

/// The game's slots in a `save_list` answer, with what the load screen says about each.
fn slots_of(list: &Json) -> Vec<SlotInfo> {
    let mut out = Vec::new();
    for s in list.get("saves").and_then(Json::as_array).unwrap_or_default() {
        let Some(name) = s.get("name").and_then(Json::as_str) else { continue };
        let slot = match (name.strip_prefix(PLAYER_PREFIX), name.strip_prefix(AUTO_PREFIX)) {
            (Some(n), _) => n.parse().ok().filter(|n| (1..=PLAYER_SLOTS).contains(n)).map(Slot::Player),
            (_, Some(n)) => n.parse().ok().filter(|n| (1..=AUTO_SLOTS).contains(n)).map(Slot::Auto),
            _ => None,
        };
        let Some(slot) = slot else { continue };
        if s.get("unreadable").is_some() {
            out.push(SlotInfo { slot, title: "Damaged save".into(), detail: "Unreadable: save over it or delete it".into() });
            continue;
        }
        let label = s.get("label").and_then(Json::as_str).unwrap_or("A saved run");
        let label = label.strip_prefix("Autosave, ").unwrap_or(label);
        let played = s.get("tick").and_then(Json::as_u64).unwrap_or(0) / u64::from(TICK_HZ);
        let when = s.get("created_unix_s").and_then(Json::as_u64).map(date_utc).unwrap_or_default();
        out.push(SlotInfo { slot, title: label.chars().take(40).collect(), detail: format!("{}:{:02} played, {when}", played / 60, played % 60) });
    }
    out
}

/// `unix seconds` as "18 Oct 2026, 06:30 UTC" (no time zone database: UTC says so).
pub(crate) fn date_utc(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let secs = unix % 86_400;
    // Civil from days (Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{d} {} {y} {:02}:{:02} UTC", MONTHS[(m - 1) as usize], secs / 3600, secs % 3600 / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_slots_read_right() {
        assert_eq!(date_utc(0), "1 Jan 1970 00:00 UTC");
        assert_eq!(date_utc(1_792_267_431), "17 Oct 2026 20:03 UTC");
        let list = Json::parse(
            r#"{"saves": [
                {"name": "sepulcra_slot_2", "label": "Wave 3/10, 9/10 tombstones", "tick": 5430, "created_unix_s": 1792267431},
                {"name": "sepulcra_autosave_1", "label": "Autosave, Wave 1/10, 10/10 tombstones", "tick": 900, "created_unix_s": 0},
                {"name": "sepulcra_slot_9", "tick": 1},
                {"name": "someone_elses", "tick": 1},
                {"name": "sepulcra_slot_3", "unreadable": {"error": "checksum"}}
            ]}"#,
        )
        .unwrap();
        let slots = slots_of(&list);
        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].slot, Slot::Player(2));
        assert_eq!(slots[0].detail, "3:01 played, 17 Oct 2026 20:03 UTC");
        assert_eq!(slots[1].slot, Slot::Auto(1));
        assert_eq!(slots[1].title, "Wave 1/10, 10/10 tombstones");
        assert_eq!(slots[2].title, "Damaged save");
    }
}

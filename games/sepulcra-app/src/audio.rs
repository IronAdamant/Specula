//! Sound and music through the engine's audio service: the procedural sound recipes in
//! `games/sepulcra/assets/sounds/` and the music pieces in `music/` (no recorded assets),
//! rendered on a worker thread at start-up and played from the simulation's events, in 3D
//! around the camera. Map 1's theme loops on the music bus and ducks under the voice bus (the
//! boss call, the victory and defeat stings) and a little under effects; a won or lost run
//! crossfades it into the victory or defeat cue.
//!
//! Sound follows the game, never the other way round: nothing here touches the simulation, so
//! replays and hashes don't depend on it. Headless engines have no audio unless started with
//! `--audio null`, and `--mute` runs never play; agents hear what played with `audio_played`
//! and `audio_levels`, and read the game's side in `play_state` (`audio`).

use sepulcra::{CellId, Content, Event};
use specula::audio_service::AudioService;
use specula::sight::Camera;
use specula_audio::music::{self, Music, MusicSource};
use specula_audio::{Bus, DuckRule, Listener, PlayParams, RenderOptions, Sound, Spatial, StreamParams, VoiceId};
use specula_core::math::Vec3;
use specula_core::Json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

/// Who the game's plays are recorded as (`audio_played`'s `by`).
const BY: &str = "sepulcra";

/// Every sound the game plays: (name, recipe).
const RECIPES: [(&str, &str); 26] = [
    ("bolt_fire", include_str!("../../sepulcra/assets/sounds/bolt_fire.json")),
    ("bolt_hit", include_str!("../../sepulcra/assets/sounds/bolt_hit.json")),
    ("boss_call", include_str!("../../sepulcra/assets/sounds/boss_call.json")),
    ("brazier_crackle_loop", include_str!("../../sepulcra/assets/sounds/brazier_crackle_loop.json")),
    ("brazier_whoosh", include_str!("../../sepulcra/assets/sounds/brazier_whoosh.json")),
    ("colossus_collapse", include_str!("../../sepulcra/assets/sounds/colossus_collapse.json")),
    ("defeat", include_str!("../../sepulcra/assets/sounds/defeat.json")),
    ("night_ambience", include_str!("../../sepulcra/assets/sounds/night_ambience.json")),
    ("obelisk_zap", include_str!("../../sepulcra/assets/sounds/obelisk_zap.json")),
    ("pillar_pulse", include_str!("../../sepulcra/assets/sounds/pillar_pulse.json")),
    ("runner_death", include_str!("../../sepulcra/assets/sounds/runner_death.json")),
    ("sell", include_str!("../../sepulcra/assets/sounds/sell.json")),
    ("shambler_death", include_str!("../../sepulcra/assets/sounds/shambler_death.json")),
    ("stone_place", include_str!("../../sepulcra/assets/sounds/stone_place.json")),
    ("tombstone_lost", include_str!("../../sepulcra/assets/sounds/tombstone_lost.json")),
    ("ui_click", include_str!("../../sepulcra/assets/sounds/ui_click.json")),
    ("ui_error", include_str!("../../sepulcra/assets/sounds/ui_error.json")),
    ("ui_hover", include_str!("../../sepulcra/assets/sounds/ui_hover.json")),
    ("upgrade_chime", include_str!("../../sepulcra/assets/sounds/upgrade_chime.json")),
    ("victory", include_str!("../../sepulcra/assets/sounds/victory.json")),
    ("wall_broken", include_str!("../../sepulcra/assets/sounds/wall_broken.json")),
    ("wall_hit", include_str!("../../sepulcra/assets/sounds/wall_hit.json")),
    ("wall_place", include_str!("../../sepulcra/assets/sounds/wall_place.json")),
    ("wave_cleared", include_str!("../../sepulcra/assets/sounds/wave_cleared.json")),
    ("wave_start", include_str!("../../sepulcra/assets/sounds/wave_start.json")),
    ("wraith_death", include_str!("../../sepulcra/assets/sounds/wraith_death.json")),
];

/// The music: (name, piece).
const PIECES: [(&str, &str); 3] = [
    ("map1", include_str!("../../sepulcra/assets/music/map1.json")),
    ("victory", include_str!("../../sepulcra/assets/music/victory.json")),
    ("defeat", include_str!("../../sepulcra/assets/music/defeat.json")),
];

/// Variations rendered of the sounds that repeat a lot (so a volley isn't one sample, F3: a
/// few renders, not one per shot).
const VARIED: [&str; 7] = ["bolt_fire", "bolt_hit", "obelisk_zap", "brazier_whoosh", "shambler_death", "runner_death", "wall_hit"];
const VARIATIONS: u32 = 3;
/// Music fades in at the start and crossfades between pieces over these times (seconds).
const MUSIC_FADE_IN: f32 = 4.0;
const CUE_CROSSFADE: f32 = 1.5;
const BACK_TO_THEME: f32 = 2.5;

/// The least time between two plays of one sound (seconds of presentation time), so fast-forward
/// and big volleys don't stack dozens of copies.
fn min_gap(name: &str) -> f32 {
    match name {
        "bolt_hit" | "wall_hit" => 0.07,
        "bolt_fire" | "obelisk_zap" | "brazier_whoosh" => 0.05,
        "pillar_pulse" => 0.25,
        _ => 0.03,
    }
}

type Rendered = Vec<(String, Result<Sound, String>)>;
type Pieces = Vec<(String, Result<Arc<music::Rendered>, String>)>;

enum Loaded {
    Sounds(Rendered),
    Music(Pieces),
}

fn arr(v: Vec3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

/// Renders the music pieces named (from `dir` when it has them, else the built-in copies).
fn render_pieces(dir: Option<&PathBuf>, names: &[&str]) -> Pieces {
    names
        .iter()
        .map(|&name| {
            let builtin = PIECES.iter().find(|(n, _)| *n == name).map(|(_, t)| *t);
            let from_disk = dir.and_then(|d| std::fs::read_to_string(d.join(format!("{name}.json"))).ok());
            let r = match from_disk.as_deref().or(builtin) {
                None => Err(format!("no music piece {name}")),
                Some(text) => Music::parse(text).and_then(|m| music::render(&m)).map(Arc::new).map_err(|e| e.to_string()),
            };
            (name.to_owned(), r)
        })
        .collect()
}

/// The game's sound: the engine's audio service, the rendered sounds and music, and what plays.
pub struct Sfx {
    audio: Option<Arc<AudioService>>,
    sounds: BTreeMap<String, Vec<Sound>>,
    pieces: BTreeMap<String, Arc<music::Rendered>>,
    loading: Vec<Receiver<Loaded>>,
    /// Reloads waiting for the running render (names), and where to read them.
    pending_sounds: std::collections::BTreeSet<String>,
    pending_pieces: std::collections::BTreeSet<&'static str>,
    reload_from: Option<PathBuf>,
    last: BTreeMap<&'static str, f32>,
    ambience: Option<VoiceId>,
    /// Brazier fires crackling, by cell.
    crackles: BTreeMap<CellId, VoiceId>,
    /// The music voice and its piece, and the piece the game wants (it may still be rendering).
    music: Option<(VoiceId, String)>,
    music_want: Option<&'static str>,
    /// Plays started, and plays skipped (too soon after the same sound, or not loaded yet).
    played: u64,
    skipped: u64,
    pub problem: Option<String>,
    volume: f32,
    counter: u32,
}

impl Default for Sfx {
    fn default() -> Self {
        Sfx::off("not started")
    }
}

impl Sfx {
    /// No audio (headless, `--mute`): every call is a no-op.
    pub fn off(why: &str) -> Sfx {
        Sfx {
            audio: None,
            sounds: BTreeMap::new(),
            pieces: BTreeMap::new(),
            loading: Vec::new(),
            pending_sounds: Default::default(),
            pending_pieces: Default::default(),
            reload_from: None,
            last: BTreeMap::new(),
            ambience: None,
            crackles: BTreeMap::new(),
            music: None,
            music_want: None,
            played: 0,
            skipped: 0,
            problem: Some(why.to_owned()),
            volume: 1.0,
            counter: 0,
        }
    }

    /// Plays through the engine's audio service (off when the service is off), rendering the
    /// sound set and the music on a worker thread; sounds play once they land, and map 1's theme
    /// starts when it does. Recipes and pieces come from `assets` (`sounds/<name>.json`,
    /// `music/<name>.json`) when it has them, else the copies built into the game.
    pub fn start(audio: Arc<AudioService>, assets: Option<PathBuf>) -> Sfx {
        if !audio.enabled() {
            return Sfx::off("the engine's audio is off (--audio off, the headless default)");
        }
        let sound_dir = assets.as_ref().map(|a| a.join("sounds"));
        let music_dir = assets.as_ref().map(|a| a.join("music"));
        let (tx, rx) = mpsc::channel();
        let md = music_dir.clone();
        let spawned = std::thread::Builder::new().name("sepulcra-sounds".into()).spawn(move || {
            let mut out: Rendered = Vec::new();
            for (name, builtin) in RECIPES {
                let from_disk = sound_dir.as_ref().and_then(|d| std::fs::read_to_string(d.join(format!("{name}.json"))).ok());
                let text = from_disk.as_deref().unwrap_or(builtin);
                let n = if VARIED.contains(&name) { VARIATIONS } else { 1 };
                for variation in 0..n {
                    let r = specula_audio::load_recipe(text, &RenderOptions { variation, ..RenderOptions::default() }).map_err(|e| e.to_string());
                    out.push((name.to_owned(), r));
                }
            }
            // Sounds first (the game needs them at once); the music takes longer to render.
            let _ = tx.send(Loaded::Sounds(out));
            let names: Vec<&str> = PIECES.iter().map(|(n, _)| *n).collect();
            let _ = tx.send(Loaded::Music(render_pieces(md.as_ref(), &names)));
        });
        let mut sfx = Sfx::off("loading");
        sfx.problem = None;
        sfx.music_want = Some("map1");
        match spawned {
            Ok(_) => sfx.loading.push(rx),
            Err(e) => sfx.problem = Some(format!("sound thread: {e}")),
        }
        // The music sits under the voice bus (stings, the boss call) and a little under effects.
        let duck = |source, target, depth_db, threshold_db| Some(DuckRule { source, target, depth_db, threshold_db, attack_s: 0.05, release_s: 0.8 });
        let ducks = [(0, duck(Bus::Voice, Bus::Music, 12.0, -45.0)), (1, duck(Bus::Sfx, Bus::Music, 4.0, -30.0)), (2, duck(Bus::Voice, Bus::Ambience, 6.0, -45.0))];
        if let Err(e) = audio.apply_settings(BY, &[], &ducks, 0.0) {
            sfx.problem = Some(format!("duck rules: {e}"));
        }
        sfx.audio = Some(audio);
        sfx
    }

    /// Hot reload: renders the sounds and pieces named (file stems under `sounds/` and `music/`)
    /// again from `assets` on a worker thread; each replaces the old one when it lands (one that
    /// fails keeps the old), and a piece that is playing crossfades to its new render.
    ///
    /// One render runs at a time: edits that arrive meanwhile are gathered (by name) and render
    /// once it finishes, from the files as they are then, so rapid saves coalesce instead of
    /// starting a render each.
    pub fn reload(&mut self, assets: PathBuf, sounds: &[String], pieces: &[String]) {
        if self.audio.is_none() {
            return;
        }
        self.pending_sounds.extend(sounds.iter().filter(|n| RECIPES.iter().any(|(r, _)| r == n)).cloned());
        self.pending_pieces.extend(pieces.iter().filter_map(|n| PIECES.iter().find(|(p, _)| p == n).map(|(p, _)| *p)));
        self.reload_from = Some(assets);
        self.start_reload();
    }

    /// Starts the gathered reloads when no render is running.
    fn start_reload(&mut self) {
        if !self.loading.is_empty() || (self.pending_sounds.is_empty() && self.pending_pieces.is_empty()) {
            return;
        }
        let Some(assets) = self.reload_from.clone() else { return };
        let sounds: Vec<String> = std::mem::take(&mut self.pending_sounds).into_iter().collect();
        let pieces: Vec<&'static str> = std::mem::take(&mut self.pending_pieces).into_iter().collect();
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("sepulcra-sounds".into()).spawn(move || {
            if !sounds.is_empty() {
                let mut out: Rendered = Vec::new();
                for name in sounds {
                    let r = std::fs::read_to_string(assets.join("sounds").join(format!("{name}.json"))).map_err(|e| e.to_string());
                    let n = if VARIED.contains(&name.as_str()) { VARIATIONS } else { 1 };
                    for variation in 0..n {
                        let s = r.clone().and_then(|t| specula_audio::load_recipe(&t, &RenderOptions { variation, ..RenderOptions::default() }).map_err(|e| e.to_string()));
                        out.push((name.clone(), s));
                    }
                }
                let _ = tx.send(Loaded::Sounds(out));
            }
            if !pieces.is_empty() {
                let _ = tx.send(Loaded::Music(render_pieces(Some(&assets.join("music")), &pieces)));
            }
        });
        match spawned {
            Ok(_) => self.loading.push(rx),
            Err(e) => self.problem = Some(format!("sound thread: {e}")),
        }
    }

    /// Picks up rendered sounds and music when they're ready (call once a frame or tick).
    pub fn poll(&mut self) {
        self.poll_loaded();
        // A render finished: start the reloads gathered meanwhile.
        self.start_reload();
    }

    fn poll_loaded(&mut self) {
        let mut i = 0;
        while i < self.loading.len() {
            match self.loading[i].try_recv() {
                Ok(Loaded::Sounds(rendered)) => {
                    // Reloaded sounds replace the old renders (all of a sound's variations at once).
                    let mut fresh: BTreeMap<String, Vec<Sound>> = BTreeMap::new();
                    for (name, r) in rendered {
                        match r {
                            Ok(s) => fresh.entry(name).or_default().push(s),
                            Err(e) => self.problem = Some(format!("sound {name}: {e}")),
                        }
                    }
                    let first = self.sounds.is_empty();
                    self.sounds.extend(fresh);
                    if first {
                        self.start_ambience();
                    }
                }
                Ok(Loaded::Music(pieces)) => {
                    let mut changed = Vec::new();
                    for (name, r) in pieces {
                        match r {
                            Ok(p) => {
                                self.pieces.insert(name.clone(), p);
                                changed.push(name);
                            }
                            Err(e) => self.problem = Some(format!("music {name}: {e}")),
                        }
                    }
                    // A piece that is playing changed: crossfade to the new render.
                    let playing = self.music.as_ref().map(|(_, n)| n.clone());
                    if let Some(p) = playing.filter(|p| changed.contains(p)) {
                        self.play_piece(&p, BACK_TO_THEME);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    i += 1;
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.loading.swap_remove(i);
                    continue;
                }
            }
        }
        // The piece the game wants, once it has rendered.
        if let Some(want) = self.music_want {
            if self.pieces.contains_key(want) {
                self.music_want = None;
                let fade = if self.music.is_none() { MUSIC_FADE_IN } else { BACK_TO_THEME };
                self.play_piece(want, fade);
            }
        }
    }

    fn start_ambience(&mut self) {
        // The night: a quiet bed under everything.
        if let Some(s) = self.sounds.get("night_ambience").and_then(|v| v.first()).cloned() {
            if let Some(a) = &self.audio {
                self.ambience = a.play(BY, &s, PlayParams { bus: Bus::Ambience, gain: 0.55, looping: true, fade_in_s: 2.0, priority: 250, ..PlayParams::default() });
            }
        }
    }

    /// Crossfades the music to `name` (map 1's theme loops; cues play once).
    fn play_piece(&mut self, name: &str, fade: f32) {
        let (Some(a), Some(piece)) = (&self.audio, self.pieces.get(name)) else { return };
        let looping = name == "map1";
        let from = self.music.as_ref().map(|(v, _)| *v);
        let params = StreamParams { bus: Bus::Music, gain: 1.0, ..StreamParams::default() };
        match a.crossfade(BY, from, Box::new(MusicSource::new(Arc::clone(piece), looping)), params, fade) {
            Ok((voice, _)) => self.music = Some((voice, name.to_owned())),
            // The old piece keeps playing and stays ours to stop (review: it was forgotten).
            Err(e) => self.problem = Some(format!("music {name}: {e}")),
        }
    }

    /// Asks for a piece: now if it has rendered, else as soon as it does.
    fn want_piece(&mut self, name: &'static str, fade: f32) {
        if self.pieces.contains_key(name) {
            self.music_want = None;
            self.play_piece(name, fade);
        } else {
            self.music_want = Some(name);
        }
    }

    pub fn enabled(&self) -> bool {
        self.audio.is_some()
    }

    /// The master volume (settings), 0..1.
    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        if let Some(a) = &self.audio {
            if let Err(e) = a.set_bus_gain(BY, Bus::Master, self.volume, 0.1) {
                self.problem = Some(format!("volume: {e}"));
            }
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// The ears follow the camera.
    pub fn listen(&self, camera: &Camera) {
        let Some(a) = &self.audio else { return };
        let f = camera.target - camera.eye;
        a.set_listener(Listener { position: arr(camera.eye), forward: arr(f), up: [0.0, 1.0, 0.0] });
    }

    fn play(&mut self, name: &'static str, now: f32, at: Option<Vec3>, gain: f32, bus: Bus) {
        let Some(a) = &self.audio else { return };
        if self.last.get(name).is_some_and(|&t| now - t < min_gap(name) && now >= t) {
            self.skipped += 1;
            return;
        }
        let Some(variants) = self.sounds.get(name).filter(|v| !v.is_empty()) else {
            self.skipped += 1;
            return;
        };
        self.counter = self.counter.wrapping_add(1);
        let sound = &variants[self.counter as usize % variants.len()];
        // The camera hovers 30-80 m over the slab: full volume within 25 m, fading out to 150.
        let spatial = at.map(|p| Spatial { position: arr(p), min_distance: 25.0, max_distance: 150.0, rolloff: 0.7, lowpass: true });
        if a.play(BY, sound, PlayParams { bus, gain, spatial, ..PlayParams::default() }).is_some() {
            self.played += 1;
            self.last.insert(name, now);
        }
    }

    /// A UI sound (click, error).
    pub fn ui(&mut self, name: &'static str, now: f32) {
        self.play(name, now, None, 0.8, Bus::Ui);
    }

    /// Sounds for one tick's events. `anchor` places a cell; `stone_at` names the stone kind
    /// standing on a cell.
    pub fn events(&mut self, events: &[Event], content: &Content, now: f32, anchor: &dyn Fn(CellId) -> Vec3, stone_at: &dyn Fn(CellId) -> Option<u8>) {
        if self.audio.is_none() {
            return;
        }
        let pos = |p: sepulcra::Pos| {
            let [x, y, z] = p.to_f32();
            Vec3::new(x, y, z)
        };
        for ev in events {
            match *ev {
                Event::Shot { stone, .. } => {
                    let name = match stone_at(stone).map(|k| content.stones[usize::from(k)].id.as_str()) {
                        Some("sky_obelisk") => "obelisk_zap",
                        Some("brazier") => "brazier_whoosh",
                        _ => "bolt_fire",
                    };
                    self.play(name, now, Some(anchor(stone)), 0.6, Bus::Sfx);
                }
                Event::Pulsed { stone, affected } if affected > 0 => self.play("pillar_pulse", now, Some(anchor(stone)), 0.6, Bus::Sfx),
                Event::Exploded { pos: p, .. } => self.play("bolt_hit", now, Some(pos(p)), 0.9, Bus::Sfx),
                Event::Hit { .. } => {}
                Event::Died { kind, pos: p, .. } => {
                    let name = match content.enemy(kind).id.as_str() {
                        "runner" => "runner_death",
                        "wraith" => "wraith_death",
                        "colossus" => "colossus_collapse",
                        _ => "shambler_death",
                    };
                    self.play(name, now, Some(pos(p)), 0.8, Bus::Sfx);
                }
                Event::StonePlaced { cell, .. } => self.play("stone_place", now, Some(anchor(cell)), 1.0, Bus::Sfx),
                Event::StoneUpgraded { cell, .. } => self.play("upgrade_chime", now, Some(anchor(cell)), 1.0, Bus::Sfx),
                Event::StoneSold { cell, .. } | Event::WallRemoved { cell, .. } => self.play("sell", now, Some(anchor(cell)), 1.0, Bus::Sfx),
                Event::WallPlaced { cell } => self.play("wall_place", now, Some(anchor(cell)), 1.0, Bus::Sfx),
                Event::WallDamaged { cell, .. } => self.play("wall_hit", now, Some(anchor(cell)), 0.8, Bus::Sfx),
                Event::WallBroken { cell } => self.play("wall_broken", now, Some(anchor(cell)), 1.0, Bus::Sfx),
                Event::WaveStarted { wave, .. } => {
                    self.play("wave_start", now, None, 0.7, Bus::Ui);
                    if is_boss_wave(content, wave) {
                        // The colossus wakes: the music ducks under its call (the voice bus).
                        self.play("boss_call", now, None, 1.0, Bus::Voice);
                    }
                }
                Event::WaveCleared { .. } => self.play("wave_cleared", now, None, 0.7, Bus::Ui),
                Event::TombstonesLost { .. } => self.play("tombstone_lost", now, None, 0.8, Bus::Ui),
                Event::Won => {
                    self.play("victory", now, None, 0.9, Bus::Voice);
                    self.want_piece("victory", CUE_CROSSFADE);
                }
                Event::Lost => {
                    self.play("defeat", now, None, 0.9, Bus::Voice);
                    self.want_piece("defeat", CUE_CROSSFADE);
                }
                _ => {}
            }
        }
    }

    /// Keeps one crackling loop on every brazier (`fires`: cell and where its flame is).
    pub fn fires(&mut self, fires: &[(CellId, Vec3)]) {
        let Some(a) = &self.audio else { return };
        // Fires that went out (sold): stop their crackle. No allocation per frame.
        self.crackles.retain(|c, v| {
            let lit = fires.iter().any(|(f, _)| f == c);
            if !lit {
                a.stop(*v, 0.3);
            }
            lit
        });
        let Some(sound) = self.sounds.get("brazier_crackle_loop").and_then(|v| v.first()) else { return };
        for &(cell, at) in fires {
            if self.crackles.contains_key(&cell) {
                continue;
            }
            let spatial = Some(Spatial { position: arr(at), min_distance: 12.0, max_distance: 90.0, rolloff: 1.0, lowpass: true });
            if let Some(v) = a.play(BY, sound, PlayParams { bus: Bus::Ambience, gain: 0.5, looping: true, fade_in_s: 0.5, spatial, ..PlayParams::default() }) {
                self.crackles.insert(cell, v);
            }
        }
    }

    /// Stops every sound but the ambience, and brings map 1's theme back (a new run).
    pub fn reset(&mut self) {
        if let Some(a) = &self.audio {
            for (_, v) in std::mem::take(&mut self.crackles) {
                a.stop(v, 0.2);
            }
            a.stop_all(Some(Bus::Sfx), 0.2);
            a.stop_all(Some(Bus::Voice), 0.2);
        }
        if self.audio.is_some() && self.music.as_ref().is_none_or(|(_, n)| n != "map1") {
            self.want_piece("map1", BACK_TO_THEME);
        }
    }

    /// For `play_state`: what the game has loaded and played (the engine's `audio_*` tools say
    /// what was heard).
    pub fn to_json(&self) -> Json {
        let Some(a) = &self.audio else {
            return Json::obj([("off", Json::from(self.problem.as_deref().unwrap_or("off")))]);
        };
        let s = a.summary_json();
        Json::obj([
            ("backend", s.get("backend").cloned().unwrap_or(Json::Null)),
            ("sounds", Json::from(self.sounds.len())),
            ("music_pieces", Json::from(self.pieces.len())),
            ("music", self.music.as_ref().map_or(Json::Null, |(_, n)| Json::from(n.as_str()))),
            ("loading", Json::from(!self.loading.is_empty())),
            ("voices", s.get("voices").cloned().unwrap_or(Json::Null)),
            ("played", Json::from(self.played)),
            ("skipped", Json::from(self.skipped)),
            ("volume", Json::from(self.volume)),
            ("problem", self.problem.as_deref().map_or(Json::Null, Json::from)),
        ])
    }
}

/// Whether wave `wave` (1-based, as `WaveStarted` counts) brings the colossus.
fn is_boss_wave(content: &Content, wave: u16) -> bool {
    let Some(boss) = content.enemy_kind("colossus") else { return false };
    content.waves.get(usize::from(wave).wrapping_sub(1)).is_some_and(|w| w.groups.iter().any(|g| g.enemy == boss))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boss_wave_is_the_colossus_wave() {
        let c = Content::builtin();
        let boss: Vec<u16> = (1..=c.waves.len() as u16).filter(|&w| is_boss_wave(&c, w)).collect();
        // Map 1 has one boss wave: the last.
        assert_eq!(boss, vec![c.waves.len() as u16]);
        assert!(!is_boss_wave(&c, 0) && !is_boss_wave(&c, 999));
    }

    #[test]
    fn the_built_in_music_and_sounds_parse() {
        for (name, text) in PIECES {
            let m = Music::parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(m.name, name);
        }
        for (name, text) in RECIPES {
            assert_eq!(specula_audio::Recipe::parse(text).unwrap_or_else(|e| panic!("{name}: {e}")).name, name);
        }
    }

    #[test]
    fn off_engines_make_every_call_a_no_op() {
        let mut s = Sfx::start(AudioService::off("headless"), None);
        assert!(!s.enabled());
        s.poll();
        s.reset();
        s.ui("ui_click", 0.0);
        assert!(s.to_json().get("off").is_some());
    }
}

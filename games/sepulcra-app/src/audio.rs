//! Sound with `specula-audio`: the procedural sound recipes in `games/sepulcra/assets/sounds/`
//! (no recorded assets), rendered on a worker thread at start-up and played from the
//! simulation's events, in 3D around the camera.
//!
//! Sound follows the game, never the other way round: nothing here touches the simulation, so
//! replays and hashes don't depend on it. Headless engines and `--mute` runs have no audio at
//! all; agents read what was played from `play_state` (`audio`).

use sepulcra::{CellId, Content, Event};
use specula::sight::Camera;
use specula_audio::{Audio, AudioConfig, Bus, Listener, PlayParams, RenderOptions, Sound, Spatial, VoiceId};
use specula_core::math::Vec3;
use specula_core::Json;
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver};

/// Every sound the game plays: (name, recipe).
const RECIPES: [(&str, &str); 25] = [
    ("bolt_fire", include_str!("../../sepulcra/assets/sounds/bolt_fire.json")),
    ("bolt_hit", include_str!("../../sepulcra/assets/sounds/bolt_hit.json")),
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

/// Variations rendered of the sounds that repeat a lot (so a volley isn't one sample, F3: a
/// few renders, not one per shot).
const VARIED: [&str; 7] = ["bolt_fire", "bolt_hit", "obelisk_zap", "brazier_whoosh", "shambler_death", "runner_death", "wall_hit"];
const VARIATIONS: u32 = 3;

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

fn arr(v: Vec3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

/// The game's sound: the audio device, the rendered sounds, and what plays.
pub struct Sfx {
    audio: Option<Audio>,
    sounds: BTreeMap<String, Vec<Sound>>,
    loading: Option<Receiver<Rendered>>,
    last: BTreeMap<&'static str, f32>,
    ambience: Option<VoiceId>,
    /// Brazier fires crackling, by cell.
    crackles: BTreeMap<CellId, VoiceId>,
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
            loading: None,
            last: BTreeMap::new(),
            ambience: None,
            crackles: BTreeMap::new(),
            played: 0,
            skipped: 0,
            problem: Some(why.to_owned()),
            volume: 1.0,
            counter: 0,
        }
    }

    /// Opens the default output (or the null backend when there is none) and renders the sound
    /// set on a worker thread; sounds play once it lands.
    pub fn start() -> Sfx {
        let audio = match Audio::start(AudioConfig::default()) {
            Ok(a) => a,
            Err(e) => return Sfx::off(&format!("audio: {e}")),
        };
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("sepulcra-sounds".into()).spawn(move || {
            let mut out: Rendered = Vec::new();
            for (name, text) in RECIPES {
                let n = if VARIED.contains(&name) { VARIATIONS } else { 1 };
                for variation in 0..n {
                    let r = specula_audio::load_recipe(text, &RenderOptions { variation, ..RenderOptions::default() }).map_err(|e| e.to_string());
                    out.push((name.to_owned(), r));
                }
            }
            let _ = tx.send(out);
        });
        let mut sfx = Sfx::off("loading");
        sfx.audio = Some(audio);
        sfx.problem = None;
        match spawned {
            Ok(_) => sfx.loading = Some(rx),
            Err(e) => sfx.problem = Some(format!("sound thread: {e}")),
        }
        sfx
    }

    /// Picks up the rendered sounds when they're ready (call once a frame).
    pub fn poll(&mut self) {
        let Some(rx) = &self.loading else { return };
        let Ok(rendered) = rx.try_recv() else { return };
        self.loading = None;
        for (name, r) in rendered {
            match r {
                Ok(s) => self.sounds.entry(name).or_default().push(s),
                Err(e) => self.problem = Some(format!("sound {name}: {e}")),
            }
        }
        // The night: a quiet bed under everything.
        if let Some(s) = self.sounds.get("night_ambience").and_then(|v| v.first()).cloned() {
            if let Some(a) = &self.audio {
                self.ambience = a.play(&s, PlayParams { bus: Bus::Ambience, gain: 0.55, looping: true, fade_in_s: 2.0, priority: 250, ..PlayParams::default() }).ok();
            }
        }
    }

    pub fn enabled(&self) -> bool {
        self.audio.is_some()
    }

    /// The master volume (settings), 0..1.
    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        if let Some(a) = &self.audio {
            let _ = a.set_bus_gain(Bus::Master, self.volume, 0.1);
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// The ears follow the camera.
    pub fn listen(&self, camera: &Camera) {
        let Some(a) = &self.audio else { return };
        let f = camera.target - camera.eye;
        let _ = a.set_listener(Listener { position: arr(camera.eye), forward: arr(f), up: [0.0, 1.0, 0.0] });
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
        if a.play(sound, PlayParams { bus, gain, spatial, ..PlayParams::default() }).is_ok() {
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
                Event::WaveStarted { .. } => self.play("wave_start", now, None, 0.7, Bus::Ui),
                Event::WaveCleared { .. } => self.play("wave_cleared", now, None, 0.7, Bus::Ui),
                Event::TombstonesLost { .. } => self.play("tombstone_lost", now, None, 0.8, Bus::Ui),
                Event::Won => self.play("victory", now, None, 0.8, Bus::Ui),
                Event::Lost => self.play("defeat", now, None, 0.8, Bus::Ui),
                _ => {}
            }
        }
    }

    /// Keeps one crackling loop on every brazier (`fires`: cell and where its flame is).
    pub fn fires(&mut self, fires: &[(CellId, Vec3)]) {
        let Some(a) = &self.audio else { return };
        let gone: Vec<CellId> = self.crackles.keys().copied().filter(|c| !fires.iter().any(|(f, _)| f == c)).collect();
        for c in gone {
            if let Some(v) = self.crackles.remove(&c) {
                let _ = a.stop(v, 0.3);
            }
        }
        let Some(sound) = self.sounds.get("brazier_crackle_loop").and_then(|v| v.first()) else { return };
        for &(cell, at) in fires {
            if self.crackles.contains_key(&cell) {
                continue;
            }
            let spatial = Some(Spatial { position: arr(at), min_distance: 12.0, max_distance: 90.0, rolloff: 1.0, lowpass: true });
            if let Ok(v) = a.play(sound, PlayParams { bus: Bus::Ambience, gain: 0.5, looping: true, fade_in_s: 0.5, spatial, ..PlayParams::default() }) {
                self.crackles.insert(cell, v);
            }
        }
    }

    /// Stops every sound but the ambience (a new run).
    pub fn reset(&mut self) {
        if let Some(a) = &self.audio {
            for (_, v) in std::mem::take(&mut self.crackles) {
                let _ = a.stop(v, 0.2);
            }
            let _ = a.stop_all(Some(Bus::Sfx), 0.2);
        }
    }

    /// For `play_state`: backend, voices, and what was played.
    pub fn to_json(&self) -> Json {
        let Some(a) = &self.audio else {
            return Json::obj([("off", Json::from(self.problem.as_deref().unwrap_or("off")))]);
        };
        let s = a.stats();
        Json::obj([
            ("backend", Json::from(format!("{:?}", s.backend).to_lowercase())),
            ("sounds", Json::from(self.sounds.len())),
            ("loading", Json::from(self.loading.is_some())),
            ("voices", Json::from(s.active_voices)),
            ("played", Json::from(self.played)),
            ("skipped", Json::from(self.skipped)),
            ("volume", Json::from(self.volume)),
            ("problem", self.problem.as_deref().map_or(Json::Null, Json::from)),
        ])
    }
}

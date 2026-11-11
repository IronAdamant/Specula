//! Sounds that modules ask for ([`specula_sdk::Cx::sound`]): found as sound recipes, rendered
//! off the engine thread, cached, and played through the engine's [`AudioService`] as
//! `module:<name>` on the `sfx` bus (`audio_played {by: "module:<name>"}` lists them).
//!
//! **Presentation only** (docs/design/GAMEPLAY_MODULES.md): the module host queues a call's
//! sounds only when the call commits, and nothing here feeds back into the simulation. A missing
//! recipe, a render error, audio being off or a sound rendered too late is skipped and traced
//! (`module.sound` under `module:<name>`), never an error to the module, so runs with and without
//! audio hash the same.
//!
//! Where a name is looked for, first match wins: the module's own folder
//! (`<module>/sounds/<name>.json`, when it was built with `logic_build`), then the engine's
//! assets (`sounds/<name>.json`, `assets/sounds/<name>.json`).
//!
//! **Cost before work:** a recipe file over [`MAX_RECIPE_BYTES`] is refused unread; the
//! length is read from the parsed recipe and one over [`MAX_SOUND_S`] is refused before any
//! rendering (the synth's own caps bound the rest); the cache holds at most [`MAX_CACHED`]
//! entries of every kind (ready, loading and failed names: failed ones go first, then the least
//! recently used sound) and [`MAX_CACHED_SAMPLES`] samples, kept as running totals; at most
//! [`MAX_LOADING`] renders run at once (more cues for new names are skipped) and each waits with
//! at most [`MAX_WAITING`] cues.

use crate::logic::SoundCue;
use crate::Engine;
use specula_audio::{PlayParams, RenderOptions, Sound, Spatial};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Largest recipe file read.
pub const MAX_RECIPE_BYTES: u64 = 64 << 10;
/// Longest module sound kept (a cue is a one-shot; music and ambience belong to the game).
pub const MAX_SOUND_S: f64 = 10.0;
/// Sounds cached at once, and their samples in all (about 32 MiB of f32).
pub const MAX_CACHED: usize = 64;
pub const MAX_CACHED_SAMPLES: usize = 8 << 20;
/// Renders in flight at once.
pub const MAX_LOADING: usize = 4;
/// Cues waiting for one render.
pub const MAX_WAITING: usize = 8;
/// A cue that waited longer than this for its render is skipped (late sound is wrong sound).
const STALE: Duration = Duration::from_millis(500);
/// A name that failed is retried after this long (an agent may have written the recipe since).
const RETRY_FAILED: Duration = Duration::from_secs(5);

enum Entry {
    Loading { waiting: Vec<(SoundCue, Instant)> },
    Ready { sound: Sound, used: u64 },
    Failed { why: String, at: Instant },
}

/// The engine's player for module sounds.
pub struct ModuleSounds {
    audio: Option<Arc<crate::audio_service::AudioService>>,
    assets: PathBuf,
    /// By `<module>/<name>` (a module's own recipe may differ from another's of the same name).
    cache: BTreeMap<String, Entry>,
    /// Running totals over `cache` (never walked to count): ready samples, loading and failed entries.
    samples: usize,
    loading: usize,
    failed: usize,
    clock: u64,
    pub played: u64,
    pub skipped: u64,
}

impl ModuleSounds {
    pub fn new(assets: &Path) -> ModuleSounds {
        ModuleSounds { audio: None, assets: assets.to_owned(), cache: BTreeMap::new(), samples: 0, loading: 0, failed: 0, clock: 0, played: 0, skipped: 0 }
    }

    pub fn set_audio(&mut self, audio: Arc<crate::audio_service::AudioService>) {
        self.audio = Some(audio);
    }

    fn enabled(&self) -> bool {
        self.audio.as_ref().is_some_and(|a| a.enabled())
    }

    fn loading(&self) -> usize {
        self.loading
    }

    fn count(&mut self, e: &Entry, sign: isize) {
        let (samples, loading, failed) = match e {
            Entry::Ready { sound, .. } => (sound.samples().len(), 0, 0),
            Entry::Loading { .. } => (0, 1, 0),
            Entry::Failed { .. } => (0, 0, 1),
        };
        let add = |t: &mut usize, n: usize| *t = if sign > 0 { *t + n } else { t.saturating_sub(n) };
        add(&mut self.samples, samples);
        add(&mut self.loading, loading);
        add(&mut self.failed, failed);
    }

    /// Puts an entry in the cache, keeping the running totals.
    fn insert(&mut self, key: String, e: Entry) {
        self.count(&e, 1);
        if let Some(old) = self.cache.insert(key, e) {
            self.count(&old, -1);
        }
    }

    fn remove(&mut self, key: &str) -> Option<Entry> {
        let e = self.cache.remove(key)?;
        self.count(&e, -1);
        Some(e)
    }

    /// Drops failed names (oldest first), then the least recently used ready sounds, until
    /// `room` more samples and one more entry fit. `false` when they can't (every entry is a
    /// render in flight). At most [`MAX_CACHED`] entries are looked at per drop.
    fn make_room(&mut self, room: usize) -> bool {
        while self.cache.len() >= MAX_CACHED || self.samples + room > MAX_CACHED_SAMPLES {
            let failed = (self.failed > 0).then(|| self.cache.iter().filter_map(|(k, e)| if let Entry::Failed { at, .. } = e { Some((*at, k.clone())) } else { None }).min().map(|(_, k)| k)).flatten();
            let victim = failed.or_else(|| self.cache.iter().filter_map(|(k, e)| if let Entry::Ready { used, .. } = e { Some((*used, k.clone())) } else { None }).min().map(|(_, k)| k));
            match victim {
                Some(k) => {
                    self.remove(&k);
                }
                None => return false,
            }
        }
        true
    }

    fn play(&mut self, cue: &SoundCue, sound: &Sound) {
        let Some(audio) = &self.audio else { return };
        let spatial = cue.at.map(|position| Spatial { position, min_distance: 2.0, max_distance: 40.0, rolloff: 1.0, lowpass: true });
        let by = crate::logic::agent_of(&cue.module);
        let played = audio.play(&by, sound, PlayParams { gain: cue.gain, spatial, ..PlayParams::default() }).is_some();
        if played {
            self.played += 1;
        } else {
            self.skipped += 1;
        }
        trace(cue, if played { "played" } else { "no_voice" });
    }

    fn skip(&mut self, cue: &SoundCue, why: &str) {
        self.skipped += 1;
        trace(cue, why);
    }

    pub fn to_json(&self) -> specula_core::Json {
        use specula_core::Json;
        Json::obj([
            ("played", Json::from(self.played)),
            ("skipped", Json::from(self.skipped)),
            ("cached", Json::from(self.cache.len().saturating_sub(self.loading + self.failed))),
            ("loading", Json::from(self.loading())),
            ("failed_names", Json::from(self.failed)),
            (
                "failed",
                Json::arr(self.cache.iter().filter_map(|(k, e)| match e {
                    Entry::Failed { why, .. } => Some(Json::obj([("sound", Json::from(k.as_str())), ("why", Json::from(why.chars().take(300).collect::<String>()))])),
                    _ => None,
                }).take(8)),
            ),
        ])
    }
}

fn trace(cue: &SoundCue, outcome: &str) {
    let sub = specula_core::trace::subsystem(&format!("module:{}", cue.module));
    specula_core::trace_event!(sub, Debug, "module.sound", name = cue.name.as_str(), outcome = outcome, tick = cue.tick);
}

/// The candidate recipe files for `name`, in the order they are tried.
fn candidates(module_dir: Option<&Path>, assets: &Path, name: &str) -> Vec<PathBuf> {
    let file = format!("{name}.json");
    let mut out = Vec::new();
    if let Some(d) = module_dir {
        out.push(d.join("sounds").join(&file));
    }
    out.push(assets.join("sounds").join(&file));
    out.push(assets.join("assets").join("sounds").join(&file));
    out
}

/// Finds, reads and renders a recipe (on a worker).
fn render(paths: &[PathBuf]) -> Result<Sound, String> {
    for p in paths {
        let Ok(meta) = std::fs::metadata(p) else { continue };
        if !meta.is_file() {
            continue;
        }
        if meta.len() > MAX_RECIPE_BYTES {
            return Err(format!("{} is {} bytes; a module sound recipe is at most {} KiB", p.display(), meta.len(), MAX_RECIPE_BYTES >> 10));
        }
        let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        // Cost before work: the length is known from the parsed recipe, before any rendering.
        let recipe = specula_audio::Recipe::parse(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        if recipe.rendered_seconds() > MAX_SOUND_S {
            return Err(format!("{} is {:.1} s long; a module sound is at most {MAX_SOUND_S} s (refused before rendering)", p.display(), recipe.rendered_seconds()));
        }
        let sound = specula_audio::synth::render(&recipe, &RenderOptions::default()).and_then(|r| r.to_sound()).map_err(|e| format!("{}: {e}", p.display()))?;
        return Ok(sound);
    }
    Err("no recipe sounds/<name>.json in the module's folder or the assets".into())
}

impl Engine {
    /// Plays (or starts rendering) the sounds committed module calls asked for. Called once per
    /// frame after the ticks; cheap when there are none.
    pub(crate) fn play_module_sounds(&mut self) {
        let cues = self.logic.take_sounds();
        if cues.is_empty() {
            return;
        }
        let now = Instant::now();
        for cue in cues {
            if !self.module_sounds.enabled() {
                self.module_sounds.skip(&cue, "audio_off");
                continue;
            }
            let key = format!("{}/{}", cue.module, cue.name);
            self.module_sounds.clock += 1;
            let clock = self.module_sounds.clock;
            match self.module_sounds.cache.get_mut(&key) {
                Some(Entry::Ready { sound, used }) => {
                    *used = clock;
                    let sound = sound.clone();
                    self.module_sounds.play(&cue, &sound);
                    continue;
                }
                Some(Entry::Loading { waiting }) => {
                    if waiting.len() < MAX_WAITING {
                        waiting.push((cue, now));
                    } else {
                        self.module_sounds.skip(&cue, "busy");
                    }
                    continue;
                }
                Some(Entry::Failed { at, .. }) if at.elapsed() < RETRY_FAILED => {
                    self.module_sounds.skip(&cue, "failed");
                    continue;
                }
                _ => {}
            }
            if self.module_sounds.loading() >= MAX_LOADING {
                self.module_sounds.skip(&cue, "busy");
                continue;
            }
            // A name that failed before is replaced; a new one needs room (failed names go first).
            if self.module_sounds.remove(&key).is_none() && !self.module_sounds.make_room(0) {
                self.module_sounds.skip(&cue, "busy");
                continue;
            }
            let dir = self.logic.origin(&cue.module).and_then(|o| o.dir.clone());
            let paths = candidates(dir.as_deref(), &self.module_sounds.assets, &cue.name);
            self.module_sounds.insert(key.clone(), Entry::Loading { waiting: vec![(cue, now)] });
            let job = crate::workers::Offload::engine(crate::workers::JobKind::ModuleSound).class(crate::schedule::Class::Disk);
            let key2 = key.clone();
            let started = self.offload(job, move |_| render(&paths), move |e: &mut Engine, r| {
                let result = match r {
                    Ok(r) => r,
                    Err(f) => Err(format!("{f:?}")),
                };
                let Some(Entry::Loading { waiting }) = e.module_sounds.remove(&key2) else { return };
                match result {
                    Ok(sound) => {
                        e.module_sounds.clock += 1;
                        let used = e.module_sounds.clock;
                        if e.module_sounds.make_room(sound.samples().len()) {
                            e.module_sounds.insert(key2, Entry::Ready { sound: sound.clone(), used });
                        }
                        for (cue, asked) in waiting {
                            if asked.elapsed() > STALE {
                                e.module_sounds.skip(&cue, "late");
                            } else {
                                e.module_sounds.play(&cue, &sound);
                            }
                        }
                    }
                    Err(why) => {
                        for (cue, _) in &waiting {
                            e.module_sounds.skip(cue, "failed");
                        }
                        if let Some((cue, _)) = waiting.first() {
                            let sub = specula_core::trace::subsystem(&format!("module:{}", cue.module));
                            specula_core::trace_event!(sub, Warn, "module.sound_failed", name = cue.name.as_str(), why = why.as_str());
                        }
                        if e.module_sounds.make_room(0) {
                            e.module_sounds.insert(key2, Entry::Failed { why, at: Instant::now() });
                        }
                    }
                }
            });
            if started.is_err() {
                if let Some(Entry::Loading { waiting }) = self.module_sounds.remove(&key) {
                    for (cue, _) in &waiting {
                        self.module_sounds.skip(cue, "busy");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipes_are_looked_for_in_the_module_then_the_assets() {
        let c = candidates(Some(Path::new("m")), Path::new("a"), "flare");
        assert_eq!(c, vec![Path::new("m/sounds/flare.json"), Path::new("a/sounds/flare.json"), Path::new("a/assets/sounds/flare.json")]);
        assert_eq!(candidates(None, Path::new("a"), "x").len(), 2);
    }

    #[test]
    fn missing_big_and_long_recipes_are_refused_before_or_after_rendering() {
        let dir = std::env::temp_dir().join(format!("specula-module-sounds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(render(&[dir.join("nope.json")]).unwrap_err().contains("no recipe"));
        let big = dir.join("big.json");
        std::fs::write(&big, vec![b' '; (MAX_RECIPE_BYTES + 1) as usize]).unwrap();
        assert!(render(&[big]).unwrap_err().contains("at most"));
        let bad = dir.join("bad.json");
        std::fs::write(&bad, "{\"sound\": 2}").unwrap();
        assert!(render(&[bad]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_cache_evicts_the_least_recently_used_sound() {
        let mut s = ModuleSounds::new(Path::new("."));
        let tone = Sound::new("t", 48_000, 1, vec![0.0; 1000]).unwrap();
        for i in 0..MAX_CACHED {
            s.insert(format!("m/s{i}"), Entry::Ready { sound: tone.clone(), used: i as u64 + 10 });
        }
        if let Some(Entry::Ready { used, .. }) = s.cache.get_mut("m/s0") {
            *used = 1000;
        }
        s.make_room(1000);
        assert_eq!(s.cache.len(), MAX_CACHED - 1);
        assert!(s.cache.contains_key("m/s0"), "recently used stays");
        assert!(!s.cache.contains_key("m/s1"), "least recently used goes");
    }

    /// Review of m5-gmods (major 4): every distinct missing name used to add a failed entry that
    /// was never evicted. Failed names count against the cap and go first.
    #[test]
    fn failed_entries_are_bounded() {
        let mut s = ModuleSounds::new(Path::new("."));
        let tone = Sound::new("t", 48_000, 1, vec![0.0; 1000]).unwrap();
        s.insert("m/kept".into(), Entry::Ready { sound: tone, used: 1 });
        for i in 0..1000 {
            assert!(s.make_room(0));
            s.insert(format!("m/missing{i}"), Entry::Failed { why: "no recipe".into(), at: Instant::now() });
        }
        assert!(s.cache.len() <= MAX_CACHED, "{}", s.cache.len());
        assert!(s.cache.contains_key("m/kept"), "failed names go before sounds");
        assert_eq!((s.samples, s.failed, s.loading), (1000, s.cache.len() - 1, 0), "running totals");
        // Renders in flight can't be dropped: past the cap, a new name is refused.
        let mut t = ModuleSounds::new(Path::new("."));
        for i in 0..MAX_CACHED {
            t.insert(format!("m/l{i}"), Entry::Loading { waiting: Vec::new() });
        }
        assert!(!t.make_room(0));
    }

    /// Cost before work (review minor): a recipe longer than a module sound may be is refused
    /// from its parsed length, before rendering.
    #[test]
    fn long_recipes_are_refused_before_rendering() {
        let dir = std::env::temp_dir().join(format!("specula-module-sounds-long-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let long = dir.join("long.json");
        std::fs::write(&long, r#"{"sound": 1, "name": "long", "duration": 60, "layers": [{"source": {"wave": "noise"}}]}"#).unwrap();
        let t = Instant::now();
        let e = render(&[long]).unwrap_err();
        assert!(e.contains("refused before rendering"), "{e}");
        assert!(t.elapsed() < Duration::from_secs(1), "no 60 s render first: {:?}", t.elapsed());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

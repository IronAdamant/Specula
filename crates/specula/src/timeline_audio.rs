//! Timelines' audio cues, played through the engine's [`AudioService`] (m5-a11y; the seam the
//! timeline and audio lanes left: Relatum `m5/audio/timeline-seam`). A cue `{sound, volume, at,
//! bus}` that fires while a timeline plays is played as `timeline:<path>` (so `audio_played
//! {by}` names the timeline, and captions can say what made a sound), on the cue's bus (default
//! `sfx`), at its point or entity when it has one. Any timeline's cues play, an agent's or the
//! hosted game's; the game still hears every cue in [`crate::game::Game::timeline_event`].
//!
//! **Presentation only:** nothing here writes the world or feeds the simulation; a missing
//! recipe, a render error, audio being off or a sound rendered too late is skipped and traced
//! (`timeline.cue_sound`), never a timeline error, so runs with and without audio hash the same.
//!
//! **Where a sound comes from:** a sound recipe `sounds/<name>.json` under the assets root (the
//! same files games and `audio_preview` use), rendered off the engine thread and cached by name.
//!
//! **Cost before work:** names are checked (1..64 of `[a-z0-9_-]`, so a cue can't name a path);
//! a recipe file over [`MAX_RECIPE_BYTES`] is refused unread; the recipe's own limits (60 s, the
//! synthesiser's caps) bound its render; a rendered sound over [`MAX_SOUND_S`] is dropped; the
//! cache holds at most [`MAX_CACHED`] sounds and [`MAX_CACHED_SAMPLES`] samples (least recently
//! used out first); at most [`MAX_LOADING`] renders run at once, each with at most
//! [`MAX_WAITING`] cues waiting, and [`MAX_CUES_PER_TICK`] cues are played a tick.

use crate::audio_service::AudioService;
use crate::game::{TimelineEvent, TimelineWhat};
use crate::Engine;
use specula_audio::{Bus, PlayParams, RenderOptions, Sound, Spatial};
use specula_tempus::{Cue, CueAt};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Largest recipe file read.
pub const MAX_RECIPE_BYTES: u64 = 64 << 10;
/// Longest cue kept (music belongs to music tracks and the game, not to cues).
pub const MAX_SOUND_S: f64 = 30.0;
/// Sounds cached at once, and their samples in all (about 32 MiB of f32).
pub const MAX_CACHED: usize = 64;
pub const MAX_CACHED_SAMPLES: usize = 8 << 20;
/// Renders in flight at once, and cues waiting for one.
pub const MAX_LOADING: usize = 4;
pub const MAX_WAITING: usize = 8;
/// Cues played in one delivery (a timeline may fire many in one tick after a hitch).
pub const MAX_CUES_PER_TICK: usize = 16;
/// A cue that waited longer than this for its render is skipped (late sound is wrong sound).
const STALE: Duration = Duration::from_millis(500);
/// A name that failed is tried again after this long (an agent may have written the recipe).
const RETRY_FAILED: Duration = Duration::from_secs(5);
/// Failed names remembered at once (so a timeline that names one every tick doesn't render it
/// every tick). They are kept apart from the sounds' bound: a timeline full of misspelt cues
/// neither grows the cache without end nor crowds the ready sounds out of it (review R1 of
/// m5-a11y, finding 6).
pub const MAX_FAILED: usize = 32;

/// A cue on its way to the mixer: what to play, how, and who it is played as.
#[derive(Clone, Debug)]
struct Pending {
    by: String,
    gain: f32,
    bus: Bus,
    at: Option<[f32; 3]>,
    sound: String,
}

enum Entry {
    Loading { waiting: Vec<(Pending, Instant)> },
    Ready { sound: Sound, used: u64 },
    Failed { at: Instant },
}

/// The engine's player for timelines' cues.
#[derive(Default)]
pub struct TimelineAudio {
    audio: Option<Arc<AudioService>>,
    assets: PathBuf,
    cache: BTreeMap<String, Entry>,
    clock: u64,
    pub played: u64,
    pub skipped: u64,
}

/// Whether `name` may name a sound recipe: 1..64 of `a-z`, `0-9`, `_`, `-`.
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// Finds, reads and renders `assets/sounds/<name>.json` (on a worker).
fn render(assets: &Path, name: &str) -> Result<Sound, String> {
    let path = assets.join("sounds").join(format!("{name}.json"));
    let meta = std::fs::metadata(&path).map_err(|_| format!("no sound recipe sounds/{name}.json under the assets"))?;
    if !meta.is_file() {
        return Err(format!("sounds/{name}.json isn't a file"));
    }
    if meta.len() > MAX_RECIPE_BYTES {
        return Err(format!("sounds/{name}.json is {} bytes; a cue's recipe is at most {} KiB", meta.len(), MAX_RECIPE_BYTES >> 10));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("sounds/{name}.json: {e}"))?;
    let sound = specula_audio::load_recipe(&text, &RenderOptions::default()).map_err(|e| format!("sounds/{name}.json: {e}"))?;
    if sound.duration_s() > MAX_SOUND_S {
        return Err(format!("sounds/{name}.json renders {:.1} s; a cue is at most {MAX_SOUND_S} s", sound.duration_s()));
    }
    Ok(sound)
}

fn trace(p: &Pending, outcome: &str) {
    specula_core::trace_event!(&crate::trace_sink::ENGINE, Debug, "timeline.cue_sound", by = p.by.as_str(), sound = p.sound.as_str(), outcome = outcome);
}

impl TimelineAudio {
    /// Where recipes are and the service to play them on.
    pub fn configure(&mut self, assets: &Path, audio: Arc<AudioService>) {
        self.assets = assets.to_owned();
        self.audio = Some(audio);
    }

    fn enabled(&self) -> bool {
        self.audio.as_ref().is_some_and(|a| a.enabled())
    }

    fn loading(&self) -> usize {
        self.cache.values().filter(|e| matches!(e, Entry::Loading { .. })).count()
    }

    fn cached_samples(&self) -> usize {
        self.cache.values().map(|e| if let Entry::Ready { sound, .. } = e { sound.samples().len() } else { 0 }).sum()
    }

    fn failed(&self) -> usize {
        self.cache.values().filter(|e| matches!(e, Entry::Failed { .. })).count()
    }

    /// Remembers that `name` failed, within [`MAX_FAILED`]: failures old enough to be tried again
    /// are forgotten first, then the oldest.
    fn remember_failure(&mut self, name: String, now: Instant) {
        self.cache.retain(|_, e| !matches!(e, Entry::Failed { at } if now.saturating_duration_since(*at) >= RETRY_FAILED));
        while self.failed() >= MAX_FAILED {
            let oldest = self.cache.iter().filter_map(|(k, e)| if let Entry::Failed { at } = e { Some((*at, k.clone())) } else { None }).min();
            match oldest {
                Some((_, k)) => {
                    self.cache.remove(&k);
                }
                None => break,
            }
        }
        self.cache.insert(name, Entry::Failed { at: now });
    }

    /// Drops the least recently used ready sounds until `room` more samples and one entry fit
    /// (failed names don't count: they hold no sound and have their own bound).
    fn make_room(&mut self, room: usize) {
        while self.cache.len() - self.failed() >= MAX_CACHED || self.cached_samples() + room > MAX_CACHED_SAMPLES {
            let oldest = self.cache.iter().filter_map(|(k, e)| if let Entry::Ready { used, .. } = e { Some((*used, k.clone())) } else { None }).min();
            match oldest {
                Some((_, k)) => {
                    self.cache.remove(&k);
                }
                None => break,
            }
        }
    }

    fn play(&mut self, p: &Pending, sound: &Sound) {
        let Some(audio) = &self.audio else { return };
        let spatial = p.at.map(|position| Spatial { position, min_distance: 25.0, max_distance: 150.0, rolloff: 0.7, lowpass: true });
        let played = audio.play(&p.by, sound, PlayParams { bus: p.bus, gain: p.gain, spatial, ..PlayParams::default() }).is_some();
        if played {
            self.played += 1;
        } else {
            self.skipped += 1;
        }
        trace(p, if played { "played" } else { "no_voice" });
    }

    fn skip(&mut self, p: &Pending, why: &str) {
        self.skipped += 1;
        trace(p, why);
    }

    pub fn to_json(&self) -> specula_core::Json {
        use specula_core::Json;
        Json::obj([
            ("played", Json::from(self.played)),
            ("skipped", Json::from(self.skipped)),
            ("cached", Json::from(self.cache.values().filter(|e| matches!(e, Entry::Ready { .. })).count())),
            ("loading", Json::from(self.loading())),
            ("failed", Json::from(self.failed())),
        ])
    }
}

/// The cue as the mixer takes it: gain, bus (unknown names fall back to `sfx`), position.
fn pending(cue: &Cue, path: &str, at_entity: impl Fn(&str) -> Option<[f32; 3]>) -> Pending {
    let at = match &cue.at {
        Some(CueAt::Point(p)) => Some(p.map(|v| v as f32)),
        Some(CueAt::Entity(e)) => at_entity(e),
        None => None,
    };
    let bus = cue.bus.as_deref().and_then(Bus::from_name).unwrap_or(Bus::Sfx);
    let gain = if cue.volume.is_finite() { cue.volume.clamp(0.0, 4.0) as f32 } else { 1.0 };
    Pending { by: format!("timeline:{path}"), gain, bus, at, sound: cue.sound.clone() }
}

impl Engine {
    /// Plays the audio cues among the timeline events about to be delivered (called before the
    /// game hears them). Cheap when there are none.
    pub(crate) fn timeline_cues(&mut self, events: &[TimelineEvent]) {
        let mut n = 0;
        for e in events {
            let TimelineWhat::Cue(cue) = &e.what else { continue };
            let world = &self.world;
            let p = pending(cue, &e.path, |key| {
                let id = world.resolve(&crate::world::EntityKey::Path(key.to_owned())).ok()?;
                let v = world.get(id)?.world_position();
                Some([v.x, v.y, v.z])
            });
            n += 1;
            if n > MAX_CUES_PER_TICK {
                self.timeline_audio.skip(&p, "too_many");
                continue;
            }
            self.timeline_cue(p);
        }
    }

    fn timeline_cue(&mut self, p: Pending) {
        let ta = &mut self.timeline_audio;
        if !ta.enabled() {
            ta.skip(&p, "audio_off");
            return;
        }
        if !valid_name(&p.sound) {
            ta.skip(&p, "bad_name");
            return;
        }
        ta.clock += 1;
        let clock = ta.clock;
        match ta.cache.get_mut(&p.sound) {
            Some(Entry::Ready { sound, used }) => {
                *used = clock;
                let sound = sound.clone();
                ta.play(&p, &sound);
                return;
            }
            Some(Entry::Loading { waiting }) => {
                if waiting.len() < MAX_WAITING {
                    waiting.push((p, Instant::now()));
                } else {
                    ta.skip(&p, "busy");
                }
                return;
            }
            Some(Entry::Failed { at }) if at.elapsed() < RETRY_FAILED => {
                ta.skip(&p, "failed");
                return;
            }
            _ => {}
        }
        if ta.loading() >= MAX_LOADING {
            ta.skip(&p, "busy");
            return;
        }
        let (assets, name) = (ta.assets.clone(), p.sound.clone());
        ta.cache.insert(name.clone(), Entry::Loading { waiting: vec![(p, Instant::now())] });
        let key = name.clone();
        let refused = name.clone();
        let job = crate::workers::Offload::engine(crate::workers::JobKind::TimelineSound).class(crate::schedule::Class::Disk);
        let started = self.offload(job, move |_| render(&assets, &name), move |e: &mut Engine, r| {
            let result = r.map_err(|f| format!("{f:?}")).and_then(|r| r);
            let ta = &mut e.timeline_audio;
            let Some(Entry::Loading { waiting }) = ta.cache.remove(&key) else { return };
            match result {
                Ok(sound) => {
                    ta.make_room(sound.samples().len());
                    ta.clock += 1;
                    let used = ta.clock;
                    ta.cache.insert(key, Entry::Ready { sound: sound.clone(), used });
                    for (p, asked) in waiting {
                        if asked.elapsed() > STALE {
                            ta.skip(&p, "late");
                        } else {
                            ta.play(&p, &sound);
                        }
                    }
                }
                Err(why) => {
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "timeline.cue_sound_failed", sound = key.as_str(), why = why.as_str());
                    for (p, _) in &waiting {
                        ta.skip(p, "failed");
                    }
                    ta.remember_failure(key, Instant::now());
                }
            }
        });
        if started.is_err() {
            if let Some(Entry::Loading { waiting }) = self.timeline_audio.cache.remove(&refused) {
                for (p, _) in &waiting {
                    self.timeline_audio.skip(p, "busy");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review R1 of m5-a11y, finding 6: every failing name used to stay for the run, and once
    /// 64 had failed every new sound evicted every ready one.
    #[test]
    fn failed_names_are_bounded_and_never_crowd_out_sounds() {
        let mut ta = TimelineAudio::default();
        let sound = |n: &str| Sound::new(n, 48_000, 1, vec![0.0; 480]).unwrap();
        for i in 0..3 {
            ta.clock += 1;
            ta.cache.insert(format!("ready_{i}"), Entry::Ready { sound: sound("s"), used: ta.clock });
        }
        let now = Instant::now();
        for i in 0..(MAX_CACHED + MAX_FAILED + 10) {
            ta.remember_failure(format!("typo_{i}"), now);
        }
        assert_eq!(ta.failed(), MAX_FAILED, "failures are bounded");
        assert_eq!(ta.cache.len(), 3 + MAX_FAILED);
        // A new sound finds room without a ready one going.
        ta.make_room(480);
        assert_eq!(ta.cache.values().filter(|e| matches!(e, Entry::Ready { .. })).count(), 3, "the ready sounds stay");
        assert_eq!(ta.to_json().get("failed").and_then(specula_core::Json::as_u64), Some(MAX_FAILED as u64));
        // Failures old enough to retry are forgotten when the next one is remembered.
        ta.remember_failure("late".into(), now + RETRY_FAILED);
        assert_eq!(ta.failed(), 1);
        // The sounds' own bound still holds.
        for i in 0..(MAX_CACHED + 5) {
            ta.make_room(480);
            ta.clock += 1;
            ta.cache.insert(format!("more_{i}"), Entry::Ready { sound: sound("s"), used: ta.clock });
        }
        assert!(ta.cache.len() - ta.failed() <= MAX_CACHED);
    }

    #[test]
    fn cue_names_cant_name_paths() {
        assert!(valid_name("brazier_whoosh") && valid_name("a-1"));
        for bad in ["", "../x", "a/b", "A", "x.json", &"a".repeat(65)] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn cues_become_plays_on_their_bus_and_place() {
        let cue = Cue { t: 0.0, sound: "bell".into(), volume: 9.0, at: Some(CueAt::Entity("/statue".into())), bus: Some("voice".into()) };
        let p = pending(&cue, "assets/timelines/intro.json", |e| (e == "/statue").then_some([1.0, 2.0, 3.0]));
        assert_eq!((p.by.as_str(), p.bus, p.gain, p.at), ("timeline:assets/timelines/intro.json", Bus::Voice, 4.0, Some([1.0, 2.0, 3.0])));
        let cue = Cue { t: 0.0, sound: "bell".into(), volume: f64::NAN, at: Some(CueAt::Point([4.0, 5.0, 6.0])), bus: Some("nope".into()) };
        let p = pending(&cue, "t", |_| None);
        assert_eq!((p.bus, p.gain, p.at), (Bus::Sfx, 1.0, Some([4.0, 5.0, 6.0])));
    }

    #[test]
    fn missing_big_and_long_recipes_are_refused() {
        let dir = std::env::temp_dir().join(format!("specula-timeline-audio-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sounds")).unwrap();
        assert!(render(&dir, "nope").unwrap_err().contains("no sound recipe"));
        std::fs::write(dir.join("sounds/big.json"), vec![b' '; (MAX_RECIPE_BYTES + 1) as usize]).unwrap();
        assert!(render(&dir, "big").unwrap_err().contains("at most"));
        std::fs::write(dir.join("sounds/bad.json"), "{\"sound\": 2}").unwrap();
        assert!(render(&dir, "bad").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_cache_evicts_the_least_recently_used_sound() {
        let mut s = TimelineAudio::default();
        let tone = Sound::new("t", 48_000, 1, vec![0.0; 1000]).unwrap();
        for i in 0..MAX_CACHED {
            s.cache.insert(format!("s{i}"), Entry::Ready { sound: tone.clone(), used: i as u64 + 10 });
        }
        if let Some(Entry::Ready { used, .. }) = s.cache.get_mut("s0") {
            *used = 1000;
        }
        s.make_room(1000);
        assert_eq!(s.cache.len(), MAX_CACHED - 1);
        assert!(s.cache.contains_key("s0") && !s.cache.contains_key("s1"));
    }
}

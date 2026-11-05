//! Timeline playback (docs/design/TIMELINE.md): timelines are project files
//! (`assets/timelines/*.json`, kind `timeline`), evaluated by `specula-tempus` on the engine's
//! fixed tick.
//!
//! - **Position in ticks.** A playback's position is a whole number of ticks; its time is
//!   `ticks / hz`. Each simulation tick moves it one tick on (never while the clock is paused), so
//!   a playback is part of the deterministic simulation: the same start tick gives the same
//!   writes at the same ticks.
//! - **Writes.** Entity, anim and vfx tracks write the world through the write policy
//!   ([`Rule::Timeline`]) as the agent who started the playback, one commit per tick that
//!   changes something (only changed values are written). A claim that refuses a write pauses the
//!   playback as `blocked`, with the refusal in its status and a trace event: playback never
//!   fights claims silently.
//! - **Parsed off the engine thread.** A timeline is parsed when its file is validated (on the
//!   writer's protocol thread) and kept by its text ([`parsed`]); the engine thread parses only
//!   a file it never saw validated (imported from disk), at most 256 KiB.
//! - **Scrubbing equals playing.** The values are a pure function of time
//!   (`specula_tempus::eval`); anim and vfx keys start at the world time of their key
//!   (`origin_s + key.t`), and a scrub rewrites every anim and vfx track from the new anchor
//!   (before a track's first key, the entity's own anim or effect from before the playback comes
//!   back), so the state after scrubbing to `t` is the state after playing to it.
//!   Things that happen once (events, audio cues, subtitle lines starting) fire only while
//!   playing.
//! - **The window.** A playback started with `window` drives the human's window camera from its
//!   rail every frame (interpolated between ticks) until it ends; a human's input skips a
//!   skippable one and otherwise takes the view back, and the camera the window had before comes
//!   back when it ends (`restore`).
//! - **The game.** Starts, ends, events, cues and subtitle lines go to the hosted game
//!   ([`Game::timeline_event`]) on the tick they happen; the game plays its own presentation
//!   timelines through [`Game::timeline_requests`].

use super::*;
use crate::game::{TimelineEvent, TimelineRequest, TimelineWhat};
use crate::project_kinds::Problem;
use crate::world::{EntityKey, LookPatch, Op, UpdateSpec};
use crate::write_policy::Rule;
use specula_core::math::Vec3;
use specula_tempus::eval::{self, EntityState};
use specula_tempus::{Fired, Timeline, TrackData};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Agents' playbacks at once, and per agent (identity key); the hosted game has its own
/// [`MAX_GAME_PLAYBACKS`], so agents can never crowd out its intro (lead review).
pub const MAX_PLAYBACKS: usize = 4;
pub const MAX_PER_AGENT: usize = 2;
pub const MAX_GAME_PLAYBACKS: usize = 2;
/// A paused or blocked playback ends (`expired`) after this long untouched.
pub const IDLE_LEASE: std::time::Duration = std::time::Duration::from_secs(120);
/// How often closed sessions and idle playbacks are looked for.
const SWEEP_EVERY: std::time::Duration = std::time::Duration::from_millis(250);
/// Ended playbacks remembered for `timeline_get`.
const MAX_ENDED: usize = 8;
/// Parsed timelines kept (by path and version).
const MAX_CACHED: usize = 16;
/// Events waiting for the game at most (the oldest go first; each fires at most once).
const MAX_PENDING: usize = 1024;

/// Text of parsed timelines kept, at most (a byte budget, least recently used out first): every
/// timeline of a project of up to 4,096 files of 4 KiB, or 64 of the largest (256 KiB) fit.
const PARSED_BUDGET: usize = 16 << 20;

/// Timelines parsed when their files were validated or read from disk, by text hash; the text is
/// kept and compared, so a hash collision never hands out another file's timeline.
static PARSED: Mutex<Parsed> = Mutex::new(Parsed { map: BTreeMap::new(), bytes: 0, clock: 0 });

/// A parsed timeline and its anim and vfx problems (`(field, message)`), both worked out off the
/// engine thread when the file was validated or read.
#[derive(Clone)]
pub(crate) struct Ready {
    pub(crate) tl: Arc<Timeline>,
    pub(crate) problems: Arc<Vec<(String, String)>>,
}

/// The parsed-timeline cache: `hash -> (text, parsed, last use)`, its text bytes, a use clock.
struct Parsed {
    map: BTreeMap<u64, (Arc<str>, Ready, u64)>,
    bytes: usize,
    clock: u64,
}

impl Parsed {
    fn get(&mut self, hash: u64, ok: impl Fn(&str) -> bool) -> Option<Ready> {
        self.clock += 1;
        let now = self.clock;
        let e = self.map.get_mut(&hash).filter(|e| ok(&e.0))?;
        e.2 = now;
        Some(e.1.clone())
    }

    fn put(&mut self, hash: u64, text: &str, ready: Ready) {
        self.put_within(hash, text, ready, PARSED_BUDGET);
    }

    fn put_within(&mut self, hash: u64, text: &str, tl: Ready, budget: usize) {
        if let Some(old) = self.map.remove(&hash) {
            self.bytes -= old.0.len();
        }
        // Least recently used out first, until the new text fits.
        while self.bytes + text.len() > budget {
            let Some(oldest) = self.map.iter().min_by_key(|(_, e)| e.2).map(|(k, _)| *k) else { break };
            if let Some(e) = self.map.remove(&oldest) {
                self.bytes -= e.0.len();
            }
        }
        self.clock += 1;
        self.bytes += text.len();
        self.map.insert(hash, (Arc::from(text), tl, self.clock));
    }
}

/// The timeline `text` (whose [`crate::project::text_hash`] is `hash`, as the project store
/// keeps it) parses to, from the cache when it is there.
pub(crate) fn parsed(text: &str, hash: u64) -> Result<Arc<Timeline>, specula_tempus::TimelineError> {
    ready(text, hash).map(|r| r.tl)
}

/// [`parsed`] with the anim and vfx problems.
pub(crate) fn ready(text: &str, hash: u64) -> Result<Ready, specula_tempus::TimelineError> {
    cached(text, hash).map_or_else(|| parse_and_keep(text, hash), Ok)
}

/// The timeline of `text` if it was parsed before (the text compared, never its hash alone).
pub(crate) fn cached(text: &str, hash: u64) -> Option<Ready> {
    PARSED.lock().unwrap_or_else(|p| p.into_inner()).get(hash, |t| t == text)
}

/// For listings only: the timeline parsed from a text with this hash and length, without reading
/// the text (a listing's summary; never what plays).
pub(crate) fn summary_of(hash: u64, len: usize) -> Option<Arc<Timeline>> {
    PARSED.lock().unwrap_or_else(|p| p.into_inner()).get(hash, |t| t.len() == len).map(|r| r.tl)
}

/// Parses a timeline file read from disk, off the engine thread (the project watcher and scans
/// call it as they read one), so playing or listing it never parses on the engine thread while
/// the project's timelines fit [`PARSED_BUDGET`].
pub(crate) fn warm(text: &str, hash: u64) {
    if cached(text, hash).is_none() {
        let _ = parse_and_keep(text, hash);
    }
}

fn parse_and_keep(text: &str, h: u64) -> Result<Ready, specula_tempus::TimelineError> {
    let tl = Timeline::parse_text(text)?;
    let ready = Ready { problems: Arc::new(trigger_problems(&tl)), tl: Arc::new(tl) };
    PARSED.lock().unwrap_or_else(|p| p.into_inner()).put(h, text, ready.clone());
    Ok(ready)
}

/// What a track last wrote (only changes are written again).
#[derive(Clone, Debug, PartialEq)]
enum Written {
    Entity(EntityState),
    Trigger(Option<usize>),
}

/// A written trigger whose start must be written again (a scrub moved the anchor).
const REANCHOR: usize = usize::MAX;

/// What an anim or vfx track's entity played before the playback: put back when a scrub goes
/// before the track's first key. Kept by (target, kind): a replay of an edited file can move
/// tracks to other indices or other entities (review round 2).
#[derive(Clone, Debug)]
enum Original {
    Anim(Option<crate::world::AnimSpec>),
    Vfx(Option<specula_scintilla::VfxSpec>),
}

/// The writes a time needs and what they write: `(ops, written values)`.
type Writes = (Vec<Op>, Vec<(usize, Written)>);

/// What a scrub compares with: every written trigger marked to be written again (the anchor
/// moved: their starts change, and before their first key the original comes back), and no entity
/// values, so every channel is written again in the scrub's one commit, whatever another agent or
/// an undo did to the entity meanwhile (lead review: a scrub compared with its own cache and left a
/// moved entity where it was).
fn reanchored(written: &BTreeMap<usize, Written>) -> BTreeMap<usize, Written> {
    let mut w = written.clone();
    w.retain(|_, v| !matches!(v, Written::Entity(_)));
    for v in w.values_mut() {
        if let Written::Trigger(Some(k)) = v {
            *k = REANCHOR;
        }
    }
    w
}

/// One timeline playing (or paused, or blocked).
pub(crate) struct Playback {
    pub(crate) path: String,
    pub(crate) version: u64,
    pub(crate) timeline: Arc<Timeline>,
    /// Who started it: its writes are journaled under this name and claims are checked as this
    /// identity key.
    pub(crate) by: String,
    pub(crate) key: String,
    pub(crate) from_game: bool,
    /// Position and end, in ticks.
    pub(crate) pos: u64,
    pub(crate) end: u64,
    pub(crate) paused: bool,
    /// World time at position 0 (anim and vfx keys start at `origin_s + key.t`).
    origin_s: f64,
    /// The last position whose once-only things fired (`None`: none yet, so keys at the start
    /// position fire too).
    fired_to: Option<u64>,
    /// It drives the window camera.
    pub(crate) window: bool,
    pub(crate) skippable: bool,
    /// The window camera to put back when it ends (`Some`: restore).
    restore: Option<Option<Camera>>,
    written: BTreeMap<usize, Written>,
    /// Anim and vfx tracks' entities as they were before it started, by (target, kind).
    originals: BTreeMap<(String, &'static str), Original>,
    /// Why it stopped writing (a claim, a missing entity), as the refusal.
    pub(crate) blocked: Option<Json>,
    pub(crate) started_tick: u64,
    pub(crate) commits: u64,
    /// When it was last paused or blocked (`None`: moving): an idle playback ends after
    /// [`IDLE_LEASE`] so it never holds a slot for good.
    pub(crate) held_since: Option<std::time::Instant>,
}

impl Playback {
    pub(crate) fn t(&self, hz: f64) -> f64 {
        self.pos as f64 / hz
    }

    pub(crate) fn state(&self) -> &'static str {
        if self.blocked.is_some() {
            "blocked"
        } else if self.paused {
            "paused"
        } else {
            "playing"
        }
    }

    fn to_json(&self, hz: f64, world: &World) -> Json {
        let t = self.t(hz);
        let mut j = Json::obj([
            ("path", Json::from(self.path.as_str())),
            ("version", Json::from(self.version)),
            ("state", Json::from(self.state())),
            ("t", Json::from(round(t, 4))),
            ("tick", Json::from(self.pos)),
            ("duration", Json::from(round(self.timeline.duration, 4))),
            ("by", Json::from(self.by.as_str())),
            ("window", Json::from(self.window)),
            ("skippable", Json::from(self.skippable)),
            ("writes_world", Json::from(self.timeline.writes_world())),
            ("commits", Json::from(self.commits)),
            ("started_tick", Json::from(self.started_tick)),
        ]);
        if self.from_game {
            j.set("from_game", Json::from(true));
        }
        if let Some(b) = &self.blocked {
            j.set("blocked", b.clone());
        }
        let lines: Vec<Json> = self.timeline.lines_at(t).into_iter().take(8).map(|(_, l)| line_json(l)).collect();
        if !lines.is_empty() {
            j.set("subtitles", Json::Arr(lines));
        }
        if let Some(p) = self.timeline.camera_at(t, &|path| entity_centre(world, path)) {
            j.set("camera", pose_json(&p, &self.timeline));
        }
        j
    }
}

/// The engine's playbacks.
#[derive(Default)]
pub(crate) struct Player {
    pub(crate) playing: Vec<Playback>,
    pub(crate) ended: VecDeque<Json>,
    cache: BTreeMap<String, (u64, Arc<Timeline>)>,
    /// Events for the game, delivered in order at the end of the tick or frame that made them.
    pending: VecDeque<TimelineEvent>,
    /// Window frames drawn through a rail camera, and playbacks a human skipped (engine_info).
    pub(crate) window_frames: u64,
    pub(crate) skipped: u64,
    pub(crate) events_fired: u64,
    /// Gamepad buttons held last frame while a timeline drives the window (`None`: not yet seen).
    pad_prev: Option<[u32; specula_platform::MAX_GAMEPADS]>,
    /// When closed sessions and idle playbacks were last looked for.
    last_sweep: Option<std::time::Instant>,
}

impl Player {
    /// The playback driving the window, if one does.
    pub(crate) fn window_path(&self) -> Option<&str> {
        self.playing.iter().find(|p| p.window).map(|p| p.path.as_str())
    }

    fn index(&self, path: &str) -> Option<usize> {
        self.playing.iter().position(|p| p.path == path)
    }

    fn push(&mut self, e: TimelineEvent) {
        if self.pending.len() >= MAX_PENDING {
            self.pending.pop_front();
        }
        self.pending.push_back(e);
    }

    pub(crate) fn to_json(&self, hz: f64, world: &World) -> Json {
        Json::obj([
            ("playing", Json::arr(self.playing.iter().map(|p| p.to_json(hz, world)))),
            ("ended", Json::arr(self.ended.iter().cloned())),
            ("window_frames", Json::from(self.window_frames)),
            ("skipped", Json::from(self.skipped)),
            ("events_fired", Json::from(self.events_fired)),
        ])
    }
}

pub(crate) fn round(x: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (x * k).round() / k
}

fn v3(v: [f64; 3]) -> Json {
    Json::arr(v.iter().map(|c| Json::from(round(*c, 3))))
}

pub(crate) fn line_json(l: &specula_tempus::Line) -> Json {
    let mut f = vec![("t", Json::from(l.t)), ("duration", Json::from(l.duration)), ("text", Json::from(l.text.as_str()))];
    if let Some(s) = &l.speaker {
        f.push(("speaker", Json::from(s.as_str())));
    }
    Json::obj(f)
}

pub(crate) fn pose_json(p: &specula_tempus::Pose, tl: &Timeline) -> Json {
    let mut j = Json::obj([
        ("track", Json::from(tl.tracks[p.track].name.as_str())),
        ("eye", v3(p.eye)),
        ("target", v3(p.target)),
        ("fov_y_deg", Json::from(round(p.fov_deg, 3))),
    ]);
    if p.roll_deg != 0.0 {
        j.set("roll_deg", Json::from(round(p.roll_deg, 3)));
    }
    if !p.look_resolved {
        j.set("look_missing", Json::from(true));
    }
    j
}

/// The centre of an entity's shown bounds (its position when it has none), by path.
pub(crate) fn entity_centre(world: &World, path: &str) -> Option<[f64; 3]> {
    let e = world.get(world.find_path(path).ok()?)?;
    let c = if e.local_bounds().is_some() {
        let (lo, hi) = e.world_aabb();
        (lo + hi) * 0.5
    } else {
        e.world_position()
    };
    Some([f64::from(c.x), f64::from(c.y), f64::from(c.z)])
}

/// The engine camera of a rail pose.
pub(crate) fn camera_of(p: &specula_tempus::Pose) -> Camera {
    let v = |a: [f64; 3]| Vec3::new(a[0] as f32, a[1] as f32, a[2] as f32);
    Camera::new(v(p.eye), v(p.target), p.fov_deg as f32).rolled(p.roll_deg as f32)
}

/// A timeline file's problems (project_kinds): the format, then what the engine plays: anim
/// and vfx values as `world_set` takes them. Whether targets exist is checked when it plays
/// (the world changes; `timeline_validate` reports them now).
pub(crate) fn validate_file(path: &str, text: &str) -> Vec<Problem> {
    let ready = match ready(text, crate::project::text_hash(text)) {
        Ok(r) => r,
        Err(e) => return vec![Problem { file: path.to_owned(), severity: "error", code: e.code.to_owned(), at: e.field.clone(), message: e.to_string(), offset: None }],
    };
    ready.problems.iter().cloned().map(|(at, message)| Problem { file: path.to_owned(), severity: "error", code: "invalid_value".into(), at, message, offset: None }).collect()
}

/// Anim and vfx keys whose value `world_set` would refuse: `(field, message)`.
pub(crate) fn trigger_problems(tl: &Timeline) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (i, t) in tl.tracks.iter().enumerate() {
        match &t.data {
            TrackData::Anim { keys, .. } => {
                for (k, key) in keys.iter().enumerate() {
                    if matches!(key.value, Json::Bool(false)) {
                        continue;
                    }
                    if let Err(e) = crate::world::AnimSpec::from_json(&key.value) {
                        out.push((format!("tracks[{i}].keys[{k}].anim"), e));
                    }
                }
            }
            TrackData::Vfx { keys, .. } => {
                for (k, key) in keys.iter().enumerate() {
                    if matches!(key.value, Json::Bool(false)) {
                        continue;
                    }
                    if let Err(e) = specula_scintilla::VfxSpec::parse(&key.value) {
                        out.push((format!("tracks[{i}].keys[{k}].vfx"), crate::vfx::error_text(&e)));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The writes that bring the timeline's entity, anim and vfx tracks to `t`, against what was
/// written (`written`; empty: everything). Returns the ops and the new written values.
fn ops_at(tl: &Timeline, origin_s: f64, written: &BTreeMap<usize, Written>, originals: &BTreeMap<(String, &'static str), Original>, t: f64) -> Result<Writes, WorldError> {
    let mut ops = Vec::new();
    let mut now = Vec::new();
    for (i, track) in tl.tracks.iter().enumerate() {
        let Some(target) = track.target() else { continue };
        let key = EntityKey::Path(target.to_owned());
        match &track.data {
            TrackData::Entity(e) => {
                let s = eval::entity_at(e, t);
                let before = match written.get(&i) {
                    Some(Written::Entity(b)) => Some(*b),
                    _ => None,
                };
                let changed = |new: Option<[f64; 3]>, old: Option<Option<[f64; 3]>>| new.filter(|n| old.flatten() != Some(*n));
                let f = |a: [f64; 3]| Vec3::new(a[0] as f32, a[1] as f32, a[2] as f32);
                let mut u = UpdateSpec {
                    position: changed(s.position, before.map(|b| b.position)).map(f),
                    rotation_deg: changed(s.rotation, before.map(|b| b.rotation)).map(f),
                    scale: changed(s.scale, before.map(|b| b.scale)).map(f),
                    color: changed(s.color, before.map(|b| b.color)).map(|c| [c[0] as f32, c[1] as f32, c[2] as f32]),
                    ..UpdateSpec::default()
                };
                if s.visible.is_some() && before.and_then(|b| b.visible) != s.visible {
                    u.visible = s.visible;
                }
                if let Some(em) = s.emissive.filter(|e| before.and_then(|b| b.emissive) != Some(*e)) {
                    u.look = LookPatch { emissive: Some(em as f32), ..LookPatch::default() };
                }
                if before != Some(s) {
                    now.push((i, Written::Entity(s)));
                }
                let any = u.position.is_some() || u.rotation_deg.is_some() || u.scale.is_some() || u.color.is_some() || u.visible.is_some() || !u.look.is_empty();
                if any {
                    ops.push(Op::Set(key, u));
                }
            }
            TrackData::Anim { keys, .. } | TrackData::Vfx { keys, .. } => {
                let k = eval::trigger_at(keys, t);
                let before = match written.get(&i) {
                    Some(Written::Trigger(b)) => Some(*b),
                    _ => None,
                };
                if before == Some(k) {
                    continue;
                }
                now.push((i, Written::Trigger(k)));
                let Some(k) = k else {
                    // Before the first key: the entity as it was before the playback (back to it
                    // when a scrub goes there after a key was written).
                    if matches!(before, Some(Some(_))) {
                        // Only an original of this track's own kind and entity.
                        match (originals.get(&(target.to_owned(), track.kind())), &track.data) {
                            (Some(Original::Anim(a)), TrackData::Anim { .. }) => ops.push(Op::Set(key, UpdateSpec { anim: Some(a.clone()), ..UpdateSpec::default() })),
                            (Some(Original::Vfx(v)), TrackData::Vfx { .. }) => ops.push(Op::Set(key, UpdateSpec { vfx: Some(v.clone()), ..UpdateSpec::default() })),
                            _ => {}
                        }
                    }
                    continue;
                };
                let key_t = keys[k].t;
                let value = &keys[k].value;
                let start = origin_s + key_t;
                let mut u = UpdateSpec::default();
                if matches!(track.data, TrackData::Anim { .. }) {
                    u.anim = Some(match value {
                        Json::Bool(false) => None,
                        v => {
                            let mut a = crate::world::AnimSpec::from_json(v).map_err(|m| WorldError::new("invalid_value", format!("track '{}' key {k}: {m}", track.name)))?;
                            a.start.get_or_insert(start);
                            Some(a)
                        }
                    });
                } else {
                    u.vfx = Some(match value {
                        Json::Bool(false) => None,
                        v => {
                            let mut s = specula_scintilla::VfxSpec::parse(v).map_err(|e| WorldError::new("invalid_value", format!("track '{}' key {k}: {}", track.name, crate::vfx::error_text(&e))))?;
                            s.start.get_or_insert(start);
                            Some(s)
                        }
                    });
                }
                ops.push(Op::Set(key, u));
            }
            _ => {}
        }
    }
    Ok((ops, now))
}

/// How a playback starts.
pub(crate) struct Start {
    pub(crate) path: String,
    pub(crate) from_s: f64,
    pub(crate) paused: bool,
    pub(crate) window: bool,
    pub(crate) skippable: bool,
    pub(crate) restore: bool,
    pub(crate) by: String,
    pub(crate) key: String,
    pub(crate) from_game: bool,
}

/// A refusal with its code and message (and fields).
pub(crate) fn refusal(code: &'static str, message: impl Into<String>) -> WorldError {
    WorldError::new(code, message)
}

impl Engine {
    fn hz(&self) -> f64 {
        f64::from(self.clock.hz())
    }

    /// The timeline at a project path, parsed (cached by version).
    pub(crate) fn timeline_load(&mut self, path: &str) -> Result<(Arc<Timeline>, u64), WorldError> {
        let Some(f) = self.world.files().get(path) else {
            let names: Vec<&str> = self.world.files().iter().filter(|(_, f)| f.kind == crate::project::FileKind::Timeline).map(|(p, _)| p.as_str()).take(16).collect();
            return Err(refusal("not_found", format!("no timeline at {path} (timelines are assets/timelines/*.json; there are: {})", if names.is_empty() { "none".to_owned() } else { names.join(", ") })));
        };
        if f.kind != crate::project::FileKind::Timeline {
            return Err(refusal("not_a_timeline", format!("{path} is a {} file, not a timeline (timelines are *.json in a folder named timelines)", f.kind.name())));
        }
        let version = f.version;
        if let Some((v, tl)) = self.timeline.cache.get(path) {
            if *v == version {
                return Ok((Arc::clone(tl), version));
            }
        }
        let (text, hash) = (Arc::clone(&f.text), f.hash);
        if cached(&text, hash).is_none() {
            // Neither validated nor warmed when read from disk (it fell out of the cache):
            // parsed here, bounded at 256 KiB.
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Debug, "timeline.parsed_on_engine", path = path, bytes = text.len());
        }
        let tl = parsed(&text, hash).map_err(|e| {
            let mut w = refusal("invalid_timeline", format!("{path} doesn't parse: {e} (timeline_validate lists every problem)"));
            w = w.with("problem", e.to_json());
            w
        })?;
        if self.timeline.cache.len() >= MAX_CACHED {
            let first = self.timeline.cache.keys().next().cloned();
            if let Some(k) = first {
                self.timeline.cache.remove(&k);
            }
        }
        self.timeline.cache.insert(path.to_owned(), (version, Arc::clone(&tl)));
        Ok((tl, version))
    }

    /// Starts (or replaces) a playback. Its first writes (the timeline's state at the start) are
    /// the caller's one commit. Answers the playback's status.
    pub(crate) fn timeline_start(&mut self, s: Start) -> Result<Json, WorldError> {
        let (tl, version) = self.timeline_load(&s.path)?;
        if s.from_game && tl.writes_world() {
            return Err(refusal("game_timeline_writes", format!("{}: a game's timeline is presentation (camera, audio, subtitles, events); it may not write entities (tracks {})", s.path, tl.targets().join(", "))));
        }
        let hz = self.hz();
        // The same timeline playing: its owner restarts it (ended only once the new start has
        // committed), and the hosted game's request takes it over from an agent (`replaced`: an
        // agent playing the game's file never blocks its cutscene; lead review R2); anyone else is
        // refused (busy).
        let replacing = self.timeline.index(&s.path).is_some();
        if let Some(p) = self.timeline.playing.iter().find(|p| p.path == s.path) {
            if p.key != s.key && !(s.from_game && !p.from_game) {
                return Err(crate::authority::refusal(crate::authority::Site::JobBusy, format!("{} is playing for {} (timeline_stop with force needs the lead)", s.path, p.by)).with("owner", p.by.clone()));
            }
        }
        // Slots: agents share MAX_PLAYBACKS, MAX_PER_AGENT each; the game has its own.
        let others = || self.timeline.playing.iter().filter(|p| p.path != s.path);
        let (all, mine, cap) = if s.from_game {
            (others().filter(|p| p.from_game).count(), 0, MAX_GAME_PLAYBACKS)
        } else {
            (others().filter(|p| !p.from_game).count(), others().filter(|p| p.key == s.key).count(), MAX_PLAYBACKS)
        };
        if all >= cap || mine >= MAX_PER_AGENT {
            let owners: Vec<Json> = self.timeline.playing.iter().map(|p| Json::obj([("path", Json::from(p.path.as_str())), ("by", Json::from(p.by.as_str())), ("state", Json::from(p.state()))])).collect();
            let why = if mine >= MAX_PER_AGENT { format!("you play {mine} timelines (at most {MAX_PER_AGENT} each): stop one first") } else { format!("{all} timelines are playing (at most {cap}): stop one first; paused and blocked ones end after {} s untouched", IDLE_LEASE.as_secs()) };
            return Err(crate::authority::refusal(crate::authority::Site::JobBusy, why).with("playing", Json::Arr(owners)).with("retry_after_s", 1));
        }
        // Every target exists, and no other playback writes it.
        let targets = tl.targets();
        let missing: Vec<&str> = targets.iter().copied().filter(|t| self.world.find_path(t).is_err()).collect();
        if !missing.is_empty() {
            return Err(refusal("target_missing", format!("{} writes entities the world doesn't have: {}", s.path, missing.join(", "))).with("missing", Json::arr(missing.iter().map(|m| Json::from(*m)))));
        }
        for p in self.timeline.playing.iter().filter(|p| p.path != s.path) {
            if let Some(t) = p.timeline.targets().into_iter().find(|t| targets.contains(t)) {
                return Err(crate::authority::refusal(crate::authority::Site::JobBusy, format!("{} is written by the timeline {} playing for {}", t, p.path, p.by)).with("owner", p.by.clone()).with("timeline", p.path.clone()));
            }
        }
        let end = ((tl.duration * hz).ceil() as u64).max(1);
        let pos = ((s.from_s.max(0.0) * hz).round() as u64).min(end);
        let origin_s = self.world.world_time() - pos as f64 / hz;
        let tick = self.clock.tick();
        let mut pb = Playback {
            path: s.path.clone(),
            version,
            timeline: Arc::clone(&tl),
            by: s.by.clone(),
            key: s.key.clone(),
            from_game: s.from_game,
            pos,
            end,
            paused: s.paused,
            origin_s,
            fired_to: None,
            window: false,
            skippable: s.skippable,
            restore: None,
            written: BTreeMap::new(),
            originals: BTreeMap::new(),
            blocked: None,
            started_tick: tick,
            commits: 0,
            held_since: s.paused.then(std::time::Instant::now),
        };
        // What anim and vfx tracks' entities played before, by (target, kind). A replaced
        // playback's own originals carry over (what was there before any of it), and its written
        // triggers on the same (target, kind) are marked to be written again, so restarting to
        // `t` gives what scrubbing to `t` gives (review round 2).
        let old = self.timeline.playing.iter().find(|p| p.path == s.path);
        let carried = old.map(|p| p.originals.clone()).unwrap_or_default();
        let old_triggers: Vec<(String, &'static str)> = old
            .map(|p| {
                p.written
                    .iter()
                    .filter(|(_, w)| matches!(w, Written::Trigger(Some(_))))
                    .filter_map(|(i, _)| p.timeline.tracks.get(*i).and_then(|tr| tr.target().map(|t| (t.to_owned(), tr.kind()))))
                    .collect()
            })
            .unwrap_or_default();
        for (i, tr) in tl.tracks.iter().enumerate() {
            let Some(target) = tr.target() else { continue };
            let k = (target.to_owned(), tr.kind());
            if old_triggers.contains(&k) {
                pb.written.insert(i, Written::Trigger(Some(REANCHOR)));
            }
            if pb.originals.contains_key(&k) {
                continue;
            }
            if let Some(o) = carried.get(&k) {
                pb.originals.insert(k, o.clone());
                continue;
            }
            let ent = self.world.find_path(target).ok().and_then(|id| self.world.get(id));
            match (&tr.data, ent) {
                (TrackData::Anim { .. }, Some(e)) => {
                    pb.originals.insert(k, Original::Anim(e.anim.clone()));
                }
                (TrackData::Vfx { .. }, Some(e)) => {
                    pb.originals.insert(k, Original::Vfx(e.vfx.clone()));
                }
                _ => {}
            }
        }
        // The state at the start: the caller's commit (claims as the caller).
        let (ops, now) = ops_at(&tl, origin_s, &pb.written, &pb.originals, pos as f64 / hz)?;
        if !ops.is_empty() {
            let by = s.by.clone();
            self.world.commit(&mut self.swarm, tick, Writer::new(&s.by, Some(&s.key)), Kind::Batch, Rule::Timeline, |wd| wd.apply_batch(ops, &by).map_err(|(_, e)| e))?;
            pb.commits += 1;
        }
        pb.written.extend(now);
        if replacing {
            if let Some(i) = self.timeline.index(&s.path) {
                self.timeline_end(i, "replaced", None);
            }
        }
        let t0 = pos as f64 / hz;
        self.timeline.push(TimelineEvent { path: s.path.clone(), t: t0, what: TimelineWhat::Started });
        if !s.paused {
            Self::fire(&mut self.timeline, &tl, &s.path, None, pos, hz);
            pb.fired_to = Some(pos);
        }
        if s.window && self.windowed {
            // One playback drives the window; the camera to restore stays the one before any.
            let inherited = self.timeline.playing.iter_mut().find(|p| p.window).and_then(|p| {
                p.window = false;
                p.restore.take()
            });
            pb.window = true;
            pb.restore = inherited.or(if s.restore { Some(self.window_camera) } else { None });
            // A timeline takes the view over an agent's window_view.
            self.window_follow = None;
        }
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "timeline.play", path = s.path.as_str(), by = s.by.as_str(), t = t0, window = pb.window);
        self.timeline.playing.push(pb);
        let out = self.timeline.playing.last().map(|p| p.to_json(hz, &self.world)).unwrap_or(Json::Null);
        self.timeline_deliver();
        Ok(out)
    }

    /// Queues what fires once in `(from, to]` (`[to, to]` when `from` is `None`).
    fn fire(player: &mut Player, tl: &Timeline, path: &str, from: Option<u64>, to: u64, hz: f64) {
        let (a, include) = match from {
            Some(f) => (f as f64 / hz, false),
            None => (to as f64 / hz, true),
        };
        for f in tl.fired(a, to as f64 / hz, include) {
            let (t, what) = match f {
                Fired::Cue { cue, .. } => {
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Debug, "timeline.cue", path = path, sound = cue.sound.as_str(), t = cue.t);
                    (cue.t, TimelineWhat::Cue(cue.clone()))
                }
                Fired::Event { event, .. } => (event.t, TimelineWhat::Event { name: event.name.clone(), data: event.data.clone() }),
                Fired::Line { line, .. } => (line.t, TimelineWhat::Subtitle(line.clone())),
            };
            player.events_fired += 1;
            player.push(TimelineEvent { path: path.to_owned(), t, what });
        }
    }

    /// Ends playback `i` (`how`: finished, skipped, stopped, replaced, blocked, world_replaced):
    /// gives the window back (restoring its camera) and tells the game.
    pub(crate) fn timeline_end(&mut self, i: usize, how: &'static str, error: Option<String>) -> Json {
        let hz = self.hz();
        let pb = self.timeline.playing.remove(i);
        if pb.window {
            if let Some(cam) = pb.restore {
                self.window_camera = cam;
            }
        }
        let t = pb.t(hz);
        let summary = Json::obj([
            ("path", Json::from(pb.path.as_str())),
            ("how", Json::from(how)),
            ("t", Json::from(round(t, 4))),
            ("by", Json::from(pb.by.as_str())),
            ("commits", Json::from(pb.commits)),
            ("ended_tick", Json::from(self.clock.tick())),
        ]);
        if self.timeline.ended.len() >= MAX_ENDED {
            self.timeline.ended.pop_front();
        }
        self.timeline.ended.push_back(summary.clone());
        if how == "skipped" {
            self.timeline.skipped += 1;
        }
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "timeline.end", path = pb.path.as_str(), how = how, t = t);
        self.timeline.push(TimelineEvent { path: pb.path, t, what: TimelineWhat::Ended { how, error } });
        summary
    }

    /// One fixed tick of every playing timeline: writes, then once-only things, then ends.
    pub(crate) fn timeline_tick(&mut self) {
        if self.timeline.playing.is_empty() {
            return;
        }
        let _z = profile::zone("timeline");
        let hz = self.hz();
        let tick = self.clock.tick();
        let mut finished = Vec::new();
        for i in 0..self.timeline.playing.len() {
            let pb = &self.timeline.playing[i];
            if pb.paused || pb.blocked.is_some() {
                continue;
            }
            let new = (pb.pos + 1).min(pb.end);
            let tl = Arc::clone(&pb.timeline);
            let t = new as f64 / hz;
            let step = ops_at(&tl, pb.origin_s, &pb.written, &pb.originals, t);
            let result = step.and_then(|(ops, now)| {
                if ops.is_empty() {
                    return Ok((now, false));
                }
                let (by, key) = (pb.by.clone(), pb.key.clone());
                self.world.commit(&mut self.swarm, tick, Writer::new(&by, Some(&key)), Kind::Batch, Rule::Timeline, |wd| wd.apply_batch(ops, &by).map_err(|(_, e)| e)).map(|_| (now, true))
            });
            let pb = &mut self.timeline.playing[i];
            match result {
                Ok((now, committed)) => {
                    pb.written.extend(now);
                    pb.commits += u64::from(committed);
                }
                Err(e) => {
                    // Never written over a claim: paused as blocked, said loudly.
                    let mut j = e.to_json();
                    j.set("t", Json::from(round(t, 4)));
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "timeline.blocked", path = pb.path.as_str(), by = pb.by.as_str(), code = e.code, message = e.message.as_str());
                    pb.blocked = Some(j);
                    pb.held_since = Some(std::time::Instant::now());
                    continue;
                }
            }
            let from = pb.fired_to;
            let path = pb.path.clone();
            pb.fired_to = Some(new);
            pb.pos = new;
            let done = new >= pb.end;
            Self::fire(&mut self.timeline, &tl, &path, from, new, hz);
            if done {
                finished.push(path);
            }
        }
        for path in finished {
            if let Some(i) = self.timeline.index(&path) {
                self.timeline_end(i, "finished", None);
            }
        }
        self.timeline_deliver();
    }

    /// Hands queued events to the hosted game, in order.
    pub(crate) fn timeline_deliver(&mut self) {
        if self.timeline.pending.is_empty() {
            return;
        }
        if self.game.is_none() {
            self.timeline.pending.clear();
            return;
        }
        while let Some(e) = self.timeline.pending.pop_front() {
            self.game_call(sim_writes::GameCall::Timeline(&e));
        }
    }

    /// The window's frame: the rail camera of the playback that drives the window, at the
    /// frame's time between ticks. A human's input skips a skippable playback, and otherwise
    /// takes the view back. Returns whether the window looks through a rail camera this frame.
    pub(crate) fn timeline_window(&mut self, human_input: bool) -> (bool, bool) {
        let Some(i) = self.timeline.playing.iter().position(|p| p.window) else { return (false, false) };
        if human_input {
            if self.timeline.playing[i].skippable {
                self.timeline_end(i, "skipped", None);
            } else {
                let p = &mut self.timeline.playing[i];
                p.window = false;
                if let Some(cam) = p.restore.take() {
                    self.window_camera = cam;
                }
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "timeline.window_released", path = p.path.as_str());
            }
            self.timeline_deliver();
            return (false, true);
        }
        let hz = self.hz();
        let p = &self.timeline.playing[i];
        let moving = !p.paused && p.blocked.is_none() && !self.clock.paused() && p.pos < p.end;
        // Frames draw between the last two ticks (as the hosted game does).
        let t = if moving { (p.pos as f64 - 1.0 + f64::from(self.clock.alpha())).max(0.0) / hz } else { p.t(hz) };
        let world = &self.world;
        if let Some(pose) = p.timeline.camera_at(t, &|path| entity_centre(world, path)) {
            self.window_camera = Some(camera_of(&pose));
            self.timeline.window_frames += 1;
        }
        (true, false)
    }

    /// Whether a gamepad button went down this frame while a timeline drives the window (a
    /// skip by controller: nothing assumes a keyboard, F6). Buttons held when it started don't
    /// count until let go and pressed again.
    pub(crate) fn timeline_pad_pressed(&mut self) -> bool {
        if self.timeline.window_path().is_none() {
            self.timeline.pad_prev = None;
            return false;
        }
        let poll = !self.logic.reads_input();
        let pads = self.logic.pad_states(poll);
        let mut held = [0u32; specula_platform::MAX_GAMEPADS];
        for (h, p) in held.iter_mut().zip(pads) {
            *h = if p.connected { p.buttons } else { 0 };
        }
        let pressed = self.timeline.pad_prev.is_some_and(|prev| held.iter().zip(prev).any(|(h, p)| h & !p != 0));
        self.timeline.pad_prev = Some(held);
        pressed
    }

    /// A window_view from an agent takes the view back from a playback (it keeps playing).
    pub(crate) fn timeline_release_window(&mut self) {
        for p in &mut self.timeline.playing {
            p.window = false;
            p.restore = None;
        }
    }

    /// The hosted game's timeline requests (after its ticks and frames).
    pub(crate) fn serve_game_timelines(&mut self) {
        let Some(game) = self.game.as_mut() else { return };
        let mut reqs = Vec::new();
        game.timeline_requests(&mut reqs);
        let by = self.game_name.clone();
        for r in reqs.into_iter().take(16) {
            match r {
                TimelineRequest::Play { path, window, skippable, restore } => {
                    let start = Start { path: path.clone(), from_s: 0.0, paused: false, window, skippable, restore, by: by.clone(), key: format!("game:{by}"), from_game: true };
                    if let Err(e) = self.timeline_start(start) {
                        specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "timeline.game_refused", path = path.as_str(), code = e.code, message = e.message.as_str());
                        self.timeline.push(TimelineEvent { path, t: 0.0, what: TimelineWhat::Ended { how: "refused", error: Some(e.message) } });
                    }
                }
                TimelineRequest::Stop { path } => {
                    if let Some(i) = self.timeline.index(&path) {
                        self.timeline_end(i, "stopped", None);
                    }
                }
            }
        }
        self.timeline_deliver();
    }

    /// The world was replaced (a load): every playback ends (their entities are gone).
    pub(crate) fn timeline_world_replaced(&mut self) {
        while !self.timeline.playing.is_empty() {
            self.timeline_end(0, "world_replaced", None);
        }
        self.timeline.cache.clear();
    }

    /// Pauses (or resumes) a playback.
    pub(crate) fn timeline_set_paused(&mut self, i: usize, paused: bool) {
        let hz = self.hz();
        let wt = self.world.world_time();
        let p = &mut self.timeline.playing[i];
        // Paused or blocked, it stood still while world time went on: its keys start from now
        // (lead review: a blocked playback, which isn't `paused`, kept its old anchor and started
        // its effects seconds in the past).
        if !paused && (p.paused || p.blocked.is_some()) {
            p.origin_s = wt - p.pos as f64 / hz;
            if p.fired_to.is_none() {
                p.fired_to = Some(p.pos);
                let (tl, path, pos) = (Arc::clone(&p.timeline), p.path.clone(), p.pos);
                Self::fire(&mut self.timeline, &tl, &path, None, pos, hz);
            }
        }
        let p = &mut self.timeline.playing[i];
        p.paused = paused;
        if !paused {
            p.blocked = None;
        }
        p.held_since = paused.then(std::time::Instant::now);
    }

    /// Ends playbacks whose starter's last session closed (`session_closed`, as step jobs do) and
    /// paused or blocked ones untouched for [`IDLE_LEASE`] (`expired`), so none holds a slot for
    /// good (lead review: four abandoned paused playbacks starved every agent).
    pub(crate) fn timeline_sweep(&mut self, server: &specula_protocol::Server) {
        let now = std::time::Instant::now();
        if self.timeline.playing.is_empty() || self.timeline.last_sweep.is_some_and(|t| now - t < SWEEP_EVERY) {
            return;
        }
        self.timeline.last_sweep = Some(now);
        let alive: std::collections::BTreeSet<String> = server.all_sessions().iter().map(|s| self.swarm.key(s)).collect();
        self.timeline_sweep_with(&alive, now);
    }

    pub(crate) fn timeline_sweep_with(&mut self, alive: &std::collections::BTreeSet<String>, now: std::time::Instant) {
        let mut i = self.timeline.playing.len();
        while i > 0 {
            i -= 1;
            let p = &self.timeline.playing[i];
            if !p.from_game && !alive.contains(&p.key) {
                self.timeline_end(i, "session_closed", None);
            } else if p.held_since.is_some_and(|h| now.saturating_duration_since(h) >= IDLE_LEASE) {
                self.timeline_end(i, "expired", None);
            }
        }
        self.timeline_deliver();
    }

    /// Moves a playback to `t` (rounded to a tick): its state is written as the caller (one
    /// commit); nothing that fires once fires.
    pub(crate) fn timeline_scrub(&mut self, i: usize, t: f64, w: Writer) -> Result<Json, WorldError> {
        let hz = self.hz();
        let p = &self.timeline.playing[i];
        let pos = ((t.max(0.0) * hz).round() as u64).min(p.end);
        let origin = self.world.world_time() - pos as f64 / hz;
        let tl = Arc::clone(&p.timeline);
        // Anim and vfx keys are anchored afresh: every written trigger is written again from the
        // new origin (or put back to what was there before, before its first key); values are
        // rewritten where they differ.
        let written = reanchored(&p.written);
        let (ops, now) = ops_at(&tl, origin, &written, &p.originals, pos as f64 / hz)?;
        let mut committed = false;
        if !ops.is_empty() {
            let by = w.name.to_owned();
            self.world.commit(&mut self.swarm, self.clock.tick(), w, Kind::Batch, Rule::Timeline, |wd| wd.apply_batch(ops, &by).map_err(|(_, e)| e))?;
            committed = true;
        }
        let p = &mut self.timeline.playing[i];
        p.written.extend(now);
        p.commits += u64::from(committed);
        p.pos = pos;
        p.origin_s = origin;
        p.blocked = None;
        if p.paused {
            p.held_since = Some(std::time::Instant::now());
        }
        // Scrubbing jumps over what fires once.
        p.fired_to = Some(pos);
        Ok(p.to_json(hz, &self.world))
    }

    pub(crate) fn timeline_status(&self) -> Json {
        self.timeline.to_json(self.hz(), &self.world)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::world::{Shape, SpawnSpec};

    const TL: &str = r#"{"schema": "specula.timeline.v1", "tracks": [
      {"kind": "entity", "target": "/door", "position": [{"t": 0, "value": [0,0,0]}, {"t": 2, "value": [0,4,0], "ease": "in_out"}],
       "visible": [{"t": 0, "value": true}, {"t": 1.5, "value": false}], "emissive": [{"t": 0, "value": 0}, {"t": 2, "value": 3}]},
      {"kind": "vfx", "target": "/door", "keys": [{"t": 1, "vfx": "fire"}]}]}"#;

    fn world() -> World {
        let mut w = World::default();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("door".into());
        w.spawn(s, "t").unwrap();
        w
    }

    fn apply(w: &mut World, tl: &Timeline, written: &mut BTreeMap<usize, Written>, t: f64) -> usize {
        apply_from(w, tl, written, &BTreeMap::new(), 0.0, t)
    }

    fn apply_from(w: &mut World, tl: &Timeline, written: &mut BTreeMap<usize, Written>, originals: &BTreeMap<(String, &'static str), Original>, origin: f64, t: f64) -> usize {
        let (ops, now) = ops_at(tl, origin, written, originals, t).unwrap();
        let n = ops.len();
        w.apply_batch(ops, "t").unwrap();
        written.extend(now);
        n
    }

    /// The entity's state as the timeline writes it.
    fn state(w: &World) -> String {
        let e = w.get(w.find_path("/door").unwrap()).unwrap();
        format!("{:?} {} {:?} {:?}", e.position, e.visible, e.look.surface.emissive, e.vfx.as_ref().map(|v| (v.effect.clone(), v.start)))
    }

    #[test]
    fn scrubbing_equals_playing() {
        let tl = Timeline::parse_text(TL).unwrap();
        let hz = 30.0;
        for stop in [0u64, 7, 30, 45, 52, 60, 90] {
            // Played tick by tick from the start.
            let mut played = world();
            let mut written = BTreeMap::new();
            for k in 0..=stop {
                apply(&mut played, &tl, &mut written, k as f64 / hz);
            }
            // Scrubbed straight there.
            let mut scrubbed = world();
            apply(&mut scrubbed, &tl, &mut BTreeMap::new(), stop as f64 / hz);
            assert_eq!(state(&played), state(&scrubbed), "at tick {stop}");
        }
    }

    /// Review finding: a scrub from a played state kept the old anchor (an effect 4 s old where a
    /// fresh scrub made it 1 s old) and, scrubbed before the first key, left the effect running.
    #[test]
    fn scrubbing_back_from_a_played_state_equals_playing_there() {
        let tl = Timeline::parse_text(TL).unwrap();
        let hz = 30.0;
        let originals: BTreeMap<(String, &'static str), Original> = [(("/door".to_owned(), "vfx"), Original::Vfx(None))].into_iter().collect();
        for back in [60u64, 31, 15, 0] {
            // Played to 5 s from origin 0, then scrubbed back with the anchor moved to 10.
            let mut played = world();
            let mut written = BTreeMap::new();
            for k in 0..=150 {
                apply_from(&mut played, &tl, &mut written, &originals, 0.0, k as f64 / hz);
            }
            assert!(state(&played).contains("fire"));
            let mut moved = reanchored(&written);
            apply_from(&mut played, &tl, &mut moved, &originals, 10.0, back as f64 / hz);
            // A fresh scrub there with the same anchor.
            let mut fresh = world();
            apply_from(&mut fresh, &tl, &mut BTreeMap::new(), &originals, 10.0, back as f64 / hz);
            assert_eq!(state(&played), state(&fresh), "scrubbed back to tick {back}");
            if back < 30 {
                assert!(!state(&played).contains("fire"), "before the vfx key the effect is gone again: {}", state(&played));
            }
        }
    }

    /// Review round 2: originals carried by track index put one entity's effect on another when
    /// a replayed file moved its tracks. Kept by (target, kind), an original never crosses over.
    #[test]
    fn an_original_belongs_to_its_entity_and_kind() {
        // /door's original "no effect" must not reach /lamp, whose vfx track now sits at index 1.
        let edited = TL.replace("{\"kind\": \"vfx\", \"target\": \"/door\"", "{\"kind\": \"vfx\", \"target\": \"/lamp\"");
        let tl = Timeline::parse_text(&edited).unwrap();
        let mut w = world();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("lamp".into());
        s.vfx = Some(specula_scintilla::VfxSpec { start: Some(0.0), ..specula_scintilla::VfxSpec::new("embers") });
        w.spawn(s, "t").unwrap();
        let originals: BTreeMap<(String, &'static str), Original> = [(("/door".to_owned(), "vfx"), Original::Vfx(None))].into_iter().collect();
        let mut written: BTreeMap<usize, Written> = [(1, Written::Trigger(Some(REANCHOR)))].into_iter().collect();
        apply_from(&mut w, &tl, &mut written, &originals, 0.0, 0.5);
        let lamp = w.get(w.find_path("/lamp").unwrap()).unwrap();
        assert_eq!(lamp.vfx.as_ref().map(|v| v.effect.as_str()), Some("embers"), "the lamp keeps its own effect");
        // With its own original, a scrub before the key puts that back.
        let mine: BTreeMap<(String, &'static str), Original> = [(("/lamp".to_owned(), "vfx"), Original::Vfx(lamp.vfx.clone()))].into_iter().collect();
        let mut written: BTreeMap<usize, Written> = BTreeMap::new();
        apply_from(&mut w, &tl, &mut written, &mine, 0.0, 2.0);
        assert_eq!(w.get(w.find_path("/lamp").unwrap()).unwrap().vfx.as_ref().map(|v| v.effect.as_str()), Some("fire"));
        let mut back = reanchored(&written);
        apply_from(&mut w, &tl, &mut back, &mine, 0.0, 0.5);
        assert_eq!(w.get(w.find_path("/lamp").unwrap()).unwrap().vfx.as_ref().map(|v| v.effect.as_str()), Some("embers"));
    }

    /// Review round 3: the cache kept 32 timelines and evicted by hash order, so a project with
    /// more parsed some on the engine thread. A byte budget, least recently used out first.
    #[test]
    fn the_parsed_cache_keeps_what_is_used_within_its_budget() {
        let tl = Ready { tl: Arc::new(Timeline::parse_text(TL).unwrap()), problems: Arc::new(Vec::new()) };
        let mut c = Parsed { map: BTreeMap::new(), bytes: 0, clock: 0 };
        let text = |i: u64| format!("{i:0>100}");
        for i in 0..3 {
            c.put_within(i, &text(i), tl.clone(), 300);
        }
        assert!(c.get(0, |t| t == text(0)).is_some(), "used: now the most recent");
        c.put_within(3, &text(3), tl.clone(), 300);
        assert_eq!(c.bytes, 300);
        assert!(c.get(1, |_| true).is_none(), "the least recently used went");
        assert!(c.get(0, |_| true).is_some() && c.get(2, |_| true).is_some() && c.get(3, |_| true).is_some());
        assert!(c.get(2, |t| t == "other").is_none(), "a hash hit with other text is a miss");
    }

    #[test]
    fn only_changes_are_written() {
        let tl = Timeline::parse_text(TL).unwrap();
        let mut w = world();
        let mut written = BTreeMap::new();
        assert_eq!(apply(&mut w, &tl, &mut written, 0.0), 1, "the start writes the entity once");
        assert_eq!(apply(&mut w, &tl, &mut written, 3.0), 2, "moved, and the effect started");
        assert_eq!(apply(&mut w, &tl, &mut written, 4.0), 0, "after every key nothing changes");
    }

    #[test]
    fn files_are_validated_with_the_engine_parsers() {
        assert!(validate_file("assets/timelines/a.json", TL).is_empty());
        let bad = TL.replace("\"fire\"", "{\"effect\": \"fire\", \"params\": {\"rate\": -4}}");
        let p = validate_file("assets/timelines/a.json", &bad);
        assert!(p.iter().any(|p| p.at == "tracks[1].keys[0].vfx"), "{p:?}");
        let p = validate_file("assets/timelines/a.json", "{\"schema\": 1}");
        assert_eq!(p[0].code, "invalid_value");
        let anim = TL.replace("{\"kind\": \"vfx\", \"target\": \"/door\", \"keys\": [{\"t\": 1, \"vfx\": \"fire\"}]}", "{\"kind\": \"anim\", \"target\": \"/door\", \"keys\": [{\"t\": 1, \"anim\": {\"clip\": \"walk\", \"bogus\": 1}}]}");
        assert!(validate_file("assets/timelines/a.json", &anim).iter().any(|p| p.at == "tracks[1].keys[0].anim"));
    }

    /// Lead review finding 4 (probe1 part 5): a scrub compared with its own cache, so an entity
    /// another agent (or an undo) moved stayed where they put it. A scrub writes every channel.
    #[test]
    fn a_scrub_writes_every_channel_again() {
        let tl = Timeline::parse_text(TL).unwrap();
        let mut w = world();
        let mut written = BTreeMap::new();
        apply(&mut w, &tl, &mut written, 1.0);
        let at = state(&w);
        // Someone else moves and hides the door.
        let mut u = UpdateSpec { position: Some(Vec3::new(9.0, 9.0, 9.0)), visible: Some(false), ..UpdateSpec::default() };
        u.look = LookPatch { emissive: Some(7.0), ..LookPatch::default() };
        w.update(&EntityKey::Path("/door".into()), u, "bob").unwrap();
        assert_ne!(state(&w), at);
        // Playing on only writes what changes; a scrub to the same time writes it all back.
        let mut again = reanchored(&written);
        assert!(apply(&mut w, &tl, &mut again, 1.0) >= 1);
        assert_eq!(state(&w), at, "the scrub put the door back where the timeline has it");
    }

    // ----- Engine-level tests (a GPU and Vulkan; they skip cleanly without them).

    use crate::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
    use specula_protocol::SessionInfo;

    const ALICE: Writer = Writer { name: "alice", session: Some("aaaaaa11") };
    const BOB: Writer = Writer { name: "bob", session: Some("bbbbbb22") };
    const CAROL: Writer = Writer { name: "carol", session: Some("cccccc33") };

    pub(crate) fn session(id: &str) -> SessionInfo {
        SessionInfo { id: id.into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() }
    }

    pub(crate) fn engine(test: &str) -> Option<crate::Engine> {
        if !specula_gpu::is_available() {
            crate::skip_test(test, "Vulkan is not available");
            return None;
        }
        let Ok(gpu) = specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) else {
            crate::skip_test(test, "no GPU");
            return None;
        };
        let dir = std::env::temp_dir().join(format!("specula-timeline-{test}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut e = crate::Engine::new(gpu, &dir, crate::models::Library::new(&dir, None)).expect("engine");
        let live: Vec<SessionInfo> = ["aaaaaa11", "bbbbbb22", "cccccc33"].iter().map(|s| session(s)).collect();
        for (id, name) in [("aaaaaa11", "alice"), ("bbbbbb22", "bob"), ("cccccc33", "carol")] {
            e.swarm.hello(&session(id), name.into(), "builder".into(), None, &live).unwrap();
        }
        Some(e)
    }

    pub(crate) fn put(e: &mut crate::Engine, path: &str, text: &str) {
        let file = crate::project::ProjectFile::new(crate::project::FileKind::Timeline, Arc::from(text), 1, Arc::from("alice"));
        e.world.commit(&mut e.swarm, 0, ALICE, Kind::File, crate::write_policy::Rule::Project, |wd| {
            wd.put_file(path, Some(file), false);
            Ok(())
        })
        .unwrap();
    }

    pub(crate) fn spawn(e: &mut crate::Engine, name: &str, x: f32) {
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some(name.into());
        s.position = Vec3::new(x, 0.0, 0.0);
        e.world.commit(&mut e.swarm, 0, ALICE, Kind::Spawn, crate::write_policy::Rule::World, |wd| wd.spawn(s, "alice").map(|_| ())).unwrap();
    }

    fn step(e: &mut crate::Engine, ticks: u64) {
        e.clock.set_paused(true);
        e.clock.step(ticks);
        e.advance_clock();
    }

    fn start(e: &mut crate::Engine, path: &str, w: Writer, paused: bool, from_game: bool) -> Result<Json, WorldError> {
        let key = if from_game { "game:test".to_owned() } else { w.session.unwrap_or(w.name).to_owned() };
        e.timeline_start(Start { path: path.into(), from_s: 0.0, paused, window: false, skippable: true, restore: true, by: w.name.into(), key, from_game })
    }

    fn vfx_start(e: &crate::Engine, path: &str) -> Option<f64> {
        e.world.get(e.world.find_path(path).unwrap()).unwrap().vfx.as_ref().and_then(|v| v.start)
    }

    fn index(e: &crate::Engine, path: &str) -> usize {
        e.timeline.playing.iter().position(|p| p.path == path).unwrap()
    }

    /// Lead review finding 1 (probe1 part 2): a blocked playback isn't `paused`, so resuming it
    /// kept its old anchor and its later effects started seconds in the past. Played after a
    /// block, an effect starts where a scrub to the same time starts it.
    #[test]
    fn a_resumed_blocked_playback_starts_its_keys_from_now() {
        let Some(mut e) = engine("a_resumed_blocked_playback_starts_its_keys_from_now") else { return };
        spawn(&mut e, "pa", 0.0);
        spawn(&mut e, "pb", 3.0);
        put(&mut e, "assets/timelines/blk.json", r#"{"schema": "specula.timeline.v1", "duration": 4, "tracks": [
          {"kind": "entity", "name": "mv", "target": "/pa", "position": [{"t": 0, "value": [0,0,0]}, {"t": 1, "value": [0,5,0]}]},
          {"kind": "vfx", "name": "fx", "target": "/pb", "keys": [{"t": 2, "vfx": "fire"}]}]}"#);
        start(&mut e, "assets/timelines/blk.json", ALICE, false, false).unwrap();
        step(&mut e, 10);
        let claim = e.swarm.take_claim(&e.world, &session("bbbbbb22"), ClaimSpec { scope: Scope::path("/pa").unwrap(), lease: DEFAULT_LEASE, note: String::new() }).unwrap();
        step(&mut e, 10);
        assert_eq!(e.timeline.playing[index(&e, "assets/timelines/blk.json")].state(), "blocked");
        step(&mut e, 300);
        e.swarm.release(BOB, claim.get("id").and_then(Json::as_u64)).unwrap();
        let i = index(&e, "assets/timelines/blk.json");
        e.timeline_set_paused(i, false);
        step(&mut e, 60);
        let played = vfx_start(&e, "/pb").expect("the effect started");
        let i = index(&e, "assets/timelines/blk.json");
        let t = e.timeline.playing[i].t(f64::from(e.clock.hz()));
        e.timeline_scrub(i, t, ALICE).unwrap();
        let scrubbed = vfx_start(&e, "/pb").unwrap();
        assert!((played - scrubbed).abs() < 1e-6, "played {played}, scrubbed {scrubbed}");
        assert!(e.world.world_time() - played < 3.0, "the effect started on its key, not seconds in the past");
    }

    /// Lead review finding 3 (probe5): abandoned paused playbacks filled every slot for good. Each
    /// agent has its own cap, the game its own slots, a closed session's playbacks end, and idle
    /// ones expire.
    #[test]
    fn abandoned_playbacks_never_hold_slots_for_good() {
        let Some(mut e) = engine("abandoned_playbacks_never_hold_slots_for_good") else { return };
        let ev = r#"{"schema": "specula.timeline.v1", "duration": 5, "tracks": [{"kind": "events", "events": [{"t": 1, "name": "x"}]}]}"#;
        for n in ["p1", "p2", "p3", "p4", "p5", "p6", "g"] {
            put(&mut e, &format!("assets/timelines/{n}.json"), ev);
        }
        let path = |n: &str| format!("assets/timelines/{n}.json");
        start(&mut e, &path("p1"), ALICE, true, false).unwrap();
        start(&mut e, &path("p2"), ALICE, true, false).unwrap();
        let mine = start(&mut e, &path("p3"), ALICE, true, false).unwrap_err();
        assert_eq!(mine.code, "job_busy", "{} per agent", MAX_PER_AGENT);
        start(&mut e, &path("p3"), CAROL, true, false).unwrap();
        start(&mut e, &path("p4"), CAROL, true, false).unwrap();
        assert_eq!(start(&mut e, &path("p5"), BOB, false, false).unwrap_err().code, "job_busy", "agents' slots are full");
        // The hosted game's request goes through at once: its slots are its own.
        start(&mut e, &path("g"), BOB, false, true).expect("a game request is never starved by agents");
        // Alice's and Carol's sessions close: their playbacks end; the game's stays.
        let alive: std::collections::BTreeSet<String> = ["bbbbbb22".to_owned()].into_iter().collect();
        e.timeline_sweep_with(&alive, std::time::Instant::now());
        let left: Vec<&str> = e.timeline.playing.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(left, vec![path("g").as_str()]);
        assert!(e.timeline.ended.iter().any(|j| j.get("how") == Some(&Json::from("session_closed"))));
        start(&mut e, &path("p5"), BOB, true, false).expect("bob plays now");
        // Untouched past the lease, a paused playback expires; a playing one doesn't.
        let i = index(&e, &path("p5"));
        e.timeline.playing[i].held_since = Some(std::time::Instant::now() - IDLE_LEASE - std::time::Duration::from_secs(1));
        e.timeline_sweep_with(&alive, std::time::Instant::now());
        assert!(e.timeline.playing.iter().all(|p| p.path != path("p5")), "expired");
        assert_eq!(e.timeline.ended.back().and_then(|j| j.get("how")), Some(&Json::from("expired")));
        assert!(e.timeline.playing.iter().any(|p| p.path == path("g")), "the playing game timeline stays");
    }

    /// Lead review R2, minor 3: the same-path busy check came before the game's own slots, so an
    /// agent playing `intro.json` got the game's intro refused. The game's request takes the path
    /// over (the agent's playback ends as `replaced`), and the agent can't take it back.
    #[test]
    fn the_game_takes_its_path_over_from_an_agent() {
        let Some(mut e) = engine("the_game_takes_its_path_over_from_an_agent") else { return };
        let path = "assets/timelines/intro.json";
        let ev = r#"{"schema": "specula.timeline.v1", "duration": 5, "tracks": [{"kind": "events", "events": [{"t": 1, "name": "x"}]}]}"#;
        put(&mut e, path, ev);
        start(&mut e, path, ALICE, false, false).unwrap();
        start(&mut e, path, BOB, false, true).expect("the game's request is never blocked by an agent playing its file");
        let left: Vec<(&str, bool)> = e.timeline.playing.iter().map(|p| (p.path.as_str(), p.from_game)).collect();
        assert_eq!(left, vec![(path, true)], "one playback of the path: the game's");
        let ended = e.timeline.ended.back().unwrap();
        assert_eq!((ended.get("how"), ended.get("by")), (Some(&Json::from("replaced")), Some(&Json::from("alice"))));
        // The agent can't take it back while the game plays it.
        assert_eq!(start(&mut e, path, ALICE, false, false).unwrap_err().code, "job_busy");
        // Between agents, the path stays its starter's (only the game takes over).
        let other = "assets/timelines/other.json";
        put(&mut e, other, ev);
        start(&mut e, other, ALICE, false, false).unwrap();
        assert_eq!(start(&mut e, other, CAROL, false, false).unwrap_err().code, "job_busy");
    }

    /// Lead review R2, minor 4: `TimelineWhat::Ended`'s docs missed endings. Every `how` the
    /// engine ends a playback with is documented there and in TIMELINE.md.
    #[test]
    fn every_ending_is_documented_for_the_game() {
        let sources = [include_str!("timeline.rs"), include_str!("timeline_tools.rs")];
        let game = include_str!("game.rs");
        let doc = game.split("pub enum TimelineWhat").nth(1).and_then(|s| s.split("Ended {").next()).expect("TimelineWhat's docs");
        let design = include_str!("../../../docs/design/TIMELINE.md");
        let mut hows = std::collections::BTreeSet::new();
        for src in sources {
            // The literal among a call's arguments (the definition has none).
            for part in src.split("timeline_end(").skip(1) {
                let args = part.split(')').next().unwrap_or_default();
                if let Some(how) = args.split('"').nth(1).filter(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_lowercase() || c == '_')) {
                    hows.insert(how.to_owned());
                }
            }
            for part in src.split("Ended { how: \"").skip(1) {
                hows.insert(part.split('"').next().unwrap_or_default().to_owned());
            }
        }
        assert!(hows.len() >= 8, "found {hows:?}");
        for how in &hows {
            assert!(doc.contains(&format!("`{how}`")), "TimelineWhat::Ended's docs miss `{how}`");
            assert!(design.contains(&format!("`{how}`")), "TIMELINE.md misses `{how}`");
        }
    }
}

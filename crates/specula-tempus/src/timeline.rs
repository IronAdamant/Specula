//! The timeline format (`specula.timeline.v1`): tracks of keys over time, parsed with
//! structured errors and bounded everywhere (docs/design/TIMELINE.md).
//!
//! ```json
//! {"schema": "specula.timeline.v1", "description": "intro", "duration": 12,
//!  "tracks": [
//!   {"kind": "camera", "name": "fly", "rail": {"points": [[0,5,40],[30,20,10],[20,40,-30]]},
//!    "move": [{"t": 0, "at": 0}, {"t": 10, "at": 1, "ease": "smooth"}],
//!    "look": {"point": [0,0,0]}, "fov": [{"t": 0, "value": 55}, {"t": 10, "value": 45, "ease": "in_out"}]},
//!   {"kind": "entity", "target": "/gate", "position": [{"t": 2, "value": [0,0,0]}, {"t": 4, "value": [0,3,0]}],
//!    "visible": [{"t": 0, "value": false}, {"t": 1, "value": true}]},
//!   {"kind": "anim", "target": "/hero", "keys": [{"t": 1, "anim": "wave"}]},
//!   {"kind": "vfx", "target": "/brazier", "keys": [{"t": 3, "vfx": "fire"}, {"t": 9, "vfx": false}]},
//!   {"kind": "audio", "cues": [{"t": 0.5, "sound": "bell", "volume": 0.8}]},
//!   {"kind": "subtitles", "lines": [{"t": 1, "duration": 3, "speaker": "Warden", "text": "The dead are restless."}]},
//!   {"kind": "events", "events": [{"t": 11.5, "name": "title"}]}]}
//! ```

use crate::ease::Ease;
use crate::error::{read, Path, Result, TimelineError};
use crate::rail::{Rail, V3};
use specula_core::Json;

pub const SCHEMA: &str = "specula.timeline.v1";
/// Longest timeline (seconds).
pub const MAX_DURATION: f64 = 600.0;
pub const MAX_TRACKS: usize = 64;
/// Keys per channel, cues, lines or events per track.
pub const MAX_KEYS: usize = 512;
/// A timeline file's text.
pub const MAX_TEXT_BYTES: usize = 256 * 1024;
/// Entity paths, sound names, speakers and event names.
pub const MAX_TARGET: usize = 256;
pub const MAX_NAME: usize = 64;
/// A subtitle line's text.
pub const MAX_LINE_TEXT: usize = 512;
/// An event's `data` (compact JSON) and the description.
pub const MAX_DATA_BYTES: usize = 4096;
/// Positions are within this many metres of the origin.
pub const MAX_COORD: f64 = 100_000.0;

pub const KINDS: [&str; 7] = ["camera", "entity", "anim", "vfx", "audio", "subtitles", "events"];

/// A value at a time, reached from the previous key with `ease`.
#[derive(Clone, Debug, PartialEq)]
pub struct Key<T> {
    pub t: f64,
    pub value: T,
    pub ease: Ease,
}

/// Keys in strictly increasing time.
#[derive(Clone, Debug, PartialEq)]
pub struct Channel<T> {
    pub keys: Vec<Key<T>>,
}

pub trait Blend: Copy {
    fn blend(a: Self, b: Self, u: f64) -> Self;
}

impl Blend for f64 {
    fn blend(a: f64, b: f64, u: f64) -> f64 {
        a + (b - a) * u
    }
}

impl Blend for V3 {
    fn blend(a: V3, b: V3, u: f64) -> V3 {
        crate::rail::lerp(a, b, u)
    }
}

impl Blend for bool {
    /// Booleans hold until the next key.
    fn blend(a: bool, b: bool, u: f64) -> bool {
        if u >= 1.0 {
            b
        } else {
            a
        }
    }
}

impl<T: Blend> Channel<T> {
    /// The value at `t`: the first key's before it, the last key's after it, eased between.
    pub fn sample(&self, t: f64) -> T {
        let k = &self.keys;
        let i = k.partition_point(|key| key.t <= t);
        if i == 0 {
            return k[0].value;
        }
        if i == k.len() {
            return k[i - 1].value;
        }
        let (a, b) = (&k[i - 1], &k[i]);
        let u = (t - a.t) / (b.t - a.t);
        T::blend(a.value, b.value, b.ease.apply(u))
    }

    pub fn span(&self) -> (f64, f64) {
        (self.keys[0].t, self.keys[self.keys.len() - 1].t)
    }
}

/// Where a camera looks.
#[derive(Clone, Debug, PartialEq)]
pub enum Look {
    /// A fixed point.
    Point(V3),
    /// An entity's centre (its shown bounds) plus an offset, as the world has it then.
    Entity { path: String, offset: V3 },
    /// A second rail, travelled at the same `at` as the camera's.
    Rail(Rail),
    /// Along the rail, this many metres ahead.
    Ahead(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CameraTrack {
    pub rail: Rail,
    /// How far along the rail (0..1 of its arc length) over time.
    pub moves: Channel<f64>,
    pub look: Look,
    /// Vertical field of view, degrees (default 50).
    pub fov: Channel<f64>,
    /// Roll about the view direction, degrees (default 0: the horizon stays level).
    pub roll: Channel<f64>,
}

impl CameraTrack {
    /// When this camera is the one in use: from its first move key to its last.
    pub fn span(&self) -> (f64, f64) {
        self.moves.span()
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct EntityTrack {
    /// The entity's path (`/district/gate`).
    pub target: String,
    pub position: Option<Channel<V3>>,
    pub rotation: Option<Channel<V3>>,
    pub scale: Option<Channel<V3>>,
    pub visible: Option<Channel<bool>>,
    /// sRGB 0..1.
    pub color: Option<Channel<V3>>,
    pub emissive: Option<Channel<f64>>,
}

/// One key of an anim or vfx track: from its time on, the entity plays this (the value is what
/// `world_set` takes for `anim` / `vfx`; the engine validates it).
#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    pub t: f64,
    pub value: Json,
}

/// Where a sound cue plays.
#[derive(Clone, Debug, PartialEq)]
pub enum CueAt {
    Point(V3),
    Entity(String),
}

/// An audio cue: a typed seam for the audio service (it binds `sound` to its sound library).
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub t: f64,
    pub sound: String,
    /// 0..4 (1 = as authored).
    pub volume: f64,
    /// `None`: not positioned (UI / music).
    pub at: Option<CueAt>,
    /// A mixer bus name (`music`, `sfx`, `voice`...), if the cue names one.
    pub bus: Option<String>,
}

/// A subtitle or caption line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub t: f64,
    pub duration: f64,
    pub text: String,
    pub speaker: Option<String>,
}

/// An event for the hosted game.
#[derive(Clone, Debug, PartialEq)]
pub struct GameEvent {
    pub t: f64,
    pub name: String,
    pub data: Json,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TrackData {
    Camera(Box<CameraTrack>),
    Entity(Box<EntityTrack>),
    Anim { target: String, keys: Vec<Trigger> },
    Vfx { target: String, keys: Vec<Trigger> },
    Audio(Vec<Cue>),
    Subtitles(Vec<Line>),
    Events(Vec<GameEvent>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub name: String,
    pub data: TrackData,
}

impl Track {
    pub fn kind(&self) -> &'static str {
        match self.data {
            TrackData::Camera(_) => "camera",
            TrackData::Entity(_) => "entity",
            TrackData::Anim { .. } => "anim",
            TrackData::Vfx { .. } => "vfx",
            TrackData::Audio(_) => "audio",
            TrackData::Subtitles(_) => "subtitles",
            TrackData::Events(_) => "events",
        }
    }

    /// The entity it writes, if any.
    pub fn target(&self) -> Option<&str> {
        match &self.data {
            TrackData::Entity(e) => Some(&e.target),
            TrackData::Anim { target, .. } | TrackData::Vfx { target, .. } => Some(target),
            _ => None,
        }
    }

    /// Whether playing it writes the world.
    pub fn writes_world(&self) -> bool {
        self.target().is_some()
    }

    /// The times of its keys (first, last), if it has any.
    pub fn span(&self) -> Option<(f64, f64)> {
        let ts: Vec<f64> = match &self.data {
            TrackData::Camera(c) => {
                let mut v = vec![c.moves.span().0, c.moves.span().1];
                v.extend([c.fov.span().0, c.fov.span().1, c.roll.span().0, c.roll.span().1]);
                v
            }
            TrackData::Entity(e) => {
                let mut v = Vec::new();
                let mut push = |s: Option<(f64, f64)>| {
                    if let Some((a, b)) = s {
                        v.extend([a, b]);
                    }
                };
                push(e.position.as_ref().map(Channel::span));
                push(e.rotation.as_ref().map(Channel::span));
                push(e.scale.as_ref().map(Channel::span));
                push(e.visible.as_ref().map(Channel::span));
                push(e.color.as_ref().map(Channel::span));
                push(e.emissive.as_ref().map(Channel::span));
                v
            }
            TrackData::Anim { keys, .. } | TrackData::Vfx { keys, .. } => keys.iter().map(|k| k.t).collect(),
            TrackData::Audio(c) => c.iter().map(|c| c.t).collect(),
            TrackData::Subtitles(l) => l.iter().flat_map(|l| [l.t, l.t + l.duration]).collect(),
            TrackData::Events(e) => e.iter().map(|e| e.t).collect(),
        };
        let lo = ts.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = ts.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Timeline {
    pub description: String,
    /// Seconds: the explicit `duration`, else the last key's time.
    pub duration: f64,
    pub tracks: Vec<Track>,
}

/// A name (`[A-Za-z0-9_.-]`, 1..=64).
fn name(v: &Json, at: &Path) -> Result<String> {
    let s = read::string(v, at, MAX_NAME)?;
    if !s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')) {
        return Err(TimelineError::new("invalid_value", at.as_str(), format!("'{at}' may hold only A-Z a-z 0-9 _ - . (1..={MAX_NAME})")).value(v));
    }
    Ok(s.to_owned())
}

/// An entity path: `/` then names separated by `/`.
fn entity_path(v: &Json, at: &Path) -> Result<String> {
    let s = read::string(v, at, MAX_TARGET)?;
    if !s.starts_with('/') || s.len() < 2 || s.ends_with('/') || s.contains("//") || s.chars().any(char::is_control) {
        return Err(TimelineError::new("invalid_value", at.as_str(), format!("'{at}' must be an entity path such as \"/district/gate\"")).value(v));
    }
    Ok(s.to_owned())
}

/// A key time in `0..=MAX_DURATION`.
fn time(obj: &Json, at: &Path) -> Result<f64> {
    read::num(read::need(obj, "t", at, "the key's time in seconds")?, &at.key("t"), 0.0, MAX_DURATION)
}

/// Keys `[{t, <field>, ease?}]` of a channel, read by `value`.
fn channel<T>(v: &Json, at: &Path, field: &str, value: impl Fn(&Json, &Path) -> Result<T>) -> Result<Channel<T>> {
    let list = read::array(v, at, 1, MAX_KEYS)?;
    let mut keys: Vec<Key<T>> = Vec::with_capacity(list.len());
    for (i, k) in list.iter().enumerate() {
        let kp = at.index(i);
        read::keys(k, &kp, &["t", field, "ease"])?;
        let t = time(k, &kp)?;
        if let Some(prev) = keys.last() {
            if t <= prev.t {
                return Err(TimelineError::new("out_of_order", kp.key("t").as_str(), format!("keys go forward in time: {t} s comes after {} s", prev.t)).value(&Json::from(t)));
            }
        }
        let val = value(read::need(k, field, &kp, "the key's value")?, &kp.key(field))?;
        let ease = match k.get("ease") {
            None | Some(Json::Null) => Ease::Linear,
            Some(e) => Ease::parse(e, &kp.key("ease"))?,
        };
        keys.push(Key { t, value: val, ease });
    }
    Ok(Channel { keys })
}

/// A channel given as keys, or (for constant values) one bare value.
fn channel_or_const<T>(v: &Json, at: &Path, field: &str, value: impl Fn(&Json, &Path) -> Result<T>) -> Result<Channel<T>> {
    if v.as_array().is_some_and(|a| a.first().is_some_and(|k| k.as_object().is_some())) {
        channel(v, at, field, value)
    } else {
        Ok(Channel { keys: vec![Key { t: 0.0, value: value(v, at)?, ease: Ease::Linear }] })
    }
}

fn constant<T>(value: T) -> Channel<T> {
    Channel { keys: vec![Key { t: 0.0, value, ease: Ease::Linear }] }
}

impl Timeline {
    /// Parses a timeline file's text (at most [`MAX_TEXT_BYTES`]).
    pub fn parse_text(text: &str) -> Result<Timeline> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(TimelineError::new("too_large", "", format!("a timeline file is at most {MAX_TEXT_BYTES} bytes (this one is {})", text.len())));
        }
        let j = Json::parse(text).map_err(|e| TimelineError::new("json", "", format!("not JSON at byte {}: {}", e.offset, e.message)))?;
        Timeline::parse(&j)
    }

    pub fn parse(v: &Json) -> Result<Timeline> {
        let root = Path::root("");
        read::keys(v, &root, &["schema", "description", "duration", "tracks"])?;
        match v.get("schema") {
            Some(Json::Str(s)) if s == SCHEMA => {}
            Some(other) => return Err(TimelineError::new("invalid_value", "schema", format!("'schema' must be \"{SCHEMA}\"")).allowed(SCHEMA).value(other)),
            None => return Err(TimelineError::new("missing_field", "schema", format!("'schema' is required: \"{SCHEMA}\""))),
        }
        let description = match v.get("description") {
            None | Some(Json::Null) => String::new(),
            Some(d) => read::string(d, &root.key("description"), MAX_DATA_BYTES)?.to_owned(),
        };
        let list = read::array(read::need(v, "tracks", &root, "the timeline's tracks")?, &Path::root("tracks"), 1, MAX_TRACKS)?;
        let mut tracks: Vec<Track> = Vec::with_capacity(list.len());
        for (i, tj) in list.iter().enumerate() {
            let tp = Path::root("tracks").index(i);
            let track = parse_track(tj, &tp, i)?;
            if tracks.iter().any(|t| t.name == track.name) {
                return Err(TimelineError::new("duplicate", tp.key("name").as_str(), format!("two tracks are named '{}': track names are unique", track.name)));
            }
            tracks.push(track);
        }
        let last = tracks.iter().filter_map(Track::span).map(|s| s.1).fold(0.0, f64::max);
        // A subtitle line's end counts too: the whole timeline stays within MAX_DURATION.
        if last > MAX_DURATION {
            return Err(TimelineError::new("invalid_value", "tracks", format!("the timeline runs to {last} s, past the {MAX_DURATION} s limit (a key, or a subtitle line's end)")).allowed(format!("0..={MAX_DURATION} s")));
        }
        let duration = match v.get("duration") {
            None | Some(Json::Null) => last,
            Some(d) => {
                let d = read::num(d, &root.key("duration"), 0.0, MAX_DURATION)?;
                if last > d + 1e-9 {
                    return Err(TimelineError::new("invalid_value", "duration", format!("a key is at {last} s, after the duration ({d} s): lengthen 'duration' or move the key")).value(&Json::from(d)));
                }
                d
            }
        };
        if duration <= 0.0 {
            return Err(TimelineError::new("invalid_value", "duration", "a timeline lasts longer than 0 s: give 'duration' or keys after 0"));
        }
        Ok(Timeline { description, duration, tracks })
    }

    pub fn track(&self, name: &str) -> Option<(usize, &Track)> {
        self.tracks.iter().enumerate().find(|(_, t)| t.name == name)
    }

    pub fn cameras(&self) -> impl Iterator<Item = (usize, &CameraTrack)> {
        self.tracks.iter().enumerate().filter_map(|(i, t)| match &t.data {
            TrackData::Camera(c) => Some((i, &**c)),
            _ => None,
        })
    }

    /// The entities it writes (sorted, each once).
    pub fn targets(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.tracks.iter().filter_map(Track::target).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Whether playing it writes the world (entity, anim or vfx tracks).
    pub fn writes_world(&self) -> bool {
        self.tracks.iter().any(Track::writes_world)
    }

    /// One line per track: `{name, kind, target?, keys, span}`.
    pub fn summary(&self) -> Json {
        let tracks = self.tracks.iter().map(|t| {
            let keys = match &t.data {
                TrackData::Camera(c) => c.moves.keys.len(),
                TrackData::Entity(e) => {
                    e.position.as_ref().map_or(0, |c| c.keys.len())
                        + e.rotation.as_ref().map_or(0, |c| c.keys.len())
                        + e.scale.as_ref().map_or(0, |c| c.keys.len())
                        + e.visible.as_ref().map_or(0, |c| c.keys.len())
                        + e.color.as_ref().map_or(0, |c| c.keys.len())
                        + e.emissive.as_ref().map_or(0, |c| c.keys.len())
                }
                TrackData::Anim { keys, .. } | TrackData::Vfx { keys, .. } => keys.len(),
                TrackData::Audio(c) => c.len(),
                TrackData::Subtitles(l) => l.len(),
                TrackData::Events(e) => e.len(),
            };
            let mut f = vec![("name", Json::from(t.name.as_str())), ("kind", Json::from(t.kind())), ("keys", Json::from(keys))];
            if let Some(target) = t.target() {
                f.push(("target", Json::from(target)));
            }
            if let TrackData::Camera(c) = &t.data {
                f.push(("rail_m", Json::from(specula_core::dmath::round_f64(c.rail.length() * 1000.0) / 1000.0)));
                f.push(("rail_points", Json::from(c.rail.points.len())));
            }
            if let Some((a, b)) = t.span() {
                f.push(("span", Json::arr([Json::from(a), Json::from(b)])));
            }
            Json::obj(f)
        });
        Json::obj([("duration", Json::from(self.duration)), ("tracks", Json::arr(tracks)), ("writes_world", Json::from(self.writes_world()))])
    }
}

fn parse_track(v: &Json, at: &Path, index: usize) -> Result<Track> {
    let kind = read::string(read::need(v, "kind", at, &format!("one of {}", KINDS.join(", ")))?, &at.key("kind"), 32)?;
    let name_of = |v: &Json| -> Result<String> {
        match v.get("name") {
            None | Some(Json::Null) => Ok(format!("{kind}{index}")),
            Some(n) => name(n, &at.key("name")),
        }
    };
    let data = match kind {
        "camera" => {
            read::keys(v, at, &["kind", "name", "rail", "move", "look", "fov", "roll"])?;
            let rail = Rail::parse(read::need(v, "rail", at, "{points: [[x, y, z], ...]} in metres")?, &at.key("rail"))?;
            let moves = channel(read::need(v, "move", at, "keys [{t, at (0..1 along the rail), ease?}]")?, &at.key("move"), "at", |j, p| read::num(j, p, 0.0, 1.0))?;
            let look = parse_look(read::need(v, "look", at, "{point: [x,y,z]} | {entity: \"/path\", offset?} | {rail: {points}} | {ahead: m}")?, &at.key("look"))?;
            let fov = match v.get("fov") {
                None | Some(Json::Null) => constant(50.0),
                Some(f) => channel_or_const(f, &at.key("fov"), "value", |j, p| read::num(j, p, 5.0, 120.0))?,
            };
            let roll = match v.get("roll") {
                None | Some(Json::Null) => constant(0.0),
                Some(r) => channel_or_const(r, &at.key("roll"), "value", |j, p| read::num(j, p, -180.0, 180.0))?,
            };
            TrackData::Camera(Box::new(CameraTrack { rail, moves, look, fov, roll }))
        }
        "entity" => {
            const CHANNELS: [&str; 6] = ["position", "rotation", "scale", "visible", "color", "emissive"];
            read::keys(v, at, &["kind", "name", "target", "position", "rotation", "scale", "visible", "color", "emissive"])?;
            let target = entity_path(read::need(v, "target", at, "the entity's path, \"/district/gate\"")?, &at.key("target"))?;
            let get = |k: &str| v.get(k).filter(|j| !j.is_null());
            let v3 = |lim: f64| move |j: &Json, p: &Path| read::vec3(j, p, lim);
            let e = EntityTrack {
                target,
                position: get("position").map(|j| channel(j, &at.key("position"), "value", v3(MAX_COORD))).transpose()?,
                rotation: get("rotation").map(|j| channel(j, &at.key("rotation"), "value", v3(36_000.0))).transpose()?,
                scale: get("scale").map(|j| channel(j, &at.key("scale"), "value", |j, p| {
                    let s = read::vec3(j, p, 1000.0)?;
                    if s.iter().any(|c| *c <= 0.0) {
                        return Err(TimelineError::new("invalid_value", p.as_str(), format!("'{p}' must be positive on every axis")).value(j));
                    }
                    Ok(s)
                }))
                .transpose()?,
                visible: get("visible")
                    .map(|j| channel(j, &at.key("visible"), "value", |j, p| j.as_bool().ok_or_else(|| TimelineError::new("bad_type", p.as_str(), format!("'{p}' must be true or false")).value(j))))
                    .transpose()?,
                color: get("color").map(|j| channel(j, &at.key("color"), "value", read::colour)).transpose()?,
                emissive: get("emissive").map(|j| channel(j, &at.key("emissive"), "value", |j, p| read::num(j, p, 0.0, 1000.0))).transpose()?,
            };
            if e.position.is_none() && e.rotation.is_none() && e.scale.is_none() && e.visible.is_none() && e.color.is_none() && e.emissive.is_none() {
                return Err(TimelineError::new("missing_field", at.as_str(), format!("an entity track animates at least one of {}", CHANNELS.join(", "))));
            }
            TrackData::Entity(Box::new(e))
        }
        "anim" | "vfx" => {
            read::keys(v, at, &["kind", "name", "target", "keys"])?;
            let target = entity_path(read::need(v, "target", at, "the entity's path")?, &at.key("target"))?;
            let list = read::array(read::need(v, "keys", at, &format!("keys [{{t, {kind}}}]"))?, &at.key("keys"), 1, MAX_KEYS)?;
            let mut keys: Vec<Trigger> = Vec::with_capacity(list.len());
            for (i, k) in list.iter().enumerate() {
                let kp = at.key("keys").index(i);
                read::keys(k, &kp, &["t", kind])?;
                let t = time(k, &kp)?;
                if keys.last().is_some_and(|p| t <= p.t) {
                    return Err(TimelineError::new("out_of_order", kp.key("t").as_str(), "keys go forward in time").value(&Json::from(t)));
                }
                let value = read::need(k, kind, &kp, &format!("what world_set takes for '{kind}': a name, an object, or false to stop"))?;
                let ok = match value {
                    Json::Str(s) => !s.is_empty() && s.len() <= MAX_NAME,
                    Json::Bool(false) => true,
                    Json::Obj(_) => value.to_string().len() <= MAX_DATA_BYTES,
                    _ => false,
                };
                if !ok {
                    return Err(TimelineError::new("invalid_value", kp.key(kind).as_str(), format!("'{kind}' is a name (1..=64), an object (at most {MAX_DATA_BYTES} bytes) or false")).value(value));
                }
                keys.push(Trigger { t, value: value.clone() });
            }
            if kind == "anim" {
                TrackData::Anim { target, keys }
            } else {
                TrackData::Vfx { target, keys }
            }
        }
        "audio" => {
            read::keys(v, at, &["kind", "name", "cues"])?;
            let list = read::array(read::need(v, "cues", at, "[{t, sound, volume?, at?, bus?}]")?, &at.key("cues"), 1, MAX_KEYS)?;
            let mut cues: Vec<Cue> = Vec::with_capacity(list.len());
            for (i, c) in list.iter().enumerate() {
                let cp = at.key("cues").index(i);
                read::keys(c, &cp, &["t", "sound", "volume", "at", "bus"])?;
                let t = time(c, &cp)?;
                if cues.last().is_some_and(|p| t < p.t) {
                    return Err(TimelineError::new("out_of_order", cp.key("t").as_str(), "cues go forward in time").value(&Json::from(t)));
                }
                let sound = name(read::need(c, "sound", &cp, "the sound's name")?, &cp.key("sound"))?;
                let volume = read::num_or(c, "volume", &cp, 0.0, 4.0, 1.0)?;
                let at_ = match c.get("at") {
                    None | Some(Json::Null) => None,
                    Some(j @ Json::Str(_)) => Some(CueAt::Entity(entity_path(j, &cp.key("at"))?)),
                    Some(j) => Some(CueAt::Point(read::vec3(j, &cp.key("at"), MAX_COORD)?)),
                };
                let bus = c.get("bus").filter(|b| !b.is_null()).map(|b| name(b, &cp.key("bus"))).transpose()?;
                cues.push(Cue { t, sound, volume, at: at_, bus });
            }
            TrackData::Audio(cues)
        }
        "subtitles" => {
            read::keys(v, at, &["kind", "name", "lines"])?;
            let list = read::array(read::need(v, "lines", at, "[{t, duration, text, speaker?}]")?, &at.key("lines"), 1, MAX_KEYS)?;
            let mut lines: Vec<Line> = Vec::with_capacity(list.len());
            for (i, l) in list.iter().enumerate() {
                let lp = at.key("lines").index(i);
                read::keys(l, &lp, &["t", "duration", "text", "speaker"])?;
                let t = time(l, &lp)?;
                if lines.last().is_some_and(|p| t < p.t) {
                    return Err(TimelineError::new("out_of_order", lp.key("t").as_str(), "lines go forward in time").value(&Json::from(t)));
                }
                let duration = read::num(read::need(l, "duration", &lp, "seconds on screen")?, &lp.key("duration"), 0.1, 60.0)?;
                let text = read::string(read::need(l, "text", &lp, "the words")?, &lp.key("text"), MAX_LINE_TEXT)?.to_owned();
                let speaker = l.get("speaker").filter(|s| !s.is_null()).map(|s| read::string(s, &lp.key("speaker"), MAX_NAME).map(str::to_owned)).transpose()?;
                lines.push(Line { t, duration, text, speaker });
            }
            TrackData::Subtitles(lines)
        }
        "events" => {
            read::keys(v, at, &["kind", "name", "events"])?;
            let list = read::array(read::need(v, "events", at, "[{t, name, data?}]")?, &at.key("events"), 1, MAX_KEYS)?;
            let mut events: Vec<GameEvent> = Vec::with_capacity(list.len());
            for (i, e) in list.iter().enumerate() {
                let ep = at.key("events").index(i);
                read::keys(e, &ep, &["t", "name", "data"])?;
                let t = time(e, &ep)?;
                if events.last().is_some_and(|p| t < p.t) {
                    return Err(TimelineError::new("out_of_order", ep.key("t").as_str(), "events go forward in time").value(&Json::from(t)));
                }
                let n = name(read::need(e, "name", &ep, "the event's name")?, &ep.key("name"))?;
                let data = e.get("data").cloned().unwrap_or(Json::Null);
                if data.to_string().len() > MAX_DATA_BYTES {
                    return Err(TimelineError::new("too_large", ep.key("data").as_str(), format!("an event's data is at most {MAX_DATA_BYTES} bytes as JSON")));
                }
                events.push(GameEvent { t, name: n, data });
            }
            TrackData::Events(events)
        }
        _ => {
            return Err(TimelineError::new("invalid_value", at.key("kind").as_str(), format!("'{}' isn't a track kind", at.key("kind"))).allowed(KINDS.join(", ")).value(&Json::from(kind)));
        }
    };
    Ok(Track { name: name_of(v)?, data })
}

fn parse_look(v: &Json, at: &Path) -> Result<Look> {
    read::keys(v, at, &["point", "entity", "offset", "rail", "ahead"])?;
    let given = ["point", "entity", "rail", "ahead"].iter().filter(|k| v.get(k).is_some_and(|j| !j.is_null())).count();
    if given != 1 {
        return Err(TimelineError::new("invalid_value", at.as_str(), format!("'{at}' takes exactly one of point, entity, rail, ahead")).allowed("{point: [x,y,z]} | {entity: \"/path\", offset?} | {rail: {points}} | {ahead: m}"));
    }
    if v.get("offset").is_some_and(|o| !o.is_null()) && v.get("entity").is_none() {
        return Err(TimelineError::new("invalid_value", at.key("offset").as_str(), "'offset' goes with 'entity'"));
    }
    if let Some(p) = v.get("point").filter(|j| !j.is_null()) {
        return Ok(Look::Point(read::vec3(p, &at.key("point"), MAX_COORD)?));
    }
    if let Some(e) = v.get("entity").filter(|j| !j.is_null()) {
        let path = entity_path(e, &at.key("entity"))?;
        let offset = match v.get("offset") {
            None | Some(Json::Null) => [0.0; 3],
            Some(o) => read::vec3(o, &at.key("offset"), 10_000.0)?,
        };
        return Ok(Look::Entity { path, offset });
    }
    if let Some(r) = v.get("rail").filter(|j| !j.is_null()) {
        return Ok(Look::Rail(Rail::parse(r, &at.key("rail"))?));
    }
    let d = read::num(v.get("ahead").expect("counted"), &at.key("ahead"), 0.1, 10_000.0)?;
    Ok(Look::Ahead(d))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const SAMPLE: &str = r#"{"schema": "specula.timeline.v1", "description": "test", "duration": 12,
      "tracks": [
       {"kind": "camera", "name": "fly", "rail": {"points": [[0,5,40],[30,20,10],[20,40,-30]]},
        "move": [{"t": 0, "at": 0}, {"t": 10, "at": 1, "ease": "smooth"}],
        "look": {"point": [0,0,0]}, "fov": [{"t": 0, "value": 55}, {"t": 10, "value": 45, "ease": "in_out"}]},
       {"kind": "entity", "name": "gate", "target": "/gate", "position": [{"t": 2, "value": [0,0,0]}, {"t": 4, "value": [0,3,0], "ease": "in_out"}],
        "visible": [{"t": 0, "value": false}, {"t": 1, "value": true}], "emissive": [{"t": 0, "value": 0}, {"t": 6, "value": 2}]},
       {"kind": "anim", "target": "/hero", "keys": [{"t": 1, "anim": "wave"}, {"t": 5, "anim": false}]},
       {"kind": "vfx", "target": "/brazier", "keys": [{"t": 3, "vfx": "fire"}, {"t": 9, "vfx": false}]},
       {"kind": "audio", "cues": [{"t": 0.5, "sound": "bell", "volume": 0.8}, {"t": 6, "sound": "gong", "at": "/gate", "bus": "sfx"}]},
       {"kind": "subtitles", "lines": [{"t": 1, "duration": 3, "speaker": "Warden", "text": "The dead are restless."}]},
       {"kind": "events", "events": [{"t": 0, "name": "start"}, {"t": 11.5, "name": "title", "data": {"x": 1}}]}]}"#;

    pub fn sample() -> Timeline {
        Timeline::parse_text(SAMPLE).unwrap()
    }

    #[test]
    fn the_sample_parses_with_every_kind() {
        let t = sample();
        assert_eq!(t.duration, 12.0);
        assert_eq!(t.tracks.len(), 7);
        let kinds: Vec<&str> = t.tracks.iter().map(Track::kind).collect();
        assert_eq!(kinds, KINDS);
        assert_eq!(t.tracks[2].name, "anim2", "default names: kind and index");
        assert_eq!(t.targets(), vec!["/brazier", "/gate", "/hero"]);
        assert!(t.writes_world());
        let s = t.summary();
        assert_eq!(s.get("tracks").and_then(Json::as_array).map(<[Json]>::len), Some(7));
    }

    fn err(text: &str) -> TimelineError {
        Timeline::parse_text(text).unwrap_err()
    }

    #[test]
    fn bad_timelines_say_where_and_what() {
        let cam = |extra: &str| format!(r#"{{"schema": "specula.timeline.v1", "tracks": [{{"kind": "camera", "rail": {{"points": [[0,0,0],[1,0,0]]}}, "move": [{{"t": 0, "at": 0}}, {{"t": 2, "at": 1}}], "look": {{"point": [0,0,5]}}{extra}}}]}}"#);
        assert!(Timeline::parse_text(&cam("")).is_ok());
        let e = err(&cam(r#", "fov": [{"t": 0, "value": 500}]"#));
        assert_eq!((e.code, e.field.as_str()), ("invalid_value", "tracks[0].fov[0].value"));
        let e = err(&cam(r#", "zoom": 2"#));
        assert_eq!(e.code, "unknown_field");
        assert!(e.message.contains("zoom"));
        let e = err(r#"{"schema": "specula.timeline.v1", "tracks": [{"kind": "camera", "rail": {"points": [[0,0,0],[1,0,0]]}, "move": [{"t": 2, "at": 0}, {"t": 1, "at": 1}], "look": {"ahead": 5}}]}"#);
        assert_eq!((e.code, e.field.as_str()), ("out_of_order", "tracks[0].move[1].t"));
        assert_eq!(err(r#"{"schema": "v0", "tracks": []}"#).field, "schema");
        assert_eq!(err(r#"{"schema": "specula.timeline.v1", "tracks": [{"kind": "lasers"}]}"#).field, "tracks[0].kind");
        assert_eq!(err(r#"{"schema": "specula.timeline.v1", "duration": 1, "tracks": [{"kind": "events", "events": [{"t": 5, "name": "x"}]}]}"#).field, "duration");
        assert_eq!(err(r#"{"schema": "specula.timeline.v1", "tracks": [{"kind": "entity", "target": "/a"}]}"#).code, "missing_field");
        assert_eq!(err(r#"{"schema": "specula.timeline.v1", "tracks": [{"kind": "entity", "target": "a", "visible": [{"t": 0, "value": true}]}]}"#).field, "tracks[0].target");
        assert_eq!(
            err(r#"{"schema": "specula.timeline.v1", "tracks": [{"kind": "events", "name": "a", "events": [{"t": 1, "name": "x"}]}, {"kind": "events", "name": "a", "events": [{"t": 1, "name": "y"}]}]}"#).code,
            "duplicate"
        );
        assert_eq!(err(r#"{"schema": "specula.timeline.v1", "tracks": [{"kind": "subtitles", "lines": [{"t": 0, "duration": 1, "text": ""}]}]}"#).field, "tracks[0].lines[0].text");
        assert_eq!(err(r#"{"schema": "specula.timeline.v1", "tracks": [{"kind": "vfx", "target": "/a", "keys": [{"t": 0, "vfx": 3}]}]}"#).field, "tracks[0].keys[0].vfx");
        assert_eq!(err(&cam(r#", "look": {"point": [0,0,1], "ahead": 3}"#)).code, "duplicate", "a key given twice is refused");
        assert_eq!(err("{").code, "json");
        assert_eq!(err(&" ".repeat(MAX_TEXT_BYTES + 1)).code, "too_large");
    }

    #[test]
    fn bounds_hold_on_hostile_input() {
        // Too many tracks, keys and points are refused before anything big is built.
        let many: Vec<String> = (0..=MAX_TRACKS).map(|i| format!(r#"{{"kind": "events", "events": [{{"t": {i}, "name": "e"}}]}}"#)).collect();
        assert_eq!(err(&format!(r#"{{"schema": "specula.timeline.v1", "tracks": [{}]}}"#, many.join(","))).code, "too_many");
        let keys: Vec<String> = (0..=MAX_KEYS).map(|i| format!(r#"{{"t": {}, "name": "e"}}"#, i as f64 * 0.01)).collect();
        assert_eq!(err(&format!(r#"{{"schema": "specula.timeline.v1", "tracks": [{{"kind": "events", "events": [{}]}}]}}"#, keys.join(","))).code, "too_many");
        let pts: Vec<String> = (0..=crate::rail::MAX_POINTS).map(|i| format!("[{i},0,0]")).collect();
        let e = err(&format!(r#"{{"schema": "specula.timeline.v1", "tracks": [{{"kind": "camera", "rail": {{"points": [{}]}}, "move": [{{"t": 0, "at": 0}}], "look": {{"ahead": 1}}}}]}}"#, pts.join(",")));
        assert_eq!(e.code, "too_many");
        // Long keys aren't echoed whole.
        let long = "k".repeat(10_000);
        let e = err(&format!(r#"{{"schema": "specula.timeline.v1", "{long}": 1, "tracks": []}}"#));
        assert!(e.to_json().to_string().len() < 2048);
        // Non-finite and huge numbers.
        assert!(Timeline::parse_text(r#"{"schema": "specula.timeline.v1", "tracks": [{"kind": "events", "events": [{"t": 1e308, "name": "e"}]}]}"#).is_err());
        // Review: a subtitle line ending past the limit made a 660 s timeline.
        let e = err(r#"{"schema": "specula.timeline.v1", "tracks": [{"kind": "subtitles", "lines": [{"t": 600, "duration": 60, "text": "x"}]}]}"#);
        assert_eq!((e.code, e.field.as_str()), ("invalid_value", "tracks"));
    }
}

//! Clips: keyframed bone tracks, scalar curves and events, as strict JSON data.
//!
//! ```text
//! { "clip": 1, "name": "flinch", "duration": 0.5, "loop": "once",      // once | loop | ping_pong
//!   "distance": 0,                                       // metres travelled per cycle (speed matching)
//!   "events": [ { "name": "hit", "time": 0.1, "data": "chest" } ],
//!   "tracks": [
//!     { "bone": "chest", "channel": "rotation", "interp": "cubic",     // step | linear | cubic
//!       "times": [0, 0.1, 0.5], "values": [[0, 0, 0], [-25, 0, 8], [0, 0, 0]] },
//!     { "bone": "hips", "channel": "translation", "times": [0, 0.5], "values": [[0, 0, 0], [0, -0.05, 0]] } ],
//!   "curves": [ { "name": "opacity", "times": [0, 0.5], "values": [1, 0] } ] }
//! ```
//!
//! - **Rest-relative values.** Translation is an offset from the bone's rest position, rotation
//!   is applied after the rest rotation (`rest · q`), scale multiplies the rest scale. A clip
//!   therefore fits any rig with the same bone names, and missing tracks mean "rest".
//! - Rotations are `[x, y, z]` Euler degrees (X, then Y, then Z) or a `[x, y, z, w]` quaternion;
//!   scale is a number or `[x, y, z]`.
//! - `cubic` is a Catmull–Rom spline through the keys (tangents from neighbouring keys; looping
//!   clips whose first and last keys sit at 0 and `duration` wrap their end tangents). Every
//!   interpolation returns key values **exactly** at key times.
//! - Strict: unknown fields, unsorted times, duplicate tracks and out-of-range numbers are errors
//!   with a path (`tracks[2].times[4]`) and an [`AnimErrorCode`].

use specula_core::Json;

use crate::error::{err, is_ident, num, vecn, AnimError, AnimErrorCode, Obj};
use crate::math::{qalign, qeuler, qnlerp, qnorm, Q};

/// The clip format version this crate reads.
pub const FORMAT_VERSION: u32 = 1;
pub const MAX_DURATION_S: f32 = 600.0;
pub const MAX_KEYS: usize = 8192;
pub const MAX_TRACKS: usize = 512;
pub const MAX_EVENTS: usize = 1024;
pub const MAX_CURVES: usize = 64;
/// Largest translation offset (m) a key may hold.
pub const MAX_OFFSET_M: f32 = 1000.0;
/// At most this many loop cycles are scanned for events in one advance.
pub const MAX_EVENT_CYCLES: i64 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interp {
    Step,
    Linear,
    Cubic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    /// Plays once and holds the last pose.
    Once,
    Loop,
    /// Forwards, then backwards, repeatedly.
    PingPong,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Channel {
    Translation,
    Rotation,
    Scale,
}

impl Interp {
    pub fn name(self) -> &'static str {
        match self {
            Interp::Step => "step",
            Interp::Linear => "linear",
            Interp::Cubic => "cubic",
        }
    }
}

impl LoopMode {
    pub fn name(self) -> &'static str {
        match self {
            LoopMode::Once => "once",
            LoopMode::Loop => "loop",
            LoopMode::PingPong => "ping_pong",
        }
    }
}

impl Channel {
    pub fn name(self) -> &'static str {
        match self {
            Channel::Translation => "translation",
            Channel::Rotation => "rotation",
            Channel::Scale => "scale",
        }
    }
}

/// Key times and values (`[x, y, z, w]` for rotations, `[x, y, z, 0]` for vectors, `[v, 0, 0, 0]`
/// for curves).
#[derive(Clone, Debug, PartialEq)]
pub struct Keys {
    pub interp: Interp,
    pub times: Vec<f32>,
    pub values: Vec<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub bone: String,
    pub channel: Channel,
    pub keys: Keys,
}

/// A named scalar over time (e.g. `opacity` for a fading death), read by the game.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub name: String,
    pub keys: Keys,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub name: String,
    /// Seconds from the clip start, 0..=duration.
    pub time: f32,
    /// Free text for the game (e.g. which foot).
    pub data: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub name: String,
    pub duration: f32,
    pub looping: LoopMode,
    /// Metres the creature travels per cycle at rate 1 (0 = in place / not a locomotion clip):
    /// the playback rate that keeps feet planted is `speed / (distance / duration)`.
    pub distance: f32,
    pub tracks: Vec<Track>,
    pub curves: Vec<Curve>,
    /// Sorted by time (stable).
    pub events: Vec<Event>,
}

impl Keys {
    /// The value at `t` (clamped to the first/last key). `wrap`: the clip's duration when the
    /// keys form a closed loop (first key at 0, last at duration), for cubic end tangents.
    pub fn sample(&self, t: f32, rotation: bool, wrap: Option<f32>) -> [f32; 4] {
        let n = self.times.len();
        let v = &self.values;
        if n == 1 || t <= self.times[0] || t.is_nan() {
            return v[0];
        }
        if t >= self.times[n - 1] {
            return v[n - 1];
        }
        let i = self.times.partition_point(|&k| k <= t); // times[i-1] <= t < times[i]
        let (t0, t1) = (self.times[i - 1], self.times[i]);
        let u = (t - t0) / (t1 - t0);
        if u <= 0.0 {
            return v[i - 1];
        }
        let a = v[i - 1];
        match self.interp {
            Interp::Step => a,
            Interp::Linear if rotation => qnlerp(a, v[i], u),
            Interp::Linear => {
                let b = v[i];
                [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u, a[2] + (b[2] - a[2]) * u, a[3] + (b[3] - a[3]) * u]
            }
            Interp::Cubic => {
                let al = |q: [f32; 4]| if rotation { qalign(a, q) } else { q };
                let b = al(v[i]);
                let dt = t1 - t0;
                let m0 = self.tangent(i - 1, wrap, &al);
                let m1 = self.tangent(i, wrap, &al);
                let (u2, u3) = (u * u, u * u * u);
                let h00 = 2.0 * u3 - 3.0 * u2 + 1.0;
                let h10 = u3 - 2.0 * u2 + u;
                let h01 = -2.0 * u3 + 3.0 * u2;
                let h11 = u3 - u2;
                let mut out = [0.0f32; 4];
                for c in 0..4 {
                    out[c] = h00 * a[c] + h10 * dt * m0[c] + h01 * b[c] + h11 * dt * m1[c];
                }
                if rotation {
                    qnorm(out)
                } else {
                    out
                }
            }
        }
    }

    /// Finite-difference tangent at key `k` (values aligned by `al`).
    fn tangent(&self, k: usize, wrap: Option<f32>, al: &dyn Fn([f32; 4]) -> [f32; 4]) -> [f32; 4] {
        let n = self.times.len();
        let closed = wrap.filter(|d| n >= 3 && self.times[0] == 0.0 && self.times[n - 1] == *d);
        let key = |j: usize, shift: f32| (self.times[j] + shift, al(self.values[j]));
        let prev = if k > 0 {
            Some(key(k - 1, 0.0))
        } else {
            closed.map(|d| key(n - 2, -d))
        };
        let next = if k + 1 < n {
            Some(key(k + 1, 0.0))
        } else {
            closed.map(|d| key(1, d))
        };
        let here = key(k, 0.0);
        let ((ta, va), (tb, vb)) = match (prev, next) {
            (Some(p), Some(q)) => (p, q),
            (None, Some(q)) => (here, q),
            (Some(p), None) => (p, here),
            (None, None) => return [0.0; 4],
        };
        let inv = 1.0 / (tb - ta);
        [(vb[0] - va[0]) * inv, (vb[1] - va[1]) * inv, (vb[2] - va[2]) * inv, (vb[3] - va[3]) * inv]
    }
}

impl Clip {
    /// Parses and validates a clip from JSON text.
    pub fn parse(text: &str) -> Result<Clip, AnimError> {
        let json = Json::parse(text).map_err(|e| {
            let mut er = err("", AnimErrorCode::Json, e.message.clone());
            er.offset = Some(e.offset);
            er
        })?;
        Clip::from_json(&json, "")
    }

    /// Parses a clip object at `path` (for clips embedded in other files).
    pub fn from_json(v: &Json, path: &str) -> Result<Clip, AnimError> {
        let mut o = Obj::new(v, path, "a clip")?;
        if let Some(ver) = o.get("clip") {
            if ver.as_f64() != Some(FORMAT_VERSION as f64) {
                return Err(err(&o.at("clip"), AnimErrorCode::OutOfRange, format!("unsupported clip format (this build reads {FORMAT_VERSION})")));
            }
        }
        // Generator specs embedded in anim sets carry `"kind": "clip"`.
        if let Some(k) = o.str("kind")? {
            if k != "clip" {
                return Err(err(&o.at("kind"), AnimErrorCode::UnknownName, "a keyed clip has kind \"clip\""));
            }
        }
        let name = o.req_str("name")?.to_string();
        let duration = o.req_num("duration", 1e-3, MAX_DURATION_S)?;
        let looping = match o.str("loop")? {
            None | Some("once") => LoopMode::Once,
            Some("loop") => LoopMode::Loop,
            Some("ping_pong") => LoopMode::PingPong,
            Some(other) => return Err(err(&o.at("loop"), AnimErrorCode::UnknownName, format!("unknown loop mode '{other}' (once, loop, ping_pong)"))),
        };
        let distance = o.num("distance", 0.0, 0.0, 1000.0)?;
        let mut clip = Clip { name, duration, looping, distance, tracks: Vec::new(), curves: Vec::new(), events: Vec::new() };
        if let Some(items) = o.arr("tracks")? {
            if items.len() > MAX_TRACKS {
                return Err(err(&o.at("tracks"), AnimErrorCode::Limit, format!("{} tracks; at most {MAX_TRACKS}", items.len())).with_numbers(items.len() as f64, MAX_TRACKS as f64));
            }
            for (i, t) in items.iter().enumerate() {
                clip.tracks.push(parse_track(t, &format!("{}[{i}]", o.at("tracks")), duration)?);
            }
        }
        if let Some(items) = o.arr("curves")? {
            if items.len() > MAX_CURVES {
                return Err(err(&o.at("curves"), AnimErrorCode::Limit, format!("{} curves; at most {MAX_CURVES}", items.len())));
            }
            for (i, c) in items.iter().enumerate() {
                let p = format!("{}[{i}]", o.at("curves"));
                let mut co = Obj::new(c, &p, "a curve")?;
                let name = co.req_str("name")?.to_string();
                let keys = parse_keys(&mut co, duration, |v, p| Ok([num(v, p, -1e6, 1e6)?, 0.0, 0.0, 0.0]))?;
                co.finish("name, interp, times, values")?;
                clip.curves.push(Curve { name, keys });
            }
        }
        if let Some(items) = o.arr("events")? {
            if items.len() > MAX_EVENTS {
                return Err(err(&o.at("events"), AnimErrorCode::Limit, format!("{} events; at most {MAX_EVENTS}", items.len())));
            }
            for (i, ev) in items.iter().enumerate() {
                let p = format!("{}[{i}]", o.at("events"));
                let mut eo = Obj::new(ev, &p, "an event")?;
                let name = eo.req_str("name")?.to_string();
                let time = eo.req_num("time", 0.0, duration)?;
                let data = eo.str("data")?.unwrap_or("").to_string();
                eo.finish("name, time, data")?;
                clip.events.push(Event { name, time, data });
            }
        }
        o.finish("clip, kind, name, duration, loop, distance, tracks, curves, events")?;
        clip.sort_events();
        clip.validate().map_err(|e| e.under(path))?;
        Ok(clip)
    }

    /// Sorts events by time (stable, so equal times keep their order).
    pub fn sort_events(&mut self) {
        self.events.sort_by(|a, b| a.time.total_cmp(&b.time));
    }

    /// Checks every rule `parse` enforces; use it on clips built in code.
    pub fn validate(&self) -> Result<(), AnimError> {
        if self.name.is_empty() {
            return Err(err("name", AnimErrorCode::MissingField, "a clip needs a name"));
        }
        if !(self.duration > 0.0 && self.duration <= MAX_DURATION_S) {
            return Err(err("duration", AnimErrorCode::OutOfRange, format!("{} is outside (0, {MAX_DURATION_S}] s", self.duration)));
        }
        if !(self.distance >= 0.0 && self.distance <= 1000.0) {
            return Err(err("distance", AnimErrorCode::OutOfRange, "distance must be 0..1000 m"));
        }
        if self.tracks.len() > MAX_TRACKS {
            return Err(err("tracks", AnimErrorCode::Limit, format!("{} tracks; at most {MAX_TRACKS}", self.tracks.len())));
        }
        for (i, t) in self.tracks.iter().enumerate() {
            let p = format!("tracks[{i}]");
            if self.tracks[..i].iter().any(|o| o.bone == t.bone && o.channel == t.channel) {
                return Err(err(&p, AnimErrorCode::Duplicate, format!("a second {} track for '{}'", t.channel.name(), t.bone)));
            }
            validate_keys(&t.keys, &p, self.duration)?;
            for (k, v) in t.keys.values.iter().enumerate() {
                let vp = format!("{p}.values[{k}]");
                let ok = match t.channel {
                    Channel::Translation => v[..3].iter().all(|x| x.abs() <= MAX_OFFSET_M),
                    Channel::Scale => v[..3].iter().all(|x| (0.0..=100.0).contains(x)),
                    Channel::Rotation => v.iter().all(|x| x.is_finite()) && ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2] + v[3] * v[3]) - 1.0).abs() < 1e-3,
                };
                if !ok {
                    return Err(err(&vp, AnimErrorCode::OutOfRange, format!("{} value {v:?} is out of range", t.channel.name())));
                }
            }
        }
        for (i, c) in self.curves.iter().enumerate() {
            let p = format!("curves[{i}]");
            if !is_ident(&c.name) {
                return Err(err(&format!("{p}.name"), AnimErrorCode::WrongType, "curve names are identifiers"));
            }
            if self.curves[..i].iter().any(|o| o.name == c.name) {
                return Err(err(&format!("{p}.name"), AnimErrorCode::Duplicate, format!("a second curve '{}'", c.name)));
            }
            validate_keys(&c.keys, &p, self.duration)?;
        }
        if self.events.len() > MAX_EVENTS {
            return Err(err("events", AnimErrorCode::Limit, format!("{} events; at most {MAX_EVENTS}", self.events.len())));
        }
        for (i, e) in self.events.iter().enumerate() {
            if e.name.is_empty() || !(0.0..=self.duration).contains(&e.time) {
                return Err(err(&format!("events[{i}]"), AnimErrorCode::OutOfRange, format!("event '{}' at {} s is outside 0..{} s (or unnamed)", e.name, e.time, self.duration)));
            }
            if i > 0 && self.events[i - 1].time > e.time {
                return Err(err(&format!("events[{i}]"), AnimErrorCode::NotIncreasing, "events must be sorted by time"));
            }
        }
        Ok(())
    }

    /// Ground speed (m/s) at rate 1, from `distance`.
    pub fn speed(&self) -> f32 {
        self.distance / self.duration
    }

    /// The `duration` to use as the cubic wrap, if this clip loops.
    pub fn wrap(&self) -> Option<f32> {
        (self.looping == LoopMode::Loop).then_some(self.duration)
    }

    /// Clip-local time for an unwrapped playhead `u` ≥ 0 (seconds since the clip started).
    pub fn local_time(&self, u: f64) -> f32 {
        let d = self.duration as f64;
        let u = u.max(0.0);
        match self.looping {
            LoopMode::Once => u.min(d) as f32,
            LoopMode::Loop => {
                let r = (u - specula_core::dmath::floor_f64(u / d) * d) as f32;
                if r >= self.duration || r < 0.0 {
                    0.0
                } else {
                    r
                }
            }
            LoopMode::PingPong => {
                let c = specula_core::dmath::floor_f64(u / d);
                let r = (u - c * d).clamp(0.0, d);
                let even = (c as i64) % 2 == 0;
                (if even { r } else { d - r }) as f32
            }
        }
    }

    /// Indices of the events the playhead crosses moving from `u0` to `u1` (unwrapped seconds,
    /// `u1 ≥ u0`): an event at clip time `e` fires when its occurrence lies in `[u0, u1)`. Each
    /// occurrence fires exactly once however the movement is split into steps. `out` receives
    /// `(event index, cycle)` in time order.
    pub fn events_between(&self, u0: f64, u1: f64, out: &mut Vec<(u16, u32)>) {
        if self.events.is_empty() || u1 <= u0 {
            return;
        }
        let d = self.duration as f64;
        let fire =|at: f64, i: usize, c: i64, buf: &mut Vec<(f64, u16, u32)>| {
            if at >= u0 && at < u1 {
                buf.push((at, i as u16, c.max(0) as u32));
            }
        };
        let mut buf: Vec<(f64, u16, u32)> = Vec::new();
        match self.looping {
            LoopMode::Once => {
                for (i, e) in self.events.iter().enumerate() {
                    fire(e.time as f64, i, 0, &mut buf);
                }
            }
            LoopMode::Loop | LoopMode::PingPong => {
                let c0 = specula_core::dmath::floor_f64(u0 / d) as i64;
                let c1 = (specula_core::dmath::floor_f64(u1 / d) as i64).min(c0 + MAX_EVENT_CYCLES);
                for c in c0.max(0)..=c1 {
                    for (i, e) in self.events.iter().enumerate() {
                        let e = e.time as f64;
                        let local = match self.looping {
                            LoopMode::Loop => {
                                if e >= d {
                                    0.0
                                } else {
                                    e
                                }
                            }
                            _ if c % 2 == 0 => e,
                            _ => d - e,
                        };
                        fire(c as f64 * d + local, i, c, &mut buf);
                    }
                }
            }
        }
        buf.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        out.extend(buf.iter().map(|(_, i, c)| (*i, *c)));
    }

    /// The clip as JSON (rotations as quaternions); `Clip::from_json` reads it back identically.
    pub fn to_json(&self) -> Json {
        let keys_json = |k: &Keys, width: usize| -> Vec<(&'static str, Json)> {
            vec![
                ("interp", Json::Str(k.interp.name().into())),
                ("times", Json::arr(k.times.iter().map(|t| Json::Num(*t as f64)))),
                (
                    "values",
                    Json::arr(k.values.iter().map(|v| if width == 1 { Json::Num(v[0] as f64) } else { Json::arr(v[..width].iter().map(|x| Json::Num(*x as f64))) })),
                ),
            ]
        };
        let tracks = self.tracks.iter().map(|t| {
            let mut pairs = vec![("bone", Json::Str(t.bone.clone())), ("channel", Json::Str(t.channel.name().into()))];
            pairs.extend(keys_json(&t.keys, if t.channel == Channel::Rotation { 4 } else { 3 }));
            Json::obj(pairs)
        });
        let curves = self.curves.iter().map(|c| {
            let mut pairs = vec![("name", Json::Str(c.name.clone()))];
            pairs.extend(keys_json(&c.keys, 1));
            Json::obj(pairs)
        });
        let events = self.events.iter().map(|e| {
            let mut pairs = vec![("name", Json::Str(e.name.clone())), ("time", Json::Num(e.time as f64))];
            if !e.data.is_empty() {
                pairs.push(("data", Json::Str(e.data.clone())));
            }
            Json::obj(pairs)
        });
        Json::obj([
            ("clip", Json::Num(FORMAT_VERSION as f64)),
            ("name", Json::Str(self.name.clone())),
            ("duration", Json::Num(self.duration as f64)),
            ("loop", Json::Str(self.looping.name().into())),
            ("distance", Json::Num(self.distance as f64)),
            ("events", Json::arr(events)),
            ("tracks", Json::arr(tracks)),
            ("curves", Json::arr(curves)),
        ])
    }
}

fn parse_track(v: &Json, path: &str, duration: f32) -> Result<Track, AnimError> {
    let mut o = Obj::new(v, path, "a track")?;
    let bone = o.req_str("bone")?.to_string();
    let channel = match o.req_str("channel")? {
        "translation" => Channel::Translation,
        "rotation" => Channel::Rotation,
        "scale" => Channel::Scale,
        other => return Err(err(&o.at("channel"), AnimErrorCode::UnknownName, format!("unknown channel '{other}' (translation, rotation, scale)"))),
    };
    let keys = parse_keys(&mut o, duration, |v, p| match channel {
        Channel::Translation => {
            let t: [f32; 3] = vecn(v, p, -MAX_OFFSET_M, MAX_OFFSET_M)?;
            Ok([t[0], t[1], t[2], 0.0])
        }
        Channel::Scale => match v {
            Json::Num(_) => {
                let s = num(v, p, 0.0, 100.0)?;
                Ok([s, s, s, 0.0])
            }
            _ => {
                let s: [f32; 3] = vecn(v, p, 0.0, 100.0)?;
                Ok([s[0], s[1], s[2], 0.0])
            }
        },
        Channel::Rotation => match v.as_array().map(|a| a.len()) {
            Some(3) => Ok(qeuler(vecn(v, p, -36_000.0, 36_000.0)?)),
            Some(4) => {
                let q: [f32; 4] = vecn(v, p, -1e3, 1e3)?;
                let l2 = q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3];
                if l2 < 1e-12 {
                    return Err(err(p, AnimErrorCode::OutOfRange, "a zero quaternion is not a rotation"));
                }
                // Already-unit quaternions keep their exact bits (lossless round trips).
                Ok(if (l2 - 1.0).abs() > 1e-6 { qnorm(q) } else { q })
            }
            _ => Err(err(p, AnimErrorCode::WrongType, "a rotation is [x, y, z] Euler degrees or an [x, y, z, w] quaternion")),
        },
    })?;
    o.finish("bone, channel, interp, times, values")?;
    Ok(Track { bone, channel, keys })
}

fn parse_keys(o: &mut Obj, duration: f32, value: impl Fn(&Json, &str) -> Result<[f32; 4], AnimError>) -> Result<Keys, AnimError> {
    let interp = match o.str("interp")? {
        None | Some("linear") => Interp::Linear,
        Some("step") => Interp::Step,
        Some("cubic") => Interp::Cubic,
        Some(other) => return Err(err(&o.at("interp"), AnimErrorCode::UnknownName, format!("unknown interp '{other}' (step, linear, cubic)"))),
    };
    let tp = o.at("times");
    let vp = o.at("values");
    let times_j = o.req("times")?.as_array().ok_or_else(|| err(&tp, AnimErrorCode::WrongType, "expected a list of key times (s)"))?;
    let values_j = o.req("values")?.as_array().ok_or_else(|| err(&vp, AnimErrorCode::WrongType, "expected a list of key values"))?;
    if times_j.is_empty() {
        return Err(err(&tp, AnimErrorCode::MissingField, "at least one key"));
    }
    if times_j.len() > MAX_KEYS {
        return Err(err(&tp, AnimErrorCode::Limit, format!("{} keys; at most {MAX_KEYS}", times_j.len())).with_numbers(times_j.len() as f64, MAX_KEYS as f64));
    }
    if values_j.len() != times_j.len() {
        return Err(err(&vp, AnimErrorCode::WrongType, format!("{} values for {} times", values_j.len(), times_j.len())));
    }
    let mut times = Vec::with_capacity(times_j.len());
    for (i, t) in times_j.iter().enumerate() {
        times.push(num(t, &format!("{tp}[{i}]"), 0.0, duration)?);
    }
    let values = values_j.iter().enumerate().map(|(i, v)| value(v, &format!("{vp}[{i}]"))).collect::<Result<Vec<_>, _>>()?;
    let keys = Keys { interp, times, values };
    validate_keys(&keys, &o.path, duration)?;
    Ok(keys)
}

fn validate_keys(k: &Keys, path: &str, duration: f32) -> Result<(), AnimError> {
    if k.times.is_empty() || k.times.len() != k.values.len() {
        return Err(err(path, AnimErrorCode::WrongType, format!("{} times and {} values (at least one key)", k.times.len(), k.values.len())));
    }
    if k.times.len() > MAX_KEYS {
        return Err(err(&format!("{path}.times"), AnimErrorCode::Limit, format!("{} keys; at most {MAX_KEYS}", k.times.len())));
    }
    for (i, t) in k.times.iter().enumerate() {
        if !(0.0..=duration).contains(t) {
            return Err(err(&format!("{path}.times[{i}]"), AnimErrorCode::OutOfRange, format!("{t} is outside 0..{duration} s")));
        }
        if i > 0 && k.times[i - 1] >= *t {
            return Err(err(&format!("{path}.times[{i}]"), AnimErrorCode::NotIncreasing, format!("{t} s is not after {} s", k.times[i - 1])));
        }
    }
    if let Some(i) = k.values.iter().position(|v| v.iter().any(|x| !x.is_finite())) {
        return Err(err(&format!("{path}.values[{i}]"), AnimErrorCode::OutOfRange, "values must be finite"));
    }
    Ok(())
}

/// A rotation key value from Euler degrees (helper for code-built clips).
pub fn rot_key(deg: [f32; 3]) -> Q {
    qeuler(deg)
}

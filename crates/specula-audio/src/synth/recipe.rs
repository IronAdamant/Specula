//! Sound recipe parsing and strict validation (format version 1; see the crate docs).

use crate::error::{AudioError, ErrorCode, Result};
use crate::mixer::Bus;
use specula_core::hash::hash_str;
use specula_core::Json;

/// Longest sound a recipe may describe, in seconds.
pub const MAX_DURATION_S: f64 = 60.0;
pub const MAX_LAYERS: usize = 32;
pub const MAX_REPEAT: u32 = 256;
pub const MAX_POINTS: usize = 256;
pub const MAX_PARTIALS: usize = 32;
pub const MAX_FILTERS: usize = 4;
pub const MAX_FX: usize = 8;
/// Most synthesis work a recipe may need ([`Recipe::work_estimate`], at the render rate) so a
/// hostile recipe can't hang a server. Fixed, not scaled by rate: long sounds at 192 kHz are
/// refused rather than taking 4× as long.
pub const MAX_WORK: f64 = 300_000_000.0; // cost units ≈ ns: about 0.3 s of rendering on one core

/// A number that is either fixed or drawn from a seeded range (`{"rand": [lo, hi]}`). Each field
/// draws from its own stream keyed by its recipe path, with a named layer's path rooted at its
/// name rather than its index (see the crate docs for exactly what that guarantees).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Num {
    pub lo: f64,
    pub hi: f64,
    pub key: u64,
}

impl Num {
    pub fn fixed(v: f64) -> Num {
        Num { lo: v, hi: v, key: 0 }
    }
}

/// A value over time: a [`Num`], or breakpoints `[[t, v], ...]` (t in seconds from the layer's
/// start) interpolated linearly or, with `"curve": "exp"`, geometrically (for pitch/cutoff).
#[derive(Clone, Debug, PartialEq)]
pub enum Param {
    Const(Num),
    Points { points: Vec<(Num, Num)>, exp: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseColor {
    White,
    Pink,
    Brown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    Sine,
    Square,
    Saw,
    Triangle,
    Noise(NoiseColor),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Partial {
    pub ratio: Num,
    pub gain: Num,
    /// Exponential decay time constant in seconds (`None` = sustained).
    pub decay: Option<Num>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fm {
    pub ratio: Num,
    pub index: Param,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Unison {
    pub voices: u32,
    pub cents: Num,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub wave: Wave,
    /// Hz. Required for tonal waves; for noise it is an optional sample-and-hold rate.
    pub freq: Option<Param>,
    pub duty: Param,
    pub partials: Vec<Partial>,
    pub fm: Option<Fm>,
    pub unison: Option<Unison>,
    pub phase: Num,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Env {
    Adsr { attack: Num, hold: Num, decay: Num, sustain: Num, release: Num, exp: bool },
    Points(Vec<(Num, Num)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vibrato {
    pub rate: Num,
    pub depth: Num,
    pub delay: Num,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arp {
    pub steps: Vec<f64>,
    pub rate: Num,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterKind {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
    Lp1,
    Hp1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    pub kind: FilterKind,
    pub cutoff: Param,
    pub q: Num,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fx {
    Distort { drive: Num, mix: Num },
    Bitcrush { bits: Num, rate: Num },
    Delay { time: Num, feedback: Num, mix: Num, damp: Num },
    Reverb { room: Num, damp: Num, mix: Num, width: Num },
    Tremolo { rate: Num, depth: Num },
}

impl Fx {
    pub fn has_tail(&self) -> bool {
        matches!(self, Fx::Delay { .. } | Fx::Reverb { .. })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Repeat {
    pub count: u32,
    pub interval: Num,
    pub jitter: Num,
    pub gain_db_step: Num,
    pub pitch_jitter: Num,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub at: Num,
    pub length: Option<Num>,
    pub gain_db: Num,
    pub pan: Num,
    pub source: Source,
    pub env: Option<Env>,
    pub vibrato: Option<Vibrato>,
    pub arp: Option<Arp>,
    pub filters: Vec<Filter>,
    pub fx: Vec<Fx>,
    pub repeat: Option<Repeat>,
    /// Stream key for this layer's per-instance draws (repeat jitter, noise seeds): from its
    /// name when it has one, else its index.
    pub key: u64,
}

/// A parsed, validated sound recipe.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    pub name: String,
    pub bus: Bus,
    pub seed: u64,
    pub duration: f64,
    pub channels: u16,
    /// Seamless-loop crossfade in seconds (`None` = one-shot).
    pub loop_crossfade: Option<f64>,
    pub gain_db: Num,
    /// Target integrated loudness (LUFS-ish); the render is scaled to meet it.
    pub loudness: Option<f64>,
    pub peak_ceiling_db: f64,
    pub vary_pitch: Option<(f64, f64)>,
    pub vary_gain_db: Option<(f64, f64)>,
    pub layers: Vec<Layer>,
    pub fx: Vec<Fx>,
    pub notes: String,
}

/// Every top-level field, for tools that describe the format.
pub const RECIPE_FIELDS: &[&str] =
    &["sound", "name", "bus", "seed", "duration", "channels", "loop", "gain_db", "loudness", "peak_ceiling_db", "vary", "layers", "fx", "notes"];
pub const LAYER_FIELDS: &[&str] = &["name", "at", "length", "gain_db", "pan", "source", "env", "vibrato", "arp", "filters", "fx", "repeat"];
pub const SOURCE_FIELDS: &[&str] = &["wave", "color", "freq", "duty", "partials", "fm", "unison", "phase"];
pub const WAVE_NAMES: &[&str] = &["sine", "square", "saw", "triangle", "noise"];
pub const FILTER_NAMES: &[&str] = &["lowpass", "highpass", "bandpass", "notch", "lp1", "hp1"];
pub const FX_NAMES: &[&str] = &["distort", "bitcrush", "delay", "reverb", "tremolo"];

struct Cx<'a> {
    path: String,
    pairs: &'a [(String, Json)],
}

fn wrong(path: &str, what: &str) -> AudioError {
    AudioError::at(ErrorCode::WrongType, path, format!("expected {what}"))
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

impl<'a> Cx<'a> {
    fn new(j: &'a Json, path: &str, allowed: &[&str]) -> Result<Cx<'a>> {
        let pairs = j.as_object().ok_or_else(|| wrong(path, "an object"))?;
        for (i, (k, _)) in pairs.iter().enumerate() {
            if !allowed.contains(&k.as_str()) {
                return Err(AudioError::at(ErrorCode::UnknownField, join(path, k), format!("unknown field `{k}` (allowed: {})", allowed.join(", "))));
            }
            if pairs[..i].iter().any(|(p, _)| p == k) {
                return Err(AudioError::at(ErrorCode::Duplicate, join(path, k), format!("duplicate field `{k}`")));
            }
        }
        Ok(Cx { path: path.to_string(), pairs })
    }

    fn get(&self, key: &str) -> Option<&'a Json> {
        self.pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn p(&self, key: &str) -> String {
        join(&self.path, key)
    }

    fn need(&self, key: &str) -> Result<&'a Json> {
        self.get(key).ok_or_else(|| AudioError::at(ErrorCode::MissingField, self.p(key), format!("missing required field `{key}`")))
    }

    fn num(&self, key: &str, lo: f64, hi: f64, default: f64) -> Result<Num> {
        match self.get(key) {
            Some(j) => num(j, &self.p(key), lo, hi),
            None => Ok(Num::fixed(default)),
        }
    }

    fn opt_num(&self, key: &str, lo: f64, hi: f64) -> Result<Option<Num>> {
        self.get(key).map(|j| num(j, &self.p(key), lo, hi)).transpose()
    }

    fn param(&self, key: &str, lo: f64, hi: f64, default: f64) -> Result<Param> {
        match self.get(key) {
            Some(j) => param(j, &self.p(key), lo, hi),
            None => Ok(Param::Const(Num::fixed(default))),
        }
    }

    fn str(&self, key: &str) -> Result<Option<&'a str>> {
        self.get(key).map(|j| j.as_str().ok_or_else(|| wrong(&self.p(key), "a string"))).transpose()
    }

    fn array(&self, key: &str, max: usize) -> Result<&'a [Json]> {
        match self.get(key) {
            None => Ok(&[]),
            Some(j) => {
                let a = j.as_array().ok_or_else(|| wrong(&self.p(key), "an array"))?;
                if a.len() > max {
                    return Err(AudioError::at(ErrorCode::Limit, self.p(key), "too many entries").with_numbers(a.len() as f64, max as f64));
                }
                Ok(a)
            }
        }
    }
}

fn range_check(v: f64, path: &str, lo: f64, hi: f64) -> Result<f64> {
    if !v.is_finite() || v < lo || v > hi {
        return Err(AudioError::at(ErrorCode::OutOfRange, path, format!("must be within {lo} ..= {hi}")).with_numbers(v, if v < lo { lo } else { hi }));
    }
    Ok(v)
}

/// Stream key for a recipe path. The first segment (`layers[2]`) and the rest
/// (`source.freq.rand`) are hashed separately and XORed, so [`key_layer`] can move a whole
/// layer's draws from its index to its name.
fn path_key(path: &str) -> u64 {
    let (head, tail) = path.split_once('.').unwrap_or((path, ""));
    hash_str(head).rotate_left(17) ^ hash_str(tail)
}

/// The stream-key head of layer `index`: its name when it has one (so inserting, removing or
/// reordering other layers never changes its random draws), else its index.
fn layer_head(layer: &Layer, index: usize) -> String {
    if layer.name.is_empty() {
        format!("layers[{index}]")
    } else {
        format!("layers{{{}}}", layer.name)
    }
}

/// Re-keys every random number in a layer (parsed under `layers[index]`) to [`layer_head`].
fn key_layer(l: &mut Layer, index: usize) {
    let head = layer_head(l, index);
    let delta = hash_str(&format!("layers[{index}]")).rotate_left(17) ^ hash_str(&head).rotate_left(17);
    l.key = hash_str(&head);
    let mut f = |n: &mut Num| {
        if n.key != 0 {
            n.key ^= delta;
        }
    };
    let param = |p: &mut Param, f: &mut dyn FnMut(&mut Num)| match p {
        Param::Const(n) => f(n),
        Param::Points { points, .. } => points.iter_mut().for_each(|(t, v)| {
            f(t);
            f(v);
        }),
    };
    for n in [&mut l.at, &mut l.gain_db, &mut l.pan] {
        f(n);
    }
    if let Some(n) = &mut l.length {
        f(n);
    }
    let s = &mut l.source;
    if let Some(p) = &mut s.freq {
        param(p, &mut f);
    }
    param(&mut s.duty, &mut f);
    f(&mut s.phase);
    for p in &mut s.partials {
        f(&mut p.ratio);
        f(&mut p.gain);
        if let Some(d) = &mut p.decay {
            f(d);
        }
    }
    if let Some(fm) = &mut s.fm {
        f(&mut fm.ratio);
        param(&mut fm.index, &mut f);
    }
    if let Some(u) = &mut s.unison {
        f(&mut u.cents);
    }
    match &mut l.env {
        Some(Env::Adsr { attack, hold, decay, sustain, release, .. }) => {
            for n in [attack, hold, decay, sustain, release] {
                f(n);
            }
        }
        Some(Env::Points(points)) => points.iter_mut().for_each(|(t, v)| {
            f(t);
            f(v);
        }),
        None => {}
    }
    if let Some(v) = &mut l.vibrato {
        for n in [&mut v.rate, &mut v.depth, &mut v.delay] {
            f(n);
        }
    }
    if let Some(a) = &mut l.arp {
        f(&mut a.rate);
    }
    for flt in &mut l.filters {
        param(&mut flt.cutoff, &mut f);
        f(&mut flt.q);
    }
    for fx in &mut l.fx {
        match fx {
            Fx::Distort { drive, mix } => [drive, mix].into_iter().for_each(&mut f),
            Fx::Bitcrush { bits, rate } => [bits, rate].into_iter().for_each(&mut f),
            Fx::Delay { time, feedback, mix, damp } => [time, feedback, mix, damp].into_iter().for_each(&mut f),
            Fx::Reverb { room, damp, mix, width } => [room, damp, mix, width].into_iter().for_each(&mut f),
            Fx::Tremolo { rate, depth } => [rate, depth].into_iter().for_each(&mut f),
        }
    }
    if let Some(r) = &mut l.repeat {
        for n in [&mut r.interval, &mut r.jitter, &mut r.gain_db_step, &mut r.pitch_jitter] {
            f(n);
        }
    }
}

fn plain_number(j: &Json, path: &str) -> Result<f64> {
    j.as_f64().ok_or_else(|| wrong(path, "a number"))
}

/// A number or `{"rand": [lo, hi]}`, within `lo..=hi`.
pub(crate) fn num(j: &Json, path: &str, lo: f64, hi: f64) -> Result<Num> {
    if let Some(v) = j.as_f64() {
        return Ok(Num { lo: range_check(v, path, lo, hi)?, hi: v, key: 0 });
    }
    let cx = Cx::new(j, path, &["rand"]).map_err(|_| wrong(path, "a number or {\"rand\": [lo, hi]}"))?;
    let r = cx.need("rand")?;
    let rp = cx.p("rand");
    let a = r.as_array().filter(|a| a.len() == 2).ok_or_else(|| wrong(&rp, "[lo, hi]"))?;
    let a0 = range_check(plain_number(&a[0], &format!("{rp}[0]"))?, &format!("{rp}[0]"), lo, hi)?;
    let a1 = range_check(plain_number(&a[1], &format!("{rp}[1]"))?, &format!("{rp}[1]"), lo, hi)?;
    if a0 > a1 {
        return Err(AudioError::at(ErrorCode::OutOfRange, rp, "rand range must be [lo, hi] with lo <= hi").with_numbers(a0, a1));
    }
    Ok(Num { lo: a0, hi: a1, key: path_key(path) })
}

fn points(j: &Json, path: &str, lo: f64, hi: f64, positive: bool) -> Result<Vec<(Num, Num)>> {
    let a = j.as_array().ok_or_else(|| wrong(path, "an array of [t, value] pairs"))?;
    if a.is_empty() || a.len() > MAX_POINTS {
        return Err(AudioError::at(ErrorCode::Limit, path, "need 1 ..= 256 points").with_numbers(a.len() as f64, MAX_POINTS as f64));
    }
    let mut out = Vec::with_capacity(a.len());
    let mut last_t = 0.0;
    for (i, p) in a.iter().enumerate() {
        let pp = format!("{path}[{i}]");
        let pair = p.as_array().filter(|x| x.len() == 2).ok_or_else(|| wrong(&pp, "[t, value]"))?;
        let t = num(&pair[0], &format!("{pp}[0]"), 0.0, MAX_DURATION_S)?;
        let v = num(&pair[1], &format!("{pp}[1]"), lo, hi)?;
        if t.lo < last_t {
            return Err(AudioError::at(ErrorCode::OutOfRange, format!("{pp}[0]"), "point times must not decrease").with_numbers(t.lo, last_t));
        }
        if positive && v.lo <= 0.0 {
            return Err(AudioError::at(ErrorCode::OutOfRange, format!("{pp}[1]"), "exp curves need values > 0").with_numbers(v.lo, 0.0));
        }
        last_t = t.hi;
        out.push((t, v));
    }
    Ok(out)
}

/// A [`Num`], or `{"points": [[t, v], ...], "curve": "linear" | "exp"}`.
pub(crate) fn param(j: &Json, path: &str, lo: f64, hi: f64) -> Result<Param> {
    if j.as_f64().is_some() || j.get("rand").is_some() {
        return Ok(Param::Const(num(j, path, lo, hi)?));
    }
    let cx = Cx::new(j, path, &["points", "curve"]).map_err(|e| if e.code == ErrorCode::UnknownField { e } else { wrong(path, "a number, {\"rand\": [lo, hi]} or {\"points\": [[t, v], ...]}") })?;
    let exp = match cx.str("curve")? {
        None | Some("linear") => false,
        Some("exp") => true,
        Some(other) => return Err(AudioError::at(ErrorCode::UnknownName, cx.p("curve"), format!("unknown curve `{other}` (linear, exp)"))),
    };
    let pts = points(cx.need("points")?, &cx.p("points"), lo, hi, exp)?;
    Ok(Param::Points { points: pts, exp })
}

fn source(j: &Json, path: &str) -> Result<Source> {
    let cx = Cx::new(j, path, SOURCE_FIELDS)?;
    let wave_name = cx.str("wave")?.ok_or_else(|| AudioError::at(ErrorCode::MissingField, cx.p("wave"), "missing required field `wave`"))?;
    let wave = match wave_name {
        "sine" => Wave::Sine,
        "square" => Wave::Square,
        "saw" => Wave::Saw,
        "triangle" => Wave::Triangle,
        "noise" => Wave::Noise(match cx.str("color")? {
            None | Some("white") => NoiseColor::White,
            Some("pink") => NoiseColor::Pink,
            Some("brown") => NoiseColor::Brown,
            Some(o) => return Err(AudioError::at(ErrorCode::UnknownName, cx.p("color"), format!("unknown noise color `{o}` (white, pink, brown)"))),
        }),
        o => return Err(AudioError::at(ErrorCode::UnknownName, cx.p("wave"), format!("unknown wave `{o}` ({})", WAVE_NAMES.join(", ")))),
    };
    let noise = matches!(wave, Wave::Noise(_));
    if !noise && cx.get("color").is_some() {
        return Err(AudioError::at(ErrorCode::UnknownField, cx.p("color"), "`color` only applies to noise"));
    }
    let freq = match cx.get("freq") {
        Some(f) => Some(param(f, &cx.p("freq"), 1.0, 24_000.0)?),
        None if noise => None,
        None => return Err(AudioError::at(ErrorCode::MissingField, cx.p("freq"), "tonal waves need `freq` (Hz)")),
    };
    if wave != Wave::Square && cx.get("duty").is_some() {
        return Err(AudioError::at(ErrorCode::UnknownField, cx.p("duty"), "`duty` only applies to square"));
    }
    let duty = cx.param("duty", 0.01, 0.99, 0.5)?;
    let mut partials = Vec::new();
    let parr = cx.array("partials", MAX_PARTIALS)?;
    if !parr.is_empty() && wave != Wave::Sine {
        return Err(AudioError::at(ErrorCode::UnknownField, cx.p("partials"), "`partials` only applies to sine"));
    }
    for (i, p) in parr.iter().enumerate() {
        let pp = format!("{}[{i}]", cx.p("partials"));
        let a = p.as_array().filter(|a| a.len() == 2 || a.len() == 3).ok_or_else(|| wrong(&pp, "[ratio, gain] or [ratio, gain, decay_s]"))?;
        partials.push(Partial {
            ratio: num(&a[0], &format!("{pp}[0]"), 0.001, 64.0)?,
            gain: num(&a[1], &format!("{pp}[1]"), -4.0, 4.0)?,
            decay: a.get(2).map(|d| num(d, &format!("{pp}[2]"), 0.001, MAX_DURATION_S)).transpose()?,
        });
    }
    let fm = match cx.get("fm") {
        Some(f) => {
            if wave != Wave::Sine {
                return Err(AudioError::at(ErrorCode::UnknownField, cx.p("fm"), "`fm` only applies to sine"));
            }
            let fcx = Cx::new(f, &cx.p("fm"), &["ratio", "index"])?;
            Some(Fm { ratio: fcx.num("ratio", 0.001, 32.0, 1.0)?, index: fcx.param("index", 0.0, 50.0, 1.0)? })
        }
        None => None,
    };
    let unison = match cx.get("unison") {
        Some(u) => {
            if noise {
                return Err(AudioError::at(ErrorCode::UnknownField, cx.p("unison"), "`unison` doesn't apply to noise"));
            }
            let ucx = Cx::new(u, &cx.p("unison"), &["voices", "cents"])?;
            let vp = ucx.p("voices");
            let voices = ucx.get("voices").map(|v| v.as_u64().ok_or_else(|| wrong(&vp, "an integer"))).transpose()?.unwrap_or(3);
            range_check(voices as f64, &vp, 1.0, 8.0)?;
            Some(Unison { voices: voices as u32, cents: ucx.num("cents", 0.0, 100.0, 10.0)? })
        }
        None => None,
    };
    Ok(Source { wave, freq, duty, partials, fm, unison, phase: cx.num("phase", 0.0, 1.0, 0.0)? })
}

fn env(j: &Json, path: &str) -> Result<Env> {
    if j.get("points").is_some() {
        let cx = Cx::new(j, path, &["points"])?;
        return Ok(Env::Points(points(cx.need("points")?, &cx.p("points"), 0.0, 4.0, false)?));
    }
    let cx = Cx::new(j, path, &["attack", "hold", "decay", "sustain", "release", "curve"])?;
    let exp = match cx.str("curve")? {
        None | Some("exp") => true,
        Some("linear") => false,
        Some(o) => return Err(AudioError::at(ErrorCode::UnknownName, cx.p("curve"), format!("unknown curve `{o}` (exp, linear)"))),
    };
    let t = MAX_DURATION_S;
    Ok(Env::Adsr {
        attack: cx.num("attack", 0.0, t, 0.002)?,
        hold: cx.num("hold", 0.0, t, 0.0)?,
        decay: cx.num("decay", 0.0, t, 0.1)?,
        sustain: cx.num("sustain", 0.0, 1.0, 1.0)?,
        release: cx.num("release", 0.0, t, 0.05)?,
        exp,
    })
}

fn fx_list(cx: &Cx, key: &str, max: usize) -> Result<Vec<Fx>> {
    let mut out = Vec::new();
    for (i, f) in cx.array(key, max)?.iter().enumerate() {
        let p = format!("{}[{i}]", cx.p(key));
        let kind = f.get("type").and_then(Json::as_str).ok_or_else(|| AudioError::at(ErrorCode::MissingField, format!("{p}.type"), "missing `type`"))?;
        let fx = match kind {
            "distort" => {
                let c = Cx::new(f, &p, &["type", "drive", "mix"])?;
                Fx::Distort { drive: c.num("drive", 1.0, 100.0, 4.0)?, mix: c.num("mix", 0.0, 1.0, 1.0)? }
            }
            "bitcrush" => {
                let c = Cx::new(f, &p, &["type", "bits", "rate"])?;
                Fx::Bitcrush { bits: c.num("bits", 1.0, 24.0, 8.0)?, rate: c.num("rate", 100.0, 192_000.0, 192_000.0)? }
            }
            "delay" => {
                let c = Cx::new(f, &p, &["type", "time", "feedback", "mix", "damp"])?;
                Fx::Delay {
                    time: c.num("time", 0.001, 2.0, 0.25)?,
                    feedback: c.num("feedback", 0.0, 0.95, 0.35)?,
                    mix: c.num("mix", 0.0, 1.0, 0.3)?,
                    damp: c.num("damp", 0.0, 1.0, 0.3)?,
                }
            }
            "reverb" => {
                let c = Cx::new(f, &p, &["type", "room", "damp", "mix", "width"])?;
                Fx::Reverb {
                    room: c.num("room", 0.0, 1.0, 0.6)?,
                    damp: c.num("damp", 0.0, 1.0, 0.5)?,
                    mix: c.num("mix", 0.0, 1.0, 0.25)?,
                    width: c.num("width", 0.0, 1.0, 1.0)?,
                }
            }
            "tremolo" => {
                let c = Cx::new(f, &p, &["type", "rate", "depth"])?;
                Fx::Tremolo { rate: c.num("rate", 0.0, 200.0, 6.0)?, depth: c.num("depth", 0.0, 1.0, 0.5)? }
            }
            o => return Err(AudioError::at(ErrorCode::UnknownName, format!("{p}.type"), format!("unknown fx `{o}` ({})", FX_NAMES.join(", ")))),
        };
        out.push(fx);
    }
    Ok(out)
}

fn layer(j: &Json, path: &str) -> Result<Layer> {
    let cx = Cx::new(j, path, LAYER_FIELDS)?;
    let t = MAX_DURATION_S;
    let mut filters = Vec::new();
    for (i, f) in cx.array("filters", MAX_FILTERS)?.iter().enumerate() {
        let p = format!("{}[{i}]", cx.p("filters"));
        let fc = Cx::new(f, &p, &["type", "cutoff", "q"])?;
        let kind = match fc.str("type")?.ok_or_else(|| AudioError::at(ErrorCode::MissingField, fc.p("type"), "missing `type`"))? {
            "lowpass" => FilterKind::Lowpass,
            "highpass" => FilterKind::Highpass,
            "bandpass" => FilterKind::Bandpass,
            "notch" => FilterKind::Notch,
            "lp1" => FilterKind::Lp1,
            "hp1" => FilterKind::Hp1,
            o => return Err(AudioError::at(ErrorCode::UnknownName, fc.p("type"), format!("unknown filter `{o}` ({})", FILTER_NAMES.join(", ")))),
        };
        if matches!(kind, FilterKind::Lp1 | FilterKind::Hp1) && fc.get("q").is_some() {
            return Err(AudioError::at(ErrorCode::UnknownField, fc.p("q"), "one-pole filters have no `q`"));
        }
        let cutoff = param(fc.need("cutoff")?, &fc.p("cutoff"), 10.0, 24_000.0)?;
        filters.push(Filter { kind, cutoff, q: fc.num("q", 0.1, 40.0, std::f64::consts::FRAC_1_SQRT_2)? });
    }
    let vibrato = match cx.get("vibrato") {
        Some(v) => {
            let c = Cx::new(v, &cx.p("vibrato"), &["rate", "depth", "delay"])?;
            Some(Vibrato { rate: c.num("rate", 0.0, 40.0, 5.0)?, depth: c.num("depth", 0.0, 24.0, 0.3)?, delay: c.num("delay", 0.0, t, 0.0)? })
        }
        None => None,
    };
    let arp = match cx.get("arp") {
        Some(a) => {
            let c = Cx::new(a, &cx.p("arp"), &["steps", "rate"])?;
            let sp = c.p("steps");
            let steps = c.need("steps")?.as_array().ok_or_else(|| wrong(&sp, "an array of semitones"))?;
            if steps.is_empty() || steps.len() > 32 {
                return Err(AudioError::at(ErrorCode::Limit, sp, "need 1 ..= 32 steps").with_numbers(steps.len() as f64, 32.0));
            }
            let steps = steps
                .iter()
                .enumerate()
                .map(|(i, s)| plain_number(s, &format!("{sp}[{i}]")).and_then(|v| range_check(v, &format!("{sp}[{i}]"), -48.0, 48.0)))
                .collect::<Result<Vec<f64>>>()?;
            Some(Arp { steps, rate: c.num("rate", 0.01, 100.0, 12.0)? })
        }
        None => None,
    };
    let repeat = match cx.get("repeat") {
        Some(r) => {
            let c = Cx::new(r, &cx.p("repeat"), &["count", "interval", "jitter", "gain_db_step", "pitch_jitter"])?;
            let cp = c.p("count");
            let count = c.need("count")?.as_u64().ok_or_else(|| wrong(&cp, "an integer"))?;
            range_check(count as f64, &cp, 1.0, MAX_REPEAT as f64)?;
            Some(Repeat {
                count: count as u32,
                interval: c.num("interval", 0.0, t, 0.1)?,
                jitter: c.num("jitter", 0.0, 10.0, 0.0)?,
                gain_db_step: c.num("gain_db_step", -48.0, 48.0, 0.0)?,
                pitch_jitter: c.num("pitch_jitter", 0.0, 24.0, 0.0)?,
            })
        }
        None => None,
    };
    Ok(Layer {
        name: cx.str("name")?.unwrap_or("").to_string(),
        at: cx.num("at", 0.0, t, 0.0)?,
        length: cx.opt_num("length", 0.0, t)?,
        gain_db: cx.num("gain_db", -96.0, 48.0, 0.0)?,
        pan: cx.num("pan", -1.0, 1.0, 0.0)?,
        source: source(cx.need("source")?, &cx.p("source"))?,
        env: cx.get("env").map(|e| env(e, &cx.p("env"))).transpose()?,
        vibrato,
        arp,
        filters,
        fx: fx_list(&cx, "fx", MAX_FX)?,
        repeat,
        key: 0,
    })
}

fn pair(j: &Json, path: &str, lo: f64, hi: f64) -> Result<(f64, f64)> {
    let a = j.as_array().filter(|a| a.len() == 2).ok_or_else(|| wrong(path, "[lo, hi]"))?;
    let x = range_check(plain_number(&a[0], &format!("{path}[0]"))?, &format!("{path}[0]"), lo, hi)?;
    let y = range_check(plain_number(&a[1], &format!("{path}[1]"))?, &format!("{path}[1]"), lo, hi)?;
    if x > y {
        return Err(AudioError::at(ErrorCode::OutOfRange, path, "range must be [lo, hi] with lo <= hi").with_numbers(x, y));
    }
    Ok((x, y))
}

impl Recipe {
    /// Parses and validates recipe JSON text.
    pub fn parse(text: &str) -> Result<Recipe> {
        let j = Json::parse(text).map_err(|e| AudioError::new(ErrorCode::Json, e.message).with_offset(e.offset))?;
        Recipe::from_json(&j)
    }

    pub fn from_json(j: &Json) -> Result<Recipe> {
        let cx = Cx::new(j, "", RECIPE_FIELDS)?;
        let v = cx.need("sound")?;
        if v.as_u64() != Some(1) {
            return Err(AudioError::at(ErrorCode::OutOfRange, "sound", "unsupported recipe version (this engine reads `\"sound\": 1`)").with_numbers(v.as_f64().unwrap_or(f64::NAN), 1.0));
        }
        let name = cx.str("name")?.ok_or_else(|| AudioError::at(ErrorCode::MissingField, "name", "missing required field `name`"))?;
        if name.is_empty() || name.len() > 64 || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
            return Err(AudioError::at(ErrorCode::OutOfRange, "name", "name must be 1-64 chars of [a-z0-9_]"));
        }
        let bus = match cx.str("bus")? {
            None => Bus::Sfx,
            Some(b) => Bus::from_name(b).ok_or_else(|| AudioError::at(ErrorCode::UnknownName, "bus", format!("unknown bus `{b}` (master, sfx, ui, ambience, music)")))?,
        };
        let seed = match cx.get("seed") {
            Some(s) => s.as_u64().ok_or_else(|| wrong("seed", "a non-negative integer"))?,
            None => 0,
        };
        let duration = range_check(plain_number(cx.need("duration")?, "duration")?, "duration", 0.001, MAX_DURATION_S)?;
        let channels = match cx.get("channels") {
            None => 1,
            Some(c) => match c.as_u64() {
                Some(1) => 1,
                Some(2) => 2,
                _ => return Err(AudioError::at(ErrorCode::OutOfRange, "channels", "channels must be 1 or 2")),
            },
        };
        let loop_crossfade = match cx.get("loop") {
            None => None,
            Some(l) => {
                let lc = Cx::new(l, "loop", &["crossfade"])?;
                let x = lc.get("crossfade").map(|x| plain_number(x, "loop.crossfade")).transpose()?.unwrap_or(duration * 0.1);
                Some(range_check(x, "loop.crossfade", 0.001, duration * 0.5)?)
            }
        };
        let (mut vary_pitch, mut vary_gain_db) = (None, None);
        if let Some(v) = cx.get("vary") {
            let vc = Cx::new(v, "vary", &["pitch", "gain_db"])?;
            vary_pitch = vc.get("pitch").map(|p| pair(p, "vary.pitch", -24.0, 24.0)).transpose()?;
            vary_gain_db = vc.get("gain_db").map(|p| pair(p, "vary.gain_db", -24.0, 24.0)).transpose()?;
        }
        let larr = cx.need("layers")?.as_array().ok_or_else(|| wrong("layers", "an array"))?;
        if larr.is_empty() || larr.len() > MAX_LAYERS {
            return Err(AudioError::at(ErrorCode::Limit, "layers", "need 1 ..= 32 layers").with_numbers(larr.len() as f64, MAX_LAYERS as f64));
        }
        let mut layers = larr.iter().enumerate().map(|(i, l)| layer(l, &format!("layers[{i}]"))).collect::<Result<Vec<_>>>()?;
        for i in 0..layers.len() {
            let name = layers[i].name.clone();
            if !name.is_empty() && layers[..i].iter().any(|l| l.name == name) {
                return Err(AudioError::at(ErrorCode::Duplicate, format!("layers[{i}].name"), format!("duplicate layer name `{name}` (names key the layer's random draws)")));
            }
            key_layer(&mut layers[i], i);
        }
        let recipe = Recipe {
            name: name.to_string(),
            bus,
            seed,
            duration,
            channels,
            loop_crossfade,
            gain_db: cx.num("gain_db", -96.0, 48.0, 0.0)?,
            loudness: cx.get("loudness").map(|l| plain_number(l, "loudness").and_then(|v| range_check(v, "loudness", -70.0, 0.0))).transpose()?,
            peak_ceiling_db: cx.get("peak_ceiling_db").map(|l| plain_number(l, "peak_ceiling_db").and_then(|v| range_check(v, "peak_ceiling_db", -40.0, 0.0))).transpose()?.unwrap_or(-1.0),
            vary_pitch,
            vary_gain_db,
            fx: fx_list(&cx, "fx", MAX_FX)?,
            notes: cx.str("notes")?.unwrap_or("").to_string(),
            layers,
        };
        let work = recipe.work_estimate(48_000);
        if work > MAX_WORK {
            return Err(AudioError::at(ErrorCode::Limit, "layers", "recipe is too expensive to render (estimated cost of layers × repeats × length × partials × fx)").with_numbers(work, MAX_WORK));
        }
        Ok(recipe)
    }

    /// Estimated synthesis cost at `rate`, in cost units (≈ ns of one modern core, release
    /// build). Each layer instance is charged per sample for what it actually runs (a sine
    /// partial ≈ one `sin`, a reverb ≈ 12 delay lines…) over the frames it can actually sound
    /// (its gate + release, or to the end when it has a delay/reverb tail).
    pub fn work_estimate(&self, rate: u32) -> f64 {
        let rate = rate as f64;
        let total = self.duration + self.loop_crossfade.unwrap_or(0.0);
        let layers: f64 = self
            .layers
            .iter()
            .map(|l| {
                let count = l.repeat.as_ref().map_or(1, |r| r.count) as f64;
                let remaining = (total - l.at.lo).max(0.0);
                let tail = l.fx.iter().any(Fx::has_tail);
                let gate = l.length.map_or(remaining, |n| n.hi);
                let sounding = match &l.env {
                    _ if tail => remaining,
                    None => gate,
                    Some(Env::Adsr { attack, hold, decay, sustain, release, .. }) => {
                        if sustain.hi == 0.0 {
                            gate.min(attack.hi + hold.hi + decay.hi) + release.hi
                        } else {
                            gate + release.hi
                        }
                    }
                    Some(Env::Points(p)) => match p.last() {
                        Some((t, v)) if v.hi == 0.0 => t.hi,
                        _ => remaining,
                    },
                };
                count * sounding.min(remaining) * rate * layer_cost(l)
            })
            .sum();
        let master: f64 = self.fx.iter().map(fx_cost).sum::<f64>() * self.channels as f64 * total * rate;
        layers + master
    }
}

/// Per-sample cost units, calibrated in a release build on the reviewer's worst-case probes
/// (reverb stacks, 32 partials × 8 unison voices, FM, saw stacks with every effect): 1 unit ≈ 1 ns.
mod cost {
    pub const BASE: f64 = 6.0;
    /// One `dmath` sin/cos/exp/exp2 call.
    pub const SIN: f64 = 25.0;
    pub const BLEP: f64 = 8.0;
    pub const NOISE: f64 = 3.0;
    pub const EXP: f64 = 25.0;
    /// Includes a `tan` every 16 samples when the cutoff moves.
    pub const SVF: f64 = 5.0;
    /// 8 combs + 4 all-passes.
    pub const REVERB: f64 = 34.0;
    pub const DELAY: f64 = 3.0;
    pub const DISTORT: f64 = 3.0;
    pub const BITCRUSH: f64 = 6.0;
    pub const TREMOLO: f64 = 25.0;
}

fn fx_cost(f: &Fx) -> f64 {
    match f {
        Fx::Reverb { .. } => cost::REVERB,
        Fx::Delay { .. } => cost::DELAY,
        Fx::Distort { .. } => cost::DISTORT,
        Fx::Bitcrush { .. } => cost::BITCRUSH,
        Fx::Tremolo { .. } => cost::TREMOLO,
    }
}

fn layer_cost(l: &Layer) -> f64 {
    let s = &l.source;
    let voices = s.unison.as_ref().map_or(1, |u| u.voices) as f64;
    let osc = match s.wave {
        Wave::Sine => voices * s.partials.len().max(1) as f64 * cost::SIN + if s.fm.is_some() { cost::SIN } else { 0.0 },
        Wave::Noise(_) => cost::NOISE,
        _ => voices * cost::BLEP,
    };
    let pitch_mod = if l.vibrato.is_some() || l.arp.is_some() { cost::EXP + cost::SIN } else { 0.0 };
    let env = match &l.env {
        Some(Env::Adsr { exp: true, .. }) => cost::EXP,
        _ => 0.0,
    };
    let filters = l.filters.len() as f64 * cost::SVF;
    cost::BASE + osc + pitch_mod + env + filters + l.fx.iter().map(fx_cost).sum::<f64>()
}

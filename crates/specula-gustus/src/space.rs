//! A gallery's parameter space: each parameter maps one to three dimensions of the unit cube to
//! a value (a number, an Oklch colour, a choice), names what the value drives in the render (its
//! [`Target`]) and, optionally, where an accepted value lives in the project ([`Apply`]).

use crate::colour::{self, Rgb};
use crate::{r4, Result, TasteError};
use specula_core::{dmath, Json};
use std::collections::BTreeMap;

/// Parameters in one space.
pub const MAX_PARAMS: usize = 8;
/// Dimensions in one space (a colour is three).
pub const MAX_DIMS: usize = 12;
/// Options of one choice parameter.
pub const MAX_OPTIONS: usize = 12;
/// Whether a file relative to the assets folder is taste state (`taste/...`, in any case, with
/// any `.` or empty segments ahead of it): never a gallery's apply target (taste review 2: an
/// agent's gallery applied into another gallery's file and changed the owner's pick).
pub fn is_taste_file(file: &str) -> bool {
    file.split('/').find(|s| !s.is_empty() && *s != ".").is_some_and(|s| s.eq_ignore_ascii_case("taste"))
}

/// Two displayed colours closer than this (Oklab) count as one on a sheet.
pub const MIN_SHOWN_DE: f64 = 0.02;
/// The same for an interface colour: thin text needs a wider gap to read apart (a sight-skeptic
/// finding: "can't afford" labels 0.04 apart looked the same at window size).
pub const MIN_SHOWN_DE_UI: f64 = 0.06;

/// What a value drives when a variant is rendered.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    KeyColor,
    KeyIntensity,
    KeyElevation,
    KeyAzimuth,
    AmbientColor,
    AmbientIntensity,
    Exposure,
    FogDensity,
    FogColor,
    Backdrop,
    Ground,
    /// `scale.x` (0), `.y` (1), `.z` (2), `.all` (3).
    Scale(u8),
    /// A recipe parameter.
    Recipe(String),
    /// A `$name` colour of a `ui` subject.
    Ui(String),
    /// A base style ([`crate::presets`]).
    Preset,
}

/// The targets as agents name them (for errors and schemas).
pub const TARGETS: &str = "light.key_color, light.key_intensity, light.key_elevation_deg, light.key_azimuth_deg, light.ambient_color, light.ambient_intensity, exposure, fog.density, fog.color, backdrop, ground, scale.x|y|z|all, recipe.<param>, ui.<name>, preset";

impl Target {
    pub fn parse(s: &str) -> Option<Target> {
        Some(match s {
            "light.key_color" => Target::KeyColor,
            "light.key_intensity" => Target::KeyIntensity,
            "light.key_elevation_deg" => Target::KeyElevation,
            "light.key_azimuth_deg" => Target::KeyAzimuth,
            "light.ambient_color" => Target::AmbientColor,
            "light.ambient_intensity" => Target::AmbientIntensity,
            "exposure" => Target::Exposure,
            "fog.density" => Target::FogDensity,
            "fog.color" => Target::FogColor,
            "backdrop" => Target::Backdrop,
            "ground" => Target::Ground,
            "scale.x" => Target::Scale(0),
            "scale.y" => Target::Scale(1),
            "scale.z" => Target::Scale(2),
            "scale.all" => Target::Scale(3),
            "preset" => Target::Preset,
            _ => {
                if let Some(p) = s.strip_prefix("recipe.").filter(|p| ident(p)) {
                    Target::Recipe(p.to_owned())
                } else {
                    Target::Ui(s.strip_prefix("ui.").filter(|p| ident(p))?.to_owned())
                }
            }
        })
    }

    pub fn name(&self) -> String {
        match self {
            Target::KeyColor => "light.key_color".into(),
            Target::KeyIntensity => "light.key_intensity".into(),
            Target::KeyElevation => "light.key_elevation_deg".into(),
            Target::KeyAzimuth => "light.key_azimuth_deg".into(),
            Target::AmbientColor => "light.ambient_color".into(),
            Target::AmbientIntensity => "light.ambient_intensity".into(),
            Target::Exposure => "exposure".into(),
            Target::FogDensity => "fog.density".into(),
            Target::FogColor => "fog.color".into(),
            Target::Backdrop => "backdrop".into(),
            Target::Ground => "ground".into(),
            Target::Scale(a) => format!("scale.{}", ["x", "y", "z", "all"][usize::from(*a).min(3)]),
            Target::Recipe(p) => format!("recipe.{p}"),
            Target::Ui(p) => format!("ui.{p}"),
            Target::Preset => "preset".into(),
        }
    }

    fn is_colour(&self) -> bool {
        matches!(self, Target::KeyColor | Target::AmbientColor | Target::FogColor | Target::Backdrop | Target::Ground | Target::Ui(_))
    }
}

fn ident(s: &str) -> bool {
    !s.is_empty() && s.len() <= 48 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A parameter's kind and range.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Number { min: f64, max: f64, step: Option<f64>, default: f64 },
    /// Oklch around `base` (`[L, C, hÂ°]`), Â± `spread` per axis.
    Colour { base: [f64; 3], spread: [f64; 3] },
    Choice { options: Vec<String>, default: usize },
}

/// Where an accepted value is written in the project.
#[derive(Clone, Debug, PartialEq)]
pub struct Apply {
    /// A project path relative to the assets mount (`style.toml`, `ui/theme.json`).
    pub file: String,
    pub at: ApplyAt,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ApplyAt {
    /// `table.key` (dotted tables: `stage.game.key_color`).
    Toml(String),
    /// A JSON path (`colors.cant_afford`).
    Json(String),
}

impl Apply {
    pub fn describe(&self) -> String {
        match &self.at {
            ApplyAt::Toml(k) => format!("{} {k}", self.file),
            ApplyAt::Json(k) => format!("{} {k}", self.file),
        }
    }

    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([("file", Json::from(self.file.as_str()))]);
        match &self.at {
            ApplyAt::Toml(k) => j.set("toml", Json::from(k.as_str())),
            ApplyAt::Json(k) => j.set("json", Json::from(k.as_str())),
        };
        j
    }
}

/// One parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub target: Target,
    pub kind: Kind,
    pub apply: Option<Apply>,
    /// Chips this parameter answers to, beyond the inferred ones: chip -> +1 / -1.
    pub chips: BTreeMap<String, f64>,
}

/// A value of a parameter.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Colour(Rgb),
    Choice(usize, String),
}

impl Value {
    pub fn to_json(&self) -> Json {
        match self {
            Value::Num(v) => r4(*v),
            Value::Colour(c) => Json::from(colour::hex(*c)),
            Value::Choice(_, s) => Json::from(s.as_str()),
        }
    }

    /// The value as a TOML literal.
    pub fn toml_literal(&self) -> String {
        match self {
            Value::Num(v) => {
                let r = dmath::round_f64(v * 10_000.0) / 10_000.0;
                if dmath::fract_f64(r) == 0.0 && r.abs() < 1e15 {
                    format!("{}", r as i64)
                } else {
                    format!("{r}")
                }
            }
            Value::Colour(c) => format!("\"{}\"", colour::hex(*c)),
            Value::Choice(_, s) => format!("\"{s}\""),
        }
    }

    pub fn as_num(&self) -> Option<f64> {
        match self {
            Value::Num(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_colour(&self) -> Option<Rgb> {
        match self {
            Value::Colour(c) => Some(*c),
            _ => None,
        }
    }
}

/// The edit chips, in the order the window shows them.
pub const CHIPS: [&str; 12] = ["darker", "brighter", "bigger", "smaller", "simpler", "more detail", "warmer", "cooler", "bolder", "subtler", "foggier", "clearer"];

const SIZE_WORDS: [&str; 8] = ["height", "width", "size", "scale", "length", "radius", "depth", "tall"];
const DETAIL_WORDS: [&str; 9] = ["detail", "segments", "count", "cracks", "runes", "tier", "density", "rings", "ornament"];

impl Param {
    /// How far apart two of this parameter's colours must be to count as different on a sheet.
    pub fn min_shown_de(&self) -> f64 {
        if matches!(self.target, Target::Ui(_)) {
            MIN_SHOWN_DE_UI
        } else {
            MIN_SHOWN_DE
        }
    }

    pub fn dims(&self) -> usize {
        match self.kind {
            Kind::Colour { .. } => 3,
            _ => 1,
        }
    }

    /// The value at unit coordinates `u` (`dims()` of them, each clamped to 0..1).
    pub fn value_at(&self, u: &[f64]) -> Value {
        let c = |i: usize| u.get(i).copied().unwrap_or(0.5).clamp(0.0, 1.0);
        match &self.kind {
            Kind::Number { min, max, step, .. } => {
                let mut v = min + (max - min) * c(0);
                if let Some(s) = step.filter(|s| *s > 0.0) {
                    v = (min + dmath::round_f64((v - min) / s) * s).clamp(*min, *max);
                }
                Value::Num(v)
            }
            Kind::Colour { base, spread } => {
                // Lightness and chroma map piecewise around the base, within what exists
                // (0..1, chroma >= 0): no two points clamp to the same colour.
                let l = around(c(0), base[0], spread[0], 0.0, 1.0);
                let ch = around(c(1), base[1], spread[1], 0.0, f64::INFINITY);
                let h = (base[2] + (2.0 * c(2) - 1.0) * spread[2]).rem_euclid(360.0);
                // Quantised to 8 bits a channel: the value is what `#rrggbb` writes.
                let c = colour::oklch_to_srgb([l, ch, h]);
                Value::Colour(colour::parse_hex(&colour::hex(c)).unwrap_or(c))
            }
            Kind::Choice { options, .. } => {
                let k = options.len().max(1);
                let i = ((c(0) * k as f64) as usize).min(k - 1);
                Value::Choice(i, options.get(i).cloned().unwrap_or_default())
            }
        }
    }

    /// Unit coordinates of `v` (clamped into the range).
    pub fn unit_of(&self, v: &Value) -> Vec<f64> {
        match (&self.kind, v) {
            (Kind::Number { min, max, .. }, Value::Num(x)) => vec![if max > min { ((x - min) / (max - min)).clamp(0.0, 1.0) } else { 0.5 }],
            (Kind::Colour { base, spread }, Value::Colour(c)) => {
                let lch = colour::srgb_to_oklch(*c);
                let f = |d: f64, s: f64| if s > 0.0 { (0.5 + d / (2.0 * s)).clamp(0.0, 1.0) } else { 0.5 };
                vec![around_inv(lch[0], base[0], spread[0], 0.0, 1.0), around_inv(lch[1], base[1], spread[1], 0.0, f64::INFINITY), f(colour::hue_delta(base[2], lch[2]), spread[2])]
            }
            (Kind::Choice { options, .. }, Value::Choice(i, _)) => vec![(*i as f64 + 0.5) / options.len().max(1) as f64],
            _ => vec![0.5; self.dims()],
        }
    }

    /// The start point's coordinates (the default; a colour's base).
    pub fn default_unit(&self) -> Vec<f64> {
        match &self.kind {
            Kind::Number { default, .. } => self.unit_of(&Value::Num(*default)),
            Kind::Colour { .. } => vec![0.5; 3],
            Kind::Choice { options, default } => vec![(*default as f64 + 0.5) / options.len().max(1) as f64],
        }
    }

    /// Parses a value given as JSON (a number, `#hex`, an option name).
    pub fn parse_value(&self, j: &Json) -> Option<Value> {
        match &self.kind {
            Kind::Number { min, max, .. } => j.as_f64().filter(|v| v.is_finite()).map(|v| Value::Num(v.clamp(*min, *max))),
            Kind::Colour { .. } => j.as_str().and_then(colour::parse_hex).map(Value::Colour),
            Kind::Choice { options, .. } => j.as_str().and_then(|s| options.iter().position(|o| o == s)).map(|i| Value::Choice(i, options[i].clone())),
        }
    }

    /// This parameter's direction (per its own dimensions) for a chip, if it answers to it.
    pub fn chip(&self, chip: &str) -> Option<Vec<f64>> {
        if let Some(s) = self.chips.get(chip) {
            let mut d = vec![0.0; self.dims()];
            d[0] = s.signum();
            return Some(d);
        }
        let name = match &self.target {
            Target::Recipe(n) => n.to_ascii_lowercase(),
            _ => String::new(),
        };
        let sizey = matches!(self.target, Target::Scale(_)) || SIZE_WORDS.iter().any(|w| name.contains(w));
        let detaily = DETAIL_WORDS.iter().any(|w| name.contains(w));
        let bright = matches!(self.target, Target::KeyIntensity | Target::AmbientIntensity | Target::Exposure);
        let one = |s: f64| Some(vec![s]);
        match (&self.kind, chip) {
            (Kind::Number { .. }, "darker") if bright => one(-1.0),
            (Kind::Number { .. }, "brighter") if bright => one(1.0),
            (Kind::Number { .. }, "bigger") if sizey => one(1.0),
            (Kind::Number { .. }, "smaller") if sizey => one(-1.0),
            (Kind::Number { .. }, "more detail") if detaily => one(1.0),
            (Kind::Number { .. }, "simpler") if detaily => one(-1.0),
            (Kind::Number { .. }, "foggier") if self.target == Target::FogDensity => one(1.0),
            (Kind::Number { .. }, "clearer") if self.target == Target::FogDensity => one(-1.0),
            (Kind::Colour { base, .. }, _) => {
                let warm = colour::hue_delta(base[2], 60.0).signum();
                let cool = colour::hue_delta(base[2], 250.0).signum();
                match chip {
                    "darker" => Some(vec![-1.0, 0.0, 0.0]),
                    "brighter" => Some(vec![1.0, 0.0, 0.0]),
                    "bolder" => Some(vec![0.0, 1.0, 0.0]),
                    "subtler" => Some(vec![0.0, -1.0, 0.0]),
                    "warmer" => Some(vec![0.0, 0.0, if warm == 0.0 { 1.0 } else { warm }]),
                    "cooler" => Some(vec![0.0, 0.0, if cool == 0.0 { -1.0 } else { cool }]),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub fn to_json(&self) -> Json {
        let mut j = Json::obj([("name", Json::from(self.name.as_str())), ("target", Json::from(self.target.name()))]);
        match &self.kind {
            Kind::Number { min, max, step, default } => {
                j.set("kind", Json::from("number"));
                j.set("min", r4(*min));
                j.set("max", r4(*max));
                if let Some(s) = step {
                    j.set("step", r4(*s));
                }
                j.set("default", r4(*default));
            }
            Kind::Colour { base, spread } => {
                j.set("kind", Json::from("colour"));
                j.set("base", Json::from(colour::hex(colour::oklch_to_srgb(*base))));
                // Full precision: a parsed space maps points to exactly the same values.
                j.set("base_oklch", Json::arr(base.map(Json::from)));
                j.set("spread", Json::obj([("l", r4(spread[0])), ("c", r4(spread[1])), ("h", r4(spread[2]))]));
            }
            Kind::Choice { options, default } => {
                j.set("kind", Json::from("choice"));
                j.set("options", Json::arr(options.iter().map(|o| Json::from(o.as_str()))));
                j.set("default", Json::from(options.get(*default).map_or("", String::as_str)));
            }
        }
        if let Some(a) = &self.apply {
            j.set("apply", a.to_json());
        }
        if !self.chips.is_empty() {
            j.set("chips", Json::obj(self.chips.iter().map(|(k, v)| (k.clone(), Json::from(*v)))));
        }
        j
    }

    /// Parses one parameter (`at`: its JSON path, for errors).
    pub fn parse(j: &Json, at: &str) -> Result<Param> {
        let bad = |code: &'static str, m: String, f: &str| TasteError::new(code, m).at(format!("{at}.{f}"));
        let obj = j.as_object().ok_or_else(|| TasteError::new("bad_param", "a parameter is an object {name, target, kind, ...}").at(at))?;
        const KEYS: [&str; 12] = ["name", "target", "kind", "min", "max", "step", "default", "base", "base_oklch", "spread", "options", "apply"];
        for (k, _) in obj {
            if !KEYS.contains(&k.as_str()) && k != "chips" {
                return Err(bad("unknown_key", format!("unknown key '{k}' (a parameter has {})", KEYS.join(", ")), k));
            }
        }
        let name = j.get("name").and_then(Json::as_str).filter(|n| ident(n)).ok_or_else(|| bad("bad_param", "name: letters, digits and _ (at most 48)".into(), "name"))?.to_owned();
        let target_s = j.get("target").and_then(Json::as_str).ok_or_else(|| bad("bad_target", format!("target is required: one of {TARGETS}"), "target"))?;
        let target = Target::parse(target_s).ok_or_else(|| bad("bad_target", format!("unknown target '{target_s}': one of {TARGETS}"), "target"))?;
        let kind_s = j.get("kind").and_then(Json::as_str).unwrap_or(if target.is_colour() {
            "colour"
        } else if target == Target::Preset {
            "choice"
        } else {
            "number"
        });
        let num = |k: &str| j.get(k).and_then(Json::as_f64).filter(|v| v.is_finite());
        let kind = match kind_s {
            "number" => {
                if target.is_colour() || target == Target::Preset {
                    return Err(bad("bad_kind", format!("{} takes a {} parameter", target.name(), if target == Target::Preset { "choice" } else { "colour" }), "kind"));
                }
                let (min, max) = match (num("min"), num("max")) {
                    (Some(a), Some(b)) if a < b => (a, b),
                    _ => return Err(bad("bad_range", "a number needs min < max".into(), "min")),
                };
                let step = num("step").filter(|s| *s > 0.0);
                let default = num("default").unwrap_or((min + max) * 0.5).clamp(min, max);
                Kind::Number { min, max, step, default }
            }
            "colour" | "color" => {
                if !target.is_colour() {
                    return Err(bad("bad_kind", format!("{} is not a colour target", target.name()), "kind"));
                }
                let base = match (j.get("base").and_then(Json::as_str), j.get("base_oklch").and_then(Json::as_array)) {
                    (_, Some(a)) if a.len() == 3 && a.iter().all(|v| v.as_f64().is_some()) => [a[0].as_f64().unwrap_or(0.5), a[1].as_f64().unwrap_or(0.0), a[2].as_f64().unwrap_or(0.0)],
                    (Some(h), _) => colour::srgb_to_oklch(colour::parse_hex(h).ok_or_else(|| bad("bad_colour", format!("base '{h}' is not #rrggbb"), "base"))?),
                    _ => return Err(bad("bad_colour", "a colour needs a base (#rrggbb)".into(), "base")),
                };
                let sp = j.get("spread");
                let s = |k: &str, d: f64| sp.and_then(|s| s.get(k)).and_then(Json::as_f64).filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(d);
                let spread = [s("l", 0.12).min(0.5), s("c", 0.05).min(0.3), s("h", 40.0).min(180.0)];
                Kind::Colour { base, spread }
            }
            "choice" => {
                let options: Vec<String> = j
                    .get("options")
                    .and_then(Json::as_array)
                    .ok_or_else(|| bad("bad_options", "a choice needs options: [names]".into(), "options"))?
                    .iter()
                    .map(|o| o.as_str().map(str::to_owned))
                    .collect::<Option<_>>()
                    .ok_or_else(|| bad("bad_options", "options are names".into(), "options"))?;
                if !(2..=MAX_OPTIONS).contains(&options.len()) || options.iter().any(|o| o.is_empty() || o.len() > 40) {
                    return Err(bad("bad_options", format!("a choice has 2..={MAX_OPTIONS} options of 1-40 characters"), "options"));
                }
                let default = j.get("default").and_then(Json::as_str).and_then(|d| options.iter().position(|o| o == d)).unwrap_or(0);
                Kind::Choice { options, default }
            }
            other => return Err(bad("bad_kind", format!("kind '{other}': number, colour or choice"), "kind")),
        };
        let apply = match j.get("apply") {
            None | Some(Json::Null) => None,
            Some(a) => {
                let file = a.get("file").and_then(Json::as_str).filter(|f| !f.is_empty() && !f.starts_with('/') && !f.contains("..") && !f.contains('\\')).ok_or_else(|| bad("bad_apply", "apply.file: a project file relative to the assets folder (style.toml, ui/theme.json)".into(), "apply.file"))?;
                if is_taste_file(file) {
                    return Err(bad("reserved", "apply.file may not be taste state (taste/: galleries, picks, boards): a gallery applies to the game's files".into(), "apply.file"));
                }
                let at = match (a.get("toml").and_then(Json::as_str), a.get("json").and_then(Json::as_str)) {
                    (Some(k), None) if k.split('.').all(ident) => ApplyAt::Toml(k.to_owned()),
                    (None, Some(k)) if !k.is_empty() && k.len() <= 200 => ApplyAt::Json(k.to_owned()),
                    _ => return Err(bad("bad_apply", "apply needs one of toml: \"table.key\" or json: \"path\"".into(), "apply")),
                };
                if matches!(at, ApplyAt::Toml(_)) && !file.ends_with(".toml") || matches!(at, ApplyAt::Json(_)) && !file.ends_with(".json") {
                    return Err(bad("bad_apply", "toml keys go in .toml files, json paths in .json files".into(), "apply"));
                }
                Some(Apply { file: file.to_owned(), at })
            }
        };
        let mut chips = BTreeMap::new();
        if let Some(c) = j.get("chips") {
            for (k, v) in c.as_object().ok_or_else(|| bad("bad_chips", "chips: {\"darker\": -1, ...}".into(), "chips"))? {
                if !CHIPS.contains(&k.as_str()) {
                    return Err(bad("bad_chips", format!("unknown chip '{k}': {}", CHIPS.join(", ")), "chips"));
                }
                chips.insert(k.clone(), v.as_f64().filter(|x| *x != 0.0 && x.is_finite()).ok_or_else(|| bad("bad_chips", "chip values are +1 or -1".into(), "chips"))?.signum());
            }
        }
        Ok(Param { name, target, kind, apply, chips })
    }
}

/// A parameter space.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Space {
    pub params: Vec<Param>,
}

impl Space {
    pub fn parse(j: &Json) -> Result<Space> {
        let arr = j.as_array().ok_or_else(|| TasteError::new("bad_space", "space is an array of parameters").at("space"))?;
        if arr.is_empty() || arr.len() > MAX_PARAMS {
            return Err(TasteError::new("bad_space", format!("a space has 1..={MAX_PARAMS} parameters")).at("space"));
        }
        let mut params: Vec<Param> = Vec::new();
        for (i, p) in arr.iter().enumerate() {
            let p = Param::parse(p, &format!("space[{i}]"))?;
            if params.iter().any(|q| q.name == p.name) {
                return Err(TasteError::new("duplicate_param", format!("two parameters are named '{}'", p.name)).at(format!("space[{i}].name")));
            }
            if params.iter().any(|q| q.target == p.target) {
                return Err(TasteError::new("duplicate_target", format!("two parameters drive {}", p.target.name())).at(format!("space[{i}].target")));
            }
            params.push(p);
        }
        let s = Space { params };
        if s.dims() > MAX_DIMS {
            return Err(TasteError::new("bad_space", format!("{} dimensions; at most {MAX_DIMS} (a colour is three)", s.dims())).at("space"));
        }
        Ok(s)
    }

    pub fn to_json(&self) -> Json {
        Json::arr(self.params.iter().map(Param::to_json))
    }

    pub fn dims(&self) -> usize {
        self.params.iter().map(Param::dims).sum()
    }

    /// The values at a unit point.
    pub fn values(&self, point: &[f64]) -> Vec<Value> {
        let mut at = 0;
        self.params
            .iter()
            .map(|p| {
                let d = p.dims();
                let v = p.value_at(point.get(at..at + d).unwrap_or(&[]));
                at += d;
                v
            })
            .collect()
    }

    /// The unit point of values given by parameter name (missing ones at their defaults).
    pub fn point_of(&self, values: &BTreeMap<String, Value>) -> Vec<f64> {
        self.params.iter().flat_map(|p| values.get(&p.name).map_or_else(|| p.default_unit(), |v| p.unit_of(v))).collect()
    }

    pub fn default_point(&self) -> Vec<f64> {
        self.params.iter().flat_map(Param::default_unit).collect()
    }

    /// Values as `{name: value}`.
    pub fn values_json(&self, values: &[Value]) -> Json {
        Json::obj(self.params.iter().zip(values).map(|(p, v)| (p.name.clone(), v.to_json())))
    }

    /// Parses `{name: value}` (unknown names are refused).
    pub fn parse_values(&self, j: &Json) -> Result<BTreeMap<String, Value>> {
        let mut out = BTreeMap::new();
        for (k, v) in j.as_object().ok_or_else(|| TasteError::new("bad_values", "values are {name: value}"))? {
            let p = self.params.iter().find(|p| &p.name == k).ok_or_else(|| TasteError::new("unknown_param", format!("no parameter '{k}' in this space")).at(k.as_str()))?;
            out.insert(k.clone(), p.parse_value(v).ok_or_else(|| TasteError::new("bad_value", format!("'{k}' takes {}", kind_word(&p.kind))).at(k.as_str()))?);
        }
        Ok(out)
    }

    /// The direction (in the unit cube, normalised) the given chips ask for; `None` when no
    /// parameter answers to any of them.
    pub fn chip_direction(&self, chips: &[String]) -> Option<Vec<f64>> {
        let mut d = vec![0.0; self.dims()];
        let mut any = false;
        let mut at = 0;
        for p in &self.params {
            for c in chips {
                if let Some(v) = p.chip(c) {
                    for (k, x) in v.iter().enumerate() {
                        d[at + k] += x;
                    }
                    any = true;
                }
            }
            at += p.dims();
        }
        let n = d.iter().map(|x| x * x).sum::<f64>().sqrt();
        (any && n > 1e-9).then(|| d.iter().map(|x| x / n).collect())
    }

    /// Whether two value sets look the same on a sheet: numbers within 1% of their range,
    /// colours within [`MIN_SHOWN_DE`] Oklab of each other as displayed (gamut-mapped and
    /// quantised), the same option.
    pub fn same_shown(&self, a: &[Value], b: &[Value]) -> bool {
        self.params.iter().zip(a.iter().zip(b)).all(|(p, (x, y))| match (&p.kind, x, y) {
            (Kind::Number { min, max, .. }, Value::Num(u), Value::Num(v)) => (u - v).abs() <= 0.01 * (max - min),
            (_, Value::Colour(u), Value::Colour(v)) => colour::delta_e(*u, *v) < p.min_shown_de(),
            (_, Value::Choice(i, _), Value::Choice(j, _)) => i == j,
            _ => false,
        })
    }

    /// The chips some parameter answers to, in [`CHIPS`] order.
    pub fn chips(&self) -> Vec<&'static str> {
        CHIPS.iter().copied().filter(|c| self.params.iter().any(|p| p.chip(c).is_some())).collect()
    }
}

/// `u` in 0..1 to `base - s ..= base + s` cut to `lo..=hi`, piecewise linear with 0.5 at `base`.
fn around(u: f64, base: f64, s: f64, lo: f64, hi: f64) -> f64 {
    let (a, b) = ((base - s).max(lo), (base + s).min(hi));
    if u <= 0.5 {
        a + (base - a) * u * 2.0
    } else {
        base + (b - base) * (u - 0.5) * 2.0
    }
}

/// The inverse of [`around`].
fn around_inv(v: f64, base: f64, s: f64, lo: f64, hi: f64) -> f64 {
    let (a, b) = ((base - s).max(lo), (base + s).min(hi));
    if v <= base {
        if base - a > 1e-12 {
            (0.5 * (v - a) / (base - a)).clamp(0.0, 0.5)
        } else {
            0.5
        }
    } else if b - base > 1e-12 {
        (0.5 + 0.5 * (v - base) / (b - base)).clamp(0.5, 1.0)
    } else {
        0.5
    }
}

fn kind_word(k: &Kind) -> &'static str {
    match k {
        Kind::Number { .. } => "a number",
        Kind::Colour { .. } => "a colour #rrggbb",
        Kind::Choice { .. } => "one of its options",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn space() -> Space {
        Space::parse(
            &Json::parse(
                r##"[{"name":"moon","target":"light.key_color","base":"#d0d9f0","spread":{"l":0.1,"c":0.04,"h":40},"apply":{"file":"style.toml","toml":"stage.game.key_color"}},
                    {"name":"fog","target":"fog.density","min":0,"max":0.04,"default":0.01},
                    {"name":"h","target":"scale.y","min":0.8,"max":1.4,"default":1,"step":0.05},
                    {"name":"look","target":"preset","options":["moonlit","storybook","muted"],"default":"muted"}]"##,
            )
            .unwrap(),
        )
        .unwrap()
    }

    /// Taste review 2: an agent's gallery applied into another gallery's file and changed the
    /// owner's pick. Taste state is never an apply target, in any spelling.
    #[test]
    fn galleries_never_apply_into_taste_state() {
        for f in ["taste/galleries/vic.json", "Taste/galleries/vic.json", "./taste/onboarding.json", "TASTE/suggestions.json"] {
            let j = Json::parse(&format!(r#"[{{"name": "n", "target": "light.key_intensity", "min": 0, "max": 8, "apply": {{"file": "{f}", "json": "rounds"}}}}]"#)).unwrap();
            assert_eq!(Space::parse(&j).unwrap_err().code, "reserved", "{f}");
        }
        let ok = Json::parse(r#"[{"name": "n", "target": "light.key_intensity", "min": 0, "max": 8, "apply": {"file": "ui/theme.json", "json": "colors.x"}}]"#).unwrap();
        assert!(Space::parse(&ok).is_ok());
    }

    #[test]
    fn spaces_map_both_ways() {
        let s = space();
        assert_eq!(s.dims(), 6);
        let p = s.default_point();
        let v = s.values(&p);
        assert_eq!(v[0].to_json().as_str(), Some("#d0d9f0"));
        assert!((v[1].as_num().unwrap() - 0.01).abs() < 1e-9);
        assert_eq!(v[3], Value::Choice(2, "muted".into()));
        // Steps round.
        let at = s.values(&[0.5, 0.5, 0.5, 0.5, 0.31, 0.0]);
        assert!((at[2].as_num().unwrap() - 0.99).abs() > 1e-9 && (dmath::round_f64(at[2].as_num().unwrap() * 100.0) as i64) % 5 == 0);
        // Values back to points.
        let mut m = BTreeMap::new();
        m.insert("fog".to_owned(), Value::Num(0.02));
        let q = s.point_of(&m);
        assert!((q[3] - 0.5).abs() < 1e-9);
        assert_eq!(s.values_json(&v).get("look").and_then(Json::as_str), Some("muted"));
    }

    #[test]
    fn light_colours_never_clamp_to_the_same_value() {
        // A pale base with a wide lightness spread: the upper half still spans distinct colours
        // (a probe from the live check: half a round came out pure white).
        let p = Param::parse(&Json::parse(r##"{"name": "c", "target": "ui.c", "base": "#ffd27a", "spread": {"l": 0.25, "c": 0.12, "h": 50}}"##).unwrap(), "p").unwrap();
        let hexes: Vec<String> = [0.55, 0.7, 0.85, 1.0].iter().map(|u| p.value_at(&[*u, 0.5, 0.5]).to_json().as_str().unwrap_or("").to_owned()).collect();
        let mut uniq = hexes.clone();
        uniq.dedup();
        assert_eq!(uniq.len(), 4, "{hexes:?}");
        assert_eq!(p.value_at(&[0.5, 0.5, 0.5]).to_json().as_str(), Some("#ffd27a"), "0.5 is the base");
        let back = p.unit_of(&p.value_at(&[0.8, 0.3, 0.6]));
        assert!((back[0] - 0.8).abs() < 0.02 && (back[1] - 0.3).abs() < 0.02, "{back:?}");
    }

    #[test]
    fn chips_follow_the_parameters() {
        let s = space();
        assert_eq!(s.chips(), vec!["darker", "brighter", "bigger", "smaller", "warmer", "cooler", "bolder", "subtler", "foggier", "clearer"]);
        let d = s.chip_direction(&["darker".into()]).unwrap();
        assert!(d[0] < -0.99, "darker lowers the moon's lightness only: {d:?}");
        assert!(s.chip_direction(&["simpler".into()]).is_none());
        let both = s.chip_direction(&["bigger".into(), "foggier".into()]).unwrap();
        assert!(both[3] > 0.7 && both[4] > 0.7);
    }

    #[test]
    fn bad_spaces_say_what_to_change() {
        let e = Space::parse(&Json::parse(r#"[{"name":"x","target":"light.colour"}]"#).unwrap()).unwrap_err();
        assert_eq!((e.code, e.field.as_deref()), ("bad_target", Some("space[0].target")));
        let e = Space::parse(&Json::parse(r#"[{"name":"x","target":"fog.density","min":1,"max":0}]"#).unwrap()).unwrap_err();
        assert_eq!(e.code, "bad_range");
        let e = Space::parse(&Json::parse(r##"[{"name":"x","target":"fog.color","base":"#fff","apply":{"file":"../x.toml","toml":"a.b"}}]"##).unwrap()).unwrap_err();
        assert_eq!(e.code, "bad_apply");
        let many: Vec<String> = (0..5).map(|i| format!(r##"{{"name":"c{i}","target":"ui.c{i}","base":"#808080"}}"##)).collect();
        assert_eq!(Space::parse(&Json::parse(&format!("[{}]", many.join(","))).unwrap()).unwrap_err().code, "bad_space");
    }
}

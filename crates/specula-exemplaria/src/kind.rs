//! The kind format (`<name>.kind.json`): a plan with its numbers set, each with where it came
//! from, and how far each may vary (docs/design/EXEMPLARIA.md, A.1).
//!
//! ```text
//! { "kind": 1, "name": "grey_wolf", "plan": "quadruped", "about": "a grey wolf, adult",
//!   "aliases": ["wolf"], "base": "dog",
//!   "size":    { "withers_height": { "mean": 0.629, "sd": 0.03, "range": [0.579, 0.672], "unit": "m", "source": "..." } },
//!   "numbers": { "tail": { "share": 0.40, "own": "no measured tail length (research C1, Not found)" } },
//!   "vary":    { "build": { "spread": 0.06, "own": "..." } },
//!   "roles":   { "coat": "#776a5a" },
//!   "motion":  { "walk": { "pattern": "lateral_walk", "froude": 0.25, "source": "..." }, "idle": {} },
//!   "levels":  { "near": 6000, "far": 150 },
//!   "foot": "digitigrade" }
//! ```
//!
//! **Every number has a `source` or an `own`**, never both and never neither: a `source` names
//! where it was measured (the primary source, with the research file it was found in), an `own`
//! says why the engine set it. Any other top-level field with a string value is one of the plan's
//! options (`foot`), checked against the plan when the kind is resolved ([`crate::resolve`]).
//!
//! **Parsing is bounded and strict**: a text over [`MAX_KIND_BYTES`] is refused before it is read,
//! nesting past the JSON reader's depth is refused by it, a control character anywhere (a key or
//! a string) is refused with its path, every list and table has a ceiling, a field this format
//! does not know is refused with the ones it does, a key given twice is refused.

use crate::error::Error;
use crate::words::valid_name;
use specula_core::Json;
use std::collections::BTreeMap;

/// The format this crate reads and writes.
pub const FORMAT: u32 = 1;
/// The largest kind file read, bytes.
pub const MAX_KIND_BYTES: usize = 64 * 1024;
/// The longest text (an `about`, a `source`, an `own`), bytes.
pub const MAX_TEXT: usize = 400;
/// The most entries in any one table (`numbers`, `vary`, `motion`, `levels`) or list (`aliases`).
pub const MAX_ENTRIES: usize = 64;
/// The most roles a kind uses: each is a draw call (ASSET_STUDIO.md: six materials a model).
pub const MAX_ROLES: usize = 4;
/// The most triangles a level may name.
pub const MAX_LEVEL_TRIANGLES: u32 = 200_000;
/// The levels a kind may name.
pub const LEVELS: &[&str] = &["near", "mid", "crowd", "far"];
/// A kind file's own top-level fields (anything else with a string value is a plan's option).
pub const FIELDS: &[&str] = &["kind", "name", "plan", "about", "aliases", "base", "licence", "from", "size", "numbers", "vary", "roles", "motion", "levels"];
/// The engine's licence, which every entry of the library carries.
pub const LICENCE: &str = "MIT OR Apache-2.0";

/// Where a number came from.
#[derive(Clone, Debug, PartialEq)]
pub enum Why {
    /// Measured: the primary source, with the research file it was found in.
    Source(String),
    /// The engine's own, with the reason.
    Own(String),
}

impl Why {
    pub fn text(&self) -> &str {
        match self {
            Why::Source(s) | Why::Own(s) => s,
        }
    }
    pub fn is_own(&self) -> bool {
        matches!(self, Why::Own(_))
    }
    fn push(&self, o: &mut Vec<(&'static str, Json)>) {
        match self {
            Why::Source(s) => o.push(("source", Json::from(s.as_str()))),
            Why::Own(s) => o.push(("own", Json::from(s.as_str()))),
        }
    }
}

/// The size row: the one number everything else follows.
#[derive(Clone, Debug, PartialEq)]
pub struct Size {
    pub name: String,
    pub mean: f64,
    /// The spread (one standard deviation), when the source prints one.
    pub sd: Option<f64>,
    /// The measured range, when the source prints one.
    pub range: Option<[f64; 2]>,
    pub unit: String,
    pub why: Why,
}

/// One number of the plan's table, set by the kind.
#[derive(Clone, Debug, PartialEq)]
pub struct Number {
    /// A share of the size at the mean size.
    pub share: Option<f64>,
    /// Or its value at the mean size, in the plan's unit.
    pub mean: Option<f64>,
    /// How it grows with size (`value ∝ size^exponent`); the plan's when left out.
    pub exponent: Option<f64>,
    /// Its own spread about what size and traits give, as a fraction.
    pub spread: Option<f64>,
    /// Its measured range (absolute, in the plan's unit): a drawn value never leaves it.
    pub range: Option<[f64; 2]>,
    pub why: Why,
}

/// How far a trait of the plan varies: a residual, normal, of this spread (a fraction).
#[derive(Clone, Debug, PartialEq)]
pub struct Vary {
    pub spread: f64,
    pub why: Why,
}

/// One clip of the plan's motions, as the kind sets it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Motion {
    /// A footfall pattern by name (`lateral_walk`, `trot`: [`crate::gaits`]).
    pub pattern: Option<String>,
    /// A preset of the plan's (`bite`, `topple`).
    pub preset: Option<String>,
    /// Speed as a Froude number (v² / g·h, h the hip height).
    pub froude: Option<f64>,
    /// Where its number came from (required when it sets one).
    pub why: Option<Why>,
}

/// A kind: a plan with its numbers set (parsed, before its base and its plan are applied).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Kind {
    pub name: String,
    pub plan: String,
    pub about: String,
    pub aliases: Vec<String>,
    /// Another kind of the same plan this one changes (one level only).
    pub base: Option<String>,
    pub licence: String,
    /// Where a project's copy came from (`exemplaria/box_creature/box_creature@<hash>`).
    pub from: Option<String>,
    pub size: Option<Size>,
    pub numbers: BTreeMap<String, Number>,
    pub vary: BTreeMap<String, Vary>,
    /// Role → hint colour (`#rrggbb`).
    pub roles: BTreeMap<String, String>,
    pub motion: BTreeMap<String, Motion>,
    pub levels: BTreeMap<String, u32>,
    /// The plan's options (`foot: digitigrade`).
    pub options: BTreeMap<String, String>,
}

fn err(code: &'static str, path: &str, message: impl Into<String>) -> Error {
    Error::new(code, path, message)
}

fn at(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

/// Refuses a control character in any key or string, and a key given twice in an object.
fn check_tree(v: &Json, path: &str) -> Result<(), Error> {
    let bad = |s: &str| s.chars().any(|c| c.is_control());
    match v {
        Json::Str(s) => {
            if bad(s) {
                return Err(err("control_character", path, "a control character in a string (a kind's text is plain: no tabs, newlines or escapes below 0x20)"));
            }
        }
        Json::Arr(items) => {
            for (i, x) in items.iter().enumerate() {
                check_tree(x, &format!("{path}[{i}]"))?;
            }
        }
        Json::Obj(pairs) => {
            for (i, (k, x)) in pairs.iter().enumerate() {
                if bad(k) {
                    return Err(err("control_character", path, "a control character in a key"));
                }
                if pairs[..i].iter().any(|(k2, _)| k2 == k) {
                    return Err(err("duplicate_key", &at(path, k), "this key is given twice"));
                }
                check_tree(x, &at(path, k))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn object<'a>(v: &'a Json, path: &str, what: &str) -> Result<&'a [(String, Json)], Error> {
    v.as_object().ok_or_else(|| err("wrong_type", path, format!("must be an object: {what}")))
}

fn table<'a>(v: &'a Json, path: &str, what: &str) -> Result<&'a [(String, Json)], Error> {
    let o = object(v, path, what)?;
    if o.len() > MAX_ENTRIES {
        return Err(err("too_many", path, format!("{} entries; at most {MAX_ENTRIES}", o.len())));
    }
    for (k, _) in o {
        if !valid_name(k) {
            return Err(err("bad_name", &at(path, k), "a name is a lowercase letter, then lowercase letters, digits and _ (at most 48)"));
        }
    }
    Ok(o)
}

fn text(v: &Json, path: &str) -> Result<String, Error> {
    let s = v.as_str().ok_or_else(|| err("wrong_type", path, "must be a string"))?;
    if s.len() > MAX_TEXT {
        return Err(err("too_large", path, format!("{} bytes; at most {MAX_TEXT}", s.len())));
    }
    Ok(s.to_string())
}

fn name(v: &Json, path: &str) -> Result<String, Error> {
    let s = v.as_str().ok_or_else(|| err("wrong_type", path, "must be a name (a string)"))?;
    if !valid_name(s) {
        return Err(err("bad_name", path, "a name is a lowercase letter, then lowercase letters, digits and _ (at most 48)"));
    }
    Ok(s.to_string())
}

fn number(v: &Json, path: &str, lo: f64, hi: f64) -> Result<f64, Error> {
    let x = v.as_f64().ok_or_else(|| err("wrong_type", path, "must be a number"))?;
    if !(x.is_finite() && x >= lo && x <= hi) {
        return Err(err("out_of_range", path, format!("{x} is outside {lo}..={hi}")));
    }
    Ok(x)
}

fn range(v: &Json, path: &str) -> Result<[f64; 2], Error> {
    let a = v.as_array().filter(|a| a.len() == 2).ok_or_else(|| err("wrong_type", path, "must be [low, high]"))?;
    let lo = number(&a[0], &format!("{path}[0]"), 0.0, 1e6)?;
    let hi = number(&a[1], &format!("{path}[1]"), 0.0, 1e6)?;
    if lo > hi {
        return Err(err("out_of_range", path, format!("low {lo} is above high {hi}")));
    }
    Ok([lo, hi])
}

/// `source` or `own`, exactly one, from an object's fields; `required`: one must be there.
fn why(o: &[(String, Json)], path: &str, required: bool) -> Result<Option<Why>, Error> {
    let source = o.iter().find(|(k, _)| k == "source").map(|(_, v)| text(v, &at(path, "source"))).transpose()?;
    let own = o.iter().find(|(k, _)| k == "own").map(|(_, v)| text(v, &at(path, "own"))).transpose()?;
    let blank = |s: &Option<String>| s.as_deref().is_some_and(|s| s.trim().is_empty());
    if blank(&source) || blank(&own) {
        return Err(err("no_source", path, "a source or an own reason must say something"));
    }
    match (source, own) {
        (Some(_), Some(_)) => Err(err("two_sources", path, "a number has a source or an own reason, not both: say which")),
        (Some(s), None) => Ok(Some(Why::Source(s))),
        (None, Some(o)) => Ok(Some(Why::Own(o))),
        (None, None) if required => Err(err(
            "no_source",
            path,
            "every number needs \"source\" (where it was measured: the primary source, and the research file it was found in) or \"own\" (why the engine set it)",
        )),
        (None, None) => Ok(None),
    }
}

/// Refuses a field outside `allowed`.
fn only(o: &[(String, Json)], path: &str, allowed: &[&str]) -> Result<(), Error> {
    for (k, _) in o {
        if !allowed.contains(&k.as_str()) {
            return Err(err("unknown_field", &at(path, k), format!("'{k}' is not a field here; it takes {}", allowed.join(", "))));
        }
    }
    Ok(())
}

fn get<'a>(o: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    o.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

impl Kind {
    /// Reads a kind file (see the module's notes for what is refused).
    pub fn parse(text_in: &str) -> Result<Kind, Error> {
        if text_in.len() > MAX_KIND_BYTES {
            return Err(err("too_large", "", format!("a kind file is {} bytes; at most {MAX_KIND_BYTES}", text_in.len())));
        }
        let json = Json::parse(text_in).map_err(|e| err("json", "", format!("{} (byte {})", e.message, e.offset)))?;
        check_tree(&json, "")?;
        let top = object(&json, "", "a kind file")?;
        let mut kind = Kind::default();
        match get(top, "kind") {
            Some(v) if v.as_f64() == Some(f64::from(FORMAT)) => {}
            Some(_) => return Err(err("out_of_range", "kind", format!("this engine reads kind format {FORMAT}"))),
            None => return Err(err("missing_field", "kind", format!("a kind file starts with \"kind\": {FORMAT}"))),
        }
        kind.name = name(get(top, "name").ok_or_else(|| err("missing_field", "name", "a kind needs its name"))?, "name")?;
        kind.plan = name(get(top, "plan").ok_or_else(|| err("missing_field", "plan", "a kind names its plan (quadruped, tree, machine...)"))?, "plan")?;
        kind.about = text(get(top, "about").ok_or_else(|| err("missing_field", "about", "a kind says what it is in one line"))?, "about")?;
        kind.licence = match get(top, "licence") {
            Some(v) => text(v, "licence")?,
            None => LICENCE.to_string(),
        };
        for (k, v) in top {
            let p = k.as_str();
            match p {
                "kind" | "name" | "plan" | "about" | "licence" => {}
                "aliases" => {
                    let a = v.as_array().ok_or_else(|| err("wrong_type", p, "must be a list of names"))?;
                    if a.len() > MAX_ENTRIES {
                        return Err(err("too_many", p, format!("{} aliases; at most {MAX_ENTRIES}", a.len())));
                    }
                    kind.aliases = a.iter().enumerate().map(|(i, x)| name(x, &format!("aliases[{i}]"))).collect::<Result<_, _>>()?;
                }
                "base" => kind.base = Some(name(v, p)?),
                "from" => kind.from = Some(text(v, p)?),
                "size" => {
                    let o = table(v, p, "{NAME: {mean, sd, range, unit, source | own}}")?;
                    if o.len() != 1 {
                        return Err(err("wrong_type", p, "the size is one number: {NAME: {...}}"));
                    }
                    let (n, row) = &o[0];
                    let rp = at(p, n);
                    let r = object(row, &rp, "{mean, sd, range, unit, source | own}")?;
                    only(r, &rp, &["mean", "sd", "range", "unit", "source", "own"])?;
                    let mean = number(get(r, "mean").ok_or_else(|| err("missing_field", &at(&rp, "mean"), "the size's mean"))?, &at(&rp, "mean"), 1e-6, 1e6)?;
                    let sd = get(r, "sd").map(|x| number(x, &at(&rp, "sd"), 0.0, mean)).transpose()?;
                    let rg = get(r, "range").map(|x| range(x, &at(&rp, "range"))).transpose()?;
                    if let Some([lo, hi]) = rg {
                        if mean < lo || mean > hi {
                            return Err(err("out_of_range", &at(&rp, "range"), format!("the mean {mean} lies outside its own range {lo}..{hi}")));
                        }
                    }
                    let unit = get(r, "unit").map(|x| text(x, &at(&rp, "unit"))).transpose()?.ok_or_else(|| err("missing_field", &at(&rp, "unit"), "the size's unit (m)"))?;
                    let why = why(r, &rp, true)?.expect("required");
                    kind.size = Some(Size { name: n.clone(), mean, sd, range: rg, unit, why });
                }
                "numbers" => {
                    for (n, row) in table(v, p, "{NAME: {share | mean, exponent, spread, range, source | own}}")? {
                        let rp = at(p, n);
                        let r = object(row, &rp, "{share | mean, exponent, spread, range, source | own}")?;
                        only(r, &rp, &["share", "mean", "exponent", "spread", "range", "source", "own"])?;
                        let share = get(r, "share").map(|x| number(x, &at(&rp, "share"), 1e-6, 100.0)).transpose()?;
                        let mean = get(r, "mean").map(|x| number(x, &at(&rp, "mean"), -1e6, 1e6)).transpose()?;
                        if share.is_some() == mean.is_some() {
                            return Err(err("missing_field", &rp, "a number is a \"share\" of the size or its \"mean\" value at the mean size: one of them"));
                        }
                        let exponent = get(r, "exponent").map(|x| number(x, &at(&rp, "exponent"), -3.0, 3.0)).transpose()?;
                        let spread = get(r, "spread").map(|x| number(x, &at(&rp, "spread"), 0.0, 0.5)).transpose()?;
                        let rg = get(r, "range").map(|x| range(x, &at(&rp, "range"))).transpose()?;
                        let why = why(r, &rp, true)?.expect("required");
                        kind.numbers.insert(n.clone(), Number { share, mean, exponent, spread, range: rg, why });
                    }
                }
                "vary" => {
                    for (n, row) in table(v, p, "{TRAIT: {spread, source | own}}")? {
                        let rp = at(p, n);
                        let r = object(row, &rp, "{spread, source | own}")?;
                        only(r, &rp, &["spread", "source", "own"])?;
                        let spread = number(get(r, "spread").ok_or_else(|| err("missing_field", &at(&rp, "spread"), "how far the trait varies, a fraction"))?, &at(&rp, "spread"), 0.0, 0.5)?;
                        kind.vary.insert(n.clone(), Vary { spread, why: why(r, &rp, true)?.expect("required") });
                    }
                }
                "roles" => {
                    let o = table(v, p, "{ROLE: \"#rrggbb\"}")?;
                    if o.len() > MAX_ROLES {
                        return Err(err("too_many", p, format!("{} roles; at most {MAX_ROLES} (each is a draw call)", o.len())));
                    }
                    for (n, hint) in o {
                        let h = text(hint, &at(p, n))?;
                        let hex = h.strip_prefix('#').filter(|x| x.len() == 6 && x.bytes().all(|b| b.is_ascii_hexdigit()));
                        if hex.is_none() {
                            return Err(err("wrong_type", &at(p, n), "a role's hint colour is \"#rrggbb\""));
                        }
                        kind.roles.insert(n.clone(), h);
                    }
                }
                "motion" => {
                    for (n, row) in table(v, p, "{CLIP: {pattern, preset, froude, source | own}}")? {
                        let rp = at(p, n);
                        let r = object(row, &rp, "{pattern, preset, froude, source | own}")?;
                        only(r, &rp, &["pattern", "preset", "froude", "source", "own"])?;
                        let froude = get(r, "froude").map(|x| number(x, &at(&rp, "froude"), 0.01, 20.0)).transpose()?;
                        let m = Motion {
                            pattern: get(r, "pattern").map(|x| name(x, &at(&rp, "pattern"))).transpose()?,
                            preset: get(r, "preset").map(|x| name(x, &at(&rp, "preset"))).transpose()?,
                            froude,
                            why: why(r, &rp, froude.is_some())?,
                        };
                        kind.motion.insert(n.clone(), m);
                    }
                }
                "levels" => {
                    for (n, x) in table(v, p, "{near | mid | crowd | far: triangles}")? {
                        if !LEVELS.contains(&n.as_str()) {
                            return Err(err("unknown_field", &at(p, n), format!("levels are {}", LEVELS.join(", "))));
                        }
                        let t = number(x, &at(p, n), 1.0, f64::from(MAX_LEVEL_TRIANGLES))?;
                        if specula_core::dmath::fract_f64(t) != 0.0 {
                            return Err(err("wrong_type", &at(p, n), "a whole number of triangles"));
                        }
                        kind.levels.insert(n.clone(), t as u32);
                    }
                }
                _ => match v {
                    // A plan's option (checked against the plan when the kind is resolved).
                    Json::Str(_) if valid_name(p) => {
                        if kind.options.len() >= MAX_ENTRIES {
                            return Err(err("too_many", p, format!("at most {MAX_ENTRIES} options")));
                        }
                        kind.options.insert(p.to_string(), name(v, p)?);
                    }
                    _ => return Err(err("unknown_field", p, format!("'{p}' is not a field of a kind; it takes {}, and the plan's options (names, as strings)", FIELDS.join(", ")))),
                },
            }
        }
        Ok(kind)
    }

    /// The kind as a kind file, in a fixed order (what a project's copy is written as).
    pub fn to_json(&self) -> Json {
        let mut o: Vec<(&str, Json)> = vec![
            ("kind", Json::from(FORMAT)),
            ("name", Json::from(self.name.as_str())),
            ("plan", Json::from(self.plan.as_str())),
            ("about", Json::from(self.about.as_str())),
        ];
        if !self.aliases.is_empty() {
            o.push(("aliases", Json::arr(self.aliases.iter().map(|a| Json::from(a.as_str())))));
        }
        if let Some(b) = &self.base {
            o.push(("base", Json::from(b.as_str())));
        }
        o.push(("licence", Json::from(self.licence.as_str())));
        if let Some(f) = &self.from {
            o.push(("from", Json::from(f.as_str())));
        }
        let mut opts: Vec<(String, Json)> = self.options.iter().map(|(k, v)| (k.clone(), Json::from(v.as_str()))).collect();
        if let Some(s) = &self.size {
            let mut r = vec![("mean", Json::from(s.mean))];
            if let Some(sd) = s.sd {
                r.push(("sd", Json::from(sd)));
            }
            if let Some([lo, hi]) = s.range {
                r.push(("range", Json::arr([Json::from(lo), Json::from(hi)])));
            }
            r.push(("unit", Json::from(s.unit.as_str())));
            s.why.push(&mut r);
            o.push(("size", Json::obj([(s.name.as_str(), Json::obj(r))])));
        }
        if !self.numbers.is_empty() {
            o.push((
                "numbers",
                Json::obj(self.numbers.iter().map(|(k, n)| {
                    let mut r = Vec::new();
                    if let Some(s) = n.share {
                        r.push(("share", Json::from(s)));
                    }
                    if let Some(m) = n.mean {
                        r.push(("mean", Json::from(m)));
                    }
                    if let Some(e) = n.exponent {
                        r.push(("exponent", Json::from(e)));
                    }
                    if let Some(s) = n.spread {
                        r.push(("spread", Json::from(s)));
                    }
                    if let Some([lo, hi]) = n.range {
                        r.push(("range", Json::arr([Json::from(lo), Json::from(hi)])));
                    }
                    n.why.push(&mut r);
                    (k.as_str(), Json::obj(r))
                })),
            ));
        }
        if !self.vary.is_empty() {
            o.push((
                "vary",
                Json::obj(self.vary.iter().map(|(k, v)| {
                    let mut r = vec![("spread", Json::from(v.spread))];
                    v.why.push(&mut r);
                    (k.as_str(), Json::obj(r))
                })),
            ));
        }
        if !self.roles.is_empty() {
            o.push(("roles", Json::obj(self.roles.iter().map(|(k, h)| (k.as_str(), Json::from(h.as_str()))))));
        }
        if !self.motion.is_empty() {
            o.push((
                "motion",
                Json::obj(self.motion.iter().map(|(k, m)| {
                    let mut r = Vec::new();
                    if let Some(p) = &m.pattern {
                        r.push(("pattern", Json::from(p.as_str())));
                    }
                    if let Some(p) = &m.preset {
                        r.push(("preset", Json::from(p.as_str())));
                    }
                    if let Some(f) = m.froude {
                        r.push(("froude", Json::from(f)));
                    }
                    if let Some(w) = &m.why {
                        w.push(&mut r);
                    }
                    (k.as_str(), Json::obj(r))
                })),
            ));
        }
        if !self.levels.is_empty() {
            o.push(("levels", Json::obj(self.levels.iter().map(|(k, t)| (k.as_str(), Json::from(*t))))));
        }
        let mut pairs: Vec<(String, Json)> = o.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        // (Options read as the plan's own fields, beside the name and the plan, as written.)
        let at = pairs.iter().position(|(k, _)| k == "licence").map_or(pairs.len(), |i| i + 1);
        for (i, kv) in opts.drain(..).enumerate() {
            pairs.insert(at + i, kv);
        }
        Json::Obj(pairs)
    }

    /// This kind with its base under it: every table of the base, with this kind's entries over
    /// it; this kind's name, about, aliases and options over the base's. The result has no base.
    pub fn over(&self, base: &Kind) -> Kind {
        let mut k = base.clone();
        k.name = self.name.clone();
        k.about = self.about.clone();
        k.aliases = self.aliases.clone();
        k.base = None;
        k.licence = self.licence.clone();
        k.from = self.from.clone();
        if self.size.is_some() {
            k.size = self.size.clone();
        }
        k.numbers.extend(self.numbers.clone());
        k.vary.extend(self.vary.clone());
        k.roles.extend(self.roles.clone());
        k.motion.extend(self.motion.clone());
        k.levels.extend(self.levels.clone());
        k.options.extend(self.options.clone());
        k
    }
}

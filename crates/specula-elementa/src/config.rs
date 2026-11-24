//! What a project says about its elements: materials, which tags are made of them, the grid, the
//! spreading rate and the limits. Parsed from `elements.json`, validated in one place
//! ([`Config::validate`]) that the save codec uses too, so a file and a save obey the same rules.

use crate::error::ElementaError;
use specula_core::hash::StableHasher;
use specula_core::json::Json;

/// The format's version (`"elements": 1`).
pub const FORMAT: u32 = 1;
/// `elements.json` is refused past this many bytes, before it is parsed.
pub const MAX_FILE_BYTES: usize = 64 * 1024;
/// Materials in all, built-in ones included.
pub const MAX_MATERIALS: usize = 64;
/// Tags mapped to materials.
pub const MAX_TAGS: usize = 1024;
/// Bytes in a material's or a tag's name.
pub const MAX_NAME: usize = 32;
/// Seconds of burning a node can hold.
pub const MAX_FUEL_S: f32 = 3600.0;
/// Nodes tracked, at most (the default is [`Limits::DEFAULT`]'s 4,096).
pub const MAX_NODES: u32 = 32_768;
/// Work per spread step, at most (cell lookups, each its two binary searches, plus neighbour
/// checks). Measured on the development laptop (one machine's figure, docs/design/EFFECTUS.md,
/// "Bounded, and said before a run"): a spread step spending all of it at the node limit took 6
/// to 35 ms; twice this, 16 to 55 ms.
pub const MAX_WORK: u32 = 1 << 19;
/// Work per spread step, at least: a floor so that a step does useful work. A neighbour query
/// larger than the budget (a crowded cell has no limit of its own) is not lost: it resumes where
/// it stopped at the next step, and each neighbour of it is given one step's heat.
pub const MIN_WORK: u32 = 1024;
/// The grid's cell, in metres.
pub const MIN_CELL_M: f32 = 0.25;
pub const MAX_CELL_M: f32 = 64.0;
/// Spreading steps a second.
pub const MAX_SPREAD_HZ: u32 = 60;

/// What a thing is made of: how it takes heat, burns, cools, wets and freezes.
///
/// `heat` is a ratio: −1 frozen solid, 0 ambient, 1 white hot. Rates are per second.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    /// The heat at which it catches fire; `None`: it never burns (stone, metal, water).
    pub ignite: Option<f32>,
    /// Seconds of burning a node of this material holds, unless the node says its own.
    pub burn_s: f32,
    /// Heat a second it gives a neighbour it touches while burning (0 at the reach's edge).
    pub heat_out: f32,
    /// The heat a burning node holds at (above `ignite`).
    pub flame: f32,
    /// How fast it returns to the ambient heat: the fraction of the gap closed a second.
    pub cool: f32,
    /// Embers thrown downwind a second while burning (a chance per step, from the seeded stream).
    pub embers: f32,
    /// Lightning passes through it (stage 10 reads it; stored and saved now).
    pub conducts: bool,
    /// The heat at or below which it freezes; `None`: it never freezes.
    pub freeze: Option<f32>,
    /// Wetness it never dries below (water is always wet).
    pub quench: f32,
    /// The heat a new node of this material starts at (ice starts frozen).
    pub start_heat: f32,
}

/// The limits of a run, checked when the data is given and said before a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Nodes tracked at once.
    pub nodes: u32,
    /// Nodes burning at once; a node that would ignite past it waits, the longest waiting
    /// catching first.
    pub burning: u32,
    /// Work per spread step (cell lookups plus neighbour checks); a step that would pass it is
    /// cut and resumed at the next step where it stopped. A burning node the budget does not
    /// reach in a step gives no heat that step: the heat is not given later. With the same nodes
    /// burning, a node then catches no sooner and only if it would with room; over a whole fire
    /// the limit can change what catches, either way (docs/design/EFFECTUS.md, "What a limit may
    /// change").
    pub work: u32,
}

impl Limits {
    pub const DEFAULT: Limits = Limits { nodes: 4096, burning: 512, work: 262_144 };
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::DEFAULT
    }
}

/// A project's elements: the materials (built-in ones with the project's overrides, sorted by
/// name), tags, the grid, the spreading rate and the limits.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub materials: Vec<Material>,
    /// Entity tag → material index, sorted by tag.
    pub tags: Vec<(String, u16)>,
    /// The neighbour grid's cell, metres.
    pub cell_m: f32,
    /// The gap between two things' surfaces that heat crosses, metres (at most `cell_m`).
    pub reach_m: f32,
    /// Spreading steps a second; the engine's tick rate must be a whole multiple of it.
    pub spread_hz: u32,
    pub limits: Limits,
}

/// One row of the built-in table: name, ignite, burn_s, heat_out, flame, cool, embers, conducts,
/// freeze, quench, start_heat. The engine's own numbers (docs/design/EFFECTUS.md, "Elementa's
/// numbers"), chosen so that a fire reads at game speed, not measured from real materials.
type Row = (&'static str, Option<f32>, f32, f32, f32, f32, f32, bool, Option<f32>, f32, f32);

const BUILT_IN: [Row; 10] = [
    ("cloth", Some(0.25), 12.0, 0.5, 0.7, 0.2, 0.03, false, None, 0.0, 0.0),
    ("flesh", Some(0.6), 8.0, 0.2, 0.7, 0.3, 0.0, false, None, 0.0, 0.0),
    ("grass", Some(0.2), 6.0, 0.4, 0.6, 0.2, 0.02, false, None, 0.0, 0.0),
    ("ice", None, 0.0, 0.0, 0.0, 0.006, 0.0, false, Some(-0.2), 1.0, -0.6),
    ("metal", None, 0.0, 0.0, 0.0, 0.3, 0.0, true, None, 0.0, 0.0),
    ("oil", Some(0.15), 20.0, 1.0, 0.95, 0.1, 0.0, false, None, 0.0, 0.0),
    ("snow", None, 0.0, 0.0, 0.0, 0.012, 0.0, false, Some(-0.15), 0.5, -0.4),
    ("stone", None, 0.0, 0.0, 0.0, 0.05, 0.0, false, None, 0.0, 0.0),
    ("water", None, 0.0, 0.0, 0.0, 0.1, 0.0, true, Some(-0.5), 1.0, 0.0),
    ("wood", Some(0.35), 40.0, 0.6, 0.8, 0.1, 0.05, false, None, 0.0, 0.0),
];

/// The built-in materials, sorted by name.
pub fn built_in() -> Vec<Material> {
    BUILT_IN
        .iter()
        .map(|&(name, ignite, burn_s, heat_out, flame, cool, embers, conducts, freeze, quench, start_heat)| Material {
            name: name.to_owned(),
            ignite,
            burn_s,
            heat_out,
            flame,
            cool,
            embers,
            conducts,
            freeze,
            quench,
            start_heat,
        })
        .collect()
}

impl Default for Config {
    fn default() -> Config {
        Config { materials: built_in(), tags: Vec::new(), cell_m: 2.0, reach_m: 0.5, spread_hz: 10, limits: Limits::DEFAULT }
    }
}

const TOP_KEYS: [&str; 7] = ["elements", "materials", "tags", "cell_m", "reach_m", "spread_hz", "limits"];
const MATERIAL_KEYS: [&str; 10] =
    ["ignite", "burn_s", "heat_out", "flame", "cool", "embers", "conducts", "freeze", "quench", "start_heat"];
const LIMIT_KEYS: [&str; 3] = ["nodes", "burning", "work"];

/// Keys an object of the file may have, at most (the tags' cap, the largest).
const MAX_KEYS: usize = MAX_TAGS;

fn object<'a>(v: &'a Json, path: &str) -> Result<&'a [(String, Json)], ElementaError> {
    let pairs = v.as_object().ok_or_else(|| wrong_type(path, "an object"))?;
    if pairs.len() > MAX_KEYS {
        return Err(ElementaError::limit("keys", MAX_KEYS as u64, pairs.len() as u64, format!("{path} has {} keys; at most {MAX_KEYS}", pairs.len()))
            .with("path", path));
    }
    // Duplicates by sorting (k log k), not by comparing every pair.
    let mut keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
    keys.sort_unstable();
    if let Some(w) = keys.windows(2).find(|w| w[0] == w[1]) {
        let k: String = w[0].chars().take(MAX_NAME).collect();
        return Err(ElementaError::new("duplicate", format!("{path}.{k} is given twice")).with("path", format!("{path}.{k}")));
    }
    Ok(pairs)
}

fn wrong_type(path: &str, want: &str) -> ElementaError {
    ElementaError::new("wrong_type", format!("{path} must be {want}")).with("path", path).with("expected", want)
}

fn known(pairs: &[(String, Json)], keys: &[&str], path: &str) -> Result<(), ElementaError> {
    if let Some((k, _)) = pairs.iter().find(|(k, _)| !keys.contains(&k.as_str())) {
        let shown: String = k.chars().take(MAX_NAME).collect();
        return Err(ElementaError::new("unknown_key", format!("{path} has no key {shown:?}; it knows {}", keys.join(", ")))
            .with("path", path)
            .with("key", shown)
            .with("known", Json::arr(keys.iter().map(|k| Json::from(*k)))));
    }
    Ok(())
}

fn get<'a>(pairs: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn number(v: &Json, path: &str, min: f64, max: f64) -> Result<f64, ElementaError> {
    let x = v.as_f64().ok_or_else(|| wrong_type(path, "a number"))?;
    if !x.is_finite() || x < min || x > max {
        return Err(ElementaError::range(path, x, min, max));
    }
    Ok(x)
}

fn whole(v: &Json, path: &str, min: u32, max: u32) -> Result<u32, ElementaError> {
    let x = number(v, path, f64::from(min), f64::from(max))?;
    if x != (x as u32) as f64 {
        return Err(ElementaError::new("wrong_type", format!("{path} must be a whole number; it is {x}")).with("path", path).with("value", x));
    }
    Ok(x as u32)
}

/// A name: 1 to 32 bytes of `a-z`, `0-9` and `_`, starting with a letter.
pub fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_NAME
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn name_error(what: &str, s: &str) -> ElementaError {
    let shown: String = s.chars().take(MAX_NAME + 8).collect();
    ElementaError::new("bad_name", format!("{what} {shown:?} must be 1 to {MAX_NAME} bytes of a-z, 0-9 and _, starting with a letter"))
        .with("name", shown)
}

/// The ranges every material field must hold (path suffix, min, max).
const RANGES: [(&str, f64, f64); 9] = [
    ("ignite", 0.01, 1.0),
    ("burn_s", 0.0, MAX_FUEL_S as f64),
    ("heat_out", 0.0, 4.0),
    ("flame", 0.0, 1.0),
    ("cool", 0.0, 10.0),
    ("embers", 0.0, 1.0),
    ("freeze", -1.0, -0.01),
    ("quench", 0.0, 1.0),
    ("start_heat", -1.0, 1.0),
];

fn range_of(field: &str) -> (f64, f64) {
    RANGES.iter().find(|r| r.0 == field).map(|r| (r.1, r.2)).unwrap_or((0.0, 0.0))
}

impl Config {
    /// Parses `elements.json`. Every key is known, every number in its range, every name valid,
    /// every count within its cap; the first fault is the error, with its path.
    pub fn parse(text: &str) -> Result<Config, ElementaError> {
        if text.len() > MAX_FILE_BYTES {
            return Err(ElementaError::new("too_large", format!("elements.json is {} bytes; at most {MAX_FILE_BYTES}", text.len()))
                .with("bytes", text.len())
                .with("max", MAX_FILE_BYTES));
        }
        let json = Json::parse(text).map_err(|e| {
            ElementaError::new("bad_json", format!("elements.json is not JSON: {}", e.message)).with("offset", e.offset)
        })?;
        Config::from_json(&json)
    }

    /// As [`Config::parse`], from parsed JSON.
    pub fn from_json(json: &Json) -> Result<Config, ElementaError> {
        let top = object(json, "elements.json")?;
        known(top, &TOP_KEYS, "elements.json")?;
        let version = get(top, "elements").ok_or_else(|| {
            ElementaError::new("unsupported_version", "elements.json must say its format: \"elements\": 1").with("path", "elements")
        })?;
        let v = whole(version, "elements", 0, u32::MAX)?;
        if v != FORMAT {
            return Err(ElementaError::new("unsupported_version", format!("elements.json format {v}; this engine reads {FORMAT}"))
                .with("version", v)
                .with("supported", FORMAT));
        }
        let mut c = Config::default();
        if let Some(m) = get(top, "materials") {
            let pairs = object(m, "materials")?;
            for (name, body) in pairs {
                if !valid_name(name) {
                    return Err(name_error("material", name).with("path", "materials"));
                }
                let path = format!("materials.{name}");
                let fields = object(body, &path)?;
                known(fields, &MATERIAL_KEYS, &path)?;
                let at = match c.materials.iter().position(|x| &x.name == name) {
                    Some(i) => i,
                    None => {
                        if c.materials.len() >= MAX_MATERIALS {
                            return Err(ElementaError::limit(
                                "materials",
                                MAX_MATERIALS as u64,
                                c.materials.len() as u64 + 1,
                                format!("more than {MAX_MATERIALS} materials, built-in ones included"),
                            ));
                        }
                        c.materials.push(Material {
                            name: name.clone(),
                            ignite: None,
                            burn_s: 0.0,
                            heat_out: 0.0,
                            flame: 0.0,
                            cool: 0.1,
                            embers: 0.0,
                            conducts: false,
                            freeze: None,
                            quench: 0.0,
                            start_heat: 0.0,
                        });
                        c.materials.len() - 1
                    }
                };
                let mat = &mut c.materials[at];
                for (key, value) in fields {
                    let p = format!("{path}.{key}");
                    if key == "conducts" {
                        mat.conducts = value.as_bool().ok_or_else(|| wrong_type(&p, "true or false"))?;
                        continue;
                    }
                    let (min, max) = range_of(key);
                    // `null` takes away a threshold a built-in material has (`"ignite": null`).
                    if value.is_null() && (key == "ignite" || key == "freeze") {
                        if key == "ignite" {
                            mat.ignite = None;
                        } else {
                            mat.freeze = None;
                        }
                        continue;
                    }
                    let x = number(value, &p, min, max)? as f32;
                    match key.as_str() {
                        "ignite" => mat.ignite = Some(x),
                        "burn_s" => mat.burn_s = x,
                        "heat_out" => mat.heat_out = x,
                        "flame" => mat.flame = x,
                        "cool" => mat.cool = x,
                        "embers" => mat.embers = x,
                        "freeze" => mat.freeze = Some(x),
                        "quench" => mat.quench = x,
                        _ => mat.start_heat = x,
                    }
                }
                // A material given a threshold to burn and no flame of its own burns at the
                // midpoint between catching and white hot.
                if let Some(ig) = mat.ignite {
                    if get(fields, "flame").is_none() && mat.flame <= ig {
                        mat.flame = (ig + 1.0) * 0.5;
                    }
                    if get(fields, "burn_s").is_none() && mat.burn_s <= 0.0 {
                        mat.burn_s = 10.0;
                    }
                    if get(fields, "heat_out").is_none() && mat.heat_out <= 0.0 {
                        mat.heat_out = 0.5;
                    }
                }
            }
            c.materials.sort_by(|a, b| a.name.cmp(&b.name));
        }
        if let Some(t) = get(top, "tags") {
            let pairs = object(t, "tags")?;
            if pairs.len() > MAX_TAGS {
                return Err(ElementaError::limit("tags", MAX_TAGS as u64, pairs.len() as u64, format!("{} tags; at most {MAX_TAGS}", pairs.len())));
            }
            for (tag, mat) in pairs {
                if !valid_name(tag) {
                    return Err(name_error("tag", tag).with("path", "tags"));
                }
                let p = format!("tags.{tag}");
                let m = mat.as_str().ok_or_else(|| wrong_type(&p, "a material's name"))?;
                let i = c.material(m).ok_or_else(|| {
                    let shown: String = m.chars().take(MAX_NAME).collect();
                    ElementaError::new("unknown_material", format!("{p} names {shown:?}, which is not a material"))
                        .with("path", p.as_str())
                        .with("material", shown)
                })?;
                c.tags.push((tag.clone(), i));
            }
            c.tags.sort();
        }
        if let Some(v) = get(top, "cell_m") {
            c.cell_m = number(v, "cell_m", f64::from(MIN_CELL_M), f64::from(MAX_CELL_M))? as f32;
        }
        if let Some(v) = get(top, "reach_m") {
            c.reach_m = number(v, "reach_m", 0.0, f64::from(MAX_CELL_M))? as f32;
        }
        if let Some(v) = get(top, "spread_hz") {
            c.spread_hz = whole(v, "spread_hz", 1, MAX_SPREAD_HZ)?;
        }
        if let Some(l) = get(top, "limits") {
            let pairs = object(l, "limits")?;
            known(pairs, &LIMIT_KEYS, "limits")?;
            if let Some(v) = get(pairs, "nodes") {
                c.limits.nodes = whole(v, "limits.nodes", 1, MAX_NODES)?;
            }
            if let Some(v) = get(pairs, "burning") {
                c.limits.burning = whole(v, "limits.burning", 1, MAX_NODES)?;
            }
            if let Some(v) = get(pairs, "work") {
                c.limits.work = whole(v, "limits.work", MIN_WORK, MAX_WORK)?;
            }
        }
        c.validate()?;
        Ok(c)
    }

    /// Every rule a config holds, whether it came from a file or a save.
    pub fn validate(&self) -> Result<(), ElementaError> {
        if self.materials.is_empty() || self.materials.len() > MAX_MATERIALS {
            return Err(ElementaError::limit("materials", MAX_MATERIALS as u64, self.materials.len() as u64, "1 to 64 materials"));
        }
        for (i, m) in self.materials.iter().enumerate() {
            if !valid_name(&m.name) {
                return Err(name_error("material", &m.name));
            }
            if i > 0 && self.materials[i - 1].name >= m.name {
                return Err(ElementaError::new("bad_config", "materials must be sorted by name, each once").with("material", m.name.as_str()));
            }
            let p = |f: &str| format!("materials.{}.{f}", m.name);
            let check = |field: &str, x: f32| -> Result<(), ElementaError> {
                let (min, max) = range_of(field);
                let v = f64::from(x);
                if !x.is_finite() || v < min || v > max {
                    return Err(ElementaError::range(&p(field), v, min, max));
                }
                Ok(())
            };
            if let Some(ig) = m.ignite {
                check("ignite", ig)?;
            }
            check("burn_s", m.burn_s)?;
            check("heat_out", m.heat_out)?;
            check("flame", m.flame)?;
            check("cool", m.cool)?;
            check("embers", m.embers)?;
            if let Some(f) = m.freeze {
                check("freeze", f)?;
            }
            check("quench", m.quench)?;
            check("start_heat", m.start_heat)?;
            if let Some(ig) = m.ignite {
                if m.flame <= ig {
                    return Err(ElementaError::new(
                        "bad_config",
                        format!("{} is {}; a burning thing must hold above its ignite ({ig}), or it puts itself out", p("flame"), m.flame),
                    )
                    .with("path", p("flame"))
                    .with("flame", m.flame)
                    .with("ignite", ig));
                }
                if m.burn_s <= 0.0 {
                    return Err(ElementaError::new("bad_config", format!("{} must be above 0 for a material that burns", p("burn_s")))
                        .with("path", p("burn_s")));
                }
            }
        }
        if self.tags.len() > MAX_TAGS {
            return Err(ElementaError::limit("tags", MAX_TAGS as u64, self.tags.len() as u64, "too many tags"));
        }
        for (i, (t, m)) in self.tags.iter().enumerate() {
            if !valid_name(t) {
                return Err(name_error("tag", t));
            }
            if i > 0 && self.tags[i - 1].0 >= *t {
                return Err(ElementaError::new("bad_config", "tags must be sorted, each once").with("tag", t.as_str()));
            }
            if usize::from(*m) >= self.materials.len() {
                return Err(ElementaError::new("unknown_material", format!("tag {t} names material {m}, past the table")).with("tag", t.as_str()));
            }
        }
        let cell = f64::from(self.cell_m);
        if !self.cell_m.is_finite() || cell < f64::from(MIN_CELL_M) || cell > f64::from(MAX_CELL_M) {
            return Err(ElementaError::range("cell_m", cell, f64::from(MIN_CELL_M), f64::from(MAX_CELL_M)));
        }
        if !self.reach_m.is_finite() || self.reach_m < 0.0 || self.reach_m > self.cell_m {
            return Err(ElementaError::range("reach_m", f64::from(self.reach_m), 0.0, cell)
                .with("why", "heat crosses at most one cell, so a query stays within its cells"));
        }
        if self.spread_hz == 0 || self.spread_hz > MAX_SPREAD_HZ {
            return Err(ElementaError::range("spread_hz", f64::from(self.spread_hz), 1.0, f64::from(MAX_SPREAD_HZ)));
        }
        let l = self.limits;
        if l.nodes == 0 || l.nodes > MAX_NODES {
            return Err(ElementaError::range("limits.nodes", f64::from(l.nodes), 1.0, f64::from(MAX_NODES)));
        }
        if l.burning == 0 || l.burning > l.nodes {
            return Err(ElementaError::range("limits.burning", f64::from(l.burning), 1.0, f64::from(l.nodes))
                .with("why", "no more can burn than are tracked"));
        }
        if l.work < MIN_WORK || l.work > MAX_WORK {
            return Err(ElementaError::range("limits.work", f64::from(l.work), f64::from(MIN_WORK), f64::from(MAX_WORK)));
        }
        Ok(())
    }

    /// A material's index by name.
    pub fn material(&self, name: &str) -> Option<u16> {
        self.materials.binary_search_by(|m| m.name.as_str().cmp(name)).ok().map(|i| i as u16)
    }

    /// The material a tag maps to.
    pub fn tag(&self, tag: &str) -> Option<u16> {
        self.tags.binary_search_by(|(t, _)| t.as_str().cmp(tag)).ok().map(|i| self.tags[i].1)
    }

    /// A stable hash of everything in the config.
    pub fn hash(&self) -> u64 {
        let mut h = StableHasher::with_seed(0x656c_656d_636f_6e66);
        h.write_u64(self.materials.len() as u64);
        for m in &self.materials {
            h.write_str(&m.name);
            h.write_bool(m.ignite.is_some());
            h.write_f32(m.ignite.unwrap_or(0.0));
            for x in [m.burn_s, m.heat_out, m.flame, m.cool, m.embers, m.quench, m.start_heat] {
                h.write_f32(x);
            }
            h.write_bool(m.conducts);
            h.write_bool(m.freeze.is_some());
            h.write_f32(m.freeze.unwrap_or(0.0));
        }
        h.write_u64(self.tags.len() as u64);
        for (t, m) in &self.tags {
            h.write_str(t);
            h.write_u32(u32::from(*m));
        }
        h.write_f32(self.cell_m);
        h.write_f32(self.reach_m);
        h.write_u32(self.spread_hz);
        h.write_u32(self.limits.nodes);
        h.write_u32(self.limits.burning);
        h.write_u32(self.limits.work);
        h.finish()
    }

    /// What the limits allow at worst, said before a run: the most operations a spread step, a
    /// grid update, an act and a change can take, so a tick's (below), the most grid entries,
    /// and the memory.
    pub fn worst_case(&self) -> WorstCase {
        let n = u64::from(self.limits.nodes);
        let entries = n * u64::from(crate::grid::MAX_NODE_CELLS);
        WorstCase {
            ops_per_step: step_ops(n, self.limits.work),
            grid_ops: crate::grid::update_ops(n, n, n.max(1024), entries, entries, entries),
            act_ops: act_ops(n),
            call_ops: crate::sim::Elementa::call_ops(n),
            steps_per_second: self.spread_hz,
            grid_entries: entries,
            // Nodes and their per-node buffers; the entries twice (the grid merges into a second
            // buffer).
            bytes: n * (std::mem::size_of::<crate::sim::Node>() as u64 + 4)
                + 2 * n * u64::from(crate::grid::MAX_NODE_CELLS) * std::mem::size_of::<crate::grid::Entry>() as u64,
        }
    }

    pub fn to_json(&self) -> Json {
        let w = self.worst_case();
        Json::obj([
            ("elements", Json::from(FORMAT)),
            ("materials", Json::arr(self.materials.iter().map(|m| Json::from(m.name.as_str())))),
            ("tags", Json::from(self.tags.len())),
            ("cell_m", Json::from(self.cell_m)),
            ("reach_m", Json::from(self.reach_m)),
            ("spread_hz", Json::from(self.spread_hz)),
            (
                "limits",
                Json::obj([
                    ("nodes", Json::from(self.limits.nodes)),
                    ("burning", Json::from(self.limits.burning)),
                    ("work", Json::from(self.limits.work)),
                ]),
            ),
            (
                "worst_case",
                Json::obj([
                    ("ops_per_step", Json::from(w.ops_per_step)),
                    ("grid_ops", Json::from(w.grid_ops)),
                    ("act_ops", Json::from(w.act_ops)),
                    ("call_ops", Json::from(w.call_ops)),
                    ("steps_per_second", Json::from(w.steps_per_second)),
                    ("grid_entries", Json::from(w.grid_entries)),
                    ("bytes", Json::from(w.bytes)),
                ]),
            ),
        ])
    }
}

/// The most one spread step costs at `n` nodes and a work limit: every node updated, the ready
/// nodes sorted (at most `n`), the step's events sorted (at most two a node: one ending, one
/// catching), the work budget, and a resumed query's skip list marked and unmarked.
pub fn step_ops(n: u64, work: u32) -> u64 {
    let skip = crate::sim::MAX_SKIP as u64;
    n + crate::grid::sort_ops(n) + crate::grid::sort_ops(2 * n) + u64::from(work) + 2 * skip * (1 + crate::grid::log2_ceil(n + 1))
}

/// The most one act costs at `n` nodes: its work (at most `2n + 1`) and one update a node.
pub fn act_ops(n: u64) -> u64 {
    3 * n + 1
}

/// The most a config's limits allow (see [`Config::worst_case`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorstCase {
    /// Node updates, the sorts and the work budget, in one spread step ([`step_ops`]).
    pub ops_per_step: u64,
    /// Bringing the grid up to date, at worst (every node at the limit changed, each covering
    /// 64 cells): at most once a tick, by the spread step that follows any add, remove or move
    /// into other cells ([`crate::grid::update_ops`]). Far less after a few changes.
    pub grid_ops: u64,
    /// One act ([`act_ops`]); at most [`crate::sim::MAX_ACTS_PER_TICK`] a tick.
    pub act_ops: u64,
    /// One add, remove or move ([`crate::sim::Elementa::call_ops`]).
    pub call_ops: u64,
    pub steps_per_second: u32,
    /// Grid entries when every node covers the most cells it may.
    pub grid_entries: u64,
    /// Bytes of nodes and grid at the node limit, at worst.
    pub bytes: u64,
}

impl WorstCase {
    /// The most one tick costs, with `acts` acts and `calls` adds, removes and moves made in it
    /// (in any order, interleaved as they come): a spread step, one grid update, and each call.
    /// An act never updates the grid (it looks at every node instead when the grid is out of
    /// date), so acts between changes add no grid work. `add_many` is counted apart
    /// ([`crate::sim::Elementa::add_many_ops`]).
    pub fn tick_ops(&self, acts: u64, calls: u64) -> u64 {
        self.ops_per_step + self.grid_ops + acts * self.act_ops + calls * self.call_ops
    }
}

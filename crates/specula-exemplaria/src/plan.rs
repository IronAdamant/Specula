//! A plan: a body's skeleton, the table of numbers it takes, its traits, roles, options and
//! motions, and how a specimen of it is written as a recipe and an anims file
//! (docs/design/EXEMPLARIA.md, A.1 and A.3). Plans are Rust and few; kinds are data and many.
//!
//! **Adding a plan**: a file under `src/plans/` with a `pub static PLAN: MyPlan` implementing
//! [`Plan`], and one line in [`crate::plans::PLANS`]. Its tables are `'static`: a test reads
//! every plan's tables for what a kind file is held to (names valid and unique, shares and
//! spreads in range, at most four roles, every motion's generator one the anim library has).

use crate::draw::Specimen;
use crate::error::Error;
use crate::kind::{Kind, Why};
use crate::write::Writing;

/// A number's unit, and how finely a drawn value is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Metres, written to 0.1 mm.
    Metre,
    /// Degrees, written to 0.01 degree.
    Degree,
    /// A plain ratio, written to 1e-4.
    Ratio,
}

impl Unit {
    pub fn name(self) -> &'static str {
        match self {
            Unit::Metre => "m",
            Unit::Degree => "deg",
            Unit::Ratio => "ratio",
        }
    }
    /// The step a drawn value is rounded to before it is written.
    pub fn step(self) -> f64 {
        match self {
            Unit::Metre | Unit::Ratio => 1e-4,
            Unit::Degree => 1e-2,
        }
    }
}

/// The size: the one number every length follows.
#[derive(Debug)]
pub struct SizeDef {
    pub name: &'static str,
    pub unit: Unit,
    /// The plan's default mean, spread and range (a kind sets its own).
    pub mean: f64,
    pub sd: f64,
    pub range: [f64; 2],
    pub about: &'static str,
}

/// A number of the plan's table.
#[derive(Debug)]
pub struct NumberDef {
    pub name: &'static str,
    pub unit: Unit,
    /// Its default share of the size, at the mean size.
    pub share: f64,
    /// How it grows with size: `value ∝ size^exponent` (1: in proportion).
    pub exponent: f64,
    /// The traits that move it, each with a weight: `value × (1 + weight × trait)`.
    pub traits: &'static [(&'static str, f64)],
    /// What the plan can write for it, low and high, in its unit (on the unit's step): a body
    /// the plan's `write` makes whole for any value inside. A kind whose draws could leave it is
    /// refused when it is resolved ([`crate::draw`], "Reach"), and a draw never leaves it.
    pub range: [f64; 2],
    pub about: &'static str,
}

/// A trait: a residual that moves several numbers together (a build moves every girth).
#[derive(Debug)]
pub struct TraitDef {
    pub name: &'static str,
    /// Its default spread (a fraction); a kind's `vary` sets its own.
    pub spread: f64,
    pub about: &'static str,
}

/// A role a kind's parts are drawn in, with the plan's default hint colour.
#[derive(Debug)]
pub struct RoleDef {
    pub name: &'static str,
    pub hint: &'static str,
    pub about: &'static str,
}

/// An option of the plan (a string a kind sets at its top level).
#[derive(Debug)]
pub struct OptionDef {
    pub name: &'static str,
    pub values: &'static [&'static str],
    pub default: &'static str,
    pub about: &'static str,
}

/// A clip the plan writes: its generator kind in `specula-anim` and what a kind may set of it.
#[derive(Debug)]
pub struct MotionDef {
    pub name: &'static str,
    /// The anim library's generator kind (`gait`, `idle`, `swing`...).
    pub generator: &'static str,
    /// Footfall patterns it takes ([`crate::gaits`]); empty: none.
    pub patterns: &'static [&'static str],
    /// Presets it takes; empty: none.
    pub presets: &'static [&'static str],
    /// Whether a Froude number sets its speed.
    pub froude: bool,
    pub about: &'static str,
}

/// A plan. See the module's notes for how one is added.
pub trait Plan: Sync {
    fn name(&self) -> &'static str;
    /// What it makes, in one line.
    fn about(&self) -> &'static str;
    fn size(&self) -> &'static SizeDef;
    fn numbers(&self) -> &'static [NumberDef];
    fn traits(&self) -> &'static [TraitDef];
    fn roles(&self) -> &'static [RoleDef];
    fn options(&self) -> &'static [OptionDef];
    fn motions(&self) -> &'static [MotionDef];
    /// Writes a specimen: its recipe (rig and parts) and its clips, through `w`.
    fn write(&self, s: &Specimen, w: &mut Writing) -> Result<(), Error>;
}

/// A kind with its base applied and checked against its plan: ready to draw.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub kind: Kind,
    pub plan: &'static dyn Plan,
    /// The base it was made over, if any.
    pub base: Option<String>,
}

impl std::fmt::Debug for dyn Plan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "plan {}", self.name())
    }
}

/// The plan of this name.
pub fn plan(name: &str) -> Option<&'static dyn Plan> {
    crate::plans::PLANS.iter().copied().find(|p| p.name() == name)
}

fn unknown(code: &'static str, path: String, what: &str, word: &str, names: &[&str]) -> Error {
    Error::new(code, path, format!("'{word}' is not {what} ({})", names.join(", "))).with_near(crate::words::nearest(word, names.iter().copied(), 5))
}

/// A kind over its base (one level), checked against its plan: every number, trait, role,
/// option, motion and pattern a name the plan knows, its roles at most four. `base` is the kind
/// `kind.base` names, found by the caller (in the library, or in the project's folder).
pub fn resolve(kind: &Kind, base: Option<&Kind>) -> Result<Resolved, Error> {
    let merged = match (&kind.base, base) {
        (None, _) => kind.clone(),
        (Some(b), None) => return Err(Error::new("base_not_found", "base", format!("the base '{b}' was not found beside this kind"))),
        (Some(b), Some(base)) => {
            if base.name != *b {
                return Err(Error::new("base_not_found", "base", format!("the base given is '{}', not '{b}'", base.name)));
            }
            if *b == kind.name {
                return Err(Error::new("base_cycle", "base", "a kind cannot be its own base"));
            }
            if let Some(bb) = &base.base {
                if *bb == kind.name {
                    return Err(Error::new("base_cycle", "base", format!("'{b}' names this kind as its base: a ring")));
                }
                return Err(Error::new("base_too_deep", "base", format!("'{b}' has a base of its own ('{bb}'): one level of base only, so a kind is read from two files at most")));
            }
            if base.plan != kind.plan {
                return Err(Error::new("base_other_plan", "base", format!("'{b}' is a {}, this kind a {}: a base is of the same plan", base.plan, kind.plan)));
            }
            kind.over(base)
        }
    };
    let plan = plan(&merged.plan).ok_or_else(|| {
        let all: Vec<&str> = crate::plans::PLANS.iter().map(|p| p.name()).collect();
        unknown("unknown_plan", "plan".into(), "a plan", &merged.plan, &all)
    })?;
    if let Some(s) = &merged.size {
        if s.name != plan.size().name {
            return Err(unknown("unknown_name", format!("size.{}", s.name), "this plan's size", &s.name, &[plan.size().name]));
        }
        if s.unit != plan.size().unit.name() {
            return Err(Error::new("out_of_range", format!("size.{}.unit", s.name), format!("this plan's size is in {}", plan.size().unit.name())));
        }
    }
    let numbers: Vec<&str> = plan.numbers().iter().map(|n| n.name).collect();
    for k in merged.numbers.keys() {
        if !numbers.contains(&k.as_str()) {
            return Err(unknown("unknown_name", format!("numbers.{k}"), "a number of this plan", k, &numbers));
        }
    }
    let traits: Vec<&str> = plan.traits().iter().map(|t| t.name).collect();
    for k in merged.vary.keys() {
        if !traits.contains(&k.as_str()) {
            return Err(unknown("unknown_name", format!("vary.{k}"), "a trait of this plan", k, &traits));
        }
    }
    let roles: Vec<&str> = plan.roles().iter().map(|r| r.name).collect();
    for k in merged.roles.keys() {
        if !roles.contains(&k.as_str()) {
            return Err(unknown("unknown_name", format!("roles.{k}"), "a role of this plan", k, &roles));
        }
    }
    if merged.roles.len() > crate::kind::MAX_ROLES {
        return Err(Error::new("too_many", "roles", format!("{} roles with the base's; at most {}", merged.roles.len(), crate::kind::MAX_ROLES)));
    }
    let options: Vec<&str> = plan.options().iter().map(|o| o.name).collect();
    for (k, v) in &merged.options {
        let Some(def) = plan.options().iter().find(|o| o.name == k) else {
            return Err(unknown("unknown_field", k.clone(), "a field of a kind or an option of this plan", k, &options));
        };
        if !def.values.contains(&v.as_str()) {
            return Err(unknown("unknown_name", k.clone(), &format!("a value of '{k}'"), v, def.values));
        }
    }
    let motions: Vec<&str> = plan.motions().iter().map(|m| m.name).collect();
    for (k, m) in &merged.motion {
        let Some(def) = plan.motions().iter().find(|d| d.name == k) else {
            return Err(unknown("unknown_name", format!("motion.{k}"), "a clip of this plan", k, &motions));
        };
        if let Some(p) = &m.pattern {
            if !def.patterns.contains(&p.as_str()) {
                return Err(unknown("unknown_name", format!("motion.{k}.pattern"), "a pattern of this clip", p, def.patterns));
            }
        }
        if let Some(p) = &m.preset {
            if !def.presets.contains(&p.as_str()) {
                return Err(unknown("unknown_name", format!("motion.{k}.preset"), "a preset of this clip", p, def.presets));
            }
        }
        if m.froude.is_some() && !def.froude {
            return Err(Error::new("unknown_field", format!("motion.{k}.froude"), format!("'{k}' has no speed")));
        }
    }
    let resolved = Resolved { kind: merged, plan, base: kind.base.clone() };
    // Every seed's body inside what the plan can write, or the kind is refused here, naming the
    // kind's own field (a spawn, a write and a take all resolve: none can accept it).
    resolved.reach().map_err(|e| match &resolved.base {
        Some(b) if !own_field(kind, &e.path) => Error { message: format!("{} (the field is its base '{b}''s, read under this kind)", e.message), ..e },
        _ => e,
    })?;
    Ok(resolved)
}

/// Whether the kind file itself (not its base) sets the field at `path` (`vary.build.spread`).
fn own_field(kind: &Kind, path: &str) -> bool {
    let mut it = path.split('.');
    let (table, key) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
    match table {
        "size" => kind.size.is_some(),
        "numbers" => kind.numbers.contains_key(key),
        "vary" => kind.vary.contains_key(key),
        _ => true,
    }
}

impl Resolved {
    /// The numbers the engine set itself in this kind (each with its reason), for its answer.
    pub fn own(&self) -> Vec<(String, String)> {
        let k = &self.kind;
        let mut out = Vec::new();
        if let Some(s) = &k.size {
            if let Why::Own(r) = &s.why {
                out.push((format!("size.{}", s.name), r.clone()));
            }
        }
        for (n, x) in &k.numbers {
            if let Why::Own(r) = &x.why {
                out.push((format!("numbers.{n}"), r.clone()));
            }
        }
        for (n, x) in &k.vary {
            if let Why::Own(r) = &x.why {
                out.push((format!("vary.{n}"), r.clone()));
            }
        }
        for (n, x) in &k.motion {
            if let Some(Why::Own(r)) = &x.why {
                out.push((format!("motion.{n}"), r.clone()));
            }
        }
        out
    }

    /// The plan's numbers this kind does not set: they take the plan's defaults (the engine's).
    pub fn defaulted(&self) -> Vec<&'static str> {
        self.plan.numbers().iter().filter(|n| !self.kind.numbers.contains_key(n.name)).map(|n| n.name).collect()
    }
}

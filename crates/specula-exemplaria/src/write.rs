//! A specimen written out: an ordinary recipe and anims file, which the modeller builds and the
//! anim library generates as any others (docs/design/EXEMPLARIA.md, A.1: "Why a specimen is a
//! recipe and not a mesh"). The recipe's `description` carries the kind, the seed and each
//! drawn number, so a specimen is reproducible from its own file.
//!
//! **Roles** ([`Roles`]): a part names what it is for (`coat`, `claw`); the build resolves a
//! role to the project style's preset (`specula_model::Style::role`). Without a style the text
//! names the roles themselves ([`AsNamed`]), as `exemplar_get` shows them.

use crate::draw::{round, Specimen};
use crate::error::Error;
use crate::gaits;
use crate::plan::{Resolved, Unit};
use specula_core::Json;
use std::collections::BTreeMap;

/// How a role becomes a material preset: the project's style (in the engine), or the role's own
/// name ([`AsNamed`]).
pub trait Roles {
    /// The preset for `role` (`hint`: the kind's hint colour, `#rrggbb`), or why there is none.
    fn preset(&self, role: &str, hint: &str) -> Result<String, String>;
}

/// Roles written as themselves (no style at hand).
pub struct AsNamed;

impl Roles for AsNamed {
    fn preset(&self, role: &str, _hint: &str) -> Result<String, String> {
        Ok(role.to_string())
    }
}

/// A specimen written out.
#[derive(Clone, Debug)]
pub struct Expansion {
    pub specimen: Specimen,
    /// The recipe's text ([`text`]).
    pub recipe: String,
    /// The anims file's text, when the plan writes clips.
    pub anims: Option<String>,
    /// Measures of what was written (`withers_built`), with units.
    pub facts: Vec<(String, f64, &'static str)>,
    /// Each role and the preset it was written as.
    pub roles: Vec<(String, String)>,
    /// Bones, parts and clips written.
    pub counts: Counts,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counts {
    pub bones: usize,
    pub parts: usize,
    pub clips: usize,
}

/// The bones a generated clip moves: the pelvis, the head, and each leg as its foot's key in a
/// footfall pattern (`lh`, `lf`, `rh`, `rf`) with its thigh and shin.
#[derive(Clone, Copy, Debug)]
pub struct Body<'a> {
    pub pelvis: &'a str,
    pub head: Option<&'a str>,
    pub legs: &'a [(&'a str, &'a str, &'a str)],
}

/// What a plan writes into, while it writes a specimen.
pub struct Writing<'a> {
    resolved: &'a Resolved,
    roles: &'a dyn Roles,
    presets: BTreeMap<String, String>,
    bones: Vec<Json>,
    parts: Vec<Json>,
    clips: Vec<Json>,
    facts: Vec<(String, f64, &'static str)>,
}

/// A point in metres, written to 0.1 mm.
pub fn v3(p: [f64; 3]) -> Json {
    Json::arr(p.map(|x| Json::from(round(x, Unit::Metre))))
}

/// A length in metres, written to 0.1 mm.
pub fn m(x: f64) -> Json {
    Json::from(round(x, Unit::Metre))
}

impl Writing<'_> {
    /// The preset a role is drawn with (the kind's hint, else the plan's, to name the nearest).
    pub fn material(&mut self, role: &str) -> Result<String, Error> {
        if let Some(p) = self.presets.get(role) {
            return Ok(p.clone());
        }
        let plan_hint = self.resolved.plan.roles().iter().find(|r| r.name == role).map_or("#808080", |r| r.hint);
        let hint = self.resolved.kind.roles.get(role).map_or(plan_hint, String::as_str);
        let preset = self.roles.preset(role, hint).map_err(|m| Error::new("role_unresolved", format!("roles.{role}"), m))?;
        self.presets.insert(role.to_string(), preset.clone());
        Ok(preset)
    }

    /// A bone of the rig (`head` and `tail` model-space, metres; parents before children).
    pub fn bone(&mut self, name: &str, parent: Option<&str>, head: [f64; 3], tail: Option<[f64; 3]>) {
        let mut o = vec![("name", Json::from(name))];
        if let Some(p) = parent {
            o.push(("parent", Json::from(p)));
        }
        o.push(("head", v3(head)));
        if let Some(t) = tail {
            o.push(("tail", v3(t)));
        }
        self.bones.push(Json::obj(o));
    }

    /// A recipe part: a shape and its ops, drawn in a role, moved by `bone`.
    pub fn part(&mut self, name: &str, role: &str, bone: &str, shape: Json, ops: Vec<Json>) -> Result<(), Error> {
        let material = self.material(role)?;
        let mut o = vec![("name", Json::from(name)), ("material", Json::from(material)), ("bones", Json::from(bone)), ("shape", shape)];
        if !ops.is_empty() {
            o.push(("ops", Json::Arr(ops)));
        }
        self.parts.push(Json::obj(o));
        Ok(())
    }

    /// A box of `size` whose bottom face is centred at `at` (a box stands on y = 0 at the origin).
    pub fn block(&mut self, name: &str, role: &str, bone: &str, size: [f64; 3], at: [f64; 3]) -> Result<(), Error> {
        let shape = Json::obj([("op", Json::from("box")), ("size", v3(size))]);
        self.part(name, role, bone, shape, vec![Json::obj([("op", Json::from("move")), ("by", v3(at))])])
    }

    /// A clip, as the anim library's generator spec.
    pub fn clip(&mut self, spec: Json) {
        self.clips.push(spec);
    }

    /// A gait clip from a footfall pattern at a Froude number, for hips `h` high, on `body`. The
    /// stride, period and duty come from [`gaits::cycle`]: the same spec fits every specimen.
    pub fn gait(&mut self, name: &str, pattern: &str, froude: f64, h: f64, body: &Body) -> Result<gaits::Cycle, Error> {
        let Body { pelvis, head, legs } = *body;
        let p = gaits::pattern(pattern).ok_or_else(|| Error::new("unknown_name", format!("motion.{name}.pattern"), format!("no pattern '{pattern}'")))?;
        let c = gaits::cycle(froude, h, p.walk);
        let legs_json = legs.iter().map(|(key, thigh, shin)| {
            let phase = p.phases.iter().find(|x| x.0 == *key).map_or(0.0, |x| x.1);
            Json::obj([("thigh", Json::from(*thigh)), ("shin", Json::from(*shin)), ("phase", Json::from(phase))])
        });
        let mut o = vec![
            ("name", Json::from(name)),
            ("kind", Json::from("gait")),
            ("duration", Json::from(round(c.period, Unit::Ratio))),
            ("stride", m(c.stride)),
            ("duty", Json::from(round(c.duty, Unit::Ratio))),
            ("note", Json::from(format!("{pattern} at Froude {froude}: {:.2} m/s for hips {:.3} m high. Phases: {}. Stride and duty: {}", c.speed, h, p.source, gaits::SIMILARITY))),
            ("pelvis", Json::from(pelvis)),
        ];
        if let Some(hd) = head {
            o.push(("head", Json::from(hd)));
        }
        o.push(("legs", Json::arr(legs_json)));
        o.push(("lift", m(0.12 * h)));
        o.push(("bounce", m(0.03 * h)));
        self.clip(Json::obj(o));
        Ok(c)
    }

    /// The kind's entry for a clip of the plan's (`None`: the kind does not list it, so it is not
    /// written).
    pub fn motion(&self, name: &str) -> Option<crate::kind::Motion> {
        self.resolved.kind.motion.get(name).cloned()
    }

    /// A measure of what was written, for the specimen's answer.
    pub fn fact(&mut self, name: &str, value: f64, unit: &'static str) {
        self.facts.push((name.to_string(), value, unit));
    }
}

/// A recipe or anims file as text: an object's fields on lines of their own to the second level,
/// and each bone, part or clip on one line, as the library's recipes are written by hand (a
/// pretty print puts every coordinate on a line: a specimen would be ten times as long).
pub fn text(v: &Json) -> String {
    let mut out = String::new();
    lines(v, 0, &mut out);
    out.push('\n');
    out
}

fn lines(v: &Json, depth: usize, out: &mut String) {
    let pad = |n: usize| "  ".repeat(n);
    match v {
        Json::Obj(pairs) if depth <= 1 && !pairs.is_empty() => {
            out.push_str("{\n");
            for (i, (k, x)) in pairs.iter().enumerate() {
                out.push_str(&pad(depth + 1));
                out.push_str(&Json::from(k.as_str()).to_string());
                out.push_str(": ");
                lines(x, depth + 1, out);
                out.push_str(if i + 1 < pairs.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(depth));
            out.push('}');
        }
        Json::Arr(items) if depth <= 2 && !items.is_empty() && items.iter().all(|x| matches!(x, Json::Obj(_))) => {
            out.push_str("[\n");
            for (i, x) in items.iter().enumerate() {
                out.push_str(&pad(depth + 1));
                out.push_str(&x.to_string());
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(depth));
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// Writes the specimen of `seed` (`None`: the mean) of a resolved kind. `from`: where the kind
/// came from, written into the description (`exemplaria/box_creature/box_creature@<hash>`).
pub fn expand(r: &Resolved, seed: Option<u64>, roles: &dyn Roles, from: &str) -> Result<Expansion, Error> {
    let s = r.draw(seed);
    let mut w = Writing { resolved: r, roles, presets: BTreeMap::new(), bones: Vec::new(), parts: Vec::new(), clips: Vec::new(), facts: Vec::new() };
    r.plan.write(&s, &mut w)?;
    let name = &r.kind.name;
    let seed_word = seed.map_or_else(|| "the mean (no seed)".to_string(), |x| format!("seed {x}"));
    let mut drawn = format!("{} {} {}", s.size.name, s.size.value, s.size.unit.name());
    for n in &s.numbers {
        drawn.push_str(&format!("; {} {} {}", n.name, n.value, n.unit.name()));
    }
    let traits: Vec<String> = s.traits.iter().map(|(t, v)| format!("{t} {v:+}")).collect();
    let description = format!(
        "A specimen of the kind {name} ({from}), {seed_word}, plan {}. Drawn: {drawn}. Traits: {}. Written by specula-exemplaria: change the kind, not this text (a recipe changed by hand no longer varies by seed).",
        r.plan.name(),
        if traits.is_empty() { "none".to_string() } else { traits.join(", ") }
    );
    let mut top = vec![
        ("recipe", Json::from(1)),
        ("name", Json::from(name.as_str())),
        ("category", Json::from("creature")),
        ("description", Json::from(description)),
    ];
    if let Some(x) = seed {
        top.push(("seed", Json::from(x)));
    }
    if let Some(t) = r.kind.levels.get("near") {
        top.push(("budget", Json::obj([("max_triangles", Json::from(*t))])));
    }
    let counts = Counts { bones: w.bones.len(), parts: w.parts.len(), clips: w.clips.len() };
    if !w.bones.is_empty() {
        top.push(("rig", Json::obj([("bones", Json::Arr(w.bones))])));
    }
    top.push(("parts", Json::Arr(w.parts)));
    let recipe = text(&Json::obj(top));
    let anims = (!w.clips.is_empty()).then(|| {
        text(&Json::obj([
            ("anims", Json::from(1)),
            ("rig", Json::from(name.as_str())),
            ("description", Json::from(format!("The clips of {name}, {seed_word}, written by specula-exemplaria from the plan's motions."))),
            ("clips", Json::Arr(w.clips)),
        ]))
    });
    Ok(Expansion { specimen: s, recipe, anims, facts: w.facts, roles: w.presets.into_iter().collect(), counts })
}

//! `box_creature`: a creature of boxes on four legs, the test plan that goes the library's whole
//! path (kind, seed, specimen, recipe built by the modeller, clips, catalogue). It is not an
//! animal and claims nothing measured: every default here is the engine's own.
//!
//! Its rig: `root`; `body` (the pelvis of the gaits); `neck`, `head`, `tail`; each leg a `thigh_X`
//! and a `shin_X` for `X` in `lf`, `rf`, `lh`, `rh` (left = +X, front = +Z).
//!
//! **Whole at every number inside its range, with its relations held** (docs/design/EXEMPLARIA.md,
//! "For the plans' lanes"). Every part is a box, and no two boxes share a face: any two faces of
//! two parts that face the same way and overlap seen along that axis are apart by a difference
//! that is either a fixed fraction of one number (a thigh's side 0.1 of its girth inside the
//! trunk's: that sets the number's least, below) or a sum of several, which is a relation
//! ([`RELATIONS`]: the two feet of a side are apart by `body_width - 2.3 leg_girth`). Parts that
//! join overlap (a shin's top 0.05 of the leg inside its thigh, its bottom 0.03 of it inside its
//! foot, the tail a twentieth of the trunk inside it) rather than touch.
//!
//! **The ranges are where the geometry breaks, found by building at them** (the test
//! `the_box_creature_builds_whole_at_its_limits`). The least of each number: its smallest fixed
//! difference is three of the recipe's 0.1 mm steps (a written position can be off by 0.1 mm at
//! each of two faces), and its relations can be kept inside the others' ranges (the leg's girth
//! at its least fits the trunk's width at its least). So a leg's girth is at least 3 mm (0.1 of
//! it parts a thigh's side from the trunk's), a leg 10 mm (0.03 of it is the shin inside the
//! foot), a head's depth 4 mm (0.075 of it parts an eye's side from the head's), a trunk's width
//! 8 mm (2.3 girths and the margin, so a girth at its least still fits). The most: 60 m a
//! number, so every position stays under 256 m, where an `f32` still holds the recipe's 0.05 mm
//! grid three times over. **The size has no limit of its own**: it enters the body only through
//! the numbers, so a kind is refused for its size only where a number it drives leaves that
//! number's range, and the refusal names the number (a mouse of 3 cm whose numbers are its own is
//! whole; at the plan's default shares a size under 0.039 m takes the girth under its 3 mm). Its
//! range is only what the arithmetic needs: from one step (a size written as 0 is no size) to
//! 10 km.

use crate::draw::Specimen;
use crate::error::Error;
use crate::plan::{MotionDef, NumberDef, OptionDef, Plan, RelationDef, RoleDef, SizeDef, TraitDef, Unit};
use crate::write::{Body, Writing};
use specula_core::Json;

pub struct BoxCreature;

/// The plan.
pub static PLAN: BoxCreature = BoxCreature;

static SIZE: SizeDef = SizeDef { name: "shoulder_height", unit: Unit::Metre, mean: 0.6, sd: 0.05, range: [0.0001, 10_000.0], about: "ground to the top of the back" };

/// The most any length of the plan may be, metres.
const MOST: f64 = 60.0;

static NUMBERS: [NumberDef; 9] = [
    NumberDef { name: "body_length", unit: Unit::Metre, share: 1.2, exponent: 1.0, traits: &[], range: [0.011, MOST], about: "the trunk, front to back" },
    NumberDef { name: "body_depth", unit: Unit::Metre, share: 0.42, exponent: 1.0, traits: &[("build", 1.0)], range: [0.004, MOST], about: "the trunk, top to bottom" },
    NumberDef { name: "body_width", unit: Unit::Metre, share: 0.32, exponent: 1.0, traits: &[("build", 1.0)], range: [0.008, MOST], about: "the trunk, side to side" },
    NumberDef { name: "leg_length", unit: Unit::Metre, share: 0.62, exponent: 1.0, traits: &[("legs", 1.0)], range: [0.01, MOST], about: "hip to ground" },
    NumberDef { name: "leg_girth", unit: Unit::Metre, share: 0.09, exponent: 1.0, traits: &[("build", 1.0)], range: [0.003, MOST], about: "a thigh's width" },
    NumberDef { name: "neck_length", unit: Unit::Metre, share: 0.3, exponent: 1.0, traits: &[], range: [0.003, MOST], about: "the neck, upright" },
    NumberDef { name: "head_length", unit: Unit::Metre, share: 0.36, exponent: 1.0, traits: &[], range: [0.005, MOST], about: "the head, back to front" },
    NumberDef { name: "head_depth", unit: Unit::Metre, share: 0.2, exponent: 1.0, traits: &[("build", 0.5)], range: [0.004, MOST], about: "the head, top to bottom" },
    NumberDef { name: "tail_length", unit: Unit::Metre, share: 0.5, exponent: 1.0, traits: &[("tail", 1.0)], range: [0.003, MOST], about: "the tail" },
];

/// Three of the recipe's steps: what every relation keeps between two faces.
const GAP: f64 = 0.0003;

/// The relations its geometry needs: each the difference of two faces of two parts that would
/// otherwise cross or meet (see the module's notes). Listed so that a yield helps the relations
/// after it: the trunk's width down first, then the head's depth, the trunk's depth, the neck
/// up, the legs' girth down last.
static RELATIONS: [RelationDef; 8] = [
    RelationDef { name: "tail_behind_neck", terms: &[("body_length", 0.95), ("body_width", -0.5)], margin: GAP, yields: "body_width", about: "the neck's back (half a trunk's width behind its front) stays ahead of the tail's root (a twentieth of the trunk inside its back)" },
    RelationDef { name: "snout_past_neck", terms: &[("head_length", 1.0), ("body_width", -0.5)], margin: GAP, yields: "body_width", about: "the head (from 0.1 of a trunk's width behind the neck) reaches past the neck's front" },
    RelationDef { name: "eyes_apart", terms: &[("body_width", 0.55), ("head_depth", -0.15)], margin: GAP, yields: "head_depth", about: "the two eyes (0.15 of the head's depth, on its sides) do not meet" },
    RelationDef { name: "eyes_inside_head", terms: &[("head_length", 0.3), ("head_depth", -0.075)], margin: GAP, yields: "head_depth", about: "an eye (0.2 of the head's length ahead of its middle) stays behind the head's front" },
    RelationDef { name: "belly_above_knees", terms: &[("leg_length", 0.45), ("body_depth", -0.1)], margin: GAP, yields: "body_depth", about: "the trunk's underside (0.1 of its depth below the hips) stays above the shins' tops" },
    RelationDef { name: "head_above_back", terms: &[("neck_length", 1.0), ("head_depth", -0.2), ("body_depth", -0.25)], margin: GAP, yields: "neck_length", about: "the head (its underside 0.2 of its depth below the neck's top) clears the back (the neck rises from 0.75 of the trunk's depth)" },
    RelationDef { name: "legs_apart_sideways", terms: &[("body_width", 1.0), ("leg_girth", -2.3)], margin: GAP, yields: "leg_girth", about: "the left and right feet (1.1 girths wide, under thighs 0.1 girth inside the trunk's sides) do not meet: each leg under its own side" },
    RelationDef { name: "legs_apart_lengthways", terms: &[("body_length", 1.0), ("leg_girth", -3.3)], margin: GAP, yields: "leg_girth", about: "a front foot and the hind foot behind it (1.3 girths long, set a girth inside the trunk's ends) do not meet: each leg under its own end" },
];

static TRAITS: [TraitDef; 3] = [
    TraitDef { name: "build", spread: 0.05, about: "every girth together" },
    TraitDef { name: "legs", spread: 0.03, about: "the legs' length" },
    TraitDef { name: "tail", spread: 0.05, about: "the tail's length" },
];

static ROLES: [RoleDef; 3] = [
    RoleDef { name: "coat", hint: "#8a7a60", about: "the body, neck, head, tail and legs" },
    RoleDef { name: "claw", hint: "#2a2420", about: "the feet" },
    RoleDef { name: "eye", hint: "#c8a040", about: "the eyes" },
];

static OPTIONS: [OptionDef; 1] = [OptionDef { name: "tail_carriage", values: &["low", "high"], default: "low", about: "where the tail leaves the back" }];

static MOTIONS: [MotionDef; 3] = [
    MotionDef { name: "walk", generator: "gait", patterns: &["lateral_walk"], presets: &[], froude: true, about: "a walk by footfall pattern and Froude number" },
    MotionDef { name: "trot", generator: "gait", patterns: &["trot"], presets: &[], froude: true, about: "a trot by footfall pattern and Froude number" },
    MotionDef { name: "idle", generator: "idle", patterns: &[], presets: &[], froude: false, about: "standing, breathing" },
];

/// Each leg: its key in a footfall pattern, its side (+1 left) and end (+1 front).
const LEGS: [(&str, f64, f64); 4] = [("lh", 1.0, -1.0), ("lf", 1.0, 1.0), ("rh", -1.0, -1.0), ("rf", -1.0, 1.0)];

impl Plan for BoxCreature {
    fn name(&self) -> &'static str {
        "box_creature"
    }
    fn about(&self) -> &'static str {
        "a creature of boxes on four legs (a test plan: not an animal)"
    }
    fn size(&self) -> &'static SizeDef {
        &SIZE
    }
    fn numbers(&self) -> &'static [NumberDef] {
        &NUMBERS
    }
    fn traits(&self) -> &'static [TraitDef] {
        &TRAITS
    }
    fn roles(&self) -> &'static [RoleDef] {
        &ROLES
    }
    fn options(&self) -> &'static [OptionDef] {
        &OPTIONS
    }
    fn motions(&self) -> &'static [MotionDef] {
        &MOTIONS
    }
    fn relations(&self) -> &'static [RelationDef] {
        &RELATIONS
    }

    fn write(&self, s: &Specimen, w: &mut Writing) -> Result<(), Error> {
        let n = |k: &str| s.get(k);
        let (l, bl, bd, bw, g) = (n("leg_length"), n("body_length"), n("body_depth"), n("body_width"), n("leg_girth"));
        let (nl, hl, hd, tl) = (n("neck_length"), n("head_length"), n("head_depth"), n("tail_length"));
        let bottom = l - 0.1 * bd;
        let top = bottom + bd;
        w.bone("root", None, [0.0; 3], None);
        w.bone("body", Some("root"), [0.0, l, 0.0], Some([0.0, l, bl / 2.0]));
        w.block("body", "coat", "body", [bw, bd, bl], [0.0, bottom, 0.0])?;
        // A thigh's outer side 0.1 girth inside the trunk's; a girth inside its ends.
        let (lx, lz) = (bw / 2.0 - 0.6 * g, bl / 2.0 - g);
        for (key, side, end) in LEGS {
            let (x, z) = (side * lx, end * lz);
            let (thigh, shin) = (format!("thigh_{key}"), format!("shin_{key}"));
            w.bone(&thigh, Some("body"), [x, l, z], None);
            w.bone(&shin, Some(&thigh), [x, l / 2.0, z], Some([x, 0.0, z]));
            w.block(&thigh, "coat", &thigh, [g, l / 2.0, g], [x, l / 2.0, z])?;
            // (The shin from inside its foot to inside its thigh: joined parts overlap, never touch.)
            w.block(&shin, "coat", &shin, [0.8 * g, 0.52 * l, 0.8 * g], [x, 0.03 * l, z])?;
            w.block(&format!("foot_{key}"), "claw", &shin, [1.1 * g, 0.06 * l, 1.3 * g], [x, 0.0, z + 0.15 * g])?;
        }
        // The neck rises from the front of the back; the head looks forward from its top, its
        // back 0.1 of the trunk's width behind the neck's.
        let (nb, nz) = (bottom + 0.75 * bd, bl / 2.0 - 0.3 * bw);
        let neck_top = nb + nl;
        w.bone("neck", Some("body"), [0.0, nb, nz], Some([0.0, neck_top, nz]));
        w.block("neck", "coat", "neck", [0.4 * bw, nl, 0.4 * bw], [0.0, nb, nz])?;
        let head_z = nz - 0.3 * bw + 0.5 * hl;
        w.bone("head", Some("neck"), [0.0, neck_top, nz], Some([0.0, neck_top, nz + hl]));
        w.block("head", "coat", "head", [0.55 * bw, hd, hl], [0.0, neck_top - 0.2 * hd, head_z])?;
        let e = 0.15 * hd;
        for (name, side) in [("eye_l", 1.0), ("eye_r", -1.0)] {
            w.block(name, "eye", "head", [e, e, e], [side * 0.275 * bw, neck_top + 0.35 * hd, head_z + 0.2 * hl])?;
        }
        // The tail: a twentieth of the trunk inside its back, 0.15 of its depth high.
        let ty = bottom + if s.option("tail_carriage") == "high" { 0.8 } else { 0.6 } * bd;
        w.bone("tail", Some("body"), [0.0, ty, -bl / 2.0], Some([0.0, ty, -bl / 2.0 - tl]));
        w.block("tail", "coat", "tail", [0.15 * bw, 0.15 * bd, tl + 0.05 * bl], [0.0, ty - 0.075 * bd, -bl / 2.0 - tl / 2.0 + 0.025 * bl])?;
        w.fact("withers_built", top, "m");
        w.fact("hip_height", l, "m");
        // The clips the kind lists, in the plan's order.
        let legs: Vec<(String, String, String)> = LEGS.iter().map(|(k, _, _)| (k.to_string(), format!("thigh_{k}"), format!("shin_{k}"))).collect();
        let legs: Vec<(&str, &str, &str)> = legs.iter().map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str())).collect();
        for def in &MOTIONS {
            let Some(m) = w.motion(def.name) else { continue };
            match def.generator {
                "gait" => {
                    let pattern = m.pattern.clone().unwrap_or_else(|| def.patterns[0].to_string());
                    let froude = m.froude.unwrap_or(if def.name == "walk" { 0.25 } else { 1.0 });
                    w.gait(def.name, &pattern, froude, l, &Body { pelvis: "body", head: Some("head"), legs: &legs })?;
                }
                _ => {
                    let legs_json = legs.iter().map(|(_, t, sh)| Json::obj([("thigh", Json::from(*t)), ("shin", Json::from(*sh))]));
                    w.clip(Json::obj([
                        ("name", Json::from(def.name)),
                        ("kind", Json::from("idle")),
                        ("duration", Json::from(2.0)),
                        ("pelvis", Json::from("body")),
                        ("head", Json::from("head")),
                        ("legs", Json::arr(legs_json)),
                    ]));
                }
            }
        }
        Ok(())
    }
}

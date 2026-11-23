//! `box_creature`: a creature of boxes on four legs, the test plan that goes the library's whole
//! path (kind, seed, specimen, recipe built by the modeller, clips, catalogue). It is not an
//! animal and claims nothing measured: every default here is the engine's own.
//!
//! Its rig: `root`; `body` (the pelvis of the gaits); `neck`, `head`, `tail`; each leg a `thigh_X`
//! and a `shin_X` for `X` in `lf`, `rf`, `lh`, `rh` (left = +X, front = +Z).

use crate::draw::Specimen;
use crate::error::Error;
use crate::plan::{MotionDef, NumberDef, OptionDef, Plan, RoleDef, SizeDef, TraitDef, Unit};
use crate::write::{Body, Writing};
use specula_core::Json;

pub struct BoxCreature;

/// The plan.
pub static PLAN: BoxCreature = BoxCreature;

static SIZE: SizeDef = SizeDef { name: "shoulder_height", unit: Unit::Metre, mean: 0.6, sd: 0.05, range: [0.3, 1.2], about: "ground to the top of the back" };

static NUMBERS: [NumberDef; 9] = [
    NumberDef { name: "body_length", unit: Unit::Metre, share: 1.2, exponent: 1.0, traits: &[], about: "the trunk, front to back" },
    NumberDef { name: "body_depth", unit: Unit::Metre, share: 0.42, exponent: 1.0, traits: &[("build", 1.0)], about: "the trunk, top to bottom" },
    NumberDef { name: "body_width", unit: Unit::Metre, share: 0.32, exponent: 1.0, traits: &[("build", 1.0)], about: "the trunk, side to side" },
    NumberDef { name: "leg_length", unit: Unit::Metre, share: 0.62, exponent: 1.0, traits: &[("legs", 1.0)], about: "hip to ground" },
    NumberDef { name: "leg_girth", unit: Unit::Metre, share: 0.09, exponent: 1.0, traits: &[("build", 1.0)], about: "a thigh's width" },
    NumberDef { name: "neck_length", unit: Unit::Metre, share: 0.3, exponent: 1.0, traits: &[], about: "the neck, upright" },
    NumberDef { name: "head_length", unit: Unit::Metre, share: 0.36, exponent: 1.0, traits: &[], about: "the head, back to front" },
    NumberDef { name: "head_depth", unit: Unit::Metre, share: 0.2, exponent: 1.0, traits: &[("build", 0.5)], about: "the head, top to bottom" },
    NumberDef { name: "tail_length", unit: Unit::Metre, share: 0.5, exponent: 1.0, traits: &[("tail", 1.0)], about: "the tail" },
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

    fn write(&self, s: &Specimen, w: &mut Writing) -> Result<(), Error> {
        let n = |k: &str| s.get(k);
        let (l, bl, bd, bw, g) = (n("leg_length"), n("body_length"), n("body_depth"), n("body_width"), n("leg_girth"));
        let (nl, hl, hd, tl) = (n("neck_length"), n("head_length"), n("head_depth"), n("tail_length"));
        let bottom = l - 0.1 * bd;
        let top = bottom + bd;
        w.bone("root", None, [0.0; 3], None);
        w.bone("body", Some("root"), [0.0, l, 0.0], Some([0.0, l, bl / 2.0]));
        w.block("body", "coat", "body", [bw, bd, bl], [0.0, bottom, 0.0])?;
        let (lx, lz) = (bw / 2.0 - g / 2.0, bl / 2.0 - g);
        for (key, side, end) in LEGS {
            let (x, z) = (side * lx, end * lz);
            let (thigh, shin) = (format!("thigh_{key}"), format!("shin_{key}"));
            w.bone(&thigh, Some("body"), [x, l, z], None);
            w.bone(&shin, Some(&thigh), [x, l / 2.0, z], Some([x, 0.0, z]));
            w.block(&thigh, "coat", &thigh, [g, l / 2.0, g], [x, l / 2.0, z])?;
            w.block(&shin, "coat", &shin, [0.8 * g, l / 2.0, 0.8 * g], [x, 0.0, z])?;
            w.block(&format!("foot_{key}"), "claw", &shin, [1.1 * g, 0.06 * l, 1.3 * g], [x, 0.0, z + 0.15 * g])?;
        }
        // The neck rises from the front of the back; the head looks forward from its top.
        let (nb, nz) = (bottom + 0.75 * bd, bl / 2.0 - 0.3 * bw);
        let neck_top = nb + nl;
        w.bone("neck", Some("body"), [0.0, nb, nz], Some([0.0, neck_top, nz]));
        w.block("neck", "coat", "neck", [0.4 * bw, nl, 0.4 * bw], [0.0, nb, nz])?;
        let head_z = nz + 0.5 * hl - 0.2 * bw;
        w.bone("head", Some("neck"), [0.0, neck_top, nz], Some([0.0, neck_top, nz + hl]));
        w.block("head", "coat", "head", [0.55 * bw, hd, hl], [0.0, neck_top - 0.5 * hd, head_z])?;
        let e = 0.15 * hd;
        for (name, side) in [("eye_l", 1.0), ("eye_r", -1.0)] {
            w.block(name, "eye", "head", [e, e, e], [side * 0.275 * bw, neck_top - 0.5 * hd + 0.55 * hd, head_z + 0.5 * hl - 0.3 * hl])?;
        }
        let ty = bottom + if s.option("tail_carriage") == "high" { 0.95 } else { 0.75 } * bd;
        w.bone("tail", Some("body"), [0.0, ty, -bl / 2.0], Some([0.0, ty, -bl / 2.0 - tl]));
        w.block("tail", "coat", "tail", [0.15 * bw, 0.15 * bw, tl], [0.0, ty - 0.075 * bw, -bl / 2.0 - tl / 2.0])?;
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

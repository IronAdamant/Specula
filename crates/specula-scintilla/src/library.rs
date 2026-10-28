//! The built-in effects: a starting set agents can play as they are, tint with `params`, or copy
//! (`vfx_get`) and change into the world's own (`vfx_set`). Fire keeps fire's colours; the
//! others are pale so a tint gives them a colour family (cyan magic, green dead, ...).

use crate::effect::Effect;
use specula_core::Json;
use std::sync::{Arc, OnceLock};

/// `(name, JSON)` of every built-in effect, in listing order.
pub const BUILTIN: &[(&str, &str)] = &[
    (
        "fire",
        r##"{"description": "A brazier's flames: a flipbook of tongues rising from a small bowl, a hot core and a flickering warm light. Fire's own colours (tint dims or warms them).",
  "emitters": [
    {"name": "flames", "rate": 26, "lifetime": [0.55, 0.9], "shape": {"kind": "disc", "radius": 0.16},
     "direction": [0, 1, 0], "spread_deg": 10, "speed": [0.35, 0.7], "gravity": [0, 1.4, 0], "drag": 1.2,
     "noise": {"strength": 0.22, "frequency": 1.6, "speed": 1.2},
     "size": [[0, 0.32], [0.35, 0.42], [1, 0.12]], "size_random": [0.8, 1.15],
     "color": [[0, "#ffd27a"], [0.3, "#ff7a1f"], [1, "#c2410c"]], "alpha": [[0, 0], [0.12, 1], [0.7, 0.8], [1, 0]],
     "emissive": 1.7, "rotation_deg": [-12, 12], "texture": "flame", "flipbook": {"cols": 4, "rows": 4, "fps": 18, "random_start": true},
     "blend": "additive", "soft": 0.12, "priority": 3},
    {"name": "core", "rate": 10, "lifetime": [0.3, 0.5], "shape": {"kind": "disc", "radius": 0.08}, "speed": [0.1, 0.3],
     "gravity": [0, 0.8, 0], "size": [[0, 0.22], [1, 0.1]], "color": "#ffd27a", "alpha": [[0, 0], [0.2, 0.8], [1, 0]],
     "emissive": 2.0, "texture": "soft", "blend": "additive", "soft": 0.08, "priority": 3}
  ],
  "light": {"color": "#ff7a1f", "intensity": 2.2, "range": 4.5, "offset": [0, 0.35, 0], "flicker": 0.22, "flicker_hz": 2.6}}"##,
    ),
    (
        "embers",
        r##"{"description": "Embers: a few hot sparks drifting up and wandering on the heat, dying orange.",
  "emitters": [
    {"name": "embers", "rate": 5, "lifetime": [1.4, 2.6], "shape": {"kind": "disc", "radius": 0.2}, "spread_deg": 25,
     "speed": [0.5, 1.1], "gravity": [0, 0.35, 0], "drag": 0.6, "noise": {"strength": 0.45, "frequency": 0.9, "speed": 0.7},
     "size": [[0, 0.05], [1, 0.025]], "color": [[0, "#ffd27a"], [0.5, "#ff7a1f"], [1, "#c2410c"]],
     "alpha": [[0, 0], [0.08, 1], [0.8, 0.8], [1, 0]], "emissive": 4.5, "texture": "spark", "stretch": 0.05, "blend": "additive", "priority": 2}
  ]}"##,
    ),
    (
        "smoke",
        r##"{"description": "Slow lit smoke rising and spreading; dark and cool, so it reads against the night.",
  "emitters": [
    {"name": "smoke", "rate": 4, "lifetime": [3, 4.5], "shape": {"kind": "disc", "radius": 0.15}, "spread_deg": 12,
     "speed": [0.25, 0.5], "gravity": [0, 0.12, 0], "drag": 0.4, "noise": {"strength": 0.25, "frequency": 0.6, "speed": 0.3},
     "size": [[0, 0.35], [1, 1.5]], "color": [[0, "#4a515c"], [1, "#2a3448"]], "alpha": [[0, 0], [0.15, 0.32], [1, 0]],
     "emissive": 1, "rotation_deg": [0, 360], "spin_deg": [-20, 20], "texture": "smoke",
     "flipbook": {"cols": 4, "rows": 4, "over_life": true}, "blend": "alpha", "soft": 0.5, "lit": true, "priority": 0.5}
  ]}"##,
    ),
    (
        "motes",
        r##"{"description": "Glowing motes and drifting glyphs around a carved stone (pale: tint it with its family's colour).",
  "emitters": [
    {"name": "motes", "rate": 7, "lifetime": [1.8, 3], "shape": {"kind": "sphere", "radius": 0.55, "surface": true},
     "spread_deg": 30, "speed": [0.05, 0.2], "gravity": [0, 0.15, 0], "noise": {"strength": 0.18, "frequency": 1.1, "speed": 0.5},
     "size": [[0, 0.03], [0.3, 0.07], [1, 0.02]], "color": [[0, "#ffffff"], [1, "#a8b8c0"]], "alpha": [[0, 0], [0.2, 1], [1, 0]],
     "emissive": 3, "texture": "soft", "blend": "additive", "priority": 1},
    {"name": "glyphs", "rate": 1.2, "lifetime": [2, 2.8], "shape": {"kind": "disc", "radius": 0.4}, "speed": [0.12, 0.25],
     "size": [[0, 0.1], [0.5, 0.16], [1, 0.12]], "color": "#ffffff", "alpha": [[0, 0], [0.25, 0.85], [1, 0]], "emissive": 2.4,
     "spin_deg": [-30, 30], "texture": "rune", "blend": "additive", "priority": 1}
  ]}"##,
    ),
    (
        "trail",
        r##"{"description": "A ghostly trail left behind something moving (world space; pale: tint it).",
  "emitters": [
    {"name": "wisps", "rate": 34, "lifetime": [0.7, 1.2], "shape": {"kind": "sphere", "radius": 0.25}, "speed": [0, 0.15],
     "space": "world", "noise": {"strength": 0.3, "frequency": 1.4, "speed": 0.8}, "drag": 1,
     "size": [[0, 0.4], [1, 0.08]], "color": [[0, "#ffffff"], [1, "#9aa4a8"]], "alpha": [[0, 0.55], [1, 0]],
     "emissive": 1.6, "rotation_deg": [0, 360], "texture": "smoke", "flipbook": {"cols": 4, "rows": 4, "over_life": true},
     "blend": 0.7, "soft": 0.2, "priority": 2}
  ]}"##,
    ),
    (
        "dust_puff",
        r##"{"description": "A one-shot puff of dust and grit kicked up where something falls (lit, bone-grey).",
  "emitters": [
    {"name": "puff", "bursts": [{"time": 0, "count": 16}], "lifetime": [0.7, 1.3], "shape": {"kind": "disc", "radius": 0.35},
     "direction": [0, 1, 0], "spread_deg": 75, "speed": [0.5, 1.3], "gravity": [0, -0.4, 0], "drag": 3,
     "size": [[0, 0.3], [1, 0.9]], "size_random": [0.8, 1.2], "color": [[0, "#9c937e"], [1, "#5b6470"]], "alpha": [[0, 0.55], [1, 0]],
     "emissive": 1, "rotation_deg": [0, 360], "spin_deg": [-40, 40], "texture": "dust", "blend": "alpha", "soft": 0.25, "lit": true, "priority": 1.5},
    {"name": "grit", "bursts": [{"time": 0, "count": 12}], "lifetime": [0.4, 0.8], "shape": {"kind": "disc", "radius": 0.25},
     "spread_deg": 55, "speed": [1.2, 2.4], "gravity": [0, -9.8, 0], "size": 0.04, "color": "#7d7564", "alpha": [[0, 1], [1, 0.6]],
     "emissive": 1, "texture": "soft", "blend": "alpha", "lit": true, "collide": {"plane": 0, "mode": "kill"}, "priority": 1.5}
  ]}"##,
    ),
    (
        "impact",
        r##"{"description": "A spell's impact: a burst of streaking sparks, an expanding ring and a short flash of light (pale: tint it).",
  "emitters": [
    {"name": "sparks", "bursts": [{"time": 0, "count": 26}], "lifetime": [0.25, 0.5], "shape": {"kind": "sphere", "radius": 0.1},
     "direction": "radial", "speed": [2.2, 4.2], "drag": 4.5, "gravity": [0, -2, 0], "size": [[0, 0.07], [1, 0.03]],
     "color": [[0, "#ffffff"], [1, "#c8d8e0"]], "alpha": [[0, 1], [1, 0]], "emissive": 4, "texture": "spark", "stretch": 0.05,
     "blend": "additive", "priority": 4},
    {"name": "ring", "bursts": [{"time": 0, "count": 1}], "lifetime": 0.32, "size": [[0, 0.3], [1, 2.2]], "color": "#ffffff",
     "alpha": [[0, 1], [1, 0]], "emissive": 3, "texture": "ring", "blend": "additive", "priority": 4},
    {"name": "glow", "bursts": [{"time": 0, "count": 1}], "lifetime": 0.22, "size": [[0, 1.2], [1, 0.4]], "color": "#ffffff",
     "alpha": [[0, 0.8], [1, 0]], "emissive": 2, "texture": "soft", "blend": "additive", "soft": 0.3, "priority": 4}
  ],
  "light": {"color": "#ffffff", "intensity": 3, "range": 4, "offset": [0, 0.3, 0], "duration": 0.35}}"##,
    ),
    (
        "sparks",
        r##"{"description": "Metal sparks: fast streaks thrown up, falling and dying on the ground.",
  "emitters": [
    {"name": "sparks", "rate": 18, "lifetime": [0.5, 1.1], "spread_deg": 40, "speed": [2, 4], "gravity": [0, -9.8, 0], "drag": 0.8,
     "size": 0.04, "color": [[0, "#fff2c8"], [1, "#ff7a1f"]], "alpha": [[0, 1], [1, 0]], "emissive": 5, "texture": "spark",
     "stretch": 0.04, "blend": "additive", "collide": {"ground": true, "mode": "kill"}, "priority": 1}
  ]}"##,
    ),
];

/// The built-in effects, parsed once.
pub fn builtins() -> &'static [(&'static str, Arc<Effect>)] {
    static PARSED: OnceLock<Vec<(&'static str, Arc<Effect>)>> = OnceLock::new();
    PARSED.get_or_init(|| {
        BUILTIN
            .iter()
            .map(|(name, text)| {
                let json = Json::parse(text).unwrap_or_else(|e| panic!("built-in effect {name}: {e}"));
                let effect = Effect::parse(&json).unwrap_or_else(|e| panic!("built-in effect {name}: {e}"));
                (*name, Arc::new(effect))
            })
            .collect()
    })
}

/// A built-in effect by name.
pub fn builtin(name: &str) -> Option<&'static Arc<Effect>> {
    builtins().iter().find(|(n, _)| *n == name).map(|(_, e)| e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{check, Limits};
    use crate::component::Params;

    #[test]
    fn every_builtin_parses_round_trips_and_passes_its_checks() {
        for (name, e) in builtins() {
            let back = Effect::parse(&e.to_json()).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert_eq!(&back, e.as_ref(), "{name}");
            let report = check(e, &Params::default(), &Limits::default(), None);
            let failed: Vec<_> = report.iter().filter(|c| !c.ok && c.severity == "error").collect();
            assert!(failed.is_empty(), "{name}: {failed:?}");
        }
        assert!(builtin("fire").is_some() && builtin("nope").is_none());
    }
}

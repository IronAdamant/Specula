//! The `water` component (docs/design/TERRAIN.md, "Stylised water"): a water surface on the
//! entity's local XZ plane, centred on its origin, `size` × its scale. Cosmetic world data like a
//! decal (saved, journaled, hashed, claimed; never read by the simulation), drawn by the GPU's
//! water pass ([`specula_gpu::Material::Water`] on the shared quad).

use specula_core::hash::StableHasher;
use specula_core::Json;
use specula_gpu::{WaterPattern, WaterStyle, WATER_PRESETS};

/// A water surface: a preset and the look parameters set over it.
#[derive(Clone, Debug, PartialEq)]
pub struct WaterSpec {
    pub preset: String,
    /// The surface in the entity's local metres (X, Z).
    pub size: [f32; 2],
    /// The preset with the overrides applied.
    pub style: WaterStyle,
}

const KEYS: [&str; 13] = [
    "preset",
    "size",
    "deep",
    "shallow",
    "foam",
    "opacity",
    "clarity",
    "foam_width",
    "waves",
    "roughness",
    "reflect",
    "emissive",
    "pattern",
];
const WAVE_KEYS: [&str; 3] = ["scale", "speed", "strength"];

fn color_of(v: &Json, field: &str) -> Result<[f32; 3], String> {
    let s = v.as_str().ok_or_else(|| format!("'water.{field}' must be a colour such as \"#13303f\""))?;
    crate::world::color_from_hex(s).ok_or_else(|| format!("'water.{field}': '{s}' is not a colour (\"#rrggbb\" or \"#rgb\")"))
}

fn num_of(v: &Json, field: &str, lo: f32, hi: f32) -> Result<f32, String> {
    match v.as_f64() {
        Some(x) if x.is_finite() && (lo as f64..=hi as f64).contains(&x) => Ok(x as f32),
        _ => Err(format!("'water.{field}' must be a number {lo}..{hi} (got {})", v)),
    }
}

fn hex(c: [f32; 3]) -> Json {
    Json::from(crate::world::color_hex(c))
}

impl WaterSpec {
    /// `"lake"` (a preset's name) or `{preset?, size?, deep?, shallow?, foam?, opacity?, clarity?,
    /// foam_width?, waves? {scale, speed, strength}, roughness?, reflect?, emissive?, pattern?}`.
    pub fn parse(v: &Json) -> Result<WaterSpec, String> {
        let presets = WATER_PRESETS.join(", ");
        if let Some(name) = v.as_str() {
            let style = WaterStyle::preset(name).ok_or_else(|| format!("'water': unknown preset '{name}' (allowed: {presets})"))?;
            return Ok(WaterSpec { preset: name.to_string(), size: [1.0, 1.0], style });
        }
        let Json::Obj(fields) = v else {
            return Err(format!("'water' must be a preset name ({presets}) or an object {{preset, size, deep, shallow, foam, clarity, ...}}; false removes it"));
        };
        for (k, _) in fields {
            if !KEYS.contains(&k.as_str()) {
                return Err(format!("'water.{k}' is not a water field (allowed: {})", KEYS.join(", ")));
            }
        }
        let preset = match v.get("preset") {
            None | Some(Json::Null) => "lake".to_string(),
            Some(p) => p.as_str().filter(|n| WATER_PRESETS.contains(n)).map(str::to_string).ok_or_else(|| format!("'water.preset' must be one of {presets}"))?,
        };
        let mut s = WaterStyle::preset(&preset).expect("checked");
        let mut size = [1.0, 1.0];
        if let Some(z) = v.get("size") {
            match z.as_array() {
                Some([x, y]) => size = [num_of(x, "size", 0.01, 10_000.0)?, num_of(y, "size", 0.01, 10_000.0)?],
                _ => return Err("'water.size' must be [x, z] metres (0.01..10000 each)".into()),
            }
        }
        if let Some(c) = v.get("deep") {
            s.deep = color_of(c, "deep")?;
        }
        if let Some(c) = v.get("shallow") {
            s.shallow = color_of(c, "shallow")?;
        }
        if let Some(c) = v.get("foam") {
            s.foam = color_of(c, "foam")?;
        }
        let num = |key: &str, slot: &mut f32, lo: f32, hi: f32| -> Result<(), String> {
            if let Some(x) = v.get(key) {
                *slot = num_of(x, key, lo, hi)?;
            }
            Ok(())
        };
        num("opacity", &mut s.opacity, 0.0, 1.0)?;
        num("clarity", &mut s.clarity, 0.01, 1000.0)?;
        num("foam_width", &mut s.foam_width, 0.0, 100.0)?;
        num("roughness", &mut s.roughness, 0.02, 1.0)?;
        num("reflect", &mut s.reflect, 0.0, 1.0)?;
        num("emissive", &mut s.emissive, 0.0, 100.0)?;
        if let Some(w) = v.get("waves") {
            let Json::Obj(wf) = w else { return Err("'water.waves' must be {scale, speed, strength}".into()) };
            if let Some((k, _)) = wf.iter().find(|(k, _)| !WAVE_KEYS.contains(&k.as_str())) {
                return Err(format!("'water.waves.{k}' is not a wave field (allowed: scale, speed, strength)"));
            }
            if let Some(x) = w.get("scale") {
                s.wave_scale = num_of(x, "waves.scale", 0.05, 1000.0)?;
            }
            if let Some(x) = w.get("speed") {
                s.wave_speed = num_of(x, "waves.speed", 0.0, 50.0)?;
            }
            if let Some(x) = w.get("strength") {
                s.wave_strength = num_of(x, "waves.strength", 0.0, 1.0)?;
            }
        }
        if let Some(p) = v.get("pattern") {
            s.pattern = p.as_str().and_then(WaterPattern::parse).ok_or_else(|| "'water.pattern' must be waves, ice or lava".to_string())?;
        }
        Ok(WaterSpec { preset, size, style: s })
    }

    /// The preset and what differs from it (so listings stay short); parses back to itself.
    pub fn to_json(&self) -> Json {
        let p = WaterStyle::preset(&self.preset).unwrap_or_default();
        let s = &self.style;
        let mut f = vec![("preset", Json::from(self.preset.as_str()))];
        if self.size != [1.0, 1.0] {
            f.push(("size", Json::arr(self.size.map(Json::from))));
        }
        for (k, a, b) in [("deep", s.deep, p.deep), ("shallow", s.shallow, p.shallow), ("foam", s.foam, p.foam)] {
            if a != b {
                f.push((k, hex(a)));
            }
        }
        for (k, a, b) in [
            ("opacity", s.opacity, p.opacity),
            ("clarity", s.clarity, p.clarity),
            ("foam_width", s.foam_width, p.foam_width),
            ("roughness", s.roughness, p.roughness),
            ("reflect", s.reflect, p.reflect),
            ("emissive", s.emissive, p.emissive),
        ] {
            if a != b {
                f.push((k, Json::from(a)));
            }
        }
        if (s.wave_scale, s.wave_speed, s.wave_strength) != (p.wave_scale, p.wave_speed, p.wave_strength) {
            f.push(("waves", Json::obj([("scale", Json::from(s.wave_scale)), ("speed", Json::from(s.wave_speed)), ("strength", Json::from(s.wave_strength))])));
        }
        if s.pattern != p.pattern {
            f.push(("pattern", Json::from(s.pattern.name())));
        }
        Json::obj(f)
    }

    pub fn hash_into(&self, h: &mut StableHasher) {
        h.write_str("water");
        h.write_str(&self.preset);
        let s = &self.style;
        for v in [self.size[0], self.size[1], s.opacity, s.clarity, s.foam_width, s.wave_scale, s.wave_speed, s.wave_strength, s.roughness, s.reflect, s.emissive] {
            h.write_f32(v);
        }
        for c in [s.deep, s.shallow, s.foam] {
            c.iter().for_each(|x| h.write_f32(*x));
        }
        h.write_u8(s.pattern as u8);
    }

    /// The surface's local box (centre, half extents): flat, 1 cm thick.
    pub fn local_box(&self) -> ([f32; 3], [f32; 3]) {
        ([0.0; 3], [self.size[0] * 0.5, 0.005, self.size[1] * 0.5])
    }
}

/// The `water` argument of `world_spawn` / `world_set`: absent (no change), `false` (remove) or
/// a spec.
pub fn water_arg(args: &Json) -> Result<Option<Option<WaterSpec>>, String> {
    match args.get("water") {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Bool(false)) => Ok(Some(None)),
        Some(v) => WaterSpec::parse(v).map(|s| Some(Some(s))),
    }
}

/// The tool schema's description of `water`.
pub const WATER_DOC: &str = "A water surface on the entity's local XZ plane (centred on it, size × its scale): a preset name \
     (lake, murk: map 1's still dark pools, frozen: map 2's frozen lake, lava: map 3, swamp) or {preset, size [x, z] m (default \
     [1, 1]), deep, shallow, foam (colours), opacity 0..1, clarity (m of water to reach the deep colour), foam_width (m of shore \
     foam), waves {scale m, speed m/s, strength 0..1}, roughness, reflect 0..1, emissive (lava's glow), pattern waves|ice|lava}; \
     anything given overrides the preset. Colour by depth and shore foam come from the scene's depth under the surface; waves are \
     normals animated by world time (captures are reproducible). Drawn after opaque surfaces and decals; it never hides anything \
     from sight (counted like glass). false removes it";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_overrides_and_round_trips() {
        let s = WaterSpec::parse(&Json::from("frozen")).unwrap();
        assert_eq!(s.style.pattern, WaterPattern::Ice);
        assert_eq!(WaterSpec::parse(&s.to_json()).unwrap(), s);
        let v = Json::parse(r##"{"preset":"murk","size":[12,8],"clarity":2.5,"deep":"#102030","waves":{"speed":0.3},"pattern":"waves"}"##).unwrap();
        let s = WaterSpec::parse(&v).unwrap();
        assert_eq!((s.size, s.style.clarity, s.style.wave_speed), ([12.0, 8.0], 2.5, 0.3));
        let back = WaterSpec::parse(&s.to_json()).unwrap();
        assert_eq!(back, s);
        assert!(s.to_json().get("pattern").is_none(), "unchanged from the preset: left out");
    }

    #[test]
    fn mistakes_name_the_field() {
        let bad = |t: &str| WaterSpec::parse(&Json::parse(t).unwrap()).unwrap_err();
        assert!(bad(r#""ocean""#).contains("unknown preset"));
        assert!(bad(r#"{"clarity":-1}"#).contains("water.clarity"));
        assert!(bad(r##"{"colour":"#fff"}"##).contains("water.colour"));
        assert!(bad(r#"{"waves":{"height":1}}"#).contains("water.waves.height"));
        assert!(bad(r#"{"size":[1]}"#).contains("water.size"));
        assert!(bad(r#"{"pattern":"glass"}"#).contains("water.pattern"));
        assert!(bad(r#"{"deep":"not a colour"}"#).contains("water.deep"));
        assert!(water_arg(&Json::parse(r#"{"water":false}"#).unwrap()).unwrap() == Some(None));
    }
}

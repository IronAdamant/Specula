//! The world components: `vfx` (play an effect from the library at an entity) and `decal`
//! (project a texture onto the opaque surfaces in the entity's box). Parsed with structured
//! errors, written canonically, and hashed by exact bits (they are world data, like a light).

use crate::effect::TextureRef;
use crate::error::{read, Path, Result, VfxError};
use specula_core::hash::StableHasher;
use specula_core::Json;

/// Characters of an effect name.
pub const MAX_NAME: usize = 64;
/// World times components may start at (seconds, as animation's).
pub const START_RANGE: std::ops::RangeInclusive<f64> = -1.0e9..=1.0e9;

/// Overrides every effect understands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Multiplies every emitter's rate and burst counts (0..10).
    pub rate: f32,
    /// Multiplies particle sizes (0.01..20).
    pub size: f32,
    /// Multiplies initial speeds (0..10).
    pub speed: f32,
    /// Multiplies alpha (0..4, capped at 1 per particle).
    pub alpha: f32,
    /// Multiplies emissive colour and the light (0..20).
    pub intensity: f32,
    /// sRGB colour multiplied into every particle (and the light).
    pub tint: [f32; 3],
    /// The effect's light, if it has one, is on.
    pub light: bool,
    /// Varies the pattern between instances that would otherwise match.
    pub seed: u32,
}

impl Default for Params {
    fn default() -> Self {
        Params { rate: 1.0, size: 1.0, speed: 1.0, alpha: 1.0, intensity: 1.0, tint: [1.0; 3], light: true, seed: 0 }
    }
}

impl Params {
    pub fn parse(v: &Json, at: &Path) -> Result<Params> {
        read::keys(v, at, &["rate", "size", "speed", "alpha", "intensity", "tint", "light", "seed"])?;
        let d = Params::default();
        Ok(Params {
            rate: read::num_or(v, "rate", at, 0.0, 10.0, d.rate)?,
            size: read::num_or(v, "size", at, 0.01, 20.0, d.size)?,
            speed: read::num_or(v, "speed", at, 0.0, 10.0, d.speed)?,
            alpha: read::num_or(v, "alpha", at, 0.0, 4.0, d.alpha)?,
            intensity: read::num_or(v, "intensity", at, 0.0, 20.0, d.intensity)?,
            tint: match v.get("tint") {
                None | Some(Json::Null) => d.tint,
                Some(t) => read::color(t, &at.key("tint"))?,
            },
            light: read::boolean_or(v, "light", at, true)?,
            seed: read::int_or(v, "seed", at, 0, u64::from(u32::MAX), 0)? as u32,
        })
    }

    pub fn to_json(&self) -> Json {
        let d = Params::default();
        let mut f = Vec::new();
        for (k, v, dv) in [("rate", self.rate, d.rate), ("size", self.size, d.size), ("speed", self.speed, d.speed), ("alpha", self.alpha, d.alpha), ("intensity", self.intensity, d.intensity)] {
            if v.to_bits() != dv.to_bits() {
                f.push((k, Json::from(v)));
            }
        }
        if self.tint != d.tint {
            f.push(("tint", Json::arr(self.tint.map(Json::from))));
        }
        if !self.light {
            f.push(("light", Json::from(false)));
        }
        if self.seed != 0 {
            f.push(("seed", Json::from(self.seed)));
        }
        Json::obj(f)
    }

    fn hash_into(&self, h: &mut StableHasher) {
        for v in [self.rate, self.size, self.speed, self.alpha, self.intensity] {
            h.write_f32(v);
        }
        self.tint.iter().for_each(|c| h.write_f32(*c));
        h.write_bool(self.light);
        h.write_u32(self.seed);
    }
}

/// Checks an effect name: 1 to 64 of `A-Z a-z 0-9 _ - .` (it is also an entity's name under
/// `/vfx` for the world's own effects).
pub fn check_name(name: &str, at: &str) -> Result<()> {
    let ok = !name.is_empty() && name.len() <= MAX_NAME && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.') && !name.starts_with('.');
    if ok {
        Ok(())
    } else {
        Err(VfxError::new("invalid_value", at, format!("'{at}' must be an effect name: 1 to {MAX_NAME} of A-Z a-z 0-9 _ - . (not starting with '.')")).value(&Json::from(name)))
    }
}

/// The `vfx` component.
#[derive(Clone, Debug, PartialEq)]
pub struct VfxSpec {
    /// A built-in effect or one of the world's (`vfx_get` lists them).
    pub effect: String,
    pub params: Params,
    /// World time the effect starts at. `None` in a request: now (the world fills it in).
    pub start: Option<f64>,
}

impl VfxSpec {
    pub fn new(effect: impl Into<String>) -> VfxSpec {
        VfxSpec { effect: effect.into(), params: Params::default(), start: None }
    }

    /// From `"name"` or `{effect, params?, start?}`. (`false` / `null` removing the component is
    /// the caller's.)
    pub fn parse(v: &Json) -> Result<VfxSpec> {
        let at = Path::root("vfx");
        if let Some(name) = v.as_str() {
            check_name(name, "vfx")?;
            return Ok(VfxSpec::new(name));
        }
        if v.as_object().is_none() {
            return Err(VfxError::new("bad_type", "vfx", "'vfx' must be an effect name or {effect, params?, start?} (false removes it)").value(v));
        }
        read::keys(v, &at, &["effect", "params", "start"])?;
        let effect = v.get("effect").and_then(Json::as_str).ok_or_else(|| VfxError::new("missing_field", "vfx.effect", "'vfx.effect' is required: the effect's name (vfx_get lists them)"))?;
        check_name(effect, "vfx.effect")?;
        let params = match v.get("params") {
            None | Some(Json::Null) => Params::default(),
            Some(p) => Params::parse(p, &at.key("params"))?,
        };
        let start = parse_start(v, &at)?;
        Ok(VfxSpec { effect: effect.to_owned(), params, start })
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![("effect", Json::from(self.effect.as_str()))];
        let p = self.params.to_json();
        if p.as_object().is_some_and(|o| !o.is_empty()) {
            f.push(("params", p));
        }
        if let Some(s) = self.start {
            f.push(("start", Json::from(s)));
        }
        Json::obj(f)
    }

    pub fn hash_into(&self, h: &mut StableHasher) {
        h.write_str("vfx");
        h.write_str(&self.effect);
        self.params.hash_into(h);
        h.write_f64(self.start.unwrap_or(0.0));
    }

    /// Approximate heap bytes (the world's memory accounting).
    pub fn heap_bytes(&self) -> usize {
        self.effect.capacity()
    }
}

fn parse_start(v: &Json, at: &Path) -> Result<Option<f64>> {
    match v.get("start") {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) if s == "now" => Ok(None),
        Some(s) => match s.as_f64() {
            Some(x) if x.is_finite() && START_RANGE.contains(&x) => Ok(Some(x)),
            _ => Err(VfxError::new("invalid_value", at.key("start").as_str(), format!("'{}' must be world seconds or \"now\"", at.key("start"))).value(s)),
        },
    }
}

/// How a decal is applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecalMode {
    /// Albedo (and normal map, roughness, specular, emissive) re-lit and blended over the surface.
    Lit,
    /// The surface's colour times the texture's (darkening: soot, scorch, damp).
    Multiply,
}

/// The `decal` component: a box (the entity's transform × `size`, centred on the entity)
/// projecting its texture along its local −Y onto the opaque surfaces inside it.
#[derive(Clone, Debug, PartialEq)]
pub struct DecalSpec {
    pub texture: TextureRef,
    /// The box in the entity's local metres: width (X), projection depth (Y), length (Z).
    pub size: [f32; 3],
    /// sRGB tint.
    pub color: [f32; 3],
    pub opacity: f32,
    pub mode: DecalMode,
    /// A tangent-space normal map (lit mode).
    pub normal: Option<TextureRef>,
    pub normal_strength: f32,
    pub roughness: f32,
    pub specular: f32,
    /// Light emitted, × the tinted albedo (runes); 0 = none.
    pub emissive: f32,
    /// Surfaces facing less than this (cosine to the box's up axis) don't get the decal; it fades
    /// in over the next 0.2.
    pub angle_fade: f32,
    /// Draw order: higher draws over lower; ties by entity id.
    pub order: i32,
    /// Texture rect u0, v0, u1, v1 (an atlas region).
    pub uv: [f32; 4],
    /// Seconds after `start` the decal shows (`None`: always).
    pub life: Option<f32>,
    pub fade_in: f32,
    /// Seconds of fade at the end of `life`.
    pub fade_out: f32,
    /// World time the decal's life counts from. `None` in a request: now.
    pub start: Option<f64>,
}

impl Default for DecalSpec {
    fn default() -> Self {
        DecalSpec {
            texture: TextureRef::Builtin("scorch"),
            size: [1.0, 0.4, 1.0],
            color: [1.0; 3],
            opacity: 1.0,
            mode: DecalMode::Lit,
            normal: None,
            normal_strength: 1.0,
            roughness: 0.8,
            specular: 0.0,
            emissive: 0.0,
            angle_fade: 0.3,
            order: 0,
            uv: [0.0, 0.0, 1.0, 1.0],
            life: None,
            fade_in: 0.0,
            fade_out: 0.0,
            start: None,
        }
    }
}

const DECAL_KEYS: &[&str] = &[
    "texture", "size", "color", "opacity", "mode", "normal", "normal_strength", "roughness", "specular", "emissive", "angle_fade",
    "order", "uv", "life", "fade_in", "fade_out", "start",
];

impl DecalSpec {
    pub fn parse(v: &Json) -> Result<DecalSpec> {
        let at = Path::root("decal");
        if let Some(name) = v.as_str() {
            return Ok(DecalSpec { texture: TextureRef::parse(v, &at, true).map_err(|e| VfxError { field: "decal".into(), ..e })?, mode: default_mode(name), ..DecalSpec::default() });
        }
        read::keys(v, &at, DECAL_KEYS)?;
        let d = DecalSpec::default();
        let texture = match v.get("texture") {
            None | Some(Json::Null) => d.texture.clone(),
            Some(t) => TextureRef::parse(t, &at.key("texture"), true)?,
        };
        let size = read::vec3_or(v, "size", &at, 1000.0, d.size)?;
        if size.iter().any(|s| *s < 0.01) {
            return Err(VfxError::new("invalid_value", "decal.size", "'decal.size' components must be at least 0.01 m").allowed("[0.01..=1000, 0.01..=1000, 0.01..=1000]"));
        }
        let mode = match v.get("mode") {
            None | Some(Json::Null) => default_mode(texture.name()),
            Some(Json::Str(s)) if s == "lit" => DecalMode::Lit,
            Some(Json::Str(s)) if s == "multiply" => DecalMode::Multiply,
            Some(m) => return Err(VfxError::new("invalid_value", "decal.mode", "'decal.mode' must be \"lit\" or \"multiply\"").allowed("lit | multiply").value(m)),
        };
        let normal = match v.get("normal") {
            None | Some(Json::Null) => None,
            Some(n) => Some(TextureRef::parse(n, &at.key("normal"), true)?),
        };
        let uv = match v.get("uv") {
            None | Some(Json::Null) => d.uv,
            Some(u) => match u.as_array() {
                Some([a, b, c, e]) => {
                    let p = at.key("uv");
                    [read::num(a, &p.index(0), 0.0, 1.0)?, read::num(b, &p.index(1), 0.0, 1.0)?, read::num(c, &p.index(2), 0.0, 1.0)?, read::num(e, &p.index(3), 0.0, 1.0)?]
                }
                _ => return Err(VfxError::new("invalid_value", "decal.uv", "'decal.uv' must be [u0, v0, u1, v1] in 0..1").value(u)),
            },
        };
        let life = match v.get("life") {
            None | Some(Json::Null) => None,
            Some(l) => Some(read::num(l, &at.key("life"), 0.0, 1.0e6)?),
        };
        Ok(DecalSpec {
            texture,
            size,
            color: match v.get("color") {
                None | Some(Json::Null) => d.color,
                Some(c) => read::color(c, &at.key("color"))?,
            },
            opacity: read::num_or(v, "opacity", &at, 0.0, 1.0, d.opacity)?,
            mode,
            normal,
            normal_strength: read::num_or(v, "normal_strength", &at, 0.0, 4.0, d.normal_strength)?,
            roughness: read::num_or(v, "roughness", &at, 0.0, 1.0, d.roughness)?,
            specular: read::num_or(v, "specular", &at, 0.0, 1.0, d.specular)?,
            emissive: read::num_or(v, "emissive", &at, 0.0, 100.0, d.emissive)?,
            angle_fade: read::num_or(v, "angle_fade", &at, -1.0, 1.0, d.angle_fade)?,
            order: read::num_or(v, "order", &at, -1000.0, 1000.0, 0.0)? as i32,
            uv,
            life,
            fade_in: read::num_or(v, "fade_in", &at, 0.0, 1.0e4, 0.0)?,
            fade_out: read::num_or(v, "fade_out", &at, 0.0, 1.0e4, 0.0)?,
            start: parse_start(v, &at)?,
        })
    }

    pub fn to_json(&self) -> Json {
        let d = DecalSpec::default();
        let mut f = vec![("texture", Json::from(self.texture.name()))];
        f.push(("size", Json::arr(self.size.map(Json::from))));
        if self.color != d.color {
            // Exact floats (not hex): a saved and reloaded decal hashes the same.
            f.push(("color", Json::arr(self.color.map(Json::from))));
        }
        if self.opacity != d.opacity {
            f.push(("opacity", Json::from(self.opacity)));
        }
        f.push(("mode", Json::from(match self.mode {
            DecalMode::Lit => "lit",
            DecalMode::Multiply => "multiply",
        })));
        if let Some(n) = &self.normal {
            f.push(("normal", Json::from(n.name())));
        }
        for (k, v, dv) in [
            ("normal_strength", self.normal_strength, d.normal_strength),
            ("roughness", self.roughness, d.roughness),
            ("specular", self.specular, d.specular),
            ("emissive", self.emissive, d.emissive),
            ("angle_fade", self.angle_fade, d.angle_fade),
            ("fade_in", self.fade_in, d.fade_in),
            ("fade_out", self.fade_out, d.fade_out),
        ] {
            if v.to_bits() != dv.to_bits() {
                f.push((k, Json::from(v)));
            }
        }
        if self.order != 0 {
            f.push(("order", Json::from(self.order)));
        }
        if self.uv != d.uv {
            f.push(("uv", Json::arr(self.uv.map(Json::from))));
        }
        if let Some(l) = self.life {
            f.push(("life", Json::from(l)));
        }
        if let Some(s) = self.start {
            f.push(("start", Json::from(s)));
        }
        Json::obj(f)
    }

    pub fn hash_into(&self, h: &mut StableHasher) {
        h.write_str("decal");
        h.write_str(self.texture.name());
        self.size.iter().chain(&self.color).for_each(|v| h.write_f32(*v));
        h.write_f32(self.opacity);
        h.write_u8(self.mode as u8);
        h.write_str(self.normal.as_ref().map_or("", TextureRef::name));
        for v in [self.normal_strength, self.roughness, self.specular, self.emissive, self.angle_fade, self.fade_in, self.fade_out] {
            h.write_f32(v);
        }
        h.write_i64(i64::from(self.order));
        self.uv.iter().for_each(|v| h.write_f32(*v));
        h.write_f32(self.life.unwrap_or(-1.0));
        h.write_f64(self.start.unwrap_or(0.0));
    }

    /// Opacity at world time `t`: its life, fades in and out (1 without a life).
    pub fn opacity_at(&self, t: f64) -> f32 {
        let Some(life) = self.life else { return self.opacity };
        let age = (t - self.start.unwrap_or(0.0)) as f32;
        if age < 0.0 || age > life {
            return 0.0;
        }
        let fin = if self.fade_in > 0.0 { (age / self.fade_in).min(1.0) } else { 1.0 };
        let fout = if self.fade_out > 0.0 { ((life - age) / self.fade_out).min(1.0) } else { 1.0 };
        self.opacity * fin * fout
    }
}

/// Darkening textures default to multiply; everything else is re-lit.
fn default_mode(texture: &str) -> DecalMode {
    if texture == "scorch" {
        DecalMode::Multiply
    } else {
        DecalMode::Lit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decal_colours_round_trip_exactly() {
        let a = DecalSpec::parse(&Json::parse(r#"{"texture": "rune_circle", "color": [0.3, 0.5, 0.7], "start": 0}"#).unwrap()).unwrap();
        let b = DecalSpec::parse(&a.to_json()).unwrap();
        assert_eq!(a.color.map(f32::to_bits), b.color.map(f32::to_bits));
        assert_eq!(format!("{}", a.to_json()), format!("{}", b.to_json()));
    }

    #[test]
    fn vfx_component_forms() {
        let s = VfxSpec::parse(&Json::from("fire")).unwrap();
        assert_eq!((s.effect.as_str(), s.start), ("fire", None));
        let s = VfxSpec::parse(&Json::parse(r##"{"effect": "fire", "params": {"rate": 2, "tint": "#ff0000", "seed": 9}, "start": 3.5}"##).unwrap()).unwrap();
        assert_eq!((s.params.rate, s.params.tint, s.params.seed, s.start), (2.0, [1.0, 0.0, 0.0], 9, Some(3.5)));
        assert_eq!(VfxSpec::parse(&s.to_json()).unwrap(), s);
        let e = VfxSpec::parse(&Json::parse(r#"{"effect": "fire", "params": {"rate": 99}}"#).unwrap()).unwrap_err();
        assert_eq!(e.field, "vfx.params.rate");
        assert_eq!(VfxSpec::parse(&Json::from("a/b")).unwrap_err().code, "invalid_value");
        assert_eq!(VfxSpec::parse(&Json::parse("{}").unwrap()).unwrap_err().field, "vfx.effect");
    }

    #[test]
    fn decal_component_forms() {
        let d = DecalSpec::parse(&Json::from("scorch")).unwrap();
        assert_eq!(d.mode, DecalMode::Multiply);
        let d = DecalSpec::parse(&Json::parse(r##"{"texture": "rune_circle", "size": [2, 0.3, 2], "color": "#3fe0d0", "emissive": 2.5, "life": 4, "fade_out": 1, "start": 10}"##).unwrap()).unwrap();
        assert_eq!(d.mode, DecalMode::Lit);
        assert_eq!(DecalSpec::parse(&d.to_json()).unwrap(), d);
        assert_eq!(d.opacity_at(9.0), 0.0);
        assert_eq!(d.opacity_at(11.0), 1.0);
        assert!((d.opacity_at(13.5) - 0.5).abs() < 1e-6);
        assert_eq!(d.opacity_at(14.5), 0.0);
        assert_eq!(DecalSpec::parse(&Json::parse(r#"{"size": [1, 0, 1]}"#).unwrap()).unwrap_err().field, "decal.size");
        assert_eq!(DecalSpec::parse(&Json::parse(r#"{"texture": "flame"}"#).unwrap()).unwrap_err().code, "unknown_texture");
    }

    #[test]
    fn hashes_follow_content() {
        let h = |s: &VfxSpec| {
            let mut h = StableHasher::with_seed(1);
            s.hash_into(&mut h);
            h.finish()
        };
        let a = VfxSpec::new("fire");
        let mut b = a.clone();
        b.params.rate = 2.0;
        assert_ne!(h(&a), h(&b));
        assert_eq!(h(&a), h(&a.clone()));
    }
}

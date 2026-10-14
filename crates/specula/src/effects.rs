//! The renderer's optional effects (shadows, bloom, FXAA, sky, fog, grading, SSAO) as world environment
//! settings: partial patches agents send with `world_env`, value limits (checked by the world, so
//! the in-process API and world files obey them too), JSON for listings and world files, and
//! state hashing.
//!
//! Every effect is off by default, so a world that never mentions them renders exactly as before.

use crate::world::{color_from_hex, color_hex, color_json_exact};
use specula_core::hash::StableHasher;
use specula_core::Json;
use specula_gpu::{BloomSettings, FogSettings, GradingSettings, ShadowSettings, SkySettings, SsaoSettings};
use std::fmt;

/// How a field is read, checked and written.
#[derive(Clone, Copy)]
pub enum Kind {
    Flag,
    /// An integer in this range.
    Count(u32, u32),
    /// A finite number in this range.
    Num(f32, f32),
    /// sRGB, components 0..1.
    Color,
    /// Three numbers (not a colour), each in this range.
    Triple(f32, f32),
    /// A file under the assets root ([`AssetPath`]).
    Path,
}

/// A value an effect setting can hold.
pub trait Field: Copy + Sized {
    /// Reads `obj[key]` (type only; ranges are [`Field::check`]'s job). `label` names the effect.
    fn read(obj: &Json, label: &str, key: &str, kind: Kind) -> Result<Option<Self>, String>;
    fn check(&self, kind: Kind) -> bool;
    fn write(&self, exact: bool, kind: Kind) -> Json;
    fn hash(&self, h: &mut StableHasher);
}

impl Field for bool {
    fn read(obj: &Json, label: &str, key: &str, _: Kind) -> Result<Option<bool>, String> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(v) => v.as_bool().map(Some).ok_or_else(|| format!("'{label}.{key}' must be true or false")),
        }
    }
    fn check(&self, _: Kind) -> bool {
        true
    }
    fn write(&self, _: bool, _: Kind) -> Json {
        Json::from(*self)
    }
    fn hash(&self, h: &mut StableHasher) {
        h.write_bool(*self);
    }
}

impl Field for u32 {
    fn read(obj: &Json, label: &str, key: &str, _: Kind) -> Result<Option<u32>, String> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(v) => v
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .map(Some)
                .ok_or_else(|| format!("'{label}.{key}' must be a whole number")),
        }
    }
    fn check(&self, kind: Kind) -> bool {
        matches!(kind, Kind::Count(lo, hi) if (lo..=hi).contains(self))
    }
    fn write(&self, _: bool, _: Kind) -> Json {
        Json::from(*self)
    }
    fn hash(&self, h: &mut StableHasher) {
        h.write_u32(*self);
    }
}

impl Field for f32 {
    fn read(obj: &Json, label: &str, key: &str, _: Kind) -> Result<Option<f32>, String> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(v) => match v.as_f64() {
                Some(f) if f.is_finite() && f.abs() <= f64::from(f32::MAX) => Ok(Some(f as f32)),
                _ => Err(format!("'{label}.{key}' must be a finite number")),
            },
        }
    }
    fn check(&self, kind: Kind) -> bool {
        matches!(kind, Kind::Num(lo, hi) if self.is_finite() && (lo..=hi).contains(self))
    }
    fn write(&self, _: bool, _: Kind) -> Json {
        Json::from(*self)
    }
    fn hash(&self, h: &mut StableHasher) {
        h.write_f32(*self);
    }
}

impl Field for [f32; 3] {
    fn read(obj: &Json, label: &str, key: &str, kind: Kind) -> Result<Option<[f32; 3]>, String> {
        let err = || match kind {
            Kind::Color => format!("'{label}.{key}' must be \"#rrggbb\" or [r, g, b] with components 0..1"),
            _ => format!("'{label}.{key}' must be [r, g, b]: three numbers"),
        };
        match obj.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(Json::Str(s)) if matches!(kind, Kind::Color) => color_from_hex(s).map(Some).ok_or_else(err),
            Some(Json::Arr(a)) => match a.iter().map(Json::as_f64).collect::<Option<Vec<_>>>().as_deref() {
                Some(&[r, g, b]) if [r, g, b].iter().all(|c| c.is_finite()) => Ok(Some([r as f32, g as f32, b as f32])),
                _ => Err(err()),
            },
            Some(_) => Err(err()),
        }
    }
    fn check(&self, kind: Kind) -> bool {
        let (lo, hi) = match kind {
            Kind::Triple(lo, hi) => (lo, hi),
            _ => (0.0, 1.0),
        };
        self.iter().all(|c| c.is_finite() && (lo..=hi).contains(c))
    }
    fn write(&self, exact: bool, kind: Kind) -> Json {
        match kind {
            Kind::Color if exact => color_json_exact(*self),
            Kind::Color => Json::from(color_hex(*self)),
            _ => Json::arr(self.map(Json::from)),
        }
    }
    fn hash(&self, h: &mut StableHasher) {
        self.iter().for_each(|c| h.write_f32(*c));
    }
}

fn kind_text(kind: Kind) -> String {
    match kind {
        Kind::Flag => "true or false".into(),
        Kind::Count(lo, hi) => format!("a whole number from {lo} to {hi}"),
        Kind::Num(lo, hi) => format!("a number from {lo} to {hi}"),
        Kind::Color => "a colour with components 0..1".into(),
        Kind::Triple(lo, hi) => format!("[r, g, b] with each from {lo} to {hi}"),
        Kind::Path => AssetPath::RULES.into(),
    }
}

/// A file under the engine's assets root (`--assets`), kept inline so environments stay `Copy`.
/// Relative, forward slashes, letters, digits and `_ . -` only, no `.` or `..` segments, at most
/// [`AssetPath::MAX`] bytes. Empty = none.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AssetPath {
    len: u8,
    bytes: [u8; AssetPath::MAX],
}

impl AssetPath {
    pub const MAX: usize = 120;
    pub const NONE: AssetPath = AssetPath { len: 0, bytes: [0; AssetPath::MAX] };
    const RULES: &'static str = "a .png path relative to the assets root: letters, digits, '_', '.', '-' and '/' only, \
                                 no '.' or '..' segments, at most 120 bytes (\"\" for none)";

    /// `None` if `s` breaks the rules (see the type's docs); `""` is [`AssetPath::NONE`].
    pub fn new(s: &str) -> Option<AssetPath> {
        if s.is_empty() {
            return Some(AssetPath::NONE);
        }
        let ok_chars = s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-' | b'/'));
        let ok_segments = s.split('/').all(|seg| !seg.is_empty() && seg != "." && seg != "..");
        if s.len() > AssetPath::MAX || !ok_chars || !ok_segments || !s.ends_with(".png") {
            return None;
        }
        let mut bytes = [0; AssetPath::MAX];
        bytes[..s.len()].copy_from_slice(s.as_bytes());
        Some(AssetPath { len: s.len() as u8, bytes })
    }

    pub fn as_str(&self) -> &str {
        // Only ASCII is ever stored (checked by `new`).
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    pub fn is_none(&self) -> bool {
        self.len == 0
    }
}

impl Default for AssetPath {
    fn default() -> Self {
        AssetPath::NONE
    }
}

impl fmt::Debug for AssetPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl Field for AssetPath {
    fn read(obj: &Json, label: &str, key: &str, _: Kind) -> Result<Option<AssetPath>, String> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(Json::Str(s)) => AssetPath::new(s).map(Some).ok_or_else(|| format!("'{label}.{key}' must be {}", AssetPath::RULES)),
            Some(_) => Err(format!("'{label}.{key}' must be {}", AssetPath::RULES)),
        }
    }
    fn check(&self, _: Kind) -> bool {
        AssetPath::new(self.as_str()).is_some()
    }
    fn write(&self, _: bool, _: Kind) -> Json {
        Json::from(self.as_str())
    }
    fn hash(&self, h: &mut StableHasher) {
        h.write_str(self.as_str());
    }
}

/// A partial update to one effect's settings: `None` fields stay as they are.
macro_rules! effect_patch {
    ($name:ident for $settings:ty, $label:literal { $($field:ident : $ty:ty = $kind:expr),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq)]
        pub struct $name {
            $(pub $field: Option<$ty>,)*
        }

        impl $name {
            pub const LABEL: &'static str = $label;
            pub const KEYS: &'static [&'static str] = &[$(stringify!($field)),*];

            pub fn apply(&self, s: &mut $settings) {
                $(if let Some(v) = self.$field {
                    s.$field = v;
                })*
            }

            /// Reads the effect's JSON object (unknown keys rejected); ranges are checked by
            /// [`Self::check`].
            pub fn parse(obj: &Json) -> Result<$name, String> {
                let Json::Obj(pairs) = obj else {
                    return Err(format!("'{}' must be an object with any of: {}", $label, Self::KEYS.join(", ")));
                };
                if let Some((key, _)) = pairs.iter().find(|(k, _)| !Self::KEYS.contains(&k.as_str())) {
                    return Err(format!("'{}.{key}' is not a setting; valid: {}", $label, Self::KEYS.join(", ")));
                }
                Ok($name { $($field: <$ty as Field>::read(obj, $label, stringify!($field), $kind)?,)* })
            }

            /// `Err((field, message))` for the first value out of range.
            pub fn check(&self) -> Result<(), (String, String)> {
                $(if let Some(v) = self.$field {
                    if !v.check($kind) {
                        let field = format!("{}.{}", $label, stringify!($field));
                        let message = format!("'{field}' must be {}", kind_text($kind));
                        return Err((field, message));
                    }
                })*
                Ok(())
            }

            pub fn to_json(s: &$settings, exact: bool) -> Json {
                Json::obj([$((stringify!($field), s.$field.write(exact, $kind)),)*])
            }

            pub fn hash(s: &$settings, h: &mut StableHasher) {
                $(s.$field.hash(h);)*
            }
        }
    };
}

effect_patch!(ShadowPatch for ShadowSettings, "shadow" {
    enabled: bool = Kind::Flag,
    resolution: u32 = Kind::Count(256, 8192),
    cascades: u32 = Kind::Count(1, specula_gpu::MAX_SHADOW_CASCADES),
    distance: f32 = Kind::Num(1.0, 10_000.0),
    softness: f32 = Kind::Num(0.0, 8.0),
    bias: f32 = Kind::Num(0.0, 10.0),
});

effect_patch!(BloomPatch for BloomSettings, "bloom" {
    enabled: bool = Kind::Flag,
    threshold: f32 = Kind::Num(0.0, 100.0),
    knee: f32 = Kind::Num(0.0, 10.0),
    intensity: f32 = Kind::Num(0.0, 10.0),
    levels: u32 = Kind::Count(1, 8),
});

effect_patch!(SkyPatch for SkySettings, "sky" {
    enabled: bool = Kind::Flag,
    zenith: [f32; 3] = Kind::Color,
    horizon: [f32; 3] = Kind::Color,
    ground: [f32; 3] = Kind::Color,
    intensity: f32 = Kind::Num(0.0, 100.0),
    horizon_sharpness: f32 = Kind::Num(0.1, 100.0),
    disc_size_deg: f32 = Kind::Num(0.0, 30.0),
    disc_intensity: f32 = Kind::Num(0.0, 1000.0),
    glow: f32 = Kind::Num(0.0, 10.0),
});

effect_patch!(FogPatch for FogSettings, "fog" {
    enabled: bool = Kind::Flag,
    color: [f32; 3] = Kind::Color,
    density: f32 = Kind::Num(0.0, 10.0),
    falloff: f32 = Kind::Num(0.0, 10.0),
    height: f32 = Kind::Num(-1.0e6, 1.0e6),
    start_distance: f32 = Kind::Num(0.0, 1.0e6),
    sun_scatter: f32 = Kind::Num(0.0, 10.0),
});

effect_patch!(GradingFields for GradingSettings, "grading" {
    enabled: bool = Kind::Flag,
    temperature: f32 = Kind::Num(-1.0, 1.0),
    tint: f32 = Kind::Num(-1.0, 1.0),
    contrast: f32 = Kind::Num(0.0, 4.0),
    saturation: f32 = Kind::Num(0.0, 4.0),
    vibrance: f32 = Kind::Num(-1.0, 1.0),
    lift: [f32; 3] = Kind::Triple(-1.0, 1.0),
    gamma: [f32; 3] = Kind::Triple(0.1, 10.0),
    gain: [f32; 3] = Kind::Triple(0.0, 4.0),
    lut_strength: f32 = Kind::Num(0.0, 1.0),
    vignette: f32 = Kind::Num(0.0, 1.0),
    vignette_radius: f32 = Kind::Num(0.0, 0.99),
    grain: f32 = Kind::Num(0.0, 4.0),
});

/// A partial update to colour grading: [`GradingFields`] plus the 3D LUT, a PNG strip under the
/// assets root (`lut`: a path, `""` for none). The LUT texture itself is the engine's business:
/// the world keeps the path.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GradingPatch {
    pub fields: GradingFields,
    pub lut: Option<AssetPath>,
}

impl GradingPatch {
    pub const LABEL: &'static str = "grading";

    fn keys() -> String {
        let mut keys = GradingFields::KEYS.to_vec();
        keys.push("lut");
        keys.join(", ")
    }

    /// Reads the grading object (unknown keys rejected).
    pub fn parse(obj: &Json) -> Result<GradingPatch, String> {
        let Json::Obj(pairs) = obj else {
            return Err(format!("'grading' must be an object with any of: {}", Self::keys()));
        };
        if let Some((key, _)) = pairs.iter().find(|(k, _)| k != "lut" && !GradingFields::KEYS.contains(&k.as_str())) {
            return Err(format!("'grading.{key}' is not a setting; valid: {}", Self::keys()));
        }
        let lut = AssetPath::read(obj, "grading", "lut", Kind::Path)?;
        let rest = Json::Obj(pairs.iter().filter(|(k, _)| k != "lut").cloned().collect());
        Ok(GradingPatch { fields: GradingFields::parse(&rest)?, lut })
    }

    pub fn check(&self) -> Result<(), (String, String)> {
        self.fields.check()
    }

    pub fn apply(&self, s: &mut GradingSettings, lut: &mut AssetPath) {
        self.fields.apply(s);
        if let Some(p) = self.lut {
            *lut = p;
        }
    }

    pub fn to_json(s: &GradingSettings, lut: &AssetPath, exact: bool) -> Json {
        let Json::Obj(mut pairs) = GradingFields::to_json(s, exact) else { unreachable!("an object") };
        pairs.push(("lut".into(), Json::from(lut.as_str())));
        Json::Obj(pairs)
    }

    /// Hashes nothing while grading is all defaults, so worlds that never graded keep their hash.
    pub fn hash(s: &GradingSettings, lut: &AssetPath, h: &mut StableHasher) {
        if *s != GradingSettings::default() || !lut.is_none() {
            h.write_str("grading");
            GradingFields::hash(s, h);
            lut.hash(h);
        }
    }
}

effect_patch!(SsaoPatch for SsaoSettings, "ssao" {
    enabled: bool = Kind::Flag,
    radius: f32 = Kind::Num(0.05, 10.0),
    intensity: f32 = Kind::Num(0.0, 4.0),
    samples: u32 = Kind::Count(4, specula_gpu::MAX_SSAO_SAMPLES),
    resolution: f32 = Kind::Num(0.25, 1.0),
});

impl SsaoPatch {
    /// Hashes nothing while SSAO is all defaults, so older worlds keep their hash.
    pub fn hash_if_set(s: &SsaoSettings, h: &mut StableHasher) {
        if *s != SsaoSettings::default() {
            h.write_str("ssao");
            SsaoPatch::hash(s, h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_patches_apply_only_what_they_name() {
        let p = BloomPatch::parse(&Json::parse(r#"{"enabled":true,"levels":3}"#).unwrap()).unwrap();
        let mut s = BloomSettings::default();
        p.apply(&mut s);
        assert_eq!((s.enabled, s.levels, s.threshold), (true, 3, BloomSettings::default().threshold));
    }

    #[test]
    fn bad_values_name_their_field() {
        let e = FogPatch::parse(&Json::parse(r#"{"haze":1}"#).unwrap()).unwrap_err();
        assert!(e.starts_with("'fog.haze'"), "{e}");
        let p = ShadowPatch::parse(&Json::parse(r#"{"cascades":9}"#).unwrap()).unwrap();
        assert_eq!(p.check().unwrap_err().0, "shadow.cascades");
        let p = SkyPatch::parse(&Json::parse(r#"{"zenith":[2,0,0]}"#).unwrap()).unwrap();
        assert_eq!(p.check().unwrap_err().0, "sky.zenith");
        assert!(SkyPatch::parse(&Json::from(3)).is_err());
    }

    #[test]
    fn grading_parses_checks_and_round_trips() {
        let j = Json::parse(r#"{"enabled":true,"lift":[-0.02,0,0.03],"gamma":[1,1.1,1],"lut":"luts/night.png"}"#).unwrap();
        let p = GradingPatch::parse(&j).unwrap();
        p.check().unwrap();
        let (mut s, mut lut) = (GradingSettings::default(), AssetPath::NONE);
        p.apply(&mut s, &mut lut);
        assert_eq!((s.enabled, s.lift, s.gamma[1], lut.as_str()), (true, [-0.02, 0.0, 0.03], 1.1, "luts/night.png"));
        let back = GradingPatch::to_json(&s, &lut, true);
        let (mut s2, mut lut2) = (GradingSettings::default(), AssetPath::NONE);
        GradingPatch::parse(&back).unwrap().apply(&mut s2, &mut lut2);
        assert_eq!((s2, lut2), (s, lut));
        // Lift is three numbers, not a colour: no hex, and negative values are fine.
        assert!(GradingPatch::parse(&Json::parse(r##"{"lift":"#ff0000"}"##).unwrap()).is_err());
        let p = GradingPatch::parse(&Json::parse(r#"{"gain":[5,1,1]}"#).unwrap()).unwrap();
        assert_eq!(p.check().unwrap_err().0, "grading.gain");
        let e = GradingPatch::parse(&Json::parse(r#"{"warmth":1}"#).unwrap()).unwrap_err();
        assert!(e.starts_with("'grading.warmth'") && e.contains("lut"), "{e}");
        // "" removes the LUT.
        let p = GradingPatch::parse(&Json::parse(r#"{"lut":""}"#).unwrap()).unwrap();
        p.apply(&mut s, &mut lut);
        assert!(lut.is_none());
    }

    #[test]
    fn asset_paths_stay_inside_the_assets_root() {
        for good in ["luts/night.png", "a.png", "x/y-z_1.png"] {
            assert_eq!(AssetPath::new(good).unwrap().as_str(), good);
        }
        for bad in ["../x.png", "/x.png", "C:/x.png", "a\\b.png", "a//b.png", "./a.png", "a/../b.png", "x.jpg", "a b.png"] {
            assert!(AssetPath::new(bad).is_none(), "{bad}");
        }
        assert!(AssetPath::new(&format!("{}.png", "a".repeat(117))).is_none(), "too long");
        assert!(AssetPath::new("").unwrap().is_none());
    }

    #[test]
    fn ssao_limits() {
        let p = SsaoPatch::parse(&Json::parse(r#"{"enabled":true,"samples":32}"#).unwrap()).unwrap();
        assert_eq!(p.check().unwrap_err().0, "ssao.samples");
        let p = SsaoPatch::parse(&Json::parse(r#"{"resolution":0.1}"#).unwrap()).unwrap();
        assert_eq!(p.check().unwrap_err().0, "ssao.resolution");
    }

    #[test]
    fn default_effects_hash_nothing() {
        let mut a = StableHasher::with_seed(1);
        GradingPatch::hash(&GradingSettings::default(), &AssetPath::NONE, &mut a);
        SsaoPatch::hash_if_set(&SsaoSettings::default(), &mut a);
        assert_eq!(a.finish(), StableHasher::with_seed(1).finish());
    }

    #[test]
    fn json_round_trips_through_a_patch() {
        let mut s = FogSettings { enabled: true, density: 0.07, ..FogSettings::default() };
        let j = FogPatch::to_json(&s, true);
        let before = s;
        s = FogSettings::default();
        FogPatch::parse(&j).unwrap().apply(&mut s);
        assert_eq!(s, before);
    }
}

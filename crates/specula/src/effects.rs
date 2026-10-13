//! The renderer's optional effects (shadows, bloom, FXAA, sky, fog) as world environment
//! settings: partial patches agents send with `world_env`, value limits (checked by the world, so
//! the in-process API and world files obey them too), JSON for listings and world files, and
//! state hashing.
//!
//! Every effect is off by default, so a world that never mentions them renders exactly as before.

use crate::world::{color_from_hex, color_hex, color_json_exact};
use specula_core::hash::StableHasher;
use specula_core::Json;
use specula_gpu::{BloomSettings, FogSettings, ShadowSettings, SkySettings};

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
}

/// A value an effect setting can hold.
pub trait Field: Copy + Sized {
    /// Reads `obj[key]` (type only; ranges are [`Field::check`]'s job). `label` names the effect.
    fn read(obj: &Json, label: &str, key: &str) -> Result<Option<Self>, String>;
    fn check(&self, kind: Kind) -> bool;
    fn write(&self, exact: bool) -> Json;
    fn hash(&self, h: &mut StableHasher);
}

impl Field for bool {
    fn read(obj: &Json, label: &str, key: &str) -> Result<Option<bool>, String> {
        match obj.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(v) => v.as_bool().map(Some).ok_or_else(|| format!("'{label}.{key}' must be true or false")),
        }
    }
    fn check(&self, _: Kind) -> bool {
        true
    }
    fn write(&self, _: bool) -> Json {
        Json::from(*self)
    }
    fn hash(&self, h: &mut StableHasher) {
        h.write_bool(*self);
    }
}

impl Field for u32 {
    fn read(obj: &Json, label: &str, key: &str) -> Result<Option<u32>, String> {
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
    fn write(&self, _: bool) -> Json {
        Json::from(*self)
    }
    fn hash(&self, h: &mut StableHasher) {
        h.write_u32(*self);
    }
}

impl Field for f32 {
    fn read(obj: &Json, label: &str, key: &str) -> Result<Option<f32>, String> {
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
    fn write(&self, _: bool) -> Json {
        Json::from(*self)
    }
    fn hash(&self, h: &mut StableHasher) {
        h.write_f32(*self);
    }
}

impl Field for [f32; 3] {
    fn read(obj: &Json, label: &str, key: &str) -> Result<Option<[f32; 3]>, String> {
        let err = || format!("'{label}.{key}' must be \"#rrggbb\" or [r, g, b] with components 0..1");
        match obj.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(Json::Str(s)) => color_from_hex(s).map(Some).ok_or_else(err),
            Some(Json::Arr(a)) => match a.iter().map(Json::as_f64).collect::<Option<Vec<_>>>().as_deref() {
                Some(&[r, g, b]) if [r, g, b].iter().all(|c| c.is_finite()) => Ok(Some([r as f32, g as f32, b as f32])),
                _ => Err(err()),
            },
            Some(_) => Err(err()),
        }
    }
    fn check(&self, _: Kind) -> bool {
        self.iter().all(|c| c.is_finite() && (0.0..=1.0).contains(c))
    }
    fn write(&self, exact: bool) -> Json {
        if exact {
            color_json_exact(*self)
        } else {
            Json::from(color_hex(*self))
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
                Ok($name { $($field: <$ty as Field>::read(obj, $label, stringify!($field))?,)* })
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
                Json::obj([$((stringify!($field), s.$field.write(exact)),)*])
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
    fn json_round_trips_through_a_patch() {
        let mut s = FogSettings { enabled: true, density: 0.07, ..FogSettings::default() };
        let j = FogPatch::to_json(&s, true);
        let before = s;
        s = FogSettings::default();
        FogPatch::parse(&j).unwrap().apply(&mut s);
        assert_eq!(s, before);
    }
}

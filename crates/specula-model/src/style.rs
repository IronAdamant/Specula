//! A game's style: named palette colours and material presets that recipes refer to by name.
//!
//! Recipes never carry raw material parameters, so a swarm stays on-style: they pick a preset
//! (`carved_stone`, `rune_glow`) and optionally a palette colour. Format (JSON):
//!
//! ```text
//! { "style": "sepulcra",
//!   "palette":   { "carved_highlight": "#aab4c0", ... },
//!   "materials": { "carved_stone": { "color": "#8c96a3", "roughness": 0.75 },
//!                  "wraith": { "kind": "glass", "color": "#9cff4f", "emissive": 0.8, "opacity": 0.35 } },
//!   "roles":     { "coat": "carved_stone" } }
//! ```
//!
//! **Roles** (docs/design/EXEMPLARIA.md, "The look is the project's"): a library entry names what
//! a part is for (`coat`, `claw`, `hull`), never a preset of any one style. [`Style::role`]
//! resolves a role to this style's preset of the same name, else through `roles`, else it is
//! refused with the presets nearest in colour to the role's hint, so the same creature is built
//! in each game's own materials.

use std::collections::BTreeMap;

use specula_core::Json;

use crate::error::{ErrorCode, RecipeError};
use crate::shape::Rgba;

/// How a preset is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialKind {
    /// The engine's opaque `standard` material.
    Standard,
    /// The engine's blended `glass` shell (`opacity` is its alpha).
    Glass,
}

/// A material preset: what the engine needs to draw a part.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialPreset {
    pub kind: MaterialKind,
    /// sRGB base colour (vertex colours replace it per vertex; this is the default).
    pub color: Rgba,
    pub roughness: f32,
    pub metallic: f32,
    /// Emission as a multiple of the colour (0 = none).
    pub emissive: f32,
    /// Highlight strength.
    pub specular: f32,
    /// Opacity of `glass`.
    pub opacity: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub name: String,
    pub palette: BTreeMap<String, Rgba>,
    pub materials: BTreeMap<String, MaterialPreset>,
    /// A role (what a library entry's part is for) to one of `materials` by name.
    pub roles: BTreeMap<String, String>,
}

/// Presets [`RoleMissing`] names: the nearest in colour to the role's hint.
pub const ROLE_NEAREST: usize = 3;

/// A role the style can't resolve: no preset of its name and no line in `roles`.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleMissing {
    pub role: String,
    /// The style's presets nearest in colour to the hint (at most [`ROLE_NEAREST`]; by name when
    /// there is no hint).
    pub nearest: Vec<String>,
}

impl std::fmt::Display for RoleMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the style has no preset for the role '{}': add \"roles\": {{\"{}\": PRESET}} to style.json", self.role, self.role)?;
        if !self.nearest.is_empty() {
            write!(f, " (nearest presets by colour: {})", self.nearest.join(", "))?;
        }
        Ok(())
    }
}

/// `#rrggbb` or `#rrggbbaa` → sRGB 0..1.
pub fn parse_hex(s: &str) -> Option<Rgba> {
    let h = s.strip_prefix('#')?;
    if !(h.len() == 6 || h.len() == 8) || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|v| v as f32 / 255.0);
    Some([byte(0)?, byte(2)?, byte(4)?, if h.len() == 8 { byte(6)? } else { 1.0 }])
}

fn style_err(path: &str, msg: impl Into<String>) -> RecipeError {
    RecipeError::new(path, ErrorCode::Style, msg)
}

impl Style {
    pub fn parse(text: &str) -> Result<Style, RecipeError> {
        let json = Json::parse(text).map_err(|e| RecipeError::new("", ErrorCode::Json, e.message.clone()).with_offset(e.offset))?;
        crate::error::check_duplicate_keys(&json, "")?;
        let obj = json.as_object().ok_or_else(|| style_err("", "style must be a JSON object"))?;
        let mut style = Style::default();
        for (k, v) in obj {
            match k.as_str() {
                "style" => style.name = v.as_str().ok_or_else(|| style_err("style", "must be a string"))?.to_string(),
                "description" | "proposals" => {}
                "palette" => {
                    for (name, c) in v.as_object().ok_or_else(|| style_err("palette", "must be an object"))? {
                        let path = format!("palette.{name}");
                        let col = c.as_str().and_then(parse_hex).ok_or_else(|| style_err(&path, "must be \"#rrggbb\""))?;
                        style.palette.insert(name.clone(), col);
                    }
                }
                "materials" => {
                    for (name, m) in v.as_object().ok_or_else(|| style_err("materials", "must be an object"))? {
                        let path = format!("materials.{name}");
                        style.materials.insert(name.clone(), parse_material(&path, m)?);
                    }
                }
                "roles" => {
                    for (role, to) in v.as_object().ok_or_else(|| style_err("roles", "must be an object: {\"coat\": PRESET}"))? {
                        let to = to.as_str().ok_or_else(|| style_err(&format!("roles.{role}"), "must be a material preset's name"))?;
                        style.roles.insert(role.clone(), to.to_string());
                    }
                }
                other => return Err(RecipeError::new(other, ErrorCode::UnknownField, "style accepts style, description, proposals, palette, materials, roles")),
            }
        }
        // (Checked once every preset is read: `roles` may come before `materials`.)
        for (role, to) in &style.roles {
            if !style.materials.contains_key(to) {
                let names: Vec<&str> = style.materials.keys().map(String::as_str).collect();
                return Err(RecipeError::new(format!("roles.{role}"), ErrorCode::UnknownMaterial, format!("'{to}' is not a preset ({})", names.join(", "))));
            }
        }
        Ok(style)
    }

    /// The preset a role is drawn with: the preset of the role's own name, else the one `roles`
    /// names; else refused with the presets nearest in colour to `hint` (sRGB, `#rrggbb`).
    pub fn role(&self, role: &str, hint: Option<&str>) -> Result<&str, RoleMissing> {
        if let Some((name, _)) = self.materials.get_key_value(role) {
            return Ok(name);
        }
        if let Some(to) = self.roles.get(role) {
            return Ok(to);
        }
        Err(RoleMissing { role: role.to_string(), nearest: self.nearest_presets(hint.and_then(parse_hex), ROLE_NEAREST) })
    }

    /// Up to `n` presets nearest in colour to `hint` (squared distance in sRGB, ties by name),
    /// or the first `n` by name without one.
    pub fn nearest_presets(&self, hint: Option<Rgba>, n: usize) -> Vec<String> {
        let mut all: Vec<(f32, &String)> = self
            .materials
            .iter()
            .map(|(name, m)| (hint.map_or(0.0, |h| (0..3).map(|i| (m.color[i] - h[i]) * (m.color[i] - h[i])).sum()), name))
            .collect();
        all.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        all.into_iter().take(n).map(|(_, name)| name.clone()).collect()
    }

    /// A colour by `#hex`, palette name or material name (its base colour).
    pub fn color(&self, name: &str) -> Option<Rgba> {
        if name.starts_with('#') {
            return parse_hex(name);
        }
        self.palette.get(name).copied().or_else(|| self.materials.get(name).map(|m| m.color))
    }
}

fn parse_material(path: &str, m: &Json) -> Result<MaterialPreset, RecipeError> {
    let obj = m.as_object().ok_or_else(|| style_err(path, "material must be an object"))?;
    let mut p = MaterialPreset {
        kind: MaterialKind::Standard,
        color: [1.0; 4],
        roughness: 0.8,
        metallic: 0.0,
        emissive: 0.0,
        specular: 0.25,
        opacity: 1.0,
    };
    for (k, v) in obj {
        let fpath = format!("{path}.{k}");
        let num = |lo: f32, hi: f32| {
            v.as_f64()
                .map(|x| x as f32)
                .filter(|x| *x >= lo && *x <= hi)
                .ok_or_else(|| RecipeError::new(fpath.as_str(), ErrorCode::OutOfRange, format!("must be a number in {lo}..={hi}")))
        };
        match k.as_str() {
            "kind" => {
                p.kind = match v.as_str() {
                    Some("standard") => MaterialKind::Standard,
                    Some("glass") => MaterialKind::Glass,
                    _ => return Err(style_err(&fpath, "must be \"standard\" or \"glass\"")),
                }
            }
            "color" => p.color = v.as_str().and_then(parse_hex).ok_or_else(|| style_err(&fpath, "must be \"#rrggbb\""))?,
            "roughness" => p.roughness = num(0.0, 1.0)?,
            "metallic" => p.metallic = num(0.0, 1.0)?,
            "emissive" => p.emissive = num(0.0, 100.0)?,
            "specular" => p.specular = num(0.0, 1.0)?,
            "opacity" => p.opacity = num(0.0, 1.0)?,
            "notes" => {}
            _ => return Err(RecipeError::new(fpath, ErrorCode::UnknownField, "material fields: kind, color, roughness, metallic, emissive, specular, opacity, notes")),
        }
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_palette_and_materials() {
        let s = Style::parse(
            r##"{"style":"t","palette":{"pale":"#b8fff6"},"materials":{"stone":{"color":"#8c96a3","roughness":0.75},
                "ghost":{"kind":"glass","color":"#9cff4f80","opacity":0.35}}}"##,
        )
        .unwrap();
        assert_eq!(s.color("pale").unwrap()[0], 0xb8 as f32 / 255.0);
        assert_eq!(s.color("stone").unwrap()[2], 0xa3 as f32 / 255.0);
        assert_eq!(s.color("#ff000080").unwrap(), [1.0, 0.0, 0.0, 128.0 / 255.0]);
        assert_eq!(s.materials["ghost"].kind, MaterialKind::Glass);
        assert!(s.color("nope").is_none());
        assert!(parse_hex("#12345").is_none() && parse_hex("123456").is_none() && parse_hex("#gg0000").is_none());
    }

    #[test]
    fn roles_resolve_by_name_then_table_else_nearest() {
        let s = Style::parse(r##"{"roles":{"coat":"fur"},"materials":{"fur":{"color":"#776a5a"},"bone":{"color":"#e0d8c0"},"claw":{"color":"#2a2420"},"ice":{"color":"#a0d0ff"}}}"##).unwrap();
        assert_eq!(s.role("claw", None), Ok("claw"), "a preset of the role's own name");
        assert_eq!(s.role("coat", Some("#000000")), Ok("fur"), "through the table");
        let miss = s.role("underside", Some("#b8ab98")).unwrap_err();
        assert_eq!(miss.nearest, ["bone", "fur", "ice"], "nearest by colour to the hint");
        assert!(miss.to_string().contains("\"roles\"") && miss.to_string().contains("underside"), "{miss}");
        assert_eq!(s.role("eye", None).unwrap_err().nearest, ["bone", "claw", "fur"], "by name with no hint");
        let bad = Style::parse(r##"{"roles":{"coat":"felt"},"materials":{"fur":{"color":"#776a5a"}}}"##).unwrap_err();
        assert_eq!((bad.code, bad.path.as_str()), (ErrorCode::UnknownMaterial, "roles.coat"));
        assert_eq!(Style::parse(r#"{"roles":{"coat":3}}"#).unwrap_err().path, "roles.coat");
    }

    #[test]
    fn rejects_bad_fields() {
        assert_eq!(Style::parse(r##"{"materials":{"a":{"colour":"#000000"}}}"##).unwrap_err().code, ErrorCode::UnknownField);
        assert_eq!(Style::parse(r#"{"palette":{"a":"red"}}"#).unwrap_err().path, "palette.a");
        assert_eq!(Style::parse("{").unwrap_err().code, ErrorCode::Json);
    }
}

//! What the rules measure about one built asset: its name, category and parameters, triangles per
//! level of detail, size, texture bytes, materials, and its surface colours weighed by area.

use crate::colour::{from8, hex, Rgb};
use specula_core::Json;
use specula_model::{MaterialKind, Model};
use std::collections::BTreeMap;

/// Distinct surface colours kept per asset (the largest by area; the rest are summed).
pub const MAX_COLOURS: usize = 4096;

/// What a recipe file says about itself (read leniently: only these fields).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeInfo {
    pub name: Option<String>,
    pub category: Option<String>,
    /// Each top-level part's name (`None`: unnamed).
    pub parts: Vec<Option<String>>,
    pub params: Vec<ParamInfo>,
    /// Every colour the recipe names (`color`, `from`, `to`, `colors` anywhere in it), sorted:
    /// the palette blends it may legitimately make across families.
    pub colours: Vec<String>,
}

/// Colour names in a recipe's JSON (any depth; bounded by the recipe's own size).
fn colour_names(v: &Json, out: &mut std::collections::BTreeSet<String>) {
    match v {
        Json::Obj(pairs) => {
            for (k, x) in pairs {
                match (k.as_str(), x) {
                    ("color" | "from" | "to", Json::Str(s)) => {
                        out.insert(s.clone());
                    }
                    ("colors", Json::Arr(items)) => out.extend(items.iter().filter_map(Json::as_str).map(str::to_string)),
                    _ => colour_names(x, out),
                }
            }
        }
        Json::Arr(items) => items.iter().for_each(|x| colour_names(x, out)),
        _ => {}
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParamInfo {
    pub name: String,
    pub default: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub integer: bool,
}

impl RecipeInfo {
    /// From a recipe's JSON text; `None` when it isn't a JSON object (a glTF source, say).
    pub fn parse(text: &str) -> Option<RecipeInfo> {
        let j = Json::parse(text).ok()?;
        j.as_object()?;
        let s = |k: &str| j.get(k).and_then(Json::as_str).map(str::to_string);
        let parts = j.get("parts").and_then(Json::as_array).map(|ps| ps.iter().map(|p| p.get("name").and_then(Json::as_str).map(str::to_string)).collect()).unwrap_or_default();
        let params = j
            .get("params")
            .and_then(Json::as_object)
            .map(|ps| {
                ps.iter()
                    .map(|(n, p)| ParamInfo {
                        name: n.clone(),
                        default: p.get("default").and_then(Json::as_f64).unwrap_or(0.0),
                        min: p.get("min").and_then(Json::as_f64),
                        max: p.get("max").and_then(Json::as_f64),
                        integer: p.get("integer").and_then(Json::as_bool).unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut colours = std::collections::BTreeSet::new();
        colour_names(&j, &mut colours);
        Some(RecipeInfo { name: s("name"), category: s("category"), parts, params, colours: colours.into_iter().collect() })
    }

    /// The parameter sets a style check covers: the defaults, crossed with every value of each
    /// integer parameter that has at most `max_values` values (a stone's tiers 1..3). At most
    /// `cap` sets, in a fixed order.
    pub fn variants(&self, max_values: usize, cap: usize) -> Vec<Vec<(String, f64)>> {
        let mut sets: Vec<Vec<(String, f64)>> = vec![Vec::new()];
        for p in &self.params {
            let values: Vec<f64> = match (p.integer, p.min, p.max) {
                (true, Some(lo), Some(hi)) if hi >= lo && (hi - lo) < max_values as f64 => (0..=(hi - lo) as i64).map(|k| lo + k as f64).collect(),
                _ => continue,
            };
            let mut next = Vec::new();
            for s in &sets {
                for v in &values {
                    let mut t = s.clone();
                    t.push((p.name.clone(), *v));
                    next.push(t);
                }
            }
            sets = next;
            if sets.len() >= cap {
                sets.truncate(cap);
                break;
            }
        }
        for s in &mut sets {
            s.sort_by(|a, b| a.0.cmp(&b.0));
        }
        sets
    }
}

/// A material preset an asset uses.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialFact {
    pub name: String,
    pub emissive: f32,
    pub glass: bool,
}

/// One distinct surface colour (sRGB) of one material, and the area it covers (m²).
#[derive(Clone, Debug, PartialEq)]
pub struct ColourArea {
    pub rgb: [u8; 3],
    pub material: usize,
    pub area: f32,
}

impl ColourArea {
    pub fn rgb(&self) -> Rgb {
        from8(self.rgb)
    }
}

/// The measurable facts of one built asset.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssetFacts {
    /// How findings name it: `warding_stone` or `warding_stone tier=3`.
    pub subject: String,
    pub name: String,
    /// The recipe file's stem (`warding_stone` for `recipes/warding_stone.json`).
    pub file_stem: Option<String>,
    pub category: Option<String>,
    pub params: Vec<(String, f64)>,
    pub lod_triangles: Vec<usize>,
    /// Width, height and depth: the bounds' extents, metres.
    pub size_m: [f32; 3],
    /// How high the lowest point is above the ground (y = 0), metres: 0 for things standing on it.
    pub float_m: f32,
    /// How deep the lowest point is below the ground, metres: 0 for things standing on it.
    pub sink_m: f32,
    /// A base-colour texture colours some of the surface (glTF): vertex colours don't tell the
    /// colours, so the palette rules aren't measured.
    pub textured: bool,
    pub texture_bytes: u64,
    pub materials: Vec<MaterialFact>,
    /// Largest first.
    pub colours: Vec<ColourArea>,
    pub total_area: f32,
    /// Top-level part names from the recipe (empty for glTF).
    pub part_names: Vec<Option<String>>,
    pub rigged: bool,
    /// The colour names the recipe uses (see [`RecipeInfo::colours`]).
    pub named_colours: Vec<String>,
}

/// `name k=v k=v` (parameters sorted, whole numbers without a fraction).
pub fn subject_name(name: &str, params: &[(String, f64)]) -> String {
    let mut s = name.to_string();
    for (k, v) in params {
        if v.fract() == 0.0 && v.abs() < 1e15 {
            s.push_str(&format!(" {k}={}", *v as i64));
        } else {
            s.push_str(&format!(" {k}={v}"));
        }
    }
    s
}

impl AssetFacts {
    /// The facts of a built model. `shown_params`: the parameters the caller set (they name the
    /// subject); `info`: the recipe's own fields, when it is a recipe.
    pub fn from_model(model: &Model, info: Option<&RecipeInfo>, file_stem: Option<&str>, shown_params: &[(String, f64)], texture_bytes: u64) -> AssetFacts {
        let materials: Vec<MaterialFact> = model.materials.iter().map(|(n, p)| MaterialFact { name: n.clone(), emissive: p.emissive, glass: p.kind == MaterialKind::Glass }).collect();
        let mut by: BTreeMap<([u8; 3], usize), f64> = BTreeMap::new();
        let mut total = 0.0f64;
        if let Some(m) = model.lods.first() {
            for (pi, part) in m.parts.iter().enumerate() {
                let mi = materials.iter().position(|f| f.name == part.material).unwrap_or(usize::MAX);
                let idx = m.part_indices(pi);
                for t in idx.as_chunks::<3>().0 {
                    let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
                    let (Some(pa), Some(pb), Some(pc)) = (m.positions.get(a), m.positions.get(b), m.positions.get(c)) else { continue };
                    let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
                    let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
                    let cr = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                    let area = 0.5 * ((cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]) as f64).sqrt();
                    if !area.is_finite() {
                        continue;
                    }
                    total += area;
                    for &vi in &[a, b, c] {
                        let col = m.colors.get(vi).map_or([255, 255, 255], |c| [c[0], c[1], c[2]]);
                        *by.entry((col, mi)).or_insert(0.0) += area / 3.0;
                    }
                }
            }
        }
        let mut colours: Vec<ColourArea> = by.into_iter().map(|((rgb, material), a)| ColourArea { rgb, material, area: a as f32 }).collect();
        // Largest first; ties in a fixed order (colour, then material).
        colours.sort_by(|x, y| y.area.total_cmp(&x.area).then(x.rgb.cmp(&y.rgb)).then(x.material.cmp(&y.material)));
        colours.truncate(MAX_COLOURS);
        let b = model.bounds;
        // The extents; how far the lowest point is above the ground (recipes stand on y = 0, a
        // hovering wraith's hem doesn't) is reported apart, so floating can't stand in for size.
        let (size, float_m, sink_m) = if b.max[0] >= b.min[0] { ([b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]], b.min[1].max(0.0), (-b.min[1]).max(0.0)) } else { ([0.0; 3], 0.0, 0.0) };
        let name = info.and_then(|i| i.name.clone()).unwrap_or_else(|| model.name.clone());
        AssetFacts {
            subject: subject_name(&name, shown_params),
            name,
            file_stem: file_stem.map(str::to_string),
            category: info.and_then(|i| i.category.clone()),
            params: model.params.clone(),
            lod_triangles: model.lods.iter().map(|l| l.triangle_count()).collect(),
            size_m: size,
            float_m,
            sink_m,
            textured: false,
            texture_bytes,
            materials,
            colours,
            total_area: total as f32,
            part_names: info.map(|i| i.parts.clone()).unwrap_or_default(),
            rigged: model.rig.is_some(),
            named_colours: info.map(|i| i.colours.clone()).unwrap_or_default(),
        }
    }

    /// Height over the larger footprint side.
    pub fn aspect(&self) -> f32 {
        self.size_m[1] / self.size_m[0].max(self.size_m[2]).max(1e-6)
    }

    /// A bounded summary.
    pub fn to_json(&self, colours: usize) -> Json {
        let r = |x: f32| Json::from((x as f64 * 1000.0).round() / 1000.0);
        let mut j = Json::obj([
            ("subject", Json::from(self.subject.as_str())),
            ("name", Json::from(self.name.as_str())),
            ("category", self.category.as_deref().map(Json::from).unwrap_or(Json::Null)),
            ("triangles", Json::arr(self.lod_triangles.iter().map(|t| Json::from(*t)))),
            ("size_m", Json::arr(self.size_m.iter().map(|x| r(*x)))),
            ("float_m", r(self.float_m)),
            ("sink_m", r(self.sink_m)),
            ("aspect", r(self.aspect())),
            ("texture_kb", r(self.texture_bytes as f32 / 1024.0)),
            ("materials", Json::arr(self.materials.iter().map(|m| Json::from(m.name.as_str())))),
        ]);
        if colours > 0 && self.total_area > 0.0 {
            j.set(
                "colours",
                Json::arr(self.colours.iter().take(colours).map(|c| {
                    Json::obj([
                        ("hex", Json::from(hex(c.rgb()))),
                        ("material", self.materials.get(c.material).map_or(Json::Null, |m| Json::from(m.name.as_str()))),
                        ("share", r(c.area / self.total_area)),
                    ])
                })),
            );
        }
        j
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_info_and_variants() {
        let i = RecipeInfo::parse(r##"{"name": "warding_stone", "category": "stone", "params": {"tier": {"default": 1, "min": 1, "max": 3, "integer": true}, "lean": {"default": 0.1}},
            "parts": [{"name": "plinth", "color": "carved_shadow", "ops": [{"op": "gradient", "from": "mist", "to": "grave_deep"}, {"op": "bands", "colors": ["moss", "#102030"]}]}, {"shape": {}}]}"##)
        .unwrap();
        assert_eq!(i.colours, ["#102030", "carved_shadow", "grave_deep", "mist", "moss"]);
        assert_eq!(i.category.as_deref(), Some("stone"));
        assert_eq!(i.parts, vec![Some("plinth".to_string()), None]);
        let v = i.variants(4, 16);
        assert_eq!(v, vec![vec![("tier".to_string(), 1.0)], vec![("tier".to_string(), 2.0)], vec![("tier".to_string(), 3.0)]]);
        assert_eq!(i.variants(2, 16), vec![Vec::<(String, f64)>::new()]);
        assert_eq!(i.variants(4, 2).len(), 2);
        assert!(RecipeInfo::parse("[1, 2]").is_none() && RecipeInfo::parse("glTF").is_none());
        assert_eq!(subject_name("warding_stone", &[("tier".into(), 3.0)]), "warding_stone tier=3");
        assert_eq!(subject_name("tombstone", &[("tilt".into(), 0.5)]), "tombstone tilt=0.5");
    }
}

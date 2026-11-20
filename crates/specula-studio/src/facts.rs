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

impl AssetFacts {
    /// The coarsest of its measured round shapes whose facets are seen at its size (they stand
    /// in from the true curve by [`crate::form::FACET_SEEN`] of its greatest width or more):
    /// the one that turns the most, by [`specula_model::Round::held`] (the greater of its faces'
    /// turn and, a ring's, its widest rim's seen down its axis, of those seen). `None` when no
    /// facet shows.
    pub fn faceting(&self) -> Option<&specula_model::Round> {
        self.faceting_held().map(|(r, _, _)| r)
    }

    /// [`AssetFacts::faceting`], with the turn it is held to and which turn that is.
    pub fn faceting_held(&self) -> Option<(&specula_model::Round, f32, specula_model::facet::By)> {
        let seen = self.width_m * crate::form::FACET_SEEN;
        self.round
            .iter()
            .filter(|r| r.measured())
            .filter_map(|r| r.held(seen).map(|(t, by)| (r, t, by)))
            .max_by(|a, b| a.1.total_cmp(&b.1).then(a.0.sag_seen().total_cmp(&b.0.sag_seen())))
    }

    /// Its round shapes whose facets were not measured (a taper, a twist or a bend after them;
    /// a shape other than a ring stretched unevenly).
    pub fn faceting_unmeasured(&self) -> impl Iterator<Item = &specula_model::Round> {
        self.round.iter().filter(|r| !r.measured())
    }
}

/// How close [`greatest_width`] comes to the true greatest width: never further below it than
/// this share of it (within [`WIDTH_PAIRS`]).
pub const WIDTH_TOLERANCE: f64 = 1e-6;

/// Most pairs of points [`greatest_width`] compares after its sweeps (past them it answers the
/// greatest found so far, never less than the sweeps found).
pub const WIDTH_PAIRS: u64 = 20_000_000;

/// The greatest width of a set of points, metres: the greatest distance between two of them,
/// to [`WIDTH_TOLERANCE`].
///
/// First a lower bound by sweeps (from a point to the one furthest from it, and on, four times,
/// started from the first point and from the point furthest from the centroid); then every
/// pair that could beat it is compared: with the points in order of their distance from the
/// centroid, two points are no further apart than the sum of those distances, so a pair whose
/// sum is within the tolerance of the best found need not be looked at, and nor need any after
/// it. A plus of two bars, whose sweeps can stay on the shorter bar, is found whole this way.
/// It depends on distances only, so it is the same however the points are turned (to the
/// tolerance). It costs the sweeps (eight passes over the points), a sort, and the pairs left:
/// none for a ball or a box, whose sweeps find the width; at most [`WIDTH_PAIRS`] of them,
/// past which it is never less than the sweeps' width, which is never less than half the true.
pub fn greatest_width(points: &[[f32; 3]]) -> f32 {
    let mut p: Vec<[f64; 3]> = points.iter().filter(|p| p.iter().all(|x| x.is_finite())).map(|p| p.map(f64::from)).collect();
    if p.is_empty() {
        return 0.0;
    }
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])).then(a[2].total_cmp(&b[2])));
    p.dedup();
    let d2 = |a: &[f64; 3], b: &[f64; 3]| (a[0] - b[0]) * (a[0] - b[0]) + (a[1] - b[1]) * (a[1] - b[1]) + (a[2] - b[2]) * (a[2] - b[2]);
    let n = p.len() as f64;
    let c = p.iter().fold([0.0f64; 3], |s, q| [s[0] + q[0] / n, s[1] + q[1] / n, s[2] + q[2] / n]);
    let mut by_reach: Vec<(f64, usize)> = p.iter().enumerate().map(|(i, q)| (d2(q, &c).sqrt(), i)).collect();
    by_reach.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let sweep = |start: usize| {
        let (mut from, mut best) = (start, 0.0f64);
        for _ in 0..4 {
            let (far, dist) = p.iter().enumerate().fold((from, 0.0f64), |m, (i, q)| {
                let x = d2(&p[from], q);
                if x > m.1 {
                    (i, x)
                } else {
                    m
                }
            });
            best = best.max(dist);
            from = far;
        }
        best
    };
    let mut best = sweep(0).max(sweep(by_reach[0].1)).sqrt();
    let mut pairs = 0u64;
    'outer: for (i, &(ri, a)) in by_reach.iter().enumerate() {
        if ri + by_reach[0].0 <= best * (1.0 + WIDTH_TOLERANCE) {
            break;
        }
        for &(rj, b) in &by_reach[..i] {
            if ri + rj <= best * (1.0 + WIDTH_TOLERANCE) {
                break;
            }
            pairs += 1;
            if pairs > WIDTH_PAIRS {
                break 'outer;
            }
            best = best.max(d2(&p[a], &p[b]).sqrt());
        }
    }
    best as f32
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
    /// Its form, measured on its finest level ([`crate::form`]).
    pub form: crate::form::FormFacts,
    /// Its greatest width, metres, on its finest level ([`greatest_width`]): its size whichever
    /// way it is turned.
    pub width_m: f32,
    /// The shapes its recipe names round, and how coarsely each was built, measured in the
    /// finished model: what a mesh alone cannot say ([`specula_model::Round`]; at most
    /// [`specula_model::MAX_ROUND`], in the order [`AssetFacts::faceting`] reads them). Empty
    /// for an imported mesh, which has no recipe to say what was meant to be round.
    pub round: Vec<specula_model::Round>,
    /// How many round shapes have a triangle of their own in it (`round` keeps the ones the
    /// answer needs first).
    pub round_total: usize,
    /// How many of them were not measured (whatever `round` keeps).
    pub round_not_measured: usize,
    /// How many round shapes its build made past what one build records (counted in
    /// `round_total`, never measured: the check fails on them as not measured).
    pub round_not_recorded: usize,
    /// What its recipe is built of: each shape it names with its own outlines and its surface
    /// ([`specula_model::Made`]), and the surface nothing of that speaks for. Read as shape
    /// language by [`crate::built::built_of`] when a check or an answer wants it (some
    /// milliseconds an outline, at most [`crate::built::MAX_READS`] of them: not at every
    /// staging). Bounded by the model (a quarter of a megabyte at the most).
    pub made: Vec<specula_model::Made>,
    pub made_rest_m2: f32,
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
        AssetFacts::from_model_with(model, info, file_stem, shown_params, texture_bytes, &|| false)
    }

    /// [`AssetFacts::from_model`], asking `stop` whether to give up measuring the mesh's form
    /// (the costly part on a large mesh: a call out of time; the facts then say its form was
    /// not measured).
    pub fn from_model_with(model: &Model, info: Option<&RecipeInfo>, file_stem: Option<&str>, shown_params: &[(String, f64)], texture_bytes: u64, stop: &dyn Fn() -> bool) -> AssetFacts {
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
            form: model.lods.first().map(|m| crate::form::measure_with(m, stop)).unwrap_or_default(),
            width_m: model.lods.first().map_or(0.0, |m| greatest_width(&m.positions)),
            round: model.extras.round.clone(),
            round_total: model.extras.round_total,
            round_not_measured: model.extras.round_not_measured,
            round_not_recorded: model.extras.round_not_recorded,
            made: model.extras.made.clone(),
            made_rest_m2: model.extras.made_rest_m2,
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
        j.set("form", self.form.to_json());
        if !self.round.is_empty() || self.round_not_recorded > 0 {
            // What is seen of it: the coarsest of the measured round shapes whose facets show at
            // the asset's size (by the greater of its faces' turn and its rim's seen down its
            // axis, of those that show); the first few in the order that is read (those not
            // measured, then those that are the answer at some size); and how many there are,
            // were kept, and could not be recorded.
            let held = self.faceting_held();
            let mut f = Json::obj([
                ("turn_deg", Json::from(held.map_or(0.0, |(_, t, _)| (f64::from(t) * 100.0).round() / 100.0))),
                ("width_m", r(self.width_m)),
                ("round", Json::arr(self.round.iter().take(8).map(|r| r.to_json()))),
                ("round_shapes", Json::from(self.round_total)),
                ("round_kept", Json::from(self.round.len())),
                ("not_measured", Json::from(self.round_not_measured.max(self.faceting_unmeasured().count()) + self.round_not_recorded)),
                ("not_recorded", Json::from(self.round_not_recorded)),
                ("level", Json::from("the finest")),
            ]);
            match held {
                Some((worst, _, by)) => {
                    f.set("worst", worst.to_json());
                    f.set("by", Json::from(if by == specula_model::facet::By::Rim { "its rim, seen along its axis" } else { "its faces" }));
                }
                None => {
                    f.set("note", Json::from("no measured round shape's facets show at the asset's size"));
                }
            }
            j.set("faceting", f);
        }
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

    /// **The verdict does not move as the asset is turned**: its size is its greatest width
    /// (the same whichever way it lies), not its box's longest side. A ten-metre bar beside a
    /// twelve-sided column, the column's facets 17 mm in: whether they show is the same at
    /// every turn from nothing to a quarter, in steps of 7.5 degrees, and the width is the
    /// bar's length throughout. The width is held against truths: a bar's length, a ball's and
    /// a cube's diagonal.
    #[test]
    fn faceting_does_not_move_as_the_asset_is_turned() {
        let style = specula_model::Style::parse(r##"{ "style": "t", "palette": { "pale": "#b8fff6" }, "materials": { "stone": { "color": "#8c96a3", "roughness": 0.8 } } }"##).unwrap();
        let facts = |deg: f64| {
            let text = format!(r#"{{ "name": "t", "parts": [ {{ "name": "bar", "material": "stone", "shape": {{"op":"box","size":[10,0.5,0.5]}}, "ops":[{{"op":"rotate","deg":[0,{deg},0]}}] }}, {{ "name": "c", "material": "stone", "shape": {{"op":"cylinder","radius":0.5,"height":0.5,"sides":12}} }} ] }}"#);
            let m = specula_model::build(&text, &specula_model::BuildParams::new(&style)).unwrap();
            AssetFacts::from_model(&m, RecipeInfo::parse(&text).as_ref(), Some("t"), &[], 0)
        };
        let straight = facts(0.0);
        let shows = straight.faceting().is_some();
        for k in 0..=12 {
            let f = facts(7.5 * f64::from(k));
            let bar = (100.0f32 + 0.25 + 0.25).sqrt();
            assert!((f.width_m - bar).abs() < 1e-3 * bar, "turned {}: width {}", 7.5 * f64::from(k), f.width_m);
            assert_eq!(f.faceting().is_some(), shows, "turned {} degrees", 7.5 * f64::from(k));
        }
        // (The 17 mm the column stands in, against a 512th of 10 m: 19.5 mm. Not seen.)
        assert!(!shows);
        let cube = specula_model::Shape::cuboid([1.0, 2.0, 3.0], 0.0).unwrap();
        assert!((greatest_width(&cube.positions) - 14.0f32.sqrt()).abs() < 1e-5);
        let ball = specula_model::Shape::sphere(0.5, 3);
        assert!((greatest_width(&ball.positions) - 1.0).abs() < 1e-3);
        assert_eq!(greatest_width(&[]), 0.0);
    }

    /// **The greatest width is the greatest distance between two points**, to its tolerance:
    /// a plus of two bars, the shorter first (whose sweeps stay on the shorter bar), and drawn
    /// clouds of points (bars, crosses, stars, blobs) held against every pair (distances only:
    /// it is the same however they are turned).
    #[test]
    fn the_greatest_width_is_found_whole() {
        let plus: Vec<[f32; 3]> = [[-0.5, 0.0, 0.0], [0.5, 0.0, 0.0], [0.0, 0.0, -0.8], [0.0, 0.0, 0.8], [0.0, 0.01, 0.0]].to_vec();
        assert!((greatest_width(&plus) - 1.6).abs() < 1e-6, "{}", greatest_width(&plus));
        let mut seed = 0x51ad_u64;
        let mut f = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        for case in 0..60 {
            // (Arms of drawn lengths from near the centre, each a little cloud.)
            let arms = 1 + case % 6;
            let mut pts: Vec<[f32; 3]> = Vec::new();
            for _ in 0..arms {
                let dir = [f() - 0.5, f() - 0.5, f() - 0.5];
                let l = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt().max(1e-9);
                let reach = 0.2 + 3.0 * f();
                for k in 0..40 {
                    let t = reach * f64::from(k) / 39.0;
                    pts.push([0, 1, 2].map(|i| (dir[i] / l * t + 0.05 * (f() - 0.5)) as f32));
                }
            }
            let brute = pts.iter().flat_map(|a| pts.iter().map(move |b| ((0..3).map(|i| f64::from(a[i] - b[i]).powi(2)).sum::<f64>()).sqrt())).fold(0.0f64, f64::max);
            let w = f64::from(greatest_width(&pts));
            assert!(w <= brute * (1.0 + 1e-6) && w >= brute * (1.0 - 2e-6), "case {case}: {w} against {brute}");
        }
        // Small clouds where sweeps alone stop short from every start (counted here, by sweeps
        // of this test's own): the pairs compared after them find the greatest.
        let dist = |a: &[f32; 3], b: &[f32; 3]| ((0..3).map(|i| f64::from(a[i] - b[i]) * f64::from(a[i] - b[i])).sum::<f64>()).sqrt();
        let mut short = 0;
        for case in 0..3000 {
            let pts: Vec<[f32; 3]> = (0..7).map(|_| [f() as f32, f() as f32, (0.3 * f()) as f32]).collect();
            let brute = pts.iter().flat_map(|a| pts.iter().map(move |b| dist(a, b))).fold(0.0f64, f64::max);
            let swept = (0..pts.len())
                .map(|start| {
                    let (mut from, mut best) = (start, 0.0f64);
                    for _ in 0..4 {
                        let (far, d) = pts.iter().enumerate().map(|(i, q)| (i, dist(&pts[from], q))).fold((from, 0.0), |m, x| if x.1 > m.1 { x } else { m });
                        best = best.max(d);
                        from = far;
                    }
                    best
                })
                .fold(f64::INFINITY, f64::min);
            if swept < brute * (1.0 - 1e-3) {
                short += 1;
            }
            let w = f64::from(greatest_width(&pts));
            assert!(w >= brute * (1.0 - 2e-6), "small case {case}: {w} against {brute}");
        }
        assert!(short >= 10, "the sweeps stopped short in only {short} clouds: the pairs were not put to the test");
    }
}

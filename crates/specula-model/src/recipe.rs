//! Recipes: JSON descriptions of a model as parts made of a shape and a list of operations.
//!
//! See the crate docs for the format. Parsing validates the **whole** recipe (every field of
//! every op, including ones switched off by `when`) before anything is built, so an agent gets
//! all structural errors up front, each with a path like `parts[2].ops[0].amount`.

use std::collections::{BTreeMap, BTreeSet};

use specula_core::hash::StableHasher;
use specula_core::rng::Rng;
use specula_core::Json;

use crate::collider::{self, Collider, ColliderKind};
use crate::dmath;
use crate::error::{ErrorCode, RecipeError};
use crate::expr::Expr;
use crate::lod::{self, LodInput};
use crate::made;
use crate::math3::{self, Affine, V2, V3};
use crate::mesh::{Bounds, MeshData};
use crate::poly;
use crate::rig::{Bone, Rig, MAX_BONES};
use crate::shape::{Gradient, Jitter, Rgba, Shape};
use crate::style::{MaterialPreset, Style};

/// The recipe format version this crate reads.
pub const FORMAT_VERSION: u32 = 1;
// Safety limits. Recipes are agent-written input: every size that drives allocation or time is
// bounded, and counts are *predicted* before a shape or op is generated, so a hostile recipe
// gets a structured `Limit` error instead of exhausting memory.

/// Triangles in one part (any shape, any op).
pub const MAX_PART_TRIANGLES: usize = 65_536;
/// Triangles in a whole model (LOD 0), counted across parts and includes as they are built.
pub const MAX_MODEL_TRIANGLES: usize = 131_072;
/// Deepest include nesting.
pub const MAX_INCLUDE_DEPTH: usize = 4;
/// Include expansions in one build (all levels together).
pub const MAX_INCLUDES: usize = 64;
/// Parts in one recipe.
pub const MAX_PARTS: usize = 256;
/// Ops in one part's `ops` or in `finish`.
pub const MAX_OPS: usize = 64;
/// Rows in a lathe profile, loft stations or a sweep path (before smoothing).
pub const MAX_ROWS: usize = 512;
/// Points in an outline after resampling and bevelling.
pub const MAX_OUTLINE_POINTS: usize = 1024;
/// Points in a sweep profile.
pub const MAX_PROFILE_POINTS: usize = 256;
/// Rings in a sweep after smoothing.
pub const MAX_SWEEP_RINGS: usize = 4096;
/// Largest coordinate (metres) any vertex may reach.
pub const MAX_EXTENT_M: f32 = 10_000.0;

// ---------------------------------------------------------------------------------------------
// Schema

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Num,
    Int,
    Bool,
    Axis,
    Color,
    Vec2,
    Vec3,
    /// A number (uniform) or a 3-vector.
    Scale,
    Nums,
    /// A list of number lists, each 2–4 long.
    Rows,
    Colors,
    Outline,
    Jitter,
    /// Boolean operands: shape nodes, each with optional `ops` (tier 2).
    Operands,
    /// An SDF node or a list of them (tier 3).
    Sdf,
    Sdfs,
    /// A sub-object with its own schema, or a list of them.
    Obj(&'static [Field]),
    Objs(&'static [Field]),
    /// A material preset name of the style.
    Material,
    /// One of these strings.
    Choice(&'static [&'static str]),
    /// One shape node with optional `ops` (scatter items).
    ShapeOps,
    /// Parameter ranges: `{name: n | [min, max]}`.
    Ranges,
    /// An identifier (a part name, a texture name).
    Name,
    /// A form's parameters by name (`args` of a `form` shape or outline: `recipe::form`).
    FormArgs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Field {
    name: &'static str,
    kind: Kind,
    required: bool,
}

const fn o(name: &'static str, kind: Kind) -> Field {
    Field { name, kind, required: false }
}
const fn r(name: &'static str, kind: Kind) -> Field {
    Field { name, kind, required: true }
}

use Kind::*;

const OUTLINE: &[Field] = &[
    o("points", Rows),
    o("rect", Vec2),
    o("sides", Int),
    o("radius", Num),
    o("arch", Vec2),
    o("segments", Int),
    o("star", Int),
    o("inner", Num),
    o("rotate", Num),
    o("bevel", Num),
    o("roundness", Num),
    o("resample", Int),
    o("jitter", Num),
    o("scale", Vec2),
    o("offset", Vec2),
    o("form", Name),
    o("args", FormArgs),
    o("detail", Int),
];
const JITTER: &[Field] = &[o("offset", Vec3), o("rotate", Vec3), o("scale", Num)];

/// Shape generators (a part's `shape`).
const SHAPES: &[(&str, &[Field])] = &[
    ("box", &[r("size", Vec3), o("bevel", Num)]),
    ("cylinder", &[r("radius", Num), r("height", Num), o("sides", Int), o("bevel", Num), o("radius_top", Num)]),
    ("cone", &[r("radius", Num), r("height", Num), o("sides", Int)]),
    ("sphere", &[r("radius", Num), o("subdivisions", Int)]),
    ("torus", &[r("radius", Num), r("tube", Num), o("segments", Int), o("sides", Int)]),
    ("capsule", &[r("radius", Num), r("height", Num), o("segments", Int), o("rings", Int)]),
    ("plane", &[r("size", Vec2), o("divisions", Vec2)]),
    (
        "extrude",
        &[
            r("outline", Outline),
            r("height", Num),
            o("taper", Num),
            o("bevel", Num),
            o("corner_bevel", Num),
            o("segments", Int),
            o("cap_inset", Vec2),
        ],
    ),
    ("loft", &[r("outline", Outline), r("stations", Rows), o("corner_bevel", Num)]),
    ("lathe", &[r("profile", Rows), o("segments", Int), o("caps", Bool)]),
    (
        "sweep",
        &[
            r("profile", Outline),
            r("path", Rows),
            o("scales", Nums),
            o("twist", Num),
            o("closed", Bool),
            o("caps", Bool),
            o("up", Vec3),
            o("smooth", Int),
        ],
    ),
    ("union", procgen::CSG_FIELDS),
    ("difference", procgen::CSG_FIELDS),
    ("intersection", procgen::CSG_FIELDS),
    ("sdf", procgen::SDF_FIELDS),
    ("figure", procgen::FIGURE_FIELDS),
    ("form", form::FORM_FIELDS),
];

/// Operations (a part's `ops`, the recipe's `finish`).
const OPS: &[(&str, &[Field])] = &[
    ("move", &[r("by", Vec3)]),
    ("rotate", &[r("deg", Vec3), o("pivot", Vec3)]),
    ("scale", &[r("by", Scale), o("pivot", Vec3)]),
    ("mirror", &[r("axis", Axis), o("keep", Bool)]),
    ("array", &[r("count", Int), r("offset", Vec3), o("jitter", Jitter)]),
    ("radial", &[r("count", Int), o("axis", Vec3), o("angle", Num), o("jitter", Jitter)]),
    ("subdivide", &[o("levels", Int)]),
    ("displace", &[r("amount", Num), o("scale", Num), o("octaves", Int), o("axes", Vec3)]),
    ("taper", &[o("axis", Axis), o("from", Num), r("to", Num)]),
    ("twist", &[r("deg_per_m", Num)]),
    ("lean", &[o("x", Num), o("z", Num)]),
    ("bend", &[r("deg_per_m", Num)]),
    ("weld", &[o("eps", Num)]),
    ("color", &[r("color", Color)]),
    ("gradient", &[o("axis", Axis), r("from", Color), r("to", Color), o("range", Vec2), o("absolute", Bool), o("strength", Num), o("keep_outside", Bool)]),
    ("bands", &[o("axis", Axis), r("colors", Colors), r("width", Num), o("wobble", Num)]),
    ("noise_tint", &[r("amount", Num), o("scale", Num)]),
    ("ao", &[o("amount", Num)]),
    ("ground_shade", &[o("amount", Num), o("height", Num)]),
    ("bevel_tint", &[r("color", Color), o("amount", Num)]),
];

/// Names of every shape generator (for docs and tools).
pub fn shape_names() -> Vec<&'static str> {
    SHAPES.iter().map(|(n, _)| *n).collect()
}

/// A shape generator's fields as the recipe's own field table declares them: `(name, required,
/// what it takes)`, for tools that show them (`formae_list`'s figure); `None` for no such
/// generator.
pub fn shape_fields(name: &str) -> Option<Vec<(&'static str, bool, &'static str)>> {
    SHAPES.iter().find(|(n, _)| *n == name).map(|(_, fields)| fields.iter().map(|f| (f.name, f.required, f.kind.word())).collect())
}

impl Kind {
    /// What a field of this kind takes, in a word or two.
    fn word(self) -> &'static str {
        match self {
            Num => "number",
            Int => "whole number",
            Bool => "true or false",
            Axis => "axis",
            Color => "colour",
            Vec2 => "[x, y]",
            Vec3 => "[x, y, z]",
            Scale => "number or [x, y, z]",
            Nums => "numbers",
            Rows => "rows of numbers",
            Colors => "colours",
            Outline => "outline",
            Jitter => "jitter",
            Operands => "shapes",
            Sdf | Sdfs => "distance-field nodes",
            Obj(_) => "object",
            Objs(_) => "objects",
            Material => "material",
            Choice(_) => "one of a list",
            ShapeOps => "a shape",
            Ranges => "ranges",
            Name => "name",
            FormArgs => "a form's parameters",
        }
    }
}

/// Names of every operation.
pub fn op_names() -> Vec<&'static str> {
    OPS.iter().map(|(n, _)| *n).collect()
}

// ---------------------------------------------------------------------------------------------
// Parsed recipe

#[derive(Clone, Debug)]
enum Val {
    Num(Expr),
    Bool(bool),
    Str(String),
    List(Vec<Val>),
    Obj(Node),
}

#[derive(Clone, Debug)]
struct Node {
    path: String,
    op: String,
    when: Option<Expr>,
    fields: Vec<(String, Val)>,
}

impl Node {
    fn get(&self, name: &str) -> Option<&Val> {
        self.fields.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }
    fn at(&self, name: &str) -> String {
        format!("{}.{name}", self.path)
    }
}

/// A declared parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamDef {
    pub name: String,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub integer: bool,
}

#[derive(Clone, Debug)]
enum Source {
    Shape(Node),
    Include { asset: String, params: Vec<(String, Expr)> },
    /// A tier-4 generator (building, scatter, terrain): several pieces.
    Generate(Node),
}

#[derive(Clone, Debug)]
struct PartDef {
    path: String,
    name: String,
    material: Option<String>,
    color: Option<String>,
    when: Option<Expr>,
    source: Source,
    ops: Vec<Node>,
    smooth_angle: Option<Expr>,
    lod_detail: Option<Expr>,
    /// Bones this part's vertices may bind to (names in the recipe's rig; empty = the root).
    bones: Vec<String>,
    /// Joint blend band in metres (overrides the rig's `blend`).
    blend: Option<Expr>,
    /// A texture graph (by name in `textures`) and its metres per repeat.
    texture: Option<String>,
    uv_scale: Option<Expr>,
}

/// A bone as written in the recipe (a `mirror` bone also produces a twin with `mirror_x`).
#[derive(Clone, Debug)]
struct BoneDef {
    path: String,
    name: String,
    parent: Option<usize>,
    head: Val,
    tail: Option<Val>,
    /// The `_r` twin of a mirrored `_l` bone: X is negated.
    mirror_x: bool,
}

#[derive(Clone, Debug)]
struct RigDef {
    blend: Option<Expr>,
    bones: Vec<BoneDef>,
    /// A rig made from a measured body (`"figure"`): the bones are [`crate::figure::BONES`]
    /// and their places come from the body when the recipe is built.
    figure: Option<Node>,
}

/// A parsed, validated recipe (not yet built).
#[derive(Clone, Debug)]
pub struct Recipe {
    pub name: String,
    pub category: Option<String>,
    pub seed: u64,
    pub params: Vec<ParamDef>,
    lets: Vec<(String, Expr)>,
    max_triangles: Option<Expr>,
    min_triangles: Option<Expr>,
    pub lod_ratios: Vec<f32>,
    pub collider: ColliderKind,
    smooth_angle: Option<Expr>,
    parts: Vec<PartDef>,
    finish: Vec<Node>,
    rig: Option<RigDef>,
    /// Texture graphs by name: parsed inline, or a library recipe's name.
    textures: Vec<(String, TexSource)>,
    /// How the model sways in the world's wind (foliage), if at all.
    pub wind: Option<WindDef>,
    /// Paint strokes, applied to the built model's vertex colours in order ([`PaintEdit`]).
    pub edits: Vec<PaintEdit>,
}

/// Most strokes a recipe's `"edits"` hold (as a terrain's).
pub const MAX_PAINT_EDITS: usize = 512;
/// A stroke's radius, metres.
pub const PAINT_RADIUS: (f64, f64) = (0.005, 64.0);
/// How far from the model's origin a stroke may be centred, metres.
pub const PAINT_REACH: f64 = 10_000.0;

/// One of a recipe's `"edits"`: a paint stroke as data, `{"op": "paint", "at": [x, y, z],
/// "radius": m, "color": "#rrggbb" or a palette name, "amount": 0..1}`. Every vertex of the built
/// model within `radius` of `at` (model space, metres) takes the colour, fully at the centre and
/// easing to nothing at the rim, times `amount`; alpha (the wind's painted weight) is left as it
/// is. Strokes apply in order, after the parts are built and before the LODs are made, so every
/// LOD carries them. Textured parts don't read vertex colours and are left alone. A recipe's
/// edits are its own model's: they are not applied where another recipe includes it.
///
/// This is the editor's vertex-paint tool as data (docs/design/EDITOR.md, "Draw tools"): a stroke
/// is a line in the recipe's file, so it is journaled, undoable and readable like the rest.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintEdit {
    pub at: [f32; 3],
    pub radius: f32,
    /// A palette or material name, or `#rrggbb` (known to the style the recipe was parsed with).
    pub color: String,
    pub amount: f32,
}

fn paint_edits(v: &Json, style: &Style) -> Result<Vec<PaintEdit>, RecipeError> {
    let items = v.as_array().ok_or_else(|| type_err("edits", "a list of strokes {op, at, radius, color, amount}"))?;
    if items.len() > MAX_PAINT_EDITS {
        return Err(e("edits", ErrorCode::Limit, format!("{} strokes; at most {MAX_PAINT_EDITS} (bake them into the parts' colours, or start the list again)", items.len()))
            .with_numbers(items.len() as f64, MAX_PAINT_EDITS as f64));
    }
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let at_path = |k: &str| format!("edits[{i}].{k}");
        let o = item.as_object().ok_or_else(|| type_err(&format!("edits[{i}]"), "an object {op, at, radius, color, amount}"))?;
        for (k, _) in o {
            if !["op", "at", "radius", "color", "amount", "note"].contains(&k.as_str()) {
                return Err(e(&at_path(k), ErrorCode::UnknownField, "a stroke accepts: op, at, radius, color, amount, note"));
            }
        }
        let g = |k: &str| o.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        if let Some(op) = g("op") {
            if op.as_str() != Some("paint") {
                return Err(e(&at_path("op"), ErrorCode::OutOfRange, "the only op is \"paint\""));
            }
        }
        let at = g("at").and_then(Json::as_array).filter(|a| a.len() == 3).ok_or_else(|| e(&at_path("at"), ErrorCode::MissingField, "a stroke needs \"at\": [x, y, z] in the model's metres"))?;
        let mut centre = [0.0f32; 3];
        for (k, c) in at.iter().enumerate() {
            let x = c.as_f64().filter(|x| x.is_finite()).ok_or_else(|| type_err(&at_path("at"), "three finite numbers"))?;
            if x.abs() > PAINT_REACH {
                return Err(e(&at_path("at"), ErrorCode::OutOfRange, format!("a stroke is centred within {PAINT_REACH} m of the model's origin")).with_numbers(x, PAINT_REACH));
            }
            centre[k] = x as f32;
        }
        let radius = g("radius").and_then(Json::as_f64).ok_or_else(|| e(&at_path("radius"), ErrorCode::MissingField, "a stroke needs a \"radius\" in metres"))?;
        if !(PAINT_RADIUS.0..=PAINT_RADIUS.1).contains(&radius) {
            return Err(e(&at_path("radius"), ErrorCode::OutOfRange, format!("radius must be {}..{} m", PAINT_RADIUS.0, PAINT_RADIUS.1)).with_numbers(radius, PAINT_RADIUS.1));
        }
        let color = g("color").and_then(Json::as_str).ok_or_else(|| e(&at_path("color"), ErrorCode::MissingField, "a stroke needs a \"color\": #rrggbb or a palette name"))?;
        if style.color(color).is_none() {
            return Err(e(&at_path("color"), ErrorCode::OutOfRange, "not #rrggbb, nor a colour or material of the style"));
        }
        let amount = match g("amount") {
            None => 1.0,
            Some(a) => a.as_f64().filter(|a| (0.0..=1.0).contains(a)).ok_or_else(|| e(&at_path("amount"), ErrorCode::OutOfRange, "amount must be 0..1").with_numbers(a.as_f64().unwrap_or(f64::NAN), 1.0))?,
        };
        out.push(PaintEdit { at: centre, radius: radius as f32, color: color.to_owned(), amount: amount as f32 });
    }
    Ok(out)
}

/// Applies a recipe's paint strokes to its built parts (see [`PaintEdit`]). Work: every stroke
/// looks at every vertex once (charged before it starts).
fn apply_paint(edits: &[PaintEdit], built: &mut [BuiltPart], style: &Style, tally: &Tally) -> Result<(), RecipeError> {
    if edits.is_empty() {
        return Ok(());
    }
    let vertices: u64 = built.iter().filter(|b| b.uv_scale.is_none()).map(|b| b.shape.positions.len() as u64).sum();
    charge(tally, "edits", vertices.saturating_mul(edits.len() as u64).saturating_mul(crate::work::PAINT_VERTEX))?;
    for ed in edits {
        let color = style.color(&ed.color).unwrap_or([1.0; 4]);
        let r2 = ed.radius * ed.radius;
        for b in built.iter_mut().filter(|b| b.uv_scale.is_none()) {
            for (p, c) in b.shape.positions.iter().zip(b.shape.colors.iter_mut()) {
                let (dx, dy, dz) = (p[0] - ed.at[0], p[1] - ed.at[1], p[2] - ed.at[2]);
                let d2 = dx * dx + dy * dy + dz * dz;
                if d2 >= r2 {
                    continue;
                }
                // 1 at the centre, 0 at the rim, eased (no square root: the same on every platform).
                let t = 1.0 - d2 / r2;
                let w = ed.amount * t * t * (3.0 - 2.0 * t);
                for k in 0..3 {
                    c[k] += (color[k] - c[k]) * w;
                }
            }
        }
    }
    Ok(())
}

/// A recipe's `"wind"`: its draws sway in the world's wind (`world_env {wind}`), weighted by
/// height (0 at y = 0, rising as the square to 1 at `height`) or by paint (the vertex colour's
/// alpha). Deterministic in simulation time; the scene, the sun's shadow and sight's ID buffer
/// all move the same vertices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindDef {
    /// Metres of sway at full weight in a strength-1 wind, 0.005..=1.
    pub amount: f32,
    /// Where the weight reaches 1 (m, 0.03..=8); `None`: the model's height.
    pub height: Option<f32>,
    pub painted: bool,
}

/// A built model's wind term (see [`WindDef`]), its height resolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wind {
    pub amount: f32,
    pub height: f32,
    pub painted: bool,
}

fn wind_def(v: &Json) -> Result<WindDef, RecipeError> {
    let o = v.as_object().ok_or_else(|| type_err("wind", "an object: {amount, height, painted}"))?;
    for (k, _) in o {
        if !["amount", "height", "painted", "note"].contains(&k.as_str()) {
            return Err(e(&format!("wind.{k}"), ErrorCode::UnknownField, "wind accepts: amount, height, painted, note"));
        }
    }
    let g = |k: &str| o.iter().find(|(n, _)| n == k).map(|(_, v)| v);
    let num = |k: &str, lo: f64, hi: f64| -> Result<Option<f32>, RecipeError> {
        match g(k) {
            None => Ok(None),
            Some(v) => {
                let x = v.as_f64().ok_or_else(|| type_err(&format!("wind.{k}"), "a number"))?;
                if !(lo..=hi).contains(&x) {
                    return Err(e(&format!("wind.{k}"), ErrorCode::OutOfRange, format!("{k} must be {lo}..{hi}")).with_numbers(x, hi));
                }
                Ok(Some(x as f32))
            }
        }
    };
    let amount = num("amount", 0.005, 1.0)?.ok_or_else(|| e("wind.amount", ErrorCode::MissingField, "wind needs an \"amount\": metres of sway at the tips in a breeze (0.005..1; grass about 0.08)"))?;
    let height = num("height", 0.03, 8.0)?;
    let painted = match g("painted") {
        None => false,
        Some(v) => v.as_bool().ok_or_else(|| type_err("wind.painted", "true or false"))?,
    };
    Ok(WindDef { amount, height, painted })
}

#[derive(Clone, Debug)]
enum TexSource {
    Inline(Box<crate::texgraph::TexGraph>),
    Library(String),
}

fn e(path: &str, code: ErrorCode, msg: impl Into<String>) -> RecipeError {
    RecipeError::new(path, code, msg)
}

fn type_err(path: &str, want: &str) -> RecipeError {
    e(path, ErrorCode::WrongType, format!("expected {want}"))
}

struct Parser<'a> {
    style: &'a Style,
    known: BTreeSet<String>,
}

impl Parser<'_> {
    fn expr(&self, path: &str, v: &Json) -> Result<Expr, RecipeError> {
        let ex = match v {
            Json::Num(x) => {
                if !x.is_finite() {
                    return Err(e(path, ErrorCode::OutOfRange, "number must be finite"));
                }
                Expr::Num(*x)
            }
            Json::Str(s) => Expr::parse(s).map_err(|er| e(path, ErrorCode::Expression, format!("{} in \"{s}\"", er.message)).with_offset(er.offset))?,
            _ => return Err(type_err(path, "a number or an expression string")),
        };
        let mut names = Vec::new();
        ex.vars(&mut names);
        if let Some(bad) = names.iter().find(|n| !self.known.contains(*n)) {
            let known: Vec<&str> = self.known.iter().map(|s| s.as_str()).collect();
            return Err(e(path, ErrorCode::UnknownName, format!("unknown name '{bad}' (known: {})", known.join(", "))));
        }
        Ok(ex)
    }

    fn color_name(&self, path: &str, v: &Json) -> Result<String, RecipeError> {
        let s = v.as_str().ok_or_else(|| type_err(path, "a colour string (\"#rrggbb\" or a palette name)"))?;
        if self.style.color(s).is_none() {
            let mut names: Vec<&str> = self.style.palette.keys().map(|k| k.as_str()).collect();
            names.extend(self.style.materials.keys().map(|k| k.as_str()));
            return Err(e(path, ErrorCode::UnknownColor, format!("'{s}' is not #rrggbb or a palette/material name ({})", names.join(", "))));
        }
        Ok(s.to_string())
    }

    fn list<'j>(&self, path: &str, v: &'j Json, want: &str) -> Result<&'j [Json], RecipeError> {
        v.as_array().ok_or_else(|| type_err(path, want))
    }

    fn value(&self, path: &str, kind: Kind, v: &Json) -> Result<Val, RecipeError> {
        Ok(match kind {
            Num | Int => Val::Num(self.expr(path, v)?),
            Bool => Val::Bool(v.as_bool().ok_or_else(|| type_err(path, "true or false"))?),
            Axis => match v.as_str() {
                Some(a @ ("x" | "y" | "z")) => Val::Str(a.to_string()),
                _ => return Err(type_err(path, "\"x\", \"y\" or \"z\"")),
            },
            Color => Val::Str(self.color_name(path, v)?),
            Vec2 | Vec3 => {
                let n = if kind == Vec2 { 2 } else { 3 };
                let items = self.list(path, v, &format!("a list of {n} numbers"))?;
                if items.len() != n {
                    return Err(type_err(path, &format!("a list of {n} numbers (got {})", items.len())));
                }
                Val::List(items.iter().enumerate().map(|(i, x)| self.expr(&format!("{path}[{i}]"), x).map(Val::Num)).collect::<Result<_, _>>()?)
            }
            Scale => match v {
                Json::Arr(_) => self.value(path, Vec3, v)?,
                _ => Val::Num(self.expr(path, v)?),
            },
            Nums => {
                let items = self.list(path, v, "a list of numbers")?;
                Val::List(items.iter().enumerate().map(|(i, x)| self.expr(&format!("{path}[{i}]"), x).map(Val::Num)).collect::<Result<_, _>>()?)
            }
            Rows => {
                let items = self.list(path, v, "a list of points")?;
                if items.is_empty() {
                    return Err(type_err(path, "at least one point"));
                }
                let mut rows = Vec::with_capacity(items.len());
                for (i, row) in items.iter().enumerate() {
                    let p = format!("{path}[{i}]");
                    let cells = self.list(&p, row, "a point [a, b] (2 to 4 numbers)")?;
                    if !(2..=4).contains(&cells.len()) {
                        return Err(type_err(&p, "a point with 2 to 4 numbers"));
                    }
                    rows.push(Val::List(cells.iter().enumerate().map(|(k, x)| self.expr(&format!("{p}[{k}]"), x).map(Val::Num)).collect::<Result<_, _>>()?));
                }
                Val::List(rows)
            }
            Colors => {
                let items = self.list(path, v, "a list of colours")?;
                if items.is_empty() {
                    return Err(type_err(path, "at least one colour"));
                }
                Val::List(items.iter().enumerate().map(|(i, x)| self.color_name(&format!("{path}[{i}]"), x).map(Val::Str)).collect::<Result<_, _>>()?)
            }
            Outline => match v {
                Json::Arr(_) => self.value(path, Rows, v)?,
                Json::Obj(_) => {
                    let n = self.node(path, v, "outline", OUTLINE, false)?;
                    let forms = ["points", "rect", "sides", "arch", "star", "form"].iter().filter(|f| n.get(f).is_some()).count();
                    if forms != 1 {
                        return Err(e(path, ErrorCode::WrongType, "an outline needs exactly one of points, rect, sides (+radius), arch, star (+radius, inner), form (+args, detail)"));
                    }
                    if n.get("form").is_some() {
                        // (A field that would do nothing beside a form is refused, never ignored: its
                        // own numbers are in `args`.)
                        if let Some(k) = ["radius", "inner", "segments", "roundness"].iter().find(|k| n.get(k).is_some()) {
                            return Err(e(&n.at(k), ErrorCode::Geometry, format!("\"{k}\" would do nothing beside a \"form\": the form's own numbers go in \"args\"")));
                        }
                        self.check_form(&n, true)?;
                    } else if let Some(k) = ["args", "detail"].iter().find(|k| n.get(k).is_some()) {
                        return Err(e(&n.at(k), ErrorCode::Geometry, format!("\"{k}\" is a \"form\" outline's: it would do nothing here")));
                    }
                    if (n.get("sides").is_some() || n.get("star").is_some()) && n.get("radius").is_none() {
                        return Err(e(&n.at("radius"), ErrorCode::MissingField, "sides/star outlines need a radius"));
                    }
                    Val::Obj(n)
                }
                _ => return Err(type_err(path, "an outline: a list of [x, z] points or an object")),
            },
            Jitter => Val::Obj(self.node(path, v, "jitter", JITTER, false)?),
            FormArgs => self.form_args(path, v)?,
            Operands | Sdf | Sdfs | Obj(_) | Objs(_) | Material | Choice(_) | ShapeOps | Ranges | Name => self.procgen_value(path, kind, v)?,
        })
    }

    /// Parses an object against `schema`. `with_op`: the object carries `op` and `when`.
    fn node(&self, path: &str, v: &Json, op: &str, schema: &[Field], with_op: bool) -> Result<Node, RecipeError> {
        let obj = v.as_object().ok_or_else(|| type_err(path, "an object"))?;
        let mut node = Node { path: path.to_string(), op: op.to_string(), when: None, fields: Vec::new() };
        for (k, val) in obj {
            let fpath = format!("{path}.{k}");
            if with_op && k == "op" {
                continue;
            }
            if with_op && k == "when" {
                node.when = Some(self.expr(&fpath, val)?);
                continue;
            }
            if k == "note" {
                continue; // free-text comment for humans and agents
            }
            let Some(f) = schema.iter().find(|f| f.name == k) else {
                let names: Vec<&str> = schema.iter().map(|f| f.name).collect();
                // An outline written as a library word (`{"heart": ...}`) is told how to name it.
                let hint = if op == "outline" { form::library_hint(k, false).map_or(String::new(), |h| format!("; {h}")) } else { String::new() };
                return Err(e(&fpath, ErrorCode::UnknownField, format!("'{k}' is not a field of '{op}'{hint}; it accepts: {}{}", names.join(", "), if with_op { ", when, note" } else { "" })));
            };
            if node.get(k).is_some() {
                return Err(e(&fpath, ErrorCode::WrongType, "duplicate field"));
            }
            node.fields.push((k.clone(), self.value(&fpath, f.kind, val)?));
        }
        for f in schema.iter().filter(|f| f.required) {
            if node.get(f.name).is_none() {
                return Err(e(&format!("{path}.{}", f.name), ErrorCode::MissingField, format!("'{op}' needs '{}'", f.name)));
            }
        }
        if op == "form" {
            self.check_form(&node, false)?;
        }
        Ok(node)
    }

    fn op_node(&self, path: &str, v: &Json, table: &[(&str, &[Field])], what: &str) -> Result<Node, RecipeError> {
        let obj = v.as_object().ok_or_else(|| type_err(path, "an object with an \"op\""))?;
        let name = obj
            .iter()
            .find(|(k, _)| k == "op")
            .and_then(|(_, v)| v.as_str())
            .ok_or_else(|| e(&format!("{path}.op"), ErrorCode::MissingField, format!("every {what} needs \"op\"")))?;
        let Some((_, schema)) = table.iter().find(|(n, _)| *n == name) else {
            let names: Vec<&str> = table.iter().map(|(n, _)| *n).collect();
            // A shape the modeller does not know may be one of the shape library's: said first,
            // with how to write it (or the library's nearest names).
            let hint = if what == "shape" { form::library_hint(name, true).map_or(String::new(), |h| format!("; {h}")) } else { String::new() };
            return Err(e(&format!("{path}.op"), ErrorCode::UnknownOp, format!("unknown {what} '{name}'{hint} (known {what}s: {})", names.join(", "))));
        };
        self.node(path, v, name, schema, true)
    }

    fn ops(&self, path: &str, v: Option<&Json>) -> Result<Vec<Node>, RecipeError> {
        let Some(v) = v else { return Ok(Vec::new()) };
        let items = v.as_array().ok_or_else(|| type_err(path, "a list of ops"))?;
        if items.len() > MAX_OPS {
            return Err(e(path, ErrorCode::Limit, format!("{} ops; at most {MAX_OPS}", items.len())).with_numbers(items.len() as f64, MAX_OPS as f64));
        }
        items.iter().enumerate().map(|(i, x)| self.op_node(&format!("{path}[{i}]"), x, OPS, "op")).collect()
    }

    /// The `rig` section: bones parents-first, `_l` bones with `"mirror": true` gain an `_r` twin
    /// right after them. Or `"figure"`, a measured body (the fields of the `figure` shape): its
    /// skeleton, under the names every figure has.
    fn rig(&self, v: &Json) -> Result<RigDef, RecipeError> {
        let o = v.as_object().ok_or_else(|| type_err("rig", "{\"bones\": [...], \"blend\": metres}"))?;
        let mut blend = None;
        let mut items = None;
        let mut figure = None;
        for (k, x) in o {
            match k.as_str() {
                "blend" => blend = Some(self.expr("rig.blend", x)?),
                "bones" => items = Some(x.as_array().ok_or_else(|| type_err("rig.bones", "a list of bones"))?),
                "figure" => figure = Some(self.node("rig.figure", x, "figure", procgen::FIGURE_FIELDS, false)?),
                "note" => {}
                _ => return Err(e(&format!("rig.{k}"), ErrorCode::UnknownField, "a rig accepts: bones, figure, blend, note")),
            }
        }
        if let Some(figure) = figure {
            if items.is_some() {
                return Err(e("rig.bones", ErrorCode::InvalidRig, "a rig has \"bones\" or a \"figure\", not both"));
            }
            // The places are the body's, found at build; the names and the tree are fixed.
            let bones = crate::figure::BONES
                .iter()
                .map(|(name, parent)| BoneDef {
                    path: "rig.figure".to_string(),
                    name: (*name).to_string(),
                    parent: parent.and_then(|p| crate::figure::BONES.iter().position(|(n, _)| *n == p)),
                    head: Val::List(Vec::new()),
                    tail: None,
                    mirror_x: false,
                })
                .collect();
            return Ok(RigDef { blend, bones, figure: Some(figure) });
        }
        let items = items.ok_or_else(|| e("rig.bones", ErrorCode::MissingField, "a rig needs \"bones\""))?;
        if items.is_empty() {
            return Err(e("rig.bones", ErrorCode::MissingField, "a rig needs at least one bone (the root)"));
        }
        let mut bones: Vec<BoneDef> = Vec::new();
        for (i, b) in items.iter().enumerate() {
            let path = format!("rig.bones[{i}]");
            let bo = b.as_object().ok_or_else(|| type_err(&path, "a bone object"))?;
            let (mut name, mut parent, mut head, mut tail, mut mirror) = (None, None, None, None, false);
            for (k, x) in bo {
                let fp = format!("{path}.{k}");
                match k.as_str() {
                    "name" => name = Some(x.as_str().ok_or_else(|| type_err(&fp, "a bone name"))?.to_string()),
                    "parent" => parent = Some(x.as_str().ok_or_else(|| type_err(&fp, "a bone name"))?.to_string()),
                    "head" => head = Some(self.value(&fp, Vec3, x)?),
                    "tail" => tail = Some(self.value(&fp, Vec3, x)?),
                    "mirror" => mirror = x.as_bool().ok_or_else(|| type_err(&fp, "true or false"))?,
                    "note" => {}
                    _ => return Err(e(&fp, ErrorCode::UnknownField, "a bone accepts: name, parent, head, tail, mirror, note")),
                }
            }
            let name = name.ok_or_else(|| e(&format!("{path}.name"), ErrorCode::MissingField, "a bone needs a \"name\""))?;
            if !is_ident(&name) {
                return Err(e(&format!("{path}.name"), ErrorCode::WrongType, "bone names are identifiers (letters, digits, _)"));
            }
            let head = head.ok_or_else(|| e(&format!("{path}.head"), ErrorCode::MissingField, "a bone needs a \"head\" (its pivot, [x, y, z] m)"))?;
            let parent = match parent {
                None if bones.is_empty() => None,
                None => return Err(e(&format!("{path}.parent"), ErrorCode::MissingField, "every bone after the first needs a parent (bone 0 is the root)")),
                Some(_) if bones.is_empty() => return Err(e(&format!("{path}.parent"), ErrorCode::InvalidRig, "the first bone is the root and has no parent")),
                Some(pn) => Some(bones.iter().position(|q| q.name == pn).ok_or_else(|| {
                    let known: Vec<&str> = bones.iter().map(|q| q.name.as_str()).collect();
                    e(&format!("{path}.parent"), ErrorCode::UnknownName, format!("parent '{pn}' is not an earlier bone (known: {})", known.join(", ")))
                })?),
            };
            let dup = |n: &str, bones: &[BoneDef]| -> Result<(), RecipeError> {
                if bones.iter().any(|q| q.name == n) {
                    return Err(e(&format!("{path}.name"), ErrorCode::Duplicate, format!("duplicate bone name '{n}'")));
                }
                Ok(())
            };
            dup(&name, &bones)?;
            bones.push(BoneDef { path: path.clone(), name: name.clone(), parent, head: head.clone(), tail: tail.clone(), mirror_x: false });
            if mirror {
                let Some(stem) = name.strip_suffix("_l") else {
                    return Err(e(&format!("{path}.mirror"), ErrorCode::InvalidRig, "a mirrored bone is named *_l (left, +X); its twin is *_r"));
                };
                let twin = format!("{stem}_r");
                dup(&twin, &bones)?;
                // The twin hangs from the parent's own twin when there is one.
                let twin_parent = parent.map(|pi| {
                    bones[pi].name.strip_suffix("_l").and_then(|s| bones.iter().position(|q| q.name == format!("{s}_r"))).unwrap_or(pi)
                });
                bones.push(BoneDef { path: format!("{path}.mirror"), name: twin, parent: twin_parent, head, tail, mirror_x: true });
            }
            if bones.len() > MAX_BONES {
                return Err(e(&path, ErrorCode::Limit, format!("more than {MAX_BONES} bones")).with_numbers(bones.len() as f64, MAX_BONES as f64));
            }
        }
        Ok(RigDef { blend, bones, figure: None })
    }
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_') && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn as_u64_exact(v: &Json) -> Option<u64> {
    let x = v.as_f64()?;
    ((0.0..=9_007_199_254_740_992.0).contains(&x) && dmath::trunc_f64(x) == x).then_some(x as u64)
}

impl Recipe {
    /// Its parts' names, in recipe order (at most `MAX_PARTS`): what a human can point at as
    /// "a part of this model" (the editor's "highlight and ask").
    pub fn part_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.parts.iter().map(|p| p.name.as_str())
    }

    /// Its parts whose shape is a sweep, by name, in recipe order: the parts a path can be
    /// sketched for (the editor's path tool; `model_path` counts them the same way).
    pub fn sweep_parts(&self) -> impl Iterator<Item = &str> + '_ {
        self.parts.iter().filter(|p| matches!(&p.source, Source::Shape(n) if n.op == "sweep")).map(|p| p.name.as_str())
    }

    /// Assets this recipe includes directly (file stems, sorted, each once), whether or not a
    /// part's `when` switches them on: what its build may read from the library.
    pub fn includes(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .parts
            .iter()
            .filter_map(|p| match &p.source {
                Source::Include { asset, .. } => Some(asset.clone()),
                Source::Shape(_) | Source::Generate(_) => None,
            })
            .chain(self.textures.iter().filter_map(|(_, t)| match t {
                TexSource::Library(n) => Some(n.clone()),
                TexSource::Inline(_) => None,
            }))
            .chain(self.parts.iter().filter_map(|p| match &p.source {
                Source::Generate(n) => procgen::generator_includes(n),
                _ => None,
            }).flatten())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Checks parameter values against the declared parameters exactly as a build does
    /// (unknown names, ranges, integers), without building.
    pub fn check_values(&self, values: &[(&str, f64)]) -> Result<(), RecipeError> {
        for (k, v) in values {
            let Some(def) = self.params.iter().find(|p| p.name == *k) else {
                let names: Vec<&str> = self.params.iter().map(|p| p.name.as_str()).collect();
                return Err(e(&format!("params.{k}"), ErrorCode::UnknownName, format!("'{}' has no parameter '{k}' (has: {})", self.name, names.join(", "))));
            };
            if !v.is_finite() || *v < def.min || *v > def.max {
                return Err(e(&format!("params.{k}"), ErrorCode::OutOfRange, format!("{v} is outside {}..{}", def.min, def.max)).with_numbers(*v, def.max));
            }
            if def.integer && dmath::trunc_f64(*v) != *v {
                return Err(e(&format!("params.{k}"), ErrorCode::OutOfRange, format!("'{k}' must be an integer, got {v}")));
            }
        }
        Ok(())
    }

    /// Parses and validates `text` against `style` (materials and colours must exist).
    pub fn parse(text: &str, style: &Style) -> Result<Recipe, RecipeError> {
        let json = Json::parse(text).map_err(|er| e("", ErrorCode::Json, er.message.clone()).with_offset(er.offset))?;
        crate::error::check_duplicate_keys(&json, "")?;
        let obj = json.as_object().ok_or_else(|| type_err("", "a recipe object"))?;
        if obj.iter().any(|(k, _)| k == "texgraph") {
            return Err(e("texgraph", ErrorCode::WrongType, "this file is a texture graph, not a recipe: use it from a recipe's \"textures\" (\"textures\": {\"stone\": \"<this file's name>\"}) and a part's \"texture\""));
        }
        const TOP: &[&str] =
            &["recipe", "name", "description", "category", "seed", "params", "let", "budget", "lods", "collider", "smooth_angle", "rig", "parts", "finish", "textures", "wind", "edits"];
        for (k, _) in obj {
            if !TOP.contains(&k.as_str()) {
                return Err(e(k, ErrorCode::UnknownField, format!("'{k}' is not a field of a recipe; it accepts: {}", TOP.join(", "))));
            }
        }
        let get = |k: &str| obj.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        if let Some(v) = get("recipe") {
            if v.as_f64() != Some(FORMAT_VERSION as f64) {
                return Err(e("recipe", ErrorCode::OutOfRange, format!("unsupported format version (this build reads {FORMAT_VERSION})")));
            }
        }
        let name = get("name").and_then(|v| v.as_str()).ok_or_else(|| e("name", ErrorCode::MissingField, "a recipe needs a \"name\" string"))?.to_string();
        let category = match get("category") {
            Some(v) => Some(v.as_str().ok_or_else(|| type_err("category", "a string"))?.to_string()),
            None => None,
        };
        let seed = match get("seed") {
            Some(v) => as_u64_exact(v).ok_or_else(|| e("seed", ErrorCode::OutOfRange, "seed must be an integer 0..2^53"))?,
            None => 0,
        };
        let mut p = Parser { style, known: BTreeSet::new() };
        // Parameters.
        let mut params = Vec::new();
        if let Some(v) = get("params") {
            let o = v.as_object().ok_or_else(|| type_err("params", "an object of parameter definitions"))?;
            for (pname, def) in o {
                let path = format!("params.{pname}");
                if !is_ident(pname) || pname == "pi" {
                    return Err(e(&path, ErrorCode::WrongType, "parameter names are identifiers (letters, digits, _)"));
                }
                let d = def.as_object().ok_or_else(|| type_err(&path, "{\"default\": n, \"min\": a, \"max\": b, \"integer\": bool}"))?;
                let mut pd = ParamDef { name: pname.clone(), default: 0.0, min: f64::MIN, max: f64::MAX, integer: false };
                let mut has_default = false;
                for (k, x) in d {
                    let fp = format!("{path}.{k}");
                    let num = || x.as_f64().filter(|v| v.is_finite()).ok_or_else(|| type_err(&fp, "a number"));
                    match k.as_str() {
                        "default" => {
                            pd.default = num()?;
                            has_default = true;
                        }
                        "min" => pd.min = num()?,
                        "max" => pd.max = num()?,
                        "integer" => pd.integer = x.as_bool().ok_or_else(|| type_err(&fp, "true or false"))?,
                        "description" => {}
                        _ => return Err(e(&fp, ErrorCode::UnknownField, "a parameter accepts: default, min, max, integer, description")),
                    }
                }
                if !has_default {
                    return Err(e(&format!("{path}.default"), ErrorCode::MissingField, "parameters need a default"));
                }
                if pd.min > pd.max {
                    return Err(e(&format!("{path}.min"), ErrorCode::OutOfRange, "min is above max").with_numbers(pd.min, pd.max));
                }
                if pd.default < pd.min || pd.default > pd.max {
                    return Err(e(&format!("{path}.default"), ErrorCode::OutOfRange, "default outside min..max").with_numbers(pd.default, pd.max));
                }
                if pd.integer && dmath::trunc_f64(pd.default) != pd.default {
                    return Err(e(&format!("{path}.default"), ErrorCode::OutOfRange, format!("'{pname}' is an integer parameter; default {} is not", pd.default)));
                }
                p.known.insert(pname.clone());
                params.push(pd);
            }
        }
        // Lets, in order: each may use parameters and earlier lets.
        let mut lets = Vec::new();
        if let Some(v) = get("let") {
            let o = v.as_object().ok_or_else(|| type_err("let", "an object of name: expression"))?;
            for (lname, x) in o {
                let path = format!("let.{lname}");
                if !is_ident(lname) || lname == "pi" || p.known.contains(lname) {
                    return Err(e(&path, ErrorCode::WrongType, "let names are new identifiers (not a parameter or earlier let)"));
                }
                lets.push((lname.clone(), p.expr(&path, x)?));
                p.known.insert(lname.clone());
            }
        }
        let (mut max_triangles, mut min_triangles) = (None, None);
        if let Some(v) = get("budget") {
            let o = v.as_object().ok_or_else(|| type_err("budget", "{\"max_triangles\": n, \"min_triangles\": n}"))?;
            for (k, x) in o {
                let path = format!("budget.{k}");
                match k.as_str() {
                    "max_triangles" => max_triangles = Some(p.expr(&path, x)?),
                    "min_triangles" => min_triangles = Some(p.expr(&path, x)?),
                    _ => return Err(e(&path, ErrorCode::UnknownField, "budget accepts: max_triangles, min_triangles")),
                }
            }
        }
        let lod_ratios = match get("lods") {
            Some(v) => {
                let items = v.as_array().ok_or_else(|| type_err("lods", "a list of ratios, e.g. [0.5, 0.25]"))?;
                let mut out = Vec::new();
                let mut prev = 1.0;
                for (i, x) in items.iter().enumerate() {
                    let r = x.as_f64().ok_or_else(|| type_err(&format!("lods[{i}]"), "a number"))?;
                    if !(r > 0.0 && r < prev) {
                        return Err(e(&format!("lods[{i}]"), ErrorCode::OutOfRange, "LOD ratios must decrease, each in (0, 1)").with_numbers(r, prev));
                    }
                    prev = r;
                    out.push(r as f32);
                }
                if out.len() > 4 {
                    return Err(e("lods", ErrorCode::Limit, "at most 4 extra LODs"));
                }
                out
            }
            None => vec![0.5, 0.25],
        };
        let collider = match get("collider") {
            Some(v) => v
                .as_str()
                .and_then(ColliderKind::parse)
                .ok_or_else(|| type_err("collider", "\"none\", \"aabb\", \"capsule\" or \"convex_hull\""))?,
            None => ColliderKind::ConvexHull,
        };
        let smooth_angle = get("smooth_angle").map(|v| p.expr("smooth_angle", v)).transpose()?;
        let rig = get("rig").map(|v| p.rig(v)).transpose()?;
        let mut textures = Vec::new();
        if let Some(v) = get("textures") {
            let o = v.as_object().ok_or_else(|| type_err("textures", "{name: a texture graph object or a library texture graph's name}"))?;
            if o.len() > 16 {
                return Err(e("textures", ErrorCode::Limit, format!("{} texture graphs; at most 16", o.len())).with_numbers(o.len() as f64, 16.0));
            }
            for (k, x) in o {
                let path = format!("textures.{k}");
                if !is_ident(k) {
                    return Err(e(&path, ErrorCode::WrongType, "texture names are identifiers"));
                }
                let src = match x {
                    Json::Str(lib) => TexSource::Library(lib.clone()),
                    Json::Obj(_) => TexSource::Inline(Box::new(crate::texgraph::TexGraph::parse_json(x, style, &path)?)),
                    _ => return Err(type_err(&path, "a texture graph object or a library texture graph's name")),
                };
                textures.push((k.clone(), src));
            }
        }
        // Parts.
        let parts_json = get("parts").ok_or_else(|| e("parts", ErrorCode::MissingField, "a recipe needs \"parts\""))?;
        let items = parts_json.as_array().ok_or_else(|| type_err("parts", "a list of parts"))?;
        if items.is_empty() {
            return Err(e("parts", ErrorCode::MissingField, "a recipe needs at least one part"));
        }
        if items.len() > MAX_PARTS {
            return Err(e("parts", ErrorCode::Limit, format!("{} parts; at most {MAX_PARTS}", items.len())).with_numbers(items.len() as f64, MAX_PARTS as f64));
        }
        let mut parts: Vec<PartDef> = Vec::new();
        for (i, pj) in items.iter().enumerate() {
            let path = format!("parts[{i}]");
            let po = pj.as_object().ok_or_else(|| type_err(&path, "a part object"))?;
            const PART: &[&str] =
                &["name", "material", "color", "when", "shape", "include", "generate", "params", "ops", "smooth_angle", "lod_detail", "bones", "blend", "texture", "uv_scale", "note"];
            for (k, _) in po {
                if !PART.contains(&k.as_str()) {
                    return Err(e(&format!("{path}.{k}"), ErrorCode::UnknownField, format!("'{k}' is not a field of a part; it accepts: {}", PART.join(", "))));
                }
            }
            let pget = |k: &str| po.iter().find(|(n, _)| n == k).map(|(_, v)| v);
            let pname = pget("name").and_then(|v| v.as_str()).ok_or_else(|| e(&format!("{path}.name"), ErrorCode::MissingField, "a part needs a \"name\""))?;
            if parts.iter().any(|q| q.name == pname) {
                return Err(e(&format!("{path}.name"), ErrorCode::WrongType, format!("duplicate part name '{pname}'")));
            }
            let material = match pget("material") {
                Some(v) => {
                    let m = v.as_str().ok_or_else(|| type_err(&format!("{path}.material"), "a material preset name"))?;
                    if !style.materials.contains_key(m) {
                        let names: Vec<&str> = style.materials.keys().map(|k| k.as_str()).collect();
                        return Err(e(&format!("{path}.material"), ErrorCode::UnknownMaterial, format!("'{m}' is not a preset ({})", names.join(", "))));
                    }
                    Some(m.to_string())
                }
                None => None,
            };
            let color = pget("color").map(|v| p.color_name(&format!("{path}.color"), v)).transpose()?;
            let when = pget("when").map(|v| p.expr(&format!("{path}.when"), v)).transpose()?;
            let source = match (pget("shape"), pget("include"), pget("generate")) {
                (None, None, Some(g)) => {
                    if pget("params").is_some() {
                        return Err(e(&format!("{path}.params"), ErrorCode::UnknownField, "params belong to include parts"));
                    }
                    let node = p.op_node(&format!("{path}.generate"), g, procgen::GENERATORS, "generator")?;
                    if material.is_none() && node.op != "building" {
                        return Err(e(&format!("{path}.material"), ErrorCode::MissingField, format!("a {} part needs a material preset (scatter: the default for its shape items)", node.op)));
                    }
                    Source::Generate(node)
                }
                (Some(s), None, None) => {
                    if material.is_none() {
                        return Err(e(&format!("{path}.material"), ErrorCode::MissingField, "a shape part needs a material preset"));
                    }
                    if pget("params").is_some() {
                        return Err(e(&format!("{path}.params"), ErrorCode::UnknownField, "params belong to include parts"));
                    }
                    Source::Shape(p.op_node(&format!("{path}.shape"), s, SHAPES, "shape")?)
                }
                (None, Some(inc), None) => {
                    if material.is_some() || color.is_some() {
                        return Err(e(&format!("{path}.material"), ErrorCode::UnknownField, "include parts keep the included recipe's materials"));
                    }
                    let asset = inc.as_str().ok_or_else(|| type_err(&format!("{path}.include"), "an asset name"))?.to_string();
                    let mut ps = Vec::new();
                    if let Some(pv) = pget("params") {
                        for (k, x) in pv.as_object().ok_or_else(|| type_err(&format!("{path}.params"), "an object"))? {
                            ps.push((k.clone(), p.expr(&format!("{path}.params.{k}"), x)?));
                        }
                    }
                    Source::Include { asset, params: ps }
                }
                _ => return Err(e(&path, ErrorCode::MissingField, "a part needs exactly one of \"shape\", \"include\" or \"generate\"")),
            };
            let ops = p.ops(&format!("{path}.ops"), pget("ops"))?;
            let smooth_angle = pget("smooth_angle").map(|v| p.expr(&format!("{path}.smooth_angle"), v)).transpose()?;
            let lod_detail = pget("lod_detail").map(|v| p.expr(&format!("{path}.lod_detail"), v)).transpose()?;
            let mut bones = Vec::new();
            if let Some(bv) = pget("bones") {
                let bpath = format!("{path}.bones");
                let Some(rig) = &rig else {
                    return Err(e(&bpath, ErrorCode::MissingField, "a part's bones need a \"rig\" section in the recipe"));
                };
                let names: Vec<&Json> = match bv {
                    Json::Str(_) => vec![bv],
                    Json::Arr(items) if !items.is_empty() => items.iter().collect(),
                    _ => return Err(type_err(&bpath, "a bone name or a non-empty list of bone names")),
                };
                for (k, n) in names.iter().enumerate() {
                    let np = if matches!(bv, Json::Arr(_)) { format!("{bpath}[{k}]") } else { bpath.clone() };
                    let n = n.as_str().ok_or_else(|| type_err(&np, "a bone name"))?;
                    if !rig.bones.iter().any(|b| b.name == n) {
                        let known: Vec<&str> = rig.bones.iter().map(|b| b.name.as_str()).collect();
                        return Err(e(&np, ErrorCode::UnknownName, format!("no bone '{n}' in the rig (bones: {})", known.join(", "))));
                    }
                    if bones.iter().any(|b| b == n) {
                        return Err(e(&np, ErrorCode::Duplicate, format!("bone '{n}' listed twice")));
                    }
                    bones.push(n.to_string());
                }
            }
            let blend = pget("blend").map(|v| p.expr(&format!("{path}.blend"), v)).transpose()?;
            if blend.is_some() && rig.is_none() {
                return Err(e(&format!("{path}.blend"), ErrorCode::MissingField, "blend needs a \"rig\" section in the recipe"));
            }
            let texture = match pget("texture") {
                Some(v) => {
                    let t = v.as_str().ok_or_else(|| type_err(&format!("{path}.texture"), "a texture name from the recipe's \"textures\""))?;
                    if !textures.iter().any(|(n, _)| n == t) {
                        let known: Vec<&str> = textures.iter().map(|(n, _)| n.as_str()).collect();
                        return Err(e(&format!("{path}.texture"), ErrorCode::UnknownName, format!("no texture '{t}' in the recipe's \"textures\" (has: {})", known.join(", "))));
                    }
                    if matches!(source, Source::Include { .. }) {
                        return Err(e(&format!("{path}.texture"), ErrorCode::UnknownField, "include parts keep the included recipe's textures"));
                    }
                    Some(t.to_string())
                }
                None => None,
            };
            let uv_scale = pget("uv_scale").map(|v| p.expr(&format!("{path}.uv_scale"), v)).transpose()?;
            parts.push(PartDef { path, name: pname.to_string(), material, color, when, source, ops, smooth_angle, lod_detail, bones, blend, texture, uv_scale });
        }
        let finish = p.ops("finish", get("finish"))?;
        let wind = get("wind").map(wind_def).transpose()?;
        let edits = get("edits").map(|v| paint_edits(v, style)).transpose()?.unwrap_or_default();
        Ok(Recipe { name, category, seed, params, lets, max_triangles, min_triangles, lod_ratios, collider, smooth_angle, parts, finish, rig, textures, wind, edits })
    }
}

// ---------------------------------------------------------------------------------------------
// Building

/// Inputs to [`build`].
#[derive(Clone, Copy, Debug)]
pub struct BuildParams<'a> {
    /// The game's palette and material presets.
    pub style: &'a Style,
    /// Parameter values (`tier` = 3); unspecified ones take their defaults.
    pub values: &'a [(&'a str, f64)],
    /// Overrides the recipe's `seed` (for variants).
    pub seed: Option<u64>,
    /// Recipes available to `include`, by name (file stem).
    pub library: Option<&'a BTreeMap<String, String>>,
}

impl<'a> BuildParams<'a> {
    pub fn new(style: &'a Style) -> BuildParams<'a> {
        BuildParams { style, values: &[], seed: None, library: None }
    }
}

/// A built model: LOD meshes, a collider and the material presets its parts use.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub name: String,
    /// `lods[0]` is full detail; later entries follow the recipe's `lods` ratios. A ratio whose
    /// mesh saves under 10% over the previous LOD is skipped, so there may be fewer entries.
    pub lods: Vec<MeshData>,
    /// Requested triangle ratio of each LOD (1.0 first).
    pub lod_ratios: Vec<f32>,
    pub collider: Collider,
    /// Every material preset `lods[0].parts` uses, resolved from the style.
    pub materials: Vec<(String, MaterialPreset)>,
    pub bounds: Bounds,
    /// Stable hash of every LOD and the collider (same recipe + params + seed ⇒ same hash on
    /// every platform).
    pub hash: u64,
    /// Parameter values used.
    pub params: Vec<(String, f64)>,
    pub seed: u64,
    /// Triangle budget the recipe declared, if any: (min, max).
    pub budget: (Option<u32>, Option<u32>),
    /// What the build changed or gave up on (skipped LODs, a hull that fell back to a box).
    pub notes: Vec<String>,
    /// The bone tree when the recipe has a `rig`; every LOD then carries per-vertex
    /// `joints`/`weights` into it. Included recipes' rigs are not merged (their parts bind to the
    /// including part's bones).
    pub rig: Option<Rig>,
    /// Tiers 2-4 output beyond the meshes: baked texture sets, a terrain, generator stats.
    pub extras: ModelExtras,
    /// The recipe's wind term (foliage), its height resolved.
    pub wind: Option<Wind>,
}

/// What procedural tiers 2-4 add to a [`Model`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelExtras {
    /// Baked texture sets by part material key (`preset+texture`, the name in `MeshData` parts).
    pub textures: Vec<(String, std::sync::Arc<crate::texgraph::TextureSet>)>,
    /// The first terrain a `terrain` generator made (heights for physics, navigation and a terrain
    /// renderer), in the part's space before its ops.
    pub terrain: Option<std::sync::Arc<crate::terrain::Terrain>>,
    /// Where that terrain sits in the model: the rigid transform its part's ops and the
    /// recipe's `finish` moved it by (colour ops don't), or `None` when an op reshaped it
    /// (displace, bend, ...), so its heights no longer describe the mesh. Colliders and
    /// scatters built from the heights apply this transform, or refuse.
    pub terrain_transform: Option<Affine>,
    /// Per generator or boolean: its path and what it did (counts, work), bounded.
    pub stats: Vec<(String, Json)>,
    /// The shapes the recipe names round, and how coarsely each was built, measured in the
    /// finished model ([`Round`], [`crate::facet`]): those with a triangle of their own left,
    /// at most [`MAX_ROUND`] but for every shape that is the coarsest seen at some size (all of
    /// those are kept), in the order the answer is read by (up to eight of the ones not
    /// measured first, then those that are the coarsest seen at some size, then the rest).
    pub round: Vec<Round>,
    /// How many round shapes have a triangle of their own in the finished model, whatever
    /// `round` keeps.
    pub round_total: usize,
    /// How many of them were not measured, whatever `round` keeps.
    pub round_not_measured: usize,
    /// What the recipe is built of ([`crate::Made`]): each shape it names with its own outlines
    /// and its own surface in the finished model, measured on the triangles that carry it
    /// ([`crate::Shape::owners`]: what its operations, a boolean and a scatter left of it), one
    /// record a path, the ones with the most surface first, at most [`crate::MAX_MADE`] and
    /// [`crate::made::MAX_RECORD_VIEWS`] outlines in all.
    pub made: Vec<crate::Made>,
    /// The surface nothing in `made` speaks for, square metres: a terrain, a building, an
    /// imported mesh, the walls cuts make (their cutters' surface), a bevel's strips, a
    /// sculpt's half spaces and its triangles not asked within
    /// [`crate::sdf::MAX_OWNER_BUILD_WORK`], and the shapes past the ones kept.
    pub made_rest_m2: f32,
}

pub use crate::facet::Round;

/// Round shapes kept for one model's facts.
pub const MAX_ROUND: usize = 64;

impl Model {
    /// A bounded summary for tool responses.
    pub fn report(&self) -> Json {
        let b = self.bounds;
        let v3 = |v: [f32; 3]| Json::arr(v.iter().map(|x| Json::Num(*x as f64)));
        Json::obj([
            ("name", Json::Str(self.name.clone())),
            ("hash", Json::Str(format!("{:016x}", self.hash))),
            ("seed", Json::Num(self.seed as f64)),
            ("params", Json::obj(self.params.iter().map(|(k, v)| (k.clone(), Json::Num(*v))))),
            (
                "lods",
                Json::arr(self.lods.iter().zip(&self.lod_ratios).map(|(m, r)| {
                    Json::obj([
                        ("ratio", Json::Num(*r as f64)),
                        ("triangles", Json::Num(m.triangle_count() as f64)),
                        ("vertices", Json::Num(m.vertex_count() as f64)),
                    ])
                })),
            ),
            ("bounds", Json::obj([("min", v3(b.min)), ("max", v3(b.max))])),
            ("materials", Json::arr(self.materials.iter().map(|(n, _)| Json::Str(n.clone())))),
            (
                "collider",
                Json::Str(
                    match &self.collider {
                        Collider::None => "none".to_string(),
                        Collider::Aabb { .. } => "aabb".to_string(),
                        Collider::Capsule { radius, .. } => format!("capsule r={radius:.3}"),
                        Collider::ConvexHull { points, .. } => format!("convex_hull {} points", points.len()),
                    },
                ),
            ),
            ("notes", Json::arr(self.notes.iter().map(|n| Json::Str(n.clone())))),
            (
                "rig",
                match &self.rig {
                    Some(r) => Json::arr(r.bones.iter().map(|b| Json::Str(b.name.clone()))),
                    None => Json::Null,
                },
            ),
            ("procgen", procgen::extras_report(&self.extras)),
        ])
    }
}

/// Builds `recipe_json` into a [`Model`]: the integration point for the engine and its tools.
///
/// ```text
/// let style = Style::parse(&std::fs::read_to_string("games/sepulcra/assets/style.json")?)?;
/// let model = build(&recipe_text, &BuildParams { values: &[("tier", 2.0)], ..BuildParams::new(&style) })?;
/// ```
pub fn build(recipe_json: &str, params: &BuildParams) -> Result<Model, RecipeError> {
    let recipe = Recipe::parse(recipe_json, params.style)?;
    build_recipe(&recipe, params)
}

/// The rig `recipe` builds with `params`, without building any geometry: the parameter values,
/// the `let`s and the bones only, so it is cheap (microseconds for a creature). The same rig as
/// [`build_recipe`]'s. `None` for a recipe without a rig.
pub fn build_rig_only(recipe: &Recipe, params: &BuildParams) -> Result<Option<Rig>, RecipeError> {
    let Some(def) = &recipe.rig else { return Ok(None) };
    let seed = params.seed.unwrap_or(recipe.seed);
    let vars = eval_vars(recipe, params.values)?;
    let (rig, _) = build_rig(recipe, def, &Ctx { vars: &vars, style: params.style, seed, tally: &Tally::default(), coarse: false, paint: std::cell::Cell::new([1.0; 4]) })?;
    Ok(Some(rig))
}

/// A recipe's variables: the values given, the other parameters' defaults, then the `let`s.
fn eval_vars(recipe: &Recipe, values: &[(&str, f64)]) -> Result<BTreeMap<String, f64>, RecipeError> {
    let mut vars: BTreeMap<String, f64> = BTreeMap::new();
    recipe.check_values(values)?;
    for (k, v) in values {
        vars.insert(k.to_string(), *v);
    }
    for p in &recipe.params {
        vars.entry(p.name.clone()).or_insert(p.default);
    }
    for (name, ex) in &recipe.lets {
        let v = ex.eval(&|n| vars.get(n).copied());
        if !v.is_finite() {
            return Err(e(&format!("let.{name}"), ErrorCode::OutOfRange, "evaluated to NaN or infinity"));
        }
        vars.insert(name.clone(), v);
    }
    Ok(vars)
}

/// Builds an already parsed recipe.
pub fn build_recipe(recipe: &Recipe, params: &BuildParams) -> Result<Model, RecipeError> {
    let seed = params.seed.unwrap_or(recipe.seed);
    let mut stack = vec![recipe.name.clone()];
    let tally = Tally::default();
    let (mut built, vars) = build_parts(recipe, params.values, seed, params, &mut stack, &tally, false)?;
    apply_paint(&recipe.edits, &mut built, params.style, &tally)?;
    let mut notes = Vec::new();
    let rig = match &recipe.rig {
        Some(def) => Some(build_rig(recipe, def, &Ctx { vars: &vars, style: params.style, seed, tally: &Tally::default(), coarse: false, paint: std::cell::Cell::new([1.0; 4]) })?),
        None => None,
    };
    // A figure's rig knows how thick each bone's flesh is: its parts bind by that.
    let flesh: Vec<f32> = match recipe.rig.as_ref().and_then(|def| def.figure.as_ref()) {
        Some(figure) => {
            let ctx = Ctx { vars: &vars, style: params.style, seed, tally: &Tally::default(), coarse: false, paint: std::cell::Cell::new([1.0; 4]) };
            crate::figure::flesh(&ctx.figure_body(figure)?.0)
        }
        None => Vec::new(),
    };
    let bindings: Vec<Binding> = match &rig {
        Some((r, blend)) => built
            .iter()
            .map(|b| Binding { bones: b.bones.iter().filter_map(|n| r.find(n)).collect(), blend: b.blend.unwrap_or(*blend), radii: flesh.clone() })
            .collect(),
        None => Vec::new(),
    };
    let skin = rig.as_ref().map(|(r, _)| (r, &bindings[..]));
    charge(&tally, "parts", built.iter().map(|b| b.shape.positions.len() as u64).sum::<u64>() * crate::work::ASSEMBLE_VERTEX)?;
    let lod0 = assemble(&built.iter().map(|b| (&b.shape, b)).collect::<Vec<_>>(), skin);
    lod0.validate().map_err(|er| e("parts", ErrorCode::InvalidMesh, er.to_string()))?;
    if lod0.triangle_count() == 0 {
        return Err(e("parts", ErrorCode::Geometry, "the model has no triangles (every part switched off?)"));
    }
    // Budget.
    let lookup = |n: &str| vars.get(n).copied();
    let eval_budget = |ex: &Option<Expr>, path: &str| -> Result<Option<u32>, RecipeError> {
        match ex {
            None => Ok(None),
            Some(x) => {
                let v = x.eval(&lookup);
                if !(v.is_finite() && v >= 0.0) {
                    return Err(e(path, ErrorCode::OutOfRange, "budget must be a non-negative number"));
                }
                Ok(Some(v as u32))
            }
        }
    };
    let max_t = eval_budget(&recipe.max_triangles, "budget.max_triangles")?;
    let min_t = eval_budget(&recipe.min_triangles, "budget.min_triangles")?;
    let n = lod0.triangle_count();
    if let Some(m) = max_t {
        if n > m as usize {
            return Err(e("budget.max_triangles", ErrorCode::OverBudget, format!("{n} triangles, budget {m}")).with_numbers(n as f64, m as f64));
        }
    }
    if let Some(m) = min_t {
        if n < m as usize {
            return Err(e("budget.min_triangles", ErrorCode::UnderBudget, format!("{n} triangles, minimum {m}")).with_numbers(n as f64, m as f64));
        }
    }
    // LODs.
    let bounds = lod0.bounds();
    let size = bounds.size();
    let extent = size[0].max(size[1]).max(size[2]);
    let mut lods = vec![lod0];
    let mut ratios = vec![1.0f32];
    let mut coarse_used = false;
    // Booleans, sculpts and generators rebuild cleanly at lower detail (no bevels, half the SDF
    // resolution, a coarser terrain grid), where clustering would fold their cuts and recesses:
    // their first LOD is the coarse rebuild when it saves enough.
    // The coarse rebuild, made at most once (procedural rebuilds aren't cheap).
    let mut coarse = CoarseOnce::default();
    let procedural = recipe.parts.iter().any(|p| match &p.source {
        Source::Generate(_) => true,
        Source::Shape(n) => matches!(n.op.as_str(), "union" | "difference" | "intersection" | "sdf" | "figure"),
        Source::Include { .. } => false,
    });
    for (i, &ratio) in recipe.lod_ratios.iter().enumerate() {
        let inputs: Vec<LodInput> = built.iter().map(|b| LodInput { shape: &b.shape, cell_scale: b.lod_detail }).collect();
        let prev = lods.last().map_or(0, |m| m.triangle_count());
        // An LOD that saves under 10% over the previous one is dropped: it would cost memory and
        // a switch for nothing (parts kept with lod_detail 0 can make a model irreducible).
        let saves = |m: &MeshData| m.triangle_count() > 0 && m.triangle_count() as f32 <= 0.9 * prev as f32;
        if i == 0 && procedural {
            if let Some(mesh) = coarse.get_or_init(recipe, params, seed, &built, skin, &lods[0], &tally)? {
                if saves(&mesh) {
                    coarse_used = true;
                    notes.push(format!("lods[{i}] ({ratio}): {COARSE_NOTE}, {} triangles (procedural parts)", mesh.triangle_count()));
                    lods.push(mesh);
                    ratios.push(ratio);
                    continue;
                }
            }
        }
        charge(&tally, &format!("lods[{i}]"), inputs.iter().map(|p| p.shape.triangle_count() as u64).sum::<u64>() * crate::work::SIMPLIFY_TRIANGLE)?;
        let simplified = match lod::simplify(&inputs, ratio, bounds.min, extent) {
            None => Err("no simplification keeps every part within tolerance"),
            Some((shapes, _cell)) => {
                let pairs: Vec<(&Shape, &BuiltPart)> = shapes.iter().zip(&built).collect();
                let mesh = assemble(&pairs, skin);
                mesh.validate().map_err(|er| e(&format!("lods[{i}]"), ErrorCode::InvalidMesh, er.to_string()))?;
                if saves(&mesh) { Ok(mesh) } else { Err("saves under 10% over the previous LOD") }
            }
        };
        let mesh = match simplified {
            Ok(mesh) => mesh,
            // Clustering can't reduce small bevelled parts (each part's cell is capped by its
            // size): try the recipe rebuilt coarse instead, once, as the far level.
            Err(why) => match if coarse_used { None } else { coarse.get_or_init(recipe, params, seed, &built, skin, &lods[0], &tally)? } {
                Some(mesh) if saves(&mesh) => {
                    coarse_used = true;
                    notes.push(format!("lods[{i}] ({ratio}): {COARSE_NOTE}, {} triangles (clustering: {why})", mesh.triangle_count()));
                    mesh
                }
                _ => {
                    notes.push(format!("lods[{i}] ({ratio}) skipped: {why}"));
                    continue;
                }
            },
        };
        lods.push(mesh);
        ratios.push(ratio);
    }
    let per_point = if recipe.collider == ColliderKind::ConvexHull { crate::work::HULL_POINT } else { crate::work::TIER1_POINT };
    charge(&tally, "collider", lods[0].positions.len() as u64 * per_point)?;
    let (collider, note) = collider::fit(recipe.collider, &lods[0].positions);
    // A cancel during the fit (it stops early) fails here.
    charge(&tally, "collider", 0)?;
    notes.extend(note);
    let mut materials = Vec::new();
    for p in &lods[0].parts {
        if !materials.iter().any(|(n, _): &(String, MaterialPreset)| n == &p.material) {
            // A textured part's key is `preset+texture`.
            let preset = p.material.split('+').next().unwrap_or(&p.material);
            materials.push((p.material.clone(), params.style.materials[preset].clone()));
        }
    }
    // The terrain renderer's layer textures bake with the parts' (keys `terrain+<graph>`).
    let terrain_keys: Vec<String> =
        tally.terrain.borrow().as_ref().and_then(|t| t.render.as_ref()).map(|r| r.layers.iter().filter_map(|l| l.texture.clone()).collect()).unwrap_or_default();
    let textures = procgen::bake_textures(recipe, params, &lods[0], &terrain_keys, tally.work.get())?;
    let mut h = StableHasher::new();
    for m in &lods {
        h.write_u64(m.content_hash());
    }
    hash_collider(&mut h, &collider);
    // Models without textures hash exactly as they did before textures existed.
    for (k, t) in &textures {
        h.write_str(k);
        h.write_u64(t.hash);
    }
    // Unrigged models hash exactly as they did before rigs existed.
    if let Some((r, _)) = &rig {
        h.write_u8(1);
        r.hash_into(&mut h);
    }
    // So do models without wind.
    let wind = recipe.wind.map(|w| Wind { amount: w.amount, height: w.height.unwrap_or_else(|| lods[0].bounds().max[1].clamp(0.03, 8.0)), painted: w.painted });
    if let Some(w) = &wind {
        h.write_u8(2);
        h.write_u32(w.amount.to_bits());
        h.write_u32(w.height.to_bits());
        h.write_u8(w.painted as u8);
    }
    let terrain = tally.terrain.borrow().clone();
    let terrain_transform = tally.terrain_transform.get();
    let stats = tally.stats.borrow().clone();
    let mut used: Vec<(String, f64)> = recipe.params.iter().map(|p| (p.name.clone(), vars[&p.name])).collect();
    used.sort_by(|a, b| a.0.cmp(&b.0));
    // What it is built of, against the finest level's whole surface.
    let surface: f64 = (0..lods[0].triangle_count())
        .map(|t| {
            let [a, b, c] = lods[0].triangle(t);
            f64::from(math3::length(math3::cross(math3::sub(b, a), math3::sub(c, a)))) * 0.5
        })
        .sum();
    let finished: Vec<&Shape> = built.iter().map(|b| &b.shape).collect();
    // (Round shapes first: `made_of` takes the record.)
    let (round, round_total, round_not_measured) = {
        let paths: Vec<String> = tally.made.borrow().iter().map(|r| r.path.clone()).collect();
        crate::facet::finish(&tally.rounds.borrow(), &paths, &finished)
    };
    let (made, made_rest_m2) = tally.made_of(&finished, surface as f32);
    Ok(Model {
        name: recipe.name.clone(),
        bounds: lods[0].bounds(),
        lods,
        lod_ratios: ratios,
        collider,
        materials,
        hash: h.finish(),
        params: used,
        seed,
        budget: (min_t, max_t),
        notes,
        rig: rig.map(|(r, _)| r),
        extras: ModelExtras { textures, terrain, terrain_transform, stats, round, round_total, round_not_measured, made, made_rest_m2 },
        wind,
    })
}

/// Which bones a built part's vertices bind to, and its blend band (metres).
struct Binding {
    bones: Vec<usize>,
    blend: f32,
    /// How thick each bone's flesh is (a figure's rig): empty binds by plain distance.
    radii: Vec<f32>,
}

/// Evaluates the rig's bones, fills default tails (a bone's only child's head, else a point) and
/// moves the rig with the recipe's rigid `finish` ops (move, rotate, scale, mirror without keep),
/// so pivots are written in the same space as the parts. Returns the rig and its default blend.
fn build_rig(recipe: &Recipe, def: &RigDef, ctx: &Ctx) -> Result<(Rig, f32), RecipeError> {
    let point = |v: &Val, path: &str| -> Result<V3, RecipeError> {
        let l = ctx.list(v, path)?;
        let p = [l[0], l[1], l[2]];
        if !p.iter().all(|x| fits(*x, MAX_EXTENT_M)) {
            return Err(e(path, ErrorCode::OutOfRange, format!("bone positions must be within ±{MAX_EXTENT_M} m")));
        }
        Ok(p)
    };
    let mut ends: Vec<(V3, Option<V3>)> = Vec::with_capacity(def.bones.len());
    // (A figure's bones have no places written in the recipe: they are the body's, below.)
    for b in if def.figure.is_none() { &def.bones[..] } else { &[] } {
        let mut head = point(&b.head, &format!("{}.head", b.path))?;
        let mut tail = b.tail.as_ref().map(|t| point(t, &format!("{}.tail", b.path))).transpose()?;
        if b.mirror_x {
            head[0] = -head[0];
            if let Some(t) = &mut tail {
                t[0] = -t[0];
            }
        }
        ends.push((head, tail));
    }
    let bones = match &def.figure {
        // A measured body's own skeleton, in the pose its surface is made in.
        Some(figure) => {
            let (body, shaping) = ctx.figure_body(figure)?;
            crate::figure::bones(&body, &shaping)
        }
        None => def
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let (head, tail) = ends[i];
                let tail = tail.unwrap_or_else(|| {
                    let mut kids = def.bones.iter().enumerate().filter(|(_, c)| c.parent == Some(i));
                    match (kids.next(), kids.next()) {
                        (Some((k, _)), None) => ends[k].0,
                        _ => head,
                    }
                });
                Bone { name: b.name.clone(), parent: b.parent, head, tail }
            })
            .collect(),
    };
    let mut rig = Rig { bones };
    for op in &recipe.finish {
        if let Some(t) = ctx.rig_affine(op)? {
            for b in &mut rig.bones {
                b.head = t.apply_point(b.head);
                b.tail = t.apply_point(b.tail);
            }
        }
    }
    rig.validate().map_err(|er| {
        let path = er.bone.map_or_else(|| "rig".to_string(), |b| def.bones[b].path.clone());
        e(&path, ErrorCode::InvalidRig, er.to_string())
    })?;
    let blend = match &def.blend {
        Some(x) => blend_value(ctx, x, "rig.blend")?,
        None => 0.0,
    };
    Ok((rig, blend))
}

fn blend_value(ctx: &Ctx, x: &Expr, path: &str) -> Result<f32, RecipeError> {
    let v = ctx.eval(x, path)?;
    if !(0.0..=10.0).contains(&v) {
        return Err(e(path, ErrorCode::OutOfRange, format!("blend {v} is outside 0..10 m")).with_numbers(v, 10.0));
    }
    Ok(v as f32)
}

/// Charges `units` of the build's work ([`crate::work`]) for a stage outside the procedural
/// tiers (tier-1 ops, assembling, LOD simplification, colliders), or fails with a Limit error
/// at `path`; a cancelled build fails here too.
fn charge(tally: &Tally, path: &str, units: u64) -> Result<(), RecipeError> {
    let used = tally.work.get().saturating_add(units);
    if used > procgen::MAX_BUILD_WORK {
        return Err(e(path, ErrorCode::Limit, format!("the model needs more than {} work units (about 1.5 s) here: fewer vertices, ops, octaves or LODs", procgen::MAX_BUILD_WORK)).with_numbers(used as f64, procgen::MAX_BUILD_WORK as f64));
    }
    if crate::work::cancelled() {
        return Err(e(path, ErrorCode::Limit, crate::work::CANCELLED));
    }
    tally.work.set(used);
    Ok(())
}

/// Running totals for one build, shared by every part and include, so limits hold across the
/// whole model and are checked *before* allocating.
#[derive(Default)]
struct Tally {
    /// Triangles of every piece built so far (the current one included once generated).
    triangles: std::cell::Cell<usize>,
    includes: std::cell::Cell<usize>,
    /// What booleans, sculpts and generators did (for the report), bounded.
    stats: std::cell::RefCell<Vec<(String, Json)>>,
    /// The first terrain built, and its transform in the model (`None`: reshaped).
    terrain: std::cell::RefCell<Option<std::sync::Arc<crate::terrain::Terrain>>>,
    terrain_transform: std::cell::Cell<Option<Affine>>,
    /// Boolean and texture work spent so far (all budgets are per build).
    work: std::cell::Cell<u64>,
    /// What the shape being built noted of how round it is, until it has a slot.
    round_notes: std::cell::RefCell<Vec<crate::facet::Note>>,
    /// The round shapes so far, each on its slot, with what has been done to them since
    /// ([`crate::facet`]), at most [`crate::facet::MAX_RECS`].
    rounds: std::cell::RefCell<Vec<crate::facet::Rec>>,
    /// What the fine level is built of so far ([`crate::Made`]): each shape the recipe names,
    /// in the order built (a path and what it is).
    made: std::cell::RefCell<Vec<made::Record>>,
    /// Their outlines, in slots: a plain shape has one, a sculpt one for each primitive that
    /// adds to it. Every triangle of the fine level knows its slot ([`Shape::owners`]), and
    /// what a slot weighs is summed from its triangles in the finished model.
    slots: std::cell::RefCell<Vec<made::Slot>>,
    /// The work spent asking a sculpt's triangles which primitive made them, against its own
    /// limit ([`crate::sdf::MAX_OWNER_BUILD_WORK`]): never against the build's.
    owner_work: std::cell::Cell<u64>,
    /// The outlines of the shape being built, until it is built and its surface known.
    making: std::cell::RefCell<Option<(&'static str, made::Views)>>,
    /// How deep the build is in what a boolean takes away: nothing there is recorded.
    cutting: std::cell::Cell<u32>,
}

impl Tally {
    /// The shape at `path` is built as `s`: what was noted of it is kept, and every triangle of
    /// `s` is its.
    fn made_now(&self, path: &str, s: &mut Shape) {
        let notes = self.round_notes.take();
        let views = self.making.take().map(|(what, views)| (what, crate::made::views(views))).filter(|(_, v)| !v.is_empty());
        if s.tris.is_empty() || (views.is_none() && notes.is_empty()) {
            return;
        }
        let mut records = self.made.borrow_mut();
        let mut slots = self.slots.borrow_mut();
        if records.len() < crate::made::MAX_KEPT && slots.len() < crate::made::MAX_SLOTS {
            // (A shape whose outlines are not kept, a cutter, still has a slot for its round
            // shapes: one that speaks for nothing in what the model is built of.)
            let what = views.as_ref().map_or_else(|| notes.first().map_or("", |n| n.what), |v| v.0);
            let speaks = views.is_some();
            slots.push(made::Slot { record: records.len() as u32, views: views.map(|v| v.1).unwrap_or_default(), speaks });
            records.push(made::Record { path: path.to_string(), what });
            let slot = slots.len() as u32 - 1;
            s.own(slot);
            drop((records, slots));
            for note in notes {
                let ring = note.ring.map(|sides| crate::facet::Ring::of(s, note.what, sides, note.radius));
                self.round_on(note, slot, ring);
            }
        }
    }

    /// A round shape's note, on its slot.
    fn round_on(&self, note: crate::facet::Note, slot: u32, ring: Option<crate::facet::Ring>) {
        if !(note.turn_deg.is_finite() && note.radius.is_finite() && note.turn_deg > 0.0 && note.radius > 0.0) {
            return;
        }
        let record = self.slots.borrow().get(slot as usize).map_or(0, |s| s.record);
        let mut recs = self.rounds.borrow_mut();
        if recs.len() < crate::facet::MAX_RECS {
            recs.push(crate::facet::Rec { note, record, slot, ring, maps: vec![crate::facet::IDENTITY], many: None, displaced: 0.0, unknown: None });
        }
    }

    /// An operation did `mv` to the triangles of `owners` (a shape's): the round shapes among
    /// them carry it. Nothing to do when nothing round was built.
    fn moved(&self, owners: &[u32], mv: &crate::facet::Moved) {
        let mut recs = self.rounds.borrow_mut();
        if recs.is_empty() || owners.is_empty() {
            return;
        }
        let here = crate::facet::slots_in(owners);
        for r in recs.iter_mut().filter(|r| here.binary_search(&r.slot).is_ok()) {
            r.moved(mv);
        }
    }

    /// The sculpt at `path` is built as `s`: `views` holds each of its primitives' outlines (in
    /// the tree's depth-first order; none for one that does not add to it), and `of` says which
    /// primitive made each triangle of `s` ([`crate::shape::NO_OWNER`]: none, or not asked).
    /// `rounds` holds each primitive's round note (the same order; none for one with nothing
    /// round), and `views` is empty for a cutter, which is not what the model is built of:
    /// a primitive that is round has a slot even where it adds nothing (what a difference
    /// takes away leaves walls), one that speaks for nothing.
    fn made_sculpt(&self, path: &str, what: &'static str, views: Vec<Option<made::Views>>, rounds: Vec<Option<crate::facet::Note>>, of: &[u32], s: &mut Shape) {
        // (A cutter has no outlines: it speaks for nothing in what the model is built of.)
        let speaks = !views.is_empty();
        let mut records = self.made.borrow_mut();
        let mut slots = self.slots.borrow_mut();
        if records.len() >= crate::made::MAX_KEPT || of.len() != s.tris.len() {
            return;
        }
        let record = records.len() as u32;
        let mut slot_of = vec![crate::shape::NO_OWNER; views.len().max(rounds.len())];
        let mut round_slots = Vec::new();
        for (i, slot) in slot_of.iter_mut().enumerate() {
            let v = if speaks { crate::made::views(views.get(i).cloned().flatten().unwrap_or_default()) } else { Vec::new() };
            let round = rounds.get(i).cloned().flatten();
            if (!v.is_empty() || round.is_some()) && slots.len() < crate::made::MAX_SLOTS {
                *slot = slots.len() as u32;
                slots.push(made::Slot { record, views: v.clone(), speaks: !v.is_empty() });
                if let Some(r) = round {
                    round_slots.push((r, *slot));
                }
            }
        }
        if slot_of.iter().all(|&x| x == crate::shape::NO_OWNER) {
            s.owners.clear();
            return;
        }
        records.push(made::Record { path: path.to_string(), what });
        s.owners = of.iter().map(|&p| slot_of.get(p as usize).copied().unwrap_or(crate::shape::NO_OWNER)).collect();
        drop((records, slots));
        for (note, slot) in round_slots {
            self.round_on(note, slot, None);
        }
    }

    /// Which primitive made each triangle of a sculpt's mesh `shape`, asked as a build asks
    /// it: every triangle up to [`crate::sdf::MAX_OWNER_ASKED`], every so many past it, within
    /// what is left of the build's own limit for it ([`crate::sdf::MAX_OWNER_BUILD_WORK`]),
    /// which is spent.
    pub(crate) fn ask_owners(&self, tree: &crate::sdf::SdfNode, seed: u64, shape: &Shape) -> crate::sdf::Asked {
        let left = crate::sdf::MAX_OWNER_BUILD_WORK.saturating_sub(self.owner_work.get());
        let a = crate::sdf::primitive_of_triangles(tree, seed, shape, crate::sdf::MAX_OWNER_ASKED, left);
        self.owner_work.set(self.owner_work.get().saturating_add(a.work));
        a
    }

    /// The records made from `from` on were made inside an include at `at` (`parts[1].include`)
    /// of `asset`: their paths are written as an error's there are (`parts[1].include(gem).
    /// parts[0].shape`).
    fn made_included(&self, from: usize, at: &str, asset: &str) {
        for m in self.made.borrow_mut().iter_mut().skip(from) {
            m.path = format!("{at}({asset}).{}", m.path);
        }
    }

    /// What the model is built of, measured on its finished parts (`shapes`) against its
    /// finest level's `surface` ([`crate::made::gather`]).
    fn made_of(&self, shapes: &[&Shape], surface: f32) -> (Vec<crate::Made>, f32) {
        crate::made::gather(&self.made.take(), &self.slots.take(), shapes, surface)
    }

    /// A piece of `old` triangles is about to become `new`: fail before allocating if that
    /// breaks the part or model limit, else record it.
    fn grow(&self, path: &str, old: usize, new: usize) -> Result<(), RecipeError> {
        if new > MAX_PART_TRIANGLES {
            return Err(e(path, ErrorCode::Limit, format!("this would make a part of {new} triangles; at most {MAX_PART_TRIANGLES}"))
                .with_numbers(new as f64, MAX_PART_TRIANGLES as f64));
        }
        let total = self.triangles.get() - old.min(self.triangles.get()) + new;
        if total > MAX_MODEL_TRIANGLES {
            return Err(e(path, ErrorCode::Limit, format!("this would make the model {total} triangles; at most {MAX_MODEL_TRIANGLES}"))
                .with_numbers(total as f64, MAX_MODEL_TRIANGLES as f64));
        }
        self.triangles.set(total);
        Ok(())
    }

    /// Replaces a prediction with the count actually built (never more than predicted).
    fn settle(&self, predicted: usize, actual: usize) {
        self.triangles.set(self.triangles.get() - predicted.min(self.triangles.get()) + actual);
    }

    fn include(&self, path: &str) -> Result<(), RecipeError> {
        let n = self.includes.get() + 1;
        if n > MAX_INCLUDES {
            return Err(e(path, ErrorCode::Limit, format!("more than {MAX_INCLUDES} include expansions in one build")).with_numbers(n as f64, MAX_INCLUDES as f64));
        }
        self.includes.set(n);
        Ok(())
    }
}

fn hash_collider(h: &mut StableHasher, c: &Collider) {
    let v3 = |h: &mut StableHasher, v: &V3| v.iter().for_each(|x| h.write_f32(*x));
    match c {
        Collider::None => h.write_u8(0),
        Collider::Aabb { min, max } => {
            h.write_u8(1);
            v3(h, min);
            v3(h, max);
        }
        Collider::Capsule { a, b, radius } => {
            h.write_u8(2);
            v3(h, a);
            v3(h, b);
            h.write_f32(*radius);
        }
        Collider::ConvexHull { points, indices } => {
            h.write_u8(3);
            points.iter().for_each(|p| v3(h, p));
            indices.iter().for_each(|i| h.write_u32(*i));
        }
    }
}

/// What a coarse far level is (model_build notes say when one was used).
pub const COARSE_NOTE: &str = "the recipe rebuilt coarse (no bevels, about half the segments and sides, no subdivision or displacement)";

/// The recipe rebuilt coarse ([`Ctx::coarse`]) as a far level, when it has the same parts as
/// `built` and keeps LOD 0's bounds and volume within the auto-LOD tolerances (the model's
/// bounds within [`lod::MODEL_BOUNDS_TOLERANCE`] of its size, a closed model's volume within
/// [`lod::VOLUME_TOLERANCE`]); `None` otherwise.
/// [`coarse_level`] computed on first use and kept.
#[derive(Default)]
struct CoarseOnce(Option<Option<MeshData>>);

impl CoarseOnce {
    #[allow(clippy::too_many_arguments)]
    fn get_or_init(&mut self, recipe: &Recipe, params: &BuildParams, seed: u64, built: &[BuiltPart], skin: Option<(&Rig, &[Binding])>, lod0: &MeshData, main: &Tally) -> Result<Option<MeshData>, RecipeError> {
        if self.0.is_none() {
            self.0 = Some(coarse_level(recipe, params, seed, built, skin, lod0, main)?);
        }
        Ok(self.0.as_ref().and_then(|m| m.clone()))
    }
}

/// `main`: the build's tally; the coarse rebuild continues its work budget and what it spends
/// is added back (its triangles are counted apart).
fn coarse_level(recipe: &Recipe, params: &BuildParams, seed: u64, built: &[BuiltPart], skin: Option<(&Rig, &[Binding])>, lod0: &MeshData, main: &Tally) -> Result<Option<MeshData>, RecipeError> {
    let mut stack = vec![recipe.name.clone()];
    let tally = Tally::default();
    tally.work.set(main.work.get());
    let rebuilt = build_parts(recipe, params.values, seed, params, &mut stack, &tally, true);
    main.work.set(tally.work.get());
    // A cancelled build stops here rather than going on without its coarse level.
    charge(main, "lods", 0)?;
    let Ok((coarse, _)) = rebuilt else {
        return Ok(None);
    };
    charge(main, "lods", coarse.iter().map(|c| c.shape.positions.len() as u64).sum::<u64>() * crate::work::ASSEMBLE_VERTEX)?;
    if coarse.len() != built.len() {
        return Ok(None);
    }
    let pairs: Vec<(&Shape, &BuiltPart)> = coarse.iter().map(|c| &c.shape).zip(built).collect();
    let mesh = assemble(&pairs, skin);
    if mesh.validate().is_err() {
        return Ok(None);
    }
    let (a, b) = (mesh.bounds(), lod0.bounds());
    let size = b.size();
    let tol = lod::MODEL_BOUNDS_TOLERANCE * size[0].max(size[1]).max(size[2]);
    if (0..3).any(|k| (a.min[k] - b.min[k]).abs() > tol || (a.max[k] - b.max[k]).abs() > tol) {
        return Ok(None);
    }
    let (v0, v) = (lod0.signed_volume(), mesh.signed_volume());
    if v0 > 1e-9 && !(v >= lod::VOLUME_TOLERANCE.0 * v0 && v <= lod::VOLUME_TOLERANCE.1 * v0) {
        return Ok(None);
    }
    Ok(Some(mesh))
}

/// One built part, before meshing.
#[derive(Clone, Debug)]
struct BuiltPart {
    shape: Shape,
    material: String,
    smooth_angle: f32,
    lod_detail: f32,
    /// Bone names from the (top-level) part that produced this piece.
    bones: Vec<String>,
    blend: Option<f32>,
    /// Box-projected UVs at this many metres per repeat (textured parts).
    uv_scale: Option<f32>,
}

/// Meshes every part and merges them by material. With a rig, each part's vertices are bound
/// to its bones (`skin.1[k]` belongs to `parts[k]`).
fn assemble(parts: &[(&Shape, &BuiltPart)], skin: Option<(&Rig, &[Binding])>) -> MeshData {
    let mut mesh = MeshData::default();
    for (k, (shape, info)) in parts.iter().enumerate() {
        let mut m = shape.finalize_with_uvs(info.smooth_angle, &info.material, info.uv_scale);
        if let Some((rig, binds)) = skin {
            let (j, w) = if binds[k].radii.is_empty() {
                rig.bind(&m.positions, &binds[k].bones, binds[k].blend)
            } else {
                rig.bind_fleshed(&m.positions, &binds[k].bones, binds[k].blend, &binds[k].radii)
            };
            m.joints = j;
            m.weights = w;
        }
        mesh.merge(&m);
    }
    mesh.group_by_material();
    mesh
}

fn stream_seed(seed: u64, stream: &str) -> u64 {
    let mut h = StableHasher::with_seed(seed);
    h.write_str(stream);
    h.finish()
}

/// Resolves parameters and lets, then builds every enabled part (with includes expanded) and
/// applies `finish`.
fn build_parts(
    recipe: &Recipe,
    values: &[(&str, f64)],
    seed: u64,
    params: &BuildParams,
    stack: &mut Vec<String>,
    tally: &Tally,
    coarse: bool,
) -> Result<(Vec<BuiltPart>, BTreeMap<String, f64>), RecipeError> {
    let vars = eval_vars(recipe, values)?;
    let ctx = Ctx { vars: &vars, style: params.style, seed, tally, coarse, paint: std::cell::Cell::new([1.0; 4]) };
    let smooth_of = |x: &Expr, path: &str| -> Result<f32, RecipeError> {
        let v = ctx.eval(x, path)?;
        if !(0.0..=180.0).contains(&v) {
            return Err(e(path, ErrorCode::OutOfRange, format!("smooth_angle {v} is outside 0..180 degrees")).with_numbers(v, 180.0));
        }
        Ok(v as f32)
    };
    let default_smooth = match &recipe.smooth_angle {
        Some(x) => smooth_of(x, "smooth_angle")?,
        None => 30.0,
    };
    let mut out: Vec<BuiltPart> = Vec::new();
    let mut terrains: Vec<(String, std::sync::Arc<crate::terrain::Terrain>, bool)> = Vec::new();
    let mut scattered: Vec<(String, Vec<crate::math3::V2>)> = Vec::new();
    // The model's terrain is the first one made, here or in an include; its transform gathers
    // the ops of every part and `finish` it passes through on the way out.
    let had_before = tally.terrain.borrow().is_some();
    for part in &recipe.parts {
        if let Some(w) = &part.when {
            if ctx.eval(w, &format!("{}.when", part.path))? == 0.0 {
                continue;
            }
        }
        let had = tally.terrain.borrow().is_some();
        let made_mark = tally.made.borrow().len();
        let pseed = stream_seed(seed, &part.name);
        let mut pieces: Vec<BuiltPart> = match &part.source {
            Source::Shape(node) => {
                let material = part.material.clone().unwrap_or_default();
                let col = match &part.color {
                    Some(c) => params.style.color(c).unwrap_or([1.0; 4]),
                    None => params.style.materials[&material].color,
                };
                ctx.paint.set(col);
                let mut shape = ctx.shape(node)?;
                // Booleans and sculpts were painted as they were built (their operands' and
                // nodes' own colours kept); everything else takes the part's colour.
                if !matches!(node.op.as_str(), "union" | "difference" | "intersection" | "sdf") {
                    shape.set_color(col);
                }
                let smooth = match &part.smooth_angle {
                    Some(x) => smooth_of(x, &format!("{}.smooth_angle", part.path))?,
                    None => default_smooth,
                };
                vec![BuiltPart { shape, material, smooth_angle: smooth, lod_detail: 1.0, bones: Vec::new(), blend: None, uv_scale: None }]
            }
            Source::Include { asset, params: inc_params } => {
                let path = format!("{}.include", part.path);
                if stack.len() > MAX_INCLUDE_DEPTH {
                    return Err(e(&path, ErrorCode::IncludeCycle, format!("includes nest deeper than {MAX_INCLUDE_DEPTH}: {}", stack.join(" > "))));
                }
                if stack.contains(asset) {
                    return Err(e(&path, ErrorCode::IncludeCycle, format!("include cycle: {} > {asset}", stack.join(" > "))));
                }
                tally.include(&path)?;
                let text = params
                    .library
                    .and_then(|l| l.get(asset))
                    .ok_or_else(|| e(&path, ErrorCode::UnknownAsset, format!("'{asset}' is not in the recipe library")))?;
                let child = Recipe::parse(text, params.style).map_err(|er| RecipeError { path: format!("{path}({asset}).{}", er.path), ..er })?;
                let mut vals: Vec<(String, f64)> = Vec::new();
                for (k, x) in inc_params {
                    vals.push((k.clone(), ctx.eval(x, &format!("{}.params.{k}", part.path))?));
                }
                let refs: Vec<(&str, f64)> = vals.iter().map(|(k, v)| (k.as_str(), *v)).collect();
                stack.push(asset.clone());
                let res = build_parts(&child, &refs, pseed, params, stack, tally, coarse);
                stack.pop();
                let built = res.map_err(|er| RecipeError { path: format!("{path}({asset}).{}", er.path), ..er })?.0;
                tally.made_included(made_mark, &path, asset);
                built
            }
            Source::Generate(node) => {
                let smooth = match &part.smooth_angle {
                    Some(x) => smooth_of(x, &format!("{}.smooth_angle", part.path))?,
                    None => default_smooth,
                };
                let mut gen = procgen::Gen {
                    ctx: &ctx,
                    part,
                    params,
                    stack: &mut *stack,
                    tally,
                    coarse,
                    seed: pseed,
                    smooth,
                    terrains: &mut terrains,
                    scattered: &mut scattered,
                    recipe_name: &recipe.name,
                    textures: &recipe.textures,
                };
                gen.run(node)?
            }
        };
        // Textures: the part's material key names its texture set; UVs from its scale.
        if let Some(t) = &part.texture {
            let scale = match &part.uv_scale {
                Some(x) => {
                    let v = ctx.eval(x, &format!("{}.uv_scale", part.path))?;
                    if !(0.01..=1000.0).contains(&v) {
                        return Err(e(&format!("{}.uv_scale", part.path), ErrorCode::OutOfRange, format!("uv_scale {v} is outside 0.01..1000 m per repeat")).with_numbers(v, 1000.0));
                    }
                    v as f32
                }
                None => 1.0,
            };
            // The key names the graph wherever the part ends up (an including recipe has its own
            // texture names): a library graph by its file, an inline one by its recipe.
            let key = match recipe.textures.iter().find(|(n, _)| n == t).map(|(_, s)| s) {
                Some(TexSource::Library(lib)) => format!("lib:{lib}"),
                _ => format!("{}/{t}", recipe.name),
            };
            // The texture dresses the part's own material (a building without one: its walls);
            // a generator's other roles (a building's door, glass) keep theirs.
            let dressed = part.material.clone().or_else(|| pieces.first().map(|p| p.material.clone()));
            for p in pieces.iter_mut().filter(|p| dressed.as_ref() == Some(&p.material)) {
                p.material = format!("{}+{key}", p.material);
                p.uv_scale = Some(scale);
            }
        }
        if let Some(d) = &part.lod_detail {
            let v = ctx.eval(d, &format!("{}.lod_detail", part.path))? as f32;
            if !(0.0..=4.0).contains(&v) {
                return Err(e(&format!("{}.lod_detail", part.path), ErrorCode::OutOfRange, "lod_detail must be 0..4 (0 = never simplify)"));
            }
            pieces.iter_mut().for_each(|p| p.lod_detail *= v);
        }
        // Bones name this recipe's rig: an include's own bindings are replaced by this part's.
        let blend = part.blend.as_ref().map(|x| blend_value(&ctx, x, &format!("{}.blend", part.path))).transpose()?;
        for p in &mut pieces {
            p.bones = part.bones.clone();
            p.blend = blend;
        }
        // (Each triangle keeps what it was made as through the part's operations: what each
        // shape weighs is measured in the finished model.)
        for piece in &mut pieces {
            for (i, op) in part.ops.iter().enumerate() {
                ctx.apply(&mut piece.shape, op, stream_seed(pseed, &format!("op{i}")))?;
            }
        }
        if !had {
            match &part.source {
                // This part's own terrain, as generated.
                Source::Generate(node) if node.op == "terrain" => {
                    if let Some((_, t, _)) = terrains.last() {
                        tally.terrain.replace(Some(std::sync::Arc::clone(t)));
                        tally.terrain_transform.set(Some(Affine::IDENTITY));
                    }
                }
                // An include's terrain, with the transform its own parts and finish gave it.
                Source::Include { .. } => {}
                // A scatter's included items are copies, not the ground.
                _ => {
                    tally.terrain.replace(None);
                    tally.terrain_transform.set(None);
                }
            }
            if tally.terrain.borrow().is_some() {
                tally.terrain_transform.set(ctx.terrain_transform(&part.ops, tally.terrain_transform.get()));
            }
        }
        out.extend(pieces);
    }
    for (i, op) in recipe.finish.iter().enumerate() {
        for (k, piece) in out.iter_mut().enumerate() {
            ctx.apply(&mut piece.shape, op, stream_seed(seed, &format!("finish{i}/{k}")))?;
        }
    }
    if !had_before && tally.terrain.borrow().is_some() {
        tally.terrain_transform.set(ctx.terrain_transform(&recipe.finish, tally.terrain_transform.get()));
    }
    Ok((out, vars))
}

struct Ctx<'a> {
    vars: &'a BTreeMap<String, f64>,
    style: &'a Style,
    seed: u64,
    tally: &'a Tally,
    /// Building the coarse far level ([`COARSE_NOTE`]): no bevels, half the segments and sides,
    /// no subdivision or displacement. Values are still checked as given.
    coarse: bool,
    /// The colour a part's boolean operands and uncoloured SDF nodes start in (the part's
    /// colour), so colours they are given themselves survive.
    paint: std::cell::Cell<Rgba>,
}

fn axis_index(s: &str) -> usize {
    match s {
        "x" => 0,
        "y" => 1,
        _ => 2,
    }
}

impl Ctx<'_> {
    fn eval(&self, x: &Expr, path: &str) -> Result<f64, RecipeError> {
        let v = x.eval(&|n| self.vars.get(n).copied());
        if !v.is_finite() {
            return Err(e(path, ErrorCode::OutOfRange, "evaluated to NaN or infinity"));
        }
        Ok(v)
    }

    fn val_num(&self, v: &Val, path: &str) -> Result<f64, RecipeError> {
        match v {
            Val::Num(x) => self.eval(x, path),
            _ => Err(type_err(path, "a number")),
        }
    }

    fn num(&self, n: &Node, name: &str, default: f64) -> Result<f64, RecipeError> {
        match n.get(name) {
            Some(v) => self.val_num(v, &n.at(name)),
            None => Ok(default),
        }
    }

    /// A number in `lo..=hi`. Bevels are 0 in a coarse build.
    fn ranged(&self, n: &Node, name: &str, default: f64, lo: f64, hi: f64) -> Result<f32, RecipeError> {
        let v = self.num(n, name, default)?;
        if v < lo || v > hi {
            return Err(e(&n.at(name), ErrorCode::OutOfRange, format!("\"{name}\" is {v}, outside {lo}..{hi}")).with_numbers(v, if v > hi { hi } else { lo }));
        }
        if self.coarse && matches!(name, "bevel" | "corner_bevel") && lo <= 0.0 {
            return Ok(0.0);
        }
        Ok(v as f32)
    }

    /// The shape being built is a `what` with these outlines along its own axes and their
    /// weights ([`crate::Made`]): kept once it is built, not for what a boolean takes away.
    /// (Not for the coarse far level, whose triangles then carry nothing.)
    fn made(&self, what: &'static str, views: Vec<(Vec<V2>, f32)>) {
        if self.tally.cutting.get() > 0 || self.coarse {
            return;
        }
        self.tally.making.replace(Some((what, views)));
    }

    /// The shape being built is one the recipe names round, built with `turn_deg` between its
    /// faces at `radius_m` by its number `count` (of `value`); `ring`: round its own Y axis
    /// (not for the coarse far level, which is coarse on purpose). Kept with the shape's slot
    /// once it is built, and measured in the finished model ([`crate::facet`]).
    fn round(&self, what: &'static str, count: &'static str, value: u32, turn_deg: f32, radius_m: f32, ring: bool) {
        if !self.coarse {
            self.tally.round_notes.borrow_mut().push(crate::facet::Note { what, count, count_value: value, turn_deg: f64::from(turn_deg), radius: f64::from(radius_m), ring: ring.then_some(value), primitive: None });
        }
    }

    /// An integer in `lo..=hi`. A coarse build halves segment and side counts (rounding up,
    /// never below `lo`), resamples nothing and subdivides nothing.
    fn int(&self, n: &Node, name: &str, default: i64, lo: i64, hi: i64) -> Result<u32, RecipeError> {
        let v = self.num(n, name, default as f64)?;
        let r = dmath::round_f64(v);
        if r < lo as f64 || r > hi as f64 {
            return Err(e(&n.at(name), ErrorCode::OutOfRange, format!("\"{name}\" is {v}, outside {lo}..{hi}")).with_numbers(v, if r > hi as f64 { hi as f64 } else { lo as f64 }));
        }
        let r = r as u32;
        if self.coarse {
            match (name, n.op.as_str()) {
                // Half, rounded up to even (arches and round sections keep their extreme points).
                ("segments" | "sides" | "rings", _) => return Ok((r.div_ceil(2).div_ceil(2) * 2).min(r).max(lo.max(0) as u32)),
                ("resample", _) | ("levels", "subdivide") => return Ok(lo.max(0) as u32),
                _ => {}
            }
        }
        Ok(r)
    }

    fn list(&self, v: &Val, path: &str) -> Result<Vec<f32>, RecipeError> {
        match v {
            Val::List(items) => items.iter().enumerate().map(|(i, x)| self.val_num(x, &format!("{path}[{i}]")).map(|f| f as f32)).collect(),
            _ => Err(type_err(path, "a list")),
        }
    }

    fn vec3(&self, n: &Node, name: &str, default: V3) -> Result<V3, RecipeError> {
        match n.get(name) {
            Some(v) => {
                let l = self.list(v, &n.at(name))?;
                Ok([l[0], l[1], l[2]])
            }
            None => Ok(default),
        }
    }

    fn vec2(&self, n: &Node, name: &str, default: V2) -> Result<V2, RecipeError> {
        match n.get(name) {
            Some(v) => {
                let l = self.list(v, &n.at(name))?;
                Ok([l[0], l[1]])
            }
            None => Ok(default),
        }
    }

    /// A list of points, each `widths.0..=widths.1` numbers, at most `max_rows` of them, every
    /// value finite and within ±[`MAX_EXTENT_M`]. Counted before evaluating anything.
    fn rows(&self, v: &Val, path: &str, widths: (usize, usize), max_rows: usize) -> Result<Vec<Vec<f32>>, RecipeError> {
        let Val::List(rows) = v else { return Err(type_err(path, "a list of points")) };
        if rows.len() > max_rows {
            return Err(e(path, ErrorCode::Limit, format!("{} points; at most {max_rows}", rows.len())).with_numbers(rows.len() as f64, max_rows as f64));
        }
        let mut out = Vec::with_capacity(rows.len());
        for (i, r) in rows.iter().enumerate() {
            let p = format!("{path}[{i}]");
            let row = self.list(r, &p)?;
            if row.len() < widths.0 || row.len() > widths.1 {
                let want = if widths.0 == widths.1 { format!("{}", widths.0) } else { format!("{} to {}", widths.0, widths.1) };
                return Err(type_err(&p, &format!("a point of {want} numbers (got {})", row.len())));
            }
            if let Some(bad) = row.iter().find(|x| !fits(**x, MAX_EXTENT_M)) {
                return Err(e(&p, ErrorCode::OutOfRange, format!("{bad} is outside ±{MAX_EXTENT_M} m")).with_numbers(*bad as f64, MAX_EXTENT_M as f64));
            }
            out.push(row);
        }
        Ok(out)
    }

    fn bool(&self, n: &Node, name: &str, default: bool) -> bool {
        match n.get(name) {
            Some(Val::Bool(b)) => *b,
            _ => default,
        }
    }

    fn str<'n>(&self, n: &'n Node, name: &str, default: &'n str) -> &'n str {
        match n.get(name) {
            Some(Val::Str(s)) => s,
            _ => default,
        }
    }

    fn color(&self, n: &Node, name: &str) -> Rgba {
        self.style.color(self.str(n, name, "#ffffff")).unwrap_or([1.0; 4])
    }

    fn positive(&self, n: &Node, name: &str, default: f64) -> Result<f32, RecipeError> {
        self.ranged(n, name, default, 1e-4, 1e4)
    }

    /// An outline as `[x, z]` points (or `[a, b]` for sweep profiles). The point count is
    /// checked before every step that multiplies it (resample, bevel) and against `later`, the
    /// factor a caller's own corner bevel will apply, so nothing large is generated or tested.
    fn outline(&self, v: &Val, path: &str, max_points: usize, later: usize) -> Result<Vec<V2>, RecipeError> {
        let too_many = |n: usize| {
            e(path, ErrorCode::Limit, format!("the outline would have {n} points; at most {max_points}")).with_numbers(n as f64, max_points as f64)
        };
        let pts = match v {
            Val::List(_) => {
                let rows = self.rows(v, path, (2, 2), max_points)?;
                rows.iter().map(|r| [r[0], r[1]]).collect::<Vec<V2>>()
            }
            Val::Obj(n) => {
                let rot = self.ranged(n, "rotate", 0.0, -3600.0, 3600.0)? as f64;
                let size2 = |name: &str| -> Result<V2, RecipeError> {
                    let s = self.vec2(n, name, [1.0, 1.0])?;
                    if !(s[0] > 0.0 && s[1] > 0.0 && s[0] <= MAX_EXTENT_M && s[1] <= MAX_EXTENT_M) {
                        return Err(e(&n.at(name), ErrorCode::OutOfRange, format!("sizes must be in (0, {MAX_EXTENT_M}] m")));
                    }
                    Ok(s)
                };
                // A field that would do nothing is refused, never ignored: `roundness` is a
                // rectangle's, and `segments` an arch's or a rounded rectangle's.
                let round = self.ranged(n, "roundness", 0.0, 0.0, 1.0)?;
                // (`segments` goes with `roundness` whatever its value, 0 too: a roundness swept
                // down to nothing must not turn into an error at its last step.)
                let rounded = n.get("rect").is_some() && n.get("roundness").is_some();
                if n.get("roundness").is_some() && n.get("rect").is_none() {
                    return Err(e(&n.at("roundness"), ErrorCode::Geometry, "\"roundness\" rounds a \"rect\" outline: this one has none"));
                }
                if n.get("segments").is_some() && n.get("arch").is_none() && !rounded {
                    return Err(e(&n.at("segments"), ErrorCode::Geometry, "\"segments\" is an \"arch\" outline's, or a \"rect\" outline's with \"roundness\": it would do nothing here"));
                }
                let mut p: Vec<V2> = if n.get("form").is_some() {
                    let bevel = if self.ranged(n, "bevel", 0.0, 0.0, 100.0)? > 0.0 { 2 } else { 1 };
                    let rs = self.int(n, "resample", 1, 1, 64)? as usize;
                    self.form_outline(n, path, max_points, bevel * rs * later)?
                } else if let Some(pv) = n.get("points") {
                    self.rows(pv, &n.at("points"), (2, 2), max_points)?.iter().map(|r| [r[0], r[1]]).collect()
                } else if n.get("rect").is_some() {
                    let s = size2("rect")?;
                    // Roundness as one number: 0 the rectangle, 1 the ellipse as wide and as
                    // deep, and between them a superellipse (flat sides, corners that tighten
                    // as the number falls), in `segments` points.
                    if round > 0.0 {
                        let points = self.int(n, "segments", 48, 8, 256)? as usize;
                        let power = 2.0 / round.max(0.05);
                        (0..points)
                            .map(|i| {
                                let (sin, cos) = dmath::sin_cos_deg(360.0 * i as f64 / points as f64);
                                let side = |c: f64, half: f32| (half as f64 * c.signum() * dmath::pow(c.abs() as f32, 2.0 / power) as f64) as f32;
                                [side(cos, 0.5 * s[0]), side(sin, 0.5 * s[1])]
                            })
                            .collect()
                    } else {
                        poly::rect(s[0], s[1])
                    }
                } else if n.get("sides").is_some() {
                    let sides = self.int(n, "sides", 6, 3, 256)?;
                    poly::regular(sides, self.positive(n, "radius", 1.0)?, 90.0 + rot)
                } else if n.get("arch").is_some() {
                    let s = size2("arch")?;
                    poly::arch(s[0], s[1], self.int(n, "segments", 6, 2, 64)?)
                } else {
                    let pts = self.int(n, "star", 5, 3, 64)?;
                    let outer = self.positive(n, "radius", 1.0)?;
                    let inner = self.positive(n, "inner", 0.5 * outer as f64)?;
                    poly::star(pts, outer, inner, 90.0 + rot)
                };
                if n.get("rotate").is_some() && (n.get("points").is_some() || n.get("rect").is_some() || n.get("arch").is_some() || n.get("form").is_some()) {
                    let (s, c) = dmath::sin_cos_deg(rot);
                    let (s, c) = (s as f32, c as f32);
                    p = p.iter().map(|q| [q[0] * c - q[1] * s, q[0] * s + q[1] * c]).collect();
                }
                let sc = self.vec2(n, "scale", [1.0, 1.0])?;
                let off = self.vec2(n, "offset", [0.0, 0.0])?;
                if sc.iter().chain(off.iter()).any(|v| !fits(*v, MAX_EXTENT_M)) {
                    return Err(e(path, ErrorCode::OutOfRange, format!("scale and offset must be within ±{MAX_EXTENT_M}")));
                }
                p = p.iter().map(|q| [q[0] * sc[0] + off[0], q[1] * sc[1] + off[1]]).collect();
                let bevel = self.ranged(n, "bevel", 0.0, 0.0, 100.0)?;
                let rs = self.int(n, "resample", 1, 1, 64)? as usize;
                let final_count = p.len() * if bevel > 0.0 { 2 } else { 1 } * rs * later;
                if final_count > max_points {
                    return Err(too_many(final_count));
                }
                if bevel > 0.0 {
                    p = poly::bevel_corners(&poly::canonical(&p), bevel);
                }
                p = poly::resample(&p, rs as u32);
                let j = self.ranged(n, "jitter", 0.0, 0.0, 100.0)?;
                if j > 0.0 {
                    p = poly::jitter(&p, j, stream_seed(self.seed, path));
                }
                p
            }
            _ => return Err(type_err(path, "an outline")),
        };
        if pts.len() * later > max_points {
            return Err(too_many(pts.len() * later));
        }
        if pts.iter().any(|q| !(q[0].abs() <= MAX_EXTENT_M && q[1].abs() <= MAX_EXTENT_M)) {
            return Err(e(path, ErrorCode::OutOfRange, format!("outline points must be within ±{MAX_EXTENT_M} m")));
        }
        let c = poly::canonical(&pts);
        if c.len() < 3 || poly::signed_area(&c).abs() < 1e-8 {
            return Err(e(path, ErrorCode::Geometry, "an outline needs at least 3 points enclosing an area"));
        }
        if !poly::is_simple(&c) {
            return Err(e(path, ErrorCode::Geometry, "the outline crosses or touches itself"));
        }
        Ok(pts)
    }

    /// Builds a shape: its triangle count is predicted (an upper bound) and checked against the
    /// part and model limits before any geometry is generated.
    fn shape(&self, n: &Node) -> Result<Shape, RecipeError> {
        let geom = |r: Result<Shape, crate::shape::GeomError>| r.map_err(|g| e(&n.path, ErrorCode::Geometry, g.0));
        let (predicted, build): (usize, Box<dyn FnOnce() -> Result<Shape, RecipeError> + '_>) = match n.op.as_str() {
            "box" => {
                let size = self.vec3(n, "size", [1.0; 3])?;
                if size.iter().any(|v| !(*v > 0.0 && *v <= MAX_EXTENT_M)) {
                    return Err(e(&n.at("size"), ErrorCode::OutOfRange, format!("box sizes must be in (0, {MAX_EXTENT_M}] m")));
                }
                let bevel = self.ranged(n, "bevel", 0.0, 0.0, 100.0)?;
                self.made("box", vec![(made::rect(size[0], size[1], bevel), 1.0), (made::rect(size[0], size[2], bevel), 1.0), (made::rect(size[2], size[1], bevel), 1.0)]);
                (60, Box::new(move || geom(Shape::cuboid(size, bevel))))
            }
            "cylinder" | "cone" => {
                let radius = self.positive(n, "radius", 1.0)?;
                let top = if n.op == "cone" { 0.0 } else { self.ranged(n, "radius_top", radius as f64, 0.0, 1e4)? };
                let height = self.positive(n, "height", 1.0)?;
                let sides = self.int(n, "sides", 12, 3, 256)?;
                self.round(if n.op == "cone" { "cone" } else { "cylinder" }, "sides", sides, 360.0 / sides as f32, radius.max(top), true);
                // (Its own section, and what it is from the side, which two of its three views are.)
                self.made(if n.op == "cone" { "cone" } else { "cylinder" }, vec![(made::ngon(sides, radius.max(top)), 1.0), (made::trapezoid(radius, top, height), 2.0)]);
                let bevel = if n.op == "cone" { 0.0 } else { self.ranged(n, "bevel", 0.0, 0.0, 100.0)? };
                (8 * sides as usize, Box::new(move || geom(Shape::cylinder(radius, top, height, sides, bevel))))
            }
            "sphere" => {
                let (r, sub) = (self.positive(n, "radius", 1.0)?, self.int(n, "subdivisions", 1, 0, 5)?);
                // (A twenty-sided ball's faces turn 41.8 degrees from one to the next, and each
                // subdivision halves that.)
                self.round("sphere", "subdivisions", sub, 41.81 / (1u32 << sub) as f32, r, false);
                self.made("sphere", vec![(made::circle(r), 1.0)]);
                (20 * 4usize.pow(sub), Box::new(move || Ok(Shape::sphere(r, sub))))
            }
            "torus" => {
                let radius = self.positive(n, "radius", 1.0)?;
                let tube = self.positive(n, "tube", 0.1)?;
                if tube >= radius {
                    return Err(e(&n.at("tube"), ErrorCode::OutOfRange, "tube must be smaller than radius").with_numbers(tube as f64, radius as f64));
                }
                let (segs, sides) = (self.int(n, "segments", 24, 3, 512)?, self.int(n, "sides", 8, 3, 64)?);
                // (Round the ring, and round the tube: two rounds, each with its own radius.)
                self.round("torus", "segments", segs, 360.0 / segs as f32, radius + tube, true);
                self.round("torus", "sides", sides, 360.0 / sides as f32, tube, false);
                self.made("torus", vec![(made::circle(radius + tube), 1.0), (made::stadium(2.0 * (radius + tube), tube), 2.0)]);
                (2 * segs as usize * sides as usize, Box::new(move || Ok(Shape::torus(radius, tube, segs, sides))))
            }
            "capsule" => {
                let (r, h) = (self.positive(n, "radius", 0.5)?, self.positive(n, "height", 2.0)?);
                let (segs, rings) = (self.int(n, "segments", 12, 3, 256)?, self.int(n, "rings", 3, 1, 32)?);
                // (Round its axis, and over each end: a quarter turn in `rings` bands.)
                self.round("capsule", "segments", segs, 360.0 / segs as f32, r, true);
                self.round("capsule", "rings", rings, 90.0 / rings as f32, r, false);
                self.made("capsule", vec![(made::circle(r), 1.0), (made::stadium(h.max(2.0 * r), r), 2.0)]);
                (2 * segs as usize * (2 * rings as usize + 1), Box::new(move || Ok(Shape::capsule(r, h, segs, rings))))
            }
            "plane" => {
                let size = self.vec2(n, "size", [1.0, 1.0])?;
                if size.iter().any(|v| !(*v > 0.0 && *v <= MAX_EXTENT_M)) {
                    return Err(e(&n.at("size"), ErrorCode::OutOfRange, format!("plane sizes must be in (0, {MAX_EXTENT_M}] m")));
                }
                let d = self.vec2(n, "divisions", [1.0, 1.0])?;
                if d.iter().any(|v| !(*v >= 1.0 && *v <= 256.0)) {
                    return Err(e(&n.at("divisions"), ErrorCode::OutOfRange, "divisions must be 1..256"));
                }
                let div = [d[0] as u32, d[1] as u32];
                self.made("plane", vec![(made::rect(size[0], size[1], 0.0), 1.0)]);
                (2 * div[0] as usize * div[1] as usize, Box::new(move || Ok(Shape::plane(size, div))))
            }
            "extrude" => {
                let corner = self.ranged(n, "corner_bevel", 0.0, 0.0, 100.0)?;
                let outline = self.outline(n.get("outline").unwrap(), &n.at("outline"), MAX_OUTLINE_POINTS, if corner > 0.0 { 2 } else { 1 })?;
                let cap = match n.get("cap_inset") {
                    Some(_) => {
                        let c = self.vec2(n, "cap_inset", [0.0, 0.0])?;
                        if !(c[0] > 0.0 && c[0] <= 100.0 && c[1].abs() <= 100.0) {
                            return Err(e(&n.at("cap_inset"), ErrorCode::OutOfRange, "cap_inset needs an inset in (0, 100] and a depth within ±100"));
                        }
                        Some((c[0], c[1]))
                    }
                    None => None,
                };
                let height = self.positive(n, "height", 1.0)?;
                let taper = self.ranged(n, "taper", 1.0, 0.0, 100.0)?;
                let bevel = self.ranged(n, "bevel", 0.0, 0.0, 100.0)?;
                let segments = self.int(n, "segments", 1, 1, 256)?;
                let m = outline.len() * if corner > 0.0 { 2 } else { 1 };
                // (Its outline, and from either side the box it is raised to, narrowing as it tapers.)
                let half = made::half_extents(&outline);
                self.made("extrude", vec![(outline.clone(), 1.0), (made::trapezoid(half[0], half[0] * taper, height), 1.0), (made::trapezoid(half[1], half[1] * taper, height), 1.0)]);
                let rings = segments as usize + 1 + if bevel > 0.0 { 2 } else { 0 } + if cap.is_some() { 2 } else { 0 };
                (2 * m * rings, Box::new(move || geom(Shape::extrude(&outline, height, taper, bevel, corner, segments, cap))))
            }
            "loft" => {
                let corner = self.ranged(n, "corner_bevel", 0.0, 0.0, 100.0)?;
                let outline = self.outline(n.get("outline").unwrap(), &n.at("outline"), MAX_OUTLINE_POINTS, if corner > 0.0 { 2 } else { 1 })?;
                let rows = self.rows(n.get("stations").unwrap(), &n.at("stations"), (2, 4), MAX_ROWS)?;
                if rows.len() < 2 {
                    return Err(e(&n.at("stations"), ErrorCode::Geometry, "a loft needs at least 2 stations"));
                }
                if rows.iter().any(|r| r.len() == 3) {
                    return Err(type_err(&n.at("stations"), "stations of [scale, y] or [scale, y, dx, dz]"));
                }
                let mut st = Vec::with_capacity(rows.len());
                for (i, r) in rows.iter().enumerate() {
                    if !(r[0] >= 0.0 && r[0] <= 100.0) {
                        return Err(e(&format!("{}[{i}][0]", n.at("stations")), ErrorCode::OutOfRange, "station scale must be 0..100"));
                    }
                    st.push([r[0], r[1], r.get(2).copied().unwrap_or(0.0), r.get(3).copied().unwrap_or(0.0)]);
                }
                let m = outline.len() * if corner > 0.0 { 2 } else { 1 };
                // (Its outline, and from either side its stations: each as wide as the outline
                // scaled by it, and where its shift puts it, so that a loft that leans leans.)
                let half = made::half_extents(&outline);
                let mid = outline.iter().fold([f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY], |m, p| [m[0].min(p[0]), m[1].max(p[0]), m[2].min(p[1]), m[3].max(p[1])]);
                let mid = [0.5 * (mid[0] + mid[1]), 0.5 * (mid[2] + mid[3])];
                let along_x: Vec<[f32; 3]> = st.iter().map(|r| [r[0] * half[0], r[1], r[0] * mid[0] + r[2]]).collect();
                let along_z: Vec<[f32; 3]> = st.iter().map(|r| [r[0] * half[1], r[1], r[0] * mid[1] + r[3]]).collect();
                self.made("loft", vec![(outline.clone(), 1.0), (made::leaning(&along_x), 1.0), (made::leaning(&along_z), 1.0)]);
                (2 * m * (st.len() + 1), Box::new(move || geom(Shape::loft(&outline, &st, corner))))
            }
            "lathe" => {
                let rows = self.rows(n.get("profile").unwrap(), &n.at("profile"), (2, 2), MAX_ROWS)?;
                if rows.len() < 2 {
                    return Err(e(&n.at("profile"), ErrorCode::Geometry, "a lathe profile needs at least 2 points"));
                }
                if let Some(i) = rows.iter().position(|r| r[0] < 0.0) {
                    return Err(e(&format!("{}[{i}][0]", n.at("profile")), ErrorCode::OutOfRange, "lathe radius must be ≥ 0"));
                }
                let prof: Vec<V2> = rows.iter().map(|r| [r[0], r[1]]).collect();
                let segs = self.int(n, "segments", 16, 3, 256)?;
                // (Round its axis, at its widest; its profile's own corners are its maker's.)
                self.round("lathe", "segments", segs, 360.0 / segs as f32, prof.iter().map(|p| p[0]).fold(0.0f32, f32::max), true);
                self.made("lathe", vec![(made::ngon(segs, prof.iter().map(|p| p[0]).fold(0.0f32, f32::max)), 1.0), (made::mirrored(&prof), 2.0)]);
                let caps = self.bool(n, "caps", true);
                (2 * segs as usize * (prof.len() + 1), Box::new(move || Ok(Shape::lathe(&prof, segs, caps))))
            }
            "sweep" => {
                let profile = self.outline(n.get("profile").unwrap(), &n.at("profile"), MAX_PROFILE_POINTS, 1)?;
                let rows = self.rows(n.get("path").unwrap(), &n.at("path"), (3, 3), MAX_ROWS)?;
                let closed = self.bool(n, "closed", false);
                if rows.len() < if closed { 3 } else { 2 } {
                    return Err(e(&n.at("path"), ErrorCode::Geometry, "a sweep path needs at least 2 points (3 if closed)"));
                }
                let path: Vec<V3> = rows.iter().map(|r| [r[0], r[1], r[2]]).collect();
                if path.windows(2).any(|w| math3::length(math3::sub(w[1], w[0])) < 1e-6) {
                    return Err(e(&n.at("path"), ErrorCode::Geometry, "consecutive path points must differ"));
                }
                let scales = match n.get("scales") {
                    Some(v) => self.list(v, &n.at("scales"))?,
                    None => vec![1.0, 1.0],
                };
                if scales.iter().any(|s| !(*s >= 0.0 && *s <= 100.0)) {
                    return Err(e(&n.at("scales"), ErrorCode::OutOfRange, "scales must be 0..100"));
                }
                if !(scales.len() == 2 || scales.len() == path.len()) {
                    return Err(e(&n.at("scales"), ErrorCode::WrongType, format!("scales needs 2 values (start, end) or one per path point ({})", path.len())));
                }
                let smooth = self.int(n, "smooth", 1, 1, 16)? as usize;
                let rings = if closed { path.len() * smooth } else { (path.len() - 1) * smooth + 1 };
                if rings > MAX_SWEEP_RINGS {
                    return Err(e(&n.at("smooth"), ErrorCode::Limit, format!("the sweep would have {rings} rings; at most {MAX_SWEEP_RINGS}"))
                        .with_numbers(rings as f64, MAX_SWEEP_RINGS as f64));
                }
                let up = match n.get("up") {
                    Some(_) => Some(self.vec3(n, "up", [0.0, 1.0, 0.0])?),
                    None => None,
                };
                let twist = self.ranged(n, "twist", 0.0, -36_000.0, 36_000.0)?;
                let caps = self.bool(n, "caps", true);
                let m = profile.len();
                // (Its profile, and from the side its path laid out straight, as wide along it
                // as the profile is scaled: a horn that narrows to nothing comes to a point.)
                if !closed {
                    let half = made::half_extents(&profile);
                    let wide = half[0].max(half[1]);
                    let mut along = 0.0f32;
                    let mut side: Vec<V2> = Vec::new();
                    for (i, q) in path.iter().enumerate() {
                        if i > 0 {
                            along += math3::length(math3::sub(*q, path[i - 1]));
                        }
                        let t = if path.len() > 1 { i as f32 / (path.len() - 1) as f32 } else { 0.0 };
                        let scale = if scales.len() == path.len() { scales[i] } else { scales[0] + (scales[1] - scales[0]) * t };
                        side.push([scale * wide, along]);
                    }
                    self.made("sweep", vec![(profile.clone(), 1.0), (made::mirrored(&side), 2.0)]);
                } else {
                    // (A closed one: its profile, and its loop as smoothed, from where it spreads
                    // widest: a ring's circle, a frame's square.)
                    self.made("sweep", vec![(profile.clone(), 1.0), (made::plan(&smooth_path(&path, &scales, smooth as u32, closed).0), 1.0)]);
                }
                (
                    2 * m * (rings + 1),
                    Box::new(move || {
                        let (path, scales) = smooth_path(&path, &scales, smooth as u32, closed);
                        geom(Shape::sweep(&profile, &path, &scales, twist, closed, caps, up))
                    }),
                )
            }
            "union" | "difference" | "intersection" => return self.csg_shape(n),
            "sdf" | "figure" => return self.sdf_shape(n),
            "form" => return self.form_shape(n),
            other => return Err(e(&n.path, ErrorCode::UnknownOp, format!("unknown shape '{other}'"))),
        };
        self.tally.grow(&n.path, 0, predicted)?;
        let mut s = build()?;
        self.hold_closed(n, &s)?;
        self.tally.made_now(&n.path, &mut s);
        self.tally.settle(predicted, s.triangle_count());
        check_extent(&s, &n.path)?;
        Ok(s)
    }

    /// A terrain's transform after `ops` (rigid ones compose onto `start`; colour ops leave it
    /// where it is), or `None` when one reshapes it (or it was reshaped already).
    fn terrain_transform(&self, ops: &[Node], start: Option<Affine>) -> Option<Affine> {
        let mut t = start?;
        for n in ops {
            if procgen::COLOUR_OPS.contains(&n.op.as_str()) {
                continue;
            }
            if let Some(w) = &n.when {
                match self.eval(w, &format!("{}.when", n.path)) {
                    Ok(0.0) => continue,
                    Ok(_) => {}
                    Err(_) => return None,
                }
            }
            match self.rig_affine(n) {
                Ok(Some(a)) => t = a.then_after(&t),
                _ => return None,
            }
        }
        Some(t)
    }

    /// The rigid transform a `finish` op applies, so a rig can follow it: `move`, `rotate`,
    /// `scale` and `mirror` without `keep`. Other ops don't move bones (`None`); a mirror that
    /// keeps both halves can't be followed and is an error.
    fn rig_affine(&self, n: &Node) -> Result<Option<Affine>, RecipeError> {
        if let Some(w) = &n.when {
            if self.eval(w, &format!("{}.when", n.path))? == 0.0 {
                return Ok(None);
            }
        }
        let pivoted = |t: Affine, pivot: V3| Affine::translate(pivot).then_after(&t.then_after(&Affine::translate(math3::scale(pivot, -1.0))));
        Ok(match n.op.as_str() {
            "move" => Some(Affine::translate(self.vec3(n, "by", [0.0; 3])?)),
            "rotate" => {
                let d = self.vec3(n, "deg", [0.0; 3])?;
                let t = Affine::rotate_euler_deg([d[0] as f64, d[1] as f64, d[2] as f64]);
                Some(pivoted(t, self.vec3(n, "pivot", [0.0; 3])?))
            }
            "scale" => {
                let by = match n.get("by") {
                    Some(Val::Num(x)) => {
                        let v = self.eval(x, &n.at("by"))? as f32;
                        [v, v, v]
                    }
                    _ => self.vec3(n, "by", [1.0; 3])?,
                };
                Some(pivoted(Affine::scale(by), self.vec3(n, "pivot", [0.0; 3])?))
            }
            "mirror" if self.bool(n, "keep", true) => {
                return Err(e(&n.path, ErrorCode::WrongType, "a rigged recipe can't finish with a mirror that keeps both halves (bones can't be duplicated): mirror the parts and use mirrored bones"));
            }
            "mirror" => {
                let mut s = [1.0; 3];
                s[axis_index(self.str(n, "axis", "x"))] = -1.0;
                Some(Affine::scale(s))
            }
            _ => None,
        })
    }

    fn jitter(&self, n: &Node) -> Result<Jitter, RecipeError> {
        match n.get("jitter") {
            Some(Val::Obj(j)) => Ok(Jitter { offset: self.vec3(j, "offset", [0.0; 3])?, rotate_deg: self.vec3(j, "rotate", [0.0; 3])?, scale: self.ranged(j, "scale", 0.0, 0.0, 0.9)? }),
            _ => Ok(Jitter::default()),
        }
    }

    fn apply(&self, s: &mut Shape, n: &Node, seed: u64) -> Result<(), RecipeError> {
        if let Some(w) = &n.when {
            if self.eval(w, &format!("{}.when", n.path))? == 0.0 {
                return Ok(());
            }
        }
        let pivoted = |t: Affine, pivot: V3| Affine::translate(pivot).then_after(&t.then_after(&Affine::translate(math3::scale(pivot, -1.0))));
        let within = |name: &str, v: V3, max: f32| -> Result<V3, RecipeError> {
            if v.iter().any(|x| !fits(*x, max)) {
                return Err(e(&n.at(name), ErrorCode::OutOfRange, format!("values must be within ±{max}")).with_numbers(v.iter().fold(0.0f32, |a, x| a.max(x.abs())) as f64, max as f64));
            }
            Ok(v)
        };
        // Ops that multiply the triangle count are checked against the limits first.
        let before = s.triangle_count();
        let predicted = match n.op.as_str() {
            "array" => before * self.int(n, "count", 2, 1, 256)? as usize,
            "radial" => before * self.int(n, "count", 6, 1, 256)? as usize,
            "mirror" if self.bool(n, "keep", true) => before * 2,
            "subdivide" => before * 4usize.pow(self.int(n, "levels", 1, 0, 4)?),
            _ => before,
        };
        self.tally.grow(&n.path, before, predicted)?;
        // Charged before it runs, by the vertices it visits or the triangles it makes.
        let v = s.positions.len() as u64;
        let units = match n.op.as_str() {
            "displace" if self.coarse => 0,
            "displace" => v * (crate::work::TIER1_DISPLACE + crate::work::TIER1_OCTAVE * self.int(n, "octaves", 2, 1, 6)? as u64),
            "subdivide" => predicted as u64 * crate::work::TIER1_SUBDIVIDE,
            "array" | "radial" | "mirror" => predicted as u64 * crate::work::TIER1_COPY + v * crate::work::TIER1_POINT,
            "weld" => v * crate::work::TIER1_WELD,
            "ao" => v * crate::work::TIER1_CAVITY,
            "noise_tint" | "bands" => v * crate::work::TIER1_TINT,
            _ => v * crate::work::TIER1_POINT,
        };
        charge(self.tally, &n.path, units)?;
        use crate::facet::{linear, Moved};
        let mut mv: Option<Moved> = None;
        match n.op.as_str() {
            "move" => s.transform(&Affine::translate(within("by", self.vec3(n, "by", [0.0; 3])?, MAX_EXTENT_M)?)),
            "rotate" => {
                let d = within("deg", self.vec3(n, "deg", [0.0; 3])?, 36_000.0)?;
                let t = Affine::rotate_euler_deg([d[0] as f64, d[1] as f64, d[2] as f64]);
                let t = pivoted(t, within("pivot", self.vec3(n, "pivot", [0.0; 3])?, MAX_EXTENT_M)?);
                mv = Some(Moved::Linear(linear(&t)));
                s.transform(&t);
            }
            "scale" => {
                let by = match n.get("by") {
                    Some(Val::Num(x)) => {
                        let v = self.eval(x, &n.at("by"))? as f32;
                        [v, v, v]
                    }
                    _ => self.vec3(n, "by", [1.0; 3])?,
                };
                if by.iter().any(|v| !(v.abs() >= 1e-4 && v.abs() <= 1e4)) {
                    return Err(e(&n.at("by"), ErrorCode::OutOfRange, "scale factors must be 1e-4..1e4 in size"));
                }
                let t = pivoted(Affine::scale(by), within("pivot", self.vec3(n, "pivot", [0.0; 3])?, MAX_EXTENT_M)?);
                mv = Some(Moved::Linear(linear(&t)));
                s.transform(&t);
            }
            "mirror" => {
                let (axis, keep) = (axis_index(self.str(n, "axis", "x")), self.bool(n, "keep", true));
                let mut m = [1.0f32; 3];
                m[axis] = -1.0;
                let m = linear(&Affine::scale(m));
                mv = Some(if keep { Moved::Copies(vec![linear(&Affine::IDENTITY), m]) } else { Moved::Linear(m) });
                s.mirror(axis, keep);
            }
            "array" => {
                let mut rng = Rng::new(seed, 1);
                let jitter = self.jitter(n)?;
                let copies = s.array_linear(self.int(n, "count", 2, 1, 256)?, within("offset", self.vec3(n, "offset", [0.0; 3])?, MAX_EXTENT_M)?, &mut rng, &jitter);
                mv = Some(Moved::Copies(copies.iter().map(linear).collect()));
            }
            "radial" => {
                let mut rng = Rng::new(seed, 2);
                let axis = self.vec3(n, "axis", [0.0, 1.0, 0.0])?;
                if math3::length(axis).is_nan() || math3::length(axis) < 1e-6 {
                    return Err(e(&n.at("axis"), ErrorCode::OutOfRange, "axis must not be zero"));
                }
                let angle = self.ranged(n, "angle", 360.0, -3600.0, 3600.0)?;
                let jitter = self.jitter(n)?;
                let copies = s.array_radial(self.int(n, "count", 6, 1, 256)?, axis, angle as f64, &mut rng, &jitter);
                mv = Some(Moved::Copies(copies.iter().map(linear).collect()));
            }
            "subdivide" => s.subdivide(self.int(n, "levels", 1, 0, 4)?),
            "displace" => {
                let amount = self.ranged(n, "amount", 0.0, -100.0, 100.0)?;
                if self.coarse {
                    return Ok(()); // surface detail: none at the far level
                }
                let axes = within("axes", self.vec3(n, "axes", [1.0; 3])?, 10.0)?;
                mv = Some(Moved::Displaced(f64::from(amount.abs() * axes.iter().fold(0.0f32, |m, a| m.max(a.abs())))));
                s.displace(amount, self.positive(n, "scale", 0.5)?, self.int(n, "octaves", 2, 1, 6)?, seed, axes);
            }
            "taper" => {
                mv = Some(Moved::Unknown("tapered (not affine)"));
                s.taper(axis_index(self.str(n, "axis", "y")), self.ranged(n, "from", 1.0, 0.0, 100.0)?, self.ranged(n, "to", 1.0, 0.0, 100.0)?)
            }
            "twist" => {
                mv = Some(Moved::Unknown("twisted (not affine)"));
                s.twist(self.ranged(n, "deg_per_m", 0.0, -36_000.0, 36_000.0)? as f64)
            }
            "lean" => {
                let l = [self.ranged(n, "x", 0.0, -100.0, 100.0)?, self.ranged(n, "z", 0.0, -100.0, 100.0)?];
                mv = Some(Moved::Linear([[1.0, 0.0, 0.0], [f64::from(l[0]), 1.0, f64::from(l[1])], [0.0, 0.0, 1.0]]));
                s.lean(l)
            }
            "bend" => {
                mv = Some(Moved::Unknown("bent (not affine)"));
                s.bend(self.ranged(n, "deg_per_m", 0.0, -720.0, 720.0)? as f64)
            }
            "weld" => s.weld(self.ranged(n, "eps", 1e-4, 1e-7, 1.0)?),
            "color" => s.set_color(self.color(n, "color")),
            "gradient" => {
                let range = self.vec2(n, "range", [0.0, 1.0])?;
                if range.iter().any(|v| !fits(*v, MAX_EXTENT_M)) {
                    return Err(e(&n.at("range"), ErrorCode::OutOfRange, format!("range must be within ±{MAX_EXTENT_M}")));
                }
                s.gradient(&Gradient {
                    axis: axis_index(self.str(n, "axis", "y")),
                    from: self.color(n, "from"),
                    to: self.color(n, "to"),
                    range,
                    absolute: self.bool(n, "absolute", false),
                    strength: self.ranged(n, "strength", 1.0, 0.0, 1.0)?,
                    keep_outside: self.bool(n, "keep_outside", false),
                });
            }
            "bands" => {
                let colors: Vec<Rgba> = match n.get("colors") {
                    Some(Val::List(items)) => items.iter().map(|v| if let Val::Str(c) = v { self.style.color(c).unwrap_or([1.0; 4]) } else { [1.0; 4] }).collect(),
                    _ => Vec::new(),
                };
                s.bands(axis_index(self.str(n, "axis", "y")), &colors, self.positive(n, "width", 1.0)?, self.ranged(n, "wobble", 0.0, 0.0, 100.0)?, seed);
            }
            "noise_tint" => s.noise_tint(self.ranged(n, "amount", 0.1, 0.0, 1.0)?, self.positive(n, "scale", 0.5)?, seed),
            "ao" => s.concavity_shade(self.ranged(n, "amount", 0.35, 0.0, 1.0)?),
            "ground_shade" => s.ground_shade(self.ranged(n, "amount", 0.3, 0.0, 1.0)?, self.positive(n, "height", 0.5)?),
            "bevel_tint" => s.bevel_tint = Some((self.color(n, "color"), self.ranged(n, "amount", 1.0, 0.0, 1.0)?)),
            other => return Err(e(&n.path, ErrorCode::UnknownOp, format!("unknown op '{other}'"))),
        }
        if let Some(mv) = mv {
            self.tally.moved(&s.owners, &mv);
        }
        self.tally.settle(predicted, s.triangle_count());
        // An op stopped part way by a cancel fails here.
        charge(self.tally, &n.path, 0)?;
        check_extent(s, &n.path)
    }
}

/// Every vertex finite and within ±[`MAX_EXTENT_M`].
/// `|v| ≤ max`, and false for NaN.
fn fits(v: f32, max: f32) -> bool {
    v.abs() <= max
}

fn check_extent(s: &Shape, path: &str) -> Result<(), RecipeError> {
    if let Some(p) = s.positions.iter().find(|p| !p.iter().all(|v| v.abs() <= MAX_EXTENT_M)) {
        let worst = p.iter().fold(0.0f32, |a, v| if v.is_finite() { a.max(v.abs()) } else { f32::INFINITY });
        return Err(e(path, ErrorCode::Limit, format!("a vertex reached {p:?}; coordinates must stay within ±{MAX_EXTENT_M} m"))
            .with_numbers(worst as f64, MAX_EXTENT_M as f64));
    }
    Ok(())
}

/// Uniform Catmull–Rom resampling: `smooth` segments per span (1 = unchanged). Scales are
/// interpolated linearly (2 values = start/end).
fn smooth_path(path: &[V3], scales: &[f32], smooth: u32, closed: bool) -> (Vec<V3>, Vec<f32>) {
    let n = path.len();
    let scale_at = |i: usize| -> f32 {
        if scales.len() == n {
            scales[i]
        } else {
            let t = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 };
            scales[0] + (scales[1] - scales[0]) * t
        }
    };
    let base_scales: Vec<f32> = (0..n).map(scale_at).collect();
    if smooth <= 1 {
        return (path.to_vec(), base_scales);
    }
    let get = |i: isize| -> V3 {
        if closed {
            path[i.rem_euclid(n as isize) as usize]
        } else {
            path[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let spans = if closed { n } else { n - 1 };
    let mut out = Vec::new();
    let mut sc = Vec::new();
    for i in 0..spans {
        let (p0, p1, p2, p3) = (get(i as isize - 1), get(i as isize), get(i as isize + 1), get(i as isize + 2));
        let (s1, s2) = (base_scales[i], base_scales[(i + 1) % n]);
        for k in 0..smooth {
            let t = k as f32 / smooth as f32;
            let (t2, t3) = (t * t, t * t * t);
            let mut p = [0.0f32; 3];
            for c in 0..3 {
                p[c] = 0.5
                    * ((2.0 * p1[c])
                        + (-p0[c] + p2[c]) * t
                        + (2.0 * p0[c] - 5.0 * p1[c] + 4.0 * p2[c] - p3[c]) * t2
                        + (-p0[c] + 3.0 * p1[c] - 3.0 * p2[c] + p3[c]) * t3);
            }
            out.push(p);
            sc.push(s1 + (s2 - s1) * t);
        }
    }
    if !closed {
        out.push(path[n - 1]);
        sc.push(base_scales[n - 1]);
    }
    (out, sc)
}

mod form;
mod procgen;
pub use procgen::{generator_names, sdf_names};

#[cfg(test)]
mod tests;

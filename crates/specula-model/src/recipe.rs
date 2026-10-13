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
use crate::math3::{self, Affine, V2, V3};
use crate::mesh::{Bounds, MeshData};
use crate::poly;
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
}

#[derive(Clone, Copy, Debug)]
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
    o("resample", Int),
    o("jitter", Num),
    o("scale", Vec2),
    o("offset", Vec2),
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
                    let forms = ["points", "rect", "sides", "arch", "star"].iter().filter(|f| n.get(f).is_some()).count();
                    if forms != 1 {
                        return Err(e(path, ErrorCode::WrongType, "an outline needs exactly one of points, rect, sides (+radius), arch, star (+radius, inner)"));
                    }
                    if (n.get("sides").is_some() || n.get("star").is_some()) && n.get("radius").is_none() {
                        return Err(e(&n.at("radius"), ErrorCode::MissingField, "sides/star outlines need a radius"));
                    }
                    Val::Obj(n)
                }
                _ => return Err(type_err(path, "an outline: a list of [x, z] points or an object")),
            },
            Jitter => Val::Obj(self.node(path, v, "jitter", JITTER, false)?),
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
                return Err(e(&fpath, ErrorCode::UnknownField, format!("'{op}' accepts: {}{}", names.join(", "), if with_op { ", when, note" } else { "" })));
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
            return Err(e(&format!("{path}.op"), ErrorCode::UnknownOp, format!("unknown {what} '{name}' (known: {})", names.join(", "))));
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
    /// Parses and validates `text` against `style` (materials and colours must exist).
    pub fn parse(text: &str, style: &Style) -> Result<Recipe, RecipeError> {
        let json = Json::parse(text).map_err(|er| e("", ErrorCode::Json, er.message.clone()).with_offset(er.offset))?;
        crate::error::check_duplicate_keys(&json, "")?;
        let obj = json.as_object().ok_or_else(|| type_err("", "a recipe object"))?;
        const TOP: &[&str] =
            &["recipe", "name", "description", "category", "seed", "params", "let", "budget", "lods", "collider", "smooth_angle", "parts", "finish"];
        for (k, _) in obj {
            if !TOP.contains(&k.as_str()) {
                return Err(e(k, ErrorCode::UnknownField, format!("a recipe accepts: {}", TOP.join(", "))));
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
            const PART: &[&str] = &["name", "material", "color", "when", "shape", "include", "params", "ops", "smooth_angle", "lod_detail", "note"];
            for (k, _) in po {
                if !PART.contains(&k.as_str()) {
                    return Err(e(&format!("{path}.{k}"), ErrorCode::UnknownField, format!("a part accepts: {}", PART.join(", "))));
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
            let source = match (pget("shape"), pget("include")) {
                (Some(s), None) => {
                    if material.is_none() {
                        return Err(e(&format!("{path}.material"), ErrorCode::MissingField, "a shape part needs a material preset"));
                    }
                    if pget("params").is_some() {
                        return Err(e(&format!("{path}.params"), ErrorCode::UnknownField, "params belong to include parts"));
                    }
                    Source::Shape(p.op_node(&format!("{path}.shape"), s, SHAPES, "shape")?)
                }
                (None, Some(inc)) => {
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
                _ => return Err(e(&path, ErrorCode::MissingField, "a part needs exactly one of \"shape\" or \"include\"")),
            };
            let ops = p.ops(&format!("{path}.ops"), pget("ops"))?;
            let smooth_angle = pget("smooth_angle").map(|v| p.expr(&format!("{path}.smooth_angle"), v)).transpose()?;
            let lod_detail = pget("lod_detail").map(|v| p.expr(&format!("{path}.lod_detail"), v)).transpose()?;
            parts.push(PartDef { path, name: pname.to_string(), material, color, when, source, ops, smooth_angle, lod_detail });
        }
        let finish = p.ops("finish", get("finish"))?;
        Ok(Recipe { name, category, seed, params, lets, max_triangles, min_triangles, lod_ratios, collider, smooth_angle, parts, finish })
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
}

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

/// Builds an already parsed recipe.
pub fn build_recipe(recipe: &Recipe, params: &BuildParams) -> Result<Model, RecipeError> {
    let seed = params.seed.unwrap_or(recipe.seed);
    let mut stack = vec![recipe.name.clone()];
    let tally = Tally::default();
    let (built, vars) = build_parts(recipe, params.values, seed, params, &mut stack, &tally)?;
    let mut notes = Vec::new();
    let lod0 = assemble(&built.iter().map(|b| (&b.shape, b)).collect::<Vec<_>>());
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
    for (i, &ratio) in recipe.lod_ratios.iter().enumerate() {
        let inputs: Vec<LodInput> = built.iter().map(|b| LodInput { shape: &b.shape, cell_scale: b.lod_detail }).collect();
        let Some((shapes, _cell)) = lod::simplify(&inputs, ratio, bounds.min, extent) else {
            notes.push(format!("lods[{i}] ({ratio}) skipped: no simplification keeps every part within tolerance"));
            continue;
        };
        let pairs: Vec<(&Shape, &BuiltPart)> = shapes.iter().zip(&built).collect();
        let mesh = assemble(&pairs);
        mesh.validate().map_err(|er| e(&format!("lods[{i}]"), ErrorCode::InvalidMesh, er.to_string()))?;
        // An LOD that saves under 10% over the previous one is dropped: it would cost memory and
        // a switch for nothing (parts kept with lod_detail 0 can make a model irreducible).
        let prev = lods.last().map_or(0, |m| m.triangle_count());
        if mesh.triangle_count() == 0 || mesh.triangle_count() as f32 > 0.9 * prev as f32 {
            notes.push(format!("lods[{i}] ({ratio}) skipped: saves under 10% over the previous LOD"));
            continue;
        }
        lods.push(mesh);
        ratios.push(ratio);
    }
    let (collider, note) = collider::fit(recipe.collider, &lods[0].positions);
    notes.extend(note);
    let mut materials = Vec::new();
    for p in &lods[0].parts {
        if !materials.iter().any(|(n, _): &(String, MaterialPreset)| n == &p.material) {
            materials.push((p.material.clone(), params.style.materials[&p.material].clone()));
        }
    }
    let mut h = StableHasher::new();
    for m in &lods {
        h.write_u64(m.content_hash());
    }
    hash_collider(&mut h, &collider);
    let mut used: Vec<(String, f64)> = recipe.params.iter().map(|p| (p.name.clone(), vars[&p.name])).collect();
    used.sort_by(|a, b| a.0.cmp(&b.0));
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
    })
}

/// Running totals for one build, shared by every part and include, so limits hold across the
/// whole model and are checked *before* allocating.
#[derive(Default)]
struct Tally {
    /// Triangles of every piece built so far (the current one included once generated).
    triangles: std::cell::Cell<usize>,
    includes: std::cell::Cell<usize>,
}

impl Tally {
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

/// One built part, before meshing.
#[derive(Clone, Debug)]
struct BuiltPart {
    shape: Shape,
    material: String,
    smooth_angle: f32,
    lod_detail: f32,
}

fn assemble(parts: &[(&Shape, &BuiltPart)]) -> MeshData {
    let mut mesh = MeshData::default();
    for (shape, info) in parts {
        mesh.merge(&shape.finalize(info.smooth_angle, &info.material));
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
) -> Result<(Vec<BuiltPart>, BTreeMap<String, f64>), RecipeError> {
    let mut vars: BTreeMap<String, f64> = BTreeMap::new();
    for (k, v) in values {
        let Some(def) = recipe.params.iter().find(|p| p.name == *k) else {
            let names: Vec<&str> = recipe.params.iter().map(|p| p.name.as_str()).collect();
            return Err(e(&format!("params.{k}"), ErrorCode::UnknownName, format!("'{}' has no parameter '{k}' (has: {})", recipe.name, names.join(", "))));
        };
        if !v.is_finite() || *v < def.min || *v > def.max {
            return Err(e(&format!("params.{k}"), ErrorCode::OutOfRange, format!("{v} is outside {}..{}", def.min, def.max)).with_numbers(*v, def.max));
        }
        if def.integer && dmath::trunc_f64(*v) != *v {
            return Err(e(&format!("params.{k}"), ErrorCode::OutOfRange, format!("'{k}' must be an integer, got {v}")));
        }
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
    let ctx = Ctx { vars: &vars, style: params.style, seed, tally };
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
    for part in &recipe.parts {
        if let Some(w) = &part.when {
            if ctx.eval(w, &format!("{}.when", part.path))? == 0.0 {
                continue;
            }
        }
        let pseed = stream_seed(seed, &part.name);
        let mut pieces: Vec<BuiltPart> = match &part.source {
            Source::Shape(node) => {
                let material = part.material.clone().unwrap_or_default();
                let mut shape = ctx.shape(node)?;
                let col = match &part.color {
                    Some(c) => params.style.color(c).unwrap_or([1.0; 4]),
                    None => params.style.materials[&material].color,
                };
                shape.set_color(col);
                let smooth = match &part.smooth_angle {
                    Some(x) => smooth_of(x, &format!("{}.smooth_angle", part.path))?,
                    None => default_smooth,
                };
                vec![BuiltPart { shape, material, smooth_angle: smooth, lod_detail: 1.0 }]
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
                let res = build_parts(&child, &refs, pseed, params, stack, tally);
                stack.pop();
                res.map_err(|er| RecipeError { path: format!("{path}({asset}).{}", er.path), ..er })?.0
            }
        };
        if let Some(d) = &part.lod_detail {
            let v = ctx.eval(d, &format!("{}.lod_detail", part.path))? as f32;
            if !(0.0..=4.0).contains(&v) {
                return Err(e(&format!("{}.lod_detail", part.path), ErrorCode::OutOfRange, "lod_detail must be 0..4 (0 = never simplify)"));
            }
            pieces.iter_mut().for_each(|p| p.lod_detail *= v);
        }
        for piece in &mut pieces {
            for (i, op) in part.ops.iter().enumerate() {
                ctx.apply(&mut piece.shape, op, stream_seed(pseed, &format!("op{i}")))?;
            }
        }
        out.extend(pieces);
    }
    for (i, op) in recipe.finish.iter().enumerate() {
        for (k, piece) in out.iter_mut().enumerate() {
            ctx.apply(&mut piece.shape, op, stream_seed(seed, &format!("finish{i}/{k}")))?;
        }
    }
    Ok((out, vars))
}

struct Ctx<'a> {
    vars: &'a BTreeMap<String, f64>,
    style: &'a Style,
    seed: u64,
    tally: &'a Tally,
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

    /// A number in `lo..=hi`.
    fn ranged(&self, n: &Node, name: &str, default: f64, lo: f64, hi: f64) -> Result<f32, RecipeError> {
        let v = self.num(n, name, default)?;
        if v < lo || v > hi {
            return Err(e(&n.at(name), ErrorCode::OutOfRange, format!("{v} is outside {lo}..{hi}")).with_numbers(v, if v > hi { hi } else { lo }));
        }
        Ok(v as f32)
    }

    fn int(&self, n: &Node, name: &str, default: i64, lo: i64, hi: i64) -> Result<u32, RecipeError> {
        let v = self.num(n, name, default as f64)?;
        let r = dmath::round_f64(v);
        if r < lo as f64 || r > hi as f64 {
            return Err(e(&n.at(name), ErrorCode::OutOfRange, format!("{v} is outside {lo}..{hi}")).with_numbers(v, if r > hi as f64 { hi as f64 } else { lo as f64 }));
        }
        Ok(r as u32)
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
                let mut p: Vec<V2> = if let Some(pv) = n.get("points") {
                    self.rows(pv, &n.at("points"), (2, 2), max_points)?.iter().map(|r| [r[0], r[1]]).collect()
                } else if n.get("rect").is_some() {
                    let s = size2("rect")?;
                    poly::rect(s[0], s[1])
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
                if n.get("rotate").is_some() && (n.get("points").is_some() || n.get("rect").is_some() || n.get("arch").is_some()) {
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
                (60, Box::new(move || geom(Shape::cuboid(size, bevel))))
            }
            "cylinder" | "cone" => {
                let radius = self.positive(n, "radius", 1.0)?;
                let top = if n.op == "cone" { 0.0 } else { self.ranged(n, "radius_top", radius as f64, 0.0, 1e4)? };
                let height = self.positive(n, "height", 1.0)?;
                let sides = self.int(n, "sides", 12, 3, 256)?;
                let bevel = if n.op == "cone" { 0.0 } else { self.ranged(n, "bevel", 0.0, 0.0, 100.0)? };
                (8 * sides as usize, Box::new(move || geom(Shape::cylinder(radius, top, height, sides, bevel))))
            }
            "sphere" => {
                let (r, sub) = (self.positive(n, "radius", 1.0)?, self.int(n, "subdivisions", 1, 0, 5)?);
                (20 * 4usize.pow(sub), Box::new(move || Ok(Shape::sphere(r, sub))))
            }
            "torus" => {
                let radius = self.positive(n, "radius", 1.0)?;
                let tube = self.positive(n, "tube", 0.1)?;
                if tube >= radius {
                    return Err(e(&n.at("tube"), ErrorCode::OutOfRange, "tube must be smaller than radius").with_numbers(tube as f64, radius as f64));
                }
                let (segs, sides) = (self.int(n, "segments", 24, 3, 512)?, self.int(n, "sides", 8, 3, 64)?);
                (2 * segs as usize * sides as usize, Box::new(move || Ok(Shape::torus(radius, tube, segs, sides))))
            }
            "capsule" => {
                let (r, h) = (self.positive(n, "radius", 0.5)?, self.positive(n, "height", 2.0)?);
                let (segs, rings) = (self.int(n, "segments", 12, 3, 256)?, self.int(n, "rings", 3, 1, 32)?);
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
                (
                    2 * m * (rings + 1),
                    Box::new(move || {
                        let (path, scales) = smooth_path(&path, &scales, smooth as u32, closed);
                        geom(Shape::sweep(&profile, &path, &scales, twist, closed, caps, up))
                    }),
                )
            }
            other => return Err(e(&n.path, ErrorCode::UnknownOp, format!("unknown shape '{other}'"))),
        };
        self.tally.grow(&n.path, 0, predicted)?;
        let s = build()?;
        self.tally.settle(predicted, s.triangle_count());
        check_extent(&s, &n.path)?;
        Ok(s)
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
        match n.op.as_str() {
            "move" => s.transform(&Affine::translate(within("by", self.vec3(n, "by", [0.0; 3])?, MAX_EXTENT_M)?)),
            "rotate" => {
                let d = within("deg", self.vec3(n, "deg", [0.0; 3])?, 36_000.0)?;
                let t = Affine::rotate_euler_deg([d[0] as f64, d[1] as f64, d[2] as f64]);
                s.transform(&pivoted(t, within("pivot", self.vec3(n, "pivot", [0.0; 3])?, MAX_EXTENT_M)?));
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
                s.transform(&pivoted(Affine::scale(by), within("pivot", self.vec3(n, "pivot", [0.0; 3])?, MAX_EXTENT_M)?));
            }
            "mirror" => s.mirror(axis_index(self.str(n, "axis", "x")), self.bool(n, "keep", true)),
            "array" => {
                let mut rng = Rng::new(seed, 1);
                let jitter = self.jitter(n)?;
                s.array_linear(self.int(n, "count", 2, 1, 256)?, within("offset", self.vec3(n, "offset", [0.0; 3])?, MAX_EXTENT_M)?, &mut rng, &jitter);
            }
            "radial" => {
                let mut rng = Rng::new(seed, 2);
                let axis = self.vec3(n, "axis", [0.0, 1.0, 0.0])?;
                if math3::length(axis).is_nan() || math3::length(axis) < 1e-6 {
                    return Err(e(&n.at("axis"), ErrorCode::OutOfRange, "axis must not be zero"));
                }
                let angle = self.ranged(n, "angle", 360.0, -3600.0, 3600.0)?;
                let jitter = self.jitter(n)?;
                s.array_radial(self.int(n, "count", 6, 1, 256)?, axis, angle as f64, &mut rng, &jitter);
            }
            "subdivide" => s.subdivide(self.int(n, "levels", 1, 0, 4)?),
            "displace" => {
                let amount = self.ranged(n, "amount", 0.0, -100.0, 100.0)?;
                s.displace(amount, self.positive(n, "scale", 0.5)?, self.int(n, "octaves", 2, 1, 6)?, seed, within("axes", self.vec3(n, "axes", [1.0; 3])?, 10.0)?);
            }
            "taper" => s.taper(axis_index(self.str(n, "axis", "y")), self.ranged(n, "from", 1.0, 0.0, 100.0)?, self.ranged(n, "to", 1.0, 0.0, 100.0)?),
            "twist" => s.twist(self.ranged(n, "deg_per_m", 0.0, -36_000.0, 36_000.0)? as f64),
            "lean" => s.lean([self.ranged(n, "x", 0.0, -100.0, 100.0)?, self.ranged(n, "z", 0.0, -100.0, 100.0)?]),
            "bend" => s.bend(self.ranged(n, "deg_per_m", 0.0, -720.0, 720.0)? as f64),
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
        self.tally.settle(predicted, s.triangle_count());
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

#[cfg(test)]
mod tests;

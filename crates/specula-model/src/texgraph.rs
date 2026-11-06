//! Procedural materials: texture graphs as text recipes, baked to PBR texture sets on the CPU.
//!
//! A graph is JSON: named nodes, each an op over earlier nodes (so no cycles by construction),
//! and outputs naming nodes (or constants) for base colour, normal, roughness, metallic,
//! ambient occlusion, emission and opacity. Everything tiles: noise lattices wrap at the tile edge and
//! every filter samples with wrap-around, so a texture repeats seamlessly on box-projected UVs.
//!
//! ```text
//! { "texgraph": 1, "name": "mossy_stone", "size": 256, "seed": 3,
//!   "nodes": {
//!     "bricks": { "op": "bricks", "rows": 6, "cols": 3, "mortar": 0.04, "bevel": 0.08, "jitter": 0.2 },
//!     "grain":  { "op": "noise", "kind": "fbm", "scale": 8, "octaves": 5 },
//!     "height": { "op": "blend", "a": "bricks", "b": "grain", "mode": "multiply", "amount": 0.35 },
//!     "moss":   { "op": "levels", "in": "grain", "in_range": [0.55, 0.7] },
//!     "stone":  { "op": "gradient_map", "in": "height", "stops": [[0, "carved_shadow"], [1, "carved_highlight"]] },
//!     "albedo": { "op": "blend", "a": "stone", "b": { "color": "moss" }, "mode": "mix", "mask": "moss" },
//!     "rough":  { "op": "levels", "in": "height", "out_range": [0.95, 0.7] } },
//!   "outputs": { "base_color": "albedo", "height": "height", "normal_strength": 4,
//!                "roughness": "rough", "metallic": 0, "ao": { "radius": 6, "strength": 0.8 } } }
//! ```
//!
//! **Ops:** `noise` (kind value | perlin | fbm | ridged | billow | worley | worley_edge; `scale`
//! cells across the tile, `octaves`, `gain`), `bricks` (rows, cols, mortar, bevel, offset,
//! jitter), `gradient` (axis u | v | radial), `constant` (value or color), `blend` (a, b, mode
//! mix | multiply | add | subtract | screen | overlay | max | min | difference, amount, mask),
//! `curve` (in, points [[x, y]...]), `levels` (in, in_range, out_range), `invert`, `warp` (in,
//! by, amount), `blur` (in, radius px), `normal` (in, strength), `ao` (in, radius, strength),
//! `curvature` (in, strength), `gradient_map` (in, stops [[t, colour]...]). Colours are palette
//! names, material names or `#rrggbb` from the game's style. An input may be a node name or an
//! inline `{ "value": v }` / `{ "color": c }`.
//!
//! **Cutouts (foliage cards, leaves, grass, fences):** `outputs.opacity` (a grey node, 1 = solid)
//! goes into the base colour's alpha, and `outputs.alpha_cutoff` (0.05..0.95, default 0.5) makes the
//! material alpha-tested: texels below it are cut, in the scene, in the sun's shadow and in sight's
//! ID buffer alike. The engine keeps a cutout's coverage through its mip chain
//! (`specula_gpu::preserve_alpha_coverage`), so cards don't thin out with distance.
//!
//! **Bounds (the iGPU is the reference):** sizes are powers of two 16..[`MAX_SIZE`] (256 by
//! default); at most [`MAX_NODES`] nodes; every node's cost is charged to [`MAX_WORK`] before it
//! runs, and an image is freed after its last use (live memory is capped too).
//!
//! **Determinism (F2):** integer hash noise, `f32` basic operations and `sqrt`; rows are baked
//! on worker threads, each pixel independent, so the bits don't depend on the thread count.

use std::collections::BTreeMap;

use specula_core::hash::StableHasher;
use specula_core::Json;

use crate::error::{ErrorCode, RecipeError};
use crate::shape::Rgba;
use crate::style::Style;

/// The format version this crate reads.
pub const FORMAT_VERSION: u32 = 1;
pub const MIN_SIZE: u32 = 16;
pub const MAX_SIZE: u32 = 1024;
pub const DEFAULT_SIZE: u32 = 256;
pub const MAX_NODES: usize = 64;
/// Pixel operations one bake may spend (a pixel of a node costs its per-pixel work): a
/// build's whole budget at [`crate::work::TEXTURE_OP`] each.
pub const MAX_WORK: u64 = crate::work::MAX_BUILD / crate::work::TEXTURE_OP;
/// GPU bytes (mip chains included) of all one model's texture sets together.
pub const MAX_MODEL_GPU_BYTES: u64 = 32 << 20;
/// Bytes of node images alive at once.
pub const MAX_LIVE_BYTES: usize = 160 << 20;

fn err(path: &str, code: ErrorCode, m: impl Into<String>) -> RecipeError {
    RecipeError::new(path, code, m)
}

/// An input to an op: another node or a constant.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Node(usize),
    Value(f32),
    Color(Rgba),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseKind {
    Value,
    Perlin,
    Fbm,
    Ridged,
    Billow,
    Worley,
    WorleyEdge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Mix,
    Multiply,
    Add,
    Subtract,
    Screen,
    Overlay,
    Max,
    Min,
    Difference,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Noise { kind: NoiseKind, scale: u32, octaves: u32, gain: f32, seed: u64 },
    Bricks { rows: u32, cols: u32, mortar: f32, bevel: f32, offset: f32, jitter: f32, seed: u64 },
    Gradient { axis: u8 },
    Constant(Input),
    Blend { a: Input, b: Input, mode: BlendMode, amount: f32, mask: Option<Input> },
    Curve { input: Input, points: Vec<[f32; 2]> },
    Levels { input: Input, in_range: [f32; 2], out_range: [f32; 2] },
    Invert(Input),
    Warp { input: Input, by: Input, amount: f32 },
    Blur { input: Input, radius: u32 },
    Normal { input: Input, strength: f32 },
    Ao { input: Input, radius: u32, strength: f32 },
    Curvature { input: Input, strength: f32 },
    GradientMap { input: Input, stops: Vec<(f32, Rgba)> },
}

/// What the material's channels come from.
#[derive(Clone, Debug, PartialEq)]
pub struct Outputs {
    pub base_color: Input,
    /// A normal-map node, or a height to derive one from (with `normal_strength`).
    pub normal: Option<Input>,
    pub height: Option<Input>,
    pub normal_strength: f32,
    pub roughness: Input,
    pub metallic: Input,
    /// Ambient occlusion: a node, or `{radius, strength}` derived from the height; multiplied into
    /// the base colour (the engine's materials have no separate occlusion map).
    pub ao: Option<Input>,
    pub ao_from_height: Option<(u32, f32)>,
    pub emissive: Option<Input>,
    /// Coverage (1 = solid) into the base colour's alpha; with it the material is alpha-tested
    /// at `alpha_cutoff`.
    pub opacity: Option<Input>,
    pub alpha_cutoff: f32,
}

/// A parsed graph.
#[derive(Clone, Debug, PartialEq)]
pub struct TexGraph {
    pub name: String,
    pub size: u32,
    pub seed: u64,
    pub names: Vec<String>,
    pub ops: Vec<Op>,
    pub outputs: Outputs,
}

/// A baked texture set: RGBA8 images, `size × size`, rows top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub struct TextureSet {
    pub name: String,
    pub size: u32,
    /// sRGB colour; alpha is `outputs.opacity` (255 without one).
    pub base_color: Vec<u8>,
    /// Tangent-space normal (x right = +u, y up = towards the top row, as the engine's normal
    /// maps), unorm.
    pub normal: Option<Vec<u8>>,
    /// glTF metallic-roughness: G roughness, B metallic (R and A unused, 255).
    pub metallic_roughness: Vec<u8>,
    /// sRGB emissive colour, when any.
    pub emissive: Option<Vec<u8>>,
    /// Alpha-tested at this cutoff (the base colour's alpha is coverage); 0 = opaque.
    pub alpha_cutoff: f32,
    /// Content hash of the images.
    pub hash: u64,
    pub work: u64,
}

impl TextureSet {
    /// GPU bytes with mip chains (about 4/3 of the base levels).
    pub fn gpu_bytes(&self) -> u64 {
        let one = self.size as u64 * self.size as u64 * 4;
        let n = 2 + self.normal.is_some() as u64 + self.emissive.is_some() as u64;
        one * n * 4 / 3
    }

    /// A contact strip (base colour, normal, roughness, metallic) as RGBA8, `4·size × size`.
    pub fn strip(&self) -> (u32, u32, Vec<u8>) {
        let s = self.size as usize;
        let w = 4 * s;
        let mut out = vec![0u8; w * s * 4];
        let normal_default: Vec<u8> = [128u8, 128, 255, 255].repeat(s * s);
        let tiles: [(&[u8], Option<usize>); 4] = [
            (&self.base_color, None),
            (self.normal.as_deref().unwrap_or(&normal_default), None),
            (&self.metallic_roughness, Some(1)),
            (&self.metallic_roughness, Some(2)),
        ];
        for (t, (img, chan)) in tiles.iter().enumerate() {
            for y in 0..s {
                for x in 0..s {
                    let src = (y * s + x) * 4;
                    let dst = (y * w + t * s + x) * 4;
                    match chan {
                        None => out[dst..dst + 4].copy_from_slice(&img[src..src + 4]),
                        Some(c) => {
                            let v = img[src + c];
                            out[dst..dst + 4].copy_from_slice(&[v, v, v, 255]);
                        }
                    }
                }
            }
        }
        (w as u32, s as u32, out)
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing

struct P<'a> {
    style: &'a Style,
    names: Vec<String>,
}

fn num(path: &str, v: &Json) -> Result<f32, RecipeError> {
    v.as_f64().filter(|x| x.is_finite()).map(|x| x as f32).ok_or_else(|| err(path, ErrorCode::WrongType, "expected a number"))
}

fn ranged(path: &str, v: Option<&Json>, default: f32, lo: f32, hi: f32) -> Result<f32, RecipeError> {
    let x = match v {
        Some(v) => num(path, v)?,
        None => return Ok(default),
    };
    if x < lo || x > hi {
        return Err(err(path, ErrorCode::OutOfRange, format!("{x} is outside {lo}..{hi}")).with_numbers(x as f64, if x > hi { hi as f64 } else { lo as f64 }));
    }
    Ok(x)
}

fn int(path: &str, v: Option<&Json>, default: u32, lo: u32, hi: u32) -> Result<u32, RecipeError> {
    let x = ranged(path, v, default as f32, lo as f32, hi as f32)?;
    if crate::dmath::fract(x) != 0.0 {
        return Err(err(path, ErrorCode::OutOfRange, format!("{x} must be a whole number")));
    }
    Ok(x as u32)
}

fn pair(path: &str, v: Option<&Json>, default: [f32; 2]) -> Result<[f32; 2], RecipeError> {
    match v {
        None => Ok(default),
        Some(Json::Arr(a)) if a.len() == 2 => Ok([num(&format!("{path}[0]"), &a[0])?, num(&format!("{path}[1]"), &a[1])?]),
        _ => Err(err(path, ErrorCode::WrongType, "expected [a, b]")),
    }
}

impl P<'_> {
    fn color(&self, path: &str, v: &Json) -> Result<Rgba, RecipeError> {
        let s = v.as_str().ok_or_else(|| err(path, ErrorCode::WrongType, "expected a colour (\"#rrggbb\" or a palette/material name)"))?;
        self.style.color(s).ok_or_else(|| err(path, ErrorCode::UnknownColor, format!("'{s}' is not #rrggbb or a palette/material name of the style")))
    }

    fn input(&self, path: &str, v: Option<&Json>) -> Result<Input, RecipeError> {
        let v = v.ok_or_else(|| err(path, ErrorCode::MissingField, "needs an input (a node name, {\"value\": v} or {\"color\": c})"))?;
        match v {
            Json::Str(name) => match self.names.iter().position(|n| n == name) {
                Some(i) => Ok(Input::Node(i)),
                None => Err(err(path, ErrorCode::UnknownName, format!("'{name}' is not an earlier node (known: {})", self.names.join(", ")))),
            },
            Json::Num(_) => Ok(Input::Value(num(path, v)?)),
            Json::Obj(o) => {
                if o.len() != 1 {
                    return Err(err(path, ErrorCode::WrongType, "an inline input is {\"value\": v} or {\"color\": c}"));
                }
                let (k, x) = &o[0];
                match k.as_str() {
                    "value" => Ok(Input::Value(num(&format!("{path}.value"), x)?)),
                    "color" => Ok(Input::Color(self.color(&format!("{path}.color"), x)?)),
                    _ => Err(err(&format!("{path}.{k}"), ErrorCode::UnknownField, "an inline input is {\"value\": v} or {\"color\": c}")),
                }
            }
            _ => Err(err(path, ErrorCode::WrongType, "expected a node name, a number or {\"color\": c}")),
        }
    }

    fn op(&self, path: &str, v: &Json, index: usize, seed: u64) -> Result<Op, RecipeError> {
        let o = v.as_object().ok_or_else(|| err(path, ErrorCode::WrongType, "a node is an object with an \"op\""))?;
        let name = o.iter().find(|(k, _)| k == "op").and_then(|(_, v)| v.as_str()).ok_or_else(|| err(&format!("{path}.op"), ErrorCode::MissingField, "every node needs \"op\""))?;
        let fields: &[&str] = match name {
            "noise" => &["kind", "scale", "octaves", "gain", "seed"],
            "bricks" => &["rows", "cols", "mortar", "bevel", "offset", "jitter", "seed"],
            "gradient" => &["axis"],
            "constant" => &["value", "color"],
            "blend" => &["a", "b", "mode", "amount", "mask"],
            "curve" => &["in", "points"],
            "levels" => &["in", "in_range", "out_range"],
            "invert" => &["in"],
            "warp" => &["in", "by", "amount"],
            "blur" => &["in", "radius"],
            "normal" => &["in", "strength"],
            "ao" => &["in", "radius", "strength"],
            "curvature" => &["in", "strength"],
            "gradient_map" => &["in", "stops"],
            other => {
                return Err(err(
                    &format!("{path}.op"),
                    ErrorCode::UnknownOp,
                    format!("unknown op '{other}' (known: noise, bricks, gradient, constant, blend, curve, levels, invert, warp, blur, normal, ao, curvature, gradient_map)"),
                ))
            }
        };
        for (k, _) in o {
            if k != "op" && k != "note" && !fields.contains(&k.as_str()) {
                return Err(err(&format!("{path}.{k}"), ErrorCode::UnknownField, format!("'{name}' accepts: {}, note", fields.join(", "))));
            }
        }
        let g = |k: &str| o.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        let at = |k: &str| format!("{path}.{k}");
        let node_seed = |extra: Option<&Json>| -> Result<u64, RecipeError> {
            let s = match extra {
                Some(v) => v.as_f64().filter(|x| *x >= 0.0 && crate::dmath::fract_f64(*x) == 0.0 && *x < 9.0e15).ok_or_else(|| err(&at("seed"), ErrorCode::OutOfRange, "seed is a whole number >= 0"))? as u64,
                None => 0,
            };
            let mut h = StableHasher::with_seed(seed);
            h.write_u64(index as u64);
            h.write_u64(s);
            Ok(h.finish())
        };
        Ok(match name {
            "noise" => {
                let kind = match g("kind").map(|v| v.as_str()) {
                    None => NoiseKind::Fbm,
                    Some(Some("value")) => NoiseKind::Value,
                    Some(Some("perlin")) => NoiseKind::Perlin,
                    Some(Some("fbm")) => NoiseKind::Fbm,
                    Some(Some("ridged")) => NoiseKind::Ridged,
                    Some(Some("billow")) => NoiseKind::Billow,
                    Some(Some("worley")) => NoiseKind::Worley,
                    Some(Some("worley_edge")) => NoiseKind::WorleyEdge,
                    _ => return Err(err(&at("kind"), ErrorCode::WrongType, "kind is value, perlin, fbm, ridged, billow, worley or worley_edge")),
                };
                Op::Noise { kind, scale: int(&at("scale"), g("scale"), 4, 1, 256)?, octaves: int(&at("octaves"), g("octaves"), 4, 1, 10)?, gain: ranged(&at("gain"), g("gain"), 0.5, 0.0, 1.0)?, seed: node_seed(g("seed"))? }
            }
            "bricks" => Op::Bricks {
                rows: int(&at("rows"), g("rows"), 8, 1, 128)?,
                cols: int(&at("cols"), g("cols"), 4, 1, 128)?,
                mortar: ranged(&at("mortar"), g("mortar"), 0.04, 0.0, 0.4)?,
                bevel: ranged(&at("bevel"), g("bevel"), 0.08, 0.0, 0.5)?,
                offset: ranged(&at("offset"), g("offset"), 0.5, 0.0, 1.0)?,
                jitter: ranged(&at("jitter"), g("jitter"), 0.0, 0.0, 1.0)?,
                seed: node_seed(g("seed"))?,
            },
            "gradient" => Op::Gradient {
                axis: match g("axis").map(|v| v.as_str()) {
                    None | Some(Some("v")) => 1,
                    Some(Some("u")) => 0,
                    Some(Some("radial")) => 2,
                    _ => return Err(err(&at("axis"), ErrorCode::WrongType, "axis is u, v or radial")),
                },
            },
            "constant" => match (g("value"), g("color")) {
                (Some(v), None) => Op::Constant(Input::Value(num(&at("value"), v)?)),
                (None, Some(c)) => Op::Constant(Input::Color(self.color(&at("color"), c)?)),
                _ => return Err(err(path, ErrorCode::MissingField, "a constant has exactly one of value or color")),
            },
            "blend" => Op::Blend {
                a: self.input(&at("a"), g("a"))?,
                b: self.input(&at("b"), g("b"))?,
                mode: match g("mode").map(|v| v.as_str()) {
                    None | Some(Some("mix")) => BlendMode::Mix,
                    Some(Some("multiply")) => BlendMode::Multiply,
                    Some(Some("add")) => BlendMode::Add,
                    Some(Some("subtract")) => BlendMode::Subtract,
                    Some(Some("screen")) => BlendMode::Screen,
                    Some(Some("overlay")) => BlendMode::Overlay,
                    Some(Some("max")) => BlendMode::Max,
                    Some(Some("min")) => BlendMode::Min,
                    Some(Some("difference")) => BlendMode::Difference,
                    _ => return Err(err(&at("mode"), ErrorCode::WrongType, "mode is mix, multiply, add, subtract, screen, overlay, max, min or difference")),
                },
                amount: ranged(&at("amount"), g("amount"), 1.0, 0.0, 1.0)?,
                mask: g("mask").map(|m| self.input(&at("mask"), Some(m))).transpose()?,
            },
            "curve" => {
                let pts = g("points").and_then(|v| v.as_array()).ok_or_else(|| err(&at("points"), ErrorCode::MissingField, "a curve needs points [[x, y], ...] with x in 0..1"))?;
                if pts.len() < 2 || pts.len() > 32 {
                    return Err(err(&at("points"), ErrorCode::OutOfRange, "a curve needs 2..32 points"));
                }
                let mut points = Vec::new();
                for (i, p) in pts.iter().enumerate() {
                    let q = pair(&format!("{}[{i}]", at("points")), Some(p), [0.0; 2])?;
                    if !(0.0..=1.0).contains(&q[0]) || points.last().is_some_and(|l: &[f32; 2]| q[0] <= l[0]) {
                        return Err(err(&format!("{}[{i}]", at("points")), ErrorCode::OutOfRange, "curve x values rise from 0..1"));
                    }
                    points.push(q);
                }
                Op::Curve { input: self.input(&at("in"), g("in"))?, points }
            }
            "levels" => Op::Levels { input: self.input(&at("in"), g("in"))?, in_range: pair(&at("in_range"), g("in_range"), [0.0, 1.0])?, out_range: pair(&at("out_range"), g("out_range"), [0.0, 1.0])? },
            "invert" => Op::Invert(self.input(&at("in"), g("in"))?),
            "warp" => Op::Warp { input: self.input(&at("in"), g("in"))?, by: self.input(&at("by"), g("by"))?, amount: ranged(&at("amount"), g("amount"), 0.05, 0.0, 1.0)? },
            "blur" => Op::Blur { input: self.input(&at("in"), g("in"))?, radius: int(&at("radius"), g("radius"), 2, 1, 32)? },
            "normal" => Op::Normal { input: self.input(&at("in"), g("in"))?, strength: ranged(&at("strength"), g("strength"), 4.0, 0.0, 64.0)? },
            "ao" => Op::Ao { input: self.input(&at("in"), g("in"))?, radius: int(&at("radius"), g("radius"), 6, 1, 32)?, strength: ranged(&at("strength"), g("strength"), 1.0, 0.0, 4.0)? },
            "curvature" => Op::Curvature { input: self.input(&at("in"), g("in"))?, strength: ranged(&at("strength"), g("strength"), 8.0, 0.0, 256.0)? },
            "gradient_map" => {
                let st = g("stops").and_then(|v| v.as_array()).ok_or_else(|| err(&at("stops"), ErrorCode::MissingField, "gradient_map needs stops [[t, colour], ...]"))?;
                if st.len() < 2 || st.len() > 16 {
                    return Err(err(&at("stops"), ErrorCode::OutOfRange, "gradient_map needs 2..16 stops"));
                }
                let mut stops = Vec::new();
                for (i, s) in st.iter().enumerate() {
                    let p = format!("{}[{i}]", at("stops"));
                    let a = s.as_array().filter(|a| a.len() == 2).ok_or_else(|| err(&p, ErrorCode::WrongType, "a stop is [t, colour]"))?;
                    let t = num(&format!("{p}[0]"), &a[0])?;
                    if !(0.0..=1.0).contains(&t) || stops.last().is_some_and(|l: &(f32, Rgba)| t < l.0) {
                        return Err(err(&format!("{p}[0]"), ErrorCode::OutOfRange, "stop positions rise within 0..1"));
                    }
                    stops.push((t, self.color(&format!("{p}[1]"), &a[1])?));
                }
                Op::GradientMap { input: self.input(&at("in"), g("in"))?, stops }
            }
            _ => unreachable!("listed above"),
        })
    }
}

impl TexGraph {
    /// Parses a graph from its JSON; `path` prefixes error paths (`textures.moss`).
    pub fn parse_json(json: &Json, style: &Style, path: &str) -> Result<TexGraph, RecipeError> {
        let pre = |k: &str| if path.is_empty() { k.to_string() } else { format!("{path}.{k}") };
        crate::error::check_duplicate_keys(json, path)?;
        let o = json.as_object().ok_or_else(|| err(path, ErrorCode::WrongType, "a texture graph is an object"))?;
        const TOP: &[&str] = &["texgraph", "name", "description", "size", "seed", "nodes", "outputs", "note"];
        for (k, _) in o {
            if !TOP.contains(&k.as_str()) {
                return Err(err(&pre(k), ErrorCode::UnknownField, format!("a texture graph accepts: {}", TOP.join(", "))));
            }
        }
        let g = |k: &str| o.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        if let Some(v) = g("texgraph") {
            if v.as_f64() != Some(FORMAT_VERSION as f64) {
                return Err(err(&pre("texgraph"), ErrorCode::OutOfRange, format!("unsupported texgraph version (this build reads {FORMAT_VERSION})")));
            }
        }
        let name = g("name").and_then(|v| v.as_str()).unwrap_or(path).to_string();
        let size = int(&pre("size"), g("size"), DEFAULT_SIZE, MIN_SIZE, MAX_SIZE)?;
        if !size.is_power_of_two() {
            return Err(err(&pre("size"), ErrorCode::OutOfRange, format!("size {size} must be a power of two ({MIN_SIZE}..{MAX_SIZE}); 256 suits the iGPU budget")));
        }
        let seed = match g("seed") {
            Some(v) => v.as_f64().filter(|x| *x >= 0.0 && crate::dmath::fract_f64(*x) == 0.0 && *x < 9.0e15).ok_or_else(|| err(&pre("seed"), ErrorCode::OutOfRange, "seed is a whole number >= 0"))? as u64,
            None => 0,
        };
        let nodes = g("nodes").and_then(|v| v.as_object()).ok_or_else(|| err(&pre("nodes"), ErrorCode::MissingField, "a texture graph needs \"nodes\": {name: {op, ...}}"))?;
        if nodes.len() > MAX_NODES {
            return Err(err(&pre("nodes"), ErrorCode::Limit, format!("{} nodes; at most {MAX_NODES}", nodes.len())).with_numbers(nodes.len() as f64, MAX_NODES as f64));
        }
        let mut p = P { style, names: Vec::new() };
        let mut ops = Vec::new();
        for (i, (n, v)) in nodes.iter().enumerate() {
            let np = format!("{}.{n}", pre("nodes"));
            ops.push(p.op(&np, v, i, seed)?);
            p.names.push(n.clone());
        }
        let outs = g("outputs").and_then(|v| v.as_object()).ok_or_else(|| err(&pre("outputs"), ErrorCode::MissingField, "a texture graph needs \"outputs\" (base_color at least)"))?;
        const OUTS: &[&str] = &["base_color", "normal", "height", "normal_strength", "roughness", "metallic", "ao", "emissive", "opacity", "alpha_cutoff"];
        for (k, _) in outs {
            if !OUTS.contains(&k.as_str()) {
                return Err(err(&format!("{}.{k}", pre("outputs")), ErrorCode::UnknownField, format!("outputs accept: {}", OUTS.join(", "))));
            }
        }
        let og = |k: &str| outs.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        let op = |k: &str| format!("{}.{k}", pre("outputs"));
        let (ao, ao_from_height) = match og("ao") {
            None => (None, None),
            Some(Json::Obj(o)) if o.iter().any(|(k, _)| k == "radius" || k == "strength") => {
                let gg = |k: &str| o.iter().find(|(n, _)| n == k).map(|(_, v)| v);
                (None, Some((int(&format!("{}.radius", op("ao")), gg("radius"), 6, 1, 32)?, ranged(&format!("{}.strength", op("ao")), gg("strength"), 1.0, 0.0, 4.0)?)))
            }
            Some(v) => (Some(p.input(&op("ao"), Some(v))?), None),
        };
        let outputs = Outputs {
            base_color: p.input(&op("base_color"), og("base_color"))?,
            normal: og("normal").map(|v| p.input(&op("normal"), Some(v))).transpose()?,
            height: og("height").map(|v| p.input(&op("height"), Some(v))).transpose()?,
            normal_strength: ranged(&op("normal_strength"), og("normal_strength"), 4.0, 0.0, 64.0)?,
            roughness: og("roughness").map(|v| p.input(&op("roughness"), Some(v))).transpose()?.unwrap_or(Input::Value(0.8)),
            metallic: og("metallic").map(|v| p.input(&op("metallic"), Some(v))).transpose()?.unwrap_or(Input::Value(0.0)),
            ao,
            ao_from_height,
            emissive: og("emissive").map(|v| p.input(&op("emissive"), Some(v))).transpose()?,
            opacity: og("opacity").map(|v| p.input(&op("opacity"), Some(v))).transpose()?,
            alpha_cutoff: ranged(&op("alpha_cutoff"), og("alpha_cutoff"), 0.5, 0.05, 0.95)?,
        };
        if og("alpha_cutoff").is_some() && outputs.opacity.is_none() {
            return Err(err(&op("alpha_cutoff"), ErrorCode::MissingField, "alpha_cutoff cuts the opacity: give outputs.opacity (a grey node, 1 = solid)"));
        }
        if ao_from_height.is_some() && outputs.height.is_none() {
            return Err(err(&op("ao"), ErrorCode::MissingField, "ao {radius, strength} is derived from the height: give outputs.height"));
        }
        Ok(TexGraph { name, size, seed, names: p.names, ops, outputs })
    }

    /// Parses a graph from text.
    pub fn parse(text: &str, style: &Style) -> Result<TexGraph, RecipeError> {
        let json = Json::parse(text).map_err(|e| err("", ErrorCode::Json, e.message.clone()).with_offset(e.offset))?;
        TexGraph::parse_json(&json, style, "")
    }

    /// The work a bake will take (checked before it starts), pixel operations.
    pub fn cost(&self) -> u64 {
        let px = self.size as u64 * self.size as u64;
        // Per pixel, measured on one core (2026-10-22, `work_calibration`): an operation is
        // about 20 ns there ([`crate::work::TEXTURE_OP`]).
        let per = |o: &Op| -> u64 {
            match o {
                Op::Noise { kind, octaves, .. } => *octaves as u64 * if matches!(kind, NoiseKind::Worley | NoiseKind::WorleyEdge) { 30 } else { 12 },
                Op::Bricks { .. } => 12,
                // Four box passes of 2r + 1 taps, plus the copy in.
                Op::Blur { radius, .. } => 4 * (2 * *radius as u64 + 1) + 4,
                Op::Ao { radius, .. } => 44 * *radius as u64,
                Op::Warp { .. } => 22,
                Op::Normal { .. } => 24,
                Op::Curvature { .. } => 16,
                _ => 8,
            }
        };
        let mut w: u64 = self.ops.iter().map(|o| px * per(o)).sum();
        if let Some((r, _)) = self.outputs.ao_from_height {
            w += px * 44 * r as u64;
        }
        // A normal map derived from a height (Sobel taps), and the emissive image.
        if self.derives_normal() {
            w += px * 24;
        }
        if self.outputs.emissive.is_some() {
            w += px * 4;
        }
        if self.outputs.opacity.is_some() {
            w += px * 2;
        }
        w + px * 16
    }

    /// Whether the set has a normal map (given, or derived from the height); charged as if
    /// derived.
    fn derives_normal(&self) -> bool {
        self.outputs.normal.is_some() || (self.outputs.height.is_some() && self.outputs.normal_strength > 0.0)
    }

    /// The GPU bytes its texture set will take (mip chains included), known before baking.
    pub fn gpu_bytes(&self) -> u64 {
        let maps = 2 + self.derives_normal() as u64 + self.outputs.emissive.is_some() as u64;
        self.size as u64 * self.size as u64 * 4 * maps * 4 / 3
    }
}

// ---------------------------------------------------------------------------------------------
// Noise that tiles

#[inline]
fn hash2(seed: u64, x: i32, y: i32) -> u32 {
    crate::noise::hash3(seed, x, y, 0x7f31)
}

#[inline]
fn unit(h: u32) -> f32 {
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}

#[inline]
fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn wrap(i: i32, p: i32) -> i32 {
    i.rem_euclid(p)
}

/// Value noise in 0..1 on a lattice of `period` cells across (u, v in 0..1).
fn value_tile(seed: u64, u: f32, v: f32, period: i32) -> f32 {
    let (x, y) = (u * period as f32, v * period as f32);
    let (fx, fy) = (crate::dmath::floor(x), crate::dmath::floor(y));
    let (ix, iy) = (fx as i32, fy as i32);
    let (tx, ty) = (smooth(x - fx), smooth(y - fy));
    let c = |dx: i32, dy: i32| unit(hash2(seed, wrap(ix + dx, period), wrap(iy + dy, period)));
    let a = c(0, 0) + (c(1, 0) - c(0, 0)) * tx;
    let b = c(0, 1) + (c(1, 1) - c(0, 1)) * tx;
    a + (b - a) * ty
}

/// Gradient (Perlin) noise in about -1..1, tiling.
fn perlin_tile(seed: u64, u: f32, v: f32, period: i32) -> f32 {
    let (x, y) = (u * period as f32, v * period as f32);
    let (fx, fy) = (crate::dmath::floor(x), crate::dmath::floor(y));
    let (ix, iy) = (fx as i32, fy as i32);
    let (rx, ry) = (x - fx, y - fy);
    // Eight gradient directions.
    const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const G: [[f32; 2]; 8] = [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0], [D, D], [-D, D], [D, -D], [-D, -D]];
    let dot = |dx: i32, dy: i32| {
        let g = G[(hash2(seed, wrap(ix + dx, period), wrap(iy + dy, period)) & 7) as usize];
        g[0] * (rx - dx as f32) + g[1] * (ry - dy as f32)
    };
    let (tx, ty) = (smooth(rx), smooth(ry));
    let a = dot(0, 0) + (dot(1, 0) - dot(0, 0)) * tx;
    let b = dot(0, 1) + (dot(1, 1) - dot(0, 1)) * tx;
    (a + (b - a) * ty) * std::f32::consts::SQRT_2
}

/// Worley distances (F1, F2) in cell units, tiling.
fn worley_tile(seed: u64, u: f32, v: f32, period: i32) -> (f32, f32) {
    let (x, y) = (u * period as f32, v * period as f32);
    let (fx, fy) = (crate::dmath::floor(x), crate::dmath::floor(y));
    let (ix, iy) = (fx as i32, fy as i32);
    let (mut f1, mut f2) = (f32::MAX, f32::MAX);
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (cx, cy) = (ix + dx, iy + dy);
            let h = hash2(seed, wrap(cx, period), wrap(cy, period));
            let px = cx as f32 + unit(h);
            let py = cy as f32 + unit(h.rotate_left(13).wrapping_mul(0x9e37_79b9));
            let d = ((px - x) * (px - x) + (py - y) * (py - y)).sqrt();
            if d < f1 {
                f2 = f1;
                f1 = d;
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    (f1, f2)
}

fn noise_px(kind: NoiseKind, seed: u64, u: f32, v: f32, scale: u32, octaves: u32, gain: f32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let single = matches!(kind, NoiseKind::Value | NoiseKind::Perlin);
    let oct = if single { 1 } else { octaves };
    for o in 0..oct {
        let period = (scale << o.min(16)) as i32;
        let s = seed.wrapping_add((o as u64).wrapping_mul(0x632b_e59b_d9b4_e019));
        let n = match kind {
            NoiseKind::Value => value_tile(s, u, v, period),
            NoiseKind::Perlin => perlin_tile(s, u, v, period) * 0.5 + 0.5,
            NoiseKind::Fbm => perlin_tile(s, u, v, period) * 0.5 + 0.5,
            NoiseKind::Ridged => 1.0 - perlin_tile(s, u, v, period).abs(),
            NoiseKind::Billow => perlin_tile(s, u, v, period).abs(),
            NoiseKind::Worley => worley_tile(s, u, v, period).0.min(1.0),
            NoiseKind::WorleyEdge => {
                let (a, b) = worley_tile(s, u, v, period);
                (b - a).min(1.0)
            }
        };
        sum += n * amp;
        norm += amp;
        amp *= gain;
    }
    (sum / norm).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------------------------
// Baking

#[derive(Clone, Debug)]
enum Img {
    Gray(Vec<f32>),
    Color(Vec<Rgba>),
}

impl Img {
    fn bytes(&self) -> usize {
        match self {
            Img::Gray(v) => v.len() * 4,
            Img::Color(v) => v.len() * 16,
        }
    }
}

/// Runs `f(row) -> row values` over every row on up to `threads` threads; rows in order.
fn rows<T: Send + Clone + Default>(size: usize, threads: usize, f: &(dyn Fn(usize, &mut [T]) + Sync)) -> Vec<T> {
    let mut out = vec![T::default(); size * size];
    let threads = threads.clamp(1, 16).min(size);
    let per = size.div_ceil(threads);
    // Rows are split by index and written in place: the bits never depend on the thread count
    // or timing (tested at 1-16 threads).
    #[allow(clippy::disallowed_methods, reason = "rows split by index, each written in place: the result is the same at any thread count or timing")]
    std::thread::scope(|sc| {
        for (c, chunk) in out.chunks_mut(per * size).enumerate() {
            sc.spawn(move || {
                for (r, row) in chunk.chunks_mut(size).enumerate() {
                    f(c * per + r, row);
                }
            });
        }
    });
    out
}

struct Ctx<'a> {
    size: usize,
    imgs: &'a [Option<Img>],
}

impl Ctx<'_> {
    fn gray(&self, i: &Input, x: usize, y: usize) -> f32 {
        match i {
            Input::Value(v) => *v,
            Input::Color(c) => (c[0] + c[1] + c[2]) / 3.0,
            Input::Node(n) => match self.imgs[*n].as_ref().expect("alive") {
                Img::Gray(g) => g[y * self.size + x],
                Img::Color(c) => {
                    let c = c[y * self.size + x];
                    (c[0] + c[1] + c[2]) / 3.0
                }
            },
        }
    }
    fn color(&self, i: &Input, x: usize, y: usize) -> Rgba {
        match i {
            Input::Value(v) => [*v, *v, *v, 1.0],
            Input::Color(c) => *c,
            Input::Node(n) => match self.imgs[*n].as_ref().expect("alive") {
                Img::Gray(g) => {
                    let v = g[y * self.size + x];
                    [v, v, v, 1.0]
                }
                Img::Color(c) => c[y * self.size + x],
            },
        }
    }
    fn is_color(&self, i: &Input) -> bool {
        match i {
            Input::Color(_) => true,
            Input::Value(_) => false,
            Input::Node(n) => matches!(self.imgs[*n], Some(Img::Color(_))),
        }
    }
    /// Bilinear, wrapping, at (u, v) in 0..1.
    fn sample(&self, i: &Input, u: f32, v: f32) -> f32 {
        let s = self.size as f32;
        let (x, y) = (u * s - 0.5, v * s - 0.5);
        let (fx, fy) = (crate::dmath::floor(x), crate::dmath::floor(y));
        let (tx, ty) = (x - fx, y - fy);
        let n = self.size as i32;
        let at = |dx: i32, dy: i32| self.gray(i, wrap(fx as i32 + dx, n) as usize, wrap(fy as i32 + dy, n) as usize);
        let a = at(0, 0) + (at(1, 0) - at(0, 0)) * tx;
        let b = at(0, 1) + (at(1, 1) - at(0, 1)) * tx;
        a + (b - a) * ty
    }
    fn wrapped(&self, i: &Input, x: i32, y: i32) -> f32 {
        let n = self.size as i32;
        self.gray(i, wrap(x, n) as usize, wrap(y, n) as usize)
    }
}

fn blend1(a: f32, b: f32, mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Mix => b,
        BlendMode::Multiply => a * b,
        BlendMode::Add => a + b,
        BlendMode::Subtract => a - b,
        BlendMode::Screen => 1.0 - (1.0 - a) * (1.0 - b),
        BlendMode::Overlay => {
            if a < 0.5 {
                2.0 * a * b
            } else {
                1.0 - 2.0 * (1.0 - a) * (1.0 - b)
            }
        }
        BlendMode::Max => a.max(b),
        BlendMode::Min => a.min(b),
        BlendMode::Difference => (a - b).abs(),
    }
}

fn curve(points: &[[f32; 2]], x: f32) -> f32 {
    if x <= points[0][0] {
        return points[0][1];
    }
    for w in points.windows(2) {
        if x <= w[1][0] {
            let t = (x - w[0][0]) / (w[1][0] - w[0][0]);
            return w[0][1] + (w[1][1] - w[0][1]) * t;
        }
    }
    points[points.len() - 1][1]
}

/// Bakes a graph on up to `threads` threads.
pub fn bake(g: &TexGraph, threads: usize) -> Result<TextureSet, RecipeError> {
    let cost = g.cost();
    if cost > MAX_WORK {
        return Err(err("size", ErrorCode::Limit, format!("the graph would take {cost} pixel operations; at most {MAX_WORK} (lower the size, octaves, blur or ao radius)")).with_numbers(cost as f64, MAX_WORK as f64));
    }
    let size = g.size as usize;
    // Last use of each node (outputs count as after every node).
    let mut last = vec![0usize; g.ops.len()];
    let uses = |i: &Input, at: usize, last: &mut Vec<usize>| {
        if let Input::Node(n) = i {
            last[*n] = last[*n].max(at);
        }
    };
    for (k, o) in g.ops.iter().enumerate() {
        match o {
            Op::Constant(i) | Op::Invert(i) => uses(i, k, &mut last),
            Op::Blend { a, b, mask, .. } => {
                uses(a, k, &mut last);
                uses(b, k, &mut last);
                if let Some(m) = mask {
                    uses(m, k, &mut last);
                }
            }
            Op::Curve { input, .. } | Op::Levels { input, .. } | Op::Blur { input, .. } | Op::Normal { input, .. } | Op::Ao { input, .. } | Op::Curvature { input, .. } | Op::GradientMap { input, .. } => {
                uses(input, k, &mut last)
            }
            Op::Warp { input, by, .. } => {
                uses(input, k, &mut last);
                uses(by, k, &mut last);
            }
            _ => {}
        }
    }
    let end = g.ops.len();
    let o = &g.outputs;
    for i in [Some(&o.base_color), o.normal.as_ref(), o.height.as_ref(), Some(&o.roughness), Some(&o.metallic), o.ao.as_ref(), o.emissive.as_ref(), o.opacity.as_ref()].into_iter().flatten() {
        uses(i, end, &mut last);
    }
    let mut imgs: Vec<Option<Img>> = vec![None; g.ops.len()];
    for (k, op) in g.ops.iter().enumerate() {
        let img = {
            let c = Ctx { size, imgs: &imgs };
            let uv = |x: usize, y: usize| ((x as f32 + 0.5) / size as f32, (y as f32 + 0.5) / size as f32);
            match op {
                Op::Noise { kind, scale, octaves, gain, seed } => Img::Gray(rows(size, threads, &|y, row: &mut [f32]| {
                    for (x, p) in row.iter_mut().enumerate() {
                        let (u, v) = uv(x, y);
                        *p = noise_px(*kind, *seed, u, v, *scale, *octaves, *gain);
                    }
                })),
                Op::Bricks { rows: nr, cols: nc, mortar, bevel, offset, jitter, seed } => Img::Gray(rows(size, threads, &|y, row: &mut [f32]| {
                    for (x, p) in row.iter_mut().enumerate() {
                        let (u, v) = uv(x, y);
                        let rv = v * *nr as f32;
                        let r = crate::dmath::floor(rv);
                        let shift = if (r as i32) % 2 == 1 { *offset } else { 0.0 };
                        let cu = u * *nc as f32 + shift;
                        let ci = crate::dmath::floor(cu);
                        let (fu, fv) = (cu - ci, rv - r);
                        // Distance to the brick's edge, in brick-height units.
                        let aspect = *nr as f32 / *nc as f32;
                        let du = fu.min(1.0 - fu) / aspect;
                        let d = du.min(fv.min(1.0 - fv));
                        let edge = ((d - mortar * 0.5) / bevel.max(1e-4)).clamp(0.0, 1.0);
                        let id = hash2(*seed, wrap(ci as i32, *nc as i32), wrap(r as i32, *nr as i32));
                        let lift = 1.0 - jitter * unit(id);
                        *p = if d < mortar * 0.5 { 0.0 } else { smooth(edge) * lift };
                    }
                })),
                Op::Gradient { axis } => Img::Gray(rows(size, threads, &|y, row: &mut [f32]| {
                    for (x, p) in row.iter_mut().enumerate() {
                        let (u, v) = uv(x, y);
                        *p = match axis {
                            0 => u,
                            1 => v,
                            _ => (1.0 - 2.0 * ((u - 0.5) * (u - 0.5) + (v - 0.5) * (v - 0.5)).sqrt()).clamp(0.0, 1.0),
                        };
                    }
                })),
                Op::Constant(i) => match i {
                    Input::Color(col) => Img::Color(vec![*col; size * size]),
                    _ => Img::Gray(vec![c.gray(i, 0, 0); size * size]),
                },
                Op::Blend { a, b, mode, amount, mask } => {
                    if c.is_color(a) || c.is_color(b) {
                        Img::Color(rows(size, threads, &|y, row: &mut [Rgba]| {
                            for (x, p) in row.iter_mut().enumerate() {
                                let (ca, cb) = (c.color(a, x, y), c.color(b, x, y));
                                let t = amount * mask.as_ref().map_or(1.0, |m| c.gray(m, x, y).clamp(0.0, 1.0));
                                let mut o = [0.0f32; 4];
                                for q in 0..4 {
                                    let r = blend1(ca[q], cb[q], *mode);
                                    o[q] = (ca[q] + (r - ca[q]) * t).clamp(0.0, 1.0);
                                }
                                *p = o;
                            }
                        }))
                    } else {
                        Img::Gray(rows(size, threads, &|y, row: &mut [f32]| {
                            for (x, p) in row.iter_mut().enumerate() {
                                let (ga, gb) = (c.gray(a, x, y), c.gray(b, x, y));
                                let t = amount * mask.as_ref().map_or(1.0, |m| c.gray(m, x, y).clamp(0.0, 1.0));
                                *p = (ga + (blend1(ga, gb, *mode) - ga) * t).clamp(0.0, 1.0);
                            }
                        }))
                    }
                }
                Op::Curve { input, points } => Img::Gray(rows(size, threads, &|y, row: &mut [f32]| {
                    for (x, p) in row.iter_mut().enumerate() {
                        *p = curve(points, c.gray(input, x, y)).clamp(0.0, 1.0);
                    }
                })),
                Op::Levels { input, in_range, out_range } => Img::Gray(rows(size, threads, &|y, row: &mut [f32]| {
                    let span = in_range[1] - in_range[0];
                    for (x, p) in row.iter_mut().enumerate() {
                        let t = if span.abs() < 1e-6 { if c.gray(input, x, y) >= in_range[0] { 1.0 } else { 0.0 } } else { ((c.gray(input, x, y) - in_range[0]) / span).clamp(0.0, 1.0) };
                        *p = out_range[0] + (out_range[1] - out_range[0]) * t;
                    }
                })),
                Op::Invert(i) => {
                    if c.is_color(i) {
                        Img::Color(rows(size, threads, &|y, row: &mut [Rgba]| {
                            for (x, p) in row.iter_mut().enumerate() {
                                let q = c.color(i, x, y);
                                *p = [1.0 - q[0], 1.0 - q[1], 1.0 - q[2], q[3]];
                            }
                        }))
                    } else {
                        Img::Gray(rows(size, threads, &|y, row: &mut [f32]| {
                            for (x, p) in row.iter_mut().enumerate() {
                                *p = 1.0 - c.gray(i, x, y);
                            }
                        }))
                    }
                }
                Op::Warp { input, by, amount } => Img::Gray(rows(size, threads, &|y, row: &mut [f32]| {
                    for (x, p) in row.iter_mut().enumerate() {
                        let (u, v) = uv(x, y);
                        let du = (c.gray(by, x, y) - 0.5) * amount;
                        let dv = (c.sample(by, u + 0.37, v + 0.61) - 0.5) * amount;
                        *p = c.sample(input, u + du, v + dv);
                    }
                })),
                Op::Blur { input, radius } => {
                    // Two box passes per axis: close to a Gaussian.
                    let r = *radius as i32;
                    let mut cur: Vec<f32> = (0..size * size).map(|i| c.gray(input, i % size, i / size)).collect();
                    for pass in 0..4 {
                        let horizontal = pass % 2 == 0;
                        let src = cur.clone();
                        cur = rows(size, threads, &|y, row: &mut [f32]| {
                            for (x, p) in row.iter_mut().enumerate() {
                                let mut s = 0.0;
                                for d in -r..=r {
                                    let (sx, sy) = if horizontal { (wrap(x as i32 + d, size as i32), y as i32) } else { (x as i32, wrap(y as i32 + d, size as i32)) };
                                    s += src[sy as usize * size + sx as usize];
                                }
                                *p = s / (2 * r + 1) as f32;
                            }
                        });
                    }
                    Img::Gray(cur)
                }
                Op::Normal { input, strength } => Img::Color(normal_rows(&c, input, *strength, size, threads)),
                Op::Ao { input, radius, strength } => Img::Gray(ao_rows(&c, input, *radius, *strength, size, threads)),
                Op::Curvature { input, strength } => Img::Gray(rows(size, threads, &|y, row: &mut [f32]| {
                    for (x, p) in row.iter_mut().enumerate() {
                        let (xi, yi) = (x as i32, y as i32);
                        let lap = c.wrapped(input, xi - 1, yi) + c.wrapped(input, xi + 1, yi) + c.wrapped(input, xi, yi - 1) + c.wrapped(input, xi, yi + 1) - 4.0 * c.gray(input, x, y);
                        *p = (0.5 - lap * strength).clamp(0.0, 1.0);
                    }
                })),
                Op::GradientMap { input, stops } => Img::Color(rows(size, threads, &|y, row: &mut [Rgba]| {
                    for (x, p) in row.iter_mut().enumerate() {
                        let t = c.gray(input, x, y).clamp(0.0, 1.0);
                        let mut col = stops[stops.len() - 1].1;
                        if t <= stops[0].0 {
                            col = stops[0].1;
                        } else {
                            for w in stops.windows(2) {
                                if t <= w[1].0 {
                                    let f = if w[1].0 > w[0].0 { (t - w[0].0) / (w[1].0 - w[0].0) } else { 1.0 };
                                    col = crate::shape::mix(w[0].1, w[1].1, f);
                                    break;
                                }
                            }
                        }
                        *p = col;
                    }
                })),
            }
        };
        imgs[k] = Some(img);
        if crate::work::cancelled() {
            return Err(err(&format!("nodes.{}", g.names[k]), ErrorCode::Limit, crate::work::CANCELLED));
        }
        // Free what nothing later reads (outputs read last).
        for j in 0..=k {
            if last[j] <= k && imgs[j].is_some() {
                imgs[j] = None;
            }
        }
        let live: usize = imgs.iter().flatten().map(Img::bytes).sum();
        if live > MAX_LIVE_BYTES {
            return Err(err(&format!("nodes.{}", g.names[k]), ErrorCode::Limit, format!("{live} bytes of images alive at once; at most {MAX_LIVE_BYTES} (lower the size)")).with_numbers(live as f64, MAX_LIVE_BYTES as f64));
        }
    }
    // Outputs.
    let c = Ctx { size, imgs: &imgs };
    let ao_img: Option<Vec<f32>> = match (&o.ao, o.ao_from_height, &o.height) {
        (Some(i), _, _) => Some((0..size * size).map(|p| c.gray(i, p % size, p / size)).collect()),
        (None, Some((r, s)), Some(h)) => Some(ao_rows(&c, h, r, s, size, threads)),
        _ => None,
    };
    let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let mut base = Vec::with_capacity(size * size * 4);
    for p in 0..size * size {
        let (x, y) = (p % size, p / size);
        let col = c.color(&o.base_color, x, y);
        let a = ao_img.as_ref().map_or(1.0, |ao| ao[p]);
        let alpha = o.opacity.as_ref().map_or(255, |i| to8(c.gray(i, x, y)));
        base.extend_from_slice(&[to8(col[0] * a), to8(col[1] * a), to8(col[2] * a), alpha]);
    }
    let normal_src: Option<Vec<Rgba>> = match (&o.normal, &o.height) {
        (Some(i), _) if c.is_color(i) => Some((0..size * size).map(|p| c.color(i, p % size, p / size)).collect()),
        (Some(i), _) => Some(normal_rows(&c, i, o.normal_strength, size, threads)),
        (None, Some(h)) if o.normal_strength > 0.0 => Some(normal_rows(&c, h, o.normal_strength, size, threads)),
        _ => None,
    };
    let normal = normal_src.map(|n| n.iter().flat_map(|q| [to8(q[0]), to8(q[1]), to8(q[2]), 255]).collect::<Vec<u8>>());
    let mut mr = Vec::with_capacity(size * size * 4);
    for p in 0..size * size {
        let (x, y) = (p % size, p / size);
        mr.extend_from_slice(&[255, to8(c.gray(&o.roughness, x, y)), to8(c.gray(&o.metallic, x, y)), 255]);
    }
    let emissive = o.emissive.as_ref().map(|i| (0..size * size).flat_map(|p| {
        let q = c.color(i, p % size, p / size);
        [to8(q[0]), to8(q[1]), to8(q[2]), 255]
    }).collect::<Vec<u8>>());
    let mut h = StableHasher::with_seed(0x7465_7867);
    h.write_u32(g.size);
    h.write_bytes(&base);
    if let Some(n) = &normal {
        h.write_bytes(n);
    }
    h.write_bytes(&mr);
    if let Some(e) = &emissive {
        h.write_bytes(e);
    }
    let alpha_cutoff = if o.opacity.is_some() { o.alpha_cutoff } else { 0.0 };
    // (Opaque sets hash exactly as they did before cutouts existed.)
    if o.opacity.is_some() {
        h.write_u32(alpha_cutoff.to_bits());
    }
    Ok(TextureSet { name: g.name.clone(), size: g.size, base_color: base, normal, metallic_roughness: mr, emissive, alpha_cutoff, hash: h.finish(), work: cost })
}

/// A tangent-space normal map from a height (0..1 per texel × `strength`): x along +u, y towards
/// the image's top row (−v), encoded 0..1.
fn normal_rows(c: &Ctx, h: &Input, strength: f32, size: usize, threads: usize) -> Vec<Rgba> {
    rows(size, threads, &|y, row: &mut [Rgba]| {
        for (x, p) in row.iter_mut().enumerate() {
            let (xi, yi) = (x as i32, y as i32);
            let du = (c.wrapped(h, xi + 1, yi) - c.wrapped(h, xi - 1, yi)) * 0.5 * strength;
            let dv = (c.wrapped(h, xi, yi + 1) - c.wrapped(h, xi, yi - 1)) * 0.5 * strength;
            // n ∝ (-dh/du, +dh/dv, 1): +y is towards decreasing v.
            let (nx, ny, nz) = (-du, dv, 1.0f32);
            let l = (nx * nx + ny * ny + nz * nz).sqrt();
            *p = [nx / l * 0.5 + 0.5, ny / l * 0.5 + 0.5, nz / l * 0.5 + 0.5, 1.0];
        }
    })
}

/// Ambient occlusion from a height: in eight directions, the steepest rise within `radius`
/// texels (heights scaled so 1 = `radius` texels), averaged; 1 = open.
fn ao_rows(c: &Ctx, h: &Input, radius: u32, strength: f32, size: usize, threads: usize) -> Vec<f32> {
    const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const DIRS: [[f32; 2]; 8] = [[1.0, 0.0], [D, D], [0.0, 1.0], [-D, D], [-1.0, 0.0], [-D, -D], [0.0, -1.0], [D, -D]];
    let r = radius.max(1);
    rows(size, threads, &|y, row: &mut [f32]| {
        for (x, p) in row.iter_mut().enumerate() {
            let h0 = c.gray(h, x, y);
            let mut occ = 0.0;
            for d in DIRS {
                let mut best = 0.0f32;
                for s in 1..=r {
                    let (sx, sy) = (x as f32 + d[0] * s as f32, y as f32 + d[1] * s as f32);
                    let hs = c.wrapped(h, crate::dmath::round(sx) as i32, crate::dmath::round(sy) as i32);
                    let rise = (hs - h0) * r as f32 / s as f32;
                    best = best.max(rise);
                }
                occ += best.min(1.0);
            }
            *p = (1.0 - strength * occ / 8.0).clamp(0.0, 1.0);
        }
    })
}

/// Graphs by name (a recipe's `textures` and the library's), for lookups.
pub type Library = BTreeMap<String, TexGraph>;

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> Style {
        Style::parse(r##"{ "palette": { "moss": "#4f6b3a", "stone": "#8c96a3", "dark": "#3d434d" }, "materials": { "carved_stone": { "color": "#8c96a3" } } }"##).unwrap()
    }

    const GRAPH: &str = r#"{ "texgraph": 1, "name": "mossy", "size": 64, "seed": 3,
      "nodes": {
        "bricks": { "op": "bricks", "rows": 4, "cols": 2, "mortar": 0.05, "bevel": 0.1, "jitter": 0.3 },
        "grain": { "op": "noise", "kind": "fbm", "scale": 4, "octaves": 4 },
        "height": { "op": "blend", "a": "bricks", "b": "grain", "mode": "multiply", "amount": 0.4 },
        "moss": { "op": "levels", "in": "grain", "in_range": [0.55, 0.7] },
        "stone": { "op": "gradient_map", "in": "height", "stops": [[0, "dark"], [1, "stone"]] },
        "albedo": { "op": "blend", "a": "stone", "b": { "color": "moss" }, "mask": "moss" },
        "rough": { "op": "levels", "in": "height", "out_range": [0.95, 0.7] },
        "soft": { "op": "blur", "in": "grain", "radius": 2 },
        "w": { "op": "warp", "in": "soft", "by": "grain", "amount": 0.1 },
        "edges": { "op": "curvature", "in": "height" },
        "cells": { "op": "noise", "kind": "worley_edge", "scale": 6 } },
      "outputs": { "base_color": "albedo", "height": "height", "normal_strength": 4, "roughness": "rough", "metallic": 0, "ao": { "radius": 4, "strength": 0.6 } } }"#;

    #[test]
    fn bakes_deterministically_on_any_thread_count() {
        let g = TexGraph::parse(GRAPH, &style()).unwrap();
        let a = bake(&g, 1).unwrap();
        let b = bake(&g, 4).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.base_color.len(), 64 * 64 * 4);
        assert!(a.normal.is_some());
        // Not flat: the base colour and the normals vary.
        assert!(a.base_color.chunks(4).any(|p| p != &a.base_color[0..4]));
        assert!(a.normal.as_ref().unwrap().chunks(4).any(|p| p[0] != 128));
        assert_eq!(a.gpu_bytes(), 64 * 64 * 4 * 3 * 4 / 3);
        let (w, h, strip) = a.strip();
        assert_eq!((w, h, strip.len()), (256, 64, 256 * 64 * 4));
    }

    #[test]
    fn noise_tiles_seamlessly() {
        // The last column continues into the first: neighbours across the seam differ like
        // neighbours anywhere else.
        let n = 128;
        let px = |x: usize, y: usize| noise_px(NoiseKind::Fbm, 9, (x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32, 4, 4, 0.5);
        let seam: f32 = (0..n).map(|y| (px(n - 1, y) - px(0, y)).abs()).sum::<f32>() / n as f32;
        let inner: f32 = (0..n).map(|y| (px(60, y) - px(61, y)).abs()).sum::<f32>() / n as f32;
        assert!(seam < inner * 3.0 + 0.01, "seam {seam} vs inner {inner}");
    }

    #[test]
    fn errors_have_paths() {
        let s = style();
        let bad = |t: &str| TexGraph::parse(t, &s).unwrap_err();
        let e = bad(r#"{ "nodes": { "a": { "op": "noise", "scale": 0 } }, "outputs": { "base_color": "a" } }"#);
        assert_eq!((e.path.as_str(), e.code), ("nodes.a.scale", ErrorCode::OutOfRange));
        let e = bad(r#"{ "nodes": { "a": { "op": "blend", "a": "b", "b": 1 } }, "outputs": { "base_color": "a" } }"#);
        assert_eq!(e.code, ErrorCode::UnknownName);
        let e = bad(r#"{ "size": 300, "nodes": {}, "outputs": { "base_color": 1 } }"#);
        assert_eq!(e.path, "size");
        let e = bad(r#"{ "nodes": { "a": { "op": "frobnicate" } }, "outputs": { "base_color": "a" } }"#);
        assert_eq!(e.code, ErrorCode::UnknownOp);
        let e = bad(r#"{ "nodes": { "c": { "op": "constant", "color": "nope" } }, "outputs": { "base_color": "c" } }"#);
        assert_eq!(e.code, ErrorCode::UnknownColor);
        // Too much work for the budget is refused before baking.
        let big = r#"{ "size": 1024, "nodes": { "a": { "op": "noise", "kind": "worley", "octaves": 10 }, "b": { "op": "ao", "in": "a", "radius": 32 }, "c": { "op": "ao", "in": "b", "radius": 32 }, "d": { "op": "ao", "in": "c", "radius": 32 }, "e": { "op": "ao", "in": "d", "radius": 32 }, "f": { "op": "ao", "in": "e", "radius": 32 }, "g": { "op": "ao", "in": "f", "radius": 32 }, "h": { "op": "ao", "in": "g", "radius": 32 } }, "outputs": { "base_color": "h" } }"#;
        let g = TexGraph::parse(big, &s).unwrap();
        assert_eq!(bake(&g, 2).unwrap_err().code, ErrorCode::Limit);
    }

    /// `outputs.opacity` goes into the base colour's alpha and makes the set alpha-tested at
    /// `alpha_cutoff` (0.5 by default); without it the alpha is solid and the set opaque.
    #[test]
    fn opacity_makes_a_cutout() {
        let s = style();
        let card = r#"{ "size": 16, "nodes": { "blade": { "op": "gradient", "axis": "u" } },
          "outputs": { "base_color": { "color": "moss" }, "opacity": "blade" } }"#;
        let set = bake(&TexGraph::parse(card, &s).unwrap(), 2).unwrap();
        assert_eq!(set.alpha_cutoff, 0.5);
        // Row 0: alpha rises with u from about 0 to about 255.
        let alpha: Vec<u8> = (0..16).map(|x| set.base_color[x * 4 + 3]).collect();
        assert!(alpha[0] < 16 && alpha[15] > 240 && alpha.windows(2).all(|w| w[0] <= w[1]), "{alpha:?}");
        let tuned = card.replace(r#""opacity": "blade""#, r#""opacity": "blade", "alpha_cutoff": 0.3"#);
        let set2 = bake(&TexGraph::parse(&tuned, &s).unwrap(), 2).unwrap();
        assert_eq!(set2.alpha_cutoff, 0.3);
        assert_ne!(set.hash, set2.hash, "the cutoff is part of the set's identity");
        // Opaque without opacity.
        let solid = bake(&TexGraph::parse(r#"{ "size": 16, "nodes": {}, "outputs": { "base_color": { "color": "moss" } } }"#, &s).unwrap(), 2).unwrap();
        assert_eq!(solid.alpha_cutoff, 0.0);
        assert!(solid.base_color.chunks(4).all(|p| p[3] == 255));
        // A cutoff needs an opacity, and stays in range.
        let e = TexGraph::parse(r#"{ "nodes": {}, "outputs": { "base_color": 1, "alpha_cutoff": 0.5 } }"#, &s).unwrap_err();
        assert_eq!((e.path.as_str(), e.code), ("outputs.alpha_cutoff", ErrorCode::MissingField));
        let e = TexGraph::parse(&card.replace(r#""opacity": "blade""#, r#""opacity": "blade", "alpha_cutoff": 1"#), &s).unwrap_err();
        assert_eq!((e.path.as_str(), e.code), ("outputs.alpha_cutoff", ErrorCode::OutOfRange));
    }
}

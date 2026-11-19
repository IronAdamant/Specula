//! Recipe glue for procedural tiers 2-4: boolean and SDF shapes, the building, scatter and
//! terrain generators, and texture graphs. Parsing reuses the recipe schema machinery (strict
//! fields, expressions, paths); building converts to the pure modules ([`crate::csg`],
//! [`crate::sdf`], [`crate::building`], [`crate::scatter`], [`crate::terrain`],
//! [`crate::texgraph`]) and charges everything to the build's tally.

use std::sync::Arc;

use super::*;
use crate::building::{self, BuildingSpec, Role, RoofKind};
use crate::csg::{self, Bevel, CsgErrorKind, CsgOp, CsgTree};
use crate::scatter::{self, ScatterSpec};
use crate::sdf;
use crate::terrain::{self, Terrain, TerrainSpec};
use crate::texgraph::{self, TexGraph, TextureSet};

/// Most operands of one boolean, items of one scatter, variants of one item.
pub(super) const MAX_OPERANDS: usize = 64;
const MAX_ITEMS: usize = 16;
const MAX_VARIANTS: u32 = 8;
/// Stats entries one build keeps.
const MAX_STATS: usize = 64;
/// Work units ([`crate::work`], ≈ ns: boolean classifications, SDF evaluations, terrain
/// visits, scatter candidates, texture bakes) one model build may spend on all its booleans,
/// sculpts, generators and textures together, includes too.
pub const MAX_BUILD_WORK: u64 = crate::work::MAX_BUILD;
/// Ops that only colour a shape (a terrain stays where its heights say).
pub(super) const COLOUR_OPS: &[&str] = &["color", "gradient", "bands", "noise_tint", "ao", "ground_shade", "bevel_tint"];

// ---------------------------------------------------------------------------------------------
// Schema

pub(super) const CSG_FIELDS: &[Field] = &[r("of", Operands), o("bevel", Num), o("bevel_segments", Int)];
pub(super) const SDF_FIELDS: &[Field] = &[r("tree", Sdf), o("resolution", Int), o("bounds", Rows)];
/// A human figure from a measured body ([`crate::figure`]): its sex, its stature (metres), its
/// build (0 is the usual weight for the stature) or its weight (kilograms), its age (years),
/// its traits by name (standard deviations), and how far its arms and legs are held out.
pub(super) const FIGURE_FIELDS: &[Field] = &[
    r("sex", Choice(&["female", "male"])),
    o("part", Choice(&crate::figure::Part::NAMES)),
    o("stature", Num),
    o("build", Num),
    o("weight", Num),
    o("age", Num),
    o("traits", Ranges),
    o("arms", Num),
    o("legs", Num),
    o("smooth", Num),
    o("resolution", Int),
];

const REPEAT: &[Field] = &[r("count", Vec3), r("spacing", Vec3)];
const DISPLACE: &[Field] = &[r("amount", Num), o("scale", Num), o("octaves", Int)];
const MIRROR: &[&str] = &["x", "y", "z", "xy", "xz", "yz", "xyz"];

macro_rules! sdf_node {
    ($($f:expr),*) => {
        &[$($f,)* o("at", Vec3), o("rotate", Vec3), o("scale", Num), o("stretch", Vec3), o("mirror", Choice(MIRROR)), o("repeat", Obj(REPEAT)), o("elongate", Vec3),
          o("twist", Num), o("bend", Num), o("round", Num), o("shell", Num), o("displace", Obj(DISPLACE)), o("color", Color)]
    };
}

/// SDF nodes: primitives (centred on `at`) and groups.
const SDF_NODES: &[(&str, &[Field])] = &[
    ("sphere", sdf_node!(r("radius", Num))),
    ("box", sdf_node!(r("size", Vec3), o("corner", Num), o("roundness", Num))),
    ("cylinder", sdf_node!(r("radius", Num), r("height", Num), o("radius_top", Num), o("corner", Num), o("roundness", Num))),
    ("cone", sdf_node!(r("radius", Num), r("height", Num))),
    ("capsule", sdf_node!(r("radius", Num), r("height", Num))),
    ("torus", sdf_node!(r("radius", Num), r("tube", Num))),
    ("ellipsoid", sdf_node!(r("radii", Vec3))),
    ("round_cone", sdf_node!(r("from", Vec3), r("to", Vec3), r("radius", Num), o("radius_to", Num))),
    ("chain", sdf_node!(r("points", Rows), o("smooth", Num))),
    ("column", sdf_node!(r("from", Vec3), r("to", Vec3), o("side", Vec3), r("sections", Rows), o("square", Num))),
    ("halfspace", sdf_node!(o("normal", Vec3), o("offset", Num))),
    ("union", sdf_node!(r("of", Sdfs), o("smooth", Num))),
    ("difference", sdf_node!(r("of", Sdfs), o("smooth", Num))),
    ("intersection", sdf_node!(r("of", Sdfs), o("smooth", Num))),
];

const WINDOWS: &[Field] = &[
    r("width", Num),
    r("height", Num),
    o("sill", Num),
    o("spacing", Num),
    o("margin", Num),
    o("depth", Num),
    o("arch", Bool),
    o("trim", Num),
    o("glass", Bool),
    o("floors", Vec2),
];
const DOOR: &[Field] = &[r("side", Int), o("at", Num), o("width", Num), o("height", Num), o("arch", Bool)];
const ROOF: &[Field] = &[r("kind", Choice(&["none", "flat", "gable", "hip", "shed", "pyramid"])), o("pitch", Num), o("overhang", Num), o("thickness", Num), o("parapet", Num)];
const ROLES: &[Field] = &[o("walls", Material), o("roof", Material), o("trim", Material), o("door", Material), o("glass", Material)];
const BUILDING: &[Field] = &[
    r("plan", Outline),
    o("floors", Int),
    o("floor_height", Num),
    o("wall", Num),
    o("hollow", Bool),
    o("plinth", Vec2),
    o("band", Vec2),
    o("cornice", Vec2),
    o("windows", Obj(WINDOWS)),
    o("doors", Objs(DOOR)),
    o("roof", Obj(ROOF)),
    o("ruin", Num),
    o("bevel", Num),
    o("materials", Obj(ROLES)),
];

const LAYER: &[Field] = &[o("kind", Choice(&["fbm", "ridged", "billow"])), r("scale", Num), o("octaves", Int), r("amplitude", Num)];
const PAD: &[Field] = &[r("at", Vec2), r("radius", Num), r("height", Num), o("blend", Num)];
const EROSION: &[Field] = &[r("droplets", Int), o("strength", Num), o("steps", Int)];
const THERMAL: &[Field] = &[r("iterations", Int), o("talus", Num)];
const SPLAT: &[Field] = &[
    r("name", Name),
    r("color", Color),
    o("slope", Vec2),
    o("height", Vec2),
    o("soft", Num),
    o("noise", Num),
    o("noise_scale", Num),
    // The terrain renderer's layer material (`"render": "terrain"`).
    o("texture", Name),
    o("uv_scale", Num),
    o("roughness", Num),
    o("specular", Num),
    o("triplanar", Num),
    o("normal_strength", Num),
];
const EDIT: &[Field] = &[r("op", Choice(&["raise", "lower", "flatten", "smooth", "paint"])), r("at", Vec2), r("radius", Num), o("amount", Num), o("height", Num), o("layer", Int)];
const TERRAIN: &[Field] = &[
    r("size", Vec2),
    r("samples", Vec2),
    o("base", Num),
    o("layers", Objs(LAYER)),
    o("warp", Num),
    o("island", Num),
    o("terrace", Vec2),
    o("pads", Objs(PAD)),
    o("erosion", Obj(EROSION)),
    o("thermal", Obj(THERMAL)),
    o("splat", Objs(SPLAT)),
    o("depth", Num),
    o("mesh_step", Int),
    // The terrain renderer (docs/design/TERRAIN.md): drawn as a CDLOD heightfield, tiles, edits.
    o("render", Choice(&["mesh", "terrain"])),
    o("offset", Vec2),
    o("edge_lock", Int),
    o("edits", Objs(EDIT)),
    o("skirt", Num),
    o("grain", Num),
];

const MASK: &[Field] = &[r("scale", Num), r("threshold", Num), o("octaves", Int)];
const SPLAT_RULE: &[Field] = &[r("layer", Int), r("least", Num)];
const CLEAR_OF: &[Field] = &[r("part", Name), r("radius", Num)];
const ITEM: &[Field] = &[o("shape", ShapeOps), o("include", Name), o("params", Ranges), o("material", Material), o("color", Color), o("weight", Num), o("variants", Int)];
const SCATTER: &[Field] = &[
    r("area", Outline),
    r("min_distance", Num),
    r("items", Objs(ITEM)),
    o("count", Int),
    o("attempts", Int),
    o("on", Name),
    o("slope", Vec2),
    o("height", Vec2),
    o("mask", Obj(MASK)),
    o("splat", Obj(SPLAT_RULE)),
    o("avoid", Rows),
    o("clear_of", Obj(CLEAR_OF)),
    o("scale", Vec2),
    o("yaw", Vec2),
    o("tilt", Num),
    o("align", Num),
    o("sink", Num),
];

/// Part generators (a part's `generate`).
pub(super) const GENERATORS: &[(&str, &[Field])] = &[("building", BUILDING), ("scatter", SCATTER), ("terrain", TERRAIN)];

/// Names of the generators (for docs and tools).
pub fn generator_names() -> Vec<&'static str> {
    GENERATORS.iter().map(|(n, _)| *n).collect()
}

/// Names of the SDF nodes.
pub fn sdf_names() -> Vec<&'static str> {
    SDF_NODES.iter().map(|(n, _)| *n).collect()
}

/// The includes a generator may build (a scatter's include items).
pub(super) fn generator_includes(n: &Node) -> Option<Vec<String>> {
    let Some(Val::List(items)) = n.get("items") else { return None };
    Some(
        items
            .iter()
            .filter_map(|i| match i {
                Val::Obj(item) => match item.get("include") {
                    Some(Val::Str(s)) => Some(s.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect(),
    )
}

impl Parser<'_> {
    /// One shape node with optional `ops`: the ops are kept in the node as an `ops` list.
    fn shape_with_ops(&self, path: &str, v: &Json) -> Result<Node, RecipeError> {
        let Json::Obj(pairs) = v else { return Err(type_err(path, "a shape object with an \"op\"")) };
        let ops_json = pairs.iter().find(|(k, _)| k == "ops").map(|(_, v)| v);
        let rest = Json::Obj(pairs.iter().filter(|(k, _)| k != "ops").cloned().collect());
        let mut node = self.op_node(path, &rest, SHAPES, "shape")?;
        let ops = self.ops(&format!("{path}.ops"), ops_json)?;
        node.fields.push(("ops".into(), Val::List(ops.into_iter().map(Val::Obj).collect())));
        Ok(node)
    }

    pub(super) fn procgen_value(&self, path: &str, kind: Kind, v: &Json) -> Result<Val, RecipeError> {
        let last = path.rsplit('.').next().unwrap_or(path);
        Ok(match kind {
            Operands => {
                let items = self.list(path, v, "a list of operand shapes")?;
                if items.is_empty() || items.len() > MAX_OPERANDS {
                    return Err(e(path, ErrorCode::Limit, format!("a boolean needs 1..{MAX_OPERANDS} operands (got {})", items.len())).with_numbers(items.len() as f64, MAX_OPERANDS as f64));
                }
                Val::List(items.iter().enumerate().map(|(i, x)| self.shape_with_ops(&format!("{path}[{i}]"), x).map(Val::Obj)).collect::<Result<_, _>>()?)
            }
            ShapeOps => Val::Obj(self.shape_with_ops(path, v)?),
            Sdf => Val::Obj(self.op_node(path, v, SDF_NODES, "sdf node")?),
            Sdfs => {
                let items = self.list(path, v, "a list of sdf nodes")?;
                if items.is_empty() || items.len() > sdf::MAX_NODES {
                    return Err(e(path, ErrorCode::Limit, format!("a group needs 1..{} nodes", sdf::MAX_NODES)));
                }
                Val::List(items.iter().enumerate().map(|(i, x)| self.op_node(&format!("{path}[{i}]"), x, SDF_NODES, "sdf node").map(Val::Obj)).collect::<Result<_, _>>()?)
            }
            Obj(schema) => Val::Obj(self.node(path, v, last, schema, false)?),
            Objs(schema) => {
                let items = self.list(path, v, "a list of objects")?;
                // A terrain's strokes have a cap of their own (`terrain::MAX_EDITS`, which
                // `terrain_edit` documents): this list's general cap stopped a terrain at 64
                // strokes, an eighth of it. Found by the editor's sculpt brush, five drags in.
                let most = if last == "edits" { terrain::MAX_EDITS } else { 64 };
                if items.len() > most {
                    return Err(e(path, ErrorCode::Limit, format!("{} entries; at most {most}", items.len())).with_numbers(items.len() as f64, most as f64));
                }
                Val::List(items.iter().enumerate().map(|(i, x)| self.node(&format!("{path}[{i}]"), x, last, schema, false).map(Val::Obj)).collect::<Result<_, _>>()?)
            }
            Material => {
                let m = v.as_str().ok_or_else(|| type_err(path, "a material preset name"))?;
                if !self.style.materials.contains_key(m) {
                    let names: Vec<&str> = self.style.materials.keys().map(|k| k.as_str()).collect();
                    return Err(e(path, ErrorCode::UnknownMaterial, format!("'{m}' is not a preset ({})", names.join(", "))));
                }
                Val::Str(m.to_string())
            }
            Choice(opts) => match v.as_str() {
                Some(s) if opts.contains(&s) => Val::Str(s.to_string()),
                _ => return Err(type_err(path, &format!("one of {}", opts.join(", ")))),
            },
            Name => match v.as_str() {
                Some(s) if is_ident(s) => Val::Str(s.to_string()),
                _ => return Err(type_err(path, "a name (letters, digits, _)")),
            },
            Ranges => {
                let o = v.as_object().ok_or_else(|| type_err(path, "{name: value or [min, max]}"))?;
                let mut node = Node { path: path.to_string(), op: "params".into(), when: None, fields: Vec::new() };
                for (k, x) in o {
                    let fp = format!("{path}.{k}");
                    let val = match x {
                        Json::Arr(_) => self.value(&fp, Vec2, x)?,
                        _ => Val::Num(self.expr(&fp, x)?),
                    };
                    node.fields.push((k.clone(), val));
                }
                Val::Obj(node)
            }
            _ => unreachable!("tier 1 kinds are parsed by value()"),
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Building helpers

fn csg_err(path: &str, er: csg::CsgError) -> RecipeError {
    let code = match er.kind {
        CsgErrorKind::Geometry => ErrorCode::Geometry,
        CsgErrorKind::Limit => ErrorCode::Limit,
        CsgErrorKind::InvalidMesh => ErrorCode::InvalidMesh,
    };
    let mut out = e(path, code, er.message);
    out.measured = er.measured;
    out.limit = er.limit;
    out
}

impl Ctx<'_> {
    fn sub<'n>(&self, n: &'n Node, name: &str) -> Option<&'n Node> {
        match n.get(name) {
            Some(Val::Obj(x)) => Some(x),
            _ => None,
        }
    }

    fn subs<'n>(&self, n: &'n Node, name: &str) -> Vec<&'n Node> {
        match n.get(name) {
            Some(Val::List(items)) => items.iter().filter_map(|v| if let Val::Obj(x) = v { Some(x) } else { None }).collect(),
            _ => Vec::new(),
        }
    }

    fn enabled(&self, n: &Node) -> Result<bool, RecipeError> {
        match &n.when {
            Some(w) => Ok(self.eval(w, &format!("{}.when", n.path))? != 0.0),
            None => Ok(true),
        }
    }

    /// What is left of the build's procedural work, capped at `cap`; a Limit error when none is.
    fn work_left(&self, path: &str, cap: u64) -> Result<u64, RecipeError> {
        let used = self.tally.work.get();
        let left = MAX_BUILD_WORK.saturating_sub(used);
        if left == 0 {
            return Err(e(path, ErrorCode::Limit, format!("the model's booleans, sculpts and generators used all {MAX_BUILD_WORK} work units (about 1.5 s); simplify them or split the model")).with_numbers(used as f64, MAX_BUILD_WORK as f64));
        }
        Ok(left.min(cap))
    }

    fn spend(&self, work: u64) {
        self.tally.work.set(self.tally.work.get().saturating_add(work));
    }

    pub(super) fn stat(&self, path: &str, j: Json) {
        let mut s = self.tally.stats.borrow_mut();
        if s.len() < MAX_STATS {
            s.push((path.to_string(), j));
        }
    }

    /// A shape node with its own `ops` applied.
    fn shape_and_ops(&self, n: &Node, seed: u64) -> Result<Shape, RecipeError> {
        let mut s = self.shape(n)?;
        if !matches!(n.op.as_str(), "union" | "difference" | "intersection" | "sdf") {
            s.set_color(self.paint.get());
        }
        if let Some(Val::List(ops)) = n.get("ops") {
            for (i, op) in ops.iter().enumerate() {
                if let Val::Obj(op) = op {
                    self.apply(&mut s, op, stream_seed(seed, &format!("op{i}")))?;
                }
            }
        }
        Ok(s)
    }

    fn csg_kind(op: &str) -> Option<CsgOp> {
        match op {
            "union" => Some(CsgOp::Union),
            "difference" => Some(CsgOp::Difference),
            "intersection" => Some(CsgOp::Intersection),
            _ => None,
        }
    }

    /// The tree of a boolean node; nested booleans without ops or bevel stay in the same exact
    /// evaluation. Returns the tree and the triangles its leaves hold.
    fn csg_tree(&self, n: &Node, depth: usize) -> Result<(CsgTree, usize), RecipeError> {
        if depth > csg::MAX_DEPTH {
            return Err(e(&n.path, ErrorCode::Limit, format!("booleans nest deeper than {}", csg::MAX_DEPTH)));
        }
        let op = Self::csg_kind(&n.op).expect("a boolean");
        let Some(Val::List(items)) = n.get("of") else { return Err(e(&n.at("of"), ErrorCode::MissingField, "a boolean needs \"of\"")) };
        let mut kids = Vec::new();
        let mut tris = 0;
        for (i, item) in items.iter().enumerate() {
            let Val::Obj(k) = item else { continue };
            if !self.enabled(k)? {
                if i == 0 && op == CsgOp::Difference {
                    return Err(e(&format!("{}.when", k.path), ErrorCode::Geometry, "the first operand of a difference (what is cut) can't be switched off"));
                }
                continue;
            }
            let plain = matches!(k.get("ops"), Some(Val::List(l)) if l.is_empty()) && k.get("bevel").is_none();
            // (What a difference takes away is not what the thing is made of: nothing built
            // for its second operand and after is recorded as that.)
            let cutter = op == CsgOp::Difference && i > 0;
            if cutter {
                self.tally.cutting.set(self.tally.cutting.get() + 1);
            }
            let built = if Self::csg_kind(&k.op).is_some() && plain {
                self.csg_tree(k, depth + 1)
            } else {
                self.shape_and_ops(k, stream_seed(self.seed, &k.path)).map(|s| {
                    let n2 = s.triangle_count();
                    (CsgTree::Leaf(s), n2)
                })
            };
            if cutter {
                self.tally.cutting.set(self.tally.cutting.get() - 1);
            }
            let (t, n2) = built?;
            kids.push(t);
            tris += n2;
        }
        if kids.is_empty() {
            return Err(e(&n.at("of"), ErrorCode::Geometry, "every operand is switched off"));
        }
        Ok((CsgTree::Op(op, kids), tris))
    }

    pub(super) fn csg_shape(&self, n: &Node) -> Result<Shape, RecipeError> {
        let (tree, leaf_tris) = self.csg_tree(n, 0)?;
        let width = self.ranged(n, "bevel", 0.0, 0.0, 100.0)?;
        let segments = self.int(n, "bevel_segments", 1, 1, csg::MAX_ROUND_SEGMENTS as i64)?;
        let bevel = (width > 0.0).then_some(Bevel { width, segments });
        let mut budget = csg::Budget { max_work: self.work_left(&n.path, csg::MAX_WORK)?, ..csg::Budget::default() };
        let res = csg::evaluate_bevelled(&tree, bevel.as_ref(), &mut budget);
        self.spend(budget.work);
        let (shape, st, bs) = res.map_err(|er| csg_err(&n.path, er))?;
        // The leaves were counted as they were built: the result replaces them.
        self.tally.grow(&n.path, leaf_tris, shape.triangle_count())?;
        let mut j = vec![
            ("op", Json::from(n.op.as_str())),
            ("input_triangles", Json::from(st.input_triangles)),
            ("triangles", Json::from(shape.triangle_count())),
            ("work", Json::from(st.work)),
            ("grid_step_mm", Json::Num(dmath::round_f64(st.step * 1e6) / 1e3)),
        ];
        // Detail the grid couldn't hold, and slivers dropped from the result.
        if st.dropped_triangles > 0 {
            j.push(("dropped_triangles", Json::from(st.dropped_triangles)));
        }
        if st.slivers > 0 {
            j.push(("slivers_dropped", Json::from(st.slivers)));
        }
        if bevel.is_some() {
            j.push(("bevelled_runs", Json::from(bs.convex + bs.concave)));
            if bs.skipped > 0 {
                j.push(("bevel_skipped", Json::from(bs.skipped)));
            }
            if let Some(f) = bs.failed {
                j.push(("bevel_failed", Json::from(f)));
            }
        }
        self.stat(&n.path, Json::obj(j));
        check_extent(&shape, &n.path)?;
        Ok(shape)
    }

    /// An SDF node, numbers evaluated.
    fn sdf_node(&self, n: &Node, depth: usize) -> Result<sdf::SdfNode, RecipeError> {
        if depth > 16 {
            return Err(e(&n.path, ErrorCode::Limit, "sdf nodes nest deeper than 16"));
        }
        let pos = |name: &str, d: f64| self.ranged(n, name, d, 1e-4, crate::recipe::MAX_EXTENT_M as f64);
        let size3 = |name: &str| -> Result<V3, RecipeError> {
            let v = self.vec3(n, name, [1.0; 3])?;
            if !v.iter().all(|x| *x > 0.0 && *x <= MAX_EXTENT_M) {
                return Err(e(&n.at(name), ErrorCode::OutOfRange, format!("sizes must be in (0, {MAX_EXTENT_M}] m")));
            }
            Ok(v)
        };
        let kind = match n.op.as_str() {
            "sphere" => sdf::Kind::Prim(sdf::Prim::Sphere { radius: pos("radius", 1.0)? }),
            "box" => {
                let s = size3("size")?;
                let half = s.map(|v| v * 0.5);
                let corner = self.ranged(n, "corner", 0.0, 0.0, half[0].min(half[1]).min(half[2]) as f64)?;
                match self.roundness(n, corner)? {
                    Some(power) => sdf::Kind::Prim(sdf::Prim::Rounded { half, power, disc: false }),
                    None => sdf::Kind::Prim(sdf::Prim::Box { half, round: corner }),
                }
            }
            "cylinder" | "cone" => {
                let radius = pos("radius", 0.5)?;
                let half_height = pos("height", 1.0)? * 0.5;
                let radius_top = if n.op == "cone" { 0.0 } else { self.ranged(n, "radius_top", radius as f64, 0.0, 1e4)? };
                let round = if n.op == "cone" { 0.0 } else { self.ranged(n, "corner", 0.0, 0.0, radius.min(half_height) as f64)? };
                match if n.op == "cone" { None } else { self.roundness(n, round)? } {
                    Some(_) if radius_top != radius => return Err(e(&n.at("roundness"), ErrorCode::Geometry, "a cylinder rounded by \"roundness\" has one radius: leave \"radius_top\" out")),
                    Some(power) => sdf::Kind::Prim(sdf::Prim::Rounded { half: [radius, half_height, radius], power, disc: true }),
                    None => sdf::Kind::Prim(sdf::Prim::Cylinder { radius, radius_top, half_height, round }),
                }
            }
            "capsule" => {
                let radius = pos("radius", 0.3)?;
                let h = pos("height", 1.0)?;
                sdf::Kind::Prim(sdf::Prim::Capsule { radius, half_height: (h * 0.5 - radius).max(0.0) })
            }
            "torus" => {
                let radius = pos("radius", 1.0)?;
                let tube = pos("tube", 0.2)?;
                sdf::Kind::Prim(sdf::Prim::Torus { radius, tube })
            }
            "ellipsoid" => sdf::Kind::Prim(sdf::Prim::Ellipsoid { radii: size3("radii")? }),
            "round_cone" => {
                let (a, b) = (self.vec3(n, "from", [0.0; 3])?, self.vec3(n, "to", [0.0, 1.0, 0.0])?);
                for (name, p) in [("from", a), ("to", b)] {
                    if p.iter().any(|v| !fits(*v, MAX_EXTENT_M)) {
                        return Err(e(&n.at(name), ErrorCode::OutOfRange, format!("positions must be within ±{MAX_EXTENT_M} m")));
                    }
                }
                let ra = pos("radius", 0.1)?;
                sdf::Kind::Prim(sdf::Prim::RoundCone { a, b, ra, rb: pos("radius_to", ra as f64)? })
            }
            // Sections along a line, each [how far along, half breadth, half depth], eased
            // from one to the next: a trunk, a limb with its swell, a neck.
            "column" => {
                let (a, b) = (self.vec3(n, "from", [0.0; 3])?, self.vec3(n, "to", [0.0, 1.0, 0.0])?);
                for (name, p) in [("from", a), ("to", b)] {
                    if p.iter().any(|v| !fits(*v, MAX_EXTENT_M)) {
                        return Err(e(&n.at(name), ErrorCode::OutOfRange, format!("positions must be within ±{MAX_EXTENT_M} m")));
                    }
                }
                if math3::length(math3::sub(b, a)) < 1e-4 {
                    return Err(e(&n.at("to"), ErrorCode::Geometry, "a column's two ends are in one place"));
                }
                let Some(given) = n.get("sections") else { return Err(e(&n.at("sections"), ErrorCode::MissingField, "a column needs \"sections\": rows of [how far along (0 to 1), half breadth, half depth]")) };
                let rows = self.rows(given, &n.at("sections"), (3, 3), sdf::MAX_SECTIONS)?;
                if rows.is_empty() {
                    return Err(e(&n.at("sections"), ErrorCode::Geometry, "a column needs one section at the least"));
                }
                let mut sections = [[0.0f32; 3]; sdf::MAX_SECTIONS];
                for (i, row) in rows.iter().enumerate() {
                    let rising = i == 0 || row[0] > rows[i - 1][0];
                    if !(row[0] >= 0.0 && row[0] <= 1.0 && rising && row[1] >= 1e-4 && row[2] >= 1e-4) {
                        return Err(e(&format!("{}[{i}]", n.at("sections")), ErrorCode::OutOfRange, "a section is [how far along, half breadth, half depth]: along from 0 to 1 and further than the section before, the two halves from 0.0001 m"));
                    }
                    sections[i] = [row[0], row[1], row[2]];
                }
                // How square its sections are: 2 an ellipse, up to 8 (nearly a box).
                let square = self.ranged(n, "square", 2.0, 2.0, 8.0)?;
                sdf::Kind::Prim(sdf::Prim::Column { a, b, side: self.vec3(n, "side", [1.0, 0.0, 0.0])?, sections, count: rows.len() as u8, square })
            }
            // A line of points, each with a radius: a round cone between every two neighbours,
            // joined (smoothly, by `smooth`). A limb, a tail, a branch or a horn in a few rows.
            "chain" => {
                let Some(points) = n.get("points") else { return Err(e(&n.at("points"), ErrorCode::MissingField, "a chain needs \"points\": rows of [x, y, z, radius]")) };
                let rows = self.rows(points, &n.at("points"), (4, 4), sdf::MAX_CHAIN)?;
                if rows.len() < 2 {
                    return Err(e(&n.at("points"), ErrorCode::Geometry, "a chain needs two points at the least, each [x, y, z, radius]"));
                }
                for (i, row) in rows.iter().enumerate() {
                    if row[..3].iter().any(|v| !fits(*v, MAX_EXTENT_M)) || !(row[3] >= 1e-4 && row[3] <= MAX_EXTENT_M) {
                        return Err(e(&format!("{}[{i}]", n.at("points")), ErrorCode::OutOfRange, format!("a point is [x, y, z, radius]: positions within ±{MAX_EXTENT_M} m, the radius from 0.0001 m")));
                    }
                }
                let kids = rows
                    .windows(2)
                    .map(|w| sdf::SdfNode { kind: sdf::Kind::Prim(sdf::Prim::RoundCone { a: [w[0][0], w[0][1], w[0][2]], b: [w[1][0], w[1][1], w[1][2]], ra: w[0][3], rb: w[1][3] }), m: sdf::Modifiers::default() })
                    .collect();
                sdf::Kind::Group { op: sdf::Combine::Near, kids, smooth: self.ranged(n, "smooth", 0.0, 0.0, 100.0)? }
            }
            "halfspace" => {
                let nrm = math3::normalize(self.vec3(n, "normal", [0.0, 1.0, 0.0])?);
                if math3::length(nrm) < 0.5 {
                    return Err(e(&n.at("normal"), ErrorCode::OutOfRange, "the normal must not be zero"));
                }
                sdf::Kind::Prim(sdf::Prim::HalfSpace { normal: nrm, offset: self.ranged(n, "offset", 0.0, -1e4, 1e4)? })
            }
            g @ ("union" | "difference" | "intersection") => {
                let op = match g {
                    "union" => sdf::Combine::Union,
                    "difference" => sdf::Combine::Difference,
                    _ => sdf::Combine::Intersection,
                };
                let mut kids = Vec::new();
                for (i, k) in self.subs(n, "of").into_iter().enumerate() {
                    if !self.enabled(k)? {
                        if i == 0 && op == sdf::Combine::Difference {
                            return Err(e(&format!("{}.when", k.path), ErrorCode::Geometry, "the first child of a difference can't be switched off"));
                        }
                        continue;
                    }
                    kids.push(self.sdf_node(k, depth + 1)?);
                }
                if kids.is_empty() {
                    return Err(e(&n.at("of"), ErrorCode::Geometry, "every child is switched off"));
                }
                sdf::Kind::Group { op, kids, smooth: self.ranged(n, "smooth", 0.0, 0.0, 100.0)? }
            }
            other => return Err(e(&n.path, ErrorCode::UnknownOp, format!("unknown sdf node '{other}'"))),
        };
        let mut m = sdf::Modifiers::default();
        let mirror = self.str(n, "mirror", "");
        m.mirror = [mirror.contains('x'), mirror.contains('y'), mirror.contains('z')];
        m.at = self.vec3(n, "at", [0.0; 3])?;
        if m.at.iter().any(|v| !fits(*v, MAX_EXTENT_M)) {
            return Err(e(&n.at("at"), ErrorCode::OutOfRange, format!("positions must be within ±{MAX_EXTENT_M} m")));
        }
        m.rotate = self.vec3(n, "rotate", [0.0; 3])?;
        if m.rotate.iter().any(|v| !fits(*v, 36_000.0)) {
            return Err(e(&n.at("rotate"), ErrorCode::OutOfRange, "rotations must be within ±36000 degrees"));
        }
        m.scale = self.ranged(n, "scale", 1.0, 1e-3, 1e3)?;
        m.stretch = self.vec3(n, "stretch", [1.0; 3])?;
        if m.stretch.iter().any(|v| !(*v >= 0.05 && *v <= 20.0)) {
            return Err(e(&n.at("stretch"), ErrorCode::OutOfRange, "stretch is a scale for each axis, each from 0.05 to 20"));
        }
        if let Some(rp) = self.sub(n, "repeat") {
            let c = self.vec3(rp, "count", [1.0; 3])?;
            let sp = self.vec3(rp, "spacing", [1.0; 3])?;
            if c.iter().any(|v| !(*v >= 1.0 && *v <= sdf::MAX_REPEAT as f32 && dmath::fract(*v) == 0.0)) || c[0] * c[1] * c[2] > sdf::MAX_REPEAT as f32 {
                return Err(e(&rp.at("count"), ErrorCode::Limit, format!("repeat counts are whole numbers >= 1 with at most {} copies in all", sdf::MAX_REPEAT)));
            }
            if sp.iter().any(|v| !(*v >= 0.0 && *v <= MAX_EXTENT_M)) {
                return Err(e(&rp.at("spacing"), ErrorCode::OutOfRange, "spacing must be 0..10000 m"));
            }
            m.repeat = Some((c.map(|v| v as u32), sp));
        }
        m.elongate = self.vec3(n, "elongate", [0.0; 3])?;
        if m.elongate.iter().any(|v| !(*v >= 0.0 && *v <= MAX_EXTENT_M)) {
            return Err(e(&n.at("elongate"), ErrorCode::OutOfRange, "elongate half lengths must be 0..10000 m"));
        }
        m.twist = self.ranged(n, "twist", 0.0, -3600.0, 3600.0)?;
        m.bend = self.ranged(n, "bend", 0.0, -720.0, 720.0)?;
        m.round = self.ranged(n, "round", 0.0, 0.0, 100.0)?;
        m.shell = self.ranged(n, "shell", 0.0, 0.0, 100.0)?;
        if let Some(d) = self.sub(n, "displace") {
            let amount = self.ranged(d, "amount", 0.0, -100.0, 100.0)?;
            if !self.coarse {
                m.displace = Some((amount, self.positive(d, "scale", 0.5)?, self.int(d, "octaves", 3, 1, 6)?));
            }
        }
        if n.get("color").is_some() {
            m.color = Some(self.color(n, "color"));
        }
        Ok(sdf::SdfNode { kind, m })
    }

    /// The body a `figure` asks for, and how it is shaped. What a body refuses (a stature
    /// nobody measured had, traits that fold it) is the recipe's error, in the body's words.
    fn figure_tree(&self, n: &Node) -> Result<sdf::SdfNode, RecipeError> {
        let (body, shaping) = self.figure_body(n)?;
        let part = crate::figure::Part::parse(self.str(n, "part", "whole")).unwrap_or(crate::figure::Part::Whole);
        Ok(crate::figure::tree(&body, &shaping, part))
    }

    /// A box's or a cylinder's roundness as one number (`roundness`, 0 to 1; not the modifier
    /// `round`, which swells any node by a radius in metres): `None` for the hard
    /// shape (0, or not given), else the power of its rounded form, 2 over the roundness (1 is
    /// the ellipsoid of its sizes, 0.5 a power of 4; under 0.05 is taken as 0.05, a power of
    /// 40: nearly the hard shape). `corner` (a radius in metres) is the other way to round an
    /// edge, and the two are not given together.
    fn roundness(&self, n: &Node, corner: f32) -> Result<Option<f32>, RecipeError> {
        let round = self.ranged(n, "roundness", 0.0, 0.0, 1.0)?;
        if round <= 0.0 {
            return Ok(None);
        }
        if corner > 0.0 {
            return Err(e(&n.at("roundness"), ErrorCode::Geometry, "\"roundness\" (0 to 1) and \"corner\" (metres) are two ways to round a shape: give one"));
        }
        Ok(Some(2.0 / round.max(0.05)))
    }

    /// The body and the shaping a `figure`'s fields ask for (a shape's, or a rig's).
    pub(super) fn figure_body(&self, n: &Node) -> Result<(specula_corpus::Body, crate::figure::Shaping), RecipeError> {
        use specula_corpus::{Body, Sex};
        // A refusal names the field it is about, with what was given and the limit it broke.
        let refused = |er: specula_corpus::BodyError| {
            use specula_corpus::BodyError as B;
            let (field, numbers) = match &er {
                B::Stature { given, least, most } => ("stature", Some((*given, if given < least { *least } else { *most }))),
                // (A build is in standard errors and the refusal's numbers are kilograms: they go
                // with a weight only.)
                B::Weight { given, least, most } if n.get("weight").is_some() => ("weight", Some((*given, if given < least { *least } else { *most }))),
                B::Weight { .. } => ("build", None),
                B::Age { given, least, most } => ("age", Some((*given, if given < least { *least } else { *most }))),
                B::Trait { given, least, most, .. } => ("traits", Some((*given, if given < least { *least } else { *most }))),
                B::UnknownTrait { .. } | B::Folded { .. } | B::NotPositive { .. } => ("traits", None),
                B::NotANumber { .. } => ("", None),
            };
            let at = if field.is_empty() { n.path.clone() } else { n.at(field) };
            let refusal = e(&at, ErrorCode::OutOfRange, format!("{}: {er}", er.code()));
            match numbers {
                Some((given, limit)) => refusal.with_numbers(given, limit),
                None => refusal,
            }
        };
        let sex = Sex::parse(self.str(n, "sex", "female")).unwrap_or(Sex::Female);
        let typical = Body::typical(sex);
        // (A recipe's numbers are single precision: the tallest stature the rows cover, written
        // as a recipe writes it, comes back a hair over itself and was refused as outside them.
        // A stature within a hundredth of a millimetre of a limit is that limit.)
        let [shortest, tallest] = specula_corpus::ansur::statures(sex).map(|mm| mm / 1000.0);
        let stature = self.ranged(n, "stature", typical.stature(), 0.3, 3.0)? as f64;
        let stature = [shortest, tallest].into_iter().find(|limit| (stature - limit).abs() < 1e-5).unwrap_or(stature);
        let mut body = if n.get("weight").is_some() {
            if n.get("build").is_some() {
                return Err(e(&n.at("build"), ErrorCode::Geometry, "a figure has a build or a weight, not both"));
            }
            // (As for the stature: a weight within a tenth of a gram of a limit is that limit.)
            let weight = self.ranged(n, "weight", typical.weight(), 1.0, 1000.0)? as f64;
            let weight = Body::weights(sex, stature).into_iter().find(|limit| (weight - limit).abs() < 1e-4).unwrap_or(weight);
            Body::new(sex, stature, weight)
        } else {
            Body::with_build(sex, stature, self.ranged(n, "build", 0.0, -20.0, 20.0)? as f64)
        }
        .map_err(refused)?;
        if n.get("age").is_some() {
            body = body.with_age(self.ranged(n, "age", 30.0, 0.0, 200.0)? as f64).map_err(refused)?;
        }
        if let Some(Val::Obj(traits)) = n.get("traits") {
            let mut set = Vec::new();
            for (name, _) in &traits.fields {
                set.push((name.as_str(), self.ranged(traits, name, 0.0, -100.0, 100.0)? as f64));
            }
            body = body.with_traits(&set).map_err(|er| {
                use specula_corpus::BodyError as B;
                let refusal = |at: &str| e(at, ErrorCode::OutOfRange, format!("{}: {er}", er.code()));
                match &er {
                    B::Trait { name, given, least, most } => refusal(&traits.at(name)).with_numbers(*given, if given < least { *least } else { *most }),
                    B::UnknownTrait { name } => refusal(&traits.at(name)),
                    _ => refusal(&traits.path),
                }
            })?;
        }
        let standard = crate::figure::Shaping::default();
        let shaping = crate::figure::Shaping {
            // (Past 60 degrees a hand's cut is no longer its room's top: not built, and refused.)
            arms: self.ranged(n, "arms", standard.arms, 0.0, 60.0)? as f64,
            legs: self.ranged(n, "legs", standard.legs, 0.0, 45.0)? as f64,
            smooth: self.ranged(n, "smooth", standard.smooth as f64, 0.0, 0.2)?,
        };
        Ok((body, shaping))
    }

    pub(super) fn sdf_shape(&self, n: &Node) -> Result<Shape, RecipeError> {
        let figure = n.op == "figure";
        let tree = if figure {
            self.figure_tree(n)?
        } else {
            let Some(Val::Obj(root)) = n.get("tree") else { return Err(e(&n.at("tree"), ErrorCode::MissingField, "an sdf needs a \"tree\"")) };
            self.sdf_node(root, 0)?
        };
        // A figure's part is meshed as finely as it needs: a finger is a third of a trunk's cell.
        let usual = match crate::figure::Part::parse(self.str(n, "part", if figure { "whole" } else { "" })) {
            Some(part) if figure => part.resolution() as i64,
            _ => 48,
        };
        let mut res = self.int(n, "resolution", usual, 4, sdf::MAX_RESOLUTION as i64)?;
        if self.coarse {
            res = (res / 2).max(4);
        }
        let bounds = match n.get("bounds") {
            Some(v) => {
                let rows = self.rows(v, &n.at("bounds"), (3, 3), 2)?;
                if rows.len() != 2 || (0..3).any(|k| rows[1][k] <= rows[0][k]) {
                    return Err(e(&n.at("bounds"), ErrorCode::OutOfRange, "bounds are [[min x, y, z], [max x, y, z]] with max above min"));
                }
                Some(([rows[0][0], rows[0][1], rows[0][2]], [rows[1][0], rows[1][1], rows[1][2]]))
            }
            None => None,
        };
        let room = MAX_MODEL_TRIANGLES.saturating_sub(self.tally.triangles.get()).min(MAX_PART_TRIANGLES);
        let opts = sdf::MeshOptions { resolution: res, bounds, seed: stream_seed(self.seed, &n.path), max_triangles: room, max_work: self.work_left(&n.path, sdf::MAX_WORK)?, color: self.paint.get() };
        let (shape, st) = sdf::mesh(&tree, &opts).map_err(|er| {
            let mut out = e(&n.path, if er.limit { ErrorCode::Limit } else { ErrorCode::Geometry }, er.message);
            out.measured = er.measured;
            out.limit = er.max;
            out
        })?;
        self.spend(st.work);
        self.tally.grow(&n.path, 0, shape.triangle_count())?;
        // What it is built of: its primitives' own outlines (a figure's are a body's limbs and
        // trunk, read the same way).
        let mut views = Vec::new();
        crate::made::of_sculpt(&tree, &mut views);
        self.made(if figure { "figure" } else { "sdf" }, views);
        self.tally.made_now(&n.path, crate::made::area(&shape));
        // A sculpt is round where its tree is: its surface is cut into cells, and round its
        // tightest ball, limb or section a cell turns the surface by the cell over the radius.
        if let Some(radius) = sdf::least_radius(&tree) {
            let turn = 2.0 * dmath::asin((st.cell / (2.0 * radius)).min(1.0)).to_degrees();
            self.round(n, "sdf", turn, radius);
        }
        self.stat(
            &n.path,
            Json::obj([
                ("op", Json::from(n.op.as_str())),
                ("resolution", Json::from(res)),
                ("grid", Json::arr(st.dims.map(Json::from))),
                ("cell_m", Json::Num(dmath::round_f64(st.cell as f64 * 1e4) / 1e4)),
                ("samples_evaluated", Json::from(st.samples_evaluated)),
                ("blocks_skipped", Json::from(st.blocks_skipped)),
                ("work", Json::from(st.work)),
                ("triangles", Json::from(shape.triangle_count())),
            ]),
        );
        check_extent(&shape, &n.path)?;
        Ok(shape)
    }
}

// ---------------------------------------------------------------------------------------------
// Generators

/// A generator part being built.
pub(super) struct Gen<'c, 'p> {
    pub ctx: &'c Ctx<'c>,
    pub part: &'c PartDef,
    pub params: &'c BuildParams<'p>,
    pub stack: &'c mut Vec<String>,
    pub tally: &'c Tally,
    pub coarse: bool,
    pub seed: u64,
    pub smooth: f32,
    /// Terrains built so far: part name, terrain, and whether the part's ops reshape it.
    pub terrains: &'c mut Vec<(String, Arc<Terrain>, bool)>,
    /// Scatters placed so far: part name and each placement's x, z (for a later `clear_of`).
    pub scattered: &'c mut Vec<(String, Vec<V2>)>,
    /// The recipe's name and texture graphs (terrain layers name them).
    pub recipe_name: &'c str,
    pub textures: &'c [(String, TexSource)],
}

impl Gen<'_, '_> {
    fn material_color(&self, material: &str) -> Rgba {
        self.params.style.materials.get(material).map_or([1.0; 4], |m| m.color)
    }

    fn piece(&self, mut shape: Shape, material: &str, colour: Option<Rgba>) -> BuiltPart {
        if let Some(c) = colour {
            shape.set_color(c);
        }
        BuiltPart { shape, material: material.to_string(), smooth_angle: self.smooth, lod_detail: 1.0, bones: Vec::new(), blend: None, uv_scale: None }
    }

    pub(super) fn run(&mut self, n: &Node) -> Result<Vec<BuiltPart>, RecipeError> {
        match n.op.as_str() {
            "building" => self.building(n),
            "terrain" => self.terrain(n),
            "scatter" => self.scatter(n),
            other => Err(e(&n.path, ErrorCode::UnknownOp, format!("unknown generator '{other}'"))),
        }
    }

    fn building(&mut self, n: &Node) -> Result<Vec<BuiltPart>, RecipeError> {
        let c = self.ctx;
        let plan = c.outline(n.get("plan").expect("required"), &n.at("plan"), building::MAX_PLAN_POINTS, 1)?;
        let opt_pair = |name: &str| -> Result<Option<(f32, f32)>, RecipeError> {
            match n.get(name) {
                None => Ok(None),
                Some(_) => {
                    let v = c.vec2(n, name, [0.0, 0.0])?;
                    if !(v[0] > 0.0 && v[0] <= 10.0 && v[1].abs() <= 5.0) {
                        return Err(e(&n.at(name), ErrorCode::OutOfRange, format!("{name} is [height 0..10 m, how far it stands out ±5 m]")));
                    }
                    Ok(Some((v[0], v[1])))
                }
            }
        };
        let floors = c.int(n, "floors", 1, 1, building::MAX_FLOORS as i64)?;
        let floor_height = c.ranged(n, "floor_height", 3.0, 1.0, 20.0)?;
        let windows = match c.sub(n, "windows") {
            Some(w) if c.enabled(w)? => {
                let fl = c.vec2(w, "floors", [0.0, (floors - 1) as f32])?;
                let width = c.ranged(w, "width", 1.0, 0.1, 20.0)?;
                Some(building::Windows {
                    width,
                    height: c.ranged(w, "height", 1.4, 0.1, 20.0)?,
                    sill: c.ranged(w, "sill", 0.9, 0.0, 20.0)?,
                    spacing: c.ranged(w, "spacing", (width * 2.4) as f64, 0.1, 100.0)?,
                    margin: c.ranged(w, "margin", 0.6, 0.0, 50.0)?,
                    depth: c.ranged(w, "depth", 0.25, 0.01, 3.0)?,
                    arch: c.bool(w, "arch", false),
                    trim: c.ranged(w, "trim", 0.0, 0.0, 1.0)?,
                    glass: c.bool(w, "glass", false),
                    floors: (fl[0].max(0.0) as u32, fl[1].max(0.0) as u32),
                })
            }
            _ => None,
        };
        let mut doors = Vec::new();
        for d in c.subs(n, "doors") {
            if !c.enabled(d)? {
                continue;
            }
            doors.push(building::Door {
                side: c.int(d, "side", 0, 0, 63)? as usize,
                at: c.ranged(d, "at", 0.5, 0.0, 1.0)?,
                width: c.ranged(d, "width", 1.1, 0.2, 20.0)?,
                height: c.ranged(d, "height", 2.2, 0.5, 20.0)?,
                arch: c.bool(d, "arch", false),
            });
        }
        let roof = match c.sub(n, "roof") {
            Some(r) => building::Roof {
                kind: match c.str(r, "kind", "flat") {
                    "none" => RoofKind::None,
                    "gable" => RoofKind::Gable,
                    "hip" => RoofKind::Hip,
                    "shed" => RoofKind::Shed,
                    "pyramid" => RoofKind::Pyramid,
                    _ => RoofKind::Flat,
                },
                pitch_deg: c.ranged(r, "pitch", 35.0, 0.0, 75.0)?,
                overhang: c.ranged(r, "overhang", 0.3, 0.0, 3.0)?,
                thickness: c.ranged(r, "thickness", 0.2, 0.0, 5.0)?,
                parapet: c.ranged(r, "parapet", 0.0, 0.0, 5.0)?,
            },
            None => building::Roof { kind: RoofKind::Flat, pitch_deg: 0.0, overhang: 0.0, thickness: 0.2, parapet: 0.0 },
        };
        let spec = BuildingSpec {
            plan,
            floors,
            floor_height,
            wall: c.ranged(n, "wall", 0.4, 0.05, 3.0)?,
            hollow: c.bool(n, "hollow", false),
            plinth: opt_pair("plinth")?,
            band: opt_pair("band")?,
            cornice: opt_pair("cornice")?,
            windows,
            doors,
            roof,
            ruin: c.ranged(n, "ruin", 0.0, 0.0, 1.0)?,
            bevel: c.ranged(n, "bevel", 0.0, 0.0, 1.0)?,
            seed: self.seed,
        };
        let mut budget = csg::Budget { max_work: c.work_left(&n.path, csg::MAX_WORK)?, ..csg::Budget::default() };
        let built = spec.build(&mut budget);
        c.spend(budget.work);
        let (pieces, st) = built.map_err(|er| {
            let mut out = e(&n.at(er.field), if er.limit { ErrorCode::Limit } else { ErrorCode::Geometry }, er.message);
            out.measured = er.measured;
            out.limit = er.max;
            out
        })?;
        let roles = c.sub(n, "materials");
        let base = self.part.material.clone();
        let mat_of = |role: Role| -> Result<String, RecipeError> {
            if let Some(Val::Str(m)) = roles.and_then(|r| r.get(role.name())) {
                return Ok(m.clone());
            }
            base.clone().or_else(|| match roles.and_then(|r| r.get("walls")) {
                Some(Val::Str(m)) => Some(m.clone()),
                _ => None,
            }).ok_or_else(|| e(&n.at("materials"), ErrorCode::MissingField, format!("no material for the building's {} (give the part a material or materials.{})", role.name(), role.name())))
        };
        // One piece per role (the role's shapes merged).
        let mut by_role: std::collections::BTreeMap<Role, Shape> = std::collections::BTreeMap::new();
        for (role, s) in pieces {
            by_role.entry(role).or_default().merge(&s);
        }
        let mut out = Vec::new();
        for (role, s) in by_role {
            self.tally.grow(&n.path, 0, s.triangle_count())?;
            let m = mat_of(role)?;
            let colour = match (role, &self.part.color) {
                (Role::Walls, Some(col)) => self.params.style.color(col),
                _ => Some(self.material_color(&m)),
            };
            out.push(self.piece(s, &m, colour));
        }
        let tris: usize = out.iter().map(|p| p.shape.triangle_count()).sum();
        c.stat(&n.path, Json::obj([("op", Json::from("building")), ("windows", Json::from(st.windows)), ("doors", Json::from(st.doors)), ("work", Json::from(st.csg_work)), ("triangles", Json::from(tris))]));
        Ok(out)
    }

    fn terrain(&mut self, n: &Node) -> Result<Vec<BuiltPart>, RecipeError> {
        let c = self.ctx;
        let size = c.vec2(n, "size", [10.0, 10.0])?;
        let samples = c.vec2(n, "samples", [33.0, 33.0])?;
        if samples.iter().any(|s| !(*s >= 2.0 && *s <= terrain::MAX_SIDE as f32 && dmath::fract(*s) == 0.0)) {
            return Err(e(&n.at("samples"), ErrorCode::Limit, format!("samples per side are whole numbers 2..{}", terrain::MAX_SIDE)).with_numbers(samples[0].max(samples[1]) as f64, terrain::MAX_SIDE as f64));
        }
        let mut layers = Vec::new();
        for l in c.subs(n, "layers") {
            layers.push(terrain::NoiseLayer {
                kind: match c.str(l, "kind", "fbm") {
                    "ridged" => terrain::NoiseKind::Ridged,
                    "billow" => terrain::NoiseKind::Billow,
                    _ => terrain::NoiseKind::Fbm,
                },
                scale: c.ranged(l, "scale", 10.0, 0.01, 1e4)?,
                octaves: c.int(l, "octaves", 4, 1, 10)?,
                amplitude: c.ranged(l, "amplitude", 1.0, -1e4, 1e4)?,
            });
        }
        let mut pads = Vec::new();
        for p in c.subs(n, "pads") {
            pads.push(terrain::Pad { at: c.vec2(p, "at", [0.0, 0.0])?, radius: c.ranged(p, "radius", 1.0, 0.0, 1e4)?, height: c.ranged(p, "height", 0.0, -1e4, 1e4)?, blend: c.ranged(p, "blend", 1.0, 0.0, 1e4)? });
        }
        let hydraulic = match c.sub(n, "erosion") {
            Some(h) => Some(terrain::Hydraulic { droplets: c.int(h, "droplets", 10_000, 0, terrain::MAX_DROPLETS as i64)?, strength: c.ranged(h, "strength", 0.5, 0.0, 1.0)?, steps: c.int(h, "steps", 32, 1, 256)? }),
            _ => None,
        };
        let thermal = match c.sub(n, "thermal") {
            Some(t) => Some(terrain::Thermal { iterations: c.int(t, "iterations", 10, 0, terrain::MAX_THERMAL as i64)?, talus_deg: c.ranged(t, "talus", 35.0, 1.0, 89.0)? }),
            _ => None,
        };
        let mut splat = Vec::new();
        for sp in c.subs(n, "splat") {
            splat.push(terrain::SplatLayer {
                name: c.str(sp, "name", "layer").to_string(),
                color: c.color(sp, "color"),
                slope: c.vec2(sp, "slope", [0.0, 90.0])?,
                height: c.vec2(sp, "height", [-1e4, 1e4])?,
                soft: c.ranged(sp, "soft", 2.0, 0.0, 1e4)?,
                noise: c.ranged(sp, "noise", 0.0, 0.0, 1.0)?,
                noise_scale: c.ranged(sp, "noise_scale", 5.0, 0.01, 1e4)?,
            });
        }
        let terrace = c.vec2(n, "terrace", [0.0, 0.0])?;
        let edit_nodes = c.subs(n, "edits");
        if edit_nodes.len() > terrain::MAX_EDITS {
            return Err(e(&n.at("edits"), ErrorCode::Limit, format!("{} strokes; at most {}", edit_nodes.len(), terrain::MAX_EDITS)).with_numbers(edit_nodes.len() as f64, terrain::MAX_EDITS as f64));
        }
        let mut edits = Vec::with_capacity(edit_nodes.len());
        for ed in &edit_nodes {
            let op = terrain::EditOp::parse(c.str(ed, "op", "raise")).unwrap_or(terrain::EditOp::Raise);
            edits.push(terrain::Edit {
                op,
                at: c.vec2(ed, "at", [0.0, 0.0])?,
                radius: c.ranged(ed, "radius", 1.0, 0.01, 1e4)?,
                amount: c.ranged(ed, "amount", if matches!(op, terrain::EditOp::Raise | terrain::EditOp::Lower) { 0.5 } else { 1.0 }, -1e3, 1e3)?,
                height: c.ranged(ed, "height", 0.0, -1e4, 1e4)?,
                layer: c.int(ed, "layer", 0, 0, terrain::MAX_SPLAT as i64 - 1)?,
            });
        }
        let spec = TerrainSpec {
            size,
            samples: [samples[0] as u32, samples[1] as u32],
            base: c.ranged(n, "base", 0.0, -1e4, 1e4)?,
            seed: self.seed,
            layers,
            warp: c.ranged(n, "warp", 0.0, 0.0, 1e4)?,
            island: c.ranged(n, "island", 0.0, 0.0, 1.0)?,
            terrace: (terrace[0].max(0.0), terrace[1].clamp(0.0, 1.0)),
            pads,
            hydraulic,
            thermal,
            splat,
            offset: c.vec2(n, "offset", [0.0, 0.0])?,
            edge_lock: c.int(n, "edge_lock", 0, 0, terrain::MAX_EDGE_LOCK as i64)?,
            edits,
        };
        let mut t = spec.build(c.work_left(&n.path, terrain::MAX_WORK)?).map_err(|er| {
            let mut out = e(&n.at(er.field), if er.limit { ErrorCode::Limit } else { ErrorCode::OutOfRange }, er.message);
            out.measured = er.measured;
            out.limit = er.max;
            out
        })?;
        c.spend(t.work);
        let render = c.str(n, "render", "mesh") == "terrain";
        if render {
            // Each layer's material; a texture names one of the recipe's graphs (baked with the
            // build under the key `terrain+<graph>`).
            let mut layers = Vec::new();
            for (k, sp) in c.subs(n, "splat").iter().enumerate() {
                let texture = match sp.get("texture") {
                    Some(Val::Str(name)) => {
                        let key = match self.textures.iter().find(|(tn, _)| tn == name).map(|(_, s)| s) {
                            Some(TexSource::Library(lib)) => format!("lib:{lib}"),
                            Some(TexSource::Inline(_)) => format!("{}/{name}", self.recipe_name),
                            None => {
                                let known: Vec<&str> = self.textures.iter().map(|(tn, _)| tn.as_str()).collect();
                                return Err(e(&format!("{}.splat[{k}].texture", n.path), ErrorCode::UnknownName, format!("no texture '{name}' in the recipe's \"textures\" (has: {})", known.join(", "))));
                            }
                        };
                        Some(format!("terrain+{key}"))
                    }
                    _ => None,
                };
                layers.push(terrain::RenderLayer {
                    texture,
                    uv_scale: c.ranged(sp, "uv_scale", 4.0, 0.01, 1000.0)?,
                    roughness: c.ranged(sp, "roughness", 0.9, 0.0, 1.0)?,
                    specular: c.ranged(sp, "specular", 0.05, 0.0, 1.0)?,
                    triplanar_deg: c.ranged(sp, "triplanar", 35.0, 0.0, 90.0)?,
                    normal_strength: c.ranged(sp, "normal_strength", 1.0, 0.0, 4.0)?,
                });
            }
            if layers.is_empty() {
                layers.push(terrain::RenderLayer::default());
            }
            t.render = Some(terrain::TerrainRender { layers, skirt: c.ranged(n, "skirt", 0.5, 0.0, 100.0)?, grain: c.ranged(n, "grain", 0.35, 0.0, 1.0)? });
        }
        // Drawn by the terrain renderer, the mesh is only its stand-in (before the textures
        // land, the asset studio, HLOD): every 4th sample unless the recipe says otherwise.
        let mut step = c.int(n, "mesh_step", if render { 4 } else { 1 }, 1, 64)? as usize;
        if self.coarse {
            step *= 2;
        }
        let depth = c.ranged(n, "depth", 0.0, 0.0, 1e4)?;
        let cols = t.columns.div_ceil(step) + 1;
        let rows = t.rows.div_ceil(step) + 1;
        self.tally.grow(&n.path, 0, 2 * cols * rows + if depth > 0.0 { 10 * (cols + rows) } else { 0 })?;
        let mesh = t.mesh(step, depth);
        self.tally.settle(2 * cols * rows + if depth > 0.0 { 10 * (cols + rows) } else { 0 }, mesh.triangle_count());
        let colour = if t.splat_names.is_empty() { Some(self.part.color.as_deref().and_then(|cn| self.params.style.color(cn)).unwrap_or_else(|| self.material_color(self.part.material.as_deref().unwrap_or("")))) } else { None };
        c.stat(
            &n.path,
            Json::obj([
                ("op", Json::from("terrain")),
                ("samples", Json::arr([Json::from(t.columns), Json::from(t.rows)])),
                ("cell_m", Json::arr(t.cell.map(|v| Json::Num(v as f64)))),
                ("height_range", Json::arr([Json::Num(t.heights.iter().fold(f32::MAX, |m, h| m.min(*h)) as f64), Json::Num(t.heights.iter().fold(f32::MIN, |m, h| m.max(*h)) as f64)])),
                ("splat", Json::arr(t.splat_names.iter().map(|s| Json::from(s.as_str())))),
                ("work", Json::from(t.work)),
                ("triangles", Json::from(mesh.triangle_count())),
            ]),
        );
        let material = self.part.material.clone().unwrap_or_default();
        let piece = self.piece(mesh, &material, colour);
        // Ops that only colour leave the terrain where scatters and colliders sample it.
        let moved = self.part.ops.iter().any(|o| !COLOUR_OPS.contains(&o.op.as_str()));
        self.terrains.push((self.part.name.clone(), Arc::new(t), moved));
        Ok(vec![piece])
    }

    fn scatter(&mut self, n: &Node) -> Result<Vec<BuiltPart>, RecipeError> {
        let c = self.ctx;
        let area = scatter::Area::Polygon(c.outline(n.get("area").expect("required"), &n.at("area"), 1024, 1)?);
        let pair = |name: &str| -> Result<Option<V2>, RecipeError> { if n.get(name).is_some() { Ok(Some(c.vec2(n, name, [0.0, 0.0])?)) } else { Ok(None) } };
        let items = c.subs(n, "items");
        if items.is_empty() || items.len() > MAX_ITEMS {
            return Err(e(&n.at("items"), ErrorCode::Limit, format!("a scatter needs 1..{MAX_ITEMS} items")).with_numbers(items.len() as f64, MAX_ITEMS as f64));
        }
        let mut weights = Vec::new();
        for it in &items {
            if it.get("shape").is_some() == it.get("include").is_some() {
                return Err(e(&it.path, ErrorCode::MissingField, "an item needs exactly one of \"shape\" or \"include\""));
            }
            weights.push(c.ranged(it, "weight", 1.0, 0.0, 1e4)?);
        }
        let mut avoid: Vec<[f32; 3]> = match n.get("avoid") {
            Some(v) => c.rows(v, &n.at("avoid"), (3, 3), scatter::MAX_AVOID)?.into_iter().map(|r| [r[0], r[1], r[2]]).collect(),
            None => Vec::new(),
        };
        // Keep clear of an earlier scatter's placements (rocks between the graves, not on them).
        if let Some(cl) = c.sub(n, "clear_of") {
            let Some(Val::Str(name)) = cl.get("part") else { unreachable!("required") };
            let radius = c.ranged(cl, "radius", 0.5, 0.0, 100.0)?;
            let (_, at) = self.scattered.iter().find(|(p, _)| p == name).ok_or_else(|| {
                e(&cl.at("part"), ErrorCode::UnknownName, format!("no earlier scatter part named '{name}' (scatters: {})", self.scattered.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>().join(", ")))
            })?;
            let total = avoid.len() + at.len();
            if total > scatter::MAX_AVOID {
                return Err(e(&cl.path, ErrorCode::Limit, format!("{total} avoid circles with '{name}''s placements; at most {}", scatter::MAX_AVOID)).with_numbers(total as f64, scatter::MAX_AVOID as f64));
            }
            avoid.extend(at.iter().map(|q| [q[0], q[1], radius]));
        }
        let spec = ScatterSpec {
            area,
            offset: [0.0, 0.0],
            min_distance: c.ranged(n, "min_distance", 1.0, 0.01, 1e4)?,
            max_count: c.int(n, "count", 256, 0, scatter::MAX_POINTS as i64)?,
            seed: self.seed,
            attempts: c.int(n, "attempts", 30, 1, scatter::MAX_ATTEMPTS as i64)?,
            slope: pair("slope")?,
            height: pair("height")?,
            mask: match c.sub(n, "mask") {
                Some(m) => Some(scatter::Mask { scale: c.ranged(m, "scale", 5.0, 0.01, 1e4)?, threshold: c.ranged(m, "threshold", 0.5, 0.0, 1.0)?, octaves: c.int(m, "octaves", 3, 1, 8)? }),
                None => None,
            },
            splat: match c.sub(n, "splat") {
                Some(s) => Some((c.int(s, "layer", 0, 0, 3)? as usize, c.ranged(s, "least", 0.5, 0.0, 1.0)?)),
                None => None,
            },
            avoid,
            weights,
            scale: c.vec2(n, "scale", [1.0, 1.0])?,
            yaw: c.vec2(n, "yaw", [0.0, 360.0])?,
            tilt: c.ranged(n, "tilt", 0.0, 0.0, 90.0)?,
            align: c.ranged(n, "align", 0.0, 0.0, 1.0)?,
            sink: c.ranged(n, "sink", 0.0, -100.0, 100.0)?,
        };
        let ground: Option<Arc<Terrain>> = match n.get("on") {
            Some(Val::Str(name)) => {
                let (_, t, moved) = self
                    .terrains
                    .iter()
                    .find(|(p, _, _)| p == name)
                    .ok_or_else(|| e(&n.at("on"), ErrorCode::UnknownName, format!("no earlier terrain part named '{name}' (terrains: {})", self.terrains.iter().map(|(p, _, _)| p.as_str()).collect::<Vec<_>>().join(", "))))?;
                if *moved {
                    return Err(e(&n.at("on"), ErrorCode::Geometry, format!("terrain part '{name}' has ops that move or reshape it: scatter samples the generator's own heights, so put the ops on a part that includes both, or use colour ops only")));
                }
                Some(Arc::clone(t))
            }
            _ => None,
        };
        let ground_fn = |x: f32, z: f32| -> Option<scatter::Ground> {
            match &ground {
                None => scatter::flat(x, z),
                Some(t) => Some(scatter::Ground { height: t.height_at(x, z)?, normal: t.normal_at(x, z)?, splat: t.splat_at(x, z) }),
            }
        };
        let (places, st) = scatter::scatter(&spec, &ground_fn, c.work_left(&n.path, scatter::MAX_WORK)?).map_err(|er| e(&n.at(er.field), if er.limit { ErrorCode::Limit } else { ErrorCode::OutOfRange }, er.message))?;
        c.spend(st.work);
        self.scattered.push((self.part.name.clone(), places.iter().map(|p| [p.position[0], p.position[2]]).collect()));
        // Each item's variants, built once.
        let mut built: Vec<Vec<Vec<BuiltPart>>> = Vec::new();
        for (ii, it) in items.iter().enumerate() {
            let variants = c.int(it, "variants", 1, 1, MAX_VARIANTS as i64)?;
            let mut vs = Vec::new();
            for v in 0..variants {
                let vseed = stream_seed(self.seed, &format!("item{ii}/variant{v}"));
                let pieces = if let Some(Val::Obj(sn)) = it.get("shape") {
                    let material = match it.get("material") {
                        Some(Val::Str(m)) => m.clone(),
                        _ => self.part.material.clone().unwrap_or_default(),
                    };
                    let colour = if it.get("color").is_some() { c.color(it, "color") } else { self.material_color(&material) };
                    c.paint.set(colour);
                    let s = c.shape_and_ops(sn, vseed)?;
                    vec![self.piece(s, &material, None)]
                } else {
                    let Some(Val::Str(asset)) = it.get("include") else { unreachable!("checked") };
                    let mut vals: Vec<(String, f64)> = Vec::new();
                    if let Some(Val::Obj(ps)) = it.get("params") {
                        let mut rng = specula_core::rng::Rng::new(vseed, 77);
                        for (k, val) in &ps.fields {
                            let x = match val {
                                Val::List(_) => {
                                    let r = c.list(val, &ps.at(k))?;
                                    let t = rng.f32() as f64;
                                    r[0] as f64 + (r[1] as f64 - r[0] as f64) * t
                                }
                                other => c.val_num(other, &ps.at(k))?,
                            };
                            vals.push((k.clone(), x));
                        }
                    }
                    let path = it.at("include");
                    build_include(asset, &vals, vseed, self.params, self.stack, self.tally, self.coarse, &path)?
                };
                // Counted as copies below: take the template out of the tally.
                let t: usize = pieces.iter().map(|p| p.shape.triangle_count()).sum();
                self.tally.settle(t, 0);
                vs.push(pieces);
            }
            built.push(vs);
        }
        let copies: usize = places.iter().map(|p| { let vs = &built[p.item as usize]; vs[(p.variant % vs.len() as u64) as usize].iter().map(|q| q.shape.triangle_count()).sum::<usize>() }).sum();
        self.tally.grow(&n.path, 0, copies)?;
        // Copies merged by material.
        let mut by_mat: Vec<BuiltPart> = Vec::new();
        for p in &places {
            let vs = &built[p.item as usize];
            let t = p.transform();
            for q in &vs[(p.variant % vs.len() as u64) as usize] {
                let mut s = q.shape.clone();
                s.transform(&t);
                match by_mat.iter_mut().find(|b| b.material == q.material && b.smooth_angle == q.smooth_angle) {
                    Some(b) => b.shape.merge(&s),
                    None => by_mat.push(BuiltPart { shape: s, ..q.clone() }),
                }
            }
        }
        if by_mat.is_empty() {
            return Err(e(&n.path, ErrorCode::Geometry, format!("the scatter placed nothing ({} sampled; rejected: outside {}, slope {}, height {}, mask {}, splat {}, avoid {})", st.sampled, st.rejected[0], st.rejected[1], st.rejected[2], st.rejected[3], st.rejected[4], st.rejected[5])));
        }
        for b in &by_mat {
            check_extent(&b.shape, &n.path)?;
        }
        c.stat(
            &n.path,
            Json::obj([
                ("op", Json::from("scatter")),
                ("placed", Json::from(st.placed)),
                ("sampled", Json::from(st.sampled)),
                ("rejected", Json::obj([("outside", st.rejected[0]), ("slope", st.rejected[1]), ("height", st.rejected[2]), ("mask", st.rejected[3]), ("splat", st.rejected[4]), ("avoid", st.rejected[5])].map(|(k, v)| (k, Json::from(v))))),
                ("triangles", Json::from(copies)),
            ]),
        );
        Ok(by_mat)
    }
}

/// Builds an included recipe's parts (as an include part does).
#[allow(clippy::too_many_arguments)]
fn build_include(asset: &str, vals: &[(String, f64)], seed: u64, params: &BuildParams, stack: &mut Vec<String>, tally: &Tally, coarse: bool, path: &str) -> Result<Vec<BuiltPart>, RecipeError> {
    if stack.len() > MAX_INCLUDE_DEPTH {
        return Err(e(path, ErrorCode::IncludeCycle, format!("includes nest deeper than {MAX_INCLUDE_DEPTH}: {}", stack.join(" > "))));
    }
    if stack.iter().any(|s| s == asset) {
        return Err(e(path, ErrorCode::IncludeCycle, format!("include cycle: {} > {asset}", stack.join(" > "))));
    }
    tally.include(path)?;
    let text = params.library.and_then(|l| l.get(asset)).ok_or_else(|| e(path, ErrorCode::UnknownAsset, format!("'{asset}' is not in the recipe library")))?;
    let child = Recipe::parse(text, params.style).map_err(|er| RecipeError { path: format!("{path}({asset}).{}", er.path), ..er })?;
    let refs: Vec<(&str, f64)> = vals.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    stack.push(asset.to_string());
    let res = build_parts(&child, &refs, seed, params, stack, tally, coarse);
    stack.pop();
    Ok(res.map_err(|er| RecipeError { path: format!("{path}({asset}).{}", er.path), ..er })?.0)
}

// ---------------------------------------------------------------------------------------------
// Textures

/// Bakes the texture set of every textured material key LOD 0 uses (`preset+texture`), each
/// texture once.
/// `spent`: the build's work so far; the bakes get what is left of [`MAX_BUILD_WORK`], and all
/// the sets together at most [`texgraph::MAX_MODEL_GPU_BYTES`].
pub(super) fn bake_textures(recipe: &Recipe, params: &BuildParams, lod0: &MeshData, extra: &[String], spent: u64) -> Result<Vec<(String, Arc<TextureSet>)>, RecipeError> {
    let mut baked: std::collections::BTreeMap<String, Arc<TextureSet>> = std::collections::BTreeMap::new();
    let mut out = Vec::new();
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4);
    let mut work = spent;
    let mut gpu = 0u64;
    // The parts' material keys, then `extra` keys (the terrain renderer's layers: `terrain+<graph>`).
    for key in lod0.parts.iter().map(|p| &p.material).chain(extra) {
        let Some((_, tex)) = key.split_once('+') else { continue };
        if out.iter().any(|(k, _): &(String, Arc<TextureSet>)| k == key) {
            continue;
        }
        let set = match baked.get(tex) {
            Some(s) => Arc::clone(s),
            None => {
                // `lib:<file>`: a library graph; `<recipe>/<name>`: an inline graph of that recipe.
                let (path, graph): (String, TexGraph) = if let Some(lib) = tex.strip_prefix("lib:") {
                    let path = recipe.textures.iter().find(|(_, s)| matches!(s, TexSource::Library(l) if l == lib)).map_or_else(|| format!("textures({lib})"), |(n, _)| format!("textures.{n}"));
                    let text = params.library.and_then(|l| l.get(lib)).ok_or_else(|| e(&path, ErrorCode::UnknownAsset, format!("texture graph '{lib}' is not in the recipe library")))?;
                    let json = Json::parse(text).map_err(|er| e(&path, ErrorCode::Json, format!("{lib}: {}", er.message)).with_offset(er.offset))?;
                    let g = TexGraph::parse_json(&json, params.style, &format!("{path}({lib})"))?;
                    (path, g)
                } else {
                    let (owner, name) = tex.split_once('/').unwrap_or(("", tex));
                    let path = format!("textures.{name}");
                    match recipe.textures.iter().find(|(n, _)| n == name).map(|(_, s)| s) {
                        Some(TexSource::Inline(g)) if owner == recipe.name => (path, (**g).clone()),
                        _ => return Err(e(&path, ErrorCode::UnknownName, format!("texture '{name}' is inline in the included recipe '{owner}': included recipes' inline graphs aren't baked; make it a library texture graph file"))),
                    }
                };
                gpu += graph.gpu_bytes();
                if gpu > texgraph::MAX_MODEL_GPU_BYTES {
                    return Err(e(&path, ErrorCode::Limit, format!("the model's texture sets would take {gpu} GPU bytes; at most {} per model (fewer or smaller textures)", texgraph::MAX_MODEL_GPU_BYTES)).with_numbers(gpu as f64, texgraph::MAX_MODEL_GPU_BYTES as f64));
                }
                work = work.saturating_add(graph.cost().saturating_mul(crate::work::TEXTURE_OP));
                if work > MAX_BUILD_WORK {
                    return Err(e(&path, ErrorCode::Limit, format!("with its textures the model needs {work} work units; at most {MAX_BUILD_WORK} (lower texture sizes, octaves, blur or ao radius)")).with_numbers(work as f64, MAX_BUILD_WORK as f64));
                }
                let s = Arc::new(texgraph::bake(&graph, threads).map_err(|er| RecipeError { path: format!("{path}.{}", er.path), ..er })?);
                baked.insert(tex.to_string(), Arc::clone(&s));
                s
            }
        };
        out.push((key.clone(), set));
    }
    Ok(out)
}

/// A bounded summary of the extras for reports.
pub(super) fn extras_report(x: &ModelExtras) -> Json {
    if x.textures.is_empty() && x.terrain.is_none() && x.stats.is_empty() {
        return Json::Null;
    }
    let mut pairs = Vec::new();
    if !x.textures.is_empty() {
        pairs.push((
            "textures",
            Json::arr(x.textures.iter().map(|(k, t)| {
                Json::obj([
                    ("material", Json::from(k.as_str())),
                    ("size", Json::from(t.size)),
                    ("gpu_bytes", Json::from(t.gpu_bytes())),
                    ("normal_map", Json::from(t.normal.is_some())),
                    ("hash", Json::from(format!("{:016x}", t.hash))),
                ])
            })),
        ));
        pairs.push(("texture_bytes", Json::from(x.textures.iter().map(|(_, t)| t.gpu_bytes()).sum::<u64>())));
    }
    if let Some(t) = &x.terrain {
        // Where the part's ops put it (null: an op reshaped it; colliders and scatters refuse).
        let placed = x.terrain_transform.map_or(Json::Null, |a| {
            let v3 = |v: [f32; 3]| Json::arr(v.map(|c| Json::Num(c as f64)));
            Json::obj([("move", v3(a.t)), ("axes", Json::arr(a.cols.map(v3)))])
        });
        pairs.push((
            "terrain",
            Json::obj([
                ("columns", Json::from(t.columns)),
                ("rows", Json::from(t.rows)),
                ("cell", Json::arr(t.cell.map(|v| Json::Num(v as f64)))),
                ("splat", Json::arr(t.splat_names.iter().map(|s| Json::from(s.as_str())))),
                ("transform", placed),
            ]),
        ));
    }
    if !x.stats.is_empty() {
        pairs.push(("steps", Json::arr(x.stats.iter().map(|(p, j)| Json::obj([("path", Json::from(p.as_str())), ("did", j.clone())])))));
    }
    Json::obj(pairs)
}

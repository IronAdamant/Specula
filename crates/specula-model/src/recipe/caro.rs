//! Recipe glue for a body ([`crate::caro`]): the shape `body` and the sculpt node `body`, its
//! bones by name (parents resolved and refused at parse time, a `*_l` bone mirrored to its
//! `*_r` twin), its field as one sculpt primitive ([`crate::sdf::Prim::Body`]), and the named
//! places it keeps through meshing as anchors (its joints, each bone's two ends, and the places
//! a bone names along it).

use super::*;
use crate::caro::{self, Bone, Carnis, End, Flesh, Swell};
use crate::sdf;

const SWELL: &[Field] = &[r("at", Vec3), r("radii", Vec3)];
/// A place a bone names: `at` along it (0 its head, 1 its tail), `side` the direction across it
/// (toward its side, across), `depth` how far under the surface.
const PLACE: &[Field] = &[r("name", Name), r("at", Num), o("side", Vec2), o("depth", Num)];
pub(super) const BONE: &[Field] = &[
    r("name", Name),
    o("parent", Name),
    r("head", Vec3),
    o("tail", Vec3),
    o("sections", Rows),
    o("side", Vec3),
    o("square", Num),
    o("swells", Objs(SWELL)),
    o("mirror", Bool),
    o("smooth", Num),
    o("places", Objs(PLACE)),
];
pub(super) const FINE: &[Field] = &[r("bone", Name), o("resolution", Int), o("at", Num)];
/// The shape `body`: bones with flesh on them, as one closed surface.
pub(super) const BODY_FIELDS: &[Field] = &[r("bones", Objs(BONE)), o("smooth", Num), o("blend", Num), o("resolution", Int), o("fine", Objs(FINE))];

/// What a body is made with unless its recipe says.
pub(super) const SMOOTH: f64 = 0.02;
pub(super) const BLEND: f64 = 0.03;
pub(super) const RESOLUTION: i64 = 96;
/// The level a body is held at until a mesher holds it at its own ([`crate::sdf`]'s
/// `fit_bodies`), metres.
pub(super) const KEEP: f32 = 0.05;
/// Most places one body names.
pub const MAX_PLACES: usize = 64;
/// A bone other than the root is at least this long, metres.
pub const LEAST_LENGTH: f32 = 0.001;
/// Most swells on one bone.
pub const MAX_BONE_SWELLS: usize = 4;

/// One bone of a body as its recipe names it: its name, its parent's place among the bones,
/// and which of the recipe's bones it is (`twin`: the `*_r` twin `mirror` adds).
#[derive(Clone, Debug)]
pub(super) struct BodyBone {
    pub name: String,
    pub parent: Option<usize>,
    pub src: usize,
    pub twin: bool,
}

fn bone_nodes(n: &Node) -> Vec<&Node> {
    match n.get("bones") {
        Some(Val::List(items)) => items.iter().filter_map(|v| if let Val::Obj(b) = v { Some(b) } else { None }).collect(),
        _ => Vec::new(),
    }
}

fn objs<'a>(n: &'a Node, field: &str) -> Vec<&'a Node> {
    match n.get(field) {
        Some(Val::List(items)) => items.iter().filter_map(|v| if let Val::Obj(b) = v { Some(b) } else { None }).collect(),
        _ => Vec::new(),
    }
}

fn name_of(n: &Node, field: &str) -> Option<String> {
    match n.get(field) {
        Some(Val::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// A body's bones with their twins, parents resolved: every refusal of a name or a parent is
/// here, at parse time, with its path. (A parent named after its child is a cycle when its own
/// parents come back to the child, and said so.)
pub(super) fn body_bones(n: &Node) -> Result<Vec<BodyBone>, RecipeError> {
    let nodes = bone_nodes(n);
    if nodes.is_empty() {
        return Err(e(&n.at("bones"), ErrorCode::MissingField, "a body needs at least one bone (the root)"));
    }
    let mut all: Vec<(String, usize, bool)> = Vec::new();
    for (i, b) in nodes.iter().enumerate() {
        let name = name_of(b, "name").unwrap_or_default();
        if name.is_empty() {
            return Err(e(&b.at("name"), ErrorCode::MissingField, format!("bone {i} has no name")));
        }
        all.push((name.clone(), i, false));
        if matches!(b.get("mirror"), Some(Val::Bool(true))) {
            let Some(stem) = name.strip_suffix("_l") else {
                return Err(e(&b.at("mirror"), ErrorCode::InvalidRig, format!("'{name}' is mirrored: a mirrored bone is named *_l (left, +X) and its twin is *_r")));
            };
            all.push((format!("{stem}_r"), i, true));
        }
    }
    if all.len() > caro::MAX_PARTS {
        return Err(e(&n.at("bones"), ErrorCode::Limit, format!("{} bones with their twins; a body has at most {}", all.len(), caro::MAX_PARTS)).with_numbers(all.len() as f64, caro::MAX_PARTS as f64));
    }
    let mut out: Vec<BodyBone> = Vec::with_capacity(all.len());
    for (k, (name, src, twin)) in all.iter().enumerate() {
        let b = nodes[*src];
        let path = if *twin { b.at("mirror") } else { b.at("name") };
        if all[..k].iter().any(|(o, _, _)| o == name) {
            return Err(e(&path, ErrorCode::Duplicate, format!("two bones are called '{name}'")));
        }
        let parent = match name_of(b, "parent") {
            None if k == 0 => None,
            None => return Err(e(&b.at("parent"), ErrorCode::MissingField, format!("'{name}' needs a parent: only the first bone is the root"))),
            Some(_) if k == 0 => return Err(e(&b.at("parent"), ErrorCode::InvalidRig, "the first bone is the root and has no parent")),
            Some(p) => {
                // A twin hangs from its parent's twin when there is one.
                let wanted = if *twin { p.strip_suffix("_l").map(|s| format!("{s}_r")).filter(|t| out.iter().any(|o| &o.name == t)).unwrap_or(p.clone()) } else { p.clone() };
                match out.iter().position(|o| o.name == wanted) {
                    Some(at) => Some(at),
                    None => {
                        let later = all.iter().position(|(o, _, _)| *o == wanted);
                        return Err(match later {
                            Some(at) => {
                                let mut chain = vec![name.clone(), wanted.clone()];
                                let mut cur = at;
                                let mut cycle = false;
                                for _ in 0..all.len() {
                                    match name_of(nodes[all[cur].1], "parent").and_then(|q| all.iter().position(|(o, _, _)| *o == q)) {
                                        Some(next) => {
                                            chain.push(all[next].0.clone());
                                            if next == k {
                                                cycle = true;
                                                break;
                                            }
                                            cur = next;
                                        }
                                        None => break,
                                    }
                                }
                                if cycle {
                                    e(&b.at("parent"), ErrorCode::InvalidRig, format!("a cycle of parents: {}", chain.join(" > ")))
                                } else {
                                    e(&b.at("parent"), ErrorCode::InvalidRig, format!("'{name}' names '{wanted}' as its parent, which comes after it: a parent comes first"))
                                }
                            }
                            None => {
                                let known: Vec<&str> = out.iter().map(|o| o.name.as_str()).collect();
                                e(&b.at("parent"), ErrorCode::UnknownName, format!("no bone '{wanted}' before '{name}' (bones so far: {})", known.join(", ")))
                            }
                        });
                    }
                }
            }
        };
        out.push(BodyBone { name: name.clone(), parent, src: *src, twin: *twin });
    }
    Ok(out)
}

/// A place a body keeps: which bone, its name, how far along, the direction across (toward
/// the bone's side, across) and how deep.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Place {
    pub bone: usize,
    pub name: String,
    pub at: f32,
    pub side: [f32; 2],
    pub depth: f32,
}

/// A body as its recipe gives it, every number evaluated and checked, and its flesh.
pub(super) struct BodyPlan {
    pub defs: Vec<BodyBone>,
    pub bones: Vec<Bone>,
    pub places: Vec<Place>,
    pub flesh: Flesh,
}

/// Where in the recipe a refusal of the field is.
fn caro_path(n: &Node, defs: &[BodyBone], swells: &[(usize, usize)], er: &caro::CaroError) -> String {
    let bone_path = |b: usize| defs.get(b).map(|d| format!("{}.bones[{}]{}", n.path, d.src, if d.twin { ".mirror" } else { "" }));
    match (er.bone, er.swell) {
        (_, Some(s)) => swells.get(s).and_then(|&(b, k)| bone_path(b).map(|p| format!("{p}.swells[{k}]"))).unwrap_or_else(|| n.at("bones")),
        (Some(b), None) => bone_path(b).unwrap_or_else(|| n.at("bones")),
        (None, None) => match er.code {
            "smooth" => n.at("smooth"),
            _ => n.at("bones"),
        },
    }
}

fn caro_error(n: &Node, defs: &[BodyBone], swells: &[(usize, usize)], er: caro::CaroError) -> RecipeError {
    let code = match er.code {
        "too_many_parts" | "too_many_swells" | "too_many_joints" | "group_seams" | "bone_sections" => ErrorCode::Limit,
        "point_bone_sections" | "profile_not_positive" | "no_flesh" | "joint_flesh" | "joint_point" | "joint_bone" => ErrorCode::Geometry,
        _ => ErrorCode::OutOfRange,
    };
    let mut out = e(&caro_path(n, defs, swells, &er), code, er.to_string());
    out.measured = er.measured;
    out.limit = er.limits.map(|(lo, hi)| if er.measured.is_some_and(|m| m < lo) { lo } else { hi });
    out
}

/// The default direction a bone's breadth is measured toward (as [`crate::caro`] takes it).
fn default_side(head: V3, tail: V3) -> V3 {
    let d = math3::sub(tail, head);
    let l = math3::length(d);
    if l > 0.0 && (d[0] / l).abs() > 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    }
}

impl Ctx<'_> {
    /// The body a `body` shape or node asks for.
    pub(super) fn body_plan(&self, n: &Node) -> Result<BodyPlan, RecipeError> {
        let defs = body_bones(n)?;
        let nodes = bone_nodes(n);
        let mut bones: Vec<Bone> = Vec::with_capacity(defs.len());
        let mut swells: Vec<Swell> = Vec::new();
        let mut swell_at: Vec<(usize, usize)> = Vec::new();
        let mut places: Vec<Place> = Vec::new();
        let mut own_smooth: Vec<Option<f32>> = Vec::new();
        let m = |v: V3| [-v[0], v[1], v[2]];
        for (i, d) in defs.iter().enumerate() {
            let b = nodes[d.src];
            if d.twin {
                // The bone just before it is its original: its places mirrored across X = 0,
                // its side the mirror of its own (so an offset across changes sign).
                let original = bones[i - 1].clone();
                let side = original.side.unwrap_or_else(|| default_side(original.head, original.tail));
                bones.push(Bone { head: m(original.head), tail: m(original.tail), side: Some(m(side)), ..original });
                let mine: Vec<Swell> = swells.iter().zip(&swell_at).filter(|(_, (bb, _))| *bb == i - 1).map(|(s, _)| Swell { bone: i, at: [s.at[0], s.at[1], -s.at[2]], radii: s.radii }).collect();
                for (k, s) in mine.into_iter().enumerate() {
                    swells.push(s);
                    swell_at.push((i, k));
                }
                let theirs: Vec<Place> = places.iter().filter(|p| p.bone == i - 1).map(|p| Place { bone: i, side: [p.side[0], -p.side[1]], ..p.clone() }).collect();
                places.extend(theirs);
                own_smooth.push(own_smooth[i - 1]);
                continue;
            }
            let head = self.vec3(b, "head", [0.0; 3])?;
            let tail = self.vec3(b, "tail", head)?;
            let length = math3::length(math3::sub(tail, head));
            if i > 0 && (length.is_nan() || length < LEAST_LENGTH) {
                return Err(e(&b.at("tail"), ErrorCode::Geometry, format!("'{}' is {length} m long; a bone is at least {LEAST_LENGTH} m (only the root may be a point)", d.name)).with_numbers(f64::from(length), f64::from(LEAST_LENGTH)));
            }
            let side = match b.get("side") {
                Some(_) => {
                    let s = self.vec3(b, "side", [1.0, 0.0, 0.0])?;
                    if !(s.iter().all(|v| v.is_finite()) && math3::length(s) >= 1e-6) {
                        return Err(e(&b.at("side"), ErrorCode::OutOfRange, format!("'{}' has a side that is no direction", d.name)));
                    }
                    Some(s)
                }
                None => None,
            };
            let square = self.ranged(b, "square", 2.0, f64::from(caro::SQUARE.0), f64::from(caro::SQUARE.1))?;
            let sections = match b.get("sections") {
                Some(v) => self.rows(v, &b.at("sections"), (3, 3), caro::MAX_STATIONS)?.into_iter().map(|r| [r[0], r[1], r[2]]).collect(),
                None => Vec::new(),
            };
            let given = objs(b, "swells");
            if given.len() > MAX_BONE_SWELLS {
                return Err(e(&b.at("swells"), ErrorCode::Limit, format!("'{}' has {} swells; at most {MAX_BONE_SWELLS}", d.name, given.len())).with_numbers(given.len() as f64, MAX_BONE_SWELLS as f64));
            }
            for (k, s) in given.into_iter().enumerate() {
                swells.push(Swell { bone: i, at: self.vec3(s, "at", [0.0; 3])?, radii: self.vec3(s, "radii", [0.01; 3])? });
                swell_at.push((i, k));
            }
            for p in objs(b, "places") {
                places.push(Place {
                    bone: i,
                    name: name_of(p, "name").unwrap_or_default(),
                    at: self.ranged(p, "at", 0.0, f64::from(caro::ALONG.0), f64::from(caro::ALONG.1))?,
                    side: self.vec2(p, "side", [1.0, 0.0])?,
                    depth: self.ranged(p, "depth", 0.0, 0.0, 1.0)?,
                });
                let last = places.last().map(|p| (p.name.clone(), p.side));
                if let Some((name, side)) = last {
                    if name.is_empty() || places[..places.len() - 1].iter().any(|q| q.bone == i && q.name == name) {
                        return Err(e(&b.at("places"), ErrorCode::Duplicate, format!("'{}' names a place '{name}' twice, or one without a name", d.name)));
                    }
                    if !(side.iter().all(|v| v.is_finite()) && (side[0] * side[0] + side[1] * side[1]).sqrt() >= 1e-6) {
                        return Err(e(&b.at("places"), ErrorCode::OutOfRange, format!("'{}': the place '{name}' has a side that is no direction across the bone", d.name)));
                    }
                }
            }
            own_smooth.push(match b.get("smooth") {
                Some(_) => Some(self.ranged(b, "smooth", 0.0, 0.0, f64::from(caro::SMOOTH.1))?),
                None => None,
            });
            bones.push(Bone { head, tail, side, square, sections });
        }
        if places.len() > MAX_PLACES {
            return Err(e(&n.at("bones"), ErrorCode::Limit, format!("{} places with their twins; a body names at most {MAX_PLACES}", places.len())).with_numbers(places.len() as f64, MAX_PLACES as f64));
        }
        if swells.len() > caro::MAX_SWELLS {
            return Err(e(&n.at("bones"), ErrorCode::Limit, format!("{} swells with their twins; a body has at most {}", swells.len(), caro::MAX_SWELLS)).with_numbers(swells.len() as f64, caro::MAX_SWELLS as f64));
        }
        // `fine` names bones where detail matters (a hint, not a second mesh: the decisions of
        // 2026-11-24); `blend` is the skin's, which is not made yet. Both are checked.
        for f in objs(n, "fine") {
            let name = name_of(f, "bone").unwrap_or_default();
            if !defs.iter().any(|d| d.name == name) {
                let known: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
                return Err(e(&f.at("bone"), ErrorCode::UnknownName, format!("no bone '{name}' in the body (bones: {})", known.join(", "))));
            }
            self.int(f, "resolution", 48, 4, i64::from(sdf::MAX_RESOLUTION))?;
            self.ranged(f, "at", 0.0, -0.5, 0.95)?;
        }
        self.ranged(n, "blend", BLEND, 0.0, 1.0)?;
        let smooth = self.ranged(n, "smooth", SMOOTH, f64::from(caro::SMOOTH.0), f64::from(caro::SMOOTH.1))?;
        let parents: Vec<Option<usize>> = defs.iter().map(|d| d.parent).collect();
        // A bone's own `smooth` is the width where it meets the bones at its head.
        let joints: Vec<caro::Joint> = caro::joints_of(&bones, &parents)
            .into_iter()
            .map(|mut j| {
                j.smooth = (0..2).filter(|&s| j.ends[s] == End::Head).find_map(|s| own_smooth[j.bones[s]]).map(|w| w.min(smooth));
                j
            })
            .collect();
        let flesh = Flesh::jointed(&bones, &swells, smooth, KEEP, &joints).map_err(|er| caro_error(n, &defs, &swell_at, er))?;
        Ok(BodyPlan { defs, bones, places, flesh })
    }

    /// The sculpt tree of a `body` shape: the body as one node.
    pub(super) fn body_tree(&self, n: &Node) -> Result<(sdf::SdfNode, BodyPlan), RecipeError> {
        let plan = self.body_plan(n)?;
        let node = sdf::SdfNode { kind: sdf::Kind::Prim(sdf::Prim::Body(Carnis::new(plan.flesh.clone()))), m: sdf::Modifiers::default() };
        Ok((node, plan))
    }
}

/// Where an anchor lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum On {
    /// On the body's surface (its field's zero, found along the place's direction).
    Surface,
    /// Under it, at the depth it gives.
    Inside,
    /// Outside the flesh (a bone with none there, or a place past its end).
    Free,
}

/// A named place of a body, kept through its meshing so detail can be attached there: where
/// it is, its frame (the bone's direction, toward its side, across), where it lies and how far
/// under the surface (the field's value there, negated: a bound of the depth, never more).
#[derive(Clone, Debug, PartialEq)]
pub struct BodyAnchor {
    pub name: String,
    pub at: V3,
    pub frame: [V3; 3],
    pub on: On,
    pub depth: f32,
}

/// How near the surface a place's search lands (metres): the halving stops within this.
pub const ANCHOR_TOLERANCE: f32 = 1e-5;

/// A body's anchors: each bone's head and tail (`BONE.head`, `BONE.tail`), each joint's middle
/// (`A~B`), and each place a bone names (`BONE.PLACE`: on the surface, along the direction it
/// gives from the bone's line at its share along, then its depth in along that direction).
pub(super) fn body_anchors(plan: &BodyPlan) -> Vec<BodyAnchor> {
    let f = &plan.flesh;
    let lie = |x: V3| {
        let v = f.value(x);
        if v < 0.0 {
            (On::Inside, -v)
        } else {
            (On::Free, 0.0)
        }
    };
    let mut out = Vec::new();
    for (d, b) in plan.defs.iter().zip(&plan.bones) {
        let frame = caro::bone_frame(b);
        for (end, at) in [("head", b.head), ("tail", b.tail)] {
            let (on, depth) = lie(at);
            out.push(BodyAnchor { name: format!("{}.{end}", d.name), at, frame, on, depth });
        }
    }
    let parents: Vec<Option<usize>> = plan.defs.iter().map(|d| d.parent).collect();
    for j in caro::joints_of(&plan.bones, &parents) {
        let end = |s: usize| if j.ends[s] == End::Head { plan.bones[j.bones[s]].head } else { plan.bones[j.bones[s]].tail };
        let at = math3::scale(math3::add(end(0), end(1)), 0.5);
        let (on, depth) = lie(at);
        out.push(BodyAnchor { name: format!("{}~{}", plan.defs[j.bones[0]].name, plan.defs[j.bones[1]].name), at, frame: caro::bone_frame(&plan.bones[j.bones[1]]), on, depth });
    }
    for p in &plan.places {
        let b = &plan.bones[p.bone];
        let frame = caro::bone_frame(b);
        let axis = math3::add(b.head, math3::scale(math3::sub(b.tail, b.head), p.at));
        let l = (p.side[0] * p.side[0] + p.side[1] * p.side[1]).sqrt();
        let dir = math3::add(math3::scale(frame[1], p.side[0] / l), math3::scale(frame[2], p.side[1] / l));
        let name = format!("{}.{}", plan.defs[p.bone].name, p.name);
        let ray = |t: f32| math3::add(axis, math3::scale(dir, t));
        if f.value(axis) >= 0.0 {
            out.push(BodyAnchor { name, at: axis, frame, on: On::Free, depth: 0.0 });
            continue;
        }
        // Out from the line until outside (within the reach of the bone and the join), then
        // halved to the surface.
        let step = (f.part_reach_of_bone(p.bone) / 16.0).max(1e-4);
        let (mut lo, mut hi) = (0.0f32, step);
        let mut steps = 0;
        while f.value(ray(hi)) < 0.0 && steps < 64 {
            lo = hi;
            hi += step;
            steps += 1;
        }
        while hi - lo > ANCHOR_TOLERANCE {
            let mid = 0.5 * (lo + hi);
            if f.value(ray(mid)) < 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let surface = ray(0.5 * (lo + hi));
        let at = math3::sub(surface, math3::scale(dir, p.depth));
        let on = if p.depth > 0.0 { On::Inside } else { On::Surface };
        out.push(BodyAnchor { name, at, frame: [frame[0], dir, math3::cross(frame[0], dir)], on, depth: p.depth });
    }
    out
}

/// An anchor as the build's stats say it.
pub(super) fn anchor_json(a: &BodyAnchor) -> Json {
    let num = |x: f32| Json::Num(dmath::round_f64(f64::from(x) * 1e5) / 1e5);
    let v = |p: V3| Json::arr(p.map(num));
    Json::obj([
        ("name", Json::from(a.name.as_str())),
        ("at", v(a.at)),
        ("frame", Json::arr(a.frame.map(v))),
        ("on", Json::from(match a.on {
            On::Surface => "surface",
            On::Inside => "inside",
            On::Free => "free",
        })),
        ("depth_m", num(a.depth)),
    ])
}

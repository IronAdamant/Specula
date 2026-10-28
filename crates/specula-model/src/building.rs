//! Tier-4 buildings: a floor plan and a few rules become walls, openings, trims and a roof.
//!
//! The plan is a polygon in XZ (any orientation). Walls rise `floors × floor_height` from
//! y = 0 (plus a plinth), solid by default (openings are recesses, cheap and dark inside) or
//! `hollow` (a shell `wall` thick: openings go through, as ruins need). Windows are laid out per
//! side and floor in bays of `spacing` metres, centred, kept `margin` from the corners and clear
//! of doors; every opening is cut with the exact booleans ([`crate::csg`]) in one pass, its
//! rims optionally chamfered. Sills, lintels, glass and door leaves are separate pieces with
//! their own roles (materials). Roofs: flat (with a parapet), gable, hip and shed on rectangular
//! plans, pyramid on convex ones, or none. `ruin` breaks the wall tops and drops the roof,
//! seeded. Everything is bounded: plan points, floors, openings and the boolean budget.

use specula_core::rng::Rng;

use crate::csg::{self, Bevel, CsgOp, CsgTree};
use crate::math3::{self, Affine, V2, V3};
use crate::poly;
use crate::shape::{Shape, TAG_FACE};

/// Most plan points, floors and openings (windows and doors together).
pub const MAX_PLAN_POINTS: usize = 64;
pub const MAX_FLOORS: u32 = 16;
pub const MAX_OPENINGS: usize = 400;

/// What a piece is made of (the recipe maps each to a material preset).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Walls,
    Roof,
    Trim,
    Door,
    Glass,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Walls => "walls",
            Role::Roof => "roof",
            Role::Trim => "trim",
            Role::Door => "door",
            Role::Glass => "glass",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Windows {
    pub width: f32,
    pub height: f32,
    /// Height of the opening's bottom above its floor.
    pub sill: f32,
    /// Bay width: one window per `spacing` metres of wall.
    pub spacing: f32,
    /// Distance kept from the plan's corners.
    pub margin: f32,
    /// Recess depth of a solid building's openings.
    pub depth: f32,
    pub arch: bool,
    /// Sill and lintel trim: their overhang past the opening (0 = none).
    pub trim: f32,
    pub glass: bool,
    /// Floors with windows: first..=last (0 = ground).
    pub floors: (u32, u32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Door {
    /// The plan side (edge index).
    pub side: usize,
    /// 0..1 along the side.
    pub at: f32,
    pub width: f32,
    pub height: f32,
    pub arch: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoofKind {
    None,
    Flat,
    Gable,
    Hip,
    Shed,
    Pyramid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roof {
    pub kind: RoofKind,
    pub pitch_deg: f32,
    pub overhang: f32,
    pub thickness: f32,
    /// Flat roofs: a parapet this high.
    pub parapet: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildingSpec {
    pub plan: Vec<V2>,
    pub floors: u32,
    pub floor_height: f32,
    pub wall: f32,
    pub hollow: bool,
    /// Plinth (height, how far it stands out), string courses at each floor line, a cornice.
    pub plinth: Option<(f32, f32)>,
    pub band: Option<(f32, f32)>,
    pub cornice: Option<(f32, f32)>,
    pub windows: Option<Windows>,
    pub doors: Vec<Door>,
    pub roof: Roof,
    /// 0..1: broken wall tops, no roof.
    pub ruin: f32,
    /// Chamfer on the openings' rims (metres; 0 = sharp).
    pub bevel: f32,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildingError {
    pub field: &'static str,
    pub message: String,
    pub limit: bool,
    pub measured: Option<f64>,
    pub max: Option<f64>,
}

impl BuildingError {
    fn bad(field: &'static str, m: impl Into<String>) -> BuildingError {
        BuildingError { field, message: m.into(), limit: false, measured: None, max: None }
    }
}

impl From<csg::CsgError> for BuildingError {
    fn from(e: csg::CsgError) -> BuildingError {
        BuildingError { field: "walls", message: format!("cutting the openings: {e}"), limit: e.kind == csg::CsgErrorKind::Limit, measured: e.measured, max: e.limit }
    }
}

/// What the generator made.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BuildingStats {
    pub windows: usize,
    pub doors: usize,
    pub csg_work: u64,
    pub bevel: csg::BevelStats,
}

/// The plan counter-clockwise in (x, z) (positive shoelace area with x right, z up).
fn ccw_xz(p: &[V2]) -> Vec<V2> {
    poly::canonical(p)
}

/// Offsets a counter-clockwise (x, z) plan outward by `d` (inward if negative).
fn offset(p: &[V2], d: f32) -> Vec<V2> {
    poly::inset(p, -d)
}

/// A vertical prism of `outline` (x, z) from `y0` to `y1`.
fn slab(outline: &[V2], y0: f32, y1: f32) -> Result<Shape, BuildingError> {
    let mut s = Shape::extrude(outline, y1 - y0, 1.0, 0.0, 0.0, 1, None).map_err(|g| BuildingError::bad("plan", g.0))?;
    s.transform(&Affine::translate([0.0, y0, 0.0]));
    Ok(s)
}

/// A closed prism: the polygon `face` (3D points in a plane, any winding) swept by `along`
/// (not in the face's plane).
fn prism(face: &[V3], along: V3) -> Shape {
    let n = face.len();
    let e = (1..n).map(|i| math3::sub(face[i], face[0])).find(|e| math3::length(*e) > 1e-6).unwrap_or([1.0, 0.0, 0.0]);
    let u = math3::normalize(e);
    let w = math3::normalize(math3::cross(along, u));
    let mut pts = face.to_vec();
    let mut flat: Vec<V2> = pts.iter().map(|p| [math3::dot(*p, u), math3::dot(*p, w)]).collect();
    // Counter-clockwise in (u, w), whose normal u × w is along `along`.
    if poly::signed_area(&flat) < 0.0 {
        pts.reverse();
        flat.reverse();
    }
    let tris = poly::triangulate(&flat).unwrap_or_else(|| (1..n as u32 - 1).map(|i| [0, i, i + 1]).collect());
    let mut s = Shape::default();
    s.positions.extend_from_slice(&pts);
    s.positions.extend(pts.iter().map(|p| math3::add(*p, along)));
    s.colors = vec![[1.0; 4]; 2 * n];
    let n32 = n as u32;
    for t in &tris {
        // The near cap faces against `along`, the far cap with it.
        s.tris.push([t[0], t[2], t[1]]);
        s.tris.push([n32 + t[0], n32 + t[1], n32 + t[2]]);
    }
    for i in 0..n32 {
        let j = (i + 1) % n32;
        s.tris.push([i, j, n32 + j]);
        s.tris.push([i, n32 + j, n32 + i]);
    }
    s.tags = vec![TAG_FACE; s.tris.len()];
    s
}

/// A box with centre `c`, half extents along the orthonormal axes `u` (horizontal along a
/// wall), up, and `n` (out of the wall).
fn oriented_box(c: V3, u: V3, n: V3, half: V3) -> Shape {
    let up = [0.0, 1.0, 0.0];
    let corner = |su: f32, sv: f32| math3::add(c, math3::add(math3::scale(u, su * half[0]), math3::add(math3::scale(up, sv * half[1]), math3::scale(n, half[2]))));
    let face = [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
    prism(&face, math3::scale(n, -2.0 * half[2]))
}

/// An opening's outline in a wall's plane: rectangle or arch, `width` × `height` from `y0`.
#[allow(clippy::too_many_arguments)]
fn opening_face(centre_xz: V2, y0: f32, u: V3, n: V3, width: f32, height: f32, arch: bool, out: f32) -> Vec<V3> {
    let outline: Vec<V2> = if arch { poly::arch(width, height, 8) } else { vec![[-width * 0.5, 0.0], [width * 0.5, 0.0], [width * 0.5, height], [-width * 0.5, height]] };
    let base = [centre_xz[0] + n[0] * out, y0, centre_xz[1] + n[2] * out];
    outline.iter().map(|q| [base[0] + u[0] * q[0], base[1] + q[1], base[2] + u[2] * q[0]]).collect()
}

impl BuildingSpec {
    pub fn check(&self) -> Result<Vec<V2>, BuildingError> {
        if self.plan.len() < 3 || self.plan.len() > MAX_PLAN_POINTS {
            return Err(BuildingError { field: "plan", message: format!("a plan needs 3..{MAX_PLAN_POINTS} points"), limit: self.plan.len() > MAX_PLAN_POINTS, measured: Some(self.plan.len() as f64), max: Some(MAX_PLAN_POINTS as f64) });
        }
        let plan = ccw_xz(&self.plan);
        if plan.len() < 3 || poly::signed_area(&plan) < 0.01 || !poly::is_simple(&plan) {
            return Err(BuildingError::bad("plan", "the plan must be a simple polygon enclosing an area"));
        }
        if !(1..=MAX_FLOORS).contains(&self.floors) {
            return Err(BuildingError { field: "floors", message: format!("floors must be 1..{MAX_FLOORS}"), limit: true, measured: Some(self.floors as f64), max: Some(MAX_FLOORS as f64) });
        }
        if !(self.floor_height >= 1.0 && self.floor_height <= 20.0) {
            return Err(BuildingError::bad("floor_height", "floor_height must be 1..20 m"));
        }
        if !(self.wall >= 0.05 && self.wall <= 3.0) {
            return Err(BuildingError::bad("wall", "wall must be 0.05..3 m"));
        }
        if let Some(w) = &self.windows {
            if !(w.width > 0.1 && w.height > 0.1 && w.spacing >= w.width && w.sill >= 0.0 && w.sill + w.height < self.floor_height) {
                return Err(BuildingError::bad("windows", "windows need width, height > 0.1 m, spacing >= width, and sill + height below the floor height"));
            }
            if !self.hollow && !(w.depth > 0.0 && w.depth < 3.0) {
                return Err(BuildingError::bad("windows.depth", "a solid building's openings need a depth in (0, 3) m"));
            }
        }
        let sides = plan.len();
        for d in &self.doors {
            if d.side >= sides {
                return Err(BuildingError::bad("doors", format!("door side {} doesn't exist (the plan has {sides} sides, 0..{})", d.side, sides - 1)));
            }
            if !(d.width > 0.2 && d.height > 0.5 && d.height < self.floor_height && (0.0..=1.0).contains(&d.at)) {
                return Err(BuildingError::bad("doors", "doors need width > 0.2 m, 0.5 m < height < floor_height, at in 0..1"));
            }
        }
        if !(0.0..=1.0).contains(&self.ruin) {
            return Err(BuildingError::bad("ruin", "ruin is 0..1"));
        }
        let r = &self.roof;
        if !(r.pitch_deg >= 0.0 && r.pitch_deg <= 75.0 && r.overhang >= 0.0 && r.overhang <= 3.0 && r.thickness >= 0.0 && r.parapet >= 0.0 && r.parapet <= 5.0) {
            return Err(BuildingError::bad("roof", "roof pitch 0..75 degrees, overhang 0..3 m, parapet 0..5 m"));
        }
        Ok(plan)
    }

    /// Builds the pieces, by role (several shapes may share a role).
    pub fn build(&self, budget: &mut csg::Budget) -> Result<(Vec<(Role, Shape)>, BuildingStats), BuildingError> {
        let plan = self.check()?;
        let mut rng = Rng::new(self.seed, 0x4255_494c);
        let mut stats = BuildingStats::default();
        let hollow = self.hollow || self.ruin > 0.0;
        let base = self.plinth.map_or(0.0, |p| p.0);
        let height = base + self.floors as f32 * self.floor_height;
        let mut pieces: Vec<(Role, Shape)> = Vec::new();
        // Walls.
        let mut walls = slab(&plan, 0.0, height)?;
        walls.set_color([1.0; 4]);
        let mut kids = vec![CsgTree::Leaf(walls)];
        if hollow {
            let inner = offset(&plan, -self.wall);
            if poly::signed_area(&inner) <= 0.0 || !poly::is_simple(&inner) {
                return Err(BuildingError::bad("wall", "the walls are too thick for the plan"));
            }
            kids.push(CsgTree::Leaf(slab(&inner, -1.0, height + 1.0)?));
        }
        let depth = |w: &Windows| if hollow { self.wall + 0.4 } else { w.depth };
        let mut cutters: Vec<Shape> = Vec::new();
        let n = plan.len();
        for side in 0..n {
            let (a, b) = (plan[side], plan[(side + 1) % n]);
            let len = ((b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1])).sqrt();
            if len < 0.2 {
                continue;
            }
            let u = [(b[0] - a[0]) / len, 0.0, (b[1] - a[1]) / len];
            // Outward: right of the direction for a counter-clockwise (x, z) plan.
            let out = [u[2], 0.0, -u[0]];
            let along = |t: f32| [a[0] + u[0] * t, a[1] + u[2] * t];
            let doors: Vec<&Door> = self.doors.iter().filter(|d| d.side == side).collect();
            for d in &doors {
                let c = along(d.at.clamp(0.0, 1.0) * len);
                let dd = self.windows.as_ref().map_or(if hollow { self.wall + 0.4 } else { 0.3 }, depth);
                let face = opening_face(c, base - 0.01, u, out, d.width, d.height, d.arch, 0.1);
                cutters.push(prism(&face, math3::scale(out, -(dd + 0.1))));
                // The leaf at the back of the recess (or in the wall's middle).
                let back = if hollow { self.wall * 0.5 } else { dd - 0.03 };
                let leaf_face = opening_face(c, base, u, out, d.width, d.height, d.arch, -back);
                pieces.push((Role::Door, prism(&leaf_face, math3::scale(out, -0.06))));
                stats.doors += 1;
            }
            let Some(w) = &self.windows else { continue };
            let usable = len - 2.0 * w.margin;
            if usable < w.width {
                continue;
            }
            let count = ((usable - w.width) / w.spacing) as usize + 1;
            for f in w.floors.0..=w.floors.1.min(self.floors - 1) {
                for k in 0..count {
                    let t = len * 0.5 + (k as f32 - (count - 1) as f32 * 0.5) * w.spacing;
                    if doors.iter().any(|d| f == 0 && (t - d.at * len).abs() < (d.width + w.width) * 0.5 + 0.3) {
                        continue;
                    }
                    if stats.windows + stats.doors >= MAX_OPENINGS {
                        return Err(BuildingError { field: "windows", message: format!("more than {MAX_OPENINGS} openings; raise the spacing"), limit: true, measured: None, max: Some(MAX_OPENINGS as f64) });
                    }
                    let c = along(t);
                    let y0 = base + f as f32 * self.floor_height + w.sill;
                    let face = opening_face(c, y0, u, out, w.width, w.height, w.arch, 0.1);
                    cutters.push(prism(&face, math3::scale(out, -(depth(w) + 0.1))));
                    stats.windows += 1;
                    if w.trim > 0.0 {
                        let tw = w.width * 0.5 + w.trim;
                        let sill_c = [c[0] + out[0] * 0.04, y0 - 0.04, c[1] + out[2] * 0.04];
                        pieces.push((Role::Trim, oriented_box(sill_c, u, out, [tw, 0.04, 0.08])));
                        if !w.arch {
                            let lint_c = [c[0] + out[0] * 0.02, y0 + w.height + 0.06, c[1] + out[2] * 0.02];
                            pieces.push((Role::Trim, oriented_box(lint_c, u, out, [tw, 0.06, 0.05])));
                        }
                    }
                    if w.glass {
                        let back = if hollow { self.wall * 0.5 } else { depth(w) - 0.02 };
                        let g = opening_face(c, y0, u, out, w.width, w.height, w.arch, -back);
                        pieces.push((Role::Glass, prism(&g, math3::scale(out, -0.02))));
                    }
                }
            }
        }
        // Ruin: notches out of the wall tops, seeded (cut after the bevelled openings: broken
        // stone stays sharp).
        let mut notches_cut: Vec<Shape> = Vec::new();
        if self.ruin > 0.0 {
            for side in 0..n {
                let (a, b) = (plan[side], plan[(side + 1) % n]);
                let len = ((b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1])).sqrt();
                let u = [(b[0] - a[0]) / len.max(1e-6), 0.0, (b[1] - a[1]) / len.max(1e-6)];
                let out = [u[2], 0.0, -u[0]];
                let notches = 1 + (len * self.ruin * 0.6) as usize;
                for _ in 0..notches.min(24) {
                    let t = rng.f32() * len;
                    let wdt = 0.4 + rng.f32() * len * 0.3 * self.ruin;
                    let drop = height * self.ruin * (0.2 + 0.6 * rng.f32());
                    let c = [a[0] + u[0] * t, height - drop * 0.5 + 0.3, a[1] + u[2] * t];
                    let mut notch = oriented_box([c[0], c[1], c[2]], u, out, [wdt * 0.5, drop * 0.5 + 0.3, self.wall + 0.5]);
                    let tilt = (rng.f32() - 0.5) * 30.0;
                    let pivot = Affine::translate(c).then_after(&Affine::rotate_about_deg(out, tilt as f64).then_after(&Affine::translate([-c[0], -c[1], -c[2]])));
                    notch.transform(&pivot);
                    notches_cut.push(notch);
                }
            }
        }
        // Every cutter a child of the difference: overlapping ones are united, disjoint groups
        // cut in one pass.
        kids.extend(cutters.into_iter().map(CsgTree::Leaf));
        let wall_shape = if kids.len() > 1 || !notches_cut.is_empty() {
            let tree = CsgTree::Op(CsgOp::Difference, kids);
            let bevel = (self.bevel > 0.0).then_some(Bevel { width: self.bevel, segments: 1 });
            let (mut s, st, bs) = csg::evaluate_bevelled(&tree, bevel.as_ref(), budget)?;
            stats.bevel = bs;
            if !notches_cut.is_empty() {
                let mut kids = vec![CsgTree::Leaf(s)];
                kids.extend(notches_cut.into_iter().map(CsgTree::Leaf));
                s = csg::evaluate(&CsgTree::Op(CsgOp::Difference, kids), budget)?.0;
            }
            stats.csg_work = st.work.max(budget.work);
            s
        } else {
            match kids.pop() {
                Some(CsgTree::Leaf(s)) => s,
                _ => unreachable!("one leaf"),
            }
        };
        pieces.insert(0, (Role::Walls, wall_shape));
        // Courses: plinth, bands at the floor lines, cornice (trim).
        let ring = |out_by: f32, y0: f32, y1: f32, budget: &mut csg::Budget| -> Result<Shape, BuildingError> {
            let outer = offset(&plan, out_by);
            if hollow {
                let inner = offset(&plan, -self.wall + 0.01);
                let tree = CsgTree::Op(CsgOp::Difference, vec![CsgTree::Leaf(slab(&outer, y0, y1)?), CsgTree::Leaf(slab(&inner, y0 - 0.1, y1 + 0.1)?)]);
                Ok(csg::evaluate(&tree, budget)?.0)
            } else {
                slab(&outer, y0, y1)
            }
        };
        if let Some((h, o)) = self.plinth {
            pieces.push((Role::Trim, ring(o, 0.0, h, budget)?));
        }
        if self.ruin <= 0.0 {
            if let Some((h, o)) = self.band {
                for f in 1..self.floors {
                    let y = base + f as f32 * self.floor_height;
                    pieces.push((Role::Trim, ring(o, y - h * 0.5, y + h * 0.5, budget)?));
                }
            }
            if let Some((h, o)) = self.cornice {
                pieces.push((Role::Trim, ring(o, height - h, height, budget)?));
            }
            pieces.extend(self.roof_pieces(&plan, height, budget)?);
        }
        Ok((pieces, stats))
    }

    fn roof_pieces(&self, plan: &[V2], top: f32, budget: &mut csg::Budget) -> Result<Vec<(Role, Shape)>, BuildingError> {
        let r = &self.roof;
        let mut out = Vec::new();
        let tan = crate::dmath::tan(r.pitch_deg * std::f32::consts::PI / 180.0);
        match r.kind {
            RoofKind::None => {}
            RoofKind::Flat => {
                let t = r.thickness.max(0.05);
                out.push((Role::Roof, slab(&offset(plan, r.overhang), top, top + t)?));
                if r.parapet > 0.0 {
                    let outer = offset(plan, r.overhang);
                    let inner = offset(plan, r.overhang - self.wall.min(0.4));
                    let tree = CsgTree::Op(CsgOp::Difference, vec![CsgTree::Leaf(slab(&outer, top + t - 0.01, top + t + r.parapet)?), CsgTree::Leaf(slab(&inner, top + t - 0.1, top + t + r.parapet + 0.1)?)]);
                    out.push((Role::Walls, csg::evaluate(&tree, budget)?.0));
                }
            }
            RoofKind::Pyramid => {
                if !is_convex(plan) {
                    return Err(BuildingError::bad("roof.kind", "a pyramid roof needs a convex plan"));
                }
                let eave = offset(plan, r.overhang);
                let c = poly::centroid(&eave);
                let inr = (0..eave.len())
                    .map(|i| {
                        let (a, b) = (eave[i], eave[(i + 1) % eave.len()]);
                        let e = [b[0] - a[0], b[1] - a[1]];
                        let l = (e[0] * e[0] + e[1] * e[1]).sqrt().max(1e-6);
                        ((c[0] - a[0]) * e[1] - (c[1] - a[1]) * e[0]).abs() / l
                    })
                    .fold(f32::MAX, f32::min);
                let apex = [c[0], top + tan * inr, c[1]];
                let mut s = Shape::default();
                for q in &eave {
                    s.positions.push([q[0], top, q[1]]);
                }
                s.positions.push(apex);
                let m = eave.len() as u32;
                // The eave ring is counter-clockwise in (x, z): (i, apex, i + 1) faces out, and
                // its triangulation (counter-clockwise in x, z) faces down.
                for i in 0..m {
                    s.tris.push([i, m, (i + 1) % m]);
                }
                let flat = poly::triangulate(&eave).ok_or_else(|| BuildingError::bad("plan", "the roof's plan doesn't triangulate"))?;
                s.tris.extend(flat);
                s.colors = vec![[1.0; 4]; s.positions.len()];
                s.tags = vec![TAG_FACE; s.tris.len()];
                out.push((Role::Roof, s));
            }
            RoofKind::Gable | RoofKind::Hip | RoofKind::Shed => {
                let (c, u, half) = rectangle(plan).ok_or_else(|| BuildingError::bad("roof.kind", "gable, hip and shed roofs need a rectangular plan (4 corners at right angles)"))?;
                // Ridge along the longer side.
                let (u, ha, hb) = if half[0] >= half[1] { (u, half[0], half[1]) } else { ([-u[1], u[0]], half[1], half[0]) };
                let v = [-u[1], u[0]];
                let (a, b) = (ha + r.overhang, hb + r.overhang);
                let p = |s: f32, t: f32, y: f32| -> V3 { [c[0] + u[0] * s + v[0] * t, y, c[1] + u[1] * s + v[1] * t] };
                let rise = tan * if r.kind == RoofKind::Shed { 2.0 * b } else { b };
                let along = |s: f32| [u[0] * 2.0 * s, 0.0, u[1] * 2.0 * s];
                match r.kind {
                    RoofKind::Gable => {
                        out.push((Role::Roof, prism(&[p(-a, -b, top), p(-a, b, top), p(-a, 0.0, top + rise)], along(a))));
                        let gr = tan * hb;
                        out.push((Role::Walls, prism(&[p(-ha, -hb, top - 0.01), p(-ha, hb, top - 0.01), p(-ha, 0.0, top + gr)], along(ha))));
                    }
                    RoofKind::Shed => {
                        out.push((Role::Roof, prism(&[p(-a, -b, top), p(-a, b, top), p(-a, -b, top + rise)], along(a))));
                        let gr = tan * 2.0 * hb;
                        out.push((Role::Walls, prism(&[p(-ha, -hb, top - 0.01), p(-ha, hb, top - 0.01), p(-ha, -hb, top + gr)], along(ha))));
                    }
                    _ => {
                        // Hip: the ridge is shorter by the roof's depth at each end.
                        let ridge = (a - b).max(0.0);
                        // Eave corners counter-clockwise in (x, z) as for the pyramid: faces out
                        // run corner, ridge, ..., the bottom counter-clockwise faces down.
                        let mut s = Shape {
                            positions: vec![p(-a, -b, top), p(a, -b, top), p(a, b, top), p(-a, b, top), p(-ridge, 0.0, top + rise), p(ridge, 0.0, top + rise)],
                            tris: vec![[0, 4, 5], [0, 5, 1], [1, 5, 2], [2, 5, 4], [2, 4, 3], [3, 4, 0], [0, 1, 2], [0, 2, 3]],
                            ..Shape::default()
                        };
                        if ridge <= 1e-4 {
                            s.positions.truncate(5);
                            s.tris = vec![[0, 4, 1], [1, 4, 2], [2, 4, 3], [3, 4, 0], [0, 1, 2], [0, 2, 3]];
                        }
                        s.colors = vec![[1.0; 4]; s.positions.len()];
                        s.tags = vec![TAG_FACE; s.tris.len()];
                        out.push((Role::Roof, s));
                    }
                }
            }
        }
        Ok(out)
    }
}

fn is_convex(p: &[V2]) -> bool {
    let n = p.len();
    (0..n).all(|i| {
        let (a, b, c) = (p[i], p[(i + 1) % n], p[(i + 2) % n]);
        (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]) >= -1e-6
    })
}

/// A rectangular plan's centre, unit direction of its first side, and half lengths (along that
/// side, across it).
fn rectangle(p: &[V2]) -> Option<(V2, V2, V2)> {
    if p.len() != 4 {
        return None;
    }
    for i in 0..4 {
        let (a, b, c) = (p[i], p[(i + 1) % 4], p[(i + 2) % 4]);
        let (e1, e2) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
        let l = ((e1[0] * e1[0] + e1[1] * e1[1]) * (e2[0] * e2[0] + e2[1] * e2[1])).sqrt();
        if (e1[0] * e2[0] + e1[1] * e2[1]).abs() > 0.02 * l {
            return None;
        }
    }
    let c = [(p[0][0] + p[2][0]) * 0.5, (p[0][1] + p[2][1]) * 0.5];
    let e = [p[1][0] - p[0][0], p[1][1] - p[0][1]];
    let le = (e[0] * e[0] + e[1] * e[1]).sqrt();
    let f = [p[2][0] - p[1][0], p[2][1] - p[1][1]];
    let lf = (f[0] * f[0] + f[1] * f[1]).sqrt();
    Some((c, [e[0] / le, e[1] / le], [le * 0.5, lf * 0.5]))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn spec() -> BuildingSpec {
        BuildingSpec {
            plan: vec![[-4.0, -3.0], [4.0, -3.0], [4.0, 3.0], [-4.0, 3.0]],
            floors: 2,
            floor_height: 3.0,
            wall: 0.4,
            hollow: false,
            plinth: Some((0.4, 0.08)),
            band: Some((0.15, 0.06)),
            cornice: Some((0.25, 0.12)),
            windows: Some(Windows { width: 0.9, height: 1.4, sill: 0.9, spacing: 2.2, margin: 0.8, depth: 0.25, arch: true, trim: 0.08, glass: true, floors: (0, 1) }),
            doors: vec![Door { side: 0, at: 0.5, width: 1.2, height: 2.3, arch: true }],
            roof: Roof { kind: RoofKind::Gable, pitch_deg: 35.0, overhang: 0.3, thickness: 0.2, parapet: 0.0 },
            ruin: 0.0,
            bevel: 0.03,
            seed: 5,
        }
    }

    fn closed(s: &Shape) -> bool {
        crate::csg::tests::unbalanced_edges(s) == 0
    }


    #[test]
    fn a_house_with_windows_door_and_gable() {
        let mut b = csg::Budget::default();
        let (pieces, st) = spec().build(&mut b).unwrap();
        assert!(st.windows >= 10 && st.doors == 1, "{st:?}");
        let roles: Vec<Role> = pieces.iter().map(|p| p.0).collect();
        for r in [Role::Walls, Role::Roof, Role::Trim, Role::Door, Role::Glass] {
            assert!(roles.contains(&r), "{r:?} missing");
        }
        for (r, s) in &pieces {
            assert!(closed(s), "{r:?} not closed");
            assert!(s.signed_volume() > 0.0, "{r:?} inside out");
            s.finalize(30.0, "m").validate().unwrap();
        }
        // The walls have recesses: less volume than the solid block.
        let walls = &pieces[0].1;
        assert!(walls.signed_volume() < 8.0 * 6.0 * 6.4);
        // Deterministic.
        let mut b2 = csg::Budget::default();
        assert_eq!(spec().build(&mut b2).unwrap().0, pieces);
    }

    #[test]
    fn roofs_and_ruins() {
        for kind in [RoofKind::Flat, RoofKind::Hip, RoofKind::Shed, RoofKind::Pyramid, RoofKind::None] {
            let mut s = spec();
            s.roof.kind = kind;
            s.roof.parapet = 0.6;
            s.windows = None;
            let mut b = csg::Budget::default();
            let (pieces, _) = s.build(&mut b).unwrap_or_else(|e| panic!("{kind:?}: {e:?}"));
            for (r, sh) in &pieces {
                assert!(closed(sh) && sh.signed_volume() > 0.0, "{kind:?} {r:?}");
            }
        }
        let mut s = spec();
        s.ruin = 0.6;
        // Two bevelled, windowed floors in ruins are about 2 s of booleans: over the budget.
        let mut b = csg::Budget::default();
        assert!(s.build(&mut b).unwrap_err().limit);
        s.floors = 1;
        let mut b = csg::Budget::default();
        let (pieces, _) = s.build(&mut b).unwrap();
        assert!(pieces.iter().all(|p| p.0 != Role::Roof), "a ruin has no roof");
        assert!(closed(&pieces[0].1));
        // An L-shaped plan with a flat roof.
        let mut s = spec();
        s.plan = vec![[0.0, 0.0], [8.0, 0.0], [8.0, 4.0], [4.0, 4.0], [4.0, 8.0], [0.0, 8.0]];
        s.roof.kind = RoofKind::Flat;
        let mut b = csg::Budget::default();
        assert!(s.build(&mut b).is_ok());
        s.roof.kind = RoofKind::Gable;
        assert_eq!(s.build(&mut csg::Budget::default()).unwrap_err().field, "roof.kind");
    }

    #[test]
    fn bad_specs() {
        let mut s = spec();
        s.doors[0].side = 9;
        assert_eq!(s.check().unwrap_err().field, "doors");
        let mut s = spec();
        s.floors = 0;
        assert_eq!(s.check().unwrap_err().field, "floors");
        let mut s = spec();
        s.plan = vec![[0.0, 0.0], [1.0, 1.0], [0.0, 1.0], [1.0, 0.0]];
        assert_eq!(s.check().unwrap_err().field, "plan");
    }
}

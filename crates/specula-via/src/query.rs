//! Queries on a [`NavMesh`]: nearest polygon, paths (A* over polygons, then the funnel), moving
//! along the surface, random points.
//!
//! Determinism: costs are IEEE `f32` sums in a fixed order; the open list orders by the cost's
//! bits and then the polygon ref, so equal costs never depend on memory or insertion order.

use crate::mesh::{NavMesh, Poly, PolyRef};
use specula_core::math::Vec3;
use specula_core::rng::Rng;
use specula_core::Json;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

/// Default and largest number of polygons one path search expands.
pub const MAX_NODES: (usize, usize) = (4096, 65536);
/// Iterations of one surface move (each crosses one polygon).
const MOVE_STEPS: usize = 32;
/// Heuristic scale (slightly below 1: never overestimates the 3D length of a step).
const H_SCALE: f32 = 0.999;

/// How a path search ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathStatus {
    /// Reaches the goal.
    Complete,
    /// The goal is unreachable: the path goes to the polygon nearest it.
    Unreachable,
    /// The search hit its node budget before the goal.
    Budget,
}

impl PathStatus {
    pub fn name(self) -> &'static str {
        match self {
            PathStatus::Complete => "complete",
            PathStatus::Unreachable => "unreachable",
            PathStatus::Budget => "budget",
        }
    }
}

/// A path between two points.
#[derive(Clone, Debug, PartialEq)]
pub struct PathResult {
    /// The polygons from start to end.
    pub polys: Vec<PolyRef>,
    /// The string-pulled path: the start, the corners, the end (on the navmesh).
    pub corners: Vec<Vec3>,
    pub status: PathStatus,
    /// Polygons expanded.
    pub visited: usize,
    pub length: f32,
}

impl PathResult {
    pub fn complete(&self) -> bool {
        self.status == PathStatus::Complete
    }
}

fn dist(a: Vec3, b: Vec3) -> f32 {
    (a - b).length()
}

fn dist_xz(a: Vec3, b: Vec3) -> f32 {
    let (dx, dz) = (a.x - b.x, a.z - b.z);
    (dx * dx + dz * dz).sqrt()
}

/// `cross(b - a, c - a)` in the X/Z plane: negative when `c` is left of `a → b` seen from above
/// (+Y), with +X east and +Z south.
fn tri(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    let (ux, uz, vx, vz) = (b.x - a.x, b.z - a.z, c.x - a.x, c.z - a.z);
    ux * vz - uz * vx
}

fn same_xz(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).abs() < 1.0e-6 && (a.z - b.z).abs() < 1.0e-6
}

/// The closest point to `p` on segment `a`-`b` in X/Z (heights interpolated).
fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let (dx, dz) = (b.x - a.x, b.z - a.z);
    let len2 = dx * dx + dz * dz;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.z - a.z) * dz) / len2).clamp(0.0, 1.0) } else { 0.0 };
    Vec3::new(a.x + dx * t, a.y + (b.y - a.y) * t, a.z + dz * t)
}

impl NavMesh {
    /// The point of polygon `p` closest to `at` (clamped in X/Z, on its surface).
    pub fn closest_on_poly(&self, p: &Poly, at: Vec3) -> Vec3 {
        let r = self.rect(p);
        let (x, z) = (at.x.clamp(r[0], r[2]), at.z.clamp(r[1], r[3]));
        Vec3::new(x, self.height(p, x, z), z)
    }

    /// Whether `at` lies inside polygon `p` in X/Z (edges included).
    pub fn contains_xz(&self, p: &Poly, at: Vec3) -> bool {
        let r = self.rect(p);
        at.x >= r[0] && at.x <= r[2] && at.z >= r[1] && at.z <= r[3]
    }

    /// The nearest polygon to `at` within `extent` (half sizes: X/Z and Y), and the closest point
    /// on it. Polygons the point is over (in X/Z) win by vertical distance; otherwise by 3D
    /// distance; ties by ref.
    pub fn nearest(&self, at: Vec3, extent: Vec3) -> Option<(PolyRef, Vec3)> {
        let lo = Vec3::new(at.x - extent.x, at.y - extent.y, at.z - extent.z);
        let hi = Vec3::new(at.x + extent.x, at.y + extent.y, at.z + extent.z);
        let mut best: Option<(f32, PolyRef, Vec3)> = None;
        let (cs, t) = (self.config().cell_size, self.config().tile_cells as f32);
        let o = self.config().bounds_min;
        let (tx_n, tz_n) = self.tile_count();
        let tiles = |lo: f32, hi: f32, o: f32, n: u32| -> Option<(u32, u32)> {
            let a = specula_core::dmath::floor((lo - o) / (t * cs));
            let b = specula_core::dmath::floor((hi - o) / (t * cs));
            if !(a.is_finite() && b.is_finite()) || b < 0.0 || a >= n as f32 {
                return None;
            }
            Some((a.max(0.0) as u32, (b as u32).min(n - 1)))
        };
        let (Some((x0, x1)), Some((z0, z1))) = (tiles(lo.x, hi.x, o.x, tx_n), tiles(lo.z, hi.z, o.z, tz_n)) else {
            return None;
        };
        for tz in z0..=z1 {
            for tx in x0..=x1 {
                let index = self.tile_index(tx, tz);
                let Some(tile) = self.tile(index) else { continue };
                for (i, p) in tile.polys.iter().enumerate() {
                    let r = self.rect(p);
                    if r[2] < lo.x || r[0] > hi.x || r[3] < lo.z || r[1] > hi.z {
                        continue;
                    }
                    let c = self.closest_on_poly(p, at);
                    if (c.y - at.y).abs() > extent.y {
                        continue;
                    }
                    // Over the polygon: vertical distance only (a floor below beats a wall's top beside).
                    let d = if self.contains_xz(p, at) { (c.y - at.y).abs() } else { dist(c, at) + extent.y };
                    let key = PolyRef::new(index, i);
                    if best.is_none_or(|(bd, bk, _)| d < bd || (d == bd && key < bk)) {
                        best = Some((d, key, c));
                    }
                }
            }
        }
        best.map(|(_, k, c)| (k, c))
    }

    /// A path from `start` to `end` (each snapped to the nearest polygon within `extent`):
    /// A* over polygons, then the funnel. `None` when `start` is off the navmesh.
    pub fn find_path(&self, start: Vec3, end: Vec3, extent: Vec3, max_nodes: usize) -> Option<PathResult> {
        let (sp, s) = self.nearest(start, extent)?;
        let goal = self.nearest(end, extent);
        let target = goal.map_or(end, |(_, e)| e);
        let (polys, status, visited) = self.search(sp, s, goal.map(|(p, _)| p), target, max_nodes.clamp(1, MAX_NODES.1));
        let last = *polys.last().expect("the start is in the path");
        let end_point = match status {
            PathStatus::Complete => target,
            _ => self.poly(last).map_or(s, |p| self.closest_on_poly(p, target)),
        };
        let corners = self.straighten(s, end_point, &polys, usize::MAX);
        let length = corners.windows(2).map(|w| dist(w[0], w[1])).sum();
        Some(PathResult { polys, corners, status, visited, length })
    }

    /// A* from polygon `sp` (at `s`) toward `goal` (at `target`); a goal of `None` is unreachable
    /// by definition, so the search returns the polygon closest to `target`.
    pub fn search(&self, sp: PolyRef, s: Vec3, goal: Option<PolyRef>, target: Vec3, max_nodes: usize) -> (Vec<PolyRef>, PathStatus, usize) {
        struct Node {
            g: f32,
            pos: Vec3,
            parent: Option<PolyRef>,
            closed: bool,
        }
        let mut nodes: BTreeMap<PolyRef, Node> = BTreeMap::new();
        let mut open: BinaryHeap<Reverse<(u32, PolyRef)>> = BinaryHeap::new();
        nodes.insert(sp, Node { g: 0.0, pos: s, parent: None, closed: false });
        open.push(Reverse(((dist(s, target) * H_SCALE).to_bits(), sp)));
        let mut best = (self.poly(sp).map_or(f32::INFINITY, |p| dist(self.closest_on_poly(p, target), target)), sp);
        let mut visited = 0usize;
        let mut status = PathStatus::Unreachable;
        let mut found = None;
        while let Some(Reverse((_, cur))) = open.pop() {
            let (g, pos) = {
                let n = nodes.get_mut(&cur).expect("queued nodes exist");
                if n.closed {
                    continue;
                }
                n.closed = true;
                (n.g, n.pos)
            };
            if Some(cur) == goal {
                status = PathStatus::Complete;
                found = Some(cur);
                break;
            }
            visited += 1;
            if visited > max_nodes {
                status = PathStatus::Budget;
                break;
            }
            let Some(p) = self.poly(cur) else { continue };
            for l in &p.links {
                let Some(q) = self.poly(l.to) else { continue };
                if nodes.get(&l.to).is_some_and(|n| n.closed) {
                    continue;
                }
                let (a, b) = self.portal(p, l);
                let np = closest_on_segment(pos, a, b);
                let mut ng = g + dist(pos, np);
                let h;
                if Some(l.to) == goal {
                    ng += dist(np, target);
                    h = 0.0;
                } else {
                    h = dist(np, target) * H_SCALE;
                    let to_target = dist(self.closest_on_poly(q, target), target);
                    if to_target < best.0 || (to_target == best.0 && l.to < best.1) {
                        best = (to_target, l.to);
                    }
                }
                let better = nodes.get(&l.to).is_none_or(|n| ng < n.g);
                if better {
                    nodes.insert(l.to, Node { g: ng, pos: np, parent: Some(cur), closed: false });
                    open.push(Reverse(((ng + h).to_bits(), l.to)));
                }
            }
        }
        let end = found.unwrap_or(best.1);
        let mut polys = vec![end];
        let mut at = end;
        while let Some(parent) = nodes.get(&at).and_then(|n| n.parent) {
            polys.push(parent);
            at = parent;
            if polys.len() > nodes.len() {
                break;
            }
        }
        polys.reverse();
        (polys, status, visited)
    }

    /// The portals along a polygon corridor, as (left, right) end points seen in the direction
    /// of travel.
    fn portals(&self, polys: &[PolyRef]) -> Vec<(Vec3, Vec3)> {
        let mut out = Vec::with_capacity(polys.len());
        for w in polys.windows(2) {
            let Some(p) = self.poly(w[0]) else { break };
            let Some(l) = p.links.iter().find(|l| l.to == w[1]) else { break };
            let (a, b) = self.portal(p, l);
            // Outward normal of the side: -x, +z, +x, -z.
            let (nx, nz) = [(-1.0f32, 0.0f32), (0.0, 1.0), (1.0, 0.0), (0.0, -1.0)][l.side as usize & 3];
            // `a` is left when cross(normal, a - b) < 0.
            let c = nx * (a.z - b.z) - nz * (a.x - b.x);
            out.push(if c < 0.0 { (a, b) } else { (b, a) });
        }
        out
    }

    /// String pulling (the simple stupid funnel algorithm) from `start` through the corridor to
    /// `end`: the start, then each corner, then the end. Stops after `max_corners` corners (the
    /// end counts as one).
    pub fn straighten(&self, start: Vec3, end: Vec3, polys: &[PolyRef], max_corners: usize) -> Vec<Vec3> {
        let mut portals = vec![(start, start)];
        portals.extend(self.portals(polys));
        portals.push((end, end));
        let mut out = vec![start];
        let (mut apex, mut left, mut right) = (start, start, start);
        let (mut left_i, mut right_i) = (0usize, 0usize);
        let mut apex_i;
        let mut i = 1;
        let mut guard = 0usize;
        while i < portals.len() {
            guard += 1;
            if guard > portals.len() * 4 + 16 || out.len() > max_corners {
                break;
            }
            let (l, r) = portals[i];
            // Tighten the right side.
            if tri(apex, right, r) <= 0.0 {
                if same_xz(apex, right) || tri(apex, left, r) >= 0.0 {
                    right = r;
                    right_i = i;
                } else {
                    // The right side crossed the left: the left point is a corner.
                    if !same_xz(*out.last().expect("start"), left) {
                        out.push(left);
                    }
                    apex = left;
                    apex_i = left_i;
                    left = apex;
                    right = apex;
                    left_i = apex_i;
                    right_i = apex_i;
                    i = apex_i + 1;
                    continue;
                }
            }
            // Tighten the left side.
            if tri(apex, left, l) >= 0.0 {
                if same_xz(apex, left) || tri(apex, right, l) <= 0.0 {
                    left = l;
                    left_i = i;
                } else {
                    if !same_xz(*out.last().expect("start"), right) {
                        out.push(right);
                    }
                    apex = right;
                    apex_i = right_i;
                    left = apex;
                    right = apex;
                    left_i = apex_i;
                    right_i = apex_i;
                    i = apex_i + 1;
                    continue;
                }
            }
            i += 1;
        }
        if out.len() <= max_corners && !same_xz(*out.last().expect("start"), end) {
            out.push(end);
        } else if out.len() == 1 {
            out.push(end);
        }
        out
    }

    /// Moves from `from` (on polygon `start`) toward `to` across the surface: through portals
    /// where there are neighbours, sliding along edges where there are none. Returns the polygon
    /// it ends on and the point (on the surface).
    pub fn move_along(&self, start: PolyRef, from: Vec3, to: Vec3) -> (PolyRef, Vec3) {
        let Some(mut p) = self.poly(start) else { return (start, from) };
        let mut cur = start;
        let mut pos = from;
        for _ in 0..MOVE_STEPS {
            if self.contains_xz(p, to) {
                return (cur, Vec3::new(to.x, self.height(p, to.x, to.z), to.z));
            }
            let r = self.rect(p);
            // Where the segment leaves the rectangle (the first side it crosses).
            let (dx, dz) = (to.x - pos.x, to.z - pos.z);
            let mut t_exit = f32::INFINITY;
            let mut side = 4usize;
            let mut consider = |t: f32, s: usize| {
                if t >= 0.0 && t < t_exit {
                    t_exit = t;
                    side = s;
                }
            };
            if dx < 0.0 {
                consider((r[0] - pos.x) / dx, 0);
            }
            if dx > 0.0 {
                consider((r[2] - pos.x) / dx, 2);
            }
            if dz > 0.0 {
                consider((r[3] - pos.z) / dz, 1);
            }
            if dz < 0.0 {
                consider((r[1] - pos.z) / dz, 3);
            }
            if side == 4 {
                break;
            }
            let t = t_exit.clamp(0.0, 1.0);
            let ex = (pos.x + dx * t).clamp(r[0], r[2]);
            let ez = (pos.z + dz * t).clamp(r[1], r[3]);
            let exit = Vec3::new(ex, self.height(p, ex, ez), ez);
            let (cs, o) = (self.config().cell_size, self.config().bounds_min);
            let along = if side == 0 || side == 2 { ez } else { ex };
            let base = if side == 0 || side == 2 { o.z } else { o.x };
            let next = p.links.iter().find(|l| {
                usize::from(l.side) == side && along >= base + l.lo as f32 * cs && along <= base + (l.hi + 1) as f32 * cs
            });
            match next.and_then(|l| self.poly(l.to).map(|q| (l.to, q))) {
                Some((nr, q)) => {
                    cur = nr;
                    p = q;
                    // Step just over the edge so the next rectangle contains the point.
                    pos = exit;
                }
                None => {
                    // A wall: slide (keep the motion along the edge, clamp the rest).
                    let x = to.x.clamp(r[0], r[2]);
                    let z = to.z.clamp(r[1], r[3]);
                    return (cur, Vec3::new(x, self.height(p, x, z), z));
                }
            }
        }
        (cur, Vec3::new(pos.x, self.height(p, pos.x, pos.z), pos.z))
    }

    /// A random point on the navmesh: a polygon chosen by area, then a point inside it.
    pub fn random_point(&self, rng: &mut Rng) -> Option<(PolyRef, Vec3)> {
        let total: f32 = self.tiles().map(|(_, t)| t.polys.iter().map(|p| area(self, p)).sum::<f32>()).sum();
        if total <= 0.0 {
            return None;
        }
        let mut pick = rng.f32() * total;
        let mut chosen = None;
        'tiles: for (i, t) in self.tiles() {
            for (k, p) in t.polys.iter().enumerate() {
                chosen = Some((PolyRef::new(i, k), p));
                pick -= area(self, p);
                if pick <= 0.0 {
                    break 'tiles;
                }
            }
        }
        let (r, p) = chosen?;
        let rect = self.rect(p);
        let x = rect[0] + (rect[2] - rect[0]) * rng.f32();
        let z = rect[1] + (rect[3] - rect[1]) * rng.f32();
        Some((r, Vec3::new(x, self.height(p, x, z), z)))
    }

    /// Length of a corridor's remaining path from `from` to `end` (string-pulled), metres.
    pub fn corridor_length(&self, from: Vec3, end: Vec3, polys: &[PolyRef]) -> f32 {
        self.straighten(from, end, polys, usize::MAX).windows(2).map(|w| dist(w[0], w[1])).sum()
    }
}

fn area(mesh: &NavMesh, p: &Poly) -> f32 {
    let r = mesh.rect(p);
    (r[2] - r[0]) * (r[3] - r[1])
}

/// A path as JSON (corners rounded to a millimetre).
pub fn path_json(p: &PathResult, max_corners: usize) -> Json {
    let r = |v: f32| Json::from((f64::from(v) * 1000.0).round() / 1000.0);
    let corners: Vec<Json> = p.corners.iter().take(max_corners).map(|c| Json::arr([r(c.x), r(c.y), r(c.z)])).collect();
    Json::obj([
        ("status", Json::from(p.status.name())),
        ("complete", Json::from(p.complete())),
        ("length", r(p.length)),
        ("corners", Json::Arr(corners)),
        ("corner_count", Json::from(p.corners.len())),
        ("polygons", Json::from(p.polys.len())),
        ("visited", Json::from(p.visited)),
    ])
}

/// Distance in X/Z (for arrival tests).
pub fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    dist_xz(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::tests::{build_all, quad, room_config, scene};
    use crate::Source;

    fn room_with_wall() -> NavMesh {
        let mut mesh = NavMesh::new(room_config()).unwrap();
        // A wall across the middle with a gap at z > 6.
        build_all(&mut mesh, &scene([-10.0, -10.0, 10.0, 10.0], &[(Vec3::new(-0.25, 0.0, -10.0), Vec3::new(0.25, 2.0, 6.0))]));
        mesh
    }

    #[test]
    fn path_goes_around_the_wall() {
        let mesh = room_with_wall();
        let ext = Vec3::new(1.0, 2.0, 1.0);
        let p = mesh.find_path(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0), ext, 4096).unwrap();
        assert!(p.complete(), "{p:?}");
        // Around the wall's end at z = 6 (+ the 0.3 m radius): the path must pass z >= 6.3.
        assert!(p.corners.iter().any(|c| c.z >= 6.25), "corners {:?}", p.corners);
        // String pulling: few corners, length a little over the detour.
        assert!(p.corners.len() <= 6, "corners {:?}", p.corners);
        let direct = 2.0 * ((5.0f32).powi(2) + (6.3f32).powi(2)).sqrt();
        assert!(p.length >= direct - 0.05 && p.length < direct + 1.0, "length {} vs {}", p.length, direct);
        // Every corner is on the navmesh.
        for c in &p.corners {
            assert!(mesh.nearest(*c, Vec3::new(0.01, 0.1, 0.01)).is_some(), "corner {c:?} off the mesh");
        }
    }

    #[test]
    fn unreachable_goal_gives_partial_path() {
        let mut mesh = NavMesh::new(room_config()).unwrap();
        // A closed box around the goal.
        let walls = [
            (Vec3::new(3.0, 0.0, -3.0), Vec3::new(3.5, 2.0, 3.0)),
            (Vec3::new(8.5, 0.0, -3.0), Vec3::new(9.0, 2.0, 3.0)),
            (Vec3::new(3.0, 0.0, -3.5), Vec3::new(9.0, 2.0, -3.0)),
            (Vec3::new(3.0, 0.0, 3.0), Vec3::new(9.0, 2.0, 3.5)),
        ];
        build_all(&mut mesh, &scene([-10.0, -10.0, 10.0, 10.0], &walls));
        let p = mesh.find_path(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(6.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 1.0), 4096).unwrap();
        assert_eq!(p.status, PathStatus::Unreachable);
        let end = *p.corners.last().unwrap();
        assert!(end.x < 3.0 || end.x > 9.0 || end.z.abs() > 3.5, "ends outside the box: {end:?}");
        // At the reachable point nearest the goal: the box's east side (3.35 m) beats its west (3.4 m).
        assert!((end - Vec3::new(6.0, 0.0, 0.0)).length() < 3.4, "{end:?}");
    }

    #[test]
    fn stairs_connect_and_cliffs_do_not() {
        let mut mesh = NavMesh::new(room_config()).unwrap();
        // Five 0.2 m steps up to a 1 m platform at x in 2..6.
        let mut boxes: Vec<(Vec3, Vec3)> = (0..5)
            .map(|i| {
                let x = -1.0 + i as f32 * 0.6;
                (Vec3::new(x, 0.0, -1.0), Vec3::new(x + 0.6, 0.2 * (i + 1) as f32, 1.0))
            })
            .collect();
        boxes.push((Vec3::new(2.0, 0.0, -1.0), Vec3::new(6.0, 1.0, 1.0)));
        // A 1 m cliff: a platform with no steps.
        boxes.push((Vec3::new(-8.0, 0.0, 4.0), Vec3::new(-4.0, 1.0, 8.0)));
        build_all(&mut mesh, &scene([-10.0, -10.0, 10.0, 10.0], &boxes));
        let ext = Vec3::new(0.5, 0.5, 0.5);
        let up = mesh.find_path(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(4.0, 1.0, 0.0), ext, 4096).unwrap();
        assert!(up.complete(), "stairs connect: {up:?}");
        let cliff = mesh.find_path(Vec3::new(-6.0, 0.0, 0.0), Vec3::new(-6.0, 1.0, 6.0), ext, 4096).unwrap();
        assert!(!cliff.complete(), "a 1 m cliff doesn't: {cliff:?}");
    }

    #[test]
    fn slopes_follow_the_limit() {
        let ramp = |deg: f32| {
            let mut mesh = NavMesh::new(room_config()).unwrap();
            let mut tris = quad(-10.0, -10.0, 0.0, 10.0, 0.0);
            let rise = 5.0 * specula_core::dmath::tan(deg.to_radians());
            let (a, b, c, d) = (Vec3::new(0.0, 0.0, -2.0), Vec3::new(5.0, rise, -2.0), Vec3::new(5.0, rise, 2.0), Vec3::new(0.0, 0.0, 2.0));
            tris.push(crate::Triangle([a, c, b]));
            tris.push(crate::Triangle([a, d, c]));
            build_all(&mut mesh, &[Source::new(tris, false)]);
            mesh.find_path(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(4.0, rise * 0.8, 0.0), Vec3::new(0.5, 1.0, 0.5), 4096).unwrap()
        };
        assert!(ramp(20.0).complete(), "a 20 degree ramp is walkable");
        let steep = ramp(30.0);
        assert!(steep.complete(), "30 degrees is under the 45 degree limit");
        assert!(!ramp(55.0).complete(), "55 degrees is not");
    }

    #[test]
    fn move_along_slides_on_walls() {
        let mesh = room_with_wall();
        let ext = Vec3::new(1.0, 2.0, 1.0);
        let (start, p) = mesh.nearest(Vec3::new(-2.0, 0.0, 0.0), ext).unwrap();
        let (_, end) = mesh.move_along(start, p, Vec3::new(2.0, 0.0, 1.0));
        // Stops at the wall's eroded face (x = -0.25 - 0.3, plus up to a column: a face on a
        // column line counts for the column before it too) and slid toward z = 1.
        assert!(end.x <= -0.5 && end.x >= -0.75, "{end:?}");
        assert!((end.z - 1.0).abs() < 1.0e-4, "{end:?}");
        let (_, free) = mesh.move_along(start, p, Vec3::new(-4.0, 0.0, -3.0));
        assert!((free.x + 4.0).abs() < 1.0e-4 && (free.z + 3.0).abs() < 1.0e-4, "{free:?}");
    }

    #[test]
    fn paths_are_deterministic() {
        let a = room_with_wall();
        let b = room_with_wall();
        assert_eq!(a.hash(), b.hash());
        let ext = Vec3::new(1.0, 2.0, 1.0);
        for i in 0..20 {
            let s = Vec3::new(-8.0 + i as f32 * 0.37, 0.0, -7.0 + i as f32 * 0.5);
            let e = Vec3::new(7.0 - i as f32 * 0.21, 0.0, 6.0 - i as f32 * 0.6);
            assert_eq!(a.find_path(s, e, ext, 4096), b.find_path(s, e, ext, 4096));
        }
        let (mut r1, mut r2) = (Rng::new(5, 1), Rng::new(5, 1));
        assert_eq!(a.random_point(&mut r1), b.random_point(&mut r2));
    }
}

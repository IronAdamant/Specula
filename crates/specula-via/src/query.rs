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
use std::collections::BinaryHeap;

/// A small open-addressing map from polygon refs to node slots (a fixed hash: lookups only,
/// never iterated, so nothing depends on its layout).
struct Slots {
    keys: Vec<u32>,
    vals: Vec<u32>,
    len: usize,
}

impl Slots {
    const EMPTY: u32 = u32::MAX;

    fn with_capacity(n: usize) -> Slots {
        let cap = (n * 2).next_power_of_two().max(16);
        Slots { keys: vec![Slots::EMPTY; cap], vals: vec![0; cap], len: 0 }
    }

    fn slot(&self, key: u32) -> usize {
        let mask = self.keys.len() - 1;
        let mut i = (key.wrapping_mul(0x9E37_79B1) >> 7) as usize & mask;
        while self.keys[i] != Slots::EMPTY && self.keys[i] != key {
            i = (i + 1) & mask;
        }
        i
    }

    fn get(&self, key: u32) -> Option<u32> {
        let i = self.slot(key);
        (self.keys[i] == key).then(|| self.vals[i])
    }

    fn insert(&mut self, key: u32, val: u32) {
        if (self.len + 1) * 2 > self.keys.len() {
            let old: Vec<(u32, u32)> = self.keys.iter().zip(&self.vals).filter(|(k, _)| **k != Slots::EMPTY).map(|(k, v)| (*k, *v)).collect();
            let cap = self.keys.len() * 2;
            self.keys = vec![Slots::EMPTY; cap];
            self.vals = vec![0; cap];
            self.len = 0;
            for (k, v) in old {
                self.insert(k, v);
            }
        }
        let i = self.slot(key);
        if self.keys[i] == Slots::EMPTY {
            self.len += 1;
        }
        self.keys[i] = key;
        self.vals[i] = val;
    }
}

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
    /// Off-mesh links on it: (index of the corner where it starts, the link's kind); the next
    /// corner is where it lands.
    pub offmesh: Vec<(usize, crate::LinkKind)>,
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
        self.find_path_filtered(start, end, extent, max_nodes, &crate::Filter::default())
    }

    /// [`NavMesh::find_path`] weighing areas with `filter` (an excluded goal is unreachable).
    pub fn find_path_filtered(&self, start: Vec3, end: Vec3, extent: Vec3, max_nodes: usize, filter: &crate::Filter) -> Option<PathResult> {
        let (sp, s) = self.nearest(start, extent)?;
        let goal = self.nearest(end, extent).filter(|(p, _)| self.poly(*p).is_some_and(|q| filter.cost_of(q.area).is_some()));
        let target = goal.map_or(end, |(_, e)| e);
        let (polys, status, visited) = self.search_filtered(sp, s, goal.map(|(p, _)| p), target, max_nodes.clamp(1, MAX_NODES.1), filter);
        let last = *polys.last().expect("the start is in the path");
        let end_point = match status {
            PathStatus::Complete => target,
            _ => self.poly(last).map_or(s, |p| self.closest_on_poly(p, target)),
        };
        let polys = self.shortcut_filtered(s, end_point, polys, filter);
        let (corners, offmesh) = self.corners_through(s, end_point, &polys);
        let length = corners.windows(2).map(|w| dist(w[0], w[1])).sum();
        Some(PathResult { polys, corners, offmesh, status, visited, length })
    }

    /// The whole string-pulled path through a corridor that may cross off-mesh links: each
    /// stretch on the surface funnelled, each link a straight hop. Returns the corners and, per
    /// link, the index of its take-off corner and its kind.
    pub fn corners_through(&self, start: Vec3, end: Vec3, polys: &[PolyRef]) -> (Vec<Vec3>, Vec<(usize, crate::LinkKind)>) {
        let mut corners = Vec::new();
        let mut links = Vec::new();
        let (mut from, mut rest) = (start, polys);
        while let Some((k, t)) = self.first_traverse(rest) {
            let part = self.straighten(from, t.from_pos, &rest[..=k], usize::MAX);
            let skip = usize::from(!corners.is_empty());
            corners.extend(part.into_iter().skip(skip));
            if !same_xz(*corners.last().expect("start"), t.from_pos) || corners.len() == 1 {
                corners.push(t.from_pos);
            }
            links.push((corners.len() - 1, t.kind));
            corners.push(t.to_pos);
            from = t.to_pos;
            rest = &rest[k + 1..];
        }
        let part = self.straighten(from, end, rest, usize::MAX);
        let skip = usize::from(!corners.is_empty());
        corners.extend(part.into_iter().skip(skip));
        (corners, links)
    }

    /// A* from polygon `sp` (at `s`) toward `goal` (at `target`); a goal of `None` is unreachable
    /// by definition, so the search returns the polygon closest to `target`.
    pub fn search(&self, sp: PolyRef, s: Vec3, goal: Option<PolyRef>, target: Vec3, max_nodes: usize) -> (Vec<PolyRef>, PathStatus, usize) {
        self.search_filtered(sp, s, goal, target, max_nodes, &crate::Filter::default())
    }

    /// [`NavMesh::search`] with area costs and exclusions, across off-mesh links too: crossing a
    /// polygon costs its distance times its area's cost, a link its length times its cost and
    /// its area's.
    pub fn search_filtered(&self, sp: PolyRef, s: Vec3, goal: Option<PolyRef>, target: Vec3, max_nodes: usize, filter: &crate::Filter) -> (Vec<PolyRef>, PathStatus, usize) {
        struct Node {
            poly: PolyRef,
            g: f32,
            pos: Vec3,
            parent: u32,
            closed: bool,
        }
        const NONE: u32 = u32::MAX;
        let mut nodes: Vec<Node> = Vec::with_capacity(256);
        let mut index = Slots::with_capacity(256);
        let mut open: BinaryHeap<Reverse<(u32, PolyRef, u32)>> = BinaryHeap::new();
        nodes.push(Node { poly: sp, g: 0.0, pos: s, parent: NONE, closed: false });
        index.insert(sp.0, 0);
        let h_scale = H_SCALE * filter.min_cost();
        open.push(Reverse(((dist(s, target) * h_scale).to_bits(), sp, 0)));
        // The polygon nearest the target so far (by its entry point), for unreachable goals.
        let mut best = (self.poly(sp).map_or(f32::INFINITY, |p| dist(self.closest_on_poly(p, target), target)), sp, 0u32);
        let mut visited = 0usize;
        let mut status = PathStatus::Unreachable;
        let mut found = None;
        while let Some(Reverse((_, cur, ci))) = open.pop() {
            let (g, pos) = {
                let n = &mut nodes[ci as usize];
                if n.closed {
                    continue;
                }
                n.closed = true;
                (n.g, n.pos)
            };
            if Some(cur) == goal {
                status = PathStatus::Complete;
                found = Some(ci);
                break;
            }
            visited += 1;
            if visited > max_nodes {
                status = PathStatus::Budget;
                break;
            }
            let Some(p) = self.poly(cur) else { continue };
            // (The start polygon may be excluded: an agent standing in it has to leave.)
            let here = filter.cost_of(p.area).unwrap_or(1.0);
            let portals = p.links.iter().map(|l| {
                let (a, b) = self.portal(p, l);
                (l.to, closest_on_segment(pos, a, b), None)
            });
            let hops = self.traverses_from(cur).filter_map(|t| {
                let c = filter.cost_of(t.area)?;
                Some((t.to, t.to_pos, Some((t.from_pos, dist(t.from_pos, t.to_pos) * t.cost.max(0.01) * c))))
            });
            for (to, np, hop) in portals.chain(hops) {
                let Some(next_cost) = self.poly(to).and_then(|q| filter.cost_of(q.area)) else { continue };
                let slot = index.get(to.0);
                if slot.is_some_and(|i| nodes[i as usize].closed) {
                    continue;
                }
                let mut ng = match hop {
                    Some((off, c)) => g + dist(pos, off) * here + c,
                    None => g + dist(pos, np) * here,
                };
                let h;
                if Some(to) == goal {
                    ng += dist(np, target) * next_cost;
                    h = 0.0;
                } else {
                    let d = dist(np, target);
                    h = d * h_scale;
                    if d < best.0 || (d == best.0 && to < best.1) {
                        best = (d, to, NONE);
                    }
                }
                let l_to = to;
                let ni = match slot {
                    Some(i) if ng >= nodes[i as usize].g => continue,
                    Some(i) => {
                        let n = &mut nodes[i as usize];
                        n.g = ng;
                        n.pos = np;
                        n.parent = ci;
                        i
                    }
                    None => {
                        let i = nodes.len() as u32;
                        nodes.push(Node { poly: l_to, g: ng, pos: np, parent: ci, closed: false });
                        index.insert(l_to.0, i);
                        i
                    }
                };
                if best.1 == l_to {
                    best.2 = ni;
                }
                open.push(Reverse(((ng + h).to_bits(), l_to, ni)));
            }
        }
        let end = match found {
            Some(i) => i,
            None => index.get(best.1 .0).unwrap_or(0),
        };
        let mut polys = vec![nodes[end as usize].poly];
        let mut at = nodes[end as usize].parent;
        while at != NONE && polys.len() <= nodes.len() {
            polys.push(nodes[at as usize].poly);
            at = nodes[at as usize].parent;
        }
        polys.reverse();
        (polys, status, visited)
    }

    /// The portal from polygon `a` into its neighbour `b`, as (left, right) end points seen in
    /// the direction of travel (`None` when they aren't neighbours).
    pub fn portal_between(&self, a: PolyRef, b: PolyRef) -> Option<(Vec3, Vec3)> {
        let p = self.poly(a)?;
        let l = p.links.iter().find(|l| l.to == b)?;
        let (x, y) = self.portal(p, l);
        // Outward normal of the side: -x, +z, +x, -z.
        let (nx, nz) = [(-1.0f32, 0.0f32), (0.0, 1.0), (1.0, 0.0), (0.0, -1.0)][l.side as usize & 3];
        // `x` is left when cross(normal, x - y) < 0.
        let c = nx * (x.z - y.z) - nz * (x.x - y.x);
        Some(if c < 0.0 { (x, y) } else { (y, x) })
    }

    /// String pulling (the simple stupid funnel algorithm) from `start` through the corridor to
    /// `end`: the start, then each corner, then the end. Stops after `max_corners` corners (the
    /// end counts as one).
    pub fn straighten(&self, start: Vec3, end: Vec3, polys: &[PolyRef], max_corners: usize) -> Vec<Vec3> {
        // Portals are worked out as the funnel reaches them (steering needs only the first few
        // corners of a long corridor).
        let mut portals = vec![(start, start)];
        let mut next = 0usize;
        let mut closed = false;
        // A corridor crossing an off-mesh link ends this stretch where the link takes off.
        let mut end = end;
        let mut fetch = |portals: &mut Vec<(Vec3, Vec3)>, i: usize, end: &mut Vec3| -> bool {
            while portals.len() <= i && !closed {
                match polys.get(next..next + 2).and_then(|w| self.portal_between(w[0], w[1])) {
                    Some(p) => {
                        portals.push(p);
                        next += 1;
                    }
                    None => {
                        if let Some(t) = polys.get(next..next + 2).and_then(|w| self.traverse_between(w[0], w[1])) {
                            *end = t.from_pos;
                        }
                        portals.push((*end, *end));
                        closed = true;
                    }
                }
            }
            i < portals.len()
        };
        let mut out = vec![start];
        let (mut apex, mut left, mut right) = (start, start, start);
        let (mut left_i, mut right_i) = (0usize, 0usize);
        let mut apex_i;
        let mut i = 1;
        let mut guard = 0usize;
        while fetch(&mut portals, i, &mut end) {
            guard += 1;
            if guard > polys.len() * 4 + 16 || out.len() > max_corners {
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
        if (out.len() <= max_corners && !same_xz(*out.last().expect("start"), end)) || out.len() == 1 {
            out.push(end);
        }
        out
    }

    /// Where the segment from `pos` toward `to` leaves rectangle `p`: the side and the point.
    fn exit(&self, p: &Poly, pos: Vec3, to: Vec3) -> Option<(usize, f32, f32)> {
        let r = self.rect(p);
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
            return None;
        }
        let t = t_exit.clamp(0.0, 1.0);
        Some((side, (pos.x + dx * t).clamp(r[0], r[2]), (pos.z + dz * t).clamp(r[1], r[3])))
    }

    /// The neighbour across `side` of `p` at the edge point `(ex, ez)`.
    fn across(&self, p: &Poly, side: usize, ex: f32, ez: f32) -> Option<(PolyRef, &Poly)> {
        let (cs, o) = (self.config().cell_size, self.config().bounds_min);
        let along = if side == 0 || side == 2 { ez } else { ex };
        let base = if side == 0 || side == 2 { o.z } else { o.x };
        let l = p.links.iter().find(|l| usize::from(l.side) == side && along >= base + l.lo as f32 * cs && along <= base + (l.hi + 1) as f32 * cs)?;
        self.poly(l.to).map(|q| (l.to, q))
    }

    /// Walks the straight segment `from` (on polygon `start`) to `to` across the navmesh: whether
    /// it gets there without leaving it, and the polygons it crosses (at most `max_polys`).
    pub fn raycast(&self, start: PolyRef, from: Vec3, to: Vec3, max_polys: usize) -> (bool, Vec<PolyRef>) {
        let mut polys = vec![start];
        let Some(mut p) = self.poly(start) else { return (false, polys) };
        let mut pos = from;
        while polys.len() <= max_polys {
            if self.contains_xz(p, to) {
                return (true, polys);
            }
            let Some((side, ex, ez)) = self.exit(p, pos, to) else { return (false, polys) };
            let Some((r, q)) = self.across(p, side, ex, ez) else { return (false, polys) };
            polys.push(r);
            p = q;
            pos = Vec3::new(ex, pos.y, ez);
        }
        (false, polys)
    }

    /// Shortens a corridor from `s` to `end`: where a straight walk on the navmesh reaches a
    /// later corner of the string-pulled path, the polygons it crosses replace the detour (A*
    /// over large polygons can bend a path at a polygon's corner it never needed).
    pub fn shortcut(&self, s: Vec3, end: Vec3, polys: Vec<PolyRef>) -> Vec<PolyRef> {
        self.shortcut_filtered(s, end, polys, &crate::Filter::default())
    }

    /// [`NavMesh::shortcut`] that never shortens across an off-mesh link (each stretch is
    /// shortened on its own) or into areas the corridor avoided: a straight walk is taken only
    /// over areas the filter allows that cost no more than plain ground or that the corridor
    /// already crosses.
    pub fn shortcut_filtered(&self, s: Vec3, end: Vec3, polys: Vec<PolyRef>, filter: &crate::Filter) -> Vec<PolyRef> {
        if let Some((k, t)) = self.first_traverse(&polys) {
            let mut out = self.shortcut_filtered(s, t.from_pos, polys[..=k].to_vec(), filter);
            out.extend(self.shortcut_filtered(t.to_pos, end, polys[k + 1..].to_vec(), filter));
            return out;
        }
        let areas: std::collections::BTreeSet<u8> = polys.iter().filter_map(|p| self.poly(*p).map(|q| q.area)).collect();
        let ok_area = |r: &PolyRef| self.poly(*r).is_some_and(|q| filter.cost_of(q.area).is_some_and(|c| c <= 1.0 || areas.contains(&q.area)));
        self.shortcut_plain(s, end, polys, &ok_area)
    }

    fn shortcut_plain(&self, s: Vec3, end: Vec3, polys: Vec<PolyRef>, ok_area: &dyn Fn(&PolyRef) -> bool) -> Vec<PolyRef> {
        if polys.len() <= 2 {
            return polys;
        }
        let (start, last) = (polys[0], *polys.last().expect("not empty"));
        let budget = polys.len() * 4 + 64;
        let (ok, ray) = self.raycast(start, s, end, budget);
        if ok && ray.last() == Some(&last) && ray.iter().all(ok_area) {
            return ray;
        }
        let corners = self.straighten(s, end, &polys, 64);
        if corners.len() <= 2 {
            return polys;
        }
        let mut out = vec![start];
        let mut i = 0;
        let mut tries = 0;
        while i + 1 < corners.len() {
            let from = *out.last().expect("not empty");
            let mut step = None;
            for j in (i + 1..corners.len()).rev() {
                tries += 1;
                if tries > 128 {
                    return polys;
                }
                let (ok, ray) = self.raycast(from, corners[i], corners[j], budget);
                if ok && (j + 1 < corners.len() || ray.last() == Some(&last)) && ray.iter().all(ok_area) {
                    step = Some((j, ray));
                    break;
                }
            }
            match step {
                Some((j, ray)) => {
                    out.extend(ray.into_iter().skip(1));
                    i = j;
                }
                None => return polys,
            }
        }
        out
    }

    /// Moves from `from` (on polygon `start`) toward `to` across the surface: through portals
    /// where there are neighbours, sliding along edges where there are none. Returns the polygon
    /// it ends on and the point (on the surface).
    pub fn move_along(&self, start: PolyRef, from: Vec3, to: Vec3) -> (PolyRef, Vec3) {
        self.move_along_filtered(start, from, to, &crate::Filter::default())
    }

    /// [`NavMesh::move_along`] where polygons of excluded areas are walls.
    pub fn move_along_filtered(&self, start: PolyRef, from: Vec3, to: Vec3, filter: &crate::Filter) -> (PolyRef, Vec3) {
        let Some(mut p) = self.poly(start) else { return (start, from) };
        let mut cur = start;
        let mut pos = from;
        for _ in 0..MOVE_STEPS {
            if self.contains_xz(p, to) {
                return (cur, Vec3::new(to.x, self.height(p, to.x, to.z), to.z));
            }
            let r = self.rect(p);
            let Some((side, ex, ez)) = self.exit(p, pos, to) else { break };
            let exit = Vec3::new(ex, self.height(p, ex, ez), ez);
            match self.across(p, side, ex, ez).filter(|(_, q)| filter.cost_of(q.area).is_some()) {
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
        let total = self.total_area();
        if total <= 0.0 {
            return None;
        }
        let r = self.poly_at_area(rng.f32() * total)?;
        let p = self.poly(r)?;
        let rect = self.rect(p);
        let x = rect[0] + (rect[2] - rect[0]) * rng.f32();
        let z = rect[1] + (rect[3] - rect[1]) * rng.f32();
        Some((r, Vec3::new(x, self.height(p, x, z), z)))
    }

    /// A random point within `radius` of `centre` (in X/Z) on the navmesh: a polygon of the tiles
    /// around it chosen by its area inside the circle's square, then a point in that overlap
    /// (a few tries to land inside the circle). Bounded by the tiles the square touches.
    pub fn random_point_near(&self, rng: &mut Rng, centre: Vec3, radius: f32) -> Option<(PolyRef, Vec3)> {
        let (lo, hi) = (Vec3::new(centre.x - radius, -f32::MAX, centre.z - radius), Vec3::new(centre.x + radius, f32::MAX, centre.z + radius));
        let mut cands: Vec<(PolyRef, [f32; 4], f32)> = Vec::new();
        let mut total = 0.0f32;
        for (tx, tz) in self.tiles_touching(Vec3::new(lo.x, self.config().bounds_min.y, lo.z), Vec3::new(hi.x, self.config().bounds_max.y, hi.z)) {
            let index = self.tile_index(tx, tz);
            let Some(tile) = self.tile(index) else { continue };
            for (k, p) in tile.polys.iter().enumerate() {
                let r = self.rect(p);
                let c = [r[0].max(lo.x), r[1].max(lo.z), r[2].min(hi.x), r[3].min(hi.z)];
                if c[2] <= c[0] || c[3] <= c[1] {
                    continue;
                }
                let a = (c[2] - c[0]) * (c[3] - c[1]);
                total += a;
                cands.push((PolyRef::new(index, k), c, total));
            }
        }
        if total <= 0.0 {
            return None;
        }
        for _ in 0..8 {
            let pick = rng.f32() * total;
            let i = cands.partition_point(|(_, _, t)| *t < pick).min(cands.len() - 1);
            let (r, c, _) = cands[i];
            let x = c[0] + (c[2] - c[0]) * rng.f32();
            let z = c[1] + (c[3] - c[1]) * rng.f32();
            let (dx, dz) = (x - centre.x, z - centre.z);
            if dx * dx + dz * dz <= radius * radius {
                let p = self.poly(r)?;
                return Some((r, Vec3::new(x, self.height(p, x, z), z)));
            }
        }
        None
    }

    /// Every polygon reachable from `start` (a flood over links), sorted; at most `max`.
    pub fn reachable_from(&self, start: PolyRef, max: usize) -> Vec<PolyRef> {
        let mut seen = std::collections::BTreeSet::new();
        let mut queue = vec![start];
        seen.insert(start);
        let mut i = 0;
        while i < queue.len() && seen.len() < max {
            let Some(p) = self.poly(queue[i]) else {
                i += 1;
                continue;
            };
            i += 1;
            for l in &p.links {
                if seen.len() < max && seen.insert(l.to) {
                    queue.push(l.to);
                }
            }
        }
        seen.into_iter().collect()
    }

    /// Length of a corridor's remaining path from `from` to `end` (string-pulled), metres.
    pub fn corridor_length(&self, from: Vec3, end: Vec3, polys: &[PolyRef]) -> f32 {
        self.straighten(from, end, polys, usize::MAX).windows(2).map(|w| dist(w[0], w[1])).sum()
    }
}

/// A path as JSON (corners rounded to a millimetre).
pub fn path_json(p: &PathResult, max_corners: usize) -> Json {
    let r = |v: f32| Json::from(specula_core::dmath::round_f64(f64::from(v) * 1000.0) / 1000.0);
    let corners: Vec<Json> = p.corners.iter().take(max_corners).map(|c| Json::arr([r(c.x), r(c.y), r(c.z)])).collect();
    let out = Json::obj([
        ("status", Json::from(p.status.name())),
        ("complete", Json::from(p.complete())),
        ("length", r(p.length)),
        ("corners", Json::Arr(corners)),
        ("corner_count", Json::from(p.corners.len())),
        ("polygons", Json::from(p.polys.len())),
        ("visited", Json::from(p.visited)),
    ]);
    let mut out = out;
    if !p.offmesh.is_empty() {
        // Each link: the corner it takes off from (the next corner is where it lands).
        out.set("offmesh", Json::arr(p.offmesh.iter().map(|(i, k)| Json::obj([("corner", Json::from(*i)), ("kind", Json::from(k.name()))]))));
    }
    out
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
        let direct = 2.0 * (5.0f32 * 5.0 + 6.3f32 * 6.3).sqrt();
        assert!(p.length >= direct - 0.05 && p.length < direct + 1.0, "length {} vs {}", p.length, direct);
        // Every corner is on the navmesh.
        for c in &p.corners {
            assert!(mesh.nearest(*c, Vec3::new(0.01, 0.1, 0.01)).is_some(), "corner {c:?} off the mesh");
        }
    }

    #[test]
    fn a_straight_line_through_a_doorway_has_no_corners() {
        // The grid origin isn't on a whole metre (bounds from geometry plus a margin).
        let cfg = crate::Config { bounds_min: Vec3::new(-13.4, -2.0, -13.4), bounds_max: Vec3::new(13.4, 5.3, 13.4), tile_cells: 48, ..crate::Config::default() };
        let mut mesh = NavMesh::new(cfg).unwrap();
        let walls = [
            (Vec3::new(-0.2, 0.0, -12.0), Vec3::new(0.2, 2.5, -1.2)),
            (Vec3::new(-0.2, 0.0, 1.2), Vec3::new(0.2, 2.5, 12.0)),
            (Vec3::new(-12.4, 0.0, -12.4), Vec3::new(12.4, 2.5, -12.0)),
            (Vec3::new(-12.4, 0.0, 12.0), Vec3::new(12.4, 2.5, 12.4)),
            (Vec3::new(-12.4, 0.0, -12.0), Vec3::new(-12.0, 2.5, 12.0)),
            (Vec3::new(12.0, 0.0, -12.0), Vec3::new(12.4, 2.5, 12.0)),
        ];
        let mut sources = vec![Source::new(crate::mesh::tests::cuboid(Vec3::new(-12.0, -1.0, -12.0), Vec3::new(12.0, 0.0, 12.0)), true)];
        sources.extend(walls.iter().map(|(lo, hi)| Source::new(crate::mesh::tests::cuboid(*lo, *hi), true)));
        // Pillars beside the doorway, east of the divider.
        for z in [2.55f32, 5.25, 7.95, 10.65] {
            sources.push(Source::new(crate::mesh::tests::cuboid(Vec3::new(1.3, 0.0, z - 0.2), Vec3::new(1.7, 2.0, z + 0.2)), true));
        }
        build_all(&mut mesh, &sources);
        let p = mesh.find_path(Vec3::new(-8.0, 0.0, -4.0), Vec3::new(8.0, 0.0, 4.0), Vec3::new(1.0, 1.8, 1.0), 4096).unwrap();
        assert!(p.complete());
        assert_eq!(p.corners.len(), 2, "the straight line fits through the doorway: {:?}", p.corners);
        // Every polygon keeps the radius (less at most a cell: a wall's edge inside a column
        // counts for the whole column) from the divider's jambs at z = +-1.2.
        for (_, t) in mesh.tiles() {
            for poly in &t.polys {
                let r = mesh.rect(poly);
                if r[2] > -0.2 && r[0] < 0.2 {
                    assert!(r[3] <= 1.2 - 0.3 + 0.15 + 1.0e-4 && r[1] >= -1.2 + 0.3 - 0.15 - 1.0e-4, "too close to a jamb: {r:?}");
                }
            }
        }
        // A path that must turn still turns at the doorway (the eroded opening steps a column at
        // its mouth, so up to four corners), and its corridor is linked.
        let q = mesh.find_path(Vec3::new(-8.0, 0.0, -10.0), Vec3::new(8.0, 0.0, -10.0), Vec3::new(1.0, 1.8, 1.0), 4096).unwrap();
        assert!(q.complete() && q.corners.len() <= 6, "{:?}", q.corners);
        assert!(q.corners.iter().any(|c| c.x.abs() < 0.7 && c.z > -1.0), "through the doorway: {:?}", q.corners);
        for w in q.polys.windows(2) {
            assert!(mesh.poly(w[0]).unwrap().links.iter().any(|l| l.to == w[1]), "corridor polygons are neighbours");
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
        // Near a point: within the radius, on the navmesh.
        let c = Vec3::new(-5.0, 0.0, 3.0);
        for _ in 0..50 {
            let (_, p) = a.random_point_near(&mut r1, c, 2.0).unwrap();
            assert!(horizontal_distance(p, c) <= 2.0 + 1.0e-4, "{p:?}");
        }
        // The other side of the wall is reachable (around its end); a sealed box isn't.
        let (s, _) = a.nearest(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 1.0)).unwrap();
        let (e, _) = a.nearest(Vec3::new(5.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 1.0)).unwrap();
        assert!(a.reachable_from(s, usize::MAX).binary_search(&e).is_ok());
    }
}

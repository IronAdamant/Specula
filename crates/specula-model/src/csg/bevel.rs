//! Chamfers and rounds on the edges a boolean made.
//!
//! An edge is a *boolean edge* when the two faces meeting there come from different operands
//! and bend by more than [`MIN_BEND_DEG`]. Straight runs of them are chained where exactly two
//! runs of the same kind (convex or concave) meet, as round a window's rim. Each chain is swept
//! into one closed tube whose cross-section is the region a chamfer (or a round of `segments`
//! facets) of `width` takes from a convex edge or fills into a concave one; at a joint the two
//! runs' sections meet in the plane bisecting them (a mitre), at a chain's ends the tube is
//! squared off. The tubes are then applied with the same exact booleans: `(result − convex
//! tubes) ∪ concave tubes`, each set disjoint, so one pass of each. Every tube reaches a little
//! past the faces it cuts, so none of its faces is coplanar with the surface.

#![allow(
    clippy::disallowed_types,
    reason = "every HashMap here is lookup-only (get, entry, insert), so no iteration order reaches the output"
)]

use super::*;

/// Faces bending by less than this aren't an edge to bevel (degrees).
pub const MIN_BEND_DEG: f32 = 15.0;
/// Most straight runs of boolean edges one bevel pass sweeps.
pub const MAX_SEGMENTS: usize = 2048;
/// Most arc segments of a round bevel (1 = a flat chamfer).
pub const MAX_ROUND_SEGMENTS: u32 = 8;

/// How boolean edges are bevelled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bevel {
    /// Distance from the edge along each face (metres).
    pub width: f32,
    /// 1 = a flat chamfer; more = a round of that many facets.
    pub segments: u32,
}

/// What a bevel pass did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BevelStats {
    /// Straight runs bevelled (convex, concave) and the chains they formed.
    pub convex: usize,
    pub concave: usize,
    pub chains: usize,
    /// Chains whose tube folded onto itself (too tight a turn for the width), or whose boolean
    /// failed on its own: left sharp.
    pub skipped: usize,
    /// Why the whole bevel was left out (its booleans failed), if it was.
    pub failed: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct Run {
    /// End points (welded ids) and positions.
    a: u32,
    b: u32,
    pa: V3,
    pb: V3,
    /// The faces' outward normals and their in-face directions away from the edge.
    n: [V3; 2],
    t: [V3; 2],
    colour: Rgba,
    convex: bool,
}

/// The tubes that bevel the boolean edges of `shape` (`leaf[t]`: the operand triangle `t` came
/// from): convex ones to cut away, concave ones to add.
pub(super) fn tubes(shape: &Shape, leaf: &[u32], bevel: &Bevel, budget: &mut Budget) -> Result<(Vec<Shape>, Vec<Shape>, BevelStats), CsgError> {
    let w = bevel.width;
    let s = shape.clone();
    // Equal positions as one point (colour seams split them), keeping each triangle's leaf.
    let mut ids: HashMap<[u32; 3], u32> = HashMap::new();
    let mut pos: Vec<V3> = Vec::new();
    let remap: Vec<u32> = s
        .positions
        .iter()
        .map(|p| {
            let k = crate::shape::canon(*p).map(f32::to_bits);
            *ids.entry(k).or_insert_with(|| {
                pos.push(*p);
                (pos.len() - 1) as u32
            })
        })
        .collect();
    let tris: Vec<[u32; 3]> = s.tris.iter().map(|t| t.map(|i| remap[i as usize])).collect();
    let mut by_edge: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        for k in 0..3 {
            by_edge.entry((t[k], t[(k + 1) % 3])).or_default().push(i);
        }
    }
    let normal = |t: usize| -> V3 {
        let [a, b, c] = tris[t].map(|i| pos[i as usize]);
        math3::normalize(math3::tri_normal(a, b, c))
    };
    let centroid = |t: usize| -> V3 {
        let [a, b, c] = tris[t].map(|i| pos[i as usize]);
        math3::scale(math3::add(math3::add(a, b), c), 1.0 / 3.0)
    };
    let cos_min = crate::dmath::cos(MIN_BEND_DEG * std::f32::consts::PI / 180.0);
    // Boolean edges, once each (from the triangle holding a -> b with a < b).
    let mut edges: Vec<(u32, u32, usize, usize)> = Vec::new();
    for (i, t) in tris.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if a >= b {
                continue;
            }
            let (Some(fw), Some(bw)) = (by_edge.get(&(a, b)), by_edge.get(&(b, a))) else { continue };
            if fw.len() != 1 || bw.len() != 1 || fw[0] != i {
                continue;
            }
            let j = bw[0];
            if leaf[i] != leaf[j] && math3::dot(normal(i), normal(j)) <= cos_min {
                edges.push((a, b, i, j));
            }
        }
    }
    edges.sort_unstable();
    budget.charge(edges.len() as u64 * 8)?;
    // Straight runs: edges meeting at a point used by exactly two boolean edges, parallel, with
    // the same pair of face normals.
    let mut at: HashMap<u32, Vec<usize>> = HashMap::new();
    for (k, e) in edges.iter().enumerate() {
        at.entry(e.0).or_default().push(k);
        at.entry(e.1).or_default().push(k);
    }
    let dir = |e: &(u32, u32, usize, usize)| math3::normalize(math3::sub(pos[e.1 as usize], pos[e.0 as usize]));
    let same = |a: V3, b: V3| math3::dot(a, b) > 0.9999;
    let same_run = |x: usize, y: usize| -> bool {
        let (ex, ey) = (&edges[x], &edges[y]);
        let par = math3::length(math3::cross(dir(ex), dir(ey))) < 1e-4;
        let (nx1, nx2, ny1, ny2) = (normal(ex.2), normal(ex.3), normal(ey.2), normal(ey.3));
        par && ((same(nx1, ny1) && same(nx2, ny2)) || (same(nx1, ny2) && same(nx2, ny1)))
    };
    let mut used = vec![false; edges.len()];
    let mut runs: Vec<Run> = Vec::new();
    for k0 in 0..edges.len() {
        if used[k0] {
            continue;
        }
        used[k0] = true;
        let (mut lo, mut hi) = (edges[k0].0, edges[k0].1);
        for end in 0..2 {
            loop {
                let v = if end == 0 { hi } else { lo };
                let list = &at[&v];
                if list.len() != 2 {
                    break;
                }
                let Some(&next) = list.iter().find(|&&k| !used[k]) else { break };
                if !same_run(k0, next) {
                    break;
                }
                used[next] = true;
                let e = edges[next];
                let other = if e.0 == v { e.1 } else { e.0 };
                if end == 0 {
                    hi = other;
                } else {
                    lo = other;
                }
            }
        }
        if runs.len() >= MAX_SEGMENTS {
            return Err(CsgError::limit(format!("more than {MAX_SEGMENTS} straight runs of boolean edges to bevel; bevel fewer cuts"), runs.len() as f64 + 1.0, MAX_SEGMENTS as f64));
        }
        let (pa, pb) = (pos[lo as usize], pos[hi as usize]);
        let d = math3::sub(pb, pa);
        if math3::length(d) < 1e-6 {
            continue;
        }
        let d = math3::normalize(d);
        let e = edges[k0];
        let faces = [e.2, e.3];
        let n = faces.map(normal);
        // In-face directions: from the edge towards each face's centroid, square to the edge.
        let t = faces.map(|f| {
            let v = math3::sub(centroid(f), pa);
            math3::normalize(math3::sub(v, math3::scale(d, math3::dot(v, d))))
        });
        let convex = math3::dot(t[1], n[0]) < 0.0;
        let colour = s.colors[s.tris[e.2][0] as usize];
        runs.push(Run { a: lo, b: hi, pa, pb, n, t, colour, convex });
    }
    if runs.is_empty() {
        return Ok((Vec::new(), Vec::new(), BevelStats::default()));
    }
    // Chains: runs of one kind meeting two at a point.
    let mut ends: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, r) in runs.iter().enumerate() {
        ends.entry(r.a).or_default().push(i);
        ends.entry(r.b).or_default().push(i);
    }
    let link = |v: u32, from: usize| -> Option<usize> {
        let list = &ends[&v];
        if list.len() != 2 {
            return None;
        }
        let other = if list[0] == from { list[1] } else { list[0] };
        (other != from && runs[other].convex == runs[from].convex).then_some(other)
    };
    let mut stats = BevelStats::default();
    let mut taken = vec![false; runs.len()];
    let (mut cutters, mut fillers) = (Vec::new(), Vec::new());
    let segments = bevel.segments.clamp(1, MAX_ROUND_SEGMENTS);
    for start in 0..runs.len() {
        if taken[start] {
            continue;
        }
        // Walk back to the chain's start (or round a loop), then forward collecting runs, each
        // oriented to continue the last.
        let mut first = start;
        let mut first_end = runs[start].a;
        let mut guard = 0;
        while let Some(prev) = link(first_end, first) {
            guard += 1;
            if prev == start || taken[prev] || guard > runs.len() {
                break;
            }
            first_end = if runs[prev].a == first_end { runs[prev].b } else { runs[prev].a };
            first = prev;
        }
        let mut chain: Vec<(usize, bool)> = Vec::new(); // (run, reversed)
        let mut cur = first;
        let mut from = first_end;
        let closed;
        loop {
            taken[cur] = true;
            let reversed = runs[cur].a != from;
            chain.push((cur, reversed));
            let to = if reversed { runs[cur].a } else { runs[cur].b };
            match link(to, cur) {
                Some(next) if next == first && chain.len() > 2 => {
                    closed = true;
                    break;
                }
                Some(next) if !taken[next] => {
                    cur = next;
                    from = to;
                }
                _ => {
                    closed = false;
                    break;
                }
            }
        }
        budget.charge(chain.len() as u64 * 64)?;
        let convex = runs[first].convex;
        let mut tube = sweep(&runs, &chain, closed, w, segments);
        if tube.open_edge_count() > 0 {
            stats.skipped += 1;
            continue;
        }
        tube.set_color(runs[first].colour);
        tube.tags.iter_mut().for_each(|t| *t = crate::shape::TAG_BEVEL);
        stats.chains += 1;
        if convex {
            stats.convex += chain.len();
            cutters.push(tube);
        } else {
            stats.concave += chain.len();
            fillers.push(tube);
        }
    }
    Ok((cutters, fillers, stats))
}

/// A run's cross-section around an edge point (offsets square to the edge): the region a
/// chamfer (`segments` 1) or a round takes from a convex edge or fills into a concave one, from
/// face `A` round to face `B`, pushed a little past both faces. Point 0 is the corner: the
/// section is star-shaped from it.
fn section(r: &Run, swap: bool, w: f32, segments: u32) -> Vec<V3> {
    let (na, nb, ta, tb) = if swap { (r.n[1], r.n[0], r.t[1], r.t[0]) } else { (r.n[0], r.n[1], r.t[0], r.t[1]) };
    let delta = (w * 0.02).max(2e-4);
    let (a, b) = (math3::scale(ta, w), math3::scale(tb, w));
    // Outward off a convex edge (into the air), inward into a concave one (into the material).
    let sign = if r.convex { 1.0 } else { -1.0 };
    let out = math3::normalize(math3::add(na, nb));
    let mut pts = vec![math3::scale(out, sign * delta * 2.0), math3::add(a, math3::scale(na, sign * delta))];
    if segments > 1 {
        // The round's centre: w along each face, then r off both faces (into the material for a
        // convex edge, into the air for a concave one).
        let dn = math3::sub(na, nb);
        let dt = math3::sub(ta, tb);
        let rad = (w * math3::dot(dt, dn) / math3::dot(dn, dn).max(1e-12)).abs();
        let c = math3::sub(a, math3::scale(na, sign * rad));
        let (ua, ub) = (math3::sub(a, c), math3::sub(b, c));
        for k in 0..=segments {
            let dirv = math3::normalize(math3::lerp(ua, ub, k as f32 / segments as f32));
            pts.push(math3::add(c, math3::scale(dirv, math3::length(ua))));
        }
    }
    pts.push(math3::add(b, math3::scale(nb, sign * delta)));
    pts
}

/// Sweeps a chain of runs (each `(run, reversed)`, continuing the last) into a closed tube.
fn sweep(runs: &[Run], chain: &[(usize, bool)], closed: bool, w: f32, segments: u32) -> Shape {
    // Orient each run along the chain, and name its faces so face A is the one it shares with
    // the run before (by normal).
    let mut oriented: Vec<(V3, V3, V3, bool)> = Vec::with_capacity(chain.len()); // (start, end, dir, swap)
    // Face A of the first run: the one it shares with the second (a window rim's wall face).
    let first = &runs[chain[0].0];
    let mut prev_na = match chain.get(1) {
        Some(&(r1, _)) => {
            let shared = |n: V3| runs[r1].n.iter().map(|m| math3::dot(n, *m)).fold(f32::MIN, f32::max);
            if shared(first.n[1]) > shared(first.n[0]) {
                first.n[1]
            } else {
                first.n[0]
            }
        }
        None => first.n[0],
    };
    for &(ri, rev) in chain {
        let r = &runs[ri];
        let (p, q) = if rev { (r.pb, r.pa) } else { (r.pa, r.pb) };
        let d = math3::normalize(math3::sub(q, p));
        let swap = math3::dot(prev_na, r.n[1]) > math3::dot(prev_na, r.n[0]);
        prev_na = if swap { r.n[1] } else { r.n[0] };
        oriented.push((p, q, d, swap));
    }
    let m = chain.len();
    let sec: Vec<Vec<V3>> = chain.iter().zip(&oriented).map(|(&(ri, _), o)| section(&runs[ri], o.3, w, segments)).collect();
    let k = sec[0].len();
    // Point on the plane through `origin` with normal `mn`, from `x` along `d`.
    let project = |x: V3, d: V3, origin: V3, mn: V3| -> V3 {
        let den = math3::dot(mn, d);
        if den.abs() < 1e-6 {
            return x;
        }
        math3::add(x, math3::scale(d, math3::dot(mn, math3::sub(origin, x)) / den))
    };
    // The joint section between run i and run i+1 (cyclic when closed): both runs' sections
    // projected onto the bisecting plane, averaged.
    let joint = |i: usize| -> Vec<V3> {
        let j = (i + 1) % m;
        let (oi, oj) = (oriented[i], oriented[j]);
        let at_pt = oi.1;
        let mut mn = math3::normalize(math3::add(oi.2, oj.2));
        if math3::length(mn) < 0.5 || math3::dot(mn, oi.2) < 0.3 {
            mn = oi.2;
        }
        (0..k)
            .map(|q| {
                let a = project(math3::add(at_pt, sec[i][q]), oi.2, at_pt, mn);
                let b = project(math3::add(at_pt, sec[j][q]), oj.2, at_pt, mn);
                math3::lerp(a, b, 0.5)
            })
            .collect()
    };
    let mut rings: Vec<Vec<V3>> = Vec::new();
    if closed {
        for i in 0..m {
            rings.push(joint(i));
        }
    } else {
        rings.push(sec[0].iter().map(|o| math3::add(oriented[0].0, *o)).collect());
        for i in 0..m - 1 {
            rings.push(joint(i));
        }
        rings.push(sec[m - 1].iter().map(|o| math3::add(oriented[m - 1].1, *o)).collect());
    }
    let mut s = Shape::default();
    for r in &rings {
        s.positions.extend_from_slice(r);
    }
    s.colors = vec![[1.0; 4]; s.positions.len()];
    let nr = rings.len();
    let bands = if closed { nr } else { nr - 1 };
    let k32 = k as u32;
    for b in 0..bands {
        let (r0, r1) = ((b * k) as u32, (((b + 1) % nr) * k) as u32);
        for q in 0..k32 {
            let q1 = (q + 1) % k32;
            s.tris.push([r0 + q, r1 + q, r1 + q1]);
            s.tris.push([r0 + q, r1 + q1, r0 + q1]);
        }
    }
    if !closed {
        // Caps: fans from point 0 (the sections are star-shaped from their corner).
        let last = ((nr - 1) * k) as u32;
        for q in 1..k32 - 1 {
            s.tris.push([0, q, q + 1]);
            s.tris.push([last, last + q + 1, last + q]);
        }
    }
    s.tags = vec![crate::shape::TAG_FACE; s.tris.len()];
    // One consistent winding; outward if the volume came out positive.
    if s.signed_volume() < 0.0 {
        s.tris.iter_mut().for_each(|t| t.swap(1, 2));
    }
    s
}

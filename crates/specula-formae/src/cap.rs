//! A flat cap with holes, cut into triangles that are proved to tile it.
//!
//! The library's faces are simple polygons with no holes, so the cap of a region with holes (the
//! top of a slab of an annulus, the plate of a relief round its figure) is made of triangles
//! whose corners are the rings' own points (no point is added). **Why the triangles are sound**:
//! a cut is accepted only when, checked in one pass,
//!
//! 1. every triangle turns counter-clockwise past rounding (its doubled area is over the bound
//!    `16 ε |b − a| |c − a|`), and
//! 2. the triangles' edges, as directed segments added up, are the rings' edges (outer
//!    counter-clockwise, holes clockwise) and nothing else: every edge inside the region is used
//!    once in each direction.
//!
//! Then the triangles tile the region, exactly once: the number of triangles over a point (each
//! counted +1, since each turns counter-clockwise) is the winding number of the sum of their
//! boundaries about it, which by (2) is the rings' winding number, 1 inside the region and 0
//! outside it (the rings being simple and nested, as a well-made outline's are). A triangle
//! outside the region, or two over one point, would make it more somewhere. How the triangles
//! are found (holes bridged to the outer ring, then ears cut) is only a way to find them: what is
//! returned has been checked, and what does not pass is refused.

use crate::maths::*;

/// Why a cap could not be cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CapFault {
    /// No ear could be cut (the rings are not a well-made region at this rounding), with the
    /// number of corners left.
    NoEar(usize),
    /// A hole could not be bridged to the outer boundary (the hole's index).
    NoBridge(usize),
    /// A triangle does not turn counter-clockwise past rounding (its index).
    Flat(usize),
    /// The triangles' edges are not the rings' edges.
    Chain,
}

fn turn(a: Pt, b: Pt, c: Pt) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// The rounding bound of [`turn`]: each product rounds by a few ε of the sides' sizes.
fn turn_bound(a: Pt, b: Pt, c: Pt) -> f64 {
    16.0 * f64::EPSILON * (dist(a, b) * dist(a, c)).max(dist(a, b) * dist(b, c))
}

/// Whether `a b c` turns counter-clockwise past rounding.
fn left(a: Pt, b: Pt, c: Pt) -> bool {
    turn(a, b, c) > turn_bound(a, b, c)
}

/// The number of triangles a region of `n` points in `rings` rings is cut into: `n + 2 h − 2`
/// for `h = rings − 1` holes (Euler's formula for a disc with holes).
pub(crate) fn triangles_of(n: u64, rings: u64) -> u64 {
    (n + 2 * rings).saturating_sub(4)
}

/// The cut of the region whose rings are `rings` (indices into `pts`; the first the outer,
/// counter-clockwise; the rest holes, clockwise): triangles of point indices, each
/// counter-clockwise, checked as the module says.
pub(crate) fn cut(pts: &[Pt], rings: &[Vec<u32>]) -> Result<Vec<[u32; 3]>, CapFault> {
    let poly = bridge(pts, rings)?;
    let mut tris = ears(pts, poly)?;
    unsliver(pts, &mut tris);
    check(pts, rings, &tris)?;
    Ok(tris)
}

/// The proof's two premises (the module's documentation), in one pass.
pub(crate) fn check(pts: &[Pt], rings: &[Vec<u32>], tris: &[[u32; 3]]) -> Result<(), CapFault> {
    for (i, t) in tris.iter().enumerate() {
        let [a, b, c] = t.map(|v| pts[v as usize]);
        if !left(a, b, c) {
            return Err(CapFault::Flat(i));
        }
    }
    // Directed edges: the triangles' with +1, the rings' with −1; each undirected pair must sum
    // to nothing in each direction.
    let mut e: Vec<(u32, u32, i32)> = Vec::with_capacity(3 * tris.len() + pts.len());
    let mut add = |a: u32, b: u32, w: i32| {
        if a < b {
            e.push((a, b, w));
        } else {
            e.push((b, a, -w));
        }
    };
    for t in tris {
        for k in 0..3 {
            add(t[k], t[(k + 1) % 3], 1);
        }
    }
    for r in rings {
        for k in 0..r.len() {
            add(r[k], r[(k + 1) % r.len()], -1);
        }
    }
    e.sort_unstable();
    let mut i = 0;
    while i < e.len() {
        let mut j = i;
        let mut s = 0;
        while j < e.len() && (e[j].0, e[j].1) == (e[i].0, e[i].1) {
            s += e[j].2;
            j += 1;
        }
        if s != 0 || e[i].0 == e[i].1 {
            return Err(CapFault::Chain);
        }
        i = j;
    }
    Ok(())
}

/// One polygon of point indices: the outer ring with each hole spliced in by a bridge (a hole's
/// point of greatest x joined to a point of the polygon it sees to its right, the bridge walked
/// both ways), holes taken from the rightmost in.
fn bridge(pts: &[Pt], rings: &[Vec<u32>]) -> Result<Vec<u32>, CapFault> {
    let mut poly: Vec<u32> = rings[0].clone();
    let mut holes: Vec<(usize, usize)> = (1..rings.len())
        .map(|h| {
            let r = &rings[h];
            let m = (0..r.len()).max_by(|&a, &b| pts[r[a] as usize][0].total_cmp(&pts[r[b] as usize][0]).then(pts[r[b] as usize][1].total_cmp(&pts[r[a] as usize][1]))).unwrap_or(0);
            (h, m)
        })
        .collect();
    holes.sort_by(|a, b| pts[rings[b.0][b.1] as usize][0].total_cmp(&pts[rings[a.0][a.1] as usize][0]));
    for (h, m) in holes {
        let hole = &rings[h];
        let mi = hole[m];
        let mp = pts[mi as usize];
        let at = visible(pts, &poly, mp).ok_or(CapFault::NoBridge(h))?;
        // poly[..=at], then the hole from m round to m, then back to poly[at] and on.
        let mut next = Vec::with_capacity(poly.len() + hole.len() + 2);
        next.extend_from_slice(&poly[..=at]);
        for k in 0..=hole.len() {
            next.push(hole[(m + k) % hole.len()]);
        }
        next.push(poly[at]);
        next.extend_from_slice(&poly[at + 1..]);
        poly = next;
    }
    Ok(poly)
}

/// The place in `poly` of a point that `m` sees (the segment from `m` to it meets no edge of
/// `poly` but at that point), found as Eberly finds it: the nearest edge hit by the ray from `m`
/// to +x, the end of it farther right, and, if a reflex corner lies in the triangle that makes
/// with the hit, the one of those nearest the ray's direction. Then checked against every edge.
fn visible(pts: &[Pt], poly: &[u32], m: Pt) -> Option<usize> {
    let n = poly.len();
    let p = |i: usize| pts[poly[i % n] as usize];
    let mut best: Option<(f64, usize)> = None;
    for i in 0..n {
        let (a, b) = (p(i), p(i + 1));
        // Edges going down past the ray (the outer ring runs counter-clockwise, so the boundary
        // to the right of a hole runs upward... either way, take both directions).
        if (a[1] > m[1]) == (b[1] > m[1]) {
            continue;
        }
        let x = a[0] + (m[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
        if x < m[0] {
            continue;
        }
        if best.is_none_or(|(bx, _)| x < bx) {
            best = Some((x, i));
        }
    }
    let (x, i) = best?;
    let hit = [x, m[1]];
    let cand = if p(i)[0] >= p(i + 1)[0] { i % n } else { (i + 1) % n };
    let cp = p(cand);
    // Reflex corners in the triangle (m, hit, cand): take the one with the least angle to +x.
    let mut pick = cand;
    let mut pick_key = (f64::INFINITY, f64::INFINITY);
    for j in 0..n {
        let q = p(j);
        if poly[j] == poly[cand] {
            continue;
        }
        let reflex = turn(p(j + n - 1), q, p(j + 1)) <= 0.0;
        if !reflex {
            continue;
        }
        let tri = if turn(m, hit, cp) >= 0.0 { [m, hit, cp] } else { [m, cp, hit] };
        let inside = (0..3).all(|k| turn(tri[k], tri[(k + 1) % 3], q) >= 0.0);
        if inside && q[0] >= m[0] {
            let d = sub(q, m);
            let key = (d[1].abs() / len(d).max(f64::MIN_POSITIVE), len(d));
            if key < pick_key {
                pick_key = key;
                pick = j;
            }
        }
    }
    // The bridge must cross no edge (a point it ends on is not a crossing).
    let target = p(pick);
    let ok = (0..n).all(|k| poly[k % n] == poly[pick] || poly[(k + 1) % n] == poly[pick] || !segments_meet(m, target, p(k), p(k + 1)));
    if ok {
        return Some(pick);
    }
    // The fallback: the nearest point of the polygon the segment to which meets no edge.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| dist(p(a), m).total_cmp(&dist(p(b), m)));
    order.into_iter().find(|&j| {
        let target = p(j);
        (0..n).all(|k| poly[k % n] == poly[j] || poly[(k + 1) % n] == poly[j] || !segments_meet(m, target, p(k), p(k + 1)))
    })
}

/// Whether the segments `p q` and `a b` cross or touch (within rounding).
fn segments_meet(p: Pt, q: Pt, a: Pt, b: Pt) -> bool {
    let side = |x: Pt, y: Pt, z: Pt| {
        let t = turn(x, y, z);
        let e = turn_bound(x, y, z);
        if t > e {
            1
        } else if t < -e {
            -1
        } else {
            0
        }
    };
    let (d1, d2, d3, d4) = (side(p, q, a), side(p, q, b), side(a, b, p), side(a, b, q));
    if d1 * d2 < 0 && d3 * d4 < 0 {
        return true;
    }
    let on = |x: Pt, y: Pt, z: Pt, d: i32| d == 0 && z[0] >= x[0].min(y[0]) && z[0] <= x[0].max(y[0]) && z[1] >= x[1].min(y[1]) && z[1] <= x[1].max(y[1]);
    on(p, q, a, d1) || on(p, q, b, d2) || on(a, b, p, d3) || on(a, b, q, d4)
}

/// Cuts ears from the polygon (point indices, a point may appear twice where a bridge runs):
/// a corner that turns counter-clockwise past rounding and whose triangle has no other corner of
/// the polygon (a reflex one: only those can be inside) in it or on it. The reflex corners are
/// kept in a grid, so a triangle is checked against those near it.
fn ears(pts: &[Pt], poly: Vec<u32>) -> Result<Vec<[u32; 3]>, CapFault> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n.saturating_sub(2));
    if n < 3 {
        return Err(CapFault::NoEar(n));
    }
    let at = |i: usize| pts[poly[i] as usize];
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut alive = vec![true; n];
    // The grid of reflex corners.
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for i in 0..n {
        let q = at(i);
        for k in 0..2 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let reflex = |i: usize, prev: &[usize], next: &[usize]| !left(at(prev[i]), at(i), at(next[i]));
    let side = ceil((n as f64).sqrt()).max(1.0) as usize;
    let cell = |q: Pt, k: usize| -> usize {
        let w = (hi[k] - lo[k]).max(f64::MIN_POSITIVE);
        (((q[k] - lo[k]) / w * side as f64) as usize).min(side - 1)
    };
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); side * side];
    for i in 0..n {
        if reflex(i, &prev, &next) {
            grid[cell(at(i), 0) * side + cell(at(i), 1)].push(i);
        }
    }
    // An ear thinner than this (its area over its longest side) is cut only when no other is
    // left: a sliver a thousandth of a millimetre wide across a kilometre fails the mesh's floor
    // (a fan from a bridge to a hole's edge that nearly lines up with it makes one).
    let thin = 8.0 * crate::outline::EDGE_FLOOR * dist(lo, hi);
    let is_ear = |i: usize, prev: &[usize], next: &[usize], alive: &[bool], grid: &[Vec<usize>], strict: bool| -> bool {
        let (a, b, c) = (at(prev[i]), at(i), at(next[i]));
        if !left(a, b, c) {
            return false;
        }
        if strict && 0.5 * turn(a, b, c) <= thin * dist(a, b).max(dist(b, c)).max(dist(c, a)) {
            return false;
        }
        let ids = [poly[prev[i]], poly[i], poly[next[i]]];
        let (x0, x1) = (cell([a[0].min(b[0]).min(c[0]), 0.0], 0), cell([a[0].max(b[0]).max(c[0]), 0.0], 0));
        let (y0, y1) = (cell([0.0, a[1].min(b[1]).min(c[1])], 1), cell([0.0, a[1].max(b[1]).max(c[1])], 1));
        for gx in x0..=x1 {
            for gy in y0..=y1 {
                for &j in &grid[gx * side + gy] {
                    if !alive[j] || ids.contains(&poly[j]) {
                        continue;
                    }
                    let q = at(j);
                    // Not clearly outside: in it, or on it.
                    let outside = !(turn(a, b, q) >= -turn_bound(a, b, q) && turn(b, c, q) >= -turn_bound(b, c, q) && turn(c, a, q) >= -turn_bound(c, a, q));
                    if !outside {
                        return false;
                    }
                }
            }
        }
        true
    };
    let width = |i: usize, prev: &[usize], next: &[usize]| {
        let (a, b, c) = (at(prev[i]), at(i), at(next[i]));
        0.5 * turn(a, b, c) / dist(a, b).max(dist(b, c)).max(dist(c, a))
    };
    let mut left_n = n;
    let mut i = 0usize;
    let mut since = 0usize;
    let mut strict = true;
    // Sweeps for the widest thin ear are bounded (each is a pass over what is left): past 64 the
    // first ear found is cut, thin or not, and the mesh's own floor refuses a sliver as data.
    let mut sweeps = 0u32;
    while left_n > 3 {
        if left_n == 4 {
            // The last two triangles: of the two ways to cut the quadrilateral left, the one whose
            // thinner triangle is wider (the last triangle is forced, and a greedy order can leave
            // it a sliver).
            let tri_w = |a: usize, b: usize, c: usize| {
                let (pa, pb, pc) = (at(a), at(b), at(c));
                if !left(pa, pb, pc) {
                    return f64::NEG_INFINITY;
                }
                0.5 * turn(pa, pb, pc) / dist(pa, pb).max(dist(pb, pc)).max(dist(pc, pa))
            };
            let mut best: Option<(f64, usize)> = None;
            let mut j = i;
            for _ in 0..4 {
                if is_ear(j, &prev, &next, &alive, &grid, false) {
                    let w = tri_w(prev[j], j, next[j]).min(tri_w(prev[j], next[j], next[next[j]]));
                    if best.is_none_or(|(bw, _)| w > bw) {
                        best = Some((w, j));
                    }
                }
                j = next[j];
            }
            let Some((_, j)) = best else { return Err(CapFault::NoEar(4)) };
            let (p, q) = (prev[j], next[j]);
            out.push([poly[p], poly[j], poly[q]]);
            out.push([poly[p], poly[q], poly[next[q]]]);
            return Ok(out);
        }
        if !strict && sweeps < 64 {
            sweeps += 1;
            // No ear but thin ones: the widest of them, found in one sweep.
            let mut best: Option<(f64, usize)> = None;
            let mut j = i;
            for _ in 0..left_n {
                if is_ear(j, &prev, &next, &alive, &grid, false) {
                    let w = width(j, &prev, &next);
                    if best.is_none_or(|(bw, _)| w > bw) {
                        best = Some((w, j));
                    }
                }
                j = next[j];
            }
            match best {
                Some((_, j)) => i = j,
                None => return Err(CapFault::NoEar(left_n)),
            }
        }
        if is_ear(i, &prev, &next, &alive, &grid, strict) {
            strict = sweeps < 64;
            let (p, q) = (prev[i], next[i]);
            out.push([poly[p], poly[i], poly[q]]);
            alive[i] = false;
            next[p] = q;
            prev[q] = p;
            left_n -= 1;
            since = 0;
            i = p;
            continue;
        }
        i = next[i];
        since += 1;
        if since > left_n {
            if !strict {
                return Err(CapFault::NoEar(left_n));
            }
            strict = false;
            since = 0;
        }
    }
    let p = prev[i];
    let q = next[i];
    out.push([poly[p], poly[i], poly[q]]);
    Ok(out)
}

/// Slivers flipped away: a triangle thinner than eight floors of the region's size across (its
/// area over its longest side) has that side flipped with the triangle across it, where the two
/// make a strictly convex quadrilateral and the flip leaves both wider than the sliver was. A
/// flip keeps the number of triangles and their edges' sum (the new inner edge is used once each
/// way), so the proof's premises are unchanged in kind and are checked after. At most 64 flips:
/// slivers are rare (a hole's edge whose line passes a ten-billionth of the size from a point of
/// the outer ring, by the samples' chance).
fn unsliver(pts: &[Pt], tris: &mut [[u32; 3]]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for t in tris.iter() {
        for &v in t {
            for k in 0..2 {
                lo[k] = lo[k].min(pts[v as usize][k]);
                hi[k] = hi[k].max(pts[v as usize][k]);
            }
        }
    }
    let thin = 8.0 * crate::outline::EDGE_FLOOR * dist(lo, hi);
    let width = |t: [u32; 3]| {
        let [a, b, c] = t.map(|v| pts[v as usize]);
        0.5 * turn(a, b, c) / dist(a, b).max(dist(b, c)).max(dist(c, a))
    };
    for _ in 0..64 {
        let Some(i) = (0..tris.len()).find(|&i| width(tris[i]) <= thin) else { return };
        let t = tris[i];
        // Its longest side, as (a, b) with c opposite.
        let k = (0..3).max_by(|&x, &y| dist(pts[t[x] as usize], pts[t[(x + 1) % 3] as usize]).total_cmp(&dist(pts[t[y] as usize], pts[t[(y + 1) % 3] as usize]))).unwrap_or(0);
        let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
        let Some(j) = (0..tris.len()).find(|&j| j != i && (0..3).any(|m| tris[j][m] == b && tris[j][(m + 1) % 3] == a)) else { return };
        let u = tris[j];
        let m = (0..3).find(|&m| u[m] == b).unwrap_or(0);
        let d = u[(m + 2) % 3];
        // The quadrilateral a, d, b, c (counter-clockwise); flipped: (c, a, d) and (d, b, c).
        let (n1, n2) = ([c, a, d], [d, b, c]);
        let [pa, pd, pb, pc] = [a, d, b, c].map(|v| pts[v as usize]);
        let convex = left(pa, pd, pb) && left(pd, pb, pc) && left(pb, pc, pa) && left(pc, pa, pd);
        let before = width(t);
        if !convex || width(n1).min(width(n2)) <= before {
            return;
        }
        tris[i] = n1;
        tris[j] = n2;
    }
}

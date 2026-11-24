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

/// A rounding bound of [`turn`]: which one a cut decides by (see [`cut`]).
type Bound = fn(Pt, Pt, Pt) -> f64;

/// The bound a cut tries first: `16 ε` times the greater of `|b − a| |c − a|` and
/// `|b − a| |c − b|`. It is more than the error of the computation (see [`tight_bound`]): a
/// corner that turns by less than about `3e-15` of a radian is taken as flat, and a point that
/// near a line as on it. A cut by it keeps clear of triangles whose corners line up to a rounding
/// (a truss's hole corners on a chord's line), which a cut by the tight bound may make: they are
/// sound, and narrower than the mesh's floor.
fn wide_bound(a: Pt, b: Pt, c: Pt) -> f64 {
    16.0 * f64::EPSILON * (dist(a, b) * dist(a, c)).max(dist(a, b) * dist(b, c))
}

/// The rounding bound of [`turn`] as it is computed: its two products `p₁ = (b₀ − a₀)(c₁ − a₁)`
/// and `p₂ = (b₁ − a₁)(c₀ − a₀)` each come out within `3 ε` of themselves (each difference and
/// the product rounded once), and the subtraction adds `ε (|p₁| + |p₂|)`, so the computed value
/// is within `4 ε (|p₁| + |p₂|)` of the true one; twice that is the bound. A cut falls back to it
/// when the wide bound leaves no ear: a triangle with a 0.3 mm side and two sides 7,500 m long
/// (9e-8 of turn) is flat by the wide bound (2e-7) and clearly turning by this one (its true error
/// under 1e-14), and the strip between a long side and a column of holes a bar in from it is cut
/// only by this one (review 1 of stage 4, finding 1).
fn tight_bound(a: Pt, b: Pt, c: Pt) -> f64 {
    let (p1, p2) = ((b[0] - a[0]) * (c[1] - a[1]), (b[1] - a[1]) * (c[0] - a[0]));
    8.0 * f64::EPSILON * (p1.abs() + p2.abs())
}

/// Whether `a b c` turns counter-clockwise past rounding, by the bound `bd`.
fn left(a: Pt, b: Pt, c: Pt, bd: Bound) -> bool {
    turn(a, b, c) > bd(a, b, c)
}

#[cfg(test)]
thread_local! {
    /// How many reflex corners the ear tests have looked at, on this thread (the tests hold the
    /// cut's cost by it: a count, never a time).
    pub(crate) static LOOKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The number of triangles a region of `n` points in `rings` rings is cut into: `n + 2 h − 2`
/// for `h = rings − 1` holes (Euler's formula for a disc with holes).
pub(crate) fn triangles_of(n: u64, rings: u64) -> u64 {
    (n + 2 * rings).saturating_sub(4)
}

/// The cut of the region whose rings are `rings` (indices into `pts`; the first the outer,
/// counter-clockwise; the rest holes, clockwise): triangles of point indices, each
/// counter-clockwise, checked as the module says.
///
/// It is cut first deciding every turn by the wide bound, as it always was; where that finds no
/// cut, by the tight one (each sound: the proof holds with either bound, since each is at least
/// the computation's error). So whatever the wide bound cut is cut as before, byte for byte.
pub(crate) fn cut(pts: &[Pt], rings: &[Vec<u32>]) -> Result<Vec<[u32; 3]>, CapFault> {
    cut_by(pts, rings, wide_bound).or_else(|first| cut_by(pts, rings, tight_bound).map_err(|_| first))
}

fn cut_by(pts: &[Pt], rings: &[Vec<u32>], bd: Bound) -> Result<Vec<[u32; 3]>, CapFault> {
    let poly = bridge(pts, rings, bd)?;
    let mut tris = ears(pts, poly, bd)?;
    unsliver(pts, &mut tris, bd);
    // (A region with holes only: its triangles are a raise's cap faces, held to the mesh's floor;
    // a face with no hole is cut for the mesh's triangles, which no floor holds, and is left as
    // it was.)
    if rings.len() > 1 && thinnest(pts, &tris) <= floor_of(pts, &tris) {
        lawson(pts, &mut tris, rings, bd);
    }
    check_by(pts, rings, &tris, bd)?;
    Ok(tris)
}

/// The proof's two premises (the module's documentation), in one pass, by the tight bound.
#[cfg(test)]
pub(crate) fn check(pts: &[Pt], rings: &[Vec<u32>], tris: &[[u32; 3]]) -> Result<(), CapFault> {
    check_by(pts, rings, tris, tight_bound)
}

fn check_by(pts: &[Pt], rings: &[Vec<u32>], tris: &[[u32; 3]], bd: Bound) -> Result<(), CapFault> {
    for (i, t) in tris.iter().enumerate() {
        let [a, b, c] = t.map(|v| pts[v as usize]);
        if !left(a, b, c, bd) {
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
fn bridge(pts: &[Pt], rings: &[Vec<u32>], bd: Bound) -> Result<Vec<u32>, CapFault> {
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
        let at = visible(pts, &poly, mp, bd).ok_or(CapFault::NoBridge(h))?;
        let at = wedge_of(pts, &poly, at, mp);
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

/// Of the places in `poly` that hold the same point as `poly[at]` (a point an earlier bridge
/// ended on is in the polygon twice, once on each side of that bridge), the one whose corner
/// opens toward `m`: the bridge to `m` must leave the point inside that corner's angle, or the
/// spliced polygon crosses itself at the point (two holes bridged to one corner of the outer
/// ring, in the wrong order round it, left no ear to cut: review 1 of stage 4, finding 1).
fn wedge_of(pts: &[Pt], poly: &[u32], at: usize, m: Pt) -> usize {
    let n = poly.len();
    let v = poly[at];
    let places: Vec<usize> = (0..n).filter(|&k| poly[k] == v).collect();
    if places.len() < 2 {
        return at;
    }
    let p = pts[v as usize];
    let d = sub(m, p);
    let cross = |a: Pt, b: Pt| a[0] * b[1] - a[1] * b[0];
    // The corner's angle runs counter-clockwise from the edge out (to the next point) to the
    // edge in (back to the previous one): the region lies there.
    let opens = |k: usize, strict: bool| {
        let (u, w) = (sub(pts[poly[(k + 1) % n] as usize], p), sub(pts[poly[(k + n - 1) % n] as usize], p));
        let (ud, dw) = (cross(u, d), cross(d, w));
        let ok = |x: f64| if strict { x > 0.0 } else { x >= 0.0 };
        if cross(u, w) > 0.0 {
            ok(ud) && ok(dw)
        } else {
            ok(ud) || ok(dw)
        }
    };
    places.iter().copied().find(|&k| opens(k, true)).or_else(|| places.iter().copied().find(|&k| opens(k, false))).unwrap_or(at)
}

/// The place in `poly` of a point that `m` sees (the segment from `m` to it meets no edge of
/// `poly` but at that point), found as Eberly finds it: the nearest edge hit by the ray from `m`
/// to +x, the end of it farther right, and, if a reflex corner lies in the triangle that makes
/// with the hit, the one of those nearest the ray's direction. Then checked against every edge.
fn visible(pts: &[Pt], poly: &[u32], m: Pt, bd: Bound) -> Option<usize> {
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
    let ok = (0..n).all(|k| poly[k % n] == poly[pick] || poly[(k + 1) % n] == poly[pick] || !segments_meet(m, target, p(k), p(k + 1), bd));
    if ok {
        return Some(pick);
    }
    // The fallback: the nearest point of the polygon the segment to which meets no edge.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| dist(p(a), m).total_cmp(&dist(p(b), m)));
    order.into_iter().find(|&j| {
        let target = p(j);
        (0..n).all(|k| poly[k % n] == poly[j] || poly[(k + 1) % n] == poly[j] || !segments_meet(m, target, p(k), p(k + 1), bd))
    })
}

/// Whether the segments `p q` and `a b` cross or touch (within rounding).
fn segments_meet(p: Pt, q: Pt, a: Pt, b: Pt, bd: Bound) -> bool {
    let side = |x: Pt, y: Pt, z: Pt| {
        let t = turn(x, y, z);
        let e = bd(x, y, z);
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
    let on = |x: Pt, y: Pt, z: Pt, d: i32| {
        // On the line within rounding (d == 0) and between its ends along it. Not by the
        // segment's box: a point a rounding off the line is outside the box of a level segment
        // (a truss's hole corners on its chord's line, 1e-17 apart), and a bridge along the chord
        // through two of them was taken as crossing nothing.
        let (u, v, w) = (sub(y, x), sub(z, x), sub(z, y));
        let dot = |a: Pt, b: Pt| a[0] * b[0] + a[1] * b[1];
        let tol = 16.0 * f64::EPSILON * dot(u, u);
        d == 0 && dot(v, u) >= -tol && dot(w, u) <= tol
    };
    on(p, q, a, d1) || on(p, q, b, d2) || on(a, b, p, d3) || on(a, b, q, d4)
}

/// Cuts ears from the polygon (point indices, a point may appear twice where a bridge runs):
/// a corner that turns counter-clockwise past rounding and whose triangle has no other corner of
/// the polygon (a reflex one: only those can be inside) in it or on it. The reflex corners are
/// kept in a grid, so a triangle is checked against those near it.
fn ears(pts: &[Pt], poly: Vec<u32>, bd: Bound) -> Result<Vec<[u32; 3]>, CapFault> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n.saturating_sub(2));
    if n < 3 {
        return Err(CapFault::NoEar(n));
    }
    let at = |i: usize| pts[poly[i] as usize];
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut alive = vec![true; n];
    // The grid of reflex corners, over the reflex corners' own bounds (a cut only ever asks
    // whether one of them lies in an ear). Over the whole region's, a small figure's many
    // corners on a large plate (a relief of a 0.1 mm sine band on a plate 28 km wide) all fell
    // in one cell, and every ear looked at every one of them: one cut took a minute (review 3,
    // finding 1). A cell is clamped to the grid, so an ear reaching past these bounds still
    // looks in every cell its box covers: the same corners are looked at, and the same ears cut.
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    let (mut rlo, mut rhi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    let reflex = |i: usize, prev: &[usize], next: &[usize]| !left(at(prev[i]), at(i), at(next[i]), bd);
    for i in 0..n {
        let q = at(i);
        let r = reflex(i, &prev, &next);
        for k in 0..2 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
            if r {
                rlo[k] = rlo[k].min(q[k]);
                rhi[k] = rhi[k].max(q[k]);
            }
        }
    }
    let side = ceil((n as f64).sqrt()).max(1.0) as usize;
    let cell = |q: Pt, k: usize| -> usize {
        let w = (rhi[k] - rlo[k]).max(f64::MIN_POSITIVE);
        let t = (q[k] - rlo[k]) / w * side as f64;
        // (Below the bounds, or with no reflex corner at all, the first cell.)
        if t.is_nan() || t <= 0.0 { 0 } else { (t as usize).min(side - 1) }
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
        if !left(a, b, c, bd) {
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
                    #[cfg(test)]
                    LOOKS.with(|l| l.set(l.get() + 1));
                    if !alive[j] || ids.contains(&poly[j]) {
                        continue;
                    }
                    let q = at(j);
                    // Not clearly outside: in it, or on it.
                    let outside = !(turn(a, b, q) >= -bd(a, b, q) && turn(b, c, q) >= -bd(b, c, q) && turn(c, a, q) >= -bd(c, a, q));
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
                if !left(pa, pb, pc, bd) {
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
fn unsliver(pts: &[Pt], tris: &mut [[u32; 3]], bd: Bound) {
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
        let convex = left(pa, pd, pb, bd) && left(pd, pb, pc, bd) && left(pb, pc, pa, bd) && left(pc, pa, pd, bd);
        let before = width(t);
        if !convex || width(n1).min(width(n2)) <= before {
            return;
        }
        tris[i] = n1;
        tris[j] = n2;
    }
}

/// The least width (area over longest side) of the triangles.
fn thinnest(pts: &[Pt], tris: &[[u32; 3]]) -> f64 {
    tris.iter().map(|&t| width_of(pts, t)).fold(f64::INFINITY, f64::min)
}

fn width_of(pts: &[Pt], t: [u32; 3]) -> f64 {
    let [a, b, c] = t.map(|v| pts[v as usize]);
    0.5 * turn(a, b, c) / dist(a, b).max(dist(b, c)).max(dist(c, a))
}

/// The mesh's floor for the region's size: a triangle no wider fails the mesh's own check
/// (whose size, the solid's, is no less), so a cut that has one was refused before it was
/// repaired, and repairing it changes no build that was made.
fn floor_of(pts: &[Pt], tris: &[[u32; 3]]) -> f64 {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for t in tris {
        for &v in t {
            for k in 0..2 {
                lo[k] = lo[k].min(pts[v as usize][k]);
                hi[k] = hi[k].max(pts[v as usize][k]);
            }
        }
    }
    crate::outline::EDGE_FLOOR * dist(lo, hi)
}

/// Where a triangle under the mesh's floor is left after [`unsliver`] (an ear clipper's greedy order can leave a fan from
/// one far corner across a strip whose sides' points are not level: a bevelled ladder of a grate,
/// whose holes grew by the inset and whose border's points did not move with them, made triangles
/// 4.5e-8 m across in a strip 1.3 mm wide), the cut is made Delaunay by Lawson's flips: every
/// edge inside the region whose two triangles make a strictly convex quadrilateral, the far
/// corner of one inside the other's circumcircle, is flipped, pass after pass, until none is (or
/// a bound of flips is reached). A flip keeps the triangles' count and their edges' sum, so the
/// proof's premises are unchanged in kind and are checked after; the rings' edges are never
/// flipped (each is used by one triangle only). Only a cut that would fail the floor is
/// touched, so every cut that was built is as it was.
fn lawson(pts: &[Pt], tris: &mut [[u32; 3]], rings: &[Vec<u32>], bd: Bound) {
    let ring_edge = {
        let mut v: Vec<(u32, u32)> = rings.iter().flat_map(|r| (0..r.len()).map(move |k| (r[k], r[(k + 1) % r.len()]))).collect();
        v.sort_unstable();
        v
    };
    let in_circle = |a: Pt, b: Pt, c: Pt, d: Pt| {
        let (ax, ay, bx, by, cx, cy) = (a[0] - d[0], a[1] - d[1], b[0] - d[0], b[1] - d[1], c[0] - d[0], c[1] - d[1]);
        (ax * ax + ay * ay) * (bx * cy - cx * by) - (bx * bx + by * by) * (ax * cy - cx * ay) + (cx * cx + cy * cy) * (ax * by - bx * ay) > 0.0
    };
    let mut budget = 8 * tris.len();
    for _ in 0..256 {
        // Each directed edge, with its triangle and its place in it, in a sorted list.
        let mut edges: Vec<((u32, u32), usize, usize)> = Vec::with_capacity(3 * tris.len());
        for (i, t) in tris.iter().enumerate() {
            for k in 0..3 {
                edges.push(((t[k], t[(k + 1) % 3]), i, k));
            }
        }
        edges.sort_unstable();
        let find = |e: (u32, u32)| edges.binary_search_by(|x| x.0.cmp(&e)).ok().map(|j| (edges[j].1, edges[j].2));
        let mut flips: Vec<(usize, usize, [u32; 3], [u32; 3])> = Vec::new();
        let mut used = vec![false; tris.len()];
        for &((a, b), i, k) in &edges {
            if a > b || ring_edge.binary_search(&(a, b)).is_ok() || ring_edge.binary_search(&(b, a)).is_ok() {
                continue;
            }
            let Some((j, m)) = find((b, a)) else { continue };
            if used[i] || used[j] {
                continue;
            }
            let c = tris[i][(k + 2) % 3];
            let d = tris[j][(m + 2) % 3];
            let [pa, pb, pc, pd] = [a, b, c, d].map(|v| pts[v as usize]);
            // (a, b, c) and (b, a, d): the quadrilateral a, d, b, c; flipped, (c, a, d) and (d, b, c).
            let convex = left(pa, pd, pb, bd) && left(pd, pb, pc, bd) && left(pb, pc, pa, bd) && left(pc, pa, pd, bd);
            if convex && in_circle(pa, pb, pc, pd) {
                let (n1, n2) = ([c, a, d], [d, b, c]);
                if width_of(pts, n1).min(width_of(pts, n2)) > width_of(pts, tris[i]).min(width_of(pts, tris[j])) {
                    used[i] = true;
                    used[j] = true;
                    flips.push((i, j, n1, n2));
                }
            }
        }
        if flips.is_empty() {
            return;
        }
        for (i, j, n1, n2) in flips {
            if budget == 0 {
                return;
            }
            budget -= 1;
            tris[i] = n1;
            tris[j] = n2;
        }
    }
}

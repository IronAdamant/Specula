//! 2D polygons: outlines for extrusion and profiles for sweeps.
//!
//! Points are `[a, b]` in a plane. **Canonical orientation is counter-clockwise** (positive
//! shoelace area) in that plane; [`canonical`] fixes any input.

use crate::dmath;
use crate::math3::V2;

/// Shoelace area: positive for counter-clockwise.
pub fn signed_area(p: &[V2]) -> f32 {
    let n = p.len();
    let mut s = 0.0;
    for i in 0..n {
        let (a, b) = (p[i], p[(i + 1) % n]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    0.5 * s
}

/// The polygon counter-clockwise, without repeated consecutive points or a closing duplicate.
pub fn canonical(p: &[V2]) -> Vec<V2> {
    let mut out: Vec<V2> = Vec::with_capacity(p.len());
    for &q in p {
        if out.last() != Some(&q) {
            out.push(q);
        }
    }
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    if signed_area(&out) < 0.0 {
        out.reverse();
    }
    out
}

fn cross2(o: V2, a: V2, b: V2) -> f32 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Whether two closed segments properly intersect (shared endpoints excluded).
fn segments_cross(p1: V2, p2: V2, q1: V2, q2: V2) -> bool {
    let d1 = cross2(q1, q2, p1);
    let d2 = cross2(q1, q2, p2);
    let d3 = cross2(p1, p2, q1);
    let d4 = cross2(p1, p2, q2);
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// True if no two non-adjacent edges cross. O(n²); outlines are small.
pub fn is_simple(p: &[V2]) -> bool {
    let n = p.len();
    if n < 3 {
        return false;
    }
    for i in 0..n {
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if segments_cross(p[i], p[(i + 1) % n], p[j], p[(j + 1) % n]) {
                return false;
            }
        }
    }
    true
}

/// Ear-clipping triangulation of a simple counter-clockwise polygon. Triangles are
/// counter-clockwise. `None` if the polygon is degenerate.
pub fn triangulate(p: &[V2]) -> Option<Vec<[u32; 3]>> {
    let n = p.len();
    if n < 3 {
        return None;
    }
    let mut idx: Vec<usize> = (0..n).collect();
    let mut tris = Vec::with_capacity(n - 2);
    let mut guard = 0;
    while idx.len() > 3 {
        let m = idx.len();
        let mut found = false;
        // Prefer the ear with the best (largest smallest-angle proxy) shape for nicer caps:
        // deterministic scan, first ear wins among equals.
        let mut best: Option<(usize, f32)> = None;
        for k in 0..m {
            let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (p[ia], p[ib], p[ic]);
            if cross2(a, b, c) <= 0.0 {
                continue; // reflex or flat
            }
            let mut inside = false;
            for &j in &idx {
                if j == ia || j == ib || j == ic {
                    continue;
                }
                let q = p[j];
                if q == a || q == b || q == c {
                    continue; // a duplicate of a corner (collapsed edge) never blocks an ear
                }
                if cross2(a, b, q) >= 0.0 && cross2(b, c, q) >= 0.0 && cross2(c, a, q) >= 0.0 {
                    inside = true;
                    break;
                }
            }
            if inside {
                continue;
            }
            let quality = ear_quality(a, b, c);
            if best.is_none_or(|(_, q)| quality > q) {
                best = Some((k, quality));
            }
        }
        if let Some((k, _)) = best {
            let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            tris.push([ia as u32, ib as u32, ic as u32]);
            idx.remove(k);
            found = true;
        }
        if !found {
            // Collinear leftovers: drop a flat vertex if there is one.
            let flat = (0..m).find(|&k| cross2(p[idx[(k + m - 1) % m]], p[idx[k]], p[idx[(k + 1) % m]]) == 0.0);
            match flat {
                Some(k) => {
                    idx.remove(k);
                }
                None => return None,
            }
        }
        guard += 1;
        if guard > 4 * n {
            return None;
        }
    }
    if cross2(p[idx[0]], p[idx[1]], p[idx[2]]) > 0.0 {
        tris.push([idx[0] as u32, idx[1] as u32, idx[2] as u32]);
    }
    if tris.is_empty() {
        None
    } else {
        Some(tris)
    }
}

/// Smallest-angle proxy: area over the sum of squared edge lengths (larger = rounder).
fn ear_quality(a: V2, b: V2, c: V2) -> f32 {
    let l = |p: V2, q: V2| (p[0] - q[0]) * (p[0] - q[0]) + (p[1] - q[1]) * (p[1] - q[1]);
    cross2(a, b, c) / (l(a, b) + l(b, c) + l(c, a))
}

fn norm2(v: V2) -> V2 {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l > 1e-20 {
        [v[0] / l, v[1] / l]
    } else {
        [0.0, 0.0]
    }
}

/// Offsets a counter-clockwise polygon inward by `d` (outward if negative), mitred, with miters
/// limited to 3·|d|.
pub fn inset(p: &[V2], d: f32) -> Vec<V2> {
    let n = p.len();
    let out = (0..n)
        .map(|i| {
            let (prev, cur, next) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
            let e0 = norm2([cur[0] - prev[0], cur[1] - prev[1]]);
            let e1 = norm2([next[0] - cur[0], next[1] - cur[1]]);
            // Inward normals of a CCW polygon point left of each edge.
            let n0 = [-e0[1], e0[0]];
            let n1 = [-e1[1], e1[0]];
            let bis = norm2([n0[0] + n1[0], n0[1] + n1[1]]);
            let cosh = bis[0] * n0[0] + bis[1] * n0[1];
            let len = if cosh > 1.0 / 3.0 { d / cosh } else { 3.0 * d };
            [cur[0] + bis[0] * len, cur[1] + bis[1] * len]
        })
        .collect();
    fix_flipped(out, p)
}

/// Edges shorter than the offset turn inside out; collapse each such edge to its midpoint
/// (both vertices kept, at the same spot, so ring vertex counts stay equal).
fn fix_flipped(mut out: Vec<V2>, original: &[V2]) -> Vec<V2> {
    let n = out.len();
    for _ in 0..4 {
        let mut changed = false;
        for i in 0..n {
            let j = (i + 1) % n;
            let (a, b) = (out[i], out[j]);
            let (oa, ob) = (original[i], original[j]);
            let d = (b[0] - a[0]) * (ob[0] - oa[0]) + (b[1] - a[1]) * (ob[1] - oa[1]);
            if d < 0.0 {
                let m = [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])];
                out[i] = m;
                out[j] = m;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    out
}

/// Cuts every corner by `c` along both adjacent edges (clamped to 45% of each edge): a 2D
/// chamfer. Each vertex becomes two.
pub fn bevel_corners(p: &[V2], c: f32) -> Vec<V2> {
    if c <= 0.0 {
        return p.to_vec();
    }
    let n = p.len();
    let mut out = Vec::with_capacity(2 * n);
    for i in 0..n {
        let (prev, cur, next) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
        let d0 = [prev[0] - cur[0], prev[1] - cur[1]];
        let d1 = [next[0] - cur[0], next[1] - cur[1]];
        let l0 = (d0[0] * d0[0] + d0[1] * d0[1]).sqrt();
        let l1 = (d1[0] * d1[0] + d1[1] * d1[1]).sqrt();
        let c0 = c.min(0.45 * l0);
        let c1 = c.min(0.45 * l1);
        out.push([cur[0] + d0[0] / l0 * c0, cur[1] + d0[1] / l0 * c0]);
        out.push([cur[0] + d1[0] / l1 * c1, cur[1] + d1[1] / l1 * c1]);
    }
    out
}

/// A regular polygon (counter-clockwise), first vertex at angle `start_deg` from +a.
pub fn regular(sides: u32, radius: f32, start_deg: f64) -> Vec<V2> {
    (0..sides)
        .map(|i| {
            let (s, c) = dmath::sin_cos_deg(start_deg + 360.0 * i as f64 / sides as f64);
            [radius * c as f32, radius * s as f32]
        })
        .collect()
}

/// A `w` × `h` rectangle centred on the origin.
pub fn rect(w: f32, h: f32) -> Vec<V2> {
    let (x, y) = (0.5 * w, 0.5 * h);
    vec![[-x, -y], [x, -y], [x, y], [-x, y]]
}

/// A tombstone/door arch: `width` wide, `height` tall from b = 0, with a semicircular top made of
/// `segments` edges.
pub fn arch(width: f32, height: f32, segments: u32) -> Vec<V2> {
    let r = 0.5 * width;
    let shoulder = (height - r).max(0.0);
    let mut out = vec![[-r, 0.0], [r, 0.0]];
    for i in 0..=segments {
        let (s, c) = dmath::sin_cos_deg(180.0 * i as f64 / segments as f64);
        out.push([r * c as f32, shoulder + r * s as f32]);
    }
    canonical(&out)
}

/// A star with `points` tips between radii `inner` and `outer`.
pub fn star(points: u32, outer: f32, inner: f32, start_deg: f64) -> Vec<V2> {
    (0..2 * points)
        .map(|i| {
            let r = if i % 2 == 0 { outer } else { inner };
            let (s, c) = dmath::sin_cos_deg(start_deg + 180.0 * i as f64 / points as f64);
            [r * c as f32, r * s as f32]
        })
        .collect()
}

/// Splits every edge into `n` equal pieces.
pub fn resample(p: &[V2], n: u32) -> Vec<V2> {
    if n <= 1 {
        return p.to_vec();
    }
    let m = p.len();
    let mut out = Vec::with_capacity(m * n as usize);
    for i in 0..m {
        let (a, b) = (p[i], p[(i + 1) % m]);
        for k in 0..n {
            let t = k as f32 / n as f32;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

/// Moves every point towards or away from the centroid direction by up to `amount` (seeded),
/// for broken, irregular outlines.
pub fn jitter(p: &[V2], amount: f32, seed: u64) -> Vec<V2> {
    p.iter()
        .enumerate()
        .map(|(i, q)| {
            let h = crate::noise::hash3(seed, i as i32, 17, 3);
            let r = ((h >> 8) as f32 / 16_777_216.0) * 2.0 - 1.0;
            let d = norm2(*q);
            [q[0] + d[0] * r * amount, q[1] + d[1] * r * amount]
        })
        .collect()
}

/// Centroid of the vertices.
pub fn centroid(p: &[V2]) -> V2 {
    let mut s = [0.0f32; 2];
    for q in p {
        s[0] += q[0];
        s[1] += q[1];
    }
    [s[0] / p.len() as f32, s[1] / p.len() as f32]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_orientation_and_area() {
        let cw = vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        let c = canonical(&cw);
        assert_eq!(c.len(), 4);
        assert_eq!(signed_area(&c), 1.0);
    }

    #[test]
    fn triangulates_concave_polygon() {
        // An L shape.
        let l = canonical(&[[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]]);
        let t = triangulate(&l).unwrap();
        assert_eq!(t.len(), 4);
        let area: f32 = t.iter().map(|t| 0.5 * cross2(l[t[0] as usize], l[t[1] as usize], l[t[2] as usize])).sum();
        assert!((area - 3.0).abs() < 1e-6);
        assert!(t.iter().all(|t| cross2(l[t[0] as usize], l[t[1] as usize], l[t[2] as usize]) > 0.0));
    }

    #[test]
    fn triangulates_star_and_arch() {
        let s = star(5, 1.0, 0.4, 90.0);
        assert!(is_simple(&s));
        assert_eq!(triangulate(&s).unwrap().len(), 8);
        let a = arch(0.5, 0.8, 6);
        assert!(signed_area(&a) > 0.0 && is_simple(&a));
        assert_eq!(triangulate(&a).unwrap().len(), a.len() - 2);
    }

    #[test]
    fn inset_and_bevel() {
        let r = rect(2.0, 2.0);
        let i = inset(&r, 0.25);
        assert!((signed_area(&i) - 2.25).abs() < 1e-5);
        let b = bevel_corners(&r, 0.1);
        assert_eq!(b.len(), 8);
        assert!((signed_area(&b) - (4.0 - 4.0 * 0.005)).abs() < 1e-5);
        assert!(is_simple(&b));
    }

    #[test]
    fn detects_self_intersection() {
        let bow = [[0.0, 0.0], [1.0, 1.0], [1.0, 0.0], [0.0, 1.0]];
        assert!(!is_simple(&bow));
        assert!(is_simple(&regular(7, 1.0, 0.0)));
    }
}

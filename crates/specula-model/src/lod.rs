//! Automatic levels of detail by vertex clustering (Rossignac & Borrel 1993).
//!
//! Vertices are snapped to a grid of cell size `c`; each occupied cell becomes one vertex (the
//! mean of its members, colour averaged) and triangles that collapse are dropped. Fast,
//! deterministic and robust on any input. The cell size for a target triangle ratio is found by
//! bisection. Quadric-error simplification (better shapes per triangle) is the planned upgrade.

use std::collections::HashMap;

use crate::dmath;
use crate::math3::{self, V3};
use crate::shape::Shape;

/// Clusters `shape` on a grid of `cell` metres anchored at `origin`.
pub fn cluster(shape: &Shape, cell: f32, origin: V3) -> Shape {
    if cell <= 0.0 {
        return shape.clone();
    }
    let inv = 1.0 / cell;
    let mut ids: HashMap<[i32; 3], u32> = HashMap::new();
    let mut sum: Vec<[f64; 3]> = Vec::new();
    let mut csum: Vec<[f64; 4]> = Vec::new();
    let mut count: Vec<u32> = Vec::new();
    let remap: Vec<u32> = shape
        .positions
        .iter()
        .zip(&shape.colors)
        .map(|(p, c)| {
            let d = math3::sub(*p, origin);
            let key = [dmath::floor(d[0] * inv) as i32, dmath::floor(d[1] * inv) as i32, dmath::floor(d[2] * inv) as i32];
            let id = *ids.entry(key).or_insert_with(|| {
                sum.push([0.0; 3]);
                csum.push([0.0; 4]);
                count.push(0);
                count.len() as u32 - 1
            }) as usize;
            for k in 0..3 {
                sum[id][k] += p[k] as f64;
            }
            for k in 0..4 {
                csum[id][k] += c[k] as f64;
            }
            count[id] += 1;
            id as u32
        })
        .collect();
    let positions: Vec<V3> = sum.iter().zip(&count).map(|(s, &n)| s.map(|v| (v / n as f64) as f32)).collect();
    let colors = csum.iter().zip(&count).map(|(s, &n)| s.map(|v| (v / n as f64) as f32)).collect();
    let mut out = Shape { positions, colors, tris: Vec::new(), tags: Vec::new(), bevel_tint: shape.bevel_tint };
    let mut seen: HashMap<[u32; 3], ()> = HashMap::new();
    for (t, &tag) in shape.tris.iter().zip(&shape.tags) {
        let m = [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]];
        if m[0] == m[1] || m[1] == m[2] || m[0] == m[2] {
            continue;
        }
        let (a, b, c) = (out.positions[m[0] as usize], out.positions[m[1] as usize], out.positions[m[2] as usize]);
        if 0.5 * math3::length(math3::tri_normal(a, b, c)) < crate::mesh::MIN_TRIANGLE_AREA {
            continue;
        }
        let mut key = m;
        key.sort_unstable();
        if seen.insert(key, ()).is_some() {
            continue; // the same three clusters already form a triangle
        }
        out.tris.push(m);
        out.tags.push(tag);
    }
    // Drop unreferenced clusters to keep the buffers tight.
    let mut used = vec![u32::MAX; out.positions.len()];
    let mut pos = Vec::new();
    let mut col = Vec::new();
    for t in &mut out.tris {
        for v in t.iter_mut() {
            let i = *v as usize;
            if used[i] == u32::MAX {
                used[i] = pos.len() as u32;
                pos.push(out.positions[i]);
                col.push(out.colors[i]);
            }
            *v = used[i];
        }
    }
    out.positions = pos;
    out.colors = col;
    out
}

/// One part of a model to simplify together: its shape and a cell-size multiplier
/// (0 = never simplify this part, e.g. thin emissive runes that must stay readable).
pub struct LodInput<'a> {
    pub shape: &'a Shape,
    pub cell_scale: f32,
}

/// Finds the cell size whose clustered triangle total is closest to (and not above, where
/// possible) `ratio` × the original total, by bisection on a log scale. Returns the simplified
/// shapes and the cell size used.
pub fn simplify(parts: &[LodInput], ratio: f32, origin: V3, extent: f32) -> (Vec<Shape>, f32) {
    let total: usize = parts.iter().map(|p| p.shape.triangle_count()).sum();
    // Kept parts (cell_scale 0) can't shrink: aim the remaining budget at the others.
    let kept: usize = parts.iter().filter(|p| p.cell_scale <= 0.0).map(|p| p.shape.triangle_count()).sum();
    let target = ((total as f32 * ratio) as usize).saturating_sub(kept) + kept;
    // No part may collapse entirely: its cell is capped at a quarter of its largest dimension.
    let caps: Vec<f32> = parts
        .iter()
        .map(|p| {
            let s = p.shape.bounds().size();
            0.25 * s[0].max(s[1]).max(s[2])
        })
        .collect();
    let run = |cell: f32| -> Vec<Shape> {
        parts.iter().zip(&caps).map(|(p, &cap)| cluster(p.shape, (cell * p.cell_scale).min(cap), origin)).collect()
    };
    let count = |v: &[Shape]| v.iter().map(|s| s.triangle_count()).sum::<usize>();
    if ratio >= 1.0 || total == 0 {
        return (parts.iter().map(|p| p.shape.clone()).collect(), 0.0);
    }
    let (mut lo, mut hi) = (extent * 1e-4, extent * 0.5);
    let mut best: Option<(Vec<Shape>, f32, usize)> = None;
    for _ in 0..24 {
        let mid = (lo * hi).sqrt();
        let r = run(mid);
        let n = count(&r);
        let better = match &best {
            None => true,
            Some((_, _, bn)) => {
                let d = (n as i64 - target as i64).abs();
                let bd = (*bn as i64 - target as i64).abs();
                (n <= target && (*bn > target || d < bd)) || (n > target && *bn > target && d < bd)
            }
        };
        if better {
            best = Some((r, mid, n));
        }
        if n > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (shapes, cell, _) = best.unwrap();
    (shapes, cell)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clustering_reduces_and_keeps_bounds_roughly() {
        let mut s = Shape::sphere(1.0, 3);
        s.set_color([0.5; 4]);
        let c = cluster(&s, 0.3, [-1.0, 0.0, -1.0]);
        assert!(c.triangle_count() < s.triangle_count() / 2, "{}", c.triangle_count());
        assert!(c.triangle_count() > 20);
        let (a, b) = (s.bounds(), c.bounds());
        for k in 0..3 {
            assert!((a.min[k] - b.min[k]).abs() < 0.3 && (a.max[k] - b.max[k]).abs() < 0.3);
        }
        assert!(c.colors.iter().all(|v| (v[0] - 0.5).abs() < 1e-6));
        let m = c.finalize(40.0, "m");
        m.validate().unwrap();
    }

    #[test]
    fn simplify_hits_target_ratio() {
        let s = Shape::sphere(1.0, 3); // 1280 triangles
        for ratio in [0.5f32, 0.25, 0.1] {
            let (out, cell) = simplify(&[LodInput { shape: &s, cell_scale: 1.0 }], ratio, [-1.0, 0.0, -1.0], 2.0);
            let n = out[0].triangle_count() as f32;
            let want = 1280.0 * ratio;
            assert!(cell > 0.0);
            assert!(n <= want * 1.05 && n >= want * 0.6, "ratio {ratio}: {n} triangles, want ~{want}");
        }
    }

    #[test]
    fn zero_cell_scale_keeps_a_part() {
        let a = Shape::sphere(1.0, 2);
        let b = Shape::sphere(0.1, 1);
        let (out, _) = simplify(&[LodInput { shape: &a, cell_scale: 1.0 }, LodInput { shape: &b, cell_scale: 0.0 }], 0.3, [-1.0, 0.0, -1.0], 2.0);
        assert_eq!(out[1].triangle_count(), 80);
        assert!(out[0].triangle_count() < 320);
    }

    #[test]
    fn deterministic() {
        let s = Shape::sphere(1.0, 3);
        let a = cluster(&s, 0.21, [-1.0, 0.0, -1.0]);
        let b = cluster(&s, 0.21, [-1.0, 0.0, -1.0]);
        assert_eq!(a, b);
    }
}

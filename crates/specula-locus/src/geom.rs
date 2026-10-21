//! Upright oriented boxes (turned about Y only) and their 2D (X/Z) tests: overlap, distance,
//! how far one sticks out of another.

use specula_core::dmath;
use specula_core::math::Vec3;

/// An upright box: centre, half sizes along its own axes, and its X and Z axes in the X/Z plane
/// (unit, perpendicular).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box3 {
    pub center: Vec3,
    pub half: Vec3,
    /// Local +X in world X/Z.
    pub ax: (f32, f32),
    /// Local +Z in world X/Z.
    pub az: (f32, f32),
}

/// A yaw (degrees, about +Y) as the local X and Z axes in world X/Z.
pub fn yaw_axes(yaw_deg: f32) -> ((f32, f32), (f32, f32)) {
    let (s, c) = dmath::sin_cos(yaw_deg.to_radians());
    // R(yaw)·X = (cos, -sin) and R(yaw)·Z = (sin, cos) in (x, z).
    ((c, -s), (s, c))
}

/// The yaw (degrees) that turns local +Z toward the world direction `(dx, dz)`.
pub fn yaw_toward(dx: f32, dz: f32) -> f32 {
    dmath::atan2(dx, dz).to_degrees()
}

/// The smallest angle between two yaws, degrees (0..180).
pub fn yaw_difference(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % 360.0;
    if d < 0.0 {
        d += 360.0;
    }
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

fn dot(a: (f32, f32), b: (f32, f32)) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

impl Box3 {
    pub fn point(p: Vec3) -> Box3 {
        Box3 { center: p, half: Vec3::ZERO, ax: (1.0, 0.0), az: (0.0, 1.0) }
    }

    pub fn aligned(lo: Vec3, hi: Vec3) -> Box3 {
        Box3 { center: (lo + hi) * 0.5, half: (hi - lo) * 0.5, ax: (1.0, 0.0), az: (0.0, 1.0) }
    }

    /// A footprint: local bounds `lo..hi` at `position` turned by `yaw_deg`.
    pub fn footprint(lo: Vec3, hi: Vec3, position: Vec3, yaw_deg: f32) -> Box3 {
        let (ax, az) = yaw_axes(yaw_deg);
        let c = (lo + hi) * 0.5;
        let center = Vec3::new(position.x + ax.0 * c.x + az.0 * c.z, position.y + c.y, position.z + ax.1 * c.x + az.1 * c.z);
        Box3 { center, half: (hi - lo) * 0.5, ax, az }
    }

    pub fn top(&self) -> f32 {
        self.center.y + self.half.y
    }

    pub fn bottom(&self) -> f32 {
        self.center.y - self.half.y
    }

    /// The four X/Z corners, in order around the box.
    pub fn corners(&self) -> [(f32, f32); 4] {
        let (c, h) = ((self.center.x, self.center.z), (self.half.x, self.half.z));
        let at = |sx: f32, sz: f32| (c.0 + self.ax.0 * h.0 * sx + self.az.0 * h.1 * sz, c.1 + self.ax.1 * h.0 * sx + self.az.1 * h.1 * sz);
        [at(-1.0, -1.0), at(1.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)]
    }

    /// Smaller by `by` on every side (never below zero).
    pub fn shrunk(&self, by: f32) -> Box3 {
        let h = self.half;
        Box3 { half: Vec3::new((h.x - by).max(0.0), (h.y - by).max(0.0), (h.z - by).max(0.0)), ..*self }
    }

    /// The point's local coordinates (along ax, along az) relative to the centre.
    pub fn local(&self, p: (f32, f32)) -> (f32, f32) {
        let d = (p.0 - self.center.x, p.1 - self.center.z);
        (dot(d, self.ax), dot(d, self.az))
    }

    /// How far a point lies outside the box in X/Z (0 inside).
    pub fn outside(&self, p: (f32, f32)) -> f32 {
        let (u, v) = self.local(p);
        let (eu, ev) = ((u.abs() - self.half.x).max(0.0), (v.abs() - self.half.z).max(0.0));
        (eu * eu + ev * ev).sqrt()
    }

    /// How far `other`'s footprint sticks out of this one in X/Z (the worst corner).
    pub fn overhang(&self, other: &Box3) -> f32 {
        other.corners().iter().map(|c| self.outside(*c)).fold(0.0, f32::max)
    }

    /// Whether the X/Z footprints overlap (separating axis test on the four axes).
    pub fn overlaps_xz(&self, o: &Box3) -> bool {
        for axis in [self.ax, self.az, o.ax, o.az] {
            let proj = |b: &Box3| {
                let c = dot((b.center.x, b.center.z), axis);
                let r = b.half.x * dot(b.ax, axis).abs() + b.half.z * dot(b.az, axis).abs();
                (c - r, c + r)
            };
            let (a0, a1) = proj(self);
            let (b0, b1) = proj(o);
            if a1 <= b0 || b1 <= a0 {
                return false;
            }
        }
        true
    }

    /// Whether the boxes overlap in 3D.
    pub fn overlaps(&self, o: &Box3) -> bool {
        (self.center.y - o.center.y).abs() < self.half.y + o.half.y && self.overlaps_xz(o)
    }

    /// Distance between the X/Z footprints (0 when they overlap).
    pub fn distance_xz(&self, o: &Box3) -> f32 {
        if self.overlaps_xz(o) {
            return 0.0;
        }
        let (ca, cb) = (self.corners(), o.corners());
        let mut best = f32::INFINITY;
        for (p, edges) in ca.iter().map(|p| (p, &cb)).chain(cb.iter().map(|p| (p, &ca))) {
            for i in 0..4 {
                best = best.min(segment_distance(*p, edges[i], edges[(i + 1) % 4]));
            }
        }
        best
    }
}

/// Many boxes (an `avoid` or `spacing` tag set) in a grid of 4 m cells, so a footprint looks only
/// at the boxes near it. Boxes covering more than 64 cells are always looked at.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxSet {
    pub boxes: Vec<Box3>,
    /// ((row, column), box, the box's own first (row, column)): sorted.
    cells: Vec<CellEntry>,
    big: Vec<u32>,
    /// Per box: whether it is in `big` (an O(1) test, not a scan of `big`).
    is_big: Vec<bool>,
}

/// A grid cell (row, column), a box in it, and that box's own first cell.
type CellEntry = ((i32, i32), u32, (i32, i32));

const SET_CELL: f32 = 4.0;
const MAX_SET_CELLS: i64 = 64;

fn xz_rect(b: &Box3) -> (f32, f32, f32, f32) {
    let (mut x0, mut z0, mut x1, mut z1) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for (x, z) in b.corners() {
        x0 = x0.min(x);
        z0 = z0.min(z);
        x1 = x1.max(x);
        z1 = z1.max(z);
    }
    (x0, z0, x1, z1)
}

fn cell_of(v: f32) -> i64 {
    dmath::floor(v / SET_CELL) as i64
}

impl BoxSet {
    pub fn new(boxes: Vec<Box3>) -> BoxSet {
        let mut cells = Vec::new();
        let mut big = Vec::new();
        for (i, b) in boxes.iter().enumerate() {
            let (x0, z0, x1, z1) = xz_rect(b);
            let (cx0, cz0, cx1, cz1) = (cell_of(x0), cell_of(z0), cell_of(x1), cell_of(z1));
            if !(x0.is_finite() && z0.is_finite() && x1.is_finite() && z1.is_finite()) || (cx1 - cx0 + 1) * (cz1 - cz0 + 1) > MAX_SET_CELLS {
                big.push(i as u32);
                continue;
            }
            for cz in cz0..=cz1 {
                for cx in cx0..=cx1 {
                    cells.push(((cz as i32, cx as i32), i as u32, (cz0 as i32, cx0 as i32)));
                }
            }
        }
        cells.sort_unstable();
        let mut is_big = vec![false; boxes.len()];
        for &i in &big {
            is_big[i as usize] = true;
        }
        BoxSet { boxes, cells, big, is_big }
    }

    pub fn len(&self) -> usize {
        self.boxes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// Calls `f` once for each box whose X/Z bounds come within `reach` of `fp`'s (and maybe a
    /// few more), counting every entry looked at in `work`.
    pub fn for_each_near(&self, fp: &Box3, reach: f32, work: &std::cell::Cell<u64>, mut f: impl FnMut(&Box3)) {
        for &i in &self.big {
            work.set(work.get() + 1);
            f(&self.boxes[i as usize]);
        }
        let (x0, z0, x1, z1) = xz_rect(fp);
        let (qx0, qz0, qx1, qz1) = (cell_of(x0 - reach), cell_of(z0 - reach), cell_of(x1 + reach), cell_of(z1 + reach));
        if !(qx0 <= qx1 && qz0 <= qz1) || (qx1 - qx0 + 1) * (qz1 - qz0 + 1) > 4096 {
            // A huge reach: everything.
            for (b, big) in self.boxes.iter().zip(&self.is_big) {
                work.set(work.get() + 1);
                if !big {
                    f(b);
                }
            }
            return;
        }
        for qz in qz0..=qz1 {
            let lo = self.cells.partition_point(|(c, _, _)| *c < (qz as i32, qx0 as i32));
            let hi = self.cells.partition_point(|(c, _, _)| *c <= (qz as i32, qx1 as i32));
            for &((cz, cx), i, (bz0, bx0)) in &self.cells[lo..hi] {
                work.set(work.get() + 1);
                // Each box once: at the first query cell it covers.
                if (cz, cx) == (bz0.max(qz as i32), bx0.max(qx0 as i32)) {
                    f(&self.boxes[i as usize]);
                }
            }
        }
    }
}

/// Distance from `p` to segment `a`-`b` in 2D.
pub fn segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dz * dz;
    let t = if len2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dz) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qz) = (a.0 + dx * t - p.0, a.1 + dz * t - p.1);
    (qx * qx + qz * qz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_sets_find_each_near_box_once() {
        let boxes: Vec<Box3> = (0..400).map(|i| Box3::aligned(Vec3::new((i % 20) as f32 * 3.0, 0.0, (i / 20) as f32 * 3.0), Vec3::new((i % 20) as f32 * 3.0 + 0.5, 1.0, (i / 20) as f32 * 3.0 + 0.5))).collect();
        let set = BoxSet::new(boxes.clone());
        let fp = Box3::aligned(Vec3::new(20.0, 0.0, 20.0), Vec3::new(21.0, 1.0, 21.0));
        let work = std::cell::Cell::new(0);
        let mut seen = Vec::new();
        set.for_each_near(&fp, 5.0, &work, |b| seen.push(b.center.x * 1000.0 + b.center.z));
        let mut sorted = seen.clone();
        sorted.sort_by(f32::total_cmp);
        sorted.dedup();
        assert_eq!(sorted.len(), seen.len(), "no box twice");
        let want = boxes.iter().filter(|b| b.distance_xz(&fp) <= 5.0).count();
        let got = boxes.iter().filter(|b| b.distance_xz(&fp) <= 5.0 && seen.contains(&(b.center.x * 1000.0 + b.center.z))).count();
        assert_eq!(got, want, "every box within reach");
        assert!(work.get() < 100, "looked at {} of 400", work.get());
    }

    #[test]
    fn a_huge_reach_over_many_big_boxes_is_linear() {
        // 8,000 boxes of 40 m (each covers more than the per-box cell cap) and 8,000 small ones.
        let mut boxes: Vec<Box3> = (0..8000).map(|i| Box3::aligned(Vec3::new(5000.0 + (i % 100) as f32 * 50.0, 0.0, (i / 100) as f32 * 50.0), Vec3::new(5040.0 + (i % 100) as f32 * 50.0, 0.1, 40.0 + (i / 100) as f32 * 50.0))).collect();
        boxes.extend((0..8000).map(|i| Box3::aligned(Vec3::new(-5000.0 - (i % 100) as f32 * 2.0, 0.0, (i / 100) as f32 * 2.0), Vec3::new(-4999.5 - (i % 100) as f32 * 2.0, 0.5, 0.5 + (i / 100) as f32 * 2.0))));
        let set = BoxSet::new(boxes);
        let fp = Box3::aligned(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0));
        let work = std::cell::Cell::new(0);
        #[allow(clippy::disallowed_methods, reason = "the test bounds a query's wall time; the query itself never reads the clock")]
        let started = std::time::Instant::now();
        let mut n = 0usize;
        set.for_each_near(&fp, 1.0e4, &work, |_| n += 1);
        assert_eq!(n, 16_000, "every box once");
        assert!(work.get() <= 16_000 + 8_000, "work {}", work.get());
        // A timing, printed; the counted work above is what is asserted.
        eprintln!("huge reach over 16,000 boxes: {:?}", started.elapsed());
    }

    #[test]
    fn boxes() {
        let a = Box3::aligned(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 2.0));
        let b = Box3::aligned(Vec3::new(3.0, 0.0, 0.0), Vec3::new(4.0, 1.0, 1.0));
        assert!(!a.overlaps(&b));
        assert!((a.distance_xz(&b) - 1.0).abs() < 1.0e-6);
        let turned = Box3::footprint(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5), Vec3::new(2.5, 0.0, 1.0), 45.0);
        // Turned 45 degrees, its corner reaches 0.707 toward a.
        assert!(turned.overlaps(&a));
        assert_eq!(a.overhang(&Box3::aligned(Vec3::new(0.5, 0.0, 0.5), Vec3::new(1.5, 1.0, 1.5))), 0.0);
        assert!((a.overhang(&Box3::aligned(Vec3::new(1.5, 0.0, 0.5), Vec3::new(2.5, 1.0, 1.5))) - 0.5).abs() < 1.0e-6);
        assert!((yaw_toward(1.0, 0.0) - 90.0).abs() < 1.0e-4);
        assert!((yaw_difference(350.0, 10.0) - 20.0).abs() < 1.0e-4);
        let (ax, az) = yaw_axes(90.0);
        assert!((ax.0).abs() < 1.0e-6 && (ax.1 + 1.0).abs() < 1.0e-6 && (az.0 - 1.0).abs() < 1.0e-6);
    }
}

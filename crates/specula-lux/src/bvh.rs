//! A bounding volume hierarchy over triangles: binned SAH build, closest-hit and any-hit rays.
//!
//! Built in a fixed order (stable partitions; ties go to the lower index), so the same triangles
//! give the same tree and the same hits on every machine.

use specula_core::math::Vec3;

/// Triangles in a leaf at most.
const LEAF: usize = 4;
/// SAH bins along the split axis.
const BINS: usize = 12;
/// Traversal stack (the build keeps depth below it: past `MAX_DEPTH` it splits at the median).
const STACK: usize = 96;
const MAX_DEPTH: usize = 80;
/// How far (barycentric) a triangle's edges are widened for ray tests ([`Tri::hit`]).
const EDGE: f32 = 1e-4;
/// How far (metres, plus a share of the coordinate) node boxes are grown, so a ray along a box's
/// face or through its edge isn't pruned by rounding before its triangles are tested.
const PAD: f32 = 1e-4;

/// A triangle as the ray test wants it: a corner and its two edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tri {
    pub v0: Vec3,
    pub e1: Vec3,
    pub e2: Vec3,
}

impl Tri {
    pub fn new(a: Vec3, b: Vec3, c: Vec3) -> Tri {
        Tri { v0: a, e1: b - a, e2: c - a }
    }

    fn bounds(&self) -> (Vec3, Vec3) {
        let b = self.v0 + self.e1;
        let c = self.v0 + self.e2;
        (self.v0.min(b).min(c), self.v0.max(b).max(c))
    }

    fn centroid(&self) -> Vec3 {
        self.v0 + (self.e1 + self.e2) * (1.0 / 3.0)
    }

    /// Möller–Trumbore: the hit distance along `d` in `(t_min, t_max)`, two-sided. The edges are
    /// widened by [`EDGE`] (in barycentric units), so a ray through an edge or corner two
    /// triangles share always hits one of them: plain Möller–Trumbore isn't watertight, and a
    /// visibility segment slipping through a box's edge let sky light into a closed room.
    #[inline]
    fn hit(&self, o: Vec3, d: Vec3, t_min: f32, t_max: f32) -> Option<f32> {
        let p = d.cross(self.e2);
        let det = self.e1.dot(p);
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        let s = o - self.v0;
        let u = s.dot(p) * inv;
        if !(-EDGE..=1.0 + EDGE).contains(&u) {
            return None;
        }
        let q = s.cross(self.e1);
        let v = d.dot(q) * inv;
        if v < -EDGE || u + v > 1.0 + EDGE {
            return None;
        }
        let t = self.e2.dot(q) * inv;
        (t > t_min && t < t_max).then_some(t)
    }
}

#[derive(Clone, Copy, Debug)]
struct Node {
    lo: Vec3,
    hi: Vec3,
    /// Leaf: first index into `order`; inner: the left child (the right is `left + 1`).
    start: u32,
    /// Triangles in a leaf; 0 for an inner node.
    count: u32,
}

/// The hierarchy; triangle indices are the caller's.
pub struct Bvh {
    tris: Vec<Tri>,
    nodes: Vec<Node>,
    /// Triangle indices, leaves' ranges into it.
    order: Vec<u32>,
}

/// The nearest hit of a ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub t: f32,
    pub tri: u32,
}

fn area(lo: Vec3, hi: Vec3) -> f32 {
    let d = hi - lo;
    if d.x < 0.0 {
        return 0.0;
    }
    2.0 * (d.x * d.y + d.y * d.z + d.z * d.x)
}

fn axis(v: Vec3, a: usize) -> f32 {
    match a {
        0 => v.x,
        1 => v.y,
        _ => v.z,
    }
}

const EMPTY_LO: Vec3 = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
const EMPTY_HI: Vec3 = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

impl Bvh {
    pub fn build(tris: Vec<Tri>) -> Bvh {
        Bvh::build_with(tris, &|| false).expect("never cancelled")
    }

    /// [`Bvh::build`], stopping (`None`) once `cancelled` says so (checked every 256 nodes).
    pub fn build_with(tris: Vec<Tri>, cancelled: &dyn Fn() -> bool) -> Option<Bvh> {
        let n = tris.len();
        let bounds: Vec<(Vec3, Vec3)> = tris.iter().map(Tri::bounds).collect();
        let centroids: Vec<Vec3> = tris.iter().map(Tri::centroid).collect();
        let mut order: Vec<u32> = (0..n as u32).collect();
        let mut nodes = vec![Node { lo: EMPTY_LO, hi: EMPTY_HI, start: 0, count: n as u32 }];
        // (node, start, end, depth)
        let mut work = vec![(0usize, 0usize, n, 0usize)];
        let mut scratch: Vec<u32> = Vec::with_capacity(n);
        let mut steps = 0u32;
        while let Some((ni, start, end, depth)) = work.pop() {
            steps = steps.wrapping_add(1);
            if steps % 256 == 0 && cancelled() {
                return None;
            }
            let (mut lo, mut hi) = (EMPTY_LO, EMPTY_HI);
            let (mut clo, mut chi) = (EMPTY_LO, EMPTY_HI);
            for &i in &order[start..end] {
                let (a, b) = bounds[i as usize];
                lo = lo.min(a);
                hi = hi.max(b);
                let c = centroids[i as usize];
                clo = clo.min(c);
                chi = chi.max(c);
            }
            let pad = |v: Vec3, k: f32| Vec3::new(v.x + k * (PAD + v.x.abs() * 1e-6), v.y + k * (PAD + v.y.abs() * 1e-6), v.z + k * (PAD + v.z.abs() * 1e-6));
            nodes[ni].lo = pad(lo, -1.0);
            nodes[ni].hi = pad(hi, 1.0);
            let count = end - start;
            if count <= LEAF {
                nodes[ni].start = start as u32;
                nodes[ni].count = count as u32;
                continue;
            }
            let ext = chi - clo;
            let ax = if ext.x >= ext.y && ext.x >= ext.z {
                0
            } else if ext.y >= ext.z {
                1
            } else {
                2
            };
            let (a0, a1) = (axis(clo, ax), axis(chi, ax));
            let mut mid = None;
            if a1 > a0 && depth < MAX_DEPTH {
                // Binned SAH along the widest centroid axis.
                let scale = BINS as f32 / (a1 - a0);
                let bin_of = |i: u32| (((axis(centroids[i as usize], ax) - a0) * scale) as usize).min(BINS - 1);
                let mut bins = [(EMPTY_LO, EMPTY_HI, 0usize); BINS];
                for &i in &order[start..end] {
                    let b = &mut bins[bin_of(i)];
                    let (lo_t, hi_t) = bounds[i as usize];
                    b.0 = b.0.min(lo_t);
                    b.1 = b.1.max(hi_t);
                    b.2 += 1;
                }
                let mut right = [(0.0f32, 0usize); BINS];
                let (mut rlo, mut rhi, mut rn) = (EMPTY_LO, EMPTY_HI, 0);
                for k in (1..BINS).rev() {
                    rlo = rlo.min(bins[k].0);
                    rhi = rhi.max(bins[k].1);
                    rn += bins[k].2;
                    right[k] = (area(rlo, rhi), rn);
                }
                let (mut llo, mut lhi, mut ln) = (EMPTY_LO, EMPTY_HI, 0);
                let mut best = (area(lo, hi) * count as f32, None);
                for k in 0..BINS - 1 {
                    llo = llo.min(bins[k].0);
                    lhi = lhi.max(bins[k].1);
                    ln += bins[k].2;
                    let (ra, rcount) = right[k + 1];
                    if ln == 0 || rcount == 0 {
                        continue;
                    }
                    let cost = area(llo, lhi) * ln as f32 + ra * rcount as f32;
                    if cost < best.0 {
                        best = (cost, Some(k));
                    }
                }
                if let Some(k) = best.1 {
                    // Stable partition: bins ≤ k first, each side in its old order.
                    scratch.clear();
                    scratch.extend(order[start..end].iter().copied().filter(|&i| bin_of(i) <= k));
                    let left = scratch.len();
                    scratch.extend(order[start..end].iter().copied().filter(|&i| bin_of(i) > k));
                    order[start..end].copy_from_slice(&scratch);
                    mid = Some(start + left);
                } else if count > 2 * LEAF {
                    mid = Some(start + count / 2); // no split pays off, but the leaf would be big
                }
            } else if count > LEAF {
                mid = Some(start + count / 2); // all centroids equal, or too deep: split in half
            }
            match mid {
                Some(m) if m > start && m < end => {
                    let left = nodes.len();
                    nodes.push(Node { lo: EMPTY_LO, hi: EMPTY_HI, start: 0, count: 0 });
                    nodes.push(Node { lo: EMPTY_LO, hi: EMPTY_HI, start: 0, count: 0 });
                    nodes[ni].start = left as u32;
                    nodes[ni].count = 0;
                    work.push((left + 1, m, end, depth + 1));
                    work.push((left, start, m, depth + 1));
                }
                _ => {
                    nodes[ni].start = start as u32;
                    nodes[ni].count = count as u32;
                }
            }
        }
        Some(Bvh { tris, nodes, order })
    }

    pub fn len(&self) -> usize {
        self.tris.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    pub fn nodes(&self) -> usize {
        self.nodes.len()
    }

    pub fn tri(&self, i: u32) -> &Tri {
        &self.tris[i as usize]
    }

    #[inline]
    fn slab(n: &Node, o: Vec3, inv: Vec3, t_max: f32) -> Option<f32> {
        let t0 = (n.lo - o).mul_each(inv);
        let t1 = (n.hi - o).mul_each(inv);
        let near = t0.min(t1);
        let far = t0.max(t1);
        let t_near = near.x.max(near.y).max(near.z).max(0.0);
        let t_far = far.x.min(far.y).min(far.z).min(t_max);
        (t_near <= t_far).then_some(t_near)
    }

    /// The nearest hit along unit `d` from `o` beyond `t_min` and before `t_max`.
    pub fn closest(&self, o: Vec3, d: Vec3, t_min: f32, t_max: f32) -> Option<Hit> {
        if self.tris.is_empty() {
            return None;
        }
        let inv = inverse(d);
        let mut best: Option<Hit> = None;
        let mut t_far = t_max;
        let mut stack = [0u32; STACK];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let n = &self.nodes[stack[sp] as usize];
            if Self::slab(n, o, inv, t_far).is_none() {
                continue;
            }
            if n.count > 0 {
                for &ti in &self.order[n.start as usize..(n.start + n.count) as usize] {
                    if let Some(t) = self.tris[ti as usize].hit(o, d, t_min, t_far) {
                        // Equal distances go to the lower triangle index (order-independent).
                        if best.is_none_or(|b| t < b.t || (t == b.t && ti < b.tri)) {
                            best = Some(Hit { t, tri: ti });
                            t_far = t;
                        }
                    }
                }
            } else {
                // Nearer child last (popped first).
                let (l, r) = (n.start, n.start + 1);
                let tl = Self::slab(&self.nodes[l as usize], o, inv, t_far);
                let tr = Self::slab(&self.nodes[r as usize], o, inv, t_far);
                let mut push = |i: u32| {
                    if sp < STACK {
                        stack[sp] = i;
                        sp += 1;
                    }
                };
                match (tl, tr) {
                    (Some(a), Some(b)) => {
                        if a <= b {
                            push(r);
                            push(l);
                        } else {
                            push(l);
                            push(r);
                        }
                    }
                    (Some(_), None) => push(l),
                    (None, Some(_)) => push(r),
                    (None, None) => {}
                }
            }
        }
        best
    }

    /// Whether anything lies along unit `d` from `o` between `t_min` and `t_max`.
    /// Whether any triangle's box overlaps the box `lo..hi` (conservative: a slanted triangle
    /// whose box reaches in counts).
    pub fn overlaps_box(&self, lo: Vec3, hi: Vec3) -> bool {
        if self.tris.is_empty() {
            return false;
        }
        let meets = |a: Vec3, b: Vec3| a.x <= hi.x && b.x >= lo.x && a.y <= hi.y && b.y >= lo.y && a.z <= hi.z && b.z >= lo.z;
        let mut stack = [0u32; STACK];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let n = &self.nodes[stack[sp] as usize];
            if !meets(n.lo, n.hi) {
                continue;
            }
            if n.count > 0 {
                for &ti in &self.order[n.start as usize..(n.start + n.count) as usize] {
                    let (a, b) = self.tris[ti as usize].bounds();
                    if meets(a, b) {
                        return true;
                    }
                }
            } else if sp + 2 <= STACK {
                stack[sp] = n.start;
                stack[sp + 1] = n.start + 1;
                sp += 2;
            }
        }
        false
    }

    pub fn occluded(&self, o: Vec3, d: Vec3, t_min: f32, t_max: f32) -> bool {
        if self.tris.is_empty() {
            return false;
        }
        let inv = inverse(d);
        let mut stack = [0u32; STACK];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let n = &self.nodes[stack[sp] as usize];
            if Self::slab(n, o, inv, t_max).is_none() {
                continue;
            }
            if n.count > 0 {
                for &ti in &self.order[n.start as usize..(n.start + n.count) as usize] {
                    if self.tris[ti as usize].hit(o, d, t_min, t_max).is_some() {
                        return true;
                    }
                }
            } else if sp + 2 <= STACK {
                stack[sp] = n.start;
                stack[sp + 1] = n.start + 1;
                sp += 2;
            }
        }
        false
    }
}

fn inverse(d: Vec3) -> Vec3 {
    let f = |x: f32| if x.abs() < 1e-20 { if x < 0.0 { -1e20 } else { 1e20 } } else { 1.0 / x };
    Vec3::new(f(d.x), f(d.y), f(d.z))
}

/// Component-wise products.
trait MulEach {
    fn mul_each(self, o: Vec3) -> Vec3;
}

impl MulEach for Vec3 {
    #[inline]
    fn mul_each(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(tris: &mut Vec<Tri>, c: Vec3, u: Vec3, v: Vec3) {
        tris.push(Tri::new(c - u - v, c + u - v, c + u + v));
        tris.push(Tri::new(c - u - v, c + u + v, c - u + v));
    }

    /// A grid of small quads plus a far wall: enough triangles for several levels.
    fn scene() -> Vec<Tri> {
        let mut t = Vec::new();
        for i in 0..20 {
            for j in 0..20 {
                quad(&mut t, Vec3::new(i as f32, 0.0, j as f32), Vec3::new(0.5, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.5));
            }
        }
        quad(&mut t, Vec3::new(10.0, 5.0, 30.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 5.0, 0.0));
        t
    }

    fn brute(tris: &[Tri], o: Vec3, d: Vec3) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        for (i, t) in tris.iter().enumerate() {
            if let Some(h) = t.hit(o, d, 1e-4, f32::MAX) {
                if best.is_none_or(|b| h < b.t) {
                    best = Some(Hit { t: h, tri: i as u32 });
                }
            }
        }
        best
    }

    #[test]
    fn the_tree_finds_what_brute_force_finds() {
        let tris = scene();
        let bvh = Bvh::build(tris.clone());
        assert!(bvh.nodes() > 10);
        let dirs = crate::sh::fibonacci(300);
        for (k, d) in dirs.iter().enumerate() {
            let o = Vec3::new(3.3 + (k % 7) as f32, 2.0, 4.1 + (k % 5) as f32);
            let a = bvh.closest(o, *d, 1e-4, f32::MAX);
            let b = brute(&tris, o, *d);
            assert_eq!(a.map(|h| h.t), b.map(|h| h.t), "ray {k}");
            assert_eq!(bvh.occluded(o, *d, 1e-4, f32::MAX), b.is_some());
        }
    }

    #[test]
    fn rays_stop_at_t_max_and_miss_an_empty_scene() {
        let bvh = Bvh::build(scene());
        let down = Vec3::new(0.0, -1.0, 0.0);
        assert!(bvh.closest(Vec3::new(5.0, 2.0, 5.0), down, 1e-4, 1.5).is_none());
        assert!((bvh.closest(Vec3::new(5.0, 2.0, 5.0), down, 1e-4, 3.0).unwrap().t - 2.0).abs() < 1e-5);
        let empty = Bvh::build(Vec::new());
        assert!(empty.closest(Vec3::ZERO, down, 0.0, 10.0).is_none() && !empty.occluded(Vec3::ZERO, down, 0.0, 10.0));
    }

    /// The six faces of the box `lo..hi`, two triangles each (as the engine's cubes come).
    fn boxed(tris: &mut Vec<Tri>, lo: Vec3, hi: Vec3) {
        let c = |x: bool, y: bool, z: bool| Vec3::new(if x { hi.x } else { lo.x }, if y { hi.y } else { lo.y }, if z { hi.z } else { lo.z });
        let faces = [
            [c(true, false, false), c(true, true, false), c(true, true, true), c(true, false, true)],
            [c(false, false, false), c(false, false, true), c(false, true, true), c(false, true, false)],
            [c(false, true, false), c(false, true, true), c(true, true, true), c(true, true, false)],
            [c(false, false, false), c(true, false, false), c(true, false, true), c(false, false, true)],
            [c(false, false, true), c(true, false, true), c(true, true, true), c(false, true, true)],
            [c(false, false, false), c(false, true, false), c(true, true, false), c(true, false, false)],
        ];
        for q in faces {
            tris.push(Tri::new(q[0], q[1], q[2]));
            tris.push(Tri::new(q[0], q[2], q[3]));
        }
    }

    #[test]
    fn segments_through_shared_edges_and_corners_are_occluded() {
        // The review's probe: from inside a wall box out through its vertical corner edge at 45
        // degrees (plain Moller-Trumbore missed both triangles there).
        let mut t = Vec::new();
        boxed(&mut t, Vec3::new(3.2, 0.0, 0.0), Vec3::new(3.4, 2.4, 3.4));
        let bvh = Bvh::build(t);
        let seg = |o: Vec3, e: Vec3| {
            let d = e - o;
            let l = d.length();
            bvh.occluded(o, d * (1.0 / l), 1e-4, l - 1e-4)
        };
        assert!(seg(Vec3::new(3.25, 0.25, 3.25), Vec3::new(3.5, 0.5, 3.5)), "through the corner edge");
        assert!(seg(Vec3::new(3.25, 0.25, 3.25), Vec3::new(3.5, 0.5, 3.45)), "beside it");
        // Every segment from inside the box to points around it leaves through a face, an edge or
        // a corner: all occluded, including those aimed exactly at edges and corners.
        let centre = Vec3::new(3.3, 1.2, 1.7);
        let mut n = 0;
        for xi in 0..9 {
            for yi in 0..9 {
                for zi in 0..9 {
                    let e = Vec3::new(3.0 + 0.05 * xi as f32, -0.2 + 0.35 * yi as f32, -0.2 + 0.475 * zi as f32);
                    let inside = (3.2..=3.4).contains(&e.x) && (0.0..=2.4).contains(&e.y) && (0.0..=3.4).contains(&e.z);
                    if !inside {
                        assert!(seg(centre, e), "from the box's middle to {e:?}");
                        // And from just inside the corner edge (the octant centres the masks use).
                        assert!(seg(Vec3::new(3.39, 2.39, 3.39), e) || (e - Vec3::new(3.39, 2.39, 3.39)).length() < 0.02, "from by the corner to {e:?}");
                        n += 1;
                    }
                }
            }
        }
        assert!(n > 400, "{n}");
        // Two boxes sharing a face, a segment along the seam from outside to outside: open air
        // on both sides of the line is not blocked by the widened edges.
        let mut t = Vec::new();
        boxed(&mut t, Vec3::ZERO, Vec3::ONE);
        let bvh = Bvh::build(t);
        let d = Vec3::new(1.0, 0.0, 0.0);
        assert!(!bvh.occluded(Vec3::new(-1.0, 0.5, 1.01), d, 1e-4, 3.0), "a centimetre past the face");
    }

    #[test]
    fn box_queries_find_what_overlaps() {
        let mut t = Vec::new();
        boxed(&mut t, Vec3::new(3.2, 0.0, 0.0), Vec3::new(3.4, 2.4, 3.4));
        let bvh = Bvh::build(t);
        assert!(bvh.overlaps_box(Vec3::new(3.0, 1.0, 1.0), Vec3::new(3.25, 1.5, 1.5)), "reaches into the wall");
        assert!(!bvh.overlaps_box(Vec3::new(3.25, 1.0, 1.0), Vec3::new(3.3, 1.5, 1.5)), "wholly inside the solid: no face reaches in (no surface point samples there)");
        assert!(!bvh.overlaps_box(Vec3::new(2.0, 1.0, 1.0), Vec3::new(3.1, 1.5, 1.5)), "clear of it");
        assert!(!Bvh::build(Vec::new()).overlaps_box(Vec3::ZERO, Vec3::ONE));
    }

    #[test]
    fn degenerate_input_still_builds() {
        // Every triangle at the same place: no SAH split exists; halves keep leaves small.
        let t = vec![Tri::new(Vec3::ZERO, Vec3::X, Vec3::Y); 100];
        let bvh = Bvh::build(t);
        assert!(bvh.closest(Vec3::new(0.2, 0.2, 1.0), Vec3::new(0.0, 0.0, -1.0), 1e-4, 10.0).is_some());
    }
}

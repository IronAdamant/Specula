//! Tier-4 scatter: Poisson-disc placement with rules, deterministic in its seed.
//!
//! Bridson's algorithm ("Fast Poisson Disk Sampling in Arbitrary Dimensions", 2007) over an
//! area (rectangle, circle or polygon in XZ), then rules: ground slope and height ranges, a
//! noise mask, a splat layer's weight, and circles to keep clear. Survivors get an item (by
//! weight), a yaw, a scale, a tilt and an optional lean towards the ground's normal, all from
//! one seeded stream in a fixed order, so the same spec and ground always give the same
//! placements. Rules run as each sample is accepted, so a scatter stops at its count; polygon
//! areas are tested on a grid of their edges; everything is charged to a work budget. Used by recipes (the `scatter` generator merges copies into one mesh) and by the
//! `procgen_scatter` tool (which spawns entities, one model each, so the GPU instances them).

use specula_core::rng::Rng;

use crate::math3::{self, Affine, V2, V3};
use crate::noise;

/// Most placements one scatter makes.
pub const MAX_POINTS: usize = 4096;
/// Most candidates tried around each accepted point.
pub const MAX_ATTEMPTS: u32 = 64;
/// Most circles to keep clear.
pub const MAX_AVOID: usize = 256;
/// Most cells of the acceleration grid.
pub const MAX_GRID_CELLS: usize = 1 << 20;
/// Most work units one scatter may spend on its own ([`crate::work`]; a recipe's scatter gets
/// what is left of its build's budget).
pub const MAX_WORK: u64 = crate::work::MAX_BUILD;

#[derive(Clone, Debug, PartialEq)]
pub enum Area {
    /// Centred on the origin: width (x) and depth (z).
    Rect(V2),
    Circle(f32),
    /// A simple polygon of [x, z] points.
    Polygon(Vec<V2>),
}

/// A noise mask: keep points where fBm noise (0..1) is above `threshold`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mask {
    pub scale: f32,
    pub threshold: f32,
    pub octaves: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScatterSpec {
    pub area: Area,
    /// Offset of the area's centre (x, z).
    pub offset: V2,
    pub min_distance: f32,
    pub max_count: u32,
    pub seed: u64,
    pub attempts: u32,
    /// Allowed ground slope (degrees) and height (metres).
    pub slope: Option<V2>,
    pub height: Option<V2>,
    pub mask: Option<Mask>,
    /// A splat layer (0..3) and the least weight (0..1) it must have there.
    pub splat: Option<(usize, f32)>,
    /// Circles (x, z, radius) to keep clear.
    pub avoid: Vec<[f32; 3]>,
    /// Item weights (an item index per placement); empty = one item.
    pub weights: Vec<f32>,
    pub scale: V2,
    /// Yaw range (degrees).
    pub yaw: V2,
    /// Random lean, up to this many degrees.
    pub tilt: f32,
    /// 0..1: how far up follows the ground's normal.
    pub align: f32,
    /// Sink into the ground (metres).
    pub sink: f32,
}

/// What the ground under a point is: its height, normal, and splat weights if any.
pub struct Ground {
    pub height: f32,
    pub normal: V3,
    pub splat: Option<[u8; 4]>,
}

/// One placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub position: V3,
    /// Rotation as Euler degrees (X, then Y, then Z: the engine's order).
    pub rotation_deg: V3,
    pub scale: f32,
    pub item: u32,
    /// A seed for this placement's variant.
    pub variant: u64,
}

impl Placement {
    pub fn transform(&self) -> Affine {
        let r = self.rotation_deg;
        Affine::translate(self.position).then_after(&Affine::rotate_euler_deg([r[0] as f64, r[1] as f64, r[2] as f64]).then_after(&Affine::scale([self.scale; 3])))
    }
}

/// What a scatter did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScatterStats {
    pub candidates: u64,
    /// Work units spent ([`crate::work`]).
    pub work: u64,
    pub sampled: usize,
    /// Rejected by each rule: outside the area/ground, slope, height, mask, splat, avoid.
    pub rejected: [usize; 6],
    pub placed: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScatterError {
    pub field: &'static str,
    pub message: String,
    pub limit: bool,
}

impl ScatterSpec {
    pub fn check(&self) -> Result<(), ScatterError> {
        let bad = |field, m: String| Err(ScatterError { field, message: m, limit: false });
        if !(self.min_distance > 0.0 && self.min_distance.is_finite()) {
            return bad("min_distance", "min_distance must be > 0 m".into());
        }
        if self.max_count as usize > MAX_POINTS {
            return Err(ScatterError { field: "count", message: format!("at most {MAX_POINTS} placements"), limit: true });
        }
        if self.avoid.len() > MAX_AVOID {
            return Err(ScatterError { field: "avoid", message: format!("{} circles to avoid; at most {MAX_AVOID}", self.avoid.len()), limit: true });
        }
        if !(1..=MAX_ATTEMPTS).contains(&self.attempts) {
            return bad("attempts", format!("attempts must be 1..{MAX_ATTEMPTS}"));
        }
        let (lo, hi) = self.bounds();
        let cells = (((hi[0] - lo[0]) / (self.min_distance / 2f32.sqrt())) as f64 + 1.0) * (((hi[1] - lo[1]) / (self.min_distance / 2f32.sqrt())) as f64 + 1.0);
        if !(cells.is_finite() && cells <= MAX_GRID_CELLS as f64) {
            return Err(ScatterError {
                field: "min_distance",
                message: format!("the area is too large for min_distance {} (more than {MAX_GRID_CELLS} grid cells); raise min_distance", self.min_distance),
                limit: true,
            });
        }
        if let Area::Polygon(p) = &self.area {
            if p.len() < 3 || p.len() > 1024 {
                return bad("area", "a polygon area needs 3..1024 points".into());
            }
        }
        if self.weights.iter().any(|w| !(*w >= 0.0 && w.is_finite())) || (!self.weights.is_empty() && self.weights.iter().sum::<f32>() <= 0.0) {
            return bad("items", "item weights must be >= 0 with a positive sum".into());
        }
        if !(self.scale[0] > 0.0 && self.scale[1] >= self.scale[0]) {
            return bad("scale", "scale is [min, max] with 0 < min <= max".into());
        }
        Ok(())
    }

    /// The area's bounds (x, z), offset included.
    fn bounds(&self) -> (V2, V2) {
        let (lo, hi) = match &self.area {
            Area::Rect(s) => ([-s[0] * 0.5, -s[1] * 0.5], [s[0] * 0.5, s[1] * 0.5]),
            Area::Circle(r) => ([-r, -r], [*r, *r]),
            Area::Polygon(p) => p.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), q| ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])])),
        };
        ([lo[0] + self.offset[0], lo[1] + self.offset[1]], [hi[0] + self.offset[0], hi[1] + self.offset[1]])
    }

    fn inside(&self, p: V2) -> bool {
        let q = [p[0] - self.offset[0], p[1] - self.offset[1]];
        match &self.area {
            Area::Rect(s) => q[0].abs() <= s[0] * 0.5 && q[1].abs() <= s[1] * 0.5,
            Area::Circle(r) => q[0] * q[0] + q[1] * q[1] <= r * r,
            Area::Polygon(poly) => {
                let mut c = false;
                for i in 0..poly.len() {
                    let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                    if (a[1] > q[1]) != (b[1] > q[1]) && q[0] < a[0] + (b[0] - a[0]) * (q[1] - a[1]) / (b[1] - a[1]) {
                        c = !c;
                    }
                }
                c
            }
        }
    }
}

/// A polygon's inside test on a grid: each cell lists the edges that touch it (conservatively,
/// a cell's margin included) and knows whether its centre is inside, so a point is classified
/// against its cell's few edges (the parity flips once per edge crossed on the way from the
/// centre) instead of all of them. Every parity uses one crossing rule ([`crosses`]), so the
/// centres and the points agree.
struct PolyGrid {
    lo: V2,
    cell: V2,
    n: usize,
    /// CSR: cell -> edges.
    start: Vec<u32>,
    edges: Vec<u32>,
    centre_inside: Vec<bool>,
}

/// Whether segment `c`-`p` crosses edge `a`-`b`, every zero orientation taken as positive (a
/// symbolic nudge: a vertex on the segment counts on one side only, so parities stay right).
fn crosses(c: V2, p: V2, a: V2, b: V2) -> bool {
    let side = |o: V2, d: V2, q: V2| -> bool { (d[0] - o[0]) * (q[1] - o[1]) - (d[1] - o[1]) * (q[0] - o[0]) >= 0.0 };
    side(c, p, a) != side(c, p, b) && side(a, b, c) != side(a, b, p)
}

impl PolyGrid {
    /// The grid and the work it took (edge-cell visits and centre tests).
    fn new(poly: &[V2]) -> (PolyGrid, u64) {
        let (lo, hi) = poly.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), q| ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])]));
        // About four cells per edge along each side, 8..256.
        let n = ((poly.len() as f32).sqrt() * 4.0).clamp(8.0, 256.0) as usize;
        let cell = [(hi[0] - lo[0]).max(1e-6) / n as f32, (hi[1] - lo[1]).max(1e-6) / n as f32];
        let ix = |v: f32, k: usize| -> usize { (crate::dmath::floor((v - lo[k]) / cell[k]).max(0.0) as usize).min(n - 1) };
        let mut lists: Vec<Vec<u32>> = vec![Vec::new(); n * n];
        let mut work = 0u64;
        for e in 0..poly.len() {
            let (a, b) = (poly[e], poly[(e + 1) % poly.len()]);
            let (zmin, zmax) = (a[1].min(b[1]), a[1].max(b[1]));
            let x_at = |z: f32| -> f32 {
                if zmax - zmin < 1e-12 {
                    a[0]
                } else {
                    a[0] + (b[0] - a[0]) * ((z.clamp(zmin, zmax) - a[1]) / (b[1] - a[1]))
                }
            };
            // Each row band the edge reaches (and the rows either side), its x span there
            // widened by a cell either side.
            for r in ix(zmin, 1).saturating_sub(1)..=(ix(zmax, 1) + 1).min(n - 1) {
                let (xa, xb) = (x_at(lo[1] + r as f32 * cell[1]), x_at(lo[1] + (r + 1) as f32 * cell[1]));
                let (xa, xb) = (xa.min(xb).min(a[0].max(b[0])).max(a[0].min(b[0])), xa.max(xb).min(a[0].max(b[0])).max(a[0].min(b[0])));
                for c in ix(xa, 0).saturating_sub(1)..=(ix(xb, 0) + 1).min(n - 1) {
                    lists[r * n + c].push(e as u32);
                    work += 1;
                }
            }
        }
        let mut start = Vec::with_capacity(lists.len() + 1);
        let mut edges = Vec::new();
        start.push(0u32);
        for l in &lists {
            edges.extend_from_slice(l);
            start.push(edges.len() as u32);
        }
        let mut grid = PolyGrid { lo, cell, n, start, edges, centre_inside: vec![false; n * n] };
        // Centres, right to left along each row: a point past the bounds is outside, and each
        // step only crosses the edges of the two cells it runs through.
        let mut seen: Vec<u32> = Vec::new();
        for r in 0..n {
            let mut prev = [hi[0] + cell[0], lo[1] + (r as f32 + 0.5) * cell[1]];
            let mut inside = false;
            for c in (0..n).rev() {
                let here = grid.centre(c, r);
                seen.clear();
                seen.extend_from_slice(grid.cell_edges(r * n + c));
                if c + 1 < n {
                    seen.extend_from_slice(grid.cell_edges(r * n + c + 1));
                }
                seen.sort_unstable();
                seen.dedup();
                for &e in &seen {
                    work += 1;
                    if crosses(prev, here, poly[e as usize], poly[(e as usize + 1) % poly.len()]) {
                        inside = !inside;
                    }
                }
                grid.centre_inside[r * n + c] = inside;
                prev = here;
            }
        }
        (grid, work)
    }

    fn centre(&self, c: usize, r: usize) -> V2 {
        [self.lo[0] + (c as f32 + 0.5) * self.cell[0], self.lo[1] + (r as f32 + 0.5) * self.cell[1]]
    }

    fn cell_edges(&self, ci: usize) -> &[u32] {
        &self.edges[self.start[ci] as usize..self.start[ci + 1] as usize]
    }

    /// Whether `p` is inside, and how many edges that took.
    fn inside(&self, poly: &[V2], p: V2) -> (bool, u64) {
        let c = (crate::dmath::floor((p[0] - self.lo[0]) / self.cell[0]).max(0.0) as usize).min(self.n - 1);
        let r = (crate::dmath::floor((p[1] - self.lo[1]) / self.cell[1]).max(0.0) as usize).min(self.n - 1);
        let centre = self.centre(c, r);
        let mut inside = self.centre_inside[r * self.n + c];
        let list = self.cell_edges(r * self.n + c);
        for &e in list {
            if crosses(centre, p, poly[e as usize], poly[(e as usize + 1) % poly.len()]) {
                inside = !inside;
            }
        }
        (inside, list.len() as u64)
    }
}

/// Places points. `ground(x, z)` gives the ground there (`None`: no ground, rejected); a flat
/// ground is [`flat`]. Stops at `max_count` placements; everything it does is charged to
/// `max_work` work units ([`crate::work`]).
pub fn scatter(spec: &ScatterSpec, ground: &dyn Fn(f32, f32) -> Option<Ground>, max_work: u64) -> Result<(Vec<Placement>, ScatterStats), ScatterError> {
    spec.check()?;
    let mut st = ScatterStats::default();
    let charge = |st: &mut ScatterStats, w: u64| -> Result<(), ScatterError> {
        st.work += w;
        if st.work > max_work {
            return Err(ScatterError { field: "count", message: format!("the scatter needs more than {max_work} work units (candidates and the polygon edges they're tested against); use fewer points, a simpler area or a larger min_distance"), limit: true });
        }
        if crate::work::cancelled() {
            return Err(ScatterError { field: "count", message: crate::work::CANCELLED.into(), limit: true });
        }
        Ok(())
    };
    let (lo, hi) = spec.bounds();
    let r = spec.min_distance;
    let cs = r / 2f32.sqrt();
    let gw = (((hi[0] - lo[0]) / cs) as usize + 1).max(1);
    let gh = (((hi[1] - lo[1]) / cs) as usize + 1).max(1);
    charge(&mut st, (gw * gh) as u64 * crate::work::SCATTER_CELL)?;
    let mut grid = vec![u32::MAX; gw * gh];
    let cell = |p: V2| -> (usize, usize) { ((((p[0] - lo[0]) / cs) as usize).min(gw - 1), (((p[1] - lo[1]) / cs) as usize).min(gh - 1)) };
    let poly = match &spec.area {
        Area::Polygon(p) => {
            let local: Vec<V2> = p.iter().map(|q| [q[0] + spec.offset[0], q[1] + spec.offset[1]]).collect();
            let (g, w) = PolyGrid::new(&local);
            charge(&mut st, w * crate::work::SCATTER_GRID_EDGE)?;
            Some((local, g))
        }
        _ => None,
    };
    // Inside the area, charged by the edges it took.
    let inside = |st: &mut ScatterStats, p: V2| -> Result<bool, ScatterError> {
        let (yes, edges) = match &poly {
            Some((pts, g)) => g.inside(pts, p),
            None => (spec.inside(p), 0),
        };
        charge(st, crate::work::SCATTER_CANDIDATE + edges * crate::work::SCATTER_EDGE)?;
        Ok(yes)
    };
    // Samples come from one stream, the placements' attributes from another, so stopping at
    // `max_count` never changes the placements made before it.
    let mut rng = Rng::new(spec.seed, 0x5343_4154);
    let mut attrs = Rng::new(spec.seed, 0x4154_5452);
    let mut pts: Vec<V2> = Vec::new();
    let mut active: Vec<u32> = Vec::new();
    let mut out = Vec::new();
    let wsum: f32 = spec.weights.iter().sum();
    let mask_cost = spec.mask.map_or(0, |m| 90 * m.octaves.clamp(1, 8) as u64);
    let rule_cost = 300 + mask_cost + 4 * spec.avoid.len() as u64;
    // A sample: the rules, then a placement with its attributes.
    let mut rules = |st: &mut ScatterStats, p: V2, out: &mut Vec<Placement>| -> Result<(), ScatterError> {
        charge(st, rule_cost)?;
        let Some(g) = ground(p[0], p[1]) else {
            st.rejected[0] += 1;
            return Ok(());
        };
        if let Some(sl) = spec.slope {
            let deg = crate::dmath::acos(g.normal[1].clamp(-1.0, 1.0)) * (180.0 / std::f32::consts::PI);
            if deg < sl[0] || deg > sl[1] {
                st.rejected[1] += 1;
                return Ok(());
            }
        }
        if let Some(h) = spec.height {
            if g.height < h[0] || g.height > h[1] {
                st.rejected[2] += 1;
                return Ok(());
            }
        }
        if let Some(m) = spec.mask {
            let s = 1.0 / m.scale.max(1e-4);
            let v = noise::fbm3(spec.seed ^ 0x4d41_534b, [p[0] * s, 0.0, p[1] * s], m.octaves.clamp(1, 8)) * 0.5 + 0.5;
            if v < m.threshold {
                st.rejected[3] += 1;
                return Ok(());
            }
        }
        if let Some((layer, least)) = spec.splat {
            let w = g.splat.map_or(0.0, |s| s[layer.min(3)] as f32 / 255.0);
            if w < least {
                st.rejected[4] += 1;
                return Ok(());
            }
        }
        if spec.avoid.iter().any(|a| {
            let (dx, dz) = (p[0] - a[0], p[1] - a[1]);
            dx * dx + dz * dz < a[2] * a[2]
        }) {
            st.rejected[5] += 1;
            return Ok(());
        }
        let item = if spec.weights.len() > 1 {
            let mut t = attrs.f32() * wsum;
            let mut chosen = spec.weights.len() - 1;
            for (i, w) in spec.weights.iter().enumerate() {
                if t < *w {
                    chosen = i;
                    break;
                }
                t -= w;
            }
            chosen as u32
        } else {
            0
        };
        let yaw = spec.yaw[0] + attrs.f32() * (spec.yaw[1] - spec.yaw[0]);
        let scale = spec.scale[0] + attrs.f32() * (spec.scale[1] - spec.scale[0]);
        let (tx, tz) = ((attrs.f32() * 2.0 - 1.0) * spec.tilt, (attrs.f32() * 2.0 - 1.0) * spec.tilt);
        // Lean towards the ground: the normal's tilt as X and Z rotations, scaled by `align`.
        let n = g.normal;
        let lean_x = crate::dmath::atan2(n[2], n[1]) * (180.0 / std::f32::consts::PI) * spec.align;
        let lean_z = -crate::dmath::atan2(n[0], n[1]) * (180.0 / std::f32::consts::PI) * spec.align;
        let variant = attrs.next_u64();
        out.push(Placement { position: [p[0], g.height - spec.sink, p[1]], rotation_deg: [lean_x + tx, yaw, lean_z + tz], scale, item, variant });
        Ok(())
    };
    let far = |pts: &[V2], grid: &[u32], p: V2| -> bool {
        let (ci, cj) = cell(p);
        for j in cj.saturating_sub(2)..(cj + 3).min(gh) {
            for i in ci.saturating_sub(2)..(ci + 3).min(gw) {
                let k = grid[j * gw + i];
                if k != u32::MAX {
                    let q = pts[k as usize];
                    let (dx, dz) = (q[0] - p[0], q[1] - p[1]);
                    if dx * dx + dz * dz < r * r {
                        return false;
                    }
                }
            }
        }
        true
    };
    let want = spec.max_count as usize;
    // The first point: a random one inside the area (a few tries).
    for _ in 0..64 {
        if want == 0 {
            break;
        }
        let p = [lo[0] + rng.f32() * (hi[0] - lo[0]), lo[1] + rng.f32() * (hi[1] - lo[1])];
        st.candidates += 1;
        if inside(&mut st, p)? {
            let (ci, cj) = cell(p);
            grid[cj * gw + ci] = 0;
            pts.push(p);
            active.push(0);
            rules(&mut st, p, &mut out)?;
            break;
        }
    }
    // Enough samples to fill the area, plus slack for rejection; never unbounded.
    let cap = MAX_POINTS * 4;
    while !active.is_empty() && pts.len() < cap && out.len() < want {
        let ai = rng.below(active.len() as u32) as usize;
        let base = pts[active[ai] as usize];
        let mut found = false;
        for _ in 0..spec.attempts {
            st.candidates += 1;
            let (s, c) = crate::dmath::sin_cos(rng.f32() * std::f32::consts::TAU);
            let d = r * (1.0 + rng.f32());
            let p = [base[0] + c * d, base[1] + s * d];
            if p[0] < lo[0] || p[1] < lo[1] || p[0] > hi[0] || p[1] > hi[1] {
                charge(&mut st, crate::work::SCATTER_CANDIDATE)?;
                continue;
            }
            if !far(&pts, &grid, p) {
                charge(&mut st, crate::work::SCATTER_CANDIDATE)?;
                continue;
            }
            if !inside(&mut st, p)? {
                continue;
            }
            let (ci, cj) = cell(p);
            grid[cj * gw + ci] = pts.len() as u32;
            active.push(pts.len() as u32);
            pts.push(p);
            rules(&mut st, p, &mut out)?;
            found = true;
            break;
        }
        if !found {
            active.swap_remove(ai);
        }
    }
    st.sampled = pts.len();
    st.placed = out.len();
    Ok((out, st))
}

/// The flat ground at height 0.
pub fn flat(_x: f32, _z: f32) -> Option<Ground> {
    Some(Ground { height: 0.0, normal: [0.0, 1.0, 0.0], splat: None })
}

/// Placements as the bounding radius test: the smallest distance between any two (for tests).
pub fn min_spacing(p: &[Placement]) -> f32 {
    let mut best = f32::MAX;
    for i in 0..p.len() {
        for j in i + 1..p.len() {
            let d = math3::sub(p[i].position, p[j].position);
            best = best.min((d[0] * d[0] + d[2] * d[2]).sqrt());
        }
    }
    best
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_methods, reason = "a test or benchmark harness (timings, cameras for pictures), not simulation code")]
    use super::*;

    fn spec() -> ScatterSpec {
        ScatterSpec {
            area: Area::Rect([20.0, 12.0]),
            offset: [0.0, 0.0],
            min_distance: 1.5,
            max_count: 500,
            seed: 3,
            attempts: 30,
            slope: None,
            height: None,
            mask: None,
            splat: None,
            avoid: vec![[0.0, 0.0, 3.0]],
            weights: vec![1.0, 3.0],
            scale: [0.8, 1.2],
            yaw: [0.0, 360.0],
            tilt: 5.0,
            align: 0.0,
            sink: 0.05,
        }
    }

    #[test]
    fn poisson_disc_spacing_and_rules() {
        let (p, st) = scatter(&spec(), &flat, MAX_WORK).unwrap();
        assert!(p.len() > 40, "{st:?}");
        assert!(min_spacing(&p) >= 1.5 - 1e-4);
        assert!(p.iter().all(|q| q.position[0].abs() <= 10.0 && q.position[2].abs() <= 6.0));
        assert!(p.iter().all(|q| q.position[0].hypot(q.position[2]) >= 3.0));
        assert!(st.rejected[5] > 0);
        // Weights 1:3.
        let ones = p.iter().filter(|q| q.item == 1).count() as f32 / p.len() as f32;
        assert!(ones > 0.6 && ones < 0.9, "{ones}");
        assert!(p.iter().all(|q| (q.position[1] + 0.05).abs() < 1e-6));
        // Deterministic.
        assert_eq!(scatter(&spec(), &flat, MAX_WORK).unwrap().0, p);
    }

    #[test]
    fn ground_rules() {
        let slope = |x: f32, _z: f32| -> Option<Ground> {
            // Steep for x > 0.
            let n = if x > 0.0 { math3::normalize([-1.0, 1.0, 0.0]) } else { [0.0, 1.0, 0.0] };
            Some(Ground { height: x * 0.1, normal: n, splat: None })
        };
        let mut s = spec();
        s.slope = Some([0.0, 30.0]);
        s.align = 1.0;
        let (p, st) = scatter(&s, &slope, MAX_WORK).unwrap();
        assert!(p.iter().all(|q| q.position[0] <= 0.0));
        assert!(st.rejected[1] > 0);
        s.mask = Some(Mask { scale: 4.0, threshold: 0.5, octaves: 2 });
        let (q, st2) = scatter(&s, &slope, MAX_WORK).unwrap();
        assert!(q.len() < p.len() && st2.rejected[3] > 0);
        s.area = Area::Circle(5.0);
        s.avoid.clear();
        let (c, _) = scatter(&s, &flat, MAX_WORK).unwrap();
        assert!(c.iter().all(|q| q.position[0].hypot(q.position[2]) <= 5.0 + 1e-4));
    }

    /// A comb: `teeth` thin vertical teeth joined along the bottom (4·teeth + 2 points).
    pub(crate) fn comb(teeth: usize) -> Vec<V2> {
        let w = 200.0 / teeth as f32;
        let mut pts = vec![[-100.0, -100.0], [100.0, -100.0], [100.0, -99.0]];
        for t in (0..teeth).rev() {
            let xl = -100.0 + t as f32 * w + w * 0.45;
            let xr = xl + w * 0.1;
            pts.extend([[xr, -99.0], [xr, 100.0], [xl, 100.0], [xl, -99.0]]);
        }
        pts.push([-100.0, -99.0]);
        pts
    }

    #[test]
    fn the_polygon_grid_agrees_with_the_plain_test() {
        let mut rng = Rng::new(9, 1);
        let star: Vec<V2> = (0..40)
            .map(|i| {
                let a = i as f32 / 40.0 * std::f32::consts::TAU;
                let r = if i % 2 == 0 { 10.0 } else { 3.0 + rng.f32() * 4.0 };
                let (s, c) = crate::dmath::sin_cos(a);
                [c * r, s * r]
            })
            .collect();
        for poly in [star, comb(60), vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [2.0, 1.0], [0.0, 4.0]]] {
            let spec = ScatterSpec { area: Area::Polygon(poly.clone()), ..spec() };
            let (lo, hi) = spec.bounds();
            let (g, _) = PolyGrid::new(&poly);
            let mut differ = 0;
            for _ in 0..20_000 {
                let p = [lo[0] + rng.f32() * (hi[0] - lo[0]), lo[1] + rng.f32() * (hi[1] - lo[1])];
                if g.inside(&poly, p).0 != spec.inside(p) {
                    differ += 1;
                }
            }
            assert_eq!(differ, 0, "{} points", poly.len());
        }
    }

    #[test]
    fn scatters_stop_at_their_count_and_charge_their_work() {
        // A 1,022-point comb: one placement wanted, so sampling stops at once (it used to fill
        // the whole comb, testing every candidate against every edge).
        let s = ScatterSpec { area: Area::Polygon(comb(255)), min_distance: 0.4, max_count: 1, attempts: 64, avoid: vec![], ..spec() };
        let (p, st) = scatter(&s, &flat, MAX_WORK).unwrap();
        assert_eq!(p.len(), 1);
        assert!(st.candidates < 1000, "{st:?}");
        // Filling it: every candidate and every edge tested is charged.
        let s = ScatterSpec { max_count: 4096, ..s };
        let (p, st) = scatter(&s, &flat, MAX_WORK).unwrap();
        assert!(p.len() > 1000);
        assert!(st.work >= st.candidates * crate::work::SCATTER_CANDIDATE, "{st:?}");
        // A smaller budget stops it with a limit, never a long run.
        let e = scatter(&s, &flat, st.work / 2).unwrap_err();
        assert!(e.limit && e.field == "count", "{e:?}");
        // Same placements, however many are asked for: the first ones never change.
        let (few, _) = scatter(&ScatterSpec { max_count: 50, ..s.clone() }, &flat, MAX_WORK).unwrap();
        assert_eq!(&p[..50], &few[..]);
    }

    #[test]
    fn limits() {
        let mut s = spec();
        s.min_distance = 1e-3;
        s.area = Area::Rect([1000.0, 1000.0]);
        assert!(scatter(&s, &flat, MAX_WORK).unwrap_err().limit);
        let mut s = spec();
        s.max_count = MAX_POINTS as u32 + 1;
        assert!(scatter(&s, &flat, MAX_WORK).unwrap_err().limit);
        let mut s = spec();
        s.avoid = vec![[0.0, 0.0, 0.1]; MAX_AVOID + 1];
        assert_eq!(scatter(&s, &flat, MAX_WORK).unwrap_err().field, "avoid");
    }
}

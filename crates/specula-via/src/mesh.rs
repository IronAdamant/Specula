//! The navmesh: tiles of rectangles and the portals between them.
//!
//! Tiles are stored by index (`tz * tiles_x + tx`). A polygon is addressed by a [`PolyRef`]
//! (tile index and rectangle index); refs into a tile are invalidated when it rebuilds, which
//! [`NavMesh::rebuilt_since`] tells. Links are kept sorted (side, first column, target), so the
//! navmesh is the same whatever order its tiles were built in.

use crate::build::{self, opposite, EdgeSpan, TileBuild};
use crate::{Config, Derived, NavError, Source};
use specula_core::hash::StableHasher;
use specula_core::math::Vec3;
use specula_core::Json;

/// A polygon: tile index in the high 16 bits, rectangle index in the low 16.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolyRef(pub u32);

impl PolyRef {
    pub fn new(tile: usize, poly: usize) -> PolyRef {
        PolyRef(((tile as u32) << 16) | (poly as u32 & 0xFFFF))
    }

    pub fn tile(self) -> usize {
        (self.0 >> 16) as usize
    }

    pub fn index(self) -> usize {
        (self.0 & 0xFFFF) as usize
    }
}

/// A portal to a neighbouring polygon, on one side, over global columns `lo..=hi` along it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Link {
    pub side: u8,
    pub lo: i32,
    pub hi: i32,
    pub to: PolyRef,
}

/// A walkable rectangle: global columns `x0..=x1`, `z0..=z1`, and the floor heights at its corner
/// columns' centres, `(x0, z0), (x1, z0), (x0, z1), (x1, z1)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Poly {
    pub x0: i32,
    pub z0: i32,
    pub x1: i32,
    pub z1: i32,
    pub h: [f32; 4],
    pub links: Vec<Link>,
}

/// One built tile.
#[derive(Clone, Debug, Default)]
pub struct Tile {
    pub polys: Vec<Poly>,
    edges: [Vec<Vec<EdgeSpan>>; 4],
    /// Hash of the tile's polygons and links (refreshed when either changes).
    hash: u64,
    /// Running total of the polygons' areas (for random points), and the whole.
    area_prefix: Vec<f32>,
    pub area: f32,
}

/// What building a tile did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TileStats {
    pub tx: u32,
    pub tz: u32,
    pub triangles: usize,
    pub spans: usize,
    pub open: usize,
    pub eroded: usize,
    pub polys: usize,
    pub errors: Vec<NavError>,
    /// See [`crate::build::TileBuild::times_us`].
    pub times_us: [f64; 5],
}

/// A tiled navmesh.
#[derive(Clone, Debug)]
pub struct NavMesh {
    cfg: Config,
    d: Derived,
    tiles: Vec<Option<Tile>>,
    /// When each tile was last (re)built, by [`NavMesh::counter`].
    rebuilt: Vec<u64>,
    counter: u64,
    scratch: build::Scratch,
}

impl NavMesh {
    pub fn new(cfg: Config) -> Result<NavMesh, NavError> {
        cfg.validate()?;
        let d = Derived::of(&cfg);
        let n = d.tiles_x as usize * d.tiles_z as usize;
        Ok(NavMesh { cfg, d, tiles: vec![None; n], rebuilt: vec![0; n], counter: 0, scratch: build::Scratch::default() })
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    pub fn derived(&self) -> &Derived {
        &self.d
    }

    /// Increases by one with every tile build.
    pub fn counter(&self) -> u64 {
        self.counter
    }

    pub fn tile_count(&self) -> (u32, u32) {
        (self.d.tiles_x, self.d.tiles_z)
    }

    pub fn tile_index(&self, tx: u32, tz: u32) -> usize {
        (tz * self.d.tiles_x + tx) as usize
    }

    pub fn tile_coords(&self, index: usize) -> (u32, u32) {
        (index as u32 % self.d.tiles_x, index as u32 / self.d.tiles_x)
    }

    pub fn tile(&self, index: usize) -> Option<&Tile> {
        self.tiles.get(index).and_then(Option::as_ref)
    }

    /// Built tiles, in index order.
    pub fn tiles(&self) -> impl Iterator<Item = (usize, &Tile)> + '_ {
        self.tiles.iter().enumerate().filter_map(|(i, t)| t.as_ref().map(|t| (i, t)))
    }

    pub fn poly(&self, r: PolyRef) -> Option<&Poly> {
        self.tile(r.tile()).and_then(|t| t.polys.get(r.index()))
    }

    pub fn poly_count(&self) -> usize {
        self.tiles().map(|(_, t)| t.polys.len()).sum()
    }

    /// Whether tile `index` was rebuilt after `counter`.
    pub fn rebuilt_since(&self, index: usize, counter: u64) -> bool {
        self.rebuilt.get(index).is_some_and(|r| *r > counter)
    }

    /// The tile's columns as world X/Z bounds `[min_x, min_z, max_x, max_z]` (without border).
    pub fn tile_rect(&self, tx: u32, tz: u32) -> [f32; 4] {
        let (cs, t) = (self.cfg.cell_size, self.cfg.tile_cells as f32);
        let o = self.cfg.bounds_min;
        [o.x + tx as f32 * t * cs, o.z + tz as f32 * t * cs, o.x + (tx + 1) as f32 * t * cs, o.z + (tz + 1) as f32 * t * cs]
    }

    /// The world box a tile's build reads triangles from: the tile, its erosion border and the
    /// navmesh's height range.
    pub fn tile_input_bounds(&self, tx: u32, tz: u32) -> (Vec3, Vec3) {
        let r = self.tile_rect(tx, tz);
        let m = self.d.border as f32 * self.cfg.cell_size;
        (Vec3::new(r[0] - m, self.cfg.bounds_min.y, r[1] - m), Vec3::new(r[2] + m, self.cfg.bounds_max.y, r[3] + m))
    }

    /// The tiles whose builds read anything inside the box `lo..hi` (sorted).
    pub fn tiles_touching(&self, lo: Vec3, hi: Vec3) -> Vec<(u32, u32)> {
        let (cs, t) = (self.cfg.cell_size, self.cfg.tile_cells as f32);
        let o = self.cfg.bounds_min;
        let m = self.d.border as f32 * cs;
        if hi.y < o.y || lo.y > self.cfg.bounds_max.y {
            return Vec::new();
        }
        let span = |lo: f32, hi: f32, o: f32, n: u32| -> Option<(u32, u32)> {
            let a = specula_core::dmath::floor((lo - m - o) / (t * cs));
            let b = specula_core::dmath::floor((hi + m - o) / (t * cs));
            if b < 0.0 || a >= n as f32 || !a.is_finite() || !b.is_finite() {
                return None;
            }
            Some((a.max(0.0) as u32, (b as u32).min(n - 1)))
        };
        let (Some((x0, x1)), Some((z0, z1))) = (span(lo.x, hi.x, o.x, self.d.tiles_x), span(lo.z, hi.z, o.z, self.d.tiles_z)) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for tz in z0..=z1 {
            for tx in x0..=x1 {
                out.push((tx, tz));
            }
        }
        out
    }

    /// Builds (or rebuilds) tile `(tx, tz)` from the sources that touch it (in a fixed order),
    /// and relinks it with its neighbours.
    pub fn build_tile(&mut self, tx: u32, tz: u32, sources: &[&Source]) -> TileStats {
        let b: TileBuild = build::build_tile(&self.cfg, &self.d, tx, tz, sources, &mut self.scratch);
        let index = self.tile_index(tx, tz);
        let polys: Vec<Poly> = b
            .rects
            .iter()
            .map(|r| Poly {
                x0: r.x0,
                z0: r.z0,
                x1: r.x1,
                z1: r.z1,
                h: r.h,
                links: r.links.iter().map(|&(q, side, lo, hi)| Link { side, lo, hi, to: PolyRef::new(index, q as usize) }).collect(),
            })
            .collect();
        let stats = TileStats {
            tx,
            tz,
            triangles: b.triangles,
            spans: b.spans,
            open: b.open,
            eroded: b.eroded,
            polys: polys.len(),
            errors: b.errors,
            times_us: b.times_us,
        };
        let (cs, mut sum) = (self.cfg.cell_size, 0.0f32);
        let area_prefix: Vec<f32> = polys
            .iter()
            .map(|p| {
                sum += (p.x1 - p.x0 + 1) as f32 * cs * ((p.z1 - p.z0 + 1) as f32 * cs);
                sum
            })
            .collect();
        self.tiles[index] = Some(Tile { polys, edges: b.edges, hash: 0, area_prefix, area: sum });
        self.counter += 1;
        self.rebuilt[index] = self.counter;
        self.relink(tx, tz);
        stats
    }

    /// Removes a tile (nothing walkable there any more).
    pub fn clear_tile(&mut self, tx: u32, tz: u32) {
        let index = self.tile_index(tx, tz);
        if self.tiles[index].is_none() {
            return;
        }
        self.tiles[index] = None;
        self.counter += 1;
        self.rebuilt[index] = self.counter;
        self.relink(tx, tz);
    }

    /// Recomputes the links between tile `(tx, tz)` and its four neighbours.
    fn relink(&mut self, tx: u32, tz: u32) {
        let index = self.tile_index(tx, tz);
        let climb = self.d.climb_q;
        let height = self.d.height_q;
        let t = self.cfg.tile_cells as i32;
        for side in 0..4usize {
            let (nx, nz) = (tx as i64 + i64::from(build::DX[side]), tz as i64 + i64::from(build::DZ[side]));
            if nx < 0 || nz < 0 || nx >= i64::from(self.d.tiles_x) || nz >= i64::from(self.d.tiles_z) {
                continue;
            }
            let other = self.tile_index(nx as u32, nz as u32);
            // Drop the old cross links both ways.
            if let Some(tile) = self.tiles[index].as_mut() {
                for p in &mut tile.polys {
                    p.links.retain(|l| l.to.tile() != other);
                }
            }
            if let Some(tile) = self.tiles[other].as_mut() {
                for p in &mut tile.polys {
                    p.links.retain(|l| l.to.tile() != index);
                }
            }
            let (Some(a), Some(b)) = (self.tiles[index].as_ref(), self.tiles[other].as_ref()) else { continue };
            // Columns along the shared edge: z for the x sides, x for the z sides.
            let base = if side == 0 || side == 2 { tz as i32 * t } else { tx as i32 * t };
            let mut pairs: Vec<(u32, u32, i32)> = Vec::new();
            let (ea, eb) = (&a.edges[side], &b.edges[opposite(side)]);
            for i in 0..ea.len().min(eb.len()) {
                for sa in &ea[i] {
                    for sb in &eb[i] {
                        let gap = sa.ceil.min(sb.ceil).saturating_sub(sa.floor.max(sb.floor));
                        if sa.floor.abs_diff(sb.floor) <= climb && gap >= height {
                            pairs.push((sa.poly, sb.poly, base + i as i32));
                        }
                    }
                }
            }
            pairs.sort_unstable();
            pairs.dedup();
            let mut runs: Vec<(u32, u32, i32, i32)> = Vec::new();
            for (pa, pb, c) in pairs {
                match runs.last_mut() {
                    Some(r) if r.0 == pa && r.1 == pb && r.3 + 1 == c => r.3 = c,
                    _ => runs.push((pa, pb, c, c)),
                }
            }
            for (pa, pb, lo, hi) in runs {
                if let Some(p) = self.tiles[index].as_mut().and_then(|t| t.polys.get_mut(pa as usize)) {
                    p.links.push(Link { side: side as u8, lo, hi, to: PolyRef::new(other, pb as usize) });
                }
                if let Some(p) = self.tiles[other].as_mut().and_then(|t| t.polys.get_mut(pb as usize)) {
                    p.links.push(Link { side: opposite(side) as u8, lo, hi, to: PolyRef::new(index, pa as usize) });
                }
            }
            if let Some(tile) = self.tiles[other].as_mut() {
                for p in &mut tile.polys {
                    p.links.sort_unstable();
                }
                tile.hash = tile_hash(tile);
            }
        }
        if let Some(tile) = self.tiles[index].as_mut() {
            for p in &mut tile.polys {
                p.links.sort_unstable();
            }
            tile.hash = tile_hash(tile);
        }
    }

    /// The polygon whose share of the walkable area holds `pick` (0..total area), by tile then
    /// polygon order.
    pub fn poly_at_area(&self, mut pick: f32) -> Option<PolyRef> {
        let mut last = None;
        for (i, t) in self.tiles() {
            if t.polys.is_empty() {
                continue;
            }
            last = Some(PolyRef::new(i, t.polys.len() - 1));
            if pick <= t.area {
                let k = t.area_prefix.partition_point(|a| *a < pick).min(t.polys.len() - 1);
                return Some(PolyRef::new(i, k));
            }
            pick -= t.area;
        }
        last
    }

    /// The walkable area, square metres.
    pub fn total_area(&self) -> f32 {
        self.tiles().map(|(_, t)| t.area).sum()
    }

    /// World X/Z bounds of a polygon: `[min_x, min_z, max_x, max_z]`.
    pub fn rect(&self, p: &Poly) -> [f32; 4] {
        let (cs, o) = (self.cfg.cell_size, self.cfg.bounds_min);
        [o.x + p.x0 as f32 * cs, o.z + p.z0 as f32 * cs, o.x + (p.x1 + 1) as f32 * cs, o.z + (p.z1 + 1) as f32 * cs]
    }

    /// The floor height of polygon `p` at world `(x, z)` (bilinear between its corner columns'
    /// centres, clamped to them).
    pub fn height(&self, p: &Poly, x: f32, z: f32) -> f32 {
        let (cs, o) = (self.cfg.cell_size, self.cfg.bounds_min);
        let centre = |c: i32, o: f32| o + (c as f32 + 0.5) * cs;
        let t = |v: f32, a: i32, b: i32, o: f32| {
            if b > a {
                ((v - centre(a, o)) / (centre(b, o) - centre(a, o))).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let (u, v) = (t(x, p.x0, p.x1, o.x), t(z, p.z0, p.z1, o.z));
        let top = p.h[0] + (p.h[1] - p.h[0]) * u;
        let bottom = p.h[2] + (p.h[3] - p.h[2]) * u;
        top + (bottom - top) * v
    }

    /// A link's portal: its two end points in world space (heights averaged between the two
    /// polygons), first the one at the lower column.
    pub fn portal(&self, from: &Poly, l: &Link) -> (Vec3, Vec3) {
        let (cs, o) = (self.cfg.cell_size, self.cfg.bounds_min);
        let r = self.rect(from);
        let to = self.poly(l.to);
        let at = |x: f32, z: f32| {
            let h = self.height(from, x, z);
            let y = match to {
                Some(q) => (h + self.height(q, x, z)) * 0.5,
                None => h,
            };
            Vec3::new(x, y, z)
        };
        let (a, b) = (l.lo as f32 * cs, (l.hi + 1) as f32 * cs);
        match l.side {
            0 => (at(r[0], o.z + a), at(r[0], o.z + b)),
            2 => (at(r[2], o.z + a), at(r[2], o.z + b)),
            3 => (at(o.x + a, r[1]), at(o.x + b, r[1])),
            _ => (at(o.x + a, r[3]), at(o.x + b, r[3])),
        }
    }

    /// Hash of every built tile's polygons and links, in tile order.
    pub fn hash(&self) -> u64 {
        let mut h = StableHasher::default();
        h.write_u64(self.tiles.len() as u64);
        for (i, t) in self.tiles() {
            h.write_u64(i as u64);
            h.write_u64(t.hash);
        }
        h.finish()
    }

    /// A summary for tools.
    pub fn summary_json(&self) -> Json {
        let built = self.tiles().count();
        let links: usize = self.tiles().map(|(_, t)| t.polys.iter().map(|p| p.links.len()).sum::<usize>()).sum();
        Json::obj([
            ("tiles", Json::arr([Json::from(self.d.tiles_x), Json::from(self.d.tiles_z)])),
            ("tiles_built", Json::from(built)),
            ("polygons", Json::from(self.poly_count())),
            ("links", Json::from(links)),
            ("hash", Json::from(format!("{:016x}", self.hash()))),
        ])
    }
}

fn tile_hash(t: &Tile) -> u64 {
    let mut h = StableHasher::default();
    h.write_u64(t.polys.len() as u64);
    for p in &t.polys {
        for v in [p.x0, p.z0, p.x1, p.z1] {
            h.write_i64(i64::from(v));
        }
        for v in p.h {
            h.write_f32(v);
        }
        h.write_u64(p.links.len() as u64);
        for l in &p.links {
            h.write_u8(l.side);
            h.write_i64(i64::from(l.lo));
            h.write_i64(i64::from(l.hi));
            h.write_u32(l.to.0);
        }
    }
    h.finish()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::Triangle;

    pub fn quad(x0: f32, z0: f32, x1: f32, z1: f32, y: f32) -> Vec<Triangle> {
        let (a, b, c, d) = (Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), Vec3::new(x1, y, z1), Vec3::new(x0, y, z1));
        vec![Triangle([a, c, b]), Triangle([a, d, c])]
    }

    /// The 12 triangles of an axis-aligned box.
    pub fn cuboid(lo: Vec3, hi: Vec3) -> Vec<Triangle> {
        let c = |i: usize| Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z });
        let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
        let mut out = Vec::new();
        for f in faces {
            out.push(Triangle([c(f[0]), c(f[1]), c(f[2])]));
            out.push(Triangle([c(f[0]), c(f[2]), c(f[3])]));
        }
        out
    }

    /// A floor quad (a surface) and boxes (solids).
    pub fn scene(floor: [f32; 4], boxes: &[(Vec3, Vec3)]) -> Vec<Source> {
        let mut out = vec![Source::new(quad(floor[0], floor[1], floor[2], floor[3], 0.0), false)];
        out.extend(boxes.iter().map(|(lo, hi)| Source::new(cuboid(*lo, *hi), true)));
        out
    }

    /// Builds tile `(tx, tz)` from the sources touching its input bounds.
    pub fn build_one(mesh: &mut NavMesh, tx: u32, tz: u32, sources: &[Source]) {
        let (lo, hi) = mesh.tile_input_bounds(tx, tz);
        let mine: Vec<&Source> = sources.iter().filter(|s| s.touches(lo, hi)).collect();
        mesh.build_tile(tx, tz, &mine);
    }

    /// Builds every tile.
    pub fn build_all(mesh: &mut NavMesh, sources: &[Source]) {
        let (nx, nz) = mesh.tile_count();
        for tz in 0..nz {
            for tx in 0..nx {
                build_one(mesh, tx, tz, sources);
            }
        }
    }

    pub fn room_config() -> Config {
        Config { bounds_min: Vec3::new(-10.0, -1.0, -10.0), bounds_max: Vec3::new(10.0, 4.0, 10.0), tile_cells: 32, ..Config::default() }
    }

    #[test]
    fn tiles_link_across_edges() {
        let mut mesh = NavMesh::new(room_config()).unwrap();
        build_all(&mut mesh, &scene([-10.0, -10.0, 10.0, 10.0], &[]));
        let (nx, nz) = mesh.tile_count();
        assert_eq!((nx, nz), (5, 5));
        // Every polygon of a flat floor links to its neighbours (no isolated tile).
        for (i, t) in mesh.tiles() {
            for p in &t.polys {
                assert!(!p.links.is_empty(), "tile {i} polygon {p:?} has no links");
                for l in &p.links {
                    let q = mesh.poly(l.to).expect("link target exists");
                    assert!(q.links.iter().any(|b| b.to.tile() == i), "links are two-way");
                }
            }
        }
    }

    #[test]
    fn incremental_equals_full_build() {
        let cfg = room_config();
        let (lo, hi) = (Vec3::new(-1.0, 0.0, -3.0), Vec3::new(1.0, 2.0, 3.0));
        let sources = scene([-10.0, -10.0, 10.0, 10.0], &[(lo, hi)]);
        let mut full = NavMesh::new(cfg).unwrap();
        build_all(&mut full, &sources);
        // Build without the box, then add it and rebuild only the touched tiles.
        let mut inc = NavMesh::new(cfg).unwrap();
        build_all(&mut inc, &sources[..1]);
        let before = inc.hash();
        let touched = inc.tiles_touching(lo, hi);
        assert!(touched.len() < 25, "only some tiles: {touched:?}");
        for (tx, tz) in touched {
            build_one(&mut inc, tx, tz, &sources);
        }
        assert_ne!(before, inc.hash());
        assert_eq!(inc.hash(), full.hash(), "rebuilding touched tiles gives the same navmesh as a full build");
    }
}

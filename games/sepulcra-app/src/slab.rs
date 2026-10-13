//! The slab: built once from the map under `/slab`, plus the geometry the rest of the game
//! needs from it (where a cell's surface is, which cell a ray hits).
//!
//! Faces are merged into rectangles of equal cells (greedy meshing), so the whole slab is about
//! two hundred instanced cubes that agents can still inspect by name: every piece carries the
//! cells it covers in its props.

use crate::ents;
use crate::looks::{hex, palette::*, Part};
use sepulcra::map::{CellKind, Face, Map};
use sepulcra::CellId;
use specula::world::World;
use specula_core::math::Vec3;
use specula_core::rng::Rng;
use specula_core::Json;

/// Fragments rise this far (plus up to [`FRAGMENT_JITTER`]) above the lane floor at y = 0.
pub const FRAGMENT_HEIGHT: f32 = 0.45;
pub const FRAGMENT_JITTER: f32 = 0.2;
pub const GRAVEYARD_HEIGHT: f32 = 0.12;
/// Rock panels on a cliff stand out from the cliff plane by this much; lanes are recessed.
const PANEL: f32 = 0.3;
const LANE_PANEL: f32 = 0.1;
/// A ledge's shelf sticks out this far from the cliff.
const SHELF_OUT: f32 = 1.8;

/// The slab's shape in metres.
#[derive(Clone, Debug)]
pub struct SlabGeo {
    pub half_x: f32,
    pub half_z: f32,
    pub cell: f32,
    pub row: f32,
    pub depth: f32,
    /// Surface height per top-face cell id (other cells: 0).
    pub surface: Vec<f32>,
}

impl SlabGeo {
    pub fn new(map: &Map) -> SlabGeo {
        let rows = map.faces().iter().filter(|g| g.face.is_cliff()).map(|g| g.rows).max().unwrap_or(6);
        let row = map.cliff_row_mm as f32 / 1000.0;
        let mut surface = vec![0.0; map.cells().len()];
        // Fragments: each connected piece of rock gets its own height and shade.
        let mut rng = Rng::named(0x5e9_c1a, "slab");
        for cells in fragment_pieces(map) {
            let h = FRAGMENT_HEIGHT + FRAGMENT_JITTER * rng.f32();
            for c in cells {
                surface[c.index()] = h;
            }
        }
        for c in map.cells() {
            if c.kind == CellKind::Graveyard {
                surface[c.id.index()] = GRAVEYARD_HEIGHT;
            }
        }
        SlabGeo {
            half_x: map.half_x as f32 / 1000.0,
            half_z: map.half_z as f32 / 1000.0,
            cell: map.cell_mm as f32 / 1000.0,
            row,
            depth: f32::from(rows) * row,
            surface,
        }
    }

    /// Unit vector out of a face.
    pub fn outward(face: Face) -> Vec3 {
        match face {
            Face::Top => Vec3::Y,
            Face::North => Vec3::new(0.0, 0.0, -1.0),
            Face::South => Vec3::Z,
            Face::East => Vec3::X,
            Face::West => Vec3::new(-1.0, 0.0, 0.0),
        }
    }

    /// Where things stand on a cell, and which way is "up" there: the top of a fragment, the lane
    /// floor, a ledge's shelf, or the face of a cliff.
    pub fn anchor(&self, map: &Map, id: CellId) -> (Vec3, Vec3) {
        let c = map.cell(id);
        let [x, y, z] = c.world_pos_f32();
        if c.face == Face::Top {
            return (Vec3::new(x, self.surface[id.index()], z), Vec3::Y);
        }
        let out = SlabGeo::outward(c.face);
        let p = Vec3::new(x, y, z);
        if c.kind == CellKind::Ledge {
            (p + out * (SHELF_OUT * 0.55) + Vec3::new(0.0, -0.3, 0.0), Vec3::Y)
        } else {
            (p + out * PANEL, out)
        }
    }

    /// The cell a ray (from the eye, unit direction) hits first: the top face, or one of the
    /// cliffs facing the ray.
    pub fn pick(&self, map: &Map, origin: Vec3, dir: Vec3) -> Option<CellId> {
        let mut best: Option<(f32, CellId)> = None;
        let mut consider = |t: f32, id: Option<CellId>| {
            if let Some(id) = id {
                if t > 0.0 && best.is_none_or(|(bt, _)| t < bt) {
                    best = Some((t, id));
                }
            }
        };
        // Top face: try the fragment height first (a raised fragment hides the lane behind it),
        // then the lane floor.
        let top_at = |h: f32| -> Option<(f32, CellId)> {
            if dir.y.abs() < 1e-6 {
                return None;
            }
            let t = (h - origin.y) / dir.y;
            let p = origin + dir * t;
            let col = ((p.x + self.half_x) / self.cell).floor();
            let row = ((p.z + self.half_z) / self.cell).floor();
            if col < 0.0 || row < 0.0 {
                return None;
            }
            let id = map.at(Face::Top, col as u8, row as u8).filter(|_| col < 256.0 && row < 256.0)?;
            Some((t, id))
        };
        let high = FRAGMENT_HEIGHT + FRAGMENT_JITTER * 0.5;
        match top_at(high) {
            Some((t, id)) if map.cell(id).kind == CellKind::Fragment => consider(t, Some(id)),
            _ => {
                if let Some((t, id)) = top_at(0.0) {
                    if map.cell(id).kind != CellKind::Void {
                        consider(t, Some(id));
                    }
                }
            }
        }
        // Ledge shelves stick out of their cliff: hit their top from above.
        if dir.y < -1e-6 {
            for c in map.cells().iter().filter(|c| c.kind == CellKind::Ledge) {
                let (a, _) = self.anchor(map, c.id);
                let t = (a.y - origin.y) / dir.y;
                let p = origin + dir * t;
                let out = SlabGeo::outward(c.face);
                let [cx, _, cz] = c.world_pos_f32();
                let rel = p - Vec3::new(cx, a.y, cz);
                let o = rel.dot(out);
                let along = (rel - out * o).length();
                if (0.0..=SHELF_OUT).contains(&o) && along <= self.cell / 2.0 {
                    consider(t, Some(c.id));
                }
            }
        }
        // Cliffs: only from outside.
        for g in map.faces().iter().filter(|g| g.face.is_cliff()) {
            let out = SlabGeo::outward(g.face);
            let plane = match g.face {
                Face::North | Face::South => self.half_z,
                _ => self.half_x,
            } + PANEL;
            let o = origin.dot(out);
            let d = dir.dot(out);
            if o <= plane || d >= -1e-6 {
                continue;
            }
            let t = (plane - o) / d;
            let p = origin + dir * t;
            let along = match g.face {
                Face::North | Face::South => p.x + self.half_x,
                _ => p.z + self.half_z,
            };
            let (col, row) = ((along / self.cell).floor(), (-p.y / self.row).floor());
            if col < 0.0 || row < 0.0 || col >= f32::from(g.cols) || row >= f32::from(g.rows) {
                continue;
            }
            consider(t, map.at(g.face, col as u8, row as u8).filter(|&id| map.cell(id).kind != CellKind::Void));
        }
        best.map(|(_, id)| id)
    }
}

/// Connected pieces of fragment cells on the top face, in cell-id order.
fn fragment_pieces(map: &Map) -> Vec<Vec<CellId>> {
    let Some(top) = map.face(Face::Top) else { return Vec::new() };
    let (cols, rows) = (top.cols, top.rows);
    let mut seen = vec![false; map.cells().len()];
    let mut pieces = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let Some(start) = map.at(Face::Top, c, r) else { continue };
            if seen[start.index()] || map.cell(start).kind != CellKind::Fragment {
                continue;
            }
            let mut piece = Vec::new();
            let mut stack = vec![start];
            seen[start.index()] = true;
            while let Some(at) = stack.pop() {
                piece.push(at);
                let cell = map.cell(at);
                let (cc, rr) = (i32::from(cell.col), i32::from(cell.row));
                for (dc, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nc, nr) = (cc + dc, rr + dr);
                    if nc < 0 || nr < 0 {
                        continue;
                    }
                    if let Some(n) = map.at(Face::Top, nc as u8, nr as u8) {
                        if !seen[n.index()] && map.cell(n).kind == CellKind::Fragment {
                            seen[n.index()] = true;
                            stack.push(n);
                        }
                    }
                }
            }
            piece.sort();
            pieces.push(piece);
        }
    }
    pieces
}

/// Rectangles `(col, row, width, height, key)` covering every cell with a key: greedy meshing,
/// widest run first, then grown downwards while the rows below match.
fn rectangles(cols: usize, rows: usize, key: impl Fn(usize, usize) -> Option<u32>) -> Vec<(usize, usize, usize, usize, u32)> {
    let mut used = vec![false; cols * rows];
    let mut out = Vec::new();
    for r in 0..rows {
        let mut c = 0;
        while c < cols {
            let Some(k) = key(c, r).filter(|_| !used[r * cols + c]) else {
                c += 1;
                continue;
            };
            let mut w = 1;
            while c + w < cols && !used[r * cols + c + w] && key(c + w, r) == Some(k) {
                w += 1;
            }
            let mut h = 1;
            'grow: while r + h < rows {
                for cc in c..c + w {
                    if used[(r + h) * cols + cc] || key(cc, r + h) != Some(k) {
                        break 'grow;
                    }
                }
                h += 1;
            }
            for rr in r..r + h {
                for cc in c..c + w {
                    used[rr * cols + cc] = true;
                }
            }
            out.push((c, r, w, h, k));
            c += w;
        }
    }
    out
}

fn shade(c: [f32; 3], k: f32) -> [f32; 3] {
    [(c[0] * k).min(1.0), (c[1] * k).min(1.0), (c[2] * k).min(1.0)]
}

/// Builds the slab under `/slab` (replacing an old one). Returns the group's id.
pub fn build(world: &mut World, agent: &str, map: &Map, geo: &SlabGeo) -> u64 {
    if let Some(old) = ents::top_level(world, "slab") {
        ents::delete(world, old);
    }
    let props = Json::obj([("map", Json::from(map.name.as_str())), ("cell_m", Json::from(geo.cell)), ("cliff_row_m", Json::from(geo.row))]);
    let slab = ents::group(world, agent, None, "slab", Vec3::ZERO, Vec3::ZERO, &["slab"], props);
    let Some(top) = map.face(Face::Top).copied() else { return slab };
    let (cols, rows) = (usize::from(top.cols), usize::from(top.rows));
    let kind_at = |c: usize, r: usize| map.at(Face::Top, c as u8, r as u8).map(|id| map.cell(id).kind);
    let x_of = |c: f32| -geo.half_x + c * geo.cell;
    let z_of = |r: f32| -geo.half_z + r * geo.cell;
    let mut rng = Rng::named(0x5e9_c1a, "slab_looks");

    // The body: rock under every cell that isn't broken away. Its top is the lane floor.
    let body = ents::group(world, agent, Some(slab), "body", Vec3::ZERO, Vec3::ZERO, &[], Json::Null);
    for (i, (c, r, w, h, _)) in rectangles(cols, rows, |c, r| (kind_at(c, r) != Some(CellKind::Void)).then_some(0)).into_iter().enumerate() {
        let pos = Vec3::new(x_of(c as f32 + w as f32 / 2.0), -geo.depth / 2.0, z_of(r as f32 + h as f32 / 2.0));
        let scale = Vec3::new(w as f32 * geo.cell, geo.depth, h as f32 * geo.cell);
        let p = Part::cube("", pos, scale, FISSURE).rough(0.95);
        ents::part_named(world, agent, body, &format!("rock_{i:02}"), &p);
    }
    // Broken corners: chipped down one cliff row, at slightly different depths.
    for r in 0..rows {
        for c in 0..cols {
            if kind_at(c, r) == Some(CellKind::Void) {
                let top_y = -geo.row - 0.6 * rng.f32();
                let pos = Vec3::new(x_of(c as f32 + 0.5), (top_y - geo.depth) / 2.0, z_of(r as f32 + 0.5));
                let p = Part::cube("", pos, Vec3::new(geo.cell, top_y + geo.depth, geo.cell), CLIFF_A).rough(0.95);
                ents::part_named(world, agent, body, &format!("broken_{c}_{r}"), &p);
            }
        }
    }
    // A jagged underside fading into the dark.
    for i in 0..7 {
        let a = i as f32 / 7.0 * std::f32::consts::TAU + rng.f32();
        let rad = 6.0 + 8.0 * rng.f32();
        let size = 7.0 + 5.0 * rng.f32();
        let pos = Vec3::new(a.cos() * rad, -geo.depth - 1.0 - 2.0 * rng.f32(), a.sin() * rad);
        let p = Part::cube("", pos, Vec3::new(size, size * 0.8, size), shade(CLIFF_A, 0.8)).rot(40.0 + 10.0 * rng.f32(), 360.0 * rng.f32(), 45.0).rough(1.0);
        ents::part_named(world, agent, body, &format!("underside_{i}"), &p);
    }

    // Fragments: raised rock, one shade per piece.
    let frags = ents::group(world, agent, Some(slab), "fragments", Vec3::ZERO, Vec3::ZERO, &["buildable"], Json::Null);
    let pieces = fragment_pieces(map);
    let mut piece_of = vec![u32::MAX; map.cells().len()];
    for (i, cells) in pieces.iter().enumerate() {
        for c in cells {
            piece_of[c.index()] = i as u32;
        }
    }
    let piece_shade: Vec<f32> = (0..pieces.len()).map(|_| 0.9 + 0.18 * rng.f32()).collect();
    let rects = rectangles(cols, rows, |c, r| {
        let id = map.at(Face::Top, c as u8, r as u8)?;
        (map.cell(id).kind == CellKind::Fragment).then_some(piece_of[id.index()])
    });
    let piece_at = |c: i32, r: i32| -> Option<u32> {
        if c < 0 || r < 0 {
            return None;
        }
        let id = map.at(Face::Top, c as u8, r as u8)?;
        (map.cell(id).kind == CellKind::Fragment).then_some(piece_of[id.index()])
    };
    for (c, r, w, h, piece) in rects {
        let id = map.at(Face::Top, c as u8, r as u8).expect("in the grid");
        let top_y = geo.surface[id.index()];
        let (ci, ri, wi, hi) = (c as i32, r as i32, w as i32, h as i32);
        // Inset a side (a seam) only where it meets something other than its own piece, so one
        // piece reads as one stone however it was split into rectangles.
        let other = |cells: Vec<(i32, i32)>| cells.into_iter().any(|(cc, rr)| piece_at(cc, rr) != Some(piece));
        let seam = 0.04;
        let west = if other((ri..ri + hi).map(|rr| (ci - 1, rr)).collect()) { seam } else { 0.0 };
        let east = if other((ri..ri + hi).map(|rr| (ci + wi, rr)).collect()) { seam } else { 0.0 };
        let north = if other((ci..ci + wi).map(|cc| (cc, ri - 1)).collect()) { seam } else { 0.0 };
        let south = if other((ci..ci + wi).map(|cc| (cc, ri + hi)).collect()) { seam } else { 0.0 };
        let (x0, x1) = (x_of(c as f32) + west, x_of((c + w) as f32) - east);
        let (z0, z1) = (z_of(r as f32) + north, z_of((r + h) as f32) - south);
        let pos = Vec3::new((x0 + x1) / 2.0, (top_y - 0.1) / 2.0, (z0 + z1) / 2.0);
        let scale = Vec3::new(x1 - x0, top_y + 0.1, z1 - z0);
        let p = Part::cube("", pos, scale, shade(SLAB_TOP, piece_shade[piece as usize])).rough(0.9);
        ents::part_named(world, agent, frags, &format!("piece{piece:02}_{c}_{r}"), &p);
    }

    // Lane-jump points: a pale chalk scar across the fragment between each pair.
    let marks = ents::group(world, agent, Some(slab), "jump_ledges", Vec3::ZERO, Vec3::ZERO, &["lane_jump"], Json::Null);
    for (a, b) in map.jump_pairs() {
        let (pa, pb) = (map.cell(a).world_pos_f32(), map.cell(b).world_pos_f32());
        let mid = map.cell(map.at(Face::Top, (map.cell(a).col + map.cell(b).col) / 2, (map.cell(a).row + map.cell(b).row) / 2).unwrap_or(a));
        let h = geo.surface[mid.id.index()].max(0.05) + 0.015;
        let along_x = (pa[0] - pb[0]).abs() > (pa[2] - pb[2]).abs();
        let len = ((pa[0] - pb[0]).abs() + (pa[2] - pb[2]).abs()).max(geo.cell);
        let scale = if along_x { Vec3::new(len, 0.03, 0.32) } else { Vec3::new(0.32, 0.03, len) };
        let pos = Vec3::new((pa[0] + pb[0]) / 2.0, h, (pa[2] + pb[2]) / 2.0);
        let name = format!("{}-{}", map.cell_name(a), map.cell_name(b));
        let g = ents::group(world, agent, Some(marks), &name, Vec3::ZERO, Vec3::ZERO, &[], Json::Null);
        ents::part(world, agent, g, &Part::cube("scar", pos, scale, CHALK).rough(1.0));
        for (end, p) in [("end_a", pa), ("end_b", pb)] {
            let cross = if along_x { Vec3::new(0.25, 0.04, 1.4) } else { Vec3::new(1.4, 0.04, 0.25) };
            ents::part(world, agent, g, &Part::cube(end, Vec3::new(p[0], 0.02, p[2]), cross, CHALK).rough(1.0));
        }
    }

    // The graveyard: pale ground, tombstones (managed by the scene) and a low fence of posts.
    let gy = ents::group(world, agent, Some(slab), "graveyard", Vec3::ZERO, Vec3::ZERO, &["graveyard"], Json::Null);
    let (mut lo, mut hi) = (Vec3::new(f32::MAX, 0.0, f32::MAX), Vec3::new(f32::MIN, 0.0, f32::MIN));
    for &g in &map.graveyard {
        let [x, _, z] = map.cell(g).world_pos_f32();
        lo = lo.min(Vec3::new(x, 0.0, z));
        hi = hi.max(Vec3::new(x, 0.0, z));
    }
    if !map.graveyard.is_empty() {
        let half = geo.cell / 2.0;
        let (lo, hi) = (lo - Vec3::new(half, 0.0, half), hi + Vec3::new(half, 0.0, half));
        let centre = (lo + hi) * 0.5;
        let ground = Part::cube("ground", Vec3::new(centre.x, GRAVEYARD_HEIGHT / 2.0, centre.z), Vec3::new(hi.x - lo.x - 0.1, GRAVEYARD_HEIGHT, hi.z - lo.z - 0.1), shade(GRAVE_STONE, 0.62)).rough(1.0);
        ents::part(world, agent, gy, &ground);
        let posts = [
            ("post_nw", lo.x, lo.z), ("post_ne", hi.x, lo.z), ("post_sw", lo.x, hi.z), ("post_se", hi.x, hi.z),
            ("post_n", centre.x, lo.z), ("post_s", centre.x, hi.z), ("post_w", lo.x, centre.z), ("post_e", hi.x, centre.z),
        ];
        // Wrought iron, but mostly matte so it reads against the dark lanes at night.
        let iron = shade(IRON, 1.5);
        for (name, x, z) in posts {
            ents::part(world, agent, gy, &Part::cube(name, Vec3::new(x, 0.45, z), Vec3::new(0.14, 0.9, 0.14), iron).metal(0.3).rough(0.55));
            ents::part_named(world, agent, gy, &format!("{name}_cap"), &Part::sphere("", Vec3::new(x, 0.95, z), Vec3::new(0.2, 0.2, 0.2), iron).metal(0.3).rough(0.55));
        }
        // A low rail between the posts (the dead step through it: it marks the ground, not a wall).
        let (w, d) = (hi.x - lo.x, hi.z - lo.z);
        let rails = [
            ("rail_n", Vec3::new(centre.x, 0.7, lo.z), Vec3::new(w, 0.06, 0.06)),
            ("rail_s", Vec3::new(centre.x, 0.7, hi.z), Vec3::new(w, 0.06, 0.06)),
            ("rail_w", Vec3::new(lo.x, 0.7, centre.z), Vec3::new(0.06, 0.06, d)),
            ("rail_e", Vec3::new(hi.x, 0.7, centre.z), Vec3::new(0.06, 0.06, d)),
        ];
        for (name, pos, scale) in rails {
            ents::part(world, agent, gy, &Part::cube(name, pos, scale, iron).metal(0.3).rough(0.55));
        }
    }

    // Cliffs: strata bands of rock, recessed climbing lanes, ledges with shelves, dark mouths
    // at the base where climbers come from. The south face has no cells: plain strata.
    let cliffs = ents::group(world, agent, Some(slab), "cliffs", Vec3::ZERO, Vec3::ZERO, &[], Json::Null);
    let cliff_rows = (geo.depth / geo.row).round() as usize;
    for face in [Face::North, Face::East, Face::South, Face::West] {
        let grid = map.face(face).copied();
        let len = match face {
            Face::North | Face::South => cols,
            _ => rows,
        };
        let rim_void = |c: usize| {
            let (tc, tr) = match face {
                Face::North => (c, 0),
                Face::South => (c, rows - 1),
                Face::East => (cols - 1, c),
                _ => (0, c),
            };
            kind_at(tc, tr) == Some(CellKind::Void)
        };
        let gorge = |c: usize| face == Face::South && kind_at(c, rows - 1) == Some(CellKind::Gorge);
        let kind = |c: usize, r: usize| -> CellKind {
            match grid {
                Some(_) => map.at(face, c as u8, r as u8).map_or(CellKind::Rock, |id| map.cell(id).kind),
                None => CellKind::Rock,
            }
        };
        let fg = ents::group(world, agent, Some(cliffs), face.name(), Vec3::ZERO, Vec3::ZERO, &[], Json::Null);
        // key: 0/1 rock strata (by row parity), 2 lane, 3 base, 4 gorge mouth
        let key = |c: usize, r: usize| -> Option<u32> {
            if r == 0 && rim_void(c) {
                return None;
            }
            if r == 0 && gorge(c) {
                return Some(4);
            }
            Some(match kind(c, r) {
                CellKind::Climb => 2,
                CellKind::Base => 3,
                CellKind::Void => return None,
                _ => (r % 2) as u32,
            })
        };
        let out = SlabGeo::outward(face);
        let plane = match face {
            Face::North | Face::South => geo.half_z,
            _ => geo.half_x,
        };
        let place = |c0: f32, w: f32, r0: f32, h: f32, depth: f32| -> (Vec3, Vec3) {
            let along = -match face {
                Face::North | Face::South => geo.half_x,
                _ => geo.half_z,
            } + (c0 + w / 2.0) * geo.cell;
            let y = -(r0 + h / 2.0) * geo.row;
            let n = out * (plane + depth / 2.0);
            match face {
                Face::North | Face::South => (Vec3::new(along, y, n.z), Vec3::new(w * geo.cell, h * geo.row, depth)),
                _ => (Vec3::new(n.x, y, along), Vec3::new(depth, h * geo.row, w * geo.cell)),
            }
        };
        for (c, r, w, h, k) in rectangles(len, cliff_rows, key) {
            let (depth, color, glow) = match k {
                0 => (PANEL, CLIFF_A, 0.0),
                1 => (PANEL, CLIFF_B, 0.0),
                2 => (LANE_PANEL, shade(FISSURE, 1.15), 0.0),
                3 => (LANE_PANEL, hex(0x15181d), 0.0),
                _ => (PANEL + 0.02, hex(0x15181d), 0.0),
            };
            let (pos, scale) = place(c as f32, w as f32, r as f32, h as f32, depth);
            let name = match k {
                0 | 1 => format!("strata_{c}_{r}"),
                2 => format!("lane_{c}_{r}"),
                3 => format!("base_{c}_{r}"),
                _ => format!("gorge_{c}"),
            };
            ents::part_named(world, agent, fg, &name, &Part::cube("", pos, scale, color).rough(0.95).glow(glow));
            if k == 3 || k == 4 {
                // Where the dead come from: a faint necromantic glow at the mouth.
                let (gr, gh) = if k == 3 { (r as f32 + 0.85, 0.12) } else { (0.0, 0.1) };
                let (gpos, gscale) = place(c as f32 + 0.15, w as f32 - 0.3, gr, gh, depth + 0.03);
                ents::part_named(world, agent, fg, &format!("{name}_glow"), &Part::cube("", gpos, gscale, GRAVE_DEEP).glow(1.2));
            }
        }
        // Ledges: a shelf to build on.
        if grid.is_some() {
            for c in 0..len {
                for r in 0..cliff_rows {
                    if kind(c, r) == CellKind::Ledge {
                        let (mut pos, _) = place(c as f32, 1.0, r as f32, 1.0, SHELF_OUT);
                        pos.y -= 0.45;
                        let scale = match face {
                            Face::North | Face::South => Vec3::new(geo.cell, 0.3, SHELF_OUT),
                            _ => Vec3::new(SHELF_OUT, 0.3, geo.cell),
                        };
                        let name = format!("ledge_{c}_{r}");
                        ents::part_named(world, agent, fg, &name, &Part::cube("", pos, scale, SLAB_TOP).rough(0.9));
                    }
                }
            }
        }
    }

    slab
}

#[cfg(test)]
mod tests {
    use super::*;
    use sepulcra::Content;

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn rectangles_cover_each_cell_once() {
        let grid = ["aab", "aab", "cca"];
        let key = |c: usize, r: usize| Some(grid[r].as_bytes()[c] as u32);
        let rects = rectangles(3, 3, key);
        let mut covered = [[0; 3]; 3];
        for (c, r, w, h, k) in &rects {
            for rr in *r..r + h {
                for cc in *c..c + w {
                    covered[rr][cc] += 1;
                    assert_eq!(grid[rr].as_bytes()[cc] as u32, *k);
                }
            }
        }
        assert_eq!(covered, [[1; 3]; 3]);
        assert_eq!(rects.len(), 4, "{rects:?}");
    }

    #[test]
    fn rays_pick_the_cell_they_point_at() {
        let content = Content::builtin();
        let map = &content.map;
        let geo = SlabGeo::new(map);
        for name in ["top:12,9", "top:6,6", "top:9,19", "north:4,2", "east:6,2", "west:16,2", "east:12,0"] {
            let id = map.cell_ref(name).unwrap();
            let (anchor, normal) = geo.anchor(map, id);
            let eye = anchor + normal * 20.0 + Vec3::new(0.3, 9.0, 0.2);
            let dir = (anchor - eye).normalize();
            assert_eq!(geo.pick(map, eye, dir).map(|c| map.cell_name(c)), Some(name.to_owned()));
        }
        // From inside the slab, looking out, no cliff is hit.
        let id = geo.pick(map, Vec3::new(0.0, -5.0, 0.0), Vec3::X);
        assert_eq!(id, None);
    }
}

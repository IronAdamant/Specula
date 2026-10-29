//! The terrain renderer's data: heightfield textures, CDLOD selection, patch draws and the
//! per-frame terrain material records (docs/design/TERRAIN.md).
//!
//! A terrain is a heightfield of `columns × rows` samples `cell` apart, centred on its local
//! origin like the physics heightfield (`specula_physics::shape::Heightfield`): sample `(i, j)`
//! sits at `(x0 + i·cell.x, h, z0 + j·cell.z)` with `x0 = -(columns-1)/2·cell.x`, and each cell is
//! the triangles `[v00, v01, v10]`, `[v10, v01, v11]`.
//!
//! - **Selection** ([`select`], pure): a CDLOD quadtree walk (Strugar 2009) over a [`HeightTree`]
//!   of per-node height ranges; each selected node is a [`TerrainPatch`], drawn as one instance of
//!   a shared grid mesh ([`grid_mesh`]: [`GRID`] quads a side, or [`GRID`]/2 for the quarter of a
//!   node drawn at its parent's level), as a [`DrawItem`] with [`Material::Terrain`]. The draw plan
//!   culls, batches and shadows patches like any opaque draw.
//! - **Morph** (the vertex shader, mirrored by [`patch_vertex`]): a vertex at distance `d` from
//!   the camera collapses onto each coarser level's grid in turn, by `m_k = clamp((d - start_k) /
//!   (end_k - start_k))` for every level `k` from its patch's own: so a vertex's position is a
//!   function of its sample and its distance alone, whichever patch draws it. Selection puts the
//!   coarser side of every border outside the finer side's range (box tests), so the finer side's
//!   extra border vertices are fully collapsed there: no cracks between any two levels, however
//!   tall the node boxes (tested below).
//! - **Textures** ([`height_texels`], [`normal_texels`]): heights as their f32 bytes in RGBA8
//!   (bit exact: the drawn surface is the collider's), normals from central differences.

use crate::{DrawItem, Material, MaterialExt, MeshId, Surface, TextureId, Vertex};

/// Quads per side of a full patch.
pub const GRID: u32 = 16;
/// Most quadtree levels below the root.
pub const MAX_LEVELS: usize = 12;
/// Most samples per side of one terrain (the GPU's texture limit leaves room).
pub const MAX_SIDE: u32 = 4097;
/// Most terrains one frame draws ([`crate::FrameDesc::terrains`]).
pub const MAX_TERRAINS: usize = 256;
/// Splat layers a terrain blends.
pub const MAX_LAYERS: usize = 4;
/// Most patches one selection produces (past it, nodes stop subdividing: coarser, never missing).
pub const MAX_PATCHES: usize = 4096;
/// The part of each level's range over which its vertices morph to the next level.
pub const MORPH: f32 = 0.3;
/// `r0` is at least this times a leaf node's diagonal (see [`ranges`]).
pub const LEAF_FACTOR: f32 = 1.0;

/// One splat layer's material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainLayer {
    /// sRGB tint (the layer's colour without a texture).
    pub tint: [f32; 3],
    pub roughness: f32,
    pub specular: f32,
    /// Metres per texture repeat (world space, so tiles line up).
    pub uv_scale: f32,
    /// Slope (degrees) from which the layer's texture is projected from the sides as well
    /// (triplanar); 90 = never.
    pub triplanar_deg: f32,
    /// Base colour texture (sRGB; alpha = height for height blending, 1 = flat).
    pub albedo: Option<TextureId>,
    /// Tangent-space normal map (linear).
    pub normal: Option<TextureId>,
    pub normal_strength: f32,
}

impl Default for TerrainLayer {
    fn default() -> Self {
        TerrainLayer { tint: [0.5; 3], roughness: 0.9, specular: 0.05, uv_scale: 4.0, triplanar_deg: 35.0, albedo: None, normal: None, normal_strength: 1.0 }
    }
}

/// A terrain as one frame draws it (see the module docs for its local frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainMaterial {
    pub columns: u32,
    pub rows: u32,
    pub cell: [f32; 2],
    /// [`height_texels`]: `columns × rows`, `Rgba8Unorm`, read with `texelFetch`.
    pub heights: TextureId,
    /// [`normal_texels`] (with mips).
    pub normals: TextureId,
    /// Splat weights, RGBA8 = layers 0..3 (with mips).
    pub splat: TextureId,
    pub layers: [TerrainLayer; MAX_LAYERS],
    /// Layers in use (1..=4).
    pub layer_count: u32,
    /// Procedural grain on untextured layers, 0..1.
    pub grain: f32,
}

/// One selected quadtree node: [`GRID`]×[`GRID`] quads (or half that, `quarter`) from sample
/// `(i0, j0)`, a vertex every `2^level` samples, morphing to coarser levels on the ranges
/// `r0 · 2^(k + level_offset)` (world metres from the camera; see [`ranges`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainPatch {
    /// Index in [`crate::FrameDesc::terrains`].
    pub terrain: u32,
    pub i0: u32,
    pub j0: u32,
    pub level: u32,
    pub quarter: bool,
    /// `range[0]` of the full-resolution terrain (world metres).
    pub r0: f32,
    /// Levels of the full terrain above this data's level 0 (a coarse copy: 3).
    pub level_offset: u32,
    /// The data's root level: vertices never collapse past it (the root patch is the coarsest
    /// the terrain gets, however far away).
    pub root: u32,
    /// Skirt depth (metres, local): drawn on the sides in `skirt_mask` (bit 0 −x, 1 +x, 2 −z, 3 +z).
    pub skirt: f32,
    pub skirt_mask: u32,
}

/// Height range of every quadtree node: level 0 nodes cover [`GRID`] cells, each level above
/// twice that. Built once per terrain.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightTree {
    pub columns: u32,
    pub rows: u32,
    /// Per level: (nodes across, nodes down, `[min, max]` per node row by row).
    levels: Vec<(u32, u32, Vec<[f32; 2]>)>,
}

impl HeightTree {
    /// `heights`: `columns × rows`, row by row along +z. `None` if the sizes don't match.
    pub fn new(heights: &[f32], columns: u32, rows: u32) -> Option<HeightTree> {
        if columns < 2 || rows < 2 || columns > MAX_SIDE || rows > MAX_SIDE || heights.len() != (columns * rows) as usize {
            return None;
        }
        let (cw, rw) = (columns - 1, rows - 1);
        let nx = cw.div_ceil(GRID);
        let nz = rw.div_ceil(GRID);
        let mut leaf = Vec::with_capacity((nx * nz) as usize);
        for nj in 0..nz {
            for ni in 0..nx {
                let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
                for j in nj * GRID..=((nj + 1) * GRID).min(rw) {
                    for i in ni * GRID..=((ni + 1) * GRID).min(cw) {
                        let h = heights[(j * columns + i) as usize];
                        lo = lo.min(h);
                        hi = hi.max(h);
                    }
                }
                leaf.push([lo, hi]);
            }
        }
        let mut levels = vec![(nx, nz, leaf)];
        while {
            let (x, z, _) = levels.last().expect("one");
            *x > 1 || *z > 1
        } {
            let (px, pz, prev) = levels.last().expect("one");
            let (x, z) = (px.div_ceil(2), pz.div_ceil(2));
            let mut out = Vec::with_capacity((x * z) as usize);
            for j in 0..z {
                for i in 0..x {
                    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
                    for (ci, cj) in [(2 * i, 2 * j), (2 * i + 1, 2 * j), (2 * i, 2 * j + 1), (2 * i + 1, 2 * j + 1)] {
                        if ci < *px && cj < *pz {
                            let r = prev[(cj * px + ci) as usize];
                            lo = lo.min(r[0]);
                            hi = hi.max(r[1]);
                        }
                    }
                    out.push([lo, hi]);
                }
            }
            levels.push((x, z, out));
        }
        Some(HeightTree { columns, rows, levels })
    }

    /// The root level (its nodes cover the terrain: at most a few of them).
    pub fn root_level(&self) -> u32 {
        (self.levels.len() - 1) as u32
    }

    /// Height range of node `(ni, nj)` at `level` (`None` outside the terrain).
    pub fn range(&self, level: u32, ni: u32, nj: u32) -> Option<[f32; 2]> {
        let (x, z, r) = self.levels.get(level as usize)?;
        (ni < *x && nj < *z).then(|| r[(nj * x + ni) as usize])
    }

    /// The whole terrain's height range.
    pub fn bounds(&self) -> [f32; 2] {
        let (_, _, r) = self.levels.last().expect("one");
        r.iter().fold([f32::INFINITY, f32::NEG_INFINITY], |a, b| [a[0].min(b[0]), a[1].max(b[1])])
    }

    /// Bytes this tree holds (memory reports).
    pub fn bytes(&self) -> usize {
        self.levels.iter().map(|(_, _, r)| r.len() * 8).sum()
    }
}

/// LOD ranges (world metres) of levels `0..MAX_LEVELS`: `range[k] = r0 · 2^k` with `r0 = max(
/// detail_m, LEAF_FACTOR · leaf diagonal)`, where a leaf is [`GRID`] cells of `cell_world`
/// (the full-resolution cell times the terrain's largest scale). The second term keeps the
/// selection crack free (see the module docs); tiles of one terrain get the same ranges.
pub fn ranges(detail_m: f32, cell_world: f32) -> [f32; MAX_LEVELS] {
    let leaf = GRID as f32 * cell_world * std::f32::consts::SQRT_2;
    let r0 = detail_m.max(LEAF_FACTOR * leaf).max(1e-3);
    std::array::from_fn(|k| r0 * (1u64 << k) as f32)
}

/// Morph start and end of the full terrain's `level` (its vertices collapse onto level
/// `level + 1`'s grid between them) for `range[0]` = `r0`: the last [`MORPH`] of the level's band.
pub fn morph_range(r0: f32, level: u32) -> [f32; 2] {
    let end = r0 * (1u64 << level.min(40)) as f32;
    let before = if level == 0 { 0.0 } else { end * 0.5 };
    [end - MORPH * (end - before), end]
}

/// What [`select`] needs about one terrain.
#[derive(Clone, Copy, Debug)]
pub struct Selection<'a> {
    pub tree: &'a HeightTree,
    pub cell: [f32; 2],
    /// Local → world (column-major).
    pub world: &'a [f32; 16],
    /// The world-space LOD ranges ([`ranges`], from the full-resolution cell).
    pub ranges: &'a [f32; MAX_LEVELS],
    /// Levels of the full terrain above this data's level 0 (a coarse copy every 8th sample: 3).
    pub level_offset: u32,
    pub terrain: u32,
    /// Skirt depth (local metres; 0 = none) and the sides that get it.
    pub skirt: f32,
    pub skirt_mask: u32,
}

/// A selected patch and its world bounding sphere (centre, radius).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selected {
    pub patch: TerrainPatch,
    pub sphere: [f32; 4],
}

fn transform(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// World AABB of a local box (all eight corners transformed: conservative under rotation).
fn world_box(m: &[f32; 16], lo: [f32; 3], hi: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let (mut a, mut b) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for c in 0..8 {
        let p = [if c & 1 == 0 { lo[0] } else { hi[0] }, if c & 2 == 0 { lo[1] } else { hi[1] }, if c & 4 == 0 { lo[2] } else { hi[2] }];
        let w = transform(m, p);
        for k in 0..3 {
            a[k] = a[k].min(w[k]);
            b[k] = b[k].max(w[k]);
        }
    }
    (a, b)
}

fn box_meets_sphere(lo: [f32; 3], hi: [f32; 3], c: [f32; 3], r: f32) -> bool {
    let mut d2 = 0.0f32;
    for k in 0..3 {
        let v = if c[k] < lo[k] {
            lo[k] - c[k]
        } else if c[k] > hi[k] {
            c[k] - hi[k]
        } else {
            0.0
        };
        d2 += v * v;
    }
    d2 <= r * r
}

/// Selects the patches of one terrain for a camera at `eye` (world) into `out`. Never culls by
/// the frustum (the draw plan does, per view), so the camera, the shadow cascades and the ID
/// buffer draw the same patches. Deterministic in its inputs; at most [`MAX_PATCHES`] (past it,
/// nodes stop subdividing).
pub fn select(s: &Selection, eye: [f32; 3], out: &mut Vec<Selected>) {
    let start = out.len();
    let root = s.tree.root_level();
    let (rx, rz, _) = &s.tree.levels[root as usize];
    for nj in 0..*rz {
        for ni in 0..*rx {
            if !select_node(s, eye, root, ni, nj, start, out) {
                // Beyond every range: the root patch, the coarsest the terrain has.
                if let Some(h) = s.tree.range(root, ni, nj) {
                    let cells = GRID << root;
                    push(s, out, root, ni * cells, nj * cells, false, h);
                }
            }
        }
    }
}

/// The local box of node `(ni, nj)` at `level`, drawn from sample `(i0, j0)` over `cells` cells
/// (clamped to the terrain), with the skirt below.
fn node_box(s: &Selection, i0: u32, j0: u32, cells: u32, h: [f32; 2]) -> ([f32; 3], [f32; 3]) {
    let t = s.tree;
    let (x0, z0) = (-((t.columns - 1) as f32) * s.cell[0] * 0.5, -((t.rows - 1) as f32) * s.cell[1] * 0.5);
    let i1 = (i0 + cells).min(t.columns - 1);
    let j1 = (j0 + cells).min(t.rows - 1);
    (
        [x0 + i0 as f32 * s.cell[0], h[0] - s.skirt, z0 + j0 as f32 * s.cell[1]],
        [x0 + i1 as f32 * s.cell[0], h[1], z0 + j1 as f32 * s.cell[1]],
    )
}

fn push(s: &Selection, out: &mut Vec<Selected>, level: u32, i0: u32, j0: u32, quarter: bool, h: [f32; 2]) {
    let cells = (GRID << level) >> quarter as u32;
    let (lo, hi) = node_box(s, i0, j0, cells, h);
    let (a, b) = world_box(s.world, lo, hi);
    let c = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5];
    let r = (0..3).map(|k| (b[k] - a[k]) * (b[k] - a[k])).sum::<f32>().sqrt() * 0.5;
    out.push(Selected {
        patch: TerrainPatch {
            terrain: s.terrain,
            i0,
            j0,
            level,
            quarter,
            r0: s.ranges[0],
            level_offset: s.level_offset,
            root: s.tree.root_level(),
            skirt: s.skirt,
            skirt_mask: s.skirt_mask,
        },
        sphere: [c[0], c[1], c[2], r],
    });
}

/// Strugar's selection: `true` when node `(ni, nj)` of `level` (or its children) drew its area.
fn select_node(s: &Selection, eye: [f32; 3], level: u32, ni: u32, nj: u32, start: usize, out: &mut Vec<Selected>) -> bool {
    let Some(h) = s.tree.range(level, ni, nj) else { return true };
    let cells = GRID << level;
    let (i0, j0) = (ni * cells, nj * cells);
    let (lo, hi) = node_box(s, i0, j0, cells, h);
    let (a, b) = world_box(s.world, lo, hi);
    let full = (level + s.level_offset) as usize;
    if !box_meets_sphere(a, b, eye, s.ranges[full.min(MAX_LEVELS - 1)]) {
        return false;
    }
    let full_out = out.len() - start >= MAX_PATCHES - 4;
    if level == 0 || full == 0 || full_out || !box_meets_sphere(a, b, eye, s.ranges[(full - 1).min(MAX_LEVELS - 1)]) {
        push(s, out, level, i0, j0, false, h);
        return true;
    }
    for (ci, cj) in [(2 * ni, 2 * nj), (2 * ni + 1, 2 * nj), (2 * ni, 2 * nj + 1), (2 * ni + 1, 2 * nj + 1)] {
        let Some(ch) = s.tree.range(level - 1, ci, cj) else { continue };
        if !select_node(s, eye, level - 1, ci, cj, start, out) {
            // The child is beyond the finer range: its quarter at this node's level.
            push(s, out, level, ci * (cells / 2), cj * (cells / 2), true, ch);
        }
    }
    true
}

/// The shared patch grids every `Gpu` holds ([`grid_mesh`] of [`GRID`] and [`GRID`]/2 quads),
/// under ids `upload_mesh` never hands out.
pub const GRID_MESH: MeshId = MeshId(u32::MAX - 1);
pub const QUARTER_MESH: MeshId = MeshId(u32::MAX - 2);

/// The patch draw of a selected node: its grid mesh ([`GRID_MESH`] or [`QUARTER_MESH`]), the
/// terrain's local → world matrix and the node's bounding sphere.
pub fn patch_draw(world: &[f32; 16], s: &Selected) -> DrawItem {
    DrawItem {
        mesh: if s.patch.quarter { QUARTER_MESH } else { GRID_MESH },
        model: *world,
        color: [1.0; 4],
        material: Material::Terrain,
        surface: Surface { ext: MaterialExt::Terrain(s.patch), ..Surface::default() },
        skin: None,
        bounds: Some(s.sphere),
    }
}

/// The shared patch grid: `n` quads a side ([`GRID`], or [`GRID`]/2 for quarters) plus a skirt
/// ring. Each vertex's position holds its grid coordinates `(x, skirt, z)`: `x, z` in `0..=n`, and
/// `skirt` = 1 on the ring (whose `x, z` repeat the border's). Cells split v10-v01 as the physics
/// heightfield's.
pub fn grid_mesh(n: u32) -> (Vec<Vertex>, Vec<u32>) {
    // Ring coordinates -1..=n+1, the outer ones clamped onto the border and flagged.
    let side = n + 3;
    let mut v = Vec::with_capacity((side * side) as usize);
    for z in 0..side {
        for x in 0..side {
            let (gx, gz) = (x as i32 - 1, z as i32 - 1);
            let ring = gx < 0 || gz < 0 || gx > n as i32 || gz > n as i32;
            let (cx, cz) = (gx.clamp(0, n as i32), gz.clamp(0, n as i32));
            v.push(Vertex::new([cx as f32, ring as u32 as f32, cz as f32], [0.0, 1.0, 0.0]));
        }
    }
    let mut idx = Vec::with_capacity(((side - 1) * (side - 1) * 6) as usize);
    for z in 0..side - 1 {
        for x in 0..side - 1 {
            let v00 = z * side + x;
            let (v10, v01, v11) = (v00 + 1, v00 + side, v00 + side + 1);
            idx.extend([v00, v01, v10, v10, v01, v11]);
        }
    }
    (v, idx)
}

/// Heights as `Rgba8Unorm` texels: each f32's little-endian bytes (the shader reassembles the bits).
pub fn height_texels(heights: &[f32]) -> Vec<u8> {
    heights.iter().flat_map(|h| h.to_bits().to_le_bytes()).collect()
}

/// Smooth normals (local space, central differences, one-sided at the border) as `Rgba8Unorm`:
/// `xyz · 0.5 + 0.5`, alpha 255.
pub fn normal_texels(heights: &[f32], columns: u32, rows: u32, cell: [f32; 2]) -> Vec<u8> {
    normal_texels_apron(heights, columns, rows, cell, &[None; 4])
}

/// [`normal_texels`] with the neighbouring tiles' samples just past each border (`apron`: the
/// −x column and +x column, `rows` long; the −z row and +z row, `columns` long), so a border
/// sample gets the same central difference from both tiles: no lighting seam where tiles meet.
pub fn normal_texels_apron(heights: &[f32], columns: u32, rows: u32, cell: [f32; 2], apron: &[Option<&[f32]>; 4]) -> Vec<u8> {
    let (c, r) = (columns as usize, rows as usize);
    let at = |i: usize, j: usize| heights[j * c + i];
    let side = |k: usize, n: usize| apron[k].filter(|a| a.len() == n);
    let mut out = Vec::with_capacity(c * r * 4);
    for j in 0..r {
        for i in 0..c {
            // Left and right neighbours of the sample (the apron past a border), and their span.
            let (hl, sl) = match (i, side(0, r)) {
                (0, Some(a)) => (a[j], 1),
                (0, None) => (at(0, j), 0),
                _ => (at(i - 1, j), 1),
            };
            let (hr, sr) = match (i + 1 == c, side(1, r)) {
                (true, Some(a)) => (a[j], 1),
                (true, None) => (at(i, j), 0),
                _ => (at(i + 1, j), 1),
            };
            let (hd, sd) = match (j, side(2, c)) {
                (0, Some(a)) => (a[i], 1),
                (0, None) => (at(i, 0), 0),
                _ => (at(i, j - 1), 1),
            };
            let (hu, su) = match (j + 1 == r, side(3, c)) {
                (true, Some(a)) => (a[i], 1),
                (true, None) => (at(i, j), 0),
                _ => (at(i, j + 1), 1),
            };
            let gx = (hr - hl) / ((sl + sr).max(1) as f32 * cell[0]);
            let gz = (hu - hd) / ((sd + su).max(1) as f32 * cell[1]);
            let (nx, ny, nz) = (-gx, 1.0, -gz);
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            let q = |v: f32| ((v / len * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
            out.extend([q(nx), q(ny), q(nz), 255]);
        }
    }
    out
}

/// Every `factor`-th sample of a `columns × rows` grid (`Some` only when both sides divide):
/// `(columns', rows', values)`. A coarse copy whose level 0 is the full grid's level
/// `log2(factor)`.
pub fn subsample<T: Copy>(values: &[T], columns: u32, rows: u32, factor: u32) -> Option<(u32, u32, Vec<T>)> {
    if factor == 0 || (columns - 1) % factor != 0 || (rows - 1) % factor != 0 || values.len() != (columns * rows) as usize {
        return None;
    }
    let (c, r) = ((columns - 1) / factor + 1, (rows - 1) / factor + 1);
    let mut out = Vec::with_capacity((c * r) as usize);
    for j in 0..r {
        for i in 0..c {
            out.push(values[((j * factor) * columns + i * factor) as usize]);
        }
    }
    Some((c, r, out))
}

/// The world position of grid vertex `(gx, gz)` of `p` for a camera at `eye`: exactly what
/// `shaders/terrain.glsl` computes (keep them in step), for the seam tests and for agents'
/// tools. `heights`: the terrain's `columns × rows`.
#[allow(clippy::too_many_arguments)]
pub fn patch_vertex(heights: &[f32], columns: u32, rows: u32, cell: [f32; 2], world: &[f32; 16], p: &TerrainPatch, g: (u32, u32), eye: [f32; 3]) -> [f32; 3] {
    let step = (1u32 << p.level) as f32;
    let (cw, rw) = ((columns - 1) as f32, (rows - 1) as f32);
    let base = [p.i0 as f32 + g.0 as f32 * step, p.j0 as f32 + g.1 as f32 * step];
    let x0 = -cw * cell[0] * 0.5;
    let z0 = -rw * cell[1] * 0.5;
    let fetch = |i: f32, j: f32| heights[(j as u32 * columns + i as u32) as usize];
    // Collapse onto each coarser level's grid in turn (data levels; ranges of the full terrain).
    // Each step's distance is measured at the vertex as it stands on that level's grid (a vertex
    // the coarser patch has too), so both sides of a border agree step by step.
    let mut q = base;
    for k in p.level..p.root.min(MAX_LEVELS as u32) {
        let c = [q[0].min(cw), q[1].min(rw)];
        let h = fetch(c[0], c[1]);
        let w = transform(world, [x0 + c[0] * cell[0], h, z0 + c[1] * cell[1]]);
        let d = ((w[0] - eye[0]).powi(2) + (w[1] - eye[1]).powi(2) + (w[2] - eye[2]).powi(2)).sqrt();
        let [start, end] = morph_range(p.r0, k + p.level_offset);
        let m = ((d - start) / (end - start).max(1e-6)).clamp(0.0, 1.0);
        if m <= 0.0 {
            break;
        }
        let span = (2u32 << k) as f32;
        let snapped = [q[0] - q[0] % span, q[1] - q[1] % span];
        q = [q[0] + (snapped[0] - q[0]) * m, q[1] + (snapped[1] - q[1]) * m];
        if m < 1.0 {
            break;
        }
    }
    let mp = [q[0].min(cw), q[1].min(rw)];
    // Bilinear between samples (only while morphing; on samples it is the sample).
    let (fi, fj) = (mp[0].floor(), mp[1].floor());
    let (u, v) = (mp[0] - fi, mp[1] - fj);
    let (i1, j1) = ((fi + 1.0).min(cw), (fj + 1.0).min(rw));
    let h00 = fetch(fi, fj);
    let h10 = fetch(i1, fj);
    let h01 = fetch(fi, j1);
    let h11 = fetch(i1, j1);
    let hm = if u == 0.0 && v == 0.0 { h00 } else { (h00 * (1.0 - u) + h10 * u) * (1.0 - v) + (h01 * (1.0 - u) + h11 * u) * v };
    transform(world, [x0 + mp[0] * cell[0], hm, z0 + mp[1] * cell[1]])
}

// ---------------------------------------------------------------------------------------------
// GPU records

/// One layer as the shader reads it; must match `TerrainLayer` in `shaders/terrain.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayerRecord {
    /// Linear tint, roughness.
    pub tint_rough: [f32; 4],
    /// Specular, 1 / uv scale, cos(triplanar slope), normal strength.
    pub params: [f32; 4],
    /// Albedo slot + 1, normal slot + 1 (0 = none), unused ×2.
    pub tex: [u32; 4],
}

/// One terrain as the shader reads it (256 bytes); must match `Terrain` in `shaders/terrain.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerrainRecord {
    /// x0, z0 (local position of sample 0), cell x, cell z.
    pub grid: [f32; 4],
    /// Columns, rows, heights slot + 1, normals slot + 1.
    pub size: [u32; 4],
    /// Splat slot + 1, layer count, unused ×2.
    pub more: [u32; 4],
    /// Grain, unused ×3.
    pub look: [f32; 4],
    pub layers: [LayerRecord; MAX_LAYERS],
}

const _: () = assert!(std::mem::size_of::<TerrainRecord>() == 256);

/// The record of `m`, its texture slots looked up by `slot` (`None` = missing: the caller has
/// refused the frame already).
pub(crate) fn record(m: &TerrainMaterial, linear: &mut impl FnMut([f32; 3]) -> [f32; 3], slot: &impl Fn(TextureId) -> Option<u32>) -> TerrainRecord {
    let s = |t: Option<TextureId>| t.and_then(slot).map_or(0, |s| s + 1);
    let mut layers = [LayerRecord::default(); MAX_LAYERS];
    for (out, l) in layers.iter_mut().zip(&m.layers) {
        let t = linear(l.tint);
        out.tint_rough = [t[0], t[1], t[2], l.roughness.clamp(0.02, 1.0)];
        let cos = (l.triplanar_deg.clamp(0.0, 90.0)).to_radians().cos();
        out.params = [l.specular.max(0.0), 1.0 / l.uv_scale.max(1e-3), if l.triplanar_deg >= 90.0 { -1.0 } else { cos }, l.normal_strength.clamp(0.0, 4.0)];
        out.tex = [s(l.albedo), s(l.normal), 0, 0];
    }
    TerrainRecord {
        grid: [-((m.columns - 1) as f32) * m.cell[0] * 0.5, -((m.rows - 1) as f32) * m.cell[1] * 0.5, m.cell[0], m.cell[1]],
        size: [m.columns, m.rows, s(Some(m.heights)), s(Some(m.normals))],
        more: [s(Some(m.splat)), m.layer_count.clamp(1, MAX_LAYERS as u32), 0, 0],
        look: [m.grain.clamp(0.0, 1.0), 0.0, 0.0, 0.0],
        layers,
    }
}

/// The patch's draw record fields beyond its model matrix: `color` = (i0, j0, step, quarter),
/// `color2` = (r0, level offset, skirt depth, 0), `ext` = (terrain, skirt mask, level, root).
pub(crate) fn patch_fields(p: &TerrainPatch) -> ([f32; 4], [f32; 4], [u32; 4]) {
    (
        [p.i0 as f32, p.j0 as f32, (1u32 << p.level.min(30)) as f32, p.quarter as u32 as f32],
        [p.r0, p.level_offset as f32, p.skirt.max(0.0), 0.0],
        [p.terrain, p.skirt_mask, p.level, p.root],
    )
}

/// Every texture a terrain material names.
pub(crate) fn textures(m: &TerrainMaterial) -> impl Iterator<Item = TextureId> + '_ {
    [m.heights, m.normals, m.splat].into_iter().chain(m.layers.iter().take(m.layer_count as usize).flat_map(|l| l.albedo.into_iter().chain(l.normal)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

    /// A rough, steep terrain (hash noise at two scales plus a cliff).
    fn rough(columns: u32, rows: u32, amp: f32) -> Vec<f32> {
        let mut v = Vec::new();
        for j in 0..rows {
            for i in 0..columns {
                let h = |a: u32, b: u32| {
                    let mut x = a.wrapping_mul(0x9e37_79b9) ^ b.wrapping_mul(0x85eb_ca6b);
                    x ^= x >> 13;
                    x = x.wrapping_mul(0xc2b2_ae35);
                    (x >> 8) as f32 / 16_777_216.0
                };
                let coarse = h(i / 8, j / 8) * amp;
                let fine = h(i, j) * amp * 0.1;
                let cliff = if i > columns / 2 { amp * 0.5 } else { 0.0 };
                v.push(coarse + fine + cliff);
            }
        }
        v
    }

    #[test]
    fn patch_draws_are_opaque_and_pack_their_patch() {
        let p = TerrainPatch { terrain: 2, i0: 32, j0: 16, level: 1, quarter: true, r0: 56.5, level_offset: 3, root: 4, skirt: 0.5, skirt_mask: 15 };
        let d = patch_draw(&ID, &Selected { patch: p, sphere: [1.0, 2.0, 3.0, 4.0] });
        assert!(!d.material.blended(), "terrain is drawn with the opaque meshes");
        assert!(crate::Material::Water.blended());
        assert_eq!((d.mesh, d.bounds), (QUARTER_MESH, Some([1.0, 2.0, 3.0, 4.0])));
        let r = crate::render::draw_record(&d);
        assert_eq!(r.color, [32.0, 16.0, 2.0, 1.0]);
        assert_eq!(r.color2, [56.5, 3.0, 0.5, 0.0]);
        assert_eq!(r.ext, [2, 15, 1, 4]);
        assert_eq!(r.model, ID);
    }

    #[test]
    fn height_texels_round_trip_bits() {
        let h = [0.0f32, -1.5, 1234.5678, f32::MIN_POSITIVE];
        let t = height_texels(&h);
        for (k, x) in h.iter().enumerate() {
            let b = [t[k * 4], t[k * 4 + 1], t[k * 4 + 2], t[k * 4 + 3]];
            // As the shader: unorm8 → round(v · 255) → the byte back, exactly.
            let back: Vec<u8> = b.iter().map(|&y| ((y as f32 / 255.0) * 255.0).round() as u8).collect();
            assert_eq!(f32::from_bits(u32::from_le_bytes([back[0], back[1], back[2], back[3]])).to_bits(), x.to_bits());
        }
    }

    #[test]
    fn height_tree_covers_every_sample() {
        let (c, r) = (37u32, 21u32);
        let h = rough(c, r, 10.0);
        let t = HeightTree::new(&h, c, r).unwrap();
        // Leaves: ceil(36/16) × ceil(20/16) = 3 × 2; root covers all.
        assert_eq!(t.levels[0].0, 3);
        assert_eq!(t.levels[0].1, 2);
        let all = t.bounds();
        assert_eq!(all, [h.iter().cloned().fold(f32::INFINITY, f32::min), h.iter().cloned().fold(f32::NEG_INFINITY, f32::max)]);
        for j in 0..r {
            for i in 0..c {
                let (ni, nj) = ((i / GRID).min(2), (j / GRID).min(1));
                let rg = t.range(0, ni, nj).unwrap();
                let v = h[(j * c + i) as usize];
                assert!(v >= rg[0] && v <= rg[1]);
            }
        }
        assert!(HeightTree::new(&h, c, r + 1).is_none());
    }

    #[test]
    fn grid_mesh_splits_cells_like_the_heightfield() {
        let (v, idx) = grid_mesh(GRID);
        assert_eq!(v.len(), ((GRID + 3) * (GRID + 3)) as usize);
        assert_eq!(idx.len(), ((GRID + 2) * (GRID + 2) * 6) as usize);
        // First interior cell: [v00, v01, v10], [v10, v01, v11] with v01 one row (+z) further.
        let side = GRID + 3;
        let c = side + 1; // grid (0, 0)
        let k = ((side - 1) + 1) as usize * 6; // that cell's first index
        assert_eq!(&idx[k..k + 6], &[c, c + side, c + 1, c + 1, c + side, c + side + 1]);
        assert_eq!(v[c as usize].position, [0.0, 0.0, 0.0]);
        // The ring is flagged and clamped.
        assert_eq!(v[0].position, [0.0, 1.0, 0.0]);
        assert_eq!(v[(side * side - 1) as usize].position, [GRID as f32, 1.0, GRID as f32]);
    }

    #[test]
    fn subsample_takes_every_nth_and_refuses_misfits() {
        let v: Vec<u32> = (0..17 * 9).collect();
        let (c, r, s) = subsample(&v, 17, 9, 8).unwrap();
        assert_eq!((c, r), (3, 2));
        assert_eq!(s, vec![0, 8, 16, 136, 144, 152]);
        assert!(subsample(&v, 17, 9, 3).is_none());
    }

    #[test]
    fn neighbouring_tiles_light_their_shared_border_alike() {
        // One 5 x 3 slope split into two tiles sharing column 2: with each other's next column as
        // the apron, the shared column's normals are the same from both sides.
        let whole: Vec<f32> = (0..15).map(|k| ((k % 5) as f32).powi(2) * 0.3 + (k / 5) as f32 * 0.2).collect();
        let tile = |i0: usize| -> Vec<f32> { (0..3).flat_map(|j| (0..3).map(move |i| (j, i0 + i))).map(|(j, i)| whole[j * 5 + i]).collect() };
        let (a, b) = (tile(0), tile(2));
        let col = |t: &[f32], i: usize| -> Vec<f32> { (0..3).map(|j| t[j * 3 + i]).collect() };
        let (a_next, b_prev) = (col(&b, 1), col(&a, 1));
        let na = normal_texels_apron(&a, 3, 3, [1.0, 1.0], &[None, Some(&a_next), None, None]);
        let nb = normal_texels_apron(&b, 3, 3, [1.0, 1.0], &[Some(&b_prev), None, None, None]);
        let full = normal_texels(&whole, 5, 3, [1.0, 1.0]);
        for j in 0..3 {
            let pa = &na[(j * 3 + 2) * 4..(j * 3 + 2) * 4 + 4];
            let pb = &nb[(j * 3) * 4..(j * 3) * 4 + 4];
            assert_eq!(pa, pb, "row {j}");
            assert_eq!(pa, &full[(j * 5 + 2) * 4..(j * 5 + 2) * 4 + 4], "as if one terrain");
        }
        // Without the apron the two sides disagree (the seam this fixes).
        let (na, nb) = (normal_texels(&a, 3, 3, [1.0, 1.0]), normal_texels(&b, 3, 3, [1.0, 1.0]));
        assert_ne!(&na[8..12], &nb[0..4]);
    }

    #[test]
    fn normals_face_up_on_flat_ground_and_tilt_on_slopes() {
        let flat = vec![2.0f32; 9];
        let n = normal_texels(&flat, 3, 3, [1.0, 1.0]);
        assert_eq!(&n[16..20], &[128, 255, 128, 255]);
        let slope: Vec<f32> = (0..9).map(|k| (k % 3) as f32).collect(); // rises along +x at 45°
        let n = normal_texels(&slope, 3, 3, [1.0, 1.0]);
        let x = n[16] as f32 / 255.0 * 2.0 - 1.0;
        let y = n[17] as f32 / 255.0 * 2.0 - 1.0;
        assert!((x + 0.707).abs() < 0.01 && (y - 0.707).abs() < 0.01, "{x} {y}");
    }

    fn rot_y(deg: f32, t: [f32; 3], s: f32) -> [f32; 16] {
        let (sn, cs) = deg.to_radians().sin_cos();
        [cs * s, 0.0, -sn * s, 0.0, 0.0, s, 0.0, 0.0, sn * s, 0.0, cs * s, 0.0, t[0], t[1], t[2], 1.0]
    }

    /// Every vertex on a patch's border, from that patch's side (the skirt ring excluded).
    fn border(h: &[f32], c: u32, r: u32, cell: [f32; 2], w: &[f32; 16], p: &TerrainPatch, eye: [f32; 3]) -> Vec<(u32, u32, [f32; 3])> {
        let n = if p.quarter { GRID / 2 } else { GRID };
        let step = 1u32 << p.level;
        let mut out = Vec::new();
        for g in 0..=n {
            for (gx, gz) in [(g, 0), (g, n), (0, g), (n, g)] {
                let (si, sj) = (p.i0 + gx * step, p.j0 + gz * step);
                out.push((si, sj, patch_vertex(h, c, r, cell, w, p, (gx, gz), eye)));
            }
        }
        out
    }

    /// The polyline of a patch side, as points along it (sorted by the sample coordinate).
    fn side_points(pts: &[(u32, u32, [f32; 3])], fixed_i: Option<u32>, fixed_j: Option<u32>, lo: u32, hi: u32) -> Vec<[f32; 3]> {
        let mut v: Vec<(u32, [f32; 3])> = pts
            .iter()
            .filter(|(i, j, _)| fixed_i.is_none_or(|f| *i == f) && fixed_j.is_none_or(|f| *j == f))
            .map(|(i, j, p)| (if fixed_i.is_some() { *j } else { *i }, *p))
            .filter(|(k, _)| *k >= lo && *k <= hi)
            .collect();
        v.sort_by_key(|(k, _)| *k);
        v.dedup_by_key(|(k, _)| *k);
        v.into_iter().map(|(_, p)| p).collect()
    }

    /// Height of the polyline `pts` (sorted along `axis`) at coordinate `x`.
    fn along(pts: &[[f32; 3]], axis: usize, x: f32) -> Option<f32> {
        // Degenerate (collapsed) points share positions: skip zero-length segments.
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (lo, hi) = (a[axis].min(b[axis]), a[axis].max(b[axis]));
            if x >= lo - 1e-4 && x <= hi + 1e-4 {
                if (b[axis] - a[axis]).abs() < 1e-6 {
                    return Some(a[1]);
                }
                let t = (x - a[axis]) / (b[axis] - a[axis]);
                return Some(a[1] + (b[1] - a[1]) * t);
            }
        }
        None
    }

    /// The seam property: wherever two selected patches share a border, the surfaces both sides
    /// draw along it are the same line (no cracks), and neighbours differ by at most one level.
    #[test]
    fn selections_are_crack_free_on_steep_terrain_from_many_cameras() {
        let (c, r) = (257u32, 257u32);
        let cell = [1.0f32, 1.0];
        let h = rough(c, r, 40.0);
        let tree = HeightTree::new(&h, c, r).unwrap();
        let mut worst = 0.0f32;
        let mut checked = 0usize;
        for (wi, world) in [ID, rot_y(30.0, [5.0, -3.0, 2.0], 1.0), rot_y(-75.0, [0.0, 0.0, 0.0], 2.0)].iter().enumerate() {
            let rg = ranges(0.0, cell[0] * if wi == 2 { 2.0 } else { 1.0 });
            let s = Selection { tree: &tree, cell, world, ranges: &rg, level_offset: 0, terrain: 0, skirt: 0.0, skirt_mask: 0 };
            for k in 0..24u32 {
                let a = k as f32 * 0.9;
                let rad = 20.0 + 15.0 * k as f32;
                let eye = transform(world, [a.cos() * rad * 0.6, 30.0 + (k % 5) as f32 * 25.0, a.sin() * rad * 0.6]);
                let mut out = Vec::new();
                select(&s, eye, &mut out);
                assert!(!out.is_empty());
                // Areas tile the terrain exactly once (sum of patch cells, clamped).
                let area: u64 = out
                    .iter()
                    .map(|p| {
                        let n = ((GRID << p.patch.level) >> p.patch.quarter as u32) as u64;
                        let w = (p.patch.i0 as u64 + n).min((c - 1) as u64) - p.patch.i0 as u64;
                        let d = (p.patch.j0 as u64 + n).min((r - 1) as u64) - p.patch.j0 as u64;
                        w * d
                    })
                    .sum();
                assert_eq!(area, ((c - 1) * (r - 1)) as u64, "patches cover the terrain once");
                let rects: Vec<(u32, u32, u32, u32, &TerrainPatch)> = out
                    .iter()
                    .map(|p| {
                        let n = (GRID << p.patch.level) >> p.patch.quarter as u32;
                        (p.patch.i0, p.patch.j0, (p.patch.i0 + n).min(c - 1), (p.patch.j0 + n).min(r - 1), &p.patch)
                    })
                    .collect();
                for (ai, a) in rects.iter().enumerate() {
                    for b in rects.iter().skip(ai + 1) {
                        // Shared vertical side (a's +x = b's −x, or the reverse), then horizontal.
                        for (pa, pb) in [(a, b), (b, a)] {
                            let pts_a = border(&h, c, r, cell, world, pa.4, eye);
                            let pts_b = border(&h, c, r, cell, world, pb.4, eye);
                            let seams = [
                                (pa.2 == pb.0, Some(pa.2), None, pa.1.max(pb.1), pa.3.min(pb.3)),
                                (pa.3 == pb.1, None, Some(pa.3), pa.0.max(pb.0), pa.2.min(pb.2)),
                            ];
                            for (shared, fi, fj, lo, hi) in seams {
                                if !shared || lo >= hi {
                                    continue;
                                }
                                assert!((pa.4.level as i64 - pb.4.level as i64).abs() <= 1, "neighbours within one level: {:?} {:?}", pa.4, pb.4);
                                let sa = side_points(&pts_a, fi, fj, lo, hi);
                                let sb = side_points(&pts_b, fi, fj, lo, hi);
                                // Compare along the seam in local sample coordinates: map each
                                // point back to the sample axis via its world position order.
                                let axis = if fi.is_some() { 2 } else { 0 };
                                // Work in local space (undo the world transform for the axis).
                                let to_local = |p: [f32; 3]| -> [f32; 3] { inverse_rigid(world, p) };
                                let la: Vec<[f32; 3]> = sa.iter().map(|p| to_local(*p)).collect();
                                let lb: Vec<[f32; 3]> = sb.iter().map(|p| to_local(*p)).collect();
                                for p in la.iter().chain(&lb) {
                                    let ya = along(&la, axis, p[axis]).expect("on a");
                                    let yb = along(&lb, axis, p[axis]).expect("on b");
                                    worst = worst.max((ya - yb).abs());
                                    checked += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > 1000, "seams checked: {checked}");
        assert!(worst < 1e-4, "largest gap along a seam: {worst} m");
    }

    /// Local coordinates of world point `p` under a rotation × uniform scale + translation.
    fn inverse_rigid(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
        let t = [p[0] - m[12], p[1] - m[13], p[2] - m[14]];
        let s2 = m[0] * m[0] + m[1] * m[1] + m[2] * m[2];
        // Rᵀ·t / s (columns are s·R's columns).
        [
            (m[0] * t[0] + m[1] * t[1] + m[2] * t[2]) / s2,
            (m[4] * t[0] + m[5] * t[1] + m[6] * t[2]) / s2,
            (m[8] * t[0] + m[9] * t[1] + m[10] * t[2]) / s2,
        ]
    }

    #[test]
    fn near_the_camera_the_full_resolution_is_drawn_exactly() {
        let (c, r) = (129u32, 129u32);
        let h = rough(c, r, 10.0);
        let tree = HeightTree::new(&h, c, r).unwrap();
        let rg = ranges(0.0, 1.0);
        let s = Selection { tree: &tree, cell: [1.0, 1.0], world: &ID, ranges: &rg, level_offset: 0, terrain: 0, skirt: 0.0, skirt_mask: 0 };
        let eye = [0.0, h[(64 * c + 64) as usize] + 2.0, 0.0];
        let mut out = Vec::new();
        select(&s, eye, &mut out);
        let under = out.iter().find(|p| p.patch.i0 <= 64 && p.patch.j0 <= 64 && p.patch.i0 + GRID > 64 && p.patch.j0 + GRID > 64).unwrap();
        assert_eq!(under.patch.level, 0);
        // Its vertices are the samples, bit for bit.
        let v = patch_vertex(&h, c, r, [1.0, 1.0], &ID, &under.patch, (64 - under.patch.i0, 64 - under.patch.j0), eye);
        assert_eq!(v[1].to_bits(), h[(64 * c + 64) as usize].to_bits());
        assert_eq!([v[0], v[2]], [0.0, 0.0]);
    }

    #[test]
    fn selection_is_bounded_and_deterministic() {
        let (c, r) = (257u32, 257u32);
        let h = rough(c, r, 5.0);
        let tree = HeightTree::new(&h, c, r).unwrap();
        // A tiny detail range everywhere at full detail would need 256 leaves: still bounded.
        let rg = ranges(1e6, 1.0);
        let s = Selection { tree: &tree, cell: [1.0, 1.0], world: &ID, ranges: &rg, level_offset: 0, terrain: 3, skirt: 0.5, skirt_mask: 15 };
        let (mut a, mut b) = (Vec::new(), Vec::new());
        select(&s, [0.0, 10.0, 0.0], &mut a);
        select(&s, [0.0, 10.0, 0.0], &mut b);
        assert_eq!(a, b);
        assert_eq!(a.len(), 256);
        assert!(a.iter().all(|p| p.patch.terrain == 3 && p.patch.skirt == 0.5 && p.patch.level == 0));
        // Far away: one root patch.
        let mut far = Vec::new();
        let rg = ranges(10.0, 1.0);
        let s = Selection { ranges: &rg, ..s };
        select(&s, [1e5, 0.0, 0.0], &mut far);
        assert_eq!(far.len(), 1, "beyond every range: the root patch");
        assert_eq!(far[0].patch.level, tree.root_level());
        // Its vertices never collapse past the root: the patch keeps its corners.
        let corner = patch_vertex(&h, c, r, [1.0, 1.0], &ID, &far[0].patch, (GRID, GRID), [1e5, 0.0, 0.0]);
        assert_eq!([corner[0], corner[2]], [128.0, 128.0]);
        far.clear();
        select(&s, [300.0, 0.0, 0.0], &mut far);
        assert!(far.len() <= 16, "{}", far.len());
    }

    #[test]
    fn coarse_copies_select_on_the_full_ranges() {
        let (c, r) = (257u32, 257u32);
        let h = rough(c, r, 5.0);
        let (cc, cr, ch) = subsample(&h, c, r, 8).unwrap();
        let coarse = HeightTree::new(&ch, cc, cr).unwrap();
        let rg = ranges(0.0, 1.0);
        let s = Selection { tree: &coarse, cell: [8.0, 8.0], world: &ID, ranges: &rg, level_offset: 3, terrain: 0, skirt: 0.0, skirt_mask: 0 };
        let mut out = Vec::new();
        select(&s, [0.0, 400.0, 0.0], &mut out);
        assert!(!out.is_empty());
        // The morph ranges are the full terrain's levels 3 and up.
        for p in &out {
            assert_eq!((p.patch.r0, p.patch.level_offset), (rg[0], 3));
        }
        assert_eq!(morph_range(10.0, 0), [7.0, 10.0]);
        assert_eq!(morph_range(10.0, 2), [34.0, 40.0]);
    }
}

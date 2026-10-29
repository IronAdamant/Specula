//! Tier-4 terrain: heightfields from noise layers, shaping and erosion, all as data, with splat
//! masks, a render mesh and exports for physics and navigation.
//!
//! A [`TerrainSpec`] (from a recipe's `terrain` generator or the `procgen_terrain` tool) builds a
//! [`Terrain`]: `columns × rows` heights centred on the origin, the **same convention as the
//! physics heightfield** (`specula_physics::shape::Heightfield`): sample `(x, z)` sits at
//! `((x - (columns-1)/2) · cell.0, heights[z·columns + x], (z - (rows-1)/2) · cell.1)`, and each
//! cell is the two triangles `[v00, v01, v10]` and `[v10, v01, v11]`. [`Terrain::height_at`]
//! interpolates on exactly those triangles, so what stands on the terrain stands on its collider.
//!
//! - **Shape:** fBm, ridged and billow noise layers (feature size in metres), domain warp, an
//!   island falloff, terraces, and flat pads (`flatten`: building sites, a slab's footprint).
//! - **Erosion:** hydraulic droplets (deterministic: one seeded stream, droplets one after
//!   another) and thermal relaxation (double-buffered), each bounded in work.
//! - **Splat:** up to four layers by slope, height and noise, normalised per sample (RGBA8):
//!   what a terrain renderer blends materials by; the mesh's vertex colours mix the layers'
//!   colours for today's renderer.
//! - **Exports:** [`Terrain::physics_heightfield`] (resampled to a side limit, e.g. the
//!   engine's 129), [`Terrain::walkable_cells`] for navmesh building, [`Terrain::mesh`] (optionally
//!   a closed slab with sides and a bottom).
//! - **Tiles and edits** (the terrain renderer's, docs/design/TERRAIN.md): `offset` moves the
//!   noise domain (tiles of one terrain line up), `edge_lock` eases erosion out at the border (so
//!   neighbouring tiles share their edge heights), and `edits` are sculpt and paint strokes as data,
//!   applied in order after everything else, in domain coordinates (local + offset).
//! - **Render settings** ([`TerrainRender`]): how the terrain renderer draws it (layer materials,
//!   skirt, grain); `None` draws the mesh as before.

use specula_core::rng::Rng;

use crate::math3::{self, V3};
use crate::noise;
use crate::shape::{Rgba, Shape, TAG_FACE};

/// Most samples per side.
pub const MAX_SIDE: u32 = 257;
/// Most noise layers, splat layers and flat pads.
pub const MAX_LAYERS: usize = 8;
pub const MAX_SPLAT: usize = 4;
pub const MAX_PADS: usize = 32;
/// Most sculpt and paint strokes one terrain holds.
pub const MAX_EDITS: usize = 512;
/// Most samples the border easing (`edge_lock`) spans.
pub const MAX_EDGE_LOCK: u32 = 64;
/// Most hydraulic droplets and thermal iterations.
pub const MAX_DROPLETS: u32 = 200_000;
pub const MAX_THERMAL: u32 = 200;
/// Most work units one terrain may spend on its own ([`crate::work`]: sample visits weighed by
/// [`crate::work::TERRAIN_VISIT`], a noise octave being one visit); a recipe's terrain gets
/// what is left of its build.
pub const MAX_WORK: u64 = crate::work::MAX_BUILD;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseKind {
    Fbm,
    Ridged,
    Billow,
}

/// One noise layer, added to the height field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseLayer {
    pub kind: NoiseKind,
    /// Feature size (metres): the first octave's wavelength.
    pub scale: f32,
    pub octaves: u32,
    /// Height of this layer (metres, peak to centre).
    pub amplitude: f32,
}

/// A flat pad: the terrain eased to `height` within `radius`, blending out over `blend`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pad {
    pub at: [f32; 2],
    pub radius: f32,
    pub height: f32,
    pub blend: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hydraulic {
    pub droplets: u32,
    /// Strength 0..1 of how much each step erodes and deposits.
    pub strength: f32,
    /// Most steps of one droplet.
    pub steps: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thermal {
    pub iterations: u32,
    /// Slopes steeper than this slide (degrees).
    pub talus_deg: f32,
}

/// A splat layer: where it shows (slope and height ranges, soft edges) and its colour.
#[derive(Clone, Debug, PartialEq)]
pub struct SplatLayer {
    pub name: String,
    pub color: Rgba,
    /// Degrees (0 = flat).
    pub slope: [f32; 2],
    /// Metres (world y).
    pub height: [f32; 2],
    /// Softness of the range edges (degrees for slope, metres for height).
    pub soft: f32,
    /// Noise breaking up the layer: amount 0..1 and feature size (metres).
    pub noise: f32,
    pub noise_scale: f32,
}

/// What a stroke does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOp {
    /// Heights up by `amount` metres at the centre.
    Raise,
    /// Heights down by `amount` metres at the centre.
    Lower,
    /// Heights towards `height` by `amount` (0..1) at the centre.
    Flatten,
    /// Heights towards their neighbours' mean by `amount` (0..1) at the centre.
    Smooth,
    /// Splat layer `layer`'s weight up by `amount` (0..1) at the centre (the others make room).
    Paint,
}

impl EditOp {
    pub const ALL: [EditOp; 5] = [EditOp::Raise, EditOp::Lower, EditOp::Flatten, EditOp::Smooth, EditOp::Paint];

    pub fn name(self) -> &'static str {
        match self {
            EditOp::Raise => "raise",
            EditOp::Lower => "lower",
            EditOp::Flatten => "flatten",
            EditOp::Smooth => "smooth",
            EditOp::Paint => "paint",
        }
    }

    pub fn parse(s: &str) -> Option<EditOp> {
        EditOp::ALL.into_iter().find(|o| o.name() == s)
    }
}

/// A sculpt or paint stroke: a disc of `radius` metres at `at` (domain coordinates: the
/// terrain's local x, z plus its `offset`), full strength at the centre, easing out to its rim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edit {
    pub op: EditOp,
    pub at: [f32; 2],
    pub radius: f32,
    pub amount: f32,
    /// `flatten`'s target height (metres).
    pub height: f32,
    /// `paint`'s splat layer.
    pub layer: u32,
}

/// How the terrain renderer draws a splat layer.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderLayer {
    /// The model's texture key of its base colour (and normal) texture set (`terrain+<graph>`),
    /// baked with the build.
    pub texture: Option<String>,
    /// Metres per texture repeat.
    pub uv_scale: f32,
    pub roughness: f32,
    pub specular: f32,
    /// Slope (degrees) from which the texture is projected from the sides too (90 = never).
    pub triplanar_deg: f32,
    pub normal_strength: f32,
}

impl Default for RenderLayer {
    fn default() -> Self {
        RenderLayer { texture: None, uv_scale: 4.0, roughness: 0.9, specular: 0.05, triplanar_deg: 35.0, normal_strength: 1.0 }
    }
}

/// A terrain the terrain renderer draws (its recipe said `"render": "terrain"`).
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainRender {
    /// One per splat layer (at least one).
    pub layers: Vec<RenderLayer>,
    /// Skirt depth (metres) under the terrain's border, hiding seams between tiles (0 = none).
    pub skirt: f32,
    /// Procedural grain on untextured layers, 0..1.
    pub grain: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSpec {
    pub size: [f32; 2],
    pub samples: [u32; 2],
    pub base: f32,
    pub seed: u64,
    pub layers: Vec<NoiseLayer>,
    /// Domain warp (metres).
    pub warp: f32,
    /// Island falloff 0..1: heights eased down towards the edges.
    pub island: f32,
    /// Terrace step (metres; 0 = none) and sharpness 0..1.
    pub terrace: (f32, f32),
    pub pads: Vec<Pad>,
    pub hydraulic: Option<Hydraulic>,
    pub thermal: Option<Thermal>,
    pub splat: Vec<SplatLayer>,
    /// The noise domain's offset (metres): tiles of one terrain set it to their centre.
    pub offset: [f32; 2],
    /// Samples over which erosion is eased out to the noise-only heights at the border (0 = none).
    pub edge_lock: u32,
    /// Sculpt and paint strokes, applied in order after everything else.
    pub edits: Vec<Edit>,
}

/// Why a terrain failed.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainError {
    pub field: &'static str,
    pub message: String,
    pub limit: bool,
    pub measured: Option<f64>,
    pub max: Option<f64>,
}

impl TerrainError {
    fn bad(field: &'static str, m: impl Into<String>) -> TerrainError {
        TerrainError { field, message: m.into(), limit: false, measured: None, max: None }
    }
    fn limit(field: &'static str, m: impl Into<String>, measured: f64, max: f64) -> TerrainError {
        TerrainError { field, message: m.into(), limit: true, measured: Some(measured), max: Some(max) }
    }
}

/// A built terrain: heights, splat weights and what it took.
#[derive(Clone, Debug, PartialEq)]
pub struct Terrain {
    pub columns: usize,
    pub rows: usize,
    pub cell: [f32; 2],
    pub heights: Vec<f32>,
    /// Splat weights per sample (RGBA8 = layers 0..3, summing to about 255).
    pub splat: Vec<[u8; 4]>,
    pub splat_names: Vec<String>,
    pub splat_colors: Vec<Rgba>,
    pub work: u64,
    /// The noise domain's offset ([`TerrainSpec::offset`]).
    pub offset: [f32; 2],
    /// Set by the recipe when the terrain renderer draws it.
    pub render: Option<TerrainRender>,
}

impl TerrainSpec {
    /// Checks every number (the recipe layer checks types and paths first).
    pub fn check(&self) -> Result<(), TerrainError> {
        if !(self.size.iter().all(|s| *s > 0.0 && *s <= crate::recipe::MAX_EXTENT_M)) {
            return Err(TerrainError::bad("size", format!("sizes must be in (0, {}] m", crate::recipe::MAX_EXTENT_M)));
        }
        if !self.samples.iter().all(|n| (2..=MAX_SIDE).contains(n)) {
            return Err(TerrainError::limit("samples", format!("samples per side must be 2..{MAX_SIDE}"), self.samples[0].max(self.samples[1]) as f64, MAX_SIDE as f64));
        }
        if self.layers.len() > MAX_LAYERS || self.splat.len() > MAX_SPLAT || self.pads.len() > MAX_PADS {
            return Err(TerrainError::limit("layers", format!("at most {MAX_LAYERS} noise layers, {MAX_SPLAT} splat layers, {MAX_PADS} flat pads"), 0.0, 0.0));
        }
        for l in &self.layers {
            if !(l.scale > 0.0 && l.octaves >= 1 && l.octaves <= 10 && l.amplitude.abs() <= 10_000.0) {
                return Err(TerrainError::bad("noise", "noise layers need scale > 0, octaves 1..10 and |amplitude| <= 10000 m"));
            }
        }
        if let Some(h) = self.hydraulic {
            if h.droplets > MAX_DROPLETS || h.steps > 256 || !(0.0..=1.0).contains(&h.strength) {
                return Err(TerrainError::limit("erosion", format!("hydraulic erosion: droplets <= {MAX_DROPLETS}, steps <= 256, strength 0..1"), h.droplets as f64, MAX_DROPLETS as f64));
            }
        }
        if let Some(t) = self.thermal {
            if t.iterations > MAX_THERMAL || !(1.0..=89.0).contains(&t.talus_deg) {
                return Err(TerrainError::limit("thermal", format!("thermal erosion: iterations <= {MAX_THERMAL}, talus 1..89 degrees"), t.iterations as f64, MAX_THERMAL as f64));
            }
        }
        if !self.offset.iter().all(|o| o.is_finite() && o.abs() <= crate::recipe::MAX_EXTENT_M) {
            return Err(TerrainError::bad("offset", format!("offset must be within ±{} m", crate::recipe::MAX_EXTENT_M)));
        }
        if self.edge_lock > MAX_EDGE_LOCK {
            return Err(TerrainError::limit("edge_lock", format!("edge_lock is at most {MAX_EDGE_LOCK} samples"), self.edge_lock as f64, MAX_EDGE_LOCK as f64));
        }
        if self.edits.len() > MAX_EDITS {
            return Err(TerrainError::limit("edits", format!("at most {MAX_EDITS} sculpt and paint strokes"), self.edits.len() as f64, MAX_EDITS as f64));
        }
        let reach = self.size[0].max(self.size[1]) * 0.25;
        for ed in &self.edits {
            let ok = ed.at.iter().all(|v| v.is_finite() && v.abs() <= crate::recipe::MAX_EXTENT_M)
                && ed.radius.is_finite()
                && ed.radius > 0.0
                && ed.radius <= reach.max(0.5)
                && ed.amount.is_finite()
                && ed.amount.abs() <= 1e3
                && ed.height.is_finite()
                && ed.height.abs() <= crate::recipe::MAX_EXTENT_M
                && (ed.op != EditOp::Paint || (ed.layer as usize) < MAX_SPLAT);
            if !ok {
                return Err(TerrainError::bad(
                    "edits",
                    format!("a stroke needs a finite centre, a radius in (0, {}] m (a quarter of the terrain), |amount| <= 1000, and paint a layer 0..{}", reach.max(0.5), MAX_SPLAT - 1),
                ));
            }
        }
        Ok(())
    }

    /// Builds the terrain (deterministic in the spec).
    pub fn build(&self, max_work: u64) -> Result<Terrain, TerrainError> {
        self.check()?;
        let (cols, rows) = (self.samples[0] as usize, self.samples[1] as usize);
        let cell = [self.size[0] / (cols - 1) as f32, self.size[1] / (rows - 1) as f32];
        let n = cols * rows;
        let mut work = 0u64;
        let mut charge = |visits: u64| -> Result<(), TerrainError> {
            work += visits * crate::work::TERRAIN_VISIT;
            if work > max_work {
                return Err(TerrainError::limit("erosion", format!("the terrain needs more than {max_work} work units (sample visits, noise octaves, droplet steps); use fewer samples, octaves, droplets or iterations"), work as f64, max_work as f64));
            }
            if crate::work::cancelled() {
                return Err(TerrainError::limit("erosion", crate::work::CANCELLED, work as f64, max_work as f64));
            }
            Ok(())
        };
        let x0 = -((cols - 1) as f32) * cell[0] * 0.5;
        let z0 = -((rows - 1) as f32) * cell[1] * 0.5;
        let pos = |i: usize, j: usize| [x0 + i as f32 * cell[0], z0 + j as f32 * cell[1]];
        let domain = |i: usize, j: usize| domain_of(i, j, cols, rows, cell, self.offset);
        // Per sample: each layer's octaves (five visits each), the warp's two 2-octave lookups,
        // the rest.
        let per_sample = self.layers.iter().map(|l| 5 * l.octaves.max(1) as u64 + 1).sum::<u64>() + if self.warp > 0.0 { 20 } else { 0 } + 2;
        charge(n as u64 * per_sample)?;
        let mut h = vec![self.base; n];
        let warp_seed = self.seed ^ 0x5741_5250;
        for j in 0..rows {
            for i in 0..cols {
                let local = pos(i, j);
                let mut p = domain(i, j);
                if self.warp > 0.0 {
                    let s = 1.0 / (self.layers.first().map_or(50.0, |l| l.scale));
                    let wx = noise::fbm3(warp_seed, [p[0] * s, 0.0, p[1] * s], 2);
                    let wz = noise::fbm3(warp_seed ^ 1, [p[0] * s, 7.3, p[1] * s], 2);
                    p = [p[0] + wx * self.warp, p[1] + wz * self.warp];
                }
                let mut y = self.base;
                for (li, l) in self.layers.iter().enumerate() {
                    let s = 1.0 / l.scale;
                    let seed = self.seed.wrapping_add((li as u64 + 1).wrapping_mul(0x9e37_79b9));
                    let v = noise::fbm3(seed, [p[0] * s, 0.0, p[1] * s], l.octaves);
                    y += l.amplitude
                        * match l.kind {
                            NoiseKind::Fbm => v,
                            NoiseKind::Ridged => 1.0 - 2.0 * v.abs(),
                            NoiseKind::Billow => 2.0 * v.abs() - 1.0,
                        };
                }
                if self.island > 0.0 {
                    let u = [local[0] / (self.size[0] * 0.5), local[1] / (self.size[1] * 0.5)];
                    let r = (u[0] * u[0] + u[1] * u[1]).sqrt().min(1.0);
                    let fall = 1.0 - self.island * r * r * (3.0 - 2.0 * r);
                    y = self.base + (y - self.base) * fall;
                }
                h[j * cols + i] = y;
            }
        }
        // Erosion eased out at the border: tiles keep the noise's edge heights.
        let before = (self.edge_lock > 0 && (self.hydraulic.is_some() || self.thermal.is_some())).then(|| h.clone());
        if let Some(hy) = self.hydraulic {
            charge(hy.droplets as u64 * hy.steps as u64 * 15)?;
            hydraulic(&mut h, cols, rows, cell, &hy, self.seed);
        }
        if let Some(t) = self.thermal {
            charge(t.iterations as u64 * n as u64 * 5)?;
            thermal(&mut h, cols, rows, cell, &t);
        }
        if let Some(base) = before {
            charge(n as u64)?;
            let lock = self.edge_lock as f32;
            for j in 0..rows {
                for i in 0..cols {
                    let b = i.min(j).min(cols - 1 - i).min(rows - 1 - j) as f32;
                    let t = (b / lock).clamp(0.0, 1.0);
                    let w = t * t * (3.0 - 2.0 * t);
                    let k = j * cols + i;
                    h[k] = base[k] + (h[k] - base[k]) * w;
                }
            }
        }
        if self.terrace.0 > 0.0 {
            let (step, sharp) = (self.terrace.0, self.terrace.1.clamp(0.0, 1.0));
            for y in &mut h {
                let k = (*y - self.base) / step;
                let f = crate::dmath::floor(k);
                let t = k - f;
                // Twice-applied smoothstep, mixed in by `sharp`: flat treads, steep risers.
                let s1 = t * t * (3.0 - 2.0 * t);
                let s2 = s1 * s1 * (3.0 - 2.0 * s1);
                let tt = t + (s2 - t) * sharp;
                *y = self.base + (f + tt.clamp(0.0, 1.0)) * step;
            }
        }
        // Pads, and the splat weights (a noise lookup per layer).
        charge(n as u64 * (self.pads.len() as u64 + 3 * self.splat.len() as u64 + 1))?;
        for pad in &self.pads {
            for j in 0..rows {
                for i in 0..cols {
                    let p = pos(i, j);
                    let (ddx, ddz) = (p[0] - pad.at[0], p[1] - pad.at[1]);
                let d = (ddx * ddx + ddz * ddz).sqrt();
                    let w = if d <= pad.radius {
                        1.0
                    } else if pad.blend > 0.0 && d < pad.radius + pad.blend {
                        let t = 1.0 - (d - pad.radius) / pad.blend;
                        t * t * (3.0 - 2.0 * t)
                    } else {
                        0.0
                    };
                    let y = &mut h[j * cols + i];
                    *y += (pad.height - *y) * w;
                }
            }
        }
        let mut t = Terrain {
            columns: cols,
            rows,
            cell,
            heights: h,
            splat: Vec::new(),
            splat_names: Vec::new(),
            splat_colors: Vec::new(),
            work: 0,
            offset: self.offset,
            render: None,
        };
        // Strokes: the samples each covers, charged before it runs.
        for ed in &self.edits {
            let (i0, i1, j0, j1) = t.stroke_range(ed);
            charge(((i1 + 1 - i0.min(i1 + 1)) * (j1 + 1 - j0.min(j1 + 1))) as u64 * 4)?;
            if ed.op != EditOp::Paint {
                t.sculpt(ed);
            }
        }
        t.compute_splat(&self.splat, self.seed);
        for ed in self.edits.iter().filter(|e| e.op == EditOp::Paint) {
            t.paint(ed);
        }
        t.work = work;
        if !t.heights.iter().all(|y| y.is_finite() && y.abs() <= crate::recipe::MAX_EXTENT_M) {
            return Err(TerrainError::bad("noise", format!("heights left ±{} m", crate::recipe::MAX_EXTENT_M)));
        }
        Ok(t)
    }
}

/// Bilinear height and gradient on a grid (for erosion droplets), at cell coordinates.
fn sample_grad(h: &[f32], cols: usize, x: f32, z: f32) -> (f32, [f32; 2]) {
    let (i, j) = (x as usize, z as usize);
    let (u, v) = (x - i as f32, z - j as f32);
    let at = |a: usize, b: usize| h[b * cols + a];
    let (h00, h10, h01, h11) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
    let gx = (h10 - h00) * (1.0 - v) + (h11 - h01) * v;
    let gz = (h01 - h00) * (1.0 - u) + (h11 - h10) * u;
    let hh = h00 * (1.0 - u) * (1.0 - v) + h10 * u * (1.0 - v) + h01 * (1.0 - u) * v + h11 * u * v;
    (hh, [gx, gz])
}

/// Droplet erosion (after Hans Theobald Beyer's method): each droplet runs downhill carrying
/// sediment, eroding where it speeds up and depositing where it slows; one seeded stream, one
/// droplet after another, so the result is the same everywhere.
fn hydraulic(h: &mut [f32], cols: usize, rows: usize, cell: [f32; 2], p: &Hydraulic, seed: u64) {
    let mut rng = Rng::new(seed ^ 0x4859_4452, 11);
    let (inertia, capacity_k, deposit, erode, evaporate, gravity) = (0.05f32, 4.0f32, 0.3f32, 0.3f32 * p.strength, 0.02f32, 4.0f32);
    let min_slope = 0.01f32;
    // Heights in cell units for the dynamics, so the result doesn't depend on the cell size.
    let scale = 1.0 / cell[0].min(cell[1]);
    for _ in 0..p.droplets {
        let mut x = rng.f32() * (cols - 2) as f32;
        let mut z = rng.f32() * (rows - 2) as f32;
        let (mut dx, mut dz) = (0.0f32, 0.0f32);
        let (mut speed, mut water, mut sediment) = (1.0f32, 1.0f32, 0.0f32);
        for _ in 0..p.steps {
            let (i, j) = (x as usize, z as usize);
            let (u, v) = (x - i as f32, z - j as f32);
            let (height, g) = sample_grad(h, cols, x, z);
            dx = dx * inertia - g[0] * scale * (1.0 - inertia);
            dz = dz * inertia - g[1] * scale * (1.0 - inertia);
            let len = (dx * dx + dz * dz).sqrt();
            if len < 1e-9 {
                break;
            }
            dx /= len;
            dz /= len;
            let (nx, nz) = (x + dx, z + dz);
            if nx < 0.0 || nz < 0.0 || nx >= (cols - 1) as f32 || nz >= (rows - 1) as f32 {
                break;
            }
            let (new_h, _) = sample_grad(h, cols, nx, nz);
            let dh = (new_h - height) * scale;
            let cap = (-dh).max(min_slope) * speed * water * capacity_k;
            let w = [(1.0 - u) * (1.0 - v), u * (1.0 - v), (1.0 - u) * v, u * v];
            let idx = [j * cols + i, j * cols + i + 1, (j + 1) * cols + i, (j + 1) * cols + i + 1];
            if sediment > cap || dh > 0.0 {
                let amount = if dh > 0.0 { dh.min(sediment) } else { (sediment - cap) * deposit };
                sediment -= amount;
                for k in 0..4 {
                    h[idx[k]] += amount * w[k] / scale;
                }
            } else {
                let amount = ((cap - sediment) * erode).min(-dh);
                for k in 0..4 {
                    let take = amount * w[k] / scale;
                    h[idx[k]] -= take;
                }
                sediment += amount;
            }
            speed = (speed * speed + dh.abs() * gravity).sqrt().min(50.0);
            water *= 1.0 - evaporate;
            x = nx;
            z = nz;
        }
    }
}

/// Thermal relaxation: material on slopes steeper than the talus angle slides to the lowest
/// neighbour, double-buffered (the same at any thread count).
fn thermal(h: &mut [f32], cols: usize, rows: usize, cell: [f32; 2], t: &Thermal) {
    let talus = crate::dmath::tan(t.talus_deg * std::f32::consts::PI / 180.0);
    let mut delta = vec![0.0f32; h.len()];
    for _ in 0..t.iterations {
        delta.iter_mut().for_each(|d| *d = 0.0);
        for j in 0..rows {
            for i in 0..cols {
                let c = j * cols + i;
                let mut best: Option<(usize, f32)> = None;
                for (di, dj, dist) in [(1isize, 0isize, cell[0]), (-1, 0, cell[0]), (0, 1, cell[1]), (0, -1, cell[1])] {
                    let (a, b) = (i as isize + di, j as isize + dj);
                    if a < 0 || b < 0 || a >= cols as isize || b >= rows as isize {
                        continue;
                    }
                    let nb = b as usize * cols + a as usize;
                    let drop = h[c] - h[nb];
                    let excess = drop - talus * dist;
                    if excess > 0.0 && best.is_none_or_less(excess) {
                        best = Some((nb, excess));
                    }
                }
                if let Some((nb, excess)) = best {
                    let mv = excess * 0.25;
                    delta[c] -= mv;
                    delta[nb] += mv;
                }
            }
        }
        for (y, d) in h.iter_mut().zip(&delta) {
            *y += d;
        }
    }
}

trait BestExcess {
    fn is_none_or_less(&self, x: f32) -> bool;
}

impl BestExcess for Option<(usize, f32)> {
    fn is_none_or_less(&self, x: f32) -> bool {
        match self {
            None => true,
            Some((_, b)) => x > *b,
        }
    }
}

/// Sample `(i, j)`'s position in the noise domain: its local x, z plus `offset`, computed in f64
/// from the whole sample index so tiles that share a border sample get the same bits there.
/// With no offset it is exactly the local position (terrains built before offsets hash the same).
fn domain_of(i: usize, j: usize, cols: usize, rows: usize, cell: [f32; 2], offset: [f32; 2]) -> [f32; 2] {
    if offset == [0.0, 0.0] {
        let x0 = -((cols - 1) as f32) * cell[0] * 0.5;
        let z0 = -((rows - 1) as f32) * cell[1] * 0.5;
        return [x0 + i as f32 * cell[0], z0 + j as f32 * cell[1]];
    }
    let x = (i as f64 - (cols - 1) as f64 * 0.5) * cell[0] as f64 + offset[0] as f64;
    let z = (j as f64 - (rows - 1) as f64 * 0.5) * cell[1] as f64 + offset[1] as f64;
    [x as f32, z as f32]
}

/// A stroke's strength at `t` = distance / radius: 1 at the centre, easing to 0 at the rim.
fn falloff(t: f32) -> f32 {
    if t >= 1.0 {
        return 0.0;
    }
    let u = 1.0 - t * t;
    u * u
}

/// Soft membership of `v` in `[lo, hi]` with edges `soft` wide.
fn band(v: f32, r: [f32; 2], soft: f32) -> f32 {
    if soft <= 0.0 {
        return if v >= r[0] && v <= r[1] { 1.0 } else { 0.0 };
    }
    let a = ((v - (r[0] - soft)) / soft).clamp(0.0, 1.0);
    let b = (((r[1] + soft) - v) / soft).clamp(0.0, 1.0);
    a.min(b)
}

impl Terrain {
    fn x0(&self) -> f32 {
        -((self.columns - 1) as f32) * self.cell[0] * 0.5
    }
    fn z0(&self) -> f32 {
        -((self.rows - 1) as f32) * self.cell[1] * 0.5
    }

    /// Sample `(i, j)` in the noise domain (local x, z plus the terrain's offset).
    pub fn domain(&self, i: usize, j: usize) -> [f32; 2] {
        domain_of(i, j, self.columns, self.rows, self.cell, self.offset)
    }

    /// The samples a stroke may touch: `(i0, i1, j0, j1)`, inclusive (empty when `i0 > i1`).
    fn stroke_range(&self, ed: &Edit) -> (usize, usize, usize, usize) {
        let (cx, cz) = (ed.at[0] - self.offset[0] - self.x0(), ed.at[1] - self.offset[1] - self.z0());
        let span = |c: f32, cell: f32, n: usize| -> (usize, usize) {
            let lo = crate::dmath::floor((c - ed.radius) / cell);
            let hi = crate::dmath::ceil((c + ed.radius) / cell);
            if hi < 0.0 || lo > (n - 1) as f32 {
                return (1, 0);
            }
            (lo.max(0.0) as usize, (hi as usize).min(n - 1))
        };
        let (i0, i1) = span(cx, self.cell[0], self.columns);
        let (j0, j1) = span(cz, self.cell[1], self.rows);
        (i0, i1, j0, j1)
    }

    /// Applies a height stroke (raise, lower, flatten, smooth).
    fn sculpt(&mut self, ed: &Edit) {
        let (i0, i1, j0, j1) = self.stroke_range(ed);
        if i0 > i1 || j0 > j1 {
            return;
        }
        let c = self.columns;
        // Smoothing reads the heights as they were before the stroke.
        let snapshot: Vec<f32> = if ed.op == EditOp::Smooth { self.heights.clone() } else { Vec::new() };
        for j in j0..=j1 {
            for i in i0..=i1 {
                let d = self.domain(i, j);
                let (dx, dz) = (d[0] - ed.at[0], d[1] - ed.at[1]);
                let f = falloff((dx * dx + dz * dz).sqrt() / ed.radius);
                if f <= 0.0 {
                    continue;
                }
                let h = &mut self.heights[j * c + i];
                match ed.op {
                    EditOp::Raise => *h += ed.amount * f,
                    EditOp::Lower => *h -= ed.amount * f,
                    EditOp::Flatten => *h += (ed.height - *h) * (ed.amount.clamp(0.0, 1.0) * f),
                    EditOp::Smooth => {
                        let at = |a: usize, b: usize| snapshot[b * c + a];
                        let (il, ir) = (i.saturating_sub(1), (i + 1).min(c - 1));
                        let (jl, jr) = (j.saturating_sub(1), (j + 1).min(self.rows - 1));
                        let mean = (at(il, j) + at(ir, j) + at(i, jl) + at(i, jr)) * 0.25;
                        *h += (mean - *h) * (ed.amount.clamp(0.0, 1.0) * f);
                    }
                    EditOp::Paint => {}
                }
            }
        }
    }

    /// Applies a paint stroke to the splat weights (they keep summing to 255).
    fn paint(&mut self, ed: &Edit) {
        let (i0, i1, j0, j1) = self.stroke_range(ed);
        let layer = ed.layer as usize;
        if i0 > i1 || j0 > j1 || layer >= 4 {
            return;
        }
        for j in j0..=j1 {
            for i in i0..=i1 {
                let d = self.domain(i, j);
                let (dx, dz) = (d[0] - ed.at[0], d[1] - ed.at[1]);
                let f = falloff((dx * dx + dz * dz).sqrt() / ed.radius) * ed.amount.clamp(-1.0, 1.0);
                if f == 0.0 {
                    continue;
                }
                let w = &mut self.splat[j * self.columns + i];
                let mut v = w.map(|x| x as f32 / 255.0);
                v[layer] = (v[layer] + f).clamp(0.0, 1.0);
                let rest: f32 = (0..4).filter(|k| *k != layer).map(|k| v[k]).sum();
                let room = 1.0 - v[layer];
                for k in (0..4).filter(|k| *k != layer) {
                    v[k] = if rest > 1e-6 { v[k] / rest * room } else if room > 0.0 && k == if layer == 0 { 1 } else { 0 } { room } else { 0.0 };
                }
                let mut b = v.map(|x| (x * 255.0) as u8);
                let total: u32 = b.iter().map(|x| *x as u32).sum();
                let big = (0..4).max_by(|&x, &y| v[x].total_cmp(&v[y]).then(y.cmp(&x))).expect("four");
                b[big] = (b[big] as u32 + 255 - total.min(255)) as u8;
                *w = b;
            }
        }
    }

    /// The sample's position.
    pub fn sample_pos(&self, i: usize, j: usize) -> V3 {
        [self.x0() + i as f32 * self.cell[0], self.heights[j * self.columns + i], self.z0() + j as f32 * self.cell[1]]
    }

    /// Extent in x and z (metres).
    pub fn size(&self) -> [f32; 2] {
        [(self.columns - 1) as f32 * self.cell[0], (self.rows - 1) as f32 * self.cell[1]]
    }

    /// The cell and the barycentric place of (x, z) on its triangle (`None` off the terrain).
    fn locate(&self, x: f32, z: f32) -> Option<(usize, usize, f32, f32)> {
        let u = (x - self.x0()) / self.cell[0];
        let v = (z - self.z0()) / self.cell[1];
        if !(u >= 0.0 && v >= 0.0 && u <= (self.columns - 1) as f32 && v <= (self.rows - 1) as f32) {
            return None;
        }
        let i = (crate::dmath::floor(u) as usize).min(self.columns - 2);
        let j = (crate::dmath::floor(v) as usize).min(self.rows - 2);
        Some((i, j, u - i as f32, v - j as f32))
    }

    /// The surface height at (x, z), on the same triangles as the physics heightfield.
    pub fn height_at(&self, x: f32, z: f32) -> Option<f32> {
        let (i, j, fu, fv) = self.locate(x, z)?;
        let at = |a: usize, b: usize| self.heights[b * self.columns + a];
        let (h00, h10, h01, h11) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
        Some(if fu + fv <= 1.0 { h00 + fu * (h10 - h00) + fv * (h01 - h00) } else { h11 + (1.0 - fu) * (h01 - h11) + (1.0 - fv) * (h10 - h11) })
    }

    /// The surface normal at (x, z) (of the triangle there).
    pub fn normal_at(&self, x: f32, z: f32) -> Option<V3> {
        let (i, j, fu, fv) = self.locate(x, z)?;
        let p = |a: usize, b: usize| self.sample_pos(a, b);
        let n = if fu + fv <= 1.0 { math3::tri_normal(p(i, j), p(i, j + 1), p(i + 1, j)) } else { math3::tri_normal(p(i + 1, j), p(i, j + 1), p(i + 1, j + 1)) };
        Some(math3::normalize(n))
    }

    /// Slope at (x, z) in degrees (0 = flat).
    pub fn slope_deg_at(&self, x: f32, z: f32) -> Option<f32> {
        let n = self.normal_at(x, z)?;
        Some(crate::dmath::acos(n[1].clamp(-1.0, 1.0)) * (180.0 / std::f32::consts::PI))
    }

    /// The splat weights at (x, z) (nearest sample).
    pub fn splat_at(&self, x: f32, z: f32) -> Option<[u8; 4]> {
        let (i, j, fu, fv) = self.locate(x, z)?;
        let (a, b) = (if fu < 0.5 { i } else { i + 1 }, if fv < 0.5 { j } else { j + 1 });
        self.splat.get(b * self.columns + a).copied()
    }

    fn compute_splat(&mut self, layers: &[SplatLayer], seed: u64) {
        self.splat_names = layers.iter().map(|l| l.name.clone()).collect();
        self.splat_colors = layers.iter().map(|l| l.color).collect();
        if layers.is_empty() {
            self.splat = vec![[255, 0, 0, 0]; self.heights.len()];
            return;
        }
        let mut out = Vec::with_capacity(self.heights.len());
        for j in 0..self.rows {
            for i in 0..self.columns {
                let p = self.sample_pos(i, j);
                let dm = self.domain(i, j);
                let slope = self.sample_slope(i, j);
                let mut w = [0.0f32; 4];
                for (k, l) in layers.iter().enumerate() {
                    let mut v = band(slope, l.slope, l.soft) * band(p[1], l.height, l.soft);
                    if l.noise > 0.0 {
                        let s = 1.0 / l.noise_scale.max(1e-3);
                        let nz = noise::fbm3(seed ^ (0x5350_4c00 + k as u64), [dm[0] * s, 0.0, dm[1] * s], 3);
                        v *= (1.0 + l.noise * nz).clamp(0.0, 2.0);
                    }
                    w[k] = v;
                }
                let sum: f32 = w.iter().sum();
                // Nothing matched: the first layer.
                let w = if sum <= 1e-6 { [1.0, 0.0, 0.0, 0.0] } else { w.map(|x| x / sum) };
                // Round, then give the remainder to the largest so the bytes sum to 255.
                let mut b = w.map(|x| (x * 255.0) as u8);
                let total: u32 = b.iter().map(|x| *x as u32).sum();
                let big = (0..4).max_by(|&x, &y| w[x].total_cmp(&w[y]).then(y.cmp(&x))).expect("four");
                b[big] = (b[big] as u32 + 255 - total.min(255)) as u8;
                out.push(b);
            }
        }
        self.splat = out;
    }

    /// Slope at a sample (from central differences), degrees.
    fn sample_slope(&self, i: usize, j: usize) -> f32 {
        let at = |a: usize, b: usize| self.heights[b * self.columns + a];
        let (il, ir) = (i.saturating_sub(1), (i + 1).min(self.columns - 1));
        let (jl, jr) = (j.saturating_sub(1), (j + 1).min(self.rows - 1));
        let gx = (at(ir, j) - at(il, j)) / ((ir - il).max(1) as f32 * self.cell[0]);
        let gz = (at(i, jr) - at(i, jl)) / ((jr - jl).max(1) as f32 * self.cell[1]);
        crate::dmath::atan((gx * gx + gz * gz).sqrt()) * (180.0 / std::f32::consts::PI)
    }

    /// The heights for a physics heightfield collider of at most `max_side` samples per side
    /// (resampled on the surface when larger), centred like this terrain: `(columns, rows,
    /// heights, cell)`.
    pub fn physics_heightfield(&self, max_side: usize) -> (usize, usize, Vec<f32>, [f32; 2]) {
        let max_side = max_side.max(2);
        if self.columns <= max_side && self.rows <= max_side {
            return (self.columns, self.rows, self.heights.clone(), self.cell);
        }
        let size = self.size();
        let (c, r) = (self.columns.min(max_side), self.rows.min(max_side));
        let cell = [size[0] / (c - 1) as f32, size[1] / (r - 1) as f32];
        let mut h = Vec::with_capacity(c * r);
        for j in 0..r {
            for i in 0..c {
                let (x, z) = (-size[0] * 0.5 + i as f32 * cell[0], -size[1] * 0.5 + j as f32 * cell[1]);
                // Clamp onto the terrain (float rounding at the far edge).
                let (x, z) = (x.min(size[0] * 0.5), z.min(size[1] * 0.5));
                h.push(self.height_at(x, z).unwrap_or(0.0));
            }
        }
        (c, r, h, cell)
    }

    /// Per cell (`(columns-1) × (rows-1)`, row by row along +z): both triangles no steeper than
    /// `max_slope_deg` — what a navmesh builder may walk on.
    pub fn walkable_cells(&self, max_slope_deg: f32) -> Vec<bool> {
        let lim = crate::dmath::cos(max_slope_deg.clamp(0.0, 90.0) * std::f32::consts::PI / 180.0);
        let mut out = Vec::with_capacity((self.columns - 1) * (self.rows - 1));
        for j in 0..self.rows - 1 {
            for i in 0..self.columns - 1 {
                let p = |a: usize, b: usize| self.sample_pos(a, b);
                let n1 = math3::normalize(math3::tri_normal(p(i, j), p(i, j + 1), p(i + 1, j)));
                let n2 = math3::normalize(math3::tri_normal(p(i + 1, j), p(i, j + 1), p(i + 1, j + 1)));
                out.push(n1[1] >= lim && n2[1] >= lim);
            }
        }
        out
    }

    /// The render mesh: every `step`-th sample (1 = all), vertex colours from the splat layers.
    /// With `depth` > 0 it is a closed slab: sides down to `min height - depth` and a bottom.
    pub fn mesh(&self, step: usize, depth: f32) -> Shape {
        let step = step.max(1);
        let pick = |n: usize| -> Vec<usize> {
            let mut v: Vec<usize> = (0..n).step_by(step).collect();
            if *v.last().expect("n >= 2") != n - 1 {
                v.push(n - 1);
            }
            v
        };
        let (xs, zs) = (pick(self.columns), pick(self.rows));
        let mut s = Shape::default();
        let colour = |i: usize, j: usize| -> Rgba {
            let w = self.splat[j * self.columns + i];
            if self.splat_colors.is_empty() {
                return [1.0; 4];
            }
            let mut c = [0.0f32; 4];
            for (k, col) in self.splat_colors.iter().enumerate() {
                for q in 0..4 {
                    c[q] += col[q] * w[k] as f32 / 255.0;
                }
            }
            c
        };
        for &j in &zs {
            for &i in &xs {
                s.positions.push(self.sample_pos(i, j));
                s.colors.push(colour(i, j));
            }
        }
        let (nx, nz) = (xs.len(), zs.len());
        for b in 0..nz - 1 {
            for a in 0..nx - 1 {
                let v00 = (b * nx + a) as u32;
                let (v10, v01, v11) = (v00 + 1, v00 + nx as u32, v00 + nx as u32 + 1);
                s.tris.push([v00, v01, v10]);
                s.tris.push([v10, v01, v11]);
            }
        }
        if depth > 0.0 {
            let floor = self.heights.iter().fold(f32::MAX, |m, h| m.min(*h)) - depth;
            // The rim, counter-clockwise seen from above: along -z edge (+x), +x edge (+z),
            // +z edge (-x), -x edge (-z).
            let mut rim: Vec<u32> = Vec::new();
            for a in 0..nx - 1 {
                rim.push(a as u32);
            }
            for b in 0..nz - 1 {
                rim.push((b * nx + nx - 1) as u32);
            }
            for a in (1..nx).rev() {
                rim.push(((nz - 1) * nx + a) as u32);
            }
            for b in (1..nz).rev() {
                rim.push((b * nx) as u32);
            }
            let side = [0.42, 0.40, 0.38, 1.0];
            // A lip a few centimetres under the rim already takes the soil colour, so the
            // splat doesn't smear down the sides (colours belong to points, which weld).
            let lip_drop = (depth * 0.25).min(0.03);
            let lip = s.positions.len() as u32;
            for &r in &rim {
                let p = s.positions[r as usize];
                s.positions.push([p[0], p[1] - lip_drop, p[2]]);
                s.colors.push(side);
            }
            let base = s.positions.len() as u32;
            for &r in &rim {
                let p = s.positions[r as usize];
                s.positions.push([p[0], floor, p[2]]);
                s.colors.push(side);
            }
            let m = rim.len() as u32;
            for k in 0..m {
                let (a, b) = (rim[k as usize], rim[((k + 1) % m) as usize]);
                let (l, l2) = (lip + k, lip + (k + 1) % m);
                s.tris.push([a, b, l2]);
                s.tris.push([a, l2, l]);
                let (a, b) = (l, l2);
                let (a2, b2) = (base + k, base + (k + 1) % m);
                // The rim runs clockwise seen from above (+x along the -z edge first); the top
                // holds each rim edge reversed, so the sides hold it forwards.
                s.tris.push([a, b, b2]);
                s.tris.push([a, b2, a2]);
            }
            // The bottom: a fan from its centre (the rim's points along each side are
            // collinear, so a fan from a corner would hold slivers).
            let centre = s.positions.len() as u32;
            s.positions.push([self.x0() + self.size()[0] * 0.5, floor, self.z0() + self.size()[1] * 0.5]);
            s.colors.push(side);
            for k in 0..m {
                s.tris.push([centre, base + k, base + (k + 1) % m]);
            }
        }
        s.tags = vec![TAG_FACE; s.tris.len()];
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> TerrainSpec {
        TerrainSpec {
            size: [40.0, 30.0],
            samples: [41, 31],
            base: 0.0,
            seed: 7,
            layers: vec![NoiseLayer { kind: NoiseKind::Fbm, scale: 12.0, octaves: 4, amplitude: 3.0 }],
            warp: 2.0,
            island: 0.0,
            terrace: (0.0, 0.0),
            pads: vec![Pad { at: [5.0, 0.0], radius: 4.0, height: 1.0, blend: 2.0 }],
            hydraulic: Some(Hydraulic { droplets: 2000, strength: 0.5, steps: 32 }),
            thermal: Some(Thermal { iterations: 5, talus_deg: 35.0 }),
            offset: [0.0, 0.0],
            edge_lock: 0,
            edits: Vec::new(),
            splat: vec![
                SplatLayer { name: "grass".into(), color: [0.3, 0.5, 0.2, 1.0], slope: [0.0, 25.0], height: [-100.0, 100.0], soft: 5.0, noise: 0.0, noise_scale: 1.0 },
                SplatLayer { name: "rock".into(), color: [0.5, 0.5, 0.5, 1.0], slope: [25.0, 90.0], height: [-100.0, 100.0], soft: 5.0, noise: 0.3, noise_scale: 4.0 },
            ],
        }
    }

    #[test]
    fn builds_deterministically_with_pads_and_splat() {
        let t = spec().build(MAX_WORK).unwrap();
        assert_eq!(t, spec().build(MAX_WORK).unwrap());
        assert_eq!(t.heights.len(), 41 * 31);
        // The pad is flat at its height.
        assert!((t.height_at(5.0, 0.0).unwrap() - 1.0).abs() < 1e-4);
        assert!(t.slope_deg_at(5.0, 0.5).unwrap() < 1.0);
        // Splat bytes sum to 255.
        assert!(t.splat.iter().all(|w| w.iter().map(|x| *x as u32).sum::<u32>() == 255));
        // Off the terrain: nothing.
        assert!(t.height_at(30.0, 0.0).is_none());
    }

    #[test]
    fn height_at_matches_the_mesh_triangles() {
        let t = spec().build(MAX_WORK).unwrap();
        let m = t.mesh(1, 0.0);
        // Every triangle's centroid lies on height_at.
        for tri in m.tris.iter().step_by(7) {
            let p = tri.map(|i| m.positions[i as usize]);
            let c = [(p[0][0] + p[1][0] + p[2][0]) / 3.0, (p[0][1] + p[1][1] + p[2][1]) / 3.0, (p[0][2] + p[1][2] + p[2][2]) / 3.0];
            assert!((t.height_at(c[0], c[2]).unwrap() - c[1]).abs() < 1e-4);
        }
        // Faces up.
        assert!(m.tris.iter().all(|tri| math3::tri_normal(m.positions[tri[0] as usize], m.positions[tri[1] as usize], m.positions[tri[2] as usize])[1] > 0.0));
    }

    #[test]
    fn closed_slab_and_exports() {
        let t = spec().build(MAX_WORK).unwrap();
        let slab = t.mesh(2, 1.5);
        assert_eq!(slab.open_edge_count(), 0);
        assert!(slab.signed_volume() > 0.0);
        // The sides are soil from just under the rim down: no splat smeared down them.
        let top = t.heights.iter().fold(f32::MAX, |m, h| m.min(*h));
        let soil = [0.42, 0.40, 0.38, 1.0];
        for (p, c) in slab.positions.iter().zip(&slab.colors) {
            if p[1] < top - 0.05 {
                assert_eq!(*c, soil, "{p:?}");
            }
        }
        let (c, r, h, cell) = t.physics_heightfield(17);
        assert_eq!((c, r, h.len()), (17, 17, 17 * 17));
        assert!((cell[0] * 16.0 - 40.0).abs() < 1e-4);
        // Corners agree with the terrain.
        assert!((h[0] - t.heights[0]).abs() < 1e-4);
        let w = t.walkable_cells(30.0);
        assert_eq!(w.len(), 40 * 30);
        assert!(w.iter().any(|x| *x));
    }

    #[test]
    fn limits() {
        let mut s = spec();
        s.samples = [MAX_SIDE + 1, 3];
        assert!(s.build(MAX_WORK).unwrap_err().limit);
        let s = spec();
        assert!(s.build(1000).unwrap_err().limit);
        let mut s = spec();
        s.edits = vec![Edit { op: EditOp::Raise, at: [0.0, 0.0], radius: 1.0, amount: 1.0, height: 0.0, layer: 0 }; MAX_EDITS + 1];
        assert!(s.build(MAX_WORK).unwrap_err().limit);
        let mut s = spec();
        s.edits = vec![Edit { op: EditOp::Raise, at: [0.0, 0.0], radius: 100.0, amount: 1.0, height: 0.0, layer: 0 }];
        assert_eq!(s.build(MAX_WORK).unwrap_err().field, "edits", "a radius past a quarter of the terrain");
        let mut s = spec();
        s.edits = vec![Edit { op: EditOp::Paint, at: [0.0, 0.0], radius: 1.0, amount: 1.0, height: 0.0, layer: 4 }];
        assert_eq!(s.build(MAX_WORK).unwrap_err().field, "edits");
        let mut s = spec();
        s.edge_lock = MAX_EDGE_LOCK + 1;
        assert!(s.build(MAX_WORK).unwrap_err().limit);
    }

    /// Two tiles of one terrain (offsets one tile apart, erosion eased out at the border) share
    /// their border samples exactly (splat weights, which follow the slope, may differ along that row).
    #[test]
    fn tiles_share_their_border_heights() {
        let tile = |ox: f32| TerrainSpec { size: [32.0, 32.0], samples: [33, 33], offset: [ox, 0.0], edge_lock: 4, pads: vec![], ..spec() };
        let a = tile(100.0).build(MAX_WORK).unwrap();
        let b = tile(132.0).build(MAX_WORK).unwrap();
        for j in 0..33 {
            let (ea, eb) = (a.heights[j * 33 + 32], b.heights[j * 33]);
            assert_eq!(ea.to_bits(), eb.to_bits(), "row {j}: {ea} vs {eb}");
            assert_eq!(a.domain(32, j), b.domain(0, j));
        }
        // Without the lock, erosion breaks the border.
        let free = |ox: f32| TerrainSpec { edge_lock: 0, ..tile(ox) };
        let (a, b) = (free(100.0).build(MAX_WORK).unwrap(), free(132.0).build(MAX_WORK).unwrap());
        assert!((0..33).any(|j| a.heights[j * 33 + 32] != b.heights[j * 33]));
        // No offset: exactly the terrain as built before offsets existed.
        assert_eq!(spec().build(MAX_WORK).unwrap().domain(3, 4), [-20.0 + 3.0, -15.0 + 4.0]);
    }

    #[test]
    fn strokes_sculpt_and_paint_in_order() {
        let base = TerrainSpec { hydraulic: None, thermal: None, pads: vec![], ..spec() };
        let plain = base.build(MAX_WORK).unwrap();
        let at = |t: &Terrain| t.height_at(0.0, 0.0).unwrap();
        let raise = Edit { op: EditOp::Raise, at: [0.0, 0.0], radius: 4.0, amount: 2.0, height: 0.0, layer: 0 };
        let raised = TerrainSpec { edits: vec![raise], ..base.clone() }.build(MAX_WORK).unwrap();
        assert!((at(&raised) - at(&plain) - 2.0).abs() < 1e-4, "full strength at the centre");
        assert_eq!(raised.height_at(8.0, 0.0), plain.height_at(8.0, 0.0), "nothing outside the radius");
        let flat = Edit { op: EditOp::Flatten, height: 5.0, amount: 1.0, ..raise };
        let flattened = TerrainSpec { edits: vec![raise, flat], ..base.clone() }.build(MAX_WORK).unwrap();
        assert!((at(&flattened) - 5.0).abs() < 1e-4, "in order: raised, then flattened to 5 m");
        let smooth = Edit { op: EditOp::Smooth, amount: 1.0, ..raise };
        let smoothed = TerrainSpec { edits: vec![smooth], ..base.clone() }.build(MAX_WORK).unwrap();
        assert!(smoothed.heights.iter().all(|h| h.is_finite()));
        let paint = Edit { op: EditOp::Paint, layer: 1, amount: 1.0, ..raise };
        let painted = TerrainSpec { edits: vec![paint], ..base.clone() }.build(MAX_WORK).unwrap();
        let w = painted.splat_at(0.0, 0.0).unwrap();
        assert_eq!(w[1], 255, "painted fully at the centre: {w:?}");
        assert!(painted.splat.iter().all(|w| w.iter().map(|x| *x as u32).sum::<u32>() == 255));
        assert_eq!(painted.heights, plain.heights, "paint never moves the ground");
        // Deterministic.
        assert_eq!(TerrainSpec { edits: vec![raise, smooth, paint], ..base.clone() }.build(MAX_WORK).unwrap(), TerrainSpec { edits: vec![raise, smooth, paint], ..base }.build(MAX_WORK).unwrap());
    }
}

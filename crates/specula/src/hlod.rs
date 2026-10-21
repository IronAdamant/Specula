//! HLOD: one merged proxy per streaming cell, drawn instead of the cell's static models once the
//! camera is far enough away that every one of them would be at its coarsest LOD anyway
//! (`docs/design/STREAMING.md`, "HLOD").
//!
//! - **What goes in.** A cell's *members*: persistent, shown, non-`ui` model entities without an
//!   `anim` or a moving body, whose build is ready, opaque (`standard` presets only) and untextured.
//!   Everything else in the cell (glass, textured glTF, animated, kinematic or dynamic bodies) is
//!   drawn on its own, so like any model it is drawn only within `radius_m` of a camera.
//! - **What comes out.** [`generate`] merges each member's coarsest LOD (the level
//!   `select_lod` reaches last, at most 3), transformed into the cell's frame, its colour (part
//!   colour times entity tint) baked into the vertex colours, into one mesh per distinct
//!   material (preset parameters): usually one to three draws for the whole cell. It is a pure
//!   function of its input, sorted by entity id, using `dmath` for the colour conversions: the
//!   same members give the same bytes on every run and platform ([`ProxyData::hash`]).
//! - **When it's drawn.** A member at distance `d` is at its last LOD once its screen size falls
//!   below that level's threshold with the window's hysteresis (`LOD_PIXELS[L-1] * (1 -
//!   LOD_HYSTERESIS)`), i.e. beyond `d_i = r_i * viewport_h / (T_i * tan(fov/2))`. The cell's
//!   switch distance is the largest `d_i` (and at least `hlod_min_m`), measured to the box of its
//!   members' centres (never farther than any member). Beyond it the proxy shows exactly the
//!   geometry the members would have drawn, so swapping never pops; the window enters the proxy
//!   10% past the switch distance and leaves it inside the switch distance (hysteresis), and
//!   only once every member is resident again (the coarsest level is uploaded first, so leaving
//!   shows the same geometry). Captures decide afresh (no hysteresis): reproducible.
//! - **Staleness.** A cell's content key hashes its members' ids, content hashes and builds. A
//!   change makes the proxy stale at once (it is no longer drawn: the members are) and a new one
//!   is built once the cell has been quiet for `settle_s`, on the HLOD worker thread.
//!
//! Covered members are skipped by `GpuModels::draw_model` through a bitset by entity id
//! ([`covers`]); proxy draws are appended to the frame's draw list.

use crate::models::{part_look, Built, LOD_HYSTERESIS, LOD_PIXELS};
use specula_core::dmath;
use specula_core::hash::StableHasher;
use specula_core::math::{Mat4, Vec3};
use specula_core::Json;
use specula_gpu::{DrawItem, Gpu, Material, MeshId, MeshStreams, Surface, Vertex};
use specula_model::MaterialKind;
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

/// The window enters a cell's proxy this far (as a fraction) past its switch distance.
pub const HYSTERESIS: f32 = 0.10;
/// Entity ids above this are never merged (the cover bitset stays at most 8 MB).
pub const MAX_COVER_ID: u64 = 1 << 26;
/// Vertices one proxy holds at most; members past it stay on their own.
pub const MAX_PROXY_VERTICES: usize = 1 << 18;
/// Proxy builds queued or running at once.
pub const MAX_JOBS: usize = 16;
/// The coarsest level `select_lod` ever picks.
const LAST_REACHABLE: usize = 3;

/// A streaming cell: `(x, z)` in cells of `cell_m` metres.
pub type CellKey = (i32, i32);

/// Whether entity `id` is drawn by a proxy (its bit in `bits`).
pub fn covers(bits: &[u64], id: u64) -> bool {
    bits.get((id >> 6) as usize).is_some_and(|w| (w >> (id & 63)) & 1 == 1)
}

fn set_bit(bits: &mut Vec<u64>, id: u64, on: bool) {
    if id >= MAX_COVER_ID {
        return;
    }
    let w = (id >> 6) as usize;
    if bits.len() <= w {
        if !on {
            return;
        }
        bits.resize(w + 1, 0);
    }
    if on {
        bits[w] |= 1 << (id & 63);
    } else {
        bits[w] &= !(1 << (id & 63));
    }
}

/// The level of a model with `count` LODs that `select_lod` reaches last.
pub fn last_level(count: usize) -> usize {
    count.saturating_sub(1).min(LAST_REACHABLE)
}

/// `r / T` for a model of world radius `r` whose last level is `level`: its switch distance is
/// this times `viewport_h / tan(fov/2)`. 0 for single-LOD models (any distance).
pub fn switch_factor(radius: f32, level: usize) -> f32 {
    if level == 0 {
        return 0.0;
    }
    radius / (LOD_PIXELS[level - 1] * (1.0 - LOD_HYSTERESIS))
}

/// Distance from `eye` to the box `lo..hi` (0 inside).
pub fn box_distance(eye: Vec3, lo: Vec3, hi: Vec3) -> f32 {
    let d = Vec3::new((lo.x - eye.x).max(eye.x - hi.x).max(0.0), (lo.y - eye.y).max(eye.y - hi.y).max(0.0), (lo.z - eye.z).max(eye.z - hi.z).max(0.0));
    d.length()
}

/// The distance past which a cell with factor `k` may draw its proxy, for a view.
pub fn switch_distance(k: f32, fov_y_deg: f32, viewport_h: u32, min_m: f32) -> f32 {
    let tan = (fov_y_deg.to_radians() * 0.5).tan().max(1e-6);
    (k * viewport_h as f32 / tan).max(min_m)
}

// ---------------------------------------------------------------------------------------------
// Generation (pure)

/// One member as [`generate`] reads it.
#[derive(Clone)]
pub struct MemberInput {
    pub id: u64,
    pub world: Mat4,
    /// The entity's colour (sRGB).
    pub tint: [f32; 3],
    pub built: Arc<Built>,
    pub level: usize,
}

/// One merged mesh: every member part with this material.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub material: Material,
    pub surface: Surface,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// A generated proxy, before upload.
#[derive(Clone, Debug)]
pub struct ProxyData {
    /// The cell content key it was made from.
    pub key: u64,
    /// The proxy's frame: vertices are relative to it (precision far from the origin).
    pub origin: Vec3,
    pub groups: Vec<Group>,
    /// Hash of every byte (the same members give the same hash).
    pub hash: u64,
    pub members: usize,
    /// Members left out because the proxy was full ([`MAX_PROXY_VERTICES`]); they draw on
    /// their own.
    pub left_out: Vec<u64>,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        dmath::pow((c + 0.055) / 1.055, 2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * dmath::pow(c, 1.0 / 2.4) - 0.055
    }
}

/// A vertex colour (sRGB bytes) times a tint (sRGB), as the shader would multiply them (in
/// linear light, a 255 channel exactly 1), back to sRGB bytes.
fn bake(c: [u8; 4], tint: [f32; 3]) -> [u8; 4] {
    let mut out = [0u8; 4];
    for k in 0..3 {
        let v = if c[k] == 255 { 1.0 } else { srgb_to_linear(f32::from(c[k]) / 255.0) };
        let t = srgb_to_linear(if tint[k].is_nan() { 0.0 } else { tint[k].clamp(0.0, 1.0) });
        out[k] = dmath::round(linear_to_srgb(v * t).clamp(0.0, 1.0) * 255.0) as u8;
    }
    out[3] = c[3];
    out
}

/// The surface parameters that decide a group (proxies hold `standard` presets only).
fn surface_key(m: Material, s: &Surface) -> [u32; 5] {
    [m as u32, s.emissive.to_bits(), s.metallic.to_bits(), s.roughness.to_bits(), s.specular.to_bits()]
}

/// Whether part materials of `built`'s level `level` can all be merged: `standard` presets,
/// no textures.
pub fn mergeable(built: &Built, level: usize) -> bool {
    let Some(lod) = built.model.lods.get(level) else { return false };
    !lod.parts.is_empty()
        && lod.parts.iter().all(|p| {
            let standard = built.model.materials.iter().find(|(n, _)| *n == p.material).is_some_and(|(_, m)| m.kind == MaterialKind::Standard);
            standard && !built.materials.get(&p.material).is_some_and(|m| m.textured())
        })
}

/// Merges `members` (see the module docs) into a proxy in the frame of `origin`.
pub fn generate(key: u64, origin: Vec3, members: &[MemberInput]) -> ProxyData {
    let mut order: Vec<&MemberInput> = members.iter().collect();
    order.sort_by_key(|m| m.id);
    let mut groups: Vec<([u32; 5], Group)> = Vec::new();
    let mut total = 0usize;
    let mut left_out = Vec::new();
    let mut merged = 0;
    let mut remap: Vec<u32> = Vec::new();
    for m in order {
        let Some(lod) = m.built.model.lods.get(m.level) else { continue };
        if total + lod.positions.len() > MAX_PROXY_VERTICES {
            left_out.push(m.id);
            continue;
        }
        merged += 1;
        let w = &m.world;
        let c = &w.cols;
        // Normals by the inverse transpose (up to scale): the cofactor columns, as the shader.
        let (a, b, cc) = (Vec3::new(c[0][0], c[0][1], c[0][2]), Vec3::new(c[1][0], c[1][1], c[1][2]), Vec3::new(c[2][0], c[2][1], c[2][2]));
        let cof = [b.cross(cc), cc.cross(a), a.cross(b)];
        let flip = if a.dot(cof[0]) < 0.0 { -1.0 } else { 1.0 };
        for (pi, part) in lod.parts.iter().enumerate() {
            let Some((_, preset)) = m.built.model.materials.iter().find(|(n, _)| *n == part.material) else { continue };
            let (material, surface, color) = part_look(preset, [1.0; 3]);
            let tint = [color[0] * m.tint[0], color[1] * m.tint[1], color[2] * m.tint[2]];
            let k = surface_key(material, &surface);
            let gi = match groups.iter().position(|(gk, _)| *gk == k) {
                Some(i) => i,
                None => {
                    groups.push((k, Group { material, surface, vertices: Vec::new(), indices: Vec::new() }));
                    groups.len() - 1
                }
            };
            let g = &mut groups[gi].1;
            remap.clear();
            remap.resize(lod.positions.len(), u32::MAX);
            for &i in lod.part_indices(pi) {
                let i = i as usize;
                if remap[i] == u32::MAX {
                    remap[i] = g.vertices.len() as u32;
                    let p = w.transform_point(Vec3::new(lod.positions[i][0], lod.positions[i][1], lod.positions[i][2])) - origin;
                    let n = lod.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                    let wn = (cof[0] * n[0] + cof[1] * n[1] + cof[2] * n[2]) * flip;
                    let len = wn.length();
                    let wn = if len > 1e-20 && len.is_finite() { wn * (1.0 / len) } else { Vec3::new(0.0, 1.0, 0.0) };
                    let vc = lod.colors.get(i).copied().unwrap_or(Vertex::WHITE);
                    g.vertices.push(Vertex { position: [p.x, p.y, p.z], normal: [wn.x, wn.y, wn.z], color: bake(vc, tint) });
                    total += 1;
                }
                g.indices.push(remap[i]);
            }
        }
    }
    let groups: Vec<Group> = groups.into_iter().map(|(_, g)| g).filter(|g| !g.indices.is_empty()).collect();
    let mut h = StableHasher::with_seed(0x484c_4f44_0000_0001);
    h.write_u64(key);
    for g in &groups {
        h.write_u64(surface_key(g.material, &g.surface).iter().fold(0u64, |a, &x| a.rotate_left(13) ^ u64::from(x)));
        for v in &g.vertices {
            v.position.iter().chain(&v.normal).for_each(|x| h.write_u32(x.to_bits()));
            h.write_u32(u32::from_le_bytes(v.color));
        }
        g.indices.iter().for_each(|i| h.write_u32(*i));
    }
    ProxyData { key, origin, groups, hash: h.finish(), members: merged, left_out }
}

// ---------------------------------------------------------------------------------------------
// The worker

struct Job {
    cell: CellKey,
    key: u64,
    origin: Vec3,
    members: Vec<MemberInput>,
}

struct Worker {
    jobs: Sender<Job>,
    done: Receiver<(CellKey, Result<ProxyData, String>, f64)>,
    in_flight: usize,
}

impl Worker {
    fn start() -> Option<Worker> {
        let (jobs, rx) = mpsc::channel::<Job>();
        let (tx, done) = mpsc::channel();
        std::thread::Builder::new()
            .name("specula-hlod".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    let started = Instant::now();
                    let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| generate(job.key, job.origin, &job.members)))
                        .map_err(|_| "the proxy build panicked".to_owned());
                    if tx.send((job.cell, made, started.elapsed().as_secs_f64() * 1e3)).is_err() {
                        return;
                    }
                }
            })
            .ok()?;
        Some(Worker { jobs, done, in_flight: 0 })
    }
}

// ---------------------------------------------------------------------------------------------
// Per-cell state

/// What a cell's proxy is made from now (computed by `streaming` from the cell's members).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Content {
    /// Hash of the members' ids, content hashes and builds (0: nothing to merge).
    pub key: u64,
    /// Member ids, sorted.
    pub members: Vec<u64>,
    /// Box of the members' centres.
    pub lo: Vec3,
    pub hi: Vec3,
    /// Largest [`switch_factor`] of a member.
    pub k: f32,
    /// Every member's build is ready (otherwise no proxy is made yet).
    pub complete: bool,
    /// The model handles still building when it was worked out.
    pub waiting_on: Vec<u32>,
    /// When a member last changed.
    pub changed: Option<Instant>,
}

struct Uploaded {
    key: u64,
    origin: Vec3,
    draws: Vec<(MeshId, Material, Surface, u32)>,
    bytes: u64,
    hash: u64,
    left_out: Vec<u64>,
    members: Vec<u64>,
}

#[derive(Default)]
struct CellProxy {
    content: Content,
    uploaded: Option<Uploaded>,
    /// Built, waiting for upload.
    ready: Option<ProxyData>,
    /// The groups of `ready` uploaded so far, and the key they belong to.
    staged: (u64, Vec<(MeshId, Material, Surface, u32)>),
    /// The key of the build running for it.
    building: Option<u64>,
    /// The window draws its proxy now (its members' bits are set in the window's cover).
    window_on: bool,
    /// Wanted this update (inside `hlod_radius_m` of a stream source), with its priority.
    want: Option<f32>,
    /// The streaming frame it was last wanted in (eviction order).
    last_wanted: u64,
}

impl CellProxy {
    /// The uploaded proxy, when it is current.
    fn current(&self) -> Option<&Uploaded> {
        self.uploaded.as_ref().filter(|u| u.key == self.content.key && self.content.key != 0)
    }
}

/// Counters for perf_frame and stream_state.
#[derive(Clone, Copy, Debug, Default)]
pub struct HlodStats {
    pub built: u64,
    pub build_ms_total: f64,
    pub build_ms_max: f64,
    pub failed: u64,
    pub uploaded: u64,
    pub evicted: u64,
    pub window_cells: u32,
    pub window_covered: u32,
    pub window_draws: u32,
}

/// Every cell's proxy, the worker, and which entities the window and the last capture cover.
#[derive(Default)]
pub struct Hlod {
    cells: BTreeMap<CellKey, CellProxy>,
    worker: Option<Worker>,
    worker_failed: bool,
    /// The window's covered entities (kept up to date as cells switch).
    pub window_cover: Vec<u64>,
    /// The last capture's.
    pub capture_cover: Vec<u64>,
    pub stats: HlodStats,
    /// The last proxy build's error.
    pub last_error: Option<String>,
    /// Cells whose proxy left members out (streaming wants those on their own).
    left_out_cells: Vec<CellKey>,
    /// An upload retired GPU memory since [`Hlod::take_retired`] (see there).
    retired_any: bool,
}

impl Hlod {
    /// Sets what cell `cell` is made from now. A changed key makes its proxy stale (no longer
    /// drawn) until the new one is uploaded.
    pub fn set_content(&mut self, cell: CellKey, content: Content) {
        let c = self.cells.entry(cell).or_default();
        if c.content.key != content.key && c.window_on {
            // Stale: the members draw again.
            for &id in &c.content.members {
                set_bit(&mut self.window_cover, id, false);
            }
            c.window_on = false;
        }
        c.content = content;
    }

    /// Forgets a cell with no members any more (its proxy is retired).
    pub fn remove(&mut self, cell: CellKey, gpu: &mut Gpu) {
        if let Some(c) = self.cells.remove(&cell) {
            if c.window_on {
                for &id in &c.content.members {
                    set_bit(&mut self.window_cover, id, false);
                }
            }
            retire_cell(gpu, c);
        }
    }

    /// Forgets the cells `keep` says no (they have no members any more: a replaced world).
    pub fn retain(&mut self, mut keep: impl FnMut(CellKey) -> bool, gpu: &mut Gpu) {
        let gone: Vec<CellKey> = self.cells.keys().copied().filter(|k| !keep(*k)).collect();
        for k in gone {
            self.remove(k, gpu);
        }
    }

    /// Drops every proxy (streaming or HLOD switched off, or the world was replaced).
    pub fn clear(&mut self, gpu: &mut Gpu) {
        for (_, c) in std::mem::take(&mut self.cells) {
            retire_cell(gpu, c);
        }
        self.window_cover.clear();
        self.capture_cover.clear();
    }

    pub fn content(&self, cell: CellKey) -> Option<&Content> {
        self.cells.get(&cell).map(|c| &c.content)
    }

    /// Marks which cells are wanted this update (`None`: not), for scheduling and eviction.
    pub fn set_wanted(&mut self, cell: CellKey, want: Option<f32>, frame: u64) {
        if let Some(c) = self.cells.get_mut(&cell) {
            c.want = want;
            if want.is_some() {
                c.last_wanted = frame;
            }
        }
    }

    /// Whether some camera wants cell `cell`'s proxy (as of the last pass).
    pub fn wanted(&self, cell: CellKey) -> bool {
        self.cells.get(&cell).is_some_and(|c| c.want.is_some())
    }

    /// Uploaded proxies of wanted cells the window doesn't draw now: `(priority, cell)` (they
    /// may give way to nearer content under the memory budget).
    pub fn wanted_uploaded(&self) -> Vec<(f32, CellKey)> {
        self.cells.iter().filter(|(_, c)| c.uploaded.is_some() && !c.window_on).filter_map(|(k, c)| Some((c.want?, *k))).collect()
    }

    /// The members cell `cell`'s proxy holds: the current uploaded proxy's (a full proxy leaves
    /// some out), else what the next one will hold.
    pub fn proxy_members(&self, cell: CellKey) -> &[u64] {
        match self.cells.get(&cell) {
            Some(c) => match c.current() {
                Some(u) => &u.members,
                None => &c.content.members,
            },
            None => &[],
        }
    }

    /// Cells whose uploaded proxy left members out since the last call.
    pub fn take_left_out(&mut self) -> Vec<CellKey> {
        let mut v = std::mem::take(&mut self.left_out_cells);
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Whether the window draws cell `cell` as its proxy now.
    pub fn window_on(&self, cell: CellKey) -> bool {
        self.cells.get(&cell).is_some_and(|c| c.window_on)
    }

    /// Drops built-but-not-uploaded proxies (and part uploads) of cells no camera wants, or
    /// whose content changed since: they would only hold memory.
    pub fn drop_unwanted(&mut self, gpu: &mut Gpu) {
        for c in self.cells.values_mut() {
            let stale = c.ready.as_ref().is_some_and(|r| r.key != c.content.key);
            if c.want.is_none() || stale {
                c.ready = None;
                for (m, ..) in c.staged.1.drain(..) {
                    gpu.retire_mesh(m);
                }
                c.staged.0 = 0;
            }
        }
    }

    /// Whether cell `cell` has a current, uploaded proxy.
    pub fn has_current(&self, cell: CellKey) -> bool {
        self.cells.get(&cell).is_some_and(|c| c.current().is_some())
    }

    /// Starts proxy builds for wanted cells whose proxy is missing or stale, settled for
    /// `settle_s`, nearest first; `inputs` gives a cell's member inputs (`None`: not ready).
    pub fn schedule(&mut self, now: Instant, settle_s: f32, mut inputs: impl FnMut(CellKey, &Content) -> Option<(Vec3, Vec<MemberInput>)>) {
        self.collect();
        if self.worker.is_none() && !self.worker_failed {
            self.worker = Worker::start();
            self.worker_failed = self.worker.is_none();
        }
        let Some(w) = &mut self.worker else { return };
        let mut todo: Vec<(f32, CellKey)> = self
            .cells
            .iter()
            .filter(|(_, c)| {
                let ct = &c.content;
                c.want.is_some()
                    && ct.key != 0
                    && ct.complete
                    && c.building.is_none()
                    && c.ready.as_ref().is_none_or(|r| r.key != ct.key)
                    && c.uploaded.as_ref().is_none_or(|u| u.key != ct.key)
                    && ct.changed.is_none_or(|t| now.duration_since(t).as_secs_f32() >= settle_s)
            })
            .map(|(k, c)| (c.want.unwrap_or(f32::MAX), *k))
            .collect();
        todo.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (_, cell) in todo {
            if w.in_flight >= MAX_JOBS {
                break;
            }
            let Some(c) = self.cells.get_mut(&cell) else { continue };
            let Some((origin, members)) = inputs(cell, &c.content) else { continue };
            let key = c.content.key;
            if w.jobs.send(Job { cell, key, origin, members }).is_err() {
                self.worker = None;
                self.worker_failed = true;
                return;
            }
            c.building = Some(key);
            w.in_flight += 1;
        }
    }

    /// Takes finished builds (kept when still current). Never waits.
    pub fn collect(&mut self) {
        let Some(w) = &mut self.worker else { return };
        while let Ok((cell, made, ms)) = w.done.try_recv() {
            w.in_flight = w.in_flight.saturating_sub(1);
            let Some(c) = self.cells.get_mut(&cell) else { continue };
            c.building = None;
            match made {
                Ok(p) if p.key == c.content.key => {
                    self.stats.built += 1;
                    self.stats.build_ms_total += ms;
                    self.stats.build_ms_max = self.stats.build_ms_max.max(ms);
                    c.ready = Some(p);
                }
                Ok(_) => {}
                Err(e) => {
                    self.stats.failed += 1;
                    self.last_error = Some(e);
                }
            }
        }
    }

    /// Built proxies waiting for upload: `(priority, cell, bytes of the next group)`.
    pub fn uploads(&self) -> impl Iterator<Item = (f32, CellKey, u64)> + '_ {
        self.cells.iter().filter_map(|(k, c)| {
            let r = c.ready.as_ref().filter(|r| r.key == c.content.key)?;
            let want = c.want?;
            let next = if c.staged.0 == r.key { c.staged.1.len() } else { 0 };
            let g = r.groups.get(next)?;
            Some((want, *k, group_bytes(g)))
        })
    }

    /// Uploads the next group of cell `cell`'s built proxy (queued: never waits; one group per
    /// call, so one item stays small); with the last group the proxy replaces the old one, which
    /// is retired. Returns the bytes uploaded.
    pub fn upload(&mut self, cell: CellKey, gpu: &mut Gpu) -> Result<u64, String> {
        let Some(c) = self.cells.get_mut(&cell) else { return Ok(0) };
        if c.ready.as_ref().is_some_and(|r| r.key != c.content.key) {
            // The cell changed while it waited: a newer build comes.
            c.ready = None;
        }
        let Some(r) = &c.ready else { return Ok(0) };
        if c.staged.0 != r.key {
            // Groups of an older build: gone.
            self.retired_any |= !c.staged.1.is_empty();
            for (m, ..) in c.staged.1.drain(..) {
                gpu.retire_mesh(m);
            }
            c.staged.0 = r.key;
        }
        let Some(g) = r.groups.get(c.staged.1.len()) else { return Ok(0) };
        match gpu.queue_mesh(&g.vertices, &g.indices, &MeshStreams::default()) {
            Ok(mesh) => c.staged.1.push((mesh, g.material, g.surface, (g.indices.len() / 3) as u32)),
            Err(e) => {
                self.retired_any |= !c.staged.1.is_empty();
                for (m, ..) in c.staged.1.drain(..) {
                    gpu.retire_mesh(m);
                }
                c.ready = None;
                self.stats.failed += 1;
                let msg = format!("HLOD proxy of cell {cell:?} not uploaded: {e}");
                self.last_error = Some(msg.clone());
                return Err(msg);
            }
        }
        let uploaded_now = group_bytes(g);
        if c.staged.1.len() < r.groups.len() {
            return Ok(uploaded_now);
        }
        let p = c.ready.take().expect("checked");
        let draws = std::mem::take(&mut c.staged.1);
        let bytes = p.groups.iter().map(group_bytes).sum();
        let members: Vec<u64> = c.content.members.iter().copied().filter(|id| !p.left_out.contains(id)).collect();
        if !p.left_out.is_empty() {
            self.left_out_cells.push(cell);
        }
        let new = Uploaded { key: p.key, origin: p.origin, draws, bytes, hash: p.hash, left_out: p.left_out, members };
        if let Some(old) = c.uploaded.replace(new) {
            if c.window_on {
                for &id in &old.members {
                    set_bit(&mut self.window_cover, id, false);
                }
                c.window_on = false;
            }
            retire(gpu, &old);
            self.retired_any = true;
        }
        self.stats.uploaded += 1;
        Ok(uploaded_now)
    }

    /// Whether an upload retired GPU memory (an older proxy or older groups) since the last call:
    /// the streamer's running total of resident bytes is summed again.
    pub fn take_retired(&mut self) -> bool {
        std::mem::take(&mut self.retired_any)
    }

    /// GPU bytes of the uploaded proxies.
    pub fn resident_bytes(&self) -> u64 {
        let staged = |c: &CellProxy| match &c.ready {
            Some(r) if c.staged.0 == r.key => r.groups.iter().take(c.staged.1.len()).map(group_bytes).sum(),
            _ => 0,
        };
        self.cells.values().map(|c| c.uploaded.as_ref().map_or(0, |u| u.bytes) + staged(c)).sum()
    }

    /// Uploaded proxies of cells not wanted now, least recently wanted first: `(frame, cell)`.
    pub fn evictable(&self) -> Vec<(u64, CellKey)> {
        let mut v: Vec<(u64, CellKey)> =
            self.cells.iter().filter(|(_, c)| c.want.is_none() && c.uploaded.is_some() && !c.window_on).map(|(k, c)| (c.last_wanted, *k)).collect();
        v.sort();
        v
    }

    /// Retires cell `cell`'s uploaded proxy. Returns the bytes freed.
    pub fn evict(&mut self, cell: CellKey, gpu: &mut Gpu) -> u64 {
        let Some(c) = self.cells.get_mut(&cell) else { return 0 };
        let Some(u) = c.uploaded.take() else { return 0 };
        if c.window_on {
            for &id in &u.members {
                set_bit(&mut self.window_cover, id, false);
            }
            c.window_on = false;
        }
        retire(gpu, &u);
        self.stats.evicted += 1;
        u.bytes
    }

    /// The window's frame: switches cells in and out of their proxy (with hysteresis; leaving
    /// waits for `resident(member)` of every member) and appends the proxies' draws to `out`.
    pub fn window(&mut self, eye: Vec3, fov_y_deg: f32, viewport_h: u32, min_m: f32, resident: impl Fn(u64) -> bool, out: &mut Vec<DrawItem>) {
        let (mut cells, mut covered, mut draws) = (0, 0, 0);
        for c in self.cells.values_mut() {
            let on = match c.uploaded.as_ref().filter(|u| u.key == c.content.key && c.content.key != 0) {
                None => false,
                Some(u) => {
                    let d = box_distance(eye, c.content.lo, c.content.hi);
                    let sw = switch_distance(c.content.k, fov_y_deg, viewport_h, min_m);
                    if c.window_on {
                        d >= sw || !u.members.iter().all(|&id| resident(id))
                    } else {
                        d > sw * (1.0 + HYSTERESIS)
                    }
                }
            };
            if on != c.window_on {
                let members = c.uploaded.as_ref().map(|u| u.members.as_slice()).unwrap_or(&[]);
                for &id in members {
                    set_bit(&mut self.window_cover, id, on);
                }
                c.window_on = on;
            }
            if on {
                let u = c.uploaded.as_ref().expect("on implies uploaded");
                cells += 1;
                covered += u.members.len() as u32;
                draws += push_draws(u, out);
            }
        }
        self.stats.window_cells = cells;
        self.stats.window_covered = covered;
        self.stats.window_draws = draws;
    }

    /// A capture's frame (no hysteresis, no memory): fills [`Hlod::capture_cover`] and appends
    /// the proxies' draws. Returns `(cells, covered entities)`.
    pub fn capture(&mut self, eye: Vec3, fov_y_deg: f32, viewport_h: u32, min_m: f32, stale: impl Fn(CellKey) -> bool, out: &mut Vec<DrawItem>) -> (u32, u32) {
        self.capture_cover.clear();
        let (mut cells, mut covered) = (0, 0);
        for (&k, c) in &self.cells {
            let Some(u) = c.current() else { continue };
            if stale(k) {
                continue;
            }
            let d = box_distance(eye, c.content.lo, c.content.hi);
            if d > switch_distance(c.content.k, fov_y_deg, viewport_h, min_m) {
                for &id in &u.members {
                    set_bit(&mut self.capture_cover, id, true);
                }
                cells += 1;
                covered += u.members.len() as u32;
                push_draws(u, out);
            }
        }
        (cells, covered)
    }

    /// Whether a capture through this view would draw cell `cell` as its proxy.
    pub fn capture_on(&self, cell: CellKey, eye: Vec3, fov_y_deg: f32, viewport_h: u32, min_m: f32) -> bool {
        self.cells.get(&cell).and_then(|c| c.current().map(|_| c)).is_some_and(|c| {
            box_distance(eye, c.content.lo, c.content.hi) > switch_distance(c.content.k, fov_y_deg, viewport_h, min_m)
        })
    }

    /// Cells: `(with members, current proxies, stale or missing, building, ready to upload)`.
    pub fn counts(&self) -> (usize, usize, usize, usize, usize) {
        let with = self.cells.values().filter(|c| c.content.key != 0).count();
        let current = self.cells.values().filter(|c| c.current().is_some()).count();
        let building = self.cells.values().filter(|c| c.building.is_some()).count();
        let ready = self.cells.values().filter(|c| c.ready.is_some()).count();
        (with, current, with - current, building, ready)
    }

    /// One cell's proxy as data (stream_state).
    pub fn cell_json(&self, cell: CellKey, eye: Option<(Vec3, f32, u32)>, min_m: f32) -> Json {
        let Some(c) = self.cells.get(&cell) else { return Json::Null };
        let state = match (c.current(), &c.uploaded, c.building, &c.ready) {
            (Some(_), ..) => "current",
            (None, _, Some(_), _) => "building",
            (None, _, _, Some(_)) => "uploading",
            (None, Some(_), ..) => "stale",
            _ if c.content.key == 0 => "none",
            _ if !c.content.complete => "waiting_for_builds",
            _ => "waiting",
        };
        let mut j = Json::obj([
            ("state", Json::from(state)),
            ("members", Json::from(c.content.members.len())),
            ("window_on", Json::from(c.window_on)),
        ]);
        if let Some((e, fov, vh)) = eye {
            j.set("switch_m", Json::from((switch_distance(c.content.k, fov, vh, min_m) * 10.0).round() / 10.0));
            j.set("distance_m", Json::from((box_distance(e, c.content.lo, c.content.hi) * 10.0).round() / 10.0));
        }
        if let Some(u) = &c.uploaded {
            j.set("draws", Json::from(u.draws.len()));
            j.set("triangles", Json::from(u.draws.iter().map(|d| u64::from(d.3)).sum::<u64>()));
            j.set("kb", Json::from(u.bytes / 1024));
            j.set("hash", Json::from(format!("{:016x}", u.hash)));
            if !u.left_out.is_empty() {
                j.set("left_out", Json::from(u.left_out.len()));
            }
        }
        j
    }

    pub fn json(&self) -> Json {
        let (with, current, stale, building, ready) = self.counts();
        let s = &self.stats;
        Json::obj([
            ("cells", Json::from(with)),
            ("current", Json::from(current)),
            ("stale_or_missing", Json::from(stale)),
            ("building", Json::from(building)),
            ("ready_to_upload", Json::from(ready)),
            ("window_cells", Json::from(s.window_cells)),
            ("window_covered_entities", Json::from(s.window_covered)),
            ("window_proxy_draws", Json::from(s.window_draws)),
            ("built", Json::from(s.built)),
            ("build_ms_mean", Json::from(if s.built > 0 { (s.build_ms_total / s.built as f64 * 100.0).round() / 100.0 } else { 0.0 })),
            ("build_ms_max", Json::from((s.build_ms_max * 100.0).round() / 100.0)),
            ("uploaded", Json::from(s.uploaded)),
            ("evicted", Json::from(s.evicted)),
            ("failed", Json::from(s.failed)),
            ("resident_mb", Json::from((self.resident_bytes() as f64 / 1_048_576.0 * 100.0).round() / 100.0)),
            ("last_error", self.last_error.as_deref().map_or(Json::Null, Json::from)),
        ])
    }
}

fn push_draws(u: &Uploaded, out: &mut Vec<DrawItem>) -> u32 {
    let o = u.origin;
    let model = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, o.x, o.y, o.z, 1.0];
    for &(mesh, material, surface, _) in &u.draws {
        out.push(DrawItem { mesh, model, color: [1.0; 4], material, surface, skin: None, bounds: None });
    }
    u.draws.len() as u32
}

fn retire(gpu: &mut Gpu, u: &Uploaded) {
    for (m, ..) in &u.draws {
        gpu.retire_mesh(*m);
    }
}

/// Retires everything a cell holds on the GPU: its proxy and a part-uploaded one.
fn retire_cell(gpu: &mut Gpu, c: CellProxy) {
    if let Some(u) = c.uploaded {
        retire(gpu, &u);
    }
    for (m, ..) in c.staged.1 {
        gpu.retire_mesh(m);
    }
}

fn group_bytes(g: &Group) -> u64 {
    g.vertices.len() as u64 * 28 + g.indices.len() as u64 * 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Library, SharedLibrary};
    use crate::world::ModelSpec;
    use std::path::PathBuf;

    fn sepulcra() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets")
    }

    /// Builds `recipe` with `seed` through the engine's library (waits for the worker).
    fn build(lib: &SharedLibrary, recipe: &str, seed: u64) -> Arc<Built> {
        let spec = ModelSpec { recipe: recipe.into(), params: Default::default(), seed: Some(seed) };
        let r = lib.lock().resolve(&spec).unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(60);
        loop {
            lib.lock().finish(Some(std::time::Duration::from_millis(20)));
            if let Some(b) = lib.lock().built(r.handle) {
                return Arc::clone(b);
            }
            assert!(Instant::now() < deadline, "build of {recipe} never landed");
        }
    }

    fn trs(x: f32, z: f32, yaw: f32, s: f32) -> Mat4 {
        Mat4::from_trs(Vec3::new(x, 0.0, z), specula_core::math::Quat::from_euler_xyz_deg(0.0, yaw, 0.0), Vec3::new(s, s * 1.2, s))
    }

    #[test]
    fn bits_cover_and_uncover() {
        let mut b = Vec::new();
        assert!(!covers(&b, 5));
        set_bit(&mut b, 5, true);
        set_bit(&mut b, 700, true);
        assert!(covers(&b, 5) && covers(&b, 700) && !covers(&b, 6) && !covers(&b, 10_000));
        set_bit(&mut b, 5, false);
        assert!(!covers(&b, 5) && covers(&b, 700));
        set_bit(&mut b, MAX_COVER_ID, true);
        assert!(!covers(&b, MAX_COVER_ID), "ids past the cap are never covered");
    }

    #[test]
    fn baking_matches_the_shader_and_white_is_exact() {
        assert_eq!(bake([200, 100, 255, 255], [1.0, 1.0, 1.0]), [200, 100, 255, 255]);
        assert_eq!(bake([255, 255, 255, 255], [0.5, 0.25, 1.0]), [128, 64, 255, 255]);
        // Linear product: half-grey (sRGB 188 ~ 0.5 linear) times half-grey tint.
        let v = bake([188, 188, 188, 255], [0.7354, 0.7354, 0.7354]);
        let expect = (linear_to_srgb(srgb_to_linear(188.0 / 255.0) * srgb_to_linear(0.7354)) * 255.0).round() as u8;
        assert_eq!(v[0], expect);
    }

    #[test]
    fn switch_distance_is_where_every_member_reaches_its_last_lod() {
        use crate::models::{screen_px, select_lod};
        // A 3-LOD model of radius 2 m: past the switch distance it is at level 2 with or
        // without memory of an earlier level; just inside, it may still be finer.
        let (fov, vh) = (50.0, 720);
        let k = switch_factor(2.0, last_level(3));
        let d = switch_distance(k, fov, vh, 0.0);
        for previous in [None, Some(0), Some(1), Some(2)] {
            assert_eq!(select_lod(screen_px(2.0, d * 1.001, fov, vh), 3, previous), 2, "{previous:?}");
        }
        assert_eq!(select_lod(screen_px(2.0, d * 0.7, fov, vh), 3, Some(1)), 1);
        assert_eq!(switch_factor(5.0, 0), 0.0, "single-LOD models are the same at any distance");
        assert_eq!(switch_distance(0.0, fov, vh, 48.0), 48.0);
        assert_eq!(box_distance(Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, -1.0, -1.0), Vec3::new(4.0, 1.0, 1.0)), 3.0);
    }

    #[test]
    fn proxies_merge_by_material_and_are_deterministic() {
        let lib = SharedLibrary::new(Library::new(&sepulcra(), None));
        let tomb = build(&lib, "recipes/tombstone.json", 3);
        let pillar = build(&lib, "recipes/rune_pillar.json", 4);
        assert!(mergeable(&tomb, last_level(tomb.model.lods.len())));
        let members: Vec<MemberInput> = (0..12u64)
            .map(|i| {
                let built = if i % 3 == 0 { Arc::clone(&pillar) } else { Arc::clone(&tomb) };
                let level = last_level(built.model.lods.len());
                MemberInput { id: 100 + i * 7, world: trs(i as f32 * 2.0, (i % 4) as f32, i as f32 * 33.0, 1.0 + i as f32 * 0.05), tint: [0.9, 0.8 + i as f32 * 0.01, 1.0], built, level }
            })
            .collect();
        let origin = Vec3::new(16.0, 0.0, 16.0);
        let a = generate(42, origin, &members);
        let mut shuffled = members.clone();
        shuffled.reverse();
        shuffled.swap(2, 7);
        let b = generate(42, origin, &shuffled);
        assert_eq!(a.hash, b.hash, "member order does not matter");
        assert_eq!(a.groups, b.groups);
        assert_eq!(a.members, 12);
        // Fewer groups than member parts: merged by material.
        let parts: usize = members.iter().map(|m| m.built.model.lods[m.level].parts.len()).sum();
        assert!(a.groups.len() < parts && !a.groups.is_empty(), "{} groups for {parts} parts", a.groups.len());
        // Exactly the members' last-LOD triangles.
        let tris: usize = members.iter().map(|m| m.built.model.lods[m.level].indices.len()).sum();
        assert_eq!(a.groups.iter().map(|g| g.indices.len()).sum::<usize>(), tris);
        // Vertices are where the members put them (relative to the origin).
        let m0 = &members[0];
        let p = m0.built.model.lods[m0.level].positions[0];
        let w = m0.world.transform_point(Vec3::new(p[0], p[1], p[2])) - origin;
        assert!(a.groups.iter().flat_map(|g| &g.vertices).any(|v| (Vec3::new(v.position[0], v.position[1], v.position[2]) - w).length() < 1e-5));
        // Another key or member set: another hash.
        assert_ne!(generate(43, origin, &members).hash, a.hash);
        assert_ne!(generate(42, origin, &members[1..]).hash, a.hash);
    }
}

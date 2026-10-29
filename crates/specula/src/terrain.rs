//! The terrain renderer and water on the engine side (docs/design/TERRAIN.md): which entities
//! are terrains (model entities whose recipe's terrain generator says `"render": "terrain"`) and
//! water surfaces (the `water` component), their GPU residency (height, normal and splat
//! textures per build, coarse far away and full near, uploaded within the streaming budget),
//! and their draws: CDLOD patches (`specula_gpu::terrain`) instead of the recipe's mesh, and
//! water quads. Cosmetic: nothing here touches the simulation.
//!
//! Per frame (window and captures alike): [`Terrains::prepare`] before the model draws (it sets
//! the cover bits that make `GpuModels::draw_model` skip a terrain's stand-in mesh once the
//! terrain renderer can draw it), then [`Terrains::append_draws`] after them.

use crate::models::{GpuModels, SharedLibrary};
use crate::sight::Camera;
use crate::world::{Entity, World};
use specula_core::math::{Mat4, Vec3};
use specula_core::Json;
use specula_gpu::terrain::{self as gt, HeightTree, Selection};
use specula_gpu::{
    DrawItem, Gpu, Material, MaterialExt, MeshId, Surface, TerrainLayer, TerrainMaterial, TextureFilter, TextureFormat, TextureId, TextureSampler,
    TextureWrap,
};
use specula_model::terrain::Terrain;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

/// Coarse copies keep every this many samples (their level 0 is the full data's level 3).
const COARSE: u32 = 8;
/// Terrains with at most this many samples a side have no coarse copy (their full set is small).
const SMALL_SIDE: usize = 65;
/// Resident sets unused for this long are freed.
const IDLE_S: f64 = 5.0;
/// A capture's use keeps a build's sets from the window's distance eviction this long.
const CAPTURE_GRACE_S: f64 = 30.0;
/// Worker texels nothing uploads within this many frames are dropped.
const TEXELS_KEEP: u64 = 120;

/// Groups a frame's drawables (`builds_of`: each one's build, nearest first) by build into
/// `builds`: `(its nearest drawable, build, apron mask)`, nearest build first. A build drawn once
/// takes `mask_of` its drawable (the neighbours its full set's border normals read); one drawn
/// at several places takes 0 (no single set of neighbours: masks per place would fight over the
/// one full set, re-making it every frame). `order` is scratch.
fn group_builds(builds_of: impl Iterator<Item = u64>, order: &mut Vec<(u64, usize)>, builds: &mut Vec<(usize, u64, u8)>, mask_of: impl Fn(usize) -> u8) {
    order.clear();
    order.extend(builds_of.enumerate().map(|(i, b)| (b, i)));
    order.sort_unstable();
    builds.clear();
    let mut k = 0;
    while k < order.len() {
        let (build, first) = order[k];
        let mut end = k + 1;
        while end < order.len() && order[end].0 == build {
            end += 1;
        }
        builds.push((first, build, if end - k == 1 { mask_of(first) } else { 0 }));
        k = end;
    }
    builds.sort_unstable();
}

/// What a terrain at `distance` wants: `(full, coarse, the full set's radius)`. Full data near;
/// small terrains and those without a coarse copy keep it out to the coarse radius (past it,
/// their stand-in mesh draws).
fn wants(t: &Terrain, distance: f32, radius: (f32, f32)) -> (bool, bool, f32) {
    let small = t.columns <= SMALL_SIDE && t.rows <= SMALL_SIDE;
    let coarse_ok = (t.columns - 1) % COARSE as usize == 0 && (t.rows - 1) % COARSE as usize == 0 && !small;
    let full_radius = if coarse_ok { radius.0 } else { radius.1 };
    (distance <= full_radius, coarse_ok && distance <= radius.1, full_radius)
}

/// One resident texture set of a build (full or coarse).
struct Set {
    columns: u32,
    rows: u32,
    cell: [f32; 2],
    heights: TextureId,
    normals: TextureId,
    splat: TextureId,
    tree: HeightTree,
    bytes: u64,
    /// Which sides' normals were computed with the neighbouring tile's samples (bit 0 −x, 1 +x,
    /// 2 −z, 3 +z): a neighbour arriving later has the set made again.
    apron: u8,
    /// When a frame last wanted it (memory pressure frees the least recently wanted first).
    wanted: Instant,
}

impl Set {
    fn free(self, gpu: &mut Gpu) {
        for t in [self.heights, self.normals, self.splat] {
            gpu.retire_texture(t);
        }
    }
}

/// A build's GPU data.
struct Resident {
    full: Option<Set>,
    coarse: Option<Set>,
    /// Mean sRGB colour of each layer's texture (what the layer looks like far away, before its
    /// texture is uploaded).
    means: [[f32; 3]; 4],
    last_used: Instant,
    /// When a capture last used the build: the window doesn't free its sets for
    /// [`CAPTURE_GRACE_S`] after (a capture would wait for them again, every call).
    captured: Option<Instant>,
}

/// A terrain entity the index found: its build and the heights' frame in the model.
struct Indexed {
    id: u64,
    built: Arc<crate::models::Built>,
    terrain: Arc<Terrain>,
    local: Mat4,
}

/// One terrain entity this frame.
struct Drawable {
    id: u64,
    build: u64,
    terrain: Arc<Terrain>,
    /// Local (the heights' frame) → world.
    world: [f32; 16],
    /// Distance from the eye to its world box.
    distance: f32,
    /// Built model's material textures by key (for layer textures).
    built: Arc<crate::models::Built>,
    /// Its place in a grid of tiles (`(shape, x, z)`: tiles of one shape side by side, unrotated),
    /// which finds its neighbours for seamless border normals.
    tile: Option<(u64, i64, i64)>,
    /// This frame's material record (an index into [`Terrains::frame`]) and whether it is the
    /// coarse set; `None`: its stand-in mesh draws.
    slot: Option<(u32, bool)>,
}

/// The texels of one set, made off the engine thread ([`make_texels`]): uploading them is a copy.
struct Texels {
    build: u64,
    coarse: bool,
    apron: u8,
    columns: u32,
    rows: u32,
    cell: [f32; 2],
    heights: Vec<u8>,
    normals: Vec<Vec<u8>>,
    splat: Vec<Vec<u8>>,
    tree: HeightTree,
    bytes: u64,
    /// The frame they were collected in (unclaimed ones are dropped after [`TEXELS_KEEP`] frames).
    made: u64,
}

/// What [`make_texels`] works from: a build's heights and splat (subsampled for the coarse set)
/// and its neighbours' borders.
struct Job {
    build: u64,
    coarse: bool,
    terrain: Arc<Terrain>,
    apron: Apron,
}

/// Texel jobs in flight at once (each holds a set's texels until it is uploaded).
const MAX_JOBS: usize = 8;

/// The GPU bytes a set of `columns × rows` takes (heights, and normals and splat with mips).
fn set_bytes(columns: usize, rows: usize) -> u64 {
    let texels = (columns * rows) as u64 * 4;
    texels + 2 * (texels * 4).div_ceil(3)
}

/// The texels for a job: heights as f32 bits, normals (central differences, the neighbours'
/// samples past the borders) and splat weights (the shared borders averaged), both mipped.
fn make_texels(job: &Job) -> Result<Texels, String> {
    let t = &job.terrain;
    let fmt = TextureFormat::Rgba8Unorm;
    let (columns, rows, cell, heights, splat) = if job.coarse {
        let (c, r, h) = gt::subsample(&t.heights, t.columns as u32, t.rows as u32, COARSE).ok_or("terrain size isn't a multiple of the coarse step")?;
        let (_, _, s) = gt::subsample(&t.splat, t.columns as u32, t.rows as u32, COARSE).ok_or("terrain splat doesn't match its size")?;
        (c, r, [t.cell[0] * COARSE as f32, t.cell[1] * COARSE as f32], Cow::Owned(h), Cow::Owned(s))
    } else {
        (t.columns as u32, t.rows as u32, t.cell, Cow::Borrowed(&t.heights[..]), Cow::Borrowed(&t.splat[..]))
    };
    let tree = HeightTree::new(&heights, columns, rows).ok_or("terrain heights don't match their size")?;
    let h = gt::height_texels(&heights);
    let sides: [Option<&[f32]>; 4] = std::array::from_fn(|k| job.apron.heights[k].as_deref());
    let normals = specula_gpu::generate_mips_owned(columns, rows, fmt, gt::normal_texels_apron(&heights, columns, rows, cell, &sides)).map_err(|e| e.to_string())?;
    let weights = border_splat(&splat, columns as usize, rows as usize, &job.apron.splat).iter().flatten().copied().collect();
    let splat = specula_gpu::generate_mips_owned(columns, rows, fmt, weights).map_err(|e| e.to_string())?;
    let apron = (0..4).filter(|k| job.apron.heights[*k].is_some()).fold(0u8, |m, k| m | 1 << k);
    let bytes = h.len() as u64 + normals.iter().chain(&splat).map(|l| l.len() as u64).sum::<u64>();
    Ok(Texels { build: job.build, coarse: job.coarse, apron, columns, rows, cell, heights: h, normals, splat, tree, bytes, made: 0 })
}

/// The texel worker: one thread making [`Texels`] for the window's frames, so an upload on the
/// engine thread is a copy (for captures too: a capture parks until its texels are up).
struct Worker {
    jobs: Sender<Job>,
    done: Receiver<(u64, bool, u8, Result<Texels, String>)>,
}

impl Worker {
    fn start() -> Option<Worker> {
        let (jobs, inbox) = std::sync::mpsc::channel::<Job>();
        let (outbox, done) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("terrain-texels".into())
            .spawn(move || {
                // Ends when the engine drops its sender.
                while let Ok(job) = inbox.recv() {
                    let apron = (0..4).filter(|k| job.apron.heights[*k].is_some()).fold(0u8, |m, k| m | 1 << k);
                    let made = make_texels(&job);
                    if outbox.send((job.build, job.coarse, apron, made)).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Worker { jobs, done })
    }
}

/// Uploads texels (a copy into the upload queue).
fn upload_texels(gpu: &mut Gpu, t: Texels) -> Result<Set, String> {
    let fmt = TextureFormat::Rgba8Unorm;
    let nearest = TextureSampler { mag: TextureFilter::Nearest, min: TextureFilter::Nearest, mipmap: TextureFilter::Nearest, wrap_u: TextureWrap::ClampToEdge, wrap_v: TextureWrap::ClampToEdge, anisotropy: false };
    let (c, r) = (t.columns, t.rows);
    let failed = |e: specula_gpu::GpuError| format!("terrain textures not uploaded: {e}");
    let heights = gpu.queue_texture_levels(c, r, fmt, &[&t.heights], nearest).map_err(failed)?;
    fn levels(l: &[Vec<u8>]) -> Vec<&[u8]> {
        l.iter().map(Vec::as_slice).collect()
    }
    let normals = match gpu.queue_texture_levels(c, r, fmt, &levels(&t.normals), clamp_sampler()) {
        Ok(n) => n,
        Err(e) => {
            gpu.retire_texture(heights);
            return Err(failed(e));
        }
    };
    let splat = match gpu.queue_texture_levels(c, r, fmt, &levels(&t.splat), clamp_sampler()) {
        Ok(s) => s,
        Err(e) => {
            gpu.retire_texture(heights);
            gpu.retire_texture(normals);
            return Err(failed(e));
        }
    };
    Ok(Set { columns: c, rows: r, cell: t.cell, heights, normals, splat, tree: t.tree, bytes: t.bytes, apron: t.apron, wanted: Instant::now() })
}

/// The tile grid key of a terrain drawn by `m` (local → world): unrotated and unscaled, at whole
/// multiples of its size; `None` otherwise.
fn tile_key(m: &[f32; 16], t: &Terrain) -> Option<(u64, i64, i64)> {
    let axes = [m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]];
    if axes != [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        return None;
    }
    let size = [(t.columns - 1) as f32 * t.cell[0], (t.rows - 1) as f32 * t.cell[1]];
    let at = |v: f32, s: f32| -> Option<i64> {
        let k = (v / s).round();
        ((v - k * s).abs() <= 1e-3 * s.max(1.0) && k.abs() < 1e9).then_some(k as i64)
    };
    let mut h = specula_core::hash::StableHasher::new();
    h.write_u64(t.columns as u64);
    h.write_u64(t.rows as u64);
    h.write_f32(t.cell[0]);
    h.write_f32(t.cell[1]);
    h.write_f32(m[13]);
    Some((h.finish(), at(m[12], size[0])?, at(m[14], size[1])?))
}

/// What the last frames drew (`terrain_info`, `perf_frame`).
#[derive(Clone, Debug, Default)]
pub struct FrameStats {
    pub terrains: u32,
    pub patches: u32,
    pub water: u32,
    /// Terrain entities still drawn by their stand-in mesh (textures or pipelines not ready, the
    /// frame's terrain records full, or the memory budget).
    pub waiting: u32,
    pub uploads: u32,
    pub upload_bytes: u64,
    pub upload_ms: f64,
    /// Material records this frame (one per build and set, shared by its entities).
    pub records: u32,
    /// Sets the memory budget kept out this frame (bytes).
    pub blocked_bytes: u64,
    /// Texel jobs on the worker.
    pub jobs: u32,
    /// Sets this frame wanted that aren't up yet (on the worker, or waiting for upload room),
    /// plus 1 while the terrain pipelines are building: what a parked capture waits for.
    pub pending: u32,
    /// Texels made and waiting for upload room (bytes), and those dropped unclaimed so far.
    pub ready_bytes: u64,
    pub texels_dropped: u64,
    /// Sets freed for memory (least recently wanted first) so far.
    pub evicted: u64,
}

/// Per entity: what the last frame drew of it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Drawn {
    pub patches: u32,
    pub coarse: bool,
    pub levels: (u32, u32),
}

/// What a frame may spend on terrain residency.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Upload allowance: the window's per frame (streaming's), a parked capture's per wait
    /// iteration; `None` (a capture rendering) uploads all the texels that are made.
    pub allowance: Option<crate::two_d::Allowance>,
    /// Streaming's `(radius_m, hlod_radius_m)`: full data within the first, coarse within the
    /// second.
    pub radius: (f32, f32),
    /// Bytes terrain sets may hold in all (streaming's `memory_mb` less what else is resident).
    pub memory: Option<u64>,
}

#[derive(Default)]
pub struct Terrains {
    resident: BTreeMap<u64, Resident>,
    /// The entities drawn as terrain, found again only when models come, go or rebuild
    /// ([`Terrains::refresh_index`]): frames visit these and the water index, never every entity.
    index: Vec<Indexed>,
    /// What the index was made from: `(world mesh epoch, library generation, world generation)`
    /// as the GPU models last synced.
    index_key: Option<(u64, u64, u64)>,
    drawable: Vec<Drawable>,
    water: Vec<u64>,
    /// This frame's terrain materials (`FrameDesc::terrains`), taken and put back by the caller.
    pub frame: Vec<TerrainMaterial>,
    /// This frame's material records: `(build, coarse, a drawable using it)`.
    slots: Vec<(u64, bool, usize)>,
    /// Entities drawn by the terrain renderer this frame (one bit per entity id; their stand-in
    /// meshes are skipped).
    pub cover: Vec<u64>,
    pub stats: FrameStats,
    /// What the last frame drew of each terrain entity, by id (sorted, reused).
    drawn: Vec<(u64, Drawn)>,
    selection: Vec<gt::Selected>,
    grid: Grid,
    worker: Option<Worker>,
    /// Jobs on the worker: `(build, coarse, apron)`.
    in_flight: Vec<(u64, bool, u8)>,
    /// Texels made, waiting for upload room.
    ready: Vec<Texels>,
    /// The terrain pipelines are built (terrain and water draw only then).
    pipelines_ready: bool,
    /// Entities the last frame's scan visited, and index rescans so far (tests: frames visit
    /// terrains and water only).
    pub(crate) visited: u64,
    pub(crate) rescans: u64,
    /// Off: every terrain draws its recipe's mesh (`--terrain off`).
    pub disabled: bool,
    /// Frames prepared so far (ages unclaimed texels).
    frames: u64,
    /// This frame's `(build, drawable)` pairs and `(nearest drawable, build, apron mask)` per
    /// build (reused).
    order: Vec<(u64, usize)>,
    builds: Vec<(usize, u64, u8)>,
    texels_dropped: u64,
    evicted: u64,
    /// Sets captures uploaded so far (repeated captures of one view upload none).
    capture_uploads: u64,
}

/// The terrain build of a model entity the terrain renderer draws, if any: its build and the
/// local → model transform (`None` when an op reshaped the terrain: it then draws as a mesh).
pub fn terrain_of(e: &Entity, lib: &crate::models::Library) -> Option<(Arc<crate::models::Built>, Arc<Terrain>, Mat4)> {
    let r = e.resolved_model()?;
    let built = lib.built(r.handle)?;
    let t = built.model.extras.terrain.as_ref()?;
    t.render.as_ref()?;
    let a = built.model.extras.terrain_transform?;
    let c = &a.cols;
    let m = Mat4 { cols: [[c[0][0], c[0][1], c[0][2], 0.0], [c[1][0], c[1][1], c[1][2], 0.0], [c[2][0], c[2][1], c[2][2], 0.0], [a.t[0], a.t[1], a.t[2], 1.0]] };
    Some((Arc::clone(built), Arc::clone(t), m))
}

fn set_bit(bits: &mut Vec<u64>, id: u64) {
    let w = (id >> 6) as usize;
    if bits.len() <= w {
        bits.resize(w + 1, 0);
    }
    bits[w] |= 1 << (id & 63);
}

fn clamp_sampler() -> TextureSampler {
    TextureSampler {
        mag: TextureFilter::Linear,
        min: TextureFilter::Linear,
        mipmap: TextureFilter::Linear,
        wrap_u: TextureWrap::ClampToEdge,
        wrap_v: TextureWrap::ClampToEdge,
        anisotropy: false, // low-resolution data maps: anisotropic taps cost the iGPU and show nothing
    }
}

impl Terrains {
    /// Whether the index may be stale for the GPU models' sync key (the caller then takes the
    /// library's lock and calls [`Terrains::refresh_index`]).
    pub(crate) fn index_stale(&self, key: Option<(u64, u64, u64)>) -> bool {
        key.is_none() || key != self.index_key
    }

    /// Finds the terrain entities again (`key`: the GPU models' sync key it is made for: a model
    /// entity came, went or changed, a build landed, the world was replaced). `found` tells a
    /// model entity's terrain build (the caller holds the library). Only model entities are
    /// looked at, and only on a new key.
    pub(crate) fn refresh_index(&mut self, world: &World, key: Option<(u64, u64, u64)>, found: impl Fn(&Entity) -> Option<(Arc<crate::models::Built>, Arc<Terrain>, Mat4)>) {
        if !self.index_stale(key) {
            return;
        }
        self.index_key = key;
        self.rescans += 1;
        self.index.clear();
        // (Ids past the cover bitset's cap, transient entities, keep their stand-in mesh.)
        for e in world.entities().filter(|e| e.model.is_some() && e.id < crate::hlod::MAX_COVER_ID) {
            if let Some((built, terrain, local)) = found(e) {
                self.index.push(Indexed { id: e.id, built, terrain, local });
            }
        }
    }

    /// This frame's terrains and water, from the indexes (no other entity is visited), nearest
    /// first.
    pub(crate) fn scan(&mut self, world: &World, eye: Vec3, include_ui: bool) {
        self.drawable.clear();
        self.water.clear();
        self.visited = 0;
        if self.disabled {
            return;
        }
        for it in &self.index {
            self.visited += 1;
            let Some(e) = world.get(it.id) else { continue };
            if !crate::sight::in_frame(e, include_ui) {
                continue;
            }
            let world_m = e.model() * it.local;
            let (lo, hi) = e.world_aabb();
            let d = Vec3::new((lo.x - eye.x).max(eye.x - hi.x).max(0.0), (lo.y - eye.y).max(eye.y - hi.y).max(0.0), (lo.z - eye.z).max(eye.z - hi.z).max(0.0)).length();
            let world = world_m.to_cols_array();
            let tile = tile_key(&world, &it.terrain);
            self.drawable.push(Drawable { id: e.id, build: it.built.id, terrain: Arc::clone(&it.terrain), world, distance: d, built: Arc::clone(&it.built), tile, slot: None });
        }
        for e in world.water_entities() {
            self.visited += 1;
            if crate::sight::in_frame(e, include_ui) {
                self.water.push(e.id);
            }
        }
        // Nearest first (uploads go in that order); ties by id: deterministic.
        self.drawable.sort_by(|a, b| a.distance.total_cmp(&b.distance).then(a.id.cmp(&b.id)));
    }

    /// Before the model draws: finds the terrains and water in view, makes sure their textures
    /// are on the GPU (the window within its allowance, its texels made on the worker; captures
    /// everything they show, at once), asks for the terrain pipelines, gives each terrain a
    /// material record (one per build and set, shared by its entities), and sets
    /// [`Terrains::cover`]. Only the window's frames free sets by distance (idle time frees the
    /// rest): a capture from elsewhere never evicts what the window draws. Returns what it
    /// uploaded.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(&mut self, gpu: &mut Gpu, library: &SharedLibrary, models_key: Option<(u64, u64, u64)>, world: &World, camera: &Camera, include_ui: bool, window: bool, budget: Budget) -> crate::two_d::Used {
        let started = Instant::now();
        if !self.disabled && self.index_stale(models_key) {
            let lib = library.lock();
            self.refresh_index(world, models_key, |e| terrain_of(e, &lib));
        }
        self.scan(world, camera.eye, include_ui);
        self.cover.iter_mut().for_each(|w| *w = 0);
        self.slots.clear();
        self.collect();
        let mut used = crate::two_d::Used::default();
        if self.drawable.is_empty() && self.water.is_empty() {
            self.frames += 1;
            self.pipelines_ready = false;
            self.free_idle(gpu, false);
            self.prune_ready();
            self.stats = FrameStats { jobs: self.in_flight.len() as u32, texels_dropped: self.texels_dropped, evicted: self.evicted, ..FrameStats::default() };
            return used;
        }
        let ready = gpu.pipeline_variants().terrain;
        if !ready {
            gpu.request_terrain_pipelines();
        }
        self.pipelines_ready = ready;
        let now = Instant::now();
        self.frames += 1;
        let (mut waiting, mut blocked, mut pending) = (0u32, 0u64, 0u32);
        let mut held = self.bytes();
        // Tiles by grid place: each finds its neighbours' samples just past its borders.
        // (Sorted, reused: no allocation per frame once warm.)
        let mut grid = std::mem::take(&mut self.grid);
        grid.clear();
        grid.extend(self.drawable.iter().enumerate().filter_map(|(i, d)| d.tile.map(|k| (k, i))));
        grid.sort_unstable();
        // Residency is decided per build, from its nearest entity (drawables come nearest first),
        // before any record: a set taken for one entity is never freed by a farther entity of
        // the same build in the same frame. A build drawn at one grid place gets its
        // neighbours' borders (seamless normals); one drawn at several places has no single set
        // of neighbours, so its full set takes none (and is never made again for each place).
        let mut order = std::mem::take(&mut self.order);
        let mut builds = std::mem::take(&mut self.builds);
        let all = &self.drawable;
        group_builds(all.iter().map(|d| d.build), &mut order, &mut builds, |i| apron_mask(&grid, all[i].tile, &all[i].terrain));
        let radius = budget.radius;
        // First, what this frame wants of what is resident: memory pressure then frees only sets
        // no build wants this frame (stamping as builds come would free a farther build's set,
        // which it then makes again, and so on every frame).
        for &(di, build, _) in &builds {
            let d = &self.drawable[di];
            let (want_full, want_coarse, _) = wants(&d.terrain, d.distance, radius);
            if let Some(r) = self.resident.get_mut(&build) {
                for s in r.full.iter_mut().filter(|_| want_full).chain(r.coarse.iter_mut().filter(|_| want_coarse)) {
                    s.wanted = now;
                }
            }
        }
        for &(di, build, have) in &builds {
            let d = &self.drawable[di];
            let t = Arc::clone(&d.terrain);
            let (distance, tile) = (d.distance, if have != 0 { d.tile } else { None });
            let (want_full, want_coarse, full_radius) = wants(&t, distance, radius);
            let r = self.resident.entry(build).or_insert_with(|| Resident { full: None, coarse: None, means: layer_means(&d.built, &t), last_used: now, captured: None });
            r.last_used = now;
            if !window {
                r.captured = Some(now);
            }
            let mut need: [Option<(bool, u8)>; 2] = [None, None];
            if want_coarse && r.coarse.is_none() {
                need[0] = Some((true, 0));
            }
            // Missing, or a neighbour arrived or left since (its border normals are made again;
            // the old set draws until the new one is up).
            if want_full && r.full.as_ref().is_none_or(|f| f.apron != have) {
                need[1] = Some((false, have));
            }
            let replacing = r.full.as_ref().map_or(0, |f| f.bytes);
            for (coarse, apron) in need.into_iter().flatten() {
                let (c, rw) = if coarse { ((t.columns - 1) / COARSE as usize + 1, (t.rows - 1) / COARSE as usize + 1) } else { (t.columns, t.rows) };
                let est = set_bytes(c, rw);
                let freed = if coarse { 0 } else { replacing };
                if let Some(m) = budget.memory {
                    // Over the budget: the least recently wanted sets (not this frame's) go first.
                    if held.saturating_sub(freed) + est > m {
                        held = self.evict_lru(gpu, held, held.saturating_sub(freed) + est - m, now);
                    }
                    if held.saturating_sub(freed) + est > m {
                        blocked += est;
                        continue;
                    }
                }
                // (The window's allowance, or a parked capture's per-iteration share.)
                let over = budget.allowance.is_some_and(|a| {
                        let spent_ms = used.ms + started.elapsed().as_secs_f64() * 1e3;
                        !(a.first_free && used.items == 0) && (used.bytes >= a.bytes || spent_ms >= a.ms)
                    });
                let job = |grid: &Grid, all: &[Drawable]| Job { build, coarse, terrain: Arc::clone(&t), apron: if coarse { Apron::default() } else { apron_of(grid, tile, &t, |i| &all[i].terrain) } };
                let texels = if let Some(k) = self.ready.iter().position(|x| x.build == build && x.coarse == coarse && x.apron == apron) {
                    if over {
                        pending += 1;
                        continue;
                    }
                    Some(self.ready.swap_remove(k))
                } else if self.start_worker() {
                    // Made on the worker, for the window and captures alike (a capture waits
                    // for them, parked like one waiting for a model build); a later frame or the
                    // parked capture uploads them.
                    pending += 1;
                    // Made on the worker; a later frame uploads it.
                    if !self.in_flight.iter().any(|j| j.0 == build && j.1 == coarse) && self.in_flight.len() < MAX_JOBS {
                        let j = job(&grid, &self.drawable);
                        let sent = self.worker.as_ref().is_some_and(|w| w.jobs.send(j).is_ok());
                        if sent {
                            self.in_flight.push((build, coarse, apron));
                        } else {
                            // The thread is gone: made here from the next frame on.
                            self.worker = None;
                        }
                    }
                    None
                } else if over {
                    pending += 1;
                    continue;
                } else {
                    // No worker thread could be started: made here.
                    make_texels(&job(&grid, &self.drawable)).ok()
                };
                let Some(texels) = texels else { continue };
                if let Ok(mut set) = upload_texels(gpu, texels) {
                    set.wanted = now;
                    used.items += 1;
                    used.bytes += set.bytes;
                    held += set.bytes;
                    let r = self.resident.get_mut(&build).expect("entered above");
                    let old = if coarse { r.coarse.replace(set) } else { r.full.replace(set) };
                    if let Some(old) = old {
                        held = held.saturating_sub(old.bytes);
                        old.free(gpu);
                    }
                }
            }
            // Far again (the build's nearest entity): the window frees the sets, with hysteresis
            // near the switch, unless a capture used them lately (a capture from elsewhere would
            // otherwise wait for them again, every call). A capture never evicts.
            let r = self.resident.get_mut(&build).expect("entered above");
            let shielded = r.captured.is_some_and(|c| now.duration_since(c).as_secs_f64() < CAPTURE_GRACE_S);
            if window && !shielded {
                if !want_full && distance > full_radius * 1.25 {
                    if let Some(s) = r.full.take() {
                        held = held.saturating_sub(s.bytes);
                        s.free(gpu);
                    }
                }
                if !want_coarse && distance > radius.1 * 1.1 {
                    if let Some(s) = r.coarse.take() {
                        held = held.saturating_sub(s.bytes);
                        s.free(gpu);
                    }
                }
            }
        }
        // Records: nothing is freed from here on, so every record's set stays resident.
        for di in 0..self.drawable.len() {
            let d = &self.drawable[di];
            let (build, distance) = (d.build, d.distance);
            let Some(r) = self.resident.get(&build) else {
                waiting += 1;
                continue;
            };
            // Full data near, the coarse copy far (or while the full one isn't up yet).
            let coarse = match (&r.full, &r.coarse) {
                (Some(_), Some(_)) if distance > radius.0 => Some(true),
                (Some(_), _) => Some(false),
                (None, Some(_)) => Some(true),
                (None, None) => None,
            };
            // One record per (build, set), shared by its entities, decided before the cover bit:
            // a terrain past the frame's records keeps its stand-in mesh (never neither).
            let slot = coarse.and_then(|c| match self.slots.iter().position(|s| s.0 == build && s.1 == c) {
                Some(k) => Some((k as u32, c)),
                None if self.slots.len() < gt::MAX_TERRAINS => {
                    self.slots.push((build, c, di));
                    Some(((self.slots.len() - 1) as u32, c))
                }
                None => None,
            });
            if ready && slot.is_some() {
                set_bit(&mut self.cover, d.id);
                self.drawable[di].slot = slot;
            } else {
                waiting += 1;
            }
        }
        debug_assert!(self.slots.iter().all(|&(b, c, _)| self.resident.get(&b).is_some_and(|r| if c { r.coarse.is_some() } else { r.full.is_some() })), "every record's set is resident");
        self.grid = grid;
        self.order = order;
        self.builds = builds;
        // Texels nothing wants any more (a build replaced or deleted, or unclaimed too long) go.
        self.prune_ready();
        // Builds no terrain drew for a while (replaced by an edit, deleted) go.
        self.free_idle(gpu, false);
        used.ms = started.elapsed().as_secs_f64() * 1e3;
        if !window {
            self.capture_uploads += u64::from(used.items);
        }
        self.stats.waiting = waiting;
        // A capture also waits for the terrain pipelines (built in the background) before it
        // draws terrain and water.
        self.stats.pending = pending + u32::from(!ready);
        self.stats.uploads = used.items;
        self.stats.upload_bytes = used.bytes;
        self.stats.upload_ms = used.ms;
        self.stats.blocked_bytes = blocked;
        self.stats.records = self.slots.len() as u32;
        self.stats.jobs = self.in_flight.len() as u32;
        self.stats.ready_bytes = self.ready.iter().map(|t| t.bytes).sum();
        self.stats.texels_dropped = self.texels_dropped;
        self.stats.evicted = self.evicted;
        used
    }

    /// Starts the texel worker if it isn't running; whether it runs.
    fn start_worker(&mut self) -> bool {
        if self.worker.is_none() {
            self.worker = Worker::start();
        }
        self.worker.is_some()
    }

    /// Takes the worker's finished texels.
    fn collect(&mut self) {
        let Some(w) = &self.worker else { return };
        while let Ok((build, coarse, apron, made)) = w.done.try_recv() {
            if let Some(k) = self.in_flight.iter().position(|j| *j == (build, coarse, apron)) {
                self.in_flight.swap_remove(k);
            }
            if let Ok(mut t) = made {
                // The newest texels for a set replace older ones still waiting.
                self.ready.retain(|x| !(x.build == t.build && x.coarse == t.coarse));
                t.made = self.frames;
                self.ready.push(t);
            }
        }
    }

    /// Drops texels nothing uploaded within [`TEXELS_KEEP`] frames (the camera moved on, a build
    /// went), counting them.
    fn prune_ready(&mut self) {
        let (frames, before) = (self.frames, self.ready.len());
        let resident = &self.resident;
        self.ready.retain(|t| resident.contains_key(&t.build) && frames.saturating_sub(t.made) <= TEXELS_KEEP);
        self.texels_dropped += (before - self.ready.len()) as u64;
    }

    /// Frees the least recently wanted sets not wanted this frame (`now`) until `need` bytes are
    /// freed or none is left; returns what is still held.
    fn evict_lru(&mut self, gpu: &mut Gpu, mut held: u64, need: u64, now: Instant) -> u64 {
        let mut freed = 0;
        while freed < need {
            let oldest = self
                .resident
                .iter()
                .flat_map(|(b, r)| [(r.full.as_ref(), false), (r.coarse.as_ref(), true)].into_iter().filter_map(move |(s, c)| s.filter(|s| s.wanted < now).map(|s| (s.wanted, *b, c))))
                .min();
            let Some((_, b, c)) = oldest else { break };
            let r = self.resident.get_mut(&b).expect("listed above");
            if let Some(s) = if c { r.coarse.take() } else { r.full.take() } {
                freed += s.bytes;
                held = held.saturating_sub(s.bytes);
                self.evicted += 1;
                s.free(gpu);
            }
        }
        held
    }

    /// Frees sets no terrain used for [`IDLE_S`] (`all`: every set: shutdown, a device reset).
    pub fn free_idle(&mut self, gpu: &mut Gpu, all: bool) {
        let now = Instant::now();
        let stale: Vec<u64> = self.resident.iter().filter(|(_, r)| all || now.duration_since(r.last_used).as_secs_f64() > IDLE_S).map(|(k, _)| *k).collect();
        for k in stale {
            if let Some(r) = self.resident.remove(&k) {
                r.full.into_iter().chain(r.coarse).for_each(|s| s.free(gpu));
            }
        }
    }

    /// After the model draws: the patches of every covered terrain (and the water surfaces, once
    /// their pass can draw them) into `out`, their owners (entity ids) into `owners`; the
    /// terrains' material records into [`Terrains::frame`].
    pub fn append_draws(&mut self, models: &GpuModels, quad: Option<MeshId>, world: &World, camera: &Camera, out: &mut Vec<DrawItem>, mut owners: Option<&mut Vec<u64>>) {
        self.frame.clear();
        self.drawn.clear();
        let (mut patches, mut terrains) = (0u32, 0u32);
        let eye = [camera.eye.x, camera.eye.y, camera.eye.z];
        // The records, in slot order.
        for &(build, coarse, di) in &self.slots {
            let d = &self.drawable[di];
            let r = &self.resident[&build];
            let Some(set) = (if coarse { r.coarse.as_ref() } else { r.full.as_ref() }) else { continue };
            let t = &d.terrain;
            let Some(render) = t.render.as_ref() else { continue };
            let mut layers = [TerrainLayer::default(); 4];
            let textures = models.uploaded.get(&build).map(|m| &m.textures);
            for (k, l) in layers.iter_mut().enumerate() {
                let Some(spec) = render.layers.get(k) else { continue };
                let tint = t.splat_colors.get(k).map_or([1.0; 3], |c| [c[0], c[1], c[2]]);
                let mat = spec.texture.as_ref().and_then(|key| d.built.materials.get(key));
                let tex = |slot: Option<usize>| slot.and_then(|i| textures.and_then(|t| t.get(i)).copied());
                let albedo = mat.and_then(|m| tex(m.base));
                // Without its texture (not built, or not uploaded yet) a layer shows its texture's
                // mean colour.
                let tint = if spec.texture.is_some() && albedo.is_none() { std::array::from_fn(|c| tint[c] * r.means[k][c]) } else { tint };
                *l = TerrainLayer {
                    tint,
                    roughness: spec.roughness,
                    specular: spec.specular,
                    uv_scale: spec.uv_scale,
                    triplanar_deg: spec.triplanar_deg,
                    albedo,
                    normal: mat.and_then(|m| tex(m.normal)),
                    normal_strength: spec.normal_strength,
                };
            }
            self.frame.push(TerrainMaterial {
                columns: set.columns,
                rows: set.rows,
                cell: set.cell,
                heights: set.heights,
                normals: set.normals,
                splat: set.splat,
                layers,
                layer_count: (render.layers.len().clamp(1, 4)) as u32,
                grain: render.grain,
            });
        }
        debug_assert_eq!(self.frame.len(), self.slots.len(), "every slot's set is resident");
        for d in &self.drawable {
            let Some((index, coarse)) = d.slot else { continue };
            let r = &self.resident[&d.build];
            let Some(set) = (if coarse { r.coarse.as_ref() } else { r.full.as_ref() }) else { continue };
            let offset = if coarse { 3 } else { 0 };
            let t = &d.terrain;
            let Some(render) = t.render.as_ref() else { continue };
            // Ranges from the full-resolution cell and the terrain's largest scale, so tiles of
            // one terrain agree.
            let m = &d.world;
            let scale = [m[0] * m[0] + m[1] * m[1] + m[2] * m[2], m[4] * m[4] + m[5] * m[5] + m[6] * m[6], m[8] * m[8] + m[9] * m[9] + m[10] * m[10]]
                .into_iter()
                .fold(0.0f32, f32::max)
                .sqrt();
            let ranges = gt::ranges(0.0, t.cell[0].max(t.cell[1]) * scale);
            let s = Selection { tree: &set.tree, cell: set.cell, world: m, ranges: &ranges, level_offset: offset, terrain: index, skirt: render.skirt, skirt_mask: if render.skirt > 0.0 { 15 } else { 0 } };
            self.selection.clear();
            gt::select(&s, eye, &mut self.selection);
            let before = out.len();
            out.extend(self.selection.iter().map(|p| gt::patch_draw(m, p)));
            if let Some(o) = owners.as_deref_mut() {
                o.resize(out.len(), d.id);
            }
            let n = (out.len() - before) as u32;
            let levels = self.selection.iter().fold((u32::MAX, 0), |(lo, hi), p| (lo.min(p.patch.level + offset), hi.max(p.patch.level + offset)));
            self.drawn.push((d.id, Drawn { patches: n, coarse, levels }));
            patches += n;
            terrains += 1;
        }
        self.drawn.sort_unstable_by_key(|x| x.0);
        let mut water = 0;
        // Water only once its pass can draw it: until then nothing may claim its pixels.
        if let (Some(quad), true) = (quad, self.pipelines_ready) {
            for &id in &self.water {
                let Some(e) = world.get(id) else { continue };
                let Some(w) = &e.water else { continue };
                let size = Mat4 { cols: [[w.size[0], 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, w.size[1], 0.0], [0.0, 0.0, 0.0, 1.0]] };
                let model = e.model() * size;
                let mut d = DrawItem::new(quad, model.to_cols_array(), [1.0; 4]);
                d.material = Material::Water;
                d.surface = Surface { ext: MaterialExt::Water(w.style), ..Surface::default() };
                out.push(d);
                if let Some(o) = owners.as_deref_mut() {
                    o.push(id);
                }
                water += 1;
            }
        }
        self.stats.terrains = terrains;
        self.stats.patches = patches;
        self.stats.water = water;
    }

    /// Whether entity `id` is drawn by the terrain renderer this frame.
    pub fn covers(&self, id: u64) -> bool {
        crate::hlod::covers(&self.cover, id)
    }

    /// What the last frame drew of entity `id`.
    pub fn drawn(&self, id: u64) -> Option<Drawn> {
        self.drawn.binary_search_by_key(&id, |x| x.0).ok().map(|k| self.drawn[k].1)
    }

    /// GPU bytes of every resident set.
    pub fn bytes(&self) -> u64 {
        self.resident.values().flat_map(|r| r.full.iter().chain(&r.coarse)).map(|s| s.bytes).sum()
    }

    /// `perf_frame`'s terrain section.
    pub fn stats_json(&self) -> Json {
        let s = &self.stats;
        Json::obj([
            ("terrains", Json::from(s.terrains)),
            ("patches", Json::from(s.patches)),
            ("records", Json::from(s.records)),
            ("water", Json::from(s.water)),
            ("waiting", Json::from(s.waiting)),
            ("indexed", Json::from(self.index.len())),
            ("resident_builds", Json::from(self.resident.len())),
            ("gpu_bytes", Json::from(self.bytes())),
            ("blocked_bytes", Json::from(s.blocked_bytes)),
            ("uploads", Json::from(s.uploads)),
            ("upload_bytes", Json::from(s.upload_bytes)),
            ("upload_ms", Json::from((s.upload_ms * 1000.0).round() / 1000.0)),
            ("texel_jobs", Json::from(s.jobs)),
            ("pending", Json::from(s.pending)),
            ("texels_waiting_bytes", Json::from(s.ready_bytes)),
            ("texels_dropped", Json::from(s.texels_dropped)),
            ("evicted_for_memory", Json::from(s.evicted)),
            ("capture_uploads", Json::from(self.capture_uploads)),
        ])
    }

    /// One build's residency (`terrain_info`): `(full, coarse, bytes)`.
    pub fn residency(&self, build: u64) -> (bool, bool, u64) {
        self.resident.get(&build).map_or((false, false, 0), |r| (r.full.is_some(), r.coarse.is_some(), r.full.iter().chain(&r.coarse).map(|s| s.bytes).sum()))
    }
}

/// What a tile's neighbours give its textures: their samples just past each border (−x, +x
/// columns; −z, +z rows) for the normals, and their splat weights on the shared border.
#[derive(Default)]
struct Apron {
    heights: [Option<Vec<f32>>; 4],
    splat: [Option<Vec<[u8; 4]>>; 4],
}

/// `splat` with each border shared with a neighbour set to the mean of both tiles' weights there
/// (each tile's slope at the border is one-sided, so their weights differ; the mean is the same
/// from both sides, so the layers meet without a step).
fn border_splat(splat: &[[u8; 4]], c: usize, r: usize, sides: &[Option<Vec<[u8; 4]>>; 4]) -> Vec<[u8; 4]> {
    let mut out = splat.to_vec();
    let mean = |a: [u8; 4], b: [u8; 4]| -> [u8; 4] { std::array::from_fn(|k| (a[k] as u16 + b[k] as u16).div_ceil(2) as u8) };
    for (k, side) in sides.iter().enumerate() {
        let Some(n) = side else { continue };
        let cells: Vec<usize> = match k {
            0 => (0..r).map(|j| j * c).collect(),
            1 => (0..r).map(|j| j * c + c - 1).collect(),
            2 => (0..c).collect(),
            _ => (0..c).map(|i| (r - 1) * c + i).collect(),
        };
        if n.len() != cells.len() {
            continue;
        }
        for (idx, w) in cells.into_iter().zip(n) {
            // Both tiles average the same two values in the same order of operands' sum.
            out[idx] = mean(splat[idx], *w);
        }
    }
    out
}

/// Which of `d`'s sides have a neighbouring tile (bit 0 −x, 1 +x, 2 −z, 3 +z).
/// Tile grid places and the drawable at each, sorted.
type Grid = Vec<((u64, i64, i64), usize)>;

fn grid_find(grid: &Grid, key: (u64, i64, i64)) -> Option<usize> {
    grid.binary_search_by(|(k, _)| k.cmp(&key)).ok().map(|i| grid[i].1)
}

fn apron_mask(grid: &Grid, tile: Option<(u64, i64, i64)>, t: &Terrain) -> u8 {
    let Some((sig, x, z)) = tile else { return 0 };
    let (c, r) = (t.columns, t.rows);
    if c < 3 || r < 3 {
        return 0;
    }
    [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)].into_iter().enumerate().filter(|(_, (dx, dz))| grid_find(grid, (sig, x + dx, z + dz)).is_some()).fold(0u8, |m, (k, _)| m | 1 << k)
}

/// The neighbouring tiles' samples just past `d`'s borders (−x, +x columns; −z, +z rows), where a
/// neighbour of the same shape sits in the grid.
fn apron_of<'a>(grid: &Grid, tile: Option<(u64, i64, i64)>, t: &Terrain, terrain_of: impl Fn(usize) -> &'a Arc<Terrain>) -> Apron {
    let mut out = Apron::default();
    let Some((sig, x, z)) = tile else { return out };
    let (c, r) = (t.columns, t.rows);
    let at = |n: &Terrain, i: usize, j: usize| n.heights[j * c + i];
    for (k, (dx, dz)) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)].into_iter().enumerate() {
        let Some(ni) = grid_find(grid, (sig, x + dx, z + dz)) else { continue };
        let n = terrain_of(ni);
        if n.columns != c || n.rows != r || c < 3 || r < 3 {
            continue;
        }
        out.heights[k] = Some(match k {
            0 => (0..r).map(|j| at(n, c - 2, j)).collect(),
            1 => (0..r).map(|j| at(n, 1, j)).collect(),
            2 => (0..c).map(|i| at(n, i, r - 2)).collect(),
            _ => (0..c).map(|i| at(n, i, 1)).collect(),
        });
        let w = |i: usize, j: usize| n.splat.get(j * c + i).copied().unwrap_or([255, 0, 0, 0]);
        out.splat[k] = Some(match k {
            0 => (0..r).map(|j| w(c - 1, j)).collect(),
            1 => (0..r).map(|j| w(0, j)).collect(),
            2 => (0..c).map(|i| w(i, r - 1)).collect(),
            _ => (0..c).map(|i| w(i, 0)).collect(),
        });
    }
    out
}

/// Mean sRGB colour of each layer's texture (white without one).
fn layer_means(built: &crate::models::Built, t: &Terrain) -> [[f32; 3]; 4] {
    let mut out = [[1.0; 3]; 4];
    let Some(render) = &t.render else { return out };
    for (k, l) in render.layers.iter().take(4).enumerate() {
        let Some(key) = &l.texture else { continue };
        let Some((_, set)) = built.model.extras.textures.iter().find(|(n, _)| n == key) else { continue };
        let px = &set.base_color;
        let n = (px.len() / 4).max(1) as f32;
        let mut sum = [0.0f32; 3];
        for p in px.chunks_exact(4) {
            for c in 0..3 {
                sum[c] += p[c] as f32 / 255.0;
            }
        }
        out[k] = sum.map(|s| s / n);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_model::terrain::{NoiseKind, NoiseLayer, TerrainSpec};

    fn tile(ox: f32) -> Arc<Terrain> {
        Arc::new(tile_spec(ox).build(specula_model::terrain::MAX_WORK).unwrap())
    }

    fn tile_spec(ox: f32) -> TerrainSpec {
        TerrainSpec {
            size: [32.0, 32.0],
            samples: [33, 33],
            base: 0.0,
            seed: 3,
            layers: vec![NoiseLayer { kind: NoiseKind::Fbm, scale: 20.0, octaves: 3, amplitude: 4.0 }],
            warp: 0.0,
            island: 0.0,
            terrace: (0.0, 0.0),
            pads: vec![],
            hydraulic: None,
            thermal: None,
            splat: vec![],
            offset: [ox, 0.0],
            edge_lock: 0,
            edits: vec![],
        }
    }

    fn at(x: f32, z: f32) -> [f32; 16] {
        [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, 0.0, z, 1.0]
    }

    /// Review of a89b5eb: every frame walked every entity, terrain or not (10k cubes: the window's
    /// draw list 1.3 ms against 0.63). Frames now visit the terrain index and the water index
    /// only; the index is made again only when the GPU models' sync key moves.
    #[test]
    fn frames_visit_terrains_and_water_only() {
        use crate::world::{EntityKey, Shape, SpawnSpec, UpdateSpec};
        let mut w = World::default();
        for i in 0..1000 {
            let mut s = SpawnSpec::new(Shape::Cube);
            s.name = Some(format!("c{i}"));
            w.spawn(s, "t").unwrap();
        }
        let mut pool = SpawnSpec::new(Shape::Group);
        pool.name = Some("pool".into());
        pool.water = Some(crate::water::WaterSpec::parse(&Json::parse(r#"{"preset": "murk"}"#).unwrap()).unwrap());
        w.spawn(pool, "t").unwrap();
        let mut t = Terrains::default();
        let looked = std::cell::Cell::new(0);
        let key = Some((1, 1, 1));
        t.refresh_index(&w, key, |_| {
            looked.set(looked.get() + 1);
            None
        });
        assert_eq!(looked.get(), 0, "no model entities: none looked at");
        for _ in 0..3 {
            assert!(!t.index_stale(key), "the same key: no rescan, no library lock");
            t.scan(&w, Vec3::new(0.0, 0.0, 0.0), false);
            assert_eq!((t.visited, t.water.len(), t.drawable.len()), (1, 1, 0), "one water entity visited, no cube");
        }
        assert_eq!(t.rescans, 1);
        assert!(t.index_stale(Some((2, 1, 1))) && t.index_stale(None), "a new key (or none) rescans");
        // The water index follows the component.
        w.update(&EntityKey::Path("/pool".into()), UpdateSpec { water: Some(None), ..Default::default() }, "t").unwrap();
        t.scan(&w, Vec3::new(0.0, 0.0, 0.0), false);
        assert_eq!((t.visited, t.water.len()), (0, 0));
        assert_eq!(w.water_entities().count(), 0);
    }

    /// Review of 7d4df4e: two entities of one build (one near, one 1 km away) and tiles of one
    /// build side by side. Residency is decided once per build, from its nearest entity, so a far
    /// entity never frees what a near one's record uses; a build drawn at several places reads no
    /// neighbours (its masks would fight over the one full set).
    #[test]
    fn residency_is_decided_once_per_build_from_its_nearest_entity() {
        let (mut order, mut builds) = (Vec::new(), Vec::new());
        // Drawables nearest first: build 7 at 0 and 3, build 9 at 1 and 2, build 5 alone at 4.
        group_builds([7u64, 9, 9, 7, 5].into_iter(), &mut order, &mut builds, |i| [0b1, 0b10, 0b100, 0b1000, 0b0110][i]);
        assert_eq!(builds, vec![(0, 7, 0), (1, 9, 0), (4, 5, 0b0110)]);
        // Wants come from the nearest entity's distance: a near one keeps the full set.
        let t = tile(0.0);
        assert!(wants(&t, 10.0, (384.0, 4096.0)).0);
        assert_eq!(wants(&t, 5000.0, (384.0, 4096.0)), (false, false, 4096.0), "33 samples: no coarse copy, full set out to the coarse radius");
    }

    /// Texels are made off the engine thread (review of a89b5eb: a 257-sample set took 15 ms on
    /// it); the memory budget's estimate is what an upload holds, and the coarse copy is the
    /// full data's every 8th sample.
    #[test]
    fn texels_are_made_whole_and_estimated() {
        let spec = TerrainSpec { size: [128.0, 128.0], samples: [129, 129], ..tile_spec(0.0) };
        let t = Arc::new(spec.build(specula_model::terrain::MAX_WORK).unwrap());
        for coarse in [false, true] {
            let job = Job { build: 7, coarse, terrain: Arc::clone(&t), apron: Apron::default() };
            let x = make_texels(&job).unwrap();
            let (c, r) = if coarse { (17, 17) } else { (129, 129) };
            assert_eq!((x.columns, x.rows, x.heights.len()), (c, r, (c * r * 4) as usize));
            let est = set_bytes(c as usize, r as usize);
            // (A slight over-estimate: the budget never lets more in than it says.)
            assert!(est >= x.bytes && est as f64 <= x.bytes as f64 * 1.05, "estimate {est} vs {} bytes", x.bytes);
            assert_eq!(x.normals.len(), x.splat.len());
        }
        // The worker makes the same texels as the engine thread would.
        let w = Worker::start().expect("a thread");
        w.jobs.send(Job { build: 9, coarse: false, terrain: Arc::clone(&t), apron: Apron::default() }).unwrap();
        let (b, coarse, apron, made) = w.done.recv().unwrap();
        let here = make_texels(&Job { build: 9, coarse: false, terrain: t, apron: Apron::default() }).unwrap();
        let made = made.unwrap();
        assert_eq!((b, coarse, apron), (9, false, 0));
        assert!(made.heights == here.heights && made.normals == here.normals && made.splat == here.splat);
    }

    /// Tiles side by side find each other and read the right samples for their border normals.
    #[test]
    fn tiles_find_their_neighbours_for_seamless_normals() {
        let (a, b) = (tile(0.0), tile(32.0));
        assert_eq!(tile_key(&at(0.0, 0.0), &a).map(|k| (k.1, k.2)), Some((0, 0)));
        assert_eq!(tile_key(&at(32.0, 0.0), &b).map(|k| (k.1, k.2)), Some((1, 0)));
        assert_eq!(tile_key(&at(10.0, 0.0), &a), None, "not on the grid");
        let mut rot = at(0.0, 0.0);
        rot[0] = 0.0;
        rot[2] = -1.0;
        assert_eq!(tile_key(&rot, &a), None, "rotated");
        let all = [(tile_key(&at(0.0, 0.0), &a), Arc::clone(&a)), (tile_key(&at(32.0, 0.0), &b), Arc::clone(&b))];
        let mut grid: Grid = all.iter().enumerate().filter_map(|(i, d)| d.0.map(|k| (k, i))).collect();
        grid.sort_unstable();
        assert_eq!(apron_mask(&grid, all[0].0, &a), 0b10);
        assert_eq!(apron_mask(&grid, all[1].0, &b), 0b01);
        let apron = apron_of(&grid, all[0].0, &a, |i| &all[i].1);
        // Past a's +x border: b's second column.
        let col: Vec<f32> = (0..33).map(|j| b.heights[j * 33 + 1]).collect();
        assert_eq!(apron.heights[1].as_deref(), Some(col.as_slice()));
        assert!(apron.heights[0].is_none() && apron.heights[2].is_none() && apron.heights[3].is_none());
        // The shared border's splat: both tiles end up with the same weights there.
        let mine = border_splat(&a.splat, 33, 33, &apron.splat);
        let back = apron_of(&grid, all[1].0, &b, |i| &all[i].1);
        let theirs = border_splat(&b.splat, 33, 33, &back.splat);
        assert!((0..33).all(|j| mine[j * 33 + 32] == theirs[j * 33]));
        // The shared border column is the same samples in both tiles (one noise domain).
        assert!((0..33).all(|j| a.heights[j * 33 + 32] == b.heights[j * 33]));
    }
}

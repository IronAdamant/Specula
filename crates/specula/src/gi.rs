//! Baked GI in the engine (docs/design/GI.md): volumes as world data under `/gi`, the bake's
//! input gathered from the world, bake jobs on a worker thread, the content-addressed store,
//! staleness as data, and the resident chunks a frame samples.
//!
//! - **Volumes** are `/gi`'s children: bare groups at their box's centre holding
//!   `props.gi_volume` (settings and, once baked, the result's hash and keys). Reserved data of
//!   the write policy (`Rule::Gi`): only `gi_bake` writes them ([`touches_gi_data`]).
//! - **The store** holds baked files by hash (`<hash>.lux`) and, by the bake's input key, which
//!   hash it gave (`input/<key>`): the world keeps only the hash, and an unchanged scene with
//!   unchanged settings is never baked twice.
//! - **Residency.** Each frame, the chunks (one per 32 m column of a volume's lattice, sharing
//!   their border probes) within `radius_m` of the window's camera and the last capture's are
//!   resident, nearest first, at most [`MAX_GI_VOLUMES`] and `memory_mb`; the GPU buffer is
//!   rebuilt only when that set changes.

use crate::world::{Entity, Shape, World};
use specula_core::hash::StableHasher;
use specula_core::math::{Mat4, Vec3};
use specula_core::{dmath, Json};
use specula_gpu::{GiFrame, GiQuality, GiView, GiVolume, Material, GI_PROBE_BYTES, MAX_GI_PROBES, MAX_GI_VOLUMES};
use specula_lux as lux;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Instant;

/// The group whose children are the volumes.
pub const GROUP: &str = "gi";
/// The props key of a volume's settings.
pub const PROP: &str = "gi_volume";
/// Volumes read from `/gi` at most.
pub const MAX_VOLUMES: usize = 64;
/// The baked files the store keeps at most (`<cache>/gi`).
pub const MAX_STORE_BYTES: u64 = 512 << 20;
/// Bakes in host memory at most (their fields for gi_check and their chunks for the GPU).
pub const MAX_HOST_BYTES: u64 = 256 << 20;
/// `gi_settings.memory_mb` at most: what the GPU side holds ([`MAX_GI_PROBES`] probes of
/// [`GI_PROBE_BYTES`]: the record and tier low's lean copy).
pub const MAX_MEMORY_MB: f32 = (MAX_GI_PROBES * GI_PROBE_BYTES / 1_048_576) as f32;
/// Excluded subtrees per volume at most.
pub const MAX_EXCLUDE: usize = 16;
/// A chunk's width in X and Z: two streaming cells (`streaming.rs`'s default `cell_m` is 32 m). Fewer,
/// bigger chunks keep the shader's volume search short (Sepulcra's top face is one chunk).
pub const CHUNK_M: f32 = 64.0;
/// Most entities a bake gathers (beyond: shrink the box or exclude subtrees).
pub const MAX_INSTANCES: usize = 65_536;
/// How long a capture's camera keeps its chunks resident.
const CAPTURE_KEEP_S: f64 = 5.0;

/// The runtime tier (`gi_settings`): engine-local, like the frame floor (it depends on the
/// machine, not the world). The cells' visibility masks apply in every tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    /// The flat ambient.
    Off,
    /// An ambient cube per draw (the CPU samples each face once), the lean probes per pixel on
    /// draws wider than two probe spacings and on terrain, no full sampler: the integrated GPU's
    /// default.
    Low,
    /// Per pixel everywhere: the default on other GPUs.
    High,
}

impl Quality {
    pub fn name(self) -> &'static str {
        match self {
            Quality::Off => "off",
            Quality::Low => "low",
            Quality::High => "high",
        }
    }

    pub fn parse(s: &str) -> Option<Quality> {
        match s {
            "off" => Some(Quality::Off),
            "low" => Some(Quality::Low),
            "high" => Some(Quality::High),
            _ => None,
        }
    }

    /// The GPU side's tier (off draws no GI at all; its pipelines would be low's).
    pub fn gpu(self) -> GiQuality {
        if self == Quality::High {
            GiQuality::High
        } else {
            GiQuality::Low
        }
    }

    /// The default for an adapter: low on integrated GPUs (the iGPU's budget), high elsewhere.
    pub fn default_for(integrated: bool) -> Quality {
        if integrated {
            Quality::Low
        } else {
            Quality::High
        }
    }

    /// How this tier samples, as data.
    pub fn describe(self) -> &'static str {
        match self {
            Quality::Off => "the flat ambient (no GI)",
            Quality::Low => "an ambient cube per draw (each face of its box sampled once on the CPU); draws wider than 2 probe spacings, terrain and water read the probes' lean copy per pixel with the cells' masks (no reaches, no backface weights)",
            Quality::High => "per pixel for every lit surface",
        }
    }
}

/// Measured scene-pass cost over `off` on the Intel iGPU: Sepulcra in play, `m3_exit.py --gi`
/// (waves 6 and 10, two alternating rounds each, a quiet machine, 2026-10-30; docs/design/GI.md,
/// Measurements): low 1.0 to 1.3 ms, high 4.8 to 5.3 ms.
pub const HIGH_COST_IGPU_MS: f64 = 5.2;
pub const LOW_COST_IGPU_MS: f64 = 1.2;

/// The warning `gi_settings` gives for a tier the adapter can't afford: `high` on an integrated
/// GPU costs [`HIGH_COST_IGPU_MS`] of its scene pass. No preset picks it there; a lead may,
/// knowingly.
pub fn tier_warning(q: Quality, integrated: bool) -> Option<Json> {
    (integrated && q == Quality::High).then(|| {
        Json::obj([
            ("code", Json::from("gi_tier_over_budget")),
            (
                "message",
                Json::from(format!(
                    "tier high samples every lit pixel: measured {HIGH_COST_IGPU_MS} ms of an integrated GPU's scene pass in Sepulcra, against {LOW_COST_IGPU_MS} ms for 'low', this adapter's default (which holds its frame budget)"
                )),
            ),
            ("cost_ms", Json::from(HIGH_COST_IGPU_MS)),
            ("low_cost_ms", Json::from(LOW_COST_IGPU_MS)),
        ])
    })
}

/// How a capture shows GI (`sight_frame {gi}`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptureView {
    /// As the window draws it.
    #[default]
    Lit,
    /// Without GI (the flat ambient): before/after pairs.
    Off,
    /// The indirect light alone.
    Irradiance,
    /// The indirect light alone, with the probes drawn as small spheres.
    Probes,
}

impl CaptureView {
    pub fn parse(s: &str) -> Result<CaptureView, String> {
        match s {
            "lit" => Ok(CaptureView::Lit),
            "off" => Ok(CaptureView::Off),
            "irradiance" => Ok(CaptureView::Irradiance),
            "probes" => Ok(CaptureView::Probes),
            other => Err(format!("'gi' must be lit, off, irradiance or probes, not '{other}'")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            CaptureView::Lit => "lit",
            CaptureView::Off => "off",
            CaptureView::Irradiance => "irradiance",
            CaptureView::Probes => "probes",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Settings and results (world data)

/// What `gi_bake` asks for; stored in `props.gi_volume`.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub spacing: f32,
    pub rays: u32,
    pub bounces: u32,
    /// Multiplies the volume's irradiance at runtime (no rebake).
    pub intensity: f32,
    /// The sky below the horizon, as a share of above.
    pub sky_below: f32,
    /// Geometry this far beyond the box still occludes and bounces (metres).
    pub margin: f32,
    /// Subtrees left out of the bake (paths).
    pub exclude: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { min: [0.0; 3], max: [0.0; 3], spacing: 1.0, rays: 256, bounces: 2, intensity: 1.0, sky_below: 1.0, margin: 8.0, exclude: Vec::new() }
    }
}

fn arr3(v: &Json, key: &str) -> Result<Option<[f32; 3]>, String> {
    match v.get(key).filter(|x| !x.is_null()) {
        None => Ok(None),
        Some(a) => {
            let xs: Option<Vec<f64>> = a.as_array().and_then(|a| a.iter().map(Json::as_f64).collect());
            match xs.as_deref() {
                Some(&[x, y, z]) if [x, y, z].iter().all(|c| c.is_finite() && c.abs() <= 1.0e6) => Ok(Some([x as f32, y as f32, z as f32])),
                _ => Err(format!("'{key}' must be [x, y, z] in metres")),
            }
        }
    }
}

fn num(v: &Json, key: &str, range: std::ops::RangeInclusive<f64>) -> Result<Option<f64>, String> {
    match v.get(key).filter(|x| !x.is_null()) {
        None => Ok(None),
        Some(x) => match x.as_f64() {
            Some(f) if range.contains(&f) => Ok(Some(f)),
            _ => Err(format!("'{key}' must be a number from {} to {}", range.start(), range.end())),
        },
    }
}

fn hex(h: u64) -> Json {
    Json::from(format!("{h:016x}"))
}

fn unhex(v: Option<&Json>) -> Option<u64> {
    v.and_then(Json::as_str).and_then(|s| u64::from_str_radix(s, 16).ok())
}

impl Settings {
    /// `base` with whatever `args` gives (gi_bake's fields), checked.
    pub fn patched(base: &Settings, args: &Json) -> Result<Settings, String> {
        let mut s = base.clone();
        if let Some(v) = arr3(args, "min")? {
            s.min = v;
        }
        if let Some(v) = arr3(args, "max")? {
            s.max = v;
        }
        if let Some(v) = num(args, "spacing", f64::from(lux::bake::MIN_SPACING)..=f64::from(lux::bake::MAX_SPACING))? {
            s.spacing = v as f32;
        }
        if let Some(v) = num(args, "rays", f64::from(lux::bake::MIN_RAYS)..=f64::from(lux::bake::MAX_RAYS))? {
            s.rays = v as u32;
        }
        if let Some(v) = num(args, "bounces", 0.0..=f64::from(lux::bake::MAX_BOUNCES))? {
            s.bounces = v as u32;
        }
        if let Some(v) = num(args, "intensity", 0.0..=16.0)? {
            s.intensity = v as f32;
        }
        if let Some(v) = num(args, "sky_below", 0.0..=1.0)? {
            s.sky_below = v as f32;
        }
        if let Some(v) = num(args, "margin", 0.0..=64.0)? {
            s.margin = v as f32;
        }
        if let Some(v) = args.get("exclude").filter(|x| !x.is_null()) {
            let list = v.as_array().filter(|l| l.len() <= MAX_EXCLUDE).ok_or(format!("'exclude' must be a list of at most {MAX_EXCLUDE} entity paths"))?;
            let mut out = Vec::new();
            for p in list {
                match p.as_str() {
                    Some(p) if p.starts_with('/') && p.len() <= 512 => out.push(p.to_owned()),
                    _ => return Err("'exclude' entries are entity paths (\"/units\")".into()),
                }
            }
            s.exclude = out;
        }
        for a in 0..3 {
            if s.max[a] < s.min[a] {
                return Err(format!("'max' must be at least 'min' on every axis ({:?} vs {:?})", s.max, s.min));
            }
        }
        Ok(s)
    }

    pub fn parse(v: &Json) -> Result<Settings, String> {
        let s = Settings::patched(&Settings::default(), v)?;
        if arr3(v, "min")?.is_none() || arr3(v, "max")?.is_none() {
            return Err("a GI volume needs 'min' and 'max'".into());
        }
        Ok(s)
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("min", Json::arr(self.min.map(Json::from))),
            ("max", Json::arr(self.max.map(Json::from))),
            ("spacing", Json::from(self.spacing)),
            ("rays", Json::from(self.rays)),
            ("bounces", Json::from(self.bounces)),
            ("intensity", Json::from(self.intensity)),
            ("sky_below", Json::from(self.sky_below)),
            ("margin", Json::from(self.margin)),
            ("exclude", Json::arr(self.exclude.iter().map(|p| Json::from(p.as_str())))),
        ])
    }

    /// What the bake depends on (not the runtime intensity).
    pub fn key(&self) -> u64 {
        let mut h = StableHasher::with_seed(0x4749_5345_5454_0001);
        for v in self.min.iter().chain(&self.max) {
            h.write_f32(*v);
        }
        h.write_f32(self.spacing);
        h.write_u32(self.rays);
        h.write_u32(self.bounces);
        h.write_f32(self.sky_below);
        h.write_f32(self.margin);
        for p in &self.exclude {
            h.write_str(p);
        }
        h.finish()
    }

    pub fn request(&self) -> lux::Request {
        lux::Request { min: self.min, max: self.max, spacing: self.spacing, rays: self.rays, bounces: self.bounces, clamp: 0.0 }
    }

    pub fn centre(&self) -> Vec3 {
        Vec3::new((self.min[0] + self.max[0]) * 0.5, (self.min[1] + self.max[1]) * 0.5, (self.min[2] + self.max[2]) * 0.5)
    }
}

/// What a bake saw, by part, so staleness can say what changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneKeys {
    pub geometry: u64,
    pub lights: u64,
    pub environment: u64,
}

impl SceneKeys {
    pub fn total(&self) -> u64 {
        let mut h = StableHasher::with_seed(0x4749_5343_454e_0001);
        h.write_u64(self.geometry);
        h.write_u64(self.lights);
        h.write_u64(self.environment);
        h.finish()
    }
}

/// A finished bake, as the world records it.
#[derive(Clone, Debug, PartialEq)]
pub struct BakedInfo {
    pub hash: u64,
    pub keys: SceneKeys,
    pub settings: u64,
    pub probes: usize,
    pub valid: usize,
    pub relocated: usize,
    pub invalid: usize,
    pub triangles: usize,
    pub lights: usize,
}

impl BakedInfo {
    /// What the world records of a bake: only what the bake determines (no timings: those are
    /// `gi_info`'s `last_job`), the same whether it ran now or landed from the store.
    pub fn of(hash: u64, keys: SceneKeys, settings: u64, counts: lux::file::Counts, probes: usize) -> BakedInfo {
        BakedInfo {
            hash,
            keys,
            settings,
            probes,
            valid: counts.valid as usize,
            relocated: counts.relocated as usize,
            invalid: counts.invalid as usize,
            triangles: counts.triangles as usize,
            lights: counts.lights as usize,
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("hash", hex(self.hash)),
            ("geometry", hex(self.keys.geometry)),
            ("lights_key", hex(self.keys.lights)),
            ("environment", hex(self.keys.environment)),
            ("settings", hex(self.settings)),
            ("probes", Json::from(self.probes)),
            ("valid", Json::from(self.valid)),
            ("relocated", Json::from(self.relocated)),
            ("invalid", Json::from(self.invalid)),
            ("triangles", Json::from(self.triangles)),
            ("lights", Json::from(self.lights)),
        ])
    }

    pub fn parse(v: &Json) -> Option<BakedInfo> {
        let n = |k: &str| v.get(k).and_then(Json::as_u64).unwrap_or(0) as usize;
        Some(BakedInfo {
            hash: unhex(v.get("hash"))?,
            keys: SceneKeys { geometry: unhex(v.get("geometry"))?, lights: unhex(v.get("lights_key"))?, environment: unhex(v.get("environment"))? },
            settings: unhex(v.get("settings"))?,
            probes: n("probes"),
            valid: n("valid"),
            relocated: n("relocated"),
            invalid: n("invalid"),
            triangles: n("triangles"),
            lights: n("lights"),
        })
    }
}

/// A volume as `/gi` holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    pub id: u64,
    pub version: u64,
    pub name: String,
    pub shown: bool,
    pub settings: Result<Settings, String>,
    pub baked: Option<BakedInfo>,
}

/// Parses a volume entity.
pub fn volume_of(e: &Entity) -> Volume {
    let prop = e.props.get(PROP);
    let settings = match prop {
        Some(p) => Settings::parse(p),
        None => Err(format!("no props.{PROP}")),
    };
    let baked = prop.and_then(|p| p.get("baked")).filter(|b| !b.is_null()).and_then(BakedInfo::parse);
    Volume { id: e.id, version: e.version, name: e.name.clone(), shown: e.shown(), settings, baked }
}

fn under_gi(p: &str) -> bool {
    p == "/gi" || p.starts_with("/gi/")
}

/// A change that writes `/gi` or GI volume settings anywhere: only `gi_bake` may make it
/// (`Rule::Gi`, the write policy's reserved data).
pub fn touches_gi_data(change: &crate::world::Change) -> bool {
    use crate::world::Change;
    let settings = |e: &Entity| e.props.get(PROP).is_some();
    match change {
        Change::Spawn { path, after } => under_gi(path) || settings(after),
        Change::Delete { path, before } => under_gi(path) || settings(before),
        Change::Set { path, prev_path, before, after } => under_gi(path) || prev_path.as_deref().is_some_and(under_gi) || settings(after) || before.as_deref().is_some_and(settings),
        Change::Pose { path, .. } => under_gi(path),
        Change::Env { .. } | Change::Camera { .. } | Change::File { .. } => false,
    }
}

// ---------------------------------------------------------------------------------------------
// The bake's input

fn srgb_to_linear(c: f32) -> f32 {
    let c = if c.is_finite() { c.clamp(0.0, 1.0) } else { 0.0 };
    if c <= 0.04045 {
        c / 12.92
    } else {
        dmath::pow((c + 0.055) / 1.055, 2.4)
    }
}

fn lin3(c: [f32; 3]) -> [f32; 3] {
    c.map(srgb_to_linear)
}

/// The geometry of one gathered entity.
pub enum Geo {
    Cube,
    Quad,
    Sphere,
    Lathe { points: Vec<[f32; 2]>, segments: u32 },
    Model { built: Arc<crate::models::Built> },
}

/// One entity the bake sees.
pub struct Instance {
    pub world: Mat4,
    /// sRGB colour (tint).
    pub color: [f32; 3],
    pub emissive: f32,
    /// Its emissive surfaces glow into the bake (not when it carries a point light).
    pub emits: bool,
    pub geo: Geo,
}

/// Everything a bake needs, gathered on the engine thread.
pub struct Input {
    pub instances: Vec<Instance>,
    pub lights: Vec<lux::PointLight>,
    /// Each light's entity path (for warnings).
    pub light_names: Vec<String>,
    pub sun: Option<lux::Sun>,
    pub sky: lux::Sky,
    pub keys: SceneKeys,
    /// Model entities in the box still building (their geometry would be missing).
    pub building: Vec<String>,
    /// The most luminance one probe ray brings ([`firefly_clamp`]).
    pub clamp: f32,
}

/// The per-ray luminance cap of a bake: 16 times the sky's, at least 0.25. Indirect light seldom
/// needs more, and a small bright emitter that only a few rays hit can't make blotches.
pub fn firefly_clamp(sky: &lux::Sky) -> f32 {
    let r = sky.radiance;
    (16.0 * (0.2126 * r[0] + 0.7152 * r[1] + 0.0722 * r[2])).max(0.25)
}

/// Why a bake can't start.
pub struct GatherError {
    pub code: &'static str,
    pub message: String,
}

fn overlaps(lo: Vec3, hi: Vec3, bmin: [f32; 3], bmax: [f32; 3]) -> bool {
    lo.x <= bmax[0] && hi.x >= bmin[0] && lo.y <= bmax[1] && hi.y >= bmin[1] && lo.z <= bmax[2] && hi.z >= bmin[2]
}

fn hash_mat(h: &mut StableHasher, m: &Mat4) {
    for c in &m.cols {
        for v in c {
            h.write_f32(*v);
        }
    }
}

/// Excluded subtrees as entity ids (paths that don't resolve are ignored).
fn excluded_ids(world: &World, s: &Settings) -> BTreeSet<u64> {
    s.exclude.iter().filter_map(|p| world.find_path(p).ok()).collect()
}

fn excluded(world: &World, e: &Entity, ex: &BTreeSet<u64>) -> bool {
    if ex.is_empty() {
        return false;
    }
    let mut at = Some(e);
    let mut depth = 0;
    while let Some(x) = at {
        if ex.contains(&x.id) {
            return true;
        }
        depth += 1;
        if depth > 256 {
            break;
        }
        at = x.parent.and_then(|p| world.get(p));
    }
    false
}

/// Whether `e` takes part in a bake as geometry: shown, persistent, 3D, opaque lit, still.
fn bakes(e: &Entity) -> bool {
    e.shown()
        && !e.in_ui()
        && !e.transient
        && e.anim.is_none()
        && matches!(e.shape, Shape::Cube | Shape::Quad | Shape::Sphere | Shape::Lathe | Shape::Model)
        && e.look.material == Material::Lit
        && !e.body.as_ref().is_some_and(|b| b.kind != crate::physics::BodyKind::Static)
}

/// The scene keys of `s`'s box (what [`gather`] would see), without gathering geometry.
pub fn scene_keys(world: &World, lib: &crate::models::Library, s: &Settings) -> SceneKeys {
    let (bmin, bmax) = grown(s);
    let ex = excluded_ids(world, s);
    let mut k = KeyHashers::new();
    for e in world.entities() {
        if bakes(e) {
            let (lo, hi) = e.world_aabb();
            if overlaps(lo, hi, bmin, bmax) && !excluded(world, e, &ex) {
                k.geometry(e, lib);
            }
        }
        if let Some(pl) = lit(e, bmin, bmax) {
            k.light(e, &pl);
        }
    }
    k.finish(world, s)
}

/// The scene keys' hashers ([`scene_keys`]; [`gather`] fills them in its own pass).
struct KeyHashers {
    g: StableHasher,
    l: StableHasher,
}

impl KeyHashers {
    fn new() -> KeyHashers {
        KeyHashers { g: StableHasher::with_seed(0x4749_4745_4f4d_0001), l: StableHasher::with_seed(0x4749_4c49_4748_0001) }
    }

    fn geometry(&mut self, e: &Entity, lib: &crate::models::Library) {
        self.g.write_u64(e.id);
        self.g.write_u64(e.content_hash());
        hash_mat(&mut self.g, &e.model());
        match e.resolved_model().and_then(|r| lib.built(r.handle)) {
            Some(b) => self.g.write_u64(b.content),
            None => self.g.write_u64(0),
        }
    }

    fn light(&mut self, e: &Entity, pl: &lux::PointLight) {
        self.l.write_u64(e.id);
        for v in pl.position.iter().chain(&pl.color) {
            self.l.write_f32(*v);
        }
        self.l.write_f32(pl.range);
    }

    fn finish(self, world: &World, s: &Settings) -> SceneKeys {
        SceneKeys { geometry: self.g.finish(), lights: self.l.finish(), environment: env_key(world, s) }
    }
}

fn grown(s: &Settings) -> ([f32; 3], [f32; 3]) {
    let m = s.margin;
    (s.min.map(|v| v - m), s.max.map(|v| v + m))
}

/// `e`'s point light as the bake sees it (linear), if it reaches the box.
fn lit(e: &Entity, bmin: [f32; 3], bmax: [f32; 3]) -> Option<lux::PointLight> {
    if !e.shown() || e.in_ui() || e.transient {
        return None;
    }
    let l = e.world_light()?;
    let r = l.range.max(0.0);
    let p = Vec3::new(l.position[0], l.position[1], l.position[2]);
    if !overlaps(p - Vec3::new(r, r, r), p + Vec3::new(r, r, r), bmin, bmax) {
        return None;
    }
    let c = lin3(l.color);
    let k = l.intensity.max(0.0);
    Some(lux::PointLight { position: l.position, color: [c[0] * k, c[1] * k, c[2] * k], range: r })
}

fn env_parts(world: &World, s: &Settings) -> (Option<lux::Sun>, lux::Sky) {
    let env = &world.env().light;
    let sun = (env.sun_intensity > 0.0).then(|| {
        let c = lin3(env.sun_color);
        let k = env.sun_intensity;
        lux::Sun { direction: env.sun_direction, color: [c[0] * k, c[1] * k, c[2] * k] }
    });
    let a = lin3(env.ambient_color);
    let k = env.ambient_intensity.max(0.0);
    (sun, lux::Sky { radiance: [a[0] * k, a[1] * k, a[2] * k], below: s.sky_below })
}

fn env_key(world: &World, s: &Settings) -> u64 {
    let (sun, sky) = env_parts(world, s);
    let mut h = StableHasher::with_seed(0x4749_454e_5600_0001);
    if let Some(sun) = sun {
        for v in sun.direction.iter().chain(&sun.color) {
            h.write_f32(*v);
        }
    }
    for v in &sky.radiance {
        h.write_f32(*v);
    }
    h.write_f32(sky.below);
    h.finish()
}

/// Gathers `s`'s box from the world (bounded: O(entities), at most [`MAX_INSTANCES`]).
pub fn gather(world: &World, lib: &crate::models::Library, s: &Settings) -> Result<Input, GatherError> {
    let (bmin, bmax) = grown(s);
    let ex = excluded_ids(world, s);
    let mut instances = Vec::new();
    let mut lights = Vec::new();
    let mut light_names = Vec::new();
    let mut building = Vec::new();
    // Triangles counted as gathered: an over-limit box is refused before anything is built (the
    // list itself is made on the bake's worker).
    let mut tris = 0usize;
    // One pass over the world: the scene keys are hashed as the instances are gathered (two
    // passes cost the engine thread twice; the T3 lane measured gi_bake stalling it 7 to 11 ms,
    // most of it reading a baked file from the store, now read on the bake's worker).
    let mut keys = KeyHashers::new();
    for e in world.entities() {
        if let Some(pl) = lit(e, bmin, bmax) {
            keys.light(e, &pl);
            lights.push(pl);
            light_names.push(world.path_of(e.id));
        }
        if !bakes(e) {
            continue;
        }
        let (lo, hi) = e.world_aabb();
        if !overlaps(lo, hi, bmin, bmax) || excluded(world, e, &ex) {
            continue;
        }
        keys.geometry(e, lib);
        let geo = match e.shape {
            Shape::Cube => Geo::Cube,
            Shape::Quad => Geo::Quad,
            Shape::Sphere => Geo::Sphere,
            Shape::Lathe => match &e.profile {
                Some(p) => Geo::Lathe { points: p.points.clone(), segments: p.segments.clamp(3, 64) },
                None => continue,
            },
            Shape::Model => match e.resolved_model().and_then(|r| lib.built(r.handle)) {
                Some(b) => Geo::Model { built: Arc::clone(b) },
                None => {
                    if building.len() < 8 {
                        building.push(world.path_of(e.id));
                    }
                    continue;
                }
            },
            _ => continue,
        };
        if instances.len() >= MAX_INSTANCES {
            return Err(GatherError {
                code: "over_limit",
                message: format!("more than {MAX_INSTANCES} entities in the box (and its margin): shrink it, lower 'margin' or 'exclude' subtrees"),
            });
        }
        tris += geo_triangles(&geo);
        if tris > lux::bake::MAX_TRIANGLES {
            return Err(GatherError {
                code: "over_limit",
                message: format!("more than {} triangles in the box (and its margin): shrink it, lower 'margin' or 'exclude' subtrees", lux::bake::MAX_TRIANGLES),
            });
        }
        // An entity with a point light: the light stands for its glow (a brazier's bowl and its
        // fire light would count twice, and the bowl is too small for the rays to find evenly).
        let emits = e.light.is_none();
        instances.push(Instance { world: e.model(), color: e.color, emissive: if emits { e.look.surface.emissive } else { 0.0 }, emits, geo });
    }
    let (sun, sky) = env_parts(world, s);
    let clamp = firefly_clamp(&sky);
    Ok(Input { instances, lights, light_names, sun, sky, keys: keys.finish(world, s), building, clamp })
}

/// The cofactor (normal) matrix of `m`'s 3x3 part, and whether it mirrors.
fn cofactor(m: &Mat4) -> ([Vec3; 3], f32) {
    let c = &m.cols;
    let (a, b, cc) = (Vec3::new(c[0][0], c[0][1], c[0][2]), Vec3::new(c[1][0], c[1][1], c[1][2]), Vec3::new(c[2][0], c[2][1], c[2][2]));
    let cof = [b.cross(cc), cc.cross(a), a.cross(b)];
    (cof, if a.dot(cof[0]) < 0.0 { -1.0 } else { 1.0 })
}

fn push_mesh(out: &mut Vec<lux::Triangle>, world: &Mat4, positions: &[[f32; 3]], normals: &[[f32; 3]], albedo: &dyn Fn(usize) -> [f32; 3], emissive: &dyn Fn([f32; 3]) -> [f32; 3], indices: &[u32]) {
    let (cof, flip) = cofactor(world);
    for t in indices.as_chunks::<3>().0 {
        let (i, j, k) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if i >= positions.len() || j >= positions.len() || k >= positions.len() {
            continue;
        }
        let p = [i, j, k].map(|x| {
            let v = world.transform_point(Vec3::new(positions[x][0], positions[x][1], positions[x][2]));
            [v.x, v.y, v.z]
        });
        let n = [i, j, k].iter().fold(Vec3::ZERO, |acc, &x| {
            let n = normals.get(x).copied().unwrap_or([0.0, 1.0, 0.0]);
            acc + (cof[0] * n[0] + cof[1] * n[1] + cof[2] * n[2]) * flip
        });
        let (a0, a1, a2) = (albedo(i), albedo(j), albedo(k));
        let a = [(a0[0] + a1[0] + a2[0]) / 3.0, (a0[1] + a1[1] + a2[1]) / 3.0, (a0[2] + a1[2] + a2[2]) / 3.0];
        out.push(lux::Triangle { p, normal: [n.x, n.y, n.z], albedo: a, emissive: emissive(a) });
    }
}

/// Unit cube (as `meshes::cube`), quad, and spheres and lathes tessellated with `dmath` (the
/// same triangles on every platform).
fn unit_mesh(geo: &Geo) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    match geo {
        Geo::Cube => {
            let (v, i) = crate::meshes::cube();
            (v.iter().map(|v| v.position).collect(), v.iter().map(|v| v.normal).collect(), i)
        }
        Geo::Quad => {
            let (v, i) = crate::meshes::quad();
            (v.iter().map(|v| v.position).collect(), v.iter().map(|v| v.normal).collect(), i)
        }
        Geo::Sphere => {
            let (seg, rings) = (16u32, 8u32);
            let mut p = Vec::new();
            for r in 0..=rings {
                let (st, ct) = dmath::sin_cos(std::f32::consts::PI * r as f32 / rings as f32);
                for s in 0..=seg {
                    let (sp, cp) = dmath::sin_cos(std::f32::consts::TAU * s as f32 / seg as f32);
                    p.push([0.5 * st * cp, 0.5 * ct, 0.5 * st * sp]);
                }
            }
            let mut idx = Vec::new();
            for r in 0..rings {
                for s in 0..seg {
                    let a = r * (seg + 1) + s;
                    let b = a + seg + 1;
                    idx.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
                }
            }
            let n = p.iter().map(|q| [q[0] * 2.0, q[1] * 2.0, q[2] * 2.0]).collect();
            (p, n, idx)
        }
        Geo::Lathe { points, segments } => {
            let mut p = Vec::new();
            let mut n = Vec::new();
            let mut idx = Vec::new();
            let seg = *segments;
            for w in points.windows(2) {
                let (a, b) = (w[0], w[1]);
                let (dr, dy) = (b[0] - a[0], b[1] - a[1]);
                let len = (dr * dr + dy * dy).sqrt();
                if len <= 1e-7 {
                    continue;
                }
                let nn = [dy / len, -dr / len];
                let base = p.len() as u32;
                for s in 0..=seg {
                    let (sn, cs) = dmath::sin_cos(std::f32::consts::TAU * s as f32 / seg as f32);
                    p.push([a[0] * cs, a[1], a[0] * sn]);
                    p.push([b[0] * cs, b[1], b[0] * sn]);
                    n.push([nn[0] * cs, nn[1], nn[0] * sn]);
                    n.push([nn[0] * cs, nn[1], nn[0] * sn]);
                }
                for s in 0..seg {
                    let k = base + 2 * s;
                    idx.extend_from_slice(&[k, k + 2, k + 1, k + 1, k + 2, k + 3]);
                }
            }
            (p, n, idx)
        }
        Geo::Model { .. } => (Vec::new(), Vec::new(), Vec::new()),
    }
}

/// The triangles [`triangles`] makes for `geo`, counted without building them.
fn geo_triangles(geo: &Geo) -> usize {
    match geo {
        Geo::Cube => 12,
        Geo::Quad => 2,
        Geo::Sphere => 16 * 8 * 2,
        Geo::Lathe { points, segments } => {
            let rings = points.windows(2).filter(|w| {
                let (dr, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
                (dr * dr + dy * dy).sqrt() > 1e-7
            });
            rings.count() * *segments as usize * 2
        }
        Geo::Model { built } => {
            let model = &built.model;
            let level = model.lods.len().saturating_sub(1).min(1);
            let Some(lod) = model.lods.get(level) else { return 0 };
            (0..lod.parts.len()).map(|pi| lod.part_indices(pi).len() / 3).sum()
        }
    }
}

/// The triangles of `input` (on the bake's worker: the heavy part of gathering).
pub fn triangles(input: &Input) -> Vec<lux::Triangle> {
    let mut out = Vec::new();
    for inst in &input.instances {
        let tint = lin3(inst.color);
        match &inst.geo {
            Geo::Model { built } => {
                let model = &built.model;
                // LOD 1 when there is one: fewer triangles, the same light.
                let level = model.lods.len().saturating_sub(1).min(1);
                let Some(lod) = model.lods.get(level) else { continue };
                for (pi, part) in lod.parts.iter().enumerate() {
                    let preset = model.materials.iter().find(|(n, _)| *n == part.material).map(|(_, p)| p);
                    if preset.is_some_and(|p| p.kind == specula_model::MaterialKind::Glass) {
                        continue; // glass neither blocks nor emits in the bake
                    }
                    let gl = built.materials.get(&part.material);
                    let factor = gl.map_or([1.0; 3], |m| lin3([m.color[0], m.color[1], m.color[2]]));
                    let glow = if inst.emits { preset.map_or(0.0, |p| p.emissive) } else { 0.0 };
                    let gl_emit = gl.filter(|m| inst.emits && m.emissive_strength > 0.0).map(|m| {
                        let c = lin3(m.emissive_color);
                        [c[0] * m.emissive_strength, c[1] * m.emissive_strength, c[2] * m.emissive_strength]
                    });
                    let colors = &lod.colors;
                    let albedo = |i: usize| {
                        let vc = colors.get(i).copied().unwrap_or([255; 4]);
                        [0, 1, 2].map(|k| srgb_to_linear(f32::from(vc[k]) / 255.0) * tint[k] * factor[k])
                    };
                    let emissive = |a: [f32; 3]| match gl_emit {
                        Some(e) => e,
                        None => a.map(|x| x * glow),
                    };
                    push_mesh(&mut out, &inst.world, &lod.positions, &lod.normals, &albedo, &emissive, lod.part_indices(pi));
                }
            }
            geo => {
                let (p, n, idx) = unit_mesh(geo);
                let glow = inst.emissive.max(0.0);
                push_mesh(&mut out, &inst.world, &p, &n, &|_| tint, &|a: [f32; 3]| a.map(|x| x * glow), &idx);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The store

/// Baked files by hash, and which hash each bake input gave.
#[derive(Clone, Debug)]
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn file(&self, hash: u64) -> PathBuf {
        self.dir.join(format!("{hash:016x}.lux"))
    }

    fn input_file(&self, input: u64) -> PathBuf {
        self.dir.join("input").join(format!("{input:016x}"))
    }

    /// The hash an earlier bake of the same input gave, if its file is still there.
    pub fn by_input(&self, input: u64) -> Option<u64> {
        let h = std::fs::read_to_string(self.input_file(input)).ok().and_then(|s| u64::from_str_radix(s.trim(), 16).ok())?;
        self.file(h).exists().then_some(h)
    }

    /// Writes a baked file and its input's entry through the disk budget (`disk::book()`, the cache
    /// category: admitted, accounted, refused below the free-space floor), then keeps the store
    /// within [`MAX_STORE_BYTES`] (oldest files first; a volume whose file went reports `missing`
    /// and bakes again). Files are content-addressed and checked by hash on reading, so a write cut
    /// short reads as corrupt and bakes again.
    pub fn put(&self, input: u64, hash: u64, bytes: &[u8]) -> Result<(), String> {
        let book = crate::disk::book();
        let (cat, need) = (crate::disk::Category::Cache, crate::disk::Need::Agent);
        let path = self.file(hash);
        if !path.exists() {
            book.write(cat, "gi", &path, bytes, need).map_err(|e| e.to_json().to_string())?;
        }
        book.write(cat, "gi", &self.input_file(input), format!("{hash:016x}\n").as_bytes(), need).map_err(|e| e.to_json().to_string())?;
        self.trim(hash, MAX_STORE_BYTES);
        Ok(())
    }

    /// Deletes the oldest baked files (never `keep`) until the store holds at most `max` bytes,
    /// inside the store's folder only and accounted with the disk budget. Input entries naming a
    /// deleted file are ignored by [`Store::by_input`].
    pub fn trim(&self, keep: u64, max: u64) {
        let Ok(dir) = std::fs::read_dir(&self.dir) else { return };
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = dir
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "lux") && e.path() != self.file(keep))
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                Some((m.modified().ok()?, m.len(), e.path()))
            })
            .collect();
        let kept = std::fs::metadata(self.file(keep)).map_or(0, |m| m.len());
        let mut total: u64 = kept + files.iter().map(|f| f.1).sum::<u64>();
        files.sort();
        // Removed by their place under the store's canonical folder (no links followed).
        let root = crate::disk::canonical(&self.dir);
        for (_, _, path) in files {
            if total <= max {
                break;
            }
            let Some(name) = path.file_name() else { continue };
            if let Ok(len) = crate::disk::remove_file_inside(&root, &root.join(name)) {
                crate::disk::book().wrote(crate::disk::Category::Cache, "gi", -(len as i64), -1);
                total -= len;
            }
        }
    }

    pub fn read(&self, hash: u64) -> Result<lux::file::Decoded, String> {
        let bytes = std::fs::read(self.file(hash)).map_err(|e| format!("missing: {e}"))?;
        let d = lux::file::decode(&bytes).map_err(|e| e.to_string())?;
        if d.hash != hash {
            return Err(format!("corrupt: the file holds {:016x}", d.hash));
        }
        Ok(d)
    }
}

/// What the bake does (gathering, emission, the clamp, the masks): bump it when a change would bake
/// the same input differently, so the store never hands back an old result. 5: octants whose
/// centre sits in a solid take their open side's probes (the T3 lane's black cells).
pub const BAKE_VERSION: u32 = 5;

/// The counts a bake's file keeps (and the world records: [`BakedInfo::of`]).
pub fn counts_of(s: &lux::Stats) -> lux::file::Counts {
    lux::file::Counts { valid: s.valid as u32, relocated: s.relocated as u32, invalid: s.invalid as u32, triangles: s.triangles as u32, lights: s.lights as u32 }
}

/// The input key of a bake: settings, scene and [`BAKE_VERSION`].
pub fn input_key(settings: &Settings, keys: &SceneKeys) -> u64 {
    let mut h = StableHasher::with_seed(0x4749_494e_5055_0001);
    h.write_u64(settings.key());
    h.write_u64(keys.total());
    h.write_u32(BAKE_VERSION);
    h.finish()
}

// ---------------------------------------------------------------------------------------------
// Jobs

/// What a finished job hands back.
pub struct Done {
    pub hash: u64,
    pub stats: lux::Stats,
    pub cached: bool,
    /// The scene's point lights (indices into the input's) whose centres are inside a solid: they
    /// light nothing in the bake (a lantern's flame inside its own glass mesh).
    pub lights_inside: Vec<usize>,
}

/// A bake running on its worker.
pub struct Job {
    pub volume: String,
    pub by: String,
    pub key: String,
    pub settings: Settings,
    pub keys: SceneKeys,
    pub plan: lux::Plan,
    pub control: Arc<lux::Control>,
    pub started: Instant,
    pub threads: usize,
    /// The input's lights' entity paths (the result's `lights_inside` index them).
    pub light_names: Vec<String>,
    result: Receiver<Result<Done, lux::BakeError>>,
}

/// The last job's outcome, for gi_info.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub json: Json,
}

impl Job {
    /// Starts baking `input` on a worker thread (it writes the file into `store` itself).
    #[allow(clippy::too_many_arguments)]
    pub fn start(volume: String, by: String, key: String, settings: Settings, input: Input, plan: lux::Plan, store: Store, threads: usize) -> Result<Job, String> {
        let control = Arc::new(lux::Control::default());
        let (tx, rx) = mpsc::channel();
        let mut req = settings.request();
        req.clamp = input.clamp;
        let keys = input.keys;
        let ikey = input_key(&settings, &keys);
        let light_names = input.light_names.clone();
        let c = Arc::clone(&control);
        std::thread::Builder::new()
            .name("specula-gi".into())
            .spawn(move || {
                let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // Baked before: the store's file (read and checked here, not on the engine
                    // thread, where decoding it stalled frames).
                    if let Some(hash) = store.by_input(ikey) {
                        if let Ok(decoded) = store.read(hash) {
                            let c = decoded.counts;
                            let stats = lux::Stats {
                                probes: decoded.field.probes.len(),
                                valid: c.valid as usize,
                                relocated: c.relocated as usize,
                                invalid: c.invalid as usize,
                                triangles: c.triangles as usize,
                                lights: c.lights as usize,
                                ..lux::Stats::default()
                            };
                            return Ok(Done { hash, stats, cached: true, lights_inside: Vec::new() });
                        }
                    }
                    let scene = lux::Scene { triangles: triangles(&input), lights: input.lights.clone(), sun: input.sun, sky: Some(input.sky) };
                    drop(input);
                    let baked = lux::bake(&scene, &req, threads, &c)?;
                    let counts = counts_of(&baked.stats);
                    let (bytes, hash) = lux::file::encode(&baked.field, lux::file::Keys { scene: keys.total(), settings: settings_key(&req) }, counts);
                    store.put(ikey, hash, &bytes).map_err(|e| lux::BakeError { code: "store_failed", message: format!("could not write {}: {e}", store.file(hash).display()), field: None, value: None, limit: None })?;
                    Ok(Done { hash, stats: baked.stats, cached: false, lights_inside: baked.lights_inside })
                }));
                let r = run.unwrap_or_else(|_| Err(lux::BakeError { code: "panicked", message: "the bake panicked".into(), field: None, value: None, limit: None }));
                let _ = tx.send(r);
            })
            .map_err(|e| format!("could not start the bake's thread: {e}"))?;
        Ok(Job { volume, by, key, settings, keys, plan, control, started: Instant::now(), threads, light_names, result: rx })
    }

    /// The result, once there is one.
    pub fn poll(&self) -> Option<Result<Done, lux::BakeError>> {
        match self.result.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(lux::BakeError { code: "worker_gone", message: "the bake's thread ended without a result".into(), field: None, value: None, limit: None })),
        }
    }

    pub fn progress_json(&self) -> Json {
        let p = self.control.progress();
        Json::obj([
            ("volume", Json::from(self.volume.as_str())),
            ("by", Json::from(self.by.as_str())),
            ("phase", Json::from(p.phase.name())),
            ("bounce", Json::from(p.bounce)),
            ("done", Json::from(p.done)),
            ("total", Json::from(p.total)),
            ("elapsed_ms", Json::from(self.started.elapsed().as_millis() as u64)),
            ("probes", Json::from(self.plan.lattice.count())),
            ("work", Json::from(self.plan.work)),
            ("threads", Json::from(self.threads)),
            ("cancelled", Json::from(self.control.cancelled())),
        ])
    }
}

fn settings_key(r: &lux::Request) -> u64 {
    let mut h = StableHasher::with_seed(0x4749_5245_5155_0001);
    for v in r.min.iter().chain(&r.max) {
        h.write_f32(*v);
    }
    h.write_f32(r.spacing);
    h.write_u32(r.rays);
    h.write_u32(r.bounces);
    h.finish()
}

/// Bake errors as tool data.
pub fn bake_error_json(e: &lux::BakeError) -> Json {
    let mut j = Json::obj([("error", Json::from(e.code)), ("message", Json::from(e.message.as_str()))]);
    if let Some(f) = e.field {
        j.set("field", Json::from(f));
    }
    if let Some(v) = e.value {
        j.set("value", Json::from(v));
    }
    if let Some(l) = e.limit {
        j.set("limit", Json::from(l));
    }
    j
}

// ---------------------------------------------------------------------------------------------
// Residency

/// One streamed chunk of a loaded bake.
pub struct Chunk {
    pub origin: [f32; 3],
    pub spacing: f32,
    pub dims: [u32; 3],
    /// Shared, so a capture can hold the frame's chunks without borrowing the engine.
    pub records: Arc<[lux::Record]>,
    /// The XZ box its probes span.
    pub lo: [f32; 2],
    pub hi: [f32; 2],
}

/// A bake in memory: its field (gi_check samples it) and its chunks.
pub struct Loaded {
    pub field: lux::Field,
    pub chunks: Vec<Chunk>,
    /// Bytes its chunks take on the GPU.
    pub bytes: u64,
    /// Bytes it holds in host memory (field and chunks).
    pub host_bytes: u64,
}

impl Loaded {
    /// Host bytes a bake whose file has `file_len` bytes takes once loaded.
    pub fn host_bytes_for(file_len: u64) -> u64 {
        let probes = file_len.saturating_sub((lux::file::HEADER + 8) as u64) / 48;
        probes * (48 + std::mem::size_of::<lux::Probe>() as u64)
    }

    pub fn of(d: lux::file::Decoded) -> Loaded {
        let l = d.field.lattice;
        let per = ((CHUNK_M / l.spacing) as u32).max(1);
        let mut chunks = Vec::new();
        let mut x0 = 0;
        while x0 + 1 < l.dims[0] {
            let x1 = (x0 + per).min(l.dims[0] - 1);
            let mut z0 = 0;
            while z0 + 1 < l.dims[2] {
                let z1 = (z0 + per).min(l.dims[2] - 1);
                let f = d.field.chunk((x0, x1), (z0, z1));
                let fl = f.lattice;
                let (lo, hi) = fl.bounds();
                chunks.push(Chunk { origin: fl.origin, spacing: fl.spacing, dims: fl.dims, records: f.pack().into(), lo: [lo[0], lo[2]], hi: [hi[0], hi[2]] });
                z0 = z1;
            }
            x0 = x1;
        }
        let bytes: u64 = chunks.iter().map(|c| c.records.len() as u64 * 48).sum();
        let host_bytes = bytes + d.field.probes.len() as u64 * std::mem::size_of::<lux::Probe>() as u64;
        Loaded { field: d.field, chunks, bytes, host_bytes }
    }
}

/// A volume as the frame sees it.
struct Live {
    vol: Volume,
    /// Loaded bake (by hash), if its file was found.
    hash: Option<u64>,
    problem: Option<String>,
}

/// Baked GI in the engine.
pub struct Gi {
    pub store: Store,
    pub quality: Quality,
    pub radius_m: f32,
    pub memory_mb: f32,
    pub job: Option<Job>,
    pub last: Option<Outcome>,
    /// What captures show (set around one capture by `sight_frame`).
    pub capture_view: CaptureView,
    /// The last `probes` capture's spheres: (drawn, invalid probes left out).
    pub marks_drawn: (usize, usize),
    loaded: BTreeMap<u64, Loaded>,
    /// Bakes loading on a worker, results coming back, and loads that failed (by hash).
    loading: BTreeSet<u64>,
    loads_tx: mpsc::Sender<(u64, Result<Loaded, String>)>,
    loads_rx: Receiver<(u64, Result<Loaded, String>)>,
    failed_loads: BTreeMap<u64, String>,
    /// A load finished: re-link the volumes on the next scan.
    relink: bool,
    live: Vec<Live>,
    /// `/gi`'s children last scanned: (id, version, shown).
    scanned: Vec<(u64, u64, bool)>,
    /// Resident chunks: (bake hash, chunk index, intensity bits), nearest first.
    resident: Vec<(u64, usize, u32)>,
    resident_key: u64,
    /// The last capture's eye and when.
    capture_eye: Option<(Vec3, Instant)>,
    /// What the resident set was last worked out for: the window's eye, whether a capture's eye
    /// counted; `dirty` when volumes, files or settings changed since.
    last_eye: Option<(Vec3, bool)>,
    pub dirty: bool,
    /// Who cancelled the running job (it isn't committed even if it finished meanwhile).
    pub cancelled_by: Option<String>,
    /// Lights a bake found inside a solid, by bake hash (a bake from the store reports its
    /// original's), and the warnings of each volume's last bake (gi_check repeats them).
    pub lights_inside: BTreeMap<u64, Vec<String>>,
    pub warnings: BTreeMap<String, Vec<Json>>,
    /// The window's last eye (a capture's residency keeps the window's chunks too).
    window_eye: Option<Vec3>,
    /// Staleness by volume id, cached for a world revision.
    stale: BTreeMap<u64, (u64, Vec<&'static str>)>,
    pub rebuilds: u64,
}

impl Gi {
    pub fn new(dir: PathBuf) -> Gi {
        let (loads_tx, loads_rx) = mpsc::channel();
        Gi {
            store: Store { dir },
            quality: Quality::High,
            radius_m: 96.0,
            memory_mb: 16.0,
            job: None,
            last: None,
            capture_view: CaptureView::Lit,
            marks_drawn: (0, 0),
            loaded: BTreeMap::new(),
            loading: BTreeSet::new(),
            loads_tx,
            loads_rx,
            failed_loads: BTreeMap::new(),
            relink: false,
            live: Vec::new(),
            scanned: Vec::new(),
            resident: Vec::new(),
            resident_key: 0,
            capture_eye: None,
            last_eye: None,
            dirty: true,
            cancelled_by: None,
            lights_inside: BTreeMap::new(),
            warnings: BTreeMap::new(),
            window_eye: None,
            stale: BTreeMap::new(),
            rebuilds: 0,
        }
    }

    /// Re-reads `/gi`'s children when any changed (versions); baked files load on a worker
    /// (never on the frame thread), within [`MAX_HOST_BYTES`]. Unchanged, it allocates nothing.
    fn scan(&mut self, world: &World) {
        self.take_loads();
        let group = world.find_path("/gi").ok();
        let mut same = true;
        let mut n = 0;
        if let Some(g) = group {
            for id in world.children_of(Some(g)).take(MAX_VOLUMES) {
                let Some(e) = world.get(id) else { continue };
                if self.scanned.get(n) != Some(&(id, e.version, e.shown())) {
                    same = false;
                    break;
                }
                n += 1;
            }
        }
        if same && n == self.scanned.len() && !self.relink {
            return;
        }
        self.relink = false;
        self.scanned.clear();
        self.dirty = true;
        self.live.clear();
        let mut wanted = BTreeSet::new();
        if let Some(g) = group {
            for id in world.children_of(Some(g)).take(MAX_VOLUMES) {
                let Some(e) = world.get(id) else { continue };
                self.scanned.push((id, e.version, e.shown()));
                let vol = volume_of(e);
                let mut live = Live { hash: None, problem: None, vol };
                if let Some(b) = &live.vol.baked {
                    wanted.insert(b.hash);
                    if self.loaded.contains_key(&b.hash) {
                        live.hash = Some(b.hash);
                    } else if let Some(p) = self.failed_loads.get(&b.hash) {
                        live.problem = Some(p.clone());
                    } else {
                        live.problem = Some("loading".into());
                        self.request_load(b.hash);
                    }
                }
                self.live.push(live);
            }
        }
        // Bakes no volume names any more are dropped (an undo brings them back from the store).
        self.loaded.retain(|h, _| wanted.contains(h));
        self.failed_loads.retain(|h, _| wanted.contains(h));
    }

    /// Starts loading `hash` on a worker (once), if the host budget allows.
    fn request_load(&mut self, hash: u64) {
        if self.loading.contains(&hash) {
            return;
        }
        let used: u64 = self.loaded.values().map(|l| l.host_bytes).sum();
        let need = std::fs::metadata(self.store.file(hash)).map_or(0, |m| Loaded::host_bytes_for(m.len()));
        if used + need > MAX_HOST_BYTES {
            self.failed_loads.insert(hash, format!("host_budget: {} MB of bakes are loaded already (at most {} MB): remove or merge volumes", used >> 20, MAX_HOST_BYTES >> 20));
            return;
        }
        self.loading.insert(hash);
        let (store, tx) = (self.store.clone(), self.loads_tx.clone());
        let spawned = std::thread::Builder::new().name("specula-gi-load".into()).spawn(move || {
            let _ = tx.send((hash, store.read(hash).map(Loaded::of)));
        });
        if spawned.is_err() {
            self.loading.remove(&hash);
            self.failed_loads.insert(hash, "could not start a loader thread".into());
        }
    }

    /// Takes finished loads (no allocation when there are none).
    fn take_loads(&mut self) {
        while let Ok((hash, r)) = self.loads_rx.try_recv() {
            self.loading.remove(&hash);
            match r {
                Ok(l) => {
                    self.loaded.insert(hash, l);
                }
                Err(e) => {
                    self.failed_loads.insert(hash, e);
                }
            }
            self.relink = true;
        }
    }

    /// Waits (at most `timeout`) for running loads: captures show what a bake landed.
    pub fn wait_loads(&mut self, world: &World, timeout: std::time::Duration) {
        self.scan(world);
        let start = Instant::now();
        while !self.loading.is_empty() {
            let left = timeout.saturating_sub(start.elapsed());
            match self.loads_rx.recv_timeout(left) {
                Ok((hash, r)) => {
                    self.loading.remove(&hash);
                    match r {
                        Ok(l) => {
                            self.loaded.insert(hash, l);
                        }
                        Err(e) => {
                            self.failed_loads.insert(hash, e);
                        }
                    }
                    self.relink = true;
                }
                Err(_) => break,
            }
        }
        self.scan(world);
    }

    /// Forgets loaded files and cached state (a world was replaced; the store may have changed).
    pub fn reset(&mut self) {
        self.scanned.clear();
        self.live.clear();
        self.loaded.clear();
        self.failed_loads.clear();
        self.relink = true;
        self.stale.clear();
        self.resident.clear();
        self.resident_key = 0;
        self.dirty = true;
    }

    /// Works out the resident chunks for the window's `eye` (and a recent capture's).
    pub fn update(&mut self, world: &World, eye: Vec3, capture: bool) {
        self.scan(world);
        if capture {
            self.capture_eye = Some((eye, Instant::now()));
        }
        let held = self.capture_eye.filter(|(_, t)| t.elapsed().as_secs_f64() < CAPTURE_KEEP_S).map(|(e, _)| e);
        // The window's eye: this one for window frames; for a capture, the window's last (so a
        // capture never drops the window's chunks and makes it upload them again).
        let window = if capture { self.window_eye.unwrap_or(eye) } else { eye };
        self.window_eye = Some(window);
        // Window frames: nothing to do until the camera moved a metre (no work, no allocation).
        if !capture && !self.dirty && self.last_eye.is_some_and(|(e, h)| (e - eye).length() < 1.0 && h == held.is_some()) {
            return;
        }
        self.dirty = false;
        self.last_eye = Some((window, held.is_some()));
        let eyes = [Some(window), held];
        let eyes: &[Vec3] = &eyes.iter().flatten().copied().collect::<Vec<_>>();
        let mut wanted: Vec<(f32, u64, usize, u32)> = Vec::new();
        for live in &self.live {
            let (Some(hash), true) = (live.hash, live.vol.shown) else { continue };
            let Ok(s) = &live.vol.settings else { continue };
            let Some(l) = self.loaded.get(&hash) else { continue };
            for (i, c) in l.chunks.iter().enumerate() {
                let d = eyes
                    .iter()
                    .map(|e| {
                        let dx = (c.lo[0] - e.x).max(e.x - c.hi[0]).max(0.0);
                        let dz = (c.lo[1] - e.z).max(e.z - c.hi[1]).max(0.0);
                        (dx * dx + dz * dz).sqrt()
                    })
                    .fold(f32::MAX, f32::min);
                if d <= self.radius_m {
                    wanted.push((d, hash, i, s.intensity.to_bits()));
                }
            }
        }
        wanted.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        // Nearest first, within MAX_GI_VOLUMES, MAX_GI_PROBES (the GPU side's limits: a set past
        // them would draw no GI at all) and memory_mb.
        let budget = (self.memory_mb.max(0.0) * 1_048_576.0) as u64;
        let mut used = 0u64;
        let mut probes = 0usize;
        let mut set = Vec::new();
        for (_, hash, i, k) in wanted {
            if set.len() >= MAX_GI_VOLUMES {
                break;
            }
            let n = self.loaded[&hash].chunks[i].records.len();
            let bytes = (n * GI_PROBE_BYTES) as u64;
            if (used + bytes > budget && !set.is_empty()) || probes + n > MAX_GI_PROBES {
                continue;
            }
            used += bytes;
            probes += n;
            set.push((hash, i, k));
        }
        // A stable order (by bake and chunk) so the key only moves when the set does.
        set.sort_unstable();
        if set != self.resident {
            let mut h = StableHasher::with_seed(0x4749_5245_5349_0001);
            for (hash, i, k) in &set {
                h.write_u64(*hash);
                h.write_u64(*i as u64);
                h.write_u32(*k);
            }
            self.resident = set;
            self.resident_key = if self.resident.is_empty() { 0 } else { h.finish().max(1) };
            self.rebuilds += 1;
        }
    }

    /// Whether the world has GI volumes (`/gi` children).
    pub fn has_volumes(&mut self, world: &World) -> bool {
        self.scan(world);
        !self.live.is_empty()
    }

    /// Whether the GI pipelines will be wanted: a volume with a settings exists (a bake may land
    /// soon) and the quality isn't off.
    pub fn wants_pipelines(&self) -> bool {
        self.quality != Quality::Off && self.live.iter().any(|l| l.vol.settings.is_ok())
    }

    /// The frame's volumes, at most [`MAX_GI_VOLUMES`] (no allocation).
    pub fn volumes(&self) -> ([GiVolume<'_>; MAX_GI_VOLUMES], usize) {
        const EMPTY: GiVolume<'static> = GiVolume { origin: [0.0; 3], spacing: 1.0, dims: [2, 2, 2], intensity: 0.0, probes: &[] };
        let mut out = [EMPTY; MAX_GI_VOLUMES];
        let mut n = 0;
        for &(hash, i, k) in &self.resident {
            let Some(c) = self.loaded.get(&hash).and_then(|l| l.chunks.get(i)) else { continue };
            out[n] = GiVolume { origin: c.origin, spacing: c.spacing, dims: c.dims, intensity: f32::from_bits(k), probes: &c.records };
            n += 1;
        }
        (out, n)
    }

    /// The `GiFrame` for `volumes` (from [`Gi::volumes`]) as `view` shows it.
    pub fn frame<'a>(&self, volumes: &'a [GiVolume<'a>], view: CaptureView) -> GiFrame<'a> {
        gi_frame(self.resident_key, self.quality, volumes, view)
    }

    /// The resident chunks, held by `Arc` (a capture's frame outlives no borrow of the engine).
    pub fn snapshot(&self) -> Snapshot {
        let items = self
            .resident
            .iter()
            .filter_map(|&(hash, i, k)| {
                let c = self.loaded.get(&hash)?.chunks.get(i)?;
                Some(Held { origin: c.origin, spacing: c.spacing, dims: c.dims, intensity: f32::from_bits(k), records: Arc::clone(&c.records) })
            })
            .collect();
        Snapshot { key: self.resident_key, quality: self.quality, items }
    }


    /// Probes to draw for the `probes` view: (position, valid) of the resident chunks' probes
    /// nearest `eye`, at most `max`.
    pub fn probe_marks(&self, eye: Vec3, max: usize) -> Vec<(Vec3, bool, f32)> {
        let mut all = Vec::new();
        for &(hash, i, _) in &self.resident {
            let Some(c) = self.loaded.get(&hash).and_then(|l| l.chunks.get(i)) else { continue };
            let lat = lux::Lattice { origin: c.origin, spacing: c.spacing, dims: c.dims };
            for (k, r) in c.records.iter().enumerate() {
                let p = lux::Probe::unpack(r);
                let pos = lat.position(k) + Vec3::new(p.offset[0], p.offset[1], p.offset[2]);
                let d = (pos - eye).length();
                all.push((d, pos, p.weight > 0.0, c.spacing));
            }
        }
        all.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Shared border probes appear twice: drop exact repeats.
        all.dedup_by(|a, b| a.1 == b.1);
        all.into_iter().take(max).map(|(_, p, v, s)| (p, v, s)).collect()
    }

    /// Staleness of every volume (cached per world revision): `(volume, reasons)`; no reasons
    /// is fresh.
    pub fn staleness(&mut self, world: &World, lib: &crate::models::Library) -> Vec<(u64, Vec<&'static str>)> {
        self.scan(world);
        let rev = world.revision();
        let mut out = Vec::new();
        for live in &self.live {
            let id = live.vol.id;
            if let Some((r, reasons)) = self.stale.get(&id) {
                if *r == rev {
                    out.push((id, reasons.clone()));
                    continue;
                }
            }
            let mut reasons = Vec::new();
            match (&live.vol.settings, &live.vol.baked) {
                (Ok(s), Some(b)) => {
                    let k = scene_keys(world, lib, s);
                    if k.geometry != b.keys.geometry {
                        reasons.push("geometry");
                    }
                    if k.lights != b.keys.lights {
                        reasons.push("lights");
                    }
                    if k.environment != b.keys.environment {
                        reasons.push("environment");
                    }
                    if s.key() != b.settings {
                        reasons.push("settings");
                    }
                }
                (Ok(_), None) => reasons.push("unbaked"),
                (Err(_), _) => reasons.push("bad_settings"),
            }
            self.stale.insert(id, (rev, reasons.clone()));
            out.push((id, reasons));
        }
        out
    }

    /// Every volume as data (gi_info), bounded by [`MAX_VOLUMES`].
    pub fn volumes_json(&mut self, world: &World, lib: &crate::models::Library) -> Vec<Json> {
        let stale: BTreeMap<u64, Vec<&'static str>> = self.staleness(world, lib).into_iter().collect();
        self.live
            .iter()
            .map(|live| {
                let v = &live.vol;
                let mut j = Json::obj([("name", Json::from(v.name.as_str())), ("path", Json::from(format!("/gi/{}", v.name))), ("id", Json::from(v.id)), ("version", Json::from(v.version))]);
                match &v.settings {
                    Ok(s) => j.set("settings", s.to_json()),
                    Err(e) => j.set("settings_error", Json::from(e.as_str())),
                };
                let reasons = stale.get(&v.id).cloned().unwrap_or_default();
                let baking = self.job.as_ref().is_some_and(|job| job.volume == v.name);
                let status = if baking {
                    "baking"
                } else if v.baked.is_some() && live.hash.is_none() && live.problem.as_deref() == Some("loading") {
                    "loading"
                } else if v.baked.is_some() && live.hash.is_none() {
                    "missing"
                } else if reasons.contains(&"unbaked") {
                    "unbaked"
                } else if reasons.is_empty() {
                    "fresh"
                } else {
                    "stale"
                };
                j.set("status", Json::from(status));
                if status == "stale" {
                    j.set("stale", Json::arr(reasons.iter().map(|r| Json::from(*r))));
                }
                if let Some(p) = &live.problem {
                    j.set("problem", Json::from(p.as_str()));
                }
                if let Some(b) = &v.baked {
                    j.set("baked", b.to_json());
                }
                if let Some(h) = live.hash {
                    let res = self.resident.iter().filter(|r| r.0 == h).count();
                    let l = &self.loaded[&h];
                    j.set("chunks", Json::obj([("total", Json::from(l.chunks.len())), ("resident", Json::from(res))]));
                }
                if !v.shown {
                    j.set("hidden", Json::from(true));
                }
                j
            })
            .collect()
    }

    /// For sight packets: the volumes with a resident chunk, their status.
    pub fn packet_json(&mut self, world: &World, lib: &crate::models::Library, view: CaptureView) -> Option<Json> {
        self.scan(world);
        if self.live.is_empty() {
            return None;
        }
        let vols: Vec<Json> = self.volumes_json(world, lib).into_iter().take(8).collect();
        let slim: Vec<Json> = vols
            .into_iter()
            .map(|v| {
                let mut s = Json::obj([("name", v.get("name").cloned().unwrap_or(Json::Null)), ("status", v.get("status").cloned().unwrap_or(Json::Null))]);
                for k in ["stale", "chunks", "problem"] {
                    if let Some(x) = v.get(k) {
                        s.set(k, x.clone());
                    }
                }
                s
            })
            .collect();
        let mut j = Json::obj([("view", Json::from(view.name())), ("quality", Json::from(self.quality.name())), ("volumes", Json::Arr(slim))]);
        if view == CaptureView::Probes {
            j.set("probes_drawn", Json::from(self.marks_drawn.0));
            j.set("invalid_probes_not_drawn", Json::from(self.marks_drawn.1));
        }
        Some(j)
    }

    /// The loaded field of a volume (gi_check).
    pub fn field_of(&self, name: &str) -> Option<(&lux::Field, &Volume)> {
        let live = self.live.iter().find(|l| l.vol.name == name)?;
        Some((&self.loaded.get(&live.hash?)?.field, &live.vol))
    }

    pub fn volume_names(&self) -> Vec<String> {
        self.live.iter().map(|l| l.vol.name.clone()).collect()
    }

    /// Resident chunks, GPU-side bytes of what is loaded, and bakes in memory.
    pub fn residency_json(&self) -> Json {
        let loaded_bytes: u64 = self.loaded.values().map(|l| l.bytes).sum();
        let resident_bytes: u64 = self.resident.iter().filter_map(|(h, i, _)| self.loaded.get(h).and_then(|l| l.chunks.get(*i))).map(|c| (c.records.len() * GI_PROBE_BYTES) as u64).sum();
        Json::obj([
            ("resident_chunks", Json::from(self.resident.len())),
            ("resident_bytes", Json::from(resident_bytes)),
            ("loaded_bakes", Json::from(self.loaded.len())),
            ("loaded_bytes", Json::from(loaded_bytes)),
            ("set_changes", Json::from(self.rebuilds)),
        ])
    }

}

/// A resident chunk held by a [`Snapshot`].
struct Held {
    origin: [f32; 3],
    spacing: f32,
    dims: [u32; 3],
    intensity: f32,
    records: Arc<[lux::Record]>,
}

/// The resident chunks of a moment (see [`Gi::snapshot`]).
pub struct Snapshot {
    key: u64,
    quality: Quality,
    items: Vec<Held>,
}

impl Snapshot {
    pub fn volumes(&self) -> Vec<GiVolume<'_>> {
        self.items.iter().map(|h| GiVolume { origin: h.origin, spacing: h.spacing, dims: h.dims, intensity: h.intensity, probes: &h.records }).collect()
    }

    pub fn frame<'a>(&self, volumes: &'a [GiVolume<'a>], view: CaptureView) -> GiFrame<'a> {
        gi_frame(self.key, self.quality, volumes, view)
    }
}

fn gi_frame<'a>(key: u64, q: Quality, volumes: &'a [GiVolume<'a>], view: CaptureView) -> GiFrame<'a> {
    let quality = q.gpu();
    if q == Quality::Off || view == CaptureView::Off || volumes.is_empty() {
        // Keep the buffer (its key) for the next frame that wants it.
        return GiFrame { key, volumes: &[], quality, view: GiView::Lit };
    }
    let view = if matches!(view, CaptureView::Irradiance | CaptureView::Probes) { GiView::Irradiance } else { GiView::Lit };
    GiFrame { key, volumes, quality, view }
}

/// How many threads a bake gets by default: all but one of the machine's, at most 4 (the frame
/// thread and the game keep theirs).
pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(1)).clamp(1, 4)
}

/// The warnings for lights a bake found inside a solid (`light_inside_geometry`, naming each).
pub fn light_warnings(names: &[String]) -> Vec<Json> {
    names
        .iter()
        .map(|n| {
            Json::obj([
                ("code", Json::from("light_inside_geometry")),
                ("light", Json::from(n.as_str())),
                ("message", Json::from(format!("{n}'s point light sits inside a solid mesh: fully shadowed, it adds nothing to the bake (move it out, or exclude the mesh around it)"))),
            ])
        })
        .collect()
}

/// A sight packet's `gi.drawn`: whether that render drew the probes (the setting in `quality`
/// is what was asked for, this what the frame got), and `gi.gpu_error` when its set failed to
/// bind. The `off` view draws none by design.
pub fn drawn_json(g: &mut Json, view: CaptureView, drawn: bool, error: Option<&str>) {
    g.set("drawn", Json::from(drawn));
    if !drawn && view != CaptureView::Off {
        match error {
            Some(e) => {
                g.set("gpu_error", Json::from(e));
            }
            None => {
                g.set("not_drawn", Json::from("its pipelines or chunks were not ready: the flat ambient"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn lights_inside_solids_are_warned_by_name() {
        let w = light_warnings(&["/lantern/flame".to_owned()]);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].get("code").and_then(Json::as_str), Some("light_inside_geometry"));
        assert_eq!(w[0].get("light").and_then(Json::as_str), Some("/lantern/flame"));
        assert!(light_warnings(&[]).is_empty());
    }

    #[test]
    fn packets_say_whether_the_render_drew_gi() {
        let mut g = Json::obj([("view", Json::from("lit"))]);
        drawn_json(&mut g, CaptureView::Lit, false, Some("GI volume 0: dims [2, 2, 3]"));
        assert_eq!(g.get("drawn"), Some(&Json::from(false)));
        assert!(g.get("gpu_error").and_then(Json::as_str).is_some_and(|e| e.contains("GI volume 0")));
        let mut g = Json::obj([("view", Json::Null)]);
        drawn_json(&mut g, CaptureView::Lit, false, None);
        assert!(g.get("not_drawn").is_some() && g.get("gpu_error").is_none());
        let mut g = Json::obj([("view", Json::Null)]);
        drawn_json(&mut g, CaptureView::Off, false, Some("old"));
        assert!(g.get("gpu_error").is_none() && g.get("not_drawn").is_none(), "off draws none by design");
        let mut g = Json::obj([("view", Json::Null)]);
        drawn_json(&mut g, CaptureView::Irradiance, true, None);
        assert_eq!(g.get("drawn"), Some(&Json::from(true)));
    }
    #[test]
    fn integrated_gpus_default_to_low_and_are_warned_off_high() {
        assert_eq!(Quality::default_for(true), Quality::Low);
        assert_eq!(Quality::default_for(false), Quality::High);
        assert!(tier_warning(Quality::High, true).is_some_and(|w| w.get("code").and_then(Json::as_str) == Some("gi_tier_over_budget")));
        assert!(tier_warning(Quality::High, false).is_none() && tier_warning(Quality::Low, true).is_none() && tier_warning(Quality::Off, true).is_none());
        assert_eq!(Quality::parse("medium"), None, "the middle tier was dropped");
    }

    #[test]
    fn settings_parse_patch_and_key() {
        let s = Settings::parse(&Json::parse(r#"{"min": [-20, 0, -20], "max": [20, 6, 20], "spacing": 2}"#).unwrap()).unwrap();
        assert_eq!((s.spacing, s.rays, s.bounces, s.intensity), (2.0, 256, 2, 1.0));
        let back = Settings::parse(&s.to_json()).unwrap();
        assert_eq!(back, s);
        let p = Settings::patched(&s, &Json::parse(r#"{"intensity": 1.5}"#).unwrap()).unwrap();
        assert_eq!(p.key(), s.key(), "intensity is runtime: no rebake");
        let p = Settings::patched(&s, &Json::parse(r#"{"rays": 512}"#).unwrap()).unwrap();
        assert_ne!(p.key(), s.key());
        assert!(Settings::patched(&s, &Json::parse(r#"{"spacing": 0.01}"#).unwrap()).is_err());
        assert!(Settings::patched(&s, &Json::parse(r#"{"max": [-30, 6, 20]}"#).unwrap()).is_err());
        assert!(Settings::patched(&s, &Json::parse(r#"{"exclude": ["units"]}"#).unwrap()).is_err());
        assert!(Settings::parse(&Json::parse(r#"{"spacing": 2}"#).unwrap()).is_err(), "a box is required");
    }

    #[test]
    fn baked_info_round_trips() {
        let b = BakedInfo {
            hash: 0xfeed_beef_0123_4567,
            keys: SceneKeys { geometry: 1, lights: u64::MAX, environment: 3 },
            settings: 4,
            probes: 10,
            valid: 9,
            relocated: 1,
            invalid: 1,
            triangles: 100,
            lights: 2,
        };
        assert_eq!(BakedInfo::parse(&b.to_json()), Some(b));
    }

    #[test]
    fn shapes_tessellate_deterministically() {
        for geo in [Geo::Cube, Geo::Quad, Geo::Sphere, Geo::Lathe { points: vec![[0.0, 0.0], [0.5, 0.0], [0.4, 1.0], [0.0, 1.0]], segments: 12 }] {
            let (p, n, i) = unit_mesh(&geo);
            assert!(!p.is_empty() && p.len() == n.len() && i.len() % 3 == 0 && i.iter().all(|&k| (k as usize) < p.len()));
            assert_eq!(unit_mesh(&geo).0, p);
            assert_eq!(geo_triangles(&geo), i.len() / 3, "the count gather checks is what the worker builds");
        }
    }

    #[test]
    fn an_over_limit_box_is_refused_before_its_triangles_are_built() {
        // 40 lathes of 255 rings x 64 segments (32,640 triangles each): 1.3 million, past
        // MAX_TRIANGLES. gather counts them as it goes and refuses at the 33rd, so the worker never
        // tries to allocate the list (an allocation failure aborts the process).
        let points: Vec<[f32; 2]> = (0..256).map(|i| [0.3 + 0.1 * (i % 2) as f32, i as f32 * 0.01]).collect();
        let vases = |n: usize| {
            let mut w = World::default();
            for k in 0..n {
                let mut s = crate::world::SpawnSpec::new(Shape::Lathe);
                s.name = Some(format!("vase{k}"));
                s.position = Vec3::new(k as f32 * 0.5, 0.0, 0.0);
                s.profile = Some(crate::world::Profile { points: points.clone(), segments: 64 });
                w.spawn(s, "t").unwrap();
            }
            w
        };
        let lib = crate::models::Library::new(std::path::Path::new("no-such-assets"), None);
        let settings = Settings::parse(&Json::parse("{\"min\": [-1, 0, -1], \"max\": [21, 3, 1]}").unwrap()).unwrap();
        let started = std::time::Instant::now();
        let e = gather(&vases(40), &lib, &settings).err().expect("refused");
        assert_eq!(e.code, "over_limit");
        assert!(e.message.contains("triangles"), "{}", e.message);
        assert!(started.elapsed().as_secs_f64() < 5.0);
        // Half as many fit: gathered, and the worker's list is exactly what was counted.
        let world = vases(20);
        let input = gather(&world, &lib, &settings).ok().expect("fits");
        assert_eq!(triangles(&input).len(), 20 * 255 * 64 * 2);
        // gather hashes the scene keys in its own pass: the same keys staleness compares with.
        assert!(input.keys == scene_keys(&world, &lib, &settings), "gather's keys are scene_keys'");
    }

    #[test]
    fn the_store_keeps_within_its_cap_oldest_first() {
        let dir = std::env::temp_dir().join(format!("specula-gi-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store { dir: dir.clone() };
        for h in 1..=4u64 {
            store.put(100 + h, h, &[7u8; 1000]).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(30)); // distinct modified times
        }
        assert!((1..=4).all(|h| store.file(h).exists()), "within the default cap");
        store.trim(4, 2500);
        assert!(!store.file(1).exists() && !store.file(2).exists(), "the two oldest went");
        assert!(store.file(3).exists() && store.file(4).exists());
        assert_eq!(store.by_input(101), None, "an input whose file went bakes again");
        assert_eq!(store.by_input(104), Some(4));
        store.trim(4, 0);
        assert!(store.file(4).exists(), "the file just written stays");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn residency_never_passes_what_the_gpu_holds() {
        // The review's blocker: nine 64 x 16 x 64 volumes (589,824 probes) with a big memory_mb
        // made a set the GPU side refused, and the window loop stopped. Residency now stops at
        // MAX_GI_PROBES (and the GPU side falls back to the flat ambient on any refusal).
        let dir = std::env::temp_dir().join(format!("specula-gi-resident-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store { dir: dir.clone() };
        let mut w = World::default();
        let mut g = crate::world::SpawnSpec::new(Shape::Group);
        g.name = Some(GROUP.into());
        w.spawn(g, "t").unwrap();
        for k in 0..9u32 {
            let x = k as f32 * 70.0;
            let settings = Settings::parse(&Json::parse(&format!("{{\"min\": [{x}, 0, 0], \"max\": [{}, 15, 63]}}", x + 63.0)).unwrap()).unwrap();
            let lattice = lux::Lattice { origin: [x + 0.5, 0.5, 0.5], spacing: 1.0, dims: [64, 16, 64] };
            let field = lux::Field { lattice, probes: vec![lux::Probe { weight: 1.0, ..lux::Probe::default() }; lattice.count()] };
            let (bytes, hash) = lux::file::encode(&field, lux::file::Keys::default(), lux::file::Counts::default());
            store.put(1000 + u64::from(k), hash, &bytes).unwrap();
            let baked = BakedInfo { hash, keys: SceneKeys::default(), settings: settings.key(), probes: field.probes.len(), valid: field.probes.len(), relocated: 0, invalid: 0, triangles: 0, lights: 0 };
            let mut prop = settings.to_json();
            prop.set("baked", baked.to_json());
            let mut v = crate::world::SpawnSpec::new(Shape::Group);
            v.name = Some(format!("v{k}"));
            v.parent = Some(crate::world::EntityKey::Path(format!("/{GROUP}")));
            v.props = Json::obj([(PROP, prop)]);
            w.spawn(v, "t").unwrap();
        }
        let mut gi = Gi::new(dir.clone());
        gi.memory_mb = 1000.0;
        gi.radius_m = 4096.0;
        gi.wait_loads(&w, std::time::Duration::from_secs(60));
        gi.update(&w, Vec3::new(300.0, 5.0, 30.0), false);
        let (vols, n) = gi.volumes();
        let probes: usize = vols[..n].iter().map(|v| v.probes.len()).sum();
        assert!(n >= 8 && probes <= MAX_GI_PROBES, "{n} volumes, {probes} probes");
        let frame = gi.frame(&vols[..n], CaptureView::Lit);
        assert!(specula_gpu::gi_check(&frame).is_ok(), "the GPU side takes the set");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bake_records_the_same_whether_fresh_or_from_the_store() {
        // F2 (review finding 3): the world's record of a bake (it enters world_hash) held wall-clock
        // milliseconds, and a bake landed from the store recorded no triangles or lights, so two
        // engines, or a warm cache and a fresh clone, disagreed.
        let mut t = Vec::new();
        for (lo, hi) in [([-4.0, -1.0, -4.0], [4.0, 0.0, 4.0]), ([0.0, 0.0, 0.0], [1.0, 2.0, 1.0])] {
            let c = |x: bool, y: bool, z: bool| [if x { hi[0] } else { lo[0] }, if y { hi[1] } else { lo[1] }, if z { hi[2] } else { lo[2] }];
            // Each box's top, facing up.
            let q = [c(false, true, false), c(false, true, true), c(true, true, true), c(true, true, false)];
            let n = [0.0, 1.0, 0.0];
            t.push(lux::Triangle { p: [q[0], q[1], q[2]], normal: n, albedo: [0.5; 3], emissive: [0.0; 3] });
            t.push(lux::Triangle { p: [q[0], q[2], q[3]], normal: n, albedo: [0.5; 3], emissive: [0.0; 3] });
        }
        let scene = lux::Scene { triangles: t, lights: vec![lux::PointLight { position: [0.5, 2.5, 0.5], color: [1.0; 3], range: 4.0 }], sun: None, sky: Some(lux::Sky { radiance: [0.2; 3], below: 1.0 }) };
        let req = lux::Request { min: [-2.0, 0.0, -2.0], max: [2.0, 2.0, 2.0], spacing: 1.0, rays: 32, bounces: 1, clamp: 0.0 };
        let keys = SceneKeys { geometry: 1, lights: 2, environment: 3 };
        let fresh = |threads| {
            let b = lux::bake(&scene, &req, threads, &lux::Control::default()).unwrap();
            let (bytes, hash) = lux::file::encode(&b.field, lux::file::Keys::default(), counts_of(&b.stats));
            (BakedInfo::of(hash, keys, 9, counts_of(&b.stats), b.stats.probes), bytes)
        };
        let (a, bytes) = fresh(1);
        let (b, _) = fresh(3);
        let d = lux::file::decode(&bytes).unwrap();
        let cached = BakedInfo::of(d.hash, keys, 9, d.counts, d.field.probes.len());
        assert_eq!(a.to_json().to_string(), b.to_json().to_string(), "two fresh bakes");
        assert_eq!(a.to_json().to_string(), cached.to_json().to_string(), "fresh and from the store");
        assert!(a.triangles == 4 && a.lights == 1, "{a:?}");
        assert!(a.to_json().get("ms").is_none(), "no timings in world data");
    }

    #[test]
    fn colours_convert_with_dmath() {
        assert_eq!(srgb_to_linear(1.0), 1.0);
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert!((srgb_to_linear(0.5) - 0.214).abs() < 1e-3);
        assert_eq!(srgb_to_linear(f32::NAN), 0.0);
    }
}

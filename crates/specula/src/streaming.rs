//! Streaming: which model builds and HLOD proxies are on the GPU, uploaded within per-frame
//! budgets and evicted under a memory budget (`docs/design/STREAMING.md`).
//!
//! **Never changes the simulation.** Everything here reads the [`World`] and writes only GPU
//! resources and its own bookkeeping: entities, their resolved models and the world hash are the
//! same whatever is resident (`stream_check.py` compares world hashes with streaming on and off).
//!
//! - **Cells.** Persistent model entities are filed into square XZ cells of `cell_m` metres by
//!   their origin ([`Index`], patched from the world's draw changes like the window's draw cache).
//! - **Sources.** The window's camera every frame, and each capture's camera for
//!   [`CAPTURE_SOURCE_FOR`] after it (so an agent's next capture finds its view resident).
//! - **Wanted.** A cell's members are wanted within `radius_m` of a source, unless its HLOD proxy
//!   is current and the source is `preload` times past the cell's switch distance (the proxy
//!   draws them). Priority is distance (halved inside the window's view, quartered for captures);
//!   transient model entities (a game's presentation) are always wanted first.
//! - **Uploads.** Each frame (and each headless loop) [`Streamer::pump`] uploads wanted builds one
//!   LOD at a time, coarsest first (an entity draws its coarser level while finer ones arrive),
//!   plus built proxies, nearest first, through the GPU's queued path (no wait), until the
//!   frame's `upload_ms` or `upload_mb` is spent. CPU work (re-indexing parts) runs inside that
//!   budget too; mesh builds and proxy generation run on worker threads.
//! - **Memory.** Uploaded builds and proxies count against `memory_mb`. Over it, what no source
//!   wants is evicted, least recently wanted first (textured glTF builds stay: their pixels left
//!   the CPU at upload). If what is wanted alone exceeds the budget, the farthest stays out and a
//!   `stream_memory_over_budget` error says so.
//! - **Agents see it.** `perf_frame.streaming`, the `stream_state` tool, sight packets
//!   (`streaming` summary; subjects `drawn_as: "hlod" | "not_resident"`), and captures wait (like
//!   for builds) until what their view needs is resident (`capture_wait`).

use crate::hlod::{self, CellKey, Content, Hlod, MemberInput};
use crate::models::{part_influence, part_look, part_mesh, textured_look, Built, GpuModel, GpuModels, Needs, PartDraw, SharedLibrary};
use crate::perf::Series;
use crate::sight::{self, Camera, Subject};
use crate::world::{Entity, Shape, World};
use specula_core::hash::StableHasher;
use specula_core::math::{Mat4, Vec3};
use specula_core::Json;
use specula_gpu::{DrawItem, Gpu, MaterialTextures, MeshStreams, TextureId};
use specula_model::{MaterialKind, MeshData};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, VecDeque};

/// Hash maps keyed by ids: the stable (unkeyed, fast) hasher, not SipHash.
type IdMap<K, V> = HashMap<K, V, specula_core::hash::StableBuildHasher>;
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

/// While the window's camera moves (a quarter cell, or turns), priorities and wanted sets are
/// recomputed at most this often; at once when the world, the builds or the sources change.
pub const UPDATE_EVERY: Duration = Duration::from_millis(100);
/// Captures with no frame started for this long (nothing drives frames: a test, a stalled
/// window) start an upload budget of their own: a budget is per frame, never per lifetime.
pub const FRAME_STALE: Duration = Duration::from_millis(100);
/// A model at least this many pixels across in a camera's view is wanted there whatever
/// `radius_m` says (a big building or a glass tower far away stays drawn).
pub const RESIDENT_MIN_PX: f32 = 4.0;
/// A build at least this many entities use is drawn instanced (one draw for all of them), so
/// HLOD leaves it out: merging it would only add draws.
pub const INSTANCED_MIN: u32 = 8;
/// Otherwise at least this often (cells settling for their proxies).
pub const UPDATE_IDLE: Duration = Duration::from_secs(1);
/// A capture's camera stays a stream source this long after the capture.
pub const CAPTURE_SOURCE_FOR: Duration = Duration::from_secs(10);
/// Capture sources kept at once (the oldest goes).
const MAX_CAPTURE_SOURCES: usize = 4;
/// Recent budget misses kept for perf_frame.
const MAX_MISSES: usize = 16;
/// Cells and entities stream_state lists at most.
pub const MAX_LISTED: usize = 64;

/// Adapter kinds a `[streaming.<kind>]` section may name.
const ADAPTER_KINDS: [&str; 5] = ["integrated", "discrete", "virtual", "cpu", "other"];

// ---------------------------------------------------------------------------------------------
// Settings: perf.toml [streaming]

/// The streaming settings (perf.toml `[streaming]`, `[streaming.<adapter kind>]` over it, and
/// `stream_settings` over both).
#[derive(Clone, Debug, PartialEq)]
pub struct StreamConfig {
    /// Off: every live build is uploaded at once (synchronously, as before streaming) and no
    /// proxies are drawn.
    pub enabled: bool,
    pub cell_m: f32,
    /// Members of cells farther than this from every source are not wanted.
    pub radius_m: f32,
    /// Per-frame upload budget: CPU milliseconds and megabytes.
    pub upload_ms: f64,
    pub upload_mb: f64,
    /// Uploaded builds and proxies stay within this.
    pub memory_mb: f64,
    pub hlod: bool,
    /// No proxy is drawn nearer than this.
    pub hlod_min_m: f32,
    /// Proxies are wanted within this of a source.
    pub hlod_radius_m: f32,
    /// Members load once a source is within `preload` times a proxied cell's switch distance.
    pub preload: f32,
    /// A changed cell waits this long without changes before its proxy is rebuilt.
    pub settle_s: f32,
    /// Captures wait for what their view needs to be resident.
    pub capture_wait: bool,
}

impl Default for StreamConfig {
    fn default() -> StreamConfig {
        StreamConfig {
            enabled: true,
            cell_m: 32.0,
            radius_m: 384.0,
            upload_ms: 2.0,
            upload_mb: 8.0,
            memory_mb: 512.0,
            hlod: true,
            hlod_min_m: 48.0,
            hlod_radius_m: 4096.0,
            preload: 1.25,
            settle_s: 1.0,
            capture_wait: true,
        }
    }
}

/// `(key, min, max)` of the numeric settings.
const RANGES: [(&str, f64, f64); 9] = [
    ("cell_m", 4.0, 1024.0),
    ("radius_m", 16.0, 1.0e5),
    ("upload_ms", 0.1, 100.0),
    ("upload_mb", 0.25, 1024.0),
    ("memory_mb", 8.0, 65536.0),
    ("hlod_min_m", 0.0, 1.0e5),
    ("hlod_radius_m", 16.0, 1.0e6),
    // At least past the window's 10% hysteresis into a proxy, so members never stop being
    // wanted before the proxy is drawn instead.
    ("preload", 1.15, 4.0),
    ("settle_s", 0.0, 60.0),
];
const FLAGS: [&str; 3] = ["enabled", "hlod", "capture_wait"];

impl StreamConfig {
    /// Lays `table`'s keys over `self` (checked; `section` names it in errors).
    pub fn apply(&mut self, table: &Json, section: &str) -> Result<(), String> {
        for (k, v) in table.as_object().ok_or_else(|| format!("[{section}] must be a table"))? {
            if ADAPTER_KINDS.contains(&k.as_str()) && section == "streaming" {
                continue; // adapter sections, applied by StreamTable
            }
            if FLAGS.contains(&k.as_str()) {
                let b = v.as_bool().ok_or_else(|| format!("[{section}] {k} must be true or false"))?;
                match k.as_str() {
                    "enabled" => self.enabled = b,
                    "hlod" => self.hlod = b,
                    _ => self.capture_wait = b,
                }
                continue;
            }
            let Some(&(_, lo, hi)) = RANGES.iter().find(|(n, ..)| n == k) else {
                let known: Vec<&str> = FLAGS.iter().copied().chain(RANGES.iter().map(|r| r.0)).collect();
                return Err(format!("unknown key '{k}' in [{section}] (known: {})", known.join(", ")));
            };
            let x = v.as_f64().filter(|x| x.is_finite() && (lo..=hi).contains(x)).ok_or_else(|| format!("[{section}] {k} must be a number from {lo} to {hi}"))?;
            match k.as_str() {
                "cell_m" => self.cell_m = x as f32,
                "radius_m" => self.radius_m = x as f32,
                "upload_ms" => self.upload_ms = x,
                "upload_mb" => self.upload_mb = x,
                "memory_mb" => self.memory_mb = x,
                "hlod_min_m" => self.hlod_min_m = x as f32,
                "hlod_radius_m" => self.hlod_radius_m = x as f32,
                "preload" => self.preload = x as f32,
                _ => self.settle_s = x as f32,
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> Json {
        let r = |x: f64| Json::from((x * 1000.0).round() / 1000.0);
        Json::obj([
            ("enabled", Json::from(self.enabled)),
            ("cell_m", r(f64::from(self.cell_m))),
            ("radius_m", r(f64::from(self.radius_m))),
            ("upload_ms", r(self.upload_ms)),
            ("upload_mb", r(self.upload_mb)),
            ("memory_mb", r(self.memory_mb)),
            ("hlod", Json::from(self.hlod)),
            ("hlod_min_m", r(f64::from(self.hlod_min_m))),
            ("hlod_radius_m", r(f64::from(self.hlod_radius_m))),
            ("preload", r(f64::from(self.preload))),
            ("settle_s", r(f64::from(self.settle_s))),
            ("capture_wait", Json::from(self.capture_wait)),
        ])
    }
}

/// perf.toml `[streaming]` as read: the base table and the adapter sections.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamTable {
    base: Vec<(String, Json)>,
    adapters: Vec<(String, Json)>,
}

impl StreamTable {
    /// Parses and checks `[streaming]` (every adapter section included).
    pub fn parse(v: &Json) -> Result<StreamTable, String> {
        let mut t = StreamTable::default();
        for (k, x) in v.as_object().ok_or("[streaming] must be a table")? {
            if ADAPTER_KINDS.contains(&k.as_str()) {
                if x.as_object().is_none() {
                    return Err(format!("[streaming.{k}] must be a table"));
                }
                t.adapters.push((k.clone(), x.clone()));
            } else {
                t.base.push((k.clone(), x.clone()));
            }
        }
        StreamConfig::default().apply(&Json::Obj(t.base.clone()), "streaming")?;
        for (kind, table) in &t.adapters {
            let mut c = StreamConfig::default();
            c.apply(&Json::Obj(t.base.clone()), "streaming")?;
            c.apply(table, &format!("streaming.{kind}"))?;
        }
        Ok(t)
    }

    /// The settings for an adapter kind.
    pub fn config(&self, kind: &str) -> StreamConfig {
        let mut c = StreamConfig::default();
        let _ = c.apply(&Json::Obj(self.base.clone()), "streaming");
        if let Some((_, t)) = self.adapters.iter().find(|(k, _)| k == kind) {
            let _ = c.apply(t, &format!("streaming.{kind}"));
        }
        c
    }
}

// ---------------------------------------------------------------------------------------------
// The cell index

/// A persistent model entity as streaming files it.
#[derive(Clone, Debug)]
struct Member {
    id: u64,
    handle: Option<u32>,
    world: Mat4,
    pos: Vec3,
    /// Its world box (the build's bounds once built; its origin before): streaming distances
    /// are measured to it, so a big model near the camera loads whatever cell its origin is in.
    lo: Vec3,
    hi: Vec3,
    tint: [f32; 3],
    /// May be merged into an HLOD proxy as far as the entity goes (shown, not `ui`, no anim, no
    /// moving body, not wide); its build decides the rest.
    mergeable: bool,
    /// Wider than a cell (a terrain, a cliff range): wanted by its own box, never merged, and
    /// its box doesn't stretch its cell's.
    wide: bool,
    /// When it last changed (None: unchanged since the index was built).
    changed: Option<Instant>,
}

impl Member {
    fn of(e: &Entity, now: Option<Instant>, cell_m: f32) -> Member {
        let world = e.model();
        let moving = e.body.is_some_and(|b| b.kind != crate::physics::BodyKind::Static);
        let mut m = Member {
            id: e.id,
            handle: e.resolved_model().map(|r| r.handle),
            world,
            pos: e.world_position(),
            lo: Vec3::ZERO,
            hi: Vec3::ZERO,
            tint: e.color,
            mergeable: e.shown() && !e.in_ui() && e.anim.is_none() && !moving && e.id < hlod::MAX_COVER_ID,
            wide: false,
            changed: now,
        };
        m.set_box(e.world_aabb(), cell_m);
        m
    }

    fn set_box(&mut self, (lo, hi): (Vec3, Vec3), cell_m: f32) {
        self.lo = lo;
        self.hi = hi;
        self.wide = (hi.x - lo.x).max(hi.z - lo.z) > cell_m;
    }

    fn merges(&self) -> bool {
        self.mergeable && !self.wide
    }
}

#[derive(Default)]
struct Cell {
    /// Sorted by id.
    members: Vec<Member>,
    /// Box of the members' world boxes.
    lo: Vec3,
    hi: Vec3,
    /// Its HLOD content must be worked out again.
    dirty: bool,
    /// Its members' distinct model handles, whether the cell's proxy holds them, and their
    /// largest world scale (the wanted set walks these, not every member). Wide members aren't
    /// in it.
    handles: Vec<(u32, bool, f32)>,
    /// It has wide members (wanted one by one, by their own boxes).
    has_wide: bool,
}

impl Cell {
    /// The box of its members' boxes (a wide member counts by its origin only).
    fn bounds(&mut self) {
        let mut it = self.members.iter().map(|m| if m.wide { (m.pos, m.pos) } else { (m.lo, m.hi) });
        let first = it.next().unwrap_or((Vec3::ZERO, Vec3::ZERO));
        let (lo, hi) = it.fold(first, |(lo, hi), (a, b)| (lo.min(a), hi.max(b)));
        self.lo = lo;
        self.hi = hi;
        self.has_wide = self.members.iter().any(|m| m.wide);
    }
}

/// Persistent model entities by cell.
#[derive(Default)]
struct Index {
    /// `(world generation, cell_m bits)` it was built for.
    key: Option<(u64, u32)>,
    /// The world's draw epoch it reflects.
    epoch: u64,
    cell_of: IdMap<u64, CellKey>,
    cells: BTreeMap<CellKey, Cell>,
    rebuilds: u64,
    patches: u64,
    /// Cells whose box a patch must work out again (reused).
    rebound: Vec<CellKey>,
    /// The cell size it was built for.
    cell_m: f32,
    /// Changed entities not filed yet (sorted).
    backlog: Vec<u64>,
    /// A refiling in progress: the world's change record no longer reached back (an agent's
    /// batch bigger than it), so every entity is filed again, a slice at a time, while the cells
    /// as they were serve (see [`Index::sync`]).
    resync: Option<Resync>,
    /// Refilings finished.
    resyncs: u64,
    /// Refilings in a row whose catch-up found the change record overflowed again: each next one
    /// gets twice the slice, so a world that keeps changing still converges (after a few, it is
    /// filed at once, as a rebuild would).
    resync_overflows: u32,
}

/// See [`Index::resync`].
struct Resync {
    /// The world's draw epoch when it began: changes since are patched in once it is done.
    epoch: u64,
    /// The last entity filed.
    cursor: Option<u64>,
    /// The highest persistent id when it began: entities spawned since are the catch-up's (a
    /// world spawning faster than the walk would otherwise keep it from ending).
    end: Option<u64>,
    cell_of: IdMap<u64, CellKey>,
    cells: BTreeMap<CellKey, Cell>,
}

fn cell_key(p: Vec3, cell_m: f32) -> CellKey {
    // dmath: the same cells on every platform (which cells stream in decides what captures show).
    let c = |v: f32| specula_core::dmath::floor(v / cell_m).clamp(-1.0e9, 1.0e9) as i32;
    (c(p.x), c(p.z))
}

impl Index {
    /// Brings the index up to date with `world`. Returns whether anything changed; `touched`
    /// gets the cells where a patch changed a mergeable member (one an HLOD proxy may hold;
    /// cleared first; empty after a rebuild).
    ///
    /// Changed entities are filed within `slice` (at least one); the rest wait in a backlog for
    /// the next call (an agent's 1,000-spawn batch costs a few milliseconds to file).
    fn sync(&mut self, world: &World, generation: u64, cell_m: f32, now: Instant, touched: &mut Vec<CellKey>, slice: Duration) -> bool {
        touched.clear();
        let key = (generation, cell_m.to_bits());
        if self.key == Some(key) && self.epoch == world.draw_epoch() && self.backlog.is_empty() && self.resync.is_none() {
            return false;
        }
        // The same world, but more changed than its change record holds: everything is filed
        // again a slice at a time (the old cells serve meanwhile), then the changes since.
        let patched = self.key == Some(key) && self.resync.is_none() && self.patch(world, cell_m, now, touched, slice);
        if !patched && self.key == Some(key) {
            if self.resync.is_none() {
                self.begin_resync(world);
            }
            let started = Instant::now();
            // Twice the slice for each refiling in a row that couldn't catch up.
            let scaled = if self.resync_overflows >= 6 { Duration::MAX } else { slice.saturating_mul(1 << self.resync_overflows) };
            let least = 64usize << self.resync_overflows.min(16);
            if self.resync_step(world, cell_m, started, scaled, least) {
                let rest = scaled.saturating_sub(started.elapsed());
                if self.patch(world, cell_m, now, touched, rest) {
                    self.resync_overflows = 0;
                } else {
                    self.resync_overflows += 1;
                    self.begin_resync(world);
                }
            }
            return true;
        }
        if !patched {
            // Another world (a load) or cell size: filed at once (the old cells don't apply).
            let _z = specula_core::profile::zone("rebuild");
            self.resync = None;
            self.backlog.clear();
            self.cells.clear();
            self.cell_of.clear();
            for e in world.persistent_entities().filter(|e| e.shape == Shape::Model) {
                let k = cell_key(e.world_position(), cell_m);
                self.cell_of.insert(e.id, k);
                self.cells.entry(k).or_default().members.push(Member::of(e, None, cell_m));
            }
            for c in self.cells.values_mut() {
                c.bounds();
                c.dirty = true;
            }
            self.key = Some(key);
            self.cell_m = cell_m;
            self.rebuilds += 1;
        }
        self.epoch = world.draw_epoch();
        true
    }

    /// Starts filing every entity again (see [`Index::resync`]).
    fn begin_resync(&mut self, world: &World) {
        self.backlog.clear();
        self.resync = Some(Resync { epoch: world.draw_epoch(), cursor: None, end: world.last_persistent_id(), cell_of: IdMap::default(), cells: BTreeMap::new() });
    }

    /// Files the next entities of a refiling for up to `slice` since `started` (`least` at least);
    /// when done, its cells replace the old ones (every one to be worked out again) and the
    /// changes since it began are read next. Returns whether it is done.
    fn resync_step(&mut self, world: &World, cell_m: f32, started: Instant, slice: Duration, least: usize) -> bool {
        let _z = specula_core::profile::zone("resync");
        let Some(r) = self.resync.as_mut() else { return true };
        let mut done = true;
        let walk = r.end.map(|end| world.persistent_entities_between(r.cursor, end));
        for (n, e) in walk.into_iter().flatten().enumerate() {
            r.cursor = Some(e.id);
            if e.shape == Shape::Model {
                let k = cell_key(e.world_position(), cell_m);
                r.cell_of.insert(e.id, k);
                r.cells.entry(k).or_default().members.push(Member::of(e, None, cell_m));
            }
            if n + 1 >= least && n % 64 == 63 && started.elapsed() >= slice {
                done = false;
                break;
            }
        }
        if !done {
            return false;
        }
        let r = self.resync.take().expect("checked");
        self.cells = r.cells;
        self.cell_of = r.cell_of;
        for c in self.cells.values_mut() {
            c.bounds();
            c.dirty = true;
        }
        // Changes since it began come next, as a patch (entities met by the walk after they
        // changed are filed again: harmless).
        self.epoch = r.epoch;
        self.backlog.clear();
        self.resyncs += 1;
        true
    }

    /// The changed entities only; false when the world's change record doesn't reach back. An
    /// entity that stays in its cell is updated in place.
    fn patch(&mut self, world: &World, cell_m: f32, now: Instant, touched: &mut Vec<CellKey>, slice: Duration) -> bool {
        if self.epoch != world.draw_epoch() {
            let _z = specula_core::profile::zone("changes");
            let Some(changes) = world.draw_changes_since(self.epoch) else { return false };
            self.backlog.extend(changes);
            self.backlog.sort_unstable();
            self.backlog.dedup();
            self.epoch = world.draw_epoch();
        }
        let zf = specula_core::profile::zone("file");
        let started = Instant::now();
        let mut done = 0;
        for i in 0..self.backlog.len() {
            if i > 0 && started.elapsed() >= slice {
                break;
            }
            done = i + 1;
            let id = self.backlog[i];
            let now_in = world.get(id).filter(|e| e.shape == Shape::Model).map(|e| (cell_key(e.world_position(), cell_m), e));
            let old = self.cell_of.get(&id).copied();
            let boxed = |k: CellKey, mergeable: bool, touched: &mut Vec<CellKey>, rebound: &mut Vec<CellKey>| {
                rebound.push(k);
                if mergeable {
                    touched.push(k);
                }
            };
            if let (Some(old), Some((k, e))) = (old, now_in) {
                if old == k {
                    if let Some(c) = self.cells.get_mut(&k) {
                        if let Ok(i) = c.members.binary_search_by_key(&id, |m| m.id) {
                            let m = Member::of(e, Some(now), cell_m);
                            let mergeable = m.merges() || c.members[i].merges();
                            c.members[i] = m;
                            c.dirty = true;
                            boxed(k, mergeable, touched, &mut self.rebound);
                            continue;
                        }
                    }
                }
            }
            if let Some(old) = old {
                self.cell_of.remove(&id);
                if let Some(c) = self.cells.get_mut(&old) {
                    if let Ok(i) = c.members.binary_search_by_key(&id, |m| m.id) {
                        let gone = c.members.remove(i);
                        boxed(old, gone.merges(), touched, &mut self.rebound);
                    }
                    c.dirty = true;
                }
            }
            if let Some((k, e)) = now_in {
                self.cell_of.insert(id, k);
                let c = self.cells.entry(k).or_default();
                let at = c.members.binary_search_by_key(&id, |m| m.id).unwrap_or_else(|i| i);
                let m = Member::of(e, Some(now), cell_m);
                let mergeable = m.merges();
                c.members.insert(at, m);
                c.dirty = true;
                boxed(k, mergeable, touched, &mut self.rebound);
            }
        }
        self.backlog.drain(..done);
        drop(zf);
        let _zb = specula_core::profile::zone("rebound");
        touched.sort_unstable();
        touched.dedup();
        self.rebound.sort_unstable();
        self.rebound.dedup();
        for k in self.rebound.drain(..) {
            if let Some(c) = self.cells.get_mut(&k) {
                c.bounds();
            }
        }
        self.patches += 1;
        true
    }

    /// Re-reads the world boxes of members whose model handle is in `handles` (their builds
    /// landed: the boxes now span the models, not just their origins).
    fn rebox(&mut self, world: &World, handles: &[u32]) {
        for c in self.cells.values_mut() {
            let mut changed = false;
            for m in c.members.iter_mut().filter(|m| m.handle.is_some_and(|h| handles.binary_search(&h).is_ok())) {
                if let Some(e) = world.get(m.id) {
                    m.set_box(e.world_aabb(), self.cell_m);
                    changed = true;
                }
            }
            if changed {
                c.bounds();
                c.dirty = true;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Sources

#[derive(Clone, Copy, Debug, PartialEq)]
enum SourceKind {
    Window,
    Capture,
}

#[derive(Clone, Copy, Debug)]
struct Source {
    kind: SourceKind,
    eye: Vec3,
    target: Vec3,
    /// Orthographic: the distance at which a perspective camera of the same field of view
    /// sees the view's height (sizes on screen don't change with distance).
    ortho_d: Option<f32>,
    fov_y_deg: f32,
    viewport_h: u32,
    view_proj: Mat4,
    /// Which way it looks (unit).
    dir: Vec3,
    /// Capture sources expire.
    until: Option<Instant>,
}

impl Source {
    fn new(kind: SourceKind, cam: &Camera, w: u32, h: u32, until: Option<Instant>) -> Source {
        let ortho_d = match cam.projection {
            sight::Projection::Ortho { height_m } => Some(height_m * 0.5 / (cam.fov_y_deg.to_radians() * 0.5).tan().max(1e-6)),
            sight::Projection::Perspective => None,
        };
        let dir = (cam.target - cam.eye).normalize();
        Source { kind, eye: cam.eye, target: cam.target, ortho_d, fov_y_deg: cam.fov_y_deg, viewport_h: h, view_proj: cam.view_proj(w, h), dir, until }
    }

    /// The distance sizes on screen are worked out from: to the box from the eye; orthographic,
    /// [`Source::ortho_d`] for anything in view (plus how far from the view's centre otherwise,
    /// so what is near the view is wanted before what is far from it).
    fn dist(&self, lo: Vec3, hi: Vec3) -> f32 {
        match self.ortho_d {
            None => hlod::box_distance(self.eye, lo, hi),
            Some(d) if box_in_view(&self.view_proj, lo, hi) => d,
            Some(d) => d + hlod::box_distance(self.target, lo, hi),
        }
    }

    /// Priority weight: captures (an agent is waiting) before the window's view before the
    /// rest.
    fn weight(&self, lo: Vec3, hi: Vec3) -> f32 {
        match self.kind {
            SourceKind::Capture => 0.25,
            SourceKind::Window if box_in_view(&self.view_proj, lo, hi) => 0.5,
            SourceKind::Window => 1.0,
        }
    }
}

/// How many pixels across `built` at world scale `scale` is from `d` away.
fn built_px(built: &Built, scale: f32, d: f32, fov_y_deg: f32, viewport_h: u32) -> f32 {
    let (lo, hi) = built.bounds();
    let r = ((hi - lo).length() * 0.5).max(1e-3) * scale;
    crate::models::screen_px(r, d.max(0.0), fov_y_deg, viewport_h)
}

/// Whether any of the box can be in view (conservative, like `sight::in_view`).
fn box_in_view(vp: &Mat4, lo: Vec3, hi: Vec3) -> bool {
    let corners = [0u8, 1, 2, 3, 4, 5, 6, 7].map(|i| {
        let p = Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z });
        vp.mul_vec4(p.extend(1.0))
    });
    let all = |f: &dyn Fn(&specula_core::math::Vec4) -> bool| corners.iter().all(f);
    !(all(&|p| p.w <= 0.0) || all(&|p| p.x < -p.w) || all(&|p| p.x > p.w) || all(&|p| p.y < -p.w) || all(&|p| p.y > p.w))
}

// ---------------------------------------------------------------------------------------------
// The streamer

/// A pass of the wanted set in progress.
struct PassState {
    /// The last cell done (`None`: none yet).
    cursor: Option<CellKey>,
    /// Cells whose proxy left members out: their handles are worked out again.
    left_out: Vec<CellKey>,
    /// Cells done first, out of turn (wherever the cursor is): edits in cells the window draws
    /// as a proxy, or in a capture's view.
    urgent: Vec<CellKey>,
    /// Only the urgent cells (no pass was running): done, the pass state goes again.
    urgent_only: bool,
    /// The window's distance to its nearest cell so far.
    nearest: f32,
    /// How far its end got: 0 still walking cells, 1 the pump's list made (the wanted set is
    /// this pass's), then the tidying (see [`Streamer::step`]).
    end: u8,
}

/// Something the memory budget may evict.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Victim {
    Build(u64),
    Proxy(CellKey),
}

impl Victim {
    fn of(kind: u8, id: u64, cell: CellKey) -> Victim {
        if kind == 0 {
            Victim::Build(id)
        } else {
            Victim::Proxy(cell)
        }
    }
}

/// The eviction queue (see [`Streamer::scan_victims`]): uploads no camera wants, least recently
/// wanted first, then soft-wanted ones, farthest first. Heaps: made in O(n), taken from in
/// O(log n), so no frame sorts every upload.
#[derive(Default)]
struct Victims {
    /// `(last wanted, kind (0 build, 1 proxy), build id, cell)`: the smallest goes first.
    gone: BinaryHeap<Reverse<(u64, u8, u64, CellKey)>>,
    /// `(priority as bits (never negative, so ordered as the floats), kind, build id, cell)`: the
    /// largest (farthest) goes first.
    soft: BinaryHeap<(u32, u8, u64, CellKey)>,
}

impl Victims {
    /// The next victim and the priority it gives way to (unwanted ones: infinity).
    fn front(&self) -> Option<(f32, Victim)> {
        if let Some(Reverse((_, kind, id, cell))) = self.gone.peek() {
            return Some((f32::INFINITY, Victim::of(*kind, *id, *cell)));
        }
        self.soft.peek().map(|&(bits, kind, id, cell)| (f32::from_bits(bits), Victim::of(kind, id, cell)))
    }

    fn pop(&mut self) {
        if self.gone.pop().is_none() {
            self.soft.pop();
        }
    }
}

/// A scan of the uploads for the next eviction queue, spread over frames after each pass.
struct VictimScan {
    /// The pass whose wanted set it reads.
    pass: u64,
    /// The last build looked at.
    cursor: Option<u64>,
    /// Every build was looked at: the proxies and the heaps are next.
    builds_done: bool,
    gone: Vec<Reverse<(u64, u8, u64, CellKey)>>,
    soft: Vec<(u32, u8, u64, CellKey)>,
}

/// What one frame's pump did.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameUpload {
    pub items: u32,
    pub bytes: u64,
    pub ms: f64,
    /// Wanted items left for later frames by the budget.
    pub deferred: u32,
    /// Wanted items that didn't fit the memory budget even after eviction.
    pub memory_blocked: u32,
    pub evicted: u32,
    /// The frame's slowest item: `(ms, KB, what)`.
    pub slowest: (f64, u64, &'static str),
}

/// Streaming's state (see the module docs).
pub struct Streamer {
    kind: String,
    /// stream_settings' keys over the perf.toml settings, and who set them last.
    overrides: Vec<(String, Json)>,
    set_by: Option<String>,
    pub cfg: StreamConfig,
    index: Index,
    pub hlod: Hlod,
    /// Live builds (the streamed path's view of what entities use).
    live: BTreeMap<u64, Arc<Built>>,
    /// Builds wanted at some update: `(priority, the update it was last wanted in, hard)`;
    /// wanted now when that is the latest update. Lower priority first; soft ones stand in for
    /// a proxy and may be evicted for anything nearer.
    want: IdMap<u64, (f32, u64, bool, usize)>,
    /// Builds wanted now and not uploaded down to the finest level needed, by priority:
    /// `(priority, build, hard, level)` (made at each update; the pump skips what finished).
    pending: Vec<(f32, u64, bool, usize)>,
    sources: Vec<Source>,
    updates: u64,
    last_update: Option<Instant>,
    force_update: bool,
    /// The world or the builds changed since the last update (it runs within UPDATE_EVERY).
    stale: bool,
    /// Where the window's eye was, which way it looked and its field of view at the last pass.
    window_eye: Option<(Vec3, Vec3, f32)>,
    /// The running pass of the wanted set (spread over frames).
    pass: Option<PassState>,
    /// The last finished pass: builds wished for in it (or the running one) are wanted now.
    done_pass: u64,
    /// The window's distance to its nearest cell at the last pass (how far it may move before
    /// the wanted levels need working out again).
    nearest: f32,
    /// The window's camera moved (or turned) since the last update.
    window_moved: bool,
    pub frame: FrameUpload,
    /// Upload ms per frame that uploaded something.
    upload_ms: Series,
    /// Milliseconds per item and per MB uploaded (smoothed): estimate an item's cost before
    /// doing it.
    ms_per_item: f64,
    ms_per_mb: f64,
    totals: FrameUpload,
    /// 2D textures (sprites, tilesets) uploaded within the budget, and frames that held one back.
    textures_2d: FrameUpload,
    frames_over: u64,
    misses: VecDeque<Json>,
    /// Bytes wanted but blocked by the memory budget at the last pump.
    blocked_bytes: u64,
    /// Upload failures since the last sync (reported like the plain path's).
    upload_errors: Vec<String>,
    /// The frame's proxy draws (reused: no allocation per frame once warm).
    pub scratch: Vec<DrawItem>,
    /// The last capture's proxied cells and covered entities.
    last_capture: (u32, u32),
    /// Upload ms spent since the window's (or the headless loop's) last frame: captures' pumps
    /// share the frame's budget instead of each getting one.
    spent_ms: f64,
    /// Items and bytes uploaded since then (the first item of a frame always goes; later ones
    /// only within the budget, whichever pump runs them).
    frame_items: u32,
    frame_bytes: u64,
    /// Builds and proxies the memory budget kept out at the last pump (sorted): captures don't
    /// wait for them, and a headless loop doesn't spin for them.
    blocked: Vec<u64>,
    blocked_cells: Vec<CellKey>,
    /// Cells the last index patch touched (reused).
    touched: Vec<CellKey>,
    /// Cells changed (edited, or a member's build rebuilt) that no pass has looked at since:
    /// the window's proxies of them are redone out of turn, captures run a whole pass first,
    /// and no capture draws their old proxy.
    dirty_cells: std::collections::BTreeSet<CellKey>,
    /// Transient model entities' builds not on the GPU yet (a game's presentation): uploaded
    /// first, without waiting for a pass.
    transients: Vec<u64>,
    /// When the current upload budget began (see [`FRAME_STALE`]).
    frame_began: Option<Instant>,
    /// When growing mesh memory ahead last failed (tried again a second later).
    grow_failed: Option<Instant>,
    /// Items the frame's pumps tried (uploaded, blocked, failed or put off): the first goes
    /// whatever the budget, the rest only within it.
    frame_tried: u32,
    /// Builds drawn instanced (at least [`INSTANCED_MIN`] entities): never merged.
    instanced: std::collections::BTreeSet<u64>,
    /// The eviction queue (see `victims`), kept across frames until the next pass.
    victims_cache: Option<Victims>,
    /// The next eviction queue being made (see [`Streamer::scan_victims`]).
    victim_scan: Option<VictimScan>,
    /// The pass the eviction queue was made for (`None`: builds changed since).
    victims_for: Option<u64>,
    /// The memory budget was full at the last pump: eviction queues are worth keeping fresh.
    memory_full: bool,
    /// Bytes on the GPU (builds and proxies) as of the last pump, kept up to date by it; `None`:
    /// worked out again (after a sync, a pass end or new settings).
    resident_cache: Option<u64>,
    /// Bytes the terrain renderer's height, normal and splat sets hold (set by the engine each
    /// frame): they count against `memory_mb` too.
    pub terrain_bytes: u64,
}

impl Default for Streamer {
    fn default() -> Streamer {
        Streamer::new(StreamTable::default(), "integrated")
    }
}

impl Streamer {
    pub fn new(table: StreamTable, kind: &str) -> Streamer {
        let cfg = table.config(kind);
        Streamer {
            kind: kind.to_owned(),
            overrides: Vec::new(),
            set_by: None,
            cfg,
            index: Index::default(),
            hlod: Hlod::default(),
            live: BTreeMap::new(),
            want: IdMap::default(),
            pending: Vec::new(),
            sources: Vec::new(),
            updates: 0,
            last_update: None,
            force_update: true,
            stale: false,
            window_eye: None,
            pass: None,
            done_pass: 0,
            nearest: f32::INFINITY,
            window_moved: false,
            frame: FrameUpload::default(),
            upload_ms: Series::default(),
            ms_per_item: 0.1,
            ms_per_mb: 2.0,
            totals: FrameUpload::default(),
            textures_2d: FrameUpload::default(),
            frames_over: 0,
            misses: VecDeque::new(),
            blocked_bytes: 0,
            upload_errors: Vec::new(),
            scratch: Vec::new(),
            last_capture: (0, 0),
            spent_ms: 0.0,
            frame_items: 0,
            frame_bytes: 0,
            blocked: Vec::new(),
            blocked_cells: Vec::new(),
            touched: Vec::new(),
            dirty_cells: Default::default(),
            transients: Vec::new(),
            instanced: Default::default(),
            frame_began: None,
            frame_tried: 0,
            grow_failed: None,
            victims_cache: None,
            victim_scan: None,
            victims_for: None,
            memory_full: false,
            resident_cache: None,
            terrain_bytes: 0,
        }
    }

    /// Work waiting for later frames (a headless engine loops sooner meanwhile).
    pub fn busy(&self) -> bool {
        let (_, _, _, building, _) = self.hlod.counts();
        self.cfg.enabled && (self.frame.deferred > 0 || building > 0 || self.hlod.uploads().any(|(_, cell, _)| self.blocked_cells.binary_search(&cell).is_err()))
    }

    pub fn enabled(&self) -> bool {
        self.cfg.enabled
    }

    /// Who last changed the settings with stream_settings.
    pub fn set_by(&self) -> Option<&str> {
        self.set_by.as_deref()
    }

    fn hlod_on(&self) -> bool {
        self.cfg.enabled && self.cfg.hlod
    }

    /// The world was replaced (a load, a save read): merged proxies show the old world and cover
    /// ids the new one reuses, so they go and are built again from the new world
    /// (`Engine::world_changed_wholesale`).
    pub fn world_replaced(&mut self, gpu: &mut Gpu, models: &mut GpuModels) {
        self.hlod.clear(gpu);
        self.index = Index::default();
        self.dirty_cells.clear();
        models.cover.fill(0);
        self.resident_cache = None;
        self.victims_cache = None;
        self.victim_scan = None;
        self.victims_for = None;
    }

    /// stream_settings: lays `patch` over the settings (checked first; nothing changes on an
    /// error). Switching streaming or HLOD off, or a new cell size, drops proxies and partial
    /// uploads (`models.synced` is reset so the plain path uploads everything).
    pub fn set(&mut self, patch: &Json, by: &str, gpu: &mut Gpu, models: &mut GpuModels) -> Result<(), String> {
        let mut next = self.cfg.clone();
        next.apply(patch, "stream_settings")?;
        for (k, v) in patch.as_object().unwrap_or_default() {
            self.overrides.retain(|(o, _)| o != k);
            self.overrides.push((k.clone(), v.clone()));
        }
        self.set_by = Some(by.to_owned());
        let old = std::mem::replace(&mut self.cfg, next);
        if old.enabled != self.cfg.enabled || old.hlod != self.cfg.hlod || old.cell_m != self.cfg.cell_m {
            self.hlod.clear(gpu);
            self.index = Index::default();
            self.dirty_cells.clear();
            models.cover.fill(0);
        }
        // Proxies may have gone: what is on the GPU is summed again, the queue made again.
        self.resident_cache = None;
        self.victims_cache = None;
        self.victim_scan = None;
        self.victims_for = None;
        if old.enabled && !self.cfg.enabled {
            // The plain path uploads whole builds: partial ones go and come back complete.
            let partial: Vec<u64> = models.uploaded.iter().filter(|(_, m)| m.resident_from > 0).map(|(id, _)| *id).collect();
            for id in partial {
                evict_build(gpu, models, id);
            }
            models.synced = None;
        }
        if !old.enabled && self.cfg.enabled {
            models.synced = None;
        }
        self.force_update = true;
        Ok(())
    }

    /// The window's camera this frame (`None`: minimized or headless).
    pub fn set_window(&mut self, camera: Option<(Camera, u32, u32)>) {
        self.sources.retain(|s| s.kind != SourceKind::Window);
        if let Some((cam, w, h)) = camera {
            let dir = (cam.target - cam.eye).normalize();
            // Moved enough to matter: a quarter cell, or a tenth of the way to the nearest cell
            // (levels of detail change fastest up close), a turn, or a zoom.
            let step = (self.nearest * 0.1).clamp(0.5, self.cfg.cell_m * 0.25);
            self.window_moved = match self.window_eye {
                None => true,
                Some((e, d, fov)) => (e - cam.eye).length() > step || d.dot(dir) < 0.97 || (fov - cam.fov_y_deg).abs() > 1.0,
            };
            // A jump (a teleport, a new view) is wanted at once.
            self.force_update |= self.window_eye.is_none_or(|(e, ..)| (e - cam.eye).length() > self.cfg.cell_m * 2.0);
            self.sources.push(Source::new(SourceKind::Window, &cam, w, h, None));
        }
    }

    /// A capture through `camera` at `w × h`: a source for a while, wanted first.
    pub fn capture_source(&mut self, camera: Camera, w: u32, h: u32) {
        if !self.cfg.enabled {
            return;
        }
        let until = Some(Instant::now() + CAPTURE_SOURCE_FOR);
        // A capture from about where an earlier one was (an agent's repeated look) needs no
        // update of its own at once: within UPDATE_EVERY is soon enough.
        let known = self.sources.iter().any(|s| s.kind == SourceKind::Capture && (s.eye - camera.eye).length() < 1.0);
        self.sources.retain(|s| !(s.kind == SourceKind::Capture && (s.eye - camera.eye).length() < 0.01));
        let mut captures: Vec<usize> = (0..self.sources.len()).filter(|&i| self.sources[i].kind == SourceKind::Capture).collect();
        while captures.len() >= MAX_CAPTURE_SOURCES {
            let oldest = captures.remove(0);
            self.sources.remove(oldest);
            captures = (0..self.sources.len()).filter(|&i| self.sources[i].kind == SourceKind::Capture).collect();
        }
        self.sources.push(Source::new(SourceKind::Capture, &camera, w, h, until));
        if known {
            self.stale = true;
        } else {
            self.force_update = true;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Builds: the streamed path's sync (instead of GpuModels::sync)

    /// When the world's model entities or the library changed: maps handles to builds, retires
    /// uploads no entity uses any more and remembers the live builds for [`Streamer::pump`].
    pub fn sync_models(&mut self, models: &mut GpuModels, gpu: &mut Gpu, library: &SharedLibrary, world: &World, world_generation: u64) -> Vec<String> {
        models.ready = gpu.pipeline_variants();
        // The world's anim assets (agent-written clips and graphs) laid over the builds' sets,
        // as GpuModels::sync does.
        models.refresh_anim_env(library, world, world_generation);
        let lib = library.lock();
        let key = (world.mesh_epoch(), lib.generation(), world_generation);
        if models.synced == Some(key) {
            if models.anim_sets_stale() {
                models.sync_anim_sets(&lib, world, false);
            }
            return std::mem::take(&mut self.upload_errors);
        }
        models.synced = Some(key);
        let live = crate::models::live_handles(world);
        let old = std::mem::take(&mut models.by_handle);
        self.live.clear();
        for &h in &live {
            let Some(b) = lib.built(h) else { continue };
            if models.by_handle.len() <= h as usize {
                models.by_handle.resize(h as usize + 1, None);
            }
            models.by_handle[h as usize] = Some(b.id);
            self.live.insert(b.id, Arc::clone(b));
        }
        drop(lib);
        // Handles rebuilt (an edited recipe): the cells using them merge again. Builds landing
        // for the first time are picked up by the cells waiting on them (`Content::waiting_on`).
        let rebuilt: Vec<u32> = (0..old.len().min(models.by_handle.len()))
            .filter(|&h| matches!((old[h], models.by_handle[h]), (Some(a), Some(b)) if a != b))
            .map(|h| h as u32)
            .collect();
        if !rebuilt.is_empty() {
            for (k, c) in self.index.cells.iter_mut() {
                if c.members.iter().any(|m| m.handle.is_some_and(|h| rebuilt.binary_search(&h).is_ok())) {
                    c.dirty = true;
                    self.dirty_cells.insert(*k);
                }
            }
        }
        // Builds many shown entities use draw instanced: no proxy merges them (cells whose builds
        // crossed the line merge again). Hidden and interface entities aren't drawn instanced.
        let _zi = specula_core::profile::zone("instanced");
        let mut uses: HashMap<u64, u32> = HashMap::new();
        for e in world.persistent_entities().filter(|e| e.shape == Shape::Model && sight::in_frame(e, false)) {
            if let Some(b) = e.resolved_model().and_then(|r| models.by_handle.get(r.handle as usize).copied().flatten()) {
                *uses.entry(b).or_default() += 1;
            }
        }
        let instanced: std::collections::BTreeSet<u64> = uses.into_iter().filter(|&(_, n)| n >= INSTANCED_MIN).map(|(b, _)| b).collect();
        if instanced != self.instanced {
            let crossed: std::collections::BTreeSet<u64> = instanced.symmetric_difference(&self.instanced).copied().collect();
            for (k, c) in self.index.cells.iter_mut() {
                if c.members.iter().any(|m| m.handle.and_then(|h| models.by_handle.get(h as usize).copied().flatten()).is_some_and(|b| crossed.contains(&b))) {
                    c.dirty = true;
                    self.dirty_cells.insert(*k);
                }
            }
            self.instanced = instanced;
        }
        drop(_zi);
        // Builds that landed (or changed): their members' boxes now span the models.
        let reboxed: Vec<u32> = (0..models.by_handle.len())
            .filter(|&h| models.by_handle[h].is_some() && old.get(h).copied().flatten() != models.by_handle[h])
            .map(|h| h as u32)
            .collect();
        if !reboxed.is_empty() {
            self.index.rebox(world, &reboxed);
        }
        // (Pinned builds stay: the asset studio renders them between frames; see GpuModels::set_pinned.)
        let stale: Vec<u64> = models.uploaded.keys().copied().filter(|id| !self.live.contains_key(id) && !models.is_pinned(*id)).collect();
        let mut released = Vec::new();
        for id in stale {
            if evict_build(gpu, models, id) {
                released.push(id);
            }
        }
        if !released.is_empty() {
            let mut lib = library.lock();
            for id in released {
                lib.forget_build(id);
            }
        }
        let live = &self.live;
        models.failed.retain(|id, _| live.contains_key(id));
        models.sync_anim_sets(&library.lock(), world, false);
        // Builds changed: a new eviction queue is scanned over the next frames (the old one, and a
        // scan already running, stay meanwhile; `evict_for` checks each entry again).
        self.victims_for = None;
        self.resident_cache = None;
        self.stale = true;
        std::mem::take(&mut self.upload_errors)
    }

    // -----------------------------------------------------------------------------------------
    // The frame

    /// One frame (or headless loop): files changed entities, recomputes what is wanted (at most
    /// every [`UPDATE_EVERY`]), schedules proxy builds and uploads within the frame's budget.
    ///
    /// `new_frame`: the window's frame (or a headless loop; after [`Streamer::begin_frame`]):
    /// passes are sliced; a capture's call (false) runs them whole. Both use what is left of the
    /// frame's upload budget.
    pub fn frame(&mut self, gpu: &mut Gpu, models: &mut GpuModels, library: &SharedLibrary, world: &World, generation: u64, new_frame: bool) {
        let now = Instant::now();
        self.frame = FrameUpload::default();
        if !self.cfg.enabled {
            return;
        }
        // Nothing has started a frame for a while: this call's budget is a fresh one.
        if !new_frame && self.frame_began.is_none_or(|t| now.duration_since(t) > FRAME_STALE) {
            self.begin_frame();
        }
        let _z = specula_core::profile::zone("stream");
        self.sources.retain(|s| s.until.is_none_or(|t| t > now));
        let mut touched = std::mem::take(&mut self.touched);
        let changed = {
            let _z = specula_core::profile::zone("index");
            // A capture files every change now (it must see what was just spawned); frames file
            // a slice of the budget's worth.
            let slice = if new_frame { Duration::from_secs_f64(self.cfg.upload_ms * 0.25 / 1e3) } else { Duration::MAX };
            self.index.sync(world, generation, self.cfg.cell_m, now, &mut touched, slice)
        };
        self.dirty_cells.extend(touched.iter().copied());
        // (Cells gone with an index rebuild are no longer anyone's concern.)
        let cells = &self.index.cells;
        self.dirty_cells.retain(|k| cells.contains_key(k));
        // A changed cell the window draws as a proxy, or one in a capture's view: its cover, proxy
        // and wanted builds change at once (an entity must not stay hidden, nor show at its old
        // place, nor a capture show the world as it was). Done out of turn at the next step,
        // without restarting a pass (edits every frame would otherwise keep it from finishing).
        let in_capture = |k: &CellKey| {
            !new_frame && cells.get(k).is_some_and(|c| self.sources.iter().any(|s| s.kind == SourceKind::Capture && box_in_view(&s.view_proj, c.lo, c.hi)))
        };
        let urgent: Vec<CellKey> = self.dirty_cells.iter().copied().filter(|k| self.hlod.window_on(*k) || in_capture(k)).collect();
        self.touched = touched;
        // Finished proxy builds are taken every frame (their uploads go by priority).
        self.hlod.collect();
        let since = self.last_update.map_or(Duration::MAX, |t| now.duration_since(t));
        self.stale |= changed;
        let due = self.force_update || since >= UPDATE_IDLE || ((self.window_moved || self.stale) && since >= UPDATE_EVERY);
        let whole = self.force_update;
        // A forced pass (a new capture view, a jump) starts afresh, so it sees every source.
        if due && (self.pass.is_none() || whole) {
            self.begin_pass(now);
        }
        let any_urgent = !urgent.is_empty();
        match &mut self.pass {
            Some(pass) => pass.urgent.extend(urgent),
            // No pass running: only those cells, against the last pass's wanted set.
            None if any_urgent => self.pass = Some(PassState { cursor: None, left_out: Vec::new(), urgent, urgent_only: true, nearest: f32::INFINITY, end: 0 }),
            None => {}
        }
        if self.pass.is_some() {
            let _z = specula_core::profile::zone("want");
            // A capture (or a jump) gets its whole pass at once (a capture also finishes a
            // running one: it reads the wanted levels); otherwise a slice of the budget.
            let slice = if whole || !new_frame { f64::INFINITY } else { self.cfg.upload_ms * 0.5 };
            self.step(world, models, library, gpu, now, slice);
            // What those cells now want uploads before the pass ends.
            if any_urgent {
                self.pend_wanted(models);
            }
        }
        // The next eviction queue, a slice a frame, while the memory budget is full.
        if self.memory_full && self.victims_cache.is_some() && (self.victims_for != Some(self.done_pass) || self.victim_scan.is_some()) {
            let _z = specula_core::profile::zone("victims_scan");
            self.scan_victims(models, self.cfg.upload_ms * 0.1);
        }
        // A game's presentation that isn't on the GPU yet goes first, without waiting for a pass.
        self.find_transients(world, models);
        // The frame's streaming work so far (filing, the wanted set) counts against the upload budget.
        self.pump(gpu, models, library, now);
        self.spent_ms += now.elapsed().as_secs_f64() * 1e3;
    }

    /// Starts a pass of the wanted set over every cell (see [`Streamer::step`]).
    fn begin_pass(&mut self, now: Instant) {
        self.updates += 1;
        self.last_update = Some(now);
        self.force_update = false;
        self.stale = false;
        self.window_eye = self.sources.iter().find(|s| s.kind == SourceKind::Window).map(|s| (s.eye, s.dir, s.fov_y_deg));
        self.window_moved = false;
        // A pass restarted before it got everywhere keeps the cells it still had to redo.
        let (mut left_out, urgent) = self.pass.take().map(|p| (p.left_out, p.urgent)).unwrap_or_default();
        left_out.extend(self.hlod.take_left_out());
        left_out.sort_unstable();
        left_out.dedup();
        self.pass = Some(PassState { cursor: None, left_out, urgent, urgent_only: false, nearest: f32::INFINITY, end: 0 });
    }

    /// Works through the running pass for at most `slice_ms` (at least one cell): each cell's
    /// HLOD content (when it changed), its handles and what its members and proxy are wanted
    /// for. The last cell finishes the pass: transient models, the pending list, proxy builds.
    /// Spread over frames this way, a big world's update never costs one frame more than its
    /// slice; captures run a whole pass at once.
    fn step(&mut self, world: &World, models: &GpuModels, library: &SharedLibrary, gpu: &mut Gpu, now: Instant, slice_ms: f64) {
        let Some(mut pass) = self.pass.take() else { return };
        let started = Instant::now();
        let frame = self.updates;
        let hlod_on = self.hlod_on();
        let cfg = self.cfg.clone();
        let lib = library.lock();
        let live = &self.live;
        let build_of = |h: Option<u32>| h.and_then(|h| models.by_handle.get(h as usize).copied().flatten());
        // The finest level a camera at `d` (field of view, viewport height) would draw `b` at,
        // with a world scale of `scale`: as the window's hysteresis keeps a finer level down to
        // (1 - LOD_HYSTERESIS) of its threshold, so is it wanted.
        let level_at = |b: u64, scale: f32, d: f32, fov: f32, vh: u32| -> usize {
            let Some(built) = live.get(&b) else { return 0 };
            let (lo, hi) = built.bounds();
            let r = ((hi - lo).length() * 0.5).max(1e-3) * scale;
            let px = crate::models::screen_px(r, d.max(0.0), fov, vh) / (1.0 - crate::models::LOD_HYSTERESIS);
            crate::models::select_lod(px, built.model.lods.len(), None)
        };
        // How many pixels across `b` (world scale `scale`) is at `d`.
        let px_at = |b: u64, scale: f32, d: f32, fov: f32, vh: u32| -> f32 { live.get(&b).map_or(0.0, |built| built_px(built, scale, d, fov, vh)) };
        let want = &mut self.want;
        // `hard`: a camera would draw it (or it is a game's presentation); soft: it stands in
        // for a proxy that isn't there yet, and gives way to anything nearer or hard-wanted.
        // `level`: the finest LOD it needs (a far camera never needs LOD 0 uploaded).
        let mut wish = |b: u64, p: f32, hard: bool, level: usize| {
            let e = want.entry(b).or_insert((p, frame, hard, level));
            if e.1 != frame {
                *e = (p, frame, hard, level);
            } else {
                e.0 = e.0.min(p);
                e.2 |= hard;
                e.3 = e.3.min(level);
            }
        };
        // The sources a cell's members are wanted by: `(priority, hard, distance, source)`.
        let mut members_by: Vec<(f32, bool, f32, usize)> = Vec::new();
        let mut others_by: Vec<(f32, bool, f32, usize)> = Vec::new();
        // Sources beyond the reach of `members_by` / `others_by`: `(priority, distance, source)`.
        let mut far_members: Vec<(f32, f32, usize)> = Vec::new();
        let mut far_others: Vec<(f32, f32, usize)> = Vec::new();
        let mut done = false;
        loop {
            let k = if let Some(k) = pass.urgent.pop() {
                // Out of turn (visited again in turn if the cursor hasn't passed it: harmless).
                if !self.index.cells.contains_key(&k) {
                    self.dirty_cells.remove(&k);
                    continue;
                }
                k
            } else if pass.urgent_only {
                // Only the urgent cells: the wanted set stays the last pass's otherwise.
                self.pass = None;
                return;
            } else {
                let next = match pass.cursor {
                    None => self.index.cells.keys().next().copied(),
                    Some(k) => self.index.cells.range((std::ops::Bound::Excluded(k), std::ops::Bound::Unbounded)).next().map(|(k, _)| *k),
                };
                let Some(k) = next else {
                    done = true;
                    break;
                };
                pass.cursor = Some(k);
                k
            };
            self.dirty_cells.remove(&k);
            let c = self.index.cells.get_mut(&k).expect("listed");
            if c.members.is_empty() {
                continue;
            }
            // HLOD content of a changed cell, or of one waiting for builds that landed.
            let waiting = self
                .hlod
                .content(k)
                .is_some_and(|ct| !ct.complete && ct.waiting_on.iter().any(|&h| lib.built(h).is_some() || lib.handle_error(h).is_some()));
            if hlod_on && (c.dirty || waiting) {
                let content = content_of(c, &lib, world, &self.instanced);
                if self.hlod.content(k) != Some(&content) {
                    self.hlod.set_content(k, content);
                }
            }
            if c.dirty || waiting || pass.left_out.binary_search(&k).is_ok() {
                // What the proxy holds: the uploaded one's members (a full proxy leaves some
                // out: they draw on their own), else what the next one will hold.
                let merged = self.hlod.proxy_members(k);
                c.handles.clear();
                for m in c.members.iter().filter(|m| !m.wide) {
                    if let Some(h) = m.handle {
                        c.handles.push((h, merged.binary_search(&m.id).is_ok(), max_scale(&m.world)));
                    }
                }
                c.handles.sort_by_key(|&(h, merged, _)| (h, merged));
                c.handles.dedup_by(|a, b| {
                    let same = (a.0, a.1) == (b.0, b.1);
                    if same {
                        b.2 = b.2.max(a.2);
                    }
                    same
                });
            }
            c.dirty = false;
            let c = &self.index.cells[&k];
            // What it is wanted for.
            members_by.clear();
            others_by.clear();
            far_members.clear();
            far_others.clear();
            let mut proxy_p: Option<f32> = None;
            let current = hlod_on && self.hlod.has_current(k);
            let content = self.hlod.content(k);
            let merging = hlod_on && content.is_some_and(|ct| ct.key != 0);
            // The biggest member by the same estimate the size test uses (`built_px`), so the
            // gate never drops one the test (or a capture's wait) would count.
            let cell_r = c
                .handles
                .iter()
                .filter_map(|&(h, _, scale)| build_of(Some(h)).and_then(|b| live.get(&b)).map(|built| (built.bounds(), scale)))
                .map(|((lo, hi), scale)| ((hi - lo).length() * 0.5).max(1e-3) * scale)
                .fold(1e-3f32, f32::max);
            for (si, s) in self.sources.iter().enumerate() {
                let d = s.dist(c.lo, c.hi);
                if s.kind == SourceKind::Window {
                    pass.nearest = pass.nearest.min(d);
                }
                let p = d * s.weight(c.lo, c.hi);
                // Within radius_m everything is wanted; beyond, what is big enough on screen (none
                // of the cell's members can be bigger than its box).
                let reach = crate::models::screen_px(cell_r, d, s.fov_y_deg, s.viewport_h) >= RESIDENT_MIN_PX;
                if d < cfg.radius_m {
                    others_by.push((p, true, d, si));
                } else if reach {
                    far_others.push((p, d, si));
                }
                // Orthographic views never draw proxies (they are for perspective distances).
                let merging = merging && s.ortho_d.is_none();
                if merging && d >= cfg.hlod_radius_m {
                    if reach {
                        far_members.push((p, d, si));
                    }
                } else if merging {
                    // Wanted wherever this camera wouldn't draw the proxy (within preload x the
                    // switch distance, whatever radius_m says: no band where nothing draws). A
                    // cell whose proxy is stale or still coming has its members stand in for it,
                    // softly beyond that, so it never vanishes.
                    let sw = content.map_or(0.0, |ct| hlod::switch_distance(ct.k, s.fov_y_deg, s.viewport_h, cfg.hlod_min_m));
                    if current {
                        if d <= sw * cfg.preload {
                            members_by.push((p, true, d, si));
                        }
                    } else {
                        members_by.push((p, d < cfg.radius_m || d <= sw * cfg.preload, d, si));
                    }
                } else if d < cfg.radius_m {
                    members_by.push((p, true, d, si));
                } else if reach {
                    far_members.push((p, d, si));
                }
                if hlod_on && s.ortho_d.is_none() && d < cfg.hlod_radius_m {
                    let pp = p * 0.5;
                    proxy_p = Some(proxy_p.map_or(pp, |q: f32| q.min(pp)));
                }
            }
            if !members_by.is_empty() || !others_by.is_empty() || !far_members.is_empty() || !far_others.is_empty() {
                for &(h, in_proxy, scale) in &c.handles {
                    let Some(b) = build_of(Some(h)) else { continue };
                    for &(p, hard, d, si) in if in_proxy { &members_by } else { &others_by } {
                        // A soft stand-in only needs the coarsest level (the proxy's).
                        let s = &self.sources[si];
                        let level = if hard { level_at(b, scale, d, s.fov_y_deg, s.viewport_h) } else { usize::MAX };
                        wish(b, p, hard, level);
                    }
                    // Beyond radius_m (or the proxies' reach): wanted where it is big enough on
                    // screen to see.
                    for &(p, d, si) in if in_proxy { &far_members } else { &far_others } {
                        let s = &self.sources[si];
                        if px_at(b, scale, d, s.fov_y_deg, s.viewport_h) >= RESIDENT_MIN_PX {
                            wish(b, p, true, level_at(b, scale, d, s.fov_y_deg, s.viewport_h));
                        }
                    }
                }
            }
            // Wide members by their own boxes.
            if c.has_wide {
                for m in c.members.iter().filter(|m| m.wide) {
                    let Some(b) = build_of(m.handle) else { continue };
                    for s in &self.sources {
                        let d = s.dist(m.lo, m.hi);
                        let scale = max_scale(&m.world);
                        if d < cfg.radius_m || px_at(b, scale, d, s.fov_y_deg, s.viewport_h) >= RESIDENT_MIN_PX {
                            wish(b, d * s.weight(m.lo, m.hi), true, level_at(b, scale, d, s.fov_y_deg, s.viewport_h));
                        }
                    }
                }
            }
            if hlod_on {
                self.hlod.set_wanted(k, proxy_p, frame);
            }
            if started.elapsed().as_secs_f64() * 1e3 >= slice_ms {
                break;
            }
        }
        if !done {
            drop(lib);
            self.pass = Some(pass);
            return;
        }
        // The pass is done: its end comes in two steps, each in a call of its own when the slice
        // is used up (a capture's call does both).
        let _z = specula_core::profile::zone("pass_end");
        if pass.end == 0 {
            let _z = specula_core::profile::zone("pending");
            // A game's presentation (transient models) comes first.
            for e in world.transient_entities().filter(|e| e.shape == Shape::Model) {
                if let Some(b) = build_of(e.resolved_model().map(|r| r.handle)) {
                    wish(b, 0.0, true, 0);
                }
            }
            if self.want.len() > live.len() * 2 + 64 {
                self.want.retain(|b, _| live.contains_key(b));
            }
            // What the pump works through until the next pass: down to the finest level needed.
            self.pending.clear();
            for (b, &(p, f, hard, level)) in &self.want {
                if f == frame && live.contains_key(b) && models.uploaded.get(b).is_none_or(|m| m.resident_from > level) {
                    self.pending.push((p, *b, hard, level));
                }
            }
            self.pending.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            self.done_pass = frame;
            self.nearest = pass.nearest;
            pass.end = 1;
            if started.elapsed().as_secs_f64() * 1e3 >= slice_ms {
                drop(lib);
                self.pass = Some(pass);
                return;
            }
        }
        // The tidying: cells gone, their proxies, proxies no camera wants, new proxy builds.
        let _zt = specula_core::profile::zone("tidy");
        self.index.cells.retain(|_, c| !c.members.is_empty());
        // Proxies retired here leave the running resident total (not summed again).
        let proxies_before = self.hlod.resident_bytes();
        let cells = &self.index.cells;
        self.hlod.retain(|k| cells.contains_key(&k), gpu);
        if hlod_on {
            // Proxies built for cells no camera wants any more go (CPU data and part uploads).
            self.hlod.drop_unwanted(gpu);
            let cells = &self.index.cells;
            self.hlod.schedule(now, self.cfg.settle_s, |key, content| inputs_of(cells.get(&key)?, content, &lib));
        }
        let gone = proxies_before.saturating_sub(self.hlod.resident_bytes());
        if let Some(r) = self.resident_cache.as_mut() {
            *r = r.saturating_sub(gone);
        }
    }

    /// Adds what the running pass wants so far (and isn't on the GPU at the level wanted) to the
    /// pump's list, which is otherwise made at a pass's end: cells done out of turn upload now.
    fn pend_wanted(&mut self, models: &GpuModels) {
        let frame = self.updates;
        let listed: std::collections::BTreeSet<u64> = self.pending.iter().map(|p| p.1).collect();
        let live = &self.live;
        let mut added = false;
        for (b, &(p, f, hard, level)) in &self.want {
            if f == frame && !listed.contains(b) && live.contains_key(b) && models.uploaded.get(b).is_none_or(|m| m.resident_from > level) {
                self.pending.push((p, *b, hard, level));
                added = true;
            }
        }
        if added {
            self.pending.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        }
    }

    /// Transient model entities (a game's presentation: creatures, effects) whose builds aren't
    /// fully on the GPU: wanted at once, first (a pass would only find them at its end).
    fn find_transients(&mut self, world: &World, models: &GpuModels) {
        self.transients.clear();
        for e in world.transient_entities().filter(|e| e.shape == Shape::Model) {
            let Some(b) = e.resolved_model().and_then(|r| models.by_handle.get(r.handle as usize).copied().flatten()) else { continue };
            if !self.live.contains_key(&b) || models.failed.contains_key(&b) || models.uploaded.get(&b).is_some_and(|m| m.resident_from == 0) {
                continue;
            }
            if !self.transients.contains(&b) {
                self.transients.push(b);
                // Wanted now (never evicted as unwanted before the next pass), at its finest level.
                self.want.insert(b, (0.0, self.updates, true, 0));
            }
        }
    }

    /// Uploads within the frame's budget, evicting under the memory budget (see the module
    /// docs).
    fn pump(&mut self, gpu: &mut Gpu, models: &mut GpuModels, library: &SharedLibrary, frame_start: Instant) {
        let _z = specula_core::profile::zone("pump");
        enum Item {
            Build(u64),
            Proxy(CellKey),
        }
        // The pending builds (sorted at the last update; finished ones dropped here) merged with
        // the proxies waiting for upload (few), by priority.
        let zr = specula_core::profile::zone("retain");
        let live = &self.live;
        self.pending.retain(|&(_, b, _, level)| live.contains_key(&b) && !models.failed.contains_key(&b) && models.uploaded.get(&b).is_none_or(|m| m.resident_from > level));
        drop(zr);
        let zp = specula_core::profile::zone("proxies");
        let mut proxies: Vec<(f32, CellKey, u64)> = if self.hlod_on() { self.hlod.uploads().collect() } else { Vec::new() };
        drop(zp);
        // Transients first (priority 0; a pass's entry for the same build follows harmlessly).
        if !self.transients.is_empty() {
            let transients = &self.transients;
            self.pending.retain(|p| !transients.contains(&p.1));
            let mut first: Vec<(f32, u64, bool, usize)> = transients.iter().map(|&b| (0.0, b, true, 0)).collect();
            first.append(&mut self.pending);
            self.pending = first;
        }
        // Mesh memory ahead of need: when the blocks are over 40% full and uploads are coming,
        // the next block is started, its `vkAllocateMemory` (milliseconds on the Intel iGPU) on
        // a worker thread.
        let (free, total, next) = gpu.mesh_space();
        let wanted_soon = !self.pending.is_empty() || !proxies.is_empty();
        // While the blocks hold less than the memory budget: what is resident lives in them, so
        // a world filling its budget still gets its last block ahead (a measured 57 ms otherwise),
        // and blocks never grow past the budget by more than one block.
        let room = (total as f64) < self.cfg.memory_mb * 1_048_576.0;
        let backoff = self.grow_failed.is_some_and(|t| t.elapsed() < Duration::from_secs(1));
        // (Under 60% of a block free: at 8 MB a frame of uploads, the allocation has a few
        // frames to finish before anything needs it.)
        // (The very first block is made by the first upload: nothing to hide it behind yet.)
        if wanted_soon && room && !backoff && total > 0 && (free as f64) < next as f64 * 0.6 {
            if let Err(e) = gpu.grow_mesh_space() {
                // (Out of memory, typically: tried again a second later; uploads go on meanwhile.)
                self.grow_failed = Some(Instant::now());
                if self.upload_errors.len() < 10 {
                    self.upload_errors.push(format!("mesh memory not grown ahead: {e}"));
                }
            }
        }
        if self.pending.is_empty() && proxies.is_empty() {
            self.blocked_bytes = 0;
            self.blocked.clear();
            self.blocked_cells.clear();
            return;
        }
        proxies.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let (mut bi, mut pi) = (0, 0);
        let pending = std::mem::take(&mut self.pending);
        let items = std::iter::from_fn(|| {
            let build = pending.get(bi);
            let proxy = proxies.get(pi);
            match (build, proxy) {
                (Some(b), Some(p)) if p.0 <= b.0 => {
                    pi += 1;
                    Some((p.0, false, Item::Proxy(p.1), p.2))
                }
                (Some(&(pr, b, hard, _)), _) => {
                    bi += 1;
                    Some((pr, hard, Item::Build(b), 0))
                }
                (None, Some(p)) => {
                    pi += 1;
                    Some((p.0, false, Item::Proxy(p.1), p.2))
                }
                (None, None) => None,
            }
        });
        let started = Instant::now();
        let budget_ms = self.cfg.upload_ms;
        let budget_bytes = (self.cfg.upload_mb * 1_048_576.0) as u64;
        let memory = (self.cfg.memory_mb * 1_048_576.0) as u64;
        let zs = specula_core::profile::zone("resident");
        // Terrain sets count against the budget too (outside the cache: the engine sets them).
        let terrain = self.terrain_bytes;
        let mut resident = terrain
            + match self.resident_cache {
                Some(r) => r,
                None => models.uploaded.values().map(|m| m.bytes).sum::<u64>() + self.hlod.resident_bytes(),
            };
        drop(zs);
        let mut blocked = 0u64;
        // The lowest eviction threshold found to leave nothing to evict: an item whose threshold
        // is no lower is blocked at once (items come nearest first; a hard one goes lower).
        let mut exhausted = f32::INFINITY;
        // What may be evicted: the queue the last scan made (re-checked entry by entry), or, the
        // first time the memory budget fills, one made at once.
        let mut victims: Option<Victims> = self.victims_cache.take();
        let mut full = false;
        let mut rebuilt = false;
        // Upload failures (memory retired outside the count): the total is summed again.
        let mut recount = false;
        self.blocked.clear();
        self.blocked_cells.clear();
        let count = (pending.len() + proxies.len()) as u32;
        // What earlier pumps of this frame (captures' calls) spent comes off the budget.
        let already = self.spent_ms;
        for (i, (prio, hard, item, est)) in items.enumerate() {
            let est = match item {
                Item::Build(b) => next_level_bytes(&self.live[&b], models.uploaded.get(&b)),
                Item::Proxy(_) => est,
            };
            let spent = already + frame_start.elapsed().as_secs_f64() * 1e3;
            // Every upload batch still in flight (the GPU is that far behind): the rest waits for
            // the next frame rather than for the GPU (or for a fence poll, which the Intel driver
            // can make slow while it is saturated). The frame's first item is tried whatever the
            // frame's other work cost, so uploads always progress (a frame whose filing and
            // wanted-set work alone exceed the budget used to upload nothing, every frame); one
            // tried, not one uploaded, so a frame whose items are all blocked stays bounded.
            if (self.frame_tried > 0 && spent >= budget_ms) || !gpu.upload_slot_free() {
                self.frame.deferred += count - i as u32;
                break;
            }
            self.frame_tried += 1;
            // An item's cost: a fixed part (buffer creation, recording) plus its size.
            let est_ms = self.ms_per_item + est as f64 / 1_048_576.0 * self.ms_per_mb;
            if self.frame_items > 0 && (spent + est_ms > budget_ms || self.frame_bytes + est > budget_bytes) {
                self.frame.deferred += 1;
                continue;
            }
            // No room in the mesh blocks while one grows on the worker: put off (queuing would
            // wait for the allocation); the block joins at a flush. A build's estimate counts 28
            // bytes a vertex; its parts may carry skin (8) and UV + tangent (16) streams too, so
            // twice the estimate must fit (a proxy group's is exact).
            let need = match item {
                Item::Build(_) => est.saturating_mul(2),
                Item::Proxy(_) => est,
            };
            if gpu.mesh_growing() && !gpu.mesh_fits(need) {
                self.frame.deferred += 1;
                continue;
            }
            if resident + est > memory {
                full = true;
                // A hard-wanted item may evict any soft victim; others only farther ones.
                let threshold = if hard { f32::NEG_INFINITY } else { prio };
                if threshold < exhausted {
                    // No queue yet, or one used up that is older than the wanted set: made now
                    // (once a pump).
                    let used_up = victims.as_ref().is_some_and(|v| v.front().is_none()) && self.victims_for != Some(self.done_pass);
                    if victims.is_none() || (used_up && !rebuilt) {
                        rebuilt = true;
                        let _z = specula_core::profile::zone("victims");
                        self.victims_cache = None;
                        self.victim_scan = None;
                        self.scan_victims(models, f64::INFINITY);
                        victims = self.victims_cache.take();
                    }
                    let v = victims.get_or_insert_with(Victims::default);
                    let _z = specula_core::profile::zone("evict");
                    resident -= self.evict_for(resident + est - memory, threshold, v, gpu, models);
                }
                if resident + est > memory {
                    exhausted = exhausted.min(threshold);
                    self.frame.memory_blocked += 1;
                    blocked += est;
                    match item {
                        Item::Build(b) => self.blocked.push(b),
                        Item::Proxy(cell) => self.blocked_cells.push(cell),
                    }
                    continue;
                }
            }
            let t = Instant::now();
            let done = match item {
                Item::Build(b) => upload_next_level(gpu, models, library, &self.live[&b]),
                Item::Proxy(cell) => self.hlod.upload(cell, gpu),
            };
            let ms = t.elapsed().as_secs_f64() * 1e3;
            if ms > self.frame.slowest.0 {
                let what = match item {
                    Item::Build(_) => "model_lod",
                    Item::Proxy(_) => "hlod_group",
                };
                self.frame.slowest = (ms, *done.as_ref().unwrap_or(&0) / 1024, what);
            }
            match done {
                Ok(bytes) => {
                    self.frame.items += 1;
                    self.frame_items += 1;
                    self.frame_bytes += bytes;
                    self.frame.bytes += bytes;
                    resident += bytes;
                    // Learn the costs: small items give the fixed part, big ones the part per MB.
                    let mb = bytes as f64 / 1_048_576.0;
                    if bytes < 64 * 1024 {
                        // A sample past 3x the estimate is a stall (a busy driver, the OS), not the cost:
                        // clipped, so one never starves the budget for later frames.
                        let ms = ms.min(self.ms_per_item * 3.0 + 0.05);
                        self.ms_per_item = (self.ms_per_item * 0.9 + ms * 0.1).clamp(0.001, 50.0);
                    } else {
                        let rate = ((ms - self.ms_per_item).max(0.0) / mb).min(self.ms_per_mb * 3.0 + 0.1);
                        self.ms_per_mb = (self.ms_per_mb * 0.8 + rate * 0.2).clamp(0.05, 200.0);
                    }
                }
                Err(e) => {
                    recount = true;
                    if self.upload_errors.len() < 10 {
                        self.upload_errors.push(e);
                    }
                }
            }
        }
        let _ze = specula_core::profile::zone("pump_end");
        self.pending = pending;
        if victims.is_some() {
            self.victims_cache = victims;
        }
        self.memory_full = full;
        // Proxy uploads that replaced older ones retired memory the count doesn't see.
        recount |= self.hlod.take_retired();
        self.resident_cache = (!recount).then_some(resident.saturating_sub(terrain));
        self.blocked_bytes = blocked;
        self.blocked.sort_unstable();
        self.blocked_cells.sort_unstable();
        let ms = started.elapsed().as_secs_f64() * 1e3;
        self.frame.ms = ms;
        if self.frame.items > 0 {
            self.upload_ms.push(started.elapsed());
            self.totals.items += self.frame.items;
            self.totals.bytes += self.frame.bytes;
            if ms > budget_ms {
                self.frames_over += 1;
                self.miss(Json::obj([
                    ("error", Json::from("stream_upload_over_budget")),
                    ("metric", Json::from("stream_upload_ms")),
                    ("value", Json::from((ms * 1000.0).round() / 1000.0)),
                    ("budget", Json::from(budget_ms)),
                    ("unit", Json::from("ms")),
                    ("items", Json::from(self.frame.items)),
                    ("kb", Json::from(self.frame.bytes / 1024)),
                    (
                        "slowest_item",
                        Json::obj([
                            ("what", Json::from(self.frame.slowest.2)),
                            ("ms", Json::from((self.frame.slowest.0 * 1000.0).round() / 1000.0)),
                            ("kb", Json::from(self.frame.slowest.1)),
                        ]),
                    ),
                ]));
            }
        }
        if self.frame.memory_blocked > 0 {
            self.totals.memory_blocked += self.frame.memory_blocked;
        }
        self.totals.deferred += self.frame.deferred;
        self.totals.evicted += self.frame.evicted;
    }

    /// Starts a frame's upload budget (the window's frame or a headless loop): 2D textures
    /// ([`Streamer::allowance`]), then [`Streamer::frame`] and captures' calls share it.
    pub fn begin_frame(&mut self) {
        self.spent_ms = 0.0;
        self.frame_items = 0;
        self.frame_tried = 0;
        self.frame_bytes = 0;
        self.frame_began = Some(Instant::now());
    }

    /// What is left of this frame's upload budget for other uploads (2D textures): `None` when
    /// streaming is off (no budget).
    /// Bytes the terrain renderer's sets may hold in all: the memory budget less the builds and
    /// proxies resident as of the last pump (`None` while streaming is off).
    pub fn terrain_room(&self) -> Option<u64> {
        let memory = (self.cfg.memory_mb * 1_048_576.0) as u64;
        self.cfg.enabled.then(|| memory.saturating_sub(self.resident_cache.unwrap_or(0)))
    }

    pub fn allowance(&self) -> Option<crate::two_d::Allowance> {
        self.cfg.enabled.then(|| crate::two_d::Allowance {
            ms: (self.cfg.upload_ms - self.spent_ms).max(0.0),
            bytes: ((self.cfg.upload_mb * 1_048_576.0) as u64).saturating_sub(self.frame_bytes),
            first_free: self.frame_items == 0,
        })
    }

    /// Other uploads (2D textures) made against [`Streamer::allowance`].
    pub fn charge(&mut self, used: crate::two_d::Used) {
        self.spent_ms += used.ms;
        self.frame_items += used.items;
        self.frame_bytes += used.bytes;
        self.textures_2d.items += used.items;
        self.textures_2d.bytes += used.bytes;
        self.textures_2d.deferred += used.deferred;
    }

    /// A build level uploaded outside the pump (the asset studio's [`upload_next_level`]): its
    /// bytes join the running resident total, so the memory budget sees them; a failed upload
    /// evicted the build (memory left outside the count), so the total is summed again. (M4 audit:
    /// studio uploads were charged to the frame's upload budget only, and the pump's cached total
    /// missed them.)
    pub fn uploaded_outside(&mut self, uploaded: &Result<u64, String>) {
        match uploaded {
            Ok(bytes) => {
                if let Some(r) = self.resident_cache.as_mut() {
                    *r += bytes;
                }
            }
            Err(_) => self.resident_cache = None,
        }
    }

    fn miss(&mut self, j: Json) {
        if self.misses.len() == MAX_MISSES {
            self.misses.pop_front();
        }
        self.misses.push_back(j);
    }

    /// Whether build `b` is wanted at the latest update.
    fn wanted_now(&self, b: u64) -> bool {
        self.want.get(&b).is_some_and(|e| e.1 >= self.done_pass)
    }

    /// What may be evicted now, in order: builds and proxies no camera wants (least recently
    /// wanted first), then builds wanted only softly (standing in for a proxy), farthest first.
    /// Made once per pump, when an upload first needs room.
    /// Scans the uploads for the next eviction queue for at most `slice_ms` (in steps of 64
    /// builds); when done, it replaces the queue. Unwanted uploads go least recently wanted first
    /// (builds before proxies on a tie), then soft-wanted ones (stand-ins, proxies the window
    /// isn't drawing) farthest first.
    fn scan_victims(&mut self, models: &GpuModels, slice_ms: f64) {
        let start = Instant::now();
        // A scan runs to its end for the pass it began on (a newer pass only starts the next one:
        // passes can end faster than a big world's scan, which would then never finish).
        let mut scan = self.victim_scan.take().unwrap_or_else(|| VictimScan { pass: self.done_pass, cursor: None, builds_done: false, gone: Vec::new(), soft: Vec::new() });
        if scan.builds_done {
            // The proxies (a few hundred cells at most) and the heaps: a step of their own.
            scan.gone.extend(self.hlod.evictable().into_iter().map(|(f, cell)| Reverse((f, 1, 0, cell))));
            scan.soft.extend(self.hlod.wanted_uploaded().into_iter().map(|(p, cell)| (p.max(0.0).to_bits(), 1, 0, cell)));
            self.victims_cache = Some(Victims { gone: BinaryHeap::from(scan.gone), soft: BinaryHeap::from(scan.soft) });
            self.victims_for = Some(scan.pass);
            return;
        }
        let range = match scan.cursor {
            None => models.uploaded.range(..),
            Some(c) => models.uploaded.range((std::ops::Bound::Excluded(c), std::ops::Bound::Unbounded)),
        };
        for (n, &id) in range.map(|(id, _)| id).enumerate() {
            if models.is_pinned(id) {
                // The asset studio's: never a victim while pinned.
                scan.cursor = Some(id);
                continue;
            }
            match self.want.get(&id) {
                Some(e) if e.1 >= self.done_pass => {
                    if !e.2 {
                        scan.soft.push((e.0.max(0.0).to_bits(), 0, id, (0, 0)));
                    }
                }
                e => scan.gone.push(Reverse((e.map_or(0, |e| e.1), 0, id, (0, 0)))),
            }
            scan.cursor = Some(id);
            if n % 16 == 15 && start.elapsed().as_secs_f64() * 1e3 >= slice_ms {
                self.victim_scan = Some(scan);
                return;
            }
        }
        scan.builds_done = true;
        self.victim_scan = Some(scan);
        if slice_ms.is_infinite() {
            // All at once (the first time the budget fills): the last step now too.
            self.scan_victims(models, slice_ms);
        }
    }

    /// Evicts from `victims` until `need` bytes are free (or none are left that may go for an
    /// item of priority `prio`: soft-wanted builds give way only to nearer items). Returns the
    /// bytes freed. A textured build's pixels left the CPU at upload: wanted again, it is
    /// imported again (`upload_next_level`).
    fn evict_for(&mut self, need: u64, prio: f32, victims: &mut Victims, gpu: &mut Gpu, models: &mut GpuModels) -> u64 {
        let mut freed = 0;
        while freed < need {
            let Some((p, victim)) = victims.front() else { break };
            // Unwanted ones (priority infinity) always go; soft ones only for a nearer item.
            if p <= prio {
                break;
            }
            victims.pop();
            let gone = p == f32::INFINITY;
            let bytes = match victim {
                // (The queue may be older than this pass: a build it now wants hard stays, and
                // one it wants at all isn't taken as unwanted.)
                Victim::Build(id) if self.want.get(&id).is_some_and(|e| e.1 >= self.done_pass && e.2) => 0,
                // (The queue may be older than a pin.)
                Victim::Build(id) if models.is_pinned(id) => 0,
                Victim::Build(id) if gone && self.wanted_now(id) => 0,
                // A soft one the latest wanted set puts no farther than the item: it stays.
                Victim::Build(id) if !gone && self.want.get(&id).is_some_and(|e| e.1 >= self.done_pass && e.0 <= prio) => 0,
                Victim::Proxy(cell) if gone && self.hlod.wanted(cell) => 0,
                Victim::Build(id) => {
                    let bytes = models.uploaded.get(&id).map_or(0, |m| m.bytes);
                    evict_build(gpu, models, id);
                    bytes
                }
                // (The queue may be older than this frame: a proxy the window draws now stays.)
                Victim::Proxy(cell) if self.hlod.window_on(cell) => 0,
                Victim::Proxy(cell) => self.hlod.evict(cell, gpu),
            };
            if bytes > 0 {
                freed += bytes;
                self.frame.evicted += 1;
            }
        }
        freed
    }

    // -----------------------------------------------------------------------------------------
    // Drawing and sight

    /// Appends the window's proxy draws (and swaps cells in and out).
    pub fn draw_window(&mut self, camera: &Camera, viewport_h: u32, models: &GpuModels, out: &mut Vec<DrawItem>) {
        if !self.hlod_on() {
            return;
        }
        let index = &self.index;
        let resident = |id: u64| -> bool {
            let Some(cell) = index.cell_of.get(&id) else { return true };
            let Some(c) = index.cells.get(cell) else { return true };
            let Ok(i) = c.members.binary_search_by_key(&id, |m| m.id) else { return true };
            let b = c.members[i].handle.and_then(|h| models.by_handle.get(h as usize).copied().flatten());
            b.is_some_and(|b| models.uploaded.contains_key(&b))
        };
        // Orthographic: no proxies (an infinite switch distance lets each go once its members are
        // on the GPU, as on any switch).
        let ortho = matches!(camera.projection, sight::Projection::Ortho { .. });
        let min_m = if ortho { f32::INFINITY } else { self.cfg.hlod_min_m };
        self.hlod.window(camera.eye, camera.fov_y_deg, viewport_h, min_m, resident, out);
    }

    /// Appends a capture's proxy draws and fills the capture cover.
    pub fn draw_capture(&mut self, camera: &Camera, viewport_h: u32, out: &mut Vec<DrawItem>) {
        let ortho = matches!(camera.projection, sight::Projection::Ortho { .. });
        self.last_capture = if self.hlod_on() && !ortho {
            // Never a changed cell's old proxy (its members draw instead).
            let (index, dirty) = (&self.index, &self.dirty_cells);
            let stale = |k: CellKey| dirty.contains(&k) || index.cells.get(&k).is_some_and(|c| c.dirty);
            self.hlod.capture(camera.eye, camera.fov_y_deg, viewport_h, self.cfg.hlod_min_m, stale, out)
        } else {
            self.hlod.capture_cover.clear();
            self.hlod.capture_groups.clear();
            (0, 0)
        };
    }

    /// A capture packet's `streaming`: what the last capture left out or merged.
    pub fn capture_json(&self, stats: &crate::models::LodStats) -> Json {
        if !self.cfg.enabled {
            return Json::Null;
        }
        Json::obj([
            ("hlod_cells", Json::from(self.last_capture.0)),
            ("hlod_entities", Json::from(self.last_capture.1)),
            ("not_resident", Json::from(stats.not_resident)),
            ("coarser_lod", Json::from(stats.coarser)),
            (
                "note",
                Json::from(
                    "subjects drawn_as \"hlod\" are drawn merged in their cell's proxy (same coarsest-LOD geometry; visible_px null), \
                     \"not_resident\" ones aren't drawn (not on the GPU); resident_lods_from > 0: finer LODs still streaming",
                ),
            ),
        ])
    }

    /// What a capture through `views` still waits for: model entities in view (within
    /// `radius_m`, not drawn by a proxy) whose build is built but not all uploaded, and cells in
    /// view whose proxy is coming. 0 when streaming is off or `capture_wait` is.
    pub fn waiting_in(&self, views: &[(Camera, u32, u32)], world: &World, models: &GpuModels) -> usize {
        if !self.cfg.enabled || !self.cfg.capture_wait {
            return 0;
        }
        let build_of = |h: Option<u32>| h.and_then(|h| models.by_handle.get(h as usize).copied().flatten());
        // Waited for down to the finest level wanted (the capture's own camera is a source); builds
        // the memory budget keeps out aren't waited for (they would not come).
        let pending = |b: u64| {
            let level = self.want.get(&b).filter(|e| e.1 >= self.done_pass).map_or(0, |e| e.3);
            !models.failed.contains_key(&b) && self.blocked.binary_search(&b).is_err() && models.uploaded.get(&b).is_none_or(|m| m.resident_from > level)
        };
        let mut waiting = 0;
        let resident = |h: Option<u32>| build_of(h).is_some_and(|b| models.uploaded.contains_key(&b));
        for (cam, w, h) in views {
            let vp = cam.view_proj(*w, *h);
            let src = Source::new(SourceKind::Capture, cam, *w, *h, None);
            let hlod_on = self.hlod_on() && src.ortho_d.is_none();
            let big = |b: u64, scale: f32, d: f32| self.live.get(&b).is_some_and(|built| built_px(built, scale, d, cam.fov_y_deg, *h) >= RESIDENT_MIN_PX);
            // A game's presentation in view (wanted first, see `find_transients`).
            for e in world.transient_entities().filter(|e| e.shape == Shape::Model && sight::in_frame(e, false)) {
                let p = vp.mul_vec4(e.world_position().extend(1.0));
                let in_view = p.w > 0.0 && p.x.abs() <= p.w * 1.5 && p.y.abs() <= p.w * 1.5;
                if in_view && build_of(e.resolved_model().map(|r| r.handle)).is_some_and(pending) {
                    waiting += 1;
                }
            }
            for (k, c) in &self.index.cells {
                if !c.has_wide && !box_in_view(&vp, c.lo, c.hi) {
                    continue;
                }
                let d = src.dist(c.lo, c.hi);
                let content = self.hlod.content(*k);
                let current = hlod_on && self.hlod.has_current(*k);
                let proxied = hlod_on && self.hlod.capture_on(*k, cam.eye, cam.fov_y_deg, *h, self.cfg.hlod_min_m);
                if hlod_on && d < self.cfg.hlod_radius_m && !proxied && self.blocked_cells.binary_search(k).is_err() {
                    // A proxy on its way that this view would draw: waited for only when its
                    // members aren't on the GPU to draw meanwhile.
                    if let Some(ct) = content.filter(|ct| ct.key != 0 && ct.complete) {
                        let would = d > hlod::switch_distance(ct.k, cam.fov_y_deg, *h, self.cfg.hlod_min_m);
                        if would && !current && !ct.members.iter().all(|id| c.members.binary_search_by_key(id, |m| m.id).is_ok_and(|i| resident(c.members[i].handle))) {
                            waiting += 1;
                        }
                    }
                }
                let merged = if hlod_on { self.hlod.proxy_members(*k) } else { &[] };
                let sw = content.filter(|ct| ct.key != 0).map_or(0.0, |ct| hlod::switch_distance(ct.k, cam.fov_y_deg, *h, self.cfg.hlod_min_m));
                for m in &c.members {
                    let in_proxy = merged.binary_search(&m.id).is_ok();
                    if in_proxy && proxied {
                        continue;
                    }
                    // What this view draws on its own, as the wanted set has it: wide models by
                    // their own box; merged members of a current proxy wherever the view doesn't
                    // draw the proxy, of a coming one within radius_m or preload x the switch
                    // distance (beyond, they stand in softly: not waited for); the rest within
                    // radius_m.
                    let b = build_of(m.handle);
                    let scale = max_scale(&m.world);
                    let seen = |d: f32| b.is_some_and(|b| big(b, scale, d));
                    let near = if m.wide {
                        let dw = src.dist(m.lo, m.hi);
                        dw < self.cfg.radius_m || seen(dw)
                    } else if in_proxy && current {
                        d < self.cfg.hlod_radius_m || seen(d)
                    } else if in_proxy {
                        d < self.cfg.radius_m || (d <= sw * self.cfg.preload && d < self.cfg.hlod_radius_m) || seen(d)
                    } else {
                        d < self.cfg.radius_m || seen(d)
                    };
                    if !near {
                        continue;
                    }
                    let shown = world.get(m.id).is_some_and(|e| sight::in_frame(e, false));
                    let p = vp.mul_vec4(m.pos.extend(1.0));
                    let in_view = if m.wide { box_in_view(&vp, m.lo, m.hi) } else { p.w > 0.0 && p.x.abs() <= p.w * 1.5 && p.y.abs() <= p.w * 1.5 };
                    if shown && in_view && build_of(m.handle).is_some_and(pending) {
                        waiting += 1;
                    }
                }
            }
        }
        waiting
    }

    /// Marks a capture's subjects: `drawn_as: "hlod"` (drawn merged in the cell's proxy: its
    /// pixels aren't counted on their own, `visible_px` is null) or `"not_resident"` (built, not
    /// on the GPU: not drawn), and `resident_lods_from` when finer levels are still streaming.
    pub fn annotate(&self, subjects: &mut [Subject], world: &World, models: &GpuModels) {
        let notes = self.notes(subjects, world, models);
        apply_notes(subjects, &notes);
    }

    /// What [`Streaming::annotate`] says of the subjects, read when the frame is drawn (a capture
    /// writes them once its ID counts are in, [`apply_notes`]): `(subject index, note)`. Visits
    /// model subjects only.
    pub fn notes(&self, subjects: &[Subject], world: &World, models: &GpuModels) -> Vec<(usize, Note)> {
        let mut out = Vec::new();
        if !self.cfg.enabled {
            return out;
        }
        for (i, s) in subjects.iter().enumerate() {
            if !s.base.is_model() {
                continue;
            }
            let Some(e) = world.get(s.id).filter(|e| e.shape == Shape::Model) else { continue };
            if hlod::covers(&self.hlod.capture_cover, s.id) {
                out.push((i, Note::Hlod));
                continue;
            }
            let b = e.resolved_model().and_then(|r| models.by_handle.get(r.handle as usize).copied().flatten());
            match b.map(|b| (b, models.uploaded.get(&b))) {
                Some((b, None)) if !models.failed.contains_key(&b) => out.push((i, Note::NotResident)),
                Some((_, Some(m))) if m.resident_from > 0 => out.push((i, Note::ResidentFrom(m.resident_from))),
                _ => {}
            }
        }
        out
    }

    // -----------------------------------------------------------------------------------------
    // Reports

    /// perf_frame's `streaming`.
    pub fn json(&self, gpu: &Gpu, models: &GpuModels) -> Json {
        let model_bytes: u64 = models.uploaded.values().map(|m| m.bytes).sum();
        let resident = model_bytes + self.hlod.resident_bytes() + self.terrain_bytes;
        let wanted = self.want.values().filter(|e| e.1 >= self.done_pass).count();
        let soft = self.want.values().filter(|e| e.1 >= self.done_pass && !e.2).count();
        let pending = self.pending.len();
        let mb = |b: u64| Json::from((b as f64 / 1_048_576.0 * 100.0).round() / 100.0);
        let f = &self.frame;
        let t = &self.totals;
        let g = gpu.upload_stats();
        Json::obj([
            ("enabled", Json::from(self.cfg.enabled)),
            ("settings", self.cfg.to_json()),
            ("set_by", self.set_by.as_deref().map_or(Json::Null, Json::from)),
            ("overrides", Json::Obj(self.overrides.clone())),
            ("adapter", Json::from(self.kind.as_str())),
            (
                "frame",
                Json::obj([
                    ("uploads", Json::from(f.items)),
                    ("kb", Json::from(f.bytes / 1024)),
                    ("ms", Json::from((f.ms * 1000.0).round() / 1000.0)),
                    ("deferred", Json::from(f.deferred)),
                    ("memory_blocked", Json::from(f.memory_blocked)),
                    ("evicted", Json::from(f.evicted)),
                ]),
            ),
            ("upload_ms", self.upload_ms.to_json()),
            ("frames_over_upload_budget", Json::from(self.frames_over)),
            (
                "totals",
                Json::obj([
                    ("uploads", Json::from(t.items)),
                    ("mb", mb(t.bytes)),
                    ("deferred", Json::from(t.deferred)),
                    ("memory_blocked", Json::from(t.memory_blocked)),
                    ("evicted", Json::from(t.evicted)),
                ]),
            ),
            (
                "textures_2d",
                Json::obj([
                    ("uploads", Json::from(self.textures_2d.items)),
                    ("mb", mb(self.textures_2d.bytes)),
                    ("deferred", Json::from(self.textures_2d.deferred)),
                ]),
            ),
            ("resident_mb", mb(resident)),
            ("passes", Json::from(self.updates)),
            ("models_mb", mb(model_bytes)),
            ("terrain_mb", mb(self.terrain_bytes)),
            ("memory_budget_mb", Json::from(self.cfg.memory_mb)),
            ("blocked_mb", mb(self.blocked_bytes)),
            ("live_builds", Json::from(self.live.len())),
            ("uploaded_builds", Json::from(models.uploaded.len())),
            ("partial_builds", Json::from(models.uploaded.values().filter(|m| m.resident_from > 0).count())),
            ("wanted_builds", Json::from(wanted)),
            ("standing_in_builds", Json::from(soft)),
            ("blocked_builds", Json::from(self.blocked.len())),
            ("blocked_proxies", Json::from(self.blocked_cells.len())),
            ("pending_builds", Json::from(pending)),
            ("sources", Json::from(self.sources.len())),
            ("cells", Json::from(self.index.cells.len())),
            (
                "index",
                Json::obj([
                    ("rebuilds", Json::from(self.index.rebuilds)),
                    ("refilings", Json::from(self.index.resyncs)),
                    ("refiling", Json::from(self.index.resync.is_some())),
                    ("patches", Json::from(self.index.patches)),
                ]),
            ),
            ("ms_per_item", Json::from((self.ms_per_item * 1000.0).round() / 1000.0)),
            ("ms_per_mb", Json::from((self.ms_per_mb * 1000.0).round() / 1000.0)),
            ("hlod", self.hlod.json()),
            (
                "gpu",
                Json::obj([
                    ("batches", Json::from(g.batches)),
                    ("items", Json::from(g.items)),
                    ("mb", mb(g.bytes)),
                    ("in_flight", Json::from(g.in_flight)),
                    ("slot_waits", Json::from(g.slot_waits)),
                    ("retired_pending", Json::from(g.retired_pending)),
                    ("freed", Json::from(g.freed)),
                    ("dedicated_staging", Json::from(g.dedicated_staging)),
                    ("mesh_blocks", Json::from(g.mesh_blocks)),
                    ("mesh_block_mb", mb(g.mesh_block_bytes)),
                    ("mesh_block_used_mb", mb(g.mesh_block_used)),
                ]),
            ),
        ])
    }

    /// Budget misses as perf_frame errors (`stream_upload_over_budget`: the p95 of frames that
    /// uploaded, or one frame over twice the budget; `stream_memory_over_budget`: wanted builds
    /// and proxies that don't fit `memory_mb`).
    pub fn budget_errors(&self, adapter: &str) -> Vec<Json> {
        let mut out = Vec::new();
        if !self.cfg.enabled {
            return out;
        }
        let err = |code: &str, metric: &str, value: f64, budget: f64, unit: &str| {
            Json::obj([
                ("error", Json::from(code)),
                ("metric", Json::from(metric)),
                ("value", Json::from((value * 1000.0).round() / 1000.0)),
                ("budget", Json::from(budget)),
                ("unit", Json::from(unit)),
                ("adapter", Json::from(adapter)),
            ])
        };
        if let Some(p95) = self.upload_ms.p95().filter(|&p| p > self.cfg.upload_ms) {
            out.push(err("stream_upload_over_budget", "stream_upload_p95_ms", p95, self.cfg.upload_ms, "ms"));
        }
        let worst = self.misses.iter().filter_map(|m| m.get("value").and_then(Json::as_f64)).fold(0.0f64, f64::max);
        if worst > self.cfg.upload_ms * 2.0 {
            out.push(err("stream_upload_spike", "stream_upload_max_ms", worst, self.cfg.upload_ms * 2.0, "ms"));
        }
        if self.blocked_bytes > 0 {
            out.push(err("stream_memory_over_budget", "stream_blocked_mb", self.blocked_bytes as f64 / 1_048_576.0, self.cfg.memory_mb, "MB"));
        }
        out
    }

    /// stream_state: the summary, plus cells near a point and entities not resident there.
    pub fn state_json(&self, gpu: &Gpu, models: &GpuModels, world: &World, near: Option<Vec3>, limit: usize) -> Json {
        let mut j = self.json(gpu, models);
        j.set("recent_misses", Json::arr(self.misses.iter().cloned()));
        let window = self.sources.iter().find(|s| s.kind == SourceKind::Window).map(|s| (s.eye, s.fov_y_deg, s.viewport_h));
        let at = near.or(window.map(|w| w.0));
        if let Some(p) = at {
            let mut cells: Vec<(f32, CellKey, &Cell)> =
                self.index.cells.iter().map(|(k, c)| (hlod::box_distance(p, c.lo, c.hi), *k, c)).collect();
            cells.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let build_of = |h: Option<u32>| h.and_then(|h| models.by_handle.get(h as usize).copied().flatten());
            let mut not_resident = Vec::new();
            let listed = cells.iter().take(limit).map(|(d, k, c)| {
                let resident = c.members.iter().filter(|m| build_of(m.handle).is_some_and(|b| models.uploaded.contains_key(&b))).count();
                for m in &c.members {
                    if not_resident.len() < limit && build_of(m.handle).is_some_and(|b| !models.uploaded.contains_key(&b)) {
                        not_resident.push(Json::obj([("id", Json::from(m.id)), ("path", Json::from(world.path_of(m.id)))]));
                    }
                }
                let view = window.map(|w| (w.0, w.1, w.2));
                Json::obj([
                    ("cell", Json::arr([Json::from(k.0), Json::from(k.1)])),
                    ("distance_m", Json::from((d * 10.0).round() / 10.0)),
                    ("members", Json::from(c.members.len())),
                    ("resident", Json::from(resident)),
                    ("hlod", self.hlod.cell_json(*k, view, self.cfg.hlod_min_m)),
                ])
            });
            j.set("near", Json::arr([p.x, p.y, p.z].map(|v| Json::from((v * 10.0).round() / 10.0))));
            j.set("nearest_cells", Json::arr(listed.collect::<Vec<_>>()));
            j.set("not_resident_near", Json::arr(not_resident));
        }
        j
    }
}

/// What cell `c` merges: its mergeable members whose build is ready, opaque and untextured at
/// its coarsest level (see `hlod`).
fn content_of(c: &Cell, lib: &crate::models::Library, world: &World, instanced: &std::collections::BTreeSet<u64>) -> Content {
    let mut h = StableHasher::with_seed(0x4365_6c6c_0000_0001);
    let mut ct = Content { complete: true, ..Content::default() };
    let mut first = true;
    for m in c.members.iter().filter(|m| m.merges()) {
        let Some(built) = m.handle.and_then(|hd| lib.built(hd)) else {
            // Still building (or failed): no proxy until every member can join.
            if let Some(hd) = m.handle.filter(|hd| lib.handle_error(*hd).is_none()) {
                ct.complete = false;
                if !ct.waiting_on.contains(&hd) {
                    ct.waiting_on.push(hd);
                }
            }
            continue;
        };
        let level = hlod::last_level(built.model.lods.len());
        if !hlod::mergeable(built, level) || instanced.contains(&built.id) {
            continue;
        }
        let (lo, hi) = built.bounds();
        let centre = m.world.transform_point((lo + hi) * 0.5);
        let radius = ((hi - lo).length() * 0.5).max(1e-3) * max_scale(&m.world);
        ct.k = ct.k.max(hlod::switch_factor(radius, level));
        if first {
            ct.lo = centre;
            ct.hi = centre;
            first = false;
        } else {
            ct.lo = ct.lo.min(centre);
            ct.hi = ct.hi.max(centre);
        }
        h.write_u64(m.id);
        // The entity's content hash: transform, colour, model, everything saved (cached by the world).
        h.write_u64(world.get(m.id).map_or(0, Entity::content_hash));
        h.write_u64(built.id);
        h.write_u64(level as u64);
        ct.members.push(m.id);
        if let Some(t) = m.changed {
            ct.changed = Some(ct.changed.map_or(t, |c: Instant| c.max(t)));
        }
    }
    if !ct.members.is_empty() {
        ct.key = h.finish().max(1);
    }
    ct
}

/// The generator's inputs for cell `c` (`None` when a member's build is gone meanwhile).
fn inputs_of(c: &Cell, content: &Content, lib: &crate::models::Library) -> Option<(Vec3, Vec<MemberInput>)> {
    let mut out = Vec::with_capacity(content.members.len());
    for &id in &content.members {
        let i = c.members.binary_search_by_key(&id, |m| m.id).ok()?;
        let m = &c.members[i];
        let built = Arc::clone(lib.built(m.handle?)?);
        let level = hlod::last_level(built.model.lods.len());
        out.push(MemberInput { id, world: m.world, tint: m.tint, built, level });
    }
    // The proxy's frame: the cell's members' centre, rounded to a metre (stable under edits).
    let o = (content.lo + content.hi) * 0.5;
    Some((Vec3::new(o.x.round(), o.y.round(), o.z.round()), out))
}

/// The longest of a transform's axis columns: how much it scales a model's bounds at most.
fn max_scale(world: &Mat4) -> f32 {
    let c = &world.cols;
    [c[0], c[1], c[2]].iter().map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(0.0f32, f32::max)
}

/// Estimated GPU bytes of the next level `upload_next_level` would upload.
fn next_level_bytes(built: &Built, m: Option<&GpuModel>) -> u64 {
    let lods = &built.model.lods;
    let level = match m {
        None => lods.len().saturating_sub(1),
        Some(m) => m.resident_from.saturating_sub(1),
    };
    let lod_bytes = lods.get(level).map_or(0, |l| l.positions.len() as u64 * 28 + l.indices.len() as u64 * 4);
    lod_bytes + if m.is_none() { crate::models::texture_bytes(built) } else { 0 }
}

/// Retires build `id`'s meshes and textures (queued frees: never waits). Returns whether it had
/// textures (their pixels left the CPU: the library must forget the build).
fn evict_build(gpu: &mut Gpu, models: &mut GpuModels, id: u64) -> bool {
    let Some(m) = models.uploaded.remove(&id) else { return false };
    for p in m.lods.iter().flatten() {
        gpu.retire_mesh(p.mesh);
    }
    let textured = !m.textures.is_empty();
    for t in m.textures {
        gpu.retire_texture(t);
    }
    models.forget_animators(id);
    textured
}

/// Uploads the next (finer) level of `built`, coarsest first, through the queued path; the
/// first upload brings the textures. Returns the bytes uploaded.
pub(crate) fn upload_next_level(gpu: &mut Gpu, models: &mut GpuModels, library: &SharedLibrary, built: &Arc<Built>) -> Result<u64, String> {
    let model = &built.model;
    let count = model.lods.len();
    if count == 0 {
        models.failed.insert(built.id, format!("model '{}' not drawn: it has no LODs", model.name));
        return Err(format!("model '{}' has no LODs", model.name));
    }
    let id = built.id;
    let first = !models.uploaded.contains_key(&id);
    let level = models.uploaded.get(&id).map_or(count - 1, |m| m.resident_from.saturating_sub(1));
    let mut textures: Vec<TextureId> = Vec::new();
    let mut bytes = 0u64;
    let fail = |gpu: &mut Gpu, models: &mut GpuModels, textures: &[TextureId], meshes: &[PartDraw], e: String| {
        for t in textures {
            gpu.retire_texture(*t);
        }
        for p in meshes {
            gpu.retire_mesh(p.mesh);
        }
        evict_build(gpu, models, id);
        let msg = format!("model '{}' not drawn: {e}", model.name);
        models.failed.insert(id, msg.clone());
        Err(msg)
    };
    if first {
        for t in &built.textures {
            // The pixels go to the GPU and leave the CPU (see `TextureData`).
            let data = std::mem::take(&mut *t.levels.lock().unwrap_or_else(PoisonError::into_inner));
            if data.is_empty() {
                for t in &textures {
                    gpu.retire_texture(*t);
                }
                // Released by an earlier upload: build it again (its entities show as building).
                library.lock().reimport(id);
                return Ok(0);
            }
            let levels: Vec<&[u8]> = data.iter().map(Vec::as_slice).collect();
            match gpu.queue_texture_levels(t.width, t.height, t.format, &levels, t.sampler) {
                Ok(tex) => textures.push(tex),
                Err(e) => return fail(gpu, models, &textures, &[], e.to_string()),
            }
        }
        bytes += crate::models::texture_bytes(built);
    }
    let all_textures: Vec<TextureId> = if first { textures.clone() } else { models.uploaded[&id].textures.clone() };
    let lod: &MeshData = &model.lods[level];
    let mut parts = Vec::with_capacity(lod.parts.len());
    for (i, part) in lod.parts.iter().enumerate() {
        let Some((_, preset)) = model.materials.iter().find(|(n, _)| *n == part.material) else { continue };
        let textured = built.materials.get(&part.material).filter(|m| m.textured() && !lod.uvs.is_empty() && preset.kind == MaterialKind::Standard);
        let p = {
            let _z = specula_core::profile::zone("part_mesh");
            part_mesh(lod, i, textured.is_some())
        };
        if p.indices.is_empty() {
            continue;
        }
        let streams = MeshStreams { skin: p.skin.as_deref(), uvs: p.uvs.as_deref(), tangents: p.tangents.as_deref() };
        let queued = {
            let _z = specula_core::profile::zone("queue_mesh");
            gpu.queue_mesh(&p.vertices, &p.indices, &streams)
        };
        let mesh = match queued {
            Ok(m) => m,
            Err(e) => return fail(gpu, models, &textures, &parts, e.to_string()),
        };
        bytes += p.bytes();
        let (material, surface, color) = match textured {
            Some(m) => textured_look(m, &all_textures),
            None => part_look(preset, p.mean),
        };
        let surface = crate::models::with_wind(surface, built.model.wind);
        let influence = match (&built.anims, &p.skin) {
            (Some(set), Some(skin)) => part_influence(&set.skeleton, &p.vertices, skin),
            _ => Vec::new(),
        };
        parts.push(PartDraw { mesh, material, surface, color, triangles: (p.indices.len() / 3) as u32, influence });
    }
    let textured_level = parts.iter().any(|p| p.surface.textures != MaterialTextures::NONE);
    match models.uploaded.get_mut(&id) {
        Some(m) => {
            m.lods[level] = parts;
            m.resident_from = level;
            m.bytes += bytes;
            m.needs.textured |= textured_level;
        }
        None => {
            let b = model.bounds;
            let (lo, hi) = (Vec3::new(b.min[0], b.min[1], b.min[2]), Vec3::new(b.max[0], b.max[1], b.max[2]));
            let mut lods: Vec<Vec<PartDraw>> = (0..count).map(|_| Vec::new()).collect();
            lods[level] = parts;
            let needs = Needs { skinned: built.anims.is_some() && model.lods.iter().any(MeshData::is_skinned), textured: textured_level };
            models.uploaded.insert(
                id,
                GpuModel {
                    lods,
                    center: (lo + hi) * 0.5,
                    radius: ((hi - lo).length() * 0.5).max(1e-3),
                    textures,
                    anims: built.anims.clone(),
                    needs,
                    resident_from: level,
                    bytes,
                },
            );
        }
    }
    Ok(bytes)
}

/// Every key `stream_settings` takes.
pub fn setting_keys() -> Vec<&'static str> {
    FLAGS.iter().copied().chain(RANGES.iter().map(|r| r.0)).collect()
}

/// A subject's streaming note ([`Streaming::notes`]).
#[derive(Clone, Copy, Debug)]
pub enum Note {
    /// Drawn merged in an HLOD proxy: its own pixels aren't counted.
    Hlod,
    /// Built but not on the GPU (streamed out): not drawn.
    NotResident,
    /// Only LODs from this one are resident (finer ones are still streaming).
    ResidentFrom(usize),
}

/// Writes [`Streaming::notes`] into the subjects.
pub fn apply_notes(subjects: &mut [Subject], notes: &[(usize, Note)]) {
    for &(i, n) in notes {
        let Some(s) = subjects.get_mut(i) else { continue };
        match n {
            Note::Hlod => {
                s.set("drawn_as", Json::from("hlod"));
                s.set("visible_px", Json::Null);
            }
            Note::NotResident => {
                s.set("drawn_as", Json::from("not_resident"));
            }
            Note::ResidentFrom(l) => {
                s.set("resident_lods_from", Json::from(l));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_parse_with_adapter_sections_and_ranges() {
        let doc = specula_core::toml::parse("[streaming]\nupload_ms = 1.5\nhlod = false\n[streaming.integrated]\nupload_mb = 4\n").unwrap();
        let t = StreamTable::parse(doc.get("streaming").unwrap()).unwrap();
        let i = t.config("integrated");
        assert_eq!((i.upload_ms, i.upload_mb, i.hlod), (1.5, 4.0, false));
        let d = t.config("discrete");
        assert_eq!((d.upload_ms, d.upload_mb), (1.5, 8.0));
        for bad in ["[streaming]\nupload_ms = 0\n", "[streaming]\nfoo = 1\n", "[streaming]\nenabled = 3\n", "[streaming.integrated]\ncell_m = 2\n"] {
            let doc = specula_core::toml::parse(bad).unwrap();
            assert!(StreamTable::parse(doc.get("streaming").unwrap()).is_err(), "{bad}");
        }
        let mut c = StreamConfig::default();
        assert!(c.apply(&Json::obj([("memory_mb", Json::from(1.0))]), "stream_settings").unwrap_err().contains("memory_mb"));
        assert_eq!(c, StreamConfig::default(), "a refused patch changes nothing");
    }

    /// M4 audit: the asset studio's uploads were charged to the frame's budget but missed the
    /// pump's cached resident total, so the memory budget undercounted.
    #[test]
    fn uploads_outside_the_pump_keep_the_resident_total() {
        let mut s = Streamer::default();
        s.uploaded_outside(&Ok(64));
        assert_eq!(s.resident_cache, None, "an unknown total stays unknown (the pump sums it)");
        s.resident_cache = Some(1000);
        s.uploaded_outside(&Ok(64));
        assert_eq!(s.resident_cache, Some(1064));
        s.uploaded_outside(&Err("lost".into()));
        assert_eq!(s.resident_cache, None, "a failed upload evicted the build: summed again");
    }

    #[test]
    fn cells_follow_entities_and_never_touch_the_world() {
        use crate::world::SpawnSpec;
        let mut world = World::default();
        let mut ids = Vec::new();
        for i in 0..20 {
            let mut s = SpawnSpec::model(crate::world::ModelSpec { recipe: "recipes/x.json".into(), params: Vec::new(), seed: Some(i) });
            s.position = Vec3::new(i as f32 * 10.0, 0.0, 5.0);
            ids.push(world.spawn(s, "test").unwrap().id);
        }
        let mut index = Index::default();
        let mut touched = Vec::new();
        let now = Instant::now();
        assert!(index.sync(&world, 0, 32.0, now, &mut touched, Duration::from_secs(1)));
        assert_eq!(index.cells.len(), 6, "x 0..190 in 32 m cells");
        assert_eq!(index.cells.values().map(|c| c.members.len()).sum::<usize>(), 20);
        assert!(!index.sync(&world, 0, 32.0, now, &mut touched, Duration::from_secs(1)), "unchanged world: nothing to do");
        // Move one far away: patched, not rebuilt.
        let rebuilds = index.rebuilds;
        world.update(&crate::world::EntityKey::Id(ids[0]), crate::world::UpdateSpec { position: Some(Vec3::new(500.0, 0.0, 5.0)), ..Default::default() }, "test").unwrap();
        assert!(index.sync(&world, 0, 32.0, now, &mut touched, Duration::from_secs(1)));
        assert_eq!(index.rebuilds, rebuilds);
        assert_eq!(index.cell_of[&ids[0]], (15, 0));
        assert!(index.cells[&(15, 0)].members[0].changed.is_some());
        assert_eq!(index.cells.values().map(|c| c.members.len()).sum::<usize>(), 20);
        assert_eq!(touched, vec![(0, 0), (15, 0)], "both cells were touched");
        // A move within its cell: updated in place, its box follows.
        world.update(&crate::world::EntityKey::Id(ids[2]), crate::world::UpdateSpec { position: Some(Vec3::new(25.0, 0.0, 5.0)), ..Default::default() }, "test").unwrap();
        assert!(index.sync(&world, 0, 32.0, now, &mut touched, Duration::from_secs(1)));
        assert_eq!(touched, vec![(0, 0)]);
        assert_eq!(index.cell_of[&ids[2]], (0, 0));
        assert_eq!(index.cells[&(0, 0)].hi.x, 30.0, "the cell's box spans its members (x 10..30)");
        assert_eq!(index.cells.values().map(|c| c.members.len()).sum::<usize>(), 20);
        world.delete(&crate::world::EntityKey::Id(ids[1]), None).unwrap();
        let hash = world.state_hash();
        index.sync(&world, 0, 32.0, now, &mut touched, Duration::from_secs(1));
        assert!(!index.cell_of.contains_key(&ids[1]));
        // Another world generation: rebuilt.
        index.sync(&world, 1, 32.0, now, &mut touched, Duration::from_secs(1));
        assert_eq!(index.rebuilds, rebuilds + 1);
        assert_eq!(world.state_hash(), hash, "filing the world never changes it");
        // A big change is filed a slice at a time (here: one entity per call), never lost.
        for i in 0..5 {
            let mut s = SpawnSpec::model(crate::world::ModelSpec { recipe: "recipes/x.json".into(), params: Vec::new(), seed: Some(100 + i) });
            s.position = Vec3::new(1000.0 + i as f32, 0.0, 0.0);
            world.spawn(s, "test").unwrap();
        }
        let before = index.cell_of.len();
        for n in 1..=5 {
            assert!(index.sync(&world, 1, 32.0, now, &mut touched, Duration::ZERO));
            assert_eq!(index.cell_of.len(), before + n);
        }
        assert!(!index.sync(&world, 1, 32.0, now, &mut touched, Duration::ZERO), "the backlog is done");
    }

    /// The eviction queue's heaps give the order the sorted queue gave: unwanted uploads least
    /// recently wanted first (a build before a proxy on a tie), then soft ones farthest first.
    #[test]
    fn eviction_queue_order() {
        let mut v = Victims {
            gone: BinaryHeap::from(vec![Reverse((7, 1, 0, (2, 2))), Reverse((3, 0, 40, (0, 0))), Reverse((7, 0, 9, (0, 0)))]),
            soft: BinaryHeap::from(vec![(10.0f32.to_bits(), 0, 5, (0, 0)), (250.5f32.to_bits(), 1, 0, (1, 1)), (99.0f32.to_bits(), 0, 6, (0, 0))]),
        };
        let mut order = Vec::new();
        while let Some((p, victim)) = v.front() {
            order.push((p, victim));
            v.pop();
        }
        let inf = f32::INFINITY;
        assert_eq!(
            order,
            [
                (inf, Victim::Build(40)),
                (inf, Victim::Build(9)),
                (inf, Victim::Proxy((2, 2))),
                (250.5, Victim::Proxy((1, 1))),
                (99.0, Victim::Build(6)),
                (10.0, Victim::Build(5)),
            ]
        );
    }

    /// More changes at once than the world's change record holds (an agent's big batch): the
    /// index files everything again a slice at a time, never all in one call, while the old cells
    /// serve; once done it holds every entity, then catches up with what changed meanwhile.
    #[test]
    fn a_refiling_is_sliced() {
        use crate::world::{ModelSpec, SpawnSpec, DRAW_LOG};
        let mut world = World::default();
        let spawn = |world: &mut World, i: u64| {
            let mut s = SpawnSpec::model(ModelSpec { recipe: "recipes/x.json".into(), params: Vec::new(), seed: Some(i) });
            s.position = Vec3::new((i % 50) as f32 * 7.0, 0.0, (i / 50) as f32 * 7.0);
            world.spawn(s, "test").unwrap().id
        };
        for i in 0..100 {
            spawn(&mut world, i);
        }
        let mut index = Index::default();
        let mut touched = Vec::new();
        let now = Instant::now();
        index.sync(&world, 0, 32.0, now, &mut touched, Duration::MAX);
        let rebuilds = index.rebuilds;
        // A batch past the change record.
        for i in 100..(100 + DRAW_LOG as u64 + 400) {
            spawn(&mut world, i);
        }
        let total = 100 + DRAW_LOG + 400;
        let mut calls = 0;
        loop {
            // A zero slice: the least a call may do (64 entities).
            index.sync(&world, 0, 32.0, now, &mut touched, Duration::ZERO);
            calls += 1;
            if index.resync.is_none() && index.backlog.is_empty() {
                break;
            }
            assert!(index.cell_of.len() == 100 || index.cell_of.len() == total, "the old cells serve until it is done");
            assert!(calls < 100, "it ends");
        }
        assert!(calls > 5, "sliced over {calls} calls");
        assert_eq!(index.rebuilds, rebuilds, "not rebuilt at once");
        assert_eq!(index.resyncs, 1);
        assert_eq!(index.cell_of.len(), total);
        assert_eq!(index.cells.values().map(|c| c.members.len()).sum::<usize>(), total);
        // Changes during a refiling are caught up after it.
        for i in 0..(DRAW_LOG as u64 + 10) {
            spawn(&mut world, 10_000 + i);
        }
        index.sync(&world, 0, 32.0, now, &mut touched, Duration::ZERO);
        let late = spawn(&mut world, 99_999);
        while index.resync.is_some() || !index.backlog.is_empty() || index.epoch != world.draw_epoch() {
            index.sync(&world, 0, 32.0, now, &mut touched, Duration::ZERO);
        }
        assert!(index.cell_of.contains_key(&late), "spawned during the refiling, filed after it");
        assert_eq!(index.cell_of.len(), total + DRAW_LOG + 11);
        // A world that keeps changing faster than a sliced refiling can catch up with (review):
        // each refiling in a row gets a bigger slice, so it still ends.
        let mut calls = 0;
        let mut next = 30_000u64;
        index.sync(&world, 0, 32.0, now, &mut touched, Duration::ZERO);
        loop {
            for _ in 0..(DRAW_LOG as u64 / 3) {
                spawn(&mut world, next);
                next += 1;
            }
            index.sync(&world, 0, 32.0, now, &mut touched, Duration::ZERO);
            calls += 1;
            if index.resync.is_none() {
                break;
            }
            assert!(calls < 500, "a refiling under constant change still ends");
        }
        while !index.backlog.is_empty() || index.epoch != world.draw_epoch() {
            index.sync(&world, 0, 32.0, now, &mut touched, Duration::ZERO);
        }
        let changing = (next - 30_000) as usize;
        assert_eq!(index.cell_of.len(), total + DRAW_LOG + 11 + changing);
        // A capture's call (no slice limit) does it all at once.
        for i in 0..(DRAW_LOG as u64 + 10) {
            spawn(&mut world, 20_000 + i);
        }
        index.sync(&world, 0, 32.0, now, &mut touched, Duration::MAX);
        assert!(index.resync.is_none() && index.backlog.is_empty());
        assert_eq!(index.cell_of.len(), total + 2 * DRAW_LOG + 21 + changing);
    }

    /// Review: refilings in a row that can't catch up get twice the slice each, so a world that
    /// keeps changing converges before the at-once fallback (a non-zero slice doubles).
    #[test]
    fn refilings_in_a_row_get_bigger_slices() {
        use crate::world::{ModelSpec, SpawnSpec, DRAW_LOG};
        let mut world = World::default();
        let mut next = 0u64;
        let mut spawn = |world: &mut World, n: u64| {
            for _ in 0..n {
                let mut s = SpawnSpec::model(ModelSpec { recipe: "recipes/x.json".into(), params: Vec::new(), seed: Some(next) });
                s.position = Vec3::new((next % 80) as f32 * 5.0, 0.0, (next / 80) as f32 * 5.0);
                world.spawn(s, "test").unwrap();
                next += 1;
            }
        };
        spawn(&mut world, 4000);
        let mut index = Index::default();
        let mut touched = Vec::new();
        let now = Instant::now();
        index.sync(&world, 0, 32.0, now, &mut touched, Duration::MAX);
        spawn(&mut world, DRAW_LOG as u64 + 1);
        let mut most = 0;
        for _ in 0..1000 {
            // Each call: changes pile up behind a refiling walking its least each call (a zero
            // slice), more than the record holds before the first one ends.
            spawn(&mut world, DRAW_LOG as u64 / 32);
            index.sync(&world, 0, 32.0, now, &mut touched, Duration::ZERO);
            most = most.max(index.resync_overflows);
            if index.resync.is_none() {
                break;
            }
        }
        assert!(index.resync.is_none(), "it converges");
        assert!(most >= 1, "it needed bigger slices");
        assert!(most < 6, "before the at-once fallback (overflows {most})");
    }

    #[test]
    fn box_view_test_is_conservative() {
        let cam = Camera::new(Vec3::new(0.0, 2.0, 10.0), Vec3::ZERO, 50.0);
        let vp = cam.view_proj(320, 200);
        assert!(box_in_view(&vp, Vec3::new(-1.0, -1.0, -1.0), Vec3::ONE));
        assert!(!box_in_view(&vp, Vec3::new(-1.0, -1.0, 20.0), Vec3::new(1.0, 1.0, 22.0)), "behind the camera");
        assert!(!box_in_view(&vp, Vec3::new(500.0, -1.0, -1.0), Vec3::new(501.0, 1.0, 1.0)), "far to the side");
    }
}

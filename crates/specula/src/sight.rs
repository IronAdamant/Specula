//! Agent sight: cameras, draw lists and the subject list that accompanies every captured frame.
//!
//! Subjects come from the engine's own transforms and the view-projection used to render, so agents
//! never guess where something is. `screen` is the projected **bounding box** of each entity
//! (clipped to the camera's near/far planes and the image): loose, and blind to occlusion. Captures
//! also render the GPU **ID buffer** (`specula_gpu::Gpu::render_with_ids`), which adds the truth
//! per entity: `visible_px`, the exact `screen_box_exact` of its drawn pixels, `no_pixels` and `occluded`
//! ([`apply_visibility`]).

use crate::meshes::{LatheMeshes, ShapeMeshes};
use crate::models::{GpuModels, LodChoice, LodStats, ModelDraw};
use crate::world::{color_hex, material_name, vec3_json, Entity, Shape, World};
use specula_core::math::{Mat4, Vec3, Vec4};
use specula_core::Json;
use specula_gpu::{DrawItem, FrameDesc, PointLight, MAX_POINT_LIGHTS};
use std::collections::BTreeMap;

pub const SCHEMA: &str = "specula.sight/v1";

/// Near clip plane in metres. With reversed-Z and no far plane it never has to grow, however
/// large the world is (a "sun" 1000 km away no longer pushes the near plane out).
pub const NEAR: f32 = 0.05;

/// Far plane of orthographic cameras, metres from the eye (perspective has none).
pub const ORTHO_FAR: f32 = 10_000.0;

/// How a camera projects.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Projection {
    /// `fov_y_deg` wide, reversed Z, no far plane.
    #[default]
    Perspective,
    /// Parallel rays: `height_m` metres of the view plane fill the image height (width follows
    /// the aspect). Reversed Z from [`NEAR`] to [`ORTHO_FAR`] in front of the eye. For 2D: an
    /// eye on +Z looking down −Z at the XY plane, +Y up (see `crate::two_d`).
    Ortho { height_m: f32 },
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub fov_y_deg: f32,
    pub projection: Projection,
    /// Roll about the view direction, degrees (positive tips the camera's up towards its right,
    /// so the horizon turns anticlockwise on screen). 0 keeps the horizon level: every camera
    /// but a timeline shot that asks for roll (docs/design/TIMELINE.md).
    pub roll_deg: f32,
}

impl Camera {
    /// A perspective camera.
    pub fn new(eye: Vec3, target: Vec3, fov_y_deg: f32) -> Camera {
        Camera { eye, target, fov_y_deg, projection: Projection::Perspective, roll_deg: 0.0 }
    }

    /// This camera rolled by `roll_deg`.
    pub fn rolled(self, roll_deg: f32) -> Camera {
        Camera { roll_deg, ..self }
    }

    /// The perspective up vector: +Y, turned about the view direction by the roll (exactly +Y
    /// without roll, so unrolled views are unchanged bit for bit).
    fn perspective_up(&self) -> Vec3 {
        if self.roll_deg == 0.0 {
            return Vec3::Y;
        }
        let f = (self.target - self.eye).normalize();
        let side = f.cross(Vec3::Y);
        let right = if side.length() > 1e-6 { side.normalize() } else { Vec3::X };
        let up = right.cross(f);
        let (s, c) = self.roll_deg.to_radians().sin_cos();
        up * c + right * s
    }

    /// The 2D convention: an orthographic camera looking down −Z at the XY plane (+Y up on
    /// screen) centred on (`x`, `y`), `height_m` tall.
    pub fn view_2d(x: f32, y: f32, height_m: f32) -> Camera {
        Camera {
            eye: Vec3::new(x, y, 100.0),
            target: Vec3::new(x, y, 0.0),
            fov_y_deg: 50.0,
            projection: Projection::Ortho { height_m },
            roll_deg: 0.0,
        }
    }

    /// The up vector of an orthographic view: +Y, unless looking straight up or down (then −Z:
    /// north at the top of a top-down view).
    fn up(&self) -> Vec3 {
        let f = (self.target - self.eye).normalize();
        if f.cross(Vec3::Y).length() > 1e-6 {
            Vec3::Y
        } else {
            Vec3::new(0.0, 0.0, -1.0)
        }
    }

    /// Reversed-Z projection × view (pair it with `FrameDesc::reverse_z = true`): infinite far
    /// plane in perspective, [`ORTHO_FAR`] orthographic.
    pub fn view_proj(&self, width: u32, height: u32) -> Mat4 {
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        match self.projection {
            Projection::Perspective => {
                let proj = Mat4::perspective_vk_reverse_z(self.fov_y_deg.to_radians(), aspect, NEAR);
                proj * Mat4::look_at_rh(self.eye, self.target, self.perspective_up())
            }
            Projection::Ortho { height_m } => {
                let (h, w) = (height_m, height_m * aspect);
                let proj = Mat4::orthographic_vk_reverse_z(-0.5 * w, 0.5 * w, -0.5 * h, 0.5 * h, NEAR, ORTHO_FAR);
                proj * Mat4::look_at_rh(self.eye, self.target, self.up())
            }
        }
    }

    /// The ray from the eye through window pixel `(x, y)` (origin top-left) of a `width` x `height`
    /// view: `(origin, unit direction)`. For mouse picking.
    pub fn ray(&self, x: f32, y: f32, width: u32, height: u32) -> (Vec3, Vec3) {
        let (w, h) = (width.max(1) as f32, height.max(1) as f32);
        let forward = (self.target - self.eye).normalize();
        if let Projection::Ortho { height_m } = self.projection {
            let right = forward.cross(self.up()).normalize();
            let up = right.cross(forward);
            let nx = (2.0 * (x + 0.5) / w - 1.0) * 0.5 * height_m * (w / h);
            let ny = (1.0 - 2.0 * (y + 0.5) / h) * 0.5 * height_m;
            return (self.eye + right * nx + up * ny, forward);
        }
        // Looking straight up or down: any horizontal right vector will do.
        let side = forward.cross(Vec3::Y);
        let right = if side.length() > 1e-6 { side.normalize() } else { Vec3::X };
        let up = right.cross(forward);
        // A rolled camera's screen axes turn with it.
        let (right, up) = if self.roll_deg == 0.0 {
            (right, up)
        } else {
            let (s, c) = self.roll_deg.to_radians().sin_cos();
            (right * c - up * s, up * c + right * s)
        };
        let tan = (self.fov_y_deg.to_radians() * 0.5).tan();
        let nx = (2.0 * (x + 0.5) / w - 1.0) * tan * (w / h);
        let ny = (1.0 - 2.0 * (y + 0.5) / h) * tan;
        (self.eye, (forward + right * nx + up * ny).normalize())
    }

    pub fn to_json(self) -> Json {
        let mut f = vec![("eye", vec3_json(self.eye)), ("target", vec3_json(self.target))];
        match self.projection {
            Projection::Perspective => f.push(("fov_y_deg", Json::from(self.fov_y_deg))),
            Projection::Ortho { height_m } => {
                f.push(("ortho", Json::obj([("height_m", Json::from(height_m)), ("far_m", Json::from(ORTHO_FAR))])))
            }
        }
        f.push(("near_m", Json::from(NEAR)));
        if self.roll_deg != 0.0 {
            f.push(("roll_deg", Json::from(self.roll_deg)));
        }
        Json::obj(f)
    }
}

/// Three-quarter view from above that fits everything in the world.
pub fn auto_camera(world: &World, fov_y_deg: f32) -> Camera {
    auto_camera_of(world.bounds(), fov_y_deg)
}

/// [`auto_camera`] for world bounds already known ([`SubjectCache::bounds`]).
pub fn auto_camera_of(bounds: Option<(Vec3, Vec3)>, fov_y_deg: f32) -> Camera {
    let (center, radius) = match bounds {
        Some((lo, hi)) => ((lo + hi) * 0.5, ((hi - lo).length() * 0.5).max(0.5)),
        None => (Vec3::ZERO, 1.5),
    };
    let distance = radius / (fov_y_deg.to_radians() * 0.5).sin() * 1.15;
    let dir = Vec3::new(0.55, 0.45, 0.7).normalize();
    Camera::new(center + dir * distance, center, fov_y_deg)
}

/// What auto-framing fits: the main cluster's bounds, and the entities it left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Framing {
    pub bounds: Option<(Vec3, Vec3)>,
    /// Entities left out as outliers, by id.
    pub excluded: Vec<u64>,
}

/// An entity this many core radii (or more) from the cluster's centre may be an outlier.
pub const OUTLIER_FACTOR: f32 = 10.0;
/// Fewer boxes than this are always framed whole (a small scene has no "main cluster" to keep:
/// the M4 final review saw a car 12 m from two cubes left out).
pub const MIN_BOXES_FOR_OUTLIERS: usize = 8;
/// Outliers are left out only when keeping them would shrink the rest below this many pixels
/// across.
pub const MIN_CORE_PX: f32 = 32.0;
/// Entities a packet names in `camera.framing.excluded` (the count is always given).
pub const MAX_EXCLUDED_NAMED: usize = 16;

/// The bounds auto-framing fits, for an image `height_px` tall seen with `fov_y_deg`, from every
/// shown entity's box `(id, lo, hi)`: all of them, less the outliers (the M3 carry-over: one cube
/// at x = 2000 shrank the rest to a pixel). The centre is the per-axis median of the boxes'
/// centres; each box's distance is from that centre to its nearest point (0 when it holds the
/// centre, as a ground plane under everything does); the core radius is the 75th percentile of
/// those distances (at least the median box's half-diagonal, and 0.5 m). Boxes more than
/// [`OUTLIER_FACTOR`] core radii away are candidates, the farthest first and at most a quarter
/// of all (`floor(n / 4)`); they are left out only when keeping them would draw the rest smaller
/// than [`MIN_CORE_PX`] across. Fewer than [`MIN_BOXES_FOR_OUTLIERS`] boxes: all kept.
/// Deterministic (order-free: medians by total order, ties by id, `excluded` by id).
pub fn framing(boxes: &[(u64, Vec3, Vec3)], height_px: u32, fov_y_deg: f32) -> Framing {
    let union = |it: &mut dyn Iterator<Item = &(u64, Vec3, Vec3)>| it.map(|b| (b.1, b.2)).reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)));
    if boxes.len() < MIN_BOXES_FOR_OUTLIERS {
        return Framing { bounds: union(&mut boxes.iter()), excluded: Vec::new() };
    }
    let n = boxes.len();
    let mid = (n - 1) / 2;
    let median = |mut v: Vec<f32>, at: usize| *v.select_nth_unstable_by(at, f32::total_cmp).1;
    let centre = |b: &(u64, Vec3, Vec3)| (b.1 + b.2) * 0.5;
    let m = Vec3::new(median(boxes.iter().map(|b| centre(b).x).collect(), mid), median(boxes.iter().map(|b| centre(b).y).collect(), mid), median(boxes.iter().map(|b| centre(b).z).collect(), mid));
    let dist = |b: &(u64, Vec3, Vec3)| {
        let d = (b.1 - m).max(m - b.2).max(Vec3::ZERO);
        d.length()
    };
    let core = median(boxes.iter().map(dist).collect(), (3 * (n - 1)) / 4);
    let size = median(boxes.iter().map(|b| (b.2 - b.1).length() * 0.5).collect(), mid);
    let limit = OUTLIER_FACTOR * core.max(size).max(0.5);
    let all = union(&mut boxes.iter());
    let core_box = union(&mut boxes.iter().filter(|b| dist(b) <= limit));
    // How big the core (everything within the limit) is drawn with everything framed
    // (auto_camera_of fits the bounds' sphere with a 15% margin): outliers are left out only if
    // that is below a readable size.
    let radius = |b: Option<(Vec3, Vec3)>| b.map_or(0.5, |(lo, hi)| ((hi - lo).length() * 0.5).max(0.5));
    let core_px = height_px as f32 * radius(core_box) / radius(all) * (fov_y_deg.to_radians() * 0.5).cos() / 1.15;
    if core_px >= MIN_CORE_PX {
        return Framing { bounds: all, excluded: Vec::new() };
    }
    // The farthest first, at most a quarter of all.
    let mut far: Vec<(f32, u64)> = boxes.iter().map(|b| (dist(b), b.0)).filter(|(d, _)| *d > limit).collect();
    far.sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    far.truncate(n / 4);
    let mut excluded: Vec<u64> = far.into_iter().map(|(_, id)| id).collect();
    excluded.sort_unstable();
    let bounds = union(&mut boxes.iter().filter(|b| excluded.binary_search(&b.0).is_err()));
    Framing { bounds, excluded }
}

impl Framing {
    /// Its packet form (`camera.framing`), when something was left out: who (by id and path, the
    /// first [`MAX_EXCLUDED_NAMED`]), how many and why.
    pub fn to_json(&self, world: &World) -> Option<Json> {
        if self.excluded.is_empty() {
            return None;
        }
        let named = self.excluded.iter().take(MAX_EXCLUDED_NAMED).map(|&id| Json::obj([("id", Json::from(id)), ("path", Json::from(world.path_of(id)))]));
        Some(Json::obj([
            ("excluded", Json::arr(named)),
            ("excluded_count", Json::from(self.excluded.len())),
            ("reason", Json::from("outlier")),
            ("note", Json::from("auto-framing fits the main cluster; these are far from it. Pass eye and target, or a named camera, to frame them")),
        ]))
    }
}

/// Every shown entity's world box `(id, lo, hi)` (hidden and `ui` entities left out), from the
/// world itself: [`SubjectCache::boxes`] is the cheap way once the cache is warm.
pub fn world_boxes(world: &World) -> Vec<(u64, Vec3, Vec3)> {
    world.entities().filter(|e| e.shown() && !e.in_ui()).filter_map(|e| Some((e.id, corners_box(&e.world_corners()?)))).map(|(id, (lo, hi))| (id, lo, hi)).collect()
}

/// The axis-aligned box of 8 corners.
fn corners_box(c: &[Vec3; 8]) -> (Vec3, Vec3) {
    c[1..].iter().fold((c[0], c[0]), |(lo, hi), &p| (lo.min(p), hi.max(p)))
}

/// An orthographic view of the whole world down −Z (the 2D convention), `height_m` tall or
/// fitting the world's bounds (with a margin) for an image of `aspect`.
pub fn auto_camera_2d(world: &World, height_m: Option<f32>, aspect: f32) -> Camera {
    let (lo, hi) = world.bounds().unwrap_or((Vec3::new(-1.5, -1.5, 0.0), Vec3::new(1.5, 1.5, 0.0)));
    let c = (lo + hi) * 0.5;
    let fit = ((hi.y - lo.y).max((hi.x - lo.x) / aspect.max(1e-3)) * 1.1).max(0.5);
    let mut cam = Camera::view_2d(c.x, c.y, height_m.unwrap_or(fit));
    cam.eye.z = hi.z + 100.0;
    cam.target.z = hi.z;
    cam
}

/// Everything uploaded that entities draw with.
pub struct Meshes<'a> {
    pub shapes: &'a ShapeMeshes,
    pub lathes: &'a LatheMeshes,
    pub models: &'a GpuModels,
}

/// Who a frame is for: the camera (for model LODs) and which entities it includes.
pub struct View<'a> {
    pub camera: Camera,
    pub viewport_h: u32,
    /// Draw `ui`-layer entities (the window does; captures don't unless asked).
    pub include_ui: bool,
    pub lod: LodChoice<'a>,
}

/// Whether `e` is in a frame that does (or doesn't) include the `ui` layer.
pub fn in_frame(e: &Entity, include_ui: bool) -> bool {
    e.shown() && (include_ui || !e.in_ui())
}

/// One draw per shown entity with a mesh (one per material part for models), appended to
/// `out` (cleared first). Groups, hidden entities, `ui` entities when the view leaves them out,
/// and meshes that failed to upload are skipped. Returns how models were drawn.
/// `owners`, when given, receives the entity id of every draw (parallel to `out`), for the ID
/// buffer.
pub fn draw_list(world: &World, meshes: &Meshes, view: &mut View, out: &mut Vec<DrawItem>, mut owners: Option<&mut Vec<u64>>) -> LodStats {
    out.clear();
    if let Some(o) = owners.as_deref_mut() {
        o.clear();
    }
    let mut stats = LodStats::default();
    for e in world.entities() {
        match plan(e, meshes, view.include_ui) {
            Plan::Skip => continue,
            Plan::Item(d) => out.push(d),
            Plan::Model => draw_model(e, meshes, view, &mut stats, out),
        }
        if let Some(o) = owners.as_deref_mut() {
            o.resize(out.len(), e.id);
        }
    }
    stats
}

/// What an entity adds to a frame: nothing, one fixed draw (built-in shapes and lathes: it
/// depends only on the entity and its mesh), or a model's draws (chosen per frame: LOD, pose).
/// Returned by value and consumed at once, never stored: boxing the draw would allocate per entity.
#[allow(clippy::large_enum_variant)]
enum Plan {
    Skip,
    Item(DrawItem),
    Model,
}

fn plan(e: &Entity, meshes: &Meshes, include_ui: bool) -> Plan {
    if !in_frame(e, include_ui) {
        return Plan::Skip;
    }
    let mesh = match e.shape {
        Shape::Group => return Plan::Skip,
        Shape::Model => return Plan::Model,
        Shape::Lathe => e.profile.as_ref().and_then(|p| meshes.lathes.get(p.key())),
        shape => meshes.shapes.get(shape),
    };
    let Some(mesh) = mesh else { return Plan::Skip };
    let c = e.color;
    Plan::Item(DrawItem {
        mesh,
        model: e.model().to_cols_array(),
        color: [c[0], c[1], c[2], e.look.opacity],
        material: e.look.material,
        surface: e.look.surface,
        skin: None,
        bounds: None,
    })
}

fn draw_model(e: &Entity, meshes: &Meshes, view: &mut View, stats: &mut LodStats, out: &mut Vec<DrawItem>) {
    let fov = view.camera.fov_y_deg;
    meshes.models.draw(e, view.camera.eye, fov, view.viewport_h, &mut view.lod, stats, out);
}

/// The window's draw list, kept between frames and patched with what changed. Each persistent entity's
/// fixed draw (built-in shapes, lathes) is kept, and each model entity's drawing state, so a frame
/// copies them instead of walking every entity's full record (F3: on the reference iGPU that walk
/// was most of the window's CPU frame at 10k entities). When entities change, only those are
/// read again ([`World::draw_changes_since`]); a full rebuild happens only when the world is
/// replaced, the lathe meshes or `include_ui` change, a change touched every entity, or more than
/// half the entries changed. Models still choose their LOD per frame (each
/// keeps its level here while its entity exists, hidden or not: the window's hysteresis) and
/// animated ones are posed from their entity. Transient entities (a game's presentation, moving
/// every tick) aren't cached: they follow every persistent one in id order and are drawn live.
/// Same draws, same order as [`draw_list`]; nothing allocated once warm.
#[derive(Default)]
pub struct DrawCache {
    /// `(world generation, LatheMeshes::version, include_ui)` the entries were built for.
    key: Option<(u64, u64, bool)>,
    /// The [`World::draw_epoch`] the entries reflect.
    epoch: u64,
    /// One per drawn shape entity and per model entity (drawn or not), in id order.
    entries: Vec<Cached>,
    /// The previous entries while rebuilding (their LOD levels carry over); kept for its capacity.
    spare: Vec<Cached>,
    /// Changed ids while patching (reused).
    dirty: Vec<u64>,
    /// Transient models' remembered LOD levels (while their entity exists).
    transient_levels: BTreeMap<u64, u8>,
    rebuilds: u64,
    patches: u64,
    /// Captures' cache ([`DrawCache::for_captures`]): models choose their LOD afresh every frame
    /// (`LodChoice::Fresh`, reproducible), never from a remembered level.
    fresh: bool,
}

#[derive(Clone, Copy)]
enum Cached {
    Item { id: u64, d: DrawItem },
    Model {
        m: ModelDraw,
        /// Has an `anim`: posed from the entity each frame.
        animated: bool,
        /// In the frame (shown, and not `ui` unless the view includes it). Hidden models keep
        /// their entry for their LOD level.
        drawn: bool,
        /// The LOD it was last drawn at (`u8::MAX`: none yet).
        level: u8,
    },
}

impl Cached {
    fn id(&self) -> u64 {
        match self {
            Cached::Item { id, .. } => *id,
            Cached::Model { m, .. } => m.id,
        }
    }

    /// `e`'s entry, if it has one (`level`: its model's remembered LOD).
    fn of(e: &Entity, meshes: &Meshes, include_ui: bool, level: u8) -> Option<Cached> {
        if e.shape == Shape::Model {
            return Some(Cached::Model { m: ModelDraw::of(e), animated: e.anim.is_some(), drawn: in_frame(e, include_ui), level });
        }
        match plan(e, meshes, include_ui) {
            Plan::Item(d) => Some(Cached::Item { id: e.id, d }),
            Plan::Skip | Plan::Model => None,
        }
    }

    fn level(&self) -> u8 {
        match self {
            Cached::Model { level, .. } => *level,
            Cached::Item { .. } => u8::MAX,
        }
    }
}

impl DrawCache {
    /// A cache for captures: the same draws in the same order as [`draw_list`] with
    /// `LodChoice::Fresh` (each model's LOD chosen afresh, as captures always did), kept between
    /// captures and patched with what changed, so a capture of a 20,000-entity world no longer
    /// reads every entity (M5 gpuwork; `docs/design/WORKERS.md`).
    pub fn for_captures() -> DrawCache {
        DrawCache { fresh: true, ..DrawCache::default() }
    }

    /// How many times the entries were rebuilt from every entity.
    pub fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// How many frames patched the entries with changed entities only.
    pub fn patches(&self) -> u64 {
        self.patches
    }

    /// [`draw_list`] for `world` (`generation` changes whenever the engine replaces its world).
    /// LODs are chosen by size with each model's remembered level, or `view.lod`'s if Fixed.
    pub fn draw_list(&mut self, world: &World, generation: u64, meshes: &Meshes, view: &mut View, out: &mut Vec<DrawItem>) -> LodStats {
        self.draw_list_owned(world, generation, meshes, view, out, None)
    }

    /// [`DrawCache::draw_list`], also naming each draw's entity in `owners` (the editor's drag
    /// preview moves the dragged entities' draws: docs/design/EDITOR.md).
    pub fn draw_list_owned(&mut self, world: &World, generation: u64, meshes: &Meshes, view: &mut View, out: &mut Vec<DrawItem>, mut owners: Option<&mut Vec<u64>>) -> LodStats {
        if let Some(o) = owners.as_deref_mut() {
            o.clear();
        }
        let key = (generation, meshes.lathes.version(), view.include_ui);
        let same_world = self.key.is_some_and(|k| k.0 == generation);
        let fresh = self.key == Some(key) && (self.epoch == world.draw_epoch() || self.patch(world, meshes, view.include_ui));
        let mut stats = LodStats::default();
        out.clear();
        if fresh {
            for c in &mut self.entries {
                let before = out.len();
                draw_cached(c, world, meshes, view, self.fresh, &mut stats, out);
                if let Some(o) = owners.as_deref_mut() {
                    o.extend(std::iter::repeat_n(c.id(), out.len() - before));
                }
            }
        } else {
            // Rebuild, drawing as it goes. Levels of the previous entries carry over by id (both
            // lists are in id order) unless the world was replaced.
            std::mem::swap(&mut self.entries, &mut self.spare);
            self.entries.clear();
            let mut old = self.spare.iter().filter(|_| same_world).map(|c| (c.id(), c.level())).peekable();
            for e in world.persistent_entities() {
                while old.next_if(|(id, _)| *id < e.id).is_some() {}
                let level = old.next_if(|(id, _)| *id == e.id).map_or(u8::MAX, |(_, l)| l);
                if let Some(c) = Cached::of(e, meshes, view.include_ui, level) {
                    self.entries.push(c);
                    if let Some(c) = self.entries.last_mut() {
                        let before = out.len();
                        draw_cached(c, world, meshes, view, self.fresh, &mut stats, out);
                        if let Some(o) = owners.as_deref_mut() {
                            o.extend(std::iter::repeat_n(c.id(), out.len() - before));
                        }
                    }
                }
            }
            self.spare.clear();
            self.key = Some(key);
            self.epoch = world.draw_epoch();
            self.rebuilds += 1;
            if !same_world {
                self.transient_levels.clear();
            }
        }
        // Transient entities (game presentation, changing every tick) come after every
        // persistent one: drawn live, each model with its remembered level.
        if self.transient_levels.len() > world.transient_len() {
            self.transient_levels.retain(|id, _| world.get(*id).is_some());
        }
        let mut transient = View { camera: view.camera, viewport_h: view.viewport_h, include_ui: view.include_ui, lod: match view.lod {
            LodChoice::Fixed(l) => LodChoice::Fixed(l),
            _ if self.fresh => LodChoice::Fresh,
            _ => LodChoice::Remembered(&mut self.transient_levels),
        } };
        for e in world.transient_entities() {
            let before = out.len();
            match plan(e, meshes, transient.include_ui) {
                Plan::Skip => {}
                Plan::Item(d) => out.push(d),
                Plan::Model => draw_model(e, meshes, &mut transient, &mut stats, out),
            }
            if let Some(o) = owners.as_deref_mut() {
                o.extend(std::iter::repeat_n(e.id, out.len() - before));
            }
        }
        stats
    }

    /// Brings the entries up to date with the entities changed since `self.epoch`; false when
    /// a rebuild is cheaper (or the world's change record no longer reaches back that far).
    /// Changed entries are replaced in place; when entities came or went, one linear merge of
    /// the entries with the changed ids makes the new list (never an insert or remove per id).
    fn patch(&mut self, world: &World, meshes: &Meshes, include_ui: bool) -> bool {
        let Some(changes) = world.draw_changes_since(self.epoch) else { return false };
        // At most World's DRAW_LOG ids; an entity moved many times counts once.
        self.dirty.clear();
        self.dirty.extend(changes); // persistent entities only (see World::draw_epoch)
        self.dirty.sort_unstable();
        self.dirty.dedup();
        if self.dirty.len() > self.entries.len() / 2 + 16 {
            return false;
        }
        let mut structural = false;
        for &id in &self.dirty {
            let at = self.entries.binary_search_by_key(&id, Cached::id);
            let level = at.map_or(u8::MAX, |i| self.entries[i].level());
            match (at, world.get(id).and_then(|e| Cached::of(e, meshes, include_ui, level))) {
                (Ok(i), Some(c)) => self.entries[i] = c,
                (Err(_), None) => {}
                _ => structural = true,
            }
        }
        if structural {
            // Unchanged entries are copied as they are; each changed id is read again and kept
            // if it draws (with its model's level).
            self.spare.clear();
            let mut old = self.entries.iter().peekable();
            for &d in &self.dirty {
                while let Some(c) = old.next_if(|c| c.id() < d) {
                    self.spare.push(*c);
                }
                let level = old.next_if(|c| c.id() == d).map_or(u8::MAX, Cached::level);
                if let Some(c) = world.get(d).and_then(|e| Cached::of(e, meshes, include_ui, level)) {
                    self.spare.push(c);
                }
            }
            self.spare.extend(old.copied());
            std::mem::swap(&mut self.entries, &mut self.spare);
            self.spare.clear();
        }
        self.epoch = world.draw_epoch();
        self.patches += 1;
        true
    }

}

/// Appends a cached entry's draws (a model's at its LOD for this view, posed if animated).
fn draw_cached(c: &mut Cached, world: &World, meshes: &Meshes, view: &View, fresh: bool, stats: &mut LodStats, out: &mut Vec<DrawItem>) {
    match c {
        Cached::Item { d, .. } => out.push(*d),
        Cached::Model { m, animated, drawn, level } => {
            if !*drawn {
                return;
            }
            let entity = if *animated { world.get(m.id) } else { None };
            let mut choice = match view.lod {
                LodChoice::Fixed(l) => LodChoice::Fixed(l),
                _ if fresh => LodChoice::Fresh,
                _ => LodChoice::Slot(level),
            };
            meshes.models.draw_model(m, entity, view.camera.eye, view.camera.fov_y_deg, view.viewport_h, &mut choice, stats, out);
        }
    }
}

/// The point lights of shown entities in the frame (entity id order), at most
/// [`MAX_POINT_LIGHTS`], into `out` (cleared first; reused, so no allocation once warm).
/// Returns how many were left out. Visits only the world's lit entities ([`World::lit_entities`]),
/// never the rest: a 10k-entity world with a few lamps pays for the lamps.
pub fn lights(world: &World, include_ui: bool, out: &mut Vec<PointLight>) -> usize {
    out.clear();
    let mut dropped = 0;
    for light in world.lit_entities().filter(|e| in_frame(e, include_ui)).filter_map(|e| e.world_light()) {
        if out.len() < MAX_POINT_LIGHTS {
            out.push(light);
        } else {
            dropped += 1;
        }
    }
    dropped
}

/// A frame of the world seen through `view_proj` (reversed Z) at animation time `time_s`.
pub fn frame<'a>(world: &World, view_proj: &Mat4, draws: &'a [DrawItem], lights: &'a [PointLight], time_s: f32) -> FrameDesc<'a> {
    let b = world.env().background;
    FrameDesc {
        view_proj: view_proj.to_cols_array(),
        clear_color: [b[0], b[1], b[2], 1.0],
        draws,
        reverse_z: true,
        environment: world.env().light,
        lights,
        time_s,
        ..Default::default()
    }
}

/// What a form view's frame is drawn against (sRGB): a dark neutral that clay, both ends of a
/// curvature view and both stripes stand off from.
pub const FORM_BACKGROUND: [f32; 4] = [0.09, 0.09, 0.10, 1.0];

/// The frame of a form view ([`specula_gpu::FormView`]): nothing of the world's light, sky, fog
/// or post effects (the views write the colours they mean, which a tone curve, a grade, bloom
/// or anti-aliasing would change), no cast shadows and no baked light to prepare, and a neutral
/// ground behind. The wind stays: what sways is where the world's frame draws it.
pub fn form_frame(frame: &mut FrameDesc, view: specula_gpu::FormView, scale: f32) {
    let wind = frame.environment.wind;
    let mut plain = specula_gpu::Environment { exposure: 1.0, tonemap: false, fxaa: false, wind, ..Default::default() };
    plain.shadow.enabled = false;
    plain.bloom.enabled = false;
    plain.sky.enabled = false;
    plain.fog.enabled = false;
    plain.grading.enabled = false;
    plain.ssao.enabled = false;
    frame.environment = plain;
    frame.clear_color = FORM_BACKGROUND;
    frame.effects = Default::default();
    frame.gi = Default::default();
    frame.lights = &[];
    frame.form = view;
    frame.form_scale = scale;
}

/// Whether any of `e`'s bounding box can be in view of `view_proj` (conservative: false only
/// when every corner is beyond one side of the view, or behind it). Entities without bounds count
/// as in view.
pub fn in_view(e: &Entity, view_proj: &Mat4) -> bool {
    let Some(corners) = e.world_corners() else { return true };
    let clip = corners.map(|p| view_proj.mul_vec4(p.extend(1.0)));
    let all = |f: &dyn Fn(&Vec4) -> bool| clip.iter().all(f);
    !(all(&|p| p.w <= 0.0) || all(&|p| p.x < -p.w) || all(&|p| p.x > p.w) || all(&|p| p.y < -p.w) || all(&|p| p.y > p.w))
}

/// What a subject's packet entry says about its entity, whatever the camera: read once per
/// change of the entity ([`SubjectCache`]), never per capture. The JSON is made from it on a
/// worker ([`Subject::full`]).
#[derive(Clone, Debug)]
pub struct SubjectBase {
    pub id: u64,
    pub name: String,
    pub path: String,
    shape: &'static str,
    what: What,
    color: [f32; 3],
    pub position: Vec3,
    created_by: std::sync::Arc<str>,
    transient: bool,
    /// On the `ui` layer (here or above).
    ui: bool,
    /// Shown (visible here and above).
    shown: bool,
    /// Its world bounding box's corners.
    corners: [Vec3; 8],
    /// A 2D entity: a billboard's card turns to the camera, so its box is made per view from
    /// the entity.
    flat: bool,
}

#[derive(Clone, Debug)]
enum What {
    Recipe(String),
    TwoD(Json),
    Material(&'static str),
}

impl SubjectBase {
    /// The subject base of `e`, or `None` when it has no geometry (groups, models not built).
    pub fn of(world: &World, e: &Entity) -> Option<SubjectBase> {
        e.local_bounds()?;
        let corners = e.world_corners()?;
        let what = match (&e.model, &e.two_d) {
            (Some(m), _) => What::Recipe(m.recipe.clone()),
            (None, Some(t)) => What::TwoD(t.subject_json()),
            (None, None) => What::Material(material_name(e.look.material)),
        };
        Some(SubjectBase {
            id: e.id,
            name: e.name.clone(),
            path: world.path_of(e.id),
            shape: e.shape.name(),
            what,
            color: e.color,
            position: e.world_position(),
            created_by: std::sync::Arc::clone(&e.created_by),
            transient: e.transient,
            ui: e.in_ui(),
            shown: e.shown(),
            corners,
            flat: e.two_d.is_some(),
        })
    }

    /// A model entity (its subject names its recipe).
    pub fn is_model(&self) -> bool {
        self.shape == Shape::Model.name()
    }

    /// In a frame that does (or doesn't) include the `ui` layer ([`in_frame`]).
    fn in_frame(&self, include_ui: bool) -> bool {
        self.shown && (include_ui || !self.ui)
    }
}

/// Every persistent entity's [`SubjectBase`], kept from the world's change record
/// ([`World::draw_changes_since`]) so a capture never reads every entity again: built once,
/// then patched with what changed (the window's draw cache works the same way). Paths of a
/// renamed entity's subtree are read again; a replaced world, a change to every entity or more
/// changes than the record holds rebuild it. Transient entities (game presentation) are read
/// live at each capture.
#[derive(Default)]
pub struct SubjectCache {
    /// The world generation and draw epoch the entries reflect.
    key: Option<(u64, u64)>,
    layout: u64,
    /// The CJK re-layout batch frames had adopted when text subjects were last read
    /// ([`crate::text_world::adopted_batch`]): a re-laid text changes no draw stamp.
    text_batch: u64,
    entries: Vec<std::sync::Arc<SubjectBase>>,
    dirty: Vec<u64>,
    rebuilds: u64,
    patches: u64,
    scans: u64,
    /// A rebuild in slices ([`SubjectCache::warm`]).
    building: Option<Rebuild>,
}

/// A rebuild in progress: the entries so far, from the world as it was at `epoch` onwards.
struct Rebuild {
    generation: u64,
    epoch: u64,
    layout: u64,
    after: Option<u64>,
    entries: Vec<std::sync::Arc<SubjectBase>>,
}

impl SubjectCache {
    /// Whether [`SubjectCache::refresh`] would be cheap now: up to date, a patch with the changes
    /// the world's record still holds, or (past the record) with what the entities' draw stamps
    /// say changed; not a rebuild from every entity.
    pub fn ready(&self, world: &World, generation: u64) -> bool {
        let limit = self.patch_limit();
        match self.key {
            Some((g, e)) if g == generation => {
                e == world.draw_epoch()
                    || world.draw_changes_since(e).map_or_else(|| world.draw_stamps_reach(e) && Self::count_changed(&self.entries, world, e, limit) <= limit, |c| c.count() <= limit)
            }
            _ => false,
        }
    }

    /// Past this many changed entities a patch costs about a rebuild: [`SubjectCache::warm`]
    /// rebuilds in slices instead (a world of 20,000 just built would otherwise be read again in
    /// one go by the next capture).
    fn patch_limit(&self) -> usize {
        self.entries.len() / 2 + 16
    }

    /// How many entities changed after `epoch` by their draw stamps (as
    /// [`SubjectCache::changed_by_stamps`] lists them), counting no further than past `stop_at`.
    fn count_changed(entries: &[std::sync::Arc<SubjectBase>], world: &World, epoch: u64, stop_at: usize) -> usize {
        let mut cached = entries.iter().map(|b| b.id).peekable();
        let mut n = 0;
        for e in world.persistent_entities() {
            while cached.next_if(|&id| id < e.id).is_some() {
                n += 1;
            }
            let _ = cached.next_if(|&id| id == e.id);
            n += usize::from(e.draw_stamp() > epoch);
            if n > stop_at {
                return n;
            }
        }
        n + cached.count()
    }

    /// Works towards [`SubjectCache::ready`] for up to `budget`: a rebuild reads the entities a
    /// slice at a time (by id) while the world goes on changing, then patches what changed
    /// meanwhile (from the change record or the draw stamps, so any amount of churn lets it
    /// finish). True once ready. A rebuild overtaken by a change to every entity at once is not
    /// started again (it could be overtaken forever): the cache is read synchronously instead.
    pub fn warm(&mut self, world: &World, generation: u64, budget: std::time::Duration) -> bool {
        if self.ready(world, generation) {
            self.building = None;
            return true;
        }
        let started = std::time::Instant::now();
        let mut b = match self.building.take() {
            Some(b) if b.generation == generation && world.draw_stamps_reach(b.epoch) => b,
            Some(_) => {
                self.refresh(world, generation);
                return true;
            }
            // (A new world, or a change to every entity.)
            None => Rebuild { generation, epoch: world.draw_epoch(), layout: world.layout_epoch(), after: None, entries: Vec::new() },
        };
        let mut n = 0usize;
        for e in world.persistent_entities_between(b.after, u64::MAX) {
            b.after = Some(e.id);
            if let Some(base) = SubjectBase::of(world, e) {
                b.entries.push(std::sync::Arc::new(base));
            }
            n += 1;
            if n.is_multiple_of(64) && started.elapsed() >= budget {
                self.building = Some(b);
                return false;
            }
        }
        // Every entity read: what changed since it began is patched in.
        self.entries = b.entries;
        self.key = Some((generation, b.epoch));
        self.layout = b.layout;
        self.rebuilds += 1;
        let patched = self.key == Some((generation, world.draw_epoch())) || self.patch(world, b.epoch, false);
        if patched {
            self.key = Some((generation, world.draw_epoch()));
            self.layout = world.layout_epoch();
        } else {
            // (Not reached while the stamps reach back, checked above: read synchronously.)
            self.key = None;
            self.refresh(world, generation);
        }
        true
    }
    /// How many times the entries were read from every entity, and patched.
    pub fn counts(&self) -> (u64, u64) {
        (self.rebuilds, self.patches)
    }

    /// Brings the entries up to `world` (`generation` changes whenever the engine replaces it).
    pub fn refresh(&mut self, world: &World, generation: u64) {
        let batch = crate::text_world::adopted_batch();
        if self.text_batch != batch {
            self.reread_texts(world, batch);
        }
        if self.key == Some((generation, world.draw_epoch())) {
            return;
        }
        let patched = match self.key {
            Some((g, epoch)) if g == generation => self.patch(world, epoch, true),
            _ => false,
        };
        if !patched {
            let _z = specula_core::profile::zone("sight.subject_cache");
            self.entries.clear();
            self.entries.extend(world.persistent_entities().filter_map(|e| SubjectBase::of(world, e)).map(std::sync::Arc::new));
            self.rebuilds += 1;
        }
        self.key = Some((generation, world.draw_epoch()));
        self.layout = world.layout_epoch();
    }

    /// Reads text subjects again after frames adopted a batch of CJK re-layouts (their box and
    /// facts came from the layout made before the face arrived; review of e97e318: packets kept
    /// reporting it while frames drew the new one). At most once per fallback face.
    fn reread_texts(&mut self, world: &World, batch: u64) {
        self.text_batch = batch;
        for c in &mut self.entries {
            if c.shape == "text" {
                if let Some(b) = world.get(c.id).filter(|e| !e.transient).and_then(|e| SubjectBase::of(world, e)) {
                    *c = std::sync::Arc::new(b);
                }
            }
        }
    }

    /// `bounded`: false when more than [`SubjectCache::patch_limit`] changed (a rebuild is cheaper).
    fn patch(&mut self, world: &World, epoch: u64, bounded: bool) -> bool {
        self.dirty.clear();
        // From the change record while it reaches back that far; past it (more changes since
        // than it holds: bodies moving every tick) from the entities' draw stamps, which never
        // fall behind. Only a change to every entity at once makes a rebuild.
        match world.draw_changes_since(epoch) {
            Some(changes) => self.dirty.extend(changes),
            None if world.draw_stamps_reach(epoch) => {
                Self::changed_by_stamps(&self.entries, world, epoch, &mut self.dirty);
                self.scans += 1;
            }
            None => return false,
        }
        self.dirty.sort_unstable();
        self.dirty.dedup();
        if bounded && self.dirty.len() > self.patch_limit() {
            return false;
        }
        // A rename moves the paths of everything below (without changing those entities):
        // the subtrees of changed entities with children are read again when names or parents
        // may have changed (the layout moved).
        if world.layout_epoch() != self.layout {
            let mut below = Vec::new();
            for &id in &self.dirty {
                if world.get(id).is_some() && world.has_children(id) {
                    world.walk(Some(id), |e, _| {
                        if e.id != id && !e.transient {
                            below.push(e.id);
                        }
                    });
                }
            }
            if !below.is_empty() {
                self.dirty.extend(below);
                self.dirty.sort_unstable();
                self.dirty.dedup();
                if bounded && self.dirty.len() > self.patch_limit() {
                    return false;
                }
            }
        }
        // One merge of the entries with the changed ids (each read again, kept if it has
        // geometry).
        let old = std::mem::take(&mut self.entries);
        let mut out = Vec::with_capacity(old.len() + 16);
        let mut it = old.into_iter().peekable();
        for &d in &self.dirty {
            while let Some(c) = it.next_if(|c| c.id < d) {
                out.push(c);
            }
            let _ = it.next_if(|c| c.id == d);
            if let Some(b) = world.get(d).filter(|e| !e.transient).and_then(|e| SubjectBase::of(world, e)) {
                out.push(std::sync::Arc::new(b));
            }
        }
        out.extend(it);
        self.entries = out;
        self.patches += 1;
        true
    }

    /// The ids that changed after `epoch` by the entities' draw stamps ([`World::draw_stamps_reach`]
    /// holds): one merge of `entries` (by id) with the persistent entities. Entries whose entity
    /// is gone are listed too; an entity without an entry that hasn't changed had no geometry then
    /// and still has none.
    fn changed_by_stamps(entries: &[std::sync::Arc<SubjectBase>], world: &World, epoch: u64, dirty: &mut Vec<u64>) {
        let mut cached = entries.iter().map(|b| b.id).peekable();
        for e in world.persistent_entities() {
            while let Some(gone) = cached.next_if(|&id| id < e.id) {
                dirty.push(gone);
            }
            let _ = cached.next_if(|&id| id == e.id);
            if e.draw_stamp() > epoch {
                dirty.push(e.id);
            }
        }
        dirty.extend(cached);
    }

    /// How many times the entries were patched from the entities' draw stamps (the change record
    /// had overflowed).
    pub fn scans(&self) -> u64 {
        self.scans
    }

    /// [`world_boxes`] from the entries (up to date: [`SubjectCache::refresh`]) and the live
    /// transient entities: the box of everything shown and not on the `ui` layer.
    pub fn boxes(&self, world: &World) -> Vec<(u64, Vec3, Vec3)> {
        let cached = self.entries.iter().filter(|b| b.shown && !b.ui).map(|b| (b.id, b.corners));
        let live = world.transient_entities().filter(|e| e.shown() && !e.in_ui()).filter_map(|e| Some((e.id, e.world_corners()?)));
        cached.chain(live).map(|(id, c)| {
            let (lo, hi) = corners_box(&c);
            (id, lo, hi)
        }).collect()
    }

    /// The entries (persistent entities with geometry, by id).
    pub fn entries(&self) -> &[std::sync::Arc<SubjectBase>] {
        &self.entries
    }
}

/// Where one entity landed in a frame: its [`SubjectBase`] and what this view adds. Its packet
/// entry ([`Subject::full`]) is made only where it is sent (on a worker for a capture's
/// packet); `json` holds the fields measurements added after projection (visibility,
/// streaming, terrain), in the order they were set.
#[derive(Clone, Debug)]
pub struct Subject {
    pub id: u64,
    pub base: std::sync::Arc<SubjectBase>,
    pub on_screen: bool,
    /// The clipped projected rectangle `[x, y, w, h]` (`None`: none of it in view).
    pub screen: Option<[u32; 4]>,
    /// Distance along the view axis to the nearest visible part of the box (on screen only).
    pub distance: f32,
    /// What the ID buffer measured (`None`: it didn't run).
    pub vis: Option<Visibility>,
    /// Fields added after that (`drawn_as`, `kind`...), in order; a key here replaces the
    /// measured one in place (an HLOD member's `visible_px` is null). Private: the computed
    /// fields (`screen`, `distance_m`, `on_screen`...) aren't in it since they are made on demand,
    /// so readers go through [`Subject::get`], [`Subject::added`] or the typed fields (the M4
    /// final audit: anim_filmstrip read `screen` from here and measured the tile's middle).
    json: Json,
    /// Pixel area of the clipped rectangle (0 when off screen).
    /// Becomes the exact box's area once the ID buffer ran ([`apply_visibility`]): ranks the response.
    pub area: u64,
    /// Pixel area of the projected rectangle, always (overlays rank by it, so their picks never
    /// depend on whether the ID buffer ran first).
    pub projected_area: u64,
}

impl Subject {
    /// The subject's packet entry: `id`, `name`, `path`, `shape`, `recipe` (models) or `two_d`
    /// or `material`, `color`, `position`, `created_by`, `on_screen`, `screen`, `distance_m`,
    /// `transient` and `layer` when they apply, then the measured fields.
    pub fn full(&self) -> Json {
        let b = &*self.base;
        let mut fields = Vec::with_capacity(14 + self.json.as_object().map_or(0, <[(String, Json)]>::len));
        fields.push(("id".to_owned(), Json::from(b.id)));
        fields.push(("name".to_owned(), Json::from(b.name.as_str())));
        fields.push(("path".to_owned(), Json::from(b.path.as_str())));
        fields.push(("shape".to_owned(), Json::from(b.shape)));
        match &b.what {
            What::Recipe(r) => fields.push(("recipe".to_owned(), Json::from(r.as_str()))),
            What::TwoD(j) => fields.push(("two_d".to_owned(), j.clone())),
            What::Material(m) => fields.push(("material".to_owned(), Json::from(*m))),
        }
        fields.push(("color".to_owned(), Json::from(color_hex(b.color))));
        fields.push(("position".to_owned(), vec3_json(b.position)));
        fields.push(("created_by".to_owned(), Json::from(&*b.created_by)));
        fields.push(("on_screen".to_owned(), Json::from(self.on_screen)));
        fields.push(("screen".to_owned(), self.screen.map_or(Json::Null, |r| Json::arr(r.map(Json::from)))));
        fields.push(("distance_m".to_owned(), if self.screen.is_some() { Json::from(self.distance) } else { Json::Null }));
        if b.transient {
            fields.push(("transient".to_owned(), Json::from(true)));
        }
        if b.ui {
            fields.push(("layer".to_owned(), Json::from("ui")));
        }
        if let Some(v) = &self.vis {
            v.write(&mut fields);
        }
        let mut out = Json::Obj(fields);
        if let Json::Obj(extra) = &self.json {
            for (k, v) in extra {
                out.set(k.as_str(), v.clone());
            }
        }
        out
    }

    /// A subject with `json` as its measured fields, of a stand-in cube (tests of what reads
    /// them).
    #[cfg(test)]
    pub fn for_test(id: u64, json: Json, on_screen: bool, projected_area: u64) -> Subject {
        let base = SubjectBase {
            id,
            name: format!("e{id}"),
            path: format!("/e{id}"),
            shape: "cube",
            what: What::Material("standard"),
            color: [1.0; 3],
            position: Vec3::ZERO,
            created_by: std::sync::Arc::from("t"),
            transient: false,
            ui: false,
            shown: true,
            corners: [Vec3::ZERO; 8],
            flat: false,
        };
        Subject { id, base: std::sync::Arc::new(base), on_screen, screen: None, distance: 0.0, vis: None, json, area: 0, projected_area }
    }

    /// Sets a field added to its packet entry after the measure (`kind`, `drawn_as`...).
    pub fn set(&mut self, key: &str, value: Json) {
        debug_assert!(!COMPUTED.contains(&key), "'{key}' is computed on demand: set the typed field");
        self.json.set(key, value);
    }

    /// A field added to its packet entry after the measure (`drawn_as`, `kind`, a parsed
    /// packet's `no_pixels`), never a computed one: `screen`, `distance_m` and the like are
    /// [`Subject::get`]'s or the typed fields' (`screen`, `on_screen`, `distance`).
    pub fn added(&self, key: &str) -> Option<&Json> {
        debug_assert!(!COMPUTED.contains(&key), "'{key}' is computed on demand: read it with Subject::get or the typed field");
        self.json.get(key)
    }

    /// [`shows`] of its packet entry, without making it.
    pub fn shows(&self) -> Option<bool> {
        if self.json.get("visible_px").is_some() {
            return shows(&self.json);
        }
        let v = self.vis?;
        Some(v.px? > 0 || v.approx)
    }

    /// Its `visible_px` (`None`: null or absent).
    pub fn visible_px(&self) -> Option<u64> {
        match self.json.get("visible_px") {
            Some(v) => v.as_u64(),
            None => self.vis.and_then(|v| v.px),
        }
    }

    /// One field of [`Subject::full`] (without making the rest).
    pub fn get(&self, key: &str) -> Option<Json> {
        if let Some(v) = self.json.get(key) {
            return Some(v.clone());
        }
        let b = &*self.base;
        Some(match key {
            "id" => Json::from(b.id),
            "name" => Json::from(b.name.as_str()),
            "path" => Json::from(b.path.as_str()),
            "position" => vec3_json(b.position),
            "on_screen" => Json::from(self.on_screen),
            "screen" => self.screen.map_or(Json::Null, |r| Json::arr(r.map(Json::from))),
            "distance_m" => {
                if self.screen.is_some() {
                    Json::from(self.distance)
                } else {
                    Json::Null
                }
            }
            "layer" if b.ui => Json::from("ui"),
            "transient" if b.transient => Json::from(true),
            _ => return self.full().get(key).cloned(),
        })
    }
}

/// The packet fields a [`Subject`] makes on demand from its typed fields and base (never kept in
/// its added fields).
const COMPUTED: [&str; 15] = ["id", "name", "path", "shape", "recipe", "two_d", "material", "color", "position", "created_by", "on_screen", "screen", "distance_m", "layer", "transient"];

/// One entry per entity with geometry in the frame, in id order: groups draw nothing, hidden
/// entities aren't drawn, and `ui` entities only count when the frame includes them.
///
/// `screen` is the pixel rectangle `[x, y, w, h]` (origin top-left) of the entity's bounding box
/// after clipping to the near plane and the image, or null when none of it is in view.
/// `distance_m` is the distance along the view axis to the nearest visible part of the box.
/// `position` is the world position. Models carry their `recipe`; transient entities say so.
///
/// Reads every entity: tests and one-off callers. Captures use [`subjects_cached`].
pub fn subjects(world: &World, view_proj: &Mat4, width: u32, height: u32, include_ui: bool) -> Vec<Subject> {
    let mut cache = SubjectCache::default();
    cache.refresh(world, 0);
    subjects_cached(&cache, world, view_proj, width, height, include_ui)
}

/// [`subjects`] from the cache (brought up to date by the caller): only the projection is made
/// per capture, plus transient entities and 2D cards read live.
pub fn subjects_cached(cache: &SubjectCache, world: &World, view_proj: &Mat4, width: u32, height: u32, include_ui: bool) -> Vec<Subject> {
    subject_set(cache, world, view_proj, width, height, include_ui).project()
}

/// A capture's subjects before their projection: the bases in the frame, in [`subjects`]'s
/// order, each with its 2D card as drawn (billboards turn to the camera). Made on the engine
/// thread (it reads the world: transient entities, cards); [`SubjectSet::project`] needs nothing
/// else, so a worker makes the projection (6-8 ms at 20,000 entities on the reference iGPU,
/// `docs/design/WORKERS.md`).
pub struct SubjectSet {
    items: Vec<(std::sync::Arc<SubjectBase>, Option<[Vec3; 4]>)>,
    view_proj: Mat4,
    width: u32,
    height: u32,
    ortho: bool,
}

/// [`subjects_cached`] up to the projection (see [`SubjectSet`]).
pub fn subject_set(cache: &SubjectCache, world: &World, view_proj: &Mat4, width: u32, height: u32, include_ui: bool) -> SubjectSet {
    let view = specula_gpu::view_2d(&view_proj.to_cols_array(), true);
    let card = |base: &SubjectBase, e: Option<&Entity>| -> Option<[Vec3; 4]> {
        if !base.flat {
            return None;
        }
        e.or_else(|| world.get(base.id)).and_then(|e| e.two_d.as_ref().and_then(|t| crate::two_d::billboard_corners(t, &e.model(), &view)))
    };
    let mut items: Vec<(std::sync::Arc<SubjectBase>, Option<[Vec3; 4]>)> =
        cache.entries().iter().filter(|b| b.in_frame(include_ui)).map(|b| (std::sync::Arc::clone(b), card(b, None))).collect();
    for e in world.transient_entities() {
        if let Some(b) = SubjectBase::of(world, e).filter(|b| b.in_frame(include_ui)) {
            let c = card(&b, Some(e));
            items.push((std::sync::Arc::new(b), c));
        }
    }
    SubjectSet { items, view_proj: *view_proj, width, height, ortho: view.ortho }
}

impl SubjectSet {
    /// No subjects (tests of who projects a view's subjects).
    #[cfg(test)]
    pub(crate) fn empty() -> SubjectSet {
        SubjectSet { items: Vec::new(), view_proj: Mat4::IDENTITY, width: 1, height: 1, ortho: false }
    }

    /// The bases, in the subjects' order (index `i` is the projection's `i`-th subject).
    pub fn bases(&self) -> impl Iterator<Item = &SubjectBase> {
        self.items.iter().map(|(b, _)| &**b)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The subjects, projected (the same as [`subjects_cached`] made from the same world).
    pub fn project(&self) -> Vec<Subject> {
        self.items
            .iter()
            .map(|(base, card)| {
                let (screen, distance) = project_box(&base.corners, *card, &self.view_proj, self.width, self.height, self.ortho);
                let area = screen.map_or(0, |r| u64::from(r[2]) * u64::from(r[3]));
                Subject {
                    id: base.id,
                    base: std::sync::Arc::clone(base),
                    on_screen: screen.is_some(),
                    screen,
                    distance,
                    vis: None,
                    json: Json::Obj(Vec::new()),
                    area,
                    projected_area: area,
                }
            })
            .collect()
    }
}

/// The clipped box's screen rectangle and nearest view depth ([`clip_points`], without
/// collecting them).
fn project_box(corners: &[Vec3; 8], card: Option<[Vec3; 4]>, view_proj: &Mat4, width: u32, height: u32, ortho: bool) -> (Option<[u32; 4]>, f32) {
    let corners = match card {
        // A flat card: its four corners twice make the "box" (zero depth).
        Some(c) => [c[0], c[1], c[3], c[2], c[0], c[1], c[3], c[2]],
        None => *corners,
    };
    let mut rect = Rect::default();
    let mut distance = f32::INFINITY;
    // Perspective: clip w is the view depth. Orthographic (w = 1): undo the reversed-Z depth
    // range of `Projection::Ortho` instead.
    let depth = |p: &Vec4| if ortho { NEAR + (1.0 - p.z / p.w) * (ORTHO_FAR - NEAR) } else { p.w };
    clip_points(&corners, view_proj, |p| {
        rect.add(&p, width, height);
        distance = distance.min(depth(&p));
    });
    (rect.finish(width, height), distance)
}
/// Dense ID-buffer ids for a draw list's owners (`draw_list`): the `k`-th distinct entity gets
/// `k + 1`. Returns `(id per draw, entity of each dense id - 1)`.
pub fn dense_ids(owners: &[u64]) -> (Vec<u32>, Vec<u64>) {
    let mut entities: Vec<u64> = Vec::new();
    let mut index: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();
    // Draw lists come in entity id order (the draw caches), each entity's draws together: the
    // same owner again, or one above every owner so far, needs no lookup (a capture of 20,000
    // entities spent milliseconds hashing them). Owners out of that order use the map.
    let (mut last, mut last_id, mut top, mut sorted) = (0u64, 0u32, 0u64, 0usize);
    let ids = owners
        .iter()
        .map(|&e| {
            // Owner 0 (an HLOD proxy's draw, many entities at once): not counted.
            if e == 0 {
                return 0;
            }
            if e == last && last_id != 0 {
                return last_id;
            }
            let id = if e > top && sorted == entities.len() {
                // Still in order: the entities so far are sorted (searched below, never hashed).
                entities.push(e);
                sorted = entities.len();
                top = e;
                entities.len() as u32
            } else if let Ok(i) = entities[..sorted].binary_search(&e) {
                i as u32 + 1
            } else {
                *index.entry(e).or_insert_with(|| {
                    entities.push(e);
                    entities.len() as u32
                })
            };
            (last, last_id) = (e, id);
            id
        })
        .collect();
    (ids, entities)
}

/// What one rendered frame actually drew, measured: the entities its draw lists submitted (mesh
/// and 2D draws), the ID buffer's coverage per entity when the ID pass ran, and what streaming
/// drew merged (HLOD) or left off the GPU. Facts about drawing are a [`DrawnFact`], which only
/// [`FrameRecord::drawn`] makes: an answer can't say "drawn" from the world's intent
/// (`docs/design/AUTHORITY.md`, "Drawn facts come from a frame record").
///
/// Passes added later (terrain, water) record their entities here when they submit a draw, never
/// in the packet directly.
#[derive(Clone, Debug, Default)]
pub struct FrameRecord {
    submitted: std::collections::BTreeSet<u64>,
    /// The ID pass's coverage per entity (`None`: the pass didn't run).
    ids: Option<BTreeMap<u64, specula_gpu::IdCoverage>>,
    /// The ID pass's dense ids: `entities[i]` drew as id `i + 1`.
    dense: Vec<u64>,
    hlod: std::collections::BTreeSet<u64>,
    /// Terrains the terrain pass drew this frame, and how many patches (0: none in view).
    terrain: BTreeMap<u64, u32>,
    /// Water surfaces outside the mesh lists (their pass records no per-entity pixels).
    water: std::collections::BTreeSet<u64>,
    /// HLOD member → its proxy's owner ([`proxy_owner`]).
    proxy_of: BTreeMap<u64, u64>,
    not_resident: std::collections::BTreeSet<u64>,
}

/// The ID-pass owner of a capture's `i`-th HLOD proxy: past every entity id, so it never names
/// an entity (proxies draw many at once).
pub fn proxy_owner(i: usize) -> u64 {
    (1 << 62) + i as u64
}

/// Whether an owner is a proxy's ([`proxy_owner`]), not an entity's.
pub fn is_proxy_owner(owner: u64) -> bool {
    owner >= 1 << 62
}

/// Whether an entity reached the image, and how that is known (made only by
/// [`FrameRecord::drawn`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawnFact {
    drawn: Option<bool>,
    drawn_as: Option<&'static str>,
    measured_by: &'static str,
    reason: Option<&'static str>,
}

impl DrawnFact {
    /// `Some(true)`: it reached the image; `Some(false)`: it didn't; `None`: unmeasured.
    pub fn drawn(&self) -> Option<bool> {
        self.drawn
    }

    /// How it was drawn when not on its own (`hlod`, `not_resident`).
    pub fn drawn_as(&self) -> Option<&'static str> {
        self.drawn_as
    }

    /// Writes `drawn`, `measured_by`, and `drawn_as`/`reason` when there are any.
    pub fn write(&self, out: &mut Json) {
        out.set("drawn", self.drawn.map_or(Json::Null, Json::Bool));
        out.set("measured_by", Json::from(self.measured_by));
        if let Some(a) = self.drawn_as {
            out.set("drawn_as", Json::from(a));
        }
        if let Some(r) = self.reason {
            out.set("reason", Json::from(r));
        }
    }
}

impl FrameRecord {
    /// The frame's draw owners (one entity id per draw, 0 for a draw of many, such as an HLOD
    /// proxy), as `draw_list` and the 2D lists give them.
    pub fn new(owners: &[u64]) -> FrameRecord {
        FrameRecord { submitted: owners.iter().copied().filter(|&e| e != 0).collect(), ..FrameRecord::default() }
    }

    /// The ID pass's counts (`entities` from [`dense_ids`], `coverage` per dense id).
    pub fn set_ids(&mut self, entities: &[u64], coverage: &[specula_gpu::IdCoverage]) {
        self.ids = Some(entities.iter().zip(coverage).map(|(e, c)| (*e, *c)).collect());
        self.dense = entities.to_vec();
    }

    /// The capture's HLOD proxies (`Hlod::capture_groups`, in draw order): each member's facts
    /// come from its proxy's pixels.
    pub fn set_proxies(&mut self, groups: &[(u32, Vec<u64>)]) {
        for (i, (_, members)) in groups.iter().enumerate() {
            for &m in members {
                self.proxy_of.insert(m, proxy_owner(i));
                self.hlod.insert(m);
            }
        }
    }

    /// The ID-pass owner of the proxy HLOD member `id` was merged in, if it was.
    pub fn proxy_owner_of(&self, id: u64) -> Option<u64> {
        self.proxy_of.get(&id).copied()
    }

    /// A terrain the terrain pass drew with `patches` patches in view (its draws are the pass's
    /// own, outside the mesh lists).
    pub fn set_terrain(&mut self, id: u64, patches: u32) {
        self.terrain.insert(id, patches);
    }

    /// A water surface (its pass draws outside the mesh lists when it isn't one of them).
    pub fn set_water(&mut self, id: u64) {
        self.water.insert(id);
    }

    /// The entity (or proxy owner) the ID pass drew as dense id `id` (1-based), for readbacks.
    pub fn owner_of_id(&self, id: u32) -> Option<u64> {
        (id > 0).then(|| self.dense.get(id as usize - 1).copied()).flatten()
    }

    /// What streaming said of the frame's subjects (`drawn_as`: `hlod` or `not_resident`).
    pub fn set_streaming(&mut self, subjects: &[Subject]) {
        for s in subjects {
            match s.json.get("drawn_as").and_then(Json::as_str) {
                Some("hlod") => {
                    self.hlod.insert(s.id);
                }
                Some("not_resident") => {
                    self.not_resident.insert(s.id);
                }
                _ => {}
            }
        }
    }

    /// Whether the ID pass ran.
    pub fn measured(&self) -> bool {
        self.ids.is_some()
    }

    /// Entities the draw lists submitted.
    pub fn submitted(&self) -> &std::collections::BTreeSet<u64> {
        &self.submitted
    }

    /// The drawing facts of entity `id` in this frame.
    pub fn drawn(&self, id: u64) -> DrawnFact {
        let fact = |drawn, drawn_as, measured_by, reason| DrawnFact { drawn, drawn_as, measured_by, reason };
        if self.not_resident.contains(&id) {
            return fact(Some(false), Some("not_resident"), "streaming", Some("built but not on the GPU (streaming: too far and too small for every camera, or evicted under memory_mb; stream_state shows why)"));
        }
        if let Some(&patches) = self.terrain.get(&id) {
            return if patches > 0 {
                fact(Some(true), Some("terrain"), "terrain_pass", Some("drawn by the terrain pass (patches in view: terrain_patches); its pixels aren't counted per entity"))
            } else {
                fact(Some(false), Some("terrain"), "terrain_pass", Some("the terrain pass drew none of its patches: none in view"))
            };
        }
        if self.water.contains(&id) && !self.submitted.contains(&id) {
            return fact(None, Some("water"), "water_pass", Some("drawn by the water pass, which records no per-entity pixels: unmeasured"));
        }
        if self.hlod.contains(&id) {
            // Merged in its cell's proxy: the proxy's pixels say whether it reached the image
            // (the proxy is chosen by distance, not by view: a cell behind the camera is drawn
            // as a proxy and culled).
            let Some(owner) = self.proxy_of.get(&id) else {
                return fact(None, Some("hlod"), "streaming", Some("merged in its cell's HLOD proxy; which proxy is unknown, so whether it reached the image is unmeasured"));
            };
            return match self.ids.as_ref().map(|ids| ids.get(owner)) {
                None => fact(None, Some("hlod"), "streaming", Some("merged in its cell's HLOD proxy; the ID pass didn't run, so whether it reached the image is unmeasured")),
                Some(Some(c)) if c.pixels > 0 || c.approx || c.rasterized == Some(true) => {
                    fact(Some(true), Some("hlod"), "id_buffer", Some("drawn merged in its cell's HLOD proxy, which reached the image: its own pixels aren't counted (visible_px null)"))
                }
                Some(Some(c)) if !c.counted => fact(None, Some("hlod"), "id_buffer", Some("merged in its cell's HLOD proxy, whose blended draws weren't counted on this GPU")),
                Some(_) => fact(Some(false), Some("hlod"), "id_buffer", Some("merged in its cell's HLOD proxy, which reached no pixel: outside the view (behind the camera, culled)")),
            };
        }
        if !self.submitted.contains(&id) {
            return fact(Some(false), None, "draw_list", Some("not in the frame's draw lists (nothing to draw: not built, hidden, or no geometry)"));
        }
        let Some(ids) = &self.ids else {
            return fact(None, None, "draw_list", Some("submitted, but the ID pass didn't run (ids: false): whether it reached the image is unmeasured"));
        };
        match ids.get(&id) {
            Some(c) if !c.counted => fact(None, None, "id_buffer", Some("its blended draws weren't counted on this GPU (no fragment atomics): unknown, not 0")),
            Some(c) if c.pixels > 0 || c.approx => fact(Some(true), None, "id_buffer", None),
            Some(c) if c.rasterized == Some(true) => fact(Some(true), None, "id_buffer", Some("rasterized, but nearer surfaces hide all of it (occluded)")),
            _ => fact(Some(false), None, "id_buffer", Some("submitted, but no fragment reached the image: outside the view (behind the camera, culled) or too small")),
        }
    }
}

/// Adds what the ID buffer saw to each subject:
/// - `visible_px`: pixels it covers after occlusion (its nearest opaque pixels plus its blended
///   pixels in front of the opaque depth; glass behind glass counts, glass hides nothing);
/// - `screen_box_exact`: `[x, y, w, h]` of those pixels, null when none;
/// - `no_pixels: true` when its box is in view and it was drawn, yet no pixel shows;
/// - `occluded: true` when, besides, its fragments were rasterized: something in front hid them
///   (without it the entity was sub-pixel, or off the image after all). Unknown (left out) on
///   GPUs without fragment atomics; there, entities with blended draws (glass, blob shadows, lava
///   lamps) get `visible_px: null` and none of the other fields: their pixels weren't counted.
/// - `visible_px_approx: true` when more blended entities overlapped at some of its pixels than
///   the ID buffer's per-pixel sets hold: `visible_px` and `screen_box_exact` are then lower
///   bounds (it shows at least that much), and it is never `no_pixels`/`occluded`.
///
/// With `include_ui`, a hosted game's opaque interface counts as in front of everything
/// ([`ui_occluders`]): pixels under its panels count for no entity, and an entity entirely under
/// them is `occluded`. Sight's own debug overlays never hide anything.
///
/// Its `area` (for ranking) becomes the exact box's.
pub fn apply_visibility(subjects: &mut [Subject], entities: &[u64], coverage: &[specula_gpu::IdCoverage]) {
    // Entity → its coverage, sorted for lookups (dense ids name each entity once).
    let mut by_entity: Vec<(u64, specula_gpu::IdCoverage)> = entities.iter().zip(coverage).map(|(e, c)| (*e, *c)).collect();
    by_entity.sort_unstable_by_key(|x| x.0);
    for s in subjects {
        let drawn = by_entity.binary_search_by_key(&s.id, |x| x.0).ok().map(|i| &by_entity[i].1);
        if drawn.is_some_and(|c| !c.counted) {
            // Blended draws went uncounted (no fragment atomics): its pixels are unknown, not 0.
            s.vis = Some(Visibility::default());
            continue;
        }
        let cov = drawn.filter(|c| c.pixels > 0);
        let exact = cov.map(|c| [c.min[0], c.min[1], c.max[0] - c.min[0] + 1, c.max[1] - c.min[1] + 1]);
        // More blended ids overlapped at some of its pixels than the ID buffer's sets hold:
        // visible_px and screen_box_exact are lower bounds (it shows at least that much).
        let approx = drawn.is_some_and(|c| c.approx);
        let mut v = Visibility { px: Some(cov.map_or(0, |c| c.pixels)), exact: Some(exact), approx, no_pixels: false, occluded: false };
        if let (true, Some(c), None, false) = (s.on_screen, drawn, cov, approx) {
            v.no_pixels = true;
            v.occluded = c.rasterized == Some(true);
        }
        s.vis = Some(v);
        let exact_area = exact.map_or(0, |r| u64::from(r[2]) * u64::from(r[3]));
        // An approximate box is a lower bound (possibly empty): rank by what it may cover.
        s.area = if approx { exact_area.max(s.projected_area) } else { exact_area };
    }
}

/// What the ID buffer measured of a subject ([`apply_visibility`]): `visible_px`,
/// `screen_box_exact`, `visible_px_approx`, `no_pixels` and `occluded` in its packet entry.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Visibility {
    /// `visible_px` (`None`: null, its blended draws went uncounted).
    pub px: Option<u64>,
    /// `screen_box_exact` (`None`: absent, uncounted; `Some(None)`: null, no pixels).
    pub exact: Option<Option<[u32; 4]>>,
    pub approx: bool,
    pub no_pixels: bool,
    pub occluded: bool,
}

impl Visibility {
    fn write(&self, fields: &mut Vec<(String, Json)>) {
        fields.push(("visible_px".to_owned(), self.px.map_or(Json::Null, Json::from)));
        if let Some(exact) = self.exact {
            fields.push(("screen_box_exact".to_owned(), exact.map_or(Json::Null, |r| Json::arr(r.map(Json::from)))));
        }
        if self.approx {
            fields.push(("visible_px_approx".to_owned(), Json::from(true)));
        }
        if self.no_pixels {
            fields.push(("no_pixels".to_owned(), Json::from(true)));
            if self.occluded {
                fields.push(("occluded".to_owned(), Json::from(true)));
            }
        }
    }
}

/// How opaque a hosted game's interface must be to hide the scene under it in sight's counts
/// (`include_ui`): at this alpha or more at most a tenth of the scene shows through, which no
/// player reads as "visible". Solid panels and buttons (alpha 1, or a menu's 0.96) hide; modal
/// dimming (0.6), drop shadows and other translucent washes don't.
pub const UI_OCCLUDER_ALPHA: f32 = 0.9;

/// The pixel rectangles `[x, y, width, height]` where a hosted game's interface (`quads`, the
/// game's overlay only, never sight's debug overlays) is at least [`UI_OCCLUDER_ALPHA`] opaque,
/// for `Gpu::render_with_ids_occluded`: entities under them count no pixels there. Returns how
/// many rectangles past `specula_gpu::MAX_ID_OCCLUDERS` were left out (the smallest go first).
///
/// The rectangles never cover a pixel the interface leaves more transparent; they may miss some
/// opaque ones (under-occlusion only). Per quad, as `shaders/overlay.frag` draws it:
/// - **counted:** untextured rects whose fill (both gradient ends) is opaque enough, inset to
///   where the anti-aliased (`feather`) edge's coverage times that alpha reaches the threshold,
///   and past the border when the border is more transparent than that; a pixel counts when its
///   centre lies inside. Rounded corners: the rect's cross minus the corner squares,
///   plus the square inscribed in the rounded rect (the corners' outer slivers are left out).
///   The clip rect cuts them as the shader does (whole pixels).
/// - **left out:** textured quads (glyphs, icons, images: their alpha is per texel, unknown here),
///   triangles (vector icons and meshes), a border ring around a too-transparent fill, and
///   non-finite geometry. Quads drawn later never make a pixel more transparent (premultiplied
///   "over"), so the union is right whatever the order.
pub fn ui_occluders(quads: &[specula_gpu::OverlayQuad], width: u32, height: u32, out: &mut Vec<[u32; 4]>) -> usize {
    out.clear();
    for q in quads {
        ui_quad_occluders(q, width, height, out);
    }
    let max = specula_gpu::MAX_ID_OCCLUDERS;
    if out.len() <= max {
        return 0;
    }
    // Deterministic: the largest first (a stable sort keeps list order among equals).
    out.sort_by_key(|r| std::cmp::Reverse(u64::from(r[2]) * u64::from(r[3])));
    let dropped = out.len() - max;
    out.truncate(max);
    dropped
}

fn ui_quad_occluders(q: &specula_gpu::OverlayQuad, width: u32, height: u32, out: &mut Vec<[u32; 4]>) {
    let alpha = |c: [f32; 4]| if c[3].is_nan() { 0.0 } else { c[3].clamp(0.0, 1.0) };
    let fill = alpha(q.color).min(q.color_bottom.map_or(1.0, alpha));
    if q.texture.is_some() || q.triangle.is_some() || fill < UI_OCCLUDER_ALPHA || !q.rect.iter().all(|v| v.is_finite()) {
        return;
    }
    let num = |x: f32, lo: f32, hi: f32| if x.is_finite() { x.clamp(lo, hi) } else { lo };
    let [x, y, w, h] = q.rect;
    let (w, h) = (w.max(0.0), h.max(0.0));
    let radius = num(q.radius, 0.0, 1.0e6).min(0.5 * w.min(h));
    let border = num(q.border, 0.0, 1.0e6);
    let feather = num(q.feather, 1.0, specula_gpu::MAX_OVERLAY_FEATHER);
    // How far inside (sd <= -k, sd the shader's signed distance) a pixel centre must be for its
    // alpha to reach the threshold T. The alpha is `a * coverage` with `a` the weakest colour
    // that can show there: the edge's coverage `0.5 - sd / feather` reaches T / a at
    // sd = (0.5 - T / a) * feather. A border at least T opaque counts with the fill (alpha >=
    // min of the two times the edge's coverage); a more transparent one is left out: the fill
    // starts where the inner edge's coverage `0.5 - (sd + border)` reaches T / fill.
    let border_alpha = alpha(q.border_color[0]).min(alpha(q.border_color[1]));
    let t = UI_OCCLUDER_ALPHA;
    let k = if border > 0.0 && border_alpha < t {
        ((t / fill - 0.5) * feather).max(border + t / fill - 0.5)
    } else {
        let a = if border > 0.0 { fill.min(border_alpha) } else { fill };
        (t / a - 0.5) * feather
    };
    // The region sd <= -k: the rounded rect inset by k, corner radius r.
    let (x0, y0, x1, y1) = (x + k, y + k, x + w - k, y + h - k);
    let r = (radius - k).max(0.0);
    let d = r * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
    // The clip in whole pixels, as `build_overlay` snaps it: pixels [c0, c1).
    let clip = match q.clip {
        None => [0.0, 0.0, width as f32, height as f32],
        Some(c) if c.iter().all(|v| v.is_finite()) => [c[0].round(), c[1].round(), (c[0] + c[2]).round(), (c[1] + c[3]).round()],
        Some(_) => return,
    };
    let pieces: &[[f32; 4]] = if r > 0.0 {
        &[[x0 + r, y0, x1 - r, y1], [x0, y0 + r, x1, y1 - r], [x0 + d, y0 + d, x1 - d, y1 - d]]
    } else {
        &[[x0, y0, x1, y1]]
    };
    for p in pieces {
        // Pixel i counts when its centre i + 0.5 lies in [lo, hi] and in the clip's [c0, c1).
        let span = |lo: f32, hi: f32, c0: f32, c1: f32, size: u32| {
            let first = (lo - 0.5).ceil().max(c0).max(0.0);
            let end = ((hi - 0.5).floor() + 1.0).min(c1).min(size as f32);
            (end > first).then_some((first as u32, (end - first) as u32))
        };
        if let (Some((px, pw)), Some((py, ph))) = (span(p[0], p[2], clip[0], clip[2], width), span(p[1], p[3], clip[1], clip[3], height)) {
            out.push([px, py, pw, ph]);
        }
    }
}

/// Whether the ID buffer says a subject (its packet JSON) shows at all: `Some(true)` when it has
/// visible pixels or an approximate count (a lower bound: it showed, maybe all of it, even at 0);
/// `None` when the ID buffer didn't count it (`visible_px` null or missing). Every consumer of
/// visibility (filters, compare, overlays, totals) goes through this.
pub fn shows(j: &Json) -> Option<bool> {
    let px = j.get("visible_px")?.as_u64()?;
    Some(px > 0 || j.get("visible_px_approx").and_then(Json::as_bool) == Some(true))
}

/// The subjects to send back to the agent: everything when it fits in `max`, otherwise the
/// on-screen subjects with the largest rectangles first. Returns `(chosen, omitted_count)`.
pub fn most_relevant(subjects: &[Subject], max: usize) -> (Vec<Json>, usize) {
    if subjects.len() <= max {
        return (subjects.iter().map(Subject::full).collect(), 0);
    }
    let mut order: Vec<&Subject> = subjects.iter().filter(|s| s.on_screen).collect();
    order.sort_by_key(|s| std::cmp::Reverse(s.area));
    order.truncate(max);
    let chosen: Vec<Json> = order.iter().map(|s| s.full()).collect();
    let omitted = subjects.len() - chosen.len();
    (chosen, omitted)
}

/// The bounding box `corners` in clip space, clipped to the view depth range 0 ≤ z ≤ w: the box
/// corners in range plus every point where a box edge crosses one of those planes, each passed
/// to `point`. Those are exactly the vertices of the clipped box, so their projection bounds its
/// image. With the reversed-Z infinite projection, z ≤ w is the near plane and z ≥ 0 holds for
/// everything in front.
fn clip_points(corners: &[Vec3; 8], view_proj: &Mat4, mut point: impl FnMut(Vec4)) {
    let corners = corners.map(|p| view_proj.mul_vec4(p.extend(1.0)));
    let inside = |p: &Vec4| p.z >= 0.0 && p.z <= p.w;
    for p in corners.iter().copied().filter(inside) {
        point(p);
    }
    // An edge crosses a plane only between corners on either side of it: none when every
    // corner is on one side of both (the common case, a box wholly in front).
    let sides = |f: fn(&Vec4) -> bool| corners.iter().any(f) && !corners.iter().all(f);
    if !sides(|p| p.z < 0.0) && !sides(|p| p.w - p.z < 0.0) {
        return;
    }
    // Signed distance to the two depth-range planes (inside when ≥ 0).
    let planes: [fn(Vec4) -> f32; 2] = [|p| p.z, |p| p.w - p.z];
    for i in 0..8 {
        for bit in [1, 2, 4] {
            if i & bit != 0 {
                continue;
            }
            let (a, b) = (corners[i], corners[i | bit]);
            for plane in planes {
                let (da, db) = (plane(a), plane(b));
                if (da < 0.0) != (db < 0.0) {
                    // The planes are parallel and near < far, so a crossing of one lies inside the
                    // other; clamp only removes rounding error.
                    let mut p = lerp(a, b, da / (da - db));
                    p.z = p.z.clamp(0.0, p.w);
                    point(p);
                }
            }
        }
    }
}

fn lerp(a: Vec4, b: Vec4, t: f32) -> Vec4 {
    Vec4::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t, a.w + (b.w - a.w) * t)
}

/// Pixel rectangle of clip-space points (all in front of the near plane), clipped to the image,
/// gathered a point at a time.
#[derive(Default)]
struct Rect {
    any: bool,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Rect {
    fn add(&mut self, p: &Vec4, width: u32, height: u32) {
        let (w, h) = (width as f32, height as f32);
        if !self.any {
            self.any = true;
            (self.x0, self.y0, self.x1, self.y1) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        }
        // Vulkan viewport transform: NDC −1..1 → 0..size (clip y already points down).
        let (x, y) = ((p.x / p.w + 1.0) * 0.5 * w, (p.y / p.w + 1.0) * 0.5 * h);
        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x);
        self.y1 = self.y1.max(y);
    }

    fn finish(&self, width: u32, height: u32) -> Option<[u32; 4]> {
        if !self.any {
            return None;
        }
        let (w, h) = (width as f32, height as f32);
        let (cx0, cy0, cx1, cy1) = (self.x0.max(0.0), self.y0.max(0.0), self.x1.min(w), self.y1.min(h));
        if !(cx0 < cx1 && cy0 < cy1) {
            return None;
        }
        let (px, py) = (cx0.floor() as u32, cy0.floor() as u32);
        Some([px, py, (cx1.ceil() as u32).saturating_sub(px).max(1), (cy1.ceil() as u32).saturating_sub(py).max(1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{EntityKey, SpawnSpec};

    #[test]
    fn camera_rays_hit_what_the_pixel_shows() {
        let cam = Camera::new(Vec3::new(3.0, 8.0, 10.0), Vec3::new(1.0, 0.0, -2.0), 50.0);
        let (w, h) = (1280u32, 720u32);
        let vp = cam.view_proj(w, h);
        for p in [Vec3::new(1.0, 0.0, -2.0), Vec3::new(4.0, 1.0, 0.0), Vec3::new(-3.0, 0.5, -6.0)] {
            // Project p to a pixel, cast a ray back through it, and check it passes through p.
            let (sx, sy, _) = specula_core::math::project_to_screen(&vp, p, w, h).unwrap();
            // `ray` takes pixel indices (it aims at the pixel centre).
            let (o, d) = cam.ray(sx - 0.5, sy - 0.5, w, h);
            let to_p = p - o;
            let miss = (to_p - d * to_p.dot(d)).length();
            assert!(miss < 1e-3 * to_p.length(), "{p:?} missed by {miss}");
            assert!((d.length() - 1.0).abs() < 1e-5);
        }
        let down = Camera::new(Vec3::new(0.0, 10.0, 0.0), Vec3::ZERO, 60.0);
        let (_, d) = down.ray(639.5, 359.5, w, h);
        assert!(d.y < -0.999, "straight down still works: {d:?}");
    }

    /// The M3 carry-over (M4 final audit repro): a 3 x 3 grid of unit cubes and one cube at
    /// x = 2000. Auto-framing fits the grid and names the far cube; the order of the boxes
    /// changes nothing; a second cluster of the same size, a ground plane under everything and
    /// a pair of boxes are all kept.
    #[test]
    fn auto_framing_leaves_out_outliers_and_says_so() {
        let cube = |id: u64, x: f32, z: f32| (id, Vec3::new(x - 0.5, 0.0, z - 0.5), Vec3::new(x + 0.5, 1.0, z + 0.5));
        let mut boxes: Vec<_> = (0..9).map(|i| cube(i + 1, (i % 3) as f32 * 1.2, (i / 3) as f32 * 1.2)).collect();
        let fr = |b: &[(u64, Vec3, Vec3)]| framing(b, 360, 50.0);
        let grid = fr(&boxes);
        assert!(grid.excluded.is_empty());
        boxes.push(cube(10, 2000.0, 0.0));
        let f = fr(&boxes);
        assert_eq!(f.excluded, vec![10]);
        assert_eq!(f.bounds, grid.bounds, "the grid alone is framed");
        boxes.reverse();
        assert_eq!(fr(&boxes), f, "order-free");
        // Two clusters of a similar size: neither is an outlier.
        let two: Vec<_> = (0..12).map(|i| cube(i + 1, if i < 6 { i as f32 } else { 500.0 + i as f32 }, 0.0)).collect();
        assert!(fr(&two).excluded.is_empty());
        // A ground plane under everything holds the centre (distance 0) and is kept.
        let mut ground = boxes.clone();
        ground.push((11, Vec3::new(-50.0, -0.1, -50.0), Vec3::new(50.0, 0.0, 50.0)));
        assert_eq!(fr(&ground).excluded, vec![10]);
        // The M4 final review: small scenes are framed whole. Two cubes and a car 12 m away (the
        // p75 of three is the median, so the core was 0 and the car "far")...
        assert!(fr(&[cube(1, 0.0, 0.0), cube(2, 1.2, 0.0), cube(3, 12.0, 0.0)]).excluded.is_empty());
        // ... and 4 tight cubes with 2 more 15 m away (2 of 6 left out, a third).
        let six: Vec<_> = (0..4).map(|i| cube(i + 1, (i % 2) as f32 * 1.1, (i / 2) as f32 * 1.1)).chain([cube(5, 15.0, 0.0), cube(6, 0.0, 15.0)]).collect();
        assert!(fr(&six).excluded.is_empty());
        // At most a quarter: 8 tight cubes and 3 far ones leave out 2 of 11 (the farthest).
        let mut many: Vec<_> = (0..8).map(|i| cube(i + 1, (i % 4) as f32 * 1.1, (i / 4) as f32 * 1.1)).collect();
        many.extend([cube(20, 3000.0, 0.0), cube(21, 2500.0, 0.0), cube(22, 0.0, 2000.0)]);
        assert_eq!(fr(&many).excluded, vec![20, 21]);
        // A third far (4 of 12): the 75th percentile is itself far, so nothing is an outlier.
        many.push(cube(23, 0.0, 1500.0));
        assert!(fr(&many).excluded.is_empty());
        // A far box that would still leave the rest readable is kept: 8 cubes and one 20 m away.
        let mut near: Vec<_> = (0..8).map(|i| cube(i + 1, (i % 4) as f32 * 1.1, (i / 4) as f32 * 1.1)).collect();
        near.push(cube(9, 20.0, 0.0));
        assert!(fr(&near).excluded.is_empty());
        // Fewer than eight boxes: nothing to call an outlier.
        assert!(fr(&[cube(1, 0.0, 0.0), cube(2, 2000.0, 0.0)]).excluded.is_empty());
        assert_eq!(fr(&[]), Framing::default());
        // In a world: the packet names it, and an explicit camera is untouched (capture.rs only
        // frames when no camera is given).
        let mut world = World::default();
        for (i, x) in [0.0, 1.2, 2.4, 0.0, 1.2, 2.4, 0.0, 1.2, 2.4, 2000.0].into_iter().enumerate() {
            spawn_scaled(&mut world, Shape::Cube, &format!("c{i}"), Vec3::new(x, 0.5, (i / 3) as f32 * 1.2), Vec3::ONE);
        }
        let f = framing(&world_boxes(&world), 360, 50.0);
        let j = f.to_json(&world).expect("named");
        assert_eq!(j.get("excluded_count").and_then(Json::as_u64), Some(1));
        assert_eq!(j.get("reason").and_then(Json::as_str), Some("outlier"));
        assert_eq!(j.get("excluded").and_then(Json::as_array).and_then(|a| a[0].get("path")).and_then(Json::as_str), Some("/c9"));
        let (lo, hi) = f.bounds.unwrap();
        assert!(hi.x < 4.0 && lo.x > -1.0, "the grid's bounds: {lo:?} {hi:?}");
    }

    fn spawn_scaled(world: &mut World, shape: Shape, name: &str, position: Vec3, scale: Vec3) {
        let mut spec = SpawnSpec::new(shape);
        spec.name = Some(name.into());
        spec.position = position;
        spec.scale = scale;
        spec.color = [1.0, 0.0, 0.0];
        world.spawn(spec, "test").unwrap();
    }

    /// The window's cached draw list is draw_list's, draw for draw and in order, through every
    /// kind of change (moves, recolours, hiding, ui layer, spawns, deletes, transient moves,
    /// a replaced world), and keeps its fixed draws while nothing changes.
    #[test]
    fn cached_draw_list_matches_draw_list() {
        use crate::meshes::{LatheMeshes, ShapeMeshes};
        use crate::models::{GpuModels, LodChoice};
        use crate::world::{Layer, ModelSpec, UpdateSpec};
        use specula_gpu::MeshId;
        let shapes = ShapeMeshes::from_ids(MeshId(1), MeshId(2), MeshId(3));
        let (lathes, models) = (LatheMeshes::default(), GpuModels::default());
        let meshes = Meshes { shapes: &shapes, lathes: &lathes, models: &models };
        let camera = cam(Vec3::new(0.0, 5.0, 10.0), Vec3::ZERO, 50.0);
        let mut cache = DrawCache::default();
        let check = |world: &World, generation: u64, cache: &mut DrawCache| {
            let (mut a, mut b) = (Vec::new(), Vec::new());
            let mut view = View { camera, viewport_h: 720, include_ui: true, lod: LodChoice::Fresh };
            let sa = draw_list(world, &meshes, &mut view, &mut a, None);
            let sb = cache.draw_list(world, generation, &meshes, &mut view, &mut b);
            assert_eq!(a, b);
            assert_eq!(sa, sb);
            a.len()
        };
        let mut world = World::default();
        for i in 0..40 {
            let shape = [Shape::Cube, Shape::Sphere, Shape::Quad, Shape::Group][i % 4];
            spawn(&mut world, shape, &format!("e{i}"), Vec3::new(i as f32, 0.0, 0.0));
        }
        world.spawn(SpawnSpec::model(ModelSpec::new("recipes/none.json")), "t").unwrap();
        assert_eq!(check(&world, 0, &mut cache), 30);
        let epoch = world.draw_epoch();
        assert_eq!(check(&world, 0, &mut cache), 30);
        assert_eq!(world.draw_epoch(), epoch, "drawing changes nothing");
        let set = |world: &mut World, path: &str, u: UpdateSpec| {
            world.update(&EntityKey::Path(path.into()), u, "t").unwrap();
        };
        set(&mut world, "/e1", UpdateSpec { position: Some(Vec3::new(0.0, 3.0, 0.0)), ..Default::default() });
        check(&world, 0, &mut cache);
        set(&mut world, "/e2", UpdateSpec { color: Some([1.0, 0.0, 0.0]), ..Default::default() });
        check(&world, 0, &mut cache);
        set(&mut world, "/e5", UpdateSpec { visible: Some(false), ..Default::default() });
        assert_eq!(check(&world, 0, &mut cache), 29);
        set(&mut world, "/e6", UpdateSpec { layer: Some(Layer::Ui), ..Default::default() });
        check(&world, 0, &mut cache);
        world.delete(&EntityKey::Path("/e9".into()), None).unwrap();
        assert_eq!(check(&world, 0, &mut cache), 28);
        let mut t = SpawnSpec::new(Shape::Cube);
        t.name = Some("enemy".into());
        t.transient = true;
        let enemy = world.spawn(t, "game").unwrap().id;
        assert_eq!(check(&world, 0, &mut cache), 29);
        world.update(&EntityKey::Id(enemy), UpdateSpec { position: Some(Vec3::new(5.0, 0.0, 5.0)), ..Default::default() }, "game").unwrap();
        check(&world, 0, &mut cache);
        assert_eq!(cache.rebuilds(), 1, "every change above was patched in, not rebuilt");
        assert!(cache.patches() >= 5);
        // Moving a group moves its subtree: every child is patched.
        let mut g = SpawnSpec::new(Shape::Group);
        g.name = Some("g".into());
        world.spawn(g, "t").unwrap();
        for i in 0..5 {
            let mut c = SpawnSpec::new(Shape::Cube);
            c.name = Some(format!("k{i}"));
            c.parent = Some(EntityKey::Path("/g".into()));
            world.spawn(c, "t").unwrap();
        }
        check(&world, 0, &mut cache);
        set(&mut world, "/g", UpdateSpec { position: Some(Vec3::new(0.0, 0.0, -4.0)), ..Default::default() });
        set(&mut world, "/g", UpdateSpec { visible: Some(false), ..Default::default() });
        assert_eq!(check(&world, 0, &mut cache), 29);
        // Too many changes at once: one rebuild instead of patching each.
        for i in 0..400 {
            set(&mut world, &format!("/e{}", i % 40 + if i % 40 == 9 { 1 } else { 0 }), UpdateSpec { color: Some([0.5, 0.5, 0.5]), ..Default::default() });
        }
        check(&world, 0, &mut cache);
        assert_eq!(cache.rebuilds(), 2);
        // A different world that happens to have the same epoch: the generation tells them apart.
        let mut other = World::default();
        spawn(&mut other, Shape::Cube, "x", Vec3::ZERO);
        while other.draw_epoch() < world.draw_epoch() {
            set(&mut other, "/x", UpdateSpec { position: Some(Vec3::ZERO), ..Default::default() });
        }
        assert_eq!(check(&other, 1, &mut cache), 1);
    }

    /// Lights come from the lit-entity index in id order: hidden ones are left out, at most
    /// MAX_POINT_LIGHTS are kept, the rest counted, and the buffer is reused.
    #[test]
    fn lights_are_gathered_in_id_order_up_to_the_limit() {
        use crate::world::{LightSpec, UpdateSpec};
        let mut world = World::default();
        for i in 0..1000 {
            spawn(&mut world, Shape::Cube, &format!("c{i}"), Vec3::ZERO);
        }
        for i in 0..20 {
            let mut s = SpawnSpec::new(Shape::Sphere);
            s.name = Some(format!("lamp{i:02}"));
            s.position = Vec3::new(i as f32, 1.0, 0.0);
            s.light = Some(LightSpec { intensity: 1.0, range: 2.0, color: None, offset: Vec3::ZERO });
            world.spawn(s, "t").unwrap();
        }
        let hide = UpdateSpec { visible: Some(false), ..Default::default() };
        world.update(&EntityKey::Path("/lamp00".into()), hide, "t").unwrap();
        let mut out = Vec::with_capacity(MAX_POINT_LIGHTS);
        let dropped = lights(&world, false, &mut out);
        assert_eq!((out.len(), dropped), (MAX_POINT_LIGHTS, 19 - MAX_POINT_LIGHTS));
        let xs: Vec<f32> = out.iter().map(|l| l.position[0]).collect();
        assert_eq!(xs, (1..=MAX_POINT_LIGHTS).map(|i| i as f32).collect::<Vec<_>>(), "id order, hidden lamp left out");
        let ptr = out.as_ptr();
        lights(&world, false, &mut out);
        assert_eq!(out.as_ptr(), ptr, "the buffer is reused");
        assert_eq!(world.lit_entities().count(), 20, "only lamps are in the index, not the 1000 cubes");
    }

    fn spawn(world: &mut World, shape: Shape, name: &str, position: Vec3) {
        spawn_scaled(world, shape, name, position, Vec3::ONE);
    }

    fn rect(subject: &Subject) -> [u64; 4] {
        subject.screen.expect("on screen").map(u64::from)
    }

    fn cam(eye: Vec3, target: Vec3, fov_y_deg: f32) -> Camera {
        Camera::new(eye, target, fov_y_deg)
    }

    fn frame(world: &World, cam: Camera, w: u32, h: u32) -> Vec<Subject> {
        subjects(world, &cam.view_proj(w, h), w, h, false)
    }

    #[test]
    fn auto_camera_frames_everything() {
        let mut world = World::default();
        spawn(&mut world, Shape::Cube, "left", Vec3::new(-3.0, 0.0, 0.0));
        spawn(&mut world, Shape::Sphere, "right", Vec3::new(3.0, 1.0, -2.0));
        spawn(&mut world, Shape::Quad, "floor", Vec3::new(0.0, -0.5, 0.0));
        for s in frame(&world, auto_camera(&world, 50.0), 960, 540) {
            let [x, y, w, h] = rect(&s);
            assert!(x + w <= 960 && y + h <= 540, "{}", s.full());
            assert!(w > 4 && h > 0, "{}", s.full());
        }
    }

    #[test]
    fn subject_rects_follow_world_layout() {
        let mut world = World::default();
        spawn(&mut world, Shape::Cube, "low", Vec3::new(0.0, 0.0, 0.0));
        spawn(&mut world, Shape::Cube, "high", Vec3::new(0.0, 3.0, 0.0));
        spawn(&mut world, Shape::Cube, "east", Vec3::new(3.0, 0.0, 0.0));
        let cam = cam(Vec3::new(1.5, 1.5, 12.0), Vec3::new(1.5, 1.5, 0.0), 50.0);
        let s = frame(&world, cam, 800, 600);
        let (low, high, east) = (rect(&s[0]), rect(&s[1]), rect(&s[2]));
        assert!(high[1] < low[1], "+Y is up on screen: high {high:?} low {low:?}");
        assert!(east[0] > low[0], "+X is right on screen: east {east:?} low {low:?}");
        assert_eq!((low[2], low[3]), (high[2], high[3]));
    }

    #[test]
    fn entities_behind_the_camera_are_off_screen() {
        let mut world = World::default();
        spawn(&mut world, Shape::Cube, "behind", Vec3::new(0.0, 0.0, 20.0));
        let cam = cam(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, 50.0);
        let s = frame(&world, cam, 640, 480);
        assert_eq!(s[0].full().get("on_screen").and_then(Json::as_bool), Some(false));
        assert!(s[0].full().get("screen").unwrap().is_null());
        assert!(s[0].full().get("distance_m").unwrap().is_null());
    }

    /// Reviewer finding: a floor under a first-person camera has corners behind the camera.
    #[test]
    fn floor_under_first_person_camera_is_on_screen() {
        let mut world = World::default();
        spawn_scaled(&mut world, Shape::Quad, "floor", Vec3::ZERO, Vec3::new(10.0, 1.0, 10.0));
        let cam = cam(Vec3::new(0.0, 1.6, 4.0), Vec3::new(0.0, 1.6, 0.0), 60.0);
        let s = frame(&world, cam, 640, 360);
        let [x, y, w, h] = rect(&s[0]);
        // The floor fills the full width of the lower part of the image.
        assert_eq!((x, x + w), (0, 640), "{}", s[0].full());
        assert!(y >= 180 && y + h == 360, "{}", s[0].full());
    }

    /// Reviewer findings: worlds over ~1 km fell beyond a fixed 2 km far plane; then a far-away
    /// "sun" pushed a fitted near plane out to 10 m and the hero cube vanished.
    #[test]
    fn huge_and_distant_worlds_stay_visible() {
        let mut world = World::default();
        spawn(&mut world, Shape::Cube, "west", Vec3::new(-1000.0, 0.0, 0.0));
        spawn(&mut world, Shape::Cube, "east", Vec3::new(1000.0, 0.0, 0.0));
        spawn_scaled(&mut world, Shape::Quad, "ground", Vec3::ZERO, Vec3::new(5000.0, 1.0, 5000.0));
        let vp = auto_camera(&world, 50.0).view_proj(800, 450);
        for e in world.entities() {
            for c in e.world_corners().unwrap() {
                let p = vp.mul_vec4(c.extend(1.0));
                assert!(p.w > 0.0 && p.z >= 0.0 && p.z <= p.w, "{} corner {c:?} outside the depth range", e.name);
            }
        }

        let mut world = World::default();
        spawn(&mut world, Shape::Cube, "hero", Vec3::ZERO);
        spawn(&mut world, Shape::Sphere, "sun", Vec3::new(0.0, 0.0, -1.0e6));
        let s = frame(&world, cam(Vec3::new(0.0, 1.7, 5.0), Vec3::new(0.0, 1.0, 0.0), 50.0), 640, 360);
        let hero = rect(&s[0]);
        assert!(hero[2] > 50 && hero[3] > 50, "hero cube must be clearly visible: {hero:?}");
        assert_eq!(s[1].full().get("on_screen").and_then(Json::as_bool), Some(true), "the sun is in view too");
    }

    #[test]
    fn top_down_camera_sees_the_scene() {
        let mut world = World::default();
        spawn(&mut world, Shape::Cube, "centre", Vec3::ZERO);
        spawn(&mut world, Shape::Cube, "north", Vec3::new(0.0, 0.0, -3.0));
        spawn(&mut world, Shape::Cube, "east", Vec3::new(3.0, 0.0, 0.0));
        let cam = cam(Vec3::new(0.0, 15.0, 0.0), Vec3::ZERO, 50.0);
        let s = frame(&world, cam, 600, 600);
        let (centre, north, east) = (rect(&s[0]), rect(&s[1]), rect(&s[2]));
        assert!(north[1] < centre[1], "north is up");
        assert!(east[0] > centre[0], "east is right");
    }

    #[test]
    fn camera_inside_a_box_and_distance_in_metres() {
        let mut world = World::default();
        spawn_scaled(&mut world, Shape::Cube, "room", Vec3::ZERO, Vec3::new(10.0, 4.0, 10.0));
        spawn(&mut world, Shape::Cube, "crate", Vec3::new(0.0, 0.0, -4.5));
        let cam = cam(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0), 60.0);
        let s = frame(&world, cam, 640, 480);
        assert_eq!(rect(&s[0]), [0, 0, 640, 480], "room surrounds the camera");
        let d = s[1].full().get("distance_m").and_then(Json::as_f64).unwrap();
        assert!((d - 4.0).abs() < 1e-3, "crate's near face is 4 m ahead, got {d}");
    }

    #[test]
    fn subjects_use_world_transforms_and_skip_groups() {
        let mut world = World::default();
        let mut group = SpawnSpec::new(Shape::Group);
        group.name = Some("tower".into());
        group.position = Vec3::new(3.0, 0.0, 0.0);
        world.spawn(group, "test").unwrap();
        let mut top = SpawnSpec::new(Shape::Cube);
        top.name = Some("top".into());
        top.parent = Some(EntityKey::Path("/tower".into()));
        top.position = Vec3::new(0.0, 2.0, 0.0);
        world.spawn(top, "test").unwrap();
        spawn(&mut world, Shape::Cube, "loose", Vec3::new(3.0, 2.0, 0.0));
        let s = frame(&world, cam(Vec3::new(0.0, 2.0, 10.0), Vec3::new(0.0, 2.0, 0.0), 50.0), 640, 480);
        assert_eq!(s.len(), 2, "the group has no subject");
        assert_eq!(s[0].full().get("path").and_then(Json::as_str), Some("/tower/top"));
        assert_eq!(s[0].full().get("position"), Some(&vec3_json(Vec3::new(3.0, 2.0, 0.0))));
        assert_eq!(rect(&s[0]), rect(&s[1]), "a child lands where the same box at its world position would");
    }

    #[test]
    fn hidden_and_ui_entities_are_not_subjects_unless_asked() {
        use crate::world::{Layer, UpdateSpec};
        let mut world = World::default();
        spawn(&mut world, Shape::Cube, "stone", Vec3::ZERO);
        let mut cursor = SpawnSpec::new(Shape::Cube);
        cursor.name = Some("cursor".into());
        cursor.layer = Layer::Ui;
        world.spawn(cursor, "game").unwrap();
        spawn(&mut world, Shape::Sphere, "ghost", Vec3::new(1.0, 0.0, 0.0));
        let hide = UpdateSpec { visible: Some(false), ..Default::default() };
        world.update(&EntityKey::Path("/ghost".into()), hide, "t").unwrap();
        let cam = cam(Vec3::new(0.0, 2.0, 8.0), Vec3::ZERO, 50.0);
        let names = |include_ui: bool| -> Vec<String> {
            subjects(&world, &cam.view_proj(640, 480), 640, 480, include_ui)
                .iter()
                .map(|s| s.full().get("name").and_then(Json::as_str).unwrap().to_owned())
                .collect()
        };
        assert_eq!(names(false), ["stone"]);
        assert_eq!(names(true), ["stone", "cursor"], "hidden stays hidden even with include_ui");
        let ui = subjects(&world, &cam.view_proj(640, 480), 640, 480, true);
        assert_eq!(ui[1].full().get("layer"), Some(&Json::from("ui")));
        assert_eq!((lights(&world, false, &mut Vec::new()), world.lit_entities().count()), (0, 0));
        assert!(!in_frame(world.get(2).unwrap(), false) && in_frame(world.get(2).unwrap(), true));
    }

    #[test]
    fn id_buffer_results_go_into_subjects() {
        use specula_gpu::IdCoverage;
        // A model's parts share its id; ids are dense in draw order.
        let (ids, entities) = dense_ids(&[7, 7, 3, 9, 3]);
        assert_eq!((ids, entities.clone()), (vec![1, 1, 2, 3, 2], vec![7, 3, 9]));
        // The in-order fast path gives what a plain map gives, in and out of order.
        let naive = |owners: &[u64]| {
            let mut ents: Vec<u64> = Vec::new();
            let ids: Vec<u32> = owners
                .iter()
                .map(|&e| if e == 0 { 0 } else { ents.iter().position(|&x| x == e).map_or_else(|| { ents.push(e); ents.len() as u32 }, |i| i as u32 + 1) })
                .collect();
            (ids, ents)
        };
        for owners in [vec![1u64, 2, 2, 5, 9, 9, 9, 12], vec![4, 0, 4, 8, 2, 8, 0, 3, 9, 4, 2, 10, 1], vec![3, 5, 5, 1, 5, 7, 3, 1, 20, 6], vec![]] {
            assert_eq!(dense_ids(&owners), naive(&owners), "{owners:?}");
        }
        let mut world = World::default();
        spawn(&mut world, Shape::Cube, "front", Vec3::ZERO);
        spawn(&mut world, Shape::Cube, "behind", Vec3::new(0.0, 0.0, -3.0));
        spawn(&mut world, Shape::Cube, "away", Vec3::new(0.0, 0.0, 40.0));
        let cam = cam(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, 50.0);
        spawn(&mut world, Shape::Cube, "speck", Vec3::new(1.0, 0.0, 0.0));
        let mut s = frame(&world, cam, 640, 480);
        // Drawn: front (visible), behind (rasterized, hidden) and speck (never rasterized);
        // `away` was culled (never drawn).
        let seen = IdCoverage {
            pixels: 1200,
            opaque_pixels: 1000,
            blended_pixels: 200,
            min: [300, 220],
            max: [339, 259],
            rasterized: Some(true),
            counted: true,
            approx: true,
        };
        let hidden = IdCoverage { rasterized: Some(true), ..IdCoverage::default() };
        let projected = s[0].area;
        apply_visibility(&mut s, &[1, 2, 4], &[seen, hidden, IdCoverage::default()]);
        assert_eq!(s[0].projected_area, projected, "overlays keep ranking by the projected box");
        let get = |i: usize, k: &str| s[i].full().get(k).cloned();
        assert_eq!(get(0, "visible_px"), Some(Json::from(1200)));
        assert_eq!(get(0, "visible_px_approx"), Some(Json::from(true)));
        assert_eq!(get(1, "visible_px_approx"), None);
        assert_eq!(get(0, "screen_box_exact"), Some(Json::arr([300, 220, 40, 40].map(Json::from))));
        // Approximate: ranked by the larger of its (lower-bound) exact box and its projected box.
        assert_eq!((get(0, "no_pixels"), get(0, "occluded"), s[0].area), (None, None, projected.max(1600)));
        assert_eq!((get(1, "visible_px"), get(1, "no_pixels"), get(1, "occluded")), (Some(Json::from(0)), Some(Json::from(true)), Some(Json::from(true))));
        assert_eq!((get(1, "screen_box_exact"), s[1].area), (Some(Json::Null), 0));
        assert_eq!(get(2, "no_pixels"), None, "behind the camera: not in view at all");
        assert_eq!((get(3, "no_pixels"), get(3, "occluded")), (Some(Json::from(true)), None), "nothing hid it: it rasterized nothing");
        // Uncounted (glass without fragment atomics): unknown, not "no pixels".
        let mut s = frame(&world, cam, 640, 480);
        let uncounted = IdCoverage { counted: false, rasterized: None, ..IdCoverage::default() };
        apply_visibility(&mut s, &[1], &[uncounted]);
        assert_eq!(s[0].full().get("visible_px"), Some(&Json::Null));
        assert!(s[0].full().get("no_pixels").is_none() && s[0].full().get("screen_box_exact").is_none() && s[0].full().get("occluded").is_none());
        assert_eq!(s[0].area, s[0].projected_area);
        // Approximate with nothing counted (pushed out of every set it was drawn in): at least 0,
        // but it did show, so neither no_pixels nor occluded.
        let mut s = frame(&world, cam, 640, 480);
        let pushed_out = IdCoverage { approx: true, rasterized: Some(true), ..IdCoverage::default() };
        apply_visibility(&mut s, &[1], &[pushed_out]);
        assert_eq!((s[0].full().get("visible_px"), s[0].full().get("visible_px_approx")), (Some(&Json::from(0)), Some(&Json::from(true))));
        assert!(s[0].full().get("no_pixels").is_none() && s[0].full().get("occluded").is_none());
    }

    /// `shaders/overlay.frag`'s alpha for an untextured quad at a pixel centre (a replica, to check
    /// the occluders against).
    fn overlay_alpha(q: &specula_gpu::OverlayQuad, px: f32, py: f32) -> f32 {
        let clip = q.clip.map_or([-1.0e9, -1.0e9, 1.0e9, 1.0e9], |c| [c[0].round(), c[1].round(), (c[0] + c[2]).round(), (c[1] + c[3]).round()]);
        if px < clip[0] || py < clip[1] || px >= clip[2] || py >= clip[3] {
            return 0.0;
        }
        let [x, y, w, h] = q.rect;
        let (w, h) = (w.max(0.0), h.max(0.0));
        let (lx, ly) = (px - x, py - y);
        let (hx, hy) = (0.5 * w, 0.5 * h);
        let r = q.radius.clamp(0.0, hx.min(hy));
        let (qx, qy) = ((lx - hx).abs() - (hx - r), (ly - hy).abs() - (hy - r));
        let sd = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
        let outer = (0.5 - sd / q.feather.clamp(1.0, specula_gpu::MAX_OVERLAY_FEATHER)).clamp(0.0, 1.0);
        let t = (ly / h.max(1.0e-6)).clamp(0.0, 1.0);
        let mix = |a: f32, b: f32| a + (b - a) * t;
        let fill = mix(q.color[3], q.color_bottom.map_or(q.color[3], |c| c[3])).clamp(0.0, 1.0);
        if q.border > 0.0 {
            let inner = (0.5 - (sd + q.border)).clamp(0.0, 1.0).min(outer);
            let b = mix(q.border_color[0][3], q.border_color[1][3]).clamp(0.0, 1.0);
            fill * inner + b * (outer - inner)
        } else {
            fill * outer
        }
    }

    #[test]
    fn ui_occluders_cover_only_opaque_interface_pixels() {
        use specula_gpu::{OverlayKind, OverlayQuad, OverlayTriangle, TextureId};
        let (w, h) = (160u32, 120u32);
        let solid = |rect: [f32; 4], a: f32| OverlayQuad::solid(rect, [0.2, 0.22, 0.25, a]);
        let mut out = Vec::new();
        // A square opaque panel: exactly its pixels.
        assert_eq!(ui_occluders(&[solid([10.0, 8.0, 20.0, 12.0], 1.0)], w, h, &mut out), 0);
        assert_eq!(out, [[10, 8, 20, 12]]);
        // Never: translucent washes, textured quads (glyphs), triangles, a soft shadow.
        let never = [
            solid([0.0, 0.0, 160.0, 120.0], 0.6),
            OverlayQuad::textured([0.0, 0.0, 50.0, 50.0], [0.0, 0.0, 1.0, 1.0], TextureId(1), OverlayKind::AlphaMask, [1.0; 4]),
            OverlayQuad { triangle: Some(OverlayTriangle { points: [[0.0, 0.0], [90.0, 0.0], [0.0, 90.0]], colors: [[1.0; 4]; 3] }), ..OverlayQuad::default() },
            OverlayQuad { feather: 9.0, ..solid([20.0, 20.0, 60.0, 30.0], 0.63) },
            OverlayQuad { color_bottom: Some([0.0, 0.0, 0.0, 0.5]), ..solid([0.0, 0.0, 40.0, 40.0], 1.0) },
            solid([f32::NAN, 0.0, 40.0, 40.0], 1.0),
        ];
        assert_eq!(ui_occluders(&never, w, h, &mut out), 0);
        assert!(out.is_empty(), "{out:?}");
        // Shapes as a HUD draws them: rounded, bordered, feathered, gradients, clipped, off the
        // edge. Every occluded pixel is opaque enough; few opaque pixels are missed.
        let quads = [
            OverlayQuad { radius: 6.0, border: 1.0, border_color: [[0.3, 0.35, 0.4, 1.0], [0.08, 0.09, 0.11, 1.0]], ..solid([10.0, 8.0, 40.5, 30.25], 1.0) },
            OverlayQuad { radius: 10.0, border: 2.0, border_color: [[0.4, 0.4, 0.4, 0.3]; 2], ..solid([60.0, 10.0, 50.0, 40.0], 0.96) },
            OverlayQuad { feather: 6.0, color_bottom: Some([0.1, 0.1, 0.1, 0.92]), ..solid([5.0, 60.0, 70.0, 40.0], 1.0) },
            OverlayQuad { radius: 4.0, clip: Some([100.0, 70.0, 30.4, 20.6]), ..solid([90.0, 60.0, 60.0, 50.0], 1.0) },
            OverlayQuad { radius: 30.0, ..solid([130.0, 90.0, 60.0, 60.0], 1.0) },
        ];
        assert_eq!(ui_occluders(&quads, w, h, &mut out), 0);
        let inside = |x: u32, y: u32| out.iter().any(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]);
        let (mut opaque, mut missed) = (0, 0);
        for y in 0..h {
            for x in 0..w {
                // "Over", in list order.
                let a = quads.iter().fold(0.0f32, |below, q| {
                    let top = overlay_alpha(q, x as f32 + 0.5, y as f32 + 0.5);
                    top + below * (1.0 - top)
                });
                if inside(x, y) {
                    assert!(a >= UI_OCCLUDER_ALPHA - 1.0e-4, "pixel ({x}, {y}) is occluded at alpha {a}: {out:?}");
                }
                if a >= UI_OCCLUDER_ALPHA {
                    opaque += 1;
                    missed += usize::from(!inside(x, y));
                }
            }
        }
        eprintln!("ui occluders: {} rects; {missed} of {opaque} opaque pixels left visible", out.len());
        assert!(opaque > 5000 && missed * 10 < opaque, "{missed} of {opaque} opaque pixels left visible");
        // Sepulcra's inspect panel as the HUD drew it (logged live at 1280x720; play_ui reports
        // its rect rounded, [1013, 270, 257, 179]): the fill reaches x = 1269.2 and y = 449.6, so
        // column 1269 and row 449 are its anti-aliased edge (coverage 0.2 and 0.6), not occluders;
        // the edges are exact on all four sides.
        let panel = OverlayQuad {
            radius: 5.4,
            border: 0.9,
            color_bottom: Some([0.133, 0.149, 0.176, 1.0]),
            border_color: [[0.31, 0.345, 0.4, 1.0], [0.078, 0.09, 0.11, 1.0]],
            ..solid([1012.7, 270.4, 256.5, 179.2], 1.0)
        };
        let (pw, ph) = (1280, 720);
        ui_occluders(&[panel], pw, ph, &mut out);
        let covered = |x: u32, y: u32| out.iter().any(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]);
        let alpha = |x: u32, y: u32| overlay_alpha(&panel, x as f32 + 0.5, y as f32 + 0.5);
        let (x0, x1) = (out.iter().map(|r| r[0]).min().unwrap(), out.iter().map(|r| r[0] + r[2] - 1).max().unwrap());
        let (y0, y1) = (out.iter().map(|r| r[1]).min().unwrap(), out.iter().map(|r| r[1] + r[3] - 1).max().unwrap());
        assert_eq!((x0, x1, y0, y1), (1013, 1268, 271, 448), "{out:?}");
        for (x, y) in [(1012, 360), (1269, 360), (1140, 270), (1140, 449)] {
            assert!(!covered(x, y) && alpha(x, y) < UI_OCCLUDER_ALPHA, "({x}, {y}): alpha {}", alpha(x, y));
        }
        for (x, y) in [(1013, 360), (1268, 360), (1140, 271), (1140, 448)] {
            assert!(covered(x, y) && alpha(x, y) >= UI_OCCLUDER_ALPHA, "({x}, {y}): alpha {}", alpha(x, y));
        }
        // Past the GPU's limit: the largest are kept.
        let many: Vec<_> = (0..specula_gpu::MAX_ID_OCCLUDERS + 3).map(|i| solid([0.0, 0.0, if i % 1000 == 1 && i < 3000 { 1.0 } else { 2.0 }, 1.0], 1.0)).collect();
        assert_eq!(ui_occluders(&many, w, h, &mut out), 3);
        assert_eq!(out.len(), specula_gpu::MAX_ID_OCCLUDERS);
        assert!(out.iter().all(|r| r[2] > 1), "the 1 px ones went first");
    }

    /// From the interface's quads to the subjects, with the ID buffer simulated on the CPU: the
    /// GPU's occluder clear leaves the opaque ids 0 inside each rectangle (exactly what
    /// `vkCmdClearAttachments` does, checked on real GPUs in specula-gpu's idbuffer tests) and its
    /// raster step still marks everything drawn as rasterized.
    #[test]
    fn opaque_ui_panels_hide_what_is_under_them() {
        use specula_gpu::{IdImage, OverlayQuad};
        let mut world = World::default();
        spawn(&mut world, Shape::Cube, "under", Vec3::new(-2.0, 0.0, 0.0));
        spawn(&mut world, Shape::Cube, "half", Vec3::new(0.0, 0.0, 0.0));
        spawn(&mut world, Shape::Cube, "clear", Vec3::new(2.0, 0.0, 0.0));
        let (w, h) = (64u32, 48u32);
        let cam = cam(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, 50.0);
        // Synthetic opaque ids: "under" in [12, 20) x [10, 18), "half" in [25, 40) x [10, 20),
        // "clear" in [45, 55) x [30, 40).
        let mut ids = vec![0u32; (w * h) as usize];
        for (id, x0, y0, x1, y1) in [(1u32, 12u32, 10u32, 20u32, 18u32), (2, 25, 10, 40, 20), (3, 45, 30, 55, 40)] {
            for y in y0..y1 {
                for x in x0..x1 {
                    ids[(y * w + x) as usize] = id;
                }
            }
        }
        let run = |quads: &[OverlayQuad]| {
            let mut rects = Vec::new();
            ui_occluders(quads, w, h, &mut rects);
            let mut img = IdImage { width: w, height: h, ids: ids.clone() };
            for (p, id) in img.ids.iter_mut().enumerate() {
                let (x, y) = (p as u32 % w, p as u32 / w);
                if rects.iter().any(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]) {
                    *id = 0;
                }
            }
            let mut cov = img.coverage(3);
            for c in &mut cov {
                c.rasterized = Some(true); // all three were drawn
            }
            let mut s = frame(&world, cam, w, h);
            assert!(s.iter().all(|s| s.on_screen));
            apply_visibility(&mut s, &[1, 2, 3], &cov);
            s
        };
        let get = |s: &[Subject], i: usize, k: &str| s[i].full().get(k).cloned();
        // An opaque panel over x 10..30, y 8..20, and a translucent wash over everything.
        let hud = [OverlayQuad::solid([10.0, 8.0, 20.0, 12.0], [0.2, 0.2, 0.2, 1.0]), OverlayQuad::solid([0.0, 0.0, 64.0, 48.0], [0.0, 0.0, 0.0, 0.5])];
        let with_ui = run(&hud);
        let without = run(&[]);
        // Entirely under the panel: nothing shows, something hid it.
        assert_eq!((get(&with_ui, 0, "visible_px"), get(&with_ui, 0, "occluded")), (Some(Json::from(0)), Some(Json::from(true))));
        assert_eq!(get(&with_ui, 0, "screen_box_exact"), Some(Json::Null));
        // Partly under it: only its uncovered columns 30..40, and the exact box shrinks to them.
        assert_eq!(get(&with_ui, 1, "visible_px"), Some(Json::from(100)));
        assert_eq!(get(&with_ui, 1, "screen_box_exact"), Some(Json::arr([30, 10, 10, 10].map(Json::from))));
        assert_eq!(get(&with_ui, 1, "occluded"), None);
        // Clear of it (the translucent wash hides nothing): as without the interface.
        assert_eq!(with_ui[2].full(), without[2].full());
        // No interface (include_ui false): the counts are the ID buffer's own.
        assert_eq!(get(&without, 0, "visible_px"), Some(Json::from(64)));
        assert_eq!(get(&without, 1, "visible_px"), Some(Json::from(150)));
        assert_eq!(get(&without, 1, "screen_box_exact"), Some(Json::arr([25, 10, 15, 10].map(Json::from))));
        let mut rects = vec![[1, 2, 3, 4]];
        assert_eq!((ui_occluders(&[], w, h, &mut rects), rects.len()), (0, 0));
    }

    #[test]
    fn billboard_subjects_are_the_card_as_drawn() {
        let mut world = World::default();
        let mut tree = SpawnSpec::new(Shape::Sprite);
        tree.name = Some("tree".into());
        tree.position = Vec3::new(0.0, 0.0, 0.0);
        let spec = crate::two_d::parse("sprite", &Json::parse(r#"{"size":[2,4],"pivot":[0.5,0],"billboard":"cylindrical"}"#).unwrap());
        tree.two_d = Some(spec.unwrap());
        world.spawn(tree, "t").unwrap();
        // Orthographic, 20 m tall at 400 x 400: 20 px per metre, seen from the side at 45 degrees.
        let cam = Camera { projection: Projection::Ortho { height_m: 20.0 }, ..Camera::new(Vec3::new(10.0, 0.0, 10.0), Vec3::ZERO, 50.0) };
        let s = subjects(&world, &cam.view_proj(400, 400), 400, 400, false);
        let [x, y, w, h] = rect(&s[0]);
        assert!((w as i64 - 40).abs() <= 2 && (h as i64 - 80).abs() <= 2, "the card faces the camera: {:?}", [x, y, w, h]);
        assert!((x as i64 - 180).abs() <= 1 && (y as i64 + h as i64 - 200).abs() <= 1, "feet at the centre: {:?}", [x, y, w, h]);
        // The card faces the eye 14.14 m away (centre at depth 14.14; its near edge too).
        let d = s[0].full().get("distance_m").and_then(Json::as_f64).unwrap();
        assert!((d - 200f64.sqrt()).abs() < 0.05, "orthographic distance along the view axis: {d}");
    }

    /// The subject cache against a cold read of every entity, through churn past the world's
    /// change record (more than DRAW_LOG changes between captures: bodies moving every tick, the
    /// stalls review's churn probe), deletes, spawns, a renamed parent, a rolled-back batch, and
    /// a rebuild in slices with the world changing past the record between every slice (it used
    /// to start again forever). A change to every entity mid-rebuild ends in one synchronous read.
    #[test]
    fn text_subjects_follow_a_re_layout_frames_adopted() {
        // Review of e97e318: a CJK text laid out again by the loader thread changes no draw stamp,
        // so the cached subject kept the old box and facts while frames drew the new layout.
        let mut world = World::default();
        let spec = crate::text_world::TextSpec::new("Here lies Ada");
        let batch = crate::text_world::tests::reserve_batch();
        let mut s = SpawnSpec::new(Shape::Text);
        s.name = Some("epitaph".into());
        s.two_d = Some(crate::two_d::TwoD::Text(Box::new(crate::text_world::tests::with_later(&spec, [-7.0, -7.0, 7.0, 7.0], batch))));
        world.spawn(s, "t").unwrap();
        let mut cache = SubjectCache::default();
        cache.refresh(&world, 1);
        let width = |c: &SubjectCache| {
            let b = c.entries.iter().find(|b| b.shape == "text").expect("a text subject");
            b.corners.iter().map(|p| p.x).fold(f32::MIN, f32::max) - b.corners.iter().map(|p| p.x).fold(f32::MAX, f32::min)
        };
        let before = width(&cache);
        assert!(before < 10.0, "the first layout's box: {before}");
        crate::text_world::tests::finish_batch(batch);
        cache.refresh(&world, 1);
        assert!((width(&cache) - 14.0).abs() < 1e-3, "the adopted layout's box: {}", width(&cache));
    }

    #[test]
    fn subject_cache_keeps_up_with_churn_past_the_change_record() {
        use crate::world::{Op, UpdateSpec, DRAW_LOG};
        let snapshot = |c: &SubjectCache| format!("{:?}", c.entries());
        let cold = |w: &World| {
            let mut c = SubjectCache::default();
            c.refresh(w, 1);
            snapshot(&c)
        };
        let mut world = World::default();
        let mut parent = SpawnSpec::new(Shape::Group);
        parent.name = Some("rig".into());
        world.spawn(parent, "t").unwrap();
        for i in 0..6 {
            let mut s = SpawnSpec::new(Shape::Cube);
            s.name = Some(format!("arm{i}"));
            s.parent = Some(EntityKey::Path("/rig".into()));
            world.spawn(s, "t").unwrap();
        }
        let n = 3000;
        for i in 0..n {
            spawn(&mut world, Shape::Cube, &format!("c{i}"), Vec3::new((i % 50) as f32, 0.0, (i / 50) as f32));
        }
        let ids: Vec<u64> = world.persistent_entities().filter(|e| e.shape == Shape::Cube && e.parent.is_none()).map(|e| e.id).collect();
        let shake = |world: &mut World, count: usize, step: f32| {
            for &id in ids.iter().take(count) {
                if world.get(id).is_none() {
                    continue;
                }
                let p = world.get(id).unwrap().position;
                world.update(&EntityKey::Id(id), UpdateSpec { position: Some(p + Vec3::new(step, 0.0, 0.0)), ..UpdateSpec::default() }, "mover").unwrap();
            }
        };
        let mut cache = SubjectCache::default();
        cache.refresh(&world, 1);
        assert_eq!(snapshot(&cache), cold(&world));
        let rebuilt = cache.counts().0;
        // Past the change record: patched from the draw stamps, never rebuilt.
        shake(&mut world, DRAW_LOG + 200, 0.25);
        let epoch = cache.key.unwrap().1;
        assert!(world.draw_changes_since(epoch).is_none(), "the record overflowed");
        assert!(cache.ready(&world, 1), "the stamps still say what changed");
        cache.refresh(&world, 1);
        assert_eq!(snapshot(&cache), cold(&world));
        assert_eq!((cache.counts().0, cache.scans()), (rebuilt, 1));
        // Deletes, spawns, a renamed parent (its children's paths move) and churn, together.
        for &id in ids.iter().skip(2000).take(10) {
            world.delete(&EntityKey::Id(id), None).unwrap();
        }
        for i in 0..5 {
            spawn(&mut world, Shape::Sphere, &format!("new{i}"), Vec3::new(i as f32, 3.0, 0.0));
        }
        world.update(&EntityKey::Path("/rig".into()), UpdateSpec { name: Some("rig2".into()), ..UpdateSpec::default() }, "t").unwrap();
        shake(&mut world, DRAW_LOG + 100, -0.5);
        cache.refresh(&world, 1);
        assert_eq!(snapshot(&cache), cold(&world));
        assert!(cache.entries().iter().any(|b| b.path == "/rig2/arm3"), "children's paths follow the rename");
        // A failed batch rolls its entities back to copies (their stamps unknown until relinked).
        let to = |p: [f32; 3]| UpdateSpec { position: Some(Vec3::new(p[0], p[1], p[2])), ..UpdateSpec::default() };
        let ops = vec![Op::Set(EntityKey::Path("/c1".into()), to([9.0, 9.0, 9.0])), Op::Set(EntityKey::Path("/nope".into()), to([0.0; 3]))];
        assert!(world.apply_batch(ops, "t").is_err(), "the batch fails and rolls back");
        shake(&mut world, DRAW_LOG + 1, 0.125);
        cache.refresh(&world, 1);
        assert_eq!(snapshot(&cache), cold(&world));
        // Most of the world changed (a world just built): not ready, so captures wait while it is
        // read again in slices, never in one go inside a call.
        shake(&mut world, ids.len(), 0.5);
        assert!(!cache.ready(&world, 1), "a patch of most entities is a rebuild: sliced");
        let mut passes = 0;
        while !cache.warm(&world, 1, std::time::Duration::ZERO) {
            passes += 1;
        }
        assert!(passes >= 10, "sliced ({passes} passes)");
        assert_eq!(snapshot(&cache), cold(&world));
        // A rebuild in slices with more than the record holds changing between slices: it ends
        // within one pass over the entities (plus the last patch), equal to a cold read.
        let mut slow = SubjectCache::default();
        let mut passes = 0;
        while !slow.warm(&world, 1, std::time::Duration::ZERO) {
            passes += 1;
            assert!(passes <= world.len() / 64 + 2, "the sliced rebuild never finishes");
            shake(&mut world, DRAW_LOG + 50, 0.0625);
        }
        assert!(passes >= 10, "it was sliced ({passes} passes)");
        assert_eq!(snapshot(&slow), cold(&world));
        // A change to every entity at once mid-rebuild: one synchronous read finishes it.
        let mut reset = SubjectCache::default();
        assert!(!reset.warm(&world, 1, std::time::Duration::ZERO));
        world.refresh_models();
        assert!(reset.warm(&world, 1, std::time::Duration::ZERO), "read synchronously, not started again");
        assert_eq!(snapshot(&reset), cold(&world));
    }

    #[test]
    fn most_relevant_keeps_big_on_screen_subjects() {
        let mut world = World::default();
        for i in 0..30 {
            spawn(&mut world, Shape::Cube, &format!("c{i}"), Vec3::new(i as f32 * 2.0, 0.0, -(i as f32)));
        }
        spawn(&mut world, Shape::Cube, "behind", Vec3::new(0.0, 0.0, 200.0));
        let cam = cam(Vec3::new(0.0, 2.0, 10.0), Vec3::new(10.0, 0.0, -10.0), 60.0);
        let all = frame(&world, cam, 640, 480);
        let (chosen, omitted) = most_relevant(&all, 5);
        assert_eq!((chosen.len(), omitted), (5, 26));
        let areas: Vec<u64> = chosen.iter().map(|j| {
            let r = j.get("screen").and_then(Json::as_array).unwrap();
            r[2].as_u64().unwrap() * r[3].as_u64().unwrap()
        }).collect();
        assert!(areas.windows(2).all(|w| w[0] >= w[1]), "largest first: {areas:?}");
        let (everything, none) = most_relevant(&all, 100);
        assert_eq!((everything.len(), none), (31, 0));
    }
}

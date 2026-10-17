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
}

impl Camera {
    /// A perspective camera.
    pub fn new(eye: Vec3, target: Vec3, fov_y_deg: f32) -> Camera {
        Camera { eye, target, fov_y_deg, projection: Projection::Perspective }
    }

    /// The 2D convention: an orthographic camera looking down −Z at the XY plane (+Y up on
    /// screen) centred on (`x`, `y`), `height_m` tall.
    pub fn view_2d(x: f32, y: f32, height_m: f32) -> Camera {
        Camera {
            eye: Vec3::new(x, y, 100.0),
            target: Vec3::new(x, y, 0.0),
            fov_y_deg: 50.0,
            projection: Projection::Ortho { height_m },
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
                proj * Mat4::look_at_rh(self.eye, self.target, Vec3::Y)
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
        Json::obj(f)
    }
}

/// Three-quarter view from above that fits everything in the world.
pub fn auto_camera(world: &World, fov_y_deg: f32) -> Camera {
    let (center, radius) = match world.bounds() {
        Some((lo, hi)) => ((lo + hi) * 0.5, ((hi - lo).length() * 0.5).max(0.5)),
        None => (Vec3::ZERO, 1.5),
    };
    let distance = radius / (fov_y_deg.to_radians() * 0.5).sin() * 1.15;
    let dir = Vec3::new(0.55, 0.45, 0.7).normalize();
    Camera::new(center + dir * distance, center, fov_y_deg)
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
        let key = (generation, meshes.lathes.version(), view.include_ui);
        let same_world = self.key.is_some_and(|k| k.0 == generation);
        let fresh = self.key == Some(key) && (self.epoch == world.draw_epoch() || self.patch(world, meshes, view.include_ui));
        let mut stats = LodStats::default();
        out.clear();
        if fresh {
            for c in &mut self.entries {
                draw_cached(c, world, meshes, view, &mut stats, out);
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
                        draw_cached(c, world, meshes, view, &mut stats, out);
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
            _ => LodChoice::Remembered(&mut self.transient_levels),
        } };
        for e in world.transient_entities() {
            match plan(e, meshes, transient.include_ui) {
                Plan::Skip => {}
                Plan::Item(d) => out.push(d),
                Plan::Model => draw_model(e, meshes, &mut transient, &mut stats, out),
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
fn draw_cached(c: &mut Cached, world: &World, meshes: &Meshes, view: &View, stats: &mut LodStats, out: &mut Vec<DrawItem>) {
    match c {
        Cached::Item { d, .. } => out.push(*d),
        Cached::Model { m, animated, drawn, level } => {
            if !*drawn {
                return;
            }
            let entity = if *animated { world.get(m.id) } else { None };
            let mut choice = match view.lod {
                LodChoice::Fixed(l) => LodChoice::Fixed(l),
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

/// Whether any of `e`'s bounding box can be in view of `view_proj` (conservative: false only
/// when every corner is beyond one side of the view, or behind it). Entities without bounds count
/// as in view.
pub fn in_view(e: &Entity, view_proj: &Mat4) -> bool {
    let Some(corners) = e.world_corners() else { return true };
    let clip = corners.map(|p| view_proj.mul_vec4(p.extend(1.0)));
    let all = |f: &dyn Fn(&Vec4) -> bool| clip.iter().all(f);
    !(all(&|p| p.w <= 0.0) || all(&|p| p.x < -p.w) || all(&|p| p.x > p.w) || all(&|p| p.y < -p.w) || all(&|p| p.y > p.w))
}

/// Where one entity landed in a frame.
#[derive(Clone, Debug)]
pub struct Subject {
    pub id: u64,
    pub json: Json,
    pub on_screen: bool,
    /// Pixel area of the clipped rectangle (0 when off screen).
    /// Becomes the exact box's area once the ID buffer ran ([`apply_visibility`]): ranks the response.
    pub area: u64,
    /// Pixel area of the projected rectangle, always (overlays rank by it, so their picks never
    /// depend on whether the ID buffer ran first).
    pub projected_area: u64,
}

/// One entry per entity with geometry in the frame, in id order: groups draw nothing, hidden
/// entities aren't drawn, and `ui` entities only count when the frame includes them.
///
/// `screen` is the pixel rectangle `[x, y, w, h]` (origin top-left) of the entity's bounding box
/// after clipping to the near plane and the image, or null when none of it is in view.
/// `distance_m` is the distance along the view axis to the nearest visible part of the box.
/// `position` is the world position. Models carry their `recipe`; transient entities say so.
pub fn subjects(world: &World, view_proj: &Mat4, width: u32, height: u32, include_ui: bool) -> Vec<Subject> {
    // Billboards turn to the camera: their subject is the card as drawn.
    let view = specula_gpu::view_2d(&view_proj.to_cols_array(), true);
    let ortho = view.ortho;
    world
        .entities()
        .filter(|e| e.local_bounds().is_some() && in_frame(e, include_ui))
        .map(|e| {
            let card = e.two_d.as_ref().and_then(|t| crate::two_d::billboard_corners(t, &e.model(), &view));
            let clip = clip_box(e, card, view_proj);
            let screen = screen_rect(&clip, width, height);
            // Perspective: clip w is the view depth. Orthographic (w = 1): undo the reversed-Z
            // depth range of `Projection::Ortho` instead.
            let depth = |p: &Vec4| if ortho { NEAR + (1.0 - p.z / p.w) * (ORTHO_FAR - NEAR) } else { p.w };
            let distance = clip.iter().map(depth).fold(f32::INFINITY, f32::min);
            let mut fields = vec![
                ("id", Json::from(e.id)),
                ("name", Json::from(e.name.as_str())),
                ("path", Json::from(world.path_of(e.id))),
                ("shape", Json::from(e.shape.name())),
            ];
            match (&e.model, &e.two_d) {
                (Some(m), _) => fields.push(("recipe", Json::from(m.recipe.as_str()))),
                (None, Some(t)) => fields.push(("two_d", t.subject_json())),
                (None, None) => fields.push(("material", Json::from(material_name(e.look.material)))),
            }
            fields.extend([
                ("color", Json::from(color_hex(e.color))),
                ("position", vec3_json(e.world_position())),
                ("created_by", Json::from(&*e.created_by)),
                ("on_screen", Json::from(screen.is_some())),
                ("screen", screen.map_or(Json::Null, |r| Json::arr(r.map(Json::from)))),
                ("distance_m", if screen.is_some() { Json::from(distance) } else { Json::Null }),
            ]);
            if e.transient {
                fields.push(("transient", Json::from(true)));
            }
            if e.in_ui() {
                fields.push(("layer", Json::from("ui")));
            }
            let json = Json::obj(fields);
            let area = screen.map_or(0, |r| u64::from(r[2]) * u64::from(r[3]));
            Subject { id: e.id, json, on_screen: screen.is_some(), area, projected_area: area }
        })
        .collect()
}

/// Dense ID-buffer ids for a draw list's owners (`draw_list`): the `k`-th distinct entity gets
/// `k + 1`. Returns `(id per draw, entity of each dense id - 1)`.
pub fn dense_ids(owners: &[u64]) -> (Vec<u32>, Vec<u64>) {
    let mut entities: Vec<u64> = Vec::new();
    let mut index: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();
    let ids = owners
        .iter()
        .map(|&e| {
            *index.entry(e).or_insert_with(|| {
                entities.push(e);
                entities.len() as u32
            })
        })
        .collect();
    (ids, entities)
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
    let by_entity: std::collections::HashMap<u64, specula_gpu::IdCoverage> =
        entities.iter().zip(coverage).map(|(e, c)| (*e, *c)).collect();
    for s in subjects {
        let drawn = by_entity.get(&s.id);
        if drawn.is_some_and(|c| !c.counted) {
            // Blended draws went uncounted (no fragment atomics): its pixels are unknown, not 0.
            s.json.set("visible_px", Json::Null);
            continue;
        }
        let cov = drawn.filter(|c| c.pixels > 0);
        let exact = cov.map(|c| [c.min[0], c.min[1], c.max[0] - c.min[0] + 1, c.max[1] - c.min[1] + 1]);
        s.json.set("visible_px", Json::from(cov.map_or(0, |c| c.pixels)));
        s.json.set("screen_box_exact", exact.map_or(Json::Null, |r| Json::arr(r.map(Json::from))));
        let approx = drawn.is_some_and(|c| c.approx);
        if approx {
            // More blended ids overlapped at some of its pixels than the ID buffer's sets hold:
            // visible_px and screen_box_exact are lower bounds (it shows at least that much).
            s.json.set("visible_px_approx", Json::from(true));
        }
        if let (true, Some(c), None, false) = (s.on_screen, drawn, cov, approx) {
            s.json.set("no_pixels", Json::from(true));
            if c.rasterized == Some(true) {
                s.json.set("occluded", Json::from(true));
            }
        }
        let exact_area = exact.map_or(0, |r| u64::from(r[2]) * u64::from(r[3]));
        // An approximate box is a lower bound (possibly empty): rank by what it may cover.
        s.area = if approx { exact_area.max(s.projected_area) } else { exact_area };
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
        return (subjects.iter().map(|s| s.json.clone()).collect(), 0);
    }
    let mut order: Vec<&Subject> = subjects.iter().filter(|s| s.on_screen).collect();
    order.sort_by_key(|s| std::cmp::Reverse(s.area));
    order.truncate(max);
    let chosen: Vec<Json> = order.iter().map(|s| s.json.clone()).collect();
    let omitted = subjects.len() - chosen.len();
    (chosen, omitted)
}

/// The entity's bounding box in clip space, clipped to the view depth range 0 ≤ z ≤ w: the box
/// corners in range plus every point where a box edge crosses one of those planes. Those are
/// exactly the vertices of the clipped box, so their projection bounds its image. With the
/// reversed-Z infinite projection, z ≤ w is the near plane and z ≥ 0 holds for everything in front.
fn clip_box(e: &Entity, card: Option<[Vec3; 4]>, view_proj: &Mat4) -> Vec<Vec4> {
    let corners = match card {
        // A flat card: its four corners twice make the "box" (zero depth).
        Some(c) => [c[0], c[1], c[3], c[2], c[0], c[1], c[3], c[2]],
        None => match e.world_corners() {
            Some(c) => c,
            None => return Vec::new(),
        },
    };
    let corners = corners.map(|p| view_proj.mul_vec4(p.extend(1.0)));
    let inside = |p: &Vec4| p.z >= 0.0 && p.z <= p.w;
    let mut points: Vec<Vec4> = corners.iter().copied().filter(inside).collect();
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
                    points.push(p);
                }
            }
        }
    }
    points
}

fn lerp(a: Vec4, b: Vec4, t: f32) -> Vec4 {
    Vec4::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t, a.w + (b.w - a.w) * t)
}

/// Pixel rectangle of clip-space points (all in front of the near plane), clipped to the image.
fn screen_rect(points: &[Vec4], width: u32, height: u32) -> Option<[u32; 4]> {
    if points.is_empty() {
        return None;
    }
    let (w, h) = (width as f32, height as f32);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for p in points {
        // Vulkan viewport transform: NDC −1..1 → 0..size (clip y already points down).
        let (x, y) = ((p.x / p.w + 1.0) * 0.5 * w, (p.y / p.w + 1.0) * 0.5 * h);
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let (cx0, cy0, cx1, cy1) = (x0.max(0.0), y0.max(0.0), x1.min(w), y1.min(h));
    if !(cx0 < cx1 && cy0 < cy1) {
        return None;
    }
    let (px, py) = (cx0.floor() as u32, cy0.floor() as u32);
    Some([px, py, (cx1.ceil() as u32).saturating_sub(px).max(1), (cy1.ceil() as u32).saturating_sub(py).max(1)])
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
        let a = subject.json.get("screen").and_then(Json::as_array).expect("on screen");
        [0, 1, 2, 3].map(|i| a[i].as_u64().unwrap())
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
            assert!(x + w <= 960 && y + h <= 540, "{}", s.json);
            assert!(w > 4 && h > 0, "{}", s.json);
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
        assert_eq!(s[0].json.get("on_screen").and_then(Json::as_bool), Some(false));
        assert!(s[0].json.get("screen").unwrap().is_null());
        assert!(s[0].json.get("distance_m").unwrap().is_null());
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
        assert_eq!((x, x + w), (0, 640), "{}", s[0].json);
        assert!(y >= 180 && y + h == 360, "{}", s[0].json);
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
        assert_eq!(s[1].json.get("on_screen").and_then(Json::as_bool), Some(true), "the sun is in view too");
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
        let d = s[1].json.get("distance_m").and_then(Json::as_f64).unwrap();
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
        assert_eq!(s[0].json.get("path").and_then(Json::as_str), Some("/tower/top"));
        assert_eq!(s[0].json.get("position"), Some(&vec3_json(Vec3::new(3.0, 2.0, 0.0))));
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
                .map(|s| s.json.get("name").and_then(Json::as_str).unwrap().to_owned())
                .collect()
        };
        assert_eq!(names(false), ["stone"]);
        assert_eq!(names(true), ["stone", "cursor"], "hidden stays hidden even with include_ui");
        let ui = subjects(&world, &cam.view_proj(640, 480), 640, 480, true);
        assert_eq!(ui[1].json.get("layer"), Some(&Json::from("ui")));
        assert_eq!((lights(&world, false, &mut Vec::new()), world.lit_entities().count()), (0, 0));
        assert!(!in_frame(world.get(2).unwrap(), false) && in_frame(world.get(2).unwrap(), true));
    }

    #[test]
    fn id_buffer_results_go_into_subjects() {
        use specula_gpu::IdCoverage;
        // A model's parts share its id; ids are dense in draw order.
        let (ids, entities) = dense_ids(&[7, 7, 3, 9, 3]);
        assert_eq!((ids, entities.clone()), (vec![1, 1, 2, 3, 2], vec![7, 3, 9]));
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
        let get = |i: usize, k: &str| s[i].json.get(k).cloned();
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
        assert_eq!(s[0].json.get("visible_px"), Some(&Json::Null));
        assert!(s[0].json.get("no_pixels").is_none() && s[0].json.get("screen_box_exact").is_none() && s[0].json.get("occluded").is_none());
        assert_eq!(s[0].area, s[0].projected_area);
        // Approximate with nothing counted (pushed out of every set it was drawn in): at least 0,
        // but it did show, so neither no_pixels nor occluded.
        let mut s = frame(&world, cam, 640, 480);
        let pushed_out = IdCoverage { approx: true, rasterized: Some(true), ..IdCoverage::default() };
        apply_visibility(&mut s, &[1], &[pushed_out]);
        assert_eq!((s[0].json.get("visible_px"), s[0].json.get("visible_px_approx")), (Some(&Json::from(0)), Some(&Json::from(true))));
        assert!(s[0].json.get("no_pixels").is_none() && s[0].json.get("occluded").is_none());
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
        let get = |s: &[Subject], i: usize, k: &str| s[i].json.get(k).cloned();
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
        assert_eq!(with_ui[2].json, without[2].json);
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
        let d = s[0].json.get("distance_m").and_then(Json::as_f64).unwrap();
        assert!((d - 200f64.sqrt()).abs() < 0.05, "orthographic distance along the view axis: {d}");
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

//! Agent sight: cameras, draw lists and the subject list that accompanies every captured frame.
//!
//! Subjects come from the engine's own transforms and the view-projection used to render, so agents
//! never guess where something is. Rectangles are the projected **bounding box** of each entity
//! (clipped to the camera's near/far planes and the image); occlusion is not considered yet.

use crate::meshes::{LatheMeshes, ShapeMeshes};
use crate::models::{GpuModels, LodChoice, LodStats, ModelDraw};
use crate::world::{color_hex, material_name, vec3_json, Entity, Shape, World};
use specula_core::math::{Mat4, Vec3, Vec4};
use specula_core::Json;
use specula_gpu::{DrawItem, FrameDesc, PointLight, MAX_POINT_LIGHTS};

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
pub fn draw_list(world: &World, meshes: &Meshes, view: &mut View, out: &mut Vec<DrawItem>) -> LodStats {
    out.clear();
    let mut stats = LodStats::default();
    for e in world.entities() {
        match plan(e, meshes, view.include_ui) {
            Plan::Skip => {}
            Plan::Item(d) => out.push(d),
            Plan::Model => draw_model(e, meshes, view, &mut stats, out),
        }
    }
    stats
}

/// What an entity adds to a frame: nothing, one fixed draw (built-in shapes and lathes: it
/// depends only on the entity and its mesh), or a model's draws (chosen per frame: LOD, pose).
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

/// The window's draw list, kept between frames. While nothing in the world changed, each
/// entity's fixed draw (built-in shapes, lathes) is copied from the last frame and each model is
/// drawn from a copy of its drawing state, so a static 10k-entity world doesn't walk every
/// entity's full record each frame (F3: on the reference iGPU that walk was most of the window's
/// CPU frame). Models still choose their LOD per frame (each keeps its level here, for the
/// window's hysteresis) and animated ones are posed from their entity. Same draws, same order as
/// [`draw_list`]; nothing allocated once warm.
#[derive(Default)]
pub struct DrawCache {
    /// `(world generation, World::draw_epoch, LatheMeshes::version, include_ui)` the entries
    /// were built for.
    key: Option<(u64, u64, u64, bool)>,
    entries: Vec<Cached>,
    /// The previous entries while rebuilding (their LOD levels carry over); kept for its capacity.
    spare: Vec<Cached>,
    rebuilds: u64,
}

#[derive(Clone, Copy)]
enum Cached {
    Item(DrawItem),
    Model {
        m: ModelDraw,
        /// Has an `anim`: posed from the entity each frame.
        animated: bool,
        /// The LOD it was last drawn at (`u8::MAX`: none yet).
        level: u8,
    },
}

impl DrawCache {
    /// How many times the entries were rebuilt (the world or its meshes changed).
    pub fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// [`draw_list`] for `world` (`generation` changes whenever the engine replaces its world).
    /// LODs are chosen by size with each model's remembered level, or `view.lod`'s if Fixed.
    pub fn draw_list(&mut self, world: &World, generation: u64, meshes: &Meshes, view: &mut View, out: &mut Vec<DrawItem>) -> LodStats {
        let key = (generation, world.draw_epoch(), meshes.lathes.version(), view.include_ui);
        if self.key != Some(key) {
            let same_world = self.key.is_some_and(|k| k.0 == generation);
            std::mem::swap(&mut self.entries, &mut self.spare);
            self.entries.clear();
            // Levels of the previous entries, matched by id (both lists are in id order).
            let mut old = self.spare.iter().filter_map(|c| match c {
                Cached::Model { m, level, .. } if same_world => Some((m.id, *level)),
                _ => None,
            }).peekable();
            for e in world.entities() {
                match plan(e, meshes, view.include_ui) {
                    Plan::Skip => {}
                    Plan::Item(d) => self.entries.push(Cached::Item(d)),
                    Plan::Model => {
                        while old.next_if(|(id, _)| *id < e.id).is_some() {}
                        let level = old.next_if(|(id, _)| *id == e.id).map_or(u8::MAX, |(_, l)| l);
                        self.entries.push(Cached::Model { m: ModelDraw::of(e), animated: e.anim.is_some(), level });
                    }
                }
            }
            self.spare.clear();
            self.key = Some(key);
            self.rebuilds += 1;
        }
        out.clear();
        let mut stats = LodStats::default();
        let fixed = match view.lod {
            LodChoice::Fixed(l) => Some(l),
            _ => None,
        };
        let (eye, fov) = (view.camera.eye, view.camera.fov_y_deg);
        for entry in &mut self.entries {
            match entry {
                Cached::Item(d) => out.push(*d),
                Cached::Model { m, animated, level } => {
                    let entity = if *animated { world.get(m.id) } else { None };
                    let mut choice = match fixed {
                        Some(l) => LodChoice::Fixed(l),
                        None => LodChoice::Slot(level),
                    };
                    meshes.models.draw_model(m, entity, eye, fov, view.viewport_h, &mut choice, &mut stats, out);
                }
            }
        }
        stats
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
pub struct Subject {
    pub json: Json,
    pub on_screen: bool,
    /// Pixel area of the clipped rectangle (0 when off screen).
    pub area: u64,
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
            Subject { json, on_screen: screen.is_some(), area }
        })
        .collect()
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
            let sa = draw_list(world, &meshes, &mut view, &mut a);
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

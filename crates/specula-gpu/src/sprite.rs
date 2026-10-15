//! 2D in the 3D scene: sprites, tile chunks and vector shapes ([`Draw2d`]), drawn inside the
//! scene pass.
//!
//! # Where they draw
//!
//! The scene pass draws opaque meshes, then the sky, then **every 2D item**, then blended meshes.
//! The graph lists this as the inline pass `sprites` joined to `scene` (no barriers, no extra
//! store or load); its GPU time is reported as the `sprites` pass. Cutout items write depth, so
//! glass and other blended meshes in front of a billboard tree blend over it correctly; a
//! blended item does not, so a blended mesh *behind* a translucent sprite draws over it (sort such
//! scenes by hand or use cutout).
//!
//! # Order
//!
//! Visible items are sorted by `layer` (ascending), then far to near along the view axis, then by
//! list position, and drawn in that order, cutout and blended alike (painter's order), with depth
//! test "greater **or equal**" (reversed Z; "less or equal" otherwise) so that coplanar items —
//! every layer of a flat 2D scene at z = 0 — draw over each other in sort order. Consecutive
//! items of one kind merge into one instanced draw call: textures are bindless (each item names
//! its own), so sprites with different textures still batch; only a change between cutout,
//! blended and shape breaks a batch (and each shape mesh is its own draw).
//!
//! # Culling
//!
//! Every item has a bounding sphere (a sprite: around its position, covering any rotation or
//! billboard turn; a tile chunk: its cells; a shape: its mesh) tested against the view frustum.
//! Tile maps are drawn one [`TileChunk`] per chunk (see [`crate::TileGrid`]), so a large map
//! costs only the chunks in view.

use crate::batch::{world_sphere, Bounds, Frustum};
use crate::device::Device;
use crate::overlay::Textures;
use crate::render::{camera_position, LinearCache, Mesh, PUSH_STAGES};
use crate::shadow::invert;
use crate::vk::*;
use crate::{DrawStats, FrameDesc, GpuError, IdKind, MeshId, TextureFilter, TextureId};
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;

/// How a sprite turns towards the camera.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Billboard {
    /// Fixed in the world: the quad lies along [`Sprite::axes`] (a 2D scene's XY plane by default).
    #[default]
    None,
    /// Always faces the camera (parallel to the image plane): particles, icons, labels.
    Spherical,
    /// Turns about its up axis (`axes[1]`) to face the camera: trees, characters standing on
    /// the ground.
    Cylindrical,
}

/// How a sprite's alpha is used.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SpriteBlend {
    /// Alpha test: texels below `threshold` are discarded, the rest are opaque and **write
    /// depth** (and may cast shadows). Pixel art, foliage cards.
    Cutout { threshold: f32 },
    /// Premultiplied blending over what is behind; no depth write. Soft edges, smoke, glows.
    #[default]
    Blended,
}


/// A textured quad in the world. Sizes in metres; colours sRGB with straight alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    /// Where the pivot is.
    pub position: [f32; 3],
    /// Width and height in metres.
    pub size: [f32; 2],
    /// Radians, counter-clockwise as seen from the front, about the pivot.
    pub rotation: f32,
    /// The point of the quad at `position`: (0, 0) = bottom left, (0.5, 0.5) = centre,
    /// (0.5, 0) = bottom centre (a character's feet).
    pub pivot: [f32; 2],
    /// The quad's right and up directions in the world (normalized here). [`Billboard::None`]
    /// uses both; [`Billboard::Cylindrical`] turns about `axes[1]`. Default: +X, +Y (a 2D
    /// scene's plane, seen from +Z).
    pub axes: [[f32; 3]; 2],
    /// `None` = a flat `color` quad.
    pub texture: Option<TextureId>,
    /// Texture coordinates of the top-left and bottom-right corners: u0, v0, u1, v1 (an atlas
    /// rect or flipbook frame).
    pub uv: [f32; 4],
    /// sRGB tint and straight alpha; multiplies the texel.
    pub color: [f32; 4],
    /// Mirror horizontally / vertically.
    pub flip: [bool; 2],
    /// Sort key: higher layers draw over lower ones (see the module docs).
    pub layer: i32,
    pub billboard: Billboard,
    pub blend: SpriteBlend,
    pub filter: TextureFilter,
    /// Orthographic views: put the quad's corner (0, 0) on a whole pixel, so pixel art at a
    /// whole-pixel scale stays crisp as the camera moves.
    pub pixel_snap: bool,
    /// Lit by the sun (with shadows), ambient and point lights; unlit sprites show their texel
    /// times the tint (exposure and tone mapping still apply).
    pub lit: bool,
    /// Cutout sprites only: cast a shadow into the sun's shadow map (the card as the camera sees
    /// it).
    pub cast_shadows: bool,
}

impl Default for Sprite {
    fn default() -> Self {
        Sprite {
            position: [0.0; 3],
            size: [1.0, 1.0],
            rotation: 0.0,
            pivot: [0.5, 0.5],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            texture: None,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: [1.0; 4],
            flip: [false; 2],
            layer: 0,
            billboard: Billboard::None,
            blend: SpriteBlend::Blended,
            filter: TextureFilter::Linear,
            pixel_snap: false,
            lit: false,
            cast_shadows: false,
        }
    }
}

/// A rectangle of one tile map layer, drawn as a single quad: the fragment shader looks each
/// cell up in `cells` (an `Rgba8Unorm` texture holding one texel per cell, encoded by
/// [`crate::TileCell::encode`]) and samples the tile from `tileset`. Editing a tile rewrites one
/// texel; animated tiles and flips cost nothing on the CPU.
///
/// Cell (column `x`, row `y`) covers `origin + axes[0]·[x, x+1]·tile_size[0] +
/// axes[1]·[y, y+1]·tile_size[1]`: `origin` is the top-left corner of cell (0, 0), and rows go
/// along `axes[1]` (default −Y, so row 0 is the top row of a 2D map, as in its image or CSV).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileChunk {
    pub origin: [f32; 3],
    /// Column and row directions (normalized here). Default +X and −Y; a ground map uses +X, +Z.
    pub axes: [[f32; 3]; 2],
    /// Metres per tile.
    pub tile_size: [f32; 2],
    pub tileset: TextureId,
    /// Tile size in tileset pixels; tiles are numbered from 1, left to right then top to bottom.
    pub tile_px: [u32; 2],
    /// Tileset pixels around the tiles and between them.
    pub margin: u32,
    pub spacing: u32,
    /// The layer's cell texture (one texel per cell).
    pub cells: TextureId,
    /// Cells drawn: x0, y0, width, height (inside `cells`).
    pub region: [u32; 4],
    /// sRGB tint, straight alpha.
    pub color: [f32; 4],
    pub layer: i32,
    pub blend: SpriteBlend,
    pub filter: TextureFilter,
    pub pixel_snap: bool,
    pub lit: bool,
}

impl TileChunk {
    /// A chunk of `cells` drawn with `tileset` at 1 m per tile in the XY plane (top-left at the
    /// origin), crisp pixel art cutouts.
    pub fn new(tileset: TextureId, tile_px: [u32; 2], cells: TextureId, region: [u32; 4]) -> TileChunk {
        TileChunk {
            origin: [0.0; 3],
            axes: [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0]],
            tile_size: [1.0, 1.0],
            tileset,
            tile_px,
            margin: 0,
            spacing: 0,
            cells,
            region,
            color: [1.0; 4],
            layer: 0,
            blend: SpriteBlend::Cutout { threshold: 0.5 },
            filter: TextureFilter::Nearest,
            pixel_snap: false,
            lit: false,
        }
    }
}

/// A tessellated vector shape ([`crate::vector`]) uploaded with [`crate::Gpu::upload_mesh`],
/// placed by `model`. Always blended (its edges fade over about a pixel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeDraw {
    pub mesh: MeshId,
    pub model: [f32; 16],
    /// sRGB tint, straight alpha; multiplies the vertex colours.
    pub color: [f32; 4],
    pub layer: i32,
}

/// One 2D item of a frame ([`crate::FrameDesc::draws_2d`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Draw2d {
    Sprite(Sprite),
    Tiles(TileChunk),
    Shape(ShapeDraw),
}

impl Draw2d {
    pub fn layer(&self) -> i32 {
        match self {
            Draw2d::Sprite(s) => s.layer,
            Draw2d::Tiles(t) => t.layer,
            Draw2d::Shape(s) => s.layer,
        }
    }
}

/// The mesh id of the shared quad whose index buffer (0 1 2, 0 2 3) every sprite and tile chunk
/// draw uses (its vertices are made in the vertex shader). Never handed out by `upload_mesh`.
pub const QUAD_MESH: u32 = u32::MAX;

pub const KIND_SPRITE: u32 = 0;
pub const KIND_TILES: u32 = 1;
pub const KIND_SHAPE: u32 = 2;

pub const FLAG_BILLBOARD_SPHERICAL: u32 = 1;
pub const FLAG_BILLBOARD_CYLINDRICAL: u32 = 2;
pub const FLAG_FLIP_X: u32 = 4;
pub const FLAG_FLIP_Y: u32 = 8;
pub const FLAG_NEAREST: u32 = 16;
pub const FLAG_SNAP: u32 = 32;
pub const FLAG_LIT: u32 = 64;
pub const FLAG_CUTOUT: u32 = 128;

/// Specialization bit of the cutout sprite pipeline (`CUTOUT` in `shaders/sprite.frag`).
pub const FEAT_CUTOUT: u32 = 128;

/// One item as the 2D shaders read it; must match `Item` in `shaders/sprite.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ItemRecord {
    /// Linear RGB, straight alpha.
    pub tint: [f32; 4],
    pub v1: [f32; 4],
    pub v2: [f32; 4],
    pub v3: [f32; 4],
    pub v4: [f32; 4],
    pub v5: [f32; 4],
    /// Texture slot + 1, flags, cell texture slot + 1, kind.
    pub info: [u32; 4],
}

/// Push constants of the 2D pipelines; must match `Push` in `shaders/sprite.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpritePush {
    pub frame: u64,
    pub items: u64,
    /// Shadow cascade (sprite shadow casters).
    pub pass: u32,
    /// Bit 0: orthographic view.
    pub mode: u32,
    pub pad: [u32; 2],
    pub cam_pos: [f32; 4],
    pub cam_right: [f32; 4],
    pub cam_up: [f32; 4],
    pub cam_forward: [f32; 4],
}

const _: () = assert!(size_of::<ItemRecord>() == 112);
const _: () = assert!(size_of::<SpritePush>() <= 128);

/// The camera as 2D drawing needs it: billboards face it, items sort along its axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View2d {
    /// Perspective: the eye. Orthographic: the centre of the near plane.
    pub position: [f32; 3],
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub ortho: bool,
}

impl Default for View2d {
    fn default() -> Self {
        View2d { position: [0.0; 3], forward: [0.0, 0.0, -1.0], right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], ortho: false }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `a` normalized, or `fallback` when it is (nearly) zero or not finite.
pub(crate) fn normalize_or(a: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l > 1e-12 && l.is_finite() {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        fallback
    }
}

/// The camera of `view_proj` (Vulkan clip; reversed Z when `reverse_z`). A singular matrix gives
/// the default view (looking down −Z).
pub fn view_2d(view_proj: &[f32; 16], reverse_z: bool) -> View2d {
    let Some(inv) = invert(view_proj) else { return View2d::default() };
    let unproject = |x: f32, y: f32, z: f32| {
        let h: [f32; 4] = std::array::from_fn(|r| inv[r] * x + inv[4 + r] * y + inv[8 + r] * z + inv[12 + r]);
        [h[0] / h[3], h[1] / h[3], h[2] / h[3]]
    };
    // The clip w row is (0, 0, 0, w) exactly when the projection is orthographic.
    let ortho = view_proj[3].abs() < 1e-12 && view_proj[7].abs() < 1e-12 && view_proj[11].abs() < 1e-12;
    let z_near = if reverse_z { 1.0 } else { 0.0 };
    let near = unproject(0.0, 0.0, z_near);
    let mid = unproject(0.0, 0.0, 0.5);
    let d = View2d::default();
    View2d {
        position: if ortho { near } else { camera_position(view_proj) },
        forward: normalize_or(sub(mid, near), d.forward),
        right: normalize_or(sub(unproject(1.0, 0.0, z_near), near), d.right),
        // Clip y points down.
        up: normalize_or(sub(unproject(0.0, -1.0, z_near), near), d.up),
        ortho,
    }
}

/// Consecutive sorted items drawn with one call: records `first .. first + count`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Batch2d {
    pub kind: BatchKind,
    pub first: u32,
    pub count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchKind {
    /// Sprites and tile chunks, alpha tested (depth written).
    Cutout,
    /// Sprites and tile chunks, blended.
    Blended,
    /// Instances of one shape mesh.
    Shape(MeshId),
}

/// A frame's 2D work. Reuse one across frames: no allocation once warm.
#[derive(Clone, Debug, Default)]
pub struct Plan2d {
    pub batches: Vec<Batch2d>,
    /// The drawn items' records in draw order, then one per shadow-casting sprite.
    pub records: Vec<ItemRecord>,
    /// For each drawn record, its index in `FrameDesc::draws_2d` (sight's ID buffer gives each
    /// record the id of its draw).
    pub order: Vec<u32>,
    /// Records of drawn items (the shadow casters follow them).
    pub drawn: u32,
    /// Shadow-casting sprite records after the drawn ones.
    pub casters: u32,
    /// Items outside the view frustum.
    pub culled: u32,
    /// Drawn items by kind.
    pub sprites: u32,
    pub tile_chunks: u32,
    pub shapes: u32,
    pub view: View2d,
    /// Sort scratch: layer key, depth key and item index packed high to low.
    keys: Vec<u128>,
}

/// Ascending order of `f` as unsigned bits (NaN as 0).
fn order_bits(f: f32) -> u32 {
    let f = if f.is_nan() { 0.0 } else { f };
    let b = f.to_bits();
    if b >> 31 == 1 {
        !b
    } else {
        b | 0x8000_0000
    }
}

fn axes_of(axes: &[[f32; 3]; 2], default: [[f32; 3]; 2]) -> [[f32; 3]; 2] {
    [normalize_or(axes[0], default[0]), normalize_or(axes[1], default[1])]
}

/// A sprite's world bounding sphere: around its position, covering the quad at any rotation
/// and any billboard turn.
pub fn sprite_sphere(s: &Sprite) -> ([f32; 3], f32) {
    let reach = |p: f32, size: f32| (p.abs().max((1.0 - p).abs()) * size).abs();
    let (rx, ry) = (reach(s.pivot[0], s.size[0]), reach(s.pivot[1], s.size[1]));
    (s.position, (rx * rx + ry * ry).sqrt())
}

/// A tile chunk's world bounding sphere (its drawn cells).
pub fn tiles_sphere(t: &TileChunk) -> ([f32; 3], f32) {
    let [ax, ay] = axes_of(&t.axes, [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0]]);
    let [x0, y0, w, h] = t.region.map(|v| v as f32);
    let cx = (x0 + 0.5 * w) * t.tile_size[0];
    let cy = (y0 + 0.5 * h) * t.tile_size[1];
    let o = t.origin;
    let center = std::array::from_fn(|k| o[k] + ax[k] * cx + ay[k] * cy);
    let (hw, hh) = (0.5 * w * t.tile_size[0], 0.5 * h * t.tile_size[1]);
    (center, (hw * hw + hh * hh).sqrt())
}

fn blend_flags(blend: SpriteBlend) -> (u32, f32) {
    match blend {
        SpriteBlend::Cutout { threshold } => (FLAG_CUTOUT, if threshold.is_finite() { threshold.clamp(0.0, 1.0) } else { 0.5 }),
        SpriteBlend::Blended => (0, 0.0),
    }
}

fn common_flags(filter: TextureFilter, snap: bool, lit: bool) -> u32 {
    let mut f = 0;
    if filter == TextureFilter::Nearest {
        f |= FLAG_NEAREST;
    }
    if snap {
        f |= FLAG_SNAP;
    }
    if lit {
        f |= FLAG_LIT;
    }
    f
}

fn finite4(v: [f32; 4]) -> [f32; 4] {
    v.map(|x| if x.is_finite() { x } else { 0.0 })
}

fn tint(color: [f32; 4], cache: &mut LinearCache) -> [f32; 4] {
    let l = cache.get([color[0], color[1], color[2]]);
    [l[0], l[1], l[2], if color[3].is_nan() { 0.0 } else { color[3].clamp(0.0, 1.0) }]
}

/// The shader record of a sprite (`slot`: its texture's bindless slot).
pub fn sprite_record(s: &Sprite, slot: Option<u32>, cache: &mut LinearCache) -> ItemRecord {
    let [ax, ay] = axes_of(&s.axes, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    let (cut, cutoff) = blend_flags(s.blend);
    let mut flags = cut | common_flags(s.filter, s.pixel_snap, s.lit);
    flags |= match s.billboard {
        Billboard::None => 0,
        Billboard::Spherical => FLAG_BILLBOARD_SPHERICAL,
        Billboard::Cylindrical => FLAG_BILLBOARD_CYLINDRICAL,
    };
    if s.flip[0] {
        flags |= FLAG_FLIP_X;
    }
    if s.flip[1] {
        flags |= FLAG_FLIP_Y;
    }
    let p = s.position;
    ItemRecord {
        tint: tint(s.color, cache),
        v1: finite4([p[0], p[1], p[2], s.rotation]),
        v2: finite4([s.size[0], s.size[1], s.pivot[0], s.pivot[1]]),
        v3: finite4(s.uv),
        v4: [ax[0], ax[1], ax[2], 0.0],
        v5: [ay[0], ay[1], ay[2], cutoff],
        info: [slot.map_or(0, |s| s + 1), flags, 0, KIND_SPRITE],
    }
}

/// The shader record of a tile chunk (`tileset` and `cells`: their bindless slots).
pub fn tiles_record(t: &TileChunk, tileset: u32, cells: u32, cache: &mut LinearCache) -> ItemRecord {
    let [ax, ay] = axes_of(&t.axes, [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0]]);
    let (cut, cutoff) = blend_flags(t.blend);
    let flags = cut | common_flags(t.filter, t.pixel_snap, t.lit);
    let o = t.origin;
    ItemRecord {
        tint: tint(t.color, cache),
        v1: finite4([o[0], o[1], o[2], t.spacing as f32]),
        v2: finite4([t.tile_size[0], t.tile_size[1], t.tile_px[0] as f32, t.tile_px[1] as f32]),
        v3: t.region.map(|v| v as f32),
        v4: [ax[0], ax[1], ax[2], t.margin as f32],
        v5: [ay[0], ay[1], ay[2], cutoff],
        info: [tileset + 1, flags, cells + 1, KIND_TILES],
    }
}

/// The shader record of a shape.
pub fn shape_record(s: &ShapeDraw, cache: &mut LinearCache) -> ItemRecord {
    let m = &s.model;
    ItemRecord {
        tint: tint(s.color, cache),
        v1: [m[0], m[1], m[2], m[3]],
        v2: [m[4], m[5], m[6], m[7]],
        v3: [m[8], m[9], m[10], m[11]],
        v4: [m[12], m[13], m[14], m[15]],
        v5: [0.0; 4],
        info: [0, 0, 0, KIND_SHAPE],
    }
}

/// Fail early: a 2D item names a texture or mesh that does not exist.
/// Texture slots with the last lookup remembered: runs of one texture (the usual case) skip the
/// hash map.
struct Slots<'a> {
    textures: &'a Textures,
    last: Option<(TextureId, Option<u32>)>,
}

impl Slots<'_> {
    fn get(&mut self, t: TextureId) -> Option<u32> {
        match self.last {
            Some((id, slot)) if id == t => slot,
            _ => {
                let slot = self.textures.slot(t);
                self.last = Some((t, slot));
                slot
            }
        }
    }
}

pub fn check_2d(draws: &[Draw2d], meshes: &HashMap<u32, Mesh>, textures: &Textures) -> Result<(), GpuError> {
    let mut slots = Slots { textures, last: None };
    let mut texture =
        |t: TextureId| if slots.get(t).is_some() { Ok(()) } else { Err(GpuError::unknown(IdKind::Texture, t.0, " in draws_2d")) };
    for d in draws {
        match d {
            Draw2d::Sprite(s) => s.texture.map_or(Ok(()), &mut texture)?,
            Draw2d::Tiles(t) => {
                texture(t.tileset)?;
                texture(t.cells)?;
            }
            Draw2d::Shape(s) => {
                if !meshes.contains_key(&s.mesh.0) {
                    return Err(GpuError::unknown(IdKind::Mesh, s.mesh.0, " in draws_2d"));
                }
            }
        }
    }
    Ok(())
}

/// Cull, sort and batch the frame's 2D items into `out` (records included). `bounds` gives a
/// shape mesh's object-space sphere. Shadow-casting cutout sprites are appended after the drawn
/// records, whether or not they are in view (their shadows may be).
pub fn plan_2d(
    frame: &FrameDesc,
    bounds: impl Fn(MeshId) -> Option<Bounds>,
    textures: &Textures,
    cache: &mut LinearCache,
    shadows: bool,
    out: &mut Plan2d,
) {
    let Plan2d { batches, records, keys, order, .. } = out;
    batches.clear();
    records.clear();
    keys.clear();
    order.clear();
    out.drawn = 0;
    out.casters = 0;
    out.culled = 0;
    out.sprites = 0;
    out.tile_chunks = 0;
    out.shapes = 0;
    if frame.draws_2d.is_empty() {
        return;
    }
    let view = view_2d(&frame.view_proj, frame.reverse_z);
    out.view = view;
    let frustum = Frustum::from_view_proj(&frame.view_proj);
    let mut culled = 0;
    for (i, d) in frame.draws_2d.iter().enumerate() {
        let (center, radius) = match d {
            Draw2d::Sprite(s) => sprite_sphere(s),
            Draw2d::Tiles(t) => tiles_sphere(t),
            Draw2d::Shape(s) => match bounds(s.mesh) {
                Some(b) => world_sphere(&s.model, b),
                None => ([s.model[12], s.model[13], s.model[14]], f32::INFINITY),
            },
        };
        if !frustum.sphere_visible(center, radius) {
            culled += 1;
            continue;
        }
        let depth = dot(sub(center, view.position), view.forward);
        // Layer ascending, then far to near, then list order.
        let layer = ((d.layer() as u32) ^ 0x8000_0000) as u128;
        keys.push(layer << 64 | (!order_bits(depth) as u128) << 32 | i as u128);
    }
    keys.sort_unstable();
    let mut slots = Slots { textures, last: None };
    for &key in keys.iter() {
        let i = key as u32;
        let d = &frame.draws_2d[i as usize];
        let (record, kind) = match d {
            Draw2d::Sprite(s) => {
                out.sprites += 1;
                let kind = if matches!(s.blend, SpriteBlend::Cutout { .. }) { BatchKind::Cutout } else { BatchKind::Blended };
                (sprite_record(s, s.texture.and_then(|t| slots.get(t)), cache), kind)
            }
            Draw2d::Tiles(t) => {
                out.tile_chunks += 1;
                let kind = if matches!(t.blend, SpriteBlend::Cutout { .. }) { BatchKind::Cutout } else { BatchKind::Blended };
                (tiles_record(t, slots.get(t.tileset).unwrap_or(0), slots.get(t.cells).unwrap_or(0), cache), kind)
            }
            Draw2d::Shape(s) => {
                out.shapes += 1;
                (shape_record(s, cache), BatchKind::Shape(s.mesh))
            }
        };
        let n = records.len() as u32;
        records.push(record);
        order.push(i);
        match batches.last_mut() {
            Some(b) if b.kind == kind => b.count += 1,
            _ => batches.push(Batch2d { kind, first: n, count: 1 }),
        }
    }
    out.drawn = records.len() as u32;
    out.culled = culled;
    if shadows {
        for d in frame.draws_2d {
            if let Draw2d::Sprite(s) = d {
                if s.cast_shadows && matches!(s.blend, SpriteBlend::Cutout { .. }) {
                    records.push(sprite_record(s, s.texture.and_then(|t| slots.get(t)), cache));
                }
            }
        }
        out.casters = records.len() as u32 - out.drawn;
    }
}

impl Plan2d {
    /// The push constants of the frame's 2D draws (`items`: device address of the records).
    pub fn push(&self, frame: u64, items: u64) -> SpritePush {
        let v = &self.view;
        let w = |a: [f32; 3]| [a[0], a[1], a[2], 0.0];
        SpritePush {
            frame,
            items,
            pass: 0,
            mode: v.ortho as u32,
            pad: [0; 2],
            cam_pos: w(v.position),
            cam_right: w(v.right),
            cam_up: w(v.up),
            cam_forward: w(v.forward),
        }
    }
}

/// The pipelines and data the 2D draws of a frame need.
#[derive(Clone, Copy)]
pub struct Draw2dContext<'a> {
    pub layout: VkPipelineLayout,
    pub cutout: VkPipeline,
    pub blended: VkPipeline,
    pub shape: VkPipeline,
    pub meshes: &'a HashMap<u32, Mesh>,
    pub push: SpritePush,
    pub reverse_z: bool,
}

fn push(dev: &Device, cmd: VkCommandBuffer, layout: VkPipelineLayout, push: &SpritePush) {
    // SAFETY: `cmd` is recording; the layout's push range covers both stages and 128 bytes.
    unsafe {
        (dev.fns.cmd_push_constants)(cmd, layout, PUSH_STAGES, 0, size_of::<SpritePush>() as u32, push as *const SpritePush as *const c_void)
    };
}

/// Record the plan's batches inside the scene's rendering instance (the global descriptor set
/// bound). Leaves the depth compare op at "or equal" and a 2D pipeline bound: the caller rebinds
/// and resets both before drawing meshes again.
pub fn record_2d(dev: &Device, cmd: VkCommandBuffer, ctx: &Draw2dContext, plan: &Plan2d, stats: &mut DrawStats) {
    if plan.batches.is_empty() {
        return;
    }
    let mut bound = VK_NULL_HANDLE;
    let mut bound_mesh = None;
    // SAFETY: `cmd` is recording inside the scene's rendering instance; the pipelines match its
    // attachments and the layout; every record index is within the plan's records in the upload
    // buffer at `ctx.push.items`; shape meshes are live (`check_2d`).
    unsafe {
        let compare = if ctx.reverse_z { VK_COMPARE_OP_GREATER_OR_EQUAL } else { VK_COMPARE_OP_LESS_OR_EQUAL };
        for b in &plan.batches {
            let pipeline = match b.kind {
                BatchKind::Cutout => ctx.cutout,
                BatchKind::Blended => ctx.blended,
                BatchKind::Shape(_) => ctx.shape,
            };
            if pipeline != bound {
                (dev.fns.cmd_bind_pipeline)(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);
                if bound == VK_NULL_HANDLE {
                    push(dev, cmd, ctx.layout, &ctx.push);
                }
                (dev.fns.cmd_set_depth_compare_op)(cmd, compare);
                bound = pipeline;
                stats.pipeline_binds += 1;
            }
            match b.kind {
                BatchKind::Shape(mesh) => {
                    let Some(m) = ctx.meshes.get(&mesh.0) else { continue };
                    if bound_mesh != Some(mesh) {
                        let offset: VkDeviceSize = 0;
                        (dev.fns.cmd_bind_vertex_buffers)(cmd, 0, 1, &m.buffer.handle, &offset);
                        (dev.fns.cmd_bind_index_buffer)(cmd, m.buffer.handle, m.index_offset, VK_INDEX_TYPE_UINT32);
                        bound_mesh = Some(mesh);
                    }
                    (dev.fns.cmd_draw_indexed)(cmd, m.index_count, b.count, 0, 0, b.first);
                    stats.triangles_2d += (m.index_count / 3) as u64 * b.count as u64;
                }
                _ => {
                    let Some(q) = ctx.meshes.get(&QUAD_MESH) else { continue };
                    if bound_mesh != Some(MeshId(QUAD_MESH)) {
                        (dev.fns.cmd_bind_index_buffer)(cmd, q.buffer.handle, q.index_offset, VK_INDEX_TYPE_UINT32);
                        bound_mesh = Some(MeshId(QUAD_MESH));
                    }
                    (dev.fns.cmd_draw_indexed)(cmd, 6, b.count, 0, 0, b.first);
                    stats.triangles_2d += 2 * b.count as u64;
                }
            }
            stats.draw_calls_2d += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    const IDENTITY: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

    fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
        let mut o = [0.0; 16];
        for c in 0..4 {
            for r in 0..4 {
                o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
            }
        }
        o
    }

    fn translate(x: f32, y: f32, z: f32) -> [f32; 16] {
        let mut m = IDENTITY;
        m[12] = x;
        m[13] = y;
        m[14] = z;
        m
    }

    /// Orthographic, reversed Z: 10 m tall view of the XY plane from z = 10, looking down −Z.
    fn ortho_2d(aspect: f32) -> [f32; 16] {
        let (h, near, far) = (10.0f32, 0.05f32, 100.0f32);
        let w = h * aspect;
        let d = far - near;
        let proj = [2.0 / w, 0.0, 0.0, 0.0, 0.0, -2.0 / h, 0.0, 0.0, 0.0, 0.0, 1.0 / d, 0.0, 0.0, 0.0, far / d, 1.0];
        mul(&proj, &translate(0.0, 0.0, -10.0))
    }

    fn perspective(z: f32) -> [f32; 16] {
        let near = 0.05f32;
        let proj = [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, near, 0.0];
        mul(&proj, &translate(0.0, 0.0, -z))
    }

    #[test]
    fn records_match_shader_layout() {
        assert_eq!(offset_of!(ItemRecord, v1), 16);
        assert_eq!(offset_of!(ItemRecord, v5), 80);
        assert_eq!(offset_of!(ItemRecord, info), 96);
        assert_eq!(size_of::<ItemRecord>(), 112);
        assert_eq!(offset_of!(SpritePush, pass), 16);
        assert_eq!(offset_of!(SpritePush, cam_pos), 32);
        assert_eq!(offset_of!(SpritePush, cam_forward), 80);
        assert_eq!(size_of::<SpritePush>(), 96);
    }

    #[test]
    fn view_basis_of_ortho_and_perspective() {
        let v = view_2d(&ortho_2d(1.6), true);
        assert!(v.ortho);
        let near = |a: [f32; 3], b: [f32; 3]| sub(a, b).iter().all(|d| d.abs() < 1e-4);
        assert!(near(v.forward, [0.0, 0.0, -1.0]), "{v:?}");
        assert!(near(v.right, [1.0, 0.0, 0.0]) && near(v.up, [0.0, 1.0, 0.0]), "{v:?}");
        assert!(near(v.position, [0.0, 0.0, 9.95]), "near plane centre: {v:?}");
        let p = view_2d(&perspective(5.0), true);
        assert!(!p.ortho);
        assert!(near(p.position, [0.0, 0.0, 5.0]) && near(p.forward, [0.0, 0.0, -1.0]), "{p:?}");
        assert!(near(p.up, [0.0, 1.0, 0.0]), "{p:?}");
        assert_eq!(view_2d(&[0.0; 16], true), View2d::default());
    }

    #[test]
    fn spheres_cover_every_corner_at_any_turn() {
        let s = Sprite { position: [1.0, 2.0, 3.0], size: [2.0, 4.0], pivot: [0.5, 0.0], ..Default::default() };
        let (c, r) = sprite_sphere(&s);
        assert_eq!(c, [1.0, 2.0, 3.0]);
        // The farthest corner is a top corner: (±1, 4) from the pivot.
        assert!((r - 17f32.sqrt()).abs() < 1e-6);
        let t = TileChunk { region: [0, 0, 4, 2], tile_size: [2.0, 2.0], ..TileChunk::new(TextureId(1), [8, 8], TextureId(2), [0; 4]) };
        let (c, r) = tiles_sphere(&t);
        assert_eq!(c, [4.0, -2.0, 0.0], "rows go down");
        assert!((r - 20f32.sqrt()).abs() < 1e-6);
    }

    fn sprite(x: f32, z: f32, layer: i32, blend: SpriteBlend) -> Draw2d {
        Draw2d::Sprite(Sprite { position: [x, 0.0, z], layer, blend, ..Default::default() })
    }

    #[test]
    fn plan_sorts_by_layer_then_far_to_near_and_batches_by_kind() {
        let cut = SpriteBlend::Cutout { threshold: 0.5 };
        let draws = [
            sprite(0.0, 0.0, 1, cut),                        // 0
            sprite(1.0, -3.0, 0, SpriteBlend::Blended),      // 1: layer 0, far
            sprite(2.0, 1.0, 0, SpriteBlend::Blended),       // 2: layer 0, near
            sprite(500.0, 0.0, 0, cut),                      // 3: off screen
            sprite(-1.0, 0.0, 1, cut),                       // 4: ties with 0: list order
            sprite(0.5, 0.0, -2, cut),                       // 5: lowest layer first
        ];
        let frame = FrameDesc { view_proj: ortho_2d(1.0), reverse_z: true, draws_2d: &draws, ..Default::default() };
        let mut plan = Plan2d::default();
        plan_2d(&frame, |_| None, &Textures::default(), &mut LinearCache::default(), false, &mut plan);
        let xs: Vec<f32> = plan.records.iter().map(|r| r.v1[0]).collect();
        assert_eq!(xs, [0.5, 1.0, 2.0, 0.0, -1.0]);
        assert_eq!((plan.culled, plan.sprites, plan.drawn), (1, 5, 5));
        let kinds: Vec<(BatchKind, u32, u32)> = plan.batches.iter().map(|b| (b.kind, b.first, b.count)).collect();
        assert_eq!(kinds, [(BatchKind::Cutout, 0, 1), (BatchKind::Blended, 1, 2), (BatchKind::Cutout, 3, 2)]);
        // Reused: no growth, same answer.
        let cap = plan.records.capacity();
        plan_2d(&frame, |_| None, &Textures::default(), &mut LinearCache::default(), false, &mut plan);
        assert_eq!((plan.records.len(), plan.records.capacity()), (5, cap));
    }

    #[test]
    fn shadow_casters_follow_the_drawn_records_even_off_screen() {
        let mut caster = Sprite { cast_shadows: true, blend: SpriteBlend::Cutout { threshold: 0.3 }, ..Default::default() };
        caster.position = [500.0, 0.0, 0.0];
        let blended = Sprite { cast_shadows: true, ..Default::default() };
        let draws = [Draw2d::Sprite(caster), Draw2d::Sprite(blended)];
        let frame = FrameDesc { view_proj: ortho_2d(1.0), reverse_z: true, draws_2d: &draws, ..Default::default() };
        let mut plan = Plan2d::default();
        plan_2d(&frame, |_| None, &Textures::default(), &mut LinearCache::default(), true, &mut plan);
        assert_eq!((plan.drawn, plan.casters), (1, 1), "only the cutout casts, even though it is off screen");
        assert_eq!(plan.records[1].v1[0], 500.0);
        assert_eq!(plan.records[1].v5[3], 0.3, "cutoff");
    }

    #[test]
    fn sprite_record_flags_and_sanitizing() {
        let s = Sprite {
            flip: [true, false],
            billboard: Billboard::Cylindrical,
            filter: TextureFilter::Nearest,
            pixel_snap: true,
            lit: true,
            blend: SpriteBlend::Cutout { threshold: f32::NAN },
            axes: [[0.0; 3], [0.0, 2.0, 0.0]],
            position: [f32::NAN, 1.0, 2.0],
            color: [1.0, 0.5, 0.0, 3.0],
            ..Default::default()
        };
        let r = sprite_record(&s, Some(7), &mut LinearCache::default());
        assert_eq!(r.info, [8, FLAG_BILLBOARD_CYLINDRICAL | FLAG_FLIP_X | FLAG_NEAREST | FLAG_SNAP | FLAG_LIT | FLAG_CUTOUT, 0, KIND_SPRITE]);
        assert_eq!(r.v4[..3], [1.0, 0.0, 0.0], "degenerate axis falls back");
        assert_eq!(r.v5[..4], [0.0, 1.0, 0.0, 0.5], "axis normalized; NaN cutoff becomes 0.5");
        assert_eq!(r.v1[0], 0.0, "non-finite position zeroed");
        assert_eq!(r.tint[3], 1.0);
        assert!((r.tint[1] - 0.214_041).abs() < 1e-5, "linear tint");
        let t = tiles_record(&TileChunk::new(TextureId(1), [16, 8], TextureId(2), [32, 64, 32, 32]), 3, 9, &mut LinearCache::default());
        assert_eq!(t.info, [4, FLAG_CUTOUT | FLAG_NEAREST, 10, KIND_TILES]);
        assert_eq!(t.v3, [32.0, 64.0, 32.0, 32.0]);
        assert_eq!(t.v2, [1.0, 1.0, 16.0, 8.0]);
    }

    #[test]
    fn unknown_ids_are_structured_errors() {
        let draws = [Draw2d::Sprite(Sprite { texture: Some(TextureId(42)), ..Default::default() })];
        let err = check_2d(&draws, &HashMap::new(), &Textures::default()).unwrap_err();
        assert_eq!(err.reason, Some(crate::ErrorReason::UnknownId { kind: IdKind::Texture, id: 42 }));
        let draws = [Draw2d::Shape(ShapeDraw { mesh: MeshId(5), model: IDENTITY, color: [1.0; 4], layer: 0 })];
        let err = check_2d(&draws, &HashMap::new(), &Textures::default()).unwrap_err();
        assert_eq!(err.reason, Some(crate::ErrorReason::UnknownId { kind: IdKind::Mesh, id: 5 }));
    }

    /// Timing report: `cargo test --release -p specula-gpu --lib plan_cost -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing report"]
    fn plan_cost_of_50k_sprites() {
        let mut x = 1u32;
        let mut r = || {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 8) as f32 / 16_777_216.0
        };
        let draws: Vec<Draw2d> = (0..50_000)
            .map(|i| {
                Draw2d::Sprite(Sprite {
                    position: [r() * 60.0 - 30.0, r() * 34.0 - 17.0, 0.0],
                    size: [0.25, 0.25],
                    rotation: r() * 6.0,
                    layer: i % 2,
                    ..Default::default()
                })
            })
            .collect();
        let frame = FrameDesc { view_proj: ortho_2d(1.6), reverse_z: true, draws_2d: &draws, ..Default::default() };
        let (mut plan, mut cache) = (Plan2d::default(), LinearCache::default());
        for _ in 0..3 {
            plan_2d(&frame, |_| None, &Textures::default(), &mut cache, false, &mut plan);
        }
        let t = std::time::Instant::now();
        for _ in 0..20 {
            plan_2d(&frame, |_| None, &Textures::default(), &mut cache, false, &mut plan);
        }
        eprintln!("plan_2d, 50k sprites: {:.3} ms", t.elapsed().as_secs_f64() * 1e3 / 20.0);
    }

    #[test]
    fn order_bits_sort_like_floats() {
        let mut v = [3.0f32, -1.0, 0.0, -0.0, f32::INFINITY, -7.5, 1e-30];
        let mut by_bits = v;
        by_bits.sort_by_key(|f| order_bits(*f));
        v.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(by_bits.map(f32::to_bits)[..], v.map(f32::to_bits)[..]);
    }
}

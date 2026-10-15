//! 2D entities: sprites, tile maps and vector shapes (shapes `sprite`, `tilemap`, `shape2d`).
//!
//! # Conventions
//!
//! 2D lives in each entity's local **XY plane** (metres, x right, **y up**, facing +Z), placed by
//! the entity's transform like any other entity: a standalone 2D game looks down −Z at the world
//! XY plane with an orthographic camera (`sight_frame {"view_2d": …}`), a ground decal or a map
//! on the floor is the same entity turned −90° about X. Entity scale multiplies sizes; the
//! entity colour and `opacity` tint everything.
//!
//! - **Sprite**: a quad of `size` metres with its `pivot` (0..1, (0, 0) = bottom left) at the
//!   entity's origin, showing a PNG from the assets root (`texture`), optionally one frame of a
//!   grid sheet (`sheet` + `frame`) or a flipbook animation (`anim`) in step with the frame's
//!   time. Billboards turn to the camera (`spherical`) or about their up axis (`cylindrical`).
//! - **Tile map**: `size` = columns × rows of `tile_size` metres; the entity origin is the
//!   **top-left** corner and rows go down (−Y local), like the rows of its text. Several layers
//!   share the grid; each is saved as rows of run-length tokens (see [`parse_row`]). Tiles come
//!   from a tileset PNG cut into `tile_px` cells (numbered from 1, left to right, top to bottom;
//!   0 = empty). Edits go through `tilemap_set`, cheaply: a chunked grid shares untouched chunks
//!   between versions, and the GPU re-uploads only the changed chunks.
//! - **Vector shape**: `items` of rects (rounded), circles, ellipses, polygons, polylines and
//!   paths (lines, quadratic and cubic Béziers, arcs), each filled and/or stroked, tessellated
//!   deterministically with anti-aliased edges.
//!
//! Sort order: `z_layer` (higher draws over lower), then far to near (see `specula_gpu`'s sprite
//! docs); coplanar items draw over each other in that order.

use crate::world::{color_from_hex, color_hex, WorldError};
use specula_core::hash::StableHasher;
use specula_core::math::{Mat4, Vec3};
use specula_core::{png, Json};
use specula_gpu::vector::{self, LineCap, LineJoin, Path, PathCmd, ShapeMesh, StrokeStyle};
use specula_gpu::{
    Billboard, Draw2d, Flipbook, Gpu, MeshId, Playback, ShapeDraw, Sprite, SpriteBlend, TextureFilter, TextureFormat, TextureId,
    TileCell, TileChunk, TileGrid, ORIENT_FLIP_X, ORIENT_FLIP_Y, ORIENT_SWAP_XY,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

/// Largest tile map, cells per side, and most layers.
pub const MAX_TILEMAP_SIDE: u32 = 1024;
pub const MAX_TILE_LAYERS: usize = 8;
/// Most tileset animations.
pub const MAX_TILE_ANIMATIONS: usize = 256;
/// Most cells one `tilemap_set` call writes.
pub const MAX_TILE_EDITS: usize = 65_536;
/// Most cells one `tilemap_get` call returns.
pub const MAX_TILE_READ: u32 = 4096;
/// Most items of one shape2d.
pub const MAX_SHAPE_ITEMS: usize = 256;
/// Most raw points or path commands of one shape2d (before flattening).
pub const MAX_SHAPE_INPUT: usize = 16_384;
/// Longest asset path.
pub const MAX_ASSET_PATH: usize = 256;

/// A 2D entity's component (present exactly for shapes `sprite`, `tilemap`, `shape2d`).
#[derive(Clone, Debug, PartialEq)]
pub enum TwoD {
    Sprite(Box<SpriteSpec>),
    Tilemap(Arc<TilemapSpec>),
    Shape2d(Arc<Shape2dSpec>),
}

/// A grid sheet: frames of `frame_px`, left to right then top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sheet {
    pub frame_px: [u32; 2],
    pub margin: u32,
    pub spacing: u32,
}

/// A flipbook over sheet frames `first .. first + count`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anim {
    pub first: u32,
    pub count: u32,
    pub fps: f32,
    pub playback: Playback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpriteSpec {
    /// PNG path under the assets root; `None` = a flat colour quad.
    pub texture: Option<String>,
    pub size: [f32; 2],
    pub pivot: [f32; 2],
    /// An explicit texture rect (u0, v0, u1, v1); otherwise the sheet frame or the whole image.
    pub uv: Option<[f32; 4]>,
    pub sheet: Option<Sheet>,
    /// The still frame of the sheet.
    pub frame: u32,
    pub anim: Option<Anim>,
    pub flip: [bool; 2],
    pub billboard: Billboard,
    pub blend: SpriteBlend,
    pub filter: TextureFilter,
    pub pixel_snap: bool,
    pub lit: bool,
    pub cast_shadows: bool,
    pub z_layer: i32,
}

impl Default for SpriteSpec {
    fn default() -> Self {
        SpriteSpec {
            texture: None,
            size: [1.0, 1.0],
            pivot: [0.5, 0.5],
            uv: None,
            sheet: None,
            frame: 0,
            anim: None,
            flip: [false; 2],
            billboard: Billboard::None,
            blend: SpriteBlend::Blended,
            filter: TextureFilter::Linear,
            pixel_snap: false,
            lit: false,
            cast_shadows: false,
            z_layer: 0,
        }
    }
}

/// A tileset animation: cells showing `tile` cycle through `frames` consecutive tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileAnim {
    pub tile: u16,
    pub frames: u8,
    pub frame_ms: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TileLayer {
    pub name: String,
    pub cells: TileGrid,
    pub z_layer: i32,
    pub blend: SpriteBlend,
    pub opacity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TilemapSpec {
    pub tileset: String,
    pub tile_px: [u32; 2],
    pub margin: u32,
    pub spacing: u32,
    pub tile_size: [f32; 2],
    pub size: [u32; 2],
    pub filter: TextureFilter,
    pub pixel_snap: bool,
    pub lit: bool,
    pub animations: Vec<TileAnim>,
    pub layers: Vec<TileLayer>,
}

/// A vector shape: its canonical description, what it costs, where it is, and its tessellation
/// (made once, shared by every entity with an identical shape; deferred while world files load,
/// see [`deferred`]).
///
/// Everything but the tessellation is worked out from the description alone when it is parsed,
/// so it is the same on every machine whatever has been drawn: the vertex bound a world charges
/// against [`MAX_WORLD_SHAPE_VERTICES`] and the bounds that queries, sight and culling use.
#[derive(Debug)]
pub struct Shape2dSpec {
    /// The canonical description (what is saved and hashed).
    pub json: Json,
    pub z_layer: i32,
    /// Hash of the canonical JSON. Not unique on its own: shapes are the same only when their
    /// JSON is ([`Shape2dSpec::same`]).
    pub key: u64,
    /// Upper bound of the tessellation's vertices (at most [`vector::MAX_SHAPE_VERTICES`]).
    pub vertices: usize,
    items: Vec<ShapeItem>,
    tolerance: f32,
    aa: bool,
    /// Local bounds from the control points (arcs as centre ± radii) widened by each stroke's
    /// reach. Every tessellated vertex lies inside (the one-pixel AA fringe aside).
    bounds: (Vec3, Vec3),
    /// The tessellation (local XY plane), or why the geometry can't be tessellated.
    mesh: std::sync::OnceLock<Result<Arc<ShapeMesh>, String>>,
}

impl PartialEq for Shape2dSpec {
    fn eq(&self, other: &Shape2dSpec) -> bool {
        self.key == other.key && self.json == other.json
    }
}

/// Cells written by one `tilemap_set`: layer (by name) and (x, y, cell).
#[derive(Clone, Debug, PartialEq)]
pub struct TileEdits {
    pub layer: String,
    pub cells: Vec<(u32, u32, TileCell)>,
}

// ---------------------------------------------------------------------------------------------
// Argument helpers (errors are messages naming the field, as tool arguments report them)
// ---------------------------------------------------------------------------------------------

fn keys(v: &Json, what: &str, allowed: &[&str]) -> Result<(), String> {
    match v {
        Json::Obj(pairs) => match pairs.iter().find(|(k, _)| !allowed.contains(&k.as_str())) {
            Some((k, _)) => Err(format!("'{k}' is not a field of {what} (fields: {})", allowed.join(", "))),
            None => Ok(()),
        },
        _ => Err(format!("{what} must be a JSON object")),
    }
}

fn opt<'a>(v: &'a Json, key: &str) -> Option<&'a Json> {
    v.get(key).filter(|j| !j.is_null())
}

fn num(v: &Json, key: &str, what: &str, lo: f64, hi: f64) -> Result<Option<f32>, String> {
    match opt(v, key) {
        None => Ok(None),
        Some(j) => match j.as_f64() {
            Some(x) if x.is_finite() && (lo..=hi).contains(&x) => Ok(Some(x as f32)),
            _ => Err(format!("'{what}.{key}' must be a number from {lo} to {hi}")),
        },
    }
}

fn int(v: &Json, key: &str, what: &str, lo: i64, hi: i64) -> Result<Option<i64>, String> {
    match opt(v, key) {
        None => Ok(None),
        Some(j) => match j.as_i64() {
            Some(x) if (lo..=hi).contains(&x) => Ok(Some(x)),
            _ => Err(format!("'{what}.{key}' must be an integer from {lo} to {hi}")),
        },
    }
}

fn boolean(v: &Json, key: &str, what: &str) -> Result<Option<bool>, String> {
    match opt(v, key) {
        None => Ok(None),
        Some(j) => j.as_bool().map(Some).ok_or_else(|| format!("'{what}.{key}' must be true or false")),
    }
}

fn nums<const N: usize>(v: &Json, key: &str, what: &str, lo: f64, hi: f64) -> Result<Option<[f32; N]>, String> {
    match opt(v, key) {
        None => Ok(None),
        Some(j) => {
            let err = || format!("'{what}.{key}' must be {N} numbers from {lo} to {hi}");
            let a = j.as_array().ok_or_else(err)?;
            if a.len() != N {
                return Err(err());
            }
            let mut out = [0.0f32; N];
            for (o, x) in out.iter_mut().zip(a) {
                match x.as_f64() {
                    Some(x) if x.is_finite() && (lo..=hi).contains(&x) => *o = x as f32,
                    _ => return Err(err()),
                }
            }
            Ok(Some(out))
        }
    }
}

fn ints<const N: usize>(v: &Json, key: &str, what: &str, lo: u64, hi: u64) -> Result<Option<[u32; N]>, String> {
    match opt(v, key) {
        None => Ok(None),
        Some(j) => {
            let err = || format!("'{what}.{key}' must be {N} integers from {lo} to {hi}");
            let a = j.as_array().ok_or_else(err)?;
            if a.len() != N {
                return Err(err());
            }
            let mut out = [0u32; N];
            for (o, x) in out.iter_mut().zip(a) {
                match x.as_u64() {
                    Some(x) if (lo..=hi).contains(&x) => *o = x as u32,
                    _ => return Err(err()),
                }
            }
            Ok(Some(out))
        }
    }
}

fn choice<T: Copy>(v: &Json, key: &str, what: &str, options: &[(&str, T)]) -> Result<Option<T>, String> {
    match opt(v, key) {
        None => Ok(None),
        Some(j) => {
            let names: Vec<&str> = options.iter().map(|o| o.0).collect();
            let s = j.as_str().ok_or_else(|| format!("'{what}.{key}' must be one of {}", names.join(", ")))?;
            options
                .iter()
                .find(|o| o.0 == s)
                .map(|o| Some(o.1))
                .ok_or_else(|| format!("'{what}.{key}' must be one of {}, not '{s}'", names.join(", ")))
        }
    }
}

/// A PNG path relative to the assets root (same rules as recipe paths, ending in `.png`).
pub fn check_image_path(field: &str, path: &str) -> Result<(), String> {
    let ok = !path.is_empty()
        && path.len() <= MAX_ASSET_PATH
        && path.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/'))
        && path.split('/').all(|seg| !seg.is_empty() && !seg.starts_with('.') && !seg.ends_with('.'))
        && path.ends_with(".png");
    if ok {
        Ok(())
    } else {
        Err(format!(
            "'{field}' must be a .png path relative to the assets root, like \"sprites/hero.png\" (A-Z a-z 0-9 _ - . /, \
             no '..', at most {MAX_ASSET_PATH} bytes); got {path:?}"
        ))
    }
}

const BILLBOARDS: [(&str, Billboard); 3] =
    [("none", Billboard::None), ("spherical", Billboard::Spherical), ("cylindrical", Billboard::Cylindrical)];
const FILTERS: [(&str, TextureFilter); 2] = [("linear", TextureFilter::Linear), ("nearest", TextureFilter::Nearest)];
const PLAYBACKS: [(&str, Playback); 3] = [("loop", Playback::Loop), ("once", Playback::Once), ("pingpong", Playback::PingPong)];

fn blend_of(v: &Json, what: &str, default: SpriteBlend) -> Result<SpriteBlend, String> {
    let cutoff = num(v, "cutoff", what, 0.0, 1.0)?;
    match choice(v, "blend", what, &[("blended", false), ("cutout", true)])? {
        Some(true) => Ok(SpriteBlend::Cutout { threshold: cutoff.unwrap_or(0.5) }),
        Some(false) if cutoff.is_some() => Err(format!("'{what}.cutoff' only applies to blend \"cutout\"")),
        Some(false) => Ok(SpriteBlend::Blended),
        None => Ok(match (default, cutoff) {
            (SpriteBlend::Cutout { .. }, Some(t)) => SpriteBlend::Cutout { threshold: t },
            (d, None) => d,
            (SpriteBlend::Blended, Some(_)) => return Err(format!("'{what}.cutoff' only applies to blend \"cutout\"")),
        }),
    }
}

fn blend_fields(b: SpriteBlend, default: SpriteBlend, f: &mut Vec<(&'static str, Json)>) {
    if b == default {
        return;
    }
    match b {
        SpriteBlend::Cutout { threshold } => {
            f.push(("blend", Json::from("cutout")));
            f.push(("cutoff", Json::from(threshold)));
        }
        SpriteBlend::Blended => f.push(("blend", Json::from("blended"))),
    }
}

fn name_of<T: PartialEq + Copy>(options: &[(&'static str, T)], v: T) -> &'static str {
    options.iter().find(|o| o.1 == v).map_or("?", |o| o.0)
}

// ---------------------------------------------------------------------------------------------
// Sprites
// ---------------------------------------------------------------------------------------------

const SPRITE_KEYS: &[&str] = &[
    "texture", "size", "pivot", "uv", "sheet", "frame", "anim", "flip", "billboard", "blend", "cutoff", "filter", "pixel_snap",
    "lit", "cast_shadows", "z_layer",
];

impl SpriteSpec {
    pub fn parse(v: &Json) -> Result<SpriteSpec, String> {
        let w = "sprite";
        keys(v, w, SPRITE_KEYS)?;
        let d = SpriteSpec::default();
        let texture = match opt(v, "texture") {
            None => None,
            Some(t) => {
                let s = t.as_str().ok_or("'sprite.texture' must be a .png path")?;
                check_image_path("sprite.texture", s)?;
                Some(s.to_owned())
            }
        };
        let sheet = match opt(v, "sheet") {
            None => None,
            Some(s) => {
                keys(s, "sprite.sheet", &["frame_px", "margin", "spacing"])?;
                let frame_px = ints::<2>(s, "frame_px", "sprite.sheet", 1, 8192)?.ok_or("'sprite.sheet.frame_px' is required")?;
                Some(Sheet {
                    frame_px,
                    margin: int(s, "margin", "sprite.sheet", 0, 4096)?.unwrap_or(0) as u32,
                    spacing: int(s, "spacing", "sprite.sheet", 0, 4096)?.unwrap_or(0) as u32,
                })
            }
        };
        let anim = match opt(v, "anim") {
            None => None,
            Some(a) => {
                keys(a, "sprite.anim", &["first", "count", "fps", "playback"])?;
                Some(Anim {
                    first: int(a, "first", "sprite.anim", 0, 65_535)?.unwrap_or(0) as u32,
                    count: int(a, "count", "sprite.anim", 1, 4096)?.ok_or("'sprite.anim.count' is required")? as u32,
                    fps: num(a, "fps", "sprite.anim", 0.0, 1000.0)?.unwrap_or(8.0),
                    playback: choice(a, "playback", "sprite.anim", &PLAYBACKS)?.unwrap_or_default(),
                })
            }
        };
        let frame = int(v, "frame", w, 0, 65_535)?.map(|f| f as u32);
        if (frame.is_some() || anim.is_some()) && sheet.is_none() {
            return Err("'sprite.frame' and 'sprite.anim' need a 'sprite.sheet' (the frame grid)".into());
        }
        let uv = nums::<4>(v, "uv", w, -16.0, 16.0)?;
        if uv.is_some() && sheet.is_some() {
            return Err("give 'sprite.uv' or 'sprite.sheet', not both".into());
        }
        if (sheet.is_some() || uv.is_some()) && texture.is_none() {
            return Err("'sprite.sheet' and 'sprite.uv' need a 'sprite.texture'".into());
        }
        let flip = match opt(v, "flip") {
            None => d.flip,
            Some(f) => match f.as_array().map(|a| a.iter().map(Json::as_bool).collect::<Option<Vec<_>>>()) {
                Some(Some(b)) if b.len() == 2 => [b[0], b[1]],
                _ => return Err("'sprite.flip' must be [flip_x, flip_y] (true/false)".into()),
            },
        };
        let blend = blend_of(v, w, d.blend)?;
        let cast_shadows = boolean(v, "cast_shadows", w)?.unwrap_or(false);
        if cast_shadows && !matches!(blend, SpriteBlend::Cutout { .. }) {
            return Err("'sprite.cast_shadows' needs blend \"cutout\" (blended sprites cast no shadow)".into());
        }
        Ok(SpriteSpec {
            texture,
            size: nums::<2>(v, "size", w, 1e-4, 1e5)?.unwrap_or(d.size),
            pivot: nums::<2>(v, "pivot", w, -16.0, 16.0)?.unwrap_or(d.pivot),
            uv,
            sheet,
            frame: frame.unwrap_or(0),
            anim,
            flip,
            billboard: choice(v, "billboard", w, &BILLBOARDS)?.unwrap_or_default(),
            blend,
            filter: choice(v, "filter", w, &FILTERS)?.unwrap_or_default(),
            pixel_snap: boolean(v, "pixel_snap", w)?.unwrap_or(false),
            lit: boolean(v, "lit", w)?.unwrap_or(false),
            cast_shadows,
            z_layer: int(v, "z_layer", w, -1_000_000, 1_000_000)?.unwrap_or(0) as i32,
        })
    }

    /// Canonical JSON (defaults left out); `parse` reads it back to an equal spec.
    pub fn to_json(&self) -> Json {
        let d = SpriteSpec::default();
        let mut f: Vec<(&'static str, Json)> = Vec::new();
        if let Some(t) = &self.texture {
            f.push(("texture", Json::from(t.as_str())));
        }
        let pair = |a: [f32; 2]| Json::arr(a.map(Json::from));
        if self.size != d.size {
            f.push(("size", pair(self.size)));
        }
        if self.pivot != d.pivot {
            f.push(("pivot", pair(self.pivot)));
        }
        if let Some(uv) = self.uv {
            f.push(("uv", Json::arr(uv.map(Json::from))));
        }
        if let Some(s) = self.sheet {
            let mut sf = vec![("frame_px", Json::arr(s.frame_px.map(Json::from)))];
            if s.margin != 0 {
                sf.push(("margin", Json::from(s.margin)));
            }
            if s.spacing != 0 {
                sf.push(("spacing", Json::from(s.spacing)));
            }
            f.push(("sheet", Json::obj(sf)));
            if self.frame != 0 {
                f.push(("frame", Json::from(self.frame)));
            }
        }
        if let Some(a) = self.anim {
            f.push((
                "anim",
                Json::obj([
                    ("first", Json::from(a.first)),
                    ("count", Json::from(a.count)),
                    ("fps", Json::from(a.fps)),
                    ("playback", Json::from(name_of(&PLAYBACKS, a.playback))),
                ]),
            ));
        }
        if self.flip != d.flip {
            f.push(("flip", Json::arr(self.flip.map(Json::from))));
        }
        if self.billboard != d.billboard {
            f.push(("billboard", Json::from(name_of(&BILLBOARDS, self.billboard))));
        }
        blend_fields(self.blend, d.blend, &mut f);
        if self.filter != d.filter {
            f.push(("filter", Json::from(name_of(&FILTERS, self.filter))));
        }
        for (k, v) in [("pixel_snap", self.pixel_snap), ("lit", self.lit), ("cast_shadows", self.cast_shadows)] {
            if v {
                f.push((k, Json::from(true)));
            }
        }
        if self.z_layer != 0 {
            f.push(("z_layer", Json::from(self.z_layer as i64)));
        }
        Json::obj(f)
    }

    /// Local bounds: the quad in the XY plane, or (billboards, which turn) a cube around the
    /// pivot that holds the quad at any turn.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let [w, h] = self.size;
        let [px, py] = self.pivot;
        if self.billboard == Billboard::None {
            return (Vec3::new(-px * w, -py * h, 0.0), Vec3::new((1.0 - px) * w, (1.0 - py) * h, 0.0));
        }
        let (_, r) = specula_gpu::sprite_sphere(&Sprite { size: self.size, pivot: self.pivot, ..Default::default() });
        (Vec3::new(-r, -r, -r), Vec3::new(r, r, r))
    }
}

// ---------------------------------------------------------------------------------------------
// Tile maps
// ---------------------------------------------------------------------------------------------

/// One cell as text: `0` empty; a tile number, then any of `h` (mirror left-right), `v` (mirror
/// top-bottom), `d` (swap x and y, applied first); e.g. `12`, `7h`, `3dv`. Rotations: `r1`..`r3`
/// (quarter turns clockwise) may follow instead of letters, e.g. `5r1`.
pub fn parse_cell(token: &str) -> Result<TileCell, String> {
    let digits = token.bytes().take_while(u8::is_ascii_digit).count();
    let tile: u32 = token[..digits].parse().map_err(|_| format!("cell {token:?}: it must start with a tile number"))?;
    if tile > u16::MAX as u32 {
        return Err(format!("cell {token:?}: tile numbers go up to {}", u16::MAX));
    }
    let mut cell = TileCell::new(tile as u16);
    let rest = &token[digits..];
    if tile == 0 && !rest.is_empty() {
        return Err(format!("cell {token:?}: an empty cell (0) has no orientation"));
    }
    if let Some(r) = rest.strip_prefix('r') {
        let q: u32 = r.parse().ok().filter(|q| (1..=3).contains(q)).ok_or_else(|| format!("cell {token:?}: rotation is r1, r2 or r3"))?;
        return Ok(cell.rotated_cw(q));
    }
    for c in rest.chars() {
        let bit = match c {
            'h' => ORIENT_FLIP_X,
            'v' => ORIENT_FLIP_Y,
            'd' => ORIENT_SWAP_XY,
            _ => return Err(format!("cell {token:?}: after the tile number only h, v, d or r1..r3")),
        };
        if cell.orient & bit != 0 {
            return Err(format!("cell {token:?}: '{c}' given twice"));
        }
        cell.orient |= bit;
    }
    Ok(cell)
}

/// The text of a cell (tile and orientation; animation comes from the tileset).
pub fn format_cell(c: TileCell) -> String {
    if c.tile == 0 {
        return "0".into();
    }
    let mut s = c.tile.to_string();
    for (bit, ch) in [(ORIENT_SWAP_XY, 'd'), (ORIENT_FLIP_X, 'h'), (ORIENT_FLIP_Y, 'v')] {
        if c.orient & bit != 0 {
            s.push(ch);
        }
    }
    s
}

/// A row: comma-separated cell tokens, each optionally `*count` (run length), e.g.
/// `"1*10,0*3,7h,7h"`. Shorter rows end with empty cells; `""` is an all-empty row.
pub fn parse_row(row: &str, width: u32) -> Result<Vec<TileCell>, String> {
    let mut out = Vec::new();
    if row.trim().is_empty() {
        return Ok(out);
    }
    for part in row.split(',') {
        let part = part.trim();
        let (tok, count) = match part.split_once('*') {
            None => (part, 1u32),
            Some((t, n)) => (t, n.parse().ok().filter(|n| *n >= 1).ok_or_else(|| format!("run {part:?}: count must be a whole number >= 1"))?),
        };
        let cell = parse_cell(tok)?;
        if out.len() as u64 + count as u64 > width as u64 {
            return Err(format!("the row has more than {width} cells"));
        }
        out.extend(std::iter::repeat_n(cell, count as usize));
    }
    Ok(out)
}

/// Row `y` of `grid` as run-length text (trailing empty cells left out).
pub fn format_row(grid: &TileGrid, y: u32, x0: u32, width: u32) -> String {
    let cells: Vec<TileCell> = (x0..(x0 + width).min(grid.width())).map(|x| grid.get(x, y)).collect();
    let end = cells.iter().rposition(|c| !c.is_empty()).map_or(0, |i| i + 1);
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < end {
        let key = |c: &TileCell| (c.tile, c.orient);
        let mut j = i + 1;
        while j < end && key(&cells[j]) == key(&cells[i]) {
            j += 1;
        }
        let tok = format_cell(cells[i]);
        parts.push(if j - i > 1 { format!("{tok}*{}", j - i) } else { tok });
        i = j;
    }
    parts.join(",")
}

const TILEMAP_KEYS: &[&str] =
    &["tileset", "tile_px", "margin", "spacing", "tile_size", "size", "filter", "pixel_snap", "lit", "animations", "layers"];
const LAYER_KEYS: &[&str] = &["name", "z_layer", "blend", "cutoff", "opacity", "rows"];
const TILE_LAYER_DEFAULT_BLEND: SpriteBlend = SpriteBlend::Cutout { threshold: 0.5 };

impl TilemapSpec {
    pub fn parse(v: &Json) -> Result<TilemapSpec, String> {
        let w = "tilemap";
        keys(v, w, TILEMAP_KEYS)?;
        let tileset = opt(v, "tileset").and_then(Json::as_str).ok_or("'tilemap.tileset' (a .png path) is required")?.to_owned();
        check_image_path("tilemap.tileset", &tileset)?;
        let tile_px = ints::<2>(v, "tile_px", w, 1, 4096)?.ok_or("'tilemap.tile_px' ([width, height] in tileset pixels) is required")?;
        let size = ints::<2>(v, "size", w, 1, MAX_TILEMAP_SIDE as u64)?.ok_or("'tilemap.size' ([columns, rows]) is required")?;
        let mut animations = Vec::new();
        if let Some(a) = opt(v, "animations") {
            let list = a.as_array().ok_or("'tilemap.animations' must be a list of {tile, frames, frame_ms}")?;
            if list.len() > MAX_TILE_ANIMATIONS {
                return Err(format!("'tilemap.animations': at most {MAX_TILE_ANIMATIONS}"));
            }
            for item in list {
                keys(item, "a tilemap animation", &["tile", "frames", "frame_ms"])?;
                let anim = TileAnim {
                    tile: int(item, "tile", "tilemap.animations", 1, u16::MAX as i64)?.ok_or("an animation needs 'tile'")? as u16,
                    frames: int(item, "frames", "tilemap.animations", 2, specula_gpu::MAX_TILE_FRAMES as i64)?
                        .ok_or("an animation needs 'frames'")? as u8,
                    frame_ms: int(item, "frame_ms", "tilemap.animations", 17, 4250)?.unwrap_or(250) as u32,
                };
                if anim.tile as u32 + anim.frames as u32 - 1 > u16::MAX as u32 {
                    return Err(format!(
                        "'tilemap.animations': tile {} with {} frames runs past tile {}",
                        anim.tile,
                        anim.frames,
                        u16::MAX
                    ));
                }
                if animations.iter().any(|o: &TileAnim| o.tile == anim.tile) {
                    return Err(format!("'tilemap.animations': tile {} animated twice", anim.tile));
                }
                animations.push(anim);
            }
            animations.sort_by_key(|a| a.tile);
        }
        let layers_json = opt(v, "layers").and_then(Json::as_array).ok_or("'tilemap.layers' (a list of layers) is required")?;
        if layers_json.is_empty() || layers_json.len() > MAX_TILE_LAYERS {
            return Err(format!("'tilemap.layers' must hold 1..{MAX_TILE_LAYERS} layers"));
        }
        let mut spec = TilemapSpec {
            tileset,
            tile_px,
            margin: int(v, "margin", w, 0, 4096)?.unwrap_or(0) as u32,
            spacing: int(v, "spacing", w, 0, 4096)?.unwrap_or(0) as u32,
            tile_size: nums::<2>(v, "tile_size", w, 1e-4, 1e5)?.unwrap_or([1.0, 1.0]),
            size,
            filter: choice(v, "filter", w, &FILTERS)?.unwrap_or(TextureFilter::Nearest),
            pixel_snap: boolean(v, "pixel_snap", w)?.unwrap_or(false),
            lit: boolean(v, "lit", w)?.unwrap_or(false),
            animations,
            layers: Vec::new(),
        };
        let mut names = BTreeSet::new();
        for (i, l) in layers_json.iter().enumerate() {
            let what = "a tilemap layer";
            keys(l, what, LAYER_KEYS)?;
            let name = match opt(l, "name") {
                None => format!("layer{i}"),
                Some(n) => n.as_str().filter(|n| crate::world::validate_name(n).is_ok()).ok_or("a layer 'name' must be a valid entity-style name")?.to_owned(),
            };
            if !names.insert(name.clone()) {
                return Err(format!("tilemap layer name '{name}' is used twice"));
            }
            let mut cells = TileGrid::new(size[0], size[1]);
            if let Some(rows) = opt(l, "rows") {
                let rows = rows.as_array().ok_or("a layer's 'rows' must be a list of strings")?;
                if rows.len() > size[1] as usize {
                    return Err(format!("layer '{name}' has {} rows; the map has {}", rows.len(), size[1]));
                }
                for (y, row) in rows.iter().enumerate() {
                    let text = row.as_str().ok_or("a layer's 'rows' must be strings")?;
                    let parsed = parse_row(text, size[0]).map_err(|e| format!("layer '{name}' row {y}: {e}"))?;
                    for (x, c) in parsed.into_iter().enumerate() {
                        cells.set(x as u32, y as u32, spec.animate(c));
                    }
                }
            }
            spec.layers.push(TileLayer {
                name,
                cells,
                z_layer: int(l, "z_layer", what, -1_000_000, 1_000_000)?.unwrap_or(i as i64) as i32,
                blend: blend_of(l, what, TILE_LAYER_DEFAULT_BLEND)?,
                opacity: num(l, "opacity", what, 0.0, 1.0)?.unwrap_or(1.0),
            });
        }
        Ok(spec)
    }

    /// `cell` with the tileset's animation for its tile (or still).
    pub fn animate(&self, cell: TileCell) -> TileCell {
        let base = TileCell { tile: cell.tile, orient: cell.orient, frames: 1, frame_ticks: 0 };
        match self.animations.binary_search_by_key(&cell.tile, |a| a.tile) {
            Ok(i) if cell.tile != 0 => {
                let a = self.animations[i];
                TileCell { frames: a.frames, frame_ticks: ((a.frame_ms as f64 * 0.06).round() as u32).clamp(1, 255) as u8, ..base }
            }
            _ => base,
        }
    }

    pub fn layer_index(&self, name: &str) -> Option<usize> {
        self.layers.iter().position(|l| l.name == name)
    }

    /// Canonical JSON with every cell (world files); `parse` reads it back equal.
    pub fn to_json(&self) -> Json {
        self.json(true)
    }

    /// A bounded summary for listings: the layout and per-layer counts, no cells.
    pub fn summary_json(&self) -> Json {
        self.json(false)
    }

    fn json(&self, cells: bool) -> Json {
        let mut f: Vec<(&'static str, Json)> = vec![
            ("tileset", Json::from(self.tileset.as_str())),
            ("tile_px", Json::arr(self.tile_px.map(Json::from))),
        ];
        if self.margin != 0 {
            f.push(("margin", Json::from(self.margin)));
        }
        if self.spacing != 0 {
            f.push(("spacing", Json::from(self.spacing)));
        }
        if self.tile_size != [1.0, 1.0] {
            f.push(("tile_size", Json::arr(self.tile_size.map(Json::from))));
        }
        f.push(("size", Json::arr(self.size.map(Json::from))));
        if self.filter != TextureFilter::Nearest {
            f.push(("filter", Json::from(name_of(&FILTERS, self.filter))));
        }
        for (k, v) in [("pixel_snap", self.pixel_snap), ("lit", self.lit)] {
            if v {
                f.push((k, Json::from(true)));
            }
        }
        if !self.animations.is_empty() {
            f.push((
                "animations",
                Json::arr(self.animations.iter().map(|a| {
                    Json::obj([("tile", Json::from(a.tile as u32)), ("frames", Json::from(a.frames as u32)), ("frame_ms", Json::from(a.frame_ms))])
                })),
            ));
        }
        let layers = self.layers.iter().enumerate().map(|(i, l)| {
            let mut lf: Vec<(&'static str, Json)> = vec![("name", Json::from(l.name.as_str()))];
            if l.z_layer != i as i32 {
                lf.push(("z_layer", Json::from(l.z_layer as i64)));
            }
            blend_fields(l.blend, TILE_LAYER_DEFAULT_BLEND, &mut lf);
            if l.opacity != 1.0 {
                lf.push(("opacity", Json::from(l.opacity)));
            }
            if cells {
                let rows: Vec<String> = (0..l.cells.height()).map(|y| format_row(&l.cells, y, 0, l.cells.width())).collect();
                let end = rows.iter().rposition(|r| !r.is_empty()).map_or(0, |i| i + 1);
                lf.push(("rows", Json::arr(rows[..end].iter().map(|r| Json::from(r.as_str())))));
            } else {
                lf.push(("filled", Json::from(l.cells.filled())));
            }
            Json::obj(lf)
        });
        f.push(("layers", Json::arr(layers)));
        Json::obj(f)
    }

    /// Local bounds: the whole grid (top-left at the origin, rows down −Y).
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let (w, h) = (self.size[0] as f32 * self.tile_size[0], self.size[1] as f32 * self.tile_size[1]);
        (Vec3::new(0.0, -h, 0.0), Vec3::new(w, 0.0, 0.0))
    }

    /// Applies edits; returns how many cells changed. Errors before changing anything.
    pub fn apply(&mut self, edits: &TileEdits) -> Result<usize, String> {
        let li = self.layer_index(&edits.layer).ok_or_else(|| {
            let names: Vec<&str> = self.layers.iter().map(|l| l.name.as_str()).collect();
            format!("no layer '{}' (layers: {})", edits.layer, names.join(", "))
        })?;
        let [w, h] = self.size;
        if let Some((x, y, _)) = edits.cells.iter().find(|(x, y, _)| *x >= w || *y >= h) {
            return Err(format!("cell ({x}, {y}) is outside the {w} x {h} map"));
        }
        let cells: Vec<(u32, u32, TileCell)> = edits.cells.iter().map(|&(x, y, c)| (x, y, self.animate(c))).collect();
        let grid = &mut self.layers[li].cells;
        Ok(cells.into_iter().filter(|&(x, y, c)| grid.set(x, y, c)).count())
    }

    fn hash_into(&self, h: &mut StableHasher) {
        h.write_str(&self.json(false).to_string());
        for l in &self.layers {
            for (x, y, c) in l.cells.cells() {
                h.write_u32(x);
                h.write_u32(y);
                h.write_bytes(&c.encode());
            }
            h.write_u8(0xff);
        }
    }
}

/// `tilemap_set`'s cells: `[[x, y, tile], ...]` where tile is a number or a cell token.
pub fn parse_tile_edits(layer: &str, cells: &Json) -> Result<TileEdits, String> {
    let list = cells.as_array().ok_or("'cells' must be a list of [x, y, tile]")?;
    if list.is_empty() || list.len() > MAX_TILE_EDITS {
        return Err(format!("'cells' must hold 1..{MAX_TILE_EDITS} cells; split larger edits"));
    }
    let mut out = Vec::with_capacity(list.len());
    for (i, c) in list.iter().enumerate() {
        let err = || format!("cells[{i}] must be [x, y, tile] with x, y whole numbers and tile a number or a token like \"7h\"");
        let a = c.as_array().filter(|a| a.len() == 3).ok_or_else(err)?;
        let (x, y) = (a[0].as_u64().ok_or_else(err)?, a[1].as_u64().ok_or_else(err)?);
        if x >= MAX_TILEMAP_SIDE as u64 || y >= MAX_TILEMAP_SIDE as u64 {
            return Err(format!("cells[{i}]: ({x}, {y}) is outside any map (at most {MAX_TILEMAP_SIDE} per side)"));
        }
        let cell = match &a[2] {
            Json::Str(s) => parse_cell(s).map_err(|e| format!("cells[{i}]: {e}"))?,
            v => match v.as_u64() {
                Some(t) if t <= u16::MAX as u64 => TileCell::new(t as u16),
                _ => return Err(err()),
            },
        };
        out.push((x as u32, y as u32, cell));
    }
    Ok(TileEdits { layer: layer.to_owned(), cells: out })
}

// ---------------------------------------------------------------------------------------------
// Vector shapes
// ---------------------------------------------------------------------------------------------

const SHAPE_KEYS: &[&str] = &["items", "tolerance", "aa", "z_layer"];
const ITEM_KEYS: &[&str] = &["rect", "radius", "circle", "ellipse", "polygon", "polyline", "path", "fill", "stroke"];
const STROKE_KEYS: &[&str] = &["width", "color", "join", "cap", "miter_limit"];

/// sRGB colour with alpha: `"#rrggbb"`, `"#rrggbbaa"`, `"#rgb"` or `[r, g, b(, a)]` in 0..1.
fn rgba(v: &Json, what: &str) -> Result<[u8; 4], String> {
    let err = || format!("'{what}' must be \"#rrggbb\", \"#rrggbbaa\" or [r, g, b(, a)] with components 0..1");
    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    match v {
        // ASCII only: byte offsets below are then character boundaries.
        Json::Str(s) if !s.is_ascii() => Err(err()),
        Json::Str(s) if s.len() == 9 => {
            let rgb = color_from_hex(&s[..7]).ok_or_else(err)?;
            let a = u8::from_str_radix(&s[7..], 16).map_err(|_| err())?;
            Ok([b(rgb[0]), b(rgb[1]), b(rgb[2]), a])
        }
        Json::Str(s) => color_from_hex(s).map(|c| [b(c[0]), b(c[1]), b(c[2]), 255]).ok_or_else(err),
        Json::Arr(a) => {
            let c: Vec<f64> = a.iter().map(Json::as_f64).collect::<Option<_>>().ok_or_else(err)?;
            if !(3..=4).contains(&c.len()) || c.iter().any(|x| !(0.0..=1.0).contains(x)) {
                return Err(err());
            }
            Ok([b(c[0] as f32), b(c[1] as f32), b(c[2] as f32), c.get(3).map_or(255, |a| b(*a as f32))])
        }
        _ => Err(err()),
    }
}

fn rgba_json(c: [u8; 4]) -> Json {
    let hex = color_hex([c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]);
    Json::from(if c[3] == 255 { hex } else { format!("{hex}{:02x}", c[3]) })
}

fn point_list(v: &Json, what: &str, budget: &mut usize) -> Result<Vec<[f32; 2]>, String> {
    let err = || format!("'{what}' must be a list of [x, y] points (finite, within ±1e6)");
    let a = v.as_array().ok_or_else(err)?;
    if a.len() > *budget {
        return Err(format!("the shape has more than {MAX_SHAPE_INPUT} points or path commands"));
    }
    *budget -= a.len();
    a.iter()
        .map(|p| match p.as_array().map(|q| q.iter().map(Json::as_f64).collect::<Option<Vec<_>>>()) {
            Some(Some(q)) if q.len() == 2 && q.iter().all(|c| c.is_finite() && c.abs() <= 1e6) => Ok([q[0] as f32, q[1] as f32]),
            _ => Err(err()),
        })
        .collect()
}

/// Path commands: `["M", x, y]`, `["L", x, y]`, `["Q", cx, cy, x, y]`, `["C", c1x, c1y, c2x, c2y,
/// x, y]`, `["A", cx, cy, rx, ry, start_deg, sweep_deg]`, `["Z"]`.
fn path_cmds(v: &Json, budget: &mut usize) -> Result<Path, String> {
    let err = |i: usize| {
        format!("'path'[{i}] must be [\"M\", x, y], [\"L\", x, y], [\"Q\", cx, cy, x, y], [\"C\", c1x, c1y, c2x, c2y, x, y], [\"A\", cx, cy, rx, ry, start_deg, sweep_deg] or [\"Z\"]")
    };
    let a = v.as_array().ok_or("'path' must be a list of commands")?;
    if a.len() > *budget {
        return Err(format!("the shape has more than {MAX_SHAPE_INPUT} points or path commands"));
    }
    *budget -= a.len();
    let mut path = Path::new();
    for (i, c) in a.iter().enumerate() {
        let parts = c.as_array().ok_or_else(|| err(i))?;
        let op = parts.first().and_then(Json::as_str).ok_or_else(|| err(i))?;
        let n: Vec<f32> = parts[1..]
            .iter()
            .map(|x| x.as_f64().filter(|x| x.is_finite() && x.abs() <= 1e6).map(|x| x as f32))
            .collect::<Option<_>>()
            .ok_or_else(|| err(i))?;
        let cmd = match (op, n.as_slice()) {
            ("M", &[x, y]) => PathCmd::MoveTo([x, y]),
            ("L", &[x, y]) => PathCmd::LineTo([x, y]),
            ("Q", &[cx, cy, x, y]) => PathCmd::QuadTo([cx, cy], [x, y]),
            ("C", &[a, b, c, d, x, y]) => PathCmd::CubicTo([a, b], [c, d], [x, y]),
            ("A", &[cx, cy, rx, ry, s, w]) if rx >= 0.0 && ry >= 0.0 => {
                PathCmd::Arc { center: [cx, cy], radii: [rx, ry], start: s.to_radians(), sweep: w.to_radians() }
            }
            ("Z", &[]) => PathCmd::Close,
            _ => return Err(err(i)),
        };
        path.cmds.push(cmd);
    }
    Ok(path)
}

/// One shape2d item, parsed and ready to tessellate.
#[derive(Clone, Debug)]
struct ShapeItem {
    path: Path,
    fill: Option<[u8; 4]>,
    stroke: Option<StrokeStyle>,
}

/// Most vertices (by [`Shape2dSpec::vertices`], their upper bounds) of the distinct shape2d
/// shapes in one world (identical shapes share one mesh, so count once). See [`ShapeBudget`].
pub const MAX_WORLD_SHAPE_VERTICES: usize = 2_000_000;

/// The distinct shapes in a world and their vertex bounds: world state, charged when an entity
/// with a shape enters the world (spawn, set, load, undo, rollback) and released when it leaves,
/// so whether a shape fits depends only on what is in the world, never on what has been drawn.
/// The journal's copies of old states don't count.
#[derive(Clone, Debug, Default)]
pub struct ShapeBudget {
    /// Key → the distinct shapes with that key (almost always one) and how many entities use each.
    shapes: BTreeMap<u64, Vec<(Arc<Shape2dSpec>, usize)>>,
    vertices: usize,
}

impl ShapeBudget {
    /// The vertex bounds of the distinct shapes in the world, summed.
    pub fn vertices(&self) -> usize {
        self.vertices
    }

    /// Distinct shapes in the world.
    pub fn distinct(&self) -> usize {
        self.shapes.values().map(Vec::len).sum()
    }

    fn uses(&self, s: &Shape2dSpec) -> usize {
        self.shapes.get(&s.key).and_then(|v| v.iter().find(|(o, _)| o.same(s))).map_or(0, |(_, n)| *n)
    }

    /// One more entity uses `s`.
    pub fn charge(&mut self, s: &Arc<Shape2dSpec>) {
        let list = self.shapes.entry(s.key).or_default();
        match list.iter_mut().find(|(o, _)| o.same(s)) {
            Some((_, n)) => *n += 1,
            None => {
                list.push((Arc::clone(s), 1));
                self.vertices += s.vertices;
            }
        }
    }

    /// One entity fewer uses `s`.
    pub fn release(&mut self, s: &Shape2dSpec) {
        let Some(list) = self.shapes.get_mut(&s.key) else { return };
        let Some(i) = list.iter().position(|(o, _)| o.same(s)) else { return };
        list[i].1 -= 1;
        if list[i].1 == 0 {
            list.swap_remove(i);
            self.vertices -= s.vertices;
            if list.is_empty() {
                self.shapes.remove(&s.key);
            }
        }
    }

    /// The sum after an entity's shape goes from `old` to `new` (`None`: no shape).
    pub fn after(&self, new: Option<&Shape2dSpec>, old: Option<&Shape2dSpec>) -> usize {
        let mut total = self.vertices;
        match (new, old) {
            (Some(n), Some(o)) if n.same(o) => {}
            (n, o) => {
                if let Some(o) = o.filter(|o| self.uses(o) == 1) {
                    total -= o.vertices;
                }
                if let Some(n) = n.filter(|n| self.uses(n) == 0) {
                    total += n.vertices;
                }
            }
        }
        total
    }
}

/// Distinct shapes by key (their JSON compared on a hit): identical shape2d components share one
/// spec and one tessellation. Only an index for sharing; budgets are per world ([`ShapeBudget`]).
static SHAPES: std::sync::Mutex<BTreeMap<u64, std::sync::Weak<Shape2dSpec>>> = std::sync::Mutex::new(BTreeMap::new());

thread_local! {
    /// Set while world files load: shapes are tessellated later, off the engine thread.
    static DEFERRED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` with shape2d tessellation deferred (world files: their shapes were valid when saved;
/// the renderer tessellates them on a worker, and a hand-edited bad one draws nothing and says
/// why instead of stalling the load).
pub fn deferred<T>(f: impl FnOnce() -> T) -> T {
    let was = DEFERRED.with(|d| d.replace(true));
    let out = f();
    DEFERRED.with(|d| d.set(was));
    out
}

fn registry() -> std::sync::MutexGuard<'static, BTreeMap<u64, std::sync::Weak<Shape2dSpec>>> {
    SHAPES.lock().unwrap_or_else(|p| p.into_inner())
}

impl Shape2dSpec {
    /// Parses and validates `v`, works out its vertex bound and bounds, and tessellates it, or
    /// returns the live spec of an identical shape (no second tessellation). Inside
    /// [`deferred`], doesn't tessellate (that happens when it is first drawn).
    pub fn parse(v: &Json) -> Result<Arc<Shape2dSpec>, String> {
        let mut spec = Shape2dSpec::prepare(v)?;
        let hit = registry().get(&spec.key).and_then(std::sync::Weak::upgrade);
        let spec = match hit.filter(|s| s.same(&spec)) {
            Some(live) => live,
            None => {
                spec.vertices = spec.vertex_bound()?;
                let spec = Arc::new(spec);
                let mut reg = registry();
                if reg.len() >= 64 && reg.len().is_power_of_two() {
                    reg.retain(|_, w| w.strong_count() > 0);
                }
                reg.insert(spec.key, Arc::downgrade(&spec));
                spec
            }
        };
        if !DEFERRED.with(|d| d.get()) {
            spec.ensure()?;
        }
        Ok(spec)
    }

    /// The same shape: equal canonical JSON (the key only rules most others out quickly).
    pub fn same(&self, other: &Shape2dSpec) -> bool {
        std::ptr::eq(self, other) || (self.key == other.key && self.json == other.json)
    }

    /// The upper bound of the tessellation's vertices, from the flattened outlines' point counts
    /// (flattening is cheap and deterministic; nothing is triangulated).
    fn vertex_bound(&self) -> Result<usize, String> {
        let mut points = vector::MAX_SHAPE_POINTS;
        let mut n = 0usize;
        for (i, item) in self.items.iter().enumerate() {
            let contours = item.path.flatten_budgeted(self.tolerance, &mut points).map_err(|e| format!("'shape2d.items[{i}]': {e}"))?;
            n = n.saturating_add(vector::vertex_bound(&contours, item.fill.is_some(), item.stroke.as_ref(), self.tolerance, self.aa));
        }
        Ok(n.min(vector::MAX_SHAPE_VERTICES))
    }

    /// Parses and validates the description, without tessellating.
    fn prepare(v: &Json) -> Result<Shape2dSpec, String> {
        let w = "shape2d";
        keys(v, w, SHAPE_KEYS)?;
        let tolerance = num(v, "tolerance", w, 1e-5, 10.0)?.unwrap_or(0.005);
        let aa = boolean(v, "aa", w)?.unwrap_or(true);
        let z_layer = int(v, "z_layer", w, -1_000_000, 1_000_000)?.unwrap_or(0) as i32;
        let list = opt(v, "items").and_then(Json::as_array).ok_or("'shape2d.items' (a list of shapes) is required")?;
        if list.is_empty() || list.len() > MAX_SHAPE_ITEMS {
            return Err(format!("'shape2d.items' must hold 1..{MAX_SHAPE_ITEMS} items"));
        }
        let mut budget = MAX_SHAPE_INPUT;
        let mut items = Vec::with_capacity(list.len());
        let mut canon_items = Vec::with_capacity(list.len());
        let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for (i, item) in list.iter().enumerate() {
            let what = format!("shape2d.items[{i}]");
            keys(item, &what, ITEM_KEYS)?;
            let geometry: Vec<&str> = ["rect", "circle", "ellipse", "polygon", "polyline", "path"].into_iter().filter(|k| opt(item, k).is_some()).collect();
            if geometry.len() != 1 {
                return Err(format!("'{what}' needs exactly one of rect, circle, ellipse, polygon, polyline, path"));
            }
            let kind = geometry[0];
            let g = opt(item, kind).expect("present");
            let mut canon: Vec<(&'static str, Json)> = Vec::new();
            let radius = num(item, "radius", &what, 0.0, 1e6)?;
            if radius.is_some() && kind != "rect" {
                return Err(format!("'{what}.radius' (rounded corners) only applies to rect"));
            }
            let quad = |key: &str, n: usize| -> Result<Vec<f32>, String> {
                let a: Vec<f64> = g.as_array().and_then(|a| a.iter().map(Json::as_f64).collect::<Option<Vec<_>>>()).unwrap_or_default();
                if a.len() != n || a.iter().any(|x| !x.is_finite() || x.abs() > 1e6) {
                    return Err(format!("'{what}.{key}' must be {n} finite numbers"));
                }
                Ok(a.into_iter().map(|x| x as f32).collect())
            };
            let path = match kind {
                "rect" => {
                    let r = quad("rect", 4)?;
                    if r[2] <= 0.0 || r[3] <= 0.0 {
                        return Err(format!("'{what}.rect' is [x, y, width, height] with a positive size"));
                    }
                    canon.push(("rect", Json::arr(r.iter().map(|x| Json::from(*x)))));
                    if let Some(rad) = radius.filter(|r| *r > 0.0) {
                        canon.push(("radius", Json::from(rad)));
                        Path::rounded_rect(r[0], r[1], r[2], r[3], rad)
                    } else {
                        Path::rect(r[0], r[1], r[2], r[3])
                    }
                }
                "circle" => {
                    let c = quad("circle", 3)?;
                    if c[2] <= 0.0 {
                        return Err(format!("'{what}.circle' is [cx, cy, radius] with a positive radius"));
                    }
                    canon.push(("circle", Json::arr(c.iter().map(|x| Json::from(*x)))));
                    Path::circle([c[0], c[1]], c[2])
                }
                "ellipse" => {
                    let c = quad("ellipse", 4)?;
                    if c[2] <= 0.0 || c[3] <= 0.0 {
                        return Err(format!("'{what}.ellipse' is [cx, cy, rx, ry] with positive radii"));
                    }
                    canon.push(("ellipse", Json::arr(c.iter().map(|x| Json::from(*x)))));
                    Path::ellipse([c[0], c[1]], [c[2], c[3]])
                }
                "polygon" | "polyline" => {
                    let pts = point_list(g, &format!("{what}.{kind}"), &mut budget)?;
                    if pts.len() < 2 {
                        return Err(format!("'{what}.{kind}' needs at least 2 points"));
                    }
                    canon.push((if kind == "polygon" { "polygon" } else { "polyline" }, Json::arr(pts.iter().map(|p| Json::arr(p.map(Json::from))))));
                    if kind == "polygon" {
                        Path::polygon(&pts)
                    } else {
                        Path::polyline(&pts)
                    }
                }
                _ => {
                    let p = path_cmds(g, &mut budget)?;
                    canon.push(("path", g.clone()));
                    p
                }
            };
            let fill = opt(item, "fill").map(|c| rgba(c, &format!("{what}.fill"))).transpose()?;
            let stroke = match opt(item, "stroke") {
                None => None,
                Some(s) => {
                    let sw = format!("{what}.stroke");
                    keys(s, &sw, STROKE_KEYS)?;
                    let limit = num(s, "miter_limit", &sw, 1.0, 100.0)?.unwrap_or(4.0);
                    let width = num(s, "width", &sw, 0.0, 1e5)?.unwrap_or(0.05);
                    if width <= 0.0 {
                        return Err(format!("'{sw}.width' must be above 0 (leave the stroke out to draw none)"));
                    }
                    Some(StrokeStyle {
                        width,
                        color: opt(s, "color").map(|c| rgba(c, &format!("{sw}.color"))).transpose()?.unwrap_or([255; 4]),
                        join: choice(s, "join", &sw, &[("miter", LineJoin::Miter { limit }), ("round", LineJoin::Round), ("bevel", LineJoin::Bevel)])?
                            .unwrap_or(LineJoin::Miter { limit }),
                        cap: choice(s, "cap", &sw, &[("butt", LineCap::Butt), ("square", LineCap::Square), ("round", LineCap::Round)])?
                            .unwrap_or_default(),
                    })
                }
            };
            if fill.is_none() && stroke.is_none() {
                return Err(format!("'{what}' needs a 'fill' colour, a 'stroke', or both"));
            }
            if kind == "polyline" && fill.is_some() {
                return Err(format!("'{what}': a polyline is open; stroke it (or use polygon to fill)"));
            }
            if let Some(c) = fill {
                canon.push(("fill", rgba_json(c)));
            }
            if let Some(s) = stroke {
                let mut sf = vec![("width", Json::from(s.width)), ("color", rgba_json(s.color))];
                match s.join {
                    LineJoin::Miter { limit } => {
                        if limit != 4.0 {
                            sf.push(("miter_limit", Json::from(limit)));
                        }
                    }
                    LineJoin::Round => sf.push(("join", Json::from("round"))),
                    LineJoin::Bevel => sf.push(("join", Json::from("bevel"))),
                }
                match s.cap {
                    LineCap::Butt => {}
                    LineCap::Square => sf.push(("cap", Json::from("square"))),
                    LineCap::Round => sf.push(("cap", Json::from("round"))),
                }
                canon.push(("stroke", Json::obj(sf)));
            }
            if let Some(c) = path.control_bounds() {
                let pad = stroke.map_or(0.0, |s| s.width as f64 * 0.5 * vector::stroke_reach(s.join));
                b = [b[0].min(c[0] - pad), b[1].min(c[1] - pad), b[2].max(c[2] + pad), b[3].max(c[3] + pad)];
            }
            canon_items.push(Json::obj(canon));
            items.push(ShapeItem { path, fill, stroke });
        }
        let mut cf: Vec<(&'static str, Json)> = vec![("items", Json::arr(canon_items))];
        if tolerance != 0.005 {
            cf.push(("tolerance", Json::from(tolerance)));
        }
        if !aa {
            cf.push(("aa", Json::from(false)));
        }
        if z_layer != 0 {
            cf.push(("z_layer", Json::from(z_layer as i64)));
        }
        let json = Json::obj(cf);
        let key = specula_core::hash::hash_str(&json.to_string());
        // f32 rounding of the flattened points: a hair of slack, outward.
        let bounds = if b[0] <= b[2] {
            let slack = 1e-6 * b.iter().fold(1e-3f64, |m, v| m.max(v.abs()));
            let lo = |v: f64| (v - slack) as f32;
            let hi = |v: f64| (v + slack) as f32;
            (Vec3::new(lo(b[0]), lo(b[1]), 0.0), Vec3::new(hi(b[2]), hi(b[3]), 0.0))
        } else {
            (Vec3::ZERO, Vec3::ZERO)
        };
        Ok(Shape2dSpec { json, z_layer, key, vertices: 0, items, tolerance, aa, bounds, mesh: std::sync::OnceLock::new() })
    }

    /// The tessellation of the items (one point budget for the whole shape).
    fn tessellate(&self) -> Result<ShapeMesh, String> {
        let mut mesh = ShapeMesh::default();
        let mut points = vector::MAX_SHAPE_POINTS;
        for (i, item) in self.items.iter().enumerate() {
            let what = format!("shape2d.items[{i}]");
            let contours = item.path.flatten_budgeted(self.tolerance, &mut points).map_err(|e| format!("'{what}': {e}"))?;
            if let Some(c) = item.fill {
                let before = mesh.indices.len();
                vector::fill(&contours, c, self.aa, &mut mesh).map_err(|e| format!("'{what}': {e}"))?;
                if mesh.indices.len() == before {
                    return Err(format!(
                        "'{what}': the fill covers no area (a degenerate or self-intersecting outline whose parts cancel, \
                         like a bow-tie); fill each loop as its own polygon"
                    ));
                }
            }
            if let Some(s) = item.stroke {
                vector::stroke(&contours, &s, self.tolerance, self.aa, &mut mesh).map_err(|e| format!("'{what}': {e}"))?;
            }
        }
        debug_assert!(mesh.vertices.len() <= self.vertices.max(1), "vertex bound {} < {}", self.vertices, mesh.vertices.len());
        Ok(mesh)
    }

    /// The mesh, tessellating now if no one has yet (blocks while another thread does). Its
    /// result is kept: it depends on nothing but the description, so an error is the geometry's
    /// own (a panic, an engine bug, is kept as an error too rather than retried every frame).
    pub fn ensure(&self) -> Result<&Arc<ShapeMesh>, String> {
        self.mesh
            .get_or_init(|| match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.tessellate())) {
                Ok(r) => r.map(Arc::new),
                Err(panic) => Err(format!("tessellation failed unexpectedly (an engine bug, please report it): {}", panic_text(&*panic))),
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// The mesh if it is tessellated (`None` while deferred or failed).
    pub fn mesh(&self) -> Option<&Arc<ShapeMesh>> {
        self.mesh.get().and_then(|m| m.as_ref().ok())
    }

    /// Why tessellation failed, once tried.
    pub fn mesh_error(&self) -> Option<&str> {
        self.mesh.get().and_then(|m| m.as_ref().err()).map(String::as_str)
    }

    /// Local bounds, from the description (see [`Shape2dSpec`]): the same before and after
    /// tessellation.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.bounds
    }
}

/// A panic payload's message.
fn panic_text(p: &(dyn std::any::Any + Send)) -> &str {
    p.downcast_ref::<&str>().copied().or_else(|| p.downcast_ref::<String>().map(String::as_str)).unwrap_or("no message")
}

impl Shape2dSpec {
}

// ---------------------------------------------------------------------------------------------
// The component as a whole
// ---------------------------------------------------------------------------------------------

impl TwoD {
    /// The JSON key and shape name: `sprite`, `tilemap` or `shape2d`.
    pub fn key(&self) -> &'static str {
        match self {
            TwoD::Sprite(_) => "sprite",
            TwoD::Tilemap(_) => "tilemap",
            TwoD::Shape2d(_) => "shape2d",
        }
    }

    /// Full, canonical JSON (world files).
    pub fn to_json(&self) -> Json {
        match self {
            TwoD::Sprite(s) => s.to_json(),
            TwoD::Tilemap(t) => t.to_json(),
            TwoD::Shape2d(s) => s.json.clone(),
        }
    }

    /// Bounded JSON for listings (tile maps without their cells).
    pub fn listing_json(&self) -> Json {
        match self {
            TwoD::Tilemap(t) => t.summary_json(),
            other => other.to_json(),
        }
    }

    pub fn bounds(&self) -> (Vec3, Vec3) {
        match self {
            TwoD::Sprite(s) => s.bounds(),
            TwoD::Tilemap(t) => t.bounds(),
            TwoD::Shape2d(s) => s.bounds(),
        }
    }

    pub fn hash_into(&self, h: &mut StableHasher) {
        h.write_str(self.key());
        match self {
            TwoD::Sprite(s) => h.write_str(&s.to_json().to_string()),
            TwoD::Tilemap(t) => t.hash_into(h),
            TwoD::Shape2d(s) => h.write_str(&s.json.to_string()),
        }
    }

    /// Approximate heap bytes (shared parts divided among their owners).
    pub fn heap_bytes(&self) -> usize {
        match self {
            TwoD::Sprite(_) => std::mem::size_of::<SpriteSpec>() + 64,
            TwoD::Tilemap(t) => {
                (std::mem::size_of::<TilemapSpec>() + t.layers.iter().map(|l| l.cells.heap_bytes() + 64).sum::<usize>())
                    / Arc::strong_count(t).max(1)
            }
            TwoD::Shape2d(s) => {
                let mesh = s.mesh().map_or(0, |m| m.vertices.len() * 28 + m.indices.len() * 4);
                (s.json.to_string().len() + mesh) / Arc::strong_count(s).max(1)
            }
        }
    }

    /// A small description for sight subjects.
    pub fn subject_json(&self) -> Json {
        match self {
            TwoD::Sprite(s) => {
                let mut f = vec![("z_layer", Json::from(s.z_layer as i64))];
                if let Some(t) = &s.texture {
                    f.push(("texture", Json::from(t.as_str())));
                }
                if s.billboard != Billboard::None {
                    f.push(("billboard", Json::from(name_of(&BILLBOARDS, s.billboard))));
                }
                Json::obj(f)
            }
            TwoD::Tilemap(t) => Json::obj([
                ("size", Json::arr(t.size.map(Json::from))),
                ("tiles", Json::from(t.layers.iter().map(|l| l.cells.filled()).sum::<u64>())),
            ]),
            TwoD::Shape2d(s) => Json::obj([
                ("z_layer", Json::from(s.z_layer as i64)),
                ("triangles", s.mesh().map_or(Json::Null, |m| Json::from(m.indices.len() / 3))),
            ]),
        }
    }
}

/// Parses the component of `shape` (`sprite`, `tilemap` or `shape2d`) from its JSON value.
pub fn parse(shape: &str, v: &Json) -> Result<TwoD, String> {
    match shape {
        "sprite" => SpriteSpec::parse(v).map(|s| TwoD::Sprite(Box::new(s))),
        "tilemap" => TilemapSpec::parse(v).map(|t| TwoD::Tilemap(Arc::new(t))),
        "shape2d" => Shape2dSpec::parse(v).map(TwoD::Shape2d),
        _ => Err(format!("'{shape}' has no 2D component")),
    }
}

/// A world value error about a 2D component.
pub fn invalid(field: &str, message: String) -> WorldError {
    WorldError::new("invalid_value", message).with("field", field)
}

/// The four world corners of a billboard sprite's card as `view` sees it (the same orientation
/// the renderer gives it), for sight subjects; `None` for sprites fixed in the world (their local
/// bounds are exact) and for other components.
pub fn billboard_corners(two_d: &TwoD, model: &Mat4, view: &specula_gpu::View2d) -> Option<[Vec3; 4]> {
    let TwoD::Sprite(s) = two_d else { return None };
    if s.billboard == Billboard::None {
        return None;
    }
    let cols = model.to_cols_array();
    let col = |i: usize| Vec3::new(cols[i * 4], cols[i * 4 + 1], cols[i * 4 + 2]);
    let pos = Vec3::new(cols[12], cols[13], cols[14]);
    let (ax, ay) = (col(0), col(1));
    let size = [s.size[0] * ax.length(), s.size[1] * ay.length()];
    let v3 = |a: [f32; 3]| Vec3::new(a[0], a[1], a[2]);
    let (right, up) = match s.billboard {
        Billboard::Spherical => (v3(view.right), v3(view.up)),
        _ => {
            let up = if ay.length() > 1e-12 { ay.normalize() } else { Vec3::Y };
            let to_cam = if view.ortho { v3(view.forward) * -1.0 } else { v3(view.position) - pos };
            let r = up.cross(to_cam);
            (if r.length() > 1e-6 { r.normalize() } else { v3(view.right) }, up)
        }
    };
    let corner = |qx: f32, qy: f32| pos + right * ((qx - s.pivot[0]) * size[0]) + up * ((qy - s.pivot[1]) * size[1]);
    Some([corner(0.0, 0.0), corner(1.0, 0.0), corner(1.0, 1.0), corner(0.0, 1.0)])
}

// ---------------------------------------------------------------------------------------------
// GPU side
// ---------------------------------------------------------------------------------------------

/// A texture uploaded from the assets root.
struct LoadedTexture {
    id: TextureId,
    size: [u32; 2],
}

/// What identifies a file's contents for change polling: modification time and length (`None`:
/// the file can't be read).
type Stamp = Option<(std::time::SystemTime, u64)>;

fn stamp_of(file: &FsPath) -> Stamp {
    let m = std::fs::metadata(file).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// Largest sprite or tileset texture side, pixels (checked from the PNG header, before decoding).
pub const MAX_TEXTURE_SIDE: u32 = 8192;

/// Reads and decodes a PNG, refusing one larger than [`MAX_TEXTURE_SIDE`] from its header alone.
fn read_texture(key: &str, file: &FsPath) -> Result<png::Image, String> {
    use std::io::Read as _;
    let mut head = [0u8; 24];
    let mut f = std::fs::File::open(file).map_err(|e| format!("{key}: {e}"))?;
    if f.read_exact(&mut head).is_err() || head[..8] != *b"\x89PNG\r\n\x1a\n" || head[12..16] != *b"IHDR" {
        return Err(format!("{key}: not a PNG"));
    }
    let (w, h) = (u32::from_be_bytes([head[16], head[17], head[18], head[19]]), u32::from_be_bytes([head[20], head[21], head[22], head[23]]));
    if w > MAX_TEXTURE_SIDE || h > MAX_TEXTURE_SIDE {
        return Err(format!("{key}: {w}x{h} pixels; sprite and tileset textures may be at most {MAX_TEXTURE_SIDE} per side"));
    }
    let bytes = std::fs::read(file).map_err(|e| format!("{key}: {e}"))?;
    png::decode(&bytes).map_err(|e| format!("{key}: not a readable PNG ({e:?})"))
}

/// One texture path: what is shown (the last good upload) and what is in flight.
struct TexEntry {
    current: Option<LoadedTexture>,
    /// A load (or reload) is on the loader thread.
    loading: bool,
    /// A change check is on the loader thread.
    checking: bool,
    /// The file as of the last load.
    stamp: Stamp,
}

/// Work for the loader thread.
enum Job {
    /// Read and decode a texture.
    Load { key: String, file: PathBuf },
    /// Reload a texture only if its file no longer matches `stamp` (the stat happens there).
    Check { key: String, file: PathBuf, stamp: Stamp },
    /// Tessellate a deferred shape.
    Shape(Arc<Shape2dSpec>),
}

/// What the loader thread sends back.
enum Done {
    Texture { key: String, stamp: Stamp, image: Result<png::Image, String> },
    Unchanged(String),
    /// A deferred shape is tessellated (or failed); by [`shape_slot`].
    Shape(usize),
}

/// A shape's GPU mesh (or why it couldn't be uploaded), with the shape it was made from.
type ShapeMeshEntry = (Arc<Shape2dSpec>, Result<MeshId, String>);

/// Identifies a shape spec while the loader holds it (its address: keys alone may collide).
fn shape_slot(spec: &Arc<Shape2dSpec>) -> usize {
    Arc::as_ptr(spec) as usize
}

/// Runs a loader job, turning a panic (an engine bug) into an error for `what`.
fn guarded<T>(what: &str, job: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(job))
        .unwrap_or_else(|p| Err(format!("{what}: the loader failed unexpectedly (an engine bug, please report it): {}", panic_text(&*p))))
}

type Loader = (std::sync::mpsc::Sender<Job>, std::sync::mpsc::Receiver<Done>);

/// One tile map layer's cell texture, and the grid it holds (to upload only changed chunks).
struct LayerTexture {
    id: TextureId,
    grid: TileGrid,
}

/// Most problems listed for one frame (missing textures, frames past a sheet, …).
const MAX_FRAME_PROBLEMS: usize = 16;

/// Writes a problem into the next reused string of `buf` (deduplicated, at most
/// [`MAX_FRAME_PROBLEMS`]); allocates only while the buffer grows.
fn push_problem(buf: &mut Vec<String>, count: &mut usize, args: std::fmt::Arguments) {
    use std::fmt::Write as _;
    if *count >= MAX_FRAME_PROBLEMS {
        return;
    }
    if *count == buf.len() {
        buf.push(String::with_capacity(160));
    }
    let slot = &mut buf[*count];
    slot.clear();
    let _ = slot.write_fmt(args);
    if !buf[..*count].contains(&buf[*count]) {
        *count += 1;
    }
}
/// How often in-use texture files are checked for changes.
const TEXTURE_POLL_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

/// The GPU resources 2D entities draw with: textures by asset path, tile maps' cell textures by
/// entity, shape meshes by shape key. Everything is created on first use and freed when no
/// entity uses it any more ([`TwoDGpu::collect`]).
///
/// - **Textures** are read (after a header check against [`MAX_TEXTURE_SIDE`]) and decoded on a
///   loader thread; the engine thread only uploads. Their files are polled there too
///   ([`TwoDGpu::poll_files`]): an edited PNG is reloaded (the old one stays on screen until the
///   new one is up), a missing or broken one is retried once its file changes. Problems are kept
///   per path and cleared when a load succeeds.
/// - **Shapes** loaded from world files are tessellated on the loader thread when first drawn.
///   Identical shapes share one spec and one mesh; a shape whose content changes every tick
///   uploads a new mesh every tick (the old ones are freed by the next collection): animate
///   shapes by their transform or colour instead.
/// - **Tile cells** re-upload only the chunks that changed since the last frame.
/// - **Problems** of the frame being drawn ([`TwoDGpu::problems`]) reuse their strings: a warm
///   frame allocates nothing, problems or not.
pub struct TwoDGpu {
    assets: PathBuf,
    textures: BTreeMap<String, TexEntry>,
    cells: BTreeMap<(u64, usize), LayerTexture>,
    /// Shape meshes by key; each key's distinct shapes (almost always one) told apart by JSON.
    shapes: BTreeMap<u64, Vec<ShapeMeshEntry>>,
    /// Deferred shapes being tessellated on the loader thread, by [`shape_slot`].
    pending_shapes: BTreeSet<usize>,
    loader: Option<Loader>,
    last_poll: std::time::Instant,
    /// Reused per-frame scratch.
    bytes: Vec<u8>,
    /// The latest problem per texture path or resource (cleared by a success, dropped with the
    /// resource).
    pub errors: BTreeMap<String, String>,
    /// This frame's problems: the first `problem_count` strings (the rest are spare capacity).
    problem_buf: Vec<String>,
    problem_count: usize,
    /// Cell chunks uploaded since start-up (edits cost only the chunks they touch).
    pub chunk_uploads: u64,
    /// Textures loaded or reloaded since start-up.
    pub texture_loads: u64,
}

impl TwoDGpu {
    pub fn new(assets: &FsPath) -> TwoDGpu {
        TwoDGpu {
            assets: assets.to_path_buf(),
            textures: BTreeMap::new(),
            cells: BTreeMap::new(),
            shapes: BTreeMap::new(),
            pending_shapes: BTreeSet::new(),
            loader: None,
            last_poll: std::time::Instant::now(),
            bytes: Vec::new(),
            errors: BTreeMap::new(),
            problem_buf: Vec::new(),
            problem_count: 0,
            chunk_uploads: 0,
            texture_loads: 0,
        }
    }

    /// Starts a frame: the problems list empties and finished work is uploaded.
    pub fn begin_frame(&mut self, gpu: &mut Gpu) {
        self.problem_count = 0;
        self.pump(gpu);
    }

    /// The problems of the items drawn since [`TwoDGpu::begin_frame`] (at most 16).
    pub fn problems(&self) -> &[String] {
        &self.problem_buf[..self.problem_count]
    }

    /// Notes a problem, written into a reused string (no allocation once warm).
    fn problem(&mut self, args: std::fmt::Arguments) {
        push_problem(&mut self.problem_buf, &mut self.problem_count, args);
    }

    fn send(&mut self, job: Job) -> bool {
        let (tx, _) = self.loader.get_or_insert_with(|| {
            let (job_tx, job_rx) = std::sync::mpsc::channel::<Job>();
            let (done_tx, done_rx) = std::sync::mpsc::channel::<Done>();
            let spawned = std::thread::Builder::new().name("specula-2d-loader".into()).spawn(move || {
                for job in job_rx {
                    let done = match job {
                        Job::Load { key, file } => {
                            Done::Texture { stamp: stamp_of(&file), image: guarded(&key, || read_texture(&key, &file)), key }
                        }
                        Job::Check { key, file, stamp } => {
                            let now = stamp_of(&file);
                            if now == stamp {
                                Done::Unchanged(key)
                            } else {
                                Done::Texture { stamp: now, image: guarded(&key, || read_texture(&key, &file)), key }
                            }
                        }
                        Job::Shape(spec) => {
                            // `ensure` keeps a panic as the shape's error.
                            let _ = spec.ensure();
                            Done::Shape(shape_slot(&spec))
                        }
                    };
                    if done_tx.send(done).is_err() {
                        break;
                    }
                }
            });
            if let Err(e) = spawned {
                eprintln!("specula: 2D loader thread: {e}");
            }
            (job_tx, done_rx)
        });
        tx.send(job).is_ok()
    }

    /// Queues `path` for loading on the loader thread (started on first use).
    fn start_load(&mut self, path: &str) {
        let file = self.assets.join(path);
        let entry = self.textures.entry(path.to_owned()).or_insert(TexEntry { current: None, loading: false, checking: false, stamp: None });
        entry.loading = true;
        if !self.send(Job::Load { key: path.to_owned(), file }) {
            if let Some(e) = self.textures.get_mut(path) {
                e.loading = false;
            }
            self.errors.insert(path.to_owned(), format!("texture {path}: the loader thread is not running"));
        }
    }

    /// Asks for `path` without drawing it (captures start loads before they render).
    pub fn request(&mut self, path: &str) {
        if !self.textures.contains_key(path) {
            self.start_load(path);
        }
    }

    /// Checks `path`'s file for changes now (a capture: it shows the file as it is when asked,
    /// not as of the last periodic poll). No-op while it is loading or already being checked.
    pub fn check_now(&mut self, path: &str) {
        let Some(t) = self.textures.get(path) else { return };
        if t.loading || t.checking {
            return;
        }
        let (stamp, file) = (t.stamp, self.assets.join(path));
        if self.send(Job::Check { key: path.to_owned(), file, stamp }) {
            if let Some(t) = self.textures.get_mut(path) {
                t.checking = true;
            }
        }
    }

    /// `path` is being loaded or checked for changes.
    pub fn is_pending(&self, path: &str) -> bool {
        self.textures.get(path).is_some_and(|t| t.loading || t.checking)
    }

    /// Asks for a deferred shape's tessellation; true while it is in flight.
    pub fn request_shape(&mut self, spec: &Arc<Shape2dSpec>) -> bool {
        if spec.mesh().is_some() || spec.mesh_error().is_some() {
            return false;
        }
        let slot = shape_slot(spec);
        if !self.pending_shapes.contains(&slot) && self.send(Job::Shape(Arc::clone(spec))) {
            self.pending_shapes.insert(slot);
        }
        self.pending_shapes.contains(&slot)
    }

    /// The loader thread is gone (it can only stop by panicking outside a job): forget what was
    /// in flight with a reason, so nothing waits for it, and start a new one on the next request.
    fn loader_stopped(&mut self) {
        self.loader = None;
        self.pending_shapes.clear();
        for (key, t) in self.textures.iter_mut() {
            if t.loading {
                self.errors.insert(key.clone(), format!("texture {key}: the loader thread stopped; retrying when the file changes"));
            }
            t.loading = false;
            t.checking = false;
        }
    }

    /// Uploads whatever the loader finished.
    pub fn pump(&mut self, gpu: &mut Gpu) {
        loop {
            let done = match &self.loader {
                Some((_, rx)) => match rx.try_recv() {
                    Ok(d) => d,
                    Err(std::sync::mpsc::TryRecvError::Empty) => return,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.loader_stopped();
                        return;
                    }
                },
                None => return,
            };
            match done {
                Done::Unchanged(key) => {
                    if let Some(e) = self.textures.get_mut(&key) {
                        e.checking = false;
                    }
                }
                Done::Shape(slot) => {
                    self.pending_shapes.remove(&slot);
                }
                Done::Texture { key, stamp, image } => {
                    let Some(entry) = self.textures.get_mut(&key) else { continue }; // collected meanwhile
                    entry.loading = false;
                    entry.checking = false;
                    entry.stamp = stamp;
                    let uploaded = image.and_then(|img| {
                        gpu.upload_texture(img.width, img.height, TextureFormat::Rgba8Srgb, &img.rgba)
                            .map(|id| LoadedTexture { id, size: [img.width, img.height] })
                            .map_err(|e| format!("{key}: {e}"))
                    });
                    match uploaded {
                        Ok(t) => {
                            if let Some(old) = entry.current.replace(t) {
                                gpu.destroy_texture(old.id);
                            }
                            self.texture_loads += 1;
                            self.errors.remove(&key);
                        }
                        // The last good texture (if any) stays on screen.
                        Err(e) => {
                            self.errors.insert(key, format!("texture {e}"));
                        }
                    }
                }
            }
        }
    }

    /// Change checks in flight on the loader thread.
    pub fn checking(&self) -> usize {
        self.textures.values().filter(|t| t.checking).count()
    }

    /// Loads in flight: textures and deferred shapes.
    pub fn loading(&self) -> usize {
        self.textures.values().filter(|t| t.loading).count() + self.pending_shapes.len()
    }

    /// Asks the loader thread to check every texture in use for changes (it stats the files;
    /// changed ones are reloaded, failed ones retried), at most every second.
    pub fn poll_files(&mut self, force: bool) {
        if !force && self.last_poll.elapsed() < TEXTURE_POLL_EVERY {
            return;
        }
        self.last_poll = std::time::Instant::now();
        let idle: Vec<(String, Stamp)> =
            self.textures.iter().filter(|(_, t)| !t.loading && !t.checking).map(|(k, t)| (k.clone(), t.stamp)).collect();
        for (key, stamp) in idle {
            let file = self.assets.join(&key);
            if self.send(Job::Check { key: key.clone(), file, stamp }) {
                if let Some(e) = self.textures.get_mut(&key) {
                    e.checking = true;
                }
            }
        }
    }

    fn texture(&mut self, path: &str) -> Option<(TextureId, [u32; 2])> {
        self.request(path);
        let entry = self.textures.get(path);
        let shown = entry.and_then(|t| t.current.as_ref()).map(|t| (t.id, t.size));
        let loading = entry.is_some_and(|t| t.loading);
        if let Some(e) = self.errors.get(path) {
            push_problem(&mut self.problem_buf, &mut self.problem_count, format_args!("{e}"));
        } else if shown.is_none() && loading {
            self.problem(format_args!("texture {path}: still loading"));
        }
        shown
    }

    /// The cell texture of layer `li` of entity `id`, brought up to date with `grid` (only
    /// changed chunks are uploaded).
    fn cells(&mut self, gpu: &mut Gpu, id: u64, li: usize, grid: &TileGrid) -> Option<TextureId> {
        let key = (id, li);
        let stale = self.cells.get(&key).is_some_and(|l| l.grid.width() != grid.width() || l.grid.height() != grid.height());
        if stale {
            if let Some(l) = self.cells.remove(&key) {
                gpu.destroy_texture(l.id);
            }
        }
        // Formatted only when needed: a warm frame allocates nothing here.
        let name = || format!("tilemap {id} layer {li}");
        match self.cells.get_mut(&key) {
            None => match gpu.upload_texture(grid.width(), grid.height(), TextureFormat::Rgba8Unorm, &grid.encode()) {
                Ok(tid) => {
                    self.cells.insert(key, LayerTexture { id: tid, grid: grid.clone() });
                    self.errors.remove(&name());
                    Some(tid)
                }
                Err(e) => {
                    let msg = format!("{}: {e}", name());
                    self.errors.insert(name(), msg.clone());
                    self.problem(format_args!("{msg}"));
                    None
                }
            },
            Some(layer) => {
                let mut failed = None;
                for i in 0..grid.chunk_count() {
                    if grid.same_chunk(&layer.grid, i) {
                        continue;
                    }
                    let r = grid.chunk_region(i);
                    grid.encode_region(r, &mut self.bytes);
                    if let Err(e) = gpu.update_texture(layer.id, r[0], r[1], r[2], r[3], &self.bytes) {
                        failed = Some(format!("{}: {e}", name()));
                    }
                    self.chunk_uploads += 1;
                }
                if !(0..grid.chunk_count()).all(|i| grid.same_chunk(&layer.grid, i)) {
                    layer.grid = grid.clone();
                }
                let id = layer.id;
                if let Some(msg) = failed {
                    self.errors.insert(name(), msg.clone());
                    self.problem(format_args!("{msg}"));
                }
                Some(id)
            }
        }
    }

    /// The mesh of entity `id`'s shape `s`, uploaded on first use.
    fn shape(&mut self, gpu: &mut Gpu, id: u64, s: &Arc<Shape2dSpec>) -> Option<MeshId> {
        let Some(mesh) = s.mesh() else {
            // Deferred (loaded from a world file): tessellated on the loader thread.
            if self.request_shape(s) {
                self.problem(format_args!("shape2d {id}: still tessellating"));
            } else if let Some(e) = s.mesh_error() {
                push_problem(&mut self.problem_buf, &mut self.problem_count, format_args!("shape2d {id}: {e}"));
            }
            return None;
        };
        if mesh.is_empty() {
            return None;
        }
        let list = self.shapes.entry(s.key).or_default();
        let at = match list.iter().position(|(o, _)| Arc::ptr_eq(o, s) || o.same(s)) {
            Some(at) => at,
            None => {
                list.push((Arc::clone(s), gpu.upload_mesh(&mesh.vertices, &mesh.indices).map_err(|e| e.to_string())));
                list.len() - 1
            }
        };
        match &list[at].1 {
            Ok(m) => Some(*m),
            Err(e) => {
                push_problem(&mut self.problem_buf, &mut self.problem_count, format_args!("shape2d {id}: {e}"));
                None
            }
        }
    }

    /// Appends the draws of 2D entity `e` (world transform `model`, sRGB `tint`) at `time_s`.
    /// Allocation-free once warm (sheet frames are computed, not listed).
    #[allow(clippy::too_many_arguments)]
    pub fn draws(&mut self, gpu: &mut Gpu, id: u64, two_d: &TwoD, model: &Mat4, tint: [f32; 4], time_s: f32, out: &mut Vec<Draw2d>) {
        let cols = model.to_cols_array();
        let col = |i: usize| Vec3::new(cols[i * 4], cols[i * 4 + 1], cols[i * 4 + 2]);
        let origin = [cols[12], cols[13], cols[14]];
        let (ax, ay) = (col(0), col(1));
        let (sx, sy) = (ax.length(), ay.length());
        match two_d {
            TwoD::Sprite(s) => {
                let mut sprite = Sprite {
                    position: origin,
                    size: [s.size[0] * sx, s.size[1] * sy],
                    pivot: s.pivot,
                    axes: [[ax.x, ax.y, ax.z], [ay.x, ay.y, ay.z]],
                    color: tint,
                    flip: s.flip,
                    layer: s.z_layer,
                    billboard: s.billboard,
                    blend: s.blend,
                    filter: s.filter,
                    pixel_snap: s.pixel_snap,
                    lit: s.lit,
                    cast_shadows: s.cast_shadows,
                    ..Default::default()
                };
                if let Some(path) = &s.texture {
                    let Some((tex, size)) = self.texture(path) else { return };
                    sprite.texture = Some(tex);
                    let (uv, problem) = sprite_uv(s, size, time_s as f64);
                    sprite.uv = uv;
                    if let Some(p) = problem {
                        self.problem(format_args!("sprite {id}: {p}"));
                    }
                }
                out.push(Draw2d::Sprite(sprite));
            }
            TwoD::Tilemap(t) => {
                let Some((tileset, _)) = self.texture(&t.tileset) else { return };
                let down = [-ay.x / sy.max(1e-12), -ay.y / sy.max(1e-12), -ay.z / sy.max(1e-12)];
                for (li, layer) in t.layers.iter().enumerate() {
                    if layer.cells.filled() == 0 {
                        continue;
                    }
                    let Some(cells) = self.cells(gpu, id, li, &layer.cells) else { continue };
                    let color = [tint[0], tint[1], tint[2], tint[3] * layer.opacity];
                    for ci in 0..layer.cells.chunk_count() {
                        if layer.cells.chunk_filled(ci) == 0 {
                            continue;
                        }
                        out.push(Draw2d::Tiles(TileChunk {
                            origin,
                            axes: [[ax.x, ax.y, ax.z], down],
                            tile_size: [t.tile_size[0] * sx, t.tile_size[1] * sy],
                            tileset,
                            tile_px: t.tile_px,
                            margin: t.margin,
                            spacing: t.spacing,
                            cells,
                            region: layer.cells.chunk_region(ci),
                            color,
                            layer: layer.z_layer,
                            blend: layer.blend,
                            filter: t.filter,
                            pixel_snap: t.pixel_snap,
                            lit: t.lit,
                        }));
                    }
                }
            }
            TwoD::Shape2d(s) => {
                if let Some(mesh) = self.shape(gpu, id, s) {
                    out.push(Draw2d::Shape(ShapeDraw { mesh, model: cols, color: tint, layer: s.z_layer }));
                }
            }
        }
    }

    /// Frees what no entity in `live` uses: textures not named, cell textures of other entities
    /// (or layers), shape meshes of other keys, and their problems. Waits for the GPU where
    /// needed (Gpu does).
    pub fn collect(&mut self, gpu: &mut Gpu, live: &[(u64, &TwoD)]) {
        let mut paths = BTreeSet::new();
        let mut layers = BTreeSet::new();
        let mut shapes: BTreeMap<u64, Vec<&Arc<Shape2dSpec>>> = BTreeMap::new();
        for (id, t) in live {
            match t {
                TwoD::Sprite(s) => {
                    if let Some(p) = &s.texture {
                        paths.insert(p.as_str());
                    }
                }
                TwoD::Tilemap(m) => {
                    paths.insert(m.tileset.as_str());
                    layers.extend((0..m.layers.len()).map(|i| (*id, i)));
                }
                TwoD::Shape2d(s) => {
                    shapes.entry(s.key).or_default().push(s);
                }
            }
        }
        let dead: Vec<String> = self.textures.keys().filter(|k| !paths.contains(k.as_str())).cloned().collect();
        for k in dead {
            if let Some(TexEntry { current: Some(t), .. }) = self.textures.remove(&k) {
                gpu.destroy_texture(t.id);
            }
            self.errors.remove(&k);
        }
        let dead: Vec<(u64, usize)> = self.cells.keys().filter(|k| !layers.contains(k)).copied().collect();
        for k in dead {
            if let Some(l) = self.cells.remove(&k) {
                gpu.destroy_texture(l.id);
            }
        }
        self.errors.retain(|k, _| !k.starts_with("tilemap ") || layers.iter().any(|(id, li)| *k == format!("tilemap {id} layer {li}")));
        for (key, list) in self.shapes.iter_mut() {
            let users = shapes.get(key).map_or(&[][..], Vec::as_slice);
            list.retain(|(s, mesh)| {
                let used = users.iter().any(|u| Arc::ptr_eq(u, s) || u.same(s));
                if let (false, Ok(m)) = (used, mesh) {
                    gpu.destroy_mesh(*m);
                }
                used
            });
        }
        self.shapes.retain(|_, list| !list.is_empty());
    }

    pub fn stats_json(&self) -> Json {
        Json::obj([
            ("textures", Json::from(self.textures.values().filter(|t| t.current.is_some()).count())),
            ("textures_loading", Json::from(self.loading())),
            ("texture_loads", Json::from(self.texture_loads)),
            ("tile_layers", Json::from(self.cells.len())),
            ("shape_meshes", Json::from(self.shapes.values().flatten().filter(|(_, m)| m.is_ok()).count())),
            ("chunk_uploads", Json::from(self.chunk_uploads)),
            ("errors", Json::arr(self.errors.values().take(MAX_FRAME_PROBLEMS).map(|e| Json::from(e.as_str())))),
        ])
    }
}

/// Why a sprite can't show the sheet frame it asks for (formatted only when reported).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UvProblem {
    /// The texture holds no whole frame.
    NoFrame { size: [u32; 2], frame_px: [u32; 2] },
    /// The frame is past the sheet's last; the last is shown.
    PastSheet { frame: u32, count: u32, shown: u32 },
}

impl std::fmt::Display for UvProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            UvProblem::NoFrame { size, frame_px } => {
                write!(f, "its {}x{} texture holds no {}x{} frame", size[0], size[1], frame_px[0], frame_px[1])
            }
            UvProblem::PastSheet { frame, count, shown } => write!(f, "frame {frame} is past the sheet's {count} frames (showing {shown})"),
        }
    }
}

/// The texture rect a sprite shows at `time_s` on a texture of `size` pixels, and a problem to
/// report (a frame past the sheet is clamped to its last frame). No allocation.
pub fn sprite_uv(s: &SpriteSpec, size: [u32; 2], time_s: f64) -> ([f32; 4], Option<UvProblem>) {
    match (s.uv, s.sheet) {
        (Some(uv), _) => (uv, None),
        (None, None) => ([0.0, 0.0, 1.0, 1.0], None),
        (None, Some(sheet)) => {
            let frame = match s.anim {
                Some(a) => a.first.saturating_add(Flipbook::index(a.count as usize, a.fps, a.playback, time_s) as u32),
                None => s.frame,
            };
            let count = Flipbook::grid_len(size, sheet.frame_px, sheet.margin, sheet.spacing);
            if count == 0 {
                return ([0.0, 0.0, 1.0, 1.0], Some(UvProblem::NoFrame { size, frame_px: sheet.frame_px }));
            }
            let shown = frame.min(count - 1);
            let rect = Flipbook::grid_rect(size, sheet.frame_px, shown, sheet.margin, sheet.spacing).unwrap_or([0.0, 0.0, 1.0, 1.0]);
            (rect, (frame >= count).then_some(UvProblem::PastSheet { frame, count, shown }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(s: &str) -> Json {
        Json::parse(s).unwrap()
    }

    #[test]
    fn cells_and_rows_round_trip() {
        for t in ["0", "1", "65535", "7h", "3dv", "12dhv"] {
            assert_eq!(format_cell(parse_cell(t).unwrap()), t);
        }
        assert_eq!(parse_cell("5r1").unwrap(), TileCell::new(5).rotated_cw(1));
        for bad in ["", "h", "0h", "65536", "3x", "3hh", "5r4", "-1"] {
            assert!(parse_cell(bad).is_err(), "{bad}");
        }
        let row = parse_row("1*3, 0*2,7h", 10).unwrap();
        assert_eq!(row.len(), 6);
        assert!(parse_row("1*11", 10).is_err());
        assert!(parse_row("1*0", 10).is_err());
        let mut g = TileGrid::new(10, 1);
        for (x, c) in row.into_iter().enumerate() {
            g.set(x as u32, 0, c);
        }
        assert_eq!(format_row(&g, 0, 0, 10), "1*3,0*2,7h");
    }

    #[test]
    fn tilemaps_parse_save_and_edit() {
        let v = json(
            r#"{"tileset":"tiles/set.png","tile_px":[16,16],"size":[8,4],"animations":[{"tile":5,"frames":3,"frame_ms":100}],
               "layers":[{"name":"ground","rows":["1*8","2,5,5h"]},{"name":"top","blend":"blended","opacity":0.5,"rows":["","0,9"]}]}"#,
        );
        let t = TilemapSpec::parse(&v).unwrap();
        assert_eq!(t.layers[0].cells.get(1, 1), TileCell::animated(5, 3, 6));
        assert_eq!(t.layers[0].cells.get(2, 1).orient, ORIENT_FLIP_X);
        assert_eq!(t.layers[1].z_layer, 1, "layers stack in order by default");
        assert_eq!(t.layers[1].cells.filled(), 1);
        assert_eq!(t.bounds(), (Vec3::new(0.0, -4.0, 0.0), Vec3::new(8.0, 0.0, 0.0)));
        let saved = t.to_json();
        assert_eq!(TilemapSpec::parse(&saved).unwrap(), t, "{saved}");
        assert!(t.summary_json().to_string().len() < saved.to_string().len() + 64);
        assert!(!t.summary_json().to_string().contains("rows"));
        let mut e = t.clone();
        let edits = parse_tile_edits("top", &json(r#"[[0,0,4],[1,0,"5"],[2,2,"3r2"]]"#)).unwrap();
        assert_eq!(e.apply(&edits).unwrap(), 3);
        assert_eq!(e.layers[1].cells.get(1, 0), TileCell::animated(5, 3, 6), "animation applies to edits");
        assert!(e.layers[0].cells.same_chunk(&t.layers[0].cells, 0), "the untouched layer shares its chunks");
        assert!(e.apply(&parse_tile_edits("nope", &json("[[0,0,1]]")).unwrap()).unwrap_err().contains("ground, top"));
        assert!(e.apply(&parse_tile_edits("top", &json("[[8,0,1]]")).unwrap()).is_err());
        for bad in [
            r#"{"tileset":"a.png","tile_px":[8,8],"size":[2,2],"layers":[{"rows":["1,1,1"]}]}"#,
            r#"{"tileset":"../a.png","tile_px":[8,8],"size":[2,2],"layers":[{}]}"#,
            r#"{"tileset":"a.png","tile_px":[8,8],"size":[2000,2],"layers":[{}]}"#,
            r#"{"tileset":"a.png","tile_px":[8,8],"size":[2,2],"layers":[]}"#,
            r#"{"tileset":"a.png","tile_px":[8,8],"size":[2,2],"layers":[{"name":"a"},{"name":"a"}]}"#,
            r#"{"tileset":"a.png","tile_px":[8,8],"size":[2,2],"layers":[{"cutoff":0.2,"blend":"blended"}]}"#,
        ] {
            assert!(TilemapSpec::parse(&json(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn sprites_parse_and_round_trip() {
        let v = json(
            r#"{"texture":"sprites/hero.png","size":[1,2],"pivot":[0.5,0],"sheet":{"frame_px":[16,32]},
               "anim":{"count":4,"fps":10},"billboard":"cylindrical","blend":"cutout","cutoff":0.3,"filter":"nearest",
               "cast_shadows":true,"lit":true,"z_layer":3,"flip":[true,false]}"#,
        );
        let s = SpriteSpec::parse(&v).unwrap();
        assert_eq!(s.blend, SpriteBlend::Cutout { threshold: 0.3 });
        assert_eq!(SpriteSpec::parse(&s.to_json()).unwrap(), s);
        assert_eq!(SpriteSpec::parse(&json("{}")).unwrap(), SpriteSpec::default());
        assert_eq!(SpriteSpec::default().to_json().to_string(), "{}");
        let (lo, hi) = SpriteSpec { size: [2.0, 1.0], pivot: [0.5, 0.0], ..Default::default() }.bounds();
        assert_eq!((lo, hi), (Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0)));
        for bad in [
            r#"{"frame":2}"#,
            r#"{"texture":"x.jpg"}"#,
            r#"{"cast_shadows":true}"#,
            r#"{"texture":"a.png","uv":[0,0,1,1],"sheet":{"frame_px":[8,8]}}"#,
            r#"{"billboard":"sideways"}"#,
            r#"{"size":[0,1]}"#,
            r#"{"colour":"red"}"#,
        ] {
            assert!(SpriteSpec::parse(&json(bad)).is_err(), "{bad}");
        }
    }

    /// The review's probes: hostile input is refused (never a panic, never seconds of work).
    #[test]
    fn hostile_shape2d_input_is_refused_quickly() {
        let timed = |what: &str, text: String| {
            let t = std::time::Instant::now();
            let r = std::panic::catch_unwind(|| Shape2dSpec::parse(&Json::parse(&text).unwrap()).map(|s| s.mesh().map_or(0, |m| m.vertices.len())));
            let ms = t.elapsed().as_secs_f64() * 1e3;
            eprintln!("{what}: {:?} in {ms:.1} ms", r.as_ref().map(|r| r.as_ref().map_err(|e| &e[..e.len().min(90)])));
            assert!(ms < 1000.0, "{what} took {ms:.0} ms");
            r.unwrap_or_else(|_| panic!("{what} panicked"))
        };
        // Non-ASCII where a hex colour's alpha is sliced.
        assert!(timed("utf-8 colour", "{\"items\":[{\"rect\":[0,0,1,1],\"fill\":\"#12345\u{e9}a\"}]}".into()).is_err());
        let zig: Vec<String> = (0..16_384).map(|i| format!("[{},{}]", i, (i % 2) * 1000)).collect();
        let r = timed(
            "zigzag stroke",
            format!(r#"{{"items":[{{"polyline":[{}],"stroke":{{"width":100000,"join":"round"}}}}],"tolerance":0.00001}}"#, zig.join(",")),
        );
        assert!(r.is_err_and(|e| e.contains("vertices")));
        let dots: Vec<String> = (0..16_000).map(|i| format!(r#"["M",{i},0]"#)).collect();
        let r = timed(
            "16k dots",
            format!(r#"{{"items":[{{"path":[{}],"stroke":{{"width":100000,"cap":"round"}}}}],"tolerance":0.00001}}"#, dots.join(",")),
        );
        assert!(r.is_err());
        let arcs: Vec<String> = (0..15).map(|i| format!(r#"["A",0,0,1000000,1000000,{},24]"#, i * 24)).collect();
        let r = timed("15 huge arcs", format!(r##"{{"items":[{{"path":[{},["Z"]],"fill":"#ffffff"}}],"tolerance":0.00001,"aa":false}}"##, arcs.join(",")));
        assert!(r.is_ok());
        let comb = |n: usize| {
            let mut pts = Vec::new();
            for i in 0..n {
                pts.push(format!("[{i},0],[{}.25,10],[{}.5,0.1]", i, i));
            }
            format!(r##"{{"items":[{{"polygon":[{},[{n},-5],[0,-5]],"fill":"#ffffff"}}],"aa":false}}"##, pts.join(","))
        };
        assert!(timed("comb over the cap", comb(1400)).is_err_and(|e| e.contains("points")));
        assert!(timed("comb at the cap", comb(1364)).is_ok());
        // Round 2: zero-width strokes, endless arcs, thousands of rings, shared point budget.
        let endless = |width: &str| {
            let arcs = vec![r#"["A",0,0,1000000,1000000,0,36000]"#; 63].join(",");
            let items: Vec<String> = (0..256).map(|_| format!(r#"{{"path":[["M",0,0],{arcs}],"stroke":{{"width":{width}}}}}"#)).collect();
            format!(r#"{{"items":[{}],"tolerance":0.00001}}"#, items.join(","))
        };
        assert!(timed("zero-width strokes", endless("0")).is_err_and(|e| e.contains("width")));
        assert!(timed("36000-degree arcs", endless("0.00001")).is_err_and(|e| e.contains("points")));
        let rings: Vec<String> = (0..7280).map(|i| format!(r#"["A",{},{},1,1,0,360],["Z"]"#, (i % 86) * 3, (i / 86) * 3)).collect();
        let r = timed("7280 rings in one fill", format!(r##"{{"items":[{{"path":[{}],"fill":"#ffffff"}}],"tolerance":0.05,"aa":false}}"##, rings.join(",")));
        assert!(r.is_err());
        // The worst shape accepted: two fills at the ear-clipping cap plus convex fills up to
        // the shape's point budget.
        let comb_items = |n: usize| {
            let mut pts = Vec::new();
            for i in 0..n {
                pts.push(format!("[{i},0],[{}.25,10],[{}.5,0.1]", i, i));
            }
            format!(r##"{{"polygon":[{},[{n},-5],[0,-5]],"fill":"#ffffff"}}"##, pts.join(","))
        };
        let mut items = vec![comb_items(1360), comb_items(1360)];
        items.extend((0..80).map(|_| r##"{"circle":[0,0,100],"fill":"#ffffff"}"##.to_owned()));
        let r = timed("worst accepted shape", format!(r#"{{"items":[{}],"tolerance":0.001}}"#, items.join(",")));
        assert!(r.is_ok());
        // Odd geometry: errors or meshes, never panics.
        for (t, ok) in [
            (r##"{"items":[{"polygon":[[0,0],[1,1],[1,0],[0,1]],"fill":"#fff"}]}"##, false), // bow-tie: no net area
            (r##"{"items":[{"polygon":[[0,0],[0,0],[0,0]],"fill":"#fff","stroke":{"width":0.1}}]}"##, false),
            (r##"{"items":[{"polygon":[[0,0],[1,0],[2,0],[3,0]],"stroke":{"width":0.1,"join":"round"}}]}"##, true),
            (r##"{"items":[{"path":[["M",0,0],["L",4,0],["L",4,4],["L",0,4],["Z"],["M",0,0],["L",2,0],["L",2,2],["Z"]],"fill":"#fff"}]}"##, true),
            (r##"{"items":[{"path":[["M",0,0],["C",0,0,0,0,0,0],["Z"]],"stroke":{"width":1,"cap":"round"}}]}"##, true),
            (r##"{"items":[{"path":[["A",0,0,1,1,0,1000000]],"stroke":{"width":0.1}}]}"##, true),
            (r##"{"items":[{"polygon":[[1e6,1e6],[-1e6,-1e6],[1e6,-1e6],[1e6,1e6],[-1e6,1e6]],"fill":"#fff","stroke":{"width":100000,"join":"miter","miter_limit":100}}]}"##, true),
            (r##"{"items":[{"polygon":[[0,0],[1,0],[0,0],[1,0]],"stroke":{"width":0.5,"join":"round"}}]}"##, true),
        ] {
            let r = timed("odd geometry", t.to_owned());
            assert_eq!(r.is_ok(), ok, "{t}: {r:?}");
        }
    }

    #[test]
    fn tilemap_extremes_are_refused_or_bounded() {
        let t = std::time::Instant::now();
        let rows: Vec<String> = (0..1024).map(|_| "\"1*1024\"".to_owned()).collect();
        let layers: Vec<String> = (0..8).map(|i| format!(r#"{{"name":"l{i}","rows":[{}]}}"#, rows.join(","))).collect();
        let text = format!(r#"{{"tileset":"a.png","tile_px":[8,8],"size":[1024,1024],"layers":[{}]}}"#, layers.join(","));
        let spec = TilemapSpec::parse(&Json::parse(&text).unwrap()).unwrap();
        let saved = spec.to_json().to_string();
        eprintln!("8 full 1024 x 1024 layers: parse + save in {:.0} ms, {} bytes", t.elapsed().as_secs_f64() * 1e3, saved.len());
        assert!(saved.len() < 200_000, "run-length rows stay small");
        assert!(parse_row("1*4294967295", 1024).is_err());
        assert!(parse_row("1*99999999999999999999", 1024).is_err());
        assert!(parse_row(",,,", 1024).is_err());
        assert!(parse_cell("\u{e9}").is_err());
        let anim = |tile: u32, frames: u32| {
            let t = format!(r#"{{"tileset":"a.png","tile_px":[8,8],"size":[2,2],"animations":[{{"tile":{tile},"frames":{frames}}}],"layers":[{{}}]}}"#);
            TilemapSpec::parse(&Json::parse(&t).unwrap())
        };
        assert!(anim(65_534, 2).is_ok());
        assert!(anim(65_535, 2).is_err_and(|e| e.contains("past tile 65535")));
    }

    #[test]
    fn sheet_frames_are_computed_and_clamped() {
        let s = SpriteSpec::parse(&json(r#"{"texture":"a.png","sheet":{"frame_px":[16,16]},"frame":9}"#)).unwrap();
        let (uv, problem) = sprite_uv(&s, [64, 32], 0.0);
        assert_eq!(uv, [0.75, 0.5, 1.0, 1.0], "frame 9 of 8 shows the last (7: column 3, row 1)");
        assert!(problem.is_some_and(|p| p.to_string().contains("past the sheet's 8 frames")));
        let (uv, problem) = sprite_uv(&SpriteSpec { frame: 5, ..s.clone() }, [64, 32], 0.0);
        assert_eq!((uv, problem), ([0.25, 0.5, 0.5, 1.0], None));
        let a = SpriteSpec::parse(&json(r#"{"texture":"a.png","sheet":{"frame_px":[16,16]},"anim":{"first":4,"count":4,"fps":10}}"#)).unwrap();
        assert_eq!(sprite_uv(&a, [64, 32], 0.25).0, [0.5, 0.5, 0.75, 1.0], "frame 4 + 2");
        let (uv, problem) = sprite_uv(&s, [8, 8], 0.0);
        assert_eq!(uv, [0.0, 0.0, 1.0, 1.0]);
        assert!(problem.is_some_and(|p| p.to_string().contains("holds no 16x16 frame")));
    }

    #[test]
    fn shapes_parse_tessellate_and_key_by_content() {
        let v = json(
            r##"{"items":[{"rect":[0,0,2,1],"radius":0.2,"fill":"#ff8800","stroke":{"width":0.05,"color":"#ffffff80","join":"round"}},
                        {"circle":[3,0.5,0.5],"fill":[0,0.5,1]},
                        {"polyline":[[0,2],[1,3],[2,2]],"stroke":{"width":0.1,"cap":"round"}},
                        {"path":[["M",0,0],["Q",1,1,2,0],["A",3,0,1,1,180,-180],["Z"]],"fill":"#123456"}]}"##,
        );
        let s = Shape2dSpec::parse(&v).unwrap();
        assert!(s.mesh().unwrap().indices.len() > 300);
        assert!(s.bounds().0.x < 0.0 && s.bounds().1.x > 3.4 && s.bounds().1.y > 3.0);
        let again = Shape2dSpec::parse(&s.json).unwrap();
        assert_eq!(again, s, "canonical JSON reads back to the same shape");
        assert!(Arc::ptr_eq(&again, &s), "an identical shape is the same spec (interned)");
        for bad in [
            r#"{"items":[]}"#,
            r#"{"items":[{"rect":[0,0,1,1]}]}"#,
            r#"{"items":[{"rect":[0,0,1,1],"circle":[0,0,1],"fill":[1,1,1]}]}"#,
            r#"{"items":[{"polyline":[[0,0],[1,1]],"fill":[1,1,1]}]}"#,
            r#"{"items":[{"circle":[0,0,-1],"fill":[1,1,1]}]}"#,
            r#"{"items":[{"path":[["X",1]],"fill":[1,1,1]}]}"#,
            r#"{"items":[{"rect":[0,0,1,1],"fill":"red"}]}"#,
        ] {
            assert!(Shape2dSpec::parse(&json(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn budgets_tell_colliding_keys_apart() {
        let a = Shape2dSpec::parse(&json(r##"{"items":[{"rect":[0,0,1,1],"fill":"#ff0000"}],"z_layer":-77}"##)).unwrap();
        // A forged hash collision: the same key, another description.
        let other = Shape2dSpec::prepare(&json(r##"{"items":[{"rect":[0,0,2,1],"fill":"#ff0000"}],"z_layer":-77}"##)).unwrap();
        let b = Arc::new(Shape2dSpec { key: a.key, vertices: 1000, ..other });
        assert!(!a.same(&b) && a.same(&a));
        let mut budget = ShapeBudget::default();
        budget.charge(&a);
        assert_eq!(budget.after(Some(&b), None), a.vertices + 1000, "a collision is still a new shape");
        budget.charge(&b);
        budget.charge(&a);
        assert_eq!((budget.distinct(), budget.vertices()), (2, a.vertices + 1000));
        assert_eq!(budget.after(Some(&b), Some(&a)), budget.vertices(), "b is in already; a has another user");
        budget.release(&a);
        assert_eq!(budget.after(Some(&b), Some(&a)), 1000, "a's last user switches to b");
        budget.release(&b);
        assert_eq!((budget.distinct(), budget.vertices()), (1, a.vertices));
        budget.release(&a);
        budget.release(&a); // more releases than charges: ignored
        assert_eq!((budget.distinct(), budget.vertices()), (0, 0));
    }

    #[test]
    fn undo_and_restore_are_checked_against_the_budget() {
        use crate::world::{EntityKey, Shape, SpawnSpec, World};
        let heavy = |tag: u32| {
            let items = vec![r##"{"circle":[0,0,100],"fill":"#ffffff"}"##; 90].join(",");
            let text = format!(r#"{{"items":[{items}],"tolerance":0.001,"z_layer":{tag}}}"#);
            deferred(|| parse("shape2d", &json(&text))).unwrap()
        };
        let spawn = |w: &mut World, name: &str, t: TwoD| {
            let mut s = SpawnSpec::new(Shape::Shape2d);
            s.name = Some(name.into());
            s.two_d = Some(t);
            w.spawn(s, "test").map(|e| e.id)
        };
        let mut w = World::default();
        spawn(&mut w, "gone", heavy(10_000)).unwrap();
        let removed = w.delete(&EntityKey::Path("/gone".into()), None).unwrap().removed.remove(0);
        let mut k = 0;
        while spawn(&mut w, &format!("h{k}"), heavy(k)).is_ok() {
            k += 1;
        }
        // Putting the deleted entity back (journal undo) would pass the budget: refused.
        let e = w.check_put(&removed).unwrap_err();
        assert_eq!((e.code, e.detail("field").and_then(Json::as_str)), ("limit", Some("shape2d")));
        w.delete(&EntityKey::Path("/h0".into()), None).unwrap();
        w.check_put(&removed).unwrap();
        // A put that keeps its own shape costs nothing.
        let h1 = w.get(w.find_path("/h1").unwrap()).unwrap().clone();
        w.check_put(&h1).unwrap();
    }

    #[test]
    fn loader_panics_become_errors() {
        let r = guarded::<()>("tiles/a.png", || panic!("boom"));
        assert!(r.is_err_and(|e| e.starts_with("tiles/a.png") && e.contains("boom")));
    }
}

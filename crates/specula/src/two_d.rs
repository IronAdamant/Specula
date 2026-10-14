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

#[derive(Clone, Debug)]
pub struct Shape2dSpec {
    /// The canonical description (what is saved and hashed).
    pub json: Json,
    pub z_layer: i32,
    /// The tessellation (local XY plane).
    pub mesh: Arc<ShapeMesh>,
    /// Identifies the mesh (hash of the canonical JSON).
    pub key: u64,
    /// Local bounds of the mesh.
    pub bounds: (Vec3, Vec3),
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

impl Shape2dSpec {
    pub fn parse(v: &Json) -> Result<Shape2dSpec, String> {
        let w = "shape2d";
        keys(v, w, SHAPE_KEYS)?;
        let tolerance = num(v, "tolerance", w, 1e-5, 10.0)?.unwrap_or(0.005);
        let aa = boolean(v, "aa", w)?.unwrap_or(true);
        let z_layer = int(v, "z_layer", w, -1_000_000, 1_000_000)?.unwrap_or(0) as i32;
        let items = opt(v, "items").and_then(Json::as_array).ok_or("'shape2d.items' (a list of shapes) is required")?;
        if items.is_empty() || items.len() > MAX_SHAPE_ITEMS {
            return Err(format!("'shape2d.items' must hold 1..{MAX_SHAPE_ITEMS} items"));
        }
        let mut mesh = ShapeMesh::default();
        let mut budget = MAX_SHAPE_INPUT;
        let mut canon_items = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
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
            let fill_color = opt(item, "fill").map(|c| rgba(c, &format!("{what}.fill"))).transpose()?;
            let stroke_style = match opt(item, "stroke") {
                None => None,
                Some(s) => {
                    let sw = format!("{what}.stroke");
                    keys(s, &sw, STROKE_KEYS)?;
                    let limit = num(s, "miter_limit", &sw, 1.0, 100.0)?.unwrap_or(4.0);
                    Some(StrokeStyle {
                        width: num(s, "width", &sw, 0.0, 1e5)?.unwrap_or(0.05),
                        color: opt(s, "color").map(|c| rgba(c, &format!("{sw}.color"))).transpose()?.unwrap_or([255; 4]),
                        join: choice(s, "join", &sw, &[("miter", LineJoin::Miter { limit }), ("round", LineJoin::Round), ("bevel", LineJoin::Bevel)])?
                            .unwrap_or(LineJoin::Miter { limit }),
                        cap: choice(s, "cap", &sw, &[("butt", LineCap::Butt), ("square", LineCap::Square), ("round", LineCap::Round)])?
                            .unwrap_or_default(),
                    })
                }
            };
            if fill_color.is_none() && stroke_style.is_none() {
                return Err(format!("'{what}' needs a 'fill' colour, a 'stroke', or both"));
            }
            if kind == "polyline" && fill_color.is_some() {
                return Err(format!("'{what}': a polyline is open; stroke it (or use polygon to fill)"));
            }
            let contours = path.flatten(tolerance).map_err(|e| format!("'{what}': {e}"))?;
            if let Some(c) = fill_color {
                vector::fill(&contours, c, aa, &mut mesh).map_err(|e| format!("'{what}': {e}"))?;
                canon.push(("fill", rgba_json(c)));
            }
            if let Some(s) = stroke_style {
                vector::stroke(&contours, &s, tolerance, aa, &mut mesh).map_err(|e| format!("'{what}': {e}"))?;
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
            if mesh.vertices.len() > vector::MAX_SHAPE_POINTS * 8 {
                return Err(format!("'{w}' tessellates to more than {} vertices; simplify it or raise 'tolerance'", vector::MAX_SHAPE_POINTS * 8));
            }
            canon_items.push(Json::obj(canon));
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
        let (mut lo, mut hi) = (Vec3::new(f32::INFINITY, f32::INFINITY, 0.0), Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, 0.0));
        for vtx in &mesh.vertices {
            let p = Vec3::new(vtx.position[0], vtx.position[1], 0.0);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        if mesh.is_empty() {
            (lo, hi) = (Vec3::ZERO, Vec3::ZERO);
        }
        Ok(Shape2dSpec { json, z_layer, mesh: Arc::new(mesh), key, bounds: (lo, hi) })
    }
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
            TwoD::Shape2d(s) => s.bounds,
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
                let mesh = s.mesh.vertices.len() * 28 + s.mesh.indices.len() * 4;
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
            TwoD::Shape2d(s) => Json::obj([("z_layer", Json::from(s.z_layer as i64)), ("triangles", Json::from(s.mesh.indices.len() / 3))]),
        }
    }
}

/// Parses the component of `shape` (`sprite`, `tilemap` or `shape2d`) from its JSON value.
pub fn parse(shape: &str, v: &Json) -> Result<TwoD, String> {
    match shape {
        "sprite" => SpriteSpec::parse(v).map(|s| TwoD::Sprite(Box::new(s))),
        "tilemap" => TilemapSpec::parse(v).map(|t| TwoD::Tilemap(Arc::new(t))),
        "shape2d" => Shape2dSpec::parse(v).map(|s| TwoD::Shape2d(Arc::new(s))),
        _ => Err(format!("'{shape}' has no 2D component")),
    }
}

/// A world value error about a 2D component.
pub fn invalid(field: &str, message: String) -> WorldError {
    WorldError::new("invalid_value", message).with("field", field)
}

// ---------------------------------------------------------------------------------------------
// GPU side
// ---------------------------------------------------------------------------------------------

/// A texture loaded from the assets root.
struct LoadedTexture {
    id: TextureId,
    size: [u32; 2],
}

/// One tile map layer's cell texture, and the grid it holds (to upload only changed chunks).
struct LayerTexture {
    id: TextureId,
    grid: TileGrid,
}

/// The GPU resources 2D entities draw with: textures by asset path, tile maps' cell textures by
/// entity, shape meshes by shape key. Everything is created on first use and freed when no
/// entity uses it any more ([`TwoDGpu::collect`]).
pub struct TwoDGpu {
    assets: PathBuf,
    textures: BTreeMap<String, Result<LoadedTexture, String>>,
    cells: BTreeMap<(u64, usize), LayerTexture>,
    shapes: BTreeMap<u64, Result<MeshId, String>>,
    /// Reused per-frame scratch.
    bytes: Vec<u8>,
    /// Problems met (missing files, bad PNGs, GPU errors), newest last, at most 16.
    pub errors: Vec<String>,
    /// Cell chunks uploaded since start-up (edits cost only the chunks they touch).
    pub chunk_uploads: u64,
}

impl TwoDGpu {
    pub fn new(assets: &FsPath) -> TwoDGpu {
        TwoDGpu {
            assets: assets.to_path_buf(),
            textures: BTreeMap::new(),
            cells: BTreeMap::new(),
            shapes: BTreeMap::new(),
            bytes: Vec::new(),
            errors: Vec::new(),
            chunk_uploads: 0,
        }
    }

    fn note(&mut self, e: String) {
        if !self.errors.contains(&e) {
            if self.errors.len() >= 16 {
                self.errors.remove(0);
            }
            self.errors.push(e);
        }
    }

    fn texture(&mut self, gpu: &mut Gpu, path: &str) -> Option<(TextureId, [u32; 2])> {
        if !self.textures.contains_key(path) {
            let file = self.assets.join(path);
            let loaded = std::fs::read(&file)
                .map_err(|e| format!("{path}: {e}"))
                .and_then(|bytes| png::decode(&bytes).map_err(|e| format!("{path}: not a readable PNG ({e:?})")))
                .and_then(|img| {
                    gpu.upload_texture(img.width, img.height, TextureFormat::Rgba8Srgb, &img.rgba)
                        .map(|id| LoadedTexture { id, size: [img.width, img.height] })
                        .map_err(|e| format!("{path}: {e}"))
                });
            if let Err(e) = &loaded {
                self.note(format!("texture {e}"));
            }
            self.textures.insert(path.to_owned(), loaded);
        }
        self.textures.get(path).and_then(|t| t.as_ref().ok()).map(|t| (t.id, t.size))
    }

    /// Forget a texture (its file changed): reloaded on next use.
    pub fn reload_texture(&mut self, gpu: &mut Gpu, path: &str) {
        if let Some(Ok(t)) = self.textures.remove(path) {
            gpu.destroy_texture(t.id);
        }
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
        match self.cells.get_mut(&key) {
            None => match gpu.upload_texture(grid.width(), grid.height(), TextureFormat::Rgba8Unorm, &grid.encode()) {
                Ok(tid) => {
                    self.cells.insert(key, LayerTexture { id: tid, grid: grid.clone() });
                    Some(tid)
                }
                Err(e) => {
                    self.note(format!("tile map {id}: {e}"));
                    None
                }
            },
            Some(layer) => {
                if (0..grid.chunk_count()).any(|i| !grid.same_chunk(&layer.grid, i)) {
                    for i in 0..grid.chunk_count() {
                        if grid.same_chunk(&layer.grid, i) {
                            continue;
                        }
                        let r = grid.chunk_region(i);
                        grid.encode_region(r, &mut self.bytes);
                        if let Err(e) = gpu.update_texture(layer.id, r[0], r[1], r[2], r[3], &self.bytes) {
                            self.errors.push(format!("tile map {id}: {e}"));
                        }
                        self.chunk_uploads += 1;
                    }
                    layer.grid = grid.clone();
                }
                Some(layer.id)
            }
        }
    }

    fn shape(&mut self, gpu: &mut Gpu, s: &Shape2dSpec) -> Option<MeshId> {
        if s.mesh.is_empty() {
            return None;
        }
        if !self.shapes.contains_key(&s.key) {
            let r = gpu.upload_mesh(&s.mesh.vertices, &s.mesh.indices).map_err(|e| format!("shape2d: {e}"));
            if let Err(e) = &r {
                self.note(e.clone());
            }
            self.shapes.insert(s.key, r);
        }
        self.shapes.get(&s.key).and_then(|r| r.as_ref().ok().copied())
    }

    /// Appends the draws of 2D entity `e` (world transform `model`, sRGB `tint`) at `time_s`.
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
                    let Some((tex, size)) = self.texture(gpu, path) else { return };
                    sprite.texture = Some(tex);
                    sprite.uv = match (s.uv, s.sheet) {
                        (Some(uv), _) => uv,
                        (None, Some(sheet)) => {
                            let frame = match s.anim {
                                Some(a) => {
                                    let book = Flipbook { frames: vec![[0.0; 4]; a.count as usize], fps: a.fps, playback: a.playback };
                                    a.first + book.frame_index(time_s as f64) as u32
                                }
                                None => s.frame,
                            };
                            let frames = Flipbook::grid(size, sheet.frame_px, frame + 1, sheet.margin, sheet.spacing);
                            frames.get(frame as usize).copied().unwrap_or([0.0, 0.0, 1.0, 1.0])
                        }
                        (None, None) => [0.0, 0.0, 1.0, 1.0],
                    };
                }
                out.push(Draw2d::Sprite(sprite));
            }
            TwoD::Tilemap(t) => {
                let Some((tileset, _)) = self.texture(gpu, &t.tileset) else { return };
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
                if let Some(mesh) = self.shape(gpu, s) {
                    out.push(Draw2d::Shape(ShapeDraw { mesh, model: cols, color: tint, layer: s.z_layer }));
                }
            }
        }
    }

    /// Frees what no entity in `live` uses: textures not named, cell textures of other entities
    /// (or layers), shape meshes of other keys. Waits for the GPU where needed (Gpu does).
    pub fn collect(&mut self, gpu: &mut Gpu, live: &[(u64, &TwoD)]) {
        let mut paths = BTreeSet::new();
        let mut layers = BTreeSet::new();
        let mut shapes = BTreeSet::new();
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
                    shapes.insert(s.key);
                }
            }
        }
        let dead: Vec<String> = self.textures.keys().filter(|k| !paths.contains(k.as_str())).cloned().collect();
        for k in dead {
            if let Some(Ok(t)) = self.textures.remove(&k) {
                gpu.destroy_texture(t.id);
            }
        }
        let dead: Vec<(u64, usize)> = self.cells.keys().filter(|k| !layers.contains(k)).copied().collect();
        for k in dead {
            if let Some(l) = self.cells.remove(&k) {
                gpu.destroy_texture(l.id);
            }
        }
        let dead: Vec<u64> = self.shapes.keys().filter(|k| !shapes.contains(k)).copied().collect();
        for k in dead {
            if let Some(Ok(m)) = self.shapes.remove(&k) {
                gpu.destroy_mesh(m);
            }
        }
    }

    /// Paths of the textures in use (for change polling).
    pub fn texture_paths(&self) -> impl Iterator<Item = &str> {
        self.textures.keys().map(String::as_str)
    }

    pub fn stats_json(&self) -> Json {
        Json::obj([
            ("textures", Json::from(self.textures.values().filter(|t| t.is_ok()).count())),
            ("tile_layers", Json::from(self.cells.len())),
            ("shape_meshes", Json::from(self.shapes.values().filter(|s| s.is_ok()).count())),
            ("chunk_uploads", Json::from(self.chunk_uploads)),
            ("errors", Json::arr(self.errors.iter().map(|e| Json::from(e.as_str())))),
        ])
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

    #[test]
    fn shapes_parse_tessellate_and_key_by_content() {
        let v = json(
            r##"{"items":[{"rect":[0,0,2,1],"radius":0.2,"fill":"#ff8800","stroke":{"width":0.05,"color":"#ffffff80","join":"round"}},
                        {"circle":[3,0.5,0.5],"fill":[0,0.5,1]},
                        {"polyline":[[0,2],[1,3],[2,2]],"stroke":{"width":0.1,"cap":"round"}},
                        {"path":[["M",0,0],["Q",1,1,2,0],["A",3,0,1,1,180,-180],["Z"]],"fill":"#123456"}]}"##,
        );
        let s = Shape2dSpec::parse(&v).unwrap();
        assert!(s.mesh.indices.len() > 300);
        assert!(s.bounds.0.x < 0.0 && s.bounds.1.x > 3.4 && s.bounds.1.y > 3.0);
        let again = Shape2dSpec::parse(&s.json).unwrap();
        assert_eq!(again, s, "canonical JSON reads back to the same shape");
        assert_eq!(again.mesh, s.mesh);
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
}

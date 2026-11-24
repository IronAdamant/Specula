//! The frame data written beside a baked atlas (`assets/sprites/<name>.json`, projet file kind
//! `sprite_sheet`): where each cell is, what it shows (clip, frame, direction), how long each
//! frame lasts, the pivot on the model's ground and the metres a pixel stands for, and for each
//! strip the 2D path's own sprite object that plays it (`world_spawn {shape: "sprite", sprite:
//! <strip.sprite>}`), so a 2D game needs no reader of its own.
//!
//! [`SheetData::parse`] is the reader: it checks the file whole (every cell inside the atlas, a
//! strip's frames consecutive in the grid the 2D path reads, every playback object one the 2D
//! path parses and pointing at its own strip), and it is the project's validator of the kind.

use crate::two_d::{self, SpriteSpec};
use specula_core::Json;

/// The format's name and version (in every file).
pub const FORMAT: &str = "specula.sprite_sheet";
pub const VERSION: u64 = 2;
/// The most strips and cells a file may list (a bake makes at most `plan::MAX_CELLS` cells).
pub const MAX_STRIPS: usize = 1024;
pub const MAX_CELLS: usize = 4096;
/// Longest name (clip, direction, event) a file may hold.
pub const MAX_NAME: usize = 64;

/// One strip: a clip (or the still) from one direction, its frames consecutive.
#[derive(Clone, Debug, PartialEq)]
pub struct StripData {
    /// `None`: the still.
    pub clip: Option<String>,
    pub direction: String,
    pub facing_deg: f32,
    pub first: u32,
    pub count: u32,
    pub fps: f32,
    pub looping: bool,
    pub duration_s: f32,
    pub distance_m: f32,
    pub events: Vec<(String, u32)>,
    /// The clip's feet (as `anim_check` finds them) and, per frame, which are down.
    pub feet: Vec<String>,
    pub feet_down: Vec<Vec<u8>>,
}

/// The shadow blob's cell (`shadow: "blob"`): alpha only, centred on its own pivot.
#[derive(Clone, Debug, PartialEq)]
pub struct ShadowData {
    pub frame: u32,
    pub rect: [u32; 4],
    pub pivot: [u32; 2],
    pub radii_px: [f32; 2],
}

/// One cell.
#[derive(Clone, Debug, PartialEq)]
pub struct CellData {
    pub rect: [u32; 4],
    pub strip: u32,
    pub frame: u32,
    pub time_s: f32,
}

/// A sprite sheet's frame data.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetData {
    /// The atlas, relative to the assets root (what a sprite's `texture` names).
    pub texture: String,
    pub size: [u32; 2],
    pub cell: [u32; 2],
    pub columns: u32,
    /// The model's origin on its ground: pixels from each cell's top left.
    pub pivot: [u32; 2],
    pub metres_per_pixel: f32,
    pub view: String,
    pub elevation_deg: f32,
    pub supersample: u32,
    pub pixel_art: bool,
    pub light: String,
    /// What it was baked from (recipe, params, seed, the model's name and content hash).
    pub source: Json,
    /// Pixel art drawn with an outline of one palette colour; frames taken in place.
    pub outline: bool,
    pub in_place: bool,
    pub strips: Vec<StripData>,
    pub cells: Vec<CellData>,
    pub shadow: Option<ShadowData>,
}

/// A problem with a file: where and why.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetError {
    pub at: String,
    pub message: String,
}

fn bad(at: impl Into<String>, message: impl Into<String>) -> SheetError {
    SheetError { at: at.into(), message: message.into() }
}

impl SheetData {
    /// The duration of each frame of a strip (seconds).
    pub fn frame_s(s: &StripData) -> f32 {
        if s.fps > 0.0 {
            1.0 / s.fps
        } else {
            0.0
        }
    }

    /// The 2D path's sprite object that plays strip `i` (sheet frames `first .. first + count`
    /// at its rate), standing on its pivot, one pixel `metres_per_pixel` across.
    pub fn sprite(&self, i: usize) -> Json {
        let s = &self.strips[i];
        let size = [self.cell[0] as f32 * self.metres_per_pixel, self.cell[1] as f32 * self.metres_per_pixel];
        let pivot = [self.pivot[0] as f32 / self.cell[0] as f32, 1.0 - self.pivot[1] as f32 / self.cell[1] as f32];
        let mut spec = SpriteSpec {
            texture: Some(self.texture.clone()),
            size,
            pivot,
            sheet: Some(two_d::Sheet { frame_px: self.cell, margin: 0, spacing: 0 }),
            frame: s.first,
            ..SpriteSpec::default()
        };
        if s.count > 1 {
            spec.anim = Some(two_d::Anim {
                first: s.first,
                count: s.count,
                fps: s.fps,
                playback: if s.looping { specula_gpu::Playback::Loop } else { specula_gpu::Playback::Once },
            });
        }
        if self.pixel_art {
            spec.filter = specula_gpu::TextureFilter::Nearest;
            spec.pixel_snap = true;
        }
        spec.to_json()
    }

    /// The 2D path's sprite object that draws the shadow blob (one frame, on its own pivot,
    /// which is to stand where the model's pivot stands, drawn under it).
    pub fn shadow_sprite(&self, sh: &ShadowData) -> Json {
        let mut spec = SpriteSpec {
            texture: Some(self.texture.clone()),
            size: [self.cell[0] as f32 * self.metres_per_pixel, self.cell[1] as f32 * self.metres_per_pixel],
            pivot: [sh.pivot[0] as f32 / self.cell[0] as f32, 1.0 - sh.pivot[1] as f32 / self.cell[1] as f32],
            sheet: Some(two_d::Sheet { frame_px: self.cell, margin: 0, spacing: 0 }),
            frame: sh.frame,
            ..SpriteSpec::default()
        };
        if self.pixel_art {
            spec.filter = specula_gpu::TextureFilter::Nearest;
            spec.pixel_snap = true;
        }
        spec.to_json()
    }

    pub fn to_json(&self) -> Json {
        let pair = |a: [u32; 2]| Json::arr(a.map(Json::from));
        let strips = self.strips.iter().enumerate().map(|(i, s)| {
            let mut j = Json::obj([
                ("clip", s.clip.as_deref().map_or(Json::Null, Json::from)),
                ("direction", Json::from(s.direction.as_str())),
                ("facing_deg", Json::from(s.facing_deg)),
                ("first", Json::from(s.first)),
                ("count", Json::from(s.count)),
                ("fps", Json::from(s.fps)),
                ("frame_s", Json::from(Self::frame_s(s))),
                ("playback", Json::from(if s.looping { "loop" } else { "once" })),
                ("duration_s", Json::from(s.duration_s)),
                ("distance_m", Json::from(s.distance_m)),
            ]);
            if !s.events.is_empty() {
                j.set("events", Json::arr(s.events.iter().map(|(n, f)| Json::obj([("name", Json::from(n.as_str())), ("frame", Json::from(*f))]))));
            }
            if !s.feet.is_empty() {
                j.set("feet", Json::arr(s.feet.iter().map(|n| Json::from(n.as_str()))));
                j.set("feet_down", Json::arr(s.feet_down.iter().map(|down| Json::arr(down.iter().map(|&k| Json::from(s.feet.get(k as usize).map_or("", String::as_str)))))));
            }
            j.set("sprite", self.sprite(i));
            j
        });
        // No list of cells: every cell is where the grid puts it (a strip's frame k is sheet
        // frame first + k), and its time is the strip's (frames split a loop's cycle; a clip
        // played once shows its first and last pose), so the reader derives them.
        let mut out = Json::obj([
            ("format", Json::from(FORMAT)),
            ("version", Json::from(VERSION)),
            ("texture", Json::from(self.texture.as_str())),
            ("size", pair(self.size)),
            ("cell", pair(self.cell)),
            ("columns", Json::from(self.columns)),
            ("pivot", pair(self.pivot)),
            ("pivot_is", Json::from("the model's origin on its ground, in pixels from each cell's top left")),
            ("cells_are", Json::from("a grid of cell-sized frames, columns wide: a strip's frame k is sheet frame first + k, at column (first + k) % columns and row (first + k) / columns; it lasts frame_s")),
            ("metres_per_pixel", Json::from(self.metres_per_pixel)),
            ("projection", Json::from("orthographic")),
            ("view", Json::from(self.view.as_str())),
            ("elevation_deg", Json::from(self.elevation_deg)),
            ("alpha", Json::from(if self.pixel_art { "binary: from the ID buffer, half a pixel covered or more" } else { "coverage: the share of a pixel's samples the model covers in the ID buffer" })),
            ("supersample", Json::from(self.supersample)),
            ("pixel_art", Json::from(self.pixel_art)),
            ("light", Json::from(self.light.as_str())),
            ("source", self.source.clone()),
            ("outline", Json::from(self.outline)),
            ("in_place", Json::from(self.in_place)),
            ("strips", Json::arr(strips)),
        ]);
        if let Some(sh) = &self.shadow {
            out.set(
                "shadow",
                Json::obj([
                    ("frame", Json::from(sh.frame)),
                    ("rect", Json::arr(sh.rect.map(Json::from))),
                    ("pivot", pair(sh.pivot)),
                    ("radii_px", Json::arr(sh.radii_px.map(Json::from))),
                    ("is", Json::from("a soft shadow blob, alpha only (black): draw its sprite under the model's, its pivot where the model's pivot stands")),
                    ("sprite", self.shadow_sprite(sh)),
                ]),
            );
        }
        out
    }

    /// Reads and checks a file's text (see the module docs).
    pub fn parse(text: &str) -> Result<SheetData, SheetError> {
        let j = Json::parse(text).map_err(|e| bad("", format!("not JSON: {e}")))?;
        let s = |k: &str| j.get(k).and_then(Json::as_str).ok_or_else(|| bad(k, format!("'{k}' must be a string")));
        let u = |v: Option<&Json>, at: &str, lo: u64, hi: u64| -> Result<u32, SheetError> {
            v.and_then(Json::as_u64).filter(|x| (lo..=hi).contains(x)).map(|x| x as u32).ok_or_else(|| bad(at, format!("'{at}' must be an integer {lo}..{hi}")))
        };
        let f = |v: Option<&Json>, at: &str, lo: f64, hi: f64| -> Result<f32, SheetError> {
            v.and_then(Json::as_f64).filter(|x| x.is_finite() && (lo..=hi).contains(x)).map(|x| x as f32).ok_or_else(|| bad(at, format!("'{at}' must be a number {lo}..{hi}")))
        };
        let pair = |k: &str, lo: u64, hi: u64| -> Result<[u32; 2], SheetError> {
            let a = j.get(k).and_then(Json::as_array).filter(|a| a.len() == 2).ok_or_else(|| bad(k, format!("'{k}' must be two integers")))?;
            Ok([u(a.first(), k, lo, hi)?, u(a.get(1), k, lo, hi)?])
        };
        let name = |v: Option<&Json>, at: &str| -> Result<String, SheetError> {
            v.and_then(Json::as_str).filter(|n| !n.is_empty() && n.len() <= MAX_NAME).map(str::to_owned).ok_or_else(|| bad(at, format!("'{at}' must be a name of 1 to {MAX_NAME} bytes")))
        };
        if s("format")? != FORMAT {
            return Err(bad("format", format!("'format' must be \"{FORMAT}\"")));
        }
        let version = j.get("version").and_then(Json::as_u64).filter(|v| (1..=VERSION).contains(v)).ok_or_else(|| bad("version", format!("'version' must be 1 or {VERSION}")))?;
        let texture = s("texture")?.to_owned();
        two_d::check_image_path("texture", &texture).map_err(|m| bad("texture", m))?;
        let size = pair("size", 1, u64::from(super::plan::MAX_ATLAS_SIDE))?;
        let cell = pair("cell", 1, u64::from(super::plan::MAX_CELL_PX))?;
        let columns = u(j.get("columns"), "columns", 1, 4096)?;
        if columns * cell[0] != size[0] || size[1] % cell[1] != 0 {
            return Err(bad("size", format!("a {}x{} atlas is not {columns} columns of {}x{} cells", size[0], size[1], cell[0], cell[1])));
        }
        let pivot = pair("pivot", 0, u64::from(super::plan::MAX_CELL_PX))?;
        if pivot[0] > cell[0] || pivot[1] > cell[1] {
            return Err(bad("pivot", "the pivot must be inside the cell"));
        }
        let metres_per_pixel = f(j.get("metres_per_pixel"), "metres_per_pixel", 1e-7, 1e4)?;
        let frames_in_atlas = columns * (size[1] / cell[1]);
        let mut strips = Vec::new();
        let list = j.get("strips").and_then(Json::as_array).filter(|a| !a.is_empty() && a.len() <= MAX_STRIPS).ok_or_else(|| bad("strips", format!("'strips' must be a list of 1 to {MAX_STRIPS}")))?;
        for (i, v) in list.iter().enumerate() {
            let at = |k: &str| format!("strips[{i}].{k}");
            let clip = match v.get("clip") {
                None | Some(Json::Null) => None,
                c => Some(name(c, &at("clip"))?),
            };
            let first = u(v.get("first"), &at("first"), 0, 65_535)?;
            let count = u(v.get("count"), &at("count"), 1, 4096)?;
            if first + count > frames_in_atlas {
                return Err(bad(at("count"), format!("frames {first}..{} run past the atlas's {frames_in_atlas}", first + count)));
            }
            let looping = match v.get("playback").and_then(Json::as_str) {
                Some("loop") => true,
                Some("once") => false,
                _ => return Err(bad(at("playback"), "'playback' must be \"loop\" or \"once\"")),
            };
            let mut events = Vec::new();
            for (k, e) in v.get("events").and_then(Json::as_array).unwrap_or(&[]).iter().enumerate() {
                let frame = u(e.get("frame"), &format!("strips[{i}].events[{k}].frame"), 0, u64::from(count - 1))?;
                events.push((name(e.get("name"), &format!("strips[{i}].events[{k}].name"))?, frame));
            }
            // The feet, and per frame the ones down (names among the feet).
            let mut feet = Vec::new();
            for (k, n) in v.get("feet").and_then(Json::as_array).unwrap_or(&[]).iter().enumerate().take(16) {
                feet.push(name(Some(n), &format!("strips[{i}].feet[{k}]"))?);
            }
            let mut feet_down = Vec::new();
            if let Some(per) = v.get("feet_down") {
                let per = per.as_array().filter(|a| a.len() == count as usize).ok_or_else(|| bad(at("feet_down"), format!("'feet_down' must list each of the strip's {count} frames")))?;
                for (k, down) in per.iter().enumerate() {
                    let down = down.as_array().ok_or_else(|| bad(format!("strips[{i}].feet_down[{k}]"), "a frame's feet down are a list of the strip's feet"))?;
                    let mut ks = Vec::new();
                    for d in down {
                        let f = d.as_str().and_then(|d| feet.iter().position(|n| n == d)).ok_or_else(|| bad(format!("strips[{i}].feet_down[{k}]"), "a foot down must be one of the strip's 'feet'"))?;
                        ks.push(f as u8);
                    }
                    feet_down.push(ks);
                }
            }
            let strip = StripData {
                feet,
                feet_down,
                clip,
                direction: name(v.get("direction"), &at("direction"))?,
                facing_deg: f(v.get("facing_deg"), &at("facing_deg"), 0.0, 360.0)?,
                first,
                count,
                fps: f(v.get("fps"), &at("fps"), 0.0, 1000.0)?,
                looping,
                duration_s: f(v.get("duration_s"), &at("duration_s"), 0.0, 1e5)?,
                distance_m: f(v.get("distance_m"), &at("distance_m"), -1e5, 1e5)?,
                events,
            };
            // The playback object is one the 2D path takes, and plays this strip's frames.
            let sprite = v.get("sprite").ok_or_else(|| bad(at("sprite"), "'sprite' (the 2D path's sprite object) is missing"))?;
            let spec = SpriteSpec::parse(sprite).map_err(|m| bad(at("sprite"), m))?;
            let plays = match (spec.sheet, spec.anim) {
                (Some(sh), Some(a)) => sh.frame_px == cell && sh.margin == 0 && sh.spacing == 0 && a.first == first && a.count == count,
                (Some(sh), None) => sh.frame_px == cell && count == 1 && spec.frame == first,
                _ => false,
            };
            if !plays {
                return Err(bad(at("sprite"), format!("the sprite object does not play this strip (sheet frames {first}..{} of {}x{})", first + count, cell[0], cell[1])));
            }
            strips.push(strip);
        }
        let mut cells = Vec::new();
        // Version 2 lists no cells: each is where the grid puts it, at its strip's frame time.
        if version >= 2 && j.get("cells").is_none() {
            for (i, st) in strips.iter().enumerate() {
                for (k, t) in super::plan::frame_times(st.duration_s, st.looping, st.count).into_iter().enumerate() {
                    let index = st.first + k as u32;
                    cells.push(CellData { rect: [(index % columns) * cell[0], (index / columns) * cell[1], cell[0], cell[1]], strip: i as u32, frame: k as u32, time_s: t });
                }
            }
        }
        let (empty, null) = (Json::Arr(Vec::new()), Json::Null);
        let given = if version >= 2 { j.get("cells").unwrap_or(&empty) } else { j.get("cells").unwrap_or(&null) };
        let list = Some(given).and_then(Json::as_array).filter(|a| a.len() <= MAX_CELLS).ok_or_else(|| bad("cells", format!("'cells' must be a list of at most {MAX_CELLS}")))?;
        for (i, v) in list.iter().enumerate() {
            let at = |k: &str| format!("cells[{i}].{k}");
            let r = v.get("rect").and_then(Json::as_array).filter(|a| a.len() == 4).ok_or_else(|| bad(at("rect"), "'rect' must be [x, y, w, h]"))?;
            let rect = [u(r.first(), &at("rect"), 0, 8192)?, u(r.get(1), &at("rect"), 0, 8192)?, u(r.get(2), &at("rect"), 1, 8192)?, u(r.get(3), &at("rect"), 1, 8192)?];
            let strip = u(v.get("strip"), &at("strip"), 0, strips.len() as u64 - 1)?;
            let st = &strips[strip as usize];
            let frame = u(v.get("frame"), &at("frame"), 0, u64::from(st.count - 1))?;
            let index = st.first + frame;
            let want = [(index % columns) * cell[0], (index / columns) * cell[1], cell[0], cell[1]];
            if rect != want {
                return Err(bad(at("rect"), format!("frame {frame} of strip {strip} is sheet frame {index}, at {want:?} in the grid the 2D path reads, not {rect:?}")));
            }
            if v.get("clip").and_then(Json::as_str) != st.clip.as_deref() || v.get("direction").and_then(Json::as_str) != Some(st.direction.as_str()) {
                return Err(bad(at("clip"), "a cell's clip and direction must be its strip's"));
            }
            cells.push(CellData { rect, strip, frame, time_s: f(v.get("time_s"), &at("time_s"), 0.0, 1e5)? });
        }
        let shadow = match j.get("shadow") {
            None | Some(Json::Null) => None,
            Some(v) => {
                let frame = u(v.get("frame"), "shadow.frame", 0, u64::from(frames_in_atlas.saturating_sub(1)))?;
                let r = v.get("rect").and_then(Json::as_array).filter(|a| a.len() == 4).ok_or_else(|| bad("shadow.rect", "'rect' must be [x, y, w, h]"))?;
                let rect = [u(r.first(), "shadow.rect", 0, 8192)?, u(r.get(1), "shadow.rect", 0, 8192)?, u(r.get(2), "shadow.rect", 1, 8192)?, u(r.get(3), "shadow.rect", 1, 8192)?];
                let want = [(frame % columns) * cell[0], (frame / columns) * cell[1], cell[0], cell[1]];
                if rect != want {
                    return Err(bad("shadow.rect", format!("the blob is sheet frame {frame}, at {want:?} in the grid the 2D path reads, not {rect:?}")));
                }
                let p = v.get("pivot").and_then(Json::as_array).filter(|a| a.len() == 2).ok_or_else(|| bad("shadow.pivot", "'pivot' must be two integers"))?;
                let pivot = [u(p.first(), "shadow.pivot", 0, u64::from(cell[0]))?, u(p.get(1), "shadow.pivot", 0, u64::from(cell[1]))?];
                let rr = v.get("radii_px").and_then(Json::as_array).filter(|a| a.len() == 2).ok_or_else(|| bad("shadow.radii_px", "'radii_px' must be two numbers"))?;
                let radii_px = [f(rr.first(), "shadow.radii_px", 0.0, 4096.0)?, f(rr.get(1), "shadow.radii_px", 0.0, 4096.0)?];
                let spec = SpriteSpec::parse(v.get("sprite").ok_or_else(|| bad("shadow.sprite", "'sprite' is missing"))?).map_err(|m| bad("shadow.sprite", m))?;
                if !(spec.sheet.is_some_and(|sh| sh.frame_px == cell) && spec.anim.is_none() && spec.frame == frame) {
                    return Err(bad("shadow.sprite", format!("the sprite object does not draw the blob (sheet frame {frame})")));
                }
                Some(ShadowData { frame, rect, pivot, radii_px })
            }
        };
        let flag = |k: &str| j.get(k).map_or(Ok(false), |v| v.as_bool().ok_or_else(|| bad(k, format!("'{k}' must be true or false"))));
        Ok(SheetData {
            outline: flag("outline")?,
            in_place: flag("in_place")?,
            shadow,
            texture,
            size,
            cell,
            columns,
            pivot,
            metres_per_pixel,
            view: name(j.get("view"), "view")?,
            elevation_deg: f(j.get("elevation_deg"), "elevation_deg", 0.0, 90.0)?,
            supersample: u(j.get("supersample"), "supersample", 1, 4)?,
            pixel_art: j.get("pixel_art").and_then(Json::as_bool).ok_or_else(|| bad("pixel_art", "'pixel_art' must be true or false"))?,
            light: name(j.get("light"), "light")?,
            source: j.get("source").cloned().unwrap_or(Json::Null),
            strips,
            cells,
        })
    }
}

/// The project's check of a `sprite_sheet` file: `Err((at, message))` when it isn't one.
pub fn validate(text: &str) -> Result<(), SheetError> {
    SheetData::parse(text).map(|_| ())
}

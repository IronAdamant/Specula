//! Sprite atlases and flipbooks, on the CPU: pack many RGBA images into one texture (so sprites
//! that use them batch and bind nothing), with transparent padding and edge extrusion so
//! bilinear filtering never bleeds a neighbour in; then play animation frames as data.
//!
//! Packing is a bottom-left skyline (the glyph atlas's idea, `specula-text`), deterministic:
//! images go in a fixed order (tallest first, then widest, then input order) into the smallest
//! size that holds them.

use std::fmt;

/// One image to pack: tightly packed RGBA8, top row first.
#[derive(Clone, Copy, Debug)]
pub struct AtlasImage<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasOptions {
    /// Transparent pixels between neighbours (after extrusion).
    pub padding: u32,
    /// Edge pixels repeated outward around each image, so bilinear samples at its border read
    /// its own colours.
    pub extrude: u32,
    /// Largest atlas side (the device's limit is at least 4096).
    pub max_size: u32,
    /// Sides are powers of two.
    pub power_of_two: bool,
}

impl Default for AtlasOptions {
    fn default() -> Self {
        AtlasOptions { padding: 2, extrude: 1, max_size: 4096, power_of_two: true }
    }
}

/// Where one image landed: its pixels (without extrusion) and texture coordinates
/// (u0, v0, u1, v1: top-left and bottom-right, as [`crate::Sprite::uv`] takes them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub uv: [f32; 4],
}

/// A packed atlas: upload `rgba` as `Rgba8Srgb`; `rects[i]` belongs to input image `i`.
#[derive(Clone, Debug, PartialEq)]
pub struct Atlas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub rects: Vec<AtlasRect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtlasError {
    /// Image `index` has a zero size or `len` bytes where `width × height × 4` were expected.
    BadImage { index: usize, width: u32, height: u32, len: usize },
    /// Everything does not fit in `max_size` × `max_size`.
    TooLarge { max_size: u32, area: u64 },
}

impl fmt::Display for AtlasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AtlasError::BadImage { index, width, height, len } => {
                write!(f, "atlas image {index}: {width}x{height} with {len} bytes (needs a non-zero size and width*height*4 bytes)")
            }
            AtlasError::TooLarge { max_size, area } => {
                write!(f, "atlas images need {area} pixels with padding; they don't fit in {max_size}x{max_size}")
            }
        }
    }
}

impl std::error::Error for AtlasError {}

#[derive(Clone, Copy, Debug)]
struct Node {
    x: u32,
    y: u32,
    w: u32,
}

/// Bottom-left skyline over a `width` × `height` page.
struct Skyline {
    width: u32,
    height: u32,
    nodes: Vec<Node>,
}

impl Skyline {
    fn new(width: u32, height: u32) -> Skyline {
        Skyline { width, height, nodes: vec![Node { x: 0, y: 0, w: width }] }
    }

    fn fit(&self, i: usize, w: u32, h: u32) -> Option<u32> {
        let x = self.nodes[i].x;
        if x + w > self.width {
            return None;
        }
        let (mut left, mut y) = (w, 0);
        for n in &self.nodes[i..] {
            y = y.max(n.y);
            if y + h > self.height {
                return None;
            }
            if n.w >= left {
                return Some(y);
            }
            left -= n.w;
        }
        None
    }

    fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let mut best: Option<(usize, u32, u32)> = None;
        for i in 0..self.nodes.len() {
            if let Some(y) = self.fit(i, w, h) {
                let better = match best {
                    None => true,
                    Some((_, by, bw)) => y < by || (y == by && self.nodes[i].w < bw),
                };
                if better {
                    best = Some((i, y, self.nodes[i].w));
                }
            }
        }
        let (i, y, _) = best?;
        let x = self.nodes[i].x;
        self.nodes.insert(i, Node { x, y: y + h, w });
        let mut j = i + 1;
        while j < self.nodes.len() {
            let end = self.nodes[j - 1].x + self.nodes[j - 1].w;
            if self.nodes[j].x >= end {
                break;
            }
            let shrink = end - self.nodes[j].x;
            if self.nodes[j].w <= shrink {
                self.nodes.remove(j);
            } else {
                self.nodes[j].x += shrink;
                self.nodes[j].w -= shrink;
                break;
            }
        }
        j = 0;
        while j + 1 < self.nodes.len() {
            if self.nodes[j].y == self.nodes[j + 1].y {
                self.nodes[j].w += self.nodes[j + 1].w;
                self.nodes.remove(j + 1);
            } else {
                j += 1;
            }
        }
        Some((x, y))
    }
}

/// Packs `images` into one atlas.
pub fn pack_atlas(images: &[AtlasImage], options: &AtlasOptions) -> Result<Atlas, AtlasError> {
    for (index, im) in images.iter().enumerate() {
        if im.width == 0 || im.height == 0 || im.rgba.len() as u64 != im.width as u64 * im.height as u64 * 4 {
            return Err(AtlasError::BadImage { index, width: im.width, height: im.height, len: im.rgba.len() });
        }
    }
    let (e, p) = (options.extrude, options.padding);
    let cell = |im: &AtlasImage| (im.width + 2 * e + p, im.height + 2 * e + p);
    let mut order: Vec<usize> = (0..images.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(images[i].height), std::cmp::Reverse(images[i].width), i));
    let area: u64 = images.iter().map(|im| cell(im).0 as u64 * cell(im).1 as u64).sum();
    let widest = images.iter().map(|im| cell(im).0).max().unwrap_or(1);
    let tallest = images.iter().map(|im| cell(im).1).max().unwrap_or(1);
    let round = |v: u32| if options.power_of_two { v.max(1).next_power_of_two() } else { v.max(1) };
    let side = round((area as f64).sqrt().ceil() as u32);
    let (mut w, mut h) = (round(side.max(widest)), round(side.max(tallest)));
    let max = options.max_size.max(1);
    let placed = loop {
        if w > max || h > max {
            return Err(AtlasError::TooLarge { max_size: max, area });
        }
        let mut sky = Skyline::new(w, h);
        let mut at = vec![(0u32, 0u32); images.len()];
        let fits = order.iter().all(|&i| {
            let (cw, ch) = cell(&images[i]);
            sky.alloc(cw, ch).map(|pos| at[i] = pos).is_some()
        });
        if fits {
            break at;
        }
        // Grow the shorter side (width first on ties), keeping powers of two.
        let grow = |v: u32| if options.power_of_two { v * 2 } else { v + v.div_ceil(4).max(1) };
        if w <= h {
            w = grow(w);
        } else {
            h = grow(h);
        }
    };
    let mut rgba = vec![0u8; w as usize * h as usize * 4];
    let mut rects = Vec::with_capacity(images.len());
    for (im, &(cx, cy)) in images.iter().zip(&placed) {
        let (x, y) = (cx + e, cy + e);
        // Every pixel of the extruded block reads its nearest image pixel.
        for by in 0..im.height + 2 * e {
            let sy = by.saturating_sub(e).min(im.height - 1);
            for bx in 0..im.width + 2 * e {
                let sx = bx.saturating_sub(e).min(im.width - 1);
                let src = ((sy * im.width + sx) * 4) as usize;
                let dst = (((cy + by) * w + cx + bx) * 4) as usize;
                rgba[dst..dst + 4].copy_from_slice(&im.rgba[src..src + 4]);
            }
        }
        let (fw, fh) = (w as f32, h as f32);
        rects.push(AtlasRect {
            x,
            y,
            width: im.width,
            height: im.height,
            uv: [x as f32 / fw, y as f32 / fh, (x + im.width) as f32 / fw, (y + im.height) as f32 / fh],
        });
    }
    Ok(Atlas { width: w, height: h, rgba, rects })
}

/// How a flipbook plays past its last frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Playback {
    #[default]
    Loop,
    /// Holds the last frame.
    Once,
    /// Forth and back (the end frames shown once per pass).
    PingPong,
}

/// An animation as data: frames (texture rects) shown at `fps`.
#[derive(Clone, Debug, PartialEq)]
pub struct Flipbook {
    /// u0, v0, u1, v1 per frame.
    pub frames: Vec<[f32; 4]>,
    pub fps: f32,
    pub playback: Playback,
}

impl Flipbook {
    /// The frame shown at `time_s` (0 before the start, and for a flipbook with no frames or no
    /// frame rate). Exact and platform-independent: one f64 multiply and a floor.
    pub fn frame_index(&self, time_s: f64) -> usize {
        Flipbook::index(self.frames.len(), self.fps, self.playback, time_s)
    }

    /// [`Flipbook::frame_index`] of a flipbook of `count` frames, without one (no allocation).
    pub fn index(count: usize, fps: f32, playback: Playback, time_s: f64) -> usize {
        let n = count;
        let fps = fps as f64;
        if n <= 1 || fps.is_nan() || fps <= 0.0 || time_s.is_nan() || time_s <= 0.0 || !fps.is_finite() {
            return 0;
        }
        let step = (time_s * fps).floor().min(u64::MAX as f64 / 2.0) as u64;
        match playback {
            Playback::Loop => (step % n as u64) as usize,
            Playback::Once => (step.min(n as u64 - 1)) as usize,
            Playback::PingPong => {
                let period = 2 * n as u64 - 2;
                let k = step % period;
                (if k < n as u64 { k } else { period - k }) as usize
            }
        }
    }

    /// How many whole frames of `frame` pixels a `sheet` holds (see [`Flipbook::grid`]).
    pub fn grid_len(sheet: [u32; 2], frame: [u32; 2], margin: u32, spacing: u32) -> u32 {
        if frame[0] == 0 || frame[1] == 0 {
            return 0;
        }
        // As tilesets count (and the tile shader): `margin` on every side, `spacing` between.
        let along = |s: u32, f: u32| (s.saturating_sub(margin.saturating_mul(2)).saturating_add(spacing)) / f.saturating_add(spacing);
        along(sheet[0], frame[0]).saturating_mul(along(sheet[1], frame[1]))
    }

    /// Frame `index` of [`Flipbook::grid`] computed directly (no allocation); `None` past the
    /// sheet.
    pub fn grid_rect(sheet: [u32; 2], frame: [u32; 2], index: u32, margin: u32, spacing: u32) -> Option<[f32; 4]> {
        if index >= Flipbook::grid_len(sheet, frame, margin, spacing) {
            return None;
        }
        let cols = (sheet[0].saturating_sub(margin.saturating_mul(2)).saturating_add(spacing)) / frame[0].saturating_add(spacing);
        let (col, row) = (index % cols, index / cols);
        let (x, y) = (margin + col * (frame[0] + spacing), margin + row * (frame[1] + spacing));
        let (sw, sh) = (sheet[0] as f32, sheet[1] as f32);
        Some([x as f32 / sw, y as f32 / sh, (x + frame[0]) as f32 / sw, (y + frame[1]) as f32 / sh])
    }

    /// The texture rect shown at `time_s` (the whole texture without frames).
    pub fn uv_at(&self, time_s: f64) -> [f32; 4] {
        self.frames.get(self.frame_index(time_s)).copied().unwrap_or([0.0, 0.0, 1.0, 1.0])
    }

    /// Frames cut from a sheet of `sheet` pixels (width, height): `count` frames of `frame`
    /// pixels, left to right then top to bottom, with `margin` pixels on every side of the sheet
    /// and `spacing` between frames (as tilesets are cut). Stops early at the sheet's edge.
    pub fn grid(sheet: [u32; 2], frame: [u32; 2], count: u32, margin: u32, spacing: u32) -> Vec<[f32; 4]> {
        let n = count.min(Flipbook::grid_len(sheet, frame, margin, spacing));
        (0..n).filter_map(|k| Flipbook::grid_rect(sheet, frame, k, margin, spacing)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Vec<u8> {
        c.iter().copied().cycle().take((w * h * 4) as usize).collect()
    }

    #[test]
    fn packs_without_overlap_and_extrudes_edges() {
        let colours = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 128], [9, 9, 9, 255]];
        let sizes = [(16, 16), (30, 8), (7, 21), (1, 1)];
        let pixels: Vec<Vec<u8>> = sizes.iter().zip(colours).map(|(&(w, h), c)| solid(w, h, c)).collect();
        let images: Vec<AtlasImage> =
            sizes.iter().zip(&pixels).map(|(&(width, height), rgba)| AtlasImage { width, height, rgba }).collect();
        let opts = AtlasOptions::default();
        let a = pack_atlas(&images, &opts).unwrap();
        assert!(a.width.is_power_of_two() && a.height.is_power_of_two());
        assert_eq!(a.rgba.len(), (a.width * a.height * 4) as usize);
        // No two padded cells overlap.
        let cell = |r: &AtlasRect| {
            let m = opts.extrude;
            [r.x - m, r.y - m, r.x + r.width + m + opts.padding, r.y + r.height + m + opts.padding]
        };
        for (i, a_) in a.rects.iter().enumerate() {
            for b_ in &a.rects[i + 1..] {
                let (p, q) = (cell(a_), cell(b_));
                assert!(p[2] <= q[0] || q[2] <= p[0] || p[3] <= q[1] || q[3] <= p[1], "{a_:?} overlaps {b_:?}");
            }
        }
        let px = |x: u32, y: u32| {
            let i = ((y * a.width + x) * 4) as usize;
            [a.rgba[i], a.rgba[i + 1], a.rgba[i + 2], a.rgba[i + 3]]
        };
        for (r, c) in a.rects.iter().zip(colours) {
            assert_eq!(px(r.x, r.y), c);
            assert_eq!(px(r.x - 1, r.y - 1), c, "extruded corner");
            assert_eq!(px(r.x + r.width, r.y + r.height - 1), c, "extruded right edge");
            let uv = r.uv;
            assert_eq!((uv[0] * a.width as f32) as u32, r.x);
            assert_eq!(((uv[3] * a.height as f32).round()) as u32, r.y + r.height);
        }
        // Deterministic.
        assert_eq!(pack_atlas(&images, &opts).unwrap(), a);
    }

    #[test]
    fn errors_are_structured() {
        let px = solid(4, 4, [1; 4]);
        let bad = [AtlasImage { width: 4, height: 3, rgba: &px }];
        assert_eq!(pack_atlas(&bad, &AtlasOptions::default()), Err(AtlasError::BadImage { index: 0, width: 4, height: 3, len: 64 }));
        let big = [AtlasImage { width: 4, height: 4, rgba: &px }; 3];
        let err = pack_atlas(&big, &AtlasOptions { max_size: 8, ..Default::default() }).unwrap_err();
        assert!(matches!(err, AtlasError::TooLarge { max_size: 8, .. }), "{err}");
    }

    #[test]
    fn flipbooks_loop_hold_and_pingpong() {
        let frames = Flipbook::grid([64, 32], [16, 16], 6, 0, 0);
        assert_eq!(frames.len(), 6);
        assert_eq!(frames[5], [0.25, 0.5, 0.5, 1.0]);
        let mut fb = Flipbook { frames, fps: 10.0, playback: Playback::Loop };
        let seq = |fb: &Flipbook| (0..14).map(|k| fb.frame_index(k as f64 * 0.1 + 0.01)).collect::<Vec<_>>();
        assert_eq!(seq(&fb), [0, 1, 2, 3, 4, 5, 0, 1, 2, 3, 4, 5, 0, 1]);
        fb.playback = Playback::Once;
        assert_eq!(seq(&fb), [0, 1, 2, 3, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5]);
        fb.playback = Playback::PingPong;
        assert_eq!(seq(&fb), [0, 1, 2, 3, 4, 5, 4, 3, 2, 1, 0, 1, 2, 3]);
        assert_eq!(fb.frame_index(-3.0), 0);
        assert_eq!(Flipbook { frames: vec![], fps: 5.0, playback: Playback::Loop }.uv_at(1.0), [0.0, 0.0, 1.0, 1.0]);
        // Margin and spacing; stops at the sheet's edge.
        let g = Flipbook::grid([40, 22], [8, 8], 10, 2, 2);
        assert_eq!(g.len(), 6, "3 per row ((40 - 4 + 2) / 10), 2 rows ((22 - 4 + 2) / 10)");
        assert_eq!(g[1][0], 12.0 / 40.0);
        assert_eq!(Flipbook::grid_len([40, 22], [8, 8], 2, 2), 6);
        assert_eq!(Flipbook::grid_rect([40, 22], [8, 8], 5, 2, 2), Some(g[5]));
        assert_eq!(Flipbook::grid_rect([40, 22], [8, 8], 6, 2, 2), None);
        assert_eq!(Flipbook::grid_len([4, 4], [8, 8], 0, 0), 0);
        assert_eq!(Flipbook::grid_len([u32::MAX, u32::MAX], [1, 1], 0, 0), u32::MAX, "saturates");
        assert_eq!(Flipbook::index(6, 10.0, Playback::PingPong, 0.61), 4);
    }
}

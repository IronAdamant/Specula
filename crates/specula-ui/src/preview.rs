//! A software renderer for [`DrawList`]s: the reference for what the GPU overlay must draw, and
//! eyes for agents and CI with no GPU. Single-threaded and deterministic (IEEE basic operations
//! plus `specula_core::dmath`), so the same draw list gives the same pixels, and the same hash,
//! on every platform.
//!
//! Like the GPU overlay, inputs are sRGB with straight alpha, converted to **linear** light;
//! blending ("over") and interpolation (gradients, vertex colours, glyph coverage) happen in
//! linear, and the result is encoded back to sRGB.

use crate::color::{to_linear, Rgba};
use std::sync::OnceLock;
use crate::draw::{DrawCmd, DrawList, Rect, RectCmd, Vertex};
use specula_core::deflate::Level;
use specula_core::hash::StableHasher;
use specula_core::png::{self, Image};
use specula_text::GlyphAtlas;

/// A texture for [`DrawCmd::Image`], by the id the draw list uses.
pub struct Texture<'a> {
    pub id: u32,
    pub image: &'a Image,
}

/// Premultiplied RGBA float canvas.
struct Canvas {
    w: usize,
    px: Vec<[f32; 4]>,
    clip: [i32; 4],
}

impl Canvas {
    #[inline]
    fn blend(&mut self, x: usize, y: usize, c: [f32; 4]) {
        // `c` premultiplied.
        let d = &mut self.px[y * self.w + x];
        let k = 1.0 - c[3];
        d[0] = c[0] + d[0] * k;
        d[1] = c[1] + d[1] * k;
        d[2] = c[2] + d[2] * k;
        d[3] = c[3] + d[3] * k;
    }

    /// Pixel bounds `[x0, y0, x1, y1)` of `r` within the clip.
    fn bounds(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Option<[usize; 4]> {
        let bx0 = (x0.floor() as i32).max(self.clip[0]);
        let by0 = (y0.floor() as i32).max(self.clip[1]);
        let bx1 = (x1.ceil() as i32).min(self.clip[2]);
        let by1 = (y1.ceil() as i32).min(self.clip[3]);
        (bx1 > bx0 && by1 > by0).then_some([bx0 as usize, by0 as usize, bx1 as usize, by1 as usize])
    }
}

/// Premultiply a **linear** colour by its alpha × coverage.
fn premul(c: Rgba, cov: f32) -> [f32; 4] {
    let a = (c[3] * cov).clamp(0.0, 1.0);
    [c[0] * a, c[1] * a, c[2] * a, a]
}

/// sRGB colour → linear (alpha unchanged).
fn lin(c: Rgba) -> Rgba {
    [to_linear(c[0]), to_linear(c[1]), to_linear(c[2]), c[3]]
}

/// sRGB byte → linear, as a table.
fn lin8(v: u8) -> f32 {
    static T: OnceLock<[f32; 256]> = OnceLock::new();
    T.get_or_init(|| std::array::from_fn(|i| to_linear(i as f32 / 255.0)))[v as usize]
}

/// Linear → sRGB byte, rounded exactly: the largest code whose lower midpoint is ≤ `l`.
fn srgb8(l: f32) -> u8 {
    static MID: OnceLock<[f32; 255]> = OnceLock::new();
    let mid = MID.get_or_init(|| std::array::from_fn(|k| to_linear((k as f32 + 0.5) / 255.0)));
    mid.partition_point(|&m| m <= l) as u8
}

fn lerp4(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

/// Rasterise `list` over `background` (sRGB straight alpha). Glyph quads sample `atlas`.
pub fn render(list: &DrawList, atlas: &GlyphAtlas, textures: &[Texture<'_>], background: Rgba) -> Image {
    let (w, h) = (list.size[0] as usize, list.size[1] as usize);
    let bg = premul(lin(background), 1.0);
    let c = Canvas { w, px: vec![bg; w * h], clip: [0, 0, w as i32, h as i32] };
    finish(raster(c, list, atlas, textures))
}

/// Rasterise `list` on top of an existing image (e.g. a game frame or a scene backdrop). The
/// image must have the draw list's size.
pub fn render_onto(base: &Image, list: &DrawList, atlas: &GlyphAtlas, textures: &[Texture<'_>]) -> Image {
    let (w, h) = (list.size[0] as usize, list.size[1] as usize);
    assert_eq!((base.width as usize, base.height as usize), (w, h), "base image size must match the draw list");
    let px = base
        .rgba
        .chunks_exact(4)
        .map(|p| premul([lin8(p[0]), lin8(p[1]), lin8(p[2]), f32::from(p[3]) / 255.0], 1.0))
        .collect();
    let c = Canvas { w, px, clip: [0, 0, w as i32, h as i32] };
    finish(raster(c, list, atlas, textures))
}

fn raster(mut c: Canvas, list: &DrawList, atlas: &GlyphAtlas, textures: &[Texture<'_>]) -> (Canvas, usize) {
    let (w, h) = (list.size[0] as usize, list.size[1] as usize);
    for cmd in &list.cmds {
        match *cmd {
            DrawCmd::Clip(r) => {
                c.clip = match r {
                    Some(r) => [
                        (r.x.round() as i32).clamp(0, w as i32),
                        (r.y.round() as i32).clamp(0, h as i32),
                        (r.right().round() as i32).clamp(0, w as i32),
                        (r.bottom().round() as i32).clamp(0, h as i32),
                    ],
                    None => [0, 0, w as i32, h as i32],
                }
            }
            DrawCmd::Rect(r) => rect(&mut c, &r),
            DrawCmd::Image { rect, uv, texture, color, .. } => {
                if let Some(t) = textures.iter().find(|t| t.id == texture) {
                    image(&mut c, rect, uv, t.image, color);
                }
            }
            DrawCmd::Glyphs { start, end } => {
                for q in &list.glyphs[start as usize..end as usize] {
                    let Some(page) = atlas.page_pixels(q.page as usize) else { continue };
                    let ps = atlas.page_size() as usize;
                    let Some(b) = c.bounds(q.rect[0], q.rect[1], q.rect[2], q.rect[3]) else { continue };
                    let tx0 = (q.uv[0] * ps as f32).round() as i64 - q.rect[0].floor() as i64;
                    let ty0 = (q.uv[1] * ps as f32).round() as i64 - q.rect[1].floor() as i64;
                    let color = lin(q.color);
                    for y in b[1]..b[3] {
                        for x in b[0]..b[2] {
                            let (tx, ty) = (x as i64 + tx0, y as i64 + ty0);
                            if tx < 0 || ty < 0 || tx >= ps as i64 || ty >= ps as i64 {
                                continue;
                            }
                            let cov = f32::from(page[ty as usize * ps + tx as usize]) / 255.0;
                            if cov > 0.0 {
                                c.blend(x, y, premul(color, cov));
                            }
                        }
                    }
                }
            }
            DrawCmd::Mesh { start, end } => {
                for t in list.vertices[start as usize..end as usize].chunks_exact(3) {
                    triangle(&mut c, t[0], t[1], t[2]);
                }
            }
        }
    }
    (c, h)
}

fn finish((c, h): (Canvas, usize)) -> Image {
    let w = c.w;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for p in &c.px {
        let a = p[3];
        let un = |v: f32| if a > 0.0 { srgb8((v / a).clamp(0.0, 1.0)) } else { 0 };
        rgba.extend_from_slice(&[un(p[0]), un(p[1]), un(p[2]), (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8]);
    }
    Image { width: w as u32, height: h as u32, rgba }
}

/// Signed distance from `p` to a rounded box centred at the origin with half-size `b`.
fn sd_round_box(p: [f32; 2], b: [f32; 2], r: f32) -> f32 {
    let q = [p[0].abs() - b[0] + r, p[1].abs() - b[1] + r];
    let outside = (q[0].max(0.0) * q[0].max(0.0) + q[1].max(0.0) * q[1].max(0.0)).sqrt();
    outside + q[0].max(q[1]).min(0.0) - r
}

fn rect(c: &mut Canvas, r: &RectCmd) {
    // Clamped like DrawList::rect and the GPU overlay, for hand-built lists too.
    let f = if r.feather.is_finite() { r.feather.clamp(1.0, crate::draw::MAX_FEATHER) } else { 1.0 };
    let rr = r.rect;
    let Some(b) = c.bounds(rr.x - f, rr.y - f, rr.right() + f, rr.bottom() + f) else { return };
    let half = [rr.w * 0.5, rr.h * 0.5];
    let centre = rr.center();
    let radius = r.radius.clamp(0.0, half[0].min(half[1]));
    let has_border = r.border > 0.0;
    let (f0, f1) = (lin(r.fill[0]), lin(r.fill[1]));
    let (b0, b1) = (lin(r.border_color[0]), lin(r.border_color[1]));
    for y in b[1]..b[3] {
        let py = y as f32 + 0.5;
        let t = ((py - rr.y) / rr.h.max(1e-6)).clamp(0.0, 1.0);
        let fill = lerp4(f0, f1, t);
        let border = lerp4(b0, b1, t);
        for x in b[0]..b[2] {
            let px = x as f32 + 0.5;
            let d = sd_round_box([px - centre[0], py - centre[1]], half, radius);
            let outer = (0.5 - d / f).clamp(0.0, 1.0);
            if outer <= 0.0 {
                continue;
            }
            let col = if has_border {
                let inner = (0.5 - (d + r.border)).clamp(0.0, 1.0).min(outer);
                let a = premul(fill, inner);
                let bb = premul(border, outer - inner);
                [a[0] + bb[0], a[1] + bb[1], a[2] + bb[2], a[3] + bb[3]]
            } else {
                premul(fill, outer)
            };
            if col[3] > 0.0 {
                c.blend(x, y, col);
            }
        }
    }
}

fn image(c: &mut Canvas, r: Rect, uv: [f32; 4], img: &Image, tint: Rgba) {
    let tint = lin(tint);
    let Some(b) = c.bounds(r.x, r.y, r.right(), r.bottom()) else { return };
    if img.width == 0 || img.height == 0 || r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    for y in b[1]..b[3] {
        let v = uv[1] + (uv[3] - uv[1]) * ((y as f32 + 0.5 - r.y) / r.h);
        let ty = ((v * img.height as f32) as i64).clamp(0, img.height as i64 - 1) as usize;
        for x in b[0]..b[2] {
            let u = uv[0] + (uv[2] - uv[0]) * ((x as f32 + 0.5 - r.x) / r.w);
            let tx = ((u * img.width as f32) as i64).clamp(0, img.width as i64 - 1) as usize;
            let p = &img.rgba[(ty * img.width as usize + tx) * 4..][..4];
            // An sRGB texture is sampled as linear (as from a *_SRGB image view), times the tint.
            let s = [lin8(p[0]) * tint[0], lin8(p[1]) * tint[1], lin8(p[2]) * tint[2], f32::from(p[3]) / 255.0 * tint[3]];
            c.blend(x, y, premul(s, 1.0));
        }
    }
}

fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

/// Top-left fill rule for a clockwise-in-screen (positive area) triangle edge a → b.
fn top_left(a: [f32; 2], b: [f32; 2]) -> bool {
    let d = [b[0] - a[0], b[1] - a[1]];
    (d[1] == 0.0 && d[0] < 0.0) || d[1] > 0.0
}

fn triangle(c: &mut Canvas, a: Vertex, b: Vertex, cc: Vertex) {
    // Vertex colours are sRGB: convert each to linear, then interpolate (as the GPU does).
    let to_lin = |v: Vertex| Vertex { pos: v.pos, color: lin(v.color) };
    let (mut v0, mut v1, v2) = (to_lin(a), to_lin(b), to_lin(cc));
    let area = edge(v0.pos, v1.pos, v2.pos);
    if area == 0.0 || !area.is_finite() {
        return;
    }
    if area < 0.0 {
        std::mem::swap(&mut v0, &mut v1);
    }
    let area = area.abs();
    let xs = [v0.pos[0], v1.pos[0], v2.pos[0]];
    let ys = [v0.pos[1], v1.pos[1], v2.pos[1]];
    let Some(bb) = c.bounds(
        xs.iter().copied().fold(f32::MAX, f32::min),
        ys.iter().copied().fold(f32::MAX, f32::min),
        xs.iter().copied().fold(f32::MIN, f32::max),
        ys.iter().copied().fold(f32::MIN, f32::max),
    ) else {
        return;
    };
    let tl = [top_left(v1.pos, v2.pos), top_left(v2.pos, v0.pos), top_left(v0.pos, v1.pos)];
    for y in bb[1]..bb[3] {
        for x in bb[0]..bb[2] {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let w = [edge(v1.pos, v2.pos, p), edge(v2.pos, v0.pos, p), edge(v0.pos, v1.pos, p)];
            let inside = (0..3).all(|k| w[k] > 0.0 || (w[k] == 0.0 && tl[k]));
            if !inside {
                continue;
            }
            let (l0, l1, l2) = (w[0] / area, w[1] / area, w[2] / area);
            let col = [
                v0.color[0] * l0 + v1.color[0] * l1 + v2.color[0] * l2,
                v0.color[1] * l0 + v1.color[1] * l1 + v2.color[1] * l2,
                v0.color[2] * l0 + v1.color[2] * l1 + v2.color[2] * l2,
                v0.color[3] * l0 + v1.color[3] * l1 + v2.color[3] * l2,
            ];
            if col[3] > 0.0 {
                c.blend(x, y, premul(col, 1.0));
            }
        }
    }
}

/// PNG bytes of an image.
pub fn to_png(img: &Image) -> Vec<u8> {
    png::encode_rgba8(img.width, img.height, &img.rgba, Level::Default)
}

/// A stable hash of the pixels (determinism checks).
pub fn hash(img: &Image) -> u64 {
    let mut h = StableHasher::new();
    h.write_u32(img.width);
    h.write_u32(img.height);
    h.write_bytes(&img.rgba);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_text::AtlasConfig;

    fn list(size: [u32; 2]) -> DrawList {
        DrawList { size, scale: 1.0, ..DrawList::default() }
    }

    fn px(img: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2], img.rgba[i + 3]]
    }

    #[test]
    fn rect_fill_border_and_rounding() {
        let mut d = list([40, 40]);
        d.rect(RectCmd {
            rect: Rect::new(5.0, 5.0, 30.0, 30.0),
            radius: 8.0,
            fill: [[1.0, 0.0, 0.0, 1.0]; 2],
            border: 2.0,
            border_color: [[0.0, 0.0, 1.0, 1.0]; 2],
            feather: 1.0,
        });
        let atlas = GlyphAtlas::new(AtlasConfig::default());
        let img = render(&d, &atlas, &[], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(px(&img, 20, 20), [255, 0, 0, 255]); // fill
        assert_eq!(px(&img, 20, 5), [0, 0, 255, 255]); // border
        assert_eq!(px(&img, 5, 5), [0, 0, 0, 255]); // rounded corner stays background
        assert_eq!(px(&img, 2, 20), [0, 0, 0, 255]); // outside
    }

    #[test]
    fn gradient_and_clip() {
        let mut d = list([10, 20]);
        d.clip(Some(Rect::new(0.0, 0.0, 10.0, 10.0)));
        d.rect(RectCmd {
            rect: Rect::new(0.0, 0.0, 10.0, 20.0),
            radius: 0.0,
            fill: [[1.0, 1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]],
            border: 0.0,
            border_color: [[0.0; 4]; 2],
            feather: 1.0,
        });
        let atlas = GlyphAtlas::new(AtlasConfig::default());
        let img = render(&d, &atlas, &[], [0.0, 0.0, 1.0, 1.0]);
        assert!(px(&img, 5, 0)[0] > 240);
        // Row 9 is 47.5 % down a white-to-black gradient: interpolated in linear light, 0.525
        // linear encodes as sRGB 193 (not the 134 of an sRGB-space lerp).
        assert!((190..=196).contains(&px(&img, 5, 9)[0]), "{:?}", px(&img, 5, 9));
        assert_eq!(px(&img, 5, 15), [0, 0, 255, 255]); // clipped away
    }

    #[test]
    fn mesh_triangles_cover_without_seams() {
        // Two triangles sharing a diagonal: every pixel of the square is covered exactly once.
        // Half-transparent white over black is 0.5 linear = sRGB 188.
        let mut d = list([8, 8]);
        let c = [1.0, 1.0, 1.0, 0.5];
        let v = |x: f32, y: f32| Vertex { pos: [x, y], color: c };
        let s = d.begin_mesh();
        d.vertices.extend([v(0.0, 0.0), v(8.0, 0.0), v(8.0, 8.0), v(0.0, 0.0), v(8.0, 8.0), v(0.0, 8.0)]);
        d.end_mesh(s);
        let atlas = GlyphAtlas::new(AtlasConfig::default());
        let img = render(&d, &atlas, &[], [0.0, 0.0, 0.0, 1.0]);
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(px(&img, x, y), [188, 188, 188, 255], "{x},{y}");
            }
        }
    }

    #[test]
    fn feathers_are_capped_like_the_gpu() {
        let shadow = |feather: f32| RectCmd {
            rect: Rect::new(150.0, 150.0, 100.0, 100.0),
            radius: 0.0,
            fill: [[1.0; 4]; 2],
            border: 0.0,
            border_color: [[0.0; 4]; 2],
            feather,
        };
        let atlas = GlyphAtlas::new(AtlasConfig::default());
        let draw = |feather: f32, via_rect: bool| {
            let mut d = list([400, 400]);
            if via_rect {
                d.rect(shadow(feather));
            } else {
                d.cmds.push(DrawCmd::Rect(shadow(feather)));
            }
            hash(&render(&d, &atlas, &[], [0.0, 0.0, 0.0, 1.0]))
        };
        let capped = draw(crate::draw::MAX_FEATHER, true);
        assert_eq!(draw(200.0, true), capped, "DrawList::rect clamps");
        assert_eq!(draw(200.0, false), capped, "the preview clamps hand-built lists too");
        assert_ne!(draw(20.0, true), capped);
    }

    #[test]
    fn rendering_is_deterministic_and_pinned() {
        let mut d = list([64, 48]);
        d.rect(RectCmd {
            rect: Rect::new(4.0, 6.0, 50.0, 30.0),
            radius: 6.0,
            fill: [[0.2, 0.22, 0.26, 1.0], [0.15, 0.17, 0.2, 1.0]],
            border: 1.5,
            border_color: [[0.35, 0.38, 0.44, 1.0], [0.08, 0.09, 0.11, 1.0]],
            feather: 1.0,
        });
        d.rect(RectCmd {
            rect: Rect::new(10.0, 12.0, 30.0, 20.0),
            radius: 4.0,
            fill: [[0.0, 0.0, 0.0, 0.5]; 2],
            border: 0.0,
            border_color: [[0.0; 4]; 2],
            feather: 6.0,
        });
        let atlas = GlyphAtlas::new(AtlasConfig::default());
        let a = render(&d, &atlas, &[], [0.04, 0.06, 0.12, 1.0]);
        let b = render(&d, &atlas, &[], [0.04, 0.06, 0.12, 1.0]);
        assert_eq!(hash(&a), hash(&b));
        // Pinned: IEEE basic operations plus specula_core::dmath give the same bits everywhere.
        assert_eq!(hash(&a), 1565204969093717806);
    }
}

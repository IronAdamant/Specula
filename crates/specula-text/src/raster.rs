//! Outline rasterisation.
//!
//! **Coverage**: exact signed-area accumulation (the approach of font-rs and stb_truetype v2).
//! Every line segment adds, to each pixel it crosses, the signed area it sweeps to the pixel's
//! right edge; a running sum along each row then gives the winding-weighted coverage. Coverage is
//! `min(|sum|, 1)`, which is the non-zero winding rule: overlapping contours of the same
//! direction saturate instead of cancelling, and opposite contours (holes) cancel. Quadratic
//! curves are flattened adaptively first (error well under 1/10 px).
//!
//! **SDF**: signed distance to the flattened outline, sign from the exact non-zero winding number
//! at each pixel centre. For scalable text (one atlas size drawn at many sizes).

use crate::error::{FontError, Reason, Tag};
use crate::font::{Font, GlyphId};
use crate::outline::{Outline, Segment};

/// Largest bitmap side the rasteriser will produce (a guard against corrupt or absurd outlines).
pub const MAX_BITMAP_DIM: u32 = 2048;

/// An 8-bit coverage (or distance) bitmap with its placement relative to the pen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    /// Pixel x of column 0 relative to the (integer) pen x.
    pub left: i32,
    /// Pixel y of row 0 relative to the baseline, y down (negative = above the baseline).
    pub top: i32,
    /// `width * height` bytes, row-major, top row first.
    pub pixels: Vec<u8>,
}

impl Bitmap {
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn get(&self, x: u32, y: u32) -> u8 {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize]
        } else {
            0
        }
    }
}

/// Pixel-space placement of an outline: `px = x * scale + dx`, `py = -y * scale + dy`.
#[derive(Clone, Copy, Debug)]
struct Xform {
    scale: f32,
    dx: f32,
    dy: f32,
}

impl Xform {
    fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        [p[0] * self.scale + self.dx, -p[1] * self.scale + self.dy]
    }
}

/// Flatten an outline to line segments in pixel space.
fn flatten(outline: &Outline, xf: Xform, mut line: impl FnMut([f32; 2], [f32; 2])) {
    outline.segments(|s| match s {
        Segment::Line(a, b) => line(xf.apply(a), xf.apply(b)),
        Segment::Quad(a, c, b) => {
            let (p0, p1, p2) = (xf.apply(a), xf.apply(c), xf.apply(b));
            let devx = p0[0] - 2.0 * p1[0] + p2[0];
            let devy = p0[1] - 2.0 * p1[1] + p2[1];
            let devsq = devx * devx + devy * devy;
            if devsq < 0.333 {
                line(p0, p2);
                return;
            }
            // Segment count from the curve's second difference: error ~ dev / (8 n^2).
            let n = (1.0 + (3.0 * devsq).sqrt().sqrt().floor()).min(256.0) as usize;
            let step = 1.0 / n as f32;
            let mut prev = p0;
            for i in 1..n {
                let t = i as f32 * step;
                let u = 1.0 - t;
                let p = [
                    u * u * p0[0] + 2.0 * u * t * p1[0] + t * t * p2[0],
                    u * u * p0[1] + 2.0 * u * t * p1[1] + t * t * p2[1],
                ];
                line(prev, p);
                prev = p;
            }
            line(prev, p2);
        }
    });
}

/// Integer pixel box of the outline at this scale and horizontal sub-pixel offset:
/// `(left, top, width, height)`.
fn pixel_box(outline: &Outline, scale: f32, offset_x: f32, pad: f32) -> Option<(i32, i32, u32, u32)> {
    let b = outline.bounds()?;
    let x0 = (b[0] * scale + offset_x - pad).floor();
    let x1 = (b[2] * scale + offset_x + pad).ceil();
    let y0 = (-b[3] * scale - pad).floor();
    let y1 = (-b[1] * scale + pad).ceil();
    let w = x1 - x0;
    let h = y1 - y0;
    if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some((x0 as i32, y0 as i32, w as u32, h as u32))
}

/// A reusable coverage rasteriser (keeps its accumulation buffer between glyphs).
#[derive(Default)]
pub struct Rasterizer {
    acc: Vec<f32>,
    w: usize,
    h: usize,
    stride: usize,
}

impl Rasterizer {
    pub fn new() -> Rasterizer {
        Rasterizer::default()
    }

    fn reset(&mut self, w: usize, h: usize) {
        self.w = w;
        self.h = h;
        self.stride = w + 2;
        self.acc.clear();
        self.acc.resize(self.stride * h, 0.0);
    }

    /// Accumulate the signed area of one line segment (pixel coordinates inside `[0,w]x[0,h]`).
    fn line(&mut self, p0: [f32; 2], p1: [f32; 2]) {
        if p0[1] == p1[1] || !(p0[0].is_finite() && p0[1].is_finite() && p1[0].is_finite() && p1[1].is_finite()) {
            return;
        }
        let (dir, a, b) = if p0[1] < p1[1] { (1.0f32, p0, p1) } else { (-1.0f32, p1, p0) };
        let hf = self.h as f32;
        let wf = self.w as f32;
        let ystart = a[1].max(0.0);
        let yend = b[1].min(hf);
        if ystart >= yend {
            return;
        }
        let dxdy = (b[0] - a[0]) / (b[1] - a[1]);
        let mut x = a[0] + (ystart - a[1]) * dxdy;
        let row0 = ystart as usize;
        let row_end = (yend.ceil() as usize).min(self.h);
        for row in row0..row_end {
            let ya = (row as f32).max(ystart);
            let yb = ((row + 1) as f32).min(yend);
            let dy = yb - ya;
            let xnext = x + dxdy * dy;
            let d = dy * dir;
            let (xa, xb) = if x < xnext { (x, xnext) } else { (xnext, x) };
            let xa = xa.clamp(0.0, wf);
            let xb = xb.clamp(0.0, wf);
            let base = row * self.stride;
            let xa_floor = xa.floor();
            let ia = xa_floor as usize;
            let xb_ceil = xb.ceil();
            let ib = xb_ceil as usize;
            let acc = &mut self.acc;
            if ib <= ia + 1 {
                // The segment stays inside one pixel column: split by its mean x.
                let xmf = 0.5 * (xa + xb) - xa_floor;
                acc[base + ia] += d - d * xmf;
                acc[base + ia + 1] += d * xmf;
            } else {
                let s = 1.0 / (xb - xa);
                let xaf = xa - xa_floor;
                let a0 = 0.5 * s * (1.0 - xaf) * (1.0 - xaf);
                let xbf = xb - xb_ceil + 1.0;
                let am = 0.5 * s * xbf * xbf;
                acc[base + ia] += d * a0;
                if ib == ia + 2 {
                    acc[base + ia + 1] += d * (1.0 - a0 - am);
                } else {
                    let a1 = s * (1.5 - xaf);
                    acc[base + ia + 1] += d * (a1 - a0);
                    for xi in ia + 2..ib - 1 {
                        acc[base + xi] += d * s;
                    }
                    let a2 = a1 + (ib - ia - 3) as f32 * s;
                    acc[base + ib - 1] += d * (1.0 - a2 - am);
                }
                acc[base + ib] += d * am;
            }
            x = xnext;
        }
    }

    /// Rasterise an outline (font units) at `scale` px per unit, shifted right by `offset_x`
    /// pixels (sub-pixel positioning). Returns an empty bitmap for empty outlines.
    pub fn rasterize(&mut self, outline: &Outline, scale: f32, offset_x: f32) -> Result<Bitmap, FontError> {
        let Some((left, top, w, h)) = pixel_box(outline, scale, offset_x, 0.0) else {
            return Ok(Bitmap::default());
        };
        check_dim(w, h)?;
        let (w, h) = (w as usize, h as usize);
        self.reset(w, h);
        let xf = Xform { scale, dx: offset_x - left as f32, dy: -(top as f32) };
        flatten(outline, xf, |a, b| self.line(a, b));
        let mut pixels = vec![0u8; w * h];
        for row in 0..h {
            let mut sum = 0.0f32;
            let acc = &self.acc[row * self.stride..row * self.stride + w];
            let out = &mut pixels[row * w..row * w + w];
            for (o, &a) in out.iter_mut().zip(acc) {
                sum += a;
                *o = (sum.abs().min(1.0) * 255.0 + 0.5) as u8;
            }
        }
        Ok(Bitmap { width: w as u32, height: h as u32, left, top, pixels })
    }
}

fn check_dim(w: u32, h: u32) -> Result<(), FontError> {
    if w > MAX_BITMAP_DIM || h > MAX_BITMAP_DIM {
        return Err(FontError::new(
            Tag(*b"glyf"),
            0,
            Reason::BadValue { field: "glyph bitmap size (px)", value: w.max(h) as i64 },
        ));
    }
    Ok(())
}

/// Largest SDF bitmap side. SDF generation is brute force, O(width × height × segments): a
/// 48 px glyph (about 60×60 px with spread) against ~100 segments is ~0.4 M distance tests, about
/// a millisecond; this cap keeps a hostile outline to well under a second. SDFs are meant to be
/// generated once at a modest size (32–64 px) and scaled by the shader.
pub const MAX_SDF_DIM: u32 = 256;

/// Signed distance field of an outline: 128 on the edge, 255 at `spread` px inside, 0 at `spread`
/// px outside. The bitmap is padded by `spread` px on every side. Sides are capped at
/// [`MAX_SDF_DIM`] (an error beyond that); `spread` is clamped to 0.5..=32 px.
pub fn rasterize_sdf(outline: &Outline, scale: f32, spread: f32) -> Result<Bitmap, FontError> {
    let spread = if spread.is_finite() { spread.clamp(0.5, 32.0) } else { 4.0 };
    let Some((left, top, w, h)) = pixel_box(outline, scale, 0.0, spread) else {
        return Ok(Bitmap::default());
    };
    if w > MAX_SDF_DIM || h > MAX_SDF_DIM {
        return Err(FontError::new(
            Tag(*b"glyf"),
            0,
            Reason::BadValue { field: "SDF bitmap size (px)", value: w.max(h) as i64 },
        ));
    }
    let xf = Xform { scale, dx: -(left as f32), dy: -(top as f32) };
    let mut segs: Vec<[f32; 4]> = Vec::new();
    flatten(outline, xf, |a, b| segs.push([a[0], a[1], b[0], b[1]]));
    let (w, h) = (w as usize, h as usize);
    let mut pixels = vec![0u8; w * h];
    // Segments crossing the current scanline (for the winding number), with their x there.
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for row in 0..h {
        let py = row as f32 + 0.5;
        crossings.clear();
        crossings.extend(segs.iter().filter_map(|s| {
            let (y0, y1) = (s[1], s[3]);
            if (y0 <= py) == (y1 <= py) {
                return None;
            }
            let t = (py - y0) / (y1 - y0);
            Some((s[0] + t * (s[2] - s[0]), if y1 > y0 { 1 } else { -1 }))
        }));
        for col in 0..w {
            let px = col as f32 + 0.5;
            let winding: i32 = crossings.iter().filter(|c| c.0 > px).map(|c| c.1).sum();
            let mut best = f32::INFINITY;
            for s in &segs {
                let (dx, dy) = (s[2] - s[0], s[3] - s[1]);
                let len2 = dx * dx + dy * dy;
                let t = if len2 > 0.0 { (((px - s[0]) * dx + (py - s[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                let (ex, ey) = (s[0] + t * dx - px, s[1] + t * dy - py);
                best = best.min(ex * ex + ey * ey);
            }
            let dist = best.sqrt();
            let signed = if winding != 0 { dist } else { -dist };
            let v = (0.5 + signed / (2.0 * spread)).clamp(0.0, 1.0);
            pixels[row * w + col] = (v * 255.0 + 0.5) as u8;
        }
    }
    Ok(Bitmap { width: w as u32, height: h as u32, left, top, pixels })
}

impl Font {
    /// Rasterise a glyph to an anti-aliased coverage bitmap at `size_px` pixels per em,
    /// shifted right by `offset_x` px (use 0, 0.25, 0.5, 0.75 for quarter-pixel positioning).
    pub fn rasterize_glyph(&self, glyph: GlyphId, size_px: f32, offset_x: f32) -> Result<Bitmap, FontError> {
        let outline = self.glyph_outline(glyph)?;
        Rasterizer::new().rasterize(&outline, self.scale_for_px(size_px), offset_x)
    }

    /// Signed-distance bitmap of a glyph at `size_px`, with a distance range of `spread` px.
    pub fn rasterize_glyph_sdf(&self, glyph: GlyphId, size_px: f32, spread: f32) -> Result<Bitmap, FontError> {
        let outline = self.glyph_outline(glyph)?;
        rasterize_sdf(&outline, self.scale_for_px(size_px), spread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::Point;

    fn poly(contours: &[&[(f32, f32)]]) -> Outline {
        let mut o = Outline::default();
        for c in contours {
            for &(x, y) in c.iter() {
                o.points.push(Point { x, y, on_curve: true });
            }
            o.contour_ends.push(o.points.len());
        }
        o
    }

    fn total(b: &Bitmap) -> f32 {
        b.pixels.iter().map(|&p| p as f32 / 255.0).sum()
    }

    #[test]
    fn axis_aligned_square_is_exact() {
        let sq = poly(&[&[(0.0, 0.0), (0.0, 4.0), (4.0, 4.0), (4.0, 0.0)]]);
        let b = Rasterizer::new().rasterize(&sq, 1.0, 0.0).unwrap();
        assert_eq!((b.width, b.height, b.left, b.top), (4, 4, 0, -4));
        assert!(b.pixels.iter().all(|&p| p == 255), "{:?}", b.pixels);
        // Half-pixel shift: edge columns are half covered.
        let b = Rasterizer::new().rasterize(&sq, 1.0, 0.5).unwrap();
        assert_eq!(b.width, 5);
        for row in 0..4 {
            assert_eq!(b.get(0, row), 128);
            assert_eq!(b.get(1, row), 255);
            assert_eq!(b.get(4, row), 128);
        }
    }

    #[test]
    fn winding_direction_does_not_matter_and_area_is_exact() {
        let tri_cw = poly(&[&[(0.0, 0.0), (0.0, 10.0), (10.0, 0.0)]]);
        let tri_ccw = poly(&[&[(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)]]);
        let a = Rasterizer::new().rasterize(&tri_cw, 1.0, 0.0).unwrap();
        let b = Rasterizer::new().rasterize(&tri_ccw, 1.0, 0.0).unwrap();
        assert_eq!(a, b);
        assert!((total(&a) - 50.0).abs() < 0.2, "area {}", total(&a));
        // Diagonal pixels are exactly half covered.
        assert_eq!(a.get(0, 0), 128);
    }

    #[test]
    fn holes_cancel_and_overlaps_saturate() {
        let outer: &[(f32, f32)] = &[(0.0, 0.0), (0.0, 10.0), (10.0, 10.0), (10.0, 0.0)];
        let hole: &[(f32, f32)] = &[(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)];
        let same: &[(f32, f32)] = &[(3.0, 3.0), (3.0, 7.0), (7.0, 7.0), (7.0, 3.0)];
        let b = Rasterizer::new().rasterize(&poly(&[outer, hole]), 1.0, 0.0).unwrap();
        assert!((total(&b) - 84.0).abs() < 0.1);
        assert_eq!(b.get(5, 5), 0);
        // Same-direction overlap: non-zero winding fills it, no double counting.
        let b = Rasterizer::new().rasterize(&poly(&[outer, same]), 1.0, 0.0).unwrap();
        assert!((total(&b) - 100.0).abs() < 0.1);
        assert_eq!(b.get(5, 5), 255);
    }

    #[test]
    fn curves_approximate_circle_area() {
        // A circle of radius 20 from 8 quadratic arcs (off-curve corners of an octagon).
        let r = 20.0f32;
        let mut o = Outline::default();
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::FRAC_PI_4;
            let on = (r * a.cos(), r * a.sin());
            let am = a + std::f32::consts::FRAC_PI_8;
            let k = r / std::f32::consts::FRAC_PI_8.cos();
            o.points.push(Point { x: on.0 + 30.0, y: on.1 + 30.0, on_curve: true });
            o.points.push(Point { x: k * am.cos() + 30.0, y: k * am.sin() + 30.0, on_curve: false });
        }
        o.contour_ends.push(16);
        let b = Rasterizer::new().rasterize(&o, 1.0, 0.0).unwrap();
        let area = std::f32::consts::PI * r * r;
        assert!((total(&b) - area).abs() / area < 0.01, "{} vs {}", total(&b), area);
    }

    #[test]
    fn sdf_sign_and_edge() {
        let sq = poly(&[&[(0.0, 0.0), (0.0, 20.0), (20.0, 20.0), (20.0, 0.0)]]);
        let b = rasterize_sdf(&sq, 1.0, 4.0).unwrap();
        assert_eq!(b.width, 28);
        // Centre is deep inside, corner outside, the edge column is ~128.
        assert_eq!(b.get(14, 14), 255);
        assert_eq!(b.get(0, 0), 0);
        let edge = b.get(4, 14) as i32; // pixel centre 0.5 px inside the left edge
        assert!((edge - 144).abs() <= 2, "edge {edge}");
    }

    #[test]
    fn absurd_sizes_are_errors_not_allocations() {
        let sq = poly(&[&[(0.0, 0.0), (0.0, 30000.0), (30000.0, 30000.0), (30000.0, 0.0)]]);
        let e = Rasterizer::new().rasterize(&sq, 1.0, 0.0).unwrap_err();
        assert_eq!(e.reason.code(), "bad_value");
        // SDFs have a much smaller cap (brute-force cost).
        assert!(rasterize_sdf(&sq, 0.005, 4.0).is_ok());
        assert_eq!(rasterize_sdf(&sq, 0.1, 4.0).unwrap_err().reason.code(), "bad_value");
        assert!(rasterize_sdf(&sq, 0.001, f32::NAN).is_ok());
        // NaN / infinite scales give an empty bitmap rather than a panic.
        assert!(Rasterizer::new().rasterize(&sq, f32::NAN, 0.0).unwrap().is_empty());
        assert!(Rasterizer::new().rasterize(&sq, f32::INFINITY, 0.0).is_err() ||
            Rasterizer::new().rasterize(&sq, f32::INFINITY, 0.0).unwrap().is_empty());
    }
}

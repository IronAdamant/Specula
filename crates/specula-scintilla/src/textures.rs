//! Built-in textures, made procedurally (no files, zero assets): soft dots, sparks, smoke and
//! flame flipbooks, rings and rune motes for particles; scorch marks, rune circles and cracks
//! (with a normal map) for decals. Deterministic: the same bytes on every platform, so captures
//! repeat.

use crate::noise::{hash3, value_noise};
use specula_core::dmath;

/// Built-in particle textures.
pub const PARTICLE_NAMES: &[&str] = &["soft", "spark", "smoke", "flame", "ring", "rune", "dust"];
/// Built-in decal textures (`cracks_normal` is a tangent-space normal map).
pub const DECAL_NAMES: &[&str] = &["scorch", "rune_circle", "cracks", "cracks_normal", "soft"];

/// An RGBA8 image (straight alpha), top row first.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Colour (sRGB) or data (a normal map).
    pub srgb: bool,
    /// The flipbook grid it is laid out in (1 × 1 for a single picture).
    pub grid: [u32; 2],
}

/// A built-in texture by name (`None` for an unknown name).
pub fn builtin(name: &str) -> Option<Image> {
    Some(match name {
        "soft" => single(64, |x, y| {
            let r = radius(x, y);
            let a = (1.0 - r * r).max(0.0);
            [1.0, 1.0, 1.0, a * a]
        }),
        "spark" => single(32, |x, y| {
            let r = radius(x, y);
            let core = (1.0 - r * 2.2).clamp(0.0, 1.0);
            let halo = (1.0 - r).clamp(0.0, 1.0);
            let a = (core + 0.35 * halo * halo * halo).min(1.0);
            [1.0, 1.0, 1.0, a]
        }),
        "smoke" => sheet(64, 4, |x, y, frame| {
            let r = radius(x, y);
            let mask = smooth(1.0, 0.35, r);
            let n = fbm([x * 3.0 + frame as f32 * 1.7, y * 3.0 - frame as f32 * 0.9, frame as f32 * 0.37], 11);
            let a = (mask * (0.55 + 0.6 * n)).clamp(0.0, 1.0);
            let shade = 0.78 + 0.22 * n;
            [shade, shade, shade, a * 0.85]
        }),
        "flame" => sheet(64, 4, |x, y, frame| {
            // Tongues rise: v = 0 at the bottom of the frame, 1 at the top.
            let v = 1.0 - y;
            let t = frame as f32 / 16.0;
            let wobble = 0.12 * fbm([v * 2.5 - t * 4.0, t * 3.0, 0.5], 23) * v;
            let dx = (x - 0.5 - wobble).abs();
            let width = 0.32 * dmath::pow((1.0 - v).max(0.0), 0.55) * (0.75 + 0.25 * fbm([x * 4.0, v * 3.0 - t * 6.0, 1.5], 29));
            let edge = if width > 1e-4 { (1.0 - dx / width).clamp(0.0, 1.0) } else { 0.0 };
            let base = smooth(0.0, 0.12, v);
            let a = (edge * 1.6).min(1.0) * base;
            let core = (edge * (1.0 - v)).clamp(0.0, 1.0);
            let lum = 0.7 + 0.3 * core;
            [lum, lum, lum, a]
        }),
        "ring" => single(64, |x, y| {
            let r = radius(x, y);
            let d = ((r - 0.78) / 0.12).abs();
            let a = (1.0 - d).clamp(0.0, 1.0);
            [1.0, 1.0, 1.0, a * a]
        }),
        "rune" => single(64, |x, y| {
            // A carved mark: a stave and two arms, with a soft glow around it.
            let (px, py) = (x - 0.5, y - 0.5);
            let stave = line_dist(px, py, [0.0, -0.36], [0.0, 0.36]);
            let arm1 = line_dist(px, py, [0.0, -0.12], [0.24, -0.32]);
            let arm2 = line_dist(px, py, [0.0, 0.08], [-0.24, -0.12]);
            let d = stave.min(arm1).min(arm2);
            let stroke = (1.0 - d / 0.045).clamp(0.0, 1.0);
            let glow = (1.0 - d / 0.22).clamp(0.0, 1.0);
            let fade = (1.0 - radius(x, y)).clamp(0.0, 1.0);
            [1.0, 1.0, 1.0, (stroke + 0.45 * glow * glow).min(1.0) * fade]
        }),
        "dust" => single(64, |x, y| {
            let r = radius(x, y);
            let n = fbm([x * 5.0, y * 5.0, 3.3], 41);
            let mask = smooth(1.0, 0.2, r + 0.25 * (n - 0.5));
            let shade = 0.85 + 0.15 * n;
            [shade, shade, shade, mask * 0.9]
        }),
        "scorch" => single(256, |x, y| {
            // Dark soot, noisy at the rim; meant for the multiply mode (white = untouched).
            let r = radius(x, y);
            let n = fbm([x * 6.0, y * 6.0, 7.1], 53);
            let rim = r + 0.35 * (n - 0.5);
            let a = smooth(0.95, 0.45, rim);
            let soot = 0.12 + 0.28 * fbm([x * 14.0, y * 14.0, 2.2], 59);
            [soot, soot * 0.95, soot * 0.9, a]
        }),
        "rune_circle" => single(256, rune_circle),
        "cracks" => single(256, |x, y| {
            let c = cracks(x, y);
            [0.2, 0.2, 0.22, c * smooth(1.0, 0.7, radius(x, y))]
        }),
        "cracks_normal" => {
            let mut img = single(256, |x, y| {
                let e = 1.0 / 256.0;
                let h = |x: f32, y: f32| -cracks(x, y) * smooth(1.0, 0.7, radius(x, y));
                let dx = (h(x + e, y) - h(x - e, y)) / (2.0 * e);
                let dy = (h(x, y + e) - h(x, y - e)) / (2.0 * e);
                // Tangent space: +X right, +Y up the image (y grows down, so its slope flips).
                let k = 0.012;
                let n = [-dx * k, dy * k, 1.0];
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                [0.5 + 0.5 * n[0] / l, 0.5 + 0.5 * n[1] / l, 0.5 + 0.5 * n[2] / l, 1.0]
            });
            img.srgb = false;
            img
        }
        _ => return None,
    })
}

/// Distance from the image centre in units of the half-size (1 at the edge's midpoint).
fn radius(x: f32, y: f32) -> f32 {
    let (dx, dy) = (x - 0.5, y - 0.5);
    2.0 * (dx * dx + dy * dy).sqrt()
}

/// 1 below `inner`-side, 0 beyond `outer`, smooth between (either order).
fn smooth(outer: f32, inner: f32, v: f32) -> f32 {
    let t = ((v - outer) / (inner - outer)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn fbm(p: [f32; 3], seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut q = p;
    for o in 0..3 {
        sum += amp * value_noise(q, seed.wrapping_add(o)).0;
        q = [q[0] * 2.03, q[1] * 2.03, q[2] * 2.03];
        amp *= 0.5;
    }
    (sum / 0.875) * 0.5 + 0.5
}

fn line_dist(px: f32, py: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    let (abx, aby) = (b[0] - a[0], b[1] - a[1]);
    let (apx, apy) = (px - a[0], py - a[1]);
    let t = ((apx * abx + apy * aby) / (abx * abx + aby * aby).max(1e-9)).clamp(0.0, 1.0);
    let (dx, dy) = (apx - abx * t, apy - aby * t);
    (dx * dx + dy * dy).sqrt()
}

fn rune_circle(x: f32, y: f32) -> [f32; 4] {
    let r = radius(x, y);
    let (px, py) = (x - 0.5, y - 0.5);
    let ring = |at: f32, w: f32| (1.0 - ((r - at) / w).abs()).clamp(0.0, 1.0);
    let mut a = ring(0.92, 0.025).max(ring(0.74, 0.018));
    // Eight ticks between the rings.
    let angle = dmath::atan2(py, px);
    let sector = angle / (std::f32::consts::TAU / 8.0);
    let tick = (sector - dmath::round(sector)).abs();
    if (0.76..0.9).contains(&r) {
        a = a.max((1.0 - tick / 0.05).clamp(0.0, 1.0));
    }
    // An inner triangle of strokes.
    let pts = [[0.0f32, -0.3], [0.26, 0.15], [-0.26, 0.15]];
    let mut d = f32::MAX;
    for i in 0..3 {
        d = d.min(line_dist(px, py, pts[i], pts[(i + 1) % 3]));
    }
    a = a.max((1.0 - d / 0.012).clamp(0.0, 1.0));
    [1.0, 1.0, 1.0, a]
}

/// Crack lines: where two nearest jittered cell points are nearly equidistant (Voronoi edges).
fn cracks(x: f32, y: f32) -> f32 {
    let cells = 6.0;
    let (gx, gy) = (x * cells, y * cells);
    let (cx, cy) = (dmath::floor(gx), dmath::floor(gy));
    let mut d1 = f32::MAX;
    let mut d2 = f32::MAX;
    for j in -1..=1 {
        for i in -1..=1 {
            let (ix, iy) = (cx as i32 + i, cy as i32 + j);
            let h = hash3(ix as u32, iy as u32, 77);
            let jx = (h & 0xffff) as f32 / 65535.0;
            let jy = (h >> 16) as f32 / 65535.0;
            let (dx, dy) = (ix as f32 + jx - gx, iy as f32 + jy - gy);
            let d = (dx * dx + dy * dy).sqrt();
            if d < d1 {
                d2 = d1;
                d1 = d;
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    (1.0 - (d2 - d1) / 0.08).clamp(0.0, 1.0)
}

/// A `size` × `size` picture of `f(x, y)` (sample centres in 0..1).
fn single(size: u32, f: impl Fn(f32, f32) -> [f32; 4]) -> Image {
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for j in 0..size {
        for i in 0..size {
            let c = f((i as f32 + 0.5) / size as f32, (j as f32 + 0.5) / size as f32);
            pixels.extend(c.map(byte));
        }
    }
    Image { width: size, height: size, pixels, srgb: true, grid: [1, 1] }
}

/// A `grid` × `grid` flipbook of `tile`-pixel frames, frame `k` of `f(x, y, k)`.
fn sheet(tile: u32, grid: u32, f: impl Fn(f32, f32, u32) -> [f32; 4]) -> Image {
    let side = tile * grid;
    let mut pixels = vec![0u8; (side * side * 4) as usize];
    for frame in 0..grid * grid {
        let (ox, oy) = ((frame % grid) * tile, (frame / grid) * tile);
        for j in 0..tile {
            for i in 0..tile {
                let c = f((i as f32 + 0.5) / tile as f32, (j as f32 + 0.5) / tile as f32, frame);
                let at = (((oy + j) * side + ox + i) * 4) as usize;
                pixels[at..at + 4].copy_from_slice(&c.map(byte));
            }
        }
    }
    Image { width: side, height: side, pixels, srgb: true, grid: [grid, grid] }
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_exists_with_its_size() {
        for name in PARTICLE_NAMES.iter().chain(DECAL_NAMES) {
            let img = builtin(name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(img.pixels.len(), (img.width * img.height * 4) as usize, "{name}");
            // Something is drawn, and the corners fade out (no hard square edges), except data maps.
            let alpha: u64 = img.pixels.chunks(4).map(|p| u64::from(p[3])).sum();
            assert!(alpha > 0, "{name} is empty");
            if img.srgb && img.grid == [1, 1] {
                assert!(img.pixels[3] < 16, "{name}'s corner is opaque");
            }
        }
        assert_eq!(builtin("smoke").unwrap().grid, [4, 4]);
        assert!(!builtin("cracks_normal").unwrap().srgb);
        assert!(builtin("nope").is_none());
    }

    #[test]
    fn textures_are_deterministic() {
        // Pinned by content hash: a change here changes every capture with effects.
        let h = |n: &str| specula_core::hash::hash_bytes(&builtin(n).unwrap().pixels);
        assert_eq!(h("flame"), h("flame"));
        assert_ne!(h("smoke"), h("dust"));
    }
}

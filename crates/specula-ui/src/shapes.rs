//! Built-in vector icons, tessellated into anti-aliased triangles. Each shape is designed in a
//! unit box (`0..1`, y down) from convex pieces, then mapped onto the icon's rect (and rotated
//! about its centre). Silhouettes follow the Sepulcra style bible: upright chamfered stones,
//! a pointed warding stone, a needle obelisk, a bowl brazier, a swallow-tailed wave banner.

use crate::color::Rgba;
use crate::draw::{arc_points, fill_outline, segments_for, Fill, Rect, Vertex};
use crate::screen::Shape;
use std::f32::consts::{PI, TAU};

type P = [f32; 2];

/// Reusable buffers for tessellation.
#[derive(Default, Debug)]
pub struct Scratch {
    pts: Vec<P>,
    aa: Vec<P>,
}

struct Pen<'a> {
    out: &'a mut Vec<Vertex>,
    s: &'a mut Scratch,
    rect: Rect,
    /// Unit square size in px (the icon is fitted into a square).
    unit: f32,
    origin: P,
    cos: f32,
    sin: f32,
    color: Rgba,
}

impl Pen<'_> {
    fn map(&self, p: P) -> P {
        let x = (p[0] - 0.5) * self.unit;
        let y = (p[1] - 0.5) * self.unit;
        let c = self.rect.center();
        let _ = self.origin;
        [c[0] + x * self.cos - y * self.sin, c[1] + x * self.sin + y * self.cos]
    }

    fn poly(&mut self, pts: &[P]) {
        self.s.pts.clear();
        for &p in pts {
            let m = self.map(p);
            self.s.pts.push(m);
        }
        let pts = std::mem::take(&mut self.s.pts);
        fill_outline(self.out, &mut self.s.aa, &pts, self.color, 1.0, Fill::Fan);
        self.s.pts = pts;
    }

    fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.poly(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1]]);
    }

    /// A circle or a convex arc slice (`a0..a1` radians, the chord closes it).
    fn disc(&mut self, c: P, r: f32, a0: f32, a1: f32) {
        let segs = segments_for(r * self.unit, a1 - a0);
        self.s.pts.clear();
        let mut pts = std::mem::take(&mut self.s.pts);
        let cm = self.map(c);
        let rot = self.sin.atan2(self.cos);
        let full = (a1 - a0) >= TAU - 1e-4;
        arc_points(&mut pts, cm, r * self.unit, a0 + rot, a1 + rot, segs);
        if full {
            pts.pop();
        }
        fill_outline(self.out, &mut self.s.aa, &pts, self.color, 1.0, Fill::Fan);
        self.s.pts = pts;
    }

    /// An annulus band between radii `r0 < r1`, from `a0` to `a1`.
    fn band(&mut self, c: P, r0: f32, r1: f32, a0: f32, a1: f32) {
        let cm = self.map(c);
        let rot = self.sin.atan2(self.cos);
        band(self.out, self.s, cm, r0 * self.unit, r1 * self.unit, a0 + rot, a1 + rot, self.color);
    }

    /// A thick line segment.
    fn stroke(&mut self, a: P, b: P, w: f32) {
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-6);
        let n = [-d[1] / len * w * 0.5, d[0] / len * w * 0.5];
        self.poly(&[[a[0] + n[0], a[1] + n[1]], [b[0] + n[0], b[1] + n[1]], [b[0] - n[0], b[1] - n[1]], [a[0] - n[0], a[1] - n[1]]]);
    }
}

/// An anti-aliased annulus sector (radial menu wedges, rings), physical px.
#[allow(clippy::too_many_arguments)]
pub fn band(out: &mut Vec<Vertex>, s: &mut Scratch, c: P, r0: f32, r1: f32, a0: f32, a1: f32, color: Rgba) {
    let segs = segments_for(r1, (a1 - a0).abs());
    s.pts.clear();
    let mut pts = std::mem::take(&mut s.pts);
    arc_points(&mut pts, c, r1, a0, a1, segs);
    arc_points(&mut pts, c, r0, a1, a0, segs);
    fill_outline(out, &mut s.aa, &pts, color, 1.0, Fill::Strip);
    s.pts = pts;
}

/// A convex polygon in physical px.
pub fn polygon(out: &mut Vec<Vertex>, s: &mut Scratch, pts: &[P], color: Rgba) {
    fill_outline(out, &mut s.aa, pts, color, 1.0, Fill::Fan);
}

/// Tessellate `shape` into `rect` (fitted as a square, centred), rotated by `rotation` radians.
pub fn draw_shape(out: &mut Vec<Vertex>, s: &mut Scratch, shape: Shape, rect: Rect, rotation: f32, color: Rgba) {
    let unit = rect.w.min(rect.h);
    if unit <= 0.0 || color[3] <= 0.0 {
        return;
    }
    let mut p = Pen { out, s, rect, unit, origin: [rect.x, rect.y], cos: rotation.cos(), sin: rotation.sin(), color };
    match shape {
        Shape::Circle => p.disc([0.5, 0.5], 0.42, 0.0, TAU),
        Shape::Ring => p.band([0.5, 0.5], 0.3, 0.44, 0.0, TAU),
        Shape::Diamond => p.poly(&[[0.5, 0.06], [0.94, 0.5], [0.5, 0.94], [0.06, 0.5]]),
        Shape::Triangle => p.poly(&[[0.5, 0.1], [0.93, 0.88], [0.07, 0.88]]),
        Shape::Arrow => {
            p.rect(0.08, 0.4, 0.56, 0.6);
            p.poly(&[[0.5, 0.14], [0.94, 0.5], [0.5, 0.86]]);
        }
        Shape::Chevron => {
            p.poly(&[[0.22, 0.1], [0.46, 0.1], [0.82, 0.5], [0.58, 0.5]]);
            p.poly(&[[0.58, 0.5], [0.82, 0.5], [0.46, 0.9], [0.22, 0.9]]);
        }
        Shape::Tombstone => {
            p.rect(0.24, 0.42, 0.76, 0.88);
            p.disc([0.5, 0.42], 0.26, PI, TAU);
            p.rect(0.12, 0.86, 0.88, 0.95);
        }
        Shape::Dust => {
            // A mound of grave-dust with two glints.
            p.disc([0.5, 0.92], 0.4, PI, TAU);
            p.poly(&[[0.3, 0.12], [0.36, 0.26], [0.3, 0.4], [0.24, 0.26]]);
            p.poly(&[[0.7, 0.2], [0.75, 0.3], [0.7, 0.4], [0.65, 0.3]]);
        }
        Shape::Wave => {
            // A swallow-tailed banner on a pole.
            p.rect(0.16, 0.06, 0.25, 0.95);
            p.poly(&[[0.25, 0.1], [0.86, 0.1], [0.7, 0.31], [0.25, 0.31]]);
            p.poly(&[[0.25, 0.31], [0.7, 0.31], [0.86, 0.52], [0.25, 0.52]]);
        }
        Shape::Rune => {
            p.stroke([0.4, 0.08], [0.4, 0.92], 0.13);
            p.stroke([0.4, 0.3], [0.72, 0.5], 0.13);
            p.stroke([0.72, 0.5], [0.4, 0.7], 0.13);
        }
        Shape::Warding => p.poly(&[[0.32, 0.95], [0.32, 0.32], [0.5, 0.05], [0.68, 0.32], [0.68, 0.95]]),
        Shape::Pillar => {
            p.rect(0.36, 0.22, 0.64, 0.86);
            p.rect(0.27, 0.1, 0.73, 0.2);
            p.rect(0.27, 0.86, 0.73, 0.95);
            p.band([0.5, 0.52], 0.2, 0.27, PI * 0.9, PI * 2.1);
        }
        Shape::Brazier => {
            p.disc([0.5, 0.48], 0.36, 0.0, PI);
            p.rect(0.44, 0.8, 0.56, 0.88);
            p.rect(0.28, 0.87, 0.72, 0.95);
            p.poly(&[[0.5, 0.04], [0.64, 0.28], [0.5, 0.42], [0.36, 0.28]]);
        }
        Shape::Obelisk => p.poly(&[[0.4, 0.95], [0.44, 0.2], [0.5, 0.03], [0.56, 0.2], [0.6, 0.95]]),
        Shape::Wall => {
            p.rect(0.06, 0.3, 0.46, 0.5);
            p.rect(0.5, 0.3, 0.94, 0.5);
            p.rect(0.06, 0.54, 0.26, 0.74);
            p.rect(0.3, 0.54, 0.7, 0.74);
            p.rect(0.74, 0.54, 0.94, 0.74);
            p.rect(0.06, 0.78, 0.46, 0.94);
            p.rect(0.5, 0.78, 0.94, 0.94);
        }
        Shape::Cross => {
            p.stroke([0.18, 0.18], [0.82, 0.82], 0.16);
            p.stroke([0.82, 0.18], [0.18, 0.82], 0.16);
        }
        Shape::Check => {
            p.stroke([0.12, 0.52], [0.4, 0.8], 0.16);
            p.stroke([0.36, 0.8], [0.88, 0.22], 0.16);
        }
        Shape::Pause => {
            p.rect(0.22, 0.12, 0.42, 0.88);
            p.rect(0.58, 0.12, 0.78, 0.88);
        }
        Shape::Play => p.poly(&[[0.22, 0.1], [0.88, 0.5], [0.22, 0.9]]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::SHAPES;

    #[test]
    fn every_shape_tessellates_inside_its_box() {
        let mut out = Vec::new();
        let mut s = Scratch::default();
        let rect = Rect::new(100.0, 50.0, 40.0, 40.0);
        for (name, shape) in SHAPES {
            out.clear();
            draw_shape(&mut out, &mut s, shape, rect, 0.0, [1.0; 4]);
            assert!(!out.is_empty() && out.len() % 3 == 0, "{name}");
            for v in &out {
                assert!(v.pos.iter().all(|c| c.is_finite()), "{name}");
                assert!(rect.grow(1.5).contains(v.pos), "{name}: {:?}", v.pos);
            }
        }
    }

    #[test]
    fn rotation_turns_an_arrow() {
        let mut out = Vec::new();
        let mut s = Scratch::default();
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        draw_shape(&mut out, &mut s, Shape::Arrow, rect, std::f32::consts::FRAC_PI_2, [1.0; 4]);
        // Pointing right at 0 rad; at +90° (y down) it points down: the tip is the max y.
        let tip = out.iter().map(|v| v.pos[1]).fold(f32::MIN, f32::max);
        assert!(tip > 93.0, "{tip}");
        let maxx = out.iter().map(|v| v.pos[0]).fold(f32::MIN, f32::max);
        assert!(maxx < 88.0, "{maxx}");
    }
}

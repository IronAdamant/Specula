//! Glyph outlines from the `glyf` table: simple glyphs (quadratic B-spline contours) and
//! composite glyphs (components with offsets, scales, 2x2 transforms and point matching).

use crate::error::{FontError, Reason};
use crate::font::{Font, GlyphId};
use crate::reader::View;

/// Composite glyphs may nest at most this deep (deeper nesting is treated as a cycle).
pub const MAX_COMPOSITE_DEPTH: u32 = 16;
/// A glyph may expand to at most this many points (guards against composite bombs).
pub const MAX_POINTS: usize = 1 << 18;
/// At most this many component references are followed while loading one glyph.
const MAX_COMPONENTS: u32 = 4096;

/// One outline point in font units (y up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    /// On-curve point; off-curve points are quadratic Bézier control points.
    pub on_curve: bool,
}

/// A segment of an outline after the implied on-curve points are made explicit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Line([f32; 2], [f32; 2]),
    /// Start, control, end.
    Quad([f32; 2], [f32; 2], [f32; 2]),
}

/// A glyph outline in font units: points plus the (exclusive) end index of each contour.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    pub points: Vec<Point>,
    pub contour_ends: Vec<usize>,
}

impl Outline {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Control-point bounding box `[x_min, y_min, x_max, y_max]` (contains the curves), or `None`.
    pub fn bounds(&self) -> Option<[f32; 4]> {
        let mut it = self.points.iter();
        let first = it.next()?;
        let mut b = [first.x, first.y, first.x, first.y];
        for p in it {
            b[0] = b[0].min(p.x);
            b[1] = b[1].min(p.y);
            b[2] = b[2].max(p.x);
            b[3] = b[3].max(p.y);
        }
        Some(b)
    }

    /// Call `f` for every line / quadratic segment of every closed contour.
    pub fn segments(&self, mut f: impl FnMut(Segment)) {
        let mut start = 0;
        for &end in &self.contour_ends {
            if end > start && end <= self.points.len() {
                contour_segments(&self.points[start..end], &mut f);
            }
            start = end;
        }
    }
}

fn mid(a: Point, b: Point) -> [f32; 2] {
    [(a.x + b.x) * 0.5, (a.y + b.y) * 0.5]
}

fn contour_segments(pts: &[Point], f: &mut impl FnMut(Segment)) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    // Start on an on-curve point; with none, start at the implied midpoint of the last and first.
    let (start, first) = match pts.iter().position(|p| p.on_curve) {
        Some(i) => ([pts[i].x, pts[i].y], i + 1),
        None => (mid(pts[n - 1], pts[0]), 0),
    };
    let mut cur = start;
    let mut ctrl: Option<[f32; 2]> = None;
    let count = if first == 0 { n } else { n - 1 };
    for k in 0..count {
        let p = pts[(first + k) % n];
        let pp = [p.x, p.y];
        if p.on_curve {
            match ctrl.take() {
                Some(c) => f(Segment::Quad(cur, c, pp)),
                None => f(Segment::Line(cur, pp)),
            }
            cur = pp;
        } else {
            if let Some(c) = ctrl {
                let m = [(c[0] + pp[0]) * 0.5, (c[1] + pp[1]) * 0.5];
                f(Segment::Quad(cur, c, m));
                cur = m;
            }
            ctrl = Some(pp);
        }
    }
    match ctrl {
        Some(c) => f(Segment::Quad(cur, c, start)),
        None => {
            if cur != start {
                f(Segment::Line(cur, start));
            }
        }
    }
}

impl Font {
    /// The outline of a glyph in font units. Glyphs without contours (space) give an empty outline.
    pub fn glyph_outline(&self, glyph: GlyphId) -> Result<Outline, FontError> {
        let mut out = Outline::default();
        let mut budget = MAX_COMPONENTS;
        load_glyph(self, glyph, 0, &mut budget, &mut out)?;
        Ok(out)
    }
}

fn load_glyph(font: &Font, glyph: GlyphId, depth: u32, budget: &mut u32, out: &mut Outline) -> Result<(), FontError> {
    let v = font.glyph_view(glyph)?;
    if v.len() == 0 {
        return Ok(());
    }
    let contours = v.i16(0)?;
    if contours >= 0 {
        simple_glyph(v, contours as usize, out)
    } else {
        composite_glyph(font, v, depth, budget, out)
    }
}

fn simple_glyph(v: View<'_>, contours: usize, out: &mut Outline) -> Result<(), FontError> {
    if contours == 0 {
        return Ok(());
    }
    let mut ends = Vec::with_capacity(contours);
    let mut prev: i64 = -1;
    for i in 0..contours {
        let e = v.u16(10 + i * 2)? as i64;
        if e < prev {
            return Err(v.bad(10 + i * 2, "glyf.endPtsOfContours not increasing", e));
        }
        prev = e;
        ends.push(e as usize + 1);
    }
    let n = prev as usize + 1;
    if out.points.len() + n > MAX_POINTS {
        return Err(v.err(0, Reason::TooManyPoints { points: MAX_POINTS }));
    }
    let ins_len = v.u16(10 + contours * 2)? as usize;
    let mut at = 12 + contours * 2 + ins_len;

    let mut flags = Vec::with_capacity(n);
    while flags.len() < n {
        let flag = v.u8(at)?;
        at += 1;
        flags.push(flag);
        if flag & 8 != 0 {
            let repeat = v.u8(at)? as usize;
            at += 1;
            for _ in 0..repeat.min(n - flags.len()) {
                flags.push(flag);
            }
        }
    }

    let base = out.points.len();
    let mut x: i64 = 0;
    for &flag in &flags {
        if flag & 2 != 0 {
            let d = v.u8(at)? as i64;
            at += 1;
            x += if flag & 16 != 0 { d } else { -d };
        } else if flag & 16 == 0 {
            x += v.i16(at)? as i64;
            at += 2;
        }
        out.points.push(Point { x: x as f32, y: 0.0, on_curve: flag & 1 != 0 });
    }
    let mut y: i64 = 0;
    for (i, &flag) in flags.iter().enumerate() {
        if flag & 4 != 0 {
            let d = v.u8(at)? as i64;
            at += 1;
            y += if flag & 32 != 0 { d } else { -d };
        } else if flag & 32 == 0 {
            y += v.i16(at)? as i64;
            at += 2;
        }
        out.points[base + i].y = y as f32;
    }
    for e in ends {
        out.contour_ends.push(base + e);
    }
    Ok(())
}

fn composite_glyph(
    font: &Font,
    v: View<'_>,
    depth: u32,
    budget: &mut u32,
    out: &mut Outline,
) -> Result<(), FontError> {
    if depth >= MAX_COMPOSITE_DEPTH {
        return Err(v.err(0, Reason::CompositeTooDeep { depth: MAX_COMPOSITE_DEPTH }));
    }
    let mut at = 10;
    loop {
        if *budget == 0 {
            return Err(v.err(at, Reason::TooManyPoints { points: MAX_POINTS }));
        }
        *budget -= 1;
        let flags = v.u16(at)?;
        let child_id = v.u16(at + 2)?;
        at += 4;
        let xy = flags & 0x0002 != 0;
        let (arg1, arg2) = if flags & 0x0001 != 0 {
            let r = if xy {
                (v.i16(at)? as i32, v.i16(at + 2)? as i32)
            } else {
                (v.u16(at)? as i32, v.u16(at + 2)? as i32)
            };
            at += 4;
            r
        } else {
            let r = if xy { (v.i8(at)? as i32, v.i8(at + 1)? as i32) } else { (v.u8(at)? as i32, v.u8(at + 1)? as i32) };
            at += 2;
            r
        };
        // [a, b, c, d]: x' = a*x + c*y, y' = b*x + d*y
        let m = if flags & 0x0008 != 0 {
            let s = v.f2dot14(at)?;
            at += 2;
            [s, 0.0, 0.0, s]
        } else if flags & 0x0040 != 0 {
            let r = [v.f2dot14(at)?, 0.0, 0.0, v.f2dot14(at + 2)?];
            at += 4;
            r
        } else if flags & 0x0080 != 0 {
            let r = [v.f2dot14(at)?, v.f2dot14(at + 2)?, v.f2dot14(at + 4)?, v.f2dot14(at + 6)?];
            at += 8;
            r
        } else {
            [1.0, 0.0, 0.0, 1.0]
        };

        let mut child = Outline::default();
        load_glyph(font, child_id, depth + 1, budget, &mut child)?;
        for p in &mut child.points {
            let (x, y) = (p.x, p.y);
            p.x = m[0] * x + m[2] * y;
            p.y = m[1] * x + m[3] * y;
        }
        let (dx, dy) = if xy {
            let (ox, oy) = (arg1 as f32, arg2 as f32);
            // SCALED_COMPONENT_OFFSET (Apple) without UNSCALED_COMPONENT_OFFSET (Microsoft default).
            if flags & 0x0800 != 0 && flags & 0x1000 == 0 {
                (m[0] * ox + m[2] * oy, m[1] * ox + m[3] * oy)
            } else {
                (ox, oy)
            }
        } else {
            // Point matching: align parent point arg1 with child point arg2.
            let parent = out.points.get(arg1 as usize).copied();
            let childp = child.points.get(arg2 as usize).copied();
            match (parent, childp) {
                (Some(p), Some(c)) => (p.x - c.x, p.y - c.y),
                _ => return Err(v.bad(at, "composite point-matching index", arg1.max(arg2) as i64)),
            }
        };
        if out.points.len() + child.points.len() > MAX_POINTS {
            return Err(v.err(0, Reason::TooManyPoints { points: MAX_POINTS }));
        }
        let base = out.points.len();
        out.points.extend(child.points.iter().map(|p| Point { x: p.x + dx, y: p.y + dy, on_curve: p.on_curve }));
        out.contour_ends.extend(child.contour_ends.iter().map(|e| e + base));

        if flags & 0x0020 == 0 {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::tests::tiny_font;

    fn pt(x: f32, y: f32, on: bool) -> Point {
        Point { x, y, on_curve: on }
    }

    #[test]
    fn simple_and_composite_outlines() {
        let font = Font::from_bytes(tiny_font()).unwrap();
        assert!(font.glyph_outline(0).unwrap().is_empty());
        let sq = font.glyph_outline(1).unwrap();
        assert_eq!(sq.points.len(), 4);
        assert_eq!(sq.contour_ends, vec![4]);
        assert_eq!(sq.bounds(), Some([100.0, 0.0, 600.0, 500.0]));
        let o = font.glyph_outline(2).unwrap();
        assert_eq!(o.contour_ends, vec![4, 8]);
        let c = font.glyph_outline(3).unwrap();
        assert_eq!(c.bounds(), Some([150.0, 100.0, 650.0, 600.0]));
        let pm = font.glyph_outline(4).unwrap();
        assert_eq!(pm.contour_ends, vec![4, 8]);
        assert_eq!(pm.bounds(), Some([100.0, 0.0, 850.0, 750.0]));
        assert_eq!(pm.points[4], Point { x: 600.0, y: 500.0, on_curve: true });
        assert!(font.glyph_outline(5).is_err());
    }

    #[test]
    fn implied_points_become_quads() {
        // Two off-curve points between on-curve points imply a midpoint.
        let o = Outline {
            points: vec![pt(0.0, 0.0, true), pt(0.0, 10.0, false), pt(10.0, 10.0, false), pt(10.0, 0.0, true)],
            contour_ends: vec![4],
        };
        let mut segs = Vec::new();
        o.segments(|s| segs.push(s));
        assert_eq!(
            segs,
            vec![
                Segment::Quad([0.0, 0.0], [0.0, 10.0], [5.0, 10.0]),
                Segment::Quad([5.0, 10.0], [10.0, 10.0], [10.0, 0.0]),
                Segment::Line([10.0, 0.0], [0.0, 0.0]),
            ]
        );
        // All off-curve: a closed curve through the midpoints.
        let o = Outline {
            points: vec![pt(0.0, 0.0, false), pt(0.0, 10.0, false), pt(10.0, 10.0, false), pt(10.0, 0.0, false)],
            contour_ends: vec![4],
        };
        let mut segs = Vec::new();
        o.segments(|s| segs.push(s));
        assert_eq!(segs.len(), 4);
        assert!(segs.iter().all(|s| matches!(s, Segment::Quad(..))));
        // Closed: the last end equals the first start.
        let first = match segs[0] {
            Segment::Quad(a, _, _) => a,
            _ => unreachable!(),
        };
        let last = match segs[3] {
            Segment::Quad(_, _, b) => b,
            _ => unreachable!(),
        };
        assert_eq!(first, last);
        assert_eq!(first, [5.0, 0.0]);
    }
}

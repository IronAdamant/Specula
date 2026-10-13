//! The UI's output: a flat, ordered list of draw commands in **physical pixels** (origin top-left,
//! y down; colours sRGB, straight alpha), for the GPU overlay or the software [`crate::preview`].
//!
//! - [`DrawCmd::Rect`]: a rounded rectangle with a vertical-gradient fill and border and a feather
//!   (anti-aliasing / soft-shadow width). Signed-distance friendly: one quad per rect.
//! - [`DrawCmd::Image`]: a textured quad (texture ids are the game's).
//! - [`DrawCmd::Glyphs`]: a run of `specula_text::GlyphQuad`s sampling the UI's glyph atlas.
//! - [`DrawCmd::Mesh`]: coloured triangles (vector icons, radial wedges, threat arrows), already
//!   anti-aliased with a 1 px alpha fringe, so a plain triangle pipeline draws them correctly.
//! - [`DrawCmd::Clip`]: scissor for the following commands (`None` = whole target).

use crate::color::Rgba;
pub use specula_text::GlyphQuad;

/// An axis-aligned rectangle, physical px.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center(&self) -> [f32; 2] {
        [self.x + self.w * 0.5, self.y + self.h * 0.5]
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        p[0] >= self.x && p[0] < self.right() && p[1] >= self.y && p[1] < self.bottom()
    }

    /// Fully inside `outer` (with a small tolerance for float noise).
    pub fn inside(&self, outer: &Rect) -> bool {
        const E: f32 = 0.01;
        self.x >= outer.x - E && self.y >= outer.y - E && self.right() <= outer.right() + E && self.bottom() <= outer.bottom() + E
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        Rect { x: x0, y: y0, w: (x1 - x0).max(0.0), h: (y1 - y0).max(0.0) }
    }

    pub fn area(&self) -> f32 {
        self.w.max(0.0) * self.h.max(0.0)
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    /// Shrink by `[top, right, bottom, left]` (negative values grow).
    pub fn inset(&self, t: f32, r: f32, b: f32, l: f32) -> Rect {
        Rect { x: self.x + l, y: self.y + t, w: (self.w - l - r).max(0.0), h: (self.h - t - b).max(0.0) }
    }

    pub fn grow(&self, d: f32) -> Rect {
        Rect { x: self.x - d, y: self.y - d, w: self.w + 2.0 * d, h: self.h + 2.0 * d }
    }

    pub fn to_array(&self) -> [f32; 4] {
        [self.x, self.y, self.w, self.h]
    }
}

/// A rounded rectangle: fill gradient (top → bottom), then a border of `border` px inside the
/// edge, with its own gradient. `feather` is the edge softness in px (1 = crisp anti-aliasing,
/// larger = soft shadow).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectCmd {
    pub rect: Rect,
    pub radius: f32,
    pub fill: [Rgba; 2],
    pub border: f32,
    pub border_color: [Rgba; 2],
    pub feather: f32,
}

/// A coloured vertex of a [`DrawCmd::Mesh`] triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub color: Rgba,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DrawCmd {
    Rect(RectCmd),
    Image { rect: Rect, uv: [f32; 4], texture: u32, color: Rgba },
    /// `DrawList::glyphs[start..end]`, R8 coverage from atlas page `GlyphQuad::page`.
    Glyphs { start: u32, end: u32 },
    /// Triangles `DrawList::vertices[start..end]` (three vertices each).
    Mesh { start: u32, end: u32 },
    Clip(Option<Rect>),
}

/// One frame of UI drawing. Reused between frames (cleared, not freed).
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub cmds: Vec<DrawCmd>,
    pub glyphs: Vec<GlyphQuad>,
    pub vertices: Vec<Vertex>,
    /// Target size, physical px.
    pub size: [u32; 2],
    /// Physical px per logical px used for this frame.
    pub scale: f32,
}

impl DrawList {
    pub fn clear(&mut self) {
        self.cmds.clear();
        self.glyphs.clear();
        self.vertices.clear();
    }

    pub fn rect(&mut self, cmd: RectCmd) {
        if cmd.rect.w > 0.0 && cmd.rect.h > 0.0 && (cmd.fill[0][3] > 0.0 || cmd.fill[1][3] > 0.0 || (cmd.border > 0.0 && cmd.border_color.iter().any(|c| c[3] > 0.0))) {
            self.cmds.push(DrawCmd::Rect(cmd));
        }
    }

    /// Start a mesh command; finish it with [`DrawList::end_mesh`].
    pub fn begin_mesh(&mut self) -> u32 {
        self.vertices.len() as u32
    }

    pub fn end_mesh(&mut self, start: u32) {
        let end = self.vertices.len() as u32;
        if end > start {
            // Merge with an immediately preceding mesh.
            if let Some(DrawCmd::Mesh { end: e, .. }) = self.cmds.last_mut() {
                if *e == start {
                    *e = end;
                    return;
                }
            }
            self.cmds.push(DrawCmd::Mesh { start, end });
        }
    }

    pub fn glyph_run(&mut self, start: u32) {
        let end = self.glyphs.len() as u32;
        if end > start {
            if let Some(DrawCmd::Glyphs { end: e, .. }) = self.cmds.last_mut() {
                if *e == start {
                    *e = end;
                    return;
                }
            }
            self.cmds.push(DrawCmd::Glyphs { start, end });
        }
    }

    pub fn clip(&mut self, r: Option<Rect>) {
        if let Some(DrawCmd::Clip(c)) = self.cmds.last_mut() {
            *c = r;
            return;
        }
        self.cmds.push(DrawCmd::Clip(r));
    }

    /// Command counts, for budgets and reports.
    pub fn stats(&self) -> DrawStats {
        let mut s = DrawStats { glyphs: self.glyphs.len(), triangles: self.vertices.len() / 3, ..DrawStats::default() };
        for c in &self.cmds {
            match c {
                DrawCmd::Rect(_) => s.rects += 1,
                DrawCmd::Image { .. } => s.images += 1,
                DrawCmd::Glyphs { .. } => s.glyph_runs += 1,
                DrawCmd::Mesh { .. } => s.meshes += 1,
                DrawCmd::Clip(_) => s.clips += 1,
            }
        }
        s
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawStats {
    pub rects: usize,
    pub images: usize,
    pub glyph_runs: usize,
    pub glyphs: usize,
    pub meshes: usize,
    pub triangles: usize,
    pub clips: usize,
}

// ---- mesh building ----

type P = [f32; 2];

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn tri(out: &mut Vec<Vertex>, a: P, b: P, c: P, ca: Rgba, cb: Rgba, cc: Rgba) {
    out.push(Vertex { pos: a, color: ca });
    out.push(Vertex { pos: b, color: cb });
    out.push(Vertex { pos: c, color: cc });
}

/// How the inside of an outline is triangulated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    /// Convex polygon: a fan.
    Fan,
    /// An outline of `2n` points where point `k` faces point `2n - 1 - k` (an arc band: outer arc
    /// forward, inner arc back).
    Strip,
}

/// Fill a closed outline with an anti-aliased edge (an alpha fringe `aa` px wide, centred on the
/// outline). `scratch` holds the inset / outset points (reused, no allocation once warm).
pub fn fill_outline(out: &mut Vec<Vertex>, scratch: &mut Vec<P>, pts: &[P], color: Rgba, aa: f32, fill: Fill) {
    let n = pts.len();
    if n < 3 || color[3] <= 0.0 {
        return;
    }
    // Orientation: make normals point outward for either winding.
    let mut area = 0.0;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        area += a[0] * b[1] - b[0] * a[1];
    }
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    scratch.clear();
    // Per-vertex offset: average of adjacent edge normals, mitred (ImGui's method).
    for i in 0..n {
        let prev = pts[(i + n - 1) % n];
        let cur = pts[i];
        let next = pts[(i + 1) % n];
        let n0 = edge_normal(prev, cur, sign);
        let n1 = edge_normal(cur, next, sign);
        let mut dm = [(n0[0] + n1[0]) * 0.5, (n0[1] + n1[1]) * 0.5];
        let d2 = dm[0] * dm[0] + dm[1] * dm[1];
        if d2 > 1e-6 {
            let inv = (1.0 / d2).min(100.0);
            dm = [dm[0] * inv, dm[1] * inv];
        }
        let h = aa * 0.5;
        scratch.push([cur[0] - dm[0] * h, cur[1] - dm[1] * h]);
        scratch.push([cur[0] + dm[0] * h, cur[1] + dm[1] * h]);
    }
    let inner = |i: usize| scratch[2 * i];
    let outer = |i: usize| scratch[2 * i + 1];
    let clear = [color[0], color[1], color[2], 0.0];
    match fill {
        Fill::Fan => {
            for i in 1..n - 1 {
                tri(out, inner(0), inner(i), inner(i + 1), color, color, color);
            }
        }
        Fill::Strip => {
            let half = n / 2;
            for k in 0..half.saturating_sub(1) {
                let (a, b) = (inner(k), inner(k + 1));
                let (c, d) = (inner(n - 2 - k), inner(n - 1 - k));
                tri(out, a, b, c, color, color, color);
                tri(out, a, c, d, color, color, color);
            }
        }
    }
    for i in 0..n {
        let j = (i + 1) % n;
        tri(out, inner(i), inner(j), outer(j), color, color, clear);
        tri(out, inner(i), outer(j), outer(i), color, clear, clear);
    }
}

fn edge_normal(a: P, b: P, sign: f32) -> P {
    let d = sub(b, a);
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    if len < 1e-12 {
        return [0.0, 0.0];
    }
    // Counter-clockwise in y-down screen space (positive area): outward is (dy, -dx).
    [d[1] / len * sign, -d[0] / len * sign]
}

/// Points of a circle (or an arc from `a0` to `a1` radians, inclusive), appended to `out`.
pub fn arc_points(out: &mut Vec<P>, c: P, r: f32, a0: f32, a1: f32, segments: usize) {
    let segments = segments.max(1);
    for k in 0..=segments {
        let t = a0 + (a1 - a0) * k as f32 / segments as f32;
        let (s, co) = specula_core::dmath::sin_cos(t);
        out.push([c[0] + r * co, c[1] + r * s]);
    }
}

/// Segments for a circle of radius `r` px: smooth but bounded.
pub fn segments_for(r: f32, sweep: f32) -> usize {
    let full = (r * 0.9).clamp(12.0, 96.0);
    ((full * sweep / std::f32::consts::TAU).ceil() as usize).max(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_maths() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.center(), [60.0, 45.0]);
        assert!(r.contains([10.0, 20.0]) && !r.contains([110.0, 20.0]));
        assert_eq!(r.intersect(&Rect::new(100.0, 0.0, 50.0, 30.0)), Rect::new(100.0, 20.0, 10.0, 10.0));
        assert!(r.intersect(&Rect::new(200.0, 0.0, 5.0, 5.0)).is_empty());
        assert_eq!(r.inset(1.0, 2.0, 3.0, 4.0), Rect::new(14.0, 21.0, 94.0, 46.0));
        assert!(Rect::new(11.0, 21.0, 5.0, 5.0).inside(&r));
    }

    #[test]
    fn convex_fill_has_fan_and_fringe() {
        let mut v = Vec::new();
        let mut s = Vec::new();
        let sq = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        fill_outline(&mut v, &mut s, &sq, [1.0; 4], 1.0, Fill::Fan);
        // 2 fan triangles + 4 edges × 2 fringe triangles.
        assert_eq!(v.len(), (2 + 8) * 3);
        // Inner points are inset by half a pixel, outer outset: for either winding.
        let min_x = v.iter().map(|p| p.pos[0]).fold(f32::MAX, f32::min);
        assert!((min_x + 0.5).abs() < 1e-4, "{min_x}");
        let mut v2 = Vec::new();
        let rev: Vec<P> = sq.iter().rev().copied().collect();
        fill_outline(&mut v2, &mut s, &rev, [1.0; 4], 1.0, Fill::Fan);
        let min_x2 = v2.iter().map(|p| p.pos[0]).fold(f32::MAX, f32::min);
        assert!((min_x2 + 0.5).abs() < 1e-4, "{min_x2}");
    }

    #[test]
    fn draw_list_merges_runs() {
        let mut d = DrawList::default();
        let s = d.begin_mesh();
        d.vertices.extend([Vertex { pos: [0.0; 2], color: [1.0; 4] }; 3]);
        d.end_mesh(s);
        let s = d.begin_mesh();
        d.vertices.extend([Vertex { pos: [0.0; 2], color: [1.0; 4] }; 3]);
        d.end_mesh(s);
        assert_eq!(d.cmds, vec![DrawCmd::Mesh { start: 0, end: 6 }]);
        d.clip(None);
        d.clip(Some(Rect::new(0.0, 0.0, 1.0, 1.0)));
        assert_eq!(d.cmds.len(), 2);
        assert_eq!(d.stats().triangles, 2);
    }
}

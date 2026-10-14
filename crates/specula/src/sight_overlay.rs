//! Debug overlays drawn into sight captures (`sight_frame {overlays: [...]}`): a ground grid
//! with metre labels, world and entity axes, bounding boxes, entity labels, claim regions,
//! skeletons and model LODs.
//!
//! Everything is drawn with the GPU overlay (`FrameDesc::overlay`), after the scene and its
//! post-processing, **on top of everything** (an x-ray: a box behind a wall still shows; the ID
//! buffer, not the overlay, says what is visible). Lines are thin triangle strips with a 1 px
//! alpha fringe; 3D lines are clipped at the camera's near plane before projection, so lines
//! passing behind the camera still draw correctly. Text uses a system UI font through
//! `specula-text`; with no font the labels are skipped (the packet says so) and everything else
//! still draws. Overlays never appear unless asked for, and the packet lists what was drawn.
//!
//! Every list is capped (boxes, claims, skeletons, labels, quads), so a huge world can't blow
//! up a capture or its packet (foundation F1).

use crate::sight::NEAR;
use specula_core::math::{Mat4, Vec3, Vec4};
use specula_core::Json;
use specula_gpu::{Gpu, OverlayKind, OverlayQuad, TextureFormat, TextureId};
use specula_text::{AtlasConfig, Font, GlyphAtlas, TextStyle};
use std::path::PathBuf;

/// The overlays a capture can draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Grid,
    Axes,
    Bounds,
    Labels,
    Claims,
    Skeletons,
    Lods,
}

impl Kind {
    pub const ALL: [Kind; 7] = [Kind::Grid, Kind::Axes, Kind::Bounds, Kind::Labels, Kind::Claims, Kind::Skeletons, Kind::Lods];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Grid => "grid",
            Kind::Axes => "axes",
            Kind::Bounds => "bounds",
            Kind::Labels => "labels",
            Kind::Claims => "claims",
            Kind::Skeletons => "skeletons",
            Kind::Lods => "lods",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == s)
    }
}

/// Default and largest number of entity labels per capture.
pub const LABELS: (usize, usize) = (32, 64);
/// Grid labels per capture (on top of the entity labels).
const GRID_LABELS: usize = 16;
/// Boxes, claims and skeletons drawn per capture.
pub const MAX_BOXES: usize = 200;
pub const MAX_CLAIMS: usize = 64;
pub const MAX_SKELETONS: usize = 64;
/// Overlay records one capture's overlays may use (well under the GPU's limit).
pub const MAX_QUADS: usize = 60_000;
/// Label text size in pixels.
const TEXT_PX: f32 = 13.0;
/// Most characters of a label.
const MAX_LABEL_CHARS: usize = 48;

/// What `sight_frame {overlays, overlay_entities, max_labels}` asked for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spec {
    /// Sorted, no repeats.
    pub kinds: Vec<Kind>,
    /// Only these subtrees get boxes, labels, axes, skeletons and LOD tags (empty: everything
    /// on screen). Paths like "/district_3".
    pub entities: Vec<String>,
    pub max_labels: usize,
}

impl Spec {
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    pub fn has(&self, k: Kind) -> bool {
        self.kinds.contains(&k)
    }

    /// Whether the entity at `path` is selected.
    pub fn selects(&self, path: &str) -> bool {
        self.entities.is_empty()
            || self.entities.iter().any(|p| p == "/" || path == p || path.strip_prefix(p.as_str()).is_some_and(|r| r.starts_with('/')))
    }
}

// ---------------------------------------------------------------------------------------------
// Colours

pub const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
pub const VISIBLE: [f32; 4] = [0.31, 0.82, 1.0, 0.95];
pub const OCCLUDED: [f32; 4] = [1.0, 0.36, 0.36, 0.95];
pub const AXIS_X: [f32; 4] = [0.95, 0.25, 0.25, 1.0];
pub const AXIS_Y: [f32; 4] = [0.3, 0.9, 0.3, 1.0];
pub const AXIS_Z: [f32; 4] = [0.3, 0.5, 1.0, 1.0];
const GRID_MINOR: [f32; 4] = [0.75, 0.78, 0.85, 0.22];
const GRID_MAJOR: [f32; 4] = [0.85, 0.88, 0.95, 0.5];
const LABEL_BG: [f32; 4] = [0.05, 0.06, 0.08, 0.78];

/// A distinct colour per owner name (stable across runs).
pub fn owner_color(name: &str) -> [f32; 4] {
    const PALETTE: [[f32; 3]; 8] = [
        [1.0, 0.62, 0.2],
        [0.55, 0.85, 0.3],
        [0.8, 0.45, 1.0],
        [1.0, 0.85, 0.25],
        [0.3, 0.85, 0.85],
        [1.0, 0.45, 0.65],
        [0.55, 0.65, 1.0],
        [0.9, 0.9, 0.9],
    ];
    // FNV-1a: stable, unlike std's randomly keyed hasher.
    let h = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    let c = PALETTE[(h % PALETTE.len() as u64) as usize];
    [c[0], c[1], c[2], 1.0]
}

pub fn hex(c: [f32; 4]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

fn with_alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

// ---------------------------------------------------------------------------------------------
// Painting

/// A label waiting for placement (higher `priority` first).
#[derive(Clone, Debug, PartialEq)]
pub struct LabelReq {
    pub text: String,
    /// Where it points at, in pixels.
    pub anchor: [f32; 2],
    pub color: [f32; 4],
    pub priority: u64,
    /// Grid labels have their own budget.
    pub grid: bool,
}

/// Draws lines, polygons and dots into overlay records for one view.
pub struct Painter {
    vp: Mat4,
    w: f32,
    h: f32,
    pub quads: Vec<OverlayQuad>,
    pub labels: Vec<LabelReq>,
    /// Records not added because [`MAX_QUADS`] was reached.
    pub dropped: usize,
}

/// Distance from the near plane in clip space (`w - z` for the reversed-Z infinite projection,
/// whose clip `z` is the constant near distance): ≥ 0 in front.
fn front(p: Vec4) -> f32 {
    p.w - p.z
}

fn lerp4(a: Vec4, b: Vec4, t: f32) -> Vec4 {
    Vec4::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t, a.w + (b.w - a.w) * t)
}

impl Painter {
    pub fn new(vp: Mat4, width: u32, height: u32) -> Painter {
        Painter { vp, w: width as f32, h: height as f32, quads: Vec::new(), labels: Vec::new(), dropped: 0 }
    }

    pub fn size(&self) -> [f32; 2] {
        [self.w, self.h]
    }

    fn to_screen(&self, p: Vec4) -> [f32; 2] {
        [(p.x / p.w + 1.0) * 0.5 * self.w, (p.y / p.w + 1.0) * 0.5 * self.h]
    }

    /// A world point in pixels, `None` behind the near plane.
    pub fn project(&self, p: Vec3) -> Option<[f32; 2]> {
        let c = self.vp.mul_vec4(p.extend(1.0));
        (front(c) >= 0.0 && c.w >= NEAR * 0.5).then(|| self.to_screen(c))
    }

    fn push(&mut self, q: OverlayQuad) {
        if self.quads.len() >= MAX_QUADS {
            self.dropped += 1;
        } else {
            self.quads.push(q);
        }
    }

    fn tri(&mut self, a: [f32; 2], b: [f32; 2], c: [f32; 2], ca: [f32; 4], cb: [f32; 4], cc: [f32; 4]) {
        self.push(OverlayQuad::triangle([a, b, c], [ca, cb, cc]));
    }

    /// The world segment `a`-`b`, clipped at the near plane: its pixel end points.
    pub fn clip_segment(&self, a: Vec3, b: Vec3) -> Option<([f32; 2], [f32; 2])> {
        let (mut ca, mut cb) = (self.vp.mul_vec4(a.extend(1.0)), self.vp.mul_vec4(b.extend(1.0)));
        let (da, db) = (front(ca), front(cb));
        if da < 0.0 && db < 0.0 {
            return None;
        }
        if da < 0.0 {
            ca = lerp4(ca, cb, da / (da - db));
        } else if db < 0.0 {
            cb = lerp4(cb, ca, db / (db - da));
        }
        if ca.w <= 0.0 || cb.w <= 0.0 {
            return None;
        }
        Some((self.to_screen(ca), self.to_screen(cb)))
    }

    /// A world-space line `width` pixels wide. Returns whether any of it is in front.
    pub fn line(&mut self, a: Vec3, b: Vec3, color: [f32; 4], width: f32) -> bool {
        match self.clip_segment(a, b) {
            Some((pa, pb)) => {
                self.line2d(pa, pb, color, width);
                true
            }
            None => false,
        }
    }

    /// A pixel-space line with an anti-aliasing fringe, clipped to a margin around the image.
    pub fn line2d(&mut self, a: [f32; 2], b: [f32; 2], color: [f32; 4], width: f32) {
        let Some((a, b)) = clip_to_rect(a, b, [-8.0, -8.0, self.w + 8.0, self.h + 8.0]) else { return };
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if !(len.is_finite() && len > 1e-3) {
            return;
        }
        let (ux, uy) = (dx / len, dy / len);
        // Half a pixel past each end, so joints meet.
        let (a, b) = ([a[0] - ux * 0.5, a[1] - uy * 0.5], [b[0] + ux * 0.5, b[1] + uy * 0.5]);
        let (nx, ny) = (-uy, ux);
        let half = (width * 0.5).max(0.25);
        let off = |p: [f32; 2], s: f32| [p[0] + nx * s, p[1] + ny * s];
        let clear = with_alpha(color, 0.0);
        let (a0, a1, b0, b1) = (off(a, -half), off(a, half), off(b, -half), off(b, half));
        self.tri(a0, a1, b1, color, color, color);
        self.tri(a0, b1, b0, color, color, color);
        // 1 px fringe on both sides, fading out.
        for s in [-1.0f32, 1.0] {
            let (ia, ib) = (off(a, s * half), off(b, s * half));
            let (oa, ob) = (off(a, s * (half + 1.0)), off(b, s * (half + 1.0)));
            self.tri(ia, oa, ob, color, clear, clear);
            self.tri(ia, ob, ib, color, clear, color);
        }
    }

    /// A filled convex world polygon, clipped at the near plane and to a guard band twice the
    /// image's size (so a face reaching past the camera stays a small, finite polygon).
    pub fn polygon(&mut self, points: &[Vec3], color: [f32; 4]) {
        const GUARD: f32 = 2.0;
        let mut poly: Vec<Vec4> = points.iter().map(|p| self.vp.mul_vec4(p.extend(1.0))).collect();
        let planes: [fn(Vec4) -> f32; 5] = [
            front,
            |p| p.x + GUARD * p.w,
            |p| GUARD * p.w - p.x,
            |p| p.y + GUARD * p.w,
            |p| GUARD * p.w - p.y,
        ];
        for plane in planes {
            let mut kept: Vec<Vec4> = Vec::with_capacity(poly.len() + 2);
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                let (da, db) = (plane(a), plane(b));
                if da >= 0.0 {
                    kept.push(a);
                }
                if (da < 0.0) != (db < 0.0) {
                    kept.push(lerp4(a, b, da / (da - db)));
                }
            }
            poly = kept;
            if poly.len() < 3 {
                return;
            }
        }
        if poly.iter().any(|p| p.w <= 0.0) {
            return;
        }
        let px: Vec<[f32; 2]> = poly.iter().map(|p| self.to_screen(*p)).collect();
        if px.iter().any(|p| !(p[0].is_finite() && p[1].is_finite())) {
            return;
        }
        for i in 1..px.len() - 1 {
            self.tri(px[0], px[i], px[i + 1], color, color, color);
        }
    }

    /// A round dot of `radius` pixels at a world point.
    pub fn dot(&mut self, p: Vec3, radius: f32, color: [f32; 4]) {
        if let Some(s) = self.project(p) {
            let d = radius * 2.0;
            self.push(OverlayQuad { radius, ..OverlayQuad::solid([s[0] - radius, s[1] - radius, d, d], color) });
        }
    }

    /// The 12 edges of a box given as 8 corners indexed by bits (x = 1, y = 2, z = 4).
    pub fn box_edges(&mut self, c: &[Vec3; 8], color: [f32; 4], width: f32) {
        for i in 0..8 {
            for bit in [1, 2, 4] {
                if i & bit == 0 {
                    self.line(c[i], c[i | bit], color, width);
                }
            }
        }
    }

    /// The 6 faces of such a box, filled.
    pub fn box_faces(&mut self, c: &[Vec3; 8], color: [f32; 4]) {
        for face in [[0, 1, 3, 2], [4, 5, 7, 6], [0, 1, 5, 4], [2, 3, 7, 6], [0, 2, 6, 4], [1, 3, 7, 5]] {
            self.polygon(&face.map(|i| c[i]), color);
        }
    }

    pub fn label(&mut self, text: &str, anchor: [f32; 2], color: [f32; 4], priority: u64) {
        let text: String = text.chars().take(MAX_LABEL_CHARS).collect();
        self.labels.push(LabelReq { text, anchor, color, priority, grid: false });
    }
}

/// Clips a 2D segment to `[x0, y0, x1, y1]` (Liang–Barsky).
pub fn clip_to_rect(a: [f32; 2], b: [f32; 2], r: [f32; 4]) -> Option<([f32; 2], [f32; 2])> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [(-dx, a[0] - r[0]), (dx, r[2] - a[0]), (-dy, a[1] - r[1]), (dy, r[3] - a[1])] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    if t0 > t1 || t0.is_nan() || t1.is_nan() {
        return None;
    }
    Some(([a[0] + dx * t0, a[1] + dy * t0], [a[0] + dx * t1, a[1] + dy * t1]))
}

// ---------------------------------------------------------------------------------------------
// Grid and axes

/// A "nice" grid step (1, 2 or 5 × 10^k metres) for a view about `distance` metres away.
pub fn grid_step(distance: f32) -> f32 {
    let raw = (distance / 12.0).max(0.1);
    let k = raw.log10().floor();
    let base = 10f32.powf(k);
    let m = raw / base;
    let nice = if m <= 1.0 { 1.0 } else if m <= 2.0 { 2.0 } else if m <= 5.0 { 5.0 } else { 10.0 };
    nice * base
}

/// Where the view centre meets the ground (y = 0), or the target's footprint.
pub fn ground_focus(eye: Vec3, target: Vec3) -> Vec3 {
    let d = target - eye;
    if d.y < -1e-4 {
        let t = -eye.y / d.y;
        if t > 0.0 && t.is_finite() {
            let p = eye + d * t;
            // Grazing views put the hit very far away: keep it near the target.
            if (p - target).length() < 4.0 * d.length() {
                return Vec3::new(p.x, 0.0, p.z);
            }
        }
    }
    Vec3::new(target.x, 0.0, target.z)
}

/// Lines per side of the grid's centre.
const GRID_HALF_LINES: i32 = 20;

/// Draws the ground grid around `focus` and returns `(step_m, lines drawn)`.
pub fn grid(p: &mut Painter, eye: Vec3, focus: Vec3) -> (f32, usize) {
    let step = grid_step((eye - focus).length());
    let (cx, cz) = ((focus.x / step).round() as i64, (focus.z / step).round() as i64);
    let n = i64::from(GRID_HALF_LINES);
    let extent = n as f32 * step;
    let (x0, x1) = (cx as f32 * step - extent, cx as f32 * step + extent);
    let (z0, z1) = (cz as f32 * step - extent, cz as f32 * step + extent);
    let mut drawn = 0;
    let fmt = |v: f32| {
        if step >= 1.0 {
            format!("{} m", v.round() as i64)
        } else {
            format!("{v:.1} m")
        }
    };
    for i in -n..=n {
        let (ix, iz) = (cx + i, cz + i);
        let (gx, gz) = (ix as f32 * step, iz as f32 * step);
        let major = |k: i64| k % 5 == 0;
        // Lines of constant x run along z, and the other way round.
        for (along_x, k, v) in [(false, ix, gx), (true, iz, gz)] {
            let (a, b) = if along_x { (Vec3::new(x0, 0.0, v), Vec3::new(x1, 0.0, v)) } else { (Vec3::new(v, 0.0, z0), Vec3::new(v, 0.0, z1)) };
            let axis = k == 0;
            let color = match (axis, along_x) {
                (true, true) => with_alpha(AXIS_X, 0.7),
                (true, false) => with_alpha(AXIS_Z, 0.7),
                _ if major(k) => GRID_MAJOR,
                _ => GRID_MINOR,
            };
            let width = if axis || major(k) { 1.5 } else { 1.0 };
            if p.line(a, b, color, width) {
                drawn += 1;
                if major(k) {
                    // Label where the line crosses the grid's centre line.
                    let at = if along_x { Vec3::new(cx as f32 * step, 0.0, v) } else { Vec3::new(v, 0.0, cz as f32 * step) };
                    // Only where that crossing is in the image (a label at the edge for a line
                    // elsewhere would mislead).
                    let [w, h] = p.size();
                    if let Some(s) = p.project(at).filter(|s| s[0] >= 0.0 && s[1] >= 0.0 && s[0] <= w && s[1] <= h) {
                        let name = if along_x { "z" } else { "x" };
                        let pri = 1000 - (i.unsigned_abs().min(999));
                        p.labels.push(LabelReq { text: format!("{name} {}", fmt(v)), anchor: s, color: GRID_MAJOR, priority: pri, grid: true });
                    }
                }
            }
        }
    }
    (step, drawn)
}

/// World axes from `origin` along the given unit axes, `length` metres, with X/Y/Z tip labels
/// when `tips`.
pub fn axes(p: &mut Painter, origin: Vec3, axes: [Vec3; 3], length: f32, tips: bool) {
    for (axis, color, name) in [(axes[0], AXIS_X, "X"), (axes[1], AXIS_Y, "Y"), (axes[2], AXIS_Z, "Z")] {
        let tip = origin + axis * length;
        if p.line(origin, tip, color, 2.0) && tips {
            if let Some(s) = p.project(tip) {
                p.labels.push(LabelReq { text: name.into(), anchor: s, color, priority: u64::MAX / 2, grid: false });
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Labels

/// A placed label: background box and text origin, both in pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub text: String,
    pub rect: [f32; 4],
    pub text_origin: [f32; 2],
    pub color: [f32; 4],
}

const PAD: [f32; 2] = [4.0, 2.0];

/// Places labels greedily by priority, without overlaps: centred on the anchor, else just above,
/// below, right or left of it; kept inside the image. `measure` gives a text's pixel size.
/// Entity labels stop at `max`, grid labels at their own budget. Returns `(placed, skipped)`.
pub fn place_labels(mut labels: Vec<LabelReq>, size: [f32; 2], max: usize, measure: &mut dyn FnMut(&str) -> [f32; 2]) -> (Vec<Placed>, usize) {
    labels.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.text.cmp(&b.text)));
    let mut placed: Vec<Placed> = Vec::new();
    let (mut entity_n, mut grid_n, mut skipped) = (0, 0, 0);
    for l in labels {
        let budget_left = if l.grid { grid_n < GRID_LABELS } else { entity_n < max };
        if !budget_left || l.text.is_empty() {
            skipped += 1;
            continue;
        }
        let t = measure(&l.text);
        let (bw, bh) = (t[0] + PAD[0] * 2.0, t[1] + PAD[1] * 2.0);
        let [ax, ay] = l.anchor;
        // Centred, then just clear of the centred spot above, below, right and left.
        let candidates = [
            [ax - bw * 0.5, ay - bh * 0.5],
            [ax - bw * 0.5, ay - bh * 1.5 - 2.0],
            [ax - bw * 0.5, ay + bh * 0.5 + 2.0],
            [ax + bw * 0.5 + 2.0, ay - bh * 0.5],
            [ax - bw * 1.5 - 2.0, ay - bh * 0.5],
        ];
        let fits = |r: [f32; 4]| placed.iter().all(|p| !overlaps(p.rect, r));
        let spot = candidates.iter().map(|c| [c[0].clamp(0.0, (size[0] - bw).max(0.0)), c[1].clamp(0.0, (size[1] - bh).max(0.0)), bw, bh]).find(|r| fits(*r));
        match spot {
            Some(rect) => {
                if l.grid {
                    grid_n += 1;
                } else {
                    entity_n += 1;
                }
                placed.push(Placed { text: l.text, text_origin: [rect[0] + PAD[0], rect[1] + PAD[1]], rect, color: l.color });
            }
            None => skipped += 1,
        }
    }
    (placed, skipped)
}

fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

/// The font and glyph atlas labels are drawn with, and their GPU pages. Loaded on first use; a
/// machine without a usable system font gets no labels (and is told so).
#[derive(Default)]
pub struct Labeler {
    /// `None` until first used; then `Some(None)` when there is no font.
    font: Option<Option<(PathBuf, Font)>>,
    atlas: Option<GlyphAtlas>,
    pages: Vec<TextureId>,
    scratch: Vec<u8>,
}

impl Labeler {
    /// The font's file, loading it on first call (`None`: no font on this machine).
    pub fn font_path(&mut self) -> Option<&PathBuf> {
        self.font.get_or_insert_with(specula_text::find_system_ui_font).as_ref().map(|(p, _)| p)
    }

    /// The pixel size of `text` (an estimate without a font).
    pub fn measure(&mut self, text: &str) -> [f32; 2] {
        match self.font.get_or_insert_with(specula_text::find_system_ui_font) {
            Some((_, font)) => {
                let m = specula_text::measure(font, text, &TextStyle::new(TEXT_PX));
                [m.width.ceil(), m.height.ceil()]
            }
            None => [text.chars().count() as f32 * TEXT_PX * 0.55, TEXT_PX * 1.2],
        }
    }

    /// Appends label backgrounds and glyphs for `placed` to `out`, uploading new glyphs. Returns
    /// whether text was drawn (false without a font: only the backgrounds are skipped too).
    pub fn draw(&mut self, gpu: &mut Gpu, placed: &[Placed], out: &mut Vec<OverlayQuad>) -> Result<bool, String> {
        let Some((_, font)) = self.font.get_or_insert_with(specula_text::find_system_ui_font) else { return Ok(false) };
        let atlas = self.atlas.get_or_insert_with(|| GlyphAtlas::new(AtlasConfig { page_size: 512, max_pages: 2, padding: 1, evict: true }));
        atlas.begin_frame();
        let mut glyphs = Vec::new();
        let mut spans = Vec::with_capacity(placed.len());
        for p in placed {
            let start = glyphs.len();
            let layout = specula_text::layout(font, &p.text, &TextStyle::new(TEXT_PX));
            layout.append_quads(font, atlas, p.text_origin, p.color, &mut glyphs).map_err(|e| format!("label glyphs: {e}"))?;
            spans.push(start..glyphs.len());
        }
        // Upload pages that are new, then the regions that changed.
        let size = atlas.page_size();
        let fresh = self.pages.len();
        while self.pages.len() < atlas.page_count() {
            let pixels = atlas.page_pixels(self.pages.len()).unwrap_or(&[]);
            let id = gpu.upload_texture(size, size, TextureFormat::R8, pixels).map_err(|e| format!("label atlas: {e}"))?;
            self.pages.push(id);
        }
        for r in atlas.take_dirty() {
            let page = r.page as usize;
            if page >= fresh || r.width == 0 || r.height == 0 {
                continue;
            }
            atlas.copy_rect(&r, &mut self.scratch);
            gpu.update_texture(self.pages[page], r.x, r.y, r.width, r.height, &self.scratch).map_err(|e| format!("label atlas: {e}"))?;
        }
        for (p, span) in placed.iter().zip(spans) {
            out.push(OverlayQuad { radius: 3.0, border: 1.0, border_color: [with_alpha(p.color, 0.6); 2], ..OverlayQuad::solid(p.rect, LABEL_BG) });
            for g in &glyphs[span] {
                let Some(&tex) = self.pages.get(g.page as usize) else { continue };
                let rect = [g.rect[0], g.rect[1], g.rect[2] - g.rect[0], g.rect[3] - g.rect[1]];
                out.push(OverlayQuad::textured(rect, g.uv, tex, OverlayKind::AlphaMask, g.color));
            }
        }
        Ok(true)
    }
}

/// What the packet says about the overlays drawn.
#[derive(Default)]
pub struct Report {
    pub drawn: Vec<&'static str>,
    pub details: Vec<(&'static str, Json)>,
}

impl Report {
    pub fn to_json(&self) -> Json {
        let mut fields = vec![("drawn", Json::arr(self.drawn.iter().map(|d| Json::from(*d))))];
        fields.extend(self.details.iter().cloned());
        Json::obj(fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sight::Camera;

    fn painter() -> (Painter, Camera) {
        let cam = Camera { eye: Vec3::new(0.0, 2.0, 10.0), target: Vec3::new(0.0, 0.0, 0.0), fov_y_deg: 50.0 };
        (Painter::new(cam.view_proj(800, 600), 800, 600), cam)
    }

    #[test]
    fn projected_lines_land_where_points_project() {
        let (mut p, cam) = painter();
        let vp = cam.view_proj(800, 600);
        let (a, b) = (Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 1.0, -2.0));
        let (pa, pb) = p.clip_segment(a, b).unwrap();
        for (world, px) in [(a, pa), (b, pb)] {
            let (x, y, _) = specula_core::math::project_to_screen(&vp, world, 800, 600).unwrap();
            assert!((x - px[0]).abs() < 1e-3 && (y - px[1]).abs() < 1e-3, "{world:?}: {px:?} vs {x},{y}");
        }
        assert!(p.line(a, b, WHITE, 2.0));
        assert_eq!(p.quads.len(), 6, "a core quad and two fringe strips: 6 triangles");
        // Every vertex of the line is within width/2 + fringe of the segment.
        for q in &p.quads {
            for v in q.triangle.unwrap().points {
                let d = dist_to_segment(v, pa, pb);
                assert!(d <= 2.0 + 0.01, "{v:?} is {d} px from the line");
            }
        }
    }

    fn dist_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        let (x, y) = (a[0] + dx * t - p[0], a[1] + dy * t - p[1]);
        // The half-pixel extension at the ends counts as on the line.
        (x * x + y * y).sqrt() - if t == 0.0 || t == 1.0 { 0.5 } else { 0.0 }
    }

    #[test]
    fn lines_behind_the_camera_are_clipped_at_the_near_plane() {
        let (mut p, _) = painter();
        // Entirely behind: nothing.
        assert!(p.clip_segment(Vec3::new(0.0, 0.0, 20.0), Vec3::new(1.0, 0.0, 30.0)).is_none());
        assert!(!p.line(Vec3::new(0.0, 0.0, 20.0), Vec3::new(1.0, 0.0, 30.0), WHITE, 1.0));
        // Crossing the camera plane: the visible half starts at the near plane, in the image's
        // lower half (the ground passes under the camera).
        let (a, b) = p.clip_segment(Vec3::new(0.0, 0.0, 30.0), Vec3::new(0.0, 0.0, -30.0)).unwrap();
        // (The far end, 40 m out and 2 m below the eye, is above the centre of this downward view.)
        assert!(a[1] > 600.0 && b[1] < 300.0 && b[1] > 150.0, "{a:?} {b:?}");
        assert!((a[0] - 400.0).abs() < 1e-2 && (b[0] - 400.0).abs() < 1e-2);
    }

    #[test]
    fn polygons_crossing_the_near_plane_are_cut() {
        let (mut p, _) = painter();
        let quad = [Vec3::new(-5.0, 0.0, -5.0), Vec3::new(5.0, 0.0, -5.0), Vec3::new(5.0, 0.0, 30.0), Vec3::new(-5.0, 0.0, 30.0)];
        p.polygon(&quad, WHITE);
        assert!(!p.quads.is_empty());
        for q in &p.quads {
            for v in q.triangle.unwrap().points {
                // Inside the guard band (twice the image about its centre), below the horizon.
                assert!(v[0] >= -400.0 - 1e-2 && v[0] <= 1200.0 + 1e-2 && v[1] <= 900.0 + 1e-2, "{v:?}");
                assert!(v[1] > 200.0, "below the horizon: {v:?}");
            }
        }
    }

    #[test]
    fn clipping_to_a_rect() {
        let r = [0.0, 0.0, 100.0, 100.0];
        assert_eq!(clip_to_rect([-50.0, 50.0], [150.0, 50.0], r), Some(([0.0, 50.0], [100.0, 50.0])));
        assert_eq!(clip_to_rect([-50.0, -50.0], [-10.0, 150.0], r), None);
        assert_eq!(clip_to_rect([10.0, 10.0], [20.0, 20.0], r), Some(([10.0, 10.0], [20.0, 20.0])));
    }

    #[test]
    fn grid_steps_are_nice_and_scale_with_distance() {
        assert_eq!(grid_step(10.0), 1.0);
        assert_eq!(grid_step(20.0), 2.0);
        assert_eq!(grid_step(50.0), 5.0);
        assert_eq!(grid_step(100.0), 10.0);
        assert_eq!(grid_step(0.5), 0.1);
        let (mut p, cam) = painter();
        let focus = ground_focus(cam.eye, cam.target);
        assert!((focus - Vec3::ZERO).length() < 1e-4);
        let (step, lines) = grid(&mut p, cam.eye, focus);
        assert_eq!(step, 1.0);
        assert!(lines > 20, "{lines}");
        let texts: Vec<&str> = p.labels.iter().map(|l| l.text.as_str()).collect();
        assert!(texts.contains(&"x 0 m") && texts.contains(&"x 5 m") && texts.contains(&"z -5 m"), "{texts:?}");
        // Looking at the sky: the ground focus is the target's footprint.
        assert_eq!(ground_focus(Vec3::ZERO, Vec3::new(1.0, 5.0, 2.0)), Vec3::new(1.0, 0.0, 2.0));
    }

    #[test]
    fn labels_never_overlap_and_respect_their_budgets() {
        let mut measure = |t: &str| [t.len() as f32 * 7.0, 14.0];
        let req = |text: &str, x: f32, y: f32, priority: u64, grid: bool| LabelReq { text: text.into(), anchor: [x, y], color: WHITE, priority, grid };
        // Ten labels on the same spot: each finds its own place or is skipped.
        let labels: Vec<LabelReq> = (0..10).map(|i| req(&format!("entity_{i}"), 200.0, 150.0, i, false)).collect();
        let (placed, skipped) = place_labels(labels, [400.0, 300.0], 32, &mut measure);
        assert_eq!(placed.len() + skipped, 10);
        assert!(placed.len() >= 5, "five spots around the anchor: {}", placed.len());
        assert_eq!(placed[0].text, "entity_9", "highest priority first");
        for (i, a) in placed.iter().enumerate() {
            assert!(a.rect[0] >= 0.0 && a.rect[1] >= 0.0 && a.rect[0] + a.rect[2] <= 400.0 && a.rect[1] + a.rect[3] <= 300.0);
            for b in &placed[i + 1..] {
                assert!(!overlaps(a.rect, b.rect), "{a:?} overlaps {b:?}");
            }
        }
        // Budgets: entity labels stop at max; grid labels have their own.
        let mut labels: Vec<LabelReq> = (0..20).map(|i| req("e", i as f32 * 30.0, 20.0, 5, false)).collect();
        labels.extend((0..30).map(|i| req("g", i as f32 * 30.0, 200.0, 1, true)));
        let (placed, skipped) = place_labels(labels, [1000.0, 300.0], 4, &mut measure);
        assert_eq!(placed.iter().filter(|p| p.text == "e").count(), 4);
        assert_eq!(placed.iter().filter(|p| p.text == "g").count(), GRID_LABELS);
        assert_eq!(skipped, 16 + 30 - GRID_LABELS);
        // Off-image anchors are pulled inside.
        let (placed, _) = place_labels(vec![req("edge", -50.0, 999.0, 1, false)], [400.0, 300.0], 4, &mut measure);
        assert_eq!(placed[0].rect[0], 0.0);
        assert_eq!(placed[0].rect[1] + placed[0].rect[3], 300.0);
    }

    #[test]
    fn spec_selects_subtrees() {
        let s = Spec { kinds: vec![Kind::Bounds], entities: vec!["/district".into()], max_labels: 8 };
        assert!(s.selects("/district") && s.selects("/district/house") && !s.selects("/district_2") && !s.selects("/other"));
        assert!(Spec::default().selects("/anything"));
        assert_eq!(Kind::parse("skeletons"), Some(Kind::Skeletons));
        assert_eq!(owner_color("ann"), owner_color("ann"));
        assert_eq!(hex([1.0, 0.0, 0.5, 1.0]), "#ff0080");
    }
}

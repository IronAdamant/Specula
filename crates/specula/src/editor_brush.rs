//! The editor's draw tools, as maths (docs/design/EDITOR.md, "Draw tools"): where the pointer's
//! ray meets a terrain, a model or a model's ground plane, the points a drag samples, and the
//! one protocol command a release sends. Nothing here touches the world: the engine hands in heights and triangles and
//! sends what comes out through the editor's own command path, so a stroke is journaled,
//! claim-checked and undone like any other edit.
//!
//! Every loop is bounded: a ray marches at most [`MAX_MARCH`] steps over a terrain, a model is
//! tested only when its mesh has at most [`MAX_HIT_TRIANGLES`] triangles (the caller passes its
//! coarsest LOD), and a stroke holds at most [`MAX_SAMPLES`] points (one tool call's worth).

use specula_core::Json;

/// Points one stroke samples: what one `terrain_edit` or `model_paint` call takes (and a
/// sketched path's points).
pub const MAX_SAMPLES: usize = 64;
/// How far from a model's origin a path may be sketched, metres (as a recipe's coordinates).
pub const PATH_REACH: f32 = 10_000.0;
/// The sweep parts the Path button steps through when the recipe isn't known yet.
pub const MAX_PATH_SWEEPS: u32 = 8;
/// Steps a ray takes across a terrain's footprint.
pub const MAX_MARCH: usize = 512;
/// Triangles a model's hit test reads (per frame, while a paint brush is on).
pub const MAX_HIT_TRIANGLES: usize = 20_000;
/// Distances [`paint_reaches`] works out at a stroke's release (vertices times dabs): about
/// a third of a millisecond.
pub const MAX_REACH_TESTS: usize = 200_000;
/// A brush's radius, metres.
pub const RADIUS: (f32, f32) = (0.05, 64.0);

pub type P3 = [f32; 3];

/// What a brush does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrushKind {
    Raise,
    Lower,
    Flatten,
    Smooth,
    /// Paint a terrain's splat layer (0..3).
    Splat(u32),
    /// Paint a model's vertex colours.
    Paint,
    /// Sketch the path of a model's sweep part (the n-th of its recipe's, from 0).
    Path(u32),
}

impl BrushKind {
    pub fn name(self) -> &'static str {
        match self {
            BrushKind::Raise => "raise",
            BrushKind::Lower => "lower",
            BrushKind::Flatten => "flatten",
            BrushKind::Smooth => "smooth",
            BrushKind::Splat(_) => "splat",
            BrushKind::Paint => "paint",
            BrushKind::Path(_) => "path",
        }
    }

    /// A terrain's brush (the others draw on a model).
    pub fn on_terrain(self) -> bool {
        !matches!(self, BrushKind::Paint | BrushKind::Path(_))
    }

    /// The path tool: it draws on the model's ground plane, and its stroke is a path.
    pub fn is_path(self) -> bool {
        matches!(self, BrushKind::Path(_))
    }

    /// The next sculpt op (the toolbar's Sculpt button cycles them).
    pub fn next_sculpt(self) -> BrushKind {
        match self {
            BrushKind::Raise => BrushKind::Lower,
            BrushKind::Lower => BrushKind::Flatten,
            BrushKind::Flatten => BrushKind::Smooth,
            _ => BrushKind::Raise,
        }
    }
}

/// The brush in hand.
#[derive(Clone, Debug, PartialEq)]
pub struct Brush {
    pub kind: BrushKind,
    /// Metres.
    pub radius: f32,
    /// Raise and lower: metres at the centre. The others: 0..1 (the path tool: its smoothing,
    /// [`Brush::path_smooth`]).
    pub amount: f32,
    /// Paint's colour: `#rrggbb`, or a colour of the style.
    pub color: String,
}

impl Default for Brush {
    fn default() -> Self {
        Brush { kind: BrushKind::Raise, radius: 2.0, amount: 0.5, color: "#c8a060".into() }
    }
}

/// The colours the Paint brush steps through (its button, pressed again): muted, so paint sits
/// with most styles; an agent's `model_paint` takes any colour of the style.
pub const PAINT_COLORS: [&str; 6] = ["#c8a060", "#6a8f4e", "#8a8f98", "#5a3e2b", "#d8d4c8", "#2f3640"];

impl Brush {
    /// The brush in words, for the Draw bar and `editor_state`.
    pub fn label(&self) -> String {
        let size = format!("{:.1} m wide", self.radius * 2.0);
        match self.kind {
            BrushKind::Raise | BrushKind::Lower => format!("{}: {size}, {:.1} m", self.kind.name(), self.amount),
            BrushKind::Flatten | BrushKind::Smooth => format!("{}: {size}, {}%", self.kind.name(), (self.amount * 100.0).round()),
            BrushKind::Splat(layer) => format!("splat layer {layer}: {size}, {}%", (self.amount * 100.0).round()),
            BrushKind::Paint => format!("paint {}: {size}, {}%", self.color, (self.amount * 100.0).round()),
            BrushKind::Path(k) => format!("path of sweep {}: a point every {:.2} m, smoothing {}", k + 1, self.radius * 0.5, self.path_smooth()),
        }
    }

    /// The path tool's `smooth` (rings between the sketched points: a spline through them),
    /// from the Strength control: 1 (straight segments) to 8.
    pub fn path_smooth(&self) -> u32 {
        ((self.amount * 8.0).round() as i64).clamp(1, 8) as u32
    }

    /// The colour after this one in [`PAINT_COLORS`] (the first, for a colour not in it).
    pub fn next_color(&self) -> &'static str {
        let at = PAINT_COLORS.iter().position(|c| *c == self.color).map_or(0, |i| (i + 1) % PAINT_COLORS.len());
        PAINT_COLORS[at]
    }

    /// The radius a step up or down gives (a quarter more or less, within [`RADIUS`]).
    pub fn stepped_radius(&self, up: bool) -> f32 {
        (self.radius * if up { 1.25 } else { 0.8 }).clamp(RADIUS.0, RADIUS.1)
    }

    /// The amount a step up or down gives (a tenth, within 0.1..1).
    pub fn stepped_amount(&self, up: bool) -> f32 {
        ((self.amount + if up { 0.1 } else { -0.1 }) * 10.0).round().clamp(1.0, 10.0) / 10.0
    }
}

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: P3, k: f32) -> P3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: P3, b: P3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Where a ray (in a terrain's own frame) first comes down onto its surface: `[x, height, z]`.
/// `half` is the footprint's half size (the terrain is centred on its origin), `step` the
/// distance between looks (about half a cell), and `height` its surface (`None` outside). A ray
/// that comes in through the terrain's side, under its surface there, meets it at its border
/// (the pointer is on the terrain's edge: found live, where a ridge raised beside the border hid
/// the ground behind it and the brush said there was nothing to draw on). A ray that starts
/// inside the footprint and under the surface, or misses the footprint, meets nothing.
pub fn terrain_hit(origin: P3, dir: P3, half: [f32; 2], step: f32, height: impl Fn(f32, f32) -> Option<f32>) -> Option<P3> {
    if step.is_nan() || step <= 0.0 || !origin.iter().chain(&dir).all(|v| v.is_finite()) {
        return None;
    }
    // A ray straight down (or up): it stays over one point of the footprint, so the slabs
    // below bound nothing. (Review R1 of m5-eddraw, m1: it met nothing, so in a top-down
    // orthographic view the terrain's brushes found nothing to draw on anywhere.)
    if dir[0].abs() < 1e-9 && dir[2].abs() < 1e-9 {
        if origin[0].abs() > half[0] - 1e-3 || origin[2].abs() > half[1] - 1e-3 {
            return None;
        }
        let h = height(origin[0], origin[2])?;
        // Ahead of the ray, and coming onto the surface from above it.
        return (dir[1] < 0.0 && origin[1] >= h).then_some([origin[0], h, origin[2]]);
    }
    // The stretch of the ray over the footprint (x and z slabs, a hair inside the border).
    let (mut t0, mut t1) = (0.0f32, f32::MAX);
    for (axis, h) in [(0usize, half[0]), (2, half[1])] {
        let h = h - 1e-3;
        if h <= 0.0 {
            return None;
        }
        if dir[axis].abs() < 1e-9 {
            if origin[axis].abs() > h {
                return None;
            }
            continue;
        }
        let (a, b) = ((-h - origin[axis]) / dir[axis], (h - origin[axis]) / dir[axis]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    if t1 == f32::MAX || t1 <= t0 {
        return None;
    }
    let steps = (((t1 - t0) / step).ceil() as usize).clamp(1, MAX_MARCH);
    let dt = (t1 - t0) / steps as f32;
    let at = |t: f32| add(origin, scale(dir, t));
    // Height of the ray over the surface (`None`: no surface there).
    let over = |t: f32| {
        let p = at(t);
        height(p[0], p[2]).map(|h| p[1] - h)
    };
    let mut prev = t0;
    if over(prev).is_some_and(|o| o < 0.0) {
        // Under the surface where the stretch begins: through the side (the border is what
        // the pointer is on), or from inside the ground.
        if t0 <= 0.0 {
            return None;
        }
        let p = at(t0);
        return height(p[0], p[2]).map(|h| [p[0], h, p[2]]);
    }
    for k in 1..=steps {
        let t = t0 + dt * k as f32;
        if over(t).is_some_and(|o| o <= 0.0) {
            // Between `prev` (over it) and `t` (under it): halve the stretch.
            let (mut lo, mut hi) = (prev, t);
            for _ in 0..16 {
                let mid = 0.5 * (lo + hi);
                if over(mid).is_some_and(|o| o <= 0.0) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let p = at(hi);
            return height(p[0], p[2]).map(|h| [p[0], h, p[2]]);
        }
        prev = t;
    }
    None
}

/// The nearest point where a ray (in a model's own frame) meets its triangles, either side.
/// `None` for a miss, and for a mesh past [`MAX_HIT_TRIANGLES`] (the caller picks a coarser LOD).
pub fn mesh_hit(origin: P3, dir: P3, positions: &[P3], indices: &[u32]) -> Option<P3> {
    if indices.len() / 3 > MAX_HIT_TRIANGLES || !origin.iter().chain(&dir).all(|v| v.is_finite()) {
        return None;
    }
    let mut best: Option<f32> = None;
    for tri in indices.chunks_exact(3) {
        let (Some(a), Some(b), Some(c)) = (positions.get(tri[0] as usize), positions.get(tri[1] as usize), positions.get(tri[2] as usize)) else { continue };
        let (e1, e2) = (sub(*b, *a), sub(*c, *a));
        let p = cross(dir, e2);
        let det = dot(e1, p);
        if det.abs() < 1e-12 {
            continue;
        }
        let inv = 1.0 / det;
        let s = sub(origin, *a);
        let u = dot(s, p) * inv;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = cross(s, e1);
        let v = dot(dir, q) * inv;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let t = dot(e2, q) * inv;
        if t > 1e-5 && best.is_none_or(|b| t < b) {
            best = Some(t);
        }
    }
    best.map(|t| add(origin, scale(dir, t)))
}

/// Where a ray (in a model's own frame) meets the model's ground plane (y = 0): `[x, 0, z]`.
/// The path tool draws there: where a rail, a pipe or a road runs, seen from above; the path
/// keeps its height. `None` for a ray along the plane or away from it, and for a point farther
/// than `reach` from the origin.
pub fn plane_hit(origin: P3, dir: P3, reach: f32) -> Option<P3> {
    if dir[1].abs() < 1e-6 {
        return None;
    }
    let t = -origin[1] / dir[1];
    if !t.is_finite() || t <= 0.0 {
        return None;
    }
    let p = [origin[0] + dir[0] * t, 0.0, origin[2] + dir[2] * t];
    (p[0].is_finite() && p[2].is_finite() && p[0].abs() <= reach && p[2].abs() <= reach).then_some(p)
}

/// Whether a paint stroke would colour anything: a vertex of the model lies within `radius` of
/// one of its dabs. Paint colours vertices, so a brush in the middle of a large flat face (a
/// box has vertices only at its corners) reaches none: the strokes would be written into the
/// recipe and nothing would change (found by looking: a "painted" crate looked as before).
/// `None`: not worked out (the mesh is too dense to test here: [`MAX_REACH_TESTS`]); a mesh
/// that dense has vertices wherever a brush can be. Otherwise it stops at the first vertex in
/// reach. (Review R1 of m5-eddraw, m4: unbounded, the worst case took 5 ms of the engine
/// thread: 60,000 vertices against 64 dabs that reach nothing.)
pub fn paint_reaches(positions: &[P3], points: &[P3], radius: f32) -> Option<bool> {
    if positions.len().saturating_mul(points.len()) > MAX_REACH_TESTS {
        return None;
    }
    let r2 = radius * radius;
    Some(positions.iter().any(|v| {
        points.iter().any(|p| {
            let d = sub(*v, *p);
            dot(d, d) <= r2
        })
    }))
}

/// A brush as its stroke's frame measures it. The Draw bar's sizes are world metres (what the
/// ring shows); a stroke is written in the entity's own units, and an entity may be scaled.
/// `across`: world metres per unit along the surface; `up`: per unit of height (a sculpt's
/// raise and lower are metres of height). (Review R1 of m5-eddraw, M4: on an entity scaled 3x
/// a "1 m" brush painted 3 m of the world, and its dabs were three times as far apart as the
/// ring said.)
pub fn in_frame(brush: &Brush, across: f32, up: f32) -> Brush {
    let ok = |s: f32| if s.is_finite() && s > 1e-6 { s } else { 1.0 };
    let amount = match brush.kind {
        BrushKind::Raise | BrushKind::Lower => brush.amount / ok(up),
        _ => brush.amount,
    };
    Brush { radius: brush.radius / ok(across), amount, ..brush.clone() }
}

/// Why a brush can't draw on an entity of this scale, in the Draw bar's own terms (world
/// metres): `local` is `world` as the entity's frame measures it ([`in_frame`]), `across` the
/// frame's scale. The tools take a radius of 0.005..64 of the entity's units and keep a
/// stroke's numbers to a thousandth; a brush outside that would be refused by the tool in the
/// entity's units, or (a sculpt's height) silently made stronger. `None`: it fits.
/// (Review R2 of m5-eddraw, N4.)
pub fn out_of_range(world: &Brush, local: &Brush, across: f32) -> Option<String> {
    const LOCAL: (f32, f32) = (0.005, 64.0);
    let s = if across.is_finite() && across > 1e-6 { across } else { 1.0 };
    /// A length with two digits that mean something, whatever its size (0.0001 is not "0.000").
    fn m(v: f32) -> String {
        let places = if v >= 10.0 {
            0
        } else if v >= 1.0 {
            1
        } else if v >= 0.1 {
            2
        } else if v >= 0.01 {
            3
        } else if v >= 0.001 {
            4
        } else {
            6
        };
        format!("{v:.places$}")
    }
    if local.radius < LOCAL.0 || local.radius > LOCAL.1 {
        return Some(format!(
            "This entity is scaled {}x: a brush {} m wide is {} of its own units across, and a stroke takes {}..{}. Here a brush can be {}..{} m wide.",
            m(s),
            m(world.radius * 2.0),
            m(local.radius * 2.0),
            m(LOCAL.0 * 2.0),
            m(LOCAL.1 * 2.0),
            m(LOCAL.0 * 2.0 * s),
            m(LOCAL.1 * 2.0 * s)
        ));
    }
    // A sculpt's height: a stroke keeps a thousandth and takes at most 100 of the terrain's own
    // units (review R3, F5: over 100 it was clamped, a weaker stroke than the bar said).
    if matches!(world.kind, BrushKind::Raise | BrushKind::Lower) && !(0.001..=100.0).contains(&local.amount) {
        let (how, fix) = if local.amount < 0.001 { ("less than a stroke keeps (0.001)", "stronger") } else { ("more than a stroke takes (100)", "weaker") };
        return Some(format!("This terrain is scaled in height: a {} of {} m is {} of its own units, {how}. Use a {fix} brush.", world.kind.name(), m(world.amount), m(local.amount)));
    }
    None
}

/// The points a drag has sampled (the frame the tool's command will name them in).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stroke {
    pub points: Vec<P3>,
}

impl Stroke {
    /// Takes `p` when it is the first, or at least half a radius from the last one taken, and
    /// the stroke has room. Whether it was taken.
    pub fn add(&mut self, p: P3, radius: f32) -> bool {
        if self.points.len() >= MAX_SAMPLES || !p.iter().all(|v| v.is_finite()) {
            return false;
        }
        if let Some(last) = self.points.last() {
            let d = sub(p, *last);
            if dot(d, d) < 0.25 * radius * radius {
                return false;
            }
        }
        self.points.push(p);
        true
    }

    pub fn full(&self) -> bool {
        self.points.len() >= MAX_SAMPLES
    }

    /// Ends a sketched path at `p`, where the pointer was let go: a path ends where the hand
    /// stopped, not at its last sample (found by looking: with points 3 m apart the rail
    /// stopped up to 3 m short). `p` is added; when it is nearer the last sample than a
    /// quarter of `radius` (half the distance between samples), or the stroke is full, the
    /// last sample moves to it instead, so the path's end is never a short stub. Nothing
    /// changes for a `p` within a centimetre of the last point, or of the one before it when
    /// the last would move. Whether the stroke changed.
    pub fn end_at(&mut self, p: P3, radius: f32) -> bool {
        const CLEAR: f32 = 1e-4; // a centimetre, squared
        let d2 = |a: P3, b: P3| {
            let d = sub(a, b);
            dot(d, d)
        };
        let n = self.points.len();
        let Some(&last) = self.points.last() else { return false };
        if !p.iter().all(|v| v.is_finite()) || d2(p, last) < CLEAR {
            return false;
        }
        let near = d2(p, last) < 0.0625 * radius * radius;
        if n >= 2 && (near || n >= MAX_SAMPLES) {
            if d2(p, self.points[n - 2]) < CLEAR {
                return false;
            }
            self.points[n - 1] = p;
        } else if n < MAX_SAMPLES {
            self.points.push(p);
        } else {
            return false;
        }
        true
    }
}

fn round3(v: f32) -> Json {
    Json::Num((f64::from(v) * 1000.0).round() / 1000.0)
}

/// `terrain_edit`'s arguments for a stroke over a terrain (`points` in its own frame; `offset`
/// is the terrain's, which its strokes' `at` includes). `collider`: the terrain entity's path,
/// so its heightfield follows the sculpt in the same journal entry. `None`: nothing to send (no
/// points, or a brush that isn't a terrain's).
pub fn terrain_edit_args(recipe: &str, collider: Option<&str>, brush: &Brush, points: &[P3], offset: [f32; 2]) -> Option<Json> {
    if points.is_empty() || !brush.kind.on_terrain() {
        return None;
    }
    let (op, amount) = match brush.kind {
        BrushKind::Raise => ("raise", brush.amount.clamp(0.001, 100.0)),
        BrushKind::Lower => ("lower", brush.amount.clamp(0.001, 100.0)),
        BrushKind::Flatten => ("flatten", brush.amount.clamp(0.0, 1.0)),
        BrushKind::Smooth => ("smooth", brush.amount.clamp(0.0, 1.0)),
        BrushKind::Splat(_) => ("paint", brush.amount.clamp(0.0, 1.0)),
        BrushKind::Paint | BrushKind::Path(_) => return None,
    };
    let strokes = points.iter().take(MAX_SAMPLES).map(|p| {
        let mut f = vec![("op", Json::from(op)), ("at", Json::arr([round3(p[0] + offset[0]), round3(p[2] + offset[1])])), ("radius", round3(brush.radius)), ("amount", round3(amount))];
        match brush.kind {
            // Flatten to where the stroke began.
            BrushKind::Flatten => f.push(("height", round3(points[0][1]))),
            BrushKind::Splat(layer) => f.push(("layer", Json::from(layer.min(3)))),
            _ => {}
        }
        Json::obj(f)
    });
    let mut args = vec![("recipe", Json::from(recipe)), ("strokes", Json::arr(strokes))];
    if let Some(c) = collider {
        // Sculpting moves the surface; a splat only recolours it.
        if !matches!(brush.kind, BrushKind::Splat(_)) {
            args.push(("collider", Json::from(c)));
        }
    }
    Some(Json::obj(args))
}

/// `model_paint`'s arguments for a stroke over a model (`points` in the model's own frame).
pub fn model_paint_args(recipe: &str, brush: &Brush, points: &[P3]) -> Option<Json> {
    if points.is_empty() || brush.kind != BrushKind::Paint {
        return None;
    }
    let strokes = points.iter().take(MAX_SAMPLES).map(|p| {
        Json::obj([
            ("at", Json::arr([round3(p[0]), round3(p[1]), round3(p[2])])),
            ("radius", round3(brush.radius)),
            ("color", Json::from(brush.color.as_str())),
            ("amount", round3(brush.amount.clamp(0.0, 1.0))),
        ])
    });
    Some(Json::obj([("recipe", Json::from(recipe)), ("strokes", Json::arr(strokes))]))
}

/// `model_path`'s arguments for a path sketched on a model's ground plane (`points` in the
/// model's own frame; their `[x, z]` is sent, so the path keeps its height). `None`: nothing to
/// send (a click is no path: two points at least; or a brush that isn't the path tool).
pub fn model_path_args(recipe: &str, brush: &Brush, points: &[P3]) -> Option<Json> {
    let BrushKind::Path(sweep) = brush.kind else { return None };
    if points.len() < 2 {
        return None;
    }
    let rows = points.iter().take(MAX_SAMPLES).map(|p| Json::arr([round3(p[0]), round3(p[2])]));
    Some(Json::obj([("recipe", Json::from(recipe)), ("sweep", Json::from(sweep)), ("points", Json::arr(rows)), ("smooth", Json::from(brush.path_smooth()))]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ray from the camera comes down on the surface where it first crosses it: a flat
    /// field, a slope, and a bump that hides the ground behind it.
    #[test]
    fn a_ray_meets_a_terrain_where_it_first_comes_down_on_it() {
        let half = [50.0, 50.0];
        let flat = |_: f32, _: f32| Some(2.0);
        let down = [0.0, -0.8, 0.6];
        let hit = terrain_hit([0.0, 10.0, -6.0], down, half, 0.5, flat).unwrap();
        assert!((hit[1] - 2.0).abs() < 1e-3 && (hit[0]).abs() < 1e-3 && (hit[2] - 0.0).abs() < 0.01, "{hit:?}");
        // A slope rising with z: the hit is on it.
        let slope = |_: f32, z: f32| Some(0.5 * z);
        let hit = terrain_hit([0.0, 10.0, -6.0], down, half, 0.5, slope).unwrap();
        assert!((hit[1] - 0.5 * hit[2]).abs() < 1e-3, "{hit:?}");
        // A wall between the camera and the ground behind it: the wall is what the ray meets.
        let wall = |_: f32, z: f32| Some(if (2.0..3.0).contains(&z) { 9.0 } else { 0.0 });
        let hit = terrain_hit([0.0, 10.0, -6.0], [0.0, -0.5, 0.866], half, 0.25, wall).unwrap();
        assert!(hit[2] >= 2.0 - 0.01 && hit[2] < 3.0 && (hit[1] - 9.0).abs() < 1e-3, "the near face of the wall: {hit:?}");
        // In through the side, under a rim raised at the border: the border, at the rim's height.
        let rim = |_: f32, z: f32| Some(if z < -45.0 { 6.0 } else { 0.0 });
        let hit = terrain_hit([0.0, 8.0, -120.0], [0.0, -0.1, 0.994_987_4], half, 0.5, rim).unwrap();
        assert!((hit[2] + 50.0).abs() < 0.01 && (hit[1] - 6.0).abs() < 1e-3, "the pointer is on the terrain's edge: {hit:?}");
        // Looking up, starting under the surface, or beside the footprint: nothing.
        assert_eq!(terrain_hit([0.0, 10.0, 0.0], [0.0, 1.0, 0.0], half, 0.5, flat), None);
        assert_eq!(terrain_hit([0.0, 1.0, 0.0], down, half, 0.5, flat), None);
        assert_eq!(terrain_hit([500.0, 10.0, 0.0], [0.0, -1.0, 0.0], half, 0.5, flat), None);
        assert_eq!(terrain_hit([0.0, f32::NAN, 0.0], down, half, 0.5, flat), None);
        assert_eq!(terrain_hit([0.0, 10.0, 0.0], down, half, 0.0, flat), None);
        // A long way off: the march is spread over the footprint, never more than its cap.
        let looks = std::cell::Cell::new(0usize);
        let counted = |_: f32, _: f32| {
            looks.set(looks.get() + 1);
            Some(0.0)
        };
        let far = terrain_hit([0.0, 4000.0, -4000.0], [0.0, -0.70710677, 0.70710677], [5000.0, 5000.0], 0.01, counted).unwrap();
        assert!(far[1].abs() < 1e-2 && far[2].abs() < 20.0, "{far:?}");
        assert!(looks.get() <= MAX_MARCH + 40, "{} looks", looks.get());
    }

    #[test]
    fn a_ray_meets_a_model_at_its_nearest_triangle() {
        // Two squares facing the ray, at z = 0 and z = 5.
        let quad = |z: f32| vec![[-1.0, -1.0, z], [1.0, -1.0, z], [1.0, 1.0, z], [-1.0, 1.0, z]];
        let mut positions = quad(5.0);
        positions.extend(quad(0.0));
        let indices = [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
        let hit = mesh_hit([0.25, 0.5, -10.0], [0.0, 0.0, 1.0], &positions, &indices).unwrap();
        assert_eq!(hit, [0.25, 0.5, 0.0], "the nearer square");
        // From behind: the same squares, the other one first (either side counts).
        let hit = mesh_hit([0.25, 0.5, 20.0], [0.0, 0.0, -1.0], &positions, &indices).unwrap();
        assert_eq!(hit, [0.25, 0.5, 5.0]);
        assert_eq!(mesh_hit([3.0, 0.0, -10.0], [0.0, 0.0, 1.0], &positions, &indices), None, "beside them");
        assert_eq!(mesh_hit([0.0, 0.0, 10.0], [0.0, 0.0, 1.0], &positions, &indices), None, "behind the ray's start");
        // A broken index is skipped, not read.
        assert_eq!(mesh_hit([0.0, 0.0, -1.0], [0.0, 0.0, 1.0], &positions, &[0, 1, 99]), None);
        // Past the cap the test isn't made at all.
        let many: Vec<u32> = (0..(MAX_HIT_TRIANGLES + 1) * 3).map(|_| 0).collect();
        assert_eq!(mesh_hit([0.0, 0.0, -1.0], [0.0, 0.0, 1.0], &positions, &many), None);
    }

    #[test]
    fn a_stroke_samples_half_a_radius_apart_and_stops_at_one_calls_worth() {
        let mut s = Stroke::default();
        assert!(s.add([0.0, 0.0, 0.0], 2.0));
        assert!(!s.add([0.5, 0.0, 0.0], 2.0), "closer than half a radius");
        assert!(s.add([1.0, 0.0, 0.0], 2.0));
        assert!(!s.add([f32::NAN, 0.0, 0.0], 2.0));
        for k in 2..200 {
            s.add([k as f32, 0.0, 0.0], 2.0);
        }
        assert_eq!(s.points.len(), MAX_SAMPLES);
        assert!(s.full() && !s.add([1000.0, 0.0, 0.0], 2.0));
    }

    #[test]
    fn a_release_is_one_call_of_the_tool_the_brush_is_for() {
        let pts = [[1.0, 3.0, 2.0], [2.0, 3.5, 2.0]];
        let brush = |kind| Brush { kind, radius: 1.5, amount: 0.4, color: "#ff0000".into() };
        let raise = terrain_edit_args("recipes/field.json", Some("/field"), &brush(BrushKind::Raise), &pts, [10.0, 20.0]).unwrap();
        assert_eq!(
            raise.to_string(),
            r#"{"recipe":"recipes/field.json","strokes":[{"op":"raise","at":[11,22],"radius":1.5,"amount":0.4},{"op":"raise","at":[12,22],"radius":1.5,"amount":0.4}],"collider":"/field"}"#
        );
        let flat = terrain_edit_args("r", None, &brush(BrushKind::Flatten), &pts, [0.0, 0.0]).unwrap().to_string();
        assert!(flat.matches(r#""height":3"#).count() == 2, "flatten goes to where the stroke began: {flat}");
        let splat = terrain_edit_args("r", Some("/field"), &brush(BrushKind::Splat(2)), &pts, [0.0, 0.0]).unwrap().to_string();
        assert!(splat.contains(r#""op":"paint""#) && splat.contains(r#""layer":2"#) && !splat.contains("collider"), "a splat moves no surface: {splat}");
        assert_eq!(terrain_edit_args("r", None, &brush(BrushKind::Paint), &pts, [0.0, 0.0]), None);
        assert_eq!(terrain_edit_args("r", None, &brush(BrushKind::Raise), &[], [0.0, 0.0]), None);
        let paint = model_paint_args("recipes/statue.json", &brush(BrushKind::Paint), &pts).unwrap();
        assert_eq!(
            paint.to_string(),
            r##"{"recipe":"recipes/statue.json","strokes":[{"at":[1,3,2],"radius":1.5,"color":"#ff0000","amount":0.4},{"at":[2,3.5,2],"radius":1.5,"color":"#ff0000","amount":0.4}]}"##
        );
        assert_eq!(model_paint_args("r", &brush(BrushKind::Raise), &pts), None);
        // What they hold is what the tools take: at most one call's strokes.
        assert_eq!(MAX_SAMPLES, crate::terrain_tools::MAX_STROKES_PER_CALL);
        assert_eq!(MAX_SAMPLES, crate::paint_tools::MAX_STROKES_PER_CALL);
        let many: Vec<P3> = (0..200).map(|k| [k as f32, 0.0, 0.0]).collect();
        let args = model_paint_args("r", &brush(BrushKind::Paint), &many).unwrap();
        assert_eq!(args.get("strokes").and_then(Json::as_array).map(<[Json]>::len), Some(MAX_SAMPLES));
        // The tool's own check takes every stroke the editor makes.
        for s in args.get("strokes").and_then(Json::as_array).unwrap() {
            assert!(crate::paint_tools::stroke(s, 0).is_ok(), "{s}");
        }
    }

    #[test]
    fn a_ray_meets_a_models_ground_plane() {
        let down = |o: P3, d: P3| plane_hit(o, d, PATH_REACH);
        assert_eq!(down([1.0, 5.0, 2.0], [0.0, -1.0, 0.0]), Some([1.0, 0.0, 2.0]));
        let p = down([0.0, 4.0, 0.0], [0.6, -0.8, 0.0]).unwrap();
        assert!((p[0] - 3.0).abs() < 1e-5 && p[1] == 0.0 && p[2] == 0.0, "{p:?}");
        // From under the plane, looking up at it.
        assert_eq!(down([0.0, -2.0, 0.0], [0.0, 1.0, 0.0]), Some([0.0, 0.0, 0.0]));
        assert_eq!(down([0.0, 4.0, 0.0], [0.0, 1.0, 0.0]), None, "looking away from it");
        assert_eq!(down([0.0, 4.0, 0.0], [1.0, 0.0, 0.0]), None, "along it");
        assert_eq!(down([0.0, 0.0, 0.0], [0.0, -1.0, 0.0]), None, "on it: no point ahead");
        assert_eq!(down([0.0, 1.0, 0.0], [1.0, -1e-5, 0.0]), None, "a grazing ray lands beyond the reach");
        assert_eq!(down([f32::NAN, 1.0, 0.0], [0.0, -1.0, 0.0]), None);
    }

    #[test]
    fn paint_reaches_only_where_a_vertex_is_within_the_brush() {
        // A 6 m box: vertices at its corners only.
        let corners: Vec<P3> = (0..8).map(|i| [if i & 1 == 0 { -3.0 } else { 3.0 }, if i & 2 == 0 { 0.0 } else { 6.0 }, if i & 4 == 0 { -3.0 } else { 3.0 }]).collect();
        let face_centre = [[0.0, 3.0, 3.0], [0.5, 3.0, 3.0]];
        assert_eq!(paint_reaches(&corners, &face_centre, 1.0), Some(false), "the middle of a face: no vertex within a metre");
        assert_eq!(paint_reaches(&corners, &face_centre, 4.5), Some(true), "a brush wide enough for the corners");
        assert_eq!(paint_reaches(&corners, &[[2.5, 5.5, 3.0]], 1.0), Some(true), "near a corner");
        assert_eq!(paint_reaches(&corners, &[[0.0, 3.0, 3.0], [3.0, 6.0, 3.0]], 0.01), Some(true), "one dab on a vertex is enough");
        assert_eq!((paint_reaches(&[], &face_centre, 100.0), paint_reaches(&corners, &[], 100.0)), (Some(false), Some(false)));
        // A mesh too dense to test at a release is not tested (review R1, m4: 5 ms): the
        // answer is "not known", which the editor takes as "paint it".
        let dense: Vec<P3> = (0..MAX_REACH_TESTS / 64 + 1).map(|i| [i as f32, 0.0, 0.0]).collect();
        let dabs: Vec<P3> = (0..64).map(|k| [k as f32, 50.0, 50.0]).collect();
        assert_eq!(paint_reaches(&dense, &dabs, 0.5), None);
        assert_eq!(paint_reaches(&dense[..dense.len() - 1], &dabs, 0.5), Some(false), "at the limit it is still worked out");
    }

    /// Review R1 of m5-eddraw, m1: a ray straight down met nothing (every ray of a top-down
    /// orthographic view is one).
    #[test]
    fn a_ray_straight_down_meets_the_terrain_under_it() {
        let hill = |x: f32, _: f32| Some(x * 0.5);
        assert_eq!(terrain_hit([4.0, 10.0, 2.0], [0.0, -1.0, 0.0], [50.0, 50.0], 0.5, hill), Some([4.0, 2.0, 2.0]));
        assert_eq!(terrain_hit([4.0, 2.0, 2.0], [0.0, -1.0, 0.0], [50.0, 50.0], 0.5, hill), Some([4.0, 2.0, 2.0]), "from on the surface");
        assert_eq!(terrain_hit([4.0, 10.0, 2.0], [0.0, 1.0, 0.0], [50.0, 50.0], 0.5, hill), None, "straight up, away from it");
        assert_eq!(terrain_hit([4.0, 1.0, 2.0], [0.0, -1.0, 0.0], [50.0, 50.0], 0.5, hill), None, "from under the surface");
        assert_eq!(terrain_hit([60.0, 10.0, 2.0], [0.0, -1.0, 0.0], [50.0, 50.0], 0.5, hill), None, "beside the footprint");
        assert_eq!(terrain_hit([4.0, 10.0, 2.0], [0.0, -1.0, 0.0], [50.0, 50.0], 0.5, |_, _| None), None, "no surface there");
    }

    /// Review R1 of m5-eddraw, M4: the Draw bar's sizes are world metres; a stroke on a scaled
    /// entity is written in the entity's own units, so what is painted is what the ring shows.
    #[test]
    fn a_stroke_on_a_scaled_entity_is_written_in_the_entitys_own_units() {
        let paint = Brush { kind: BrushKind::Paint, radius: 0.6, amount: 0.5, ..Brush::default() };
        let local = in_frame(&paint, 3.0, 3.0);
        assert!((local.radius - 0.2).abs() < 1e-6 && local.amount == 0.5, "a third of the radius; a share is a share");
        let args = model_paint_args("r.json", &local, &[[1.0, 2.0, 3.0]]).unwrap();
        assert!(args.to_string().contains(r#""radius":0.2,"#), "{args}");
        // A sculpt's raise is metres of height: scaled by the entity's height scale.
        let raise = Brush { kind: BrushKind::Raise, radius: 6.0, amount: 1.0, ..Brush::default() };
        let local = in_frame(&raise, 2.0, 4.0);
        assert_eq!((local.radius, local.amount), (3.0, 0.25));
        let args = terrain_edit_args("t.json", None, &local, &[[0.0, 0.0, 0.0]], [0.0, 0.0]).unwrap();
        assert!(args.to_string().contains(r#""radius":3,"amount":0.25"#), "{args}");
        // The dabs are spaced by the same measure: half the local radius.
        let mut s = Stroke::default();
        assert!(s.add([0.0, 0.0, 0.0], local.radius) && !s.add([1.4, 0.0, 0.0], local.radius) && s.add([1.5, 0.0, 0.0], local.radius));
        // An unscaled entity: as before. A degenerate scale is taken as none.
        assert_eq!(in_frame(&paint, 1.0, 1.0), paint);
        assert_eq!(in_frame(&paint, 0.0, f32::NAN), paint);
        // The label stays in world metres (it is what the ring shows).
        assert_eq!(in_frame(&paint, 3.0, 3.0).kind, paint.kind);
        // Review R2, N4: a brush the entity's scale puts outside what a stroke takes is said in
        // world metres, with the widths that fit here; and a sculpt too weak to keep is said,
        // never made stronger.
        assert_eq!(out_of_range(&paint, &in_frame(&paint, 3.0, 3.0), 3.0), None);
        let default = Brush { kind: BrushKind::Paint, radius: 2.0, ..Brush::default() };
        let why = out_of_range(&default, &in_frame(&default, 0.01, 0.01), 0.01).unwrap();
        assert!(why.contains("scaled 0.010x") && why.contains("4.0 m wide") && why.contains("0.000100..1.3 m wide"), "no length reads as 0.000 (review R3, F5): {why}");
        let tiny = Brush { kind: BrushKind::Paint, radius: 0.05, ..Brush::default() };
        let why = out_of_range(&tiny, &in_frame(&tiny, 100.0, 100.0), 100.0).unwrap();
        assert!(why.contains("1.0..12800 m wide"), "{why}");
        let weak = Brush { kind: BrushKind::Raise, radius: 20.0, amount: 0.05, ..Brush::default() };
        let why = out_of_range(&weak, &in_frame(&weak, 100.0, 100.0), 100.0).unwrap();
        assert!(why.contains("a raise of 0.050 m") && why.contains("stronger brush"), "{why}");
        // The other end (review R3, F5): a terrain squashed in height, where a metre is over
        // 100 of its own units, was clamped to a weaker stroke than the bar said.
        let squashed = Brush { kind: BrushKind::Lower, radius: 2.0, amount: 1.0, ..Brush::default() };
        let why = out_of_range(&squashed, &in_frame(&squashed, 1.0, 0.005), 1.0).unwrap();
        assert!(why.contains("a lower of 1.0 m is 200 of its own units") && why.contains("weaker brush"), "{why}");
        assert_eq!(out_of_range(&squashed, &in_frame(&squashed, 1.0, 0.01), 1.0), None, "exactly 100 is taken");
        let strong = Brush { amount: 0.5, ..weak.clone() };
        let local = in_frame(&strong, 100.0, 100.0);
        assert_eq!(out_of_range(&strong, &local, 100.0), None);
        let args = terrain_edit_args("t.json", None, &local, &[[0.0, 0.0, 0.0]], [0.0, 0.0]).unwrap();
        assert!(args.to_string().contains(r#""amount":0.005"#), "0.5 m on a 100x terrain is 0.005 of its units, as sent: {args}");
    }

    #[test]
    fn a_sketched_path_ends_where_the_pointer_was_let_go() {
        let mut s = Stroke::default();
        assert!(!s.end_at([1.0, 0.0, 0.0], 2.0), "no stroke to end");
        // Samples a metre apart (radius 2), then the release 0.6 m on: a point of its own.
        for x in [0.0, 1.0, 2.0] {
            assert!(s.add([x, 0.0, 0.0], 2.0));
        }
        assert!(s.end_at([2.6, 0.0, 0.0], 2.0));
        assert_eq!(s.points.last(), Some(&[2.6, 0.0, 0.0]));
        assert_eq!(s.points.len(), 4);
        // A release just past the last sample moves it there: no short stub at the end.
        let mut t = Stroke { points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]] };
        assert!(t.end_at([2.3, 0.0, 0.0], 2.0));
        assert_eq!(t.points, vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.3, 0.0, 0.0]]);
        // On the last sample (a click, or a hand that stopped on it): nothing changes.
        assert!(!t.end_at([2.3, 0.0, 0.005], 2.0));
        // One sample and a release near it: two points, a path (never a move of the only one).
        let mut one = Stroke { points: vec![[0.0, 0.0, 0.0]] };
        assert!(one.end_at([0.05, 0.0, 0.0], 2.0));
        assert_eq!(one.points.len(), 2);
        // A full stroke ends at the release too: its last sample moves.
        let mut full = Stroke { points: (0..MAX_SAMPLES).map(|i| [i as f32, 0.0, 0.0]).collect() };
        assert!(full.end_at([70.0, 0.0, 0.0], 2.0));
        assert_eq!((full.points.len(), full.points[MAX_SAMPLES - 1]), (MAX_SAMPLES, [70.0, 0.0, 0.0]));
        // A release back on the point before the last would fold the path onto itself.
        let mut back = Stroke { points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.2, 0.0, 0.0]] };
        assert!(!back.end_at([1.0, 0.0, 0.001], 2.0));
        assert!(!back.end_at([f32::NAN, 0.0, 0.0], 2.0));
        assert_eq!(back.points.len(), 3);
    }

    #[test]
    fn a_sketched_path_is_one_model_path_call() {
        let brush = Brush { kind: BrushKind::Path(1), radius: 1.0, amount: 0.5, ..Brush::default() };
        assert!(!brush.kind.on_terrain() && brush.kind.is_path());
        assert_eq!(brush.label(), "path of sweep 2: a point every 0.50 m, smoothing 4");
        let pts = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.5], [2.0004, 0.0, 0.0]];
        let args = model_path_args("recipes/fence.json", &brush, &pts).unwrap();
        assert_eq!(args.to_string(), r#"{"recipe":"recipes/fence.json","sweep":1,"points":[[0,0],[1,0.5],[2,0]],"smooth":4}"#);
        // The tool takes what the editor sends.
        const { assert!(MAX_SAMPLES <= crate::path_tools::MAX_POINTS) };
        let (width, rows) = crate::path_tools::points(&args).unwrap();
        assert_eq!((width, rows.len()), (2, 3));
        // A click is no path; the other brushes send no path; the path tool sends no strokes.
        assert!(model_path_args("r", &brush, &pts[..1]).is_none());
        assert!(model_path_args("r", &Brush::default(), &pts).is_none());
        assert!(terrain_edit_args("r", None, &brush, &pts, [0.0, 0.0]).is_none());
        assert!(model_paint_args("r", &brush, &pts).is_none());
        // Smoothing follows the Strength control, within the sweep's range.
        let smooth = |amount: f32| Brush { amount, ..brush.clone() }.path_smooth();
        assert_eq!((smooth(0.1), smooth(0.5), smooth(1.0), smooth(0.0), smooth(9.0)), (1, 4, 8, 1, 8));
    }

    #[test]
    fn the_brush_steps_within_its_range() {
        let mut b = Brush::default();
        for _ in 0..100 {
            b.radius = b.stepped_radius(true);
            b.amount = b.stepped_amount(true);
        }
        assert_eq!((b.radius, b.amount), (RADIUS.1, 1.0));
        for _ in 0..100 {
            b.radius = b.stepped_radius(false);
            b.amount = b.stepped_amount(false);
        }
        assert_eq!((b.radius, b.amount), (RADIUS.0, 0.1));
        assert_eq!(BrushKind::Raise.next_sculpt().next_sculpt().next_sculpt().next_sculpt(), BrushKind::Raise);
        assert_eq!(BrushKind::Paint.next_sculpt(), BrushKind::Raise);
    }
}

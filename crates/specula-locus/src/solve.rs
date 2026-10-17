//! The solver: candidates from the most constraining relation, every relation measured at each,
//! collisions checked through the [`Probe`], the best kept (docs/design/LOCUS.md, "The solver").

use crate::geom::{segment_distance, yaw_axes, yaw_difference, yaw_toward, Box3};
use crate::{MAX_CANDIDATES, MAX_COUNT, MAX_RELATIONS};
use specula_core::dmath;
use specula_core::math::Vec3;

/// What the solver asks the world (the engine implements it over physics and the navmesh).
pub trait Probe {
    /// The first surface a ray straight down from `(x, from_y, z)` hits within `depth` metres:
    /// its height and entity. With `only`, that entity alone counts.
    fn ground(&mut self, x: f32, z: f32, from_y: f32, depth: f32, only: Option<u64>) -> Option<(f32, u64)>;
    /// The first entity the box overlaps (lowest id), if any.
    fn blocked(&mut self, b: &Box3) -> Option<u64>;
    /// The nearest navmesh point to `p` (close by), if any.
    fn navmesh(&mut self, p: Vec3) -> Option<Vec3>;
    /// Whether a complete path leads from `from` to `to` on the navmesh.
    fn reachable(&mut self, from: Vec3, to: Vec3) -> bool;
    /// The navmesh's walkable area as a box (for `on_navmesh` with nothing else to go on).
    fn navmesh_area(&mut self) -> Option<Box3>;
}

/// A relation, with its targets already resolved to geometry.
#[derive(Clone, Debug, PartialEq)]
pub enum Rel {
    /// Rests on the target's top (`None`: on whatever is below).
    On { target: Option<(Box3, u64)> },
    /// Back against one of the target's vertical faces (`side`: the world direction the face
    /// looks toward), `gap` metres away.
    Against { target: Box3, side: Option<(f32, f32)>, gap: f32 },
    /// On the line `a`-`b` (shifted `offset` metres to its left looking from a to b).
    Along { a: Vec3, b: Vec3, offset: f32 },
    /// On the segment `a`-`b` (at fraction `t` when given).
    Between { a: Vec3, b: Vec3, t: Option<f32> },
    /// Within `max` (and beyond `min`) of the target's footprint.
    Near { target: Box3, min: f32, max: f32 },
    /// Footprint inside the region (in X/Z; `y` bounds the bottom and top when given).
    Inside { region: Box3, y: Option<(f32, f32)> },
    /// Front toward a point or along a direction.
    Facing { point: Option<Vec3>, direction: Option<(f32, f32)> },
    /// At least `min` (at most `max` from the nearest) between instances of this call, and at
    /// least `min` from the `among` boxes.
    Spacing { min: f32, max: Option<f32>, among: Vec<Box3> },
    /// At least `distance` from every box.
    Avoid { boxes: Vec<Box3>, distance: f32 },
    /// On the navmesh (and reachable by path from a point).
    OnNavmesh { reachable_from: Option<Vec3> },
}

impl Rel {
    pub fn name(&self) -> &'static str {
        match self {
            Rel::On { .. } => "on",
            Rel::Against { .. } => "against",
            Rel::Along { .. } => "along",
            Rel::Between { .. } => "between",
            Rel::Near { .. } => "near",
            Rel::Inside { .. } => "inside",
            Rel::Facing { .. } => "facing",
            Rel::Spacing { .. } => "spacing",
            Rel::Avoid { .. } => "avoid",
            Rel::OnNavmesh { .. } => "on_navmesh",
        }
    }

    fn is_angle(&self) -> bool {
        matches!(self, Rel::Facing { .. })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Relation {
    pub rel: Rel,
    /// A preference: weighs 1 instead of 1000, and never makes a placement unsatisfied.
    pub soft: bool,
}

/// What to place.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// The object's local bounds (scaled), relative to its origin.
    pub lo: Vec3,
    pub hi: Vec3,
    pub count: usize,
    pub relations: Vec<Relation>,
    /// Where it is now (a move) or was asked to spawn; the fallback search centre.
    pub start: Vec3,
    /// Its yaw when no relation decides one (degrees).
    pub yaw_deg: f32,
    /// Which local direction is its front, as a yaw from local +Z (0: +Z, 180: -Z, 90: +X).
    pub front_deg: f32,
    /// Rest on whatever is below when no relation says where vertically.
    pub settle: bool,
    /// Placements must not overlap anything (and each other).
    pub collide: bool,
    /// Offsets the candidate grid's phase (0: none).
    pub seed: u64,
}

/// One relation's result at a placement.
#[derive(Clone, Debug, PartialEq)]
pub struct RelReport {
    pub index: usize,
    pub rel: &'static str,
    pub satisfied: bool,
    /// How far off: metres, or degrees for `facing`.
    pub off: f32,
    pub why: Option<String>,
}

/// One instance's best placement.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// The object's origin, world space.
    pub position: Vec3,
    pub yaw_deg: f32,
    pub footprint: Box3,
    /// Every hard relation holds and nothing collides.
    pub satisfied: bool,
    pub relations: Vec<RelReport>,
    /// The entity it would overlap (only when no free spot was found).
    pub blocked_by: Option<u64>,
    /// The instance of this call it would overlap.
    pub overlaps_instance: Option<usize>,
    pub rests_on: Option<u64>,
    /// Candidates evaluated.
    pub candidates: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Solution {
    pub placements: Vec<Placement>,
    /// What to relax, in words.
    pub hints: Vec<String>,
}

/// Tolerances: a relation off by less counts as held.
const TOL_M: f32 = 1.0e-3;
const TOL_DEG: f32 = 0.5;
/// Degrees count this many metres in the score.
const DEG_WEIGHT: f32 = 0.02;
const HARD: f32 = 1000.0;
const COLLISION: f32 = 1.0e6;
/// How far outside its generator the solver looks for a near miss (metres).
const RING: f32 = 2.0;
/// Navmesh closeness that counts as on it (metres).
const NAV_TOL: f32 = 0.05;
/// Settling looks for ground from this far above the generator's base (a step, metres).
const SETTLE_STEP: f32 = 0.5;

/// Where candidates come from.
#[derive(Clone, Debug)]
enum Gen {
    /// Segments with an optional yaw each (an `against` face) and the base height.
    Segments(Vec<(Vec3, Vec3, Option<f32>)>),
    /// An area (X/Z of the box) and the base height.
    Area(Box3, f32),
    /// Around a point (base height in its y).
    Point(Vec3),
}

#[derive(Clone, Copy, Debug)]
struct Cand {
    x: f32,
    z: f32,
    /// Base (bottom) height before settling.
    base: f32,
    /// A yaw from the generator (an `against` face).
    yaw: Option<f32>,
    /// Position along the generator (segments: metres from the start; for even shares).
    s: f32,
    /// The generator's length (segments).
    len: f32,
}

struct Scored {
    score: f32,
    placement: Placement,
}

/// A small, fixed hash for the grid phase.
fn phase(seed: u64) -> f32 {
    if seed == 0 {
        return 0.0;
    }
    let h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    ((h >> 40) as f32) / ((1u64 << 24) as f32)
}

fn faces(b: &Box3) -> [((f32, f32), f32, f32); 4] {
    // (outward normal, distance from centre to the face, half length of the face)
    [(b.ax, b.half.x, b.half.z), ((-b.ax.0, -b.ax.1), b.half.x, b.half.z), (b.az, b.half.z, b.half.x), ((-b.az.0, -b.az.1), b.half.z, b.half.x)]
}

impl Request {
    fn half(&self) -> Vec3 {
        (self.hi - self.lo) * 0.5
    }

    /// The generator: the most constraining relation's geometry.
    fn generator(&self, probe: &mut dyn Probe) -> Gen {
        let rels = &self.relations;
        for r in rels {
            if let Rel::Between { a, b, t: Some(t) } = &r.rel {
                return Gen::Point(*a + (*b - *a) * *t);
            }
        }
        for r in rels {
            match &r.rel {
                Rel::Between { a, b, t: None } => return Gen::Segments(vec![(*a, *b, None)]),
                Rel::Along { a, b, offset } => {
                    let (dx, dz) = (b.x - a.x, b.z - a.z);
                    let len = (dx * dx + dz * dz).sqrt().max(1.0e-6);
                    let (lx, lz) = (dz / len * offset, -dx / len * offset);
                    let shift = Vec3::new(lx, 0.0, lz);
                    return Gen::Segments(vec![(*a + shift, *b + shift, None)]);
                }
                _ => {}
            }
        }
        for r in rels {
            if let Rel::Against { target, side, gap } = &r.rel {
                let mut list: Vec<(f32, usize, (Vec3, Vec3, Option<f32>))> = Vec::new();
                for (i, (n, d, along)) in faces(target).into_iter().enumerate() {
                    if let Some(s) = side {
                        // The face looking most toward the side, only.
                        let best = faces(target).iter().map(|(m, _, _)| m.0 * s.0 + m.1 * s.1).fold(f32::NEG_INFINITY, f32::max);
                        if n.0 * s.0 + n.1 * s.1 < best - 1.0e-4 {
                            continue;
                        }
                    }
                    let yaw = yaw_toward(n.0, n.1) - self.front_deg;
                    // Footprint depth behind its origin, turned so its back faces the wall.
                    let (ax, az) = yaw_axes(yaw);
                    let half = self.half();
                    let c = (self.lo + self.hi) * 0.5;
                    // Extent of the footprint along -n from the origin.
                    let ext_ax = (ax.0 * -n.0 + ax.1 * -n.1) * c.x + (ax.0 * -n.0 + ax.1 * -n.1).abs() * half.x;
                    let ext_az = (az.0 * -n.0 + az.1 * -n.1) * c.z + (az.0 * -n.0 + az.1 * -n.1).abs() * half.z;
                    let behind = ext_ax + ext_az;
                    let out = d + gap + behind;
                    let centre = (target.center.x + n.0 * out, target.center.z + n.1 * out);
                    let t = (-n.1, n.0);
                    let a = Vec3::new(centre.0 - t.0 * along, target.bottom(), centre.1 - t.1 * along);
                    let b = Vec3::new(centre.0 + t.0 * along, target.bottom(), centre.1 + t.1 * along);
                    list.push((-along, i, (a, b, Some(yaw))));
                }
                // Longest faces first (then face order).
                list.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
                return Gen::Segments(list.into_iter().map(|(_, _, s)| s).collect());
            }
        }
        for r in rels {
            match &r.rel {
                Rel::On { target: Some((b, _)) } => return Gen::Area(*b, b.top()),
                Rel::Inside { region, y } => return Gen::Area(*region, y.map_or(self.start.y + self.lo.y, |y| y.0)),
                Rel::Near { target, max, .. } => {
                    let grow = *max + self.half().x.max(self.half().z);
                    let area = Box3 { half: Vec3::new(target.half.x + grow, target.half.y, target.half.z + grow), ..*target };
                    return Gen::Area(area, target.bottom());
                }
                _ => {}
            }
        }
        if rels.iter().any(|r| matches!(r.rel, Rel::OnNavmesh { .. })) {
            if let Some(area) = probe.navmesh_area() {
                return Gen::Area(area, area.bottom());
            }
        }
        Gen::Point(Vec3::new(self.start.x, self.start.y + self.lo.y, self.start.z))
    }

    /// The candidates, in a fixed order, at most [`MAX_CANDIDATES`].
    fn candidates(&self, gen: &Gen, step: f32) -> Vec<Cand> {
        let mut out = Vec::new();
        let ph = phase(self.seed);
        match gen {
            Gen::Point(p) => {
                out.push(Cand { x: p.x, z: p.z, base: p.y, yaw: None, s: 0.0, len: 0.0 });
                for r in [0.25f32, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0] {
                    for k in 0..16 {
                        let (s, c) = dmath::sin_cos((k as f32 + ph) * (std::f32::consts::TAU / 16.0));
                        out.push(Cand { x: p.x + c * r, z: p.z + s * r, base: p.y, yaw: None, s: 0.0, len: 0.0 });
                    }
                }
            }
            Gen::Segments(list) => {
                let per = (MAX_CANDIDATES / list.len().max(1) / 9).max(8);
                for (a, b, yaw) in list {
                    let (dx, dz) = (b.x - a.x, b.z - a.z);
                    let len = (dx * dx + dz * dz).sqrt();
                    let (ux, uz) = if len > 1.0e-6 { (dx / len, dz / len) } else { (1.0, 0.0) };
                    let (nx, nz) = (uz, -ux);
                    let total = len + 2.0 * RING;
                    let st = step.max(total / per as f32);
                    let n = (dmath::floor(total / st) as usize + 1).min(per);
                    for off in [0.0f32, 0.25, -0.25, 0.5, -0.5, 1.0, -1.0, 2.0, -2.0] {
                        for i in 0..n {
                            let s = -RING + (i as f32 + ph) * st;
                            out.push(Cand {
                                x: a.x + ux * s + nx * off,
                                z: a.z + uz * s + nz * off,
                                base: a.y + (b.y - a.y) * (s / len.max(1.0e-6)).clamp(0.0, 1.0),
                                yaw: *yaw,
                                s,
                                len,
                            });
                        }
                    }
                }
            }
            Gen::Area(area, base) => {
                out.push(Cand { x: area.center.x, z: area.center.z, base: *base, yaw: None, s: 0.0, len: 0.0 });
                let (hu, hv) = (area.half.x + RING * 0.5, area.half.z + RING * 0.5);
                let side = (MAX_CANDIDATES as f32).sqrt() as usize;
                let nu = ((2.0 * hu / step) as usize + 1).clamp(2, side);
                let nv = ((2.0 * hv / step) as usize + 1).clamp(2, side);
                for j in 0..nv {
                    for i in 0..nu {
                        let u = -hu + 2.0 * hu * ((i as f32 + ph) / (nu - 1) as f32).min(1.0);
                        let v = -hv + 2.0 * hv * ((j as f32 + ph) / (nv - 1) as f32).min(1.0);
                        out.push(Cand {
                            x: area.center.x + area.ax.0 * u + area.az.0 * v,
                            z: area.center.z + area.ax.1 * u + area.az.1 * v,
                            base: *base,
                            yaw: None,
                            s: 0.0,
                            len: 0.0,
                        });
                    }
                }
            }
        }
        out.truncate(MAX_CANDIDATES + 1);
        out
    }
}

/// Solves where `req.count` instances go.
pub fn solve(req: &Request, probe: &mut dyn Probe) -> Solution {
    let count = req.count.clamp(1, MAX_COUNT);
    let relations: Vec<&Relation> = req.relations.iter().take(MAX_RELATIONS).collect();
    let gen = req.generator(probe);
    let half = req.half();
    let step = (half.x.min(half.z) * 0.5).clamp(0.05, 1.0);
    let coarse = req.candidates(&gen, step);
    let mut placed: Vec<Placement> = Vec::new();
    let mut hints = Vec::new();
    for k in 0..count {
        let mut best: Option<Scored> = None;
        let mut evaluated = 0usize;
        for c in &coarse {
            evaluated += 1;
            if let Some(s) = evaluate(req, &relations, c, k, count, &placed, probe, best.as_ref().map_or(f32::INFINITY, |b| b.score)) {
                if best.as_ref().is_none_or(|b| s.score < b.score) {
                    best = Some(s);
                }
            }
        }
        // Refine around the best (a finer grid, or finer steps along its segment).
        if let Some(b) = &best {
            let p = b.placement.position;
            let fine = step * 0.25;
            let mut extra = Vec::new();
            let seg = matches!(gen, Gen::Segments(_));
            let found = coarse.iter().find(|c| (c.x - p.x).abs() < 1.0e-6 && (c.z - p.z).abs() < 1.0e-6).copied();
            if let Some(c0) = found {
                if seg && c0.len > 0.0 {
                    let (dx, dz) = match &gen {
                        Gen::Segments(list) => list
                            .iter()
                            .find(|(_, _, y)| *y == c0.yaw)
                            .map(|(a, b, _)| {
                                let (dx, dz) = (b.x - a.x, b.z - a.z);
                                let l = (dx * dx + dz * dz).sqrt().max(1.0e-6);
                                (dx / l, dz / l)
                            })
                            .unwrap_or((1.0, 0.0)),
                        _ => (1.0, 0.0),
                    };
                    for i in -8i32..=8 {
                        let d = i as f32 * fine;
                        extra.push(Cand { x: c0.x + dx * d, z: c0.z + dz * d, s: c0.s + d, ..c0 });
                    }
                } else {
                    for j in -4i32..=4 {
                        for i in -4i32..=4 {
                            extra.push(Cand { x: c0.x + i as f32 * fine, z: c0.z + j as f32 * fine, ..c0 });
                        }
                    }
                }
            }
            for c in &extra {
                evaluated += 1;
                if let Some(s) = evaluate(req, &relations, c, k, count, &placed, probe, best.as_ref().map_or(f32::INFINITY, |b| b.score)) {
                    if best.as_ref().is_none_or(|b| s.score < b.score) {
                        best = Some(s);
                    }
                }
            }
        }
        let Some(mut b) = best else { break };
        b.placement.candidates = evaluated;
        placed.push(b.placement);
    }
    // Hints for spreading several instances along a segment.
    if placed.iter().any(|p| !p.satisfied) {
        if let Gen::Segments(list) = &gen {
            let len = list.iter().map(|(a, b, _)| {
                let (dx, dz) = (b.x - a.x, b.z - a.z);
                (dx * dx + dz * dz).sqrt()
            });
            let len = len.fold(0.0f32, f32::max);
            let width = 2.0 * half.x.max(half.z);
            let spacing = relations.iter().find_map(|r| match &r.rel {
                Rel::Spacing { min, .. } => Some(*min),
                _ => None,
            });
            if count > 1 {
                // Centres lie on the segment; neighbours are `width + spacing` apart.
                let gap = spacing.unwrap_or(0.0);
                let pitch = width + gap;
                let fit = dmath::floor(len / pitch.max(1.0e-3)) as usize + 1;
                let mut h = format!("{} of {count} fit with {gap:.2} m between them along {len:.2} m", fit.min(count));
                let most = len / (count - 1) as f32 - width;
                if most >= 0.0 {
                    h.push_str(&format!("; all {count} fit with at most {most:.2} m between them"));
                } else {
                    h.push_str(&format!("; even touching, at most {} fit", dmath::floor(len / width.max(1.0e-3)) as usize + 1));
                }
                hints.push(h);
            }
        }
        if placed.iter().any(|p| p.blocked_by.is_some() || p.overlaps_instance.is_some()) {
            hints.push("no collision-free spot was found for some instances within 2 m of what the relations ask: move or remove what blocks them, or loosen a relation".into());
        }
    }
    Solution { placements: placed, hints }
}

/// Scores one candidate; `None` when it can't beat `bound` (probes skipped).
#[allow(clippy::too_many_arguments)]
fn evaluate(req: &Request, rels: &[&Relation], c: &Cand, k: usize, count: usize, placed: &[Placement], probe: &mut dyn Probe, bound: f32) -> Option<Scored> {
    // Yaw: facing, else the generator's (against), else along's line, else as given.
    let mut yaw = c.yaw.unwrap_or(req.yaw_deg);
    let mut facing_yaw = None;
    for r in rels {
        if let Rel::Along { a, b, .. } = &r.rel {
            if c.yaw.is_none() {
                // Local X along the line.
                yaw = dmath::atan2(-(b.z - a.z), b.x - a.x).to_degrees() - req.front_deg;
            }
        }
    }
    for r in rels {
        if let Rel::Facing { point, direction } = &r.rel {
            let want = match (point, direction) {
                (Some(p), _) => {
                    let (dx, dz) = (p.x - c.x, p.z - c.z);
                    (dx * dx + dz * dz > 1.0e-8).then(|| yaw_toward(dx, dz))
                }
                (None, Some(d)) => Some(yaw_toward(d.0, d.1)),
                _ => None,
            };
            if let Some(w) = want {
                facing_yaw = Some(w - req.front_deg);
                if !r.soft || c.yaw.is_none() {
                    yaw = w - req.front_deg;
                }
            }
        }
    }
    // Height: rest on the target (on) or on what is below (settle), else the generator's base.
    let on_target = rels.iter().find_map(|r| match &r.rel {
        Rel::On { target } => Some(*target),
        _ => None,
    });
    let probe_ground = on_target.is_some() || req.settle;
    let height = req.hi.y - req.lo.y;
    let fp0 = Box3::footprint(req.lo, req.hi, Vec3::new(c.x, 0.0, c.z), yaw);
    let mut base = c.base;
    let mut rests_on = None;
    let mut ground_missing = false;
    if probe_ground {
        let only = on_target.flatten().map(|(_, id)| id);
        // On a target: from just above its top. Settling: from a step above the generator's base
        // (so it lands on the floor beside a crate, not on the crate), then from high up if
        // nothing is below that.
        let starts: &[f32] = match on_target.flatten() {
            Some((b, _)) => &[b.top() + 0.5],
            None => &[c.base + SETTLE_STEP, c.base + height.max(1.0) + 20.0],
        };
        let inset = fp0.shrunk(0.01);
        let mut points = vec![(c.x, c.z)];
        points.extend(inset.corners());
        let mut top: Option<(f32, u64)> = None;
        for &from in starts {
            for &(x, z) in &points {
                if let Some((y, id)) = probe.ground(x, z, from, 60.0, only) {
                    if top.is_none_or(|(t, tid)| y > t || (y == t && id < tid)) {
                        top = Some((y, id));
                    }
                }
            }
            if top.is_some() {
                break;
            }
        }
        match top {
            Some((y, id)) => {
                base = y;
                rests_on = Some(id);
            }
            None => ground_missing = true,
        }
    }
    let position = Vec3::new(c.x, base - req.lo.y, c.z);
    let fp = Box3::footprint(req.lo, req.hi, position, yaw);
    // Relations.
    let mut reports = Vec::with_capacity(rels.len());
    let mut score = 0.0f32;
    let mut nav_point = None;
    for (i, r) in rels.iter().enumerate() {
        let mut why = None;
        let off = match &r.rel {
            Rel::On { target } => match target {
                Some((b, id)) => {
                    let over = b.overhang(&fp);
                    if rests_on != Some(*id) {
                        why = Some("not over the target".to_string());
                        over.max(b.outside((c.x, c.z))).max(TOL_M * 2.0)
                    } else {
                        if over > TOL_M {
                            why = Some("sticks out over the target's edge".into());
                        }
                        over
                    }
                }
                None => {
                    if ground_missing {
                        why = Some("nothing below within 60 m".into());
                        1.0
                    } else {
                        0.0
                    }
                }
            },
            Rel::Against { target, side, gap } => {
                let d = target.distance_xz(&fp);
                let mut off = (d - gap).abs();
                if let Some(s) = side {
                    // The nearest face must be the one looking toward the side.
                    let (u, v) = target.local((fp.center.x, fp.center.z));
                    let (eu, ev) = (u.abs() - target.half.x, v.abs() - target.half.z);
                    let n = if eu >= ev { (target.ax.0 * u.signum(), target.ax.1 * u.signum()) } else { (target.az.0 * v.signum(), target.az.1 * v.signum()) };
                    if n.0 * s.0 + n.1 * s.1 < 0.7 {
                        why = Some("against another face than the side asked for".into());
                        off += 1.0;
                    }
                }
                if off > TOL_M && why.is_none() {
                    why = Some(format!("{d:.3} m from the target, asked {gap:.3} m"));
                }
                off
            }
            Rel::Along { a, b, offset } => {
                let (dx, dz) = (b.x - a.x, b.z - a.z);
                let len = (dx * dx + dz * dz).sqrt().max(1.0e-6);
                let (lx, lz) = (dz / len * offset, -dx / len * offset);
                let d = segment_distance((c.x, c.z), (a.x + lx, a.z + lz), (b.x + lx, b.z + lz));
                if d > TOL_M {
                    why = Some("off the line".into());
                }
                d
            }
            Rel::Between { a, b, t } => {
                let d = match t {
                    Some(t) => {
                        let p = *a + (*b - *a) * *t;
                        ((p.x - c.x) * (p.x - c.x) + (p.z - c.z) * (p.z - c.z)).sqrt()
                    }
                    None => segment_distance((c.x, c.z), (a.x, a.z), (b.x, b.z)),
                };
                if d > TOL_M {
                    why = Some("off the segment between them".into());
                }
                d
            }
            Rel::Near { target, min, max } => {
                let d = target.distance_xz(&fp);
                let off = (d - max).max(0.0) + (min - d).max(0.0);
                if off > TOL_M {
                    why = Some(format!("{d:.3} m from the target"));
                }
                off
            }
            Rel::Inside { region, y } => {
                let mut off = region.overhang(&fp);
                if let Some((lo, hi)) = y {
                    off += (lo - fp.bottom()).max(0.0) + (fp.top() - hi).max(0.0);
                }
                if off > TOL_M {
                    why = Some("sticks out of the region".into());
                }
                off
            }
            Rel::Facing { .. } => {
                let d = facing_yaw.map_or(0.0, |f| yaw_difference(f, yaw));
                if d > TOL_DEG {
                    why = Some(format!("turned {d:.1} degrees away (another relation sets its yaw)"));
                }
                d
            }
            Rel::Spacing { min, max, among } => {
                let mut nearest = f32::INFINITY;
                for p in placed {
                    nearest = nearest.min(p.footprint.distance_xz(&fp));
                }
                let mut off = if nearest.is_finite() { (min - nearest).max(0.0) } else { 0.0 };
                if let (Some(mx), true) = (max, nearest.is_finite()) {
                    off += (nearest - mx).max(0.0);
                }
                for b in among {
                    off += (min - b.distance_xz(&fp)).max(0.0);
                }
                if off > TOL_M {
                    why = Some(if nearest.is_finite() { format!("{nearest:.3} m from the nearest other instance") } else { "too close to an 'among' entity".into() });
                }
                off
            }
            Rel::Avoid { boxes, distance } => {
                let off = boxes.iter().map(|b| (distance - b.distance_xz(&fp)).max(0.0)).fold(0.0, f32::max);
                if off > TOL_M {
                    why = Some(format!("within {:.3} m of something to avoid", distance - off));
                }
                off
            }
            Rel::OnNavmesh { reachable_from } => match probe.navmesh(position) {
                None => {
                    why = Some("no navmesh here".into());
                    1.0
                }
                Some(q) => {
                    nav_point = Some(q);
                    let d = ((q.x - position.x) * (q.x - position.x) + (q.z - position.z) * (q.z - position.z)).sqrt();
                    let mut off = (d - NAV_TOL).max(0.0);
                    if off > 0.0 {
                        why = Some(format!("{d:.3} m off the navmesh"));
                    }
                    if let Some(from) = reachable_from {
                        if score + off * HARD < bound && !probe.reachable(*from, q) {
                            why = Some("not reachable by path".into());
                            off += 1.0;
                        }
                    }
                    off
                }
            },
        };
        let (tol, w) = if r.rel.is_angle() { (TOL_DEG, DEG_WEIGHT) } else { (TOL_M, 1.0) };
        let excess = (off - tol).max(0.0);
        score += excess * w * if r.soft { 1.0 } else { HARD };
        reports.push(RelReport { index: i, rel: r.rel.name(), satisfied: off <= tol, off, why });
    }
    // Small preferences: an even share along a segment, the middle of an area or gap, close.
    if c.len > 0.0 && count > 1 {
        let want = c.len * (k as f32 + 0.5) / count as f32;
        score += (c.s - want).abs() * 0.01;
    } else if c.len > 0.0 {
        score += (c.s - c.len * 0.5).abs() * 0.001;
    } else {
        let (dx, dz) = (c.x - req.start.x, c.z - req.start.z);
        score += (dx * dx + dz * dz).sqrt() * 0.001;
    }
    let _ = nav_point;
    if score >= bound {
        return None;
    }
    // Collisions last (the dearest probe).
    let mut blocked_by = None;
    let mut overlaps_instance = None;
    if req.collide {
        let mut test = fp.shrunk(0.005);
        test.center.y += 0.01;
        if let Some(i) = placed.iter().position(|p| p.footprint.shrunk(0.005).overlaps(&test)) {
            overlaps_instance = Some(i);
            score += COLLISION;
        } else if let Some(id) = probe.blocked(&test) {
            blocked_by = Some(id);
            score += COLLISION;
        }
    }
    let satisfied = blocked_by.is_none() && overlaps_instance.is_none() && reports.iter().zip(rels).all(|(rep, r)| r.soft || rep.satisfied);
    Some(Scored {
        score,
        placement: Placement { position, yaw_deg: yaw, footprint: fp, satisfied, relations: reports, blocked_by, overlaps_instance, rests_on, candidates: 0 },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat floor at y = 0 and some boxes (solid, with ids).
    struct Room {
        boxes: Vec<(u64, Box3)>,
    }

    impl Probe for Room {
        fn ground(&mut self, x: f32, z: f32, from_y: f32, depth: f32, only: Option<u64>) -> Option<(f32, u64)> {
            let mut best: Option<(f32, u64)> = None;
            for (id, b) in &self.boxes {
                if only.is_some_and(|o| o != *id) {
                    continue;
                }
                if b.outside((x, z)) == 0.0 && b.top() <= from_y && b.top() >= from_y - depth && best.is_none_or(|(y, _)| b.top() > y) {
                    best = Some((b.top(), *id));
                }
            }
            if only.is_none() && best.is_none() && from_y >= 0.0 {
                best = Some((0.0, 1));
            }
            best
        }
        fn blocked(&mut self, t: &Box3) -> Option<u64> {
            self.boxes.iter().find(|(_, b)| b.overlaps(t)).map(|(id, _)| *id)
        }
        fn navmesh(&mut self, p: Vec3) -> Option<Vec3> {
            Some(Vec3::new(p.x.clamp(-5.0, 5.0), 0.0, p.z.clamp(-5.0, 5.0)))
        }
        fn reachable(&mut self, _: Vec3, _: Vec3) -> bool {
            true
        }
        fn navmesh_area(&mut self) -> Option<Box3> {
            Some(Box3::aligned(Vec3::new(-5.0, 0.0, -5.0), Vec3::new(5.0, 0.0, 5.0)))
        }
    }

    fn unit() -> (Vec3, Vec3) {
        (Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5))
    }

    fn req(rels: Vec<Rel>) -> Request {
        let (lo, hi) = unit();
        Request {
            lo,
            hi,
            count: 1,
            relations: rels.into_iter().map(|rel| Relation { rel, soft: false }).collect(),
            start: Vec3::ZERO,
            yaw_deg: 0.0,
            front_deg: 0.0,
            settle: true,
            collide: true,
            seed: 0,
        }
    }

    fn table() -> Box3 {
        Box3::aligned(Vec3::new(1.0, 0.0, 1.0), Vec3::new(3.0, 0.8, 2.0))
    }

    #[test]
    fn on_the_table_rests_on_top() {
        let mut room = Room { boxes: vec![(10, table())] };
        let s = solve(&req(vec![Rel::On { target: Some((table(), 10)) }]), &mut room);
        let p = &s.placements[0];
        assert!(p.satisfied, "{p:#?}");
        assert!((p.position.y - 1.3).abs() < 1.0e-4, "the cube's bottom on the 0.8 m top: {:?}", p.position);
        assert_eq!(p.rests_on, Some(10));
        assert!(table().overhang(&p.footprint) <= 1.0e-3);
    }

    #[test]
    fn against_a_wall_faces_away_and_touches() {
        let wall = Box3::aligned(Vec3::new(-4.0, 0.0, -3.1), Vec3::new(4.0, 2.5, -2.9));
        let mut room = Room { boxes: vec![(20, wall)] };
        let s = solve(&req(vec![Rel::Against { target: wall, side: Some((0.0, 1.0)), gap: 0.0 }]), &mut room);
        let p = &s.placements[0];
        assert!(p.satisfied, "{p:#?}");
        // The wall's south face is at z = -2.9; a unit cube's centre half a metre south of it.
        assert!((p.position.z + 2.4).abs() < 0.01, "{:?}", p.position);
        assert!((p.yaw_deg - 0.0).abs() < 1.0e-3, "front (+Z) away from the wall: {}", p.yaw_deg);
        assert!((p.position.y - 0.5).abs() < 1.0e-4, "settled on the floor");
    }

    #[test]
    fn spaced_along_a_line_with_a_hint_when_too_many() {
        let line = Rel::Along { a: Vec3::new(-4.0, 0.0, 0.0), b: Vec3::new(4.0, 0.0, 0.0), offset: 0.0 };
        let mut r = req(vec![line.clone(), Rel::Spacing { min: 1.0, max: None, among: Vec::new() }]);
        r.count = 4;
        let mut room = Room { boxes: Vec::new() };
        let s = solve(&r, &mut room);
        assert_eq!(s.placements.len(), 4);
        assert!(s.placements.iter().all(|p| p.satisfied), "{:#?}", s.placements);
        let mut xs: Vec<f32> = s.placements.iter().map(|p| p.position.x).collect();
        xs.sort_by(f32::total_cmp);
        for w in xs.windows(2) {
            assert!(w[1] - w[0] >= 2.0 - 1.0e-3, "1 m gaps between unit cubes: {xs:?}");
        }
        // Ten don't fit 1 m apart on 8 m.
        r.count = 10;
        let s = solve(&r, &mut room);
        assert!(s.placements.iter().any(|p| !p.satisfied));
        assert!(s.hints.iter().any(|h| h.starts_with("5 of 10 fit")), "{:?}", s.hints);
    }

    #[test]
    fn blocked_spot_gives_the_nearest_free_alternative() {
        // Between two points, but a crate sits in the middle: the solver slides along the segment.
        let crate_box = Box3::aligned(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5));
        let mut room = Room { boxes: vec![(30, crate_box)] };
        let s = solve(&req(vec![Rel::Between { a: Vec3::new(-3.0, 0.0, 0.0), b: Vec3::new(3.0, 0.0, 0.0), t: None }]), &mut room);
        let p = &s.placements[0];
        assert!(p.satisfied, "{p:#?}");
        assert!(p.position.x.abs() >= 1.0 - 1.0e-3, "beside the crate: {:?}", p.position);
        // With t fixed at the middle, it can't be satisfied: the alternative is collision-free and close.
        let s = solve(&req(vec![Rel::Between { a: Vec3::new(-3.0, 0.0, 0.0), b: Vec3::new(3.0, 0.0, 0.0), t: Some(0.5) }]), &mut room);
        let p = &s.placements[0];
        assert!(!p.satisfied);
        assert!(p.blocked_by.is_none(), "the alternative doesn't collide");
        let off = p.relations[0].off;
        assert!(off > 0.9 && off < 1.6, "about a metre off the asked point: {off}");
    }

    #[test]
    fn facing_a_point() {
        let mut room = Room { boxes: Vec::new() };
        let mut r = req(vec![Rel::Near { target: Box3::point(Vec3::new(0.0, 0.0, 0.0)), min: 2.0, max: 3.0 }, Rel::Facing { point: Some(Vec3::ZERO), direction: None }]);
        r.start = Vec3::new(5.0, 0.0, 0.0);
        let s = solve(&r, &mut room);
        let p = &s.placements[0];
        assert!(p.satisfied, "{p:#?}");
        let want = yaw_toward(-p.position.x, -p.position.z);
        assert!(yaw_difference(want, p.yaw_deg) < 0.5);
    }

    #[test]
    fn deterministic() {
        let mut room = Room { boxes: vec![(10, table())] };
        let mut r = req(vec![Rel::Inside { region: Box3::aligned(Vec3::new(-3.0, 0.0, -3.0), Vec3::new(3.0, 3.0, 3.0)), y: None }, Rel::Spacing { min: 0.5, max: None, among: Vec::new() }]);
        r.count = 6;
        let a = solve(&r, &mut room);
        let b = solve(&r, &mut room);
        assert_eq!(a, b);
        assert!(a.placements.iter().all(|p| p.satisfied));
    }
}

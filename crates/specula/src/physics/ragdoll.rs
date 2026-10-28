//! Ragdolls and powered ragdolls (A4; docs/design/ANIMATION.md, "The seam for A4", and
//! docs/design/PHYSICS.md): a rigged model's skeleton becomes capsules joined by joints (P4)
//! whose limits come from its anim graph's `limits`; physics moves them and the pose they take
//! plays through the graph's `{external: true}` nodes, so transitions blend into the ragdoll
//! (a death, a fall) and back out of it (getting up) like any other state.
//!
//! - **Data.** A model entity's `props.ragdoll` turns it on: `{mode: limp | powered | off,
//!   strength, mass, relocate, hit: {bone, impulse, seq}}` (see [`Spec`]). It is world data, so
//!   it is journaled, undoable and claimable for saved entities, and games write it for their
//!   transient presentation (Sepulcra's deaths). `ragdoll_set` writes it (and the graph's
//!   `ragdoll` parameter) in one change.
//! - **Built from the skeleton, deterministically** ([`plan`]): every bone at least
//!   [`MIN_BONE_M`] long (and not the root) gets a capsule along it; the others ride along with
//!   their parent, as the pose had them. Each part is joined to its nearest ancestor part at its
//!   head: a **hinge** for bones whose graph limits give one axis (knees, elbows), else a **ball
//!   joint** twisting about the bone with a swing cone (from the limits when given). Frames are
//!   set from the rest pose, so joint angles are rest-relative exactly like the graph's limits.
//!   Mass (default [`DEFAULT_MASS_KG`] for a figure at scale 1) is shared as [`plan`] says. A
//!   part can be a convex hull fitted to the mesh instead of a capsule ([`fit_hulls`]).
//!   Parts of one ragdoll never collide with each other (one collision group).
//! - **Starting** from the animation at the tick's time (the play evaluated without IK), with
//!   each part's velocity from the pose a tick on, so the body falls on from how it
//!   moved instead of freezing. A `hit` adds an impulse to a bone (each new `seq` once).
//! - **Powered**: the root part (the pelvis) and the legs follow the animation as kinematic bodies (planted where it plants them) and every
//!   joint's motor springs towards the animation's pose, evaluated at physics ticks (pure, so
//!   deterministic), with a torque proportional to what it carries times `strength`: hit
//!   reactions that recover. The engine draws a powered ragdoll's pose as it is (it already
//!   follows the animation); a limp one only through the graph's external nodes.
//! - **Off** (getting up): the ragdoll's last pose stays as the external pose for
//!   [`FROZEN_S`] seconds, so the graph's transition out blends from where the body lies, and
//!   (with `relocate`, the default) the entity first moves under the pelvis so the next
//!   animation starts there.
//! - **Transient entities' ragdolls are cosmetic** ([`specula_physics::BodyDesc::cosmetic`]):
//!   they land on the world's static colliders but never touch its simulated bodies or the
//!   physics hash. Saved entities' ragdolls are simulation (hashed, deterministic in twin runs);
//!   like warm starts their bodies aren't in world files (a loaded world's ragdoll starts again
//!   from the animation), but save games keep them ([`SavedRagdoll`]).
//! - **Checks**: each ragdoll records its pose every tick (up to [`RECORD_FRAMES`]) and
//!   `ragdoll_info` runs the motion checks on it (penetration, where it comes to rest, joint
//!   limits, pops where physics takes over), beside its joints' own errors.

use crate::write_policy::WorldWrite;
use super::joints::SYNTHETIC;
use super::*;
use specula_anim::graph::{AnimGraph, Limit, Value};
use specula_anim::math::{compose, inverse, qarc, qaxis, qconj, qmul, rotation_of, Affine, Transform, Q, QID, V3};
use specula_anim::play::{GraphPlayer, Graphed, Surroundings, Timing};
use specula_anim::{AnimSet, Pose as APose};
use specula_physics::joint::{twist_angle, Drive, JointDesc, JointKind, Motor};
use specula_core::bytes::{ByteReader, ByteWriter, DecodeError};

/// The props key that holds a ragdoll.
pub const PROP: &str = "ragdoll";
/// The graph parameter (bool) `ragdoll_set` drives, when the graph has it.
pub const PARAM: &str = "ragdoll";
/// Bones shorter than this (model metres) ride along with their parent.
pub const MIN_BONE_M: f32 = 0.05;
/// Total mass of a ragdoll at scale 1 unless `mass` says otherwise.
pub const DEFAULT_MASS_KG: f32 = 40.0;
/// Live ragdolls at once (more are listed as not simulated, with why).
pub const MAX_RAGDOLLS: usize = 64;
/// Poses recorded per ragdoll for its motion checks.
pub const RECORD_FRAMES: usize = 150;
/// How far inside a limit opened at the start (see [`specula_physics::joint::open_to`]) the
/// starting pose lies (rad, about 3 degrees).
const START_MARGIN_RAD: f32 = 0.05;
/// Seconds a ragdoll's last pose stays after it is switched off.
pub const FROZEN_S: f64 = 2.0;
/// Thinnest part (model metres), and the damping parts move with (joints lose energy like real
/// ones: a ragdoll comes to rest instead of jittering).
pub const MIN_RADIUS_M: f32 = 0.03;
pub const LINEAR_DAMPING: f32 = 0.2;
pub const ANGULAR_DAMPING: f32 = 2.5;
/// Powered joints spring to the animation this stiffly (Hz). Soft constraints are stiff relative
/// to the two bodies a joint links, not what hangs below them, so a softer drive lets a chain
/// sag under its own weight; strength is the torque cap instead.
pub const DRIVE_HZ: f32 = 60.0;
/// Joint friction torque per kilogram a joint carries (N·m/kg at scale 1).
pub const FRICTION_PER_KG: f32 = 0.6;
/// The collision layer ragdoll parts are on.
pub const LAYER: &str = "ragdoll";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Limp,
    Powered,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Limp => "limp",
            Mode::Powered => "powered",
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        [Mode::Off, Mode::Limp, Mode::Powered].into_iter().find(|m| m.name() == s)
    }
}

/// An impulse on one bone (applied once per `seq`).
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub bone: String,
    /// N·s, world.
    pub impulse: Vec3,
    pub seq: u64,
}

/// `props.ragdoll` as data.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub mode: Mode,
    /// Powered: motor strength (1 = holds the pose firmly; 0..10).
    pub strength: f32,
    /// Total kg (`None`: [`DEFAULT_MASS_KG`] × scale³).
    pub mass: Option<f32>,
    /// Off: move the entity under the pelvis first (default true).
    pub relocate: bool,
    pub hit: Option<Hit>,
    /// Capsule radii by bone name (model metres), over the graph's `ragdoll.radius` (sorted).
    pub radii: Vec<(String, f32)>,
    /// Convex hull parts by bone name: 4 to [`MAX_HULL_POINTS`] points in the bone's rest frame
    /// (model metres: origin at its head, its axes), over the graph's `ragdoll.hull` and over
    /// radii (sorted). `ragdoll_set {fit: "hulls"}` measures them.
    pub hulls: Vec<(String, Vec<V3>)>,
}

/// Points a fitted hull part has at most: moving hulls stay within physics' cap.
pub const MAX_HULL_POINTS: usize = specula_physics::shape::MAX_MOVING_HULL_POINTS;
/// How far a fitted hull stands out from the vertices it wraps (m): the mesh bends between bones
/// where the rigid part doesn't, and contacts give a little on impact.
pub const HULL_SKIN_M: f32 = 0.02;

const KEYS: &[&str] = &["mode", "strength", "mass", "relocate", "hit", "radii", "hulls"];

/// Hull points as data: `[[x, y, z], ...]`, 4 to [`MAX_HULL_POINTS`], each within ±10 m.
/// Whether they make a solid is [`Spec::check_solid`]'s job (checked where the data is written,
/// not each time it is read).
pub fn hull_points(path: &str, v: &Json) -> Result<Vec<V3>, String> {
    let bad = || format!("'{path}' must be 4 to {MAX_HULL_POINTS} points [[x, y, z], ...] in the bone's frame (model metres, within ±10)");
    let pts = v.as_array().filter(|a| (4..=MAX_HULL_POINTS).contains(&a.len())).ok_or_else(bad)?;
    let out: Vec<V3> = pts
        .iter()
        .map(|p| match p.as_array() {
            Some([x, y, z]) => {
                let c = |j: &Json| j.as_f64().filter(|f| f.is_finite() && f.abs() <= 10.0).map(|f| f as f32).ok_or_else(bad);
                Ok([c(x)?, c(y)?, c(z)?])
            }
            _ => Err(bad()),
        })
        .collect::<Result<_, _>>()?;
    Ok(out)
}

/// `props.ragdoll` as written (spawns and updates check it, see [`crate::world::check_spawn`]):
/// well formed, and every hull part a solid.
pub fn check_props(props: &Json) -> Result<(), crate::world::WorldError> {
    let Some(r) = props.get(PROP).filter(|r| !r.is_null()) else { return Ok(()) };
    Spec::from_json(r).and_then(|s| s.check_solid()).map_err(|m| crate::world::WorldError::new("invalid_component", m).with("field", "props.ragdoll"))
}

/// The JSON of hull points, in millimetres.
pub fn hull_json(pts: &[V3]) -> Json {
    let mm = |x: f32| Json::from((f64::from(x) * 1000.0).round() / 1000.0);
    Json::arr(pts.iter().map(|p| Json::arr(p.map(mm))))
}

impl Spec {
    /// Every hull part is a solid: flat or repeated points make no hull, and physics would refuse
    /// the part when the ragdoll starts. Building hulls costs; this runs where `props.ragdoll` is
    /// written ([`check_props`], `ragdoll_set`), never per tick.
    pub fn check_solid(&self) -> Result<(), String> {
        for (bone, pts) in &self.hulls {
            Shape::hull(&pts.iter().map(|p| v3(*p)).collect::<Vec<_>>()).map_err(|e| format!("'ragdoll.hulls.{bone}' is no solid: {}", e.message))?;
        }
        Ok(())
    }

    pub fn from_json(v: &Json) -> Result<Spec, String> {
        let pairs = v.as_object().ok_or("'ragdoll' must be an object: {mode: limp | powered | off, ...}")?;
        if let Some((k, _)) = pairs.iter().find(|(k, _)| !KEYS.contains(&k.as_str())) {
            return Err(format!("'ragdoll.{k}' is not a field; valid: {}", KEYS.join(", ")));
        }
        let mode = match v.get("mode") {
            None | Some(Json::Null) => Mode::Off,
            Some(m) => m.as_str().and_then(Mode::parse).ok_or("'ragdoll.mode' must be limp, powered or off")?,
        };
        let num = |k: &str, lo: f64, hi: f64| -> Result<Option<f32>, String> {
            match v.get(k) {
                None | Some(Json::Null) => Ok(None),
                Some(x) => match x.as_f64() {
                    Some(f) if f.is_finite() && (lo..=hi).contains(&f) => Ok(Some(f as f32)),
                    _ => Err(format!("'ragdoll.{k}' must be a number {lo}..{hi}")),
                },
            }
        };
        let hit = match v.get("hit") {
            None | Some(Json::Null) => None,
            Some(h) => {
                let bone = h.get("bone").and_then(Json::as_str).ok_or("'ragdoll.hit.bone' must be a bone name")?.to_owned();
                let impulse = match h.get("impulse").and_then(Json::as_array) {
                    Some([x, y, z]) => {
                        let c = |j: &Json| j.as_f64().filter(|f| f.is_finite() && f.abs() <= 1.0e5).map(|f| f as f32).ok_or("'ragdoll.hit.impulse' must be [x, y, z] within ±1e5 N·s");
                        Vec3::new(c(x)?, c(y)?, c(z)?)
                    }
                    _ => return Err("'ragdoll.hit.impulse' must be [x, y, z] (N·s, world)".into()),
                };
                Some(Hit { bone, impulse, seq: h.get("seq").and_then(Json::as_u64).unwrap_or(0) })
            }
        };
        let mut radii = Vec::new();
        if let Some(r) = v.get("radii").filter(|r| !r.is_null()) {
            let pairs = r.as_object().ok_or("'ragdoll.radii' must be an object: {bone: metres}")?;
            if pairs.len() > specula_model::rig::MAX_BONES {
                return Err(format!("'ragdoll.radii' lists at most {} bones", specula_model::rig::MAX_BONES));
            }
            for (bone, x) in pairs {
                let x = x.as_f64().filter(|x| x.is_finite() && (0.005..=10.0).contains(x)).ok_or_else(|| format!("'ragdoll.radii.{bone}' must be a radius in model metres, 0.005..10"))?;
                radii.push((bone.clone(), x as f32));
            }
            radii.sort_by(|a, b| a.0.cmp(&b.0));
        }
        let mut hulls = Vec::new();
        if let Some(h) = v.get("hulls").filter(|h| !h.is_null()) {
            let pairs = h.as_object().ok_or("'ragdoll.hulls' must be an object: {bone: [[x, y, z], ...]}")?;
            if pairs.len() > specula_model::rig::MAX_BONES {
                return Err(format!("'ragdoll.hulls' lists at most {} bones", specula_model::rig::MAX_BONES));
            }
            for (bone, pts) in pairs {
                hulls.push((bone.clone(), hull_points(&format!("ragdoll.hulls.{bone}"), pts)?));
            }
            hulls.sort_by(|a, b| a.0.cmp(&b.0));
        }
        Ok(Spec {
            radii,
            hulls,
            mode,
            strength: num("strength", 0.0, 10.0)?.unwrap_or(1.0),
            mass: num("mass", 0.01, 1.0e5)?,
            relocate: match v.get("relocate") {
                None | Some(Json::Null) => true,
                Some(b) => b.as_bool().ok_or("'ragdoll.relocate' must be true or false")?,
            },
            hit,
        })
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![("mode", Json::from(self.mode.name()))];
        if self.strength != 1.0 {
            f.push(("strength", Json::from(self.strength)));
        }
        if let Some(m) = self.mass {
            f.push(("mass", Json::from(m)));
        }
        if !self.relocate {
            f.push(("relocate", Json::from(false)));
        }
        if let Some(h) = &self.hit {
            f.push(("hit", Json::obj([("bone", Json::from(h.bone.as_str())), ("impulse", crate::world::vec3_json(h.impulse)), ("seq", Json::from(h.seq))])));
        }
        if !self.radii.is_empty() {
            f.push(("radii", Json::obj(self.radii.iter().map(|(b, r)| (b.clone(), Json::from((f64::from(*r) * 1000.0).round() / 1000.0))))));
        }
        if !self.hulls.is_empty() {
            f.push(("hulls", Json::obj(self.hulls.iter().map(|(b, p)| (b.clone(), hull_json(p))))));
        }
        Json::obj(f)
    }
}

// ---------------------------------------------------------------------------------------------
// The plan: bodies and joints from a skeleton (pure)

/// A joint between a part and its parent part.
#[derive(Clone, Debug, PartialEq)]
pub struct PartJoint {
    pub kind: JointKind,
    /// In the parent part's body frame and this part's (scaled metres).
    pub frame_parent: Pose,
    pub frame_child: Pose,
    /// Radians: the hinge range, or the ball joint's twist range.
    pub limit: Option<[f32; 2]>,
    /// Ball joints: swing half-angles (radians).
    pub swing: Option<[f32; 2]>,
}

/// One bone's body.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub bone: usize,
    /// The nearest ancestor with a body (`None`: a root part).
    pub parent: Option<usize>,
    pub radius: f32,
    /// Half the capsule's segment (scaled metres).
    pub half: f32,
    /// The capsule (or the hull's centre) in the bone's frame (scaled).
    pub offset: Pose,
    /// A hull part's points about `offset` (scaled; empty: a capsule).
    pub hull: Vec<Vec3>,
    pub mass: f32,
    /// The mass this part's joint carries: the part and every part below it (kg).
    pub carried: f32,
    pub joint: Option<PartJoint>,
    /// A leg (a limb hanging from the root part): a powered ragdoll's legs follow the animation
    /// as kinematic bodies, planted where it plants them, while the upper body reacts.
    pub planted: bool,
}

fn v3(a: V3) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

fn q4(q: Q) -> Quat {
    Quat { x: q[0], y: q[1], z: q[2], w: q[3] }
}

fn qa(q: Quat) -> Q {
    [q.x, q.y, q.z, q.w]
}

/// A bone's segment in its own frame (unscaled): its tail, or the mean of its children's heads
/// when the skeleton has no tails (imported files).
fn segment(set: &AnimSet, rest: &[Affine], b: usize) -> V3 {
    let s = &set.skeleton;
    let inv = inverse(&rest[b]).unwrap_or(Affine::IDENTITY);
    let tail = inv.apply_point(s.tails[b]);
    if specula_anim::math::math3::length(tail) > 1.0e-4 {
        return tail;
    }
    let kids: Vec<V3> = (0..s.len()).filter(|&c| s.parents[c] == Some(b as u16)).map(|c| inv.apply_point(s.heads[c])).collect();
    if kids.is_empty() {
        return [0.0; 3];
    }
    let n = kids.len() as f32;
    kids.iter().fold([0.0; 3], |a, k| [a[0] + k[0] / n, a[1] + k[1] / n, a[2] + k[2] / n])
}

/// The ragdoll of `set`'s skeleton at uniform `scale`, with `mass` kg in all, joints limited by
/// `limits` (a graph's) and capsule radii from `radii` (model metres, by bone; the others from
/// their bone's length), or convex hulls from `hulls` (bone frame, model metres; over a radius):
/// see the module docs.
pub fn plan(set: &AnimSet, limits: &[Limit], radii: &[(usize, f32)], hulls: &[(usize, Vec<V3>)], scale: f32, mass: f32) -> Vec<Part> {
    let s = &set.skeleton;
    let mut rest = Vec::new();
    s.model_space(&s.rest_pose(), &mut rest);
    let segs: Vec<V3> = (0..s.len()).map(|b| segment(set, &rest, b)).collect();
    let len = |b: usize| specula_anim::math::math3::length(segs[b]);
    let has_body: Vec<bool> = (0..s.len()).map(|b| (b != 0 || s.len() == 1) && len(b) >= MIN_BONE_M).collect();
    let body_parent = |b: usize| {
        let mut p = s.parents[b];
        while let Some(i) = p {
            if has_body[i as usize] {
                return Some(i as usize);
            }
            p = s.parents[i as usize];
        }
        None
    };
    let mut parts: Vec<Part> = Vec::new();
    let mut first_root: Option<usize> = None;
    for b in 0..s.len() {
        if !has_body[b] {
            continue;
        }
        let l = len(b) * scale;
        let dir = specula_anim::math::math3::normalize(segs[b]);
        // Limbs: a fifth of their length; a trunk bone (it branches) as wide as a third of how
        // far apart its branches start, up to half its length.
        let kids: Vec<V3> = (0..s.len()).filter(|&c| s.parents[c] == Some(b as u16)).map(|c| s.heads[c]).collect();
        let spread = kids.iter().flat_map(|a| kids.iter().map(move |c| specula_anim::math::math3::length(specula_anim::math::math3::sub(*a, *c)))).fold(0.0f32, f32::max) * scale;
        let trunk = if kids.len() >= 2 { (spread / 3.0).min(0.5 * l.max(0.2 * scale)) } else { 0.0 };
        let radius = match radii.iter().find(|(rb, _)| *rb == b) {
            Some((_, r)) => (r * scale).max(MIN_RADIUS_M * scale),
            None => (0.2 * l).max(trunk).clamp(MIN_RADIUS_M * scale, 0.16 * scale).min(0.45 * l.max(0.2 * scale)),
        };
        // A fitted radius: the bone's whole segment swept by it (that contains the mesh it was
        // measured from); a guessed one: a little shorter, so ends don't scrape the ground.
        let fitted = radii.iter().any(|(rb, _)| *rb == b);
        let half = if fitted { (0.5 * l).max(0.005 * scale) } else { (0.5 * l - 0.5 * radius).max(0.25 * l) };
        let mut offset = Pose { p: v3(segs[b]) * (0.5 * scale), q: q4(qarc([0.0, 1.0, 0.0], dir)) };
        // A hull part: its points about their centre (the body's centre for the start velocity).
        let mut hull = Vec::new();
        if let Some((_, pts)) = hulls.iter().find(|(hb, _)| *hb == b) {
            let n = pts.len().max(1) as f32;
            let c = pts.iter().fold(Vec3::ZERO, |a, p| a + v3(*p) * (scale / n));
            hull = pts.iter().map(|p| v3(*p) * scale - c).collect();
            offset = Pose { p: c, q: Quat::IDENTITY };
        }
        let parent = body_parent(b).or_else(|| first_root.filter(|r| *r != b));
        first_root = first_root.or(Some(b));
        let joint = parent.map(|p| {
            // Rest-relative frames: angles are zero at the rest pose, like the graph's limits.
            let rel = compose(&inverse(&rest[p]).unwrap_or(Affine::IDENTITY), &rest[b]);
            let rel_q = rotation_of(&rel);
            let lim = limits.iter().find(|l| l.bone == b);
            let given: Vec<usize> = lim.map(|l| (0..3).filter(|&a| l.axes[a].is_some()).collect()).unwrap_or_default();
            let deg = |v: f32| v.to_radians();
            let (kind, frame_q, limit, swing) = if given.len() == 1 {
                let a = given[0];
                let [lo, hi] = lim.expect("given").axes[a].expect("given");
                // The frame's x axis along the bone's own axis `a`.
                let f = match a {
                    0 => QID,
                    1 => qaxis([0.0, 0.0, 1.0], 90.0),
                    _ => qaxis([0.0, 1.0, 0.0], -90.0),
                };
                (JointKind::Hinge, f, Some([deg(lo.max(-180.0)), deg(hi.min(180.0))]), None)
            } else {
                // Twist about the bone, swing within a cone.
                let f = qarc([1.0, 0.0, 0.0], dir);
                let along = (0..3).max_by(|&i, &j| dir[i].abs().total_cmp(&dir[j].abs())).unwrap_or(1);
                let twist = lim.and_then(|l| l.axes[along]).map(|[lo, hi]| if dir[along] >= 0.0 { [lo, hi] } else { [-hi, -lo] }).unwrap_or([-30.0, 30.0]);
                let cone = lim
                    .map(|l| (0..3).filter(|&i| i != along).filter_map(|i| l.axes[i]).map(|[lo, hi]| lo.abs().max(hi.abs())).fold(0.0f32, f32::max))
                    .filter(|c| *c > 0.0)
                    .unwrap_or(50.0)
                    .clamp(10.0, 150.0);
                (JointKind::Ball, f, Some([deg(twist[0].max(-180.0)), deg(twist[1].min(180.0))]), Some([deg(cone), deg(cone)]))
            };
            PartJoint {
                kind,
                frame_parent: Pose { p: v3(rel.t) * scale, q: q4(qmul(rel_q, frame_q)) },
                frame_child: Pose { p: Vec3::ZERO, q: q4(frame_q) },
                limit,
                swing,
            }
        });
        parts.push(Part { bone: b, parent, radius, half, offset, hull, mass: 0.0, carried: 0.0, joint, planted: false });
    }
    // Mass: the trunk (the parts from the root part to the last branching part, the pelvis to
    // the chest) weighs four times what a limb of its length does, as bodies do; a quarter of
    // the mass is shared equally so no part is many times lighter than its neighbour (big mass
    // ratios across a joint make chains soft and jittery, and a light trunk can't carry limbs).
    let branches = |b: usize| parts.iter().filter(|p| p.parent == Some(b)).count();
    let mut trunk = vec![false; s.len()];
    for p in &parts {
        if branches(p.bone) >= 2 {
            // It and its ancestors up to the root part.
            let mut at = Some(p.bone);
            while let Some(b) = at {
                trunk[b] = true;
                at = parts.iter().find(|q| q.bone == b).and_then(|q| q.parent);
            }
        }
    }
    // Legs: limbs whose nearest trunk part is the root part.
    let planted: Vec<bool> = parts
        .iter()
        .map(|p| {
            if trunk[p.bone] || p.parent.is_none() {
                return false;
            }
            let mut at = p.parent;
            while let Some(b) = at {
                if trunk[b] {
                    return parts.iter().find(|q| q.bone == b).is_some_and(|q| q.parent.is_none());
                }
                at = parts.iter().find(|q| q.bone == b).and_then(|q| q.parent);
            }
            false
        })
        .collect();
    for (p, l) in parts.iter_mut().zip(planted) {
        p.planted = l;
    }
    let n = parts.len() as f32;
    let weight = |p: &Part| len(p.bone) * if trunk[p.bone] { 4.0 } else { 1.0 };
    let total: f32 = parts.iter().map(weight).sum::<f32>().max(1.0e-6);
    let shares: Vec<f32> = parts.iter().map(|p| 0.75 * weight(p) / total + 0.25 / n).collect();
    for (p, share) in parts.iter_mut().zip(shares) {
        p.mass = mass * share;
    }
    // What each joint carries, once (a powered ragdoll's drives read it every tick).
    let carried: Vec<f32> = parts.iter().map(|p| parts.iter().filter(|q| s.is_under(q.bone, p.bone)).map(|q| q.mass).sum()).collect();
    for (p, c) in parts.iter_mut().zip(carried) {
        p.carried = c;
    }
    parts
}

/// Capsule radii (model metres) for every bone that gets a part, measured from a skinned mesh:
/// for each bone, how far the vertices it drives most lie from its segment (the 95th
/// percentile, so a claw or a spike doesn't fatten a whole limb; the farthest for end parts).
/// For `ragdoll_set {fit}`, which
/// writes them into the entity as data (the mesh build is asynchronous; data isn't).
pub fn fit_radii(set: &AnimSet, mesh: &specula_model::MeshData) -> Vec<(usize, f32)> {
    let s = &set.skeleton;
    let mut rest = Vec::new();
    s.model_space(&s.rest_pose(), &mut rest);
    let mut dists: Vec<Vec<f32>> = vec![Vec::new(); s.len()];
    for (i, p) in mesh.positions.iter().enumerate() {
        let (Some(j), Some(w)) = (mesh.joints.get(i), mesh.weights.get(i)) else { break };
        let k = (0..4).max_by_key(|&q| w[q]).unwrap_or(0);
        let b = j[k] as usize;
        if b >= s.len() {
            continue;
        }
        let head = rest[b].t;
        let tail = rest[b].apply_point(segment(set, &rest, b));
        let ab = specula_anim::math::math3::sub(tail, head);
        let ap = specula_anim::math::math3::sub(*p, head);
        let l2 = specula_anim::math::math3::dot(ab, ab);
        let t = if l2 > 1.0e-12 { (specula_anim::math::math3::dot(ap, ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
        let c = specula_anim::math::math3::add(head, specula_anim::math::math3::scale(ab, t));
        dists[b].push(specula_anim::math::math3::length(specula_anim::math::math3::sub(*p, c)));
    }
    let parts = plan(set, &[], &[], &[], 1.0, 1.0);
    parts
        .iter()
        .filter_map(|part| {
            let d = &mut dists[part.bone];
            if d.len() < 4 {
                return None;
            }
            d.sort_by(f32::total_cmp);
            // Ends (feet, hands, heads: nothing hangs below them) take their farthest vertex: a
            // flat sole's corners would otherwise sink into the ground the capsule rests on.
            let leaf = !parts.iter().any(|q| q.parent == Some(part.bone));
            let r = if leaf { d[d.len() - 1] } else { d[(d.len() * 19 / 20).min(d.len() - 1)] };
            Some((part.bone, ((r * 1000.0).round() / 1000.0).clamp(MIN_RADIUS_M, 1.0)))
        })
        .collect()
}

/// Convex hull parts measured from a skinned mesh, for every bone that gets a part: the
/// vertices each bone drives most, in its rest frame, wrapped by at most [`MAX_HULL_POINTS`] of
/// them. It starts from the vertices reaching furthest along the three axes, then adds the vertex
/// furthest outside the hull so far until none is more than a millimetre out or the next would
/// pass the cap (so what is left out sticks out least). Deterministic: ties go to the first
/// vertex. Bones whose vertices don't make a solid are left out (they keep capsules). For
/// `ragdoll_set {fit: "hulls"}`.
pub fn fit_hulls(set: &AnimSet, mesh: &specula_model::MeshData) -> Vec<(usize, Vec<V3>)> {
    let s = &set.skeleton;
    let mut rest = Vec::new();
    s.model_space(&s.rest_pose(), &mut rest);
    let inv: Vec<Affine> = rest.iter().map(|m| inverse(m).unwrap_or(Affine::IDENTITY)).collect();
    let mut pts: Vec<Vec<Vec3>> = vec![Vec::new(); s.len()];
    for (i, p) in mesh.positions.iter().enumerate() {
        let (Some(j), Some(w)) = (mesh.joints.get(i), mesh.weights.get(i)) else { break };
        let k = (0..4).max_by_key(|&q| w[q]).unwrap_or(0);
        let b = j[k] as usize;
        if b < s.len() {
            // Millimetres, as the data will hold them.
            pts[b].push(v3(inv[b].apply_point(*p).map(|x| (x * 1000.0).round() / 1000.0)));
        }
    }
    let parts = plan(set, &[], &[], &[], 1.0, 1.0);
    parts
        .iter()
        .filter_map(|part| {
            let v = &pts[part.bone];
            if v.len() < 8 {
                return None;
            }
            // The extremes along each axis, then the vertex sticking out furthest, one by one.
            let mut chosen: Vec<Vec3> = Vec::with_capacity(MAX_HULL_POINTS);
            for axis in 0..3 {
                let along = |p: &Vec3| [p.x, p.y, p.z][axis];
                for far in [false, true] {
                    let pick = v.iter().enumerate().fold(None::<(f32, usize)>, |best, (i, p)| {
                        let x = if far { along(p) } else { -along(p) };
                        if best.is_none_or(|(bx, _)| x > bx) {
                            Some((x, i))
                        } else {
                            best
                        }
                    });
                    if let Some((_, i)) = pick {
                        if !chosen.contains(&v[i]) {
                            chosen.push(v[i]);
                        }
                    }
                }
            }
            let mut hull = Shape::hull(&chosen).ok()?;
            loop {
                let h = hull.polytope()?;
                let out = |p: &Vec3| h.normals.iter().zip(&h.offsets).map(|(n, o)| n.dot(*p) - o).fold(f32::MIN, f32::max);
                let worst = v.iter().enumerate().fold(None::<(f32, usize)>, |best, (i, p)| {
                    let d = out(p);
                    if best.is_none_or(|(bd, _)| d > bd) {
                        Some((d, i))
                    } else {
                        best
                    }
                });
                let Some((_, i)) = worst.filter(|(d, _)| *d > 1.0e-3) else { break };
                let mut next: Vec<Vec3> = h.points.clone();
                next.push(v[i]);
                match Shape::hull(&next) {
                    Ok(n) if n.polytope().is_some_and(|q| q.points.len() <= MAX_HULL_POINTS) => hull = n,
                    _ => break,
                }
            }
            // A skin: the mesh bends between bones (blended weights) where the part doesn't, and
            // contacts give a little on impact.
            let h = hull.polytope()?;
            let pts: Vec<V3> = h
                .points
                .iter()
                .map(|p| {
                    let out = specula_physics::math::normalize_or(*p - h.centroid, Vec3::Y);
                    let q = *p + out * HULL_SKIN_M;
                    [q.x, q.y, q.z].map(|x| (x * 1000.0).round() / 1000.0)
                })
                .collect();
            (pts.len() >= 4).then_some((part.bone, pts))
        })
        .collect()
}

/// A ragdoll part's body (and joint) key: [`SYNTHETIC`], the entity, the bone.
pub fn key(entity: u64, bone: usize) -> u64 {
    SYNTHETIC | (entity << 8) | bone as u64
}

/// The entity a physics key belongs to (a ragdoll part's: its ragdoll's entity).
pub fn owner_of(k: u64) -> u64 {
    unkey(k).map_or(k, |(e, _)| e)
}

/// `(entity, bone)` of a ragdoll key.
pub fn unkey(k: u64) -> Option<(u64, usize)> {
    (k >= SYNTHETIC).then_some(((k & !SYNTHETIC) >> 8, (k & 0xff) as usize))
}

// ---------------------------------------------------------------------------------------------
// Live ragdolls in the engine

/// A ragdoll the engine is simulating (or holding after it was switched off).
pub struct Live {
    pub entity: u64,
    pub mode: Mode,
    pub cosmetic: bool,
    pub set: Arc<AnimSet>,
    pub parts: Vec<Part>,
    /// The pose it started from (the bones without bodies keep their place in it).
    pub start: Vec<Transform>,
    pub scale: f32,
    pub strength: f32,
    /// The last hit applied.
    pub hit_seq: u64,
    /// World time it was switched off (it then only holds its last pose).
    pub off_since: Option<f64>,
    pub started_tick: u64,
    /// Poses recorded each tick from the start (model space, for the motion checks).
    pub frames: Vec<APose>,
    pub frame_dt: f32,
    /// The latest pose.
    pub pose: Arc<APose>,
}

/// An entity's `props.ragdoll` as parsed: (entity version, content hash, the spec or why not).
type Parsed = (u64, u64, Result<Arc<Spec>, String>);

/// The engine's ragdolls.
#[derive(Default)]
pub struct Ragdolls {
    pub live: BTreeMap<u64, Live>,
    /// Saved entities with `props.ragdoll` (kept from the world's change log; transient ones are
    /// found each tick).
    pub persistent: BTreeSet<u64>,
    player: GraphPlayer,
    /// Each ragdoll entity's `props.ragdoll` as parsed, by the entity's version and content hash
    /// when it was parsed (read again only when the entity changes; the hash too, because a
    /// force-undo that restores a deleted entity can give it a version it had before).
    parsed: BTreeMap<u64, Parsed>,
    /// How many times a `props.ragdoll` was parsed (tests guard the per-tick cost with it).
    pub parses: u64,
}

impl Ragdolls {
    /// The collision group of entity `id`'s ragdoll (0: none).
    pub fn group_of(&self, id: u64) -> u64 {
        if self.live.get(&id).is_some_and(|l| l.off_since.is_none()) {
            id + 1
        } else {
            0
        }
    }
}

/// The local pose `e` plays at world time `t` (no IK: the simulation's pose), or the rest pose.
/// `base_only` leaves the graph's other layers in their entry states: a takeover moves on at the
/// base motion, not at a reaction layer just set off (a killing blow's flinch, whose attack would
/// fling the arms at 15 m/s).
fn pose_at(player: &mut GraphPlayer, set: &AnimSet, e: &world::Entity, t: f64, ragdoll_off: bool, base_only: bool) -> APose {
    let skel = &set.skeleton;
    let Some(a) = &e.anim else { return skel.rest_pose() };
    if let Some(gp) = &a.graph {
        if let Some(graph) = set.graph(&gp.graph) {
            let mut play = (**gp).clone();
            if ragdoll_off && graph.param(PARAM).is_some() {
                // A powered ragdoll follows what the graph plays without its ragdoll state.
                play.set_param(PARAM, Value::Bool(false));
            }
            if base_only {
                // A layer not listed starts in its entry state at the anchor.
                play.layers.retain(|l| l.layer == "base");
            }
            let g = Graphed { set, graph, scale: crate::anim::entity_scale(e) };
            return player.evaluate(g, &play, Timing { anchor: a.start.unwrap_or(0.0), speed: a.speed }, t, &Surroundings::default()).clone();
        }
        return skel.rest_pose();
    }
    match set.id(&a.clip) {
        Some(id) => {
            let clip = set.clip(id);
            let mut p = skel.rest_pose();
            specula_anim::blend::sample(skel, clip, set.track_bones(id), clip.local_time(a.clip_time(t)), specula_anim::blend::Base::Rest, &mut p);
            p
        }
        None => skel.rest_pose(),
    }
}

/// Entity `e`'s ragdoll as `spec` and its graph describe it: its anim set, the parts, its
/// uniform scale and its world pose. Pure: the same entity and spec always give the same parts.
fn plan_for(world: &World, e: &world::Entity, spec: &Spec) -> Result<(Arc<AnimSet>, Vec<Part>, f32, Pose), String> {
    if e.shape != world::Shape::Model {
        return Err("only model entities with a rig have ragdolls".into());
    }
    let set = crate::anim::write_set_of(world, e).ok_or("its model has no rig (or its recipe or anims file doesn't load)")?;
    if set.skeleton.len() < 2 {
        return Err("its rig has one bone: nothing to joint".into());
    }
    let (pose_e, scale3) = world_pose(world, e);
    let scale = (scale3.x + scale3.y + scale3.z) / 3.0;
    let limits: Vec<Limit> = match graph_of(&set, e) {
        Some(g) => g.limits.clone(),
        None => specula_anim::checks::limits_of(&set),
    };
    let mass = spec.mass.unwrap_or(DEFAULT_MASS_KG * scale * scale * scale);
    // Sizes: the entity's own (props) over its graph's, bone by bone; a hull over a radius from
    // the same place.
    let mut radii: Vec<(usize, f32)> = graph_of(&set, e).map(|g| g.ragdoll_radius.clone()).unwrap_or_default();
    let mut hulls: Vec<(usize, Vec<V3>)> = graph_of(&set, e).map(|g| g.ragdoll_hull.clone()).unwrap_or_default();
    for (name, r) in &spec.radii {
        let b = set.skeleton.find(name).ok_or_else(|| format!("'ragdoll.radii' names no bone '{name}'"))?;
        radii.retain(|(x, _)| *x != b);
        radii.push((b, *r));
        if !spec.hulls.iter().any(|(h, _)| h == name) {
            hulls.retain(|(x, _)| *x != b);
        }
    }
    for (name, pts) in &spec.hulls {
        let b = set.skeleton.find(name).ok_or_else(|| format!("'ragdoll.hulls' names no bone '{name}'"))?;
        hulls.retain(|(x, _)| *x != b);
        hulls.push((b, pts.clone()));
    }
    let parts = plan(&set, &limits, &radii, &hulls, scale, mass);
    if parts.is_empty() {
        return Err(format!("no bone is at least {MIN_BONE_M} m long"));
    }
    Ok((set, parts, scale, pose_e))
}

/// A part's body at `pose` (no velocity): kinematic for a powered ragdoll's root and legs.
fn part_body(set: &AnimSet, p: &Part, powered: bool, pose: Pose, layer: u64, id: u64, cosmetic: bool) -> Result<BodyDesc, String> {
    let shape = if p.hull.is_empty() { Shape::capsule(p.radius, p.half) } else { Shape::hull(&p.hull).map_err(|e| format!("{}'s hull: {}", set.skeleton.names[p.bone], e.message))? };
    let collider = ColliderDesc { layer, offset: p.offset, friction: 0.8, ..ColliderDesc::new(shape) };
    let kind = if powered && (p.parent.is_none() || p.planted) { specula_physics::BodyKind::Kinematic } else { specula_physics::BodyKind::Dynamic };
    let mut d = BodyDesc::new(kind, pose, Some(collider));
    d.mass = p.mass;
    d.linear_damping = LINEAR_DAMPING;
    d.angular_damping = ANGULAR_DAMPING;
    d.group = id + 1;
    d.cosmetic = cosmetic;
    Ok(d)
}

/// Save games (docs/design/SAVES.md): a ragdoll as the physics section holds it. Saved
/// entities' ragdolls only (transient entities aren't saved; their cosmetic ragdolls go with
/// them). The parts are planned again from the loaded entity (the plan is pure); what the world
/// can't say is kept: the joints as they were (limits opened at the start, the drives' last
/// targets), the pose and the bookkeeping. The bodies' motion and the joints' warm starts are
/// in the space's own snapshot. The motion checks' recording starts again from the load.
pub struct SavedRagdoll {
    entity: u64,
    mode: Mode,
    strength: f32,
    hit_seq: u64,
    off_since: Option<f64>,
    started_tick: u64,
    frame_dt: f32,
    start: Vec<Transform>,
    pose: Vec<Transform>,
    bones: Vec<usize>,
    joints: Vec<(usize, JointDesc)>,
}

fn w_transform(w: &mut ByteWriter, t: &Transform) {
    t.t.iter().chain(&t.r).chain(&t.s).for_each(|x| w.f32(*x));
}

fn r_transform(r: &mut ByteReader<'_>) -> Result<Transform, DecodeError> {
    let mut v = [0.0f32; 10];
    for x in &mut v {
        *x = r.f32("ragdolls.pose")?;
        if !x.is_finite() {
            return Err(r.error("invalid", "ragdolls.pose", "a pose number isn't finite"));
        }
    }
    Ok(Transform { t: [v[0], v[1], v[2]], r: [v[3], v[4], v[5], v[6]], s: [v[7], v[8], v[9]] })
}

fn ragdoll_bad(e: DecodeError) -> WorldError {
    WorldError::new("bad_save", format!("the save's ragdolls are damaged: {e}")).with("section", "physics").with("offset", e.offset).with("field", e.field.as_str())
}

impl Physics {
    /// Writes the saved entities' live ragdolls (see [`SavedRagdoll`]).
    pub(crate) fn save_ragdolls(&self, w: &mut ByteWriter) {
        let live: Vec<&Live> = self.ragdolls.live.values().filter(|l| !l.cosmetic).collect();
        w.len_u32(live.len());
        for l in live {
            w.u64(l.entity);
            w.u8(match l.mode {
                Mode::Off => 0,
                Mode::Limp => 1,
                Mode::Powered => 2,
            });
            w.f32(l.strength);
            w.u64(l.hit_seq);
            w.bool(l.off_since.is_some());
            if let Some(s) = l.off_since {
                w.f64(s);
            }
            w.u64(l.started_tick);
            w.f32(l.frame_dt);
            w.len_u32(l.start.len());
            l.start.iter().for_each(|t| w_transform(w, t));
            w.len_u32(l.pose.local.len());
            l.pose.local.iter().for_each(|t| w_transform(w, t));
            w.len_u32(l.parts.len());
            l.parts.iter().for_each(|p| w.u32(p.bone as u32));
            let joints: Vec<(usize, &JointDesc)> = if l.off_since.is_some() {
                Vec::new()
            } else {
                l.parts.iter().filter_map(|p| self.space.joint(key(l.entity, p.bone)).map(|j| (p.bone, j.desc))).collect()
            };
            w.len_u32(joints.len());
            for (b, d) in joints {
                w.u32(b as u32);
                specula_physics::write_joint_desc(w, d);
            }
        }
    }

    /// Reads [`Physics::save_ragdolls`]'s bytes (nothing changes yet).
    pub(crate) fn read_ragdolls(r: &mut ByteReader<'_>) -> Result<Vec<SavedRagdoll>, WorldError> {
        let bones_max = specula_model::rig::MAX_BONES;
        let n = r.count("ragdolls", MAX_RAGDOLLS, 40).map_err(ragdoll_bad)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let f = "ragdolls";
            let entity = r.u64(f).map_err(ragdoll_bad)?;
            let mode = match r.u8(f).map_err(ragdoll_bad)? {
                0 => Mode::Off,
                1 => Mode::Limp,
                2 => Mode::Powered,
                _ => return Err(ragdoll_bad(r.error("invalid", f, "unknown ragdoll mode"))),
            };
            let strength = r.f32(f).map_err(ragdoll_bad)?;
            let hit_seq = r.u64(f).map_err(ragdoll_bad)?;
            let off_since = if r.bool(f).map_err(ragdoll_bad)? { Some(r.f64(f).map_err(ragdoll_bad)?) } else { None };
            let started_tick = r.u64(f).map_err(ragdoll_bad)?;
            let frame_dt = r.f32(f).map_err(ragdoll_bad)?;
            if !(0.0..=10.0).contains(&strength) || !(frame_dt.is_finite() && frame_dt > 0.0 && frame_dt <= 1.0) || off_since.is_some_and(|s| !s.is_finite()) {
                return Err(ragdoll_bad(r.error("invalid", f, "a ragdoll's strength must be 0..10, its frame time 0..1 s, its times finite")));
            }
            let k = r.count(f, bones_max, 40).map_err(ragdoll_bad)?;
            let start = (0..k).map(|_| r_transform(r)).collect::<Result<Vec<_>, _>>().map_err(ragdoll_bad)?;
            let k = r.count(f, bones_max, 40).map_err(ragdoll_bad)?;
            let pose = (0..k).map(|_| r_transform(r)).collect::<Result<Vec<_>, _>>().map_err(ragdoll_bad)?;
            let k = r.count(f, bones_max, 4).map_err(ragdoll_bad)?;
            let bones = (0..k).map(|_| r.u32(f).map(|b| b as usize)).collect::<Result<Vec<_>, _>>().map_err(ragdoll_bad)?;
            let k = r.count(f, bones_max, 60).map_err(ragdoll_bad)?;
            let mut joints = Vec::with_capacity(k);
            for _ in 0..k {
                let b = r.u32(f).map_err(ragdoll_bad)? as usize;
                let d = specula_physics::read_joint_desc(r).map_err(|e| WorldError::new("bad_save", format!("a saved ragdoll joint: {}", e.message)).with("section", "physics"))?;
                joints.push((b, d));
            }
            out.push(SavedRagdoll { entity, mode, strength, hit_seq, off_since, started_tick, frame_dt, start, pose, bones, joints });
        }
        Ok(out)
    }

    /// Rebuilds saved ragdolls in a reset bridge (before the world's bodies are added): each
    /// planned again from its entity, its parts' bodies put where its saved pose has them (the
    /// space snapshot then sets their exact motion) and its joints as they were.
    pub(crate) fn rebuild_ragdolls(&mut self, world: &World, saved: Vec<SavedRagdoll>) -> Result<(), WorldError> {
        let mismatch = |id: u64, m: String| WorldError::new("save_mismatch", m).with("section", "physics").with("id", id);
        for s in saved {
            let id = s.entity;
            let e = world.get(id).filter(|e| !e.transient).ok_or_else(|| mismatch(id, format!("the physics section has a ragdoll for entity {id}, which the save's world doesn't have")))?;
            let spec = e.props.get(PROP).ok_or_else(|| mismatch(id, format!("{} had a ragdoll but has no props.ragdoll", world.path_of(id))))?;
            let spec = Spec::from_json(spec).map_err(|m| mismatch(id, format!("{}'s props.ragdoll: {m}", world.path_of(id))))?;
            let (set, parts, scale, pose_e) = plan_for(world, e, &spec).map_err(|m| mismatch(id, format!("{}'s ragdoll: {m}", world.path_of(id))))?;
            let bones: Vec<usize> = parts.iter().map(|p| p.bone).collect();
            if bones != s.bones || s.start.len() != set.skeleton.len() || s.pose.len() != set.skeleton.len() {
                return Err(mismatch(id, format!("{}'s ragdoll has other parts than when it was saved", world.path_of(id))));
            }
            let pose = APose { local: s.pose };
            if s.off_since.is_none() {
                let layer = self.layer_bit(LAYER).map_err(|m| mismatch(id, m))?;
                let mut m = Vec::new();
                set.skeleton.model_space(&pose, &mut m);
                let mut bodies = Vec::with_capacity(parts.len());
                for p in &parts {
                    let d = part_body(&set, p, s.mode == Mode::Powered, bone_world(&pose_e, scale, &m[p.bone]), layer, id, false).map_err(|m| mismatch(id, m))?;
                    bodies.push((key(id, p.bone), d));
                }
                let mut joints = Vec::with_capacity(s.joints.len());
                for (b, d) in s.joints {
                    // A saved joint ties exactly what the plan ties: this part to its parent part
                    // (never a body the save names; a crafted save can't reach the world).
                    let part = parts.iter().find(|p| p.bone == b && p.joint.is_some());
                    let ties = part.and_then(|p| p.parent).is_some_and(|parent| d.a == Some(key(id, parent)) && d.b == Some(key(id, b)));
                    if !ties {
                        return Err(mismatch(id, format!("a saved joint of {}'s ragdoll doesn't tie the parts its plan does", world.path_of(id))));
                    }
                    joints.push((key(id, b), d));
                }
                self.insert_all(bodies, joints).map_err(|m| mismatch(id, m))?;
            }
            let pose = Arc::new(pose);
            self.ragdolls.live.insert(
                id,
                Live {
                    entity: id,
                    mode: s.mode,
                    cosmetic: false,
                    set,
                    parts,
                    start: s.start,
                    scale,
                    strength: s.strength,
                    hit_seq: s.hit_seq,
                    off_since: s.off_since,
                    started_tick: s.started_tick,
                    frames: vec![(*pose).clone()],
                    frame_dt: s.frame_dt,
                    pose,
                },
            );
        }
        Ok(())
    }
}

fn graph_of<'a>(set: &'a AnimSet, e: &world::Entity) -> Option<&'a AnimGraph> {
    e.anim.as_ref().and_then(|a| a.graph.as_ref()).and_then(|g| set.graph(&g.graph))
}

/// World pose of a bone at model transform `m` for an entity at `pose` with uniform `scale`.
fn bone_world(pose: &Pose, scale: f32, m: &Affine) -> Pose {
    Pose { p: pose.point(v3(m.t) * scale), q: (pose.q * q4(rotation_of(m))).normalize() }
}

/// The world-space twist (rotation vector) taking `from` to `to`, per second.
fn spin(from: Quat, to: Quat, inv_dt: f32) -> Vec3 {
    specula_physics::joint::rotation_vector((to * from.conjugate()).normalize()) * inv_dt
}

impl Physics {
    /// Notes whether saved entity `e` has a ragdoll (from the world's change log).
    pub(super) fn note_ragdoll(&mut self, e: Option<&world::Entity>, id: u64) {
        match e {
            Some(e) if !e.transient && e.props.get(PROP).is_some() => {
                self.ragdolls.persistent.insert(id);
            }
            _ => {
                self.ragdolls.persistent.remove(&id);
            }
        }
    }

    /// Starts, switches, hits, drives and ends ragdolls (before the step).
    pub(super) fn ragdolls_before_step(&mut self, world: &mut dyn WorldWrite, tick: u64, dt: f32) {
        if self.ragdolls.live.is_empty() && self.ragdolls.persistent.is_empty() && world.transient_entities().next().is_none() {
            return;
        }
        let t = world.world_time();
        // A ragdoll restored from a save shows its pose again (the world's external poses are
        // presentation, not saved).
        for (id, l) in &self.ragdolls.live {
            if world.external_pose(*id).is_none() && world.get(*id).is_some() {
                world.set_external_pose(*id, Some(world::ExternalPose { pose: Arc::clone(&l.pose), direct: l.off_since.is_none() && l.mode == Mode::Powered }));
            }
        }
        // Who has a ragdoll now (switched off and not live: nothing to do, so not listed).
        let mut wanted: Vec<(u64, Arc<Spec>)> = Vec::new();
        let transient = world.transient_entities().filter(|e| e.props.get(PROP).is_some()).map(|e| e.id);
        let ids: Vec<u64> = self.ragdolls.persistent.iter().copied().chain(transient).collect();
        // Parsed specs of entities no longer listed go.
        if self.ragdolls.parsed.len() > ids.len() {
            let listed: BTreeSet<u64> = ids.iter().copied().collect();
            self.ragdolls.parsed.retain(|k, _| listed.contains(k));
        }
        for id in ids {
            let Some(e) = world.get(id) else { continue };
            let content = e.content_hash();
            let fresh = self.ragdolls.parsed.get(&id).is_some_and(|(v, h, _)| *v == e.version && *h == content);
            if !fresh {
                self.ragdolls.parses += 1;
                self.ragdolls.parsed.insert(id, (e.version, content, Spec::from_json(e.props.get(PROP).expect("listed")).map(Arc::new)));
            }
            match &self.ragdolls.parsed[&id].2 {
                Ok(s) if s.mode == Mode::Off && !self.ragdolls.live.contains_key(&id) => {}
                Ok(s) => wanted.push((id, Arc::clone(s))),
                Err(m) => {
                    let m = format!("ragdoll: {m}");
                    self.note_problem_only(world, id, &m);
                }
            }
        }
        // Gone, or no longer a ragdoll: drop it.
        let still: BTreeSet<u64> = wanted.iter().map(|(id, _)| *id).collect();
        let gone: Vec<u64> = self.ragdolls.live.keys().copied().filter(|id| !still.contains(id)).collect();
        for id in gone {
            self.drop_ragdoll(world, id);
        }
        for (id, spec) in wanted {
            let active = self.ragdolls.live.get(&id).is_some_and(|l| l.off_since.is_none());
            match (active, spec.mode) {
                (false, Mode::Off) => {
                    // Holding a switched-off ragdoll's pose for a while, then letting go.
                    if self.ragdolls.live.get(&id).and_then(|l| l.off_since).is_some_and(|s| t - s > FROZEN_S) {
                        self.ragdolls.live.remove(&id);
                        world.set_external_pose(id, None);
                    }
                }
                (false, _) => {
                    if self.ragdolls.live.get(&id).is_some_and(|l| l.off_since.is_none()) {
                        continue;
                    }
                    let n = self.ragdolls.live.values().filter(|l| l.off_since.is_none()).count();
                    if n >= MAX_RAGDOLLS {
                        self.note_problem_only(world, id, &format!("ragdoll: at most {MAX_RAGDOLLS} ragdolls move at once"));
                        continue;
                    }
                    if let Err(m) = self.start_ragdoll(world, id, &spec, t, dt, tick) {
                        self.note_problem_only(world, id, &format!("ragdoll: {m}"));
                    } else {
                        self.problems.remove(&id);
                    }
                }
                (true, Mode::Off) => self.stop_ragdoll(world, id, spec.relocate, t),
                (true, mode) => {
                    let live = self.ragdolls.live.get_mut(&id).expect("active");
                    let changed = live.mode != mode || live.strength != spec.strength;
                    live.mode = mode;
                    live.strength = spec.strength;
                    if changed {
                        self.switch_ragdoll(id);
                    }
                }
            }
            // A new hit.
            if let (Some(h), Some(live)) = (&spec.hit, self.ragdolls.live.get_mut(&id)) {
                if live.off_since.is_none() && h.seq != live.hit_seq {
                    live.hit_seq = h.seq;
                    let skel = &live.set.skeleton;
                    let mut bone = skel.find(&h.bone);
                    // A bone without a body: its nearest ancestor with one takes the hit.
                    while let Some(b) = bone.filter(|b| !live.parts.iter().any(|p| p.bone == *b)) {
                        bone = skel.parents[b].map(usize::from);
                    }
                    if let Some(b) = bone.or(live.parts.first().map(|p| p.bone)) {
                        let k = key(id, b);
                        if let Some(c) = self.space.body(k).map(|v| v.center) {
                            self.space.apply_impulse(k, h.impulse, c);
                        }
                    }
                }
            }
            // Powered: targets from the animation at the end of this step.
            if self.ragdolls.live.get(&id).is_some_and(|l| l.off_since.is_none() && l.mode == Mode::Powered) {
                self.drive_ragdoll(world, id, t + f64::from(dt));
            }
        }
    }

    fn note_problem_only(&mut self, world: &World, id: u64, error: &str) {
        if self.problems.len() < MAX_PROBLEMS || self.problems.contains_key(&id) {
            self.problems.insert(id, Json::obj([("id", Json::from(id)), ("path", Json::from(world.path_of(id))), ("error", Json::from(error))]));
        }
    }

    /// Inserts a ragdoll's bodies and joints all or nothing: on any refusal (a budget, a shape)
    /// what already went in comes out again, so no part is left simulated without its ragdoll.
    fn insert_all(&mut self, bodies: Vec<(u64, BodyDesc)>, joints: Vec<(u64, JointDesc)>) -> Result<(), String> {
        let (mut in_b, mut in_j) = (Vec::with_capacity(bodies.len()), Vec::with_capacity(joints.len()));
        let mut failed = None;
        for (k, d) in bodies {
            match self.space.insert(k, d) {
                Ok(()) => in_b.push(k),
                Err(e) => {
                    failed = Some(e.message);
                    break;
                }
            }
        }
        if failed.is_none() {
            for (k, d) in joints {
                match self.space.insert_joint(k, d) {
                    Ok(()) => in_j.push(k),
                    Err(e) => {
                        failed = Some(e.message);
                        break;
                    }
                }
            }
        }
        match failed {
            None => Ok(()),
            Some(m) => {
                for k in in_j {
                    self.space.remove_joint(k);
                }
                for k in in_b {
                    self.space.remove(k);
                }
                Err(m)
            }
        }
    }

    fn start_ragdoll(&mut self, world: &mut dyn WorldWrite, id: u64, spec: &Spec, t: f64, dt: f32, tick: u64) -> Result<(), String> {
        let e = world.get(id).ok_or("gone")?;
        let (set, parts, scale, pose_e) = plan_for(world, e, spec)?;
        self.start_planned(world, id, spec, set, parts, scale, pose_e, t, dt, tick)
    }

    #[allow(clippy::too_many_arguments)]
    fn start_planned(&mut self, world: &mut dyn WorldWrite, id: u64, spec: &Spec, set: Arc<AnimSet>, parts: Vec<Part>, scale: f32, pose_e: Pose, t: f64, dt: f32, tick: u64) -> Result<(), String> {
        let e = world.get(id).ok_or("gone")?;
        let powered = spec.mode == Mode::Powered;
        // What it was playing (without its ragdoll state, which plays this very pose), now and
        // a tick on (a game usually switches the ragdoll on in the same write that settles its
        // play, so the animation is only known from now on).
        let now = pose_at(&mut self.ragdolls.player, &set, e, t, true, true);
        let next = pose_at(&mut self.ragdolls.player, &set, e, t + f64::from(dt), true, true);
        let (mut m_now, mut m_next) = (Vec::new(), Vec::new());
        set.skeleton.model_space(&now, &mut m_now);
        set.skeleton.model_space(&next, &mut m_next);
        let cosmetic = e.transient;
        let layer = self.layer_bit(LAYER)?;
        let inv_dt = 1.0 / dt.max(1.0e-6);
        let mut starts: Vec<(usize, Pose)> = Vec::with_capacity(parts.len());
        let mut bodies: Vec<(u64, BodyDesc)> = Vec::with_capacity(parts.len());
        let mut joints: Vec<(u64, JointDesc)> = Vec::with_capacity(parts.len());
        for p in &parts {
            let w = bone_world(&pose_e, scale, &m_now[p.bone]);
            starts.push((p.bone, w));
            let w1 = bone_world(&pose_e, scale, &m_next[p.bone]);
            let mut d = part_body(&set, p, powered, w, layer, id, cosmetic)?;
            // Moving on as the animation was (the centre's velocity from the bone's motion).
            let c_now = w.point(p.offset.p);
            let c_next = w1.point(p.offset.p);
            if d.kind == specula_physics::BodyKind::Dynamic {
                d.velocity = (c_next - c_now) * inv_dt;
                d.angular_velocity = spin(w.q, w1.q, inv_dt);
                let vmax = 20.0;
                if d.velocity.length() > vmax {
                    d.velocity = d.velocity * (vmax / d.velocity.length());
                }
                if d.angular_velocity.length() > 30.0 {
                    d.angular_velocity = d.angular_velocity * (30.0 / d.angular_velocity.length());
                }
            }
            bodies.push((key(id, p.bone), d));
        }
        for p in &parts {
            let (Some(j), Some(parent)) = (&p.joint, p.parent) else { continue };
            let mut jd = JointDesc::new(j.kind, Some(key(id, parent)), Some(key(id, p.bone)), j.frame_parent, j.frame_child);
            jd.limit = j.limit;
            jd.swing = j.swing;
            // A pose already past a limit (an animation reaching further than the rest-relative
            // limits) opens it to there: pushed back inside, the part would be flung.
            let start = |b: usize| starts.iter().find(|(x, _)| *x == b).map(|(_, w)| *w);
            if let (Some(pa), Some(pb)) = (start(parent), start(p.bone)) {
                specula_physics::joint::open_to(&mut jd, &pa, &pb, START_MARGIN_RAD);
            }
            // Joint friction in proportion to what the joint carries.
            jd.friction = FRICTION_PER_KG * p.carried * scale;
            joints.push((key(id, p.bone), jd));
        }
        // Everything built and checked first; then all of it goes in, or none.
        self.insert_all(bodies, joints)?;
        // The entity's own collider (if any) stops colliding with its parts.
        self.space.set_group(id, id + 1);
        let hit_seq = spec.hit.as_ref().map_or(0, |h| h.seq.wrapping_sub(1));
        let frame_dt = dt;
        let start = now.local.clone();
        let pose = Arc::new(now);
        self.ragdolls.live.insert(
            id,
            Live {
                entity: id,
                mode: spec.mode,
                cosmetic,
                set,
                parts,
                start,
                scale,
                strength: spec.strength,
                hit_seq,
                off_since: None,
                started_tick: tick,
                frames: vec![(*pose).clone()],
                frame_dt,
                pose: Arc::clone(&pose),
            },
        );
        world.set_external_pose(id, Some(world::ExternalPose { pose, direct: powered }));
        if powered {
            self.switch_ragdoll(id);
        }
        Ok(())
    }

    /// Limp ↔ powered: the root part's kind and the joints' motors.
    fn switch_ragdoll(&mut self, id: u64) {
        let Some(live) = self.ragdolls.live.get(&id) else { return };
        let powered = live.mode == Mode::Powered;
        let roots: Vec<usize> = live.parts.iter().filter(|p| p.parent.is_none() || p.planted).map(|p| p.bone).collect();
        let joints: Vec<usize> = live.parts.iter().filter(|p| p.joint.is_some()).map(|p| p.bone).collect();
        for b in roots {
            let k = key(id, b);
            let Some(v) = self.space.body(k) else { continue };
            let want = if powered { specula_physics::BodyKind::Kinematic } else { specula_physics::BodyKind::Dynamic };
            if v.kind != want {
                let (pose, vel, ang) = (v.pose, v.velocity, v.angular_velocity);
                let Some(c) = v.collider.cloned() else { continue };
                let part = &live.parts.iter().find(|p| p.bone == b).expect("listed").clone();
                let mut d = BodyDesc::new(want, pose, Some(c));
                d.mass = part.mass;
                (d.linear_damping, d.angular_damping, d.group, d.cosmetic) = (LINEAR_DAMPING, ANGULAR_DAMPING, id + 1, live.cosmetic);
                (d.velocity, d.angular_velocity) = if want == specula_physics::BodyKind::Dynamic { (vel, ang) } else { (Vec3::ZERO, Vec3::ZERO) };
                let _ = self.space.insert(k, d);
            }
        }
        if !powered {
            for b in joints {
                let _ = self.space.set_joint_motor(key(id, b), None);
            }
        }
    }

    /// Powered: the root part follows the animation; joints spring towards its pose at `t`.
    fn drive_ragdoll(&mut self, world: &World, id: u64, t: f64) {
        let Some(e) = world.get(id) else { return };
        let Some(live) = self.ragdolls.live.get(&id) else { return };
        let set = Arc::clone(&live.set);
        let target = pose_at(&mut self.ragdolls.player, &set, e, t, true, false);
        let mut m = Vec::new();
        set.skeleton.model_space(&target, &mut m);
        let pose_e = super::world_pose_only(world, e);
        let live = self.ragdolls.live.get(&id).expect("checked");
        let scale = live.scale;
        let mut motors: Vec<(u64, Motor)> = Vec::new();
        let mut roots: Vec<(u64, Pose)> = Vec::new();
        for p in &live.parts {
            match (&p.joint, p.parent) {
                (Some(j), Some(parent)) if !p.planted => {
                    let rel = qmul(qconj(rotation_of(&m[parent])), rotation_of(&m[p.bone]));
                    let target = (j.frame_parent.q.conjugate() * q4(rel) * j.frame_child.q).normalize();
                    let drive = match j.kind {
                        JointKind::Hinge => Drive::Position(twist_angle(target)),
                        _ => Drive::Rotation(target),
                    };
                    let max_force = live.strength * 25.0 * p.carried * scale.max(0.1);
                    motors.push((key(id, p.bone), Motor { drive, max_force, hertz: DRIVE_HZ, damping_ratio: 1.0 }));
                }
                _ => roots.push((key(id, p.bone), bone_world(&pose_e, scale, &m[p.bone]))),
            }
        }
        for (k, motor) in motors {
            let _ = self.space.set_joint_motor(k, Some(motor));
        }
        for (k, pose) in roots {
            self.space.move_kinematic(k, pose);
        }
    }

    /// Switches a ragdoll off: (optionally) moves the entity under the pelvis, keeps the last
    /// pose for the transition out, and removes the bodies.
    fn stop_ragdoll(&mut self, world: &mut dyn WorldWrite, id: u64, relocate: bool, t: f64) {
        if relocate {
            let root = self.ragdolls.live.get(&id).and_then(|l| l.parts.iter().find(|p| p.parent.is_none()).map(|p| p.bone));
            if let (Some(b), Some(e)) = (root, world.get(id)) {
                if let Some(v) = self.space.body(key(id, b)) {
                    let at = e.world_position();
                    let to = Vec3::new(v.center.x, at.y, v.center.z);
                    let local = match e.parent.and_then(|p| world.get(p)) {
                        Some(parent) => parent.model().inverse().unwrap_or(specula_core::math::Mat4::IDENTITY).transform_point(to),
                        None => to,
                    };
                    let _ = world.update(&EntityKey::Id(id), UpdateSpec { position: Some(local), ..UpdateSpec::default() }, AGENT);
                }
            }
        }
        // The last pose, relative to where the entity is now.
        self.capture_ragdoll(world, id, false);
        self.remove_ragdoll_bodies(id);
        if let Some(l) = self.ragdolls.live.get_mut(&id) {
            l.off_since = Some(t);
        }
        if let Some(l) = self.ragdolls.live.get(&id) {
            world.set_external_pose(id, Some(world::ExternalPose { pose: Arc::clone(&l.pose), direct: false }));
        }
        self.space.set_group(id, 0);
    }

    fn remove_ragdoll_bodies(&mut self, id: u64) {
        let Some(l) = self.ragdolls.live.get(&id) else { return };
        let bones: Vec<usize> = l.parts.iter().map(|p| p.bone).collect();
        for b in bones {
            self.space.remove_joint(key(id, b));
            self.space.remove(key(id, b));
        }
    }

    /// The entity is gone or has no ragdoll any more.
    fn drop_ragdoll(&mut self, world: &mut dyn WorldWrite, id: u64) {
        self.remove_ragdoll_bodies(id);
        self.ragdolls.live.remove(&id);
        world.set_external_pose(id, None);
        if world.get(id).is_some() {
            self.space.set_group(id, 0);
        }
    }

    /// After the step: each ragdoll's pose from its bodies (as the entity's external pose), and
    /// its record for the checks.
    pub(super) fn ragdolls_after_step(&mut self, world: &mut dyn WorldWrite) {
        let ids: Vec<u64> = self.ragdolls.live.iter().filter(|(_, l)| l.off_since.is_none()).map(|(id, _)| *id).collect();
        for id in ids {
            self.capture_ragdoll(world, id, true);
        }
    }

    /// Computes ragdoll `id`'s local pose from its bodies (relative to the entity as it is now).
    fn capture_ragdoll(&mut self, world: &mut dyn WorldWrite, id: u64, record: bool) {
        let Some(e) = world.get(id) else { return };
        let pose_e = super::world_pose_only(world, e);
        let Some(live) = self.ragdolls.live.get_mut(&id) else { return };
        let skel = &live.set.skeleton;
        let inv_q = pose_e.q.conjugate();
        let scale = live.scale.max(1.0e-6);
        let mut model: Vec<Affine> = Vec::with_capacity(skel.len());
        for b in 0..skel.len() {
            let from_body = live.parts.iter().any(|p| p.bone == b).then(|| self.space.body(key(id, b))).flatten();
            let m = match from_body {
                Some(v) => {
                    let q = qa((inv_q * v.pose.q).normalize());
                    let p = pose_e.inv_point(v.pose.p) * (1.0 / scale);
                    Transform { t: [p.x, p.y, p.z], r: q, s: [1.0; 3] }.to_affine()
                }
                None => {
                    let local = live.start[b].to_affine();
                    match skel.parents[b] {
                        Some(par) => compose(&model[par as usize], &local),
                        None => local,
                    }
                }
            };
            model.push(m);
        }
        let mut local = Vec::with_capacity(skel.len());
        for b in 0..skel.len() {
            let l = match skel.parents[b] {
                Some(par) => compose(&inverse(&model[par as usize]).unwrap_or(Affine::IDENTITY), &model[b]),
                None => model[b],
            };
            local.push(Transform { t: l.t, r: rotation_of(&l), s: live.start[b].s });
        }
        let pose = Arc::new(APose { local });
        if record && live.frames.len() < RECORD_FRAMES {
            live.frames.push((*pose).clone());
        }
        live.pose = Arc::clone(&pose);
        let direct = live.mode == Mode::Powered;
        world.set_external_pose(id, Some(world::ExternalPose { pose, direct }));
    }

    /// A ragdoll key as data: `{ragdoll: {id, path}, bone}`.
    pub fn ragdoll_key_json(&self, world: &World, k: u64) -> Json {
        let Some((entity, bone)) = unkey(k) else { return Json::obj([("key", Json::from(k))]) };
        let name = self.ragdolls.live.get(&entity).and_then(|l| l.set.skeleton.names.get(bone).cloned()).unwrap_or_else(|| format!("#{bone}"));
        Json::obj([("ragdoll", Json::obj([("id", Json::from(entity)), ("path", Json::from(world.path_of(entity)))])), ("bone", Json::from(name))])
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A small humanoid rig (recipe-style: rest frames along the model axes).
    fn humanoid() -> AnimSet {
        let bone = |name: &str, parent: Option<usize>, head: V3, tail: V3| specula_model::rig::Bone { name: name.into(), parent, head, tail };
        let rig = specula_model::Rig {
            bones: vec![
                bone("root", None, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
                bone("hips", Some(0), [0.0, 0.9, 0.0], [0.0, 1.15, 0.0]),
                bone("chest", Some(1), [0.0, 1.15, 0.0], [0.0, 1.5, 0.0]),
                bone("head", Some(2), [0.0, 1.5, 0.0], [0.0, 1.75, 0.0]),
                bone("thigh_l", Some(1), [0.12, 0.9, 0.0], [0.12, 0.5, 0.0]),
                bone("shin_l", Some(4), [0.12, 0.5, 0.0], [0.12, 0.08, 0.0]),
                bone("thigh_r", Some(1), [-0.12, 0.9, 0.0], [-0.12, 0.5, 0.0]),
                bone("shin_r", Some(6), [-0.12, 0.5, 0.0], [-0.12, 0.08, 0.0]),
                bone("toe_r", Some(7), [-0.12, 0.08, 0.0], [-0.12, 0.06, 0.02]),
            ],
        };
        AnimSet::new(specula_anim::Skeleton::from_rig(&rig).unwrap())
    }

    #[test]
    fn plans_follow_the_skeleton_and_its_limits() {
        let set = humanoid();
        let knee = Limit { bone: set.skeleton.find("shin_l").unwrap(), axes: [Some([-5.0, 150.0]), None, None] };
        let parts = plan(&set, &[knee], &[], &[], 1.0, 40.0);
        let names: Vec<&str> = parts.iter().map(|p| set.skeleton.names[p.bone].as_str()).collect();
        assert_eq!(names, ["hips", "chest", "head", "thigh_l", "shin_l", "thigh_r", "shin_r"], "short bones and the root ride along");
        let total: f32 = parts.iter().map(|p| p.mass).sum();
        assert!((total - 40.0).abs() < 0.01, "{total}");
        // What each joint carries is planned once (review finding 5: drives summed it per joint
        // per tick, O(n^3) for a big skeleton): the root carries everything, an end part itself.
        let root = parts.iter().find(|p| p.parent.is_none()).unwrap();
        assert!((root.carried - total).abs() < 0.01, "{}", root.carried);
        for p in parts.iter().filter(|p| !parts.iter().any(|q| q.parent == Some(p.bone))) {
            assert_eq!(p.carried, p.mass);
        }
        let shin = parts.iter().find(|p| set.skeleton.names[p.bone] == "shin_l").unwrap();
        let j = shin.joint.as_ref().unwrap();
        assert_eq!(j.kind, JointKind::Hinge, "one limited axis: a knee hinge");
        assert!((j.limit.unwrap()[1] - 150f32.to_radians()).abs() < 1e-5);
        // The knee's anchor in the thigh's frame is the shin's head, 0.4 m down.
        assert!((j.frame_parent.p - Vec3::new(0.0, -0.4, 0.0)).length() < 1e-5);
        let other = parts.iter().find(|p| set.skeleton.names[p.bone] == "shin_r").unwrap();
        assert_eq!(other.joint.as_ref().unwrap().kind, JointKind::Ball, "no limits: a ball joint with a cone");
        assert_eq!(parts[0].parent, None);
        // Scaled up twice: twice the size, eight times... the mass we ask for.
        let big = plan(&set, &[], &[], &[], 2.0, 320.0);
        assert!((big[0].half - 2.0 * parts[0].half).abs() < 1e-5 && (big[0].offset.p - parts[0].offset.p * 2.0).length() < 1e-5);
        // The same input, the same plan.
        assert_eq!(plan(&set, &[], &[], &[], 1.0, 40.0), plan(&set, &[], &[], &[], 1.0, 40.0));
        // Given radii win (scaled with the entity).
        let hips = set.skeleton.find("hips").unwrap();
        let wide = plan(&set, &[], &[(hips, 0.2)], &[], 2.0, 40.0);
        assert!((wide[0].radius - 0.4).abs() < 1e-6);
        assert_eq!(unkey(key(77, 5)), Some((77, 5)));
    }

    /// The Sepulcra shambler (its recipe's rig, anims and creature graph) on a floor.
    pub(crate) fn shambler_world() -> (World, u64) {
        use crate::models::{Library, SharedLibrary};
        use crate::world::{ModelSource, ModelSpec, Shape as WShape, SpawnSpec};
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let shared = Arc::new(SharedLibrary::new(Library::new(&root, None)));
        let mut w = World::default();
        w.set_model_source(Some(Arc::clone(&shared) as Arc<dyn ModelSource>));
        let mut floor = SpawnSpec::new(WShape::Cube);
        floor.name = Some("floor".into());
        floor.position = Vec3::new(0.0, -0.5, 0.0);
        floor.scale = Vec3::new(20.0, 1.0, 20.0);
        floor.collider = Some(Collider::default());
        w.spawn(floor, "t").unwrap();
        let mut spec = SpawnSpec::model(ModelSpec::new("recipes/shambler.json"));
        spec.name = Some("shambler".into());
        let mut play = specula_anim::play::GraphPlay::new("creature");
        play.set_param("act", Value::Name("move".into()));
        play.set_param("speed", Value::Num(0.9));
        spec.anim = Some(crate::world::AnimSpec::graph(play));
        let id = w.spawn(spec, "t").unwrap().id;
        (w, id)
    }

    pub(crate) fn ragdoll(w: &mut World, id: u64, json: &str) {
        let props = Json::obj([(PROP, Json::parse(json).unwrap())]);
        w.update(&EntityKey::Id(id), UpdateSpec { props: Some(props), ..UpdateSpec::default() }, "t").unwrap();
    }

    /// Runs physics ticks at 30 Hz with the world clock following.
    pub(crate) fn run(w: &mut World, p: &mut Physics, from: u64, n: u64) {
        let settings = w.env().physics;
        for tick in from..from + n {
            w.set_clock_s(tick as f64 / 30.0);
            w.set_recording(crate::world::Recording::Snapshot);
            p.tick(w, 0, &settings, tick, 1.0 / 30.0);
            w.set_recording(crate::world::Recording::Full);
            w.take_changes();
        }
    }

    /// The shambler's built mesh and anim set.
    fn shambler_mesh() -> (World, u64, Arc<AnimSet>, specula_model::MeshData) {
        let (w, id) = shambler_world();
        let set = crate::anim::write_set_of(&w, w.get(id).unwrap()).expect("rigged");
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let style = specula_model::Style::parse(&std::fs::read_to_string(assets.join(crate::models::STYLE_FILE)).unwrap()).unwrap();
        let recipe = std::fs::read_to_string(assets.join("recipes/shambler.json")).unwrap();
        let model = specula_model::recipe::build(&recipe, &specula_model::recipe::BuildParams::new(&style)).unwrap();
        (w, id, set, model.lods[0].clone())
    }

    /// A body's lowest point: its hull's lowest corner (a hull's box is a loose bound), else its box.
    fn lowest_point(b: &specula_physics::BodyView) -> f32 {
        match b.collider.and_then(|c| c.shape.polytope().map(|h| (c, h))) {
            Some((c, h)) => {
                let at = b.pose.mul(&c.offset);
                h.points.iter().map(|q| at.point(*q).y).fold(f32::MAX, f32::min)
            }
            None => b.aabb.unwrap().min.y,
        }
    }

    /// The shambler knocked over with capsule parts or fitted hull parts: where its trunk ends up.
    fn knocked_over(hulls: bool) -> (Vec<(String, f32)>, bool) {
        let (mut w, id, set, mesh) = shambler_mesh();
        let mut spec = Spec {
            mode: Mode::Limp,
            strength: 1.0,
            mass: None,
            relocate: true,
            hit: Some(Hit { bone: "chest".into(), impulse: Vec3::new(0.0, 10.0, -60.0), seq: 1 }),
            radii: fit_radii(&set, &mesh).into_iter().map(|(b, r)| (set.skeleton.names[b].clone(), r)).collect(),
            hulls: Vec::new(),
        };
        if hulls {
            spec.hulls = fit_hulls(&set, &mesh).into_iter().map(|(b, p)| (set.skeleton.names[b].clone(), p)).collect();
        }
        let mut p = Physics::default();
        run(&mut w, &mut p, 0, 30);
        w.update(&EntityKey::Id(id), UpdateSpec { props: Some(Json::obj([(PROP, spec.to_json())])), ..UpdateSpec::default() }, "t").unwrap();
        run(&mut w, &mut p, 30, 150);
        let live = p.ragdolls.live.get(&id).expect("live");
        let heights = live.parts.iter().map(|part| (live.set.skeleton.names[part.bone].clone(), p.space.body(key(id, part.bone)).unwrap().center.y)).collect();
        let asleep = live.parts.iter().all(|part| p.space.body(key(id, part.bone)).unwrap().sleeping);
        (heights, asleep)
    }

    #[test]
    fn hull_parts_fitted_to_the_mesh_let_the_shambler_lie_down() {
        let (_, _, set, mesh) = shambler_mesh();
        let hulls = fit_hulls(&set, &mesh);
        assert!(hulls.len() >= 12, "{} hull parts", hulls.len());
        for (b, pts) in &hulls {
            assert!((4..=MAX_HULL_POINTS).contains(&pts.len()), "{}: {} points", set.skeleton.names[*b], pts.len());
            assert!(Shape::hull(&pts.iter().map(|p| v3(*p)).collect::<Vec<_>>()).is_ok());
        }
        assert_eq!(fit_hulls(&set, &mesh), hulls, "deterministic");
        // Round capsules prop the hunched body up on its coat; hulls the coat's shape let it
        // lie on its back.
        let height = |h: &[(String, f32)], b: &str| h.iter().find(|(n, _)| n == b).unwrap().1;
        let (caps, _) = knocked_over(false);
        let (hull, asleep) = knocked_over(true);
        assert!(asleep, "at rest");
        assert!(height(&hull, "chest") < 0.45 && height(&hull, "hips") < 0.25, "lying down: {hull:?}");
        assert!(height(&caps, "chest") - height(&hull, "chest") > 0.15, "capsules {caps:?} hulls {hull:?}");
        // Twin runs end the same.
        assert_eq!(knocked_over(true).0, hull);
    }

    /// Review round 3, finding 2 (probes hullfit.py, hullcost.py): ragdoll props cost nothing per
    /// tick while they don't change. Hull solidity is checked where the props are written; each
    /// entity's spec is parsed once per entity version. 20 entities with 14 hull parts of 16
    /// points, switched off, used to take 76 ms a tick (every hull built every tick).
    #[test]
    fn ragdoll_props_are_read_once_per_change_not_per_tick() {
        use crate::world::{Shape as WShape, SpawnSpec};
        let pts: Vec<String> = (0..16).map(|i| {
            let a = i as f64 * 0.7;
            format!("[{:.4},{:.4},{:.4}]", 0.1 * specula_core::dmath::cos_f64(a) * (1.0 + 0.1 * (i % 3) as f64), 0.02 * i as f64 - 0.15, 0.1 * specula_core::dmath::sin_f64(a))
        }).collect();
        let hulls: Vec<String> = (0..14).map(|b| format!("\"bone{b}\": [{}]", pts.join(","))).collect();
        let spec = format!(r#"{{"mode": "off", "hulls": {{{}}}}}"#, hulls.join(","));
        let mut w = World::default();
        let mut ids = Vec::new();
        for i in 0..20 {
            let mut s = SpawnSpec::new(WShape::Group);
            s.name = Some(format!("e{i}"));
            s.props = Json::obj([(PROP, Json::parse(&spec).unwrap())]);
            ids.push(w.spawn(s, "t").unwrap().id);
        }
        let mut p = Physics::default();
        run(&mut w, &mut p, 0, 30);
        assert_eq!(p.ragdolls.parses, 20, "once per entity while nothing changes");
        // Moving every entity every tick re-reads them (the version changes), but cheaply: no hull
        // is built.
        let started = std::time::Instant::now();
        for t in 30..60u64 {
            for (i, id) in ids.iter().enumerate() {
                let position = Some(Vec3::new(i as f32, 0.0, t as f32 * 0.01));
                w.update(&EntityKey::Id(*id), UpdateSpec { position, ..UpdateSpec::default() }, "t").unwrap();
            }
            run(&mut w, &mut p, t, 1);
        }
        let took = started.elapsed();
        assert_eq!(p.ragdolls.parses, 20 + 20 * 30);
        assert!(took < std::time::Duration::from_millis(1500), "30 ticks of 20 changing ragdoll entities took {took:?}");
    }

    /// Review round 4, finding 3 (probe stalecache.py): a force-undo that restored a deleted
    /// entity handed it a version it had had before, with other props, so the parsed spec (and
    /// physics' body cache) kept the old state. Restored versions now go past every version the
    /// entity had (`World::version_floor`), and the parsed spec is keyed on the content too.
    #[test]
    fn a_restored_entity_with_a_reused_version_is_read_again() {
        use crate::journal::{Kind, UndoRequest, Writer};
        let (mut w, id) = shambler_world();
        w.set_recording(crate::world::Recording::Full);
        let mut sw = crate::swarm::Swarm::default();
        let me = Writer { name: "probe", session: None };
        let mut p = Physics::default();
        let set = |sw: &mut crate::swarm::Swarm, w: &mut World, json: &str| {
            let props = Json::obj([(PROP, Json::parse(json).unwrap())]);
            sw.commit(w, 0, me, Kind::Set, |w| w.update(&EntityKey::Id(id), UpdateSpec { props: Some(props), ..UpdateSpec::default() }, "probe").map(|e| e.version)).unwrap()
        };
        let (v_off, _) = set(&mut sw, &mut w, r#"{"mode": "off"}"#);
        run(&mut w, &mut p, 0, 3);
        let (v_limp, limp_seq) = set(&mut sw, &mut w, r#"{"mode": "limp"}"#);
        run(&mut w, &mut p, 3, 1);
        assert!(p.ragdolls.live.get(&id).is_some_and(|l| l.off_since.is_none()), "limp and live");
        let (v_powered, _) = set(&mut sw, &mut w, r#"{"mode": "powered"}"#);
        sw.commit(&mut w, 0, me, Kind::Delete, |w| w.delete(&EntityKey::Id(id), None).map(|_| ())).unwrap();
        sw.undo(&mut w, 0, me, &UndoRequest { ids: Some(vec![limp_seq.unwrap()]), count: 1, force: true }).unwrap();
        let e = w.get(id).expect("restored");
        assert_eq!(v_limp, v_off + 1);
        assert!(e.version > v_powered, "a restored entity takes a version it never had ({} after {v_powered}); it used to get the limp one, {v_limp}, again", e.version);
        assert_eq!(e.props.get(PROP).and_then(|r| r.get("mode")).and_then(Json::as_str), Some("off"));
        run(&mut w, &mut p, 4, 3);
        assert!(p.ragdolls.live.get(&id).is_none_or(|l| l.off_since.is_some()), "switched off, as the props say");
    }

    /// Getting up is content: the shambler's graph has a getup state that blends from where the
    /// body lay to standing (0.8 s) and holds it a moment before `act` takes over (no snap).
    #[test]
    fn the_shambler_gets_up_before_it_walks_on() {
        let (mut w, id) = shambler_world();
        let set_param = |w: &mut World, name: &str, v: bool, at: f64| {
            w.set_clock_s(at);
            let mut play = specula_anim::play::GraphPlay::new("creature");
            play.set_param(name, Value::Bool(v));
            w.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(crate::world::AnimSpec::graph(play))), ..UpdateSpec::default() }, "t").unwrap();
        };
        set_param(&mut w, "ragdoll", true, 1.0);
        set_param(&mut w, "ragdoll", false, 2.0);
        let e = w.get(id).unwrap();
        let set = crate::anim::write_set_of(&w, e).expect("the shambler's set");
        let a = e.anim.as_ref().unwrap();
        let gp = a.graph.as_ref().unwrap();
        let g = Graphed { set: &set, graph: set.graph(&gp.graph).unwrap(), scale: 1.0 };
        let timing = Timing { anchor: a.start.unwrap_or(0.0), speed: a.speed };
        let mut sc = specula_anim::play::Scratch::default();
        let top = |t: f64, sc: &mut specula_anim::play::Scratch| specula_anim::play::state_at(g, gp, timing, t, sc)[0].stack.last().unwrap().state.clone();
        assert_eq!(top(2.1, &mut sc), "getup");
        assert_eq!(top(2.7, &mut sc), "getup", "it stands a moment");
        assert_eq!(top(3.2, &mut sc), "move", "then walks on");
    }

    /// Review round 2, finding 2 (probe orphan.py): a ragdoll whose start fails partway (a
    /// budget, a shape) leaves nothing behind, and a flat hull is refused as data.
    #[test]
    fn a_failed_start_leaves_no_parts_behind() {
        let (mut w, id) = shambler_world();
        let mut p = Physics::default();
        run(&mut w, &mut p, 0, 20);
        let before = (p.space.len(), p.space.hash());
        // Parts that went in come out again when a later one is refused (here a joint tying a
        // part to itself, after every body went in).
        let parts: Vec<(u64, BodyDesc)> = (1..4u64)
            .map(|b| (key(id, b as usize), BodyDesc::new(specula_physics::BodyKind::Dynamic, Pose::at(Vec3::new(b as f32, 1.0, 0.0)), Some(ColliderDesc::new(Shape::sphere(0.1))))))
            .collect();
        let bad = JointDesc::new(JointKind::Ball, Some(key(id, 1)), Some(key(id, 1)), Pose::IDENTITY, Pose::IDENTITY);
        let good = JointDesc::new(JointKind::Ball, Some(key(id, 1)), Some(key(id, 2)), Pose::IDENTITY, Pose::IDENTITY);
        assert!(p.insert_all(parts, vec![(key(id, 2), good), (key(id, 3), bad)]).is_err());
        assert_eq!((p.space.len(), p.space.hash()), before, "no orphan parts");
        assert!(p.space.joints().next().is_none(), "nor joints");
        // A coplanar hull never gets that far: it is refused as data where it is written.
        let flat = r#"{"mode": "limp", "hulls": {"shin_l": [[0,0,0],[0.1,0,0],[0,0.1,0],[0.1,0.1,0]]}}"#;
        let e = Spec::from_json(&Json::parse(flat).unwrap()).unwrap().check_solid().unwrap_err();
        assert!(e.contains("no solid"), "{e}");
        let props = Json::obj([(PROP, Json::parse(flat).unwrap())]);
        let e = w.update(&EntityKey::Id(id), UpdateSpec { props: Some(props), ..UpdateSpec::default() }, "t").unwrap_err();
        assert_eq!((e.code, e.message.contains("no solid")), ("invalid_component", true), "{e:?}");
    }

    /// Save games: a saved entity's ragdoll, mid-fall, continues bit for bit in a world loaded
    /// from its files plus the physics section; a transient entity's (cosmetic) one is dropped
    /// without touching the simulation.
    #[test]
    fn ragdolls_survive_a_save_and_load_bit_identically() {
        for powered in [false, true] {
            let (mut w, id) = shambler_world();
            let mut p = Physics::default();
            run(&mut w, &mut p, 0, 30);
            let mode = if powered { "powered" } else { "limp" };
            ragdoll(&mut w, id, &format!(r#"{{"mode": "{mode}", "hit": {{"bone": "chest", "impulse": [0, 10, -40], "seq": 1}}}}"#));
            run(&mut w, &mut p, 30, 12);
            let bytes = p.save_state(&w, 0).expect("physics ran");
            let files = crate::persist::render(&w);
            let mut b = crate::persist::load_with(|f| {
                files.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).ok_or_else(|| crate::persist::PersistError::new("not_found", f.to_owned()))
            })
            .unwrap()
            .0;
            b.set_model_source(w.model_source().cloned());
            b.set_clock_s(w.world_time());
            let mut q = Physics::default();
            q.restore_state(&b, 0, 41, &bytes).unwrap();
            assert_eq!(q.hash(), Some(p.space.hash()));
            assert_eq!(q.ragdolls.live.get(&id).map(|l| l.parts.len()), p.ragdolls.live.get(&id).map(|l| l.parts.len()));
            for t in 42..120 {
                run(&mut w, &mut p, t, 1);
                run(&mut b, &mut q, t, 1);
                assert_eq!(p.space.hash(), q.space.hash(), "{mode}: physics diverged at tick {t}");
                assert_eq!(w.state_hash(), b.state_hash(), "{mode}: the world diverged at tick {t}");
            }
        }
    }

    /// Found in Sepulcra: the killing blow sets off the graph's flinch layer in the same write
    /// that turns the ragdoll on; taken from the play with it, the takeover flung the hands
    /// up at 16 m/s. The ragdoll moves on at the base motion.
    #[test]
    fn a_takeover_moves_on_at_the_base_motion_not_a_flinch_just_set_off() {
        for phase in [20u64, 33] {
            let (mut w, id) = shambler_world();
            let mut p = Physics::default();
            run(&mut w, &mut p, 0, phase);
            // The blow (one write), then the death (another), at the tick's time.
            w.set_clock_s(phase as f64 / 30.0);
            for (name, v) in [("hit", true), ("ragdoll", true)] {
                let mut play = specula_anim::play::GraphPlay::new("creature");
                play.set_param(name, Value::Bool(v));
                w.update(&EntityKey::Id(id), UpdateSpec { anim: Some(Some(crate::world::AnimSpec::graph(play))), ..UpdateSpec::default() }, "t").unwrap();
            }
            ragdoll(&mut w, id, r#"{"mode": "limp"}"#);
            run(&mut w, &mut p, phase, 2);
            let live = p.ragdolls.live.get(&id).expect("a live ragdoll");
            for part in &live.parts {
                let v = p.space.body(key(id, part.bone)).unwrap().velocity.length();
                assert!(v < 4.0, "phase {phase}: {} moves at {v} m/s", live.set.skeleton.names[part.bone]);
            }
        }
    }

    #[test]
    fn a_walking_shambler_falls_limp_comes_to_rest_and_lets_go() {
        let (mut w, id) = shambler_world();
        let mut p = Physics::default();
        run(&mut w, &mut p, 0, 30);
        ragdoll(&mut w, id, r#"{"mode": "limp", "hit": {"bone": "chest", "impulse": [0, 0, -30], "seq": 1}}"#);
        run(&mut w, &mut p, 30, 120);
        let live = p.ragdolls.live.get(&id).expect("a live ragdoll");
        assert!(live.parts.len() >= 10, "{} parts", live.parts.len());
        assert!(w.external_pose(id).is_some_and(|x| !x.direct), "a limp ragdoll plays through the graph");
        // Lying on the floor, at rest, in one piece.
        let mut lowest = f32::MAX;
        for part in &live.parts {
            let b = p.space.body(key(id, part.bone)).unwrap();
            lowest = lowest.min(lowest_point(&b));
            assert!(b.velocity.length() < 0.3 || b.sleeping, "{} still moving: {:?}", live.set.skeleton.names[part.bone], b.velocity);
        }
        assert!(lowest > -0.03 && lowest < 0.05, "on the floor: lowest point {lowest}");
        let hips = p.space.body(key(id, live.set.skeleton.find("hips").unwrap())).unwrap();
        assert!(hips.center.y < 0.55, "it fell: the hips are at {:?}", hips.center);
        let worst = p.space.joints().filter(|j| j.key >= SYNTHETIC).map(|j| (j.state.linear_error, j.state.limit_excess)).fold((0.0f32, 0.0f32), |a, b| (a.0.max(b.0), a.1.max(b.1)));
        assert!(worst.0 < 0.02 && worst.1 < 0.06, "joints hold (gap, past limit): {worst:?}");
        // The motion checks run on what it did.
        let rules = specula_anim::checks::MotionRules::default();
        let report = specula_anim::checks::check_frames(&live.set, "shambler/ragdoll", &live.frames, live.frame_dt, &[], &rules, None);
        let pen = report.checks.iter().find(|c| c.check == "ground_penetration").expect("ran");
        assert!(pen.passed, "{pen:?}");
        // Off: the shambler moves under its pelvis and holds the pose, then lets go.
        let at = w.get(id).unwrap().position;
        ragdoll(&mut w, id, r#"{"mode": "off"}"#);
        run(&mut w, &mut p, 150, 1);
        assert!(p.space.joints().all(|j| j.key < SYNTHETIC) && p.space.keys().all(|k| k < SYNTHETIC), "the parts are gone");
        assert!(w.external_pose(id).is_some(), "the last pose stays for the transition out");
        assert!((w.get(id).unwrap().position - at).length() > 1.0e-4, "moved under the pelvis");
        run(&mut w, &mut p, 151, 70);
        assert!(w.external_pose(id).is_none() && p.ragdolls.live.is_empty(), "then lets go");
    }

    #[test]
    fn ragdolls_are_deterministic_and_cosmetic_ones_never_touch_the_simulation() {
        let go = |cosmetic_too: bool| {
            let (mut w, id) = shambler_world();
            // A crate the ragdoll lands on (simulation).
            let mut c = crate::world::SpawnSpec::new(crate::world::Shape::Cube);
            c.name = Some("crate".into());
            c.position = Vec3::new(0.0, 0.25, -0.8);
            c.scale = Vec3::new(0.5, 0.5, 0.5);
            c.collider = Some(Collider::default());
            c.body = Some(Body::new(BodyKind::Dynamic, 5.0));
            w.spawn(c, "t").unwrap();
            if cosmetic_too {
                // A transient shambler (game presentation) beside it, ragdolled too.
                let mut s = crate::world::SpawnSpec::model(crate::world::ModelSpec::new("recipes/shambler.json"));
                s.name = Some("ghost".into());
                s.transient = true;
                s.position = Vec3::new(0.3, 0.0, -0.5);
                s.props = Json::obj([(PROP, Json::parse(r#"{"mode": "limp", "hit": {"bone": "chest", "impulse": [0, 0, -20], "seq": 1}}"#).unwrap())]);
                w.spawn(s, "t").unwrap();
            }
            let mut p = Physics::default();
            run(&mut w, &mut p, 0, 10);
            ragdoll(&mut w, id, r#"{"mode": "limp", "hit": {"bone": "chest", "impulse": [0, 0, -30], "seq": 1}}"#);
            let mut hashes = Vec::new();
            for t in 10..80 {
                run(&mut w, &mut p, t, 1);
                hashes.push((p.hash().unwrap(), w.state_hash()));
            }
            (hashes, p.ragdolls.live.len())
        };
        let (a, n) = go(false);
        let (b, _) = go(false);
        assert_eq!(n, 1);
        assert_eq!(a, b, "twin runs step the same");
        let (c, m) = go(true);
        assert_eq!(m, 2, "the cosmetic ragdoll ran");
        assert_eq!(a, c, "a cosmetic ragdoll changes neither the physics hash nor the world");
    }

    /// Every joint's angle off the animation, worst first (for messages).
    fn joints_off(live: &Live, pose: &APose, target: &APose) -> Vec<(String, f32)> {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        live.set.skeleton.model_space(pose, &mut a);
        live.set.skeleton.model_space(target, &mut b);
        let rel = |m: &[Affine], p: usize, c: usize| q4(qmul(qconj(rotation_of(&m[p])), rotation_of(&m[c])));
        let mut v: Vec<(String, f32)> = live
            .parts
            .iter()
            .filter_map(|part| part.parent.map(|parent| (parent, part.bone)))
            .map(|(p, c)| (live.set.skeleton.names[c].clone(), specula_physics::joint::rotation_vector((rel(&a, p, c) * rel(&b, p, c).conjugate()).normalize()).length().to_degrees()))
            .collect();
        v.sort_by(|x, y| y.1.total_cmp(&x.1));
        v.truncate(4);
        v
    }

    #[test]
    fn powered_ragdolls_follow_the_animation_and_recover_from_hits() {
        let (mut w, id) = shambler_world();
        let mut p = Physics::default();
        run(&mut w, &mut p, 0, 10);
        ragdoll(&mut w, id, r#"{"mode": "powered", "strength": 0.6}"#);
        run(&mut w, &mut p, 10, 30);
        assert!(w.external_pose(id).is_some_and(|x| x.direct), "drawn as it is");
        // `run` leaves the world clock at the last tick's start; physics poses are for its end.
        let target = |w: &World, live: &Live| pose_at(&mut GraphPlayer::new(), &live.set, w.get(id).unwrap(), w.world_time() + 1.0 / 30.0, true, false);
        let live = &p.ragdolls.live[&id];
        // (Its legs follow the animation kinematically: the upper body is what the motors hold.)
        let tracking = joints_off_all(live, &live.pose, &target(&w, live)).into_iter().map(|(_, d)| d).fold(0.0f32, f32::max);
        assert!(tracking < 10.0, "follows the walk within 10 degrees at every joint: {tracking} {:?}", joints_off(live, &live.pose, &target(&w, live)));
        // A blow to the reaching arm knocks it off, and it recovers.
        let arm = |live: &Live, w: &World| {
            let off = joints_off_all(live, &live.pose, &target(w, live));
            off.iter().filter(|(n, _)| n.ends_with("arm_l") || n == "hand_l").map(|(_, d)| *d).fold(0.0f32, f32::max)
        };
        let before = arm(&p.ragdolls.live[&id], &w);
        ragdoll(&mut w, id, r#"{"mode": "powered", "strength": 0.6, "hit": {"bone": "forearm_l", "impulse": [0, 20, -20], "seq": 1}}"#);
        let mut peak = 0.0f32;
        for t in 40..46 {
            run(&mut w, &mut p, t, 1);
            peak = peak.max(arm(&p.ragdolls.live[&id], &w));
        }
        run(&mut w, &mut p, 46, 45);
        let live = &p.ragdolls.live[&id];
        let after = arm(live, &w);
        assert!(peak > before + 6.0, "the hit shows in the arm: {peak} vs {before} degrees");
        assert!(after < before + 4.0, "and it recovers: {after} degrees (before the hit {before}) {:?}", joints_off(live, &live.pose, &target(&w, live)));
        let all = joints_off_all(live, &live.pose, &target(&w, live)).into_iter().map(|(_, d)| d).fold(0.0f32, f32::max);
        assert!(all < 15.0, "the whole body follows again: {all} {:?}", joints_off(live, &live.pose, &target(&w, live)));
    }

    fn joints_off_all(live: &Live, pose: &APose, target: &APose) -> Vec<(String, f32)> {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        live.set.skeleton.model_space(pose, &mut a);
        live.set.skeleton.model_space(target, &mut b);
        let rel = |m: &[Affine], p: usize, c: usize| q4(qmul(qconj(rotation_of(&m[p])), rotation_of(&m[c])));
        live.parts
            .iter()
            .filter_map(|part| part.parent.map(|parent| (parent, part.bone)))
            .map(|(p, c)| (live.set.skeleton.names[c].clone(), specula_physics::joint::rotation_vector((rel(&a, p, c) * rel(&b, p, c).conjugate()).normalize()).length().to_degrees()))
            .collect()
    }

    #[test]
    fn specs_round_trip_and_explain_mistakes() {
        let s = Spec::from_json(&Json::parse(r#"{"mode":"powered","strength":2,"hit":{"bone":"chest","impulse":[0,0,30],"seq":3}}"#).unwrap()).unwrap();
        assert_eq!(Spec::from_json(&s.to_json()).unwrap(), s);
        assert!(Spec::from_json(&Json::parse(r#"{"mode":"floppy"}"#).unwrap()).unwrap_err().contains("limp"));
        assert!(Spec::from_json(&Json::parse(r#"{"mode":"limp","wobble":1}"#).unwrap()).unwrap_err().contains("valid"));
    }
}

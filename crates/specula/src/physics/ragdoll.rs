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
//!   Mass (default [`DEFAULT_MASS_KG`] for a figure at scale 1) is shared by capsule volume.
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
//!   like warm starts their bodies aren't saved in world files: a loaded world's ragdoll starts
//!   again from the animation.
//! - **Checks**: each ragdoll records its pose every tick (up to [`RECORD_FRAMES`]) and
//!   `ragdoll_info` runs the motion checks on it (penetration, where it comes to rest, joint
//!   limits, pops where physics takes over), beside its joints' own errors.

use super::joints::SYNTHETIC;
use super::*;
use specula_anim::graph::{AnimGraph, Limit, Value};
use specula_anim::math::{compose, inverse, qarc, qaxis, qconj, qmul, rotation_of, Affine, Transform, Q, QID, V3};
use specula_anim::play::{GraphPlayer, Graphed, Surroundings, Timing};
use specula_anim::{AnimSet, Pose as APose};
use specula_physics::joint::{twist_angle, Drive, JointDesc, JointKind, Motor};

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
}

const KEYS: &[&str] = &["mode", "strength", "mass", "relocate", "hit", "radii"];

impl Spec {
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
        Ok(Spec {
            radii,
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
    /// The capsule in the bone's frame (scaled).
    pub offset: Pose,
    pub mass: f32,
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
/// their bone's length): see the module docs.
pub fn plan(set: &AnimSet, limits: &[Limit], radii: &[(usize, f32)], scale: f32, mass: f32) -> Vec<Part> {
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
        let offset = Pose { p: v3(segs[b]) * (0.5 * scale), q: q4(qarc([0.0, 1.0, 0.0], dir)) };
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
        parts.push(Part { bone: b, parent, radius, half, offset, mass: 0.0, joint, planted: false });
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
    let parts = plan(set, &[], &[], 1.0, 1.0);
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

/// The engine's ragdolls.
#[derive(Default)]
pub struct Ragdolls {
    pub live: BTreeMap<u64, Live>,
    /// Saved entities with `props.ragdoll` (kept from the world's change log; transient ones are
    /// found each tick).
    pub persistent: BTreeSet<u64>,
    player: GraphPlayer,
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
    pub(super) fn ragdolls_before_step(&mut self, world: &mut World, tick: u64, dt: f32) {
        if self.ragdolls.live.is_empty() && self.ragdolls.persistent.is_empty() && world.transient_entities().next().is_none() {
            return;
        }
        let t = world.world_time();
        // Who has a ragdoll now.
        let mut wanted: Vec<(u64, Spec)> = Vec::new();
        let transient = world.transient_entities().filter(|e| e.props.get(PROP).is_some()).map(|e| e.id);
        let ids: Vec<u64> = self.ragdolls.persistent.iter().copied().chain(transient).collect();
        for id in ids {
            let Some(e) = world.get(id) else { continue };
            match Spec::from_json(e.props.get(PROP).expect("listed")) {
                Ok(s) => wanted.push((id, s)),
                Err(m) => self.note_problem_only(world, id, &format!("ragdoll: {m}")),
            }
        }
        // Gone, or no longer a ragdoll: drop it.
        let gone: Vec<u64> = self.ragdolls.live.keys().copied().filter(|id| !wanted.iter().any(|(w, _)| w == id)).collect();
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

    fn start_ragdoll(&mut self, world: &mut World, id: u64, spec: &Spec, t: f64, dt: f32, tick: u64) -> Result<(), String> {
        let e = world.get(id).ok_or("gone")?;
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
        // Radii: the entity's own (props), over its graph's.
        let mut radii: Vec<(usize, f32)> = graph_of(&set, e).map(|g| g.ragdoll_radius.clone()).unwrap_or_default();
        for (name, r) in &spec.radii {
            let b = set.skeleton.find(name).ok_or_else(|| format!("'ragdoll.radii' names no bone '{name}'"))?;
            radii.retain(|(x, _)| *x != b);
            radii.push((b, *r));
        }
        let parts = plan(&set, &limits, &radii, scale, mass);
        if parts.is_empty() {
            return Err(format!("no bone is at least {MIN_BONE_M} m long"));
        }
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
        for p in &parts {
            let w = bone_world(&pose_e, scale, &m_now[p.bone]);
            starts.push((p.bone, w));
            let w1 = bone_world(&pose_e, scale, &m_next[p.bone]);
            let collider = ColliderDesc { layer, ..ColliderDesc::new(Shape::capsule(p.radius, p.half)) };
            let collider = ColliderDesc { offset: p.offset, friction: 0.8, ..collider };
            let kind = if powered && (p.parent.is_none() || p.planted) { specula_physics::BodyKind::Kinematic } else { specula_physics::BodyKind::Dynamic };
            let mut d = BodyDesc::new(kind, w, Some(collider));
            d.mass = p.mass;
            d.linear_damping = LINEAR_DAMPING;
            d.angular_damping = ANGULAR_DAMPING;
            d.group = id + 1;
            d.cosmetic = cosmetic;
            // Moving on as the animation was (the centre's velocity from the bone's motion).
            let c_now = w.point(p.offset.p);
            let c_next = w1.point(p.offset.p);
            if kind == specula_physics::BodyKind::Dynamic {
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
            self.space.insert(key(id, p.bone), d).map_err(|e| e.message)?;
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
            let carried: f32 = parts.iter().filter(|q| set.skeleton.is_under(q.bone, p.bone)).map(|q| q.mass).sum();
            jd.friction = FRICTION_PER_KG * carried * scale;
            self.space.insert_joint(key(id, p.bone), jd).map_err(|e| e.message)?;
        }
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
        let (pose_e, _) = world_pose(world, e);
        let live = self.ragdolls.live.get(&id).expect("checked");
        let scale = live.scale;
        // What each joint carries: its part and the parts below it.
        let carried = |b: usize| -> f32 { live.parts.iter().filter(|p| set.skeleton.is_under(p.bone, b)).map(|p| p.mass).sum() };
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
                    let max_force = live.strength * 25.0 * carried(p.bone) * scale.max(0.1);
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
    fn stop_ragdoll(&mut self, world: &mut World, id: u64, relocate: bool, t: f64) {
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
    fn drop_ragdoll(&mut self, world: &mut World, id: u64) {
        self.remove_ragdoll_bodies(id);
        self.ragdolls.live.remove(&id);
        world.set_external_pose(id, None);
        if world.get(id).is_some() {
            self.space.set_group(id, 0);
        }
    }

    /// After the step: each ragdoll's pose from its bodies (as the entity's external pose), and
    /// its record for the checks.
    pub(super) fn ragdolls_after_step(&mut self, world: &mut World) {
        let ids: Vec<u64> = self.ragdolls.live.iter().filter(|(_, l)| l.off_since.is_none()).map(|(id, _)| *id).collect();
        for id in ids {
            self.capture_ragdoll(world, id, true);
        }
    }

    /// Computes ragdoll `id`'s local pose from its bodies (relative to the entity as it is now).
    fn capture_ragdoll(&mut self, world: &mut World, id: u64, record: bool) {
        let Some(e) = world.get(id) else { return };
        let (pose_e, _) = world_pose(world, e);
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
mod tests {
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
        let parts = plan(&set, &[knee], &[], 1.0, 40.0);
        let names: Vec<&str> = parts.iter().map(|p| set.skeleton.names[p.bone].as_str()).collect();
        assert_eq!(names, ["hips", "chest", "head", "thigh_l", "shin_l", "thigh_r", "shin_r"], "short bones and the root ride along");
        let total: f32 = parts.iter().map(|p| p.mass).sum();
        assert!((total - 40.0).abs() < 0.01, "{total}");
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
        let big = plan(&set, &[], &[], 2.0, 320.0);
        assert!((big[0].half - 2.0 * parts[0].half).abs() < 1e-5 && (big[0].offset.p - parts[0].offset.p * 2.0).length() < 1e-5);
        // The same input, the same plan.
        assert_eq!(plan(&set, &[], &[], 1.0, 40.0), plan(&set, &[], &[], 1.0, 40.0));
        // Given radii win (scaled with the entity).
        let hips = set.skeleton.find("hips").unwrap();
        let wide = plan(&set, &[], &[(hips, 0.2)], 2.0, 40.0);
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

    fn ragdoll(w: &mut World, id: u64, json: &str) {
        let props = Json::obj([(PROP, Json::parse(json).unwrap())]);
        w.update(&EntityKey::Id(id), UpdateSpec { props: Some(props), ..UpdateSpec::default() }, "t").unwrap();
    }

    /// Runs physics ticks at 30 Hz with the world clock following.
    fn run(w: &mut World, p: &mut Physics, from: u64, n: u64) {
        let settings = w.env().physics;
        for tick in from..from + n {
            w.set_clock_s(tick as f64 / 30.0);
            w.set_recording(crate::world::Recording::Snapshot);
            p.tick(w, 0, &settings, tick, 1.0 / 30.0);
            w.set_recording(crate::world::Recording::Full);
            w.take_changes();
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
            lowest = lowest.min(b.aabb.unwrap().min.y);
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

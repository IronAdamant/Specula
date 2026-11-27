//! Procedural motion generators: small JSON specs that produce ordinary [`Clip`]s, so agents
//! author creature motion with parameters (stride, bounce, sway, swing) instead of keyframes.
//!
//! Every spec has `name`, `kind`, `duration` (s) and optionally `rate` (keys/s, default 30),
//! `loop`, `events` (extra `{name, time, data}`) and `wobble` (sine layers on any bone). Kinds:
//!
//! | kind | motion | default loop |
//! |---|---|---|
//! | `gait` | walk/run/stomp cycle: IK feet (planted stance, lifted swing), pelvis bounce/sway/roll/twist, spine lean and counter-twist, arm swing, limp; `footstep` events; `distance` = `stride` for speed matching | loop |
//! | `idle` | `gait` with no stride: breathing, sway and a slow look while the feet stay planted | loop |
//! | `hover` | float: pelvis bob and drift, a bone `chain` (tail, shroud) waving with a delay per bone | loop |
//! | `swing` | rest → `windup` pose → `strike` pose → hold → rest, with a `hit` event; feet stay planted | once |
//! | `leap` | crouch, ballistic root arc (`distance`, `height`) with tucked legs, landing absorb; `takeoff`/`land` events; root motion | once |
//! | `collapse` | death: `topple`, `crumble`, `scatter` (bones fly apart, seeded) or `fade` (rise, spin, shrink, `opacity` curve); `reach` (deg) swings the arms forward over the head as a topple falls, so a body face down ends with them along the ground | once |
//! | `wobble` | only the `wobble` layers (mechanisms, flames) | loop |
//! | `keys` | keyframed poses: `keys: [{time, pose: {bone: {rot, move, scale}}}]`, `interp` (`step`, `linear`, `cubic`); bones missing from a key are at rest in it; feet stay planted when `legs` are given | once |
//! | `pose` | one held `pose` (a hand-authored stance, a grip) | loop |
//! | `recoil` | a hit or kick: snaps to `pose` over `attack` s, then springs back (`frequency` Hz, `damping` ratio); made for additive layers | once |
//! | `breathe` | breathing only: the last `spine` bone pitches `deg` and swells `scale`, `breaths` per loop, the head counters; made for additive layers | loop |
//! | `chain` | secondary motion over another clip (`source`, earlier in the set): a bone `chain` (tail, shroud, cloth strip) simulated as a damped spring chain (`stiffness`, `damping`, `gravity`) driven by the source's motion and travel, then baked; loops seamlessly | the source's |
//!
//! `"body": "auto"` fills `pelvis`, `spine`, `head`, `legs` and `arms` from the rig's bone names
//! (`hips`/`pelvis`, `spine*`/`chest`, `head`, `thigh_X`/`shin_X`/`foot_X`, `upperarm_X`/`forearm_X`
//! for every side `X`), legs half a cycle apart and each arm against its side's leg: a walk cycle
//! from a rig and a gait description (`{"kind": "gait", "body": "auto", "stride": 1.0, ...}`).
//! Anything given explicitly wins.
//!
//! Body parts are named bones: `pelvis`, `spine` (list, lower first), `head`, `legs`
//! (`{thigh, shin, foot, phase, lift, foot_pitch, z, ankle_y}`), `arms`
//! (`{upper, fore, phase, swing, bend, roll, lag}`). Angles are degrees about the model axes
//! (rest frames are model-aligned): +X pitches forward/down (bends knees, nods, opens jaws),
//! −X swings a hanging arm forward. Generators need a skeleton with model-aligned rest frames
//! (every recipe rig).
//!
//! The legs are solved with analytic two-bone IK every key, so a gait's planted foot moves
//! backwards in a straight line at exactly `stride / duration`: playing the clip at the matched
//! rate leaves no foot sliding. The pelvis is lowered automatically (`crouch`) when a leg would
//! have to over-stretch.

use specula_core::rng::Rng;
use specula_core::Json;

use crate::clip::{Channel, Clip, Curve, Event, Interp, Keys, LoopMode, Track};
use crate::error::{err, num, vecn, AnimError, AnimErrorCode, Obj};
use crate::math::{
    self, cos_turns, frac, inverse, math3, qaxis, qconj, qeuler, qframe, qmul, qnorm, qrot, rotation_of, sin_turns, smoothstep, Affine, Transform, V3, Q, QID,
};
use crate::set::AnimSet;
use crate::skeleton::Skeleton;

/// Generator kinds.
pub const KINDS: &[&str] = &["gait", "idle", "hover", "swing", "leap", "collapse", "wobble", "keys", "pose", "recoil", "breathe", "chain"];
/// Most keys a `keys` spec may list.
pub const MAX_POSE_KEYS: usize = 256;
/// Most bones a `chain` may simulate.
pub const MAX_CHAIN: usize = 16;

/// Most a leg may stretch, as a fraction of its full length, before the pelvis is lowered.
pub const MAX_REACH: f32 = 0.97;
/// Most legs or arms a generator spec may list.
pub const MAX_LIMBS: usize = 8;
/// Most `wobble` layers in one spec.
pub const MAX_WOBBLES: usize = 64;
/// Most generation work one generated clip may take, in frames × bones (a 60 s gait at 120 Hz on
/// an 18-bone rig is about 130,000: refused; Sepulcra's clips run about 30 to 121 frames each).
pub const MAX_CLIP_WORK: u64 = 24_000;
/// Most generation work all the generated clips of one anim set may take together, in frames ×
/// bones (overlays of world assets count too). Sepulcra's largest set (the colossus) is about
/// 11,000. About a quarter of a second of generation at the limit (release).
pub const MAX_SET_WORK: u64 = 160_000;

use specula_model::rig::MAX_BONES;

// Each generator kind is a file of its own (gen/<kind>.rs).
mod gait;
mod hover;
mod swing;
mod leap;
mod collapse;
mod keyed;
mod recoil;
mod breathe;
mod chain;
use {gait::gait, hover::hover, swing::swing, leap::leap, collapse::collapse, keyed::keyed, recoil::recoil, breathe::breathe, chain::chain};

/// Generates the clip a spec describes, for `skel`. Errors carry paths under `path`. Kinds that
/// build on other clips (`chain`) need [`generate_in`].
pub fn generate(spec: &Json, path: &str, skel: &Skeleton) -> Result<Clip, AnimError> {
    generate_inner(spec, skel, None).map(|(c, _)| c).map_err(|e| e.under(path))
}

/// [`generate`] for a clip of `set`: generators may build on the set's clips.
pub fn generate_in(spec: &Json, path: &str, set: &AnimSet) -> Result<Clip, AnimError> {
    generate_counted(spec, path, set).map(|(c, _)| c)
}

/// [`generate_in`], with the generation work it took (frames × bones), refused past
/// [`MAX_CLIP_WORK`] or past what is left of the set's [`MAX_SET_WORK`].
pub(crate) fn generate_counted(spec: &Json, path: &str, set: &AnimSet) -> Result<(Clip, u64), AnimError> {
    generate_inner(spec, &set.skeleton, Some(set)).map_err(|e| e.under(path))
}

// ---------------------------------------------------------------------------------------------
// Body description

#[derive(Clone, Debug)]
struct Leg {
    thigh: usize,
    shin: usize,
    foot: Option<usize>,
    phase: f32,
    lift: f32,
    foot_pitch: f32,
    /// Rest model-space ankle.
    ankle: V3,
    /// Stance centre (model space, root at rest): x of the rest ankle, ankle height, hip z + `z`.
    plant: V3,
    /// Rest thigh and shin vectors.
    a: V3,
    b: V3,
    /// Rest bend-plane normal and which side of the hip–ankle line the knee sits on.
    n0: V3,
    side: f32,
}

#[derive(Clone, Debug)]
struct Arm {
    upper: usize,
    fore: Option<usize>,
    phase: f32,
    swing: f32,
    bend: f32,
    roll: f32,
    lag: f32,
}

#[derive(Clone, Debug)]
struct Wobble {
    bone: usize,
    axis: usize,
    deg: f32,
    scale: f32,
    cycles: f32,
    phase: f32,
}

#[derive(Clone, Debug, Default)]
struct Body {
    pelvis: Option<usize>,
    spine: Vec<usize>,
    head: Option<usize>,
    legs: Vec<Leg>,
    arms: Vec<Arm>,
    wobbles: Vec<Wobble>,
}

/// Rest-relative deltas for every bone at one key, plus IK foot targets.
#[derive(Clone, Debug)]
struct Frame {
    d: Vec<Transform>,
    /// Per leg: ankle target (model space with the root at rest) and foot pitch (deg).
    feet: Vec<Option<(V3, f32)>>,
}

impl Frame {
    fn new(bones: usize, legs: usize) -> Frame {
        Frame { d: vec![Transform::IDENTITY; bones], feet: vec![None; legs] }
    }
    fn rot(&mut self, b: usize, q: Q) {
        self.d[b].r = qmul(self.d[b].r, q);
    }
    fn mov(&mut self, b: usize, v: V3) {
        self.d[b].t = math3::add(self.d[b].t, v);
    }
    fn scale(&mut self, b: usize, s: V3) {
        self.d[b].s = math3::mul(self.d[b].s, s);
    }
}

fn bone_of(skel: &Skeleton, name: &str, path: &str) -> Result<usize, AnimError> {
    skel.find(name).ok_or_else(|| err(path, AnimErrorCode::UnknownBone, format!("no bone '{name}' (bones: {})", skel.names.join(", "))))
}

fn opt_bone(o: &mut Obj, k: &str, skel: &Skeleton) -> Result<Option<usize>, AnimError> {
    let p = o.at(k);
    o.str(k)?.map(|n| bone_of(skel, n, &p)).transpose()
}

fn req_bone(o: &mut Obj, k: &str, skel: &Skeleton) -> Result<usize, AnimError> {
    let p = o.at(k);
    bone_of(skel, o.req_str(k)?, &p)
}

/// Fails with `Limit` (before any work) if a list has more than `max` entries.
fn capped<'a>(o: &mut Obj<'a>, k: &str, max: usize) -> Result<Option<&'a [Json]>, AnimError> {
    let p = o.at(k);
    match o.arr(k)? {
        Some(items) if items.len() > max => {
            Err(err(&p, AnimErrorCode::Limit, format!("{} entries; at most {max}", items.len())).with_numbers(items.len() as f64, max as f64))
        }
        other => Ok(other),
    }
}

fn bone_list(o: &mut Obj, k: &str, skel: &Skeleton) -> Result<Vec<usize>, AnimError> {
    let p = o.at(k);
    let Some(items) = capped(o, k, MAX_BONES)? else { return Ok(Vec::new()) };
    items
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let ip = format!("{p}[{i}]");
            bone_of(skel, v.as_str().ok_or_else(|| err(&ip, AnimErrorCode::WrongType, "expected a bone name"))?, &ip)
        })
        .collect()
}

fn child_of(skel: &Skeleton, child: usize, parent: usize, path: &str) -> Result<(), AnimError> {
    if skel.parents[child] != Some(parent as u16) {
        return Err(err(path, AnimErrorCode::InvalidRig, format!("'{}' must be a child of '{}'", skel.names[child], skel.names[parent])));
    }
    Ok(())
}

/// The body parts a rig's bone names suggest (`"body": "auto"`), as spec JSON.
fn auto_body(skel: &Skeleton) -> Vec<(&'static str, Json)> {
    let find = |names: &[&str]| names.iter().find_map(|n| skel.find(n));
    let mut out: Vec<(&'static str, Json)> = Vec::new();
    let pelvis = find(&["hips", "pelvis"]);
    if let Some(p) = pelvis {
        out.push(("pelvis", Json::Str(skel.names[p].clone())));
    }
    let head = find(&["head"]);
    // The spine: the head's ancestors below it and above the pelvis, lowest first.
    if let (Some(h), Some(p)) = (head, pelvis) {
        let mut chain = Vec::new();
        let mut k = skel.parents[h];
        while let Some(i) = k {
            if i as usize == p {
                break;
            }
            chain.push(i as usize);
            k = skel.parents[i as usize];
        }
        chain.reverse();
        if !chain.is_empty() {
            out.push(("spine", Json::arr(chain.iter().map(|&b| Json::Str(skel.names[b].clone())))));
        }
    }
    if let Some(h) = head {
        out.push(("head", Json::Str(skel.names[h].clone())));
    }
    // Sides: every suffix a thigh has (`thigh_l` → `_l`).
    let mut legs = Vec::new();
    let mut arms = Vec::new();
    for (i, name) in skel.names.iter().enumerate() {
        let Some(side) = name.strip_prefix("thigh") else { continue };
        let (shin, foot) = (format!("shin{side}"), format!("foot{side}"));
        let Some(sh) = skel.find(&shin) else { continue };
        if skel.parents[sh] != Some(i as u16) {
            continue;
        }
        let phase = if legs.len() % 2 == 0 { 0.0 } else { 0.5 };
        let mut leg = vec![("thigh", Json::Str(name.clone())), ("shin", Json::Str(shin)), ("phase", Json::Num(phase))];
        if skel.find(&foot).is_some_and(|f| skel.parents[f] == Some(sh as u16)) {
            leg.push(("foot", Json::Str(foot)));
        }
        legs.push(Json::obj(leg));
        let (upper, fore) = (format!("upperarm{side}"), format!("forearm{side}"));
        if let Some(u) = skel.find(&upper) {
            let mut arm = vec![("upper", Json::Str(upper)), ("phase", Json::Num(if phase == 0.0 { 0.5 } else { 0.0 })), ("swing", Json::Num(12.0)), ("bend", Json::Num(10.0))];
            if skel.find(&fore).is_some_and(|f| skel.parents[f] == Some(u as u16)) {
                arm.push(("fore", Json::Str(fore)));
            }
            arms.push(Json::obj(arm));
        }
    }
    if !legs.is_empty() {
        out.push(("legs", Json::Arr(legs)));
    }
    if !arms.is_empty() {
        out.push(("arms", Json::Arr(arms)));
    }
    out
}

fn parse_body(o: &mut Obj, skel: &Skeleton, looping: bool) -> Result<Body, AnimError> {
    let mut body = Body { pelvis: opt_bone(o, "pelvis", skel)?, spine: bone_list(o, "spine", skel)?, head: opt_bone(o, "head", skel)?, ..Body::default() };
    if let Some(items) = capped(o, "legs", MAX_LIMBS)? {
        let base = o.at("legs");
        let mut explicit_y = Vec::new();
        for (i, v) in items.iter().enumerate() {
            let p = format!("{base}[{i}]");
            let mut lo = Obj::new(v, &p, "a leg")?;
            let thigh = req_bone(&mut lo, "thigh", skel)?;
            let shin = req_bone(&mut lo, "shin", skel)?;
            child_of(skel, shin, thigh, &lo.at("shin"))?;
            let foot = opt_bone(&mut lo, "foot", skel)?;
            if let Some(f) = foot {
                child_of(skel, f, shin, &lo.at("foot"))?;
            }
            let phase = lo.num("phase", 0.0, 0.0, 1.0)?;
            let lift = lo.num("lift", 1.0, 0.0, 3.0)?;
            let foot_pitch = lo.num("foot_pitch", 0.0, -180.0, 180.0)?;
            let z = lo.num("z", 0.0, -10.0, 10.0)?;
            explicit_y.push(match lo.get("ankle_y") {
                Some(v) => Some(num(v, &lo.at("ankle_y"), -10.0, 100.0)?),
                None => None,
            });
            lo.finish("thigh, shin, foot, phase, lift, foot_pitch, z, ankle_y")?;
            // Two legs can't drive the same bone (the second solve would overwrite the first).
            let mine = [Some(thigh), Some(shin), foot];
            if let Some(other) = body.legs.iter().position(|l| [Some(l.thigh), Some(l.shin), l.foot].iter().flatten().any(|b| mine.contains(&Some(*b)))) {
                return Err(err(&p, AnimErrorCode::Duplicate, format!("this leg shares a bone with legs[{other}]")));
            }
            let hip = skel.heads[thigh];
            let knee = skel.heads[shin];
            let ankle = foot.map_or(skel.tails[shin], |f| skel.heads[f]);
            let a = math3::sub(knee, hip);
            let b = math3::sub(ankle, knee);
            if math3::length(a) < 1e-3 || math3::length(b) < 1e-3 {
                return Err(err(&p, AnimErrorCode::InvalidRig, "thigh and shin must both have length (the shin needs a foot bone or a tail)"));
            }
            let cr = math3::cross(a, b);
            let n0 = if math3::length(cr) > 1e-4 * math3::length(a) * math3::length(b) { math3::normalize(cr) } else { [1.0, 0.0, 0.0] };
            let c = math3::normalize(math3::add(a, b));
            let k_side = math3::sub(a, math3::scale(c, math3::dot(a, c)));
            let side = if math3::length(k_side) > 1e-4 {
                if math3::dot(k_side, math3::cross(n0, c)) >= 0.0 {
                    1.0
                } else {
                    -1.0
                }
            } else {
                -1.0 // a straight rest leg bends its knee forward (+Z)
            };
            body.legs.push(Leg { thigh, shin, foot, phase, lift, foot_pitch, ankle, plant: [ankle[0], 0.0, hip[2] + z], a, b, n0, side });
        }
        // Planted ankle height: explicit, else the lowest rest ankle (feet share one ground).
        let ground = body.legs.iter().map(|l| l.ankle[1]).fold(f32::MAX, f32::min);
        for (l, y) in body.legs.iter_mut().zip(explicit_y) {
            l.plant[1] = y.unwrap_or(ground);
        }
    }
    if let Some(items) = capped(o, "arms", MAX_LIMBS)? {
        let base = o.at("arms");
        for (i, v) in items.iter().enumerate() {
            let p = format!("{base}[{i}]");
            let mut ao = Obj::new(v, &p, "an arm")?;
            let upper = req_bone(&mut ao, "upper", skel)?;
            let fore = opt_bone(&mut ao, "fore", skel)?;
            if let Some(f) = fore {
                child_of(skel, f, upper, &ao.at("fore"))?;
            }
            let arm = Arm {
                upper,
                fore,
                phase: ao.num("phase", 0.0, 0.0, 1.0)?,
                swing: ao.num("swing", 0.0, -180.0, 180.0)?,
                bend: ao.num("bend", 0.0, -180.0, 180.0)?,
                roll: ao.num("roll", 0.0, -180.0, 180.0)?,
                lag: ao.num("lag", 0.0, 0.0, 1.0)?,
            };
            ao.finish("upper, fore, phase, swing, bend, roll, lag")?;
            body.arms.push(arm);
        }
    }
    if let Some(items) = capped(o, "wobble", MAX_WOBBLES)? {
        let base = o.at("wobble");
        for (i, v) in items.iter().enumerate() {
            let p = format!("{base}[{i}]");
            let mut wo = Obj::new(v, &p, "a wobble")?;
            let bone = req_bone(&mut wo, "bone", skel)?;
            let axis = match wo.str("axis")? {
                None | Some("x") => 0,
                Some("y") => 1,
                Some("z") => 2,
                Some(other) => return Err(err(&wo.at("axis"), AnimErrorCode::UnknownName, format!("axis '{other}' (x, y or z)"))),
            };
            let w = Wobble {
                bone,
                axis,
                deg: wo.num("deg", 0.0, -180.0, 180.0)?,
                scale: wo.num("scale", 0.0, -0.9, 2.0)?,
                cycles: wo.num("cycles", 1.0, 0.0, 64.0)?,
                phase: wo.num("phase", 0.0, -1.0, 1.0)?,
            };
            if looping && w.cycles != specula_core::dmath::round(w.cycles) {
                return Err(err(&wo.at("cycles"), AnimErrorCode::OutOfRange, "a looping clip's wobble needs a whole number of cycles (seamless loop)"));
            }
            wo.finish("bone, axis, deg, scale, cycles, phase")?;
            body.wobbles.push(w);
        }
    }
    Ok(body)
}

fn axis_vec(a: usize) -> V3 {
    let mut v = [0.0; 3];
    v[a] = 1.0;
    v
}

fn apply_wobbles(f: &mut Frame, body: &Body, s: f32) {
    for w in &body.wobbles {
        let k = sin_turns(w.cycles * s + w.phase);
        if w.deg != 0.0 {
            f.rot(w.bone, qaxis(axis_vec(w.axis), w.deg * k));
        }
        if w.scale != 0.0 {
            let mut sc = [1.0; 3];
            sc[w.axis] = 1.0 + w.scale * k;
            f.scale(w.bone, sc);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Kinematics

fn local_affine(skel: &Skeleton, d: &[Transform], b: usize) -> Affine {
    let r = skel.rest[b];
    Transform { t: math3::add(r.t, d[b].t), r: qmul(r.r, d[b].r), s: math3::mul(r.s, d[b].s) }.to_affine()
}

/// Model-space transform of bone `b` under deltas `d`.
fn global(skel: &Skeleton, d: &[Transform], b: usize) -> Affine {
    let mut chain = Vec::with_capacity(8);
    let mut k = Some(b as u16);
    while let Some(i) = k {
        chain.push(i as usize);
        k = skel.parents[i as usize];
    }
    let mut m = Affine::IDENTITY;
    for &j in chain.iter().rev() {
        m = m.then_after(&local_affine(skel, d, j));
    }
    m
}

/// Model-space transforms of every bone.
fn fk(skel: &Skeleton, d: &[Transform]) -> Vec<Affine> {
    let mut out: Vec<Affine> = Vec::with_capacity(skel.len());
    for b in 0..skel.len() {
        let l = local_affine(skel, d, b);
        let m = match skel.parents[b] {
            Some(p) => out[p as usize].then_after(&l),
            None => l,
        };
        out.push(m);
    }
    out
}

/// Solves every leg with a foot target; returns the largest stretch ratio asked for.
fn solve(skel: &Skeleton, body: &Body, f: &mut Frame) -> f32 {
    let mut worst = 0.0f32;
    for (li, leg) in body.legs.iter().enumerate() {
        let Some((target, pitch)) = f.feet[li] else { continue };
        let g_root = global(skel, &f.d, 0);
        let q_root = rotation_of(&g_root);
        // Targets are written with the root at rest; carry them with the root.
        let target = g_root.apply_point(math3::sub(target, skel.heads[0]));
        let parent = skel.parents[leg.thigh].map_or(0, |p| p as usize);
        let gp = if skel.parents[leg.thigh].is_some() { global(skel, &f.d, parent) } else { Affine::IDENTITY };
        // Solve in the thigh's parent space: rest vectors are in its units, so a scaled body
        // (crumbling, shrinking) stays consistent.
        let q_foot_model = qmul(q_root, qaxis([1.0, 0.0, 0.0], pitch + leg.foot_pitch));
        let qp_model = rotation_of(&gp);
        let target = inverse(&gp).unwrap_or(Affine::IDENTITY).apply_point(target);
        let qp = QID;
        let hip = math3::add(skel.rest[leg.thigh].t, f.d[leg.thigh].t);
        let (l1, l2) = (math3::length(leg.a), math3::length(leg.b));
        let dv = math3::sub(target, hip);
        let want = math3::length(dv);
        worst = worst.max(want / (l1 + l2));
        let dn = if want > 1e-6 { math3::scale(dv, 1.0 / want) } else { [0.0, -1.0, 0.0] };
        let d = want.clamp((l1 - l2).abs() + 1e-4, (l1 + l2) * (1.0 - 1e-4));
        let np = qrot(qp, leg.n0);
        let mut n1 = math3::sub(np, math3::scale(dn, math3::dot(np, dn)));
        if math3::length(n1) < 1e-4 {
            n1 = math3::perpendicular(dn);
        }
        let n1 = math3::normalize(n1);
        let cos_a = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0);
        let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
        let kdir = math3::add(math3::scale(dn, cos_a), math3::scale(math3::cross(n1, dn), sin_a * leg.side));
        let knee = math3::add(hip, math3::scale(kdir, l1));
        let ankle = math3::add(hip, math3::scale(dn, d));
        let q_thigh = qframe(leg.a, leg.n0, kdir, n1);
        let q_shin = qframe(leg.b, leg.n0, math3::sub(ankle, knee), n1);
        f.d[leg.thigh].r = qnorm(qmul(qconj(qp), q_thigh));
        f.d[leg.shin].r = qnorm(qmul(qconj(q_thigh), q_shin));
        if let Some(foot) = leg.foot {
            // Foot model rotation = qp_model · q_shin(parent space) · local.
            f.d[foot].r = qnorm(qmul(qconj(qmul(qp_model, q_shin)), q_foot_model));
        }
    }
    worst
}

/// Converts desired model-space transforms of every bone into rest-relative deltas.
fn deltas_from_globals(skel: &Skeleton, g: &[Affine]) -> Vec<Transform> {
    (0..skel.len())
        .map(|b| {
            let local = match skel.parents[b] {
                Some(p) => inverse(&g[p as usize]).unwrap_or(Affine::IDENTITY).then_after(&g[b]),
                None => g[b],
            };
            let s = [math3::length(local.cols[0]), math3::length(local.cols[1]), math3::length(local.cols[2])];
            Transform { t: math3::sub(local.t, skel.rest[b].t), r: rotation_of(&local), s }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Kinds

struct Out {
    looping: LoopMode,
    distance: f32,
    events: Vec<Event>,
    curves: Vec<Curve>,
    frames: Vec<Frame>,
}

fn generate_inner(spec: &Json, skel: &Skeleton, set: Option<&AnimSet>) -> Result<(Clip, u64), AnimError> {
    if !skel.model_aligned() {
        return Err(err("", AnimErrorCode::InvalidRig, "generators need model-aligned rest frames (recipe rigs)"));
    }
    // `body: "auto"`: the parts the bone names suggest, under anything given explicitly.
    let auto;
    let spec = match spec.get("body") {
        Some(Json::Str(a)) if a == "auto" => {
            let mut j = spec.clone();
            if let Json::Obj(pairs) = &mut j {
                pairs.retain(|(k, _)| k != "body");
            }
            for (k, v) in auto_body(skel) {
                if j.get(k).is_none() {
                    j.set(k, v);
                }
            }
            auto = j;
            &auto
        }
        Some(_) => return Err(err("body", AnimErrorCode::WrongType, "\"body\": \"auto\" (or leave it out and name the parts)")),
        None => spec,
    };
    let mut o = Obj::new(spec, "", "a generator spec")?;
    let name = o.req_str("name")?.to_string();
    let kind = o.req_str("kind")?;
    // A chain takes its source's duration and loop mode unless told otherwise.
    let source = if kind == "chain" {
        let sp = o.at("source");
        let src = o.req_str("source")?;
        let set = set.ok_or_else(|| err(&sp, AnimErrorCode::MissingField, "a chain builds on a clip of its anim set: generate it inside the set"))?;
        let id = set.id(src).ok_or_else(|| err(&sp, AnimErrorCode::UnknownName, format!("no clip '{src}' before this one in the set (clips: {})", set.clips().map(|(_, c)| c.name.as_str()).collect::<Vec<_>>().join(", "))))?;
        Some((set, id))
    } else {
        None
    };
    let duration = match (o.get("duration"), source) {
        (Some(d), _) => num(d, &o.at("duration"), 0.05, 60.0)?,
        (None, Some((set, id))) => set.clip(id).duration.max(0.05),
        (None, None) if kind == "pose" => 1.0,
        (None, None) => return Err(err("duration", AnimErrorCode::MissingField, "missing \"duration\"")),
    };
    // At least 48 keys per clip, so fast cycles (a 0.5 s sprint) stay smooth and planted.
    let rate = o.num("rate", (48.0 / duration).clamp(30.0, 120.0), 4.0, 120.0)?;
    let default_loop = match source {
        Some((set, id)) => set.clip(id).looping,
        None if matches!(kind, "gait" | "idle" | "hover" | "wobble" | "pose" | "breathe") => LoopMode::Loop,
        None => LoopMode::Once,
    };
    let looping = match o.str("loop")? {
        None => default_loop,
        Some("loop") => LoopMode::Loop,
        Some("once") => LoopMode::Once,
        Some("ping_pong") => LoopMode::PingPong,
        Some(other) => return Err(err("loop", AnimErrorCode::UnknownName, format!("unknown loop mode '{other}'"))),
    };
    let n = (specula_core::dmath::round(duration * rate) as usize).max(2);
    // Generation work is bounded before any of it is done (it runs on whatever thread loads the
    // set, the engine's included).
    let work = (n as u64 + 1) * skel.len() as u64;
    if work > MAX_CLIP_WORK {
        return Err(err("duration", AnimErrorCode::Limit, format!("generating this clip takes {} frames × {} bones = {work}; at most {MAX_CLIP_WORK}: shorten it or lower its \"rate\"", n + 1, skel.len())).with_numbers(work as f64, MAX_CLIP_WORK as f64));
    }
    if let Some(set) = set {
        if set.generated + work > MAX_SET_WORK {
            return Err(err("", AnimErrorCode::Limit, format!("the anim set's generated clips would take {} frames × bones in all with this one; at most {MAX_SET_WORK}: fewer, shorter or sparser generated clips", set.generated + work)).with_numbers((set.generated + work) as f64, MAX_SET_WORK as f64));
        }
    }
    let times: Vec<f32> = (0..=n).map(|k| if k == n { duration } else { duration * k as f32 / n as f32 }).collect();
    let body = parse_body(&mut o, skel, looping == LoopMode::Loop)?;
    let mut out = match kind {
        "gait" | "idle" => gait(&mut o, skel, &body, &times, duration, kind == "idle")?,
        "hover" => hover(&mut o, skel, &body, &times, duration)?,
        "swing" => swing(&mut o, skel, &body, &times, duration)?,
        "leap" => leap(&mut o, skel, &body, &times, duration)?,
        "collapse" => collapse(&mut o, skel, &body, &times, duration)?,
        "keys" | "pose" => keyed(&mut o, skel, &body, &times, duration, kind == "pose", looping)?,
        "recoil" => recoil(&mut o, skel, &body, &times, duration)?,
        "breathe" => breathe(&mut o, skel, &body, &times, duration)?,
        "chain" => {
            let (set, id) = source.expect("chain has a source");
            chain(&mut o, set, id, &times, duration, looping)?
        }
        "wobble" => {
            let frames = times
                .iter()
                .map(|&t| {
                    let mut f = Frame::new(skel.len(), body.legs.len());
                    apply_wobbles(&mut f, &body, t / duration);
                    f
                })
                .collect();
            Out { looping: LoopMode::Loop, distance: 0.0, events: Vec::new(), curves: Vec::new(), frames }
        }
        other => return Err(err("kind", AnimErrorCode::UnknownName, format!("unknown generator kind '{other}' ({})", KINDS.join(", ")))),
    };
    out.looping = looping;
    if let Some(items) = o.arr("events")? {
        for (i, ev) in items.iter().enumerate() {
            let p = format!("events[{i}]");
            let mut eo = Obj::new(ev, &p, "an event")?;
            let e = Event { name: eo.req_str("name")?.to_string(), time: eo.req_num("time", 0.0, duration)?, data: eo.str("data")?.unwrap_or("").to_string() };
            eo.finish("name, time, data")?;
            out.events.push(e);
        }
    }
    o.finish("name, kind, duration, rate, loop, events, body, pelvis, spine, head, legs, arms, wobble and the kind's parameters")?;
    if looping == LoopMode::Loop {
        // A seamless loop: the last key repeats the first exactly.
        let first = out.frames[0].clone();
        *out.frames.last_mut().unwrap() = first;
    }
    let mut clip = Clip {
        name,
        duration,
        looping,
        distance: out.distance,
        tracks: bake(skel, &times, &out.frames),
        curves: out.curves,
        events: out.events,
    };
    clip.sort_events();
    clip.validate()?;
    Ok((clip, work))
}

/// Turns per-key deltas into cubic tracks, skipping channels that never leave rest.
fn bake(skel: &Skeleton, times: &[f32], frames: &[Frame]) -> Vec<Track> {
    let mut tracks = Vec::new();
    for b in 0..skel.len() {
        let moved = frames.iter().any(|f| f.d[b].t.iter().any(|v| v.abs() > 1e-6));
        let turned = frames.iter().any(|f| (1.0 - f.d[b].r[3].abs()) > 1e-9 || f.d[b].r[..3].iter().any(|v| v.abs() > 1e-6));
        let scaled = frames.iter().any(|f| f.d[b].s.iter().any(|v| (v - 1.0).abs() > 1e-6));
        let keys = |values: Vec<[f32; 4]>| Keys { interp: Interp::Cubic, times: times.to_vec(), values };
        if moved {
            tracks.push(Track { bone: skel.names[b].clone(), channel: Channel::Translation, keys: keys(frames.iter().map(|f| [f.d[b].t[0], f.d[b].t[1], f.d[b].t[2], 0.0]).collect()) });
        }
        if turned {
            // Keep consecutive keys in one hemisphere so cubic tangents follow the short way.
            let mut prev = QID;
            let values = frames
                .iter()
                .map(|f| {
                    let q = math::qalign(prev, qnorm(f.d[b].r));
                    prev = q;
                    q
                })
                .collect();
            tracks.push(Track { bone: skel.names[b].clone(), channel: Channel::Rotation, keys: keys(values) });
        }
        if scaled {
            tracks.push(Track { bone: skel.names[b].clone(), channel: Channel::Scale, keys: keys(frames.iter().map(|f| [f.d[b].s[0], f.d[b].s[1], f.d[b].s[2], 0.0]).collect()) });
        }
    }
    tracks
}

/// The most the pelvis may be lowered: 60% of its height above the planted ankles.
fn max_crouch(skel: &Skeleton, body: &Body) -> f32 {
    let ground = body.legs.iter().map(|l| l.plant[1]).fold(f32::MAX, f32::min);
    body.pelvis.map_or(0.0, |p| 0.6 * (skel.heads[p][1] - ground).max(0.0))
}

/// Solves every key; with `auto_crouch`, lowers the pelvis until no leg needs to stretch past
/// [`MAX_REACH`] over the whole clip. When that can't work (the crouch would pass
/// [`max_crouch`], or lowering stops helping), fails with `OutOfRange` at `path`, giving the
/// reach the motion needs against the reach the legs have.
fn solve_all(skel: &Skeleton, body: &Body, make: &dyn Fn(f32, f32) -> Frame, times: &[f32], auto_crouch: bool, path: &str) -> Result<(Vec<Frame>, f32), AnimError> {
    let mut crouch = 0.0f32;
    let total: f32 = body.legs.iter().map(|l| math3::length(l.a) + math3::length(l.b)).fold(0.0, f32::max);
    let limit = max_crouch(skel, body);
    let mut prev = f32::MAX;
    for _ in 0..16 {
        let mut frames: Vec<Frame> = times.iter().map(|&t| make(t, crouch)).collect();
        let worst = frames.iter_mut().map(|f| solve(skel, body, f)).fold(0.0f32, f32::max);
        if !auto_crouch || body.pelvis.is_none() || worst <= MAX_REACH + 1e-4 {
            return Ok((frames, crouch));
        }
        let next = crouch + (worst - MAX_REACH) * total * 1.15 + 1e-3;
        if next > limit || worst >= prev - 1e-5 {
            return Err(err(
                path,
                AnimErrorCode::OutOfRange,
                format!(
                    "the legs would need to reach {:.2} m but are {:.2} m long, even with the hips lowered {:.2} m: shorten the stride (or raise the duty / lower the lift)",
                    worst * total,
                    total,
                    crouch
                ),
            )
            .with_numbers((worst * total) as f64, (MAX_REACH * total) as f64));
        }
        prev = worst;
        crouch = next;
    }
    Err(err(path, AnimErrorCode::OutOfRange, "the legs can't reach the foot targets").with_numbers(prev as f64 * total as f64, (MAX_REACH * total) as f64))
}

fn footstep_events(body: &Body, skel: &Skeleton, duration: f32, name: &str, phase_offset: f32) -> Vec<Event> {
    body.legs
        .iter()
        .map(|l| Event { name: name.to_string(), time: frac(l.phase + phase_offset) * duration, data: skel.names[l.foot.unwrap_or(l.shin)].clone() })
        .collect()
}

/// A target pose for some bones: Euler rotation, translation offset and uniform scale.
#[derive(Clone, Debug)]
struct PoseKey {
    bone: usize,
    rot: V3,
    mov: V3,
    scale: f32,
}

fn parse_pose(o: &mut Obj, k: &str, skel: &Skeleton) -> Result<Vec<PoseKey>, AnimError> {
    let p = o.at(k);
    let Some(v) = o.get(k) else { return Ok(Vec::new()) };
    let pairs = v.as_object().ok_or_else(|| err(&p, AnimErrorCode::WrongType, "expected {\"bone\": {\"rot\": [x, y, z], \"move\": [x, y, z], \"scale\": s}}"))?;
    let mut out: Vec<PoseKey> = Vec::new();
    for (name, spec) in pairs {
        let bp = format!("{p}.{name}");
        let bone = bone_of(skel, name, &bp)?;
        if out.iter().any(|q| q.bone == bone) {
            return Err(err(&bp, AnimErrorCode::Duplicate, "bone listed twice"));
        }
        let mut bo = Obj::new(spec, &bp, "a bone pose")?;
        let key = PoseKey { bone, rot: bo.vec("rot", [0.0; 3], -720.0, 720.0)?, mov: bo.vec("move", [0.0; 3], -100.0, 100.0)?, scale: bo.num("scale", 1.0, 0.0, 10.0)? };
        bo.finish("rot, move, scale")?;
        out.push(key);
    }
    Ok(out)
}

/// Applies `Σ w_k · pose_k` (Euler and offsets blended linearly, scales too).
fn apply_poses(f: &mut Frame, poses: &[(&[PoseKey], f32)]) {
    let mut acc: Vec<(usize, V3, V3, f32)> = Vec::new();
    for (pose, w) in poses {
        for k in pose.iter() {
            let i = match acc.iter().position(|a| a.0 == k.bone) {
                Some(i) => i,
                None => {
                    acc.push((k.bone, [0.0; 3], [0.0; 3], 0.0));
                    acc.len() - 1
                }
            };
            acc[i].1 = math3::add(acc[i].1, math3::scale(k.rot, *w));
            acc[i].2 = math3::add(acc[i].2, math3::scale(k.mov, *w));
            acc[i].3 += (k.scale - 1.0) * w;
        }
    }
    for (b, rot, mov, sc) in acc {
        f.mov(b, mov);
        f.rot(b, qeuler(rot));
        if sc != 0.0 {
            f.scale(b, [1.0 + sc; 3]);
        }
    }
}

fn planted(f: &mut Frame, body: &Body) {
    for (i, l) in body.legs.iter().enumerate() {
        f.feet[i] = Some((l.plant, 0.0));
    }
}

/// Reads a `[x, y, z]` from JSON (helper for tests and tools).
pub fn vec3(v: &Json, path: &str) -> Result<V3, AnimError> {
    vecn(v, path, -1e6, 1e6)
}

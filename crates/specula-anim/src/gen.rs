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

use specula_model::rig::MAX_BONES;

/// Generates the clip a spec describes, for `skel`. Errors carry paths under `path`. Kinds that
/// build on other clips (`chain`) need [`generate_in`].
pub fn generate(spec: &Json, path: &str, skel: &Skeleton) -> Result<Clip, AnimError> {
    generate_inner(spec, skel, None).map_err(|e| e.under(path))
}

/// [`generate`] for a clip of `set`: generators may build on the set's clips.
pub fn generate_in(spec: &Json, path: &str, set: &AnimSet) -> Result<Clip, AnimError> {
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

fn generate_inner(spec: &Json, skel: &Skeleton, set: Option<&AnimSet>) -> Result<Clip, AnimError> {
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
    Ok(clip)
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

fn gait(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32, idle: bool) -> Result<Out, AnimError> {
    let stride = if idle { 0.0 } else { o.req_num("stride", 0.0, 50.0)? };
    let duty = o.num("duty", 0.6, 0.2, 0.9)?;
    // The part of the swing at each end where the foot rises or lands in place (moving with the
    // ground), so it never scrapes forward while its toe is still down. Runs take less.
    let clear = o.num("clear", if duty >= 0.5 { 0.12 } else { 0.06 }, 0.0, 0.4)?;
    let lift = o.num("lift", if idle { 0.0 } else { 0.1 }, 0.0, 3.0)?;
    let bounce = o.num("bounce", if idle { 0.006 } else { 0.03 }, 0.0, 2.0)?;
    let sway = o.num("sway", if idle { 0.015 } else { 0.02 }, 0.0, 2.0)?;
    let roll = o.num("roll", if idle { 1.5 } else { 4.0 }, -60.0, 60.0)?;
    let twist = o.num("twist", if idle { 0.0 } else { 6.0 }, -60.0, 60.0)?;
    let lean = o.num("lean", 0.0, -80.0, 80.0)?;
    let nod = o.num("nod", if idle { 1.0 } else { 3.0 }, -45.0, 45.0)?;
    let limp = o.num("limp", 0.0, 0.0, 1.0)?;
    let breathe = o.num("breathe", if idle { 2.5 } else { 0.0 }, 0.0, 30.0)?;
    let breathe_scale = o.num("breathe_scale", if idle { 0.02 } else { 0.0 }, 0.0, 0.3)?;
    let breaths = o.num("breaths", 1.0, 0.0, 32.0)?;
    let look = o.num("look", 0.0, -90.0, 90.0)?;
    let fixed_crouch = match o.get("crouch") {
        Some(v) => Some(num(v, &o.at("crouch"), 0.0, max_crouch(skel, body))?),
        None => None,
    };
    let step_name = match o.get("footstep") {
        None => (!idle).then(|| "footstep".to_string()),
        Some(Json::Bool(false)) => None,
        Some(Json::Str(s)) if !s.is_empty() => Some(s.clone()),
        Some(_) => return Err(err(&o.at("footstep"), AnimErrorCode::WrongType, "an event name, or false for none")),
    };
    if breaths != specula_core::dmath::round(breaths) {
        return Err(err(&o.at("breaths"), AnimErrorCode::OutOfRange, "breaths per loop must be a whole number"));
    }
    let s0 = body.legs.first().map_or(0.0, |l| l.phase);
    let side0 = body.legs.first().map_or(1.0, |l| if l.ankle[0] < 0.0 { -1.0 } else { 1.0 });
    let walk = duty >= 0.5;
    let stance_len = stride * duty;
    let make = |t: f32, crouch: f32| -> Frame {
        let s = t / duration;
        let mut f = Frame::new(skel.len(), body.legs.len());
        let m = s - s0 - duty * 0.5; // 0 at leg 0's mid-stance
        let w1 = 0.5 - 0.5 * cos_turns(m); // 1 at the other leg's mid-stance
        let vert = bounce * cos_turns(2.0 * m) * if walk { 1.0 } else { -1.0 } - limp * bounce * 2.5 * w1 * w1 - crouch;
        let lateral = sway * side0 * cos_turns(m) * (1.0 + 0.8 * limp * w1);
        let roll_z = roll * side0 * cos_turns(m) * (1.0 + limp * w1);
        let yaw = -twist * side0 * sin_turns(m);
        if let Some(p) = body.pelvis {
            f.mov(p, [lateral, vert, 0.0]);
            f.rot(p, qeuler([0.0, yaw, roll_z]));
        }
        let ns = body.spine.len().max(1) as f32;
        let breath = sin_turns(breaths * s);
        for (k, &b) in body.spine.iter().enumerate() {
            let last = k + 1 == body.spine.len();
            let bob = 0.25 * bounce * 40.0 * cos_turns(2.0 * m + 0.1) * if walk { 1.0 } else { -1.0 };
            f.rot(b, qeuler([lean / ns + bob / ns - if last { breathe * breath } else { 0.0 }, -0.9 * yaw / ns, -0.6 * roll_z / ns]));
            if last && breathe_scale > 0.0 {
                f.scale(b, [1.0 + breathe_scale * breath, 1.0 + 0.5 * breathe_scale * breath, 1.0 + breathe_scale * breath]);
            }
        }
        if let Some(h) = body.head {
            f.rot(h, qeuler([-0.5 * lean + nod * cos_turns(2.0 * m - 0.15), 0.4 * yaw + look * sin_turns(s), -0.5 * roll_z]));
        }
        for a in &body.arms {
            let c = cos_turns(s - a.phase - a.lag);
            f.rot(a.upper, qeuler([-a.swing * c, 0.0, a.roll * cos_turns(m - a.lag) * side0]));
            if let Some(fo) = a.fore {
                f.rot(fo, qaxis([1.0, 0.0, 0.0], -a.bend * (0.7 + 0.3 * c)));
            }
        }
        for (i, l) in body.legs.iter().enumerate() {
            let p = frac(s - l.phase);
            let (dz, y, pitch) = if p < duty {
                (stance_len * (0.5 - p / duty), 0.0, 0.0)
            } else {
                let q = (p - duty) / (1.0 - duty);
                let up = specula_core::dmath::sin(std::f32::consts::PI * q);
                // The stance's backward speed (per unit of swing): the foot rises and lands
                // moving with the ground over the first and last `clear` of the swing, and
                // in between a Hermite carries it forward with matching end slopes, so it
                // leaves and meets the ground at zero ground speed.
                let m = -stance_len * (1.0 - duty) / duty;
                let (za, zb) = (-0.5 * stance_len + m * clear, 0.5 * stance_len - m * clear);
                let z = if q < clear {
                    -0.5 * stance_len + m * q
                } else if q > 1.0 - clear {
                    0.5 * stance_len + m * (q - 1.0)
                } else {
                    let span = (1.0 - 2.0 * clear).max(1e-6);
                    let u = (q - clear) / span;
                    let mu = m * span;
                    let (u2, u3) = (u * u, u * u * u);
                    (2.0 * u3 - 3.0 * u2 + 1.0) * za + (u3 - 2.0 * u2 + u) * mu + (-2.0 * u3 + 3.0 * u2) * zb + (u3 - u2) * mu
                };
                // The toe lifts through the swing (dorsiflexion) so it clears the ground.
                (z, lift * l.lift * up, -14.0 * up * (lift * l.lift / 0.1).min(1.0))
            };
            f.feet[i] = Some(([l.plant[0], l.plant[1] + y, l.plant[2] + dz], pitch));
        }
        apply_wobbles(&mut f, body, s);
        f
    };
    let (frames, _crouch) = match fixed_crouch {
        Some(c) => solve_all(skel, body, &|t, _| make(t, c), times, false, "crouch")?,
        None => solve_all(skel, body, &make, times, true, "stride")?,
    };
    let events = match step_name {
        Some(nm) => footstep_events(body, skel, duration, &nm, 0.0),
        None => Vec::new(),
    };
    Ok(Out { looping: LoopMode::Loop, distance: stride, events, curves: Vec::new(), frames })
}

fn hover(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let pelvis = body.pelvis.ok_or_else(|| err("pelvis", AnimErrorCode::MissingField, "hover needs a \"pelvis\" bone to float"))?;
    let bob = o.num("bob", 0.12, 0.0, 5.0)?;
    let bobs = o.num("bobs", 1.0, 1.0, 16.0)?;
    let pitch = o.num("pitch", 4.0, -60.0, 60.0)?;
    let roll = o.num("roll", 5.0, -60.0, 60.0)?;
    let yaw = o.num("yaw", 0.0, -90.0, 90.0)?;
    let lean = o.num("lean", 0.0, -80.0, 80.0)?;
    let distance = o.num("distance", 0.0, 0.0, 100.0)?;
    let (chain, chain_deg, chain_roll, delay) = match o.get("chain") {
        Some(v) => {
            let mut co = Obj::new(v, &o.at("chain"), "a chain")?;
            let bones = bone_list(&mut co, "bones", skel)?;
            let r = (bones, co.num("deg", 8.0, -90.0, 90.0)?, co.num("roll", 4.0, -90.0, 90.0)?, co.num("delay", 0.12, 0.0, 1.0)?);
            co.finish("bones, deg, roll, delay")?;
            r
        }
        None => (Vec::new(), 0.0, 0.0, 0.0),
    };
    if bobs != specula_core::dmath::round(bobs) {
        return Err(err(&o.at("bobs"), AnimErrorCode::OutOfRange, "bobs per loop must be a whole number"));
    }
    let frames = times
        .iter()
        .map(|&t| {
            let s = t / duration;
            let mut f = Frame::new(skel.len(), body.legs.len());
            f.mov(pelvis, [0.0, bob * sin_turns(bobs * s), 0.0]);
            f.rot(pelvis, qeuler([lean + pitch * sin_turns(bobs * s + 0.25), yaw * sin_turns(s), roll * sin_turns(s + 0.1)]));
            for (k, &b) in chain.iter().enumerate() {
                let ph = bobs * s - k as f32 * delay;
                f.rot(b, qeuler([chain_deg * sin_turns(ph), 0.0, chain_roll * cos_turns(s - k as f32 * delay)]));
            }
            if let Some(h) = body.head {
                f.rot(h, qeuler([-0.6 * pitch * sin_turns(bobs * s + 0.25), -0.5 * yaw * sin_turns(s), -0.5 * roll * sin_turns(s + 0.1)]));
            }
            for a in &body.arms {
                let c = sin_turns(bobs * s - a.phase - a.lag);
                f.rot(a.upper, qeuler([-a.swing * c, 0.0, a.roll * cos_turns(s - a.phase)]));
                if let Some(fo) = a.fore {
                    f.rot(fo, qaxis([1.0, 0.0, 0.0], -a.bend * (0.6 + 0.4 * c)));
                }
            }
            apply_wobbles(&mut f, body, s);
            f
        })
        .collect();
    Ok(Out { looping: LoopMode::Loop, distance, events: Vec::new(), curves: Vec::new(), frames })
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

fn swing(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let wu = o.num("windup", 0.35, 0.0, 1.0)?;
    let st = o.num("strike", 0.15, 0.01, 1.0)?;
    let hold = o.num("hold", 0.1, 0.0, 1.0)?;
    if wu + st + hold > 0.99 {
        return Err(err(&o.at("hold"), AnimErrorCode::OutOfRange, "windup + strike + hold must leave time to recover (< 0.99)").with_numbers((wu + st + hold) as f64, 0.99));
    }
    let windup = parse_pose(o, "windup_pose", skel)?;
    let strike = parse_pose(o, "strike_pose", skel)?;
    let event = o.str("event")?.unwrap_or("hit").to_string();
    let frames_make = |t: f32, crouch: f32| -> Frame {
        let q = t / duration;
        let mut f = Frame::new(skel.len(), body.legs.len());
        let (ww, ws) = if q < wu {
            (smoothstep(q / wu), 0.0)
        } else if q < wu + st {
            let u = (q - wu) / st;
            let e = u * u; // accelerate into the blow
            (1.0 - e, e)
        } else if q < wu + st + hold {
            (0.0, 1.0)
        } else {
            let u = (q - wu - st - hold) / (1.0 - wu - st - hold);
            (0.0, 1.0 - smoothstep(u))
        };
        apply_poses(&mut f, &[(&windup, ww), (&strike, ws)]);
        if let Some(p) = body.pelvis {
            f.mov(p, [0.0, -crouch, 0.0]);
        }
        planted(&mut f, body);
        apply_wobbles(&mut f, body, q);
        f
    };
    let (frames, _) = solve_all(skel, body, &frames_make, times, true, "strike_pose")?;
    let events = vec![Event { name: event, time: (wu + st) * duration, data: String::new() }];
    Ok(Out { looping: LoopMode::Once, distance: 0.0, events, curves: Vec::new(), frames })
}

fn leap(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let distance = o.num("distance", 3.0, 0.0, 50.0)?;
    let height = o.num("height", 0.8, 0.0, 20.0)?;
    let crouch_amt = o.num("crouch", 0.2, 0.0, 5.0)?;
    let tuck = o.num("tuck", 0.35, 0.0, 5.0)?;
    let split = o.num("split", 0.25, 0.0, 5.0)?;
    let ant = o.num("anticipation", 0.25, 0.05, 0.6)?;
    let fl = o.num("flight", 0.45, 0.1, 0.8)?;
    let lean = o.num("lean", 20.0, -80.0, 80.0)?;
    if ant + fl > 0.95 {
        return Err(err(&o.at("flight"), AnimErrorCode::OutOfRange, "anticipation + flight must leave time to land (< 0.95)"));
    }
    let ns = body.spine.len().max(1) as f32;
    let make = |t: f32, crouch: f32| -> Frame {
        let q = t / duration;
        let mut f = Frame::new(skel.len(), body.legs.len());
        let (z, y, dip, lean_w, arm, air) = if q < ant {
            let e = smoothstep(q / ant);
            (0.0, 0.0, crouch_amt * e, 0.5 * e, e, 0.0)
        } else if q < ant + fl {
            let u = (q - ant) / fl;
            let rise = (1.0 - smoothstep(u / 0.25)) * crouch_amt;
            (distance * u, height * 4.0 * u * (1.0 - u), rise, 0.5 + 0.5 * specula_core::dmath::sin(std::f32::consts::PI * u), 1.0 - 2.0 * smoothstep(u / 0.4), specula_core::dmath::sin(std::f32::consts::PI * u))
        } else {
            let u = (q - ant - fl) / (1.0 - ant - fl);
            (distance, 0.0, 0.8 * crouch_amt * specula_core::dmath::sin(std::f32::consts::PI * u.min(1.0)) * (1.0 - u), 0.5 * (1.0 - smoothstep(u)), -(1.0 - smoothstep(u)), 0.0)
        };
        f.mov(0, [0.0, y, z]);
        if let Some(p) = body.pelvis {
            f.mov(p, [0.0, -dip - crouch, 0.0]);
        }
        for &b in &body.spine {
            f.rot(b, qaxis([1.0, 0.0, 0.0], lean * lean_w / ns));
        }
        if let Some(h) = body.head {
            f.rot(h, qaxis([1.0, 0.0, 0.0], -0.6 * lean * lean_w));
        }
        for a in &body.arms {
            // Back during the crouch (+), thrown forward in the air (−).
            f.rot(a.upper, qeuler([a.swing * arm, 0.0, a.roll * air]));
            if let Some(fo) = a.fore {
                f.rot(fo, qaxis([1.0, 0.0, 0.0], -a.bend * (0.5 + 0.5 * air)));
            }
        }
        for (i, l) in body.legs.iter().enumerate() {
            let fwd = if i % 2 == 0 { split } else { -split };
            f.feet[i] = Some(([l.plant[0], l.plant[1] + tuck * air, l.plant[2] + fwd * air], -25.0 * air));
        }
        apply_wobbles(&mut f, body, q);
        f
    };
    let (frames, _) = solve_all(skel, body, &make, times, true, "distance")?;
    let mut events = vec![Event { name: "takeoff".into(), time: ant * duration, data: String::new() }, Event { name: "land".into(), time: (ant + fl) * duration, data: String::new() }];
    events.extend(body.legs.iter().map(|l| Event { name: "footstep".into(), time: (ant + fl) * duration, data: skel.names[l.foot.unwrap_or(l.shin)].clone() }));
    Ok(Out { looping: LoopMode::Once, distance, events, curves: Vec::new(), frames })
}

fn collapse(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let style = o.str("style")?.unwrap_or("topple").to_string();
    let height = skel.heads.iter().chain(skel.tails.iter()).map(|p| p[1]).fold(0.0f32, f32::max).max(0.1);
    let hip_h = body.pelvis.map_or(0.5 * height, |p| skel.heads[p][1]);
    let drop = o.num("drop", 0.45 * hip_h, 0.0, 100.0)?;
    let curl = o.num("curl", 50.0, -180.0, 180.0)?;
    let fall = o.num("fall", 82.0, 0.0, 180.0)?;
    let lie = o.num("lie", 0.12 * height, 0.0, 100.0)?;
    let reach = o.num("reach", 0.0, -180.0, 180.0)?;
    let dir = match o.str("direction")?.unwrap_or("forward") {
        "forward" => ([1.0, 0.0, 0.0], 1.0),
        "back" => ([1.0, 0.0, 0.0], -1.0),
        "left" => ([0.0, 0.0, 1.0], -1.0),
        "right" => ([0.0, 0.0, 1.0], 1.0),
        other => return Err(err(&o.at("direction"), AnimErrorCode::UnknownName, format!("direction '{other}' (forward, back, left, right)"))),
    };
    let sink = o.num("sink", 0.5, 0.0, 100.0)?;
    let shrink = o.num("shrink", 0.35, 0.02, 1.0)?;
    let rise = o.num("rise", 1.0, -100.0, 100.0)?;
    let spin = o.num("spin", 270.0, -3600.0, 3600.0)?;
    let spread = o.num("spread", 2.5, 0.0, 50.0)?;
    let seed = o.num("seed", 1.0, 0.0, 1e9)? as u64;
    let ns = body.spine.len().max(1) as f32;
    let bend_body = |f: &mut Frame, w: f32, crouch_drop: f32| {
        if let Some(p) = body.pelvis {
            f.mov(p, [0.0, -crouch_drop, 0.0]);
        }
        for &b in &body.spine {
            f.rot(b, qaxis([1.0, 0.0, 0.0], curl * w / ns));
        }
        if let Some(h) = body.head {
            f.rot(h, qaxis([1.0, 0.0, 0.0], 0.6 * curl * w));
        }
    };
    let mut events = Vec::new();
    let mut curves = Vec::new();
    let frames = match style.as_str() {
        "topple" | "crumble" => {
            let crumble = style == "crumble";
            let (buckle_end, fall_end) = if crumble { (0.35, 0.55) } else { (0.3, 0.72) };
            let fall_deg = if crumble { fall.min(40.0) } else { fall };
            let make = |t: f32, crouch: f32| -> Frame {
                let q = t / duration;
                let mut f = Frame::new(skel.len(), body.legs.len());
                let buckle = smoothstep(q / buckle_end);
                let u = ((q - buckle_end) / (fall_end - buckle_end)).clamp(0.0, 1.0);
                // Gravity: the angle grows with the square of time, then a small bounce.
                let settle = ((q - fall_end) / (1.0 - fall_end)).clamp(0.0, 1.0);
                let bounce = 5.0 * specula_core::dmath::sin(std::f32::consts::PI * settle.min(0.5) * 2.0) * (1.0 - settle);
                let angle = fall_deg * u * u - if q > fall_end { bounce } else { 0.0 };
                f.mov(0, [0.0, lie * (angle / fall_deg.max(1e-3)).clamp(0.0, 1.0), 0.0]);
                f.rot(0, qaxis(dir.0, dir.1 * angle));
                bend_body(&mut f, buckle + 0.5 * u, drop * buckle + crouch);
                for a in &body.arms {
                    // Arms fly up as the body drops, then splay.
                    f.rot(a.upper, qeuler([-a.swing * (buckle - 0.6 * u) - reach * u, 0.0, a.roll * u]));
                    if let Some(fo) = a.fore {
                        f.rot(fo, qaxis([1.0, 0.0, 0.0], -a.bend * buckle));
                    }
                }
                if crumble {
                    let c = smoothstep(((q - fall_end) / (1.0 - fall_end)).clamp(0.0, 1.0));
                    // The whole body shrinks towards the ground and sinks into it.
                    f.mov(0, [0.0, -sink * c, 0.0]);
                    f.scale(0, [1.0 - (1.0 - shrink) * c; 3]);
                }
                planted(&mut f, body);
                apply_wobbles(&mut f, body, q);
                f
            };
            events.push(Event { name: "impact".into(), time: fall_end * duration, data: String::new() });
            if crumble {
                events.push(Event { name: "crumble".into(), time: fall_end * duration, data: String::new() });
            }
            solve_all(skel, body, &make, times, false, "drop")?.0
        }
        "scatter" => {
            let stagger = 0.15f32;
            let make = |t: f32| -> Frame {
                let q = (t / duration).min(stagger) / stagger;
                let mut f = Frame::new(skel.len(), body.legs.len());
                bend_body(&mut f, -0.4 * specula_core::dmath::sin(std::f32::consts::PI * q), 0.05 * hip_h * q);
                planted(&mut f, body);
                f
            };
            let mut base = make(stagger * duration);
            solve(skel, body, &mut base);
            let g0 = fk(skel, &base.d);
            // Each bone (not the root) gets a seeded launch velocity and spin.
            let mut rng = Rng::new(seed, 7);
            let launch: Vec<(V3, V3, f32)> = (0..skel.len())
                .map(|b| {
                    let ang = rng.range_f32(0.0, 1.0);
                    let sp = spread * rng.range_f32(0.4, 1.0);
                    let up = rng.range_f32(1.0, 3.0);
                    let axis = math3::normalize([rng.range_f32(-1.0, 1.0), rng.range_f32(-1.0, 1.0), rng.range_f32(-1.0, 1.0)]);
                    let w = rng.range_f32(180.0, 720.0) * if b % 2 == 0 { 1.0 } else { -1.0 };
                    ([sp * cos_turns(ang), up, sp * sin_turns(ang)], axis, w)
                })
                .collect();
            events.push(Event { name: "shatter".into(), time: stagger * duration, data: String::new() });
            times
                .iter()
                .map(|&t| {
                    if t <= stagger * duration {
                        let mut f = make(t);
                        solve(skel, body, &mut f);
                        return f;
                    }
                    let tau = t - stagger * duration;
                    let mut g = g0.clone();
                    for b in 1..skel.len() {
                        let (v, axis, w) = launch[b];
                        let p0 = g0[b].t;
                        let floor = 0.06f32;
                        // Ballistic flight until the joint reaches the floor, then it rests.
                        let land = {
                            let (a, bb, c) = (-4.9f32, v[1], p0[1] - floor);
                            let disc = (bb * bb - 4.0 * a * c).max(0.0);
                            ((-bb - disc.sqrt()) / (2.0 * a)).max(0.0)
                        };
                        let tt = tau.min(land);
                        let off = [v[0] * tt, v[1] * tt - 4.9 * tt * tt, v[2] * tt];
                        let rm = Affine { cols: math::qcols(qaxis(axis, w * tt)), t: [0.0; 3] };
                        // Spin about the joint, then move it.
                        let about = Affine::translate(math3::add(p0, off)).then_after(&rm.then_after(&Affine::translate(math3::scale(p0, -1.0))));
                        g[b] = about.then_after(&g0[b]);
                    }
                    let mut f = Frame::new(skel.len(), body.legs.len());
                    f.d = deltas_from_globals(skel, &g);
                    f
                })
                .collect()
        }
        "fade" => {
            curves.push(Curve { name: "opacity".into(), keys: Keys { interp: Interp::Linear, times: vec![0.0, 0.3 * duration, duration], values: vec![[1.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]] } });
            events.push(Event { name: "vanish".into(), time: duration, data: String::new() });
            let pelvis = body.pelvis.unwrap_or(0);
            times
                .iter()
                .map(|&t| {
                    let q = t / duration;
                    let e = smoothstep(q);
                    let mut f = Frame::new(skel.len(), body.legs.len());
                    f.mov(pelvis, [0.0, rise * e, 0.0]);
                    f.rot(pelvis, qaxis([0.0, 1.0, 0.0], spin * q * q));
                    f.scale(pelvis, [1.0 - (1.0 - shrink) * e; 3]);
                    bend_body(&mut f, -0.5 * e, 0.0);
                    for a in &body.arms {
                        f.rot(a.upper, qeuler([-a.swing * e, 0.0, a.roll * e]));
                    }
                    apply_wobbles(&mut f, body, q);
                    f
                })
                .collect()
        }
        other => return Err(err(&o.at("style"), AnimErrorCode::UnknownName, format!("collapse style '{other}' (topple, crumble, scatter, fade)"))),
    };
    Ok(Out { looping: LoopMode::Once, distance: 0.0, events, curves, frames })
}

/// `keys` (poses at times) and `pose` (one held pose).
fn keyed(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32, single: bool, looping: LoopMode) -> Result<Out, AnimError> {
    let mut keys: Vec<(f32, Vec<PoseKey>)> = Vec::new();
    let interp = if single {
        let pose = parse_pose(o, "pose", skel)?;
        if pose.is_empty() {
            return Err(err(&o.at("pose"), AnimErrorCode::MissingField, "a pose names bones: {\"bone\": {\"rot\": [x, y, z], \"move\": [x, y, z], \"scale\": s}}"));
        }
        keys.push((0.0, pose));
        Interp::Step
    } else {
        let interp = match o.str("interp")? {
            None | Some("cubic") => Interp::Cubic,
            Some("linear") => Interp::Linear,
            Some("step") => Interp::Step,
            Some(other) => return Err(err(&o.at("interp"), AnimErrorCode::UnknownName, format!("unknown interp '{other}' (step, linear, cubic)"))),
        };
        let kp = o.at("keys");
        let items = capped(o, "keys", MAX_POSE_KEYS)?.ok_or_else(|| err(&kp, AnimErrorCode::MissingField, "missing \"keys\": [{time, pose}, ...]"))?;
        if items.is_empty() {
            return Err(err(&kp, AnimErrorCode::MissingField, "at least one key"));
        }
        for (i, k) in items.iter().enumerate() {
            let p = format!("{kp}[{i}]");
            let mut ko = Obj::new(k, &p, "a key")?;
            let t = ko.req_num("time", 0.0, duration)?;
            if let Some((prev, _)) = keys.last() {
                if t <= *prev {
                    return Err(err(&ko.at("time"), AnimErrorCode::NotIncreasing, format!("{t} s is not after {prev} s")));
                }
            }
            let pose = parse_pose(&mut ko, "pose", skel)?;
            ko.finish("time, pose")?;
            keys.push((t, pose));
        }
        interp
    };
    // A looping clip ends where it starts; a once clip holds its last key.
    let closed = looping == LoopMode::Loop && keys.len() > 1;
    let key_at = |i: isize| -> (f32, &[PoseKey]) {
        let n = keys.len() as isize;
        if closed {
            let k = i.rem_euclid(n) as usize;
            let wraps = i.div_euclid(n) as f32;
            (keys[k].0 + wraps * duration, &keys[k].1)
        } else {
            let k = i.clamp(0, n - 1) as usize;
            (keys[k].0, &keys[k].1)
        }
    };
    let make = |t: f32, crouch: f32| -> Frame {
        let mut f = Frame::new(skel.len(), body.legs.len());
        // The key span holding `t` (wrapping for loops).
        let n = keys.len() as isize;
        let mut i = keys.iter().rposition(|(kt, _)| *kt <= t).map_or(-1, |i| i as isize);
        if !closed && i < 0 {
            i = 0;
        }
        let (t0, p0) = key_at(i);
        let (t1, p1) = key_at(i + 1);
        let u = if t1 > t0 && (closed || i + 1 < n) { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 0.0 };
        match interp {
            Interp::Step => apply_poses(&mut f, &[(p0, 1.0)]),
            Interp::Linear => apply_poses(&mut f, &[(p0, 1.0 - u), (p1, u)]),
            Interp::Cubic => {
                // Catmull–Rom weights over the four keys around the span (uniform).
                let (_, pm) = key_at(i - 1);
                let (_, p2) = key_at(i + 2);
                let (u2, u3) = (u * u, u * u * u);
                let w = [-0.5 * u3 + u2 - 0.5 * u, 1.5 * u3 - 2.5 * u2 + 1.0, -1.5 * u3 + 2.0 * u2 + 0.5 * u, 0.5 * u3 - 0.5 * u2];
                apply_poses(&mut f, &[(pm, w[0]), (p0, w[1]), (p1, w[2]), (p2, w[3])]);
            }
        }
        if let Some(p) = body.pelvis {
            f.mov(p, [0.0, -crouch, 0.0]);
        }
        planted(&mut f, body);
        apply_wobbles(&mut f, body, t / duration);
        f
    };
    let (frames, _) = solve_all(skel, body, &make, times, !body.legs.is_empty(), "keys")?;
    Ok(Out { looping, distance: 0.0, events: Vec::new(), curves: Vec::new(), frames })
}

/// A hit or kick: rises to the pose, then a damped spring brings it back.
fn recoil(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let pose = parse_pose(o, "pose", skel)?;
    if pose.is_empty() {
        return Err(err(&o.at("pose"), AnimErrorCode::MissingField, "a recoil needs the \"pose\" it snaps to"));
    }
    let attack = o.num("attack", 0.06, 0.0, duration * 0.5)?;
    let freq = o.num("frequency", 3.0, 0.1, 30.0)?;
    let zeta = o.num("damping", 0.35, 0.0, 0.99)?;
    let event = o.str("event")?.unwrap_or("recoil").to_string();
    let omega = std::f32::consts::TAU * freq;
    let omega_d = omega * (1.0 - zeta * zeta).sqrt();
    let make = |t: f32, crouch: f32| -> Frame {
        let mut f = Frame::new(skel.len(), body.legs.len());
        let w = if t < attack {
            smoothstep(t / attack.max(1e-4))
        } else {
            let tau = t - attack;
            specula_core::dmath::exp(-zeta * omega * tau) * specula_core::dmath::cos(omega_d * tau)
        };
        // Fade the tail out so the clip ends exactly at rest.
        let end = 1.0 - smoothstep((t / duration - 0.8) / 0.2);
        apply_poses(&mut f, &[(&pose, w * end)]);
        if let Some(p) = body.pelvis {
            f.mov(p, [0.0, -crouch, 0.0]);
        }
        planted(&mut f, body);
        apply_wobbles(&mut f, body, t / duration);
        f
    };
    let (frames, _) = solve_all(skel, body, &make, times, !body.legs.is_empty(), "pose")?;
    Ok(Out { looping: LoopMode::Once, distance: 0.0, events: vec![Event { name: event, time: 0.0, data: String::new() }], curves: Vec::new(), frames })
}

/// Breathing only, for an additive layer.
fn breathe(o: &mut Obj, skel: &Skeleton, body: &Body, times: &[f32], duration: f32) -> Result<Out, AnimError> {
    let chest = *body.spine.last().ok_or_else(|| err("spine", AnimErrorCode::MissingField, "breathe needs a \"spine\" (its last bone breathes)"))?;
    let deg = o.num("deg", 3.0, -30.0, 30.0)?;
    let swell = o.num("scale", 0.03, 0.0, 0.3)?;
    let breaths = o.num("breaths", 1.0, 1.0, 32.0)?;
    if breaths != specula_core::dmath::round(breaths) {
        return Err(err(&o.at("breaths"), AnimErrorCode::OutOfRange, "breaths per loop must be a whole number"));
    }
    let frames = times
        .iter()
        .map(|&t| {
            let mut f = Frame::new(skel.len(), body.legs.len());
            // In through the first half (chest lifts back and swells), out through the second.
            let k = 0.5 - 0.5 * cos_turns(breaths * t / duration);
            f.rot(chest, qaxis([1.0, 0.0, 0.0], -deg * k));
            if swell > 0.0 {
                f.scale(chest, [1.0 + swell * k, 1.0 + 0.5 * swell * k, 1.0 + swell * k]);
            }
            if let Some(h) = body.head {
                f.rot(h, qaxis([1.0, 0.0, 0.0], 0.6 * deg * k));
            }
            apply_wobbles(&mut f, body, t / duration);
            f
        })
        .collect();
    Ok(Out { looping: LoopMode::Loop, distance: 0.0, events: Vec::new(), curves: Vec::new(), frames })
}

/// Secondary motion: a spring chain driven by a source clip, baked over it.
fn chain(o: &mut Obj, set: &AnimSet, source: crate::set::ClipId, times: &[f32], duration: f32, looping: LoopMode) -> Result<Out, AnimError> {
    let skel = &set.skeleton;
    let bones = bone_list(o, "bones", skel)?;
    if bones.is_empty() || bones.len() > MAX_CHAIN {
        return Err(err(&o.at("bones"), AnimErrorCode::OutOfRange, format!("a chain simulates 1..={MAX_CHAIN} bones, each the parent of the next")));
    }
    for w in bones.windows(2) {
        child_of(skel, w[1], w[0], &o.at("bones"))?;
    }
    let stiffness = o.num("stiffness", 0.2, 0.0, 1.0)?;
    let damping = o.num("damping", 0.1, 0.0, 1.0)?;
    let gravity = o.num("gravity", 9.8, -50.0, 50.0)?;
    let warmup = o.num("cycles", 3.0, 1.0, 16.0)? as usize;
    let src = set.clip(source);
    let speed = src.speed();
    // Points: each chain bone's head, then the last bone's tail.
    let last = *bones.last().expect("non-empty");
    let mut tail = math3::sub(skel.tails[last], skel.heads[last]);
    if math3::length(tail) < 1e-4 {
        tail = [0.0, -0.1, 0.0];
    }
    let n = bones.len() + 1;
    let point_of = |model: &[Affine], j: usize| -> V3 {
        if j < bones.len() {
            model[bones[j]].t
        } else {
            model[last].apply_point(tail)
        }
    };
    let rest_len: Vec<f32> = {
        let mut m = Vec::new();
        skel.model_space(&skel.rest_pose(), &mut m);
        (1..n).map(|j| math3::length(math3::sub(point_of(&m, j), point_of(&m, j - 1)))).collect()
    };
    let bound = set.track_bones(source).to_vec();
    let sample = |t: f32, pose: &mut crate::skeleton::Pose| crate::blend::sample(skel, src, &bound, src.local_time(t as f64), crate::blend::Base::Rest, pose);
    let dt = if times.len() > 1 { times[1] - times[0] } else { duration };
    let steps = times.len() - 1;
    let total_cycles = if looping == LoopMode::Loop { warmup + 2 } else { 1 };
    let mut pose = skel.rest_pose();
    let mut model: Vec<Affine> = Vec::new();
    sample(0.0, &mut pose);
    skel.model_space(&pose, &mut model);
    let mut x: Vec<V3> = (0..n).map(|j| point_of(&model, j)).collect();
    let mut prev = x.clone();
    // Per recorded cycle: per key, the chain bones' local rotations.
    let mut cycles: Vec<Vec<Vec<Q>>> = Vec::new();
    for c in 0..total_cycles {
        let mut rec: Vec<Vec<Q>> = Vec::with_capacity(times.len());
        for (k, &t) in times.iter().enumerate() {
            if k == steps && c + 1 < total_cycles {
                // The next cycle's first key is this time.
                continue;
            }
            sample(t, &mut pose);
            skel.model_space(&pose, &mut model);
            // The source travels `speed` m/s along +Z: the chain trails in the world.
            let travel = [0.0, 0.0, speed * (c as f32 * duration + t)];
            let target: Vec<V3> = (0..n).map(|j| math3::add(point_of(&model, j), travel)).collect();
            if c == 0 && k == 0 {
                x.clone_from(&target);
                prev.clone_from(&target);
            }
            x[0] = target[0];
            for j in 1..n {
                let v = math3::scale(math3::sub(x[j], prev[j]), 1.0 - damping);
                prev[j] = x[j];
                x[j] = math3::add(math3::add(x[j], v), [0.0, -gravity * dt * dt, 0.0]);
                x[j] = math3::add(x[j], math3::scale(math3::sub(target[j], x[j]), stiffness));
            }
            for j in 1..n {
                let d = math3::sub(x[j], x[j - 1]);
                let l = math3::length(d);
                x[j] = if l > 1e-6 { math3::add(x[j - 1], math3::scale(d, rest_len[j - 1] / l)) } else { target[j] };
            }
            // Turn each chain bone so it points where the simulation put its child point.
            for (j, &b) in bones.iter().enumerate() {
                let have = math3::sub(point_of(&model, j + 1), point_of(&model, j));
                let want = math3::sub(math3::sub(x[j + 1], travel), point_of(&model, j));
                let turn = crate::math::qarc(have, want);
                let g = rotation_of(&model[b]);
                pose.local[b].r = qnorm(qmul(pose.local[b].r, qmul(qconj(g), qmul(turn, g))));
                crate::ik::fk_from(skel, &pose, &mut model, b);
            }
            rec.push(bones.iter().map(|&b| pose.local[b].r).collect());
        }
        cycles.push(rec);
    }
    // Loops: the last cycle, blended at its end into the cycle before it (whose end runs into
    // the last cycle's start), so the clip ends where it begins.
    let rot_at = |k: usize, j: usize| -> Q {
        let last_c = &cycles[cycles.len() - 1];
        if looping != LoopMode::Loop {
            return last_c[k.min(last_c.len() - 1)][j];
        }
        let prev_c = &cycles[cycles.len() - 2];
        let b = if k < last_c.len() { last_c[k][j] } else { cycles[cycles.len() - 1][0][j] };
        let a = if k < prev_c.len() { prev_c[k][j] } else { last_c[0][j] };
        let w = smoothstep((k as f32 / steps as f32 - 0.7) / 0.3);
        crate::math::qnlerp(b, a, w)
    };
    let mut frames = Vec::with_capacity(times.len());
    for (k, &t) in times.iter().enumerate() {
        sample(t, &mut pose);
        let mut f = Frame::new(skel.len(), 0);
        for b in 0..skel.len() {
            let r = skel.rest[b];
            f.d[b] = Transform { t: math3::sub(pose.local[b].t, r.t), r: pose.local[b].r, s: [pose.local[b].s[0] / r.s[0], pose.local[b].s[1] / r.s[1], pose.local[b].s[2] / r.s[2]] };
        }
        for (j, &b) in bones.iter().enumerate() {
            f.d[b].r = rot_at(k, j);
        }
        frames.push(f);
    }
    Ok(Out { looping, distance: src.distance, events: src.events.clone(), curves: src.curves.clone(), frames })
}

/// Reads a `[x, y, z]` from JSON (helper for tests and tools).
pub fn vec3(v: &Json, path: &str) -> Result<V3, AnimError> {
    vecn(v, path, -1e6, 1e6)
}

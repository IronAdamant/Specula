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
//! | `collapse` | death: `topple`, `crumble`, `scatter` (bones fly apart, seeded) or `fade` (rise, spin, shrink, `opacity` curve) | once |
//! | `wobble` | only the `wobble` layers (mechanisms, flames) | loop |
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
    self, cos_turns, frac, inverse, math3, qaxis, qconj, qeuler, qfrom_cols, qframe, qmul, qnorm, qrot, sin_turns, smoothstep, Affine, Transform, V3, Q, QID,
};
use crate::skeleton::Skeleton;

/// Generator kinds.
pub const KINDS: &[&str] = &["gait", "idle", "hover", "swing", "leap", "collapse", "wobble"];

/// Most a leg may stretch, as a fraction of its full length, before the pelvis is lowered.
pub const MAX_REACH: f32 = 0.97;

/// Generates the clip a spec describes, for `skel`. Errors carry paths under `path`.
pub fn generate(spec: &Json, path: &str, skel: &Skeleton) -> Result<Clip, AnimError> {
    generate_inner(spec, skel).map_err(|e| e.under(path))
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

fn bone_list(o: &mut Obj, k: &str, skel: &Skeleton) -> Result<Vec<usize>, AnimError> {
    let p = o.at(k);
    let Some(items) = o.arr(k)? else { return Ok(Vec::new()) };
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

fn parse_body(o: &mut Obj, skel: &Skeleton, looping: bool) -> Result<Body, AnimError> {
    let mut body = Body { pelvis: opt_bone(o, "pelvis", skel)?, spine: bone_list(o, "spine", skel)?, head: opt_bone(o, "head", skel)?, ..Body::default() };
    if let Some(items) = o.arr("legs")? {
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
    if let Some(items) = o.arr("arms")? {
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
    if let Some(items) = o.arr("wobble")? {
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

/// Rotation of an affine's (possibly scaled) linear part.
fn rotation_of(m: &Affine) -> Q {
    qfrom_cols([math3::normalize(m.cols[0]), math3::normalize(m.cols[1]), math3::normalize(m.cols[2])])
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

fn generate_inner(spec: &Json, skel: &Skeleton) -> Result<Clip, AnimError> {
    if !skel.model_aligned() {
        return Err(err("", AnimErrorCode::InvalidRig, "generators need model-aligned rest frames (recipe rigs)"));
    }
    let mut o = Obj::new(spec, "", "a generator spec")?;
    let name = o.req_str("name")?.to_string();
    let kind = o.req_str("kind")?;
    let duration = o.req_num("duration", 0.05, 60.0)?;
    // At least 48 keys per clip, so fast cycles (a 0.5 s sprint) stay smooth and planted.
    let rate = o.num("rate", (48.0 / duration).clamp(30.0, 120.0), 4.0, 120.0)?;
    let default_loop = matches!(kind, "gait" | "idle" | "hover" | "wobble");
    let looping = match o.str("loop")? {
        None => {
            if default_loop {
                LoopMode::Loop
            } else {
                LoopMode::Once
            }
        }
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
    o.finish("name, kind, duration, rate, loop, events, pelvis, spine, head, legs, arms, wobble and the kind's parameters")?;
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

/// Lowers the pelvis until no leg needs to stretch past [`MAX_REACH`] over the whole clip.
fn solve_all(skel: &Skeleton, body: &Body, make: &dyn Fn(f32, f32) -> Frame, times: &[f32], auto_crouch: bool) -> (Vec<Frame>, f32) {
    let mut crouch = 0.0f32;
    let total: f32 = body.legs.iter().map(|l| math3::length(l.a) + math3::length(l.b)).fold(0.0, f32::max);
    for _ in 0..12 {
        let mut frames: Vec<Frame> = times.iter().map(|&t| make(t, crouch)).collect();
        let worst = frames.iter_mut().map(|f| solve(skel, body, f)).fold(0.0f32, f32::max);
        if !auto_crouch || body.pelvis.is_none() || worst <= MAX_REACH + 1e-4 {
            return (frames, crouch);
        }
        crouch += (worst - MAX_REACH) * total * 1.15 + 1e-3;
    }
    let mut frames: Vec<Frame> = times.iter().map(|&t| make(t, crouch)).collect();
    frames.iter_mut().for_each(|f| {
        solve(skel, body, f);
    });
    (frames, crouch)
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
        Some(v) => Some(num(v, &o.at("crouch"), 0.0, 10.0)?),
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
                // Hermite from back to front whose end slopes match the stance's backward
                // speed, so the foot leaves and meets the ground at zero ground speed.
                let m = -stance_len * (1.0 - duty) / duty;
                let (q2, q3) = (q * q, q * q * q);
                let z = (2.0 * q3 - 3.0 * q2 + 1.0) * (-0.5 * stance_len) + (q3 - 2.0 * q2 + q) * m + (-2.0 * q3 + 3.0 * q2) * (0.5 * stance_len) + (q3 - q2) * m;
                (z, lift * l.lift * up, 22.0 * sin_turns(q) * (lift * l.lift / 0.1).min(1.0))
            };
            f.feet[i] = Some(([l.plant[0], l.plant[1] + y, l.plant[2] + dz], pitch));
        }
        apply_wobbles(&mut f, body, s);
        f
    };
    let (frames, _crouch) = match fixed_crouch {
        Some(c) => solve_all(skel, body, &|t, _| make(t, c), times, false),
        None => solve_all(skel, body, &make, times, true),
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
    let (frames, _) = solve_all(skel, body, &frames_make, times, true);
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
    let (frames, _) = solve_all(skel, body, &make, times, true);
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
    let dir = match o.str("direction")?.unwrap_or("forward") {
        "forward" => ([1.0, 0.0, 0.0], 1.0),
        "back" => ([1.0, 0.0, 0.0], -1.0),
        "left" => ([0.0, 0.0, 1.0], -1.0),
        "right" => ([0.0, 0.0, 1.0], 1.0),
        other => return Err(err(&o.at("direction"), AnimErrorCode::UnknownName, format!("direction '{other}' (forward, back, left, right)"))),
    };
    let sink = o.num("sink", 0.5, 0.0, 100.0)?;
    let shrink = o.num("shrink", 0.35, 0.0, 1.0)?;
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
                    f.rot(a.upper, qeuler([-a.swing * (buckle - 0.6 * u), 0.0, a.roll * u]));
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
            solve_all(skel, body, &make, times, false).0
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

/// Reads a `[x, y, z]` from JSON (helper for tests and tools).
pub fn vec3(v: &Json, path: &str) -> Result<V3, AnimError> {
    vecn(v, path, -1e6, 1e6)
}

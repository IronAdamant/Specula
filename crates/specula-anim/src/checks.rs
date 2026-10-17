//! Motion checks as data: measured numbers with thresholds from the game's `style.toml`, so a
//! failure is an error agents act on rather than a judgement call (docs/design/ANIMATION.md,
//! docs/research/AGENT_GAMEMAKING.md).
//!
//! | check | measures | unit |
//! |---|---|---|
//! | `foot_sliding` | feet's ground speed while their soles are within `contact_m` (2.5 cm) of the ground, weighted `2 − 2^(h/contact)` (the standard foot-skating metric), averaged over contact frames; the fastest frame is reported too | cm/s |
//! | `floating` | how high the lowest sole stays above the ground over the whole subject (a creature that never touches down hovers); for play-once subjects (deaths, leaps, strikes) how high the body (its soles, or its lowest point when it is off its feet) rests at the end | cm |
//! | `hover` | for flyers (`[motion.hover]`): how far the body's lowest point leaves the subject's hover band above the ground (the band is in the check's `band_low_cm` / `band_high_cm`; limit 0) | cm |
//! | `leap_apex` | for leaps (a `takeoff` event carrying the spec height, as the `leap` generator writes): how far the root's apex between takeoff and land misses that height | cm |
//! | `ground_penetration` | how far the lowest joint (or skinned vertex, when a mesh is given) goes below the ground | cm |
//! | `joint_limits` | the largest excess past a graph's `limits` (Euler degrees, rest-relative) | deg |
//! | `pops` | at a transition: the largest per-frame joint jump beyond what the states moved on their own, scaled to a 1.8 m figure (a giant's hand may travel further in a frame than a person's) | cm/frame |
//! | `root_drift` | root-motion travel per cycle against the clip's declared `distance` (in-place clips: the root leaving home) | cm |
//! | `loop_seam` | the joint gap between a looping clip's last and first frame | mm |
//!
//! Subjects are clips (moving over the ground at their natural speed), graph states held with
//! parameters (moving at their `match_speed` parameter), and transitions between two parameter
//! sets. Everything is sampled at `sample_hz` (60, a game's frame rate) in the model's own space:
//! y = 0 is the ground. Soles are the skinned vertices each foot bone drives, when the model's
//! mesh is given (the engine and the examples always give it); without one, the foot bone's
//! lowest point, taking the lowest foot of the rest pose as standing on the ground.
//!
//! ```toml
//! [motion]
//! contact_m = 0.025
//! foot_sliding_cm_s = 6.0
//! floating_cm = 2.5
//! ground_penetration_cm = 3.0
//! joint_limits_deg = 5.0
//! pops_cm = 6.0
//! root_drift_cm = 2.0
//! loop_seam_mm = 1.0
//! leap_apex_cm = 5.0
//! sample_hz = 60
//!
//! [motion.allow]          # what a subject is exempt from: "rig/subject" (* matches anything)
//! "skeleton_runner/*death" = ["joint_limits"]      # the bones fly apart: no joints left to limit
//! "shambler/*walk" = ["foot_sliding:foot_r"]       # one check for one bone: the dragged foot scrapes
//!
//! [motion.hover]          # flyers: the band (cm above the ground) the body's lowest point keeps
//! "wraith/*" = [3, 90]
//! ```
//!
//! A graph state is exempt from what the clips it plays are exempt from (`"shambler/climb"`
//! exempts the state that plays the climb). Checks a subject skips (foot sliding for a play-once
//! clip that doesn't travel, a fall) are listed in the report's `skipped`, so a verdict never
//! claims more than ran.
//!
//! A hover subject has no feet on the ground: `foot_sliding` and `floating` don't apply to it,
//! `hover` does (exempt a flyer's death from it to let it rise away).
//!
//! Foot sliding is measured per foot and reported for the worst one; an exempt foot is left out.

use specula_core::Json;

use crate::clip::LoopMode;
use crate::error::{err, AnimError, AnimErrorCode};
use crate::graph::{AnimGraph, Limit, Value};
use crate::math::{euler_of, math3, qconj, qmul, Affine, V3};
use crate::play::{self, Entry, GraphPlay, GraphPlayer, Graphed, LayerPlay, Scratch, Surroundings, Timing};
use crate::set::{AnimSet, ClipId};
use crate::skeleton::Pose;

/// Most frames one subject is sampled at (40 s at 60 Hz).
pub const MAX_FRAMES: usize = 2400;
/// Most notes a report's JSON lists.
pub const MAX_NOTES: usize = 16;
/// Check names, in report order.
pub const CHECKS: [&str; 9] = ["foot_sliding", "floating", "ground_penetration", "joint_limits", "pops", "root_drift", "loop_seam", "hover", "leap_apex"];

/// Thresholds (the `[motion]` table of `style.toml`).
#[derive(Clone, Debug, PartialEq)]
pub struct MotionRules {
    pub contact_m: f32,
    pub foot_sliding_cm_s: f32,
    pub floating_cm: f32,
    pub ground_penetration_cm: f32,
    pub joint_limits_deg: f32,
    pub pops_cm: f32,
    pub root_drift_cm: f32,
    pub loop_seam_mm: f32,
    pub leap_apex_cm: f32,
    pub sample_hz: f32,
    /// `(subject pattern, checks it is exempt from)`.
    pub allow: Vec<(String, Vec<String>)>,
    /// `(subject pattern, [lowest, highest] cm)`: flyers' hover bands (the first match counts).
    pub hover: Vec<(String, [f32; 2])>,
    /// Where the rules came from (a file path, or "defaults").
    pub source: String,
}

impl Default for MotionRules {
    fn default() -> Self {
        MotionRules {
            contact_m: 0.025,
            foot_sliding_cm_s: 6.0,
            floating_cm: 2.5,
            ground_penetration_cm: 3.0,
            joint_limits_deg: 5.0,
            pops_cm: 6.0,
            root_drift_cm: 2.0,
            loop_seam_mm: 1.0,
            leap_apex_cm: 5.0,
            sample_hz: 60.0,
            allow: Vec::new(),
            hover: Vec::new(),
            source: "defaults".into(),
        }
    }
}

impl MotionRules {
    /// The `[motion]` table of a `style.toml` (defaults for what it leaves out; a file without
    /// the table gives the defaults). Unknown keys in the table are errors.
    pub fn from_toml(text: &str, source: &str) -> Result<MotionRules, AnimError> {
        let doc = specula_core::toml::parse(text).map_err(|e| {
            let mut er = err(&format!("line {}", e.line), AnimErrorCode::Json, format!("{source}: {}", e.message));
            er.offset = Some(e.offset);
            er
        })?;
        let mut r = MotionRules { source: source.to_string(), ..MotionRules::default() };
        let Some(m) = doc.get("motion") else { return Ok(r) };
        let pairs = m.as_object().ok_or_else(|| err("motion", AnimErrorCode::WrongType, "[motion] is a table"))?;
        for (k, v) in pairs {
            let p = format!("motion.{k}");
            let num = |lo: f32, hi: f32| crate::error::num(v, &p, lo, hi);
            match k.as_str() {
                "contact_m" => r.contact_m = num(0.001, 1.0)?,
                "foot_sliding_cm_s" => r.foot_sliding_cm_s = num(0.0, 1e4)?,
                "floating_cm" => r.floating_cm = num(0.0, 1e4)?,
                "ground_penetration_cm" => r.ground_penetration_cm = num(0.0, 1e4)?,
                "joint_limits_deg" => r.joint_limits_deg = num(0.0, 360.0)?,
                "pops_cm" => r.pops_cm = num(0.0, 1e4)?,
                "root_drift_cm" => r.root_drift_cm = num(0.0, 1e5)?,
                "loop_seam_mm" => r.loop_seam_mm = num(0.0, 1e5)?,
                "leap_apex_cm" => r.leap_apex_cm = num(0.0, 1e4)?,
                "sample_hz" => r.sample_hz = num(10.0, 480.0)?,
                "allow" => {
                    let allow = v.as_object().ok_or_else(|| err(&p, AnimErrorCode::WrongType, "[motion.allow] maps \"rig/subject\" to a list of check names"))?;
                    for (subject, checks) in allow {
                        let cp = format!("{p}.{subject}");
                        let list = checks.as_array().ok_or_else(|| err(&cp, AnimErrorCode::WrongType, "a list of check names"))?;
                        let mut names = Vec::new();
                        for (i, c) in list.iter().enumerate() {
                            let n = c
                                .as_str()
                                .filter(|n| CHECKS.contains(&n.split(':').next().unwrap_or("")))
                                .ok_or_else(|| err(&format!("{cp}[{i}]"), AnimErrorCode::UnknownName, format!("a check name, optionally with a bone (\"foot_sliding:foot_r\"): {}", CHECKS.join(", "))))?;
                            names.push(n.to_string());
                        }
                        r.allow.push((subject.clone(), names));
                    }
                }
                "hover" => {
                    let bands = v.as_object().ok_or_else(|| err(&p, AnimErrorCode::WrongType, "[motion.hover] maps \"rig/subject\" to [lowest_cm, highest_cm]"))?;
                    for (subject, band) in bands {
                        let bp = format!("{p}.{subject}");
                        let pair = band.as_array().filter(|a| a.len() == 2).ok_or_else(|| err(&bp, AnimErrorCode::WrongType, "[lowest_cm, highest_cm]"))?;
                        let lo = crate::error::num(&pair[0], &format!("{bp}[0]"), -1e4, 1e4)?;
                        let hi = crate::error::num(&pair[1], &format!("{bp}[1]"), lo, 1e4)?;
                        r.hover.push((subject.clone(), [lo, hi]));
                    }
                }
                other => return Err(err(&p, AnimErrorCode::UnknownField, format!("'{other}' is not a motion rule (contact_m, foot_sliding_cm_s, floating_cm, ground_penetration_cm, joint_limits_deg, pops_cm, root_drift_cm, loop_seam_mm, leap_apex_cm, sample_hz, allow, hover)"))),
            }
        }
        Ok(r)
    }

    /// Whether `subject` (`"rig/name"`) is exempt from `check` (for `bone`, when given: an entry
    /// `"check:bone"` exempts that bone only).
    pub fn allows(&self, subject: &str, check: &str, bone: Option<&str>) -> bool {
        self.allow.iter().any(|(pat, checks)| {
            glob(pat, subject)
                && checks.iter().any(|c| match c.split_once(':') {
                    None => c == check,
                    Some((k, b)) => k == check && bone == Some(b),
                })
        })
    }

    /// The hover band (cm) of `subject`, when it is a flyer's.
    pub fn hover_band(&self, subject: &str) -> Option<[f32; 2]> {
        self.hover.iter().find(|(pat, _)| glob(pat, subject)).map(|(_, b)| *b)
    }

    fn limit(&self, check: &str) -> f32 {
        match check {
            "foot_sliding" => self.foot_sliding_cm_s,
            "floating" => self.floating_cm,
            "hover" => 0.0,
            "leap_apex" => self.leap_apex_cm,
            "ground_penetration" => self.ground_penetration_cm,
            "joint_limits" => self.joint_limits_deg,
            "pops" => self.pops_cm,
            "root_drift" => self.root_drift_cm,
            _ => self.loop_seam_mm,
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("source", Json::Str(self.source.clone())),
            ("contact_m", Json::Num(self.contact_m as f64)),
            ("foot_sliding_cm_s", Json::Num(self.foot_sliding_cm_s as f64)),
            ("floating_cm", Json::Num(self.floating_cm as f64)),
            ("ground_penetration_cm", Json::Num(self.ground_penetration_cm as f64)),
            ("joint_limits_deg", Json::Num(self.joint_limits_deg as f64)),
            ("pops_cm", Json::Num(self.pops_cm as f64)),
            ("root_drift_cm", Json::Num(self.root_drift_cm as f64)),
            ("loop_seam_mm", Json::Num(self.loop_seam_mm as f64)),
            ("leap_apex_cm", Json::Num(self.leap_apex_cm as f64)),
            ("sample_hz", Json::Num(self.sample_hz as f64)),
            ("allow", Json::obj(self.allow.iter().map(|(p, c)| (p.clone(), Json::arr(c.iter().map(|n| Json::Str(n.clone()))))))),
            ("hover", Json::obj(self.hover.iter().map(|(p, b)| (p.clone(), Json::arr([Json::Num(b[0] as f64), Json::Num(b[1] as f64)]))))),
        ])
    }
}

/// `*` matches any run of characters; everything else matches itself.
fn glob(pat: &str, s: &str) -> bool {
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return pat == s;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !s.starts_with(first) || !s[first.len()..].ends_with(last) || s.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &s[first.len()..s.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

/// One measurement.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub check: &'static str,
    pub subject: String,
    pub value: f32,
    pub limit: f32,
    pub unit: &'static str,
    /// Within the limit, or exempt.
    pub passed: bool,
    pub exempt: bool,
    /// Seconds into the subject where the worst value was.
    pub at_s: Option<f32>,
    pub bone: Option<String>,
    /// More numbers (e.g. the fastest contact frame, contact fraction).
    pub detail: Vec<(&'static str, f32)>,
}

impl Check {
    pub fn to_json(&self) -> Json {
        let r = |x: f32| Json::Num((x as f64 * 1000.0).round() / 1000.0);
        let mut f = vec![
            ("check", Json::from(self.check)),
            ("subject", Json::Str(self.subject.clone())),
            ("value", r(self.value)),
            ("limit", r(self.limit)),
            ("unit", Json::from(self.unit)),
            ("passed", Json::Bool(self.passed)),
        ];
        if self.exempt {
            f.push(("exempt", Json::Bool(true)));
        }
        if let Some(t) = self.at_s {
            f.push(("at_s", r(t)));
        }
        if let Some(b) = &self.bone {
            f.push(("bone", Json::Str(b.clone())));
        }
        for (k, v) in &self.detail {
            f.push((k, r(*v)));
        }
        Json::obj(f)
    }

    /// The failure as a structured error (`None` when it passed).
    pub fn error_json(&self) -> Option<Json> {
        (!self.passed).then(|| {
            let mut e = self.to_json();
            e.set("error", Json::from(self.check));
            e
        })
    }
}

/// Every check of one or more subjects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub checks: Vec<Check>,
    /// What else an agent should know (a state its parameters don't hold, ...).
    pub notes: Vec<String>,
    /// Checks this subject skipped, with why (`"foot_sliding: off its feet"`).
    pub skipped: Vec<String>,
}

impl Report {
    pub fn failures(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter().filter(|c| !c.passed)
    }

    pub fn passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }

    /// `{passed, errors: [...], checks: [...]}`: `checks` at most `max_checks`, failures first;
    /// `errors` at most `max_checks` too (the rest counted in `errors_omitted`); notes at most
    /// [`MAX_NOTES`].
    pub fn to_json(&self, max_checks: usize) -> Json {
        let mut sorted: Vec<&Check> = self.checks.iter().collect();
        sorted.sort_by_key(|c| c.passed);
        let shown: Vec<Json> = sorted.iter().take(max_checks).map(|c| c.to_json()).collect();
        let failed = self.failures().count();
        let mut out = vec![
            ("passed", Json::Bool(self.passed())),
            ("errors", Json::arr(self.failures().take(max_checks).filter_map(Check::error_json))),
            ("checks", Json::Arr(shown)),
        ];
        if failed > max_checks {
            out.push(("errors_omitted", Json::from(failed - max_checks)));
        }
        if !self.notes.is_empty() {
            out.push(("notes", Json::arr(self.notes.iter().take(MAX_NOTES).map(|n| Json::Str(n.clone())))));
            if self.notes.len() > MAX_NOTES {
                out.push(("notes_omitted", Json::from(self.notes.len() - MAX_NOTES)));
            }
        }
        if self.checks.len() > max_checks {
            out.push(("checks_omitted", Json::from(self.checks.len() - max_checks)));
        }
        if !self.skipped.is_empty() {
            out.push(("skipped", Json::arr(self.skipped.iter().take(MAX_NOTES).map(|n| Json::Str(n.clone())))));
        }
        Json::obj(out)
    }

    /// The names of the checks skipped (without the reasons), for labels.
    pub fn skipped_checks(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.skipped {
            let n = s.split(':').next().unwrap_or(s).to_string();
            if !out.contains(&n) {
                out.push(n);
            }
        }
        out
    }

    /// Names of the checks that passed only because the rules exempt them (`"check"` or
    /// `"check:bone"`), for labels.
    pub fn exemptions(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in self.checks.iter().filter(|c| c.exempt) {
            let n = match &c.bone {
                Some(b) if c.check == "foot_sliding" => format!("{}:{b}", c.check),
                _ => c.check.to_string(),
            };
            if !out.contains(&n) {
                out.push(n);
            }
        }
        out
    }
}

/// What a subject did, frame by frame (model space, travel included: the ground is y = 0).
struct Sampled {
    dt: f32,
    /// Bone heads and tails per frame, world-ish (model + travel).
    heads: Vec<Vec<V3>>,
    tails: Vec<Vec<V3>>,
    /// Local poses (joint limits).
    poses: Vec<Pose>,
}

fn tails_of(set: &AnimSet, model: &[Affine]) -> Vec<V3> {
    let s = &set.skeleton;
    model.iter().enumerate().map(|(b, m)| m.apply_point(math3::sub(s.tails[b], s.heads[b]))).collect()
}

fn push_frame(out: &mut Sampled, set: &AnimSet, pose: &Pose, model: &[Affine], travel: V3) {
    out.heads.push(model.iter().map(|m| math3::add(m.t, travel)).collect());
    out.tails.push(tails_of(set, model).into_iter().map(|p| math3::add(p, travel)).collect());
    out.poses.push(pose.clone());
}

fn frames_for(duration: f32, hz: f32) -> usize {
    ((duration * hz).ceil() as usize).clamp(2, MAX_FRAMES)
}

/// A clip moving over the ground at its natural speed.
fn sample_clip(set: &AnimSet, id: ClipId, hz: f32) -> Sampled {
    let clip = set.clip(id);
    let n = frames_for(clip.duration, hz);
    let dt = clip.duration / n as f32;
    // A clip with a root translation track travels by itself; others move at their natural speed.
    let v = if set.bound(id).root_t.is_some() { 0.0 } else { clip.speed() };
    let mut out = Sampled { dt, heads: Vec::with_capacity(n + 1), tails: Vec::with_capacity(n + 1), poses: Vec::with_capacity(n + 1) };
    let mut pose = set.skeleton.rest_pose();
    let mut model = Vec::new();
    for k in 0..=n {
        let t = (k as f32 * dt).min(clip.duration);
        crate::blend::sample(&set.skeleton, clip, set.track_bones(id), t, crate::blend::Base::Rest, &mut pose);
        set.skeleton.model_space(&pose, &mut model);
        push_frame(&mut out, set, &pose, &model, [0.0, 0.0, v * t]);
    }
    out
}

/// A play over world times `t0..t1`, moving `speed` model metres per second along +Z plus its
/// extracted root motion.
fn sample_play(g: Graphed, play: &GraphPlay, timing: Timing, t0: f64, t1: f64, hz: f32, speed: f32) -> Sampled {
    let n = frames_for((t1 - t0) as f32, hz);
    let dt = ((t1 - t0) / n as f64) as f32;
    let mut out = Sampled { dt, heads: Vec::with_capacity(n + 1), tails: Vec::with_capacity(n + 1), poses: Vec::with_capacity(n + 1) };
    let mut player = GraphPlayer::new();
    let mut s = Scratch::default();
    let mut root = [0.0f32; 3];
    let mut prev_t = t0;
    for k in 0..=n {
        let t = t0 + (t1 - t0) * k as f64 / n as f64;
        if k > 0 {
            let d = play::root_motion(g, play, timing, prev_t, t, &mut s);
            root = math3::add(root, d);
        }
        prev_t = t;
        let pose = player.evaluate(g, play, timing, t, &Surroundings::default()).clone();
        let travel = math3::add(root, [0.0, 0.0, speed * (t - t0) as f32]);
        let model = player.model().to_vec();
        push_frame(&mut out, g.set, &pose, &model, travel);
    }
    out
}

/// Context every check of a subject shares.
struct Ctx<'a> {
    set: &'a AnimSet,
    rules: &'a MotionRules,
    subject: String,
    /// More subjects whose exemptions apply (a state's clips, `"rig/clip"`).
    also: Vec<String>,
}

impl Ctx<'_> {
    fn allows(&self, check: &str, bone: Option<&str>) -> bool {
        self.rules.allows(&self.subject, check, bone) || self.also.iter().any(|s| self.rules.allows(s, check, bone))
    }

    #[allow(clippy::too_many_arguments)]
    fn check(&self, out: &mut Report, check: &'static str, value: f32, unit: &'static str, at_s: Option<f32>, bone: Option<usize>, detail: Vec<(&'static str, f32)>) {
        let limit = self.rules.limit(check);
        let exempt = self.allows(check, None);
        let passed = exempt || (value.is_finite() && value <= limit);
        out.checks.push(Check {
            check,
            subject: self.subject.clone(),
            value,
            limit,
            unit,
            passed,
            exempt,
            at_s,
            bone: bone.map(|b| self.set.skeleton.names[b].clone()),
            detail,
        });
    }
}

/// The bones checked as feet: the graph's `feet`, else the clip's footstep bones, else bones
/// named `foot*`.
pub fn feet_of(set: &AnimSet, graph: Option<&AnimGraph>, clip: Option<ClipId>) -> Vec<usize> {
    if let Some(g) = graph.filter(|g| !g.feet.is_empty()) {
        return g.feet.clone();
    }
    let mut out: Vec<usize> = Vec::new();
    if let Some(id) = clip {
        for e in &set.clip(id).events {
            if let Some(b) = set.skeleton.find(&e.data) {
                if (e.name == "footstep" || e.name == "stomp") && !out.contains(&b) {
                    out.push(b);
                }
            }
        }
    }
    if out.is_empty() {
        out = set.skeleton.names.iter().enumerate().filter(|(_, n)| n.starts_with("foot")).map(|(i, _)| i).collect();
    }
    out
}

/// Joint limits from every graph of the set (a clip has none of its own).
pub fn limits_of(set: &AnimSet) -> Vec<Limit> {
    let mut out: Vec<Limit> = Vec::new();
    for (_, g) in set.graphs() {
        for l in &g.limits {
            if !out.iter().any(|o| o.bone == l.bone) {
                out.push(l.clone());
            }
        }
    }
    out
}

/// Per foot and frame: the ankle (bone head, travel included) and the sole's height above the
/// ground.
struct FootTrack {
    bone: usize,
    frames: Vec<(V3, f32)>,
}

fn foot_tracks(set: &AnimSet, s: &Sampled, feet: &[usize], mesh: Option<&specula_model::MeshData>) -> Vec<FootTrack> {
    let skel = &set.skeleton;
    // Each foot's vertices: those it drives most (at least half the weight).
    let verts: Vec<Vec<u32>> = feet
        .iter()
        .map(|&f| match mesh.filter(|m| m.is_skinned()) {
            Some(m) => (0..m.positions.len() as u32)
                .filter(|&i| {
                    let (j, w) = (m.joints[i as usize], m.weights[i as usize]);
                    (0..4).any(|k| j[k] as usize == f && w[k] >= 128)
                })
                .collect(),
            None => Vec::new(),
        })
        .collect();
    let clearance = feet.iter().map(|&f| skel.heads[f][1].min(skel.tails[f][1])).fold(f32::MAX, f32::min);
    let (mut model, mut skin) = (Vec::new(), Vec::new());
    let mut out: Vec<FootTrack> = feet.iter().map(|&f| FootTrack { bone: f, frames: Vec::with_capacity(s.heads.len()) }).collect();
    for k in 0..s.heads.len() {
        let need_skin = verts.iter().any(|v| !v.is_empty());
        if need_skin {
            skel.model_space(&s.poses[k], &mut model);
            skel.skin_matrices(&model, &mut skin);
        }
        for (i, &f) in feet.iter().enumerate() {
            let ankle = s.heads[k][f];
            let sole = if verts[i].is_empty() {
                s.heads[k][f][1].min(s.tails[k][f][1]) - clearance
            } else {
                let m = mesh.expect("verts come from the mesh");
                verts[i].iter().map(|&vi| skin_vertex(m, &skin, vi as usize)[1]).fold(f32::MAX, f32::min)
            };
            out[i].frames.push((ankle, sole));
        }
    }
    out
}

/// One skinned vertex position (linear blend skinning, like `skin::skin_into`).
fn skin_vertex(m: &specula_model::MeshData, skin: &[Affine], i: usize) -> V3 {
    let (j, w) = (m.joints[i], m.weights[i]);
    let mut p = [0.0f32; 3];
    for k in 0..4 {
        if w[k] == 0 {
            continue;
        }
        let q = skin[j[k] as usize].apply_point(m.positions[i]);
        p = math3::add(p, math3::scale(q, w[k] as f32 * (1.0 / 255.0)));
    }
    p
}

/// Feet sliding: the worst foot the rules check is one check; each foot the rules exempt is
/// measured too and reported as its own exempt check (so a label can say what passed only by
/// exemption, and how much that foot slides).
fn foot_sliding(ctx: &Ctx, s: &Sampled, tracks: &[FootTrack], out: &mut Report) {
    if tracks.is_empty() || s.heads.len() < 2 {
        return;
    }
    let skel = &ctx.set.skeleton;
    let h = ctx.rules.contact_m;
    // Per foot: (weighted mean m/s, fastest m/s, when, contact fraction, bone).
    let measure = |tr: &FootTrack| {
        let (mut sum, mut n, mut fastest, mut at) = (0.0f64, 0usize, 0.0f32, 0.0f32);
        for k in 0..tr.frames.len() - 1 {
            let ((p, hp), (q, hq)) = (tr.frames[k], tr.frames[k + 1]);
            let height = 0.5 * (hp + hq);
            if height > h {
                continue;
            }
            let speed = math3::length([q[0] - p[0], 0.0, q[2] - p[2]]) / s.dt;
            let weight = (2.0 - specula_core::dmath::exp2(height.max(0.0) / h)).max(0.0);
            sum += (speed * weight) as f64;
            n += 1;
            if speed > fastest {
                (fastest, at) = (speed, k as f32 * s.dt);
            }
        }
        let mean = if n > 0 { (sum / n as f64) as f32 } else { 0.0 };
        (mean, fastest, at, n as f32 / (tr.frames.len() - 1) as f32, tr.bone)
    };
    let mut worst: Option<(f32, f32, f32, f32, usize)> = None;
    let mut exempt = Vec::new();
    for tr in tracks {
        let m = measure(tr);
        if ctx.allows("foot_sliding", Some(&skel.names[tr.bone])) {
            exempt.push(m);
        } else if worst.is_none_or(|w| m.0 > w.0) {
            worst = Some(m);
        }
    }
    if let Some((mean, fastest, at, contact, foot)) = worst {
        let mut detail = vec![("fastest_cm_s", fastest * 100.0), ("contact", contact)];
        if !exempt.is_empty() {
            detail.push(("exempt_feet", exempt.len() as f32));
        }
        ctx.check(out, "foot_sliding", mean * 100.0, "cm/s", Some(at), Some(foot), detail);
    }
    for (mean, fastest, at, contact, foot) in exempt {
        let mut one = Report::default();
        ctx.check(&mut one, "foot_sliding", mean * 100.0, "cm/s", Some(at), Some(foot), vec![("fastest_cm_s", fastest * 100.0), ("contact", contact)]);
        for c in &mut one.checks {
            (c.exempt, c.passed) = (true, true);
        }
        out.checks.extend(one.checks);
    }
}

/// The lowest sole over the whole subject should reach the ground.
fn floating(ctx: &Ctx, s: &Sampled, tracks: &[FootTrack], out: &mut Report) {
    let mut low: Option<(f32, f32, usize)> = None;
    for tr in tracks {
        for (k, (_, h)) in tr.frames.iter().enumerate() {
            if low.is_none_or(|l| *h < l.0) {
                low = Some((*h, k as f32 * s.dt, tr.bone));
            }
        }
    }
    if let Some((h, at, bone)) = low {
        ctx.check(out, "floating", h.max(0.0) * 100.0, "cm", Some(at), Some(bone), vec![("lowest_sole_m", h)]);
    }
}

/// Scratch for [`lowest_at`].
type SkinScratch = (Vec<Affine>, Vec<Affine>, Vec<V3>, Vec<V3>);

/// The lowest point of the body at frame `k`: skinned vertices with a mesh, else joints (not the
/// root) and bone ends. Travel doesn't change heights.
fn lowest_at(set: &AnimSet, s: &Sampled, mesh: Option<&specula_model::MeshData>, k: usize, scratch: &mut SkinScratch) -> f32 {
    match mesh {
        Some(m) => {
            let (model, sk, pos, nor) = scratch;
            set.skeleton.model_space(&s.poses[k], model);
            set.skeleton.skin_matrices(model, sk);
            crate::skin::skin_into(m, sk, pos, nor);
            pos.iter().map(|p| p[1]).fold(f32::MAX, f32::min)
        }
        None => (1..s.heads[k].len()).flat_map(|b| [s.heads[k][b][1], s.tails[k][b][1]]).fold(f32::MAX, f32::min),
    }
}

/// A play-once subject should come to rest on the ground: how high its lowest point (soles
/// standing, the body lying) is at the end.
fn floating_end(ctx: &Ctx, s: &Sampled, mesh: Option<&specula_model::MeshData>, out: &mut Report) {
    let Some(last) = s.heads.len().checked_sub(1) else { return };
    let h = lowest_at(ctx.set, s, mesh, last, &mut SkinScratch::default());
    ctx.check(out, "floating", h.max(0.0) * 100.0, "cm", Some(last as f32 * s.dt), None, vec![("end_lowest_m", h)]);
}

/// A flyer's lowest point stays within its hover band (every fourth frame and the last).
fn hover(ctx: &Ctx, s: &Sampled, band: [f32; 2], mesh: Option<&specula_model::MeshData>, out: &mut Report) {
    let Some(last) = s.heads.len().checked_sub(1) else { return };
    let mut scratch = SkinScratch::default();
    let (mut worst, mut at, mut low, mut high) = (0.0f32, 0.0f32, f32::MAX, f32::MIN);
    for k in (0..=last).step_by(4).chain((last % 4 != 0).then_some(last)) {
        let h = lowest_at(ctx.set, s, mesh, k, &mut scratch) * 100.0;
        (low, high) = (low.min(h), high.max(h));
        let excess = (band[0] - h).max(h - band[1]).max(0.0);
        if excess > worst {
            (worst, at) = (excess, k as f32 * s.dt);
        }
    }
    ctx.check(out, "hover", worst, "cm", Some(at), None, vec![("lowest_cm", low), ("highest_cm", high), ("band_low_cm", band[0]), ("band_high_cm", band[1])]);
}

fn ground_penetration(ctx: &Ctx, s: &Sampled, mesh: Option<&specula_model::MeshData>, out: &mut Report) {
    if s.heads.is_empty() {
        return;
    }
    // The lowest joint (not the root) or, with a mesh, skinned vertex (labelled with the bone that
    // drives it most); every frame for joints, every fourth plus the last for the mesh.
    let (mut low, mut at, mut bone) = (f32::MAX, 0.0f32, 0usize);
    let last = s.heads.len() - 1;
    let mut end_low = f32::MAX;
    for k in 0..s.heads.len() {
        for b in 1..s.heads[k].len() {
            for p in [s.heads[k][b], s.tails[k][b]] {
                if p[1] < low {
                    (low, at, bone) = (p[1], k as f32 * s.dt, b);
                }
                if k == last {
                    end_low = end_low.min(p[1]);
                }
            }
        }
    }
    let mut detail = Vec::new();
    if let Some(m) = mesh {
        let skel = &ctx.set.skeleton;
        let (mut model, mut sk) = (Vec::new(), Vec::new());
        let (mut pos, mut nor) = (Vec::new(), Vec::new());
        let mut mesh_low = f32::MAX;
        let frames = (0..s.poses.len()).step_by(4).chain((last % 4 != 0).then_some(last));
        let mut mesh_end = f32::MAX;
        for k in frames {
            skel.model_space(&s.poses[k], &mut model);
            skel.skin_matrices(&model, &mut sk);
            crate::skin::skin_into(m, &sk, &mut pos, &mut nor);
            // Travel doesn't change heights.
            for (i, p) in pos.iter().enumerate() {
                if k == last {
                    mesh_end = mesh_end.min(p[1]);
                }
                if p[1] < mesh_low {
                    mesh_low = p[1];
                    if mesh_low < low {
                        let driver = if m.is_skinned() {
                            let (j, w) = (m.joints[i], m.weights[i]);
                            j[(0..4).max_by_key(|&q| w[q]).unwrap_or(0)] as usize
                        } else {
                            0
                        };
                        (low, at, bone) = (mesh_low, k as f32 * s.dt, driver);
                    }
                }
            }
        }
        detail.push(("mesh_lowest_m", mesh_low));
        end_low = mesh_end;
    }
    detail.push(("lowest_m", low));
    // Where it ends up: a death should end on the ground, not above it.
    detail.push(("end_lowest_m", end_low));
    ctx.check(out, "ground_penetration", (-low).max(0.0) * 100.0, "cm", Some(at), Some(bone), detail);
}

fn joint_limits(ctx: &Ctx, s: &Sampled, limits: &[Limit], out: &mut Report) {
    if limits.is_empty() {
        return;
    }
    let skel = &ctx.set.skeleton;
    let (mut worst, mut at, mut bone, mut angle) = (0.0f32, 0.0f32, limits[0].bone, 0.0f32);
    for (k, pose) in s.poses.iter().enumerate() {
        for l in limits {
            let rel = qmul(qconj(skel.rest[l.bone].r), pose.local[l.bone].r);
            let e = euler_of(rel);
            for (ax, range) in l.axes.iter().enumerate() {
                let Some([lo, hi]) = range else { continue };
                let excess = (lo - e[ax]).max(e[ax] - hi).max(0.0);
                if excess > worst {
                    (worst, at, bone, angle) = (excess, k as f32 * s.dt, l.bone, e[ax]);
                }
            }
        }
    }
    ctx.check(out, "joint_limits", worst, "deg", Some(at), Some(bone), vec![("angle_deg", angle)]);
}

fn loop_and_drift(ctx: &Ctx, id: ClipId, s: &Sampled, out: &mut Report) {
    let clip = ctx.set.clip(id);
    if let Some(seam) = crate::measure::loop_seam(ctx.set, id) {
        ctx.check(out, "loop_seam", seam * 1000.0, "mm", None, None, Vec::new());
    }
    // Root drift: a root-motion clip's travel against its declared distance; an in-place one's root leaving home.
    let b = ctx.set.bound(id);
    let first = &s.heads[0][0];
    let last = &s.heads[s.heads.len() - 1][0];
    let drift = if b.root_t.is_some() {
        let moved = [last[0] - first[0], 0.0, last[2] - first[2]];
        let expect = if clip.looping == LoopMode::PingPong { 0.0 } else { clip.distance };
        math3::length([moved[0], 0.0, moved[2] - expect])
    } else {
        let home = ctx.set.skeleton.heads[0];
        s.heads.iter().enumerate().map(|(k, h)| math3::length([h[0][0] - home[0], 0.0, h[0][2] - home[2] - clip.speed() * k as f32 * s.dt])).fold(0.0, f32::max)
    };
    ctx.check(out, "root_drift", drift * 100.0, "cm", None, Some(0), Vec::new());
}

/// Checks one clip of `set` (`rig`: its name in subjects, `"rig/clip"`). `mesh`: the model's
/// mesh, for penetration by skinned vertices.
pub fn check_clip(set: &AnimSet, rig: &str, id: ClipId, rules: &MotionRules, mesh: Option<&specula_model::MeshData>) -> Report {
    let clip = set.clip(id);
    let ctx = Ctx { set, rules, subject: format!("{rig}/{}", clip.name), also: Vec::new() };
    let s = sample_clip(set, id, rules.sample_hz);
    let mut out = Report::default();
    // Feet slide only matters for clips that stand or move on them (not deaths, not flyers).
    let feet = feet_of(set, None, Some(id));
    let tracks = foot_tracks(set, &s, &feet, mesh);
    // Feet only matter for clips that stand or move on them (not deaths, not once-off moves).
    let grounded = (clip.looping != LoopMode::Once || clip.distance > 0.0) && !clip.tracks.is_empty();
    // A clip that moves no foot nor anything above one (a flinch on the upper body) leaves the
    // feet where the rest pose has them: nothing to check.
    let skel = &set.skeleton;
    let above = |b: usize, foot: usize| {
        let mut x = Some(foot as u16);
        while let Some(i) = x {
            if i as usize == b {
                return true;
            }
            x = skel.parents[i as usize];
        }
        false
    };
    let moves_feet = set.track_bones(id).iter().any(|&b| feet.iter().any(|&f| above(b as usize, f)));
    let band = rules.hover_band(&ctx.subject);
    match band {
        Some(band) => hover(&ctx, &s, band, mesh, &mut out),
        None if clip.tracks.is_empty() || tracks.is_empty() => {}
        None => {
            if grounded {
                foot_sliding(&ctx, &s, &tracks, &mut out);
            }
            if !moves_feet {
            } else if clip.looping != LoopMode::Once {
                floating(&ctx, &s, &tracks, &mut out);
            } else {
                floating_end(&ctx, &s, mesh, &mut out);
            }
        }
    }
    let none = if clip.tracks.is_empty() {
        Some("an empty clip (the rest pose)")
    } else if tracks.is_empty() {
        Some("no feet found (a graph's feet, footstep events or bones named foot*)")
    } else {
        None
    };
    let slide_why = none.unwrap_or("a play-once clip that doesn't travel (a graph state that plays it on its feet checks it)");
    let float_why = none.unwrap_or("it moves no foot nor anything above one (an upper-body clip)");
    let expect: &[(&str, &str)] = if band.is_some() {
        &[("hover", ""), ("ground_penetration", ""), ("joint_limits", NO_LIMITS), ("root_drift", "")]
    } else {
        &[("foot_sliding", slide_why), ("floating", float_why), ("ground_penetration", ""), ("joint_limits", NO_LIMITS), ("root_drift", "")]
    };
    ground_penetration(&ctx, &s, mesh, &mut out);
    joint_limits(&ctx, &s, &limits_of(set), &mut out);
    loop_and_drift(&ctx, id, &s, &mut out);
    leap_apex(&ctx, clip, &s, 1.0 / clip.duration.max(1e-6) as f64, &mut out);
    skip_missing(&mut out, expect);
    out
}

/// Checks poses recorded from outside the animation (a ragdoll's, from physics; A4): `poses`
/// are local poses `dt` seconds apart in model space, the ground at y = 0 as for clips. Runs
/// ground penetration, where the body comes to rest (`floating`, at the last frame), joint
/// limits against `limits` and pops where physics takes over (the first frames): a joint jumping
/// off the path it was moving on (the change of its per-frame motion, so a fast smooth fall isn't
/// a pop and a teleport is), scaled to a 1.8 m figure like the transition check.
pub fn check_frames(set: &AnimSet, subject: &str, poses: &[Pose], dt: f32, limits: &[Limit], rules: &MotionRules, mesh: Option<&specula_model::MeshData>) -> Report {
    let ctx = Ctx { set, rules, subject: subject.to_string(), also: Vec::new() };
    let mut s = Sampled { dt, heads: Vec::with_capacity(poses.len()), tails: Vec::with_capacity(poses.len()), poses: Vec::with_capacity(poses.len()) };
    let mut model = Vec::new();
    for p in poses.iter().take(MAX_FRAMES) {
        if p.local.len() != set.skeleton.len() {
            continue;
        }
        set.skeleton.model_space(p, &mut model);
        push_frame(&mut s, set, p, &model, [0.0; 3]);
    }
    let mut out = Report::default();
    if s.heads.len() < 2 {
        out.skipped.push("all: fewer than two frames recorded".into());
        return out;
    }
    ground_penetration(&ctx, &s, mesh, &mut out);
    floating_end(&ctx, &s, mesh, &mut out);
    joint_limits(&ctx, &s, limits, &mut out);
    // Pops where physics takes over (the first frames): the largest change of a joint's
    // per-frame motion (second difference). Later ones are impacts, which physics resolves.
    let height = set.skeleton.heads.iter().chain(&set.skeleton.tails).map(|p| p[1]).fold(0.0f32, f32::max).max(0.1);
    let to_human = 1.8 / height;
    let (mut worst, mut at, mut bone) = (0.0f32, 0.0f32, 0usize);
    for k in 2..s.heads.len().min(TAKEOVER_FRAMES) {
        for b in 1..s.heads[k].len() {
            let (p0, p1, p2) = (s.heads[k - 2][b], s.heads[k - 1][b], s.heads[k][b]);
            let d = math3::length(math3::sub(math3::sub(p2, p1), math3::sub(p1, p0)));
            if d > worst {
                (worst, at, bone) = (d, k as f32 * dt, b);
            }
        }
    }
    ctx.check(&mut out, "pops", worst * 100.0 * to_human, "cm", Some(at), Some(bone), vec![("unscaled_cm", worst * 100.0), ("frame_s", dt)]);
    if limits.is_empty() {
        out.skipped.push(format!("joint_limits: {NO_LIMITS}"));
    }
    out
}

/// Frames after a recording starts that [`check_frames`] looks at for pops.
const TAKEOVER_FRAMES: usize = 6;

/// Why a joint-limits check didn't run.
const NO_LIMITS: &str = "no joint limits are given (a graph's `limits`)";

/// Lists each check of `expect` (those that apply to this kind of subject) that didn't run and
/// isn't listed yet, with its reason, so a verdict never claims more than ran.
fn skip_missing(out: &mut Report, expect: &[(&str, &str)]) {
    for (name, why) in expect {
        let ran = out.checks.iter().any(|c| c.check == *name);
        let listed = out.skipped.iter().any(|s| s.split(':').next() == Some(*name));
        if !ran && !listed {
            let why = if why.is_empty() { "not measured for this subject" } else { why };
            out.skipped.push(format!("{name}: {why}"));
        }
    }
}

/// A leap's apex: the root's highest point between `takeoff` and `land` over its height at
/// takeoff, against the height the `takeoff` event carries. `rate`: clip cycles per second of
/// the sampled subject.
fn leap_apex(ctx: &Ctx, clip: &crate::clip::Clip, s: &Sampled, rate: f64, out: &mut Report) {
    let ev = |n: &str| clip.events.iter().find(|e| e.name == n);
    let (Some(up), Some(down)) = (ev("takeoff"), ev("land")) else { return };
    let Ok(spec) = up.data.trim().parse::<f32>() else { return };
    let frame = |time: f32| ((time as f64 / clip.duration.max(1e-6) as f64 / rate / s.dt as f64).round() as usize).min(s.heads.len().saturating_sub(1));
    let (k0, k1) = (frame(up.time), frame(down.time));
    if k1 <= k0 {
        return;
    }
    let base = s.heads[k0][0][1];
    let (mut apex, mut at) = (f32::MIN, k0);
    for (k, h) in s.heads.iter().enumerate().take(k1 + 1).skip(k0) {
        if h[0][1] - base > apex {
            (apex, at) = (h[0][1] - base, k);
        }
    }
    ctx.check(out, "leap_apex", (apex - spec).abs() * 100.0, "cm", Some(at as f32 * s.dt), Some(0), vec![("apex_m", apex), ("spec_m", spec)]);
}

/// The clips (not additive layers' deltas) a node plays, at most [`crate::graph::MAX_LEAVES`].
fn clips_under(g: Graphed, node: crate::graph::NodeId) -> Vec<ClipId> {
    use crate::graph::Node;
    let mut out = Vec::new();
    let mut todo = vec![node];
    while let Some(n) = todo.pop() {
        if out.len() as u64 >= crate::graph::MAX_LEAVES {
            break;
        }
        match &g.graph.nodes[n.0 as usize] {
            Node::Clip { clip, .. } => {
                if !out.contains(clip) {
                    out.push(*clip);
                }
            }
            Node::Blend1 { points, .. } => todo.extend(points.iter().map(|(_, c)| *c)),
            Node::Blend2 { points, .. } => todo.extend(points.iter().map(|(_, c)| *c)),
            Node::Additive { base, .. } => todo.push(*base),
            Node::External => {}
        }
    }
    out
}

/// A play of `graph` held in `state` (base layer) with `params`, for `cycles` cycles of it.
fn state_play(graph_name: &str, state: &str, params: &[(String, Value)]) -> GraphPlay {
    let mut p = GraphPlay::new(graph_name);
    for (n, v) in params {
        p.set_param(n, v.clone());
    }
    p.layers.push(LayerPlay { layer: "base".into(), stack: vec![Entry { state: state.to_string(), phase: 0.0, fade: None }] });
    p
}

/// Checks a graph state held with parameters for `cycles` of its cycle (at most 40 s), moving at
/// its `match_speed` parameter (world m/s over `scale`). Reports where it left the state if the
/// parameters don't hold it there.
#[allow(clippy::too_many_arguments)]
pub fn check_state(g: Graphed, rig: &str, graph_name: &str, state: &str, params: &[(String, Value)], cycles: f32, rules: &MotionRules, mesh: Option<&specula_model::MeshData>) -> Result<Report, AnimError> {
    let li_state = g.graph.layers[0].state(state).ok_or_else(|| err("state", AnimErrorCode::UnknownName, format!("no state '{state}' in graph '{graph_name}' (states: {})", g.graph.layers[0].states.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", "))))?;
    let play = state_play(graph_name, state, params);
    let timing = Timing { anchor: 0.0, speed: 1.0 };
    let mut sc = Scratch::default();
    let now = play::state_at(g, &play, timing, 0.0, &mut sc);
    let rate = now[0].stack.iter().find(|e| e.state == state).map_or(1.0, |e| e.rate).max(1e-3);
    let window = (cycles.max(0.25) as f64 / rate).min(MAX_FRAMES as f64 / rules.sample_hz as f64);
    let st = &g.graph.layers[0].states[li_state as usize];
    let speed = st.match_speed.map_or(0.0, |p| match play.param(&g.graph.params[p as usize].name) {
        Some(Value::Num(x)) => *x,
        _ => 0.0,
    }) / g.scale.max(1e-6);
    let st_clips = clips_under(g, st.node);
    let also = st_clips.iter().map(|&c| format!("{rig}/{}", g.set.clip(c).name)).collect();
    let ctx = Ctx { set: g.set, rules, subject: format!("{rig}/{graph_name}.{state}"), also };
    let s = sample_play(g, &play, timing, 0.0, window, rules.sample_hz, speed);
    let mut out = Report::default();
    let feet = feet_of(g.set, Some(g.graph), None);
    // Feet IK weight 0 marks a state off its feet (a death, a leap).
    let feet_off = st.ik.iter().any(|(i, w)| matches!(g.graph.ik[*i as usize].solver, crate::graph::Solver::Feet(_)) && *w == 0.0);
    let tracks = foot_tracks(g.set, &s, &feet, mesh);
    let once = !g.graph.node_loops[st.node.0 as usize];
    match rules.hover_band(&ctx.subject) {
        Some(band) => hover(&ctx, &s, band, mesh, &mut out),
        None if feet_off => {
            // Off its feet (feet IK off): a climb or a leap still moves on its feet, a fall doesn't.
            let travels = st_clips.iter().any(|&c| {
                let c = g.set.clip(c);
                c.looping != LoopMode::Once || c.distance > 0.0
            });
            if travels {
                foot_sliding(&ctx, &s, &tracks, &mut out);
            } else if !tracks.is_empty() {
                out.skipped.push("foot_sliding: off its feet (feet IK off) in a fall".into());
            }
            floating_end(&ctx, &s, mesh, &mut out);
        }
        None => {
            foot_sliding(&ctx, &s, &tracks, &mut out);
            if once {
                floating_end(&ctx, &s, mesh, &mut out);
            } else {
                floating(&ctx, &s, &tracks, &mut out);
            }
        }
    }
    ground_penetration(&ctx, &s, mesh, &mut out);
    let mut limits = g.graph.limits.clone();
    for l in limits_of(g.set) {
        if !limits.iter().any(|o| o.bone == l.bone) {
            limits.push(l);
        }
    }
    joint_limits(&ctx, &s, &limits, &mut out);
    if let [c] = st_clips[..] {
        leap_apex(&ctx, g.set.clip(c), &s, rate, &mut out);
    }
    if rules.hover_band(&ctx.subject).is_some() {
        skip_missing(&mut out, &[("hover", ""), ("ground_penetration", ""), ("joint_limits", NO_LIMITS)]);
    } else {
        let why = if tracks.is_empty() { "no feet found (a graph's feet, footstep events or bones named foot*)" } else { "not measured for this state" };
        skip_missing(&mut out, &[("foot_sliding", why), ("floating", why), ("ground_penetration", ""), ("joint_limits", NO_LIMITS)]);
    }
    // The parameters may move the state on: say so (the checks then cover what played).
    let end = play::state_at(g, &play, timing, window, &mut sc);
    if let Some(top) = end[0].stack.last().filter(|e| e.state != state) {
        out.notes.push(format!("with these parameters '{state}' moves on to '{}' within {window:.2} s: the checks cover what played; give parameters that hold it", top.state));
    }
    Ok(out)
}

/// Checks the transition caused by changing parameters from `from` to `to` after `settle_s`
/// seconds: the pops it makes (excess per-frame joint jumps over the window the blend spans,
/// against both sides moving on their own), plus penetration and floating over the window (a
/// blend that lifts the body off its feet hovers).
#[allow(clippy::too_many_arguments)]
pub fn check_transition(g: Graphed, rig: &str, graph_name: &str, from: &[(String, Value)], to: &[(String, Value)], settle_s: f32, rules: &MotionRules, mesh: Option<&specula_model::MeshData>) -> Report {
    let mut before = GraphPlay::new(graph_name);
    for (n, v) in from {
        before.set_param(n, v.clone());
    }
    let t0 = Timing { anchor: 0.0, speed: 1.0 };
    let ts = settle_s.max(0.0) as f64;
    let mut sc = Scratch::default();
    let (mut after, _) = play::settle(g, &before, t0, ts, &mut sc);
    for (n, v) in to {
        after.set_param(n, v.clone());
    }
    let t1 = Timing { anchor: ts, speed: 1.0 };
    let names = |p: &GraphPlay, tm: Timing, t: f64, sc: &mut Scratch| play::state_at(g, p, tm, t, sc)[0].stack.last().map(|e| e.state.clone()).unwrap_or_default();
    let (a, b) = (names(&before, t0, ts, &mut sc), names(&after, t1, ts + 1.5, &mut sc));
    let ctx = Ctx { set: g.set, rules, subject: format!("{rig}/{graph_name}.{a}->{b}"), also: Vec::new() };
    // The longest blend the change could start, plus a margin.
    let blend = g.graph.layers.iter().flat_map(|l| l.transitions.iter().map(|t| t.blend)).fold(0.0f32, f32::max).min(2.0) as f64;
    let (w0, w1) = (ts - 0.1, ts + blend + 0.2);
    let hz = rules.sample_hz;
    let n = frames_for((w1 - w0) as f32, hz);
    let dt = (w1 - w0) / n as f64;
    let mut player = GraphPlayer::new();
    let eval = |player: &mut GraphPlayer, p: &GraphPlay, tm: Timing, t: f64| -> Vec<V3> {
        player.evaluate(g, p, tm, t, &Surroundings::default());
        player.model().iter().map(|m| m.t).collect()
    };
    // The target side on its own: the play after the change with its blends collapsed.
    let (settled_to, _) = play::settle(g, &after, t1, ts + blend + 0.25, &mut sc);
    let tm_to = Timing { anchor: ts + blend + 0.25, speed: 1.0 };
    let (mut worst, mut worst_at, mut worst_bone) = (0.0f32, 0.0f32, 0usize);
    let mut prev: Option<(Vec<V3>, Vec<V3>, Vec<V3>)> = None;
    let mut s = Sampled { dt: dt as f32, heads: Vec::new(), tails: Vec::new(), poses: Vec::new() };
    for k in 0..=n {
        let t = w0 + dt * k as f64;
        let actual = if t < ts { eval(&mut player, &before, t0, t) } else { eval(&mut player, &after, t1, t) };
        let pose = player.pose().clone();
        let model = player.model().to_vec();
        push_frame(&mut s, g.set, &pose, &model, [0.0; 3]);
        let src = eval(&mut player, &before, t0, t);
        let dst = eval(&mut player, &settled_to, tm_to, t);
        if let Some((pa, ps, pd)) = &prev {
            for j in 1..actual.len() {
                let jump = math3::length(math3::sub(actual[j], pa[j]));
                let own = math3::length(math3::sub(src[j], ps[j])).max(math3::length(math3::sub(dst[j], pd[j])));
                let excess = jump - own;
                if excess > worst {
                    (worst, worst_at, worst_bone) = (excess, (t - ts) as f32, j);
                }
            }
        }
        prev = Some((actual, src, dst));
    }
    let mut out = Report::default();
    // Scaled to a 1.8 m figure: the rest pose's height.
    let skel = &g.set.skeleton;
    let height = skel.heads.iter().chain(&skel.tails).map(|p| p[1]).fold(0.0f32, f32::max).max(0.1);
    let to_human = 1.8 / height;
    ctx.check(&mut out, "pops", worst * 100.0 * to_human, "cm", Some(worst_at), Some(worst_bone), vec![("blend_s", blend as f32), ("unscaled_cm", worst * 100.0), ("height_m", height)]);
    ground_penetration(&ctx, &s, mesh, &mut out);
    let mut limits = g.graph.limits.clone();
    for l in limits_of(g.set) {
        if !limits.iter().any(|o| o.bone == l.bone) {
            limits.push(l);
        }
    }
    joint_limits(&ctx, &s, &limits, &mut out);
    match rules.hover_band(&ctx.subject) {
        Some(band) => {
            hover(&ctx, &s, band, mesh, &mut out);
            skip_missing(&mut out, &[("joint_limits", NO_LIMITS)]);
        }
        None => {
            let tracks = foot_tracks(g.set, &s, &feet_of(g.set, Some(g.graph), None), mesh);
            floating(&ctx, &s, &tracks, &mut out);
            skip_missing(&mut out, &[("foot_sliding", "a blend window (feet slide in a crossfade by design; the states on each side are checked)"), ("joint_limits", NO_LIMITS)]);
        }
    }
    out
}

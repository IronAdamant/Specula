//! Motion checks for a rail camera, measured on its path sampled at the tick rate: no sudden jumps
//! in position (speed, acceleration) or rotation (turn rate, turn acceleration), never upside
//! down (the view stays clear of straight up and down, where a camera with a world-up horizon
//! flips), and roll held at zero unless the limits allow it. Cuts between camera tracks are
//! deliberate and aren't measured across. (Which checks need the world, such as the subject on
//! screen or the camera passing through geometry, are the engine's.)

use crate::eval::Pose;
use crate::rail::{dot, length, scale, sub, V3};
use crate::timeline::Timeline;
use specula_core::dmath;
use specula_core::Json;

/// Samples a check measures at most.
pub const MAX_SAMPLES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// m/s.
    pub max_speed: f64,
    /// m/s²: a kink between keys, or a step, shows here.
    pub max_accel: f64,
    /// Degrees per second the view direction turns.
    pub max_turn: f64,
    /// Degrees per second².
    pub max_turn_accel: f64,
    /// Degrees above or below the horizon the view may point (90 is straight up or down).
    pub max_pitch: f64,
    /// Degrees of roll allowed (0 unless a shot asks for it).
    pub max_roll: f64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_speed: 40.0, max_accel: 80.0, max_turn: 90.0, max_turn_accel: 720.0, max_pitch: 80.0, max_roll: 0.5 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub t: f64,
    pub pose: Pose,
}

/// One measured check.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub check: &'static str,
    pub ok: bool,
    /// The worst value measured.
    pub value: f64,
    pub limit: f64,
    pub unit: &'static str,
    /// When the worst value was measured.
    pub at_t: Option<f64>,
    pub message: String,
}

fn round(x: f64, places: i32) -> f64 {
    let k = [1.0, 10.0, 100.0, 1000.0, 10_000.0][places.clamp(0, 4) as usize];
    dmath::round_f64(x * k) / k
}

impl Check {
    pub fn to_json(&self) -> Json {
        let mut f = vec![
            ("check", Json::from(self.check)),
            ("ok", Json::from(self.ok)),
            ("value", Json::from(round(self.value, 3))),
            ("limit", Json::from(round(self.limit, 3))),
            ("unit", Json::from(self.unit)),
        ];
        if let Some(t) = self.at_t {
            f.push(("at_t", Json::from(round(t, 4))));
        }
        f.push(("message", Json::from(self.message.as_str())));
        Json::obj(f)
    }
}

/// The camera path from `from` to `to` (seconds), every `1 / hz`, both ends included; at most
/// [`MAX_SAMPLES`] samples (the step widens to fit).
pub fn sample(tl: &Timeline, from: f64, to: f64, hz: f64, resolve: &dyn Fn(&str) -> Option<V3>) -> Vec<Sample> {
    let n = sample_count(from, to, hz);
    (0..n)
        .filter_map(|k| {
            let t = sample_time(from, to, n, k);
            tl.camera_at(t, resolve).map(|pose| Sample { t, pose })
        })
        .collect()
}

/// How many samples [`sample`] takes from `from` to `to` at `hz` (2..=[`MAX_SAMPLES`]): a caller
/// that samples in slices takes the same ones.
pub fn sample_count(from: f64, to: f64, hz: f64) -> usize {
    let span = (to - from).max(0.0);
    (dmath::ceil_f64(span * hz.max(1.0)) as usize + 1).clamp(2, MAX_SAMPLES)
}

/// The time of sample `k` of `n`.
pub fn sample_time(from: f64, to: f64, n: usize, k: usize) -> f64 {
    from + (to - from).max(0.0) * k as f64 / (n.max(2) - 1) as f64
}

fn degrees(rad: f64) -> f64 {
    rad * (180.0 / std::f64::consts::PI)
}

/// Pitch of a unit direction above the horizon, degrees.
pub fn pitch_deg(forward: V3) -> f64 {
    degrees(dmath::asin_f64(forward[1].clamp(-1.0, 1.0)))
}

/// The motion checks of a sampled path. `cuts`: times the camera cuts (not measured across).
pub fn motion(samples: &[Sample], cuts: &[f64], limits: &Limits) -> Vec<Check> {
    let crosses_cut = |a: f64, b: f64| cuts.iter().any(|c| *c > a && *c <= b);
    let mut speed = (0.0f64, None);
    let mut accel = (0.0f64, None);
    let mut turn = (0.0f64, None);
    let mut turn_accel = (0.0f64, None);
    let mut pitch = (0.0f64, None);
    let mut roll = (0.0f64, None);
    let mut spin = (0.0f64, None);
    let mut prev_v: Option<V3> = None;
    let mut prev_turn: Option<f64> = None;
    let worse = |slot: &mut (f64, Option<f64>), v: f64, t: f64| {
        if v > slot.0 || slot.1.is_none() {
            *slot = (v, Some(t));
        }
    };
    for (i, s) in samples.iter().enumerate() {
        let f = s.pose.forward();
        worse(&mut pitch, pitch_deg(f).abs(), s.t);
        worse(&mut roll, s.pose.roll_deg.abs(), s.t);
        let Some(p) = i.checked_sub(1).map(|j| samples[j]) else { continue };
        let dt = s.t - p.t;
        if dt <= 0.0 || crosses_cut(p.t, s.t) {
            prev_v = None;
            prev_turn = None;
            continue;
        }
        let v = scale(sub(s.pose.eye, p.pose.eye), 1.0 / dt);
        worse(&mut speed, length(v), s.t);
        if let Some(pv) = prev_v {
            worse(&mut accel, length(sub(v, pv)) / dt, s.t);
        }
        prev_v = Some(v);
        let cos = dot(p.pose.forward(), f).clamp(-1.0, 1.0);
        let w = degrees(dmath::acos_f64(cos)) / dt;
        worse(&mut turn, w, s.t);
        if let Some(pw) = prev_turn {
            worse(&mut turn_accel, (w - pw).abs() / dt, s.t);
        }
        prev_turn = Some(w);
        // The picture's own rotation: how fast the screen's horizontal (the camera's right vector
        // under a world-up horizon) turns. Turning left or right turns it at the yaw rate; near
        // straight up or down it whips round while the view barely moves (sight-skeptic, round 1:
        // a rail just under the upright limit could spin the image with every other check passing).
        if let (Some(a), Some(b)) = (right_of(p.pose.forward()), right_of(f)) {
            let c = dot(a, b).clamp(-1.0, 1.0);
            worse(&mut spin, degrees(dmath::acos_f64(c)) / dt, s.t);
        }
    }
    let check = |name: &'static str, (value, at): (f64, Option<f64>), limit: f64, unit: &'static str, what: &str| {
        let ok = value <= limit + 1e-9;
        let message = if ok { format!("{what}: at most {} {unit}", round(value, 2)) } else { format!("{what} reaches {} {unit} at t = {} s (limit {limit})", round(value, 2), round(at.unwrap_or(0.0), 3)) };
        Check { check: name, ok, value, limit, unit, at_t: at, message }
    };
    vec![
        check("speed", speed, limits.max_speed, "m/s", "the camera's speed"),
        check("jump", accel, limits.max_accel, "m/s2", "sudden changes of the camera's velocity (a jump or a kink between keys)"),
        check("turn_rate", turn, limits.max_turn, "deg/s", "how fast the view turns"),
        check("turn_jump", turn_accel, limits.max_turn_accel, "deg/s2", "sudden changes of the turn rate"),
        check("upright", pitch, limits.max_pitch, "deg", "the view's angle above or below the horizon (90 flips the camera)"),
        check("roll", roll, limits.max_roll, "deg", "roll (kept at zero unless the shot asks for it)"),
        check("horizon_spin", spin, limits.max_turn, "deg/s", "how fast the picture's horizon turns (it whips round near straight up or down)"),
    ]
}

/// The camera's right vector under a world-up horizon (`None` looking straight up or down).
fn right_of(forward: V3) -> Option<V3> {
    let r = [-forward[2], 0.0, forward[0]];
    let l = length(r);
    (l > 1e-6).then(|| scale(r, 1.0 / l))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::tests::sample as timeline;

    fn none(_: &str) -> Option<V3> {
        None
    }

    fn checks(text: &str) -> Vec<Check> {
        let t = Timeline::parse_text(text).unwrap();
        let s = sample(&t, 0.0, t.duration, 30.0, &none);
        motion(&s, &t.camera_cuts(), &Limits::default())
    }

    fn get<'a>(c: &'a [Check], name: &str) -> &'a Check {
        c.iter().find(|c| c.check == name).unwrap()
    }

    #[test]
    fn a_smooth_rail_passes() {
        let t = timeline();
        let s = sample(&t, 0.0, t.duration, 30.0, &none);
        assert_eq!(s.len(), 361);
        let c = motion(&s, &t.camera_cuts(), &Limits::default());
        assert!(c.iter().all(|c| c.ok), "{c:#?}");
        assert!(get(&c, "speed").value > 5.0, "it does move");
    }

    #[test]
    fn steps_spins_flips_and_roll_fail() {
        let cam = |mv: &str, look: &str, extra: &str| {
            format!(r#"{{"schema": "specula.timeline.v1", "tracks": [{{"kind": "camera", "rail": {{"points": [[0,10,0],[40,10,0]]}}, "move": {mv}, "look": {look}{extra}}}]}}"#)
        };
        let smooth = r#"[{"t": 0, "at": 0}, {"t": 4, "at": 1, "ease": "smooth"}]"#;
        assert!(checks(&cam(smooth, r#"{"point": [20,0,-30]}"#, "")).iter().all(|c| c.ok));
        // A step: the camera jumps 40 m in one tick.
        let c = checks(&cam(r#"[{"t": 0, "at": 0}, {"t": 2, "at": 1, "ease": "step"}]"#, r#"{"point": [20,0,-30]}"#, ""));
        assert!(!get(&c, "speed").ok && !get(&c, "jump").ok, "{c:#?}");
        let at = get(&c, "speed").at_t.unwrap();
        assert!((at - 2.0).abs() < 0.05, "the jump is at the key: {at}");
        // Looking straight down flips.
        let c = checks(&cam(smooth, r#"{"rail": {"points": [[0,0,0.001],[40,0,0.001]]}}"#, ""));
        assert!(!get(&c, "upright").ok, "{c:#?}");
        // Sight-skeptic: looking almost straight down while drifting past the point below spins
        // the picture; with upright allowed to 90 only horizon_spin sees it.
        let t = Timeline::parse_text(r#"{"schema": "specula.timeline.v1", "tracks": [{"kind": "camera", "rail": {"points": [[-1,30,0.5],[1,30,0.5]]}, "move": [{"t": 0, "at": 0}, {"t": 2, "at": 1, "ease": "smooth"}], "look": {"point": [0,0,0]}}]}"#).unwrap();
        let lax = Limits { max_pitch: 90.0, ..Limits::default() };
        let c = motion(&sample(&t, 0.0, t.duration, 30.0, &none), &[], &lax);
        assert!(get(&c, "upright").ok && get(&c, "turn_rate").ok, "{c:#?}");
        assert!(!get(&c, "horizon_spin").ok, "{c:#?}");
        // Roll is refused unless allowed.
        let c = checks(&cam(smooth, r#"{"point": [20,0,-30]}"#, r#", "roll": 20"#));
        assert!(!get(&c, "roll").ok);
        // A whip pan.
        let c = checks(&cam(smooth, r#"{"rail": {"points": [[0,10,-5],[40,10,5]]}}"#, ""));
        assert!(!get(&c, "turn_rate").ok, "{c:#?}");
    }

    #[test]
    fn cuts_are_not_jumps() {
        let text = r#"{"schema": "specula.timeline.v1", "tracks": [
          {"kind": "camera", "name": "a", "rail": {"points": [[0,10,0],[4,10,0]]}, "move": [{"t": 0, "at": 0}, {"t": 2, "at": 1, "ease": "smooth"}], "look": {"point": [0,0,-20]}},
          {"kind": "camera", "name": "b", "rail": {"points": [[100,10,0],[104,10,0]]}, "move": [{"t": 2.5, "at": 0}, {"t": 5, "at": 1, "ease": "smooth"}], "look": {"point": [100,0,-20]}}]}"#;
        let c = checks(text);
        assert!(c.iter().all(|c| c.ok), "{c:#?}");
        // Probatio survivors: only the step across the cut is skipped, never the rest. A step
        // inside the second camera's span (not at the cut) is still a jump.
        let stepped = text.replace(r#"{"t": 5, "at": 1, "ease": "smooth"}"#, r#"{"t": 4, "at": 0.2}, {"t": 4.5, "at": 1, "ease": "step"}"#);
        let c = checks(&stepped);
        let speed = get(&c, "speed");
        assert!(!speed.ok, "{c:#?}");
        assert!((speed.at_t.unwrap() - 4.5).abs() < 0.05, "at the step, not the cut: {:?}", speed.at_t);
    }

    #[test]
    fn a_cut_at_a_sample_is_skipped_exactly_there() {
        // Samples at 0, 1, 2 with a cut at 1: only the step 0 -> 1 crosses it.
        let pose = |x: f64| Pose { eye: [x, 10.0, 0.0], target: [x, 0.0, -20.0], fov_deg: 50.0, roll_deg: 0.0, track: 0, look_resolved: true };
        let s = [Sample { t: 0.0, pose: pose(0.0) }, Sample { t: 1.0, pose: pose(100.0) }, Sample { t: 2.0, pose: pose(101.0) }];
        let c = motion(&s, &[1.0], &Limits::default());
        assert!((get(&c, "speed").value - 1.0).abs() < 1e-9, "the 100 m cut isn't speed; the 1 m step after it is: {c:#?}");
        let c = motion(&s, &[], &Limits::default());
        assert!(!get(&c, "speed").ok, "without the cut it is a jump");
        // Equal times are skipped, never divided by.
        let same = [Sample { t: 1.0, pose: pose(0.0) }, Sample { t: 1.0, pose: pose(50.0) }];
        assert!(motion(&same, &[], &Limits::default()).iter().all(|c| c.value.is_finite()));
        // Exactly at a limit passes.
        let at = Limits { max_speed: 1.0, ..Limits::default() };
        assert!(get(&motion(&s, &[1.0], &at), "speed").ok);
    }
}

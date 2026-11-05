//! Camera rails: centripetal Catmull-Rom splines through points, parameterised by **arc length**,
//! so `at = 0.5` is halfway along the rail in metres, and equal steps of `at` move the camera
//! equal distances (no speeding up where points are far apart).
//!
//! Centripetal (alpha 0.5) Catmull-Rom never loops or overshoots into cusps between close points,
//! which keeps a camera from kinking. Open rails get mirrored end controls; closed rails wrap.
//! Arc length comes from a table of [`SAMPLES_PER_SEGMENT`] chords per segment, built once; a
//! position is found by a binary search in it and evaluated on the curve itself. Only `+ − × ÷`
//! and `sqrt` (both IEEE-exact), so a rail is the same on every machine.

use crate::error::{read, Path, Result, TimelineError};
use specula_core::Json;

/// Chords per segment in the arc-length table.
pub const SAMPLES_PER_SEGMENT: usize = 48;
/// Points a rail holds.
pub const MIN_POINTS: usize = 2;
pub const MAX_POINTS: usize = 64;
/// Coordinates are within this many metres of the origin.
pub const MAX_COORD: f64 = 100_000.0;
/// Neighbouring points closer than this are refused (a rail can't stand still on itself).
pub const MIN_GAP: f64 = 0.001;

pub type V3 = [f64; 3];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn length(a: V3) -> f64 {
    dot(a, a).sqrt()
}
pub fn lerp(a: V3, b: V3, u: f64) -> V3 {
    [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u, a[2] + (b[2] - a[2]) * u]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// A spline through points, with its arc-length table.
#[derive(Clone, Debug, PartialEq)]
pub struct Rail {
    pub points: Vec<V3>,
    pub closed: bool,
    /// `(arc length at the sample, segment, local parameter 0..1)`, in order.
    table: Vec<(f64, u32, f64)>,
    length: f64,
}

impl Rail {
    /// A rail through `points` (validated as [`Rail::parse`] does).
    pub fn new(points: Vec<V3>, closed: bool) -> Result<Rail> {
        let at = Path::root("points");
        if points.len() < MIN_POINTS || points.len() > MAX_POINTS {
            return Err(TimelineError::new("invalid_value", at.as_str(), format!("a rail has {MIN_POINTS} to {MAX_POINTS} points (got {})", points.len())).allowed(format!("{MIN_POINTS}..={MAX_POINTS} points")));
        }
        if closed && points.len() < 3 {
            return Err(TimelineError::new("invalid_value", "closed", "a closed rail needs at least 3 points"));
        }
        let n = points.len();
        let pairs = if closed { n } else { n - 1 };
        for i in 0..pairs {
            if length(sub(points[(i + 1) % n], points[i])) < MIN_GAP {
                return Err(TimelineError::new("duplicate", at.index((i + 1) % n).as_str(), format!("points {i} and {} are closer than {MIN_GAP} m: remove one", (i + 1) % n)));
            }
        }
        let mut r = Rail { points, closed, table: Vec::new(), length: 0.0 };
        r.build();
        Ok(r)
    }

    /// `{points: [[x,y,z], ...], closed?}`.
    pub fn parse(v: &Json, at: &Path) -> Result<Rail> {
        read::keys(v, at, &["points", "closed"])?;
        let list = read::array(read::need(v, "points", at, "the rail's points [[x, y, z], ...] in metres")?, &at.key("points"), MIN_POINTS, MAX_POINTS)?;
        let mut points = Vec::with_capacity(list.len());
        for (i, p) in list.iter().enumerate() {
            points.push(read::vec3(p, &at.key("points").index(i), MAX_COORD)?);
        }
        let closed = read::boolean_or(v, "closed", at, false)?;
        Rail::new(points, closed).map_err(|mut e| {
            e.field = format!("{at}.{}", e.field);
            e
        })
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![("points", Json::arr(self.points.iter().map(|p| Json::arr(p.iter().map(|c| Json::from(*c))))))];
        if self.closed {
            f.push(("closed", Json::from(true)));
        }
        Json::obj(f)
    }

    pub fn segments(&self) -> usize {
        if self.closed {
            self.points.len()
        } else {
            self.points.len() - 1
        }
    }

    /// Total arc length in metres.
    pub fn length(&self) -> f64 {
        self.length
    }

    /// The four control points of segment `i` (from point `i` to point `i + 1`).
    fn controls(&self, i: usize) -> [V3; 4] {
        let n = self.points.len();
        let p = |k: isize| -> V3 {
            if self.closed {
                self.points[k.rem_euclid(n as isize) as usize]
            } else if k < 0 {
                // Mirrored: the curve leaves the first point heading for the second.
                sub(scale(self.points[0], 2.0), self.points[1])
            } else if k as usize >= n {
                sub(scale(self.points[n - 1], 2.0), self.points[n - 2])
            } else {
                self.points[k as usize]
            }
        };
        let i = i as isize;
        [p(i - 1), p(i), p(i + 1), p(i + 2)]
    }

    /// The point at local parameter `u` (0..1) of segment `seg`.
    pub fn segment_point(&self, seg: usize, u: f64) -> V3 {
        let [p0, p1, p2, p3] = self.controls(seg);
        let knot = |a: V3, b: V3| length(sub(b, a)).sqrt().max(1e-6);
        let t0 = 0.0;
        let t1 = t0 + knot(p0, p1);
        let t2 = t1 + knot(p1, p2);
        let t3 = t2 + knot(p2, p3);
        let t = t1 + (t2 - t1) * u.clamp(0.0, 1.0);
        let mix = |a: V3, b: V3, ta: f64, tb: f64| lerp(a, b, (t - ta) / (tb - ta));
        let a1 = mix(p0, p1, t0, t1);
        let a2 = mix(p1, p2, t1, t2);
        let a3 = mix(p2, p3, t2, t3);
        let b1 = mix(a1, a2, t0, t2);
        let b2 = mix(a2, a3, t1, t3);
        mix(b1, b2, t1, t2)
    }

    fn build(&mut self) {
        let segs = self.segments();
        self.table = Vec::with_capacity(segs * SAMPLES_PER_SEGMENT + 1);
        let mut s = 0.0;
        let mut prev = self.points[0];
        self.table.push((0.0, 0, 0.0));
        for seg in 0..segs {
            for k in 1..=SAMPLES_PER_SEGMENT {
                let u = k as f64 / SAMPLES_PER_SEGMENT as f64;
                let p = self.segment_point(seg, u);
                s += length(sub(p, prev));
                prev = p;
                self.table.push((s, seg as u32, u));
            }
        }
        self.length = s;
    }

    /// The point `at` (0..1) of the way along the rail by arc length.
    pub fn point_at(&self, at: f64) -> V3 {
        let at = if at.is_nan() { 0.0 } else { at.clamp(0.0, 1.0) };
        if self.length <= 0.0 {
            return self.points[0];
        }
        let s = at * self.length;
        // The first table entry at or past s.
        let i = self.table.partition_point(|e| e.0 < s).clamp(1, self.table.len() - 1);
        let (s0, seg0, u0) = self.table[i - 1];
        let (s1, seg1, u1) = self.table[i];
        let f = if s1 > s0 { (s - s0) / (s1 - s0) } else { 0.0 };
        // Both ends of a chord lie in one segment, except where a chord starts a segment (u0 is
        // then the previous segment's 1.0, the same point as this segment's 0.0).
        let (seg, ua) = if seg0 == seg1 { (seg1, u0) } else { (seg1, 0.0) };
        self.segment_point(seg as usize, ua + (u1 - ua) * f)
    }

    /// Where the rail is at `at` and its unit direction there (towards larger `at`).
    pub fn frame_at(&self, at: f64) -> (V3, V3) {
        let p = self.point_at(at);
        let h = (0.5 / self.length.max(1e-6)).min(0.01);
        let (a, b) = if at + h <= 1.0 { (p, self.point_at(at + h)) } else { (self.point_at(at - h), p) };
        let d = sub(b, a);
        let l = length(d);
        (p, if l > 0.0 { scale(d, 1.0 / l) } else { [0.0, 0.0, 1.0] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_straight_rail_is_a_line_at_constant_speed() {
        let r = Rail::new(vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]], false).unwrap();
        assert!((r.length() - 10.0).abs() < 1e-6, "{}", r.length());
        for i in 0..=10 {
            let p = r.point_at(i as f64 / 10.0);
            assert!((p[0] - i as f64).abs() < 1e-6 && p[1].abs() < 1e-9, "{i}: {p:?}");
        }
    }

    #[test]
    fn rails_pass_through_their_points_at_even_speed() {
        let pts = vec![[0.0, 2.0, 0.0], [10.0, 4.0, 0.0], [12.0, 4.0, 1.0], [30.0, 0.0, 20.0]];
        let r = Rail::new(pts.clone(), false).unwrap();
        // Through every point (segment ends).
        for (i, p) in pts.iter().enumerate().take(3) {
            let q = r.segment_point(i, 0.0);
            assert!(length(sub(*p, q)) < 1e-9, "{i}");
        }
        assert!(length(sub(r.point_at(1.0), pts[3])) < 1e-6);
        // Equal steps of `at` cover equal distances (to the table's resolution).
        let n = 200;
        let step = r.length() / n as f64;
        let mut prev = r.point_at(0.0);
        for k in 1..=n {
            let p = r.point_at(k as f64 / n as f64);
            let d = length(sub(p, prev));
            assert!((d - step).abs() < step * 0.02, "step {k}: {d} vs {step}");
            prev = p;
        }
        // The same bits every time.
        let again = Rail::new(pts, false).unwrap();
        assert_eq!(again.point_at(0.37), r.point_at(0.37));
    }

    #[test]
    fn closed_rails_wrap_and_bad_rails_are_refused() {
        let r = Rail::new(vec![[10.0, 0.0, 0.0], [0.0, 0.0, 10.0], [-10.0, 0.0, 0.0], [0.0, 0.0, -10.0]], true).unwrap();
        assert!(length(sub(r.point_at(0.0), r.point_at(1.0))) < 1e-6, "a closed rail ends where it starts");
        assert!(r.length() > 55.0 && r.length() < 70.0, "{}", r.length());
        assert_eq!(Rail::new(vec![[0.0; 3]], false).unwrap_err().code, "invalid_value");
        assert_eq!(Rail::new(vec![[0.0; 3], [0.0; 3]], false).unwrap_err().code, "duplicate");
        assert!(Rail::new(vec![[0.0; 3], [1.0, 0.0, 0.0]], true).is_err());
        let j = Json::parse(r#"{"points": [[0,0,0],[1,0,"x"]]}"#).unwrap();
        let e = Rail::parse(&j, &Path::root("rail")).unwrap_err();
        assert_eq!(e.field, "rail.points[1]");
        let j = Json::parse(r#"{"points": [[0,0,0],[1,0,0]], "bend": 1}"#).unwrap();
        assert_eq!(Rail::parse(&j, &Path::root("rail")).unwrap_err().code, "unknown_field");
        let (_, d) = Rail::new(vec![[0.0; 3], [0.0, 0.0, 5.0]], false).unwrap().frame_at(0.5);
        assert!((d[2] - 1.0).abs() < 1e-9);
    }
}

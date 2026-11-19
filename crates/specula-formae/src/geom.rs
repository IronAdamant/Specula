//! A form's exact geometry, and everything the library derives from it: the point count before
//! building, the build, the exact measures where every piece is a line or an arc, and the
//! error at a detail.
//!
//! A ring (or a curve) is a list of pieces end to end. Each piece emits its start point and
//! its interior sample points; the next piece's start is its end, so a ring has no repeated end
//! point. A curve emits its last end point too, unless it is closed.

use crate::detail::segments;
use crate::maths::*;
use crate::outline::{Curve, Outline};
use crate::Measures;
use std::rc::Rc;

/// A parametric piece's function: the point at a parameter.
pub(crate) type PtFn = Rc<dyn Fn(f64) -> Pt>;

/// One piece of a ring or a curve.
#[derive(Clone)]
pub(crate) enum Seg {
    /// A straight piece from `a` to `b`.
    Line { a: Pt, b: Pt },
    /// An arc of the circle of centre `c` and radius `r`, from angle `a0` turning by `sweep`
    /// radians (positive counter-clockwise).
    Arc { c: Pt, r: f64, a0: f64, sweep: f64 },
    /// A parametric piece from `t0` to `t1`, sampled evenly in its parameter, which is worth
    /// `turns` full turns of detail.
    Fn { f: PtFn, t0: f64, t1: f64, turns: f64 },
}

impl Seg {
    pub fn start(&self) -> Pt {
        match self {
            Seg::Line { a, .. } => *a,
            Seg::Arc { c, r, a0, .. } => polar(*c, *r, *a0),
            Seg::Fn { f, t0, .. } => f(*t0),
        }
    }
    pub fn end(&self) -> Pt {
        match self {
            Seg::Line { b, .. } => *b,
            Seg::Arc { c, r, a0, sweep } => polar(*c, *r, a0 + sweep),
            Seg::Fn { f, t1, .. } => f(*t1),
        }
    }
    /// How many sample segments this piece is cut into at `detail`.
    pub fn k(&self, detail: u32) -> u64 {
        match self {
            Seg::Line { .. } => 1,
            Seg::Arc { sweep, .. } => segments(sweep.abs() / TAU, detail),
            Seg::Fn { turns, .. } => segments(*turns, detail),
        }
    }
    #[cfg(test)]
    pub fn curved(&self) -> bool {
        !matches!(self, Seg::Line { .. })
    }
    /// The point at `i / k` of the way along the piece.
    fn at(&self, i: u64, k: u64) -> Pt {
        let u = i as f64 / k as f64;
        match self {
            Seg::Line { a, b } => [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u],
            Seg::Arc { c, r, a0, sweep } => polar(*c, *r, a0 + sweep * u),
            Seg::Fn { f, t0, t1, .. } => f(t0 + (t1 - t0) * u),
        }
    }
    /// Emits the start and interior points at `detail`; returns the greatest distance from a
    /// sample segment's chord to the true curve: for an arc its sag, exactly; for a parametric
    /// piece the greatest of the distances at a quarter, a half and three quarters of each
    /// sample's parameter (an estimate, not a bound).
    fn emit(&self, detail: u32, out: &mut Vec<Pt>) -> f64 {
        let k = self.k(detail);
        let mut worst = 0.0f64;
        let mut prev = self.at(0, k);
        out.push(prev);
        for i in 1..=k {
            let p = self.at(i, k);
            if i < k {
                out.push(p);
            }
            match self {
                Seg::Line { .. } => {}
                Seg::Arc { r, sweep, .. } => worst = worst.max(r * (1.0 - cos(sweep.abs() / (2 * k) as f64))),
                Seg::Fn { .. } => {
                    // At a quarter, a half and three quarters of the sample's parameter: the
                    // middle alone misses a curve that runs past the chord's end (a needle
                    // ellipse's tip, where the chord is steep against the curve).
                    for q in 1..4 {
                        let m = self.at(4 * (i - 1) + q, 4 * k);
                        worst = worst.max(dist_to_segment(m, prev, p));
                    }
                }
            }
            prev = p;
        }
        worst
    }
}

/// What a form's `geom` returns.
#[derive(Clone)]
pub(crate) enum Geom {
    /// A plane figure: the outer ring (counter-clockwise), then holes (clockwise).
    Rings(Vec<Vec<Seg>>),
    /// A curve: open (its last end point is a point) or closed.
    Path { segs: Vec<Seg>, closed: bool },
}

impl Geom {
    /// The points and parts (rings; 1 for a curve) a build at `detail` will have, exactly.
    pub fn count(&self, detail: u32) -> (u64, u64) {
        let sum = |s: &[Seg]| s.iter().map(|x| x.k(detail)).fold(0u64, |a, b| a.saturating_add(b));
        match self {
            Geom::Rings(rings) => (rings.iter().map(|r| sum(r)).fold(0, u64::saturating_add), rings.len() as u64),
            Geom::Path { segs, closed } => (sum(segs).saturating_add(u64::from(!closed)), 1),
        }
    }

    /// The build at `detail` (call [`Geom::count`] against the ceiling first).
    pub fn build(&self, detail: u32) -> crate::Built {
        match self {
            Geom::Rings(rings) => {
                let mut err = 0.0f64;
                let rings = rings
                    .iter()
                    .map(|r| {
                        let mut pts = Vec::new();
                        for s in r {
                            err = err.max(s.emit(detail, &mut pts));
                        }
                        pts
                    })
                    .collect();
                crate::Built::Outline(Outline { rings, chord_error: err })
            }
            Geom::Path { segs, closed } => {
                let mut pts = Vec::new();
                let mut err = 0.0f64;
                for s in segs {
                    err = err.max(s.emit(detail, &mut pts));
                }
                if !closed {
                    if let Some(last) = segs.last() {
                        pts.push(last.end());
                    }
                }
                crate::Built::Curve(Curve { points: pts, closed: *closed, chord_error: err })
            }
        }
    }

    fn segs(&self) -> Vec<&Seg> {
        match self {
            Geom::Rings(r) => r.iter().flatten().collect(),
            Geom::Path { segs, .. } => segs.iter().collect(),
        }
    }

    /// Whether any piece is curved.
    #[cfg(test)]
    pub fn curved(&self) -> bool {
        self.segs().iter().any(|s| s.curved())
    }

    /// The greatest distance between the build at `detail` and the true outline, exactly, when
    /// every curved piece is an arc (the sag `r (1 - cos(δ/2))` of its sample angle δ); `None`
    /// when a parametric piece is there (then the build's own `chord_error` is the measure).
    pub fn sag(&self, detail: u32) -> Option<f64> {
        let mut worst = 0.0f64;
        for s in self.segs() {
            match s {
                Seg::Line { .. } => {}
                Seg::Arc { r, sweep, .. } => worst = worst.max(r * (1.0 - cos(sweep.abs() / (2 * s.k(detail)) as f64))),
                Seg::Fn { .. } => return None,
            }
        }
        Some(worst)
    }

    /// The exact measures, where every piece is a line or an arc: area (signed sum over the
    /// rings), perimeter, centroid and bounds, by integrals along each piece (Green's theorem).
    /// For a curve: its length and bounds.
    pub fn exact(&self) -> Measures {
        let segs = self.segs();
        if segs.iter().any(|s| matches!(s, Seg::Fn { .. })) {
            return Measures::default();
        }
        let (mut area, mut mx, mut my, mut length) = (0.0, 0.0, 0.0, 0.0);
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        let mut grow = |p: Pt| {
            for i in 0..2 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        };
        for s in &segs {
            match s {
                Seg::Line { a, b } => {
                    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                    area += 0.5 * (a[0] * b[1] - b[0] * a[1]);
                    mx += 0.5 * dy * (a[0] * a[0] + a[0] * dx + dx * dx / 3.0);
                    my -= 0.5 * dx * (a[1] * a[1] + a[1] * dy + dy * dy / 3.0);
                    length += len([dx, dy]);
                    grow(*a);
                    grow(*b);
                }
                Seg::Arc { c, r, a0, sweep } => {
                    let (cx, cy, r) = (c[0], c[1], *r);
                    let (p0, p1) = (*a0, a0 + sweep);
                    let (s0, c0) = sin_cos(p0);
                    let (s1, c1) = sin_cos(p1);
                    area += 0.5 * (r * r * sweep + r * (cx * (s1 - s0) - cy * (c1 - c0)));
                    // ∫ x² dy and ∫ y² dx along the arc.
                    let fx = |p: f64, s: f64, c: f64| r * (cx * cx * s + 2.0 * cx * r * (0.5 * p + 0.5 * s * c) + r * r * (s - s * s * s / 3.0));
                    let fy = |p: f64, s: f64, c: f64| -r * (-cy * cy * c + cy * r * (p - s * c) + r * r * (c * c * c / 3.0 - c));
                    mx += 0.5 * (fx(p1, s1, c1) - fx(p0, s0, c0));
                    my -= 0.5 * (fy(p1, s1, c1) - fy(p0, s0, c0));
                    length += r * sweep.abs();
                    grow(polar(*c, r, p0));
                    grow(polar(*c, r, p1));
                    // The axis points the arc passes.
                    let (from, to) = if *sweep >= 0.0 { (p0, p1) } else { (p1, p0) };
                    let mut q = ceil(from / (0.5 * PI)) * (0.5 * PI);
                    for _ in 0..8 {
                        if q > to {
                            break;
                        }
                        grow(polar(*c, r, q));
                        q += 0.5 * PI;
                    }
                }
                Seg::Fn { .. } => unreachable!("checked above"),
            }
        }
        let bounds = Some(([lo[0], lo[1], 0.0], [hi[0], hi[1], 0.0]));
        match self {
            Geom::Rings(_) => Measures {
                size: Some(area),
                boundary: Some(length),
                centroid: Some([mx / area, my / area, 0.0]),
                bounds,
                series: None,
            },
            Geom::Path { .. } => Measures { size: None, boundary: Some(length), centroid: None, bounds, series: None },
        }
    }

    #[cfg(test)]
    /// For an outline of lines and arcs only: the area its build at `detail` must have, which
    /// is the exact area less, for each sample chord of each arc, the signed area between the
    /// chord and the arc, `r²(δ − sin δ)/2` (δ the signed sample angle). `None` otherwise.
    pub fn built_area(&self, detail: u32) -> Option<f64> {
        let exact = self.exact().size?;
        let mut cut = 0.0;
        for s in self.segs() {
            if let Seg::Arc { r, sweep, .. } = s {
                let k = s.k(detail) as f64;
                let d = sweep / k;
                cut += k * 0.5 * r * r * (d - sin(d));
            }
        }
        Some(exact - cut)
    }

    #[cfg(test)]
    /// The largest gap between one piece's end and the next one's start (0 for a well-made
    /// ring): a test holds every form's to its size.
    pub fn gap(&self) -> f64 {
        let mut worst = 0.0f64;
        let mut chain = |s: &[Seg], closed: bool| {
            for i in 0..s.len() {
                if i + 1 == s.len() && !closed {
                    break;
                }
                let next = &s[(i + 1) % s.len()];
                worst = worst.max(dist(s[i].end(), next.start()));
            }
        };
        match self {
            Geom::Rings(r) => r.iter().for_each(|x| chain(x, true)),
            Geom::Path { segs, closed } => chain(segs, *closed),
        }
        worst
    }

}

/// A closed ring of straight sides through `pts`.
pub(crate) fn poly(pts: &[Pt]) -> Vec<Seg> {
    (0..pts.len()).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % pts.len()] }).collect()
}

/// A straight piece.
pub(crate) fn line(a: Pt, b: Pt) -> Seg {
    Seg::Line { a, b }
}

/// An arc from angle `a0` to angle `a1` (radians; `a1 < a0` turns clockwise).
pub(crate) fn arc(c: Pt, r: f64, a0: f64, a1: f64) -> Seg {
    Seg::Arc { c, r, a0, sweep: a1 - a0 }
}

/// A parametric piece.
pub(crate) fn func(f: impl Fn(f64) -> Pt + 'static, t0: f64, t1: f64, turns: f64) -> Seg {
    Seg::Fn { f: Rc::new(f), t0, t1, turns }
}

/// A parametric piece sharing a function with others.
pub(crate) fn func_rc(f: &PtFn, t0: f64, t1: f64, turns: f64) -> Seg {
    Seg::Fn { f: f.clone(), t0, t1, turns }
}

/// One outline of one ring.
pub(crate) fn ring(segs: Vec<Seg>) -> Geom {
    Geom::Rings(vec![segs])
}

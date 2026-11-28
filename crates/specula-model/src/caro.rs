//! Flesh on a skeleton as one field (`caro`, flesh): the base surface of a person or a
//! creature ([FABRICA.md], step 3), as a field that is **1-Lipschitz by construction** and
//! whose cull is **exact**. The statements and their proofs are in
//! `docs/design/DISTANCE_FIELDS.md`, section 7; this module is what they are about.
//!
//! - **A bone** ([`Bone`]) is a segment with sections along it (`[t, half breadth, half
//!   depth]`, `t` a share of its length, as the sculpt's `column` takes) and one squareness
//!   `p` (2 an ellipse, up to 12 a rounded box). Its field ([`Flesh::part_field`]) is
//!   `max(ψ / L, c)`: `ψ = m (min(n, 2) − 1)`, `n` the gauge of the section at the nearest
//!   station along the bone (with a cap of length `m` past each end), `m` the section's least
//!   half size, `L = √(1 + (g + 2k)²)` from how fast the sizes and the proportions change
//!   (7.1, 7.3), and `c` the distance to its axis less the largest half size it reaches. It is
//!   a bound of its flesh on both sides; its zero set is exactly the sections and the caps.
//! - **A swell** ([`Swell`]) is an ellipsoid in a bone's frame (7.4).
//! - **The body** ([`Flesh`]) joins its parts' values in order from the least by the quadratic
//!   join of width `smooth`, and holds the result at most `keep` (7.5). [`Flesh::value`] joins
//!   only the parts whose capsule, widened by [`Flesh::reach`] (`keep + 2 smooth`, and a
//!   little for rounding), holds the point, and is the same number as [`Flesh::whole`], which
//!   joins them all (7.6).
//!
//! What the next slice calls: [`Flesh::value`] (or [`Flesh::value_among`] with the list a block
//! got from [`Flesh::meets`]), and [`Flesh::range`] over a ball. Not wired into the recipe or
//! the mesher.
//!
//! **In `f32`, and what each allowance covers** (7.6, "In `f32`"). With `u = 2⁻²⁴` the unit
//! roundoff and `W` = [`Flesh::scale`], every value is within [`Flesh::slack`]
//! `σ(x) = 2⁻¹⁴ (|x|₁ + W)` of the exact field of the numbers this module stores. Counted:
//! - *the point in the part's frame* ([`Part::local`]): three differences (`u |d|₁`), three
//!   dot products of three terms (`3 u` of each), and a frame that is a turn only to within a
//!   few `u` of each component (it is made in `f64` and rounded): the point is evaluated as if
//!   moved by at most `12 u |d|`, `|d| ≤ |x|₁ + |a|₁`, and the exact field moves by no more;
//! - *the profile*: the share `f` along a piece (two roundings) and the cubic (eight products
//!   and sums of numbers at most six times the piece's greatest size): about `50 u` of the
//!   greatest size for `A`, as much relative for `q`, and `ψ` changes by at most twice a change
//!   of `A` or `B` where `n ≤ 2`: under `300 u R`;
//! - *the gauge and `ψ`*: a division each way, the norm (a root, or two correctly rounded
//!   powers), `e`, `n` and the product: under `25 u m`; the capsule: under `4 u (|d| + R)`;
//! - *the join of up to 64 values*: each join under `u (8 smooth + |value|)`, with
//!   `|value| ≤ keep + smooth + R`.
//!
//! That is under `350 u` of `|x|₁ + W` with `W` = the greatest `|a|₁ + R` of the parts plus
//! `9 smooth + keep`; `2⁻¹⁴` is `1024 u`. The numbers `L`, `R` and a swell's radii are worked
//! out in `f64` from the numbers stored and rounded the safe way, so each holds for the profile
//! this module evaluates; the tests hold `σ` against a reference in `f64`.
//!
//! **Determinism (F2):** basic operations, `sqrt`, and [`crate::dmath::pow`] (correctly
//! rounded) only; no hash maps; the order of a join is by value, then by part.
//!
//! [FABRICA.md]: ../../../docs/design/FABRICA.md

use crate::dmath;
use crate::math3::V3;

/// Most parts (bones with flesh, and swells) in one body.
pub const MAX_PARTS: usize = 64;
/// Most sections on one bone.
pub const MAX_STATIONS: usize = 8;
/// Most swells in one body.
pub const MAX_SWELLS: usize = 24;
/// How square a bone's sections may be (2 an ellipse; under 2 the gauge exceeds the length,
/// DISTANCE_FIELDS.md 7.2).
pub const SQUARE: (f32, f32) = (2.0, 12.0);
/// A half breadth, half depth or a swell's radius, metres.
pub const SIZE: (f32, f32) = (0.0005, 5.0);
/// Where along its bone a section may be (0 its head, 1 its tail).
pub const ALONG: (f32, f32) = (-1.0, 2.0);
/// How far a swell may sit off its bone's line, metres.
pub const OFFSET: f32 = 5.0;
/// The body's smoothing width, metres (0: a plain union).
pub const SMOOTH: (f32, f32) = (0.0, 0.5);
/// The level up to which the culled field is the whole field, metres (7.6's `T`).
pub const KEEP: (f32, f32) = (0.0005, 1.0);
/// How far from the origin any place may be, metres.
pub const EXTENT: f32 = crate::recipe::MAX_EXTENT_M;
/// Shorter than this a bone is a point: one section, two caps.
pub const LEAST_LENGTH: f32 = 1e-6;
/// The least gap between two stations, as a share of the bone.
pub const LEAST_GAP: f32 = 1e-3;
/// The gauge is held at this many sections out (7.3): past it the capsule carries the field.
const CLAMP: f32 = 2.0;
/// `2⁻¹⁴`: the slack's share of `|x|₁ + W` (see the module's doc).
const SLACK: f32 = 1.0 / 16384.0;

/// One bone and its flesh. A bone with no sections has none (it may still carry swells).
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    pub head: V3,
    pub tail: V3,
    /// Toward which the breadth is measured: X unless the bone runs nearly along X, then Z.
    pub side: Option<V3>,
    pub square: f32,
    /// `[t, half breadth, half depth]`, `t` rising, at least [`LEAST_GAP`] apart.
    pub sections: Vec<[f32; 3]>,
}

/// An ellipsoid on a bone: `at` is `[t along the bone, metres toward its side, metres
/// across]`; `radii` are along the bone, toward its side and across.
#[derive(Clone, Debug, PartialEq)]
pub struct Swell {
    pub bone: usize,
    pub at: V3,
    pub radii: V3,
}

/// Why a body could not be made: a code, the bone or swell it is about, and the number that
/// failed with its limits.
#[derive(Clone, Debug, PartialEq)]
pub struct CaroError {
    pub code: &'static str,
    pub bone: Option<usize>,
    pub swell: Option<usize>,
    pub measured: Option<f64>,
    pub limits: Option<(f64, f64)>,
}

impl std::fmt::Display for CaroError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "caro: {}", self.code)?;
        if let Some(b) = self.bone {
            write!(f, " (bone {b})")?;
        }
        if let Some(s) = self.swell {
            write!(f, " (swell {s})")?;
        }
        if let Some(m) = self.measured {
            write!(f, ": {m}")?;
        }
        if let Some((lo, hi)) = self.limits {
            write!(f, " not in [{lo}, {hi}]")?;
        }
        Ok(())
    }
}

impl std::error::Error for CaroError {}

fn fail(code: &'static str, bone: Option<usize>, swell: Option<usize>, measured: Option<f64>, limits: Option<(f32, f32)>) -> CaroError {
    CaroError { code, bone, swell, measured, limits: limits.map(|(a, b)| (f64::from(a), f64::from(b))) }
}

/// Which bone or swell a part of the body is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartOf {
    Bone(usize),
    Swell(usize),
}

/// One piece of a profile between two stations: `[A0, A1, Δ s0, Δ s1]` for the half breadth
/// and the same for the proportion `q = B / A` (7.1), from `t0` over `span` metres.
#[derive(Clone, Copy, Debug, Default)]
struct Piece {
    a: [f32; 4],
    q: [f32; 4],
    t0: f32,
    span: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Gauge {
    Two,
    Four,
    Eight,
    /// `p` and `1 / p`.
    Power(f32, f32),
}

#[derive(Clone, Debug)]
struct Shaft {
    origin: V3,
    frame: [V3; 3],
    /// The first and the last station, metres along the bone from its head.
    t0: f32,
    t1: f32,
    stations: [f32; MAX_STATIONS],
    count: usize,
    pieces: [Piece; MAX_STATIONS - 1],
    gauge: Gauge,
    /// 7.3's `L` (at least 1).
    lip: f32,
    /// 7.3's `R`: at least every section's farthest point from the axis.
    radius: f32,
}

#[derive(Clone, Debug)]
struct Egg {
    origin: V3,
    frame: [V3; 3],
    inv: V3,
    least: f32,
    most: f32,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant, reason = "parts sit in one Vec and are asked at every sample; a box would add a step to each, and swells are few")]
enum Part {
    Bone(Shaft),
    Egg(Egg),
}

impl Part {
    /// The point in the part's frame.
    #[inline]
    fn local(&self, x: V3) -> V3 {
        let (o, f) = match self {
            Part::Bone(b) => (b.origin, &b.frame),
            Part::Egg(e) => (e.origin, &e.frame),
        };
        let d = [x[0] - o[0], x[1] - o[1], x[2] - o[2]];
        let dot = |a: V3| d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
        [dot(f[0]), dot(f[1]), dot(f[2])]
    }

    /// 7.3's `c` (7.4's for a swell): the distance to its axis (its middle) less its reach.
    #[inline]
    fn capsule(&self, l: V3) -> f32 {
        match self {
            Part::Bone(b) => {
                let along = l[0] - l[0].clamp(b.t0, b.t1);
                (along * along + l[1] * l[1] + l[2] * l[2]).sqrt() - b.radius
            }
            Part::Egg(e) => (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt() - e.most,
        }
    }

    /// The part's field, given its capsule term `c` at the same point.
    #[inline]
    fn field(&self, l: V3, c: f32) -> f32 {
        match self {
            Part::Bone(b) => {
                let tau = l[0].clamp(b.t0, b.t1);
                let (a, bb) = b.sizes(tau);
                let m = a.min(bb);
                let e = (l[0] - tau) / m;
                let ns = gauge(b.gauge, l[1].abs() / a, l[2].abs() / bb);
                let n = (e * e + ns * ns).sqrt();
                let psi = m * (n.min(CLAMP) - 1.0);
                (psi / b.lip).max(c)
            }
            Part::Egg(e) => {
                let (x, y, z) = (l[0] * e.inv[0], l[1] * e.inv[1], l[2] * e.inv[2]);
                (e.least * ((x * x + y * y + z * z).sqrt() - 1.0)).max(c)
            }
        }
    }

    fn reach_of(&self) -> f32 {
        match self {
            Part::Bone(b) => b.radius,
            Part::Egg(e) => e.most,
        }
    }

    fn origin(&self) -> V3 {
        match self {
            Part::Bone(b) => b.origin,
            Part::Egg(e) => e.origin,
        }
    }
}

impl Shaft {
    /// The half breadth and half depth at `tau` metres along the bone (in `[t0, t1]`).
    #[inline]
    fn sizes(&self, tau: f32) -> (f32, f32) {
        let mut i = 0;
        while i + 2 < self.count && tau > self.stations[i + 1] {
            i += 1;
        }
        let p = &self.pieces[i];
        if self.count == 1 {
            return (p.a[0], p.q[0] * p.a[0]);
        }
        let f = ((tau - p.t0) / p.span).clamp(0.0, 1.0);
        let a = hermite(p.a, f);
        (a, hermite(p.q, f) * a)
    }
}

/// A Hermite piece `[y0, y1, Δ s0, Δ s1]` at the share `f` of its span.
#[inline]
fn hermite(c: [f32; 4], f: f32) -> f32 {
    let g = 1.0 - f;
    let h00 = (1.0 + 2.0 * f) * g * g;
    let h01 = f * f * (3.0 - 2.0 * f);
    let h10 = f * g * g;
    let h11 = -(f * f * g);
    h00 * c[0] + h01 * c[1] + h10 * c[2] + h11 * c[3]
}

/// `N_p(a, b)` for `a, b ≥ 0`.
#[inline]
fn gauge(g: Gauge, a: f32, b: f32) -> f32 {
    if g == Gauge::Two {
        return (a * a + b * b).sqrt();
    }
    let w = a.max(b);
    if w == 0.0 {
        return 0.0;
    }
    let r = a.min(b) / w;
    let s = match g {
        Gauge::Four => {
            let r2 = r * r;
            (1.0 + r2 * r2).sqrt().sqrt()
        }
        Gauge::Eight => {
            let r4 = r * r * (r * r);
            (1.0 + r4 * r4).sqrt().sqrt().sqrt()
        }
        Gauge::Power(p, ip) => dmath::pow(1.0 + dmath::pow(r, p), ip),
        Gauge::Two => unreachable!("handled above"),
    };
    w * s
}

/// The quadratic join of width `k` (DISTANCE_FIELDS.md section 5).
#[inline]
fn join(a: f32, b: f32, k: f32) -> f32 {
    let m = a.min(b);
    if k <= 0.0 {
        return m;
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    m - k * h * h * 0.25
}

/// 7.5's `G`: the values sorted from the least (stable, so ties keep the parts' order) and
/// joined in that order; `+∞` for none.
fn chain(values: &mut [f32], k: f32) -> f32 {
    for i in 1..values.len() {
        let v = values[i];
        let mut j = i;
        while j > 0 && values[j - 1] > v {
            values[j] = values[j - 1];
            j -= 1;
        }
        values[j] = v;
    }
    let Some((&first, rest)) = values.split_first() else { return f32::INFINITY };
    rest.iter().fold(first, |r, &y| join(r, y, k))
}

/// The least `f32` at or above `x`.
fn up(x: f64) -> f32 {
    let f = x as f32;
    if f64::from(f) < x {
        f.next_up()
    } else {
        f
    }
}

/// The greatest `f32` at or below `x`.
fn down(x: f64) -> f32 {
    let f = x as f32;
    if f64::from(f) > x {
        f.next_down()
    } else {
        f
    }
}

type D3 = [f64; 3];

fn d3(v: V3) -> D3 {
    [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])]
}
fn dsub(a: D3, b: D3) -> D3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn ddot(a: D3, b: D3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn dlen(a: D3) -> f64 {
    ddot(a, a).sqrt()
}
fn dscale(a: D3, s: f64) -> D3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dcross(a: D3, b: D3) -> D3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn f3(v: D3) -> V3 {
    [v[0] as f32, v[1] as f32, v[2] as f32]
}

/// A bone's frame in `f64`: its direction (+Y for a point), toward its side, and across.
fn frame_of(bone: &Bone) -> ([D3; 3], f64) {
    let along = dsub(d3(bone.tail), d3(bone.head));
    let length = dlen(along);
    let e = if length >= f64::from(LEAST_LENGTH) { dscale(along, 1.0 / length) } else { [0.0, 1.0, 0.0] };
    let fallback = if e[0].abs() > 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    let pick = |side: D3| {
        let s = dsub(side, dscale(e, ddot(side, e)));
        let l = dlen(s);
        (l > 1e-6 * dlen(side).max(1e-30)).then(|| dscale(s, 1.0 / l))
    };
    let s = bone.side.map(d3).and_then(pick).or_else(|| pick(fallback)).unwrap_or([1.0, 0.0, 0.0]);
    let w = dcross(e, s);
    ([e, s, w], if length >= f64::from(LEAST_LENGTH) { length } else { 0.0 })
}

/// The greatest of `|dP/df|` on `[0, 1]` for a Hermite piece, in `f64` from its stored numbers
/// (a quadratic: at an end or where it turns).
fn slope_most(c: [f32; 4]) -> f64 {
    let [y0, y1, m0, m1] = c.map(f64::from);
    let (c2, c1, c0) = (6.0 * y0 + 3.0 * m0 - 6.0 * y1 + 3.0 * m1, -6.0 * y0 - 4.0 * m0 + 6.0 * y1 - 2.0 * m1, m0);
    let p = |f: f64| c0 + c1 * f + c2 * f * f;
    let mut most = p(0.0).abs().max(p(1.0).abs());
    if c2 != 0.0 {
        let f = -c1 / (2.0 * c2);
        if f > 0.0 && f < 1.0 {
            most = most.max(p(f).abs());
        }
    }
    most
}

/// The least and the greatest of a Hermite piece on `[0, 1]`, in `f64` (at an end or where its
/// slope is nothing).
fn value_span(c: [f32; 4]) -> (f64, f64) {
    let [y0, y1, m0, m1] = c.map(f64::from);
    let at = |f: f64| {
        let g = 1.0 - f;
        (1.0 + 2.0 * f) * g * g * y0 + f * f * (3.0 - 2.0 * f) * y1 + f * g * g * m0 - f * f * g * m1
    };
    let (c2, c1, c0) = (6.0 * y0 + 3.0 * m0 - 6.0 * y1 + 3.0 * m1, -6.0 * y0 - 4.0 * m0 + 6.0 * y1 - 2.0 * m1, m0);
    let mut lo = y0.min(y1);
    let mut hi = y0.max(y1);
    let mut look = |f: f64| {
        if f > 0.0 && f < 1.0 {
            let v = at(f);
            lo = lo.min(v);
            hi = hi.max(v);
        }
    };
    if c2 != 0.0 {
        let disc = c1 * c1 - 4.0 * c2 * c0;
        if disc >= 0.0 {
            let r = disc.sqrt();
            look((-c1 + r) / (2.0 * c2));
            look((-c1 - r) / (2.0 * c2));
        }
    } else if c1 != 0.0 {
        look(-c0 / c1);
    }
    // A little to spare for the f64 roundings of the search itself.
    (lo - 1e-12 * lo.abs(), hi + 1e-12 * hi.abs())
}

/// The slopes of 7.1 at the stations `ts` (metres) of the values `ys`: 0 at both ends; between,
/// the harmonic mean of the two rises when they have one sign, else 0.
fn slopes(ts: &[f64], ys: &[f64]) -> Vec<f64> {
    let n = ts.len();
    let rise = |j: usize| (ys[j + 1] - ys[j]) / (ts[j + 1] - ts[j]);
    (0..n)
        .map(|j| {
            if j == 0 || j + 1 == n {
                return 0.0;
            }
            let (r0, r1) = (rise(j - 1), rise(j));
            if r0 * r1 > 0.0 {
                2.0 * r0 * r1 / (r0 + r1)
            } else {
                0.0
            }
        })
        .collect()
}

fn finite(v: V3) -> bool {
    v.iter().all(|c| c.is_finite())
}

fn in_extent(v: V3) -> bool {
    finite(v) && v.iter().all(|c| c.abs() <= EXTENT)
}

fn shaft(bone: &Bone, index: usize) -> Result<Shaft, CaroError> {
    let err = |code, measured: Option<f64>, limits| fail(code, Some(index), None, measured, limits);
    if !(in_extent(bone.head) && in_extent(bone.tail)) {
        return Err(err("bone_extent", None, Some((-EXTENT, EXTENT))));
    }
    let n = bone.sections.len();
    if n > MAX_STATIONS {
        return Err(err("bone_sections", Some(n as f64), Some((1.0, MAX_STATIONS as f32))));
    }
    if !(bone.square >= SQUARE.0 && bone.square <= SQUARE.1) {
        return Err(err("square", Some(f64::from(bone.square)), Some(SQUARE)));
    }
    let (frame, length) = frame_of(bone);
    for (j, s) in bone.sections.iter().enumerate() {
        if !(s[0] >= ALONG.0 && s[0] <= ALONG.1) {
            return Err(err("section_along", Some(f64::from(s[0])), Some(ALONG)));
        }
        for &h in &s[1..] {
            if !(h >= SIZE.0 && h <= SIZE.1) {
                return Err(err("section_size", Some(f64::from(h)), Some(SIZE)));
            }
        }
        if j > 0 && s[0] - bone.sections[j - 1][0] < LEAST_GAP {
            return Err(err("section_order", Some(f64::from(s[0] - bone.sections[j - 1][0])), Some((LEAST_GAP, ALONG.1 - ALONG.0))));
        }
    }
    if length == 0.0 && n > 1 {
        return Err(err("point_bone_sections", Some(n as f64), Some((1.0, 1.0))));
    }
    // The stations in metres, as stored: the profile is defined by these numbers.
    let stations: Vec<f32> = bone.sections.iter().map(|s| (f64::from(s[0]) * length) as f32).collect();
    if stations.windows(2).any(|w| w[1] <= w[0]) {
        return Err(err("section_order", None, None));
    }
    let ts: Vec<f64> = stations.iter().map(|&t| f64::from(t)).collect();
    let a: Vec<f64> = bone.sections.iter().map(|s| f64::from(s[1])).collect();
    let q: Vec<f64> = bone.sections.iter().map(|s| f64::from(s[2]) / f64::from(s[1])).collect();
    let (sa, sq) = (slopes(&ts, &a), slopes(&ts, &q));
    let mut pieces = [Piece::default(); MAX_STATIONS - 1];
    if n == 1 {
        let (a0, q0) = (a[0] as f32, q[0] as f32);
        pieces[0] = Piece { a: [a0, a0, 0.0, 0.0], q: [q0, q0, 0.0, 0.0], t0: stations[0], span: 1.0 };
    }
    for j in 0..n.saturating_sub(1) {
        let span = ts[j + 1] - ts[j];
        pieces[j] = Piece {
            a: [a[j] as f32, a[j + 1] as f32, (sa[j] * span) as f32, (sa[j + 1] * span) as f32],
            q: [q[j] as f32, q[j + 1] as f32, (sq[j] * span) as f32, (sq[j + 1] * span) as f32],
            t0: stations[j],
            span: (stations[j + 1] - stations[j]),
        };
    }
    // 7.1's g and k, and 7.3's R, from the stored numbers, piece by piece.
    let (mut g, mut k, mut radius) = (0.0f64, 0.0f64, 0.0f64);
    let square = bone.square;
    for (j, p) in pieces[..n.saturating_sub(1).max(1)].iter().enumerate() {
        let (alo, ahi) = value_span(p.a);
        let (qlo, qhi) = value_span(p.q);
        if !(alo > 0.0 && qlo > 0.0) {
            return Err(err("profile_not_positive", Some(alo.min(qlo)), None));
        }
        let span = if n == 1 { 1.0 } else { ts[j + 1] - ts[j] };
        let (da, dq) = if n == 1 { (0.0, 0.0) } else { (slope_most(p.a) / span, slope_most(p.q) / span) };
        let db = dq * ahi + qhi * da;
        let gj = if qlo >= 1.0 {
            da
        } else if qhi <= 1.0 {
            db
        } else {
            da.max(db)
        };
        g = g.max(gj);
        k = k.max(ahi * dq);
        let r = if square == 2.0 { ahi * qhi.max(1.0) } else { ahi * (1.0 + qhi * qhi).sqrt() };
        radius = radius.max(r);
    }
    let lip = (1.0 + (g + 2.0 * k) * (g + 2.0 * k)).sqrt() * (1.0 + 1e-6);
    let gauge = if square == 2.0 {
        Gauge::Two
    } else if square == 4.0 {
        Gauge::Four
    } else if square == 8.0 {
        Gauge::Eight
    } else {
        Gauge::Power(square, 1.0 / square)
    };
    let mut st = [0.0f32; MAX_STATIONS];
    st[..n.max(1)].copy_from_slice(&stations[..n.max(1)]);
    Ok(Shaft {
        origin: bone.head,
        frame: frame.map(f3),
        t0: stations[0],
        t1: stations[n - 1],
        stations: st,
        count: n,
        pieces,
        gauge,
        lip: up(lip),
        radius: up(radius * (1.0 + 1e-6)),
    })
}

fn egg(bones: &[Bone], swell: &Swell, index: usize) -> Result<Egg, CaroError> {
    let err = |code, measured: Option<f64>, limits| fail(code, Some(swell.bone), Some(index), measured, limits);
    let Some(bone) = bones.get(swell.bone) else {
        return Err(fail("swell_bone", None, Some(index), Some(swell.bone as f64), Some((0.0, bones.len() as f32 - 1.0))));
    };
    if !(in_extent(bone.head) && in_extent(bone.tail)) {
        return Err(err("bone_extent", None, Some((-EXTENT, EXTENT))));
    }
    if !(swell.at[0] >= ALONG.0 && swell.at[0] <= ALONG.1) {
        return Err(err("swell_along", Some(f64::from(swell.at[0])), Some(ALONG)));
    }
    for &o in &swell.at[1..] {
        if o.is_nan() || o.abs() > OFFSET {
            return Err(err("swell_offset", Some(f64::from(o)), Some((-OFFSET, OFFSET))));
        }
    }
    for &r in &swell.radii {
        if !(r >= SIZE.0 && r <= SIZE.1) {
            return Err(err("swell_size", Some(f64::from(r)), Some(SIZE)));
        }
    }
    let (frame, length) = frame_of(bone);
    let at = d3(swell.at);
    let mut o = d3(bone.head);
    for (c, axis) in [at[0] * length, at[1], at[2]].into_iter().zip(frame) {
        o = [o[0] + c * axis[0], o[1] + c * axis[1], o[2] + c * axis[2]];
    }
    let inv = swell.radii.map(|r| 1.0 / r);
    // The ellipsoid evaluated is the one of half axes 1 / inv: least at most its least, most
    // at least its greatest (7.4).
    let axes = inv.map(|i| 1.0 / f64::from(i));
    let least = down(axes[0].min(axes[1]).min(axes[2]));
    let most = up(axes[0].max(axes[1]).max(axes[2]) * (1.0 + 1e-6));
    Ok(Egg { origin: f3(o), frame: frame.map(f3), inv, least, most })
}

/// A body's flesh: its bones' and swells' fields joined (DISTANCE_FIELDS.md 7.5), culled
/// exactly (7.6).
#[derive(Clone, Debug)]
pub struct Flesh {
    parts: Vec<Part>,
    of: Vec<PartOf>,
    smooth: f32,
    keep: f32,
    reach: f32,
    scale: f32,
}

impl Flesh {
    /// The flesh of `bones` (those with sections) and `swells`, joined with width `smooth`,
    /// culled exactly below `keep` (7.6's `T`).
    pub fn new(bones: &[Bone], swells: &[Swell], smooth: f32, keep: f32) -> Result<Flesh, CaroError> {
        if !(smooth >= SMOOTH.0 && smooth <= SMOOTH.1) {
            return Err(fail("smooth", None, None, Some(f64::from(smooth)), Some(SMOOTH)));
        }
        if !(keep >= KEEP.0 && keep <= KEEP.1) {
            return Err(fail("keep", None, None, Some(f64::from(keep)), Some(KEEP)));
        }
        if swells.len() > MAX_SWELLS {
            return Err(fail("too_many_swells", None, None, Some(swells.len() as f64), Some((0.0, MAX_SWELLS as f32))));
        }
        let fleshed = bones.iter().filter(|b| !b.sections.is_empty()).count();
        if fleshed + swells.len() > MAX_PARTS {
            return Err(fail("too_many_parts", None, None, Some((fleshed + swells.len()) as f64), Some((1.0, MAX_PARTS as f32))));
        }
        let mut parts = Vec::new();
        let mut of = Vec::new();
        for (i, b) in bones.iter().enumerate() {
            if !b.sections.is_empty() {
                parts.push(Part::Bone(shaft(b, i)?));
                of.push(PartOf::Bone(i));
            }
        }
        for (i, s) in swells.iter().enumerate() {
            parts.push(Part::Egg(egg(bones, s, i)?));
            of.push(PartOf::Swell(i));
        }
        if parts.is_empty() {
            return Err(fail("no_flesh", None, None, None, None));
        }
        // 7.6 in f32: reach at least (keep + 2 smooth + 2⁻¹⁴ smooth) / (1 − 2⁻¹⁴).
        let (t, k) = (f64::from(keep), f64::from(smooth));
        let slack = f64::from(SLACK);
        let reach = up((t + 2.0 * k + slack * k) / (1.0 - slack));
        let size = |v: V3| v[0].abs() + v[1].abs() + v[2].abs();
        let widest = parts.iter().map(|p| f64::from(size(p.origin())) + f64::from(p.reach_of())).fold(0.0f64, f64::max);
        let scale = up(widest + 9.0 * k + t);
        Ok(Flesh { parts, of, smooth, keep, reach, scale })
    }

    /// The body's field at `x`: only the parts whose capsule widened by [`Flesh::reach`] holds
    /// `x` are asked, and the value is [`Flesh::whole`]'s (7.6).
    pub fn value(&self, x: V3) -> f32 {
        let mut values = [0.0f32; MAX_PARTS];
        let mut n = 0;
        for p in &self.parts {
            let l = p.local(x);
            let c = p.capsule(l);
            if c <= self.reach {
                values[n] = p.field(l, c);
                n += 1;
            }
        }
        chain(&mut values[..n], self.smooth).min(self.keep)
    }

    /// [`Flesh::value`] asking only the parts in `among` (rising, as [`Flesh::meets`] gives
    /// them): the same number wherever `among` holds every part [`Flesh::value`] would ask.
    pub fn value_among(&self, x: V3, among: &[u16]) -> f32 {
        let mut values = [0.0f32; MAX_PARTS];
        let mut n = 0;
        for &i in among.iter().take(MAX_PARTS) {
            let Some(p) = self.parts.get(usize::from(i)) else { continue };
            let l = p.local(x);
            let c = p.capsule(l);
            if c <= self.reach {
                values[n] = p.field(l, c);
                n += 1;
            }
        }
        chain(&mut values[..n], self.smooth).min(self.keep)
    }

    /// The body's field asking every part: 7.5's `H`, what [`Flesh::value`] is defined to equal.
    pub fn whole(&self, x: V3) -> f32 {
        let mut values = [0.0f32; MAX_PARTS];
        for (v, p) in values.iter_mut().zip(&self.parts) {
            let l = p.local(x);
            *v = p.field(l, p.capsule(l));
        }
        chain(&mut values[..self.parts.len()], self.smooth).min(self.keep)
    }

    /// The parts any point of the ball of radius `r` about `o` can ask, rising: every part
    /// [`Flesh::value`] asks at such a point is among them.
    pub fn meets(&self, o: V3, r: f32, out: &mut Vec<u16>) {
        out.clear();
        let limit = self.reach + r + 2.0 * self.slack_ball(o, r);
        for (i, p) in self.parts.iter().enumerate() {
            if p.capsule(p.local(o)) <= limit {
                out.push(i as u16);
            }
        }
    }

    /// Bounds of [`Flesh::value`] over the ball of radius `r` about `o` (7.6, "Over a ball"):
    /// every value it takes there, in `f32`, lies between them.
    pub fn range(&self, o: V3, r: f32) -> (f32, f32) {
        let h = self.value(o);
        let s = self.slack_ball(o, r);
        let hi = (h + r + 2.0 * s).min(self.keep);
        let least = self.parts.iter().map(|p| p.capsule(p.local(o))).fold(f32::INFINITY, f32::min);
        let far = (least - r - self.smooth - s).min(self.keep) - s;
        ((h - r - 2.0 * s).max(far), hi)
    }

    /// How far a value at `x` may be from the exact field: `2⁻¹⁴ (|x|₁ + W)`.
    pub fn slack(&self, x: V3) -> f32 {
        SLACK * (x[0].abs() + x[1].abs() + x[2].abs() + self.scale)
    }

    /// [`Flesh::slack`] at the farthest point of a ball.
    fn slack_ball(&self, o: V3, r: f32) -> f32 {
        SLACK * (o[0].abs() + o[1].abs() + o[2].abs() + 1.732_051 * r + self.scale)
    }

    /// How many parts the body has.
    pub fn parts(&self) -> usize {
        self.parts.len()
    }

    /// Which bone or swell part `i` is.
    pub fn part_of(&self, i: usize) -> PartOf {
        self.of[i]
    }

    /// Part `i`'s own field at `x` (7.3, 7.4): 1-Lipschitz, a bound of its flesh.
    pub fn part_field(&self, i: usize, x: V3) -> f32 {
        let p = &self.parts[i];
        let l = p.local(x);
        p.field(l, p.capsule(l))
    }

    /// Part `i`'s capsule term at `x` (7.6's `c_i`): at most its field.
    pub fn part_capsule(&self, i: usize, x: V3) -> f32 {
        let p = &self.parts[i];
        p.capsule(p.local(x))
    }

    /// Part `i`'s `L` of 7.3 (1 for a swell): how fast its `ψ` may change, which its field is
    /// divided by.
    pub fn part_lip(&self, i: usize) -> f32 {
        match &self.parts[i] {
            Part::Bone(b) => b.lip,
            Part::Egg(_) => 1.0,
        }
    }

    /// The width a part's capsule is widened by for the cull (7.6's `ε`).
    pub fn reach(&self) -> f32 {
        self.reach
    }

    /// The level up to which the culled field is the whole field (7.6's `T`).
    pub fn keep(&self) -> f32 {
        self.keep
    }

    /// The smoothing width (7.5's `K`).
    pub fn smooth(&self) -> f32 {
        self.smooth
    }

    /// `W` of [`Flesh::slack`].
    pub fn scale(&self) -> f32 {
        self.scale
    }
}

#[cfg(test)]
mod tests;

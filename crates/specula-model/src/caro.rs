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
//! - **The body** ([`Flesh`]) joins its parts' values by a join of any number of values that
//!   does not depend on their order (7.9: for two values the quadratic join of width `smooth`),
//!   and holds the result at most `keep` (7.5). [`Flesh::value`] joins only the parts whose
//!   capsule, widened by [`Flesh::reach`] (`keep + 3 smooth / 2`, and a little for rounding),
//!   holds the point, and is the same number as [`Flesh::whole`], which joins them all (7.6,
//!   7.9).
//!
//! - **A joint** ([`Joint`], found by [`joints_of`]) is two bones meeting end to end; with
//!   [`Flesh::jointed`] the join's width falls from `smooth` to nothing as the two run straight
//!   on, near the joint only, changing by at most [`WIDTH_SLOPE`] a metre, and the joined value
//!   is divided by `1 + WIDTH_SLOPE / 2` so that it stays 1-Lipschitz (7.8, 7.9): no ring at a
//!   spine's or a tail's joints, the body's width at a bend.
//!
//! The sculpt's `body` node ([`crate::sdf::Prim::Body`]) asks [`Flesh::value_among`] with the
//! list a block got from [`Flesh::meets`], and [`Flesh::range`] over a ball.
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
//! - *the join of up to 64 values* (7.9), a level: the least exact, each difference to it one
//!   rounding of a number under `smooth`, `λ` (a sum of up to 64 of them, over their count)
//!   within `64 u smooth`, the squares' sum within `32 u smooth` of its share: under
//!   `u (100 smooth + |value|)` a level, two levels, with `|value| ≤ keep + smooth + R`;
//! - *7.8's width*: a seam's distance in a few roundings of numbers under `|d| + ball`, so the
//!   width is within `10 u (|x|₁ + W)` of the exact one, and the joined value moves by at most
//!   half as much (7.9 (2)); the division by `1 + Λ/2` one rounding.
//!
//! That is under `400 u` of `|x|₁ + W` with `W` = the greatest `|a|₁ + R` of the parts plus
//! `9 smooth + keep`; `2⁻¹⁴` is `1024 u`. The numbers `L`, `R` and a swell's radii are worked
//! out in `f64` from the numbers stored and rounded the safe way, so each holds for the profile
//! this module evaluates; the tests hold `σ` against a reference in `f64`.
//!
//! **Determinism (F2):** basic operations, `sqrt`, and [`crate::dmath::pow`] (correctly
//! rounded) only; no hash maps; a join sorts the values it is given, so it is one number in
//! whatever order they come.
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
    /// 7.3's `R`: at least every section's farthest point from the axis, and every cap's.
    radius: f32,
    /// 7.3's `σ` of the tilted cap at the head and at the tail (0 for a level end), and
    /// `1 + σ²` as stored.
    cap: [f32; 2],
    cap_one: [f32; 2],
    /// The least half size anywhere along the bone (at most every station's `m`).
    least: f32,
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
                // Past an end its cap, tilted by the end's `σ` (7.3; `e` is 0 between the ends,
                // and a level end's numbers, 1 and 0, change no bit).
                let end = usize::from(l[0] > b.t1);
                let n = (b.cap_one[end] * e * e + ns * ns).sqrt() - b.cap[end] * e.abs();
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

/// 7.5's join of any number of values at width `k` (DISTANCE_FIELDS.md 7.9): with `m` the
/// least and `d_i = v_i − m` for the values within `k` of it, sorted, `λ` the level at which
/// the shares `(λ − d_i) ∨ 0` add to `k`, it is `m + λ − k/2 − Σ (λ − d_i)² / (2k)`. A
/// function of the values alone, not of their order; for two values the quadratic join of
/// section 5; a value `k` or more above the least changes nothing. `+∞` for none.
fn blend(values: &[f32], k: f32) -> f32 {
    let Some(m) = values.iter().copied().reduce(f32::min) else { return f32::INFINITY };
    if k <= 0.0 {
        return m;
    }
    // The values within the width of the least (a difference of `k` or more rounds to at
    // least `k`, so a value past it is never seen), sorted: the least first, as 0.
    let mut d = [0.0f32; MAX_PARTS];
    let mut n = 0;
    for &v in values.iter().take(MAX_PARTS) {
        let e = v - m;
        if e < k {
            let mut j = n;
            while j > 0 && d[j - 1] > e {
                d[j] = d[j - 1];
                j -= 1;
            }
            d[j] = e;
            n += 1;
        }
    }
    // One value within the width is the least itself (so a value left out never makes a
    // difference of a rounding: the cull is bit for bit).
    if n == 1 {
        return m;
    }
    // The shares' support is the first `s` of them (7.9, the support).
    let (mut sum, mut lambda, mut s) = (0.0f32, k, 1usize);
    for (i, &e) in d[..n].iter().enumerate().skip(1) {
        let next = sum + e;
        let l = (k + next) / (i + 1) as f32;
        if e < l {
            (sum, lambda, s) = (next, l, i + 1);
        } else {
            break;
        }
    }
    let mut sq = 0.0f32;
    for &e in &d[..s] {
        let w = lambda - e;
        sq += w * w;
    }
    m + ((lambda - 0.5 * k) - sq / (2.0 * k))
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

/// A bone's frame as its field takes it: its direction (+Y for a point), toward its side, and
/// across.
pub fn bone_frame(bone: &Bone) -> [V3; 3] {
    frame_of(bone).0.map(f3)
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

fn shaft(bone: &Bone, index: usize, ends: [f64; 2]) -> Result<Shaft, CaroError> {
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
    // 7.1: a station at a joint that runs on takes the slope it is given, held to `[0, 2δ]` of
    // its own piece's rise `δ`, so that the piece still runs one way.
    let mut sa = sa;
    if n >= 2 {
        for (j, near, given) in [(0, 1, ends[0]), (n - 1, n - 2, ends[1])] {
            let (lo, hi) = (j.min(near), j.max(near));
            let d = (a[hi] - a[lo]) / (ts[hi] - ts[lo]);
            sa[j] = if given * d > 0.0 { given.signum() * given.abs().min(2.0 * d.abs()) } else { 0.0 };
        }
    }
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
    let (mut g, mut k, mut radius, mut least) = (0.0f64, 0.0f64, 0.0f64, f64::INFINITY);
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
        least = least.min(alo.min(alo * qlo));
        k = k.max(ahi * dq);
        let r = if square == 2.0 { ahi * qhi.max(1.0) } else { ahi * (1.0 + qhi * qhi).sqrt() };
        radius = radius.max(r);
    }
    // 7.3's tilted caps: `σ = m A′ / A` outward at each end (`q`'s end slope is 0), and what
    // each asks of `L` and `R`, from the numbers stored.
    let mut lip = (1.0 + (g + 2.0 * k) * (g + 2.0 * k)).sqrt() * (1.0 + 1e-6);
    let (mut cap, mut cap_one) = ([0.0f32; 2], [1.0f32; 2]);
    if n >= 2 {
        for (s, (j, out)) in [(0, -sa[0]), (n - 1, sa[n - 1])].into_iter().enumerate() {
            cap[s] = (out * q[j].min(1.0)) as f32;
            let sigma = f64::from(cap[s]);
            cap_one[s] = (1.0 + sigma * sigma) as f32;
            let root = f64::from(cap_one[s]).sqrt();
            lip = lip.max((root + (-sigma).max(0.0)) * (1.0 + 1e-6));
            let plus = sigma.max(0.0);
            let kappa = plus + (1.0 + plus * plus).sqrt();
            let (aj, qj) = (f64::from(a[j] as f32), f64::from(q[j] as f32));
            let reach = if square == 2.0 { aj * qj.max(1.0) } else { aj * (1.0 + qj * qj).sqrt() };
            radius = radius.max(reach * kappa);
        }
    }
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
        cap,
        cap_one,
        least: down(least),
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

/// Which end of a bone meets a joint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Head,
    Tail,
}

/// Two bones that meet end to end (DISTANCE_FIELDS.md 7.8): bone `bones[0]` at its end
/// `ends[0]` and bone `bones[1]` at `ends[1]`. `smooth` is the joint's own width where its body
/// says so (at most the body's, and at most the smaller reach of the two bones); else the width
/// falls from the body's as the two bones come into line ([`bend_share`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    pub bones: [usize; 2],
    pub ends: [End; 2],
    pub smooth: Option<f32>,
}

/// How fast the width may change a metre (7.8's `Λ`): the joined field is divided by `1 + Λ/2`
/// wherever a joint's width is under the body's.
pub const WIDTH_SLOPE: f32 = 0.5;
/// The bend (degrees) from which a joint has the body's whole width; under it the width falls
/// smoothly to nothing at a straight joint ([`bend_share`]).
pub const FULL_BEND: f64 = 40.0;
/// `1 − cos FULL_BEND`.
const FULL_BEND_ONE_LESS_COS: f64 = 1.0 - 0.766_044_443_118_978;

/// The share of the body's width a joint takes when its two bones run on from one another at
/// `cos_bend` (the cosine of the angle `θ` between the one's direction into the joint and the
/// other's out of it: 1 straight on): `s(u)`, `u = θ / 40°` held to `[0, 1]`, `s` the smooth
/// step `u² (3 − 2u)`. 0 straight on, 1 from 40 degrees of bend. (`u` grows with the angle
/// itself: a share that grew with `1 − cos θ` left a joint bent by a few degrees nearly no
/// width, and the union's crease of that angle round it.)
pub fn bend_share(cos_bend: f64) -> f64 {
    let u = (dmath::acos_f64(cos_bend.clamp(-1.0, 1.0)) / (FULL_BEND * dmath::PI / 180.0)).clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// How far apart in size two bones' end sections may be (the greater difference of their half
/// breadths and half depths, over the greater) before a joint between them takes the body's
/// whole width: the engine's own estimate (2026-11-28), not a measured figure.
pub const FULL_STEP: f64 = 0.1;

/// The share of the body's width a joint takes for the step in size between its two bones'
/// sections at the joint (`[A, B]` of each): `s(u)`, `u` the step over [`FULL_STEP`] held to
/// `[0, 1]`. Two bones of one size running straight on need no width (7.8: no ring); a step
/// between them needs one, or the union leaves a crease round it.
pub fn step_share(a: [f32; 2], b: [f32; 2]) -> f64 {
    let step = (0..2).filter(|&i| a[i].max(b[i]) > 0.0).map(|i| (f64::from(a[i]) - f64::from(b[i])).abs() / f64::from(a[i].max(b[i]))).fold(0.0f64, f64::max);
    let u = (step / FULL_STEP).clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// 7.1's slopes at the joints that run on: for each bone, the slope of its half breadth `A`
/// (a metre along it) at its first and its last station, 0 where its end is level. A joint
/// runs on where its two bones are bent by under [`FULL_BEND`], their end stations are at the
/// joint, their end sections differ by at most a quarter in each half size, and neither end is
/// in another such joint; its slope is the harmonic mean of the rises of the two pieces beside
/// it along the run (0 where they differ in sign), which both bones take.
fn end_slopes(bones: &[Bone], joints: &[Joint]) -> Vec<[f64; 2]> {
    let mut out = vec![[0.0f64; 2]; bones.len()];
    let side = |e: End| usize::from(e == End::Tail);
    // The rise of `A` a metre from the joint into bone `b` at its end `e`, where its end
    // station is at the joint.
    let rise = |b: &Bone, e: End| -> Option<f64> {
        let s = &b.sections;
        let length = frame_of(b).1;
        if s.len() < 2 || length == 0.0 {
            return None;
        }
        let (end, next) = match e {
            End::Head => (s[0], s[1]),
            End::Tail => (s[s.len() - 1], s[s.len() - 2]),
        };
        let at = if e == End::Head { 0.0 } else { 1.0 };
        (end[0] == at).then(|| (f64::from(next[1]) - f64::from(end[1])) / ((f64::from(next[0]) - f64::from(end[0])).abs() * length))
    };
    let outward = |b: &Bone, e: End| {
        let d = frame_of(b).0[0];
        if e == End::Head { d } else { dscale(d, -1.0) }
    };
    let cos_full = 1.0 - FULL_BEND_ONE_LESS_COS;
    let runs: Vec<&Joint> = joints
        .iter()
        .filter(|j| {
            let (Some(a), Some(b)) = (bones.get(j.bones[0]), bones.get(j.bones[1])) else { return false };
            if a.sections.is_empty() || b.sections.is_empty() || frame_of(a).1 == 0.0 || frame_of(b).1 == 0.0 {
                return false;
            }
            -ddot(outward(a, j.ends[0]), outward(b, j.ends[1])) > cos_full
        })
        .collect();
    let mut count = vec![[0u8; 2]; bones.len()];
    for j in &runs {
        for s in 0..2 {
            count[j.bones[s]][side(j.ends[s])] += 1;
        }
    }
    for j in runs {
        let ([b0, b1], [e0, e1]) = (j.bones, j.ends);
        if count[b0][side(e0)] != 1 || count[b1][side(e1)] != 1 {
            continue;
        }
        let near = |b: &Bone, e: End| if e == End::Head { b.sections[0] } else { b.sections[b.sections.len() - 1] };
        let (n0, n1) = (near(&bones[b0], e0), near(&bones[b1], e1));
        if (1..3).any(|i| (n0[i] - n1[i]).abs() > 0.25 * n0[i].max(n1[i])) {
            continue;
        }
        let (Some(r0), Some(r1)) = (rise(&bones[b0], e0), rise(&bones[b1], e1)) else { continue };
        // Along the run, from the first bone through the joint into the second.
        let (p0, p1) = (-r0, r1);
        if p0 * p1 <= 0.0 {
            continue;
        }
        let run = 2.0 * p0 * p1 / (p0 + p1);
        // Outward of the first bone the slope is `run`, outward of the second `−run`; a head's
        // outward is back along the bone.
        let along = |e: End, outward: f64| if e == End::Tail { outward } else { -outward };
        out[b0][side(e0)] = along(e0, run);
        out[b1][side(e1)] = along(e1, -run);
    }
    out
}

/// The joints of a skeleton (`parents[i]` the parent of bone `i`): a bone and its parent where
/// the child's head is at the parent's tail (or head), and two children of one parent whose
/// heads are at one place; "at" is within a quarter of the smaller reach of the two. Bones
/// without flesh, and bones shorter than [`LEAST_LENGTH`], meet nothing.
pub fn joints_of(bones: &[Bone], parents: &[Option<usize>]) -> Vec<Joint> {
    let reach = |b: &Bone| b.sections.iter().map(|s| s[1].max(s[2])).fold(0.0f32, f32::max);
    let long = |b: &Bone| !b.sections.is_empty() && frame_of(b).1 > 0.0;
    let near = |p: V3, q: V3, tol: f32| {
        let d = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= tol
    };
    let mut out = Vec::new();
    for (i, b) in bones.iter().enumerate() {
        let Some(Some(p)) = parents.get(i) else { continue };
        let Some(pb) = bones.get(*p) else { continue };
        if !(long(b) && long(pb)) {
            continue;
        }
        let tol = 0.25 * reach(b).min(reach(pb));
        if near(b.head, pb.tail, tol) {
            out.push(Joint { bones: [*p, i], ends: [End::Tail, End::Head], smooth: None });
        } else if near(b.head, pb.head, tol) {
            out.push(Joint { bones: [*p, i], ends: [End::Head, End::Head], smooth: None });
        }
    }
    for (i, b) in bones.iter().enumerate() {
        for (k, c) in bones.iter().enumerate().skip(i + 1) {
            let (Some(pi), Some(pk)) = (parents.get(i).copied().flatten(), parents.get(k).copied().flatten()) else { continue };
            if pi != pk || !(long(b) && long(c)) {
                continue;
            }
            if near(b.head, c.head, 0.25 * reach(b).min(reach(c))) {
                out.push(Joint { bones: [i, k], ends: [End::Head, End::Head], smooth: None });
            }
        }
    }
    out
}

/// Where a joint's width is less than the body's (7.8): a disc about the joint, square to the
/// line the two bones run on, of radius `radius` and half thickness `half`; the width there is
/// `width`, and `width + Λ d` at a distance `d` from it. `ball` holds every point where that is
/// under the body's width, with room to spare for rounding.
#[derive(Clone, Copy, Debug)]
struct Seam {
    centre: V3,
    normal: V3,
    half: f32,
    radius: f32,
    width: f32,
    ball: f32,
    /// The distance over which the width starts to rise (the body's smoothing width).
    soft: f32,
    /// The parts of the joint's two bones, and the group they are in (7.8: the bones of the
    /// joints that run on are one group, and the seam's width is that group's alone).
    pair: [u8; 2],
    group: u8,
}

impl Seam {
    /// The seam's width at `x`, or `None` where it is the body's or more.
    #[inline]
    fn at(&self, x: V3) -> Option<f32> {
        let d = [x[0] - self.centre[0], x[1] - self.centre[1], x[2] - self.centre[2]];
        let r2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        if r2 > self.ball * self.ball {
            return None;
        }
        let z = d[0] * self.normal[0] + d[1] * self.normal[1] + d[2] * self.normal[2];
        let rho = (r2 - z * z).max(0.0).sqrt();
        let (dz, dr) = ((z.abs() - self.half).max(0.0), (rho - self.radius).max(0.0));
        let d = (dz * dz + dr * dr).sqrt();
        // `φ(d)`: level at the seam, then the distance less half the softening (7.8: so that
        // the width has a continuous slope where it starts to rise).
        let soft = if d < self.soft { d * d / (2.0 * self.soft) } else { d - 0.5 * self.soft };
        Some(self.width + WIDTH_SLOPE * soft)
    }
}

/// Most joints in one body.
pub const MAX_SEAMS: usize = 128;

/// The seams of `joints` (7.8): of each whose width is under the body's `smooth`, the disc
/// about it and its ball, worked out in `f64` and rounded the safe way.
fn make_seams(bones: &[Bone], parts: &[Part], of: &[PartOf], joints: &[Joint], smooth: f32) -> Result<Vec<Seam>, CaroError> {
    if joints.len() > MAX_SEAMS {
        return Err(fail("too_many_joints", None, None, Some(joints.len() as f64), Some((0.0, MAX_SEAMS as f32))));
    }
    let mut out = Vec::new();
    let k = f64::from(smooth);
    for j in joints {
        let mut reach = 0.0f64;
        let mut ends = [[0.0f64; 3]; 2];
        let mut outward = [[0.0f64; 3]; 2];
        let mut pair = [0u8; 2];
        let mut size = [[0.0f32; 2]; 2];
        for s in 0..2 {
            let b = j.bones[s];
            let Some(bone) = bones.get(b) else {
                return Err(fail("joint_bone", Some(b), None, Some(b as f64), Some((0.0, bones.len() as f32 - 1.0))));
            };
            let at = of.iter().position(|o| *o == PartOf::Bone(b));
            let Some(Part::Bone(shaft)) = at.map(|p| &parts[p]) else {
                return Err(fail("joint_flesh", Some(b), None, None, None));
            };
            pair[s] = at.map_or(0, |p| p as u8);
            let (frame, length) = frame_of(bone);
            if length == 0.0 {
                return Err(fail("joint_point", Some(b), None, None, None));
            }
            reach = reach.max(f64::from(shaft.radius));
            // The section at the station nearest the joint.
            let near = match j.ends[s] {
                End::Head => bone.sections.first(),
                End::Tail => bone.sections.last(),
            };
            size[s] = near.map_or([0.0; 2], |n| [n[1], n[2]]);
            (ends[s], outward[s]) = match j.ends[s] {
                End::Head => (d3(bone.head), frame[0]),
                End::Tail => (d3(bone.tail), dscale(frame[0], -1.0)),
            };
        }
        let own = match j.smooth {
            Some(w) => {
                let most = k.min(reach);
                if !(f64::from(w) >= 0.0 && f64::from(w) <= most) {
                    return Err(fail("joint_smooth", Some(j.bones[1]), None, Some(f64::from(w)), Some((0.0, most as f32))));
                }
                f64::from(w)
            }
            // It falls as the two run on from one another (their outward directions opposite),
            // and rises again with a step in size between them at the joint (7.8).
            None => k * bend_share(-ddot(outward[0], outward[1])).max(step_share(size[0], size[1])),
        };
        if own >= k {
            continue;
        }
        let centre = dscale([ends[0][0] + ends[1][0], ends[0][1] + ends[1][1], ends[0][2] + ends[1][2]], 0.5);
        let line = dsub(outward[1], outward[0]);
        let normal = if dlen(line) > 1e-3 { dscale(line, 1.0 / dlen(line)) } else { outward[1] };
        let gap = dlen(dsub(ends[0], ends[1]));
        let radius = reach + k + gap;
        let half = f64::from(WIDTH_SLOPE) * reach + gap;
        // Past the ball every point's width is the body's: there the seam's is at least `K`
        // and a half again, out of its softened join with `K` (7.8: `φ` loses half the softening).
        let ball = ((half * half + radius * radius).sqrt() + (1.5 * k - own) / f64::from(WIDTH_SLOPE) + 0.5 * k) * (1.0 + 1e-3) + 1e-6;
        out.push(Seam { centre: f3(centre), normal: f3(normal), half: up(half), radius: up(radius), width: down(own), ball: up(ball), soft: smooth, pair, group: 0 });
    }
    Ok(out)
}

/// 7.8's groups: the parts joined through seams; each part in the group of the least part it
/// is joined to so (a part in no seam, its own), and each seam told its group.
fn make_groups(parts: usize, seams: &mut [Seam]) -> Vec<u8> {
    let mut group: Vec<u8> = (0..parts).map(|i| i as u8).collect();
    loop {
        let mut moved = false;
        for s in seams.iter() {
            let [a, b] = s.pair.map(usize::from);
            let least = group[a].min(group[b]);
            if group[a] != least || group[b] != least {
                let (ga, gb) = (group[a], group[b]);
                group.iter_mut().filter(|g| **g == ga || **g == gb).for_each(|g| *g = least);
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    for s in seams.iter_mut() {
        s.group = group[usize::from(s.pair[0])];
    }
    group
}

/// 7.6's `ε` in `f32` for the held level `t` and the width `k`: at least
/// `(t + 3k/2 + 2⁻¹⁴ k) / (1 − 2⁻¹⁴)` where the parts are joined at one level, and
/// `(t + 7k/2 + 2⁻¹³ k) / (1 − 2⁻¹³)` where groups are joined first (7.9, the cull).
fn cull_reach(t: f64, k: f64, grouped: bool) -> f32 {
    let slack = f64::from(SLACK);
    if grouped {
        up((t + 3.5 * k + 2.0 * slack * k) / (1.0 - 2.0 * slack))
    } else {
        up((t + 1.5 * k + slack * k) / (1.0 - slack))
    }
}

/// A body's flesh: its bones' and swells' fields joined (DISTANCE_FIELDS.md 7.5), culled
/// exactly (7.6), with a width that falls at straight joints (7.8).
#[derive(Clone, Debug)]
pub struct Flesh {
    parts: Vec<Part>,
    of: Vec<PartOf>,
    smooth: f32,
    keep: f32,
    reach: f32,
    scale: f32,
    seams: Vec<Seam>,
    /// Each part's group (7.8): its own index where no seam joins it.
    group: Vec<u8>,
    /// 7.9's `1 + Λ/2` where any seam narrows the width, else 1: what the joined value is
    /// divided by.
    div: f32,
    /// How far the join can fall below the least value: `K/2` at one level, `K` where groups
    /// are joined first (7.9).
    fall: f32,
}

impl Flesh {
    /// The flesh of `bones` (those with sections) and `swells`, joined with width `smooth`,
    /// culled exactly below `keep` (7.6's `T`).
    pub fn new(bones: &[Bone], swells: &[Swell], smooth: f32, keep: f32) -> Result<Flesh, CaroError> {
        Flesh::jointed(bones, swells, smooth, keep, &[])
    }

    /// [`Flesh::new`], with the width at each of `joints` falling as its two bones come into
    /// line (7.8; [`joints_of`] finds a skeleton's).
    pub fn jointed(bones: &[Bone], swells: &[Swell], smooth: f32, keep: f32, joints: &[Joint]) -> Result<Flesh, CaroError> {
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
        let slopes = end_slopes(bones, joints);
        let mut parts = Vec::new();
        let mut of = Vec::new();
        for (i, b) in bones.iter().enumerate() {
            if !b.sections.is_empty() {
                parts.push(Part::Bone(shaft(b, i, slopes[i])?));
                of.push(PartOf::Bone(i));
            }
        }
        for (i, s) in swells.iter().enumerate() {
            parts.push(Part::Egg(egg(bones, s, i)?));
            of.push(PartOf::Swell(i));
        }
        // Each part's reach widened so that within `2K` of its surface, outside, its field is
        // its own and not the capsule's (7.9, "Where the capsule takes over"): the capsule's
        // break of slope then lies beyond where any join acts; at most `4K` wider, so that a
        // very flat part does not reach far (it then keeps its own field to a value of
        // `4K / (L R / m_min − 1)`).
        let k2 = 2.0 * f64::from(smooth);
        for p in &mut parts {
            match p {
                Part::Bone(b) => {
                    let spread = f64::from(b.lip) * f64::from(b.radius) / f64::from(b.least) - 1.0;
                    b.radius = up((f64::from(b.radius) + k2 * spread.clamp(0.0, 2.0)) * (1.0 + 1e-6));
                }
                Part::Egg(e) => {
                    let spread = f64::from(e.most) / f64::from(e.least) - 1.0;
                    e.most = up((f64::from(e.most) + k2 * spread.clamp(0.0, 2.0)) * (1.0 + 1e-6));
                }
            }
        }
        if parts.is_empty() {
            return Err(fail("no_flesh", None, None, None, None));
        }
        let (t, k) = (f64::from(keep), f64::from(smooth));
        let size = |v: V3| v[0].abs() + v[1].abs() + v[2].abs();
        let widest = parts.iter().map(|p| f64::from(size(p.origin())) + f64::from(p.reach_of())).fold(0.0f64, f64::max);
        let scale = up(widest + 9.0 * k + t);
        let mut seams = make_seams(bones, &parts, &of, joints, smooth)?;
        let group = make_groups(parts.len(), &mut seams);
        let grouped = !seams.is_empty();
        let reach = cull_reach(t, k, grouped);
        let div = if grouped { 1.0 + 0.5 * WIDTH_SLOPE } else { 1.0 };
        let fall = if grouped { smooth } else { 0.5 * smooth };
        Ok(Flesh { parts, of, smooth, keep, reach, scale, seams, group, div, fall })
    }

    /// 7.8's width at `x` of each group (indexed by group): the body's, or less within one of
    /// that group's seams' balls, the seams' widths and the body's joined softly (7.9's join
    /// at half the body's width, and at least 0), so that the width has a continuous slope.
    /// `seams` rising indices into the body's seams, or every seam.
    #[inline]
    fn widths(&self, x: V3, seams: Option<&[u16]>, out: &mut [f32; MAX_PARTS]) {
        let mut seen = [(0u8, 0.0f32); MAX_SEAMS];
        let mut n = 0;
        let mut see = |s: &Seam| {
            if let Some(w) = s.at(x) {
                if n < MAX_SEAMS {
                    seen[n] = (s.group, w);
                    n += 1;
                }
            }
        };
        match seams {
            None => self.seams.iter().for_each(&mut see),
            Some(list) => list.iter().filter_map(|&i| self.seams.get(usize::from(i))).for_each(&mut see),
        }
        let mut done = 0u64;
        for i in 0..n {
            let g = seen[i].0;
            if done & (1 << g) != 0 {
                continue;
            }
            done |= 1 << g;
            let mut vals = [self.smooth; MAX_PARTS];
            let mut m = 1;
            for &(h, w) in &seen[i..n] {
                if h == g && m < MAX_PARTS {
                    vals[m] = w;
                    m += 1;
                }
            }
            out[usize::from(g)] = blend(&vals[..m], 0.5 * self.smooth).max(0.0);
        }
    }

    /// 7.8's `G` where groups exist: each group's values joined (7.9) at the group's own
    /// width, then the groups' values joined at the body's width (`tags[i]` the group of
    /// `values[i]`).
    fn grouped(&self, values: &[f32], tags: &[u8], x: V3, seams: Option<&[u16]>) -> f32 {
        let mut width = [self.smooth; MAX_PARTS];
        self.widths(x, seams, &mut width);
        let n = values.len().min(MAX_PARTS);
        // The values put in their groups' order (a count of each group, then each in place).
        let mut count = [0u8; MAX_PARTS];
        let mut seen = 0u64;
        for &t in &tags[..n] {
            count[usize::from(t)] += 1;
            seen |= 1 << t;
        }
        let mut at = [0u8; MAX_PARTS];
        let (mut bits, mut acc) = (seen, 0u8);
        while bits != 0 {
            let g = bits.trailing_zeros() as usize;
            at[g] = acc;
            acc += count[g];
            bits &= bits - 1;
        }
        let mut placed = [0.0f32; MAX_PARTS];
        let mut next = at;
        for (&v, &t) in values[..n].iter().zip(&tags[..n]) {
            let g = usize::from(t);
            placed[usize::from(next[g])] = v;
            next[g] += 1;
        }
        let mut outer = [0.0f32; MAX_PARTS];
        let mut m = 0;
        while seen != 0 {
            let g = seen.trailing_zeros() as usize;
            let from = usize::from(at[g]);
            outer[m] = blend(&placed[from..from + usize::from(count[g])], width[g]);
            m += 1;
            seen &= seen - 1;
        }
        blend(&outer[..m], self.smooth)
    }

    /// The joined value of `n` part values at `x` (`tags` their groups), held at `keep` and
    /// divided by 7.8's factor.
    #[inline]
    fn joined(&self, values: &mut [f32], tags: &[u8], x: V3, seams: Option<&[u16]>) -> f32 {
        let g = if values.len() > 1 && !self.seams.is_empty() { self.grouped(values, tags, x, seams) } else { blend(values, self.smooth) };
        let h = g.min(self.keep);
        if self.div == 1.0 {
            h
        } else {
            h / self.div
        }
    }

    /// The body's field at `x`: only the parts whose capsule widened by [`Flesh::reach`] holds
    /// `x` are asked, and the value is [`Flesh::whole`]'s (7.6).
    pub fn value(&self, x: V3) -> f32 {
        let mut values = [0.0f32; MAX_PARTS];
        let mut tags = [0u8; MAX_PARTS];
        let mut n = 0;
        for (p, &g) in self.parts.iter().zip(&self.group) {
            let l = p.local(x);
            let c = p.capsule(l);
            if c <= self.reach {
                values[n] = p.field(l, c);
                tags[n] = g;
                n += 1;
            }
        }
        self.joined(&mut values[..n], &tags[..n], x, None)
    }

    /// [`Flesh::value`] asking only the parts in `among` (rising, as [`Flesh::meets`] gives
    /// them; past the parts, the seams it gives): the same number wherever `among` holds every
    /// part [`Flesh::value`] would ask and every seam whose ball holds `x`.
    pub fn value_among(&self, x: V3, among: &[u16]) -> f32 {
        let mut values = [0.0f32; MAX_PARTS];
        let mut tags = [0u8; MAX_PARTS];
        let mut n = 0;
        let parts = self.parts.len();
        let split = among.partition_point(|&i| usize::from(i) < parts);
        for &i in among[..split].iter().take(MAX_PARTS) {
            let Some(p) = self.parts.get(usize::from(i)) else { continue };
            let l = p.local(x);
            let c = p.capsule(l);
            if c <= self.reach {
                values[n] = p.field(l, c);
                tags[n] = self.group[usize::from(i)];
                n += 1;
            }
        }
        let mut seams = [0u16; MAX_SEAMS];
        let mut m = 0;
        for &i in among[split..].iter().take(MAX_SEAMS) {
            seams[m] = i - parts as u16;
            m += 1;
        }
        self.joined(&mut values[..n], &tags[..n], x, Some(&seams[..m]))
    }

    /// The body's field asking every part: 7.5's `H` (7.8's, divided by its factor), what
    /// [`Flesh::value`] is defined to equal.
    pub fn whole(&self, x: V3) -> f32 {
        let mut values = [0.0f32; MAX_PARTS];
        for (v, p) in values.iter_mut().zip(&self.parts) {
            let l = p.local(x);
            *v = p.field(l, p.capsule(l));
        }
        let n = self.parts.len();
        self.joined(&mut values[..n], &self.group, x, None)
    }

    /// The parts any point of the ball of radius `r` about `o` can ask, rising, and after them
    /// (numbered from [`Flesh::parts`] on) the seams whose ball such a point can be in: every
    /// part and seam [`Flesh::value`] asks at such a point is among them.
    pub fn meets(&self, o: V3, r: f32, out: &mut Vec<u16>) {
        out.clear();
        let sb = self.slack_ball(o, r);
        let limit = self.reach + r + 2.0 * sb;
        for (i, p) in self.parts.iter().enumerate() {
            if p.capsule(p.local(o)) <= limit {
                out.push(i as u16);
            }
        }
        for (i, s) in self.seams.iter().enumerate() {
            let d = [o[0] - s.centre[0], o[1] - s.centre[1], o[2] - s.centre[2]];
            let room = (s.ball + r) * (1.0 + 1e-4) + 2.0 * sb;
            if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= room * room {
                out.push((self.parts.len() + i) as u16);
            }
        }
    }

    /// Bounds of [`Flesh::value`] over the ball of radius `r` about `o` (7.6, "Over a ball"):
    /// every value it takes there, in `f32`, lies between them.
    pub fn range(&self, o: V3, r: f32) -> (f32, f32) {
        let h = self.value(o);
        let s = self.slack_ball(o, r);
        let top = if self.div == 1.0 { self.keep } else { self.keep / self.div };
        let hi = (h + r + 2.0 * s).min(top);
        let least = self.parts.iter().map(|p| p.capsule(p.local(o))).fold(f32::INFINITY, f32::min);
        let low = (least - r - self.fall - s).min(self.keep);
        let far = if self.div == 1.0 { low } else { low / self.div } - s;
        ((h - r - 2.0 * s).max(far), hi)
    }

    /// [`Flesh::meets`] looking only among `among` (a list [`Flesh::meets`] gave for a ball
    /// holding this one; `None`: every part and seam).
    pub fn meets_among(&self, o: V3, r: f32, among: Option<&[u16]>, out: &mut Vec<u16>) {
        let Some(list) = among else { return self.meets(o, r, out) };
        out.clear();
        let sb = self.slack_ball(o, r);
        let limit = self.reach + r + 2.0 * sb;
        let parts = self.parts.len();
        for &i in list {
            let i = usize::from(i);
            if let Some(p) = self.parts.get(i) {
                if p.capsule(p.local(o)) <= limit {
                    out.push(i as u16);
                }
            } else if let Some(s) = self.seams.get(i - parts) {
                let d = [o[0] - s.centre[0], o[1] - s.centre[1], o[2] - s.centre[2]];
                let room = (s.ball + r) * (1.0 + 1e-4) + 2.0 * sb;
                if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= room * room {
                    out.push(i as u16);
                }
            }
        }
    }

    /// The same body held at another `keep` (7.6's `T`: the mesher's, from its cells).
    pub fn with_keep(&self, keep: f32) -> Result<Flesh, CaroError> {
        if !(keep >= KEEP.0 && keep <= KEEP.1) {
            return Err(fail("keep", None, None, Some(f64::from(keep)), Some(KEEP)));
        }
        let (t, k) = (f64::from(keep), f64::from(self.smooth));
        let size = |v: V3| v[0].abs() + v[1].abs() + v[2].abs();
        let widest = self.parts.iter().map(|p| f64::from(size(p.origin())) + f64::from(p.reach_of())).fold(0.0f64, f64::max);
        Ok(Flesh { keep, reach: cull_reach(t, k, !self.seams.is_empty()), scale: up(widest + 9.0 * k + t), ..self.clone() })
    }

    /// A box holding every point where [`Flesh::value`] is at most `level` (7.5 (2): there some
    /// part's capsule term is at most `level (1 + Λ/2) + K/2`, `+ K` with groups: 7.9), with the slack
    /// of its far corner;
    /// `None` where `level` is the held level or more (every point is then).
    pub fn reach_box(&self, level: f32) -> Option<(V3, V3)> {
        if !(level.is_finite() && level * self.div < self.keep) {
            return None;
        }
        let extra = (f64::from(level) * f64::from(self.div) + f64::from(self.fall)).max(0.0);
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for p in &self.parts {
            let (a, b, r) = match p {
                Part::Bone(s) => {
                    let at = |t: f32| [0, 1, 2].map(|c| f64::from(s.origin[c]) + f64::from(t) * f64::from(s.frame[0][c]));
                    (at(s.t0), at(s.t1), f64::from(s.radius))
                }
                Part::Egg(e) => (d3(e.origin), d3(e.origin), f64::from(e.most)),
            };
            for c in 0..3 {
                lo[c] = lo[c].min(a[c].min(b[c]) - r - extra);
                hi[c] = hi[c].max(a[c].max(b[c]) + r + extra);
            }
        }
        let far = (0..3).map(|c| lo[c].abs().max(hi[c].abs())).sum::<f64>();
        let pad = 2.0 * f64::from(SLACK) * (far + f64::from(self.scale)) + 1e-6 * far;
        Some((lo.map(|v| down(v - pad)), hi.map(|v| up(v + pad))))
    }

    /// [`Flesh::range`], where every point of the ball asks only parts in `among` (a list
    /// [`Flesh::meets`] gave for a ball holding this one): the capsules' bound is taken over
    /// the list (a part left out of it has its capsule past `ε + r` at `o`, where the bound is
    /// the held level already).
    pub fn range_among(&self, o: V3, r: f32, among: Option<&[u16]>) -> (f32, f32) {
        let Some(list) = among else { return self.range(o, r) };
        let h = self.value_among(o, list);
        let s = self.slack_ball(o, r);
        let top = if self.div == 1.0 { self.keep } else { self.keep / self.div };
        let hi = (h + r + 2.0 * s).min(top);
        let least = list.iter().filter_map(|&i| self.parts.get(usize::from(i))).map(|p| p.capsule(p.local(o))).fold(f32::INFINITY, f32::min);
        let low = (least - r - self.fall - s).min(self.keep);
        let far = if self.div == 1.0 { low } else { low / self.div } - s;
        ((h - r - 2.0 * s).max(far), hi)
    }

    /// What a part's field weighs to evaluate: 0 a section by roots (squareness 2, 4 or 8), 1
    /// by powers (any other), 2 a swell.
    pub fn part_kind(&self, i: usize) -> usize {
        match &self.parts[i] {
            Part::Bone(b) if matches!(b.gauge, Gauge::Power(..)) => 1,
            Part::Bone(_) => 0,
            Part::Egg(_) => 2,
        }
    }

    /// How tightly the flesh turns at its tightest, metres: of each station's section the
    /// least radius of its ellipse (`min² / max` of its half sizes), of each swell its least
    /// radius squared over its greatest.
    pub fn tightest(&self) -> f32 {
        let mut t = f32::INFINITY;
        for p in &self.parts {
            match p {
                Part::Bone(b) => {
                    for piece in &b.pieces[..b.count.saturating_sub(1).max(1)] {
                        for end in 0..2 {
                            let (a, bb) = (piece.a[end], piece.q[end] * piece.a[end]);
                            t = t.min(a.min(bb) * a.min(bb) / a.max(bb));
                        }
                    }
                }
                Part::Egg(e) => t = t.min(e.least * e.least / e.most),
            }
        }
        t
    }

    /// The reach (7.3's `R`) of bone `bone`'s flesh, or 0 for a bone without.
    pub fn part_reach_of_bone(&self, bone: usize) -> f32 {
        self.of.iter().position(|o| *o == PartOf::Bone(bone)).map_or(0.0, |i| self.parts[i].reach_of())
    }

    /// How many seams (joints whose width is under the body's) the body has.
    pub fn seams(&self) -> usize {
        self.seams.len()
    }

    /// 7.8's least width at `x` of any group (the body's smoothing, or less near a straight
    /// joint).
    pub fn width_at(&self, x: V3) -> f32 {
        let mut w = [self.smooth; MAX_PARTS];
        self.widths(x, None, &mut w);
        w.into_iter().fold(self.smooth, f32::min)
    }

    /// What the joined value is divided by (7.9's `1 + Λ/2`, or 1 where no width changes).
    pub fn divisor(&self) -> f32 {
        self.div
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

/// A body as one node of a sculpt ([`crate::sdf::Prim::Body`]): its flesh, shared, and the parts
/// and seams the ball it was last pruned for can ask (`None`: all), so that a block's samples
/// ask its list and nothing else. Two are equal when their flesh's numbers and their lists are.
#[derive(Clone)]
pub struct Carnis {
    flesh: std::sync::Arc<Flesh>,
    among: Option<std::sync::Arc<[u16]>>,
    key: u64,
}

impl std::fmt::Debug for Carnis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Carnis {{ parts: {}, seams: {}, asked: {:?}, key: {:#x} }}", self.flesh.parts(), self.flesh.seams(), self.among.as_ref().map(|a| a.len()), self.key)
    }
}

impl PartialEq for Carnis {
    fn eq(&self, other: &Carnis) -> bool {
        self.key == other.key && self.among == other.among
    }
}

impl Carnis {
    /// The node of `flesh`, asking every part.
    pub fn new(flesh: Flesh) -> Carnis {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        let mut eat = |v: f32| {
            h ^= u64::from(v.to_bits());
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        };
        for p in &flesh.parts {
            match p {
                Part::Bone(b) => {
                    b.origin.iter().chain(b.frame.iter().flatten()).for_each(|&v| eat(v));
                    [b.t0, b.t1, b.lip, b.radius].into_iter().for_each(&mut eat);
                    b.stations[..b.count].iter().for_each(|&v| eat(v));
                    for piece in &b.pieces[..b.count.saturating_sub(1).max(1)] {
                        piece.a.iter().chain(&piece.q).chain(&[piece.t0, piece.span]).for_each(|&v| eat(v));
                    }
                    eat(match b.gauge {
                        Gauge::Two => 2.0,
                        Gauge::Four => 4.0,
                        Gauge::Eight => 8.0,
                        Gauge::Power(p, _) => p,
                    });
                }
                Part::Egg(e) => e.origin.iter().chain(e.frame.iter().flatten()).chain(&e.inv).chain(&[e.least, e.most]).for_each(|&v| eat(v)),
            }
        }
        for s in &flesh.seams {
            s.centre.iter().chain(&s.normal).chain(&[s.half, s.radius, s.width, s.ball, s.soft, f32::from(s.group)]).for_each(|&v| eat(v));
        }
        [flesh.smooth, flesh.keep].into_iter().for_each(eat);
        Carnis { flesh: std::sync::Arc::new(flesh), among: None, key: h }
    }

    pub fn flesh(&self) -> &Flesh {
        &self.flesh
    }

    /// The field at `q` (in the node's frame), asking the node's list.
    #[inline]
    pub fn value(&self, q: V3) -> f32 {
        match &self.among {
            Some(list) => self.flesh.value_among(q, list),
            None => self.flesh.value(q),
        }
    }

    /// The node as it is for every point of the ball of radius `r` about `q`: its list cut to
    /// what such a point can ask ([`Flesh::meets_among`]); the same value at every such point.
    pub fn near(&self, q: V3, r: f32) -> Carnis {
        let mut out = Vec::new();
        self.flesh.meets_among(q, r, self.among.as_deref(), &mut out);
        Carnis { flesh: self.flesh.clone(), among: Some(out.into()), key: self.key }
    }

    /// Bounds of the node's field over the ball of radius `r` about `q` ([`Flesh::range_among`]).
    pub fn range(&self, q: V3, r: f32) -> (f32, f32) {
        self.flesh.range_among(q, r, self.among.as_deref())
    }

    /// The parts a sample asks, by [`Flesh::part_kind`], and the seams: the list's, or all.
    pub fn kinds_asked(&self) -> ([u64; 3], u64) {
        let mut k = [0u64; 3];
        let parts = self.flesh.parts();
        let mut seams = 0;
        let mut count = |i: usize| {
            if i < parts {
                k[self.flesh.part_kind(i)] += 1;
            } else {
                seams += 1;
            }
        };
        match &self.among {
            Some(list) => list.iter().for_each(|&i| count(usize::from(i))),
            None => (0..parts + self.flesh.seams()).for_each(count),
        }
        (k, seams)
    }

    /// How many parts and seams a sample asks the capsules of: the list's, or all.
    pub fn asked(&self) -> (usize, usize) {
        let parts = self.flesh.parts();
        match &self.among {
            Some(list) => {
                let split = list.partition_point(|&i| usize::from(i) < parts);
                (split, list.len() - split)
            }
            None => (parts, self.flesh.seams()),
        }
    }

    /// The same node with its flesh held at `keep` (the mesher's), asking every part.
    pub fn with_keep(&self, keep: f32) -> Result<Carnis, CaroError> {
        if keep == self.flesh.keep && self.among.is_none() {
            return Ok(self.clone());
        }
        Ok(Carnis::new(self.flesh.with_keep(keep)?))
    }
}

#[cfg(test)]
pub(crate) mod tests;

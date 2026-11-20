//! The numbered statements of docs/design/DISTANCE_FIELDS.md, held in numbers. Nothing of the
//! modeller is used: each field here is written out in a few lines from its definition, each
//! bound is held against a nearest point found by search over a sampled surface, and each
//! "reached exactly where" against the case that reaches it. If a proof on that page were
//! wrong, a test here would say so.

use specula_core::dmath;

type P2 = [f64; 2];
type P3 = [f64; 3];
/// A join of two numbers at a width.
type Join = fn(f64, f64, f64) -> f64;
/// A join of two numbers, its width fixed.
type Joined = Box<dyn Fn(f64, f64) -> f64>;

/// xorshift64*: a fixed sequence.
struct Seq(u64);
impl Seq {
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    fn span(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

fn len2(p: P2) -> f64 {
    (p[0] * p[0] + p[1] * p[1]).sqrt()
}
fn dist2(a: P2, b: P2) -> f64 {
    len2([a[0] - b[0], a[1] - b[1]])
}
fn dist3(a: P3, b: P3) -> f64 {
    ((a[0] - b[0]) * (a[0] - b[0]) + (a[1] - b[1]) * (a[1] - b[1]) + (a[2] - b[2]) * (a[2] - b[2])).sqrt()
}

// ---- The three joins of section 5, from their definitions. ----

fn reach(a: f64, b: f64, k: f64) -> f64 {
    (k - (a - b).abs()).max(0.0) / k
}
/// The polynomial join of degree `n`: `m - k h^n / (2n)` (2: quadratic, 3: cubic).
fn poly(a: f64, b: f64, k: f64, n: u32) -> f64 {
    let h = reach(a, b, k);
    let mut hn = 1.0;
    for _ in 0..n {
        hn *= h;
    }
    a.min(b) - k * hn / (2.0 * f64::from(n))
}
fn expo(a: f64, b: f64, k: f64) -> f64 {
    // (Written from the lesser, so that large arguments cannot overflow.)
    a.min(b) - k * dmath::ln_f64(1.0 + dmath::exp_f64(-(a - b).abs() / k))
}
fn ln2() -> f64 {
    dmath::ln_f64(2.0)
}

/// 5.1: how far each join dips below the lesser, and where it reaches its limit.
#[test]
fn a_join_dips_by_at_most_its_limit_and_reaches_it_where_the_fields_are_equal() {
    let mut s = Seq(0x5eed_d157_0001);
    for _ in 0..200_000 {
        let k = dmath::exp_f64(s.span(-9.0, 9.0));
        let a = s.span(-4.0, 4.0) * k;
        let b = if s.unit() < 0.2 { a } else { a + s.span(-3.0, 3.0) * k };
        let m = a.min(b);
        let tiny = 1e-12 * (k + a.abs() + b.abs());
        for (n, limit) in [(2u32, 0.25), (3, 1.0 / 6.0), (4, 0.125), (5, 0.1)] {
            let dip = m - poly(a, b, k, n);
            assert!(dip >= -tiny && dip <= k * limit + tiny, "degree {n}: a {a}, b {b}, k {k}: dips {dip}, limit {}", k * limit);
            if (a - b).abs() >= k {
                assert!(dip.abs() <= tiny, "degree {n}: the fields are {} apart, over k = {k}, and the join dips {dip}", (a - b).abs());
            }
            if a == b {
                assert!((dip - k * limit).abs() <= tiny, "degree {n}: equal fields dip {dip}, not {}", k * limit);
            }
        }
        let dip = m - expo(a, b, k);
        assert!(dip > 0.0 || (a - b).abs() / k > 30.0, "the exponential join is not below the lesser: a {a}, b {b}, k {k}");
        assert!(dip <= k * ln2() + tiny, "the exponential join dips {dip}, over k ln 2 = {}", k * ln2());
        if a == b {
            assert!((dip - k * ln2()).abs() <= tiny, "equal fields dip {dip} exponentially, not {}", k * ln2());
        }
    }
}

/// 5.2 and 5.3: each join's two slopes are shares of one, so a join changes by no more than the
/// greater change of its two inputs.
#[test]
fn a_joins_slopes_are_two_shares_of_one() {
    let mut s = Seq(0x5eed_d157_0002);
    let joins: [(&str, Join); 4] =[("quadratic", |a, b, k| poly(a, b, k, 2)), ("cubic", |a, b, k| poly(a, b, k, 3)), ("quartic", |a, b, k| poly(a, b, k, 4)), ("exponential", expo)];
    for _ in 0..100_000 {
        let k = dmath::exp_f64(s.span(-3.0, 3.0));
        let (a, b) = (s.span(-3.0, 3.0) * k, s.span(-3.0, 3.0) * k);
        let (a2, b2) = (a + s.span(-2.0, 2.0) * k, b + s.span(-2.0, 2.0) * k);
        let step = 1e-6 * k;
        for (name, f) in joins {
            // The slopes by differences either side.
            let da = (f(a + step, b, k) - f(a - step, b, k)) / (2.0 * step);
            let db = (f(a, b + step, k) - f(a, b - step, k)) / (2.0 * step);
            assert!(da >= -1e-6 && db >= -1e-6 && (da + db - 1.0).abs() <= 1e-5, "{name}: slopes {da} and {db} at a {a}, b {b}, k {k}");
            // Moving both inputs by one moves the join by one.
            let t = s.span(-5.0, 5.0) * k;
            assert!((f(a + t, b + t, k) - f(a, b, k) - t).abs() <= 1e-9 * (k + t.abs() + a.abs() + b.abs()), "{name}: not carried along by {t}");
            // 5.3.
            let moved = (f(a, b, k) - f(a2, b2, k)).abs();
            let most = (a - a2).abs().max((b - b2).abs());
            assert!(moved <= most * (1.0 + 1e-12) + 1e-12 * k, "{name}: moved {moved} for inputs moved {most}");
        }
        // On the line of equal fields both shares are a half, from either side.
        for (name, f) in joins {
            let e = 1e-7 * k;
            let left = (f(a + e, a, k) - f(a, a, k)) / e;
            let right = (f(a, a, k) - f(a - e, a, k)) / e;
            assert!((left - 0.5).abs() <= 1e-5 && (right - 0.5).abs() <= 1e-5, "{name}: slopes {right} and {left} across equal fields");
        }
    }
}

/// 5.2: the quadratic join's second derivative steps where the fields are `k` apart, and the
/// cubic join's does not.
#[test]
fn the_cubic_join_is_twice_differentiable_and_the_quadratic_once() {
    let k = 1.0;
    let second = |f: &dyn Fn(f64) -> f64, t: f64| {
        let e = 1e-4;
        (f(t + e) - 2.0 * f(t) + f(t - e)) / (e * e)
    };
    let quadratic = |t: f64| poly(0.0, t, k, 2);
    let cubic = |t: f64| poly(0.0, t, k, 3);
    // Just inside and just outside the join's reach.
    let (q_in, q_out) = (second(&quadratic, 0.99), second(&quadratic, 1.01));
    let (c_in, c_out) = (second(&cubic, 0.99), second(&cubic, 1.01));
    assert!((q_in + 0.5).abs() < 1e-3 && q_out.abs() < 1e-6, "the quadratic join's second derivative is {q_in} inside its reach and {q_out} outside");
    assert!(c_in.abs() < 0.02 && c_out.abs() < 1e-6, "the cubic join's second derivative is {c_in} inside its reach and {c_out} outside");
    // Across equal fields both are continuous.
    for f in [&quadratic as &dyn Fn(f64) -> f64, &cubic] {
        let (a, b) = (second(f, -0.01), second(f, 0.01));
        assert!((a - b).abs() < 1e-3, "a second derivative steps across equal fields: {a} and {b}");
    }
}

// ---- Plane solids with their exact signed distances, from their definitions. ----

#[derive(Clone, Copy)]
enum Plane {
    Disc { c: P2, r: f64 },
    Box { c: P2, half: P2 },
}

impl Plane {
    fn signed(self, p: P2) -> f64 {
        match self {
            Plane::Disc { c, r } => dist2(p, c) - r,
            Plane::Box { c, half } => {
                let q = [(p[0] - c[0]).abs() - half[0], (p[1] - c[1]).abs() - half[1]];
                len2([q[0].max(0.0), q[1].max(0.0)]) + q[0].max(q[1]).min(0.0)
            }
        }
    }
    /// Points of its surface, no further apart than `gap`.
    fn surface(self, gap: f64) -> Vec<P2> {
        match self {
            Plane::Disc { c, r } => {
                let n = (std::f64::consts::TAU * r / gap) as usize + 8;
                (0..n)
                    .map(|i| {
                        let (sn, cs) = dmath::sin_cos_f64(std::f64::consts::TAU * i as f64 / n as f64);
                        [c[0] + r * cs, c[1] + r * sn]
                    })
                    .collect()
            }
            Plane::Box { c, half } => {
                let mut out = Vec::new();
                let corners = [[-half[0], -half[1]], [half[0], -half[1]], [half[0], half[1]], [-half[0], half[1]]];
                for i in 0..4 {
                    let (a, b) = (corners[i], corners[(i + 1) % 4]);
                    let n = (dist2(a, b) / gap) as usize + 2;
                    for j in 0..n {
                        let t = j as f64 / n as f64;
                        out.push([c[0] + a[0] + (b[0] - a[0]) * t, c[1] + a[1] + (b[1] - a[1]) * t]);
                    }
                }
                out
            }
        }
    }
}

fn nearest2(points: &[P2], p: P2) -> f64 {
    points.iter().map(|q| dist2(*q, p)).fold(f64::MAX, f64::min)
}

fn pair(s: &mut Seq) -> (Plane, Plane) {
    let one = |s: &mut Seq| {
        let c = [s.span(-0.6, 0.6), s.span(-0.6, 0.6)];
        if s.unit() < 0.5 {
            Plane::Disc { c, r: s.span(0.3, 1.0) }
        } else {
            Plane::Box { c, half: [s.span(0.2, 0.9), s.span(0.2, 0.9)] }
        }
    };
    (one(s), one(s))
}

/// 2.2: a signed distance never changes faster than one for one.
#[test]
fn a_signed_distance_changes_no_faster_than_one_for_one() {
    let mut s = Seq(0x5eed_d157_0003);
    for _ in 0..2_000 {
        let (a, _) = pair(&mut s);
        for _ in 0..50 {
            let (p, q) = ([s.span(-3.0, 3.0), s.span(-3.0, 3.0)], [s.span(-3.0, 3.0), s.span(-3.0, 3.0)]);
            assert!((a.signed(p) - a.signed(q)).abs() <= dist2(p, q) * (1.0 + 1e-12) + 1e-12);
        }
    }
}

/// 3.2, 3.3 and 3.4: a union is exact outside and a bound inside; a meeting and a cut are exact
/// inside and bounds outside. The truth is the nearest point of the surface, found by search.
#[test]
fn a_union_is_exact_outside_and_a_meeting_inside_and_both_are_bounds() {
    let mut s = Seq(0x5eed_d157_0004);
    let gap = 2e-3;
    let (mut unions, mut meetings, mut cuts) = (0usize, 0usize, 0usize);
    for _ in 0..60 {
        let (a, b) = pair(&mut s);
        let (sa, sb) = (a.surface(gap), b.surface(gap));
        // The three solids' surfaces: the parts of each surface on the right side of the other.
        let union: Vec<P2> = sa.iter().filter(|p| b.signed(**p) >= 0.0).chain(sb.iter().filter(|p| a.signed(**p) >= 0.0)).copied().collect();
        let meeting: Vec<P2> = sa.iter().filter(|p| b.signed(**p) <= 0.0).chain(sb.iter().filter(|p| a.signed(**p) <= 0.0)).copied().collect();
        let cut: Vec<P2> = sa.iter().filter(|p| b.signed(**p) >= 0.0).chain(sb.iter().filter(|p| a.signed(**p) <= 0.0)).copied().collect();
        for _ in 0..300 {
            let p = [s.span(-2.5, 2.5), s.span(-2.5, 2.5)];
            let (fa, fb) = (a.signed(p), b.signed(p));
            // The union.
            let (u, truth) = (fa.min(fb), nearest2(&union, p));
            if u > 0.0 {
                assert!((u - truth).abs() <= gap, "a union outside: the field is {u} and the surface is {truth} away");
            } else {
                assert!(-u <= truth + gap, "a union inside: the field is {u} deep and the surface is only {truth} away");
            }
            unions += 1;
            // The meeting, where there is one.
            if !meeting.is_empty() {
                let (m, truth) = (fa.max(fb), nearest2(&meeting, p));
                if m < 0.0 {
                    assert!((-m - truth).abs() <= gap, "a meeting inside: the field is {m} and the surface is {truth} away");
                } else {
                    assert!(m <= truth + gap, "a meeting outside: the field is {m} and the surface is only {truth} away");
                }
                meetings += 1;
            }
            // The cut, where anything is left.
            if !cut.is_empty() && cut.iter().any(|q| a.signed(*q).abs() < 1e-9) {
                let (c, truth) = (fa.max(-fb), nearest2(&cut, p));
                if c < 0.0 {
                    assert!((-c - truth).abs() <= gap, "a cut inside: the field is {c} and the surface is {truth} away");
                } else {
                    assert!(c <= truth + gap, "a cut outside: the field is {c} and the surface is only {truth} away");
                }
                cuts += 1;
            }
        }
    }
    assert!(unions > 10_000 && meetings > 5_000 && cuts > 5_000, "{unions} unions, {meetings} meetings, {cuts} cuts");
    // The page's own example of a union falling short inside: two stretches of a line.
    let (a, b, both) = (|x: f64| (x + 0.5).abs() - 1.5, |x: f64| (x - 0.5).abs() - 1.5, |x: f64| x.abs() - 2.0);
    assert!(a(0.0).min(b(0.0)) == -1.0 && both(0.0) == -2.0, "the union of [-2, 1] and [-1, 2] at 0 reads {} and is {} deep", a(0.0).min(b(0.0)), both(0.0));
    // And of a meeting falling short outside by any ratio: a wedge, the thinner the shorter. Two
    // half-planes through the origin, each holding the points where its normal's product is not
    // positive; their meeting is the wedge about the negative x axis, whose nearest point to
    // `p` on the positive axis is its tip.
    let mut last = 1.0;
    let p: P2 = [1.0, 0.0];
    for half_angle in [0.5f64, 0.1, 0.02, 0.004] {
        let (sn, cs) = dmath::sin_cos_f64(half_angle);
        let field = (sn * p[0] + cs * p[1]).max(sn * p[0] - cs * p[1]);
        let ratio = field / len2(p);
        assert!(ratio < last && (ratio - sn).abs() < 1e-12, "a wedge of half angle {half_angle}: field over distance {ratio}");
        last = ratio;
    }
    assert!(last < 0.005, "the thinnest wedge's field is {last} of the distance");
}

/// 4.1: a plane figure raised into a slab has the exact distance the page gives.
#[test]
fn a_figure_raised_is_exact() {
    let mut s = Seq(0x5eed_d157_0005);
    let gap = 0.02;
    for _ in 0..8 {
        let (fig, _) = pair(&mut s);
        let h = s.span(0.1, 0.8);
        // The slab's surface: the wall over the figure's outline, and the two caps.
        let mut surface: Vec<P3> = Vec::new();
        let levels = (2.0 * h / gap) as usize + 2;
        for p in fig.surface(gap) {
            for j in 0..=levels {
                surface.push([p[0], p[1], -h + 2.0 * h * j as f64 / levels as f64]);
            }
        }
        let grid = (4.0 / gap) as usize;
        for i in 0..=grid {
            for j in 0..=grid {
                let p = [-2.0 + 4.0 * i as f64 / grid as f64, -2.0 + 4.0 * j as f64 / grid as f64];
                if fig.signed(p) <= 0.0 {
                    surface.push([p[0], p[1], h]);
                    surface.push([p[0], p[1], -h]);
                }
            }
        }
        for _ in 0..120 {
            let p = [s.span(-2.2, 2.2), s.span(-2.2, 2.2), s.span(-1.5, 1.5)];
            let (w1, w2) = (fig.signed([p[0], p[1]]), p[2].abs() - h);
            let f = w1.max(w2).min(0.0) + len2([w1.max(0.0), w2.max(0.0)]);
            let truth = surface.iter().map(|q| dist3(*q, p)).fold(f64::MAX, f64::min);
            assert!((f.abs() - truth).abs() <= 1.5 * gap, "a slab: the field is {f} at {p:?} and the surface is {truth} away");
            assert_eq!(f <= 0.0, w1 <= 0.0 && w2 <= 0.0, "a slab: the field's sign at {p:?}");
        }
    }
}

/// 4.2: a plane figure turned about an axis has the exact distance of the figure with its mirror
/// image; without the mirror it reads short inside, near the axis.
#[test]
fn a_figure_turned_is_exact_with_its_mirror_image() {
    let mut s = Seq(0x5eed_d157_0006);
    let gap = 0.02;
    // A ring: a disc off the axis, turned (a torus). Its mirror image does not meet it.
    for _ in 0..6 {
        let (centre, r) = ([s.span(1.0, 1.6), s.span(-0.3, 0.3)], s.span(0.2, 0.7));
        let fig = Plane::Disc { c: centre, r };
        let outline = fig.surface(gap);
        let mut surface: Vec<P3> = Vec::new();
        for q in &outline {
            let n = (std::f64::consts::TAU * q[0] / gap) as usize + 8;
            for i in 0..n {
                let (sn, cs) = dmath::sin_cos_f64(std::f64::consts::TAU * i as f64 / n as f64);
                surface.push([q[0] * cs, q[1], q[0] * sn]);
            }
        }
        for _ in 0..100 {
            let p = [s.span(-2.6, 2.6), s.span(-1.3, 1.3), s.span(-2.6, 2.6)];
            let f = fig.signed([len2([p[0], p[2]]), p[1]]);
            let truth = surface.iter().map(|q| dist3(*q, p)).fold(f64::MAX, f64::min);
            assert!((f.abs() - truth).abs() <= 1.5 * gap, "a ring: the field is {f} at {p:?} and the surface is {truth} away");
        }
    }
    // A ball: the half disc that touches the axis, turned. With its mirror image it is the whole
    // disc, whose distance is the ball's; alone, the stretch of axis counts as a surface.
    let whole = |r: f64, y: f64| len2([r, y]) - 1.0;
    let alone = |r: f64, y: f64| whole(r, y).max(-r);
    for _ in 0..20_000 {
        let p = [s.span(-1.6, 1.6), s.span(-1.6, 1.6), s.span(-1.6, 1.6)];
        let (r, truth) = (len2([p[0], p[2]]), (dist3(p, [0.0; 3]) - 1.0).abs());
        assert!((whole(r, p[1]).abs() - truth).abs() <= 1e-12, "a ball by its mirrored half disc");
        assert!(alone(r, p[1]).abs() <= truth + 1e-12, "a ball by its half disc alone exceeds the distance at {p:?}");
    }
    assert!((alone(0.1, 0.0) + 0.1).abs() < 1e-12 && (whole(0.1, 0.0) + 0.9).abs() < 1e-12, "near the axis the half disc alone reads 0.1 deep where the ball is 0.9 deep");
}

/// 4.3: a field less `rho` is exact outside the solid swollen by `rho`.
#[test]
fn a_swelling_is_exact_outside() {
    let mut s = Seq(0x5eed_d157_0007);
    let gap = 2e-3;
    for _ in 0..20 {
        let half = [s.span(0.2, 0.9), s.span(0.2, 0.9)];
        let rho = s.span(0.05, 0.6);
        let body = Plane::Box { c: [0.0, 0.0], half };
        // The swollen box's surface: its four sides moved out, and a quarter circle at each corner.
        let mut surface = Vec::new();
        for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
            let n = (std::f64::consts::FRAC_PI_2 * rho / gap) as usize + 4;
            for i in 0..=n {
                let (sn, cs) = dmath::sin_cos_f64(std::f64::consts::FRAC_PI_2 * i as f64 / n as f64);
                surface.push([sx * (half[0] + rho * cs), sy * (half[1] + rho * sn)]);
            }
        }
        for (axis, side) in [(0usize, 1.0), (0, -1.0), (1, 1.0), (1, -1.0)] {
            let other = 1 - axis;
            let n = (2.0 * half[other] / gap) as usize + 2;
            for i in 0..=n {
                let mut p = [0.0; 2];
                p[axis] = side * (half[axis] + rho);
                p[other] = -half[other] + 2.0 * half[other] * i as f64 / n as f64;
                surface.push(p);
            }
        }
        for _ in 0..400 {
            let p = [s.span(-3.0, 3.0), s.span(-3.0, 3.0)];
            let f = body.signed(p) - rho;
            if f > 0.0 {
                assert!((f - nearest2(&surface, p)).abs() <= gap, "a swollen box outside: the field is {f} and the surface is {} away", nearest2(&surface, p));
            } else {
                assert!(-f <= nearest2(&surface, p) + gap, "a swollen box inside: the field exceeds the depth");
            }
        }
    }
}

/// 2.5: an unevenly scaled field times its least scale never exceeds the distance.
#[test]
fn an_uneven_scale_keeps_a_bound_when_multiplied_by_its_least_factor() {
    let mut s = Seq(0x5eed_d157_0008);
    let gap = 2e-3;
    for _ in 0..30 {
        let scale = [s.span(0.2, 3.0), s.span(0.2, 3.0)];
        let least = scale[0].min(scale[1]);
        let field = |p: P2| least * (len2([p[0] / scale[0], p[1] / scale[1]]) - 1.0);
        // The ellipse itself.
        let n = (std::f64::consts::TAU * scale[0].max(scale[1]) / gap) as usize + 8;
        let surface: Vec<P2> = (0..n)
            .map(|i| {
                let (sn, cs) = dmath::sin_cos_f64(std::f64::consts::TAU * i as f64 / n as f64);
                [scale[0] * cs, scale[1] * sn]
            })
            .collect();
        for _ in 0..400 {
            let (p, q) = ([s.span(-5.0, 5.0), s.span(-5.0, 5.0)], [s.span(-5.0, 5.0), s.span(-5.0, 5.0)]);
            assert!((field(p) - field(q)).abs() <= dist2(p, q) * (1.0 + 1e-12) + 1e-12, "an unevenly scaled field changes faster than one for one");
            assert!(field(p).abs() <= nearest2(&surface, p) + gap, "an unevenly scaled field exceeds the distance: {} against {}", field(p), nearest2(&surface, p));
        }
    }
}

/// 5.4: a smooth join of two solids never changes faster than one for one, lies between the
/// union's field and that field less the join's dip, and so thickens the union by at most the
/// dip; a polynomial join changes nothing where the fields are `k` or more apart.
#[test]
fn a_smooth_join_of_two_solids_thickens_their_union_by_at_most_its_dip() {
    let mut s = Seq(0x5eed_d157_0009);
    for _ in 0..300 {
        let (a, b) = pair(&mut s);
        let k = s.span(0.02, 1.2);
        let joins: [(&str, f64, Joined); 3] =[("quadratic", k / 4.0, Box::new(move |x, y| poly(x, y, k, 2))), ("cubic", k / 6.0, Box::new(move |x, y| poly(x, y, k, 3))), ("exponential", k * ln2(), Box::new(move |x, y| expo(x, y, k)))];
        for (name, dip, f) in &joins {
            for _ in 0..100 {
                let (p, q) = ([s.span(-3.0, 3.0), s.span(-3.0, 3.0)], [s.span(-3.0, 3.0), s.span(-3.0, 3.0)]);
                let (up, uq) = (f(a.signed(p), b.signed(p)), f(a.signed(q), b.signed(q)));
                assert!((up - uq).abs() <= dist2(p, q) * (1.0 + 1e-12) + 1e-12, "{name}: the joined field changes {} over {}", (up - uq).abs(), dist2(p, q));
                let plain = a.signed(p).min(b.signed(p));
                assert!(up <= plain + 1e-12 && up >= plain - dip - 1e-12, "{name}: the joined field {up} is not within {dip} below the union's {plain}");
                // In the joined solid, and so within the dip of the union.
                if up <= 0.0 {
                    assert!(plain <= dip + 1e-12, "{name}: a point of the joined solid is {plain} from the union, over the dip {dip}");
                }
                if *name != "exponential" && (a.signed(p) - b.signed(p)).abs() >= k {
                    assert!((up - plain).abs() <= 1e-12, "{name}: changed where the fields are over k apart");
                }
            }
        }
    }
    // The slope through a join of two walls meeting square, with equal shares: cos 45 degrees.
    let k = 0.5;
    let walls = |p: P2| poly(p[0], p[1], k, 2);
    let (at, e) = ([0.3, 0.3], 1e-6);
    let slope = len2([(walls([at[0] + e, at[1]]) - walls([at[0] - e, at[1]])) / (2.0 * e), (walls([at[0], at[1] + e]) - walls([at[0], at[1] - e])) / (2.0 * e)]);
    assert!((slope - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6, "the slope through a square join is {slope}");
}

/// 5.5: several fields joined at once exponentially, and two at a time by a polynomial join.
#[test]
fn several_fields_joined() {
    let mut s = Seq(0x5eed_d157_000a);
    let mut orders_differ = false;
    for _ in 0..50_000 {
        let k = dmath::exp_f64(s.span(-2.0, 2.0));
        let n = 2 + (s.unit() * 6.0) as usize;
        let equal = s.unit() < 0.1;
        let first = s.span(-3.0, 3.0) * k;
        let fields: Vec<f64> = (0..n).map(|i| if equal || i == 0 { first } else { first + s.span(-2.0, 2.0) * k }).collect();
        let least = fields.iter().cloned().fold(f64::MAX, f64::min);
        // All at once, exponentially (from the least, so that nothing overflows).
        let sum: f64 = fields.iter().map(|a| dmath::exp_f64(-(a - least) / k)).sum();
        let at_once = least - k * dmath::ln_f64(sum);
        let limit = k * dmath::ln_f64(n as f64);
        let tiny = 1e-11 * (k + least.abs());
        assert!(at_once <= least + tiny && at_once >= least - limit - tiny, "{n} fields at once: {at_once} against the least {least} and its limit {limit}");
        if equal {
            assert!((least - at_once - limit).abs() <= tiny, "{n} equal fields dip {}, not k ln n = {limit}", least - at_once);
        }
        // Two at a time gives the same, in either order.
        let forward = fields.iter().skip(1).fold(fields[0], |acc, a| expo(acc, *a, k));
        let backward = fields.iter().rev().skip(1).fold(fields[n - 1], |acc, a| expo(acc, *a, k));
        assert!((forward - at_once).abs() <= 1e-9 * (k + least.abs()) && (backward - at_once).abs() <= 1e-9 * (k + least.abs()), "the exponential join depends on its order: {forward}, {backward}, {at_once}");
        // The polynomial joins, two at a time: within (n - 1) dips, and not the same in every order.
        for (degree, dip) in [(2u32, k / 4.0), (3, k / 6.0)] {
            let forward = fields.iter().skip(1).fold(fields[0], |acc, a| poly(acc, *a, k, degree));
            let backward = fields.iter().rev().skip(1).fold(fields[n - 1], |acc, a| poly(acc, *a, k, degree));
            for nested in [forward, backward] {
                assert!(nested <= least + tiny && nested >= least - (n - 1) as f64 * dip - tiny, "degree {degree}: {n} fields nested give {nested}, the least is {least}, a dip {dip}");
            }
            orders_differ |= (forward - backward).abs() > 1e-6 * k;
        }
    }
    assert!(orders_differ, "the polynomial joins never depended on their order: the page says they do");
}

/// 5.6 and 5.7: the smooth meeting rises above the greater by at most the dip; and the widths
/// that make every join's dip the width itself.
#[test]
fn the_smooth_meeting_and_the_width_that_means_what_it_says() {
    let mut s = Seq(0x5eed_d157_000b);
    for _ in 0..100_000 {
        let k = dmath::exp_f64(s.span(-3.0, 3.0));
        let (a, b) = (s.span(-3.0, 3.0) * k, s.span(-3.0, 3.0) * k);
        let tiny = 1e-12 * (k + a.abs() + b.abs());
        for (dip, meet) in [(k / 4.0, -poly(-a, -b, k, 2)), (k / 6.0, -poly(-a, -b, k, 3)), (k * ln2(), -expo(-a, -b, k))] {
            assert!(meet >= a.max(b) - tiny && meet <= a.max(b) + dip + tiny, "a smooth meeting {meet} against the greater {} and the dip {dip}", a.max(b));
        }
        // With equal fields each widened join dips by exactly k.
        for dip in [a - poly(a, a, 4.0 * k, 2), a - poly(a, a, 6.0 * k, 3), a - expo(a, a, k / ln2())] {
            assert!((dip - k).abs() <= 1e-11 * (k + a.abs()), "a join of width k dips {dip}, not {k}");
        }
    }
}

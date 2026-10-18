//! The contact solve pass four constraints at a time ("wide", Box2D v3's SIMD solver in plain
//! Rust): the constraints of a colour group share no moving body, so four of them can be solved
//! side by side, each lane doing exactly the arithmetic [`crate::solver::solve_one`] does in the
//! same order (no fused or reordered operations), so the results are bit-identical to solving
//! them one by one. The lanes are fixed-size arrays the compiler can keep in vector registers.

use crate::solver::{Constraint, Imp, Local, SolverPoint};

/// Lanes.
pub const W: usize = 4;

type F = [f32; W];

#[derive(Clone, Copy)]
struct V {
    x: F,
    y: F,
    z: F,
}

#[inline(always)]
fn map(f: impl Fn(usize) -> f32) -> F {
    std::array::from_fn(f)
}

#[inline(always)]
fn add(a: F, b: F) -> F {
    map(|l| a[l] + b[l])
}

#[inline(always)]
fn sub(a: F, b: F) -> F {
    map(|l| a[l] - b[l])
}

#[inline(always)]
fn mul(a: F, b: F) -> F {
    map(|l| a[l] * b[l])
}

impl V {
    #[inline(always)]
    fn add(self, o: V) -> V {
        V { x: add(self.x, o.x), y: add(self.y, o.y), z: add(self.z, o.z) }
    }

    #[inline(always)]
    fn sub(self, o: V) -> V {
        V { x: sub(self.x, o.x), y: sub(self.y, o.y), z: sub(self.z, o.z) }
    }

    /// Component times a per-lane scalar (`Vec3 * f32`).
    #[inline(always)]
    fn scale(self, s: F) -> V {
        V { x: mul(self.x, s), y: mul(self.y, s), z: mul(self.z, s) }
    }

    /// Component-wise product (`math::mul`).
    #[inline(always)]
    fn mul(self, o: V) -> V {
        V { x: mul(self.x, o.x), y: mul(self.y, o.y), z: mul(self.z, o.z) }
    }

    #[inline(always)]
    fn dot(self, o: V) -> F {
        add(add(mul(self.x, o.x), mul(self.y, o.y)), mul(self.z, o.z))
    }

    #[inline(always)]
    fn cross(self, o: V) -> V {
        V { x: sub(mul(self.y, o.z), mul(self.z, o.y)), y: sub(mul(self.z, o.x), mul(self.x, o.z)), z: sub(mul(self.x, o.y), mul(self.y, o.x)) }
    }
}

/// A body side of four constraints.
struct Side {
    v: V,
    w: V,
    dp: V,
    q: V,
    qw: F,
    inv_mass: V,
}

fn v_of(f: impl Fn(usize) -> crate::math::Vec3) -> V {
    let a: [crate::math::Vec3; W] = std::array::from_fn(f);
    V { x: map(|l| a[l].x), y: map(|l| a[l].y), z: map(|l| a[l].z) }
}

impl Side {
    fn load(s: &[Local; W]) -> Side {
        Side {
            v: v_of(|l| s[l].v),
            w: v_of(|l| s[l].w),
            dp: v_of(|l| s[l].dp),
            q: v_of(|l| crate::math::Vec3::new(s[l].dq.x, s[l].dq.y, s[l].dq.z)),
            qw: map(|l| s[l].dq.w),
            inv_mass: v_of(|l| s[l].inv_mass),
        }
    }

    fn store(&self, s: &mut [Local; W]) {
        for (l, side) in s.iter_mut().enumerate() {
            side.v = crate::math::Vec3::new(self.v.x[l], self.v.y[l], self.v.z[l]);
            side.w = crate::math::Vec3::new(self.w.x[l], self.w.y[l], self.w.z[l]);
        }
    }

    /// `dq.rotate(r)`, as [`crate::math::Quat::rotate`] computes it.
    #[inline(always)]
    fn rotate(&self, r: V) -> V {
        let t = self.q.cross(r).scale([2.0; W]);
        r.add(t.scale(self.qw)).add(self.q.cross(t))
    }
}

/// One row of four lanes.
struct RowW {
    dir: V,
    rxa: V,
    rxb: V,
    aa: V,
    ab: V,
    mass: F,
}

impl RowW {
    #[inline(always)]
    fn velocity(&self, a: &Side, b: &Side) -> F {
        sub(add(self.dir.dot(b.v.sub(a.v)), self.rxb.dot(b.w)), self.rxa.dot(a.w))
    }

    #[inline(always)]
    fn apply(&self, l: F, a: &mut Side, b: &mut Side) {
        let p = self.dir.scale(l);
        a.v = a.v.sub(a.inv_mass.mul(p));
        a.w = a.w.sub(self.aa.scale(l));
        b.v = b.v.add(b.inv_mass.mul(p));
        b.w = b.w.add(self.ab.scale(l));
    }
}

fn row(p: &[Option<&SolverPoint>; W], pick: impl Fn(&SolverPoint) -> &crate::solver::Row) -> RowW {
    let zero = crate::solver::Row::default();
    let r: [&crate::solver::Row; W] = std::array::from_fn(|l| p[l].map_or(&zero, &pick));
    // `zero` lives to the end of this function; the lanes are copied out before then.
    RowW {
        dir: v_of(|l| r[l].dir),
        rxa: v_of(|l| r[l].rxa),
        rxb: v_of(|l| r[l].rxb),
        aa: v_of(|l| r[l].aa),
        ab: v_of(|l| r[l].ab),
        mass: map(|l| r[l].mass),
    }
}

/// [`crate::solver::solve_one`] for up to four constraints of one colour group at once.
/// `pts[l]` and `imps[l]` are lane `l`'s points and impulses (`imps[l]` holds as many as it
/// has points); `a`, `b` its sides. A lane with no points changes nothing.
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
pub(crate) fn solve4(cons: &[&Constraint; W], pts: &[&[SolverPoint]; W], imps: &mut [[Imp; 4]; W], a: &mut [Local; W], b: &mut [Local; W], inv_h: f32, push_speed: f32, use_bias: bool) {
    let (mut sa, mut sb) = (Side::load(a), Side::load(b));
    let dp = sb.dp.sub(sa.dp);
    let most = pts.iter().map(|p| p.len()).max().unwrap_or(0);
    let bias_rate = map(|l| cons[l].soft.bias_rate);
    let mass_scale = map(|l| cons[l].soft.mass_scale);
    let impulse_scale = map(|l| cons[l].soft.impulse_scale);
    let friction = map(|l| cons[l].friction);
    for k in 0..most {
        let p: [Option<&SolverPoint>; W] = std::array::from_fn(|l| pts[l].get(k));
        let on: [bool; W] = std::array::from_fn(|l| p[l].is_some());
        let ra = v_of(|l| p[l].map_or(crate::math::Vec3::ZERO, |p| p.ra));
        let rb = v_of(|l| p[l].map_or(crate::math::Vec3::ZERO, |p| p.rb));
        let normal = v_of(|l| p[l].map_or(crate::math::Vec3::ZERO, |p| p.normal));
        let adj = map(|l| p[l].map_or(0.0, |p| p.adjusted_separation));
        let n = row(&p, |p| &p.n);
        let pra = sa.rotate(ra);
        let prb = sb.rotate(rb);
        let s = add(dp.add(prb.sub(pra)).dot(normal), adj);
        let mut bias = [0.0; W];
        let mut ms = [1.0; W];
        let mut is = [0.0; W];
        for l in 0..W {
            if s[l] > 0.0 {
                bias[l] = s[l] * inv_h;
            } else if use_bias {
                (bias[l], ms[l], is[l]) = ((bias_rate[l] * s[l]).max(-push_speed), mass_scale[l], impulse_scale[l]);
            }
        }
        let vn = n.velocity(&sa, &sb);
        let mut imp = [0.0; W];
        for l in 0..W {
            if on[l] {
                let old = imps[l][k].normal;
                let mut impulse = -n.mass[l] * ms[l] * (vn[l] + bias[l]) - is[l] * old;
                let new = (old + impulse).max(0.0);
                impulse = new - old;
                imps[l][k].normal = new;
                imps[l][k].max_normal = imps[l][k].max_normal.max(impulse);
                imp[l] = impulse;
            }
        }
        n.apply(imp, &mut sa, &mut sb);
    }
    // Friction: a disc of radius friction x normal impulse in the tangent plane.
    for k in 0..most {
        let p: [Option<&SolverPoint>; W] = std::array::from_fn(|l| pts[l].get(k));
        let t1 = row(&p, |p| &p.t1);
        let t2 = row(&p, |p| &p.t2);
        let v1 = t1.velocity(&sa, &sb);
        let v2 = t2.velocity(&sa, &sb);
        let (mut d1, mut d2) = ([0.0; W], [0.0; W]);
        for l in 0..W {
            if p[l].is_some() {
                let l1 = -t1.mass[l] * v1[l];
                let l2 = -t2.mass[l] * v2[l];
                let [old1, old2] = imps[l][k].tangent;
                let (mut n1, mut n2) = (old1 + l1, old2 + l2);
                let max = friction[l] * imps[l][k].normal;
                let len2 = n1 * n1 + n2 * n2;
                if len2 > max * max {
                    let s = if len2 > 0.0 { max / len2.sqrt() } else { 0.0 };
                    n1 *= s;
                    n2 *= s;
                }
                imps[l][k].tangent = [n1, n2];
                (d1[l], d2[l]) = (n1 - old1, n2 - old2);
            }
        }
        t1.apply(d1, &mut sa, &mut sb);
        t2.apply(d2, &mut sa, &mut sb);
    }
    sa.store(a);
    sb.store(b);
}

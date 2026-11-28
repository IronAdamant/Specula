//! The statements of DISTANCE_FIELDS.md section 7, held by sampling. Every test is seeded and
//! deterministic. The reference in `f64` below is written apart from the module's code: it
//! reads the numbers a part stores and evaluates 7.3's and 7.4's formulas from them.

use super::*;
use specula_core::json::Json;

// ---------------------------------------------------------------------------------------
// Seeded numbers.

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.unit()
    }
    fn log(&mut self, a: f64, b: f64) -> f64 {
        dmath::exp_f64(self.range(dmath::ln_f64(a), dmath::ln_f64(b)))
    }
    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn dir(&mut self) -> D3 {
        loop {
            let v = [self.range(-1.0, 1.0), self.range(-1.0, 1.0), self.range(-1.0, 1.0)];
            let l = dlen(v);
            if l > 0.1 && l <= 1.0 {
                return dscale(v, 1.0 / l);
            }
        }
    }
}

fn add(a: D3, b: D3) -> D3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn dist(a: V3, b: V3) -> f64 {
    dlen(dsub(d3(a), d3(b)))
}

// ---------------------------------------------------------------------------------------
// Bones and bodies to test with.

fn random_bone(r: &mut Rng) -> Bone {
    let head = [r.range(-0.5, 0.5), r.range(-0.5, 0.5), r.range(-0.5, 0.5)];
    let point = r.unit() < 0.05;
    let length = if point { 0.0 } else { r.log(0.005, 1.0) };
    let tail = add(head, dscale(r.dir(), length));
    let count = if point { 1 } else { 1 + r.pick(MAX_STATIONS) };
    let scale = r.log(0.005, 0.25);
    let mut t = r.range(-0.3, 0.3);
    let mut sections = Vec::new();
    for _ in 0..count {
        let a = (scale * r.log(0.4, 2.5)).clamp(f64::from(SIZE.0), f64::from(SIZE.1));
        let b = (scale * r.log(0.4, 2.5)).clamp(f64::from(SIZE.0), f64::from(SIZE.1));
        sections.push([t as f32, a as f32, b as f32]);
        t += r.range(0.05, (1.7 / count as f64).min(0.35));
    }
    let squares = [2.0, 2.0, 3.0, 4.0, 8.0, 2.5, 6.0, 12.0];
    let square = if r.unit() < 0.2 { r.range(2.0, 8.0) as f32 } else { squares[r.pick(squares.len())] };
    let side = (r.unit() < 0.5).then(|| f3(r.dir()));
    Bone { head: f3(head), tail: f3(tail), side, square, sections }
}

fn random_swell(r: &mut Rng, bones: usize) -> Swell {
    let s = r.log(0.005, 0.2);
    Swell {
        bone: r.pick(bones),
        at: [r.range(-0.2, 1.2) as f32, r.range(-0.1, 0.1) as f32, r.range(-0.1, 0.1) as f32],
        radii: [(s * r.log(0.4, 2.5)) as f32, (s * r.log(0.4, 2.5)) as f32, (s * r.log(0.4, 2.5)) as f32],
    }
}

/// A body of one to five random bones and up to two swells.
fn random_flesh(r: &mut Rng) -> Flesh {
    let bones: Vec<Bone> = (0..1 + r.pick(5)).map(|_| random_bone(r)).collect();
    let swells: Vec<Swell> = (0..r.pick(3)).map(|_| random_swell(r, bones.len())).collect();
    let smooth = if r.unit() < 0.1 { 0.0 } else { r.range(0.001, 0.06) as f32 };
    Flesh::new(&bones, &swells, smooth, r.range(0.005, 0.1) as f32).expect("a random body is valid")
}

const BODIES: [&str; 4] = ["biped", "quadruped", "serpent", "forked"];

/// The bones of one of the earlier body lane's four test bodies (`tests/bodies/*.json`): only
/// their heads, tails, sides, squareness and sections, and the body's smoothing.
fn test_body(name: &str) -> (Vec<Bone>, f32) {
    let (bones, _, smooth) = test_skeleton(name);
    (bones, smooth)
}

/// [`test_body`] with each bone's parent.
fn test_skeleton(name: &str) -> (Vec<Bone>, Vec<Option<usize>>, f32) {
    let text = match name {
        "biped" => include_str!("../../tests/bodies/biped.json"),
        "quadruped" => include_str!("../../tests/bodies/quadruped.json"),
        "serpent" => include_str!("../../tests/bodies/serpent.json"),
        _ => include_str!("../../tests/bodies/forked.json"),
    };
    let json = Json::parse(text).expect("the body's file is JSON");
    let shape = json.get("parts").and_then(|p| p.as_array()).and_then(|p| p.first()).and_then(|p| p.get("shape")).expect("a shape");
    let v3 = |j: &Json| -> V3 {
        let a = j.as_array().expect("three numbers");
        [a[0].as_f64().unwrap() as f32, a[1].as_f64().unwrap() as f32, a[2].as_f64().unwrap() as f32]
    };
    let rows = shape.get("bones").and_then(|b| b.as_array()).expect("bones");
    let names: Vec<&str> = rows.iter().map(|b| b.get("name").and_then(|n| n.as_str()).unwrap_or("")).collect();
    let parents = rows.iter().map(|b| b.get("parent").and_then(|p| p.as_str()).and_then(|p| names.iter().position(|n| *n == p))).collect();
    let bones = rows
        .iter()
        .map(|b| {
            let head = v3(b.get("head").unwrap());
            Bone {
                head,
                tail: b.get("tail").map(v3).unwrap_or(head),
                side: b.get("side").map(v3),
                square: b.get("square").and_then(|s| s.as_f64()).unwrap_or(2.0) as f32,
                sections: b.get("sections").and_then(|s| s.as_array()).map(|rows| rows.iter().map(v3).collect()).unwrap_or_default(),
            }
        })
        .collect();
    (bones, parents, shape.get("smooth").and_then(|s| s.as_f64()).unwrap_or(0.02) as f32)
}

fn skeleton(name: &str) -> Flesh {
    let (bones, smooth) = test_body(name);
    Flesh::new(&bones, &[], smooth, 0.04).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// [`skeleton`] with its joints (7.8).
fn jointed(name: &str) -> Flesh {
    let (bones, parents, smooth) = test_skeleton(name);
    Flesh::jointed(&bones, &[], smooth, 0.04, &joints_of(&bones, &parents)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// A random chain of two to five bones, end to end, bending by up to `most` degrees at each
/// joint (some straight on), with random sections, and swells now and then; jointed.
fn random_chain(r: &mut Rng, most: f64) -> Flesh {
    loop {
        let n = 2 + r.pick(4);
        let mut head = [r.range(-0.3, 0.3), r.range(-0.3, 0.3), r.range(-0.3, 0.3)];
        let mut dir = r.dir();
        let mut bones = Vec::new();
        let mut parents = Vec::new();
        for i in 0..n {
            let length = r.log(0.03, 0.4);
            let tail = add(head, dscale(dir, length));
            let mut b = random_bone(r);
            b.head = f3(head);
            b.tail = f3(tail);
            if r.unit() < 0.5 {
                // Sections from end to end, as a limb's are.
                let s = r.log(0.01, 0.12);
                b.sections = vec![[0.0, s as f32, (s * r.log(0.6, 1.6)) as f32], [1.0, (s * r.log(0.6, 1.6)) as f32, (s * r.log(0.6, 1.6)) as f32]];
            }
            bones.push(b);
            parents.push(i.checked_sub(1));
            head = tail;
            let bend = if r.unit() < 0.4 { 0.0 } else { r.range(0.0, most) };
            let (s, c) = dmath::sin_cos_f64(bend * dmath::PI / 180.0);
            let side = r.dir();
            let across = dsub(side, dscale(dir, ddot(side, dir)));
            if dlen(across) > 1e-3 {
                dir = add(dscale(dir, c), dscale(across, s / dlen(across)));
                dir = dscale(dir, 1.0 / dlen(dir));
            }
        }
        let swells: Vec<Swell> = (0..r.pick(3)).map(|_| random_swell(r, n)).collect();
        let smooth = r.range(0.002, 0.06) as f32;
        if let Ok(f) = Flesh::jointed(&bones, &swells, smooth, r.range(0.005, 0.1) as f32, &joints_of(&bones, &parents)) {
            return f;
        }
    }
}

/// A point near part `i`: on or about its axis, from a hair to several of its reaches away.
fn near_part(r: &mut Rng, f: &Flesh, i: usize) -> V3 {
    let (o, e, t0, t1, reach) = match &f.parts[i] {
        Part::Bone(b) => (b.origin, b.frame[0], b.t0, b.t1, b.radius),
        Part::Egg(g) => (g.origin, g.frame[0], 0.0, 0.0, g.most),
    };
    let reach = f64::from(reach);
    let t = r.range(f64::from(t0) - reach, f64::from(t1) + reach);
    let off = if r.unit() < 0.1 { r.log(reach, 10.0 * reach) } else { r.log(1e-4 * reach, 2.5 * reach) };
    f3(add(add(d3(o), dscale(d3(e), t)), dscale(r.dir(), off)))
}

fn near(r: &mut Rng, f: &Flesh) -> V3 {
    let i = r.pick(f.parts.len());
    near_part(r, f, i)
}

/// A box about every part, widened by `pad`.
fn bounds(f: &Flesh, pad: f64) -> (D3, D3) {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in &f.parts {
        let (ends, reach) = match p {
            Part::Bone(b) => {
                let e = d3(b.frame[0]);
                ([add(d3(b.origin), dscale(e, f64::from(b.t0))), add(d3(b.origin), dscale(e, f64::from(b.t1)))], f64::from(b.radius))
            }
            Part::Egg(g) => ([d3(g.origin), d3(g.origin)], f64::from(g.most)),
        };
        for end in ends {
            for c in 0..3 {
                lo[c] = lo[c].min(end[c] - reach - pad);
                hi[c] = hi[c].max(end[c] + reach + pad);
            }
        }
    }
    (lo, hi)
}

fn in_box(r: &mut Rng, (lo, hi): (D3, D3)) -> V3 {
    f3([r.range(lo[0], hi[0]), r.range(lo[1], hi[1]), r.range(lo[2], hi[2])])
}

// ---------------------------------------------------------------------------------------
// The reference in f64.

struct Ref {
    psi: f64,
    field: f64,
}

fn ref_sizes(b: &Shaft, tau: f64) -> (f64, f64) {
    let n = b.count;
    let p = &b.pieces;
    if n == 1 {
        let a = f64::from(p[0].a[0]);
        return (a, f64::from(p[0].q[0]) * a);
    }
    let st: Vec<f64> = b.stations[..n].iter().map(|&s| f64::from(s)).collect();
    let mut i = 0;
    while i + 2 < n && tau > st[i + 1] {
        i += 1;
    }
    let f = ((tau - st[i]) / (st[i + 1] - st[i])).clamp(0.0, 1.0);
    let cubic = |c: [f32; 4]| {
        let [y0, y1, m0, m1] = c.map(f64::from);
        let g = 1.0 - f;
        (1.0 + 2.0 * f) * g * g * y0 + f * f * (3.0 - 2.0 * f) * y1 + f * g * g * m0 - f * f * g * m1
    };
    let a = cubic(p[i].a);
    (a, cubic(p[i].q) * a)
}

fn ref_gauge(p: f64, a: f64, b: f64) -> f64 {
    if p == 2.0 {
        return (a * a + b * b).sqrt();
    }
    let w = a.max(b);
    if w == 0.0 {
        return 0.0;
    }
    let r = a.min(b) / w;
    let rp = if r > 0.0 { dmath::exp_f64(p * dmath::ln_f64(r)) } else { 0.0 };
    w * dmath::exp_f64(dmath::ln_f64(1.0 + rp) / p)
}

fn ref_part(part: &Part, x: D3) -> Ref {
    let (o, fr) = match part {
        Part::Bone(b) => (b.origin, b.frame),
        Part::Egg(e) => (e.origin, e.frame),
    };
    let d = dsub(x, d3(o));
    let l = [ddot(d, d3(fr[0])), ddot(d, d3(fr[1])), ddot(d, d3(fr[2]))];
    match part {
        Part::Bone(b) => {
            let tau = l[0].clamp(f64::from(b.t0), f64::from(b.t1));
            let (a, bb) = ref_sizes(b, tau);
            let m = a.min(bb);
            let e = (l[0] - tau) / m;
            let p = match b.gauge {
                Gauge::Two => 2.0,
                Gauge::Four => 4.0,
                Gauge::Eight => 8.0,
                Gauge::Power(p, _) => f64::from(p),
            };
            let ns = ref_gauge(p, l[1].abs() / a, l[2].abs() / bb);
            let n = (e * e + ns * ns).sqrt();
            let psi = m * (n.min(2.0) - 1.0);
            let along = l[0] - tau;
            let c = (along * along + l[1] * l[1] + l[2] * l[2]).sqrt() - f64::from(b.radius);
            Ref { psi, field: (psi / f64::from(b.lip)).max(c) }
        }
        Part::Egg(g) => {
            let k0 = dlen([l[0] * f64::from(g.inv[0]), l[1] * f64::from(g.inv[1]), l[2] * f64::from(g.inv[2])]);
            let c = dlen(l) - f64::from(g.most);
            let least = f64::from(g.least);
            Ref { psi: least * (k0 - 1.0), field: (least * (k0 - 1.0)).max(c) }
        }
    }
}

fn ref_join(a: f64, b: f64, k: f64) -> f64 {
    let m = a.min(b);
    if k <= 0.0 {
        return m;
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    m - k * h * h / 4.0
}

fn ref_whole(f: &Flesh, x: D3) -> f64 {
    let mut v: Vec<f64> = f.parts.iter().map(|p| ref_part(p, x).field).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let k = f64::from(f.smooth);
    let g = v[1..].iter().fold(v[0], |r, &y| ref_join(r, y, k));
    g.min(f64::from(f.keep))
}

/// A Flesh of one part only, to ask that part's own field.
fn one(r: &mut Rng) -> Flesh {
    loop {
        let bones = vec![random_bone(r)];
        let swells = if r.unit() < 0.15 { vec![random_swell(r, 1)] } else { vec![] };
        let bones = if swells.is_empty() { bones } else { vec![Bone { sections: vec![], ..bones[0].clone() }] };
        if let Ok(f) = Flesh::new(&bones, &swells, 0.0, 0.05) {
            return f;
        }
    }
}

// ---------------------------------------------------------------------------------------
// 7.1 to 7.4: one part.

/// 7.3 (1) and (2), 7.4: a million pairs of points about random bones (sections changing in
/// size and in proportion, squareness 2 to 12, thin and fat, short and long) and swells. In
/// `f64` the reference's `ψ` changes by at most `L` and its field by at most 1 a metre; in
/// `f32` the module's field changes by at most the distance and the two points' slack.
#[test]
fn every_part_changes_no_faster_than_its_bound() {
    let mut r = Rng(0xCA20_0001);
    let (mut pairs, mut worst32, mut worst64, mut worst_psi) = (0u32, 0.0f64, 0.0f64, 0.0f64);
    while pairs < 1_000_000 {
        let f = one(&mut r);
        let lip = f64::from(f.part_lip(0));
        for _ in 0..500 {
            let x = near_part(&mut r, &f, 0);
            let y = f3(add(d3(x), dscale(r.dir(), r.log(1e-3, 0.3))));
            let d = dist(x, y);
            let (rx, ry) = (ref_part(&f.parts[0], d3(x)), ref_part(&f.parts[0], d3(y)));
            assert!((rx.field - ry.field).abs() <= d * (1.0 + 1e-6) + 1e-12, "f64 field: {} over {d}", (rx.field - ry.field).abs());
            assert!((rx.psi - ry.psi).abs() <= lip * d * (1.0 + 1e-6) + 1e-12, "f64 psi: {} over {d}, L {lip}", (rx.psi - ry.psi).abs());
            let (fx, fy) = (f.part_field(0, x), f.part_field(0, y));
            let allowed = d * (1.0 + 1e-6) + f64::from(f.slack(x) + f.slack(y));
            let change = f64::from((fx - fy).abs());
            assert!(change <= allowed, "f32 field changed {change} over {d} (allowed {allowed}) at {x:?}, {y:?}");
            worst32 = worst32.max(change / allowed);
            worst64 = worst64.max((rx.field - ry.field).abs() / d);
            worst_psi = worst_psi.max((rx.psi - ry.psi).abs() / d / lip);
            pairs += 1;
        }
    }
    assert!(worst64 > 0.9, "the pairs reach steep places ({worst64})");
    assert!(worst_psi > 0.5 && worst32 > 0.5, "{worst_psi} {worst32}");
}

/// The surface of a part, sampled densely: its sections along the shaft and its caps.
fn surface(b: &Shaft) -> Vec<D3> {
    let p = match b.gauge {
        Gauge::Two => 2.0,
        Gauge::Four => 4.0,
        Gauge::Eight => 8.0,
        Gauge::Power(p, _) => f64::from(p),
    };
    let pow = |c: f64, e: f64| if c == 0.0 { 0.0 } else { c.signum() * dmath::exp_f64(e * dmath::ln_f64(c.abs())) };
    let ring = |a: f64, bb: f64, k: usize, of: usize| {
        let (s, c) = dmath::sin_cos_f64(2.0 * dmath::PI * k as f64 / of as f64);
        (a * pow(c, 2.0 / p), bb * pow(s, 2.0 / p))
    };
    let (o, fr) = (d3(b.origin), b.frame.map(d3));
    let at = |t: f64, u: f64, v: f64| add(add(add(o, dscale(fr[0], t)), dscale(fr[1], u)), dscale(fr[2], v));
    let (t0, t1) = (f64::from(b.t0), f64::from(b.t1));
    let mut out = Vec::new();
    const ROUND: usize = 128;
    for i in 0..=96 {
        let t = t0 + (t1 - t0) * i as f64 / 96.0;
        let (a, bb) = ref_sizes(b, t);
        for k in 0..ROUND {
            let (u, v) = ring(a, bb, k, ROUND);
            out.push(at(t, u, v));
        }
    }
    for (end, sign) in [(t0, -1.0), (t1, 1.0)] {
        let (a, bb) = ref_sizes(b, end);
        let m = a.min(bb);
        for j in 1..=24 {
            let (s, c) = dmath::sin_cos_f64(0.5 * dmath::PI * j as f64 / 24.0);
            for k in 0..ROUND {
                let (u, v) = ring(a, bb, k, ROUND);
                out.push(at(end + sign * m * s, c * u, c * v));
            }
        }
    }
    out
}

/// 7.3 (3): a bone's field never exceeds the distance to its surface, outside or inside,
/// against the nearest of its surface's points sampled densely (which is never nearer than
/// the surface). Also says how short it reads (measured, not asserted).
#[test]
fn a_bones_field_never_exceeds_the_distance_to_its_surface() {
    let mut r = Rng(0xCA20_0002);
    let (mut checked, mut inside, mut least_ratio) = (0, 0, f64::INFINITY);
    for _ in 0..200 {
        let f = loop {
            let f = one(&mut r);
            if matches!(f.parts[0], Part::Bone(_)) {
                break f;
            }
        };
        let Part::Bone(b) = &f.parts[0] else { unreachable!() };
        let pts = surface(b);
        for _ in 0..25 {
            let x = near_part(&mut r, &f, 0);
            let xd = d3(x);
            let d = pts.iter().map(|p| dlen(dsub(*p, xd))).fold(f64::INFINITY, f64::min);
            let v = f64::from(f.part_field(0, x));
            assert!(v.abs() <= d + f64::from(f.slack(x)), "field {v} where the surface is at most {d} away ({x:?})");
            if v < 0.0 {
                inside += 1;
            } else if d > 1e-3 * f64::from(b.radius) {
                least_ratio = least_ratio.min(v / d);
            }
            checked += 1;
        }
    }
    assert!(checked == 5000 && inside > 200, "{inside} of {checked} inside");
    assert!(least_ratio > 0.0, "outside, the field is above nothing ({least_ratio})");
}

/// 7.3 (3) and its words: the zero set is exactly the sections (along each station's two
/// axes, at the middle of each piece against the profile) and the caps (along the axis past
/// each end, at `m` of that end), to within `4 L ρ σ` in `f32`.
#[test]
fn a_bones_surface_is_its_sections_and_caps() {
    let mut r = Rng(0xCA20_0003);
    let mut worst = 0.0f64;
    for _ in 0..400 {
        let f = loop {
            let f = one(&mut r);
            if matches!(f.parts[0], Part::Bone(_)) {
                break f;
            }
        };
        let Part::Bone(b) = &f.parts[0] else { unreachable!() };
        let (o, fr) = (d3(b.origin), b.frame.map(d3));
        let at = |t: f64, u: f64, v: f64| f3(add(add(add(o, dscale(fr[0], t)), dscale(fr[1], u)), dscale(fr[2], v)));
        // The zero of the field along a line from a point inside, out to `far`.
        let zero = |from: (f64, f64, f64), dir: (f64, f64, f64), far: f64| {
            let (mut lo, mut hi) = (0.0f64, far);
            let point = |s: f64| at(from.0 + s * dir.0, from.1 + s * dir.1, from.2 + s * dir.2);
            assert!(f.part_field(0, point(lo)) < 0.0 && f.part_field(0, point(hi)) > 0.0);
            for _ in 0..80 {
                let mid = 0.5 * (lo + hi);
                if f.part_field(0, point(mid)) <= 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            0.5 * (lo + hi)
        };
        let lip = f64::from(b.lip);
        let tol = |a: f64, bb: f64, x: V3| 4.0 * lip * (a.max(bb) / a.min(bb)).max(1.4143) * f64::from(f.slack(x)) + 1e-6 * a.max(bb);
        let far = 2.0 * f64::from(b.radius) + 0.01;
        let mut stations: Vec<f64> = b.stations[..b.count].iter().map(|&s| f64::from(s)).collect();
        let middles: Vec<f64> = stations.windows(2).map(|w| 0.5 * (w[0] + w[1])).collect();
        stations.extend(middles);
        for t in stations {
            let (a, bb) = ref_sizes(b, t);
            let u = zero((t, 0.0, 0.0), (0.0, 1.0, 0.0), far);
            let v = zero((t, 0.0, 0.0), (0.0, 0.0, 1.0), far);
            let x = at(t, a, 0.0);
            assert!((u - a).abs() <= tol(a, bb, x) && (v - bb).abs() <= tol(a, bb, x), "at {t}: {u} for {a}, {v} for {bb}");
            worst = worst.max((u - a).abs()).max((v - bb).abs());
        }
        for (end, sign) in [(f64::from(b.t0), -1.0), (f64::from(b.t1), 1.0)] {
            let (a, bb) = ref_sizes(b, end);
            let m = a.min(bb);
            let cap = zero((end, 0.0, 0.0), (sign, 0.0, 0.0), far + m);
            assert!((cap - m).abs() <= tol(a, bb, at(end, 0.0, 0.0)), "cap {cap} for {m}");
        }
    }
    assert!(worst < 1e-4, "the sections are met to within {worst} m");
}

/// 7.6, "In `f32`": the module's value is within its slack of the reference's, for single
/// parts, random bodies and the four skeletons.
#[test]
fn every_value_is_within_its_slack_of_the_reference() {
    let mut r = Rng(0xCA20_0004);
    let mut worst = 0.0f64;
    let check = |f: &Flesh, x: V3, worst: &mut f64| {
        let (v, e) = (f64::from(f.value(x)), ref_whole(f, d3(x)));
        let s = f64::from(f.slack(x));
        assert!((v - e).abs() <= s, "{v} against {e} (slack {s}) at {x:?}");
        *worst = worst.max((v - e).abs() / s);
    };
    for _ in 0..1000 {
        let f = random_flesh(&mut r);
        for _ in 0..100 {
            let x = near(&mut r, &f);
            check(&f, x, &mut worst);
        }
    }
    for name in BODIES {
        let f = skeleton(name);
        for _ in 0..25_000 {
            let x = near(&mut r, &f);
            check(&f, x, &mut worst);
        }
    }
    assert!(worst < 0.5, "the slack has room to spare: {worst}");
}

// ---------------------------------------------------------------------------------------
// 7.5: the joined body.

/// The exact signed distance to a round capsule.
fn capsule(x: D3, a: D3, b: D3, radius: f64) -> f64 {
    let ab = dsub(b, a);
    let t = (ddot(dsub(x, a), ab) / ddot(ab, ab)).clamp(0.0, 1.0);
    dlen(dsub(x, add(a, dscale(ab, t)))) - radius
}

/// 7.5 (1) and (4): two round bones of one radius meeting at a joint at 0 to 150 degrees,
/// joined with a range of widths. The joined field is 1-Lipschitz, and its surface lies within
/// `K/4` of the plain union of the two (exact distances), and reaches it at a straight joint.
#[test]
fn a_joint_keeps_the_bound_and_bulges_by_a_quarter_of_the_width() {
    let mut r = Rng(0xCA20_0005);
    for degrees in (0..=150).step_by(15) {
        let (s, c) = dmath::sin_cos_f64(f64::from(degrees) * dmath::PI / 180.0);
        for &radius in &[0.02f64, 0.05, 0.1] {
            for &width in &[0.005f64, 0.01, 0.02, 0.04] {
                if width / 4.0 >= radius {
                    continue;
                }
                let (a, j) = ([0.0, -0.4, 0.0], [0.0, 0.0, 0.0]);
                let b = [s * 0.4, c * 0.4, 0.0];
                let ring = |at: f32| vec![[at, radius as f32, radius as f32]];
                let bones = vec![
                    Bone { head: f3(a), tail: f3(j), side: None, square: 2.0, sections: ring(0.0).into_iter().chain(ring(1.0)).collect() },
                    Bone { head: f3(j), tail: f3(b), side: None, square: 2.0, sections: ring(0.0).into_iter().chain(ring(1.0)).collect() },
                ];
                let f = Flesh::new(&bones, &[], width as f32, 0.05).unwrap();
                // 1-Lipschitz.
                for _ in 0..2000 {
                    let x = near(&mut r, &f);
                    let y = f3(add(d3(x), dscale(r.dir(), r.log(1e-3, 0.2))));
                    let d = dist(x, y);
                    let change = f64::from((f.value(x) - f.value(y)).abs());
                    assert!(change <= d * (1.0 + 1e-6) + f64::from(f.slack(x) + f.slack(y)), "{degrees} deg: {change} over {d}");
                }
                // The bulge: along rays from the joint, where the joined surface is, against
                // the plain union.
                let mut bulge = 0.0f64;
                for _ in 0..3000 {
                    let dir = r.dir();
                    let p = |t: f64| f3(dscale(dir, t));
                    let mut t = 0.0;
                    while f.value(p(t)) <= 0.0 && t < 1.0 {
                        t += radius / 8.0;
                    }
                    let (mut lo, mut hi) = (t - radius / 8.0, t);
                    for _ in 0..60 {
                        let mid = 0.5 * (lo + hi);
                        if f.value(p(mid)) <= 0.0 {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    let q = d3(p(0.5 * (lo + hi)));
                    let union = capsule(q, a, j, radius).min(capsule(q, j, b, radius));
                    bulge = bulge.max(union);
                }
                let bound = width / 4.0;
                assert!(bulge <= bound * (1.0 + 1e-3) + 1e-5, "{degrees} deg, r {radius}, K {width}: bulge {bulge} over {bound}");
                assert!(bulge >= 0.9 * bound, "{degrees} deg: the joint bulges by K/4 where its fields are equal: {bulge} of {bound}");
            }
        }
    }
}

/// 7.7: the quadratic join changes by at most the greater change of its two values plus a
/// quarter of the change of its width, over a million pairs of inputs, near and far apart;
/// and the quarter is reached (two equal values: the join is `a − K/4`).
#[test]
fn a_join_of_changing_width_changes_by_a_quarter_of_it() {
    let mut r = Rng(0xCA20_000A);
    for _ in 0..1_000_000 {
        let k = r.log(1e-4, 1.0);
        let a = r.range(-1.0, 1.0);
        let b = if r.unit() < 0.5 { a + r.range(-k, k) } else { r.range(-1.0, 1.0) };
        let step = r.log(1e-9, 1.0);
        let (da, db, dk) = (step * r.range(-1.0, 1.0), step * r.range(-1.0, 1.0), step * r.range(-1.0, 1.0));
        let k2 = (k + dk).max(1e-5);
        let change = (ref_join(a + da, b + db, k2) - ref_join(a, b, k)).abs();
        let bound = da.abs().max(db.abs()) + (k2 - k).abs() / 4.0;
        assert!(change <= bound * (1.0 + 1e-9) + 1e-14, "{change} over {bound}");
    }
    let (k, dk) = (0.1, 1e-6);
    let reached = (ref_join(0.5, 0.5, k + dk) - ref_join(0.5, 0.5, k)).abs() / (dk / 4.0);
    assert!(reached > 0.999, "the quarter is reached: {reached}");
}

// ---------------------------------------------------------------------------------------
// 7.8: the width that falls at a straight joint.

/// 7.8's width in `f64`, from the numbers each seam stores.
fn ref_width(f: &Flesh, x: D3) -> f64 {
    let mut k = f64::from(f.smooth);
    for s in &f.seams {
        let d = dsub(x, d3(s.centre));
        let z = ddot(d, d3(s.normal));
        let rho = (ddot(d, d) - z * z).max(0.0).sqrt();
        let (dz, dr) = ((z.abs() - f64::from(s.half)).max(0.0), (rho - f64::from(s.radius)).max(0.0));
        k = k.min(f64::from(s.width) + f64::from(WIDTH_SLOPE) * (dz * dz + dr * dr).sqrt());
    }
    k
}

/// 7.8's field in `f64`.
fn ref_jointed(f: &Flesh, x: D3) -> f64 {
    let mut v: Vec<f64> = f.parts.iter().map(|p| ref_part(p, x).field).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let k = ref_width(f, x);
    let g = v[1..].iter().fold(v[0], |r, &y| ref_join(r, y, k));
    g.min(f64::from(f.keep)) / f64::from(f.div)
}

/// Where the joined surface is along the ray from `from` toward `dir`, found by stepping out
/// from inside and halving.
fn surface_on(f: &Flesh, from: D3, dir: D3, step: f64) -> D3 {
    let p = |t: f64| f3(add(from, dscale(dir, t)));
    let mut t = 0.0;
    while f.value(p(t)) <= 0.0 && t < 2.0 {
        t += step;
    }
    let (mut lo, mut hi) = ((t - step).max(0.0), t);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if f.value(p(mid)) <= 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    d3(p(0.5 * (lo + hi)))
}

/// 7.8: the share of the width a joint takes runs smoothly from nothing straight on to all of
/// it at 40 degrees, and a chain straight on has no width at its joints, a bent one the body's.
#[test]
fn the_width_falls_to_nothing_as_two_bones_come_into_line() {
    let mut last = -1.0;
    for i in 0..=1000 {
        let deg = 0.09 * f64::from(i);
        let (_, c) = dmath::sin_cos_f64(deg * dmath::PI / 180.0);
        let s = bend_share(c);
        assert!(s >= last && (0.0..=1.0).contains(&s), "{deg}: {s}");
        last = s;
    }
    assert_eq!(bend_share(1.0), 0.0);
    let (_, c40) = dmath::sin_cos_f64(40.0 * dmath::PI / 180.0);
    assert!(bend_share(c40) > 0.999_999);
    let round = |at: f32| [at, 0.05, 0.05];
    let line = |bend: f64| {
        let (s, c) = dmath::sin_cos_f64(bend * dmath::PI / 180.0);
        let bones = vec![
            Bone { head: [0.0, -0.3, 0.0], tail: [0.0; 3], side: None, square: 2.0, sections: vec![round(0.0), round(1.0)] },
            Bone { head: [0.0; 3], tail: [(0.3 * s) as f32, (0.3 * c) as f32, 0.0], side: None, square: 2.0, sections: vec![round(0.0), round(1.0)] },
        ];
        Flesh::jointed(&bones, &[], 0.02, 0.05, &joints_of(&bones, &[None, Some(0)])).unwrap()
    };
    let straight = line(0.0);
    assert_eq!((straight.seams(), straight.divisor()), (1, 1.0 + WIDTH_SLOPE));
    assert_eq!(straight.width_at([0.05, 0.0, 0.0]), 0.0, "on the ring the width is nothing");
    assert!((straight.width_at([0.0, 0.25, 0.0]) - 0.02).abs() < 1e-7, "far along, the body's");
    let bent = line(90.0);
    assert_eq!((bent.seams(), bent.divisor()), (0, 1.0), "a joint bent past 40 degrees keeps the body's width");
}

/// 7.8: chains of round bones of one size run straight on through two joints have no ring:
/// the joined surface lies within a thousandth of the bones' radius of their plain union
/// (exact distances), against `K/4` for the width that does not fall (7.5 (4)); and a joint
/// bent a degree or two keeps under a fiftieth of `K/4`.
#[test]
fn a_straight_joint_has_no_ring() {
    let mut r = Rng(0xCA20_0010);
    let mut worst = [0.0f64; 3];
    for (b, &bend) in [0.0f64, 1.0, 2.0].iter().enumerate() {
        for &radius in &[0.02f64, 0.05, 0.1] {
            for &width in &[0.005f64, 0.01, 0.02, 0.04] {
                let (s, c) = dmath::sin_cos_f64(bend * dmath::PI / 180.0);
                let pts = [[0.0, -0.5, 0.0], [0.0, -0.15, 0.0], [0.0, 0.15, 0.0], [0.3 * s, 0.15 + 0.3 * c, 0.0]];
                let ring = |at: f32| [at, radius as f32, radius as f32];
                let bones: Vec<Bone> = (0..3).map(|i| Bone { head: f3(pts[i]), tail: f3(pts[i + 1]), side: None, square: 2.0, sections: vec![ring(0.0), ring(1.0)] }).collect();
                let f = Flesh::jointed(&bones, &[], width as f32, 0.05, &joints_of(&bones, &[None, Some(0), Some(1)])).unwrap();
                let mut bulge = 0.0f64;
                for _ in 0..1500 {
                    // From the axis near a joint, out in every direction.
                    let joint = pts[1 + r.pick(2)];
                    let from = add(joint, [0.0, r.range(-3.0, 3.0) * radius, 0.0]);
                    let q = surface_on(&f, from, r.dir(), radius / 8.0);
                    let union = (0..3).map(|i| capsule(q, pts[i], pts[i + 1], radius)).fold(f64::INFINITY, f64::min);
                    bulge = bulge.max(union);
                }
                worst[b] = worst[b].max(bulge / (width / 4.0));
                if bend == 0.0 {
                    assert!(bulge <= 1e-3 * radius + 1e-5, "r {radius}, K {width}: a ring of {bulge} at a straight joint");
                } else {
                    assert!(bulge <= 0.02 * width / 4.0 + 1e-3 * radius + 1e-5, "{bend} deg, r {radius}, K {width}: a ring of {bulge}");
                }
            }
        }
    }
    println!("the ring at a straight joint, as a share of K/4, at 0, 1 and 2 degrees: {worst:?}");
}

/// 7.8: two round bones of one size meeting at 0 to 150 degrees, jointed: the surface lies
/// within `K/4` of the plain union at every bend (7.5 (4) with the greatest width), the whole
/// `K/4` from 45 degrees on, and the field is 1-Lipschitz.
#[test]
fn a_bent_joint_keeps_its_bulge_under_the_bound() {
    let mut r = Rng(0xCA20_0011);
    for degrees in (0..=150).step_by(15) {
        let (s, c) = dmath::sin_cos_f64(f64::from(degrees) * dmath::PI / 180.0);
        for &radius in &[0.02f64, 0.05, 0.1] {
            for &width in &[0.005f64, 0.01, 0.02, 0.04] {
                let (a, j, b) = ([0.0, -0.4, 0.0], [0.0, 0.0, 0.0], [s * 0.4, c * 0.4, 0.0]);
                let ring = |at: f32| [at, radius as f32, radius as f32];
                let bones = vec![
                    Bone { head: f3(a), tail: f3(j), side: None, square: 2.0, sections: vec![ring(0.0), ring(1.0)] },
                    Bone { head: f3(j), tail: f3(b), side: None, square: 2.0, sections: vec![ring(0.0), ring(1.0)] },
                ];
                let f = Flesh::jointed(&bones, &[], width as f32, 0.05, &joints_of(&bones, &[None, Some(0)])).unwrap();
                for _ in 0..2000 {
                    let x = near(&mut r, &f);
                    let y = f3(add(d3(x), dscale(r.dir(), r.log(1e-3, 0.2))));
                    let d = dist(x, y);
                    let change = f64::from((f.value(x) - f.value(y)).abs());
                    assert!(change <= d * (1.0 + 1e-6) + f64::from(f.slack(x) + f.slack(y)), "{degrees} deg: {change} over {d}");
                }
                let mut bulge = 0.0f64;
                for _ in 0..3000 {
                    let q = surface_on(&f, j, r.dir(), radius / 8.0);
                    bulge = bulge.max(capsule(q, a, j, radius).min(capsule(q, j, b, radius)));
                }
                let bound = width / 4.0;
                assert!(bulge <= bound * (1.0 + 1e-3) + 1e-5, "{degrees} deg, r {radius}, K {width}: bulge {bulge} over {bound}");
                if degrees >= 45 {
                    assert!(bulge >= 0.9 * bound, "{degrees} deg: a bent joint keeps the body's width: {bulge} of {bound}");
                }
            }
        }
    }
}

/// 7.8: over a million pairs of points about random chains (straight and bent joints, swells,
/// sections of every kind) and the four skeletons jointed, the field changes by at most the
/// distance and the two points' slack, and every value is within its slack of the reference in
/// `f64` (whose width is worked out apart, from the seams' numbers).
#[test]
fn a_jointed_body_changes_no_faster_than_the_distance() {
    let mut r = Rng(0xCA20_0012);
    let (mut pairs, mut worst, mut off) = (0u32, 0.0f64, 0.0f64);
    let mut check = |f: &Flesh, r: &mut Rng, pairs: &mut u32| {
        for _ in 0..500 {
            let x = near(r, f);
            let y = f3(add(d3(x), dscale(r.dir(), r.log(1e-4, 0.3))));
            let d = dist(x, y);
            let change = f64::from((f.value(x) - f.value(y)).abs());
            let allowed = d * (1.0 + 1e-6) + f64::from(f.slack(x) + f.slack(y));
            assert!(change <= allowed, "changed {change} over {d} (allowed {allowed}) at {x:?}, {y:?}");
            worst = worst.max(change / d);
            let e = (f64::from(f.value(x)) - ref_jointed(f, d3(x))).abs();
            assert!(e <= f64::from(f.slack(x)), "{e} from the reference at {x:?}");
            off = off.max(e / f64::from(f.slack(x)));
            *pairs += 1;
        }
    };
    for name in BODIES {
        let f = jointed(name);
        assert!(f.seams() > 0, "{name} has straight joints");
        for _ in 0..100 {
            check(&f, &mut r, &mut pairs);
        }
    }
    while pairs < 1_000_000 {
        let f = random_chain(&mut r, 150.0);
        check(&f, &mut r, &mut pairs);
    }
    assert!(worst > 0.5, "the pairs reach steep places ({worst})");
    println!("steepest {worst}; worst error {off} of the slack");
}

// ---------------------------------------------------------------------------------------
// 7.6: the cull.

/// 7.6: the culled value is the whole value, the same number, over a million samples of the
/// four skeletons (in their boxes and near their bones), and over random bodies.
#[test]
fn the_culled_field_is_the_whole_field() {
    let mut r = Rng(0xCA20_0006);
    let mut among = Vec::new();
    for f in BODIES.iter().flat_map(|n| [skeleton(n), jointed(n)]) {
        let name = "a skeleton";
        let room = bounds(&f, 0.2);
        for k in 0..125_000 {
            let x = if k % 2 == 0 { in_box(&mut r, room) } else { near(&mut r, &f) };
            let (v, w) = (f.value(x), f.whole(x));
            assert!(v == w, "{name}: culled {v}, whole {w} at {x:?}");
            if k % 16 == 0 {
                f.meets(x, 0.0, &mut among);
                assert!(f.value_among(x, &among) == w);
            }
        }
    }
    for i in 0..4000 {
        let f = if i % 2 == 0 { random_flesh(&mut r) } else { random_chain(&mut r, 150.0) };
        for k in 0..100 {
            let x = near(&mut r, &f);
            let w = f.whole(x);
            assert!(f.value(x) == w);
            if k % 8 == 0 {
                f.meets(x, 0.0, &mut among);
                assert!(f.value_among(x, &among) == w);
            }
        }
    }
}

/// 7.6, "Over a ball": over a million balls (four skeletons and random bodies; centres in
/// their boxes and near their bones; radii from a hair to half a metre), two samples each,
/// one on the rim: every value lies in the ball's range, every part a sample asks is among
/// those [`Flesh::meets`] gives, and asking only those gives the same value.
#[test]
fn a_range_over_a_ball_holds_every_value_in_it() {
    let mut r = Rng(0xCA20_0007);
    let mut among = Vec::new();
    let ball = |f: &Flesh, o: V3, rad: f32, r: &mut Rng, among: &mut Vec<u16>| {
        let (lo, hi) = f.range(o, rad);
        assert!(lo <= hi, "{lo} {hi}");
        f.meets(o, rad, among);
        for k in 0..2 {
            let reach = if k == 0 { f64::from(rad) } else { f64::from(rad) * r.unit() };
            let y = f3(add(d3(o), dscale(r.dir(), reach)));
            let v = f.value(y);
            assert!(v >= lo && v <= hi, "{v} outside [{lo}, {hi}] for the ball {o:?}, {rad}");
            assert!(f.value_among(y, among) == v, "the ball's parts give the value");
        }
    };
    let mut balls = 0u32;
    for f in BODIES.iter().flat_map(|n| [skeleton(n), jointed(n)]) {
        let room = bounds(&f, 0.3);
        for k in 0..62_500 {
            let o = if k % 2 == 0 { in_box(&mut r, room) } else { near(&mut r, &f) };
            let rad = r.log(1e-4, 0.5) as f32;
            ball(&f, o, rad, &mut r, &mut among);
            balls += 1;
        }
    }
    while balls < 1_000_000 {
        let f = if balls % 200 == 0 { random_chain(&mut r, 150.0) } else { random_flesh(&mut r) };
        let room = bounds(&f, 0.3);
        for k in 0..100 {
            let o = if k % 2 == 0 { in_box(&mut r, room) } else { near(&mut r, &f) };
            let rad = r.log(1e-4, 0.5) as f32;
            ball(&f, o, rad, &mut r, &mut among);
            balls += 1;
        }
    }
}

/// A far ball's range is above nothing without asking a field: the capsules' bound prunes it.
#[test]
fn a_far_ball_is_pruned_by_the_capsules() {
    let f = skeleton("biped");
    let (lo, hi) = bounds(&f, 0.0);
    let o = f3([hi[0] + 2.0, 0.5 * (lo[1] + hi[1]), 0.0]);
    let (low, _) = f.range(o, 1.5);
    assert!(low > 0.0, "a ball 0.5 m clear of every capsule is outside: {low}");
}

// ---------------------------------------------------------------------------------------
// Bits, refusals.

/// The same bits in debug and in release: a fixed set of samples, hashed.
#[test]
fn the_same_bits_in_every_build() {
    let mut r = Rng(0xCA20_0008);
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut eat = |v: f32| {
        hash ^= u64::from(v.to_bits());
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    };
    for name in BODIES {
        let f = skeleton(name);
        let room = bounds(&f, 0.1);
        for _ in 0..2500 {
            let x = in_box(&mut r, room);
            eat(f.value(x));
            let (lo, hi) = f.range(x, 0.01);
            eat(lo);
            eat(hi);
        }
    }
    for _ in 0..200 {
        let f = random_flesh(&mut r);
        for _ in 0..20 {
            eat(f.value(near(&mut r, &f)));
        }
    }
    assert_eq!(hash, BITS, "the samples' bits changed: {hash:#x}");
}

/// What [`the_same_bits_in_every_build`] read in a debug build and in a release build.
const BITS: u64 = 0xd499_f5c9_4424_8f80;

/// Every refusal names what failed.
#[test]
fn hostile_bodies_are_refused_with_a_reason() {
    let good = Bone { head: [0.0; 3], tail: [0.0, 1.0, 0.0], side: None, square: 2.0, sections: vec![[0.0, 0.1, 0.1], [1.0, 0.05, 0.05]] };
    let code = |bones: &[Bone], swells: &[Swell], smooth: f32, keep: f32| Flesh::new(bones, swells, smooth, keep).err().map(|e| e.code);
    assert!(code(std::slice::from_ref(&good), &[], 0.02, 0.04).is_none());
    let with = |f: &dyn Fn(&mut Bone)| {
        let mut b = good.clone();
        f(&mut b);
        code(&[b], &[], 0.02, 0.04)
    };
    assert_eq!(with(&|b| b.square = 1.5), Some("square"));
    assert_eq!(with(&|b| b.square = f32::NAN), Some("square"));
    assert_eq!(with(&|b| b.head[0] = f32::INFINITY), Some("bone_extent"));
    assert_eq!(with(&|b| b.tail[1] = 2.0 * EXTENT), Some("bone_extent"));
    assert_eq!(with(&|b| b.sections[1][1] = 0.0), Some("section_size"));
    assert_eq!(with(&|b| b.sections[1][2] = f32::NAN), Some("section_size"));
    assert_eq!(with(&|b| b.sections[1][0] = 0.0), Some("section_order"));
    assert_eq!(with(&|b| b.sections[1][0] = 3.0), Some("section_along"));
    assert_eq!(with(&|b| b.sections = vec![[0.0, 0.1, 0.1]; 9]), Some("bone_sections"));
    assert_eq!(with(&|b| b.tail = b.head), Some("point_bone_sections"));
    assert_eq!(code(std::slice::from_ref(&good), &[], -0.1, 0.04), Some("smooth"));
    assert_eq!(code(std::slice::from_ref(&good), &[], 0.02, 0.0), Some("keep"));
    assert_eq!(code(&[], &[], 0.02, 0.04), Some("no_flesh"));
    assert_eq!(code(&vec![good.clone(); MAX_PARTS + 1], &[], 0.02, 0.04), Some("too_many_parts"));
    let swell = Swell { bone: 0, at: [0.5, 0.0, 0.0], radii: [0.05, 0.05, 0.05] };
    assert!(code(std::slice::from_ref(&good), std::slice::from_ref(&swell), 0.02, 0.04).is_none());
    assert_eq!(code(std::slice::from_ref(&good), &[Swell { bone: 3, ..swell.clone() }], 0.02, 0.04), Some("swell_bone"));
    assert_eq!(code(std::slice::from_ref(&good), &[Swell { radii: [0.05, 9.0, 0.05], ..swell.clone() }], 0.02, 0.04), Some("swell_size"));
    assert_eq!(code(std::slice::from_ref(&good), &[Swell { at: [0.5, 6.0, 0.0], ..swell }], 0.02, 0.04), Some("swell_offset"));
}

// ---------------------------------------------------------------------------------------
// What it costs (measured, not asserted: rule 20 of the lanes).

/// How many parts a sample asks, with the cull and without, over each skeleton's box; and
/// how long a sample takes in this build. `cargo test -p specula-model --release --lib
/// caro::tests::what_a_sample_costs -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement: run in release with --nocapture"]
#[allow(clippy::disallowed_methods, reason = "a measurement of this build's speed, outside every result (rule 20 of the lanes: timed from outside the code timed)")]
fn what_a_sample_costs() {
    let mut r = Rng(0xCA20_0009);
    let mut among = Vec::new();
    for name in BODIES {
        let f = skeleton(name);
        let room = bounds(&f, 0.05);
        let points: Vec<V3> = (0..200_000).map(|_| in_box(&mut r, room)).collect();
        let mut asked: Vec<usize> = points
            .iter()
            .map(|&x| {
                f.meets(x, 0.0, &mut among);
                among.len()
            })
            .collect();
        asked.sort_unstable();
        let mean = asked.iter().sum::<usize>() as f64 / asked.len() as f64;
        // A block of 8 cm (a sample's list from its block's `meets`).
        let block = 0.04f32;
        let mut block_asked = Vec::new();
        let time = |what: &dyn Fn(V3) -> f32| {
            let start = std::time::Instant::now();
            let mut sum = 0.0f32;
            for &x in &points {
                sum += what(x);
            }
            (start.elapsed().as_nanos() as f64 / points.len() as f64, sum)
        };
        let (culled, s1) = time(&|x| f.value(x));
        let (whole, s2) = time(&|x| f.whole(x));
        // Samples grouped by blocks: each block asks `meets` once, its samples `value_among`.
        let start = std::time::Instant::now();
        let mut sum3 = 0.0f32;
        let mut done = 0usize;
        for &o in points.iter().step_by(64) {
            f.meets(o, block * 1.732_051, &mut among);
            block_asked.push(among.len());
            for k in 0..64 {
                let x = [o[0] + block * ((k & 3) as f32 / 2.0 - 0.75), o[1] + block * (((k >> 2) & 3) as f32 / 2.0 - 0.75), o[2] + block * ((k >> 4) as f32 / 2.0 - 0.75)];
                sum3 += f.value_among(x, &among);
                done += 1;
            }
        }
        let blocked = start.elapsed().as_nanos() as f64 / done as f64;
        block_asked.sort_unstable();
        println!(
            "{name}: {} parts; asked a sample with the cull: least {}, median {}, greatest {}, mean {mean:.2}; by an 8 cm block's list: median {}, greatest {}; ns a sample: culled {culled:.0}, whole {whole:.0}, by block {blocked:.0} (sums {s1} {s2} {sum3})",
            f.parts(),
            asked[0],
            asked[asked.len() / 2],
            asked[asked.len() - 1],
            block_asked[block_asked.len() / 2],
            block_asked[block_asked.len() - 1],
        );
    }
}

//! The lead's review probe of stage 1c (2.5D), kept as arms of `corners`: figures raised, solids
//! drawn flat, solids cut. Every check here is written from the points and faces alone: a mesh
//! checker of its own, a vertical line through a raised solid, a ray cast for the drawings, a
//! parity for the cuts. Arguments are drawn together and at their corners; details are 8, 9, 30
//! and 101.
//!
//! Kept as the review wrote it, with these changes only: its counts are printed nowhere (the
//! crate's tests print nothing) and are said in the assertions instead; each arm runs a small
//! draw by default and the review's in an ignored `_full` test (`FORMAE_REVIEW_SETS` for another
//! number of argument sets a figure in the raised arm); what the review only noted is demanded
//! (a slab is never refused for an inset, review 1c finding 1; an inset named as holding is
//! never under the least length, finding 3); the retry of a named inset is made for the bevel's
//! width and the terraces' run as well as the pillow's inset; and its last test, which timed the
//! dearest drawings and layerings by the wall clock, demands only that each is answered (a
//! drawing or layers, or a refusal as data), with no time. Its own two mistakes are corrected and
//! said where they were made (edge-on is an angle, not a size; a coarse ball does not reach its
//! radius everywhere).

use specula_formae::draw::{draw as draw_solid, views, Projection};
use specula_formae::slice::{layers, slice, Axis, Plane};
use specula_formae::{aliases, find, forms, params_of, raise, Built, Error, Found, Kind, Mesh, Param, Spec};
use specula_core::dmath;
use std::collections::BTreeMap;

type P2 = [f64; 2];
type P3 = [f64; 3];

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: P3, k: f64) -> P3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: P3) -> P3 {
    mul(a, 1.0 / norm(a))
}

/// xorshift64*.
struct Seq(u64);
impl Seq {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    fn pick<T: Copy>(&mut self, of: &[T]) -> T {
        of[((self.next() * of.len() as f64) as usize).min(of.len() - 1)]
    }
}

fn seed(name: &str) -> u64 {
    name.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)) | 1
}

fn draw(p: &Param, seq: &mut Seq, set: usize) -> f64 {
    let pick = (seq.next() * 10.0) as usize;
    let u = seq.next();
    if p.integer {
        let wide = set.is_multiple_of(8);
        let top = if wide { p.max } else { p.max.min(p.min + 14.0) };
        return match pick {
            0 => p.min,
            1 => top,
            2 => p.default,
            _ => (p.min + dmath::floor_f64(u * (top - p.min + 1.0))).min(top),
        };
    }
    let log = p.min > 0.0 && p.max / p.min > 100.0;
    match pick {
        0 => p.min,
        1 => p.max,
        2 => p.default,
        3 => {
            if log {
                p.min * 1.001
            } else {
                p.min + (p.max - p.min) * 1e-6
            }
        }
        4 => {
            if log {
                p.max / 1.001
            } else {
                p.max - (p.max - p.min) * 1e-6
            }
        }
        _ => {
            if log {
                dmath::exp_f64(dmath::ln_f64(p.min) + (dmath::ln_f64(p.max) - dmath::ln_f64(p.min)) * u).clamp(p.min, p.max)
            } else {
                p.min + (p.max - p.min) * u
            }
        }
    }
}

struct Own {
    volume: f64,
    area: f64,
    lo: P3,
    hi: P3,
    reach: f64,
    size: f64,
}

/// The mesh checked from its points and faces alone.
fn own(mesh: &Mesh, genus: u32) -> Result<Own, String> {
    let n = mesh.points.len();
    if n == 0 || mesh.faces.is_empty() {
        return Err("empty".into());
    }
    let mut used = vec![false; n];
    let mut edges: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let (mut volume, mut area) = (0.0f64, 0.0f64);
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for p in &mesh.points {
        if !p.iter().all(|x| x.is_finite()) {
            return Err("a point is not finite".into());
        }
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let reach = lo.iter().chain(hi.iter()).fold(0.0f64, |a, x| a.max(x.abs()));
    let size = norm(sub(hi, lo));
    for (fi, f) in mesh.faces.iter().enumerate() {
        let k = f.verts.len();
        if k < 3 {
            return Err(format!("face {fi} has {k} corners"));
        }
        if f.surface as usize >= mesh.surfaces.len() {
            return Err(format!("face {fi} names surface {} of {}", f.surface, mesh.surfaces.len()));
        }
        for i in 0..k {
            let (a, b) = (f.verts[i], f.verts[(i + 1) % k]);
            if a as usize >= n {
                return Err(format!("face {fi} names point {a} of {n}"));
            }
            if f.verts[..i].contains(&a) {
                return Err(format!("face {fi} names point {a} twice"));
            }
            used[a as usize] = true;
            *edges.entry((a, b)).or_insert(0) += 1;
        }
        let p0 = mesh.points[f.verts[0] as usize];
        let mut av = [0.0f64; 3];
        for i in 1..k - 1 {
            let (a, b) = (mesh.points[f.verts[i] as usize], mesh.points[f.verts[i + 1] as usize]);
            let c = cross(sub(a, p0), sub(b, p0));
            av = add(av, mul(c, 0.5));
            volume += dot(p0, cross(a, b)) / 6.0;
        }
        let fa = norm(av);
        area += fa;
        if fa <= 0.0 {
            return Err(format!("face {fi} has no area"));
        }
        let un = mul(av, 1.0 / fa);
        for &v in &f.verts {
            let off = dot(sub(mesh.points[v as usize], p0), un).abs();
            if off > 1e-7 * size {
                return Err(format!("face {fi} is not planar: a corner {off} off it (size {size})"));
            }
        }
        if let Some(ns) = &f.normals {
            if ns.len() != k {
                return Err(format!("face {fi} has {} normals for {k} corners", ns.len()));
            }
            for nn in ns {
                if (norm(*nn) - 1.0).abs() > 1e-6 {
                    return Err(format!("face {fi}: a normal of length {}", norm(*nn)));
                }
                if dot(*nn, un) <= 0.0 {
                    return Err(format!("face {fi}: a normal points behind its face"));
                }
            }
        }
    }
    if let Some(i) = used.iter().position(|u| !u) {
        return Err(format!("point {i} is used by no face"));
    }
    for (&(a, b), &c) in &edges {
        if c != 1 {
            return Err(format!("edge {a}>{b} is walked {c} times"));
        }
        if !edges.contains_key(&(b, a)) {
            return Err(format!("edge {a}>{b} has no face on its other side"));
        }
    }
    let euler = n as i64 - (edges.len() / 2) as i64 + mesh.faces.len() as i64;
    if euler != 2 - 2 * i64::from(genus) {
        return Err(format!("Euler's number is {euler} for genus {genus}"));
    }
    if volume <= 0.0 {
        return Err(format!("its volume is {volume}"));
    }
    Ok(Own { volume, area, lo, hi, reach, size })
}

/// Whether `p` is inside `poly` by the even-odd rule, and how near it is to an edge of it.
fn scan(poly: &[P2], p: P2) -> (bool, f64) {
    let (mut inside, mut near) = (false, f64::MAX);
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if x > p[0] {
                inside = !inside;
            }
        }
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let l2 = ex * ex + ey * ey;
        let t = if l2 > 0.0 { (((p[0] - a[0]) * ex + (p[1] - a[1]) * ey) / l2).clamp(0.0, 1.0) } else { 0.0 };
        let (dx, dy) = (p[0] - a[0] - t * ex, p[1] - a[1] - t * ey);
        near = near.min((dx * dx + dy * dy).sqrt());
    }
    (inside, near)
}

/// The same over several rings (an outline with holes).
fn scan_rings(rings: &[Vec<P2>], p: P2) -> (bool, f64) {
    rings.iter().fold((false, f64::MAX), |(i, n), r| {
        let (ri, rn) = scan(r, p);
        (i != ri, n.min(rn))
    })
}

fn shoelace(ring: &[P2]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| ring[i][0] * ring[(i + 1) % n][1] - ring[(i + 1) % n][0] * ring[i][1]).sum::<f64>() * 0.5
}

/// A face as three linear coordinates of its corners: a polygon in the first two, and the third
/// as a plane over them (`None` when it stands edge-on to them).
struct Flat {
    poly: Vec<P2>,
    /// `w = w0 - (nx (u - u0) + ny (v - v0)) / nz`.
    plane: Option<(P3, P3)>,
}

fn flat(points: &[P3]) -> Flat {
    let k = points.len() as f64;
    let c = points.iter().fold([0.0; 3], |c, p| add(c, mul(*p, 1.0 / k)));
    let mut n = [0.0f64; 3];
    for i in 0..points.len() {
        let (a, b) = (sub(points[i], c), sub(points[(i + 1) % points.len()], c));
        n = add(n, cross(a, b));
    }
    let scale = points.iter().map(|p| norm(sub(*p, c))).fold(0.0f64, f64::max);
    // (Edge-on is an angle, not a size: a needle's side seen from above is thin and still seen.)
    let plane = (scale > 0.0 && n[2].abs() > 1e-10 * norm(n)).then_some((c, n));
    Flat { poly: points.iter().map(|p| [p[0], p[1]]).collect(), plane }
}

impl Flat {
    fn third(&self, p: P2) -> Option<f64> {
        self.plane.map(|(c, n)| c[2] - (n[0] * (p[0] - c[0]) + n[1] * (p[1] - c[1])) / n[2])
    }
}

fn sets(default: usize) -> usize {
    std::env::var("FORMAE_REVIEW_SETS").ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn plane_figures() -> Vec<Found> {
    forms().iter().filter(|f| f.kind == Kind::Plane).map(|f| Found { form: f, alias: None }).collect()
}

fn outline_of(fig: &Spec, detail: u32) -> Option<Vec<Vec<P2>>> {
    match fig.build(detail) {
        Ok(Built::Outline(o)) => Some(o.rings),
        _ => None,
    }
}

fn box2(rings: &[Vec<P2>]) -> (P2, P2) {
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in rings.iter().flatten() {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

fn len(x: f64) -> f64 {
    x.clamp(1e-4, 1e4)
}

struct Faults(Vec<String>, BTreeMap<String, usize>);
impl Faults {
    fn new() -> Faults {
        Faults(Vec::new(), BTreeMap::new())
    }
    /// At most three of a kind a subject's family (the kind is the message's words).
    fn say(&mut self, family: &str, said: String) {
        let kind: String = said.rsplit(": ").next().unwrap_or("").chars().filter(|c| c.is_ascii_alphabetic() || *c == ' ').take(28).collect();
        let n = self.1.entry(format!("{family}|{kind}")).or_insert(0);
        *n += 1;
        if *n <= 3 {
            self.0.push(said);
        }
    }
    fn report(&self, what: &str) {
        let total: usize = self.1.values().sum();
        assert!(self.0.is_empty(), "{what}: {total} faults of {} kinds:\n{}", self.1.len(), self.0.join("\n"));
    }
}

/// Every closed plane figure raised every way, its arguments and the raising's drawn together.
/// The default run: the defaults and two corner draws a figure, at the four details.
#[test]
fn raised_by_the_reviews_checks() {
    raised(sets(3), &[8, 9, 30, 101], 600);
}

/// The review's run: eight sets a figure at details 8, 9, 30 and 101.
#[test]
#[ignore = "the review's raised draw (8 sets a figure, four details, vertical lines): minutes in release"]
fn raised_by_the_reviews_checks_full() {
    raised(sets(8), &[8, 9, 30, 101], 1_500);
}

fn raised(n_sets: usize, details: &[u32], least: usize) {
    let mut faults = Faults::new();
    let (mut built, mut refused, mut lines) = (0usize, 0usize, 0usize);
    let (mut retried, mut none_holds) = (0usize, 0usize);
    let mut rules: BTreeMap<String, usize> = BTreeMap::new();
    for w in plane_figures() {
        let params = params_of(&w);
        let mut seq = Seq(seed(w.name()) ^ 0x1c1c);
        for set in 0..n_sets {
            let given: Vec<(&str, f64)> = if set == 0 { Vec::new() } else { params.iter().map(|p| (p.name, draw(p, &mut seq, set))).collect() };
            let Ok(fig) = Spec::of(w, &given) else { continue };
            let Some(rings30) = outline_of(&fig, 30) else { continue };
            let (lo, hi) = box2(&rings30);
            let wide = (hi[0] - lo[0]).max(hi[1] - lo[1]);
            let t = len(wide * seq.pick(&[1e-3, 0.05, 0.3, 1.0, 20.0]));
            let both = seq.next() < 0.5;
            let bw = len(t * seq.pick(&[1e-3, 0.1, 0.3, 0.5, 1.0]) * if both { 0.5 } else { 1.0 });
            let inset = len(wide * seq.pick(&[1e-4, 0.01, 0.05, 0.2, 0.6]));
            let rise = len(wide * seq.pick(&[1e-3, 0.05, 0.5, 5.0]));
            let steps = seq.pick(&[1u32, 2, 3, 7, 64]);
            let run = len(wide * seq.pick(&[1e-4, 0.005, 0.03, 0.2]));
            let margin = len(wide * seq.pick(&[1e-3, 0.1, 2.0]));
            let plate = len(t * seq.pick(&[1e-2, 1.0, 10.0]));
            let round = seq.next() < 0.5;
            let made: Vec<(String, Result<specula_formae::ops::Solid, Error>, f64)> = vec![
                (format!("slab t {t}"), raise::slab(&fig, t), t),
                (format!("bevelled t {t} w {bw} round {round} both {both}"), raise::bevelled(&fig, t, bw, round, both), t),
                (format!("pillow t {t} rise {rise} inset {inset}"), raise::pillow(&fig, t, rise, inset), t + rise),
                (format!("stepped {steps} rise {rise} run {run}"), raise::stepped(&fig, steps, rise, run), f64::from(steps) * rise),
                (format!("relief h {rise} margin {margin} plate {plate}"), raise::relief(&fig, rise, margin, plate), plate + rise),
            ];
            for (how, solid, top) in made {
                let op = how.split(' ').next().unwrap_or("").to_string();
                let what = format!("{how} of {} {given:?}", w.name());
                let solid = match solid {
                    Ok(s) => s,
                    Err(Error::Constraint { rule, .. }) => {
                        refused += 1;
                        *rules.entry(format!("{op}: {rule}")).or_insert(0) += 1;
                        continue;
                    }
                    Err(e) => {
                        faults.say(&op, format!("{what}: refused at once: {e}"));
                        continue;
                    }
                };
                for &detail in details {
                    let what = format!("{what} at detail {detail}");
                    let mesh = match solid.build(detail) {
                        Ok(m) => m,
                        Err(Error::Constraint { rule, message, values, .. }) => {
                            refused += 1;
                            *rules.entry(format!("{op}: {rule}")).or_insert(0) += 1;
                            // A slab asks for no inset (finding 1: noted by the review, demanded here).
                            if op == "slab" && rule == "inset_too_deep" {
                                faults.say(&op, format!("{what}: {rule}: {message} {values:?}; the figure alone builds at this detail: {}: a slab refused for an inset", outline_of(&fig, detail).is_some()));
                            }
                            // The inset it names as holding holds (the pillow's inset, the bevel's
                            // width, the terraces' run: each built again with the value named).
                            if rule == "inset_too_deep" {
                                let named = values.iter().find(|(k, _)| *k == "inset_that_holds" || *k == "width_that_holds" || *k == "run_that_holds").map(|v| v.1);
                                if let Some(&(_, holds)) = values.iter().find(|(k, _)| *k == "deepest_inset_that_holds") {
                                    // Finding 3: an inset under the least length cannot be asked for.
                                    if holds < 1e-4 {
                                        faults.say(&op, format!("{what}: names {holds} as the deepest inset that holds: under the least length a caller may give"));
                                    }
                                    retried += 1;
                                    let again = match (op.as_str(), named) {
                                        ("pillow", Some(p)) => raise::pillow(&fig, t, rise, p),
                                        ("bevelled", Some(p)) => raise::bevelled(&fig, t, p, round, both),
                                        ("stepped", Some(p)) => raise::stepped(&fig, steps, rise, p),
                                        _ => {
                                            faults.say(&op, format!("{what}: names an inset {holds} and no value of its parameter that gives it: {values:?}"));
                                            continue;
                                        }
                                    };
                                    match again.and_then(|s| s.build(detail)) {
                                        Err(Error::Constraint { rule: "inset_too_deep", .. }) => faults.say(&op, format!("{what}: refused, naming {holds} as an inset that holds, and that is refused too: the inset named as holding does not hold")),
                                        Ok(m) => {
                                            if let Err(e) = own(&m, solid.genus()) {
                                                faults.say(&op, format!("{what}: at the inset named as holding, {holds}: {e}"));
                                            }
                                        }
                                        Err(_) => {}
                                    }
                                } else {
                                    none_holds += 1;
                                }
                            }
                            continue;
                        }
                        Err(Error::TooManyPoints { .. }) => {
                            refused += 1;
                            *rules.entry(format!("{op}: too_many_points")).or_insert(0) += 1;
                            continue;
                        }
                        Err(e) => {
                            faults.say(&op, format!("{what}: {e}"));
                            continue;
                        }
                    };
                    built += 1;
                    let o = match own(&mesh, solid.genus()) {
                        Ok(o) => o,
                        Err(e) => {
                            faults.say(&op, format!("{what}: {e}"));
                            continue;
                        }
                    };
                    match solid.count(detail) {
                        Ok(c) if (c.points, c.parts) == (mesh.points.len() as u64, mesh.faces.len() as u64) => {}
                        other => faults.say(&op, format!("{what}: counted {other:?} and built {} points, {} faces: the count is not the build", mesh.points.len(), mesh.faces.len())),
                    }
                    if solid.build(detail).ok().as_ref() != Some(&mesh) {
                        faults.say(&op, format!("{what}: built twice and not the same"));
                    }
                    let h = mesh.chord_error;
                    if !(h.is_finite() && h >= 0.0) {
                        faults.say(&op, format!("{what}: its error is {h}: not a number to trust"));
                        continue;
                    }
                    let hair = 1e-9 * o.reach + 64.0 * mesh.points.len() as f64 * f64::EPSILON * o.reach;
                    if o.lo[1].abs() > hair {
                        faults.say(&op, format!("{what}: lowest at y = {}: it does not stand on the ground", o.lo[1]));
                    }
                    if (o.hi[1] - top).abs() > hair + 1e-9 * top {
                        faults.say(&op, format!("{what}: highest at y = {} and {top} was asked: its top is not where asked", o.hi[1]));
                    }
                    let closed = solid.closed();
                    if let Some(c) = closed.size {
                        let allowed = 2.0 * h * o.area + 1e-9 * c.abs() + hair * o.reach * o.reach;
                        if (c - o.volume).abs().is_nan() || (c - o.volume).abs() > allowed {
                            faults.say(&op, format!("{what}: closed {c}, built {} (off {:e}, allowed {allowed:e}): closed volume and build apart", o.volume, (c - o.volume).abs()));
                        }
                    }
                    // The figure's own polygon at this detail, as a truth of my own.
                    let Some(rings) = outline_of(&fig, detail) else {
                        faults.say(&op, format!("{what}: raised, and the figure does not build at this detail"));
                        continue;
                    };
                    let a: f64 = rings.iter().map(|r| shoelace(r)).sum::<f64>().abs();
                    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * y.abs() + hair * o.reach * o.reach;
                    match op.as_str() {
                        "slab" if !near(o.volume, a * t) => faults.say(&op, format!("{what}: holds {} and its figure's polygon times its thickness is {}: a slab is not its figure raised", o.volume, a * t)),
                        "bevelled" if o.volume > a * t * (1.0 + 1e-9) + hair => faults.say(&op, format!("{what}: holds {} and the slab it is cut from {}: a bevel added volume", o.volume, a * t)),
                        "pillow" if o.volume < a * t * (1.0 - 1e-9) - hair || o.volume > a * (t + rise) * (1.0 + 1e-9) + hair => faults.say(&op, format!("{what}: holds {}, outside its slab {} and its box {}: a pillow out of its bounds", o.volume, a * t, a * (t + rise))),
                        "stepped" if o.volume < a * rise * (1.0 - 1e-9) - hair || o.volume > a * rise * f64::from(steps) * (1.0 + 1e-9) + hair => faults.say(&op, format!("{what}: holds {}, outside one terrace {} and all as slabs {}: terraces out of their bounds", o.volume, a * rise, a * rise * f64::from(steps))),
                        "relief" => {
                            let rect = (o.hi[0] - o.lo[0]) * (o.hi[2] - o.lo[2]);
                            if !near(o.volume, rect * plate + a * rise) {
                                faults.say(&op, format!("{what}: holds {} and its plate and figure are {}: a relief is not plate and figure", o.volume, rect * plate + a * rise));
                            }
                            let (flo, fhi) = closed_or_built_bounds(&fig, &rings);
                            let want = [flo[0] - margin, fhi[0] + margin, -fhi[1] - margin, -flo[1] + margin];
                            let got = [o.lo[0], o.hi[0], o.lo[2], o.hi[2]];
                            if want.iter().zip(&got).any(|(x, y)| (x - y).abs() > hair + 2.0 * h) {
                                faults.say(&op, format!("{what}: its plate is {got:?} and the figure's bounds widened are {want:?}: the plate is not the bounds and a margin"));
                            }
                        }
                        _ => {}
                    }
                    // A vertical line meets it twice over the figure and never elsewhere.
                    if (detail == 9 || detail == 30) && mesh.faces.len() <= 20_000 {
                        let plans: Vec<Flat> = mesh.faces.iter().map(|f| flat(&f.verts.iter().map(|&v| [mesh.points[v as usize][0], -mesh.points[v as usize][2], mesh.points[v as usize][1]]).collect::<Vec<_>>())).collect();
                        for _ in 0..24 {
                            let p = [o.lo[0] + (o.hi[0] - o.lo[0]) * seq.next(), -(o.lo[2] + (o.hi[2] - o.lo[2]) * seq.next())];
                            let (in_figure, edge) = scan_rings(&rings, p);
                            let mut near_edge = edge;
                            let mut ys: Vec<f64> = Vec::new();
                            for f in &plans {
                                let (inside, e) = scan(&f.poly, p);
                                near_edge = near_edge.min(e);
                                if inside {
                                    if let Some(y) = f.third(p) {
                                        ys.push(y);
                                    }
                                }
                            }
                            if near_edge < 1e-7 * o.size {
                                continue;
                            }
                            lines += 1;
                            let want = if op == "relief" || in_figure { 2 } else { 0 };
                            if ys.len() != want {
                                faults.say(&op, format!("{what}: a vertical line at {p:?} (in the figure: {in_figure}) meets it {} times at {ys:?}: it is not a floor and a roof over its figure", ys.len()));
                                continue;
                            }
                            if ys.iter().any(|y| *y < -hair || *y > top + hair) {
                                faults.say(&op, format!("{what}: a vertical line at {p:?} meets it at {ys:?}, outside 0 to {top}: a face outside its heights"));
                            }
                            if op == "relief" {
                                let roof = ys.iter().cloned().fold(f64::MIN, f64::max);
                                let want = if in_figure { plate + rise } else { plate };
                                if (roof - want).abs() > hair + 1e-9 * want {
                                    faults.say(&op, format!("{what}: over {p:?} (in the figure: {in_figure}) its top is at {roof} and {want} was asked: a relief's heights are not plate and figure"));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let said = format!("raised: {built} built, {refused} refused, {lines} vertical lines; refusals by rule: {rules:?}; {retried} built again at the inset named as holding, {none_holds} named none");
    faults.report(&said);
    assert!(built > least, "only {built} builds: {said}");
}

fn closed_or_built_bounds(fig: &Spec, rings: &[Vec<P2>]) -> (P2, P2) {
    match fig.closed().ok().and_then(|c| c.bounds) {
        Some((lo, hi)) => ([lo[0], lo[1]], [hi[0], hi[1]]),
        None => box2(rings),
    }
}

/// Meshes to draw and to cut: every solid word as it is given and with its arguments drawn, and
/// plane figures raised.
fn subjects(most_faces: usize, n_sets: usize) -> Vec<(String, Mesh)> {
    let mut out = Vec::new();
    let mut words: Vec<Found> = forms().iter().filter(|f| f.kind == Kind::Solid).map(|f| Found { form: f, alias: None }).collect();
    for a in aliases().iter().filter(|a| a.kind == Kind::Solid) {
        words.push(find(a.name, Some(Kind::Solid)).expect("every alias resolves"));
    }
    for w in words {
        let params = params_of(&w);
        let mut seq = Seq(seed(w.name()) ^ 0xd4a3);
        for set in 0..n_sets.min(3) {
            if set > 0 && w.alias.is_some() {
                continue;
            }
            let given: Vec<(&str, f64)> = if set == 0 { Vec::new() } else { params.iter().map(|p| (p.name, draw(p, &mut seq, set))).collect() };
            let detail = [9u32, 8, 30][set];
            if let Ok(Built::Mesh(m)) = Spec::of(w, &given).and_then(|s| s.build(detail)) {
                if m.faces.len() <= most_faces {
                    out.push((format!("{} {given:?} at {detail}", w.name()), m));
                }
            }
        }
    }
    for (i, w) in plane_figures().into_iter().enumerate() {
        let Ok(fig) = Spec::of(w, &[]) else { continue };
        let Some(rings) = outline_of(&fig, 9) else { continue };
        let (lo, hi) = box2(&rings);
        let wide = (hi[0] - lo[0]).max(hi[1] - lo[1]);
        let made = [
            ("slab", raise::slab(&fig, wide * 0.3)),
            ("stepped", raise::stepped(&fig, 3, wide * 0.1, wide * 0.02)),
            ("relief", raise::relief(&fig, wide * 0.2, wide * 0.1, wide * 0.05)),
            ("pillow", raise::pillow(&fig, wide * 0.2, wide * 0.1, wide * 0.03)),
            ("bevelled", raise::bevelled(&fig, wide * 0.2, wide * 0.02, true, true)),
        ];
        for (k, (op, s)) in made.into_iter().enumerate() {
            // (Every figure as a slab; the other four in turn, so each is met a dozen times.)
            if k != 0 && k != 1 + i % 4 {
                continue;
            }
            if let Ok(m) = s.and_then(|s| s.build(9)) {
                if m.faces.len() <= most_faces {
                    out.push((format!("{op} of {} at 9", w.name()), m));
                }
            }
        }
    }
    out
}

/// What the named views are said to be, from their rows.
#[test]
fn the_named_views_are_what_their_names_say() {
    let row = |name: &str| views().into_iter().find(|v| v.name == name).unwrap_or_else(|| panic!("no view {name}")).projection.rows();
    let pic = |r: &[P3; 3], p: P3| [dot(r[0], p), dot(r[1], p)];
    let (x, y, z) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    let close = |a: P2, b: P2| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12;
    let l = |a: P2| (a[0] * a[0] + a[1] * a[1]).sqrt();
    let top = row("top");
    assert!(close(pic(&top, x), [1.0, 0.0]) && close(pic(&top, z), [0.0, -1.0]) && l(pic(&top, y)) < 1e-12, "top");
    let front = row("front");
    assert!(close(pic(&front, x), [1.0, 0.0]) && close(pic(&front, y), [0.0, 1.0]) && l(pic(&front, z)) < 1e-12, "front");
    let side = row("side");
    assert!(close(pic(&side, z), [-1.0, 0.0]) && close(pic(&side, y), [0.0, 1.0]) && l(pic(&side, x)) < 1e-12, "side");
    let iso = row("isometric");
    for a in [x, y, z] {
        assert!((l(pic(&iso, a)) - (2.0f64 / 3.0).sqrt()).abs() < 1e-12, "isometric: an axis draws {} long", l(pic(&iso, a)));
    }
    let (ix, iz) = (pic(&iso, x), pic(&iso, z));
    assert!(((ix[0] * iz[0] + ix[1] * iz[1]) / (l(ix) * l(iz)) + 0.5).abs() < 1e-12, "isometric: x and z are not 120 degrees apart");
    assert!(pic(&iso, y)[0].abs() < 1e-12 && pic(&iso, y)[1] > 0.0, "isometric: y is not up");
    let game = row("game_isometric");
    for a in [x, z] {
        let p = pic(&game, a);
        assert!((p[1].abs() / p[0].abs() - 0.5).abs() < 1e-12, "game isometric: a ground axis rises {} for one across", p[1].abs() / p[0].abs());
    }
    let dim = row("dimetric");
    assert!((l(pic(&dim, x)) - l(pic(&dim, y))).abs() < 1e-12 && (l(pic(&dim, z)) - 0.5 * l(pic(&dim, x))).abs() < 1e-12, "dimetric: {} {} {}", l(pic(&dim, x)), l(pic(&dim, y)), l(pic(&dim, z)));
    for (name, depth) in [("cavalier", 1.0), ("cabinet", 0.5)] {
        let r = row(name);
        let back = pic(&r, [0.0, 0.0, -1.0]);
        assert!(close(pic(&r, x), [1.0, 0.0]) && close(pic(&r, y), [0.0, 1.0]) && (l(back) - depth).abs() < 1e-12 && (back[0] - back[1]).abs() < 1e-12 && back[0] > 0.0, "{name}: depth draws {back:?}");
    }
    assert_eq!(views().len(), 8);
}

/// Every drawing against a ray cast of my own: at points of the picture, the polygon drawn there
/// is the nearest face along the ray, and there is exactly one where the ray meets the solid. The
/// default run: every solid word as given and the raised figures, of up to 3,000 faces, 30 points
/// a drawing.
#[test]
fn drawings_by_the_reviews_ray_cast() {
    drawings(1, 3_000, 30, 1_000, 20_000);
}

/// The review's run: three sets a word, up to 3,000 faces, 80 points a drawing.
#[test]
#[ignore = "the review's drawings against a ray cast (three sets a word, 80 points a drawing): minutes in release"]
fn drawings_by_the_reviews_ray_cast_full() {
    drawings(3, 3_000, 80, 2_000, 60_000);
}

fn drawings(n_sets: usize, most_faces: usize, samples: usize, least_drawn: usize, least_points: usize) {
    let mut faults = Faults::new();
    let mut seq = Seq(0x51de_c0de_1c1c_0001);
    let (mut drawn, mut refused, mut points, mut no_outline, mut convex) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut rules: BTreeMap<String, usize> = BTreeMap::new();
    for (name, mesh) in subjects(most_faces, n_sets) {
        let family = name.split(' ').next().unwrap_or("").to_string();
        let mut projections: Vec<(String, Projection)> = views().into_iter().map(|v| (v.name.to_string(), v.projection)).collect();
        for (a, e) in [(0.0, -90.0), (180.0, 0.0), (37.0, 11.0), (301.0, -47.0), (90.0, 89.999), (360.0 * seq.next(), 180.0 * seq.next() - 90.0)] {
            projections.push((format!("from {a} up {e}"), Projection::Orthographic { azimuth: a, elevation: e }));
        }
        projections.push(("oblique 200 at 0.3".into(), Projection::Oblique { angle: 200.0, depth: 0.3 }));
        let size = {
            let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
            for p in &mesh.points {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            norm(sub(hi, lo))
        };
        for (view, projection) in projections {
            let what = format!("{name} drawn {view}");
            let d = match draw_solid(&mesh, projection) {
                Ok(d) => d,
                Err(Error::Constraint { rule, .. }) => {
                    refused += 1;
                    *rules.entry(rule.to_string()).or_insert(0) += 1;
                    continue;
                }
                Err(e) => {
                    faults.say(&family, format!("{what}: {e}"));
                    continue;
                }
            };
            drawn += 1;
            convex += usize::from(d.convex);
            if d.silhouette_refused.is_some() {
                no_outline += 1;
            }
            let rows = projection.rows();
            let to = |p: P3| [dot(rows[0], p), dot(rows[1], p), dot(rows[2], p)];
            let faces: Vec<Flat> = mesh.faces.iter().map(|f| flat(&f.verts.iter().map(|&v| to(mesh.points[v as usize])).collect::<Vec<_>>())).collect();
            let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
            for f in &faces {
                for p in &f.poly {
                    for k in 0..2 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
            }
            let pic = ((hi[0] - lo[0]) * (hi[0] - lo[0]) + (hi[1] - lo[1]) * (hi[1] - lo[1])).sqrt();
            // The polygons themselves.
            let mut last = f64::MAX;
            let mut bad = false;
            for (i, g) in d.polygons.iter().enumerate() {
                let n = g.points.len();
                let finite = g.points.iter().flatten().all(|x| x.is_finite());
                let turns = (0..n).all(|k| {
                    let (a, b, c) = (g.points[k], g.points[(k + 1) % n], g.points[(k + 2) % n]);
                    (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]) >= -1e-9 * pic * pic
                });
                if n < 3 || !finite || !turns || shoelace(&g.points) <= 0.0 {
                    faults.say(&family, format!("{what}: polygon {i} of {n} corners (finite {finite}, turning left {turns}, area {}): a drawn polygon is not convex and counter-clockwise", shoelace(&g.points)));
                    bad = true;
                    break;
                }
                if g.face as usize >= mesh.faces.len() || mesh.faces[g.face as usize].surface != g.surface || !(0.0..=1.0).contains(&g.shade) {
                    faults.say(&family, format!("{what}: polygon {i} names face {} surface {} shade {}: a drawn polygon's face, surface or shade is wrong", g.face, g.surface, g.shade));
                    bad = true;
                    break;
                }
                if g.depth > last + 1e-9 * size {
                    faults.say(&family, format!("{what}: polygon {i} at depth {} comes after one at {last}: the polygons are not farthest first", g.depth));
                    bad = true;
                    break;
                }
                last = g.depth;
            }
            if bad {
                continue;
            }
            if draw_solid(&mesh, projection).ok().as_ref() != Some(&d) {
                faults.say(&family, format!("{what}: drawn twice and not the same"));
            }
            for _ in 0..samples {
                let p = [lo[0] + (hi[0] - lo[0]) * seq.next(), lo[1] + (hi[1] - lo[1]) * seq.next()];
                // Mine: the nearest face the ray meets (the smallest depth).
                let mut edge = f64::MAX;
                let mut hits: Vec<(f64, usize)> = Vec::new();
                for (fi, f) in faces.iter().enumerate() {
                    let (inside, e) = scan(&f.poly, p);
                    edge = edge.min(e);
                    if inside {
                        if let Some(depth) = f.third(p) {
                            hits.push((depth, fi));
                        }
                    }
                }
                let mut theirs: Vec<usize> = Vec::new();
                for (gi, g) in d.polygons.iter().enumerate() {
                    let (inside, e) = scan(&g.points, p);
                    edge = edge.min(e);
                    if inside {
                        theirs.push(gi);
                    }
                }
                let (in_outline, e) = scan_rings(&d.silhouette, p);
                if d.silhouette_refused.is_none() {
                    edge = edge.min(e);
                }
                if edge < 1e-6 * pic {
                    continue;
                }
                hits.sort_by(|a, b| a.0.total_cmp(&b.0));
                if hits.len() >= 2 && hits[1].0 - hits[0].0 < 1e-7 * size {
                    continue;
                }
                points += 1;
                if d.silhouette_refused.is_none() && in_outline != !hits.is_empty() {
                    faults.say(&family, format!("{what}: at {p:?} the ray meets {} faces and the silhouette says inside {in_outline}: the silhouette is not the picture's outline", hits.len()));
                }
                match (hits.first(), theirs.as_slice()) {
                    (None, []) => {}
                    (Some(&(depth, fi)), [gi]) => {
                        let g = &d.polygons[*gi];
                        let theirs_depth = faces[g.face as usize].third(p);
                        if g.face as usize != fi && theirs_depth.is_none_or(|t| (t - depth).abs() > 1e-7 * size) {
                            faults.say(&family, format!("{what}: at {p:?} face {fi} is nearest (depth {depth}) and face {} is drawn (depth {theirs_depth:?}): a hidden face is drawn", g.face));
                        }
                    }
                    (mine, theirs) => faults.say(&family, format!("{what}: at {p:?} the ray's nearest face is {:?} and {} polygons are drawn there: the polygons do not cover the picture once", mine.map(|h| h.1), theirs.len())),
                }
            }
        }
    }
    let said = format!("drawings: {drawn} drawn ({convex} by facing alone, {no_outline} with no silhouette), {refused} refused {rules:?}, {points} points held against a ray cast");
    faults.report(&said);
    assert!(drawn > least_drawn && points > least_points, "only {drawn} drawings and {points} points: {said}");
    for bad in [Projection::Orthographic { azimuth: f64::NAN, elevation: 0.0 }, Projection::Orthographic { azimuth: 0.0, elevation: f64::INFINITY }, Projection::Oblique { angle: f64::NAN, depth: 1.0 }, Projection::Oblique { angle: 45.0, depth: f64::INFINITY }] {
        let cube = match Spec::of(find("cube", Some(Kind::Solid)).expect("a cube"), &[]).and_then(|s| s.build(9)) {
            Ok(Built::Mesh(m)) => m,
            _ => panic!("no cube"),
        };
        assert!(draw_solid(&cube, bad).is_err(), "{bad:?} was drawn");
    }
}

/// Whether `p` is inside the mesh, by the parity of a ray's crossings; `None` where the ray
/// passes too near an edge or starts on the surface.
fn inside(mesh: &Mesh, p: P3, dir: P3, size: f64) -> Option<bool> {
    let mut odd = false;
    for f in &mesh.faces {
        let pts: Vec<P3> = f.verts.iter().map(|&v| mesh.points[v as usize]).collect();
        let k = pts.len() as f64;
        let c = pts.iter().fold([0.0; 3], |c, q| add(c, mul(*q, 1.0 / k)));
        let mut n = [0.0f64; 3];
        for i in 0..pts.len() {
            n = add(n, cross(sub(pts[i], c), sub(pts[(i + 1) % pts.len()], c)));
        }
        let nl = norm(n);
        if nl == 0.0 {
            return None;
        }
        let n = mul(n, 1.0 / nl);
        let off = dot(n, sub(c, p));
        let along = dot(n, dir);
        if along.abs() < 1e-9 {
            if off.abs() < 1e-7 * size {
                return None;
            }
            continue;
        }
        let s = off / along;
        if s.abs() < 1e-7 * size {
            return None;
        }
        if s < 0.0 {
            continue;
        }
        let hit = add(p, mul(dir, s));
        let drop = if n[0].abs() >= n[1].abs() && n[0].abs() >= n[2].abs() { 0 } else if n[1].abs() >= n[2].abs() { 1 } else { 2 };
        let two = |q: P3| -> P2 {
            match drop {
                0 => [q[1], q[2]],
                1 => [q[0], q[2]],
                _ => [q[0], q[1]],
            }
        };
        let poly: Vec<P2> = pts.iter().map(|q| two(*q)).collect();
        let (is_in, edge) = scan(&poly, two(hit));
        if edge < 1e-7 * size {
            return None;
        }
        odd ^= is_in;
    }
    Some(odd)
}

/// Every cut against a parity of my own: a point of the plane is inside the cut's rings exactly
/// when it is inside the solid. Planes through the solid's own points are cut too. The default
/// run: every solid word as given and the raised figures, of up to 2,500 faces, 15 points a cut.
#[test]
fn cuts_by_the_reviews_parity() {
    cuts(1, 2_500, 15, 1_000, 5_000);
}

/// The review's run: three sets a word, up to 2,500 faces, 30 points a cut.
#[test]
#[ignore = "the review's cuts against a parity (three sets a word, 30 points a cut): minutes in release"]
fn cuts_by_the_reviews_parity_full() {
    cuts(3, 2_500, 30, 1_500, 20_000);
}

fn cuts(n_sets: usize, most_faces: usize, samples: usize, least_cut: usize, least_points: usize) {
    let mut faults = Faults::new();
    let mut seq = Seq(0x0c07_5eed_1c1c_0003);
    let (mut cut, mut points, mut through) = (0usize, 0usize, 0usize);
    let mut refusals: BTreeMap<String, usize> = BTreeMap::new();
    let dirs = [unit([0.5377, 0.6311, 0.5589]), unit([-0.3141, 0.2718, 0.9093])];
    for (name, mesh) in subjects(most_faces, n_sets) {
        let family = name.split(' ').next().unwrap_or("").to_string();
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        for p in &mesh.points {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let size = norm(sub(hi, lo));
        let mid = mul(add(lo, hi), 0.5);
        let mut planes: Vec<(String, Plane)> = Vec::new();
        for (k, axis) in [Axis::X, Axis::Y, Axis::Z].into_iter().enumerate() {
            planes.push((format!("{axis:?} at its middle"), Plane::at(axis, mid[k])));
            planes.push((format!("{axis:?} at 0.123"), Plane::at(axis, lo[k] + (hi[k] - lo[k]) * 0.123)));
            // Through the solid's own points: its lowest, its highest, and one between.
            let mut own_heights: Vec<f64> = mesh.points.iter().map(|p| p[k]).collect();
            own_heights.sort_by(f64::total_cmp);
            own_heights.dedup();
            for h in [own_heights[0], own_heights[own_heights.len() / 2], own_heights[own_heights.len() - 1]] {
                planes.push((format!("{axis:?} through its own point at {h}"), Plane::at(axis, h)));
            }
        }
        for _ in 0..2 {
            let n = unit([seq.next() - 0.5, seq.next() - 0.5, seq.next() - 0.5]);
            planes.push((format!("aslant {n:?}"), Plane { normal: n, offset: dot(n, mid) + (seq.next() - 0.5) * 0.3 * size }));
        }
        for (how, plane) in planes {
            let what = format!("{name} cut {how}");
            let n = unit(plane.normal);
            let offset = plane.offset / norm(plane.normal);
            let on_points = mesh.points.iter().any(|p| (dot(n, *p) - offset).abs() < 1e-9 * size);
            through += usize::from(on_points);
            let s = match slice(&mesh, plane) {
                Ok(s) => s,
                Err(Error::Constraint { rule, .. }) => {
                    *refusals.entry(format!("{rule}{}", if on_points { " (through its own points)" } else { " (through none of its points)" })).or_insert(0) += 1;
                    continue;
                }
                Err(e) => {
                    faults.say(&family, format!("{what}: {e}"));
                    continue;
                }
            };
            cut += 1;
            if s.rings.iter().any(|r| r.len() < 3 || r.iter().flatten().any(|x| !x.is_finite())) {
                faults.say(&family, format!("{what}: a ring of under three points, or not finite: a cut's ring is not a ring"));
                continue;
            }
            if slice(&mesh, plane).ok().as_ref() != Some(&s) {
                faults.say(&family, format!("{what}: cut twice and not the same"));
            }
            let region_rings: usize = s.regions.iter().map(|r| r.rings.len()).sum();
            if region_rings != s.rings.len() {
                faults.say(&family, format!("{what}: {} rings and {region_rings} in its regions: the regions do not hold every ring", s.rings.len()));
            }
            let (u, v) = plane.frame();
            let to = |p: P3| -> P2 { [dot(u, p), dot(v, p)] };
            let (mut plo, mut phi) = ([f64::MAX; 2], [f64::MIN; 2]);
            for p in &mesh.points {
                let q = to(*p);
                for k in 0..2 {
                    plo[k] = plo[k].min(q[k]);
                    phi[k] = phi[k].max(q[k]);
                }
            }
            for _ in 0..samples {
                let q = [plo[0] + (phi[0] - plo[0]) * seq.next(), plo[1] + (phi[1] - plo[1]) * seq.next()];
                let world = add(add(mul(u, q[0]), mul(v, q[1])), mul(n, offset));
                let (in_rings, edge) = scan_rings(&s.rings, q);
                if edge < 1e-6 * size {
                    continue;
                }
                let (Some(a), Some(b)) = (inside(&mesh, world, dirs[0], size), inside(&mesh, world, dirs[1], size)) else { continue };
                if a != b {
                    continue;
                }
                points += 1;
                if a != in_rings {
                    faults.say(&family, format!("{what}: the point {world:?} is inside the solid: {a}, and inside the cut's rings: {in_rings}: a cut is not the solid's section"));
                }
                let holding = s.regions.iter().filter(|r| scan_rings(&r.rings, q).0).count();
                if holding != usize::from(in_rings) {
                    faults.say(&family, format!("{what}: the point {q:?} is in {holding} regions and inside the rings: {in_rings}: the regions are not the rings' areas"));
                }
            }
        }
    }
    let said = format!("cuts: {cut} cut ({through} planes through a solid's own points), {points} points held against a parity; refusals: {refusals:?}");
    faults.report(&said);
    assert!(cut > least_cut && points > least_points, "only {cut} cuts and {points} points: {said}");
}

/// Truths that are exact: a slab's level cut is its figure; a slab's layers hold its volume; a
/// ball's cut lies on the ball; layers are cuts at the middles; the bounds refuse.
#[test]
fn cuts_that_are_known_exactly() {
    let mut faults = Faults::new();
    let mut slabs = 0usize;
    for w in plane_figures() {
        let Ok(fig) = Spec::of(w, &[]) else { continue };
        for detail in [9u32, 30] {
            let Some(rings) = outline_of(&fig, detail) else { continue };
            let (lo, hi) = box2(&rings);
            let t = (hi[0] - lo[0]).max(hi[1] - lo[1]) * 0.37;
            let Ok(mesh) = raise::slab(&fig, t).and_then(|s| s.build(detail)) else { continue };
            slabs += 1;
            let a: f64 = rings.iter().map(|r| shoelace(r)).sum::<f64>().abs();
            let what = format!("a slab of {} at detail {detail}", w.name());
            for share in [0.5, 0.123, 0.999] {
                match slice(&mesh, Plane::at(Axis::Y, t * share)) {
                    Ok(s) => {
                        if (s.area() - a).abs() > 1e-9 * a || s.rings.len() != rings.len() {
                            faults.say("slab", format!("{what} cut level at {share} of its height: area {} in {} rings, and its figure is {a} in {}: a slab's level cut is not its figure", s.area(), s.rings.len(), rings.len()));
                        }
                    }
                    Err(e) => faults.say("slab", format!("{what} cut level at {share} of its height: {e}: a slab's level cut was refused")),
                }
            }
            match layers(&mesh, Axis::Y, 7) {
                Ok(l) => {
                    let held: f64 = l.iter().map(|s| s.area() * t / 7.0).sum();
                    if l.len() != 7 || (held - a * t).abs() > 1e-9 * a * t {
                        faults.say("slab", format!("{what} in 7 layers: {} layers holding {held}, and it holds {}: a slab's layers do not hold its volume", l.len(), a * t));
                    }
                    for (i, s) in l.iter().enumerate() {
                        let at = t * (i as f64 + 0.5) / 7.0;
                        if (s.plane.offset - at).abs() > 1e-12 * t || slice(&mesh, Plane::at(Axis::Y, s.plane.offset)).ok().as_ref() != Some(s) {
                            faults.say("slab", format!("{what}: layer {i} is at {} (the middle is {at}) or is not the cut there: a layer is not the cut at its span's middle", s.plane.offset));
                        }
                    }
                }
                Err(e) => faults.say("slab", format!("{what} in 7 layers: {e}: a slab's layers were refused")),
            }
        }
    }
    assert!(slabs > 60, "only {slabs} slabs");
    let ball = find("sphere", Some(Kind::Solid)).expect("a sphere");
    for detail in [8u32, 9, 30, 101] {
        let spec = Spec::of(ball, &[]).expect("a sphere as given");
        let r = spec.args.f("radius");
        let Ok(Built::Mesh(mesh)) = spec.build(detail) else { panic!("no sphere at {detail}") };
        let h = mesh.chord_error;
        for (axis, k) in [(Axis::X, 0usize), (Axis::Y, 1), (Axis::Z, 2)] {
            for share in [-0.83, -0.2, 0.0, 0.41, 0.97] {
                let centre = [0.0, r, 0.0];
                let plane = Plane::at(axis, centre[k] + r * share);
                // (A coarse ball does not reach its radius everywhere: a plane past the build
                // itself rightly cuts nothing.)
                let reach = mesh.points.iter().map(|p| p[k]).fold(f64::MIN, f64::max);
                if plane.offset >= reach - 1e-9 * r {
                    continue;
                }
                let what = format!("a sphere at detail {detail} cut {axis:?} at {share} of its radius");
                match slice(&mesh, plane) {
                    Ok(s) => {
                        let (u, v) = plane.frame();
                        let unit_n = [if k == 0 { 1.0 } else { 0.0 }, if k == 1 { 1.0 } else { 0.0 }, if k == 2 { 1.0 } else { 0.0 }];
                        let worst = s.rings.iter().flatten().map(|q| r - norm(sub(add(add(mul(u, q[0]), mul(v, q[1])), mul(unit_n, plane.offset)), centre))).fold(f64::MIN, f64::max);
                        let least = s.rings.iter().flatten().map(|q| r - norm(sub(add(add(mul(u, q[0]), mul(v, q[1])), mul(unit_n, plane.offset)), centre))).fold(f64::MAX, f64::min);
                        let true_area = std::f64::consts::PI * r * r * (1.0 - share * share);
                        if s.rings.len() != 1 || worst > h * (1.0 + 1e-6) + 1e-12 * r || least < -1e-12 * r || s.area() > true_area * (1.0 + 1e-12) {
                            faults.say("sphere", format!("{what}: {} rings, points from {least} to {worst} under the ball (its error {h}), area {} of the disc's {true_area}: a ball's cut is not within its error of a circle", s.rings.len(), s.area()));
                        }
                    }
                    Err(e) => faults.say("sphere", format!("{what}: {e}: a ball's cut was refused")),
                }
            }
        }
        for count in [0u32, 4097, u32::MAX] {
            assert!(matches!(layers(&mesh, Axis::Y, count), Err(Error::OutOfRange { .. })), "{count} layers were cut");
        }
        for bad in [Plane { normal: [0.0; 3], offset: 0.0 }, Plane { normal: [f64::NAN, 0.0, 1.0], offset: 0.0 }, Plane { normal: [0.0, 1.0, 0.0], offset: f64::INFINITY }] {
            assert!(slice(&mesh, bad).is_err(), "{bad:?} cut a ball");
        }
        // A plane that misses the solid cuts nothing, and says so as an empty cut or a refusal.
        match slice(&mesh, Plane::at(Axis::Y, 3.0 * r)) {
            Ok(s) if s.rings.is_empty() && s.area() == 0.0 => {}
            Ok(s) => faults.say("sphere", format!("a sphere at detail {detail} cut above itself: {} rings: a plane that misses cut something", s.rings.len())),
            Err(_) => {}
        }
    }
    faults.report("exact cuts");
}

/// The largest drawings and layerings are answered: every solid word as given at detail 101,
/// drawn in the isometric and cut into the most layers, each a drawing or layers held to their
/// counts, or a refusal as data (the review timed them by the wall clock and printed the
/// dearest; a time is not asserted here, and the crate's tests read no clock).
#[test]
#[ignore = "every solid word at detail 101 drawn isometric and cut into 4096 layers: minutes in the gate's build"]
fn the_largest_are_answered() {
    let mut answered = 0usize;
    let mut words: Vec<Found> = forms().iter().filter(|f| f.kind == Kind::Solid).map(|f| Found { form: f, alias: None }).collect();
    words.sort_by_key(|w| w.name());
    let iso = views().into_iter().find(|v| v.name == "isometric").expect("isometric").projection;
    for w in words {
        let Ok(Built::Mesh(mesh)) = Spec::of(w, &[]).and_then(|s| s.build(101)) else { continue };
        match draw_solid(&mesh, iso) {
            Ok(d) => assert!(!d.polygons.is_empty(), "{} drawn with no polygon", w.name()),
            Err(Error::Constraint { .. }) => {}
            Err(e) => panic!("{} drawn: {e}", w.name()),
        }
        match layers(&mesh, Axis::Y, 4096) {
            Ok(l) => assert_eq!(l.len(), 4096, "{}", w.name()),
            Err(Error::Constraint { .. }) => {}
            Err(e) => panic!("{} in layers: {e}", w.name()),
        }
        answered += 1;
    }
    assert!(answered > 70, "only {answered}");
}

//! The lead's review probe of stage 1b, kept as a test of the crate: **every word, its arguments
//! drawn at their corners together** (each parameter at its least, its greatest, just inside
//! either, its default, or at random), at details the table tests do not otherwise use (8, 9, 30,
//! 101: odd and not powers of two), held by checks written here from the points alone. Nothing of
//! the crate's own checkers (`Mesh::validate`, `Outline::validate`, `Curve::validate`) is trusted:
//! a solid's edges are paired, its Euler number, volume, area, planarity and normals taken from
//! its points and faces; a figure's rings are walked for their winding, their crossings and their
//! holes from their points.
//!
//! Each build is refused as data or holds all of it. Refusals are counted; a refusal for a normal
//! behind its face, or for a build not well made, is a fault for every form but the tubes swept
//! along a curve (whose construction does not prove its normals sound: they are refused as data
//! where a detail is too coarse for them).
//!
//! The default run draws a few sets a word; the `_full` tests (ignored: minutes) draw 24 (12 for
//! the operators), as the review did, and `FORMAE_CORNER_SETS` draws another number.
//!
//! The lead's review probe of stage 1c is kept whole as the module `review_1c` (its own checkers
//! and its arms: the vertical line through a raised solid, the retry of a named inset, the ray
//! cast for drawings, the named views, the parity for cuts, the exact cuts).

/// The lead's probe of stage 1c, as its arms of this test.
#[path = "corners/review_1c.rs"]
mod review_1c;

use specula_core::dmath;
use specula_formae::{aliases, find, forms, ops, params_of, raise, Built, Error, Family, Found, Kind, Mesh, Param, Spec, Unit};
use std::collections::BTreeMap;

type P3 = [f64; 3];
type Pt = [f64; 2];

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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

/// xorshift64*: another sequence than the crate's other tests draw from.
struct Seq(u64);
impl Seq {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn seed(name: &str) -> u64 {
    name.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)) | 1
}

/// A parameter's value for set `set`: its least, its greatest, its default, just inside either
/// end, or anywhere (lengths by their logarithm).
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
                let (lo, hi) = (dmath::ln_f64(p.min), dmath::ln_f64(p.max));
                dmath::exp_f64(lo + (hi - lo) * u).clamp(p.min, p.max)
            } else {
                p.min + (p.max - p.min) * u
            }
        }
    }
}

fn sets(default: usize) -> usize {
    std::env::var("FORMAE_CORNER_SETS").ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

const DETAILS: [u32; 4] = [8, 9, 30, 101];

/// Gathers a fault, two of each kind a word (the kind is the message's words, numbers aside).
fn gather(faults: &mut Vec<String>, word: &str, said: String) {
    let words_of = |s: &str| -> String { s.split_once(": ").map_or("", |x| x.1).chars().filter(|c| c.is_ascii_alphabetic() || *c == ' ').take(22).collect() };
    let kind = words_of(&said);
    if faults.iter().filter(|f| f.starts_with(word) && words_of(f) == kind).count() < 2 {
        faults.push(said);
    }
}

fn words(kind: Kind) -> Vec<Found> {
    let mut v: Vec<Found> = forms().iter().filter(|f| f.kind == kind).map(|f| Found { form: f, alias: None }).collect();
    for a in aliases().iter().filter(|a| a.kind == kind) {
        v.push(find(a.name, Some(kind)).expect("every alias resolves"));
    }
    v
}

// ---------------------------------------------------------------- solids

struct Own {
    volume: f64,
    area: f64,
    lo: P3,
    hi: P3,
    reach: f64,
}

/// The mesh checked from its points and faces alone. `Err` says what is wrong.
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
        // Newell's area vector, and the volume by the divergence theorem (about the origin).
        let p0 = mesh.points[f.verts[0] as usize];
        let mut av = [0.0f64; 3];
        for i in 1..k - 1 {
            let (a, b) = (mesh.points[f.verts[i] as usize], mesh.points[f.verts[i + 1] as usize]);
            let c = cross(sub(a, p0), sub(b, p0));
            av = [av[0] + c[0] * 0.5, av[1] + c[1] * 0.5, av[2] + c[2] * 0.5];
            volume += dot(p0, cross(a, b)) / 6.0;
        }
        let fa = norm(av);
        area += fa;
        if fa <= 0.0 {
            return Err(format!("face {fi} has no area"));
        }
        // Planar: every corner within a hair of the plane through the first, across the normal.
        let unit = [av[0] / fa, av[1] / fa, av[2] / fa];
        for &v in &f.verts {
            let off = dot(sub(mesh.points[v as usize], p0), unit).abs();
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
                if dot(*nn, unit) <= 0.0 {
                    return Err(format!("face {fi}: a normal points in"));
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
    Ok(Own { volume, area, lo, hi, reach })
}

/// A refusal that may not happen to this word: a build not well made, or a normal behind its face
/// for any form but a tube swept along a curve.
fn forbidden(w: &Found, rule: &str) -> bool {
    rule == "not_well_made" || (rule == "normal_behind_face" && w.form.family != Family::Sweep)
}

/// The default run: three sets a solid word.
#[test]
fn solids_by_checks_of_my_own() {
    solids(sets(3), 200);
}

/// The full run: 24 sets a solid word, as the review drew.
#[test]
#[ignore = "the full corner draw of every solid word (24 sets, four details each): over a minute in release, several in the gate's build"]
fn solids_full() {
    solids(sets(24), 2_000);
}

fn solids(n_sets: usize, least: usize) {
    let mut faults: Vec<String> = Vec::new();
    let (mut built, mut refused) = (0usize, 0usize);
    for w in words(Kind::Solid) {
        let params = params_of(&w);
        let mut seq = Seq(seed(w.name()));
        for set in 0..n_sets {
            let given: Vec<(&str, f64)> = params.iter().map(|p| (p.name, draw(p, &mut seq, set))).collect();
            let what = format!("{} {given:?}", w.name());
            let s = match Spec::of(w, &given) {
                Ok(s) => s,
                Err(e) => {
                    gather(&mut faults, w.name(), format!("{what}: inside its ranges, refused at once: {e}"));
                    continue;
                }
            };
            for detail in DETAILS {
                let what = format!("{what} at detail {detail}");
                let b = match s.build(detail) {
                    Ok(Built::Mesh(m)) => m,
                    Ok(_) => {
                        gather(&mut faults, w.name(), format!("{what}: a solid built to something else"));
                        continue;
                    }
                    Err(Error::Constraint { rule, .. }) if forbidden(&w, rule) => {
                        gather(&mut faults, w.name(), format!("{what}: refused as {rule}"));
                        continue;
                    }
                    Err(Error::Constraint { .. } | Error::TooManyPoints { .. }) => {
                        refused += 1;
                        continue;
                    }
                    Err(e) => {
                        gather(&mut faults, w.name(), format!("{what}: {e}"));
                        continue;
                    }
                };
                built += 1;
                let Some(genus) = s.genus() else {
                    gather(&mut faults, w.name(), format!("{what}: no genus"));
                    continue;
                };
                let o = match own(&b, genus) {
                    Ok(o) => o,
                    Err(e) => {
                        gather(&mut faults, w.name(), format!("{what}: {e}"));
                        continue;
                    }
                };
                match s.count(detail) {
                    Ok(c) if (c.points, c.parts) == (b.points.len() as u64, b.faces.len() as u64) => {}
                    other => gather(&mut faults, w.name(), format!("{what}: counted {other:?}, built {} points, {} faces", b.points.len(), b.faces.len())),
                }
                // The same again is the same.
                if s.build(detail).ok() != Some(Built::Mesh(b.clone())) {
                    gather(&mut faults, w.name(), format!("{what}: built twice, not the same"));
                }
                let h = b.chord_error;
                if !(h.is_finite() && h >= 0.0) {
                    gather(&mut faults, w.name(), format!("{what}: chord error {h}"));
                    continue;
                }
                let hair = 1e-9 * o.reach + 64.0 * b.points.len() as f64 * f64::EPSILON * o.reach;
                // Its own measures against mine.
                let m = b.measures();
                if let (Some(v), Some(a)) = (m.size, m.boundary) {
                    if (v - o.volume).abs() > 1e-9 * o.volume + hair * o.reach * o.reach || (a - o.area).abs() > 1e-9 * o.area + hair * o.reach {
                        gather(&mut faults, w.name(), format!("{what}: its measures say volume {v}, area {a}; the faces give {} and {}", o.volume, o.area));
                    }
                } else {
                    gather(&mut faults, w.name(), format!("{what}: a mesh with no volume or area of its own"));
                }
                // Its triangles hold the same volume.
                let tris = b.triangles();
                let tv: f64 = tris.iter().map(|(_, t)| dot(b.points[t[0] as usize], cross(b.points[t[1] as usize], b.points[t[2] as usize])) / 6.0).sum();
                let want: usize = b.faces.iter().map(|f| f.verts.len() - 2).sum();
                if tris.len() != want || (tv - o.volume).abs() > 1e-9 * o.volume + hair * o.reach * o.reach {
                    gather(&mut faults, w.name(), format!("{what}: {} triangles of {want}, holding {tv} of {}", tris.len(), o.volume));
                }
                // The closed forms.
                let closed = match s.closed() {
                    Ok(c) => c,
                    Err(e) => {
                        gather(&mut faults, w.name(), format!("{what}: built, and its closed forms refused: {e}"));
                        continue;
                    }
                };
                let flat = !w.form.curved;
                if flat && h != 0.0 {
                    gather(&mut faults, w.name(), format!("{what}: a form of flat faces with a chord error of {h}"));
                }
                if let Some(c) = closed.size {
                    let allowed = 2.0 * h * o.area + 1e-9 * c.abs() + hair * o.reach * o.reach;
                    if over((c - o.volume).abs(), allowed) {
                        gather(&mut faults, w.name(), format!("{what}: volume closed {c}, built {} (off by {}, allowed {allowed})", o.volume, (c - o.volume).abs()));
                    }
                }
                if let (true, Some(c)) = (flat, closed.boundary) {
                    if over((c - o.area).abs(), 1e-9 * c.abs() + hair * o.reach) {
                        gather(&mut faults, w.name(), format!("{what}: area closed {c}, built {}", o.area));
                    }
                }
                if let Some((lo, hi)) = closed.bounds {
                    for k in 0..3 {
                        // No point outside the true bounds; and the build reaches them within
                        // twice its error (a sample can miss an extreme by the sag).
                        if o.lo[k] < lo[k] - hair - 1e-12 * h || o.hi[k] > hi[k] + hair + 1e-12 * h {
                            gather(&mut faults, w.name(), format!("{what}: axis {k}: built {}..{} outside the closed bounds {}..{}", o.lo[k], o.hi[k], lo[k], hi[k]));
                        }
                        if o.lo[k] - lo[k] > 2.0 * h + hair || hi[k] - o.hi[k] > 2.0 * h + hair {
                            gather(&mut faults, w.name(), format!("{what}: axis {k}: built {}..{} short of the closed bounds {}..{} by more than twice its error {h}", o.lo[k], o.hi[k], lo[k], hi[k]));
                        }
                    }
                }
                if let Some(c) = closed.centroid {
                    for k in 0..3 {
                        if c[k] < o.lo[k] - 2.0 * h - hair || c[k] > o.hi[k] + 2.0 * h + hair {
                            gather(&mut faults, w.name(), format!("{what}: centroid {c:?} outside the build's box {:?}..{:?}", o.lo, o.hi));
                            break;
                        }
                    }
                }
                // It stands on y = 0.
                if o.lo[1].abs() > 2.0 * h + hair {
                    gather(&mut faults, w.name(), format!("{what}: its lowest point is at y = {} (error {h})", o.lo[1]));
                }
                // A ball's points lie on the ball, and no face sinks deeper than the error.
                if ["sphere", "icosphere", "cube_sphere"].contains(&w.form.name) && w.alias.is_none() {
                    let r = s.args.f("radius");
                    let c = [0.0, r, 0.0];
                    let off = b.points.iter().map(|p| (norm(sub(*p, c)) - r).abs()).fold(0.0f64, f64::max);
                    if off > 1e-9 * r {
                        gather(&mut faults, w.name(), format!("{what}: a point {off} off the ball"));
                    }
                    let deep = b
                        .faces
                        .iter()
                        .map(|f| {
                            let k = f.verts.len() as f64;
                            let g = f.verts.iter().fold([0.0; 3], |g, &v| [g[0] + b.points[v as usize][0] / k, g[1] + b.points[v as usize][1] / k, g[2] + b.points[v as usize][2] / k]);
                            r - norm(sub(g, c))
                        })
                        .fold(0.0f64, f64::max);
                    if deep > h * (1.0 + 1e-6) + 1e-12 * r {
                        gather(&mut faults, w.name(), format!("{what}: a face lies {deep} under the ball, and the error says {h}"));
                    }
                }
                // An ellipsoid's and a superellipsoid's normals are their surface's own: along the
                // gradient of |x/rx|^n + |(y − ry)/ry|^n + |z/rz|^n (n = 2 for the ellipsoid).
                if ["ellipsoid", "superellipsoid"].contains(&w.form.name) && w.alias.is_none() {
                    let rr = [s.args.f("rx"), s.args.f("ry"), s.args.f("rz")];
                    let n = if w.form.name == "ellipsoid" { 2.0 } else { s.args.f("exponent") };
                    let g = |x: f64, r: f64| {
                        if x == 0.0 {
                            return 0.0;
                        }
                        let a = dmath::exp_f64((n - 1.0) * dmath::ln_f64((x / r).abs())) / r;
                        if x < 0.0 {
                            -a
                        } else {
                            a
                        }
                    };
                    let mut worst = 0.0f64;
                    for f in &b.faces {
                        if let Some(ns) = &f.normals {
                            for (nn, &v) in ns.iter().zip(&f.verts) {
                                let p = b.points[v as usize];
                                let gr = [g(p[0], rr[0]), g(p[1] - rr[1], rr[1]), g(p[2], rr[2])];
                                let l = norm(gr);
                                if l > 0.0 {
                                    worst = worst.max(norm(sub(*nn, [gr[0] / l, gr[1] / l, gr[2] / l])));
                                }
                            }
                        }
                    }
                    if worst > 1e-6 {
                        gather(&mut faults, w.name(), format!("{what}: a normal is {worst} from the surface's own"));
                    }
                }
                // Seven times as large is the same shape seven times as large.
                if detail == 9 {
                    for k in [7.0f64, 1.0 / 7.0] {
                        let scaled: Vec<(&str, f64)> = given.iter().zip(&params).map(|(&(n, v), p)| (n, if p.unit == Unit::Metres { v * k } else { v })).collect();
                        if scaled.iter().zip(&params).any(|(&(_, v), p)| v < p.min || v > p.max) {
                            continue;
                        }
                        match Spec::of(w, &scaled).and_then(|s2| s2.build(detail)) {
                            Ok(Built::Mesh(m2)) => {
                                let same_faces = m2.faces.len() == b.faces.len() && m2.faces.iter().zip(&b.faces).all(|(x, y)| x.verts == y.verts);
                                let far = if m2.points.len() == b.points.len() { m2.points.iter().zip(&b.points).map(|(x, y)| norm(sub(*x, [y[0] * k, y[1] * k, y[2] * k]))).fold(0.0f64, f64::max) } else { f64::MAX };
                                if !same_faces || far > 1e-8 * o.reach * k {
                                    gather(&mut faults, w.name(), format!("{what}: {k} times as large is another mesh (same faces: {same_faces}; a point {far} from where it should be, reach {})", o.reach * k));
                                }
                            }
                            other => gather(&mut faults, w.name(), format!("{what}: built, and {k} times as large it is {:?}", other.map(|_| "something else").map_err(|e| e.to_string()))),
                        }
                    }
                }
            }
        }
    }
    assert!(faults.is_empty(), "{} faults ({built} built, {refused} refused):\n{}", faults.len(), faults.join("\n"));
    assert!(built > least, "only {built} builds ({refused} refused)");
}

// ---------------------------------------------------------------- the operators

/// The operators on every plane figure: a prism's volume is the figure's area times its height,
/// a pyramid's a third of that, a turned figure's by Pappus from the figure's own area and
/// centroid (to `1e-12`: nothing in Pappus's product cancels, and the solid's closed volume is
/// taken so that nothing does); each well made by my own check. The default run: three sets a
/// figure.
#[test]
fn operators_by_checks_of_my_own() {
    operators(sets(3), 100);
}

/// The full run: twelve sets a figure, as the review drew.
#[test]
#[ignore = "the full corner draw of the operators on every plane figure (12 sets): a minute in the gate's build"]
fn operators_full() {
    operators(sets(12), 300);
}

fn operators(n_sets: usize, least: usize) {
    let mut faults: Vec<String> = Vec::new();
    let (mut built, mut refused) = (0usize, 0usize);
    for f in forms().iter().filter(|f| f.kind == Kind::Plane) {
        let w = Found { form: f, alias: None };
        let params = params_of(&w);
        let mut seq = Seq(seed(f.name) ^ 0x5555);
        for set in 0..n_sets {
            let given: Vec<(&str, f64)> = if set == 0 { Vec::new() } else { params.iter().map(|p| (p.name, draw(p, &mut seq, set))).collect() };
            let Ok(fig) = Spec::of(w, &given) else { continue };
            let Ok(plane) = fig.closed() else { continue };
            let height = dmath::exp_f64(dmath::ln_f64(1e-3) + seq.next() * (dmath::ln_f64(1e3) - dmath::ln_f64(1e-3)));
            let width = plane.bounds.map_or(1.0, |(lo, hi)| (hi[0] - lo[0]).max(hi[1] - lo[1]));
            let offset = plane.bounds.map_or(1.0, |(lo, _)| -lo[0] + width * (0.01 + 3.0 * seq.next())).min(1e4);
            let angle = if set % 3 == 0 { 360.0 } else { 1.0 + 359.0 * seq.next() };
            let made = [("prism_of", ops::prism_of(&fig, height)), ("pyramid_of", ops::pyramid_of(&fig, height)), ("revolve_of", ops::revolve_of(&fig, offset, angle))];
            for (op, solid) in made {
                let what = format!("{op} {} {given:?} height {height} offset {offset} angle {angle}", f.name);
                let solid = match solid {
                    Ok(s) => s,
                    Err(Error::Constraint { .. }) => {
                        refused += 1;
                        continue;
                    }
                    Err(e) => {
                        faults.push(format!("{what}: {e}"));
                        continue;
                    }
                };
                for detail in [8u32, 30] {
                    let mesh = match solid.build(detail) {
                        Ok(m) => m,
                        Err(Error::Constraint { rule, .. }) if rule == "not_well_made" || rule == "normal_behind_face" => {
                            faults.push(format!("{what} at {detail}: refused as {rule}"));
                            continue;
                        }
                        Err(Error::Constraint { .. } | Error::TooManyPoints { .. }) => {
                            refused += 1;
                            continue;
                        }
                        Err(e) => {
                            faults.push(format!("{what} at {detail}: {e}"));
                            continue;
                        }
                    };
                    built += 1;
                    let o = match own(&mesh, solid.genus()) {
                        Ok(o) => o,
                        Err(e) => {
                            faults.push(format!("{what} at {detail}: {e}"));
                            continue;
                        }
                    };
                    match solid.count(detail) {
                        Ok(c) if (c.points, c.parts) == (mesh.points.len() as u64, mesh.faces.len() as u64) => {}
                        other => faults.push(format!("{what} at {detail}: counted {other:?}, built {} and {}", mesh.points.len(), mesh.faces.len())),
                    }
                    let closed = solid.closed();
                    let h = mesh.chord_error;
                    if let Some(c) = closed.size {
                        let allowed = 2.0 * h * o.area + 1e-9 * c.abs() + 1e-9 * o.reach * o.reach * o.reach;
                        if over((c - o.volume).abs(), allowed) {
                            faults.push(format!("{what} at {detail}: volume closed {c}, built {} (allowed {allowed})", o.volume));
                        }
                        // What the closed form must be, from the figure's own.
                        let expect = match (op, plane.size, plane.centroid) {
                            ("prism_of", Some(a), _) => Some(a * height),
                            ("pyramid_of", Some(a), _) => Some(a * height / 3.0),
                            ("revolve_of", Some(a), Some(g)) => Some(a * (g[0] + offset) * angle.to_radians()),
                            _ => None,
                        };
                        if let Some(e) = expect {
                            if (c - e).abs() > 1e-12 * e.abs() && faults.iter().filter(|x| x.starts_with(&format!("{op} {} ", f.name)) && x.contains("own measures")).count() < 1 {
                                faults.push(format!("{what}: its closed volume is {c}, and the figure's own measures give {e} (off by {:e} of it)", (c - e).abs() / e.abs()));
                            }
                        }
                    }
                    if o.lo[1].abs() > 2.0 * h + 1e-9 * o.reach {
                        faults.push(format!("{what} at {detail}: its lowest point is at y = {}", o.lo[1]));
                    }
                }
            }
        }
    }
    faults.truncate(120);
    assert!(faults.is_empty(), "{} faults ({built} built, {refused} refused):\n{}", faults.len(), faults.join("\n"));
    assert!(built > least, "only {built} builds ({refused} refused)");
}

// ---------------------------------------------------------------- plane figures and curves

/// `(b − a) × (c − a)`, and its rounding: a few ε of the product of the two sides' sizes.
fn turn(a: Pt, b: Pt, c: Pt) -> (f64, f64) {
    let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]);
    (u[0] * v[1] - u[1] * v[0], 8.0 * f64::EPSILON * (u[0].abs() + u[1].abs()) * (v[0].abs() + v[1].abs()))
}

/// The sign of a turn past its rounding: 1, −1, or 0 where it is within it.
fn side(a: Pt, b: Pt, c: Pt) -> i32 {
    let (t, e) = turn(a, b, c);
    if t > e {
        1
    } else if t < -e {
        -1
    } else {
        0
    }
}

/// Whether two segments meet: they cross, or an end of one is on the other (as good as on its
/// line, within rounding, and inside its box). A well-made figure's edges that are not
/// neighbours are farther apart than that.
fn meet(p: Pt, q: Pt, r: Pt, s: Pt) -> bool {
    let (d1, d2, d3, d4) = (side(p, q, r), side(p, q, s), side(r, s, p), side(r, s, q));
    if d1 * d2 < 0 && d3 * d4 < 0 {
        return true;
    }
    let on = |a: Pt, b: Pt, c: Pt, d: i32| d == 0 && c[0] >= a[0].min(b[0]) && c[0] <= a[0].max(b[0]) && c[1] >= a[1].min(b[1]) && c[1] <= a[1].max(b[1]);
    on(p, q, r, d1) || on(p, q, s, d2) || on(r, s, p, d3) || on(r, s, q, d4)
}

/// Twice the signed area of a ring (the shoelace).
fn shoelace(r: &[Pt]) -> f64 {
    (0..r.len()).map(|i| r[i][0] * r[(i + 1) % r.len()][1] - r[(i + 1) % r.len()][0] * r[i][1]).sum()
}

/// Whether `p` is inside the ring (by the crossings of a ray to +x).
fn inside(p: Pt, r: &[Pt]) -> bool {
    let mut c = false;
    for i in 0..r.len() {
        let (a, b) = (r[i], r[(i + 1) % r.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            c = !c;
        }
    }
    c
}

/// An outline checked from its points alone: rings of three points or more, every point finite,
/// no edge as short as a billionth of its size, the first ring counter-clockwise and the rest
/// clockwise, no two edges meeting but neighbours at their common point (and those not folded
/// back on each other), every hole inside the outer ring. The edges are swept in order of their
/// least x, each compared with those whose x range it reaches. Its area (outer less holes) and
/// perimeter.
fn own_outline(rings: &[Vec<Pt>]) -> Result<(f64, f64), String> {
    if rings.is_empty() {
        return Err("no rings".into());
    }
    if !rings.iter().flatten().all(|p| p[0].is_finite() && p[1].is_finite()) {
        return Err("a point is not finite".into());
    }
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in rings.iter().flatten() {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let size = ((hi[0] - lo[0]) * (hi[0] - lo[0]) + (hi[1] - lo[1]) * (hi[1] - lo[1])).sqrt();
    let (mut area, mut perimeter) = (0.0, 0.0);
    let mut edges: Vec<(usize, usize, Pt, Pt)> = Vec::new();
    for (ri, r) in rings.iter().enumerate() {
        if r.len() < 3 {
            return Err(format!("ring {ri} has {} points", r.len()));
        }
        let a = 0.5 * shoelace(r);
        if (ri == 0) != (a > 0.0) {
            return Err(format!("ring {ri} is wound the wrong way (signed area {a})"));
        }
        area += a;
        for i in 0..r.len() {
            let (p, q) = (r[i], r[(i + 1) % r.len()]);
            let l = ((q[0] - p[0]) * (q[0] - p[0]) + (q[1] - p[1]) * (q[1] - p[1])).sqrt();
            if l <= 1e-9 * size {
                return Err(format!("ring {ri} edge {i} is {l} long, of a figure {size} across"));
            }
            perimeter += l;
            edges.push((ri, i, p, q));
        }
        if ri > 0 && !r.iter().any(|p| inside(*p, &rings[0])) {
            return Err(format!("hole {ri} is not inside the outer ring"));
        }
    }
    edges.sort_by(|a, b| a.2[0].min(a.3[0]).total_cmp(&b.2[0].min(b.3[0])));
    for x in 0..edges.len() {
        let (ra, ia, p, q) = edges[x];
        let right = p[0].max(q[0]);
        for &(rb, ib, r, s) in &edges[x + 1..] {
            if r[0].min(s[0]) > right {
                break;
            }
            let n = rings[ra].len();
            if ra == rb && (ia.abs_diff(ib) == 1 || ia.abs_diff(ib) == n - 1) {
                // Neighbours share a point; they may not fold back along each other.
                let ((a, b), (_, c)) = if (ia + 1) % n == ib { ((p, q), (r, s)) } else { ((r, s), (p, q)) };
                let (t, e) = turn(a, b, c);
                let back = (c[0] - b[0]) * (a[0] - b[0]) + (c[1] - b[1]) * (a[1] - b[1]);
                if t.abs() <= e && back > 0.0 {
                    return Err(format!("ring {ra}: edges {ia} and {ib} fold back on each other"));
                }
                continue;
            }
            if meet(p, q, r, s) {
                return Err(format!("ring {ra} edge {ia} meets ring {rb} edge {ib}"));
            }
        }
    }
    Ok((area, perimeter))
}

/// The default run: six sets a plane figure's or a curve's word.
#[test]
fn figures_and_curves_by_checks_of_my_own() {
    figures(sets(6), 1_000);
}

/// The full run: 24 sets a word.
#[test]
#[ignore = "the full corner draw of every plane figure's and curve's word (24 sets, four details each): a minute in the gate's build"]
fn figures_full() {
    figures(sets(24), 4_000);
}

fn figures(n_sets: usize, least: usize) {
    let mut faults: Vec<String> = Vec::new();
    let (mut built, mut refused) = (0usize, 0usize);
    let mut ws = words(Kind::Plane);
    ws.extend(words(Kind::Curve));
    for w in ws {
        let params = params_of(&w);
        let mut seq = Seq(seed(w.name()) ^ 0xA5A5);
        for set in 0..n_sets {
            let given: Vec<(&str, f64)> = params.iter().map(|p| (p.name, draw(p, &mut seq, set))).collect();
            let what = format!("{} {given:?}", w.name());
            let s = match Spec::of(w, &given) {
                Ok(s) => s,
                Err(e) => {
                    gather(&mut faults, w.name(), format!("{what}: inside its ranges, refused at once: {e}"));
                    continue;
                }
            };
            for detail in DETAILS {
                let what = format!("{what} at detail {detail}");
                let b = match s.build(detail) {
                    Ok(b) => b,
                    Err(Error::Constraint { rule: "not_well_made", .. }) => {
                        gather(&mut faults, w.name(), format!("{what}: refused as not well made"));
                        continue;
                    }
                    Err(Error::Constraint { .. } | Error::TooManyPoints { .. }) => {
                        refused += 1;
                        continue;
                    }
                    Err(e) => {
                        gather(&mut faults, w.name(), format!("{what}: {e}"));
                        continue;
                    }
                };
                built += 1;
                match s.count(detail) {
                    Ok(c) if (c.points, c.parts) == (b.points(), b.parts()) => {}
                    other => gather(&mut faults, w.name(), format!("{what}: counted {other:?}, built {} points in {} parts", b.points(), b.parts())),
                }
                if s.build(detail).ok().as_ref() != Some(&b) {
                    gather(&mut faults, w.name(), format!("{what}: built twice, not the same"));
                }
                let h = b.chord_error();
                if !(h.is_finite() && h >= 0.0) || (!w.form.curved && h != 0.0) {
                    gather(&mut faults, w.name(), format!("{what}: chord error {h} (curved: {})", w.form.curved));
                    continue;
                }
                let closed = match s.closed() {
                    Ok(c) => c,
                    Err(e) => {
                        gather(&mut faults, w.name(), format!("{what}: built, and its closed forms refused: {e}"));
                        continue;
                    }
                };
                let points: Vec<Pt> = match &b {
                    Built::Outline(o) => o.rings.iter().flatten().copied().collect(),
                    Built::Curve(c) => c.points.clone(),
                    _ => {
                        gather(&mut faults, w.name(), format!("{what}: a figure built to a solid"));
                        continue;
                    }
                };
                let reach = points.iter().flatten().fold(0.0f64, |a, x| a.max(x.abs()));
                let round = 64.0 * points.len() as f64 * f64::EPSILON * reach;
                match &b {
                    Built::Outline(o) => {
                        let (area, perimeter) = match own_outline(&o.rings) {
                            Ok(x) => x,
                            Err(e) => {
                                gather(&mut faults, w.name(), format!("{what}: {e}"));
                                continue;
                            }
                        };
                        // The area within a strip as wide as the error along the perimeter.
                        if let Some(c) = closed.size {
                            let allowed = 2.0 * h * perimeter + round * reach + 1e-9 * c.abs();
                            if over((c - area).abs(), allowed) {
                                gather(&mut faults, w.name(), format!("{what}: area closed {c}, built {area} (allowed {allowed})"));
                            }
                        }
                    }
                    Built::Curve(c) => {
                        if c.points.len() < 2 || !c.points.iter().all(|p| p[0].is_finite() && p[1].is_finite()) {
                            gather(&mut faults, w.name(), format!("{what}: {} points, or one not finite", c.points.len()));
                            continue;
                        }
                        let n = c.points.len();
                        let steps = if c.closed { n } else { n - 1 };
                        let mut length = 0.0;
                        for i in 0..steps {
                            let (p, q) = (c.points[i], c.points[(i + 1) % n]);
                            let l = ((q[0] - p[0]) * (q[0] - p[0]) + (q[1] - p[1]) * (q[1] - p[1])).sqrt();
                            if l <= 1e-9 * reach {
                                gather(&mut faults, w.name(), format!("{what}: step {i} is {l} long"));
                            }
                            length += l;
                        }
                        // The chords of a curve are no longer than the curve.
                        if let Some(cl) = closed.boundary {
                            if length > cl * (1.0 + 1e-9) + round {
                                gather(&mut faults, w.name(), format!("{what}: the chords ({length}) are longer than the curve ({cl})"));
                            }
                        }
                    }
                    _ => {}
                }
                // Within the closed bounds, and reaching them within twice the error.
                if let Some((lo, hi)) = closed.bounds {
                    let (mut blo, mut bhi) = ([f64::MAX; 2], [f64::MIN; 2]);
                    for p in &points {
                        for k in 0..2 {
                            blo[k] = blo[k].min(p[k]);
                            bhi[k] = bhi[k].max(p[k]);
                        }
                    }
                    let hair = 1e-9 * reach + round;
                    for k in 0..2 {
                        if blo[k] < lo[k] - hair || bhi[k] > hi[k] + hair || blo[k] - lo[k] > 2.0 * h + hair || hi[k] - bhi[k] > 2.0 * h + hair {
                            gather(&mut faults, w.name(), format!("{what}: axis {k}: built {}..{}, the closed bounds {}..{}, error {h}", blo[k], bhi[k], lo[k], hi[k]));
                        }
                    }
                }
            }
        }
    }
    assert!(faults.is_empty(), "{} faults ({built} built, {refused} refused):\n{}", faults.len(), faults.join("\n"));
    assert!(built > least, "only {built} builds ({refused} refused)");
}

/// Whether `x` is over `limit`, or not a number at all.
fn over(x: f64, limit: f64) -> bool {
    x.is_nan() || x > limit
}

// ---------------------------------------------------------------- shallow arcs

/// What rounding may move a point that was computed by a few operations from numbers no larger
/// than the shape's reach: `1e-12` of the reach (some 4,500 ε). A point placed from a centre far
/// beyond the shape (the arc of a flat lens has its centre up to 1e12 m away) is off by ε times
/// that distance, far over this; a point placed from the shape's own numbers is well under it.
const TIGHT: f64 = 1e-12;

/// Every curved word with each of its measured parameters at its least or its greatest, all
/// together (at most 64 sets a word), at details 8, 9 and 30: the corners where an arc is
/// shallowest (a radius over its chord by up to 1e8, a centre far off the shape). Each build is
/// refused as data or lands on its exact bounds to rounding (`TIGHT`, not the hair of the corner
/// draw above: a rim at its radius, a pole on its axis), lies within them, and holds its closed
/// area or volume within its own chord error over its boundary, so that an error said to be small
/// is small (an arc's sag that rounded to nothing failed here).
#[test]
fn shallow_arcs_land_on_their_ends() {
    shallow(&[9], 400);
}

/// The full run: details 8, 9 and 30.
#[test]
#[ignore = "the shallow-arc corners at three details: a minute and more in the gate's build"]
fn shallow_full() {
    shallow(&[8, 9, 30], 1_000);
}

fn shallow(details: &[u32], least: usize) {
    let mut faults: Vec<String> = Vec::new();
    let (mut built, mut refused) = (0usize, 0usize);
    let mut ws = words(Kind::Solid);
    ws.extend(words(Kind::Plane));
    // The review's own case first, and then the corners.
    let mut cases: Vec<(Found, Vec<(&str, f64)>)> = vec![(find("biconvex_lens", Some(Kind::Solid)).expect("the lens"), vec![("radius", 4785.767163963165), ("thickness", 1e-4)])];
    for w in ws.into_iter().filter(|w| w.form.curved) {
        let params = params_of(&w);
        let free: Vec<usize> = (0..params.len()).filter(|&i| !params[i].integer).take(6).collect();
        for bits in 0..(1u32 << free.len()) {
            let given: Vec<(&str, f64)> = params
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let v = match free.iter().position(|&j| j == i) {
                        Some(k) if bits >> k & 1 == 1 => p.max,
                        Some(_) => p.min,
                        None => p.default,
                    };
                    (p.name, v)
                })
                .collect();
            cases.push((w, given));
        }
    }
    for (w, given) in cases {
        let what = format!("{} {given:?}", w.name());
        let Ok(s) = Spec::of(w, &given) else {
            refused += 1;
            continue;
        };
        let Ok(closed) = s.closed() else { continue };
        for &detail in details {
            let what = format!("{what} at detail {detail}");
            let b = match s.build(detail) {
                Ok(b) => b,
                Err(Error::Constraint { rule, .. }) if forbidden(&w, rule) => {
                    gather(&mut faults, w.name(), format!("{what}: refused as {rule}"));
                    continue;
                }
                Err(Error::Constraint { .. } | Error::TooManyPoints { .. }) => {
                    refused += 1;
                    continue;
                }
                Err(e) => {
                    gather(&mut faults, w.name(), format!("{what}: {e}"));
                    continue;
                }
            };
            built += 1;
            let h = b.chord_error();
            let (points, size, boundary): (Vec<P3>, f64, f64) = match &b {
                Built::Mesh(m) => match own(m, s.genus().unwrap_or(0)) {
                    Ok(o) => (m.points.clone(), o.volume, o.area),
                    Err(e) => {
                        gather(&mut faults, w.name(), format!("{what}: {e}"));
                        continue;
                    }
                },
                Built::Outline(o) => match own_outline(&o.rings) {
                    Ok((a, p)) => (o.rings.iter().flatten().map(|q| [q[0], q[1], 0.0]).collect(), a, p),
                    Err(e) => {
                        gather(&mut faults, w.name(), format!("{what}: {e}"));
                        continue;
                    }
                },
                _ => continue,
            };
            let reach = points.iter().flatten().fold(0.0f64, |a, x| a.max(x.abs()));
            let tight = TIGHT * reach;
            if let Some((lo, hi)) = closed.bounds {
                let dims = if matches!(b, Built::Mesh(_)) { 3 } else { 2 };
                for k in 0..dims {
                    let blo = points.iter().map(|p| p[k]).fold(f64::MAX, f64::min);
                    let bhi = points.iter().map(|p| p[k]).fold(f64::MIN, f64::max);
                    if blo < lo[k] - tight || bhi > hi[k] + tight {
                        gather(&mut faults, w.name(), format!("{what}: axis {k}: built {blo}..{bhi}, past the closed bounds {}..{} (by {:e}; rounding allows {tight:e})", lo[k], hi[k], (lo[k] - blo).max(bhi - hi[k])));
                    }
                    if blo - lo[k] > 2.0 * h + tight || hi[k] - bhi > 2.0 * h + tight {
                        gather(&mut faults, w.name(), format!("{what}: axis {k}: built {blo}..{bhi}, short of the closed bounds {}..{} by more than twice its error {h}", lo[k], hi[k]));
                    }
                }
            }
            if let Some(c) = closed.size {
                let allowed = (2.0 * h + tight) * boundary + 1e-9 * c.abs();
                if over((c - size).abs(), allowed) {
                    gather(&mut faults, w.name(), format!("{what}: closed {c}, built {size} (off by {:e}; its error {h:e} allows {allowed:e})", (c - size).abs()));
                }
            }
        }
    }
    assert!(faults.is_empty(), "{} faults ({built} built, {refused} refused):\n{}", faults.len(), faults.join("\n"));
    assert!(built > least, "only {built} builds ({refused} refused)");
}

// ---------------------------------------------------------------- raised figures

/// Whether a solid stands over its plan as a graph: over each of a few points of its plan, the
/// faces that are not walls are met by a vertical line once going up (a face whose normal points
/// up: the top) and once going down (the bottom), or not at all. Points within a hair of a face's
/// edge in plan are passed over. From the points alone; `Err` says where it does not hold.
fn graph_over_plan(mesh: &Mesh, seq: &mut Seq) -> Result<(), String> {
    let tris: Vec<[P3; 3]> = mesh.triangles().iter().map(|(_, t)| t.map(|v| mesh.points[v as usize])).collect();
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in &mesh.points {
        for (k, c) in [p[0], -p[2]].into_iter().enumerate() {
            lo[k] = lo[k].min(c);
            hi[k] = hi[k].max(c);
        }
    }
    let size = ((hi[0] - lo[0]) * (hi[0] - lo[0]) + (hi[1] - lo[1]) * (hi[1] - lo[1])).sqrt();
    let plan = |p: P3| [p[0], -p[2]];
    for _ in 0..24 {
        let q = [lo[0] + (hi[0] - lo[0]) * seq.next(), lo[1] + (hi[1] - lo[1]) * seq.next()];
        let (mut up, mut down, mut near) = (0, 0, false);
        for t in &tris {
            let [a, b, c] = t.map(plan);
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if area.abs() <= 1e-24 * size * size {
                continue;
            }
            let sides = [(a, b), (b, c), (c, a)].map(|(u, v)| {
                let l = ((v[0] - u[0]) * (v[0] - u[0]) + (v[1] - u[1]) * (v[1] - u[1])).sqrt();
                ((v[0] - u[0]) * (q[1] - u[1]) - (v[1] - u[1]) * (q[0] - u[0])) / l.max(f64::MIN_POSITIVE)
            });
            let inside = if area > 0.0 { sides.iter().all(|s| *s > 0.0) } else { sides.iter().all(|s| *s < 0.0) };
            let close = sides.iter().any(|s| s.abs() <= 1e-9 * size);
            if close {
                near = true;
            }
            if inside {
                if area > 0.0 {
                    up += 1;
                } else {
                    down += 1;
                }
            }
        }
        if near {
            continue;
        }
        if up != down || up > 1 {
            return Err(format!("over the plan point {q:?} a vertical line meets {up} faces going up and {down} going down"));
        }
    }
    Ok(())
}

/// Every closed plane figure's word raised by each of the five raised forms, the figure's
/// arguments and the raised form's drawn at their corners together (lengths at the least or
/// the greatest, or drawn against the figure's own size), at details 8, 9, 30 and 101: each is
/// refused as data or is closed and wound outward by my own check, of the genus its holes give
/// (none for a relief), a graph over its plan (not tangled), standing on y = 0, within its closed
/// bounds and reaching them within twice its error, of its count, the same built twice, and its
/// closed volume (where there is one: the slab's and the relief's for any figure, every raised
/// form's for a figure of straight sides) within its error times its surface.
#[test]
fn raised_by_checks_of_my_own() {
    raised(sets(1), &[8, 30], 150);
}

/// The full run: eight sets a figure.
#[test]
#[ignore = "the full corner draw of the raised forms on every plane figure (8 sets): minutes in the gate's build"]
fn raised_full() {
    raised(sets(8), &DETAILS, 1_500);
}

fn raised(n_sets: usize, details: &[u32], least: usize) {
    let mut faults: Vec<String> = Vec::new();
    let (mut built, mut refused) = (0usize, 0usize);
    let lp = Param { name: "length", about: "", unit: Unit::Metres, default: 0.1, min: 1e-4, max: 1e4, integer: false };
    for w in words(Kind::Plane) {
        let params = params_of(&w);
        let mut seq = Seq(seed(w.name()) ^ 0x7777);
        for set in 0..n_sets {
            let given: Vec<(&str, f64)> = if set == 0 { Vec::new() } else { params.iter().map(|p| (p.name, draw(p, &mut seq, set))).collect() };
            let Ok(fig) = Spec::of(w, &given) else { continue };
            let Ok(plane) = fig.closed() else { continue };
            let holes = match fig.build(64) {
                Ok(Built::Outline(o)) => o.rings.len() as u32 - 1,
                _ => continue,
            };
            let size = plane.bounds.map_or(1.0, |(lo, hi)| (hi[0] - lo[0]).max(hi[1] - lo[1]));
            // A length: at the ranges' ends, or against the figure's size.
            let l = |seq: &mut Seq| if seq.next() < 0.4 { draw(&lp, seq, set) } else { (size * dmath::exp_f64(dmath::ln_f64(1e-3) + seq.next() * dmath::ln_f64(1e3))).clamp(1e-4, 1e4) };
            let (t, a, b) = (l(&mut seq), l(&mut seq), l(&mut seq));
            let steps = 1 + (seq.next() * 6.0) as u32;
            let (round, both) = (seq.next() < 0.5, seq.next() < 0.5);
            let made = [
                ("slab", raise::slab(&fig, t), holes),
                ("bevelled", raise::bevelled(&fig, t, a.min(t), round, both), holes),
                ("pillow", raise::pillow(&fig, t, a, b), holes),
                ("stepped", raise::stepped(&fig, steps, a, b), holes),
                ("relief", raise::relief(&fig, t, a, b), 0),
            ];
            for (op, solid, genus) in made {
                let what = format!("{op} of {} {given:?} (t {t}, a {a}, b {b}, steps {steps}, round {round}, both {both})", w.name());
                let solid = match solid {
                    Ok(s) => s,
                    Err(Error::Constraint { .. }) => {
                        refused += 1;
                        continue;
                    }
                    Err(e) => {
                        gather(&mut faults, op, format!("{what}: {e}"));
                        continue;
                    }
                };
                if solid.genus() != genus {
                    gather(&mut faults, op, format!("{what}: genus {} for {holes} holes", solid.genus()));
                }
                for &detail in details {
                    let what = format!("{what} at detail {detail}");
                    let mesh = match solid.build(detail) {
                        Ok(m) => m,
                        Err(Error::Constraint { rule, .. }) if rule == "not_well_made" || rule == "normal_behind_face" => {
                            gather(&mut faults, op, format!("{what}: refused as {rule}"));
                            continue;
                        }
                        Err(Error::Constraint { .. } | Error::TooManyPoints { .. }) => {
                            refused += 1;
                            continue;
                        }
                        Err(e) => {
                            gather(&mut faults, op, format!("{what}: {e}"));
                            continue;
                        }
                    };
                    built += 1;
                    let o = match own(&mesh, genus) {
                        Ok(o) => o,
                        Err(e) => {
                            gather(&mut faults, op, format!("{what}: {e}"));
                            continue;
                        }
                    };
                    if mesh.faces.len() <= 20_000 {
                        if let Err(e) = graph_over_plan(&mesh, &mut seq) {
                            gather(&mut faults, op, format!("{what}: {e}"));
                        }
                    }
                    match solid.count(detail) {
                        Ok(c) if (c.points, c.parts) == (mesh.points.len() as u64, mesh.faces.len() as u64) => {}
                        other => gather(&mut faults, op, format!("{what}: counted {other:?}, built {} and {}", mesh.points.len(), mesh.faces.len())),
                    }
                    if detail == 9 && solid.build(detail).ok().as_ref() != Some(&mesh) {
                        gather(&mut faults, op, format!("{what}: built twice, not the same"));
                    }
                    let h = mesh.chord_error;
                    let hair = 1e-9 * o.reach + 64.0 * mesh.points.len() as f64 * f64::EPSILON * o.reach;
                    let closed = solid.closed();
                    if let Some(c) = closed.size {
                        let allowed = 2.0 * h * o.area + 1e-9 * c.abs() + hair * o.reach * o.reach;
                        if over((c - o.volume).abs(), allowed) {
                            gather(&mut faults, op, format!("{what}: volume closed {c}, built {} (allowed {allowed})", o.volume));
                        }
                    }
                    if let (Some(c), true) = (closed.boundary, h == 0.0) {
                        if over((c - o.area).abs(), 1e-9 * c.abs() + hair * o.reach) {
                            gather(&mut faults, op, format!("{what}: area closed {c}, built {}", o.area));
                        }
                    }
                    if let Some((lo, hi)) = closed.bounds {
                        for k in 0..3 {
                            if o.lo[k] < lo[k] - hair - 2.0 * h || o.hi[k] > hi[k] + hair + 2.0 * h || o.lo[k] - lo[k] > 2.0 * h + hair || hi[k] - o.hi[k] > 2.0 * h + hair {
                                gather(&mut faults, op, format!("{what}: axis {k}: built {}..{}, closed {}..{}, error {h}", o.lo[k], o.hi[k], lo[k], hi[k]));
                            }
                        }
                    }
                    if o.lo[1].abs() > hair {
                        gather(&mut faults, op, format!("{what}: its lowest point is at y = {}", o.lo[1]));
                    }
                }
            }
        }
    }
    faults.truncate(120);
    assert!(faults.is_empty(), "{} faults ({built} built, {refused} refused):\n{}", faults.len(), faults.join("\n"));
    assert!(built > least, "only {built} builds ({refused} refused)");
}

// ---------------------------------------------------------------- drawings

/// A drawing checked from the mesh's points and the view's rows alone: its polygons convex,
/// counter-clockwise and finite, farthest first, shades in 0..1; their areas summing to the
/// silhouette's; and at points of the picture drawn from `seq`, the face a ray cast from the
/// viewer meets first (by my own barycentric test over every triangle) is the face of the one
/// polygon that holds the point, no polygon holding a point the ray misses, and the silhouette
/// holding a point exactly when the ray meets the solid. Points within a hair of an edge (of a
/// triangle or a polygon) or of the silhouette are passed over.
fn check_drawing(mesh: &Mesh, d: &specula_formae::draw::Drawing, rows: [P3; 3], seq: &mut Seq, samples: usize) -> Result<(), String> {
    let proj = |p: P3| -> (Pt, f64) { ([dot(rows[0], p), dot(rows[1], p)], dot(rows[2], p)) };
    let tris: Vec<(u32, [(Pt, f64); 3])> = mesh.triangles().iter().map(|(f, t)| (*f, t.map(|v| proj(mesh.points[v as usize])))).collect();
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for (_, t) in &tris {
        for (p, _) in t {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    let size = ((hi[0] - lo[0]) * (hi[0] - lo[0]) + (hi[1] - lo[1]) * (hi[1] - lo[1])).sqrt();
    let hair = 1e-9 * size;
    let len2 = |a: Pt, b: Pt| ((b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1])).sqrt();
    let mut sum = 0.0;
    for (i, p) in d.polygons.iter().enumerate() {
        let k = p.points.len();
        if k < 3 || !p.points.iter().all(|q| q[0].is_finite() && q[1].is_finite()) {
            return Err(format!("polygon {i} has {k} points, or one not finite"));
        }
        for c in 0..k {
            let (a, b, e) = (p.points[c], p.points[(c + 1) % k], p.points[(c + 2) % k]);
            let t = (b[0] - a[0]) * (e[1] - a[1]) - (b[1] - a[1]) * (e[0] - a[0]);
            if t < -1e-12 * size * size {
                return Err(format!("polygon {i} turns clockwise at its corner {c}"));
            }
        }
        if !(0.0..=1.0).contains(&p.shade) {
            return Err(format!("polygon {i} has the shade {}", p.shade));
        }
        if i > 0 && p.depth > d.polygons[i - 1].depth {
            return Err(format!("polygon {i} is farther than the one before it"));
        }
        sum += 0.5 * shoelace(&p.points);
    }
    let has_outline = d.silhouette_refused.is_none();
    let sil: f64 = d.silhouette.iter().map(|r| 0.5 * shoelace(r)).sum();
    if has_outline && (sum - sil).abs() > 1e-8 * size * size {
        return Err(format!("the polygons' areas sum to {sum} and the silhouette's to {sil}"));
    }
    // Inside the rings (even-odd over all of them), and how near they come.
    let in_rings = |q: Pt, rings: &[Vec<Pt>]| -> (bool, f64) {
        let mut inside = false;
        let mut near = f64::MAX;
        for r in rings {
            for i in 0..r.len() {
                let (a, b) = (r[i], r[(i + 1) % r.len()]);
                if (a[1] > q[1]) != (b[1] > q[1]) && q[0] < a[0] + (q[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                    inside = !inside;
                }
                let e = [b[0] - a[0], b[1] - a[1]];
                let l2 = e[0] * e[0] + e[1] * e[1];
                let t = (((q[0] - a[0]) * e[0] + (q[1] - a[1]) * e[1]) / l2).clamp(0.0, 1.0);
                let (dx, dy) = (a[0] + e[0] * t - q[0], a[1] + e[1] * t - q[1]);
                near = near.min((dx * dx + dy * dy).sqrt());
            }
        }
        (inside, near)
    };
    for _ in 0..samples {
        let q = [lo[0] + (hi[0] - lo[0]) * seq.next(), lo[1] + (hi[1] - lo[1]) * seq.next()];
        // The ray: the nearest triangle over q.
        let mut best: Option<(f64, u32)> = None;
        let mut ambiguous = false;
        for (f, t) in &tris {
            let [(a, za), (b, zb), (c, zc)] = *t;
            let den = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if den.abs() <= 1e-20 * size * size {
                continue;
            }
            let l1 = ((b[0] - q[0]) * (c[1] - q[1]) - (b[1] - q[1]) * (c[0] - q[0])) / den;
            let l2 = ((c[0] - q[0]) * (a[1] - q[1]) - (c[1] - q[1]) * (a[0] - q[0])) / den;
            let l3 = 1.0 - l1 - l2;
            // Distances to the triangle's sides: each λ times its corner's height over the side.
            let ds = [l1 * den.abs() / len2(b, c), l2 * den.abs() / len2(c, a), l3 * den.abs() / len2(a, b)];
            if ds.iter().all(|x| *x > -hair) && ds.iter().any(|x| x.abs() <= hair) {
                ambiguous = true;
            }
            if ds.iter().all(|x| *x > hair) {
                let z = l1 * za + l2 * zb + l3 * zc;
                if best.is_none_or(|(bz, _)| z < bz) {
                    best = Some((z, *f));
                }
            }
        }
        if ambiguous {
            continue;
        }
        let mut holders: Vec<u32> = Vec::new();
        let mut near_piece = false;
        for p in &d.polygons {
            let k = p.points.len();
            let dists: Vec<f64> = (0..k)
                .map(|c| {
                    let (a, b) = (p.points[c], p.points[(c + 1) % k]);
                    ((b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0])) / len2(a, b).max(f64::MIN_POSITIVE)
                })
                .collect();
            if dists.iter().all(|x| *x > -hair) {
                if dists.iter().any(|x| *x <= hair) {
                    near_piece = true;
                }
                holders.push(p.face);
            }
        }
        if near_piece {
            continue;
        }
        let (in_sil, near_sil) = in_rings(q, &d.silhouette);
        match best {
            Some((_, f)) => {
                if holders != [f] {
                    return Err(format!("at {q:?} the ray meets face {f} first, and the drawing holds the point in the polygons of faces {holders:?}"));
                }
                if has_outline && !in_sil && near_sil > hair {
                    return Err(format!("at {q:?} the ray meets the solid, and the silhouette does not hold the point"));
                }
            }
            None => {
                if !holders.is_empty() {
                    return Err(format!("at {q:?} the ray meets nothing, and the drawing holds the point in faces {holders:?}"));
                }
                if has_outline && in_sil && near_sil > hair {
                    return Err(format!("at {q:?} the ray meets nothing, and the silhouette holds the point"));
                }
            }
        }
    }
    Ok(())
}

/// Every solid word, and raised figures with holes or hollows, drawn in every named view and two
/// free ones, each checked by [`check_drawing`]. The default run draws each word at its defaults
/// at detail 9.
#[test]
fn drawings_by_checks_of_my_own() {
    drawings(1, &[9], 48, 1_000);
}

/// The full run: two sets a word (its defaults and a corner draw) at details 9 and 30, with more
/// points a drawing.
#[test]
#[ignore = "the full draw of every solid word in every view (two sets, two details): minutes in the gate's build"]
fn drawings_full() {
    drawings(sets(2), &[9, 30], 160, 3_000);
}

fn drawings(n_sets: usize, details: &[u32], samples: usize, least: usize) {
    use specula_formae::draw::{draw as picture, views, Projection};
    let mut faults: Vec<String> = Vec::new();
    let (mut drawn, mut refused, mut no_outline) = (0usize, 0usize, 0usize);
    let mut subjects: Vec<(String, Mesh)> = Vec::new();
    for w in words(Kind::Solid) {
        let params = params_of(&w);
        let mut seq = Seq(seed(w.name()) ^ 0x3C3C);
        for set in 0..n_sets {
            let given: Vec<(&str, f64)> = if set == 0 { Vec::new() } else { params.iter().map(|p| (p.name, draw(p, &mut seq, set))).collect() };
            let Ok(s) = Spec::of(w, &given) else { continue };
            for &detail in details {
                if let Ok(Built::Mesh(m)) = s.build(detail) {
                    subjects.push((format!("{} {given:?} at {detail}", w.name()), m));
                }
            }
        }
    }
    for name in ["annulus", "frame", "star", "polygon_ring", "cross", "heart"] {
        let fig = Spec::new(name, Some(Kind::Plane), &[]).expect("a figure");
        let made = [("slab", raise::slab(&fig, 0.2)), ("stepped", raise::stepped(&fig, 3, 0.1, 0.03)), ("relief", raise::relief(&fig, 0.15, 0.1, 0.05)), ("pillow", raise::pillow(&fig, 0.1, 0.2, 0.04))];
        for (op, s) in made {
            if let Ok(m) = s.and_then(|s| s.build(details[0])) {
                subjects.push((format!("{op} of {name}"), m));
            }
        }
    }
    for (name, mesh) in &subjects {
        let mut seq = Seq(seed(name));
        let mut projections: Vec<(String, Projection)> = views().iter().map(|v| (v.name.to_string(), v.projection)).collect();
        for _ in 0..2 {
            let (a, e) = (-180.0 + 360.0 * seq.next(), -89.0 + 178.0 * seq.next());
            projections.push((format!("free {a} {e}"), Projection::Orthographic { azimuth: a, elevation: e }));
        }
        for (vname, p) in projections {
            let what = format!("{name} in {vname}");
            let d = match picture(mesh, p) {
                Ok(d) => d,
                Err(Error::Constraint { rule: "drawing_too_large", .. }) => {
                    refused += 1;
                    continue;
                }
                Err(e) => {
                    gather(&mut faults, name, format!("{what}: {e}"));
                    continue;
                }
            };
            drawn += 1;
            if d.silhouette_refused.is_some() {
                no_outline += 1;
            }
            if let Err(e) = check_drawing(mesh, &d, p.rows(), &mut seq, samples) {
                gather(&mut faults, name, format!("{what}: {e}"));
            }
        }
    }
    faults.truncate(80);
    assert!(faults.is_empty(), "{} faults ({drawn} drawn, {refused} refused):\n{}", faults.len(), faults.join("\n"));
    assert!(drawn > least, "only {drawn} drawn ({refused} refused)");
    // A silhouette the drawing could not close is said, and rare (a coil seen along its axis).
    assert!(no_outline * 50 <= drawn, "{no_outline} of {drawn} drawings have no silhouette");
}

// ---------------------------------------------------------------- slices and layers

/// The distance from `p` to the triangle `a b c` (my own: to its plane where the foot is inside,
/// else to its nearest side).
fn to_triangle(p: P3, a: P3, b: P3, c: P3) -> f64 {
    let to_seg = |a: P3, b: P3| {
        let e = sub(b, a);
        let t = (dot(sub(p, a), e) / dot(e, e)).clamp(0.0, 1.0);
        norm(sub(p, [a[0] + e[0] * t, a[1] + e[1] * t, a[2] + e[2] * t]))
    };
    let n = cross(sub(b, a), sub(c, a));
    let nn = dot(n, n);
    if nn > 0.0 && [(a, b), (b, c), (c, a)].iter().all(|(u, v)| dot(cross(sub(*v, *u), sub(p, *u)), n) >= 0.0) {
        return dot(sub(p, a), n).abs() / nn.sqrt();
    }
    to_seg(a, b).min(to_seg(b, c)).min(to_seg(c, a))
}

/// A cut checked from the mesh's points alone: each region well made by my own outline check
/// (its rings simple, the outer counter-clockwise, the holes clockwise and inside it), the
/// regions' areas the cut's, and every ring point (put back in space by the plane's frame) on
/// the plane and on the mesh's surface to rounding.
fn check_slice(mesh: &Mesh, s: &specula_formae::slice::Slice) -> Result<(), String> {
    let (u, w) = s.plane.frame();
    let n = s.plane.normal;
    let reach = mesh.points.iter().flatten().fold(0.0f64, |m, x| m.max(x.abs()));
    let hair = 1e-9 * reach;
    let mut area = 0.0;
    for (i, r) in s.regions.iter().enumerate() {
        let (a, _) = own_outline(&r.rings).map_err(|e| format!("region {i}: {e}"))?;
        area += a;
    }
    if (area - s.area()).abs() > 1e-9 * area.abs() + hair * hair {
        return Err(format!("the regions hold {area} and the cut {}", s.area()));
    }
    let tris: Vec<[P3; 3]> = mesh.triangles().iter().map(|(_, t)| t.map(|v| mesh.points[v as usize])).collect();
    for r in s.rings.iter().take(4) {
        for q in r.iter().step_by((r.len() / 8).max(1)) {
            let p = [u[0] * q[0] + w[0] * q[1] + n[0] * s.plane.offset, u[1] * q[0] + w[1] * q[1] + n[1] * s.plane.offset, u[2] * q[0] + w[2] * q[1] + n[2] * s.plane.offset];
            let d = tris.iter().map(|t| to_triangle(p, t[0], t[1], t[2])).fold(f64::MAX, f64::min);
            if d > hair {
                return Err(format!("a ring point {q:?} is {d} from the mesh"));
            }
        }
    }
    Ok(())
}

/// Every solid word cut at drawn heights along each axis (checked by [`check_slice`]); a ball's
/// cut is the circle Pythagoras gives (every ring point at `√(r² − h²)` from the axis, less no
/// more than the build's error); a cone's shrinks in a straight line; a torus cut through its
/// axis is two circles of the tube's radius; and the layers of every word sum, times their
/// spacing, to its volume within the midpoint rule's bound for what the cuts show.
#[test]
fn slices_by_checks_of_my_own() {
    slices(1, &[30]);
}

/// The full run: two sets a word, details 9, 30 and 101.
#[test]
#[ignore = "the full cut of every solid word (two sets, three details): minutes in the gate's build"]
fn slices_full() {
    slices(sets(2), &[9, 30, 101]);
}

fn slices(n_sets: usize, details: &[u32]) {
    use specula_formae::slice::{layers, slice, Axis, Plane};
    let mut faults: Vec<String> = Vec::new();
    let (mut cuts, mut refused) = (0usize, 0usize);
    for w in words(Kind::Solid) {
        let params = params_of(&w);
        let mut seq = Seq(seed(w.name()) ^ 0x5A5A);
        for set in 0..n_sets {
            let given: Vec<(&str, f64)> = if set == 0 { Vec::new() } else { params.iter().map(|p| (p.name, draw(p, &mut seq, set))).collect() };
            let Ok(s) = Spec::of(w, &given) else { continue };
            for &detail in details {
                let Ok(Built::Mesh(m)) = s.build(detail) else { continue };
                let what = format!("{} {given:?} at {detail}", w.name());
                let Some((lo, hi)) = m.bounds() else { continue };
                let h = m.chord_error;
                let reach = m.points.iter().flatten().fold(0.0f64, |a, x| a.max(x.abs()));
                let hair = 1e-9 * reach;
                for (k, axis) in [Axis::X, Axis::Y, Axis::Z].into_iter().enumerate() {
                    for _ in 0..2 {
                        let at = lo[k] + (hi[k] - lo[k]) * (0.02 + 0.96 * seq.next());
                        match slice(&m, Plane::at(axis, at)) {
                            Ok(c) => {
                                cuts += 1;
                                if let Err(e) = check_slice(&m, &c) {
                                    gather(&mut faults, w.name(), format!("{what}: cut {axis:?} at {at}: {e}"));
                                }
                                // The ball's cut: Pythagoras.
                                if ["sphere", "icosphere", "cube_sphere"].contains(&w.form.name) && axis == Axis::Y {
                                    let r = s.args.f("radius");
                                    let y = at - r;
                                    for ring in &c.rings {
                                        for q in ring {
                                            let d = (q[0] * q[0] + q[1] * q[1] + y * y).sqrt();
                                            if d > r + hair || d < r - h - hair {
                                                gather(&mut faults, w.name(), format!("{what}: a point of the cut at {at} is {d} from the centre, radius {r}, error {h}"));
                                            }
                                        }
                                    }
                                    if c.regions.len() != 1 || c.rings.len() != 1 {
                                        gather(&mut faults, w.name(), format!("{what}: the ball's cut at {at} has {} rings", c.rings.len()));
                                    }
                                }
                                // The cone's cut shrinks in a straight line.
                                if w.form.name == "cone" && w.alias.is_none() && axis == Axis::Y {
                                    let (rr, hh) = (s.args.f("radius"), s.args.f("height"));
                                    let want = rr * (1.0 - at / hh);
                                    for q in c.rings.iter().flatten() {
                                        let d = (q[0] * q[0] + q[1] * q[1]).sqrt();
                                        if d > want + hair || d < want - h - hair {
                                            gather(&mut faults, w.name(), format!("{what}: a point of the cut at {at} is {d} from the axis, the cone's radius there {want}"));
                                        }
                                    }
                                }
                            }
                            Err(Error::Constraint { rule: "edge_under_floor", .. }) => refused += 1,
                            Err(e) => gather(&mut faults, w.name(), format!("{what}: cut {axis:?} at {at}: {e}")),
                        }
                    }
                }
                // A torus cut through its axis: two circles of the tube's radius.
                if w.form.name == "torus" && w.alias.is_none() {
                    let (big, tube) = (s.args.f("radius"), s.args.f("tube"));
                    match slice(&m, Plane::at(Axis::Z, 0.0)) {
                        Ok(c) => {
                            if c.regions.len() != 2 {
                                gather(&mut faults, w.name(), format!("{what}: the cut through the axis has {} regions", c.regions.len()));
                            }
                            // In the frame of +Z, u = x and v = y: the circles' middles are at (±big, tube);
                            // the one at +x is cut along a meridian of the build at every detail (its
                            // ring's first sample), so its points lie on the tube's circle there.
                            for q in c.rings.iter().flatten().filter(|q| q[0] > 0.0) {
                                let d = ((q[0] - big) * (q[0].abs() - big) + (q[1] - tube) * (q[1] - tube)).sqrt();
                                if d > tube + hair || d < tube - h - hair {
                                    gather(&mut faults, w.name(), format!("{what}: a point of the cut through the axis is {d} from the tube's middle, tube {tube}, error {h}"));
                                }
                            }
                        }
                        Err(e) => gather(&mut faults, w.name(), format!("{what}: the cut through the axis: {e}")),
                    }
                }
                // Layers: the midpoint rule's sum against the volume, within the span times the
                // areas' total change and their greatest (on each span the integral and its
                // middle's value differ by no more than the span times the area's change over it,
                // which the samples bound together with the greatest area, for a function of few
                // turns).
                let n_layers = 48;
                match layers(&m, Axis::Y, n_layers) {
                    Ok(ls) => {
                        let step = (hi[1] - lo[1]) / n_layers as f64;
                        let areas: Vec<f64> = ls.iter().map(|l| l.area()).collect();
                        let sum: f64 = areas.iter().sum::<f64>() * step;
                        let vol = m.measures().size.unwrap_or(0.0);
                        let tv: f64 = areas.windows(2).map(|x| (x[1] - x[0]).abs()).sum::<f64>() + 2.0 * areas.iter().fold(0.0f64, |a, x| a.max(x.abs()));
                        if (sum - vol).abs() > step * tv + 1e-9 * vol.abs() {
                            gather(&mut faults, w.name(), format!("{what}: the layers hold {sum} and the volume is {vol}"));
                        }
                    }
                    Err(Error::Constraint { rule: "edge_under_floor", .. }) => refused += 1,
                    Err(e) => gather(&mut faults, w.name(), format!("{what}: layers: {e}")),
                }
            }
        }
    }
    faults.truncate(80);
    assert!(faults.is_empty(), "{} faults ({cuts} cuts):\n{}", faults.len(), faults.join("\n"));
    assert!(cuts > 500 && refused * 20 <= cuts, "{cuts} cuts, {refused} refused as edges under the floor");
}

/// A ball's layers sum, times their spacing, to its volume plus the midpoint rule's error, which
/// for the true ball of radius `r` cut into `N` layers is exactly `2 π r³ / (3 N²)` (each layer's
/// area `π (r² − y²)` is a quadratic, on which the rule is off by `−Δ³ A'' / 24` a span): at 8,
/// 16 and 32 layers, within what the build's error takes from the areas.
#[test]
fn layers_of_a_ball_converge_as_the_square_of_the_spacing() {
    use specula_formae::slice::{layers, Axis};
    let r = 0.7;
    let Ok(Built::Mesh(m)) = Spec::new("sphere", Some(Kind::Solid), &[("radius", r)]).unwrap().build(512) else { panic!("a sphere") };
    let h = m.chord_error;
    let pi = std::f64::consts::PI;
    for n in [8u32, 16, 32] {
        let ls = layers(&m, Axis::Y, n).unwrap();
        let step = 2.0 * r / n as f64;
        let sum: f64 = ls.iter().map(|l| l.area()).sum::<f64>() * step;
        let want = 4.0 / 3.0 * pi * r * r * r + 2.0 * pi * r * r * r / (3.0 * (n * n) as f64);
        // A cut of the build is inside the ball's and within `h` of it: its area short by no more
        // than its perimeter times h, `2 π r h`, a layer.
        let allowed = 2.0 * pi * r * h * 2.0 * r;
        assert!((sum - want).abs() <= allowed, "{n} layers: {sum}, the rule says {want} (allowed {allowed})");
    }
}

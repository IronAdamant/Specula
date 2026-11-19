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

use specula_core::dmath;
use specula_formae::{aliases, find, forms, ops, params_of, Built, Error, Family, Found, Kind, Mesh, Param, Spec, Unit};
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
        for detail in [8u32, 9, 30] {
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
    assert!(built > 1_000, "only {built} builds ({refused} refused)");
}

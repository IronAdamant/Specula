//! The reviewer's probe of stage 1a, kept as a test: **every word with its arguments drawn at
//! random together**, through the public interface alone. (The crate's own table tests move one
//! parameter at a time to its least and its greatest, the others at their defaults: a figure
//! whose parameters must agree with each other, a lune's two radii and their distance, a star's
//! inner and outer radius, a trapezium's sides, is only ever tried there at one corner.)
//!
//! For each word, a few hundred argument sets from a fixed sequence, lengths spread evenly by
//! their logarithm (a millimetre is as likely as a kilometre). Each set either is refused as
//! data (a rule between its parameters, or the point ceiling) or builds; and what builds:
//!
//! - has the count it gave before building, and finite points;
//! - is a well-made outline (rings simple, the outer one counter-clockwise, holes clockwise
//!   and inside it), or a curve with no step of zero length;
//! - has anchors that are finite;
//! - has a closed-form area and length that the build agrees with as far as its own chord
//!   error allows (the same bound the crate's tests derive: the region between a chord and its
//!   curve lies in a strip as long as the chord and as wide as the error).
//! - and, for a solid (stage 1b): is well made for its genus (closed, wound outward, planar faces,
//!   Euler's formula), has no two faces crossing and no normal pointing in, and has a volume and
//!   an area within what its error allows of the closed forms (the crate's derivations: a shell of
//!   `2 h A` for the volume; for the area the faces' lean and their edges' strips). A solid's area
//!   is not held under its closed form, as a curve's length is under its: a faceted surface's
//!   area can be over the true one where the surface is not convex.
//!
//! No form may refuse everything: a word that never builds from its own ranges is a table
//! that lies about them.

use specula_core::dmath;
use specula_formae::{aliases, find, forms, params_of, Built, Error, Found, Kind, Spec};

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}

/// A fixed sequence (a 64-bit linear congruential generator; the high bits).
struct Seq(u64);

impl Seq {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn seed(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3))
}

fn words() -> Vec<Found> {
    let mut v: Vec<Found> = forms().iter().map(|f| Found { form: f, alias: None }).collect();
    for a in aliases() {
        v.push(find(a.name, Some(a.kind)).expect("every alias resolves"));
    }
    v
}

/// The default run: 60 argument sets a plane figure's or a curve's word and 8 a solid's, each
/// held to every demand below; [`every_word_full`] draws 300 and 40 (as the reviewer's probe
/// first did). That no word refuses every set drawn from its own ranges is held by the full run
/// alone: in 60 sets a word whose sides must agree (`triangle_sss`) may draw none that do.
#[test]
fn every_word_with_arguments_drawn_together() {
    run(60, 8, 4_000, false);
}

/// The full run: 300 sets a plane figure's or a curve's word and 40 a solid's
/// (`FORMAE_PROBE_SOLID_SETS` draws more of a solid's, as a check).
#[test]
#[ignore = "the full run of the drawn arguments (300 sets a figure, 40 a solid): minutes; run in release at a stage's end"]
fn every_word_full() {
    let solid_sets: usize = std::env::var("FORMAE_PROBE_SOLID_SETS").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
    run(300, solid_sets, 20_000, true);
}

fn run(sets: usize, solid_sets: usize, least_builds: usize, every_word_builds: bool) {
    let mut never: Vec<&str> = Vec::new();
    let (mut built, mut refused) = (0usize, 0usize);
    // (Every fault is gathered, a few of each word, and they are said together.)
    let mut faults: Vec<String> = Vec::new();
    let mut fault = |word: &str, said: String| {
        if faults.iter().filter(|f| f.starts_with(word)).count() < 2 {
            faults.push(said);
        }
    };
    for w in words() {
        let params = params_of(&w);
        let mut seq = Seq(seed(w.name()));
        let mut ok = 0usize;
        let solid = w.form.kind == Kind::Solid;
        for set in 0..if solid { solid_sets } else { sets } {
            let given: Vec<(&str, f64)> = params
                .iter()
                .map(|p| {
                    let u = seq.next();
                    let v = if p.integer {
                        // (Small counts mostly: a gear of 500 teeth is one case, not the rule.)
                        let span = (p.max - p.min).min(if set % 4 == 0 { f64::MAX } else { 12.0 });
                        (p.min + dmath::floor_f64(u * (span + 1.0))).min(p.max)
                    } else if p.min > 0.0 && p.max / p.min > 100.0 {
                        // By its logarithm; computed with what every machine rounds alike.
                        let (lo, hi) = (dmath::ln_f64(p.min), dmath::ln_f64(p.max));
                        dmath::exp_f64(lo + (hi - lo) * u).clamp(p.min, p.max)
                    } else {
                        p.min + (p.max - p.min) * u
                    };
                    (p.name, v)
                })
                .collect();
            let what = format!("{} {given:?}", w.name());
            let s = Spec::of(w, &given).unwrap_or_else(|e| panic!("{what}: an argument inside its range was refused: {e}"));
            for detail in [8u32, 64, 256] {
                if solid && detail == 256 && set % 4 != 0 {
                    continue;
                }
                let b = match s.build(detail) {
                    Ok(b) => b,
                    Err(Error::Constraint { .. } | Error::TooManyPoints { .. }) => {
                        refused += 1;
                        continue;
                    }
                    Err(e) => panic!("{what} at detail {detail}: {e}"),
                };
                built += 1;
                if detail == 64 {
                    ok += 1;
                }
                let c = s.count(detail).unwrap_or_else(|e| panic!("{what}: built, and its count refused: {e}"));
                assert_eq!((c.points, c.parts), (b.points(), b.parts()), "{what} at {detail}: the count before building");
                let m = b.measures();
                let (lo, hi) = m.bounds.unwrap_or_else(|| panic!("{what}: a build with no bounds"));
                let reach = lo.iter().chain(hi.iter()).fold(0.0f64, |a, x| a.max(x.abs()));
                assert!(reach.is_finite() && b.chord_error().is_finite(), "{what} at {detail}: not finite");
                match &b {
                    Built::Outline(o) => {
                        if let Err(f) = o.validate() {
                            fault(w.name(), format!("{what} at detail {detail}: {f:?}"));
                            continue;
                        }
                    }
                    Built::Curve(c) => {
                        let n = c.points.len();
                        assert!(n >= 2, "{what}");
                        let size = dist([lo[0], lo[1]], [hi[0], hi[1]]);
                        let steps = if c.closed { n } else { n - 1 };
                        for i in 0..steps {
                            let (a, z) = (c.points[i], c.points[(i + 1) % n]);
                            let l = dist(a, z);
                            if l <= size * 1e-9 {
                                fault(w.name(), format!("{what} at {detail}: step {i} of the curve has length {l}"));
                                break;
                            }
                        }
                    }
                    Built::Mesh(mesh) => {
                        // A solid: well made for its genus (closed, wound outward, planar faces,
                        // nothing under the floor, Euler's formula), no two faces crossing (searched
                        // where the mesh is small enough for a test), every normal on the outer
                        // side of its face.
                        let genus = s.genus().unwrap_or_else(|| panic!("{what}: a solid with no genus"));
                        if let Err(f) = mesh.validate(genus) {
                            fault(w.name(), format!("{what} at detail {detail}: {f:?}"));
                            continue;
                        }
                        if mesh.faces.len() <= 10_000 {
                            if let Some(pair) = mesh.self_crossing() {
                                fault(w.name(), format!("{what} at detail {detail}: faces {pair:?} cross"));
                                continue;
                            }
                        }
                        for f in &mesh.faces {
                            if let Some(ns) = &f.normals {
                                let a = mesh.area_vector(f);
                                if ns.iter().any(|n| n[0] * a[0] + n[1] * a[1] + n[2] * a[2] <= 0.0) {
                                    fault(w.name(), format!("{what} at detail {detail}: a normal points in"));
                                    break;
                                }
                            }
                        }
                    }
                    _ => {}
                }
                for a in s.anchors().unwrap_or_else(|e| panic!("{what}: built, and its anchors refused: {e}")) {
                    assert!(a.at.iter().all(|x| x.is_finite()), "{what}: anchor {} is not finite", a.name);
                }
                // The closed forms against the build.
                let closed = s.closed().unwrap_or_else(|e| panic!("{what}: built, and its closed forms refused: {e}"));
                let round = 64.0 * b.points() as f64 * f64::EPSILON * reach;
                if let (Built::Mesh(mesh), Some(c), Some(x), Some(a)) = (&b, closed.size, m.size, m.boundary) {
                    // A solid's volume: the true and the built solid differ only within the build's
                    // error `h` of the surface, a shell of at most `2 h A` (as an area's `2 h P`).
                    let allowed = 2.0 * b.chord_error() * a + round * reach * reach + 1e-9 * c.abs();
                    if (c - x).abs() > allowed {
                        fault(w.name(), format!("{what} at detail {detail}: volume closed {c}, built {x} (off by {}, allowed {allowed})", (c - x).abs()));
                    }
                    // Its area: a face stands for a patch whose normals lean by at most the turn `t`
                    // to its neighbours' (so the patch is within `1 / cos t` of the face's area), and
                    // each edge lies within `h` of where the patch's edge falls (an area of at most
                    // `h` times its length on each side).
                    if let Some(ca) = closed.boundary {
                        let t = mesh.rounds().iter().map(|r| r.turn_deg.to_radians()).fold(0.0f64, f64::max).min(1.5);
                        let edges: f64 = mesh.faces.iter().map(|f| (0..f.verts.len()).map(|i| dist3(mesh.points[f.verts[i] as usize], mesh.points[f.verts[(i + 1) % f.verts.len()] as usize])).sum::<f64>()).sum::<f64>() * 0.5;
                        let allowed = a * (1.0 / dmath::cos_f64(t) - 1.0) + 2.0 * b.chord_error() * edges + round * reach + 1e-9 * ca.abs();
                        if (ca - a).abs() > allowed {
                            fault(w.name(), format!("{what} at detail {detail}: area closed {ca}, built {a} (off by {}, allowed {allowed})", (ca - a).abs()));
                        }
                    }
                    continue;
                }
                if let (Some(c), Some(x), Some(p)) = (closed.size, m.size, m.boundary) {
                    let allowed = 2.0 * b.chord_error() * p + round * reach + 1e-9 * c.abs();
                    if (c - x).abs() > allowed {
                        fault(w.name(), format!("{what} at detail {detail}: area closed {c}, built {x} (off by {}, allowed {allowed})", (c - x).abs()));
                    }
                }
                if let (Some(c), Some(x)) = (closed.boundary, m.boundary) {
                    if x > c * (1.0 + 1e-9) + round {
                        fault(w.name(), format!("{what} at detail {detail}: the chords ({x}) are longer than the curve ({c})"));
                    }
                }
            }
        }
        if ok == 0 {
            never.push(w.name());
        }
    }
    assert!(faults.is_empty(), "{} faults:
{}", faults.len(), faults.join("
"));
    assert!(!every_word_builds || never.is_empty(), "these words never built from {sets} argument sets inside their own ranges: {never:?}");
    assert!(built > least_builds, "only {built} builds ({refused} refused)");
}

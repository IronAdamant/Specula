//! How coarsely each shape a recipe names round was built, **measured in the finished model**
//! (`ModelExtras::round`, docs/design/FORM.md, "Faceting, from what the recipe names round"):
//! the same mesh gets the same answer however its recipe wrote it, a shape with no triangle of
//! its own left says nothing, and what cannot be measured says so. Each case here is one the
//! first review of faceting found wrong, or one no test held; truths come from the built mesh
//! or from the same shape built at its final size.

use std::collections::BTreeMap;

use specula_model::{build, BuildParams, Model, Round, Style, MAX_ROUND};

fn style() -> Style {
    Style::parse(r##"{ "style": "t", "palette": { "pale": "#b8fff6" }, "materials": { "stone": { "color": "#8c96a3", "roughness": 0.8 } } }"##).unwrap()
}

fn make_with(text: &str, lib: &BTreeMap<String, String>) -> Model {
    let s = style();
    build(text, &BuildParams { library: Some(lib), ..BuildParams::new(&s) }).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn make(text: &str) -> Model {
    make_with(text, &BTreeMap::new())
}

fn part(name: &str, shape: &str, ops: &str) -> String {
    format!(r#"{{ "name": "{name}", "material": "stone", "shape": {shape}, "ops": [{ops}] }}"#)
}

fn parts(list: &[String], finish: &str) -> Model {
    make(&format!(r#"{{ "name": "t", "parts": [{}], "finish": [{finish}] }}"#, list.join(", ")))
}

fn one(shape: &str, ops: &str) -> Model {
    parts(&[part("p", shape, ops)], "")
}

fn the(m: &Model, path: &str) -> Round {
    let found: Vec<&Round> = m.extras.round.iter().filter(|r| r.path == path).collect();
    assert_eq!(found.len(), 1, "{path} in {:?}", m.extras.round);
    found[0].clone()
}

fn near(a: f32, b: f32, rel: f32) -> bool {
    (a - b).abs() <= rel * b.abs().max(1e-6)
}

/// **The same mesh gets the same answer however its recipe wrote it**: a six-sided column of
/// radius 1, one of 0.1 scaled by ten (as a part's op, as an include's, as the finish), and one
/// of 100 scaled by a thousandth, all measured as what they are in the model.
#[test]
fn the_same_mesh_gets_the_same_answer() {
    let c = |r: f32| format!(r#"{{"op":"cylinder","radius":{r},"height":1,"sides":6}}"#);
    let truth = the(&one(&c(1.0), ""), "parts[0].shape");
    assert!(near(truth.radius_m, 1.0, 1e-6) && near(truth.turn_deg, 60.0, 1e-6) && near(truth.sag_m(), 1.0 - 0.866_025_4, 1e-5), "{truth:?}");
    let by_op = the(&one(&c(0.1), r#"{"op":"scale","by":10}"#), "parts[0].shape");
    let by_finish = the(&parts(&[part("p", &c(0.1), "")], r#"{"op":"scale","by":10}"#), "parts[0].shape");
    let mut lib = BTreeMap::new();
    lib.insert("pin".to_string(), format!(r#"{{ "name": "pin", "parts": [ {} ] }}"#, part("q", &c(0.1), "")));
    let included = make_with(r#"{ "name": "t", "parts": [ { "name": "b", "include": "pin", "ops": [{"op":"scale","by":10}] } ] }"#, &lib);
    let by_include = the(&included, "parts[0].include(pin).parts[0].shape");
    for r in [&by_op, &by_finish, &by_include] {
        assert!(near(r.radius_m, truth.radius_m, 1e-5) && near(r.turn_deg, truth.turn_deg, 1e-5) && near(r.sag_m(), truth.sag_m(), 1e-4) && r.measured(), "{r:?} against {truth:?}");
    }
    let small = the(&one(&c(100.0), r#"{"op":"scale","by":0.001}"#), "parts[0].shape");
    assert!(near(small.radius_m, 0.1, 1e-4) && near(small.sag_m(), 0.1 * (1.0 - 0.866_025_4), 1e-3), "{small:?}");
    // Copies: an array, a radial and a mirror that keeps both halves leave a similarity's
    // answer as it is; a radial of more copies than are kept apart too.
    for ops in [r#"{"op":"array","count":3,"offset":[3,0,0]}"#, r#"{"op":"radial","count":70}"#, r#"{"op":"mirror","axis":"x","keep":true},{"op":"move","by":[2,0,0]}"#, r#"{"op":"rotate","deg":[30,40,50]},{"op":"radial","count":5,"jitter":{"rotate":[0,40,0]}}"#] {
        let r = the(&one(&c(1.0), ops), "parts[0].shape");
        assert!(near(r.turn_deg, 60.0, 1e-4) && near(r.sag_m(), truth.sag_m(), 1e-4) && r.measured(), "{ops}: {r:?}");
    }
    // Copies scaled by a jitter: the largest copy speaks.
    let r = the(&one(&c(1.0), r#"{"op":"array","count":6,"offset":[3,0,0],"jitter":{"scale":0.5}}"#), "parts[0].shape");
    assert!(r.radius_m > 1.0 && r.radius_m <= 1.5 + 1e-4 && near(r.turn_deg, 60.0, 1e-4), "{r:?}");
}

/// A scale that is not the same every way makes a ring an ellipse: its turn and how far its
/// sides stand in are measured on the ellipse, held against the built mesh's own top ring
/// (its corners in order round it, and the true ellipse searched finely along each side).
#[test]
fn a_ring_stretched_unevenly_is_measured_on_its_ellipse() {
    for (sides, sx, sz) in [(6u32, 3.0f32, 1.0f32), (12, 1.0, 2.5), (9, 0.4, 1.7)] {
        let m = one(&format!(r#"{{"op":"cylinder","radius":1,"height":1,"sides":{sides}}}"#), &format!(r#"{{"op":"scale","by":[{sx},1,{sz}]}}"#));
        let r = the(&m, "parts[0].shape");
        // (The top ring, from the mesh: its corners at y = 1, ordered round the axis.)
        let mesh = &m.lods[0];
        let mut ring: Vec<[f64; 2]> = Vec::new();
        for p in &mesh.positions {
            if (p[1] - 1.0).abs() < 1e-4 && (p[0] * p[0] / (sx * sx) + p[2] * p[2] / (sz * sz) - 1.0).abs() < 1e-3 {
                let q = [f64::from(p[0]), f64::from(p[2])];
                if !ring.iter().any(|x| (x[0] - q[0]).abs() < 1e-5 && (x[1] - q[1]).abs() < 1e-5) {
                    ring.push(q);
                }
            }
        }
        assert_eq!(ring.len(), sides as usize);
        let ang = |q: &[f64; 2]| specula_model::dmath::atan2_f64(q[1] / f64::from(sz), q[0] / f64::from(sx));
        ring.sort_by(|a, b| ang(a).total_cmp(&ang(b)));
        let n = ring.len();
        let (mut turn, mut sag) = (0.0f64, 0.0f64);
        for k in 0..n {
            let (a, b, c) = (ring[(k + n - 1) % n], ring[k], ring[(k + 1) % n]);
            let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
            turn = turn.max(specula_model::dmath::atan2_f64((u[0] * v[1] - u[1] * v[0]).abs(), u[0] * v[0] + u[1] * v[1]).to_degrees());
            let (t0, mut t1) = (ang(&b), ang(&c));
            if t1 < t0 {
                t1 += std::f64::consts::TAU;
            }
            let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
            for j in 0..=400 {
                let (s, co) = specula_model::dmath::sin_cos_f64(t0 + (t1 - t0) * f64::from(j) / 400.0);
                let q = [f64::from(sx) * co, f64::from(sz) * s];
                sag = sag.max(((q[0] - b[0]) * v[1] - (q[1] - b[1]) * v[0]).abs() / l);
            }
        }
        assert!((f64::from(r.turn_deg) - turn).abs() < 0.05 && (f64::from(r.sag_m()) - sag).abs() < 1e-3 * sag + 1e-5 && near(r.radius_m, ring.iter().map(|q| (q[0] * q[0] + q[1] * q[1]).sqrt()).fold(0.0, f64::max) as f32, 1e-4) && r.measured(), "{sides} sides scaled [{sx},1,{sz}]: {r:?}; the mesh turns {turn:.3} and stands in {sag:.5}");
    }
    // A lean is a shear: affine, measured; a ball stretched into an egg is measured face by
    // face, and stands in further than the ball, by no more than the stretch.
    assert!(the(&one(r#"{"op":"cylinder","radius":1,"height":1,"sides":8}"#, r#"{"op":"lean","x":0.5}"#), "parts[0].shape").measured());
    let ball = the(&one(r#"{"op":"sphere","radius":0.5,"subdivisions":2}"#, ""), "parts[0].shape");
    let egg = the(&one(r#"{"op":"sphere","radius":0.5,"subdivisions":2}"#, r#"{"op":"scale","by":[1,2,1]}"#), "parts[0].shape");
    assert!(egg.measured() && egg.sag_m() > ball.sag_m() && egg.sag_m() <= 2.0 * ball.sag_m() * 1.0001 && egg.turn_deg > ball.turn_deg, "{egg:?} {ball:?}");
}

/// What is not affine is not measured, and says so: a taper, a twist and a bend; a torus's
/// tube and a capsule's ends stretched unevenly. A displacement moves the facets with the
/// curve: measured, with how far it moved them.
#[test]
fn what_cannot_be_measured_says_so() {
    let c = r#"{"op":"cylinder","radius":0.5,"height":1,"sides":8}"#;
    for (ops, why) in [(r#"{"op":"taper","from":1,"to":3}"#, "tapered"), (r#"{"op":"twist","deg_per_m":40}"#, "twisted"), (r#"{"op":"bend","deg_per_m":30}"#, "bent")] {
        let r = the(&one(c, ops), "parts[0].shape");
        assert!(!r.measured() && r.unmeasured.is_some_and(|u| u.contains(why)) && r.to_json().get("not_measured").is_some(), "{ops}: {r:?}");
    }
    let torus = one(r#"{"op":"torus","radius":0.7,"tube":0.1,"segments":12,"sides":5}"#, r#"{"op":"scale","by":[2,1,1]}"#);
    let (ring, tube): (Vec<&Round>, Vec<&Round>) = torus.extras.round.iter().partition(|r| r.count == "segments");
    assert!(ring[0].measured() && !tube[0].measured(), "{:?}", torus.extras.round);
    // Past the eight not measured that are kept, the counts still count them all.
    let list: Vec<String> = (0..11).map(|i| part(&format!("t{i}"), c, r#"{"op":"taper","from":1,"to":2}"#)).collect();
    let m = parts(&list, "");
    assert!(m.extras.round_total == 11 && m.extras.round_not_measured == 11 && m.extras.round.len() == 8, "{} kept, {} not measured of {}", m.extras.round.len(), m.extras.round_not_measured, m.extras.round_total);
    let d = the(&one(c, r#"{"op":"displace","amount":0.02,"scale":0.3},{"op":"scale","by":2}"#), "parts[0].shape");
    assert!(d.measured() && near(d.displaced_m, 0.04, 1e-4) && near(d.turn_deg, 45.0, 1e-4), "{d:?}");
}

/// **A shape with no triangle of its own in the finished model says nothing**: a cutter that
/// cuts nothing, a shape cut wholly away, a part switched off. A cutter that does cut speaks
/// for the walls it left. (A shape whole and wholly inside another part is in the mesh, and is
/// said: no sound test of that is cheap; docs/design/FORM.md says so.)
#[test]
fn a_shape_nobody_sees_says_nothing() {
    let cut = |at: f32| one(&format!(r#"{{"op":"difference","of":[{{"op":"box","size":[2,2,2]}},{{"op":"cylinder","radius":0.3,"height":3,"sides":3,"ops":[{{"op":"move","by":[{at},-0.5,0]}}]}}]}}"#), "");
    assert!(cut(9.0).extras.round.is_empty() && cut(9.0).extras.round_total == 0);
    let walls = cut(0.0);
    assert!(the(&walls, "parts[0].shape.of[1]").turn_deg == 120.0);
    assert!(walls.extras.made.iter().all(|m| !m.path.contains(".of[1]")), "a cutter is still not what the model is built of");
    let gone = one(r#"{"op":"difference","of":[{"op":"union","of":[{"op":"box","size":[1,1,1]},{"op":"cylinder","radius":0.2,"height":0.5,"sides":3,"ops":[{"op":"move","by":[3,0,0]}]}]},{"op":"box","size":[1,2,1],"ops":[{"op":"move","by":[3,-0.5,0]}]}]}"#, "");
    assert!(gone.extras.round.is_empty(), "{:?}", gone.extras.round);
    let off = make(r#"{ "name": "t", "parts": [ { "name": "a", "material": "stone", "shape": {"op":"box","size":[1,1,1]} }, { "name": "b", "material": "stone", "when": "0", "shape": {"op":"cylinder","radius":0.5,"height":1,"sides":3} } ] }"#);
    assert!(off.extras.round.is_empty());
}

/// **A sculpt speaks for each of its primitives that has surface of its own**, each with its
/// own radius against the cell: a bead of 2 mm does not hide the coarse ball of 1 m; a cutter
/// that cuts nothing says nothing; a cut that does leaves walls that speak.
#[test]
fn a_sculpt_speaks_for_each_primitive() {
    let ball = |extra: &str| one(&format!(r#"{{"op":"sdf","resolution":8,"tree":{{"op":"{}","of":[{{"op":"sphere","radius":1.0}},{extra}]}}}}"#, if extra.contains("cut") { "difference" } else { "union" }).replace("\"cut\":1,", ""), "");
    let with_bead = ball(r#"{"op":"sphere","radius":0.002,"at":[0.999,0,0]}"#);
    let big = the(&with_bead, "parts[0].shape.tree#0");
    assert!(near(big.radius_m, 1.0, 1e-4) && big.count == "resolution" && big.count_value == 8 && big.sag_m() > 0.005, "{big:?}");
    let far_cut = ball(r#"{"cut":1,"op":"sphere","radius":0.2,"at":[3,0,0]}"#);
    assert!(far_cut.extras.round.iter().all(|r| r.path != "parts[0].shape.tree#1"), "{:?}", far_cut.extras.round);
    let dimple = ball(r#"{"cut":1,"op":"sphere","radius":0.5,"at":[1,0,0]}"#);
    assert!(dimple.extras.round.iter().any(|r| r.path == "parts[0].shape.tree#1"), "{:?}", dimple.extras.round);
    // The measured figure reports by its limbs and trunk, each its own primitive.
    let fig = one(r#"{"op":"figure","part":"hand_l","sex":"female"}"#, "");
    assert!(fig.extras.round.len() > 1 && fig.extras.round.iter().all(|r| r.what == "figure" && r.path.starts_with("parts[0].shape.tree#")), "{:?}", fig.extras.round);
}

/// Each term of a capsule's turn decides alone (and is named), and a cone turns at its base's
/// radius, the larger of a cylinder's two.
#[test]
fn every_count_speaks_for_itself() {
    for (segs, rings, count) in [(3u32, 32u32, "segments"), (256, 1, "rings"), (5, 9, "segments"), (101, 23, "rings")] {
        let m = one(&format!(r#"{{"op":"capsule","radius":0.3,"height":1.5,"segments":{segs},"rings":{rings}}}"#), "");
        let worst = m.extras.round.iter().max_by(|a, b| a.turn_deg.total_cmp(&b.turn_deg)).unwrap();
        let want = if count == "segments" { 360.0 / segs as f32 } else { 90.0 / rings as f32 };
        assert!(worst.count == count && near(worst.turn_deg, want, 1e-5), "{segs} segments, {rings} rings: {:?}", m.extras.round);
    }
    let cone = the(&one(r#"{"op":"cone","radius":0.5,"height":1,"sides":5}"#, ""), "parts[0].shape");
    assert!(near(cone.radius_m, 0.5, 1e-6) && cone.count == "sides" && cone.count_value == 5);
    let flared = the(&one(r#"{"op":"cylinder","radius":0.2,"radius_top":0.6,"height":1,"sides":31}"#, ""), "parts[0].shape");
    assert!(near(flared.radius_m, 0.6, 1e-6));
}

/// **The records the answer needs are kept, in one order**: with 64 fine columns and one
/// spike of three sides, the spike is kept (it is the answer at any size where it shows), and
/// the facts say how many there were and were kept.
#[test]
fn the_records_kept_are_the_ones_the_answer_needs() {
    let mut list: Vec<String> = (0..MAX_ROUND).map(|i| part(&format!("c{i}"), r#"{"op":"cylinder","radius":1,"height":1,"sides":12}"#, &format!(r#"{{"op":"move","by":[{},0,0]}}"#, i as f32 * 0.01))).collect();
    list.push(part("spike", r#"{"op":"cylinder","radius":0.05,"height":1,"sides":3}"#, ""));
    let m = parts(&list, "");
    assert!(m.extras.round.len() == MAX_ROUND && m.extras.round_total == MAX_ROUND + 1, "{} kept of {}", m.extras.round.len(), m.extras.round_total);
    assert!(m.extras.round.iter().any(|r| r.path == format!("parts[{MAX_ROUND}].shape")), "the spike is kept");
    // (The kept order: the furthest standing first; each later one of the front turns more.)
    assert_eq!(m.extras.round[0].turn_deg, 30.0);
    assert_eq!(m.extras.round[1].turn_deg, 120.0);
}

/// **The kept records never drop the answer**, however many shapes are the answer at some
/// size (settled for the second review, which could not build it): seventy six-sided discs on
/// one spot, each leant a little more (turning more) and scaled a little smaller (standing in
/// less) than the one before, so that every one is the answer at some size, and all of them
/// show at the asset's size (they stand in further than a 512th of its box's diagonal, which
/// is no less than its greatest width). The one that turns the most is the answer, and is
/// kept; so is every other, past the 64 kept otherwise.
#[test]
fn the_answer_is_kept_however_many_shapes_are_answers() {
    let n = 70usize;
    let list: Vec<String> = (0..n)
        .map(|i| part(&format!("d{i}"), r#"{"op":"cylinder","radius":1,"height":0.01,"sides":6}"#, &format!(r#"{{"op":"lean","x":{}}},{{"op":"scale","by":{}}}"#, 0.03 * i as f32, (0..i).fold(1.0f32, |a, _| a * 0.97))))
        .collect();
    let m = parts(&list, "");
    let b = m.lods[0].bounds();
    let diag = (0..3).map(|k| (b.max[k] - b.min[k]) * (b.max[k] - b.min[k])).sum::<f32>().sqrt();
    let all: Vec<Round> = (0..n).map(|i| the(&m, &format!("parts[{i}].shape"))).collect();
    for w in all.windows(2) {
        assert!(w[1].turn_deg > w[0].turn_deg && w[1].sag_m() < w[0].sag_m(), "each is the answer at some size: {:?} then {:?}", w[0], w[1]);
    }
    assert!(all.iter().all(|r| r.sag_m() >= diag / 512.0), "all show");
    assert!(m.extras.round.len() == n && m.extras.round_total == n, "{} kept of {}", m.extras.round.len(), m.extras.round_total);
}

/// **Past what one build records, round shapes are counted and said** (the third review's
/// fourth finding): nine includes of 64 columns each and a tapered column after them are 577
/// round shapes, of which one build records 512; the 65 past them (the tapered one among them)
/// are in the total and said not recorded, never dropped without a word.
#[test]
fn round_shapes_past_the_records_are_counted() {
    let cols: Vec<String> = (0..64).map(|i| part(&format!("c{i}"), r#"{"op":"cylinder","radius":0.1,"height":1,"sides":5}"#, &format!(r#"{{"op":"move","by":[{},0,0]}}"#, f64::from(i) * 0.3))).collect();
    let mut lib = BTreeMap::new();
    lib.insert("row".to_string(), format!(r#"{{ "name": "row", "parts": [{}] }}"#, cols.join(", ")));
    let mut list: Vec<String> = (0..9).map(|i| format!(r#"{{ "name": "r{i}", "include": "row", "ops": [{{"op":"move","by":[0,0,{}]}}] }}"#, f64::from(i) * 0.3)).collect();
    list.push(part("last", r#"{"op":"cylinder","radius":0.1,"height":1,"sides":5}"#, r#"{"op":"taper","from":1,"to":0.5}"#));
    let m = make_with(&format!(r#"{{ "name": "t", "parts": [{}] }}"#, list.join(", ")), &lib);
    assert!(m.extras.round_total == 577 && m.extras.round_not_recorded == 65 && m.extras.round_not_measured == 0, "total {}, not recorded {}, not measured {}", m.extras.round_total, m.extras.round_not_recorded, m.extras.round_not_measured);
}

/// **A weld that merges a ring's own points leaves it not measured** (the third review's third
/// finding): a column of 64 sides of radius 0.5 (a side 49 mm long) welded by a millimetre is
/// the ring it was, measured; welded by 6 or 15 cm (past a side) the mesh no longer has the
/// ring its count made, and the record says so.
#[test]
fn a_weld_that_merges_a_rings_points_is_said() {
    let col = r#"{"op":"cylinder","radius":0.5,"height":1,"sides":64}"#;
    let plain = the(&one(col, ""), "parts[0].shape");
    let fine = the(&one(col, r#"{"op":"weld","eps":0.001}"#), "parts[0].shape");
    assert!(fine.measured() && fine.turn_deg == plain.turn_deg && fine.sag == plain.sag, "{fine:?}");
    for eps in [0.06, 0.15] {
        let r = the(&one(col, &format!(r#"{{"op":"weld","eps":{eps}}}"#)), "parts[0].shape");
        assert!(!r.measured() && r.unmeasured.is_some_and(|w| w.contains("weld")), "weld {eps}: {r:?}");
    }
}

/// **The radius said is the radius of the band that turns** (the third review's fifth finding):
/// a torus of radius 0.6 with a three-sided tube turns most round its axis at its inner wall,
/// 0.5 from the axis, though it reaches 0.8; seen from above its rim is the 0.8.
#[test]
fn the_radius_said_is_the_bands_that_turns() {
    let r = one(r#"{"op":"torus","radius":0.6,"tube":0.2,"segments":12,"sides":3}"#, "").extras.round.iter().find(|r| r.count == "segments").cloned().unwrap();
    let rim = r.rim.unwrap();
    assert!(near(r.turn_radius_m, 0.5, 1e-5) && near(r.radius_m, 0.8, 1e-5) && near(rim.radius_m, 0.8, 1e-5) && near(r.turn_deg, 30.0, 1e-4) && near(rim.turn_deg, 30.0, 1e-4), "{r:?}");
}

/// A scatter's round items are measured as each was placed: the largest placement speaks.
#[test]
fn a_scatter_measures_its_round_items() {
    let m = make(
        r#"{ "name": "sc", "seed": 3, "parts": [ { "name": "things", "material": "stone", "generate": { "op": "scatter", "area": { "rect": [20, 20] }, "min_distance": 0.8, "count": 40,
          "items": [ { "shape": { "op": "cylinder", "radius": 0.15, "height": 0.4, "sides": 5 } } ] } } ] }"#,
    );
    let r = the(&m, "parts[0].generate.items[0].shape");
    assert!(r.measured() && (r.turn_deg - 72.0).abs() < 1e-3 && r.radius_m >= 0.15 * 0.5 && r.radius_m <= 0.15 * 2.0, "{r:?}");
}

/// A form by name (`specula-formae`) is a round shape like any other: a ball of radius 0.1 at
/// detail 24 scaled by ten is measured as the same ball built at radius 1 (the turn as built,
/// the radius and how far a facet stands in scaled), and so is a curved outline by name
/// extruded; under a taper, which is not affine, a curved form says it is not measured.
#[test]
fn a_forms_facets_are_measured_in_the_finished_model() {
    let ball = |r: f32| format!(r#"{{"op":"form","form":"sphere","args":{{"radius":{r}}},"detail":24}}"#);
    let truth = one(&ball(1.0), "");
    let scaled = one(&ball(0.1), r#"{"op":"scale","by":10}"#);
    let (t, s) = (&truth.extras.round, &scaled.extras.round);
    assert!(!t.is_empty() && t.len() == s.len(), "{t:?} {s:?}");
    for (a, b) in t.iter().zip(s) {
        assert!(a.what == "sphere" && a.count == "detail" && a.count_value == 24 && a.measured() && b.measured(), "{a:?} {b:?}");
        assert!(near(b.turn_deg, a.turn_deg, 1e-5) && near(b.radius_m, a.radius_m, 1e-5) && near(b.sag_m(), a.sag_m(), 1e-4), "{a:?} against {b:?}");
        assert!(near(a.radius_m, 1.0, 1e-5) && a.turn_deg > 0.0 && a.turn_deg < 30.0, "{a:?}");
    }
    let out = |r: f32| format!(r#"{{"op":"extrude","outline":{{"form":"circle","args":{{"radius":{r}}},"detail":16}},"height":0.5}}"#);
    let (a, b) = (the(&one(&out(1.0), ""), "parts[0].shape"), the(&one(&out(0.5), r#"{"op":"scale","by":2}"#), "parts[0].shape"));
    assert!(a.what == "circle" && near(a.turn_deg, 22.5, 1e-4) && near(a.radius_m, 1.0, 1e-5), "{a:?}");
    assert!(b.measured() && near(b.turn_deg, a.turn_deg, 1e-5) && near(b.radius_m, 1.0, 1e-5) && near(b.sag_m(), a.sag_m(), 1e-4), "{a:?} against {b:?}");
    for shape in [ball(1.0), r#"{"op":"form","form":"torus","detail":16}"#.to_string(), out(1.0)] {
        let m = one(&shape, r#"{"op":"taper","from":1,"to":3}"#);
        assert!(!m.extras.round.is_empty() && m.extras.round.iter().all(|r| !r.measured() && r.unmeasured.is_some_and(|u| u.contains("tapered"))), "{shape}: {:?}", m.extras.round);
    }
}

/// **A form's record names a greatest detail that builds** (the faceting record's fourth round:
/// advice only ever a count that builds): for solids whose triangles grow as a line and as the
/// square of their detail, and for outlines by name, the record's range is the library's least
/// detail to a most no greater than its greatest, and the same shape asked at that most is built
/// (a part's and the model's limits hold), its record measured.
#[test]
fn a_forms_most_detail_builds() {
    let s = style();
    let shapes = [
        r#"{"op":"form","form":"sphere","detail":24}"#.to_string(),
        r#"{"op":"form","form":"torus","detail":16}"#.to_string(),
        r#"{"op":"form","form":"cylinder","detail":40}"#.to_string(),
        r#"{"op":"form","form":"cube_sphere","detail":12}"#.to_string(),
        r#"{"op":"form","form":"capsule","detail":9}"#.to_string(),
        r#"{"op":"form","form":"circle","raise":{"as":"bevelled","thickness":0.2,"width":0.05,"round":true},"detail":30}"#.to_string(),
        r#"{"op":"extrude","outline":{"form":"heart","detail":10},"height":0.2}"#.to_string(),
        r#"{"op":"extrude","outline":{"form":"ellipse","detail":9},"height":0.2}"#.to_string(),
    ];
    for shape in shapes {
        let m = one(&shape, "");
        assert!(!m.extras.round.is_empty(), "{shape}: no record");
        for r in &m.extras.round {
            // (Every one of these has finer details that build, and a solid's record says so; an
            // outline's may stop where its count steps past the square of the ratio: see
            // `form_rounds`.)
            let solid = shape.contains(r#""op":"form""#);
            assert!(r.count == "detail" && r.least == 8 && r.most >= r.count_value && (r.most > r.count_value || !solid) && r.most <= 4096, "{shape}: {r:?}");
            let at = shape.replacen(&format!("\"detail\":{}", r.count_value), &format!("\"detail\":{}", r.most), 1);
            let text = format!(r#"{{ "name": "t", "parts": [{}] }}"#, part("p", &at, ""));
            let again = build(&text, &BuildParams::new(&s)).unwrap_or_else(|e| panic!("{shape}: its most {} ({}) is refused: {e}", r.most, r.most_by));
            assert!(again.extras.round.iter().any(|q| q.count_value == r.most && q.measured()), "{at}: {:?}", again.extras.round);
        }
    }
}

/// **A form's most detail builds, the build's work included** (`form_rounds` and
/// `facet::Grows::Above`, held to the work bound of docs/design/FORM.md, "What was mended,
/// fourth round"): forms with a texture, four far levels, copies and `ao` passes, where the
/// work limit binds before a part's triangles do. Each record's most builds, and what the
/// build spent there is within the bound the record gave for it; most of them say the work
/// limit is what bound them.
#[test]
fn a_forms_most_detail_builds_within_the_work_limit() {
    const WORK: &str = "the build's work limit (1,500 million units)";
    let mut nodes = Vec::new();
    for i in 0..8 {
        nodes.push(format!(r#""n{i}": {{ "op": "noise", "kind": "fbm", "scale": {}, "octaves": 6 }}"#, 3 + i));
    }
    let mut last = "n0".to_string();
    for i in 1..8 {
        nodes.push(format!(r#""b{i}": {{ "op": "blend", "a": "{last}", "b": "n{i}", "mode": "multiply", "amount": 0.3 }}"#));
        last = format!("b{i}");
    }
    nodes.push(format!(r#""albedo": {{ "op": "gradient_map", "in": "{last}", "stops": [[0, "pale"], [1, "pale"]] }}"#));
    let tex = format!(r#"{{ "size": 256, "seed": 1, "nodes": {{ {} }}, "outputs": {{ "base_color": "albedo", "height": "{last}" }} }}"#, nodes.join(","));
    let ao = std::iter::repeat_n(r#"{"op":"ao","amount":0.3}"#, 14).collect::<Vec<_>>().join(",");
    let shapes: [(&str, u32); 5] = [
        (r#"{"op":"form","form":"sphere","detail":{N}}"#, 40),
        (r#"{"op":"form","form":"torus","detail":{N}}"#, 40),
        (r#"{"op":"form","form":"capsule","detail":{N}}"#, 40),
        (r#"{"op":"form","form":"circle","raise":{"as":"bevelled","thickness":0.2,"width":0.05,"round":true},"detail":{N}}"#, 20),
        (r#"{"op":"form","form":"annulus","raise":{"as":"bevelled","thickness":0.2,"width":0.02,"round":true},"detail":{N}}"#, 20),
    ];
    let s = style();
    let at = |shape: &str, copies: u32, n: u32| {
        format!(
            r#"{{ "name": "t", "lods": [0.5, 0.25, 0.12, 0.06], "smooth_angle": 0, "textures": {{ "skin": {tex} }}, "parts": [ {{ "name": "p", "material": "stone", "texture": "skin", "shape": {}, "ops": [{{"op":"array","count":{copies},"offset":[0,0,0]}},{ao}] }} ] }}"#,
            shape.replace("{N}", &n.to_string())
        )
    };
    let (mut by_work, mut records) = (0, 0);
    for (shape, copies) in shapes {
        let first = 8;
        let m = build(&at(shape, copies, first), &BuildParams::new(&s)).unwrap_or_else(|e| panic!("{shape}: {e}"));
        assert!(!m.extras.round.is_empty(), "{shape}: no record");
        for r in &m.extras.round {
            records += 1;
            assert!(r.count == "detail" && r.most >= first, "{shape}: {r:?}");
            by_work += usize::from(r.most_by == WORK);
            let again = build(&at(shape, copies, r.most), &BuildParams::new(&s)).unwrap_or_else(|e| panic!("{shape}: told its most {} ({}) builds: {e}", r.most, r.most_by));
            let bound = r.work_at(r.most).unwrap();
            assert!(again.extras.work <= bound, "{shape}: at {} it spent {} over the bound {bound}", r.most, again.extras.work);
        }
    }
    assert!(2 * by_work > records, "the work limit bound only {by_work} of {records} records");
}

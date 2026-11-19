//! What a recipe is built of (`ModelExtras::made`, docs/design/FORM.md, "From what the recipe
//! is built of"): **a record's surface is that shape's own surface in the finished model, and
//! what no record speaks for is `made_rest_m2`.** Each case here is one the first review of the
//! record found wrong, or one no test held.

use std::collections::BTreeMap;

use specula_model::made::{half_extents, MAX_RECORD_VIEWS, MAX_VIEWS};
use specula_model::{build, BuildParams, Made, Model, Style};

const PI: f32 = std::f32::consts::PI;

fn style() -> Style {
    Style::parse(r##"{ "style": "t", "palette": { "dark": "#202020", "pale": "#b8fff6" }, "materials": { "stone": { "color": "#8c96a3", "roughness": 0.8 } } }"##).unwrap()
}

fn make_with(text: &str, lib: &BTreeMap<String, String>) -> Model {
    let s = style();
    build(text, &BuildParams { library: Some(lib), ..BuildParams::new(&s) }).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn make(text: &str) -> Model {
    make_with(text, &BTreeMap::new())
}

/// One part of this shape, with these part ops.
fn one(shape: &str, ops: &str) -> Model {
    make(&format!(r#"{{ "name": "t", "parts": [ {{ "name": "p", "material": "stone", "shape": {shape}, "ops": [{ops}] }} ] }}"#))
}

fn part(name: &str, shape: &str, ops: &str) -> String {
    format!(r#"{{ "name": "{name}", "material": "stone", "shape": {shape}, "ops": [{ops}] }}"#)
}

fn parts(list: &[String]) -> Model {
    make(&format!(r#"{{ "name": "t", "parts": [{}] }}"#, list.join(", ")))
}

fn surface(m: &Model) -> f32 {
    let l = &m.lods[0];
    let mut s = 0.0f64;
    for t in 0..l.triangle_count() {
        let [a, b, c] = l.triangle(t);
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        s += 0.5 * f64::from((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt());
    }
    s as f32
}

fn of<'a>(m: &'a Model, path: &str) -> &'a Made {
    m.extras.made.iter().find(|x| x.path == path).unwrap_or_else(|| panic!("nothing at {path}: {:?}", m.extras.made.iter().map(|x| (&x.path, x.area_m2)).collect::<Vec<_>>()))
}

fn near(a: f32, b: f32, rel: f32) -> bool {
    (a - b).abs() <= rel * b.abs()
}

/// What the record says and what the mesh has add up: the records and the rest are the
/// model's surface, and no path is recorded twice.
fn adds_up(m: &Model) {
    let spoken: f32 = m.extras.made.iter().map(|x| x.area_m2).sum();
    let s = surface(m);
    assert!((spoken + m.extras.made_rest_m2 - s).abs() <= 0.01 * s + 1e-4, "spoken {spoken} + rest {} against {s}", m.extras.made_rest_m2);
    let mut paths: Vec<&str> = m.extras.made.iter().map(|x| x.path.as_str()).collect();
    paths.sort_unstable();
    assert!(paths.windows(2).all(|w| w[0] != w[1]), "a path twice: {paths:?}");
}

/// A small fixed sequence of numbers (an LCG: the recipes below are the same on every run).
struct Seq(u64);

impl Seq {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32) / (1u64 << 24) as f32
    }
    fn within(&mut self, lo: f32, hi: f32) -> f32 {
        let v = lo + (hi - lo) * self.next();
        (v * 1000.0).round() / 1000.0
    }
    fn pick(&mut self, n: usize) -> usize {
        ((self.next() * n as f32) as usize).min(n - 1)
    }
}

/// A shape node standing on y = 0 and one metre tall (so that a part's taper, which reaches
/// from the bottom of what it tapers to its top, is the same on it alone as among others),
/// and its own ops: moved along X and Z, turned about Y, scaled along X and Z. `level`: one that
/// reaches y = 0 and y = 1 exactly (not a ball: its points miss its poles), for a union, whose
/// taper reaches from the lowest of its operands to the highest.
fn a_shape(s: &mut Seq, level: bool) -> (String, String) {
    let kind = if level { [0, 1, 2, 4, 5][s.pick(5)] } else { s.pick(6) };
    let shape = match kind {
        0 => format!(r#"{{"op":"box","size":[{},1,{}]}}"#, s.within(0.2, 1.0), s.within(0.2, 1.0)),
        1 => format!(r#"{{"op":"cylinder","radius":{},"height":1,"sides":{}}}"#, s.within(0.1, 0.5), 6 + s.pick(20)),
        2 => format!(r#"{{"op":"cone","radius":{},"height":1,"sides":{}}}"#, s.within(0.1, 0.5), 3 + s.pick(20)),
        3 => format!(r#"{{"op":"sphere","radius":0.5,"subdivisions":{}}}"#, 1 + s.pick(3)),
        4 => format!(r#"{{"op":"capsule","radius":{},"height":1}}"#, s.within(0.1, 0.4)),
        _ => format!(r#"{{"op":"lathe","profile":[[0,0],[{},0],[{},0.5],[{},1],[0,1]],"segments":{}}}"#, s.within(0.2, 0.5), s.within(0.1, 0.6), s.within(0.05, 0.3), 6 + s.pick(18)),
    };
    let mut own = vec![if shape.contains("sphere") { r#"{"op":"move","by":[0,0.5,0]}"#.to_string() } else { String::new() }];
    if s.next() < 0.5 {
        own.push(format!(r#"{{"op":"scale","by":[{},1,{}]}}"#, s.within(0.3, 2.5), s.within(0.3, 2.5)));
    }
    if s.next() < 0.5 {
        own.push(format!(r#"{{"op":"rotate","deg":[0,{},0]}}"#, s.within(-90.0, 90.0)));
    }
    own.retain(|o| !o.is_empty());
    (shape, own.join(","))
}

/// A part's ops: a scale by a vector, a taper, a twist, a mirror, an array, a subdivision, in
/// that order where drawn, and a bend last (a bend moves the bottoms and tops of what it bends
/// apart, so nothing that reaches between them may follow it).
fn part_ops(s: &mut Seq) -> String {
    let mut ops = Vec::new();
    if s.next() < 0.6 {
        ops.push(format!(r#"{{"op":"scale","by":[{},{},{}]}}"#, s.within(0.2, 2.0), s.within(0.03, 2.0), s.within(0.2, 2.0)));
    }
    if s.next() < 0.4 {
        ops.push(format!(r#"{{"op":"taper","axis":"y","from":{},"to":{}}}"#, s.within(0.5, 1.5), s.within(0.0, 1.2)));
    }
    if s.next() < 0.3 {
        ops.push(format!(r#"{{"op":"twist","deg_per_m":{}}}"#, s.within(-90.0, 90.0)));
    }
    if s.next() < 0.3 {
        ops.push(r#"{"op":"mirror","axis":"z","keep":true}"#.to_string());
    }
    if s.next() < 0.3 {
        ops.push(format!(r#"{{"op":"array","count":{},"offset":[0,0,{}]}}"#, 2 + s.pick(3), s.within(3.0, 6.0)));
    }
    if s.next() < 0.2 {
        ops.push(r#"{"op":"subdivide","levels":1}"#.to_string());
    }
    if s.next() < 0.3 {
        ops.push(format!(r#"{{"op":"bend","deg_per_m":{}}}"#, s.within(-60.0, 60.0)));
    }
    ops.join(",")
}

fn joined(lists: &[&str]) -> String {
    lists.iter().filter(|l| !l.is_empty()).copied().collect::<Vec<_>>().join(",")
}

/// **Each shape's record is its own surface, held against a truth that does not come from the
/// record**: the same shape built alone, as the only part of a recipe of its own, with the same
/// operations (its own, its part's, an include's, the finish), measured on that model's mesh.
/// Recipes drawn from a fixed sequence: shapes with their own ops; part ops that scale by a
/// vector, taper, twist, mirror, copy, subdivide and bend; unions of shapes apart (nothing
/// hidden, so exact up to the boolean's grid; an operand alone is a union of it alone, since a
/// boolean cuts faces into triangles of its own) and of shapes that overlap (what is hidden is
/// gone: never more than alone, and nothing lost, as a union has no cutter); includes with ops
/// of their own and the including part's; a finish that scales by a vector and twists.
#[test]
fn every_record_is_its_shape_alone() {
    let mut s = Seq(20_261_120);
    let mut checked = (0usize, 0usize);
    for round in 0..28 {
        // (One to three parts, each a shape, a union or an include; and a finish.)
        let mut lib = BTreeMap::new();
        let mut parts = Vec::new();
        // (Each named shape: its path, its node, the ops it goes through in order, and whether
        // a union may hide some of it.)
        let mut named: Vec<(String, String, Vec<String>, bool)> = Vec::new();
        for k in 0..1 + s.pick(3) {
            let ops = part_ops(&mut s);
            let x = 8.0 * k as f32;
            let place = format!(r#"{{"op":"move","by":[{x},0,0]}}"#);
            match s.pick(3) {
                0 => {
                    let (shape, own) = a_shape(&mut s, false);
                    parts.push(format!(r#"{{ "name": "p{k}", "material": "stone", "shape": {shape}, "ops": [{}] }}"#, joined(&[&own, &ops, &place])));
                    named.push((format!("parts[{k}].shape"), shape, vec![own, ops, place], false));
                }
                1 => {
                    let overlap = s.next() < 0.4;
                    let mut of = Vec::new();
                    for i in 0..2 + s.pick(2) {
                        let (shape, own) = a_shape(&mut s, true);
                        let apart = if overlap { format!(r#"{{"op":"move","by":[{},0,0]}}"#, 0.3 * i as f32) } else { format!(r#"{{"op":"move","by":[{},0,0]}}"#, 2.5 * i as f32) };
                        let own = joined(&[&own, &apart]);
                        of.push(format!(r#"{},"ops":[{own}]}}"#, &shape[..shape.len() - 1]));
                        named.push((format!("parts[{k}].shape.of[{i}]"), format!(r#"{{"op":"union","of":[{}]}}"#, of[i]), vec![ops.clone(), place.clone()], overlap));
                    }
                    parts.push(format!(r#"{{ "name": "p{k}", "material": "stone", "shape": {{"op":"union","of":[{}]}}, "ops": [{}] }}"#, of.join(","), joined(&[&ops, &place])));
                }
                _ => {
                    let mut kid = Vec::new();
                    let name = format!("kid{k}");
                    for j in 0..2 {
                        let (shape, own) = a_shape(&mut s, false);
                        let inner = format!(r#"{{"op":"scale","by":[{},1,{}]}},{{"op":"move","by":[{},0,0]}}"#, s.within(0.5, 1.5), s.within(0.5, 1.5), 3.0 * j as f32);
                        kid.push(format!(r#"{{ "name": "q{j}", "material": "stone", "shape": {shape}, "ops": [{}] }}"#, joined(&[&own, &inner])));
                        named.push((format!("parts[{k}].include({name}).parts[{j}].shape"), shape, vec![own, inner, ops.clone(), place.clone()], false));
                    }
                    lib.insert(name.clone(), format!(r#"{{ "name": "{name}", "parts": [{}] }}"#, kid.join(", ")));
                    parts.push(format!(r#"{{ "name": "p{k}", "include": "{name}", "ops": [{}] }}"#, joined(&[&ops, &place])));
                }
            }
        }
        let finish = if s.next() < 0.5 { format!(r#"{{"op":"scale","by":[{},{},{}]}},{{"op":"twist","deg_per_m":{}}}"#, s.within(0.5, 2.0), s.within(0.1, 2.0), s.within(0.5, 2.0), s.within(-45.0, 45.0)) } else { String::new() };
        let text = format!(r#"{{ "name": "t{round}", "parts": [{}], "finish": [{finish}] }}"#, parts.join(", "));
        let m = make_with(&text, &lib);
        let spoken: f32 = m.extras.made.iter().map(|x| x.area_m2).sum();
        let whole = surface(&m);
        // (Nothing here is cut, bevelled or generated: every triangle is a named shape's.)
        assert!(near(spoken, whole, 1e-3) && m.extras.made_rest_m2 <= 1e-3 * whole, "{text}: records {spoken} of {whole} m2");
        for (path, shape, ops, overlap) in &named {
            let alone = make(&format!(r#"{{ "name": "a", "parts": [ {{ "name": "a", "material": "stone", "shape": {shape}, "ops": [{}] }} ], "finish": [{finish}] }}"#, joined(&ops.iter().map(String::as_str).collect::<Vec<_>>())));
            let truth = surface(&alone);
            let said = m.extras.made.iter().find(|x| &x.path == path).map_or(0.0, |x| x.area_m2);
            if *overlap {
                assert!(said <= truth * 1.01, "{path} in {text}: {said} m2, more than alone ({truth})");
                checked.1 += 1;
            } else {
                // (A union operand's truth is the union of it alone, so that its faces are cut into
                // the triangles the boolean makes of them, which a taper, a twist or a bend after it
                // then bends: within a part in a thousand (the boolean's grid); a shape alone, in
                // ten thousand.)
                assert!(near(said, truth, if path.contains(".of[") { 1e-3 } else { 1e-4 }), "{path} in {text}: recorded {said} m2, alone {truth}");
                checked.0 += 1;
            }
        }
    }
    assert!(checked.0 >= 40 && checked.1 >= 4, "{checked:?} shapes checked exactly and hidden");
}

/// A boolean operand's own ops count in its surface: a ball its own ops scale by twenty has
/// the surface it has as a part of its own, and a cone its own ops copy twelve times counts
/// twelve times (less what the slab takes of each).
#[test]
fn a_boolean_operand_counts_after_its_own_ops() {
    let union = one(r#"{"op":"union","of":[{"op":"box","size":[1,1,1]},{"op":"sphere","radius":0.05,"subdivisions":3,"ops":[{"op":"scale","by":20},{"op":"move","by":[2,0,0]}]}]}"#, "");
    let alone = parts(&[part("a", r#"{"op":"box","size":[1,1,1]}"#, ""), part("s", r#"{"op":"sphere","radius":0.05,"subdivisions":3}"#, r#"{"op":"scale","by":20},{"op":"move","by":[2,0,0]}"#)]);
    let (ball, ball_alone) = (of(&union, "parts[0].shape.of[1]").area_m2, of(&alone, "parts[1].shape").area_m2);
    assert!(near(ball, ball_alone, 0.02), "a ball scaled by its own ops: {ball} against {ball_alone} as a part");
    adds_up(&union);
    let row = one(r#"{"op":"union","of":[{"op":"box","size":[6,0.2,1]},{"op":"cone","radius":0.2,"height":1,"sides":24,"ops":[{"op":"move","by":[-2.75,0.1,0]},{"op":"array","count":12,"offset":[0.5,0,0]}]}]}"#, "");
    let cone = one(r#"{"op":"cone","radius":0.2,"height":1,"sides":24}"#, "");
    let (cones, single) = (of(&row, "parts[0].shape.of[1]").area_m2, of(&cone, "parts[0].shape").area_m2);
    // (Each cone loses its base, a sixth of it, and the slab hides a fifth of what is left.)
    assert!(cones > 7.0 * single && cones < 12.0 * single, "twelve cones: {cones} m2, one {single} m2");
    adds_up(&row);
}

/// Each operand is recorded with what is left of its surface in the result: a ball of 3 m
/// that a 1 m box is inside of leaves the box nearly whole and of itself only the bowl the box
/// stands in; what a difference cuts is a wall that is its cutter's, which no record speaks for.
#[test]
fn a_boolean_records_what_is_left_of_each_operand() {
    // (A ball stands on the ground, as a box does: its bottom is the box's floor.)
    let m = one(r#"{"op":"intersection","of":[{"op":"sphere","radius":3,"subdivisions":3},{"op":"box","size":[1,1,1]}]}"#, "");
    let (ball, cube) = (of(&m, "parts[0].shape.of[0]").area_m2, of(&m, "parts[0].shape.of[1]").area_m2);
    assert!(cube > 4.0 * ball && ball < 1.5 && cube + ball <= 6.0 * 1.01, "an intersection: the ball {ball} m2 (113 m2 alone), the box {cube} m2");
    adds_up(&m);
    // A box with a round hole through it.
    let m = one(r#"{"op":"difference","of":[{"op":"box","size":[2,2,2]},{"op":"cylinder","radius":0.4,"height":3,"sides":32,"ops":[{"op":"move","by":[0,-0.5,0]}]}]}"#, "");
    assert!(m.extras.made.len() == 1 && near(of(&m, "parts[0].shape.of[0]").area_m2, 24.0 - 2.0 * PI * 0.16, 0.02), "{:?}", m.extras.made);
    assert!(near(m.extras.made_rest_m2, 2.0 * PI * 0.4 * 2.0, 0.03), "the hole's wall is no record's: {}", m.extras.made_rest_m2);
    adds_up(&m);
    // A union: the half of each ball the other is inside of is gone.
    let m = one(r#"{"op":"union","of":[{"op":"sphere","radius":1,"subdivisions":2},{"op":"sphere","radius":1,"subdivisions":2,"ops":[{"op":"move","by":[1,0,0]}]}]}"#, "");
    let (a, b) = (of(&m, "parts[0].shape.of[0]").area_m2, of(&m, "parts[0].shape.of[1]").area_m2);
    assert!(near(a, b, 0.02) && near(a, 0.75 * 4.0 * PI, 0.05), "two balls a radius apart: {a} and {b}");
    adds_up(&m);
}

/// An include's records are under the including part's path, written as its errors are, so
/// no two shapes share a path and a fix names the right one.
#[test]
fn an_include_records_under_its_path() {
    let mut lib = BTreeMap::new();
    lib.insert("spike".into(), r#"{ "name": "spike", "parts": [ { "name": "c", "material": "stone", "shape": { "op": "cone", "radius": 0.3, "height": 1, "sides": 24 } } ] }"#.into());
    lib.insert("pair".into(), r#"{ "name": "pair", "parts": [ { "name": "a", "include": "spike" }, { "name": "b", "include": "spike", "ops": [ { "op": "move", "by": [2, 0, 0] } ] } ] }"#.into());
    let m = make_with(r#"{ "name": "t", "parts": [ { "name": "slab", "material": "stone", "shape": { "op": "box", "size": [2, 0.2, 2] } }, { "name": "spikes", "include": "pair" } ] }"#, &lib);
    let mut paths: Vec<&str> = m.extras.made.iter().map(|x| x.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(paths, ["parts[0].shape", "parts[1].include(pair).parts[0].include(spike).parts[0].shape", "parts[1].include(pair).parts[1].include(spike).parts[0].shape"]);
    adds_up(&m);
    // The same form an error has there.
    lib.insert("bad".into(), r#"{ "name": "bad", "parts": [ { "name": "c", "material": "stone", "shape": { "op": "cone", "radius": -1 } } ] }"#.into());
    let s = style();
    let er = build(r#"{ "name": "t", "parts": [ { "name": "x", "include": "bad" } ] }"#, &BuildParams { library: Some(&lib), ..BuildParams::new(&s) }).unwrap_err();
    assert!(er.path.starts_with("parts[0].include(bad).parts[0].shape"), "{}", er.path);
}

/// What no record speaks for stays no record's: an included box beside an included terrain is
/// the box's own surface, and the terrain is the rest.
#[test]
fn what_no_record_speaks_for_is_the_rest() {
    let mut lib = BTreeMap::new();
    lib.insert(
        "kid".into(),
        r#"{ "name": "kid", "parts": [ { "name": "rock", "material": "stone", "shape": { "op": "box", "size": [0.2, 0.2, 0.2] } },
            { "name": "g", "material": "stone", "generate": { "op": "terrain", "size": [10, 10], "samples": [17, 17], "layers": [{ "scale": 4, "amplitude": 0.2 }] } } ] }"#
            .into(),
    );
    for ops in ["", r#", "ops": [ { "op": "scale", "by": 2 } ]"#] {
        let m = make_with(&format!(r#"{{ "name": "t", "parts": [ {{ "name": "ball", "material": "stone", "shape": {{ "op": "sphere", "radius": 1, "subdivisions": 3 }} }}, {{ "name": "k", "include": "kid"{ops} }} ] }}"#), &lib);
        let k = if ops.is_empty() { 1.0 } else { 4.0 };
        let rock = of(&m, "parts[1].include(kid).parts[0].shape").area_m2;
        assert!(near(rock, k * 0.24, 0.01), "the rock is its own 0.24 m2 (times {k}): {rock}");
        assert!(m.extras.made_rest_m2 > k * 99.0, "the terrain is the rest: {}", m.extras.made_rest_m2);
        adds_up(&m);
    }
}

/// A scatter records each item by how many were placed of it, under a path into its items; a
/// variant never placed records nothing, and an item's variants are one record.
#[test]
fn a_scatter_records_what_it_placed() {
    let m = make(
        r#"{ "name": "sc", "seed": 3, "parts": [ { "name": "things", "material": "stone", "generate": { "op": "scatter", "area": { "rect": [20, 20] }, "min_distance": 0.8, "count": 120,
          "items": [ { "shape": { "op": "box", "size": [0.3, 0.3, 0.3] }, "weight": 1 }, { "shape": { "op": "sphere", "radius": 0.15, "subdivisions": 2 }, "weight": 20, "variants": 3 } ] } } ] }"#,
    );
    let (boxes, balls) = (of(&m, "parts[0].generate.items[0].shape"), of(&m, "parts[0].generate.items[1].shape"));
    let placed = m.extras.stats.iter().find_map(|(_, j)| j.get("placed").and_then(|v| v.as_f64())).unwrap();
    let ball = of(&one(r#"{"op":"sphere","radius":0.15,"subdivisions":2}"#, ""), "parts[0].shape").area_m2;
    let (nb, nx) = (balls.area_m2 / ball, boxes.area_m2 / 0.54);
    // (A placement may turn and scale its copy by a little: counts within a tenth.)
    assert!(near(nb + nx, placed as f32, 0.1) && nb > 8.0 * nx, "{nb} balls and {nx} boxes of {placed} placed");
    assert_eq!(m.extras.made.len(), 2, "three variants of one ball are one record");
    adds_up(&m);
    // One placed of two items: only it is recorded.
    let m = make(
        r#"{ "name": "sc", "seed": 3, "parts": [ { "name": "things", "material": "stone", "generate": { "op": "scatter", "area": { "rect": [4, 4] }, "min_distance": 0.5, "count": 1,
          "items": [ { "shape": { "op": "box", "size": [0.3, 0.3, 0.3] } }, { "shape": { "op": "sphere", "radius": 0.15 } } ] } } ] }"#,
    );
    assert_eq!(m.extras.made.len(), 1, "{:?}", m.extras.made);
    adds_up(&m);
}

/// A sculpt's primitives are weighed by the surface each makes in the mesh: a repeat's and a
/// mirror's copies count, an elongation's length counts and widens its outline, and an
/// intersection's furthest primitive is what is left.
#[test]
fn a_sculpt_weighs_its_primitives_by_what_they_make() {
    let sdf = |tree: &str| one(&format!(r#"{{"op":"sdf","resolution":48,"tree":{tree}}}"#), "");
    let weights = |m: &Model| -> Vec<f32> { of(m, "parts[0].shape").views.iter().map(|v| v.1).collect() };
    // A box of 0.3 m and nine balls of 0.08 in a row on it: the balls are about half.
    let m = sdf(r#"{"op":"union","of":[{"op":"box","size":[0.3,0.3,0.3]},{"op":"sphere","radius":0.08,"at":[0,0.3,0],"repeat":{"count":[9,1,1],"spacing":[0.2,1,1]}}]}"#);
    let w = weights(&m);
    let (cube, balls) = (w[..3].iter().sum::<f32>(), w[3..].iter().sum::<f32>());
    assert!(balls > 0.8 * cube, "nine balls {balls} against the box {cube}");
    adds_up(&m);
    // Two arms by one mirror: both count.
    let m = sdf(r#"{"op":"union","of":[{"op":"box","size":[0.3,0.3,0.3]},{"op":"round_cone","from":[0.3,0,0],"to":[0.9,0,0],"radius":0.05,"radius_to":0.05,"mirror":"x"}]}"#);
    let w = weights(&m);
    let arm = 2.0 * PI * 0.05 * 0.6 + 4.0 * PI * 0.05 * 0.05;
    assert!(near(w[3..].iter().sum::<f32>(), 2.0 * arm, 0.15), "two arms {} m2, one {arm}", w[3..].iter().sum::<f32>());
    // A ball elongated into a slab: its outlines are the slab's, and so is its surface.
    let m = sdf(r#"{"op":"sphere","radius":0.05,"elongate":[0.6,0,0.6]}"#);
    let views = &of(&m, "parts[0].shape").views;
    let ext: Vec<[f32; 2]> = views.iter().map(|v| half_extents(&v.0)).collect();
    assert!(near(ext[0][0], 0.65, 0.01) && near(ext[0][1], 0.05, 0.01) && near(ext[1][0], 0.65, 0.01) && near(ext[1][1], 0.65, 0.01), "{ext:?}");
    assert!(of(&m, "parts[0].shape").area_m2 > 3.0, "a slab of 1.3 by 1.3 m: {}", of(&m, "parts[0].shape").area_m2);
    // A ball of 3 m and a box of 1 m inside it: the box is what is left.
    let m = sdf(r#"{"op":"intersection","of":[{"op":"sphere","radius":3},{"op":"box","size":[1,1,1]}]}"#);
    let v = &of(&m, "parts[0].shape").views;
    assert!(v.len() == 3 && v.iter().all(|(p, _)| p.len() == 4), "only the box's three rectangles: {v:?}");
    // What a difference takes away is not recorded, and the wall it cuts is the rest.
    let m = sdf(r#"{"op":"difference","of":[{"op":"box","size":[1,1,1]},{"op":"cylinder","radius":0.25,"height":2}]}"#);
    assert_eq!(of(&m, "parts[0].shape").views.len(), 3);
    assert!(near(m.extras.made_rest_m2, 2.0 * PI * 0.25, 0.1), "the hole's wall: {}", m.extras.made_rest_m2);
    adds_up(&m);
}

/// A group's stretch widens each outline along that outline's own axes: a ball in a union
/// stretched four times along Z is a circle from the front and an oval from above and the side.
#[test]
fn a_groups_stretch_keeps_each_outlines_axes() {
    let g = one(r#"{"op":"sdf","resolution":32,"tree":{"op":"union","stretch":[1,1,4],"scale":2,"of":[{"op":"sphere","radius":0.2}]}}"#, "");
    let p = one(r#"{"op":"sdf","resolution":32,"tree":{"op":"sphere","radius":0.2,"stretch":[1,1,4],"scale":2}}"#, "");
    let ext = |m: &Model| of(m, "parts[0].shape").views.iter().map(|v| half_extents(&v.0)).collect::<Vec<_>>();
    let e = ext(&g);
    assert!(near(e[0][0], 0.2, 0.01) && near(e[0][1], 0.2, 0.01) && near(e[1][1], 0.8, 0.01) && near(e[2][0], 0.8, 0.01), "{e:?}");
    assert_eq!(e, ext(&p), "a stretch on the group is a stretch on the ball");
}

/// A column's outline from the side closes past each end in the cap its field has there: a
/// limb is not a rectangle.
#[test]
fn a_columns_outline_has_its_caps() {
    let m = one(r#"{"op":"sdf","resolution":32,"tree":{"op":"column","from":[0,0,0],"to":[0,1,0],"sections":[[0,0.1,0.08],[1,0.06,0.05]]}}"#, "");
    let side = &of(&m, "parts[0].shape").views[1].0;
    let (lo, hi) = side.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
    assert!(near(lo, -0.08, 0.02) && near(hi, 1.05, 0.01), "past its ends by its caps: {lo} to {hi}");
    // Its widest points are its sections', and the tips are on its axis.
    assert!(side.iter().any(|p| p[1] == lo && p[0] == 0.0) && side.iter().all(|p| p[0].abs() <= 0.1 + 1e-6));
}

/// Every generator's own outlines: what each is from its sides.
#[test]
fn every_generator_records_its_own_outlines() {
    let views = |shape: &str| of(&one(shape, ""), "parts[0].shape").views.iter().map(|v| (half_extents(&v.0), v.0.len(), v.1)).collect::<Vec<_>>();
    // An extrude that tapers: from either side narrower at the top by its taper.
    let e = one(r#"{"op":"extrude","outline":{"rect":[2,1]},"height":1,"taper":0.5}"#, "");
    let side = &of(&e, "parts[0].shape").views[1].0;
    let top: Vec<f32> = side.iter().filter(|p| p[1] > 0.5).map(|p| p[0].abs()).collect();
    assert!(top.iter().all(|x| near(*x, 0.5, 0.01)) && side.iter().any(|p| near(p[0].abs(), 1.0, 0.01)), "{side:?}");
    // A loft: its outline, and from each side its stations where their shifts put them.
    let l = one(r#"{"op":"loft","outline":{"rect":[1,0.5]},"stations":[[1,0,0,0],[0.5,1,2,0]]}"#, "");
    let v = &of(&l, "parts[0].shape").views;
    assert_eq!(v.len(), 3);
    assert!(v[1].0.iter().any(|p| near(p[0], 2.25, 0.01) && p[1] == 1.0) && near(half_extents(&v[2].0)[0], 0.25, 0.01), "{v:?}");
    // A sweep: open, its profile and its path laid out straight; closed, its profile and its loop.
    let small = r#"{"rect":[0.2,0.2],"roundness":1,"segments":16}"#;
    let open = views(&format!(r#"{{"op":"sweep","profile":{small},"path":[[0,0,0],[0,2,0]],"scales":[1,0.5]}}"#));
    assert!(open.len() == 2 && near(open[1].0[1], 1.0, 0.01), "{open:?}");
    let ring = views(&format!(r#"{{"op":"sweep","profile":{small},"path":[[1,0,0],[0,0,1],[-1,0,0],[0,0,-1]],"closed":true,"smooth":4}}"#));
    assert!(ring.len() == 2 && ring[1].1 > 4 && near(ring[1].0[0], 1.0, 0.05) && near(ring[1].0[1], 1.0, 0.05), "a ring from above, as smoothed: {ring:?}");
    // A torus: a circle from above, a stadium from the side.
    let t = views(r#"{"op":"torus","radius":1,"tube":0.1}"#);
    assert!(near(t[0].0[0], 1.1, 1e-4) && near(t[1].0[0], 1.1, 1e-4) && near(t[1].0[1], 0.1, 1e-3), "{t:?}");
    // A capsule's height is from end to end.
    let c = views(r#"{"op":"capsule","radius":0.1,"height":3}"#);
    assert!(near(c[1].0[0], 1.5, 1e-3) && near(c[1].0[1], 0.1, 1e-3), "{c:?}");
    // A plane, a rectangle; a frustum, its wider radius from above and a trapezoid.
    assert_eq!(views(r#"{"op":"plane","size":[2,1]}"#)[0].0, [1.0, 0.5]);
    let f = views(r#"{"op":"cylinder","radius":1,"radius_top":0.5,"height":2,"sides":32}"#);
    assert!(f[0].0 == [1.0, 1.0] && f[1].1 == 4 && f[1].0 == [1.0, 1.0], "{f:?}");
    // An sdf cylinder's corner rounds its rims.
    let s = of(&one(r#"{"op":"sdf","resolution":24,"tree":{"op":"cylinder","radius":0.5,"height":1,"corner":0.2}}"#, ""), "parts[0].shape").views.clone();
    assert!(s[1].0.len() > 4, "a rounded side: {:?}", s[1].0);
}

/// A recipe's `finish` scales its records by what it did to the parts.
#[test]
fn a_finish_scales_what_was_recorded() {
    let plain = parts(&[part("a", r#"{"op":"box","size":[1,1,1]}"#, "")]);
    let grown = make(r#"{ "name": "t", "parts": [ { "name": "a", "material": "stone", "shape": {"op":"box","size":[1,1,1]} } ], "finish": [ { "op": "scale", "by": 3 } ] }"#);
    assert!(near(of(&grown, "parts[0].shape").area_m2, 9.0 * of(&plain, "parts[0].shape").area_m2, 1e-3));
    adds_up(&grown);
}

/// The record is bounded: the [`specula_model::MAX_MADE`] shapes with the most surface, and at
/// most [`MAX_RECORD_VIEWS`] outlines in all; what is past them is the rest.
#[test]
fn the_record_is_bounded() {
    let many: Vec<String> = (0..70).map(|i| part(&format!("b{i}"), &format!(r#"{{"op":"box","size":[{},1,1]}}"#, 1.0 + 0.1 * i as f32), &format!(r#"{{"op":"move","by":[{},0,0]}}"#, 20 * i))).collect();
    let m = parts(&many);
    assert_eq!(m.extras.made.len(), 64, "the 64 largest shapes are kept");
    // (The six smallest left out: boxes of 1 to 1.5 m.)
    let left: f32 = (0..6).map(|i| { let x = 1.0 + 0.1 * i as f32; 2.0 + 4.0 * x }).sum();
    assert!(near(m.extras.made_rest_m2, left, 0.01) && m.extras.made.iter().all(|x| x.path != "parts[0].shape"), "{}", m.extras.made_rest_m2);
    adds_up(&m);
    // Sculpts of many primitives, every outline different: 6 of 16 ellipsoids, 288 outlines (each
    // sculpt larger than the one before); 256 in all are kept, the largest sculpts' first, and
    // the rest is an outline of no points.
    let sculpts: Vec<String> = (0..6)
        .map(|s| {
            let balls: Vec<String> = (0..16).map(|i| format!(r#"{{"op":"ellipsoid","radii":[{},{},{}],"at":[{},0,0]}}"#, 0.1 + 0.002 * i as f32 + 0.03 * s as f32, 0.1 + 0.01 * i as f32, 0.12 + 0.003 * i as f32, 0.3 * i as f32)).collect();
            part(&format!("s{s}"), &format!(r#"{{"op":"sdf","resolution":64,"tree":{{"op":"union","of":[{}]}}}}"#, balls.join(",")), &format!(r#"{{"op":"move","by":[0,{},0]}}"#, 3 * s))
        })
        .collect();
    let m = parts(&sculpts);
    let total: usize = m.extras.made.iter().map(|x| x.views.iter().filter(|v| !v.0.is_empty()).count()).sum();
    let short: Vec<&str> = m.extras.made.iter().filter(|x| x.views.iter().any(|v| v.0.is_empty())).map(|x| x.path.as_str()).collect();
    assert!(total == 256 && total == MAX_RECORD_VIEWS && short == ["parts[0].shape"], "{total} outlines; cut short: {short:?}");
    // (Six of 48 outlines: the five largest sculpts keep all of theirs, the smallest 16 of 48.)
    adds_up(&m);
}

/// A sculpt keeps the outlines of its largest primitives, by the surface their own triangles
/// have, wherever they are in its tree; what is past [`MAX_VIEWS`] (96) is one outline of no
/// points, not read. Forty small balls before a large box: the box's three rectangles are kept.
#[test]
fn a_sculpt_keeps_its_largest_outlines() {
    let mut kids: Vec<String> = (0..40).map(|i| format!(r#"{{"op":"sphere","radius":{},"at":[{},0,{}]}}"#, 0.06 + 0.0005 * i as f32, 0.15 * (i % 8) as f32, 0.15 * (i / 8) as f32)).collect();
    kids.push(r#"{"op":"box","size":[1.5,1.5,1.5],"at":[3,0,0]}"#.to_string());
    let m = one(&format!(r#"{{"op":"sdf","resolution":64,"tree":{{"op":"union","of":[{}]}}}}"#, kids.join(",")), "");
    let r = of(&m, "parts[0].shape");
    let drawn: Vec<&(Vec<[f32; 2]>, f32)> = r.views.iter().filter(|v| !v.0.is_empty()).collect();
    let squares = drawn.iter().filter(|v| v.0.len() == 4).count();
    let unread: f32 = r.views.iter().filter(|v| v.0.is_empty()).map(|v| v.1).sum();
    assert!(drawn.len() == 96 && drawn.len() == MAX_VIEWS && squares == 3, "{} outlines, {squares} of them the box's", drawn.len());
    // (The outlines left out are the smallest balls': their weight, and nothing of the box's.)
    let total: f32 = r.views.iter().map(|v| v.1).sum();
    assert!(unread > 0.0 && unread < 0.1 * total && near(r.area_m2, surface(&m), 1e-3), "unread {unread} of {total}; {} of {}", r.area_m2, surface(&m));
}

/// No more than the first 512 shapes built are recorded (the record of a build is bounded
/// before the largest are chosen): three includes of a row of 200 boxes, each box larger than
/// the one before and each include's three times the last's, keep the 64 largest of the first
/// 512 (the third include's first 112).
#[test]
fn the_shapes_recorded_while_building_are_bounded() {
    let row: Vec<String> = (0..200).map(|i| part(&format!("b{i}"), &format!(r#"{{"op":"box","size":["k * {}",0.1,0.1]}}"#, 0.1 + 0.01 * i as f32), &format!(r#"{{"op":"move","by":[{},0,0]}}"#, 10 * i))).collect();
    let mut lib = BTreeMap::new();
    lib.insert("row".to_string(), format!(r#"{{ "name": "row", "params": {{ "k": {{ "default": 1, "min": 1, "max": 9 }} }}, "parts": [{}] }}"#, row.join(", ")));
    let includes: Vec<String> = (0..3).map(|i| format!(r#"{{ "name": "r{i}", "include": "row", "params": {{ "k": {} }}, "ops": [{{"op":"move","by":[0,0,{}]}}] }}"#, 3u32.pow(i), 5 * i)).collect();
    let m = make_with(&format!(r#"{{ "name": "t", "parts": [{}] }}"#, includes.join(", ")), &lib);
    let largest = m.extras.made.iter().map(|x| x.path.as_str()).next();
    assert_eq!(largest, Some("parts[2].include(row).parts[111].shape"), "the last shape recorded is the 512th");
    assert_eq!(m.extras.made.len(), 64);
}

/// **The record never refuses a build**: 52 sculpts of 32 ellipsoids each (a recipe that built
/// before the record asked its triangles anything, and was refused by the second version for
/// work) builds. Asking which primitive made each triangle spends a limit of its own, 120
/// million units, and never passes it (within one question's work of it); the triangles not asked
/// within it are no record's, the rest.
#[test]
fn the_record_never_refuses_a_build() {
    let mut ps = Vec::new();
    for i in 0..52 {
        let kids: Vec<String> = (0..32).map(|j| format!(r#"{{"op":"ellipsoid","radii":[{},{},{}],"at":[{},0,0]}}"#, 0.05 + 0.001 * j as f32, 0.05 + 0.0013 * (i * 32 + j) as f32 / 64.0, 0.06 + 0.0007 * j as f32, 0.06 * j as f32)).collect();
        ps.push(part(&format!("s{i}"), &format!(r#"{{"op":"sdf","resolution":48,"tree":{{"op":"union","of":[{}]}}}}"#, kids.join(",")), &format!(r#"{{"op":"move","by":[0,{},0]}}"#, i as f32 * 0.3)));
    }
    let m = parts(&ps);
    let sum = |key: &str| m.extras.stats.iter().map(|(_, j)| j.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)).sum::<f64>();
    let (work, unanswered) = (sum("owners_work"), sum("owners_unanswered"));
    // (One question costs what the tree's 32 ellipsoids and their group cost to evaluate.)
    assert!(work > 120e6 - 4_000.0 && work <= 120e6, "{work} units spent asking");
    assert!(unanswered > 0.0 && m.extras.made_rest_m2 > 0.0, "{unanswered} triangles unanswered, rest {}", m.extras.made_rest_m2);
    adds_up(&m);
}

/// A group's stretch is put on a round cone's and a column's outlines along the axis they run
/// on: a round cone standing along Y in a group stretched three times along Y is from the side
/// three times as long, as the same cone built three times as long is; a column lying along X
/// in a group stretched along X the same.
#[test]
fn a_groups_stretch_lands_on_the_axis_a_limb_runs_on() {
    let side = |tree: &str| -> [f32; 2] {
        let m = one(&format!(r#"{{"op":"sdf","resolution":40,"tree":{tree}}}"#), "");
        let v = &of(&m, "parts[0].shape").views;
        half_extents(&v[1].0)
    };
    let stretched = side(r#"{"op":"union","stretch":[1,3,1],"of":[{"op":"round_cone","from":[0,0,0],"to":[0,0.5,0],"radius":0.1,"radius_to":0.02}]}"#);
    let plain = side(r#"{"op":"round_cone","from":[0,0,0],"to":[0,0.5,0],"radius":0.1,"radius_to":0.02}"#);
    // (Along: its length and the two balls, 0.5 + 0.1 + 0.02, three times; across: as it was.)
    assert!(near(stretched[0], 3.0 * plain[0], 0.01) && near(stretched[1], plain[1], 0.01), "{stretched:?} against {plain:?}");
    let col = |stretch: &str| side(&format!(r#"{{"op":"union","stretch":{stretch},"of":[{{"op":"column","from":[0,0,0],"to":[1,0,0],"side":[0,0,1],"sections":[[0,0.1,0.05],[1,0.1,0.05]]}}]}}"#));
    let (one_, three) = (col("[1,1,1]"), col("[3,1,1]"));
    // (Its first side, toward `side` (Z) and along X: three times as long, as wide as it was.)
    assert!(near(three[1], 3.0 * one_[1], 0.01) && near(three[0], one_[0], 0.01), "{three:?} against {one_:?}");
}

//! Budget guards from the procgen review (2026-10-22): each worst case the reviewer timed is
//! refused by its work budget, and the units it is charged are checked against what it did
//! (evaluations, classifications, candidates), never against the clock. Also the sparse SDF
//! sampler against dense sampling with unbounded displaced children, CSG's degenerate inputs
//! and results, texture memory per model, and cancelling a running build.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use specula_model::csg::{evaluate, Budget, CsgErrorKind, CsgOp, CsgTree};
use specula_model::math3::Affine;
use specula_model::sdf::{eval_cost, mesh, Combine, Kind, MeshOptions, Modifiers, Prim, SdfNode};
use specula_model::work::{self, MAX_BUILD};
use specula_model::{build, BuildParams, ErrorCode, Shape, Style};

fn prim(p: Prim) -> SdfNode {
    SdfNode { kind: Kind::Prim(p), m: Modifiers::default() }
}

fn displaced(octaves: u32) -> Modifiers {
    Modifiers { displace: Some((1.0, 0.6, octaves)), ..Modifiers::default() }
}

/// A union with a far-away smooth sphere: its slack is so large no block is ever skipped, so
/// the mesh is the dense reference.
fn dense(mut t: SdfNode) -> SdfNode {
    let mut far = prim(Prim::Sphere { radius: 0.1 });
    far.m.at = [0.0, 500.0, 0.0];
    let far = SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![far], smooth: 50.0 }, m: Modifiers::default() };
    if let Kind::Group { kids, .. } = &mut t.kind {
        kids.push(far);
    }
    t
}

#[test]
fn sparse_sampling_keeps_unbounded_displaced_children() {
    let opts = MeshOptions { resolution: 64, bounds: Some(([-1.6; 3], [1.6; 3])), seed: 1, max_triangles: 1 << 21, max_work: MAX_BUILD, color: [1.0; 4] };
    let half_space = || prim(Prim::HalfSpace { normal: [0.0, 1.0, 0.0], offset: 0.0 });
    // A box minus a displaced half-space (the cutter unbounded, its noise ignored before).
    let cut = SdfNode {
        kind: Kind::Group {
            op: Combine::Difference,
            kids: vec![prim(Prim::Box { half: [1.5; 3], round: 0.0 }), SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![half_space()], smooth: 0.0 }, m: displaced(3) }],
            smooth: 0.0,
        },
        m: Modifiers::default(),
    };
    let (sparse, st) = mesh(&cut, &opts).unwrap();
    let (reference, rst) = mesh(&dense(cut.clone()), &opts).unwrap();
    assert_eq!(rst.blocks_skipped, 0);
    let (v, vr) = (sparse.signed_volume(), reference.signed_volume());
    assert!((v - vr).abs() < 1e-4 * vr, "sparse {v} vs dense {vr} ({} blocks skipped)", st.blocks_skipped);
    // A displaced half-space alone (the root unbounded): the same.
    let root = SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![half_space()], smooth: 0.0 }, m: displaced(3) };
    let o2 = MeshOptions { bounds: Some(([-2.0; 3], [2.0; 3])), ..opts };
    let (s, _) = mesh(&root, &o2).unwrap();
    let (r, _) = mesh(&dense(root), &o2).unwrap();
    assert!((s.signed_volume() - r.signed_volume()).abs() < 1e-4 * r.signed_volume());
}

#[test]
fn sdf_work_weighs_noise_and_refuses_the_worst_case() {
    // Noise octaves weigh what they cost: a displaced sphere far more than a plain one.
    let plain = eval_cost(&prim(Prim::Sphere { radius: 1.0 }));
    let six = eval_cost(&SdfNode { kind: Kind::Prim(Prim::Sphere { radius: 1.0 }), m: displaced(6) });
    assert!(six >= 5 * plain, "{plain} vs {six}");
    // Every sample evaluated is charged at least the tree's cost.
    let t = SdfNode { kind: Kind::Prim(Prim::Sphere { radius: 1.0 }), m: displaced(4) };
    let (_, st) = mesh(&t, &MeshOptions { resolution: 32, bounds: None, seed: 1, max_triangles: 1 << 20, max_work: MAX_BUILD, color: [1.0; 4] }).unwrap();
    assert!(st.work >= st.samples_evaluated * eval_cost(&t), "{st:?}");
    // The reviewer's worst case (127 spheres with 6-octave noise at resolution 160, 212 s
    // before): refused by the budget, having charged no more than its cap plus one block.
    let kids: Vec<SdfNode> = (0..127)
        .map(|i| {
            let mut m = Modifiers { displace: Some((2.0, 0.05, 6)), ..Modifiers::default() };
            m.at = [((i * 37) % 100) as f32 * 0.04 - 2.0, ((i * 53) % 100) as f32 * 0.04 - 2.0, ((i * 71) % 100) as f32 * 0.04 - 2.0];
            SdfNode { kind: Kind::Prim(Prim::Sphere { radius: 0.6 }), m }
        })
        .collect();
    let worst = SdfNode { kind: Kind::Group { op: Combine::Union, kids, smooth: 0.0 }, m: Modifiers::default() };
    let cost = eval_cost(&worst);
    let e = mesh(&worst, &MeshOptions { resolution: 160, bounds: Some(([-3.0, -3.0, -2.8], [3.0, 3.0, 2.8])), seed: 1, max_triangles: 600_000, max_work: MAX_BUILD, color: [1.0; 4] }).unwrap_err();
    assert!(e.limit, "{e:?}");
    assert!(e.measured.unwrap() <= (MAX_BUILD + 9 * 9 * 9 * cost) as f64, "{e:?}");
}

fn cyl(r: f32, h: f32, sides: u32) -> Shape {
    Shape::cylinder(r, r, h, sides, 0.0).unwrap()
}

#[test]
fn csg_worst_cases_are_refused_with_exact_predicates_charged() {
    let rotated = CsgTree::Op(
        CsgOp::Union,
        (0..48)
            .map(|i| {
                let mut c = cyl(0.5, 1.0, 96);
                c.transform(&Affine::rotate_euler_deg([0.0, i as f64 * 0.37, 0.0]));
                CsgTree::Leaf(c)
            })
            .collect(),
    );
    let mut kids = vec![CsgTree::Leaf(Shape::sphere(1.0, 4))];
    for i in 0..12 {
        let mut c = cyl(0.2, 3.0, 48);
        c.transform(&Affine::translate([0.0, -1.5, 0.0]));
        c.transform(&Affine::rotate_euler_deg([i as f64 * 15.0, i as f64 * 29.0, 0.0]));
        c.transform(&Affine::translate([0.0, 1.0, 0.0]));
        kids.push(CsgTree::Leaf(c));
    }
    let sphere_cut = CsgTree::Op(CsgOp::Difference, kids);
    for t in [rotated, sphere_cut] {
        let mut b = Budget::default();
        let e = evaluate(&t, &mut b).unwrap_err();
        assert_eq!(e.kind, CsgErrorKind::Limit, "{e:?}");
        // Exact predicates were taken, and charged on top of the classifications.
        assert!(b.exact > 0 && b.work >= b.exact * work::CSG_EXACT, "work {} exact {}", b.work, b.exact);
        assert!(b.work <= MAX_BUILD + 64 * work::CSG_CLASSIFY + b.exact * work::CSG_EXACT);
    }
    // 64 identical cylinders: coplanar everywhere, every orientation exact.
    let same = CsgTree::Op(CsgOp::Intersection, (0..16).map(|_| CsgTree::Leaf(cyl(0.5, 1.0, 32))).collect());
    let mut b = Budget::default();
    evaluate(&same, &mut b).unwrap();
    assert!(b.exact > 1000, "{}", b.exact);
}

fn bx(size: [f32; 3], at: [f32; 3]) -> Shape {
    let mut s = Shape::cuboid(size, 0.0).unwrap();
    s.transform(&Affine::translate([at[0], at[1] - size[1] * 0.5, at[2]]));
    s
}

#[test]
fn csg_refuses_what_the_grid_cant_hold_and_slivers() {
    let run = |t: CsgTree| {
        let mut b = Budget::default();
        evaluate(&t, &mut b).map(|(s, _, st)| (s, st))
    };
    use CsgTree::{Leaf, Op};
    // A 1 cm cut on a 10 km slab: smaller than the grid step there.
    let Err(e) = run(Op(CsgOp::Difference, vec![Leaf(bx([10000.0, 10.0, 10000.0], [0.0; 3])), Leaf(bx([0.01; 3], [3.0, 5.0, 3.0]))])) else { panic!("a 1 cm cut on 10 km") };
    assert!(e.message.contains("smaller than the grid"), "{}", e.message);
    // A box minus itself nudged by 1 µm or 30 µm, or turned 0.001°: only slivers are left.
    for shift in [1e-6f32, 3e-5] {
        let Err(e) = run(Op(CsgOp::Difference, vec![Leaf(bx([1.0; 3], [0.0; 3])), Leaf(bx([1.0; 3], [shift, 0.0, 0.0]))])) else { panic!("a nudged self-difference") };
        assert!(e.message.contains("no volume"), "{shift}: {}", e.message);
    }
    let mut turned = bx([1.0; 3], [0.0; 3]);
    turned.transform(&Affine::rotate_euler_deg([0.0, 0.001, 0.0]));
    let Err(e) = run(Op(CsgOp::Difference, vec![Leaf(bx([1.0; 3], [0.0; 3])), Leaf(turned)])) else { panic!("a turned self-difference") };
    assert!(e.message.contains("no volume"), "{}", e.message);
    // A real thin wall (1 mm) is kept; a sliver beside a real result is dropped and counted.
    let (s, st) = run(Op(CsgOp::Difference, vec![Leaf(bx([1.0; 3], [0.0; 3])), Leaf(bx([1.0; 3], [0.001, 0.0, 0.0]))])).map_err(|e| e.message).unwrap();
    assert!((s.signed_volume() - 0.001).abs() < 1e-5 && st.slivers == 0, "{} {st:?}", s.signed_volume());
    // A cut leaving a detached 30 µm sliver beside half the box: the sliver goes.
    let w = 0.5 - 3e-5;
    let (s, st) = run(Op(CsgOp::Difference, vec![Leaf(bx([1.0; 3], [0.0; 3])), Leaf(bx([w, 2.0, 2.0], [-0.5 + 3e-5 + w * 0.5, -0.5, 0.0]))])).map_err(|e| e.message).unwrap();
    assert!(st.slivers == 1 && (s.signed_volume() - 0.5).abs() < 1e-4, "{} {st:?}", s.signed_volume());
}

fn style() -> Style {
    Style::parse(r##"{ "style": "t", "palette": { "a": "#4f6b3a" }, "materials": { "stone": { "color": "#8c96a3" } } }"##).unwrap()
}

#[test]
fn texture_memory_and_work_are_capped_per_model() {
    let s = style();
    let mut lib = std::collections::BTreeMap::new();
    // Each 1024 set with a normal map is 16 MiB with mips; the second also glows (21 MiB).
    lib.insert("t1".to_string(), r#"{ "texgraph": 1, "size": 1024, "nodes": { "n": { "op": "gradient" } }, "outputs": { "base_color": "n", "height": "n", "normal_strength": 2 } }"#.to_string());
    lib.insert("t2".to_string(), r#"{ "texgraph": 1, "size": 1024, "nodes": { "n": { "op": "gradient" } }, "outputs": { "base_color": "n", "height": "n", "normal_strength": 2, "emissive": "n" } }"#.to_string());
    let recipe = r#"{ "name": "two", "textures": { "a": "t1", "b": "t2" }, "parts": [
        { "name": "p", "material": "stone", "texture": "a", "shape": { "op": "box", "size": [1, 1, 1] } },
        { "name": "q", "material": "stone", "texture": "b", "shape": { "op": "box", "size": [1, 1, 1] }, "ops": [ { "op": "move", "by": [2, 0, 0] } ] } ] }"#;
    let Err(e) = build(recipe, &BuildParams { library: Some(&lib), ..BuildParams::new(&s) }) else { panic!("two 1024 sets with normal maps are over the cap") };
    assert_eq!(e.code, ErrorCode::Limit, "{e}");
    assert!(e.message.contains("GPU bytes"), "{e}");
    // One of them fits.
    let one = recipe.replace("\"texture\": \"b\"", "\"texture\": \"a\"");
    let m = build(&one, &BuildParams { library: Some(&lib), ..BuildParams::new(&s) }).unwrap_or_else(|e| panic!("{e}"));
    assert!(m.extras.textures.iter().map(|(_, t)| t.gpu_bytes()).sum::<u64>() <= specula_model::texgraph::MAX_MODEL_GPU_BYTES);
}

/// A comb polygon's points as recipe text.
fn comb_text() -> String {
    let teeth = 254;
    let w = 200.0 / teeth as f64;
    let mut pts = vec!["[-100, -100]".to_string(), "[100, -100]".to_string(), "[100, -99]".to_string()];
    for t in (0..teeth).rev() {
        let xl = -100.0 + t as f64 * w + w * 0.45;
        let xr = xl + w * 0.1;
        pts.extend([format!("[{xr}, -99]"), format!("[{xr}, 100]"), format!("[{xl}, 100]"), format!("[{xl}, -99]")]);
    }
    pts.push("[-100, -99]".to_string());
    format!("[{}]", pts.join(","))
}

#[test]
fn recipe_scatters_are_charged_to_the_build() {
    let s = style();
    let area = comb_text();
    let part = |k: usize, count: u32| format!(r#"{{ "name": "s{k}", "material": "stone", "generate": {{ "op": "scatter", "area": {area}, "min_distance": 0.4, "count": {count}, "attempts": 64, "items": [{{ "shape": {{ "op": "box", "size": [0.1, 0.1, 0.1] }} }}] }} }}"#);
    // Eight comb scatters of one point each (54 s before): each stops at its point.
    let parts: Vec<String> = (0..8).map(|k| part(k, 1)).collect();
    let m = build(&format!(r#"{{ "name": "evade", "lods": [], "parts": [{}] }}"#, parts.join(",")), &BuildParams::new(&s)).unwrap_or_else(|e| panic!("{e}"));
    let report = m.report().to_string();
    assert!(report.contains("\"placed\":1"), "{report}");
    // Filling all eight: the build's budget runs out with a limit error at a scatter.
    let parts: Vec<String> = (0..8).map(|k| part(k, 4096)).collect();
    let e = build(&format!(r#"{{ "name": "fill", "lods": [], "budget": {{ "max_triangles": 400000 }}, "parts": [{}] }}"#, parts.join(",")), &BuildParams::new(&s)).unwrap_err();
    assert_eq!(e.code, ErrorCode::Limit, "{e}");
}

/// The reviewer's tier-1 recipe (round 2): a 180² plane, 63 six-octave displaces, four LODs
/// and a hull, 12.2 s with nothing charged before.
fn tier1(displaces: usize) -> String {
    let ops: Vec<String> = (0..displaces).map(|i| format!(r#"{{"op": "displace", "amount": 0.01, "scale": {}, "octaves": 6}}"#, 0.1 * (i % 9 + 1) as f64)).collect();
    format!(
        r#"{{"recipe": 1, "name": "tier1_a", "lods": [0.5, 0.25, 0.12, 0.06], "collider": "convex_hull", "budget": {{"max_triangles": 200000}},
            "parts": [{{"name": "p0", "material": "stone", "shape": {{"op": "plane", "size": [10, 10], "divisions": [180, 180]}}, "ops": [{}]}}]}}"#,
        ops.join(",")
    )
}

#[test]
fn tier_one_ops_lods_and_colliders_are_charged() {
    let s = style();
    let Err(e) = build(&tier1(63), &BuildParams::new(&s)) else { panic!("63 displaces of 32k vertices are over the budget") };
    assert_eq!(e.code, ErrorCode::Limit, "{e}");
    assert!(e.path.starts_with("parts[0].ops") && e.message.contains("work units"), "{e}");
    // The units it reached are the vertices times the displace weight, op by op: it stops at
    // the op that would pass the budget, so never beyond it.
    assert!(e.measured.unwrap() > MAX_BUILD as f64 && e.measured.unwrap() <= (MAX_BUILD + 32_761 * (work::TIER1_DISPLACE + 6 * work::TIER1_OCTAVE)) as f64, "{e:?}");
    // A few displaces fit, with the LOD attempts and the hull charged too.
    build(&tier1(4), &BuildParams::new(&s)).unwrap_or_else(|e| panic!("{e}"));
    // A cancelled tier-1 build stops.
    let flag = Arc::new(AtomicBool::new(true));
    let _guard = work::cancel_on(Arc::clone(&flag));
    let Err(e) = build(&tier1(4), &BuildParams::new(&s)) else { panic!("cancelled") };
    assert!(e.message.contains("cancelled"), "{e}");
}

#[test]
fn a_cancelled_build_stops() {
    let s = style();
    let flag = Arc::new(AtomicBool::new(true));
    let _guard = work::cancel_on(Arc::clone(&flag));
    let sculpt = r#"{ "name": "rock", "parts": [ { "name": "r", "material": "stone", "shape": { "op": "sdf", "resolution": 40, "tree": { "op": "sphere", "radius": 1 } } } ] }"#;
    let e = build(sculpt, &BuildParams::new(&s)).unwrap_err();
    assert!(e.message.contains("cancelled"), "{e}");
    let boolean = r#"{ "name": "cut", "parts": [ { "name": "c", "material": "stone", "shape": { "op": "difference", "of": [ { "op": "box", "size": [1, 1, 1] }, { "op": "box", "size": [0.5, 2, 0.5] } ] } } ] }"#;
    let e = build(boolean, &BuildParams::new(&s)).unwrap_err();
    assert!(e.message.contains("cancelled"), "{e}");
    // Uncancelled, the same builds go through.
    flag.store(false, Ordering::Relaxed);
    build(sculpt, &BuildParams::new(&s)).unwrap();
    build(boolean, &BuildParams::new(&s)).unwrap();
}

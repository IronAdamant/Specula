use super::*;
use crate::math3::Affine;

fn cube(size: [f32; 3], at: [f32; 3]) -> Shape {
    let mut s = Shape::cuboid(size, 0.0).unwrap();
    // Cuboids rest on y = 0: centre them on `at`.
    s.transform(&Affine::translate([at[0], at[1] - size[1] * 0.5, at[2]]));
    s
}

fn run(t: &CsgTree) -> (Shape, CsgStats) {
    let mut b = Budget::default();
    let (s, leaf, st) = evaluate(t, &mut b).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(leaf.len(), s.tris.len());
    assert_eq!(unbalanced_edges(&s), 0, "closed");
    (s, st)
}

/// Directed edges (after welding equal points) not matched by as many reverse edges.
pub(crate) fn unbalanced_edges(s: &Shape) -> usize {
    let mut w = s.clone();
    w.weld_exact();
    let mut e = std::collections::BTreeMap::new();
    for t in &w.tris {
        for k in 0..3 {
            *e.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
        }
    }
    e.iter().filter(|(&(a, b), &n)| e.get(&(b, a)).copied().unwrap_or(0) != n).count()
}

fn leaf(s: Shape) -> CsgTree {
    CsgTree::Leaf(s)
}

fn approx(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn union_of_overlapping_boxes_has_the_right_volume() {
    let t = CsgTree::Op(CsgOp::Union, vec![leaf(cube([2.0; 3], [0.0; 3])), leaf(cube([2.0; 3], [1.0, 1.0, 1.0]))]);
    let (s, _) = run(&t);
    // 8 + 8 - 1 (the overlap is a unit cube).
    assert!(approx(s.signed_volume(), 15.0, 1e-3), "{}", s.signed_volume());
}

#[test]
fn difference_cuts_a_hole_through() {
    let wall = cube([4.0, 3.0, 0.4], [0.0, 1.5, 0.0]);
    let window = cube([1.0, 1.2, 1.0], [0.0, 1.6, 0.0]);
    let (s, st) = run(&CsgTree::Op(CsgOp::Difference, vec![leaf(wall), leaf(window)]));
    assert!(approx(s.signed_volume(), 4.0 * 3.0 * 0.4 - 1.0 * 1.2 * 0.4, 1e-3), "{}", s.signed_volume());
    assert!(st.output_triangles >= 24, "{st:?}");
    // No vertex inside the opening.
    assert!(s.positions.iter().all(|p| !(p[0].abs() < 0.49 && (p[1] - 1.6).abs() < 0.59)));
}

#[test]
fn intersection_of_boxes() {
    let t = CsgTree::Op(CsgOp::Intersection, vec![leaf(cube([2.0; 3], [0.0; 3])), leaf(cube([2.0; 3], [1.0, 0.5, 0.0]))]);
    let (s, _) = run(&t);
    assert!(approx(s.signed_volume(), 1.0 * 1.5 * 2.0, 1e-3), "{}", s.signed_volume());
}

#[test]
fn coplanar_faces_are_exact() {
    // Two boxes sharing a face exactly: the union is one box, with no internal faces.
    let a = cube([1.0, 1.0, 1.0], [0.0, 0.5, 0.0]);
    let b = cube([1.0, 1.0, 1.0], [1.0, 0.5, 0.0]);
    let (s, _) = run(&CsgTree::Op(CsgOp::Union, vec![leaf(a.clone()), leaf(b.clone())]));
    assert!(approx(s.signed_volume(), 2.0, 1e-4));
    // No triangle lies in the shared plane x = 0.5.
    let inner = s.tris.iter().filter(|t| t.iter().all(|&i| (s.positions[i as usize][0] - 0.5).abs() < 1e-5)).count();
    assert_eq!(inner, 0);
    // A cutter flush with a face: difference keeps the rest exactly.
    let c = cube([1.0, 1.0, 1.0], [0.0, 0.5, 0.0]);
    let notch = cube([0.5, 1.0, 1.0], [0.25, 0.5, 0.0]);
    let (d, _) = run(&CsgTree::Op(CsgOp::Difference, vec![leaf(c), leaf(notch)]));
    assert!(approx(d.signed_volume(), 0.5, 1e-4), "{}", d.signed_volume());
}

#[test]
fn nested_trees_and_many_cutters() {
    let wall = cube([6.0, 3.0, 0.5], [0.0, 1.5, 0.0]);
    let mut cutters = Vec::new();
    for i in 0..4 {
        cutters.push(leaf(cube([0.8, 1.2, 1.0], [-2.25 + 1.5 * i as f32, 1.6, 0.0])));
    }
    let round = Shape::cylinder(0.3, 0.3, 2.0, 16, 0.0).unwrap();
    let mut round = round;
    round.transform(&Affine::translate([0.0, -1.0, 0.0]));
    round.transform(&Affine::rotate_axis_deg(0, 90.0));
    round.transform(&Affine::translate([0.0, 2.6, 0.0]));
    let mut kids = vec![leaf(wall)];
    kids.push(CsgTree::Op(CsgOp::Union, cutters));
    kids.push(leaf(round));
    let (s, st) = run(&CsgTree::Op(CsgOp::Difference, kids));
    let expect = 6.0 * 3.0 * 0.5 - 4.0 * 0.8 * 1.2 * 0.5;
    // The round hole takes about pi r^2 * 0.5 (a 16-gon a little less).
    let v = s.signed_volume();
    assert!(v < expect && v > expect - 0.15, "{v} vs {expect}");
    assert!(st.work > 0 && st.polygons > 0);
}

#[test]
fn spheres_and_cylinders_stay_watertight() {
    // Curved operands meeting at awkward angles (many near-degenerate splits).
    let mut rng = specula_core::rng::Rng::new(7, 3);
    for case in 0..12 {
        let a = Shape::sphere(1.0, 2);
        let mut b = Shape::cylinder(0.4 + rng.f32() * 0.3, 0.4, 2.5, 12 + (case % 5) as u32, 0.0).unwrap();
        b.transform(&Affine::translate([0.0, -1.25, 0.0]));
        b.transform(&Affine::rotate_euler_deg([(rng.f32() * 90.0) as f64, (rng.f32() * 90.0) as f64, 0.0]));
        b.transform(&Affine::translate([rng.f32() * 0.6 - 0.3, 1.0 + rng.f32() * 0.4 - 0.2, rng.f32() * 0.6 - 0.3]));
        for op in [CsgOp::Union, CsgOp::Difference, CsgOp::Intersection] {
            let t = CsgTree::Op(op, vec![leaf(a.clone()), leaf(b.clone())]);
            let (s, _) = run(&t);
            let (va, vb, v) = (a.signed_volume(), b.signed_volume(), s.signed_volume());
            match op {
                CsgOp::Union => assert!(v >= va.max(vb) - 1e-3 && v <= va + vb + 1e-3, "case {case} union {v}"),
                CsgOp::Difference => assert!(v <= va + 1e-3 && v >= va - vb - 1e-3, "case {case} difference {v}"),
                CsgOp::Intersection => assert!(v <= va.min(vb) + 1e-3 && v >= -1e-4, "case {case} intersection {v}"),
            }
        }
    }
}

#[test]
fn volumes_add_up() {
    // |A ∪ B| = |A| + |B| - |A ∩ B| and |A - B| = |A| - |A ∩ B|.
    let a = Shape::sphere(1.0, 2);
    let mut b = cube([1.2, 1.2, 1.2], [0.6, 1.4, 0.3]);
    b.transform(&Affine::rotate_about_deg([0.3, 1.0, 0.2], 27.0));
    let vol = |op| run(&CsgTree::Op(op, vec![leaf(a.clone()), leaf(b.clone())])).0.signed_volume();
    let (u, d, i) = (vol(CsgOp::Union), vol(CsgOp::Difference), vol(CsgOp::Intersection));
    let (va, vb) = (a.signed_volume(), b.signed_volume());
    assert!(approx(u, va + vb - i, 1e-3), "{u} {va} {vb} {i}");
    assert!(approx(d, va - i, 1e-3), "{d} {va} {i}");
}

#[test]
fn deterministic_bits() {
    let mk = || {
        let a = Shape::sphere(1.0, 2);
        let mut b = Shape::cylinder(0.5, 0.5, 3.0, 10, 0.0).unwrap();
        b.transform(&Affine::rotate_euler_deg([33.0, 12.0, 5.0]));
        CsgTree::Op(CsgOp::Difference, vec![leaf(a), leaf(b)])
    };
    let (s1, _) = run(&mk());
    let (s2, _) = run(&mk());
    assert_eq!(s1, s2);
}

#[test]
fn colours_follow_their_operand() {
    let mut wall = cube([2.0, 2.0, 0.4], [0.0, 1.0, 0.0]);
    wall.set_color([1.0, 1.0, 1.0, 1.0]);
    let mut cut = cube([0.6, 0.6, 1.0], [0.0, 1.0, 0.0]);
    cut.set_color([1.0, 0.0, 0.0, 1.0]);
    let (s, _) = run(&CsgTree::Op(CsgOp::Difference, vec![leaf(wall), leaf(cut)]));
    // The opening's sides come from the cutter: red; the wall faces stay white.
    let red = s.tris.iter().filter(|t| t.iter().all(|&i| s.colors[i as usize] == [1.0, 0.0, 0.0, 1.0])).count();
    let white = s.tris.iter().filter(|t| t.iter().all(|&i| s.colors[i as usize] == [1.0; 4])).count();
    assert!(red >= 8 && white >= 12 && red + white == s.tris.len(), "{red} {white} {}", s.tris.len());
}

#[test]
fn open_operands_and_budgets_are_refused() {
    let plane = Shape::plane([1.0, 1.0], [1, 1]);
    let mut b = Budget::default();
    let e = evaluate(&CsgTree::Op(CsgOp::Union, vec![leaf(plane), leaf(cube([1.0; 3], [0.0; 3]))]), &mut b).unwrap_err();
    assert_eq!(e.kind, CsgErrorKind::Geometry);
    let mut tiny = Budget { max_work: 50, ..Budget::default() };
    let e = evaluate(&CsgTree::Op(CsgOp::Union, vec![leaf(Shape::sphere(1.0, 2)), leaf(cube([1.0; 3], [0.5; 3]))]), &mut tiny).unwrap_err();
    assert_eq!(e.kind, CsgErrorKind::Limit);
    assert!(e.measured.is_some() && e.limit == Some(50.0));
    // Disjoint intersection: empty, refused with a reason.
    let e = evaluate(&CsgTree::Op(CsgOp::Intersection, vec![leaf(cube([1.0; 3], [0.0; 3])), leaf(cube([1.0; 3], [5.0, 0.0, 0.0]))]), &mut Budget::default()).unwrap_err();
    assert_eq!(e.kind, CsgErrorKind::Geometry);
}

fn random_solid(rng: &mut specula_core::rng::Rng) -> Shape {
    let mut s = match rng.below(4) {
        0 => Shape::cuboid([0.4 + rng.f32(), 0.4 + rng.f32(), 0.4 + rng.f32()], 0.0).unwrap(),
        1 => Shape::sphere(0.3 + rng.f32() * 0.6, 1 + rng.below(2)),
        2 => Shape::cylinder(0.2 + rng.f32() * 0.5, 0.2 + rng.f32() * 0.5, 0.5 + rng.f32(), 5 + rng.below(12), 0.0).unwrap(),
        _ => Shape::torus(0.6, 0.15 + rng.f32() * 0.2, 8 + rng.below(8), 6),
    };
    s.transform(&Affine::rotate_euler_deg([(rng.f32() * 360.0) as f64, (rng.f32() * 360.0) as f64, (rng.f32() * 360.0) as f64]));
    s.transform(&Affine::translate([rng.f32() - 0.5, rng.f32() - 0.5, rng.f32() - 0.5]));
    s
}

#[test]
#[allow(clippy::disallowed_methods, reason = "a test or benchmark harness (timings, cameras for pictures), not simulation code")]
fn random_solids_stay_closed_and_validate() {
    let mut rng = specula_core::rng::Rng::new(91, 5);
    let start = std::time::Instant::now();
    let mut ops = 0;
    for case in 0..40 {
        let (a, b, c) = (random_solid(&mut rng), random_solid(&mut rng), random_solid(&mut rng));
        let op = [CsgOp::Union, CsgOp::Difference, CsgOp::Intersection][case % 3];
        let t = CsgTree::Op(op, vec![leaf(a), CsgTree::Op(CsgOp::Union, vec![leaf(b), leaf(c)])]);
        let mut budget = Budget::default();
        match evaluate(&t, &mut budget) {
            Ok((s, _, _)) => {
                assert_eq!(unbalanced_edges(&s), 0, "case {case}");
                let m = s.finalize(30.0, "m");
                m.validate().unwrap_or_else(|e| panic!("case {case}: {e}"));
                ops += 1;
            }
            // Disjoint operands may intersect to nothing: that is the only refusal allowed.
            Err(e) => assert!(e.kind == CsgErrorKind::Geometry && op == CsgOp::Intersection, "case {case}: {e:?}"),
        }
    }
    eprintln!("{ops} random boolean trees in {:?}", start.elapsed());
}

#[test]
fn pieces_of_one_face_are_merged() {
    let wall = cube([4.0, 3.0, 0.4], [0.0, 1.5, 0.0]);
    let window = cube([1.0, 1.2, 1.0], [0.3, 1.6, 0.0]);
    let (s, st) = run(&CsgTree::Op(CsgOp::Difference, vec![leaf(wall), leaf(window)]));
    // Front and back faces with a hole, four reveal faces, four plain sides.
    assert!(s.tris.len() <= 40, "{} triangles ({st:?})", s.tris.len());
}

#[test]
fn window_rims_get_chamfers_and_rounds() {
    let wall = cube([3.0, 3.0, 0.4], [0.0, 1.5, 0.0]);
    let window = cube([1.0, 1.2, 1.0], [0.0, 1.6, 0.0]);
    let t = CsgTree::Op(CsgOp::Difference, vec![leaf(wall.clone()), leaf(window.clone())]);
    let plain = run(&t).0.signed_volume();
    for segments in [1, 3] {
        let mut b = Budget::default();
        let (s, st, bs) = evaluate_bevelled(&t, Some(&Bevel { width: 0.05, segments }), &mut b).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(unbalanced_edges(&s), 0);
        s.finalize(30.0, "m").validate().unwrap();
        // Two rims of four runs each, all convex.
        assert_eq!((bs.convex, bs.concave), (8, 0), "{bs:?}");
        // Chamfers take 8 runs x 0.05^2 / 2 x their length; rounds take less.
        let taken = plain - s.signed_volume();
        let chamfer = 0.05 * 0.05 * 0.5 * 2.0 * (2.0 * 1.0 + 2.0 * 1.2);
        if segments == 1 {
            assert!((taken - chamfer).abs() < chamfer * 0.2, "chamfer took {taken}, expected {chamfer}");
        } else {
            assert!(taken > 0.0 && taken < chamfer, "round took {taken}");
        }
        // The bevel faces are tagged.
        assert!(s.tags.contains(&crate::shape::TAG_BEVEL));
        assert!(st.output_triangles > 32);
    }
    // A buttress unioned on: its join is concave and gets filled.
    let post = cube([0.4, 2.0, 0.4], [0.0, 1.0, 0.4]);
    let u = CsgTree::Op(CsgOp::Union, vec![leaf(cube([3.0, 2.0, 0.4], [0.0, 1.0, 0.0])), leaf(post)]);
    let mut b = Budget::default();
    let (s, _, bs) = evaluate_bevelled(&u, Some(&Bevel { width: 0.04, segments: 1 }), &mut b).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(bs.concave >= 2, "{bs:?}");
    assert_eq!(unbalanced_edges(&s), 0);
}

#[test]
fn ear_clip_handles_collinear_sides() {
    let pts: Vec<[f64; 2]> = vec![[-175828.0, 159844.0], [-191812.99999999997, 159843.99999999997], [-191813.0, 95903.28568236661], [-191813.00000000003, 89512.8], [-191813.0, 70331.0], [-175828.0, 70331.0]];
    let ring: Vec<u32> = (0..pts.len() as u32).collect();
    let p2 = |i: u32| pts[i as usize];
    let mut out = Vec::new();
    assert!(ear_clip(&ring, &p2, &mut out).is_some(), "{out:?}");
    assert_eq!(out.len(), 4);
}

/// Sepulcra's ringed cross and broken obelisk (headstone_carved variants 2 and 3).
fn headstone_trees() -> Vec<(&'static str, CsgTree, f32)> {
    let post = Shape::cuboid([0.16, 1.05, 0.13], 0.0).unwrap();
    let mut arm = Shape::cuboid([0.56, 0.15, 0.13], 0.0).unwrap();
    arm.transform(&Affine::translate([0.0, 0.66, 0.0]));
    let mut ring = Shape::torus(0.19, 0.035, 16, 4);
    ring.transform(&Affine::translate([0.0, -0.035, 0.0]));
    ring.transform(&Affine::rotate_euler_deg([90.0, 0.0, 0.0]));
    ring.transform(&Affine::translate([0.0, 0.735, 0.0]));
    let cross = CsgTree::Op(CsgOp::Union, vec![leaf(post), leaf(arm), leaf(ring)]);
    let ob = Shape::extrude(&crate::poly::rect(0.3, 0.3), 1.15, 0.7, 0.02, 0.0, 1, None).unwrap();
    let mut top = Shape::cuboid([0.8, 0.5, 0.8], 0.0).unwrap();
    top.transform(&Affine::rotate_euler_deg([18.0, 0.0, 24.0]));
    top.transform(&Affine::translate([0.0, 0.95, 0.0]));
    let mut chip = Shape::cuboid([0.12, 0.12, 0.2], 0.0).unwrap();
    chip.transform(&Affine::rotate_euler_deg([0.0, 45.0, 30.0]));
    chip.transform(&Affine::translate([0.15, 0.45, 0.12]));
    let obelisk = CsgTree::Op(CsgOp::Difference, vec![leaf(ob), leaf(top), leaf(chip)]);
    vec![("cross", cross, 0.015), ("obelisk", obelisk, 0.012)]
}

#[test]
fn one_bad_tube_leaves_only_its_edges_sharp() {
    // Each of these has one bevel loop whose boolean fails on its own; the rest still bevel.
    for (name, t, w) in headstone_trees() {
        let mut b = Budget::default();
        let (s, _, bs) = evaluate_bevelled(&t, Some(&Bevel { width: w, segments: 1 }), &mut b).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert!(bs.failed.is_none(), "{name}: {bs:?}");
        assert!(bs.skipped >= 1 && bs.skipped < bs.chains, "{name}: {bs:?}");
        assert_eq!(unbalanced_edges(&s), 0, "{name}");
        assert!(s.tags.contains(&crate::shape::TAG_BEVEL), "{name}");
        s.finalize(30.0, "m").validate().unwrap();
    }
}

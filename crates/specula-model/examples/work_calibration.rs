//! Checks the work-unit weights ([`specula_model::work`]) against the clock: each tier's worst
//! cases run until they finish or hit a budget, and the ratio of wall nanoseconds to units
//! charged is printed. A ratio near or below 1 means the caps mean what they say (a capped
//! build stops within about [`specula_model::work::MAX_BUILD`] ns); well above 1 means a weight
//! is too low. Run: `cargo run --release -p specula-model --example work_calibration`.

#![allow(clippy::disallowed_methods, reason = "a test or benchmark harness (timings, cameras for pictures), not simulation code")]

use std::time::Instant;

use specula_model::csg::{evaluate, Budget, CsgOp, CsgTree};
use specula_model::math3::Affine;
use specula_model::scatter::{flat, scatter, Area, ScatterSpec};
use specula_model::sdf::{mesh, Combine, Kind, MeshOptions, Modifiers, Prim, SdfNode};
use specula_model::terrain::{Hydraulic, NoiseKind, NoiseLayer, TerrainSpec, Thermal};
use specula_model::texgraph::{bake, TexGraph};
use specula_model::work::MAX_BUILD;
use specula_model::{Shape, Style};

fn report(name: &str, t0: Instant, work: u64, outcome: &str) {
    let ns = t0.elapsed().as_nanos() as f64;
    println!("{name:<52} {:>8.0} ms  {:>13} units  ratio {:>5.2}  {outcome}", ns / 1e6, work, ns / work.max(1) as f64);
}

fn csg(name: &str, t: CsgTree) {
    let t0 = Instant::now();
    let mut b = Budget::default();
    let r = evaluate(&t, &mut b);
    report(name, t0, b.work, &format!("exact {} {}", b.exact, if r.is_ok() { "ok" } else { "limit" }));
}

fn cyl(r: f32, h: f32, sides: u32) -> Shape {
    Shape::cylinder(r, r, h, sides, 0.0).unwrap()
}

fn main() {
    println!("cap: {MAX_BUILD} units");
    // CSG: all-coplanar operands (exact predicates everywhere), many near-coplanar ones, a
    // curved operand cut by many.
    csg("csg union of 64 identical cylinders(64)", CsgTree::Op(CsgOp::Union, (0..64).map(|_| CsgTree::Leaf(cyl(0.5, 1.0, 64))).collect()));
    csg(
        "csg union of 48 rotated cylinders(96)",
        CsgTree::Op(
            CsgOp::Union,
            (0..48)
                .map(|i| {
                    let mut c = cyl(0.5, 1.0, 96);
                    c.transform(&Affine::rotate_euler_deg([0.0, i as f64 * 0.37, 0.0]));
                    CsgTree::Leaf(c)
                })
                .collect(),
        ),
    );
    let mut kids = vec![CsgTree::Leaf(Shape::sphere(1.0, 4))];
    for i in 0..12 {
        let mut c = cyl(0.2, 3.0, 48);
        c.transform(&Affine::translate([0.0, -1.5, 0.0]));
        c.transform(&Affine::rotate_euler_deg([i as f64 * 15.0, i as f64 * 29.0, 0.0]));
        c.transform(&Affine::translate([0.0, 1.0, 0.0]));
        kids.push(CsgTree::Leaf(c));
    }
    csg("csg sphere(4) minus 12 cylinders(48)", CsgTree::Op(CsgOp::Difference, kids));
    let mut wall = Shape::cuboid([6.0, 3.0, 0.4], 0.0).unwrap();
    wall.transform(&Affine::translate([0.0, 0.0, 0.0]));
    let mut kids = vec![CsgTree::Leaf(wall)];
    for i in 0..40 {
        let mut w = Shape::cuboid([0.1, 0.4, 1.0], 0.0).unwrap();
        w.transform(&Affine::translate([-2.9 + i as f32 * 0.145, 0.5 + (i % 5) as f32 * 0.45, -0.5]));
        kids.push(CsgTree::Leaf(w));
    }
    csg("csg wall minus 40 slots", CsgTree::Op(CsgOp::Difference, kids));

    // SDF: noise-heavy trees, a twisted repeat.
    let disp = |octaves| {
        let kids: Vec<SdfNode> = (0..127)
            .map(|i| {
                let mut m = Modifiers { displace: Some((2.0, 0.05, octaves)), ..Modifiers::default() };
                m.at = [((i * 37) % 100) as f32 * 0.04 - 2.0, ((i * 53) % 100) as f32 * 0.04 - 2.0, ((i * 71) % 100) as f32 * 0.04 - 2.0];
                SdfNode { kind: Kind::Prim(Prim::Sphere { radius: 0.6 }), m }
            })
            .collect();
        SdfNode { kind: Kind::Group { op: Combine::Union, kids, smooth: 0.0 }, m: Modifiers::default() }
    };
    for (name, tree) in [("sdf 127 displaced spheres x6, res 160", disp(6)), ("sdf 127 displaced spheres x1, res 160", disp(1))] {
        let opts = MeshOptions { resolution: 160, bounds: Some(([-3.0, -3.0, -2.8], [3.0, 3.0, 2.8])), seed: 1, max_triangles: 2_000_000, max_work: MAX_BUILD, color: [1.0; 4] };
        let t0 = Instant::now();
        match mesh(&tree, &opts) {
            Ok((_, st)) => report(name, t0, st.work, "ok"),
            Err(e) => report(name, t0, e.measured.unwrap_or(0.0) as u64, "limit"),
        }
    }
    let twisted = SdfNode {
        kind: Kind::Prim(Prim::Box { half: [0.3, 1.0, 0.3], round: 0.02 }),
        m: Modifiers { repeat: Some(([4, 1, 4], [1.0, 1.0, 1.0])), twist: 40.0, displace: Some((0.02, 0.2, 3)), ..Modifiers::default() },
    };
    let opts = MeshOptions { resolution: 160, bounds: None, seed: 1, max_triangles: 2_000_000, max_work: MAX_BUILD, color: [1.0; 4] };
    let t0 = Instant::now();
    match mesh(&twisted, &opts) {
        Ok((_, st)) => report("sdf twisted repeated boxes, res 160", t0, st.work, "ok"),
        Err(e) => report("sdf twisted repeated boxes, res 160", t0, e.measured.unwrap_or(0.0) as u64, "limit"),
    }

    // Terrain: noise only, erosion only, thermal only (each under the cap).
    let base_t = TerrainSpec {
        size: [200.0, 200.0],
        samples: [257, 257],
        base: 0.0,
        seed: 3,
        layers: vec![NoiseLayer { kind: NoiseKind::Fbm, scale: 40.0, octaves: 8, amplitude: 10.0 }, NoiseLayer { kind: NoiseKind::Ridged, scale: 10.0, octaves: 8, amplitude: 2.0 }],
        warp: 4.0,
        island: 0.0,
        terrace: (0.0, 0.0),
        pads: vec![],
        hydraulic: None,
        thermal: None,
        splat: vec![],
    };
    for (name, spec) in [
        ("terrain 257², 2x8 octaves and warp", base_t.clone()),
        ("terrain 257², 1 octave, 60k droplets x 64", TerrainSpec { layers: vec![NoiseLayer { kind: NoiseKind::Fbm, scale: 40.0, octaves: 1, amplitude: 10.0 }], warp: 0.0, hydraulic: Some(Hydraulic { droplets: 60_000, strength: 0.5, steps: 64 }), ..base_t.clone() }),
        ("terrain 257², 1 octave, thermal 150", TerrainSpec { layers: vec![NoiseLayer { kind: NoiseKind::Fbm, scale: 40.0, octaves: 1, amplitude: 10.0 }], warp: 0.0, thermal: Some(Thermal { iterations: 150, talus_deg: 30.0 }), ..base_t.clone() }),
    ] {
        let t0 = Instant::now();
        match spec.build(MAX_BUILD) {
            Ok(t) => report(name, t0, t.work, "ok"),
            Err(e) => report(name, t0, e.measured.unwrap_or(0.0) as u64, "limit"),
        }
    }

    // Scatter: a 1,024-point comb, and a big rectangle, both filling up.
    let n = 512;
    let mut pts = Vec::new();
    for i in 0..n {
        let x0 = -100.0 + 200.0 * i as f32 / n as f32;
        pts.push([x0, 100.0]);
        pts.push([x0 + 200.0 / n as f32 * 0.2, -100.0]);
    }
    let base = ScatterSpec {
        area: Area::Polygon(pts),
        offset: [0.0, 0.0],
        min_distance: 0.4,
        max_count: 4096,
        seed: 1,
        attempts: 64,
        slope: None,
        height: None,
        mask: None,
        splat: None,
        avoid: vec![],
        weights: vec![],
        scale: [1.0, 1.0],
        yaw: [0.0, 360.0],
        tilt: 0.0,
        align: 0.0,
        sink: 0.0,
    };
    for (name, s) in [
        ("scatter comb polygon 1024 pts, count 4096", base.clone()),
        ("scatter comb polygon 1024 pts, count 1", ScatterSpec { max_count: 1, ..base.clone() }),
        ("scatter rect 150 m at 1 m, count 4096, 256 avoid", ScatterSpec { area: Area::Rect([150.0, 150.0]), min_distance: 1.0, avoid: vec![[500.0, 0.0, 1.0]; 256], ..base.clone() }),
    ] {
        let t0 = Instant::now();
        match scatter(&s, &flat, MAX_BUILD) {
            Ok((p, st)) => report(name, t0, st.work, &format!("{} placed", p.len())),
            Err(e) => report(name, t0, 0, &format!("err {}", e.message)),
        }
    }

    // Tier 1 on a 180² plane (32,761 vertices): ns per vertex of each op, simplify and colliders.
    let plane = Shape::plane([10.0, 10.0], [180, 180]);
    let per_vertex = |name: &str, f: &dyn Fn(&mut Shape)| {
        let mut s = plane.clone();
        let n = s.positions.len();
        let t0 = Instant::now();
        f(&mut s);
        println!("tier1 {name:<36} {:>8.1} ns per vertex", t0.elapsed().as_nanos() as f64 / n as f64);
    };
    let rng = || specula_core::rng::Rng::new(1, 1);
    per_vertex("displace x1", &|s| s.displace(0.01, 0.3, 1, 1, [1.0; 3]));
    per_vertex("displace x6", &|s| s.displace(0.01, 0.3, 6, 1, [1.0; 3]));
    per_vertex("smooth normals", &|s| drop(s.smooth_normals()));
    per_vertex("twist", &|s| s.twist(10.0));
    per_vertex("bend", &|s| s.bend(10.0));
    per_vertex("weld", &|s| s.weld(1e-4));
    per_vertex("ao (concavity shade)", &|s| s.concavity_shade(0.3));
    per_vertex("noise tint", &|s| s.noise_tint(0.1, 0.5, 1));
    per_vertex("subdivide 1 (per input vertex)", &|s| s.subdivide(1));
    per_vertex("array x4 (per input vertex)", &|s| s.array_linear(4, [11.0, 0.0, 0.0], &mut rng(), &specula_model::shape::Jitter::default()));
    per_vertex("finalize", &|s| drop(s.finalize(30.0, "m")));
    let mesh = plane.finalize(30.0, "m");
    for kind in [specula_model::ColliderKind::ConvexHull, specula_model::ColliderKind::Capsule] {
        let t0 = Instant::now();
        let _ = specula_model::collider::fit(kind, &mesh.positions);
        println!("tier1 collider {kind:?} {:>30.1} ns per vertex", t0.elapsed().as_nanos() as f64 / mesh.positions.len() as f64);
    }
    for ratio in [0.5f32, 0.06] {
        let t0 = Instant::now();
        let _ = specula_model::lod::simplify(&[specula_model::lod::LodInput { shape: &plane, cell_scale: 1.0 }], ratio, [-5.0, 0.0, -5.0], 10.0);
        println!("tier1 simplify to {ratio:<26} {:>8.1} ns per input triangle", t0.elapsed().as_nanos() as f64 / plane.triangle_count() as f64);
    }

    // Textures on one thread (as charged: low-end devices may have one core).
    let style = Style::parse(r##"{ "palette": { "a": "#4f6b3a" }, "materials": { "m": { "color": "#8c96a3" } } }"##).unwrap();
    let graphs = [
        ("tex worley x5 + edge x4 at 256", r#"{ "size": 256, "nodes": { "a": { "op": "noise", "kind": "worley", "octaves": 5, "scale": 64 }, "b": { "op": "noise", "kind": "worley_edge", "octaves": 4, "scale": 64 }, "c": { "op": "blend", "a": "a", "b": "b" } }, "outputs": { "base_color": "c", "height": "a" } }"#.to_string()),
        ("tex ao radius 12 at 256", r#"{ "size": 256, "nodes": { "a": { "op": "noise", "kind": "fbm", "octaves": 4 }, "b": { "op": "ao", "in": "a", "radius": 12 } }, "outputs": { "base_color": "b" } }"#.to_string()),
        ("tex 24 warps at 256", {
            let mut nodes = String::from(r#""n0": { "op": "noise", "kind": "value" }"#);
            for i in 1..24 {
                nodes.push_str(&format!(r#", "n{i}": {{ "op": "warp", "in": "n{}", "by": "n{}", "amount": 0.3 }}"#, i - 1, i - 1));
            }
            format!(r#"{{ "size": 256, "nodes": {{ {nodes} }}, "outputs": {{ "base_color": "n23" }} }}"#)
        }),
        ("tex blur 32 x 4 at 256", r#"{ "size": 256, "nodes": { "a": { "op": "noise", "kind": "fbm", "octaves": 4 }, "b": { "op": "blur", "in": "a", "radius": 32 }, "c": { "op": "blur", "in": "b", "radius": 32 }, "d": { "op": "blur", "in": "c", "radius": 32 } }, "outputs": { "base_color": "d", "height": "d" } }"#.to_string()),
    ];
    // Each texture op on one thread at 256 px: ns per pixel against the units its cost charges.
    let node = |n: &str| format!(r#"{{ "size": 256, "nodes": {{ "a": {{ "op": "noise", "kind": "value", "octaves": 1 }}, "b": {{ "op": "noise", "kind": "value", "octaves": 1, "seed": 2 }}, "x": {n} }}, "outputs": {{ "base_color": "x" }} }}"#);
    let base = TexGraph::parse(&node(r#"{ "op": "constant", "value": 0.5 }"#), &style).unwrap();
    let t0 = Instant::now();
    bake(&base, 1).unwrap();
    let base_ns = t0.elapsed().as_nanos() as f64;
    for (name, n) in [
        ("noise value x1", r#"{ "op": "noise", "kind": "value", "octaves": 1 }"#),
        ("noise fbm x4", r#"{ "op": "noise", "kind": "fbm", "octaves": 4 }"#),
        ("noise worley x1", r#"{ "op": "noise", "kind": "worley", "octaves": 1 }"#),
        ("bricks", r#"{ "op": "bricks" }"#),
        ("blend", r#"{ "op": "blend", "a": "a", "b": "b" }"#),
        ("curve", r#"{ "op": "curve", "in": "a", "points": [[0, 0], [0.5, 0.8], [1, 1]] }"#),
        ("levels", r#"{ "op": "levels", "in": "a" }"#),
        ("warp", r#"{ "op": "warp", "in": "a", "by": "b" }"#),
        ("blur r4", r#"{ "op": "blur", "in": "a", "radius": 4 }"#),
        ("normal", r#"{ "op": "normal", "in": "a" }"#),
        ("ao r4", r#"{ "op": "ao", "in": "a", "radius": 4 }"#),
        ("curvature", r#"{ "op": "curvature", "in": "a" }"#),
        ("gradient_map", r#"{ "op": "gradient_map", "in": "a", "stops": [[0, "a"], [1, "a"]] }"#),
        ("gradient", r#"{ "op": "gradient" }"#),
        ("invert", r#"{ "op": "invert", "in": "a" }"#),
    ] {
        let g = match TexGraph::parse(&node(n), &style) {
            Ok(g) => g,
            Err(e) => {
                println!("tex op {name}: {e}");
                continue;
            }
        };
        let t0 = Instant::now();
        bake(&g, 1).unwrap();
        let px = 256.0 * 256.0;
        let ns = (t0.elapsed().as_nanos() as f64 - base_ns).max(0.0) / px;
        let charged = (g.cost() - base.cost()) as f64 / px * specula_model::work::TEXTURE_OP as f64;
        println!("tex op {name:<20} {ns:>7.1} ns/px on 1 thread, charged {charged:>6.0} units/px");
    }
    for (name, text) in graphs {
        let g = match TexGraph::parse(&text, &style) {
            Ok(g) => g,
            Err(e) => {
                println!("{name}: {e}");
                continue;
            }
        };
        let work = g.cost() * specula_model::work::TEXTURE_OP;
        let t0 = Instant::now();
        let r = bake(&g, 1);
        report(name, t0, work, if r.is_ok() { "ok" } else { "refused" });
    }
}

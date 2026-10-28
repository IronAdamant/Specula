//! Build times of the procedural modelling tiers 2-4 (booleans, SDF, generators, texture
//! graphs) on this machine: `cargo run --release -p specula-model --example procgen_bench`.
//! Prints one line per case (best of a few runs, milliseconds); docs/design/PROCEDURAL_MODELLING.md
//! keeps the results.

#![allow(clippy::disallowed_methods, reason = "a test or benchmark harness (timings, cameras for pictures), not simulation code")]

use std::time::Instant;

use specula_model::csg::{evaluate, evaluate_bevelled, Bevel, Budget, CsgOp, CsgTree};
use specula_model::math3::Affine;
use specula_model::scatter::{flat, scatter, Area, ScatterSpec};
use specula_model::sdf::{mesh, Combine, Kind, MeshOptions, Modifiers, Prim, SdfNode};
use specula_model::terrain::{Hydraulic, NoiseKind, NoiseLayer, TerrainSpec, Thermal};
use specula_model::texgraph::{bake, TexGraph};
use specula_model::{build, BuildParams, Shape, Style};

fn time<T>(name: &str, runs: u32, mut f: impl FnMut() -> T, describe: impl Fn(&T) -> String) {
    let mut best = f64::MAX;
    let mut last = None;
    for _ in 0..runs {
        let t = Instant::now();
        let out = f();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        last = Some(out);
    }
    println!("{name:<40} {best:>9.2} ms  {}", describe(last.as_ref().expect("ran")));
}

fn cube(size: [f32; 3], at: [f32; 3]) -> Shape {
    let mut s = Shape::cuboid(size, 0.0).unwrap();
    s.transform(&Affine::translate([at[0], at[1] - size[1] * 0.5, at[2]]));
    s
}

fn main() {
    let csg = |t: &CsgTree| {
        let mut b = Budget::default();
        evaluate(t, &mut b).map(|(s, _, st)| (s.tris.len(), st))
    };
    let show = |r: &Result<(usize, specula_model::csg::CsgStats), specula_model::csg::CsgError>| match r {
        Ok((n, st)) => format!("{n} tris (in {}, work {}, polys {}, t-junctions {})", st.input_triangles, st.work, st.polygons, st.t_junctions),
        Err(e) => format!("error: {e}"),
    };
    // A wall with 12 windows and a door.
    let wall = {
        let mut kids = vec![CsgTree::Leaf(cube([12.0, 6.0, 0.5], [0.0, 3.0, 0.0]))];
        for f in 0..2 {
            for i in 0..6 {
                kids.push(CsgTree::Leaf(cube([0.9, 1.4, 1.0], [-5.0 + 2.0 * i as f32, 1.6 + 3.0 * f as f32, 0.0])));
            }
        }
        CsgTree::Op(CsgOp::Difference, kids)
    };
    time("csg: wall minus 12 windows", 5, || csg(&wall), show);
    // A sphere minus three cylinders (curved operands, many splits).
    let ball = {
        let mut kids = vec![CsgTree::Leaf(Shape::sphere(1.0, 3))];
        for (i, r) in [[90.0, 0.0, 0.0], [0.0, 0.0, 90.0], [0.0, 0.0, 0.0]].iter().enumerate() {
            let mut c = Shape::cylinder(0.45, 0.45, 3.0, 24, 0.0).unwrap();
            c.transform(&Affine::translate([0.0, -1.5, 0.0]));
            c.transform(&Affine::rotate_euler_deg(*r));
            c.transform(&Affine::translate([0.0, 1.0, 0.0]));
            let _ = i;
            kids.push(CsgTree::Leaf(c));
        }
        CsgTree::Op(CsgOp::Difference, kids)
    };
    time("csg: sphere(1280) minus 3 cylinders(24)", 3, || csg(&ball), show);
    let bevelled = |t: &CsgTree| {
        let mut b = Budget::default();
        evaluate_bevelled(t, Some(&Bevel { width: 0.03, segments: 1 }), &mut b).map(|(s, st, bs)| (s.tris.len(), st, bs))
    };
    time("csg: wall minus 12 windows, chamfered", 5, || bevelled(&wall), |r| match r {
        Ok((n, st, bs)) => format!("{n} tris (work {}, {} runs bevelled)", st.work, bs.convex + bs.concave),
        Err(e) => format!("error: {e}"),
    });

    // SDF: a smooth-blended, displaced rock at three resolutions.
    let rock = SdfNode {
        kind: Kind::Group {
            op: Combine::Union,
            smooth: 0.12,
            kids: vec![
                SdfNode { kind: Kind::Prim(Prim::Ellipsoid { radii: [0.6, 0.35, 0.45] }), m: Modifiers::default() },
                SdfNode { kind: Kind::Prim(Prim::Sphere { radius: 0.3 }), m: Modifiers { at: [0.35, 0.15, 0.1], ..Modifiers::default() } },
                SdfNode { kind: Kind::Prim(Prim::Box { half: [0.2, 0.2, 0.2], round: 0.03 }), m: Modifiers { at: [-0.4, 0.0, 0.0], rotate: [0.0, 30.0, 10.0], ..Modifiers::default() } },
            ],
        },
        m: Modifiers { displace: Some((0.03, 0.15, 3)), ..Modifiers::default() },
    };
    for res in [24u32, 48, 96] {
        let opts = MeshOptions { resolution: res, bounds: None, seed: 1, max_triangles: 1 << 20, max_work: specula_model::sdf::MAX_WORK, color: [1.0; 4] };
        time(&format!("sdf: displaced rock, resolution {res}"), 3, || mesh(&rock, &opts), |r| match r {
            Ok((s, st)) => format!("{} tris ({} samples evaluated, {} blocks skipped)", s.tris.len(), st.samples_evaluated, st.blocks_skipped),
            Err(e) => format!("error: {e:?}"),
        });
    }

    // Terrain: 129 x 129 with 20k droplets of erosion.
    let spec = TerrainSpec {
        size: [64.0, 64.0],
        samples: [129, 129],
        base: 0.0,
        seed: 3,
        layers: vec![NoiseLayer { kind: NoiseKind::Fbm, scale: 20.0, octaves: 5, amplitude: 4.0 }],
        warp: 3.0,
        island: 0.0,
        terrace: (0.0, 0.0),
        pads: vec![],
        hydraulic: Some(Hydraulic { droplets: 20_000, strength: 0.5, steps: 48 }),
        thermal: Some(Thermal { iterations: 10, talus_deg: 35.0 }),
        splat: vec![],
    };
    time("terrain: 129x129, 20k droplets, 10 thermal", 3, || spec.build(specula_model::terrain::MAX_WORK), |r| match r {
        Ok(t) => format!("{} samples, work {}", t.heights.len(), t.work),
        Err(e) => format!("error: {e:?}"),
    });

    // Scatter: Poisson-disc over 100 x 100 m at 1.5 m.
    let sc = ScatterSpec {
        area: Area::Rect([100.0, 100.0]),
        offset: [0.0, 0.0],
        min_distance: 1.5,
        max_count: 4096,
        seed: 1,
        attempts: 30,
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
    time("scatter: 100 x 100 m, 1.5 m apart", 5, || scatter(&sc, &flat, specula_model::scatter::MAX_WORK), |r| match r {
        Ok((p, st)) => format!("{} placed of {} candidates", p.len(), st.candidates),
        Err(e) => format!("error: {e:?}"),
    });

    // Texture graph: the Sepulcra crypt stone at 256 and 512 px.
    let style = Style::parse(&std::fs::read_to_string("games/sepulcra/assets/style.json").expect("run from the repo root")).unwrap();
    let text = std::fs::read_to_string("games/sepulcra/assets/recipes/tex_crypt_stone.json").unwrap();
    let mut g = TexGraph::parse(&text, &style).unwrap();
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4);
    for size in [256u32, 512] {
        g.size = size;
        time(&format!("texgraph: crypt stone {size} px, {threads} threads"), 3, || bake(&g, threads), |r| match r {
            Ok(t) => format!("{} GPU bytes, work {}", t.gpu_bytes(), t.work),
            Err(e) => format!("error: {e}"),
        });
        time(&format!("texgraph: crypt stone {size} px, 1 thread"), 2, || bake(&g, 1), |_| String::new());
    }

    // Whole Sepulcra procgen recipes through the recipe layer.
    let mut lib = std::collections::BTreeMap::new();
    for e in std::fs::read_dir("games/sepulcra/assets/recipes").unwrap() {
        let p = e.unwrap().path();
        lib.insert(p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&p).unwrap());
    }
    for (name, vals) in [("headstone_carved", vec![("variant", 2.0)]), ("crypt", vec![]), ("ruined_wall", vec![]), ("grave_rock", vec![]), ("graveyard_ground", vec![]), ("graveyard_plot", vec![])] {
        time(&format!("recipe: {name}"), 3, || build(&lib[name], &BuildParams { values: &vals, library: Some(&lib), ..BuildParams::new(&style) }), |r| match r {
            Ok(m) => format!("LODs {:?} triangles", m.lods.iter().map(|l| l.triangle_count()).collect::<Vec<_>>()),
            Err(e) => format!("error: {e}"),
        });
    }
}

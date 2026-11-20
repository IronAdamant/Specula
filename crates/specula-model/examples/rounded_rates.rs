//! What a build of rounded shapes (`roundness`) takes for the work it is charged, by time and by
//! count, against a plain box cut from a plain box timed in the same run:
//! `cargo run --release -p specula-model --example rounded_rates [runs]`.
//!
//! For each case: the median over `runs` (default 5) of its nanoseconds a unit of work over
//! the plain cut's, each run timed right after a plain cut; and the powers its rounded samples
//! took for every thousand units charged (a count: the same on every machine). Run it on a
//! quiet machine: a time taken while other builds run says nothing of a rate. The cases are
//! the calibrated one (a cube of roundness 0.3 cut from a 2 m box) and the dearest the
//! weights' second mending found by count, with the eighth and ninth reviews' own cases (a
//! plank cut where its samples lie just past its thin face, beside the rim; a cube under 40
//! and 120 nested unions).

#![allow(clippy::disallowed_methods, reason = "a benchmark harness (timings), not simulation code")]

use std::time::Instant;

use specula_model::sdf::{mesh, Combine, Kind, MeshOptions, Modifiers, Prim, SdfNode};

fn node(p: Prim, at: [f32; 3]) -> SdfNode {
    SdfNode { kind: Kind::Prim(p), m: Modifiers { at, ..Modifiers::default() } }
}

fn cut(cutter: SdfNode) -> SdfNode {
    SdfNode { kind: Kind::Group { op: Combine::Difference, kids: vec![node(Prim::Box { half: [1.0; 3], round: 0.0 }, [0.0; 3]), cutter], smooth: 0.0 }, m: Modifiers::default() }
}

fn rounded(half: [f32; 3], power: f32, disc: bool, at: [f32; 3]) -> SdfNode {
    cut(node(Prim::Rounded { half, power, disc }, at))
}

/// `leaf` under `depth` plain unions of one child each.
fn chain(leaf: SdfNode, depth: usize) -> SdfNode {
    (0..depth).fold(leaf, |t, _| SdfNode { kind: Kind::Group { op: Combine::Union, kids: vec![t], smooth: 0.0 }, m: Modifiers::default() })
}

/// Seconds, work and the rounded samples' powers of one build at resolution 96.
fn build(tree: &SdfNode) -> (f64, u64, u64) {
    let options = MeshOptions { resolution: 96, bounds: None, seed: 1, max_triangles: 4_000_000, max_work: u64::MAX, color: [1.0; 4] };
    let t = Instant::now();
    let (_, stats) = mesh(tree, &options).expect("builds");
    (t.elapsed().as_secs_f64(), stats.work, stats.rounded_powers)
}

fn main() {
    let runs: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(5).max(1);
    let plain = cut(node(Prim::Box { half: [0.3; 3], round: 0.0 }, [0.0; 3]));
    let cases = [
        ("calibrated: cube, roundness 0.3", rounded([0.3; 3], 6.67, false, [0.0; 3])),
        ("2 m sheet 1:1e6, roundness 0.67, through", rounded([1.0, 1e-6, 1.0], 3.0, false, [0.0; 3])),
        ("2 m sheet 1:1e6, roundness 0.4, through", rounded([1.0, 1e-6, 1.0], 5.0, false, [0.0; 3])),
        ("2 m board 1:20, roundness 0.67, through", rounded([1.0, 0.05, 1.0], 3.0, false, [0.0; 3])),
        ("review: 100 m sheet past the corner", rounded([50.0, 5e-5, 50.0], 10.0, false, [-51.1, 0.0, -51.1])),
        ("review: 10 m sheet past the corner", rounded([5.0, 5e-4, 5.0], 8.0, false, [-6.1, 0.0, -6.1])),
        ("review: foil 1:9000, roundness 0.3", rounded([0.5, 5.55e-5, 0.5], 6.67, false, [0.0; 3])),
        ("review: foil 1:9000, roundness 0.25", rounded([0.5, 5.55e-5, 0.5], 8.0, false, [0.0; 3])),
        ("review: hair 1:9000, roundness 0.05", rounded([5.55e-5, 0.5, 5.55e-5], 40.0, false, [0.0; 3])),
        ("review: foil 1:20, roundness 0.99", rounded([0.5, 0.025, 0.5], 2.0202, false, [0.0; 3])),
        ("100 m disc past the corner", rounded([50.0, 5e-5, 0.0], 10.0, true, [-36.8, 0.0, -36.8])),
        ("ninth review: plank 1:1e5 past its face", rounded([1.0, 0.5, 1e-5], 6.67, false, [0.0, 0.0, -0.98])),
        ("ninth review: cube under 40 unions", cut(chain(node(Prim::Rounded { half: [0.3; 3], power: 6.67, disc: false }, [0.0; 3]), 40))),
        ("ninth review: cube under 120 unions", cut(chain(node(Prim::Rounded { half: [0.3; 3], power: 6.67, disc: false }, [0.0; 3]), 120))),
    ];
    println!("{:<44} {:>14} {:>16}", "case", "rate (plain 1)", "powers / 1000 u");
    for (name, tree) in &cases {
        let mut rates = Vec::with_capacity(runs);
        let mut count = (0, 0);
        for _ in 0..runs {
            let (ps, pw, _) = build(&plain);
            let (s, w, powers) = build(tree);
            rates.push((s / w as f64) / (ps / pw as f64));
            count = (w, powers);
        }
        rates.sort_by(f64::total_cmp);
        println!("{name:<44} {:>14.2} {:>16.2}", rates[runs / 2], count.1 as f64 * 1000.0 / count.0 as f64);
    }
}

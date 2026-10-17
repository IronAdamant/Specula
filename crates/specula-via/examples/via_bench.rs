//! Navigation timings against the budgets in docs/design/NAVIGATION.md ("Budgets (F3)"):
//! building a 64 m x 64 m greybox room set, rebuilding one tile, paths across it, and a crowd of
//! 100 agents stepping. Run with `cargo run --release -p specula-via --example via_bench`.
//! Wall-clock numbers (one thread); the machine may be shared, so read them as rough.

use specula_core::math::Vec3;
use specula_core::rng::Rng;
use specula_via::{AgentParams, Config, Crowd, CrowdSettings, NavMesh, Source, Triangle};
use std::time::Instant;

/// Unrotated boxes go the engine's way: the axis-aligned fast path.
fn cuboid(lo: Vec3, hi: Vec3) -> Source {
    Source::aligned_box(lo, hi)
}

/// A box as 12 triangles (for comparison: rotated boxes, hulls and meshes take this path).
fn cuboid_triangles(lo: Vec3, hi: Vec3) -> Source {
    let c = |i: usize| Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z });
    let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    let mut t = Vec::new();
    for f in faces {
        t.push(Triangle([c(f[0]), c(f[1]), c(f[2])]));
        t.push(Triangle([c(f[0]), c(f[2]), c(f[3])]));
    }
    Source::new(t, true)
}

/// A 64 m x 64 m floor cut into an 8 x 8 grid of rooms by 0.3 m walls with 1.6 m doorways, plus
/// crates and a few ramps.
fn rooms() -> Vec<Source> {
    let mut s = vec![cuboid(Vec3::new(-32.0, -1.0, -32.0), Vec3::new(32.0, 0.0, 32.0))];
    for i in 0..=8 {
        let p = -32.0 + i as f32 * 8.0;
        for j in 0..8 {
            let a = -32.0 + j as f32 * 8.0;
            // Walls along x and along z, each 8 m long with a doorway in the middle.
            s.push(cuboid(Vec3::new(a, 0.0, p - 0.15), Vec3::new(a + 3.2, 2.5, p + 0.15)));
            s.push(cuboid(Vec3::new(a + 4.8, 0.0, p - 0.15), Vec3::new(a + 8.0, 2.5, p + 0.15)));
            s.push(cuboid(Vec3::new(p - 0.15, 0.0, a), Vec3::new(p + 0.15, 2.5, a + 3.2)));
            s.push(cuboid(Vec3::new(p - 0.15, 0.0, a + 4.8), Vec3::new(p + 0.15, 2.5, a + 8.0)));
        }
    }
    let mut rng = Rng::new(7, 1);
    for _ in 0..150 {
        let (x, z) = (rng.range_f32(-31.0, 31.0), rng.range_f32(-31.0, 31.0));
        let h = rng.range_f32(0.4, 1.2);
        s.push(cuboid(Vec3::new(x - 0.4, 0.0, z - 0.4), Vec3::new(x + 0.4, h, z + 0.4)));
    }
    s
}

fn build_all(mesh: &mut NavMesh, sources: &[Source]) -> [f64; 5] {
    let (nx, nz) = mesh.tile_count();
    let mut stages = [0.0; 5];
    for tz in 0..nz {
        for tx in 0..nx {
            let (lo, hi) = mesh.tile_input_bounds(tx, tz);
            let mine: Vec<&Source> = sources.iter().filter(|s| s.touches(lo, hi)).collect();
            let s = mesh.build_tile(tx, tz, &mine);
            for (total, us) in stages.iter_mut().zip(s.times_us) {
                *total += us / 1000.0;
            }
        }
    }
    stages
}

fn main() {
    let sources = rooms();
    let cfg = Config { bounds_min: Vec3::new(-33.0, -2.0, -33.0), bounds_max: Vec3::new(33.0, 5.0, 33.0), ..Config::default() };
    let tris: usize = sources.iter().map(|s| s.triangles.len()).sum();
    // Full build (best of 3).
    let mut best = f64::INFINITY;
    let mut mesh = NavMesh::new(cfg).unwrap();
    let mut stages = [0.0; 5];
    for _ in 0..3 {
        let mut m = NavMesh::new(cfg).unwrap();
        let t = Instant::now();
        let s = build_all(&mut m, &sources);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        if ms < best {
            best = ms;
            stages = s;
        }
        mesh = m;
    }
    let (nx, nz) = mesh.tile_count();
    println!("build: 64 m x 64 m, {} sources, {tris} triangles, {nx} x {nz} tiles, {} polygons: {best:.2} ms (target < 50 ms)", sources.len(), mesh.poly_count());
    println!(
        "  stages ms: rasterise {:.2}, open space + links {:.2}, erode + islands {:.2}, rectangles {:.2}, rectangle links {:.2}",
        stages[0], stages[1], stages[2], stages[3], stages[4]
    );
    // The same scene as triangles (what rotated boxes, hulls and meshes cost).
    let tri_sources: Vec<Source> = sources.iter().map(|s| cuboid_triangles(s.lo, s.hi)).collect();
    let mut m = NavMesh::new(cfg).unwrap();
    let t = Instant::now();
    build_all(&mut m, &tri_sources);
    println!("  as triangles: {:.2} ms ({} polygons; columns a face only touches can differ)", t.elapsed().as_secs_f64() * 1000.0, m.poly_count());
    // One tile rebuilt (the middle one), many times.
    let (tx, tz) = (nx / 2, nz / 2);
    let (lo, hi) = mesh.tile_input_bounds(tx, tz);
    let mine: Vec<&Source> = sources.iter().filter(|s| s.touches(lo, hi)).collect();
    let n = 50;
    let t = Instant::now();
    for _ in 0..n {
        mesh.build_tile(tx, tz, &mine);
    }
    println!("rebuild one tile: {:.3} ms (target < 2 ms)", t.elapsed().as_secs_f64() * 1000.0 / n as f64);
    // Paths across the map.
    let mut rng = Rng::new(3, 9);
    let ext = Vec3::new(1.0, 1.8, 1.0);
    let mut times = Vec::new();
    let mut complete = 0;
    let mut nodes = 0;
    for _ in 0..200 {
        let a = Vec3::new(rng.range_f32(-31.0, -20.0), 0.0, rng.range_f32(-31.0, 31.0));
        let b = Vec3::new(rng.range_f32(20.0, 31.0), 0.0, rng.range_f32(-31.0, 31.0));
        let t = Instant::now();
        let p = mesh.find_path(a, b, ext, 4096);
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        if let Some(p) = p {
            complete += usize::from(p.complete());
            nodes += p.visited;
        }
    }
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "paths across (>= 40 m): mean {mean:.3} ms, p95 {:.3} ms, max {:.3} ms, {complete}/200 complete, {} polygons expanded on average (target < 0.2 ms)",
        times[times.len() * 95 / 100],
        times[times.len() - 1],
        nodes / 200
    );
    // A crowd of 100 agents walking to random goals.
    let mut crowd = Crowd::new(CrowdSettings::default());
    let mut rng = Rng::new(11, 2);
    for i in 0..100u64 {
        let p = mesh.random_point(&mut rng).unwrap().1;
        crowd.upsert(i + 1, AgentParams { speed: 2.5, ..AgentParams::default() }, p).unwrap();
        let g = mesh.random_point(&mut rng).unwrap().1;
        crowd.set_target(i + 1, Some(g));
    }
    let mut step_ms = Vec::new();
    let mut repaths = 0;
    for _ in 0..300 {
        let t = Instant::now();
        let s = crowd.step(&mesh, 1.0 / 30.0);
        step_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        repaths += s.repaths;
    }
    // Steady state: after the first second (its repath burst).
    let steady: Vec<f64> = step_ms[30..].to_vec();
    let mean = steady.iter().sum::<f64>() / steady.len() as f64;
    let max = steady.iter().copied().fold(0.0, f64::max);
    let first = step_ms[..30].iter().copied().fold(0.0, f64::max);
    println!("crowd of 100: {mean:.3} ms per step steady (max {max:.3}), first second max {first:.3} ms, {repaths} repaths in 10 s (target < 0.5 ms)");
}

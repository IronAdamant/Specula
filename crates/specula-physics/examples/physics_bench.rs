//! Physics benchmark (foundation F3): the PHYSICS.md budget scenes, timed on this machine.
//!
//! `cargo run --release -p specula-physics --example physics_bench [-- --quick]`
//!
//! - **pyramid**: a 20-row pyramid of boxes (210 bodies) settling and falling asleep.
//! - **awake 1000**: 1,000 mixed bodies (boxes, spheres, capsules, hulls) raining onto a floor;
//!   the target is under 2 ms per tick.
//! - **sleeping 10000**: 10,000 bodies asleep on the floor; the target is nearly nothing.
//! - **characters**: 100 capsule characters walking over a heightfield; the target is under
//!   0.05 ms per character.
//!
//! Each scene prints mean, p95 and max ms per tick and its final state hash (two runs on the
//! same build must print the same hashes: determinism).

#![allow(clippy::disallowed_methods, reason = "a benchmark measures wall-clock time")]

use specula_core::math::Vec3;
use specula_physics::{BodyDesc, BodyKind, CharacterConfig, ColliderDesc, Pose, Settings, Shape, Space};
use std::time::Instant;

const DT: f32 = 1.0 / 60.0;

fn v3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn floor(s: &mut Space, half: f32) {
    let c = ColliderDesc::new(Shape::cuboid(v3(half, 0.5, half)));
    s.insert(1, BodyDesc::new(BodyKind::Static, Pose::at(v3(0.0, -0.5, 0.0)), Some(c))).unwrap();
}

struct Timing {
    samples: Vec<f64>,
    stages: [f64; 7],
}

impl Timing {
    fn new() -> Timing {
        Timing { samples: Vec::new(), stages: [0.0; 7] }
    }

    fn step(&mut self, s: &mut Space) {
        let t = Instant::now();
        s.step(DT);
        self.samples.push(t.elapsed().as_secs_f64() * 1000.0);
        let st = s.report().times;
        for (acc, v) in self.stages.iter_mut().zip([
            st.pairs_us,
            st.narrowphase_us,
            st.solve_us,
            st.integrate_us,
            st.ccd_us,
            st.sleep_us,
            st.broadphase_us,
        ]) {
            *acc += v;
        }
    }

    fn line(&self, name: &str, s: &Space, per: Option<(f64, &str)>) {
        let mut v = self.samples.clone();
        v.sort_by(f64::total_cmp);
        let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
        let p95 = v[(v.len() * 95 / 100).min(v.len() - 1)];
        let max = v[v.len() - 1];
        let r = s.report();
        let extra = per.map_or(String::new(), |(n, what)| format!(", {:.4} ms per {what}", mean / n));
        println!(
            "{name:<16} mean {mean:7.3} ms  p95 {p95:7.3} ms  max {max:7.3} ms per tick{extra}  ({} bodies, {} awake, {} contacts)  hash {:016x}",
            r.bodies,
            r.awake,
            r.touching,
            s.hash()
        );
        if self.stages.iter().any(|v| *v > 0.0) {
            let n = self.samples.len().max(1) as f64 * 1000.0;
            let names = ["pairs", "narrowphase", "solve", "kinematic", "ccd", "sleep", "broadphase"];
            let parts: Vec<String> = names.iter().zip(self.stages).map(|(k, v)| format!("{k} {:.3}", v / n)).collect();
            println!("                 stages (ms): {}", parts.join(", "));
        }
    }
}

fn main() {
    let quick = std::env::args().any(|a| a == "--quick");
    if std::env::args().any(|a| a == "--pairs") {
        // Narrowphase cost per shape pair, resting contact.
        use specula_core::math::Quat;
        use specula_physics::contact::{collide, Manifold};
        let hull =
            Shape::hull(&[v3(-0.4, -0.3, -0.4), v3(0.4, -0.3, -0.4), v3(0.0, -0.3, 0.45), v3(0.0, 0.4, 0.0), v3(0.2, 0.1, 0.2)]).unwrap();
        let bx = Shape::cuboid(v3(0.4, 0.4, 0.4));
        let cases: Vec<(&str, Shape, Shape)> = vec![
            ("box-box", bx.clone(), bx.clone()),
            ("sphere-box", Shape::sphere(0.4), bx.clone()),
            ("capsule-box", Shape::capsule(0.3, 0.3), bx.clone()),
            ("sphere-sphere", Shape::sphere(0.4), Shape::sphere(0.4)),
            ("hull-box", hull.clone(), bx.clone()),
            ("hull-hull", hull.clone(), hull.clone()),
        ];
        let pa = Pose::new(v3(0.0, 0.0, 0.0), Quat::from_euler_xyz_deg(5.0, 20.0, 3.0));
        let pb = Pose::new(v3(0.1, 0.78, 0.05), Quat::from_euler_xyz_deg(-4.0, 50.0, 2.0));
        let mut m = Manifold::default();
        for (name, a, b) in cases {
            let n = 200_000;
            let t = Instant::now();
            let mut points = 0;
            for _ in 0..n {
                collide(&a, &pa, &b, &pb, 0.02, &mut m);
                points += m.len;
            }
            println!("{name:<14} {:7.1} ns per call ({} points)", t.elapsed().as_secs_f64() * 1e9 / n as f64, points / n);
        }
        return;
    }
    let ticks = if quick { 120 } else { 600 };
    println!("specula-physics bench: {} ticks per scene at 60 Hz, 4 substeps", ticks);

    // Pyramid.
    {
        let mut s = Space::default();
        floor(&mut s, 50.0);
        let rows = 20u64;
        let mut key = 100;
        for row in 0..rows {
            for col in 0..rows - row {
                let x = col as f32 * 1.02 - (rows - row - 1) as f32 * 0.51;
                let c = ColliderDesc::new(Shape::cuboid(v3(0.5, 0.5, 0.5)));
                s.insert(key, BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(x, 0.5 + row as f32, 0.0)), Some(c))).unwrap();
                key += 1;
            }
        }
        let mut t = Timing::new();
        for _ in 0..ticks {
            t.step(&mut s);
        }
        t.line("pyramid 20", &s, None);
        let top = s.body(key - 1).unwrap();
        println!("                 top box at y = {:.3} (expected {:.1}), asleep: {}", top.pose.p.y, rows as f32 - 0.5, top.sleeping);
    }

    // 1000 awake bodies.
    {
        let mut s = Space::default();
        floor(&mut s, 60.0);
        let hull =
            Shape::hull(&[v3(-0.4, -0.3, -0.4), v3(0.4, -0.3, -0.4), v3(0.0, -0.3, 0.45), v3(0.0, 0.4, 0.0), v3(0.2, 0.1, 0.2)]).unwrap();
        for i in 0..1000u64 {
            let shape = match i % 4 {
                0 => Shape::cuboid(v3(0.4, 0.4, 0.4)),
                1 => Shape::sphere(0.4),
                2 => Shape::capsule(0.3, 0.3),
                _ => hull.clone(),
            };
            let (x, z, y) = ((i % 10) as f32 * 1.2 - 6.0, ((i / 10) % 10) as f32 * 1.2 - 6.0, 1.0 + (i / 100) as f32 * 1.2);
            let c = ColliderDesc::new(shape);
            s.insert(100 + i, BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(x, y, z)), Some(c))).unwrap();
        }
        let mut t = Timing::new();
        for _ in 0..ticks.min(240) {
            t.step(&mut s);
        }
        t.line("awake 1000", &s, None);
    }

    // 10000 sleeping bodies.
    {
        let mut s = Space::default();
        floor(&mut s, 200.0);
        for i in 0..10_000u64 {
            let (x, z) = ((i % 100) as f32 * 1.5 - 75.0, (i / 100) as f32 * 1.5 - 75.0);
            let c = ColliderDesc::new(Shape::cuboid(v3(0.5, 0.5, 0.5)));
            let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(x, 0.5, z)), Some(c));
            d.sleeping = true;
            s.insert(100 + i, d).unwrap();
        }
        // First step finds the pairs; then measure.
        s.step(DT);
        let mut t = Timing::new();
        for _ in 0..ticks {
            t.step(&mut s);
        }
        t.line("sleeping 10000", &s, None);
    }

    // Characters on a heightfield.
    {
        let mut s = Space::new(Settings::default());
        let n = 65usize;
        let heights: Vec<f32> = (0..n * n)
            .map(|i| {
                let (x, z) = ((i % n) as f32, (i / n) as f32);
                0.4 * specula_core::dmath::sin(x * 0.3) * specula_core::dmath::cos(z * 0.25)
            })
            .collect();
        let hf = Shape::heightfield(n, n, heights, (1.0, 1.0)).unwrap();
        s.insert(1, BodyDesc::new(BodyKind::Static, Pose::IDENTITY, Some(ColliderDesc::new(hf)))).unwrap();
        let count = 100u64;
        for i in 0..count {
            let c = ColliderDesc::new(Shape::capsule(0.3, 0.6));
            let at = v3((i % 10) as f32 * 3.0 - 15.0, 2.0, (i / 10) as f32 * 3.0 - 15.0);
            s.insert(100 + i, BodyDesc::new(BodyKind::Kinematic, Pose::at(at), Some(c))).unwrap();
        }
        let cfg = CharacterConfig::default();
        let mut vel: Vec<Vec3> = (0..count).map(|i| v3(1.5 * if i % 2 == 0 { 1.0 } else { -1.0 }, 0.0, 1.0)).collect();
        let mut samples = Vec::new();
        for _ in 0..ticks {
            let t = Instant::now();
            for i in 0..count {
                let m = s.move_character(100 + i, vel[i as usize], &cfg, DT).unwrap();
                vel[i as usize] = m.velocity;
            }
            s.step(DT);
            samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let t = Timing { samples, stages: [0.0; 7] };
        t.line("characters 100", &s, Some((count as f64, "character")));
    }
}

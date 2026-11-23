//! Headless physics scenarios (docs/design/PHYSICS.md, "Test suite"): stacks, sleeping, bounce,
//! friction, tunnelling, triggers, the character course, 2D, budgets and determinism.

use specula_core::math::{Quat, Vec3};
use specula_physics::{BodyDesc, BodyKind, CharacterConfig, ColliderDesc, EventKind, Filter, Pose, Settings, Shape, Space};

const DT: f32 = 1.0 / 60.0;

fn v3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn ground(space: &mut Space) {
    let col = ColliderDesc::new(Shape::cuboid(v3(50.0, 0.5, 50.0)));
    space.insert(1, BodyDesc::new(BodyKind::Static, Pose::at(v3(0.0, -0.5, 0.0)), Some(col))).unwrap();
}

fn dynamic(space: &mut Space, key: u64, shape: Shape, at: Vec3) {
    space.insert(key, BodyDesc::new(BodyKind::Dynamic, Pose::at(at), Some(ColliderDesc::new(shape)))).unwrap();
}

fn tower(space: &mut Space, n: u64) {
    for i in 0..n {
        dynamic(space, 100 + i, Shape::cuboid(v3(0.5, 0.5, 0.5)), v3(0.0, 0.5 + i as f32, 0.0));
    }
}

#[test]
fn a_sphere_falls_rests_and_sleeps() {
    let mut s = Space::default();
    ground(&mut s);
    dynamic(&mut s, 2, Shape::sphere(0.5), v3(0.0, 3.0, 0.0));
    let mut began = false;
    for _ in 0..240 {
        let r = s.step(DT);
        began |= r.events.iter().any(|e| e.kind == EventKind::ContactBegin && (e.a, e.b) == (1, 2));
    }
    let b = s.body(2).unwrap();
    assert!(began, "a contact_begin event when it lands");
    assert!((b.pose.p.y - 0.5).abs() < 0.01, "rests on the ground: y = {}", b.pose.p.y);
    assert!(b.sleeping, "asleep after resting");
    assert_eq!(s.touching(2), vec![1]);
}

#[test]
fn a_ten_box_tower_stands_for_600_ticks_and_sleeps() {
    let mut s = Space::default();
    ground(&mut s);
    tower(&mut s, 10);
    let mut max_drift = 0.0f32;
    let mut deepest = 0.0f32;
    for _ in 0..600 {
        let r = s.step(DT);
        if let Some((d, _, _)) = r.deepest {
            deepest = deepest.min(d);
        }
        for i in 0..10 {
            let p = s.body(100 + i).unwrap().pose.p;
            max_drift = max_drift.max((p.x * p.x + p.z * p.z).sqrt());
        }
    }
    let top = s.body(109).unwrap();
    assert!((top.pose.p.y - 9.5).abs() < 0.05, "top box at {}", top.pose.p.y);
    assert!(max_drift < 0.01, "boxes drift sideways by {max_drift} m");
    assert!(deepest > -0.03, "penetration stays small: {deepest}");
    assert!(top.sleeping, "the tower falls asleep");
    assert_eq!(s.report().awake, 0);
}

#[test]
fn a_pyramid_settles_and_sleeps() {
    let mut s = Space::default();
    ground(&mut s);
    let rows = 8u64;
    let mut key = 100;
    for row in 0..rows {
        for col in 0..rows - row {
            let x = col as f32 * 1.02 - (rows - row - 1) as f32 * 0.51;
            dynamic(&mut s, key, Shape::cuboid(v3(0.5, 0.5, 0.5)), v3(x, 0.5 + row as f32, 0.0));
            key += 1;
        }
    }
    for _ in 0..900 {
        s.step(DT);
    }
    let top = s.body(key - 1).unwrap();
    assert!((top.pose.p.y - (rows as f32 - 0.5)).abs() < 0.08, "pyramid top at {}", top.pose.p.y);
    assert_eq!(s.report().awake, 0, "the whole pyramid sleeps");
}

/// The same scene built in a different order (other slots, another tree shape) steps to
/// bit-identical states: nothing depends on insertion order.
#[test]
fn determinism_hashes_match_whatever_the_insertion_order() {
    let build = |reverse: bool| {
        let mut s = Space::default();
        let mut items: Vec<(u64, Shape, Vec3)> = Vec::new();
        for i in 0..30u64 {
            let shape = match i % 3 {
                0 => Shape::cuboid(v3(0.4, 0.3, 0.5)),
                1 => Shape::sphere(0.4),
                _ => Shape::capsule(0.3, 0.4),
            };
            items.push((100 + i, shape, v3((i % 5) as f32 * 0.9 - 2.0, 1.0 + (i / 5) as f32 * 1.1, (i % 2) as f32 * 0.3)));
        }
        if reverse {
            items.reverse();
            for (k, sh, p) in items {
                dynamic(&mut s, k, sh, p);
            }
            ground(&mut s);
        } else {
            ground(&mut s);
            for (k, sh, p) in items {
                dynamic(&mut s, k, sh, p);
            }
        }
        s
    };
    let (mut a, mut b) = (build(false), build(true));
    for tick in 0..300 {
        a.step(DT);
        b.step(DT);
        assert_eq!(a.hash(), b.hash(), "diverged at tick {tick}");
        assert_eq!(a.hash(), a.hash_uncached(), "the cached part of the hash is stale at tick {tick}");
    }
    // And a third run from scratch matches too.
    let mut c = build(false);
    for _ in 0..300 {
        c.step(DT);
    }
    assert_eq!(a.hash(), c.hash());
}

#[test]
fn restitution_bounces_and_friction_holds_on_slopes() {
    let mut s = Space::default();
    ground(&mut s);
    let mut col = ColliderDesc::new(Shape::sphere(0.25));
    col.restitution = 0.8;
    s.insert(2, BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(0.0, 5.0, 0.0)), Some(col))).unwrap();
    let mut bounced_to = 0.0f32;
    let mut landed = false;
    for _ in 0..240 {
        s.step(DT);
        let b = s.body(2).unwrap();
        if b.velocity.y > 0.5 {
            landed = true;
        }
        if landed {
            bounced_to = bounced_to.max(b.pose.p.y);
        }
    }
    // Ideal: 0.25 + 4.75 · 0.8² ≈ 3.29.
    assert!(bounced_to > 2.6 && bounced_to < 3.5, "bounced to {bounced_to}");

    // A box on a 20° slope: holds with friction 0.8, slides with 0.05.
    for (friction, slides) in [(0.8, false), (0.05, true)] {
        let mut s = Space::default();
        let tilt = Quat::from_euler_xyz_deg(0.0, 0.0, 20.0);
        let mut slope = ColliderDesc::new(Shape::cuboid(v3(20.0, 0.5, 5.0)));
        slope.friction = friction;
        s.insert(1, BodyDesc::new(BodyKind::Static, Pose::new(v3(0.0, -0.5, 0.0), tilt), Some(slope))).unwrap();
        let mut c = ColliderDesc::new(Shape::cuboid(v3(0.5, 0.5, 0.5)));
        c.friction = friction;
        let start = tilt.rotate(v3(0.0, 0.5, 0.0));
        s.insert(2, BodyDesc::new(BodyKind::Dynamic, Pose::new(start, tilt), Some(c))).unwrap();
        for _ in 0..120 {
            s.step(DT);
        }
        let moved = (s.body(2).unwrap().pose.p - start).length();
        assert_eq!(moved > 0.5, slides, "friction {friction}: moved {moved} m");
    }
}

/// A bouncing ball at a 30 Hz tick moves up to ~35 cm a tick; speculative contacts scaled by
/// the closing speed catch every landing, so it never sinks into the floor.
#[test]
fn bouncing_balls_never_sink_into_the_floor() {
    let mut s = Space::new(Settings { substeps: 8, ..Settings::default() });
    ground(&mut s);
    let mut c = ColliderDesc::new(Shape::sphere(0.3));
    c.restitution = 0.7;
    s.insert(2, BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(0.0, 6.0, 0.0)), Some(c))).unwrap();
    let mut deepest = 0.0f32;
    for _ in 0..300 {
        let r = s.step(1.0 / 30.0);
        if let Some((d, _, _)) = r.deepest {
            deepest = deepest.min(d);
        }
    }
    assert!(deepest > -0.02, "sank {deepest} m into the floor");
    assert!((s.body(2).unwrap().pose.p.y - 0.3).abs() < 0.01);
}

#[test]
fn fast_bodies_do_not_tunnel_through_thin_walls() {
    for ccd in [true, false] {
        let mut s = Space::new(Settings { gravity: Vec3::ZERO, ..Settings::default() });
        let wall = ColliderDesc::new(Shape::cuboid(v3(0.025, 5.0, 5.0)));
        s.insert(1, BodyDesc::new(BodyKind::Static, Pose::at(v3(5.0, 0.0, 0.0)), Some(wall))).unwrap();
        let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::at(Vec3::ZERO), Some(ColliderDesc::new(Shape::sphere(0.1))));
        d.velocity = v3(300.0, 0.0, 0.0);
        d.ccd = ccd;
        s.insert(2, d).unwrap();
        let mut guarded = false;
        let mut fast = false;
        for _ in 0..30 {
            let r = s.step(DT);
            guarded |= r.events.iter().any(|e| e.kind == EventKind::Ccd && e.a == 2 && e.b == 1);
            fast |= r.fast.iter().any(|f| f.key == 2);
        }
        let x = s.body(2).unwrap().pose.p.x;
        assert!(fast, "reported as a fast body");
        if ccd {
            assert!(guarded, "the guard fired");
            assert!(x < 5.0, "stopped at the wall: x = {x}");
        } else {
            assert!(x > 5.0, "without the guard it passes through: x = {x}");
        }
    }
}

#[test]
fn fast_bodies_do_not_tunnel_through_thin_dynamic_bodies() {
    // A bullet at 300 m/s and a free-floating 5 cm plate: the guard casts against the plate too.
    for ccd in [true, false] {
        let mut s = Space::new(Settings { gravity: Vec3::ZERO, ..Settings::default() });
        let mut plate = BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(5.0, 0.0, 0.0)), Some(ColliderDesc::new(Shape::cuboid(v3(0.025, 2.0, 2.0)))));
        plate.mass = 20.0;
        s.insert(1, plate).unwrap();
        let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::at(Vec3::ZERO), Some(ColliderDesc::new(Shape::sphere(0.1))));
        d.velocity = v3(300.0, 0.0, 0.0);
        d.ccd = ccd;
        s.insert(2, d).unwrap();
        let mut guarded = false;
        for _ in 0..30 {
            let r = s.step(DT);
            guarded |= r.events.iter().any(|e| e.kind == EventKind::Ccd && e.a == 2 && e.b == 1);
        }
        let (x, plate_v) = (s.body(2).unwrap().pose.p.x, s.body(1).unwrap().velocity.x);
        if ccd {
            assert!(guarded, "the guard fired on the dynamic plate");
            assert!(x < s.body(1).unwrap().pose.p.x, "the bullet stayed on its side: x = {x}");
            assert!(plate_v > 0.5, "and hit it: the plate moves at {plate_v} m/s");
        } else {
            assert!(x > 5.0, "without the guard it passes through: x = {x}");
        }
    }
}

#[test]
fn the_threaded_narrowphase_gives_the_same_states() {
    // 900 boxes in a heap (thousands of pairs: the narrowphase splits them over threads).
    let build = |threads: usize| {
        let mut s = Space::new(Settings { threads, ..Settings::default() });
        ground(&mut s);
        for i in 0..900u64 {
            let (x, y, z) = ((i % 10) as f32 * 1.05 - 5.0, 0.5 + (i / 100) as f32 * 1.05, ((i / 10) % 10) as f32 * 1.05 - 5.0);
            dynamic(&mut s, 10 + i, Shape::cuboid(v3(0.5, 0.5, 0.5)), v3(x, y, z));
        }
        s
    };
    let (mut one, mut four) = (build(1), build(4));
    for t in 0..40 {
        one.step(DT);
        four.step(DT);
        assert_eq!(one.hash(), four.hash(), "diverged at step {t}");
    }
    assert!(one.report().pairs > 2000, "enough pairs to split: {}", one.report().pairs);
}

/// The coloured solver on its thread pool: a heap of boxes and spheres, a pendulum chain of
/// joints, a spinning kinematic paddle and a tower, the same states (bit for bit, every step) on
/// 1, 2, 3, 4 and 8 threads; and coloured differs from pair order only in the order it solves.
#[test]
fn the_coloured_solver_gives_the_same_states_on_any_thread_count() {
    use specula_physics::joint::{JointDesc, JointKind};
    let build = |threads: usize| {
        let mut s = Space::new(Settings { threads, coloured: true, ..Settings::default() });
        ground(&mut s);
        for i in 0..300u64 {
            let (x, y, z) = ((i % 10) as f32 * 1.02 - 5.0, 0.5 + (i / 100) as f32 * 1.05, ((i / 10) % 10) as f32 * 1.02 - 5.0);
            let shape = if i % 4 == 0 { Shape::sphere(0.5) } else { Shape::cuboid(v3(0.5, 0.5, 0.5)) };
            dynamic(&mut s, 10 + i, shape, v3(x, y, z));
        }
        let mut k = BodyDesc::new(BodyKind::Kinematic, Pose::at(v3(0.0, 0.3, 9.0)), Some(ColliderDesc::new(Shape::cuboid(v3(4.0, 0.3, 0.3)))));
        k.angular_velocity = v3(0.0, 1.5, 0.0);
        s.insert(5, k).unwrap();
        for i in 0..6u64 {
            dynamic(&mut s, 2000 + i, Shape::cuboid(v3(0.1, 0.25, 0.1)), v3(9.0, 6.0 - 0.5 * i as f32, 0.0));
            let a = (i > 0).then_some(2000 + i - 1);
            let fa = if i == 0 { Pose::at(v3(9.0, 6.25, 0.0)) } else { Pose::at(v3(0.0, -0.25, 0.0)) };
            s.insert_joint(3000 + i, JointDesc::new(JointKind::Ball, a, Some(2000 + i), fa, Pose::at(v3(0.0, 0.25, 0.0)))).unwrap();
        }
        dynamic(&mut s, 2100, Shape::cuboid(v3(0.4, 0.4, 0.4)), v3(7.0, 3.5, 0.0));
        s.set_velocity(2100, v3(4.0, 0.0, 0.0), Vec3::ZERO);
        s
    };
    let mut runs: Vec<Space> = [1, 2, 3, 4, 8].into_iter().map(build).collect();
    // And the wide solve (four constraints in lanes) on four threads.
    let mut wide = build(4);
    wide.settings.wide = true;
    runs.push(wide);
    let mut most = 0;
    for t in 0..120 {
        for s in &mut runs {
            s.step(DT);
        }
        most = most.max(runs[0].report().constraints);
        let h = runs[0].hash();
        for (i, s) in runs.iter().enumerate().skip(1) {
            assert_eq!(s.hash(), h, "{} diverged at step {t}", ["1 thread", "2 threads", "3 threads", "4 threads", "8 threads", "4 threads, wide"][i]);
        }
    }
    assert!(most > 300, "enough contacts to colour: {most}");
}

#[test]
fn compound_bodies_stand_on_their_parts() {
    // A stool: a seat on four thin legs, dropped from a metre. It lands on its legs, upright,
    // with the seat clear of the floor.
    let mut s = Space::default();
    ground(&mut s);
    let leg = |x: f32, z: f32| (Pose::at(v3(x, -0.25, z)), Shape::cuboid(v3(0.03, 0.25, 0.03)));
    let stool = Shape::compound(vec![(Pose::at(v3(0.0, 0.03, 0.0)), Shape::cuboid(v3(0.3, 0.03, 0.3))), leg(0.25, 0.25), leg(-0.25, 0.25), leg(0.25, -0.25), leg(-0.25, -0.25)]).unwrap();
    let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(0.0, 1.5, 0.0)), Some(ColliderDesc::new(stool)));
    d.mass = 4.0;
    s.insert(2, d).unwrap();
    for _ in 0..240 {
        s.step(DT);
    }
    let b = s.body(2).unwrap();
    assert!((b.pose.p.y - 0.5).abs() < 0.02, "on its legs: the seat's underside at {}", b.pose.p.y);
    assert!(b.pose.q.rotate(Vec3::Y).y > 0.999, "upright");
    assert!(b.sleeping, "and asleep");
    // Its centre of mass is where its parts' volumes put it (the seat is most of it).
    assert!(b.center.y > b.pose.p.y - 0.1 && b.center.y < b.pose.p.y + 0.03, "{:?}", b.center);
    // Rays and casts see the parts, not the box around them: one between the legs misses.
    let (hits, _) = s.raycast(v3(0.0, 0.25, -2.0), v3(0.0, 0.0, 1.0), 4.0, &Filter::default(), 4);
    assert!(hits.is_empty(), "between the legs: {hits:?}");
    let (hits, _) = s.raycast(v3(0.25, 0.25, -2.0), v3(0.0, 0.0, 1.0), 4.0, &Filter::default(), 4);
    assert_eq!(hits.first().map(|h| h.key), Some(2), "a leg");
    // Nested compounds, meshes inside them and empty ones are refused.
    assert!(Shape::compound(vec![]).is_err());
    assert!(Shape::compound(vec![(Pose::IDENTITY, Shape::compound(vec![(Pose::IDENTITY, Shape::sphere(1.0))]).unwrap())]).is_err());
}

#[test]
fn triggers_report_enter_and_exit() {
    let mut s = Space::default();
    let mut zone = ColliderDesc::new(Shape::cuboid(v3(2.0, 0.5, 2.0)));
    zone.is_trigger = true;
    s.insert(1, BodyDesc::new(BodyKind::Static, Pose::at(v3(0.0, 5.0, 0.0)), Some(zone))).unwrap();
    dynamic(&mut s, 2, Shape::sphere(0.25), v3(0.0, 8.0, 0.0));
    let mut kinds = Vec::new();
    for _ in 0..120 {
        for e in &s.step(DT).events {
            kinds.push((e.kind, e.a, e.b));
        }
    }
    assert_eq!(kinds, vec![(EventKind::TriggerEnter, 1, 2), (EventKind::TriggerExit, 1, 2)]);
    // Triggers never block, and queries skip them unless asked.
    assert!(s.body(2).unwrap().pose.p.y < 4.0);
    let (hits, _) = s.raycast(v3(0.0, 10.0, 0.0), v3(0.0, -1.0, 0.0), 20.0, &Filter::default(), 8);
    assert!(hits.iter().all(|h| h.key != 1));
    let (hits, _) = s.raycast(v3(0.0, 10.0, 0.0), v3(0.0, -1.0, 0.0), 20.0, &Filter { triggers: true, ..Filter::default() }, 8);
    assert_eq!(hits.first().map(|h| h.key), Some(1));
}

#[test]
fn queries_are_sorted_and_filtered_by_layer() {
    let mut s = Space::default();
    for i in 0..5u64 {
        let mut c = ColliderDesc::new(Shape::sphere(0.5));
        c.layer = 1 << (i % 2);
        s.insert(10 + i, BodyDesc::new(BodyKind::Static, Pose::at(v3(i as f32 * 2.0, 0.0, 0.0)), Some(c))).unwrap();
    }
    let (hits, total) = s.raycast(v3(-5.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), 100.0, &Filter::default(), 3);
    assert_eq!((hits.iter().map(|h| h.key).collect::<Vec<_>>(), total), (vec![10, 11, 12], 5));
    assert!((hits[0].at - 4.5).abs() < 1e-4 && (hits[0].normal.x + 1.0).abs() < 1e-4);
    let (hits, _) = s.raycast(v3(-5.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), 100.0, &Filter { mask: 2, ..Filter::default() }, 8);
    assert_eq!(hits.iter().map(|h| h.key).collect::<Vec<_>>(), vec![11, 13]);
    let (keys, total) = s.overlap(&Shape::cuboid(v3(2.5, 1.0, 1.0)), &Pose::at(v3(2.0, 0.0, 0.0)), &Filter::default(), 10);
    assert_eq!((keys, total), (vec![10, 11, 12], 3));
    let (hits, _) = s.cast(&Shape::sphere(0.25), &Pose::at(v3(4.0, 5.0, 0.0)), v3(0.0, -10.0, 0.0), &Filter::default(), 4, 0.01);
    assert_eq!(hits.first().map(|h| h.key), Some(12));
    assert!((hits[0].at - (5.0 - 0.75) / 10.0).abs() < 0.003, "{:?}", hits[0]);
}

fn character(s: &mut Space, at: Vec3) -> u64 {
    let col = ColliderDesc::new(Shape::capsule(0.3, 0.6));
    // Body origin at the capsule's centre: feet at y − 0.9.
    s.insert(50, BodyDesc::new(BodyKind::Kinematic, Pose::at(at), Some(col))).unwrap();
    50
}

fn walk(s: &mut Space, key: u64, vel: Vec3, ticks: usize) -> specula_physics::CharacterMove {
    let cfg = CharacterConfig::default();
    let mut v = vel;
    let mut last = None;
    for _ in 0..ticks {
        let m = s.move_character(key, v, &cfg, DT).unwrap();
        v = m.velocity;
        s.step(DT);
        last = Some(m);
    }
    last.unwrap()
}

#[test]
fn the_character_walks_steps_and_respects_slopes() {
    // Flat ground: 2 m/s for one second.
    let mut s = Space::default();
    ground(&mut s);
    let c = character(&mut s, v3(0.0, 0.92, 0.0));
    let m = walk(&mut s, c, v3(2.0, 0.0, 0.0), 60);
    assert!(m.grounded, "{m:?}");
    assert!((m.position.x - 2.0).abs() < 0.05 && (m.position.y - 0.91).abs() < 0.03, "{m:?}");

    // A wall blocks.
    let wall = ColliderDesc::new(Shape::cuboid(v3(0.25, 2.0, 5.0)));
    s.insert(2, BodyDesc::new(BodyKind::Static, Pose::at(v3(3.5, 2.0, 0.0)), Some(wall))).unwrap();
    let m = walk(&mut s, c, v3(2.0, 0.0, 0.0), 60);
    assert!(m.position.x < 3.25 - 0.3 + 0.02 && m.position.x > 2.8, "stopped at the wall: {m:?}");

    // Stairs of 0.2 m: climbed; a 0.6 m ledge: not.
    let mut s = Space::default();
    ground(&mut s);
    for i in 0..4 {
        let step = ColliderDesc::new(Shape::cuboid(v3(0.25, 0.1 + 0.1 * i as f32, 2.0)));
        s.insert(10 + i, BodyDesc::new(BodyKind::Static, Pose::at(v3(2.0 + 0.5 * i as f32, 0.1 + 0.1 * i as f32, 0.0)), Some(step)))
            .unwrap();
    }
    let ledge = ColliderDesc::new(Shape::cuboid(v3(1.0, 0.3, 2.0)));
    s.insert(20, BodyDesc::new(BodyKind::Static, Pose::at(v3(3.75 + 1.0, 0.8 + 0.3, 0.0)), Some(ledge))).unwrap();
    let c = character(&mut s, v3(0.0, 0.92, 0.0));
    let m = walk(&mut s, c, v3(1.5, 0.0, 0.0), 180);
    assert!(m.position.y > 0.8 + 0.9 - 0.05, "climbed the stairs to the top step: {m:?}");
    assert!(m.position.x < 3.75 - 0.3 + 0.05 && m.grounded, "the 0.6 m ledge blocks: {m:?}");

    // Slopes: 30° is walkable, 60° is not.
    for (deg, climbs) in [(30.0f32, true), (60.0, false)] {
        let mut s = Space::default();
        ground(&mut s);
        let q = Quat::from_euler_xyz_deg(0.0, 0.0, deg);
        let ramp = ColliderDesc::new(Shape::cuboid(v3(4.0, 0.25, 3.0)));
        // A ramp rising towards +x, its lower edge at x ≈ 2.
        let centre = v3(2.0, 0.0, 0.0) + q.rotate(v3(4.0, -0.25, 0.0));
        s.insert(2, BodyDesc::new(BodyKind::Static, Pose::new(centre, q), Some(ramp))).unwrap();
        let c = character(&mut s, v3(0.0, 0.92, 0.0));
        let m = walk(&mut s, c, v3(2.0, 0.0, 0.0), 150);
        assert_eq!(m.position.y > 1.5, climbs, "{deg}°: {m:?}");
    }
}

#[test]
fn the_character_falls_off_ledges_and_lands() {
    let mut s = Space::default();
    let high = ColliderDesc::new(Shape::cuboid(v3(1.0, 1.0, 2.0)));
    s.insert(2, BodyDesc::new(BodyKind::Static, Pose::at(v3(0.0, 1.0, 0.0)), Some(high))).unwrap();
    ground(&mut s);
    let c = character(&mut s, v3(0.0, 2.92, 0.0));
    let m = walk(&mut s, c, v3(2.0, 0.0, 0.0), 45);
    assert!(m.position.x > 1.4 && !m.grounded, "walked off: {m:?}");
    let m = walk(&mut s, c, v3(0.0, 0.0, 0.0), 90);
    assert!(m.grounded && (m.position.y - 0.91).abs() < 0.03, "landed on the ground: {m:?}");
}

#[test]
fn kinematic_platforms_push_boxes_and_carry_characters() {
    let mut s = Space::default();
    ground(&mut s);
    let plat = ColliderDesc::new(Shape::cuboid(v3(1.0, 0.1, 1.0)));
    s.insert(5, BodyDesc::new(BodyKind::Kinematic, Pose::at(v3(0.0, 0.2, 0.0)), Some(plat))).unwrap();
    let c = character(&mut s, v3(0.0, 0.3 + 0.92, 0.0));
    // Let it settle on the platform, then move the platform 1 m in 1 s.
    walk(&mut s, c, Vec3::ZERO, 10);
    let start = s.body(c).unwrap().pose.p;
    let cfg = CharacterConfig::default();
    let mut v = Vec3::ZERO;
    for i in 1..=60 {
        s.move_kinematic(5, Pose::at(v3(i as f32 / 60.0, 0.2, 0.0)));
        let m = s.move_character(c, v, &cfg, DT).unwrap();
        v = m.velocity;
        s.step(DT);
    }
    let end = s.body(c).unwrap().pose.p;
    assert!((end.x - start.x - 1.0).abs() < 0.1, "carried about 1 m: {start:?} -> {end:?}");

    // A kinematic pusher moves a box.
    let mut s = Space::default();
    ground(&mut s);
    dynamic(&mut s, 2, Shape::cuboid(v3(0.5, 0.5, 0.5)), v3(2.0, 0.5, 0.0));
    let pusher = ColliderDesc::new(Shape::cuboid(v3(0.25, 0.5, 1.0)));
    let mut d = BodyDesc::new(BodyKind::Kinematic, Pose::at(v3(0.0, 0.5, 0.0)), Some(pusher));
    d.velocity = v3(1.0, 0.0, 0.0);
    s.insert(5, d).unwrap();
    for _ in 0..180 {
        s.step(DT);
    }
    let bx = s.body(2).unwrap().pose.p.x;
    assert!(bx > 3.0, "pushed along: {bx}");
}

#[test]
fn two_d_mode_keeps_bodies_in_the_plane() {
    let mut s = Space::new(Settings { plane_2d: true, ..Settings::default() });
    ground(&mut s);
    // A tilted box dropped on a corner: in 2D it tips over about Z only.
    let col = ColliderDesc::new(Shape::cuboid(v3(0.5, 0.5, 0.1)));
    let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::new(v3(0.0, 2.0, 0.0), Quat::from_euler_xyz_deg(10.0, 20.0, 30.0)), Some(col));
    d.angular_velocity = v3(1.0, 2.0, 3.0);
    s.insert(2, d).unwrap();
    for _ in 0..240 {
        s.step(DT);
        let b = s.body(2).unwrap();
        assert_eq!(b.pose.p.z, 0.0);
        assert_eq!((b.angular_velocity.x, b.angular_velocity.y), (0.0, 0.0));
    }
}

#[test]
fn budgets_refuse_with_structured_errors() {
    let mut s = Space::new(Settings { max_bodies: 3, ..Settings::default() });
    for k in 0..3 {
        dynamic(&mut s, k, Shape::sphere(0.5), v3(k as f32, 0.0, 0.0));
    }
    let e = s.insert(9, BodyDesc::new(BodyKind::Dynamic, Pose::IDENTITY, None)).unwrap_err();
    assert_eq!(e.code, "budget");
    let r = s.step(DT);
    assert_eq!(r.errors.first().map(|e| e.code), Some("budget"));
    // Static-only shapes refuse dynamic bodies.
    let mesh = Shape::mesh(vec![v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0)], vec![[0, 2, 1]]).unwrap();
    let e = Space::default().insert(1, BodyDesc::new(BodyKind::Dynamic, Pose::IDENTITY, Some(ColliderDesc::new(mesh)))).unwrap_err();
    assert_eq!(e.code, "static_only_shape");
}

#[test]
fn removing_a_support_wakes_what_rests_on_it() {
    let mut s = Space::default();
    ground(&mut s);
    tower(&mut s, 3);
    for _ in 0..200 {
        s.step(DT);
    }
    assert!(s.body(102).unwrap().sleeping);
    s.remove(101);
    for _ in 0..120 {
        s.step(DT);
    }
    let top = s.body(102).unwrap();
    assert!((top.pose.p.y - 1.5).abs() < 0.05, "the top box fell onto the bottom one: {}", top.pose.p.y);
}

// ---------------------------------------------------------------------------------------------
// Review round 1 regressions (each one a reviewer's repro).

/// Finding 2: thin and huge boxes (a 1000 x 0.01 x 1000 plate) are valid colliders, and bodies
/// rest on them.
#[test]
fn thin_plates_hold_bodies() {
    let mut s = Space::default();
    let plate = ColliderDesc::new(Shape::cuboid(v3(500.0, 0.005, 500.0)));
    s.insert(1, BodyDesc::new(BodyKind::Static, Pose::IDENTITY, Some(plate))).unwrap();
    dynamic(&mut s, 2, Shape::cuboid(v3(0.5, 0.5, 0.5)), v3(0.0, 2.0, 0.0));
    let tiny = ColliderDesc::new(Shape::cuboid(v3(5.0, 5.0, 5.0e-5)));
    s.insert(3, BodyDesc::new(BodyKind::Static, Pose::at(v3(20.0, 6.0, 0.0)), Some(tiny))).unwrap();
    for _ in 0..240 {
        s.step(DT);
    }
    let b = s.body(2).unwrap();
    assert!((b.pose.p.y - 0.505).abs() < 0.02 && b.sleeping, "{:?}", b.pose.p);
    let (keys, _) = s.overlap(&Shape::cuboid(v3(5000.0, 5.0e-5, 5000.0)), &Pose::IDENTITY, &Filter::default(), 8);
    assert_eq!(keys, vec![1]);
}

/// Finding 4: an idle character doesn't keep waking what is near it.
#[test]
fn an_idle_character_lets_its_neighbours_sleep() {
    let mut s = Space::default();
    ground(&mut s);
    dynamic(&mut s, 2, Shape::cuboid(v3(0.5, 0.5, 0.5)), v3(0.62, 0.5, 0.0));
    let c = character(&mut s, v3(-0.3, 0.92, 0.0));
    walk(&mut s, c, Vec3::ZERO, 300);
    assert!(s.body(2).unwrap().sleeping, "the crate sleeps next to the idle character");
    let before = s.body(2).unwrap().pose;
    walk(&mut s, c, Vec3::ZERO, 120);
    assert!(s.body(2).unwrap().sleeping);
    assert_eq!(s.body(2).unwrap().pose, before);
    // Walking into it wakes and pushes it.
    walk(&mut s, c, v3(1.0, 0.0, 0.0), 60);
    assert!(s.body(2).unwrap().pose.p.x > before.p.x + 0.2, "{:?}", s.body(2).unwrap().pose.p);
}

/// Finding 5: a big box on a fine heightfield is held at its corners (nearest-first triangles),
/// rests and sleeps, and the per-pair cap is reported as a structured budget error.
#[test]
fn big_boxes_rest_on_fine_heightfields() {
    let n = 129usize;
    let hf = Shape::heightfield(n, n, vec![0.0; n * n], (0.25, 0.25)).unwrap();
    let mut s = Space::default();
    s.insert(1, BodyDesc::new(BodyKind::Static, Pose::IDENTITY, Some(ColliderDesc::new(hf)))).unwrap();
    dynamic(&mut s, 2, Shape::cuboid(v3(3.0, 3.0, 3.0)), v3(0.3, 3.5, 0.2));
    let mut capped = false;
    for _ in 0..600 {
        let r = s.step(DT);
        capped |= r.errors.iter().any(|e| e.code == "budget" && e.message.contains("mesh triangles"));
    }
    let b = s.body(2).unwrap();
    assert!((b.center.y - 3.0).abs() < 0.02 && b.sleeping, "{:?} sleeping {}", b.center, b.sleeping);
    assert!(capped, "2,300 triangles under a 6 m box exceed the per-pair cap: reported");
}

/// Finding 5: a shape cast across a large mesh considers every triangle on its way (nearest
/// first, pruned by the best hit), not the first few hundred.
#[test]
fn casts_across_large_meshes_find_their_hit() {
    let (nx, nz) = (400usize, 4usize);
    let mut v = Vec::new();
    for z in 0..=nz {
        for x in 0..=nx {
            v.push(v3(x as f32 - 200.0, 0.0, z as f32 - 2.0));
        }
    }
    let row = (nx + 1) as u32;
    let mut t = Vec::new();
    for z in 0..nz as u32 {
        for x in 0..nx as u32 {
            let a = z * row + x;
            t.push([a, a + row, a + 1]);
            t.push([a + 1, a + row, a + row + 1]);
        }
    }
    let mut s = Space::default();
    s.insert(1, BodyDesc::new(BodyKind::Static, Pose::IDENTITY, Some(ColliderDesc::new(Shape::mesh(v, t).unwrap())))).unwrap();
    let (hits, _) = s.cast(&Shape::sphere(0.5), &Pose::at(v3(-190.0, 1.0, 0.0)), v3(380.0, -2.0, 0.0), &Filter::default(), 4, 0.001);
    assert_eq!(hits.len(), 1);
    assert!((hits[0].at - 0.25).abs() < 0.002, "{:?}", hits[0]);
}

/// Finding 6: hulls on moving bodies are capped (structured error); hull piles stay correct.
#[test]
fn moving_hulls_are_capped_and_hull_piles_settle() {
    let pts: Vec<Vec3> = (0..32).map(|i| {
        let a = i as f32 * 2.399;
        let y = 1.0 - 2.0 * (i as f32 + 0.5) / 32.0;
        let r = (1.0 - y * y).sqrt();
        v3(specula_core::dmath::cos(a) * r * 0.5, y * 0.5, specula_core::dmath::sin(a) * r * 0.5)
    }).collect();
    let big = Shape::hull(&pts).unwrap();
    let e = Space::default().insert(1, BodyDesc::new(BodyKind::Dynamic, Pose::IDENTITY, Some(ColliderDesc::new(big.clone())))).unwrap_err();
    assert_eq!(e.code, "hull_too_big");
    assert!(Space::default().insert(1, BodyDesc::new(BodyKind::Static, Pose::IDENTITY, Some(ColliderDesc::new(big)))).is_ok());
    // A pile of 16-point hulls lands and sleeps; the cached features keep it as stable as a fresh test.
    let small = Shape::hull(&pts.iter().step_by(2).copied().collect::<Vec<_>>()).unwrap();
    let mut s = Space::default();
    ground(&mut s);
    for i in 0..27u64 {
        dynamic(&mut s, 10 + i, small.clone(), v3((i % 3) as f32 * 1.1, 0.6 + (i / 9) as f32 * 1.1, ((i / 3) % 3) as f32 * 1.1));
    }
    for _ in 0..900 {
        s.step(DT);
    }
    assert_eq!(s.report().awake, 0, "the hull pile sleeps");
    let deepest = s.report().deepest.map_or(0.0, |d| d.0);
    assert!(deepest > -0.02, "{deepest}");
}

/// Finding 8: the cached part of the hash follows every change (sleep, wake, removal, moves).
#[test]
fn the_incremental_hash_matches_a_full_one() {
    let mut s = Space::default();
    ground(&mut s);
    tower(&mut s, 4);
    for t in 0..400 {
        s.step(DT);
        if t == 200 {
            s.remove(101);
        }
        if t == 300 {
            s.set_pose(1, Pose::at(v3(0.0, -0.4, 0.0)));
        }
        assert_eq!(s.hash(), s.hash_uncached(), "tick {t}");
    }
}

/// Review round 2, finding 1: teleporting a static wall wakes what it touched and what it now
/// touches, not every sleeping body inside the box spanning both places.
#[test]
fn teleporting_a_wall_wakes_only_its_ends() {
    let mut s = Space::default();
    s.insert(1, BodyDesc::new(BodyKind::Static, Pose::at(v3(0.0, -0.5, 0.0)), Some(ColliderDesc::new(Shape::cuboid(v3(100.0, 0.5, 100.0)))))).unwrap();
    for i in 0..1600u64 {
        let (x, z) = ((i % 40) as f32 * 1.5 - 30.0, (i / 40) as f32 * 1.5 - 30.0);
        let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(x, 0.5, z)), Some(ColliderDesc::new(Shape::cuboid(v3(0.5, 0.5, 0.5)))));
        d.sleeping = true;
        s.insert(100 + i, d).unwrap();
    }
    s.insert(2, BodyDesc::new(BodyKind::Static, Pose::at(v3(-50.0, 0.5, -50.0)), Some(ColliderDesc::new(Shape::cuboid(v3(1.0, 1.0, 1.0)))))).unwrap();
    s.step(DT);
    assert_eq!(s.report().awake, 0);
    // Across the whole grid, both ends outside it: nothing wakes.
    s.set_pose(2, Pose::at(v3(50.0, 0.5, 50.0)));
    s.step(DT);
    assert_eq!(s.report().awake, 0, "a teleport doesn't wake the ground it jumps over");
    // Onto the grid's far corner: only the crates it now overlaps wake.
    s.set_pose(2, Pose::at(v3(28.5, 0.5, 28.5)));
    s.step(DT);
    let awake = s.report().awake;
    assert!((1..=9).contains(&awake), "{awake}");
    // A small move (under two of its sizes) wakes its whole path, not just its ends.
    let mut s2 = Space::default();
    ground(&mut s2);
    let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(1.6, 0.5, 0.0)), Some(ColliderDesc::new(Shape::cuboid(v3(0.5, 0.5, 0.5)))));
    d.sleeping = true;
    s2.insert(10, d).unwrap();
    s2.insert(2, BodyDesc::new(BodyKind::Kinematic, Pose::at(v3(0.0, 0.5, 0.0)), Some(ColliderDesc::new(Shape::cuboid(v3(0.5, 0.5, 0.5)))))).unwrap();
    s2.step(DT);
    assert!(s2.body(10).unwrap().sleeping);
    // (Its ends don't touch the crate; the path between them does.)
    s2.set_pose(2, Pose::at(v3(3.2, 0.5, 0.0)));
    assert!(!s2.body(10).unwrap().sleeping, "the crate on its short path wakes");
}

fn corners(h: Vec3) -> Vec<Vec3> {
    (0..8).map(|i| v3(if i & 1 == 0 { -h.x } else { h.x }, if i & 2 == 0 { -h.y } else { h.y }, if i & 4 == 0 { -h.z } else { h.z })).collect()
}

fn ball_points(n: usize, r: f32) -> Vec<Vec3> {
    (0..n)
        .map(|i| {
            let a = i as f32 * 2.399;
            let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
            let rr = (1.0 - y * y).sqrt();
            v3(specula_core::dmath::cos(a) * rr * r, y * r, specula_core::dmath::sin(a) * rr * r)
        })
        .collect()
}

/// Rests a hull crate on a hull floor, then replaces the floor's and/or the crate's shape, in
/// place or by removing and re-inserting. Returns (peak speed, deepest contact, final position bits).
fn change_shapes(in_place: bool, which: u32) -> (f32, f32, [u32; 3]) {
    let mut s = Space::new(Settings { sleep: false, ..Default::default() });
    s.insert(1, BodyDesc::new(BodyKind::Static, Pose::at(v3(0.0, -0.5, 0.0)), Some(ColliderDesc::new(Shape::hull(&corners(v3(5.0, 0.5, 5.0))).unwrap())))).unwrap();
    dynamic(&mut s, 2, Shape::hull(&corners(v3(0.5, 0.5, 0.5))).unwrap(), v3(0.0, 0.5, 0.0));
    for _ in 0..60 {
        s.step(DT);
    }
    // A floor with extra points (more faces, in another order); a crate that is a 16-point ball.
    let mut fp = vec![v3(0.0, -0.5, 0.0), v3(0.0, 0.5, -5.0), v3(-5.0, 0.5, 0.0), v3(5.0, -0.5, 2.0)];
    fp.extend(corners(v3(5.0, 0.5, 5.0)));
    let floor = if which & 1 != 0 { Shape::hull(&fp).unwrap() } else { Shape::hull(&corners(v3(5.0, 0.5, 5.0))).unwrap() };
    let crate_shape = if which & 2 != 0 { Shape::hull(&ball_points(16, 0.5)).unwrap() } else { Shape::hull(&corners(v3(0.5, 0.5, 0.5))).unwrap() };
    let b = s.body(2).unwrap();
    let (pose, v, w) = (b.pose, b.velocity, b.angular_velocity);
    if !in_place {
        s.remove(1);
        s.remove(2);
    }
    s.insert(1, BodyDesc::new(BodyKind::Static, Pose::at(v3(0.0, -0.5, 0.0)), Some(ColliderDesc::new(floor)))).unwrap();
    let mut d = BodyDesc::new(BodyKind::Dynamic, pose, Some(ColliderDesc::new(crate_shape)));
    d.velocity = v;
    d.angular_velocity = w;
    s.insert(2, d).unwrap();
    let (mut peak, mut deepest) = (0.0f32, 0.0f32);
    for _ in 0..30 {
        s.step(DT);
        peak = peak.max(s.body(2).unwrap().velocity.length());
        deepest = deepest.min(s.report().deepest.map_or(0.0, |d| d.0));
    }
    let p = s.body(2).unwrap().pose.p;
    (peak, deepest, [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()])
}

/// Review round 2, finding 2: changing a collider in place starts its pairs afresh (no stale
/// face or edge index from the old shape, no warm starts): the same result as a fresh insert.
#[test]
fn in_place_shape_changes_match_fresh_inserts() {
    for which in [1u32, 2, 3] {
        let (in_place, fresh) = (change_shapes(true, which), change_shapes(false, which));
        assert_eq!(in_place, fresh, "change {which}");
        assert!(in_place.1 > -0.05, "change {which}: no phantom penetration ({})", in_place.1);
    }
    let (peak, deepest, _) = change_shapes(true, 1);
    assert!(peak < 0.05 && deepest > -0.01, "a new floor under a resting crate doesn't kick it ({peak}, {deepest})");
}

/// Review round 2, finding 3 (and the hash fuzz): random edits, sleeps, shape swaps and steps
/// keep the cached hash equal to a full one; the polytope test's memory is hashed too.
#[test]
fn hashes_survive_random_edits() {
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn f(&mut self, lo: f32, hi: f32) -> f32 {
            lo + (hi - lo) * ((self.next() % 10000) as f32 / 10000.0)
        }
        fn n(&mut self, k: u64) -> u64 {
            self.next() % k
        }
    }
    let shapes = [Shape::cuboid(v3(0.4, 0.4, 0.4)), Shape::sphere(0.4), Shape::capsule(0.3, 0.3), Shape::hull(&ball_points(12, 0.5)).unwrap()];
    for seed in 1..9u64 {
        let mut r = Rng(seed * 7919);
        let mut s = Space::default();
        s.insert(1, BodyDesc::new(BodyKind::Static, Pose::at(v3(0.0, -0.5, 0.0)), Some(ColliderDesc::new(Shape::cuboid(v3(20.0, 0.5, 20.0)))))).unwrap();
        for op in 0..300 {
            let key = 10 + r.n(30);
            match r.n(12) {
                0 | 1 => {
                    let kind = match r.n(5) {
                        0 => BodyKind::Static,
                        1 => BodyKind::Kinematic,
                        _ => BodyKind::Dynamic,
                    };
                    let mut d = BodyDesc::new(kind, Pose::at(v3(r.f(-3.0, 3.0), r.f(0.4, 4.0), r.f(-3.0, 3.0))), Some(ColliderDesc::new(shapes[r.n(4) as usize].clone())));
                    d.sleeping = kind == BodyKind::Dynamic && r.n(3) == 0;
                    let _ = s.insert(key, d);
                }
                2 => {
                    s.remove(key);
                }
                3 => s.set_pose(key, Pose::at(v3(r.f(-3.0, 3.0), r.f(0.4, 4.0), r.f(-3.0, 3.0)))),
                4 => s.set_velocity(key, v3(r.f(-2.0, 2.0), r.f(-2.0, 2.0), 0.0), Vec3::ZERO),
                5 => s.wake(key),
                6 => s.apply_impulse(key, v3(r.f(-5.0, 5.0), 0.0, 0.0), v3(0.0, 1.0, 0.0)),
                7 => s.move_kinematic(key, Pose::at(v3(r.f(-3.0, 3.0), 1.0, 0.0))),
                8 => s.settings.gravity = v3(0.0, r.f(-12.0, 0.0), 0.0),
                9 => s.wake_all(),
                10 => s.set_pose(1, Pose::at(v3(0.0, -0.5 + r.f(-0.01, 0.01), 0.0))),
                _ => {
                    for _ in 0..r.n(40) {
                        s.step(DT);
                    }
                }
            }
            let _ = s.hash();
            assert_eq!(s.hash(), s.hash_uncached(), "seed {seed} op {op}");
        }
    }
}

/// A landing's begin event says how fast the two closed, not what the solver left of it at
/// the step they touch (it said 0.4 m/s for a ball that fell half a metre: found when the
/// rules of a game were first told of a touch, 2026-11-21). And a run restored from a snapshot
/// taken at any step of the fall says the same speed to the bit (the rules hear it).
#[test]
fn a_begin_says_how_fast_the_two_closed_and_a_restored_run_says_the_same() {
    use specula_core::bytes::{ByteReader, ByteWriter};
    let scene = || {
        let mut s = Space::default();
        ground(&mut s);
        dynamic(&mut s, 2, Shape::sphere(0.5), v3(0.0, 1.0, 0.0));
        s
    };
    let begin = |s: &mut Space, steps: usize| -> Option<(usize, f32)> {
        (0..steps).find_map(|i| s.step(DT).events.iter().find(|e| e.kind == EventKind::ContactBegin).map(|e| (i, e.speed)))
    };
    let mut a = scene();
    let (at, speed) = begin(&mut a, 120).expect("it lands");
    // Half a metre of free fall: the square root of 2 x 9.81 x 0.5 is 3.13 m/s.
    assert!((2.8..=3.3).contains(&speed), "a ball that fell 0.5 m began its contact at {speed} m/s");
    for taken in 0..=at {
        let mut b = scene();
        for _ in 0..taken {
            assert!(b.step(DT).events.iter().all(|e| e.kind != EventKind::ContactBegin));
        }
        let mut w = ByteWriter::new();
        b.save_state(&mut w);
        let bytes = w.into_bytes();
        let mut c = scene();
        let mut r = ByteReader::new(&bytes);
        c.restore_state(&mut r).unwrap();
        r.finish().unwrap();
        let (later, again) = begin(&mut c, 120).expect("the restored one lands");
        assert_eq!((taken + later, again.to_bits()), (at, speed.to_bits()), "restored after {taken} steps");
    }
}

/// The bridge of the comparison's 3D game (2026-11-21): a ball at rest on a slab that slides 2 m
/// in 3 s and back. Returns the ball's place less the slab's, at the end of each leg.
fn ball_on_a_sliding_slab(carries: bool) -> [f32; 2] {
    let mut s = Space::default();
    let mut slab = ColliderDesc::new(Shape::cuboid(v3(1.0, 0.1, 1.0)));
    slab.carries = carries;
    s.insert(5, BodyDesc::new(BodyKind::Kinematic, Pose::at(v3(0.0, -0.1, 0.0)), Some(slab))).unwrap();
    dynamic(&mut s, 2, Shape::sphere(0.25), v3(0.0, 0.25, 0.0));
    for _ in 0..30 {
        s.step(DT);
    }
    let mut off = [0.0; 2];
    for i in 1..=360 {
        let x = if i <= 180 { i as f32 / 90.0 } else { (360 - i) as f32 / 90.0 };
        s.move_kinematic(5, Pose::at(v3(x, -0.1, 0.0)));
        s.step(DT);
        if i % 180 == 0 {
            off[i / 180 - 1] = s.body(2).unwrap().pose.p.x - x;
        }
    }
    off
}

#[test]
fn a_ball_on_a_sliding_slab_rolls_behind_it_unless_the_slab_carries() {
    // Honest contact: friction spins the ball up, and it settles at 2/7 of the slab's speed. It
    // is left behind (and off the slab's end, 1 m back) well before the first leg ends.
    let [first, _] = ball_on_a_sliding_slab(false);
    assert!(first < -1.0, "left behind: {first}");
    // A slab that carries: what rests on it has the slab's speed and takes its changes, so a
    // ball at rest on it stays at rest on it, through the turn at the far end too.
    let [first, second] = ball_on_a_sliding_slab(true);
    assert!(first.abs() < 0.02 && second.abs() < 0.02, "carried: {first} {second}");
}

/// A long slab that carries, sliding at 1 m/s along x from the first step, with a static floor
/// far off at x = 100 for what is to rest on something else. `step` moves the slab and steps.
fn sliding_carrier() -> (Space, impl FnMut(&mut Space)) {
    let mut s = Space::default();
    let mut slab = ColliderDesc::new(Shape::cuboid(v3(30.0, 0.1, 2.0)));
    slab.carries = true;
    s.insert(5, BodyDesc::new(BodyKind::Kinematic, Pose::at(v3(0.0, -0.1, 0.0)), Some(slab))).unwrap();
    s.insert(6, BodyDesc::new(BodyKind::Static, Pose::at(v3(100.0, -0.1, 0.0)), Some(ColliderDesc::new(Shape::cuboid(v3(5.0, 0.1, 2.0)))))).unwrap();
    let mut x = 0.0f32;
    (s, move |s: &mut Space| {
        x += DT;
        s.move_kinematic(5, Pose::at(v3(x, -0.1, 0.0)));
        s.step(DT);
    })
}

/// What comes to rest on a carrier that already moves is carried as what was there from the
/// start (the comparison of 2026-11-22: a ball that rolled onto the sliding bridge had none of
/// the bridge's speed, and was thrown back by twice it at the turn).
#[test]
fn a_ball_that_lands_on_a_carrier_already_moving_is_carried() {
    let (mut s, mut step) = sliding_carrier();
    for _ in 0..30 {
        step(&mut s);
    }
    dynamic(&mut s, 2, Shape::sphere(0.25), v3(1.0, 0.75, 0.0));
    for _ in 0..60 {
        step(&mut s);
    }
    let slab = |s: &Space| s.body(5).unwrap().pose.p.x;
    let off = s.body(2).unwrap().pose.p.x - slab(&s);
    for _ in 0..180 {
        step(&mut s);
    }
    let b = s.body(2).unwrap();
    assert!((b.pose.p.x - slab(&s) - off).abs() < 0.03 && (b.velocity.x - 1.0).abs() < 0.03, "carried from its landing: {} then {}, at {}", off, b.pose.p.x - slab(&s), b.velocity.x);
}

/// A ball that bounces on a carrier is given the carrier's speed once: it keeps it in the air.
#[test]
fn a_ball_that_bounces_on_a_carrier_is_given_its_speed_once() {
    let (mut s, mut step) = sliding_carrier();
    let mut ball = ColliderDesc::new(Shape::sphere(0.25));
    ball.restitution = 0.8;
    s.insert(2, BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(0.0, 1.25, 0.0)), Some(ball))).unwrap();
    let mut landings = 0;
    let mut falling = true;
    for _ in 0..600 {
        step(&mut s);
        let vy = s.body(2).unwrap().velocity.y;
        if falling && vy > 0.5 {
            landings += 1;
        }
        falling = vy <= 0.5;
    }
    let vx = s.body(2).unwrap().velocity.x;
    assert!(landings >= 3 && (vx - 1.0).abs() < 0.05, "after {landings} bounces the ball moves at {vx}");
}

/// A body that came off a carrier and rests on something else owes the carrier nothing: back on
/// it, it is given the carrier's speed at once and not left to friction. So is one told to be at
/// rest (a ball put back after a fall).
#[test]
fn what_left_a_carrier_or_was_put_at_rest_is_carried_afresh() {
    let (mut s, mut step) = sliding_carrier();
    dynamic(&mut s, 2, Shape::cuboid(v3(0.25, 0.25, 0.25)), v3(0.0, 0.25, 0.0));
    for _ in 0..60 {
        step(&mut s);
    }
    assert!((s.body(2).unwrap().velocity.x - 1.0).abs() < 0.03, "the box rides");
    // Onto the still floor, with the speed it had: friction stops it there.
    s.set_pose(2, Pose::at(v3(98.0, 0.25, 0.0)));
    for _ in 0..120 {
        step(&mut s);
    }
    assert!(s.body(2).unwrap().velocity.x.abs() < 0.03, "the box stopped on the floor");
    // Back on the carrier: within two steps it has the carrier's speed (friction alone gives a
    // fifth of it in that time).
    let on_slab = |s: &Space| Pose::at(v3(s.body(5).unwrap().pose.p.x, 0.25, 0.0));
    let at = on_slab(&s);
    s.set_pose(2, at);
    for _ in 0..3 {
        step(&mut s);
    }
    assert!((s.body(2).unwrap().velocity.x - 1.0).abs() < 0.05, "given the speed afresh: {}", s.body(2).unwrap().velocity.x);
    // Put at rest where it is (it never left the carrier): carried again, at once.
    let at = on_slab(&s);
    s.set_pose(2, at);
    s.set_velocity(2, Vec3::ZERO, Vec3::ZERO);
    for _ in 0..3 {
        step(&mut s);
    }
    assert!((s.body(2).unwrap().velocity.x - 1.0).abs() < 0.05, "put at rest and carried: {}", s.body(2).unwrap().velocity.x);
}

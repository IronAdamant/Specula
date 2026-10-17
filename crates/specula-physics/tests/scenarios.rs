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
        s.insert(10 + i, BodyDesc::new(BodyKind::Static, Pose::at(v3(2.0 + 0.5 * i as f32, 0.1 + 0.1 * i as f32, 0.0)), Some(step))).unwrap();
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

//! Joint scenarios (P4, docs/design/PHYSICS.md): hinges with limits and motors, chains, ball
//! joints with cones and twist limits, ropes, rods and springs, sliders, welds, sleeping,
//! determinism, and cosmetic bodies that never touch the simulation.

use specula_core::math::{Quat, Vec3};
use specula_physics::joint::{frame, Drive, JointDesc, JointKind, Motor};
use specula_physics::{BodyDesc, BodyKind, ColliderDesc, Pose, Shape, Space};
use std::f32::consts::PI;

const DT: f32 = 1.0 / 60.0;

fn v3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn ground(s: &mut Space) {
    let col = ColliderDesc::new(Shape::cuboid(v3(50.0, 0.5, 50.0)));
    s.insert(1, BodyDesc::new(BodyKind::Static, Pose::at(v3(0.0, -0.5, 0.0)), Some(col))).unwrap();
}

fn body(s: &mut Space, key: u64, shape: Shape, at: Vec3) {
    s.insert(key, BodyDesc::new(BodyKind::Dynamic, Pose::at(at), Some(ColliderDesc::new(shape)))).unwrap();
}

/// A joint between `a` (or the world) and `b` at world `anchor` with world `axis`, frames
/// computed from the bodies' current poses (what the engine's tool does).
fn joint(s: &Space, kind: JointKind, a: Option<u64>, b: u64, anchor: Vec3, axis: Vec3) -> JointDesc {
    let world = frame(anchor, axis, if axis.y.abs() > 0.9 { Vec3::X } else { Vec3::Y });
    let local = |k: Option<u64>| match k {
        None => world,
        Some(k) => s.body(k).unwrap().pose.inv_mul(&world),
    };
    JointDesc::new(kind, a, Some(b), local(a), local(Some(b)))
}

fn deg(r: f32) -> f32 {
    r * 180.0 / PI
}

#[test]
fn a_hinged_bar_swings_on_its_axis_and_holds_together() {
    let mut s = Space::default();
    body(&mut s, 2, Shape::cuboid(v3(0.5, 0.05, 0.05)), v3(0.5, 2.0, 0.0));
    let j = joint(&s, JointKind::Hinge, None, 2, v3(0.0, 2.0, 0.0), Vec3::Z);
    s.insert_joint(10, j).unwrap();
    let mut lowest = f32::MAX;
    let mut worst = (0.0f32, 0.0f32);
    for _ in 0..240 {
        s.step(DT);
        let st = s.joint(10).unwrap().state;
        assert!(st.active, "{st:?}");
        worst = (worst.0.max(st.linear_error), worst.1.max(st.angular_error));
        lowest = lowest.min(s.body(2).unwrap().pose.p.y);
    }
    let p = s.body(2).unwrap().pose;
    // It swings about Z only: it stays in its plane, its centre on the 0.5 m circle.
    assert!(p.p.z.abs() < 1e-3, "out of plane: {:?}", p.p);
    assert!(((p.p - v3(0.0, 2.0, 0.0)).length() - 0.5).abs() < 0.01, "{:?}", p.p);
    assert!(lowest < 1.55, "it swung down: lowest {lowest}");
    assert!(worst.0 < 0.01 && worst.1 < 0.01, "joint errors {worst:?}");
}

#[test]
fn hinge_limits_hold_and_motors_drive() {
    // A door: hinged at the world, limited to 0..90 degrees, pushed by a speed motor.
    let mut s = Space::default();
    body(&mut s, 2, Shape::cuboid(v3(0.5, 1.0, 0.05)), v3(0.5, 1.0, 0.0));
    s.settings.gravity = Vec3::ZERO;
    let mut j = joint(&s, JointKind::Hinge, None, 2, v3(0.0, 1.0, 0.0), Vec3::Y);
    j.limit = Some([0.0, PI / 2.0]);
    j.motor = Some(Motor { drive: Drive::Speed(2.0), max_force: 50.0, hertz: 0.0, damping_ratio: 0.0 });
    s.insert_joint(10, j.clone()).unwrap();
    let mut over = 0.0f32;
    for _ in 0..120 {
        s.step(DT);
        over = over.max(s.joint(10).unwrap().state.limit_excess);
    }
    let st = s.joint(10).unwrap().state;
    assert!((st.position - PI / 2.0).abs() < 0.02, "rests open at the upper limit: {} deg", deg(st.position));
    assert!(deg(over) < 1.0, "never more than 1 degree past the limit: {}", deg(over));
    // A position drive takes it to 30 degrees and holds it there.
    j.motor = Some(Motor { drive: Drive::Position(PI / 6.0), max_force: 500.0, hertz: 4.0, damping_ratio: 1.0 });
    s.set_joint_motor(10, j.motor).unwrap();
    for _ in 0..180 {
        s.step(DT);
    }
    let st = s.joint(10).unwrap().state;
    assert!((deg(st.position) - 30.0).abs() < 1.0, "position drive: {} deg", deg(st.position));
    // And it holds still there.
    let w = s.body(2).unwrap().angular_velocity;
    assert!(w.length() < 0.05, "settled: {w:?}");
}

#[test]
fn a_ball_joint_chain_hangs_settles_and_sleeps() {
    let mut s = Space::default();
    ground(&mut s);
    let n = 10u64;
    for i in 0..n {
        // Real chains lose energy at their links: a little damping lets it come to rest.
        let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(0.4 * (i as f32 + 0.5), 5.0, 0.0)), Some(ColliderDesc::new(Shape::capsule(0.05, 0.15))));
        (d.linear_damping, d.angular_damping) = (1.0, 1.5);
        s.insert(100 + i, d).unwrap();
    }
    for i in 0..n {
        let a = if i == 0 { None } else { Some(99 + i) };
        let j = joint(&s, JointKind::Ball, a, 100 + i, v3(0.4 * i as f32, 5.0, 0.0), Vec3::X);
        s.insert_joint(1000 + i, j).unwrap();
    }
    let mut worst = 0.0f32;
    for t in 0..1200 {
        s.step(DT);
        if t > 60 {
            worst = worst.max(s.joints().map(|j| j.state.linear_error).fold(0.0, f32::max));
        }
    }
    let tip = s.body(100 + n - 1).unwrap().pose.p;
    assert!(tip.x.abs() < 0.2 && tip.y < 5.0 - 3.5, "hangs straight down: {tip:?}");
    assert!(worst < 0.02, "links stay together: worst gap {worst} m");
    // At rest the whole chain falls asleep, together.
    assert!((0..n).all(|i| s.body(100 + i).unwrap().sleeping), "the chain sleeps");
    // Kicking the last link wakes every link through the joints.
    s.apply_impulse(100 + n - 1, v3(0.0, 0.0, 0.5), tip);
    s.step(DT);
    assert!((0..n).all(|i| !s.body(100 + i).unwrap().sleeping), "the chain wakes together");
}

#[test]
fn ball_joints_keep_their_cone_and_twist_limits() {
    // A limb hanging from a shoulder, released far outside a 30 degree cone.
    let mut s = Space::default();
    s.settings.gravity = v3(0.0, -9.81, 0.0);
    body(&mut s, 2, Shape::capsule(0.06, 0.25), v3(0.35, 3.0, 0.0));
    let mut j = joint(&s, JointKind::Ball, None, 2, v3(0.0, 3.0, 0.0), v3(0.0, -1.0, 0.0));
    // The frame's axis points down; the arm starts horizontal (90 degrees of swing).
    j.frame_b = s.body(2).unwrap().pose.inv_mul(&frame(v3(0.0, 3.0, 0.0), v3(1.0, 0.0, 0.0), Vec3::Y));
    j.swing = Some([30f32.to_radians(), 30f32.to_radians()]);
    j.limit = Some([-10f32.to_radians(), 10f32.to_radians()]);
    s.insert_joint(10, j).unwrap();
    s.apply_impulse(2, v3(0.0, 0.0, 0.0), v3(0.0, 0.0, 0.0));
    s.step(DT);
    let first = s.joint(10).unwrap().state;
    for _ in 0..240 {
        s.step(DT);
    }
    let st = s.joint(10).unwrap().state;
    assert!(deg(first.swing) > 30.0, "it started outside the cone: {}", deg(first.swing));
    assert!(deg(st.swing) < 31.5, "pulled back inside the cone: {} deg", deg(st.swing));
    assert!(deg(st.position).abs() < 11.0, "twist within its range: {} deg", deg(st.position));
    assert!(st.linear_error < 0.01, "{st:?}");
}

#[test]
fn ropes_rods_and_springs_keep_their_lengths() {
    let mut s = Space::default();
    // Rope: 2 m long, the body starts 1 m below and falls until it's taut.
    body(&mut s, 2, Shape::sphere(0.1), v3(0.0, 4.0, 0.0));
    let mut rope = JointDesc::new(JointKind::Distance, None, Some(2), Pose::at(v3(0.0, 5.0, 0.0)), Pose::IDENTITY);
    rope.length = [0.0, 2.0];
    s.insert_joint(10, rope).unwrap();
    // Rod: exactly 1 m, starting sideways (a pendulum).
    body(&mut s, 3, Shape::sphere(0.1), v3(4.0, 5.0, 0.0));
    let mut rod = JointDesc::new(JointKind::Distance, None, Some(3), Pose::at(v3(3.0, 5.0, 0.0)), Pose::IDENTITY);
    rod.length = [1.0, 1.0];
    s.insert_joint(11, rod).unwrap();
    // Spring: rest length 1 m, free between 0.2 and 3 m.
    body(&mut s, 4, Shape::sphere(0.1), v3(-3.0, 4.0, 0.0));
    let mut spring = JointDesc::new(JointKind::Distance, None, Some(4), Pose::at(v3(-3.0, 5.0, 0.0)), Pose::IDENTITY);
    spring.length = [0.2, 3.0];
    spring.rest_length = 1.0;
    spring.spring_hertz = 2.0;
    spring.spring_damping = 0.5;
    s.insert_joint(12, spring).unwrap();
    let (mut rope_max, mut rod_err) = (0.0f32, 0.0f32);
    for _ in 0..300 {
        s.step(DT);
        rope_max = rope_max.max(s.joint(10).unwrap().state.position);
        rod_err = rod_err.max((s.joint(11).unwrap().state.position - 1.0).abs());
    }
    assert!(rope_max < 2.02 && rope_max > 1.9, "the rope goes taut and holds: {rope_max}");
    assert!(rod_err < 0.01, "the rod keeps 1 m: {rod_err}");
    // The spring settles where its pull balances gravity (rest + g / ω², about 1.06 m).
    let len = s.joint(12).unwrap().state.position;
    let omega = 2.0 * PI * 2.0;
    assert!((len - (1.0 + 9.81 / (omega * omega))).abs() < 0.05, "spring length {len}");
}

#[test]
fn sliders_slide_on_their_axis_within_limits() {
    let mut s = Space::default();
    body(&mut s, 2, Shape::cuboid(v3(0.2, 0.2, 0.2)), v3(0.0, 3.0, 0.0));
    // A 45 degree rail in the XY plane; the block may slide 1 m down from where it starts.
    let axis = v3(1.0, -1.0, 0.0).normalize();
    let mut j = joint(&s, JointKind::Prismatic, None, 2, v3(0.0, 3.0, 0.0), axis);
    j.limit = Some([-0.2, 1.0]);
    s.insert_joint(10, j).unwrap();
    for _ in 0..240 {
        s.step(DT);
    }
    let p = s.body(2).unwrap().pose;
    let st = s.joint(10).unwrap().state;
    assert!((st.position - 1.0).abs() < 0.02, "stopped at the end of the rail: {}", st.position);
    assert!(st.linear_error < 0.005 && st.angular_error < 0.01, "on the rail, not turning: {st:?}");
    assert!((p.p.x - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.03 && (p.p.y - (3.0 - std::f32::consts::FRAC_1_SQRT_2)).abs() < 0.03, "{:?}", p.p);
}

#[test]
fn welded_bodies_fall_as_one_and_jointed_pairs_ignore_each_other() {
    let mut s = Space::default();
    ground(&mut s);
    // Two overlapping boxes welded together: without the joint they would push apart.
    body(&mut s, 2, Shape::cuboid(v3(0.3, 0.3, 0.3)), v3(0.0, 2.0, 0.0));
    body(&mut s, 3, Shape::cuboid(v3(0.3, 0.3, 0.3)), v3(0.4, 2.2, 0.0));
    let j = joint(&s, JointKind::Fixed, Some(2), 3, v3(0.2, 2.1, 0.0), Vec3::X);
    s.insert_joint(10, j).unwrap();
    let rel0 = s.body(2).unwrap().pose.inv_mul(&s.body(3).unwrap().pose);
    for _ in 0..240 {
        s.step(DT);
    }
    let rel = s.body(2).unwrap().pose.inv_mul(&s.body(3).unwrap().pose);
    assert!((rel.p - rel0.p).length() < 0.01, "kept their offset: {:?} vs {:?}", rel.p, rel0.p);
    assert!(s.body(2).unwrap().pose.p.y < 1.0, "fell to the ground");
    assert!(!s.touching(2).contains(&3), "a jointed pair doesn't collide");
    // Removing the joint lets them collide (and push apart) again.
    s.remove_joint(10);
    for _ in 0..60 {
        s.step(DT);
    }
    let gap = (s.body(3).unwrap().pose.p - s.body(2).unwrap().pose.p).length();
    assert!(gap > 0.55, "pushed apart once free: {gap}");
}

#[test]
fn a_rotation_drive_holds_a_limb_against_gravity() {
    // A powered shoulder: the drive holds the arm out horizontally (90 degrees from hanging).
    let mut s = Space::default();
    body(&mut s, 2, Shape::capsule(0.06, 0.25), v3(0.0, 2.65, 0.0));
    let mut j = joint(&s, JointKind::Ball, None, 2, v3(0.0, 3.0, 0.0), v3(0.0, -1.0, 0.0));
    let target = Quat::from_axis_angle(Vec3::Z, PI / 2.0);
    j.motor = Some(Motor { drive: Drive::Rotation(target), max_force: 200.0, hertz: 8.0, damping_ratio: 1.0 });
    s.insert_joint(10, j).unwrap();
    for _ in 0..240 {
        s.step(DT);
    }
    let st = s.joint(10).unwrap().state;
    assert!((deg(st.swing) - 90.0).abs() < 5.0, "held out: swing {} deg", deg(st.swing));
    // A weak drive sags: max_force caps the torque.
    let mut weak = s.joint(10).unwrap().desc.clone();
    weak.motor = Some(Motor { drive: Drive::Rotation(target), max_force: 0.5, hertz: 8.0, damping_ratio: 1.0 });
    s.set_joint_motor(10, weak.motor).unwrap();
    for _ in 0..240 {
        s.step(DT);
    }
    assert!(deg(s.joint(10).unwrap().state.swing) < 60.0, "a weak drive can't hold it");
}

/// A scene of falling hinged plates (the "falling hinges" determinism scene).
fn hinges(order: &[u64]) -> Space {
    let mut s = Space::default();
    ground(&mut s);
    for &i in order {
        let x = (i % 5) as f32 * 1.3 - 2.6;
        let z = (i / 5) as f32 * 1.3 - 2.6;
        let y = 1.0 + (i % 3) as f32 * 0.4;
        body(&mut s, 100 + 2 * i, Shape::cuboid(v3(0.25, 0.05, 0.4)), v3(x, y, z));
        body(&mut s, 101 + 2 * i, Shape::cuboid(v3(0.25, 0.05, 0.4)), v3(x + 0.5, y + 0.1, z));
    }
    for &i in order {
        let x = (i % 5) as f32 * 1.3 - 2.6;
        let z = (i / 5) as f32 * 1.3 - 2.6;
        let y = 1.0 + (i % 3) as f32 * 0.4;
        let mut j = joint(&s, JointKind::Hinge, Some(100 + 2 * i), 101 + 2 * i, v3(x + 0.25, y + 0.05, z), Vec3::Z);
        j.limit = Some([-1.0, 1.0]);
        s.insert_joint(5000 + i, j).unwrap();
        s.apply_impulse(101 + 2 * i, v3(0.0, 0.3, 0.1 * (i % 2) as f32), v3(x + 0.6, y, z));
    }
    s
}

#[test]
fn falling_hinges_are_deterministic_in_any_insertion_order() {
    let forward: Vec<u64> = (0..25).collect();
    let backward: Vec<u64> = (0..25).rev().collect();
    let (mut a, mut b) = (hinges(&forward), hinges(&backward));
    for t in 0..300 {
        a.step(DT);
        b.step(DT);
        assert_eq!(a.hash(), b.hash(), "diverged at step {t}");
    }
    assert_eq!(a.hash(), a.hash_uncached(), "the cached hash covers joints");
    // Settled and asleep, with the hinges within their limits.
    for j in a.joints() {
        assert!(j.state.limit_excess < 0.03, "{} past its limit by {}", j.key, j.state.limit_excess);
    }
}

#[test]
fn cosmetic_bodies_never_change_the_simulation() {
    let build = |cosmetic: bool| {
        let mut s = Space::default();
        ground(&mut s);
        for i in 0..6u64 {
            body(&mut s, 10 + i, Shape::cuboid(v3(0.3, 0.3, 0.3)), v3(i as f32 * 0.25 - 0.6, 0.6 + i as f32 * 0.7, 0.0));
        }
        if cosmetic {
            // A cosmetic ragdoll-ish chain right where the boxes fall.
            for i in 0..5u64 {
                let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(i as f32 * 0.3 - 0.6, 1.5, 0.0)), Some(ColliderDesc::new(Shape::capsule(0.1, 0.1))));
                d.cosmetic = true;
                d.group = 7;
                s.insert(1_000_000 + i, d).unwrap();
            }
            for i in 1..5u64 {
                let j = joint(&s, JointKind::Ball, Some(999_999 + i), 1_000_000 + i, v3(i as f32 * 0.3 - 0.75, 1.5, 0.0), Vec3::X);
                s.insert_joint(2_000_000 + i, j).unwrap();
            }
        }
        s
    };
    let (mut plain, mut with) = (build(false), build(true));
    for t in 0..240 {
        let ea = format!("{:?}", plain.step(DT).events);
        let eb = format!("{:?}", with.step(DT).events);
        // Review finding 10: nor its events (the cosmetic chain lands on the ground, quietly).
        assert_eq!(ea, eb, "events differ at step {t}");
        assert_eq!(plain.hash(), with.hash(), "the simulation changed at step {t}");
        for i in 0..6u64 {
            assert_eq!(plain.body(10 + i).unwrap().pose, with.body(10 + i).unwrap().pose);
        }
    }
    // The cosmetic chain did fall onto the ground (it collides with static bodies)...
    assert!(with.body(1_000_002).unwrap().pose.p.y < 0.4, "{:?}", with.body(1_000_002).unwrap().pose.p);
    // ...and queries don't see it.
    let (hits, _) = with.raycast(v3(-0.3, 5.0, 0.0), v3(0.0, -1.0, 0.0), 10.0, &Default::default(), 8);
    assert!(hits.iter().all(|h| h.key < 1_000_000), "{hits:?}");
}

#[test]
fn joints_wait_for_missing_bodies_and_refuse_bad_input() {
    let mut s = Space::default();
    let j = JointDesc::new(JointKind::Ball, Some(2), Some(3), Pose::IDENTITY, Pose::IDENTITY);
    s.insert_joint(10, j).unwrap();
    s.step(DT);
    assert_eq!(s.joint(10).unwrap().state.inactive, Some("missing_body"));
    body(&mut s, 2, Shape::sphere(0.1), v3(0.0, 1.0, 0.0));
    body(&mut s, 3, Shape::sphere(0.1), v3(0.0, 1.0, 0.0));
    s.step(DT);
    assert!(s.joint(10).unwrap().state.active);
    let mut bad = JointDesc::new(JointKind::Hinge, Some(2), Some(3), Pose::IDENTITY, Pose::IDENTITY);
    bad.limit = Some([2.0, 1.0]);
    let e = s.insert_joint(11, bad).unwrap_err();
    assert_eq!((e.code, e.details.iter().any(|(k, _)| *k == "field")), ("invalid_joint", true));
    s.settings.max_joints = 1;
    let e = s.insert_joint(12, JointDesc::new(JointKind::Ball, Some(2), None, Pose::IDENTITY, Pose::IDENTITY)).unwrap_err();
    assert_eq!(e.code, "budget");
}

/// Review finding 4 (probe "rope"): a rope tied to a box asleep 7 m below its reach used to stop
/// pulling halfway (the correction lives in the biased pass, so the box reported rest and fell
/// asleep in mid-air, 2.26 m off). Bodies of a joint that doesn't hold stay awake until it does,
/// and the rope's over-stretch counts as past its limits.
#[test]
fn a_rope_added_to_a_sleeping_body_pulls_it_all_the_way() {
    let mut s = Space::default();
    ground(&mut s);
    let mut d = BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(0.0, 0.5, 0.0)), Some(ColliderDesc::new(Shape::cuboid(v3(0.5, 0.5, 0.5)))));
    d.mass = 5.0;
    s.insert(2, d).unwrap();
    for _ in 0..30 {
        s.step(1.0 / 30.0);
    }
    let mut j = JointDesc::new(JointKind::Distance, None, Some(2), Pose::at(v3(0.0, 8.0, 0.0)), Pose::IDENTITY);
    j.length = [0.0, 0.5];
    s.insert_joint(10, j).unwrap();
    s.step(1.0 / 30.0);
    assert!(s.joint(10).unwrap().state.limit_excess > 6.0, "over-stretched counts as past the limit: {:?}", s.joint(10).unwrap().state);
    for _ in 0..150 {
        s.step(1.0 / 30.0);
    }
    let b = s.body(2).unwrap();
    let st = s.joint(10).unwrap().state;
    assert!(b.pose.p.y > 7.4 && st.linear_error < 0.02, "hangs from the rope: y {} error {}", b.pose.p.y, st.linear_error);
}

/// Review finding 7 (probe "wrap"): a hinge position target of 1.3e9 rad never finished a step.
#[test]
fn huge_hinge_targets_wrap_in_one_go() {
    let mut s = Space::default();
    s.insert(2, BodyDesc::new(BodyKind::Dynamic, Pose::at(v3(0.5, 2.0, 0.0)), Some(ColliderDesc::new(Shape::cuboid(v3(0.5, 0.05, 0.05)))))).unwrap();
    let w = frame(v3(0.0, 2.0, 0.0), Vec3::Z, Vec3::Y);
    let lb = s.body(2).unwrap().pose.inv_mul(&w);
    for target in [1.3e9f32, -4.0e12, f32::MAX] {
        let mut j = JointDesc::new(JointKind::Hinge, None, Some(2), w, lb);
        j.motor = Some(Motor { drive: Drive::Position(target), max_force: 10.0, hertz: 5.0, damping_ratio: 1.0 });
        s.insert_joint(10, j).unwrap();
        s.step(DT);
    }
    let w = specula_physics::joint::wrap(1.3e9);
    assert!(w.abs() <= PI, "{w}");
}

//! The limits: refused before any work, with the number named; and a forest of 20,000 things
//! steps within the stated work bound, counted in operations (never timed).

mod common;

use common::{forest, forest_centre, mat, sim};
use specula_core::math::Vec3;
use specula_elementa::grid::MAX_NODE_CELLS;
use specula_elementa::sim::{MAX_ACTS_PER_TICK, MAX_ACT_RADIUS};
use specula_elementa::{Act, ActKind, NodeDesc, Phase, Target};

fn num(e: &specula_elementa::ElementaError, key: &str) -> u64 {
    e.detail(key).and_then(|v| v.as_u64()).unwrap_or_else(|| panic!("{key} in {e:?}"))
}

#[test]
fn the_node_limit_refuses_and_names_its_number() {
    let mut e = sim(r#"{"elements": 1, "limits": {"nodes": 10, "burning": 5}}"#, 1);
    let wood = mat(&e, "wood");
    for id in 0..10 {
        e.add(NodeDesc::new(id, wood, Vec3::new(id as f32 * 3.0, 0.0, 0.0), Vec3::ONE)).unwrap();
    }
    let err = e.add(NodeDesc::new(10, wood, Vec3::ZERO, Vec3::ONE)).unwrap_err();
    assert_eq!(err.code, "limit");
    assert_eq!(num(&err, "max"), 10);
    assert_eq!(err.detail("limit").and_then(|v| v.as_str()), Some("nodes"));
    assert_eq!(e.nodes().len(), 10);
    assert_eq!(e.refused(), 1);
    let dup = e.add(NodeDesc::new(3, wood, Vec3::ZERO, Vec3::ONE)).unwrap_err();
    assert_eq!(dup.code, "duplicate");

    // A batch past the default limit of 4,096: the highest ids are left out and counted.
    let mut f = sim(r#"{"elements": 1}"#, 1);
    let trees = forest(&f, 20_000, 1);
    let r = f.add_many(&trees).unwrap();
    assert_eq!((r.added, r.left_out, r.limit), (4096, 15_904, 4096));
    assert_eq!(f.refused(), 15_904);
    assert_eq!(f.nodes().last().unwrap().id, 4096, "the lowest ids are kept");
    assert!(f.info().to_string().contains("\"refused\":15904"));
}

#[test]
fn bad_nodes_and_acts_are_refused_before_any_change() {
    let mut e = sim(r#"{"elements": 1, "cell_m": 2}"#, 1);
    let wood = mat(&e, "wood");
    let err = e.add(NodeDesc::new(1, wood, Vec3::ZERO, Vec3::new(10.0, 10.0, 10.0))).unwrap_err();
    assert_eq!(err.code, "too_large");
    assert!(num(&err, "cells") > u64::from(MAX_NODE_CELLS));
    assert_eq!(num(&err, "max"), u64::from(MAX_NODE_CELLS));
    // A tall, thin tree fits: 1 x 10 x 1 m covers at most 2 x 6 x 2 cells of 2 m.
    e.add(NodeDesc::new(2, wood, Vec3::new(0.5, 5.0, 0.5), Vec3::new(0.5, 5.0, 0.5))).unwrap();
    for bad in [
        NodeDesc::new(3, 99, Vec3::ZERO, Vec3::ONE),
        NodeDesc::new(3, wood, Vec3::new(f32::NAN, 0.0, 0.0), Vec3::ONE),
        NodeDesc::new(3, wood, Vec3::new(2.0e6, 0.0, 0.0), Vec3::ONE),
        NodeDesc::new(3, wood, Vec3::ZERO, Vec3::new(-1.0, 1.0, 1.0)),
        NodeDesc::new(3, wood, Vec3::ZERO, Vec3::ONE).heat(2.0),
        NodeDesc::new(3, wood, Vec3::ZERO, Vec3::ONE).wet(-0.1),
        NodeDesc::new(3, wood, Vec3::ZERO, Vec3::ONE).fuel(1.0e9),
    ] {
        assert!(e.add(bad.clone()).is_err(), "{bad:?}");
        assert!(e.add_many(&[NodeDesc::new(4, wood, Vec3::ZERO, Vec3::ONE), bad]).is_err());
    }
    assert_eq!(e.nodes().len(), 1, "a refused batch adds nothing");
    assert!(e.set_wind(Vec3::new(100.0, 0.0, 0.0)).is_err());
    assert!(e.set_ambient(1.5).is_err());
    assert_eq!(e.move_node(77, Vec3::ZERO, Vec3::ONE).unwrap_err().code, "unknown_node");

    let sphere = |radius| Act { kind: ActKind::Ignite, target: Target::Sphere { at: Vec3::ZERO, radius }, amount: 1.0 };
    assert_eq!(e.act(sphere(MAX_ACT_RADIUS + 1.0)).unwrap_err().code, "out_of_range");
    assert_eq!(e.act(Act { kind: ActKind::Chill, target: Target::Node(2), amount: 3.0 }).unwrap_err().code, "out_of_range");
    for _ in 0..MAX_ACTS_PER_TICK {
        e.act(sphere(1.0)).unwrap();
    }
    let err = e.act(sphere(1.0)).unwrap_err();
    assert_eq!((err.code, num(&err, "max")), ("limit", u64::from(MAX_ACTS_PER_TICK)));
    e.step();
    assert!(e.act(sphere(1.0)).is_ok(), "the count starts again each tick");
    // An act's work is at most the node count, however large its sphere.
    let r = e.act(sphere(MAX_ACT_RADIUS)).unwrap();
    assert!(r.work as usize <= e.nodes().len().max(1), "{r:?}");
}

#[test]
fn the_burning_limit_holds_and_the_rest_wait_in_id_order() {
    let mut e = sim(r#"{"elements": 1, "limits": {"nodes": 200, "burning": 10}}"#, 1);
    let wood = mat(&e, "wood");
    for id in 0..100u64 {
        e.add(NodeDesc::new(id, wood, Vec3::new((id % 10) as f32 * 3.0, 0.0, (id / 10) as f32 * 3.0), Vec3::ONE)).unwrap();
    }
    let r = e.act(Act { kind: ActKind::Ignite, target: Target::Sphere { at: Vec3::new(13.5, 0.0, 13.5), radius: 30.0 }, amount: 1.0 }).unwrap();
    assert_eq!(r.nodes, 100);
    for _ in 0..6 {
        e.step();
    }
    let rep = *e.last_report();
    assert_eq!((rep.burning, rep.waiting), (10, 90));
    let lit: Vec<u64> = e.nodes().iter().filter(|n| n.phase == Phase::Burning).map(|n| n.id).collect();
    assert_eq!(lit, (0..10).collect::<Vec<_>>(), "the lowest ids catch first");
    for _ in 0..600 {
        assert!(e.step().burning <= 10);
    }
}

#[test]
fn a_forest_of_20000_steps_within_the_stated_work_bound() {
    let json = r#"{"elements": 1, "reach_m": 1.0, "limits": {"nodes": 20000, "burning": 512, "work": 32768}}"#;
    let mut e = sim(json, 7);
    let trees = forest(&e, 20_000, 1);
    assert_eq!(e.add_many(&trees).unwrap().added, 20_000);
    e.set_wind(Vec3::new(4.0, 0.0, 1.0)).unwrap();
    let worst = e.config().worst_case();
    assert_eq!(worst.ops_per_step, 20_000 + 32_768);
    e.act(Act { kind: ActKind::Ignite, target: Target::Sphere { at: forest_centre(20_000), radius: 4.0 }, amount: 1.0 }).unwrap();
    let (mut steps, mut cut, mut waited, mut ignited, mut peak_work, mut entries, mut total_ops) = (0, 0, 0, 0u64, 0, 0, 0u64);
    let (mut processed, mut work) = (0u64, 0u64);
    for _ in 0..60 * 120 {
        let r = *e.step();
        if !r.spread {
            assert_eq!(r.ops(), 0, "nothing is done between spread steps");
            continue;
        }
        steps += 1;
        assert!(r.work <= 32_768, "step at tick {}: work {}", r.tick, r.work);
        assert_eq!(r.work, r.lookups + r.checks);
        assert!(r.burning <= 512, "{} burning", r.burning);
        assert_eq!(r.node_updates, 20_000);
        assert!(r.ops() <= worst.ops_per_step);
        cut += u32::from(r.deferred > 0 || r.truncated > 0);
        waited += u32::from(r.waiting > 0);
        peak_work = peak_work.max(r.work);
        entries = entries.max(r.grid_entries);
        total_ops += r.ops();
        processed += u64::from(r.processed);
        work += u64::from(r.work);
        ignited += e.take_events().count(specula_elementa::EventKind::Ignited);
    }
    assert_eq!(steps, 1200);
    assert!(ignited > 2_000, "the fire spread: {ignited} caught");
    assert!(cut > 0 || waited > 0, "the limits bit at least once (cut {cut}, waited {waited})");
    assert!(u64::from(entries) <= 20_000 * u64::from(MAX_NODE_CELLS));
    // The figures the design note quotes for 20,000 things (printed, counted; not timed).
    println!(
        "forest 20000: grid entries {entries} ({:.2} a node), peak work {peak_work}, steps cut {cut}, steps waiting {waited}, \
         caught {ignited}, ops over 120 s {total_ops} ({} a step on average), work per burning node heated {:.1}",
        f64::from(entries) / 20_000.0,
        total_ops / steps as u64,
        work as f64 / processed.max(1) as f64
    );
}

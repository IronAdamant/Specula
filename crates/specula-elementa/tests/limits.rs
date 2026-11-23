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
    assert!(r.work as usize <= 2 * e.nodes().len() + 1, "{r:?}");
}

#[test]
fn the_burning_limit_holds_and_the_longest_waiting_catch_first() {
    let mut e = sim(r#"{"elements": 1, "limits": {"nodes": 200, "burning": 10}}"#, 1);
    let wood = mat(&e, "wood");
    // A hundred short-lived logs (half a second of fuel), 3 m apart so none lights another.
    for id in 1..=100u64 {
        let (x, z) = (((id - 1) % 10) as f32 * 3.0, ((id - 1) / 10) as f32 * 3.0);
        e.add(NodeDesc::new(id, wood, Vec3::new(x, 0.0, z), Vec3::ONE).fuel(0.5)).unwrap();
    }
    // One more, with the lowest id of all, far off.
    e.add(NodeDesc::new(0, wood, Vec3::new(-50.0, 0.0, 0.0), Vec3::ONE).fuel(0.5)).unwrap();
    let r = e.act(Act { kind: ActKind::Ignite, target: Target::Sphere { at: Vec3::new(13.5, 0.0, 13.5), radius: 30.0 }, amount: 1.0 }).unwrap();
    assert_eq!(r.nodes, 100);
    let mut lit_at = std::collections::BTreeMap::new();
    for t in 0..60 * 10 {
        if t == 12 {
            // Ready two steps after the hundred: it waits behind every one of them.
            e.act(Act { kind: ActKind::Ignite, target: Target::Node(0), amount: 1.0 }).unwrap();
        }
        let rep = *e.step();
        assert!(rep.burning <= 10);
        if rep.tick == 6 {
            assert_eq!((rep.burning, rep.waiting, rep.ordered), (10, 90, 100));
        }
        for ev in e.take_events().list {
            if ev.kind == specula_elementa::EventKind::Ignited {
                lit_at.insert(ev.id, ev.tick);
            }
        }
    }
    assert_eq!(lit_at.len(), 101, "every one caught in its turn");
    let last_of_batch = (1..=100).map(|id| lit_at[&id]).max().unwrap();
    assert!(lit_at[&0] >= last_of_batch, "id 0 became ready last and caught last ({} against {last_of_batch}), whatever its id", lit_at[&0]);
}

/// Burnt or burning nodes of a square grass field after 30 s, below and above the row it was
/// lit on (z), with the ids laid out rising or falling with z.
fn field(ids_fall_with_z: bool) -> (u32, u32) {
    let json = r#"{"elements": 1, "reach_m": 1.0, "limits": {"nodes": 20000, "burning": 64, "work": 32768}}"#;
    let mut e = sim(json, 7);
    let grass = mat(&e, "grass");
    let cols = 61usize;
    let mut d = Vec::new();
    for i in 0..cols * cols {
        let (x, z) = (i % cols, i / cols);
        let row = if ids_fall_with_z { cols - 1 - z } else { z };
        d.push(NodeDesc::new((row * cols + x) as u64, grass, Vec3::new(x as f32, 0.2, z as f32), Vec3::new(0.3, 0.2, 0.3)));
    }
    e.add_many(&d).unwrap();
    // The wind is along x, so nothing in the physics tells +z from -z.
    e.set_wind(Vec3::new(3.0, 0.0, 0.0)).unwrap();
    e.act(Act { kind: ActKind::Ignite, target: Target::Sphere { at: Vec3::new(30.0, 0.2, 30.0), radius: 1.5 }, amount: 1.0 }).unwrap();
    let mut waited = 0;
    for _ in 0..60 * 30 {
        waited += u32::from(e.step().waiting > 0);
    }
    assert!(waited > 100, "the burning limit bit ({waited} steps)");
    let (mut below, mut above) = (0, 0);
    for n in e.nodes().iter().filter(|n| n.phase != Phase::Intact) {
        if n.center.z < 30.0 {
            below += 1;
        } else if n.center.z > 30.0 {
            above += 1;
        }
    }
    (below, above)
}

#[test]
fn under_the_burning_limit_the_fire_does_not_follow_the_ids() {
    let rising = field(false);
    let falling = field(true);
    println!("ids rising with z: below {} above {}; falling: below {} above {}", rising.0, rising.1, falling.0, falling.1);
    for (below, above) in [rising, falling] {
        let (lo, hi) = (below.min(above), below.max(above));
        assert!(lo * 10 >= hi * 8, "the fire is lopsided along z, where nothing physical differs: {below} below, {above} above");
    }
    let (a, b) = (rising.0 + rising.1, falling.0 + falling.1);
    assert!(a.abs_diff(b) * 20 <= a.max(b), "{a} against {b} caught");
}

#[test]
fn a_crowded_cell_is_heated_whole_across_steps() {
    // 2,000 tufts in one 2 m cell: one neighbour query is about twice the smallest work budget.
    let mut e = sim(r#"{"elements":1, "cell_m": 2, "reach_m": 1, "limits": {"nodes": 4096, "burning": 4096, "work": 1024}}"#, 1);
    let grass = mat(&e, "grass");
    let d: Vec<NodeDesc> = (0..2000u64)
        .map(|i| NodeDesc::new(i, grass, Vec3::new(0.05 + (i % 40) as f32 * 0.045, 0.5, 0.05 + (i / 40) as f32 * 0.03), Vec3::new(0.01, 0.01, 0.01)))
        .collect();
    e.add_many(&d).unwrap();
    e.act(Act { kind: ActKind::Ignite, target: Target::Node(1999), amount: 1.0 }).unwrap();
    let mut truncated = 0;
    for _ in 0..60 * 120 {
        let r = *e.step();
        truncated += r.truncated;
        assert!(r.work <= 1024);
    }
    assert!(truncated > 0, "queries were cut");
    let never = e.nodes().iter().filter(|n| n.phase == Phase::Intact).count();
    assert_eq!(never, 0, "a query the budget cuts resumes where it stopped, so no node is always left out");
}

#[test]
fn an_acts_work_is_at_most_twice_the_node_count() {
    // Each node covers 4 x 4 x 4 cells of 0.25 m: the sphere's cells hold 64 entries a node.
    let mut e = sim(r#"{"elements":1, "cell_m": 0.25, "reach_m": 0.25, "limits": {"nodes": 32768, "burning": 512}}"#, 1);
    let wood = mat(&e, "wood");
    let d: Vec<NodeDesc> = (0..1000u64).map(|i| NodeDesc::new(i, wood, Vec3::new(0.5, 0.5, 0.5), Vec3::new(0.45, 0.45, 0.45))).collect();
    e.add_many(&d).unwrap();
    let wet = |at: Vec3, radius: f32| Act { kind: ActKind::Wet, target: Target::Sphere { at, radius }, amount: 0.1 };
    let r = e.act(wet(Vec3::new(0.5, 0.5, 0.5), 1.0)).unwrap();
    assert_eq!(r.nodes, 1000);
    assert!(r.work <= 2 * 1000 + 1, "work {} for 1,000 nodes", r.work);
    assert!(r.grid_ops > 0, "the first act after adding pays the grid's rebuild, said apart");
    // Spread out, a small sphere looks only at its own cells.
    let mut f = sim(r#"{"elements":1}"#, 1);
    let d: Vec<NodeDesc> = (0..1000u64).map(|i| NodeDesc::new(i, wood, Vec3::new((i % 40) as f32 * 3.0, 0.0, (i / 40) as f32 * 3.0), Vec3::ONE)).collect();
    f.add_many(&d).unwrap();
    let r = f.act(wet(Vec3::new(30.0, 0.0, 30.0), 2.0)).unwrap();
    assert!(r.nodes >= 1 && r.work < 100, "{r:?}");
    let r = f.act(wet(Vec3::new(30.0, 0.0, 30.0), 2.0)).unwrap();
    assert_eq!(r.grid_ops, 0, "no rebuild when nothing changed");
}

#[test]
fn a_grid_rebuild_is_counted_and_said_before_it_runs() {
    let mut e = sim(r#"{"elements": 1}"#, 1);
    let trees = forest(&e, 3000, 1);
    e.add_many(&trees).unwrap();
    let due = e.pending_rebuild_ops();
    let entries = specula_elementa::grid::entries_for(e.nodes(), e.config().cell_m);
    assert_eq!(due, specula_elementa::grid::rebuild_ops(entries));
    assert!(due <= e.config().worst_case().rebuild_ops);
    let bound = e.check_bound(60, 0);
    assert!(bound >= due + 10 * (2 * 3000 + 32_768), "the prediction's ceiling counts the rebuild: {bound}");
    let mut seen = 0;
    for _ in 0..12 {
        let r = *e.step();
        if r.spread {
            if seen == 0 {
                assert_eq!(r.grid_ops, due);
                assert!(r.grid_rebuilt && r.ops() >= due);
            } else {
                assert_eq!(r.grid_ops, 0);
            }
            seen += 1;
        }
    }
    assert_eq!(e.pending_rebuild_ops(), 0);
    e.move_node(1, Vec3::new(5.0, 1.5, 5.0), Vec3::new(0.35, 1.5, 0.35)).unwrap();
    let after = specula_elementa::grid::rebuild_ops(specula_elementa::grid::entries_for(e.nodes(), e.config().cell_m));
    assert!(after > 0 && e.pending_rebuild_ops() == after, "one moved node rebuilds the whole grid: batch the moves");
}

#[test]
fn heat_passed_over_by_the_budget_is_given_late_not_lost() {
    const BRUSH: &str = r#""brush": {"ignite": 0.2, "burn_s": 600, "heat_out": 0.4, "flame": 0.6, "cool": 0.05, "embers": 0}"#;
    // A source and its target, and far away a crowd of burning brush that eats the budget.
    let run = |work: u32| {
        let json = format!(r#"{{"elements": 1, "materials": {{{BRUSH}}}, "reach_m": 1.0, "limits": {{"nodes": 4096, "burning": 4096, "work": {work}}}}}"#);
        let mut e = sim(&json, 1);
        let brush = mat(&e, "brush");
        let h = Vec3::new(0.1, 0.1, 0.1);
        e.add(NodeDesc::new(1, brush, Vec3::ZERO, h)).unwrap();
        e.add(NodeDesc::new(2, brush, Vec3::new(1.0, 0.0, 0.0), h)).unwrap();
        let crowd: Vec<NodeDesc> =
            (0..40u64).map(|i| NodeDesc::new(100 + i, brush, Vec3::new(100.0 + (i % 8) as f32 * 0.2, 0.0, (i / 8) as f32 * 0.2), h)).collect();
        e.add_many(&crowd).unwrap();
        e.act(Act { kind: ActKind::Ignite, target: Target::Sphere { at: Vec3::new(100.7, 0.0, 0.4), radius: 2.0 }, amount: 1.0 }).unwrap();
        e.act(Act { kind: ActKind::Ignite, target: Target::Node(1), amount: 1.0 }).unwrap();
        let (mut lit, mut cut, mut target) = (None, 0, None);
        for _ in 0..60 * 60 {
            let r = *e.step();
            cut += u32::from(r.deferred > 0 || r.truncated > 0);
            for ev in e.take_events().list {
                match ev.id {
                    1 => lit = lit.or(Some(ev.tick)),
                    2 => target = target.or(Some(ev.tick)),
                    _ => {}
                }
            }
        }
        ((target.unwrap() - lit.unwrap()) / 6, cut)
    };
    let (free, none) = run(1 << 20);
    let (tight, cut) = run(1024);
    assert_eq!(none, 0);
    assert!(cut > 10, "the budget cut {cut} steps");
    println!("target caught {free} steps after its source with room to spare, {tight} with the budget cut");
    assert!(tight.abs_diff(free) <= 4, "{tight} steps against {free}: the heat owed is given, not dropped");
}

#[test]
fn a_forest_of_20000_steps_within_the_stated_work_bound() {
    let json = r#"{"elements": 1, "reach_m": 1.0, "limits": {"nodes": 20000, "burning": 512, "work": 32768}}"#;
    let mut e = sim(json, 7);
    let trees = forest(&e, 20_000, 1);
    assert_eq!(e.add_many(&trees).unwrap().added, 20_000);
    e.set_wind(Vec3::new(4.0, 0.0, 1.0)).unwrap();
    let worst = e.config().worst_case();
    assert_eq!(worst.ops_per_step, 2 * 20_000 + 32_768);
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
        assert!(r.grid_ops <= worst.rebuild_ops);
        assert!(r.ops() <= worst.ops_per_step + r.grid_ops, "ops {} at tick {}", r.ops(), r.tick);
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

//! What the work limit may and may not change (docs/design/EFFECTUS.md, "What a limit may
//! change"): a query the budget cuts gives every neighbour of its pass the same heat, once; heat
//! the budget could not give is not given later; so a limit can slow a fire or keep it smaller,
//! and never lets heat cross a gap that the same burning things could not cross with room in
//! the budget. And a cut query that the world changes under before it resumes heats no
//! neighbour twice.

mod common;

use common::{mat, sim};
use specula_core::math::Vec3;
use specula_core::rng::Rng;
use specula_elementa::{Elementa, EventKind, NodeDesc, Phase};
use std::collections::BTreeMap;

/// A torch that burns for an hour giving very little heat, and `n` sinks (which never cool, so
/// a sink's heat is all the heat it was given) at one gap from it, in one 2 m cell. With the
/// smallest work budget its query takes two steps.
fn sinks(work: u32, n: u64) -> Elementa {
    let json = format!(
        r#"{{"elements": 1, "materials": {{"torch": {{"ignite": 0.1, "flame": 0.9, "burn_s": 3600, "heat_out": 0.001, "cool": 0}},
            "sink": {{"cool": 0}}}}, "cell_m": 2, "reach_m": 1.0, "limits": {{"nodes": 8192, "burning": 8192, "work": {work}}}}}"#
    );
    let mut e = sim(&json, 1);
    let (torch, sink) = (mat(&e, "torch"), mat(&e, "sink"));
    let h = Vec3::new(0.01, 0.01, 0.01);
    let mut d = vec![NodeDesc::new(0, torch, Vec3::new(0.5, 0.5, 0.5), h).heat(1.0)];
    for i in 1..=n {
        d.push(NodeDesc::new(i * 10, sink, Vec3::new(1.0, 0.5, 0.5), h));
    }
    e.add_many(&d).unwrap();
    e
}

fn sink_heats(e: &Elementa) -> Vec<f32> {
    e.nodes().iter().filter(|n| n.id != 0).map(|n| n.heat).collect()
}

#[test]
fn every_neighbour_of_a_cut_query_is_given_the_same_heat() {
    let mut room = sinks(1 << 20, 2000);
    for _ in 0..6 * 61 {
        room.step();
    }
    let full = sink_heats(&room)[0];
    assert!(full > 0.0 && sink_heats(&room).iter().all(|&h| h == full));
    for work in [1024u32, 1500] {
        let mut e = sinks(work, 2000);
        let mut cut = 0;
        for _ in 0..6 * 61 {
            cut += e.step().truncated;
        }
        assert!(cut > 10, "work {work}: the budget cut the query {cut} times");
        let h = sink_heats(&e);
        let (lo, hi) = (h.iter().copied().fold(f32::MAX, f32::min), h.iter().copied().fold(f32::MIN, f32::max));
        println!("work {work}: every sink {lo} .. {hi}; with room {full}");
        // One pass, one step's heat for each: the same for all, whichever step reached it (on
        // 52be4bf7 the first half of the query had half the heat of the second).
        assert_eq!(lo, hi, "work {work}: sinks at one gap were given different heat");
        assert!(hi <= full, "work {work}: a cut query gave more than room would");
    }
}

#[test]
fn a_tight_budget_never_crosses_a_gap_that_room_cannot() {
    // A source and a target that cools as fast as it is heated (the model says it never
    // catches), and far off a hundred burning decoys whose queries eat the budget. On
    // 52be4bf7 the heat the budget delayed was given in one lump and the target caught.
    let run = |work: u32| {
        let json = format!(
            r#"{{"elements": 1, "materials": {{"decoy": {{"ignite": 0.1, "flame": 0.9, "burn_s": 3600, "heat_out": 0.5, "cool": 0.1}},
                "src": {{"ignite": 0.1, "flame": 0.9, "burn_s": 3600, "heat_out": 0.6, "cool": 0.1}},
                "damp": {{"ignite": 0.35, "burn_s": 10, "heat_out": 0.5, "flame": 0.8, "cool": 10}}}},
                "cell_m": 2, "reach_m": 1.0, "limits": {{"nodes": 4096, "burning": 4096, "work": {work}}}}}"#
        );
        let mut e = sim(&json, 1);
        let (decoy, src, damp) = (mat(&e, "decoy"), mat(&e, "src"), mat(&e, "damp"));
        let mut d: Vec<NodeDesc> =
            (0..100u64).map(|k| NodeDesc::new(1000 + k, decoy, Vec3::new(20.0 * k as f32, 100.0, 0.0), Vec3::new(3.9, 3.9, 3.9)).heat(1.0)).collect();
        d.push(NodeDesc::new(1, src, Vec3::ZERO, Vec3::new(0.1, 0.1, 0.1)).heat(1.0));
        d.push(NodeDesc::new(2, damp, Vec3::new(0.7, 0.0, 0.0), Vec3::new(0.1, 0.1, 0.1)));
        e.add_many(&d).unwrap();
        assert_eq!(e.steps_to_ignite(e.node(1).unwrap(), e.node(2).unwrap()), None, "the model: never");
        let mut cut = 0;
        for _ in 0..60 * 60 {
            let r = *e.step();
            cut += u32::from(r.deferred > 0 || r.truncated > 0);
        }
        (e.node(2).unwrap().phase, cut)
    };
    assert_eq!(run(1 << 20), (Phase::Intact, 0));
    let (phase, cut) = run(1024);
    assert!(cut > 100, "the budget cut {cut} steps");
    assert_eq!(phase, Phase::Intact, "a limit let heat cross a gap the model says it never crosses");
}

/// One random small world: a few burning sources that never go out (some throw embers), many
/// targets that catch and give no heat (so what burns is the sources only, whatever catches),
/// and burning decoys far off that eat the budget. When each target first caught, by tick.
/// Decoys' ids start here.
const DECOYS: u64 = 1 << 40;

fn world(seed: u64, work: u32) -> BTreeMap<u64, u64> {
    let mut r = Rng::named(seed, "budget-worlds");
    let json = format!(
        r#"{{"elements": 1, "materials": {{
            "decoy": {{"ignite": 0.1, "flame": 0.9, "burn_s": 3600, "heat_out": 0.5, "cool": 0.1}},
            "src": {{"ignite": 0.1, "flame": 0.9, "burn_s": 3600, "heat_out": 0.5, "cool": 0.1, "embers": 0.8}},
            "dry": {{"ignite": 0.3, "burn_s": 10, "heat_out": 0, "flame": 0.8, "cool": 0.3}},
            "damp": {{"ignite": 0.35, "burn_s": 10, "heat_out": 0, "flame": 0.8, "cool": 1.5}}}},
            "cell_m": 2, "reach_m": 1.0, "limits": {{"nodes": 4096, "burning": 4096, "work": {work}}}}}"#
    );
    let mut e = sim(&json, seed);
    let (decoy, src, dry, damp) = (mat(&e, "decoy"), mat(&e, "src"), mat(&e, "dry"), mat(&e, "damp"));
    let f = |r: &mut Rng, span: f32| (r.below(1_000_000) as f32 / 1_000_000.0) * span;
    let mut d: Vec<NodeDesc> =
        (0..(80 + r.below(120)) as u64).map(|k| NodeDesc::new(DECOYS + k, decoy, Vec3::new(10.0 * k as f32, 100.0, 0.0), Vec3::new(0.1, 0.1, 0.1)).heat(1.0)).collect();
    let sources = 1 + r.below(5) as u64;
    let targets = 5 + r.below(30) as u64;
    for k in 0..sources + targets {
        let at = Vec3::new(f(&mut r, 6.0), f(&mut r, 2.0), f(&mut r, 6.0));
        let h = Vec3::new(0.05 + f(&mut r, 0.3), 0.05 + f(&mut r, 0.3), 0.05 + f(&mut r, 0.3));
        let m = if k < sources {
            src
        } else if r.below(2) == 0 {
            dry
        } else {
            damp
        };
        let desc = NodeDesc::new(1 + r.below(5000) as u64 * 1000 + k, m, at, h);
        d.push(if k < sources { desc.heat(1.0) } else { desc.wet(f(&mut r, 0.3)) });
    }
    e.add_many(&d).unwrap();
    e.set_wind(Vec3::new(f(&mut r, 12.0) - 6.0, 0.0, f(&mut r, 12.0) - 6.0)).unwrap();
    let mut caught = BTreeMap::new();
    for _ in 0..60 * 10 {
        e.step();
        for ev in e.take_events().list {
            if ev.kind == EventKind::Ignited && ev.id < DECOYS {
                caught.entry(ev.id).or_insert(ev.tick);
            }
        }
    }
    caught
}

#[test]
fn in_random_worlds_a_tight_budget_catches_no_more_and_none_sooner() {
    // What is promised (EFFECTUS.md): with the same things burning, a node is given at most the
    // heat it is given with room in the budget, at every step, embers included; so it catches
    // no sooner, and catches only if it would with room.
    let (mut fewer, mut later, mut cut_worlds) = (0, 0, 0);
    for seed in 0..60u64 {
        let room = world(seed, 1 << 20);
        let tight = world(seed, 1024);
        for (id, t) in &tight {
            let Some(r) = room.get(id) else {
                panic!("world {seed}: node {id} caught at tick {t} under the tight budget and never with room");
            };
            assert!(t >= r, "world {seed}: node {id} caught at {t} under the tight budget, at {r} with room");
            later += u32::from(t > r);
        }
        fewer += room.len() - tight.len();
        cut_worlds += u32::from(tight != room);
    }
    println!("60 worlds: {cut_worlds} differ; {fewer} fewer caught and {later} caught later under the tight budget");
    assert!(cut_worlds > 20 && fewer + later as usize > 20, "the tight budget did bite");
}

#[test]
fn a_cut_query_heats_no_neighbour_twice_when_nodes_come_and_go() {
    // The reference: no change between a cut and its resume.
    let run = |what: &str| {
        let mut e = sinks(1024, 2000);
        let sink = mat(&e, "sink");
        let h = Vec3::new(0.01, 0.01, 0.01);
        let mut changes = 0u64;
        for _ in 0..6 * 61 {
            let r = *e.step();
            if r.spread && r.truncated > 0 {
                assert!(e.cursor().is_mid());
                changes += 1;
                match what {
                    // The lowest sink, which the query has passed.
                    "remove" => assert!(e.remove(e.nodes()[1].id)),
                    // A sink with a lower id than any the query has still to reach.
                    "add" => e.add(NodeDesc::new(1 + changes * 10, sink, Vec3::new(1.0, 0.5, 0.5), h)).unwrap(),
                    _ => {}
                }
            }
        }
        (sink_heats(&e), e.nodes().iter().map(|n| (n.id, n.heat)).collect::<BTreeMap<u64, f32>>())
    };
    let (none, by_id) = run("none");
    let per = none[0];
    assert!(none.iter().all(|&x| x == per));
    // A removed sink moves no other: every sink still there was given what it would have been
    // (on 52be4bf7 the resumed query skipped one).
    let (removed, _) = run("remove");
    assert!(removed.iter().all(|&x| x == per), "after removes: {:?} against {per}", (removed.iter().copied().fold(f32::MAX, f32::min), removed.iter().copied().fold(f32::MIN, f32::max)));
    // An added sink heats no other twice (on 52be4bf7 one was heated twice in a pass). The
    // query grows by the sinks added, so the last pass may end a step later for some: never
    // more than one step's heat a pass.
    let (_, added) = run("add");
    for (id, x) in &added {
        if *id != 0 {
            assert!(*x <= per, "sink {id} was given {x} with sinks added, more than one step's heat a pass ({per})");
            assert!(!by_id.contains_key(id) || *x >= per * 0.9, "sink {id} lost more than its last pass: {x} against {per}");
        }
    }
}

/// A torch at the origin and sinks in the query's first cell, (-1, -1, -1); the query's cells
/// run (-1..0) on each axis, so (0, -1, -1) is met after it. One sink, `MOVER`, has the lowest
/// id: the query passes it first.
fn corner(work: u32) -> Elementa {
    let json = format!(
        r#"{{"elements": 1, "materials": {{"torch": {{"ignite": 0.1, "flame": 0.9, "burn_s": 3600, "heat_out": 0.001, "cool": 0}},
            "sink": {{"cool": 0}}}}, "cell_m": 2, "reach_m": 1.0, "limits": {{"nodes": 8192, "burning": 8192, "work": {work}}}}}"#
    );
    let mut e = sim(&json, 1);
    let (torch, sink) = (mat(&e, "torch"), mat(&e, "sink"));
    let h = Vec3::new(0.01, 0.01, 0.01);
    let mut d = vec![NodeDesc::new(5_000_000, torch, Vec3::ZERO, h).heat(1.0)];
    for i in 0..2000u64 {
        d.push(NodeDesc::new(MOVER + i, sink, Vec3::new(-0.5, -0.5, -0.5), h));
    }
    e.add_many(&d).unwrap();
    e
}

const MOVER: u64 = 1;

#[test]
fn a_cut_query_heats_no_neighbour_twice_when_nodes_move() {
    let h = Vec3::new(0.01, 0.01, 0.01);
    let (here, there) = (Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, -0.5, -0.5));
    // With room: what one sink is given in the run (at either place: the same gap and lean).
    let mut room = corner(1 << 20);
    for _ in 0..6 * 61 {
        room.step();
    }
    let full = room.node(MOVER + 1).unwrap().heat;
    // The passed sink moves ahead of the cut, then back after the step, every time a query is
    // cut: on 52be4bf7 it was heated in both places in one pass.
    let mut e = corner(1024);
    let mut ended = 0;
    for _ in 0..6 * 61 {
        let r = *e.step();
        if r.spread && r.truncated > 0 {
            e.move_node(MOVER, there, h).unwrap();
            ended += u32::from(!e.cursor().is_mid());
        } else if r.spread {
            e.move_node(MOVER, here, h).unwrap();
        }
    }
    assert!(ended > 0, "the moves ended cut queries");
    let mover = e.node(MOVER).unwrap().heat;
    assert!(mover <= full, "the moving sink was given {mover}, more than with room ({full})");
    assert!(e.nodes().iter().filter(|n| n.id != 5_000_000).all(|n| n.heat <= full));

    // The burning node itself moves (or goes) between a cut and its resume: its query ends
    // there and the pass moves on, so nothing is heated twice.
    let mut e = corner(1024);
    let mut moved = 0;
    for _ in 0..6 * 61 {
        let r = *e.step();
        if r.spread && r.truncated > 0 {
            let c = e.node(5_000_000).unwrap().center;
            e.move_node(5_000_000, c + Vec3::new(0.0, 0.0, 1.0e-4), h).unwrap();
            assert_eq!(e.cursor().id, 5_000_001, "the pass moved past the moved node");
            moved += 1;
        }
    }
    assert!(moved > 0);
    assert!(e.nodes().iter().filter(|n| n.id != 5_000_000).all(|n| n.heat <= full));
    let mut e = corner(1024);
    loop {
        let r = *e.step();
        if r.truncated > 0 {
            break;
        }
    }
    assert!(e.cursor().is_mid() && e.remove(5_000_000));
    assert!(!e.cursor().is_mid(), "a removed node's cut query is over");
    for _ in 0..60 {
        e.step();
    }
    let reloaded = Elementa::load(&e.save()).unwrap();
    assert_eq!(reloaded.hash(), e.hash());
}

#[test]
fn a_cursor_part_way_through_a_query_is_checked_on_load() {
    let mut e = sinks(1024, 2000);
    loop {
        let r = *e.step();
        if r.spread && r.truncated > 0 {
            break;
        }
    }
    let c = e.cursor();
    assert!(c.is_mid() && c.id == 0);
    let bytes = e.save();
    assert_eq!(Elementa::load(&bytes).unwrap().cursor(), c);
    // The cursor follows the tick (8 bytes) and the ember key (8): id, cell, next.
    let tick_at = (0..bytes.len() - 8).find(|&i| bytes[i..i + 8] == e.tick().to_le_bytes()).unwrap();
    let at = tick_at + 16;
    assert_eq!(bytes[at..at + 8], c.id.to_le_bytes());
    // Part-way through the query of a node that does not burn (a sink) or does not exist.
    for id in [10u64, 7] {
        let mut b = bytes.clone();
        b[at..at + 8].copy_from_slice(&id.to_le_bytes());
        assert_eq!(Elementa::load(&b).unwrap_err().code, "snapshot_invalid", "cursor on node {id}");
    }
    // At a cell past the query's last.
    let mut b = bytes.clone();
    b[at + 8..at + 12].copy_from_slice(&5000u32.to_le_bytes());
    let err = Elementa::load(&b).unwrap_err();
    assert_eq!(err.code, "snapshot_invalid", "{err}");
    // Any `next` is an id: it loads, and resumes at the next sink at or above it.
    let mut b = bytes.clone();
    b[at + 12..at + 20].copy_from_slice(&u64::MAX.to_le_bytes());
    let mut l = Elementa::load(&b).unwrap();
    for _ in 0..60 {
        l.step();
    }
}

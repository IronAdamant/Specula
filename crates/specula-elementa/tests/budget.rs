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
use specula_elementa::{Act, ActKind, Elementa, EventKind, NodeDesc, Phase, Scenario, Target};
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
    let mut room = sinks(1 << 19, 2000);
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
    // catches), and far off sixty burning decoys whose queries eat a tight budget (and fit in a full one). On
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
            (0..60u64).map(|k| NodeDesc::new(1000 + k, decoy, Vec3::new(20.0 * k as f32, 100.0, 0.0), Vec3::new(3.9, 3.9, 3.9)).heat(1.0)).collect();
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
    assert_eq!(run(1 << 19), (Phase::Intact, 0));
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
        let room = world(seed, 1 << 19);
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
    let mut room = corner(1 << 19);
    for _ in 0..6 * 61 {
        room.step();
    }
    let full = room.node(MOVER + 1).unwrap().heat;
    // The passed sink moves ahead of the cut, then back after the step, every time a query is
    // cut: on 52be4bf7 it was heated in both places in one pass. Now it goes on the query's
    // skip list and the query goes on.
    let mut e = corner(1024);
    let mut kept = 0;
    for _ in 0..6 * 61 {
        let r = *e.step();
        if r.spread && r.truncated > 0 {
            e.move_node(MOVER, there, h).unwrap();
            kept += u32::from(e.cursor().is_mid());
        } else if r.spread {
            e.move_node(MOVER, here, h).unwrap();
        }
    }
    assert!(kept > 10, "the cut queries went on past the moves ({kept})");
    let mover = e.node(MOVER).unwrap().heat;
    assert!(mover <= full, "the moving sink was given {mover}, more than with room ({full})");
    assert!(e.nodes().iter().filter(|n| n.id != 5_000_000).all(|n| n.heat <= full));
    // Against a sink of the same run, met right after it in every pass: exactly the same, one
    // step's heat a pass (with the skip list taken out the mover had exactly twice: the fourth
    // reviewer's mutant M1, which the comparison with room above cannot see).
    let next = e.node(MOVER + 1).unwrap().heat;
    assert_eq!(mover, next, "the moving sink against a staying one of the same run");

    // The burning node itself moves between a cut and its resume: within its query's cells the
    // query goes on; into other cells it ends there and the pass moves on (its cells are not
    // the ones it was walking), so nothing is heated twice.
    let mut e = corner(1024);
    let mut moved = 0;
    for _ in 0..6 * 61 {
        let r = *e.step();
        if r.spread && r.truncated > 0 {
            let c = e.node(5_000_000).unwrap().center;
            e.move_node(5_000_000, c + Vec3::new(0.0, 0.0, 1.0e-4), h).unwrap();
            assert!(e.cursor().is_mid() && e.cursor().id == 5_000_000, "a move within its cells keeps the query");
            e.move_node(5_000_000, c + Vec3::new(0.0, 0.0, 3.0), h).unwrap();
            assert_eq!(e.cursor().id, 5_000_001, "the pass moved past the node that left its cells");
            e.move_node(5_000_000, c, h).unwrap();
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

/// The second reviewer's `meet`: a line of 30 brush (front B) runs towards a target from one
/// side; one brush (A) on the other side is lit by an act at `act_at`; the target catches only
/// if both burn at once. Eighty decoys burning for 15 s eat a tight budget early (and fit in a full one), which
/// delays front B. When B's last brush caught and burnt out, and when the target caught.
fn meet(work: u32, act_at: Option<u64>) -> (Option<u64>, Option<u64>, Option<u64>, u32) {
    let json = format!(
        r#"{{"elements": 1, "materials": {{"decoy": {{"ignite": 0.1, "flame": 0.9, "burn_s": 15, "heat_out": 0.5, "cool": 0.1}},
            "brush": {{"ignite": 0.2, "burn_s": 20, "heat_out": 0.4, "flame": 0.6, "cool": 0.05, "embers": 0}},
            "damp": {{"ignite": 0.35, "burn_s": 10, "heat_out": 0, "flame": 0.8, "cool": 0.32}}}},
            "cell_m": 2, "reach_m": 1.0, "limits": {{"nodes": 4096, "burning": 4096, "work": {work}}}}}"#
    );
    let mut e = sim(&json, 1);
    let (decoy, brush, damp) = (mat(&e, "decoy"), mat(&e, "brush"), mat(&e, "damp"));
    let h = Vec3::new(0.1, 0.1, 0.1);
    let mut d: Vec<NodeDesc> =
        (0..80u64).map(|k| NodeDesc::new(1000 + k, decoy, Vec3::new(20.0 * k as f32, 100.0, 0.0), Vec3::new(3.9, 3.9, 3.9)).heat(1.0)).collect();
    for k in 1..=30u64 {
        let n = NodeDesc::new(k, brush, Vec3::new(-31.0 + k as f32, 0.0, 0.0), h);
        d.push(if k == 1 { n.heat(1.0) } else { n });
    }
    d.push(NodeDesc::new(100, damp, Vec3::ZERO, h));
    d.push(NodeDesc::new(200, brush, Vec3::new(1.0, 0.0, 0.0), h));
    e.add_many(&d).unwrap();
    let (mut b_lit, mut b_out, mut t_lit, mut cut) = (None, None, None, 0);
    for _ in 0..60 * 150 {
        if Some(e.tick()) == act_at {
            e.act(Act { kind: ActKind::Ignite, target: Target::Node(200), amount: 1.0 }).unwrap();
        }
        let r = *e.step();
        cut += u32::from(r.deferred > 0 || r.truncated > 0);
        for ev in e.take_events().list {
            match (ev.id, ev.kind) {
                (30, EventKind::Ignited) => b_lit = b_lit.or(Some(ev.tick)),
                (30, EventKind::Burnt) => b_out = b_out.or(Some(ev.tick)),
                (100, EventKind::Ignited) => t_lit = t_lit.or(Some(ev.tick)),
                _ => {}
            }
        }
    }
    (b_lit, b_out, t_lit, cut)
}

#[test]
fn over_a_whole_fire_a_tight_budget_can_change_what_catches() {
    // What happens, held so that a change to it is seen (EFFECTUS.md, "What a limit may
    // change"): with room, front B has burnt out before A is lit, and the target never catches;
    // under a tight budget B is late, both burn at once, and the target catches. The promise is
    // for the same nodes burning; here they are not the same.
    let (_, room_out, _, _) = meet(1 << 19, None);
    let (tight_lit, _, _, _) = meet(1024, None);
    // A is lit just after front B reaches the target under the tight budget, which is long
    // after B has burnt out with room.
    let act = tight_lit.expect("front B arrives under the tight budget") + 60;
    assert!(act > room_out.expect("front B burns out with room") + 60);
    let room = meet(1 << 19, Some(act));
    assert_eq!(room.3, 0, "with room no step is cut");
    let tight = meet(1024, Some(act));
    println!("front B lit/out: room {:?}/{:?}, tight {:?}/{:?}; A lit at {act}; target: room {:?}, tight {:?}", room.0, room.1, tight.0, tight.1, room.2, tight.2);
    assert!(tight.0 > room.0, "the tight budget delays front B");
    assert_eq!(room.2, None, "with room the two fires never burn at once, and the target never catches");
    assert!(tight.2.is_some(), "under the tight budget both burn at once and the target catches");
}

#[test]
fn a_mover_cannot_starve_a_cut_query() {
    // The second reviewer's `starve`: a node moved from a cell the cut query has passed into the
    // cell it is cut in, after every cut, and back between spread steps. On 17a5a593 each such
    // move ended the query: 299 of 300 passes, and 986 of 2,000 sinks were never heated. Now
    // the mover goes on the query's skip list and every pass runs to its end: every sink is
    // given exactly what it is given with no mover.
    let run = |oscillate: bool| {
        let mut e = sinks(1024, 2000);
        let sink = mat(&e, "sink");
        let h = Vec3::new(0.01, 0.01, 0.01);
        let (before, ahead) = (Vec3::new(-0.5, 0.5, 0.5), Vec3::new(1.0, 0.5, 0.5));
        e.add(NodeDesc::new(999_999, sink, before, h)).unwrap();
        let mut kept = 0;
        for _ in 0..6 * 300 {
            let r = *e.step();
            if oscillate {
                if r.spread && e.cursor().is_mid() {
                    e.move_node(999_999, ahead, h).unwrap();
                    kept += u32::from(e.cursor().is_mid());
                } else if !r.spread && e.node(999_999).unwrap().center == ahead {
                    e.move_node(999_999, before, h).unwrap();
                }
            }
        }
        (e.nodes().iter().filter(|n| n.id != 0 && n.id != 999_999).map(|n| n.heat).collect::<Vec<f32>>(), kept)
    };
    let (still, _) = run(false);
    let (moved, kept) = run(true);
    assert!(kept > 100, "the mover crossed the cut point {kept} times and the query went on");
    assert!(still.iter().all(|&h| h > 0.0));
    assert_eq!(moved, still, "with a mover crossing every cut, every sink is given what it is given with none");
}

#[test]
fn a_node_removed_and_added_again_ahead_of_the_cut_is_not_heated_twice() {
    // The second reviewer's `readd`: a sink the query has passed is removed and added again,
    // with its own heat, in the cell the query is cut in; put back after the pass. On 17a5a593
    // it ended at 1.61 times the heat of a sink that stayed.
    let mut e = sinks(1024, 2000);
    let sink = mat(&e, "sink");
    let h = Vec3::new(0.01, 0.01, 0.01);
    let (early, ahead) = (Vec3::new(-0.2, 0.5, 0.5), Vec3::new(1.0, 0.5, 0.5));
    e.add(NodeDesc::new(999_000, sink, early, h)).unwrap();
    let mut readded = 0;
    for _ in 0..6 * 300 {
        let r = *e.step();
        let at = e.node(999_000).unwrap().center;
        if r.spread && e.cursor().is_mid() && at == early {
            let heat = e.node(999_000).unwrap().heat;
            assert!(e.remove(999_000));
            e.add(NodeDesc::new(999_000, sink, ahead, h).heat(heat)).unwrap();
            readded += 1;
        } else if r.spread && !e.cursor().is_mid() && at == ahead {
            let heat = e.node(999_000).unwrap().heat;
            assert!(e.remove(999_000));
            e.add(NodeDesc::new(999_000, sink, early, h).heat(heat)).unwrap();
        }
    }
    let (it, other) = (e.node(999_000).unwrap().heat, e.node(10).unwrap().heat);
    assert!(readded > 50, "re-added {readded} times");
    assert!(it <= other, "the re-added node was given {it}, more than a sink that stayed ({other})");
}

#[test]
fn an_ember_never_lands_in_a_cell_more_crowded_than_the_work_limit_and_check_says_so() {
    // The second reviewer's `crowdember`: one burning log in a wind, and 1,500 grass tufts
    // tiling the cell downwind. An ember lands whole or not at all, so where a cell holds more
    // than the work limit can look at, none ever lands there, whatever else burns. The
    // prediction says so before the run.
    let run = |work: u32| {
        let json = format!(r#"{{"elements": 1, "materials": {{"wood": {{"embers": 1.0}}}}, "cell_m": 2, "reach_m": 0.5, "limits": {{"nodes": 4096, "burning": 4096, "work": {work}}}}}"#);
        let mut e = sim(&json, 3);
        let (wood, grass) = (mat(&e, "wood"), mat(&e, "grass"));
        let mut d = vec![NodeDesc::new(1, wood, Vec3::new(1.0, 1.0, 1.0), Vec3::new(0.4, 0.4, 0.4)).heat(1.0)];
        for i in 0..1500u64 {
            d.push(NodeDesc::new(10 + i, grass, Vec3::new(4.05 + (i % 30) as f32 * 0.06, 1.0, 0.05 + (i / 30) as f32 * 0.038), Vec3::new(0.03, 0.03, 0.019)));
        }
        e.add_many(&d).unwrap();
        e.set_wind(Vec3::new(4.0, 0.0, 0.0)).unwrap();
        let p = e.check(&Scenario { acts: vec![], wind: None, ambient: None, ticks: 60 * 60, max_ops: u64::MAX }).unwrap();
        (p.crowded_cells, p.most_in_a_cell, p.ignited)
    };
    let (crowded, most, caught) = run(1 << 19);
    assert_eq!(crowded, 0);
    assert!(most >= 1500 && caught > 1000, "with room the embers light the tufts ({caught})");
    let (crowded, _, caught) = run(1024);
    assert_eq!(crowded, 1, "the prediction names the crowded cell");
    assert_eq!(caught, 1, "only the log: no ember lands among the tufts");
}

/// The sinks world with `extra` more nodes, all at `at`, ids from 100,000.
fn sinks_and(work: u32, n: u64, extra: u64, at: Vec3) -> Elementa {
    let mut e = sinks(work, n);
    let sink = mat(&e, "sink");
    let h = Vec3::new(0.01, 0.01, 0.01);
    let d: Vec<NodeDesc> = (0..extra).map(|k| NodeDesc::new(100_000 + k, sink, at, h)).collect();
    e.add_many(&d).unwrap();
    e
}

/// The state saves, and loads to the same hash: what the crate makes it always accepts.
fn loads(e: &Elementa, when: &str) {
    match Elementa::load(&e.save()) {
        Ok(l) => assert_eq!(l.hash(), e.hash(), "{when}: loaded to another hash"),
        Err(err) => panic!("{when}: the state's own save is refused: {err}"),
    }
}

#[test]
fn a_crowd_moving_out_of_the_reach_neither_ends_a_cut_query_nor_tilts_it() {
    // The fourth reviewer's `jitter`: 1,100 nodes in a cell the query walks before the sinks'
    // (x = -1.5, query cell 3), two metres beyond the torch's reach, jiggled by 0.1 mm within
    // their cell after every cut. On b2d675d4 each went on the skip list: 149 of 299 passes ended,
    // 1,289 of 2,000 sinks were never heated and the lowest ids got twice the heat. They are
    // never heated, so a move of theirs changes nothing a pass would see: nothing is listed.
    let base = Vec3::new(-1.5, 0.5, 0.5);
    let run = |move_them: bool| {
        let mut e = sinks_and(1024, 2000, 1100, base);
        let (mut ended, mut cuts) = (0u32, 0u32);
        for t in 0..6 * 300u64 {
            let r = *e.step();
            if move_them && r.spread && e.cursor().is_mid() {
                cuts += 1;
                let off = if t.is_multiple_of(12) { 1.0e-4 } else { -1.0e-4 };
                for k in 0..1100u64 {
                    e.move_node(100_000 + k, base + Vec3::new(off, 0.0, 0.0), Vec3::new(0.01, 0.01, 0.01)).unwrap();
                }
                ended += u32::from(!e.cursor().is_mid());
                loads(&e, "after the jiggle");
            }
        }
        (e.nodes().iter().filter(|n| n.id != 0 && n.id < 100_000).map(|n| n.heat).collect::<Vec<f32>>(), ended, cuts)
    };
    let (still, _, _) = run(false);
    let (moved, ended, cuts) = run(true);
    assert!(cuts > 100, "{cuts} cuts");
    assert_eq!(ended, 0, "no cut query ended by moves out of its reach");
    assert!(still.iter().all(|&h| h > 0.0));
    assert_eq!(moved, still, "every sink is given what it is given with no crowd");
}

#[test]
fn a_node_moved_behind_the_cut_and_back_is_still_given_its_heat_once() {
    // The fourth reviewer's `wrongskip`: a sink ahead of every cut (the highest id in the cut
    // cell) moved behind the cut (x = -0.5, a cell walked earlier, within the reach) and straight
    // back after each cut; on b2d675d4 it went on the skip list and got 0 against 0.005148 for a
    // sink that stayed. And a walker that crosses over two gaps: behind after one cut, back after
    // the next step.
    let (ahead, behind) = (Vec3::new(1.0, 0.5, 0.5), Vec3::new(-0.5, 0.5, 0.5));
    let h = Vec3::new(0.01, 0.01, 0.01);
    let run = |how: u32| {
        let mut e = sinks_and(1024, 2000, 0, ahead);
        let sink = mat(&e, "sink");
        e.add(NodeDesc::new(999_999, sink, ahead, h)).unwrap();
        let mut done = 0;
        for _ in 0..6 * 300 {
            let r = *e.step();
            if !r.spread {
                continue;
            }
            let here = e.node(999_999).unwrap().center;
            match how {
                1 if e.cursor().is_mid() => {
                    e.move_node(999_999, behind, h).unwrap();
                    e.move_node(999_999, ahead, h).unwrap();
                    done += 1;
                }
                2 if e.cursor().is_mid() && here == ahead => {
                    e.move_node(999_999, behind, h).unwrap();
                    done += 1;
                }
                2 if here == behind => e.move_node(999_999, ahead, h).unwrap(),
                _ => {}
            }
            loads(&e, "after the walker");
        }
        (e.node(999_999).unwrap().heat, e.node(20_000).unwrap().heat, done)
    };
    let (alone, other, _) = run(0);
    assert!(alone > 0.0 && alone == other);
    let (jiggled, other, done) = run(1);
    assert!(done > 100);
    assert_eq!(jiggled, other, "moved behind and back in one gap: given its heat, once a pass, as a sink that stayed");
    // Over two gaps it misses the passes in which it sat behind the cut (the pass had looked
    // there before it came), and is given its heat once in every other: never more than a sink
    // that stayed, and not nothing.
    let (walker, other, done) = run(2);
    assert!(done > 50);
    assert!(walker > 0.0 && walker <= other, "the walker {walker} against {other}");
}

#[test]
fn whatever_ends_a_cut_pass_the_state_it_leaves_saves_and_loads() {
    // A cut pass's lists exist only while its query is cut, and hold at most 1,024 ids each; a
    // save of anything else is refused on load. So the state must clear them whenever a pass
    // ends, however it ends. (The fourth reviewer's mutants M2 to M4 left a state whose save
    // load refused, and no test said so.) Every tick of each run is saved and loaded.
    let h = Vec3::new(0.01, 0.01, 0.01);
    let away = Vec3::new(50.0, 0.5, 0.5);
    let behind = Vec3::new(-0.5, 0.5, 0.5);
    // Steps to the first cut, holding the save every tick.
    let to_cut = |e: &mut Elementa| loop {
        let r = *e.step();
        loads(e, "before the cut");
        if r.spread && e.cursor().is_mid() {
            break;
        }
    };
    // Passed sinks (ids below the cursor's next, in the cut cell) and ones still to come.
    let passed = |e: &Elementa| -> Vec<u64> { e.nodes().iter().filter(|n| n.id != 0 && n.id < e.cursor().next).map(|n| n.id).collect() };
    let ahead = |e: &Elementa| -> Vec<u64> { e.nodes().iter().filter(|n| n.id >= e.cursor().next && n.id < 100_000).map(|n| n.id).collect() };
    let ticks = |e: &mut Elementa, what: &str, n: u32| {
        for _ in 0..n {
            e.step();
            loads(e, what);
        }
    };

    // The pass finishes with ids on both lists.
    let mut e = sinks(1024, 2000);
    to_cut(&mut e);
    let (p, a) = (passed(&e), ahead(&e));
    e.move_node(p[0], away, h).unwrap();
    e.move_node(a[0], behind, h).unwrap();
    assert!(e.cursor().is_mid());
    loads(&e, "with both lists");
    let c = e.node(0).unwrap().center;
    e.move_node(0, c + Vec3::new(0.0, 1.0e-4, 0.0), h).unwrap();
    assert!(e.cursor().is_mid(), "the cut node moved within its cells keeps its query");
    loads(&e, "the cut node moved");
    ticks(&mut e, "after the pass finished", 60);
    // A sink taken away after the torch gave it heat got exactly one step's heat less a pass than
    // it would; the one moved behind is in no worse place than the sinks: both are heated still.
    assert!(e.node(p[0]).unwrap().heat > 0.0);

    // The cut node removed.
    let mut e = sinks(1024, 2000);
    to_cut(&mut e);
    let (p, a) = (passed(&e), ahead(&e));
    e.move_node(p[0], away, h).unwrap();
    e.move_node(a[0], behind, h).unwrap();
    assert!(e.remove(0) && !e.cursor().is_mid());
    loads(&e, "the cut node removed");
    ticks(&mut e, "after the cut node went", 12);

    // The cut node put out (wet through: it goes out at the next spread step, its pass still cut:
    // 4,000 sinks take the smallest budget several steps).
    let mut e = sinks(1024, 4000);
    to_cut(&mut e);
    let (p, a) = (passed(&e), ahead(&e));
    e.move_node(p[0], away, h).unwrap();
    e.move_node(a[0], behind, h).unwrap();
    e.act(Act { kind: ActKind::Wet, target: Target::Node(0), amount: 1.0 }).unwrap();
    ticks(&mut e, "the cut node put out", 12);
    assert_ne!(e.node(0).unwrap().phase, Phase::Burning);

    // The cut node moved out of its query's cells.
    let mut e = sinks(1024, 2000);
    to_cut(&mut e);
    let p = passed(&e);
    e.move_node(p[0], away, h).unwrap();
    e.move_node(0, Vec3::new(9.5, 0.5, 0.5), h).unwrap();
    assert!(!e.cursor().is_mid());
    loads(&e, "the cut node left its cells");

    // Full lists: 1,024 given sinks taken out of the reach keep the query; the 1,025th ends it.
    let mut e = sinks(1500, 3000);
    to_cut(&mut e);
    let p = passed(&e);
    assert!(p.len() > 1025, "{} passed", p.len());
    for &id in &p[..1024] {
        e.move_node(id, away, h).unwrap();
    }
    assert!(e.cursor().is_mid(), "1,024 on the skip list keep the query");
    loads(&e, "a full skip list");
    e.move_node(p[0], away + Vec3::new(1.0, 0.0, 0.0), h).unwrap();
    assert!(e.cursor().is_mid(), "one already listed, moved again, adds nothing");
    e.move_node(p[1024], away, h).unwrap();
    assert!(!e.cursor().is_mid(), "the 1,025th ends it");
    loads(&e, "past a full skip list");
    ticks(&mut e, "after a full skip list", 12);

    // And 1,025 sinks not yet given heat moved where the pass has been.
    let mut e = sinks(1500, 3000);
    to_cut(&mut e);
    let a = ahead(&e);
    assert!(a.len() > 1025, "{} ahead", a.len());
    for &id in &a[..1024] {
        e.move_node(id, behind, h).unwrap();
    }
    assert!(e.cursor().is_mid(), "1,024 on the missed list keep the query");
    loads(&e, "a full missed list");
    e.move_node(a[1024], behind, h).unwrap();
    assert!(!e.cursor().is_mid(), "the 1,025th ends it");
    loads(&e, "past a full missed list");

    // The same by a batch: 1,024 new sinks where the pass has been keep it, one more ends it.
    let sink_of = |e: &Elementa| mat(e, "sink");
    for (count, mid) in [(1024u64, true), (1025, false)] {
        let mut e = sinks(1500, 3000);
        to_cut(&mut e);
        let sink = sink_of(&e);
        let d: Vec<NodeDesc> = (0..count).map(|k| NodeDesc::new(500_000 + k, sink, behind, h)).collect();
        e.add_many(&d).unwrap();
        assert_eq!(e.cursor().is_mid(), mid, "{count} added where the pass has been");
        loads(&e, "after a batch");
        ticks(&mut e, "after a batch", 12);
    }
}

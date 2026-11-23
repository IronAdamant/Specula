//! Costs counted as what they are: sorts as sorts, a tick's acts and changes together within a
//! bound said before it, the grid brought up to date at most once a tick, and no allocation in a
//! step or an act once warm.

mod common;

use common::{forest, forest_centre, mat, sim};
use specula_core::math::Vec3;
use specula_elementa::config::{act_ops, step_ops};
use specula_elementa::grid::sort_ops;
use specula_elementa::{Act, ActKind, Elementa, Events, NodeDesc, Target};

#[test]
fn putting_ready_nodes_in_order_is_counted_as_a_sort() {
    let mut e = sim(r#"{"elements": 1, "limits": {"nodes": 200, "burning": 10}}"#, 1);
    let wood = mat(&e, "wood");
    for id in 1..=100u64 {
        let (x, z) = (((id - 1) % 10) as f32 * 3.0, ((id - 1) / 10) as f32 * 3.0);
        e.add(NodeDesc::new(id, wood, Vec3::new(x, 0.0, z), Vec3::ONE).fuel(0.5)).unwrap();
    }
    e.act(Act { kind: ActKind::Ignite, target: Target::Sphere { at: Vec3::new(13.5, 0.0, 13.5), radius: 30.0 }, amount: 1.0 }).unwrap();
    let r = loop {
        let r = *e.step();
        if r.spread {
            break r;
        }
    };
    assert_eq!((r.ordered, r.events), (100, 10));
    // 100 ready nodes: placed once and at most 7 comparisons each (on 52be4bf7, 100 in all).
    assert_eq!(r.sort_ops, sort_ops(100) + sort_ops(10));
    assert_eq!(r.ops(), u64::from(r.node_updates) + r.sort_ops + u64::from(r.work) + r.grid_ops);
    assert!(r.ops() <= step_ops(100, e.config().limits.work) + r.grid_ops);
}

#[test]
fn a_tick_of_acts_between_adds_removes_and_moves_stays_within_its_bound() {
    let mut e = sim(r#"{"elements": 1, "materials": {"wood": {"embers": 0.5}}, "limits": {"nodes": 8192, "burning": 512}}"#, 3);
    e.add_many(&forest(&e, 3000, 1)).unwrap();
    let wood = mat(&e, "wood");
    e.set_wind(Vec3::new(3.0, 0.0, 0.0)).unwrap();
    let c = forest_centre(3000);
    e.act(Act { kind: ActKind::Ignite, target: Target::Sphere { at: c, radius: 3.0 }, amount: 1.0 }).unwrap();
    e.step();
    let worst = e.config().worst_case();
    let mut next_id = 100_000u64;
    let mut updates = 0;
    for t in 0..600u64 {
        // Up to the tick's 64 acts, each after an add, a move into another cell or a remove.
        let (mut acts, mut calls) = (0u64, 0u64);
        for k in 0..64u64 {
            match k % 3 {
                0 => {
                    e.add(NodeDesc::new(next_id, wood, c + Vec3::new((k % 9) as f32, 0.0, (t % 7) as f32), Vec3::ONE * 0.3)).unwrap();
                    next_id += 1;
                }
                1 => {
                    let id = 1 + (t * 64 + k) % 2900;
                    let n = e.node(id).unwrap().clone();
                    let dx = if t % 2 == 0 { 2.0 } else { -2.0 };
                    e.move_node(id, n.center + Vec3::new(dx, 0.0, 0.0), n.half).unwrap();
                }
                _ => {
                    assert!(e.remove(next_id - 1));
                }
            }
            calls += 1;
            let r = e.act(Act { kind: ActKind::Wet, target: Target::Sphere { at: c + Vec3::new(k as f32, 0.0, 0.0), radius: 4.0 }, amount: 0.01 }).unwrap();
            assert!(r.scanned, "an act after a change looks at every node instead of updating the grid");
            assert!(r.ops() <= act_ops(e.nodes().len() as u64));
            acts += 1;
        }
        let n = e.nodes().len() as u64;
        let due = e.pending_grid_ops();
        let r = *e.step();
        // Everything the tick cost: the step (with at most one grid update, the one said before
        // it) and every call made since the last tick.
        let total = r.ops() + r.calls_ops;
        let said = step_ops(n, e.config().limits.work) + due + acts * act_ops(n) + calls * Elementa::call_ops(n);
        assert!(total <= said, "tick {t}: {total} ops against {said}");
        assert!(total <= worst.tick_ops(acts, calls), "tick {t}: {total} against the worst case");
        if r.spread {
            assert_eq!(r.grid_ops, due, "the one update of the tick is the one said");
            updates += 1;
        } else {
            assert_eq!(r.grid_ops, 0, "between spread steps nothing updates the grid");
        }
        e.take_events();
    }
    assert_eq!(updates, 100);
}

#[test]
fn a_warm_state_allocates_nothing_in_its_steps_and_acts() {
    let mut e = sim(r#"{"elements": 1, "materials": {"wood": {"embers": 0.5}, "grass": {"embers": 0.3}}, "limits": {"nodes": 4096, "burning": 256}}"#, 9);
    e.add_many(&forest(&e, 3000, 1)).unwrap();
    e.set_wind(Vec3::new(3.0, 0.0, 1.0)).unwrap();
    let c = forest_centre(3000);
    let mut events = Events::default();
    // The same load every tick: a few nodes moved back and forth across cells, a few moved in
    // place, acts, events taken by swapping.
    let tick = |e: &mut Elementa, events: &mut Events, t: u64| {
        if t.is_multiple_of(600) {
            e.act(Act { kind: ActKind::Ignite, target: Target::Sphere { at: c, radius: 2.0 }, amount: 1.0 }).unwrap();
        }
        for k in 0..20u64 {
            let id = 1 + k * 97;
            let n = e.node(id).unwrap().clone();
            let dx = if t.is_multiple_of(2) { 2.0 } else { -2.0 };
            e.move_node(id, n.center + Vec3::new(dx, 0.0, 0.0), n.half).unwrap();
            let id = 2 + k * 89;
            let n = e.node(id).unwrap().clone();
            e.move_node(id, n.center + Vec3::new(0.0, 0.0, 0.001), n.half).unwrap();
        }
        for k in 0..8u32 {
            e.act(Act { kind: ActKind::Wet, target: Target::Sphere { at: c + Vec3::new(k as f32 * 5.0, 0.0, 0.0), radius: 6.0 }, amount: 0.001 }).unwrap();
            e.act(Act { kind: ActKind::Chill, target: Target::Node(u64::from(k) + 1), amount: 0.001 }).unwrap();
        }
        e.step();
        e.swap_events(events);
    };
    for t in 0..1200 {
        tick(&mut e, &mut events, t);
    }
    let (warm, out) = (e.capacities(), events.list.capacity());
    let mut burned = 0;
    for t in 1200..6000 {
        tick(&mut e, &mut events, t);
        burned = burned.max(e.burning());
        assert_eq!((e.capacities(), events.list.capacity()), (warm.clone(), out), "tick {t}: a buffer grew");
    }
    assert!(burned > 50, "a fire ran through it ({burned} at most burning)");
}

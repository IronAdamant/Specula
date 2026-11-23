//! The model: fire spreads at the speed its numbers say, more downwind; fuel runs out; wet resists
//! and dries; stone stops it; fire meets ice; cold and water put it out; embers jump gaps.

mod common;

use common::{mat, sim, TICK_HZ};
use specula_core::math::Vec3;
use specula_elementa::{Act, ActKind, Cause, Elementa, EventKind, NodeDesc, Phase, Target};
use std::collections::BTreeMap;

const LINE: &str = r#"{"elements": 1,
    "materials": {"brush": {"ignite": 0.2, "burn_s": 600, "heat_out": 0.4, "flame": 0.6, "cool": 0.05, "embers": 0}},
    "cell_m": 2, "reach_m": 1.0}"#;

fn ignite(e: &mut Elementa, id: u64) {
    e.act(Act { kind: ActKind::Ignite, target: Target::Node(id), amount: 1.0 }).unwrap();
}

/// Steps until no event of `kind` has come for a while or `ticks` pass; the tick each node
/// first had one.
fn run_until(e: &mut Elementa, ticks: u32, kind: EventKind) -> BTreeMap<u64, u64> {
    let mut at = BTreeMap::new();
    for _ in 0..ticks {
        e.step();
        for ev in e.take_events().list {
            if ev.kind == kind {
                at.entry(ev.id).or_insert(ev.tick);
            }
        }
    }
    at
}

#[test]
fn a_line_burns_at_the_speed_the_model_says_and_faster_downwind() {
    let mut e = sim(LINE, 1);
    let brush = mat(&e, "brush");
    let half = Vec3::new(0.1, 0.1, 0.1);
    for k in 0..41u64 {
        e.add(NodeDesc::new(k, brush, Vec3::new(k as f32, 0.0, 0.0), half)).unwrap();
    }
    e.set_wind(Vec3::new(2.0, 0.0, 0.0)).unwrap();
    // The model's own numbers, before the run, for a fresh pair (gap 0.8 m in a 1 m reach).
    let n = |a: u64, b: u64| e.steps_to_ignite(e.node(a).unwrap(), e.node(b).unwrap()).unwrap();
    let (down, up) = (n(20, 21), n(20, 19));
    let (v_down, v_up) = (e.front_speed(e.node(20).unwrap(), e.node(21).unwrap()).unwrap(), e.front_speed(e.node(20).unwrap(), e.node(19).unwrap()).unwrap());
    ignite(&mut e, 20);
    let at = run_until(&mut e, 40 * up * 6 + 60, EventKind::Ignited);
    let every = u64::from(e.ticks_per_step());
    for k in 20..40u64 {
        assert_eq!(at[&(k + 1)] - at[&k], u64::from(down) * every, "downwind, node {k} to {}", k + 1);
    }
    for k in 1..=20u64 {
        assert_eq!(at[&(k - 1)] - at[&k], u64::from(up) * every, "upwind, node {k} to {}", k - 1);
    }
    // The lean alone is 1.5 downwind and 0.5 upwind (2 m/s at 0.25 per m/s); cooling slows the
    // weaker side more, so the steps' ratio is above the lean's 3.
    assert!(down < up, "{down} steps downwind, {up} upwind");
    let ratio = up as f32 / down as f32;
    assert!((3.0..4.0).contains(&ratio), "steps upwind over downwind: {ratio}");
    assert!((v_down / v_up - ratio).abs() < 1e-4, "speeds {v_down} and {v_up} m/s");
    let measured = 20.0 / ((at[&40] - at[&20]) as f32 / TICK_HZ as f32);
    assert!((measured - v_down).abs() < 1e-3, "measured {measured} m/s downwind, the model says {v_down}");
    // Upward leans too: a node straight above catches sooner than one beside, with no wind.
    let mut f = sim(LINE, 1);
    let b = mat(&f, "brush");
    for (id, p) in [(1u64, Vec3::ZERO), (2, Vec3::new(1.0, 0.0, 0.0)), (3, Vec3::new(0.0, 1.0, 0.0))] {
        f.add(NodeDesc::new(id, b, p, half)).unwrap();
    }
    let side = f.steps_to_ignite(f.node(1).unwrap(), f.node(2).unwrap()).unwrap();
    let above = f.steps_to_ignite(f.node(1).unwrap(), f.node(3).unwrap()).unwrap();
    assert!(above < side, "above {above} steps, beside {side}");
}

#[test]
fn fuel_runs_out_and_a_burnt_thing_never_burns_again() {
    let mut e = sim(r#"{"elements": 1}"#, 1);
    let wood = mat(&e, "wood");
    e.add(NodeDesc::new(7, wood, Vec3::ZERO, Vec3::new(0.5, 0.5, 0.5))).unwrap();
    ignite(&mut e, 7);
    let lit = run_until(&mut e, 6, EventKind::Ignited)[&7];
    let burnt = run_until(&mut e, 60 * 45, EventKind::Burnt)[&7];
    // 40 s of fuel at 10 steps a second: burnt 400 or 401 steps after catching (the fuel is
    // counted down in f32 steps of 0.1 s).
    let steps = (burnt - lit) / 6;
    assert!((400..=401).contains(&steps), "burnt {steps} steps after catching");
    let n = e.node(7).unwrap();
    assert_eq!((n.phase, n.fuel), (Phase::Burnt, 0.0));
    ignite(&mut e, 7);
    assert!(run_until(&mut e, 600, EventKind::Ignited).is_empty(), "a burnt thing never catches again");
    // A node given its own fuel burns for that.
    let mut f = sim(r#"{"elements": 1}"#, 1);
    f.add(NodeDesc::new(1, wood, Vec3::ZERO, Vec3::ONE).fuel(2.0)).unwrap();
    ignite(&mut f, 1);
    let lit = run_until(&mut f, 6, EventKind::Ignited)[&1];
    let burnt = run_until(&mut f, 600, EventKind::Burnt)[&1];
    assert!(((burnt - lit) / 6).abs_diff(20) <= 1);
}

#[test]
fn wet_things_resist_and_dry() {
    let mut e = sim(LINE, 1);
    let brush = mat(&e, "brush");
    let half = Vec3::new(0.1, 0.1, 0.1);
    // Two pairs far apart: a burning source and a dry or a soaked target.
    e.add(NodeDesc::new(1, brush, Vec3::new(0.0, 0.0, 0.0), half)).unwrap();
    e.add(NodeDesc::new(2, brush, Vec3::new(1.0, 0.0, 0.0), half)).unwrap();
    e.add(NodeDesc::new(3, brush, Vec3::new(0.0, 0.0, 50.0), half)).unwrap();
    e.add(NodeDesc::new(4, brush, Vec3::new(1.0, 0.0, 50.0), half).wet(1.0)).unwrap();
    let dry = e.steps_to_ignite(e.node(1).unwrap(), e.node(2).unwrap()).unwrap();
    let wet = e.steps_to_ignite(e.node(3).unwrap(), e.node(4).unwrap()).unwrap();
    assert!(wet > dry, "the soaked one takes longer: {wet} steps against {dry}");
    ignite(&mut e, 1);
    ignite(&mut e, 3);
    let at = run_until(&mut e, 60 * 120, EventKind::Ignited);
    assert_eq!(at[&2] - at[&1], u64::from(dry) * 6);
    assert_eq!(at[&4] - at[&3], u64::from(wet) * 6, "wetness takes the heat first, as the model says");

    // The same heat that lights a dry log only dries a soaked one.
    let mut f = sim(r#"{"elements": 1}"#, 1);
    let wood = mat(&f, "wood");
    f.add(NodeDesc::new(1, wood, Vec3::ZERO, Vec3::ONE)).unwrap();
    f.add(NodeDesc::new(2, wood, Vec3::new(50.0, 0.0, 0.0), Vec3::ONE).wet(1.0)).unwrap();
    for id in [1, 2] {
        f.act(Act { kind: ActKind::Ignite, target: Target::Node(id), amount: 0.6 }).unwrap();
    }
    let lit = run_until(&mut f, 6, EventKind::Ignited);
    assert!(lit.contains_key(&1) && !lit.contains_key(&2), "{lit:?}");
    assert_eq!(f.node(2).unwrap().wet, 0.0, "0.5 of the 0.6 dried it");

    // Left alone, wet dries slowly: 1/600 a second at ambient.
    let mut g = sim(r#"{"elements": 1}"#, 1);
    g.add(NodeDesc::new(1, wood, Vec3::ZERO, Vec3::ONE).wet(0.5)).unwrap();
    for _ in 0..60 * 60 {
        g.step();
    }
    let w = g.node(1).unwrap().wet;
    assert!((w - 0.4).abs() < 1e-3, "after a minute: {w}");
    // Water never dries below its quench.
    let water = mat(&g, "water");
    g.add(NodeDesc::new(2, water, Vec3::new(9.0, 0.0, 0.0), Vec3::ONE)).unwrap();
    for _ in 0..600 {
        g.step();
    }
    assert_eq!(g.node(2).unwrap().wet, 1.0);
}

#[test]
fn stone_stops_a_fire_by_not_being_fuel() {
    let mut e = sim(LINE, 1);
    let (brush, stone) = (mat(&e, "brush"), mat(&e, "stone"));
    let half = Vec3::new(0.1, 0.1, 0.1);
    e.add(NodeDesc::new(1, brush, Vec3::new(0.0, 0.0, 0.0), half)).unwrap();
    e.add(NodeDesc::new(2, stone, Vec3::new(1.0, 0.0, 0.0), half)).unwrap();
    e.add(NodeDesc::new(3, brush, Vec3::new(2.0, 0.0, 0.0), half)).unwrap();
    ignite(&mut e, 1);
    let at = run_until(&mut e, 60 * 60, EventKind::Ignited);
    assert_eq!(at.keys().copied().collect::<Vec<_>>(), [1]);
    assert_eq!(e.node(2).unwrap().phase, Phase::Intact);
    assert!(e.node(2).unwrap().heat > 0.0, "the stone is warmed, and cannot burn");
}

#[test]
fn fire_meets_ice_and_cold_and_water_put_fire_out() {
    // Fire beside ice thaws it.
    let mut e = sim(r#"{"elements": 1}"#, 1);
    let (wood, ice, water) = (mat(&e, "wood"), mat(&e, "ice"), mat(&e, "water"));
    e.add(NodeDesc::new(1, wood, Vec3::ZERO, Vec3::new(0.5, 0.5, 0.5))).unwrap();
    e.add(NodeDesc::new(2, ice, Vec3::new(1.2, 0.0, 0.0), Vec3::new(0.5, 0.5, 0.5))).unwrap();
    assert_eq!(e.node(2).unwrap().phase, Phase::Frozen, "ice starts frozen");
    ignite(&mut e, 1);
    let thawed = run_until(&mut e, 60 * 30, EventKind::Thawed);
    assert!(thawed.contains_key(&2), "the fire thawed the ice");
    assert_eq!(e.node(1).unwrap().phase, Phase::Burning);

    // Cold puts a fire out; so does water.
    let mut f = sim(r#"{"elements": 1}"#, 1);
    f.add(NodeDesc::new(1, wood, Vec3::ZERO, Vec3::ONE)).unwrap();
    f.add(NodeDesc::new(2, wood, Vec3::new(50.0, 0.0, 0.0), Vec3::ONE)).unwrap();
    ignite(&mut f, 1);
    ignite(&mut f, 2);
    run_until(&mut f, 6, EventKind::Ignited);
    // Just lit, it is near white hot (0.96); a chill of 0.6 leaves it above catching (0.35), so
    // it takes more.
    f.act(Act { kind: ActKind::Chill, target: Target::Node(1), amount: 0.65 }).unwrap();
    f.act(Act { kind: ActKind::Wet, target: Target::Node(2), amount: 0.6 }).unwrap();
    for _ in 0..6 {
        f.step();
    }
    let ev = f.take_events();
    let out: Vec<(u64, Option<Cause>)> = ev.list.iter().filter(|e| e.kind == EventKind::Extinguished).map(|e| (e.id, e.by)).collect();
    assert_eq!(out, [(1, Some(Cause::Cold)), (2, Some(Cause::Wet))]);
    assert!(f.node(1).unwrap().fuel > 30.0, "put out, it keeps its fuel");

    // Water freezes in a winter and thaws in a thaw.
    let mut g = sim(r#"{"elements": 1}"#, 1);
    g.add(NodeDesc::new(5, water, Vec3::ZERO, Vec3::ONE)).unwrap();
    g.set_ambient(-0.8).unwrap();
    assert!(run_until(&mut g, 60 * 120, EventKind::Frozen).contains_key(&5));
    g.set_ambient(0.5).unwrap();
    assert!(run_until(&mut g, 60 * 120, EventKind::Thawed).contains_key(&5));
}

#[test]
fn events_come_in_a_fixed_order() {
    let mut e = sim(r#"{"elements": 1}"#, 1);
    let (wood, water) = (mat(&e, "wood"), mat(&e, "water"));
    // Ids out of order on purpose; in one step: 9 catches, 4 freezes, 2 catches, 6 is put out.
    for (id, m, x) in [(9u64, wood, 0.0f32), (4, water, 20.0), (2, wood, 40.0), (6, wood, 60.0)] {
        e.add(NodeDesc::new(id, m, Vec3::new(x, 0.0, 0.0), Vec3::ONE)).unwrap();
    }
    ignite(&mut e, 6);
    run_until(&mut e, 6, EventKind::Ignited);
    ignite(&mut e, 9);
    ignite(&mut e, 2);
    e.act(Act { kind: ActKind::Chill, target: Target::Node(4), amount: 1.0 }).unwrap();
    e.act(Act { kind: ActKind::Wet, target: Target::Node(6), amount: 1.0 }).unwrap();
    for _ in 0..6 {
        e.step();
    }
    let ev = e.take_events();
    let order: Vec<(u64, EventKind)> = ev.list.iter().map(|e| (e.id, e.kind)).collect();
    assert_eq!(order, [(2, EventKind::Ignited), (4, EventKind::Frozen), (6, EventKind::Extinguished), (9, EventKind::Ignited)]);
    assert!(ev.list.iter().all(|x| x.tick == ev.list[0].tick));
    assert!(e.take_events().is_empty(), "taken once");
}

#[test]
fn embers_jump_a_gap_downwind_and_only_in_a_wind() {
    let json = r#"{"elements": 1, "materials": {"wood": {"embers": 1.0}}}"#;
    let run = |wind: f32| {
        let mut e = sim(json, 99);
        let wood = mat(&e, "wood");
        e.add(NodeDesc::new(1, wood, Vec3::ZERO, Vec3::new(0.5, 0.5, 0.5))).unwrap();
        // A wide target 1.5 m past the reach, downwind (+X).
        e.add(NodeDesc::new(2, wood, Vec3::new(4.5, 0.0, 0.0), Vec3::new(1.5, 1.0, 1.5))).unwrap();
        e.set_wind(Vec3::new(wind, 0.0, 0.0)).unwrap();
        ignite(&mut e, 1);
        let mut embers = 0;
        let mut lit = None;
        for _ in 0..60 * 120 {
            embers += e.step().embers;
            if lit.is_none() && e.node(2).unwrap().phase == Phase::Burning {
                lit = Some(e.tick());
            }
        }
        (embers, lit, e.hash())
    };
    let (embers, lit, hash) = run(4.0);
    assert!(embers > 0 && lit.is_some(), "{embers} embers, target lit at {lit:?}");
    assert_eq!(run(4.0), (embers, lit, hash), "the same seed throws the same embers");
    let (calm, never, _) = run(0.0);
    assert_eq!((calm, never), (0, None), "no wind, no embers");
}

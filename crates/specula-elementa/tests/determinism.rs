//! The same seed and inputs give the same hash every tick, in any order of insertion; a save and
//! a load mid-fire continue to the same hash; the prediction equals the run.

mod common;

use common::{forest, forest_centre, sim};
use specula_core::math::Vec3;
use specula_elementa::{Act, ActKind, Elementa, EventKind, NodeDesc, Scenario, Target};

const JSON: &str = r#"{"elements": 1, "materials": {"wood": {"embers": 0.5}, "grass": {"embers": 0.3}}}"#;
const N: usize = 3_000;
const TICKS: u64 = 10_000;

/// The inputs, by tick: lighting at the start, a chill at 3,000, a wind change at 5,000, rain at
/// 7,000.
fn inputs(e: &mut Elementa) {
    let c = forest_centre(N);
    match e.tick() {
        0 => {
            e.set_wind(Vec3::new(2.5, 0.0, 1.0)).unwrap();
            e.act(Act { kind: ActKind::Ignite, target: Target::Sphere { at: c, radius: 2.0 }, amount: 1.0 }).unwrap();
        }
        3_000 => {
            e.act(Act { kind: ActKind::Chill, target: Target::Sphere { at: c + Vec3::new(8.0, 0.0, 0.0), radius: 5.0 }, amount: 1.0 }).unwrap();
        }
        5_000 => e.set_wind(Vec3::new(-1.0, 0.0, 4.0)).unwrap(),
        7_000 => {
            e.act(Act { kind: ActKind::Wet, target: Target::Sphere { at: c, radius: 20.0 }, amount: 0.7 }).unwrap();
        }
        _ => {}
    }
}

/// Runs from `e`'s tick to `until`, hashing every tick; counts ignitions. With `loads`, every
/// tick the state's own save loads to the same hash (what the crate makes, it accepts).
fn run_with(e: &mut Elementa, until: u64, loads: bool) -> (Vec<u64>, u64) {
    let mut hashes = Vec::new();
    let mut caught = 0;
    while e.tick() < until {
        inputs(e);
        e.step();
        caught += e.take_events().count(EventKind::Ignited);
        hashes.push(e.hash());
        if loads {
            let l = Elementa::load(&e.save()).unwrap_or_else(|err| panic!("tick {}: the state's own save is refused: {err}", e.tick()));
            assert_eq!(l.hash(), e.hash());
        }
    }
    (hashes, caught)
}

fn run(e: &mut Elementa, until: u64) -> (Vec<u64>, u64) {
    run_with(e, until, false)
}

fn build(order: &str) -> Elementa {
    let mut e = sim(JSON, 42);
    let trees = forest(&e, N, 100);
    match order {
        "batch" => {
            e.add_many(&trees).unwrap();
        }
        "reverse" => {
            for d in trees.iter().rev() {
                e.add(d.clone()).unwrap();
            }
        }
        _ => {
            // Interleaved: odd positions first, then even, in batches of 7.
            let odd: Vec<NodeDesc> = trees.iter().skip(1).step_by(2).cloned().collect();
            let even: Vec<NodeDesc> = trees.iter().step_by(2).cloned().collect();
            for chunk in odd.chunks(7).chain(even.chunks(7)) {
                e.add_many(chunk).unwrap();
            }
        }
    }
    e
}

#[test]
fn the_same_inputs_give_the_same_hash_every_tick_in_any_order_of_insertion() {
    // The reference run: its own save loads at every one of its 10,000 ticks.
    let (a, caught) = run_with(&mut build("batch"), TICKS, true);
    assert!(caught > 300, "the fire really ran: {caught} caught");
    let (b, _) = run(&mut build("batch"), TICKS);
    assert_eq!(a.len(), TICKS as usize);
    assert!(a == b, "two runs differ first at tick {:?}", a.iter().zip(&b).position(|(x, y)| x != y));
    for order in ["reverse", "interleaved"] {
        let (c, _) = run(&mut build(order), TICKS);
        assert!(a == c, "{order} insertion differs first at tick {:?}", a.iter().zip(&c).position(|(x, y)| x != y));
    }
    // Another seed throws other embers: the hash is not blind to the stream.
    let mut other = sim(JSON, 43);
    other.add_many(&forest(&other, N, 100)).unwrap();
    let (d, _) = run(&mut other, TICKS);
    assert_ne!(a.last(), d.last());
}

#[test]
fn a_save_and_load_mid_fire_continue_to_the_same_hash() {
    let mut e = build("batch");
    run_with(&mut e, 2_000, true);
    assert!(e.burning() > 0, "saved mid-fire");
    let bytes = e.save();
    let mut loaded = Elementa::load(&bytes).unwrap();
    assert_eq!(loaded.hash(), e.hash());
    assert_eq!(loaded.save(), bytes, "a load saves to the same bytes");
    assert_eq!(loaded.take_events(), e.take_events(), "waiting events are part of the save");
    assert_eq!(loaded.last_report().burning, e.burning(), "a loaded state says what burns before its next spread step");
    let (a, b) = (*e.step(), *loaded.step());
    assert!(!a.spread, "this tick is between spread steps");
    assert_eq!(a, b, "the same report on the tick after a load");
    let (a, _) = run_with(&mut e, 2_600, true);
    let (b, _) = run_with(&mut loaded, 2_600, true);
    assert!(a == b, "differ first at tick {:?}", a.iter().zip(&b).position(|(x, y)| x != y));
}

#[test]
fn the_prediction_equals_the_run() {
    let mut e = build("batch");
    e.set_wind(Vec3::new(3.0, 0.0, 0.0)).unwrap();
    let light = Act { kind: ActKind::Ignite, target: Target::Sphere { at: forest_centre(N), radius: 2.0 }, amount: 1.0 };
    let s = Scenario { acts: vec![light], wind: None, ambient: None, ticks: 3_600, max_ops: u64::MAX };
    let (before, bytes) = (e.hash(), e.save());
    let p = e.check(&s).unwrap();
    assert!(p.ignited > 50 && !p.cut, "{p:?}");
    assert!(p.ops <= p.bound_ops, "{} ops against the bound {}", p.ops, p.bound_ops);
    assert_eq!(e.hash(), before, "a prediction leaves the state alone");
    assert_eq!(e.save(), bytes);
    // The run itself.
    e.act(light).unwrap();
    let (mut ignited, mut burnt) = (0, 0);
    for _ in 0..3_600 {
        e.step();
        let ev = e.take_events();
        ignited += ev.count(EventKind::Ignited);
        burnt += ev.count(EventKind::Burnt);
    }
    assert_eq!((p.ignited, p.burnt, p.burning_at_end, p.hash), (ignited, burnt, e.burning(), e.hash()));
    assert!(p.to_json().to_string().len() < 2048);
    // A ceiling on ops stops it and says so.
    let small = e.check(&Scenario { max_ops: 1_000, ..s.clone() }).unwrap();
    assert!(small.cut && small.ticks < 3_600);
    assert_eq!(e.check(&Scenario { ticks: 0, ..s }).unwrap_err().code, "out_of_range");
}

#[test]
fn a_prediction_starts_with_none_of_the_ticks_acts() {
    let mut e = sim(JSON, 1);
    e.add_many(&forest(&e, 100, 1)).unwrap();
    let one = Act { kind: ActKind::Wet, target: Target::Node(1), amount: 0.01 };
    for _ in 0..specula_elementa::sim::MAX_ACTS_PER_TICK {
        e.act(one).unwrap();
    }
    assert_eq!(e.act(one).unwrap_err().code, "limit", "the live tick has spent its acts");
    let light = Act { kind: ActKind::Ignite, target: Target::Node(5), amount: 1.0 };
    let p = e.check(&Scenario { acts: vec![light], wind: None, ambient: None, ticks: 60, max_ops: u64::MAX }).unwrap();
    assert!(p.ignited >= 1, "{p:?}");
}

#[test]
fn mid_cut_moves_adds_and_acts_give_one_hash_in_any_call_order_and_through_a_load_every_tick() {
    // The fourth reviewer's `order`, with acts: a torch whose query the smallest budget cuts
    // every pass, 2,000 sinks, and 400 walkers moved across the cut and out of the reach by a
    // fixed plan, sinks added and removed; an act on a sphere through the walkers every tick.
    // Run a gives the calls in id order, b in reverse, and c is saved and loaded every tick
    // (so its grid is never current when it acts, and it looks at every node). Every tick: the
    // same hash in all three, each act reached the same nodes in all three, and the state's own
    // save loads.
    use specula_elementa::Phase;
    let json = r#"{"elements": 1, "materials": {"torch": {"ignite": 0.1, "flame": 0.9, "burn_s": 3600, "heat_out": 0.001, "cool": 0},
        "sink": {"cool": 0}}, "cell_m": 2, "reach_m": 1.0, "limits": {"nodes": 8192, "burning": 8192, "work": 1024}}"#;
    let h = Vec3::new(0.01, 0.01, 0.01);
    let mk = || {
        let mut e = sim(json, 1);
        let (torch, sink) = (common::mat(&e, "torch"), common::mat(&e, "sink"));
        let mut d = vec![NodeDesc::new(0, torch, Vec3::new(0.5, 0.5, 0.5), h).heat(1.0)];
        for i in 1..=2000u64 {
            d.push(NodeDesc::new(i * 10, sink, Vec3::new(1.0, 0.5, 0.5), h));
        }
        for k in 0..400u64 {
            d.push(NodeDesc::new(100_000 + k, sink, Vec3::new(-0.5 + (k % 20) as f32 * 0.07, 0.5, 0.2 + (k / 20) as f32 * 0.05), h));
        }
        e.add_many(&d).unwrap();
        e
    };
    let plan = |t: u64| -> Vec<(u64, Vec3)> {
        (0..400u64)
            .filter(|k| (k * 7 + t).is_multiple_of(5))
            .map(|k| (100_000 + k, Vec3::new(((k * 13 + t * 3) % 40) as f32 * 0.1 - 2.0, 0.5, 0.2 + (k / 20) as f32 * 0.05)))
            .collect()
    };
    let (mut a, mut b, mut c) = (mk(), mk(), mk());
    let sink = common::mat(&a, "sink");
    let (mut mids, mut reached) = (0u32, 0u64);
    for t in 0..6 * 200u64 {
        let p = plan(t);
        for &(id, at) in &p {
            if a.node(id).is_some() {
                a.move_node(id, at, h).unwrap();
                c.move_node(id, at, h).unwrap();
            }
        }
        for &(id, at) in p.iter().rev() {
            if b.node(id).is_some() {
                b.move_node(id, at, h).unwrap();
            }
        }
        if t % 7 == 3 {
            let (id, gone) = (200_000 + t, 100_000 + (t % 400));
            for e in [&mut a, &mut c] {
                e.add(NodeDesc::new(id, sink, Vec3::new(1.0, 0.5, 0.5), h)).unwrap();
                e.remove(gone);
            }
            b.remove(gone);
            b.add(NodeDesc::new(id, sink, Vec3::new(1.0, 0.5, 0.5), h)).unwrap();
        }
        let at = Vec3::new(((t * 11) % 40) as f32 * 0.1 - 2.0, 0.5, 0.6);
        let act = Act { kind: ActKind::Wet, target: Target::Sphere { at, radius: 0.3 + (t % 9) as f32 * 0.1 }, amount: 0.001 };
        let ra = a.act(act).unwrap();
        let rb = b.act(act).unwrap();
        let rc = c.act(act).unwrap();
        assert_eq!((ra.nodes, rb.nodes), (rc.nodes, rc.nodes), "tick {t}: an act reached other nodes");
        reached += u64::from(ra.nodes);
        a.step();
        b.step();
        c.step();
        let bytes = c.save();
        c = Elementa::load(&bytes).unwrap_or_else(|e| panic!("tick {t}: the state's own save is refused: {e}"));
        for e in [&a, &b] {
            let l = Elementa::load(&e.save()).unwrap_or_else(|e| panic!("tick {t}: the state's own save is refused: {e}"));
            assert_eq!(l.hash(), e.hash());
        }
        mids += u32::from(a.cursor().is_mid());
        assert_eq!(a.hash(), b.hash(), "tick {t}: the calls in reverse order differ");
        assert_eq!(a.hash(), c.hash(), "tick {t}: a load every tick differs");
    }
    assert!(mids > 500 && reached > 1000, "{mids} ticks mid-cut, {reached} nodes reached by acts");
    assert_eq!(a.node(0).unwrap().phase, Phase::Burning);
}

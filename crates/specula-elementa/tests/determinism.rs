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

/// Runs from `e`'s tick to `until`, hashing every tick; counts ignitions.
fn run(e: &mut Elementa, until: u64) -> (Vec<u64>, u64) {
    let mut hashes = Vec::new();
    let mut caught = 0;
    while e.tick() < until {
        inputs(e);
        e.step();
        caught += e.take_events().count(EventKind::Ignited);
        hashes.push(e.hash());
    }
    (hashes, caught)
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
    let (a, caught) = run(&mut build("batch"), TICKS);
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
    run(&mut e, 2_000);
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
    let (a, _) = run(&mut e, 2_600);
    let (b, _) = run(&mut loaded, 2_600);
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

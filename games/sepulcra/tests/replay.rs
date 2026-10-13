//! The replay suite (DESIGN.md, "How we test it"): scripted layouts must reach known outcomes,
//! the same seed and inputs must give the same hash on every tick, and a couple of full runs
//! are pinned by their final hash.

use std::sync::Arc;

use sepulcra::content::Content;
use sepulcra::script::{replay, run, Player, Recording, RunEnd, RunReport, Script};
use sepulcra::sim::{Event, Outcome, State};
use sepulcra::{EnemyKind, ErrorCode, TICK_HZ};

const CENTRE: &str = include_str!("../data/layouts/centre.json");
const NO_AIR: &str = include_str!("../data/layouts/no_air.json");
const FULL_BLOCK: &str = include_str!("../data/layouts/full_block.json");
const NONE: &str = include_str!("../data/layouts/none.json");

fn content() -> Arc<Content> {
    Arc::new(Content::builtin())
}

fn script(c: &Content, text: &str) -> Script {
    Script::parse(text, c, "layout").expect("layout parses")
}

fn play(c: &Arc<Content>, layout: &str, seed: u64) -> RunReport {
    let r = run(c.clone(), seed, &script(c, layout), None, |_, _| {});
    assert!(r.skipped.is_empty(), "layout entries skipped: {:?}", r.skipped);
    r
}

#[test]
fn no_defence_loses_early() {
    let c = content();
    for seed in 1..=10 {
        let r = play(&c, NONE, seed);
        assert_eq!(r.outcome, Outcome::Lost, "seed {seed}");
        assert!(r.wave <= 2, "seed {seed}: lost in wave {}", r.wave);
    }
}

#[test]
fn no_anti_air_loses_to_wraiths_by_wave_6() {
    let c = content();
    let wraith = c.enemy_kind("wraith").unwrap();
    for seed in 1..=10 {
        let mut lost_to = [0i32; 8];
        let r = run(c.clone(), seed, &script(&c, NO_AIR), None, |_, ev| {
            for e in ev {
                if let Event::TombstonesLost { kind: EnemyKind(k), lost, .. } = e {
                    lost_to[usize::from(*k)] += lost;
                }
            }
            // Once the run is lost, nothing else happens in that tick.
            if let Some(i) = ev.iter().position(|e| *e == Event::Lost) {
                let after = &ev[i + 1..];
                assert!(after.iter().all(|e| matches!(e, Event::TombstonesLost { .. })), "seed {seed}: events after Lost: {after:?}");
            }
        });
        assert_eq!(r.outcome, Outcome::Lost, "seed {seed}");
        assert!((4..=6).contains(&r.wave), "seed {seed}: lost in wave {}", r.wave);
        let to_wraiths = lost_to[usize::from(wraith.0)];
        assert!(to_wraiths >= 8, "seed {seed}: wraiths took {to_wraiths} of 10 tombstones ({lost_to:?})");
    }
}

#[test]
fn centre_heavy_layout_survives_all_ten_waves() {
    let c = content();
    for seed in 1..=10 {
        let r = play(&c, CENTRE, seed);
        assert_eq!(r.outcome, Outcome::Won, "seed {seed}");
        assert_eq!(r.wave, 10);
        assert!(r.tombstones >= 1);
        let minutes = f64::from(r.ticks) / f64::from(TICK_HZ) / 60.0;
        assert!((11.0..13.5).contains(&minutes), "seed {seed}: a run lasts about 12 minutes, got {minutes:.1}");
    }
}

#[test]
fn a_full_wall_block_makes_enemies_break_a_wall() {
    let c = content();
    let (mut attacks, mut broken) = (0, 0);
    let r = run(c.clone(), 1, &script(&c, FULL_BLOCK), None, |_, ev| {
        for e in ev {
            match e {
                Event::AttackingWall { .. } => attacks += 1,
                Event::WallBroken { .. } => broken += 1,
                _ => {}
            }
        }
    });
    assert!(attacks > 0 && broken > 0, "attacks {attacks}, broken {broken}");
    assert_eq!(r.outcome, Outcome::Lost, "walls alone don't hold: they only buy time");
}

#[test]
fn same_seed_and_inputs_give_the_same_hash_every_tick() {
    let c = content();
    let s = script(&c, CENTRE);
    let (mut a, mut b) = (State::new(c.clone(), 42), State::new(c.clone(), 42));
    let (mut pa, mut pb) = (Player::new(s.clone()), Player::new(s));
    let mut ev = Vec::new();
    while a.outcome() == Outcome::Running {
        let ca = pa.drive(&mut a, &mut ev);
        let cb = pb.drive(&mut b, &mut ev);
        assert_eq!(ca, cb);
        let (ea, eb) = (a.step(&[]), b.step(&[]));
        assert_eq!(ea, eb, "tick {}", a.tick());
        assert_eq!(a.hash(), b.hash(), "tick {}", a.tick());
    }
    assert_eq!(b.outcome(), a.outcome());
}

#[test]
fn different_seeds_give_different_runs() {
    let c = content();
    let hashes: Vec<u64> = (1..=4).map(|seed| run(c.clone(), seed, &script(&c, CENTRE), Some(3), |_, _| {}).hash).collect();
    for i in 0..hashes.len() {
        for j in i + 1..hashes.len() {
            assert_ne!(hashes[i], hashes[j], "seeds {} and {}", i + 1, j + 1);
        }
    }
    assert_ne!(State::new(c.clone(), 1).hash(), State::new(c, 2).hash());
}

#[test]
fn a_recording_replays_bit_for_bit_and_checks_itself() {
    let c = content();
    // A seed above 2^53 must survive JSON (it is written as a decimal string).
    for seed in [5, u64::MAX, (1 << 53) + 1] {
        let r = run(c.clone(), seed, &script(&c, CENTRE), Some(4), |_, _| {});
        assert!(!r.journal.is_empty());
        let rec = r.recording(&c);
        let text = rec.to_json(&c).to_pretty();
        let back = Recording::from_json(&text, &c, "rec.json").expect("recording parses");
        assert_eq!(back, rec, "seed {seed}");
        let state = back.verify(c.clone()).expect("replay matches");
        assert_eq!((state.tick(), state.hash()), (r.ticks, r.hash));
        let replayed = replay(c.clone(), seed, &rec.journal, r.ticks, |_, _| {});
        assert_eq!(replayed.hash(), r.hash);
    }
}

#[test]
fn a_recording_refuses_other_content_and_reports_mismatches() {
    let c = content();
    let r = run(c.clone(), 3, &script(&c, CENTRE), Some(2), |_, _| {});
    let rec = r.recording(&c);
    let text = rec.to_json(&c).to_pretty();

    let other = Content::from_texts(
        sepulcra::content::SLAB_01_JSON,
        sepulcra::content::STONES_JSON,
        sepulcra::content::ENEMIES_JSON,
        &sepulcra::content::WAVES_JSON.replace("\"start_dust\": 150", "\"start_dust\": 151"),
    )
    .unwrap();
    let e = Recording::from_json(&text, &other, "rec.json").unwrap_err();
    assert_eq!((e.code, e.field.as_str()), (ErrorCode::ContentMismatch, "content_hash"));

    let e = Recording::from_json(&text.replacen("\"seed\": \"3\"", "\"seed\": 9007199254740993", 1), &c, "rec.json").unwrap_err();
    assert_eq!((e.code, e.field.as_str()), (ErrorCode::BadType, "seed"), "a double can't hold 2^53 + 1");

    let mut tampered = rec.clone();
    tampered.journal.pop();
    let m = tampered.verify(c.clone()).unwrap_err();
    assert_eq!(m.expected, RunEnd { tick: r.ticks, hash: r.hash });
    assert_ne!(m.got.hash, m.expected.hash);
}

#[test]
fn a_clone_continues_identically() {
    let c = content();
    let s = script(&c, CENTRE);
    let mut a = State::new(c.clone(), 11);
    let mut p = Player::new(s);
    let mut ev = Vec::new();
    for _ in 0..30 * 200 {
        p.drive(&mut a, &mut ev);
        a.step(&[]);
    }
    let mut b = a.clone();
    let mut q = p.clone();
    for _ in 0..30 * 200 {
        p.drive(&mut a, &mut ev);
        q.drive(&mut b, &mut ev);
        a.step(&[]);
        b.step(&[]);
    }
    assert_eq!(a.hash(), b.hash());
}

#[test]
fn layouts_round_trip_as_json() {
    let c = content();
    for text in [CENTRE, NO_AIR, FULL_BLOCK, NONE] {
        let s = script(&c, text);
        let again = Script::parse(&s.to_json(&c).to_pretty(), &c, "again").unwrap();
        assert_eq!(s, again);
    }
}

/// Pinned full runs. These hashes change whenever the rules or the default content change; if
/// that was intended, update them in the same commit and say so. If they change unexpectedly,
/// the simulation is no longer deterministic or a change leaked into the rules.
#[test]
fn pinned_runs() {
    let c = content();
    let cases: [(&str, &str, u64, Outcome, u32, i32, u64); 3] = [
        ("centre", CENTRE, 1, Outcome::Won, 21838, 5, 0x1061_f78a_7bb1_17db),
        ("no_air", NO_AIR, 7, Outcome::Lost, 11782, 0, 0x6ae4_2ac9_add7_0fde),
        ("none", NONE, 3, Outcome::Lost, 3385, 0, 0x2b89_1f15_0a89_44b0),
    ];
    let mut report = String::new();
    let mut ok = true;
    for (name, layout, seed, outcome, ticks, tombs, hash) in cases {
        let r = play(&c, layout, seed);
        report.push_str(&format!("(\"{name}\", {}, {seed}, Outcome::{:?}, {}, {}, 0x{:016x}),\n", name.to_uppercase(), r.outcome, r.ticks, r.tombstones, r.hash));
        ok &= (r.outcome, r.ticks, r.tombstones, r.hash) == (outcome, ticks, tombs, hash);
    }
    assert!(ok, "pinned runs changed; now:\n{report}");
}

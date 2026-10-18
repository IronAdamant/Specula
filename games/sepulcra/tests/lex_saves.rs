//! Save games for Lex runs (docs/design/SAVES.md): the Sepulcra slice saved mid-wave, with stones
//! firing and projectiles in flight, restores to the same state hash and steps on identically.

use specula_core::Json;
use specula_lex::{compile, Document, Natives, Sim};
use std::sync::Arc;

#[test]
fn the_slice_saved_mid_wave_continues_identically() {
    let doc = Document::parse(include_str!("../lex/slice.lex.json")).unwrap();
    let rules = Arc::new(compile(&doc, &Default::default()).unwrap_or_else(|d| panic!("{d:#?}")).rules);
    for seed in [1u64, 9, 23] {
        let mut a = Sim::new(Arc::clone(&rules), seed, &Natives::new()).unwrap();
        for (stone, site) in [("warding_1", "top:8,14"), ("rune_pillar_1", "top:10,14")] {
            let c = a.parse_command("place", &Json::obj([("stone", Json::from(stone)), ("site", Json::from(site))])).unwrap();
            a.step(&[c]);
        }
        for _ in 0..2500 {
            a.step(&[]);
        }
        let bytes = a.save_state();
        let mut b = Sim::restore(Arc::clone(&rules), &Natives::new(), &bytes).unwrap();
        assert_eq!(b.hash(), a.hash(), "seed {seed}");
        for t in 0..4000 {
            a.step(&[]);
            b.step(&[]);
            assert_eq!(b.hash(), a.hash(), "seed {seed}: diverged {t} ticks after the restore");
        }
        assert_eq!(a.outcome(), b.outcome());
    }
}

/// Saves taken all through a run, the last wave and the tick before the outcome included, all
/// restore (wave numbers are 1-based: the last wave once failed to load).
#[test]
fn saves_all_through_a_run_restore() {
    let doc = Document::parse(include_str!("../lex/slice.lex.json")).unwrap();
    let rules = Arc::new(compile(&doc, &Default::default()).unwrap_or_else(|d| panic!("{d:#?}")).rules);
    let mut a = Sim::new(Arc::clone(&rules), 1, &Natives::new()).unwrap();
    for (stone, site) in [("warding_1", "top:8,14"), ("rune_pillar_1", "top:10,14")] {
        let c = a.parse_command("place", &Json::obj([("stone", Json::from(stone)), ("site", Json::from(site))])).unwrap();
        a.step(&[c]);
    }
    let waves = rules.waves.as_ref().map_or(0, |w| w.waves.len()) as u16;
    let mut saw_last = false;
    let mut t = 0u32;
    while a.outcome().is_none() && t < 400_000 {
        let before = a.save_state();
        a.step(&[]);
        t += 1;
        if t % 150 == 0 || a.outcome().is_some() || (a.state().wave == waves && !saw_last) {
            saw_last |= a.state().wave == waves;
            Sim::restore(Arc::clone(&rules), &Natives::new(), &a.save_state()).unwrap_or_else(|e| panic!("tick {}: {e}", a.tick()));
            Sim::restore(Arc::clone(&rules), &Natives::new(), &before).unwrap_or_else(|e| panic!("the tick before {}: {e}", a.tick()));
        }
    }
    assert!(a.outcome().is_some() && saw_last, "the run reached its last wave and an outcome");
}

/// The whole game's rules (natives, phase `order`, wave groups' `with`): saved at many points of
/// a centre-layout game, each restore has the same hash and steps on with the same hashes and
/// the same work per tick as the run it was saved from (the hooks' caches start cold after a
/// restore, and charge the same).
#[test]
fn the_whole_game_saved_anywhere_continues_identically() {
    let content = Arc::new(sepulcra::content::Content::builtin());
    let rules = sepulcra::lex::compiled(&[]).unwrap();
    assert!(rules.ordered && rules.with_groups, "the set uses order and with");
    let script = sepulcra::script::Script::parse(include_str!("../data/layouts/centre.json"), &content, "centre").unwrap();
    let journal = sepulcra::script::run(Arc::clone(&content), 7, &script, None, |_, _| {}).journal;
    let mut a = sepulcra::lex::sim(Arc::clone(&rules), 7).unwrap();
    let mut k = 0;
    while a.outcome().is_none() && a.tick() < 20_000 {
        let mut now = Vec::new();
        while k < journal.len() && journal[k].0 <= a.tick() {
            now.push(sepulcra::lex::command_call(&a, &content, &journal[k].1).unwrap());
            k += 1;
        }
        a.step(&now);
        if a.tick() % 2_500 == 0 {
            let bytes = a.save_state();
            let mut b = Sim::restore(Arc::clone(&rules), &sepulcra::lex::natives(), &bytes).unwrap();
            assert_eq!(b.hash(), a.hash(), "tick {}", a.tick());
            assert_eq!(b.save_state(), bytes, "canonical bytes");
            let mut c = a.clone();
            for _ in 0..300 {
                c.step(&[]);
                b.step(&[]);
                assert_eq!((b.hash(), b.tick_work()), (c.hash(), c.tick_work()), "after a restore at tick {}", a.tick());
            }
        }
    }
}

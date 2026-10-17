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

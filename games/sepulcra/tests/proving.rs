//! The proving ground (DESIGN.md, "Elemental stones"): every layout is deterministic, a save at
//! any moment continues the same, and the shipped game is untouched by the elements.

use std::sync::Arc;

use sepulcra::content::Content;
use sepulcra::script::{run, Player, Script};
use sepulcra::sim::{Event, State};

const LAYOUTS: [(&str, &str); 16] = [
    ("baseline", include_str!("../data/proving/layouts/baseline.json")),
    ("fire", include_str!("../data/proving/layouts/fire.json")),
    ("lightning", include_str!("../data/proving/layouts/lightning.json")),
    ("frost", include_str!("../data/proving/layouts/frost.json")),
    ("wind", include_str!("../data/proving/layouts/wind.json")),
    ("earth", include_str!("../data/proving/layouts/earth.json")),
    ("fire_lightning", include_str!("../data/proving/layouts/fire_lightning.json")),
    ("fire_frost", include_str!("../data/proving/layouts/fire_frost.json")),
    ("fire_wind", include_str!("../data/proving/layouts/fire_wind.json")),
    ("fire_earth", include_str!("../data/proving/layouts/fire_earth.json")),
    ("lightning_frost", include_str!("../data/proving/layouts/lightning_frost.json")),
    ("lightning_wind", include_str!("../data/proving/layouts/lightning_wind.json")),
    ("lightning_earth", include_str!("../data/proving/layouts/lightning_earth.json")),
    ("frost_wind", include_str!("../data/proving/layouts/frost_wind.json")),
    ("frost_earth", include_str!("../data/proving/layouts/frost_earth.json")),
    ("wind_earth", include_str!("../data/proving/layouts/wind_earth.json")),
];

fn content() -> Arc<Content> {
    Arc::new(Content::proving())
}

#[test]
fn every_proving_layout_parses_places_everything_and_strands_nobody() {
    let c = content();
    for (name, text) in LAYOUTS {
        let s = Script::parse(text, &c, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let r = run(Arc::clone(&c), 1, &s, None, |_, _| {});
        assert!(r.skipped.is_empty(), "{name}: entries refused {:?}", r.skipped);
        assert_eq!(r.stranded(), 0, "{name}");
    }
}

/// The first proving sweep found runs that never ended: a gust every few seconds pushed the last
/// shamblers of a wave back as far as they walked between gusts. The dead lean into the wind
/// (`shove_fade`): every run of every layout ends, won or lost.
#[test]
fn the_wind_cannot_hold_a_wave_for_ever() {
    let c = content();
    for (name, text) in LAYOUTS.iter().filter(|(n, _)| n.contains("wind")) {
        let s = Script::parse(text, &c, name).unwrap();
        for seed in 1..=12 {
            let r = run(Arc::clone(&c), seed, &s, None, |_, _| {});
            assert_ne!(r.outcome, sepulcra::Outcome::Running, "{name} seed {seed}: unfinished at tick {}", r.ticks);
        }
    }
}

#[test]
fn the_same_seed_gives_the_same_run_tick_by_tick() {
    let c = content();
    for (name, text) in LAYOUTS {
        let s = Script::parse(text, &c, name).unwrap();
        let mut a = Vec::new();
        let ra = run(Arc::clone(&c), 3, &s, None, |st, _| a.push(st.hash()));
        let mut b = Vec::new();
        let rb = run(Arc::clone(&c), 3, &s, None, |st, _| b.push(st.hash()));
        assert_eq!(a, b, "{name}: tick by tick");
        assert_eq!((ra.hash, ra.ticks), (rb.hash, rb.ticks));
        let other = run(Arc::clone(&c), 4, &s, None, |_, _| {});
        assert_ne!(other.hash, ra.hash, "{name}: another seed is another run");
    }
}

/// A save taken at moments where the elements are busy (fire on the slab, a wave of the
/// exploding dead, the colossus pushed) loads and steps exactly like the original.
#[test]
fn a_save_mid_fire_and_mid_wave_continues_the_same() {
    let c = content();
    for name in ["fire_wind", "lightning_frost", "wind_earth", "fire_earth"] {
        let text = LAYOUTS.iter().find(|(n, _)| *n == name).unwrap().1;
        let script = Script::parse(text, &c, name).unwrap();
        let mut s = State::new(Arc::clone(&c), 7);
        let mut p = Player::new(script);
        let mut ev = Vec::new();
        let mut saved = 0;
        let mut fire_seen = false;
        for _ in 0..9000 {
            let _ = p.drive(&mut s, &mut ev);
            ev.clear();
            s.step_into(&[], &mut ev);
            let busy = s.blaze().burning(s.content()).count() > 0 || s.enemies().iter().any(|e| e.el != Default::default());
            fire_seen |= ev.iter().any(|e| matches!(e, Event::CellCaught { .. }));
            if busy && s.tick() % 450 == 0 {
                let bytes = s.to_bytes();
                let mut b = State::from_bytes(Arc::clone(&c), &bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(b.to_bytes(), bytes, "{name}: canonical bytes");
                let mut a = s.clone();
                let (mut ea, mut eb) = (Vec::new(), Vec::new());
                for t in 0..900 {
                    ea.clear();
                    eb.clear();
                    a.step_into(&[], &mut ea);
                    b.step_into(&[], &mut eb);
                    assert_eq!((a.hash(), &ea), (b.hash(), &eb), "{name}: diverged {t} ticks after a save at tick {}", s.tick());
                }
                saved += 1;
            }
        }
        assert!(saved >= 3, "{name}: saved {saved} times while the elements were busy");
        assert!(fire_seen || !name.contains("fire"), "{name}: the grass caught at some point");
    }
}

#[test]
fn damaged_proving_saves_are_errors_never_panics() {
    let c = content();
    let text = LAYOUTS.iter().find(|(n, _)| *n == "fire_wind").unwrap().1;
    let script = Script::parse(text, &c, "fire_wind").unwrap();
    let mut s = State::new(Arc::clone(&c), 7);
    let mut p = Player::new(script);
    let mut ev = Vec::new();
    for _ in 0..3000 {
        let _ = p.drive(&mut s, &mut ev);
        ev.clear();
        s.step_into(&[], &mut ev);
    }
    let bytes = s.to_bytes();
    for cut in (0..bytes.len()).step_by(7) {
        assert!(State::from_bytes(Arc::clone(&c), &bytes[..cut]).is_err(), "cut {cut}");
    }
    // A shipped-game save is refused by the proving ground, and the other way round.
    let shipped = Arc::new(Content::builtin());
    assert_eq!(State::from_bytes(Arc::clone(&shipped), &bytes).unwrap_err().code, "content_changed");
    assert_eq!(State::from_bytes(c, &State::new(shipped, 1).to_bytes()).unwrap_err().code, "content_changed");
}

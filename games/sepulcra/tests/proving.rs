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
            if busy && s.tick().is_multiple_of(450) {
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

/// The shipped sweep prints the lines it printed before the elements (the headless example's
/// summary is `script::sweep_lines`): the same kinds of line, in the same order, and no "all
/// runs" line; the proving ground's sweep has that one more.
#[test]
fn the_shipped_sweep_prints_the_lines_it_printed_before_the_elements() {
    let kinds = |content: &Arc<Content>, layouts: &[(&str, &str)]| -> Vec<String> {
        let mut reports = Vec::new();
        for (name, text) in layouts {
            let s = Script::parse(text, content, name).unwrap();
            reports.push(run(Arc::clone(content), 1, &s, None, |_, _| {}));
        }
        assert!(reports.iter().any(|r| r.outcome == sepulcra::Outcome::Won) && reports.iter().any(|r| r.outcome == sepulcra::Outcome::Lost), "a win and a loss, so every kind of line shows");
        sepulcra::script::sweep_lines(content, &reports).iter().map(|l| l.split(':').next().unwrap().split(' ').next().unwrap().to_owned()).collect()
    };
    let shipped = Arc::new(Content::builtin());
    let got = kinds(&shipped, &[("centre", include_str!("../data/layouts/centre.json")), ("none", include_str!("../data/layouts/none.json"))]);
    assert_eq!(got, ["won", "wins", "losses", "mean", "stranded"], "the lines of the sweep before the elements");
    let proving = content();
    let got = kinds(&proving, &[("baseline", LAYOUTS[0].1), ("wind", LAYOUTS[4].1)]);
    assert_eq!(got, ["won", "wins", "losses", "all", "mean", "stranded"]);
}

/// A run stopped early leaves entries never applied, and a won run none. (How a layout waits, for
/// dust and for one of the dead on a cell, is held below and in the rules' own tests: under the
/// one-shove-at-a-time wind no wave-1 straggler stands on the south gate when the wind layout's
/// wall there is due, as one did before.)
#[test]
fn a_run_says_what_its_layout_never_applied() {
    let c = content();
    let wind = Script::parse(LAYOUTS[4].1, &c, "wind").unwrap();
    let short = run(Arc::clone(&c), 1, &wind, Some(1), |_, _| {});
    assert!(short.waits.never_applied > 0, "stopped after wave 1, the later entries were never applied: {:?}", short.waits);
    let all = Script::parse(LAYOUTS[0].1, &c, "baseline").unwrap();
    let full = run(Arc::clone(&c), 1, &all, None, |_, _| {});
    assert_eq!(full.waits.never_applied, 0, "a won run applied every entry: {:?}", full.waits);
}

/// One entry that waits a known number of ticks is one wait of that length: six cairns spend
/// the starting dust, and the seventh waits for the dust to come (the wind kills nothing, so it
/// comes only with a wave's income). How long, the run without the seventh says: the first tick
/// it has the dust.
#[test]
fn one_entry_that_waits_a_known_time_is_one_wait_of_that_length() {
    let c = content();
    let cells = ["top:10,15", "top:8,12", "top:8,15", "top:12,8", "top:10,14", "top:8,14", "top:10,12"];
    let text = |n: usize| {
        let entries: Vec<String> = cells[..n].iter().map(|cell| format!("{{ \"wave\": 0, \"place\": \"ventus\", \"at\": \"{cell}\" }}")).collect();
        format!("{{ \"name\": \"waits\", \"commands\": [{}] }}", entries.join(", "))
    };
    let (six, seven) = (Script::parse(&text(6), &c, "six").unwrap(), Script::parse(&text(7), &c, "seven").unwrap());
    let cost = c.stone(c.stone_kind("ventus").unwrap()).tiers[0].cost;
    let mut s = State::new(Arc::clone(&c), 1);
    let mut p = Player::new(six);
    let mut ev = Vec::new();
    assert_eq!(p.drive(&mut s, &mut ev).len(), 6);
    assert!(s.dust() < cost, "the seventh is short of dust: {}", s.dust());
    let mut n = 0u32;
    while s.dust() < cost {
        s.step_into(&[], &mut ev);
        ev.clear();
        n += 1;
    }
    assert!(n > 100, "it waits a while: {n}");
    let r = run(Arc::clone(&c), 1, &seven, None, |_, _| {});
    assert!(r.skipped.is_empty(), "{:?}", r.skipped);
    assert_eq!((r.waits.entries, r.waits.longest, r.waits.never_applied), (1, n, 0), "{:?}", r.waits);
}

/// DESIGN.md, "Furor": stock content never reaches a wave's hour. No run of any proving layout
/// or shipped layout, over a few seeds, has an enemy in furor; and the most any wave's dead stood
/// past its last entry is printed against the hour (measured on 2026-11-28 in DESIGN.md).
#[test]
fn no_stock_run_reaches_a_waves_hour() {
    use sepulcra::content::WAVE_HOUR_TICKS;
    let shipped = Arc::new(Content::builtin());
    let layouts: [(&str, &str); 4] = [
        ("centre", include_str!("../data/layouts/centre.json")),
        ("no_air", include_str!("../data/layouts/no_air.json")),
        ("full_block", include_str!("../data/layouts/full_block.json")),
        ("none", include_str!("../data/layouts/none.json")),
    ];
    let runs: Vec<(Arc<Content>, &str, &str)> = LAYOUTS.iter().map(|&(n, t)| (content(), n, t)).chain(layouts.iter().map(|&(n, t)| (Arc::clone(&shipped), n, t))).collect();
    let (mut furor, mut most) = (0u32, (0u32, String::new()));
    for (c, name, text) in &runs {
        let s = Script::parse(text, c, name).unwrap();
        let spans: Vec<u32> = c.waves.iter().map(|w| w.entry_span()).collect();
        for seed in [1, 7, 42] {
            run(Arc::clone(c), seed, &s, None, |st, ev| {
                furor += ev.iter().filter(|e| matches!(e, Event::Furor { .. })).count() as u32;
                for e in st.enemies() {
                    let w = usize::from(e.wave) - 1;
                    if let Some(start) = st.wave_stats()[w].started_at {
                        let past = st.tick().saturating_sub(start + spans[w]);
                        if past > most.0 {
                            most = (past, format!("{name} seed {seed} wave {}", w + 1));
                        }
                    }
                }
            });
        }
    }
    println!("FUROR-PROBE stock: the longest any wave stood past its last entry: {} ticks ({}), against an hour of {WAVE_HOUR_TICKS}", most.0, most.1);
    assert_eq!(furor, 0, "a stock run reached a wave's hour");
    assert!(most.0 * 4 < WAVE_HOUR_TICKS, "{most:?}");
}

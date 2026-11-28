//! Tests of the elements in the rules: each stone does what its row in DESIGN.md says.

use super::*;
use crate::content::{PROVING_ENEMIES_JSON, PROVING_STONES_JSON, PROVING_WAVES_JSON, SLAB_01_JSON};

fn proving() -> Arc<Content> {
    Arc::new(Content::proving())
}

/// The proving ground with other fuel (its `"fuel"` list replaced).
fn with_fuel(fuel: &str) -> Arc<Content> {
    let stones = PROVING_STONES_JSON.replace("\r\n", "\n");
    let from = "\"fuel\": [\"grass\", \"top:9,14..top:9,16\", \"top:5,17..top:8,17\"]";
    assert!(stones.contains(from), "test fixture");
    let stones = stones.replacen(from, &format!("\"fuel\": [{fuel}]"), 1);
    Arc::new(Content::from_texts(SLAB_01_JSON, &stones, PROVING_ENEMIES_JSON, PROVING_WAVES_JSON).expect("content"))
}

fn at(c: &Content, s: &str) -> CellId {
    c.map.cell_ref(s).unwrap_or_else(|| panic!("no cell {s}"))
}

/// A state with no wave timer (nothing spawns by itself), wave 1 started.
fn quiet(c: &Arc<Content>) -> State {
    let mut s = State::new(Arc::clone(c), 1);
    s.w.next_wave_tick = None;
    s.w.wave = 1;
    s.w.wave_stats[0].started_at = Some(0);
    s
}

/// Plants an enemy of `kind` standing on `cell`; returns its id.
fn plant(s: &mut State, kind: &str, cell: &str) -> EnemyId {
    let c = Arc::clone(&s.content);
    let id = s.w.next_enemy;
    s.w.next_enemy += 1;
    let k = c.enemy_kind(kind).unwrap();
    let cell = at(&c, cell);
    let hp = c.enemy(k).hp;
    let mut e = Enemy {
        id,
        kind: k,
        wave: 1,
        hp,
        max_hp: hp,
        pos: Pos::default(),
        motion: Motion::Ground { from: cell, to: cell, progress: 0, len: 0, leap: true },
        slow: 0,
        slow_until: 0,
        attacking: None,
        remaining: 0,
        alive: true,
        carry: 0,
        next_summon: 1 << 29,
        stuck: 0,
        el: EnemyEl::default(),
    };
    s.w.place(&c, &mut e);
    s.w.enemies.push(e);
    s.w.wave_stats[0].alive += 1;
    id
}

fn place(s: &mut State, stone: &str, cell: &str, tier: u8) {
    let c = Arc::clone(&s.content);
    let cell = at(&c, cell);
    let mut ev = Vec::new();
    s.w.dust = 100_000;
    s.apply(&Command::PlaceStone { cell, stone: c.stone_kind(stone).unwrap() }, &mut ev).expect("placed");
    for _ in 0..tier {
        s.apply(&Command::Upgrade { cell }, &mut ev).expect("upgraded");
    }
}

fn idx(s: &State, id: EnemyId) -> usize {
    s.w.enemy_index(id).expect("enemy")
}

fn steps(s: &mut State, n: u32) -> Vec<Event> {
    let mut ev = Vec::new();
    for _ in 0..n {
        s.step_into(&[], &mut ev);
    }
    ev
}

fn hold_still(s: &mut State) {
    for e in &mut s.w.enemies {
        e.el.stagger_until = 100_000;
    }
}

#[test]
fn the_shipped_game_has_no_elements_and_saves_as_before() {
    let shipped = Arc::new(Content::builtin());
    assert!(!shipped.elemental() && shipped.fuel.is_empty() && !shipped.rules.elements.on);
    assert_eq!(State::new(Arc::clone(&shipped), 1).to_bytes()[..4], 1u32.to_le_bytes(), "version 1: the layout from before the elements");
    let p = proving();
    assert!(p.elemental() && !p.fuel.is_empty());
    assert_eq!(State::new(Arc::clone(&p), 1).to_bytes()[..4], STATE_VERSION.to_le_bytes());
    // Fuel is grass on fragments and the dry grass laid on lanes: never the graveyard.
    assert!(p.fuel.iter().all(|&f| matches!(p.map.cell(f).kind, CellKind::Fragment | CellKind::Fissure)));
}

#[test]
fn a_gust_deals_nothing_pushes_back_and_must_recharge() {
    let c = with_fuel("");
    let mut s = quiet(&c);
    let ids = [plant(&mut s, "shambler", "top:9,15"), plant(&mut s, "shambler", "top:9,16"), plant(&mut s, "runner", "top:9,17")];
    // Slowed to a stop, so they stay in range: only the wind moves them.
    hold_still_but_shove(&mut s);
    steps(&mut s, 3);
    let before: Vec<(i32, i32)> = ids.iter().map(|&id| (s.enemy(id).unwrap().hp, s.enemy(id).unwrap().remaining)).collect();
    place(&mut s, "ventus", "top:10,15", 0);
    let cell = at(&c, "top:10,15");
    assert_eq!(s.charge(cell), Some(1000), "a new cairn is charged");
    let ev = steps(&mut s, 1);
    assert!(ev.iter().any(|e| matches!(e, Event::Gusted { affected: 3, .. })), "{ev:?}");
    assert!(s.charge(cell).unwrap() < 50, "the gust spent the charge");
    let cd = c.stone(c.stone_kind("ventus").unwrap()).tiers[0].cooldown;
    let mut later = Vec::new();
    let mut gusts_at = Vec::new();
    let mut last = s.charge(cell).unwrap();
    for t in 1..=cd + 5 {
        later.clear();
        s.step_into(&[], &mut later);
        let ch = s.charge(cell).unwrap();
        if later.iter().any(|e| matches!(e, Event::Gusted { .. })) {
            gusts_at.push(t);
        } else {
            assert!(ch >= last, "the charge only fills between gusts");
        }
        last = ch;
        if t == 15 {
            // The shove (3 m over 0.5 s) is done: every one is farther from the graveyard.
            for (k, &id) in ids.iter().enumerate() {
                let e = s.enemy(id).unwrap();
                assert_eq!(e.hp, before[k].0, "the wind deals nothing");
                assert!(e.remaining > before[k].1, "pushed back: {} > {}", e.remaining, before[k].1);
            }
            // Put them back where they stood, in range again: only the charge holds the next gust.
            for (k, cell) in ["top:9,15", "top:9,16", "top:9,17"].iter().enumerate() {
                let i = idx(&s, ids[k]);
                let at = at(&c, cell);
                s.w.enemies[i].motion = Motion::Ground { from: at, to: at, progress: 0, len: 0, leap: true };
                let mut e = s.w.enemies[i];
                s.w.place(&c, &mut e);
                s.w.enemies[i] = e;
            }
        }
    }
    assert_eq!(gusts_at, vec![cd], "the next gust only once recharged, a full cooldown after the first");
    assert!(s.w.enemies.iter().all(|e| e.hp == e.max_hp), "no damage at all");
}

#[test]
fn a_lone_enemy_in_range_is_pushed_back() {
    // The owner (2026-11-24): the enemy "gets pushed back in their lane". One alone too.
    let c = with_fuel("");
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,15");
    hold_still_but_shove(&mut s);
    steps(&mut s, 1);
    let r0 = s.enemy(id).unwrap().remaining;
    place(&mut s, "ventus", "top:10,15", 0);
    let ev = steps(&mut s, 1);
    assert!(ev.iter().any(|e| matches!(e, Event::Gusted { affected: 1, .. })), "{ev:?}");
    steps(&mut s, 20);
    let e = s.enemy(id).unwrap();
    assert!(e.remaining > r0, "pushed back: {} > {r0}", e.remaining);
    assert_eq!(e.hp, e.max_hp, "and unhurt");
}

#[test]
fn a_lone_enemy_held_by_gusts_still_gets_through() {
    // The hardest case the wind can make: one of the dead alone in range of three cairns at
    // tier III, every gust it takes weaker by shove_fade. It reaches the graveyard all the same.
    let c = with_fuel("");
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,19");
    for cell in ["top:10,15", "top:8,15", "top:8,12"] {
        place(&mut s, "ventus", cell, 2);
    }
    let mut ev = Vec::new();
    let mut gusts = 0;
    for _ in 0..(10 * 60 * TICK_HZ) {
        ev.clear();
        s.step_into(&[], &mut ev);
        gusts += ev.iter().filter(|e| matches!(e, Event::Gusted { .. })).count();
        if s.enemy(id).is_none() {
            break;
        }
    }
    assert!(gusts > 3, "the cairns gusted at it: {gusts}");
    assert!(s.enemy(id).is_none(), "held for ten minutes by {gusts} gusts: the fade must let it through");
    assert_eq!(s.wave_stats()[0].leaked, 1, "it walked into the graveyard");
}

#[test]
fn a_shove_reaches_nobody_once_the_series_has_run_out() {
    let c = with_fuel("");
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,15");
    let i = idx(&s, id);
    let keep = 1000 - c.rules.elements.shove_fade;
    // The last shove that still moves a 30 km push (far past what content allows), and the next.
    let last = (0..u16::MAX).rev().find(|&n| crate::content::shove_mm(30_000_000, 1000, keep, n) > 0).expect("one moves it");
    s.w.enemies[i].el.shoves = last;
    s.w.shove(&c, i, 30_000_000, 15);
    assert!(s.w.enemies[i].el.shove > 0, "the last shove of the series still moves it");
    s.w.enemies[i].el.shove = 0;
    s.w.shove(&c, i, 30_000_000, 15);
    assert_eq!((s.w.enemies[i].el.shove, s.w.enemies[i].el.shoves), (0, last + 2), "after it, nothing");
    assert!(last < 100, "the series ends within a hundred shoves at the proving fade: {last}");
}

#[test]
fn an_enemy_being_shoved_is_not_shoved_again_until_it_ends() {
    let c = with_fuel("");
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,15");
    let i = idx(&s, id);
    s.w.shove(&c, i, 3_000, 15);
    let (first, rate) = (s.w.enemies[i].el.shove, s.w.enemies[i].el.shove_rate);
    assert_eq!((first, rate, s.w.enemies[i].el.shoves), (3_000, 200, 1));
    // A second shove (a bigger push too) while the first runs: no change, and not counted.
    s.w.shove(&c, i, 9_000, 30);
    assert_eq!((s.w.enemies[i].el.shove, s.w.enemies[i].el.shove_rate, s.w.enemies[i].el.shoves), (first, rate, 1));
    // A cairn keeps its charge while the only enemy in its range is still being shoved.
    hold_still_but_shove(&mut s);
    place(&mut s, "ventus", "top:10,15", 0);
    let cell = at(&c, "top:10,15");
    let ev = steps(&mut s, 1);
    assert!(!ev.iter().any(|e| matches!(e, Event::Gusted { .. })), "no gust at one still being shoved: {ev:?}");
    assert_eq!(s.charge(cell), Some(1000), "charged still");
    let mut gusted = false;
    for _ in 0..20 {
        let ev = steps(&mut s, 1);
        if ev.iter().any(|e| matches!(e, Event::Gusted { affected: 1, .. })) {
            gusted = true;
            break;
        }
    }
    assert!(gusted, "it gusts once the first shove has ended");
    assert_eq!(s.w.enemies[idx(&s, id)].el.shoves, 2);
}

/// The fourth review's item 5: a gust at the dead it can no longer move (their series run out,
/// or in furor) is no gust: the cairn keeps its charge, and nobody's fire is fanned.
#[test]
fn a_gust_at_the_dead_it_cannot_move_keeps_its_charge() {
    let c = with_fuel("");
    let cell = at(&c, "top:10,15");
    let keep = 1000 - c.rules.elements.shove_fade;
    let push = c.stone(c.stone_kind("ventus").unwrap()).tiers[0].el.push;
    for case in ["series run out", "in furor"] {
        let mut s = quiet(&c);
        let id = plant(&mut s, "shambler", "top:9,15");
        hold_still_but_shove(&mut s);
        let i = idx(&s, id);
        if case == "series run out" {
            s.w.enemies[i].el.shoves = (0..u16::MAX).find(|&n| crate::content::shove_mm(push, 1000, keep, n) == 0).unwrap();
        } else {
            s.w.tick = s.w.hour_of(&c, 1).unwrap();
        }
        (s.w.enemies[i].el.burn, s.w.enemies[i].el.burn_until) = (10, s.w.tick + 5);
        place(&mut s, "ventus", "top:10,15", 0);
        let ev = steps(&mut s, 3);
        assert!(!ev.iter().any(|e| matches!(e, Event::Gusted { .. })), "{case}: no gust: {ev:?}");
        assert_eq!(s.charge(cell), Some(1000), "{case}: charged still");
        assert!(s.enemy(id).is_none_or(|e| e.el.burn_until <= s.w.tick + 5), "{case}: no fire fanned");
    }
}

#[test]
fn content_without_a_shove_fade_gets_one_and_the_wind_layouts_end() {
    let stones = PROVING_STONES_JSON.replace("\r\n", "\n");
    let without = stones.replace("    \"shove_fade\": 0.25,\n", "");
    assert_ne!(without, stones, "test fixture");
    let c = Arc::new(Content::from_texts(SLAB_01_JSON, &without, PROVING_ENEMIES_JSON, PROVING_WAVES_JSON).expect("content"));
    assert_eq!(c.rules.elements.shove_fade, 250, "the default is the proving ground's 0.25");
    let zero = stones.replace("\"shove_fade\": 0.25", "\"shove_fade\": 0");
    let err = Content::from_texts(SLAB_01_JSON, &zero, PROVING_ENEMIES_JSON, PROVING_WAVES_JSON).unwrap_err();
    assert!(err.to_string().contains("shove_fade"), "a fade of 0 brings the stall back: refused ({err})");
    let text = include_str!("../data/proving/layouts/fire_wind.json");
    let script = crate::script::Script::parse(text, &c, "fire_wind").expect("layout");
    for seed in 1..=8 {
        let r = crate::script::run(Arc::clone(&c), seed, &script, None, |_, _| {});
        assert_ne!(r.outcome, Outcome::Running, "seed {seed} ended");
    }
}

#[test]
fn a_gust_on_a_leaper_in_mid_air_waits_for_it_to_land() {
    let c = with_fuel("");
    let mut s = quiet(&c);
    let id = plant(&mut s, "runner", "top:9,17");
    let i = idx(&s, id);
    let (a, b) = (at(&c, "top:9,17"), at(&c, "top:9,16"));
    s.w.enemies[i].motion = Motion::Ground { from: a, to: b, progress: 500, len: 4000, leap: true };
    s.w.enemies[i].el.shove = 3000;
    s.w.enemies[i].el.shove_rate = 200;
    steps(&mut s, 1);
    let e = s.enemy(id).unwrap();
    let Motion::Ground { progress, leap, .. } = e.motion else { panic!("{:?}", e.motion) };
    assert!(leap && progress > 500, "it flies on, not held in the air: {:?}", e.motion);
    assert_eq!(e.el.shove, 3000, "the shove is not spent in the air");
    // It lands, then the shove takes it back.
    let mut landed = false;
    for _ in 0..(10 * TICK_HZ) {
        steps(&mut s, 1);
        let e = s.enemy(id).unwrap();
        if !matches!(e.motion, Motion::Ground { leap: true, progress, len, .. } if len > 0 && progress > 0 && progress < len) {
            landed = true;
        }
        if landed && e.el.shove == 0 {
            return;
        }
    }
    panic!("the shove never took it after it landed: {:?}", s.enemy(id).unwrap());
}

#[test]
fn the_light_are_pushed_farther_than_the_heavy() {
    let c = with_fuel("");
    let shove = |kind: &str| {
        let mut s = quiet(&c);
        let id = plant(&mut s, kind, "top:9,15");
        let i = idx(&s, id);
        s.w.shove(&c, i, 3_000, 15);
        let r0 = s.enemy(id).unwrap().remaining;
        hold_still_but_shove(&mut s);
        steps(&mut s, 15);
        s.enemy(id).unwrap().remaining - r0
    };
    let (runner, shambler, colossus) = (shove("runner"), shove("shambler"), shove("colossus"));
    assert_eq!(shambler, 3_000, "mass 1: the whole push");
    assert!(runner > shambler && shambler > colossus && colossus > 0, "{runner} {shambler} {colossus}");
}

#[test]
fn each_later_shove_is_weaker() {
    let c = with_fuel("");
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,15");
    let i = idx(&s, id);
    let keep = i64::from(1000 - c.rules.elements.shove_fade);
    assert_eq!(keep, 750, "each shove three quarters of the one before");
    let mut got = Vec::new();
    for _ in 0..4 {
        s.w.enemies[i].el.shove = 0;
        s.w.shove(&c, i, 3_000, 15);
        got.push(s.w.enemies[i].el.shove);
    }
    // A fixed fraction of the one before, whichever cairn or blast gives it: 3000 x 0.75^n.
    let want: Vec<i32> = (0..4).map(|n| (3_000 * keep.pow(n) / 1000i64.pow(n)) as i32).collect();
    assert_eq!(got, want);
    assert_eq!(got, vec![3_000, 2_250, 1_687, 1_265]);
}

/// Nothing but the shove moves them (a speed of zero): slows them to a stop.
fn hold_still_but_shove(s: &mut State) {
    for e in &mut s.w.enemies {
        e.slow = 1000;
        e.slow_until = 100_000;
    }
}

#[test]
fn frost_freezes_the_frozen_stand_and_shatter_or_thaw_wet() {
    let c = with_fuel("");
    let gelu = c.stone(c.stone_kind("gelu").unwrap()).tiers[0].el;
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,17");
    steps(&mut s, 10);
    let mut ev = Vec::new();
    let i = idx(&s, id);
    for _ in 0..3 {
        s.w.chill(&c, i, &gelu, &mut ev);
    }
    assert!(ev.iter().any(|e| matches!(e, Event::Frozen { .. })), "0.4 a hit: frozen at the third");
    let pos = s.enemy(id).unwrap().pos;
    steps(&mut s, gelu.freeze_ticks - 1);
    assert_eq!(s.enemy(id).unwrap().pos, pos, "the frozen stand");
    let ev = steps(&mut s, 2);
    assert!(ev.iter().any(|e| matches!(e, Event::Thawed { steam: false, .. })), "{ev:?}");
    assert!(s.enemy(id).unwrap().el.wet(s.tick()), "a thaw leaves it wet");
    // Wet, it does not catch fire.
    let mut ev = Vec::new();
    let i = idx(&s, id);
    s.w.ignite(&c, i, 100, 60, &mut ev);
    assert!(!s.enemy(id).unwrap().el.burning(s.tick()) && ev.is_empty());

    // Frozen again, a hit shatters it for the frost's bonus (shatter 1.0: double).
    let id2 = plant(&mut s, "shambler", "top:9,18");
    let i = idx(&s, id2);
    let mut ev = Vec::new();
    for _ in 0..3 {
        s.w.chill(&c, i, &gelu, &mut ev);
    }
    let hp = s.enemy(id2).unwrap().hp;
    s.w.hurt(&c, i, 5_000, &mut ev);
    assert!(ev.iter().any(|e| matches!(e, Event::Shattered { damage: 10_000, .. })), "{ev:?}");
    assert_eq!(s.enemy(id2).unwrap().hp, hp - 10_000);
    assert!(!s.enemy(id2).unwrap().el.frozen(s.tick()), "the ice is gone");

    // Fire on the frozen thaws it (steam) instead of burning it.
    let id3 = plant(&mut s, "shambler", "top:9,16");
    let i = idx(&s, id3);
    let mut ev = Vec::new();
    for _ in 0..3 {
        s.w.chill(&c, i, &gelu, &mut ev);
    }
    ev.clear();
    s.w.ignite(&c, i, 100, 60, &mut ev);
    assert_eq!(ev, vec![Event::Thawed { enemy: id3, steam: true }]);
    assert!(!s.enemy(id3).unwrap().el.burning(s.tick()));
}

#[test]
fn frost_does_not_freeze_the_colossus_quickly() {
    let c = with_fuel("");
    let gelu = c.stone(c.stone_kind("gelu").unwrap()).tiers[2].el;
    let mut s = quiet(&c);
    let id = plant(&mut s, "colossus", "top:9,18");
    let (i, mut ev) = (idx(&s, id), Vec::new());
    s.w.chill(&c, i, &gelu, &mut ev);
    assert_eq!(s.enemy(id).unwrap().el.chill, 75, "mass 8: 0.6 a hit is 0.075");
}

#[test]
fn frost_puts_a_burning_enemy_out() {
    let c = with_fuel("");
    let gelu = c.stone(c.stone_kind("gelu").unwrap()).tiers[0].el;
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,17");
    let (i, mut ev) = (idx(&s, id), Vec::new());
    s.w.ignite(&c, i, 100, 300, &mut ev);
    assert!(s.enemy(id).unwrap().el.burning(s.tick()));
    s.w.chill(&c, i, &gelu, &mut ev);
    assert!(ev.contains(&Event::Extinguished { enemy: id }) && !s.enemy(id).unwrap().el.burning(s.tick()));
}

#[test]
fn fire_moves_through_a_packed_column_and_burns_out() {
    let c = with_fuel("");
    let mut s = quiet(&c);
    // Two side by side (within 1.2 m), one 2 m behind.
    let a = plant(&mut s, "shambler", "top:9,18");
    let b = plant(&mut s, "shambler", "top:9,18");
    let far = plant(&mut s, "shambler", "top:9,17");
    hold_still(&mut s);
    let (i, mut ev) = (idx(&s, a), Vec::new());
    s.w.ignite(&c, i, 100, 75, &mut ev);
    let ev = steps(&mut s, c.rules.elements.spread_ticks + 1);
    assert!(ev.contains(&Event::Ignited { enemy: b }), "the packed neighbour caught: {ev:?}");
    assert!(!ev.contains(&Event::Ignited { enemy: far }), "2 m is past the dead's reach");
    steps(&mut s, 400);
    assert!(s.w.enemies.iter().all(|e| !e.el.burning(s.tick())), "it ends");
}

fn caught(ev: &[Event]) -> Vec<CellId> {
    ev.iter().filter_map(|e| if let Event::CellCaught { cell } = e { Some(*cell) } else { None }).collect()
}

#[test]
fn fire_does_not_cross_a_break_wider_than_its_reach_and_ends() {
    // Two runs of dry grass on the connector lane, 6 m of bare stone between them.
    let c = with_fuel("\"top:5,17..top:7,17\", \"top:10,17..top:12,17\"");
    let mut s = quiet(&c);
    s.w.blaze.heat(&c, at(&c, "top:5,17"), 1000, 0);
    let ev = steps(&mut s, 60 * TICK_HZ);
    let got = caught(&ev);
    for x in ["top:5,17", "top:6,17", "top:7,17"] {
        assert!(got.contains(&at(&c, x)), "{x} burnt");
    }
    for x in ["top:10,17", "top:11,17", "top:12,17"] {
        assert!(!got.contains(&at(&c, x)), "{x}: the break held");
    }
    assert_eq!(s.blaze().burning(&c).count(), 0, "every fire ended");
    assert_eq!(ev.iter().filter(|e| matches!(e, Event::CellSpent { .. })).count(), 3, "each burnt cell is spent");

    // Without the break it runs the whole way.
    let c = with_fuel("\"top:5,17..top:12,17\"");
    let mut s = quiet(&c);
    s.w.blaze.heat(&c, at(&c, "top:5,17"), 1000, 0);
    let got = caught(&steps(&mut s, 60 * TICK_HZ));
    assert!(got.contains(&at(&c, "top:12,17")), "no break: across");
}

/// Ticks until each of `cells` caught, from a fire lit at `from`, in a wind of `wind` if given.
fn catch_times(wind: Option<(i32, i32)>, from: &str, cells: &[&str]) -> Vec<u32> {
    let c = with_fuel("\"top:4,17..top:15,17\"");
    let mut s = quiet(&c);
    if let Some(dir) = wind {
        s.w.blaze.add_zone(Zone { at: c.map.cell(at(&c, "top:9,17")).pos, radius: 50_000, dir, until: 100_000 });
    }
    s.w.blaze.heat(&c, at(&c, from), 1000, 0);
    let mut ev = Vec::new();
    let mut when = vec![0u32; cells.len()];
    for t in 1..=90 * TICK_HZ {
        ev.clear();
        s.step_into(&[], &mut ev);
        for (k, x) in cells.iter().enumerate() {
            if when[k] == 0 && ev.contains(&Event::CellCaught { cell: at(&c, x) }) {
                when[k] = t;
            }
        }
    }
    when
}

#[test]
fn wind_makes_fire_run_faster_downwind_than_upwind() {
    let still = catch_times(None, "top:9,17", &["top:12,17", "top:6,17"]);
    assert!(still[0] > 0 && still[0] == still[1], "no wind: the same both ways {still:?}");
    let east = catch_times(Some((1000, 0)), "top:9,17", &["top:12,17", "top:6,17"]);
    assert!(east[0] > 0 && east[1] > 0, "{east:?}");
    assert!(east[0] < still[0] && east[1] > still[1], "downwind sooner, upwind later: {east:?} against {still:?}");
}

#[test]
fn frost_and_earth_cover_stop_the_fire_and_put_a_cell_out() {
    let c = with_fuel("\"top:5,17..top:12,17\"");
    let mut s = quiet(&c);
    let mut ev = Vec::new();
    s.w.cover(&c, at(&c, "top:8,17"), 100_000, &mut ev);
    s.w.blaze.heat(&c, at(&c, "top:5,17"), 1000, 0);
    let got = caught(&steps(&mut s, 60 * TICK_HZ));
    assert!(got.contains(&at(&c, "top:7,17")) && !got.contains(&at(&c, "top:8,17")) && !got.contains(&at(&c, "top:9,17")), "the rime held: {got:?}");
    // A cover on a burning cell puts it out.
    let mut s = quiet(&c);
    s.w.blaze.heat(&c, at(&c, "top:10,17"), 1000, 0);
    steps(&mut s, c.rules.elements.spread_ticks);
    assert!(s.blaze().is_burning(&c, at(&c, "top:10,17")));
    let mut ev = Vec::new();
    let t = s.tick();
    s.w.cover(&c, at(&c, "top:10,17"), t + 10, &mut ev);
    assert_eq!(ev, vec![Event::CellSmothered { cell: at(&c, "top:10,17") }]);
}

#[test]
fn a_burning_cell_sets_the_dead_beside_it_alight() {
    let c = with_fuel("\"top:9,16\"");
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,17");
    hold_still(&mut s);
    s.w.blaze.heat(&c, at(&c, "top:9,16"), 1000, 0);
    let ev = steps(&mut s, 2 * c.rules.elements.spread_ticks + 1);
    assert!(ev.contains(&Event::Ignited { enemy: id }), "{ev:?}");
}

#[test]
fn lightning_chains_to_the_nearest_harder_on_the_wet_and_stops_at_the_grounded() {
    let c = with_fuel("");
    let strike = |wet: bool, grounded: bool| {
        let mut s = quiet(&c);
        let ids = [plant(&mut s, "shambler", "top:9,15"), plant(&mut s, "shambler", "top:9,16"), plant(&mut s, "shambler", "top:9,17"), plant(&mut s, "shambler", "top:9,18")];
        hold_still_but_shove(&mut s);
        if grounded {
            s.w.enemies[0].el.stagger_until = 100_000;
        }
        if wet {
            s.w.enemies[1].el.wet_until = 100_000;
        }
        place(&mut s, "fulmen", "top:10,15", 0);
        let ev = steps(&mut s, 1);
        let hits: Vec<(EnemyId, i32, u8)> = ev.iter().filter_map(|e| if let Event::Struck { enemy, damage, link, .. } = e { Some((*enemy, *damage, *link)) } else { None }).collect();
        (ids, hits)
    };
    let (ids, hits) = strike(false, false);
    // Chain 2: the first (nearest the graveyard) and two jumps, 70 % each time.
    assert_eq!(hits.iter().map(|h| h.2).collect::<Vec<_>>(), vec![0, 1, 2], "{hits:?}");
    assert_eq!(hits.iter().map(|h| h.0).collect::<Vec<_>>(), ids[..3].to_vec());
    assert_eq!(hits.iter().map(|h| h.1).collect::<Vec<_>>(), vec![14_000, 9_800, 6_860]);
    let (_, wet) = strike(true, false);
    assert_eq!(wet[1].1, 14_700, "the wet take half again: {wet:?}");
    let (_, grounded) = strike(false, true);
    assert_eq!(grounded.len(), 1, "a staggered enemy grounds the strike: {grounded:?}");
}

#[test]
fn earth_staggers_smothers_and_raises_rubble_that_crumbles() {
    let c = with_fuel("\"top:9,16\"");
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,18");
    hold_still_but_shove(&mut s);
    s.w.blaze.heat(&c, at(&c, "top:9,16"), 1000, 0);
    steps(&mut s, c.rules.elements.spread_ticks);
    assert!(s.blaze().is_burning(&c, at(&c, "top:9,16")));
    place(&mut s, "saxum", "top:10,15", 1);
    let ev = steps(&mut s, 1);
    assert!(ev.iter().any(|e| matches!(e, Event::Heaved { affected: 1, cells: 3, .. })), "{ev:?}");
    assert!(ev.contains(&Event::CellSmothered { cell: at(&c, "top:9,16") }), "earth smothers the grass on its line: {ev:?}");
    let rubble: Vec<CellId> = ev.iter().filter_map(|e| if let Event::RubbleRaised { cell } = e { Some(*cell) } else { None }).collect();
    assert_eq!(rubble, vec![at(&c, "top:9,16")], "on the farthest cell of the line: {ev:?}");
    assert!(s.wall(rubble[0]).is_some_and(|w| w.cost == 0), "rubble is a wall worth nothing");
    assert!(s.w.blocked[rubble[0].index()], "it blocks and re-routes");
    assert!(s.enemy(id).unwrap().el.staggered(s.tick()));
    let life = c.stone(c.stone_kind("saxum").unwrap()).tiers[1].el.rubble_ticks;
    let mut ev = Vec::new();
    s.apply(&Command::Sell { cell: at(&c, "top:10,15") }, &mut ev).unwrap();
    let ev = steps(&mut s, life);
    assert!(ev.contains(&Event::WallCrumbled { cell: rubble[0] }), "it crumbled: {ev:?}");
    assert!(s.wall(rubble[0]).is_none() && !s.w.blocked[rubble[0].index()]);
    assert_eq!(s.rubble().count(), 0);
}

/// Steps until `id` bursts (at most `max` ticks): the state the tick before, and that tick's events.
fn until_burst(s: &mut State, id: EnemyId, max: u32) -> Option<(State, Vec<Event>)> {
    let mut ev = Vec::new();
    for _ in 0..max {
        let before = s.clone();
        ev.clear();
        s.step_into(&[], &mut ev);
        if ev.iter().any(|e| matches!(e, Event::Detonated { enemy, .. } if *enemy == id)) {
            return Some((before, ev.clone()));
        }
    }
    None
}

#[test]
fn an_exploder_bursts_on_a_wall_the_wall_goes_and_the_dead_reroute() {
    let c = with_fuel("");
    let mut s = quiet(&c);
    let wall = at(&c, "top:9,14");
    let mut ev = Vec::new();
    s.w.dust = 1000;
    s.apply(&Command::PlaceWall { cell: wall }, &mut ev).unwrap();
    let probe = at(&c, "top:9,17");
    let walled = s.fields().open(Class::Walker)[probe.index()];
    let bomb = plant(&mut s, "tumidus", "top:9,18");
    let runner = plant(&mut s, "runner", "top:9,18");
    let (before, ev) = until_burst(&mut s, bomb, 60 * TICK_HZ).expect("it reached the wall and burst");
    assert!(ev.contains(&Event::WallBroken { cell: wall }), "{ev:?}");
    assert!(s.wall(wall).is_none());
    let open = s.fields().open(Class::Walker)[probe.index()];
    assert!(open < walled, "the straight lane is open again: {open} < {walled}");
    assert_eq!(*s.fields(), Fields::build(&c.map, &s.w.blocked, c.rules.wall.breach_penalty), "the fields are rebuilt from the walls left");
    assert_eq!((s.wave_stats()[0].detonated, s.wave_stats()[0].killed), (1, 0), "a burst on a wall is no kill");
    // Every live ground enemy's distance left is the new field's: the runner going round now
    // goes straight (its remaining distance fell by the detour at once, on its next move).
    let r_before = before.enemy(runner).map(|e| e.remaining);
    steps(&mut s, 1);
    if let (Some(rb), Some(r)) = (r_before, s.enemy(runner)) {
        assert!(r.remaining < rb, "the runner's way is shorter now: {} < {rb}", r.remaining);
    }
    // A save the tick before the burst continues exactly as the run did.
    let mut a = before.clone();
    let mut b = State::from_bytes(Arc::clone(&c), &before.to_bytes()).expect("loads");
    assert_eq!(a.hash(), b.hash());
    let (mut ea, mut eb) = (Vec::new(), Vec::new());
    for _ in 0..600 {
        ea.clear();
        eb.clear();
        a.step_into(&[], &mut ea);
        b.step_into(&[], &mut eb);
        assert_eq!((a.hash(), &ea), (b.hash(), &eb));
    }
}

#[test]
fn a_frozen_exploder_dies_whole_and_lightning_sets_one_off() {
    let c = with_fuel("");
    let gelu = c.stone(c.stone_kind("gelu").unwrap()).tiers[2].el;
    let mut s = quiet(&c);
    let bomb = plant(&mut s, "tumidus", "top:9,18");
    let i = idx(&s, bomb);
    let mut ev = Vec::new();
    // Mass 1.5: chill 0.6 a hit is 0.4; three hits freeze it.
    for _ in 0..3 {
        s.w.chill(&c, i, &gelu, &mut ev);
    }
    assert!(s.enemy(bomb).unwrap().el.frozen(s.tick()));
    s.w.hurt(&c, i, 1_000_000, &mut ev);
    s.w.run_bursts(&c, &mut ev);
    assert!(!ev.iter().any(|e| matches!(e, Event::Detonated { .. })), "{ev:?}");
    assert_eq!(s.wave_stats()[0].killed, 1, "it died, whole");

    let mut s = quiet(&c);
    let bomb = plant(&mut s, "tumidus", "top:9,16");
    hold_still_but_shove(&mut s);
    place(&mut s, "fulmen", "top:10,15", 0);
    let ev = steps(&mut s, 1);
    assert!(ev.iter().any(|e| matches!(e, Event::Detonated { enemy, .. } if *enemy == bomb)), "struck, it bursts at once: {ev:?}");
}

/// A Tumidus frozen solid by Gelu tier I (frozen for a long time, so no thaw interferes).
fn frozen_bomb(s: &mut State, cell: &str) -> EnemyId {
    let c = Arc::clone(&s.content);
    let gelu = c.stone(c.stone_kind("gelu").unwrap()).tiers[0].el;
    let bomb = plant(s, "tumidus", cell);
    let i = idx(s, bomb);
    let mut ev = Vec::new();
    while !s.w.enemies[i].el.frozen(s.tick()) {
        s.w.chill(&c, i, &gelu, &mut ev);
    }
    s.w.enemies[i].el.frozen_until = 100_000;
    bomb
}

fn detonated(ev: &[Event], id: EnemyId) -> bool {
    ev.iter().any(|e| matches!(e, Event::Detonated { enemy, .. } if *enemy == id))
}

#[test]
fn a_frozen_exploder_struck_by_lightning_and_surviving_does_not_burst() {
    // The review's path: the strike's hurt shatters the ice first; the enemy lives through the
    // shatter (40 HP, 28 of it); it was frozen when struck, so it cannot burst.
    let c = with_fuel("");
    let mut s = quiet(&c);
    let bomb = frozen_bomb(&mut s, "top:9,16");
    hold_still_but_shove(&mut s);
    place(&mut s, "fulmen", "top:10,15", 0);
    let ev = steps(&mut s, 1);
    assert!(ev.iter().any(|e| matches!(e, Event::Shattered { enemy, .. } if *enemy == bomb)), "{ev:?}");
    let e = s.enemy(bomb).expect("it lived through the shatter");
    assert!(e.hp > 0 && !e.el.frozen(s.tick()), "the ice is gone");
    assert!(!detonated(&ev, bomb), "frozen when struck, it does not burst: {ev:?}");
    assert_eq!(s.wave_stats()[0].killed, 0);
    // The ice gone, it is an exploder like any other: the next strike sets it off.
    let cd = c.stone(c.stone_kind("fulmen").unwrap()).tiers[0].cooldown;
    let ev = steps(&mut s, cd);
    assert!(detonated(&ev, bomb), "thawed by the shatter, the next strike bursts it: {ev:?}");
}

#[test]
fn a_frozen_exploder_killed_by_a_strike_or_a_bolt_dies_whole() {
    let c = with_fuel("");
    // Killed outright by the strike's shatter.
    let mut s = quiet(&c);
    let bomb = frozen_bomb(&mut s, "top:9,16");
    let i = idx(&s, bomb);
    s.w.enemies[i].hp = 1;
    hold_still_but_shove(&mut s);
    place(&mut s, "fulmen", "top:10,15", 0);
    let ev = steps(&mut s, 1);
    assert!(ev.iter().any(|e| matches!(e, Event::Died { enemy, .. } if *enemy == bomb)) && !detonated(&ev, bomb), "{ev:?}");
    // Killed by another stone's bolt.
    let mut s = quiet(&c);
    let bomb = frozen_bomb(&mut s, "top:9,16");
    let i = idx(&s, bomb);
    s.w.enemies[i].hp = 1;
    place(&mut s, "warding", "top:10,15", 0);
    let mut all = Vec::new();
    for _ in 0..(5 * TICK_HZ) {
        all.extend(steps(&mut s, 1));
        if s.enemy(bomb).is_none() {
            break;
        }
    }
    assert!(all.iter().any(|e| matches!(e, Event::Died { enemy, .. } if *enemy == bomb)), "the bolt killed it");
    assert!(!detonated(&all, bomb), "frozen, it died whole: {all:?}");
}

#[test]
fn a_frozen_exploder_in_another_ones_blast_does_not_burst() {
    let c = with_fuel("");
    for hp in [1, 1_000_000] {
        let mut s = quiet(&c);
        let lit = plant(&mut s, "tumidus", "top:9,16");
        let cold = frozen_bomb(&mut s, "top:9,16");
        let i = idx(&s, cold);
        s.w.enemies[i].hp = hp;
        hold_still(&mut s);
        let i = idx(&s, lit);
        let mut ev = Vec::new();
        s.w.kill(&c, i, true, &mut ev);
        s.w.run_bursts(&c, &mut ev);
        assert!(detonated(&ev, lit), "{ev:?}");
        assert!(!detonated(&ev, cold), "hp {hp}: frozen in the blast, no second burst: {ev:?}");
        assert!(ev.iter().any(|e| matches!(e, Event::Shattered { enemy, .. } if *enemy == cold)), "the blast shattered it");
        assert_eq!(s.enemy(cold).is_some_and(|e| e.alive), hp > 1, "hp {hp}");
        // Alive and the ice gone: nothing is left lit in it.
        if let Some(e) = s.enemy(cold).filter(|e| e.alive) {
            assert_eq!(e.el.fuse_at, 0);
            assert!(!detonated(&steps(&mut s, 3 * TICK_HZ), cold));
        }
    }
}

#[test]
fn a_fuse_does_not_burn_in_ice() {
    let c = with_fuel("");
    let gelu = c.stone(c.stone_kind("gelu").unwrap()).tiers[0].el;
    let fuse = c.enemy(c.enemy_kind("tumidus").unwrap()).burst.unwrap().fuse;
    // Lit, then frozen before the fuse ends: frost puts the fire out, and the fuse with it.
    let mut s = quiet(&c);
    let bomb = plant(&mut s, "tumidus", "top:9,18");
    hold_still(&mut s);
    let (i, mut ev) = (idx(&s, bomb), Vec::new());
    s.w.ignite(&c, i, 1, 300, &mut ev);
    steps(&mut s, fuse / 2);
    let i = idx(&s, bomb);
    while !s.w.enemies[i].el.frozen(s.tick()) {
        s.w.chill(&c, i, &gelu, &mut ev);
    }
    assert!(until_burst(&mut s, bomb, 10 * TICK_HZ).is_none(), "frozen, its fuse is out");
    // However a fuse came to be set on a frozen one, the ice puts it out on the tick it would end.
    let mut s = quiet(&c);
    let bomb = frozen_bomb(&mut s, "top:9,18");
    let i = idx(&s, bomb);
    let t = s.tick();
    s.w.enemies[i].el.fuse_at = t + 2;
    assert!(until_burst(&mut s, bomb, 10 * TICK_HZ).is_none(), "no burst in ice");
    assert_eq!(s.enemy(bomb).unwrap().el.fuse_at, 0);
}

#[test]
fn a_frozen_exploder_struck_and_set_alight_in_one_tick_bursts_only_by_its_fuse() {
    // In one tick the stones fire first (the strike shatters the ice and the exploder lives),
    // then the projectiles land (a fire lob: the ice is gone, so it catches and its fuse burns).
    let c = with_fuel("");
    let fuse = c.enemy(c.enemy_kind("tumidus").unwrap()).burst.unwrap().fuse;
    let mut s = quiet(&c);
    let bomb = frozen_bomb(&mut s, "top:9,16");
    // Tough enough that neither the strike nor the burn kills it before the fuse ends.
    let i = idx(&s, bomb);
    s.w.enemies[i].hp = 1_000_000;
    hold_still(&mut s);
    place(&mut s, "fulmen", "top:10,15", 0);
    let rogus = c.stone_kind("rogus").unwrap();
    let aim = s.enemy(bomb).unwrap().pos;
    let id = s.w.next_projectile;
    s.w.next_projectile += 1;
    s.w.projectiles.push(Projectile { id, stone: rogus, tier: 0, origin: aim, pos: aim, aim, target: Some(bomb), homing: false, done: false });
    let ev = steps(&mut s, 1);
    let order: Vec<&str> = ev
        .iter()
        .filter_map(|e| match e {
            Event::Shattered { enemy, .. } if *enemy == bomb => Some("shattered"),
            Event::Ignited { enemy } if *enemy == bomb => Some("ignited"),
            Event::Thawed { enemy, .. } if *enemy == bomb => Some("thawed"),
            Event::Detonated { enemy, .. } if *enemy == bomb => Some("detonated"),
            _ => None,
        })
        .collect();
    assert_eq!(order, ["shattered", "ignited"], "{ev:?}");
    let lit = s.tick() - 1;
    // The menhir goes, so only the fuse can set it off.
    let mut ev = Vec::new();
    s.apply(&Command::Sell { cell: at(&c, "top:10,15") }, &mut ev).unwrap();
    let (before, _) = until_burst(&mut s, bomb, 10 * TICK_HZ).expect("its fuse burns down");
    assert_eq!(before.tick(), lit + fuse, "by its fuse, not by the strike");
}

#[test]
fn a_thawed_exploder_struck_by_lightning_bursts() {
    // Fire on a frozen one thaws it (steam, wet): no longer frozen, a strike sets it off.
    let c = with_fuel("");
    let mut s = quiet(&c);
    let bomb = frozen_bomb(&mut s, "top:9,16");
    hold_still_but_shove(&mut s);
    let (i, mut ev) = (idx(&s, bomb), Vec::new());
    s.w.ignite(&c, i, 100, 60, &mut ev);
    assert_eq!(ev, vec![Event::Thawed { enemy: bomb, steam: true }]);
    place(&mut s, "fulmen", "top:10,15", 0);
    assert!(detonated(&steps(&mut s, 1), bomb));
}

#[test]
fn no_burst_goes_off_after_the_game_is_lost() {
    // The review's probe: an exploder beside a wall (first in the list) bursts on it in the same
    // move as a runner reaches the graveyard and the last tombstone goes.
    let c = with_fuel("");
    let mut s = quiet(&c);
    let wall = at(&c, "top:9,14");
    let mut ev = Vec::new();
    s.w.dust = 1000;
    s.apply(&Command::PlaceWall { cell: wall }, &mut ev).unwrap();
    plant(&mut s, "tumidus", "top:9,15");
    let map = &c.map;
    let (from, to, cost) = map
        .cells()
        .iter()
        .filter(|cl| cl.kind.walkable(false) && cl.kind != CellKind::Graveyard)
        .find_map(|cl| map.links(cl.id).iter().find(|l| map.cell(l.to).kind == CellKind::Graveyard).map(|l| (cl.id, l.to, l.cost)))
        .expect("a cell beside the graveyard");
    let runner = plant(&mut s, "runner", "top:9,18");
    let ri = idx(&s, runner);
    s.w.enemies[ri].motion = Motion::Ground { from, to, progress: cost - 1, len: cost, leap: false };
    let weak = plant(&mut s, "shambler", "top:9,15");
    let wi = idx(&s, weak);
    s.w.enemies[wi].hp = 1;
    s.w.tombstones = 1;
    let ev = steps(&mut s, 1);
    let lost = ev.iter().position(|e| matches!(e, Event::Lost)).expect("lost this tick");
    assert!(ev[lost + 1..].is_empty(), "nothing after Lost: {:?}", &ev[lost + 1..]);
    assert!(s.wall(wall).is_some(), "the wall still stands");
    assert!(s.w.bursts.is_empty(), "no burst is kept");
}

fn proving_with(from: &str, to: &str) -> Result<Content, crate::error::DataError> {
    let stones = PROVING_STONES_JSON.replace("\r\n", "\n");
    assert_eq!(stones.matches(from).count(), 1, "test fixture: {from}");
    Content::from_texts(SLAB_01_JSON, &stones.replacen(from, to, 1), PROVING_ENEMIES_JSON, PROVING_WAVES_JSON)
}

#[test]
fn a_key_an_attack_does_not_read_is_refused_by_name() {
    let pulse = "{ \"cost\": 60, \"range_m\": 5.0, \"cooldown_s\": 1.0, \"damage\": 2, \"slow\": 0.35, \"slow_s\": 1.5 }";
    let strike = "\"cooldown_s\": 1.5, \"damage\": 14, \"chain\": 2,";
    let bolt = "\"damage\": 10, \"projectile_mps\": 20 },";
    let gust = "\"push_m\": 3.0, \"push_s\": 0.5 }";
    let heave = "\"damage\": 15, \"line\": 2, \"stagger_s\": 0.8 }";
    for (label, from, to, key, attack) in [
        ("a pulse with only chill", pulse, "{ \"cost\": 60, \"range_m\": 5.0, \"cooldown_s\": 1.0, \"chill\": 0.5, \"freeze_s\": 2 }", "chill", "pulse"),
        ("a pulse that burns", pulse, "{ \"cost\": 60, \"range_m\": 5.0, \"cooldown_s\": 1.0, \"damage\": 2, \"burn_dps\": 4, \"burn_s\": 3 }", "burn_dps", "pulse"),
        ("a strike that burns", strike, "\"cooldown_s\": 1.5, \"damage\": 14, \"burn_dps\": 50, \"burn_s\": 5, \"chain\": 2,", "burn_dps", "strike"),
        ("a strike that chills", strike, "\"cooldown_s\": 1.5, \"damage\": 14, \"chill\": 0.5, \"freeze_s\": 2, \"chain\": 2,", "chill", "strike"),
        ("a strike that slows", strike, "\"cooldown_s\": 1.5, \"damage\": 14, \"slow\": 0.5, \"chain\": 2,", "slow", "strike"),
        ("a bolt that chains", bolt, "\"damage\": 10, \"projectile_mps\": 20, \"chain\": 8, \"chain_m\": 5 },", "chain", "bolt"),
        ("a bolt with wet_bonus", bolt, "\"damage\": 10, \"projectile_mps\": 20, \"wet_bonus\": 1 },", "wet_bonus", "bolt"),
        ("a gust with a line", gust, "\"push_m\": 3.0, \"push_s\": 0.5, \"line\": 2 }", "line", "gust"),
        ("a gust with a projectile", gust, "\"push_m\": 3.0, \"push_s\": 0.5, \"projectile_mps\": 10 }", "projectile_mps", "gust"),
        ("a heave that chills", heave, "\"damage\": 15, \"line\": 2, \"stagger_s\": 0.8, \"chill\": 0.5, \"freeze_s\": 1 }", "chill", "heave"),
        ("a heave that pushes", heave, "\"damage\": 15, \"line\": 2, \"stagger_s\": 0.8, \"push_m\": 2, \"push_s\": 0.5 }", "push_m", "heave"),
        ("a heave with splash", heave, "\"damage\": 15, \"line\": 2, \"stagger_s\": 0.8, \"splash_m\": 2 }", "splash_m", "heave"),
    ] {
        let err = proving_with(from, to).expect_err(label);
        let text = err.to_string();
        assert!(text.contains(key) && text.contains(&format!("a {attack} does not read {key}")), "{label}: {text}");
    }
    // What each attack reads stays accepted (the proving ground parses).
    Content::proving();
}

#[test]
fn a_strike_three_times_a_second_is_allowed_and_faster_is_refused() {
    let from = "\"cooldown_s\": 1.5, \"damage\": 14, \"chain\": 2,";
    let c = proving_with(from, "\"cooldown_s\": 0.3334, \"damage\": 14, \"chain\": 2,").expect("10 ticks: exactly three a second");
    assert_eq!(c.stone(c.stone_kind("fulmen").unwrap()).tiers[0].cooldown, 10);
    assert_eq!(crate::content::MIN_STRIKE_TICKS * 3, TICK_HZ);
    let err = proving_with(from, "\"cooldown_s\": 0.3, \"damage\": 14, \"chain\": 2,").expect_err("9 ticks: more than three a second");
    assert!(err.to_string().contains("cooldown_s"), "{err}");
}

#[test]
fn a_content_folder_plays_map_1_only_when_it_says_so() {
    let root = std::env::temp_dir().join(format!("sepulcra-slab-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let stones = PROVING_STONES_JSON.replace("\r\n", "\n");
    let write = |dir: &std::path::Path, stones: &str, slab: bool| {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("stones.json"), stones).unwrap();
        std::fs::write(dir.join("enemies.json"), PROVING_ENEMIES_JSON).unwrap();
        std::fs::write(dir.join("waves.json"), PROVING_WAVES_JSON).unwrap();
        if slab {
            std::fs::write(dir.join("slab_01.json"), SLAB_01_JSON).unwrap();
        }
    };
    let says = "  \"slab\": \"map_1\",\n";
    assert!(stones.contains(says), "test fixture");
    // Saying so, with no slab of its own: map 1's (the proving ground).
    write(&root.join("says"), &stones, false);
    let c = Content::load_dir(&root.join("says")).expect("plays on map 1");
    assert_eq!(c.hash, Content::proving().hash);
    // Not saying so, with no slab: an error that names the file, as before.
    write(&root.join("silent"), &stones.replace(says, ""), false);
    let err = Content::load_dir(&root.join("silent")).unwrap_err();
    assert!(err.to_string().contains("slab_01.json"), "{err}");
    // Its own slab and no word: its own. Both: refused.
    write(&root.join("own"), &stones.replace(says, ""), true);
    assert!(Content::load_dir(&root.join("own")).is_ok());
    write(&root.join("both"), &stones, true);
    let err = Content::load_dir(&root.join("both")).unwrap_err();
    assert!(err.to_string().contains("slab"), "{err}");
    // Another name is refused.
    let err = proving_with(says, "  \"slab\": \"map_2\",\n").unwrap_err();
    assert!(err.to_string().contains("slab"), "{err}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_burning_exploder_bursts_when_its_fuse_burns_down_unless_put_out() {
    let c = with_fuel("");
    let fuse = c.enemy(c.enemy_kind("tumidus").unwrap()).burst.unwrap().fuse;
    let mut s = quiet(&c);
    let bomb = plant(&mut s, "tumidus", "top:9,18");
    let (i, mut ev) = (idx(&s, bomb), Vec::new());
    s.w.ignite(&c, i, 1, 300, &mut ev);
    let lit = s.tick();
    let (before, _) = until_burst(&mut s, bomb, 10 * TICK_HZ).expect("the fuse burnt down");
    assert_eq!(before.tick(), lit + fuse, "it bursts on the tick its fuse ends");
    // Put out before the fuse ends, it never bursts.
    let mut s = quiet(&c);
    let bomb = plant(&mut s, "tumidus", "top:9,18");
    let (i, mut ev) = (idx(&s, bomb), Vec::new());
    s.w.ignite(&c, i, 1, 300, &mut ev);
    steps(&mut s, fuse / 2);
    let i = idx(&s, bomb);
    s.w.extinguish(i, &mut ev);
    assert!(until_burst(&mut s, bomb, 10 * TICK_HZ).is_none());
}

#[test]
fn a_shove_never_enters_a_wall_and_stops_at_a_spawn() {
    let c = with_fuel("");
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,18");
    let i = idx(&s, id);
    s.w.enemies[i].el.shove = 50_000;
    s.w.enemies[i].el.shove_rate = 50_000;
    steps(&mut s, 1);
    let e = s.enemy(id).unwrap();
    assert_eq!(e.cell(), Some(at(&c, "top:9,19")), "pinned at the spawn");
    assert_eq!(e.el.shove, 0);
    // Pushed toward a wall, it stops before it.
    let mut s = quiet(&c);
    let mut ev = Vec::new();
    s.w.dust = 1000;
    s.apply(&Command::PlaceWall { cell: at(&c, "top:9,17") }, &mut ev).unwrap();
    let id = plant(&mut s, "shambler", "top:9,16");
    let i = idx(&s, id);
    s.w.enemies[i].el.shove = 50_000;
    s.w.enemies[i].el.shove_rate = 50_000;
    steps(&mut s, 1);
    let e = s.enemy(id).unwrap();
    assert_ne!(e.cell(), Some(at(&c, "top:9,17")), "never into the wall");
}

/// An enemy of `kind` frozen solid (for a long time) on `cell`.
fn frozen_one(s: &mut State, kind: &str, cell: &str) -> EnemyId {
    let c = Arc::clone(&s.content);
    let gelu = c.stone(c.stone_kind("gelu").unwrap()).tiers[0].el;
    let id = plant(s, kind, cell);
    let i = idx(s, id);
    let mut ev = Vec::new();
    while !s.w.enemies[i].el.frozen(s.tick()) {
        s.w.chill(&c, i, &gelu, &mut ev);
    }
    s.w.enemies[i].el.frozen_until = 100_000;
    id
}

#[test]
fn every_attack_decides_its_elements_from_the_enemy_before_the_hit() {
    // The one rule (`Before`), through each stone's real hit with its real damage, on a shambler
    // and on an exploder, both frozen: the hit shatters the ice, and its elements see the enemy as
    // it was, frozen. Fire thaws it with steam (no fire, no fuse), frost adds no chill to the ice it
    // broke, and no hit sets the frozen exploder off.
    let c = with_fuel("");
    for stone in ["warding", "rogus", "gelu", "rune_pillar", "fulmen", "saxum"] {
        for kind in ["shambler", "tumidus"] {
            let mut s = quiet(&c);
            let id = frozen_one(&mut s, kind, "top:9,16");
            let i = idx(&s, id);
            s.w.enemies[i].hp = 1_000_000;
            hold_still_but_shove(&mut s);
            place(&mut s, stone, "top:10,15", 0);
            let mut hit_tick = None;
            for _ in 0..(10 * TICK_HZ) {
                let ev = steps(&mut s, 1);
                if ev.iter().any(|e| matches!(e, Event::Shattered { enemy, .. } if *enemy == id)) {
                    hit_tick = Some(ev);
                    break;
                }
            }
            let ev = hit_tick.unwrap_or_else(|| panic!("{stone} on a {kind}: never hit"));
            let label = format!("{stone} on a frozen {kind}");
            assert!(!ev.iter().any(|e| matches!(e, Event::Ignited { enemy } if *enemy == id)), "{label}: set alight: {ev:?}");
            assert!(!ev.iter().any(|e| matches!(e, Event::Frozen { enemy } if *enemy == id)), "{label}: frozen again at once: {ev:?}");
            assert!(!detonated(&ev, id), "{label}: burst: {ev:?}");
            let e = *s.enemy(id).expect("alive");
            let t = s.tick();
            assert!(!e.el.burning(t) && e.el.fuse_at == 0, "{label}: {:?}", e.el);
            if stone == "rogus" {
                assert!(ev.contains(&Event::Thawed { enemy: id, steam: true }), "{label}: thawed with steam: {ev:?}");
                assert!(e.el.wet(t), "{label}: wet after the steam");
            }
            if stone == "gelu" {
                assert_eq!(e.el.chill, 0, "{label}: no chill on the ice it broke");
            }
            // Nothing set it off later either (the stone and its shots gone).
            s.w.projectiles.clear();
            let mut sell = Vec::new();
            s.apply(&Command::Sell { cell: at(&c, "top:10,15") }, &mut sell).unwrap();
            let later = steps(&mut s, 3 * TICK_HZ);
            assert!(!detonated(&later, id) && !later.iter().any(|e| matches!(e, Event::Ignited { enemy } if *enemy == id)), "{label}: later: {later:?}");
        }
    }
}

#[test]
fn bursts_are_counted_when_they_go_off_and_the_counts_agree() {
    let c = proving();
    let text = include_str!("../data/proving/layouts/fire_earth.json");
    let script = crate::script::Script::parse(text, &c, "fire_earth").expect("layout");
    let (mut events, mut total) = (0u32, 0u32);
    for seed in 1..=6 {
        let mut n = 0u32;
        let r = crate::script::run(Arc::clone(&c), seed, &script, None, |_, ev| n += ev.iter().filter(|e| matches!(e, Event::Detonated { .. })).count() as u32);
        let counted: u32 = r.waves.iter().map(|w| w.detonated).sum();
        assert_eq!(counted, n, "seed {seed}: the waves' count is the bursts that went off");
        events += n;
        total += counted;
    }
    assert!(events > 0 && events == total, "{events} bursts");
}

/// A Ventus III that pushes `push_m` over 5 s and recharges one tick later.
fn slow_gust(push_m: f64) -> String {
    format!("{{ \"cost\": 90, \"range_m\": 6.5, \"cooldown_s\": 5.034, \"push_m\": {push_m:.1}, \"push_s\": 5.0 }}")
}

/// A lone enemy of `kind` set down on `from` with four tier-III cairns of `c` round `ring`'s
/// centre: the ticks until it is through (it cannot be killed: the wind deals nothing), at most
/// `limit`, the gusts that shoved it, and whether it entered furor.
fn lone_under_wind(c: &Arc<Content>, kind: &str, from: &str, ring: [&str; 4], limit: u64) -> (u64, u32, bool) {
    let mut s = quiet(c);
    let id = plant(&mut s, kind, from);
    for cell in ring {
        place(&mut s, "ventus", cell, 2);
    }
    let mut ev = Vec::new();
    let (mut ticks, mut gusts, mut furor) = (0u64, 0u32, false);
    while s.enemy(id).is_some() && ticks <= limit {
        ev.clear();
        s.step_into(&[], &mut ev);
        ticks += 1;
        gusts += ev.iter().filter(|e| matches!(e, Event::Gusted { .. })).count() as u32;
        furor |= ev.iter().any(|e| matches!(e, Event::Furor { .. }));
    }
    assert!(s.enemy(id).is_none(), "a lone {kind} still held after {ticks} ticks ({gusts} gusts), past {limit}");
    assert_eq!(s.wave_stats()[0].leaked, 1, "the {kind} walked into the graveyard");
    (ticks, gusts, furor)
}

/// Where the lone enemies of these tests walk: from the south-west gorge, along the lane to the
/// south gate, through a ring of cairns round top:9,13 far enough from the gorge that a shove of
/// up to about 11 m does not pin them at it.
const GORGE: &str = "top:4,19";
const RING: [&str; 4] = ["top:8,12", "top:10,12", "top:8,14", "top:10,14"];

/// The stock wind holds nobody near a wave's hour: a lone enemy of each ground kind, shoved at
/// every turn by a ring of four tier-III cairns, is through in minutes, never in furor.
#[test]
fn the_stock_wind_lets_a_lone_enemy_through_long_before_its_waves_hour() {
    let c = proving();
    let hour = u64::from(quiet(&c).w.hour_of(&c, 1).unwrap());
    for kind in ["shambler", "runner", "tumidus", "colossus"] {
        let (ticks, gusts, furor) = lone_under_wind(&c, kind, GORGE, RING, hour);
        assert!(ticks * 10 < hour && gusts > 0 && !furor, "{kind}: {ticks} ticks, {gusts} gusts, furor {furor}, hour {hour}");
    }
}

/// Where the frost tests' Gelu stones stand.
#[derive(Clone, Copy, Debug)]
enum Frost {
    /// The first n of [`RING`]; the enemy is set down on [`GORGE`] and walks through them.
    Ring(usize),
    /// The n buildable cells nearest [`AMID`], where the enemy is set down, all placed in the
    /// tick it is: they fire together.
    Round(usize),
}

/// Where [`Frost::Round`] sets its enemy down: a lane cell beside [`RING`]'s centre.
const AMID: &str = "top:9,13";

/// The `n` buildable cells nearest `cell` on its face, nearest first (ties by id).
fn nearest_build_cells(c: &Content, cell: &str, n: usize) -> Vec<CellId> {
    let centre = c.map.cell(at(c, cell));
    let mut near: Vec<&crate::map::Cell> = c.map.cells().iter().filter(|x| x.face == centre.face && x.kind.buildable()).collect();
    near.sort_by_key(|x| (x.pos.dist2(centre.pos), x.id));
    near.iter().take(n).map(|x| x.id).collect()
}

/// A lone `kind` at the HP of the largest wave scale under Gelu stones of `tier` stood as `at`,
/// played from `seed`: the ticks until it is through or dead (at most `limit`), the ticks it
/// stood frozen, and whether it entered furor.
fn lone_under_frost(c: &Arc<Content>, kind: &str, at_: Frost, tier: u8, seed: u64, limit: u64) -> (u64, u64, bool) {
    let mut s = State::new(Arc::clone(c), seed);
    s.w.next_wave_tick = None;
    s.w.wave = 1;
    s.w.wave_stats[0].started_at = Some(0);
    let k = c.enemy_kind(kind).unwrap();
    let scale = c.waves.iter().map(|w| w.hp_scale).max().unwrap_or(1000).max(1000);
    let hp = (i64::from(c.enemy(k).hp) * i64::from(scale) / 1000) as i32;
    let (from, cells): (&str, Vec<CellId>) = match at_ {
        Frost::Ring(n) => (GORGE, RING[..n].iter().map(|x| at(c, x)).collect()),
        Frost::Round(n) => (AMID, nearest_build_cells(c, AMID, n)),
    };
    let id = plant(&mut s, kind, from);
    let i = idx(&s, id);
    (s.w.enemies[i].hp, s.w.enemies[i].max_hp) = (hp, hp);
    let gelu = c.stone_kind("gelu").unwrap();
    let mut ev = Vec::new();
    s.w.dust = 1_000_000;
    for cell in cells {
        s.apply(&Command::PlaceStone { cell, stone: gelu }, &mut ev).expect("placed");
        for _ in 0..tier {
            s.apply(&Command::Upgrade { cell }, &mut ev).expect("upgraded");
        }
    }
    let (mut ticks, mut frozen, mut furor) = (0u64, 0u64, false);
    while s.enemy(id).is_some() && ticks <= limit {
        ev.clear();
        s.step_into(&[], &mut ev);
        ticks += 1;
        furor |= ev.iter().any(|e| matches!(e, Event::Furor { .. }));
        if s.enemy(id).is_some_and(|e| e.el.frozen(s.tick())) {
            frozen += 1;
        }
    }
    assert!(s.enemy(id).is_none(), "a lone {kind} ({at_:?}, tier {tier}, seed {seed}) still held after {ticks} ticks, past {limit}");
    (ticks, frozen, furor)
}

/// Gelu III as the proving ground's stones.json writes it.
const GELU_III: &str = "{ \"cost\": 100, \"range_m\": 7.5, \"cooldown_s\": 0.9, \"damage\": 14, \"projectile_mps\": 20, \"slow\": 0.35, \"slow_s\": 1.5, \"chill\": 0.6, \"freeze_s\": 3.0, \"shatter\": 2.0 }";

/// The stock frost holds nobody near a wave's hour: the four ground kinds at the largest wave
/// scale's HP, under Gelu I, II and III, walking through four or set down amid eight that fire
/// together, over three seeds, are through or dead in minutes, never in furor.
#[test]
fn the_stock_frost_lets_a_lone_enemy_through_or_kills_it_long_before_its_waves_hour() {
    let c = proving();
    let hour = u64::from(quiet(&c).w.hour_of(&c, 1).unwrap());
    let mut most = (0u64, String::new());
    for kind in ["shambler", "runner", "tumidus", "colossus"] {
        for tier in 0..3u8 {
            for at_ in [Frost::Ring(4), Frost::Round(8)] {
                for seed in 1..=3 {
                    let (ticks, frozen, furor) = lone_under_frost(&c, kind, at_, tier, seed, hour);
                    let what = format!("{kind} {at_:?} tier {} seed {seed}: {ticks} ticks, {frozen} frozen", tier + 1);
                    assert!(!furor && ticks * 4 < hour, "{what}; hour {hour}");
                    if ticks > most.0 {
                        most = (ticks, what);
                    }
                }
            }
        }
    }
    assert!(most.0 > 0, "{most:?}");
}

/// A leaper's chance leaps with no shoves (DESIGN.md, "Furor": the run's `leaps` term): it rolls
/// only when it walks onto a jump point, so it leaps at most once at each, and its whole way takes
/// no longer than the run's walk and leaps. Over twenty seeds from each gorge, some leap.
#[test]
fn a_runners_chance_leaps_stay_within_the_runs_stated_leaps() {
    let c = proving();
    let b = c.run_bound();
    let jumps = c.map.cells().iter().filter(|x| !c.map.jump_partners(x.id).is_empty()).count() as u64;
    assert_eq!(jumps, 4, "two ledges, a jump point at each end");
    let mut leapt = 0u32;
    for gorge in ["top:4,19", "top:15,19"] {
        for seed in 1..=20 {
            let mut s = State::new(Arc::clone(&c), seed);
            s.w.next_wave_tick = None;
            s.w.wave = 1;
            s.w.wave_stats[0].started_at = Some(0);
            let id = plant(&mut s, "runner", gorge);
            let mut ev = Vec::new();
            let (mut ticks, mut leaps) = (0u64, 0u64);
            while s.enemy(id).is_some() && ticks <= b.walk + b.leaps {
                ev.clear();
                s.step_into(&[], &mut ev);
                ticks += 1;
                leaps += ev.iter().filter(|e| matches!(e, Event::Jumped { enemy, .. } if *enemy == id)).count() as u64;
            }
            assert!(s.enemy(id).is_none() && leaps <= jumps, "{gorge} seed {seed}: {ticks} ticks, {leaps} leaps, against {} and {jumps}", b.walk + b.leaps);
            leapt += u32::from(leaps > 0);
        }
    }
    assert!(leapt > 0, "some runner leapt");
}

/// The rules hold no budget of the app's drawing: more cells may burn than the app draws grass
/// for (it draws the fuel first and says in a trace what it leaves out).
#[test]
fn more_fuel_than_the_app_draws_is_legal() {
    let c = with_fuel("\"top:0,2..top:19,2\", \"top:0,3..top:19,3\", \"top:0,4..top:19,4\", \"top:0,5..top:19,5\"");
    assert!(c.fuel.len() > crate::fuel::MAX_GRASS, "{} cells burn", c.fuel.len());
}

/// A burn of nothing is no fire, whatever its end says; and since the rules set a burn and its
/// end together, a save holding one without the other is refused.
#[test]
fn a_burn_of_nothing_is_no_fire_and_no_save_holds_one() {
    let nothing = EnemyEl { burn: 0, burn_until: 100, ..EnemyEl::default() };
    assert!(!nothing.burning(0) && !nothing.burning(99));
    assert!(EnemyEl { burn: 1, burn_until: 100, ..EnemyEl::default() }.burning(99));
    let c = proving();
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,15");
    let i = idx(&s, id);
    assert!(State::from_bytes(Arc::clone(&c), &s.to_bytes()).is_ok());
    s.w.enemies[i].el = nothing;
    let e = State::from_bytes(Arc::clone(&c), &s.to_bytes()).unwrap_err();
    assert!(e.message.contains("burn_until"), "{e}");
    s.w.enemies[i].el = EnemyEl { burn: 5, burn_until: 0, ..EnemyEl::default() };
    assert!(State::from_bytes(Arc::clone(&c), &s.to_bytes()).is_err());
}

/// A layout entry refused because one of the dead stands on its cell waits, and is counted as one
/// wait as long as the enemy stood there.
#[test]
fn a_layout_waits_for_one_of_the_dead_to_leave_a_cell() {
    let c = with_fuel("");
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,14");
    hold_still(&mut s);
    let text = "{ \"name\": \"wall\", \"commands\": [ { \"wave\": 0, \"wall\": \"top:9,14\" } ] }";
    let mut p = crate::script::Player::new(crate::script::Script::parse(text, &c, "wall").expect("layout"));
    let mut ev = Vec::new();
    for _ in 0..10 {
        assert!(p.drive(&mut s, &mut ev).is_empty(), "the wall waits while the shambler stands on its cell");
        s.step_into(&[], &mut ev);
    }
    let i = idx(&s, id);
    s.w.enemies[i].el.stagger_until = 0;
    let mut t = 10u32;
    while p.drive(&mut s, &mut ev).is_empty() {
        s.step_into(&[], &mut ev);
        t += 1;
        assert!(t < 60 * TICK_HZ, "the wall never went up: {:?}", p.skipped);
    }
    assert!(p.skipped.is_empty(), "{:?}", p.skipped);
    let w = p.waits_at(s.tick());
    assert_eq!((w.entries, w.longest, w.never_applied), (1, t, 0), "{w:?}");
}

// ---- Rubble is temporary (the owner, 2026-11-28: "rendering those stones in the path as temporary") ----

/// The proving ground with Saxum III replaced by `tier`.
fn saxum_iii(tier: &str) -> Arc<Content> {
    Arc::new(proving_with("{ \"cost\": 110, \"range_m\": 7.0, \"cooldown_s\": 2.6, \"damage\": 36, \"line\": 4, \"stagger_s\": 1.2, \"rubble\": { \"hp\": 200, \"s\": 8 } }", tier).expect("legal content"))
}

/// The fourth review's rubble: no damage, no stagger, rubble of 2,000 HP for 60 s every second.
const REVIEW_RUBBLE: &str = "{ \"cost\": 110, \"range_m\": 7.0, \"cooldown_s\": 1.0, \"line\": 4, \"rubble\": { \"hp\": 2000, \"s\": 60 } }";

/// What a lone enemy's run came to.
#[derive(Debug, Default)]
struct Lone {
    ticks: u64,
    gone: bool,
    leaked: bool,
    /// Rubble raised, and the most raised on one cell.
    raised: u32,
    most_on_a_cell: u32,
    furor: u32,
}

/// Plays a lone `kind` set down on `from` (wave 1, started at tick 0) among `stones` (name,
/// cell, tier), with furor on or off, until it is gone or `limit` ticks have passed.
fn lone(c: &Arc<Content>, kind: &str, from: &str, stones: &[(&str, CellId, u8)], furor: bool, limit: u64) -> Lone {
    let mut s = quiet(c);
    s.w.no_furor = !furor;
    let id = plant(&mut s, kind, from);
    s.w.dust = 10_000_000;
    let mut ev = Vec::new();
    for &(name, cell, tier) in stones {
        s.apply(&Command::PlaceStone { cell, stone: c.stone_kind(name).unwrap() }, &mut ev).expect("placed");
        for _ in 0..tier {
            s.apply(&Command::Upgrade { cell }, &mut ev).expect("upgraded");
        }
    }
    let mut on: BTreeMap<CellId, u32> = BTreeMap::new();
    let mut r = Lone::default();
    while s.enemy(id).is_some() && r.ticks < limit {
        ev.clear();
        s.step_into(&[], &mut ev);
        r.ticks += 1;
        for e in &ev {
            match e {
                Event::RubbleRaised { cell } => {
                    r.raised += 1;
                    *on.entry(*cell).or_default() += 1;
                }
                Event::Furor { .. } => r.furor += 1,
                _ => {}
            }
        }
    }
    r.gone = s.enemy(id).is_none();
    r.leaked = s.wave_stats()[0].leaked == 1;
    r.most_on_a_cell = on.values().copied().max().unwrap_or(0);
    r
}

/// A heave on standing rubble restores neither its health nor its life: the rubble the dead
/// have half broken stays half broken, and crumbles when it was to.
#[test]
fn a_heave_never_restores_standing_rubble() {
    let c = saxum_iii(REVIEW_RUBBLE);
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,18");
    hold_still_but_shove(&mut s);
    place(&mut s, "saxum", "top:10,15", 2);
    let ev = steps(&mut s, 1);
    let cell = ev.iter().find_map(|e| if let Event::RubbleRaised { cell } = e { Some(*cell) } else { None }).expect("rubble rose");
    let first = s.rubble().find(|r| r.cell == cell).unwrap();
    assert_eq!((first.hp, first.max_hp, first.until - first.raised), (2_000_000, 2_000_000, 60 * TICK_HZ));
    s.w.walls.get_mut(&cell).unwrap().hp = 900_000;
    let ev = steps(&mut s, 5 * TICK_HZ);
    assert!(ev.iter().filter(|e| matches!(e, Event::Heaved { .. })).count() >= 4, "the stone kept heaving: {ev:?}");
    let now = s.rubble().find(|r| r.cell == cell).expect("still standing");
    assert_eq!((now.hp, now.raised, now.until), (900_000, first.raised, first.until), "neither its health nor its life came back");
    let _ = id;
}

/// A cell whose rubble has gone takes none again while one of the dead that lived then still
/// stands; once they have all gone it may.
#[test]
fn a_cell_whose_rubble_went_takes_none_while_its_dead_stand() {
    let c = saxum_iii("{ \"cost\": 110, \"range_m\": 7.0, \"cooldown_s\": 0.5, \"line\": 4, \"rubble\": { \"hp\": 2000, \"s\": 2 } }");
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,18");
    hold_still_but_shove(&mut s);
    place(&mut s, "saxum", "top:10,15", 2);
    let ev = steps(&mut s, 1);
    let cell = ev.iter().find_map(|e| if let Event::RubbleRaised { cell } = e { Some(*cell) } else { None }).expect("rubble rose");
    let ev = steps(&mut s, 20 * TICK_HZ);
    assert!(ev.contains(&Event::WallCrumbled { cell }), "it crumbled");
    assert!(!ev.contains(&Event::RubbleRaised { cell }), "and never rose there again while the shambler stood: {ev:?}");
    assert!(s.w.barred(cell));
    // The shambler goes; a newcomer does not bar the cell.
    let i = idx(&s, id);
    s.w.enemies[i].alive = false;
    s.w.wave_stats[0].alive -= 1;
    steps(&mut s, 1);
    plant(&mut s, "shambler", "top:9,18");
    hold_still_but_shove(&mut s);
    assert!(!s.w.barred(cell), "nobody who saw it go still stands");
    let ev = steps(&mut s, 5 * TICK_HZ);
    assert!(ev.iter().any(|e| matches!(e, Event::RubbleRaised { .. })), "rubble rises again: {ev:?}");
}

/// The fourth review's case: a lone shambler stood 100,000 ticks at full health attacking
/// rubble raised again as it crumbled. By rubble's own rule (furor switched off), it now passes:
/// each cell takes rubble at most once in its life, and each rubble holds it at most the lesser
/// of its life and the time its claws take to break it.
#[test]
fn the_reviews_rubble_lets_a_lone_enemy_through_by_its_own_rule() {
    let c = saxum_iii(REVIEW_RUBBLE);
    let cell = nearest_build_cells(&c, AMID, 1)[0];
    let stones = [("saxum", cell, 2u8)];
    let control = lone(&saxum_iii("{ \"cost\": 110, \"range_m\": 7.0, \"cooldown_s\": 1.0, \"line\": 4 }"), "shambler", GORGE, &stones, false, 100_000);
    let r = lone(&c, "shambler", GORGE, &stones, false, 100_000);
    let t = c.stone(c.stone_kind("saxum").unwrap()).tiers[2].el;
    let claws = c.enemy(c.enemy_kind("shambler").unwrap()).wall_damage;
    let each = u64::from(t.rubble_ticks).min((t.rubble_hp as u64).div_ceil(claws as u64));
    assert!(r.gone && r.leaked && r.furor == 0, "{r:?} (control {control:?}, each rubble at most {each} ticks)");
    assert_eq!(r.most_on_a_cell, 1, "{r:?}");
    assert!(r.ticks <= control.ticks + u64::from(r.raised) * each, "{r:?} against {} + {} x {each}", control.ticks, r.raised);
}

// ---- Furor (DESIGN.md, "Furor"): one rule for every hold ----

/// The proving ground's first wave: fourteen shamblers, one every 0.9 s (27 ticks), so its
/// last enters 351 ticks after it begins, and its hour is twenty minutes after that.
#[test]
fn a_waves_hour_is_twenty_minutes_after_its_last_entry() {
    let c = proving();
    assert_eq!(c.waves[0].entry_span(), 13 * 27);
    let s = quiet(&c);
    assert_eq!(s.w.hour_of(&c, 1), Some(351 + 20 * 60 * TICK_HZ));
    assert_eq!(s.w.hour_of(&c, 2), None, "wave 2 has not begun");
    // A wave called at another tick has its hour from that tick.
    let mut s = quiet(&c);
    s.w.wave_stats[0].started_at = Some(500);
    assert_eq!(s.w.hour_of(&c, 1), Some(500 + 351 + crate::content::WAVE_HOUR_TICKS));
}

/// A shambler of wave 1 held every way at once, one tick before its wave's hour.
fn held_at_the_hour(c: &Arc<Content>) -> (State, EnemyId, u32) {
    let mut s = quiet(c);
    let hour = s.w.hour_of(c, 1).unwrap();
    s.w.tick = hour - 1;
    let id = plant(&mut s, "shambler", "top:9,15");
    let i = idx(&s, id);
    let far = hour + 10_000;
    let e = &mut s.w.enemies[i];
    (e.slow, e.slow_until) = (900, far);
    (e.el.frozen_until, e.el.shatter, e.el.chill, e.el.stagger_until) = (far, 500, 400, far);
    (e.el.shove, e.el.shove_rate) = (50_000, 1);
    (s, id, hour)
}

/// At its wave's hour an enemy still standing is in furor: whatever holds or slows it ends at
/// once, said once, and it walks on at its own pace.
#[test]
fn at_its_waves_hour_whatever_holds_an_enemy_ends_at_once() {
    let c = proving();
    let (mut s, id, hour) = held_at_the_hour(&c);
    let start = s.enemy(id).unwrap().pos;
    let ev = steps(&mut s, 1);
    assert_eq!(s.tick(), hour, "the tick before its hour has been played");
    let e = *s.enemy(id).unwrap();
    assert!(e.el.frozen(s.tick()) && e.pos == start && !ev.iter().any(|x| matches!(x, Event::Furor { .. })), "held the tick before: {ev:?}");
    let ev = steps(&mut s, 1);
    assert_eq!(ev.iter().filter(|x| matches!(x, Event::Furor { enemy } if *enemy == id)).count(), 1, "{ev:?}");
    assert!(ev.contains(&Event::Thawed { enemy: id, steam: false }), "the ice goes: {ev:?}");
    let e = *s.enemy(id).unwrap();
    assert!(s.w.in_furor(&c, &e));
    assert_eq!((e.slow_until, e.el.frozen_until, e.el.chill, e.el.stagger_until, e.el.shove), (0, 0, 0, 0, 0), "{e:?}");
    assert!(e.pos != start, "it walked on that tick");
    let ev = steps(&mut s, 10);
    assert!(!ev.iter().any(|x| matches!(x, Event::Furor { .. })), "said once only");
}

/// Every writer of a hold asks [`World::can_hold`]: on an enemy in furor a freeze, a chill, a
/// shove (a gust's or a blast's), a stone's slow and the roots do nothing; a heave hurts it but
/// does not stagger it.
#[test]
fn no_writer_of_a_hold_holds_an_enemy_in_furor() {
    let c = proving();
    let (mut s, id, _) = held_at_the_hour(&c);
    steps(&mut s, 2);
    let i = idx(&s, id);
    assert!(s.w.in_furor(&c, &s.w.enemies[i]));
    let mut ev = Vec::new();
    let gelu = c.stone(c.stone_kind("gelu").unwrap()).tiers[2].el;
    let frost = TierEl { chill: 1000, freeze_ticks: 900, ..gelu };
    s.w.chill(&c, i, &frost, &mut ev);
    assert_eq!((s.w.enemies[i].el.chill, s.w.enemies[i].el.frozen_until), (0, 0), "no chill, no freeze");
    s.w.shove(&c, i, 30_000, 30);
    assert_eq!(s.w.enemies[i].el.shove, 0, "no shove");
    s.w.slow_enemy(&c, i, 900, 18_000);
    assert_eq!(s.w.enemies[i].slow_until, 0, "no slow");
    let e = s.w.enemies[i];
    assert_eq!(s.w.speed_permille(&c, &e, false), 1000, "no slow and no roots in its pace");
    // A heave: hurt, not staggered.
    let hp = s.w.enemies[i].hp;
    let cell = nearest_build_cells(&c, "top:9,15", 1)[0];
    s.w.dust = 100_000;
    s.apply(&Command::PlaceStone { cell, stone: c.stone_kind("saxum").unwrap() }, &mut ev).unwrap();
    let ev = steps(&mut s, 1);
    assert!(ev.iter().any(|x| matches!(x, Event::Heaved { affected: 1, .. })), "{ev:?}");
    let e = *s.enemy(id).unwrap();
    assert!(e.hp < hp && e.el.stagger_until == 0, "{e:?}");
    // And the same writers do hold an enemy that is not in furor (the control).
    let mut s = quiet(&c);
    let j = plant(&mut s, "shambler", "top:9,15");
    let j = idx(&s, j);
    let mut ev = Vec::new();
    s.w.chill(&c, j, &frost, &mut ev);
    s.w.slow_enemy(&c, j, 900, 18_000);
    assert!(s.w.enemies[j].el.frozen_until > 0 && s.w.enemies[j].slow_until > 0);
    s.w.enemies[j].el.frozen_until = 0;
    s.w.shove(&c, j, 30_000, 30);
    assert!(s.w.enemies[j].el.shove > 0);
    let e = s.w.enemies[j];
    assert!(s.w.speed_permille(&c, &e, false) < 1000);
}

/// While any of the dead is in furor no rubble stands and none rises; an enemy in furor calls
/// no summons.
#[test]
fn furor_crumbles_rubble_raises_none_and_calls_no_summons() {
    let c = proving();
    let (mut s, id, hour) = held_at_the_hour(&c);
    let i = idx(&s, id);
    (s.w.enemies[i].hp, s.w.enemies[i].max_hp) = (50_000_000, 50_000_000);
    let rubble = at(&c, "top:9,17");
    s.w.walls.insert(rubble, Wall { hp: 100_000_000, max_hp: 100_000_000, cost: 0 });
    s.w.rubble.insert(rubble, (hour - 2, hour + 100_000));
    s.w.set_blocked(&c, rubble, true);
    let cell = nearest_build_cells(&c, "top:9,15", 1)[0];
    s.w.dust = 100_000;
    let mut ev = Vec::new();
    s.apply(&Command::PlaceStone { cell, stone: c.stone_kind("saxum").unwrap() }, &mut ev).unwrap();
    s.apply(&Command::Upgrade { cell }, &mut ev).unwrap();
    s.apply(&Command::Upgrade { cell }, &mut ev).unwrap();
    let ev = steps(&mut s, 2);
    assert!(ev.contains(&Event::WallCrumbled { cell: rubble }), "{ev:?}");
    let ev = steps(&mut s, 10 * TICK_HZ);
    assert!(ev.iter().any(|x| matches!(x, Event::Heaved { .. })) && !ev.iter().any(|x| matches!(x, Event::RubbleRaised { .. })), "heaves, no rubble: {ev:?}");
    assert_eq!(s.rubble().count(), 0);
    let _ = id;
    // A colossus in furor, its next call due: none comes.
    let mut s = quiet(&c);
    let hour = s.w.hour_of(&c, 1).unwrap();
    s.w.tick = hour;
    let big = plant(&mut s, "colossus", "top:9,13");
    let i = idx(&s, big);
    s.w.enemies[i].next_summon = hour;
    let ev = steps(&mut s, 30 * TICK_HZ);
    assert!(!ev.iter().any(|x| matches!(x, Event::Spawned { .. })), "{ev:?}");
    // The control: the same colossus before its hour calls two.
    let mut s = quiet(&c);
    let big = plant(&mut s, "colossus", "top:9,13");
    let i = idx(&s, big);
    s.w.enemies[i].next_summon = 0;
    let ev = steps(&mut s, 1);
    assert_eq!(ev.iter().filter(|x| matches!(x, Event::Spawned { .. })).count(), 2, "{ev:?}");
}

/// The owner's rule, on the one cell a heave's line can share with standing rubble (the target's
/// own, when the heave kills it there): a heave on standing rubble the dead have half broken and
/// that has aged restores neither its health nor its life.
#[test]
fn a_heave_on_damaged_ageing_rubble_restores_neither_its_health_nor_its_life() {
    let c = saxum_iii("{ \"cost\": 110, \"range_m\": 7.0, \"cooldown_s\": 1.0, \"damage\": 36, \"line\": 1, \"rubble\": { \"hp\": 2000, \"s\": 60 } }");
    let mut s = quiet(&c);
    s.w.tick = 600;
    let cell = at(&c, "top:9,17");
    let id = plant(&mut s, "shambler", "top:9,17");
    hold_still(&mut s);
    let i = idx(&s, id);
    s.w.enemies[i].hp = 1;
    s.w.walls.insert(cell, Wall { hp: 900_000, max_hp: 2_000_000, cost: 0 });
    s.w.rubble.insert(cell, (100, 100 + 60 * TICK_HZ));
    s.w.set_blocked(&c, cell, true);
    let stone = c.map.cell_name(nearest_build_cells(&c, "top:9,17", 1)[0]);
    place(&mut s, "saxum", &stone, 2);
    let before = s.rubble().find(|r| r.cell == cell).unwrap();
    let ev = steps(&mut s, 1);
    assert!(ev.iter().any(|e| matches!(e, Event::Heaved { affected: 1, .. })) && s.enemy(id).is_none(), "the heave killed it on the rubble's cell: {ev:?}");
    assert!(!ev.contains(&Event::RubbleRaised { cell }), "{ev:?}");
    let after = s.rubble().find(|r| r.cell == cell).expect("still standing");
    assert_eq!((before.hp, before.max_hp, before.raised, before.until), (900_000, 2_000_000, 100, 100 + 60 * TICK_HZ));
    assert_eq!((after.hp, after.max_hp, after.raised, after.until), (before.hp, before.max_hp, before.raised, before.until), "neither its health nor its life came back");
}

/// What a rubble bar shows is the lesser of its health and its life: health the lesser in one
/// case, life in the other, and the two equal at the boundary.
#[test]
fn a_rubble_bar_is_the_lesser_of_health_and_life() {
    let r = |hp: i32, tick: u32| RubbleStand { cell: CellId(0), hp, max_hp: 1_000_000, raised: 0, until: 1000 }.left_permille(tick);
    assert_eq!(r(300_000, 100), 300, "health the lesser (life 900)");
    assert_eq!(r(900_000, 700), 300, "life the lesser (health 900)");
    assert_eq!(r(500_000, 500), 500, "the two equal");
    assert_eq!((r(0, 0), r(1_000_000, 1000), r(1_000_000, 2000)), (0, 0, 0), "broken, or its life over");
}

/// An enemy in furor standing on a rooted cell walks at its own pace; the same enemy before its
/// hour is slowed by the roots alone (no stone's slow on it).
#[test]
fn the_roots_do_not_slow_an_enemy_in_furor() {
    let c = proving();
    assert!(c.rules.nature.slow > 0);
    let (mut s, id, _) = held_at_the_hour(&c);
    steps(&mut s, 2);
    let e = *s.enemy(id).unwrap();
    assert!(s.w.in_furor(&c, &e) && !e.is_slowed(s.tick()));
    s.w.rooted[e.cell().unwrap().index()] = true;
    assert_eq!(s.w.speed_permille(&c, &e, false), 1000, "in furor: no roots");
    let mut s = quiet(&c);
    let j = plant(&mut s, "shambler", "top:9,15");
    let e = *s.enemy(j).unwrap();
    s.w.rooted[e.cell().unwrap().index()] = true;
    assert!(!e.is_slowed(s.tick()));
    assert_eq!(s.w.speed_permille(&c, &e, false), i64::from(1000 - c.rules.nature.slow), "before its hour the roots slow it");
}

/// The shipped content with walls of the most HP the format takes, and one wave only, and a
/// layout that closes every gate with them before the dead come: a run that cannot end.
fn sealed_shipped() -> (Arc<Content>, crate::script::Script) {
    use crate::content::{ENEMIES_JSON, STONES_JSON, WAVES_JSON};
    let stones = STONES_JSON.replace("\r\n", "\n");
    let from = "\"wall\": { \"cost\": 15, \"hp\": 300,";
    assert_eq!(stones.matches(from).count(), 1, "fixture");
    let stones = stones.replacen(from, "\"wall\": { \"cost\": 15, \"hp\": 1000000,", 1);
    let waves = WAVES_JSON.replace("\r\n", "\n");
    let cut = waves.find("]},").expect("fixture") + 2;
    let waves = format!("{}\n  ]\n}}\n", &waves[..cut]);
    let c = Arc::new(Content::from_texts(SLAB_01_JSON, &stones, ENEMIES_JSON, &waves).expect("content"));
    assert_eq!(c.waves.len(), 1);
    let text = "{ \"name\": \"sealed\", \"commands\": [ { \"wave\": 0, \"wall\": \"top:10,7\" }, { \"wave\": 0, \"wall\": \"top:12,10\" }, { \"wave\": 0, \"wall\": \"top:9,12\" }, { \"wave\": 0, \"wall\": \"top:7,9\" } ] }";
    let script = crate::script::Script::parse(text, &c, "sealed").expect("layout");
    (c, script)
}

/// Furor is elemental content's only (DESIGN.md, "Furor"): in the shipped game a wave walled in
/// past its hour is never in furor, and the rune pillar's slow, nature's roots and the colossus's
/// summons still act at a tick past that hour, as the Lex rules (which have no furor) play them.
#[test]
fn in_the_shipped_game_a_wave_walled_in_past_its_hour_is_still_held() {
    let (c, script) = sealed_shipped();
    assert!(!c.elemental());
    let (mut past, mut furor) = (0u32, 0u32);
    let r = crate::script::run(Arc::clone(&c), 1, &script, None, |st, ev| {
        furor += ev.iter().filter(|e| matches!(e, Event::Furor { .. })).count() as u32;
        if st.w.hour_of(&c, 1).is_some_and(|h| st.tick() > h) {
            past += 1;
            assert!(st.w.enemies.iter().all(|e| !e.alive || (!st.w.in_furor(&c, e) && st.w.can_hold(&c, e))), "tick {}", st.tick());
        }
    });
    assert!(past > 0 && r.waves[0].alive > 0, "the dead stand at the walls past their wave's hour: past {past}, {:?}", r.waves);
    assert_eq!(furor, 0, "no furor in the shipped game");
    // Each of the three holds acts on the shipped content ten seconds past wave 1's hour.
    let past_hour = |s: &mut State| {
        let hour = s.w.hour_of(&c, 1).unwrap();
        s.w.tick = hour + 10 * TICK_HZ;
    };
    // The rune pillar's slow.
    let mut s = quiet(&c);
    past_hour(&mut s);
    let id = plant(&mut s, "shambler", "top:9,15");
    let cell = c.map.cell_name(nearest_build_cells(&c, "top:9,15", 1)[0]);
    place(&mut s, "rune_pillar", &cell, 0);
    let mut slowed = false;
    for _ in 0..5 * TICK_HZ {
        steps(&mut s, 1);
        slowed |= s.enemy(id).is_some_and(|e| e.is_slowed(s.tick()));
    }
    assert!(slowed, "the rune pillar slows it past its wave's hour");
    // Nature's roots.
    let mut s = quiet(&c);
    past_hour(&mut s);
    let id = plant(&mut s, "shambler", "top:9,15");
    let e = *s.enemy(id).unwrap();
    s.w.rooted[e.cell().unwrap().index()] = true;
    assert!(c.rules.nature.slow > 0);
    assert_eq!(s.w.speed_permille(&c, &e, false), i64::from(1000 - c.rules.nature.slow), "the roots slow it past its wave's hour");
    // The colossus's summons.
    let mut s = quiet(&c);
    past_hour(&mut s);
    let big = plant(&mut s, "colossus", "top:9,13");
    let i = idx(&s, big);
    s.w.enemies[i].next_summon = s.w.tick;
    let ev = steps(&mut s, 1);
    assert_eq!(ev.iter().filter(|x| matches!(x, Event::Spawned { .. })).count(), 2, "the colossus calls past its wave's hour: {ev:?}");
}

/// A line heave asks about each enemy it reaches, not about its target alone: of two on its
/// line, one in furor and one whose wave has not begun (never in furor), the one in furor is hurt
/// and not staggered, and the other hurt and staggered, whichever of the two it aimed at.
#[test]
fn a_line_heave_asks_about_each_enemy_it_reaches() {
    let c = saxum_iii("{ \"cost\": 110, \"range_m\": 2.5, \"cooldown_s\": 1.0, \"damage\": 1, \"line\": 3, \"stagger_s\": 1.2 }");
    let mut s = quiet(&c);
    let hour = s.w.hour_of(&c, 1).unwrap();
    s.w.tick = hour + 1;
    let fresh = plant(&mut s, "shambler", "top:9,18");
    let i = idx(&s, fresh);
    s.w.enemies[i].wave = 2;
    let raging = plant(&mut s, "shambler", "top:9,18");
    let stone = c.map.cell_name(nearest_build_cells(&c, "top:9,18", 1)[0]);
    place(&mut s, "saxum", &stone, 2);
    let ev = steps(&mut s, 1);
    assert!(ev.iter().any(|e| matches!(e, Event::Heaved { affected: 2, .. })), "one heave reached both: {ev:?}");
    let (f, r) = (*s.enemy(fresh).unwrap(), *s.enemy(raging).unwrap());
    assert!(!s.w.in_furor(&c, &f) && s.w.in_furor(&c, &r));
    assert!(r.hp < r.max_hp && r.el.stagger_until == 0, "in furor: hurt, not staggered: {r:?}");
    assert!(f.hp < f.max_hp && f.el.stagger_until > s.tick(), "not in furor: hurt and staggered: {f:?}");
}

/// A save's spent cells are range-checked: a bar at the next id loads (and steps alike), a bar
/// above it, against enemies not yet born, is refused as a state that does not fit.
#[test]
fn a_save_refuses_a_spent_cell_barred_against_enemies_not_yet_born() {
    let c = proving();
    let mut s = quiet(&c);
    plant(&mut s, "shambler", GORGE);
    let cell = at(&c, "top:9,17");
    let next = s.w.next_enemy;
    s.w.spent.insert(cell, next);
    let loaded = State::from_bytes(Arc::clone(&c), &s.to_bytes()).expect("a bar at the next id loads");
    assert_eq!(loaded.hash(), s.hash());
    s.w.spent.insert(cell, next + 1);
    let e = State::from_bytes(Arc::clone(&c), &s.to_bytes()).unwrap_err();
    assert_eq!(e.code, "bad_state", "{e:?}");
    assert!(e.message.contains("spent cell") && e.offset.is_some(), "{e:?}");
}

/// No rubble rises while an enemy in furor stands in the lane, though the heave's own target is
/// of a wave not yet begun (so not in furor); the control, with nobody in furor, raises it.
#[test]
fn no_rubble_rises_while_an_enemy_in_furor_stands() {
    let c = saxum_iii("{ \"cost\": 110, \"range_m\": 2.5, \"cooldown_s\": 1.0, \"line\": 4, \"rubble\": { \"hp\": 2000, \"s\": 60 } }");
    let run = |furor: bool| {
        let mut s = quiet(&c);
        let hour = s.w.hour_of(&c, 1).unwrap();
        s.w.tick = if furor { hour + 1 } else { hour - 100 };
        let far = plant(&mut s, "shambler", GORGE);
        let near = plant(&mut s, "shambler", "top:9,18");
        let i = idx(&s, near);
        s.w.enemies[i].wave = 2;
        s.w.enemies[i].el.stagger_until = u32::MAX;
        let stone = c.map.cell_name(nearest_build_cells(&c, "top:9,18", 1)[0]);
        place(&mut s, "saxum", &stone, 2);
        let ev = steps(&mut s, 1);
        assert_eq!(s.w.in_furor(&c, s.enemy(far).unwrap()), furor);
        assert!(!s.w.in_furor(&c, s.enemy(near).unwrap()));
        assert!(ev.iter().any(|e| matches!(e, Event::Heaved { affected: 1, .. })), "it heaved at the near one only: {ev:?}");
        ev.iter().any(|e| matches!(e, Event::RubbleRaised { .. }))
    };
    assert!(run(false), "the control raises rubble");
    assert!(!run(true), "none rises while the far one is in furor");
}

/// A save holds rubble that rose at its own tick or before, and refuses rubble said to rise
/// after it: at the save's tick, one before, one after.
#[test]
fn a_save_takes_rubble_risen_at_its_tick_and_refuses_one_after() {
    let c = saxum_iii(REVIEW_RUBBLE);
    let save_with = |raised: u32| {
        let mut s = quiet(&c);
        s.w.tick = 500;
        let cell = at(&c, "top:9,17");
        s.w.walls.insert(cell, Wall { hp: 2_000_000, max_hp: 2_000_000, cost: 0 });
        s.w.rubble.insert(cell, (raised, raised + 100));
        s.w.set_blocked(&c, cell, true);
        State::from_bytes(Arc::clone(&c), &s.to_bytes()).map(|l| l.hash() == s.hash())
    };
    assert_eq!(save_with(499).ok(), Some(true));
    assert_eq!(save_with(500).ok(), Some(true), "risen at the save's tick");
    assert!(save_with(501).is_err(), "risen after the save's tick");
}

/// Furor is decided by ticks alone: a save in furor loads in furor and steps exactly like the
/// game it was taken from.
#[test]
fn a_save_in_furor_loads_in_furor_and_steps_alike() {
    let c = proving();
    let (mut s, id, _) = held_at_the_hour(&c);
    steps(&mut s, 3);
    let mut loaded = State::from_bytes(Arc::clone(&c), &s.to_bytes()).expect("loads");
    assert!(loaded.w.in_furor(&c, loaded.enemy(id).unwrap()));
    for _ in 0..200 {
        let (a, b) = (s.step(&[]), loaded.step(&[]));
        assert_eq!((a, s.hash()), (b, loaded.hash()));
    }
}

// ---- A run's length (DESIGN.md, "Furor") ----

/// The run's stated length for the proving ground, term by term from its data: built for 20 s,
/// waves of 50, 55, 60, 55 and 90 s before the last, whose last enemy enters 24 s in, and twenty
/// minutes after that; then the longest walk and the runner's leaps.
#[test]
fn a_runs_length_is_the_sum_design_states() {
    let c = proving();
    let b = c.run_bound();
    assert_eq!(b.hour, u64::from((20 + 50 + 55 + 60 + 55 + 90 + 24) * TICK_HZ + crate::content::WAVE_HOUR_TICKS), "{b:?}");
    assert_eq!(b.ticks, b.hour + b.walk + b.leaps + 1);
    // The walk is the longest way on the open slab (no walls), and the leaps the runner's.
    assert_eq!((b.walk, b.leaps, b.ticks), (2_757, 440, 49_818), "{b:?}");
    let shipped = Content::builtin();
    let s = shipped.run_bound();
    assert_eq!(s.ticks, s.hour + s.walk + s.leaps + 1);
}

/// A run that cannot end (the player's own walls, of the most HP the format takes, close every
/// gate before the dead) stops at exactly the run's stated length, no sooner and no later.
#[test]
fn a_run_that_cannot_end_stops_at_its_stated_length() {
    let (c, script) = sealed_shipped();
    let r = crate::script::run(Arc::clone(&c), 1, &script, None, |_, _| {});
    assert_eq!((r.outcome, u64::from(r.ticks)), (Outcome::Running, c.run_bound().ticks), "{:?}", r.waves);
    assert!(r.waves[0].alive > 0, "the dead still at the walls");
}

// ---- The property that closes the class (DESIGN.md, "Furor"): every run ends inside its stated length ----

/// The proving ground's stones in file order, and the attack each one's tiers are for.
const PROP_STONES: [(&str, Attack); 9] = [
    ("warding", Attack::Bolt),
    ("rune_pillar", Attack::Pulse),
    ("brazier", Attack::Lob),
    ("sky_obelisk", Attack::Bolt),
    ("rogus", Attack::Lob),
    ("fulmen", Attack::Strike),
    ("gelu", Attack::Bolt),
    ("ventus", Attack::Gust),
    ("saxum", Attack::Heave),
];

/// A uniform draw in `lo..=hi`.
fn draw(r: &mut Rng, lo: i64, hi: i64) -> i64 {
    lo + (r.next_u64() % (hi - lo + 1) as u64) as i64
}

/// A legal value in `lo..=hi` (thousandths of the file's unit), weighted on the extremes: the
/// least or the most a fifth of the time each, the least's near neighbour a tenth, otherwise
/// anywhere between.
fn wild(r: &mut Rng, lo: i64, hi: i64) -> i64 {
    match r.below(10) {
        0 | 1 => lo,
        2 | 3 => hi,
        4 => (lo + 1).min(hi),
        _ => draw(r, lo, hi),
    }
}

/// Thousandths as the decimal a file writes.
fn th(v: i64) -> String {
    format!("{}.{:03}", v / 1000, v % 1000)
}

/// One random legal tier of `attack`, over every field it reads, each across its whole legal
/// range (content.rs: `parse_stones`, `parse_tier_el`, `check_tier`).
fn wild_tier(r: &mut Rng, attack: Attack) -> String {
    let mut k: Vec<String> = vec!["\"cost\": 1".into(), format!("\"range_m\": {}", th(wild(r, 500, 100_000)))];
    let some = |r: &mut Rng| r.below(3) != 0;
    match attack {
        Attack::Bolt | Attack::Lob | Attack::Pulse => {
            k.push(format!("\"cooldown_s\": {}", th(wild(r, 34, 600_000))));
            let damage = wild(r, 0, 100_000_000);
            let (slow, slow_s) = if some(r) { (wild(r, 1, 900), wild(r, 0, 600_000)) } else { (0, 0) };
            let (mut chill, mut burn) = (0, 0);
            if attack != Attack::Pulse {
                k.push(format!("\"projectile_mps\": {}", th(wild(r, 34, 1_000_000))));
                if attack == Attack::Lob {
                    k.push(format!("\"splash_m\": {}", th(wild(r, 1, 100_000))));
                }
                if some(r) {
                    chill = wild(r, 1, 1000);
                    k.push(format!("\"chill\": {}, \"freeze_s\": {}, \"shatter\": {}", th(chill), th(wild(r, 34, 30_000)), th(wild(r, 0, 5_000))));
                }
                if some(r) {
                    burn = wild(r, 30, 10_000_000);
                    k.push(format!("\"burn_dps\": {}, \"burn_s\": {}", th(burn), th(wild(r, 34, 60_000))));
                }
            }
            // A tier must deal damage, slow, chill or burn.
            let damage = if damage == 0 && slow == 0 && chill == 0 && burn == 0 { 1 } else { damage };
            k.push(format!("\"damage\": {}", th(damage)));
            if slow > 0 {
                k.push(format!("\"slow\": {}, \"slow_s\": {}", th(slow), th(slow_s)));
            }
        }
        Attack::Strike => {
            k.push(format!("\"cooldown_s\": {}, \"damage\": {}", th(wild(r, 334, 600_000)), th(wild(r, 1, 100_000_000))));
            let chain = wild(r, 0, 8);
            k.push(format!("\"chain\": {chain}, \"chain_keep\": {}, \"wet_bonus\": {}", th(wild(r, 0, 1000)), th(wild(r, 0, 5000))));
            if chain > 0 {
                k.push(format!("\"chain_m\": {}", th(wild(r, 1, 20_000))));
            }
        }
        Attack::Gust => {
            k.push(format!("\"cooldown_s\": {}, \"push_m\": {}, \"push_s\": {}", th(wild(r, 34, 600_000)), th(wild(r, 1, 30_000)), th(wild(r, 34, 5_000))));
        }
        Attack::Heave => {
            k.push(format!("\"cooldown_s\": {}, \"damage\": {}, \"line\": {}", th(wild(r, 34, 600_000)), th(wild(r, 0, 100_000_000)), wild(r, 1, 8)));
            if some(r) {
                k.push(format!("\"stagger_s\": {}", th(wild(r, 0, 10_000))));
            }
            if some(r) {
                k.push(format!("\"rubble\": {{ \"hp\": {}, \"s\": {} }}", th(wild(r, 1_000, 100_000_000)), th(wild(r, 100, 120_000))));
            }
        }
    }
    format!("{{ {} }}", k.join(", "))
}

/// The proving ground's `stones.json` with every stone's three tiers drawn by [`wild_tier`] and a
/// drawn `shove_fade` (0.05 to 1).
fn wild_stones(r: &mut Rng) -> String {
    let mut text = PROVING_STONES_JSON.replace("\r\n", "\n");
    let fade = "\"shove_fade\": 0.25,";
    assert_eq!(text.matches(fade).count(), 1, "fixture");
    text = text.replacen(fade, &format!("\"shove_fade\": {},", th(wild(r, 50, 1000))), 1);
    let mut out = String::new();
    let mut rest = text.as_str();
    for (_, attack) in PROP_STONES {
        let at = rest.find("\"tiers\": [").expect("fixture: a stone's tiers");
        let end = at + rest[at..].find("\n      ]").expect("fixture: the tiers' end");
        let tiers: Vec<String> = (0..3).map(|_| wild_tier(r, attack)).collect();
        out.push_str(&rest[..at]);
        out.push_str(&format!("\"tiers\": [\n        {}", tiers.join(",\n        ")));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Waves of one: a lone `kind`, or (`None`) the proving ground's last wave, everything at once.
fn prop_waves(kind: Option<&str>) -> String {
    let groups = match kind {
        Some(k) => format!("{{ \"enemy\": \"{k}\", \"count\": 1, \"from\": \"{}\" }}", match k { "wraith" => "air", "shambler" => "ground", _ => "gorges" }),
        None => "{ \"enemy\": \"shambler\", \"count\": 12, \"from\": \"ground\", \"start_s\": 0, \"every_s\": 1.5 }, { \"enemy\": \"tumidus\", \"count\": 6, \"from\": \"gorges\", \"start_s\": 5, \"every_s\": 2.5 }, { \"enemy\": \"runner\", \"count\": 12, \"from\": \"gorges\", \"start_s\": 10, \"every_s\": 1.0 }, { \"enemy\": \"wraith\", \"count\": 4, \"from\": \"air\", \"start_s\": 15, \"every_s\": 3 }, { \"enemy\": \"colossus\", \"count\": 1, \"from\": \"gorge_mid\", \"start_s\": 0 }".to_owned(),
    };
    format!("{{ \"start_dust\": 1000000, \"tombstones\": 1000, \"build_time_s\": 0, \"waves\": [ {{ \"duration_s\": 100, \"hp\": 1.5, \"groups\": [ {groups} ] }} ] }}")
}

/// Lane cells the drawn layouts gather round: the middle lane, both gorges, the south lane, the
/// north-west bend.
const PROP_ANCHORS: [&str; 5] = [AMID, GORGE, "top:15,19", "top:9,16", "top:6,9"];

/// A drawn layout: up to twelve stones of drawn kinds and tiers (mostly III) round one anchor,
/// as many as twelve along one lane a third of the time.
fn wild_layout(r: &mut Rng, c: &Content) -> Vec<(StoneKind, CellId, u8)> {
    let anchor = PROP_ANCHORS[r.below(PROP_ANCHORS.len() as u32) as usize];
    let mut cells = nearest_build_cells(c, anchor, 24);
    let n = if r.below(3) == 0 { 12 } else { 1 + r.below(12) as usize };
    let mut out = Vec::new();
    for _ in 0..n {
        let cell = cells.remove(r.below(cells.len() as u32) as usize);
        let kind = StoneKind(r.below(PROP_STONES.len() as u32) as u8);
        let tier = [0u8, 1, 2, 2, 2][r.below(5) as usize];
        out.push((kind, cell, tier));
    }
    out
}

/// What one run came to, against the length the rules state for it.
#[derive(Debug, Clone, Copy, Default)]
struct PropRun {
    ticks: u64,
    figure: u64,
    furor: u32,
}

/// Plays one run of `c` with `stones` set down at its first tick: the lone enemy its waves bring,
/// or the whole wave; or (`planted`) a lone `kind` set down on a cell, as the reviews' probes did.
/// Panics, naming the run, unless it ends (won: nobody left standing) inside its stated length.
fn prop_run(c: &Arc<Content>, stones: &[(StoneKind, CellId, u8)], planted: Option<(&str, &str)>, seed: u64, what: &dyn Fn() -> String) -> PropRun {
    let mut s = match planted {
        Some(_) => quiet(c),
        None => State::new(Arc::clone(c), seed),
    };
    let lone = planted.map(|(kind, cell)| plant(&mut s, kind, cell));
    let b = c.run_bound();
    let figure = match planted {
        Some(_) => u64::from(s.w.hour_of(c, 1).unwrap()) + b.walk + b.leaps + 1,
        None => b.ticks,
    };
    s.w.dust = 100_000_000;
    let mut ev = Vec::new();
    for &(stone, cell, tier) in stones {
        if s.apply(&Command::PlaceStone { cell, stone }, &mut ev).is_ok() {
            for _ in 0..tier {
                let _ = s.apply(&Command::Upgrade { cell }, &mut ev);
            }
        }
    }
    let mut r = PropRun { figure, ..PropRun::default() };
    let ended = |s: &State| match lone {
        Some(id) => s.enemy(id).is_none(),
        None => s.outcome() != Outcome::Running,
    };
    while !ended(&s) && r.ticks <= figure {
        ev.clear();
        s.step_into(&[], &mut ev);
        r.ticks += 1;
        r.furor += ev.iter().filter(|e| matches!(e, Event::Furor { .. })).count() as u32;
    }
    let standing = s.enemies().len();
    assert!(
        ended(&s) && r.ticks <= figure && (lone.is_some() || (s.outcome() == Outcome::Won && standing == 0)),
        "a run did not end inside its stated length: {r:?}, {standing} standing, outcome {:?}, {:?}\n{}",
        s.outcome(),
        b,
        what()
    );
    r
}

/// The reviews' own extremes, as content (the proving ground with these tiers put in), where
/// their stones stood, and the lone enemy and its cell.
/// One of the reviews' extremes: its name, the stones.json text put in (what, by what), its
/// stones (kind, cells, tier), and the lone enemy's kind and cell.
type Extreme = (&'static str, Vec<(&'static str, String)>, Vec<(&'static str, Vec<CellId>, u8)>, &'static str, &'static str);

fn review_extremes() -> Vec<Extreme> {
    let c = proving();
    let ring: Vec<CellId> = RING.iter().map(|x| at(&c, x)).collect();
    let amid = nearest_build_cells(&c, AMID, 1);
    let saxum3 = "{ \"cost\": 110, \"range_m\": 7.0, \"cooldown_s\": 2.6, \"damage\": 36, \"line\": 4, \"stagger_s\": 1.2, \"rubble\": { \"hp\": 200, \"s\": 8 } }";
    let pillar3 = "{ \"cost\": 90, \"range_m\": 6.0, \"cooldown_s\": 1.0, \"damage\": 6, \"slow\": 0.55, \"slow_s\": 1.5 }";
    let ventus3 = "{ \"cost\": 90, \"range_m\": 6.5, \"cooldown_s\": 4.0, \"push_m\": 5.0, \"push_s\": 0.6 }";
    let fade = "\"shove_fade\": 0.25,";
    let free: Vec<CellId> = nearest_build_cells(&c, AMID, 64).into_iter().filter(|x| !ring.contains(x)).take(1).chain(nearest_build_cells(&c, GORGE, 64).into_iter().filter(|x| !ring.contains(x)).take(1)).collect();
    vec![
        // Review 3, r3_a: a push of 30 m over 5 s at the slowest fade, four cairns.
        ("r3_a wind", vec![(ventus3, slow_gust(30.0)), (fade, "\"shove_fade\": 0.05,".into())], vec![("ventus", ring.clone(), 2)], "shambler", GORGE),
        // Review 3, r3_e: frost with no damage, chill 1 and a 30 s freeze, every tick.
        ("r3_e frost", vec![(GELU_III, "{ \"cost\": 100, \"range_m\": 7.5, \"cooldown_s\": 0.034, \"projectile_mps\": 1000, \"chill\": 1.0, \"freeze_s\": 30 }".into())], vec![("gelu", ring.clone(), 2)], "shambler", GORGE),
        // Review 4, p5 (r4_a): a heave of 0.01 that staggers 10 s every 9.9 s, and 0.001 every tick.
        ("p5 stagger 9.9 s", vec![(saxum3, "{ \"cost\": 110, \"range_m\": 7.0, \"cooldown_s\": 9.9, \"damage\": 0.01, \"line\": 4, \"stagger_s\": 10 }".into())], vec![("saxum", amid.clone(), 2)], "shambler", AMID),
        ("p5 stagger every tick", vec![(saxum3, "{ \"cost\": 110, \"range_m\": 7.0, \"cooldown_s\": 0.034, \"damage\": 0.001, \"line\": 4, \"stagger_s\": 10 }".into())], vec![("saxum", amid.clone(), 2)], "shambler", AMID),
        // Review 4, p6 and p7 (r4_f, r4_i): rubble of 2,000 HP for 60 s every second, and of
        // 100,000 HP for 120 s every tick on a line of 8.
        ("p6 rubble 2000", vec![(saxum3, REVIEW_RUBBLE.into())], vec![("saxum", amid.clone(), 2)], "shambler", GORGE),
        ("p6 rubble 100000", vec![(saxum3, "{ \"cost\": 110, \"range_m\": 7.0, \"cooldown_s\": 0.034, \"line\": 8, \"rubble\": { \"hp\": 100000, \"s\": 120 } }".into())], vec![("saxum", amid.clone(), 2)], "runner", GORGE),
        // Review 4, p5 (r4_b): a slow of 0.9 for 600 s under the wind at the old edge of refusal.
        ("p5 slow under wind", vec![(pillar3, "{ \"cost\": 90, \"range_m\": 6.0, \"cooldown_s\": 1.0, \"slow\": 0.9, \"slow_s\": 600 }".into()), (ventus3, slow_gust(14.1)), (fade, "\"shove_fade\": 0.05,".into())], vec![("ventus", ring, 2), ("rune_pillar", free, 2)], "shambler", GORGE),
    ]
}

/// The property, over `cases` drawn contents from `seed`: for each, a lone enemy of each kind and
/// the whole last wave; then the reviews' extremes. Returns the run that came nearest its figure.
fn the_property(cases: u64, seed: u64) -> (PropRun, String, u32, u32) {
    let mut nearest = (PropRun::default(), String::new());
    let (mut runs, mut furious) = (0u32, 0u32);
    let mut note = |r: PropRun, what: String| {
        runs += 1;
        furious += u32::from(r.furor > 0);
        if r.ticks * nearest.0.figure.max(1) > nearest.0.ticks * r.figure {
            nearest = (r, what);
        }
    };
    for case in 0..cases {
        let mut rng = Rng::new(seed, case);
        let stones = wild_stones(&mut rng);
        for kind in [Some("shambler"), Some("runner"), Some("tumidus"), Some("colossus"), Some("wraith"), None] {
            let waves = prop_waves(kind);
            let c = match Content::from_texts(SLAB_01_JSON, &stones, PROVING_ENEMIES_JSON, &waves) {
                Ok(c) => Arc::new(c),
                Err(e) => panic!("the generator drew content the loader refuses ({e}): case {case} of seed {seed}\n{stones}"),
            };
            let layout = wild_layout(&mut rng, &c);
            let what = || format!("case {case} of seed {seed}, {kind:?}, layout {:?}\nstones.json:\n{stones}", layout.iter().map(|&(k, cell, t)| (c.stone(k).id.clone(), c.map.cell_name(cell), t + 1)).collect::<Vec<_>>());
            let r = prop_run(&c, &layout, None, case, &what);
            note(r, what());
        }
    }
    for (name, subs, stones, kind, from) in review_extremes() {
        let mut text = PROVING_STONES_JSON.replace("\r\n", "\n");
        for (a, b) in &subs {
            assert_eq!(text.matches(a).count(), 1, "fixture {name}: {a}");
            text = text.replacen(a, b, 1);
        }
        let c = Arc::new(Content::from_texts(SLAB_01_JSON, &text, PROVING_ENEMIES_JSON, PROVING_WAVES_JSON).unwrap_or_else(|e| panic!("{name}: the format takes it now: {e}")));
        let layout: Vec<(StoneKind, CellId, u8)> = stones.iter().flat_map(|(k, cells, t)| cells.iter().map(|&cell| (c.stone_kind(k).unwrap(), cell, *t))).collect();
        let what = || format!("the reviews' {name}");
        let r = prop_run(&c, &layout, Some((kind, from)), 1, &what);
        note(r, what());
    }
    (nearest.0, nearest.1, runs, furious)
}

/// **Every run ends inside the length the rules state**, over random legal content drawn from
/// the whole format (every stone, every tier field across its legal range, weighted on the
/// extremes: no damage, the longest freeze and stagger, the slowest recharge and the fastest,
/// the largest push, slows of 0.9 for 600 s, rubble of any health and life, many stones of mixed
/// kinds along one lane), a lone enemy of each kind and a whole wave on each, and the reviews'
/// extremes. A stone that holds in a way furor does not end fails here. 8 contents (48 runs) and
/// the reviews' 7, a few seconds in a debug build (the extremes are the runs that reach furor);
/// the release run below draws a hundred times as many contents.
#[test]
fn every_run_ends_inside_its_stated_length() {
    let (nearest, what, runs, furious) = the_property(8, 0x5e9_f0e0);
    let near = what.lines().next().unwrap_or("");
    assert!(nearest.ticks <= nearest.figure, "{runs} runs; the nearest passed its figure: {nearest:?} ({near})");
    assert!(furious > 0, "some drawn content held a run to its wave's hour: the property met furor ({runs} runs, {furious} reached furor; nearest {nearest:?}, {near})");
}

/// The same property over a hundred times as many contents: `cargo test -p sepulcra --release
/// --lib -- --ignored every_run_ends_inside_its_stated_length_at_length`.
#[test]
#[ignore = "800 drawn contents: run by hand, in release"]
fn every_run_ends_inside_its_stated_length_at_length() {
    let (nearest, what, runs, furious) = the_property(800, 0x5e9_f0e1);
    let near = what.lines().next().unwrap_or("");
    assert!(nearest.ticks <= nearest.figure, "{runs} runs, {furious} reached furor; the nearest passed its figure: {nearest:?} ({near})");
}

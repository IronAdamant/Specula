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
fn a_shove_reaches_nobody_after_its_last() {
    let c = with_fuel("");
    let mut s = quiet(&c);
    let id = plant(&mut s, "shambler", "top:9,15");
    let i = idx(&s, id);
    s.w.enemies[i].el.shoves = MAX_SHOVES - 1;
    // A push far past what content allows (30 km), so the fade alone does not bring it to 0.
    s.w.shove(&c, i, 30_000_000, 15);
    assert!(s.w.enemies[i].el.shove > 0, "the last shove still moves it");
    s.w.enemies[i].el.shove = 0;
    s.w.shove(&c, i, 30_000_000, 15);
    assert_eq!((s.w.enemies[i].el.shove, s.w.enemies[i].el.shoves), (0, MAX_SHOVES), "after it, nothing");
}

#[test]
fn content_without_a_shove_fade_gets_one_and_the_wind_layouts_end() {
    let stones = PROVING_STONES_JSON.replace("\r\n", "\n");
    let without = stones.replace("    \"shove_fade\": 0.5,\n", "");
    assert_ne!(without, stones, "test fixture");
    let c = Arc::new(Content::from_texts(SLAB_01_JSON, &without, PROVING_ENEMIES_JSON, PROVING_WAVES_JSON).expect("content"));
    assert_eq!(c.rules.elements.shove_fade, 500, "the default is the proving ground's 0.5");
    let zero = stones.replace("\"shove_fade\": 0.5", "\"shove_fade\": 0");
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
    let fade = c.rules.elements.shove_fade;
    assert!(fade > 0);
    let mut got = Vec::new();
    for _ in 0..3 {
        s.w.enemies[i].el.shove = 0;
        s.w.shove(&c, i, 3_000, 15);
        got.push(s.w.enemies[i].el.shove);
    }
    assert_eq!(got, vec![3_000, 3_000_000 / (1000 + fade), 3_000_000 / (1000 + 2 * fade)]);
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

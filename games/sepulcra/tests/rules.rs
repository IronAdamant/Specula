//! Game rules, tested through the public API with small custom waves.

use std::sync::Arc;

use sepulcra::content::{Content, ENEMIES_JSON, SLAB_01_JSON, STONES_JSON};
use sepulcra::sim::{DustSource, Event, Motion, Outcome, Rejection, State};
use sepulcra::{CellId, Command, Face};

/// The default slab, stones and enemies, with one custom wave after a 1 s build time.
fn one_wave(groups: &str, enemies: &str) -> Arc<Content> {
    let waves = format!(r#"{{"start_dust": 2000, "tombstones": 10, "build_time_s": 1, "waves": [{{"duration_s": 600, "groups": [{groups}]}}]}}"#);
    Arc::new(Content::from_texts(SLAB_01_JSON, STONES_JSON, enemies, &waves).expect("test content"))
}

fn cell(c: &Content, s: &str) -> CellId {
    c.map.cell_ref(s).unwrap_or_else(|| panic!("no cell {s}"))
}

fn apply(state: &mut State, cmd: Command) {
    let mut ev = Vec::new();
    state.apply(&cmd, &mut ev).unwrap_or_else(|r| panic!("{cmd:?} refused: {r}"));
}

/// Steps without commands until `stop` returns true or `max` ticks pass. Returns all events.
fn run_for(state: &mut State, max: u32, mut stop: impl FnMut(&State, &[Event]) -> bool) -> Vec<Event> {
    let mut all = Vec::new();
    for _ in 0..max {
        let ev = state.step(&[]);
        let done = stop(state, &ev);
        all.extend(ev);
        if done || state.outcome() != Outcome::Running {
            break;
        }
    }
    all
}

fn leaked(ev: &[Event]) -> usize {
    ev.iter().filter(|e| matches!(e, Event::TombstonesLost { .. })).count()
}

fn no_enemy_on_a_wall(s: &State) {
    for e in s.enemies() {
        if let Some(c) = e.cell() {
            assert!(s.wall(c).is_none(), "enemy {} stands on a wall at {}", e.id, s.content().map.cell_name(c));
        }
    }
}

const GATES: [&str; 4] = ["top:10,7", "top:12,10", "top:9,12", "top:7,9"];

#[test]
fn an_open_route_is_always_taken_however_long() {
    let c = one_wave(r#"{"enemy": "runner", "count": 1, "from": "gorge_east"}"#, ENEMIES_JSON);
    let mut base = State::new(c.clone(), 1);
    run_for(&mut base, 3000, |_, ev| leaked(ev) > 0);
    let direct = base.tick();

    let mut s = State::new(c.clone(), 1);
    for g in &GATES[..3] {
        apply(&mut s, Command::PlaceWall { cell: cell(&c, g) });
    }
    let ev = run_for(&mut s, 3000, |s, ev| {
        no_enemy_on_a_wall(s);
        leaked(ev) > 0
    });
    assert_eq!(leaked(&ev), 1, "the runner walked round to the open west gate");
    assert!(!ev.iter().any(|e| matches!(e, Event::AttackingWall { .. })), "no wall is attacked while a route is open");
    assert!(s.walls().all(|(_, w)| w.hp == w.max_hp));
    assert!(s.tick() > direct + 60, "the long way round: {} vs {direct} ticks", s.tick());
}

#[test]
fn a_wall_placed_mid_run_reroutes_and_cannot_land_on_an_enemy() {
    let c = one_wave(r#"{"enemy": "runner", "count": 1, "from": "gorge_mid"}"#, ENEMIES_JSON);
    let mut s = State::new(c.clone(), 1);
    let at = |s: &State| s.enemies().first().and_then(|e| e.cell());
    run_for(&mut s, 600, |s, _| at(s) == Some(cell(&c, "top:9,17")));
    let here = at(&s).expect("runner on the connector");
    let mut ev = Vec::new();
    let id = s.enemies()[0].id;
    assert_eq!(s.apply(&Command::PlaceWall { cell: here }, &mut ev), Err(Rejection::EnemyInTheWay { enemy: id }));
    apply(&mut s, Command::PlaceWall { cell: cell(&c, "top:9,14") });
    let wall = cell(&c, "top:9,14");
    let ev = run_for(&mut s, 3000, |s, ev| {
        no_enemy_on_a_wall(s);
        for e in s.enemies() {
            if let Motion::Ground { to, .. } = e.motion {
                assert_ne!(to, wall, "walked into the new wall");
            }
        }
        leaked(ev) > 0
    });
    assert_eq!(leaked(&ev), 1);
    assert!(!ev.iter().any(|e| matches!(e, Event::AttackingWall { .. })));
}

#[test]
fn a_full_block_makes_enemies_break_the_wall_on_the_shortest_route() {
    let c = one_wave(r#"{"enemy": "shambler", "count": 1, "from": "gorge_mid"}"#, ENEMIES_JSON);
    let mut s = State::new(c.clone(), 1);
    for g in GATES {
        apply(&mut s, Command::PlaceWall { cell: cell(&c, g) });
    }
    let south = cell(&c, "top:9,12");
    let ev = run_for(&mut s, 5000, |s, ev| {
        no_enemy_on_a_wall(s);
        leaked(ev) > 0
    });
    let attacked: Vec<CellId> = ev.iter().filter_map(|e| if let Event::AttackingWall { wall, .. } = e { Some(*wall) } else { None }).collect();
    assert_eq!(attacked, vec![south], "the central lane leads straight to the south gate");
    assert!(ev.contains(&Event::WallBroken { cell: south }));
    let marks: Vec<i32> = ev
        .iter()
        .filter_map(|e| if let Event::WallDamaged { cell, hp, max_hp } = e { (*cell == south).then_some(hp * 100 / max_hp) } else { None })
        .collect();
    assert_eq!(marks.len(), 3, "reported at 75, 50 and 25 %: {marks:?}");
    assert!(marks[0] <= 75 && marks[0] > 70 && marks[1] <= 50 && marks[2] <= 25, "{marks:?}");
    assert_eq!(leaked(&ev), 1, "through the breach");
    assert_eq!(s.walls().count(), 3, "the other walls stand untouched");
    assert!(s.walls().all(|(_, w)| w.hp == w.max_hp));
}

#[test]
fn opening_a_route_stops_the_attack() {
    let c = one_wave(r#"{"enemy": "shambler", "count": 1, "from": "gorge_mid"}"#, ENEMIES_JSON);
    let mut s = State::new(c.clone(), 1);
    for g in GATES {
        apply(&mut s, Command::PlaceWall { cell: cell(&c, g) });
    }
    run_for(&mut s, 3000, |_, ev| ev.iter().any(|e| matches!(e, Event::AttackingWall { .. })));
    let south = cell(&c, "top:9,12");
    assert_eq!(s.enemies()[0].attacking, Some(south));
    apply(&mut s, Command::RemoveWall { cell: cell(&c, "top:10,7") });
    s.step(&[]);
    let hp = s.wall(south).unwrap().hp;
    assert_eq!(s.enemies()[0].attacking, None, "an open route exists again");
    let ev = run_for(&mut s, 5000, |_, ev| leaked(ev) > 0);
    assert_eq!(s.wall(south).unwrap().hp, hp, "no more damage to the south gate");
    assert_eq!(leaked(&ev), 1, "round to the reopened north gate");
}

#[test]
fn runners_leap_cracked_ledges_by_seeded_chance() {
    let sure = ENEMIES_JSON.replace("\"chance\": 0.4", "\"chance\": 1.0");
    let never = ENEMIES_JSON.replace("\"chance\": 0.4", "\"chance\": 0.0");
    let jumps = |enemies: &str, count: u32, seed: u64| {
        let c = one_wave(&format!(r#"{{"enemy": "runner", "count": {count}, "from": "gorge_west", "every_s": 0.5}}"#), enemies);
        let mut s = State::new(c.clone(), seed);
        // Close the connector east of the west gorge, so runners climb the west lane.
        apply(&mut s, Command::PlaceWall { cell: cell(&c, "top:5,17") });
        let ev = run_for(&mut s, 5000, |s, _| s.wave_stats()[0].cleared_at.is_some());
        let leaps: Vec<(CellId, CellId)> = ev.iter().filter_map(|e| if let Event::Jumped { from, to, .. } = e { Some((*from, *to)) } else { None }).collect();
        (c, leaps)
    };
    let (c, leaps) = jumps(&sure, 1, 1);
    assert_eq!(leaps, vec![(cell(&c, "top:4,13"), cell(&c, "top:4,11"))]);
    assert!(jumps(&never, 5, 1).1.is_empty());
    let n = jumps(ENEMIES_JSON, 30, 7).1.len();
    assert!((5..=20).contains(&n), "about 40% of 30 runners jump: {n}");
    assert_eq!(n, jumps(ENEMIES_JSON, 30, 7).1.len(), "same seed, same leaps");
}

/// Review finding: a runner sealed in a lane whose only way out is a cracked ledge has an open
/// route (the leap), so the owner's rule says it must take it, on every seed, without
/// attacking a wall. Walkers that can't leap must break a wall instead.
#[test]
fn a_leap_that_is_the_only_open_route_is_always_taken() {
    let sealed = |enemies: &str, seed: u64| {
        let c = one_wave(r#"{"enemy": "runner", "count": 1, "from": "gorge_west"}"#, enemies);
        let mut s = State::new(c.clone(), seed);
        apply(&mut s, Command::PlaceWall { cell: cell(&c, "top:5,17") });
        apply(&mut s, Command::PlaceWall { cell: cell(&c, "top:5,13") });
        let ev = run_for(&mut s, 5000, |s, ev| {
            no_enemy_on_a_wall(s);
            leaked(ev) > 0
        });
        (c, s, ev)
    };
    for seed in 1..=50 {
        let (c, s, ev) = sealed(ENEMIES_JSON, seed);
        let leaps: Vec<&Event> = ev.iter().filter(|e| matches!(e, Event::Jumped { .. })).collect();
        assert_eq!(leaps.len(), 1, "seed {seed}");
        assert!(matches!(leaps[0], Event::Jumped { from, to, .. } if *from == cell(&c, "top:4,13") && *to == cell(&c, "top:4,11")));
        assert!(!ev.iter().any(|e| matches!(e, Event::AttackingWall { .. })), "seed {seed}: attacked a wall with a leap open");
        assert_eq!(leaked(&ev), 1, "seed {seed}");
        assert!(s.walls().all(|(_, w)| w.hp == w.max_hp));
    }
    // Leaping is a capability (the `jump` object), not the roll: a runner that never takes a
    // ledge by chance still leaps when the ledge is its only open route.
    let no_roll = ENEMIES_JSON.replace("\"chance\": 0.4", "\"chance\": 0.0");
    let (_, _, ev) = sealed(&no_roll, 1);
    assert_eq!(ev.iter().filter(|e| matches!(e, Event::Jumped { .. })).count(), 1);
    assert!(!ev.iter().any(|e| matches!(e, Event::AttackingWall { .. })));
    // A runner that can't leap at all has no open route: it breaks a wall.
    let cant = ENEMIES_JSON.replace("\"jump\": { \"chance\": 0.4, \"max_detour_m\": 4.0 },", "");
    assert_ne!(cant, ENEMIES_JSON);
    let (_, _, ev) = sealed(&cant, 1);
    assert!(ev.iter().any(|e| matches!(e, Event::AttackingWall { .. })));
    assert!(!ev.iter().any(|e| matches!(e, Event::Jumped { .. })));
}

#[test]
fn flyers_ignore_walls_and_only_sky_obelisks_hit_them() {
    let c = one_wave(r#"{"enemy": "wraith", "count": 3, "from": "air", "every_s": 2}"#, ENEMIES_JSON);
    let mut s = State::new(c.clone(), 3);
    for g in GATES {
        apply(&mut s, Command::PlaceWall { cell: cell(&c, g) });
    }
    let warding = c.stone_kind("warding").unwrap();
    for at in ["top:12,9", "top:7,8", "top:8,12", "top:11,7"] {
        apply(&mut s, Command::PlaceStone { cell: cell(&c, at), stone: warding });
    }
    let ev = run_for(&mut s, 5000, |s, _| s.wave_stats()[0].cleared_at.is_some());
    assert_eq!(leaked(&ev), 3, "walls don't stop flyers");
    assert!(!ev.iter().any(|e| matches!(e, Event::Shot { .. })), "warding stones can't target the air");
    assert!(!ev.iter().any(|e| matches!(e, Event::AttackingWall { .. })));

    let mut s = State::new(c.clone(), 3);
    let obelisk = c.stone_kind("sky_obelisk").unwrap();
    for at in ["top:12,11", "top:7,7", "top:11,7", "top:8,12"] {
        apply(&mut s, Command::PlaceStone { cell: cell(&c, at), stone: obelisk });
    }
    let ev = run_for(&mut s, 5000, |s, _| s.wave_stats()[0].cleared_at.is_some());
    assert!(ev.iter().any(|e| matches!(e, Event::Shot { .. })));
    assert!(ev.iter().filter(|e| matches!(e, Event::Died { .. })).count() >= 2, "four obelisks shoot down most wraiths");
}

#[test]
fn climbers_scale_the_cliff_and_cross_the_rim() {
    let c = one_wave(r#"{"enemy": "shambler", "count": 1, "from": "cliff_north_west"}"#, ENEMIES_JSON);
    let mut s = State::new(c.clone(), 1);
    let mut faces = Vec::new();
    let ev = run_for(&mut s, 6000, |s, ev| {
        if let Some(f) = s.enemies().first().and_then(|e| e.face(s.content())) {
            if faces.last() != Some(&f) {
                faces.push(f);
            }
        }
        leaked(ev) > 0
    });
    assert_eq!(faces, vec![Face::North, Face::Top]);
    assert_eq!(leaked(&ev), 1);
    // 15 m at 60 % of 0.8 m/s, then about 28 m on top at 0.8 m/s: roughly 66 s.
    let secs = s.tick() / sepulcra::TICK_HZ;
    assert!((55..80).contains(&secs), "{secs} s");
}

#[test]
fn economy_costs_refunds_and_rejections() {
    let c = Arc::new(Content::builtin());
    let mut s = State::new(c.clone(), 1);
    let warding = c.stone_kind("warding").unwrap();
    let frag = cell(&c, "top:12,9");
    let mut ev = Vec::new();
    assert_eq!(s.dust(), 150);
    apply(&mut s, Command::PlaceStone { cell: frag, stone: warding });
    assert_eq!(s.dust(), 100);
    apply(&mut s, Command::Upgrade { cell: frag });
    assert_eq!((s.dust(), s.stone(frag).unwrap().tier, s.stone(frag).unwrap().invested), (40, 1, 110));
    assert_eq!(s.apply(&Command::Upgrade { cell: frag }, &mut ev), Err(Rejection::NotEnoughDust { need: 90, have: 40 }));
    assert_eq!(s.apply(&Command::PlaceStone { cell: frag, stone: warding }, &mut ev), Err(Rejection::Occupied));
    assert_eq!(s.apply(&Command::PlaceStone { cell: cell(&c, "top:9,14"), stone: warding }, &mut ev), Err(Rejection::NotBuildable));
    assert_eq!(s.apply(&Command::PlaceStone { cell: cell(&c, "top:9,9"), stone: warding }, &mut ev), Err(Rejection::NotBuildable));
    assert_eq!(s.apply(&Command::PlaceStone { cell: frag, stone: sepulcra::StoneKind(99) }, &mut ev), Err(Rejection::UnknownStone));
    assert_eq!(s.apply(&Command::Sell { cell: cell(&c, "top:12,8") }, &mut ev), Err(Rejection::NoStone));
    assert_eq!(s.apply(&Command::Sell { cell: CellId(60_000) }, &mut ev), Err(Rejection::UnknownCell));
    ev.clear();
    apply(&mut s, Command::Sell { cell: frag });
    assert_eq!(s.dust(), 40 + 77, "70 % of 110");
    assert!(s.stone(frag).is_none());

    // Walls: top-face lanes only.
    for not_lane in ["top:12,9", "top:9,9", "top:9,19", "north:4,3"] {
        assert_eq!(s.apply(&Command::PlaceWall { cell: cell(&c, not_lane) }, &mut ev), Err(Rejection::NotWallable), "{not_lane}");
    }
    let lane = cell(&c, "top:9,14");
    apply(&mut s, Command::PlaceWall { cell: lane });
    assert_eq!(s.dust(), 117 - 15);
    assert_eq!(s.apply(&Command::PlaceWall { cell: lane }, &mut ev), Err(Rejection::WallExists));
    apply(&mut s, Command::RemoveWall { cell: lane });
    assert_eq!(s.dust(), 102 + 10, "70 % of 15, rounded down");
    assert_eq!(s.apply(&Command::RemoveWall { cell: lane }, &mut ev), Err(Rejection::NoWall));

    // Tier III is the top.
    let mut rich = State::new(Arc::new(Content::from_texts(SLAB_01_JSON, STONES_JSON, ENEMIES_JSON, &sepulcra::content::WAVES_JSON.replace("\"start_dust\": 150", "\"start_dust\": 1000")).unwrap()), 1);
    apply(&mut rich, Command::PlaceStone { cell: frag, stone: warding });
    apply(&mut rich, Command::Upgrade { cell: frag });
    apply(&mut rich, Command::Upgrade { cell: frag });
    assert_eq!(rich.apply(&Command::Upgrade { cell: frag }, &mut ev), Err(Rejection::MaxTier));
    assert_eq!(rich.dust(), 1000 - 50 - 60 - 90);
}

#[test]
fn calling_a_wave_early_pays_for_the_time_left() {
    let c = Arc::new(Content::builtin());
    let mut s = State::new(c.clone(), 1);
    for _ in 0..300 {
        s.step(&[]);
    }
    let ev = s.step(&[Command::CallWave]);
    // 20 s of build time left at 0.5 dust/s; wave 1 has no income.
    assert!(ev.contains(&Event::DustGained { amount: 10, source: DustSource::EarlyCall }), "{ev:?}");
    assert!(ev.contains(&Event::WaveStarted { wave: 1, early: true }));
    assert_eq!(s.dust(), 160);
    assert_eq!(s.next_wave_tick(), Some(300 + 55 * 30));
    for w in 2..=10 {
        let ev = s.step(&[Command::CallWave]);
        assert!(ev.contains(&Event::WaveStarted { wave: w, early: true }));
    }
    assert_eq!(s.next_wave_tick(), None);
    let ev = s.step(&[Command::CallWave]);
    assert!(ev.contains(&Event::Rejected { index: 0, reason: Rejection::NoMoreWaves }));
}

#[test]
fn kills_pay_dust_in_the_enemys_range() {
    let c = Arc::new(Content::builtin());
    let script = sepulcra::Script::parse(include_str!("../data/layouts/centre.json"), &c, "centre.json").unwrap();
    let mut kills = 0;
    sepulcra::run(c.clone(), 4, &script, Some(3), |_, ev| {
        for pair in ev.windows(2) {
            if let [Event::Died { kind, .. }, Event::DustGained { amount, source: DustSource::Kill }] = pair {
                let (lo, hi) = c.enemy(*kind).dust;
                assert!((lo..=hi).contains(amount), "{amount} not in {lo}..={hi}");
                kills += 1;
            }
        }
    });
    assert!(kills > 20, "{kills}");
}

#[test]
fn the_colossus_summons_shamblers() {
    let c = one_wave(r#"{"enemy": "colossus", "count": 1, "from": "gorge_mid"}"#, ENEMIES_JSON);
    let shambler = c.enemy_kind("shambler").unwrap();
    let mut s = State::new(c.clone(), 1);
    let ev = run_for(&mut s, 30 * 16, |_, _| false);
    let summoned = ev.iter().filter(|e| matches!(e, Event::Spawned { kind, .. } if *kind == shambler)).count();
    assert_eq!(summoned, 4, "two every 7 s");
    assert_eq!(s.wave_stats()[0].spawned, 5);
}

#[test]
fn rune_pillars_slow_and_braziers_splash() {
    let c = one_wave(r#"{"enemy": "shambler", "count": 6, "from": "gorge_mid", "every_s": 0.2}"#, ENEMIES_JSON);
    let mut s = State::new(c.clone(), 1);
    apply(&mut s, Command::PlaceStone { cell: cell(&c, "top:8,15"), stone: c.stone_kind("rune_pillar").unwrap() });
    apply(&mut s, Command::PlaceStone { cell: cell(&c, "top:10,15"), stone: c.stone_kind("brazier").unwrap() });
    let mut slowed = false;
    let ev = run_for(&mut s, 3000, |s, _| {
        slowed |= s.enemies().iter().any(|e| e.is_slowed(s.tick()));
        s.wave_stats()[0].cleared_at.is_some()
    });
    assert!(slowed && ev.iter().any(|e| matches!(e, Event::Pulsed { affected, .. } if *affected >= 2)));
    let explosion = ev.iter().find_map(|e| if let Event::Exploded { projectile, .. } = e { Some(*projectile) } else { None }).expect("a lob landed");
    let first_hits = ev.iter().filter(|e| matches!(e, Event::Hit { projectile, .. } if *projectile == explosion)).count();
    assert!(first_hits >= 2, "a packed group takes splash damage together: {first_hits}");
}

#[test]
fn roots_grow_from_upgraded_stones_once_nature_wakes() {
    let c = Arc::new(Content::builtin());
    let mut s = State::new(c.clone(), 1);
    let frag = cell(&c, "top:12,9");
    apply(&mut s, Command::PlaceStone { cell: frag, stone: c.stone_kind("warding").unwrap() });
    apply(&mut s, Command::Upgrade { cell: frag });
    let rooted = |s: &State| ["top:13,9", "top:12,10", "top:13,10"].iter().all(|n| s.is_rooted(cell(&c, n)));
    for _ in 0..4 {
        s.step(&[Command::CallWave]);
    }
    assert_eq!(s.wave(), 4);
    assert!(!rooted(&s), "no roots before wave 5");
    s.step(&[Command::CallWave]);
    assert!(rooted(&s), "wave 5: tier II stones root the lanes beside them");
    assert!(!s.is_rooted(cell(&c, "top:9,14")));
    apply(&mut s, Command::Sell { cell: frag });
    assert!(!rooted(&s), "selling the stone pulls its roots");
}

#[test]
fn refused_commands_change_nothing() {
    let c = Arc::new(Content::builtin());
    let mut a = State::new(c.clone(), 9);
    let mut b = a.clone();
    let bad = [
        Command::PlaceStone { cell: cell(&c, "top:9,14"), stone: c.stone_kind("warding").unwrap() },
        Command::Upgrade { cell: cell(&c, "top:12,9") },
        Command::RemoveWall { cell: cell(&c, "top:9,14") },
        Command::PlaceWall { cell: cell(&c, "top:9,9") },
    ];
    let ev = a.step(&bad);
    b.step(&[]);
    assert_eq!(ev.iter().filter(|e| matches!(e, Event::Rejected { .. })).count(), 4);
    assert_eq!(a.hash(), b.hash());
}

#[test]
fn losing_stops_time_and_refuses_commands() {
    let c = Arc::new(Content::builtin());
    let mut s = State::new(c, 1);
    let ev = run_for(&mut s, 30 * 60 * 5, |_, _| false);
    assert_eq!(s.outcome(), Outcome::Lost);
    assert_eq!(s.tombstones(), 0);
    assert_eq!(ev.iter().filter(|e| **e == Event::Lost).count(), 1);
    let tick = s.tick();
    let hash = s.hash();
    let ev = s.step(&[Command::CallWave]);
    assert_eq!(ev, vec![Event::Rejected { index: 0, reason: Rejection::GameOver }]);
    assert_eq!((s.tick(), s.hash()), (tick, hash));
}

#[test]
fn state_ascii_draws_stones_walls_and_enemies() {
    let c = one_wave(r#"{"enemy": "runner", "count": 1, "from": "gorge_mid"}"#, ENEMIES_JSON);
    let mut s = State::new(c.clone(), 1);
    apply(&mut s, Command::PlaceStone { cell: cell(&c, "top:12,9"), stone: c.stone_kind("warding").unwrap() });
    apply(&mut s, Command::PlaceWall { cell: cell(&c, "top:13,9") });
    run_for(&mut s, 31, |_, _| false);
    let a = s.ascii();
    assert!(a.starts_with("tick 31 (1s)  wave 1/1  dust"), "{a}");
    assert!(a.lines().any(|l| l == " 9  ------ ######..GGGGW=###### ------"), "{a}");
    assert!(a.lines().any(|l| l == "19  ------ x###S####r#####S##xx ------"), "{a}");
}

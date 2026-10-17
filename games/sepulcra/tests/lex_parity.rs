//! The Lex proof (docs/design/LEX.md): a slice of Sepulcra written as Lex data
//! (`lex/slice.lex.json`) runs next to the hand-written sim on the same content
//! (`lex/slice_waves.json` with the built-in slab, stones and enemies), with the same seed and
//! commands, and every tick must match: grave-dust, tombstones, the wave, the outcome, which
//! commands were refused, and every enemy's and projectile's position (to the millimetre) and
//! every enemy's HP (to the milli-HP).

use std::sync::Arc;

use sepulcra::content::{Content, ENEMIES_JSON, SLAB_01_JSON, STONES_JSON};
use sepulcra::sim::{Command, Event, Outcome, State};
use specula_core::Json;
use specula_lex::{compile, CommandCall, Document, Natives, Options, Sim};

const SLICE: &str = include_str!("../lex/slice.lex.json");
const SLICE_WAVES: &str = include_str!("../lex/slice_waves.json");

#[derive(Clone, Copy, Debug)]
enum Cmd {
    Place(&'static str, &'static str),
    Upgrade(&'static str),
    Sell(&'static str),
    CallWave,
}

fn content() -> Arc<Content> {
    Arc::new(Content::from_texts(SLAB_01_JSON, STONES_JSON, ENEMIES_JSON, SLICE_WAVES).expect("slice content is valid"))
}

fn lex(seed: u64) -> Sim {
    lex_from(SLICE, seed)
}

fn lex_from(text: &str, seed: u64) -> Sim {
    let doc = Document::parse(text).unwrap_or_else(|d| panic!("{d:?}"));
    let compiled = compile(&doc, &Options::default()).unwrap_or_else(|d| panic!("the slice must compile:\n{}", d.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("\n")));
    assert!(compiled.warnings.is_empty(), "the slice compiles without warnings: {:?}", compiled.warnings.iter().map(|w| w.to_string()).collect::<Vec<_>>());
    Sim::new(Arc::new(compiled.rules), seed, &Natives::new()).expect("no native hooks")
}

fn sep_cmd(c: &Content, cmd: Cmd) -> Command {
    let cell = |s: &str| c.map.cell_ref(s).unwrap_or_else(|| panic!("no cell {s}"));
    match cmd {
        Cmd::Place(stone, at) => Command::PlaceStone { cell: cell(at), stone: c.stone_kind(stone).expect("stone") },
        Cmd::Upgrade(at) => Command::Upgrade { cell: cell(at) },
        Cmd::Sell(at) => Command::Sell { cell: cell(at) },
        Cmd::CallWave => Command::CallWave,
    }
}

fn lex_cmd(l: &Sim, cmd: Cmd) -> CommandCall {
    let parse = |name: &str, args: Json| l.parse_command(name, &args).unwrap_or_else(|e| panic!("{name}: {e:?}"));
    match cmd {
        Cmd::Place(stone, at) => parse("place", Json::obj([("site", Json::from(at)), ("stone", Json::from(format!("{stone}_1")))])),
        Cmd::Upgrade(at) => parse("upgrade", Json::obj([("stone", Json::from(at))])),
        Cmd::Sell(at) => parse("sell", Json::obj([("stone", Json::from(at))])),
        Cmd::CallWave => parse("call_wave", Json::obj::<&str>([])),
    }
}

/// What both sims expose each tick, in the same units.
#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    tick: u32,
    dust: i64,
    tombstones: i64,
    wave: u16,
    outcome: Option<&'static str>,
    /// (position mm, hp milli) per living enemy, by id.
    enemies: Vec<([i64; 3], i64)>,
    projectiles: Vec<[i64; 3]>,
}

fn snap_sep(s: &State) -> Snapshot {
    Snapshot {
        tick: s.tick(),
        dust: i64::from(s.dust()),
        tombstones: i64::from(s.tombstones()),
        wave: s.wave(),
        outcome: match s.outcome() {
            Outcome::Running => None,
            Outcome::Won => Some("won"),
            Outcome::Lost => Some("lost"),
        },
        enemies: s.enemies().iter().map(|e| ([e.pos.x, e.pos.y, e.pos.z].map(i64::from), i64::from(e.hp))).collect(),
        projectiles: s.projectiles().iter().map(|p| [p.pos.x, p.pos.y, p.pos.z].map(i64::from)).collect(),
    }
}

fn snap_lex(l: &Sim) -> Snapshot {
    let r = l.rules();
    let tag = |n: &str| r.tag(n).expect("tag");
    let has = |e: &specula_lex::sim::Entity, t: u16| r.archetypes[usize::from(e.arch)].tags.contains(&t);
    let (enemy, projectile) = (tag("enemy"), tag("projectile"));
    Snapshot {
        tick: l.tick(),
        dust: l.resource("dust").unwrap(),
        tombstones: l.resource("tombstones").unwrap(),
        wave: l.state().wave,
        outcome: l.outcome().map(|(res, _)| if res == "won" { "won" } else { "lost" }),
        enemies: l.entities().iter().filter(|e| e.alive && has(e, enemy)).map(|e| (e.pos, l.attr(e.id, "hp").unwrap())).collect(),
        projectiles: l.entities().iter().filter(|e| e.alive && has(e, projectile)).map(|e| e.pos).collect(),
    }
}

/// Runs both sims side by side; panics at the first difference. Returns the final snapshot.
fn parity(seed: u64, scenario: &[(u32, Cmd)], max_ticks: u32) -> (Snapshot, u32) {
    parity_with(SLICE, seed, scenario, max_ticks)
}

fn parity_with(text: &str, seed: u64, scenario: &[(u32, Cmd)], max_ticks: u32) -> (Snapshot, u32) {
    let c = content();
    let mut s = State::new(c.clone(), seed);
    let mut l = lex_from(text, seed);
    let mut refused_total = 0;
    while s.tick() < max_ticks && s.outcome() == Outcome::Running {
        let tick = s.tick();
        let now: Vec<Cmd> = scenario.iter().filter(|(t, _)| *t == tick).map(|(_, c)| *c).collect();
        let sc: Vec<Command> = now.iter().map(|x| sep_cmd(&c, *x)).collect();
        let lc: Vec<CommandCall> = now.iter().map(|x| lex_cmd(&l, *x)).collect();
        let ev = s.step(&sc);
        let lr = l.step(&lc);
        let s_refused: Vec<usize> = ev.iter().filter_map(|e| if let Event::Rejected { index, .. } = e { Some(usize::from(*index)) } else { None }).collect();
        let l_refused: Vec<usize> = lr.iter().map(|(i, _)| *i).collect();
        assert_eq!(s_refused, l_refused, "seed {seed} tick {tick}: refused commands differ ({now:?}); lex said {lr:?}");
        refused_total += s_refused.len();
        let (a, b) = (snap_sep(&s), snap_lex(&l));
        if a != b {
            let diff = if a.enemies.len() != b.enemies.len() {
                format!("enemy count {} vs {}", a.enemies.len(), b.enemies.len())
            } else if let Some(i) = (0..a.enemies.len()).find(|i| a.enemies[*i] != b.enemies[*i]) {
                format!("enemy #{i}: sepulcra {:?} lex {:?}", a.enemies[i], b.enemies[i])
            } else {
                String::new()
            };
            panic!("seed {seed} tick {tick}: the sims diverge {diff}\nsepulcra: {a:?}\nlex:      {b:?}");
        }
    }
    (snap_sep(&s), refused_total as u32)
}

/// A build order that exercises placing, refusals for lack of dust, upgrades of all three
/// stones, an early call, selling and placing again. Placements go in increasing cell id order
/// (see `placements_follow_cell_order`): the hand-written sim fires stones by cell, Lex by
/// entity id, and the two orders agree when stones are placed that way.
const DEFENDED: &[(u32, Cmd)] = &[
    (0, Cmd::Place("warding", "top:10,14")),
    (0, Cmd::Place("rune_pillar", "top:8,15")),
    (600, Cmd::Place("brazier", "top:10,15")),
    (1500, Cmd::Place("brazier", "top:10,15")),
    (2100, Cmd::Upgrade("top:10,14")),
    (2400, Cmd::CallWave),
    (2700, Cmd::Upgrade("top:8,15")),
    (3300, Cmd::Upgrade("top:10,15")),
    (3600, Cmd::Sell("top:8,15")),
    (3601, Cmd::Place("warding", "top:8,16")),
    (4200, Cmd::Upgrade("top:10,14")),
    (4500, Cmd::CallWave),
    (5000, Cmd::Upgrade("top:8,16")),
    (5100, Cmd::Upgrade("top:10,14")),
];

#[test]
fn placements_follow_cell_order() {
    let c = content();
    let mut last = None;
    for (_, cmd) in DEFENDED {
        if let Cmd::Place(_, at) = cmd {
            let id = c.map.cell_ref(at).unwrap();
            assert!(last.is_none_or(|l| id >= l), "{at} (cell {id:?}) is placed after a higher cell id");
            last = Some(id);
        }
    }
}

#[test]
fn defended_slice_matches_the_hand_written_sim_tick_by_tick() {
    for seed in [1, 2, 7, 42, 1234] {
        let (end, refused) = parity(seed, DEFENDED, 20_000);
        eprintln!("seed {seed}: {:?} at tick {}, dust {}, tombstones {}, refused {refused}", end.outcome, end.tick, end.dust, end.tombstones);
        assert!(end.outcome.is_some(), "seed {seed}: the run should finish");
        assert!(refused >= 1, "the scenario includes a refusal for lack of dust");
    }
}

#[test]
fn undefended_slice_loses_identically() {
    for seed in [3, 9] {
        let (end, _) = parity(seed, &[], 20_000);
        assert_eq!(end.outcome, Some("lost"), "seed {seed}");
        assert_eq!(end.tombstones, 0);
    }
}

#[test]
fn lex_slice_is_deterministic_and_seed_sensitive() {
    let run = |seed| {
        let mut l = lex(seed);
        let mut hashes = Vec::new();
        for t in 0..6000u32 {
            let cmds: Vec<CommandCall> = DEFENDED.iter().filter(|(at, _)| *at == t).map(|(_, c)| lex_cmd(&l, *c)).collect();
            l.step(&cmds);
            if t % 500 == 0 {
                hashes.push(l.hash());
            }
        }
        hashes
    };
    assert_eq!(run(5), run(5), "same seed, same commands: same hashes");
    assert_ne!(run(5).last(), run(6).last(), "another seed draws other drops");
}

/// What the interpreter costs against hand-written Rust (foundation F3), on the defended run:
/// `cargo test -p sepulcra --release --test lex_parity -- --ignored --nocapture`.
#[test]
#[ignore]
fn bench_lex_against_rust() {
    let c = content();
    let runs = 200u64;
    let t = std::time::Instant::now();
    let mut ticks = 0u64;
    for seed in 0..runs {
        let mut s = State::new(c.clone(), seed);
        while s.outcome() == Outcome::Running && s.tick() < 20_000 {
            let tick = s.tick();
            let cmds: Vec<Command> = DEFENDED.iter().filter(|(at, _)| *at == tick).map(|(_, x)| sep_cmd(&c, *x)).collect();
            s.step(&cmds);
        }
        ticks += u64::from(s.tick());
    }
    let rust = t.elapsed();
    let t = std::time::Instant::now();
    for seed in 0..runs {
        let mut l = lex(seed);
        while l.outcome().is_none() && l.tick() < 20_000 {
            let tick = l.tick();
            let cmds: Vec<CommandCall> = DEFENDED.iter().filter(|(at, _)| *at == tick).map(|(_, x)| lex_cmd(&l, *x)).collect();
            l.step(&cmds);
        }
    }
    let lexed = t.elapsed();
    let per = |d: std::time::Duration| d.as_secs_f64() * 1e6 / ticks as f64;
    println!("{runs} defended runs, {ticks} ticks: rust {:.2} us/tick, lex {:.2} us/tick (x{:.1}); a 12-minute run in lex: {:.1} ms",
        per(rust), per(lexed), lexed.as_secs_f64() / rust.as_secs_f64(), per(lexed) * 21_600.0 / 1000.0);
}

/// A run stopped and continued (as `save_as` / `from` do) ends where an uninterrupted run does.
#[test]
fn a_continued_run_matches_one_run() {
    use specula_lex::run::{run, RunSpec, Timed};
    for seed in [1u64, 7, 99] {
        let mut a = lex(seed);
        let cmds: Vec<Timed> = DEFENDED.iter().map(|(t, c)| Timed { tick: *t, name: "c".into(), call: lex_cmd(&a, *c) }).collect();
        let full = run(&mut a, &RunSpec { ticks: 9000, commands: cmds.clone(), ..Default::default() });
        let mut b = lex(seed);
        run(&mut b, &RunSpec { ticks: 1234, commands: cmds.iter().filter(|c| c.tick < 1234).cloned().collect(), ..Default::default() });
        let mut c = b.clone();
        let rest = run(&mut c, &RunSpec { ticks: 9000 - 1234, commands: cmds.iter().filter(|c| c.tick >= 1234).cloned().collect(), ..Default::default() });
        assert_eq!(full.get("hash"), rest.get("hash"), "seed {seed}");
        assert!(full.get("hash").is_some());
    }
}

/// The harness must notice a one-point change to one rule, so a pass means something.
#[test]
fn a_changed_rule_is_caught() {
    let changed = SLICE.replacen("\"payload\": [{ \"damage\": \"10pt\" }]", "\"payload\": [{ \"damage\": \"11pt\" }]", 1);
    assert_ne!(changed, SLICE, "the edit applies");
    let panic = std::panic::catch_unwind(|| parity_with(&changed, 1, DEFENDED, 20_000)).expect_err("warding damage 11 instead of 10 must diverge from the hand-written sim");
    let msg = panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned())).unwrap_or_default();
    assert!(msg.contains("diverge"), "it failed for the right reason (a divergence, not a compile or parse error): {msg}");
}

//! The Lex proof (docs/design/LEX.md): a slice of Sepulcra written as Lex data
//! (`lex/slice.lex.json`) runs next to the hand-written sim on the same content
//! (`lex/slice_waves.json` with the built-in slab without its props, stones and enemies), with the same seed and
//! commands, and every tick must match: grave-dust, tombstones, the wave, the outcome, which
//! commands were refused, and every enemy's and projectile's position (to the millimetre) and
//! every enemy's HP (to the milli-HP).

#![allow(clippy::disallowed_methods, reason = "a test and benchmark harness: wall-clock timings and parallel runs, each run deterministic on its own")]

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

/// The built-in slab without its props: the slice is the smallest example and states no prop
/// rule (the whole game below does, with `sepulcra.prop_clear`).
fn slab_without_props() -> String {
    match Json::parse(SLAB_01_JSON).expect("the slab is JSON") {
        Json::Obj(pairs) => Json::Obj(pairs.into_iter().filter(|(k, _)| k != "props").collect()).to_string(),
        _ => unreachable!("the slab is an object"),
    }
}

fn content() -> Arc<Content> {
    Arc::new(Content::from_texts(&slab_without_props(), STONES_JSON, ENEMIES_JSON, SLICE_WAVES).expect("slice content is valid"))
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
#[ignore = "what the interpreter costs against Rust: run by hand, in release"]
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

// ---- the whole game (lex/sepulcra.lex.json + lex/slab_01.lex.json, src/lex.rs) ----

mod full {
    use std::sync::Arc;

    use sepulcra::content::Content;
    use sepulcra::lex::Snapshot;
    use sepulcra::script::{run, Journal, Script};
    use sepulcra::sim::{Command, Event, Outcome, State};
    use specula_lex::CommandCall;

    const CENTRE: &str = include_str!("../data/layouts/centre.json");
    const NO_AIR: &str = include_str!("../data/layouts/no_air.json");
    const FULL_BLOCK: &str = include_str!("../data/layouts/full_block.json");
    const NONE: &str = include_str!("../data/layouts/none.json");
    pub const LAYOUTS: [(&str, &str); 4] = [("centre", CENTRE), ("no_air", NO_AIR), ("full_block", FULL_BLOCK), ("none", NONE)];

    pub fn content() -> Arc<Content> {
        Arc::new(Content::builtin())
    }

    pub fn rules() -> Arc<specula_lex::Rules> {
        sepulcra::lex::compiled(&[]).unwrap_or_else(|e| panic!("the rule set must compile:\n{e}"))
    }

    /// The end of a parity run.
    #[derive(Debug)]
    pub struct End {
        pub snap: Snapshot,
        pub refused: usize,
        /// How often the hand-written sim saw these: walls broken, lane jumps, walls attacked, summons.
        pub broke: u32,
        pub jumped: u32,
        pub attacked: u32,
        pub summoned: u32,
        pub lex_hash: u64,
    }

    /// Steps both sims with the same commands per tick and panics at the first difference.
    pub fn parity(rules: &Arc<specula_lex::Rules>, c: &Arc<Content>, seed: u64, journal: &[(u32, Command)], max_ticks: u32) -> End {
        let mut k = 0;
        parity_with(rules, c, seed, max_ticks, |s| {
            let mut now = Vec::new();
            while k < journal.len() && journal[k].0 <= s.tick() {
                now.push(journal[k].1);
                k += 1;
            }
            now
        })
    }

    /// [`parity`] with the commands chosen each tick by `player`, which sees the state first.
    pub fn parity_with(rules: &Arc<specula_lex::Rules>, c: &Arc<Content>, seed: u64, max_ticks: u32, mut player: impl FnMut(&State) -> Vec<Command>) -> End {
        let mut s = State::new(c.clone(), seed);
        let mut l = sepulcra::lex::sim(rules.clone(), seed).expect("the hooks bind");
        let mut refused = 0;
        let (mut broke, mut jumped, mut attacked, mut summoned) = (0, 0, 0, 0);
        while s.tick() < max_ticks && s.outcome() == Outcome::Running {
            let tick = s.tick();
            let now = player(&s);
            // A command whose args name nothing in the rule set (a stone on a lane cell) is Lex's
            // refusal; the rest are applied in order, like the hand-written sim's.
            let calls: Vec<Result<CommandCall, specula_lex::Rejection>> = now.iter().map(|x| sepulcra::lex::command_call(&l, c, x)).collect();
            let boss_wave = s.enemies().iter().any(|e| c.enemy(e.kind).summon.is_some());
            let ev = s.step(&now);
            for e in &ev {
                match e {
                    Event::WallBroken { .. } => broke += 1,
                    Event::Jumped { .. } => jumped += 1,
                    Event::AttackingWall { .. } => attacked += 1,
                    // Wave 10 sends shamblers up the cliffs only: one on the top face is a summon.
                    Event::Spawned { kind, pos, .. } if boss_wave && pos.y == 0 && c.enemy(*kind).id == "shambler" => summoned += 1,
                    _ => {}
                }
            }
            let parsed: Vec<CommandCall> = calls.iter().filter_map(|x| x.as_ref().ok().cloned()).collect();
            let index: Vec<usize> = calls.iter().enumerate().filter(|(_, x)| x.is_ok()).map(|(i, _)| i).collect();
            let lr = l.step(&parsed);
            let s_refused: Vec<usize> = ev.iter().filter_map(|e| if let Event::Rejected { index, .. } = e { Some(usize::from(*index)) } else { None }).collect();
            let mut l_refused: Vec<usize> = calls.iter().enumerate().filter(|(_, x)| x.is_err()).map(|(i, _)| i).collect();
            l_refused.extend(lr.iter().map(|(i, _)| index[*i]));
            l_refused.sort_unstable();
            let why: Vec<String> = calls.iter().filter_map(|x| x.as_ref().err().map(|r| r.code.clone())).chain(lr.iter().map(|(_, r)| format!("{}: {}", r.code, r.message))).collect();
            assert_eq!(s_refused, l_refused, "seed {seed} tick {tick}: refused commands differ ({now:?}); lex said {why:?}");
            refused += s_refused.len();
            let (a, b) = (Snapshot::of_state(&s), Snapshot::of_lex(&l, &c.map));
            if let Some(d) = a.diff(&b) {
                let diag = &l.state().diag;
                panic!("seed {seed} tick {tick}: the sims diverge: {d}\nlex diagnostics: {:?} {:?}", diag.counts, diag.first);
            }
        }
        let diag = &l.state().diag;
        assert!(diag.counts.is_empty(), "seed {seed}: lex run-time problems: {:?} {:?}", diag.counts, diag.first);
        End { snap: Snapshot::of_state(&s), refused, broke, jumped, attacked, summoned, lex_hash: l.hash() }
    }

    /// A layout's applied commands (with their ticks) as the hand-written sim plays it.
    pub fn journal(c: &Arc<Content>, layout: &str, seed: u64) -> (Journal, Outcome, u32) {
        let script = Script::parse(layout, c, "layout").expect("layout");
        let r = run(c.clone(), seed, &script, None, |_, _| {});
        (r.journal, r.outcome, r.ticks)
    }
}

#[test]
fn the_rule_set_compiles_without_warnings_and_its_slab_matches_the_map() {
    let doc = sepulcra::lex::document().expect("documents");
    let compiled = compile(&doc, &Options::default()).unwrap_or_else(|d| panic!("{}", d.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("\n")));
    let warnings: Vec<String> = compiled.warnings.iter().map(|w| w.to_string()).collect();
    assert!(warnings.is_empty(), "warnings: {warnings:#?}");
    let regenerated = sepulcra::lex::slab_document_text(sepulcra::content::SLAB_01_JSON).unwrap();
    assert_eq!(regenerated, sepulcra::lex::SLAB_LEX.replace("\r\n", "\n"), "lex/slab_01.lex.json is out of date: cargo run -p sepulcra --example lex_slab -- --write");
}

/// A player that tries everything, often wrongly: stones on any cell (lanes, the graveyard,
/// cliff rock), upgrades past the last tier, selling what isn't there, walls under enemies'
/// feet and on cells they are stepping into, walls everywhere until enemies must break them,
/// removing walls, calling waves. Every refusal and every acceptance must agree, and the
/// state must match on every tick.
fn chaos_player(c: &sepulcra::Content, seed: u64, calls: bool) -> impl FnMut(&State) -> Vec<Command> + '_ {
    use sepulcra::{CellId, Motion, StoneKind};
    let mut rng = specula_core::rng::Rng::named(seed, "chaos");
    let n = c.map.cells().len() as u32;
    let buildable: Vec<CellId> = c.map.cells().iter().filter(|x| x.kind.buildable()).map(|x| x.id).collect();
    let wallable: Vec<CellId> = c.map.cells().iter().filter(|x| x.kind.wallable()).map(|x| x.id).collect();
    move |s: &State| {
        let mut out = Vec::new();
        if !s.tick().is_multiple_of(20) {
            return out;
        }
        for _ in 0..rng.below(3) {
            let any = CellId(rng.below(n) as u16);
            let pick = |rng: &mut specula_core::rng::Rng, v: &[CellId]| v[rng.below(v.len() as u32) as usize];
            let stone_cell = |rng: &mut specula_core::rng::Rng| s.stones().map(|(c, _)| c).nth(rng.below(s.stones().count().max(1) as u32) as usize);
            let enemy_cell = |rng: &mut specula_core::rng::Rng| {
                let ground: Vec<(CellId, CellId)> = s.enemies().iter().filter_map(|e| match e.motion {
                    Motion::Ground { from, to, .. } => Some((from, to)),
                    Motion::Air { .. } => None,
                }).collect();
                (!ground.is_empty()).then(|| {
                    let (f, t) = ground[rng.below(ground.len() as u32) as usize];
                    if rng.below(2) == 0 { f } else { t }
                })
            };
            let cmd = match rng.below(100) {
                0..=24 => Command::PlaceStone { cell: if rng.below(4) == 0 { any } else { pick(&mut rng, &buildable) }, stone: StoneKind(rng.below(c.stones.len() as u32) as u8) },
                25..=39 => Command::Upgrade { cell: stone_cell(&mut rng).filter(|_| rng.below(5) != 0).unwrap_or(any) },
                40..=46 => Command::Sell { cell: stone_cell(&mut rng).filter(|_| rng.below(5) != 0).unwrap_or(any) },
                47..=59 => Command::PlaceWall { cell: enemy_cell(&mut rng).unwrap_or(any) },
                60..=79 => Command::PlaceWall { cell: if rng.below(5) == 0 { any } else { pick(&mut rng, &wallable) } },
                80..=93 => Command::RemoveWall { cell: s.walls().map(|(c, _)| c).nth(rng.below(s.walls().count().max(1) as u32) as usize).filter(|_| rng.below(4) != 0).unwrap_or(any) },
                _ if calls => Command::CallWave,
                _ => Command::Upgrade { cell: any },
            };
            out.push(cmd);
        }
        out
    }
}

#[test]
fn a_player_trying_everything_matches_the_hand_written_sim_tick_by_tick() {
    let (c, rules) = (full::content(), full::rules());
    let mut outcomes = Vec::new();
    for seed in 1..=6u64 {
        let end = full::parity_with(&rules, &c, seed, 30_000, chaos_player(&c, seed, true));
        eprintln!("chaos seed {seed}: {:?} at tick {}, dust {}, walls {}, stones {}, refused {}", end.snap.outcome, end.snap.tick, end.snap.dust, end.snap.walls.len(), end.snap.stones.len(), end.refused);
        assert!(end.refused > 10, "the player is refused often: {}", end.refused);
        outcomes.push(end.snap.outcome.clone());
    }
    assert!(outcomes.iter().all(Option::is_some), "every run finishes: {outcomes:?}");
}

/// Map 1's props in both rule sets: a stone on every cell a prop covers (the big pieces refuse it
/// with `prop_blocks` naming the same prop, dressing takes it), poor and rich, and the same
/// stones on dressing then fight a whole game identically. Lex states the rule with
/// `sepulcra.prop_clear`; the hand-written sim with `Rejection::PropBlocks`.
#[test]
fn props_block_building_alike_in_both_rule_sets() {
    use sepulcra::content::{ENEMIES_JSON, SLAB_01_JSON, STONES_JSON, WAVES_JSON};
    use sepulcra::sim::Rejection;
    let waves = WAVES_JSON.replace("\"start_dust\": 150", "\"start_dust\": 4000");
    let rich = Arc::new(Content::from_texts(SLAB_01_JSON, STONES_JSON, ENEMIES_JSON, &waves).unwrap());
    let rich_rules = sepulcra::lex::compiled(&[("start_dust", Json::from(4000))]).unwrap();
    let (poor, poor_rules) = (full::content(), full::rules());
    for (c, rules) in [(&rich, &rich_rules), (&poor, &poor_rules)] {
        let map = &c.map;
        let warding = c.stone_kind("warding").unwrap();
        // Every prop cell once, in cell order (stones fire in cell order in both).
        let mut cells: Vec<sepulcra::CellId> = map.props.iter().flat_map(|p| p.cells.iter().copied()).collect();
        cells.sort();
        cells.dedup();
        assert!(cells.len() > 40, "{} prop cells", cells.len());
        // The Lex refusal names the same prop as the hand-written one.
        let l = sepulcra::lex::sim(rules.clone(), 1).unwrap();
        let s = State::new(c.clone(), 1);
        for &at in &cells {
            let cmd = Command::PlaceStone { cell: at, stone: warding };
            let mut probe = l.clone();
            let lex = sepulcra::lex::command_call(&probe, c, &cmd).and_then(|call| probe.apply(&call));
            match (s.check(&cmd), lex) {
                (Err(Rejection::PropBlocks { prop }), Err(r)) if r.code == "prop_blocks" => {
                    let named = r.details.iter().find(|(k, _)| k == "prop").and_then(|(_, v)| v.as_i64());
                    assert_eq!(named, Some(i64::from(prop)), "{}: {}", map.cell_name(at), r.message);
                    assert!(r.message.contains(&map.props[usize::from(prop)].what), "{}", r.message);
                }
                // Poor: Lex checks the cost first (LEX.md's honest list), both refuse.
                (Err(Rejection::PropBlocks { .. }), Err(r)) if r.code == "not_enough_dust" => {}
                (Ok(()), Ok(())) | (Err(Rejection::NotEnoughDust { .. }), Err(_)) => {}
                (rust, lex) => panic!("{}: rust {rust:?}, lex {lex:?}", map.cell_name(at)),
            }
        }
        // A whole game with a stone tried on every prop cell at the start, and again later.
        let mut tried = 0;
        let end = full::parity_with(rules, c, 1, 40_000, |st: &State| {
            if st.tick() != 0 && st.tick() != 3000 {
                return Vec::new();
            }
            tried += cells.len();
            cells.iter().map(|&at| Command::PlaceStone { cell: at, stone: warding }).collect()
        });
        let blocked = cells.iter().filter(|&&at| map.blocking_prop(at).is_some()).count();
        assert!(end.refused >= blocked * 2, "every blocking cell refused twice: {} of {tried}", end.refused);
        assert!(end.snap.outcome.is_some(), "the game ends: {:?}", end.snap);
        eprintln!("props: {} cells tried twice, {blocked} blocking, refused {}, end {:?} at tick {}", cells.len(), end.refused, end.snap.outcome, end.snap.tick);
    }
}

/// The same player with dust to spare and no early calls, so whole games play out: every tier,
/// roots, cliff ledges, walls everywhere (enemies walled in break them, runners leap out), and
/// the colossus with its summons. The start dust is a param override in Lex and an edited
/// waves.json for the hand-written sim.
#[test]
fn a_rich_player_trying_everything_matches_over_whole_games() {
    use sepulcra::content::{ENEMIES_JSON, SLAB_01_JSON, STONES_JSON, WAVES_JSON};
    let waves = WAVES_JSON.replace("\"start_dust\": 150", "\"start_dust\": 4000");
    assert_ne!(waves, WAVES_JSON);
    let c = Arc::new(Content::from_texts(SLAB_01_JSON, STONES_JSON, ENEMIES_JSON, &waves).unwrap());
    let rules = sepulcra::lex::compiled(&[("start_dust", Json::from(4000))]).unwrap();
    let mut seen = [0u32; 4];
    for seed in 1..=4u64 {
        let end = full::parity_with(&rules, &c, seed, 40_000, chaos_player(&c, seed, false));
        eprintln!(
            "rich chaos seed {seed}: {:?} at tick {} wave {}, dust {}, walls {}, stones {}, refused {}, walls broken {}, attacked {}, jumps {}, summons {}",
            end.snap.outcome, end.snap.tick, end.snap.wave, end.snap.dust, end.snap.walls.len(), end.snap.stones.len(), end.refused, end.broke, end.attacked, end.jumped, end.summoned
        );
        assert!(end.snap.wave >= 5, "seed {seed}: plays into the later waves (wave {})", end.snap.wave);
        for (k, v) in [end.broke, end.attacked, end.jumped, end.summoned].into_iter().enumerate() {
            seen[k] += v;
        }
    }
    assert!(seen.iter().all(|n| *n > 0), "walls broken, walls attacked, lane jumps and summons all happen: {seen:?}");
}

/// Many seeds on every core (`LEX_SWEEP_SEEDS`, default 250 per layout, plus 100 rich chaos
/// games): `cargo test -p sepulcra --release --test lex_parity -- --ignored sweep --nocapture`.
#[test]
#[ignore = "many seeds on every core: run by hand, in release (minutes)"]
fn sweep_every_layout_and_many_seeds() {
    use sepulcra::content::{ENEMIES_JSON, SLAB_01_JSON, STONES_JSON, WAVES_JSON};
    let seeds: u64 = std::env::var("LEX_SWEEP_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(250);
    let (c, rules) = (full::content(), full::rules());
    let rich_c = Arc::new(Content::from_texts(SLAB_01_JSON, STONES_JSON, ENEMIES_JSON, &WAVES_JSON.replace("\"start_dust\": 150", "\"start_dust\": 4000")).unwrap());
    let rich_rules = sepulcra::lex::compiled(&[("start_dust", Json::from(4000))]).unwrap();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()) as u64;
    let t = std::time::Instant::now();
    let results: Vec<(String, u64, Option<String>, u32)> = std::thread::scope(|sc| {
        let jobs: Vec<_> = (0..threads)
            .map(|k| {
                let (c, rules, rich_c, rich_rules) = (&c, &rules, &rich_c, &rich_rules);
                sc.spawn(move || {
                    let mut out = Vec::new();
                    for seed in (1..=seeds).filter(|s| s % threads == k) {
                        for (name, layout) in full::LAYOUTS {
                            let (journal, _, _) = full::journal(c, layout, seed);
                            let end = full::parity(rules, c, seed, &journal, u32::MAX);
                            out.push((name.to_owned(), seed, end.snap.outcome, end.snap.tick));
                        }
                    }
                    for seed in (1..=100u64).filter(|s| s % threads == k) {
                        let end = full::parity_with(rich_rules, rich_c, seed, 40_000, chaos_player(rich_c, seed, false));
                        out.push(("rich_chaos".to_owned(), seed, end.snap.outcome, end.snap.tick));
                    }
                    out
                })
            })
            .collect();
        jobs.into_iter().flat_map(|j| j.join().expect("a parity run diverged (see the panic above)")).collect()
    });
    let mut by: std::collections::BTreeMap<(String, Option<String>), u32> = std::collections::BTreeMap::new();
    let mut ticks = 0u64;
    for (name, _, outcome, t) in &results {
        *by.entry((name.clone(), outcome.clone())).or_default() += 1;
        ticks += u64::from(*t);
    }
    eprintln!("{} runs, {ticks} ticks, identical on every tick, in {:.1} s on {threads} threads: {by:?}", results.len(), t.elapsed().as_secs_f64());
}

/// What the interpreter and the hooks cost against hand-written Rust on the whole game (F3):
/// `cargo test -p sepulcra --release --test lex_parity -- --ignored bench_full --nocapture`.
#[test]
#[ignore = "what the interpreter and hooks cost on the whole game: run by hand, in release"]
fn bench_full_game_lex_against_rust() {
    let (c, rules) = (full::content(), full::rules());
    let layout = full::LAYOUTS[0].1;
    let runs = 20u64;
    let journals: Vec<_> = (0..runs).map(|seed| full::journal(&c, layout, seed).0).collect();
    let t = std::time::Instant::now();
    let mut ticks = 0u64;
    for seed in 0..runs {
        let s = sepulcra::script::replay(c.clone(), seed, &journals[seed as usize], u32::MAX, |_, _| {});
        ticks += u64::from(s.tick());
    }
    let rust = t.elapsed();
    let t = std::time::Instant::now();
    for seed in 0..runs {
        let mut l = sepulcra::lex::sim(rules.clone(), seed).unwrap();
        let j = &journals[seed as usize];
        let mut k = 0;
        while l.outcome().is_none() {
            let mut now = Vec::new();
            while k < j.len() && j[k].0 <= l.tick() {
                now.push(sepulcra::lex::command_call(&l, &c, &j[k].1).unwrap());
                k += 1;
            }
            l.step(&now);
        }
    }
    let lexed = t.elapsed();
    let per = |d: std::time::Duration| d.as_secs_f64() * 1e6 / ticks as f64;
    println!(
        "{runs} centre-heavy games, {ticks} ticks: rust {:.2} us/tick, lex {:.2} us/tick (x{:.1}); a 12-minute game in lex: {:.0} ms",
        per(rust),
        per(lexed),
        lexed.as_secs_f64() / rust.as_secs_f64(),
        per(lexed) * 21_600.0 / 1000.0
    );
}

/// Review repro (race.rs): clones of one run stepping on two threads share their bound hooks;
/// what the hooks cache must never leak from one clone's state into the other's.
#[test]
fn clones_stepping_on_threads_agree_with_sequential_runs() {
    let (c, rules) = (full::content(), full::rules());
    let seed = 2;
    let layout = |n: &str| full::LAYOUTS.iter().find(|(x, _)| *x == n).unwrap().1;
    let (ja, _, _) = full::journal(&c, layout("full_block"), seed);
    let (jb, _, _) = full::journal(&c, layout("centre"), seed);
    let play = |mut l: specula_lex::Sim, j: &[(u32, Command)]| {
        let mut k = 0;
        while l.outcome().is_none() && l.tick() < 40_000 {
            let mut now = Vec::new();
            while k < j.len() && j[k].0 <= l.tick() {
                if let Ok(x) = sepulcra::lex::command_call(&l, &c, &j[k].1) {
                    now.push(x);
                }
                k += 1;
            }
            l.step(&now);
        }
        (l.tick(), l.hash())
    };
    let base = sepulcra::lex::sim(rules, seed).unwrap();
    let (ra, rb) = (play(base.clone(), &ja), play(base.clone(), &jb));
    let proof = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lex").join("proof.json")).unwrap();
    assert!(proof.contains(&format!("{:016x}", ra.1)), "the sequential full block run is proof.json's");
    for trial in 0..3 {
        let (a, b) = (base.clone(), base.clone());
        let (x, y) = std::thread::scope(|s| {
            let h1 = s.spawn(|| play(a, &ja));
            let h2 = s.spawn(|| play(b, &jb));
            (h1.join().unwrap(), h2.join().unwrap())
        });
        assert_eq!((x, y), (ra, rb), "trial {trial}: concurrent clones diverged from the sequential runs");
    }
}

/// Review repro (targeted.rs): attribute values an agent may write (huge ratios, speeds, chances,
/// cells) never panic the hooks (an engine's main thread runs them in live runs); a bad state is a
/// run-time diagnostic.
#[test]
fn extreme_attribute_values_never_panic_the_hooks() {
    let cases: &[(&str, &str, &str)] = &[
        ("shambler", "climb_speed", "\"1000000000000000%\""),
        ("shambler", "climb_speed", "\"9000000000000%\""),
        ("enemy", "speed_scale", "\"9000000000000%\""),
        ("wraith", "circle_max", "9000000000000000"),
        ("wraith", "circle_min", "9000000000000000"),
        ("wraith", "speed", "\"1000000000000m/s\""),
        ("shambler", "speed", "\"1000000000000m/s\""),
        ("runner", "jump_chance", "\"1000000000000%\""),
        ("runner", "jump_detour", "\"9000000000000m\""),
        ("enemy", "c_from", "9000000000000000"),
        ("enemy", "prog", "\"-5m\""),
        ("enemy", "elen", "\"9000000000m\""),
        ("enemy", "carry", "-9000000000000000"),
        ("shambler", "wall_dps", "\"9000000000000pt\""),
        ("wraith", "dir", "-9000000000000000"),
        ("wraith", "hops", "-9000000000000000"),
    ];
    let base = sepulcra::lex::document().unwrap();
    for (item, attr, v) in cases {
        let mut doc = base.clone();
        let it = doc.items.get_mut(*item).unwrap();
        let mut attrs = it.get("attributes").cloned().unwrap_or(Json::Obj(vec![]));
        attrs.set(*attr, Json::parse(v).unwrap());
        it.set("attributes", attrs);
        let opts = Options { overrides: vec![("start_dust".into(), Json::from(100_000))], ..Default::default() };
        let Ok(compiled) = compile(&doc, &opts) else { continue };
        let rules = Arc::new(compiled.rules);
        let mut sim = Sim::new(rules.clone(), 3, &sepulcra::lex::natives()).unwrap();
        let spots: Vec<String> = rules.sites.iter().filter(|s| rules.site_groups[usize::from(s.group)] == "build_spots").map(|s| s.name.clone()).collect();
        let stones = ["warding_1", "rune_pillar_1", "brazier_1", "sky_obelisk_1"];
        for (i, s) in spots.iter().enumerate().take(40) {
            if let Ok(c) = sim.parse_command("place", &Json::obj([("site", Json::from(s.as_str())), ("stone", Json::from(stones[i % 4]))])) {
                let _ = sim.apply(&c);
            }
            if let Ok(c) = sim.parse_command("upgrade", &Json::obj([("stone", Json::from(s.as_str()))])) {
                let _ = sim.apply(&c);
            }
        }
        for _ in 0..6000 {
            let mut cmds = vec![];
            if sim.tick().is_multiple_of(300) {
                if let Ok(c) = sim.parse_command("call_wave", &Json::Obj(vec![])) {
                    cmds.push(c);
                }
            }
            sim.step(&cmds);
            if sim.outcome().is_some() {
                break;
            }
        }
        eprintln!("{item}.{attr} = {v}: tick {} outcome {:?} diagnostics {:?}", sim.tick(), sim.outcome(), sim.state().diag.counts);
    }
}

/// `lex/proof.json`: whole games as Lex commands with the hashes and outcomes Lex reaches, for
/// `scripts/sepulcra_lex_tools.py` (which rebuilds the rule set through the lex_* tools and
/// simulates these). Rewrite with `LEX_WRITE_PROOF=1`.
#[test]
fn the_tools_proof_fixture_is_current() {
    let (c, rules) = (full::content(), full::rules());
    let cases = [("centre", 7u64), ("no_air", 1), ("full_block", 2)];
    let mut out = Vec::new();
    for (name, seed) in cases {
        let layout = full::LAYOUTS.iter().find(|(n, _)| *n == name).unwrap().1;
        let (journal, outcome, ticks) = full::journal(&c, layout, seed);
        let mut l = sepulcra::lex::sim(rules.clone(), seed).unwrap();
        let mut k = 0;
        let mut commands = Vec::new();
        while l.outcome().is_none() {
            let mut now = Vec::new();
            while k < journal.len() && journal[k].0 <= l.tick() {
                let (cmd, args) = sepulcra::lex::command_json(&c, &journal[k].1);
                commands.push(Json::obj([("tick", Json::from(journal[k].0)), ("command", Json::from(cmd)), ("args", args)]));
                now.push(sepulcra::lex::command_call(&l, &c, &journal[k].1).unwrap());
                k += 1;
            }
            assert!(l.step(&now).is_empty());
        }
        let rust = match outcome {
            sepulcra::Outcome::Won => "won",
            sepulcra::Outcome::Lost => "lost",
            sepulcra::Outcome::Running => "running",
        };
        assert_eq!((l.tick(), l.outcome().map(|(r, _)| r)), (ticks, Some(rust)), "{name}");
        out.push(Json::obj([
            ("layout", Json::from(name)),
            ("seed", Json::from(seed)),
            ("ticks", Json::from(l.tick())),
            ("outcome", Json::from(rust)),
            ("hash", Json::from(format!("{:016x}", l.hash()))),
            ("rules_hash", Json::from(format!("{:016x}", rules.hash))),
            ("rust_hash", Json::from(format!("{:016x}", sepulcra::script::replay(c.clone(), seed, &journal, u32::MAX, |_, _| {}).hash()))),
            ("commands", Json::Arr(commands)),
        ]));
    }
    let doc = Json::obj([
        ("notes", Json::from("Whole games of Sepulcra as Lex commands (from the hand-written sim's layouts), with the tick, outcome and Lex hash the rule set reaches; rust_hash is the hand-written sim's. Generated by tests/lex_parity.rs (LEX_WRITE_PROOF=1); scripts/sepulcra_lex_tools.py simulates them through the tools.")),
        ("cases", Json::Arr(out)),
    ]);
    let mut text = String::new();
    sepulcra::lex::pretty(&doc, 0, &mut text);
    text.push('\n');
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lex").join("proof.json");
    if std::env::var("LEX_WRITE_PROOF").is_ok_and(|v| v == "1") {
        std::fs::write(&path, &text).unwrap();
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    assert!(current == text, "lex/proof.json is out of date: LEX_WRITE_PROOF=1 cargo test -p sepulcra --test lex_parity the_tools_proof");
}

#[test]
fn every_layout_matches_the_hand_written_sim_tick_by_tick() {
    let (c, rules) = (full::content(), full::rules());
    for (name, layout) in full::LAYOUTS {
        for seed in [1u64, 7] {
            let (journal, outcome, ticks) = full::journal(&c, layout, seed);
            let end = full::parity(&rules, &c, seed, &journal, u32::MAX);
            eprintln!("{name} seed {seed}: {:?} at tick {} (rust {outcome:?} at {ticks}), tombstones {}, lex hash {:016x}", end.snap.outcome, end.snap.tick, end.snap.tombstones, end.lex_hash);
            assert_eq!(end.snap.tick, ticks, "{name} seed {seed}");
        }
    }
}

/// Review round 5 (workcache.rs): a tick's work is a function of the state alone. A run and a
/// clone of it (a fresh state instance: the hooks' caches miss for it) step to the same hash
/// with the same `tick_work` on every tick of a whole game, so live budgets don't depend on
/// what a process has cached.
#[test]
fn tick_work_does_not_depend_on_the_native_cache() {
    let c = full::content();
    let rules = sepulcra::lex::compiled(&[]).unwrap();
    let seed = 7;
    let script = sepulcra::script::Script::parse(full::LAYOUTS[0].1, &c, "centre").unwrap();
    let journal = sepulcra::script::run(c.clone(), seed, &script, None, |_, _| {}).journal;
    let mut a = sepulcra::lex::sim(rules, seed).unwrap();
    let (mut k, mut ticks) = (0, 0);
    while a.outcome().is_none() && a.tick() < 21_000 {
        let mut now = Vec::new();
        while k < journal.len() && journal[k].0 <= a.tick() {
            if let Ok(x) = sepulcra::lex::command_call(&a, &c, &journal[k].1) {
                now.push(x);
            }
            k += 1;
        }
        let mut b = a.clone();
        a.step(&now);
        b.step(&now);
        assert_eq!((a.hash(), a.tick_work()), (b.hash(), b.tick_work()), "tick {}", a.tick());
        ticks += 1;
    }
    assert!(ticks > 20_000, "a whole game: {ticks} ticks");
}

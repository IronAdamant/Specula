//! Headless Sepulcra: play a layout (build order) for one seed, or sweep many seeds. The first
//! balance tool.
//!
//! ```text
//! cargo run -p sepulcra --example headless --release -- --seed 1
//! cargo run -p sepulcra --example headless --release -- --layout no_air --sweep 200
//! cargo run -p sepulcra --example headless --release -- --layout my.json --data my_data/ --waves 4
//! ```
//!
//! Flags: `--seed N` (default 1; the first seed of a sweep), `--layout NAME|FILE` (built-ins:
//! centre, no_air, full_block, none, or `all` for each built-in in turn; default centre), `--waves N` (stop before wave N+1),
//! `--sweep N` (N seeds, outcome stats), `--data DIR` (custom content files), `--ground proving` (the
//! proving ground of the elemental stones, with its own layouts: baseline, fire, lightning, frost, wind,
//! earth and their ten pairs such as fire_wind; DESIGN.md, "Elemental stones"), `--map` (print
//! the slab), `--ascii` (print the final state), `--journal FILE` (write a self-checking replay
//! recording), `--replay FILE` (replay a recording and verify its final tick and hash).

#![allow(clippy::disallowed_methods, reason = "a test and benchmark harness: wall-clock timings and parallel runs, each run deterministic on its own")]

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use sepulcra::content::Content;
use sepulcra::script::{run, sweep_lines, Recording, ReplayStatus, RunReport, Script};
use sepulcra::sim::{Event, Outcome};
use sepulcra::TICK_HZ;

const LAYOUTS: [(&str, &str); 4] = [
    ("centre", include_str!("../data/layouts/centre.json")),
    ("no_air", include_str!("../data/layouts/no_air.json")),
    ("full_block", include_str!("../data/layouts/full_block.json")),
    ("none", include_str!("../data/layouts/none.json")),
];

/// The proving ground's layouts (`--ground proving`; DESIGN.md, "Elemental stones").
const PROVING: [(&str, &str); 16] = [
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

struct Args {
    seed: u64,
    layout: String,
    waves: Option<u16>,
    sweep: Option<u64>,
    data: Option<String>,
    proving: bool,
    map: bool,
    ascii: bool,
    journal: Option<String>,
    replay: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { seed: 1, layout: "centre".into(), waves: None, sweep: None, data: None, proving: false, map: false, ascii: false, journal: None, replay: None };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match flag.as_str() {
            "--seed" => a.seed = val("--seed")?.parse().map_err(|e| format!("--seed: {e}"))?,
            "--layout" => a.layout = val("--layout")?,
            "--waves" => a.waves = Some(val("--waves")?.parse().map_err(|e| format!("--waves: {e}"))?),
            "--sweep" => a.sweep = Some(val("--sweep")?.parse().map_err(|e| format!("--sweep: {e}"))?),
            "--data" => a.data = Some(val("--data")?),
            "--ground" => match val("--ground")?.as_str() {
                "proving" => a.proving = true,
                "slab_01" | "map1" => a.proving = false,
                other => return Err(format!("--ground: {other:?} (proving or slab_01)")),
            },
            "--journal" => a.journal = Some(val("--journal")?),
            "--replay" => a.replay = Some(val("--replay")?),
            "--map" => a.map = true,
            "--ascii" => a.ascii = true,
            other => return Err(format!("unknown flag {other} (see the doc comment in examples/headless.rs)")),
        }
    }
    Ok(a)
}

fn clock(ticks: u32) -> String {
    let s = ticks / TICK_HZ;
    format!("{}:{:02}", s / 60, s % 60)
}

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<(), String> {
    let args = parse_args()?;
    let content = Arc::new(match &args.data {
        Some(dir) => Content::load_dir(std::path::Path::new(dir)).map_err(|e| e.to_string())?,
        None if args.proving => Content::proving(),
        None => Content::builtin(),
    });
    let layout = if args.proving && args.layout == "centre" { "baseline".to_owned() } else { args.layout.clone() };
    let layouts: &[(&str, &str)] = if args.proving { &PROVING } else { &LAYOUTS };
    let dir = if args.proving { "proving/layouts" } else { "layouts" };
    if args.map {
        println!("{}", content.map.ascii());
    }
    if let Some(path) = &args.replay {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let rec = Recording::from_json(&text, &content, path).map_err(|e| e.to_string())?;
        let t0 = Instant::now();
        let done = rec.verify(Arc::clone(&content)).map_err(|e| e.to_string())?;
        let (state, verdict) = match done.status {
            ReplayStatus::Verified => (done.state, "replay verified"),
            ReplayStatus::Unverified => (done.state, "replayed (unverified: the recording has no end tick and hash)"),
        };
        println!(
            "{verdict}: seed {}, {} commands, {:?} at tick {}, hash {:016x} ({:.1} ms)",
            rec.seed,
            rec.journal.len(),
            state.outcome(),
            state.tick(),
            state.hash(),
            t0.elapsed().as_secs_f64() * 1000.0
        );
        return Ok(());
    }
    let names: Vec<&str> = if layout == "all" { layouts.iter().map(|(n, _)| *n).collect() } else { vec![layout.as_str()] };
    let mut stranded_runs = 0;
    for name in names {
        let (layout_text, layout_file) = match layouts.iter().find(|(n, _)| *n == name) {
            Some((n, t)) => (t.to_string(), format!("{dir}/{n}.json")),
            None => (std::fs::read_to_string(name).map_err(|e| format!("{name}: {e}"))?, name.to_owned()),
        };
        let script = Script::parse(&layout_text, &content, &layout_file).map_err(|e| e.to_string())?;
        match args.sweep {
            Some(n) => stranded_runs += sweep(&content, &script, args.seed, n, args.waves)?,
            None => single(&content, &script, &args)?,
        }
    }
    // Validated content never strands an enemy, so any stranding is a bug worth failing on.
    if stranded_runs > 0 {
        return Err(format!("{stranded_runs} runs stranded enemies (no route at all): check the content"));
    }
    Ok(())
}

fn single(content: &Arc<Content>, script: &Script, args: &Args) -> Result<(), String> {
    // (tick, dust, tombstones) when each wave started.
    let mut starts: Vec<(u32, i32, i32)> = Vec::new();
    let mut jumps = 0u32;
    let mut walls_broken = 0u32;
    // The elements (proving ground): ignited, frozen, shattered, struck, gusts, heaves, bursts (the
    // `Detonated` events: the same count as the waves' `detonated`), fuel cells caught.
    let mut el = [0u32; 8];
    let mut last_ascii = String::new();
    let t0 = Instant::now();
    let report = run(Arc::clone(content), args.seed, script, args.waves, |s, ev| {
        for e in ev {
            match e {
                Event::WaveStarted { .. } => starts.push((s.tick() - 1, s.dust(), s.tombstones())),
                Event::Jumped { .. } => jumps += 1,
                Event::WallBroken { .. } => walls_broken += 1,
                Event::Ignited { .. } => el[0] += 1,
                Event::Frozen { .. } => el[1] += 1,
                Event::Shattered { .. } => el[2] += 1,
                Event::Struck { .. } => el[3] += 1,
                Event::Gusted { .. } => el[4] += 1,
                Event::Heaved { .. } => el[5] += 1,
                Event::Detonated { .. } => el[6] += 1,
                Event::CellCaught { .. } => el[7] += 1,
                _ => {}
            }
        }
        if args.ascii && s.outcome() != Outcome::Running {
            last_ascii = s.ascii();
        }
    });
    let elapsed = t0.elapsed();
    println!("Sepulcra headless: layout {:?}, seed {}", script.name, args.seed);
    println!("wave  start  cleared  spawned  killed  leaked  stranded  lost  dust@start  tombs@start  kill-dust");
    for (i, w) in report.waves.iter().enumerate() {
        let Some((t, dust, tombs)) = starts.get(i) else { break };
        let cleared = w.cleared_at.map_or("-".to_owned(), clock);
        println!(
            "{:>4}  {:>5}  {:>7}  {:>7}  {:>6}  {:>6}  {:>8}  {:>4}  {:>10}  {:>11}  {:>9}",
            i + 1,
            clock(*t),
            cleared,
            w.spawned,
            w.killed,
            w.leaked,
            w.stranded,
            w.tombstones_lost,
            dust,
            tombs,
            w.dust_from_kills
        );
    }
    println!(
        "outcome {:?} at {} (tick {}), wave {}, tombstones {}/{}, dust {}, lane jumps {}, walls broken {}",
        report.outcome,
        clock(report.ticks),
        report.ticks,
        report.wave,
        report.tombstones,
        content.rules.tombstones,
        report.dust,
        jumps,
        walls_broken
    );
    if content.elemental() {
        println!(
            "elements: ignited {}, frozen {}, shattered {}, strikes {}, gusts {}, heaves {}, bursts {}, fuel cells caught {}",
            el[0], el[1], el[2], el[3], el[4], el[5], el[6], el[7]
        );
    }
    for (i, r) in &report.skipped {
        println!("skipped layout entry {i}: {r}");
    }
    let w = report.waits;
    if w.entries > 0 || w.never_applied > 0 {
        println!("layout waits: {} entries waited (for dust or for one of the dead on the cell), the longest {} ticks; {} never applied", w.entries, w.longest, w.never_applied);
    }
    println!("final hash {:016x}", report.hash);
    let secs = elapsed.as_secs_f64();
    println!("simulated {} of game time in {:.1} ms ({:.0}x real time)", clock(report.ticks), secs * 1000.0, f64::from(report.ticks) / f64::from(TICK_HZ) / secs);
    if args.ascii {
        println!("{last_ascii}");
    }
    if let Some(path) = &args.journal {
        std::fs::write(path, report.recording(content).to_json(content).to_pretty()).map_err(|e| format!("{path}: {e}"))?;
        println!("recording written to {path} (check it with --replay {path})");
    }
    Ok(())
}

/// Returns how many runs stranded an enemy.
fn sweep(content: &Arc<Content>, script: &Script, first: u64, n: u64, waves: Option<u16>) -> Result<u64, String> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(n.max(1) as usize);
    let t0 = Instant::now();
    let mut reports: Vec<RunReport> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|k| {
                let content = Arc::clone(content);
                s.spawn(move || {
                    (first + k as u64..first + n).step_by(threads).map(|seed| run(Arc::clone(&content), seed, script, waves, |_, _| {})).collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("sweep thread")).collect()
    });
    let elapsed = t0.elapsed();
    reports.sort_by_key(|r| r.seed);
    println!("Sepulcra sweep: layout {:?}, seeds {}..{}, {} threads", script.name, first, first + n - 1, threads);
    // The summary's lines come from the library (`sweep_lines`), where a test holds that content
    // without the elements gets the lines it got before them.
    for line in sweep_lines(content, &reports) {
        println!("{line}");
    }
    let stranded_runs = reports.iter().filter(|r| r.stranded() > 0).count() as u64;
    let ticks: u64 = reports.iter().map(|r| u64::from(r.ticks)).sum();
    println!(
        "{} runs, {} game-minutes in {:.2} s wall ({:.1} ms per run per thread)",
        reports.len(),
        ticks / u64::from(TICK_HZ) / 60,
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0 * threads as f64 / reports.len().max(1) as f64
    );
    Ok(stranded_runs)
}

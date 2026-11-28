//! The gate: what a change must pass before it is reported or merged, as one command.
//!
//! ```text
//! cargo run -p specula-examen -- gate                  # what this change needs
//! cargo run -p specula-examen -- gate --all            # everything
//! cargo run -p specula-examen -- gate --only tests,clippy
//! ```
//!
//! **It is sized to what changed** since `--base` (`origin/main` unless named): `changed.rs`
//! puts every changed path in a tier, and the change's tier names the steps. A path that file
//! does not know is the engine's, so sizing only ever leaves a step out for a path known not to
//! need it. The first lines of a run say what was chosen and what was left out, and the summary
//! keeps them: a sized pass is never mistaken for a full one. `--all` runs everything (the end
//! of a milestone, a move of the Rust pin); `--tier` and `--only` choose by hand.
//!
//! **Commit, then gate.** With no `--base`, a change is measured from the last commit the gate
//! passed at in this checkout (`passed.rs`), and from `origin/main` when there is none. A pass is
//! recorded only when nothing is uncommitted, so the order is: commit, run the gate, and fix on
//! top if it fails.
//!
//! Steps (each one's log is kept; the summary is `<target>/gate/summary.json`):
//!
//! - `rust`: always. The Rust this run uses is the one `rust-toolchain.toml` pins (`pin.rs`).
//! - `gpu`: before any step that runs tests. Which adapters the tests will see. GPU tests skip
//!   themselves cleanly without one (so CI stays green), which on a developer's machine would be
//!   a gate that tested nothing: no adapter fails the gate unless `--no-gpu` says it is expected.
//! - `docs`: the document guards and this tool's own tests (`cargo test -p specula-examen`).
//! - `tests`: `cargo test --no-fail-fast --workspace`, once.
//! - `repeat`: the engine's library tests again, `--repeat` times (5) at `--threads` test
//!   threads (16). One run can't tell a solid test from one that passes three times in four.
//! - `slow`: the same tests once more with `SPECULA_TEST_WORKER_DELAY_MS` set: every worker
//!   thread starts its job late, so a result that depends on when a worker runs fails every
//!   time, not now and then (`crates/specula/src/workers.rs`, `test_delay`).
//! - `speed`: the tests that hold a speed budget (`speed_budget(..)`), one at a time, with
//!   `SPECULA_SPEED_TESTS` set: alone, their tight bounds apply, scaled by how much slower than
//!   its best the machine is just before each package's run (`mensura.rs`); past 8 times the
//!   step fails as too loaded to judge, naming no test.
//! - `clippy`: Windows and Linux, warnings denied.
//! - `builds`: debug and release of the engine and the game (the live checks run both).
//!
//! A test that failed in some runs and passed in others is reported as FLAKY and fails the
//! gate: it is a bug whose cause hasn't been found yet, never "noise" (owner, 2026-11-12: a
//! gate must not pass by luck). A failed gate ends with what each failed test said, not its
//! name alone: on CI the logs it keeps are gone with the machine.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use specula_core::Json;

use crate::changed::{self, Change, Tier};
use crate::cli::Args;
use crate::mensura;
use crate::passed;
use crate::pin;
use crate::run::{root, run, target_dir, Ran};
use crate::testlog::{flaky, outputs, parse, verdict, Exit, Parsed};

pub const STEPS: [&str; 7] = ["docs", "tests", "repeat", "slow", "speed", "clippy", "builds"];
/// The steps that run tests of the engine, and so need to know which adapters they will see.
const TEST_STEPS: [&str; 4] = ["tests", "repeat", "slow", "speed"];

/// Rule 3: an environment `RUSTFLAGS` replaces the config's, so the linker flag is said again.
fn deny() -> &'static str {
    if cfg!(windows) {
        "-Dwarnings -C link-self-contained=yes"
    } else {
        "-Dwarnings"
    }
}

/// What the run was asked for.
struct Options {
    jobs: u64,
    repeat: u64,
    threads: u64,
    slow_workers_ms: u64,
    package: String,
    no_gpu: bool,
    out: PathBuf,
}

/// Which steps run, and why: (steps, the lines that say so, the sizing for the summary).
fn chosen(only: Vec<String>, all: bool, tier: Option<String>, base: &str, root: &Path) -> Result<(Vec<&'static str>, Vec<String>, Json), String> {
    let pick = |names: &[String]| -> Result<Vec<&'static str>, String> {
        let unknown: Vec<&String> = names.iter().filter(|n| !STEPS.contains(&n.as_str())).collect();
        if unknown.is_empty() {
            Ok(STEPS.iter().copied().filter(|s| names.iter().any(|n| n == s)).collect())
        } else {
            Err(format!("unknown step(s) {unknown:?}: choose from {STEPS:?}"))
        }
    };
    if !only.is_empty() {
        let steps = pick(&only)?;
        return Ok((steps, vec![format!("steps by hand (--only): {}", only.join(", "))], Json::obj([("by", Json::Str("only".to_owned()))])));
    }
    let (by, change) = if all {
        ("all", Change::everything("--all"))
    } else if let Some(name) = tier {
        let tier = Tier::named(&name).ok_or_else(|| format!("unknown tier {name:?}: docs, tests or engine"))?;
        ("tier", Change { tier, since: "--tier".to_owned(), paths: Vec::new(), because: None })
    } else {
        ("change", changed::since(base, root))
    };
    let steps: Vec<&'static str> = STEPS.iter().copied().filter(|s| change.tier.steps().contains(s)).collect();
    let left_out: Vec<&str> = STEPS.iter().copied().filter(|s| !steps.contains(s) && *s != "docs").collect();
    let mut lines = vec![match by {
        "all" => "everything (--all)".to_owned(),
        "tier" => format!("tier by hand (--tier): {}", change.tier.name()),
        _ => format!("sized to what changed: {}", change.said()),
    }];
    if !left_out.is_empty() {
        lines.push(format!("left out: {} (--all runs everything)", left_out.join(", ")));
    }
    let sizing = Json::obj([
        ("by", Json::Str(by.to_owned())),
        ("tier", Json::Str(change.tier.name().to_owned())),
        ("since", Json::Str(change.since.clone())),
        ("paths", Json::Num(change.paths.len() as f64)),
        ("because", change.because.clone().map_or(Json::Null, Json::Str)),
        ("left_out", Json::arr(left_out.iter().map(|s| Json::Str((*s).to_owned())))),
    ]);
    Ok((steps, lines, sizing))
}

/// `fn NAME(` or `fn NAME<` at the start of a line (after `pub`, `pub(..)` and `async`).
fn fn_line(line: &str) -> Option<&str> {
    let mut s = line.trim_start();
    if let Some(rest) = s.strip_prefix("pub") {
        if let Some(rest) = rest.strip_prefix(' ') {
            s = rest;
        } else {
            let rest = rest.strip_prefix('(')?;
            let n = rest.bytes().take_while(u8::is_ascii_lowercase).count();
            s = rest[n..].strip_prefix(") ").filter(|_| n > 0)?;
        }
    }
    s = s.strip_prefix("async ").unwrap_or(s);
    let rest = s.strip_prefix("fn ")?;
    let n = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').map(char::len_utf8).sum::<usize>();
    let after = rest[n..].trim_start();
    (n > 0 && (after.starts_with('(') || after.starts_with('<'))).then_some(&rest[..n])
}

/// Adds to `found` the functions in `text` that hold a speed budget.
fn speed_tests_in(text: &str, found: &mut BTreeSet<String>) {
    let mut name: Option<&str> = None;
    for line in text.lines() {
        if let Some(n) = fn_line(line) {
            name = Some(n);
        } else if line.split("//").next().unwrap_or("").contains("speed_budget(") {
            if let Some(n) = name.filter(|n| *n != "speed_budget") {
                found.insert(n.to_owned());
            }
        }
    }
}

/// The cargo package a source file belongs to (the nearest `Cargo.toml` above it).
fn package_of(path: &Path) -> Option<String> {
    let manifest = path.ancestors().skip(1).map(|dir| dir.join("Cargo.toml")).find(|m| m.is_file())?;
    let text = std::fs::read_to_string(manifest).ok()?;
    let line = text.lines().find(|l| l.starts_with("name") && l[4..].trim_start().starts_with('='))?;
    let name = line.split_once('=')?.1.trim().strip_prefix('"')?.split('"').next()?;
    (!name.is_empty()).then(|| name.to_owned())
}

/// {package: names} of every test in the tree that holds a speed budget.
fn speed_tests(root: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut stack = vec![root.join("crates"), root.join("games")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for path in entries.flatten().map(|e| e.path()) {
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            if path.is_dir() {
                if !rel.ends_with("/target") {
                    stack.push(path);
                }
            } else if rel.ends_with(".rs") && !rel.ends_with("tests/clock_guard.rs") {
                let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap_or_default()).into_owned();
                if text.contains("speed_budget(") {
                    if let Some(package) = package_of(&path) {
                        speed_tests_in(&text, found.entry(package).or_default());
                    }
                }
            }
        }
    }
    found.retain(|_, names| !names.is_empty());
    found
}

fn strings(list: &[String]) -> Json {
    Json::arr(list.iter().cloned().map(Json::Str))
}

fn exit_json(exit: Exit) -> Json {
    match exit {
        Exit::Code(c) => Json::Num(f64::from(c)),
        Exit::TimedOut => Json::Str("timed out".to_owned()),
    }
}

fn parsed_json(r: &Parsed, ran: &Ran) -> Vec<(&'static str, Json)> {
    vec![
        ("passed", Json::Num(r.passed as f64)),
        ("failed", Json::Num(r.failed as f64)),
        ("failed_tests", strings(&r.failed_tests)),
        ("broken", strings(&r.broken)),
        ("still_running", strings(&r.still_running)),
        ("drivers_not_loaded", Json::Num(r.drivers_not_loaded as f64)),
        ("exit", exit_json(ran.exit)),
        ("seconds", Json::Num(ran.seconds.round())),
    ]
}

/// A gate's run: its steps' records, why it failed, and what its failed tests said.
struct Gate {
    opts: Options,
    root: PathBuf,
    steps: Vec<(String, Json)>,
    failures: Vec<String>,
    said: Vec<(String, String, String)>,
    /// The machine's speed reference, first measured at the gate's start (`mensura.rs`).
    reference: Option<mensura::Reference>,
}

impl Gate {
    fn fail(&mut self, why: String) {
        self.failures.push(why);
    }

    fn record(&mut self, step: &str, fields: Vec<(&'static str, Json)>) {
        self.steps.push((step.to_owned(), Json::obj(fields)));
    }

    /// Runs `cmd` with its log at `<out>/<name>.log`.
    fn run(&self, name: &str, cmd: &[String], env: &[(&str, String)], minutes: u64, quiet: bool) -> Ran {
        let ran = run(cmd, &self.root, &self.opts.out.join(format!("{name}.log")), env, Duration::from_secs(minutes * 60));
        if !quiet {
            println!("  {name}: {} in {:.0} s", ran.exit.said(), ran.seconds);
        }
        ran
    }

    /// What the step's failed tests said, each message once (a test that fails the same way in
    /// three runs is said once, with the first run it failed in).
    fn heard(&mut self, step: &str, log: &str) {
        for (name, text) in outputs(log, 25) {
            if !self.said.iter().any(|(_, n, t)| *n == name && *t == text) {
                self.said.push((step.to_owned(), name, text));
            }
        }
    }

    /// One run of tests: its log read, what it said kept, and why it is not a pass.
    fn tests(&mut self, label: &str, name: &str, cmd: &[String], env: &[(&str, String)], minutes: u64) -> (Parsed, Ran, Vec<String>) {
        let ran = self.run(name, cmd, env, minutes, false);
        let parsed = parse(&ran.log);
        self.heard(label, &ran.log);
        let why = verdict(&parsed, ran.exit);
        (parsed, ran, why)
    }

    fn cargo(&self, args: &[&str]) -> Vec<String> {
        let mut cmd = vec!["cargo".to_owned(), args[0].to_owned(), "-j".to_owned(), self.opts.jobs.to_string()];
        cmd.extend(args[1..].iter().map(|a| (*a).to_owned()));
        cmd
    }

    fn step_rust(&mut self) {
        println!("rust (the toolchain this run uses)");
        let rust = pin::in_use(&self.root);
        match (rust.pinned, rust.failures.is_empty()) {
            (Some(pin), true) => println!("  {}; the repository pins {}", rust.rustc, pin::dotted(pin)),
            _ => println!("  {}", if rust.rustc.is_empty() { "rustc -V gave no version" } else { &rust.rustc }),
        }
        if let Some(newer) = rust.newer {
            println!("  NOTE: {} is out; moving the pin is a change of its own, through the gate with --all", pin::dotted(newer));
        }
        self.record(
            "rust",
            vec![
                ("rustc", Json::Str(rust.rustc.clone())),
                ("pinned", rust.pinned.map_or(Json::Null, |p| Json::Str(pin::dotted(p)))),
                ("newer_stable", rust.newer.map_or(Json::Null, |n| Json::Str(pin::dotted(n)))),
            ],
        );
        self.failures.extend(rust.failures);
    }

    fn step_gpu(&mut self) {
        println!("gpu (the adapters the tests will see)");
        let ran = self.run("gpu", &self.cargo(&["test", "-p", "specula-gpu", "--test", "adapters", "--", "--nocapture"]), &[], 30, true);
        let line = ran.log.lines().map(str::trim).find(|l| l.starts_with("gate-gpu:")).map(str::to_owned);
        let adapters = line.as_deref().and_then(adapters_in).unwrap_or(0);
        println!("  {}", line.as_deref().map_or("no report (see gpu.log)", |l| l["gate-gpu:".len()..].trim()));
        self.record(
            "gpu",
            vec![("exit", exit_json(ran.exit)), ("adapters", Json::Num(adapters as f64)), ("line", Json::Str(line.clone().unwrap_or_default().chars().take(300).collect()))],
        );
        if !ran.exit.ok() || line.is_none() {
            self.fail("gpu: the adapter report didn't run (see gpu.log)".to_owned());
        } else if adapters == 0 && !self.opts.no_gpu {
            self.fail("gpu: no Vulkan adapter, so every GPU test would skip itself and prove nothing (pass --no-gpu where that is expected)".to_owned());
        }
    }

    fn step_docs(&mut self) {
        println!("docs (the document guards, and this tool's own tests)");
        let (parsed, ran, why) = self.tests("docs", "docs", &self.cargo(&["test", "--no-fail-fast", "-p", "specula-examen"]), &[], 30);
        println!("  {} passed, {} failed", parsed.passed, parsed.failed);
        self.record("docs", parsed_json(&parsed, &ran));
        for w in why {
            self.fail(format!("docs: {w}"));
        }
    }

    fn step_tests(&mut self) {
        println!("tests (the whole workspace, once)");
        let (parsed, ran, why) = self.tests("tests", "tests", &self.cargo(&["test", "--no-fail-fast", "--workspace"]), &[], 150);
        println!("  {} passed, {} failed", parsed.passed, parsed.failed);
        if parsed.drivers_not_loaded > 0 {
            println!("  NOTE: a GPU driver failed to load {} time(s) in this run: those tests ran without that adapter", parsed.drivers_not_loaded);
        }
        self.record("tests", parsed_json(&parsed, &ran));
        for w in why {
            self.fail(format!("tests: {w}"));
        }
    }

    /// The engine's library tests, as the repeat and the slow-worker runs start them.
    fn lib(&self) -> Vec<String> {
        let threads = self.opts.threads.to_string();
        self.cargo(&["test", "-p", &self.opts.package, "--lib", "--", "--test-threads", &threads])
    }

    fn step_repeat(&mut self, runs: &mut Vec<Parsed>) {
        println!("repeat ({}'s library tests, {} more runs at {} test threads)", self.opts.package, self.opts.repeat, self.opts.threads);
        let (mut failed_runs, mut counts) = (0, BTreeSet::new());
        for i in 1..=self.opts.repeat {
            let (parsed, _, why) = self.tests(&format!("repeat run {i}"), &format!("repeat-{i}"), &self.lib(), &[], 45);
            failed_runs += usize::from(!why.is_empty());
            counts.insert(parsed.passed + parsed.failed);
            for w in why {
                self.fail(format!("repeat run {i}: {w}"));
            }
            runs.push(parsed);
        }
        self.record(
            "repeat",
            vec![("runs", Json::Num(self.opts.repeat as f64)), ("failed_runs", Json::Num(failed_runs as f64)), ("tests_each", Json::arr(counts.iter().map(|c| Json::Num(*c as f64))))],
        );
        if counts.len() > 1 {
            self.fail(format!("repeat: the runs disagree on how many tests there are ({counts:?}): a test binary ended early"));
        }
    }

    fn step_slow(&mut self, runs: &mut Vec<Parsed>) {
        println!("slow workers (every worker starts its job {} ms late)", self.opts.slow_workers_ms);
        let env = [("SPECULA_TEST_WORKER_DELAY_MS", self.opts.slow_workers_ms.to_string())];
        let (parsed, ran, why) = self.tests("slow workers", "slow-workers", &self.lib(), &env, 60);
        self.record("slow", parsed_json(&parsed, &ran));
        for w in why {
            self.fail(format!("slow workers: {w}: these depend on when a worker runs"));
        }
        runs.push(parsed);
    }

    fn step_speed(&mut self) {
        let budgets = speed_tests(&self.root);
        let total: usize = budgets.values().map(BTreeSet::len).sum();
        println!("speed ({total} tests that hold a speed budget, one at a time, with their tight bounds)");
        if total == 0 {
            self.fail("speed: found no test that holds a speed budget (has `speed_budget(` been renamed?)".to_owned());
        }
        // How slow the machine is now against its best (`mensura.rs`): the budgets are scaled by
        // it. The best is the least sample this checkout ever took, the gate's start included.
        let mut reference = self.reference.take().unwrap_or_else(|| mensura::Reference::open(mensura::kept_at(&target_dir())));
        let mut packages = Vec::new();
        for (package, names) in &budgets {
            let now = mensura::median(&reference.take());
            let best = reference.best();
            let factor = mensura::factor(now, best);
            let young = reference.youth_on(&mensura::today());
            let load = format!("speed: the machine is {factor:.2} times slower than its best (reference {best:.0} ms, now {now:.0} ms): budgets scaled by {factor:.2}{young}");
            println!("  {package}: {load}");
            let measured = [("best_ms", Json::Num((best * 10.0).round() / 10.0)), ("now_ms", Json::Num((now * 10.0).round() / 10.0)), ("factor", Json::Num(factor)), ("load", Json::Str(load))];
            if mensura::too_loaded(factor) {
                let mut fields = measured.to_vec();
                fields.push(("named", Json::Num(names.len() as f64)));
                packages.push((package.clone(), Json::obj(fields)));
                self.fail(format!(
                    "speed ({package}): the machine is too loaded to judge speed: the reference took {now:.0} ms against its best of {best:.0} ms, {factor:.2} times slower (more than {}); no test was run and none is named a regression",
                    mensura::CAP
                ));
                continue;
            }
            let mut args = vec!["test", "-p", package.as_str(), "--", "--test-threads", "1"];
            args.extend(names.iter().map(String::as_str));
            let env = [("SPECULA_SPEED_TESTS", format!("{factor:.2}"))];
            let (parsed, ran, why) = self.tests(&format!("speed ({package})"), &format!("speed-{package}"), &self.cargo(&args), &env, 45);
            let mut fields = parsed_json(&parsed, &ran);
            fields.extend(measured);
            fields.push(("named", Json::Num(names.len() as f64)));
            packages.push((package.clone(), Json::obj(fields)));
            if why.is_empty() && parsed.passed < names.len() as u64 {
                self.fail(format!("speed ({package}): {} functions hold a budget and {} tests ran: a budget in a helper isn't run here ({names:?})", names.len(), parsed.passed));
            }
            for w in why {
                self.fail(format!("speed ({package}): {w}: over budget running alone, with nothing else of the gate's on the machine, the budgets scaled by {factor:.2} for its load"));
            }
        }
        self.record("speed", vec![("tests", Json::Num(total as f64)), ("packages", Json::Obj(packages))]);
    }

    fn step_clippy(&mut self) {
        println!("clippy (warnings denied)");
        let errors = |log: &str| -> String {
            let all = log.lines().filter(|l| l.starts_with("error")).map(|l| l.trim().chars().take(140).collect::<String>()).collect::<Vec<_>>().join("; ");
            all.chars().take(600).collect()
        };
        let ran = self.run("clippy", &self.cargo(&["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]), &[("RUSTFLAGS", deny().to_owned())], 90, false);
        self.record("clippy", vec![("exit", exit_json(ran.exit))]);
        if !ran.exit.ok() {
            self.fail(format!("clippy: {}", errors(&ran.log)));
        }
        if cfg!(windows) {
            let cmd = self.cargo(&["clippy", "--workspace", "--all-targets", "--target", "x86_64-unknown-linux-gnu", "--", "-D", "warnings"]);
            let ran = self.run("clippy-linux", &cmd, &[("RUSTFLAGS", "-Dwarnings".to_owned())], 90, false);
            self.record("clippy_linux", vec![("exit", exit_json(ran.exit))]);
            if !ran.exit.ok() {
                self.fail(format!("clippy (Linux): {}", errors(&ran.log)));
            }
        }
    }

    fn step_builds(&mut self) {
        println!("builds (debug and release: the live checks run both)");
        for (name, release) in [("build-debug", false), ("build-release", true)] {
            let mut args = vec!["build"];
            if release {
                args.push("--release");
            }
            args.extend(["-p", "specula", "-p", "sepulcra-app"]);
            let ran = self.run(name, &self.cargo(&args), &[], 120, false);
            self.record(name, vec![("exit", exit_json(ran.exit))]);
            if !ran.exit.ok() {
                self.fail(format!("{name}: {} (see its log)", ran.exit.said().to_lowercase()));
            }
        }
    }
}

/// `gate-gpu: N adapter...`: N.
fn adapters_in(line: &str) -> Option<u64> {
    let rest = line.strip_prefix("gate-gpu: ")?;
    let n = rest.bytes().take_while(u8::is_ascii_digit).count();
    rest[n..].starts_with(" adapter").then(|| rest[..n].parse().ok()).flatten()
}

/// The gate. Returns the process's exit code: 0 passed, 1 failed, 2 asked wrongly.
pub fn main(mut args: Args) -> i32 {
    let parsed = (|| -> Result<_, String> {
        let opts = Options {
            jobs: args.number("--jobs", 8)?,
            repeat: args.number("--repeat", 5)?,
            threads: args.number("--threads", 16)?,
            slow_workers_ms: args.number("--slow-workers-ms", 30)?,
            package: args.value("--package")?.unwrap_or_else(|| "specula".to_owned()),
            no_gpu: args.flag("--no-gpu"),
            out: args.value("--out")?.map_or_else(|| target_dir().join("gate"), PathBuf::from),
        };
        let only = args.list("--only")?;
        let all = args.flag("--all");
        let tier = args.value("--tier")?;
        let base = args.value("--base")?;
        args.done()?;
        Ok((opts, only, all, tier, base))
    })();
    let (opts, only, all, tier, base) = match parsed {
        Ok(p) => p,
        Err(e) => {
            println!("{e}");
            return 2;
        }
    };
    let root = root();
    // What is checked out, before anything is measured: a pass is recorded for this commit, and
    // only if it is still the one checked out, unchanged, when the last step has ended.
    let began_at = passed::checked_out(&root);
    // What the change is measured from: the commit named, else the last commit the gate passed
    // at in this checkout (`passed.rs`), else `origin/main`.
    let (base, base_is_ours) = match base {
        Some(named) => (named, false),
        None => (passed::last(&root).unwrap_or_else(|| "origin/main".to_owned()), true),
    };
    let (steps, lines, sizing) = match chosen(only, all, tier, &base, &root) {
        Ok(c) => c,
        Err(e) => {
            println!("{e}");
            return 2;
        }
    };
    // A pass is recorded for a run that chose its own steps from a base nobody named: a base
    // given by hand may leave commits out, and they would then count as passed.
    let by = sizing.get("by").and_then(Json::as_str).unwrap_or("").to_owned();
    let records = by == "all" || (by == "change" && base_is_ours);
    if let Err(e) = std::fs::create_dir_all(&opts.out) {
        println!("cannot make {}: {e}", opts.out.display());
        return 2;
    }
    for line in &lines {
        println!("{line}");
    }
    let has = |s: &str| steps.contains(&s);
    // A set of reference samples before any cargo step, when the machine is at its quietest for
    // this gate: it only lowers the best (`mensura.rs`).
    let reference = has("speed").then(|| {
        let mut r = mensura::Reference::open(mensura::kept_at(&target_dir()));
        let now = mensura::median(&r.take());
        let since = if r.since().is_empty() { String::new() } else { format!(" (kept since {})", r.since()) };
        println!("speed reference: {now:.0} ms now, best {:.0} ms{since}", r.best());
        r
    });
    let mut gate = Gate { opts, root, steps: Vec::new(), failures: Vec::new(), said: Vec::new(), reference };

    gate.step_rust();
    if TEST_STEPS.iter().any(|s| has(s)) {
        gate.step_gpu();
    }
    // (The workspace's tests include this tool's: the docs step is for a run without them.)
    if has("docs") && !has("tests") {
        gate.step_docs();
    }
    if has("tests") {
        gate.step_tests();
    }
    let mut runs = Vec::new();
    if has("repeat") {
        gate.step_repeat(&mut runs);
    }
    if has("slow") {
        gate.step_slow(&mut runs);
    }
    let odd = if runs.len() > 1 { flaky(&runs) } else { BTreeMap::new() };
    if !odd.is_empty() {
        for (test, n) in &odd {
            println!("  FLAKY {test}: failed {n} of {} runs (a bug: find its cause)", runs.len());
        }
        gate.fail(format!("flaky tests (failed in some runs, passed in others): {odd:?}"));
    }
    if has("speed") {
        gate.step_speed();
    }
    if has("clippy") {
        gate.step_clippy();
    }
    if has("builds") {
        gate.step_builds();
    }

    let summary = Json::obj([
        ("schema", Json::Str("specula.gate/v3".to_owned())),
        ("sizing", sizing),
        ("ran", Json::arr(steps.iter().map(|s| Json::Str((*s).to_owned())))),
        ("steps", Json::Obj(gate.steps.clone())),
        ("flaky", Json::Obj(odd.iter().map(|(t, n)| (t.clone(), Json::Num(*n as f64))).collect())),
        ("failures", strings(&gate.failures)),
        (
            "said",
            Json::arr(gate.said.iter().map(|(step, test, output)| Json::obj([("step", Json::Str(step.clone())), ("test", Json::Str(test.clone())), ("output", Json::Str(output.clone()))]))),
        ),
    ]);
    let out = gate.opts.out.clone();
    if let Err(e) = std::fs::write(out.join("summary.json"), summary.to_pretty() + "\n") {
        gate.fail(format!("the summary could not be written: {e}"));
    }
    if gate.failures.is_empty() {
        println!("GATE PASSED: {} (logs: {})", lines.join("; "), out.display());
        if records {
            match passed::record(&gate.root, began_at.as_ref()) {
                Ok(commit) => println!("recorded: the next gate in this checkout is measured from {}", commit.chars().take(8).collect::<String>()),
                Err(why) => println!("not recorded: {why}"),
            }
        }
        return 0;
    }
    println!("GATE FAILED ({}):", gate.failures.len());
    for f in &gate.failures {
        println!("  - {}", f.chars().take(400).collect::<String>());
    }
    if !gate.said.is_empty() {
        println!("what the failed tests said ({} of {}):", gate.said.len().min(12), gate.said.len());
        for (step, test, output) in gate.said.iter().take(12) {
            println!("  [{step}] {test}");
            for line in output.lines() {
                println!("      {line}");
            }
        }
    }
    println!("logs: {}", out.display());
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The speed step finds the tests that hold a budget, not the helper's own definition.
    #[test]
    fn the_speed_step_finds_the_tests_that_hold_a_budget() {
        let text = "fn speed_budget(tight: D) -> D {\n    tight\n}\n\n#[test]\nfn picking_is_quick() {\n    let t = now();\n\
            \x20   assert!(t.elapsed() < speed_budget(ms(20)));\n}\n\n#[test]\nfn other() {\n    // speed_budget(..) in a comment\n    assert!(true);\n}\n";
        let mut found = BTreeSet::new();
        speed_tests_in(text, &mut found);
        assert_eq!(found, BTreeSet::from(["picking_is_quick".to_owned()]));
    }

    #[test]
    fn a_functions_first_line_is_read() {
        assert_eq!(fn_line("    pub(crate) async fn a_b<T>(x: T) {"), Some("a_b"));
        assert_eq!(fn_line("fn main() {"), Some("main"));
        assert_eq!(fn_line("pub fn x (a: u8)"), Some("x"));
        assert_eq!(fn_line("    let f = fn_like(3);"), None);
        assert_eq!(fn_line("pub() fn x()"), None);
        assert_eq!(fn_line("fn (x)"), None);
        assert_eq!(fn_line("// fn commented()"), None);
    }

    #[test]
    fn the_tree_has_tests_that_hold_a_budget_and_each_has_a_package() {
        let found = speed_tests(&root());
        assert!(found.values().map(BTreeSet::len).sum::<usize>() >= 5, "{found:?}");
        assert!(found.contains_key("specula"), "{:?}", found.keys());
    }

    #[test]
    fn the_adapter_report_is_a_count() {
        assert_eq!(adapters_in("gate-gpu: 2 adapter(s): A, B"), Some(2));
        assert_eq!(adapters_in("gate-gpu: none (Vulkan is not available)"), None);
    }

    #[test]
    fn steps_are_chosen_by_the_change_or_by_hand() {
        let root = root();
        let names = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let (steps, lines, _) = chosen(names(&["clippy", "tests"]), false, None, "origin/main", &root).unwrap();
        assert_eq!(steps, ["tests", "clippy"], "in the gate's order");
        assert!(lines[0].contains("--only"));
        assert!(chosen(names(&["test"]), false, None, "origin/main", &root).is_err());
        let (steps, lines, sizing) = chosen(Vec::new(), true, None, "origin/main", &root).unwrap();
        assert_eq!(steps, ["tests", "repeat", "slow", "speed", "clippy", "builds"]);
        assert!(lines.len() == 1 && sizing.get("by").and_then(Json::as_str) == Some("all"), "{lines:?}");
        let (steps, lines, sizing) = chosen(Vec::new(), false, Some("docs".to_owned()), "origin/main", &root).unwrap();
        assert_eq!(steps, ["docs"]);
        assert!(lines[1].starts_with("left out: tests, repeat, slow, speed, clippy, builds"), "{lines:?}");
        assert_eq!(sizing.get("left_out").and_then(Json::as_array).map(<[Json]>::len), Some(6));
        let (steps, _, _) = chosen(Vec::new(), false, Some("tests".to_owned()), "origin/main", &root).unwrap();
        assert_eq!(steps, ["tests", "clippy"]);
        assert!(chosen(Vec::new(), false, Some("everything".to_owned()), "origin/main", &root).is_err());
        // A base git cannot use is everything, never less.
        let (steps, _, _) = chosen(Vec::new(), false, None, "no-such-branch-or-commit-anywhere", &root).unwrap();
        assert_eq!(steps.len(), 6);
    }
}

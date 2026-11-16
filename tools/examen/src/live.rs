//! Runs every live check, one after the other, and says which passed: the final audit of a
//! milestone (AGENTS.md, "Live checks": CI has no GPU and no desktop, so these are run by hand).
//!
//! ```text
//! cargo run -p specula-examen -- live [--out DIR] [--port 7720] [--only NAME,NAME] [--skip NAME,NAME] [--list] [--plan] [--no-build]
//! ```
//!
//! Each check starts its own engines; this runner only runs them, gives each its own ports and
//! its own output folder, keeps its log, and reports exit codes. A check is run once: one that
//! fails here is a finding, never retried into passing. (The checks themselves are still the
//! scripts of `scripts/`, until each is ported: ROADMAP M6, "The repository's own tooling in
//! Rust".)
//!
//! It builds the engine and the game first, debug and release, in this checkout: an executable
//! keeps the folder it was built from (a game finds its assets there), so one left in a target
//! folder by another checkout runs with that checkout's files, or with none (M5's final audit
//! ran 17 checks into failures that way before it built its own).
//!
//! Every check is told its ports: the eight from its own first port on. A check that names its
//! ports another way than `--port` has its flags in `RUNS`; one this runner has no port flag for
//! stops the run before anything starts.
//!
//! On Windows `--out` has a depth limit, said before anything is started: the checks build
//! modules in projects of their own below it, rustc hands a module's linker its arguments in a
//! file deep in that build, and the toolchain's linker opens a path of at most 259 characters.
//!
//! A check leaves the checkout as it found it: after each one this runner asks git what differs
//! from the last commit, and a check that left something new there (a changed file, a file git
//! does not ignore, a further change to a file that was already changed) fails, with the paths.
//! Not seen: a further change to a file git does not track; run the audit in a clean checkout.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use specula_core::Json;

use crate::cli::Args;
use crate::run::{root, run, said, target_dir};
use crate::testlog::Exit;

/// Scripts that are not live checks. Everything else in `scripts/` is run.
const NOT_CHECKS: &[&str] = &[
    // Helpers other scripts import, the visual history's tool, and the measured comparison's
    // harness (it runs agents; its own tests are in UNIT).
    "specula_mcp.py",
    "window_recorder.py",
    "playtest_swarm.py",
    "gallery.py",
    "leverage.py",
    // Measurements: they print figures to compare between two builds and pass no verdict
    // (perf/README.md says how each is read).
    "perf_baseline.py",
    "call_rate.py",
    "stall_load.py",
    "capture_drill.py",
    // It draws on an engine someone already has open (the interactive one's port by default).
    "lava_lamps.py",
    // It judges a file of results that the exhaustive scan of dmath writes (a test run by hand,
    // hours long): a verdict, but not of anything this runner can start.
    "dmath_oracle.py",
    // A milestone's proof of its day, kept as the record of it: M4's swarm built 33 props into
    // a map that had none, and the map has shipped them since M5 (its run is refused at the
    // first spawn; the approved world is in target/m4-exit-approved).
    "m4_exit.py",
];

/// What a check needs beyond its defaults (each list is one run of the script). `{port}` is the
/// check's first port, `{port1}` and `{port3}` the next and the third after it, `{out}` its
/// output folder, `{perf}` a perf.toml this runner writes whose lease pool is the check's other
/// ports.
const RUNS: &[(&str, &[&[&str]])] = &[
    ("evals.py", &[&["--check"], &["--reference", "--workers", "3", "--lead-port", "{port}", "--pool", "{port1}-{port3}"]]),
    // (Its own header: "The milestone audit runs --known-ok": the stalls a later lane owns,
    // each within its listed peak, are named and do not fail the audit.)
    ("budget_check.py", &[&["--known-ok"]]),
    ("desync_check.py", &[&["--ports", "{port}", "{port1}"]]),
    ("save_check.py", &[&["--ports", "{port}", "{port1}"]]),
    // (Three workers, not its eight: each leases two ports, one to record and one to verify,
    // and the check's own eight are its lead's and seven for the pool. Its dated results go to
    // the audit's folder, not into perf/playtests.)
    ("playtest_suite.py", &[&["--lead-port", "{port}", "--workers", "3", "--perf", "{perf}", "--results", "{out}"]]),
    ("toolscope_check.py", &[&["--results", "{out}"]]),
    ("balance_playtests.py", &[&["--lead-port", "{port}", "--pool", "{port1}-{port3}"]]),
    // (Its sheet is a tracked picture in docs/fidelity: the audit's goes to the audit's folder.)
    ("fidelity_t3.py", &[&["--sheet", "{out}"]]),
];
/// Checks whose `--out` is one JSON file, not a folder.
const OUT_IS_A_FILE: &[&str] = &["balance_check.py", "desync_check.py", "save_check.py"];
/// The flags a check names its ports with.
const PORT_FLAGS: &[&str] = &["--port", "--ports", "--lead-port"];
/// Unit tests of the scripts themselves, run first.
const UNIT: &[&str] = &["evals_test.py", "leverage_test.py"];
/// What is built before anything runs (debug and release of the first; the example is a
/// check's own engine, scripts/model_stress.py).
const BUILDS: &[&[&str]] = &[
    &["cargo", "build", "-p", "specula", "-p", "sepulcra-app"],
    &["cargo", "build", "-p", "specula", "-p", "sepulcra-app", "--release"],
    &["cargo", "build", "-p", "specula", "--example", "model_stress"],
];
const LIMIT: Duration = Duration::from_secs(2400);
/// Ports a check owns, from its first.
const SPAN: u64 = 8;

/// The deepest file a check makes below `--out`: the one rustc hands a module's linker its
/// arguments in, in project_check's project (the names' lengths are what matter; the layout is
/// Cargo's since Rust 1.100, the deeper of the two).
const DEEPEST_BELOW_OUT: &str = r"\project_check\20261115-132118\ws\.specula-cache\modules\ambulo-8c5d3caf\debug\build\ambulo\0123456789abcdef\out\rustcXXXXXX\linker-arguments";
/// What the Rust toolchain's linker opens on Windows (`specula_logic::build::LINKER_PATH_MAX`).
const LINKER_PATH_MAX: usize = 259;

/// By how many characters `out` is too deep for the checks' module builds (0: it is not).
fn too_deep(out: &str, windows: bool) -> usize {
    if windows {
        (out.chars().count() + DEEPEST_BELOW_OUT.chars().count()).saturating_sub(LINKER_PATH_MAX)
    } else {
        0
    }
}

fn stem(script: &str) -> &str {
    script.strip_suffix(".py").unwrap_or(script)
}

/// One run of a check.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Planned {
    script: String,
    port: u64,
    /// Which of the script's runs this is (0 for the first).
    k: usize,
    /// The arguments after the script's path.
    args: Vec<String>,
    needs_ports: bool,
    /// The script has more runs than one.
    several: bool,
}

/// The runs of `script` (`source` is its text), on `port`, with `out` as its output folder.
fn runs_of(script: &str, source: &str, port: u64, out: &Path) -> Vec<Planned> {
    let fill = |x: &str| -> String {
        let perf = out.parent().unwrap_or(out).join(format!("{}.perf.toml", stem(script)));
        x.replace("{port1}", &(port + 1).to_string())
            .replace("{port3}", &(port + 3).to_string())
            .replace("{port}", &port.to_string())
            .replace("{out}", &out.to_string_lossy())
            .replace("{perf}", &perf.to_string_lossy())
    };
    let runs: &[&[&str]] = RUNS.iter().find(|(name, _)| *name == script).map_or(&[&[]], |(_, runs)| runs);
    runs.iter()
        .enumerate()
        .map(|(k, extra)| {
            let mut args: Vec<String> = extra.iter().map(|x| fill(x)).collect();
            let offline = UNIT.contains(&script) || *extra == ["--check"];
            if !offline {
                if !args.iter().any(|a| PORT_FLAGS.contains(&a.as_str())) && source.contains("\"--port\"") {
                    args.extend(["--port".to_owned(), port.to_string()]);
                }
                if source.contains("\"--out\"") {
                    let file = if OUT_IS_A_FILE.contains(&script) { ".json" } else { "" };
                    args.extend(["--out".to_owned(), format!("{}{file}", out.to_string_lossy())]);
                }
            }
            Planned { script: script.to_owned(), port, k, args, needs_ports: !offline, several: runs.len() > 1 }
        })
        .collect()
}

impl Planned {
    fn label(&self) -> String {
        if self.k > 0 || self.several {
            format!("{} ({})", stem(&self.script), self.args.iter().take(2).cloned().collect::<Vec<_>>().join(" "))
        } else {
            stem(&self.script).to_owned()
        }
    }

    fn log_name(&self) -> String {
        if self.k > 0 {
            format!("{}-{}.txt", stem(&self.script), self.k)
        } else {
            format!("{}.txt", stem(&self.script))
        }
    }
}

/// The line of a tree's state that stands for the content of its changes.
const CHANGES: &str = "changes to tracked files: ";

/// What git says differs from the last commit in this checkout: a line a path (every file of a
/// folder git does not track, not the folder alone), and one line for the changes themselves,
/// so that a second change to a file is a difference too. `None` when git cannot say (no git,
/// or not a checkout).
fn tree_state(root: &Path) -> Option<BTreeSet<String>> {
    let limit = Duration::from_secs(300);
    let (0, status) = said(&["git", "status", "--porcelain", "-uall"], root, limit)? else { return None };
    let (0, diff) = said(&["git", "diff", "HEAD"], root, limit)? else { return None };
    let mut state: BTreeSet<String> = String::from_utf8_lossy(&status).lines().filter(|l| !l.trim().is_empty()).map(str::to_owned).collect();
    state.insert(format!("{CHANGES}{:016x}", specula_core::hash::hash_bytes(&diff)));
    Some(state)
}

/// What a check left in the checkout: the lines of `after` that `before` did not have. A file
/// changed for the first time is named by its own line; the line for the changes is kept only
/// when no path says it (a file that was already changed, changed again).
fn left_behind(before: Option<&BTreeSet<String>>, after: Option<&BTreeSet<String>>) -> Vec<String> {
    let (Some(before), Some(after)) = (before, after) else { return Vec::new() };
    let new: Vec<&String> = after.difference(before).collect();
    let paths: Vec<String> = new.iter().filter(|x| !x.starts_with(CHANGES)).map(|x| (*x).clone()).collect();
    if !paths.is_empty() {
        paths
    } else if !new.is_empty() {
        vec!["a file that was already changed was changed again (git diff HEAD)".to_owned()]
    } else {
        Vec::new()
    }
}

/// The Python that runs the checks which are still scripts (a machine may answer `python` with
/// a stub that is not Python: it is asked for its version).
fn python() -> Option<&'static str> {
    ["python", "python3"].into_iter().find(|p| Command::new(p).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success()))
}

/// Every check, in the order they run: the scripts of `scripts/` (the unit tests of the scripts
/// first, then every check, sorted), then the checks written in Rust ([`crate::native::CHECKS`]).
/// The native ones come last so that a script keeps the ports it had before any was ported.
fn everything(scripts: &Path) -> Result<Vec<String>, String> {
    let mut checks: Vec<String> = std::fs::read_dir(scripts)
        .map_err(|e| format!("{}: {e}", scripts.display()))?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".py") && !NOT_CHECKS.contains(&n.as_str()) && !UNIT.contains(&n.as_str()))
        .collect();
    checks.sort();
    Ok(UNIT.iter().map(|u| (*u).to_owned()).chain(checks).chain(crate::native::CHECKS.iter().map(|c| c.name.to_owned())).collect())
}

/// The plan: every run of every chosen check, each check on the ports of its place in the whole
/// list, so that a run of a few uses the same ports as the full run.
fn plan(scripts: &Path, all: &[String], names: &[String], first_port: u64, out: &Path) -> Result<Vec<Planned>, String> {
    let mut plan = Vec::new();
    for name in names {
        let place = all.iter().position(|n| n == name).expect("a chosen check is one of the checks") as u64;
        // A check written in Rust runs in this process, on its own ports: it is told them.
        if crate::native::find(name).is_some() {
            plan.push(Planned { script: name.clone(), port: first_port + SPAN * place, k: 0, args: Vec::new(), needs_ports: false, several: false });
            continue;
        }
        let source = String::from_utf8_lossy(&std::fs::read(scripts.join(name)).map_err(|e| format!("scripts/{name}: {e}"))?).into_owned();
        for run in runs_of(name, &source, first_port + SPAN * place, &out.join(stem(name))) {
            if run.needs_ports && !run.args.iter().any(|a| PORT_FLAGS.contains(&a.as_str())) {
                return Err(format!("{name} takes no --port and RUNS names no port flag for it: it would run on its default ports. Nothing was run."));
            }
            plan.push(run);
        }
    }
    Ok(plan)
}

/// The runner. Returns the process's exit code: 0 when every check that ran passed.
pub fn main(mut args: Args) -> i32 {
    let parsed = (|| -> Result<_, String> {
        let out = args.value("--out")?.map_or_else(|| target_dir().join("live-checks"), PathBuf::from);
        let port = args.number("--port", 7720)?;
        let (only, skip) = (args.list("--only")?, args.list("--skip")?);
        let (list, show_plan, no_build) = (args.flag("--list"), args.flag("--plan"), args.flag("--no-build"));
        args.done()?;
        Ok((out, port, only, skip, list, show_plan, no_build))
    })();
    let (out, port, only, skip, list, show_plan, no_build) = match parsed {
        Ok(p) => p,
        Err(e) => {
            println!("{e}");
            return 2;
        }
    };
    let root = root();
    let scripts = root.join("scripts");
    let all = match everything(&scripts) {
        Ok(all) => all,
        Err(e) => {
            println!("{e}");
            return 2;
        }
    };
    let chosen = |n: &String, list: &[String]| list.iter().any(|x| x == n || x == stem(n));
    let names: Vec<String> = all.iter().filter(|n| (only.is_empty() || chosen(n, &only)) && !chosen(n, &skip)).cloned().collect();
    if list {
        println!("{}", names.join("\n"));
        return 0;
    }
    let out = std::path::absolute(&out).unwrap_or(out);
    let over = too_deep(&out.to_string_lossy(), cfg!(windows));
    if over > 0 {
        println!(
            "--out is {over} character(s) too deep ({}: {}). The checks build modules in projects below it, and the linker's argument file of the deepest would be past the {LINKER_PATH_MAX} characters the Rust toolchain's linker opens on Windows. Name a shorter folder (one near the drive's root will do). Nothing was run.",
            out.to_string_lossy().chars().count(),
            out.display()
        );
        return 2;
    }
    let plan = match plan(&scripts, &all, &names, port, &out) {
        Ok(plan) => plan,
        Err(e) => {
            println!("{e}");
            return 2;
        }
    };
    if show_plan {
        for run in &plan {
            println!("{} {} {}", run.port, run.script, run.args.join(" "));
        }
        return 0;
    }
    // (Asked for only when a check that is still a script is to run.)
    let scripted = plan.iter().any(|run| crate::native::find(&run.script).is_none());
    let python = if scripted { python() } else { Some("") };
    let Some(python) = python else {
        println!("no Python on this machine: the checks that are still scripts cannot run (--only with the checks written in Rust runs those: --list names them all). Nothing was run.");
        return 2;
    };
    if let Err(e) = std::fs::create_dir_all(&out) {
        println!("cannot make {}: {e}", out.display());
        return 2;
    }
    if !no_build {
        for cmd in BUILDS {
            println!("building: {}", cmd.join(" "));
            let built = Command::new(cmd[0]).args(&cmd[1..]).current_dir(&root).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output();
            if !built.as_ref().is_ok_and(|b| b.status.success()) {
                let said = built.map_or_else(|e| e.to_string(), |b| String::from_utf8_lossy(&b.stderr).into_owned());
                let tail: String = said.chars().rev().take(2000).collect::<Vec<_>>().into_iter().rev().collect();
                println!("{tail}");
                println!("the build failed: nothing was checked");
                return 2;
            }
        }
    }
    let mut results = Vec::new();
    let mut failed = Vec::new();
    let mut tree = tree_state(&root);
    if tree.is_none() {
        println!("git does not answer here: what a check leaves in the checkout is not looked at");
    }
    for run_ in &plan {
        if run_.args.iter().any(|a| a.contains("perf.toml")) {
            let text = format!("[swarm]\nlease_ports = [{}, {}]\n", run_.port + 1, run_.port + SPAN - 1);
            if let Err(e) = std::fs::write(out.join(format!("{}.perf.toml", stem(&run_.script))), text) {
                println!("cannot write the check's perf.toml: {e}");
                return 2;
            }
        }
        let label = run_.label();
        let log = out.join(run_.log_name());
        let ran = match crate::native::find(&run_.script) {
            // A check written in Rust: in this process, in its own folder, on its own ports.
            Some(check) => {
                let started = std::time::Instant::now();
                let (passed, _) = crate::native::run(check, &out.join(&run_.script), run_.port, &log);
                let text = std::fs::read(&log).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default();
                crate::run::Ran { exit: Exit::Code(i32::from(!passed)), log: text, seconds: started.elapsed().as_secs_f64() }
            }
            None => {
                let mut cmd = vec![python.to_owned(), "-u".to_owned(), scripts.join(&run_.script).to_string_lossy().into_owned()];
                cmd.extend(run_.args.iter().cloned());
                run(&cmd, &root, &log, &[], LIMIT)
            }
        };
        let mut last: String = ran.log.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("").chars().take(140).collect();
        let now = tree_state(&root);
        let left = left_behind(tree.as_ref(), now.as_ref());
        if now.is_some() {
            tree = now;
        }
        if !left.is_empty() {
            last = format!("left in the checkout: {}", left.join(", ").chars().take(110).collect::<String>());
        }
        let passed = ran.exit.ok() && left.is_empty();
        let exit = match ran.exit {
            Exit::Code(c) => c.to_string(),
            Exit::TimedOut => "timeout".to_owned(),
        };
        println!("{} {label:44} exit {exit:8} {:6.0} s  {last}", if passed { "ok  " } else { "FAIL" }, ran.seconds);
        if !passed {
            let left_said = if left.is_empty() { String::new() } else { format!("; left in the checkout: {}", left.join(", ")) };
            failed.push((label.clone(), format!("FAILED: {label} (exit {exit}; {}){left_said}", log.display())));
        }
        results.push(Json::obj([
            ("check", Json::Str(label)),
            ("exit", match ran.exit {
                Exit::Code(c) => Json::Num(f64::from(c)),
                Exit::TimedOut => Json::Str("timeout".to_owned()),
            }),
            ("seconds", Json::Num(ran.seconds.round())),
            ("ports", Json::arr([Json::Num(run_.port as f64), Json::Num((run_.port + SPAN - 1) as f64)])),
            ("log", Json::Str(log.to_string_lossy().into_owned())),
            ("last", Json::Str(last)),
            ("left", Json::arr(left.into_iter().map(Json::Str))),
        ]));
    }
    let total = results.len();
    let report = Json::obj([("results", Json::Arr(results)), ("failed", Json::arr(failed.iter().map(|(label, _)| Json::Str(label.clone()))))]);
    let report_path = out.join("report.json");
    if let Err(e) = std::fs::write(&report_path, report.to_pretty()) {
        println!("the report could not be written: {e}");
        return 2;
    }
    println!("\n{} of {total} passed; report: {}", total - failed.len(), report_path.display());
    for (_, line) in &failed {
        println!("{line}");
    }
    i32::from(!failed.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(lines: &[&str]) -> BTreeSet<String> {
        lines.iter().map(|l| (*l).to_owned()).collect()
    }

    #[test]
    fn an_output_folder_too_deep_for_a_modules_link_is_refused() {
        assert_eq!(DEEPEST_BELOW_OUT.chars().count(), 141);
        assert_eq!(too_deep(&"x".repeat(118), true), 0, "the audits that passed");
        assert_eq!(too_deep(&"x".repeat(119), true), 1, "the audit whose project_check failed at its link");
        assert_eq!(too_deep(&"x".repeat(400), false), 0, "no such limit off Windows");
    }

    #[test]
    fn what_a_check_left_behind_is_named() {
        let a = set(&[" M x.rs", "changes to tracked files: aaaa"]);
        assert!(left_behind(Some(&a), Some(&a.clone())).is_empty(), "nothing new");
        let b = set(&[" M x.rs", " M y.rs", "changes to tracked files: bbbb"]);
        assert_eq!(left_behind(Some(&a), Some(&b)), [" M y.rs"], "a file changed for the first time is named");
        let (c, d) = (set(&["changes to tracked files: 0"]), set(&["?? new/file.txt", "changes to tracked files: 0"]));
        assert_eq!(left_behind(Some(&c), Some(&d)), ["?? new/file.txt"], "a file git does not track");
        let again = set(&[" M x.rs", "changes to tracked files: bbbb"]);
        assert_eq!(left_behind(Some(&a), Some(&again)), ["a file that was already changed was changed again (git diff HEAD)"]);
        assert!(left_behind(None, Some(&a)).is_empty() && left_behind(Some(&a), None).is_empty(), "git gave no answer");
        if let Some(state) = tree_state(&root()) {
            assert_eq!(state.iter().filter(|l| l.starts_with(CHANGES)).count(), 1, "{state:?}");
        }
    }

    #[test]
    fn a_check_is_told_its_ports_and_its_folder() {
        let out = Path::new("/audit/a_check");
        let plain = runs_of("a_check.py", "ap.add_argument(\"--port\")\nap.add_argument(\"--out\")\n", 7728, out);
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].args, ["--port", "7728", "--out", &out.to_string_lossy()]);
        assert!(plain[0].needs_ports && plain[0].label() == "a_check" && plain[0].log_name() == "a_check.txt");
        // A check with no --port and no entry in RUNS has no port flag: the plan refuses it.
        let none = runs_of("b_check.py", "", 7728, out);
        assert!(none[0].needs_ports && none[0].args.is_empty());
        // A check whose --out is one file.
        let file = runs_of("save_check.py", "\"--ports\" \"--out\"", 7736, Path::new("/audit/save_check"));
        assert_eq!(file[0].args[..3], ["--ports", "7736", "7737"]);
        assert!(file[0].args[4].ends_with("save_check.json"), "{:?}", file[0].args);
        // Two runs: the first is offline and is told nothing; the second names its own ports.
        let evals = runs_of("evals.py", "\"--port\" \"--out\"", 7744, Path::new("/audit/evals"));
        assert_eq!(evals[0].args, ["--check"]);
        assert!(!evals[0].needs_ports && evals[0].label() == "evals (--check)");
        assert_eq!(evals[1].args[..7], ["--reference", "--workers", "3", "--lead-port", "7744", "--pool", "7745-7747"]);
        assert!(evals[1].log_name() == "evals-1.txt" && evals[1].label() == "evals (--reference --workers)");
        // The perf.toml this runner writes for a check is beside the check's folder.
        let suite = runs_of("playtest_suite.py", "", 7752, Path::new("/audit/playtest_suite"));
        let perf = &suite[0].args[5];
        assert!(perf.ends_with("playtest_suite.perf.toml") && !perf.contains("playtest_suite/playtest_suite") && !perf.contains("playtest_suite\\playtest_suite"), "{perf}");
        // A unit test of the scripts is told nothing.
        assert!(runs_of("evals_test.py", "\"--port\" \"--out\"", 7720, out)[0].args.is_empty());
    }

    #[test]
    fn every_check_of_the_folder_has_a_plan_on_its_own_ports() {
        let scripts = root().join("scripts");
        let all = everything(&scripts).expect("scripts/");
        assert_eq!(all[..2], ["evals_test.py", "leverage_test.py"], "the scripts' own tests run first");
        // The scripts, sorted; then the checks written in Rust, in their own order, so that a
        // script keeps the ports it had before any check was ported.
        let native = crate::native::CHECKS.len();
        let scripted = &all[..all.len() - native];
        assert!(scripted.len() >= 60 && scripted.windows(2).skip(2).all(|w| w[0] < w[1]), "{} scripts, sorted", scripted.len());
        assert_eq!(all[all.len() - native..], crate::native::CHECKS.iter().map(|c| c.name.to_owned()).collect::<Vec<_>>());
        let own = super::plan(&scripts, &all, &all[all.len() - native..], 7720, Path::new("/a")).unwrap();
        assert!(own.len() == native && own.iter().all(|p| p.args.is_empty() && !p.needs_ports && p.port >= 7720 + SPAN * scripted.len() as u64), "{own:?}");
        let plan = plan(&scripts, &all, &all, 7720, Path::new("/a")).expect("every check names its ports");
        assert!(plan.len() >= all.len());
        // No two checks share a port, and a run of one check uses the full run's ports.
        let mut ports: Vec<u64> = plan.iter().map(|p| p.port).collect();
        ports.dedup();
        assert_eq!(ports.len(), all.len());
        assert!(ports.windows(2).all(|w| w[1] == w[0] + SPAN), "{ports:?}");
        let one = &all[all.len() / 2];
        let alone = super::plan(&scripts, &all, std::slice::from_ref(one), 7720, Path::new("/a")).unwrap();
        assert_eq!(alone, plan.iter().filter(|p| p.script == *one).cloned().collect::<Vec<_>>());
        // Every script RUNS and the other lists name is there.
        for name in RUNS.iter().map(|(n, _)| *n).chain(OUT_IS_A_FILE.iter().copied()).chain(NOT_CHECKS.iter().copied()).chain(UNIT.iter().copied()) {
            assert!(scripts.join(name).is_file(), "scripts/{name} is named in live.rs and is not there");
        }
    }
}

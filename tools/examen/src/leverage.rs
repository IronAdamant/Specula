//! The measured comparison's harness (docs/design/LEVERAGE.md): the same game jobs built
//! through the engine and without it, by the same model on the same budget, checked against
//! one list by someone who built neither.
//!
//! This is the harness's plain part: it validates the data, makes a run's folder and its
//! brief, starts and stops the engine arm's engine, logs every call made through it, collects
//! what a run left, packs what the judge gets, and writes the report. Where an agent has a
//! command line that takes a prompt and runs to its end, `run` starts it as a process of its
//! own in the run's folder (settings.json's `runner`), stops it at the time limit, and records
//! what it used. Otherwise whoever runs the comparison hands each brief to their own agent and
//! tells `record` what that agent's harness reported.
//!
//! ```text
//! examen leverage --check
//! examen leverage prepare JOB ARM N --out ROOT [--exe SPECULA_EXE] [--port P] [--from RUN] [--build NAME]
//! examen leverage engine start|stop RUN
//! examen leverage call RUN TOOL ['{json}' | @FILE]
//! examen leverage tools RUN [FILTER]
//! examen leverage run RUN [--judge [--again WHY]] [--note TEXT]
//! examen leverage finish RUN
//! examen leverage record RUN --tokens N --tool-uses N --duration-s N [--usage TRANSCRIPT] [--by-kind I,W,R,O] [--ended done|wall|stopped] [--note TEXT] [--judge]
//! examen leverage usage TRANSCRIPT
//! examen leverage judge RUN [--verdict FILE [--again WHY]]
//! examen leverage report ROOT
//! examen leverage export ROOT DEST
//! examen leverage where RUN [RUN ...] [--json]
//! examen leverage trace RUN [RUN ...] [--judge] [--json]
//! ```
//!
//! A run's folder (`ROOT/<job>-<arm>-<n>/`) holds what the builder and the judge see:
//! `BRIEF.md`, `work/` (the game), and for the engine arm `engine/` (its settings and trace).
//! What a run cost is kept apart, in `ROOT/costs/`, so the judge never reads it.
//!
//! It was `scripts/leverage.py` through M5 (owner, 2026-11-16: the only language here is
//! Rust). What changed with the port, and only this: **the engine arm's work folder is made by
//! `specula new`** (docs/design/FIRST_HOUR.md, slice 7), where the script copied the SDK into
//! an empty project by hand. Runs the script made are read as they are.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use specula_core::Json;

use crate::run::stop_tree;

pub const ARMS: [&str; 2] = ["engine", "plain"];
pub const KINDS: [&str; 2] = ["behaviour", "checkable"];
/// What a run's size counts leave out: build output, the engine's own files, the SDK copy.
const SKIP_DIRS: [&str; 9] = ["target", "specula-sdk", "specula-core", ".specula-cache", "captures", "worlds", "saves", "__pycache__", ".git"];
const CODE: [&str; 12] = ["rs", "py", "js", "ts", "html", "css", "json", "toml", "lex", "md", "txt", "glsl"];
/// A run's tokens by kind: sent and not cached, written to the cache (new in the context),
/// read again from the cache (the context so far, re-read at every request), written out.
pub const TOKEN_KINDS: [&str; 4] = ["input", "cache_write", "cache_read", "output"];
/// The names a request's usage record gives them (Claude Code's).
const USAGE_NAMES: [&str; 4] = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens", "output_tokens"];
/// Words a brief may not hold: it is one text for both arms.
const BRIEF_NAMES_NO: [&str; 5] = ["Specula", "engine", "Python", "Rust", "cargo"];

/// What was asked cannot be done; the text says why.
pub type R<T> = Result<T, String>;

// ---- small things -------------------------------------------------------------------------

pub fn repo() -> PathBuf {
    crate::run::root()
}

fn data() -> PathBuf {
    repo().join("evals").join("leverage")
}

/// A path with `/`, as a brief names it.
pub fn posix(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn load_json(path: &Path) -> R<Json> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", posix(path)))?;
    Json::parse(text.trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {}", posix(path), e.message))
}

fn write_json(path: &Path, json: &Json) -> R<()> {
    std::fs::write(path, json.to_pretty() + "\n").map_err(|e| format!("{}: {e}", posix(path)))
}

pub fn text<'j>(j: &'j Json, key: &str) -> Option<&'j str> {
    j.get(key).and_then(Json::as_str)
}

pub fn num(j: &Json, key: &str) -> Option<f64> {
    j.get(key).and_then(Json::as_f64)
}

/// A whole number under `key`.
pub fn whole(j: &Json, key: &str) -> Option<i64> {
    num(j, key).filter(|n| n.fract() == 0.0 && n.abs() < 9.0e15).map(|n| n as i64)
}

pub fn truthy(j: Option<&Json>) -> bool {
    match j {
        None | Some(Json::Null) | Some(Json::Bool(false)) => false,
        Some(Json::Num(n)) => *n != 0.0,
        Some(Json::Str(s)) => !s.is_empty(),
        Some(Json::Arr(a)) => !a.is_empty(),
        Some(Json::Obj(o)) => !o.is_empty(),
        Some(Json::Bool(true)) => true,
    }
}

/// Takes `key` out of an object.
pub fn take(j: &mut Json, key: &str) -> Option<Json> {
    let Json::Obj(pairs) = j else { return None };
    let at = pairs.iter().position(|(k, _)| k == key)?;
    Some(pairs.remove(at).1)
}

/// A whole number with a comma every three digits.
pub fn commas(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// A number as the report shows it: `-` for none, commas in a whole one.
pub fn fmt(n: Option<f64>) -> String {
    match n {
        None => "-".to_owned(),
        Some(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => commas(n as i64),
        Some(n) => format!("{n}"),
    }
}

/// The time now as `2026-11-16T14:03:05Z` (UTC: the standard library knows no time zone).
fn now_text() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to a date (the civil calendar, by era of 400 years).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

fn epoch_s() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

// ---- data -----------------------------------------------------------------------------------

pub fn settings() -> R<Json> {
    let s = load_json(&data().join("settings.json"))?;
    let problems = check_settings(&s);
    if problems.is_empty() {
        Ok(s)
    } else {
        Err(format!("settings.json: {}", problems.join("; ")))
    }
}

pub fn check_settings(s: &Json) -> Vec<String> {
    let mut out = Vec::new();
    if whole(s, "leverage") != Some(1) {
        out.push("leverage must be 1 (the format's version)".to_owned());
    }
    if !whole(s, "runs_per_cell").is_some_and(|n| (1..=20).contains(&n)) {
        out.push("runs_per_cell must be a whole number from 1 to 20".to_owned());
    }
    for k in ["builder_model", "judge_model"] {
        if !text(s, k).is_some_and(|m| !m.is_empty()) {
            out.push(format!("{k} must name a model"));
        }
    }
    let arms: Option<Vec<&str>> = s.get("arms").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_str).collect());
    if arms.as_deref() != Some(&ARMS[..]) {
        out.push(format!("arms must be {ARMS:?}: the comparison is between the two"));
    }
    let jobs: &[(String, Json)] = match s.get("jobs").and_then(Json::as_object) {
        Some(jobs) if !jobs.is_empty() => jobs,
        _ => {
            out.push("jobs must map a job's id to its limits".to_owned());
            &[]
        }
    };
    let null = Json::Null;
    let judge = ("judge".to_owned(), s.get("judge").cloned().unwrap_or(null));
    for (name, lim) in jobs.iter().chain([&judge]) {
        if lim.as_object().is_none() {
            out.push(format!("{name}: limits must be an object"));
            continue;
        }
        for (k, least) in [("token_cap", 10_000), ("wall_s", 60)] {
            if !whole(lim, k).is_some_and(|n| n >= least) {
                out.push(format!("{name}.{k} must be a whole number, at least {}", commas(least)));
            }
        }
    }
    let ports: Vec<i64> = s.get("ports").and_then(Json::as_array).map(|p| p.iter().filter_map(|x| x.as_f64().filter(|n| n.fract() == 0.0).map(|n| n as i64)).collect()).unwrap_or_default();
    if !(s.get("ports").and_then(Json::as_array).is_some_and(|p| p.len() == 2) && ports.len() == 2 && 1024 <= ports[0] && ports[0] <= ports[1] && ports[1] <= 65_535) {
        out.push("ports must be [first, last]".to_owned());
    }
    let weights = s.get("token_weights").and_then(Json::as_object);
    let good = weights.is_some_and(|w| w.len() == TOKEN_KINDS.len() && TOKEN_KINDS.iter().all(|k| w.iter().any(|(name, v)| name == k && v.as_f64().is_some_and(|x| x >= 0.0))));
    if !good {
        out.push(format!("token_weights must give a number for each of {TOKEN_KINDS:?}"));
    }
    let command: Option<Vec<&str>> = s.get("runner").and_then(|r| r.get("command")).and_then(Json::as_array).and_then(|c| c.iter().map(Json::as_str).collect());
    match command {
        Some(c) if c.contains(&"{prompt}") && c.contains(&"{model}") && c.len() >= 2 => {}
        _ => out.push("runner.command must be a command line as a list, with {prompt} and {model} where they go".to_owned()),
    }
    let rules: Option<Vec<&str>> = s.get("rules").and_then(Json::as_array).and_then(|r| r.iter().map(Json::as_str).collect());
    match rules {
        None => out.push("rules must be a list of lines (what every builder is told about the machine)".to_owned()),
        Some(rules) => {
            for must in ["Download nothing", "127.0.0.1", "pointer"] {
                if !rules.iter().any(|r| r.contains(must)) {
                    out.push(format!("rules must tell a builder what a person's machine needs of it (a line with '{must}')"));
                }
            }
        }
    }
    out
}

pub fn job(name: &str) -> R<Json> {
    let j = load_json(&data().join("jobs").join(format!("{name}.json")))?;
    let problems = check_job(&j, name);
    if problems.is_empty() {
        Ok(j)
    } else {
        Err(format!("jobs/{name}.json: {}", problems.join("; ")))
    }
}

fn echo(j: &Json) -> String {
    j.to_string().chars().take(60).collect()
}

pub fn check_job(j: &Json, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    if whole(j, "leverage") != Some(1) {
        out.push("leverage must be 1".to_owned());
    }
    if text(j, "id") != Some(name) {
        out.push(format!("id must be '{name}' (the file's name)"));
    }
    if !text(j, "title").is_some_and(|t| !t.is_empty()) {
        out.push("title is missing".to_owned());
    }
    let brief: Option<Vec<&str>> = j.get("brief").and_then(Json::as_array).filter(|b| !b.is_empty()).and_then(|b| b.iter().map(Json::as_str).collect());
    if brief.is_none() {
        out.push("brief must be a list of lines".to_owned());
    }
    let acc: &[Json] = match j.get("acceptance").and_then(Json::as_array) {
        Some(acc) if !acc.is_empty() => acc,
        _ => {
            out.push("acceptance must be a list".to_owned());
            &[]
        }
    };
    let mut seen: Vec<&str> = Vec::new();
    for a in acc {
        let (Some(id), Some(_), true) = (text(a, "id"), text(a, "text"), text(a, "kind").is_some_and(|k| KINDS.contains(&k))) else {
            out.push(format!("an acceptance item needs id, text and kind ({}): {}", KINDS.join(" or "), echo(a)));
            continue;
        };
        if seen.contains(&id) {
            out.push(format!("acceptance id {id} is used twice"));
        }
        seen.push(id);
    }
    let change = j.get("change_of");
    if change.is_some_and(|c| c.as_str().is_none_or(|c| c == name)) {
        out.push("change_of must name another job".to_owned());
    }
    // (What crates/specula/tests/leverage_data.rs holds, so that the two never disagree.)
    let all: String = j.get("brief").and_then(Json::as_array).map(|b| b.iter().filter_map(Json::as_str).collect::<Vec<_>>().join("\n")).unwrap_or_default();
    if all.chars().count() < 400 {
        out.push("the brief says what the game does (it is under 400 characters)".to_owned());
    }
    for word in BRIEF_NAMES_NO {
        if all.contains(word) {
            out.push(format!("the brief is the same for both arms and names no tool ({word})"));
        }
    }
    if !(all.contains("headless") && all.contains("frame") && (change.is_some() || (all.contains("PNG") && all.contains("JSON")))) {
        out.push("the brief asks both arms for the two checks (a headless run printing JSON, a frame as a PNG)".to_owned());
    }
    if change.is_some() && !all.contains("built by someone else") {
        out.push("a change's brief says its builder has no memory of the build".to_owned());
    }
    let good: Vec<&Json> = acc.iter().filter(|a| text(a, "id").is_some() && text(a, "text").is_some()).collect();
    if good.len() < 8 {
        out.push("a list worth judging has at least 8 items".to_owned());
    }
    for a in &good {
        let (id, said) = (text(a, "id").unwrap_or(""), text(a, "text").unwrap_or(""));
        let chars: Vec<char> = id.chars().collect();
        if !(chars.len() == 3 && name.starts_with(chars[0]) && chars[1..].iter().all(char::is_ascii_digit)) {
            out.push(format!("{id}: an item's id is the job's first letter and two digits"));
        }
        if said.chars().count() < 30 || !said.ends_with('.') {
            out.push(format!("{id}: an item is one sentence that is true or false of a run"));
        }
    }
    let checkable = good.iter().filter(|a| text(a, "kind") == Some("checkable")).count();
    if !good.is_empty() && !(checkable >= 1 && checkable * 4 <= good.len()) {
        out.push("the checks themselves are a small, marked part of the list (at least one item, at most a quarter)".to_owned());
    }
    out
}

/// Every problem in the data files, as lines (none: the data is good).
pub fn check_all() -> Vec<String> {
    let mut out = Vec::new();
    let s = match load_json(&data().join("settings.json")) {
        Ok(s) => s,
        Err(e) => return vec![e],
    };
    out.extend(check_settings(&s).into_iter().map(|p| format!("settings.json: {p}")));
    let mut names: Vec<String> = std::fs::read_dir(data().join("jobs"))
        .map(|d| d.flatten().filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json")).map(str::to_owned)).collect())
        .unwrap_or_default();
    names.sort();
    let mut ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut any_change = false;
    for name in &names {
        let j = match load_json(&data().join("jobs").join(format!("{name}.json"))) {
            Ok(j) => j,
            Err(e) => {
                out.push(e);
                continue;
            }
        };
        out.extend(check_job(&j, name).into_iter().map(|p| format!("jobs/{name}.json: {p}")));
        if let Some(of) = text(&j, "change_of") {
            if !names.iter().any(|n| n == of) {
                out.push(format!("jobs/{name}.json: change_of names no job ({of})"));
            }
        }
        any_change |= j.get("change_of").is_some();
        for a in j.get("acceptance").and_then(Json::as_array).unwrap_or(&[]) {
            if let Some(id) = text(a, "id") {
                ids.entry(id.to_owned()).or_default().push(format!("{name}.json"));
            }
        }
    }
    out.extend(ids.iter().filter(|(_, at)| at.len() > 1).map(|(id, at)| format!("{id} names one item across all jobs (it is in {})", at.join(", "))));
    if !any_change {
        out.push("the change round is one of the jobs".to_owned());
    }
    let limits: Vec<&str> = s.get("jobs").and_then(Json::as_object).map(|j| j.iter().map(|(k, _)| k.as_str()).collect()).unwrap_or_default();
    for name in &limits {
        if !names.iter().any(|n| n == name) {
            out.push(format!("settings.json: jobs.{name} has no jobs/{name}.json"));
        }
    }
    for name in &names {
        if !limits.contains(&name.as_str()) {
            out.push(format!("jobs/{name}.json has no limits in settings.json"));
        }
    }
    out
}

// ---- a run's folder -------------------------------------------------------------------------

fn run_name(run: &Path) -> String {
    absolute(run).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn read_run(run: &Path) -> R<Json> {
    let f = run.join("run.json");
    if !f.is_file() {
        return Err(format!("{}: not a run's folder (no run.json; 'prepare' makes one)", posix(run)));
    }
    load_json(&f)
}

fn write_run(run: &Path, r: &Json) -> R<()> {
    write_json(&run.join("run.json"), r)
}

pub fn costs_dir(run: &Path) -> R<PathBuf> {
    let d = absolute(run).parent().map(|p| p.join("costs")).ok_or_else(|| format!("{}: a run's folder has a folder above it", posix(run)))?;
    std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", posix(&d)))?;
    Ok(d)
}

fn cost_file(run: &Path) -> R<PathBuf> {
    Ok(costs_dir(run)?.join(format!("{}.json", run_name(run))))
}

fn calls_file(run: &Path) -> R<PathBuf> {
    Ok(costs_dir(run)?.join(format!("{}.calls.jsonl", run_name(run))))
}

pub fn read_cost(run: &Path) -> R<Json> {
    let f = cost_file(run)?;
    if f.is_file() {
        load_json(&f)
    } else {
        Ok(Json::Obj(Vec::new()))
    }
}

pub(crate) fn write_cost(run: &Path, c: &Json) -> R<()> {
    write_json(&cost_file(run)?, c)
}

/// How a builder's brief names this tool and its run.
fn me(run: &Path) -> (String, String) {
    let exe = std::env::current_exe().map(|e| posix(&e)).unwrap_or_else(|_| "examen".to_owned());
    (format!("\"{exe}\" leverage"), format!("\"{}\"", posix(&absolute(run))))
}

fn arm_text(arm: &str, run: &Path) -> Vec<String> {
    let (tool, rd) = me(run);
    let repo = repo();
    if arm == "plain" {
        return vec![
            "What you have".to_owned(),
            "- You build it without a game engine, with what is already on this machine and needs no download: the Rust toolchain (the standard library only; `cargo --offline`) and Python 3 (its standard library only). You choose.".to_owned(),
            format!("- Do not use or read Specula (the repository at {}), its tools or its documents.", posix(&repo)),
        ];
    }
    vec![
        "What you have".to_owned(),
        "- You build it with the Specula engine. Your work folder is a Specula project made by the engine (`specula new`): a small sample game that builds and plays, with every part in place (a game file, screens and words, a module with the rules, the SDK, a check). `notes/how.md` in it says how such a game is built, run and checked: read it first. Replace the sample's rules, screens and words with your game's.".to_owned(),
        format!("- Run the game with no window with `{tool} play {rd} --ticks N [--inputs T:WORD,T:WORD,...] [--seed N] [--settings FOLDER] [--frame FILE.png --size WxH]`: the engine's own `specula run` on your work folder (notes/how.md says what it takes and what it prints). It builds the module first and prints the game's state as JSON; no engine need be started for it. Paths in it are taken from the folder you run it in."),
        format!("- An engine left running serves the project's tools, should you need them: start or stop it with `{tool} engine start {rd}` and `{tool} engine stop {rd}` (start it first; restart it this way if you need to). Never run the engine's executable yourself."),
        format!("- Call a tool with `{tool} call {rd} TOOL '{{json}}'`; arguments too long or awkward for a command line go in a file: `{tool} call {rd} TOOL @args.json`. `{tool} tools {rd} [FILTER]` lists tools, and the `tools_catalog` tool describes them."),
        format!("- notes/how.md is written to be enough for a game of this size. Where it is not, the engine's documents are in {} (design notes for each area in `docs/design/`), with {}.", posix(&repo.join("docs")), posix(&repo.join("README.md"))),
        "- The two checks the brief asks for can be that one command, with the game file's `report` saying what it prints; or small scripts of yours.".to_owned(),
        "- Whoever checks the game starts with no engine running: the commands in your README must work from there.".to_owned(),
    ]
}

fn brief_text(j: &Json, arm: &str, run: &Path, s: &Json) -> String {
    let id = text(j, "id").unwrap_or("");
    let lim = s.get("jobs").and_then(|jobs| jobs.get(id)).cloned().unwrap_or(Json::Null);
    let mut lines = vec![format!("# {}", text(j, "title").unwrap_or("")), String::new()];
    lines.extend(j.get("brief").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(str::to_owned));
    lines.push(String::new());
    lines.extend(arm_text(arm, run));
    lines.push(String::new());
    lines.push("Limits and rules".to_owned());
    lines.push(format!(
        "- Budget: about {} tokens and {} minutes. Hand in what works when you are done or the budget is nearly spent; a smaller game that works is worth more than a larger one that does not.",
        commas(whole(&lim, "token_cap").unwrap_or(0)),
        whole(&lim, "wall_s").unwrap_or(0) / 60
    ));
    lines.push(format!("- Your work folder is {}. Write nowhere else.", posix(&absolute(run).join("work"))));
    lines.extend(s.get("rules").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(|r| format!("- {r}")));
    lines.push("- When you finish, answer with: what you built, the commands for the two checks, what you checked yourself, and what does not work.".to_owned());
    lines.join("\n") + "\n"
}

/// What `cmd` printed, and its exit code, run in `cwd` for at most `limit` (`None`: it could
/// not be started, or was stopped at the limit).
fn output(cmd: &[String], cwd: &Path, limit: Duration) -> Option<(i32, String)> {
    let refs: Vec<&str> = cmd.iter().map(String::as_str).collect();
    crate::run::said(&refs, cwd, limit).map(|(code, bytes)| (code, String::from_utf8_lossy(&bytes).into_owned()))
}

/// Which engine a run was made with: the repository's commit, marked when the tree has
/// changes the commit does not.
fn engine_build() -> String {
    let git = |args: &[&str]| crate::run::said_text(args, &repo(), Duration::from_secs(30)).map(|t| t.trim().to_owned());
    match (git(&["git", "rev-parse", "--short=10", "HEAD"]), git(&["git", "status", "--porcelain", "--untracked-files=no"])) {
        (Some(head), Some(dirty)) if !head.is_empty() => format!("{head}{}", if dirty.is_empty() { "" } else { "+changes" }),
        _ => "unknown".to_owned(),
    }
}

fn runs_of(root: &Path) -> Vec<PathBuf> {
    let mut runs: Vec<PathBuf> = std::fs::read_dir(root).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.join("run.json").is_file()).collect()).unwrap_or_default();
    runs.sort();
    runs
}

fn free_port(s: &Json, root: &Path) -> R<i64> {
    let used: Vec<i64> = runs_of(root).iter().filter_map(|run| load_json(&run.join("run.json")).ok()).filter_map(|r| whole(&r, "port")).collect();
    let ports: Vec<i64> = s.get("ports").and_then(Json::as_array).map(|p| p.iter().filter_map(Json::as_f64).map(|n| n as i64).collect()).unwrap_or_default();
    (ports[0]..=ports[1]).find(|p| !used.contains(p)).ok_or_else(|| "no port left in settings.json's range".to_owned())
}

/// Copies a folder, leaving out the entries named in `skip` at any depth.
fn copy_tree(from: &Path, to: &Path, skip: &[&str]) -> R<()> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", posix(to)))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", posix(from)))?.flatten() {
        let name = entry.file_name();
        if skip.iter().any(|s| name.to_str() == Some(s)) {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        match entry.file_type() {
            Ok(t) if t.is_dir() => copy_tree(&src, &dst, skip)?,
            Ok(t) if t.is_file() => {
                std::fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", posix(&src)))?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub struct Prepare {
    pub job: String,
    pub arm: String,
    pub n: i64,
    pub out: PathBuf,
    pub exe: Option<PathBuf>,
    pub port: Option<i64>,
    pub from: Option<PathBuf>,
    pub build: Option<String>,
}

pub fn prepare(a: &Prepare) -> R<Json> {
    let s = settings()?;
    let j = job(&a.job)?;
    if !ARMS.contains(&a.arm.as_str()) {
        return Err(format!("arm must be one of {ARMS:?}"));
    }
    let root = absolute(&a.out);
    let run = root.join(format!("{}-{}-{}", a.job, a.arm, a.n));
    if run.exists() {
        return Err(format!("{} exists already: a run is prepared once", posix(&run)));
    }
    let work = run.join("work");
    // (Checked before anything is made: a refused prepare leaves no folder that refuses the next.)
    let exe = a.exe.as_deref().map(absolute);
    if a.arm == "engine" && !exe.as_ref().is_some_and(|e| e.is_file()) {
        return Err("the engine arm needs --exe: the specula executable (a release build)".to_owned());
    }
    let limits = s.get("jobs").and_then(|jobs| jobs.get(&a.job)).cloned().unwrap_or(Json::Null);
    let pick = |k: &str| s.get(k).cloned().unwrap_or(Json::Null);
    let mut r = Json::obj([
        ("leverage", Json::Num(1.0)),
        ("job", Json::Str(a.job.clone())),
        ("arm", Json::Str(a.arm.clone())),
        ("n", Json::Num(a.n as f64)),
        ("phase", Json::Str("build".to_owned())),
        ("prepared", Json::Str(now_text())),
        (
            "settings",
            Json::obj([
                ("runs_per_cell", pick("runs_per_cell")),
                ("builder_model", pick("builder_model")),
                ("judge_model", pick("judge_model")),
                ("limits", limits.clone()),
                ("judge", pick("judge")),
                ("token_weights", pick("token_weights")),
            ]),
        ),
    ]);
    let port = match (a.arm.as_str(), a.port) {
        ("engine", Some(p)) => Some(p),
        ("engine", None) => Some(free_port(&s, &root)?),
        _ => None,
    };
    if let Some(of) = text(&j, "change_of") {
        let Some(from) = &a.from else { return Err(format!("the {} job changes a finished {of} game: --from RUN names it", a.job)) };
        let src = read_run(from)?;
        if text(&src, "job") != Some(of) || text(&src, "arm") != Some(a.arm.as_str()) {
            return Err(format!("--from must be a {of} run of the {} arm (it is {} / {})", a.arm, text(&src, "job").unwrap_or("?"), text(&src, "arm").unwrap_or("?")));
        }
        std::fs::create_dir_all(&run).map_err(|e| format!("{}: {e}", posix(&run)))?;
        // The game as it was handed in; never its builder's costs or the judge's verdict.
        copy_tree(&from.join("work"), &work, &["target", ".specula-cache"])?;
        r.set("from", Json::Str(run_name(from)));
    } else if a.arm == "engine" {
        // The engine arm starts where anyone who builds through the engine starts: a project
        // the engine made (docs/design/FIRST_HOUR.md, slice 7). Its SDK is the engine's own.
        std::fs::create_dir_all(run.join("engine")).map_err(|e| format!("{}: {e}", posix(&run)))?;
        let exe = exe.clone().unwrap_or_default();
        let cmd = [exe.to_string_lossy().into_owned(), "new".to_owned(), "work".to_owned(), "--in".to_owned(), run.to_string_lossy().into_owned(), "--settings".to_owned(), run.join("engine").join("settings").to_string_lossy().into_owned()];
        match output(&cmd, &run, Duration::from_secs(180)) {
            Some((0, _)) if work.join("game.json").is_file() => {}
            Some((code, said)) => return Err(format!("`specula new` did not make the engine arm's project (exit {code}): {}", said.chars().take(400).collect::<String>())),
            None => return Err("`specula new` could not be run, or did not end in 180 s".to_owned()),
        }
        r.set("started_with", sizes(&work));
    } else {
        std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", posix(&work)))?;
    }
    if a.arm == "engine" {
        r.set("exe", Json::Str(posix(&exe.unwrap_or_default())));
        r.set("engine_build", Json::Str(a.build.clone().unwrap_or_else(engine_build)));
        r.set("port", port.map_or(Json::Null, |p| Json::Num(p as f64)));
        std::fs::create_dir_all(run.join("engine")).map_err(|e| format!("{}: {e}", posix(&run)))?;
    }
    write_run(&run, &r)?;
    std::fs::write(run.join("BRIEF.md"), brief_text(&j, &a.arm, &run, &s)).map_err(|e| format!("BRIEF.md: {e}"))?;
    Ok(Json::obj([
        ("run", Json::Str(posix(&run))),
        ("brief", Json::Str(posix(&run.join("BRIEF.md")))),
        ("port", r.get("port").cloned().unwrap_or(Json::Null)),
        ("model", pick("builder_model")),
        ("token_cap", limits.get("token_cap").cloned().unwrap_or(Json::Null)),
        ("wall_s", limits.get("wall_s").cloned().unwrap_or(Json::Null)),
    ]))
}

// ---- the engine arm's engine ------------------------------------------------------------------

/// One command of the engine's own command line, with this run's settings folder.
fn specula(r: &Json, run: &Path, words: &[String], limit: Duration) -> Option<(i32, String)> {
    let run = absolute(run);
    let mut cmd = vec![text(r, "exe").unwrap_or("specula").to_owned()];
    cmd.extend(words.iter().cloned());
    cmd.extend(["--settings".to_owned(), run.join("engine").join("settings").to_string_lossy().into_owned()]);
    output(&cmd, &run, limit)
}

fn engine_arm(r: &Json) -> R<()> {
    if text(r, "arm") == Some("engine") {
        Ok(())
    } else {
        Err("this run's arm has no engine".to_owned())
    }
}

fn port_text(r: &Json) -> String {
    whole(r, "port").unwrap_or(0).to_string()
}

/// `engine start|stop RUN`: prints the engine's own answer; returns its exit code.
pub fn engine_cmd(what: &str, run: &Path) -> R<i32> {
    let r = read_run(run)?;
    engine_arm(&r)?;
    let run = absolute(run);
    let own = |name: &[&str]| name.iter().fold(run.clone(), |p, part| p.join(part)).to_string_lossy().into_owned();
    let ran = if what == "start" {
        // (Opened through `specula open`, which starts the engine, waits until it serves and
        // ends: the engine is not this command's child.)
        let cmd: Vec<String> = [text(&r, "exe").unwrap_or("specula"), "open", &own(&["work"]), "--settings", &own(&["engine", "settings"]), "--", "--headless", "--port", &port_text(&r), "--lead", "builder", "--lead", "judge", "--trace", &own(&["engine", "trace"]), "--captures", &own(&["work", "captures"])]
            .iter()
            .map(|w| (*w).to_owned())
            .collect();
        output(&cmd, &run, Duration::from_secs(300))
    } else {
        specula(&r, &run, &["stop".to_owned(), "--port".to_owned(), port_text(&r)], Duration::from_secs(180))
    };
    let Some((code, said)) = ran else { return Err("the engine's command did not answer in time".to_owned()) };
    print!("{said}");
    log_call(&run, &r, &format!("engine.{what}"), 0, said.len(), code, 0.0)?;
    Ok(code)
}

/// Stops the run's engine and says whether none is left: stopped now, or none was running
/// (`specula stop` refuses both a stop that failed and an engine that is not there with exit
/// code 1: its answer tells them apart).
pub fn stop_engine(r: &Json, run: &Path) -> bool {
    if text(r, "arm") != Some("engine") {
        return true;
    }
    match specula(r, run, &["stop".to_owned(), "--port".to_owned(), port_text(r)], Duration::from_secs(180)) {
        None => false,
        Some((_, said)) => stopped_says(&said),
    }
}

/// Whether `specula stop`'s answer says no engine of the run is left.
pub fn stopped_says(answer: &str) -> bool {
    let j = Json::parse(answer.trim()).unwrap_or(Json::Null);
    truthy(j.get("stopped")) || matches!(text(&j, "error"), Some("not_listed" | "no_engine"))
}

fn log_call(run: &Path, r: &Json, tool: &str, args_bytes: usize, answer_bytes: usize, code: i32, ms: f64) -> R<()> {
    let row = Json::obj([
        ("t", Json::Num((epoch_s() * 1000.0).round() / 1000.0)),
        ("phase", Json::Str(text(r, "phase").unwrap_or("build").to_owned())),
        ("tool", Json::Str(tool.to_owned())),
        ("args_bytes", Json::Num(args_bytes as f64)),
        ("answer_bytes", Json::Num(answer_bytes as f64)),
        ("exit", Json::Num(f64::from(code))),
        ("ms", Json::Num((ms * 10.0).round() / 10.0)),
    ]);
    let file = calls_file(run)?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&file).map_err(|e| format!("{}: {e}", posix(&file)))?;
    writeln!(f, "{row}").map_err(|e| format!("{}: {e}", posix(&file)))
}

/// `call RUN TOOL [ARGS]`: one tool call through the run's engine, as its builder (or, once
/// the run is finished, its judge), logged. Prints the answer; returns the exit code.
pub fn call_cmd(run: &Path, tool: &str, args: Option<&str>) -> R<i32> {
    let r = read_run(run)?;
    engine_arm(&r)?;
    let run = absolute(run);
    let mut words = vec!["call".to_owned(), tool.to_owned()];
    let mut sent = String::new();
    let mut handed = None;
    if let Some(given) = args.filter(|a| !a.is_empty()) {
        let given = match given.strip_prefix('@') {
            Some(file) => {
                let f = absolute(Path::new(file));
                std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", posix(&f)))?
            }
            None => given.to_owned(),
        };
        // (Sent on one line: what a file holds may be laid out over many.)
        sent = Json::parse(given.trim_start_matches('\u{feff}')).map_err(|e| format!("the arguments are not JSON ({}); in a shell, quoting is easier from a file: TOOL @args.json", e.message))?.to_string();
        // (Handed over in a file, as `specula call TOOL @FILE` takes them: a command line
        // holds about 32,000 characters, and a recipe or a module's source is more.)
        let file = run.join("engine").join(format!("args-{}.json", std::process::id()));
        std::fs::write(&file, &sent).map_err(|e| format!("{}: {e}", posix(&file)))?;
        words.push(format!("@{}", file.to_string_lossy()));
        handed = Some(file);
    }
    // (The run's builder, and later its judge, are the leads of their own engine: a module
    // is native code, which only a lead the operator named may build and load.)
    let who = if text(&r, "phase") == Some("judge") { "judge" } else { "builder" };
    words.extend(["--port".to_owned(), port_text(&r), "--as".to_owned(), who.to_owned(), "--role".to_owned(), "lead".to_owned()]);
    let began = Instant::now();
    let ran = specula(&r, &run, &words, Duration::from_secs(180));
    if let Some(file) = handed {
        let _ = std::fs::remove_file(file);
    }
    let Some((code, said)) = ran else { return Err("the engine's command did not answer in 180 s".to_owned()) };
    log_call(&run, &r, tool, sent.len(), said.len(), code, began.elapsed().as_secs_f64() * 1000.0)?;
    print!("{said}");
    Ok(code)
}

/// `play RUN [FLAGS]`: the game's own headless run (`specula run`, with the flags given) on the
/// run's work folder, logged as one call. Prints what it printed; returns its exit code. Paths
/// among the flags are the caller's, from the folder the caller is in.
pub fn play_cmd(run: &Path, flags: &[String]) -> R<i32> {
    let r = read_run(run)?;
    engine_arm(&r)?;
    let run = absolute(run);
    let mut cmd = vec![text(&r, "exe").unwrap_or("specula").to_owned(), "run".to_owned(), run.join("work").to_string_lossy().into_owned()];
    cmd.extend(flags.iter().cloned());
    let here = std::env::current_dir().unwrap_or_else(|_| run.clone());
    let began = Instant::now();
    // (Its own time limit is the command's. What it says beside its answer, on its standard
    // error, is what the game says is wrong with its files: passed on as it is.)
    let ran = std::process::Command::new(&cmd[0]).args(&cmd[1..]).current_dir(&here).stdin(std::process::Stdio::null()).output().map_err(|e| format!("{}: {e}", cmd[0]))?;
    let (code, said) = (ran.status.code().unwrap_or(1), String::from_utf8_lossy(&ran.stdout).into_owned());
    eprint!("{}", String::from_utf8_lossy(&ran.stderr));
    log_call(&run, &r, "(run)", flags.iter().map(|f| f.len() + 1).sum(), said.len(), code, began.elapsed().as_secs_f64() * 1000.0)?;
    print!("{said}");
    Ok(code)
}

/// `tools RUN [FILTER]`.
pub fn tools_cmd(run: &Path, filter: Option<&str>) -> R<i32> {
    let r = read_run(run)?;
    engine_arm(&r)?;
    let began = Instant::now();
    let mut words = vec!["tools".to_owned()];
    words.extend(filter.map(str::to_owned));
    words.extend(["--port".to_owned(), port_text(&r)]);
    let Some((code, said)) = specula(&r, run, &words, Duration::from_secs(180)) else { return Err("the engine's command did not answer in 180 s".to_owned()) };
    log_call(run, &r, "(tools)", filter.map_or(0, str::len), said.len(), code, began.elapsed().as_secs_f64() * 1000.0)?;
    print!("{said}");
    Ok(code)
}

// ---- what a run left ------------------------------------------------------------------------

/// The files a run's work folder holds: how many, their bytes, and the lines of the text
/// among them by kind.
pub fn sizes(work: &Path) -> Json {
    let (mut files, mut size, mut lines) = (0u64, 0u64, 0u64);
    let mut by_ext: BTreeMap<String, u64> = BTreeMap::new();
    let mut stack = vec![work.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            match entry.file_type() {
                Ok(t) if t.is_dir() => {
                    if !SKIP_DIRS.contains(&name.as_str()) {
                        stack.push(path);
                    }
                }
                Ok(t) if t.is_file() => {
                    let Ok(meta) = entry.metadata() else { continue };
                    files += 1;
                    size += meta.len();
                    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                    if CODE.contains(&ext.as_str()) && meta.len() < 4_000_000 {
                        // (Lines as a reader counts them: a last line with no line end is one.)
                        let k = std::fs::read(&path).map(|bytes| bytes.split_inclusive(|b| *b == b'\n').count() as u64).unwrap_or(0);
                        lines += k;
                        *by_ext.entry(format!(".{ext}")).or_default() += k;
                    }
                }
                _ => {}
            }
        }
    }
    Json::obj([
        ("files", Json::Num(files as f64)),
        ("bytes", Json::Num(size as f64)),
        ("text_lines", Json::Num(lines as f64)),
        ("lines_by_kind", Json::Obj(by_ext.into_iter().map(|(k, v)| (k, Json::Num(v as f64))).collect())),
    ])
}

/// The calls a run made through its engine in `phase`, counted.
fn call_totals(run: &Path, phase: &str) -> R<Json> {
    let file = calls_file(run)?;
    let rows: Vec<Json> = std::fs::read_to_string(&file).unwrap_or_default().lines().filter_map(|l| Json::parse(l).ok()).filter(|row| text(row, "phase") == Some(phase)).collect();
    let mut by_tool: BTreeMap<String, u64> = BTreeMap::new();
    for row in &rows {
        *by_tool.entry(text(row, "tool").unwrap_or("").to_owned()).or_default() += 1;
    }
    // (Starting and stopping the engine are the harness's own commands, not tool calls.)
    let calls: Vec<&Json> = rows.iter().filter(|row| !text(row, "tool").unwrap_or("").starts_with("engine.")).collect();
    let sum = |k: &str| calls.iter().filter_map(|row| num(row, k)).sum::<f64>();
    let mut by: Vec<(String, u64)> = by_tool.into_iter().collect();
    by.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(Json::obj([
        ("calls", Json::Num(calls.len() as f64)),
        ("refused", Json::Num(calls.iter().filter(|row| num(row, "exit") != Some(0.0)).count() as f64)),
        ("args_bytes", Json::Num(sum("args_bytes"))),
        ("answer_bytes", Json::Num(sum("answer_bytes"))),
        ("by_tool", Json::Obj(by.into_iter().map(|(k, v)| (k, Json::Num(v as f64))).collect())),
    ]))
}

/// `finish RUN`: what the builder left, counted; the engine stopped; the run is the judge's.
pub fn finish(run: &Path) -> R<Json> {
    let mut r = read_run(run)?;
    let mut c = read_cost(run)?;
    if text(&r, "arm") == Some("engine") {
        c.set("engine_stopped", Json::Bool(stop_engine(&r, run)));
        c.set("engine", call_totals(run, "build")?);
    }
    c.set("left", sizes(&run.join("work")));
    c.set("finished", Json::Str(now_text()));
    write_cost(run, &c)?;
    r.set("phase", Json::Str("judge".to_owned()));
    write_run(run, &r)?;
    Ok(Json::obj([("left", c.get("left").cloned().unwrap_or(Json::Null)), ("engine", c.get("engine").cloned().unwrap_or(Json::Null))]))
}

// ---- tokens ---------------------------------------------------------------------------------

fn kinds_json(by: &[f64; 4], requests: usize, last: f64) -> Json {
    let mut pairs: Vec<(String, Json)> = TOKEN_KINDS.iter().zip(by).map(|(k, v)| ((*k).to_owned(), Json::Num(*v))).collect();
    pairs.push(("requests".to_owned(), Json::Num(requests as f64)));
    pairs.push(("context_at_end".to_owned(), Json::Num(last)));
    Json::Obj(pairs)
}

fn usage_of(u: &Json) -> [f64; 4] {
    USAGE_NAMES.map(|name| num(u, name).unwrap_or(0.0))
}

/// A run's tokens by kind, summed over the requests of a transcript that keeps each request's
/// usage (Claude Code's: one JSON object a line, `message.usage`, a request's lines sharing
/// `message.id`). Nothing of the transcript's text is kept.
pub fn transcript_usage(path: &Path) -> R<Json> {
    let all = std::fs::read(path).map_err(|e| format!("{}: {e}", posix(path)))?;
    let (mut tot, mut seen, mut last) = ([0.0f64; 4], Vec::<String>::new(), 0.0);
    for line in String::from_utf8_lossy(&all).lines() {
        let Ok(j) = Json::parse(line) else { continue };
        let Some(m) = j.get("message") else { continue };
        let Some(u) = m.get("usage").filter(|u| u.as_object().is_some()) else { continue };
        let id = m.get("id").map(Json::to_string).unwrap_or_default();
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        let got = usage_of(u);
        for (t, g) in tot.iter_mut().zip(got) {
            *t += g;
        }
        last = got[0] + got[1] + got[2];
    }
    if seen.is_empty() {
        return Err(format!("{}: no request with a usage record (this reads Claude Code's transcripts; for another harness give the figures to 'record' yourself)", posix(path)));
    }
    Ok(kinds_json(&tot, seen.len(), last))
}

/// One figure for a run's tokens: each kind times its weight (settings.json's
/// `token_weights`: relative prices, data like every other size), rounded half to even.
pub fn weighted(by_kind: &Json, weights: &Json) -> f64 {
    TOKEN_KINDS.iter().map(|k| num(by_kind, k).unwrap_or(0.0) * num(weights, k).unwrap_or(0.0)).sum::<f64>().round_ties_even()
}

/// What a run used, from the lines its process printed (Claude Code's `stream-json`: one JSON
/// object a line; each request's message carries its usage, and the last line is the result
/// with the totals and the price). A run stopped at its time limit has no last line: its
/// requests are summed, and it says its tokens are incomplete.
pub fn stream_usage(path: &Path) -> Json {
    let all = std::fs::read(path).unwrap_or_default();
    let mut by = [0.0f64; 4];
    let mut seen: Vec<(String, [f64; 4])> = Vec::new();
    let (mut tools, mut last, mut result) = (0u64, 0.0, None);
    for line in String::from_utf8_lossy(&all).lines() {
        let Ok(j) = Json::parse(line) else { continue };
        if j.as_object().is_none() {
            continue;
        }
        if text(&j, "type") == Some("result") {
            result = Some(j);
            continue;
        }
        let assistant = text(&j, "type") == Some("assistant");
        let Some(m) = j.get("message").filter(|m| m.as_object().is_some()) else { continue };
        if let (true, Some(content)) = (assistant, m.get("content").and_then(Json::as_array)) {
            tools += content.iter().filter(|c| text(c, "type") == Some("tool_use")).count() as u64;
        }
        let Some(u) = m.get("usage").filter(|u| assistant && u.as_object().is_some()) else { continue };
        // (A request's message arrives in parts, each with the usage so far: the last counts.)
        let id = m.get("id").map(Json::to_string).unwrap_or_default();
        let got = usage_of(u);
        match seen.iter_mut().find(|(known, _)| *known == id) {
            Some((_, before)) => {
                for ((b, old), new) in by.iter_mut().zip(*before).zip(got) {
                    *b += new - old;
                }
                *before = got;
            }
            None => {
                for (b, new) in by.iter_mut().zip(got) {
                    *b += new;
                }
                seen.push((id, got));
            }
        }
        last = got[0] + got[1] + got[2];
    }
    let mut by_kind = kinds_json(&by, seen.len(), last);
    let mut out = vec![("tool_uses".to_owned(), Json::Num(tools as f64)), ("complete".to_owned(), Json::Bool(result.is_some()))];
    if let Some(result) = &result {
        if let Some(models) = result.get("modelUsage").and_then(Json::as_object).filter(|m| !m.is_empty()) {
            let total = |name: &str| models.iter().filter_map(|(_, v)| num(v, name)).sum::<f64>();
            for (kind, name) in TOKEN_KINDS.iter().zip(["inputTokens", "cacheCreationInputTokens", "cacheReadInputTokens", "outputTokens"]) {
                by_kind.set(*kind, Json::Num(total(name)));
            }
            let mut names: Vec<&str> = models.iter().map(|(k, _)| k.as_str()).collect();
            names.sort_unstable();
            out.push(("models".to_owned(), Json::arr(names.into_iter().map(|n| Json::Str(n.to_owned())))));
        }
        out.push(("cost_usd".to_owned(), result.get("total_cost_usd").cloned().unwrap_or(Json::Null)));
        out.push(("turns".to_owned(), result.get("num_turns").cloned().unwrap_or(Json::Null)));
        out.push(("said".to_owned(), Json::Str(text(result, "result").unwrap_or("").to_owned())));
        out.push(("error".to_owned(), Json::Bool(truthy(result.get("is_error")))));
    }
    out.insert(0, ("by_kind".to_owned(), by_kind));
    Json::Obj(out)
}

// ---- a builder's or a judge's process -------------------------------------------------------

const BUILDER_PROMPT: &str = "You are a builder in a measured comparison. Your whole task is in the file BRIEF.md in the folder you were started in: \
read it and build what it asks, in the work folder it names, within the limits and rules it states. \
When you finish, answer with what the brief asks for.";

fn judge_prompt(rules: &str) -> String {
    format!(
        "You are the judge of one game in a measured comparison. You built none of it. Your whole task is in the file JUDGE.md in the folder \
you were started in: read it and do what it says, for the game in the work folder beside it. Rules of this machine: {rules} \
Repair nothing in the game. Write only verdict.json beside JUDGE.md, and scratch files of your own in a new folder judge-scratch beside it. \
When you finish, answer in a few lines: how many items passed, which failed and why, and anything that got in your way as a judge."
    )
}

/// The file a command's first word names, as the system would find it (`PATH`, and on Windows
/// the extensions a command may have).
fn which(name: &str) -> Option<PathBuf> {
    let given = Path::new(name);
    if given.components().count() > 1 {
        return given.is_file().then(|| given.to_path_buf());
    }
    let exts: Vec<String> = if cfg!(windows) {
        let mut exts = vec![String::new()];
        exts.extend(std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT".to_owned()).split(';').filter(|e| !e.is_empty()).map(str::to_owned));
        exts
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| exts.iter().map(|ext| dir.join(format!("{name}{ext}"))).find(|p| p.is_file()))
}

pub struct RunArgs {
    pub run: PathBuf,
    pub judge: bool,
    pub again: Option<String>,
    pub note: Option<String>,
}

/// `run RUN [--judge]`: starts the run's builder (or its judge) as a process of its own in the
/// run's folder, stops it at its time limit, and records what it used.
pub fn run_cmd(a: &RunArgs) -> R<i32> {
    // An interrupt (Ctrl+C, the console closing) ends the run as its time limit does: the
    // agent's whole process tree and the run's engine are stopped, and what it used so far is
    // recorded, with `ended: stopped`. (The script this was stopped both on any way out;
    // review R2 of M6 part 3: the port left the engine serving when it was interrupted.)
    let _ = specula_sys::interrupt::install();
    let out = run_agent(a);
    specula_sys::interrupt::finished();
    out
}

fn run_agent(a: &RunArgs) -> R<i32> {
    let s = settings()?;
    let r = read_run(&a.run)?;
    let run = absolute(&a.run);
    let who = if a.judge { "judge" } else { "builder" };
    let rules: Vec<&str> = s.get("rules").and_then(Json::as_array).map(|r| r.iter().filter_map(Json::as_str).collect()).unwrap_or_default();
    let (prompt, model, wall) = if a.judge {
        if text(&r, "phase") != Some("judge") {
            return Err("'finish' the run first: the judge starts from what the builder handed in".to_owned());
        }
        let had = read_cost(&run)?;
        let judged = truthy(had.get("verdict")) || truthy(had.get("judge"));
        if judged && a.again.is_none() {
            return Err("this run is judged already ('--judge --again WHY' judges it once more and keeps the first judging)".to_owned());
        }
        // (The judge's page first: it refuses when the game before a change has no verdict,
        // and nothing of an earlier judging is moved for a judging that will not happen.)
        let page = judge_text(&run, &r)?;
        if judged {
            keep_judging(&run, had, a.again.as_deref().unwrap_or(""))?;
        }
        std::fs::write(run.join("JUDGE.md"), page).map_err(|e| format!("JUDGE.md: {e}"))?;
        (judge_prompt(&rules.join(" ")), text(&s, "judge_model").unwrap_or("").to_owned(), s.get("judge").and_then(|j| whole(j, "wall_s")).unwrap_or(1800))
    } else {
        if text(&r, "phase") != Some("build") || truthy(read_cost(&run)?.get("builder")) {
            return Err("this run's builder has run already: a run is built once".to_owned());
        }
        let wall = r.get("settings").and_then(|s| s.get("limits")).and_then(|l| whole(l, "wall_s")).unwrap_or(3600);
        (BUILDER_PROMPT.to_owned(), text(&s, "builder_model").unwrap_or("").to_owned(), wall)
    };
    let cmd: Vec<String> = s.get("runner").and_then(|r| r.get("command")).and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(|c| c.replace("{prompt}", &prompt).replace("{model}", &model)).collect();
    let Some(exe) = cmd.first().and_then(|c| which(c)) else {
        return Err(format!("{}: no such command on this machine (settings.json's runner.command)", cmd.first().map_or("", String::as_str)));
    };
    let stream = costs_dir(&run)?.join(format!("{}.{who}.stream.jsonl", run_name(&run)));
    let began = Instant::now();
    let mut ended = "done";
    let file = std::fs::File::create(&stream).and_then(|f| Ok((f.try_clone()?, f))).map_err(|e| format!("{}: {e}", posix(&stream)))?;
    let mut command = Command::new(&exe);
    command.args(&cmd[1..]).current_dir(&run).stdin(Stdio::null()).stdout(file.0).stderr(file.1);
    // (A group of its own off Windows, so that stopping it stops what it started.)
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let exit = match command.spawn() {
        Err(e) => {
            // Whatever happened, no engine of this run is left behind.
            stop_engine(&r, &run);
            return Err(format!("{}: could not be started ({e})", posix(&exe)));
        }
        Ok(mut child) => loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) if began.elapsed() >= Duration::from_secs(wall.max(0) as u64) => {
                    ended = "wall";
                    stop_tree(&mut child);
                    break child.try_wait().ok().flatten().and_then(|s| s.code());
                }
                Ok(None) if specula_sys::interrupt::requested() => {
                    ended = "stopped";
                    stop_tree(&mut child);
                    break child.try_wait().ok().flatten().and_then(|s| s.code());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                Err(_) => break None,
            }
        },
    };
    // An agent that ended on the interrupt itself (most command-line agents do) was stopped
    // all the same, not done. What such an agent had started and left running is not found:
    // a tree is stopped through its root while the root lives (review R3 of M6 part 3; a job
    // object would hold it, and the tooling has none). Where interrupts are not caught (off
    // Windows, for now) an interrupt ends this command at once, as it always did, and the
    // run's engine is stopped by hand: `examen leverage engine stop RUN`.
    if specula_sys::interrupt::requested() && ended == "done" {
        ended = "stopped";
    }
    // Whatever the agent did or did not do, no engine of this run is left behind: an engine is
    // started by `specula open`, which exits, so it is not among the agent's children.
    let engine_gone = stop_engine(&r, &run);
    let took = began.elapsed().as_secs_f64();
    let u = stream_usage(&stream);
    let by_kind = u.get("by_kind").cloned().unwrap_or(Json::Null);
    let weights = s.get("token_weights").cloned().unwrap_or(Json::Null);
    let w = weighted(&by_kind, &weights);
    let errored = truthy(u.get("error"));
    let mut row = Json::obj([
        ("tokens", by_kind.get("context_at_end").cloned().unwrap_or(Json::Null)),
        ("tool_uses", u.get("tool_uses").cloned().unwrap_or(Json::Null)),
        ("duration_s", Json::Num((took * 10.0).round() / 10.0)),
        ("by_kind", by_kind),
        ("weighted", Json::Num(w)),
        ("cost_usd", u.get("cost_usd").cloned().unwrap_or(Json::Null)),
        ("turns", u.get("turns").cloned().unwrap_or(Json::Null)),
        ("exit", exit.map_or(Json::Null, |c| Json::Num(f64::from(c)))),
        ("runner", Json::Str(cmd[0].clone())),
        ("model", Json::Str(model)),
        ("models", u.get("models").cloned().unwrap_or(Json::Null)),
        ("tokens_complete", u.get("complete").cloned().unwrap_or(Json::Bool(false))),
        ("engine_stopped", Json::Bool(engine_gone)),
    ]);
    if a.judge {
        row.set("ended", Json::Str(ended.to_owned()));
    } else {
        let cap = r.get("settings").and_then(|s| s.get("limits")).and_then(|l| num(l, "token_cap")).unwrap_or(f64::MAX);
        row.set("ended", Json::Str(if errored { "error" } else { ended }.to_owned()));
        row.set("over_token_cap", Json::Bool(w > cap));
        row.set("over_wall", Json::Bool(ended == "wall"));
        row.set("note", Json::Str(a.note.clone().unwrap_or_default()));
    }
    let mut c = read_cost(&run)?;
    c.set(who, row.clone());
    write_cost(&run, &c)?;
    let said = text(&u, "said").filter(|s| !s.is_empty()).unwrap_or("(the process ended without a last answer)\n");
    let said_file = costs_dir(&run)?.join(format!("{}.{who}.said.md", run_name(&run)));
    std::fs::write(&said_file, said).map_err(|e| format!("{}: {e}", posix(&said_file)))?;
    if a.judge {
        let v = run.join("verdict.json");
        if v.is_file() {
            match judge(&run, Some(&v), None) {
                Ok(totals) => println!("{totals}"),
                Err(e) => println!("{}", Json::obj([("error", Json::Str("bad_verdict".to_owned())), ("message", Json::Str(e))])),
            }
        } else {
            println!("{}", Json::obj([("error", Json::Str("no_verdict".to_owned())), ("message", Json::Str("the judge left no verdict.json".to_owned()))]));
        }
    } else {
        println!("{}", finish(&run)?);
    }
    let shown = ["ended", "duration_s", "tool_uses", "weighted", "cost_usd", "turns", "exit"];
    println!("{}", Json::Obj(shown.iter().map(|k| ((*k).to_owned(), row.get(k).cloned().unwrap_or(Json::Null))).collect()));
    Ok(0)
}

pub struct Record {
    pub run: PathBuf,
    pub tokens: f64,
    pub tool_uses: f64,
    pub duration_s: f64,
    pub ended: String,
    pub note: Option<String>,
    pub usage: Option<PathBuf>,
    pub by_kind: Option<[f64; 4]>,
    pub judge: bool,
}

/// `record RUN ...`: what an agent's own harness reported, for a run this tool did not start.
pub fn record(a: &Record) -> R<Json> {
    let r = read_run(&a.run)?;
    let mut c = read_cost(&a.run)?;
    let who = if a.judge { "judge" } else { "builder" };
    let mut row = c.get(who).cloned().filter(|j| j.as_object().is_some()).unwrap_or(Json::Obj(Vec::new()));
    row.set("tokens", Json::Num(a.tokens));
    row.set("tool_uses", Json::Num(a.tool_uses));
    row.set("duration_s", Json::Num(a.duration_s));
    let by_kind = match (&a.usage, a.by_kind) {
        (Some(transcript), _) => Some(transcript_usage(transcript)?),
        (None, Some(by)) => Some(Json::Obj(TOKEN_KINDS.iter().zip(by).map(|(k, v)| ((*k).to_owned(), Json::Num(v))).collect())),
        (None, None) => None,
    };
    let mut held = a.tokens;
    if let Some(by_kind) = by_kind {
        held = weighted(&by_kind, &settings()?.get("token_weights").cloned().unwrap_or(Json::Null));
        row.set("by_kind", by_kind);
        row.set("weighted", Json::Num(held));
    }
    if !a.judge {
        let lim = r.get("settings").and_then(|s| s.get("limits")).cloned().unwrap_or(Json::Null);
        row.set("ended", Json::Str(a.ended.clone()));
        row.set("over_token_cap", Json::Bool(held > num(&lim, "token_cap").unwrap_or(f64::MAX)));
        row.set("over_wall", Json::Bool(a.duration_s > num(&lim, "wall_s").unwrap_or(f64::MAX)));
        row.set("note", Json::Str(a.note.clone().unwrap_or_default()));
    }
    c.set(who, row.clone());
    write_cost(&a.run, &c)?;
    Ok(row)
}

// ---- judging --------------------------------------------------------------------------------

fn acceptance_of(r: &Json) -> R<(Json, Vec<Json>)> {
    let j = job(text(r, "job").unwrap_or(""))?;
    let items = j.get("acceptance").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
    Ok((j, items))
}

/// The judge's page for a run.
pub fn judge_text(run: &Path, r: &Json) -> R<String> {
    let (j, items) = acceptance_of(r)?;
    let (tool, rd) = me(run);
    let mut lines = vec![
        format!("# Judging: {}", text(&j, "title").unwrap_or("")),
        String::new(),
        "You built none of this. Decide, item by item, whether the game in `work/` does what its brief asked. The brief the builder had is `BRIEF.md`, beside this file.".to_owned(),
        String::new(),
        "How".to_owned(),
        "- Read the game's README.md (CHANGES.md for a change) for the commands of its headless run and its frame. Run them yourself, with inputs you choose: an item passes on what you saw the game do, never on what its builder wrote.".to_owned(),
        "- Open the frames (PNG) and look at them.".to_owned(),
        "- A check that cannot be run as the game's own documents say fails, and so does every item that needed it; say so once and go on.".to_owned(),
        "- You may read the game's files to choose inputs and to explain a failure. Do not repair anything.".to_owned(),
    ];
    if text(r, "arm") == Some("engine") {
        lines.push(format!(
            "- This game runs on the Specula engine. `{tool} play {rd} ...` is the engine's own headless run of it (it needs no engine started); if its documents use an engine left running, start it with `{tool} engine start {rd}` and stop it with `{tool} engine stop {rd}` when you are done, and calls go through `{tool} call {rd} TOOL '{{json}}'`. Never run the engine's executable yourself."
        ));
    }
    lines.push(String::new());
    lines.push("The list".to_owned());
    lines.extend(items.iter().map(|a| format!("- **{}** ({}): {}", text(a, "id").unwrap_or(""), text(a, "kind").unwrap_or(""), text(a, "text").unwrap_or(""))));
    if let Some(of) = text(&j, "change_of") {
        let base = job(of)?;
        let from = text(r, "from").unwrap_or("");
        let before_run = absolute(run).parent().map(|p| p.join(from)).unwrap_or_default();
        let before = read_cost(&before_run)?.get("verdict").cloned().filter(|v| truthy(Some(v)));
        let Some(before) = before else { return Err(format!("judge {from} first: the change's judge is told what the game passed before the change")) };
        let was = before.get("items").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default();
        lines.push(String::new());
        lines.push(
            "The list of the game before the change, as its own judge found it (read each with the change in mind). The item that says nothing may be broken holds the changed game to the ones that PASSED then, and only to those:"
                .to_owned(),
        );
        for a in base.get("acceptance").and_then(Json::as_array).unwrap_or(&[]) {
            let id = text(a, "id").unwrap_or("");
            let then = match was.iter().find(|i| text(i, "id") == Some(id)) {
                Some(i) if truthy(i.get("pass")) => "passed before the change".to_owned(),
                Some(i) => format!("FAILED before the change (not held against it): {}", text(i, "evidence").unwrap_or("")),
                None => "FAILED before the change (not held against it): not judged".to_owned(),
            };
            lines.push(format!("- **{id}**: {} [{then}]", text(a, "text").unwrap_or("")));
        }
    }
    lines.extend(
        [
            "",
            "Your answer",
            "Write `verdict.json` beside this file, and nothing else outside `work/`:",
            "```json",
            "{\"items\": [{\"id\": \"s01\", \"pass\": true, \"evidence\": \"what you ran and what it printed, in a line or two\"}],",
            " \"defects\": [\"anything wrong that the list does not name, one line each\"],",
            " \"could_not_check\": [\"an item's id and why, if you could not decide it\"]}",
            "```",
            "Every item of the list is in `items` once. An item you could not decide is `\"pass\": false` and named in `could_not_check`.",
        ]
        .map(str::to_owned),
    );
    Ok(lines.join("\n") + "\n")
}

/// An earlier judging goes into the record (`judged_before`) with why there is another, and
/// its verdict file is kept under a number: a verdict is never written over.
fn keep_judging(run: &Path, mut had: Json, why: &str) -> R<()> {
    let run = absolute(run);
    let earlier = Json::obj([("why", Json::Str(why.to_owned())), ("verdict", take(&mut had, "verdict").unwrap_or(Json::Null)), ("judge", take(&mut had, "judge").unwrap_or(Json::Null))]);
    let mut before = take(&mut had, "judged_before").and_then(|b| b.as_array().map(<[Json]>::to_vec)).unwrap_or_default();
    before.push(earlier);
    let n = before.len();
    had.set("judged_before", Json::Arr(before));
    write_cost(&run, &had)?;
    let old = run.join("verdict.json");
    if old.is_file() {
        std::fs::rename(&old, run.join(format!("verdict.{n}.json"))).map_err(|e| format!("verdict.json: {e}"))?;
    }
    Ok(())
}

/// `judge RUN [--verdict FILE [--again WHY]]`: without a verdict, writes the judge's page and
/// says what the judge gets; with one, scores it and records it. Returns what to print.
pub fn judge(run: &Path, verdict: Option<&Path>, again: Option<&str>) -> R<Json> {
    let r = read_run(run)?;
    let run = absolute(run);
    let Some(verdict) = verdict else {
        if text(&r, "phase") != Some("judge") {
            return Err("'finish' the run first: the judge starts from what the builder handed in".to_owned());
        }
        std::fs::write(run.join("JUDGE.md"), judge_text(&run, &r)?).map_err(|e| format!("JUDGE.md: {e}"))?;
        let s = settings()?;
        let lim = s.get("judge").cloned().unwrap_or(Json::Null);
        return Ok(Json::obj([
            ("judge", Json::Str(posix(&run.join("JUDGE.md")))),
            ("model", s.get("judge_model").cloned().unwrap_or(Json::Null)),
            ("token_cap", lim.get("token_cap").cloned().unwrap_or(Json::Null)),
            ("wall_s", lim.get("wall_s").cloned().unwrap_or(Json::Null)),
        ]));
    };
    let v = load_json(verdict)?;
    let (_, items) = acceptance_of(&r)?;
    let problems = check_verdict(&v, &items);
    if !problems.is_empty() {
        return Err(format!("verdict: {}", problems.join("; ")));
    }
    let mut c = read_cost(&run)?;
    if truthy(c.get("verdict")) {
        // (By hand, on a run that has a verdict: kept, with why, or refused.)
        let Some(why) = again else { return Err("this run has a verdict already ('judge RUN --verdict FILE --again WHY' records another and keeps the first)".to_owned()) };
        let kept = std::fs::read_to_string(absolute(verdict)).map_err(|e| format!("{}: {e}", posix(verdict)))?;
        keep_judging(&run, c, why)?;
        std::fs::write(run.join("verdict.json"), kept).map_err(|e| format!("verdict.json: {e}"))?;
        c = read_cost(&run)?;
    }
    let scored = score(&v, &items);
    let totals = scored.get("totals").cloned().unwrap_or(Json::Null);
    c.set("verdict", scored);
    let mut by = c.get("judge").cloned().filter(|j| j.as_object().is_some()).unwrap_or(Json::Obj(Vec::new()));
    if text(&r, "arm") == Some("engine") {
        by.set("engine_stopped", Json::Bool(stop_engine(&r, &run)));
        by.set("engine", call_totals(&run, "judge")?);
    }
    c.set("judge", by);
    write_cost(&run, &c)?;
    Ok(totals)
}

pub fn check_verdict(v: &Json, items: &[Json]) -> Vec<String> {
    let Some(got) = v.get("items").and_then(Json::as_array) else { return vec!["items must be a list".to_owned()] };
    let mut out = Vec::new();
    let ids: Vec<&str> = got.iter().filter_map(|i| text(i, "id")).collect();
    for a in items {
        let id = text(a, "id").unwrap_or("");
        let times = ids.iter().filter(|x| **x == id).count();
        if times != 1 {
            out.push(format!("{id} must be answered once (it is {times} times)"));
        }
    }
    for i in got {
        let good = i.as_object().is_some() && i.get("pass").and_then(Json::as_bool).is_some() && text(i, "evidence").is_some_and(|e| !e.trim().is_empty());
        if !good {
            out.push(format!("an item needs id, pass (true or false) and evidence: {}", echo(i)));
        }
    }
    for k in ["defects", "could_not_check"] {
        if v.get(k).is_some_and(|x| x.as_array().is_none()) {
            out.push(format!("{k} must be a list"));
        }
    }
    out
}

/// A checked verdict as the record keeps it: each item of the list with its kind, and totals.
pub fn score(v: &Json, items: &[Json]) -> Json {
    let got = v.get("items").and_then(Json::as_array).unwrap_or(&[]);
    let rows: Vec<Json> = items
        .iter()
        .map(|a| {
            let id = text(a, "id").unwrap_or("");
            let by = got.iter().find(|i| text(i, "id") == Some(id));
            Json::obj([
                ("id", Json::Str(id.to_owned())),
                ("kind", Json::Str(text(a, "kind").unwrap_or("").to_owned())),
                ("pass", Json::Bool(by.is_some_and(|i| truthy(i.get("pass"))))),
                ("evidence", Json::Str(by.and_then(|i| text(i, "evidence")).unwrap_or("").to_owned())),
            ])
        })
        .collect();
    let count = |of: &dyn Fn(&&Json) -> bool| {
        let of: Vec<&Json> = rows.iter().filter(of).collect();
        Json::obj([("passed", Json::Num(of.iter().filter(|r| truthy(r.get("pass"))).count() as f64)), ("of", Json::Num(of.len() as f64))])
    };
    let mut totals: Vec<(String, Json)> = KINDS.iter().map(|kind| ((*kind).to_owned(), count(&|r| text(r, "kind") == Some(kind)))).collect();
    totals.push(("all".to_owned(), count(&|_| true)));
    let list = |k: &str| Json::Arr(v.get(k).and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default());
    Json::obj([("items", Json::Arr(rows)), ("totals", Json::Obj(totals)), ("defects", list("defects")), ("could_not_check", list("could_not_check"))])
}

// ---- the command line -----------------------------------------------------------------------

fn words(args: crate::cli::Args, what: &str, least: usize, most: usize) -> R<Vec<String>> {
    let rest = args.rest();
    if rest.len() < least || rest.len() > most {
        return Err(format!("usage: examen leverage {what}"));
    }
    Ok(rest)
}

fn number(text: &str, what: &str) -> R<f64> {
    text.parse::<f64>().ok().filter(|n| n.is_finite() && *n >= 0.0).ok_or_else(|| format!("{what} takes a number, not {text:?}"))
}

fn dispatch(mut args: crate::cli::Args) -> R<i32> {
    if args.flag("--check") {
        let problems = check_all();
        for line in &problems {
            println!("{line}");
        }
        println!("leverage data: {}", if problems.is_empty() { "ok".to_owned() } else { format!("{} problem(s)", problems.len()) });
        return Ok(i32::from(!problems.is_empty()));
    }
    let mut all = args.rest();
    if all.is_empty() {
        return Err("usage: examen leverage --check | prepare | engine | call | tools | play | run | finish | record | usage | judge | report | export | where | trace (examen help says what each takes)".to_owned());
    }
    let sub = all.remove(0);
    let mut args = crate::cli::Args::new(all);
    match sub.as_str() {
        "prepare" => {
            let out = args.value("--out")?.ok_or("prepare takes --out ROOT")?;
            let (exe, from, build) = (args.value("--exe")?, args.value("--from")?, args.value("--build")?);
            let port = args.value("--port")?.map(|p| p.parse::<i64>().map_err(|_| format!("--port takes a whole number, not {p:?}"))).transpose()?;
            let w = words(args, "prepare JOB ARM N --out ROOT [--exe SPECULA_EXE] [--port P] [--from RUN] [--build NAME]", 3, 3)?;
            let n = w[2].parse::<i64>().map_err(|_| format!("N is a whole number, not {:?}", w[2]))?;
            println!("{}", prepare(&Prepare { job: w[0].clone(), arm: w[1].clone(), n, out: PathBuf::from(out), exe: exe.map(PathBuf::from), port, from: from.map(PathBuf::from), build })?);
            Ok(0)
        }
        "engine" => {
            let w = words(args, "engine start|stop RUN", 2, 2)?;
            if !matches!(w[0].as_str(), "start" | "stop") {
                return Err("usage: examen leverage engine start|stop RUN".to_owned());
            }
            engine_cmd(&w[0], Path::new(&w[1]))
        }
        // (No flags: a tool's arguments may be any text.)
        "call" => {
            let w = words(args, "call RUN TOOL ['{json}' | @FILE]", 2, 3)?;
            call_cmd(Path::new(&w[0]), &w[1], w.get(2).map(String::as_str))
        }
        "play" => {
            let rest = args.rest();
            let Some((run, flags)) = rest.split_first() else { return Err("usage: examen leverage play RUN [--ticks N --inputs ... (specula run's flags)]".to_owned()) };
            play_cmd(Path::new(run), flags)
        }
        "tools" => {
            let w = words(args, "tools RUN [FILTER]", 1, 2)?;
            tools_cmd(Path::new(&w[0]), w.get(1).map(String::as_str))
        }
        "finish" => {
            let w = words(args, "finish RUN", 1, 1)?;
            println!("{}", finish(Path::new(&w[0]))?);
            Ok(0)
        }
        "run" => {
            let (judge, again, note) = (args.flag("--judge"), args.value("--again")?, args.value("--note")?);
            let w = words(args, "run RUN [--judge [--again WHY]] [--note TEXT]", 1, 1)?;
            run_cmd(&RunArgs { run: PathBuf::from(&w[0]), judge, again, note })
        }
        "record" => {
            let need = |v: Option<String>, name: &str| v.ok_or_else(|| format!("record takes {name}"));
            let tokens = number(&need(args.value("--tokens")?, "--tokens N")?, "--tokens")?;
            let tool_uses = number(&need(args.value("--tool-uses")?, "--tool-uses N")?, "--tool-uses")?;
            let duration_s = number(&need(args.value("--duration-s")?, "--duration-s N")?, "--duration-s")?;
            let ended = args.value("--ended")?.unwrap_or_else(|| "done".to_owned());
            if !matches!(ended.as_str(), "done" | "wall" | "stopped") {
                return Err("--ended is one of: done, wall, stopped".to_owned());
            }
            let by_kind = match args.list("--by-kind")? {
                list if list.is_empty() => None,
                list if list.len() == 4 => Some([number(&list[0], "--by-kind")?, number(&list[1], "--by-kind")?, number(&list[2], "--by-kind")?, number(&list[3], "--by-kind")?]),
                _ => return Err("--by-kind takes four numbers with commas: INPUT,CACHE_WRITE,CACHE_READ,OUTPUT".to_owned()),
            };
            let (note, usage, judge) = (args.value("--note")?, args.value("--usage")?.map(PathBuf::from), args.flag("--judge"));
            let w = words(args, "record RUN --tokens N --tool-uses N --duration-s N [...]", 1, 1)?;
            println!("{}", record(&Record { run: PathBuf::from(&w[0]), tokens, tool_uses, duration_s, ended, note, usage, by_kind, judge })?);
            Ok(0)
        }
        "usage" => {
            let w = words(args, "usage TRANSCRIPT", 1, 1)?;
            println!("{}", transcript_usage(Path::new(&w[0]))?);
            Ok(0)
        }
        "judge" => {
            let (verdict, again) = (args.value("--verdict")?.map(PathBuf::from), args.value("--again")?);
            let w = words(args, "judge RUN [--verdict FILE [--again WHY]]", 1, 1)?;
            println!("{}", judge(Path::new(&w[0]), verdict.as_deref(), again.as_deref())?);
            Ok(0)
        }
        "report" => {
            let w = words(args, "report ROOT", 1, 1)?;
            let said = crate::leverage_report::report(Path::new(&w[0]))?;
            print!("{}", said.chars().map(|c| if c.is_ascii() { c } else { '?' }).collect::<String>());
            Ok(0)
        }
        "export" => {
            let w = words(args, "export ROOT DEST", 2, 2)?;
            println!("{}", crate::leverage_report::export(Path::new(&w[0]), Path::new(&w[1]))?);
            Ok(0)
        }
        "where" => {
            let json = args.flag("--json");
            let w = words(args, "where RUN [RUN ...] [--json]", 1, 64)?;
            let found: Vec<Json> = w.iter().map(|run| crate::leverage_report::where_of(Path::new(run))).collect::<R<_>>()?;
            if json {
                println!("{}", Json::Arr(found).to_pretty());
            } else {
                for one in &found {
                    println!("{}", crate::leverage_report::where_text(one));
                }
            }
            Ok(0)
        }
        "trace" => {
            let (json, judge) = (args.flag("--json"), args.flag("--judge"));
            let w = words(args, "trace RUN [RUN ...] [--judge] [--json]", 1, 64)?;
            let found: Vec<Json> = w.iter().map(|run| crate::leverage_trace::trace_of(Path::new(run), judge)).collect::<R<_>>()?;
            if json {
                println!("{}", Json::Arr(found).to_pretty());
            } else {
                for one in &found {
                    println!("{}", crate::leverage_trace::trace_text(one));
                }
            }
            Ok(0)
        }
        other => Err(format!("'{other}' is not a command of examen leverage (--check, prepare, engine, call, tools, play, run, finish, record, usage, judge, report, export, where, trace)")),
    }
}

/// `examen leverage ...`. What cannot be done is said as one JSON object, with exit code 1.
pub fn main(args: crate::cli::Args) -> i32 {
    // An engine this tool starts (`engine start`, through `specula open`) outlives it. On
    // Windows a child inherits every inheritable handle of its parent, whatever its own
    // standard handles are: the engine held this command's output pipe open, and whoever read
    // the command's answer (a builder's shell) went on waiting for the engine to end. Found by
    // the first dry run of this port (the script it was did not pass its handles on).
    specula_sys::process::keep_std_handles();
    match dispatch(args) {
        Ok(code) => code,
        Err(message) => {
            println!("{}", Json::obj([("error", Json::Str("leverage".to_owned())), ("message", Json::Str(message))]));
            1
        }
    }
}

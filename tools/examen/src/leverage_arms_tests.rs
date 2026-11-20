//! The harness with its arms as data (docs/design/LEVERAGE.md, "Another data folder"): the
//! default data's prompts unchanged to the byte, arms read and refused, the shape library's
//! three arms, and a report measured against a named baseline. Nothing is started.

use std::path::{Path, PathBuf};

use specula_core::Json;

use crate::leverage::{self as lev, absolute, commas, posix, repo, text, whole};
use crate::leverage_arms::{self as arms, Exes, Kind};
use crate::leverage_report as rep;

fn j(text: &str) -> Json {
    Json::parse(text).unwrap_or_else(|e| panic!("{text}: {}", e.message))
}

fn props_data() -> PathBuf {
    repo().join("evals").join("leverage-props")
}

/// A folder of the test's own, removed when it ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Scratch {
        let tmp = std::env::temp_dir().join(format!("examen-arms-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        Scratch(tmp)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---- the brief as it was made before arms were data (copied unchanged, to hold it) ----------

fn before_arm_text(arm: &str, run: &Path, change: bool) -> Vec<String> {
    let (tool, rd) = lev::me(run);
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
        if change {
            "- The game is built with the Specula engine: your work folder is a Specula project (a game file, screens and words, a module with the rules). Start with one read, from that folder: `cat notes/how.md README.md game.json modules/*/src/lib.rs modules/*/actions.json $(ls assets/ui/*.json | grep -v /theme.json) assets/strings/*.json; ls checks` (how such a game is built and run; what its builder says of it; the game file, the rules, the inputs, the screens, the words; `assets/ui/theme.json` is the look and needs no reading). That is the game: no other reading is needed to change it.".to_owned()
        } else {
            "- You build it with the Specula engine. Your work folder is a Specula project made by the engine (`specula new`): a small sample game that builds and plays, with every part in place. Start with one read, from that folder: `cat notes/how.md game.json modules/*/src/lib.rs modules/*/actions.json assets/ui/hud.json assets/ui/settings.json assets/strings/en.json` (how such a game is built and run; the sample's game file, rules, inputs, two of its screens and its words). That is the reading done: notes/how.md is complete for a game of this size. Replace the sample's rules, screens and words with your game's, and delete the sample's `checks` folder.".to_owned()
        },
        format!("- Run the game with no window with `{tool} play {rd} --ticks N [--inputs T:WORD,T:WORD,...] [--seed N] [--settings FOLDER] [--frame FILE.png --size WxH] [--follow]`: the engine's own `specula run` on your work folder (notes/how.md says what it takes and what it prints). It builds the module first and prints the game's state as JSON; no engine need be started for it. Paths in it are taken from the folder you run it in."),
        format!("- Also there, and not needed to build or run a game: `{tool} check {rd} [--only NAME]` (the engine's own `specula check`: it plays every run that `checks/runs.json` lists and says which did not hold; notes/how.md); and an engine left running, with its tools (`{tool} engine start {rd}`, `{tool} engine stop {rd}`, `{tool} tools {rd} [FILTER]`, `{tool} call {rd} TOOL '{{json}}'` or `TOOL @args.json`). Never run the engine's executable yourself."),
        format!("- notes/how.md is written to be enough for a game of this size. Where it is not, the engine's documents are in {} (design notes for each area in `docs/design/`), with {}.", posix(&repo.join("docs")), posix(&repo.join("README.md"))),
        "- The two checks the brief asks for can be the `play` command, with the game file's `report` saying what it prints; or small scripts of yours.".to_owned(),
        "- Whoever checks the game starts with no engine running: the commands in your README must work from there.".to_owned(),
    ]
}

fn before_brief_text(j: &Json, arm: &str, run: &Path, s: &Json) -> String {
    let id = text(j, "id").unwrap_or("");
    let lim = s.get("jobs").and_then(|jobs| jobs.get(id)).cloned().unwrap_or(Json::Null);
    let mut lines = vec![format!("# {}", text(j, "title").unwrap_or("")), String::new()];
    lines.extend(j.get("brief").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(str::to_owned));
    lines.push(String::new());
    lines.extend(before_arm_text(arm, run, j.get("change_of").is_some()));
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

/// Every job and arm of `evals/leverage` is told, to the byte, what it was told before arms
/// were data; and `prompt` prints those briefs.
#[test]
fn the_default_datas_prompts_are_unchanged_to_the_byte() {
    let s = lev::settings().unwrap();
    let all = arms::arms_of(&s).unwrap();
    assert_eq!(all.iter().map(|a| (a.name.as_str(), a.kind, a.text.is_none())).collect::<Vec<_>>(), [("engine", Kind::Engine, true), ("plain", Kind::Plain, true)]);
    assert_eq!(arms::baseline_of(&s), "plain");
    let root = absolute(Path::new("ROOT"));
    let jobs: Vec<String> = s.get("jobs").and_then(Json::as_object).unwrap().iter().map(|(k, _)| k.clone()).collect();
    assert!(jobs.len() >= 4 && jobs.iter().any(|k| lev::job(k).unwrap().get("change_of").is_some()), "a change job is among them: {jobs:?}");
    let printed = lev::prompts(&lev::default_data(), None, None, &root, &Exes::default()).unwrap();
    for name in &jobs {
        let job = lev::job(name).unwrap();
        for arm in &all {
            let run = root.join(format!("{name}-{}-1", arm.name));
            let before = before_brief_text(&job, &arm.name, &run, &s);
            assert_eq!(lev::brief_text(&job, arm, &run, &s, None).unwrap(), before, "{name} / {}", arm.name);
            // (An executable changes nothing for an arm whose text is the code's.)
            assert_eq!(lev::brief_text(&job, arm, &run, &s, Some(Path::new("C:/elsewhere/specula.exe"))).unwrap(), before);
            assert!(printed.contains(&before), "prompt prints {name} / {}", arm.name);
        }
    }
}

/// The settings a refusal is tried on: the props comparison's, with `path` set to `value`.
fn props_with<T>(edit: impl FnOnce(&mut Json) -> T) -> Json {
    let mut s = lev::load_json(&props_data().join("settings.json")).unwrap();
    edit(&mut s);
    s
}

fn arm_set(s: &mut Json, i: usize, key: &str, value: Json) {
    let mut list = s.get("arms").and_then(Json::as_array).unwrap().to_vec();
    list[i].set(key, value);
    s.set("arms", Json::Arr(list));
}

#[test]
fn arms_as_data_are_read_and_refused_saying_what_and_where() {
    let good = props_with(|_| {});
    assert_eq!(lev::check_settings(&good), Vec::<String>::new());
    let problems = |s: &Json| lev::check_settings(s).join(" | ");
    // An unknown kind.
    let said = problems(&props_with(|s| arm_set(s, 1, "kind", Json::Str("hybrid".to_owned()))));
    assert!(said.contains("arms[1] (found): kind must be one of") && said.contains("\"hybrid\""), "{said}");
    // Two arms with one name.
    let said = problems(&props_with(|s| arm_set(s, 2, "name", Json::Str("none".to_owned()))));
    assert!(said.contains("arms[2] (none): the name none is arms[0]'s too"), "{said}");
    // A baseline that is not an arm, and none at all.
    let said = problems(&props_with(|s| s.set("baseline", Json::Str("plain".to_owned()))));
    assert!(said.contains("baseline names no arm (\"plain\"; the arms are none, found, told, guided)"), "{said}");
    let said = problems(&props_with(|s| {
        lev::take(s, "baseline");
    }));
    assert!(said.contains("baseline must name the arm"), "{said}");
    // A placeholder nobody fills, a brace left open, a plain arm with an engine, a bad key.
    let said = problems(&props_with(|s| arm_set(s, 0, "text", j(r#"["What you have", "- run {engine}"]"#))));
    assert!(said.contains("arms[0] (none): text[1]: {engine} is not a placeholder"), "{said}");
    let said = problems(&props_with(|s| arm_set(s, 0, "text", j(r#"["What you have", "- call '{json'"]"#))));
    assert!(said.contains("arms[0] (none): text[1]: a '{' that is not closed"), "{said}");
    let said = problems(&props_with(|s| arm_set(s, 1, "kind", Json::Str("plain".to_owned()))));
    assert!(said.contains("arms[1] (found): exe names the engine an arm runs, and a plain arm runs none"), "{said}");
    let said = problems(&props_with(|s| arm_set(s, 1, "exe", Json::Str("Library Build".to_owned()))));
    assert!(said.contains("arms[1] (found): exe must be a key"), "{said}");
    let said = problems(&props_with(|s| arm_set(s, 1, "text", Json::Arr(Vec::new()))));
    assert!(said.contains("arms[1] (found): text must be a list of lines"), "{said}");
    let said = problems(&props_with(|s| arm_set(s, 1, "name", Json::Str("Found It".to_owned()))));
    assert!(said.contains("arms[1] (Found It): name must be"), "{said}");
    let said = problems(&props_with(|s| s.set("arms", j(r#"["engine", {"name": "x", "kind": "plain", "text": ["a"]}]"#))));
    assert!(said.contains("arms[0]: an arm is an object"), "{said}");
    // Words as before: only the two, and a baseline among them.
    let default = lev::settings().unwrap();
    let mut three = default.clone();
    three.set("arms", j(r#"["engine", "plain", "library"]"#));
    assert!(lev::check_settings(&three).join(" | ").contains("arms must be [\"engine\", \"plain\"]"));
    let mut based = default.clone();
    based.set("baseline", Json::Str("engine".to_owned()));
    assert_eq!(lev::check_settings(&based), Vec::<String>::new());
    assert_eq!(arms::baseline_of(&based), "engine");
    based.set("baseline", Json::Str("none".to_owned()));
    assert!(lev::check_settings(&based).join(" | ").contains("baseline names no arm"));
}

#[test]
fn an_engine_arm_with_no_executable_resolved_is_refused_and_nothing_is_made() {
    let s = Scratch::new("exe");
    let out = s.0.join("runs");
    let real = std::env::current_exe().unwrap();
    let mut a = lev::Prepare { job: "kit".into(), arm: "found".into(), n: 1, out: out.clone(), exe: Exes::default(), port: None, from: None, build: None, data: Some(props_data()) };
    let refused = |a: &lev::Prepare| lev::prepare(a).expect_err("refused");
    assert!(refused(&a).contains("the found arm runs the engine named library: give it as --exe library=PATH"), "{}", refused(&a));
    // Another key, and a path with no key, are not this arm's engine.
    a.exe = Exes::parse(&[format!("base={}", real.display()), real.display().to_string()]).unwrap();
    assert!(refused(&a).contains("--exe library=PATH"));
    // The key given, to a file that is not there.
    a.exe = Exes::parse(&[format!("library={}", s.0.join("missing.exe").display())]).unwrap();
    assert!(refused(&a).contains("is not a file"));
    // An arm the data has not.
    a.arm = "engine".into();
    assert!(refused(&a).contains("arm must be one of [\"none\", \"found\", \"told\", \"guided\"]"));
    assert!(!out.exists() || std::fs::read_dir(&out).unwrap().next().is_none(), "a refused prepare makes no run");
    // The default data's engine arm still asks for --exe as it did.
    let d = lev::Prepare { job: "small".into(), arm: "engine".into(), n: 1, out: out.clone(), exe: Exes::default(), port: None, from: None, build: None, data: None };
    assert_eq!(refused(&d), "the engine arm needs --exe: the specula executable (a release build)");
}

#[test]
fn exe_values_are_keyed_or_plain_paths() {
    let e = Exes::parse(&["base=C:/b/specula.exe".into(), "library=D:/l/specula.exe".into(), "C:/x=y/specula.exe".into()]).unwrap();
    assert_eq!(e.0.get("base"), Some(&PathBuf::from("C:/b/specula.exe")));
    assert_eq!(e.0.get("library"), Some(&PathBuf::from("D:/l/specula.exe")));
    assert_eq!(e.0.get(arms::UNKEYED), Some(&PathBuf::from("C:/x=y/specula.exe")), "a drive is no key");
    assert!(Exes::parse(&["base=a".into(), "base=b".into()]).unwrap_err().contains("--exe base=... is given twice"));
    assert!(Exes::parse(&["a".into(), "b".into()]).unwrap_err().contains("twice"));
    assert!(Exes::parse(&["base=".into()]).is_err());
}

#[test]
fn placeholders_are_filled_and_braces_kept() {
    let v = [("tool", "T"), ("run", "\"R\""), ("work", "W"), ("docs", "D")];
    assert_eq!(arms::fill("`{tool} call {run} x '{{json}}'` in {work}, {docs}", &v).unwrap(), "`T call \"R\" x '{json}'` in W, D");
    assert_eq!(arms::fill("'{{}}' and '{{\"a\": 1}}'", &v).unwrap(), "'{}' and '{\"a\": 1}'");
    assert!(arms::fill("{nope}", &v).unwrap_err().contains("{nope} is not a placeholder"));
    assert!(arms::fill("x }", &v).unwrap_err().contains("opens nothing"));
}

/// The shape library's comparison: its data passes the harness's check; `none` and `found`
/// are told the same to the byte, on two different engines; `told` only adds its line.
#[test]
fn the_shape_librarys_three_arms() {
    let data = props_data();
    assert_eq!(lev::check_all_in(&data), Vec::<String>::new());
    let s = lev::settings_in(&data).unwrap();
    let all = arms::arms_of(&s).unwrap();
    let names: Vec<(&str, Kind, &str)> = all.iter().map(|a| (a.name.as_str(), a.kind, a.exe.as_str())).collect();
    // (`guided`, 2026-11-21: after the engine was given a page on models in a new project's
    // notes, an arm whose brief points at that page and no longer at a design document.)
    assert_eq!(names, [("none", Kind::Engine, "base"), ("found", Kind::Engine, "library"), ("told", Kind::Engine, "library"), ("guided", Kind::Engine, "library")]);
    let guided = all[3].text.as_ref().unwrap();
    assert!(guided.iter().any(|l| l.contains("notes/models.md")) && !guided.iter().any(|l| l.contains("PROCEDURAL_MODELLING") || l.contains("formae") || l.contains("librar")), "{guided:?}");
    assert_eq!(arms::baseline_of(&s), "none");
    assert_eq!(s.get("runs_per_cell").and_then(Json::as_f64), Some(2.0));
    let (none, found, told) = (&all[0], &all[1], &all[2]);
    assert_eq!(none.text, found.text, "none and found are told the same, to the byte");
    let (n, t) = (none.text.as_ref().unwrap(), told.text.as_ref().unwrap());
    assert_eq!(&t[..n.len()], &n[..], "told is none's text and one line more");
    assert_eq!(t.len(), n.len() + 1);
    assert!(t[n.len()].contains("formae_list") && t[n.len()].contains("formae_get"));
    for line in n {
        assert!(!line.contains("formae") && !line.contains("librar"), "none's text names the library: {line}");
    }
    // Each job's brief: one text, the arms' lines apart (the same run folder and engine for all
    // three, so that only the arm's text can differ: in a real run each is in its own folder,
    // and `{docs}` is its own engine's checkout).
    let run = absolute(Path::new("ROOT")).join("kit-x-1");
    let nowhere = std::env::temp_dir().join("examen-arms-no-checkout").join("specula.exe");
    let exe = Some(nowhere.as_path());
    let here_exe = repo().join("target").join("release").join("specula.exe");
    for name in ["kit", "props"] {
        let job = lev::job_in(&data, name).unwrap();
        let brief = |arm| lev::brief_text(&job, arm, &run, &s, Some(&here_exe)).unwrap();
        assert_eq!(brief(none), brief(found), "{name}");
        let (bn, bt) = (brief(none), brief(told));
        let extra: Vec<&str> = bt.lines().filter(|l| !bn.lines().any(|m| m == *l)).collect();
        assert_eq!(extra.len(), 1, "{name}: told differs by one line: {extra:?}");
        assert_eq!(bn.lines().count() + 1, bt.lines().count());
        // Its placeholders are filled; an executable outside any checkout has no {docs}.
        assert!(!bn.contains("{tool}") && !bn.contains("{run}") && bn.contains("TOOL '{json}'") && bn.contains(&format!("\"{}\"", posix(&run))));
        assert!(bt.contains("formae_get '{\"name\": \"gear\"}'"));
        assert!(lev::brief_text(&job, none, &run, &s, exe).unwrap_err().contains("{docs}"));
    }
    // With this checkout's own engine, {docs} is this checkout's documents.
    assert_eq!(arms::engine_docs(&here_exe), Some(repo().join("docs")));
    let job = lev::job_in(&data, "kit").unwrap();
    assert!(lev::brief_text(&job, none, &run, &s, Some(&here_exe)).unwrap().contains(&format!("its documents are in {} (", posix(&repo().join("docs")))));
    // The words a props brief may not name are its own: it may say "the engine".
    assert!(arms::brief_names_no(&s).iter().all(|w| w != "engine"));
    let mut said_engine = job.clone();
    let mut lines = job.get("brief").and_then(Json::as_array).unwrap().to_vec();
    lines.push(Json::Str("Build them with the engine.".to_owned()));
    said_engine.set("brief", Json::Arr(lines.clone()));
    assert_eq!(lev::check_job_with(&said_engine, "kit", &arms::brief_names_no(&s)), Vec::<String>::new());
    assert!(!lev::check_job(&said_engine, "kit").is_empty(), "the default data's words still hold for its own jobs");
    lines.push(Json::Str("Use the shape library.".to_owned()));
    said_engine.set("brief", Json::Arr(lines));
    assert!(lev::check_job_with(&said_engine, "kit", &arms::brief_names_no(&s)).iter().any(|p| p.contains("names no tool (library)")));
    // `prompt` prints every arm of every job.
    let printed = lev::prompts(&data, Some("kit"), None, Path::new("ROOT"), &Exes::default()).unwrap();
    for arm in ["none", "found", "told"] {
        assert!(printed.contains(&format!("===== kit / {arm}: ")), "{arm}");
    }
    assert!(printed.contains("<the docs folder of the engine given as --exe base=PATH>"));
    assert!(lev::prompts(&data, Some("small"), None, Path::new("ROOT"), &Exes::default()).is_err());
}

#[test]
fn a_run_keeps_its_data_folder_and_a_later_command_is_held_to_it() {
    let s = Scratch::new("data");
    let a = lev::Prepare { job: "small".into(), arm: "plain".into(), n: 1, out: s.0.join("runs"), exe: Exes::default(), port: None, from: None, build: None, data: None };
    lev::prepare(&a).unwrap();
    let r = lev::read_run(&s.0.join("runs").join("small-plain-1")).unwrap();
    assert_eq!((text(&r, "data"), text(&r, "kind")), (Some("evals/leverage"), Some("plain")));
    assert_eq!(r.get("settings").and_then(|x| text(x, "baseline")), Some("plain"));
    assert_eq!(absolute(&lev::run_data(&r)), absolute(&lev::default_data()));
    assert!(lev::run_data_checked(&r, None).is_ok() && lev::run_data_checked(&r, Some(&lev::default_data())).is_ok());
    assert!(lev::run_data_checked(&r, Some(&props_data())).unwrap_err().contains("this run was prepared from the data in"));
    // A run made before runs kept one is the default's; its kind is its arm's name.
    let old = j(r#"{"arm": "engine"}"#);
    assert_eq!((absolute(&lev::run_data(&old)), arms::kind_of_run(&old)), (absolute(&lev::default_data()), Kind::Engine));
    assert_eq!(arms::kind_of_run(&j(r#"{"arm": "told", "kind": "engine"}"#)), Kind::Engine);
    assert_eq!(arms::kind_of_run(&j(r#"{"arm": "told"}"#)), Kind::Plain);
    assert_eq!(absolute(&lev::data_named(Path::new("evals/leverage-props"))), absolute(&props_data()));
}

/// A run of the props comparison's, as `prepare` and `run` leave it, with `weighted` tokens,
/// `requests` requests and the items of `failed` failed.
fn props_run(root: &Path, arm: &str, n: u32, weighted: f64, requests: f64, failed: &[&str]) {
    let data = props_data();
    let s = lev::settings_in(&data).unwrap();
    let d = root.join(format!("kit-{arm}-{n}"));
    std::fs::create_dir_all(d.join("work")).unwrap();
    let r = Json::obj([
        ("leverage", Json::Num(1.0)),
        ("job", Json::Str("kit".to_owned())),
        ("arm", Json::Str(arm.to_owned())),
        ("kind", Json::Str("engine".to_owned())),
        ("data", Json::Str("evals/leverage-props".to_owned())),
        ("n", Json::Num(f64::from(n))),
        ("phase", Json::Str("judge".to_owned())),
        ("engine_build", Json::Str(if arm == "none" { "base" } else { "library" }.to_owned())),
        (
            "settings",
            Json::obj([
                ("limits", s.get("jobs").and_then(|jobs| jobs.get("kit")).cloned().unwrap()),
                ("builder_model", Json::Str("builder".to_owned())),
                ("judge_model", Json::Str("judge".to_owned())),
                ("runs_per_cell", Json::Num(2.0)),
                ("token_weights", s.get("token_weights").cloned().unwrap()),
                ("baseline", Json::Str("none".to_owned())),
            ]),
        ),
    ]);
    std::fs::write(d.join("run.json"), r.to_string()).unwrap();
    let items: Vec<Json> = lev::job_in(&data, "kit").unwrap().get("acceptance").and_then(Json::as_array).unwrap().to_vec();
    let rows = items.iter().map(|a| {
        let id = text(a, "id").unwrap();
        Json::obj([("id", Json::Str(id.to_owned())), ("pass", Json::Bool(!failed.contains(&id))), ("evidence", Json::Str("seen".to_owned()))])
    });
    let v = Json::obj([("items", Json::arr(rows)), ("defects", Json::Arr(Vec::new())), ("could_not_check", Json::Arr(Vec::new()))]);
    let by = Json::obj([("input", Json::Num(0.0)), ("cache_write", Json::Num(0.0)), ("cache_read", Json::Num(0.0)), ("output", Json::Num(0.0)), ("requests", Json::Num(requests)), ("context_at_end", Json::Num(1.0))]);
    let builder = Json::obj([("weighted", Json::Num(weighted)), ("by_kind", by), ("duration_s", Json::Num(600.0)), ("ended", Json::Str("done".to_owned())), ("tokens_complete", Json::Bool(true))]);
    lev::write_cost(&d, &Json::obj([("builder", builder), ("verdict", lev::score(&v, &items))])).unwrap();
}

#[test]
fn the_report_measures_every_arm_against_the_named_baseline() {
    let s = Scratch::new("report");
    let root = s.0.join("runs");
    props_run(&root, "none", 1, 100_000.0, 10.0, &[]);
    props_run(&root, "none", 2, 200_000.0, 20.0, &["k03"]);
    props_run(&root, "found", 1, 300_000.0, 30.0, &[]);
    props_run(&root, "found", 2, 300_000.0, 30.0, &[]);
    props_run(&root, "told", 1, 75_000.0, 15.0, &["k03", "k05"]);
    let said = rep::report(&root).unwrap();
    let rows: Vec<Vec<String>> = said.lines().filter(|l| l.starts_with("| kit |")).map(|l| l.trim_matches('|').split('|').map(|c| c.trim().to_owned()).collect()).collect();
    let row = |arm: &str, cols: usize| rows.iter().find(|r| r[1] == arm && r.len() == cols).unwrap_or_else(|| panic!("no {arm} row of {cols}: {rows:?}")).clone();
    // The cells: all three arms, each its own mean.
    assert_eq!(row("none", 8)[2..4], ["2 of 2", "150,000"]);
    assert_eq!(row("found", 8)[2..4], ["2 of 2", "300,000"]);
    assert_eq!(row("told", 8)[2..4], ["1 of 1", "75,000"]);
    // Against `none`: tokens, requests and checks; the baseline is not measured against itself.
    assert!(said.contains("## Against the baseline arm, `none`"), "{said}");
    let against: Vec<&Vec<String>> = rows.iter().filter(|r| r.len() == 8 && r[4].contains('.') && !r[4].contains(" / ")).collect();
    assert_eq!(against.len(), 2, "{against:?}");
    let ratio = |arm: &str| against.iter().find(|r| r[1] == arm).map(|r| r[2..].to_vec()).unwrap_or_else(|| panic!("no ratio for {arm}"));
    assert_eq!(ratio("found"), ["300,000", "150,000", "2.00", "2.00", "9.0 / 9", "8.5 / 9"]);
    assert_eq!(ratio("told"), ["75,000", "150,000", "0.50", "1.00", "7.0 / 9", "8.5 / 9"]);
    assert!(!against.iter().any(|r| r[1] == "none"));
    let kept = lev::load_json(&root.join("report.json")).unwrap();
    assert_eq!(text(&kept, "baseline"), Some("none"));
    let ratios = kept.get("against_baseline").and_then(Json::as_array).unwrap();
    assert_eq!(ratios.iter().map(|r| (text(r, "arm").unwrap(), r.get("times").and_then(Json::as_f64).unwrap())).collect::<Vec<_>>(), [("found", 2.0), ("told", 0.5)]);
    // A data folder given names the baseline; with no judged run of it, no ratio is made up.
    let other = rep::report_in(&root, Some(&lev::default_data())).unwrap();
    assert!(other.contains("## Against the baseline arm, `plain`") && other.contains("| kit | none | 150,000 | no judged run | - | - |"), "{other}");
}

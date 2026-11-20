//! The harness's own tests (no engine, no agent, nothing started): what its two reviews probed
//! by hand in M5, kept. They were `scripts/leverage_test.py`; each is the same test, in Rust.

use std::path::{Path, PathBuf};

use specula_core::Json;

use crate::leverage::{self as lev, text, R};
use crate::leverage_report as rep;

fn settings() -> Json {
    lev::settings().expect("evals/leverage/settings.json is good")
}

/// `base` with the value at `path` (keys joined by `.`) replaced.
fn changed(base: &Json, path: &str, value: Json) -> Json {
    fn put(j: &mut Json, keys: &[&str], value: Json) {
        match keys {
            [last] => {
                j.set(*last, value);
            }
            [first, rest @ ..] => {
                let mut inner = j.get(first).cloned().unwrap_or(Json::Obj(Vec::new()));
                put(&mut inner, rest, value);
                j.set(*first, inner);
            }
            [] => {}
        }
    }
    let mut out = base.clone();
    put(&mut out, &path.split('.').collect::<Vec<_>>(), value);
    out
}

fn j(text: &str) -> Json {
    Json::parse(text).unwrap_or_else(|e| panic!("{text}: {}", e.message))
}

struct Scratch {
    tmp: PathBuf,
    root: PathBuf,
}

impl Scratch {
    fn new(test: &str) -> Scratch {
        let tmp = std::env::temp_dir().join(format!("examen-leverage-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        Scratch { root: tmp.join("runs"), tmp }
    }

    fn run_dir(&self, job: &str, arm: &str, n: u32, extra: &[(&str, Json)]) -> PathBuf {
        let d = self.root.join(format!("{job}-{arm}-{n}"));
        std::fs::create_dir_all(d.join("work")).unwrap();
        let s = settings();
        let mut r = Json::obj([
            ("leverage", Json::Num(1.0)),
            ("job", Json::Str(job.to_owned())),
            ("arm", Json::Str(arm.to_owned())),
            ("n", Json::Num(f64::from(n))),
            ("phase", Json::Str("judge".to_owned())),
            (
                "settings",
                Json::obj([
                    ("limits", s.get("jobs").and_then(|jobs| jobs.get(job)).cloned().unwrap()),
                    ("judge", s.get("judge").cloned().unwrap()),
                    ("builder_model", Json::Str("builder".to_owned())),
                    ("judge_model", Json::Str("judge".to_owned())),
                    ("runs_per_cell", Json::Num(2.0)),
                    ("token_weights", s.get("token_weights").cloned().unwrap()),
                ]),
            ),
        ]);
        for (k, v) in extra {
            r.set(*k, v.clone());
        }
        std::fs::write(d.join("run.json"), r.to_string()).unwrap();
        d
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

/// A verdict for `job` with `failed` failing, as a judge writes it and as the record keeps it.
fn verdict(job: &str, failed: &[&str]) -> (Json, Json) {
    let items: Vec<Json> = lev::job(job).unwrap().get("acceptance").and_then(Json::as_array).unwrap().to_vec();
    let rows = items.iter().map(|a| {
        let id = text(a, "id").unwrap();
        Json::obj([("id", Json::Str(id.to_owned())), ("pass", Json::Bool(!failed.contains(&id))), ("evidence", Json::Str("seen".to_owned()))])
    });
    let v = Json::obj([("items", Json::arr(rows)), ("defects", Json::Arr(Vec::new())), ("could_not_check", Json::Arr(Vec::new()))]);
    let scored = lev::score(&v, &items);
    (v, scored)
}

fn builder(weighted: f64, complete: bool, kinds: &[(&str, f64)]) -> Json {
    let mut by = j(r#"{"input": 0, "cache_write": 0, "cache_read": 0, "output": 0, "requests": 5, "context_at_end": 1}"#);
    for (k, v) in kinds {
        by.set(*k, Json::Num(*v));
    }
    Json::obj([
        ("tokens", Json::Num(1.0)),
        ("tool_uses", Json::Num(5.0)),
        ("duration_s", Json::Num(60.0)),
        ("by_kind", by),
        ("weighted", Json::Num(weighted)),
        ("cost_usd", Json::Num(1.0)),
        ("ended", Json::Str("done".to_owned())),
        ("tokens_complete", Json::Bool(complete)),
    ])
}

fn refused<T>(r: R<T>) -> String {
    match r {
        Err(why) => why,
        Ok(_) => panic!("it was not refused"),
    }
}

#[test]
fn the_data_is_good_and_check_holds_what_the_data_test_holds() {
    assert_eq!(lev::check_all(), Vec::<String>::new());
    let s = settings();
    assert_eq!(lev::check_settings(&s), Vec::<String>::new());
    // (Review R2: each of these passed --check and failed crates/specula/tests/leverage_data.rs.)
    for (path, bad) in [
        ("runner.command", r#"["claude", "-p", "{prompt}"]"#),
        ("jobs.small.token_cap", "5"),
        ("judge.wall_s", "1"),
        ("arms", r#"["engine"]"#),
        ("rules", r#"["Stop every process you started."]"#),
        ("token_weights", r#"{"input": 1}"#),
        ("runs_per_cell", "0"),
    ] {
        assert!(!lev::check_settings(&changed(&s, path, j(bad))).is_empty(), "{path} = {bad}");
    }
    let small = lev::load_json(&lev::repo().join("evals").join("leverage").join("jobs").join("small.json")).unwrap();
    assert_eq!(lev::check_job(&small, "small"), Vec::<String>::new());
    // (Review R1: a brief naming a tool, a list of three, a change of itself.)
    let mut brief = small.get("brief").and_then(Json::as_array).unwrap().to_vec();
    brief.push(Json::Str("Build it with Specula.".to_owned()));
    assert!(lev::check_job(&changed(&small, "brief", Json::Arr(brief)), "small").iter().any(|p| p.contains("names no tool")));
    let three = small.get("acceptance").and_then(Json::as_array).unwrap()[..3].to_vec();
    assert!(lev::check_job(&changed(&small, "acceptance", Json::Arr(three)), "small").iter().any(|p| p.contains("at least 8")));
    assert!(lev::check_job(&changed(&small, "change_of", Json::Str("small".to_owned())), "small").iter().any(|p| p.contains("another job")));
    assert!(!lev::check_job(&changed(&small, "id", Json::Str("other".to_owned())), "small").is_empty());
}

/// The shape library's comparison (`evals/leverage-props/`, written and not run: the lead runs
/// it): its jobs are in the harness's form and pass what every job is held to; its two arms
/// differ only in the line that points at the library, which the engine arm never names.
#[test]
fn the_shape_librarys_jobs_are_in_the_harness_form() {
    let root = lev::repo().join("evals").join("leverage-props");
    let s = lev::load_json(&root.join("settings.json")).unwrap();
    let mut seen: Vec<String> = Vec::new();
    for name in ["kit", "props"] {
        let job = lev::load_json(&root.join("jobs").join(format!("{name}.json"))).unwrap();
        assert_eq!(lev::check_job(&job, name), Vec::<String>::new(), "{name}");
        assert!(s.get("jobs").and_then(|j| j.get(name)).is_some(), "{name} has no limits");
        for a in job.get("acceptance").and_then(Json::as_array).unwrap() {
            let id = text(a, "id").unwrap().to_owned();
            assert!(!seen.contains(&id), "{id} twice");
            seen.push(id);
        }
    }
    let arm = |name: &str| -> Vec<String> { s.get("arm_text").and_then(|a| a.get(name)).and_then(Json::as_array).unwrap().iter().filter_map(Json::as_str).map(str::to_owned).collect() };
    let (library, engine) = (arm("library"), arm("engine"));
    assert!(engine.iter().all(|l| !l.contains("formae") && !l.contains("shape library")), "the engine arm names the library");
    assert!(library.iter().any(|l| l.contains("formae_list")));
    let rest: Vec<&String> = library.iter().filter(|l| !l.contains("formae_list")).collect();
    assert_eq!(rest, engine.iter().collect::<Vec<_>>(), "the arms differ only in the library's line");
}

#[test]
fn weighted_is_each_kind_times_its_weight() {
    let by = j(r#"{"input": 10, "cache_write": 100, "cache_read": 1000, "output": 20}"#);
    assert_eq!(lev::weighted(&by, &j(r#"{"input": 1.0, "cache_write": 1.25, "cache_read": 0.1, "output": 5.0}"#)), 335.0);
    // Half goes to the even number, as the figures of M5's runs were rounded.
    assert_eq!(lev::weighted(&j(r#"{"input": 1}"#), &j(r#"{"input": 0.5}"#)), 0.0);
    assert_eq!(lev::weighted(&j(r#"{"input": 3}"#), &j(r#"{"input": 0.5}"#)), 2.0);
}

#[test]
fn prepare_makes_nothing_before_it_has_checked() {
    let s = Scratch::new("prepare");
    // (Review R1: without --exe it left a half-made folder that refused every retry.)
    let mut a = lev::Prepare { job: "small".into(), arm: "engine".into(), n: 1, out: s.root.clone(), exe: None, port: None, from: None, build: None };
    assert!(refused(lev::prepare(&a)).contains("--exe"));
    assert!(!s.root.join("small-engine-1").exists());
    a.arm = "plain".into();
    lev::prepare(&a).unwrap();
    let run = lev::read_run(&s.root.join("small-plain-1")).unwrap();
    assert_eq!(run.get("settings").and_then(|x| x.get("token_weights")), settings().get("token_weights"), "a run keeps the weights its figures are made with");
    let brief = std::fs::read_to_string(s.root.join("small-plain-1").join("BRIEF.md")).unwrap();
    assert!(brief.contains("Do not use or read Specula"));
    assert!(refused(lev::prepare(&a)).contains("prepared once"));
    // An arm nobody has, and a job nobody has.
    a.arm = "both".into();
    assert!(refused(lev::prepare(&a)).contains("arm must be one of"));
    a.job = "huge".into();
    assert!(lev::prepare(&a).is_err());
}

#[test]
fn a_judged_run_is_judged_again_only_with_its_first_judging_kept() {
    let s = Scratch::new("again");
    s.run_dir("medium", "plain", 9, &[]);
    let ch = s.run_dir("change", "plain", 9, &[("from", Json::Str("medium-plain-9".to_owned()))]);
    let (v, scored) = verdict("change", &["c02"]);
    std::fs::write(ch.join("verdict.json"), v.to_string()).unwrap();
    lev::write_cost(&ch, &Json::obj([("verdict", scored), ("judge", j(r#"{"weighted": 7, "cost_usd": 1.0}"#))])).unwrap();
    // (Review R2: --again moved the first verdict away before the judge's page could refuse.)
    let again = lev::RunArgs { run: ch.clone(), judge: true, again: Some("the list changed".into()), note: None };
    assert!(refused(lev::run_cmd(&again)).contains("judge medium-plain-9 first"));
    let cost = lev::read_cost(&ch).unwrap();
    assert!(lev::truthy(cost.get("verdict")) && cost.get("judged_before").is_none() && ch.join("verdict.json").is_file(), "nothing was moved");
    // By hand: a verdict is never written over unkept.
    assert!(refused(lev::judge(&ch, Some(&ch.join("verdict.json")), None)).contains("has a verdict already"));
    lev::judge(&ch, Some(&ch.join("verdict.json")), Some("a second look")).unwrap();
    let cost = lev::read_cost(&ch).unwrap();
    let before = cost.get("judged_before").and_then(Json::as_array).unwrap();
    assert_eq!((before.len(), text(&before[0], "why")), (1, Some("a second look")));
    assert!(ch.join("verdict.1.json").is_file() && ch.join("verdict.json").is_file());
}

#[test]
fn a_changes_judge_is_told_what_the_game_passed_before() {
    let s = Scratch::new("before");
    let medium = s.run_dir("medium", "plain", 9, &[]);
    let ch = s.run_dir("change", "plain", 9, &[("from", Json::Str("medium-plain-9".to_owned()))]);
    let (_, scored) = verdict("medium", &["m10"]);
    lev::write_cost(&medium, &Json::obj([("verdict", scored)])).unwrap();
    let page = lev::judge_text(&ch, &lev::read_run(&ch).unwrap()).unwrap();
    let line = |id: &str| page.lines().find(|x| x.starts_with(&format!("- **{id}**:"))).unwrap_or_else(|| panic!("no line for {id}")).to_owned();
    assert!(line("m10").contains("FAILED before the change (not held against it)"));
    assert!(line("m01").contains("[passed before the change]"));
    // The plain arm's judge is told of no engine; the page asks for the verdict's form.
    assert!(!page.contains("Specula engine") && page.contains("verdict.json") && page.contains("could_not_check"));
}

#[test]
fn whether_an_engine_stopped_is_read_from_the_answer() {
    // (Review R1: `specula stop` exits 1 for an engine that had stopped already and for one
    // that would not; every run recorded "not stopped".)
    for (answer, gone) in [(r#"{"stopped": true}"#, true), (r#"{"error": "not_listed"}"#, true), (r#"{"error": "no_engine"}"#, true), (r#"{"error": "still_running"}"#, false), ("{}", false), ("", false)] {
        assert_eq!(lev::stopped_says(answer), gone, "{answer}");
    }
    assert!(lev::stop_engine(&j(r#"{"arm": "plain"}"#), Path::new(".")), "a run with no engine has none left");
}

#[test]
fn a_stream_with_no_result_line_says_its_tokens_are_incomplete() {
    let s = Scratch::new("stream");
    let message = |id: &str, out: u32| {
        format!(
            r#"{{"type": "assistant", "message": {{"id": "{id}", "content": [{{"type": "tool_use", "id": "t{id}0", "name": "Bash", "input": {{"command": "ls"}}}}], "usage": {{"input_tokens": 2, "cache_creation_input_tokens": 10, "cache_read_input_tokens": 100, "output_tokens": {out}}}}}}}"#
        )
    };
    let stream = s.tmp.join("s.jsonl");
    std::fs::write(&stream, [message("a", 5), message("a", 9), message("b", 7), "not json".to_owned()].join("\n") + "\n").unwrap();
    let cut = lev::stream_usage(&stream);
    let n = |v: &Json, path: &[&str]| path.iter().try_fold(v, |v, k| v.get(k)).and_then(Json::as_f64);
    assert_eq!(cut.get("complete"), Some(&Json::Bool(false)));
    // (A request's message arrives in parts, each with the usage so far: the last counts.)
    assert_eq!((n(&cut, &["by_kind", "requests"]), n(&cut, &["by_kind", "output"]), n(&cut, &["tool_uses"])), (Some(2.0), Some(16.0), Some(3.0)));
    assert_eq!((n(&cut, &["by_kind", "input"]), n(&cut, &["by_kind", "cache_read"]), n(&cut, &["by_kind", "context_at_end"])), (Some(4.0), Some(200.0), Some(112.0)));
    let result = r#"{"type": "result", "total_cost_usd": 0.5, "num_turns": 3, "result": "done", "is_error": false, "modelUsage": {"m": {"inputTokens": 4, "outputTokens": 40, "cacheReadInputTokens": 200, "cacheCreationInputTokens": 20}}}"#;
    std::fs::write(&stream, [message("a", 5), message("b", 7), result.to_owned()].join("\n") + "\n").unwrap();
    let whole = lev::stream_usage(&stream);
    assert_eq!(whole.get("complete"), Some(&Json::Bool(true)));
    assert_eq!((n(&whole, &["by_kind", "output"]), n(&whole, &["by_kind", "cache_read"]), n(&whole, &["cost_usd"])), (Some(40.0), Some(200.0), Some(0.5)));
    assert_eq!(whole.get("models").map(Json::to_string).as_deref(), Some(r#"["m"]"#));
    assert_eq!((text(&whole, "said"), whole.get("error")), (Some("done"), Some(&Json::Bool(false))));
}

#[test]
fn the_report_counts_what_was_run_and_every_judging() {
    let s = Scratch::new("report");
    let (_, all_pass) = verdict("small", &[]);
    let (_, one_miss) = verdict("small", &["s05"]);
    let judged = || j(r#"{"weighted": 100, "cost_usd": 0.5}"#);
    for (n, weighted, build) in [(1, 400_000.0, "first"), (2, 700_000.0, "first"), (3, 500_000.0, "second")] {
        let d = s.run_dir("small", "engine", n, &[("engine_build", Json::Str(build.to_owned()))]);
        lev::write_cost(&d, &Json::obj([("builder", builder(weighted, true, &[])), ("verdict", all_pass.clone()), ("judge", judged())])).unwrap();
    }
    let d = s.run_dir("small", "engine", 4, &[("engine_build", Json::Str("second".to_owned()))]);
    // (A run stopped at its limit: its tokens are incomplete, and it is in no mean.)
    let mut stopped = builder(1.0, false, &[]);
    stopped.set("ended", Json::Str("wall".to_owned()));
    stopped.set("over_wall", Json::Bool(true));
    lev::write_cost(&d, &Json::obj([("builder", stopped), ("verdict", all_pass.clone())])).unwrap();
    let d = s.run_dir("small", "plain", 1, &[]);
    let earlier = Json::obj([("why", Json::Str("the list changed".to_owned())), ("verdict", all_pass.clone()), ("judge", j(r#"{"weighted": 50, "cost_usd": 0.25}"#))]);
    lev::write_cost(&d, &Json::obj([("builder", builder(100_000.0, true, &[])), ("verdict", one_miss), ("judge", judged()), ("judged_before", Json::arr([earlier]))])).unwrap();
    let said = rep::report(&s.root).unwrap();
    let cells: Vec<Vec<String>> = said.lines().filter(|l| l.starts_with("| small |")).map(|l| l.trim_matches('|').split('|').take(4).map(|c| c.trim().to_owned()).collect()).collect();
    let has = |cell: [&str; 4]| cells.iter().any(|c| c.iter().map(String::as_str).eq(cell));
    assert!(has(["small", "engine, build first", "2 of 2", "550,000"]), "{cells:?}");
    assert!(has(["small", "engine, build second", "1 of 2", "500,000"]), "the incomplete run is left out of its cell's mean: {cells:?}");
    assert!(said.contains("incomplete"));
    // Past the cap by the weighted figure (600,000 for this job), whatever an older record said.
    let row = |run: &str| said.lines().find(|l| l.starts_with(&format!("| {run} "))).unwrap().to_owned();
    assert!(row("small-engine-2").contains("past the token cap") && !row("small-engine-1").contains("past the token cap"));
    assert!(row("small-engine-4").contains("past the time limit"));
    // (Review R2: the judging total left out the earlier judgings.)
    assert!(said.contains("5 judging(s) of 4 run(s), 450 weighted tokens"), "{said}");
    assert!(said.contains("5.00 USD for the builds, 2.00 USD for the judging shown above, and 0.25 USD for the earlier judgings of the runs judged again (7.25 USD in all)"));
    assert!(said.contains("small-plain-1: 14 / 14. Judged again because: the list changed"));
    assert!(said.contains("small-plain-1 **s05**"));
    assert!(s.root.join("report.md").is_file() && lev::load_json(&s.root.join("report.json")).unwrap().get("runs").and_then(Json::as_array).is_some_and(|r| r.len() == 5));
}

#[test]
fn a_place_of_the_machine_is_found_however_a_tool_wrote_it() {
    let places = [rep::Place::new(Path::new("C:/Users/someone"), "<home>"), rep::Place::new(Path::new("D:/Coding Projects/Repo"), "<repo>")]; // private-scan: allow
    let clean = |t: &str| rep::scrub(t, &places, Some("someone"));
    assert_eq!(clean("at C:/Users/someone/x.txt"), "at <home>/x.txt"); // private-scan: allow
    assert_eq!(clean(r"at C:\Users\someone\x.txt"), r"at <home>\x.txt"); // private-scan: allow
    assert_eq!(clean(r"in JSON: C:\\Users\\someone\\x"), r"in JSON: <home>\\x"); // private-scan: allow
    assert_eq!(clean(r"twice quoted: C:\\\\Users\\\\someone"), "twice quoted: <home>"); // private-scan: allow
    assert_eq!(clean("lower: c:/users/SOMEONE/bin"), "lower: <home>/bin"); // private-scan: allow
    assert_eq!(clean("a shell's: /c/Users/someone/bin"), "a shell's: <home>/bin");
    assert_eq!(clean("a longer name: C:/Users/someonex/y"), "a longer name: <home>/y", "the tail of a longer name does not leak"); // private-scan: allow
    assert_eq!(clean("d:/coding projects/repo/docs"), "<repo>/docs");
    // The name alone, wherever it stands as a word; never inside another word.
    assert_eq!(clean("file:///X:/home/someone and Users%5Csomeone%5Cx"), "file:///X:/home/<user> and Users%5C<user>%5Cx"); // private-scan: allow
    assert_eq!(clean("Someone's run; someones; asomeone; some one"), "<user>'s run; someones; asomeone; some one");
    // A name too short to be told from a word is left.
    assert_eq!(rep::scrub("al said so", &[], Some("al")), "al said so");
    assert_eq!(rep::scrub("nothing here", &places, None), "nothing here");
}

#[test]
fn export_keeps_no_path_and_no_name_of_the_machine() {
    let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).filter(|h| h.file_name().is_some_and(|n| n.len() >= 3)) else {
        // (No home folder named in this environment: there is no name to keep out.)
        return;
    };
    let user = home.file_name().unwrap().to_string_lossy().into_owned();
    let s = Scratch::new("export");
    let d = s.run_dir("small", "plain", 1, &[("exe", Json::Str(home.join("bin").join("specula.exe").to_string_lossy().into_owned()))]);
    let (_, mut scored) = verdict("small", &[]);
    let root = lev::absolute(&s.root);
    let (h, native) = (lev::posix(&home), home.to_string_lossy().into_owned());
    let shell = if h.chars().nth(1) == Some(':') { format!("/{}{}", h[..1].to_lowercase(), &h[2..]) } else { h.clone() };
    let forms = [
        h.clone(),
        native.clone(),
        native.replace('\\', "\\\\"),
        h.to_lowercase(),
        format!("{h}x/y"),
        shell,
        format!("{}/small-plain-1/work", lev::posix(&root)),
        root.to_string_lossy().to_uppercase(),
        format!("{}/docs", lev::posix(&lev::repo())),
        format!("file:///{h}"),
        format!("Users%5C{user}%5Cx"),
    ];
    let mut items = scored.get("items").and_then(Json::as_array).unwrap().to_vec();
    items[0].set("evidence", Json::Str(forms.join(" | ")));
    scored.set("items", Json::Arr(items));
    let mut b = builder(700_000.0, true, &[]);
    b.set("over_token_cap", Json::Bool(false));
    b.set("runner", Json::Str(home.join("claude").to_string_lossy().into_owned()));
    lev::write_cost(&d, &Json::obj([("builder", b), ("verdict", scored), ("engine_stopped", Json::Bool(false))])).unwrap();
    let dest = s.tmp.join("kept");
    rep::export(&s.root, &dest).unwrap();
    let root_name = root.file_name().unwrap().to_string_lossy().to_lowercase();
    for f in std::fs::read_dir(&dest).unwrap().flatten() {
        let said = std::fs::read_to_string(f.path()).unwrap();
        // (Review R2: four backslashes, a longer name beginning with the home's, the name alone.)
        assert!(!said.to_lowercase().contains(&user.to_lowercase()), "{:?} holds the user's name", f.file_name());
        assert!(!said.replace('\\', "/").to_lowercase().contains(&format!("{root_name}/small")), "{:?} holds the runs' folder", f.file_name());
    }
    let kept = lev::load_json(&dest.join("small-plain-1.json")).unwrap();
    assert!(kept.get("run").unwrap().get("exe").is_none());
    let b = kept.get("costs").and_then(|c| c.get("builder")).unwrap();
    assert!(b.get("runner").is_none());
    assert_eq!(b.get("over_token_cap"), Some(&Json::Bool(true)), "held against the weighted figure");
    assert!(kept.get("costs").unwrap().get("engine_stopped").is_none(), "what an older harness could not know is not said");
}

#[test]
fn where_reads_a_builders_steps() {
    let repo = lev::posix(&lev::repo());
    let kind = |name: &str, input: &str| rep::step_kind(name, &j(input));
    let bash = |command: &str| rep::step_kind("Bash", &Json::obj([("command", Json::Str(command.to_owned()))]));
    assert_eq!(kind("Write", r#"{"file_path": "x.rs"}"#), "write code");
    // The harness as it was through M5 (a script), and as it is (this tool).
    for harness in [format!("python \"{repo}/scripts/leverage.py\""), format!("\"{repo}/target/debug/examen.exe\" leverage")] {
        assert_eq!(bash(&format!("{harness} call \"R\" logic_build @a.json")), "build the module", "{harness}");
        assert_eq!(bash(&format!("{harness} tools \"R\" world_")), "find tools and their arguments", "{harness}");
        assert_eq!(bash(&format!("{harness} engine start \"R\"")), "start or stop the engine", "{harness}");
        assert_eq!(bash(&format!("{harness} call \"R\" world_spawn @a.json")), "call the engine", "{harness}");
        // (A build named on a later line of the same command is not this call's.)
        assert_eq!(bash(&format!("{harness} call \"R\" world_spawn @a.json\necho logic_build")), "call the engine", "{harness}");
    }
    assert_eq!(bash(&format!("sed -n 1,80p \"{repo}/docs/design/MODULES.md\"")), "read the engine's documents and source");
    assert_eq!(bash("cat > game.py <<'EOF'\nprint(1)\nEOF"), "write code");
    assert_eq!(bash("python game.py --headless --ticks 60"), "run its own checks");
    assert_eq!(bash("cargo build --offline"), "build the module");
    assert_eq!(bash("ls -la work"), "read its own files");
    assert_eq!(bash("echo done"), "other commands");
    assert_eq!(kind("Read", r#"{"file_path": "work/frame.png"}"#), "look at a frame");
    assert_eq!(kind("Read", r#"{"file_path": "work/main.rs"}"#), "read its own files");
    assert_eq!(kind("WebFetch", "{}"), "other");
    let s = Scratch::new("where");
    let d = s.run_dir("small", "engine", 1, &[]);
    lev::write_cost(&d, &Json::obj([("builder", builder(335.0, true, &[("input", 10.0), ("cache_write", 100.0), ("cache_read", 1000.0), ("output", 20.0)]))])).unwrap();
    let used = |id: &str, name: &str, input: Json| Json::obj([("type", Json::Str("assistant".to_owned())), ("message", Json::obj([("id", Json::Str(id.to_owned())), ("content", Json::arr([Json::obj([("type", Json::Str("tool_use".to_owned())), ("id", Json::Str(id.to_owned())), ("name", Json::Str(name.to_owned())), ("input", input)])]))]))]).to_string();
    let answered = |id: &str, said: &str| Json::obj([("type", Json::Str("user".to_owned())), ("message", Json::obj([("content", Json::arr([Json::obj([("type", Json::Str("tool_result".to_owned())), ("tool_use_id", Json::Str(id.to_owned())), ("content", Json::Str(said.to_owned()))])]))]))]).to_string();
    let command = |c: String| Json::obj([("command", Json::Str(c))]);
    let build = || command(format!("python \"{repo}/scripts/leverage.py\" call \"R\" logic_build @a.json"));
    let lines = [
        used("1", "Bash", command(format!("cat \"{repo}/README.md\""))),
        answered("1", &"x".repeat(400)),
        used("2", "Bash", build()),
        answered("2", r#"{"ok": false, "errors": 2}"#),
        used("3", "Write", Json::obj([("file_path", Json::Str("m.rs".to_owned())), ("content", Json::Str("y".repeat(80)))])),
        answered("3", "ok"),
        used("4", "Bash", build()),
        answered("4", r#"{"ok": true, "errors": 0}"#),
    ];
    std::fs::write(lev::costs_dir(&d).unwrap().join("small-engine-1.builder.stream.jsonl"), lines.join("\n") + "\n").unwrap();
    let w = rep::where_of(&d).unwrap();
    let n = |path: &[&str]| path.iter().try_fold(&w, |v, k| v.get(k)).and_then(Json::as_f64);
    assert_eq!((n(&["steps"]), n(&["steps_before_the_first_code"]), n(&["builds", "steps"]), n(&["builds", "failed"])), (Some(4.0), Some(2.0), Some(2.0), Some(1.0)));
    assert_eq!((n(&["weighted_by_kind", "read_again"]), n(&["weighted_by_kind", "written_out"]), n(&["weighted_by_kind", "new_in_context"])), (Some(100.0), Some(100.0), Some(135.0)));
    assert_eq!(n(&["weighted"]), Some(335.0), "the three parts are the weighted figure");
    assert_eq!(n(&["by_step", "read the engine's documents and source", "answered_tokens"]), Some(100.0));
    assert!(text(&w, "how").unwrap().contains("characters / 4"));
    assert!(rep::where_text(&w).contains("4 steps (2 before the first code); weighted 335 = read again 100 + written out 100 + new in the context 135"));
}

#[test]
fn trace_gives_every_request_its_cost_and_what_it_left_to_carry() {
    use crate::leverage_trace as tr;
    let s = Scratch::new("trace");
    let d = s.run_dir("small", "engine", 1, &[]);
    // Three requests. The context: 1,000 at the start; the first adds 400 (an answer of 1,200
    // characters and 100 written); the second adds 200.
    let request = |id: &str, write: f64, read: f64, content: Json| {
        let usage = Json::obj([("input_tokens", Json::Num(0.0)), ("cache_creation_input_tokens", Json::Num(write)), ("cache_read_input_tokens", Json::Num(read)), ("output_tokens", Json::Num(1.0))]);
        Json::obj([("type", Json::Str("assistant".to_owned())), ("message", Json::obj([("id", Json::Str(id.to_owned())), ("usage", usage), ("content", Json::arr([content]))]))]).to_string()
    };
    let used = |id: &str, name: &str, input: Json| Json::obj([("type", Json::Str("tool_use".to_owned())), ("id", Json::Str(id.to_owned())), ("name", Json::Str(name.to_owned())), ("input", input)]);
    let answered = |id: &str, said: &str| Json::obj([("type", Json::Str("user".to_owned())), ("message", Json::obj([("content", Json::arr([Json::obj([("type", Json::Str("tool_result".to_owned())), ("tool_use_id", Json::Str(id.to_owned())), ("content", Json::Str(said.to_owned()))])]))]))]).to_string();
    let lines = [
        request("a", 1000.0, 0.0, used("1", "Read", Json::obj([("file_path", Json::Str("work/notes/how.md".to_owned()))]))),
        answered("1", &"x".repeat(1200)),
        request("b", 400.0, 1000.0, used("2", "Write", Json::obj([("file_path", Json::Str("work/m.rs".to_owned())), ("content", Json::Str("y".repeat(400)))]))),
        answered("2", "ok"),
        request("c", 200.0, 1400.0, Json::obj([("type", Json::Str("text".to_owned())), ("text", Json::Str("z".repeat(80)))])),
    ];
    std::fs::write(lev::costs_dir(&d).unwrap().join("small-engine-1.builder.stream.jsonl"), lines.join("\n") + "\n").unwrap();
    // Read: 1,600 new x 1.25 + 2,400 read again x 0.1 = 2,240. Written: 320 x 5 = 1,600.
    lev::write_cost(&d, &Json::obj([("builder", builder(3840.0, true, &[("cache_write", 1600.0), ("cache_read", 2400.0), ("output", 320.0)]))])).unwrap();
    let t = tr::trace_of(&d, false).unwrap();
    let n = |path: &[&str]| path.iter().try_fold(&t, |v, k| v.get(k)).and_then(Json::as_f64);
    assert_eq!((n(&["requests"]), n(&["reading"]), n(&["writing"]), n(&["in_the_ledger"])), (Some(3.0), Some(2240.0), Some(1600.0), Some(3840.0)), "the ledger is the run's weighted figure");
    let rows = t.get("rows").and_then(Json::as_array).unwrap();
    let row = |i: usize, k: &str| rows[i].get(k).and_then(Json::as_f64).unwrap();
    assert_eq!((row(0, "context"), row(1, "context"), row(2, "context")), (1000.0, 1400.0, 1600.0));
    // What a request wrote: its growth less its answer (400 - 300 = 100; 200 - 0 = 200; the
    // last: what can be seen, 20), scaled to the run's 320.
    assert_eq!((row(0, "written_out"), row(1, "written_out"), row(2, "written_out")), (100.0, 200.0, 20.0));
    // What the first request added (400) was written to the cache at the second (x 1.25) and
    // read again at the third (x 0.1); the second's 200 was written at the third.
    assert_eq!((row(0, "carried"), row(1, "carried"), row(2, "carried")), (540.0, 250.0, 0.0));
    // The start (1,000) was written once and read again twice. With the rows: all the reading.
    assert_eq!(n(&["start", "carried"]), Some(1450.0));
    assert_eq!(1450.0 + 540.0 + 250.0, 2240.0);
    assert_eq!(row(2, "cost_so_far"), 3840.0);
    // By the first step of each request, and by file.
    assert_eq!(n(&["by_kind", "read its own files", "carried"]), Some(540.0));
    assert_eq!(n(&["by_kind", "write code", "writing"]), Some(1000.0));
    let files = t.get("files").and_then(Json::as_array).unwrap();
    assert!(files.iter().any(|f| text(f, "file") == Some("work/notes/how.md") && f.get("reads").and_then(Json::as_f64) == Some(1.0) && f.get("answered").and_then(Json::as_f64) == Some(300.0)));
    let said = tr::trace_text(&t);
    assert!(said.contains("3 requests; weighted 3,840 (in the ledger 3,840 = reading 2,240 + writing 1,600)"), "{said}");
    // What was done at a request, by its number in the ledger: the step whole, its answer's
    // start, and the model's own words; cut where asked, and only the requests asked for.
    let done = tr::said_text(&d, false, 1, 1, 50).unwrap();
    assert!(done.contains("[1] Read: {\"file_path\":\"work/notes/how.md\"}") && done.contains(&format!("      -> {} ... (1200 characters)", "x".repeat(50))) && !done.contains("[2]"), "{done}");
    let last = tr::said_text(&d, false, 2, 3, 600).unwrap();
    assert!(last.contains("[2] Write: ") && last.contains("      -> ok") && last.contains(&format!("[3] said: {}", "z".repeat(80))) && !last.contains("[1]"), "{last}");
    // A run stopped at its time limit has no total of what it wrote: nothing is scaled to one.
    lev::write_cost(&d, &Json::obj([("builder", builder(0.0, false, &[("output", 3.0)]))])).unwrap();
    let cut = tr::trace_of(&d, false).unwrap();
    assert_eq!(cut.get("rows").and_then(Json::as_array).unwrap()[1].get("written_out").and_then(Json::as_f64), Some(200.0));
    assert_eq!(cut.get("complete").and_then(Json::as_bool), Some(false));
}

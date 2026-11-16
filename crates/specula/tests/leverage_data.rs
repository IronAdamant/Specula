//! The measured comparison's data (evals/leverage/, used by `examen leverage`: tools/examen; design in
//! docs/design/LEVERAGE.md) stays well formed without an engine or an agent: the settings name
//! every job and give each its limits, every job has a brief and an acceptance list whose ids
//! are its own, the list says which items are the "checkable without hands" ones, and a change
//! job names the job it changes. The jobs are original designs told as rules and numbers: a
//! brief never names another game.

use specula_core::Json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/leverage")
}

fn read_json(path: &Path) -> Json {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Json::parse(&text).unwrap_or_else(|e| panic!("{}: not JSON: {e:?}", path.display()))
}

fn jobs() -> Vec<(String, Json)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(data().join("jobs")).expect("evals/leverage/jobs") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            out.push((path.file_stem().unwrap().to_string_lossy().into_owned(), read_json(&path)));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn whole(j: &Json, key: &str) -> u64 {
    j.get(key).and_then(Json::as_u64).unwrap_or_else(|| panic!("{key} must be a whole number: {j}"))
}

#[test]
fn the_settings_give_every_job_its_limits_and_sizes_are_data() {
    let s = read_json(&data().join("settings.json"));
    assert_eq!(whole(&s, "leverage"), 1);
    assert!((1..=20).contains(&whole(&s, "runs_per_cell")));
    for model in ["builder_model", "judge_model"] {
        assert!(s.get(model).and_then(Json::as_str).is_some_and(|m| !m.is_empty()), "{model}");
    }
    let arms: Vec<&str> = s.get("arms").and_then(Json::as_array).expect("arms").iter().filter_map(Json::as_str).collect();
    assert_eq!(arms, ["engine", "plain"]);
    let Some(Json::Obj(limits)) = s.get("jobs") else { panic!("jobs") };
    let named: BTreeSet<&str> = limits.iter().map(|(k, _)| k.as_str()).collect();
    let files = jobs();
    let have: BTreeSet<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(named, have, "settings.json's jobs and the files in jobs/ are the same set");
    for (name, lim) in limits.iter().map(|(k, v)| (k.as_str(), v)).chain([("judge", s.get("judge").expect("judge"))]) {
        assert!(whole(lim, "token_cap") >= 10_000 && whole(lim, "wall_s") >= 60, "{name}: {lim}");
    }
    let ports: Vec<u64> = s.get("ports").and_then(Json::as_array).expect("ports").iter().filter_map(Json::as_u64).collect();
    assert!(ports.len() == 2 && 1024 <= ports[0] && ports[0] <= ports[1] && ports[1] <= 65535, "{ports:?}");
    // One figure for a run's tokens needs a weight for each kind, and they are data.
    let Some(Json::Obj(weights)) = s.get("token_weights") else { panic!("token_weights") };
    let kinds: BTreeSet<&str> = weights.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kinds, BTreeSet::from(["cache_read", "cache_write", "input", "output"]));
    assert!(weights.iter().all(|(_, w)| w.as_f64().is_some_and(|w| w >= 0.0)));
    // How a builder is started is a command line with the prompt in it: any agent's fits.
    let command: Vec<&str> = s.get("runner").and_then(|r| r.get("command")).and_then(Json::as_array).expect("runner.command").iter().filter_map(Json::as_str).collect();
    assert!(command.len() >= 2 && command.contains(&"{prompt}") && command.contains(&"{model}"), "{command:?}");
    // What every builder is told about the machine it runs on: nobody's machine is assumed
    // to be free of a person, and nothing is fetched.
    let rules: Vec<&str> = s.get("rules").and_then(Json::as_array).expect("rules").iter().filter_map(Json::as_str).collect();
    assert!(rules.iter().any(|r| r.contains("Download nothing")) && rules.iter().any(|r| r.contains("127.0.0.1")) && rules.iter().any(|r| r.contains("pointer")), "{rules:?}");
}

#[test]
fn every_job_has_one_brief_and_a_list_that_can_be_answered_item_by_item() {
    let files = jobs();
    assert!(files.len() >= 3, "a small job, a medium one and a change");
    let names: BTreeSet<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    let mut all_ids = BTreeSet::new();
    for (name, j) in &files {
        assert_eq!(whole(j, "leverage"), 1, "{name}");
        assert_eq!(j.get("id").and_then(Json::as_str), Some(name.as_str()), "a job's id is its file's name");
        assert!(j.get("title").and_then(Json::as_str).is_some_and(|t| !t.is_empty()), "{name}: title");
        let brief: Vec<&str> = j.get("brief").and_then(Json::as_array).unwrap_or_else(|| panic!("{name}: brief")).iter().map(|l| l.as_str().expect("a line")).collect();
        let text = brief.join("\n");
        // One brief for both arms: it says what to build and what to hand in, never how.
        assert!(text.len() >= 400, "{name}: the brief says what the game does");
        for word in ["Specula", "engine", "Python", "Rust", "cargo"] {
            assert!(!text.contains(word), "{name}: the brief is the same for both arms and names no tool ({word})");
        }
        // (A change keeps the two checks its game already has: its brief says they still work.)
        let checks_named = j.get("change_of").is_some() || (text.contains("PNG") && text.contains("JSON"));
        assert!(text.contains("headless") && text.contains("frame") && checks_named, "{name}: both arms hand in the two checks");
        let items = j.get("acceptance").and_then(Json::as_array).unwrap_or_else(|| panic!("{name}: acceptance"));
        assert!(items.len() >= 8, "{name}: a list worth judging");
        let mut checkable = 0;
        for a in items {
            let id = a.get("id").and_then(Json::as_str).unwrap_or_else(|| panic!("{name}: an item's id: {a}"));
            assert!(all_ids.insert(id.to_owned()), "{id} names one item across all jobs");
            assert!(id.starts_with(&name[..1]) && id.len() == 3 && id[1..].bytes().all(|b| b.is_ascii_digit()), "{name}: {id}");
            let kind = a.get("kind").and_then(Json::as_str).unwrap_or_default();
            assert!(kind == "behaviour" || kind == "checkable", "{id}: kind is behaviour or checkable");
            checkable += usize::from(kind == "checkable");
            // An item is one sentence that is true or false of a run.
            let item = a.get("text").and_then(Json::as_str).unwrap_or_default();
            assert!(item.len() >= 30 && item.ends_with('.'), "{id}: {item}");
        }
        assert!(checkable >= 1 && checkable * 4 <= items.len(), "{name}: the checks themselves are a small, marked part of the list");
        if let Some(of) = j.get("change_of") {
            let of = of.as_str().expect("change_of names a job");
            assert!(names.contains(of) && of != name, "{name}: change_of {of}");
            assert!(text.contains("built by someone else"), "{name}: its builder has no memory of the build");
        }
    }
    assert!(files.iter().any(|(_, j)| j.get("change_of").is_some()), "the change round is one of the jobs");
}


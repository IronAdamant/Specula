//! The agent-eval suite (evals/, run by scripts/evals.py; design in docs/design/EVALS.md) stays
//! well-formed without an engine or a GPU: every task file has the format's fields, its set matches
//! its folder, the files it reads exist, every expectation parses with the playtest runner's
//! matcher, and every playtest a verifier runs is a valid spec. The shared matcher cases
//! (evals/matcher_cases.json) hold the engine's matcher to the same answers as the script's.

use specula::playtest::{expectation_holds, Spec};
use specula_core::Json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(path: &Path) -> Json {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Json::parse(&text).unwrap_or_else(|e| panic!("{}: not JSON: {e:?}", path.display()))
}

const TASK_KEYS: [&str; 22] = [
    "eval", "id", "title", "set", "area", "summary", "game", "engine_args", "engine_cwd", "solver_role", "needs", "files", "setup", "actors", "brief",
    "tools", "limits", "verify", "reference", "broken", "cheats", "notes",
];

#[test]
fn matcher_cases_match_the_playtest_runner() {
    let doc = read_json(&repo().join("evals/matcher_cases.json"));
    let answer = doc.get("answer").expect("answer");
    let cases = doc.get("cases").and_then(Json::as_array).expect("cases");
    assert!(cases.len() >= 30, "keep the shared cases broad");
    let mut wrong = Vec::new();
    for (i, c) in cases.iter().enumerate() {
        let path = c.get("path").and_then(Json::as_str).expect("path");
        let matcher = c.get("matcher").expect("matcher");
        let want = c.get("want").expect("want");
        let got = match expectation_holds(path, matcher, answer) {
            Ok(b) => Json::Bool(b),
            Err(_) => Json::from("error"),
        };
        if &got != want {
            wrong.push(format!("case {i}: {path} {matcher}: want {want}, the playtest matcher says {got}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Review round 2: a matcher object with a duplicate key matched in Rust (first-match lookups) but not
/// in Python (which keeps the last), and keys of the answer weren't all checked. Objects with a
/// duplicate key now equal nothing; the Python runner refuses such task files at load.
#[test]
fn duplicate_keys_never_match() {
    let answer = Json::parse(r#"{"o": {"a": 1, "b": 2}, "d": {"a": 1, "a": 2}}"#).unwrap();
    let dup = Json::parse(r#"{"eq": {"a": 1, "a": 2}}"#).unwrap();
    assert!(!expectation_holds("o", &Json::parse(r#"{"eq": {"a": 1, "a": 1}}"#).unwrap(), &answer).unwrap());
    assert!(!expectation_holds("d", &dup, &answer).unwrap());
    assert!(!expectation_holds("d", &Json::parse(r#"{"eq": {"a": 1, "b": 2}}"#).unwrap(), &answer).unwrap());
    assert!(expectation_holds("o", &Json::parse(r#"{"eq": {"b": 2, "a": 1}}"#).unwrap(), &answer).unwrap());
}

/// Every `expect` (and `wait_until` / actor `watch` expectations) in a list of steps.
fn check_steps(steps: &Json, where_: &str, problems: &mut Vec<String>, files: &mut Vec<String>) {
    let Some(steps) = steps.as_array() else {
        problems.push(format!("{where_}: must be an array"));
        return;
    };
    for (i, s) in steps.iter().enumerate() {
        let at = format!("{where_}[{i}]");
        for holder in [Some(s), s.get("wait_until"), s.get("watch")].into_iter().flatten() {
            if let Some(expect) = holder.get("expect") {
                match expect.as_object() {
                    Some(pairs) => {
                        for (path, m) in pairs {
                            if let Err(e) = expectation_holds(path, m, &Json::Null) {
                                problems.push(format!("{at}.expect.{path}: {}", e.message));
                            }
                        }
                    }
                    None => problems.push(format!("{at}.expect must be an object")),
                }
            }
            if let Some(Json::Obj(sel)) = holder.get("select") {
                for (k, v) in sel {
                    if k == "where" {
                        for (path, m) in v.as_object().unwrap_or_default() {
                            if let Err(e) = expectation_holds(path, m, &Json::Null) {
                                problems.push(format!("{at}.select.where.{path}: {}", e.message));
                            }
                        }
                    }
                }
            }
        }
        if let Some(spec) = s.get("playtest") {
            if let Err(e) = Spec::parse(spec) {
                problems.push(format!("{at}.playtest: {}: {}", e.field, e.message));
            }
        }
        for f in s.get("args_files").and_then(Json::as_object).unwrap_or_default() {
            files.push(f.1.as_str().unwrap_or_default().to_owned());
        }
        if let Some(f) = s.get("text_file").and_then(Json::as_str) {
            files.push(f.to_owned());
        }
        if let Some(d) = s.get("do") {
            check_steps(d, &format!("{at}.do"), problems, files);
        }
    }
}

/// What `tests/private_paths.rs` (every tracked file: home folders, app-data and Claude temp folders,
/// session ids, UUIDs) doesn't look for but eval results mustn't hold either: any absolute drive path
/// (the scrubber turns them into `<target>`, `<repo>`, `~` or `<abs>`), a Claude project folder name
/// (`X--Some-Folder`) or a hex id cut short at the end of a quote (a truncated UUID or session id).
fn eval_marker(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let word = |i: usize| i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
    for i in 0..b.len() {
        if !text.is_char_boundary(i) {
            continue;
        }
        let rest = &text[i..];
        let quote = |n: usize| rest.chars().take(n).collect::<String>();
        // C:\ or C:/ (not "http://", whose letter follows another letter).
        if b[i].is_ascii_alphabetic() && !word(i) && b.get(i + 1) == Some(&b':') && matches!(b.get(i + 2), Some(b'\\' | b'/')) {
            return Some(format!("a drive path: {}", quote(40)));
        }
        // A Claude project folder: a drive letter, "--", then the folder's path with dashes.
        if b[i].is_ascii_alphabetic() && !word(i) && rest.get(1..3) == Some("--") && b.get(i + 3).is_some_and(u8::is_ascii_alphanumeric) {
            return Some(format!("a project folder: {}", quote(40)));
        }
        // A session folder's id after a masked project folder, however short a cut left it.
        if let Some(tail) = rest.strip_prefix("<folder>") {
            let after = tail.trim_start_matches(['\\', '/']);
            if after.len() < tail.len() && after.starts_with(|c: char| c.is_ascii_hexdigit()) {
                return Some(format!("an id after a project folder: {}", quote(30)));
            }
        }
        // A hex id cut short: 8 hex digits, a dash, more hex, then "..." (a truncated UUID).
        let hex = |r: std::ops::Range<usize>| r.clone().all(|k| b.get(i + k).is_some_and(u8::is_ascii_hexdigit));
        if !word(i) && hex(0..8) && b.get(i + 8) == Some(&b'-') && b.get(i + 9).is_some_and(u8::is_ascii_hexdigit) {
            let tail = rest[9..].trim_start_matches(|c: char| c.is_ascii_hexdigit() || c == '-');
            if tail.starts_with("...") {
                return Some(format!("a truncated id: {}", quote(30)));
            }
        }
    }
    None
}

fn text_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            text_files(&path, out);
        } else if path.extension().is_some_and(|x| ["json", "jsonl", "txt", "md", "rs", "toml"].contains(&x.to_string_lossy().as_ref())) {
            out.push(path);
        }
    }
}

/// The eval tasks, fixtures and committed results (transcripts, journals, reports) are public. On top
/// of `private_paths.rs`, they may not hold any absolute drive path, a Claude project folder name or
/// an id cut short (review findings: answers were truncated before the scrubber ran, so cut paths and
/// ids slipped through).
#[test]
fn committed_eval_files_hold_no_private_paths() {
    let root = repo();
    let mut files = Vec::new();
    text_files(&root.join("evals"), &mut files);
    text_files(&root.join("perf").join("evals"), &mut files);
    assert!(files.len() > 20, "found the eval files ({})", files.len());
    let mut leaks = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        if let Some(m) = eval_marker(&text) {
            leaks.push(format!("{}: {m}", f.strip_prefix(&root).unwrap_or(&f).display()));
        }
    }
    assert!(leaks.is_empty(), "private paths or ids in committed eval files:\n{}", leaks.join("\n"));
}

#[test]
fn the_eval_marker_finds_what_it_should() {
    // Made-up values, built from pieces (the repository is public; see private_paths.rs).
    for leak in [
        concat!(r#"{"path": "Q:"#, r#"\\Projects\\x"}"#),
        concat!(r#""dir": "D:"#, r#"/Builds/x""#),
        concat!("X--Some", "-Folder\\b1"),
        concat!("12345678-90ab-cdef", "-12... (5 bytes)"),
        concat!("<folder>", "\\\\b10... (300 bytes)"),
    ] {
        assert!(eval_marker(leak).is_some(), "{leak}");
    }
    for clean in [r#"{"path": "<target>\\evals-runs\\x.png", "url": "http://127.0.0.1:7493/mcp"}"#, "<abs> <id> <folder> ~/x", "wave-3 run_7a", "0123abcd-ef", "<folder>\\<id>\\x", "20261022-041454-reference"] {
        assert_eq!(eval_marker(clean), None, "{clean}");
    }
}

#[test]
fn task_files_are_well_formed() {
    let root = repo();
    let mut ids = BTreeSet::new();
    let mut problems = Vec::new();
    let mut count = 0;
    for set in ["public", "heldout"] {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(root.join("evals").join(set))
            .unwrap_or_else(|e| panic!("evals/{set}: {e}"))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        paths.sort();
        for path in paths {
            count += 1;
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let t = read_json(&path);
            let mut p = |m: String| problems.push(format!("{set}/{name}.json: {m}"));
            for (k, _) in t.as_object().expect("a task is an object") {
                if !TASK_KEYS.contains(&k.as_str()) {
                    p(format!("unknown key {k}"));
                }
            }
            if t.get("eval").and_then(Json::as_u64) != Some(1) {
                p("eval must be 1".into());
            }
            if t.get("id").and_then(Json::as_str) != Some(name.as_str()) {
                p("id must match the file name".into());
            }
            if t.get("set").and_then(Json::as_str) != Some(set) {
                p(format!("set must be {set} (its folder)"));
            }
            if !ids.insert(name.clone()) {
                p("duplicate id across sets".into());
            }
            for k in ["title", "area", "brief"] {
                if t.get(k).and_then(Json::as_str).is_none_or(|s| s.trim().is_empty()) {
                    p(format!("{k} is required"));
                }
            }
            if t.get("tools").and_then(Json::as_array).is_none_or(|a| a.is_empty()) {
                p("tools must list what the agent may call".into());
            }
            for k in ["reference", "broken", "verify"] {
                if t.get(k).and_then(Json::as_array).is_none_or(|a| a.is_empty()) {
                    p(format!("{k} is required and non-empty"));
                }
            }
            let names: Vec<&str> = t.get("verify").and_then(Json::as_array).unwrap_or_default().iter().filter_map(|v| v.get("name").and_then(Json::as_str)).collect();
            if names.len() != t.get("verify").and_then(Json::as_array).map_or(0, <[Json]>::len) || names.iter().collect::<BTreeSet<_>>().len() != names.len() {
                p("every verifier needs a unique name".into());
            }
            let mut files = Vec::new();
            let mut step_problems = Vec::new();
            for k in ["setup", "reference", "broken", "verify", "actors"] {
                if let Some(steps) = t.get(k) {
                    check_steps(steps, k, &mut step_problems, &mut files);
                }
            }
            for (i, c) in t.get("cheats").and_then(Json::as_array).unwrap_or_default().iter().enumerate() {
                match c.get("steps") {
                    Some(steps) if c.get("id").and_then(Json::as_str).is_some() => check_steps(steps, &format!("cheats[{i}].steps"), &mut step_problems, &mut files),
                    _ => step_problems.push(format!("cheats[{i}] needs id and steps")),
                }
            }
            for f in t.get("files").and_then(Json::as_array).unwrap_or_default() {
                files.push(f.get("from").and_then(Json::as_str).unwrap_or_default().to_owned());
            }
            for m in step_problems {
                p(m);
            }
            for f in files {
                if !root.join(&f).exists() {
                    p(format!("reads {f}, which doesn't exist"));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(count >= 20, "the suite has {count} tasks; the roadmap asks for at least 20");
}

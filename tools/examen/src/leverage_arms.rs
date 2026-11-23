//! A comparison's arms as data (docs/design/LEVERAGE.md, "Another data folder").
//!
//! A data folder's `settings.json` names its arms. As it always did, `"arms": ["engine",
//! "plain"]`: the two arms whose text is written in [`crate::leverage`]'s code, compared against
//! `plain`. Or as objects, each with a name, a kind, its text and, for an arm of kind `engine`,
//! the key of the engine executable it runs, which the command line resolves (`--exe KEY=PATH`):
//!
//! ```json
//! "arms": [
//!   {"name": "none", "kind": "engine", "exe": "base", "text": ["What you have", "- ..."]},
//!   {"name": "told", "kind": "engine", "exe": "library", "text": ["What you have", "- ..."]}
//! ],
//! "baseline": "none"
//! ```
//!
//! An arm's kind is what the harness does for it: `engine`, a project made by `specula new`, an
//! engine the builder may start, its tools through this tool and every call logged; `plain`, an
//! empty work folder and nothing else. Its text is the "What you have" lines of its brief, with
//! `{tool}` (this tool's command, quoted), `{run}` (the run's folder, quoted), `{work}` (the run's
//! work folder) and `{docs}` (the documents of the repository the arm's engine was built in)
//! filled in; `{{` and `}}` are a brace.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use specula_core::Json;

use crate::leverage::{text, R};

/// The arms of a settings file that names them as words, as every settings file did before
/// arms were data.
pub const DEFAULT_ARMS: [&str; 2] = ["engine", "plain"];
/// The arm the others are measured against when a settings file names none.
pub const DEFAULT_BASELINE: &str = "plain";
/// The key of an executable given as `--exe PATH`, with no key.
pub const UNKEYED: &str = "";
/// What an arm's text may name, in braces.
pub const PLACEHOLDERS: [&str; 4] = ["tool", "run", "work", "docs"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Engine,
    Plain,
}

impl Kind {
    pub const NAMES: [&str; 2] = ["engine", "plain"];

    pub fn named(name: &str) -> Option<Kind> {
        match name {
            "engine" => Some(Kind::Engine),
            "plain" => Some(Kind::Plain),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Engine => "engine",
            Kind::Plain => "plain",
        }
    }
}

/// One arm of a comparison.
#[derive(Clone, Debug, PartialEq)]
pub struct Arm {
    pub name: String,
    pub kind: Kind,
    /// Its "What you have" lines; `None`: the text written in the harness's code (the arms of a
    /// settings file that names them as words).
    pub text: Option<Vec<String>>,
    /// The key of the engine executable it runs ([`UNKEYED`]: the one given as `--exe PATH`).
    pub exe: String,
}

/// A name a folder may carry: `kit-none-1`.
fn good_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 32 && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// An executable's key, as `--exe KEY=PATH` gives it.
pub fn good_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 32 && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// `line` with its placeholders filled in by `values`; refused, saying which, when it names one
/// that is not a placeholder or has a brace that is not closed.
pub fn fill(line: &str, values: &[(&str, &str)]) -> R<String> {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find(['{', '}']) {
        out.push_str(&rest[..at]);
        let here = &rest[at..];
        if let Some(after) = here.strip_prefix("{{") {
            out.push('{');
            rest = after;
        } else if let Some(after) = here.strip_prefix("}}") {
            out.push('}');
            rest = after;
        } else if let Some(after) = here.strip_prefix('{') {
            let Some(end) = after.find('}') else { return Err(format!("a '{{' that is not closed (write '{{{{' for a brace): {}", short(line))) };
            let name = &after[..end];
            let Some((_, value)) = values.iter().find(|(k, _)| *k == name) else {
                return Err(format!("{{{name}}} is not a placeholder (they are {}; write '{{{{' and '}}}}' for a brace)", PLACEHOLDERS.map(|p| format!("{{{p}}}")).join(", ")));
            };
            out.push_str(value);
            rest = &after[end + 1..];
        } else {
            return Err(format!("a '}}' that opens nothing (write '}}}}' for a brace): {}", short(line)));
        }
    }
    out.push_str(rest);
    Ok(out)
}

fn short(line: &str) -> String {
    line.chars().take(60).collect()
}

/// Every problem with a settings file's arms, its baseline and the words its briefs may not
/// name, as lines saying where (none: they are good).
pub fn check_arms(s: &Json) -> Vec<String> {
    let mut out = Vec::new();
    let listed = s.get("arms").and_then(Json::as_array);
    let names: Vec<String> = match listed {
        None => {
            out.push(format!("arms must be {DEFAULT_ARMS:?}, or a list of arms as objects (name, kind, text, exe)"));
            Vec::new()
        }
        Some(list) if list.iter().all(|a| a.as_str().is_some()) => {
            let words: Vec<&str> = list.iter().filter_map(Json::as_str).collect();
            if words != DEFAULT_ARMS {
                out.push(format!("arms must be {DEFAULT_ARMS:?}: the comparison is between the two (other arms are given as objects: name, kind, text, exe)"));
            }
            words.iter().map(|w| (*w).to_owned()).collect()
        }
        Some(list) => {
            let mut names: Vec<String> = Vec::new();
            for (i, a) in list.iter().enumerate() {
                let at = match text(a, "name") {
                    Some(name) => format!("arms[{i}] ({name})"),
                    None => format!("arms[{i}]"),
                };
                if a.as_object().is_none() {
                    out.push(format!("{at}: an arm is an object (name, kind, text, exe), not {}; arms are all words or all objects", short(&a.to_string())));
                    continue;
                }
                match text(a, "name") {
                    Some(name) if good_name(name) => {
                        if let Some(j) = names.iter().position(|n| n == name) {
                            out.push(format!("{at}: the name {name} is arms[{j}]'s too: two arms have one name"));
                        }
                        names.push(name.to_owned());
                    }
                    Some(name) => out.push(format!("{at}: name must be 1 to 32 of a-z, 0-9 and _ (it names the run's folder), not {name:?}")),
                    None => out.push(format!("{at}: name is missing")),
                }
                let kind = text(a, "kind").and_then(Kind::named);
                if kind.is_none() {
                    out.push(format!("{at}: kind must be one of {:?}, not {}", Kind::NAMES, a.get("kind").map_or_else(|| "missing".to_owned(), Json::to_string)));
                }
                match a.get("text").and_then(Json::as_array) {
                    Some(lines) if !lines.is_empty() && lines.iter().all(|l| l.as_str().is_some()) => {
                        let empty: Vec<(&str, &str)> = PLACEHOLDERS.iter().map(|p| (*p, "")).collect();
                        for (k, line) in lines.iter().filter_map(Json::as_str).enumerate() {
                            if let Err(why) = fill(line, &empty) {
                                out.push(format!("{at}: text[{k}]: {why}"));
                            }
                        }
                    }
                    _ => out.push(format!("{at}: text must be a list of lines (what the builder is told it has)")),
                }
                match (kind, a.get("exe")) {
                    (_, None) => {}
                    (Some(Kind::Plain), Some(_)) => out.push(format!("{at}: exe names the engine an arm runs, and a plain arm runs none")),
                    (_, Some(key)) if key.as_str().is_some_and(good_key) => {}
                    (_, Some(key)) => out.push(format!("{at}: exe must be a key of 1 to 32 of a-z, 0-9, _ and - (the command line gives it a path: --exe KEY=PATH), not {}", short(&key.to_string()))),
                }
            }
            if s.get("baseline").is_none() {
                out.push("baseline must name the arm the others are measured against".to_owned());
            }
            names
        }
    };
    if listed.is_some_and(|l| l.len() < 2) {
        out.push("arms must be at least two: a comparison is between arms".to_owned());
    }
    if let Some(b) = s.get("baseline") {
        match b.as_str() {
            Some(b) if names.iter().any(|n| n == b) => {}
            _ => out.push(format!("baseline names no arm ({}; the arms are {})", short(&b.to_string()), names.join(", "))),
        }
    }
    if let Some(words) = s.get("brief_names_no") {
        if !words.as_array().is_some_and(|w| w.iter().all(|x| x.as_str().is_some_and(|x| !x.is_empty()))) {
            out.push("brief_names_no must be a list of words (what no job's brief may name)".to_owned());
        }
    }
    if s.get("change_round").is_some_and(|c| c.as_bool().is_none()) {
        out.push("change_round must be true or false (whether one of the jobs must change another's game)".to_owned());
    }
    out
}

/// A settings file's arms, in its order (it is checked: [`check_arms`]).
pub fn arms_of(s: &Json) -> R<Vec<Arm>> {
    let problems = check_arms(s);
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }
    let list = s.get("arms").and_then(Json::as_array).unwrap_or(&[]);
    Ok(list
        .iter()
        .map(|a| match a.as_str() {
            Some(word) => Arm { name: word.to_owned(), kind: Kind::named(word).unwrap_or(Kind::Plain), text: None, exe: UNKEYED.to_owned() },
            None => Arm {
                name: text(a, "name").unwrap_or("").to_owned(),
                kind: text(a, "kind").and_then(Kind::named).unwrap_or(Kind::Plain),
                text: a.get("text").and_then(Json::as_array).map(|l| l.iter().filter_map(Json::as_str).map(str::to_owned).collect()),
                exe: text(a, "exe").unwrap_or(UNKEYED).to_owned(),
            },
        })
        .collect())
}

/// The arm named `name`, or a refusal that names the arms there are.
pub fn arm_named(s: &Json, name: &str) -> R<Arm> {
    let arms = arms_of(s)?;
    let names: Vec<&str> = arms.iter().map(|a| a.name.as_str()).collect();
    arms.iter().find(|a| a.name == name).cloned().ok_or_else(|| format!("arm must be one of {names:?} (not {name:?})"))
}

/// The arm the others are measured against.
pub fn baseline_of(s: &Json) -> String {
    text(s, "baseline").unwrap_or(DEFAULT_BASELINE).to_owned()
}

/// The words no job's brief may name: one text goes to every arm.
pub fn brief_names_no(s: &Json) -> Vec<String> {
    match s.get("brief_names_no").and_then(Json::as_array) {
        Some(words) => words.iter().filter_map(Json::as_str).map(str::to_owned).collect(),
        None => crate::leverage::BRIEF_NAMES_NO.iter().map(|w| (*w).to_owned()).collect(),
    }
}

/// A run's kind: what its record says, or, for a run made before arms were data, its arm's
/// name (only `engine` had an engine).
pub fn kind_of_run(r: &Json) -> Kind {
    match text(r, "kind").and_then(Kind::named) {
        Some(kind) => kind,
        None if text(r, "arm") == Some("engine") => Kind::Engine,
        None => Kind::Plain,
    }
}

/// The engine executables the command line named: `--exe PATH` (no key) and `--exe KEY=PATH`.
#[derive(Clone, Debug, Default)]
pub struct Exes(pub BTreeMap<String, PathBuf>);

impl Exes {
    /// From each `--exe` value given, in order. A value is `KEY=PATH` when what stands before
    /// its first `=` is a key ([`good_key`]); otherwise it is a path.
    pub fn parse(given: &[String]) -> R<Exes> {
        let mut out = BTreeMap::new();
        for value in given {
            let (key, path) = match value.split_once('=') {
                Some((key, path)) if good_key(key) => (key.to_owned(), path),
                _ => (UNKEYED.to_owned(), value.as_str()),
            };
            if path.is_empty() {
                return Err(format!("--exe {value}: no path after the key"));
            }
            if out.insert(key.clone(), PathBuf::from(path)).is_some() {
                return Err(if key.is_empty() { "--exe PATH is given twice".to_owned() } else { format!("--exe {key}=... is given twice") });
            }
        }
        Ok(Exes(out))
    }

    /// The executable `arm` runs, or a refusal saying how to give it.
    pub fn of(&self, arm: &Arm) -> R<PathBuf> {
        let given = self.0.get(&arm.exe).map(|p| crate::leverage::absolute(p));
        match given {
            Some(p) if p.is_file() => Ok(p),
            Some(p) => Err(format!("the {} arm's engine {} is not a file", arm.name, crate::leverage::posix(&p))),
            None if arm.exe.is_empty() => Err(format!("the {} arm needs --exe: the specula executable (a release build)", arm.name)),
            None => Err(format!("the {} arm runs the engine named {}: give it as --exe {}=PATH (the specula executable, a release build)", arm.name, arm.exe, arm.exe)),
        }
    }
}

/// The documents of the repository an engine executable was built in: the first folder above
/// it with `docs/design/` and a `Cargo.toml` (`target/release/specula.exe` is two below it).
pub fn engine_docs(exe: &Path) -> Option<PathBuf> {
    crate::leverage::absolute(exe).ancestors().skip(1).find(|d| d.join("docs").join("design").is_dir() && d.join("Cargo.toml").is_file()).map(|d| d.join("docs"))
}

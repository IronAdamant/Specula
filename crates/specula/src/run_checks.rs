//! A game's own checks, as data: runs of the game and what each must print (`specula check`:
//! docs/design/FIRST_HOUR.md, "The game's own checks").
//!
//! `checks/runs.json` lists headless runs of the project's game, each as `specula run` takes
//! one (ticks, a script of inputs, a seed, a settings folder, a frame), with what its state is
//! expected to be. One command plays them all and says which did not print what was expected,
//! and how. It is what a builder otherwise does a run and a request at a time: after their
//! code was written, the builders of the comparison's second pass ran their game five to
//! fifteen times, each run a request that read the whole context again (a quarter to nearly
//! half of a build that went well: docs/design/LEVERAGE.md).
//!
//! This file is the checks themselves: reading them, and holding a state against what a check
//! expects. Playing them is `launch_run.rs`'s (each is a run, with an engine of its own).

use std::path::PathBuf;

use specula_core::Json;

/// The file a project keeps its checks in, below the project.
pub const FILE: &str = "checks/runs.json";
pub const FORMAT: &str = "specula.checks.v1";
/// Most checks one file holds, and the longest name of one.
pub const MAX_CHECKS: usize = 200;
const MAX_NAME: usize = 80;
/// Most keys one check expects.
const MAX_EXPECT: usize = 64;
/// The folder below the project that a check's frame is written under.
pub const FRAMES: &str = "checks";
/// Most characters of one value in a check's line (what a key is, or was expected to be).
pub const MAX_SHOWN: usize = 400;
/// The largest checks file read.
pub const MAX_FILE: u64 = 1 << 20;

/// One run of the game, and what its state must be.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub name: String,
    pub ticks: u64,
    pub seed: Option<u64>,
    pub inputs: Vec<(u64, String)>,
    /// A settings folder by name: checks that name the same one share the player's settings,
    /// in the order they are written (`None`: this run's settings are kept nowhere, whatever
    /// the command itself was told).
    pub settings: Option<String>,
    /// Where a frame of the run's end is written, from the project's folder: a PNG below its
    /// `checks` folder.
    pub frame: Option<PathBuf>,
    pub size: Option<(u32, u32)>,
    /// A key of the state (or a path into it: `walkers.0.health`) and what it must be.
    pub expect: Vec<(String, Json)>,
}

fn field<'j>(j: &'j Json, at: &str, key: &str, known: &[&str]) -> Result<Option<&'j Json>, String> {
    if !known.contains(&key) {
        return Err(format!("{at}: '{key}' is not a key of a check (it takes: {})", known.join(", ")));
    }
    Ok(j.get(key).filter(|v| !v.is_null()))
}

/// The checks of a file: `{"format": "specula.checks.v1", "checks": [{"name", "ticks",
/// "inputs", "seed", "settings", "frame", "size", "expect"}]}`.
pub fn parse(text: &str) -> Result<Vec<Check>, String> {
    let j = Json::parse(text.trim_start_matches('\u{feff}')).map_err(|e| format!("not JSON ({})", e.message))?;
    if j.get("format").and_then(Json::as_str) != Some(FORMAT) {
        return Err(format!("its \"format\" must be \"{FORMAT}\""));
    }
    let list = j.get("checks").and_then(Json::as_array).ok_or("\"checks\": a list of checks")?;
    if list.len() > MAX_CHECKS {
        return Err(format!("at most {MAX_CHECKS} checks a file (it has {})", list.len()));
    }
    // (A file that checks nothing would read as a game that passed its checks.)
    if list.is_empty() {
        return Err("\"checks\" is empty: a checks file has at least one run".to_owned());
    }
    const KNOWN: [&str; 9] = ["name", "ticks", "inputs", "seed", "settings", "frame", "size", "expect", "notes"];
    let mut out: Vec<Check> = Vec::new();
    for (i, c) in list.iter().enumerate() {
        let at = format!("checks[{i}]");
        let Some(keys) = c.as_object() else { return Err(format!("{at}: a check is an object")) };
        for (key, _) in keys {
            field(c, &at, key, &KNOWN)?;
        }
        let name = c.get("name").and_then(Json::as_str).map(str::trim).filter(|n| !n.is_empty() && n.chars().count() <= MAX_NAME && !n.chars().any(char::is_control)).ok_or_else(|| format!("{at}: \"name\": what the check is, in 1 to {MAX_NAME} characters"))?;
        if out.iter().any(|o| o.name == name) {
            return Err(format!("{at}: a second check named '{name}'"));
        }
        let at = format!("the check '{name}'");
        let ticks = c.get("ticks").and_then(Json::as_u64).filter(|t| *t <= crate::script_run::MAX_TICKS).ok_or_else(|| format!("{at}: \"ticks\": how many ticks to play, a whole number up to {}", crate::script_run::MAX_TICKS))?;
        // (A script as the command takes it, or a list of such pieces: long scripts read better.)
        let script = match c.get("inputs") {
            None | Some(Json::Null) => String::new(),
            Some(Json::Str(s)) => s.clone(),
            Some(Json::Arr(parts)) if parts.iter().all(|p| p.as_str().is_some()) => parts.iter().filter_map(Json::as_str).collect::<Vec<_>>().join(","),
            Some(_) => return Err(format!("{at}: \"inputs\": a script (\"0:enter,5:up\"), or a list of such")),
        };
        let inputs = crate::script_run::parse_inputs(&script).map_err(|e| format!("{at}: \"inputs\": {e}"))?;
        crate::script_run::check_span(&inputs, ticks).map_err(|e| format!("{at}: \"inputs\": {e}"))?;
        let seed = match c.get("seed") {
            None | Some(Json::Null) => None,
            Some(s) => Some(s.as_u64().filter(|s| *s <= crate::script_run::MAX_SEED).ok_or_else(|| format!("{at}: \"seed\": a whole number up to {}", crate::script_run::MAX_SEED))?),
        };
        let settings = match c.get("settings") {
            None | Some(Json::Null) => None,
            // (Small letters only: two names that differ by their capitals would be one
            // folder on one system and two on another.)
            Some(s) => Some(s.as_str().filter(|s| (1..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')).ok_or_else(|| format!("{at}: \"settings\": a name for the settings folder these checks share (small letters, digits, _ and -, at most 40)"))?.to_owned()),
        };
        // A frame is a PNG below the project's `checks` folder and nowhere else: the file is
        // the project's data, and whoever plays its checks did not write it. No way up, no
        // root, no drive, no second stream of a file; and nothing but a picture's name, so
        // that no file of the game is written over. (That no step of the path is a link is
        // held where the frame is written: `launch_run.rs`.)
        let below = |f: &&str| {
            let path = std::path::Path::new(f);
            let mut parts = path.components();
            !f.is_empty()
                && f.len() <= 200
                && !f.contains(':')
                && path.components().all(|c| matches!(c, std::path::Component::Normal(_)))
                && parts.next().is_some_and(|first| first.as_os_str() == FRAMES)
                && parts.next().is_some()
                && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"))
        };
        let frame = match c.get("frame") {
            None | Some(Json::Null) => None,
            Some(f) => Some(PathBuf::from(f.as_str().filter(below).ok_or_else(|| format!("{at}: \"frame\": a PNG's path below the project's {FRAMES} folder ({FRAMES}/out/title.png)"))?)),
        };
        let size = match c.get("size") {
            None | Some(Json::Null) => None,
            Some(s) => {
                let side = |i: usize| s.as_array().filter(|a| a.len() == 2).and_then(|a| a[i].as_u64()).and_then(|n| u32::try_from(n).ok()).filter(|n| crate::script_run::FRAME_SIDE.contains(n));
                Some((side(0), side(1))).and_then(|(w, h)| Some((w?, h?))).map(Some).ok_or_else(|| format!("{at}: \"size\": [width, height], each 64 to 2048"))?
            }
        };
        if size.is_some() && frame.is_none() {
            return Err(format!("{at}: \"size\" is a frame's: give \"frame\" with it"));
        }
        let expect: Vec<(String, Json)> = match c.get("expect") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Obj(pairs)) if pairs.len() <= MAX_EXPECT => pairs.clone(),
            Some(_) => return Err(format!("{at}: \"expect\": an object of at most {MAX_EXPECT} keys of the state, each with what it must be")),
        };
        for (key, want) in &expect {
            matchers(want).map_err(|why| format!("{at}: \"expect\": '{key}': {why}"))?;
        }
        out.push(Check { name: name.to_owned(), ticks, seed, inputs, settings, frame, size, expect });
    }
    Ok(out)
}

/// What a key is held against: a value it must equal, or one comparison.
#[derive(Debug, PartialEq)]
enum Matcher<'j> {
    Is(&'j Json),
    Not(&'j Json),
    /// `lt`, `lte`, `gt`, `gte`, with the number.
    Than(&'static str, f64),
    Has(&'j Json),
    Len(u64),
}

const COMPARISONS: [&str; 7] = ["ne", "lt", "lte", "gt", "gte", "has", "len"];

/// An expectation as written: a value (the key must equal it), or an object of comparisons,
/// every one of which must hold (`{"gt": 8.4, "lt": 8.6}` is a range): `ne`, `lt`, `lte`,
/// `gt`, `gte` (numbers), `has` (a list holds this value; a text holds this text) and `len`
/// (how many a list holds, or characters a text has).
///
/// An object with no comparison in it is a value to equal. One that mixes a comparison with
/// another key is refused, with the key named: `{"gt": 8.4, "max": 8.6}` read as a value to
/// equal would hold against nothing, and say so only when the check is played. (An object of
/// the state that has such a key of its own is held a key at a time, by its path.) A key
/// written twice in an expected object is refused too: which of the two is meant is not said.
fn matchers(want: &Json) -> Result<Vec<Matcher<'_>>, String> {
    let value = || twice(want).map_or(Ok(vec![Matcher::Is(want)]), |key| Err(format!("'{key}' is there twice")));
    let Json::Obj(pairs) = want else { return value() };
    let word = |w: &str| COMPARISONS.iter().find(|c| **c == w).copied();
    let Some(first) = pairs.iter().find_map(|(w, _)| word(w)) else { return value() };
    if let Some((stray, _)) = pairs.iter().find(|(w, _)| word(w).is_none()) {
        return Err(format!(
            "'{stray}' is beside \"{first}\" and is not a comparison ({}): an object is comparisons only, or a value to equal with none in it (a key of an object in the state is held by its path, `key.{stray}`)",
            COMPARISONS.join(" ")
        ));
    }
    let mut seen: Vec<&str> = Vec::new();
    let mut all = Vec::with_capacity(pairs.len());
    for (w, value) in pairs {
        let Some(word) = word(w) else { continue };
        if seen.contains(&word) {
            return Err(format!("\"{word}\" is there twice"));
        }
        seen.push(word);
        all.push(match word {
            "ne" => Matcher::Not(value),
            "has" => Matcher::Has(value),
            "len" => Matcher::Len(value.as_u64().ok_or("\"len\" takes a whole number")?),
            _ => Matcher::Than(word, value.as_f64().filter(|n| n.is_finite()).ok_or_else(|| format!("\"{word}\" takes a number"))?),
        });
    }
    Ok(all)
}

/// A key written twice in an object anywhere inside `value`, if there is one.
fn twice(value: &Json) -> Option<&str> {
    match value {
        Json::Arr(items) => items.iter().find_map(twice),
        Json::Obj(pairs) => pairs
            .iter()
            .enumerate()
            .find_map(|(i, (key, _))| pairs[..i].iter().any(|(k, _)| k == key).then_some(key.as_str()))
            .or_else(|| pairs.iter().find_map(|(_, v)| twice(v))),
        _ => None,
    }
}

/// Two values of a state are the same: numbers within a rounding of each other (a state's
/// numbers are sums of ticks: 8.499999999999991 is 8.5), everything else as it is written.
/// Equality and `ne` use it; `lt`, `lte`, `gt` and `gte` compare the numbers as they are.
fn same(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Num(x), Json::Num(y)) => (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0),
        (Json::Arr(x), Json::Arr(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y)),
        (Json::Obj(x), Json::Obj(y)) => x.len() == y.len() && x.iter().all(|(key, x)| y.iter().find(|(k, _)| k == key).is_some_and(|(_, y)| same(x, y))),
        _ => a == b,
    }
}

/// The value at `key` of a state: a key of its own, or a path into it by dots
/// (`walkers.0.health`). `None` where the state has no such key.
fn at<'j>(state: &'j Json, key: &str) -> Option<&'j Json> {
    if let Some(found) = state.get(key) {
        return Some(found);
    }
    key.split('.').try_fold(state, |v, part| match v {
        Json::Arr(items) => part.parse::<usize>().ok().and_then(|i| items.get(i)),
        _ => v.get(part),
    })
}

/// What a check's line says a key is when the state has no such key: a key that is not there
/// holds against nothing, whatever was expected of it (a misspelt key that "is not 5" would
/// be a check that cannot fail).
pub const NOT_THERE: &str = "<not in the state>";

/// A value as a check's line shows it: itself, or its beginning when it is long (a module's
/// list can be any length, and a line that does not hold is read by an agent).
fn shown(value: &Json) -> Json {
    let text = value.to_string();
    match text.char_indices().nth(MAX_SHOWN) {
        Some((at, _)) => Json::from(format!("{} ... ({} characters)", &text[..at], text.chars().count()).as_str()),
        None => value.clone(),
    }
}

/// Where a state is not what a check expects: for each such key, what it is and what was
/// expected (as written; each cut to [`MAX_SHOWN`]). Empty: the check holds.
pub fn differs(expect: &[(String, Json)], state: &Json) -> Vec<(String, Json, Json)> {
    let mut out = Vec::new();
    for (key, want) in expect {
        let Some(is) = at(state, key) else {
            out.push((key.clone(), Json::from(NOT_THERE), shown(want)));
            continue;
        };
        let one = |m: &Matcher| match m {
            Matcher::Is(v) => same(is, v),
            Matcher::Not(v) => !same(is, v),
            Matcher::Than(word, n) => is.as_f64().is_some_and(|x| match *word {
                "lt" => x < *n,
                "lte" => x <= *n,
                "gt" => x > *n,
                _ => x >= *n,
            }),
            Matcher::Has(v) => match (is, v) {
                (Json::Arr(items), _) => items.iter().any(|i| same(i, v)),
                (Json::Str(text), Json::Str(part)) => text.contains(part.as_str()),
                _ => false,
            },
            Matcher::Len(n) => match is {
                Json::Arr(items) => items.len() as u64 == *n,
                Json::Str(text) => text.chars().count() as u64 == *n,
                _ => false,
            },
        };
        let holds = matchers(want).is_ok_and(|all| all.iter().all(one));
        if !holds {
            out.push((key.clone(), shown(is), shown(want)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(checks: &str) -> String {
        format!(r#"{{"format": "specula.checks.v1", "checks": [{checks}]}}"#)
    }

    #[test]
    fn a_checks_file_is_runs_and_what_each_must_print() {
        let all = parse(&file(
            r#"{"name": "five taps win", "ticks": 40, "inputs": "0:enter,5:tap", "expect": {"phase": "won", "taps": 5}},
               {"name": "set", "ticks": 4, "inputs": ["0:down,1:enter", "2:left"], "settings": "kept", "seed": 7},
               {"name": "the title", "ticks": 0, "frame": "checks/out/title.png", "size": [800, 480], "notes": "what a player sees first"}"#,
        ))
        .unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!((all[0].name.as_str(), all[0].ticks, all[0].inputs.len(), all[0].expect.len()), ("five taps win", 40, 2, 2));
        assert_eq!((all[1].inputs.len(), all[1].settings.as_deref(), all[1].seed), (3, Some("kept"), Some(7)));
        assert_eq!((all[2].frame.clone(), all[2].size), (Some(PathBuf::from("checks/out/title.png")), Some((800, 480))));
        let no = |checks: &str| parse(&file(checks)).unwrap_err();
        assert!(no(r#"{"ticks": 4}"#).contains("\"name\""));
        assert!(no(r#"{"name": "a", "ticks": 4}, {"name": "a", "ticks": 4}"#).contains("second check named 'a'"));
        assert!(no(r#"{"name": "a"}"#).contains("\"ticks\""));
        assert!(no(r#"{"name": "a", "ticks": 4, "input": "0:up"}"#).contains("'input' is not a key of a check"));
        assert!(no(r#"{"name": "a", "ticks": 4, "inputs": "9:up"}"#).contains("after the run's last tick"));
        assert!(no(r#"{"name": "a", "ticks": 4, "settings": "../x"}"#).contains("\"settings\""));
        assert!(no(r#"{"name": "a", "ticks": 4, "settings": "Kept"}"#).contains("small letters"));
        assert!(parse(r#"{"format": "specula.checks.v1", "checks": []}"#).unwrap_err().contains("at least one run"));
        assert!(no(r#"{"name": "a", "ticks": 4, "size": [800, 480]}"#).contains("give \"frame\""));
        // A frame's place is a PNG below the project's checks folder: no way up, no root, no
        // drive, no other folder of the project, no second stream, no file that is not a picture.
        for out in ["../out.png", "checks/../../out.png", "/out.png", "C:/out.png", "C:out.png", "\\\\server\\share\\out.png", "", "out.png", "assets/out.png", "game.json", "notes/how.md", "checks", "checks/runs.json", "checks/out.png:x", "checks/a.png:b.png"] {
            let text = format!(r#"{{"name": "a", "ticks": 4, "frame": {}}}"#, Json::from(out));
            assert!(no(&text).contains("below the project's checks folder"), "{out}");
        }
        assert!(parse(&file(r#"{"name": "a", "ticks": 4, "frame": "checks/out/deep/Title.PNG"}"#)).is_ok());
        assert!(no(r#"{"name": "a", "ticks": 4, "expect": {"score": {"gte": "many"}}}"#).contains("takes a number"));
        assert!(no(r#"{"name": "a", "ticks": 4, "expect": {"score": {"gte": 1, "lt": "many"}}}"#).contains("\"lt\" takes a number"));
        assert!(parse(&file(r#"{"name": "a", "ticks": 4, "expect": {"score": {"gte": 1, "lt": 9}}}"#)).is_ok());
        // A comparison beside a key that is none is refused when the file is read, with the
        // key: read as a value to equal it would hold against nothing.
        let mixed = no(r#"{"name": "a", "ticks": 4, "expect": {"score": {"gt": 8.4, "max": 8.6}}}"#);
        assert!(mixed.contains("'max' is beside \"gt\"") && mixed.contains("`key.max`"), "{mixed}");
        assert!(no(r#"{"name": "a", "ticks": 4, "expect": {"score": {"near": 2, "lte": 3}}}"#).contains("'near' is beside \"lte\""));
        // A key twice: a comparison, or a key of a value to equal (at any depth).
        assert!(no(r#"{"name": "a", "ticks": 4, "expect": {"score": {"gt": 1, "gt": 2}}}"#).contains("\"gt\" is there twice"));
        assert!(no(r#"{"name": "a", "ticks": 4, "expect": {"walkers": [{"x": 3, "x": 3}]}}"#).contains("'x' is there twice"));
        assert!(parse(&file(r#"{"name": "a", "ticks": 4, "expect": {"walkers.0": {"x": 3, "health": 2}, "list": [{"x": 1}, {"x": 1}]}}"#)).is_ok());
        assert!(parse(r#"{"checks": []}"#).unwrap_err().contains("format"));
        assert!(parse("[").unwrap_err().contains("not JSON"));
        let many: Vec<String> = (0..=MAX_CHECKS).map(|i| format!(r#"{{"name": "c{i}", "ticks": 0}}"#)).collect();
        assert!(no(&many.join(",")).contains("at most"));
    }

    #[test]
    fn a_state_is_held_against_what_a_check_expects() {
        let state = Json::parse(r#"{"phase": "won", "taps": 5, "guard_y": 8.499999999999991, "texts": ["You won", "Score: 60"], "walkers": [{"x": 3, "health": 2}], "title": "Hold the gate", "dotted.key": 1, "none": null}"#).unwrap();
        let expect = |text: &str| match Json::parse(text).unwrap() {
            Json::Obj(pairs) => pairs,
            _ => unreachable!(),
        };
        let holds = |text: &str| differs(&expect(text), &state).is_empty();
        assert!(holds(r#"{"phase": "won", "taps": 5, "guard_y": 8.5, "texts": ["You won", "Score: 60"], "none": null, "title": {"ne": null}}"#));
        assert!(holds(r#"{"taps": {"gte": 5}, "guard_y": {"lt": 9}, "phase": {"ne": "lost"}, "texts": {"has": "You won"}, "title": {"has": "gate"}, "walkers": {"len": 1}}"#));
        assert!(holds(r#"{"walkers.0.health": 2, "walkers.0": {"x": 3, "health": 2}, "dotted.key": 1}"#));
        // What does not hold is said with what it is and what was expected.
        let off = differs(&expect(r#"{"phase": "lost", "taps": {"gt": 5}, "texts": {"has": "You lost"}, "walkers": {"len": 2}, "missing": 1, "taps.deeper": 1}"#), &state);
        let keys: Vec<&str> = off.iter().map(|(key, _, _)| key.as_str()).collect();
        assert_eq!(keys, ["phase", "taps", "texts", "walkers", "missing", "taps.deeper"]);
        assert_eq!((off[0].1.as_str(), off[0].2.as_str()), (Some("won"), Some("lost")));
        // A key the state does not have holds against nothing: not "is null", not "is not 5"
        // (a misspelt key must not make a check that cannot fail).
        let misspelt = differs(&expect(r#"{"phsae": {"ne": "title"}, "scoer": null, "walkers.4.health": null, "taps": {"ne": 4}}"#), &state);
        assert_eq!(misspelt.iter().map(|(key, is, _)| (key.as_str(), is.as_str())).collect::<Vec<_>>(), [("phsae", Some(NOT_THERE)), ("scoer", Some(NOT_THERE)), ("walkers.4.health", Some(NOT_THERE))]);
        // A long value is shown by its beginning.
        let long = Json::obj([("walkers", Json::arr((0..400).map(|i| Json::from(i as u64))))]);
        let cut = differs(&expect(r#"{"walkers": {"len": 0}}"#), &long);
        assert!(cut[0].1.as_str().is_some_and(|s| s.chars().count() < MAX_SHOWN + 40 && s.ends_with("characters)")), "{:?}", cut[0].1);
        // A comparison of numbers never holds for what is not a number.
        assert!(!holds(r#"{"phase": {"gte": 1}}"#));
        // Several comparisons of one key must all hold: a range (a builder of the fourth pass
        // wrote one, and it held against nothing: two words were read as a value to equal).
        assert!(holds(r#"{"guard_y": {"gt": 8.4, "lt": 8.6}, "taps": {"gte": 5, "lte": 5, "ne": 4}, "texts": {"has": "You won", "len": 2}}"#));
        assert!(!holds(r#"{"guard_y": {"gt": 8.4, "lt": 8.45}}"#) && !holds(r#"{"guard_y": {"gt": 8.55, "lt": 8.6}}"#) && !holds(r#"{"texts": {"has": "You won", "len": 3}}"#));
        // The rounding is equality's (and `ne`'s); a comparison takes the number as it is, so
        // a range leaves a margin.
        assert!(holds(r#"{"guard_y": 8.5}"#) && !holds(r#"{"guard_y": {"ne": 8.5}}"#) && !holds(r#"{"guard_y": {"gte": 8.5}}"#) && holds(r#"{"guard_y": {"lt": 8.5}}"#));
        // An object with no comparison in it is a value to equal.
        assert!(holds(r#"{"walkers.0": {"health": 2, "x": 3}}"#) && !holds(r#"{"walkers.0": {"health": 2}}"#));
    }
}

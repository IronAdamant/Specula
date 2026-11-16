//! The measured comparison's report, what a repository keeps of a set of runs, and where a
//! builder's tokens went (the second half of [`crate::leverage`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use specula_core::Json;

use crate::leverage::{absolute, commas, costs_dir, fmt, load_json, num, posix, read_cost, read_run, repo, settings, take, text, truthy, whole, R, TOKEN_KINDS};

fn runs_of(root: &Path) -> Vec<PathBuf> {
    let mut runs: Vec<PathBuf> = std::fs::read_dir(root).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.join("run.json").is_file()).collect()).unwrap_or_default();
    runs.sort();
    runs
}

fn name_of(run: &Path) -> String {
    run.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A float with a comma every three digits of its whole part, rounded to no decimals.
fn commas0(x: f64) -> String {
    commas(format!("{x:.0}").parse::<i64>().unwrap_or(0))
}

/// A weight as Python's `g` shows it: no trailing zeros.
fn weight(x: f64) -> String {
    format!("{x}")
}

fn at<'j>(j: &'j Json, path: &[&str]) -> Option<&'j Json> {
    path.iter().try_fold(j, |j, key| j.get(key))
}

fn passed_of(v: &Json, kind: &str) -> String {
    let n = |k: &str| at(v, &["totals", kind, k]).and_then(Json::as_f64).map_or(0, |n| n as i64);
    format!("{} / {}", n("passed"), n("of"))
}

/// One run's row of the report's data (`report.json`'s `runs`).
struct Row {
    run: String,
    job: String,
    arm: String,
    engine_build: String,
    builder: Json,
    verdict: Option<Json>,
    json: Json,
}

/// `report ROOT`: writes `report.md` and `report.json` into the runs' folder and returns the
/// report's text.
pub fn report(root: &Path) -> R<String> {
    let root = absolute(root);
    let mut runs = Vec::new();
    for run in runs_of(&root) {
        runs.push((name_of(&run), load_json(&run.join("run.json"))?, read_cost(&run)?));
    }
    if runs.is_empty() {
        return Err(format!("{}: no runs", posix(&root)));
    }
    let mut out = vec!["# Measured leverage: the runs".to_owned(), String::new(), format!("Folder: `{}`. Every run is here, failed ones included.", name_of(&root)), String::new()];
    let mut seen: Vec<String> = Vec::new();
    let default_weights = || settings().ok().and_then(|s| s.get("token_weights").cloned()).unwrap_or(Json::Null);
    let mut weight_sets: BTreeMap<String, Json> = BTreeMap::new();
    for (_, r, _) in &runs {
        let s = r.get("settings").cloned().unwrap_or(Json::Null);
        if let Some(build) = text(r, "engine_build").filter(|b| !b.is_empty()) {
            let built = format!("- engine build of the {} runs: {build}", text(r, "job").unwrap_or(""));
            if !seen.contains(&built) {
                seen.push(built);
            }
        }
        let line = format!(
            "- {}: builder {}, cap {} tokens and {} min; judge {}; {} run(s) a cell",
            text(r, "job").unwrap_or(""),
            text(&s, "builder_model").unwrap_or(""),
            commas(at(&s, &["limits", "token_cap"]).and_then(Json::as_f64).map_or(0, |n| n as i64)),
            at(&s, &["limits", "wall_s"]).and_then(Json::as_f64).map_or(0, |n| n as i64) / 60,
            text(&s, "judge_model").unwrap_or(""),
            whole(&s, "runs_per_cell").unwrap_or(0)
        );
        if !seen.contains(&line) {
            seen.push(line);
        }
        let w = s.get("token_weights").cloned().filter(|w| truthy(Some(w))).unwrap_or_else(default_weights);
        // (Compared with their keys sorted, as two ways of writing one set are one set.)
        let mut pairs: Vec<(String, Json)> = w.as_object().map(<[(String, Json)]>::to_vec).unwrap_or_default();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        weight_sets.insert(Json::Obj(pairs).to_string(), w);
    }
    let w = weight_sets.values().next().cloned().unwrap_or(Json::Null);
    if weight_sets.len() > 1 {
        seen.push("- these runs were weighted with more than one set of weights (each run's file has its own); the first is described below".to_owned());
    }
    out.push("## Settings these runs used".to_owned());
    out.push(String::new());
    out.extend(seen);
    out.push(String::new());
    out.push(format!(
        "Tokens are given by kind: new in the context (sent, and written to the cache), read again (the context so far, re-read from the cache at every request), written out. \"Weighted\" is one figure for them: {} (settings.json's `token_weights`: relative prices, change them for yours).",
        TOKEN_KINDS.iter().map(|k| format!("{k} x {}", weight(num(&w, k).unwrap_or(0.0)))).collect::<Vec<_>>().join(", ")
    ));
    out.push(String::new());
    out.extend(
        [
            "## Runs",
            "",
            "| Run | Weighted tokens | Price (USD) | New in context | Read again | Written out | Requests | Tool uses | Minutes | Ended | Engine calls | Bytes sent / answered | Checks passed | of which checkable | Defects | Lines left |",
            "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|",
        ]
        .map(str::to_owned),
    );
    let mut table: Vec<Row> = Vec::new();
    for (name, r, c) in &runs {
        let b = c.get("builder").cloned().unwrap_or(Json::Obj(Vec::new()));
        let v = c.get("verdict").cloned().filter(|v| truthy(Some(v)));
        let e = c.get("engine").cloned().filter(|e| truthy(Some(e)));
        let left = c.get("left").cloned().unwrap_or(Json::Obj(Vec::new()));
        let cap = at(r, &["settings", "limits", "token_cap"]).and_then(Json::as_f64).unwrap_or(f64::MAX);
        let over = match num(&b, "weighted") {
            Some(weighted) => weighted > cap,
            None => truthy(b.get("over_token_cap")),
        };
        let ended = format!("{}{}{}", text(&b, "ended").unwrap_or("not recorded"), if over { " (past the token cap)" } else { "" }, if truthy(b.get("over_wall")) { " (past the time limit)" } else { "" });
        let checks = v.as_ref().map_or_else(|| "not judged".to_owned(), |v| passed_of(v, "all"));
        let checkable = v.as_ref().map_or_else(|| "-".to_owned(), |v| passed_of(v, "checkable"));
        let k = b.get("by_kind").cloned().filter(|k| truthy(Some(k)));
        let usd = num(&b, "cost_usd").map_or_else(|| "-".to_owned(), |x| format!("{x:.2}"));
        let partial = if b.get("tokens_complete").is_none_or(|t| truthy(Some(t))) { "" } else { " (incomplete: stopped before its totals)" };
        let kind = |name: &str| k.as_ref().and_then(|k| num(k, name));
        let new = k.as_ref().map(|k| num(k, "input").unwrap_or(0.0) + num(k, "cache_write").unwrap_or(0.0));
        let minutes = num(&b, "duration_s").map_or_else(|| "-".to_owned(), |d| format!("{:.0}", d / 60.0));
        let defects = v.as_ref().map_or_else(|| "-".to_owned(), |v| v.get("defects").and_then(Json::as_array).map_or(0, <[Json]>::len).to_string());
        let engine_calls = e.as_ref().map_or_else(|| "-".to_owned(), |e| fmt(num(e, "calls")));
        let bytes = e.as_ref().map_or_else(|| "-".to_owned(), |e| format!("{} / {}", fmt(num(e, "args_bytes")), fmt(num(e, "answer_bytes"))));
        out.push(format!(
            "| {name} | {}{partial} | {usd} | {} | {} | {} | {} | {} | {minutes} | {ended} | {engine_calls} | {bytes} | {checks} | {checkable} | {defects} | {} |",
            fmt(num(&b, "weighted")),
            new.map_or_else(|| "-".to_owned(), |n| fmt(Some(n))),
            fmt(kind("cache_read")),
            fmt(kind("output")),
            fmt(kind("requests")),
            fmt(num(&b, "tool_uses")),
            fmt(num(&left, "text_lines")),
        ));
        let failed = v.as_ref().map(|v| {
            Json::Arr(
                v.get("items")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter(|i| !truthy(i.get("pass")))
                    .map(|i| Json::obj([("id", i.get("id").cloned().unwrap_or(Json::Null)), ("evidence", i.get("evidence").cloned().unwrap_or(Json::Null))]))
                    .collect(),
            )
        });
        let or_null = |j: Option<Json>| j.unwrap_or(Json::Null);
        let json = Json::obj([
            ("run", Json::Str(name.clone())),
            ("job", or_null(r.get("job").cloned())),
            ("arm", or_null(r.get("arm").cloned())),
            ("n", or_null(r.get("n").cloned())),
            ("from", or_null(r.get("from").cloned())),
            ("engine_build", or_null(r.get("engine_build").cloned())),
            ("builder", b.clone()),
            ("engine", or_null(e)),
            ("left", left),
            ("verdict", or_null(v.as_ref().and_then(|v| v.get("totals").cloned()))),
            ("defects", or_null(v.as_ref().and_then(|v| v.get("defects").cloned()))),
            ("judge", or_null(c.get("judge").cloned())),
            ("failed", or_null(failed)),
        ]);
        table.push(Row {
            run: name.clone(),
            job: text(r, "job").unwrap_or("").to_owned(),
            arm: text(r, "arm").unwrap_or("").to_owned(),
            engine_build: text(r, "engine_build").unwrap_or("").to_owned(),
            builder: b,
            verdict: v.as_ref().and_then(|v| v.get("totals").cloned()),
            json,
        });
    }
    out.extend(
        [
            "",
            "## By job and arm (the mean of each cell's runs that were judged)",
            "",
            "| Job | Arm | Runs | Weighted tokens | Lowest to highest | Minutes | Checks passed | Weighted tokens a passed check |",
            "|---|---|---|---|---|---|---|---|",
        ]
        .map(str::to_owned),
    );
    let mut cells: BTreeMap<(String, String, String), Vec<&Row>> = BTreeMap::new();
    for t in &table {
        cells.entry((t.job.clone(), t.arm.clone(), t.engine_build.clone())).or_default().push(t);
    }
    let mut builds: BTreeMap<(String, String), Vec<&String>> = BTreeMap::new();
    for (job, arm, build) in cells.keys() {
        builds.entry((job.clone(), arm.clone())).or_default().push(build);
    }
    for ((job, arm, build), ts) in &cells {
        // (An arm run on two engine builds is two cells: the build is part of what was run.)
        let arm = if builds[&(job.clone(), arm.clone())].len() > 1 { format!("{arm}, build {build}") } else { arm.clone() };
        let done: Vec<&&Row> = ts.iter().filter(|t| t.verdict.is_some() && num(&t.builder, "weighted").is_some() && t.builder.get("tokens_complete").is_none_or(|x| truthy(Some(x)))).collect();
        if done.is_empty() {
            out.push(format!("| {job} | {arm} | {} | - | - | - | not judged | - |", ts.len()));
            continue;
        }
        let n = done.len() as f64;
        let weighted: Vec<f64> = done.iter().filter_map(|t| num(&t.builder, "weighted")).collect();
        let tokens = weighted.iter().sum::<f64>() / n;
        let minutes = done.iter().filter_map(|t| num(&t.builder, "duration_s")).sum::<f64>() / n / 60.0;
        let all = |t: &Row, k: &str| t.verdict.as_ref().and_then(|v| at(v, &["all", k])).and_then(Json::as_f64).unwrap_or(0.0);
        let passed = done.iter().map(|t| all(t, "passed")).sum::<f64>() / n;
        let of = all(done[0], "of") as i64;
        let per = if passed > 0.0 { commas0(tokens / passed) } else { "none passed".to_owned() };
        let (low, high) = (weighted.iter().copied().fold(f64::MAX, f64::min), weighted.iter().copied().fold(f64::MIN, f64::max));
        out.push(format!("| {job} | {arm} | {} of {} | {} | {} to {} | {minutes:.0} | {passed:.1} / {of} | {per} |", done.len(), ts.len(), commas0(tokens), fmt(Some(low)), fmt(Some(high))));
    }
    let object = |j: Option<&Json>| j.cloned().filter(|j| j.as_object().is_some()).unwrap_or(Json::Obj(Vec::new()));
    let last: Vec<Json> = runs.iter().map(|(_, _, c)| object(c.get("judge"))).collect();
    let earlier: Vec<Json> = runs.iter().flat_map(|(_, _, c)| c.get("judged_before").and_then(Json::as_array).unwrap_or(&[]).iter().map(|b| object(b.get("judge")))).collect();
    let weighed = |js: &[Json]| js.iter().filter(|j| num(j, "weighted").is_some()).count();
    let judgings: Vec<&Json> = last.iter().chain(&earlier).filter(|j| num(j, "weighted").is_some()).collect();
    if !judgings.is_empty() {
        let total: f64 = judgings.iter().filter_map(|j| num(j, "weighted")).sum();
        let tail = if earlier.is_empty() { ".".to_owned() } else { format!(" ({} of them earlier judgings of runs judged again).", weighed(&earlier)) };
        out.push(String::new());
        out.push(format!("Judging: {} judging(s) of {} run(s), {} weighted tokens in all{tail}", judgings.len(), weighed(&last), fmt(Some(total))));
    }
    let usd = |js: &[Json]| js.iter().filter_map(|j| num(j, "cost_usd")).sum::<f64>();
    let built: f64 = table.iter().filter_map(|t| num(&t.builder, "cost_usd")).sum();
    let (judged_usd, earlier_usd) = (usd(&last), usd(&earlier));
    if built != 0.0 || judged_usd != 0.0 || earlier_usd != 0.0 {
        let tail = if earlier.is_empty() {
            format!(" ({:.2} USD in all).", built + judged_usd)
        } else {
            format!(", and {earlier_usd:.2} USD for the earlier judgings of the runs judged again ({:.2} USD in all).", built + judged_usd + earlier_usd)
        };
        out.push(String::new());
        out.push(format!("At the runner's list prices: {built:.2} USD for the builds, {judged_usd:.2} USD for the judging shown above{tail}"));
    }
    let again: Vec<(&String, &[Json])> = runs.iter().filter_map(|(name, _, c)| c.get("judged_before").and_then(Json::as_array).filter(|b| !b.is_empty()).map(|b| (name, b))).collect();
    if !again.is_empty() {
        out.extend(["", "## Judged more than once", "", "The figures above are the last judging. Before it:", ""].map(str::to_owned));
        for (name, before) in again {
            for b in before {
                let v0 = b.get("verdict").filter(|v| truthy(Some(v)));
                let was = v0.map_or_else(|| "no verdict".to_owned(), |v| passed_of(v, "all"));
                let failed: Vec<&str> = v0.and_then(|v| v.get("items")).and_then(Json::as_array).unwrap_or(&[]).iter().filter(|i| !truthy(i.get("pass"))).filter_map(|i| text(i, "id")).collect();
                let failed = if failed.is_empty() { String::new() } else { format!(" (failed: {})", failed.join(", ")) };
                let why = b.get("why").map_or_else(|| "None".to_owned(), |w| w.as_str().map_or_else(|| w.to_string(), str::to_owned));
                out.push(format!("- {name}: {was}{failed}. Judged again because: {why}"));
            }
        }
    }
    let failed: Vec<String> = table
        .iter()
        .flat_map(|t| t.json.get("failed").and_then(Json::as_array).unwrap_or(&[]).iter().map(move |i| format!("- {} **{}**: {}", t.run, text(i, "id").unwrap_or(""), text(i, "evidence").unwrap_or(""))))
        .collect();
    if !failed.is_empty() {
        out.extend(["", "## Items that failed", ""].map(str::to_owned));
        out.extend(failed);
    }
    out.extend(["", "## What each judge found beyond the list", ""].map(str::to_owned));
    for t in &table {
        for d in t.json.get("defects").and_then(Json::as_array).unwrap_or(&[]) {
            out.push(format!("- {}: {}", t.run, d.as_str().map_or_else(|| d.to_string(), str::to_owned)));
        }
    }
    let said = out.join("\n") + "\n";
    std::fs::write(root.join("report.md"), &said).map_err(|e| format!("report.md: {e}"))?;
    let data = Json::obj([("leverage", Json::Num(1.0)), ("runs", Json::Arr(table.into_iter().map(|t| t.json).collect()))]);
    std::fs::write(root.join("report.json"), data.to_pretty() + "\n").map_err(|e| format!("report.json: {e}"))?;
    Ok(said)
}

// ---- what a repository keeps ------------------------------------------------------------------

fn sep(c: char) -> bool {
    c == '/' || c == '\\'
}

fn tail_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// A path of this machine, to be found in text however a tool wrote it: `C:/a/b`, `C:\a\b`,
/// `C:\\a\\b` (inside JSON), `/c/a/b`, in any case. Any run of separators stands between two
/// parts, and the last part is taken to its end, so that a longer name beginning with it leaks
/// no tail.
pub struct Place {
    drive: Option<char>,
    parts: Vec<Vec<char>>,
    name: &'static str,
}

impl Place {
    pub fn new(path: &Path, name: &'static str) -> Place {
        let posix: Vec<char> = posix(path).chars().collect();
        let (drive, rest) = if posix.len() > 1 && posix[1] == ':' { (Some(posix[0]), &posix[2..]) } else { (None, &posix[..]) };
        let parts = rest.split(|c| *c == '/').filter(|p| !p.is_empty()).map(<[char]>::to_vec).collect();
        Place { drive, parts, name }
    }

    /// How many characters at the start of `text` are this place (0: it does not start here).
    fn len_at(&self, text: &[char]) -> usize {
        let same = |a: char, b: char| a.to_lowercase().eq(b.to_lowercase());
        let mut at = 0;
        if let Some(drive) = self.drive {
            // `C:` or `/c`.
            let here = text.len() >= 2 && ((same(text[0], drive) && text[1] == ':') || (text[0] == '/' && same(text[1], drive)));
            if !here {
                return 0;
            }
            at = 2;
        }
        if self.parts.is_empty() {
            return 0;
        }
        for part in &self.parts {
            let seps = text[at..].iter().take_while(|c| sep(**c)).count();
            if seps == 0 {
                return 0;
            }
            at += seps;
            if text.len() < at + part.len() || !text[at..at + part.len()].iter().zip(part).all(|(a, b)| same(*a, *b)) {
                return 0;
            }
            at += part.len();
        }
        at + text[at..].iter().take_while(|c| tail_char(**c)).count()
    }
}

/// `text` with every place of `places` replaced by its name (the longest place first), and
/// `user` (the name of whoever ran it), wherever it stands as a word, by `<user>`.
pub fn scrub(text: &str, places: &[Place], user: Option<&str>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    'next: while i < chars.len() {
        for place in places {
            let n = place.len_at(&chars[i..]);
            if n > 0 {
                out.push_str(place.name);
                i += n;
                continue 'next;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    let Some(user) = user.filter(|u| u.chars().count() >= 3) else { return out };
    // The name, in any case, not inside a longer word; a percent-encoded separator before it
    // (%5C or %2F) ends in a letter or a digit and is not part of a word.
    let chars: Vec<char> = out.chars().collect();
    let name: Vec<char> = user.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let here = chars.len() >= i + name.len() && chars[i..i + name.len()].iter().zip(&name).all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()));
        let before: String = chars[..i].iter().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect();
        let free_before = !chars[..i].last().is_some_and(char::is_ascii_alphanumeric) || before.eq_ignore_ascii_case("%5C") || before.eq_ignore_ascii_case("%2F");
        let free_after = !chars.get(i + name.len()).is_some_and(char::is_ascii_alphanumeric);
        if here && free_before && free_after {
            out.push_str("<user>");
            i += name.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// `export ROOT DEST`: what a repository keeps of a set of runs: the report, and one file a run
/// with its settings, what it cost and its verdict. Never the games, the transcripts or a path
/// of the machine they ran on (each is replaced by a name).
pub fn export(root: &Path, dest: &Path) -> R<Json> {
    let (root, dest) = (absolute(root), absolute(dest));
    let said = report(&root)?;
    print!("{}", said.chars().map(|c| if c.is_ascii() { c } else { '?' }).collect::<String>());
    std::fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", posix(&dest)))?;
    let repo = repo();
    let mut places = vec![Place::new(&root, "<runs>"), Place::new(&repo, "<repo>")];
    let home = home();
    if let Some(home) = &home {
        places.push(Place::new(home, "<home>"));
    }
    // (A worktree's main checkout is a path of the machine too.)
    if let Some(common) = crate::run::said_text(&["git", "rev-parse", "--git-common-dir"], &repo, Duration::from_secs(30)) {
        let common = common.trim();
        if !common.is_empty() {
            if let Some(main) = absolute(&repo.join(common)).parent().map(Path::to_path_buf) {
                if std::fs::canonicalize(&main).ok() != std::fs::canonicalize(&repo).ok() {
                    places.push(Place::new(&main, "<repo>"));
                }
            }
        }
    }
    places.sort_by_key(|p| std::cmp::Reverse(p.parts.iter().map(|part| part.len() + 1).sum::<usize>()));
    let user = home.as_ref().and_then(|h| h.file_name()).map(|n| n.to_string_lossy().into_owned());
    let clean = |text: &str| scrub(text, &places, user.as_deref());
    for name in ["report.md", "report.json"] {
        let text = std::fs::read_to_string(root.join(name)).map_err(|e| format!("{name}: {e}"))?;
        std::fs::write(dest.join(name), clean(&text)).map_err(|e| format!("{name}: {e}"))?;
    }
    let mut kept = 0;
    for run in runs_of(&root) {
        let mut r = load_json(&run.join("run.json"))?;
        take(&mut r, "exe");
        let mut c = read_cost(&run)?;
        for who in ["builder", "judge"] {
            if let Some(mut row) = take(&mut c, who) {
                take(&mut row, "runner");
                c.set(who, row);
            }
        }
        let builder = c.get("builder").cloned().unwrap_or(Json::Null);
        if c.get("engine_stopped").is_some() && builder.get("engine_stopped").is_none() {
            // (Recorded from `specula stop`'s exit code, which is 1 for an engine that had
            // stopped already as for one that would not: not known, so not said.)
            take(&mut c, "engine_stopped");
        }
        if let Some(weighted) = num(&builder, "weighted") {
            // (Held against the weighted figure, as the report holds it.)
            let cap = r.get("settings").and_then(|s| s.get("limits")).and_then(|l| num(l, "token_cap")).unwrap_or(f64::MAX);
            let mut row = builder.clone();
            row.set("over_token_cap", Json::Bool(weighted > cap));
            c.set("builder", row);
        }
        if let Ok(where_) = where_of(&run) {
            c.set("where", where_);
        }
        let text = Json::obj([("run", r), ("costs", c)]).to_pretty() + "\n";
        std::fs::write(dest.join(format!("{}.json", name_of(&run))), clean(&text)).map_err(|e| format!("{}: {e}", name_of(&run)))?;
        kept += 1;
    }
    Ok(Json::obj([("kept", Json::Num(f64::from(kept))), ("in", Json::Str(posix(&dest)))]))
}

// ---- where a builder's tokens went ----------------------------------------------------------

fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Whether `word` stands in `text` as a word of its own (`\bword\b`).
fn has_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| !text[..at].chars().next_back().is_some_and(word_char) && !text[at + word.len()..].chars().next().is_some_and(word_char))
}

fn any_word(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| has_word(text, w))
}

/// Whether a command writes a file from its own text: a here-document, `cat >`, or `tee`.
fn writes_a_file(low: &str) -> bool {
    let heredoc = low.match_indices("<<").any(|(at, _)| {
        let rest = low[at + 2..].trim_start().trim_start_matches(['\'', '"']);
        rest.chars().next().is_some_and(word_char)
    });
    let cat_into = low.match_indices("cat").any(|(at, _)| !low[..at].chars().next_back().is_some_and(word_char) && low[at + 3..].trim_start().starts_with('>'));
    heredoc || cat_into || has_word(low, "tee")
}

/// Whether a command runs a Python script of the builder's own, or a script with the words of
/// a check after it.
fn runs_its_checks(low: &str) -> bool {
    // `python[0-9.]*\s+[\w./"-]*\.py`
    let python = low.match_indices("python").any(|(at, _)| {
        let rest = low[at + 6..].trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
        let spaced = rest.trim_start();
        spaced.len() < rest.len() && {
            let word: String = spaced.chars().take_while(|c| word_char(*c) || matches!(c, '.' | '/' | '"' | '-')).collect();
            word.contains(".py")
        }
    });
    // `\.py\b.*(run|frame|--headless|--ticks)`
    let script = low.match_indices(".py").any(|(at, _)| {
        let after = &low[at + 3..];
        // (To the end of its line: the pattern it was does not cross a line end.)
        let after = after.lines().next().unwrap_or("");
        !after.chars().next().is_some_and(word_char) && ["run", "frame", "--headless", "--ticks"].iter().any(|w| after.contains(w))
    });
    python || script
}

/// What one step of a builder was, from the tool it used and on what (Claude Code's tools:
/// Bash, Read, Write, Edit, Glob, Grep). A reading of a command's text: good enough to say
/// where a run's steps went, not an exact account.
pub fn step_kind(name: &str, input: &Json) -> &'static str {
    let repo = posix(&repo()).to_lowercase();
    let inside = |text: &str| text.replace('\\', "/").to_lowercase().contains(&repo);
    let said = |key: &str| input.get(key).map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned)).unwrap_or_default();
    match name {
        "Write" | "Edit" => return "write code",
        "Read" => {
            let target = said("file_path");
            return if target.to_lowercase().ends_with(".png") {
                "look at a frame"
            } else if inside(&target) {
                "read the engine's documents and source"
            } else {
                "read its own files"
            };
        }
        "Glob" | "Grep" => return if inside(&format!("{}{}", said("path"), said("pattern"))) { "read the engine's documents and source" } else { "read its own files" },
        "Bash" => {}
        _ => return "other",
    }
    let low = said("command").replace('\\', "/").to_lowercase();
    // The harness's own command: the script it was through M5, or this tool.
    let harness = low.contains("leverage.py") || (low.contains("examen") && has_word(&low, "leverage"));
    if harness {
        let call = has_word(&low, "call");
        // (On one line, as the pattern it was: `bcallb.*logic_build` does not cross a line end.)
        let builds = low.lines().any(|line| line.match_indices("call").any(|(at, _)| !line[..at].chars().next_back().is_some_and(word_char) && !line[at + 4..].chars().next().is_some_and(word_char) && line[at + 4..].contains("logic_build")));
        if builds {
            return "build the module";
        }
        if has_word(&low, "tools") || low.contains("tools_catalog") || low.contains("tools_activate") {
            return "find tools and their arguments";
        }
        if (has_word(&low, "engine start") || has_word(&low, "engine stop")) && !call {
            return "start or stop the engine";
        }
        return "call the engine";
    }
    const READERS: [&str; 10] = ["cat", "sed", "head", "tail", "grep", "rg", "ls", "find", "wc", "awk"];
    if has_word(&low, "cargo") {
        "build the module"
    } else if inside(&low) && any_word(&low, &READERS) {
        "read the engine's documents and source"
    } else if writes_a_file(&low) {
        "write code"
    } else if runs_its_checks(&low) {
        "run its own checks"
    } else if any_word(&low, &["cat", "sed", "head", "tail", "grep", "ls", "find", "wc"]) {
        "read its own files"
    } else {
        "other commands"
    }
}

/// Whether a build's answer says it failed: `"ok": false`, an `"errors": N` that is not 0, or
/// a line that starts with `error:` or `error[E0123]:`.
fn build_failed(text: &str) -> bool {
    let counted = text.match_indices("\"errors\": ").any(|(at, m)| {
        let digits: String = text[at + m.len()..].chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u64>().is_ok_and(|n| n > 0)
    });
    let compiler = text.lines().any(|line| {
        let Some(rest) = line.strip_prefix("error") else { return false };
        rest.starts_with(':') || rest.strip_prefix("[E").is_some_and(|code| code.trim_start_matches(|c: char| c.is_ascii_digit()).starts_with("]:") && code.starts_with(|c: char| c.is_ascii_digit()))
    });
    text.contains("\"ok\": false") || counted || compiler
}

/// Python's `len(json.dumps(x))`: a value as JSON with a space after each comma and colon.
fn dumps_len(j: &Json) -> usize {
    match j {
        Json::Arr(items) if !items.is_empty() => 2 + items.iter().map(dumps_len).sum::<usize>() + 2 * (items.len() - 1),
        Json::Obj(pairs) if !pairs.is_empty() => 2 + pairs.iter().map(|(k, v)| python_str_len(k) + 2 + dumps_len(v)).sum::<usize>() + 2 * (pairs.len() - 1),
        Json::Str(s) => python_str_len(s),
        other => other.to_string().chars().count(),
    }
}

/// A string as Python's `json.dumps` writes it (`ensure_ascii`): its length in characters.
fn python_str_len(s: &str) -> usize {
    2 + s
        .chars()
        .map(|c| match c {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
            c if (c as u32) < 0x20 || (0x7f..=0xffff).contains(&(c as u32)) => 6,
            c if (c as u32) > 0xffff => 12,
            _ => 1,
        })
        .sum::<usize>()
}

#[derive(Default, Clone)]
struct Kind {
    steps: u64,
    answered: u64,
    written: u64,
}

/// A builder's steps by kind, what each kind's answers put into the context and what the
/// builder wrote for it (characters / 4, as tokens: a measure of share, beside the run's own
/// exact counts by kind), and how many steps came before the first line of code.
pub fn where_of(run: &Path) -> R<Json> {
    let run = absolute(run);
    let name = name_of(&run);
    let stream = costs_dir(&run)?.join(format!("{name}.builder.stream.jsonl"));
    if !stream.is_file() {
        return Err(format!("{name}: no builder's stream (only runs started by 'run' keep one)"));
    }
    let mut kinds: Vec<(&'static str, Kind)> = Vec::new();
    fn row<'k>(kinds: &'k mut Vec<(&'static str, Kind)>, kind: &'static str) -> &'k mut Kind {
        if let Some(at) = kinds.iter().position(|(k, _)| *k == kind) {
            return &mut kinds[at].1;
        }
        kinds.push((kind, Kind::default()));
        &mut kinds.last_mut().expect("just pushed").1
    }
    let mut pending: Vec<(String, &'static str)> = Vec::new();
    let mut order: Vec<&'static str> = Vec::new();
    let (mut builds, mut failed) = (0u64, 0u64);
    let all = std::fs::read(&stream).map_err(|e| format!("{}: {e}", posix(&stream)))?;
    for line in String::from_utf8_lossy(&all).lines() {
        let Ok(j) = Json::parse(line) else { continue };
        let Some(content) = j.get("message").and_then(|m| m.get("content")).and_then(Json::as_array) else { continue };
        let who = text(&j, "type").unwrap_or("");
        for c in content.iter().filter(|c| c.as_object().is_some()) {
            match (who, text(c, "type").unwrap_or("")) {
                ("assistant", "tool_use") => {
                    let input = c.get("input").cloned().filter(|i| truthy(Some(i))).unwrap_or(Json::Obj(Vec::new()));
                    let kind = step_kind(text(c, "name").unwrap_or(""), &input);
                    pending.push((c.get("id").map(Json::to_string).unwrap_or_default(), kind));
                    order.push(kind);
                    let r = row(&mut kinds, kind);
                    r.steps += 1;
                    r.written += (dumps_len(&input) / 4) as u64;
                }
                ("assistant", "text" | "thinking") => {
                    let said = text(c, "text").filter(|t| !t.is_empty()).or_else(|| text(c, "thinking")).unwrap_or("");
                    row(&mut kinds, "words between steps").written += (said.chars().count() / 4) as u64;
                }
                ("user", "tool_result") => {
                    let out = c.get("content");
                    let said = match out {
                        Some(Json::Str(s)) => s.clone(),
                        Some(Json::Arr(parts)) => parts.iter().filter(|x| x.as_object().is_some()).map(|x| text(x, "text").unwrap_or("")).collect::<Vec<_>>().join(" "),
                        _ => String::new(),
                    };
                    let id = c.get("tool_use_id").map(Json::to_string).unwrap_or_default();
                    let kind = pending.iter().rev().find(|(known, _)| *known == id).map_or("other", |(_, k)| *k);
                    row(&mut kinds, kind).answered += (said.chars().count() / 4) as u64;
                    if kind == "build the module" {
                        // (A build the builder asked for itself, and whether its answer says it failed.)
                        builds += 1;
                        failed += u64::from(build_failed(&said));
                    }
                }
                _ => {}
            }
        }
    }
    let b = read_cost(&run)?.get("builder").cloned().unwrap_or(Json::Null);
    let k = b.get("by_kind").cloned().filter(|k| truthy(Some(k)));
    let w = read_run(&run)?.get("settings").and_then(|s| s.get("token_weights")).cloned().filter(|w| truthy(Some(w))).map_or_else(|| settings().map(|s| s.get("token_weights").cloned().unwrap_or(Json::Null)), Ok)?;
    let split = match &k {
        None => Json::Obj(Vec::new()),
        Some(k) => {
            let part = |kind: &str| num(k, kind).unwrap_or(0.0) * num(&w, kind).unwrap_or(0.0);
            Json::obj([
                ("read_again", Json::Num(part("cache_read").round_ties_even())),
                ("written_out", Json::Num(part("output").round_ties_even())),
                ("new_in_context", Json::Num((part("cache_write") + part("input")).round_ties_even())),
            ])
        }
    };
    let before = order.iter().position(|kind| *kind == "write code").unwrap_or(order.len());
    // (The largest share first; two equal shares stay in the order they were first met.)
    kinds.sort_by_key(|(_, r)| std::cmp::Reverse(r.answered + r.written));
    let by_step = Json::Obj(
        kinds
            .into_iter()
            .map(|(kind, r)| (kind.to_owned(), Json::obj([("steps", Json::Num(r.steps as f64)), ("answered_tokens", Json::Num(r.answered as f64)), ("written_tokens", Json::Num(r.written as f64))])))
            .collect(),
    );
    Ok(Json::obj([
        ("run", Json::Str(name)),
        ("requests", k.as_ref().and_then(|k| k.get("requests").cloned()).unwrap_or(Json::Null)),
        ("steps", Json::Num(order.len() as f64)),
        ("steps_before_the_first_code", Json::Num(before as f64)),
        ("weighted", b.get("weighted").cloned().unwrap_or(Json::Null)),
        ("weighted_by_kind", split),
        ("builds", Json::obj([("steps", Json::Num(builds as f64)), ("failed", Json::Num(failed as f64))])),
        ("how", Json::Str("weighted_by_kind is exact (the run's own token counts times the weights); a step's kind is a reading of its command's text, and answered_tokens / written_tokens are characters / 4".to_owned())),
        ("by_step", by_step),
    ]))
}

/// `where RUN [RUN ...]`: as a table for a person.
pub fn where_text(w: &Json) -> String {
    let split = w.get("weighted_by_kind").filter(|s| truthy(Some(s)));
    let n = |j: &Json, k: &str| fmt(num(j, k));
    let mut out = format!(
        "\n{}: {} requests, {} steps ({} before the first code); weighted {}",
        text(w, "run").unwrap_or(""),
        w.get("requests").and_then(Json::as_f64).map_or_else(|| "None".to_owned(), |r| format!("{r}")),
        n(w, "steps"),
        n(w, "steps_before_the_first_code"),
        n(w, "weighted")
    );
    if let Some(s) = split {
        out.push_str(&format!(" = read again {} + written out {} + new in the context {}", n(s, "read_again"), n(s, "written_out"), n(s, "new_in_context")));
    }
    out.push_str(&format!("\n  {:40} {:>5} {:>18} {:>18}", "step", "steps", "answers (~tokens)", "written (~tokens)"));
    for (kind, r) in w.get("by_step").and_then(Json::as_object).unwrap_or(&[]) {
        out.push_str(&format!("\n  {kind:40} {:5} {:>18} {:>18}", num(r, "steps").unwrap_or(0.0), n(r, "answered_tokens"), n(r, "written_tokens")));
    }
    out
}

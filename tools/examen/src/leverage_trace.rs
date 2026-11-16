//! A run's ledger: every request its builder made, what it read, what it wrote, and what that
//! cost (`examen leverage trace RUN`).
//!
//! `where` says what kinds of steps a run was made of. This says what each request cost and
//! why: a request reads the whole context so far, so what a step puts into the context is paid
//! for again at every later request. The ledger gives each request its own cost and the cost
//! of carrying what it added.
//!
//! **What is exact and what is not.** A request's tokens read (new, and read again from the
//! cache) are the run's own records, and so is how far the context grew between two requests.
//! A request's tokens written out are not in a stream (only the run's total is): each
//! request's share is its growth of the context less its answers' size (characters / 4), and
//! the shares are scaled to the exact total. What was written and cannot be seen in the
//! stream (thinking) is the difference between that share and the visible text.

use std::path::Path;

use specula_core::Json;

use crate::leverage::{absolute, costs_dir, fmt, num, posix, read_cost, read_run, repo, settings, text, truthy, R, TOKEN_KINDS};
use crate::leverage_report::{dumps_len, home, scrub, step_kind, Place};

/// One tool use of a request.
struct Step {
    id: String,
    tool: String,
    kind: &'static str,
    what: String,
    /// The file a Read, Write or Edit named (as said, scrubbed).
    file: Option<String>,
    written: usize,
    answered: usize,
}

/// One request to the model.
struct Request {
    id: String,
    /// input, cache_write, cache_read at this request (exact).
    read: [f64; 3],
    /// Visible characters of its text.
    said: usize,
    steps: Vec<Step>,
}

impl Request {
    fn context(&self) -> f64 {
        self.read.iter().sum()
    }
}

fn tokens(chars: usize) -> f64 {
    (chars / 4) as f64
}

/// A command or a target, short enough for a row and with no path of this machine in it.
fn short(said: &str, places: &[Place], user: Option<&str>) -> String {
    let clean = scrub(said, places, user);
    let one: String = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    // (The harness's own command line is long and the same in every row: its tool is what matters.)
    let one = match one.find(" leverage ") {
        Some(at) if one[..at].to_lowercase().contains("examen") => format!("examen{}", &one[at..]),
        _ => one,
    };
    let one = one.replace("\"<run>\" ", "").replace("<run> ", "");
    // What lies under the home folder is this machine's (a temporary folder's names, a
    // session's): only the file's own name is kept.
    let mut kept = String::with_capacity(one.len());
    let mut rest = one.as_str();
    while let Some(at) = rest.find("<home>") {
        kept.push_str(&rest[..at]);
        let path: String = rest[at + 6..].chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '\\' | '.' | '_' | '-')).collect();
        let name = path.rsplit(['/', '\\']).next().unwrap_or("");
        kept.push_str("<home>");
        if !name.is_empty() {
            kept.push_str(if name.len() < path.trim_start_matches(['/', '\\']).len() { "/.../" } else { "/" });
            kept.push_str(name);
        }
        rest = &rest[at + 6 + path.len()..];
    }
    kept.push_str(rest);
    let one = kept;
    if one.chars().count() > 110 {
        format!("{}...", one.chars().take(107).collect::<String>())
    } else {
        one
    }
}

/// The places of the machine a run's stream names: where the run was (its process's own
/// folder, which the stream's first line says; the folder it is in now may be another), the
/// repository and the home folder.
fn places_of(stream: &str, run: &Path) -> (Vec<Place>, Option<String>) {
    let mut places = vec![Place::new(run, "<run>"), Place::new(&repo(), "<repo>")];
    let was = stream.lines().take(8).filter_map(|l| Json::parse(l).ok()).find_map(|j| text(&j, "cwd").map(str::to_owned));
    if let Some(was) = was {
        places.push(Place::new(Path::new(&was), "<run>"));
    }
    let home = home();
    if let Some(home) = &home {
        places.push(Place::new(home, "<home>"));
    }
    places.sort_by_key(|p| std::cmp::Reverse(p.len()));
    (places, home.as_ref().and_then(|h| h.file_name()).map(|n| n.to_string_lossy().into_owned()))
}

fn requests_of(stream: &str, places: &[Place], user: Option<&str>) -> Vec<Request> {
    let mut out: Vec<Request> = Vec::new();
    for line in stream.lines() {
        let Ok(j) = Json::parse(line) else { continue };
        let Some(m) = j.get("message").filter(|m| m.as_object().is_some()) else { continue };
        let Some(content) = m.get("content").and_then(Json::as_array) else { continue };
        match text(&j, "type") {
            Some("assistant") => {
                let id = m.get("id").map(Json::to_string).unwrap_or_default();
                if out.last().is_none_or(|r| r.id != id) {
                    let u = m.get("usage");
                    let n = |name: &str| u.and_then(|u| num(u, name)).unwrap_or(0.0);
                    out.push(Request { id, read: [n("input_tokens"), n("cache_creation_input_tokens"), n("cache_read_input_tokens")], said: 0, steps: Vec::new() });
                }
                let r = out.last_mut().expect("just pushed, or the same request");
                for c in content.iter().filter(|c| c.as_object().is_some()) {
                    match text(c, "type") {
                        Some("text") => r.said += text(c, "text").map_or(0, |t| t.chars().count()),
                        Some("tool_use") => {
                            let input = c.get("input").cloned().filter(|i| truthy(Some(i))).unwrap_or(Json::Obj(Vec::new()));
                            let tool = text(c, "name").unwrap_or("").to_owned();
                            let said = |key: &str| text(&input, key).map(str::to_owned);
                            let file = said("file_path").map(|f| short(&f, places, user));
                            let target = said("command").or_else(|| said("file_path")).or_else(|| said("pattern")).unwrap_or_default();
                            r.steps.push(Step {
                                id: c.get("id").map(Json::to_string).unwrap_or_default(),
                                kind: step_kind(&tool, &input),
                                what: short(&target, places, user),
                                file,
                                written: dumps_len(&input),
                                answered: 0,
                                tool,
                            });
                        }
                        _ => {}
                    }
                }
            }
            Some("user") => {
                for c in content.iter().filter(|c| text(c, "type") == Some("tool_result")) {
                    let said = match c.get("content") {
                        Some(Json::Str(s)) => s.chars().count(),
                        Some(Json::Arr(parts)) => parts.iter().filter_map(|x| text(x, "text")).map(|t| t.chars().count()).sum(),
                        _ => 0,
                    };
                    let id = c.get("tool_use_id").map(Json::to_string).unwrap_or_default();
                    if let Some(step) = out.iter_mut().rev().flat_map(|r| r.steps.iter_mut()).find(|s| s.id == id) {
                        step.answered += said;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// What reading the context cost at each request, shared among what was in it: the start
/// (`[0]`: the harness's own prompt, its tool list and the brief) and what each request
/// added (`[i + 1]`). At a request the oldest tokens are the ones read again from the cache
/// and the newest the ones written to it, so each item is charged the rate of where it lies.
/// The parts sum to the run's exact cost of reading.
fn carried(reqs: &[Request], grew: &[f64], w: [f64; 3]) -> Vec<f64> {
    let mut cost = vec![0.0; reqs.len() + 1];
    for (i, r) in reqs.iter().enumerate() {
        let [input, write, read] = r.read;
        let fresh = input + write;
        let fresh_rate = if fresh > 0.0 { (input * w[0] + write * w[1]) / fresh } else { 0.0 };
        // The items in the context at request i, oldest first.
        let start = reqs.first().map_or(0.0, Request::context);
        let mut left_cached = read;
        for (item, size) in std::iter::once(start).chain(grew.iter().copied().take(i)).enumerate() {
            let cached = size.min(left_cached);
            left_cached -= cached;
            cost[item] += cached * w[2] + (size - cached) * fresh_rate;
        }
    }
    cost
}

/// `trace RUN`: the ledger of a builder's requests (`judge`: of its judge's).
pub fn trace_of(run: &Path, judge: bool) -> R<Json> {
    let run = absolute(run);
    let name = run.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let who = if judge { "judge" } else { "builder" };
    let path = costs_dir(&run)?.join(format!("{name}.{who}.stream.jsonl"));
    if !path.is_file() {
        return Err(format!("{name}: no {who}'s stream (only runs started by 'run' keep one)"));
    }
    let all = std::fs::read(&path).map_err(|e| format!("{}: {e}", posix(&path)))?;
    let stream = String::from_utf8_lossy(&all);
    let (places, user) = places_of(&stream, &run);
    let reqs = requests_of(&stream, &places, user.as_deref());
    if reqs.is_empty() {
        return Err(format!("{name}: its {who}'s stream has no request"));
    }
    let cost = read_cost(&run)?.get(who).cloned().unwrap_or(Json::Null);
    let by_kind = cost.get("by_kind").cloned().unwrap_or(Json::Null);
    let weights = read_run(&run)?.get("settings").and_then(|s| s.get("token_weights")).cloned().filter(|w| truthy(Some(w))).map_or_else(|| settings().map(|s| s.get("token_weights").cloned().unwrap_or(Json::Null)), Ok)?;
    let w: Vec<f64> = TOKEN_KINDS.iter().map(|k| num(&weights, k).unwrap_or(0.0)).collect();
    let n = reqs.len();

    // How far the context grew after each request (exact between two requests; the last one's
    // is what can be seen of it).
    let answered = |r: &Request| r.steps.iter().map(|s| tokens(s.answered)).sum::<f64>();
    let visible = |r: &Request| tokens(r.said) + r.steps.iter().map(|s| tokens(s.written)).sum::<f64>();
    let grew: Vec<f64> = (0..n).map(|i| if i + 1 < n { (reqs[i + 1].context() - reqs[i].context()).max(0.0) } else { visible(&reqs[i]) + answered(&reqs[i]) }).collect();
    // Written out, a request: its growth less its answers, scaled to the run's exact total.
    let raw: Vec<f64> = (0..n).map(|i| (grew[i] - answered(&reqs[i])).max(visible(&reqs[i])).max(0.0)).collect();
    // (A run stopped at its time limit has no total: its requests keep what could be read of them.)
    let complete = cost.get("tokens_complete").and_then(Json::as_bool) != Some(false);
    let total_out = num(&by_kind, "output").filter(|_| complete);
    let scale = match (total_out, raw.iter().sum::<f64>()) {
        (Some(total), sum) if sum > 0.0 => total / sum,
        _ => 1.0,
    };
    let out: Vec<f64> = raw.iter().map(|x| x * scale).collect();
    let carry = carried(&reqs, &grew, [w[0], w[1], w[2]]);
    let read_cost_of = |r: &Request| r.read[0] * w[0] + r.read[1] * w[1] + r.read[2] * w[2];

    let mut rows = Vec::new();
    let mut by: Vec<(&'static str, [f64; 6])> = Vec::new(); // requests, steps, answered, written out, its own cost, carried
    let mut files: Vec<(String, [f64; 4])> = Vec::new(); // reads, writes, answered tokens, written tokens
    let mut running = 0.0;
    for (i, r) in reqs.iter().enumerate() {
        let own = read_cost_of(r) + out[i] * w[3];
        running += own;
        let kind = r.steps.first().map_or("words only", |s| s.kind);
        // (A request is one kind's: its first step's. Its other steps are counted as steps.)
        let at = by.iter().position(|(k, _)| *k == kind).unwrap_or_else(|| {
            by.push((kind, [0.0; 6]));
            by.len() - 1
        });
        let b = &mut by[at].1;
        b[0] += 1.0;
        b[1] += r.steps.len() as f64;
        b[2] += answered(r);
        b[3] += out[i];
        b[4] += out[i] * w[3];
        b[5] += carry[i + 1];
        for s in &r.steps {
            let Some(file) = &s.file else { continue };
            let at = files.iter().position(|(f, _)| f == file).unwrap_or_else(|| {
                files.push((file.clone(), [0.0; 4]));
                files.len() - 1
            });
            let f = &mut files[at].1;
            if s.tool == "Read" {
                f[0] += 1.0;
                f[2] += tokens(s.answered);
            } else {
                f[1] += 1.0;
                f[3] += tokens(s.written);
            }
        }
        rows.push(Json::obj([
            ("n", Json::Num((i + 1) as f64)),
            ("context", Json::Num(r.context())),
            ("new", Json::Num(r.read[0] + r.read[1])),
            ("read_again", Json::Num(r.read[2])),
            ("written_out", Json::Num(out[i].round())),
            ("of_which_unseen", Json::Num((out[i] - visible(r)).max(0.0).round())),
            ("cost", Json::Num(own.round())),
            ("cost_so_far", Json::Num(running.round())),
            ("carried", Json::Num(carry[i + 1].round())),
            (
                "steps",
                Json::arr(r.steps.iter().map(|s| {
                    Json::obj([
                        ("tool", Json::Str(s.tool.clone())),
                        ("kind", Json::Str(s.kind.to_owned())),
                        ("what", Json::Str(s.what.clone())),
                        ("written", Json::Num(tokens(s.written))),
                        ("answered", Json::Num(tokens(s.answered))),
                    ])
                })),
            ),
        ]));
    }
    by.sort_by(|a, b| (b.1[4] + b.1[5]).total_cmp(&(a.1[4] + a.1[5])));
    files.sort_by(|a, b| (b.1[2] + b.1[3]).total_cmp(&(a.1[2] + a.1[3])));
    let reading: f64 = reqs.iter().map(read_cost_of).sum();
    let writing = out.iter().sum::<f64>() * w[3];
    let unseen: f64 = (0..n).map(|i| (out[i] - visible(&reqs[i])).max(0.0)).sum();
    Ok(Json::obj([
        ("run", Json::Str(name)),
        ("who", Json::Str(who.to_owned())),
        ("complete", Json::Bool(complete)),
        ("requests", Json::Num(n as f64)),
        ("weighted", cost.get("weighted").cloned().unwrap_or(Json::Null)),
        ("in_the_ledger", Json::Num((reading + writing).round())),
        ("reading", Json::Num(reading.round())),
        ("writing", Json::Num(writing.round())),
        ("written_out", Json::Num(out.iter().sum::<f64>().round())),
        ("of_which_unseen", Json::Num(unseen.round())),
        ("start", Json::obj([("context", Json::Num(reqs[0].context())), ("carried", Json::Num(carry[0].round()))])),
        ("context_at_end", Json::Num(reqs[n - 1].context())),
        (
            "by_kind",
            Json::Obj(
                by.into_iter()
                    .map(|(kind, b)| {
                        (
                            kind.to_owned(),
                            Json::obj([
                                ("requests", Json::Num(b[0])),
                                ("steps", Json::Num(b[1])),
                                ("answered", Json::Num(b[2])),
                                ("written_out", Json::Num(b[3].round())),
                                ("writing", Json::Num(b[4].round())),
                                ("carried", Json::Num(b[5].round())),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "files",
            Json::arr(files.into_iter().map(|(file, f)| Json::obj([("file", Json::Str(file)), ("reads", Json::Num(f[0])), ("writes", Json::Num(f[1])), ("answered", Json::Num(f[2])), ("written", Json::Num(f[3]))]))),
        ),
        ("rows", Json::Arr(rows)),
        (
            "how",
            Json::Str(
                "context, new, read_again and how far the context grew are the run's own records. written_out a request is its growth less its answers (characters / 4), scaled to the run's exact total; of_which_unseen is that less the visible text (thinking). cost is what the request read and wrote, weighted; carried is what reading again what it added cost at every later request (start.carried: the same for what every request starts from). reading + writing is in_the_ledger"
                    .to_owned(),
            ),
        ),
    ]))
}

/// `trace RUN`: as tables for a person.
pub fn trace_text(t: &Json) -> String {
    let n = |j: &Json, k: &str| fmt(num(j, k));
    let start = t.get("start").cloned().unwrap_or(Json::Null);
    let mut out = format!(
        "\n{} ({}{}): {} requests; weighted {} (in the ledger {} = reading {} + writing {}); written out {} tokens, of which {} cannot be seen in the stream; the context: {} at the start, {} at the end; what every request starts from cost {} to carry\n",
        text(t, "run").unwrap_or(""),
        text(t, "who").unwrap_or(""),
        if t.get("complete").and_then(Json::as_bool) == Some(false) { ", stopped at its time limit: what it wrote is only what could be read" } else { "" },
        n(t, "requests"),
        n(t, "weighted"),
        n(t, "in_the_ledger"),
        n(t, "reading"),
        n(t, "writing"),
        n(t, "written_out"),
        n(t, "of_which_unseen"),
        n(&start, "context"),
        n(t, "context_at_end"),
        n(&start, "carried")
    );
    out.push_str(&format!("\n  {:40} {:>8} {:>5} {:>9} {:>11} {:>9} {:>9} {:>9}\n", "a request's first step", "requests", "steps", "answered", "written out", "writing", "carried", "together"));
    for (kind, b) in t.get("by_kind").and_then(Json::as_object).unwrap_or(&[]) {
        let together = num(b, "writing").unwrap_or(0.0) + num(b, "carried").unwrap_or(0.0);
        out.push_str(&format!("  {kind:40} {:>8} {:>5} {:>9} {:>11} {:>9} {:>9} {:>9}\n", n(b, "requests"), n(b, "steps"), n(b, "answered"), n(b, "written_out"), n(b, "writing"), n(b, "carried"), fmt(Some(together))));
    }
    out.push_str(&format!("\n  {:>3} {:>8} {:>7} {:>8} {:>7} {:>7} {:>8} {:>9} {:>8}  step (tokens written > answered)\n", "#", "context", "new", "written", "unseen", "cost", "so far", "carried", ""));
    for r in t.get("rows").and_then(Json::as_array).unwrap_or(&[]) {
        let steps = r.get("steps").and_then(Json::as_array).unwrap_or(&[]);
        let said = |s: &Json| format!("{}: {} ({} > {})", text(s, "tool").unwrap_or(""), text(s, "what").unwrap_or(""), n(s, "written"), n(s, "answered"));
        let first = steps.first().map_or_else(|| "(words only)".to_owned(), said);
        out.push_str(&format!("  {:>3} {:>8} {:>7} {:>8} {:>7} {:>7} {:>8} {:>9} {:>8}  {first}\n", n(r, "n"), n(r, "context"), n(r, "new"), n(r, "written_out"), n(r, "of_which_unseen"), n(r, "cost"), n(r, "cost_so_far"), n(r, "carried"), ""));
        for s in steps.iter().skip(1) {
            out.push_str(&format!("  {:>3} {:>8} {:>7} {:>8} {:>7} {:>7} {:>8} {:>9} {:>8}  {}\n", "", "", "", "", "", "", "", "", "", said(s)));
        }
    }
    let files = t.get("files").and_then(Json::as_array).unwrap_or(&[]);
    if !files.is_empty() {
        out.push_str(&format!("\n  {:>5} {:>6} {:>9} {:>8}  file (by the file tools; a shell command's files are in its row)\n", "reads", "writes", "answered", "written"));
        for f in files {
            out.push_str(&format!("  {:>5} {:>6} {:>9} {:>8}  {}\n", n(f, "reads"), n(f, "writes"), n(f, "answered"), n(f, "written"), text(f, "file").unwrap_or("")));
        }
    }
    out
}

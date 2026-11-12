//! The `trace_*` tools (docs/design/TRACE.md): the engine's structured log, the causal tree of
//! one call, crash reports and incidents, for agents. Answered on the caller's connection thread,
//! never the engine thread, so a stalled engine can still be traced. Bounded: pages, limits and
//! byte caps; concise by default.

use crate::incidents::{self, Action, Status};
use crate::response::{Format, KEY as FORMAT_KEY};
use crate::tools::{argument_error, check_keys};
use crate::trace_sink;
use specula_core::trace::{self, Event, EventKind, Filter, Level};
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};

/// Answers stop adding events past this many bytes (compact JSON).
const MAX_BYTES: usize = 32 * 1024;
const QUERY_LIMIT: (usize, usize) = (10, 200);
const FOLLOW_LIMIT: (usize, usize) = (50, 200);
const INCIDENT_LIMIT: (usize, usize) = (20, 100);
/// Nodes a call's tree lists.
const TREE_NODES: usize = 200;
/// Who is asking, for what they may change. (What they may see is the same for everyone: call
/// digests hold no text, only keys, types, sizes and numbers: public metadata.)
struct Viewer {
    /// What the session may do: the engine's one answer, read on this (the caller's) thread
    /// (`authority::of_session`; roles are granted, never self-declared, docs/design/AUTHORITY.md).
    authority: crate::authority::Authority,
    agent: String,
}

impl Viewer {
    fn of(session: &SessionInfo) -> Viewer {
        let named = trace_sink::name_of(&session.id);
        Viewer { authority: crate::authority::of_session(&session.id), agent: named.map_or_else(|| crate::swarm::default_name(session), |(a, _)| a) }
    }

    /// Engine-wide trace settings are the lead's.
    fn lead(&self) -> bool {
        self.authority >= crate::authority::Authority::Lead
    }

    /// Any incident may be resolved or reopened by a reviewer (the debugger job reviews
    /// incidents: grant it with --reviewer NAME) or the lead.
    fn resolver(&self) -> bool {
        self.authority >= crate::authority::Authority::Reviewer
    }
}

/// Names in trace settings: at most 64 characters of `[a-z0-9_.:*-]`.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b':' | b'*' | b'-'))
}

fn error(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

fn bad(message: String) -> ToolOutput {
    ToolOutput::error(argument_error(&message).to_string())
}

fn limit(args: &Json, (default, max): (usize, usize)) -> Result<usize, String> {
    match args.get("limit") {
        None | Some(Json::Null) => Ok(default),
        Some(v) => match v.as_u64() {
            Some(n) if n >= 1 && n as usize <= max => Ok(n as usize),
            _ => Err(format!("'limit' must be an integer 1..{max}")),
        },
    }
}

fn level_arg(args: &Json, key: &str) -> Result<Option<Option<Level>>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) => trace::parse_setting(s).map(Some).ok_or_else(|| format!("'{key}' must be one of error, warn, info, debug, trace, off")),
        Some(_) => Err(format!("'{key}' must be a string level")),
    }
}

fn strings(args: &Json, key: &str) -> Result<Vec<String>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Str(s)) => Ok(vec![s.clone()]),
        Some(Json::Arr(a)) if a.len() <= 16 => a.iter().map(|v| v.as_str().map(str::to_owned).ok_or_else(|| format!("'{key}' must be strings"))).collect(),
        Some(_) => Err(format!("'{key}' must be a string or a list of at most 16 strings")),
    }
}

fn range(args: &Json, key: &str) -> Result<Option<(u64, u64)>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Arr(a)) if a.len() == 2 => match (a[0].as_u64(), a[1].as_u64()) {
            (Some(x), Some(y)) if x <= y => Ok(Some((x, y))),
            _ => Err(format!("'{key}' must be [from, to] with whole numbers, from <= to")),
        },
        Some(v) => match v.as_u64() {
            Some(x) => Ok(Some((x, x))),
            None => Err(format!("'{key}' must be a whole number or [from, to]")),
        },
    }
}

fn u64_arg(args: &Json, key: &str) -> Result<Option<u64>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or_else(|| format!("'{key}' must be a whole number")),
    }
}

/// The filter of trace_query and trace_follow, starting from the session's own view.
fn filter(args: &Json, session: &SessionInfo) -> Result<Filter, String> {
    let view = trace_sink::view(&session.id);
    let mut f = Filter { level: view.level, subsystems: view.subsystems, ..Filter::default() };
    if let Some(l) = level_arg(args, "level")? {
        f.level = Some(l.ok_or("'level' can't be off in a query")?);
    }
    let subs = strings(args, "subsystem")?;
    if let Some(bad) = subs.iter().find(|s| !valid_name(s)) {
        return Err(format!("'subsystem' {bad:?}: names are at most 64 characters of a-z 0-9 _ . : * -"));
    }
    if !subs.is_empty() {
        f.subsystems = subs;
    }
    f.codes = strings(args, "code")?;
    if let Some(agent) = args.get("agent") {
        let a = agent.as_str().filter(|a| a.len() <= 64).ok_or("'agent' must be a name of at most 64 characters")?;
        f.calls = Some(trace_sink::calls_of(a, trace_sink::MAX_CALLS));
    }
    if let Some(c) = u64_arg(args, "call")? {
        f.calls = Some(match f.calls {
            Some(of_agent) => of_agent.into_iter().filter(|x| *x == c).collect(),
            None => [c].into_iter().collect(),
        });
    }
    f.tick = range(args, "tick")?;
    f.frame = range(args, "frame")?;
    f.time_us = range(args, "t_ms")?.map(|(a, b)| (a * 1000, b * 1000 + 999));
    f.text = match args.get("text") {
        None | Some(Json::Null) => None,
        Some(Json::Str(s)) if !s.is_empty() && s.len() <= 128 => Some(s.clone()),
        Some(_) => return Err("'text' must be a string of 1..128 characters".into()),
    };
    Ok(f)
}

fn incident_json(i: &incidents::Incident, detailed: bool) -> Json {
    if detailed {
        i.detail_json()
    } else {
        i.summary_json()
    }
}

/// Adds events until `MAX_BYTES`: the list and whether some were left out.
fn capped<'a>(events: impl Iterator<Item = &'a Event>, budget: usize) -> (Vec<Json>, bool) {
    let mut out = Vec::new();
    let mut bytes = 0;
    for e in events {
        let j = e.to_json();
        bytes += j.to_string().len() + 1;
        if bytes > budget && !out.is_empty() {
            return (out, true);
        }
        out.push(j);
    }
    (out, false)
}

fn ring_json(first: u64, next: u64, overwritten: u64) -> Json {
    Json::obj([("first_id", Json::from(first)), ("next_id", Json::from(next)), ("overwritten", Json::from(overwritten))])
}

pub(crate) fn call(session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    let format = match Format::arg(args, Format::Concise) {
        Ok(f) => f,
        Err(e) => return bad(e),
    };
    let detailed = format == Format::Detailed;
    let out = match name {
        "trace_query" => query(session, args, detailed),
        "trace_follow" => follow(session, args),
        "trace_call" => call_tree(session, args, detailed),
        "trace_settings" => settings(session, args),
        "trace_crash" => crash(args, detailed),
        "trace_incidents" => list_incidents(args, detailed),
        "trace_incident" => incident(session, args),
        other => Err(format!("no trace tool '{other}'")),
    };
    out.unwrap_or_else(bad)
}

fn query(session: &SessionInfo, args: &Json, detailed: bool) -> Result<ToolOutput, String> {
    check_keys(args, "trace_query", &[&["level", "subsystem", "code", "agent", "call", "tick", "frame", "t_ms", "text", "limit", "before", FORMAT_KEY]])?;
    let f = filter(args, session)?;
    let lim = limit(args, if detailed { (50, QUERY_LIMIT.1) } else { QUERY_LIMIT })?;
    let before = u64_arg(args, "before")?;
    let (matched, page, more, by_code, ring) = trace::read(|r| {
        let mut by_code: std::collections::BTreeMap<&'static str, u64> = std::collections::BTreeMap::new();
        let mut page: Vec<Event> = Vec::new();
        let mut matched = 0usize;
        for e in r.iter().rev().filter(|e| f.matches(e)) {
            matched += 1;
            *by_code.entry(e.code).or_default() += 1;
            if before.is_none_or(|b| e.id < b) && page.len() <= lim {
                page.push(e.clone());
            }
        }
        let more = page.len() > lim;
        page.truncate(lim);
        (matched, page, more, by_code, ring_json(r.first_id(), r.next_id(), r.overwritten))
    });
    let (events, cut) = capped(page.iter(), MAX_BYTES);
    let next_before = if more || cut { events.last().and_then(|e| e.get("id")).cloned().unwrap_or(Json::Null) } else { Json::Null };
    let mut codes: Vec<(&str, u64)> = by_code.into_iter().collect();
    codes.sort_by_key(|c| std::cmp::Reverse(c.1));
    Ok(ToolOutput::text(
        Json::obj([
            ("matched", Json::from(matched)),
            ("returned", Json::from(events.len())),
            ("by_code", Json::obj(codes.into_iter().take(12).map(|(c, n)| (c, Json::from(n))))),
            ("events", Json::Arr(events)),
            ("next_before", next_before),
            ("ring", ring),
        ])
        .to_string(),
    ))
}

fn follow(session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "trace_follow", &[&["cursor", "level", "subsystem", "code", "agent", "limit", FORMAT_KEY]])?;
    let f = filter(args, session)?;
    let lim = limit(args, FOLLOW_LIMIT)?;
    let from = match u64_arg(args, "cursor")? {
        Some(c) => Some(c),
        None => trace_sink::cursor(&session.id),
    };
    let (page, last_seen, missed, ring) = trace::read(|r| {
        // Without a cursor: from the oldest kept event.
        let start = from.unwrap_or(0);
        let missed = r.first_id().saturating_sub(start + 1);
        let mut page: Vec<Event> = Vec::new();
        let mut last_seen = start.max(r.first_id().saturating_sub(1));
        for e in r.after(start) {
            if page.len() == lim {
                break;
            }
            last_seen = e.id;
            if f.matches(e) {
                page.push(e.clone());
            }
        }
        (page, last_seen, missed, ring_json(r.first_id(), r.next_id(), r.overwritten))
    });
    let (events, cut) = capped(page.iter(), MAX_BYTES);
    // A cut answer resumes after the last event it shows.
    let cursor = if cut { events.last().and_then(|e| e.get("id")).and_then(Json::as_u64).unwrap_or(last_seen) } else { last_seen };
    trace_sink::set_cursor(&session.id, cursor);
    let more = trace::read(|r| r.next_id().saturating_sub(1) > cursor);
    Ok(ToolOutput::text(
        Json::obj([
            ("events", Json::Arr(events)),
            ("cursor", Json::from(cursor)),
            ("more", Json::from(more)),
            ("missed", Json::from(missed)),
            ("ring", ring),
        ])
        .to_string(),
    ))
}

/// One node of a call's tree: a span with its children, or an event.
fn tree(events: &[Event], detailed: bool) -> (Json, usize) {
    use std::collections::BTreeMap;
    let spans: BTreeMap<u64, &Event> = events.iter().filter(|e| matches!(e.kind, EventKind::Span { .. })).map(|e| (e.span, e)).collect();
    // Children by parent span (0: the root); span close events hang under their parent, other
    // events under the span they happened in.
    let mut children: BTreeMap<u64, Vec<&Event>> = BTreeMap::new();
    for e in events {
        let parent = match e.kind {
            EventKind::Span { .. } => e.parent,
            EventKind::Event => e.span,
        };
        let parent = if parent != 0 && !spans.contains_key(&parent) { 0 } else { parent };
        children.entry(parent).or_default().push(e);
    }
    let mut nodes = 0usize;
    fn build(id: u64, children: &BTreeMap<u64, Vec<&Event>>, nodes: &mut usize, depth: usize, detailed: bool) -> Vec<Json> {
        let mut out = Vec::new();
        for e in children.get(&id).map(Vec::as_slice).unwrap_or_default() {
            if *nodes >= TREE_NODES {
                break;
            }
            *nodes += 1;
            let mut j = Json::obj([("id", Json::from(e.id)), ("level", Json::from(e.level.as_str())), ("sub", Json::from(e.subsystem)), ("code", Json::from(e.code))]);
            if let Some(d) = e.dur_us() {
                j.set("dur_ms", Json::from((d / 10) as f64 / 100.0));
            }
            if (detailed || e.level <= Level::Info) && !e.fields.is_empty() {
                j.set("fields", Json::Obj(e.fields.iter().map(|(k, v)| ((*k).to_owned(), v.to_json())).collect()));
            }
            if e.tick != 0 {
                j.set("tick", Json::from(e.tick));
            }
            if let EventKind::Span { .. } = e.kind {
                if depth < 16 {
                    let kids = build(e.span, children, nodes, depth + 1, detailed);
                    if !kids.is_empty() {
                        j.set("children", Json::Arr(kids));
                    }
                }
            }
            out.push(j);
        }
        out
    }
    let root = build(0, &children, &mut nodes, 0, detailed);
    (Json::Arr(root), nodes)
}

fn call_tree(session: &SessionInfo, args: &Json, detailed: bool) -> Result<ToolOutput, String> {
    check_keys(args, "trace_call", &[&["call", FORMAT_KEY]])?;
    let me = trace::Ctx::current().call;
    let id = match u64_arg(args, "call")? {
        Some(c) => c,
        None => trace_sink::previous_call(&session.id, me).ok_or("no earlier call of this session is kept: pass 'call' (a call id from trace_query or an incident)")?,
    };
    let record = trace_sink::call(id);
    let events: Vec<Event> = trace::read(|r| r.iter().filter(|e| e.call == id).cloned().collect());
    if record.is_none() && events.is_empty() {
        return Ok(error("no_call", format!("call {id} is not kept (the last {} calls are) and left no events in the ring", trace_sink::MAX_CALLS)));
    }
    let (root, nodes) = tree(&events, detailed);
    let mut by_code: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for e in &events {
        *by_code.entry(e.code).or_default() += 1;
    }
    let mut out = Json::obj([
        ("call", record.as_ref().map_or(Json::obj([("call", Json::from(id))]), trace_sink::CallRecord::to_json)),
        ("events", Json::from(events.len())),
        ("by_code", Json::obj(by_code.into_iter().map(|(c, n)| (c, Json::from(n))))),
        ("journal", Json::arr(trace_sink::journal_of(id).into_iter().map(Json::from))),
        ("incidents", Json::arr(incidents_of_call(id))),
        ("tree", root),
        ("tree_nodes", Json::from(nodes)),
        ("tree_cut", Json::from(nodes < events.len())),
    ]);
    if out.to_string().len() > MAX_BYTES {
        out.set("tree", Json::from("too big for one answer: page it with trace_query {call}"));
    }
    Ok(ToolOutput::text(out.to_string()))
}

fn incidents_of_call(call: u64) -> Vec<Json> {
    let (all, _, _) = incidents::list(None, false, None, 0, incidents::MAX_INCIDENTS);
    all.iter().filter(|i| i.calls.contains(&call)).take(8).map(|i| Json::obj([("id", Json::from(i.id)), ("code", Json::from(i.code)), ("status", Json::from(i.status.as_str()))])).collect()
}

fn settings_json() -> Json {
    let s = trace_sink::settings();
    let subs = trace::subsystems();
    let shown = subs.len().min(64);
    Json::obj([
        ("default", Json::from(trace::setting_str(trace::default_level()))),
        (
            "subsystems",
            Json::obj(subs.iter().take(shown).map(|(n, l, own)| (*n, Json::from(if *own { trace::setting_str(*l).to_owned() } else { format!("{} (default)", trace::setting_str(*l)) })))),
        ),
        ("subsystems_omitted", Json::from(subs.len() - shown)),
        ("file", Json::from(trace::setting_str(s.file))),
        ("stderr", Json::from(trace::setting_str(s.stderr))),
        ("budgets_ms", s.budgets.to_json()),
        ("dir", Json::from(trace_sink::dir().map(|d| d.display().to_string()))),
    ])
}

fn settings(session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "trace_settings", &[&["scope", "level", "subsystems", "file", "stderr", "budgets_ms", FORMAT_KEY]])?;
    let scope = args.get("scope").and_then(Json::as_str).unwrap_or("global");
    let subs: Vec<(String, Option<Option<Level>>)> = match args.get("subsystems") {
        None | Some(Json::Null) => Vec::new(),
        Some(Json::Obj(pairs)) if pairs.len() <= 64 => pairs
            .iter()
            .map(|(k, v)| match v.as_str() {
                _ if !valid_name(k) => Err(format!("'subsystems' key {:?}: names are at most 64 characters of a-z 0-9 _ . : * -", k.chars().take(80).collect::<String>())),
                Some("default") => Ok((k.clone(), None)),
                Some(s) => trace::parse_setting(s).map(|l| (k.clone(), Some(l))).ok_or_else(|| format!("'subsystems.{k}' must be a level, off or default")),
                None => Err(format!("'subsystems.{k}' must be a string")),
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err("'subsystems' must be an object {name: level | \"off\" | \"default\"} of at most 64".into()),
    };
    let level = level_arg(args, "level")?;
    match scope {
        "session" => {
            if ["file", "stderr", "budgets_ms"].iter().any(|k| args.get(k).is_some()) {
                return Err("'file', 'stderr' and 'budgets_ms' are engine-wide: use scope \"global\" (lead or director only)".into());
            }
            let mut v = trace_sink::view(&session.id);
            if let Some(l) = level {
                v.level = l;
            }
            if !subs.is_empty() {
                v.subsystems = subs.into_iter().map(|(k, _)| k).collect();
            }
            trace_sink::set_view(&session.id, v.clone());
            return Ok(ToolOutput::text(
                Json::obj([("scope", Json::from("session")), ("level", Json::from(v.level.map(Level::as_str))), ("subsystems", Json::arr(v.subsystems.iter().map(|s| Json::from(s.as_str()))))]).to_string(),
            ));
        }
        "global" => {}
        _ => return Err("'scope' must be \"global\" or \"session\"".into()),
    }
    let changing = level.is_some() || !subs.is_empty() || ["file", "stderr", "budgets_ms"].iter().any(|k| args.get(k).is_some());
    if changing {
        if !Viewer::of(session).lead() {
            return Ok(error(
                "not_lead",
                "engine-wide trace settings belong to the lead (granted by the operator: --lead NAME, docs/design/AUTHORITY.md), or use scope \"session\" to change only what your own queries show",
            ));
        }
        let mut s = trace_sink::settings();
        if let Some(Some(l)) = level_arg(args, "file")? {
            s.file = Some(l);
        } else if args.get("file").and_then(Json::as_str) == Some("off") {
            s.file = None;
        }
        if let Some(l) = level_arg(args, "stderr")? {
            s.stderr = l;
        }
        if let Some(b) = args.get("budgets_ms") {
            let Json::Obj(pairs) = b else { return Err("'budgets_ms' must be an object {class: ms}".into()) };
            for (k, v) in pairs {
                let ms = v.as_f64().filter(|m| *m >= 0.1 && *m <= 60_000.0).ok_or_else(|| format!("'budgets_ms.{k}' must be a number 0.1..60000"))?;
                *s.budgets.get_mut(k).ok_or_else(|| format!("'budgets_ms.{k}': classes are {}", trace_sink::Budgets::NAMES.join(", ")))? = ms;
            }
        }
        if let Some(l) = level {
            trace::set_default(l);
        }
        for (name, l) in subs {
            match l {
                Some(l) => trace::set_level(&name, l),
                None => trace::reset_level(&name),
            }
        }
        trace_sink::set_settings(s);
        let who = trace_sink::name_of(&session.id).map(|(a, _)| a).unwrap_or_default();
        specula_core::trace_event!(&trace_sink::TRACE, Info, "trace.settings_changed", by = who);
    }
    Ok(ToolOutput::text(settings_json().to_string()))
}

/// Which engine wrote a report, against this one: engines started from one folder share it.
fn engine_label(report: &Json) -> Json {
    let theirs = report.get("engine").cloned().unwrap_or(Json::Null);
    let me = trace_sink::identity();
    let same_run = me.is_some_and(|m| theirs.get("run").and_then(Json::as_str) == Some(m.run.as_str()));
    let same_port = me.is_some_and(|m| theirs.get("port").and_then(Json::as_u64) == Some(u64::from(m.port)));
    let mut j = theirs;
    if let Json::Obj(_) = j {
        j.set("this_run", Json::from(same_run));
        j.set("this_port", Json::from(same_port));
    }
    j
}

fn crash(args: &Json, detailed: bool) -> Result<ToolOutput, String> {
    check_keys(args, "trace_crash", &[&["index", "list", "events", "caught", "port", FORMAT_KEY]])?;
    let Some(dir) = trace_sink::dir() else { return Ok(error("no_trace_dir", "this engine runs without a trace folder")) };
    let caught = args.get("caught").and_then(Json::as_bool) == Some(true);
    let prefix = if caught { trace_sink::CAUGHT_PREFIX } else { trace_sink::CRASH_PREFIX };
    let port = u64_arg(args, "port")?;
    let read = |p: &std::path::PathBuf| std::fs::read_to_string(p).ok().and_then(|t| Json::parse(&t).ok());
    // Other engines' reports (by port) are left out when a port is named.
    let files: Vec<(std::path::PathBuf, Json)> = trace_sink::crash_files(&dir, prefix)
        .into_iter()
        .filter_map(|p| read(&p).map(|r| (p, r)))
        .filter(|(_, r)| port.is_none_or(|want| r.get("engine").and_then(|e| e.get("port")).and_then(Json::as_u64) == Some(want)))
        .collect();
    let name_of = |p: &std::path::Path| Json::from(p.file_name().map(|n| n.to_string_lossy().into_owned()));
    if args.get("list").and_then(Json::as_bool) == Some(true) {
        let list = files.iter().map(|(p, head)| {
            let field = |k: &str| head.get(k).cloned().unwrap_or(Json::Null);
            Json::obj([("file", name_of(p)), ("reason", field("reason")), ("message", field("message")), ("unix_ms", field("unix_ms")), ("engine", engine_label(head))])
        });
        return Ok(ToolOutput::text(Json::obj([("reports", Json::arr(list)), ("this_engine", trace_sink::identity().map_or(Json::Null, trace_sink::Identity::to_json))]).to_string()));
    }
    let index = u64_arg(args, "index")?.unwrap_or(0) as usize;
    let Some((path, report)) = files.get(index) else {
        let what = if caught { "caught-panic report" } else { "crash report" };
        let kept = if caught { trace_sink::CAUGHT_KEPT } else { trace_sink::CRASH_KEPT };
        return Ok(error("no_crash_report", format!("no {what}{} in the trace folder (the newest {kept} are kept; list: true shows them)", if index > 0 { format!(" {index}") } else { String::new() })));
    };
    let mut report = report.clone();
    report.set("engine", engine_label(&report));
    let keep = match u64_arg(args, "events")? {
        Some(n) => n.clamp(1, trace_sink::CRASH_EVENTS as u64) as usize,
        None if detailed => trace_sink::CRASH_EVENTS,
        None => 20,
    };
    if let Some(Json::Arr(events)) = report.get("events").cloned() {
        // The newest events (closest to the crash).
        let skip = events.len().saturating_sub(keep);
        let kept: Vec<Json> = events.into_iter().skip(skip).collect();
        // The newest that fit MAX_BYTES (each event's size measured once).
        let mut room = MAX_BYTES;
        let fit = kept
            .iter()
            .rev()
            .take_while(|e| {
                let n = e.to_string().len() + 1;
                let ok = n <= room;
                room = room.saturating_sub(n);
                ok
            })
            .count()
            .max(1);
        let kept: Vec<Json> = kept[kept.len() - fit.min(kept.len())..].to_vec();
        report.set("events_shown", Json::from(kept.len()));
        report.set("events", Json::Arr(kept));
    }
    report.set("file", name_of(path));
    report.set("reports_kept", Json::from(files.len()));
    Ok(ToolOutput::text(report.to_string()))
}

fn list_incidents(args: &Json, detailed: bool) -> Result<ToolOutput, String> {
    check_keys(args, "trace_incidents", &[&["status", "code", "limit", "offset", FORMAT_KEY]])?;
    let (status, active) = match args.get("status").and_then(Json::as_str) {
        None | Some("active") => (None, true),
        Some("all") => (None, false),
        Some(s) => (Some(Status::parse(s).ok_or("'status' must be active (default: open or recurring), all, open, acknowledged, resolved or recurring")?), false),
    };
    let code = args.get("code").and_then(Json::as_str);
    let lim = limit(args, INCIDENT_LIMIT)?;
    let offset = u64_arg(args, "offset")?.unwrap_or(0) as usize;
    let (page, total, totals) = incidents::list(status, active, code, offset, lim);
    let items = Json::arr(page.iter().map(|i| incident_json(i, detailed)));
    let mut out = Json::obj([("matched", Json::from(total)), ("totals", totals), ("incidents", items)]);
    if offset + page.len() < total {
        out.set("next_offset", Json::from(offset + page.len()));
    }
    if out.to_string().len() > MAX_BYTES {
        out.set("incidents", Json::arr(page.iter().map(|i| i.summary_json())));
    }
    Ok(ToolOutput::text(out.to_string()))
}

fn incident(session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "trace_incident", &[&["id", "action", "note", "critique", "task", FORMAT_KEY]])?;
    let id = u64_arg(args, "id")?.ok_or("'id' is required (trace_incidents lists them)")?;
    let note = match args.get("note") {
        None | Some(Json::Null) => None,
        Some(Json::Str(s)) => Some(s.as_str()),
        Some(_) => return Err("'note' must be a string".into()),
    };
    let action = match args.get("action").and_then(Json::as_str) {
        None => None,
        Some("ack") => Some(Action::Ack),
        Some("resolve") => Some(Action::Resolve),
        Some("reopen") => Some(Action::Reopen),
        Some("note") => Some(Action::Note),
        Some("link") => {
            let (kind, link) = match (u64_arg(args, "critique")?, u64_arg(args, "task")?) {
                (Some(c), None) => ("critique", c),
                (None, Some(t)) => ("task", t),
                _ => return Err("'link' needs one of 'critique' or 'task' (the id critique_add or task_post answered)".into()),
            };
            Some(Action::Link { kind: kind.into(), id: link })
        }
        Some(_) => return Err("'action' must be ack, resolve, reopen, link or note".into()),
    };
    let viewer = Viewer::of(session);
    let by = viewer.agent.clone();
    if let Some(Action::Resolve | Action::Reopen) = &action {
        // Anyone may ack, note or link (an ack confers nothing). Closing or reopening is for an
        // agent the incident names (whose calls raised it: the fix owner), a reviewer (the
        // debugger role reviews incidents) or the lead; an incident naming nobody, only for a
        // reviewer or the lead.
        let Some(inc) = incidents::get(id) else { return Ok(error("no_incident", format!("no incident {id} (trace_incidents lists them)"))) };
        // A security incident is the input monitor's: security_clear ends it (the owner or the
        // lead), never its offender resolving it here (docs/design/SECURITY.md).
        if inc.code.starts_with("security.") {
            return Ok(error("use_security_clear", format!("incident {id} is a security incident: the owner or the lead clears it with security_clear (resolving it here wouldn't unfreeze anything)")));
        }
        let allowed = viewer.resolver() || inc.agents.iter().any(|a| *a == by);
        if !allowed {
            let named = if inc.agents.is_empty() { "it names no agent".to_owned() } else { format!("it names {}", inc.agents.iter().cloned().collect::<Vec<_>>().join(", ")) };
            return Ok(error("not_owner", format!("incident {id} is resolved or reopened by an agent it names, a reviewer or the lead ({named}); ack, note or link it instead")));
        }
    }
    let inc = match action {
        Some(a) => match incidents::act(id, &by, a, note) {
            Ok(i) => i,
            Err((code, message)) => return Ok(error(code, message)),
        },
        None => match incidents::get(id) {
            Some(i) => i,
            None => return Ok(error("no_incident", format!("no incident {id} (trace_incidents lists them)"))),
        },
    };
    Ok(ToolOutput::text(incident_json(&inc, true).to_string()))
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let any = |d: &str| Json::obj([("description", Json::from(d))]);
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))])
    };
    let def = |name: &str, description: &str, input_schema: Json| ToolDef { name: name.into(), description: description.into(), input_schema };
    let format = || (FORMAT_KEY, Format::schema());
    let filters = || {
        vec![
            ("level", t("string", "At most this verbose: error, warn, info, debug or trace")),
            ("subsystem", any("A subsystem or list (engine, mcp, write_policy, sight, gpu, jobs, logic, world, module:<name>; a trailing * matches a prefix)")),
            ("code", any("A code prefix or list (engine.stall, write.refused, frame.)")),
            ("agent", t("string", "Only events of this agent's calls")),
            ("call", t("integer", "Only events of this call")),
        ]
    };
    vec![
        def(
            "trace_query",
            "Search the engine's structured log: events with level, subsystem, machine code, typed fields, and the call, \
             span, tick and frame they belong to. Filters: level, subsystem, code, agent, call, tick/frame/t_ms (one value or \
             [from, to]; t_ms is milliseconds since the engine started tracing), text (in codes and field text). Newest first, \
             10 by default (limit up to 200), with counts by code; page older with before = next_before. Answered off the \
             engine thread, so it works while the engine is stalled.",
            schema(
                filters()
                    .into_iter()
                    .chain([
                        ("tick", any("Tick or [from, to]")),
                        ("frame", any("Frame or [from, to]")),
                        ("t_ms", any("Trace time (ms) or [from, to]")),
                        ("text", t("string", "Substring of the code, subsystem or a field")),
                        ("limit", t("integer", "Events to return (default 10, detailed 50, max 200)")),
                        ("before", t("integer", "Only events older than this id (next_before of the last page)")),
                        format(),
                    ])
                    .collect(),
                &[],
            ),
        ),
        def(
            "trace_follow",
            "New log events since your cursor, oldest first, like world_changes (the engine remembers your session's cursor; \
             without one you start at the oldest kept event). Same filters as trace_query; missed says how many events were \
             overwritten before you read them. Poll it to watch the engine.",
            schema(filters().into_iter().chain([("cursor", t("integer", "Start after this event id")), ("limit", t("integer", "At most this many events (default 50, max 200)")), format()]).collect(), &[]),
        ),
        def(
            "trace_call",
            "Everything one call caused: its record (tool, agent, args digest, ms, error), the journal entries it made, its \
             incidents, and its causal tree: spans (engine-thread work, model builds, renders, previews, save writers) with \
             their events, write-policy decisions and GPU messages. Without call: your session's previous call.",
            schema(vec![("call", t("integer", "A call id (trace_query and incidents show them)")), format()], &[]),
        ),
        def(
            "trace_settings",
            "Trace levels and sinks. No arguments: the current settings and every subsystem's level. scope \"global\" (lead or \
             director only): level (the default), subsystems {name: level | off | default}, file (the JSONL sink's level or \
             off), stderr, budgets_ms {read, edit, render, disk, big, sim, game, frame} (engine.stall thresholds per call \
             class). scope \"session\": level and subsystems become the default filter of your own queries.",
            schema(
                vec![
                    ("scope", t("string", "\"global\" (default) or \"session\"")),
                    ("level", t("string", "error, warn, info, debug, trace or off")),
                    ("subsystems", t("object", "{subsystem: level | \"off\" | \"default\"}")),
                    ("file", t("string", "The file sink's level, or off")),
                    ("stderr", t("string", "stderr's level, or off")),
                    ("budgets_ms", t("object", "Stall budgets per call class, ms")),
                    format(),
                ],
                &[],
            ),
        ),
        def(
            "trace_crash",
            "The newest crash report (a panic, a module panic, a fatal engine error or device loss): reason, message, \
             location, the call it was in, which engine wrote it (pid, port, run, this_run), and the last events before it (20 by default, up to 256). Readable after a restart. \
             list: true lists the kept reports; index picks an older one.",
            schema(
                vec![
                    ("index", t("integer", "0 = newest")),
                    ("list", t("boolean", "List the reports instead")),
                    ("events", t("integer", "Events to show (1..256)")),
                    ("caught", t("boolean", "Panics that were caught (a structured error; the engine ran on) instead of crashes")),
                    ("port", t("integer", "Only reports of the engine on this port (engines started from one folder share it)")),
                    format(),
                ],
                &[],
            ),
        ),
        def(
            "trace_incidents",
            "Problems the watchdogs found, deduplicated: engine.stall, frame.over_budget, write.refused, swarm.refusal_spike, \
             sight.packet_inconsistent, world.replay_mismatch, gpu.validation, jobs.stuck, memory.over_budget, \
             module.disabled. Each has a count, first/last seen and a status. status: active (default: open or recurring), \
             all, open, acknowledged, resolved, recurring; code prefix; paged with offset.",
            schema(vec![("status", t("string", "active (default), all, open, acknowledged, resolved or recurring")), ("code", t("string", "Code prefix")), ("limit", t("integer", "Default 20, max 100")), ("offset", t("integer", "Skip this many")), format()], &[]),
        ),
        def(
            "trace_incident",
            "One incident in full: its calls, agents, frames, ticks, events, evidence (numbers, capture and packet paths), \
             notes, links and who acked it. action: ack (you're on it), resolve (fixed; turns recurring if seen again; for an agent it names, a reviewer, the debugger role or the lead), reopen, note, or \
             link with critique or task (the id critique_add or task_post gave), each with an optional note.",
            schema(
                vec![
                    ("id", t("integer", "The incident")),
                    ("action", t("string", "ack, resolve, reopen, link or note")),
                    ("note", t("string", "What you found or did")),
                    ("critique", t("integer", "With link: a critique id")),
                    ("task", t("integer", "With link: a task id")),
                    format(),
                ],
                &["id"],
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str) -> SessionInfo {
        SessionInfo { id: id.into(), client_name: "t".into(), client_version: "0".into(), protocol_version: "x".into() }
    }

    fn answer(out: ToolOutput) -> Json {
        let specula_protocol::Content::Text(t) = &out.content[0] else { panic!("text") };
        Json::parse(t).unwrap()
    }

    static SUB: trace::Subsystem = trace::Subsystem::new("test.tools");

    #[test]
    fn query_follow_and_call_find_one_calls_work() {
        let s = session("trace-tools-1");
        let id = trace_sink::begin_call(&s, "world_spawn", &Json::Null);
        {
            let _c = trace::Ctx { call: id, span: 0 }.enter();
            let _sp = trace::span(&SUB, Level::Info, "test.engine_work");
            specula_core::trace_event!(&SUB, Info, "test.inside", n = 1u64);
        }
        trace_sink::end_call(id, &ToolOutput::text("{}".to_owned()));
        let q = answer(call(&s, "trace_query", &Json::parse(&format!(r#"{{"call":{id}}}"#)).unwrap()));
        assert_eq!(q.get("matched").and_then(Json::as_u64), Some(2), "{q}");
        let t = answer(call(&s, "trace_call", &Json::parse(&format!(r#"{{"call":{id}}}"#)).unwrap()));
        let tree = t.get("tree").and_then(Json::as_array).unwrap();
        assert_eq!(tree.len(), 1, "{t}");
        assert_eq!(tree[0].get("code").and_then(Json::as_str), Some("test.engine_work"));
        let kids = tree[0].get("children").and_then(Json::as_array).unwrap();
        assert_eq!(kids[0].get("code").and_then(Json::as_str), Some("test.inside"));
        assert_eq!(t.get("call").and_then(|c| c.get("tool")).and_then(Json::as_str), Some("world_spawn"));
        // Follow: a first read, then nothing new, then the next event only.
        let f1 = answer(call(&s, "trace_follow", &Json::parse(r#"{"subsystem":"test.tools","limit":200}"#).unwrap()));
        let c1 = f1.get("cursor").and_then(Json::as_u64).unwrap();
        specula_core::trace_event!(&SUB, Warn, "test.later");
        let mut seen = Vec::new();
        for _ in 0..200 {
            let f = answer(call(&s, "trace_follow", &Json::parse(r#"{"subsystem":"test.tools","limit":200}"#).unwrap()));
            seen.extend(f.get("events").and_then(Json::as_array).unwrap().iter().filter_map(|e| e.get("code").and_then(Json::as_str).map(str::to_owned)));
            if f.get("more") == Some(&Json::from(false)) {
                break;
            }
        }
        assert!(seen.iter().any(|c| c == "test.later"), "{seen:?}");
        assert!(trace_sink::cursor("trace-tools-1").unwrap() > c1);
    }

    /// Review probe (b893d0a, blocker): another agent read ann's arguments through trace_call.
    #[test]
    fn digests_are_public_metadata_without_text() {
        let ann = session("trace-tools-ann-args");
        trace_sink::set_name("trace-tools-ann-args", "ann-args", "builder");
        let id = trace_sink::begin_call(&ann, "sight_frame", &Json::parse(r#"{"width":2048,"resume_token":"cafebabe12345678"}"#).unwrap());
        {
            let _c = trace::Ctx { call: id, span: 0 }.enter();
            let _sp = trace::span(&SUB, Level::Info, "test.args_work");
            specula_core::trace_event!(&SUB, Warn, "test.args_stall", args = "{width:2048}");
        }
        trace_sink::end_call(id, &ToolOutput::text("{}".to_owned()));
        let mallory = session("trace-tools-mallory-args");
        trace_sink::set_name("trace-tools-mallory-args", "mallory-args", "builder");
        // Another session sees the digest (keys, types, sizes, numbers: public metadata) and
        // never the secret.
        let theirs = call(&mallory, "trace_call", &Json::parse(&format!(r#"{{"call":{id},"response_format":"detailed"}}"#)).unwrap());
        let text = format!("{:?}", theirs.content);
        assert!(text.contains("width:2048") && !text.contains("cafebabe"), "{text}");
        let q = call(&mallory, "trace_query", &Json::parse(&format!(r#"{{"call":{id},"text":"cafe","response_format":"detailed"}}"#)).unwrap());
        assert_eq!(answer(q).get("matched").and_then(Json::as_u64), Some(0), "the text filter finds no hidden value");
        let own = answer(call(&ann, "trace_call", &Json::parse(&format!(r#"{{"call":{id}}}"#)).unwrap()));
        let own_args = own.get("call").and_then(|c| c.get("args")).and_then(Json::as_str).unwrap_or_default().to_owned();
        assert!(own_args.contains("width:2048") && own_args.contains("<redacted>") && !own_args.contains("cafebabe"), "{own_args}");
    }

    #[test]
    fn global_settings_need_the_lead() {
        let s = session("trace-tools-builder");
        trace_sink::set_name("trace-tools-builder", "bob", "builder");
        let out = call(&s, "trace_settings", &Json::parse(r#"{"subsystems":{"test.tools.lead":"debug"}}"#).unwrap());
        assert!(out.is_error);
        assert_eq!(answer(out).get("error").and_then(Json::as_str), Some("not_lead"));
        let own = call(&s, "trace_settings", &Json::parse(r#"{"scope":"session","level":"warn"}"#).unwrap());
        assert!(!own.is_error);
        assert_eq!(trace_sink::view("trace-tools-builder").level, Some(Level::Warn));
        let lead = session("trace-tools-lead");
        trace_sink::set_name("trace-tools-lead", "ann", "lead");
        crate::authority::publish("trace-tools-lead", crate::authority::Authority::Lead);
        let ok = answer(call(&lead, "trace_settings", &Json::parse(r#"{"subsystems":{"test.tools.lead":"debug"},"budgets_ms":{"render":45}}"#).unwrap()));
        assert_eq!(ok.get("budgets_ms").and_then(|b| b.get("render")).and_then(Json::as_f64), Some(45.0));
        static LEAD: trace::Subsystem = trace::Subsystem::new("test.tools.lead");
        assert!(LEAD.enabled(Level::Debug));
        let mut s = trace_sink::settings();
        s.budgets = trace_sink::Budgets::default();
        trace_sink::set_settings(s);
        assert!(call(&lead, "trace_settings", &Json::parse(r#"{"budgets_ms":{"nope":1}}"#).unwrap()).is_error);
    }

    #[test]
    fn bad_arguments_say_what_to_change() {
        let s = session("trace-tools-args");
        let out = call(&s, "trace_query", &Json::parse(r#"{"limit":5000}"#).unwrap());
        assert!(out.is_error);
        assert!(answer(out).get("message").and_then(Json::as_str).unwrap().contains("limit"));
        assert!(call(&s, "trace_query", &Json::parse(r#"{"tick":[5,1]}"#).unwrap()).is_error);
        assert!(call(&s, "trace_incident", &Json::parse(r#"{"id":1,"action":"link"}"#).unwrap()).is_error);
        assert!(call(&s, "trace_query", &Json::parse(r#"{"bogus":1}"#).unwrap()).is_error);
    }

    #[test]
    fn answers_are_byte_capped() {
        let s = session("trace-tools-cap");
        let long = "y".repeat(250);
        for _ in 0..300 {
            specula_core::trace_event!(&SUB, Info, "test.cap", a = long.as_str(), b = long.as_str(), c = long.as_str());
        }
        let out = call(&s, "trace_query", &Json::parse(r#"{"code":"test.cap","limit":200}"#).unwrap());
        let specula_protocol::Content::Text(t) = &out.content[0] else { panic!() };
        assert!(t.len() < MAX_BYTES + 4096, "{}", t.len());
        let j = Json::parse(t).unwrap();
        assert!(j.get("next_before").and_then(Json::as_u64).is_some(), "a cut answer pages on");
    }

    #[test]
    fn incidents_can_be_listed_acked_and_linked() {
        let s = session("trace-tools-inc");
        let id = incidents::raise(incidents::Raise {
            sub: &SUB,
            level: Level::Warn,
            code: "test.tools_incident",
            key: "k",
            summary: "x".into(),
            fields: vec![],
            evidence: Json::Null,
            call: 0,
            agent: None,
        });
        let l = answer(call(&s, "trace_incidents", &Json::parse(r#"{"code":"test.tools_incident"}"#).unwrap()));
        assert_eq!(l.get("matched").and_then(Json::as_u64), Some(1));
        let linked = answer(call(&s, "trace_incident", &Json::parse(&format!(r#"{{"id":{id},"action":"link","critique":3,"note":"filed"}}"#)).unwrap()));
        assert_eq!(linked.get("links").and_then(Json::as_array).map(<[Json]>::len), Some(1));
        // Review probes (b893d0a minor 13, e45f66c minor 4): anyone could resolve anyone's
        // incident, then anyone who acked it first.
        let stranger = call(&s, "trace_incident", &Json::parse(&format!(r#"{{"id":{id},"action":"resolve"}}"#)).unwrap());
        assert!(stranger.is_error, "not named, not a reviewer or the lead");
        assert_eq!(answer(stranger).get("error").and_then(Json::as_str), Some("not_owner"));
        let acked = answer(call(&s, "trace_incident", &Json::parse(&format!(r#"{{"id":{id},"action":"ack"}}"#)).unwrap()));
        assert_eq!(acked.get("status").and_then(Json::as_str), Some("acknowledged"));
        let still = call(&s, "trace_incident", &Json::parse(&format!(r#"{{"id":{id},"action":"resolve"}}"#)).unwrap());
        assert!(still.is_error, "an ack confers nothing");
        // An agent the incident names may (its calls raised it).
        let named = incidents::raise(incidents::Raise {
            sub: &SUB,
            level: Level::Warn,
            code: "test.tools_incident_named",
            key: "k",
            summary: "x".into(),
            fields: vec![],
            evidence: Json::Null,
            call: 0,
            agent: Some("trace-tools-named-agent".into()),
        });
        let fixer = session("trace-tools-fixer");
        trace_sink::set_name("trace-tools-fixer", "trace-tools-named-agent", "builder");
        let fixed = answer(call(&fixer, "trace_incident", &Json::parse(&format!(r#"{{"id":{named},"action":"resolve"}}"#)).unwrap()));
        assert_eq!(fixed.get("status").and_then(Json::as_str), Some("resolved"));
        // A reviewer may resolve one that names nobody.
        let reviewer = session("trace-tools-reviewer");
        trace_sink::set_name("trace-tools-reviewer", "rev", "reviewer");
        crate::authority::publish("trace-tools-reviewer", crate::authority::Authority::Reviewer);
        let done = answer(call(&reviewer, "trace_incident", &Json::parse(&format!(r#"{{"id":{id},"action":"resolve"}}"#)).unwrap()));
        assert_eq!(done.get("status").and_then(Json::as_str), Some("resolved"));
        let gone = answer(call(&s, "trace_incidents", &Json::parse(r#"{"code":"test.tools_incident"}"#).unwrap()));
        assert_eq!(gone.get("matched").and_then(Json::as_u64), Some(0), "resolved isn't active");
    }

    #[test]
    fn every_tool_has_a_definition() {
        let names: Vec<String> = definitions().into_iter().map(|d| d.name).collect();
        for n in ["trace_query", "trace_follow", "trace_call", "trace_settings", "trace_crash", "trace_incidents", "trace_incident"] {
            assert!(names.iter().any(|x| x == n), "{n}");
        }
    }
}

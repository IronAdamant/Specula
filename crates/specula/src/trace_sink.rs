//! The engine's side of tracing (docs/design/TRACE.md): the call table (every MCP call's id,
//! agent and tool), agents' names, the sinks (a rotated JSONL file and stderr), crash reports and
//! the settings the trace tools change.
//!
//! Everything here is global (like the profiler): agent calls are answered on connection threads,
//! the engine thread and workers emit, and the panic hook reads it from wherever a panic happens.
//! Every call takes two short locks of its own (the call table, then the names' read lock); the
//! settings the engine thread reads per call are atomics.
//!
//! **Arguments are never kept verbatim.** A call's digest records each top-level key with its
//! value's type and size; only numbers and booleans keep their values, and keys that name
//! secrets (`token`, `secret`, `password`, `auth`, `key`) are always redacted. A resume token
//! (an `agent_hello` argument today) can't leak through stalls, `trace_call`, the JSONL file or a
//! crash report. M6's per-agent auth tokens travel in a request header, never as an argument.

use crate::disk::{self, Category, Need};
use crate::schedule::Class;
use specula_core::trace::{self, Event, Level, Subsystem};
use specula_core::Json;
use specula_protocol::{Content, SessionInfo, ToolOutput};
use std::collections::{BTreeMap, VecDeque};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock};
use std::time::Duration;

/// Agent calls (their records, at debug: one event each).
pub static MCP: Subsystem = Subsystem::new("mcp");
/// The engine thread and its loop (stalls, frames).
pub static ENGINE: Subsystem = Subsystem::new("engine");
/// Workers and queued jobs (model builds, studio renders, previews, save writers).
pub static JOBS: Subsystem = Subsystem::new("jobs");
/// Write-policy decisions.
pub static WRITES: Subsystem = Subsystem::new("write_policy");
/// Sight packets.
pub static SIGHT: Subsystem = Subsystem::new("sight");
/// GPU messages and failures.
pub static GPU: Subsystem = Subsystem::new("gpu");
/// The module host.
pub static LOGIC: Subsystem = Subsystem::new("logic");
/// World checks (replays).
pub static WORLD: Subsystem = Subsystem::new("world");
/// Crash reports, sinks and settings.
pub static TRACE: Subsystem = Subsystem::new("trace");

/// Calls kept in the call table.
pub const MAX_CALLS: usize = 4096;
/// Journal entries kept per call record.
pub const MAX_CALL_ENTRIES: usize = 64;
/// Sessions whose agent names are kept.
const MAX_SESSIONS: usize = 1024;
/// Events a crash report keeps.
pub const CRASH_EVENTS: usize = 256;
/// Crash reports kept in the trace folder, and reports of panics that were caught (a structured
/// error, the engine ran on: kept apart so they never push a real crash out).
pub const CRASH_KEPT: usize = 8;
pub const CAUGHT_KEPT: usize = 4;
/// The file sink starts a new file past this size; this many trace files are kept in the folder
/// (whichever engines wrote them).
pub const FILE_BYTES: u64 = 4 << 20;
pub const FILE_COUNT: usize = 8;
/// How often the sink thread drains threads and writes.
const SINK_EVERY: Duration = Duration::from_millis(100);
/// Characters kept of a call's argument digest, and of each key in it.
const DIGEST_CHARS: usize = 160;
const DIGEST_KEY_CHARS: usize = 32;
/// The crash report file prefixes.
pub const CRASH_PREFIX: &str = "crash-";
pub const CAUGHT_PREFIX: &str = "caught-";

/// One MCP call.
#[derive(Clone, Debug, PartialEq)]
pub struct CallRecord {
    pub id: u64,
    pub session: Arc<str>,
    pub agent: Arc<str>,
    pub tool: Arc<str>,
    /// Its arguments in brief ([`digest`]): keys, types, sizes and numbers, never text.
    pub args: Arc<str>,
    /// Journal entries it made (at most [`MAX_CALL_ENTRIES`]), noted as they are appended.
    pub journal: Vec<u64>,
    /// Since the trace epoch.
    pub started_us: u64,
    /// `None` while it runs.
    pub ms: Option<f64>,
    /// The error code it answered with, if any.
    pub error: Option<String>,
}

impl CallRecord {
    /// As JSON, its argument digest included: digests hold no text (keys, types, sizes, numbers),
    /// so they are public metadata (docs/design/TRACE.md).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("call", Json::from(self.id)),
            ("tool", Json::from(&*self.tool)),
            ("agent", Json::from(&*self.agent)),
            ("args", Json::from(&*self.args)),
            ("t_ms", Json::from((self.started_us / 100) as f64 / 10.0)),
            ("ms", self.ms.map_or(Json::Null, |m| Json::from((m * 100.0).round() / 100.0))),
            ("error", Json::from(self.error.clone())),
        ])
    }
}

// ── settings (atomics: the engine thread reads them per call) ──────────────

/// How long one piece of engine-thread work may take before it is `engine.stall`, per call class
/// ([`Class`]) and for a window frame's own work, in milliseconds.
///
/// **Why these numbers.** A 60 Hz frame is 16.7 ms, and the schedule serves calls in the part of
/// it the frame doesn't need, keeping a 0.5-3 ms margin before the present. Reads and edits
/// normally cost tens of microseconds (the schedule's floor for them is 50 us): **4 ms**, a
/// quarter frame, is already more than the margin, so one such call pushes the next present past
/// its vblank. Renders, disk work, big batches, stepped simulation and game tools legitimately
/// cost about a frame: **30 ms** is two 60 Hz frames, where one dropped frame becomes a hitch a
/// human sees. A window frame's own work (outside serving and present) gets 30 ms too: past it
/// the window visibly stutters whatever caused it. Single tools with their own budget:
/// [`TOOL_BUDGETS`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Budgets {
    pub read: f64,
    pub edit: f64,
    pub render: f64,
    pub disk: f64,
    pub big: f64,
    pub sim: f64,
    pub game: f64,
    pub frame: f64,
}

impl Default for Budgets {
    fn default() -> Budgets {
        Budgets { read: 4.0, edit: 4.0, render: 30.0, disk: 30.0, big: 30.0, sim: 30.0, game: 30.0, frame: 30.0 }
    }
}

/// Tools with a budget of their own (milliseconds), over their class's.
///
/// - `perf_frame` (read class): it copies both paths' frame statistics (the window's flash grid
///   included), evaluates every perf.toml budget and serialises the render graph: 4.5 ms in a dev
///   build and up to 6.3 ms in release on the reference laptop (review, 2026-10-29). Agents call
///   it occasionally, not in loops (perf_capture records frames), so **10 ms** (well under one
///   60 Hz frame) is its budget: past that something about it changed.
pub const TOOL_BUDGETS: [(&str, f64); 1] = [("perf_frame", 10.0)];

impl Budgets {
    pub const NAMES: [&'static str; 8] = ["read", "edit", "render", "disk", "big", "sim", "game", "frame"];

    pub fn of(&self, c: Class) -> f64 {
        match c {
            Class::Read => self.read,
            Class::Edit => self.edit,
            Class::Render => self.render,
            Class::Disk => self.disk,
            Class::Big => self.big,
            Class::Sim => self.sim,
            Class::Game => self.game,
        }
    }

    fn values(&self) -> [f64; 8] {
        [self.read, self.edit, self.render, self.disk, self.big, self.sim, self.game, self.frame]
    }

    fn from_values(v: [f64; 8]) -> Budgets {
        Budgets { read: v[0], edit: v[1], render: v[2], disk: v[3], big: v[4], sim: v[5], game: v[6], frame: v[7] }
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut f64> {
        Some(match name {
            "read" => &mut self.read,
            "edit" => &mut self.edit,
            "render" => &mut self.render,
            "disk" => &mut self.disk,
            "big" => &mut self.big,
            "sim" => &mut self.sim,
            "game" => &mut self.game,
            "frame" => &mut self.frame,
            _ => return None,
        })
    }

    pub fn to_json(&self) -> Json {
        Json::obj(Self::NAMES.iter().zip(self.values()).map(|(n, ms)| (*n, Json::from(ms))))
    }
}

/// What the trace tools change (and the monitors read).
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub budgets: Budgets,
    pub file: Option<Level>,
    pub stderr: Option<Level>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { budgets: Budgets::default(), file: Some(Level::Info), stderr: Some(Level::Warn) }
    }
}

const UNSET: u8 = u8::MAX;
static BUDGETS: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
static BUDGETS_SET: AtomicBool = AtomicBool::new(false);
static FILE_LEVEL: AtomicU8 = AtomicU8::new(UNSET);
static STDERR_LEVEL: AtomicU8 = AtomicU8::new(UNSET);

fn level_u8(l: Option<Level>) -> u8 {
    l.map_or(0, |l| l as u8)
}

fn u8_level(v: u8, default: Option<Level>) -> Option<Level> {
    if v == UNSET {
        return default;
    }
    Level::ALL.into_iter().find(|l| *l as u8 == v)
}

/// The settings now (a snapshot of atomics: no lock).
pub fn settings() -> Settings {
    let d = Settings::default();
    let budgets = if BUDGETS_SET.load(Relaxed) { Budgets::from_values(std::array::from_fn(|i| f64::from_bits(BUDGETS[i].load(Relaxed)))) } else { d.budgets };
    Settings { budgets, file: u8_level(FILE_LEVEL.load(Relaxed), d.file), stderr: u8_level(STDERR_LEVEL.load(Relaxed), d.stderr) }
}

pub fn set_settings(new: Settings) {
    for (a, v) in BUDGETS.iter().zip(new.budgets.values()) {
        a.store(v.to_bits(), Relaxed);
    }
    BUDGETS_SET.store(true, Relaxed);
    FILE_LEVEL.store(level_u8(new.file), Relaxed);
    STDERR_LEVEL.store(level_u8(new.stderr), Relaxed);
}

/// The budget for one piece of work: the tool's own, else its class's, else the frame's.
pub fn budget_for(tool: Option<&str>, class: Option<Class>) -> f64 {
    if let Some(ms) = tool.and_then(|t| TOOL_BUDGETS.iter().find(|(n, _)| *n == t)).map(|(_, ms)| *ms) {
        return ms;
    }
    let b = settings().budgets;
    class.map_or(b.frame, |c| b.of(c))
}

/// A session's own view: the filter its trace_query and trace_follow default to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub level: Option<Level>,
    pub subsystems: Vec<String>,
}

// ── calls and names ─────────────────────────────────────────────────────────

/// The call table: ids are handed out under its lock, so records are in id order and a record
/// is found by its offset from the oldest.
struct Calls {
    next: u64,
    ring: VecDeque<CallRecord>,
}

static CALLS: Mutex<Calls> = Mutex::new(Calls { next: 1, ring: VecDeque::new() });

impl Calls {
    fn index(&self, id: u64) -> Option<usize> {
        let first = self.ring.front()?.id;
        let i = usize::try_from(id.checked_sub(first)?).ok()?;
        (i < self.ring.len()).then_some(i)
    }
}

struct Named {
    agent: Arc<str>,
    role: Arc<str>,
}

#[derive(Default)]
struct Names {
    map: BTreeMap<String, Named>,
    /// Sessions in the order they were named (the oldest are forgotten past MAX_SESSIONS).
    order: VecDeque<String>,
}

static NAMES: RwLock<Names> = RwLock::new(Names { map: BTreeMap::new(), order: VecDeque::new() });

#[derive(Default)]
struct Sessions {
    cursors: BTreeMap<String, u64>,
    views: BTreeMap<String, View>,
}

static SESSIONS: Mutex<Sessions> = Mutex::new(Sessions { cursors: BTreeMap::new(), views: BTreeMap::new() });

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn calls() -> MutexGuard<'static, Calls> {
    lock(&CALLS)
}

/// Whether a key names a secret: its value is never recorded.
pub fn secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    ["token", "secret", "password", "passwd", "auth", "credential"].iter().any(|s| k.contains(s))
        || k.split(['_', '-', '.']).any(|part| part == "key" || part.ends_with("key") && part.len() > 3 && part.starts_with("api"))
        || k == "key"
}

/// A call's arguments in brief, for stall reports: each top-level key with its value's type and
/// size (`name:str(4)`, `ops:[300]`, `props:{2}`); numbers and booleans keep their values
/// (`width:2048`); keys that name secrets are redacted whatever their value. At most
/// [`DIGEST_CHARS`] characters, keys cut at [`DIGEST_KEY_CHARS`]. O(top-level keys): nothing is
/// serialised.
pub fn digest(args: &Json) -> String {
    use std::fmt::Write as _;
    let Json::Obj(pairs) = args else { return String::new() };
    let mut out = String::from("{");
    for (i, (k, v)) in pairs.iter().enumerate() {
        if out.len() >= DIGEST_CHARS {
            out.push_str("...");
            break;
        }
        if i > 0 {
            out.push_str(", ");
        }
        let key: String = k.chars().filter(|c| !c.is_control()).take(DIGEST_KEY_CHARS).collect();
        let _ = if secret_key(k) {
            write!(out, "{key}:<redacted>")
        } else {
            match v {
                Json::Null => write!(out, "{key}:null"),
                Json::Bool(b) => write!(out, "{key}:{b}"),
                Json::Num(n) => write!(out, "{key}:{}", Json::Num(*n)),
                Json::Str(s) => write!(out, "{key}:str({})", s.chars().count()),
                Json::Arr(a) => write!(out, "{key}:[{}]", a.len()),
                Json::Obj(o) => write!(out, "{key}:{{{}}}", o.len()),
            }
        };
    }
    out.push('}');
    if out.chars().count() > DIGEST_CHARS + 4 {
        out = out.chars().take(DIGEST_CHARS).collect::<String>() + "...";
    }
    out
}

fn agent_of_session(session: &SessionInfo) -> Arc<str> {
    let names = NAMES.read().unwrap_or_else(PoisonError::into_inner);
    names.map.get(&session.id).map_or_else(|| Arc::from(crate::swarm::default_name(session)), |n| Arc::clone(&n.agent))
}

/// A call starts: its id, recorded with its session's agent, the tool and its arguments' digest
/// (built before any lock).
pub fn begin_call(session: &SessionInfo, tool: &str, args: &Json) -> u64 {
    let args: Arc<str> = Arc::from(digest(args));
    let agent = agent_of_session(session);
    // Tool names are bounded by the protocol (64 characters); unknown ones are answered as errors.
    let tool: Arc<str> = Arc::from(tool.chars().take(64).collect::<String>());
    let session: Arc<str> = Arc::from(session.id.as_str());
    let started_us = trace::now_us();
    let mut c = calls();
    let id = c.next;
    c.next += 1;
    if c.ring.len() >= MAX_CALLS {
        c.ring.pop_front();
    }
    c.ring.push_back(CallRecord { id, session, agent, tool, args, journal: Vec::new(), started_us, ms: None, error: None });
    id
}

/// The call is really `tool` with `args` (a `tools_invoke` names the tool it calls).
pub fn invoked(id: u64, tool: &str, args: &Json) {
    let digest: Arc<str> = Arc::from(digest(args));
    let tool: Arc<str> = Arc::from(tool.chars().take(64).collect::<String>());
    let mut c = calls();
    let Some(i) = c.index(id) else { return };
    c.ring[i].tool = tool;
    c.ring[i].args = digest;
}

/// A call ends with `out`: its duration and error code go on its record.
pub fn end_call(id: u64, out: &ToolOutput) {
    let error = out.is_error.then(|| error_code(out));
    let now = trace::now_us();
    let (tool, ms) = {
        let mut c = calls();
        let Some(i) = c.index(id) else { return };
        let rec = &mut c.ring[i];
        let ms = (now.saturating_sub(rec.started_us)) as f64 / 1000.0;
        rec.ms = Some(ms);
        rec.error = error.clone();
        (Arc::clone(&rec.tool), ms)
    };
    specula_core::trace_event!(&MCP, Debug, "mcp.call", tool = &*tool, ms = ms, error = error);
}

/// The error code of a tool's error answer (compact JSON `{error, message}`; else `error`).
fn error_code(out: &ToolOutput) -> String {
    out.content
        .iter()
        .find_map(|c| match c {
            Content::Text(t) if t.starts_with('{') => Json::parse(t).ok()?.get("error")?.as_str().map(|s| s.chars().take(64).collect()),
            _ => None,
        })
        .unwrap_or_else(|| "error".to_owned())
}

/// agent_hello named `session`.
pub fn set_name(session: &str, agent: &str, role: &str) {
    let mut names = NAMES.write().unwrap_or_else(PoisonError::into_inner);
    if !names.map.contains_key(session) {
        if names.map.len() >= MAX_SESSIONS {
            if let Some(old) = names.order.pop_front() {
                names.map.remove(&old);
                let mut s = lock(&SESSIONS);
                s.cursors.remove(&old);
                s.views.remove(&old);
            }
        }
        names.order.push_back(session.to_owned());
    }
    names.map.insert(session.to_owned(), Named { agent: Arc::from(agent), role: Arc::from(role) });
}

/// `session`'s agent name and role (as agent_hello set them; unnamed: `None`).
pub fn name_of(session: &str) -> Option<(String, String)> {
    let names = NAMES.read().unwrap_or_else(PoisonError::into_inner);
    names.map.get(session).map(|n| (n.agent.to_string(), n.role.to_string()))
}

/// The record of call `id`, if still kept.
pub fn call(id: u64) -> Option<CallRecord> {
    let c = calls();
    c.index(id).map(|i| c.ring[i].clone())
}

/// The tool call `id` was (`None`: not an agent's call, or forgotten).
pub fn tool_of(id: u64) -> Option<Arc<str>> {
    let c = calls();
    c.index(id).map(|i| Arc::clone(&c.ring[i].tool))
}

/// The agent of call `id`.
pub fn agent_of(id: u64) -> Option<Arc<str>> {
    let c = calls();
    c.index(id).map(|i| Arc::clone(&c.ring[i].agent))
}

/// The session of call `id`.
pub fn session_of(id: u64) -> Option<Arc<str>> {
    let c = calls();
    c.index(id).map(|i| Arc::clone(&c.ring[i].session))
}

/// Ids of the kept calls `agent` made, at most `limit` (the newest).
pub fn calls_of(agent: &str, limit: usize) -> std::collections::BTreeSet<u64> {
    let c = calls();
    c.ring.iter().rev().filter(|r| &*r.agent == agent).take(limit).map(|r| r.id).collect()
}

/// The journal appended entry `seq` inside call `call` (0: no call; not kept). Kept on the call's
/// record: calls commit in any order (review, e45f66c: a list in commit order searched by call id
/// lost 57 of 2,040 calls' entries).
pub fn note_journal(call: u64, seq: u64) {
    if call == 0 {
        return;
    }
    let mut c = calls();
    let Some(i) = c.index(call) else { return };
    let j = &mut c.ring[i].journal;
    if j.len() < MAX_CALL_ENTRIES {
        j.push(seq);
    }
}

/// Journal entries call `call` made (at most [`MAX_CALL_ENTRIES`]).
pub fn journal_of(call: u64) -> Vec<u64> {
    let c = calls();
    c.index(call).map(|i| c.ring[i].journal.clone()).unwrap_or_default()
}

/// The newest kept call of `session` before `before` (its previous call).
pub fn previous_call(session: &str, before: u64) -> Option<u64> {
    calls().ring.iter().rev().find(|c| c.id < before && &*c.session == session).map(|c| c.id)
}

/// `session`'s trace_follow cursor.
pub fn cursor(session: &str) -> Option<u64> {
    lock(&SESSIONS).cursors.get(session).copied()
}

pub fn set_cursor(session: &str, at: u64) {
    let mut s = lock(&SESSIONS);
    if s.cursors.len() >= MAX_SESSIONS && !s.cursors.contains_key(session) {
        let Some(old) = s.cursors.keys().next().cloned() else { return };
        s.cursors.remove(&old);
    }
    s.cursors.insert(session.to_owned(), at);
}

pub fn view(session: &str) -> View {
    lock(&SESSIONS).views.get(session).cloned().unwrap_or_default()
}

pub fn set_view(session: &str, view: View) {
    let mut s = lock(&SESSIONS);
    if s.views.len() >= MAX_SESSIONS && !s.views.contains_key(session) {
        let Some(old) = s.views.keys().next().cloned() else { return };
        s.views.remove(&old);
    }
    s.views.insert(session.to_owned(), view);
}

// ── identity ────────────────────────────────────────────────────────────────

/// Which engine wrote a line or a report: engines started from one folder share its trace folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Identity {
    pub pid: u32,
    pub port: u16,
    /// Random per run (the OS's randomness through std's hasher keys; not a secret).
    pub run: String,
}

static IDENTITY: OnceLock<Identity> = OnceLock::new();

fn run_nonce() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(trace::epoch_unix_ms());
    h.write_u32(std::process::id());
    format!("{:08x}", h.finish() as u32)
}

pub fn identity() -> Option<&'static Identity> {
    IDENTITY.get()
}

impl Identity {
    pub fn to_json(&self) -> Json {
        Json::obj([("pid", Json::from(self.pid)), ("port", Json::from(u32::from(self.port))), ("run", Json::from(self.run.as_str()))])
    }
}

// ── sinks ───────────────────────────────────────────────────────────────────

static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
/// This run's trace file stem (`trace-<unix ms>-<pid>`) and its part number.
static FILE_STEM: Mutex<Option<(String, u32)>> = Mutex::new(None);

/// Starts the sinks: the JSONL file in `dir` (when `file` is on) and stderr, written by a thread
/// of their own every 100 ms; and the panic hook that writes crash reports to `dir`. `port`
/// names this engine in every line and report.
pub fn start(dir: &Path, file: bool, port: u16) {
    let _ = IDENTITY.set(Identity { pid: std::process::id(), port, run: run_nonce() });
    *lock(&DIR) = Some(disk::canonical(dir));
    *lock(&FILE_STEM) = Some((format!("trace-{}-{}", trace::epoch_unix_ms(), std::process::id()), 0));
    if !file {
        let mut s = settings();
        s.file = None;
        set_settings(s);
    }
    if let Err(e) = disk::book().make_dir(Category::Cache, dir, Need::Essential) {
        specula_core::trace_event!(&TRACE, Warn, "trace.dir_unavailable", error = e.to_string());
    }
    install_panic_hook();
    let spawned = std::thread::Builder::new().name("specula-trace-sink".into()).spawn(|| loop {
        std::thread::sleep(SINK_EVERY);
        pump();
    });
    if let Err(e) = spawned {
        specula_core::trace_event!(&TRACE, Warn, "trace.sink_not_started", error = e.to_string());
    }
}

/// The trace folder (crash reports, the JSONL files), once started.
pub fn dir() -> Option<PathBuf> {
    lock(&DIR).clone()
}

/// Writes what the sinks haven't yet (at shutdown).
pub fn finish() {
    pump();
}

static SINK_CURSOR: Mutex<u64> = Mutex::new(0);

/// Writes new events to stderr and the file (the cursor is shared: the final [`finish`]
/// continues where the thread was).
fn pump() {
    let mut cursor = lock(&SINK_CURSOR);
    let set = settings();
    let widest = match (set.file, set.stderr) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    let from = *cursor;
    let (events, next): (Vec<Event>, u64) =
        trace::read(|r| (r.after(from).filter(|e| widest.is_some_and(|w| e.level <= w)).cloned().collect(), r.next_id().saturating_sub(1)));
    *cursor = next;
    if events.is_empty() {
        return;
    }
    if let Some(level) = set.stderr {
        let mut err = std::io::stderr().lock();
        for e in events.iter().filter(|e| e.level <= level) {
            let _ = writeln!(err, "specula: {}", e.line());
        }
    }
    if let (Some(level), Some(dir)) = (set.file, dir()) {
        let mut text = String::new();
        let wall = trace::epoch_unix_ms();
        let id = identity();
        for e in events.iter().filter(|e| e.level <= level) {
            let mut j = e.to_json();
            j.set("unix_ms", Json::from(wall + e.time_us / 1000));
            if let Some(id) = id {
                j.set("pid", Json::from(id.pid));
                j.set("port", Json::from(u32::from(id.port)));
                j.set("run", Json::from(id.run.as_str()));
            }
            text.push_str(&j.to_string());
            text.push('\n');
        }
        append_rotated(&dir, &text);
    }
}

/// Trace files in `dir`, oldest first (by the start time in their names).
fn trace_files(dir: &Path) -> Vec<(String, PathBuf)> {
    // Sorted by (start ms, pid, part) as numbers: names sort wrong past part 999 or between
    // pids of different widths (review, e45f66c: rotation could delete the newest part).
    let key = |n: &str| -> Option<(u64, u64, u64)> {
        let mut it = n.strip_prefix("trace-")?.strip_suffix(".jsonl")?.split('-').map(|x| x.parse::<u64>().ok());
        Some((it.next()??, it.next()??, it.next()??))
    };
    let mut files: Vec<((u64, u64, u64), String, PathBuf)> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().into_string().ok().and_then(|n| key(&n).map(|k| (k, n, e.path())))).collect())
        .unwrap_or_default();
    files.sort();
    files.into_iter().map(|(_, n, p)| (n, p)).collect()
}

/// Appends `text` to this run's trace file in `dir` through the disk book (a new part past
/// [`FILE_BYTES`]); the oldest files past [`FILE_COUNT`] go.
fn append_rotated(dir: &Path, text: &str) {
    let path = {
        let mut stem = lock(&FILE_STEM);
        let (name, part) = stem.get_or_insert_with(|| (format!("trace-{}-{}", trace::epoch_unix_ms(), std::process::id()), 0));
        let mut path = dir.join(format!("{name}-{part:03}.jsonl"));
        let size = std::fs::metadata(&path).map_or(0, |m| m.len());
        if size > 0 && size + text.len() as u64 > FILE_BYTES {
            *part += 1;
            path = dir.join(format!("{name}-{part:03}.jsonl"));
        }
        path
    };
    let new = !path.exists();
    if disk::book().append(Category::Cache, "trace", &path, text.as_bytes(), Need::Essential).is_ok() && new {
        let files = trace_files(dir);
        for (_, old) in files.iter().take(files.len().saturating_sub(FILE_COUNT)) {
            if *old != path {
                if let Ok(bytes) = disk::remove_file_inside(dir, old) {
                    disk::book().wrote(Category::Cache, "trace", -(bytes as i64), -1);
                }
            }
        }
    }
}

// ── crash reports ───────────────────────────────────────────────────────────

/// Writes a crash report (the reason, the message, where, the context, this engine's identity and
/// the last [`CRASH_EVENTS`] events) to the trace folder; the newest [`CRASH_KEPT`] are kept
/// ([`CAUGHT_KEPT`] of `caught_panic`, kept apart). Never blocks on the trace's locks or the disk
/// book (it may run in a panic hook): the files are a reserved budget, bounded by count. Returns
/// its path.
pub fn write_crash(reason: &str, message: &str, location: Option<&str>, extra: Json) -> Option<PathBuf> {
    let dir = DIR.try_lock().ok().and_then(|d| d.clone())?;
    let ctx = trace::Ctx::current();
    let (tick, frame) = trace::now();
    let unix_ms = trace::epoch_unix_ms() + trace::now_us() / 1000;
    let events = trace::last_nonblocking(CRASH_EVENTS);
    let call = CALLS.try_lock().ok().and_then(|c| c.index(ctx.call).map(|i| c.ring[i].to_json()));
    let report = Json::obj([
        ("reason", Json::from(reason)),
        ("message", Json::from(message.chars().take(4096).collect::<String>())),
        ("location", Json::from(location)),
        ("thread", Json::from(std::thread::current().name().unwrap_or("unnamed"))),
        ("unix_ms", Json::from(unix_ms)),
        ("engine", identity().map_or(Json::Null, Identity::to_json)),
        ("tick", Json::from(tick)),
        ("frame", Json::from(frame)),
        ("call", call.unwrap_or(Json::Null)),
        ("engine_version", Json::from(env!("CARGO_PKG_VERSION"))),
        ("extra", extra),
        ("events_kept", Json::from(events.len())),
        ("events", Json::arr(events.iter().map(Event::to_json))),
    ]);
    let (prefix, keep) = if reason == "caught_panic" { (CAUGHT_PREFIX, CAUGHT_KEPT) } else { (CRASH_PREFIX, CRASH_KEPT) };
    // Reports are named by time; two in one millisecond get a suffix.
    let mut path = dir.join(format!("{prefix}{unix_ms}.json"));
    for n in 1..100 {
        if !path.exists() {
            break;
        }
        path = dir.join(format!("{prefix}{unix_ms}-{n}.json"));
    }
    disk::book().write_reserved(Category::Cache, "trace", &path, report.to_string().as_bytes()).ok()?;
    for old in crash_files(&dir, prefix).into_iter().skip(keep) {
        if let Ok(bytes) = disk::remove_file_inside(&dir, &old) {
            disk::book().account_reserved(Category::Cache, "trace", -(bytes as i64), -1);
        }
    }
    Some(path)
}

/// Report files with `prefix` in `dir`, newest first.
pub fn crash_files(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut files: Vec<((u64, u32), PathBuf)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().into_string().ok()?;
                    let stem = name.strip_prefix(prefix)?.strip_suffix(".json")?;
                    let (ms, n) = match stem.split_once('-') {
                        Some((ms, n)) => (ms.parse::<u64>().ok()?, n.parse::<u32>().ok()?),
                        None => (stem.parse::<u64>().ok()?, 0),
                    };
                    Some(((ms, n), e.path()))
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files.into_iter().map(|(_, p)| p).collect()
}

/// Panics on threads inside [`trace::catch_unwind`] are caught (a structured error, the engine
/// runs on): reported as `caught_panic`, kept apart from crashes.
fn install_panic_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic with a non-string payload".to_owned());
            let location = info.location().map(|l| format!("{}:{}", l.file(), l.line()));
            let reason = if trace::is_catching() { "caught_panic" } else { "panic" };
            write_crash(reason, &message, location.as_deref(), Json::Null);
            previous(info);
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str) -> SessionInfo {
        SessionInfo { id: id.into(), client_name: "client".into(), client_version: "0".into(), protocol_version: "x".into() }
    }

    #[test]
    fn calls_are_recorded_with_their_agent_and_outcome() {
        let s = session("trace-sink-test-1");
        let before = begin_call(&s, "world_spawn", &Json::Null);
        assert_eq!(call(before).map(|c| c.agent.to_string()), Some(crate::swarm::default_name(&s)));
        set_name("trace-sink-test-1", "builder-ann", "builder");
        let id = begin_call(&s, "world_set", &Json::parse(r#"{"path":"/a","position":[1,2,3],"props":{"x":1,"y":2},"expect_version":3}"#).unwrap());
        end_call(id, &ToolOutput::error(r#"{"error":"claimed","message":"held by bob"}"#.to_owned()));
        let rec = call(id).unwrap();
        assert_eq!(&*rec.agent, "builder-ann");
        assert_eq!(&*rec.tool, "world_set");
        assert_eq!(rec.error.as_deref(), Some("claimed"));
        assert!(rec.ms.is_some());
        assert_eq!(tool_of(id).as_deref(), Some("world_set"));
        assert!(calls_of("builder-ann", 10).contains(&id));
        assert_eq!(name_of("trace-sink-test-1"), Some(("builder-ann".into(), "builder".into())));
        assert_eq!(tool_of(0), None);
        assert_eq!(&*rec.args, "{path:str(2), position:[3], props:{2}, expect_version:3}");
        assert!(rec.to_json().get("args").is_some(), "digests are public metadata");
        invoked(id, "world_spawn", &Json::parse(r#"{"shape":"cube"}"#).unwrap());
        assert_eq!(tool_of(id).as_deref(), Some("world_spawn"), "tools_invoke records the inner tool");
        assert_eq!(call(id).map(|c| c.args.to_string()).as_deref(), Some("{shape:str(4)}"));
    }

    /// Review probe (b893d0a, blocker): a resume token in agent_hello's arguments was readable by
    /// any agent through the digest.
    #[test]
    fn digests_never_keep_text_or_secrets() {
        let hello = Json::parse(r#"{"name":"ann","resume_token":"0123456789abcdef","role":"lead","api_key":42,"password":"x","keys":[1,2]}"#).unwrap();
        let d = digest(&hello);
        assert!(!d.contains("0123456789abcdef") && !d.contains("ann") && !d.contains("lead"), "{d}");
        assert_eq!(d, "{name:str(3), resume_token:<redacted>, role:str(4), api_key:<redacted>, password:<redacted>, keys:[2]}");
        assert!(secret_key("auth_header") && secret_key("key") && secret_key("SECRET") && !secret_key("monkeys") && !secret_key("keys"));
    }

    /// Review probe (e45f66c, major 2): calls committing out of id order lost their entries.
    #[test]
    fn journal_entries_stay_with_their_call_whatever_the_order() {
        let s = session("trace-sink-journal");
        let (a, b, c) = (begin_call(&s, "world_set", &Json::Null), begin_call(&s, "world_set", &Json::Null), begin_call(&s, "world_set", &Json::Null));
        // Commits land in another order than the ids were handed out.
        note_journal(c, 10);
        note_journal(a, 11);
        note_journal(b, 12);
        note_journal(a, 13);
        assert_eq!(journal_of(a), [11, 13]);
        assert_eq!(journal_of(b), [12]);
        assert_eq!(journal_of(c), [10]);
        for i in 0..100 {
            note_journal(b, 100 + i);
        }
        assert_eq!(journal_of(b).len(), MAX_CALL_ENTRIES, "bounded");
        assert!(journal_of(0).is_empty());
    }

    /// Review decision (e45f66c, minor 3): digests are public, so no string value may ever appear
    /// in one, whatever the arguments' shape.
    #[test]
    fn no_string_value_ever_appears_in_a_digest() {
        let secrets = ["zq-secret-1", "zq-secret-2", "zq-secret-3", "zq-secret-4", "zq-secret-5"];
        let samples = [
            format!(r#"{{"name":"{}","path":"/{}","nested":{{"x":"{}"}},"list":["{}"],"token":"{}"}}"#, secrets[0], secrets[1], secrets[2], secrets[3], secrets[4]),
            format!(r#"{{"{}":1,"a":"{}","b":true,"c":null,"d":2.5}}"#, "k", secrets[0]),
            format!(r#"["{}"]"#, secrets[1]),
            format!(r#""{}""#, secrets[2]),
        ];
        for text in &samples {
            let d = digest(&Json::parse(text).unwrap());
            assert!(secrets.iter().all(|s| !d.contains(s)), "{d}");
            assert!(!d.contains('"'), "no quoted text at all: {d}");
        }
        assert_eq!(digest(&Json::parse(r#"{"d":2.5,"b":true}"#).unwrap()), "{d:2.5, b:true}", "numbers and booleans are kept");
    }

    #[test]
    fn digests_are_short_whatever_the_arguments() {
        let ops = Json::arr((0..10_000).map(|i| Json::from(i as u64)));
        let big = Json::obj((0..500).map(|i| (format!("key{i}"), ops.clone())));
        let d = digest(&big);
        assert!(d.chars().count() <= DIGEST_CHARS + 8, "{d}");
        assert_eq!(digest(&Json::Null), "");
        // Review probe: a 16 MB key cost 35 ms under the global lock. Keys are cut.
        let huge_key = Json::obj([("k".repeat(1 << 20), Json::from(1u64))]);
        let started = std::time::Instant::now();
        let d = digest(&huge_key);
        assert!(d.len() < DIGEST_CHARS + 16, "{}", d.len());
        assert!(started.elapsed() < Duration::from_millis(50));
    }

    /// Review probe (b893d0a, major 2): under concurrency about a third of the records couldn't
    /// be found (ids handed out before the lock, then a binary search on an unsorted table).
    #[test]
    fn every_call_is_findable_under_concurrency() {
        let threads: Vec<_> = (0..8)
            .map(|t| {
                std::thread::spawn(move || {
                    let s = session(&format!("trace-sink-conc-{t}"));
                    (0..400).map(|_| begin_call(&s, "world_query", &Json::Null)).collect::<Vec<u64>>()
                })
            })
            .collect();
        let ids: Vec<u64> = threads.into_iter().flat_map(|h| h.join().unwrap()).collect();
        assert_eq!(ids.len(), 3200);
        let lost: Vec<&u64> = ids.iter().filter(|id| call(**id).is_none_or(|c| c.id != **id)).collect();
        assert!(lost.is_empty(), "{} of 3200 records not found", lost.len());
        let c = calls();
        assert!(c.ring.iter().zip(c.ring.iter().skip(1)).all(|(a, b)| a.id + 1 == b.id), "the table is in id order");
    }

    #[test]
    fn settings_are_atomic_snapshots() {
        let mut s = settings();
        s.budgets.render = 45.0;
        set_settings(s.clone());
        assert_eq!(settings().budgets.render, 45.0);
        assert_eq!(budget_for(Some("perf_frame"), Some(Class::Read)), 10.0);
        assert_eq!(budget_for(Some("world_query"), Some(Class::Read)), s.budgets.read);
        s.budgets.render = Budgets::default().render;
        set_settings(s);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("specula-trace-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        disk::canonical(&dir)
    }

    /// Review nit (e45f66c): names sorted as text put part 1000 before part 999.
    #[test]
    fn trace_files_sort_by_number() {
        let dir = temp_dir("sort");
        for name in ["trace-1700000000000-9-999.jsonl", "trace-1700000000000-9-1000.jsonl", "trace-1600000000000-12345-002.jsonl"] {
            std::fs::write(dir.join(name), "x").unwrap();
        }
        let names: Vec<String> = trace_files(&dir).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["trace-1600000000000-12345-002.jsonl", "trace-1700000000000-9-999.jsonl", "trace-1700000000000-9-1000.jsonl"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotation_keeps_a_bounded_number_of_files() {
        let dir = temp_dir("rotate");
        let line = "x".repeat(1 << 20);
        for _ in 0..60 {
            append_rotated(&dir, &line);
        }
        let n = trace_files(&dir).len();
        assert_eq!(n, FILE_COUNT);
        let total: u64 = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.metadata().unwrap().len()).sum();
        assert!(total <= FILE_BYTES * FILE_COUNT as u64, "{total}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_reports_persist_the_last_events_and_caught_panics_keep_apart() {
        static SUB: Subsystem = Subsystem::new("test.crash");
        let dir = temp_dir("crash");
        *lock(&DIR) = Some(dir.clone());
        specula_core::trace_event!(&SUB, Error, "test.before_crash", n = 1u64);
        // At once (other tests' events would soon push it out of the last 256).
        let first = write_crash("panic", "boom", Some("here.rs:1"), Json::Null).expect("written");
        let report = Json::parse(&std::fs::read_to_string(&first).unwrap()).unwrap();
        assert_eq!(report.get("reason").and_then(Json::as_str), Some("panic"));
        let events = report.get("events").and_then(Json::as_array).unwrap();
        assert!(events.iter().any(|e| e.get("code").and_then(Json::as_str) == Some("test.before_crash")));
        let mut paths = Vec::new();
        for i in 0..(CRASH_KEPT + 3) {
            paths.push(write_crash("panic", &format!("boom {i}"), Some("here.rs:1"), Json::Null).expect("written"));
        }
        // Review probe (minor 7): caught panics pushed real crashes out.
        for i in 0..(CAUGHT_KEPT + 5) {
            write_crash("caught_panic", &format!("caught {i}"), None, Json::Null).expect("written");
        }
        let crashes = crash_files(&dir, CRASH_PREFIX);
        assert_eq!(crashes.len(), CRASH_KEPT);
        assert_eq!(crashes[0], *paths.last().unwrap(), "newest first");
        assert!(!crashes.contains(&first), "the oldest went");
        assert_eq!(crash_files(&dir, CAUGHT_PREFIX).len(), CAUGHT_KEPT);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! The engine side of headless playtests (the spec and the runner are in [`crate::playtest`]):
//! the `playtest_run` and `playtest_report` tools, running a playtest a slice at a time between
//! the engine's other work, and writing replay files.
//!
//! One playtest runs at a time per engine: it owns the clock (paused, stepped tick by tick by the
//! runner; `time_control` is refused meanwhile, except `info`). Its calls go through the same path
//! as an agent's (claims, journal, attribution), as the agent that started it. A headless engine
//! runs it flat out (about 50,000 Sepulcra ticks a second), a windowed one a few milliseconds a
//! frame, so the window stays fluid. Swarms run many playtests on many engines
//! (`scripts/playtest_swarm.py`).
//!
//! Everything here talks to the engine through [`Host`], so the bookkeeping (ownership, waiting
//! callers, replay files, bounds) is tested without a GPU; [`Engine`] is the real host.

use crate::playtest::{self, Action, Expected, Run, Spec, Status};
use crate::response::Format;
use crate::tools::{argument_error, bool_arg, check_keys, num_in, string_arg, Command, Request, TimeOp};
use crate::Engine;
use specula_core::Json;
use specula_protocol::{Server, SessionInfo, ToolDef, ToolOutput};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

/// Longest a call waits for a playtest to finish before answering with its progress (tool calls
/// time out after 30 s).
pub const MAX_WAIT_S: f32 = 20.0;
/// Finished playtests whose reports `playtest_report` still has.
const KEPT: usize = 16;
/// Calls waiting for one playtest's end at most (more are answered at once with its progress).
const MAX_WAITERS: usize = 32;
/// A call inside a playtest that isn't answered in this long fails the step.
const CALL_WAIT: Duration = Duration::from_secs(30);
/// Time a headless engine gives a playtest per loop turn (then it serves calls); a windowed one
/// gives it this much per frame.
pub const HEADLESS_SLICE: Duration = Duration::from_millis(10);
pub const WINDOW_SLICE: Duration = Duration::from_millis(4);
/// Replay files are written under this folder of the engine's working directory, and only there.
pub const RECORD_DIR: &str = "playtests";
/// Tools that write files (images, packets, traces): a playtest makes at most
/// [`playtest::MAX_CAPTURES`] such calls.
const FILE_TOOLS: [&str; 5] = ["sight_frame", "sight_orbit", "sight_compare", "perf_capture", "save_write"];

pub enum Source {
    Inline(Json),
    File(String),
    Replay(String),
}

pub enum PlaytestCall {
    Run { source: Source, record: Option<String>, overwrite: bool, wait: Duration, format: Format, allow_module_mismatch: bool },
    Report { id: Option<u64>, wait: Duration, cancel: bool, force: bool, format: Format },
}

fn wait_arg(args: &Json, default: f32) -> Result<Duration, String> {
    Ok(Duration::from_secs_f32(num_in(args, "wait_s", 0.0..=MAX_WAIT_S)?.unwrap_or(default)))
}

pub fn parse(name: &str, args: &Json) -> Option<Result<PlaytestCall, String>> {
    let call = match name {
        "playtest_run" => (|| {
            check_keys(args, name, &[&["spec", "path", "replay", "record", "overwrite", "wait_s", "response_format", "allow_module_mismatch"]])?;
            let given = ["spec", "path", "replay"].iter().filter(|k| args.get(k).is_some_and(|v| !v.is_null())).count();
            if given != 1 {
                return Err("give exactly one of 'spec' (the playtest as JSON), 'path' (a spec file) or 'replay' (a replay file to verify)".to_owned());
            }
            let source = if let Some(spec) = args.get("spec").filter(|v| !v.is_null()) {
                if !matches!(spec, Json::Obj(_)) {
                    return Err("'spec' must be the playtest as a JSON object".into());
                }
                Source::Inline(spec.clone())
            } else if let Some(p) = string_arg(args, "path")? {
                Source::File(p.to_owned())
            } else {
                Source::Replay(string_arg(args, "replay")?.ok_or("'replay' must be a file path")?.to_owned())
            };
            Ok(PlaytestCall::Run {
                source,
                record: string_arg(args, "record")?.map(str::to_owned),
                overwrite: bool_arg(args, "overwrite")?.unwrap_or(false),
                wait: wait_arg(args, MAX_WAIT_S)?,
                format: Format::arg(args, Format::Concise)?,
                allow_module_mismatch: bool_arg(args, "allow_module_mismatch")?.unwrap_or(false),
            })
        })(),
        "playtest_report" => (|| {
            check_keys(args, name, &[&["id", "wait_s", "cancel", "force", "response_format"]])?;
            Ok(PlaytestCall::Report {
                id: crate::tools::int_arg(args, "id", 1, u64::MAX >> 11)?,
                wait: wait_arg(args, 0.0)?,
                cancel: bool_arg(args, "cancel")?.unwrap_or(false),
                force: bool_arg(args, "force")?.unwrap_or(false),
                format: Format::arg(args, Format::Concise)?,
            })
        })(),
        _ => return None,
    };
    Some(call)
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let schema = |props: Vec<(&str, Json)>| {
        Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::Arr(Vec::new()))])
    };
    vec![
        ToolDef {
            name: "playtest_run".into(),
            description: "Run a headless playtest: tool calls at ticks, assertions on their answers, captures, and a replay \
                          that verifies by hash. The spec (JSON): {name, game?, steps: [{at: tick from the start, call: tool, \
                          args?, expect?: {\"dotted.path\": value | {eq|ne|lt|lte|gt|gte|in|exists|contains|near+tol}}, \
                          expect_error?: true | code, label?}], until?: {call, args?, expect, every?: ticks (30)}, ticks?: \
                          run length (default: the last step, or 36000 with until), end?: [steps without at, run once at the \
                          end], checkpoint_every?: ticks between hashes (300), stop_on_failure?, max_wall_s?}. A path's '#' \
                          is a length (\"enemies.items.#\"). The playtest owns the clock (time_control is refused while it \
                          runs) and calls as you; sight_frame steps are listed as captures (at most 64 file-writing calls). \
                          record: write a replay file (spec plus every hash, and the game modules' builds) under playtests/; \
                          replay: run one again and stop at the first hash that differs, naming the parts that differ (a \
                          replay is refused with module_build_mismatch when a module's build differs from the recording's, \
                          unless allow_module_mismatch). Answers when done or after \
                          wait_s with progress (then poll playtest_report). For identical hashes across engines start them \
                          with --paused. One playtest at a time per engine."
                .into(),
            input_schema: schema(vec![
                ("spec", t("object", "The playtest (see the description)")),
                ("path", t("string", "A spec file (JSON), relative to the engine's working directory")),
                ("replay", t("string", "A replay file to run again and verify, relative to the working directory (record's answer gives it)")),
                ("record", t("string", "Write the replay file here when the run ends: a .json name relative to playtests/")),
                ("overwrite", t("boolean", "Let record replace an existing replay file (refused otherwise)")),
                (
                    "allow_module_mismatch",
                    t("boolean", "Run a replay even though game modules' builds differ from the recording's (reported as module_builds_differ)"),
                ),
                ("wait_s", t("number", "Seconds to wait for the result, 0..20 (default 20); 0 answers at once with the id")),
                ("response_format", Format::schema()),
            ]),
        },
        ToolDef {
            name: "playtest_report".into(),
            description: "A playtest's report: status (running, passed, failed, diverged, error, cancelled), ticks, steps \
                          passed and failed, failures (part, index, tick, call, kind, path, expected, actual), captures, \
                          hashes and, for a replay, whether it verified. Without id: the running playtest, else the latest. \
                          wait_s waits for it to finish; cancel stops it (only its starter, or anyone with force: true, \
                          which is logged on the board)."
                .into(),
            input_schema: schema(vec![
                ("id", t("integer", "The playtest (from playtest_run)")),
                ("wait_s", t("number", "Seconds to wait for it to finish, 0..20 (default 0)")),
                ("cancel", t("boolean", "Stop it (the report says cancelled)")),
                ("force", t("boolean", "With cancel: stop another agent's playtest")),
                ("response_format", Format::schema()),
            ]),
        },
    ]
}

/// What a playtest needs from the engine.
pub(crate) trait Host {
    fn tick_hz(&self) -> u32;
    /// Where spec and replay files are read and replays written (under [`RECORD_DIR`]).
    fn working_dir(&self) -> Result<PathBuf, String>;
    /// The hosted game's name and its tools' names.
    fn game(&self) -> Option<(String, Vec<String>)>;
    fn engine_tools(&self) -> Vec<String>;
    /// Tools that take `response_format` themselves (the starter's detailed default is passed
    /// on to them, and only to them).
    fn native_format_tools(&self) -> Vec<String>;
    /// The hosted game's tools that write files (each counts toward the playtest's file cap).
    fn game_file_tools(&self) -> Vec<String>;
    /// A session's agent name and identity key.
    fn identity(&self, session: &SessionInfo) -> (String, String);
    fn format_default(&self, session: &SessionInfo) -> Format;
    /// Pauses the clock and drops queued steps; whether it was paused before.
    fn take_clock(&mut self) -> bool;
    fn release_clock(&mut self, was_paused: bool);
    /// Runs up to `n` ticks now; how many ran.
    fn run_ticks(&mut self, n: u64) -> u64;
    /// The hashes a checkpoint records.
    fn parts(&mut self) -> Vec<(String, u64)>;
    /// The game modules loaded, with their build hashes (a replay expects the same code).
    fn module_builds(&self) -> Vec<(String, u64)>;
    /// Hands a call to the engine as `session`; the answer comes through the receiver.
    fn dispatch(&mut self, session: &SessionInfo, tool: &str, args: &Json, game_tool: bool) -> Result<Receiver<ToolOutput>, ToolOutput>;
    fn windowed(&self) -> bool;
    /// Records something other agents should be able to see (a cancel of another's playtest).
    fn log(&mut self, by: &str, text: String);
}

/// time_control is refused while a playtest runs, except `info`.
pub fn clock_refused(op: TimeOp, running: bool) -> bool {
    running && op != TimeOp::Info
}

struct Waiter {
    reply: Sender<ToolOutput>,
    until: Instant,
    format: Format,
}

struct Active {
    id: u64,
    run: Run,
    session: SessionInfo,
    by: String,
    /// The starter's identity key: only it may cancel without force.
    key: String,
    started: Instant,
    was_paused: bool,
    /// A call answered later (a capture waiting for builds).
    waiting: Option<Pending>,
    /// Where to write the replay: (as shown, resolved).
    record: Option<(String, PathBuf)>,
    game_tools: Vec<String>,
    /// See [`Host::native_format_tools`].
    native_format: Vec<String>,
    /// Engine and game tools that write files.
    file_tools: Vec<String>,
    waiters: Vec<Waiter>,
    /// Who asked to cancel it.
    cancel: Option<String>,
    /// Calls that write files so far.
    file_calls: usize,
    /// The game modules' builds as the run began (written to the replay file).
    modules: Vec<(String, u64)>,
    /// A replay allowed to run with other module builds: how they differ.
    module_mismatch: Option<Json>,
}

/// The modules whose builds differ between a recording and this engine (`[]`: none).
fn module_mismatch(recorded: &[(String, u64)], loaded: &[(String, u64)]) -> Vec<Json> {
    let names: std::collections::BTreeSet<&String> = recorded.iter().chain(loaded).map(|(n, _)| n).collect();
    let get = |l: &[(String, u64)], n: &str| l.iter().find(|(m, _)| m == n).map(|(_, h)| *h);
    names
        .into_iter()
        .filter(|n| get(recorded, n) != get(loaded, n))
        .map(|n| {
            Json::obj([
                ("module", Json::from(n.as_str())),
                ("recorded", Json::from(get(recorded, n).map(crate::journal::hex))),
                ("loaded", Json::from(get(loaded, n).map(crate::journal::hex))),
            ])
        })
        .collect()
}

/// The modules' builds as a replay file lists them.
fn modules_json(modules: &[(String, u64)]) -> Json {
    Json::arr(modules.iter().map(|(n, h)| Json::obj([("module", Json::from(n.as_str())), ("build_hash", Json::from(crate::journal::hex(*h)))])))
}

/// A call's answer still to come: the receiver, when to give up, and the answer's shape.
struct Pending {
    rx: Receiver<ToolOutput>,
    deadline: Instant,
    tool: String,
    format: Option<Format>,
}

impl Pending {
    /// As an agent gets it over MCP (shaped, compact, errors coded).
    fn answer(&self, out: ToolOutput) -> ToolOutput {
        crate::response::tidy(match self.format {
            Some(f) => crate::response::shape(&self.tool, f, out),
            None => out,
        })
    }
}

struct Finished {
    id: u64,
    concise: Json,
    detailed: Json,
}

/// The engine's playtests: the running one and the last few reports.
#[derive(Default)]
pub(crate) struct Playtests {
    next_id: u64,
    active: Option<Active>,
    done: VecDeque<Finished>,
}

fn error(code: &str, message: impl Into<String>, details: Vec<(&str, Json)>) -> ToolOutput {
    let mut pairs = vec![("error", Json::from(code)), ("message", Json::from(message.into()))];
    pairs.extend(details);
    ToolOutput::error(Json::obj(pairs).to_string())
}

/// time_control while a playtest runs.
pub fn clock_busy() -> ToolOutput {
    error("playtest_running", "a playtest owns the clock until it ends: wait with playtest_report {wait_s}, or cancel it", vec![])
}

fn with_id(id: u64, report: Json, extra: Vec<(&str, Json)>) -> Json {
    let mut pairs = vec![("id".to_owned(), Json::from(id))];
    if let Json::Obj(rest) = report {
        pairs.extend(rest);
    }
    pairs.extend(extra.into_iter().map(|(k, v)| (k.to_owned(), v)));
    Json::Obj(pairs)
}

impl Active {
    fn report(&self, format: Format, replay_file: Option<&Json>) -> Json {
        let wall = self.started.elapsed().as_secs_f64();
        let mut extra = vec![
            ("wall_ms", Json::from((wall * 1000.0).round())),
            ("ticks_per_s", Json::from(if wall > 0.0 { (self.run.tick as f64 / wall).round() } else { 0.0 })),
        ];
        if format == Format::Detailed {
            extra.push(("started_by", Json::from(self.by.as_str())));
        }
        if let Some(m) = &self.module_mismatch {
            extra.push(("module_builds_differ", m.clone()));
        }
        if let Some(f) = replay_file {
            extra.push(("replay_file", f.clone()));
        } else if let Some((rel, _)) = &self.record {
            extra.push(("replay_file", Json::obj([("path", Json::from(rel.as_str())), ("written", Json::from(false))])));
        }
        with_id(self.id, self.run.report(format == Format::Detailed), extra)
    }

    /// Answers now (`wait` zero, or too many callers waiting already) or when the playtest ends
    /// or `wait` runs out.
    fn wait_or_answer(&mut self, reply: Sender<ToolOutput>, wait: Duration, format: Format) {
        if wait.is_zero() || self.waiters.len() >= MAX_WAITERS {
            let _ = reply.send(ToolOutput::text(self.report(format, None).to_string()));
        } else {
            self.waiters.push(Waiter { reply, until: Instant::now() + wait, format });
        }
    }
}

impl Playtests {
    pub fn running(&self) -> bool {
        self.active.is_some()
    }

    /// The running playtest waits for a deferred answer (no need to spin).
    pub fn waiting_on_call(&self) -> bool {
        self.active.as_ref().is_some_and(|a| a.waiting.is_some())
    }

    /// playtest_run and playtest_report (answered now or once the playtest ends).
    pub(crate) fn call(&mut self, host: &mut dyn Host, call: PlaytestCall, session: &SessionInfo, reply: Sender<ToolOutput>) {
        match call {
            PlaytestCall::Run { source, record, overwrite, wait, format, allow_module_mismatch } => match self.start(host, source, record, overwrite, allow_module_mismatch, session) {
                Ok(()) => self.active.as_mut().expect("just started").wait_or_answer(reply, wait, format),
                Err(e) => {
                    let _ = reply.send(e);
                }
            },
            PlaytestCall::Report { id, wait, cancel, force, format } => {
                if let Some(a) = self.active.as_mut().filter(|a| id.is_none_or(|i| i == a.id)) {
                    if cancel {
                        let (by, key) = host.identity(session);
                        if key != a.key && !force {
                            let msg = format!("playtest {} is {}'s; only they cancel it (or pass force: true, which is logged)", a.id, a.by);
                            let _ = reply.send(error("not_yours", msg, vec![("id", Json::from(a.id)), ("started_by", Json::from(a.by.as_str()))]));
                            return;
                        }
                        if key != a.key {
                            host.log(&by, format!("cancelled playtest {} '{}' of {} (force)", a.id, a.run.spec.name, a.by));
                        }
                        a.cancel.get_or_insert(by);
                    }
                    // A cancel answers with the final report, at the next slice.
                    let wait = if cancel { wait.max(Duration::from_secs(5)) } else { wait };
                    a.wait_or_answer(reply, wait, format);
                    return;
                }
                let found = match id {
                    Some(i) => self.done.iter().find(|f| f.id == i),
                    None => self.done.back(),
                };
                let out = match found {
                    Some(f) => ToolOutput::text(if format == Format::Detailed { f.detailed.to_string() } else { f.concise.to_string() }),
                    None => {
                        let kept: Vec<Json> = self.done.iter().map(|f| Json::from(f.id)).collect();
                        let msg = match id {
                            Some(i) => format!("no playtest {i}: this engine keeps the last {KEPT} reports"),
                            None => "no playtest has run in this engine yet: start one with playtest_run".to_owned(),
                        };
                        error("not_found", msg, vec![("kept", Json::Arr(kept))])
                    }
                };
                let _ = reply.send(out);
            }
        }
    }

    fn start(&mut self, host: &mut dyn Host, source: Source, record: Option<String>, overwrite: bool, allow_module_mismatch: bool, session: &SessionInfo) -> Result<(), ToolOutput> {
        if let Some(a) = &self.active {
            return Err(error(
                "playtest_busy",
                format!("playtest {} ('{}') is running; one at a time per engine (run more engines for more at once)", a.id, a.run.spec.name),
                vec![("id", Json::from(a.id)), ("progress", a.run.progress())],
            ));
        }
        let root = host.working_dir().map_err(|e| error("io", format!("the working directory: {e}"), vec![]))?;
        let bad = |e: playtest::SpecError| ToolOutput::error(e.to_json().to_string());
        let io = |field: &str, e: String| error("bad_path", e, vec![("field", Json::from(field))]);
        let (spec, expected): (Spec, Option<Expected>) = match source {
            Source::Inline(v) => (Spec::parse(&v).map_err(bad)?, None),
            Source::File(rel) => {
                let (_, text) = playtest::read_file(&root, &rel).map_err(|e| io("path", e))?;
                let v = Json::parse(&text).map_err(|e| error("bad_spec", format!("{rel} is not JSON: {e}"), vec![("field", Json::from("path"))]))?;
                (Spec::parse(&v).map_err(bad)?, None)
            }
            Source::Replay(rel) => {
                let (_, text) = playtest::read_file(&root, &rel).map_err(|e| io("replay", e))?;
                let (spec, exp) = playtest::parse_replay(&text, &rel, host.tick_hz()).map_err(bad)?;
                (spec, Some(exp))
            }
        };
        let game = host.game();
        if let Some(g) = &spec.game {
            if game.as_ref().is_none_or(|(name, _)| name != g) {
                let hosted = game.as_ref().map_or(Json::Null, |(n, _)| Json::from(n.as_str()));
                return Err(error("wrong_game", format!("the spec is for the game '{g}'"), vec![("hosted", hosted)]));
            }
        }
        // Code is part of the input (DETERMINISM.md rule 8): a replay recorded with other module
        // builds isn't expected to match, so it is refused unless asked for.
        let modules = host.module_builds();
        let mut mismatch = None;
        if let Some(recorded) = expected.as_ref().and_then(|e| e.modules.as_deref()) {
            let differ = module_mismatch(recorded, &modules);
            if !differ.is_empty() {
                if !allow_module_mismatch {
                    let names: Vec<String> = differ.iter().filter_map(|d| d.get("module").and_then(Json::as_str).map(str::to_owned)).collect();
                    return Err(error(
                        "module_build_mismatch",
                        format!(
                            "the replay was recorded with other builds of module(s) {}: load the recorded builds (logic_reload), or pass \
                             allow_module_mismatch: true to run it anyway (its hashes are then not expected to match)",
                            names.join(", ")
                        ),
                        vec![("modules", Json::Arr(differ))],
                    ));
                }
                mismatch = Some(Json::Arr(differ));
            }
        }
        let game_tools = game.map(|(_, t)| t).unwrap_or_default();
        let engine_tools = host.engine_tools();
        if let Some(unknown) = spec.calls().find(|c| !game_tools.iter().any(|t| t == c) && !engine_tools.iter().any(|t| t == c)) {
            return Err(error("unknown_tool", format!("the spec calls '{unknown}', which this engine doesn't have (tools/list shows them)"), vec![("call", Json::from(unknown))]));
        }
        let record = match record {
            Some(rel) => {
                // Only .json files under playtests/, and never over an existing one unasked.
                if !rel.to_ascii_lowercase().ends_with(".json") {
                    return Err(io("record", format!("'{rel}': a replay file's name ends in .json")));
                }
                let shown = format!("{RECORD_DIR}/{rel}");
                let path = playtest::safe_path(&root, &shown, true).map_err(|e| io("record", e))?;
                // safe_path keeps it inside the working directory; a junction or link inside
                // playtests/ must not carry it anywhere else in there.
                let confined = root.canonicalize().map(|r| r.join(RECORD_DIR)).ok().is_some_and(|dir| path.parent().is_some_and(|p| p.starts_with(&dir)));
                if !confined {
                    return Err(io("record", format!("'{shown}' resolves outside {RECORD_DIR}/ (through a link or junction)")));
                }
                if path.exists() && !overwrite {
                    return Err(error("exists", format!("{shown} already exists; pass overwrite: true to replace it"), vec![("path", Json::from(shown.as_str()))]));
                }
                Some((shown, path))
            }
            None => None,
        };
        self.next_id += 1;
        let id = self.next_id;
        let was_paused = host.take_clock();
        let (by, key) = host.identity(session);
        eprintln!("specula: playtest {id} '{}' started by {by}{}", spec.name, if expected.is_some() { " (replay)" } else { "" });
        self.active = Some(Active {
            id,
            run: Run::new(spec, expected),
            session: session.clone(),
            by,
            key,
            started: Instant::now(),
            was_paused,
            waiting: None,
            record,
            game_tools,
            native_format: host.native_format_tools(),
            file_tools: FILE_TOOLS.iter().map(|t| (*t).to_owned()).chain(host.game_file_tools()).collect(),
            waiters: Vec::new(),
            cancel: None,
            file_calls: 0,
            modules,
            module_mismatch: mismatch,
        });
        Ok(())
    }

    /// Runs the active playtest for about `budget`.
    pub(crate) fn pump(&mut self, host: &mut dyn Host, budget: Duration) {
        let Some(mut a) = self.active.take() else { return };
        let end = Instant::now() + budget;
        // Fewer ticks between time checks with a window (its frames come first).
        let chunk = if host.windowed() { 16 } else { 64 };
        loop {
            if let Some(by) = &a.cancel {
                a.run.stop(Status::Cancelled, format!("cancelled by {by}"));
            }
            if a.started.elapsed() > Duration::from_secs(a.run.spec.max_wall_s) {
                a.run.stop(Status::Error, format!("ran past max_wall_s ({} s)", a.run.spec.max_wall_s));
            }
            if a.run.status().is_some() {
                // A stopped run doesn't wait for an answer (it arrives in a closed channel).
                a.waiting = None;
            }
            if let Some(p) = &a.waiting {
                match p.rx.try_recv() {
                    Ok(out) => a.run.complete(&p.answer(out)),
                    Err(TryRecvError::Empty) if Instant::now() < p.deadline => break,
                    Err(_) => a.run.complete(&error("no_answer", format!("the call wasn't answered within {} s", CALL_WAIT.as_secs()), vec![])),
                }
                a.waiting = None;
            }
            match a.run.next_action() {
                Action::Call(why) => {
                    let (tool, args) = a.run.call_of(why);
                    let (tool, args) = (tool.to_owned(), args.clone());
                    match Self::dispatch(host, &mut a, &tool, &args) {
                        Ok(p) => match p.rx.try_recv() {
                            Ok(out) => a.run.complete(&p.answer(out)),
                            Err(TryRecvError::Empty) => a.waiting = Some(p),
                            Err(TryRecvError::Disconnected) => a.run.complete(&error("no_answer", "the call was dropped unanswered", vec![])),
                        },
                        Err(out) => a.run.complete(&out),
                    }
                }
                Action::Hash(point) => {
                    let parts = host.parts();
                    a.run.record_hash(point, parts);
                }
                Action::Advance(n) => {
                    let ran = host.run_ticks(n.min(chunk));
                    if ran == 0 {
                        a.run.stop(Status::Error, "the clock didn't advance");
                    }
                    a.run.advanced(ran);
                }
                Action::Wait => break,
                Action::Finish => {
                    self.finish(host, a);
                    return;
                }
            }
            if Instant::now() >= end {
                break;
            }
        }
        let now = Instant::now();
        let mut keep = Vec::new();
        for w in std::mem::take(&mut a.waiters) {
            if now >= w.until {
                let _ = w.reply.send(ToolOutput::text(a.report(w.format, None).to_string()));
            } else {
                keep.push(w);
            }
        }
        a.waiters = keep;
        self.active = Some(a);
    }

    /// A playtest's call, as its agent: answered at once or later through the receiver.
    fn dispatch(host: &mut dyn Host, a: &mut Active, tool: &str, args: &Json) -> Result<Pending, ToolOutput> {
        if let Some(why) = playtest::forbidden(tool) {
            return Err(error("forbidden", format!("a playtest can't call {tool}: {why}"), vec![]));
        }
        if a.file_tools.iter().any(|t| t == tool) {
            if a.file_calls >= playtest::MAX_CAPTURES {
                let msg = format!("a playtest makes at most {} calls that write files (captures, traces)", playtest::MAX_CAPTURES);
                return Err(error("capture_limit", msg, vec![("max", Json::from(playtest::MAX_CAPTURES))]));
            }
            a.file_calls += 1;
        }
        // As over MCP: the starter's default format, response_format of the heavy tools applied
        // to the answer.
        let default = host.format_default(&a.session);
        let shaped = crate::response::split(tool, args, default).map_err(|e| ToolOutput::error(argument_error(&e).to_string()))?;
        let format = shaped.as_ref().map(|(f, _)| *f);
        let native = shaped.is_none() && a.native_format.iter().any(|t| t == tool);
        let defaulted = if native { crate::response::with_default(args, default) } else { None };
        let args = shaped.as_ref().map(|(_, a)| a).or(defaulted.as_ref()).unwrap_or(args);
        let game_tool = a.game_tools.iter().any(|t| t == tool);
        let rx = host.dispatch(&a.session, tool, args, game_tool)?;
        Ok(Pending { rx, deadline: Instant::now() + CALL_WAIT, tool: tool.to_owned(), format })
    }

    fn finish(&mut self, host: &mut dyn Host, mut a: Active) {
        host.release_clock(a.was_paused);
        let replay_file = a.record.as_ref().map(|(rel, path)| {
            let game = host.game().map(|(n, _)| n);
            let mut replay = a.run.replay_json(game.as_deref(), host.tick_hz());
            replay.set("modules", modules_json(&a.modules));
            let text = replay.to_pretty();
            match std::fs::write(path, &text) {
                Ok(()) => Json::obj([("path", Json::from(rel.as_str())), ("written", Json::from(true)), ("bytes", Json::from(text.len()))]),
                Err(e) => Json::obj([("path", Json::from(rel.as_str())), ("written", Json::from(false)), ("error", Json::from(e.to_string()))]),
            }
        });
        let concise = a.report(Format::Concise, replay_file.as_ref());
        let detailed = a.report(Format::Detailed, replay_file.as_ref());
        for w in a.waiters.drain(..) {
            let r = if w.format == Format::Detailed { &detailed } else { &concise };
            let _ = w.reply.send(ToolOutput::text(r.to_string()));
        }
        let status = a.run.status().map_or("running", Status::name);
        eprintln!("specula: playtest {} '{}' {status} after {} ticks in {:.2} s", a.id, a.run.spec.name, a.run.tick, a.started.elapsed().as_secs_f64());
        if self.done.len() == KEPT {
            self.done.pop_front();
        }
        self.done.push_back(Finished { id: a.id, concise, detailed });
    }
}

/// The engine as a playtest's host.
struct EngineHost<'a> {
    e: &'a mut Engine,
    server: &'a Server,
}

impl Host for EngineHost<'_> {
    fn tick_hz(&self) -> u32 {
        self.e.clock.hz()
    }

    fn working_dir(&self) -> Result<PathBuf, String> {
        std::env::current_dir().map_err(|e| e.to_string())
    }

    fn game(&self) -> Option<(String, Vec<String>)> {
        self.e.game.as_ref().map(|g| (self.e.game_name.clone(), g.tools().into_iter().map(|t| t.name).collect()))
    }

    fn engine_tools(&self) -> Vec<String> {
        crate::tools::engine_tool_names()
    }

    fn native_format_tools(&self) -> Vec<String> {
        let game = self.e.game.as_ref().map(|g| g.tools()).unwrap_or_default();
        crate::tools::engine_definitions().iter().chain(&game).filter(|d| crate::response::takes_format(d)).map(|d| d.name.clone()).collect()
    }

    fn game_file_tools(&self) -> Vec<String> {
        self.e.game.as_ref().map(|g| g.file_tools().iter().map(|t| (*t).to_owned()).collect()).unwrap_or_default()
    }

    fn identity(&self, session: &SessionInfo) -> (String, String) {
        (self.e.swarm.agent_name(session), self.e.swarm.key(session))
    }

    fn format_default(&self, session: &SessionInfo) -> Format {
        self.e.formats.get(&session.id)
    }

    fn take_clock(&mut self) -> bool {
        let was = self.e.clock.paused();
        self.e.clock.set_paused(true);
        // Ticks an earlier time_control step queued would run on into the playtest.
        self.e.clock.cancel_steps();
        was
    }

    fn release_clock(&mut self, was_paused: bool) {
        if !was_paused {
            self.e.clock.set_paused(false);
            self.e.last_advance = Instant::now();
        }
    }

    fn run_ticks(&mut self, n: u64) -> u64 {
        let before = self.e.clock.tick();
        self.e.clock.step(n);
        self.e.advance_clock();
        self.e.clock.tick() - before
    }

    fn parts(&mut self) -> Vec<(String, u64)> {
        let mut parts = vec![("world".to_owned(), self.e.state_hash())];
        if let Some(h) = self.e.physics.hash() {
            parts.push(("physics".to_owned(), h));
        }
        if let Some(g) = &self.e.game {
            let mut hashes = Vec::new();
            g.state_hashes(&mut hashes);
            parts.extend(hashes.into_iter().map(|(k, h)| (format!("game.{k}"), h)));
        }
        // Module state the engine keeps outside the world (stores, RNG positions, events), as
        // the desync detector hashes it.
        if let Some(h) = self.e.logic.state_hash() {
            parts.push(("logic".to_owned(), h));
        }
        parts
    }

    fn module_builds(&self) -> Vec<(String, u64)> {
        self.e.logic.builds()
    }

    fn dispatch(&mut self, session: &SessionInfo, tool: &str, args: &Json, game_tool: bool) -> Result<Receiver<ToolOutput>, ToolOutput> {
        let command = if game_tool {
            Command::Game { name: tool.to_owned(), args: args.clone() }
        } else {
            crate::tools::parse(tool, args).map_err(|e| ToolOutput::error(argument_error(&e).to_string()))?
        };
        let (reply, answer) = mpsc::channel();
        let request = Request { session: session.clone(), command, reply, deadline: Instant::now() + Duration::from_secs(25) };
        self.e.handle(request, self.server);
        Ok(answer)
    }

    fn windowed(&self) -> bool {
        self.e.windowed
    }

    fn log(&mut self, by: &str, text: String) {
        let tick = self.e.clock.tick();
        self.e.swarm.board.log(by, tick, "playtest", None, text);
    }
}

impl Engine {
    /// playtest_run and playtest_report.
    pub(crate) fn playtest_call(&mut self, call: PlaytestCall, session: SessionInfo, reply: Sender<ToolOutput>, server: &Server) {
        let mut p = std::mem::take(&mut self.playtests);
        p.call(&mut EngineHost { e: self, server }, call, &session, reply);
        self.playtests = p;
    }

    /// Runs the active playtest for about `budget`.
    pub(crate) fn pump_playtests(&mut self, server: &Server, budget: Duration) {
        if !self.playtests.running() {
            return;
        }
        let mut p = std::mem::take(&mut self.playtests);
        p.pump(&mut EngineHost { e: self, server }, budget);
        self.playtests = p;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host without an engine: tools answer from a table, the clock is a counter.
    struct Fake {
        root: PathBuf,
        tick: u64,
        paused: bool,
        calls: Vec<String>,
        logs: Vec<String>,
        /// Senders of answers that never come (kept so the receivers stay open).
        held: Vec<Sender<ToolOutput>>,
        /// The starter's default format.
        format: Format,
        /// Every call's arguments, as dispatched.
        args: Vec<Json>,
        /// The modules "loaded".
        modules: Vec<(String, u64)>,
    }

    impl Fake {
        fn new(label: &str) -> Fake {
            let root = std::env::temp_dir().join(format!("specula-playtest-engine-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Fake { root, tick: 0, paused: false, calls: Vec::new(), logs: Vec::new(), held: Vec::new(), format: Format::Concise, args: Vec::new(), modules: Vec::new() }
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    impl Host for Fake {
        fn tick_hz(&self) -> u32 {
            30
        }
        fn working_dir(&self) -> Result<PathBuf, String> {
            Ok(self.root.clone())
        }
        fn game(&self) -> Option<(String, Vec<String>)> {
            Some(("g".into(), vec!["play_state".into(), "play_record".into()]))
        }
        fn engine_tools(&self) -> Vec<String> {
            ["world_query", "world_spawn", "sight_frame", "time_control", "never", "logic_status"].map(String::from).to_vec()
        }
        fn native_format_tools(&self) -> Vec<String> {
            vec!["play_state".into(), "logic_status".into()]
        }
        fn game_file_tools(&self) -> Vec<String> {
            vec!["play_record".into()]
        }
        fn identity(&self, s: &SessionInfo) -> (String, String) {
            (format!("agent-{}", s.id), format!("key-{}", s.id))
        }
        fn format_default(&self, _: &SessionInfo) -> Format {
            self.format
        }
        fn take_clock(&mut self) -> bool {
            std::mem::replace(&mut self.paused, true)
        }
        fn release_clock(&mut self, was: bool) {
            self.paused = was;
        }
        fn run_ticks(&mut self, n: u64) -> u64 {
            self.tick += n;
            n
        }
        fn parts(&mut self) -> Vec<(String, u64)> {
            vec![("world".into(), self.tick)]
        }
        fn module_builds(&self) -> Vec<(String, u64)> {
            self.modules.clone()
        }
        fn dispatch(&mut self, _: &SessionInfo, tool: &str, args: &Json, _: bool) -> Result<Receiver<ToolOutput>, ToolOutput> {
            self.calls.push(tool.to_owned());
            self.args.push(args.clone());
            let (tx, rx) = mpsc::channel();
            match tool {
                // As the engine's parsers do: an argument the tool doesn't take is an error.
                "world_spawn" if args.get("response_format").is_some() => {
                    tx.send(ToolOutput::error(r#"{"error": "invalid_argument", "message": "'response_format' is not an argument of world_spawn"}"#)).unwrap()
                }
                "sight_frame" => tx.send(ToolOutput::text(r#"{"image": {"path": "f.png"}}"#)).unwrap(),
                "never" => self.held.push(tx),
                _ => tx.send(ToolOutput::text(args.to_string())).unwrap(),
            }
            Ok(rx)
        }
        fn windowed(&self) -> bool {
            false
        }
        fn log(&mut self, by: &str, text: String) {
            self.logs.push(format!("{by}: {text}"));
        }
    }

    fn session(id: &str) -> SessionInfo {
        SessionInfo { id: id.into(), client_name: "t".into(), client_version: "0".into(), protocol_version: "x".into() }
    }

    fn run_call(spec: &str, record: Option<&str>, overwrite: bool) -> PlaytestCall {
        PlaytestCall::Run {
            source: Source::Inline(Json::parse(spec).unwrap()),
            record: record.map(String::from),
            overwrite,
            wait: Duration::ZERO,
            format: Format::Concise,
            allow_module_mismatch: false,
        }
    }

    fn report_call(cancel: bool, force: bool, wait: Duration) -> PlaytestCall {
        PlaytestCall::Report { id: None, wait, cancel, force, format: Format::Concise }
    }

    /// Calls and returns the answer, if there is one yet.
    fn ask(p: &mut Playtests, h: &mut Fake, call: PlaytestCall, s: &SessionInfo) -> (Receiver<ToolOutput>, Option<Json>) {
        let (tx, rx) = mpsc::channel();
        p.call(h, call, s, tx);
        let now = rx.try_recv().ok().map(|o| Json::parse(match &o.content[0] {
            specula_protocol::Content::Text(t) => t,
            _ => "null",
        }).unwrap());
        (rx, now)
    }

    fn finish(p: &mut Playtests, h: &mut Fake) {
        for _ in 0..10_000 {
            if !p.running() {
                return;
            }
            p.pump(h, Duration::from_millis(50));
        }
        panic!("never finished");
    }

    fn code(j: &Json) -> Option<&str> {
        j.get("error").and_then(Json::as_str)
    }

    #[test]
    fn the_clock_is_the_playtests_while_it_runs() {
        assert!(clock_refused(TimeOp::Step(1), true));
        assert!(clock_refused(TimeOp::Resume, true));
        assert!(!clock_refused(TimeOp::Info, true), "info stays open");
        assert!(!clock_refused(TimeOp::Step(1), false));
        let (mut p, mut h, a) = (Playtests::default(), Fake::new("clock"), session("a"));
        ask(&mut p, &mut h, run_call(r#"{"name": "c", "ticks": 100}"#, None, false), &a);
        assert!(h.paused, "taken over");
        finish(&mut p, &mut h);
        assert_eq!((h.tick, h.paused), (100, false), "given back as it was");
    }

    #[test]
    fn replays_are_written_under_playtests_and_never_overwritten_unasked() {
        let (mut p, mut h, a) = (Playtests::default(), Fake::new("record"), session("a"));
        let spec = r#"{"name": "r", "steps": [{"at": 0, "call": "world_query"}], "ticks": 600}"#;
        let (_, ans) = ask(&mut p, &mut h, run_call(spec, Some("../AGENTS.md"), false), &a);
        assert_eq!(code(&ans.unwrap()), Some("bad_path"), "not a .json name");
        let (_, ans) = ask(&mut p, &mut h, run_call(spec, Some("../x.json"), false), &a);
        assert_eq!(code(&ans.unwrap()), Some("bad_path"), "out of playtests/");
        ask(&mut p, &mut h, run_call(spec, Some("r.json"), false), &a);
        finish(&mut p, &mut h);
        let file = h.root.join(RECORD_DIR).join("r.json");
        assert!(file.exists());
        let (_, ans) = ask(&mut p, &mut h, run_call(spec, Some("r.json"), false), &a);
        assert_eq!(code(&ans.unwrap()), Some("exists"));
        h.tick = 0;
        let (_, ans) = ask(&mut p, &mut h, run_call(spec, Some("r.json"), true), &a);
        assert!(code(ans.as_ref().unwrap()).is_none(), "overwrite: true");
        finish(&mut p, &mut h);
        // The replay runs again and verifies (the fake's hash is the tick since the start).
        h.tick = 0;
        let replay = |allow| PlaytestCall::Run {
            source: Source::Replay("playtests/r.json".into()),
            record: None,
            overwrite: false,
            wait: Duration::ZERO,
            format: Format::Concise,
            allow_module_mismatch: allow,
        };
        ask(&mut p, &mut h, replay(false), &a);
        finish(&mut p, &mut h);
        let (_, r) = ask(&mut p, &mut h, report_call(false, false, Duration::ZERO), &a);
        assert_eq!(r.unwrap().get("replay").and_then(|r| r.get("verified")).and_then(Json::as_bool), Some(true));
        // Recorded with no modules; with one loaded now, the code differs: refused, by name.
        h.modules = vec![("ambulo".into(), 0xabc)];
        h.tick = 0;
        let (_, ans) = ask(&mut p, &mut h, replay(false), &a);
        let ans = ans.expect("refused at once");
        assert_eq!(code(&ans), Some("module_build_mismatch"), "{ans}");
        let m = &ans.get("modules").and_then(Json::as_array).expect("named")[0];
        assert_eq!((m.get("module").and_then(Json::as_str), m.get("recorded")), (Some("ambulo"), Some(&Json::Null)));
        assert_eq!(m.get("loaded").and_then(Json::as_str), Some("0000000000000abc"));
        // Asked for: it runs, and the report says the builds differ.
        ask(&mut p, &mut h, replay(true), &a);
        finish(&mut p, &mut h);
        let (_, r) = ask(&mut p, &mut h, report_call(false, false, Duration::ZERO), &a);
        assert_eq!(r.unwrap().get("module_builds_differ").and_then(Json::as_array).map(<[Json]>::len), Some(1));
    }

    #[test]
    fn only_the_starter_cancels_unless_forced_and_logged() {
        let (mut p, mut h, a, b) = (Playtests::default(), Fake::new("cancel"), session("a"), session("b"));
        ask(&mut p, &mut h, run_call(r#"{"name": "c", "ticks": 1000000}"#, None, false), &a);
        let (_, ans) = ask(&mut p, &mut h, report_call(true, false, Duration::ZERO), &b);
        assert_eq!(code(&ans.unwrap()), Some("not_yours"));
        p.pump(&mut h, Duration::from_millis(1));
        assert!(p.running(), "still running");
        let (rx, _) = ask(&mut p, &mut h, report_call(true, true, Duration::ZERO), &b);
        assert_eq!(h.logs.len(), 1, "a forced cancel is logged: {:?}", h.logs);
        finish(&mut p, &mut h);
        let out = rx.try_recv().expect("answered at the end");
        assert!(format!("{:?}", out.content).contains("cancelled by agent-b"));
        // The starter needs no force.
        ask(&mut p, &mut h, run_call(r#"{"name": "c", "ticks": 1000000}"#, None, false), &a);
        ask(&mut p, &mut h, report_call(true, false, Duration::ZERO), &a);
        finish(&mut p, &mut h);
    }

    #[test]
    fn a_detailed_default_reaches_only_tools_that_take_a_format() {
        let (mut p, mut h, a) = (Playtests::default(), Fake::new("detailed"), session("a"));
        h.format = Format::Detailed;
        let spec = r#"{"name": "d", "steps": [{"at": 0, "call": "world_spawn", "args": {"shape": "cube"}}, {"at": 0, "call": "play_state"},
            {"at": 0, "call": "logic_status"}, {"at": 0, "call": "world_query"}]}"#;
        ask(&mut p, &mut h, run_call(spec, None, false), &a);
        finish(&mut p, &mut h);
        let (_, r) = ask(&mut p, &mut h, report_call(false, false, Duration::ZERO), &a);
        assert_eq!(r.unwrap().get("status").and_then(Json::as_str), Some("passed"), "world_spawn took no response_format");
        let fmt = |i: usize| h.args[i].get("response_format").and_then(Json::as_str).map(str::to_owned);
        assert_eq!((fmt(0), fmt(1), fmt(2)), (None, Some("detailed".into()), Some("detailed".into())));
        assert_eq!(h.args[3].get("fields").map(Json::to_string).as_deref(), Some(r#"["all"]"#), "shaped: its own knob");
    }

    #[test]
    fn game_tools_that_write_files_count_toward_the_cap() {
        let (mut p, mut h, a) = (Playtests::default(), Fake::new("gamefiles"), session("a"));
        let spec = r#"{"name": "rec", "until": {"call": "play_record", "every": 1, "expect": {"never": true}}, "ticks": 300}"#;
        ask(&mut p, &mut h, run_call(spec, None, false), &a);
        finish(&mut p, &mut h);
        assert_eq!(h.calls.iter().filter(|c| *c == "play_record").count(), playtest::MAX_CAPTURES);
    }

    #[cfg(windows)]
    #[test]
    fn replays_stay_in_playtests_through_junctions() {
        let (mut p, mut h, a) = (Playtests::default(), Fake::new("junction"), session("a"));
        std::fs::create_dir_all(h.root.join(RECORD_DIR)).unwrap();
        let link = h.root.join(RECORD_DIR).join("jroot");
        let made = std::process::Command::new("cmd").args(["/C", "mklink", "/J", link.to_str().unwrap(), h.root.to_str().unwrap()]).output();
        if !made.as_ref().is_ok_and(|o| o.status.success()) {
            eprintln!("skipped: could not create a junction ({made:?})");
            return;
        }
        let (_, ans) = ask(&mut p, &mut h, run_call(r#"{"name": "j", "ticks": 1}"#, Some("jroot/escaped.json"), false), &a);
        let _ = std::fs::remove_dir(&link);
        assert_eq!(code(&ans.unwrap()), Some("bad_path"));
        assert!(!h.root.join("escaped.json").exists());
    }

    #[test]
    fn waiting_callers_are_capped() {
        let (mut p, mut h, a) = (Playtests::default(), Fake::new("waiters"), session("a"));
        ask(&mut p, &mut h, run_call(r#"{"name": "w", "steps": [{"at": 0, "call": "never"}], "ticks": 10}"#, None, false), &a);
        let answered_now = (0..MAX_WAITERS + 8).filter(|_| ask(&mut p, &mut h, report_call(false, false, Duration::from_secs(10)), &a).1.is_some()).count();
        assert_eq!(answered_now, 8, "past {MAX_WAITERS} waiting callers, the rest are answered at once");
        assert_eq!(p.active.as_ref().unwrap().waiters.len(), MAX_WAITERS);
    }

    #[test]
    fn forbidden_tools_and_too_many_captures_are_refused() {
        let (mut p, mut h, a) = (Playtests::default(), Fake::new("forbidden"), session("a"));
        let (_, ans) = ask(&mut p, &mut h, run_call(r#"{"name": "f", "steps": [{"at": 0, "call": "time_control"}]}"#, None, false), &a);
        assert_eq!(code(&ans.unwrap()), Some("bad_spec"));
        let (_, ans) = ask(&mut p, &mut h, run_call(r#"{"name": "f", "steps": [{"at": 0, "call": "nope"}]}"#, None, false), &a);
        assert_eq!(code(&ans.unwrap()), Some("unknown_tool"));
        let (_, ans) = ask(&mut p, &mut h, run_call(r#"{"name": "f", "game": "other"}"#, None, false), &a);
        assert_eq!(code(&ans.unwrap()), Some("wrong_game"));
        let shots: Vec<String> = (0..playtest::MAX_CAPTURES + 6).map(|i| format!(r#"{{"at": {i}, "call": "sight_frame"}}"#)).collect();
        let spec = format!(r#"{{"name": "shots", "steps": [{}]}}"#, shots.join(","));
        ask(&mut p, &mut h, run_call(&spec, None, false), &a);
        finish(&mut p, &mut h);
        assert_eq!(h.calls.iter().filter(|c| *c == "sight_frame").count(), playtest::MAX_CAPTURES, "the engine never saw the rest");
        let (_, r) = ask(&mut p, &mut h, report_call(false, false, Duration::ZERO), &a);
        let r = r.unwrap();
        assert_eq!(r.get("failures_total").and_then(Json::as_u64), Some(6));
        assert!(r.to_string().contains("capture_limit"));
    }

    #[test]
    fn reports_stay_within_their_byte_budget() {
        let (mut p, mut h, a) = (Playtests::default(), Fake::new("budget"), session("a"));
        // 48 failing steps (a spec near its byte cap), each quoting a 2 KB matcher and a big actual value.
        let big = "x".repeat(playtest::MAX_MATCHER_BYTES - 16);
        let steps: Vec<String> = (0..48).map(|i| format!(r#"{{"at": {i}, "call": "world_query", "args": {{"pad": "{big}"}}, "expect": {{"pad": "{big}y"}}}}"#)).collect();
        let spec = format!(r#"{{"name": "big", "steps": [{}]}}"#, steps.join(","));
        ask(&mut p, &mut h, run_call(&spec, None, false), &a);
        finish(&mut p, &mut h);
        let f = p.done.back().unwrap();
        assert!(f.concise.to_string().len() < 16 * 1024, "{}", f.concise.to_string().len());
        assert!(f.detailed.to_string().len() <= playtest::MAX_REPORT_BYTES + 4096, "{}", f.detailed.to_string().len());
        assert_eq!(f.detailed.get("failures_total").and_then(Json::as_u64), Some(48));
    }
}

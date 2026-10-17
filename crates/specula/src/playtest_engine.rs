//! The engine side of headless playtests (the spec and the runner are in [`crate::playtest`]):
//! the `playtest_run` and `playtest_report` tools, running a playtest a slice at a time between
//! the engine's other work, and writing replay files.
//!
//! One playtest runs at a time per engine: it owns the clock (paused, stepped tick by tick by the
//! runner; `time_control` is refused meanwhile, except `info`). Its calls go through the same path
//! as an agent's (claims, journal, attribution), as the agent that started it. A headless engine
//! runs it flat out (about 15,000 Sepulcra ticks a second), a windowed one a few milliseconds a
//! frame, so the window stays fluid. Swarms run many playtests on many engines
//! (`scripts/playtest_swarm.py`).

use crate::playtest::{self, Action, Expected, Run, Spec, Status};
use crate::response::Format;
use crate::tools::{argument_error, bool_arg, check_keys, num_in, string_arg, Command, Request};
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

pub enum Source {
    Inline(Json),
    File(String),
    Replay(String),
}

pub enum PlaytestCall {
    Run { source: Source, record: Option<String>, wait: Duration, format: Format },
    Report { id: Option<u64>, wait: Duration, cancel: bool, format: Format },
}

fn wait_arg(args: &Json, default: f32) -> Result<Duration, String> {
    Ok(Duration::from_secs_f32(num_in(args, "wait_s", 0.0..=MAX_WAIT_S)?.unwrap_or(default)))
}

pub fn parse(name: &str, args: &Json) -> Option<Result<PlaytestCall, String>> {
    let call = match name {
        "playtest_run" => (|| {
            check_keys(args, name, &[&["spec", "path", "replay", "record", "wait_s", "response_format"]])?;
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
                wait: wait_arg(args, MAX_WAIT_S)?,
                format: Format::arg(args, Format::Concise)?,
            })
        })(),
        "playtest_report" => (|| {
            check_keys(args, name, &[&["id", "wait_s", "cancel", "response_format"]])?;
            Ok(PlaytestCall::Report {
                id: crate::tools::int_arg(args, "id", 1, u64::MAX >> 11)?,
                wait: wait_arg(args, 0.0)?,
                cancel: bool_arg(args, "cancel")?.unwrap_or(false),
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
                          runs) and calls as you; sight_frame steps are listed as captures. record: write a replay file \
                          (spec plus every hash); replay: run one again and stop at the first hash that differs, naming the \
                          parts that differ. Answers when done or after wait_s with progress (then poll playtest_report). \
                          For identical hashes across engines start them with --paused. One playtest at a time per engine."
                .into(),
            input_schema: schema(vec![
                ("spec", t("object", "The playtest (see the description)")),
                ("path", t("string", "A spec file (JSON), relative to the engine's working directory")),
                ("replay", t("string", "A replay file (written by record) to run again and verify")),
                ("record", t("string", "Write the replay file here (relative path) when the run ends")),
                ("wait_s", t("number", "Seconds to wait for the result, 0..20 (default 20); 0 answers at once with the id")),
                ("response_format", Format::schema()),
            ]),
        },
        ToolDef {
            name: "playtest_report".into(),
            description: "A playtest's report: status (running, passed, failed, diverged, error, cancelled), ticks, steps \
                          passed and failed, failures (part, index, tick, call, kind, path, expected, actual), captures, \
                          hashes and, for a replay, whether it verified. Without id: the running playtest, else the latest. \
                          wait_s waits for it to finish; cancel stops it."
                .into(),
            input_schema: schema(vec![
                ("id", t("integer", "The playtest (from playtest_run)")),
                ("wait_s", t("number", "Seconds to wait for it to finish, 0..20 (default 0)")),
                ("cancel", t("boolean", "Stop it (the report says cancelled)")),
                ("response_format", Format::schema()),
            ]),
        },
    ]
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
    started: Instant,
    was_paused: bool,
    /// A call answered later (a capture waiting for builds).
    waiting: Option<Pending>,
    /// Where to write the replay: (as given, resolved).
    record: Option<(String, PathBuf)>,
    game_tools: Vec<String>,
    waiters: Vec<Waiter>,
    cancel: bool,
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

impl Playtests {
    pub fn running(&self) -> bool {
        self.active.is_some()
    }

    /// The running playtest waits for a deferred answer (no need to spin).
    pub fn waiting_on_call(&self) -> bool {
        self.active.as_ref().is_some_and(|a| a.waiting.is_some())
    }
}

fn error(code: &str, message: impl Into<String>, details: Vec<(&str, Json)>) -> ToolOutput {
    let mut pairs = vec![("error", Json::from(code)), ("message", Json::from(message.into()))];
    pairs.extend(details);
    ToolOutput::error(Json::obj(pairs).to_pretty())
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

impl Engine {
    /// playtest_run and playtest_report (answered now or once the playtest ends).
    pub(crate) fn playtest_call(&mut self, call: PlaytestCall, session: SessionInfo, reply: Sender<ToolOutput>) {
        match call {
            PlaytestCall::Run { source, record, wait, format } => match self.start_playtest(source, record, &session) {
                Ok(()) => {
                    let a = self.playtests.active.as_mut().expect("just started");
                    a.wait_or_answer(reply, wait, format);
                }
                Err(e) => {
                    let _ = reply.send(e);
                }
            },
            PlaytestCall::Report { id, wait, cancel, format } => {
                if let Some(a) = self.playtests.active.as_mut().filter(|a| id.is_none_or(|i| i == a.id)) {
                    a.cancel |= cancel;
                    // A cancel answers with the final report, at the next slice.
                    let wait = if cancel { wait.max(Duration::from_secs(5)) } else { wait };
                    a.wait_or_answer(reply, wait, format);
                    return;
                }
                let found = match id {
                    Some(i) => self.playtests.done.iter().find(|f| f.id == i),
                    None => self.playtests.done.back(),
                };
                let out = match found {
                    Some(f) => ToolOutput::text(if format == Format::Detailed { f.detailed.to_string() } else { f.concise.to_string() }),
                    None => {
                        let kept: Vec<Json> = self.playtests.done.iter().map(|f| Json::from(f.id)).collect();
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

    fn start_playtest(&mut self, source: Source, record: Option<String>, session: &SessionInfo) -> Result<(), ToolOutput> {
        if let Some(a) = &self.playtests.active {
            return Err(error(
                "playtest_busy",
                format!("playtest {} ('{}') is running; one at a time per engine (run more engines for more at once)", a.id, a.run.spec.name),
                vec![("id", Json::from(a.id)), ("progress", a.run.progress())],
            ));
        }
        let root = std::env::current_dir().map_err(|e| error("io", format!("the working directory: {e}"), vec![]))?;
        let bad = |e: playtest::SpecError| ToolOutput::error(e.to_json().to_pretty());
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
                let (spec, exp) = playtest::parse_replay(&text, &rel, self.clock.hz()).map_err(bad)?;
                (spec, Some(exp))
            }
        };
        if let Some(g) = &spec.game {
            if self.game.is_none() || *g != self.game_name {
                let hosted = if self.game.is_some() { Json::from(self.game_name.as_str()) } else { Json::Null };
                return Err(error("wrong_game", format!("the spec is for the game '{g}'"), vec![("hosted", hosted)]));
            }
        }
        let game_tools: Vec<String> = self.game.as_ref().map(|g| g.tools().into_iter().map(|t| t.name).collect()).unwrap_or_default();
        let engine_tools = crate::tools::engine_tool_names();
        if let Some(unknown) = spec.calls().find(|c| !game_tools.iter().any(|t| t == c) && !engine_tools.iter().any(|t| t == c)) {
            return Err(error("unknown_tool", format!("the spec calls '{unknown}', which this engine doesn't have (tools/list shows them)"), vec![("call", Json::from(unknown))]));
        }
        let record = match record {
            Some(rel) => {
                let path = playtest::safe_path(&root, &rel, true).map_err(|e| io("record", e))?;
                Some((rel, path))
            }
            None => None,
        };
        self.playtests.next_id += 1;
        let id = self.playtests.next_id;
        let was_paused = self.clock.paused();
        self.clock.set_paused(true);
        // Ticks an earlier time_control step queued would run on into the playtest.
        self.clock.cancel_steps();
        let by = self.swarm.agent_name(session);
        eprintln!("specula: playtest {id} '{}' started by {by}{}", spec.name, if expected.is_some() { " (replay)" } else { "" });
        self.playtests.active = Some(Active {
            id,
            run: Run::new(spec, expected),
            session: session.clone(),
            by,
            started: Instant::now(),
            was_paused,
            waiting: None,
            record,
            game_tools,
            waiters: Vec::new(),
            cancel: false,
        });
        Ok(())
    }

    /// Runs the active playtest for about `budget`.
    pub(crate) fn pump_playtests(&mut self, server: &Server, budget: Duration) {
        let Some(mut a) = self.playtests.active.take() else { return };
        let end = Instant::now() + budget;
        // Fewer ticks between time checks with a window (its frames come first).
        let chunk = if self.windowed { 16 } else { 64 };
        loop {
            if a.cancel {
                a.run.stop(Status::Cancelled, "cancelled with playtest_report");
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
                    match self.playtest_dispatch(&a, &tool, &args, server) {
                        Ok(p) => match p.rx.try_recv() {
                            Ok(out) => a.run.complete(&p.answer(out)),
                            Err(TryRecvError::Empty) => a.waiting = Some(p),
                            Err(TryRecvError::Disconnected) => a.run.complete(&error("no_answer", "the call was dropped unanswered", vec![])),
                        },
                        Err(out) => a.run.complete(&out),
                    }
                }
                Action::Hash(point) => {
                    let parts = self.playtest_parts();
                    a.run.record_hash(point, parts);
                }
                Action::Advance(n) => {
                    let before = self.clock.tick();
                    self.clock.step(n.min(chunk));
                    self.advance_clock();
                    let ran = self.clock.tick() - before;
                    if ran == 0 {
                        a.run.stop(Status::Error, "the clock didn't advance");
                    }
                    a.run.advanced(ran);
                }
                Action::Wait => break,
                Action::Finish => {
                    self.finish_playtest(a);
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
        self.playtests.active = Some(a);
    }

    /// A playtest's call, as its agent: answered at once or later through the receiver.
    fn playtest_dispatch(&mut self, a: &Active, tool: &str, args: &Json, server: &Server) -> Result<Pending, ToolOutput> {
        if let Some((_, why)) = playtest::FORBIDDEN.iter().find(|(n, _)| *n == tool) {
            return Err(error("forbidden", format!("a playtest can't call {tool}: {why}"), vec![]));
        }
        // As over MCP: response_format of the heavy tools is applied to the answer.
        let shaped = crate::response::split(tool, args).map_err(|e| ToolOutput::error(argument_error(&e).to_pretty()))?;
        let format = shaped.as_ref().map(|(f, _)| *f);
        let args = shaped.as_ref().map_or(args, |(_, a)| a);
        let command = if a.game_tools.iter().any(|t| t == tool) {
            Command::Game { name: tool.to_owned(), args: args.clone() }
        } else {
            crate::tools::parse(tool, args).map_err(|e| ToolOutput::error(argument_error(&e).to_pretty()))?
        };
        let (reply, answer) = mpsc::channel();
        let request = Request { session: a.session.clone(), command, reply, deadline: Instant::now() + Duration::from_secs(25) };
        self.handle(request, server);
        Ok(Pending { rx: answer, deadline: Instant::now() + CALL_WAIT, tool: tool.to_owned(), format })
    }

    /// The hashes a playtest checkpoint records: the world, physics and the game's subsystems.
    fn playtest_parts(&mut self) -> Vec<(String, u64)> {
        let mut parts = vec![("world".to_owned(), self.state_hash())];
        if let Some(h) = self.physics.hash() {
            parts.push(("physics".to_owned(), h));
        }
        if let Some(g) = &self.game {
            let mut hashes = Vec::new();
            g.state_hashes(&mut hashes);
            parts.extend(hashes.into_iter().map(|(k, h)| (format!("game.{k}"), h)));
        }
        parts
    }

    fn finish_playtest(&mut self, mut a: Active) {
        if !a.was_paused {
            self.clock.set_paused(false);
            self.last_advance = Instant::now();
        }
        let replay_file = a.record.as_ref().map(|(rel, path)| {
            let game = self.game.as_ref().map(|_| self.game_name.as_str());
            let text = a.run.replay_json(game, self.clock.hz()).to_pretty();
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
        if self.playtests.done.len() == KEPT {
            self.playtests.done.pop_front();
        }
        self.playtests.done.push_back(Finished { id: a.id, concise, detailed });
    }
}

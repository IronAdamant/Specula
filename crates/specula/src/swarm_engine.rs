//! The engine side of the swarm communication tools (see `swarm_tools` for their arguments):
//! the board, intents, notes, roll-ups, leases, the swarm settings, desync traces, conflict help
//! on refused writes, and saving the board with the world.

use crate::board::{Board, NoteTarget};
use crate::claims::Scope;
use crate::conflict::{self, Yours};
use crate::desync::{self, Trace};
use crate::intents::IntentSpec;
use crate::journal::Writer;
use crate::leases::Pools;
use crate::swarm_tools::{NoteOn, SwarmCall, TraceRef};
use crate::world::{Op, WorldError};
use crate::Engine;
use specula_core::Json;
use specula_protocol::{Server, SessionInfo, ToolOutput};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// A board write's outcome: the folder and the error on failure.
type WriteResult = Result<(), (String, String)>;

/// Writing `board.json`: the revision last written, a write running on a worker thread, and the
/// last failure (shown by engine_info and world_save until a write succeeds).
#[derive(Default)]
pub(crate) struct BoardSaver {
    pub saved: u64,
    job: Option<(u64, std::sync::mpsc::Receiver<WriteResult>)>,
    pub error: Option<Json>,
    /// The world's `board.json` couldn't be read (shown by engine_info and task_list until an
    /// agent acknowledges it with `task_list {ack_board_warning: true}`).
    pub warning: Option<Json>,
    /// It couldn't be moved aside either: the board isn't saved there (that would overwrite it).
    pub blocked: bool,
}

impl BoardSaver {
    fn note(&mut self, revision: u64, result: WriteResult) {
        match result {
            Ok(()) => {
                self.saved = revision;
                self.error = None;
            }
            Err((dir, message)) => {
                eprintln!("specula: could not save the task board to {dir}: {message}");
                self.error = Some(Json::obj([
                    ("error", Json::from("io")),
                    ("message", Json::from(format!("the task board was not saved to {dir}: {message}; the next autosave retries"))),
                    ("dir", Json::from(dir)),
                    ("at_revision", Json::from(revision)),
                ]));
            }
        }
    }
}

/// The board's file in a world folder.
pub const BOARD_FILE: &str = "board.json";
const BOARD_TMP: &str = "board.json.tmp";

/// What task edits answer: the task's summary and its last few events (task_get has the rest).
fn brief(t: &crate::board::Task) -> Json {
    let mut j = t.summary_json();
    let n = t.events.len().min(4);
    j.set("events", Json::arr(t.events.iter().skip(t.events.len() - n).map(|e| e.to_json())));
    j
}

fn out(j: Json) -> ToolOutput {
    ToolOutput::text(j.to_string())
}

fn fail(e: &WorldError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_pretty())
}

fn hello_required(what: &str) -> ToolOutput {
    fail(&WorldError::new("hello_required", format!("{what} belong to agent names: call agent_hello with a unique name first")))
}

/// Writes `text` to `dir/board.json` (temporary file, then rename).
fn write_board(dir: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let tmp = dir.join(BOARD_TMP);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, dir.join(BOARD_FILE))
}

impl Engine {
    /// The swarm communication tools.
    pub(crate) fn handle_swarm(&mut self, call: SwarmCall, session: &SessionInfo, server: &Server) -> ToolOutput {
        let by = self.swarm.agent_name(session);
        let key = self.swarm.key(session);
        let w = Writer::new(&by, Some(&key));
        let named = self.swarm.introduced(&session.id);
        let tick = self.clock.tick();
        let now = Instant::now();
        match call {
            SwarmCall::TaskPost(spec) if named => match self.swarm.board.post(w, tick, spec) {
                Ok(t) => out(brief(t)),
                Err(e) => fail(&e),
            },
            SwarmCall::TaskTake { id, expect } if named => match self.swarm.board.take(w, tick, id, expect) {
                Ok(t) => {
                    let mut j = brief(t);
                    j.set("current", Json::from(true));
                    out(j)
                }
                Err(e) => fail(&e),
            },
            SwarmCall::TaskUpdate { id, patch } if named => match self.swarm.board.update(w, tick, id, patch) {
                Ok(t) => out(brief(t)),
                Err(e) => fail(&e),
            },
            SwarmCall::TaskDone { id, summary, cancel, expect } if named => match self.swarm.board.done(w, tick, id, summary, cancel, expect) {
                Ok(t) => out(brief(t)),
                Err(e) => fail(&e),
            },
            SwarmCall::TaskPost(_) | SwarmCall::TaskTake { .. } | SwarmCall::TaskUpdate { .. } | SwarmCall::TaskDone { .. } => hello_required("tasks"),
            SwarmCall::TaskList { mut query, mine, ready_for_me, events_since, ack_board_warning } => {
                if mine {
                    query.owner = Some(by.clone());
                }
                if ready_for_me {
                    query.ready = true;
                    query.ready_for = Some(by.clone());
                }
                let mut j = self.swarm.board.list(&query);
                if let Some(s) = events_since {
                    let events: Vec<Json> = self.swarm.board.events_since(s, query.limit.max(1)).map(|e| e.to_json()).collect();
                    j.set("events", Json::Arr(events));
                }
                if let Some(t) = self.swarm.board.current(w) {
                    j.set("your_current_task", Json::from(t.id));
                }
                if ack_board_warning {
                    self.board_io.warning = None;
                }
                if let Some(warning) = &self.board_io.warning {
                    j.set("board_warning", warning.clone());
                }
                out(j)
            }
            SwarmCall::TaskGet { id, events } => match self.swarm.board.get(id) {
                Some(t) => {
                    // The newest questions (open ones first): a task holds up to 32 of 2000 characters.
                    let mut j = t.to_json_with(Some(events), Some(8));
                    j.set("notes", Json::arr(self.swarm.board.notes_on_task(id, 10).into_iter().map(|n| n.to_json())));
                    if let Some(p) = t.parent {
                        j.set("parent_title", self.swarm.board.get(p).map_or(Json::Null, |p| Json::from(p.title.as_str())));
                    }
                    let subtasks: Vec<Json> =
                        self.swarm.board.tasks().filter(|s| s.parent == Some(id)).take(20).map(|s| Json::obj([("id", Json::from(s.id)), ("status", Json::from(s.status.name()))])).collect();
                    if !subtasks.is_empty() {
                        j.set("subtasks", Json::Arr(subtasks));
                    }
                    out(j)
                }
                None => fail(&WorldError::new("task_not_found", format!("there is no task {id} (see task_list)")).with("task", id)),
            },
            SwarmCall::IntentDeclare { scopes, text, task, lease, replace } if named => {
                let summary = scopes.iter().map(Scope::describe).collect::<Vec<_>>().join(", ");
                let spec = IntentSpec { scopes, text, task, lease, replace };
                match self.swarm.intents.declare(&self.world, &self.swarm.claims, w, tick, spec, now) {
                    Ok(j) => {
                        self.swarm.board.log(&by, tick, "intent", task, summary);
                        out(j)
                    }
                    Err(e) => fail(&e),
                }
            }
            SwarmCall::IntentDeclare { .. } => hello_required("intents"),
            SwarmCall::IntentList { owner, mine, path_prefix, after, limit } => {
                let owner = if mine { Some(by.clone()) } else { owner };
                out(self.swarm.intents.list(owner.as_deref(), path_prefix.as_deref(), after, limit, now))
            }
            SwarmCall::IntentClear { id } => match self.swarm.intents.clear(w, id) {
                Ok(ids) => {
                    if !ids.is_empty() {
                        self.swarm.board.log(&by, tick, "intent_clear", None, format!("{ids:?}"));
                    }
                    out(Json::obj([("cleared", Json::arr(ids.into_iter().map(Json::from)))]))
                }
                Err(e) => fail(&e),
            },
            SwarmCall::NoteAdd { target, text } if named => {
                let target = match target {
                    NoteOn::Entity(k) => match self.world.resolve(&k) {
                        Ok(id) => NoteTarget::Entity { id, path: self.world.path_of(id) },
                        Err(e) => return fail(&e),
                    },
                    NoteOn::Path(p) => NoteTarget::Path(p),
                    NoteOn::Task(t) => NoteTarget::Task(t),
                };
                match self.swarm.board.add_note(w, tick, target, text) {
                    Ok(n) => out(n.to_json()),
                    Err(e) => fail(&e),
                }
            }
            SwarmCall::NoteAdd { .. } => hello_required("notes"),
            SwarmCall::NoteList(q) => out(self.swarm.board.list_notes(&q)),
            SwarmCall::NoteDelete { id, force } => match self.swarm.board.delete_note(w, tick, id, force) {
                Ok(n) => out(Json::obj([("deleted", Json::from(n.id))])),
                Err(e) => fail(&e),
            },
            SwarmCall::Rollup(q) => out(crate::rollup::rollup(&self.swarm, &q, now)),
            SwarmCall::LeaseTake { want, note, lease } if named => {
                let pools = self.lease_pools(server);
                // The OS's listener table, read once per call (no socket is opened to ask).
                let listening = matches!(want, crate::leases::Want::Ports(_)).then(specula_sys::net::tcp_listening_ports).and_then(Result::ok);
                let free = |p: u16| crate::leases::port_free(p, listening.as_ref());
                match self.swarm.leases.take(w, (want, note, lease), &pools, &free, now) {
                    Ok(l) => {
                        let j = l.to_json(now);
                        let text = format!("{} lease {}", l.resource.kind(), l.id);
                        self.swarm.board.log(&by, tick, "lease", None, text);
                        out(j)
                    }
                    Err(e) => fail(&e),
                }
            }
            SwarmCall::LeaseTake { .. } => hello_required("leases"),
            SwarmCall::LeaseRenew { id, lease } => match self.swarm.leases.renew(w, id, lease, now) {
                Ok(renewed) => out(Json::obj([("renewed", Json::Arr(renewed))])),
                Err(e) => fail(&e),
            },
            SwarmCall::LeaseRelease { id, delete } => {
                let root = self.lease_pools(server).scratch_root;
                match self.swarm.leases.release(w, id, delete, &root, now) {
                    Ok((ids, deleted)) => {
                        if !ids.is_empty() {
                            self.swarm.board.log(&by, tick, "lease_release", None, format!("{ids:?}"));
                        }
                        out(Json::obj([
                            ("released", Json::arr(ids.into_iter().map(Json::from))),
                            ("deleted_dirs", Json::arr(deleted.into_iter().map(Json::from))),
                        ]))
                    }
                    Err(e) => fail(&e),
                }
            }
            SwarmCall::LeaseList { kind, owner, mine, after, limit } => {
                let owner = if mine { Some(by.clone()) } else { owner };
                let mut j = self.swarm.leases.list(kind.as_deref(), owner.as_deref(), after, limit, now);
                let pools = self.lease_pools(server);
                j.set(
                    "pools",
                    Json::obj([
                        ("ports", Json::arr([Json::from(u64::from(pools.ports.0)), Json::from(u64::from(pools.ports.1))])),
                        ("gpu_ms_per_s", Json::from(pools.gpu_ms_per_s)),
                        ("gpu_leased_ms_per_s", Json::from(self.swarm.leases.gpu_leased(now))),
                        ("scratch_root", Json::from(pools.scratch_root.display().to_string())),
                    ]),
                );
                out(j)
            }
            SwarmCall::Settings { frame_floor_fps: Some(_) } if !named || !matches!(self.swarm.role(&session.id), "lead" | "director") => {
                fail(&WorldError::new(
                    "lead_only",
                    "the frame floor is chosen by the lead agent before it deploys a swarm: say agent_hello with role \"lead\" \
                     (or \"director\") first; anyone may read swarm_settings",
                )
                .with("role", self.swarm.role(&session.id)))
            }
            SwarmCall::Settings { frame_floor_fps } => {
                if let Some(fps) = frame_floor_fps {
                    self.frame_floor = (fps, Some(by.clone()));
                    self.schedule.set_frame_floor(fps);
                    self.swarm.board.log(&by, tick, "settings", None, format!("frame_floor_fps {fps}"));
                }
                out(self.settings_json())
            }
            SwarmCall::DesyncTrace { from, to } => {
                let trace = self.live_trace(from, to);
                if trace.ticks.is_empty() {
                    let range = self.trace.range().map_or(Json::Null, |(a, b)| Json::arr([Json::from(a), Json::from(b)]));
                    return fail(
                        &WorldError::new("no_ticks", "no recorded ticks in that range (the clock may be paused: time_control step runs ticks)")
                            .with("recorded", range),
                    );
                }
                self.traces_written += 1;
                let (lo, hi) = (trace.ticks.keys().next().copied().unwrap_or(0), trace.ticks.keys().next_back().copied().unwrap_or(0));
                let path = self.capture_dir.join(format!("trace_{:04}_{lo}-{hi}.json", self.traces_written));
                let text = trace.render();
                let written = std::fs::create_dir_all(&self.capture_dir).and_then(|()| std::fs::write(&path, &text));
                match written {
                    Ok(()) => {
                        let mut j = trace.summary();
                        j.set("file", Json::from(path.display().to_string()));
                        j.set("bytes", Json::from(text.len()));
                        if let Some((_, (w, _))) = trace.ticks.iter().next_back() {
                            j.set("last_world_hash", Json::from(crate::journal::hex(*w)));
                        }
                        out(j)
                    }
                    Err(e) => fail(&WorldError::new("io", format!("could not write {}: {e}", path.display())).with("file", path.display().to_string())),
                }
            }
            SwarmCall::DesyncCompare { a, b, from_tick } => {
                if b == TraceRef::Replay {
                    let mut j = desync::against_replay(&self.trace, &self.swarm.journal, &self.world, from_tick);
                    j.set("a", Json::from("live"));
                    j.set("b", Json::from("replay"));
                    return out(j);
                }
                let load = |r: &TraceRef, e: &Engine| -> Result<Trace, WorldError> {
                    match r {
                        TraceRef::Live => Ok(e.live_trace(None, None)),
                        TraceRef::Replay => unreachable!("checked when parsed"),
                        TraceRef::File(p) => {
                            let path = e.trace_path(p);
                            Trace::read(&path).map_err(|m| WorldError::new("bad_trace", m).with("file", path.display().to_string()))
                        }
                    }
                };
                let (ta, tb) = match (load(&a, self), load(&b, self)) {
                    (Ok(x), Ok(y)) => (x, y),
                    (Err(e), _) | (_, Err(e)) => return fail(&e),
                };
                match desync::compare(&ta, &tb) {
                    Ok(mut j) => {
                        j.set("a", ta.summary());
                        j.set("b", tb.summary());
                        out(j)
                    }
                    Err(e) => fail(&e),
                }
            }
        }
    }

    /// A trace file named in `desync_compare`: absolute, or relative to this run's captures.
    fn trace_path(&self, p: &str) -> PathBuf {
        let path = PathBuf::from(p);
        if path.is_absolute() || path.exists() {
            path
        } else {
            self.capture_dir.join(path)
        }
    }

    fn live_trace(&self, from: Option<u64>, to: Option<u64>) -> Trace {
        let game = self.game.as_ref().map(|_| self.game_name.as_str());
        Trace::live(&self.trace, &self.swarm.journal, game, self.clock.hz(), from, to)
    }

    fn lease_pools(&self, server: &Server) -> Pools {
        let cfg = &self.perf_cfg.swarm;
        let root = cfg.scratch_root.clone().unwrap_or_else(|| self.capture_dir.parent().unwrap_or(&self.capture_dir).join("scratch"));
        Pools { ports: cfg.lease_ports, reserved_port: Some(server.local_addr().port()), gpu_ms_per_s: cfg.gpu_ms_per_s, scratch_root: root }
    }

    /// `swarm_settings`' answer: the frame floor, the operator's hardware and a suggestion.
    fn settings_json(&self) -> Json {
        let a = self.gpu.adapter();
        let kind = crate::perf::adapter_kind(a.kind);
        let heaps = self.gpu.memory_stats().heaps;
        let vram_mb = heaps.iter().filter(|h| h.device_local).map(|h| h.size).max().map(|b| b / (1 << 20));
        let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
        let refresh = self.schedule.refresh_known();
        let power = specula_sys::power();
        let on_battery = power.is_some_and(|p| p.has_battery && p.on_ac == Some(false));
        let vram_gb = vram_mb.unwrap_or(0) / 1024;
        // The owner's line (2026-10-17): low- and mid-spec laptops can't spare frames; high-spec
        // laptops, desktops and cloud machines can. A hint only: the lead decides.
        let (suggest, why) = if !self.windowed {
            (self.frame_floor.0, "headless: no window to keep fluid, so the floor doesn't apply (agents get every frame)")
        } else if on_battery {
            (60, "on battery: keep 60 so the operator's window stays fluid (and the battery lasts)")
        } else if kind == "integrated" || threads <= 4 {
            (60, "an integrated GPU or few CPU threads: a low- or mid-spec machine. Keep 60 so the operator's window stays fluid")
        } else if kind == "discrete" && threads >= 16 && vram_gb >= 10 {
            (30, "a strong discrete GPU and many CPU threads: a high-spec machine can trade frames for agent throughput (30, or 20 for big swarms)")
        } else {
            (60, "a mid-spec machine: keep 60 for 3D; 2D work may afford 30")
        };
        let floor = self.frame_floor.0;
        Json::obj([
            ("frame_floor_fps", Json::from(floor)),
            ("frame_floor_default", Json::from(crate::schedule::DEFAULT_FRAME_FLOOR_FPS)),
            ("frame_floor_range", Json::arr([Json::from(crate::schedule::MIN_FRAME_FLOOR_FPS), Json::from(crate::schedule::MAX_FRAME_FLOOR_FPS)])),
            (
                "set_by",
                match &self.frame_floor.1 {
                    Some(who) => Json::from(who.as_str()),
                    None if self.perf_cfg.source.is_some() => Json::from("perf.toml"),
                    None => Json::from("default"),
                },
            ),
            ("windowed", Json::from(self.windowed)),
            (
                "hardware",
                Json::obj([
                    ("gpu", Json::from(a.name.as_str())),
                    ("gpu_kind", Json::from(kind)),
                    ("vendor_id", Json::from(format!("0x{:04x}", a.vendor_id))),
                    ("vram_mb", Json::from(vram_mb)),
                    ("vram_note", Json::from(if kind == "integrated" { "shared with the CPU (system memory)" } else { "device-local heap" })),
                    ("cpu_threads", Json::from(threads)),
                    ("has_battery", Json::from(power.map(|p| p.has_battery))),
                    ("on_ac_power", Json::from(power.and_then(|p| p.on_ac))),
                    ("display_refresh_hz", Json::from(refresh.map(|(d, _)| (1.0 / d.as_secs_f64() * 10.0).round() / 10.0))),
                    ("refresh_from", Json::from(refresh.map(|(_, s)| s))),
                ]),
            ),
            ("suggested_frame_floor_fps", Json::from(suggest)),
            ("why", Json::from(why)),
            ("serve", if self.windowed { self.schedule.to_json() } else { Json::Null }),
        ])
    }

    /// Adds `conflict` (base, theirs, yours) to a refused write's error (see [`conflict`]).
    pub(crate) fn conflict_help(&self, e: WorldError, yours: Option<&Yours>) -> WorldError {
        match yours {
            Some(y) => conflict::help(&self.world, &self.swarm.journal, e, y),
            None => conflict::help(&self.world, &self.swarm.journal, e, &Yours::default()),
        }
    }

    /// What a batch's ops with `expect_version` set (a stale one is named by its op index), for
    /// conflict help; empty (no allocation) when no op has one.
    pub(crate) fn batch_yours(&self, ops: &[Op]) -> Vec<(usize, Yours)> {
        ops.iter()
            .enumerate()
            .filter_map(|(i, op)| match op {
                Op::Set(_, u) if u.expect_version.is_some() => Some((i, Yours::of_update(u))),
                Op::Delete(_, Some(_)) => Some((i, Yours::delete())),
                _ => None,
            })
            .collect()
    }

    /// After a claim was taken: link it to the agent's current task and name others' intents it
    /// overlaps.
    pub(crate) fn claim_taken(&mut self, w: Writer, scope: &Scope, mut reply: Json) -> Json {
        let tick = self.clock.tick();
        if let Some(id) = reply.get("id").and_then(Json::as_u64) {
            if let Some(task) = self.swarm.board.note_claim(w, id, tick) {
                reply.set("task", Json::from(task));
            }
        }
        let warnings = self.swarm.intents.warnings(&self.world, w, scope, Instant::now());
        if !warnings.is_empty() {
            reply.set("intent_warnings", Json::Arr(warnings));
        }
        reply
    }

    /// Records tick `tick`'s hashes for the desync detector (world hash `hash`, the game's own).
    pub(crate) fn record_trace(&mut self, tick: u64, hash: u64) {
        if let Some(g) = &self.game {
            g.state_hashes(&mut self.trace.scratch);
        }
        // Physics' own state (warm starts, sleep timers, contact memory) isn't in the world hash.
        if let Some(h) = self.physics.hash() {
            self.trace.scratch.push(("physics", h));
        }
        // So is navigation's (corridors, crowd velocities, minds' running nodes).
        if let Some(h) = self.nav.last_hash() {
            self.trace.scratch.push(("nav", h));
        }
        // Module state kept in the engine (stores, RNG positions, events): saved by save games.
        if let Some(h) = self.logic.state_hash() {
            self.trace.scratch.push(("logic", h));
        }
        // Live Lex runs' state and when they step (saved by save games).
        if let Some(h) = self.lex_live.state_hash() {
            self.trace.scratch.push(("lex_live", h));
        }
        self.trace.record(tick, hash, self.world.next_id());
    }

    /// Whether the board has changes not yet written next to the world.
    pub(crate) fn board_unsaved(&self) -> bool {
        self.swarm.board.revision() != self.board_io.saved
    }

    /// Where the board is written: the current world's folder, once the world has been saved there.
    fn board_dir(&self) -> Option<PathBuf> {
        if self.board_io.blocked {
            return None;
        }
        let dir = self.worlds_dir.join(self.world_name.as_ref()?);
        dir.join("world.json").exists().then_some(dir)
    }

    /// Autosave: when the board changed since it was last written, renders it (only the tasks that
    /// changed are rendered again) and writes it on a worker thread, like the world's autosave.
    pub(crate) fn save_board_if_due(&mut self) {
        self.finish_board_save(false);
        if self.board_io.job.is_some() || !self.board_unsaved() {
            return;
        }
        let Some(dir) = self.board_dir() else { return };
        let text = {
            let _z = specula_core::profile::zone("board.render");
            self.swarm.board.render_cached()
        };
        let revision = self.swarm.board.revision();
        let (tx, done) = std::sync::mpsc::channel();
        let job = move || {
            let _z = specula_core::profile::zone("board.write");
            let _ = tx.send(write_board(&dir, &text).map_err(|e| (dir.display().to_string(), e.to_string())));
        };
        if std::thread::Builder::new().name("specula-board".into()).spawn(job).is_ok() {
            self.board_io.job = Some((revision, done));
        }
    }

    /// Collects a finished board write (or waits for it with `block`).
    fn finish_board_save(&mut self, block: bool) {
        let Some((revision, done)) = &self.board_io.job else { return };
        let result = if block {
            done.recv().ok()
        } else {
            match done.try_recv() {
                Ok(r) => Some(r),
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
            }
        };
        let revision = *revision;
        self.board_io.job = None;
        self.board_io.note(revision, result.unwrap_or_else(|| Err((String::new(), "the board writer stopped without reporting".into()))));
    }

    /// At shutdown: finishes a running write and writes what changed since.
    pub(crate) fn save_board_blocking(&mut self) {
        self.finish_board_save(true);
        if self.board_unsaved() {
            self.save_board_now();
        }
    }

    /// Writes the board next to the current world now (world_save: the world may have a new
    /// folder; world_load: the outgoing world keeps its board). Returns the error, if any (also
    /// shown by engine_info until a later write succeeds).
    pub(crate) fn save_board_now(&mut self) -> Option<Json> {
        self.finish_board_save(true);
        let dir = self.board_dir()?;
        let _z = specula_core::profile::zone("board.save");
        let text = self.swarm.board.render_cached();
        let revision = self.swarm.board.revision();
        self.board_io.note(revision, write_board(&dir, &text).map_err(|e| (dir.display().to_string(), e.to_string())));
        self.board_io.error.clone()
    }

    /// Reads the board saved with the world in `dir` (an empty board when there is none).
    /// Returns a warning when the file is there but unreadable (the board then starts empty and
    /// the file is kept until the next save replaces it).
    pub(crate) fn load_board(&mut self, dir: &Path) -> Option<String> {
        let path = dir.join(BOARD_FILE);
        let problem = match std::fs::read_to_string(&path) {
            Ok(text) => match Board::parse(&text) {
                Ok(b) => {
                    self.swarm.board = b;
                    None
                }
                Err(e) => Some(format!("not loaded: {e}")),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.swarm.board = Board::default();
                None
            }
            Err(e) => Some(format!("not read: {e}")),
        };
        self.board_io.warning = None;
        self.board_io.blocked = false;
        if let Some(why) = problem {
            // Keep the unreadable file (a later save must not overwrite it): move it aside, or
            // stop saving the board here while it can't be moved.
            self.swarm.board = Board::default();
            let aside = (1..1000).map(|n| dir.join(format!("{BOARD_FILE}.unreadable-{n}"))).find(|p| !p.exists());
            let moved = aside.as_ref().filter(|to| std::fs::rename(&path, to).is_ok());
            let message = match moved {
                Some(to) => format!("task board {} {why}; it was moved to {} and the board starts empty", path.display(), to.display()),
                None => {
                    self.board_io.blocked = true;
                    format!("task board {} {why}; it couldn't be moved aside, so the board is not saved here until it is fixed or removed", path.display())
                }
            };
            self.board_io.warning = Some(Json::obj([
                ("error", Json::from("board_unreadable")),
                ("message", Json::from(message.as_str())),
                ("file", Json::from(path.display().to_string())),
                ("moved_to", Json::from(moved.map(|p| p.display().to_string()))),
            ]));
            self.board_io.saved = self.swarm.board.revision();
            return Some(message);
        }
        self.board_io.saved = self.swarm.board.revision();
        None
    }
}

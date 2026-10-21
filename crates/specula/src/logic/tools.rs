//! The module host's agent tools: `logic_build`, `logic_reload`, `logic_unload`,
//! `logic_status`, `logic_input` and `logic_hook`.
//!
//! `logic_build` and `logic_reload` do their slow parts on the calling connection's thread,
//! never the engine's: Cargo (seconds; through one engine-wide [`BuildService`] that runs one
//! Cargo at a time and merges duplicate requests) and opening the library (copying it and
//! mapping it, which the OS may scan first: 100-230 ms per new library, whatever its size). Only the
//! swap goes to the engine thread, applied at a tick boundary like any other command (about a
//! millisecond: describe, stop the old build, start the new one).

use super::{root_of, LogicHost, Origin, Phase};
use crate::game::Clock;
use crate::journal::{Kind, Writer};
use crate::swarm::Swarm;
use crate::tools::{argument_error, bool_arg, check_keys, int_arg, string_arg, Command, Request};
use crate::world::{Mark, World, WorldError};
use specula_core::Json;
use specula_logic::{BuildReport, BuildRequest, BuildService, ModuleCode};
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Diagnostics one `logic_build` answer lists (errors first); the rest are counted.
pub const MAX_DIAGNOSTICS: usize = 30;
/// How long `logic_build` waits for the engine to apply the reload after the build.
const RELOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// A module-host command for the engine thread.
pub enum LogicCall {
    /// Swap in a library already opened (and ABI-checked) off the engine thread.
    Load { code: ModuleCode, origin: Origin, seed: Option<u64>, open_ms: f64 },
    /// Where module `module` was loaded from (for `logic_reload`).
    Origin { module: String },
    Unload { module: String, forget: bool },
    Status { module: Option<String>, detailed: bool },
    Input { module: String, actions: Json, ticks: u64 },
    Hook { hook: String, args: Json, reason: String },
}

/// The engine's one build service.
pub fn build_service() -> &'static BuildService {
    static SERVICE: OnceLock<BuildService> = OnceLock::new();
    SERVICE.get_or_init(BuildService::default)
}

fn engine_root() -> PathBuf {
    std::env::current_dir().map(|d| specula_logic::fix::canonical(&d)).unwrap_or_else(|_| PathBuf::from("."))
}

/// `path` (relative to the engine's working directory, or absolute) as a canonical path that
/// must exist and lie under the working directory: agents may only build and load code from
/// the project the engine runs in. Checked by its text first (no `..`, no UNC, device or
/// verbatim prefixes, absolute paths spelled under the root), so a refused path is never looked
/// up on disk (or on the network); then by the file system, so links can't lead out either.
pub(crate) fn project_path(path: &str, what: &str) -> Result<PathBuf, String> {
    let root = engine_root();
    let outside = || format!("'{what}' must be inside the engine's working directory ({}); {path} is not", root.display());
    if path.starts_with(r"\\") || path.starts_with("//") || path.contains('\0') {
        return Err(outside());
    }
    let p = Path::new(path);
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("'{what}' may not contain '..': give the path below {}", root.display()));
    }
    let joined = if p.is_absolute() || p.has_root() {
        // Spelled under the root (case-insensitively on Windows, where paths are)?
        let norm = |s: &Path| {
            let t = s.to_string_lossy().replace('\\', "/");
            let t = t.trim_end_matches('/').to_owned();
            if cfg!(windows) { t.to_lowercase() } else { t }
        };
        let (a, r) = (norm(p), norm(&root));
        if !(a == r || a.starts_with(&format!("{r}/"))) {
            return Err(outside());
        }
        p.to_owned()
    } else {
        if p.components().any(|c| matches!(c, Component::Prefix(_))) {
            return Err(outside());
        }
        root.join(p)
    };
    if !joined.exists() {
        return Err(format!("'{what}' {path} does not exist (relative paths start at {})", root.display()));
    }
    let canon = specula_logic::fix::canonical(&joined);
    if !canon.starts_with(&root) {
        return Err(format!("'{what}' must be inside the engine's working directory ({}); {path} is not", root.display()));
    }
    Ok(canon)
}

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([
            ("type", Json::from("object")),
            ("properties", Json::obj(props)),
            ("required", Json::arr(required.iter().map(|r| Json::from(*r)))),
        ])
    };
    let t = |ty: &str, desc: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(desc))]);
    let module = t("string", "Module name (as its ModuleInfo::name says; logic_status lists them)");
    vec![
        ToolDef {
            name: "logic_build".into(),
            description: "Build a game module (a Rust cdylib depending only on specula-sdk) with Cargo and hot-reload it into the \
                          running world at the next tick boundary: the world, the module's state store and its RNG streams survive; \
                          a build that fails or can't load leaves the running one in place. Compiler errors and warnings come back \
                          as data (level, code, file, line, column, label, notes, suggested fixes with their applicability); \
                          fix:true applies rustc's MachineApplicable fixes inside the module folder (as cargo fix) and rebuilds. \
                          check:true only type-checks (fast, no reload). The module's sources are also checked against the \
                          determinism rules (platform maths such as f32::sin instead of dmath, wall-clock time, HashMap, threads, \
                          static mut / thread_local): each hit is a warning with code determinism::<rule> and a help note \
                          (determinism_warnings counts them; `// determinism: allowed: <why>` marks a deliberate use). \
                          One Cargo runs at a time engine-wide; identical queued \
                          requests from several agents share one build (requests_merged). Timings in timings_ms. See \
                          docs/design/MODULES.md."
                .into(),
            input_schema: schema(
                vec![
                    ("path", t("string", "The module crate's folder, relative to the engine's working directory (e.g. games/ambulo)")),
                    ("check", t("boolean", "Only cargo check: diagnostics, no library, no reload (default false)")),
                    ("fix", t("boolean", "Apply machine-applicable fixes inside the module folder, then rebuild (default false)")),
                    ("release", t("boolean", "Release profile (default: dev)")),
                    ("reload", t("boolean", "Load the built library into the world (default true)")),
                    ("detailed", t("boolean", "Include each diagnostic's full rustc rendering (default false; costs tokens)")),
                    ("seed", t("integer", "Re-seed the module's RNG streams (default: keep them)")),
                ],
                &["path"],
            ),
        },
        ToolDef {
            name: "logic_reload".into(),
            description: "Reload a module from the library it was last loaded from (after building it yourself), or load a \
                          prebuilt module library for the first time. Applied at a tick boundary; same guarantees as logic_build."
                .into(),
            input_schema: schema(
                vec![
                    ("module", module.clone()),
                    ("library", t("string", "A module library (.dll/.so) inside the working directory; required the first time")),
                    ("seed", t("integer", "Re-seed the module's RNG streams")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "logic_unload".into(),
            description: "Stop and unload a module (its stop runs first, e.g. removing what it spawned). Its state store and RNG \
                          streams are kept for a later load unless forget_state is true."
                .into(),
            input_schema: schema(vec![("module", module.clone()), ("forget_state", t("boolean", "Also drop the module's state store and RNG streams"))], &["module"]),
        },
        ToolDef {
            name: "logic_status".into(),
            description: "Loaded modules: status (running/disabled and why), generation, build hash, ticks, failures and the last \
                          error, tick time, input actions, agent-held input, native hooks, state size; plus recent loads, log lines, \
                          events and hook uses (with reasons), and the engine's recent builds."
                .into(),
            input_schema: schema(
                vec![
                    ("module", module.clone()),
                    ("detailed", t("boolean", "More history and the state store's values (bounded)")),
                    ("response_format", crate::response::Format::schema()),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "logic_input".into(),
            description: "Hold a module's input actions for the next N ticks, as a player would (deterministic: applied at tick \
                          boundaries, on top of window and gamepad input). Buttons take true/false (a press on the first tick, \
                          a release after the last), axes a number, 2D axes [x, y]. 0 ticks or a neutral value releases. Use \
                          time_control to step the clock and see the result."
                .into(),
            input_schema: schema(
                vec![
                    ("module", module.clone()),
                    ("actions", Json::obj([("type", Json::from("object")), ("description", Json::from("{\"move\": [1, 0], \"jump\": true}"))])),
                    ("ticks", t("integer", "How many ticks to hold them (1..18000, default 1)")),
                ],
                &["module", "actions"],
            ),
        },
        ToolDef {
            name: "logic_hook".into(),
            description: "Call a native hook a module declares (module/hook, listed by logic_status): the code escape hatch for \
                          behaviour data can't express yet. Every use is logged with your reason, so the data language can grow \
                          from evidence. A failing hook changes nothing."
                .into(),
            input_schema: schema(
                vec![
                    ("hook", t("string", "module/hook, e.g. ambulo/launch")),
                    ("args", Json::obj([("type", Json::from("object")), ("description", Json::from("The hook's arguments (its schema is in logic_status)"))])),
                    ("reason", t("string", "Why a native hook is needed here (required; logged)")),
                ],
                &["hook", "reason"],
            ),
        },
    ]
}

fn seed_arg(args: &Json) -> Result<Option<u64>, String> {
    int_arg(args, "seed", 0, u64::MAX >> 11)
}

/// The engine-thread tools (`logic_build` and `logic_reload` are answered by [`build`] and
/// [`reload`] on the connection's thread instead).
pub fn parse(name: &str, args: &Json) -> Option<Result<Command, String>> {
    let call = match name {
        "logic_unload" => (|| {
            check_keys(args, name, &[&["module", "forget_state"]])?;
            let module = string_arg(args, "module")?.ok_or("'module' is required")?.to_owned();
            Ok(LogicCall::Unload { module, forget: bool_arg(args, "forget_state")?.unwrap_or(false) })
        })(),
        "logic_status" => (|| {
            check_keys(args, name, &[&["module", "detailed", "response_format"]])?;
            // response_format, as on the other tools, is the same knob as `detailed`.
            let format = crate::response::Format::arg(args, crate::response::Format::Concise)?;
            let detailed = bool_arg(args, "detailed")?.unwrap_or(false) || format == crate::response::Format::Detailed;
            Ok(LogicCall::Status { module: string_arg(args, "module")?.map(str::to_owned), detailed })
        })(),
        "logic_input" => (|| {
            check_keys(args, name, &[&["module", "actions", "ticks"]])?;
            let module = string_arg(args, "module")?.ok_or("'module' is required")?.to_owned();
            let actions = args.get("actions").filter(|a| matches!(a, Json::Obj(_))).ok_or("'actions' is required: {\"action\": true | number | [x, y]}")?;
            let ticks = int_arg(args, "ticks", 0, super::input::MAX_HOLD_TICKS)?.unwrap_or(1);
            Ok(LogicCall::Input { module, actions: actions.clone(), ticks })
        })(),
        "logic_hook" => (|| {
            check_keys(args, name, &[&["hook", "args", "reason"]])?;
            let hook = string_arg(args, "hook")?.ok_or("'hook' is required: module/hook")?.to_owned();
            let reason = string_arg(args, "reason")?.ok_or("'reason' is required: why a native hook is needed")?.to_owned();
            let hook_args = args.get("args").cloned().unwrap_or(Json::Null);
            if !matches!(hook_args, Json::Obj(_) | Json::Null) {
                return Err("'args' must be an object".to_owned());
            }
            Ok(LogicCall::Hook { hook, args: hook_args, reason })
        })(),
        _ => return None,
    };
    Some(call.map(Command::Logic))
}

/// How the engine journals module changes.
pub(crate) fn kind(phase: Phase) -> Kind {
    match phase {
        Phase::Start => Kind::GameStart,
        Phase::Tick => Kind::GameTick,
        Phase::Hook | Phase::Stop => Kind::GameTool,
    }
}

fn output(r: Result<Json, WorldError>) -> ToolOutput {
    match r {
        Ok(j) => ToolOutput::text(j.to_string()),
        Err(e) => ToolOutput::error(e.to_json().to_pretty()),
    }
}

/// Opens a module library (off the engine thread: it can take a while).
fn open(library: &Path) -> Result<ModuleCode, WorldError> {
    let copies = library.parent().unwrap_or(Path::new(".")).join("specula-modules");
    ModuleCode::open(library, &copies).map_err(|e| {
        let mut w = WorldError::new("load_failed", e.message.clone())
            .with("reason", e.code)
            .with("library", e.path.display().to_string());
        if let Some(c) = e.os_code {
            w = w.with("os_error", c);
        }
        if let Some(a) = e.module_abi {
            w = w.with("module_abi", a).with("engine_abi", specula_logic::abi::ABI_VERSION);
        }
        w
    })
}

/// What an agent's load, unload or hook commits: the module's changes checked against other
/// agents' claims as `w`'s, and journaled under the module's name via `w`.
fn agent_commit<'a>(swarm: &'a mut Swarm, tick: u64, w: Writer<'a>) -> impl FnMut(&mut World, Mark, &str, Phase) -> Result<(), WorldError> + 'a {
    move |world: &mut World, mark: Mark, agent: &str, _: Phase| swarm.check_game_changes(world, mark, tick, agent, w)
}

/// Who may load, replace or unload a module: anyone, unless another agent holds a live claim
/// covering the module's root (`/<name>`). Claiming a module's root is how an agent owns it;
/// `claim_share` lets others work on it too. (The start and stop changes are also checked
/// against claims like any other write.)
fn check_module_owner(swarm: &Swarm, module: &str, w: Writer, doing: &str) -> Result<(), WorldError> {
    swarm.check_claimed_path(&root_of(module), w, &format!("{doing} module '{module}' is refused")).map_err(|e| e.with("module", module))
}

/// Applies a module-host command on the engine thread, at a tick boundary. `w` is the calling
/// agent: what its loads, unloads and hooks change is checked against other agents' claims, and
/// a module whose root (`/<name>`) another agent has claimed can't be replaced or unloaded.
pub fn handle(host: &mut LogicHost, call: LogicCall, world: &mut World, clock: &Clock, swarm: &mut Swarm, w: Writer) -> ToolOutput {
    let (tick, hz) = (clock.tick(), clock.hz());
    let result = match call {
        LogicCall::Load { code, origin, seed, open_ms } => LogicHost::name_of(&code)
            .and_then(|name| check_module_owner(swarm, &name, w, "loading"))
            .and_then(|()| host.load(code, origin, seed, world, tick, hz, &mut agent_commit(swarm, tick, w)))
            .map(|mut report| {
            // Opening the library (on the caller's thread) is part of the reload's time.
            let mut timings = report.get("timings_ms").cloned().unwrap_or(Json::Null);
            timings.set("open", Json::from(open_ms));
            report.set("timings_ms", timings);
            report
        }),
        LogicCall::Origin { module } => match host.origin(&module) {
            Some(o) => Ok(Json::obj([
                ("library", o.library.as_ref().map_or(Json::Null, |l| Json::from(l.display().to_string()))),
                ("dir", o.dir.as_ref().map_or(Json::Null, |d| Json::from(d.display().to_string()))),
            ])),
            None => Err(WorldError::new("no_module", format!("no module '{module}' is loaded; give 'library' to load one")).with("module", module.as_str())),
        },
        LogicCall::Unload { module, forget } => check_module_owner(swarm, &module, w, "unloading")
            .and_then(|()| host.unload(&module, forget, world, tick, hz, &mut agent_commit(swarm, tick, w))),
        LogicCall::Status { module, detailed } => host.status(module.as_deref(), detailed).map(|mut j| {
            j.set("builds", builds_json());
            j
        }),
        LogicCall::Input { module, actions, ticks } => host.hold(&module, &actions, ticks),
        // Like a game tool: the hook's changes may not reach into another agent's claim (if
        // they do, nothing the hook did is kept and the error says whose claim).
        LogicCall::Hook { hook, args, reason } => {
            host.call_hook(&hook, &args, &reason, w.name, world, tick, hz, &mut agent_commit(swarm, tick, w))
        }
    };
    output(result)
}

/// The JSON an agent gets for a build.
fn report_json(r: &BuildReport, req: &BuildRequest, detailed: bool) -> Json {
    let base = &req.dir;
    let shown: Vec<Json> = r.diagnostics.iter().take(MAX_DIAGNOSTICS).map(|d| d.to_json(base, detailed)).collect();
    let mut f = vec![
        ("ok", Json::from(r.ok)),
        ("path", Json::from(base.display().to_string())),
        ("package", r.package.as_deref().map_or(Json::Null, Json::from)),
        ("check", Json::from(req.check)),
        ("fresh", Json::from(r.fresh)),
        ("errors", Json::from(r.errors())),
        ("warnings", Json::from(r.warnings())),
        // Of the warnings: the module's own sources breaking the determinism rules.
        ("determinism_warnings", Json::from(r.determinism_warnings())),
        ("fixable", Json::from(r.fixable())),
        ("diagnostics", Json::arr(shown)),
    ];
    if r.diagnostics.len() > MAX_DIAGNOSTICS {
        f.push(("diagnostics_omitted", Json::from(r.diagnostics.len() - MAX_DIAGNOSTICS)));
    }
    if !r.fixes_applied.is_empty() {
        let applied = r.fixes_applied.iter().map(|a| {
            let file = a.file.strip_prefix(base).unwrap_or(&a.file).to_string_lossy().replace('\\', "/");
            Json::obj([
                ("file", Json::from(file)),
                ("line", Json::from(a.line)),
                ("column", Json::from(a.column)),
                ("replacement", Json::from(a.replacement.as_str())),
                ("message", Json::from(a.message.as_str())),
            ])
        });
        f.push(("fixes_applied", Json::arr(applied)));
    }
    if let Some((code, message)) = &r.error {
        f.push(("error", Json::obj([("code", Json::from(*code)), ("message", Json::from(message.as_str()))])));
        if r.diagnostics.is_empty() && !r.stderr_tail.is_empty() {
            f.push(("stderr_tail", Json::arr(r.stderr_tail.iter().map(|l| Json::from(l.as_str())))));
        }
    }
    if let Some(lib) = &r.library {
        f.push(("library", Json::from(lib.display().to_string())));
    }
    f.push(("requests_merged", Json::from(r.requests)));
    f.push(("command", Json::from(r.command.as_str())));
    Json::obj(f)
}

/// `logic_build`, on the calling connection's thread: build (queued behind other builds), then
/// hand the reload to the engine thread and wait for it.
pub fn build(to_engine: &Sender<Request>, session: &SessionInfo, args: &Json) -> ToolOutput {
    let t0 = Instant::now();
    let parsed = (|| -> Result<(BuildRequest, bool, bool, Option<u64>), String> {
        check_keys(args, "logic_build", &[&["path", "check", "fix", "release", "reload", "detailed", "seed"]])?;
        let path = string_arg(args, "path")?.ok_or("'path' is required: the module crate's folder, e.g. games/ambulo")?;
        let dir = project_path(path, "path")?;
        if !dir.join("Cargo.toml").is_file() {
            return Err(format!("{path} has no Cargo.toml: 'path' is a module crate's folder"));
        }
        // What the build may use (path crates under the root, no build scripts, proc-macros,
        // registry crates or folder-level Cargo config) is checked by the build service against
        // `cargo metadata` before Cargo builds anything.
        let mut req = BuildRequest::new(dir);
        req.roots = vec![engine_root()];
        req.check = bool_arg(args, "check")?.unwrap_or(false);
        req.fix = bool_arg(args, "fix")?.unwrap_or(false);
        req.release = bool_arg(args, "release")?.unwrap_or(false);
        let reload = bool_arg(args, "reload")?.unwrap_or(true) && !req.check;
        Ok((req, reload, bool_arg(args, "detailed")?.unwrap_or(false), seed_arg(args)?))
    })();
    let (req, reload, detailed, seed) = match parsed {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(argument_error(&e).to_pretty()),
    };
    // Fixes write the module's sources: when they are project files, others' claims on them
    // apply, and the fixes become the caller's journaled change (not an edit by `disk`).
    if req.fix {
        let (gate, refused) = project_call(to_engine, session, crate::project_tools::ProjectCall::FixGate { dir: req.dir.clone() });
        if refused {
            return ToolOutput::error(gate.to_string());
        }
    }
    let report = build_service().build(req.clone());
    let mut out = report_json(&report, &req, detailed);
    if req.fix {
        let (adopted, refused) = project_call(to_engine, session, crate::project_tools::ProjectCall::Adopt { dir: req.dir.clone() });
        out.set(if refused { "fixes_refused" } else { "fixes_journaled" }, adopted);
    }
    let build_ms = report.build_time.as_secs_f64() * 1000.0;
    let queued_ms = report.queued.as_secs_f64() * 1000.0;
    let mut reload_ms = 0.0;
    let mut failed = !report.ok;
    if let (true, Some(library)) = (report.ok && reload, report.library.clone()) {
        let t = Instant::now();
        let (loaded, is_error) = load_library(to_engine, session, library, Some(req.dir.clone()), seed);
        reload_ms = t.elapsed().as_secs_f64() * 1000.0;
        failed |= is_error;
        out.set(if is_error { "reload_error" } else { "reload" }, loaded);
    }
    out.set(
        "timings_ms",
        Json::obj([
            ("queued", Json::from(queued_ms)),
            ("build", Json::from(build_ms)),
            ("reload", Json::from(reload_ms)),
            ("total", Json::from(t0.elapsed().as_secs_f64() * 1000.0)),
        ]),
    );
    if failed {
        ToolOutput::error(out.to_string())
    } else {
        ToolOutput::text(out.to_string())
    }
}

/// Sends a project command to the engine thread: `(answer as JSON, is_error)`.
fn project_call(to_engine: &Sender<Request>, session: &SessionInfo, call: crate::project_tools::ProjectCall) -> (Json, bool) {
    let (reply, answer) = mpsc::channel();
    let request = Request { session: session.clone(), command: Command::Project(call), reply, deadline: Instant::now() + RELOAD_TIMEOUT };
    let out = if to_engine.send(request).is_err() {
        ToolOutput::error("the engine is shutting down")
    } else {
        answer.recv_timeout(RELOAD_TIMEOUT + Duration::from_secs(5)).unwrap_or_else(|_| ToolOutput::error("the engine didn't answer in time"))
    };
    let text = out.content.iter().find_map(|c| match c {
        specula_protocol::Content::Text(t) => Some(t.clone()),
        _ => None,
    });
    (text.as_deref().and_then(|t| Json::parse(t).ok()).unwrap_or(Json::Null), out.is_error)
}

/// Sends a command to the engine thread and waits for its answer: `(answer as JSON, is_error)`.
fn engine_call(to_engine: &Sender<Request>, session: &SessionInfo, call: LogicCall) -> (Json, bool) {
    let (reply, answer) = mpsc::channel();
    let request = Request { session: session.clone(), command: Command::Logic(call), reply, deadline: Instant::now() + RELOAD_TIMEOUT };
    let out = if to_engine.send(request).is_err() {
        ToolOutput::error("the engine is shutting down; nothing was loaded")
    } else {
        answer.recv_timeout(RELOAD_TIMEOUT + Duration::from_secs(5)).unwrap_or_else(|_| ToolOutput::error("the engine didn't answer in time; nothing was loaded"))
    };
    let text = out.content.iter().find_map(|c| match c {
        specula_protocol::Content::Text(t) => Some(t.clone()),
        _ => None,
    });
    let json = text.as_deref().and_then(|t| Json::parse(t).ok()).unwrap_or_else(|| Json::from(text.unwrap_or_default()));
    (json, out.is_error)
}

/// Opens `library` here (the caller's thread) and has the engine swap it in.
fn load_library(to_engine: &Sender<Request>, session: &SessionInfo, library: PathBuf, dir: Option<PathBuf>, seed: Option<u64>) -> (Json, bool) {
    let t = Instant::now();
    let code = match open(&library) {
        Ok(c) => c,
        Err(e) => return (e.to_json(), true),
    };
    let open_ms = t.elapsed().as_secs_f64() * 1000.0;
    let origin = Origin { dir, library: Some(library) };
    engine_call(to_engine, session, LogicCall::Load { code, origin, seed, open_ms })
}

/// `logic_reload`, on the calling connection's thread (see the module docs).
pub fn reload(to_engine: &Sender<Request>, session: &SessionInfo, args: &Json) -> ToolOutput {
    // (module, library, seed)
    type Parsed = (Option<String>, Option<PathBuf>, Option<u64>);
    let parsed = (|| -> Result<Parsed, String> {
        check_keys(args, "logic_reload", &[&["module", "library", "seed"]])?;
        let module = string_arg(args, "module")?.map(str::to_owned);
        let library = string_arg(args, "library")?.map(|l| project_path(l, "library")).transpose()?;
        if module.is_none() && library.is_none() {
            return Err("give 'module' (reload from where it was loaded) or 'library' (a module library to load)".to_owned());
        }
        Ok((module, library, seed_arg(args)?))
    })();
    let (module, library, seed) = match parsed {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(argument_error(&e).to_pretty()),
    };
    let (library, dir) = match (module, library) {
        (_, Some(lib)) => (lib, None),
        (Some(m), None) => {
            let (origin, is_error) = engine_call(to_engine, session, LogicCall::Origin { module: m.clone() });
            if is_error {
                return ToolOutput::error(origin.to_pretty());
            }
            match origin.get("library").and_then(Json::as_str) {
                Some(lib) => (PathBuf::from(lib), origin.get("dir").and_then(Json::as_str).map(PathBuf::from)),
                None => {
                    let e = WorldError::new("no_library", format!("module '{m}' wasn't loaded from a library file; give 'library'")).with("module", m.as_str());
                    return ToolOutput::error(e.to_json().to_pretty());
                }
            }
        }
        (None, None) => unreachable!("checked above"),
    };
    let (out, is_error) = load_library(to_engine, session, library, dir, seed);
    if is_error {
        ToolOutput::error(out.to_pretty())
    } else {
        ToolOutput::text(out.to_string())
    }
}

/// `logic_status` also shows the build service (answered on the engine thread, which has no
/// other access to it).
pub fn builds_json() -> Json {
    let svc = build_service();
    let (running, queued) = svc.load();
    let recent = svc.recent().into_iter().rev().take(8).map(|b| {
        Json::obj([
            ("path", Json::from(b.dir.display().to_string())),
            ("ok", Json::from(b.ok)),
            ("check", Json::from(b.check)),
            ("build_ms", Json::from(b.build_ms)),
            ("requests", Json::from(b.requests)),
            ("errors", Json::from(b.errors)),
            ("warnings", Json::from(b.warnings)),
        ])
    });
    Json::obj([("running", Json::from(running)), ("queued", Json::from(queued)), ("recent", Json::arr(recent))])
}

/// A journal for tests: drops the changes.
#[cfg(test)]
pub(crate) fn no_journal() -> impl FnMut(&mut World, Mark, &str, Phase) -> Result<(), WorldError> {
    |w: &mut World, _: Mark, _: &str, _: Phase| {
        w.take_changes();
        Ok(())
    }
}

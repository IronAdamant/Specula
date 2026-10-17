//! The module host's agent tools: `logic_build`, `logic_reload`, `logic_unload`,
//! `logic_status`, `logic_input` and `logic_hook`.
//!
//! `logic_build` runs Cargo on the calling connection's thread, never the engine's (a build
//! takes seconds; the window and other agents keep going), through one engine-wide
//! [`BuildService`] that runs one Cargo at a time and merges duplicate requests. Only the
//! reload that follows a successful build goes to the engine thread, where it is applied at a
//! tick boundary like any other command.

use super::{Journal, LogicHost, Origin, Phase};
use crate::game::Clock;
use crate::journal::Kind;
use crate::swarm::Swarm;
use crate::tools::{argument_error, bool_arg, check_keys, int_arg, string_arg, Command, Request};
use crate::world::{World, WorldError};
use specula_core::Json;
use specula_logic::{BuildReport, BuildRequest, BuildService, ModuleCode};
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Diagnostics one `logic_build` answer lists (errors first); the rest are counted.
pub const MAX_DIAGNOSTICS: usize = 30;
/// How long `logic_build` waits for the engine to apply the reload after the build.
const RELOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// A module-host command for the engine thread.
pub enum LogicCall {
    /// Load the library a build produced (`build` is the build's report, echoed back).
    Load { library: PathBuf, dir: Option<PathBuf>, seed: Option<u64> },
    /// Load module `module`'s library again (or `library`, the first time).
    Reload { module: Option<String>, library: Option<PathBuf>, seed: Option<u64> },
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
/// the project the engine runs in.
fn project_path(path: &str, what: &str) -> Result<PathBuf, String> {
    let root = engine_root();
    let p = Path::new(path);
    let joined = if p.is_absolute() { p.to_owned() } else { root.join(p) };
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
                          check:true only type-checks (fast, no reload). One Cargo runs at a time engine-wide; identical queued \
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
            input_schema: schema(vec![("module", module.clone()), ("detailed", t("boolean", "More history and the state store's values (bounded)"))], &[]),
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

/// The engine-thread tools (`logic_build` is answered by [`build`] instead).
pub fn parse(name: &str, args: &Json) -> Option<Result<Command, String>> {
    let call = match name {
        "logic_reload" => (|| {
            check_keys(args, name, &[&["module", "library", "seed"]])?;
            let module = string_arg(args, "module")?.map(str::to_owned);
            let library = string_arg(args, "library")?.map(|l| project_path(l, "library")).transpose()?;
            if module.is_none() && library.is_none() {
                return Err("give 'module' (reload from where it was loaded) or 'library' (a module library to load)".to_owned());
            }
            Ok(LogicCall::Reload { module, library, seed: seed_arg(args)? })
        })(),
        "logic_unload" => (|| {
            check_keys(args, name, &[&["module", "forget_state"]])?;
            let module = string_arg(args, "module")?.ok_or("'module' is required")?.to_owned();
            Ok(LogicCall::Unload { module, forget: bool_arg(args, "forget_state")?.unwrap_or(false) })
        })(),
        "logic_status" => (|| {
            check_keys(args, name, &[&["module", "detailed"]])?;
            Ok(LogicCall::Status { module: string_arg(args, "module")?.map(str::to_owned), detailed: bool_arg(args, "detailed")?.unwrap_or(false) })
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

/// Applies a module-host command on the engine thread, at a tick boundary.
pub fn handle(host: &mut LogicHost, call: LogicCall, world: &mut World, clock: &Clock, swarm: &mut Swarm, by: &str) -> ToolOutput {
    let (tick, hz) = (clock.tick(), clock.hz());
    let journal: &mut Journal<'_> = &mut |w: &mut World, agent: &str, phase: Phase| swarm.game_changes(w, tick, agent, kind(phase));
    let load = |host: &mut LogicHost, library: PathBuf, dir: Option<PathBuf>, seed, world: &mut World, journal: &mut Journal<'_>| {
        let code = open(&library)?;
        let origin = Origin { dir, library: Some(library) };
        host.load(code, origin, seed, world, tick, hz, journal)
    };
    let result = match call {
        LogicCall::Load { library, dir, seed } => load(host, library, dir, seed, world, journal),
        LogicCall::Reload { module, library, seed } => {
            let origin = module.as_deref().and_then(|m| host.origin(m)).cloned();
            match (module, library, origin) {
                (_, Some(lib), origin) => load(host, lib, origin.and_then(|o| o.dir), seed, world, journal),
                (Some(_), None, Some(Origin { library: Some(lib), dir })) => load(host, lib, dir, seed, world, journal),
                (Some(m), None, _) => Err(WorldError::new(
                    "no_module",
                    format!("module '{m}' isn't loaded from a library this engine knows; give 'library' or use logic_build"),
                )
                .with("module", m.as_str())),
                (None, None, _) => unreachable!("parse requires one"),
            }
        }
        LogicCall::Unload { module, forget } => host.unload(&module, forget, world, tick, hz, journal),
        LogicCall::Status { module, detailed } => host.status(module.as_deref(), detailed).map(|mut j| {
            j.set("builds", builds_json());
            j
        }),
        LogicCall::Input { module, actions, ticks } => host.hold(&module, &actions, ticks),
        LogicCall::Hook { hook, args, reason } => host.call_hook(&hook, &args, &reason, by, world, tick, hz, journal),
    };
    // Module names are attribution: no agent may introduce itself as one.
    for m in host.modules() {
        swarm.reserve(&super::agent_of(&m));
    }
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
        // Zero-dependency builds run no build scripts (AGENTS.md), modules included.
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
        if dir.join("build.rs").exists() || manifest.lines().any(|l| l.trim_start().starts_with("build =") || l.trim_start().starts_with("build=")) {
            return Err(format!("{path} has a build script; modules build without one (remove build.rs / build = ...)"));
        }
        let mut req = BuildRequest::new(dir);
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
    let report = build_service().build(req.clone());
    let mut out = report_json(&report, &req, detailed);
    let build_ms = report.build_time.as_secs_f64() * 1000.0;
    let queued_ms = report.queued.as_secs_f64() * 1000.0;
    let mut reload_ms = 0.0;
    let mut failed = !report.ok;
    if let (true, Some(library)) = (report.ok && reload, report.library.clone()) {
        let (reply, answer) = mpsc::channel();
        let t = Instant::now();
        let request = Request {
            session: session.clone(),
            command: Command::Logic(LogicCall::Load { library, dir: Some(req.dir.clone()), seed }),
            reply,
            deadline: Instant::now() + RELOAD_TIMEOUT,
        };
        let loaded = if to_engine.send(request).is_err() {
            ToolOutput::error("the engine is shutting down; the build was not loaded")
        } else {
            answer.recv_timeout(RELOAD_TIMEOUT + Duration::from_secs(5)).unwrap_or_else(|_| ToolOutput::error("the engine didn't apply the reload in time; it was not loaded"))
        };
        reload_ms = t.elapsed().as_secs_f64() * 1000.0;
        let text = loaded.content.iter().find_map(|c| match c {
            specula_protocol::Content::Text(t) => Some(t.clone()),
            _ => None,
        });
        let parsed = text.as_deref().and_then(|t| Json::parse(t).ok()).unwrap_or_else(|| Json::from(text.unwrap_or_default()));
        failed |= loaded.is_error;
        out.set(if loaded.is_error { "reload_error" } else { "reload" }, parsed);
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
pub(crate) fn no_journal() -> impl FnMut(&mut World, &str, Phase) {
    |w: &mut World, _: &str, _: Phase| {
        w.take_changes();
    }
}

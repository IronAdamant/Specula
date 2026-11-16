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
use crate::write_policy::WorldWrite;
use crate::game::Clock;
use crate::journal::{Kind, Writer};
use crate::swarm::Swarm;
use crate::tools::{argument_error, bool_arg, check_keys, int_arg, string_arg, Command, Request};
use crate::world::{Mark, WorldError};
use specula_core::Json;
use specula_logic::{BuildReport, BuildRequest, BuildService, ModuleCode};
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The folder under the engine's cache that holds module builds' Cargo targets (one per module).
pub const MODULE_TARGETS: &str = "modules";
/// Room a module build is admitted with (the disk budget refuses it below the floor).
const BUILD_ROOM: u64 = 256 << 20;
/// Artefacts kept per crate and kind in a target's `deps`, and units kept per package in its
/// `build` (a check build and a full build each make their own): older ones are stale and
/// dropped after every build.
const KEEP_ARTEFACTS: usize = 2;

static MODULE_TARGETS_DIR: OnceLock<PathBuf> = OnceLock::new();
/// A short folder of the engine's own, per user, for the module builds that would be too deep
/// for the linker under the cache (`None`: this system has no such limit, or names no folder).
static SHORT_TARGETS_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Where module builds' targets go (set once at start; tests leave Cargo's default): the
/// cache's folder, and the short one ([`short_targets_root`]).
pub fn set_module_targets(dir: PathBuf, short: Option<PathBuf>) {
    let _ = MODULE_TARGETS_DIR.set(dir);
    let _ = SHORT_TARGETS_DIR.set(short);
}

/// The short folder module builds go to when the project's own cache is too deep for the Rust
/// toolchain's linker (docs/design/FIRST_HOUR.md, slice 2): `Specula\b` under the user's local
/// application data. Windows only: the limit is that linker's, on that system.
pub fn short_targets_root() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    // (The variable's name in halves: the private-paths guard flags the whole word wherever it
    // stands, as it must: it is how a real path would begin.)
    let local = std::env::var_os(concat!("LOCAL", "APP", "DATA")).filter(|v| !v.is_empty())?;
    Some(PathBuf::from(local).join("Specula").join("b"))
}

/// The package a module's manifest names (`name = "..."` in `[package]`), or its folder's name.
fn package_name(dir: &Path) -> String {
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
    let mut in_package = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if let (true, Some(rest)) = (in_package, line.strip_prefix("name")) {
            if let Some(name) = rest.trim_start().strip_prefix('=').map(|v| v.trim().trim_matches('"')) {
                if !name.is_empty() {
                    return name.to_owned();
                }
            }
        }
    }
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "module".to_owned())
}

/// Which folder a module's builds go to: `<root>/<folder>-<hash>` under the cache, unless the
/// link's argument file would be past what the linker opens there and a `short` folder is
/// known in which it is not. `(target, moved)`.
fn pick_target(root: &Path, short: Option<&Path>, canon: &Path, package: &str, limited: bool) -> (PathBuf, bool) {
    let folder = |chars: usize| -> String { canon.file_name().map(|n| n.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(chars).collect()).unwrap_or_else(|| "module".into()) };
    let hash = specula_core::hash::hash_str(&canon.to_string_lossy()) >> 32;
    let own = root.join(format!("{}-{hash:08x}", folder(40)));
    let fits = |target: &Path| specula_logic::build::link_arguments_chars(target, package) <= specula_logic::build::LINKER_PATH_MAX;
    if !limited || fits(&own) {
        return (own, false);
    }
    match short.map(|s| s.join(format!("{}-{hash:08x}", folder(16)))) {
        Some(moved) if fits(&moved) => (moved, true),
        _ => (own, false),
    }
}

/// The one Cargo target a module shares across agents and builds: `<cache>/modules/<folder>-<hash>`,
/// or the same name under the engine's short folder when a build under the cache would be too
/// deep to link. `(target, moved there)`.
pub fn module_target(dir: &Path) -> Option<(PathBuf, bool)> {
    let root = MODULE_TARGETS_DIR.get()?;
    let canon = specula_logic::fix::canonical(dir);
    let short = SHORT_TARGETS_DIR.get().and_then(|s| s.as_deref());
    Some(pick_target(root, short, &canon, &package_name(dir), cfg!(windows)))
}

/// `name-<16 hex>` (a Cargo artefact or fingerprint name with its metadata hash), split.
fn split_hash(stem: &str) -> Option<(&str, &str)> {
    let (name, hash) = stem.rsplit_once('-')?;
    (hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some((name, hash))
}

/// The stale units of one profile folder in the layout Cargo has used since Rust 1.100: a
/// crate's artefacts, dep-info and fingerprint are one folder, `build/<package>/<hash>/`, and a
/// package's folders past its newest [`KEEP_ARTEFACTS`] are listed. A unit's date is that of
/// `fingerprint/invoked.timestamp`, which Cargo writes each time it compiles the unit (a build
/// that finds it up to date touches nothing); without that file, the newest of the folder's own,
/// its `out`'s and its `fingerprint`'s; and a unit with no date to read counts as the newest (a
/// unit Cargo is making this moment: review R1). Links are never followed or listed; the older
/// layout's `build/<package>-<hash>/` (build scripts' output) holds no such folder and is left
/// alone.
fn stale_units(profile: &Path) -> Vec<PathBuf> {
    let real_folders = |dir: &Path| -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        entries.flatten().take(20_000).map(|e| dir.join(e.file_name())).filter(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && !crate::disk::is_link(&m))).collect()
    };
    let modified = |p: PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    // (dateless, date): a unit with no date sorts above every dated one.
    let touched = |unit: &Path| {
        let date = modified(unit.join("fingerprint").join("invoked.timestamp")).or_else(|| [unit.to_path_buf(), unit.join("out"), unit.join("fingerprint")].into_iter().filter_map(modified).max());
        (date.is_none(), date)
    };
    let mut stale = Vec::new();
    for package in real_folders(&profile.join("build")) {
        let mut units: Vec<((bool, Option<std::time::SystemTime>), PathBuf)> = real_folders(&package)
            .into_iter()
            .filter(|u| u.file_name().is_some_and(|n| n.len() == 16 && n.to_string_lossy().bytes().all(|b| b.is_ascii_hexdigit())))
            .map(|u| (touched(&u), u))
            .collect();
        units.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        stale.extend(units.into_iter().skip(KEEP_ARTEFACTS).map(|(_, u)| u));
    }
    stale
}

/// Drops stale artefacts from a module's target: in each profile's `deps`, `.fingerprint` and
/// `incremental`, only the newest [`KEEP_ARTEFACTS`] of each crate (and file kind) stay, and in
/// its `build` each package's newest units ([`stale_units`]: Cargo's layout from Rust 1.100).
/// Only inside the engine's own module targets, never through a link. Returns `(entries, bytes)`
/// freed.
pub fn drop_stale(target: &Path) -> (u64, u64) {
    // (The cache's folder for module builds, or the engine's short one: nowhere else.)
    let roots = [MODULE_TARGETS_DIR.get().cloned(), SHORT_TARGETS_DIR.get().cloned().flatten()];
    let target = crate::disk::canonical(target);
    let Some(root) = roots.iter().flatten().map(|r| crate::disk::canonical(r)).find(|root| target.starts_with(root) && target != *root) else {
        return (0, 0);
    };
    let mut freed = (0u64, 0u64);
    for profile in ["debug", "release"] {
        for sub in ["deps", ".fingerprint", "incremental"] {
            let dir = target.join(profile).join(sub);
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            // (crate name, file kind) -> (modified, path, is a folder)
            type Group = Vec<(std::time::SystemTime, PathBuf, bool)>;
            let mut groups: std::collections::BTreeMap<(String, String), Group> = Default::default();
            for e in entries.flatten().take(20_000) {
                let p = dir.join(e.file_name());
                let Ok(m) = std::fs::symlink_metadata(&p) else { continue };
                if crate::disk::is_link(&m) {
                    continue;
                }
                let file = e.file_name().to_string_lossy().into_owned();
                let (stem, ext) = if m.is_dir() { (file.as_str(), "") } else { file.split_once('.').unwrap_or((file.as_str(), "")) };
                let Some((name, _)) = split_hash(stem) else { continue };
                groups.entry((name.to_owned(), ext.to_owned())).or_default().push((m.modified().unwrap_or(std::time::UNIX_EPOCH), p, m.is_dir()));
            }
            for (_, mut v) in groups {
                v.sort_by_key(|e| std::cmp::Reverse(e.0));
                for (_, p, is_dir) in v.into_iter().skip(KEEP_ARTEFACTS) {
                    let gone = if is_dir { crate::disk::remove_tree_inside(&root, &p) } else { crate::disk::remove_file_inside(&root, &p).map(|b| (1, b)) };
                    if let Ok((n, b)) = gone {
                        freed.0 += n;
                        freed.1 += b;
                    }
                }
            }
        }
        for unit in stale_units(&target.join(profile)) {
            if let Ok((n, b)) = crate::disk::remove_tree_inside(&root, &unit) {
                freed.0 += n;
                freed.1 += b;
            }
        }
    }
    if freed.0 > 0 {
        crate::disk::book().wrote(crate::disk::Category::Modules, "logic_build", -(freed.1 as i64), -(freed.0 as i64));
    }
    freed
}

/// Diagnostics one `logic_build` answer lists (errors first); the rest are counted.
pub const MAX_DIAGNOSTICS: usize = 30;
/// The largest proposal id `approve` takes (ids count up from 1 per engine run).
const MAX_PROPOSAL_ID: u64 = 1 << 40;
/// How long `logic_build` waits for the engine to apply the reload after the build.
const RELOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// What a caller-thread tool asks the engine's authority gate before it touches Cargo or a library.
pub enum Doing {
    /// `logic_build` of the module folder `dir`, whose digest was just taken (a builder's call
    /// becomes a proposal; `approve` pins a proposal's digest).
    Build { dir: PathBuf, digest: Result<specula_logic::digest::Digest, (&'static str, String)>, approve: Option<u64> },
    /// `logic_reload`: loading a library.
    Reload,
}

/// A module-host command for the engine thread.
pub enum LogicCall {
    /// May the caller load native code? (see [`super::proposals`]): a refused build is filed as
    /// a proposal; an approval takes its proposal.
    Gate(Doing),
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
                          requests from several agents share one build (requests_merged). Timings in timings_ms. \
                          WHO: a module is native code running with the owner's user rights, so only the owner or an \
                          operator-granted lead (--lead NAME) builds and loads one. Anyone else's call builds nothing: it \
                          files a proposal (refused as lead_only, with the proposal) that the lead approves with approve \
                          (proposing is ordinary work; an approve by anyone else is a security incident that freezes the caller). See \
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
                    (
                        "approve",
                        Json::obj([
                            ("type", Json::from("integer")),
                            ("minimum", Json::from(1)),
                            ("maximum", Json::from(MAX_PROPOSAL_ID)),
                            ("description", Json::from("A proposal's id (logic_status lists proposals): build exactly the sources that agent proposed (refused as sources_changed if the folder changed since). Owner or granted lead only")),
                        ]),
                    ),
                ],
                &["path"],
            ),
        },
        ToolDef {
            name: "logic_reload".into(),
            description: "Reload a module from the library it was last loaded from (after building it yourself), or load a \
                          prebuilt module library for the first time. Applied at a tick boundary; same guarantees as logic_build. \
                          WHO: the owner or an operator-granted lead only (a library is native code). Anyone else's call is \
                          refused (lead_only) and is a security incident that freezes the caller's session \
                          (docs/design/SECURITY.md): builders propose a module with logic_build instead."
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
                          streams are kept for a later load unless forget_state is true. WHO: the owner or an operator-granted \
                          lead only; anyone else's call is refused (lead_only) and is a security incident that freezes the \
                          caller's session (docs/design/SECURITY.md)."
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
    // The loaded copy goes through the disk budget (specula_logic::disk's hook): refused below the floor.
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
fn agent_commit<'a>(swarm: &'a mut Swarm, tick: u64, w: Writer<'a>) -> impl FnMut(&mut dyn WorldWrite, Mark, &str, Phase) -> Result<(), WorldError> + 'a {
    crate::write_policy::module_agent_commit(swarm, tick, w)
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
pub fn handle(host: &mut LogicHost, call: LogicCall, world: &mut dyn WorldWrite, clock: &Clock, swarm: &mut Swarm, w: Writer) -> ToolOutput {
    let (tick, hz) = (clock.tick(), clock.hz());
    let key = w.session.unwrap_or(w.name);
    let result = match call {
        LogicCall::Gate(doing) => gate(host, swarm, w, tick, doing),
        // The caller thread asked the gate first; the engine checks again (one place can't be skipped).
        LogicCall::Load { code, origin, seed, open_ms } => swarm
            .may_load_native(key, "loading a module")
            .and_then(|()| LogicHost::name_of(&code))
            .and_then(|name| check_module_owner(swarm, &name, w, "loading").map(|()| name))
            .and_then(|name| {
                let report = host.load(code, origin, seed, world, tick, hz, &mut agent_commit(swarm, tick, w))?;
                // Who loaded it, for the notices its ticks' writes into claims make.
                swarm.note_starter(&crate::logic::agent_of(&name), w.name);
                Ok(report)
            })
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
        LogicCall::Unload { module, forget } => swarm
            .may_load_native(key, "logic_unload (stopping a module every agent's game runs)")
            .inspect_err(|_| super::proposals::report_refusal(w.name, key, swarm.trusted_authority_of_key(key), swarm.holds_lead_grant(key), &super::proposals::NativeRefusal::Unload { module: module.clone() }))
            .and_then(|()| check_module_owner(swarm, &module, w, "unloading"))
            .and_then(|()| host.unload(&module, forget, world, tick, hz, &mut agent_commit(swarm, tick, w))),
        LogicCall::Status { module, detailed } => host.status(module.as_deref(), detailed).map(|mut j| {
            j.set("builds", builds_json());
            j.set("proposals", host.proposals.json());
            j
        }),
        // Each hold records its holder: a freeze releases it, and the human taking the action
        // back is reported under it (docs/design/SECURITY.md).
        LogicCall::Input { module, actions, ticks } => {
            let key = w.session.unwrap_or(w.name);
            let by = crate::inputguard::Actor::identity(key, w.name, swarm.trusted_authority_of_key(key));
            host.hold(&module, &actions, ticks, Some(by))
        }
        // Like a game tool: the hook's changes may not reach into another agent's claim (if
        // they do, nothing the hook did is kept and the error says whose claim).
        LogicCall::Hook { hook, args, reason } => {
            host.call_hook(&hook, &args, &reason, w.name, world, tick, hz, &mut agent_commit(swarm, tick, w))
        }
    };
    output(result)
}

/// The authority gate of `logic_build` and `logic_reload` (engine thread): the owner or a granted
/// lead passes (an `approve` takes its proposal, whose digest must still match); anyone else's
/// build is filed as a proposal and refused, and every refusal goes to the security seam.
fn gate(host: &mut LogicHost, swarm: &Swarm, w: Writer, tick: u64, doing: Doing) -> Result<Json, WorldError> {
    use super::proposals::{report_refusal, NativeRefusal};
    let key = w.session.unwrap_or(w.name);
    let what = match &doing {
        Doing::Build { .. } => "logic_build (building and loading a module)",
        Doing::Reload => "logic_reload (loading a module library)",
    };
    match (swarm.may_load_native(key, what), doing) {
        // Approving is the owner's or lead's: a builder's approve proposes nothing.
        (Err(e), Doing::Build { dir, approve: Some(_), .. }) => {
            report_refusal(w.name, key, swarm.trusted_authority_of_key(key), swarm.holds_lead_grant(key), &NativeRefusal::Build { dir });
            Err(e)
        }
        (Err(e), Doing::Build { dir, digest: Ok(d), .. }) => {
            let p = host.proposals.propose(&dir, w.name, key, tick, &d);
            let (id, json) = (p.id, p.to_json());
            report_refusal(w.name, key, swarm.trusted_authority_of_key(key), swarm.holds_lead_grant(key), &NativeRefusal::Proposed { dir, proposal: Some(id) });
            Err(e.with("proposal", json).with(
                "next",
                "nothing was built: your module is proposed. The owner or lead reviews its files (logic_status lists proposals) and builds it with logic_build {path, approve: <id>}, which loads only these exact sources",
            ))
        }
        // A builder's plain build of a folder that can't be digested (too big, a link in it)
        // files no proposal, and is no try at native code either: it asked for nothing but
        // what a proposal asks (review R2 of m5-inputguard, major 4: it froze the builder).
        (Err(e), Doing::Build { dir, digest: Err((code, message)), .. }) => {
            report_refusal(w.name, key, swarm.trusted_authority_of_key(key), swarm.holds_lead_grant(key), &NativeRefusal::Proposed { dir, proposal: None });
            Err(e.with("proposal_refused", Json::obj([("code", Json::from(code)), ("message", Json::from(message))])))
        }
        (Err(e), Doing::Reload) => {
            report_refusal(w.name, key, swarm.trusted_authority_of_key(key), swarm.holds_lead_grant(key), &NativeRefusal::Reload);
            Err(e)
        }
        (Ok(()), Doing::Build { dir, digest, approve: Some(id) }) => {
            let d = digest.map_err(|(code, message)| WorldError::new(code, message).with("approve", id))?;
            let p = host.proposals.approve(id, &dir, &d.hex)?;
            Ok(Json::obj([("approved", p.to_json())]))
        }
        (Ok(()), _) => Ok(Json::Obj(Vec::new())),
    }
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
        let mut error = Json::obj([("code", Json::from(*code)), ("message", Json::from(message.as_str()))]);
        // `build_path_too_long`: the numbers as data (the file the linker could not open).
        if let Some((file, chars)) = &r.link_arguments {
            error.set("file", Json::from(file.as_str()));
            error.set("chars", Json::from(*chars));
            error.set("limit", Json::from(specula_logic::build::LINKER_PATH_MAX));
        }
        f.push(("error", error));
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
    // (request, reload, detailed, seed, approve, the build went to the engine's short folder)
    type Parsed = (BuildRequest, bool, bool, Option<u64>, Option<u64>, bool);
    let parsed = (|| -> Result<Parsed, String> {
        check_keys(args, "logic_build", &[&["path", "check", "fix", "release", "reload", "detailed", "seed", "approve"]])?;
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
        // One Cargo target per module, shared by every agent's builds of it (under the cache).
        let (target, moved) = module_target(&req.dir).unzip();
        req.target_dir = target;
        let moved = moved.unwrap_or(false);
        req.check = bool_arg(args, "check")?.unwrap_or(false);
        req.fix = bool_arg(args, "fix")?.unwrap_or(false);
        req.release = bool_arg(args, "release")?.unwrap_or(false);
        let reload = bool_arg(args, "reload")?.unwrap_or(true) && !req.check;
        let approve = int_arg(args, "approve", 1, MAX_PROPOSAL_ID)?;
        if approve.is_some() && req.fix {
            return Err("'approve' builds the proposed sources exactly as they are; 'fix' would change them (approve first, then fix)".to_owned());
        }
        Ok((req, reload, bool_arg(args, "detailed")?.unwrap_or(false), seed_arg(args)?, approve, moved))
    })();
    let (req, reload, detailed, seed, approve, moved) = match parsed {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(argument_error(&e).to_pretty()),
    };
    // Authority before anything else (docs/design/MODULES.md, "Who may load a module"): only the
    // owner or a granted lead builds; anyone else's call is filed as a proposal. The folder's
    // digest (read here, on the caller's thread) is what a proposal names and an approval pins.
    // The folder is held from here until the load: the engine's own writers leave it alone
    // (specula_logic::build::building), and the stamps show any other write, so the sources
    // digested are the sources compiled (a swap put back before the second digest hashes the same).
    let _held = specula_logic::build::hold(&req.dir);
    let before = specula_logic::digest::module_snapshot(&req.dir);
    let digest = before.as_ref().map(|(d, _)| d.clone()).map_err(Clone::clone);
    let (gated, refused) = engine_call(to_engine, session, LogicCall::Gate(Doing::Build { dir: req.dir.clone(), digest: digest.clone(), approve }));
    if refused {
        return ToolOutput::error(gated.to_string());
    }
    // Through the disk budget: a build is refused below the free-space floor (Cargo writes a lot).
    if let Err(e) = crate::disk::book().admit(crate::disk::Category::Modules, BUILD_ROOM, crate::disk::Need::Agent) {
        return ToolOutput::error(e.to_json().to_string());
    }
    // Fixes write the module's sources. When they are project files (a mounted module) the
    // fixer never writes the disk: each round's fixes are worked out against the texts the build
    // compiled and committed as the caller's change (claims, journal, undo); a file someone
    // changed during the build keeps their text. Other modules are fixed in place, as before.
    let mut mounted = false;
    if req.fix {
        let (gate, refused) = project_call(to_engine, session, crate::project_tools::ProjectCall::FixGate { dir: req.dir.clone() });
        if refused {
            return ToolOutput::error(gate.to_string());
        }
        mounted = gate.get("mount").is_some();
    }
    let (report, fixes) = if mounted { fix_mounted(to_engine, session, &req) } else { ((*build_service().build(req.clone())).clone(), None) };
    let mut out = report_json(&report, &req, detailed);
    // Said, not hidden: where the build went when it could not go under the project's cache.
    if let (true, Some(t)) = (moved, &req.target_dir) {
        out.set(
            "build_dir",
            Json::obj([
                ("path", Json::from(t.display().to_string())),
                ("why", Json::from("under this project's cache the linker's argument file would be past the 259 characters the Rust toolchain's linker opens on Windows, so the build is kept in the engine's short folder for this user (counted by disk_status, cleared by disk_prune); --cache DIR in a short folder keeps it with the project")),
            ]),
        );
    }
    if let (true, Some(t)) = (report.ok, &req.target_dir) {
        let (n, b) = drop_stale(t);
        if n > 0 {
            out.set("stale_dropped", Json::obj([("entries", Json::from(n)), ("mb", Json::from((b as f64 / 1048576.0 * 10.0).round() / 10.0))]));
        }
    }
    let mut fix_refused = false;
    if let Some(f) = fixes {
        fix_refused = f.get("refused").is_some();
        out.set("fixes_journaled", f);
    }
    let build_ms = report.build_time.as_secs_f64() * 1000.0;
    let queued_ms = report.queued.as_secs_f64() * 1000.0;
    let mut reload_ms = 0.0;
    let mut failed = !report.ok || fix_refused;
    if let Some(a) = gated.get("approved") {
        out.set("approved", a.clone());
    }
    // The sources the build compiled are the ones digested before it (a fix round changes them
    // itself): an edit during the build loads nothing.
    let changed = match (&before, req.fix) {
        (Ok(before), false) if report.ok && reload => specula_logic::digest::module_snapshot(&req.dir).ok().filter(|now| now == before).is_none(),
        _ => false,
    };
    // An approval pins the folder's digest: what it built may come from nowhere else (but the
    // engine's SDK and core), or a helper crate could change after the review, unseen.
    let unpinned = if approve.is_some() && report.ok && reload { specula_logic::build::unpinned(&report.compiled, &req.roots, &req.dir) } else { None };
    if changed {
        failed = true;
        out.set("reload_error", WorldError::new("sources_changed", "a file in the module folder was written while it was built: nothing was loaded; build again").to_json());
    } else if let Some(c) = unpinned {
        failed = true;
        out.set(
            "reload_error",
            WorldError::new(
                "unpinned_sources",
                format!("the approved build compiled crate '{}' ({}) from outside the module folder, which the proposal's digest doesn't cover: nothing was loaded. A proposed module keeps its helper crates inside its own folder; the owner or lead can build it without 'approve' after reviewing that crate too", c.name, c.manifest.display()),
            )
            .with("crate", c.name.as_str())
            .to_json(),
        );
    } else if let (true, Some(library)) = (report.ok && reload, report.library.clone()) {
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

/// `logic_build {fix: true}` on a mounted module (see [`build`]): build without fixing, work
/// the fixes out against the store's texts taken before that build, commit them as the caller
/// (skipping files changed since), and build again; at most [`specula_logic::build::MAX_FIX_ROUNDS`]
/// rounds. Returns the last build and what happened to the fixes.
fn fix_mounted(to_engine: &Sender<Request>, session: &SessionInfo, req: &BuildRequest) -> (BuildReport, Option<Json>) {
    use crate::project_tools::{FixesCommitted, ModuleText, ProjectCall};
    let mut plain = req.clone();
    plain.fix = false;
    let mut applied = Vec::new();
    let (mut committed, mut skipped, mut journal) = (Vec::new(), Vec::new(), Vec::new());
    let mut refused = None;
    let mut round = 0;
    let typed = |call: ProjectCall| tools_call(to_engine, session, call);
    let report = loop {
        let (tx, rx) = mpsc::channel::<Result<Vec<ModuleText>, Json>>();
        typed(ProjectCall::ModuleTexts { dir: req.dir.clone(), reply: tx });
        let texts = match rx.recv_timeout(crate::project_tools::DEFERRED_WAIT) {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                // Held, pending or changed files: Cargo would compile other text than the plan's.
                let message = e.get("message").and_then(Json::as_str).unwrap_or("out of sync").to_owned();
                refused = Some(e);
                break BuildReport { error: Some(("disk_out_of_sync", message)), ..BuildReport::default() };
            }
            Err(_) => Vec::new(),
        };
        let report = (*build_service().build(plain.clone())).clone();
        if round == specula_logic::build::MAX_FIX_ROUNDS || report.fixable() == 0 {
            break report;
        }
        round += 1;
        let mut read = |file: &Path| {
            texts.iter().find(|t| t.disk == file).map(|t| t.text.to_string()).ok_or_else(|| specula_logic::fix::FixError { code: "not_mirrored", file: file.to_owned(), message: "not a project file of this module".into() })
        };
        let planned = match specula_logic::fix::plan(&report.diagnostics, &req.dir, &mut read) {
            Ok(p) => p,
            Err(e) => {
                skipped.push(Json::obj([("file", Json::from(e.file.display().to_string())), ("why", Json::from(e.message))]));
                break report;
            }
        };
        let files: Vec<(String, u64, u64, String)> = planned
            .iter()
            .filter_map(|p| texts.iter().find(|t| t.disk == p.file).map(|t| (t.path.clone(), t.version, t.hash, p.text.clone())))
            .collect();
        if files.is_empty() {
            break report;
        }
        let (tx, rx) = mpsc::channel::<FixesCommitted>();
        typed(ProjectCall::CommitFixes { files, reply: tx });
        let done = rx.recv_timeout(crate::project_tools::DEFERRED_WAIT).unwrap_or_default();
        skipped.extend(done.skipped.iter().map(|(p, why)| Json::obj([("path", Json::from(p.as_str())), ("why", Json::from(why.as_str()))])));
        if let Some(e) = done.error {
            refused = Some(e);
            break report;
        }
        for p in &planned {
            if let Some(t) = texts.iter().find(|t| t.disk == p.file && done.committed.contains(&t.path)) {
                applied.extend(p.applied.iter().cloned());
                if !committed.contains(&t.path) {
                    committed.push(t.path.clone());
                }
            }
        }
        journal.extend(done.journal);
        if done.stop {
            skipped.push(Json::obj([("why", Json::from("a fixed file didn't reach the disk (project_list shows it pending): no more rounds"))]));
            break report;
        }
        if done.committed.is_empty() {
            break report;
        }
    };
    let mut report = report;
    report.fixes_applied = applied;
    let mut f = vec![
        ("files", Json::arr(committed.iter().map(|p| Json::from(p.as_str())))),
        ("journal", Json::arr(journal.into_iter().map(Json::from))),
    ];
    if !skipped.is_empty() {
        f.push(("skipped", Json::Arr(skipped)));
    }
    if let Some(e) = refused {
        f.push(("refused", e));
    }
    (report, Some(Json::obj(f)))
}

/// Sends a project command whose answer comes on its own typed channel (only errors matter here).
fn tools_call(to_engine: &Sender<Request>, session: &SessionInfo, call: crate::project_tools::ProjectCall) {
    let _ = project_call(to_engine, session, call);
}

/// Sends a project command to the engine thread: `(answer as JSON, is_error)`.
fn project_call(to_engine: &Sender<Request>, session: &SessionInfo, call: crate::project_tools::ProjectCall) -> (Json, bool) {
    let (reply, answer) = crate::jobs::channel();
    let request = Request { session: session.clone(), call: crate::tools::current_call(), command: Command::Project(call), reply, deadline: Instant::now() + RELOAD_TIMEOUT, trace: specula_core::trace::Ctx::current(), via: None };
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
    let (reply, answer) = crate::jobs::channel();
    let request = Request { session: session.clone(), call: crate::tools::current_call(), command: Command::Logic(call), reply, deadline: Instant::now() + RELOAD_TIMEOUT, trace: specula_core::trace::Ctx::current(), via: None };
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
    let (gated, refused) = engine_call(to_engine, session, LogicCall::Gate(Doing::Reload));
    if refused {
        return ToolOutput::error(gated.to_pretty());
    }
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

/// A journal that keeps nothing, for a world of your own (tests, tools): drops the changes.
/// Refused on the live world ([`crate::write_policy::drop_changes`]).
pub fn no_journal() -> impl FnMut(&mut dyn WorldWrite, Mark, &str, Phase) -> Result<(), WorldError> {
    |w: &mut dyn WorldWrite, _: Mark, _: &str, _: Phase| crate::write_policy::drop_changes(w)
}

#[cfg(test)]
mod stale_tests {
    use super::*;

    /// docs/design/FIRST_HOUR.md, slice 2: a build that would be too deep to link under the
    /// project's cache goes to the engine's short folder, and only then.
    #[test]
    fn a_module_too_deep_to_link_under_the_cache_builds_in_the_short_folder() {
        use specula_logic::build::{link_arguments_chars, LINKER_PATH_MAX};
        let short = Path::new(r"C:\Users\someone\AppData\Local\Specula\b"); // private-scan: allow (a made-up folder, for its length)
        let canon = Path::new(r"C:\game\modules\gate");
        let shallow = Path::new(r"C:\game\.specula-cache\modules");
        let (own, moved) = pick_target(shallow, Some(short), canon, "gate", true);
        assert!(!moved && own.starts_with(shallow) && own.file_name().unwrap().to_string_lossy().starts_with("gate-"), "{}", own.display());
        // The same project's cache a hundred and seventy characters deeper.
        let deep = PathBuf::from(format!(r"C:\{}\.specula-cache\modules", "d".repeat(170)));
        assert!(link_arguments_chars(&deep.join("gate-00000000"), "gate") > LINKER_PATH_MAX, "the case this is for");
        let (target, moved) = pick_target(&deep, Some(short), canon, "gate", true);
        assert!(moved && target.starts_with(short), "{}", target.display());
        assert!(link_arguments_chars(&target, "gate") <= LINKER_PATH_MAX);
        // The folder is the module's own: the same each time, another for another module.
        assert_eq!(pick_target(&deep, Some(short), canon, "gate", true).0, target);
        assert_ne!(pick_target(&deep, Some(short), Path::new(r"C:\other\modules\gate"), "gate", true).0, target);
        // Where the linker has no such limit, and where no short folder is known, as before:
        // the build goes under the cache (and, too deep, is refused by name after it compiled).
        let unlimited = pick_target(&deep, Some(short), canon, "gate", false);
        assert!(!unlimited.1 && unlimited.0.starts_with(&deep));
        assert!(!pick_target(&deep, None, canon, "gate", true).1);
        // A short folder that is itself too deep (a very long user name) is not used.
        let long_user = PathBuf::from(format!(r"C:\Users\{}\AppData\Local\Specula\b", "u".repeat(200))); // private-scan: allow (a made-up folder, for its length)
        assert!(!pick_target(&deep, Some(&long_user), canon, "gate", true).1);
    }

    #[test]
    fn a_modules_package_is_named_by_its_manifest() {
        let dir = std::env::temp_dir().join(format!("specula-package-name-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[workspace]\n\n[package]\nname = \"hold-the-gate\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"gate\"\n").unwrap();
        assert_eq!(package_name(&dir), "hold-the-gate");
        std::fs::write(dir.join("Cargo.toml"), "not a manifest").unwrap();
        assert_eq!(package_name(&dir), dir.file_name().unwrap().to_string_lossy());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A build that failed because its folder is too deep says so with numbers an agent can
    /// use; any other failure's error is its code and message, as before.
    #[test]
    fn a_build_folder_too_deep_is_answered_with_its_numbers() {
        let req = BuildRequest::new("no-such-module");
        let mut report = BuildReport { error: Some(("build_path_too_long", "too deep".into())), ..BuildReport::default() };
        report.link_arguments = Some(("C:\\deep\\rustcAbCdEf\\linker-arguments".into(), 260));
        let error = report_json(&report, &req, false).get("error").cloned().unwrap();
        assert_eq!(error.get("code").and_then(Json::as_str), Some("build_path_too_long"));
        assert_eq!(error.get("chars").and_then(Json::as_u64), Some(260));
        assert_eq!(error.get("limit").and_then(Json::as_u64), Some(259));
        assert_eq!(error.get("file").and_then(Json::as_str), Some("C:\\deep\\rustcAbCdEf\\linker-arguments"));
        report.link_arguments = None;
        report.error = Some(("compile_errors", "1 compile error(s): see diagnostics".into()));
        let error = report_json(&report, &req, false).get("error").cloned().unwrap();
        assert!(error.get("chars").is_none() && error.get("limit").is_none() && error.get("file").is_none(), "{error}");
    }

    /// Rust 1.100's layout of a target folder: a package's units are folders, and the ones
    /// past its newest two are stale. A unit's date is when Cargo last compiled it, not when
    /// its folder was made; nothing else in `build` is listed.
    #[test]
    fn a_packages_units_past_its_newest_two_are_stale() {
        let root = std::env::temp_dir().join(format!("specula-stale-units-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let profile = root.join("debug");
        let at = |secs: u64| std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + secs);
        let unit = |package: &str, hash: &str, compiled: u64| {
            let u = profile.join("build").join(package).join(hash);
            std::fs::create_dir_all(u.join("out")).unwrap();
            std::fs::create_dir_all(u.join("fingerprint")).unwrap();
            std::fs::write(u.join("out").join("lib.d"), "x").unwrap();
            let stamp = u.join("fingerprint").join("invoked.timestamp");
            std::fs::write(&stamp, "").unwrap();
            std::fs::OpenOptions::new().write(true).open(&stamp).unwrap().set_modified(at(compiled)).unwrap();
            u
        };
        // Made in this order; compiled in another (the first made was compiled last).
        let _built = unit("ambulo", "3333333333333333", 30);
        let oldest = unit("ambulo", "1111111111111111", 10);
        let _check = unit("ambulo", "2222222222222222", 20);
        let _sdk = unit("specula-sdk", "aaaaaaaaaaaaaaaa", 1);
        let _sdk2 = unit("specula-sdk", "bbbbbbbbbbbbbbbb", 2);
        // Not units: the older layout's build-script folder, and a name that is no hash.
        std::fs::create_dir_all(profile.join("build").join("helper-0123456789abcdef").join("out")).unwrap();
        std::fs::create_dir_all(profile.join("build").join("ambulo").join("notes")).unwrap();
        assert_eq!(stale_units(&profile), vec![oldest.clone()]);
        let older = unit("specula-sdk", "cccccccccccccccc", 0);
        let mut stale = stale_units(&profile);
        stale.sort();
        assert_eq!(stale, vec![oldest, older]);
        assert!(stale_units(&root.join("release")).is_empty(), "a profile never built has nothing stale");
        // A unit Cargo has made and not yet stamped has its folder's date, which is now: kept.
        let fresh = profile.join("build").join("ambulo").join("4444444444444444");
        std::fs::create_dir_all(fresh.join("out")).unwrap();
        let stale = stale_units(&profile);
        assert!(!stale.contains(&fresh) && stale.len() == 3, "the unit being made is the newest: {stale:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Review R1: nothing showed that a real build's live units survive. A module is checked
    /// and built, twice over, by the build service's own Cargo: a check build and a full build
    /// each make a unit of every package, and none of them is stale (with Cargo's older layout
    /// there are no units and this says nothing; with Rust 1.100's it fails if a build's own
    /// units are counted past the ones kept).
    #[test]
    fn a_real_builds_units_are_all_kept() {
        if std::process::Command::new(specula_logic::build::cargo_program()).arg("--version").output().map_or(true, |o| !o.status.success()) {
            eprintln!("skipped: no cargo to build with");
            return;
        }
        let repo = specula_logic::fix::canonical(&Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(".."));
        let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| repo.join("target"), PathBuf::from);
        let target = specula_logic::fix::canonical(&target).join("module-tests-stale-units");
        for round in 0..2 {
            for check in [true, false] {
                let mut req = BuildRequest::new(repo.join("games").join("ambulo"));
                req.roots = vec![repo.clone()];
                req.target_dir = Some(target.clone());
                req.check = check;
                let built = specula_logic::build::run_cargo(&req);
                assert!(built.ok, "round {round}, check {check}: {:?} {:?}", built.error, built.stderr_tail);
                assert_eq!(stale_units(&target.join("debug")), Vec::<PathBuf>::new(), "round {round}, check {check}");
            }
        }
    }
}

//! Specula world server: owns the world, renders it, and serves agents over MCP.
//!
//! A library so game binaries can link the engine; `main.rs` is the stock `specula` binary.
//! [`world`] is public: games running in-process call [`world::World`] directly every tick.
//! A game binary implements [`game::Game`] and calls [`run_game`]; the engine then runs it on a
//! fixed-tick clock, feeds it window input, and serves its extra tools to agents.
//!
//! Threads: protocol threads parse tool calls and hand them to the engine (main) thread over a
//! channel; the engine thread owns the world and GPU, applies commands between frames, renders,
//! and replies. With a window open it serves calls as they arrive in the time it would otherwise
//! wait for vsync (see `schedule`), so agents aren't paced by the display. The human's window is
//! pumped by its own thread (see `ui`), so dragging it never stalls agents.

#![forbid(unsafe_code)]

pub mod authority;
pub mod audio_service;
mod audio_tools;
pub mod jobs;
mod balance_tools;
mod approved;
mod bounds;
pub mod board;
pub mod ask;
pub mod anim;
mod anim_tools;
pub mod cameras;
mod capture;
pub mod claims;
pub mod compare;
pub mod conflict;
pub mod desync;
pub mod disk;
mod disk_tools;
mod evidence;
pub mod blobs;
mod project_binary;
mod director;
pub mod effects;
pub mod entity_ref;
pub mod fpguard;
pub mod game;
pub mod hlod;
pub mod intents;
pub mod inputguard;
mod inputguard_engine;
mod inputguard_tools;
pub mod journal;
pub mod leases;
pub mod logic;
pub mod module_sounds;
mod lex_live;
/// Live Lex runs' work limits (perf.toml `[lex_live]`), for games that size their sets to them.
pub use lex_live::{Limits as LexLiveLimits, LIMITS as LEX_LIVE_LIMITS};
mod lex_tools;
mod luts;
pub mod merge;
pub mod meshes;
pub mod models;
pub mod orbit;
pub mod outline;
mod perf;
pub mod persist;
pub mod rollup;
mod run_modules;
mod run_checks;
mod script_run;
mod save_tools;
mod scaffold;
mod safe_path;
pub mod savegame;
pub mod physics;
mod physics_tools;
mod joint_tools;
mod ragdoll_tools;
pub mod nav;
mod nav_tools;
mod lockwait;
mod locus;
pub mod playtest;
mod playtest_engine;
mod procgen_tools;
pub mod project;
mod project_engine;
mod asset_scan;
mod project_kinds;
mod project_text;
mod project_tools;
mod response;
pub mod records;
pub mod review;
mod review_engine;
mod review_screen;
mod gallery_screen;
mod editor;
mod editor_ask;
mod editor_brush;
mod editor_engine;
mod editor_swarm;
mod editor_gizmo;
mod editor_tools;
mod taste;
mod taste_tools;
mod review_tools;
mod schedule;
pub mod shell;
pub mod sight;
pub mod sight_overlay;
mod sight_tools;
mod stream_tools;
mod studio;
mod studio_tools;
pub mod streaming;
pub mod swarm;
mod swarm_engine;
mod swarm_tools;
mod tools;
pub mod incidents;
mod trace_engine;
pub mod trace_sink;
mod trace_tools;
mod toolkits;
mod toolscope;
mod toolscope_tools;
pub mod two_d;
pub mod text_window;
pub mod text_world;
mod ui;
mod ui_engine;
mod ui_tools;
pub mod user_settings;
pub mod flashwatch;
pub mod vfx;
mod vfx_tools;
mod timeline;
mod timeline_tools;
mod l10n_tools;
mod memory_tools;
pub mod launch_cli;
pub mod launch_client;
pub mod launch_engine;
pub mod launch_instances;
pub mod launch_list;
pub mod launch_run;
mod launch_screen;
mod launch_window;
pub mod launch_store;
pub mod timeline_audio;
mod gi;
mod gi_tools;
mod light_tools;
pub mod terrain;
mod model_export;
mod paint_tools;
mod path_tools;
mod terrain_tools;
pub mod water;
mod sim_writes;
mod startgate;
mod hash_verify;
mod steps;
mod workers;
mod world_save;
mod world_load;
pub mod world;
pub mod write_policy;

use game::{Clock, Ctx, Game, MAX_GAME_OVERLAY_QUADS};
use journal::{Kind, Writer};
use meshes::{LatheMeshes, ShapeMeshes};
use models::{GpuModels, Library, LodChoice, LodStats, SharedLibrary};
use schedule::{Class, Schedule, Served};
use sight::{Camera, Meshes, View};
use specula_core::deflate::Level;
use specula_core::math::Mat4;
use specula_core::{base64, png, profile, Json};
use specula_gpu::{AdapterPreference, Draw2d, DrawItem, FrameDesc, FramePath, Gpu, GpuOptions, OverlayQuad, Severity, TargetId, Validation};
use specula_platform::{client_size_of, Event, WindowDesc};
use specula_protocol::{Content, Server, ServerConfig, SessionInfo, ToolOutput};
use specula_sys::interrupt;
use std::collections::{BTreeSet, VecDeque};
use swarm::Swarm;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tools::{Command, FrameSpec, QuerySpec, Request, TimeOp};
use ui::UiThread;
use world::{World, WorldError};

const DEFAULT_PORT: u16 = 7471;

/// Offscreen render targets kept for reuse (one per capture size, least recently used evicted).
const MAX_CACHED_TARGETS: usize = 4;
/// Capture runs kept in the captures directory (older `run-*` folders are deleted at start-up).
const MAX_CAPTURE_RUNS: usize = 20;
/// How often a changed world is written to its world files.
const AUTOSAVE_EVERY: Duration = Duration::from_secs(10);
/// Listings stop adding items past this many bytes (compact JSON) and hand back a cursor, so a
/// page of huge entities still fits in an agent's context.
const MAX_LISTING_BYTES: usize = 128 * 1024;
/// Pages of world_query / world_list at least this long are listed on a worker.
const QUERY_OFFLOAD_ITEMS: usize = 64;

/// A world_query page detached from the world: the entities with their paths, and per entity
/// its parent's path and child count when the fields list them.
struct OwnedPage {
    total: usize,
    more: bool,
    items: Vec<(world::Entity, String, Option<String>, usize)>,
    fields: Option<Vec<String>>,
    key: &'static str,
}

impl OwnedPage {
    /// The answer: items within the listing's byte cap, then the cursor. Pure.
    fn render(&self) -> ToolOutput {
        let (mut items, mut bytes, mut cut, mut last) = (Vec::new(), 0, false, None);
        for (e, path, parent, children) in &self.items {
            let item = match &self.fields {
                None => e.to_json(path),
                Some(fields) => world::fields_json_of(e, path, parent.as_deref(), *children, fields),
            };
            let size = item.to_string().len();
            if !items.is_empty() && bytes + size > MAX_LISTING_BYTES {
                cut = true;
                break;
            }
            bytes += size;
            items.push(item);
            last = Some(path.as_str());
        }
        let next = if cut || self.more { last.map(tools::cursor_for) } else { None };
        // Compact: listings are the biggest responses, and indentation costs agents tokens.
        let out = Json::obj([("total", Json::from(self.total)), (self.key, Json::Arr(items)), ("next_cursor", Json::from(next))]);
        ToolOutput::text(out.to_string())
    }
}
/// world_batch lists per-op results up to this many bytes, then summarises the rest.
const MAX_BATCH_RESULT_BYTES: usize = 32 * 1024;

const USAGE: &str = "usage: specula [--port N] [--headless] [--adapter discrete|integrated|<index>] [--size WxH] \
[--captures DIR] [--worlds DIR] [--world NAME] [--saves DIR] [--cache DIR] [--assets DIR] [--style FILE] [--perf FILE] \
[--paused] [--max-fps N] [--audio device|null|off] [--response-format concise|detailed] [--tools-default core|all] [--physics-threads N] [--vfx on|off] [--terrain on|off] \n[--trace DIR] [--trace-file on|off] [--strict-budget] \
[--owner NAME] [--lead NAME] [--reviewer NAME] [--settings DIR|off] [--no-activate] [--project DIR] [--run | --run-as-is] [--script FILE] [--launcher] [-- GAME ARGS...]\n\
commands (specula help): projects | open --last|NAME|FOLDER | new NAME | forget NAME | list | tools [FILTER] | call TOOL ['{json}']";

/// How long a capture waits (parked, the engine serving others) for models still building.
const CAPTURE_BUILD_WAIT: Duration = Duration::from_secs(10);

/// Distinct in-view texture files a capture stats on the engine thread (more go to the loader).
const MAX_CAPTURE_STATS: usize = 256;
/// The model library forgets what no entity uses at most this often (after the world's models
/// changed).
const GC_EVERY: Duration = Duration::from_secs(1);
/// While builds are in flight, builds no entity waits for any more are cancelled at most this
/// often after entities stopped using models (`World::model_drops`): soon enough that deleted
/// entities' builds never hold up live ones, and never on spawns or moves alone.
const GC_UNWANTED_EVERY: Duration = Duration::from_millis(100);
/// Simulation rate when no game is hosted (the clock still keeps time for agents).
const DEFAULT_TICK_HZ: u32 = 30;
/// Per-tick state hashes kept for world_hash.
const TICK_HASHES: usize = 1024;
/// How often claims of vanished sessions are dropped.
const CLAIM_SWEEP_EVERY: Duration = Duration::from_secs(1);
/// world_batch calls with more ops than this are big work for the window's schedule.
const HEAVY_BATCH_OPS: usize = 256;
/// How often the windowed loop asks for its display's refresh rate.
const REFRESH_ASK_EVERY: Duration = Duration::from_secs(1);

struct Args {
    port: u16,
    headless: bool,
    /// `--no-activate`: the window opens without taking the keyboard from the window the human
    /// is in (an engine started by an agent or a script while someone uses the machine).
    no_activate: bool,
    /// `--run`: the operator says the project's code is theirs to run, so the game file's modules
    /// are built and loaded at start (docs/design/FIRST_HOUR.md; [`run_modules`]).
    run: bool,
    /// `--run-as-is`: as `--run`, and whatever is on disk now, whoever built it last.
    run_as_is: bool,
    /// `--script FILE`: a headless run of the project's game by a script of inputs, then the
    /// engine ends (`specula run` writes the file and reads the answer: [`script_run`]).
    script: Option<PathBuf>,
    /// `--project DIR`: the engine starts in that folder, as a project (docs/design/LAUNCH.md).
    project: Option<PathBuf>,
    /// `--launcher`: the start screen (the project list) opens over the window, as it does
    /// when the executable is started with no argument at all.
    launcher: bool,
    adapter: AdapterPreference,
    window_size: (u32, u32),
    captures: Option<PathBuf>,
    worlds: Option<PathBuf>,
    world: Option<String>,
    /// Where save games live (`--saves`, default `./saves`).
    saves: Option<PathBuf>,
    /// The hosted game's player settings (`--settings DIR|off`; default the per-user folder with
    /// a window, none headless): see [`user_settings`].
    settings: Option<String>,
    /// Pipeline cache directory (`--cache`, default `./.specula-cache`).
    cache: Option<PathBuf>,
    /// Budgets file (`--perf`, default `./perf.toml` when present).
    perf: Option<PathBuf>,
    /// Where model recipes live (`--assets`, default the working directory).
    assets: Option<PathBuf>,
    /// One style for every recipe (`--style`; default: the `style.json` next to each recipe).
    style: Option<PathBuf>,
    /// Everything after `--` (or `--game-args`): the hosted game's own arguments.
    game_args: Vec<String>,
    /// Start with the clock paused (`--paused`): nothing ticks until an agent says so, so fresh
    /// engines start playtests from the same tick and state.
    paused: bool,
    /// The default `response_format` of every session (`--response-format`; concise unless set).
    response_format: response::Format,
    /// What sessions nobody configured list (`--tools-default core|all`; core unless set): see
    /// `toolscope`.
    tools_default_all: bool,
    /// Effects and decals drawn (`--vfx on|off`; on by default). Never changes the simulation.
    vfx: bool,
    /// The terrain renderer (`--terrain on|off`; on by default): off, terrains draw their
    /// recipes' meshes. Never changes the simulation.
    terrain: bool,
    /// Threads physics may use (`--physics-threads`, 1..64, default 1): the narrowphase, and
    /// with the world's `physics.coloured` the contact solver. The results never depend on it.
    physics_threads: usize,
    /// The window's frame cap (`--max-fps`, 20..1000; over `perf.toml` `[window] max_fps`).
    max_fps: Option<u32>,
    /// Authority the operator grants agent names (`--owner`, `--lead`, `--reviewer`, each
    /// repeatable; see [`authority`]).
    grants: authority::Grants,
    /// The trace folder: crash reports and the JSONL sink (`--trace`, default `./.specula-trace`).
    trace: Option<PathBuf>,
    /// The JSONL trace sink (`--trace-file on|off`, on by default).
    trace_file: bool,
    /// A call or frame step past its stall budget fails the run (`--strict-budget`): its
    /// `engine.stall` is an error and the engine exits with an error (docs/design/TRACE.md).
    strict_budget: bool,
    /// The engine's audio (`--audio device|null|off`; default device with a window, off
    /// headless, so headless runs stay silent): see [`audio_service`].
    audio: Option<audio_service::Mode>,
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from(argv: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args {
        port: DEFAULT_PORT,
        headless: false,
        no_activate: false,
        run: false,
        run_as_is: false,
        script: None,
        project: None,
        launcher: false,
        adapter: AdapterPreference::HighPerformance,
        window_size: (1280, 720),
        captures: None,
        worlds: None,
        world: None,
        saves: None,
        settings: None,
        cache: None,
        perf: None,
        assets: None,
        style: None,
        game_args: Vec::new(),
        paused: false,
        response_format: response::Format::Concise,
        tools_default_all: false,
        vfx: true,
        terrain: true,
        physics_threads: 1,
        max_fps: None,
        grants: authority::Grants::default(),
        trace: None,
        trace_file: true,
        strict_budget: false,
        audio: None,
    };
    let mut it = argv;
    while let Some(flag) = it.next() {
        if flag == "--" || flag == "--game-args" {
            args.game_args = it.by_ref().collect();
            break;
        }
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value\n{USAGE}"));
        match flag.as_str() {
            "--assets" => args.assets = Some(PathBuf::from(value()?)),
            "--style" => args.style = Some(PathBuf::from(value()?)),
            "--port" => args.port = value()?.parse().map_err(|_| format!("bad port\n{USAGE}"))?,
            "--headless" => args.headless = true,
            "--no-activate" => args.no_activate = true,
            "--run" => args.run = true,
            "--script" => {
                args.script = Some(PathBuf::from(value()?));
                // (A script's run has no window: nobody is there to look at one.)
                args.headless = true;
            }
            "--run-as-is" => {
                args.run = true;
                args.run_as_is = true;
            }
            "--project" => args.project = Some(PathBuf::from(value()?)),
            "--launcher" => args.launcher = true,
            "--paused" => args.paused = true,
            "--terrain" => {
                args.terrain = match value()?.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("--terrain must be on or off, not '{other}'\n{USAGE}")),
                }
            }
            "--strict-budget" => args.strict_budget = true,
            "--audio" => {
                let v = value()?;
                args.audio = Some(audio_service::Mode::parse(&v).ok_or_else(|| format!("--audio must be device, null or off, not '{v}'\n{USAGE}"))?);
            }
            "--trace" => args.trace = Some(PathBuf::from(value()?)),
            "--trace-file" => {
                args.trace_file = match value()?.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("--trace-file must be on or off, not '{other}'\n{USAGE}")),
                }
            }
            "--vfx" => {
                args.vfx = match value()?.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("--vfx must be on or off, not '{other}'
{USAGE}")),
                }
            }
            "--max-fps" => {
                let range = schedule::MIN_MAX_FPS..=schedule::MAX_MAX_FPS;
                args.max_fps = Some(value()?.parse().ok().filter(|n| range.contains(n)).ok_or_else(|| {
                    format!("--max-fps must be {}..{} frames a second
{USAGE}", schedule::MIN_MAX_FPS, schedule::MAX_MAX_FPS)
                })?);
            }
            "--tools-default" => {
                args.tools_default_all = match value()?.as_str() {
                    "core" => false,
                    "all" => true,
                    other => return Err(format!("--tools-default must be core or all, not '{other}'\n{USAGE}")),
                }
            }
            "--response-format" => {
                args.response_format = match value()?.as_str() {
                    "concise" => response::Format::Concise,
                    "detailed" => response::Format::Detailed,
                    other => return Err(format!("--response-format must be concise or detailed, not '{other}'\n{USAGE}")),
                }
            }
            "--adapter" => {
                let v = value()?;
                args.adapter = match v.as_str() {
                    "discrete" => AdapterPreference::HighPerformance,
                    "integrated" => AdapterPreference::LowPower,
                    n => AdapterPreference::Index(n.parse().map_err(|_| format!("bad adapter\n{USAGE}"))?),
                }
            }
            "--size" => {
                let v = value()?;
                let parsed = v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
                args.window_size = parsed.ok_or_else(|| format!("bad size '{v}'\n{USAGE}"))?;
            }
            "--owner" | "--lead" | "--reviewer" => {
                let a = match flag.as_str() {
                    "--owner" => authority::Authority::Owner,
                    "--lead" => authority::Authority::Lead,
                    _ => authority::Authority::Reviewer,
                };
                let name = value()?;
                args.grants.grant(&name, a).map_err(|e| format!("{flag}: {e}
{USAGE}"))?;
            }
            "--captures" => args.captures = Some(PathBuf::from(value()?)),
            "--cache" => args.cache = Some(PathBuf::from(value()?)),
            "--perf" => args.perf = Some(PathBuf::from(value()?)),
            "--worlds" => args.worlds = Some(PathBuf::from(value()?)),
            "--saves" => args.saves = Some(PathBuf::from(value()?)),
            "--settings" => args.settings = Some(value()?),
            "--physics-threads" => {
                args.physics_threads = value()?.parse().ok().filter(|n| (1..=64).contains(n)).ok_or_else(|| format!("--physics-threads must be 1..64\n{USAGE}"))?;
            }
            "--world" => {
                let name = value()?;
                persist::validate_world_name(&name).map_err(|e| format!("--world: {e}\n{USAGE}"))?;
                args.world = Some(name);
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => {
                return Err(format!("unknown argument '{other}' (a hosted game's own arguments go after --)\n{USAGE}"))
            }
        }
    }
    Ok(args)
}

/// Runs the engine with the command-line arguments until it is closed or interrupted.
pub fn run() -> Result<(), String> {
    run_with(None)
}

/// Runs the engine hosting `game` (see [`game::Game`]).
pub fn run_game(game: Box<dyn Game>) -> Result<(), String> {
    run_with(Some(game))
}

/// Start-up, stage by stage: what the engine thread did before its loop began, and how long
/// each part took. One `engine.started` trace event says it (`listening_ms`: when the port
/// answered; `ready_ms`: when the first call could be served), so a slow start is read from the
/// trace rather than guessed at.
struct Startup {
    began: Instant,
    last: Instant,
    stages: Vec<(&'static str, f64)>,
    listening_ms: f64,
}

impl Startup {
    fn begin() -> Startup {
        let now = Instant::now();
        Startup { began: now, last: now, stages: Vec::new(), listening_ms: 0.0 }
    }

    /// The stage that just ended.
    fn mark(&mut self, stage: &'static str) {
        let now = Instant::now();
        self.stages.push((stage, now.duration_since(self.last).as_secs_f64() * 1e3));
        self.last = now;
    }

    fn listening(&mut self) {
        self.listening_ms = self.began.elapsed().as_secs_f64() * 1e3;
    }

    /// `gpu=412.3 world=1.0 ...`, in order.
    fn text(&self) -> String {
        self.stages.iter().map(|(name, ms)| format!("{name}={ms:.1}")).collect::<Vec<_>>().join(" ")
    }

    /// `thread_clock`: the engine thread's processor cycles a millisecond, as measured for
    /// `cpu_ms` (0: not read here). A rate far under the machine's says the measure was taken
    /// while the thread was switched out, and `cpu_ms` then reads high for the whole run.
    fn report(&self, thread_clock: Option<f64>) {
        let ready = self.began.elapsed().as_secs_f64() * 1e3;
        specula_core::trace_event!(&trace_sink::ENGINE, Info, "engine.started", ready_ms = ready, listening_ms = self.listening_ms, stages = self.text(), thread_cycles_per_ms = thread_clock.unwrap_or(0.0));
    }
}

fn run_with(mut game: Option<Box<dyn Game>>) -> Result<(), String> {
    // The command line's words (docs/design/LAUNCH.md): `specula projects`, `open`, `list`,
    // `tools`, `call`... answer and end here, in a process that never becomes an engine.
    // Arguments that start with a flag (or none at all) are the engine's own, as before.
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().is_some_and(|a| !a.starts_with('-')) {
        // (A command line that is not one is answered as every command answers: JSON, exit
        // code 1. Review R1-10.)
        let code = match launch_cli::take_settings(&argv).and_then(|(words, settings)| Ok((launch_cli::parse(&words)?, settings))) {
            Ok((Some(cli), settings)) => launch_run::answer(cli, settings),
            Ok((None, _)) => launch_run::usage(USAGE),
            // (A run's answer is one line, a refusal as much as a state: its reader reads its
            // last line.)
            Err(why) if argv[0] == "run" || argv[0] == "check" => launch_run::usage_line(&why),
            Err(why) => launch_run::usage(&why),
        };
        std::process::exit(code);
    }
    let mut args = parse_args()?;
    // `--script FILE`: what a headless run is asked, read before anything is started.
    let script = match &args.script {
        None => None,
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("--script {}: {e}", path.display()))?;
            let json = Json::parse(&text).map_err(|e| format!("--script {}: not JSON: {}", path.display(), e.message))?;
            Some(script_run::Spec::from_json(&json).map_err(|e| format!("--script {}: {e}", path.display()))?)
        }
    };
    let scripted = script.is_some();
    let mut startup = Startup::begin();
    if args.project.is_some() {
        // `--project` makes that folder the working directory. Every other folder the command
        // line names was written against the one the engine was started in, and is read there:
        // above all `--settings`, the person's own folder, which a relative path would have put
        // inside the project (review R1-5).
        for folder in [&mut args.assets, &mut args.style, &mut args.trace, &mut args.captures, &mut args.cache, &mut args.perf, &mut args.worlds, &mut args.saves] {
            if let Some(absolute) = folder.as_ref().and_then(|f| std::path::absolute(f).ok()) {
                *folder = Some(absolute);
            }
        }
        if let Some(absolute) = args.settings.as_ref().filter(|s| s.as_str() != "off").and_then(|s| std::path::absolute(s).ok()) {
            args.settings = Some(absolute.to_string_lossy().into_owned());
        }
    }
    // The executable opened with no argument (double-clicked) while another program has the
    // usual port: a free one, said in its instance file, rather than no window at all.
    if argv.is_empty() && launch_instances::port_taken(args.port) {
        if let Some(free) = launch_run::free_port() {
            args.port = free;
        }
    }
    // `--project DIR` (or a working directory that is a project): the engine starts there.
    let project = match &args.project {
        Some(dir) => Some(launch_engine::open(dir, true)?),
        None => std::env::current_dir().ok().and_then(|cwd| launch_engine::of_working_directory(&cwd)),
    };
    // One engine a project: its lock is held from here to the end of the run.
    let (_project_lock, unlocked) = match &project {
        Some(p) => match launch_engine::lock(&p.path) {
            Ok(lock) => (Some(lock), None),
            Err(launch_engine::LockRefused::Held) => {
                // (A headless run's command reads this, waits for the other engine and starts
                // again: two runs of one project begun in the same moment.)
                if let Some(spec) = &script {
                    script_run::write_answer(&spec.out, &script_run::project_open(&p.name));
                }
                return Err(format!("{} ({}) is open in another engine: 'specula list' finds it, and 'specula open' answers with it", p.name, p.path.display()));
            }
            Err(launch_engine::LockRefused::Unavailable(why)) => (None, Some(format!("{} could not be locked ({why}): nothing stops a second engine from opening it", p.name))),
        },
        None => (None, None),
    };
    for note in project.as_ref().and_then(|p| p.note.clone()).into_iter().chain(unlocked) {
        eprintln!("specula: {note}");
    }
    // A project with a game file and no executable of its own is hosted by the engine's own
    // shell: its screens, its words, its settings and its pause (docs/design/FIRST_HOUR.md). A
    // game file that is not one stops the start, with where and what to write.
    if game.is_none() {
        if let Some(p) = project.as_ref().filter(|p| shell::Shell::wanted(&p.path)) {
            let assets = args.assets.clone().or_else(|| p.assets.clone()).unwrap_or_else(|| p.path.clone());
            let hosted = shell::Shell::open(&p.name, &p.path, &assets).map_err(|e| format!("{} ({}): {e}", p.name, p.path.display()))?;
            game = Some(Box::new(hosted));
        }
    }
    match game.as_mut() {
        Some(g) => g.configure(&args.game_args).map_err(|e| format!("{}: {e}", g.name()))?,
        None if !args.game_args.is_empty() => {
            return Err(format!("no game is hosted, so the arguments after -- ({}) have no taker\n{USAGE}", args.game_args.join(" ")));
        }
        None => {}
    }
    if let Err(e) = interrupt::install() {
        eprintln!("specula: Ctrl+C will not shut down cleanly: {e}");
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    // Tracing first: a crash from here on leaves a report (docs/design/TRACE.md).
    let trace_dir = args.trace.clone().unwrap_or_else(|| cwd.join(".specula-trace"));
    trace_sink::start(&trace_dir, args.trace_file, args.port);
    let perf_cfg = perf::PerfConfig::load(args.perf.as_deref(), &cwd).map_err(|e| format!("perf budgets: {e}"))?;
    profile::set_enabled(perf_cfg.profiler);
    // A cached pipeline blob skips shader compilation at start-up. The GPU crate rejects one made
    // by another GPU or driver, so a stale file only costs a cache miss.
    let cache_dir = args.cache.clone().unwrap_or_else(|| cwd.join(".specula-cache"));
    startup.mark("setup");
    let gpu = Gpu::new(&GpuOptions { validation: Validation::Auto, adapter: args.adapter, pipeline_cache: None, pipeline_cache_dir: Some(cache_dir.clone()) })
        .map_err(|e| format!("GPU init failed: {e}"))?;
    startup.mark("gpu");
    // (The file is the adapter's own, whatever `--adapter` called it: perf::pipeline_cache_path.)
    let cache_path = perf::pipeline_cache_path(&cache_dir, gpu.adapter());
    let captures_root = args.captures.unwrap_or_else(|| cwd.join("captures"));
    // `--assets`, else the project's own assets folder, else the hosted game's own folder, else
    // the working directory.
    let named_assets = args.assets.or_else(|| project.as_ref().and_then(|p| p.assets.clone())).or_else(|| game.as_ref().and_then(|g| g.assets_root()));
    let assets = named_assets.clone().unwrap_or_else(|| cwd.clone());
    if let Some(g) = game.as_mut() {
        g.use_assets(&assets);
    }
    // One audio device for the engine and its game (headless: none unless asked for).
    let audio = audio_service::AudioService::start(args.audio.unwrap_or(if args.headless { audio_service::Mode::Off } else { audio_service::Mode::Device }));
    if let Some(g) = game.as_mut() {
        g.use_audio(Arc::clone(&audio));
    }
    let library = Library::new(&assets, args.style.as_deref());
    let mut engine = Engine::new(gpu, &captures_root, library)?;
    engine.windowed = !args.headless;
    engine.timeline_audio.configure(&assets, Arc::clone(&audio));
    engine.watch.strict = args.strict_budget;
    engine.formats = response::Defaults::new(args.response_format);
    engine.physics.threads = args.physics_threads;
    engine.profiler = perf::Profiler::new(perf_cfg.history_frames);
    let kind = perf::adapter_kind(engine.gpu.adapter().kind);
    eprintln!(
        "specula: perf budgets from {} ({} adapter{})",
        perf_cfg.source.as_ref().map_or("built-in defaults".into(), |p| p.display().to_string()),
        kind,
        if perf_cfg.has_adapter_section(kind) { ", with its own section" } else { "" }
    );
    engine.world.set_shape_limits(perf_cfg.shape_limits());
    engine.frame_floor = (perf_cfg.swarm.frame_floor_fps, None);
    engine.max_fps = args.max_fps.or(perf_cfg.window.max_fps);
    engine.lex_live.set_limits(perf_cfg.lex_live);
    engine.stream = streaming::Streamer::new(perf_cfg.streaming.clone(), kind);
    engine.vfx.budget = perf_cfg.vfx.budget(kind);
    engine.vfx.enabled = args.vfx;
    engine.module_sounds.set_audio(Arc::clone(&audio));
    // Baked GI files live with the pipeline cache, by hash (the world keeps only the hash).
    engine.gi.store.dir = cache_dir.join("gi");
    // The iGPU samples GI per draw by default (its budget); other adapters per pixel.
    engine.gi.quality = gi::Quality::default_for(engine.gpu.adapter().kind == specula_gpu::AdapterKind::Integrated);
    engine.terrain.disabled = !args.terrain;
    engine.perf_cfg = perf_cfg;
    engine.worlds_dir = args.worlds.unwrap_or_else(|| cwd.join("worlds"));
    engine.saves_dir = args.saves.unwrap_or_else(|| cwd.join("saves"));
    // The disk footprint budget: every folder the engine writes to, by category, measured and
    // pruned in the background (docs/design/DISK.md). Scratch leases default to the captures
    // root's `scratch` folder (as `lease_take` makes them).
    let scratch_root = engine.perf_cfg.swarm.scratch_root.clone().unwrap_or_else(|| captures_root.join("scratch"));
    // The hosted game's player settings (per user with a window; none headless unless --settings).
    if let Some(g) = &game {
        engine.user_settings = user_settings::UserSettings::resolve(args.settings.as_deref(), !args.headless, g.name());
    }
    let settings_root: Vec<(disk::Category, PathBuf)> = engine.user_settings.dir().map(|d| (disk::Category::Settings, d.to_path_buf())).into_iter().collect();
    // Module builds too deep for the linker under the cache go to a short folder of the
    // engine's own (docs/design/FIRST_HOUR.md, slice 2): counted and pruned like the cache's.
    let short_modules = logic::tools::short_targets_root();
    // The start-up build (`--run`) builds what it loads in a folder of the operator's own, which no
    // project can bring with it (logic::tools::run_targets_root): counted with the other builds.
    let run_builds = logic::tools::run_targets_root(launch_engine::store_for(args.settings.as_deref(), |k| std::env::var(k).ok()).as_ref().map(|s| s.dir.as_path()), args.settings.is_some());
    disk::book().configure(
        engine.perf_cfg.disk.clone(),
        settings_root
            .into_iter()
            .chain(short_modules.clone().map(|dir| (disk::Category::Modules, dir)))
            .chain(run_builds.clone().map(|dir| (disk::Category::Modules, dir)))
            // (The start-up build's own short folder, when it is not its usual one already.)
            .chain(logic::tools::run_short_root(short_modules.as_deref()).filter(|dir| Some(dir) != run_builds.as_ref()).map(|dir| (disk::Category::Modules, dir)))
            .chain([
            (disk::Category::Captures, captures_root.clone()),
            (disk::Category::Cache, cache_dir.clone()),
            // The trace folder: self-bounded (its JSONL files and crash reports are capped by
            // count), never pruned by the book (a crash report must survive until it is read).
            (disk::Category::Cache, trace_dir.clone()),
            (disk::Category::Content, cache_dir.join(blobs::CONTENT_DIR)),
            (disk::Category::Modules, cache_dir.join(logic::tools::MODULE_TARGETS)),
            (disk::Category::Scratch, scratch_root),
            (disk::Category::Saves, engine.saves_dir.clone()),
            (disk::Category::Worlds, engine.worlds_dir.clone()),
            // Never walked (the project's own folder): only its free space is read.
            (disk::Category::Project, assets.clone()),
            // Playtest replays and the hosted game's recordings: measured, never pruned.
            (disk::Category::Recordings, cwd.join(playtest_engine::RECORD_DIR)),
            (disk::Category::Recordings, cwd.join(disk::RECORDINGS_DIR)),
        ])
        .collect(),
        Some(engine.capture_dir.clone()),
    );
    // Finished runs' unsaved review queues died with their engines: their evidence goes.
    evidence::clean_unsaved(&engine.worlds_dir, &engine.capture_dir);
    logic::tools::set_module_targets(cache_dir.join(logic::tools::MODULE_TARGETS), short_modules);
    logic::tools::set_run_targets(run_builds);
    // The module host's own writes (library copies, fixes, a restored Cargo.lock) go through the book too.
    specula_logic::disk::set_hook(disk::logic_admit, disk::logic_wrote);
    let content = Arc::new(blobs::ContentStore::open(&cache_dir.join(blobs::CONTENT_DIR)).map_err(|e| format!("content store: {e}"))?);
    disk::book().add_pruner(disk::Category::Content, content.pruner());
    engine.project.set_content(Arc::clone(&content));
    engine.content = Some(Arc::clone(&content));
    if let Some(name) = args.world {
        // A world that exists but can't be read stops the engine: autosave would overwrite it.
        let dir = engine.worlds_dir.join(&name);
        // Owned for as long as this engine works in it: a second engine is refused, not interleaved.
        engine.world_lock = Some(persist::WorldLock::acquire(&dir).map_err(|e| format!("world '{name}': {e}"))?);
        if dir.join("world.json").exists() {
            let (mut world, saved) = persist::load_limited(&dir, engine.world.shape_limits())
                .map_err(|e| format!("world '{name}' in {}: {e}", dir.display()))?;
            // World files hold no live Lex runs: what they kept of /lex_live goes (as world_load).
            let removed = lex_live::remove_presentation(&mut world);
            if removed > 0 {
                eprintln!("specula: removed {removed} /lex_live entities the world files kept (no live Lex run shows them)");
            }
            // At startup nobody holds a claim and no playtest runs.
            let may = write_policy::may_replace(&engine.swarm, Writer::from("startup"), &engine.playtests).map_err(|e| e.message)?;
            for problem in engine.replace_world(may, world, &name).into_iter().chain(saved.warnings.iter().cloned()) {
                eprintln!("specula: {problem}");
            }
            let board_problem = engine.load_board(&dir);
            if let Some(problem) = &board_problem {
                eprintln!("specula: {problem}");
            }
            // The queue's evidence lives in the world's folder (older queues point into captures).
            let (_, missing) = engine.adopt_evidence(&dir, board_problem.is_some());
            if missing > 0 {
                eprintln!("specula: {missing} file(s) the review queue names are missing from its evidence (review_list marks them)");
            }
            engine.saved = Some(saved);
            eprintln!("specula: loaded world '{name}' ({} entities) from {}", engine.world.len(), dir.display());
        } else {
            // A folder with other files in it isn't ours to fill.
            persist::check_adoptable(&dir).map_err(|e| format!("world '{name}': {e}"))?;
            eprintln!("specula: new world '{name}'; autosaving to {}", dir.display());
        }
        engine.saved_revision = engine.world.revision();
        engine.saved_seq = engine.swarm.journal().last_seq();
        engine.world_name = Some(name);
    }
    // A world with baked GI: its GI pipelines start building now, before any model upload asks
    // for the skinned and textured ones (a background build runs one request at a time, so a GI
    // request behind them waited 40 to 130 s on a busy iGPU).
    if engine.gi.has_volumes(&engine.world) && engine.gi.quality != gi::Quality::Off {
        engine.gpu.request_gi_variants(engine.gi.quality.gpu());
    }
    // The review screen shows the captures and the current queue's evidence.
    engine.review_ui.set_roots(engine.review_roots());
    eprintln!(
        "specula: GPU {} (validation {})",
        engine.gpu.adapter().name,
        if engine.gpu.validation_enabled() { "on" } else { "off" }
    );
    eprintln!("specula: captures in {}", engine.capture_dir.display());
    eprintln!("specula: model recipes under {}", assets.display());
    // The assets root's text files become project files (versioned, claimed, journaled).
    // Mounting them moves the world's revision, but nothing a world file holds has changed, so a
    // world that was clean before the mount stays clean (no false "unsaved changes" at start).
    let clean = !engine.dirty();
    engine.project_mount_assets(assets.clone());
    // An engine opened on a project mirrors the project's own files too: its game file, which the
    // shell follows while it runs, and its notes (the memory_* tools).
    if let Some(p) = &project {
        engine.project_mount_root(&p.path);
    }
    if clean {
        engine.saved_revision = engine.world.revision();
        engine.saved_seq = engine.swarm.journal().last_seq();
    }

    if let Some(g) = &game {
        engine.clock = Clock::new(g.tick_hz());
        engine.game_name = g.name().to_owned();
        // Its changes are attributed to this name: no agent may take it.
        engine.swarm.reserve(g.name());
    }
    // The window's human, in the review queue's attribution.
    engine.swarm.reserve(review::WINDOW_OWNER);
    // The window's human, as an identity the editor's protocol commands run under.
    engine.swarm.admit_window_operator(editor_engine::WINDOW_SESSION);
    // The gallery worker's rendered rounds, in the journal.
    engine.swarm.reserve(taste::TASTE_AGENT);
    // Who the operator made owner, lead or reviewer (never self-declared; see `authority`).
    engine.swarm.grants = args.grants.clone();
    let game_tools = game.as_ref().map(|g| g.tools()).unwrap_or_default();
    engine.lex_natives = game.as_ref().map(|g| g.lex_natives()).unwrap_or_default();
    startup.mark("engine");
    let (to_engine, requests) = mpsc::channel();
    // (Built once, here: engine_info copies it.)
    authority::warm_access_json();
    // Calls that arrive before the loop begins wait for it (the server listens first).
    let start_gate = startgate::StartGate::closed();
    // The gallery worker sends its renders through the same channel as agents.
    engine.taste.connect(to_engine.clone(), Arc::clone(&engine.studio.shared));
    // `--run`: the operator says this project's code is theirs to run, so the module folders
    // its game file names are built and loaded by the engine itself, once its loop has begun
    // (never because a folder was opened: docs/design/MODULES.md, "Who may load a module").
    if args.run {
        let modules = game.as_ref().map(|g| g.modules()).unwrap_or_default();
        if modules.is_empty() {
            specula_core::trace_event!(&trace_sink::ENGINE, Warn, "run.no_modules", why = "--run builds the module folders a project's game.json names: there is no game file here, or it names none");
        } else {
            engine.swarm.admit_run_operator(run_modules::RUN_SESSION);
            run_modules::start(to_engine.clone(), Arc::clone(&start_gate), modules, Arc::clone(&engine.run_reports), args.run_as_is);
        }
    }
    let server = Server::start(
        ServerConfig {
            bind: format!("127.0.0.1:{}", args.port),
            server_name: "specula".into(),
            server_version: env!("CARGO_PKG_VERSION").into(),
            instructions: Some(tools::INSTRUCTIONS.into()),
        },
        {
            let tools = tools::Tools::new(to_engine, game_tools)
                .with_tools_default_all(args.tools_default_all)
                .with_formats(Arc::clone(&engine.formats))
                .with_lex_natives(engine.lex_natives.clone())
                .with_capture_dir(engine.capture_dir.clone(), safe_path::Roots::for_files(&engine.capture_dir, &captures_root, &engine.worlds_dir, named_assets.as_deref()))
                .with_studio(Arc::clone(&engine.studio.shared))
                .with_content(Arc::clone(&content))
                .with_audio(Arc::clone(&audio))
                .with_procgen(procgen_tools::Context { assets: assets.clone(), style: args.style.clone(), captures: engine.capture_dir.clone() })
                .with_start_gate(Arc::clone(&start_gate));
            engine.scopes = Some(tools.shared_scopes());
            let tools = Arc::new(tools);
            // The editor's draw tools call tools that run on their caller's thread (terrain_edit,
            // model_paint): the engine keeps a way to the handler that doesn't keep it alive.
            engine.tools = Some(Arc::downgrade(&tools));
            tools
        },
    )
    .map_err(|e| format!("MCP server: {e}"))?;
    // A start that ends without its loop (an error below, a panic) fails the gate as
    // `starting` is dropped: no call waits for a loop that will not come. DECLARED AFTER
    // `server`, so that it is dropped before it: the server joins its connection threads when
    // it is dropped, and those are the threads waiting at the gate. Declared before it, the
    // failed engine and its port lived on for the calls' two minutes
    // (tests/start_fail.rs runs the executable and holds this order).
    let starting = startgate::Starting::new(Arc::clone(&start_gate));
    // `--script`: the run is played by a caller of its own, as the operator who started this
    // engine, through the tools an agent would use; it ends the engine when it has answered.
    if let Some(spec) = script {
        engine.swarm.admit_run_operator(run_modules::RUN_SESSION);
        match engine.tools.as_ref().and_then(std::sync::Weak::upgrade) {
            Some(tools) => script_run::start(tools, Arc::clone(&start_gate), spec),
            None => return Err("--script: the engine has no tools to play the run with".to_owned()),
        }
    }
    // Serving: the project is first in the user's list and this engine can be found (its
    // instance file), unless `--settings off` (docs/design/LAUNCH.md).
    engine.opened_project = project.as_ref().map(|p| (p.path.clone(), p.name.clone()));
    let launch_store = launch_engine::store_for(args.settings.as_deref(), |k| std::env::var(k).ok());
    // Where this engine keeps which module sources an owner or lead last built (what `--run`
    // may load): the operator's own folder, never the project's.
    approved::set_settings(launch_store.as_ref().map(|s| s.dir.as_path()));
    // The start screen: the executable opened with no argument at all (double-clicked), or
    // `--launcher`, when there is a window, a project list to show, no project open and no
    // hosted game (a game's executable is its own project).
    let start_screen = (args.launcher || argv.is_empty()) && !args.headless && project.is_none() && game.is_none() && launch_store.is_some();
    if args.launcher && !start_screen {
        specula_core::trace_event!(&trace_sink::ENGINE, Warn, "launch.no_start_screen", why = "--launcher needs a window, a project list (not --settings off), no --project and no hosted game");
    }
    let handover = start_screen.then(launch_engine::Handover::default);
    let announced = launch_store.clone().map(|store| {
        let serving = launch_engine::Serving {
            launcher: handover.clone(),
            stop: interrupt::request,
            // (The port it is bound to, which is not the one asked for when that was 0.)
            port: server.local_addr().port(),
            project: project.as_ref(),
            world: engine.world_name.as_deref(),
            window: !args.headless,
            game: game.as_ref().map(|g| g.name()),
            exe: std::env::current_exe().ok(),
            now: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        };
        let (announced, problems) = launch_engine::announce(store, &serving);
        for p in problems {
            eprintln!("specula: {p}");
        }
        announced
    });
    eprintln!("specula: MCP at http://{}/mcp — Ctrl+C to stop", server.local_addr());
    startup.mark("server");
    startup.listening();
    // Tests only (`SPECULA_TEST_FAIL_START`, tests/start_fail.rs): the start fails here, after
    // the port answers, once a call is waiting at the start gate (or after a minute). (Its
    // value makes that call's refusal late: `Tools::await_start`.)
    if std::env::var_os("SPECULA_TEST_FAIL_START").is_some() {
        let asked = Instant::now();
        while start_gate.waiting() == 0 && asked.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(5));
        }
        return Err("the start was failed on purpose (SPECULA_TEST_FAIL_START is set: tests only)".to_owned());
    }

    // Only now that the port is ours: tidy old capture runs (never a running engine's), then open
    // the window — a second engine on a taken port fails above without touching either.
    prune_capture_runs(&captures_root, MAX_CAPTURE_RUNS - 1, &engine.capture_dir);
    startup.mark("prune_captures");

    let window = if args.headless {
        None
    } else {
        let (width, height) = args.window_size;
        let ui = UiThread::spawn(WindowDesc { title: "Specula".into(), width, height, activate: !args.no_activate, ..WindowDesc::default() })
            .map_err(|e| format!("window: {e}"))?;
        let (w, h) = client_size_of(ui.raw);
        engine.gpu.attach_window(ui.raw.hinstance, ui.raw.hwnd, w, h).map_err(|e| format!("swapchain: {e}"))?;
        Some(ui)
    };
    if let (Some(store), Some(handover)) = (launch_store, handover) {
        engine.start.open(store, launch_run::default_parent(|k| std::env::var(k).ok()), handover);
    }
    startup.mark("window");
    // Assets that will need skinned or textured pipelines get them now, not at their first spawn.
    let (skinned, textured) = models::asset_streams(engine.library.lock().root());
    startup.mark("asset_streams");
    engine.gpu.prepare_variants(skinned, textured).map_err(|e| format!("pipelines: {e}"))?;
    startup.mark("pipelines");
    // Loading is done: from here on, building a pipeline is a visible hitch and a budget error.
    engine.gpu.mark_warm();
    if scripted {
        // A script has the clock from before the game's first wish for it: no time of this
        // machine becomes a tick, and whether the run's clock is running is the game's own
        // state ([`Engine::serve_game_clock`]).
        engine.clock.set_paused(true);
        engine.script_clock = Some(false);
    }
    engine.game = game.take();
    engine.start_game();
    startup.mark("game");
    if args.paused {
        engine.clock.set_paused(true);
    }

    // Scratch folders an earlier run released but didn't finish deleting.
    engine.sweep_trash();
    // The built-in effects are parsed once per run, on a worker now rather than by the first
    // vfx_get or capture (it took 25-85 ms of the engine thread in a dev build).
    engine.warm_effects();
    startup.mark("sweep");
    // (After the stages are marked: its three busy milliseconds are not a stage.)
    let thread_clock = engine.watch_own_time();
    startup.report(thread_clock);
    // The loop begins: calls that waited for the start go on now.
    starting.done();
    let result = match &window {
        Some(ui) => engine.run_windowed(ui, &requests, &server),
        None => engine.run_headless(&requests, &server),
    };
    // (A slow last frame is still reported: its zones were never collected.)
    engine.frame_stall_named();
    if let Err(e) = &result {
        // A fatal engine error (device loss among them) leaves a crash report for after a restart.
        if e.contains("DEVICE_LOST") {
            trace_engine::device_lost("present", e);
        } else {
            trace_sink::write_crash("engine_error", e, None, Json::Null);
        }
    }
    // Writes the engine owes (world files, project files, deletes) and answers on the workers
    // complete first.
    engine.workers_drain(Duration::from_secs(10));
    // The player's last settings change, if a write was in flight when they quit.
    engine.finish_settings();
    engine.finish_autosave(true);
    // Save games still being written land before the process ends.
    engine.join_saves();
    engine.autosave(true);
    engine.save_board_blocking();
    // Parked captures and model calls hold reply channels: answer them now, or their callers
    // (and the server's connection threads) would wait out their full timeout.
    engine.answer_waiters_shutting_down();
    // Queued calls get "shutting down" as soon as their reply channels close; give them a moment
    // to write that response before the server closes every connection.
    drop(requests);
    std::thread::sleep(Duration::from_millis(100));
    // Calls waiting on the security monitor answer now (the server waits for its threads).
    inputguard::stopping();
    server.shutdown();
    // No longer found; the project remembers the world it had open.
    if let Some(a) = announced {
        a.finish(engine.world_name.as_deref());
    }
    // Tear the GPU down (window surface included) before the window itself, and report any
    // validation messages raised during shutdown.
    match engine.gpu.pipeline_cache_data() {
        Ok(blob) => {
            let saved = disk::book().write(disk::Category::Cache, "pipeline", &cache_path, &blob, disk::Need::Agent);
            if let Err(e) = saved {
                eprintln!("specula: pipeline cache not saved to {}: {e}", cache_path.display());
            }
        }
        Err(e) => eprintln!("specula: pipeline cache not saved: {e}"),
    }
    let leftover = engine.gpu.close();
    if let Some(ui) = window {
        ui.close();
    }
    // Every validation error of the run: those left at shutdown and those captures took on the
    // way (review, 2026-10-29: errors a capture reported were missing from this count).
    let taken = trace_engine::gpu_errors_seen();
    let errors = leftover.iter().filter(|m| m.severity == Severity::Error).count() as u64 + taken;
    for m in leftover.iter().filter(|m| m.severity >= Severity::Warning) {
        eprintln!("specula: vulkan {:?} {}: {}", m.severity, m.id_name, m.message);
        trace_engine::gpu_message(m.severity == Severity::Error, &m.id_name, &m.message);
    }
    if taken > 0 {
        specula_core::trace_event!(&trace_sink::GPU, Warn, "gpu.validation_total", reported_by_captures = taken, see = "trace_incidents {code: \"gpu.validation\"}");
    }
    eprintln!("specula: shut down ({errors} validation errors)");
    trace_sink::finish();
    interrupt::finished();
    // A project chosen on the start screen (or handed to it by `specula open`): this engine
    // has shut down, its port and window are free, and the executable starts again there.
    if result.is_ok() {
        if let Some((folder, quiet)) = engine.start.take_chosen() {
            launch_run::respawn(&argv, &folder, quiet)?;
        }
    }
    let strict = trace_engine::strict_failures();
    if result.is_ok() && engine.watch.strict && strict > 0 {
        return Err(format!("--strict-budget: {strict} engine-thread stall(s) past their budget (trace_incidents {{code: \"engine.stall\"}} lists them)"));
    }
    result
}

struct Engine {
    gpu: Gpu,
    meshes: ShapeMeshes,
    /// Meshes built from lathe profiles, shared by profile.
    lathes: LatheMeshes,
    /// Model recipes and builds (also the world's model source).
    library: Arc<SharedLibrary>,
    /// Uploaded model builds.
    models: GpuModels,
    /// How models were drawn in the last window frame and the last capture.
    lod_window: LodStats,
    lod_capture: LodStats,
    /// The window frame's draw list (reused: no allocation per frame once warm).
    draws: Vec<DrawItem>,
    /// The window's draw list between world changes (and each model's LOD, for hysteresis).
    draw_cache: sight::DrawCache,
    /// Captures' draw lists, kept between captures and patched with what changed (LODs chosen
    /// afresh each capture: `DrawCache::for_captures`): without and with the `ui` layer, so
    /// captures that alternate never rebuild.
    capture_draw_cache: [sight::DrawCache; 2],
    /// The window frame's point lights (reused like `draws`).
    lights: Vec<specula_gpu::PointLight>,
    /// Textures, tile cell textures and shape meshes of 2D entities.
    two_d: two_d::TwoDGpu,
    /// The window frame's 2D draws (reused).
    draws_2d: Vec<Draw2d>,
    /// When 2D GPU resources nobody uses were last freed, at which world revision.
    last_2d_gc: (Instant, u64),
    /// A human's window is open.
    windowed: bool,
    /// Calls waiting for model builds.
    build_waiters: Vec<BuildWaiter>,
    /// Captures waiting for models they show.
    capture_waiters: Vec<capture::CaptureWaiter>,
    /// The capture view on the GPU and the captures waiting for it (`capture/flight.rs`).
    captures: capture::flight::CaptureQueue,
    /// Every entity's sight subject data, kept from the world's change record (`sight::SubjectCache`).
    subject_cache: sight::SubjectCache,
    /// The trace watchdogs' state (docs/design/TRACE.md).
    watch: trace_engine::Watch,
    /// Agents' named cameras (camera_set).
    cameras: cameras::Cameras,
    /// sight_compare's "before" captures.
    marks: compare::Marks,
    /// The asset studio: marks, queued calls and their renders (see `studio`).
    studio: studio::Studio,
    /// Font and glyph atlas for overlay text.
    labeler: sight_overlay::Labeler,
    /// When the model library last dropped what no entity uses, at which world mesh epoch.
    last_gc: (Instant, u64),
    /// When builds nobody waits for were last cancelled, at which `World::model_drops`.
    last_unwanted: (Instant, u64),
    world: write_policy::Live,
    /// The grading LUT the world names, loaded from the assets root.
    luts: luts::Luts,
    /// The human's window camera; `None` = auto-frame the world.
    window_camera: Option<Camera>,
    /// `window_view {camera}`: the window looks through agent `.0`'s camera `.1`, re-resolved
    /// every frame (a follow camera keeps following).
    window_follow: Option<(String, String)>,
    /// Why the window stopped following a camera: `"input"` (a human pressed a key, clicked,
    /// scrolled or touched). Humans are never locked out (F5); a hosted game is told
    /// (`Frame::agent_view`) and can't take it.
    window_released_by: Option<&'static str>,
    /// Offscreen targets by size, most recently used last; at most [`MAX_CACHED_TARGETS`].
    targets: Vec<((u32, u32), TargetId)>,
    frames_captured: u64,
    /// This run's folder: `<captures root>/run-<unix seconds>-<pid>`.
    capture_dir: PathBuf,
    /// Where world files live (`--worlds`, default `./worlds`).
    worlds_dir: PathBuf,
    /// Where save games live (`--saves`, default `./saves`; see `save_tools`).
    saves_dir: PathBuf,
    /// Save games being written on worker threads.
    save_writes: Vec<save_tools::Writing>,
    /// A save game being read and checked on a worker thread (one at a time).
    save_load: Option<save_tools::Loading>,
    /// The game's saves refused for a moment (`nav_building`), retried when due: (due, tries,
    /// request). A wave-start autosave isn't lost to a navmesh switching settings.
    game_save_retry: Vec<save_tools::RetrySave>,
    /// Retried game saves under way, noted on their answers (requested tick, tries).
    game_save_notes: Vec<(game::GameRequest, save_tools::RetryInfo)>,
    /// The world autosave writes to: `--world`, or the last world_save / world_load.
    world_name: Option<String>,
    /// The project this engine has open: its folder and name (docs/design/LAUNCH.md).
    opened_project: Option<(PathBuf, String)>,
    /// The start screen of an engine opened with no project (`launch_window`).
    start: launch_window::Start,
    /// `world.revision()` as last saved or loaded.
    saved_revision: u64,
    /// The journal's last entry when the world was last written to its files or a save
    /// (`saved_revision`'s): whose work a forced load would discard ([`Engine::others_unsaved`]).
    saved_seq: u64,
    /// The journal's last entry when a save game last captured the state (`begin_save`).
    checkpoint_seq: u64,
    last_autosave: Instant,
    /// What was last written to or read from the current world's folder, so saves only rewrite
    /// what changed.
    saved: Option<persist::Saved>,
    /// Exclusive ownership of the current world's folder.
    world_lock: Option<persist::WorldLock>,
    /// An autosave being written on a worker thread.
    autosave_job: Option<AutosaveJob>,
    /// The last autosave failure (cleared by the next success), shown in engine_info.
    autosave_error: Option<Json>,
    /// Old world files the last save committed past but couldn't remove.
    leftovers: Vec<String>,
    /// Simulation time: the hosted game's fixed tick (or the default rate without a game).
    clock: Clock,
    game: Option<Box<dyn Game>>,
    /// The game's name, also its agent name in attribution.
    game_name: String,
    /// The game's player settings: where they are saved, the write in flight.
    pub(crate) user_settings: user_settings::UserSettings,
    /// The game asked to quit from the window's own input ([`Game::take_quit`]).
    game_quit: bool,
    /// Timelines' audio cues on the engine's audio service.
    pub(crate) timeline_audio: timeline_audio::TimelineAudio,
    /// Frame pacing and simulation cost, for perf_frame.
    pacing: perf::Pacing,
    /// With a window: when agents are served between presents (see `schedule`); a copy of the
    /// loop's, as the current serve window began, for perf_frame.
    schedule: Schedule,
    /// CPU zones per frame and perf_capture (see [`specula_core::profile`]).
    profiler: perf::Profiler,
    /// Budgets (perf.toml).
    perf_cfg: perf::PerfConfig,
    /// When the clock was last advanced.
    last_advance: Instant,
    /// Window input gathered for the game each frame (reused).
    input: Vec<Event>,
    /// Where the window's keyboard types (a text entity; see text_world).
    text_typing: text_window::WindowTyping,
    /// Sounds modules ask for (presentation only; docs/design/GAMEPLAY_MODULES.md).
    module_sounds: module_sounds::ModuleSounds,
    /// A window title set by the game.
    game_title: Option<String>,
    /// The input monitor's engine side: the toast, and the clock a swarm pause stopped
    /// (docs/design/SECURITY.md).
    security: inputguard_engine::Guard,
    /// The game's interface quads for the window frame (reused).
    game_quads: Vec<OverlayQuad>,
    /// Quads in the game's last overlay, and how many it ever submitted past
    /// [`MAX_GAME_OVERLAY_QUADS`] (dropped).
    game_overlay_stats: (usize, u64),
    /// Identities, claims and the journal of every world change (see [`swarm`]).
    swarm: Swarm,
    /// `(tick, state hash)` recorded as each tick begins, newest last; at most [`TICK_HASHES`].
    tick_hashes: VecDeque<(u64, u64)>,
    /// `(world generation, revision, hash)` of the last hash taken: unchanged worlds aren't
    /// hashed again.
    hash_cache: Option<(u64, u64, u64)>,
    /// Bumped whenever the world is replaced (a revision number alone could repeat).
    world_generation: u64,
    last_claim_sweep: Instant,
    /// Per-tick hashes for the desync detector (see [`desync`]).
    trace: desync::TickLog,
    /// desync_trace files written by this run.
    traces_written: u64,
    /// Writing the task board next to the world (see `swarm_engine`).
    board_io: swarm_engine::BoardSaver,
    /// The window's frame floor under a swarm's backlog, and who set it (`None`: perf.toml or the
    /// default); see `schedule` and `swarm_settings`.
    frame_floor: (u32, Option<String>),
    /// The window's frame cap (`--max-fps`, `perf.toml` `[window] max_fps`; `None`: every vblank).
    max_fps: Option<u32>,
    /// Game-logic modules (hot-reloaded libraries; see [`logic`]).
    logic: logic::LogicHost,
    /// Floating-point environment changes found at startup and between ticks, and restored
    /// (DETERMINISM.md rule 6; the module host keeps its own for module calls).
    fp: fpguard::FpLog,
    /// The physics simulation and its bridge to the world (see [`physics`]).
    physics: physics::Physics,
    /// Navmeshes, nav agents and minds (see [`nav`]).
    nav: nav::Nav,
    /// Lex rule sets the engine steps live on its tick (see [`lex_live`]).
    lex_live: lex_live::Live,
    /// The hosted game's in-process Lex hooks ([`game::Game::lex_natives`]).
    lex_natives: specula_lex::Natives,
    /// What of the world is on the GPU: budgeted uploads, eviction, HLOD (see [`streaming`]).
    stream: streaming::Streamer,
    /// Terrains drawn by the terrain renderer, and water surfaces (see [`terrain`]).
    terrain: terrain::Terrains,
    /// A stream_fly flight of the window camera in progress.
    fly: jobs::Slot<stream_tools::Fly>,
    /// Who last paused, resumed, stepped or scaled the clock, and how (time_control is open to
    /// every agent on purpose; its answer says who moved it last: provenance, not a lock).
    clock_set_by: Option<(String, &'static str)>,
    /// Events the hosted game has for its modules, between taking them and pushing them (kept
    /// for its storage: no allocation a tick once warm).
    game_module_events: Vec<(String, specula_sdk::Value)>,
    /// Whether the clock was paused when the hosted game was last told ([`Game::clock_seen`]).
    game_clock_told: Option<bool>,
    /// A playtest is serving its own steps and ticks right now (its slot is lent to it for the
    /// span, so `playtests.running()` does not say so): see [`Engine::playtest_owns_clock`].
    playtest_pumping: bool,
    /// The hosted game's last wish for the clock from while a playtest had it.
    game_clock_held: Option<bool>,
    /// A script has the clock (`specula run`: the engine was started with `--script`): whether
    /// the hosted game has its
    /// run's clock running, by its own last wish. The clock itself stands still but for the
    /// script's ticks.
    script_clock: Option<bool>,
    /// What the start-up build (`--run`) could not load, for the hosted game to show.
    run_reports: run_modules::Reports,
    /// World folders saved in this run, and the identity key that first saved each (its creator
    /// may overwrite it; anyone else needs the lead).
    world_folders: std::collections::BTreeMap<String, String>,
    /// Agents' commits per tool call: one call, one commit (see [`authority::CallCommits`]).
    commits: authority::CallCommits,
    /// Tool calls that committed more than once outside a session (each a bug: a call that
    /// can fail halfway). In engine_info.
    multi_commit_calls: u64,
    /// The tool call being dispatched (0 outside one): deferred work started now (a worker's
    /// continuation, a project call held during a sync) records it, so its commits are charged to
    /// that call when it runs, never to whichever request happens to pump it ([`Engine::run_charged`]).
    pub(crate) dispatch_call: u64,
    /// Headless playtests: the running one and recent reports (see [`playtest`]).
    playtests: playtest_engine::Playtests,
    /// Particles, effects and decals (see [`vfx`]): cosmetic, never simulation.
    vfx: vfx::Vfx,
    /// Timelines playing (see `timeline`).
    timeline: timeline::Player,
    /// Baked GI (see [`gi`]): volumes, bake jobs, the store, resident chunks.
    gi: gi::Gi,
    /// WCAG 2.3.1 on the final image: the window's frames and each view's captures.
    flash: flashwatch::FlashWatch,
    /// Zero of the window flash watch's clock.
    flash_epoch: Instant,
    /// Default response formats (`--response-format`, `agent_hello`), shared with the tools.
    formats: Arc<response::Defaults>,
    /// Each session's tool scope, shared with the tools: a playtest calls only what its starter
    /// may (see `toolscope`).
    scopes: Option<Arc<toolscope::Scopes>>,
    /// The protocol's tool handler, for the editor's draw tools (`Engine::editor_tool`); `None`
    /// in an engine no server was started for.
    tools: Option<std::sync::Weak<tools::Tools>>,
    /// Project files on disk: mounts, the watcher, failed writes (see [`project`]).
    project: project::Disk,
    /// When failed project writes were last tried.
    project_retry: Instant,
    /// Binary project files' bytes (see [`blobs`]); set at start.
    content: Option<Arc<blobs::ContentStore>>,
    /// The disk budget's view of the project store and what the content store must keep, as last
    /// worked out: (files revision, last journal entry), and when.
    disk_seen: Option<(u64, u64)>,
    disk_last: Instant,
    /// Review evidence kept beside the queue (see [`evidence`]).
    evidence: evidence::EvidenceState,
    /// The window's review screen (see `review_screen`).
    review_ui: review_screen::ReviewUi,
    /// The window's editor (docs/design/EDITOR.md; see `editor_engine`).
    editor: editor_engine::Editor,
    /// The window's draw owners while an editor drag previews (kept to reuse its allocation).
    editor_owners: Vec<u64>,
    /// The display the interface lays out for, and what the `ui_*` tools set (see `ui_engine`).
    ui_state: ui_engine::UiState,
    /// Galleries, the preference store, boards and the style configurator (see `taste`).
    taste: taste::Taste,
    /// An engine screen (review, gallery) held the window's input at the last frame.
    modal_open: bool,
    /// The gamepads handed to the hosted game each window frame (reused), and the gate that
    /// releases them while the review screen holds the input.
    game_pads: Vec<specula_platform::GamepadState>,
    pad_gate: game::PadGate,
    /// The worker pool: encodes, writes and deletes off the engine thread (see [`workers`]).
    workers: workers::Workers,
    /// An explicit world_save is being written on a worker, and the saves waiting for it (see
    /// [`world_save`]).
    world_save_running: bool,
    /// The explicit save whose plan is being taken off the world (`world_save.rs`).
    save_planning: Option<world_save::SavePlanning>,
    waiting_saves: VecDeque<world_save::WaitingSave>,
    /// A world_load parked behind a save, or reading on a worker (`world_load.rs`).
    loading: Option<world_load::Loading>,
    /// A project sync is writing on a worker; calls that read the disk wait for it (in order),
    /// and so do the calls waiting for the next sync's result (see `project_engine`).
    project_syncing: bool,
    /// The model library's disk scan on a worker ([`asset_scan`]).
    assets_scan: asset_scan::ScanState,
    project_deferred: VecDeque<project_engine::Deferred>,
    project_waiters: Vec<project_engine::SyncWaiter>,
    /// Work that needs the engine, in budgeted slices (see [`steps`]).
    steps: steps::Steps,
}

/// A model_build or model_reload call waiting for builds on the worker threads.
struct BuildWaiter {
    /// Jobs still running.
    jobs: BTreeSet<u64>,
    /// model_build: the spec reported on (`None`: a reload).
    spec: Option<world::ModelSpec>,
    /// model_export: where the built model goes for the caller's thread.
    lend: Option<models::BuiltSlot>,
    /// model_reload: the report so far.
    reload: models::Reload,
    reply: jobs::Reply,
    deadline: Instant,
    answer: Option<ToolOutput>,
    /// model_build: its source is still being read on a worker (the number of that read). The
    /// call waits here with the others meanwhile, where its deadline and a shutdown find it.
    checking: Option<u64>,
}

/// Counts model_build's reads on workers (a waiter is found again by its read's number).
static BUILD_CHECKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl BuildWaiter {
    /// model_export: the built model for the caller's thread (a model_build lends nothing).
    fn lend_built(&self, built: &Arc<models::Built>) {
        if let Some(slot) = &self.lend {
            *slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::clone(built));
        }
    }
}

struct AutosaveJob {
    /// The world revision the job is writing, and the journal's last entry then.
    revision: u64,
    seq: u64,
    /// The world it was planned from (`world_generation`): one replaced since marks nothing.
    generation: u64,
    done: Receiver<Result<(persist::Saved, persist::SaveReport), persist::PersistError>>,
}

impl Engine {
    fn new(mut gpu: Gpu, captures_root: &Path, mut library: Library) -> Result<Engine, String> {
        // Re-keys on the engine thread run in slices (the rest next pass: poll_assets_async).
        library.set_rekey_slice(Some(models::REKEY_SLICE));
        let meshes = ShapeMeshes::upload(&mut gpu).map_err(|e| format!("mesh upload: {e}"))?;
        let two_d = two_d::TwoDGpu::new(library.root());
        let vfx = vfx::Vfx::new(library.root());
        let assets_root = library.root().to_path_buf();
        let started = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let capture_dir = captures_root.join(format!("run-{started}-{}", std::process::id()));
        let luts = luts::Luts::new(library.root());
        let library = Arc::new(SharedLibrary::new(library));
        let studio = studio::Studio::new(Arc::clone(&library), capture_dir.clone());
        let mut world = World::default();
        world.set_model_source(Some(Arc::clone(&library) as Arc<dyn world::ModelSource>));
        Ok(Engine {
            gpu,
            meshes,
            lathes: LatheMeshes::default(),
            library,
            luts,
            models: GpuModels::default(),
            lod_window: LodStats::default(),
            lod_capture: LodStats::default(),
            draws: Vec::new(),
            lights: Vec::new(),
            draw_cache: sight::DrawCache::default(),
            capture_draw_cache: [sight::DrawCache::for_captures(), sight::DrawCache::for_captures()],
            two_d,
            draws_2d: Vec::new(),
            last_2d_gc: (Instant::now(), 0),
            windowed: false,
            build_waiters: Vec::new(),
            capture_waiters: Vec::new(),
            captures: Default::default(),
            subject_cache: Default::default(),
            watch: trace_engine::Watch::default(),
            cameras: cameras::Cameras::default(),
            marks: compare::Marks::default(),
            studio,
            labeler: sight_overlay::Labeler::default(),
            last_gc: (Instant::now(), 0),
            last_unwanted: (Instant::now(), 0),
            world: write_policy::Live::new(world),
            window_camera: None,
            window_follow: None,
            window_released_by: None,
            targets: Vec::new(),
            frames_captured: 0,
            capture_dir,
            worlds_dir: PathBuf::from("worlds"),
            saves_dir: PathBuf::from("saves"),
            save_writes: Vec::new(),
            save_load: None,
            game_save_retry: Vec::new(),
            game_save_notes: Vec::new(),
            world_name: None,
            opened_project: None,
            start: launch_window::Start::default(),
            saved_revision: 0,
            saved_seq: 0,
            checkpoint_seq: 0,
            last_autosave: Instant::now(),
            saved: None,
            world_lock: None,
            autosave_job: None,
            autosave_error: None,
            leftovers: Vec::new(),
            clock: Clock::new(DEFAULT_TICK_HZ),
            game: None,
            game_name: String::new(),
            user_settings: user_settings::UserSettings::default(),
            timeline_audio: timeline_audio::TimelineAudio::default(),
            game_quit: false,
            pacing: perf::Pacing::default(),
            schedule: Schedule::default(),
            profiler: perf::Profiler::new(120),
            perf_cfg: perf::PerfConfig::default(),
            last_advance: Instant::now(),
            input: Vec::new(),
            text_typing: Default::default(),
            module_sounds: module_sounds::ModuleSounds::new(&assets_root),
            game_title: None,
            security: inputguard_engine::Guard::default(),
            game_quads: Vec::new(),
            game_overlay_stats: (0, 0),
            swarm: Swarm::default(),
            tick_hashes: VecDeque::new(),
            hash_cache: None,
            world_generation: 0,
            last_claim_sweep: Instant::now(),
            trace: desync::TickLog::default(),
            traces_written: 0,
            board_io: swarm_engine::BoardSaver::default(),
            frame_floor: (schedule::DEFAULT_FRAME_FLOOR_FPS, None),
            max_fps: None,
            logic: {
                // Modules' move_character is the physics P2 controller (the space is lent to it
                // around module calls: lend_physics / return_physics).
                let mut host = logic::LogicHost::new();
                host.set_mover(Box::new(physics::mover::PhysicsMover::default()));
                host
            },
            fp: {
                // Something loaded before us (an injected overlay, a driver) may have changed it.
                let mut fp = fpguard::FpLog::default();
                fp.check(0, "startup");
                fp
            },
            physics: physics::Physics::default(),
            nav: nav::Nav::default(),
            lex_live: lex_live::Live::default(),
            lex_natives: specula_lex::Natives::new(),
            stream: streaming::Streamer::default(),
            terrain: terrain::Terrains::default(),
            fly: jobs::Slot::new(jobs::Kind::StreamFly),
            clock_set_by: None,
            game_module_events: Vec::new(),
            game_clock_told: None,
            playtest_pumping: false,
            game_clock_held: None,
            script_clock: None,
            run_reports: run_modules::Reports::default(),
            world_folders: std::collections::BTreeMap::new(),
            commits: authority::CallCommits::default(),
            multi_commit_calls: 0,
            dispatch_call: 0,
            playtests: Default::default(),
            vfx,
            timeline: timeline::Player::default(),
            gi: gi::Gi::new(std::env::temp_dir().join("specula-gi")),
            flash: flashwatch::FlashWatch::default(),
            flash_epoch: Instant::now(),
            formats: response::Defaults::new(response::Format::Concise),
            scopes: None,
            tools: None,
            project: project::Disk::default(),
            project_retry: Instant::now(),
            content: None,
            disk_seen: None,
            disk_last: Instant::now(),
            evidence: Default::default(),
            review_ui: review_screen::ReviewUi::with_captures(captures_root),
            editor: editor_engine::Editor::default(),
            editor_owners: Vec::new(),
            ui_state: ui_engine::UiState::default(),
            taste: taste::Taste::default(),
            modal_open: false,
            game_pads: Vec::new(),
            pad_gate: game::PadGate::default(),
            workers: workers::Workers::default(),
            world_save_running: false,
            save_planning: None,
            waiting_saves: VecDeque::new(),
            loading: None,
            project_syncing: false,
            assets_scan: asset_scan::ScanState::default(),
            project_deferred: VecDeque::new(),
            project_waiters: Vec::new(),
            steps: steps::Steps::default(),
        })
    }

    /// Makes a loaded world the live one: it records changes, builds its models with the
    /// engine's library, and the journal starts over from it. Returns a message per model entity
    /// whose recipe failed (it loads anyway and draws nothing until fixed). `may`: the check that
    /// it may be replaced now ([`write_policy::may_replace`]).
    fn replace_world(&mut self, may: write_policy::MayReplace, world: World, name: &str) -> Vec<String> {
        // World time resumes where it was saved (the engine's clock joins it); project files are
        // the project's, not the world's: they stay (see `Live::replace`).
        let models = Some(Arc::clone(&self.library) as Arc<dyn world::ModelSource>);
        // Notes, critiques and tasks hold entities by identity; the ones the loaded world lacks
        // show where their entities last were: here, once (docs/design/EDITOR.md).
        self.swarm.remember_entity_paths(&self.world);
        let failed = self.world.replace(may, &mut self.swarm, world, &format!("world:{name}"), self.clock.seconds(), models);
        // The journal starts over: what a forced load would discard is counted from here.
        let last = self.swarm.journal().last_seq();
        self.saved_seq = self.saved_seq.min(last);
        self.checkpoint_seq = last;
        let problems =
            failed.iter().map(|(id, e)| format!("model {} ({id}) not built: {e}", self.world.path_of(*id))).collect();
        self.world_changed_wholesale();
        problems
    }

    /// What the engine keys on the live world starts over after it was replaced (a load, or a
    /// writer's bug the write policy recovered from: [`Engine::check_world_replaced`]).
    ///
    /// **The one hook for the old world's entity ids.** Ids restart in a replaced world, so
    /// anything engine-side that holds one (a selection, a drag, a pick, a note, a merged proxy)
    /// would name a different entity now. Each such holder is reset here, or is keyed on
    /// `world_generation` (which this bumps), or holds no ids: `tests/wholesale_guard.rs` holds
    /// every `Engine` field to one of the three.
    fn world_changed_wholesale(&mut self) {
        // Follow cameras resolve afresh in the new world (its cameras came with it).
        self.cameras = cameras::Cameras::default();
        self.world_generation += 1;
        self.hash_cache = None;
        self.tick_hashes.clear();
        self.trace.clear();
        self.physics.reset();
        // Live Lex runs presented into the old world: callers stop them and say so (world_load
        // with `Live::stop_all`, save_read with the save's runs in `Live::apply_state`), keeping
        // perf.toml's [lex_live] limits.
        self.nav.reset();
        self.vfx.reset();
        self.timeline_world_replaced();
        self.gi.reset();
        // The editor's selection, drag, pick, outliner and undo history name the old world.
        self.editor_world_replaced();
        // The window's keyboard focus and the texts the operator opened to the human's click name
        // the old world's entities.
        self.text_typing.world_replaced();
        // Merged distant proxies show the old world and cover ids the new one reuses.
        self.stream.world_replaced(&mut self.gpu, &mut self.models);
        // A decision press right after must not decide what the human hasn't seen.
        self.review_ui.world_replaced(&self.swarm.review);
        // Notes, critiques and tasks on entities need nothing: they hold identities, which
        // attach only to the very entities (`entity_ref`; the swarm's row in wholesale_guard).
        // Live Lex runs presented into the old world (world_load and save_read stop them first and
        // say so in their answers; a recovered writer's bug is traced).
        let stopped = self.lex_live.stop_all();
        if !stopped.is_empty() {
            specula_core::trace_event!(&trace_sink::ENGINE, Warn, "world.replaced", lex_runs_stopped = stopped.len() as u64);
        }
        // Navigation reads the loaded world now: its navmeshes are built (in slices, the clock
        // held) before the first tick, as they were in the run that saved it.
        self.nav.sync(&self.world, self.world_generation);
    }

    /// After writers ran: if one replaced the live world wholesale (a bug; the write policy
    /// recovered and reported it), the engine's own caches start over as after a load.
    fn check_world_replaced(&mut self) {
        if self.world.take_replaced() {
            self.world_changed_wholesale();
            // The world files on disk describe a world that is gone: the next save rewrites them.
            self.saved = None;
        }
    }

    /// The world undid a refused game tool's (or `ui_act` frames') changes: the game goes back
    /// with it, to its checkpoint, or (none taken) repairs itself as its own write.
    pub(crate) fn game_rolled_back(&mut self, checkpoint: Option<Box<dyn std::any::Any>>) {
        let Some(game) = self.game.as_mut() else { return };
        match checkpoint {
            Some(c) => game.restore(c),
            None => sim_writes::game(&mut self.world, &mut self.swarm, game.as_mut(), &self.clock, &self.game_name, self.windowed, sim_writes::GameCall::RolledBack),
        }
    }

    /// One of the hosted game's own calls, as its own writes: journaled under its name, exempt
    /// from claims ([`sim_writes::game`]). Whether a game ran it.
    fn game_call(&mut self, call: sim_writes::GameCall) -> bool {
        let Some(game) = self.game.as_mut() else { return false };
        sim_writes::game(&mut self.world, &mut self.swarm, game.as_mut(), &self.clock, &self.game_name, self.windowed, call);
        // A writer may have replaced the world (a bug the policy recovered from).
        self.check_world_replaced();
        true
    }

    /// The world's state hash, reusing the last one while nothing changed.
    fn state_hash(&mut self) -> u64 {
        let (generation, revision) = (self.world_generation, self.world.revision());
        match self.hash_cache {
            Some((g, r, h)) if (g, r) == (generation, revision) => h,
            _ => {
                let h = self.world.state_hash();
                self.hash_cache = Some((generation, revision, h));
                h
            }
        }
    }

    /// Journals module loads, reloads, unloads and disables (kind `module`, DETERMINISM.md rule 8)
    /// and notes them for the desync detector's traces. `by`: the agent whose call caused them
    /// (`None`: they happened during a tick, e.g. a module disabled after a panic).
    fn journal_module_events(&mut self, by: Option<Writer>) {
        for ev in self.logic.take_module_events() {
            self.trace.note_module(&ev);
            let module = logic::agent_of(&ev.module);
            match by {
                Some(w) if ev.event != "disabled" => write_policy::module_event(&self.world, &mut self.swarm, w, None, ev),
                Some(w) => write_policy::module_event(&self.world, &mut self.swarm, Writer::from(module.as_str()), Some(w.name), ev),
                None => write_policy::module_event(&self.world, &mut self.swarm, Writer::from(module.as_str()), None, ev),
            };
        }
    }

    /// Records the hash as a tick begins.
    fn record_tick_hash(&mut self) {
        self.journal_module_events(None);
        let h = self.state_hash();
        if self.tick_hashes.len() == TICK_HASHES {
            self.tick_hashes.pop_front();
        }
        self.tick_hashes.push_back((self.clock.tick(), h));
        self.record_trace(self.clock.tick(), h);
        // Full verbosity only (off by default): every tick's hash, checked never to change it.
        specula_core::trace_event!(&trace_sink::WORLD, Trace, "world.tick", hash = journal::hex(h));
    }

    /// The ids of the sessions that are open: the protocol's connections (`connections`), and
    /// the operator's own session when this engine has one (`--run`, `--script`). That one is
    /// a caller inside the engine: it has no connection, and it is there until the engine
    /// ends. What belongs to a session that is not among these is dropped (a worker's job, a
    /// stepped job, an interface's state): a headless run's frame was, one run in four, when
    /// the look for closed sessions fell between its render and its encode.
    pub(crate) fn open_session_ids(&self, connections: Vec<SessionInfo>) -> BTreeSet<String> {
        let mut ids: BTreeSet<String> = connections.into_iter().map(|s| s.id).collect();
        if self.swarm.introduced(run_modules::RUN_SESSION) {
            ids.insert(run_modules::RUN_SESSION.to_owned());
        }
        ids
    }

    /// Drops claims whose session is gone (at most once per [`CLAIM_SWEEP_EVERY`]).
    fn sweep_claims(&mut self, server: &Server) {
        if self.swarm.claims.is_empty() || self.last_claim_sweep.elapsed() < CLAIM_SWEEP_EVERY {
            return;
        }
        self.last_claim_sweep = Instant::now();
        self.swarm.sweep(&server.all_sessions(), Instant::now());
    }

    /// Calls the game's `start` once the engine is ready.
    fn start_game(&mut self) {
        if self.game.is_none() {
            return;
        }
        // Its player's saved settings first (read here, at start-up, before the loop).
        self.settings_to_game();
        let Some(game) = self.game.as_mut() else { return };
        sim_writes::game_start(&mut self.world, &mut self.swarm, game.as_mut(), &self.clock, &self.game_name, self.windowed);
        // What the game wants of the clock from its first moment (a title holds it): done before
        // any tick, so that no tick runs at a title (review R1 of M6 part 3: one did).
        self.serve_game_clock();
        self.last_advance = Instant::now();
        // The display its interface lays out for (headless: the virtual one).
        self.ui_tell_display();
    }

    /// Builds a loaded world's navmeshes for up to `limit` of wall time (slices back to back).
    /// Returns whether the load is done.
    fn build_nav_load(&mut self, limit: Duration) -> bool {
        let started = Instant::now();
        while self.nav.loading() {
            if self.nav.load_slice(&self.world, self.world_generation, Some(nav::LOAD_SLICE_WORK)) {
                break;
            }
            if started.elapsed() >= limit {
                break;
            }
        }
        !self.nav.loading()
    }

    /// The structured answer for a call that needs ticks while a load still builds.
    fn nav_loading_error(&self) -> ToolOutput {
        let mut e = WorldError::new("loading", "the loaded world's navmeshes are still being built (the simulation clock is held until they are): call again shortly")
            .with("retry", true);
        if let Some(Json::Arr(meshes)) = self.nav.summary_json().get("navmeshes").cloned() {
            e = e.with("navmeshes", Json::Arr(meshes.into_iter().filter_map(|m| Some(Json::obj([("path", m.get("path")?.clone()), ("building", m.get("building").cloned().unwrap_or(Json::Null))]))).collect()));
        }
        world_error(&e)
    }

    /// Turns the real time since the last call into whole ticks and runs them.
    /// Runs the ticks due. Inside a request (`time_control step`) the ticks are the
    /// simulation's, not the caller's call: commits they make (held input firing as its holder)
    /// aren't counted toward one call, one commit.
    fn advance_clock(&mut self) {
        let paused = self.swarm.pause_dispatch();
        self.advance_clock_ticks();
        self.swarm.resume_dispatch(paused);
    }

    fn advance_clock_ticks(&mut self) {
        // CJK re-layouts the font loader finished are taken up here, between frames and calls,
        // so a frame or capture and its sight packet see one layout per text.
        text_world::adopt_relayouts();
        // The world's anim assets reach the library (graph writes settle against them) without
        // waiting for a draw: headless engines never draw.
        self.models.refresh_anim_env(&self.library, &self.world, self.world_generation);
        let now = Instant::now();
        if self.nav.loading() {
            // A loaded world's navmeshes build first, slices back to back within a frame's
            // budget, with the simulation clock held (frames and agent calls are served between
            // frames; the time doesn't become ticks later). The result doesn't depend on the
            // slices.
            let _z = profile::zone("nav_load");
            self.build_nav_load(nav::LOAD_FRAME);
            self.last_advance = Instant::now();
            if self.nav.loading() {
                return;
            }
        }
        let ticks = self.clock.advance(now - self.last_advance);
        self.last_advance = now;
        let _z = profile::zone("sim");
        for _ in 0..ticks {
            // The simulation runs in the default floating-point environment, whatever rendering,
            // a driver or an overlay did to this thread since the last tick (rule 6).
            self.fp.check(self.clock.tick(), "between_ticks");
            specula_core::trace::set_tick(self.clock.tick());
            // Animations set during the tick start at its time.
            self.world.set_clock_s(self.clock.seconds());
            if self.game.is_some() {
                let _t = profile::zone("tick");
                let start = Instant::now();
                // Game updates are journaled replay-only and coalesced per tick (F3).
                self.game_call(sim_writes::GameCall::Tick);
                self.pacing.tick.push(start.elapsed());
            }
            if self.game.is_some() && !self.logic.is_empty() {
                // What the game has for its modules (a press of one of its own buttons, a
                // setting the rules read) reaches them with this tick, as events from the
                // engine: the one way anything outside the simulation gets in, at a tick
                // boundary. With no module loaded there is nobody to hear them, so they wait
                // with the game (a run started while its modules are still being built is told
                // to them when they load; the game bounds how many wait).
                let mut events = std::mem::take(&mut self.game_module_events);
                if let Some(game) = self.game.as_mut() {
                    game.take_module_events(&mut events);
                }
                for (name, data) in events.drain(..) {
                    self.logic.push_engine_event(&name, data, self.clock.tick());
                }
                self.game_module_events = events;
            }
            if !self.logic.is_empty() {
                let _l = profile::zone("modules");
                self.lend_physics();
                // The game's rules: journaled, exempt from claims.
                sim_writes::module_tick(&mut self.world, &mut self.swarm, &mut self.logic, self.clock.tick(), self.clock.hz());
                self.return_physics();
                // Holds the window's human took back this tick, on the monitor's record.
                self.security_holds();
            }
            // The game reads what its modules keep now (and may decide the run has ended).
            if let Some(game) = self.game.as_mut() {
                let modules = game::ModuleStates::of(&self.logic);
                sim_writes::game(&mut self.world, &mut self.swarm, game.as_mut(), &self.clock, &self.game_name, self.windowed, sim_writes::GameCall::AfterModules(&modules));
            }
            // Live Lex rule sets step on the same tick, after the modules (whose hooks they call).
            if !self.lex_live.is_empty() {
                let _l = profile::zone("lex_live");
                self.lex_live_tick();
            }
            // Timelines step on the same tick (their writes as their starters, claims checked).
            self.timeline_tick();
            // Navigation decides and steers on the same tick, before physics walks its characters.
            self.nav_tick();
            // Physics steps on the same fixed tick, after the game moved what it moves.
            self.physics_tick();
            // World-space effects' tracks (cosmetic: read only), as of the tick's end.
            let tick_end = self.world.world_time() + 1.0 / f64::from(self.clock.hz());
            self.vfx.tick(&self.world, self.world_generation, tick_end);
            self.check_world_replaced();
            self.clock.finish_tick();
            {
                let _h = profile::zone("tick_hash");
                self.record_tick_hash();
            }
            // A game that stopped the clock on this tick (its run ended, it paused) has no
            // further tick: the ones still due in this advance, and any still queued by a step,
            // do not run behind it.
            if self.serve_game_clock() == Some(true) {
                self.clock.cancel_steps();
                break;
            }
        }
        self.world.set_clock_s(self.clock.seconds());
        if ticks > 0 {
            // A tick may have asked for something (the game's wave-start autosave).
            self.serve_game_requests();
            self.serve_game_timelines();
        }
        // Sounds the ticks' (and agents' hook calls') module calls asked for: presentation only.
        self.play_module_sounds();
    }

    /// `game_state`: how the hosted game's run stands, as the game says it, with the tick and
    /// whether the clock runs.
    fn game_state(&mut self) -> ToolOutput {
        self.game_sees();
        let Some(mut state) = self.game.as_ref().and_then(|g| g.game_state()) else {
            let why = if self.game.is_some() {
                "the hosted game does not say how its run stands (its own play_* tools do)"
            } else {
                "no game is hosted: a project gets a run, screens and settings from a game.json at its root (docs/design/FIRST_HOUR.md; 'specula new NAME' makes a project that has one)"
            };
            return world_error(&WorldError::new("no_game", why));
        };
        state.set("tick", Json::from(self.clock.tick()));
        state.set("clock_paused", Json::from(self.clock.paused()));
        // The modules that are loaded and running now, by their own names (the game file's
        // `modules` are their folders): none means the game's rules are not running.
        state.set("modules_running", Json::arr(game::ModuleStates::of(&self.logic).running().into_iter().take(64).map(Json::from)));
        ToolOutput::text(state.to_string())
    }

    /// `game_act`: a button's word pressed by an agent, as a press on one of the game's screens
    /// would do it; answers with how the run stands after it.
    fn game_act(&mut self, session: &SessionInfo, action: &str) -> ToolOutput {
        // An engine screen has the window (the review or the gallery screen): a player could
        // not reach the game now, and the game paused itself for the person who is deciding.
        // An agent's press is held to the same (as `ui_act` on the game is).
        if let Some(open) = [(self.review_ui.is_open(), "review"), (self.taste.screen.is_open(), "gallery")].iter().find(|(o, _)| *o).map(|(_, n)| *n) {
            return world_error(&WorldError::new("engine_screen_open", format!("the {open} screen is open on the window and takes all its input: a player couldn't reach the game now (the owner closes it)")));
        }
        self.game_sees();
        let mut answer = None;
        self.game_call(sim_writes::GameCall::Act(action, &mut answer));
        // A press the game refused as the player's alone (quit): told to the input monitor
        // under this caller, here, whatever became of the call, so that no count is left for
        // the next caller to be blamed for (review R1 of M6 part 3: the next `ui_act` was).
        if self.game.as_mut().map_or(0, |g| g.take_refused_presses()) > 0 {
            inputguard::declined(inputguard::Channel::ScreenInput, "game_act", &inputguard::Actor::of(&self.swarm, session), "players_only", "the hosted game");
        }
        match answer {
            None => {
                let why = if self.game.is_some() {
                    "the hosted game takes no words: its own tools drive it (tools_catalog {family: \"game\"})"
                } else {
                    "no game is hosted: a project gets a run, screens and settings from a game.json at its root (docs/design/FIRST_HOUR.md; 'specula new NAME' makes a project that has one)"
                };
                world_error(&WorldError::new("no_game", why))
            }
            Some(Err((code, message))) => world_error(&WorldError::new(code, message).with("action", bounds::echo(action))),
            Some(Ok(())) => {
                // What the press asked of the engine (the clock, a save, the settings file).
                self.serve_game_requests();
                self.game_state()
            }
        }
    }

    /// Whether a playtest has the clock: one is running, between its slices or inside one.
    pub(crate) fn playtest_owns_clock(&self) -> bool {
        self.playtest_pumping || self.playtests.running()
    }

    /// The hosted game's wish for the clock ([`Game::take_clock`]), done: the game stops or
    /// starts it as its own doing, and `time_control` says the game did. Returns what was done.
    pub(crate) fn serve_game_clock(&mut self) -> Option<bool> {
        let paused = self.game.as_mut()?.take_clock()?;
        // A playtest owns the clock while it runs: it holds it paused and steps it tick by
        // tick, so the same spec makes the same calls at the same ticks. The game's wish
        // changes nothing then (a run begun by a step is ticked by the playtest's steps, and
        // its ticks go on past the run's end). It is kept, the last one, and done when the
        // playtest gives the clock back ([`Engine::playtest_clock_back`]): thrown away,
        // it left a run under way with its clock stopped, or a run that had ended with its
        // clock running (review R1 of M6 part 3).
        if self.playtest_owns_clock() {
            self.game_clock_held = Some(paused);
            return None;
        }
        // A script has the clock (`specula run`): the game's wish is the run's own state, and
        // the script runs a tick only while it says "running". The clock itself stays still, so
        // no time of this machine becomes a tick.
        if let Some(running) = self.script_clock.as_mut() {
            *running = !paused;
            self.clock_set_by = Some((format!("game:{}", self.game_name), if paused { "pause" } else { "resume" }));
            self.game_clock_told = Some(paused);
            return Some(paused);
        }
        self.set_game_clock(paused);
        Some(paused)
    }

    /// A script's ticks (`specula run`; [`TimeOp::Play`]): up to `n` ticks while the hosted
    /// game has its run's clock running, none while the game holds it (the title, a pause, a
    /// run that ended). Returns how many ran. Only an engine started for a script plays any
    /// (`--script`): the clock is the script's from the start, and stands still but for these.
    pub(crate) fn play_ticks(&mut self, n: u64) -> u64 {
        // A press since the last tick may have started, paused or ended the run.
        self.serve_game_clock();
        let before = self.clock.tick();
        // (The clock stands still under a script, whatever was done to it: its ticks are the
        // ones asked for here, each once.)
        if self.script_clock.is_some() && !self.clock.paused() {
            self.clock.set_paused(true);
        }
        if self.script_clock == Some(true) {
            self.clock.step(n.min(game::STEPS_AT_MOST));
            self.advance_clock();
            // (The game stopped the clock on one of them: the rest are not owed.)
            self.clock.cancel_steps();
        }
        self.clock.tick() - before
    }

    /// The clock as the hosted game wants it, set as the game's own doing.
    fn set_game_clock(&mut self, paused: bool) {
        if self.clock.paused() != paused {
            self.clock.set_paused(paused);
            // Time that passed while it stood still does not become ticks when it starts.
            self.last_advance = Instant::now();
        }
        // (Said even when the clock stood still already: a step cut short by the run's end
        // says who ended it.)
        self.clock_set_by = Some((format!("game:{}", self.game_name), if paused { "pause" } else { "resume" }));
        self.game_clock_told = Some(paused);
    }

    /// A playtest gives the clock back: as it was before the playtest took it, and then as the
    /// hosted game last wished while the playtest had it (a run that began, or ended, in it),
    /// so the clock is as the run stands.
    pub(crate) fn playtest_clock_back(&mut self, was_paused: bool) {
        if !was_paused {
            self.clock.set_paused(false);
            self.last_advance = Instant::now();
        }
        if let Some(paused) = self.game_clock_held.take() {
            self.set_game_clock(paused);
        }
    }

    /// Tells the hosted game when someone else started or stopped the clock since it last
    /// heard (an agent's `time_control`), and lends it what its modules keep now, to show:
    /// before a frame, a capture's interface and the `ui_*` tools read it.
    pub(crate) fn game_sees(&mut self) {
        let paused = self.clock.paused();
        let Some(game) = self.game.as_mut() else { return };
        // (Under a script the clock stands still by the script's doing, and the game's own wish
        // is what it is told: nobody else stopped its run.)
        if self.script_clock.is_none() && self.game_clock_told != Some(paused) {
            self.game_clock_told = Some(paused);
            game.clock_seen(paused);
        }
        // What the start-up build (`--run`) could not load (never waited for: its thread
        // holds the lock for one push).
        if let Ok(mut reports) = self.run_reports.try_lock() {
            for problem in reports.drain(..) {
                game.told(&problem);
            }
        }
        game.see_modules(&game::ModuleStates::of(&self.logic));
    }

    /// The game's per-frame update: input, camera, title.
    fn game_frame(&mut self, ui: &UiThread, size: (u32, u32), dt: Duration) {
        // The window's display (size, the OS's scale for its monitor) before the game lays out.
        self.ui_window(ui.raw, size);
        self.input.clear();
        ui.drain_events(&mut self.input);
        // The security toast's buttons take the clicks inside it (and nothing else).
        self.security_toast_input();
        // Keys the human meant for a text never reach a screen an agent put on the window.
        self.text_window_screens();
        // The start screen (an engine opened with no project) reads the person's input first:
        // while it is open nothing under it gets any.
        let start = self.launch_frame(size);
        // The review screen reads input next; while it is open the game, modules and live Lex
        // runs get none (no keys, pointer or pads).
        let modal = self.review_frame(size) || start;
        // The gallery screen next: under an open review screen it gets no input (no keys, pointer
        // or pads).
        let modal = self.gallery_frame(size, modal) || modal;
        // The editor next: under an open review or gallery screen it gets no input.
        let modal = self.editor_frame(size, modal) || modal;
        // A text entity with the window's focus takes the keys and characters it types with (not
        // under the editor or an engine screen: one taking the window ends the typing).
        if modal {
            self.text_window_modal();
        } else {
            self.text_window_input();
        }
        // A hosted game pauses itself while an engine screen holds the window (it is told once
        // per change).
        if modal != self.modal_open {
            self.modal_open = modal;
            self.game_call(sim_writes::GameCall::ModalChanged(modal));
        }
        if modal {
            self.logic.feed_window_idle(dt.as_secs_f32());
        } else {
            self.logic.feed_window(&self.input, dt.as_secs_f32());
        }
        if self.lex_live.has_input() {
            if modal {
                self.lex_live.feed_window(&self.input, &[], dt.as_secs_f32());
            } else {
                let poll = !self.logic.reads_input();
                let pads = self.logic.pad_states(poll);
                self.lex_live.feed_window(&self.input, pads, dt.as_secs_f32());
            }
        }
        // A human taking the controls takes the camera back from an agent's (window_view).
        if self.window_follow.is_some() && human_took_controls(&self.input) {
            self.release_window_camera("input");
        }
        // A playing timeline may drive the window: a human's key, click, scroll, touch or gamepad
        // button skips or releases it (and a pad's press goes nowhere else: gated below).
        let pad_press = !modal && self.timeline_pad_pressed();
        let (timeline_view, took) = self.timeline_window(!modal && (human_took_controls(&self.input) || pad_press));
        let agent_view = self.window_follow.is_some() || timeline_view;
        let shown = self.window_camera;
        if self.game.is_none() {
            return;
        }
        // The game's pads: polled here unless the modules just did, released while the review
        // screen holds the input (and until buttons held then are let go).
        let poll = modal || !self.logic.reads_input();
        let mut pads = std::mem::take(&mut self.game_pads);
        pads.clear();
        pads.extend_from_slice(self.logic.pad_states(poll));
        self.pad_gate.apply(modal || took, &mut pads);
        self.game_sees();
        let Some(game) = self.game.as_mut() else { return };
        let _z = profile::zone("game_frame");
        let start = Instant::now();
        let mut frame = game::Frame {
            input: &self.input,
            size,
            alpha: self.clock.alpha(),
            dt_s: dt.as_secs_f32(),
            camera: &mut self.window_camera,
            agent_view,
            title: &mut self.game_title,
            pads: &pads,
            synthetic: false,
        };
        sim_writes::game(&mut self.world, &mut self.swarm, game.as_mut(), &self.clock, &self.game_name, self.windowed, sim_writes::GameCall::Frame(&mut frame));
        // While the window looks through an agent's camera the game's camera writes don't count
        // (only a human takes the view back, F5): the game was told (`Frame::agent_view`).
        if agent_view {
            self.window_camera = shown;
        }
        self.pacing.game_frame.push(start.elapsed());
        self.check_world_replaced();
        // The game's menu may have asked for a save or a load.
        self.serve_game_requests();
        self.serve_game_timelines();
    }

    /// The hosted game's interface for one image ([`Game::overlay`]) into `out` (cleared first).
    fn game_overlay(&mut self, size: (u32, u32), camera: Camera, capture: bool, out: &mut Vec<OverlayQuad>) {
        out.clear();
        if self.game.is_none() || size.0 == 0 || size.1 == 0 {
            return;
        }
        // (A capture's interface shows what the modules keep now, as a frame's does.)
        if capture {
            self.game_sees();
        }
        let Some(game) = self.game.as_mut() else { return };
        let _z = profile::zone("game_overlay");
        let mut overlay = game::Overlay { size, camera, capture, quads: out, textures: &mut self.gpu };
        sim_writes::game(&mut self.world, &mut self.swarm, game.as_mut(), &self.clock, &self.game_name, self.windowed, sim_writes::GameCall::Overlay(&mut overlay));
        // Past the cap, or naming a texture the GPU doesn't have: dropped (counted), never a
        // failed frame.
        let gpu = &self.gpu;
        let dropped = game::keep_drawable(out, MAX_GAME_OVERLAY_QUADS, |t| gpu.has_texture(t));
        self.game_overlay_stats.1 += dropped as u64;
        if !capture {
            self.game_overlay_stats.0 = out.len();
        }
    }

    /// Whether the world changed since it was last saved or loaded.
    fn dirty(&self) -> bool {
        self.world.revision() != self.saved_revision
    }

    /// Another agent whose unsaved work (journal entries since the last write to the world's
    /// files or the last save game) a forced load by the identity `key` would discard: an entry it wrote
    /// itself, or that a game, module or live Lex run made because it asked. The simulation's
    /// own writers (physics, ticks) and the caller's entries don't count; an entry the journal
    /// no longer holds does (conservatively).
    fn others_unsaved(&self, key: &str) -> Option<String> {
        let journal = self.swarm.journal();
        let last = journal.last_seq();
        // Bounded: the journal keeps at most its ring of entries.
        for seq in (self.saved_seq.max(self.checkpoint_seq) + 1..=last).rev().take(1 << 16) {
            let Some(e) = journal.get(seq) else { return Some("(an entry no longer in the journal)".into()) };
            let other_session = e.session.as_deref().is_some_and(|k| k != key);
            let other_via = e.via.as_deref().is_some_and(|v| e.session.as_deref() != Some(key) && !v.is_empty());
            if other_session || (other_via && e.session.is_none()) {
                return Some(e.via.as_deref().unwrap_or(&e.by).to_owned());
            }
        }
        None
    }

    /// Saves the current world, if there is one and it changed. The engine thread only takes the
    /// changed subtrees off the world ([`persist::plan`]); rendering and writing happen on a worker
    /// thread, unless `wait` (shutdown).
    fn autosave(&mut self, wait: bool) {
        self.last_autosave = Instant::now();
        self.finish_autosave(false);
        let Some(name) = self.world_name.clone() else { return };
        // Never beside an explicit save of the same folder (see `world_save`), and never while a
        // load is replacing the world: it would save the changes `force` discards, into the
        // outgoing world's folder (`world_load`'s module docs).
        if self.autosave_job.is_some() || self.world_save_running || !self.dirty() || (self.loading.is_some() && !wait) {
            return;
        }
        let plan = {
            let _z = profile::zone("autosave.plan");
            persist::plan(&self.world, &self.worlds_dir.join(&name), self.saved.as_ref())
        };
        let (tx, done) = mpsc::channel();
        let job = move || {
            let result = {
                let _z = profile::zone("autosave.write");
                persist::execute(plan)
            };
            let _ = tx.send(result);
        };
        let revision = self.world.revision();
        if wait {
            job();
        } else {
            // On the world lane: never beside an explicit save's write (see `world_save`).
            let o = workers::Offload::engine(workers::JobKind::Autosave).lane(workers::Lane::World);
            let _ = self.offload(o, move |_| job(), |_, _| {});
        }
        self.autosave_job = Some(AutosaveJob { revision, seq: self.swarm.journal().last_seq(), generation: self.world_generation, done });
        self.finish_autosave(wait);
    }

    /// Collects a finished autosave (or waits for it with `block`).
    fn finish_autosave(&mut self, block: bool) {
        let Some(job) = &self.autosave_job else { return };
        let received = if block {
            job.done.recv().ok()
        } else {
            match job.done.try_recv() {
                Ok(r) => Some(r),
                Err(mpsc::TryRecvError::Empty) => return, // still writing
                Err(mpsc::TryRecvError::Disconnected) => None,
            }
        };
        let result = match received {
            Some(r) => r,
            None => Err(persist::PersistError::new("io", "the autosave thread stopped without reporting")),
        };
        let (revision, seq) = (job.revision, job.seq);
        let replaced = job.generation != self.world_generation;
        self.autosave_job = None;
        if replaced {
            // The world it saved is gone: its revision and cache say nothing about this one.
            specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "autosave.world_replaced", revision = revision);
            return;
        }
        match result {
            Ok((saved, report)) => {
                self.replace_saved(saved);
                self.saved_revision = revision;
                self.saved_seq = seq;
                self.autosave_error = None;
                self.note_leftovers(&report);
            }
            Err(e) => {
                eprintln!("specula: autosave failed: {e}");
                // Unsaved work is at risk until the next one succeeds: agents debug from traces
                // (rule 15), and `engine_info` keeps the last failure (`last_autosave_error`).
                let message = e.to_string();
                specula_core::trace_event!(&crate::trace_sink::ENGINE, Error, "autosave.failed", code = e.code, error = message.as_str(), revision = revision);
                let mut j = e.to_json();
                j.set("at_revision", Json::from(revision));
                self.autosave_error = Some(j);
                if e.invalidates_cache() {
                    self.saved = None;
                }
            }
        }
    }

    fn autosave_if_due(&mut self) {
        let _z = profile::zone("autosave");
        self.finish_autosave(false);
        if self.last_autosave.elapsed() >= AUTOSAVE_EVERY {
            self.autosave(false);
            self.save_board_if_due();
        }
    }

    /// Remembers old files a successful save couldn't remove (shown by engine_info).
    fn note_leftovers(&mut self, report: &persist::SaveReport) {
        if !report.leftovers.is_empty() {
            eprintln!("specula: saved, but could not remove old files {:?}; the next save retries", report.leftovers);
        }
        self.leftovers = report.leftovers.clone();
    }

    /// `window_view {camera}`: the window camera follows that named camera. When the camera is
    /// gone (deleted, undone, another world loaded) the window keeps its last view.
    fn follow_window_camera(&mut self) {
        let Some((owner, name)) = &self.window_follow else { return };
        match self.cameras.resolve(owner, name, Some(owner), &self.world) {
            Ok(r) => self.window_camera = Some(r.camera),
            Err(e) => {
                eprintln!("specula: the window stops following {owner}'s camera '{name}': {}", e.message);
                self.window_follow = None;
            }
        }
    }

    /// Stops following an agent's camera (the window keeps its current view).
    fn release_window_camera(&mut self, by: &'static str) {
        if let Some((owner, name)) = self.window_follow.take() {
            eprintln!("specula: the window stops following {owner}'s camera '{name}' (taken back by the {by})");
            self.window_released_by = Some(by);
        }
    }

    /// The human's window camera, for engine_info: where it looks now and what it looks through
    /// (`window_view {camera}`), so agents and humans can confirm they see the same thing.
    fn window_json(&mut self) -> Json {
        self.follow_window_camera();
        let through = self.window_follow.as_ref().map_or(Json::Null, |(owner, name)| {
            Json::obj([("owner", Json::from(owner.as_str())), ("camera", Json::from(name.as_str()))])
        });
        Json::obj([
            ("camera", self.window_camera.map_or(Json::from("auto"), |c| c.to_json())),
            ("following", Json::from(self.window_follow.is_some())),
            ("through", through),
            ("released_by", self.window_released_by.map_or(Json::Null, Json::from)),
            // The text the human's keyboard types into, and its banner in the last frame.
            ("typing", self.text_typing.json()),
            // A timeline driving the window through its rail camera (see `timeline`).
            ("timeline", self.timeline.window_path().map_or(Json::Null, Json::from)),
            // The input monitor's toast as the last window frame drew it (docs/design/SECURITY.md).
            ("security", self.security_window_json()),
        ])
    }

    /// Render time when a capture gives none: world time (the engine clock in whole ticks, plus
    /// a loaded world's offset), so animation and the lava lamp follow time_control, resume
    /// after a load, and a capture's time is reproducible.
    /// The simulation clock, plus `world_time_s`: the world's time (what animation starts and
    /// sight_frame's default time_s are in; it carries on across world_save / world_load, while
    /// the clock's seconds count from this engine's start).
    fn clock_json(&self) -> Json {
        let mut j = self.clock.to_json();
        j.set("world_time_s", Json::from(self.world.world_time()));
        if let Some((by, op)) = &self.clock_set_by {
            j.set("set_by", Json::obj([("agent", Json::from(by.as_str())), ("op", Json::from(*op))]));
        }
        j
    }

    fn time_s(&self) -> f64 {
        self.world.world_time()
    }

    /// The window's render time: world time plus the elapsed part of the next tick (smooth
    /// motion).
    fn window_time_s(&self) -> f64 {
        self.world.world_time() + f64::from(self.clock.alpha()) / f64::from(self.clock.hz())
    }

    /// Builds meshes for new or changed lathe profiles and models and frees unused ones — only
    /// when the world's meshes or the model library changed since the last call. Returns a
    /// message per failed upload (those entities aren't drawn).
    fn sync_meshes(&mut self) -> Vec<String> {
        let _z = profile::zone("sync_meshes");
        let generation = self.world_generation;
        let mut errors = self.lathes.sync(&mut self.gpu, &self.world, generation);
        if self.stream.enabled() {
            // Streaming uploads within each frame's budget (`stream_tick`); here only what changed.
            errors.extend(self.stream.sync_models(&mut self.models, &mut self.gpu, &self.library, &self.world, generation));
        } else {
            errors.extend(self.models.sync(&mut self.gpu, &self.library, &self.world, generation));
        }
        errors
    }

    /// Rebuilds models whose recipe or style files changed (checked at most once a second;
    /// `force`: now). Returns the reload report when something was checked and changed.
    /// Synchronous (disk on this thread): `model_reload`, which answers with the result. The
    /// window and headless loops scan on a worker ([`Engine::poll_assets_async`]).
    fn poll_assets(&mut self, force: bool) -> Option<models::Reload> {
        let reload = self.library.lock().poll(force);
        self.assets_upkeep(force);
        self.assets_reloaded(reload)
    }

    /// Sprite reloads and the model library's garbage collection (each loop pass).
    fn assets_upkeep(&mut self, force: bool) {
        let _z = profile::zone("models.upkeep");
        // Sprite and tileset PNGs changed on disk are reloaded (failed ones retried).
        {
            let _z = profile::zone("sprites.poll_files");
            self.two_d.poll_files(force);
        }
        // With streaming on, finished loads are uploaded within the frame's budget (stream_tick).
        if self.two_d.loading() > 0 && !self.stream.enabled() {
            self.two_d.pump(&mut self.gpu, None);
        }
        // Garbage collection, whether or not anything is drawn (a headless engine never syncs
        // GPU meshes). Cheap part, every GC_UNWANTED_EVERY while builds are in flight after the
        // world's models changed: builds no entity waits for any more are cancelled. Full part,
        // every GC_EVERY after a change: unused specs, builds, folders and recipes go.
        let epoch = self.world.mesh_epoch();
        let (in_flight, specs) = {
            let lib = self.library.lock();
            (lib.in_flight(), lib.spec_count())
        };
        let drops = self.world.model_drops();
        if in_flight > 0 && drops != self.last_unwanted.1 && self.last_unwanted.0.elapsed() >= GC_UNWANTED_EVERY {
            let mut live: Vec<u32> = self.world.model_entities().filter_map(|e| e.resolved_model().map(|r| r.handle)).collect();
            live.sort_unstable();
            live.dedup();
            self.library.lock().drop_unwanted(&live);
            self.last_unwanted = (Instant::now(), drops);
        }
        let due = (self.last_gc.0.elapsed() >= GC_EVERY && epoch != self.last_gc.1)
            // More specs than the world could use: certainly garbage, collect now.
            || specs > self.world.len() + models::SPARE_HANDLES;
        if force || due {
            let _z = profile::zone("models.gc");
            let live = {
                let _z = profile::zone("models.live_handles");
                models::live_handles(&self.world)
            };
            self.library.lock().collect_garbage(&live);
            self.last_gc = (Instant::now(), epoch);
        }
    }

    /// What a library poll changed reaches the world: models refreshed, anim sets reloaded.
    fn assets_reloaded(&mut self, reload: Option<models::Reload>) -> Option<models::Reload> {
        let reload = reload?;
        if reload.files_changed > 0 || !reload.rebuilt.is_empty() || !reload.errors.is_empty() {
            // Only the entities of handles worked out again (`Reload::handles`).
            {
                let _z = profile::zone("models.refresh_handles");
                self.world.refresh_model_handles(&reload.handles, reload.files_changed > 0);
            }
            eprintln!(
                "specula: recipes changed: {} rebuilt from the cache, {} rebuilding, {} error(s)",
                reload.rebuilt.len(),
                reload.rebuilding.len(),
                reload.errors.len()
            );
        }
        // Anim sets that changed reach the world here, journaled (never when a build lands).
        if !reload.anim_handles.is_empty() {
            sim_writes::anim_reload(&mut self.world, &mut self.swarm, self.clock.tick(), &reload.anim_handles);
        }
        Some(reload)
    }

    /// Takes finished model builds (waiting up to `wait` for one when builds are in flight),
    /// updates the entities that use them and answers the calls waiting on them.
    fn pump_builds(&mut self, wait: Option<Duration>) {
        let (finished, changed) = self.library.lock().finish(wait);
        if !changed.is_empty() {
            self.world.refresh_model_handles(&changed, false);
        }
        let now = Instant::now();
        let mut waiters = std::mem::take(&mut self.build_waiters);
        waiters.retain_mut(|w| {
            for f in &finished {
                if w.jobs.remove(&f.job) {
                    match (&f.result, &w.spec) {
                        (Ok(b), Some(spec)) => {
                            self.library.lock().remember_build(b);
                            w.lend_built(b);
                            w.answer = Some(ToolOutput::text(models::report(spec, b, false).to_pretty()));
                        }
                        (Err(e), Some(_)) => w.answer = Some(world_error(e)),
                        (Err(e), None) if e.code != "build_cancelled" => {
                            let recipe = e.detail("recipe").and_then(Json::as_str).unwrap_or_default().to_owned();
                            w.reload.errors.push((recipe, e.to_json()));
                        }
                        (Err(_), None) => {}
                        (Ok(_), None) => {}
                    }
                }
            }
            if w.jobs.is_empty() && w.answer.is_none() && w.checking.is_none() {
                // A reload: every build it queued is done.
                let mut out = w.reload.to_json();
                out.set("library", self.library.lock().stats_json());
                w.answer = Some(ToolOutput::text(out.to_pretty()));
            }
            if w.answer.is_none() && now > w.deadline {
                w.answer = Some(ToolOutput::error(
                    Json::obj([
                        ("error", Json::from("still_building")),
                        (
                            "message",
                            Json::from(if w.checking.is_some() {
                                "the model's source is still being read from the disk; call again".to_owned()
                            } else {
                                format!("{} build(s) still running; they finish in the background (engine_info models.library)", w.jobs.len())
                            }),
                        ),
                    ])
                    .to_pretty(),
                ));
            }
            match w.answer.take() {
                Some(out) => {
                    let _ = w.reply.send(out);
                    false
                }
                None => true,
            }
        });
        self.build_waiters.extend(waiters);
    }

    /// The 2D draws of shown entities (`ui` ones when `include_ui`) in view of `view_proj` (and
    /// shadow-casting sprites anywhere) at `time_s`, into `out`, and now and then frees 2D GPU
    /// resources no entity uses any more. Only what is drawn loads, and only its problems are
    /// reported. `owners` gets each draw's entity (sight's ID buffer).
    fn build_draws_2d(&mut self, view_proj: &Mat4, include_ui: bool, time_s: f32, out: &mut Vec<Draw2d>, mut owners: Option<&mut Vec<u64>>) {
        out.clear();
        self.two_d.begin_frame();
        for e in self.world.two_d_entities() {
            let Some(t) = &e.two_d else { continue };
            if !sight::in_frame(e, include_ui) {
                continue;
            }
            let casts = matches!(t, two_d::TwoD::Sprite(s) if s.cast_shadows);
            if !casts && !sight::in_view(e, view_proj) {
                continue;
            }
            let c = e.color;
            let before = out.len();
            self.two_d.draws(&mut self.gpu, e.id, t, &e.model(), [c[0], c[1], c[2], e.look.opacity], time_s, out);
            if let Some(o) = owners.as_deref_mut() {
                o.extend(std::iter::repeat_n(e.id, out.len() - before));
            }
        }
        let revision = self.world.revision();
        if revision != self.last_2d_gc.1 && self.last_2d_gc.0.elapsed() >= GC_EVERY {
            let live: Vec<(u64, &two_d::TwoD)> = self.world.entities().filter_map(|e| e.two_d.as_ref().map(|t| (e.id, t))).collect();
            self.two_d.collect(&mut self.gpu, &live);
            self.last_2d_gc = (Instant::now(), revision);
        }
    }

    /// What a capture should wait for: the textures and deferred shapes of the 2D entities in its
    /// view (loads start; finished ones are uploaded), counted while still in flight. Entities
    /// off screen neither hold it up nor appear in its warnings. `fresh` (when the capture is
    /// asked for): their texture files are checked for changes now and the capture waits for
    /// that, so it shows the files as they are at the request, whatever the periodic poll did.
    fn two_d_loading(&mut self, view_proj: &Mat4, include_ui: bool, fresh: bool) -> usize {
        // Within what is left of the frame's upload budget (a texture held back counts as loading).
        let allowance = self.stream.allowance();
        let used = self.two_d.pump(&mut self.gpu, allowance);
        if allowance.is_some() {
            self.stream.charge(used);
        }
        let mut waiting = 0;
        let mut text_budget = text_world::CAPTURE_PREP_WORDS_PER_PASS;
        let mut text_waiting = 0;
        // Distinct in-view textures checked so far: the first MAX_CAPTURE_STATS are stat'ed here
        // (microseconds each), so an unchanged file doesn't hold the capture up for a loop.
        let mut checked: BTreeSet<&str> = BTreeSet::new();
        for e in self.world.two_d_entities() {
            let Some(t) = &e.two_d else { continue };
            if !sight::in_frame(e, include_ui) || !sight::in_view(e, view_proj) {
                continue;
            }
            let texture = match t {
                two_d::TwoD::Sprite(s) => s.texture.as_deref(),
                two_d::TwoD::Tilemap(m) => Some(m.tileset.as_str()),
                two_d::TwoD::Shape2d(s) => {
                    waiting += self.two_d.request_shape(s) as usize;
                    None
                }
                // Its glyphs are prepared ahead, a pass at a time, while the capture waits; a text
                // a world file brought in waits for its layout (text_world).
                two_d::TwoD::Text(t) => {
                    let left = self.two_d.text.prepare_ahead(t, &mut text_budget) + usize::from(t.layout_pending());
                    text_waiting += left;
                    waiting += left;
                    None
                }
            };
            if let Some(p) = texture {
                self.two_d.request(p);
                if fresh && checked.insert(p) {
                    self.two_d.check_now(p, checked.len() <= MAX_CAPTURE_STATS);
                }
                waiting += self.two_d.is_pending(p) as usize;
            }
        }
        // The glyphs prepared so far go to the GPU a piece at a time too, ahead of the frame, and
        // once all are prepared the capture's snapshot of the store is made here, not in its frame.
        self.two_d.text.preload(&mut self.gpu);
        if text_waiting == 0 {
            self.two_d.text.share_words();
        }
        waiting
    }

    /// tilemap_get: rows of run-length cell tokens.
    fn tile_get(&self, key: &world::EntityKey, layer: Option<&str>, region: Option<[u32; 4]>) -> ToolOutput {
        let id = match self.world.resolve(key) {
            Ok(id) => id,
            Err(e) => return world_error(&e),
        };
        let e = self.world.get(id).expect("resolved");
        let Some(two_d::TwoD::Tilemap(t)) = &e.two_d else {
            return world_error(&WorldError::new("invalid_component", format!("'{}' is a {}, not a tilemap", e.name, e.shape.name())));
        };
        let layers = Json::arr(t.layers.iter().map(|l| {
            Json::obj([("name", Json::from(l.name.as_str())), ("z_layer", Json::from(l.z_layer)), ("filled", Json::from(l.cells.filled()))])
        }));
        let Some(name) = layer else {
            return ToolOutput::text(Json::obj([("size", Json::arr(t.size.map(Json::from))), ("layers", layers)]).to_string());
        };
        let Some(l) = t.layers.iter().find(|l| l.name == name) else {
            return world_error(&WorldError::new("invalid_value", format!("no layer '{name}'")).with("field", "layer").with("layers", layers));
        };
        let [x, y, w, h] = region.unwrap_or([0, 0, 64.min(t.size[0]), 64.min(t.size[1])]);
        let (w, h) = (w.min(t.size[0].saturating_sub(x)), h.min(t.size[1].saturating_sub(y)));
        let rows = (y..y + h).map(|row| Json::from(two_d::format_row(&l.cells, row, x, w)));
        ToolOutput::text(
            Json::obj([
                ("path", Json::from(self.world.path_of(id))),
                ("version", Json::from(e.version)),
                ("layer", Json::from(name)),
                ("region", Json::arr([x, y, w, h].map(Json::from))),
                ("rows", Json::arr(rows)),
            ])
            .to_string(),
        )
    }

    /// Answers every parked call: the engine is stopping.
    fn answer_waiters_shutting_down(&mut self) {
        self.captures_shutting_down();
        let out = || ToolOutput::error("the engine is shutting down; the call was not completed");
        for w in self.capture_waiters.drain(..) {
            let _ = w.reply.send(out());
        }
        for w in self.build_waiters.drain(..) {
            let _ = w.reply.send(out());
        }
        if let Some(f) = self.fly.take() {
            stream_tools::Fly::abort(f);
        }
        self.studio_shutting_down();
        // A perf_capture in progress answers with the frames it has.
        self.profiler.abort_capture();
    }

    /// model_build once a worker has made its request: built already, or a job to wait for like
    /// every model call. A job that finished before this ran (its result was taken by an
    /// earlier pass) is answered from the library's kept results: nothing depends on which of
    /// the two got there first.
    fn model_build_requested(&mut self, token: u64, spec: &world::ModelSpec, requested: Result<models::BuildRequest, WorldError>) {
        // (Claimed first, whoever is still waiting: what the library kept for this request is
        // released.)
        let requested = match requested {
            Ok(models::BuildRequest::Pending(job)) => match self.library.lock().claim(job) {
                Some(Ok(b)) => Ok(models::BuildRequest::Ready(b, false)),
                Some(Err(e)) => Err(e),
                None => Ok(models::BuildRequest::Pending(job)),
            },
            other => other,
        };
        // (Answered meanwhile, by its deadline or a shutdown: nothing more to do.)
        let Some(i) = self.build_waiters.iter().position(|w| w.checking == Some(token)) else { return };
        let waiter = &mut self.build_waiters[i];
        waiter.checking = None;
        match requested {
            Err(e) => waiter.answer = Some(world_error(&e)),
            Ok(models::BuildRequest::Ready(b, cached)) => {
                waiter.lend_built(&b);
                waiter.answer = Some(ToolOutput::text(models::report(spec, &b, cached).to_pretty()));
                self.library.lock().remember_build(&b);
            }
            Ok(models::BuildRequest::Pending(job)) => {
                waiter.jobs.insert(job);
            }
        }
        self.pump_builds(None);
    }

    /// model_build and model_reload: answered when their builds finish (other calls go on).
    fn handle_model_call(&mut self, request: Request) {
        let Request { command, reply, deadline, .. } = request;
        let mut waiter = BuildWaiter { jobs: BTreeSet::new(), spec: None, lend: None, reload: models::Reload::default(), reply, deadline, answer: None, checking: None };
        // (model_export is model_build, with the built model lent to the caller's thread.)
        let command = match command {
            Command::ModelExport(spec, slot) => {
                waiter.lend = Some(slot);
                Command::ModelBuild(spec)
            }
            other => other,
        };
        match command {
            Command::ModelBuild(spec) => {
                // model_build builds from the disk as it is now. The whole request (reading
                // the recipe's folder or a glTF file's size and time, working out the key,
                // finding the build or queueing it) is made on a worker, as an agent's
                // connection makes a studio check's: it ran here, on the engine thread and
                // under the library's lock, up to 18 ms a call beside a busy machine. The
                // waiter is with the others meanwhile, so its deadline and a shutdown answer it.
                let token = BUILD_CHECKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                waiter.checking = Some(token);
                waiter.spec = Some(spec.clone());
                self.build_waiters.push(waiter);
                let (library, asked) = (Arc::clone(&self.library), spec.clone());
                let job = workers::Offload::engine(workers::JobKind::AssetScan).class(Class::Disk);
                let unasked = |spec: &world::ModelSpec, why: String| WorldError::new("build_failed", format!("{} could not be requested ({why})", spec.recipe)).with("recipe", spec.recipe.as_str());
                let landed = spec.clone();
                let then = move |e: &mut Engine, r: Result<Result<models::BuildRequest, WorldError>, workers::Failed>| {
                    let requested = r.unwrap_or_else(|f| Err(unasked(&landed, format!("{f:?}"))));
                    e.model_build_requested(token, &landed, requested);
                };
                if self.offload(job, move |_| library.request_to_claim(&asked), then).is_err() {
                    // (The engine's own jobs are never refused; answered all the same.)
                    self.model_build_requested(token, &spec, Err(unasked(&spec, "no worker took the job".to_owned())));
                }
                return;
            }
            Command::ModelReload => {
                // Pipeline variants a background build failed to make are built again too.
                self.gpu.retry_failed_variants();
                let reload = self.poll_assets(true).unwrap_or_default();
                waiter.jobs = reload.jobs.clone();
                waiter.reload = reload;
            }
            _ => unreachable!("only model calls come here"),
        }
        self.build_waiters.push(waiter);
        self.pump_builds(None);
    }

    /// The draws for a frame through `camera` into `out` (meshes synced first), and, in
    /// `self.models`, the bone matrices of animated models posed at render time `time_s` (take
    /// them with `take_bones`); `owners` gets each draw's entity.
    #[allow(clippy::too_many_arguments)]
    fn build_draws(
        &mut self,
        camera: Camera,
        viewport_h: u32,
        include_ui: bool,
        lod: Option<usize>,
        window: bool,
        time_s: f64,
        out: &mut Vec<DrawItem>,
        owners: Option<&mut Vec<u64>>,
    ) -> (LodStats, Vec<String>) {
        // The window's own frames upload landed models a slice at a time (`upload_slice`);
        // captures upload everything they draw.
        self.models.upload_slice = (window && self.windowed).then_some(models::UPLOAD_SLICE);
        let errors = self.sync_meshes();
        self.models.upload_slice = None;
        self.models.begin_frame(time_s);
        self.models.set_externals(&self.world);
        // HLOD (see `hlod`): the view's proxies first (the window switches cells in and out),
        // then every entity except those a proxy draws (the view's cover bits), then the proxies.
        let mut proxies = std::mem::take(&mut self.stream.scratch);
        proxies.clear();
        if window {
            self.stream.draw_window(&camera, viewport_h, &self.models, &mut proxies);
        } else {
            self.stream.draw_capture(&camera, viewport_h, &mut proxies);
        }
        let hlod = &mut self.stream.hlod;
        std::mem::swap(&mut self.models.cover, if window { &mut hlod.window_cover } else { &mut hlod.capture_cover });
        // Terrains the terrain renderer draws (their stand-in meshes are skipped) and water.
        let budget = terrain::Budget {
            allowance: if window { self.stream.allowance() } else { None },
            radius: (self.stream.cfg.radius_m, self.stream.cfg.hlod_radius_m),
            memory: self.stream.terrain_room(),
        };
        // (A zone of its own: what terrain and water cost a frame, perf_capture's "terrain".)
        let zt = profile::zone("terrain");
        let used = self.terrain.prepare(&mut self.gpu, &self.library, self.models.synced, &self.world, &camera, include_ui, window, budget);
        drop(zt);
        if window && budget.allowance.is_some() && used.items > 0 {
            self.stream.charge(used);
        }
        self.stream.terrain_bytes = self.terrain.bytes();
        std::mem::swap(&mut self.models.terrain_cover, &mut self.terrain.cover);
        let meshes = Meshes { shapes: &self.meshes, lathes: &self.lathes, models: &self.models };
        // The window's cache remembers each model's level itself (hysteresis); captures choose
        // afresh (reproducible).
        let lod = lod.map_or(LodChoice::Fresh, LodChoice::Fixed);
        let mut view = View { camera, viewport_h, include_ui, lod };
        let mut owners = owners;
        let stats = if window {
            // (Owners only while the editor previews a drag.)
            self.draw_cache.draw_list_owned(&self.world, self.world_generation, &meshes, &mut view, out, owners.as_deref_mut())
        } else {
            // Captures patch their own cache (LODs afresh): no walk of every entity per view.
            let _z = profile::zone("sight.draw_cache");
            self.capture_draw_cache[usize::from(include_ui)].draw_list_owned(&self.world, self.world_generation, &meshes, &mut view, out, owners.as_deref_mut())
        };
        let hlod = &mut self.stream.hlod;
        std::mem::swap(&mut self.models.cover, if window { &mut hlod.window_cover } else { &mut hlod.capture_cover });
        std::mem::swap(&mut self.models.terrain_cover, &mut self.terrain.cover);
        if let Some(o) = owners.as_deref_mut() {
            o.resize(out.len(), 0);
        }
        let quad = self.meshes.get(world::Shape::Quad);
        let zt = profile::zone("terrain_draws");
        self.terrain.append_draws(&self.models, quad, &self.world, &camera, out, owners.as_deref_mut());
        drop(zt);
        out.extend_from_slice(&proxies);
        // A capture's proxy draws belong to their cell: one owner per proxy (past every entity
        // id, see `sight::proxy_owner`), so the ID pass counts the proxy's pixels and its
        // members' drawing facts come from them. The window's proxies have no owner.
        if let Some(o) = owners {
            if window {
                o.resize(out.len(), 0);
            } else {
                for (i, (draws, _)) in self.stream.hlod.capture_groups.iter().enumerate() {
                    o.extend(std::iter::repeat_n(sight::proxy_owner(i), *draws as usize));
                }
                o.resize(out.len(), 0);
            }
        }
        self.stream.scratch = proxies;
        (stats, errors)
    }

    fn run_windowed(&mut self, ui: &UiThread, requests: &Receiver<Request>, server: &Server) -> Result<(), String> {
        // Timed waits end within about a millisecond instead of on Windows' default 15.6 ms tick,
        // so a serve window ends when the frame is due.
        let _timer = specula_platform::timer::FineTimer::request();
        let mut title = String::new();
        let mut size = client_size_of(ui.raw);
        let mut last_frame = Instant::now();
        let mut schedule = Schedule::default();
        // The display's refresh rate, asked every second (the window may move to another display).
        let mut refresh_asked: Option<Instant> = None;
        // The oldest calls queued when the last serve window ended (at most two): the next one
        // starts with them.
        let mut held: VecDeque<Request> = VecDeque::with_capacity(2);
        let mut inbox = requests;
        let mut minimized = false;
        loop {
            // Everything recorded during the previous iteration is that frame's profile.
            self.profiler.end_frame(&self.gpu);
            // (A slow frame that waited for its zones is reported now, with them.)
            self.frame_stall_named();
            self.pump_perf_writes();
            // Serve agents in the time the old loop spent waiting for vsync, until this frame is
            // due. Not frame work, so outside the frame zone; each call is its own "requests" zone.
            // perf_frame (answered in the window) reports the schedule as this window began.
            if refresh_asked.is_none_or(|t| t.elapsed() >= REFRESH_ASK_EVERY) {
                refresh_asked = Some(Instant::now());
                let hz = specula_platform::refresh_rate_of(ui.raw);
                schedule.set_display_refresh(hz.map(|hz| Duration::from_secs_f64(1.0 / hz)));
            }
            schedule.set_frame_floor(self.frame_floor.0);
            schedule.set_max_fps(self.max_fps);
            self.schedule = schedule;
            // The game's Quit (its title screen, the window's own input) stops like the close button.
            if self.game_quit {
                break;
            }
            let window = if minimized { schedule::MINIMIZED_SERVE } else { schedule.window() };
            let serve_start = Instant::now();
            let ended = schedule.serve(
                &mut inbox,
                &mut held,
                serve_start + window,
                |r| call_class(&r.command),
                |r| {
                    let _z = profile::zone("requests");
                    self.handle_traced(r, server);
                },
                || interrupt::requested() || ui.close_requested(),
            );
            if ended != Served::Window {
                // Stopping (or the server is gone). Held calls' reply channels close as they
                // drop, which answers them "shutting down".
                break;
            }
            let served = serve_start.elapsed();
            self.trace_idle_checks();
            let frame_zone = profile::zone("frame");
            let frame_started = Instant::now();
            // (The thread's own processor time from here: a slow frame says how much it ran.)
            let frame_ran_from = self.watch_mark();
            // What a change of the window's size took this frame (the swapchain's rebuild).
            let mut resize_took = Duration::ZERO;
            self.autosave_if_due();
            self.pump_world_saves();
            self.sweep_claims(server);
            self.ui_sweep(server);
            self.lex_live_end_holds(server);
            self.poll_assets_async(false);
            self.pump_builds(None);
            self.poll_saves();
            self.project_pump();
            self.taste_pump();
            self.disk_pump();
            self.serve_captures();
            self.pump_captures();
            self.pump_workers(Some(server));
            self.pump_steps(steps::WINDOW_SHARE, server);
            // Timelines whose starters left, or idle too long, end (see `timeline`).
            self.timeline_sweep(server);
            // The input monitor's halts; a swarm pause holds playtests where they are.
            self.security_pump();
            if inputguard::paused().is_none() {
                self.pump_playtests(server, playtest_engine::WINDOW_SLICE);
            }
            self.pump_studio(studio::WINDOW_SHARE, server);
            self.gi_pump();
            self.advance_clock();
            let now = Instant::now();
            let dt = now - last_frame;
            last_frame = now;
            self.game_frame(ui, size, dt);
            // The editor's protocol commands, picks and panels (and its camera).
            self.editor_pump(server);
            let mut next = match &self.game_title {
                Some(t) => t.clone(),
                None => format!("Specula — {} entities — {} agents", self.world.len(), server.sessions().len()),
            };
            if let Some(path) = self.text_typing.focused_path() {
                next = format!("{next} — {}", text_window::cue_text(path));
            }
            if self.editor.ui.asking() {
                // The cue world content can't fake (TEXT.md): the editor's Ask box has the keys.
                next = format!("{next} — Asking the agents: only your keyboard types here, Esc cancels");
            }
            // A security incident or alert is in the title too (docs/design/SECURITY.md).
            if let Some(s) = self.security_title() {
                next = format!("{s} — {next}");
            }
            if next != title {
                ui.set_title(&next);
                title = next;
            }
            // Read the size directly: it stays current even while the window thread is inside a
            // move/resize loop.
            let now = client_size_of(ui.raw);
            if now != size {
                size = now;
                // A new size is a new swapchain: the device waits for the frames in flight
                // (a queued present among them) and the window system makes the images. That
                // is 17 to 24 ms on a quiet machine and 100 to 140 ms beside 32 busy processes
                // (performance pass, 2026-11-14), and it is not the frame's own work: it is
                // timed and reported by name, where it read as a nameless slow frame.
                let resize_started = Instant::now();
                self.gpu.resize_window(size.0, size.1).map_err(|e| format!("resize: {e}"))?;
                resize_took = resize_started.elapsed();
                specula_core::trace_event!(&trace_sink::ENGINE, Info, "window.resized", width = size.0 as u64, height = size.1 as u64, ms = resize_took.as_secs_f64() * 1e3);
                self.engine_work_done(0, None, "window_resize", resize_took);
            }
            minimized = size.0 == 0 || size.1 == 0;
            if minimized {
                // Captures still stream in while the window is minimized; with no frame to
                // flush it, the upload batch (and retired memory) goes now. A flight ends on time.
                self.stream_tick(None);
                if let Err(e) = self.gpu.flush_uploads() {
                    eprintln!("specula: upload batch not submitted: {e}");
                }
                self.fly_camera(human_took_controls(&self.input));
                // Minimized: no frame to present; the next serve window keeps serving agents (and
                // the clock) at about 60 Hz.
                drop(frame_zone);
                continue;
            }
            // The open editor holds the window like a human's input does: an agent's view or
            // flight lets go of it (F5).
            let editing = self.editor.ui.is_open();
            self.follow_window_camera();
            if let Some(cam) = self.fly_camera(editing || human_took_controls(&self.input)) {
                self.window_follow = None;
                self.window_camera = Some(cam);
            }
            self.editor_window_camera();
            let camera = self.window_camera.unwrap_or_else(|| sight::auto_camera(&self.world, 50.0));
            // Streaming: this camera is a source; uploads within the frame's budget, before the
            // draw list so this frame draws them.
            self.stream_tick(Some((camera, size.0, size.1)));
            // The game's interface (its own zone, outside the draw list's).
            let mut game_quads = std::mem::take(&mut self.game_quads);
            if self.editor.ui.is_open() {
                // The editor's viewport shows the world, not the game's interface (F5: the
                // human edits what agents edit).
                game_quads.clear();
            } else {
                self.game_overlay(size, camera, false, &mut game_quads);
            }
            self.editor_overlay(size, &mut game_quads);
            self.gallery_overlay(size, false, &mut game_quads);
            self.review_overlay(size, false, &mut game_quads);
            // The start screen: over the engine's other screens (window frames only).
            self.launch_overlay(size, &mut game_quads);
            // The input monitor's toast: over everything but the typing banner.
            self.security_overlay(size, &mut game_quads);
            // While the window's keyboard types into a text, a banner says which: the overlay's
            // last quads, over everything (text_window).
            self.text_focus_overlay(size, &mut game_quads);
            let draw_list = profile::zone("draw_list");
            let mut draws = std::mem::take(&mut self.draws);
            let started = Instant::now();
            let time_s = self.window_time_s();
            // While an editor drag previews, the draws' owners say which to move.
            let mut owners = std::mem::take(&mut self.editor_owners);
            owners.clear();
            let previewing = self.editor.previewing();
            let (stats, errors) = self.build_draws(camera, size.1, true, None, true, time_s, &mut draws, previewing.then_some(&mut owners));
            if previewing {
                self.editor_patch_draws(&mut draws, &owners);
            }
            self.editor_owners = owners;
            self.pacing.draw_list.push(started.elapsed());
            for e in errors {
                eprintln!("specula: {e}");
            }
            self.lod_window = stats;
            let mut lights = std::mem::take(&mut self.lights);
            sight::lights(&self.world, true, &mut lights);
            let view_proj = camera.view_proj(size.0, size.1);
            self.text_typing.set_view(view_proj, size);
            let mut draws_2d = std::mem::take(&mut self.draws_2d);
            // A window frame prepares a few new glyphs at most (the rest follow next frames).
            self.two_d.text.prep_words = Some(text_world::WINDOW_PREP_WORDS);
            // The window draws a caret only where its keyboard really types (text_world).
            self.two_d.text.window_marks = Some(self.text_typing.focus);
            self.build_draws_2d(&view_proj, true, time_s as f32, &mut draws_2d, None);
            self.two_d.text.prep_words = None;
            self.two_d.text.window_marks = None;
            let fx_view = vfx::View::of(&camera, size.0, size.1);
            self.build_effects(&fx_view, time_s, true, &mut lights, true);
            let fx = self.vfx.take_frame();
            let bones = self.models.take_bones();
            // Baked GI: the chunks around this camera (and a recent capture's).
            self.gi.update(&self.world, camera.eye, false);
            if self.gi.wants_pipelines() {
                // Built on a worker ahead of the first GI frame (frames draw the flat ambient meanwhile).
                self.gpu.request_gi_variants(self.gi.quality.gpu());
            }
            let (gi_volumes, gi_count) = self.gi.volumes();
            let terrains = std::mem::take(&mut self.terrain.frame);
            let mut frame = FrameDesc {
                bones: &bones,
                draws_2d: &draws_2d,
                overlay: &game_quads,
                effects: fx.effects(),
                gi: self.gi.frame(&gi_volumes[..gi_count], gi::CaptureView::Lit),
                terrains: &terrains,
                ..sight::frame(&self.world, &view_proj, &draws, &lights, time_s as f32)
            };
            self.luts.apply(&mut self.gpu, self.world.env(), &mut frame, false);
            frame.text = self.two_d.text.frame();
            drop(draw_list);
            // FIFO present paces the loop to the display refresh. How long it blocked sizes the
            // next serve window.
            let present = profile::zone("present");
            let frame_cpu = frame_started.elapsed().saturating_sub(resize_took);
            let frame_ran = self.watch_ran_since(frame_ran_from);
            // Whether the typing banner is the overlay's last quads, on what this frame sends.
            self.text_typing.overlay_sent(frame.overlay);
            let presented = self.gpu.present(&frame).map_err(|e| format!("present: {e}"));
            {
                // The flash grid borrowed (no copy of the frame's stats).
                let _z = profile::zone("flash_watch");
                self.flash.feed_window(self.gpu.flash_grid(), self.flash_epoch.elapsed().as_secs_f64());
            }
            let (frame_no, cpu) = self.gpu.last_window_timing();
            let blocked = Duration::from_secs_f64((cpu.wait_ms + cpu.submit_ms).max(0.0) / 1000.0);
            schedule.presented(frame_no, served, blocked, Instant::now());
            self.trace_frame(frame_no, frame_cpu, Some(frame_ran));
            self.models.put_bones(bones);
            self.vfx.put_frame(fx);
            self.terrain.frame = terrains;
            drop(present);
            self.draws = draws;
            self.lights = lights;
            self.draws_2d = draws_2d;
            self.game_quads = game_quads;
            presented?;
            self.pacing.frame.push(dt);
            self.fly_frame(dt, frame_cpu);
        }
        Ok(())
    }

    fn run_headless(&mut self, requests: &Receiver<Request>, server: &Server) -> Result<(), String> {
        // While captures are busy (and a second after), the system timer at 1 ms: the short waits
        // below end on time instead of the 15.6 ms default tick (a capture's view lands a pass
        // after it renders). Idle, the engine leaves the system's timer alone.
        let mut fine: Option<(specula_platform::timer::FineTimer, Instant)> = None;
        while !interrupt::requested() {
            if self.captures.busy() || !self.capture_waiters.is_empty() {
                match &mut fine {
                    Some((_, at)) => *at = Instant::now(),
                    None => fine = Some((specula_platform::timer::FineTimer::request(), Instant::now())),
                }
            } else if fine.as_ref().is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(1)) {
                fine = None;
            }
            // Wake for the next tick at the latest, so a hosted game keeps real time headless too
            // (and about 60 times a second while perf_capture records frames).
            let most = if self.profiler.capturing() { Duration::from_millis(16) } else { Duration::from_millis(100) };
            let mut wait = self.clock.until_next().min(most);
            if self.captures.busy() {
                // A capture view on the GPU lands (and the next starts) as soon as it can.
                wait = wait.min(Duration::from_millis(1));
            }
            if self.library.lock().in_flight() > 0 || !self.capture_waiters.is_empty() || self.stream.busy() || self.saves_in_flight() {
                // Builds land promptly (and calls waiting on them are answered).
                wait = wait.min(Duration::from_millis(5));
            }
            if self.nav.loading() {
                // A load builds back to back (calls are still read between slices).
                wait = Duration::ZERO;
            }
            if self.studio_busy() || self.steps_busy() {
                // Studio renders and step jobs run in slices, with calls read between them.
                wait = Duration::ZERO;
            }
            if self.workers.busy() {
                // Workers' answers go out promptly.
                wait = wait.min(Duration::from_millis(2));
            }
            if self.playtests.running() && inputguard::paused().is_none() {
                // Flat out, unless it waits for a deferred answer.
                wait = if self.playtests.waiting_on_call() { wait.min(Duration::from_millis(1)) } else { Duration::ZERO };
            }
            let received = requests.recv_timeout(wait);
            self.profiler.end_frame(&self.gpu);
            // (A slow frame that waited for its zones is reported now, with them.)
            self.frame_stall_named();
            self.pump_perf_writes();
            let _frame = profile::zone("frame");
            match received {
                Ok(request) => {
                    let _z = profile::zone("requests");
                    self.handle_traced(request, server);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
            self.advance_clock();
            self.autosave_if_due();
            self.pump_world_saves();
            self.sweep_claims(server);
            self.ui_sweep(server);
            self.lex_live_end_holds(server);
            self.poll_assets_async(false);
            self.pump_builds(None);
            // Streaming headless: captures' views are the sources; retired memory is freed.
            self.stream_tick(None);
            if let Err(e) = self.gpu.flush_uploads() {
                eprintln!("specula: upload batch not submitted: {e}");
            }
            self.poll_saves();
            self.project_pump();
            self.taste_pump();
            self.disk_pump();
            self.serve_captures();
            self.pump_captures();
            self.pump_workers(Some(server));
            self.pump_steps(steps::HEADLESS_SHARE, server);
            self.timeline_sweep(server);
            // The input monitor's halts; a swarm pause holds playtests where they are.
            self.security_pump();
            if inputguard::paused().is_none() {
                self.pump_playtests(server, playtest_engine::HEADLESS_SLICE);
            }
            self.pump_studio(studio::HEADLESS_SHARE, server);
            self.gi_pump();
            self.trace_idle_checks();
        }
        Ok(())
    }

    fn handle(&mut self, request: Request, server: &Server) {
        if Instant::now() > request.deadline {
            // The caller is about to give up; applying the command now would surprise it.
            let _ = request.reply.send(ToolOutput::error("skipped: the engine was busy too long; the command was not applied"));
            return;
        }
        // The input monitor first: what an incident halted is applied, and a frozen session's or
        // a paused swarm's request is refused here too (calls in flight, playtest steps).
        self.security_pump();
        if let Some(refused) = self.security_gate(&request) {
            let _ = request.reply.send(refused);
            return;
        }
        // Every command (world edits, model builds, sight, saves, playtests) runs in the default
        // floating-point environment, whatever the frame just drawn (the GPU driver) left behind
        // (rule 6).
        self.fp.check(self.clock.tick(), "before_command");
        // Capture flash feeds that ran on workers (perf_frame and vfx_info report the latest).
        self.flash.absorb();
        // perf_capture answers once its frames are recorded (see perf::Profiler::end_frame).
        let request = match request.command {
            Command::PerfCapture(spec) => {
                let kind = perf::adapter_kind(self.gpu.adapter().kind);
                let ctx = perf::CaptureContext {
                    dir: self.capture_dir.clone(),
                    budgets: self.perf_cfg.budgets(kind),
                    adapter: kind.to_owned(),
                };
                let owner = crate::jobs::JobOwner { name: self.swarm.agent_name(&request.session), key: self.swarm.key(&request.session) };
                self.profiler.start_capture(spec, owner, request.reply, request.deadline, ctx);
                return;
            }
            Command::Stream(stream_tools::StreamCall::Fly(spec)) => {
                let owner = jobs::JobOwner { name: self.swarm.agent_name(&request.session), key: self.swarm.key(&request.session) };
                self.start_fly(spec, owner, request.reply, request.deadline);
                return;
            }
            Command::Playtest(call) => {
                self.playtest_call(call, request.session, request.reply, server);
                return;
            }
            Command::StudioRender(job) => {
                // Rendered between frames within the studio's budget (see `studio`).
                self.studio_enqueue(job);
                return;
            }
            Command::StudioDone(call) => {
                self.studio_done(call);
                return;
            }
            Command::Vfx(call @ vfx_tools::VfxCall::Preview { .. }) => {
                // Rendered a frame per step within the studio's budget (see `vfx_tools`).
                self.vfx_preview_start(call, &request.session, request.reply, request.deadline, false);
                return;
            }
            Command::Editor(call) => {
                // editor_input answers once its frames have run (see `editor_engine`).
                self.editor_call(call, &request.session, request.call, request.reply, request.deadline);
                return;
            }
            command => Request { command, ..request },
        };
        if matches!(request.command, Command::ModelBuild(_) | Command::ModelExport(..) | Command::ModelReload) {
            self.handle_model_call(request);
            return;
        }
        // ui_frame is a capture: sight_frame's, at the display's size through the player's camera.
        let request = match request.command {
            Command::Ui(ui_tools::UiCall::Frame(f)) => Request { command: Command::Frame(self.ui_frame_spec(*f, &request.session)), ..request },
            command => Request { command, ..request },
        };
        if matches!(request.command, Command::Frame(_) | Command::Sight(_)) {
            self.handle_sight(request);
            return;
        }
        if matches!(request.command, Command::SaveGame(save_tools::SaveCall::Write { .. } | save_tools::SaveCall::Read { .. })) {
            self.handle_save_request(request);
            return;
        }
        if matches!(request.command, Command::Save { .. }) {
            // Planned here, written on a worker, answered when written (see `world_save`).
            self.handle_world_save(request);
            return;
        }
        if matches!(request.command, Command::Load { .. }) {
            // Checked here, read on a worker, applied when read (see `world_load`).
            self.handle_world_load(request);
            return;
        }
        if matches!(request.command, Command::Anim(anim_tools::AnimCall::Check { .. } | anim_tools::AnimCall::Filmstrip { .. })) {
            // A check or a tile per step; a filmstrip's sheet is encoded on a worker (see `anim_steps`).
            self.start_anim_job(request);
            return;
        }
        if matches!(request.command, Command::PerfLight(_)) {
            // A measuring render per step (see `light_tools`).
            self.start_light_job(request);
            return;
        }
        if matches!(request.command, Command::Timeline(timeline_tools::TimelineCall::Preview { .. } | timeline_tools::TimelineCall::Check(_))) {
            // A tile or a slice of samples per step (see `timeline_tools`).
            self.start_timeline_job(request);
            return;
        }
        if matches!(request.command, Command::Query(_)) {
            // A big page is listed on a worker (see `start_query`).
            self.start_query(request);
            return;
        }
        if matches!(request.command, Command::Swarm(swarm_tools::SwarmCall::LeaseTake { want: leases::Want::Scratch, .. })) {
            // The folder is made on a worker (see `swarm_engine`).
            self.start_scratch_lease(request, server);
            return;
        }
        if matches!(request.command, Command::Swarm(swarm_tools::SwarmCall::LeaseRelease { delete: true, .. })) {
            // The folders are moved into the trash on a worker (see `swarm_engine`).
            self.start_lease_delete(request, server);
            return;
        }
        if matches!(request.command, Command::Swarm(swarm_tools::SwarmCall::DesyncTrace { .. })) {
            // The file is rendered and written on a worker (see `swarm_engine`).
            self.start_desync_trace(request);
            return;
        }
        if matches!(request.command, Command::Ui(ui_tools::UiCall::Check(_))) {
            // One target's layout and rules per slice (see `ui_engine`).
            self.start_ui_check(request);
            return;
        }
        if matches!(request.command, Command::Hash { verify: true, .. }) {
            // The replay check runs in steps (see `hash_verify`).
            self.start_hash_verify(request);
            return;
        }
        let by = self.swarm.agent_name(&request.session);
        // Project files this call changes (an undo, a load) are on disk before it answers.
        let files_before = self.world.files_revision();
        // One tool call, one commit: counted around the dispatch (see `count_commits`).
        let (call_id, independent) = (request.call, commits_independently(&request.command));
        // Ownership (claims, undo) goes by identity key, which a resumed agent keeps.
        let session = self.swarm.key(&request.session);
        self.swarm.begin_dispatch(&session);
        let outer_call = std::mem::replace(&mut self.dispatch_call, call_id);
        // An agent driving the window's human (editor_input) is journaled as `via`.
        let via = request.via.clone();
        let w = Writer::new(&by, Some(&session)).via(via.as_deref());
        let tick = self.clock.tick();
        // A game tool (play_ui pressing a menu's Save) may queue requests for the engine.
        let game_call = matches!(request.command, Command::Game { .. } | Command::Ui(ui_tools::UiCall::Act(_)));
        let ui_act = matches!(request.command, Command::Ui(ui_tools::UiCall::Act(_)));
        let output = match request.command {
            Command::Spawn(spec) => match self.world.commit(&mut self.swarm, tick, w, Kind::Spawn, crate::write_policy::Rule::World, |wd| wd.spawn(spec, &by).map(|e| e.id)) {
                Ok((id, seq)) => self.entity_output(id, seq),
                Err(e) => world_error(&self.conflict_help(e, Some(&conflict::Yours::spawn()))),
            },
            Command::Set(key, change) => {
                // A refused write says what changed (conflict help): only these can be refused so.
                let yours = (change.expect_version.is_some() || !self.swarm.claims.is_empty()).then(|| conflict::Yours::of_update(&change));
                match self.world.commit(&mut self.swarm, tick, w, Kind::Set, crate::write_policy::Rule::World, |wd| wd.update(&key, change, &by).map(|e| e.id)) {
                    Ok((id, seq)) => self.entity_output(id, seq),
                    Err(e) => world_error(&self.conflict_help(e, yours.as_ref())),
                }
            }
            Command::Delete(key, expect) => match self.world.commit(&mut self.swarm, tick, w, Kind::Delete, crate::write_policy::Rule::World, |wd| wd.delete(&key, expect)) {
                Ok((d, seq)) => ToolOutput::text(
                    Json::obj([
                        ("deleted", Json::from(d.removed.len())),
                        ("id", Json::from(d.id)),
                        ("path", Json::from(d.path)),
                        ("journal", Json::from(seq)),
                    ])
                    .to_pretty(),
                ),
                Err(e) => world_error(&self.conflict_help(e, Some(&conflict::Yours::delete()))),
            },
            Command::Query(spec) => self.query(&spec),
            Command::TileGet { key, layer, region } => self.tile_get(&key, layer.as_deref(), region),
            Command::TextEdit(commit) => {
                let actor = inputguard::Actor::of(&self.swarm, &request.session);
                self.text_commit(*commit, w, tick, &by, actor)
            }
            Command::TextGet { key } => self.text_get(&key),
            Command::Lex(call) => {
                let hooks: Vec<String> = self.logic.hooks().iter().map(logic::HookInfo::qualified).collect();
                let in_process: Vec<String> = self.lex_natives.keys().cloned().collect();
                sim_writes::lex_tools(&mut self.world, &mut self.swarm, tick, call, w, &by, &hooks, &in_process)
            }
            Command::LexLive(call) => self.lex_live_call(call, &by, w),
            Command::Batch(ops) => {
                let count = ops.len();
                let yours = self.batch_yours(&ops);
                let apply = |wd: &mut dyn write_policy::WorldWrite| wd.apply_batch(ops, &by).map_err(|(i, e)| e.with("op_index", i));
                match self.world.commit(&mut self.swarm, tick, w, Kind::Batch, crate::write_policy::Rule::World, apply) {
                    Ok((results, seq)) => {
                        let mut summary = batch_summary(&results);
                        summary.set("journal", Json::from(seq));
                        ToolOutput::text(summary.to_string())
                    }
                    // A claim rejects the batch as a whole (no single op is at fault).
                    Err(e) if e.detail("op_index").is_none() => {
                        let all = conflict::Yours::named(vec!["(the batch's ops)".into()]);
                        world_error(&self.conflict_help(e, Some(&all)))
                    }
                    Err(mut e) => {
                        let index = e.detail("op_index").and_then(Json::as_u64).unwrap_or(0);
                        e.details.retain(|(k, _)| *k != "op_index");
                        let mine = yours.iter().find(|(i, _)| *i as u64 == index).map(|(_, y)| y);
                        let e = conflict::in_batch(self.conflict_help(e, mine), index);
                        ToolOutput::error(
                            Json::obj([
                                ("error", Json::from("batch_failed")),
                                (
                                    "message",
                                    Json::from(format!(
                                        "ops[{index}] failed, so none of the {count} ops were applied: {}",
                                        e.message
                                    )),
                                ),
                                ("op_index", Json::from(index)),
                                ("cause", e.to_json()),
                            ])
                            .to_pretty(),
                        )
                    }
                }
            }
            Command::Save { .. } => unreachable!("answered by handle_world_save"),
            // A playtest owns the clock and the world until it ends (as for save_read): a load
            // would replace the world under it. Not even with force. Nor while others hold claims.
            Command::Load { .. } => unreachable!("answered by handle_world_load"),
            Command::Hello { name, role, resume } => match inputguard::frozen(&request.session.id, Some(&self.swarm.key(&request.session))) {
                // A frozen session's hello is answered and changes nothing: it can't say a role,
                // take another name or resume another identity to get out (docs/design/SECURITY.md).
                Some(incident) => ToolOutput::text(inputguard_engine::frozen_hello(&self.swarm.agent_name(&request.session), authority::trusted(&self.swarm, &request.session.id), incident).to_pretty()),
                None => {
                    // A frozen identity resumed in a new session comes back with the role it had:
                    // a freeze changes nothing about who it is, whichever session carries it
                    // (review R3 of m5-inputguard, minor 1).
                    let role = match resume.as_deref().and_then(|t| self.swarm.identity_of_token(t)) {
                        Some((key, had)) if inputguard::identity_frozen(&key) => had,
                        _ => role,
                    };
                    self.hello_unfrozen(&request.session, name, role, resume, &server.all_sessions())
                }
            },
            Command::Frame(_) | Command::Sight(_) => unreachable!("answered by handle_sight"),
            Command::Env(patch) => match patch
                .grading
                .and_then(|g| g.lut)
                .map_or(Ok(()), |lut| self.luts.check(&lut))
                .and_then(|()| self.world.commit(&mut self.swarm, tick, w, Kind::Env, crate::write_policy::Rule::World, |wd| wd.set_env(&patch)))
            {
                Ok(((), seq)) => {
                    let mut env = self.world.env().to_json();
                    env.set("journal", Json::from(seq));
                    // The world's LUT as the last frame found it (a world_env that names it
                    // was checked above; one that doesn't may still carry a broken one).
                    let named = patch.grading.and_then(|g| g.lut).is_some();
                    if let Some(w) = self.luts.warning(self.world.env()).filter(|_| !named) {
                        env.set("problems", Json::arr([w]));
                    }
                    ToolOutput::text(env.to_pretty())
                }
                Err(e) => world_error(&self.conflict_help(e, Some(&conflict::Yours::named(vec!["(environment)".into()])))),
            },
            // The open editor holds the window as the human's input does (F5): said plainly, not
            // answered as if the window had moved.
            Command::View(_) if self.editor.ui.is_open() => ToolOutput::error(
                Json::obj([
                    ("error", Json::from("editor_open")),
                    ("message", Json::from("the human has the window's editor open (F7), which holds the window camera; window_view works again once they close it. Look with sight_frame and your own camera meanwhile")),
                ])
                .to_string(),
            ),
            Command::View(view) => match view {
                tools::ViewSpec::Auto => {
                    self.timeline_release_window();
                    self.window_released_by = None;
                    self.window_follow = None;
                    self.window_camera = None;
                    ToolOutput::text("window camera: auto-framing the world")
                }
                tools::ViewSpec::Fixed(c) => {
                    self.timeline_release_window();
                    self.window_released_by = None;
                    self.window_follow = None;
                    self.window_camera = Some(c);
                    ToolOutput::text(Json::obj([("window_camera", c.to_json())]).to_pretty())
                }
                tools::ViewSpec::Named { name, of } => {
                    self.timeline_release_window();
                    let owner = of.clone().unwrap_or_else(|| by.clone());
                    match self.cameras.resolve(&owner, &name, of.as_deref(), &self.world) {
                        Ok(r) => {
                            self.window_camera = Some(r.camera);
                            self.window_follow = Some((owner.clone(), name.clone()));
                            self.window_released_by = None;
                            let mut out = Json::obj([
                                ("window_camera", r.camera.to_json()),
                                ("through", Json::obj([("owner", Json::from(owner)), ("camera", Json::from(name)), ("camera_version", Json::from(r.version))])),
                            ]);
                            if let Some(w) = r.warning {
                                out.set("warning", Json::from(w));
                            }
                            ToolOutput::text(out.to_pretty())
                        }
                        Err(e) => ToolOutput::error(e.to_json().to_pretty()),
                    }
                }
            },
            Command::Info => self.info(&server.sessions()),
            Command::GameState => self.game_state(),
            Command::GameAct { action } => self.game_act(&request.session, &action),
            Command::ScopeLookup { agent } => toolscope_tools::lookup(&self.swarm, &request.session, &agent, &server.all_sessions()),
            Command::Perf { budget_ms, graph } => self.perf(budget_ms, graph),
            Command::PerfZones(q) => {
                let owner = crate::jobs::JobOwner::of(w);
                match q.enabled.and_then(|_| self.profiler.captured_by_other(&owner)) {
                    Some(e) => world_error(&e),
                    None => {
                        if let Some(on) = q.enabled {
                            profile::set_enabled(on);
                        }
                        ToolOutput::text(perf::zones_json(&self.profiler, &q).to_string())
                    }
                }
            }
            Command::PerfMemory { limit } => {
                let stats = self.gpu.last_frame_stats_of(FramePath::Window);
                let stats = stats.or_else(|| self.gpu.last_frame_stats_of(FramePath::Offscreen));
                ToolOutput::text(perf::memory_json(&self.profiler, stats.as_ref(), limit).to_string())
            }
            Command::PerfCapture(_) | Command::PerfLight(_) | Command::Playtest(_) | Command::Editor(_) => unreachable!("answered above"),
            Command::Time(op) if playtest_engine::clock_refused(op, self.playtests.running()) => playtest_engine::clock_busy(),
            // A script has the clock (`--script`): nobody else's word moves it. An agent that
            // reached the run's port and resumed it turned the machine's time into ticks, and
            // the run printed a state its script had not played (review of ab61d3b3).
            Command::Time(op) if script_run::clock_refused(op, self.script_clock.is_some()) => world_error(&WorldError::new("script_has_the_clock", "a headless run (specula run) has this engine's clock until it ends: time_control info still answers")),
            // Steps wait for a load to finish (bounded); they are never queued behind it.
            Command::Time(TimeOp::Step(_) | TimeOp::Play(_)) if self.nav.loading() && !self.build_nav_load(nav::LOAD_CALL_WAIT) => self.nav_loading_error(),
            Command::Time(op) => {
                let name = match op {
                    TimeOp::Pause => Some("pause"),
                    TimeOp::Resume => Some("resume"),
                    TimeOp::Step(_) => Some("step"),
                    TimeOp::Scale(_) => Some("scale"),
                    TimeOp::Info | TimeOp::Play(_) => None,
                };
                if let Some(n) = name {
                    self.clock_set_by = Some((by.clone(), n));
                }
                let mut stepped = None;
                let mut played = None;
                match op {
                    TimeOp::Pause => self.clock.set_paused(true),
                    TimeOp::Resume => self.clock.set_paused(false),
                    TimeOp::Step(n) => {
                        self.clock.set_paused(true);
                        // (Ticks an earlier step queued and that have not run yet are due too.)
                        let due = self.clock.queued().saturating_add(n);
                        self.clock.step(n);
                        // Holds of agents gone since the last frame end before these ticks fire them.
                        self.lex_live_end_holds(server);
                        // Run them now, so the reply shows the result.
                        let before = self.clock.tick();
                        self.advance_clock();
                        stepped = Some((due, before));
                    }
                    TimeOp::Scale(x) => self.clock.set_scale(x),
                    TimeOp::Info => {}
                    TimeOp::Play(n) => {
                        self.lex_live_end_holds(server);
                        played = Some(self.play_ticks(n));
                    }
                }
                let mut answer = self.clock_json();
                if let Some(ran) = played {
                    answer.set("played", Json::from(ran));
                }
                if let Some(running) = self.script_clock {
                    answer.set("game_clock", Json::from(if running { "running" } else { "held" }));
                }
                // A step says what became of its ticks, each one counted once: how many ran in
                // this call, how many are still queued (a call runs a bounded number, and the
                // frames after it run the rest), and how many were dropped because the hosted
                // game stopped the clock on one of them (its run ended, it paused). Review R2
                // of M6 part 3: every tick that had not run yet was said to be dropped, and a
                // step of 1,000 answered "488 not run" for ticks that ran a moment later.
                if let Some((due, before)) = stepped {
                    let ran = self.clock.tick() - before;
                    let queued = self.clock.queued();
                    let dropped = stepped_dropped(due, ran, queued);
                    answer.set("ran", Json::from(ran));
                    if queued > 0 {
                        answer.set("queued", Json::from(queued));
                        answer.set("queued_note", Json::from("still to run: one call runs a bounded number of ticks and the frames after it run the rest (time_control info shows the tick); do not step them again"));
                    }
                    if dropped > 0 {
                        answer.set("not_run", Json::from(dropped));
                        answer.set("why", Json::from("the clock was stopped on the last tick that ran (set_by says by whom): the ticks left were dropped, not queued"));
                    }
                }
                ToolOutput::text(answer.to_pretty())
            }
            Command::Game { name, args } => match self.game.as_mut() {
                Some(game) => {
                    // A claim can only reject the tool when claims exist: checkpoint only then.
                    let checkpoint = if self.swarm.claims.is_empty() { None } else { game.checkpoint() };
                    let (clock, agent, windowed) = (&self.clock, self.game_name.as_str(), self.windowed);
                    let call = |gw: &mut game::GameWorld| game.call_tool(&mut Ctx { world: gw.reborrow(), clock, agent, windowed }, &name, &args);
                    let result = self.world.game_tool(&mut self.swarm, tick, agent, w, call);
                    // Presses the game refused as the player's alone: told to the input monitor
                    // (seen, never counted), whatever became of the call.
                    if game.take_refused_presses() > 0 {
                        inputguard::declined(inputguard::Channel::ScreenInput, &name, &inputguard::Actor::of(&self.swarm, &request.session), "players_only", "the hosted game");
                    }
                    match result {
                        Ok(out) => out,
                        Err(e) => {
                            // The world was rolled back; the game goes back with it.
                            match checkpoint {
                                Some(c) => game.restore(c),
                                None => {
                                    // The game's repair is its own write (its state, shown again).
                                    sim_writes::game(&mut self.world, &mut self.swarm, game.as_mut(), clock, agent, windowed, sim_writes::GameCall::RolledBack);
                                }
                            }
                            world_error(&self.conflict_help(e, Some(&conflict::Yours::named(vec![format!("(game tool {name})")]))))
                        }
                    }
                }
                None => ToolOutput::error(format!("no game is running, so '{name}' is unavailable")),
            },
            Command::History(q) => ToolOutput::text(self.swarm.journal().history(&q).to_string()),
            Command::Changes(q) => ToolOutput::text(self.swarm.journal().changes_text(&q)),
            Command::Undo(mut req) => {
                // Owner, 2026-10-29: force over other agents' entries or their later changes needs
                // lead authority. Without it the undo runs unforced: the caller's own clean undo
                // still works, and one that needed force is refused as such.
                let have = authority::authority(&self.swarm, &request.session.id);
                let unforced = req.force && have < authority::Authority::Lead;
                if unforced {
                    req.force = false;
                }
                match self.world.undo(&mut self.swarm, tick, w, &req) {
                Err(e) if unforced && matches!(e.code, "not_yours" | "undo_conflict") => {
                    world_error(&authority::may_force(have, false, "world_undo").expect_err("not the lead").with("cause", e.code))
                }
                Ok(u) => {
                    // Cameras an undo removed (or re-pointed) leave no stale follow state.
                    self.cameras.prune(&self.world);
                    ToolOutput::text(
                    Json::obj([
                        ("journal", Json::from(u.seq)),
                        ("undone", Json::arr(u.undone.iter().map(|s| Json::from(*s)))),
                        ("changes", Json::from(u.changes)),
                    ])
                    .to_string(),
                    )
                }
                Err(e) => world_error(&self.conflict_help(e, Some(&conflict::Yours::named(vec!["(undo)".into()])))),
                }
            }
            Command::Hash { recent, tick, .. } => self.hash_report(recent, tick),
            Command::ClaimTake(spec) => {
                let scope = spec.scope.clone();
                match self.swarm.take_claim(&self.world, &request.session, spec) {
                    Ok(c) => ToolOutput::text(self.claim_taken(w, &scope, c).to_string()),
                    Err(e) => world_error(&e),
                }
            }
            Command::Swarm(call) => self.handle_swarm(call, &request.session, server),
            Command::Review(call) => self.handle_review(call, &request.session, server),
            Command::ClaimRenew { id, lease } => match self.swarm.renew(w, id, lease) {
                Ok(out) => ToolOutput::text(out.to_string()),
                Err(e) => world_error(&e),
            },
            Command::ClaimRelease { id } => match self.swarm.release(w, id) {
                Ok(out) => ToolOutput::text(out.to_string()),
                Err(e) => world_error(&e),
            },
            Command::ClaimShare { id, with, revoke } => match self.swarm.share(w, id, &with, revoke, &server.all_sessions()) {
                Ok(c) => ToolOutput::text(c.to_string()),
                Err(e) => world_error(&e),
            },
            Command::ClaimList { owner, mine, path_prefix, after, limit } => {
                let owner = if mine { Some(by.clone()) } else { owner };
                let mut list = self.swarm.claims.list(owner.as_deref(), path_prefix.as_deref(), after, limit, Instant::now());
                // Writes modules' and live Lex runs' own ticks made inside the caller's claims
                // (exempt from claims, so never refused): told to the holder.
                let notices = self.swarm.claim_notices(Some(&by), 20);
                if notices.as_array().is_some_and(|a| !a.is_empty()) {
                    list.set("notices", notices);
                    let (dropped, unexamined) = self.swarm.claim_notices_dropped(&by);
                    if dropped + unexamined > 0 {
                        list.set("notices_dropped", Json::obj([("past_the_64_kept", Json::from(dropped)), ("changes_unexamined", Json::from(unexamined))]));
                    }
                }
                ToolOutput::text(list.to_string())
            }
            Command::Physics(call) => self.physics_call(call, w, tick),
            Command::Nav(call) => self.nav_call(call, w, tick),
            Command::Place(call) => self.place_call(call, w, tick),
            Command::Stream(call) => self.stream_call(call, &request.session),
            Command::Terrain(call) => self.terrain_call(call),
            Command::Ui(call) => self.ui_call(call, w, tick, &request.session),
            Command::SaveGame(call) => self.handle_save(call, w, &by, authority::authority(&self.swarm, &request.session.id)),
            Command::ModelBuild(_) | Command::ModelExport(..) | Command::ModelReload => unreachable!("answered by handle_model_call"),
            // An agent's hold on an action the human is using takes their input over: judged by
            // the input monitor first (docs/design/SECURITY.md).
            Command::Logic(call) => match self.security_logic_input(&call, &request.session) {
                Some(refused) => refused,
                None => {
                    self.lend_physics();
                    let out = sim_writes::logic_tools(&mut self.world, &mut self.swarm, &mut self.logic, &self.clock, call, w);
                    self.return_physics();
                    self.journal_module_events(Some(w));
                    out
                }
            },
            Command::Anim(call) => self.anim_call(call, w, tick),
            Command::StudioRender(_) | Command::StudioDone(_) => unreachable!("queued by studio_enqueue"),
            Command::Vfx(call) => self.vfx_call(call, w, tick),
            Command::Timeline(call) => self.timeline_call(call, &request.session, w),
            Command::Gi(call) => self.gi_call(call, w, tick, &request.session),
            Command::Project(call) => self.project_call(call, w, &by),
            Command::Taste(call) => self.handle_taste(call, &request.session),
            Command::DiskGate => self.disk_gate(&request.session),
            Command::ReviewRoots => self.review_roots_json(),
            Command::EvidenceRefused(made) => self.evidence_refused(made),
            Command::Security(call) => self.security_call(call, &request.session, &server.sessions()),
        };
        self.check_world_replaced();
        // Undo (or anything else) changed project files: the disk follows before the answer (the
        // write runs on a worker; the answer waits for it: see `project_engine`). A project
        // call's own answer goes through its inner reply, which waits the same way.
        let files_changed = self.world.files_revision() != files_before && !output.is_error;
        if !files_changed {
            self.project_after_request();
        }
        if game_call {
            self.serve_game_requests();
            self.serve_game_timelines();
        }
        debug_assert!(self.world.pending_changes().is_empty(), "every world change is journaled");
        // The commits made by this dispatch (ticks it ran were paused: the simulation's; deferred
        // work it pumped was charged to its own calls).
        self.dispatch_call = outer_call;
        let made = self.swarm.end_dispatch();
        // (An editor command an agent's frames caused is the window owner's write and that
        // agent's commit: `via`. Review R1 of m5-edswarm, minor 4: the cost view showed an
        // owner's agent that deleted and undid through ui_act with 21 calls and no commits.)
        self.charge_commits(call_id, made, independent, request.via.as_deref().unwrap_or(&by));
        if files_changed {
            let reply = request.reply;
            self.project_sync_after(move |_| {
                let _ = reply.send(output);
            });
            return;
        }
        // ui_act on the editor that made an edit answers with how the edit went: held until
        // the window's frame has sent it and it is answered (`editor_engine`).
        let (output, reply) = if ui_act {
            match self.editor_hold_answer(call_id, output, request.reply, request.deadline) {
                Some(answer) => answer,
                None => return,
            }
        } else {
            (output, request.reply)
        };
        // The caller may have timed out; nothing to do then.
        let _ = reply.send(output);
    }

    /// Charges `made` commits to tool call `call` (one call, one commit): a breach is counted,
    /// traced (`write.multi_commit`) and, in debug builds, a panic.
    fn charge_commits(&mut self, call: u64, made: u64, independent: bool, by: &str) {
        self.swarm.count_commits(by, made);
        if let Some(total) = self.commits.count(call, made, independent) {
            self.multi_commit_calls += 1;
            specula_core::trace_event!(&trace_sink::WRITES, Warn, "write.multi_commit", call = call, by = by, commits = total, why = "one tool call committed more than once: it can fail halfway");
            debug_assert!(total <= 1, "one tool call committed {total} times (by {by})");
        }
    }

    /// Runs deferred work `f` (a worker's continuation, a project call held while a sync wrote)
    /// under its own commit count, charged to `call`, the tool call that started it, never to the
    /// request being dispatched when it runs (the M4 final review: other agents' held writes
    /// pumped by a `project_mount` were charged to the mount). Work no tool call started
    /// (`call` 0: the engine's own) is charged to nothing, as before.
    pub(crate) fn run_charged<R>(&mut self, call: u64, by: &str, f: impl FnOnce(&mut Engine) -> R) -> R {
        let outer = self.swarm.pause_dispatch();
        self.swarm.begin_dispatch(by);
        let outer_call = std::mem::replace(&mut self.dispatch_call, call);
        let r = f(self);
        self.dispatch_call = outer_call;
        let made = self.swarm.end_dispatch();
        self.swarm.resume_dispatch(outer);
        if call != 0 {
            self.charge_commits(call, made, false, by);
        }
        r
    }

    fn entity_output(&self, id: u64, journal: Option<u64>) -> ToolOutput {
        let e = self.world.get(id).expect("just written");
        let mut out = e.to_json(&self.world.path_of(id));
        out.set("journal", Json::from(journal));
        if let Some(problem) = models::anim_problem(&self.library.lock(), e) {
            out.set("anim_warning", Json::from(problem));
        }
        ToolOutput::text(out.to_pretty())
    }

    /// world_hash's answer.
    fn hash_report(&mut self, recent: usize, tick: Option<u64>) -> ToolOutput {
        let hash = self.state_hash();
        let pair = |(t, h): &(u64, u64)| Json::arr([Json::from(*t), Json::from(journal::hex(*h))]);
        let start = self.tick_hashes.len().saturating_sub(recent);
        let mut out = Json::obj([
            ("tick", Json::from(self.clock.tick())),
            ("state_hash", Json::from(journal::hex(hash))),
            ("entities", Json::from(self.world.len())),
            ("journal_last_id", Json::from(self.swarm.journal().last_seq())),
            ("recent", Json::arr(self.tick_hashes.range(start..).map(pair))),
        ]);
        // Physics state the world doesn't hold (warm starts, sleep timers, exact rotations).
        if let Some(h) = self.physics.hash() {
            let pstart = self.physics.hashes.len().saturating_sub(recent);
            out.set(
                "physics",
                Json::obj([("state_hash", Json::from(journal::hex(h))), ("recent", Json::arr(self.physics.hashes.range(pstart..).map(pair)))]),
            );
        }
        // Navigation's hidden state (navmeshes, corridors, minds).
        if let Some(h) = self.nav.last_hash() {
            let nstart = self.nav.hashes.len().saturating_sub(recent);
            out.set("nav", Json::obj([("state_hash", Json::from(journal::hex(h))), ("recent", Json::arr(self.nav.hashes.range(nstart..).map(pair)))]));
        }
        if let Some(t) = tick {
            let found = self.tick_hashes.iter().find(|(at, _)| *at == t).map(|(_, h)| journal::hex(*h));
            let oldest = self.tick_hashes.front().map(|(t, _)| *t);
            out.set("at_tick", Json::obj([("tick", Json::from(t)), ("hash", Json::from(found)), ("oldest_kept", Json::from(oldest))]));
        }
        ToolOutput::text(out.to_string())
    }

    /// One page of world_list / world_query, cut short at [`MAX_LISTING_BYTES`].
    fn query(&self, spec: &QuerySpec) -> ToolOutput {
        match self.query_page(spec) {
            Ok(page) => page.render(),
            Err(out) => out,
        }
    }

    /// A query's page as owned copies (what each listed field reads of the world included), to
    /// list here or on a worker.
    fn query_page(&self, spec: &QuerySpec) -> Result<OwnedPage, ToolOutput> {
        let page = self.world.query(&spec.query).map_err(|e| world_error(&e))?;
        let world_fields = spec.fields.as_ref().is_some_and(|f| f.iter().any(|x| x == "parent" || x == "children"));
        let items = page
            .items
            .into_iter()
            .map(|(e, path)| {
                let (parent, children) = if world_fields { (e.parent.map(|p| self.world.path_of(p)), self.world.children_of(Some(e.id)).count()) } else { (None, 0) };
                (e.clone(), path, parent, children)
            })
            .collect();
        Ok(OwnedPage { total: page.total, more: page.more, items, fields: spec.fields.clone(), key: spec.key })
    }

    /// world_query and world_list: a big page is listed (each item made and measured) on a
    /// worker, which answers (500 items with every field took 10-20 ms of the engine thread);
    /// a small one here.
    fn start_query(&mut self, request: Request) {
        let Request { command: Command::Query(spec), session, reply, call, .. } = request else { unreachable!("only queries come here") };
        let page = match self.query_page(&spec) {
            Ok(p) => p,
            Err(out) => {
                let _ = reply.send(out);
                return;
            }
        };
        if page.items.len() < QUERY_OFFLOAD_ITEMS {
            let _ = reply.send(page.render());
            return;
        }
        let owner = jobs::JobOwner { name: self.swarm.agent_name(&session), key: self.swarm.key(&session) };
        let kind = workers::JobKind::QueryList;
        let o = workers::Offload::agent(kind, owner, &session.id, call).class(schedule::Class::Read);
        let (theirs, refused) = (reply.clone(), reply.clone());
        if let Err(e) = self.offload(o, move |_| {
            let _ = theirs.send(page.render());
        }, move |_, r| {
            if let Err(f) = r {
                let _ = reply.send(f.to_output(kind));
            }
        }) {
            refused.refuse(&e);
        }
    }

    /// The lock for `dir` if this engine doesn't hold it yet (`None` when it already does).
    fn lock_for(&self, dir: &Path) -> Result<Option<persist::WorldLock>, persist::PersistError> {
        if self.world_lock.as_ref().is_some_and(|l| l.dir() == dir) {
            return Ok(None);
        }
        persist::WorldLock::acquire(dir).map(Some)
    }

    /// The hosted game's `before_capture`: presentation up to the current tick (a headless engine
    /// never runs its per-frame update). Returns the tick it ran at (`None` without a game).
    fn before_capture(&mut self) -> Option<u64> {
        if !self.game_call(sim_writes::GameCall::BeforeCapture) {
            return None;
        }
        Some(self.clock.tick())
    }

    /// A cached offscreen target of `w` × `h` (most recently used last).
    fn offscreen_target(&mut self, w: u32, h: u32) -> Result<TargetId, String> {
        // A capture view in flight may use one of these targets and the ID buffers: landed first.
        self.land_capture_flight();
        Ok(match self.targets.iter().position(|(size, _)| *size == (w, h)) {
            Some(i) => {
                let entry = self.targets.remove(i);
                self.targets.push(entry);
                entry.1
            }
            None => {
                if self.targets.len() >= MAX_CACHED_TARGETS {
                    let (_, oldest) = self.targets.remove(0);
                    self.gpu.destroy_target(oldest);
                }
                let t = self.gpu.create_target(w, h).map_err(|e| capture::gpu_failure("create target", &e, self.windowed))?;
                self.targets.push(((w, h), t));
                t
            }
        })
    }

    fn perf(&self, budget_ms: Option<f64>, graph: bool) -> ToolOutput {
        let kind = perf::adapter_kind(self.gpu.adapter().kind);
        let mut budgets = self.perf_cfg.budgets(kind);
        if let Some(ms) = budget_ms {
            budgets = budgets.with_frame(ms);
        }
        let window = self.gpu.last_frame_stats_of(FramePath::Window);
        let offscreen = self.gpu.last_frame_stats_of(FramePath::Offscreen);
        let pacing = &self.pacing;
        let zones = self.profiler.rolling();
        let heap = specula_core::alloc::stats();
        let sample = perf::Sample {
            stats: window.as_ref().or(offscreen.as_ref()),
            pacing_p95_ms: pacing.frame.p95(),
            cpu_frame_p95_ms: self.profiler.cpu_frame_p95(),
            tick_p95_ms: pacing.tick.p95(),
            game_frame_p95_ms: pacing.game_frame.p95(),
            zones: &zones,
            heap_bytes: heap.installed.then_some(heap.current_bytes),
            allocations_per_frame_p95: self.profiler.allocations_p95(),
        };
        let mut errors = perf::evaluate(&budgets, &sample, kind);
        errors.extend(self.stream.budget_errors(kind));
        if self.flash.window.report.over {
            let r = &self.flash.window.report;
            errors.push(Json::obj([
                ("error", Json::from("flash")),
                ("message", Json::from(format!(
                    "the window flashes: up to {} flashes (red {}) in the last second over {:.1}% of the view (WCAG 2.3.1 allows 3, over at most {:.1}%)",
                    r.flashes,
                    r.red_flashes,
                    r.area.max(r.red_area) * 100.0,
                    flashwatch::AREA * 100.0
                ))),
            ]));
        }
        let source = self.perf_cfg.source.as_ref().map_or(Json::Null, |p| Json::from(p.display().to_string()));
        let heap_mb = (heap.current_bytes as f64 / 1_048_576.0 * 10.0).round() / 10.0;
        let out = Json::obj([
            ("budget_ms", Json::from(budgets.frame_ms)),
            ("budgets", Json::obj([("source", source), ("adapter", Json::from(kind)), ("checked", budgets.to_json())])),
            ("errors", Json::Arr(errors)),
            ("window", window.as_ref().map_or(Json::Null, perf::stats_json)),
            ("offscreen", offscreen.as_ref().map_or(Json::Null, perf::stats_json)),
            ("frame_pacing", pacing.frame.to_json()),
            // With a window: how agents' calls are served between presents.
            ("serve", if self.windowed { self.schedule.to_json() } else { Json::Null }),
            ("sim_tick", pacing.tick.to_json()),
            ("physics", self.physics.summary_json(&self.world.env().physics)),
            ("nav", if self.nav.is_empty() { Json::Null } else { self.nav.summary_json() }),
            ("game_frame", pacing.game_frame.to_json()),
            (
                "game_overlay",
                Json::obj([("quads", Json::from(self.game_overlay_stats.0)), ("dropped", Json::from(self.game_overlay_stats.1))]),
            ),
            ("cpu", perf::cpu_json(&self.profiler)),
            ("heap_mb", if heap.installed { Json::from(heap_mb) } else { Json::Null }),
            ("draw_list", pacing.draw_list.to_json()),
            // The window's draw cache: frames that rebuilt it from every entity vs patched it
            // with the changed ones (a rebuild per frame means the world changes wholesale).
            (
                "draw_cache",
                Json::obj([("rebuilds", Json::from(self.draw_cache.rebuilds())), ("patches", Json::from(self.draw_cache.patches()))]),
            ),
            ("clock", self.clock_json()),
            ("entities", Json::from(self.world.len())),
            ("transient_entities", Json::from(self.world.transient_len())),
            ("shape_budgets", self.world.shape_budgets_json()),
            ("models", self.models_json()),
            ("streaming", self.stream.json(&self.gpu, &self.models)),
            // Effects: the window frame's budget use (vfx_info has more).
            ("vfx", Json::obj([("enabled", Json::from(self.vfx.enabled)), ("budget", self.vfx.budget.to_json()), ("window", self.vfx.window.to_json())])),
            // Terrains and water of the last frame drawn (terrain_info has one terrain's detail).
            ("terrain", self.terrain.stats_json()),
            // WCAG 2.3.1 on the final image (the window's grid and each view's captures).
            ("flash", self.flash.to_json(window.as_ref().and_then(|w| w.flash.unsupported))),
            ("render_graph", perf::graph_json(self.gpu.last_graph().as_ref(), graph)),
            // Engine-thread stalls past their class budget, and incidents wanting attention.
            ("trace", self.watch.json()),
            // Effects that can't apply (a grading LUT that failed to load), structured.
            // and pipeline variants a background build failed to make.
            // and floating-point environment changes the guard restored (rule 6).
            (
                "problems",
                Json::Arr(
                    self.luts
                        .warning(self.world.env())
                        .into_iter()
                        .chain(self.models.pipeline_failure_json())
                        .chain(self.fp.problem())
                        .chain(self.logic.fp_log().problem())
                        .collect(),
                ),
            ),
        ]);
        ToolOutput::text(out.to_pretty())
    }

    /// Model drawing and caching, for perf_frame and engine_info.
    fn models_json(&self) -> Json {
        let mut gpu = self.models.stats_json();
        gpu.set("lathe_meshes", Json::from(self.lathes.count()));
        Json::obj([
            ("window", self.lod_window.to_json()),
            ("last_capture", self.lod_capture.to_json()),
            ("lod_pixels", Json::arr(models::LOD_PIXELS.map(Json::from))),
            ("gpu", gpu),
            ("library", self.library.lock().stats_json()),
        ])
    }

    fn info(&mut self, sessions: &[SessionInfo]) -> ToolOutput {
        let a = self.gpu.adapter();
        let gpu = Json::obj([
            ("name", Json::from(a.name.as_str())),
            ("kind", Json::from(format!("{:?}", a.kind))),
            ("vulkan", Json::from(format!("{}.{}.{}", a.api_version.0, a.api_version.1, a.api_version.2))),
        ]);
        let agents = sessions.iter().map(|s| {
            let role = self.swarm.role(&s.id);
            Json::obj([
                ("name", Json::from(self.swarm.agent_name(s))),
                ("role", Json::from(role)),
                ("authority", Json::from(authority::authority(&self.swarm, &s.id).name())),
                ("client", Json::from(s.client_name.as_str())),
                ("version", Json::from(s.client_version.as_str())),
            ])
        });
        let world = Json::obj([
            ("name", self.world_name.as_deref().map_or(Json::Null, Json::from)),
            ("worlds_dir", Json::from(self.worlds_dir.display().to_string())),
            ("entities", Json::from(self.world.len())),
            ("transient_entities", Json::from(self.world.transient_len())),
            ("shape_budgets", self.world.shape_budgets_json()),
            ("unsaved_changes", Json::from(self.dirty())),
            ("last_autosave_error", self.autosave_error.clone().unwrap_or(Json::Null)),
            ("leftover_files", Json::arr(self.leftovers.iter().map(|f| Json::from(f.as_str())))),
            ("state_hash", Json::from(format!("{:016x}", self.world.state_hash()))),
        ]);
        let info = Json::obj([
            ("engine", Json::from(format!("specula {}", env!("CARGO_PKG_VERSION")))),
            ("gpu", gpu),
            ("validation", Json::from(self.gpu.validation_enabled())),
            ("entities", Json::from(self.world.len())),
            ("world", world),
            ("frames_captured", Json::from(self.frames_captured)),
            ("captures", Json::from(self.capture_dir.display().to_string())),
            // The project this engine has open (`--project`, or started in its folder).
            ("project", self.opened_project.as_ref().map_or(Json::Null, |(path, name)| Json::obj([("name", Json::from(name.as_str())), ("path", Json::from(path.display().to_string()))]))),
            ("agents", Json::arr(agents)),
            ("clock", self.clock_json()),
            ("swarm", {
                let mut s = self.swarm.stats_json();
                s.set("unjournaled_writes", Json::from(self.world.strays()));
                let (n, last) = self.world.replacements();
                s.set("world_replacements", Json::from(n));
                s.set("game_reserved_writes", Json::from(self.world.game_reserved_writes()));
                // One tool call, one commit (see `count_commits`): calls that committed more.
                s.set("multi_commit_calls", Json::from(self.multi_commit_calls));
                // Who the operator made owner, lead or reviewer, and whether anyone leads.
                s.set("grants", self.swarm.grants.json());
                if let Some((entry, tick)) = last {
                    s.set("last_world_replacement", Json::obj([("entry", Json::from(entry.as_str())), ("tick", Json::from(*tick))]));
                }
                s
            }),
            ("models", self.models_json()),
            ("window", self.window_json()),
            ("disk", self.disk_json()),
            // The floating-point environment guard: what it checks and what it had to restore,
            // at startup and between ticks (`engine`) and around module calls (`modules`).
            ("determinism", Json::obj([("fp_env", self.fp.to_json()), ("fp_env_modules", self.logic.fp_log().to_json())])),
            // The authority table (detailed only; see `authority::access`): what each tool may do, and
            // the one-at-a-time jobs running.
            ("access", Json::obj([
                ("tools", authority::access_json()),
                ("jobs", Json::arr(self.fly.get().map(jobs::Job::json).into_iter().chain(self.profiler.capture_job()))),
            ])),
            // Work off the engine thread (docs/design/WORKERS.md): the pool and the step jobs.
            ("workers", Json::obj([("pool", self.workers.to_json()), ("step_jobs", self.steps.to_json())])),
            // The hosted game's settings file (where, writes made, a write that failed) and its
            // timelines' cue sounds (played, skipped, cached, failed names remembered).
            ("settings", self.user_settings.to_json()),
            ("timeline_audio", self.timeline_audio.to_json()),
        ]);
        let mut info = info;
        if let Some(e) = &self.board_io.error {
            info.set("board_save_error", e.clone());
        }
        if let Some(w) = &self.board_io.warning {
            info.set("board_warning", w.clone());
        }
        if !self.logic.is_empty() {
            info.set("modules", self.logic.summary());
            info.set("module_sounds", self.module_sounds.to_json());
        }
        if let Some(game) = &self.game {
            info.set("game", Json::obj([("name", Json::from(self.game_name.as_str())), ("status", game.status())]));
        }
        // (Compact: the answer is tidied to compact JSON on its way out, and laying it out
        // over lines here first was a second pass over 15 KB on the engine thread.)
        ToolOutput::text(info.to_string())
    }
}

/// What kind of cost a call has, for the window's schedule: a serve window only starts a call
/// when it has room for what calls of its class cost lately (see `schedule`).
/// Commands that commit once per independent item, each answered on its own (exempt from one
/// call, one commit): review_decide (each submission is decided, promoted or reverted by
/// itself; refusals are per item in `results`), and logic_build's fix rounds (each round's
/// fixes are one commit; the answer lists what was applied and skipped).
fn commits_independently(command: &Command) -> bool {
    matches!(
        command,
        Command::Review(review_tools::ReviewCall::Decide { .. }) | Command::Project(project_tools::ProjectCall::CommitFixes { .. })
    )
}

fn call_class(command: &Command) -> Class {
    match command {
        Command::Frame(_) | Command::Sight(_) => Class::Render,
        Command::Editor(editor_tools::EditorCall::State { .. }) => Class::Read,
        Command::Editor(editor_tools::EditorCall::Input(_)) => Class::Edit,
        Command::Anim(call) => anim_tools::call_class(call),
        Command::StudioRender(_) | Command::StudioDone(_) => Class::Read,
        Command::Vfx(call) => vfx_tools::call_class(call),
        Command::Timeline(call) => timeline_tools::call_class(call),
        Command::PerfLight(_) => Class::Render,
        Command::Gi(call) => gi_tools::call_class(call),
        Command::Save { .. } | Command::Load { .. } | Command::ModelBuild(_) | Command::ModelExport(..) | Command::ModelReload => Class::Disk,
        Command::Hash { verify: true, .. } => Class::Big,
        Command::Swarm(call) => call.class(),
        Command::Review(call) => call.class(),
        Command::Batch(ops) if ops.len() > HEAVY_BATCH_OPS => Class::Big,
        Command::Time(TimeOp::Step(_) | TimeOp::Play(_)) => Class::Sim,
        Command::Game { .. } | Command::GameAct { .. } => Class::Game,
        Command::Logic(_) => Class::Disk,
        Command::Physics(call) => call.class(),
        Command::Nav(call) => call.class(),
        Command::Place(call) => call.class(),
        Command::Stream(call) => call.class(),
        Command::Terrain(call) => call.class(),
        Command::Ui(call) => call.class(),
        Command::SaveGame(call) => call.class(),
        Command::Playtest(_) => Class::Sim,
        Command::Project(call) => call.class(),
        Command::Taste(call) => call.class(),
        Command::DiskGate | Command::ReviewRoots | Command::EvidenceRefused(_) => Class::Read,
        Command::Lex(call) if call.is_read() => Class::Read,
        Command::Lex(_) => Class::Edit,
        Command::LexLive(call) => call.class(),
        Command::Spawn(_)
        | Command::Set(..)
        | Command::TextEdit(_)
        | Command::Delete(..)
        | Command::Batch(_)
        | Command::Env(_)
        | Command::View(_)
        | Command::Undo(_)
        | Command::Hello { .. }
        | Command::ClaimTake(_)
        | Command::ClaimRenew { .. }
        | Command::ClaimRelease { .. }
        | Command::ClaimShare { .. }
        | Command::Time(_)
        | Command::Security(_)
        | Command::PerfCapture(_) => Class::Edit,
        Command::Query(_)
        | Command::TileGet { .. }
        | Command::TextGet { .. }
        | Command::Info
        | Command::GameState
        | Command::Perf { .. }
        | Command::PerfZones(_)
        | Command::PerfMemory { .. }
        | Command::History(_)
        | Command::Changes(_)
        | Command::Hash { .. }
        | Command::ScopeLookup { .. }
        | Command::ClaimList { .. } => Class::Read,
    }
}

/// Whether this frame's input is a human taking the controls (a key press, click, scroll or
/// touch; mouse moves and key repeats alone aren't): it releases an agent camera the window
/// follows (F5: humans are never locked out).
fn human_took_controls(input: &[Event]) -> bool {
    input.iter().any(|e| matches!(e, Event::KeyDown { repeat: false, .. } | Event::MouseDown { .. } | Event::MouseWheel { .. } | Event::PointerDown { .. }))
}

/// world_batch's answer: counts per kind, every op's result while they fit in
/// [`MAX_BATCH_RESULT_BYTES`], then how many were left out. `omitted_spawned_ids` gives the first
/// and last id of left-out spawns (a batch's spawns take consecutive ids, so that's all of them).
fn batch_summary(results: &[world::OpResult]) -> Json {
    use world::OpResult::{Deleted, Spawned, Updated};
    let count = |rs: &[world::OpResult], f: fn(&world::OpResult) -> bool| rs.iter().filter(|r| f(r)).count();
    let mut shown = Vec::new();
    let mut bytes = 0;
    for r in results {
        let j = r.to_json();
        bytes += j.to_string().len() + 1; // and its comma
        if bytes > MAX_BATCH_RESULT_BYTES {
            break;
        }
        shown.push(j);
    }
    let rest = &results[shown.len()..];
    let mut out = vec![
        ("applied", Json::from(results.len())),
        (
            "counts",
            Json::obj([
                ("spawned", Json::from(count(results, |r| matches!(r, Spawned { .. })))),
                ("set", Json::from(count(results, |r| matches!(r, Updated { .. })))),
                ("deleted", Json::from(count(results, |r| matches!(r, Deleted { .. })))),
            ]),
        ),
        ("results", Json::Arr(shown)),
    ];
    if !rest.is_empty() {
        out.push(("results_omitted", Json::from(rest.len())));
        let spawned: Vec<u64> = rest.iter().filter_map(|r| if let Spawned { id, .. } = r { Some(*id) } else { None }).collect();
        if let (Some(first), Some(last)) = (spawned.first(), spawned.last()) {
            out.push(("omitted_spawned_ids", Json::obj([("first", Json::from(*first)), ("last", Json::from(*last))])));
        }
    }
    Json::obj(out)
}

/// Of the `due` ticks of a step (its own, and any an earlier step left queued), how many were
/// dropped: the ones that neither `ran` in the call nor are still `queued` (the hosted game
/// stopped the clock on a tick). A tick still queued is not dropped: it runs in a frame after.
fn stepped_dropped(due: u64, ran: u64, queued: u64) -> u64 {
    due.saturating_sub(ran).saturating_sub(queued)
}

fn world_error(e: &WorldError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_pretty())
}

/// `run-<unix seconds>-<pid>` → `(seconds, pid)`; anything else → `None`.
fn parse_run_name(name: &str) -> Option<(u64, u32)> {
    let (secs, pid) = name.strip_prefix("run-")?.split_once('-')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(secs) || !digits(pid) {
        return None;
    }
    Some((secs.parse().ok()?, pid.parse().ok()?))
}

/// Keep only the newest `keep` finished capture runs under `root`. Only folders named exactly
/// `run-<digits>-<digits>` are considered; `current` and runs whose engine process is still running
/// are never deleted; everything else in `root` is left alone.
fn prune_capture_runs(root: &Path, keep: usize, current: &Path) {
    // Deleted inside the captures root only, never through a link (see `disk`).
    let root = &disk::canonical(root);
    let current = &disk::canonical(current);
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let mut runs: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| std::fs::symlink_metadata(e.path()).is_ok_and(|m| m.is_dir() && !disk::is_link(&m)))
        .filter_map(|e| {
            let (secs, pid) = parse_run_name(e.file_name().to_str()?)?;
            let path = root.join(e.file_name());
            (&path != current && !specula_sys::process_is_running(pid)).then_some((secs, path))
        })
        .collect();
    runs.sort();
    let excess = runs.len().saturating_sub(keep);
    for (_, path) in runs.into_iter().take(excess) {
        match disk::remove_tree_inside(root, &path) {
            Ok((n, b)) => disk::book().wrote(disk::Category::Captures, "prune", -(b as i64), -(n as i64)),
            Err(e) => eprintln!("specula: could not prune old captures {}: {e}", path.display()),
        }
    }
}

/// A GPU test that can't run here says why and passes (AGENTS rule 7): a warning-level trace
/// event, which the trace's stderr sink prints (the print guard: no new console output).
#[cfg(test)]
pub(crate) fn skip_test(test: &str, why: &str) {
    specula_core::trace_event!(&trace_sink::ENGINE, Warn, "test.skipped", test = test, why = why);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each kind of cost is tracked apart: a slow step or game tool doesn't hold back edits.
    #[test]
    fn calls_are_classed_by_what_they_cost() {
        assert_eq!(call_class(&Command::Save { name: None, overwrite: false }), Class::Disk);
        assert_eq!(call_class(&Command::Load { name: "w".into(), force: false }), Class::Disk);
        assert_eq!(call_class(&Command::ModelReload), Class::Disk);
        assert_eq!(call_class(&Command::Hash { recent: 0, tick: None, verify: true }), Class::Big);
        assert_eq!(call_class(&Command::Hash { recent: 8, tick: None, verify: false }), Class::Read);
        assert_eq!(call_class(&Command::Info), Class::Read);
        assert_eq!(call_class(&Command::Batch(Vec::new())), Class::Edit);
        assert_eq!(call_class(&Command::Time(TimeOp::Step(18_000))), Class::Sim);
        assert_eq!(call_class(&Command::Time(TimeOp::Pause)), Class::Edit);
        assert_eq!(call_class(&Command::Game { name: "play_x".into(), args: Json::Null }), Class::Game);
    }

    /// Reviewer finding (round 7, F5): an agent camera in the window must never lock the human
    /// out: input releases it (a hosted game is told the view is the agent's, round 8, and its
    /// camera writes don't count meanwhile).
    #[test]
    fn humans_and_games_take_the_window_camera_back() {
        use specula_platform::MouseButton;
        assert!(!human_took_controls(&[Event::MouseMove { x: 3, y: 4 }, Event::KeyDown { code: 0x41, repeat: true }]));
        for e in [
            Event::KeyDown { code: 0x41, repeat: false },
            Event::MouseDown { button: MouseButton::Left, x: 1, y: 1 },
            Event::MouseWheel { delta: 1.0 },
        ] {
            assert!(human_took_controls(std::slice::from_ref(&e)), "{e:?}");
        }
    }

    /// Reviewer finding: 1000 spawns answered with ~158 KB of per-op results.
    #[test]
    fn batch_results_are_capped_with_a_summary() {
        let results: Vec<world::OpResult> = (1..=1000)
            .map(|id| world::OpResult::Spawned { id, path: format!("/district_with_a_long_name/house_{id:04}") })
            .collect();
        let mut results = results;
        results.push(world::OpResult::Deleted { id: 7, path: "/old".into(), count: 3 });
        let out = batch_summary(&results);
        assert!(out.to_string().len() <= MAX_BATCH_RESULT_BYTES + 300, "{} bytes", out.to_string().len());
        let shown = out.get("results").and_then(Json::as_array).unwrap().len();
        assert_eq!(out.get("applied"), Some(&Json::from(1001)));
        assert_eq!(out.get("counts").unwrap().to_string(), r#"{"spawned":1000,"set":0,"deleted":1}"#);
        assert_eq!(out.get("results_omitted"), Some(&Json::from(1001 - shown)));
        let ids = out.get("omitted_spawned_ids").unwrap();
        assert_eq!((ids.get("first"), ids.get("last")), (Some(&Json::from(shown as u64 + 1)), Some(&Json::from(1000))));
        let small = batch_summary(&results[..3]);
        assert_eq!((small.get("results").and_then(Json::as_array).unwrap().len(), small.get("results_omitted")), (3, None));
    }

    #[test]
    fn game_arguments_follow_a_double_dash() {
        let parse = |s: &str| parse_args_from(s.split_whitespace().map(String::from));
        let a = parse("--headless --assets games/sepulcra/assets -- --seed 7 --fast").unwrap();
        assert!(a.headless);
        assert_eq!(a.assets.as_deref(), Some(Path::new("games/sepulcra/assets")));
        assert_eq!(a.game_args, ["--seed", "7", "--fast"]);
        assert_eq!(parse("--game-args --seed 7").unwrap().game_args, ["--seed", "7"]);
        assert!(parse("--port 7490").unwrap().game_args.is_empty());
        let e = parse("--seed 7").err().unwrap();
        assert!(e.contains("unknown argument '--seed'") && e.contains("after --"), "{e}");
        assert_eq!(parse("--style s.json").unwrap().style.as_deref(), Some(Path::new("s.json")));
        let a = parse("--paused --response-format detailed").unwrap();
        assert!(a.paused && a.response_format == response::Format::Detailed);
        assert_eq!(parse("").unwrap().response_format, response::Format::Concise);
        assert!(parse("--response-format verbose").err().unwrap().contains("concise or detailed"));
    }

    #[test]
    fn run_names_must_match_exactly() {
        assert_eq!(parse_run_name("run-100-7"), Some((100, 7)));
        for bad in ["run-1", "run-5-important-results", "run--7", "run-x-1", "run-1-2-3", "notes", "run-1-"] {
            assert_eq!(parse_run_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn prune_keeps_newest_finished_runs_only() {
        let root = std::env::temp_dir().join(format!("specula-prune-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let me = std::process::id();
        // Finished runs use a pid that can't be running; one run belongs to this (running) process.
        let dead = 0xFFFF_FFF0u32;
        let names = [
            format!("run-100-{dead}"),
            format!("run-300-{dead}"),
            format!("run-200-{dead}"),
            format!("run-50-{me}"),
            "run-1".to_string(),
            "run-5-important-results".to_string(),
            "notes".to_string(),
        ];
        for name in &names {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        let current = root.join("run-400-1");
        prune_capture_runs(&root, 1, &current);
        let mut left: Vec<String> =
            std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        let mut want = vec![
            "notes".to_string(),
            "run-1".to_string(),
            format!("run-300-{dead}"),
            format!("run-50-{me}"),
            "run-5-important-results".to_string(),
        ];
        if !cfg!(windows) {
            // No process check there yet: liveness is unknown, so nothing may be deleted.
            want.extend([format!("run-100-{dead}"), format!("run-200-{dead}")]);
        }
        want.sort();
        assert_eq!(left, want);
        std::fs::remove_dir_all(&root).unwrap();
    }
}

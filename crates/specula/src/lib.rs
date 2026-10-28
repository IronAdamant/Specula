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

mod balance_tools;
pub mod board;
pub mod anim;
mod anim_tools;
pub mod cameras;
mod capture;
pub mod claims;
pub mod compare;
pub mod conflict;
pub mod desync;
mod director;
pub mod effects;
pub mod fpguard;
pub mod game;
pub mod hlod;
pub mod intents;
pub mod journal;
pub mod leases;
pub mod logic;
mod lex_live;
/// Live Lex runs' work limits (perf.toml `[lex_live]`), for games that size their sets to them.
pub use lex_live::{Limits as LexLiveLimits, LIMITS as LEX_LIVE_LIMITS};
mod lex_tools;
mod luts;
pub mod merge;
pub mod meshes;
pub mod models;
pub mod orbit;
mod perf;
pub mod persist;
pub mod rollup;
mod save_tools;
mod safe_path;
pub mod savegame;
pub mod physics;
mod physics_tools;
mod joint_tools;
mod ragdoll_tools;
pub mod nav;
mod nav_tools;
mod locus;
pub mod playtest;
mod playtest_engine;
mod procgen_tools;
pub mod project;
mod project_engine;
mod project_kinds;
mod project_text;
mod project_tools;
mod response;
pub mod review;
mod review_engine;
mod review_screen;
mod review_tools;
mod schedule;
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
pub mod two_d;
mod ui;
mod sim_writes;
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
/// world_batch lists per-op results up to this many bytes, then summarises the rest.
const MAX_BATCH_RESULT_BYTES: usize = 32 * 1024;

const USAGE: &str = "usage: specula [--port N] [--headless] [--adapter discrete|integrated|<index>] [--size WxH] \
[--captures DIR] [--worlds DIR] [--world NAME] [--saves DIR] [--cache DIR] [--assets DIR] [--style FILE] [--perf FILE] \
[--paused] [--max-fps N] [--response-format concise|detailed] [--physics-threads N] [-- GAME ARGS...]";

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
    adapter: AdapterPreference,
    window_size: (u32, u32),
    captures: Option<PathBuf>,
    worlds: Option<PathBuf>,
    world: Option<String>,
    /// Where save games live (`--saves`, default `./saves`).
    saves: Option<PathBuf>,
    /// Pipeline cache directory (`--cache`, default `./.specula-cache`).
    cache: Option<PathBuf>,
    /// How `--adapter` was given, to name the pipeline cache file.
    adapter_label: String,
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
    /// Threads physics may use (`--physics-threads`, 1..64, default 1): the narrowphase, and
    /// with the world's `physics.coloured` the contact solver. The results never depend on it.
    physics_threads: usize,
    /// The window's frame cap (`--max-fps`, 20..1000; over `perf.toml` `[window] max_fps`).
    max_fps: Option<u32>,
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from(argv: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args {
        port: DEFAULT_PORT,
        headless: false,
        adapter: AdapterPreference::HighPerformance,
        window_size: (1280, 720),
        captures: None,
        worlds: None,
        world: None,
        saves: None,
        cache: None,
        adapter_label: "discrete".into(),
        perf: None,
        assets: None,
        style: None,
        game_args: Vec::new(),
        paused: false,
        response_format: response::Format::Concise,
        physics_threads: 1,
        max_fps: None,
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
            "--paused" => args.paused = true,
            "--max-fps" => {
                let range = schedule::MIN_MAX_FPS..=schedule::MAX_MAX_FPS;
                args.max_fps = Some(value()?.parse().ok().filter(|n| range.contains(n)).ok_or_else(|| {
                    format!("--max-fps must be {}..{} frames a second
{USAGE}", schedule::MIN_MAX_FPS, schedule::MAX_MAX_FPS)
                })?);
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
                args.adapter_label = v.clone();
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
            "--captures" => args.captures = Some(PathBuf::from(value()?)),
            "--cache" => args.cache = Some(PathBuf::from(value()?)),
            "--perf" => args.perf = Some(PathBuf::from(value()?)),
            "--worlds" => args.worlds = Some(PathBuf::from(value()?)),
            "--saves" => args.saves = Some(PathBuf::from(value()?)),
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

fn run_with(mut game: Option<Box<dyn Game>>) -> Result<(), String> {
    let args = parse_args()?;
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
    let perf_cfg = perf::PerfConfig::load(args.perf.as_deref(), &cwd).map_err(|e| format!("perf budgets: {e}"))?;
    profile::set_enabled(perf_cfg.profiler);
    // A cached pipeline blob skips shader compilation at start-up. The GPU crate rejects one made
    // by another GPU or driver, so a stale file only costs a cache miss.
    let cache_dir = args.cache.clone().unwrap_or_else(|| cwd.join(".specula-cache"));
    let cache_path = perf::pipeline_cache_path(&cache_dir, &args.adapter_label);
    let pipeline_cache = std::fs::read(&cache_path).ok();
    let gpu = Gpu::new(&GpuOptions { validation: Validation::Auto, adapter: args.adapter, pipeline_cache })
        .map_err(|e| format!("GPU init failed: {e}"))?;
    let captures_root = args.captures.unwrap_or_else(|| cwd.join("captures"));
    // `--assets`, else the hosted game's own folder, else the working directory.
    let named_assets = args.assets.or_else(|| game.as_ref().and_then(|g| g.assets_root()));
    let assets = named_assets.clone().unwrap_or_else(|| cwd.clone());
    if let Some(g) = game.as_mut() {
        g.use_assets(&assets);
    }
    let library = Library::new(&assets, args.style.as_deref());
    let mut engine = Engine::new(gpu, &captures_root, library)?;
    engine.windowed = !args.headless;
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
    engine.perf_cfg = perf_cfg;
    engine.worlds_dir = args.worlds.unwrap_or_else(|| cwd.join("worlds"));
    engine.saves_dir = args.saves.unwrap_or_else(|| cwd.join("saves"));
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
            if let Some(problem) = engine.load_board(&dir) {
                eprintln!("specula: {problem}");
            }
            engine.saved = Some(saved);
            eprintln!("specula: loaded world '{name}' ({} entities) from {}", engine.world.len(), dir.display());
        } else {
            // A folder with other files in it isn't ours to fill.
            persist::check_adoptable(&dir).map_err(|e| format!("world '{name}': {e}"))?;
            eprintln!("specula: new world '{name}'; autosaving to {}", dir.display());
        }
        engine.saved_revision = engine.world.revision();
        engine.world_name = Some(name);
    }
    eprintln!(
        "specula: GPU {} (validation {})",
        engine.gpu.adapter().name,
        if engine.gpu.validation_enabled() { "on" } else { "off" }
    );
    eprintln!("specula: captures in {}", engine.capture_dir.display());
    eprintln!("specula: model recipes under {}", assets.display());
    // The assets root's text files become project files (versioned, claimed, journaled).
    engine.project_mount_assets(assets.clone());

    if let Some(g) = &game {
        engine.clock = Clock::new(g.tick_hz());
        engine.game_name = g.name().to_owned();
        // Its changes are attributed to this name: no agent may take it.
        engine.swarm.reserve(g.name());
    }
    // The window's human, in the review queue's attribution.
    engine.swarm.reserve(review::WINDOW_OWNER);
    let game_tools = game.as_ref().map(|g| g.tools()).unwrap_or_default();
    engine.lex_natives = game.as_ref().map(|g| g.lex_natives()).unwrap_or_default();
    let (to_engine, requests) = mpsc::channel();
    let server = Server::start(
        ServerConfig {
            bind: format!("127.0.0.1:{}", args.port),
            server_name: "specula".into(),
            server_version: env!("CARGO_PKG_VERSION").into(),
            instructions: Some(tools::INSTRUCTIONS.into()),
        },
        Arc::new(
            tools::Tools::new(to_engine, game_tools)
                .with_formats(Arc::clone(&engine.formats))
                .with_lex_natives(engine.lex_natives.clone())
                .with_capture_dir(engine.capture_dir.clone(), safe_path::Roots::for_files(&engine.capture_dir, &captures_root, &engine.worlds_dir, named_assets.as_deref()))
                .with_studio(Arc::clone(&engine.studio.shared))
                .with_procgen(procgen_tools::Context { assets: assets.clone(), style: args.style.clone(), captures: engine.capture_dir.clone() }),
        ),
    )
    .map_err(|e| format!("MCP server: {e}"))?;
    eprintln!("specula: MCP at http://{}/mcp — Ctrl+C to stop", server.local_addr());

    // Only now that the port is ours: tidy old capture runs (never a running engine's), then open
    // the window — a second engine on a taken port fails above without touching either.
    prune_capture_runs(&captures_root, MAX_CAPTURE_RUNS - 1, &engine.capture_dir);

    let window = if args.headless {
        None
    } else {
        let (width, height) = args.window_size;
        let ui = UiThread::spawn(WindowDesc { title: "Specula".into(), width, height, ..WindowDesc::default() })
            .map_err(|e| format!("window: {e}"))?;
        let (w, h) = client_size_of(ui.raw);
        engine.gpu.attach_window(ui.raw.hinstance, ui.raw.hwnd, w, h).map_err(|e| format!("swapchain: {e}"))?;
        Some(ui)
    };
    // Assets that will need skinned or textured pipelines get them now, not at their first spawn.
    let (skinned, textured) = models::asset_streams(engine.library.lock().root());
    engine.gpu.prepare_variants(skinned, textured).map_err(|e| format!("pipelines: {e}"))?;
    // Loading is done: from here on, building a pipeline is a visible hitch and a budget error.
    engine.gpu.mark_warm();
    engine.game = game.take();
    engine.start_game();
    if args.paused {
        engine.clock.set_paused(true);
    }

    let result = match &window {
        Some(ui) => engine.run_windowed(ui, &requests, &server),
        None => engine.run_headless(&requests, &server),
    };
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
    server.shutdown();
    // Tear the GPU down (window surface included) before the window itself, and report any
    // validation messages raised during shutdown.
    match engine.gpu.pipeline_cache_data() {
        Ok(blob) => {
            let saved = std::fs::create_dir_all(&cache_dir).and_then(|()| std::fs::write(&cache_path, blob));
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
    let errors = leftover.iter().filter(|m| m.severity == Severity::Error).count();
    for m in leftover.iter().filter(|m| m.severity >= Severity::Warning) {
        eprintln!("specula: vulkan {:?} {}: {}", m.severity, m.id_name, m.message);
    }
    eprintln!("specula: shut down ({errors} validation errors)");
    interrupt::finished();
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
    /// `world.revision()` as last saved or loaded.
    saved_revision: u64,
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
    /// A window title set by the game.
    game_title: Option<String>,
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
    /// A stream_fly flight of the window camera in progress.
    fly: Option<stream_tools::Fly>,
    /// Headless playtests: the running one and recent reports (see [`playtest`]).
    playtests: playtest_engine::Playtests,
    /// Default response formats (`--response-format`, `agent_hello`), shared with the tools.
    formats: Arc<response::Defaults>,
    /// Project files on disk: mounts, the watcher, failed writes (see [`project`]).
    project: project::Disk,
    /// When failed project writes were last tried.
    project_retry: Instant,
    /// The window's review screen (see `review_screen`).
    review_ui: review_screen::ReviewUi,
    /// The gamepads handed to the hosted game each window frame (reused), and the gate that
    /// releases them while the review screen holds the input.
    game_pads: Vec<specula_platform::GamepadState>,
    pad_gate: game::PadGate,
}

/// A model_build or model_reload call waiting for builds on the worker threads.
struct BuildWaiter {
    /// Jobs still running.
    jobs: BTreeSet<u64>,
    /// model_build: the spec reported on (`None`: a reload).
    spec: Option<world::ModelSpec>,
    /// model_reload: the report so far.
    reload: models::Reload,
    reply: std::sync::mpsc::Sender<ToolOutput>,
    deadline: Instant,
    answer: Option<ToolOutput>,
}

struct AutosaveJob {
    /// The world revision the job is writing.
    revision: u64,
    done: Receiver<Result<(persist::Saved, persist::SaveReport), persist::PersistError>>,
}

impl Engine {
    fn new(mut gpu: Gpu, captures_root: &Path, library: Library) -> Result<Engine, String> {
        let meshes = ShapeMeshes::upload(&mut gpu).map_err(|e| format!("mesh upload: {e}"))?;
        let two_d = two_d::TwoDGpu::new(library.root());
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
            two_d,
            draws_2d: Vec::new(),
            last_2d_gc: (Instant::now(), 0),
            windowed: false,
            build_waiters: Vec::new(),
            capture_waiters: Vec::new(),
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
            saved_revision: 0,
            last_autosave: Instant::now(),
            saved: None,
            world_lock: None,
            autosave_job: None,
            autosave_error: None,
            leftovers: Vec::new(),
            clock: Clock::new(DEFAULT_TICK_HZ),
            game: None,
            game_name: String::new(),
            pacing: perf::Pacing::default(),
            schedule: Schedule::default(),
            profiler: perf::Profiler::new(120),
            perf_cfg: perf::PerfConfig::default(),
            last_advance: Instant::now(),
            input: Vec::new(),
            game_title: None,
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
            fly: None,
            playtests: Default::default(),
            formats: response::Defaults::new(response::Format::Concise),
            project: project::Disk::default(),
            project_retry: Instant::now(),
            review_ui: review_screen::ReviewUi::default(),
            game_pads: Vec::new(),
            pad_gate: game::PadGate::default(),
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
        let failed = self.world.replace(may, &mut self.swarm, world, &format!("world:{name}"), self.clock.seconds(), models);
        let problems =
            failed.iter().map(|(id, e)| format!("model {} ({id}) not built: {e}", self.world.path_of(*id))).collect();
        self.world_changed_wholesale();
        problems
    }

    /// What the engine keys on the live world starts over after it was replaced (a load, or a
    /// writer's bug the write policy recovered from: [`Engine::check_world_replaced`]).
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
        let Some(game) = self.game.as_mut() else { return };
        sim_writes::game_start(&mut self.world, &mut self.swarm, game.as_mut(), &self.clock, &self.game_name, self.windowed);
        self.last_advance = Instant::now();
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
    fn advance_clock(&mut self) {
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
            // Animations set during the tick start at its time.
            self.world.set_clock_s(self.clock.seconds());
            if self.game.is_some() {
                let _t = profile::zone("tick");
                let start = Instant::now();
                // Game updates are journaled replay-only and coalesced per tick (F3).
                self.game_call(sim_writes::GameCall::Tick);
                self.pacing.tick.push(start.elapsed());
            }
            if !self.logic.is_empty() {
                let _l = profile::zone("modules");
                self.lend_physics();
                // The game's rules: journaled, exempt from claims.
                sim_writes::module_tick(&mut self.world, &mut self.swarm, &mut self.logic, self.clock.tick(), self.clock.hz());
                self.return_physics();
            }
            // Live Lex rule sets step on the same tick, after the modules (whose hooks they call).
            if !self.lex_live.is_empty() {
                let _l = profile::zone("lex_live");
                self.lex_live_tick();
            }
            // Navigation decides and steers on the same tick, before physics walks its characters.
            self.nav_tick();
            // Physics steps on the same fixed tick, after the game moved what it moves.
            self.physics_tick();
            self.check_world_replaced();
            self.clock.finish_tick();
            let _h = profile::zone("tick_hash");
            self.record_tick_hash();
        }
        self.world.set_clock_s(self.clock.seconds());
        if ticks > 0 {
            // A tick may have asked for something (the game's wave-start autosave).
            self.serve_game_requests();
        }
    }

    /// The game's per-frame update: input, camera, title.
    fn game_frame(&mut self, ui: &UiThread, size: (u32, u32), dt: Duration) {
        self.input.clear();
        ui.drain_events(&mut self.input);
        // The review screen reads input first; while it is open the game, modules and live Lex
        // runs get none (no keys, pointer or pads).
        let modal = self.review_frame(size);
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
        let agent_view = self.window_follow.is_some();
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
        self.pad_gate.apply(modal, &mut pads);
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
    }

    /// The hosted game's interface for one image ([`Game::overlay`]) into `out` (cleared first).
    fn game_overlay(&mut self, size: (u32, u32), camera: Camera, capture: bool, out: &mut Vec<OverlayQuad>) {
        out.clear();
        let Some(game) = self.game.as_mut() else { return };
        if size.0 == 0 || size.1 == 0 {
            return;
        }
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

    /// Saves the current world, if there is one and it changed. The engine thread only takes the
    /// changed subtrees off the world ([`persist::plan`]); rendering and writing happen on a worker
    /// thread, unless `wait` (shutdown).
    fn autosave(&mut self, wait: bool) {
        self.last_autosave = Instant::now();
        self.finish_autosave(false);
        let Some(name) = self.world_name.clone() else { return };
        if self.autosave_job.is_some() || !self.dirty() {
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
        } else if let Err(e) = std::thread::Builder::new().name("specula-autosave".into()).spawn(job) {
            // The plan went with the closure; try again at the next autosave.
            eprintln!("specula: could not start the autosave thread: {e}");
            return;
        }
        self.autosave_job = Some(AutosaveJob { revision, done });
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
        let revision = job.revision;
        self.autosave_job = None;
        match result {
            Ok((saved, report)) => {
                self.saved = Some(saved);
                self.saved_revision = revision;
                self.autosave_error = None;
                self.note_leftovers(&report);
            }
            Err(e) => {
                eprintln!("specula: autosave failed: {e}");
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
    fn poll_assets(&mut self, force: bool) -> Option<models::Reload> {
        let reload = self.library.lock().poll(force);
        // Sprite and tileset PNGs changed on disk are reloaded (failed ones retried).
        self.two_d.poll_files(force);
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
            let mut live: Vec<u32> = self.world.entities().filter_map(|e| e.resolved_model().map(|r| r.handle)).collect();
            live.sort_unstable();
            live.dedup();
            self.library.lock().drop_unwanted(&live);
            self.last_unwanted = (Instant::now(), drops);
        }
        let due = (self.last_gc.0.elapsed() >= GC_EVERY && epoch != self.last_gc.1)
            // More specs than the world could use: certainly garbage, collect now.
            || specs > self.world.len() + models::SPARE_HANDLES;
        if force || due {
            let live = models::live_handles(&self.world);
            self.library.lock().collect_garbage(&live);
            self.last_gc = (Instant::now(), epoch);
        }
        let reload = reload?;
        if reload.files_changed > 0 || !reload.rebuilt.is_empty() || !reload.errors.is_empty() {
            for (id, e) in self.world.refresh_models().into_iter().take(10) {
                eprintln!("specula: model {} ({id}) not rebuilt: {e}", self.world.path_of(id));
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
            self.world.refresh_model_handles(&changed);
        }
        let now = Instant::now();
        let mut waiters = std::mem::take(&mut self.build_waiters);
        waiters.retain_mut(|w| {
            for f in &finished {
                if w.jobs.remove(&f.job) {
                    match (&f.result, &w.spec) {
                        (Ok(b), Some(spec)) => {
                            self.library.lock().remember_build(b);
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
            if w.jobs.is_empty() && w.answer.is_none() {
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
                            Json::from(format!(
                                "{} build(s) still running; they finish in the background (engine_info models.library)",
                                w.jobs.len()
                            )),
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
            };
            if let Some(p) = texture {
                self.two_d.request(p);
                if fresh && checked.insert(p) {
                    self.two_d.check_now(p, checked.len() <= MAX_CAPTURE_STATS);
                }
                waiting += self.two_d.is_pending(p) as usize;
            }
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
        let out = || ToolOutput::error("the engine is shutting down; the call was not completed");
        for w in self.capture_waiters.drain(..) {
            let _ = w.reply.send(out());
        }
        for w in self.build_waiters.drain(..) {
            let _ = w.reply.send(out());
        }
        if let Some(f) = self.fly.take() {
            f.abort();
        }
        self.studio_shutting_down();
        // A perf_capture in progress answers with the frames it has.
        self.profiler.abort_capture();
    }

    /// model_build and model_reload: answered when their builds finish (other calls go on).
    fn handle_model_call(&mut self, request: Request) {
        let Request { command, reply, deadline, .. } = request;
        let mut waiter = BuildWaiter { jobs: BTreeSet::new(), spec: None, reload: models::Reload::default(), reply, deadline, answer: None };
        match command {
            Command::ModelBuild(spec) => {
                let requested = self.library.lock().request(&spec);
                match requested {
                    Err(e) => waiter.answer = Some(world_error(&e)),
                    Ok(models::BuildRequest::Ready(b, cached)) => {
                        waiter.answer = Some(ToolOutput::text(models::report(&spec, &b, cached).to_pretty()))
                    }
                    Ok(models::BuildRequest::Pending(job)) => {
                        waiter.jobs.insert(job);
                        waiter.spec = Some(spec);
                    }
                }
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
        let errors = self.sync_meshes();
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
        let meshes = Meshes { shapes: &self.meshes, lathes: &self.lathes, models: &self.models };
        // The window's cache remembers each model's level itself (hysteresis); captures choose
        // afresh (reproducible).
        let lod = lod.map_or(LodChoice::Fresh, LodChoice::Fixed);
        let mut view = View { camera, viewport_h, include_ui, lod };
        let mut owners = owners;
        let stats = if window {
            self.draw_cache.draw_list(&self.world, self.world_generation, &meshes, &mut view, out)
        } else {
            sight::draw_list(&self.world, &meshes, &mut view, out, owners.as_deref_mut())
        };
        let hlod = &mut self.stream.hlod;
        std::mem::swap(&mut self.models.cover, if window { &mut hlod.window_cover } else { &mut hlod.capture_cover });
        out.extend_from_slice(&proxies);
        // Proxy draws belong to no one entity: id 0, which the ID buffer doesn't count.
        if let Some(o) = owners {
            o.resize(out.len(), 0);
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
            let window = if minimized { schedule::MINIMIZED_SERVE } else { schedule.window() };
            let serve_start = Instant::now();
            let ended = schedule.serve(
                &mut inbox,
                &mut held,
                serve_start + window,
                |r| call_class(&r.command),
                |r| {
                    let _z = profile::zone("requests");
                    self.handle(r, server);
                },
                || interrupt::requested() || ui.close_requested(),
            );
            if ended != Served::Window {
                // Stopping (or the server is gone). Held calls' reply channels close as they
                // drop, which answers them "shutting down".
                break;
            }
            let served = serve_start.elapsed();
            let frame_zone = profile::zone("frame");
            let frame_started = Instant::now();
            self.autosave_if_due();
            self.sweep_claims(server);
            self.lex_live_end_holds(server);
            self.poll_assets(false);
            self.pump_builds(None);
            self.poll_saves();
            self.project_pump();
            self.serve_captures();
            self.pump_playtests(server, playtest_engine::WINDOW_SLICE);
            self.pump_studio(studio::WINDOW_SHARE, server);
            self.advance_clock();
            let now = Instant::now();
            let dt = now - last_frame;
            last_frame = now;
            self.game_frame(ui, size, dt);
            let next = match &self.game_title {
                Some(t) => t.clone(),
                None => format!("Specula — {} entities — {} agents", self.world.len(), server.sessions().len()),
            };
            if next != title {
                ui.set_title(&next);
                title = next;
            }
            // Read the size directly: it stays current even while the window thread is inside a
            // move/resize loop.
            let now = client_size_of(ui.raw);
            if now != size {
                size = now;
                self.gpu.resize_window(size.0, size.1).map_err(|e| format!("resize: {e}"))?;
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
            self.follow_window_camera();
            if let Some(cam) = self.fly_camera(human_took_controls(&self.input)) {
                self.window_follow = None;
                self.window_camera = Some(cam);
            }
            let camera = self.window_camera.unwrap_or_else(|| sight::auto_camera(&self.world, 50.0));
            // Streaming: this camera is a source; uploads within the frame's budget, before the
            // draw list so this frame draws them.
            self.stream_tick(Some((camera, size.0, size.1)));
            // The game's interface (its own zone, outside the draw list's).
            let mut game_quads = std::mem::take(&mut self.game_quads);
            self.game_overlay(size, camera, false, &mut game_quads);
            self.review_overlay(size, false, &mut game_quads);
            let draw_list = profile::zone("draw_list");
            let mut draws = std::mem::take(&mut self.draws);
            let started = Instant::now();
            let time_s = self.window_time_s();
            let (stats, errors) = self.build_draws(camera, size.1, true, None, true, time_s, &mut draws, None);
            self.pacing.draw_list.push(started.elapsed());
            for e in errors {
                eprintln!("specula: {e}");
            }
            self.lod_window = stats;
            let mut lights = std::mem::take(&mut self.lights);
            sight::lights(&self.world, true, &mut lights);
            let view_proj = camera.view_proj(size.0, size.1);
            let mut draws_2d = std::mem::take(&mut self.draws_2d);
            self.build_draws_2d(&view_proj, true, time_s as f32, &mut draws_2d, None);
            let bones = self.models.take_bones();
            let mut frame = FrameDesc {
                bones: &bones,
                draws_2d: &draws_2d,
                overlay: &game_quads,
                ..sight::frame(&self.world, &view_proj, &draws, &lights, time_s as f32)
            };
            self.luts.apply(&mut self.gpu, self.world.env(), &mut frame, false);
            drop(draw_list);
            // FIFO present paces the loop to the display refresh. How long it blocked sizes the
            // next serve window.
            let present = profile::zone("present");
            let frame_cpu = frame_started.elapsed();
            let presented = self.gpu.present(&frame).map_err(|e| format!("present: {e}"));
            let (frame_no, cpu) = self.gpu.last_window_timing();
            let blocked = Duration::from_secs_f64((cpu.wait_ms + cpu.submit_ms).max(0.0) / 1000.0);
            schedule.presented(frame_no, served, blocked, Instant::now());
            self.models.put_bones(bones);
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
        while !interrupt::requested() {
            // Wake for the next tick at the latest, so a hosted game keeps real time headless too
            // (and about 60 times a second while perf_capture records frames).
            let most = if self.profiler.capturing() { Duration::from_millis(16) } else { Duration::from_millis(100) };
            let mut wait = self.clock.until_next().min(most);
            if self.library.lock().in_flight() > 0 || !self.capture_waiters.is_empty() || self.stream.busy() || self.saves_in_flight() {
                // Builds land promptly (and calls waiting on them are answered).
                wait = wait.min(Duration::from_millis(5));
            }
            if self.nav.loading() {
                // A load builds back to back (calls are still read between slices).
                wait = Duration::ZERO;
            }
            if self.studio_busy() {
                // Studio renders run in slices, with calls read between them.
                wait = Duration::ZERO;
            }
            if self.playtests.running() {
                // Flat out, unless it waits for a deferred answer.
                wait = if self.playtests.waiting_on_call() { wait.min(Duration::from_millis(1)) } else { Duration::ZERO };
            }
            let received = requests.recv_timeout(wait);
            self.profiler.end_frame(&self.gpu);
            let _frame = profile::zone("frame");
            match received {
                Ok(request) => {
                    let _z = profile::zone("requests");
                    self.handle(request, server);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
            self.advance_clock();
            self.autosave_if_due();
            self.sweep_claims(server);
            self.lex_live_end_holds(server);
            self.poll_assets(false);
            self.pump_builds(None);
            // Streaming headless: captures' views are the sources; retired memory is freed.
            self.stream_tick(None);
            if let Err(e) = self.gpu.flush_uploads() {
                eprintln!("specula: upload batch not submitted: {e}");
            }
            self.poll_saves();
            self.project_pump();
            self.serve_captures();
            self.pump_playtests(server, playtest_engine::HEADLESS_SLICE);
            self.pump_studio(studio::HEADLESS_SHARE, server);
        }
        Ok(())
    }

    fn handle(&mut self, request: Request, server: &Server) {
        if Instant::now() > request.deadline {
            // The caller is about to give up; applying the command now would surprise it.
            let _ = request.reply.send(ToolOutput::error("skipped: the engine was busy too long; the command was not applied"));
            return;
        }
        // Every command (world edits, model builds, sight, saves, playtests) runs in the default
        // floating-point environment, whatever the frame just drawn (the GPU driver) left behind
        // (rule 6).
        self.fp.check(self.clock.tick(), "before_command");
        // perf_capture answers once its frames are recorded (see perf::Profiler::end_frame).
        let request = match request.command {
            Command::PerfCapture(spec) => {
                let kind = perf::adapter_kind(self.gpu.adapter().kind);
                let ctx = perf::CaptureContext {
                    dir: self.capture_dir.clone(),
                    budgets: self.perf_cfg.budgets(kind),
                    adapter: kind.to_owned(),
                };
                self.profiler.start_capture(spec, request.reply, request.deadline, ctx);
                return;
            }
            Command::Stream(stream_tools::StreamCall::Fly(spec)) => {
                self.start_fly(spec, request.reply, request.deadline);
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
            command => Request { command, ..request },
        };
        if matches!(request.command, Command::ModelBuild(_) | Command::ModelReload) {
            self.handle_model_call(request);
            return;
        }
        if matches!(request.command, Command::Frame(_) | Command::Sight(_)) {
            self.handle_sight(request);
            return;
        }
        if matches!(request.command, Command::SaveGame(save_tools::SaveCall::Write { .. } | save_tools::SaveCall::Read { .. })) {
            self.handle_save_request(request);
            return;
        }
        let by = self.swarm.agent_name(&request.session);
        // Ownership (claims, undo) goes by identity key, which a resumed agent keeps.
        let session = self.swarm.key(&request.session);
        let w = Writer::new(&by, Some(&session));
        let tick = self.clock.tick();
        // A game tool (play_ui pressing a menu's Save) may queue requests for the engine.
        let game_call = matches!(request.command, Command::Game { .. });
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
            Command::Save { name, overwrite } => self.save(name, overwrite),
            // A playtest owns the clock and the world until it ends (as for save_read): a load
            // would replace the world under it. Not even with force. Nor while others hold claims.
            Command::Load { name, force } => match write_policy::may_replace(&self.swarm, w, &self.playtests) {
                Ok(may) => self.load(may, name, force),
                Err(e) => world_error(&e),
            },
            Command::Hello { name, role, resume } => match self.swarm.hello(&request.session, name, role, resume, &server.all_sessions()) {
                Ok(reply) => ToolOutput::text(reply.to_pretty()),
                Err(e) => world_error(&e),
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
            Command::View(view) => match view {
                tools::ViewSpec::Auto => {
                    self.window_released_by = None;
                    self.window_follow = None;
                    self.window_camera = None;
                    ToolOutput::text("window camera: auto-framing the world")
                }
                tools::ViewSpec::Fixed(c) => {
                    self.window_released_by = None;
                    self.window_follow = None;
                    self.window_camera = Some(c);
                    ToolOutput::text(Json::obj([("window_camera", c.to_json())]).to_pretty())
                }
                tools::ViewSpec::Named { name, of } => {
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
            Command::Perf { budget_ms, graph } => self.perf(budget_ms, graph),
            Command::PerfZones(q) => {
                if let Some(on) = q.enabled {
                    profile::set_enabled(on);
                }
                ToolOutput::text(perf::zones_json(&self.profiler, &q).to_string())
            }
            Command::PerfMemory { limit } => {
                let stats = self.gpu.last_frame_stats_of(FramePath::Window);
                let stats = stats.or_else(|| self.gpu.last_frame_stats_of(FramePath::Offscreen));
                ToolOutput::text(perf::memory_json(&self.profiler, stats.as_ref(), limit).to_string())
            }
            Command::PerfCapture(_) | Command::Playtest(_) => unreachable!("answered above"),
            Command::Time(op) if playtest_engine::clock_refused(op, self.playtests.running()) => playtest_engine::clock_busy(),
            // Steps wait for a load to finish (bounded); they are never queued behind it.
            Command::Time(TimeOp::Step(_)) if self.nav.loading() && !self.build_nav_load(nav::LOAD_CALL_WAIT) => self.nav_loading_error(),
            Command::Time(op) => {
                match op {
                    TimeOp::Pause => self.clock.set_paused(true),
                    TimeOp::Resume => self.clock.set_paused(false),
                    TimeOp::Step(n) => {
                        self.clock.set_paused(true);
                        self.clock.step(n);
                        // Holds of agents gone since the last frame end before these ticks fire them.
                        self.lex_live_end_holds(server);
                        // Run them now, so the reply shows the result.
                        self.advance_clock();
                    }
                    TimeOp::Scale(x) => self.clock.set_scale(x),
                    TimeOp::Info => {}
                }
                ToolOutput::text(self.clock_json().to_pretty())
            }
            Command::Game { name, args } => match self.game.as_mut() {
                Some(game) => {
                    // A claim can only reject the tool when claims exist: checkpoint only then.
                    let checkpoint = if self.swarm.claims.is_empty() { None } else { game.checkpoint() };
                    let (clock, agent, windowed) = (&self.clock, self.game_name.as_str(), self.windowed);
                    let call = |gw: &mut game::GameWorld| game.call_tool(&mut Ctx { world: gw.reborrow(), clock, agent, windowed }, &name, &args);
                    match self.world.game_tool(&mut self.swarm, tick, agent, w, call) {
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
            Command::Undo(req) => match self.world.undo(&mut self.swarm, tick, w, &req) {
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
            },
            Command::Hash { recent, tick, verify } => self.hash_report(recent, tick, verify),
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
            Command::SaveGame(call) => self.handle_save(call, w, &by),
            Command::ModelBuild(_) | Command::ModelReload => unreachable!("answered by handle_model_call"),
            Command::Logic(call) => {
                self.lend_physics();
                let out = sim_writes::logic_tools(&mut self.world, &mut self.swarm, &mut self.logic, &self.clock, call, w);
                self.return_physics();
                self.journal_module_events(Some(w));
                out
            }
            Command::Anim(call) => self.anim_call(call, w, tick),
            Command::StudioRender(_) | Command::StudioDone(_) => unreachable!("queued by studio_enqueue"),
            Command::Project(call) => self.project_call(call, w, &by),
        };
        self.check_world_replaced();
        // Undo (or anything else) changed project files: the disk follows before the answer.
        self.project_after_request();
        if game_call {
            self.serve_game_requests();
        }
        debug_assert!(self.world.pending_changes().is_empty(), "every world change is journaled");
        // The caller may have timed out; nothing to do then.
        let _ = request.reply.send(output);
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
    fn hash_report(&mut self, recent: usize, tick: Option<u64>, verify: bool) -> ToolOutput {
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
        if verify {
            let started = Instant::now();
            let report = match self.swarm.journal().replay() {
                Some(w) => {
                    let replayed = w.state_hash();
                    Json::obj([
                        ("replayable", Json::from(true)),
                        ("entries", Json::from(self.swarm.journal().len())),
                        ("replay_hash", Json::from(journal::hex(replayed))),
                        ("matches", Json::from(replayed == hash)),
                        ("ms", Json::from(started.elapsed().as_secs_f64() * 1000.0)),
                    ])
                }
                None => Json::obj([
                    ("replayable", Json::from(false)),
                    ("message", Json::from("older journal entries were dropped (memory limit), so it can't replay from its base")),
                ]),
            };
            out.set("verify", report);
        }
        ToolOutput::text(out.to_string())
    }

    /// One page of world_list / world_query, cut short at [`MAX_LISTING_BYTES`].
    fn query(&self, spec: &QuerySpec) -> ToolOutput {
        let page = match self.world.query(&spec.query) {
            Ok(page) => page,
            Err(e) => return world_error(&e),
        };
        let (mut items, mut bytes, mut cut, mut last) = (Vec::new(), 0, false, None);
        for (e, path) in &page.items {
            let item = match &spec.fields {
                None => e.to_json(path),
                Some(fields) => self.world.fields_json(e, path, fields),
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
        let next = if cut || page.more { last.map(tools::cursor_for) } else { None };
        // Compact: listings are the biggest responses, and indentation costs agents tokens.
        let out = Json::obj([("total", Json::from(page.total)), (spec.key, Json::Arr(items)), ("next_cursor", Json::from(next))]);
        ToolOutput::text(out.to_string())
    }

    /// The lock for `dir` if this engine doesn't hold it yet (`None` when it already does).
    fn lock_for(&self, dir: &Path) -> Result<Option<persist::WorldLock>, persist::PersistError> {
        if self.world_lock.as_ref().is_some_and(|l| l.dir() == dir) {
            return Ok(None);
        }
        persist::WorldLock::acquire(dir).map(Some)
    }

    fn save(&mut self, name: Option<String>, overwrite: bool) -> ToolOutput {
        self.finish_autosave(true);
        let Some(name) = name.or_else(|| self.world_name.clone()) else {
            return ToolOutput::error(
                Json::obj([
                    ("error", Json::from("no_world_name")),
                    ("message", Json::from("give 'name': there is no current world (start with --world NAME or save once with a name)")),
                ])
                .to_pretty(),
            );
        };
        let dir = self.worlds_dir.join(&name);
        let current = self.world_name.as_deref() == Some(name.as_str());
        if !current && !overwrite && dir.join("world.json").exists() {
            // Another world lives there: never replace it by accident. (Its world.json records its
            // entity count, so this costs one small read, not a load.)
            let (entities, problem) = match persist::peek_entity_count(&dir) {
                Ok(n) => (Json::from(n), Json::Null),
                Err(e) => (Json::Null, e.to_json()),
            };
            return ToolOutput::error(
                Json::obj([
                    ("error", Json::from("world_exists")),
                    (
                        "message",
                        Json::from(format!(
                            "a different world named '{name}' already exists; pick another name, world_load it, or pass \
                             overwrite: true to replace it"
                        )),
                    ),
                    ("name", Json::from(name.as_str())),
                    ("entities", entities),
                    ("unreadable", problem),
                ])
                .to_pretty(),
            );
        }
        let lock = match self.lock_for(&dir) {
            Ok(lock) => lock,
            Err(e) => return ToolOutput::error(e.to_json().to_pretty()),
        };
        match persist::save(&self.world, &dir, self.saved.as_ref()) {
            Ok((saved, report)) => {
                if lock.is_some() {
                    self.world_lock = lock;
                }
                self.saved = Some(saved);
                self.world_name = Some(name.clone());
                self.saved_revision = self.world.revision();
                self.last_autosave = Instant::now();
                self.autosave_error = None;
                self.note_leftovers(&report);
                let board_error = self.save_board_now();
                ToolOutput::text(
                    Json::obj([
                        ("saved", Json::from(name)),
                        ("board_save_error", board_error.unwrap_or(Json::Null)),
                        ("dir", Json::from(report.dir.display().to_string())),
                        ("generation", Json::from(report.generation)),
                        ("files", Json::from(report.files)),
                        ("files_written", Json::from(report.written)),
                        ("files_removed", Json::from(report.removed)),
                        ("leftover_files", Json::arr(report.leftovers.iter().map(|f| Json::from(f.as_str())))),
                        // Transient entities are never saved.
                        ("entities", Json::from(self.world.persistent_len())),
                        ("state_hash", Json::from(format!("{:016x}", self.world.state_hash()))),
                    ])
                    .to_pretty(),
                )
            }
            Err(e) => {
                if e.invalidates_cache() {
                    self.saved = None;
                }
                ToolOutput::error(e.to_json().to_pretty())
            }
        }
    }

    fn load(&mut self, may: write_policy::MayReplace, name: String, force: bool) -> ToolOutput {
        self.finish_autosave(true);
        // The outgoing world's task board is kept with it (a world without a folder can't keep one).
        self.save_board_now();
        let board_unsaved = self.board_unsaved();
        if (self.dirty() || board_unsaved) && !force {
            let current = self.world_name.as_deref().map_or(Json::Null, Json::from);
            return ToolOutput::error(
                Json::obj([
                    ("error", Json::from("unsaved_changes")),
                    (
                        "message",
                        Json::from(
                            "the live world (or its task board) has unsaved changes; world_save first, or pass force: true to discard them",
                        ),
                    ),
                    ("current_world", current),
                    ("board_unsaved", Json::from(board_unsaved)),
                    ("entities", Json::from(self.world.len())),
                ])
                .to_pretty(),
            );
        }
        let dir = self.worlds_dir.join(&name);
        let lock = match self.lock_for(&dir) {
            Ok(lock) => lock,
            Err(e) => return ToolOutput::error(e.to_json().to_pretty()),
        };
        match persist::load_limited(&dir, self.world.shape_limits()) {
            Ok((mut world, saved)) => {
                if lock.is_some() {
                    self.world_lock = lock;
                }
                // Live Lex runs presented into the outgoing world, and world files hold no runs:
                // they stop, and what the files kept of /lex_live (nothing would update it) goes.
                let removed = lex_live::remove_presentation(&mut world);
                let stopped = self.lex_live.stop_all();
                let mut problems = self.replace_world(may, world, &name);
                problems.extend(saved.warnings.iter().cloned());
                problems.extend(self.load_board(&dir));
                self.saved = Some(saved);
                self.world_name = Some(name.clone());
                self.saved_revision = self.world.revision();
                self.last_autosave = Instant::now();
                let mut out = vec![
                    ("loaded", Json::from(name)),
                    ("dir", Json::from(dir.display().to_string())),
                    ("entities", Json::from(self.world.len())),
                    ("state_hash", Json::from(format!("{:016x}", self.world.state_hash()))),
                    ("warnings", Json::arr(problems.into_iter().take(20).map(Json::from))),
                ];
                if !stopped.is_empty() || removed > 0 {
                    out.push((
                        "lex_live",
                        Json::obj([
                            ("stopped", Json::arr(stopped.into_iter().map(Json::from))),
                            ("presentation_removed", Json::from(removed)),
                            ("note", Json::from("world files hold no live Lex runs: runs live before the load stopped, and /lex_live entities in the files were removed; lex_live {action: start} runs a set again (save_write / save_read keep runs)")),
                        ]),
                    ));
                }
                ToolOutput::text(Json::obj(out).to_pretty())
            }
            Err(e) => ToolOutput::error(e.to_json().to_pretty()),
        }
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

    fn save_capture(&self, extension: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.capture_dir).map_err(|e| e.to_string())?;
        let path = self.capture_dir.join(format!("frame_{:05}.{extension}", self.frames_captured));
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(path)
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
            ("render_graph", perf::graph_json(self.gpu.last_graph().as_ref(), graph)),
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
            ("agents", Json::arr(agents)),
            ("clock", self.clock_json()),
            ("swarm", {
                let mut s = self.swarm.stats_json();
                s.set("unjournaled_writes", Json::from(self.world.strays()));
                let (n, last) = self.world.replacements();
                s.set("world_replacements", Json::from(n));
                s.set("game_reserved_writes", Json::from(self.world.game_reserved_writes()));
                if let Some((entry, tick)) = last {
                    s.set("last_world_replacement", Json::obj([("entry", Json::from(entry.as_str())), ("tick", Json::from(*tick))]));
                }
                s
            }),
            ("models", self.models_json()),
            ("window", self.window_json()),
            // The floating-point environment guard: what it checks and what it had to restore,
            // at startup and between ticks (`engine`) and around module calls (`modules`).
            ("determinism", Json::obj([("fp_env", self.fp.to_json()), ("fp_env_modules", self.logic.fp_log().to_json())])),
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
        }
        if let Some(game) = &self.game {
            info.set("game", Json::obj([("name", Json::from(self.game_name.as_str())), ("status", game.status())]));
        }
        ToolOutput::text(info.to_pretty())
    }
}

/// What kind of cost a call has, for the window's schedule: a serve window only starts a call
/// when it has room for what calls of its class cost lately (see `schedule`).
fn call_class(command: &Command) -> Class {
    match command {
        Command::Frame(_) | Command::Sight(_) => Class::Render,
        Command::Anim(call) => anim_tools::call_class(call),
        Command::StudioRender(_) | Command::StudioDone(_) => Class::Read,
        Command::Save { .. } | Command::Load { .. } | Command::ModelBuild(_) | Command::ModelReload => Class::Disk,
        Command::Hash { verify: true, .. } => Class::Big,
        Command::Swarm(call) => call.class(),
        Command::Review(call) => call.class(),
        Command::Batch(ops) if ops.len() > HEAVY_BATCH_OPS => Class::Big,
        Command::Time(TimeOp::Step(_)) => Class::Sim,
        Command::Game { .. } => Class::Game,
        Command::Logic(_) => Class::Disk,
        Command::Physics(call) => call.class(),
        Command::Nav(call) => call.class(),
        Command::Place(call) => call.class(),
        Command::Stream(call) => call.class(),
        Command::SaveGame(call) => call.class(),
        Command::Playtest(_) => Class::Sim,
        Command::Project(call) => call.class(),
        Command::Lex(call) if call.is_read() => Class::Read,
        Command::Lex(_) => Class::Edit,
        Command::LexLive(call) => call.class(),
        Command::Spawn(_)
        | Command::Set(..)
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
        | Command::PerfCapture(_) => Class::Edit,
        Command::Query(_)
        | Command::TileGet { .. }
        | Command::Info
        | Command::Perf { .. }
        | Command::PerfZones(_)
        | Command::PerfMemory { .. }
        | Command::History(_)
        | Command::Changes(_)
        | Command::Hash { .. }
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
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let mut runs: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let (secs, pid) = parse_run_name(e.file_name().to_str()?)?;
            let path = e.path();
            (path != current && !specula_sys::process_is_running(pid)).then_some((secs, path))
        })
        .collect();
    runs.sort();
    let excess = runs.len().saturating_sub(keep);
    for (_, path) in runs.into_iter().take(excess) {
        if let Err(e) = std::fs::remove_dir_all(&path) {
            eprintln!("specula: could not prune old captures {}: {e}", path.display());
        }
    }
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

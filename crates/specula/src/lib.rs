//! Specula world server: owns the world, renders it, and serves agents over MCP.
//!
//! A library so game binaries can link the engine; `main.rs` is the stock `specula` binary.
//! [`world`] is public: games running in-process call [`world::World`] directly every tick.
//! A game binary implements [`game::Game`] and calls [`run_game`]; the engine then runs it on a
//! fixed-tick clock, feeds it window input, and serves its extra tools to agents.
//!
//! Threads: protocol threads parse tool calls and hand them to the engine (main) thread over a
//! channel; the engine thread owns the world and GPU, applies commands between frames, renders,
//! and replies. The human's window is pumped by its own thread (see `ui`), so dragging it never
//! stalls agents.

pub mod cameras;
mod capture;
pub mod claims;
pub mod compare;
pub mod effects;
pub mod game;
pub mod journal;
mod meshes;
pub mod models;
pub mod orbit;
mod perf;
pub mod persist;
pub mod sight;
pub mod sight_overlay;
mod sight_tools;
pub mod swarm;
mod tools;
mod ui;
pub mod world;

use game::{Clock, Ctx, Game};
use journal::{Kind, Writer};
use meshes::{LatheMeshes, ShapeMeshes};
use models::{GpuModels, Library, LodChoice, LodStats, SharedLibrary};
use sight::{Camera, Meshes, View};
use specula_core::deflate::Level;
use specula_core::{base64, png, Json};
use specula_gpu::{AdapterPreference, DrawItem, FrameDesc, FramePath, Gpu, GpuOptions, Severity, TargetId, Validation};
use specula_platform::{client_size_of, Event, WindowDesc};
use specula_protocol::{Content, Server, ServerConfig, SessionInfo, ToolOutput};
use specula_sys::interrupt;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use swarm::Swarm;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tools::{Command, FrameSpec, QuerySpec, Request, TimeOp};
use ui::UiThread;
use world::{Recording, World, WorldError};

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
[--captures DIR] [--worlds DIR] [--world NAME] [--cache DIR] [--assets DIR] [--style FILE] [-- GAME ARGS...]";

/// How long a capture waits (parked, the engine serving others) for models still building.
const CAPTURE_BUILD_WAIT: Duration = Duration::from_secs(10);
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

struct Args {
    port: u16,
    headless: bool,
    adapter: AdapterPreference,
    window_size: (u32, u32),
    captures: Option<PathBuf>,
    worlds: Option<PathBuf>,
    world: Option<String>,
    /// Pipeline cache directory (`--cache`, default `./.specula-cache`).
    cache: Option<PathBuf>,
    /// How `--adapter` was given, to name the pipeline cache file.
    adapter_label: String,
    /// Where model recipes live (`--assets`, default the working directory).
    assets: Option<PathBuf>,
    /// One style for every recipe (`--style`; default: the `style.json` next to each recipe).
    style: Option<PathBuf>,
    /// Everything after `--` (or `--game-args`): the hosted game's own arguments.
    game_args: Vec<String>,
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
        cache: None,
        adapter_label: "discrete".into(),
        assets: None,
        style: None,
        game_args: Vec::new(),
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
            "--worlds" => args.worlds = Some(PathBuf::from(value()?)),
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
    // A cached pipeline blob skips shader compilation at start-up. The GPU crate rejects one made
    // by another GPU or driver, so a stale file only costs a cache miss.
    let cache_dir = args.cache.clone().unwrap_or_else(|| cwd.join(".specula-cache"));
    let cache_path = perf::pipeline_cache_path(&cache_dir, &args.adapter_label);
    let pipeline_cache = std::fs::read(&cache_path).ok();
    let gpu = Gpu::new(&GpuOptions { validation: Validation::Auto, adapter: args.adapter, pipeline_cache })
        .map_err(|e| format!("GPU init failed: {e}"))?;
    let captures_root = args.captures.unwrap_or_else(|| cwd.join("captures"));
    let assets = args.assets.unwrap_or_else(|| cwd.clone());
    let library = Library::new(&assets, args.style.as_deref());
    let mut engine = Engine::new(gpu, &captures_root, library)?;
    engine.windowed = !args.headless;
    engine.worlds_dir = args.worlds.unwrap_or_else(|| cwd.join("worlds"));
    if let Some(name) = args.world {
        // A world that exists but can't be read stops the engine: autosave would overwrite it.
        let dir = engine.worlds_dir.join(&name);
        // Owned for as long as this engine works in it: a second engine is refused, not interleaved.
        engine.world_lock = Some(persist::WorldLock::acquire(&dir).map_err(|e| format!("world '{name}': {e}"))?);
        if dir.join("world.json").exists() {
            let (world, saved) = persist::load(&dir).map_err(|e| format!("world '{name}' in {}: {e}", dir.display()))?;
            for problem in engine.replace_world(world, &name) {
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

    if let Some(g) = &game {
        engine.clock = Clock::new(g.tick_hz());
        engine.game_name = g.name().to_owned();
        // Its changes are attributed to this name: no agent may take it.
        engine.swarm.reserve(g.name());
    }
    let game_tools = game.as_ref().map(|g| g.tools()).unwrap_or_default();
    let (to_engine, requests) = mpsc::channel();
    let server = Server::start(
        ServerConfig {
            bind: format!("127.0.0.1:{}", args.port),
            server_name: "specula".into(),
            server_version: env!("CARGO_PKG_VERSION").into(),
            instructions: Some(tools::INSTRUCTIONS.into()),
        },
        Arc::new(tools::Tools::new(to_engine, game_tools)),
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
    // Loading is done: from here on, building a pipeline is a visible hitch and a budget error.
    engine.gpu.mark_warm();
    engine.game = game.take();
    engine.start_game();

    let result = match &window {
        Some(ui) => engine.run_windowed(ui, &requests, &server),
        None => engine.run_headless(&requests, &server),
    };
    engine.finish_autosave(true);
    engine.autosave(true);
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
    /// The window's LOD per model entity (kept for hysteresis).
    lod_levels: BTreeMap<u64, u8>,
    /// How models were drawn in the last window frame and the last capture.
    lod_window: LodStats,
    lod_capture: LodStats,
    /// The window frame's draw list (reused: no allocation per frame once warm).
    draws: Vec<DrawItem>,
    /// A human's window is open.
    windowed: bool,
    /// The world mesh epoch `lod_levels` was last pruned at.
    lod_epoch: u64,
    /// Calls waiting for model builds.
    build_waiters: Vec<BuildWaiter>,
    /// Captures waiting for models they show.
    capture_waiters: Vec<capture::CaptureWaiter>,
    /// Agents' named cameras (camera_set).
    cameras: cameras::Cameras,
    /// sight_compare's "before" captures.
    marks: compare::Marks,
    /// Font and glyph atlas for overlay text.
    labeler: sight_overlay::Labeler,
    /// When the model library last dropped what no entity uses, at which world mesh epoch.
    last_gc: (Instant, u64),
    /// When builds nobody waits for were last cancelled, at which `World::model_drops`.
    last_unwanted: (Instant, u64),
    world: World,
    /// Animation time zero.
    started: Instant,
    /// The human's window camera; `None` = auto-frame the world.
    window_camera: Option<Camera>,
    /// Offscreen targets by size, most recently used last; at most [`MAX_CACHED_TARGETS`].
    targets: Vec<((u32, u32), TargetId)>,
    frames_captured: u64,
    /// This run's folder: `<captures root>/run-<unix seconds>-<pid>`.
    capture_dir: PathBuf,
    /// Where world files live (`--worlds`, default `./worlds`).
    worlds_dir: PathBuf,
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
    /// When the clock was last advanced.
    last_advance: Instant,
    /// Window input gathered for the game each frame (reused).
    input: Vec<Event>,
    /// A window title set by the game (its HUD until the engine draws text).
    game_title: Option<String>,
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
        let started = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let capture_dir = captures_root.join(format!("run-{started}-{}", std::process::id()));
        let library = Arc::new(SharedLibrary::new(library));
        let mut world = World::default();
        world.set_recording(Recording::Full);
        world.set_model_source(Some(Arc::clone(&library) as Arc<dyn world::ModelSource>));
        Ok(Engine {
            gpu,
            meshes,
            lathes: LatheMeshes::default(),
            library,
            models: GpuModels::default(),
            lod_levels: BTreeMap::new(),
            lod_window: LodStats::default(),
            lod_capture: LodStats::default(),
            draws: Vec::new(),
            windowed: false,
            lod_epoch: 0,
            build_waiters: Vec::new(),
            capture_waiters: Vec::new(),
            cameras: cameras::Cameras::default(),
            marks: compare::Marks::default(),
            labeler: sight_overlay::Labeler::default(),
            last_gc: (Instant::now(), 0),
            last_unwanted: (Instant::now(), 0),
            world,
            started: Instant::now(),
            window_camera: None,
            targets: Vec::new(),
            frames_captured: 0,
            capture_dir,
            worlds_dir: PathBuf::from("worlds"),
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
            last_advance: Instant::now(),
            input: Vec::new(),
            game_title: None,
            swarm: Swarm::default(),
            tick_hashes: VecDeque::new(),
            hash_cache: None,
            world_generation: 0,
            last_claim_sweep: Instant::now(),
        })
    }

    /// Makes a loaded world the live one: it records changes, builds its models with the
    /// engine's library, and the journal starts over from it. Returns a message per model entity
    /// whose recipe failed (it loads anyway and draws nothing until fixed).
    fn replace_world(&mut self, mut world: World, name: &str) -> Vec<String> {
        world.set_recording(Recording::Full);
        let failed = world.set_model_source(Some(Arc::clone(&self.library) as Arc<dyn world::ModelSource>));
        let problems =
            failed.iter().map(|(id, e)| format!("model {} ({id}) not built: {e}", world.path_of(*id))).collect();
        self.swarm.journal.rebase(&world, &format!("world:{name}"));
        self.world = world;
        self.world_generation += 1;
        self.hash_cache = None;
        self.tick_hashes.clear();
        self.lod_levels.clear();
        problems
    }

    /// Journals what the hosted game just changed on its own (exempt from claims).
    fn journal_game(&mut self, kind: Kind) {
        self.swarm.game_changes(&mut self.world, self.clock.tick(), &self.game_name, kind);
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

    /// Records the hash as a tick begins.
    fn record_tick_hash(&mut self) {
        let h = self.state_hash();
        if self.tick_hashes.len() == TICK_HASHES {
            self.tick_hashes.pop_front();
        }
        self.tick_hashes.push_back((self.clock.tick(), h));
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
        game.start(&mut Ctx { world: &mut self.world, clock: &self.clock, agent: &self.game_name, windowed: self.windowed });
        self.journal_game(Kind::GameStart);
        self.last_advance = Instant::now();
    }

    /// Turns the real time since the last call into whole ticks and runs them.
    fn advance_clock(&mut self) {
        let now = Instant::now();
        let ticks = self.clock.advance(now - self.last_advance);
        self.last_advance = now;
        for _ in 0..ticks {
            if let Some(game) = self.game.as_mut() {
                let start = Instant::now();
                // Game updates are journaled replay-only and coalesced per tick (F3).
                self.world.set_recording(Recording::Snapshot);
                game.tick(&mut Ctx { world: &mut self.world, clock: &self.clock, agent: &self.game_name, windowed: self.windowed });
                self.world.set_recording(Recording::Full);
                self.journal_game(Kind::GameTick);
                self.pacing.tick.push(start.elapsed());
            }
            self.clock.finish_tick();
            self.record_tick_hash();
        }
    }

    /// The game's per-frame update: input, camera, title.
    fn game_frame(&mut self, ui: &UiThread, size: (u32, u32), dt: Duration) {
        self.input.clear();
        ui.drain_events(&mut self.input);
        let Some(game) = self.game.as_mut() else { return };
        let start = Instant::now();
        let mut frame = game::Frame {
            input: &self.input,
            size,
            alpha: self.clock.alpha(),
            dt_s: dt.as_secs_f32(),
            camera: &mut self.window_camera,
            title: &mut self.game_title,
        };
        self.world.set_recording(Recording::Snapshot);
        let ctx = &mut Ctx { world: &mut self.world, clock: &self.clock, agent: &self.game_name, windowed: self.windowed };
        game.frame(ctx, &mut frame);
        self.world.set_recording(Recording::Full);
        self.journal_game(Kind::GameFrame);
        self.pacing.game_frame.push(start.elapsed());
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
        let plan = persist::plan(&self.world, &self.worlds_dir.join(&name), self.saved.as_ref());
        let (tx, done) = mpsc::channel();
        let job = move || {
            let _ = tx.send(persist::execute(plan));
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
            None => Err(persist::PersistError {
                code: "io",
                message: "the autosave thread stopped without reporting".into(),
                file: None,
                entity: None,
                field: None,
            }),
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
        self.finish_autosave(false);
        if self.last_autosave.elapsed() >= AUTOSAVE_EVERY {
            self.autosave(false);
        }
    }

    /// Remembers old files a successful save couldn't remove (shown by engine_info).
    fn note_leftovers(&mut self, report: &persist::SaveReport) {
        if !report.leftovers.is_empty() {
            eprintln!("specula: saved, but could not remove old files {:?}; the next save retries", report.leftovers);
        }
        self.leftovers = report.leftovers.clone();
    }

    fn time_s(&self) -> f32 {
        self.started.elapsed().as_secs_f32()
    }

    /// Builds meshes for new or changed lathe profiles and models and frees unused ones — only
    /// when the world's meshes or the model library changed since the last call. Returns a
    /// message per failed upload (those entities aren't drawn).
    fn sync_meshes(&mut self) -> Vec<String> {
        let generation = self.world_generation;
        let mut errors = self.lathes.sync(&mut self.gpu, &self.world, generation);
        errors.extend(self.models.sync(&mut self.gpu, &self.library, &self.world, generation));
        if self.world.mesh_epoch() != self.lod_epoch {
            // Forget the LOD of entities that are gone.
            self.lod_epoch = self.world.mesh_epoch();
            let world = &self.world;
            self.lod_levels.retain(|id, _| world.get(*id).is_some());
        }
        errors
    }

    /// Rebuilds models whose recipe or style files changed (checked at most once a second;
    /// `force`: now). Returns the reload report when something was checked and changed.
    fn poll_assets(&mut self, force: bool) -> Option<models::Reload> {
        let reload = self.library.lock().poll(force);
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

    /// Answers every parked call: the engine is stopping.
    fn answer_waiters_shutting_down(&mut self) {
        let out = || ToolOutput::error("the engine is shutting down; the call was not completed");
        for w in self.capture_waiters.drain(..) {
            let _ = w.reply.send(out());
        }
        for w in self.build_waiters.drain(..) {
            let _ = w.reply.send(out());
        }
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
                let reload = self.poll_assets(true).unwrap_or_default();
                waiter.jobs = reload.jobs.clone();
                waiter.reload = reload;
            }
            _ => unreachable!("only model calls come here"),
        }
        self.build_waiters.push(waiter);
        self.pump_builds(None);
    }

    /// The draws for a frame through `camera` at `size`, into `out`: meshes synced first.
    #[allow(clippy::too_many_arguments)]
    fn build_draws(&mut self, camera: Camera, viewport_h: u32, include_ui: bool, lod: Option<usize>, window: bool, out: &mut Vec<DrawItem>) -> (LodStats, Vec<String>) {
        let errors = self.sync_meshes();
        let meshes = Meshes { shapes: &self.meshes, lathes: &self.lathes, models: &self.models };
        let lod = match (lod, window) {
            (Some(l), _) => LodChoice::Fixed(l),
            (None, true) => LodChoice::Remembered(&mut self.lod_levels),
            (None, false) => LodChoice::Fresh,
        };
        let mut view = View { camera, viewport_h, include_ui, lod };
        let stats = sight::draw_list(&self.world, &meshes, &mut view, out);
        (stats, errors)
    }

    fn run_windowed(&mut self, ui: &UiThread, requests: &Receiver<Request>, server: &Server) -> Result<(), String> {
        let mut title = String::new();
        let mut size = client_size_of(ui.raw);
        let mut last_frame = Instant::now();
        while !interrupt::requested() && !ui.close_requested() {
            while let Ok(request) = requests.try_recv() {
                self.handle(request, server);
            }
            self.autosave_if_due();
            self.sweep_claims(server);
            self.poll_assets(false);
            self.pump_builds(None);
            self.serve_captures();
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
            if size.0 == 0 || size.1 == 0 {
                // Minimized: keep serving agents without a frame to present.
                std::thread::sleep(Duration::from_millis(16));
                continue;
            }
            let camera = self.window_camera.unwrap_or_else(|| sight::auto_camera(&self.world, 50.0));
            let mut draws = std::mem::take(&mut self.draws);
            let started = Instant::now();
            let (stats, errors) = self.build_draws(camera, size.1, true, None, true, &mut draws);
            self.pacing.draw_list.push(started.elapsed());
            for e in errors {
                eprintln!("specula: {e}");
            }
            self.lod_window = stats;
            let (lights, _) = sight::lights(&self.world, true);
            let view_proj = camera.view_proj(size.0, size.1);
            let frame = sight::frame(&self.world, &view_proj, &draws, &lights, self.time_s());
            // FIFO present paces the loop to the display refresh.
            let presented = self.gpu.present(&frame).map_err(|e| format!("present: {e}"));
            self.draws = draws;
            presented?;
            self.pacing.frame.push(dt);
        }
        Ok(())
    }

    fn run_headless(&mut self, requests: &Receiver<Request>, server: &Server) -> Result<(), String> {
        while !interrupt::requested() {
            // Wake for the next tick at the latest, so a hosted game keeps real time headless too.
            let mut wait = self.clock.until_next().min(Duration::from_millis(100));
            if self.library.lock().in_flight() > 0 || !self.capture_waiters.is_empty() {
                // Builds land promptly (and calls waiting on them are answered).
                wait = wait.min(Duration::from_millis(5));
            }
            match requests.recv_timeout(wait) {
                Ok(request) => self.handle(request, server),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
            self.advance_clock();
            self.autosave_if_due();
            self.sweep_claims(server);
            self.poll_assets(false);
            self.pump_builds(None);
            self.serve_captures();
        }
        Ok(())
    }

    fn handle(&mut self, request: Request, server: &Server) {
        if Instant::now() > request.deadline {
            // The caller is about to give up; applying the command now would surprise it.
            let _ = request.reply.send(ToolOutput::error("skipped: the engine was busy too long; the command was not applied"));
            return;
        }
        if matches!(request.command, Command::ModelBuild(_) | Command::ModelReload) {
            self.handle_model_call(request);
            return;
        }
        if matches!(request.command, Command::Frame(_) | Command::Sight(_)) {
            self.handle_sight(request);
            return;
        }
        let by = self.swarm.agent_name(&request.session);
        // Ownership (claims, undo) goes by identity key, which a resumed agent keeps.
        let session = self.swarm.key(&request.session);
        let w = Writer::new(&by, Some(&session));
        let tick = self.clock.tick();
        let output = match request.command {
            Command::Spawn(spec) => match self.swarm.commit(&mut self.world, tick, w, Kind::Spawn, |wd| wd.spawn(spec, &by).map(|e| e.id)) {
                Ok((id, seq)) => self.entity_output(id, seq),
                Err(e) => world_error(&e),
            },
            Command::Set(key, change) => {
                match self.swarm.commit(&mut self.world, tick, w, Kind::Set, |wd| wd.update(&key, change, &by).map(|e| e.id)) {
                    Ok((id, seq)) => self.entity_output(id, seq),
                    Err(e) => world_error(&e),
                }
            }
            Command::Delete(key, expect) => match self.swarm.commit(&mut self.world, tick, w, Kind::Delete, |wd| wd.delete(&key, expect)) {
                Ok((d, seq)) => ToolOutput::text(
                    Json::obj([
                        ("deleted", Json::from(d.removed.len())),
                        ("id", Json::from(d.id)),
                        ("path", Json::from(d.path)),
                        ("journal", Json::from(seq)),
                    ])
                    .to_pretty(),
                ),
                Err(e) => world_error(&e),
            },
            Command::Query(spec) => self.query(&spec),
            Command::Batch(ops) => {
                let count = ops.len();
                let apply = |wd: &mut World| wd.apply_batch(ops, &by).map_err(|(i, e)| e.with("op_index", i));
                match self.swarm.commit(&mut self.world, tick, w, Kind::Batch, apply) {
                    Ok((results, seq)) => {
                        let mut summary = batch_summary(&results);
                        summary.set("journal", Json::from(seq));
                        ToolOutput::text(summary.to_string())
                    }
                    // A claim rejects the batch as a whole (no single op is at fault).
                    Err(e) if e.detail("op_index").is_none() => world_error(&e),
                    Err(mut e) => {
                        let index = e.detail("op_index").and_then(Json::as_u64).unwrap_or(0);
                        e.details.retain(|(k, _)| *k != "op_index");
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
            Command::Load { name, force } => match self.swarm.check_replace_world(w) {
                Ok(()) => self.load(name, force),
                Err(e) => world_error(&e),
            },
            Command::Hello { name, role, resume } => match self.swarm.hello(&request.session, name, role, resume, &server.all_sessions()) {
                Ok(reply) => ToolOutput::text(reply.to_pretty()),
                Err(e) => world_error(&e),
            },
            Command::Frame(_) | Command::Sight(_) => unreachable!("answered by handle_sight"),
            Command::Env(patch) => match self.swarm.commit(&mut self.world, tick, w, Kind::Env, |wd| wd.set_env(&patch)) {
                Ok(((), seq)) => {
                    let mut env = self.world.env().to_json();
                    env.set("journal", Json::from(seq));
                    ToolOutput::text(env.to_pretty())
                }
                Err(e) => world_error(&e),
            },
            Command::View(camera) => {
                self.window_camera = camera;
                ToolOutput::text(match camera {
                    Some(c) => Json::obj([("window_camera", c.to_json())]).to_pretty(),
                    None => "window camera: auto-framing the world".into(),
                })
            }
            Command::Info => self.info(&server.sessions()),
            Command::Perf { budget_ms } => self.perf(budget_ms),
            Command::Time(op) => {
                match op {
                    TimeOp::Pause => self.clock.set_paused(true),
                    TimeOp::Resume => self.clock.set_paused(false),
                    TimeOp::Step(n) => {
                        self.clock.set_paused(true);
                        self.clock.step(n);
                        // Run them now, so the reply shows the result.
                        self.advance_clock();
                    }
                    TimeOp::Scale(x) => self.clock.set_scale(x),
                    TimeOp::Info => {}
                }
                ToolOutput::text(self.clock.to_json().to_pretty())
            }
            Command::Game { name, args } => match self.game.as_mut() {
                Some(game) => {
                    // A claim can only reject the tool when claims exist: checkpoint only then.
                    let checkpoint = if self.swarm.claims.is_empty() { None } else { game.checkpoint() };
                    let (clock, agent, windowed) = (&self.clock, self.game_name.as_str(), self.windowed);
                    let call = |wd: &mut World| game.call_tool(&mut Ctx { world: wd, clock, agent, windowed }, &name, &args);
                    match self.swarm.game_tool(&mut self.world, tick, agent, w, call) {
                        Ok(out) => out,
                        Err(e) => {
                            // The world was rolled back; the game goes back with it.
                            match checkpoint {
                                Some(c) => game.restore(c),
                                None => {
                                    self.world.set_recording(Recording::Snapshot);
                                    game.world_rolled_back(&mut Ctx { world: &mut self.world, clock, agent, windowed });
                                    self.world.set_recording(Recording::Full);
                                    self.swarm.game_changes(&mut self.world, tick, agent, Kind::GameTool);
                                }
                            }
                            world_error(&e)
                        }
                    }
                }
                None => ToolOutput::error(format!("no game is running, so '{name}' is unavailable")),
            },
            Command::History(q) => ToolOutput::text(self.swarm.journal.history(&q).to_string()),
            Command::Changes(q) => ToolOutput::text(self.swarm.journal.changes_text(&q)),
            Command::Undo(req) => match self.swarm.undo(&mut self.world, tick, w, &req) {
                Ok(u) => ToolOutput::text(
                    Json::obj([
                        ("journal", Json::from(u.seq)),
                        ("undone", Json::arr(u.undone.iter().map(|s| Json::from(*s)))),
                        ("changes", Json::from(u.changes)),
                    ])
                    .to_string(),
                ),
                Err(e) => world_error(&e),
            },
            Command::Hash { recent, tick, verify } => self.hash_report(recent, tick, verify),
            Command::ClaimTake(spec) => match self.swarm.take_claim(&self.world, &request.session, spec) {
                Ok(c) => ToolOutput::text(c.to_string()),
                Err(e) => world_error(&e),
            },
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
                let list = self.swarm.claims.list(owner.as_deref(), path_prefix.as_deref(), after, limit, Instant::now());
                ToolOutput::text(list.to_string())
            }
            Command::ModelBuild(_) | Command::ModelReload => unreachable!("answered by handle_model_call"),
        };
        debug_assert!(self.world.pending_changes().is_empty(), "every world change is journaled");
        // The caller may have timed out; nothing to do then.
        let _ = request.reply.send(output);
    }

    fn entity_output(&self, id: u64, journal: Option<u64>) -> ToolOutput {
        let e = self.world.get(id).expect("just written");
        let mut out = e.to_json(&self.world.path_of(id));
        out.set("journal", Json::from(journal));
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
            ("journal_last_id", Json::from(self.swarm.journal.last_seq())),
            ("recent", Json::arr(self.tick_hashes.range(start..).map(pair))),
        ]);
        if let Some(t) = tick {
            let found = self.tick_hashes.iter().find(|(at, _)| *at == t).map(|(_, h)| journal::hex(*h));
            let oldest = self.tick_hashes.front().map(|(t, _)| *t);
            out.set("at_tick", Json::obj([("tick", Json::from(t)), ("hash", Json::from(found)), ("oldest_kept", Json::from(oldest))]));
        }
        if verify {
            let started = Instant::now();
            let report = match self.swarm.journal.replay() {
                Some(w) => {
                    let replayed = w.state_hash();
                    Json::obj([
                        ("replayable", Json::from(true)),
                        ("entries", Json::from(self.swarm.journal.len())),
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
                ToolOutput::text(
                    Json::obj([
                        ("saved", Json::from(name)),
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

    fn load(&mut self, name: String, force: bool) -> ToolOutput {
        self.finish_autosave(true);
        if self.dirty() && !force {
            let current = self.world_name.as_deref().map_or(Json::Null, Json::from);
            return ToolOutput::error(
                Json::obj([
                    ("error", Json::from("unsaved_changes")),
                    (
                        "message",
                        Json::from("the live world has unsaved changes; world_save first, or pass force: true to discard them"),
                    ),
                    ("current_world", current),
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
        match persist::load(&dir) {
            Ok((world, saved)) => {
                if lock.is_some() {
                    self.world_lock = lock;
                }
                let problems = self.replace_world(world, &name);
                self.saved = Some(saved);
                self.world_name = Some(name.clone());
                self.saved_revision = self.world.revision();
                self.last_autosave = Instant::now();
                ToolOutput::text(
                    Json::obj([
                        ("loaded", Json::from(name)),
                        ("dir", Json::from(dir.display().to_string())),
                        ("entities", Json::from(self.world.len())),
                        ("state_hash", Json::from(format!("{:016x}", self.world.state_hash()))),
                        ("warnings", Json::arr(problems.into_iter().take(20).map(Json::from))),
                    ])
                    .to_pretty(),
                )
            }
            Err(e) => ToolOutput::error(e.to_json().to_pretty()),
        }
    }

    /// The hosted game's `before_capture`: presentation up to the current tick (a headless engine
    /// never runs its per-frame update). Returns the tick it ran at (`None` without a game).
    fn before_capture(&mut self) -> Option<u64> {
        let game = self.game.as_mut()?;
        self.world.set_recording(Recording::Snapshot);
        let ctx = &mut Ctx { world: &mut self.world, clock: &self.clock, agent: &self.game_name, windowed: self.windowed };
        game.before_capture(ctx);
        self.world.set_recording(Recording::Full);
        self.journal_game(Kind::GameFrame);
        Some(self.clock.tick())
    }

    fn render_offscreen(&mut self, w: u32, h: u32, frame: &FrameDesc) -> Result<Vec<u8>, String> {
        let target = match self.targets.iter().position(|(size, _)| *size == (w, h)) {
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
                let t = self.gpu.create_target(w, h).map_err(|e| format!("create target: {e}"))?;
                self.targets.push(((w, h), t));
                t
            }
        };
        self.gpu.render(target, frame).map_err(|e| format!("render: {e}"))?;
        self.gpu.read_rgba8(target).map_err(|e| format!("readback: {e}"))
    }

    fn save_capture(&self, extension: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.capture_dir).map_err(|e| e.to_string())?;
        let path = self.capture_dir.join(format!("frame_{:05}.{extension}", self.frames_captured));
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(path)
    }

    fn perf(&self, budget_ms: Option<f64>) -> ToolOutput {
        let budget = budget_ms.unwrap_or(perf::DEFAULT_BUDGET_MS);
        let window = self.gpu.last_frame_stats_of(FramePath::Window);
        let offscreen = self.gpu.last_frame_stats_of(FramePath::Offscreen);
        let pacing = &self.pacing;
        let p95 = pacing.frame.to_json().get("p95_ms").and_then(Json::as_f64);
        let errors = match window.as_ref().or(offscreen.as_ref()) {
            Some(s) => perf::budget_errors(s, budget, p95),
            None => Vec::new(),
        };
        let out = Json::obj([
            ("budget_ms", Json::from(budget)),
            ("errors", Json::Arr(errors)),
            ("window", window.as_ref().map_or(Json::Null, perf::stats_json)),
            ("offscreen", offscreen.as_ref().map_or(Json::Null, perf::stats_json)),
            ("frame_pacing", pacing.frame.to_json()),
            ("sim_tick", pacing.tick.to_json()),
            ("game_frame", pacing.game_frame.to_json()),
            ("draw_list", pacing.draw_list.to_json()),
            ("clock", self.clock.to_json()),
            ("entities", Json::from(self.world.len())),
            ("transient_entities", Json::from(self.world.transient_len())),
            ("models", self.models_json()),
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
            ("clock", self.clock.to_json()),
            ("swarm", self.swarm.stats_json()),
            ("models", self.models_json()),
        ]);
        let mut info = info;
        if let Some(game) = &self.game {
            info.set("game", Json::obj([("name", Json::from(self.game_name.as_str())), ("status", game.status())]));
        }
        ToolOutput::text(info.to_pretty())
    }
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

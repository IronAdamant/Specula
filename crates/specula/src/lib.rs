//! Specula world server: owns the world, renders it, and serves agents over MCP.
//!
//! A library so game binaries can link the engine; `main.rs` is the stock `specula` binary.
//! [`world`] is public: games running in-process call [`world::World`] directly every tick.
//!
//! Threads: protocol threads parse tool calls and hand them to the engine (main) thread over a
//! channel; the engine thread owns the world and GPU, applies commands between frames, renders,
//! and replies. The human's window is pumped by its own thread (see `ui`), so dragging it never
//! stalls agents.

mod meshes;
pub mod persist;
mod sight;
mod tools;
mod ui;
pub mod world;

use meshes::ShapeMeshes;
use specula_core::deflate::Level;
use specula_core::{base64, png, Json};
use specula_gpu::{AdapterPreference, FrameDesc, Gpu, GpuOptions, Severity, TargetId, Validation};
use sight::{Camera, LatheMeshes};
use specula_platform::{client_size_of, WindowDesc};
use specula_protocol::{Content, Server, ServerConfig, SessionInfo, ToolOutput};
use specula_sys::interrupt;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tools::{Command, FrameSpec, QuerySpec, Request};
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
/// Agent identities remembered (sessions are capped at 1024 by the protocol).
const MAX_AGENTS: usize = 1024;
/// world_batch lists per-op results up to this many bytes, then summarises the rest.
const MAX_BATCH_RESULT_BYTES: usize = 32 * 1024;

const USAGE: &str = "usage: specula [--port N] [--headless] [--adapter discrete|integrated|<index>] [--size WxH] \
[--captures DIR] [--worlds DIR] [--world NAME]";

struct Args {
    port: u16,
    headless: bool,
    adapter: AdapterPreference,
    window_size: (u32, u32),
    captures: Option<PathBuf>,
    worlds: Option<PathBuf>,
    world: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        port: DEFAULT_PORT,
        headless: false,
        adapter: AdapterPreference::HighPerformance,
        window_size: (1280, 720),
        captures: None,
        worlds: None,
        world: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value\n{USAGE}"));
        match flag.as_str() {
            "--port" => args.port = value()?.parse().map_err(|_| format!("bad port\n{USAGE}"))?,
            "--headless" => args.headless = true,
            "--adapter" => {
                args.adapter = match value()?.as_str() {
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
            other => return Err(format!("unknown argument '{other}'\n{USAGE}")),
        }
    }
    Ok(args)
}

/// Runs the engine with the command-line arguments until it is closed or interrupted.
pub fn run() -> Result<(), String> {
    let args = parse_args()?;
    if let Err(e) = interrupt::install() {
        eprintln!("specula: Ctrl+C will not shut down cleanly: {e}");
    }
    let gpu = Gpu::new(&GpuOptions { validation: Validation::Auto, adapter: args.adapter })
        .map_err(|e| format!("GPU init failed: {e}"))?;
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let captures_root = args.captures.unwrap_or_else(|| cwd.join("captures"));
    let mut engine = Engine::new(gpu, &captures_root)?;
    engine.worlds_dir = args.worlds.unwrap_or_else(|| cwd.join("worlds"));
    if let Some(name) = args.world {
        // A world that exists but can't be read stops the engine: autosave would overwrite it.
        let dir = engine.worlds_dir.join(&name);
        if dir.join("world.json").exists() {
            let (world, saved) = persist::load(&dir).map_err(|e| format!("world '{name}' in {}: {e}", dir.display()))?;
            engine.world = world;
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

    let (to_engine, requests) = mpsc::channel();
    let server = Server::start(
        ServerConfig {
            bind: format!("127.0.0.1:{}", args.port),
            server_name: "specula".into(),
            server_version: env!("CARGO_PKG_VERSION").into(),
            instructions: Some(tools::INSTRUCTIONS.into()),
        },
        Arc::new(tools::Tools::new(to_engine)),
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

    let result = match &window {
        Some(ui) => engine.run_windowed(ui, &requests, &server),
        None => engine.run_headless(&requests, &server),
    };
    engine.finish_autosave(true);
    engine.autosave(true);
    // Queued calls get "shutting down" as soon as their reply channels close; give them a moment
    // to write that response before the server closes every connection.
    drop(requests);
    std::thread::sleep(Duration::from_millis(100));
    server.shutdown();
    // Tear the GPU down (window surface included) before the window itself, and report any
    // validation messages raised during shutdown.
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
    /// Meshes built from lathe profiles, by entity id.
    lathes: LatheMeshes,
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
    /// An autosave being written on a worker thread.
    autosave_job: Option<AutosaveJob>,
    /// The last autosave failure (cleared by the next success), shown in engine_info.
    autosave_error: Option<Json>,
    /// Session id → identity given with agent_hello.
    agents: BTreeMap<String, Agent>,
}

struct AutosaveJob {
    /// The world revision the job is writing.
    revision: u64,
    done: Receiver<Result<(persist::Saved, persist::SaveReport), persist::PersistError>>,
}

struct Agent {
    name: String,
    role: String,
}

impl Engine {
    fn new(mut gpu: Gpu, captures_root: &Path) -> Result<Engine, String> {
        let meshes = ShapeMeshes::upload(&mut gpu).map_err(|e| format!("mesh upload: {e}"))?;
        let started = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let capture_dir = captures_root.join(format!("run-{started}-{}", std::process::id()));
        Ok(Engine {
            gpu,
            meshes,
            lathes: LatheMeshes::new(),
            world: World::default(),
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
            autosave_job: None,
            autosave_error: None,
            agents: BTreeMap::new(),
        })
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
            Ok((saved, _)) => {
                self.saved = Some(saved);
                self.saved_revision = revision;
                self.autosave_error = None;
            }
            Err(e) => {
                eprintln!("specula: autosave failed: {e}");
                let mut j = e.to_json();
                j.set("at_revision", Json::from(revision));
                self.autosave_error = Some(j);
            }
        }
    }

    fn autosave_if_due(&mut self) {
        self.finish_autosave(false);
        if self.last_autosave.elapsed() >= AUTOSAVE_EVERY {
            self.autosave(false);
        }
    }

    /// Who a session is in attribution: its agent_hello name, else its MCP client name.
    fn agent_name(&self, session: &SessionInfo) -> String {
        match self.agents.get(&session.id) {
            Some(a) => a.name.clone(),
            None if session.client_name.is_empty() => "anonymous".into(),
            None => session.client_name.clone(),
        }
    }
    fn time_s(&self) -> f32 {
        self.started.elapsed().as_secs_f32()
    }

    /// Build meshes for new or changed lathe profiles and free those of deleted entities.
    /// Returns a message per failed upload (that entity is not drawn).
    fn sync_lathes(&mut self) -> Vec<String> {
        let mut errors = Vec::new();
        let gpu = &mut self.gpu;
        for e in self.world.entities() {
            let Some(profile) = &e.profile else { continue };
            let key = profile.key();
            if self.lathes.get(&e.id).is_some_and(|&(k, _)| k == key) {
                continue;
            }
            if let Some((_, old)) = self.lathes.remove(&e.id) {
                gpu.destroy_mesh(old);
            }
            let (vertices, indices) = meshes::lathe(&profile.points, profile.segments);
            match gpu.upload_mesh(&vertices, &indices) {
                Ok(mesh) => {
                    self.lathes.insert(e.id, (key, mesh));
                }
                Err(err) => errors.push(format!("lathe '{}' not drawn: {err}", e.name)),
            }
        }
        let live: std::collections::BTreeSet<u64> =
            self.world.entities().filter(|e| e.profile.is_some()).map(|e| e.id).collect();
        let stale: Vec<u64> = self.lathes.keys().copied().filter(|id| !live.contains(id)).collect();
        for id in stale {
            if let Some((_, mesh)) = self.lathes.remove(&id) {
                gpu.destroy_mesh(mesh);
            }
        }
        errors
    }

    fn run_windowed(&mut self, ui: &UiThread, requests: &Receiver<Request>, server: &Server) -> Result<(), String> {
        let mut title = String::new();
        let mut size = client_size_of(ui.raw);
        while !interrupt::requested() && !ui.close_requested() {
            while let Ok(request) = requests.try_recv() {
                self.handle(request, server);
            }
            self.autosave_if_due();
            let next = format!("Specula — {} entities — {} agents", self.world.len(), server.sessions().len());
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
            for e in self.sync_lathes() {
                eprintln!("specula: {e}");
            }
            let camera = self.window_camera.unwrap_or_else(|| sight::auto_camera(&self.world, 50.0));
            let draws = sight::draw_list(&self.world, &self.meshes, &self.lathes);
            let (lights, _) = sight::lights(&self.world);
            let view_proj = camera.view_proj(size.0, size.1);
            let frame = sight::frame(&self.world, &view_proj, &draws, &lights, self.time_s());
            // FIFO present paces the loop to the display refresh.
            self.gpu.present(&frame).map_err(|e| format!("present: {e}"))?;
        }
        Ok(())
    }

    fn run_headless(&mut self, requests: &Receiver<Request>, server: &Server) -> Result<(), String> {
        while !interrupt::requested() {
            match requests.recv_timeout(Duration::from_millis(100)) {
                Ok(request) => self.handle(request, server),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
            self.autosave_if_due();
        }
        Ok(())
    }

    fn handle(&mut self, request: Request, server: &Server) {
        if Instant::now() > request.deadline {
            // The caller is about to give up; applying the command now would surprise it.
            let _ = request.reply.send(ToolOutput::error("skipped: the engine was busy too long; the command was not applied"));
            return;
        }
        let by = self.agent_name(&request.session);
        let output = match request.command {
            Command::Spawn(spec) => match self.world.spawn(spec, &by).map(|e| e.id) {
                Ok(id) => self.entity_output(id),
                Err(e) => world_error(&e),
            },
            Command::Set(key, change) => match self.world.update(&key, change, &by).map(|e| e.id) {
                Ok(id) => self.entity_output(id),
                Err(e) => world_error(&e),
            },
            Command::Delete(key, expect) => match self.world.delete(&key, expect) {
                Ok(d) => ToolOutput::text(
                    Json::obj([("deleted", Json::from(d.removed.len())), ("id", Json::from(d.id)), ("path", Json::from(d.path))])
                        .to_pretty(),
                ),
                Err(e) => world_error(&e),
            },
            Command::Query(spec) => self.query(&spec),
            Command::Batch(ops) => {
                let count = ops.len();
                match self.world.apply_batch(ops, &by) {
                    Ok(results) => ToolOutput::text(batch_summary(&results).to_string()),
                    Err((index, e)) => ToolOutput::error(
                        Json::obj([
                            ("error", Json::from("batch_failed")),
                            (
                                "message",
                                Json::from(format!("ops[{index}] failed, so none of the {count} ops were applied: {}", e.message)),
                            ),
                            ("op_index", Json::from(index)),
                            ("cause", e.to_json()),
                        ])
                        .to_pretty(),
                    ),
                }
            }
            Command::Save { name, overwrite } => self.save(name, overwrite),
            Command::Load { name, force } => self.load(name, force),
            Command::Hello { name, role } => {
                if self.agents.len() >= MAX_AGENTS && !self.agents.contains_key(&request.session.id) {
                    let live: Vec<String> = server.sessions().into_iter().map(|s| s.id).collect();
                    self.agents.retain(|id, _| live.contains(id));
                }
                let reply = Json::obj([("agent", Json::from(name.as_str())), ("role", Json::from(role.as_str()))]);
                self.agents.insert(request.session.id.clone(), Agent { name, role });
                ToolOutput::text(reply.to_pretty())
            }
            Command::Frame(spec) => self.capture(&spec, &request.session),
            Command::Env(patch) => match self.world.set_env(&patch) {
                Ok(()) => ToolOutput::text(self.world.env().to_json().to_pretty()),
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
        };
        // The caller may have timed out; nothing to do then.
        let _ = request.reply.send(output);
    }

    fn entity_output(&self, id: u64) -> ToolOutput {
        let e = self.world.get(id).expect("just written");
        ToolOutput::text(e.to_json(&self.world.path_of(id)).to_pretty())
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
            // Another world lives there: never replace it by accident.
            let (entities, problem) = match persist::load(&dir) {
                Ok((w, _)) => (Json::from(w.len()), Json::Null),
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
        match persist::save(&self.world, &dir, self.saved.as_ref()) {
            Ok((saved, report)) => {
                self.saved = Some(saved);
                self.world_name = Some(name.clone());
                self.saved_revision = self.world.revision();
                self.last_autosave = Instant::now();
                self.autosave_error = None;
                ToolOutput::text(
                    Json::obj([
                        ("saved", Json::from(name)),
                        ("dir", Json::from(report.dir.display().to_string())),
                        ("generation", Json::from(report.generation)),
                        ("files", Json::from(report.files)),
                        ("files_written", Json::from(report.written)),
                        ("files_removed", Json::from(report.removed)),
                        ("entities", Json::from(self.world.len())),
                        ("state_hash", Json::from(format!("{:016x}", self.world.state_hash()))),
                    ])
                    .to_pretty(),
                )
            }
            Err(e) => ToolOutput::error(e.to_json().to_pretty()),
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
        match persist::load(&dir) {
            Ok((world, saved)) => {
                self.world = world;
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
                    ])
                    .to_pretty(),
                )
            }
            Err(e) => ToolOutput::error(e.to_json().to_pretty()),
        }
    }

    fn capture(&mut self, spec: &FrameSpec, session: &SessionInfo) -> ToolOutput {
        let camera = match (spec.eye, spec.target) {
            (Some(eye), Some(target)) => sight::Camera { eye, target, fov_y_deg: spec.fov_y_deg },
            _ => sight::auto_camera(&self.world, spec.fov_y_deg),
        };
        let (w, h) = (spec.width, spec.height);
        let view_proj = camera.view_proj(w, h);
        let mut warnings: Vec<Json> = self.sync_lathes().into_iter().map(Json::from).collect();
        let draws = sight::draw_list(&self.world, &self.meshes, &self.lathes);
        let (lights, dropped) = sight::lights(&self.world);
        if dropped > 0 {
            warnings.push(Json::from(format!(
                "{dropped} point light(s) not rendered: at most {} per frame",
                specula_gpu::MAX_POINT_LIGHTS
            )));
        }
        let time_s = spec.time_s.unwrap_or_else(|| self.time_s());
        let frame = sight::frame(&self.world, &view_proj, &draws, &lights, time_s);

        let rgba = match self.render_offscreen(w, h, &frame) {
            Ok(rgba) => rgba,
            Err(e) => return ToolOutput::error(e),
        };
        let png_bytes = png::encode_rgba8(w, h, &rgba, Level::Default);
        self.frames_captured += 1;
        let png_path = self.save_capture("png", &png_bytes);

        for m in self.gpu.take_validation_messages() {
            if m.severity >= Severity::Warning {
                warnings.push(Json::from(format!("vulkan {:?} {}: {}", m.severity, m.id_name, m.message)));
            }
        }
        if let Err(e) = &png_path {
            warnings.push(Json::from(format!("capture not saved to disk: {e}")));
        }
        // The full packet is saved next to its PNG so a verifier can re-check both later.
        let png_file = png_path.as_ref().ok();
        let packet_path = png_file.map(|p| p.with_extension("json"));
        let show = |p: Option<&PathBuf>| -> Json { p.map(|p| p.display().to_string()).into() };
        let image = Json::obj([
            ("width", Json::from(w)),
            ("height", Json::from(h)),
            ("png_bytes", Json::from(png_bytes.len())),
            ("path", show(png_file)),
            ("packet_path", show(packet_path.as_ref())),
        ]);
        let subjects = sight::subjects(&self.world, &view_proj, w, h);
        let on_screen = subjects.iter().filter(|s| s.on_screen).count();
        let (chosen, omitted) = sight::most_relevant(&subjects, spec.max_subjects);
        let mut packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("frame", Json::from(self.frames_captured)),
            ("requested_by", Json::from(self.agent_name(session))),
            ("image", image),
            ("camera", camera.to_json()),
            ("time_s", Json::from(time_s)),
            ("lights", Json::from(lights.len())),
            ("subjects_total", Json::from(subjects.len())),
            ("subjects_on_screen", Json::from(on_screen)),
            ("subjects", Json::arr(subjects.iter().map(|s| s.json.clone()))),
            ("warnings", Json::Arr(warnings.clone())),
        ]);
        if let Some(path) = &packet_path {
            if let Err(e) = std::fs::write(path, packet.to_pretty()) {
                warnings.push(Json::from(format!("packet not saved to disk: {e}")));
                packet.set("warnings", Json::Arr(warnings));
            }
        }
        // The response carries the most relevant subjects only; agents' context is precious.
        packet.set("subjects", Json::Arr(chosen));
        packet.set("subjects_omitted", Json::from(omitted));
        ToolOutput {
            content: vec![
                Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) },
                Content::Text(packet.to_pretty()),
            ],
            is_error: false,
        }
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

    fn info(&mut self, sessions: &[SessionInfo]) -> ToolOutput {
        let a = self.gpu.adapter();
        let gpu = Json::obj([
            ("name", Json::from(a.name.as_str())),
            ("kind", Json::from(format!("{:?}", a.kind))),
            ("vulkan", Json::from(format!("{}.{}.{}", a.api_version.0, a.api_version.1, a.api_version.2))),
        ]);
        let agents = sessions.iter().map(|s| {
            let role = self.agents.get(&s.id).map_or("builder", |a| a.role.as_str());
            Json::obj([
                ("name", Json::from(self.agent_name(s))),
                ("role", Json::from(role)),
                ("client", Json::from(s.client_name.as_str())),
                ("version", Json::from(s.client_version.as_str())),
            ])
        });
        let world = Json::obj([
            ("name", self.world_name.as_deref().map_or(Json::Null, Json::from)),
            ("worlds_dir", Json::from(self.worlds_dir.display().to_string())),
            ("entities", Json::from(self.world.len())),
            ("unsaved_changes", Json::from(self.dirty())),
            ("last_autosave_error", self.autosave_error.clone().unwrap_or(Json::Null)),
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
        ]);
        ToolOutput::text(info.to_pretty())
    }
}

/// world_batch's answer: every op's result while they fit in [`MAX_BATCH_RESULT_BYTES`], then a
/// summary of the rest (spawned ids are consecutive, so first and last describe them).
fn batch_summary(results: &[world::OpResult]) -> Json {
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
    let mut out = vec![("applied", Json::from(results.len())), ("results", Json::Arr(Vec::new()))];
    let rest = &results[shown.len()..];
    out[1].1 = Json::Arr(shown);
    if !rest.is_empty() {
        let id = |r: &world::OpResult| match r {
            world::OpResult::Spawned { id, .. } | world::OpResult::Updated { id, .. } | world::OpResult::Deleted { id, .. } => *id,
        };
        out.extend([
            ("results_omitted", Json::from(rest.len())),
            ("first_id", Json::from(id(&rest[0]))),
            ("last_id", Json::from(id(&rest[rest.len() - 1]))),
        ]);
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
        let out = batch_summary(&results);
        assert!(out.to_string().len() <= MAX_BATCH_RESULT_BYTES + 200, "{} bytes", out.to_string().len());
        let shown = out.get("results").and_then(Json::as_array).unwrap().len();
        assert_eq!(out.get("applied"), Some(&Json::from(1000)));
        assert_eq!(out.get("results_omitted"), Some(&Json::from(1000 - shown)));
        assert_eq!(out.get("first_id"), Some(&Json::from(shown as u64 + 1)));
        assert_eq!(out.get("last_id"), Some(&Json::from(1000)));
        let small = batch_summary(&results[..3]);
        assert_eq!((small.get("results").and_then(Json::as_array).unwrap().len(), small.get("results_omitted")), (3, None));
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

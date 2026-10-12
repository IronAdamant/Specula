//! Specula world server: owns the world, renders it, and serves agents over MCP.
//!
//! Threads: protocol threads parse tool calls and hand them to the engine (main) thread over a
//! channel; the engine thread owns the world and GPU, applies commands between frames, renders,
//! and replies. The human's window is pumped by its own thread (see `ui`), so dragging it never
//! stalls agents.

mod meshes;
mod sight;
mod tools;
mod ui;
mod world;

use meshes::ShapeMeshes;
use specula_core::deflate::Level;
use specula_core::{base64, png, Json};
use specula_gpu::{AdapterPreference, FrameDesc, Gpu, GpuOptions, Severity, TargetId, Validation};
use specula_platform::{client_size_of, WindowDesc};
use specula_protocol::{Content, Server, ServerConfig, SessionInfo, ToolOutput};
use specula_sys::interrupt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tools::{Command, FrameSpec, Request};
use ui::UiThread;
use world::World;

const DEFAULT_PORT: u16 = 7471;

/// Offscreen render targets kept for reuse (one per capture size, least recently used evicted).
const MAX_CACHED_TARGETS: usize = 4;
/// Capture runs kept in the captures directory (older `run-*` folders are deleted at start-up).
const MAX_CAPTURE_RUNS: usize = 20;

const USAGE: &str = "usage: specula [--port N] [--headless] [--adapter discrete|integrated|<index>] [--size WxH] \
[--captures DIR]";

struct Args {
    port: u16,
    headless: bool,
    adapter: AdapterPreference,
    window_size: (u32, u32),
    captures: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        port: DEFAULT_PORT,
        headless: false,
        adapter: AdapterPreference::HighPerformance,
        window_size: (1280, 720),
        captures: None,
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
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument '{other}'\n{USAGE}")),
        }
    }
    Ok(args)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("specula: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    if let Err(e) = interrupt::install() {
        eprintln!("specula: Ctrl+C will not shut down cleanly: {e}");
    }
    let gpu = Gpu::new(&GpuOptions { validation: Validation::Auto, adapter: args.adapter })
        .map_err(|e| format!("GPU init failed: {e}"))?;
    let captures_root = match args.captures {
        Some(dir) => dir,
        None => std::env::current_dir().map_err(|e| e.to_string())?.join("captures"),
    };
    let mut engine = Engine::new(gpu, &captures_root)?;
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
    world: World,
    /// Offscreen targets by size, most recently used last; at most [`MAX_CACHED_TARGETS`].
    targets: Vec<((u32, u32), TargetId)>,
    frames_captured: u64,
    /// This run's folder: `<captures root>/run-<unix seconds>-<pid>`.
    capture_dir: PathBuf,
}

impl Engine {
    fn new(mut gpu: Gpu, captures_root: &Path) -> Result<Engine, String> {
        let meshes = ShapeMeshes::upload(&mut gpu).map_err(|e| format!("mesh upload: {e}"))?;
        let started = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let capture_dir = captures_root.join(format!("run-{started}-{}", std::process::id()));
        Ok(Engine { gpu, meshes, world: World::default(), targets: Vec::new(), frames_captured: 0, capture_dir })
    }

    fn run_windowed(&mut self, ui: &UiThread, requests: &Receiver<Request>, server: &Server) -> Result<(), String> {
        let mut title = String::new();
        let mut size = client_size_of(ui.raw);
        while !interrupt::requested() && !ui.close_requested() {
            while let Ok(request) = requests.try_recv() {
                self.handle(request, server);
            }
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
            let camera = sight::auto_camera(&self.world, 50.0);
            let draws = sight::draw_list(&self.world, &self.meshes);
            let frame = FrameDesc {
                view_proj: camera.view_proj(size.0, size.1).to_cols_array(),
                clear_color: sight::CLEAR_COLOR,
                draws: &draws,
                reverse_z: true,
            };
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
        }
        Ok(())
    }

    fn handle(&mut self, request: Request, server: &Server) {
        if Instant::now() > request.deadline {
            // The caller is about to give up; applying the command now would surprise it.
            let _ = request.reply.send(ToolOutput::error("skipped: the engine was busy too long; the command was not applied"));
            return;
        }
        let creator = &request.session.client_name;
        let output = match request.command {
            Command::Spawn(spec) => match self.world.spawn(spec, creator) {
                Ok(entity) => ToolOutput::text(entity.to_json().to_pretty()),
                Err(e) => ToolOutput::error(e),
            },
            Command::Set(key, change) => match self.world.update(&key, change) {
                Ok(entity) => ToolOutput::text(entity.to_json().to_pretty()),
                Err(e) => ToolOutput::error(e),
            },
            Command::List => {
                let list = Json::arr(self.world.entities().iter().map(|e| e.to_json()));
                ToolOutput::text(Json::obj([("entities", list)]).to_pretty())
            }
            Command::Delete(key) => match self.world.delete(&key) {
                Some(e) => ToolOutput::text(format!("deleted {} (id {})", e.name, e.id)),
                None => ToolOutput::error("no such entity"),
            },
            Command::Frame(spec) => self.capture(&spec, &request.session),
            Command::Info => self.info(&server.sessions()),
        };
        // The caller may have timed out; nothing to do then.
        let _ = request.reply.send(output);
    }

    fn capture(&mut self, spec: &FrameSpec, session: &SessionInfo) -> ToolOutput {
        let camera = match (spec.eye, spec.target) {
            (Some(eye), Some(target)) => sight::Camera { eye, target, fov_y_deg: spec.fov_y_deg },
            _ => sight::auto_camera(&self.world, spec.fov_y_deg),
        };
        let (w, h) = (spec.width, spec.height);
        let view_proj = camera.view_proj(w, h);
        let draws = sight::draw_list(&self.world, &self.meshes);
        let frame =
            FrameDesc { view_proj: view_proj.to_cols_array(), clear_color: sight::CLEAR_COLOR, draws: &draws, reverse_z: true };

        let rgba = match self.render_offscreen(w, h, &frame) {
            Ok(rgba) => rgba,
            Err(e) => return ToolOutput::error(e),
        };
        let png_bytes = png::encode_rgba8(w, h, &rgba, Level::Default);
        self.frames_captured += 1;
        let png_path = self.save_capture("png", &png_bytes);

        let mut warnings = Vec::new();
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
            ("requested_by", Json::from(session.client_name.as_str())),
            ("image", image),
            ("camera", camera.to_json()),
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
            Json::obj([("client", Json::from(s.client_name.as_str())), ("version", Json::from(s.client_version.as_str()))])
        });
        let info = Json::obj([
            ("engine", Json::from(format!("specula {}", env!("CARGO_PKG_VERSION")))),
            ("gpu", gpu),
            ("validation", Json::from(self.gpu.validation_enabled())),
            ("entities", Json::from(self.world.len())),
            ("frames_captured", Json::from(self.frames_captured)),
            ("captures", Json::from(self.capture_dir.display().to_string())),
            ("agents", Json::arr(agents)),
        ]);
        ToolOutput::text(info.to_pretty())
    }
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

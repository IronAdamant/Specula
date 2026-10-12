//! Specula world server: owns the world, renders it, and serves agents over MCP.
//!
//! Threads: protocol threads parse tool calls and hand them to the main thread over a channel; the
//! main thread owns the world, GPU and window, applies commands between frames, and replies.

mod meshes;
mod sight;
mod tools;
mod world;

use meshes::ShapeMeshes;
use specula_core::deflate::Level;
use specula_core::{base64, png, Json};
use specula_gpu::{AdapterPreference, FrameDesc, Gpu, GpuOptions, Severity, TargetId, Validation};
use specula_platform::{Event, Window, WindowDesc};
use specula_protocol::{Content, Server, ServerConfig, SessionInfo, ToolOutput};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;
use tools::{Command, FrameSpec, Request};
use world::World;

const DEFAULT_PORT: u16 = 7471;

const USAGE: &str = "usage: specula [--port N] [--headless] [--adapter discrete|integrated|<index>] [--size WxH]";

struct Args {
    port: u16,
    headless: bool,
    adapter: AdapterPreference,
    window_size: (u32, u32),
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        port: DEFAULT_PORT,
        headless: false,
        adapter: AdapterPreference::HighPerformance,
        window_size: (1280, 720),
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
    let gpu = Gpu::new(&GpuOptions { validation: Validation::Auto, adapter: args.adapter })
        .map_err(|e| format!("GPU init failed: {e}"))?;
    let mut engine = Engine::new(gpu)?;
    eprintln!(
        "specula: GPU {} (validation {})",
        engine.gpu.adapter().name,
        if engine.gpu.validation_enabled() { "on" } else { "off" }
    );

    let mut window = if args.headless {
        None
    } else {
        let (width, height) = args.window_size;
        let window = Window::new(&WindowDesc { title: "Specula".into(), width, height, ..WindowDesc::default() })
            .map_err(|e| format!("window: {e}"))?;
        let raw = window.raw();
        let (w, h) = window.client_size();
        engine.gpu.attach_window(raw.hinstance, raw.hwnd, w, h).map_err(|e| format!("swapchain: {e}"))?;
        Some(window)
    };

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
    eprintln!("specula: MCP at http://{}/mcp", server.local_addr());

    let result = match window.as_mut() {
        Some(window) => engine.run_windowed(window, &requests, &server),
        None => engine.run_headless(&requests, &server),
    };
    // Drop queued requests first: their reply channels close, so in-flight tool calls return at
    // once instead of holding up shutdown until their 30 s timeout.
    drop(requests);
    server.shutdown();
    result
}

struct Engine {
    gpu: Gpu,
    meshes: ShapeMeshes,
    world: World,
    /// Offscreen targets reused per capture size.
    targets: HashMap<(u32, u32), TargetId>,
    frames_captured: u64,
    capture_dir: PathBuf,
}

impl Engine {
    fn new(mut gpu: Gpu) -> Result<Engine, String> {
        let meshes = ShapeMeshes::upload(&mut gpu).map_err(|e| format!("mesh upload: {e}"))?;
        let capture_dir = std::env::current_dir().map_err(|e| e.to_string())?.join("captures");
        Ok(Engine { gpu, meshes, world: World::default(), targets: HashMap::new(), frames_captured: 0, capture_dir })
    }

    fn run_windowed(&mut self, window: &mut Window, requests: &Receiver<Request>, server: &Server) -> Result<(), String> {
        let mut title = String::new();
        loop {
            for event in window.poll_events() {
                match event {
                    Event::CloseRequested => return Ok(()),
                    Event::Resized { width, height } => {
                        self.gpu.resize_window(width, height).map_err(|e| format!("resize: {e}"))?;
                    }
                    _ => {}
                }
            }
            while let Ok(request) = requests.try_recv() {
                self.handle(request, server);
            }

            let next = format!("Specula — {} entities — {} agents", self.world.len(), server.sessions().len());
            if next != title {
                window.set_title(&next);
                title = next;
            }

            let (w, h) = window.client_size();
            if w == 0 || h == 0 {
                std::thread::sleep(Duration::from_millis(16));
                continue;
            }
            let camera = sight::auto_camera(&self.world, 50.0);
            let draws = sight::draw_list(&self.world, &self.meshes);
            let frame = FrameDesc {
                view_proj: camera.view_proj(w, h).to_cols_array(),
                clear_color: sight::CLEAR_COLOR,
                draws: &draws,
            };
            // FIFO present paces the loop to the display refresh.
            self.gpu.present(&frame).map_err(|e| format!("present: {e}"))?;
        }
    }

    fn run_headless(&mut self, requests: &Receiver<Request>, server: &Server) -> Result<(), String> {
        loop {
            match requests.recv_timeout(Duration::from_millis(100)) {
                Ok(request) => self.handle(request, server),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    }

    fn handle(&mut self, request: Request, server: &Server) {
        let output = match request.command {
            Command::Spawn(spec) => match self.world.spawn(spec, &request.session.client_name) {
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
        let frame = FrameDesc { view_proj: view_proj.to_cols_array(), clear_color: sight::CLEAR_COLOR, draws: &draws };

        let rgba = match self.render_offscreen(w, h, &frame) {
            Ok(rgba) => rgba,
            Err(e) => return ToolOutput::error(e),
        };
        let png_bytes = png::encode_rgba8(w, h, &rgba, Level::Default);
        self.frames_captured += 1;
        let path = self.save_capture(&png_bytes);

        let mut warnings = Vec::new();
        for m in self.gpu.take_validation_messages() {
            if m.severity >= Severity::Warning {
                warnings.push(Json::from(format!("vulkan {:?} {}: {}", m.severity, m.id_name, m.message)));
            }
        }
        if let Err(e) = &path {
            warnings.push(Json::from(format!("capture not saved to disk: {e}")));
        }
        let image = Json::obj([
            ("width", Json::from(w)),
            ("height", Json::from(h)),
            ("png_bytes", Json::from(png_bytes.len())),
            ("path", path.ok().map(|p| p.display().to_string()).into()),
        ]);
        let packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("frame", Json::from(self.frames_captured)),
            ("requested_by", Json::from(session.client_name.as_str())),
            ("image", image),
            ("camera", camera.to_json()),
            ("subjects", Json::arr(sight::subjects(&self.world, &view_proj, w, h))),
            ("warnings", Json::Arr(warnings)),
        ]);
        ToolOutput {
            content: vec![
                Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) },
                Content::Text(packet.to_pretty()),
            ],
            is_error: false,
        }
    }

    fn render_offscreen(&mut self, w: u32, h: u32, frame: &FrameDesc) -> Result<Vec<u8>, String> {
        let target = match self.targets.get(&(w, h)) {
            Some(t) => *t,
            None => {
                let t = self.gpu.create_target(w, h).map_err(|e| format!("create target: {e}"))?;
                self.targets.insert((w, h), t);
                t
            }
        };
        self.gpu.render(target, frame).map_err(|e| format!("render: {e}"))?;
        self.gpu.read_rgba8(target).map_err(|e| format!("readback: {e}"))
    }

    fn save_capture(&self, png_bytes: &[u8]) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.capture_dir).map_err(|e| e.to_string())?;
        let path = self.capture_dir.join(format!("frame_{:05}.png", self.frames_captured));
        std::fs::write(&path, png_bytes).map_err(|e| e.to_string())?;
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
            ("agents", Json::arr(agents)),
        ]);
        ToolOutput::text(info.to_pretty())
    }
}

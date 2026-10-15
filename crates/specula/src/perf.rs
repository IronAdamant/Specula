//! Performance agents can see (foundation F3): `perf_frame` reports the renderer's own statistics
//! (GPU time per pass, draws, memory, pipelines) plus frame pacing and simulation cost measured
//! here, and turns budget misses into errors an agent can act on. Budgets come from `perf.toml`
//! (see `perf.example.toml` at the repository root).
//!
//! The CPU side comes from [`specula_core::profile`] zones around the engine loop's phases:
//! `perf_zones` shows the zone tree, `perf_memory` the heap (with the counting allocator), and
//! `perf_capture` records a run of frames to a file plus a Chrome trace-event timeline.

use specula_core::profile::{self, Collector, FrameProfile, RollingZone};
use specula_core::{alloc, Json};
use specula_gpu::{CacheRejection, FramePath, FrameStats, Gpu, GraphInfo, MemoryKind, PipelineCacheLoad};
use specula_protocol::ToolOutput;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

/// Frames of history for pacing statistics (about two seconds at 60 Hz).
const HISTORY: usize = 120;

/// Default frame budget: 60 fps on the reference iGPU.
pub const DEFAULT_BUDGET_MS: f64 = 1000.0 / 60.0;

/// Budgets file read from the working directory when `--perf` isn't given.
pub const DEFAULT_CONFIG_FILE: &str = "perf.toml";

/// The engine thread's name in zone reports.
pub const ENGINE_THREAD: &str = "main";

/// Frames one perf_capture may record.
pub const MAX_CAPTURE_FRAMES: u64 = 600;

/// Rolling window of durations, in milliseconds.
#[derive(Default)]
pub struct Series {
    samples: VecDeque<f64>,
}

impl Series {
    pub fn push(&mut self, d: Duration) {
        if self.samples.len() == HISTORY {
            self.samples.pop_front();
        }
        self.samples.push_back(d.as_secs_f64() * 1000.0);
    }

    pub fn to_json(&self) -> Json {
        match Stats::of(self.samples.iter().copied()) {
            None => Json::Null,
            Some(s) => Json::obj([
                ("samples", Json::from(s.n)),
                ("mean_ms", ms(s.mean)),
                ("median_ms", ms(s.p50)),
                ("p95_ms", ms(s.p95)),
                ("max_ms", ms(s.max)),
            ]),
        }
    }

    pub fn p95(&self) -> Option<f64> {
        Stats::of(self.samples.iter().copied()).map(|s| s.p95)
    }
}

/// Summary statistics of a list of numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub n: usize,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

impl Stats {
    pub fn of(values: impl Iterator<Item = f64>) -> Option<Stats> {
        let mut sorted: Vec<f64> = values.collect();
        if sorted.is_empty() {
            return None;
        }
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        let pick = |q: f64| sorted[((q * (n - 1) as f64).round() as usize).min(n - 1)];
        Some(Stats { n, mean: sorted.iter().sum::<f64>() / n as f64, p50: pick(0.5), p95: pick(0.95), max: sorted[n - 1] })
    }

    /// `{mean, p95, max}`, each rounded by `round`.
    fn json(&self, round: fn(f64) -> Json) -> Json {
        Json::obj([("mean", round(self.mean)), ("p95", round(self.p95)), ("max", round(self.max))])
    }
}

/// Timings the engine loop collects.
#[derive(Default)]
pub struct Pacing {
    /// Window frame to window frame (what the player feels).
    pub frame: Series,
    /// One simulation tick of the hosted game.
    pub tick: Series,
    /// The game's per-frame presentation update.
    pub game_frame: Series,
    /// Building the window frame's draw list (mesh sync, model LOD choice).
    pub draw_list: Series,
}

fn ms(v: f64) -> Json {
    Json::from((v * 1000.0).round() / 1000.0)
}

fn one_decimal(v: f64) -> Json {
    Json::from((v * 10.0).round() / 10.0)
}

pub fn path_name(p: FramePath) -> &'static str {
    match p {
        FramePath::Window => "window",
        FramePath::Offscreen => "offscreen",
    }
}

fn kind_name(k: MemoryKind) -> &'static str {
    match k {
        MemoryKind::Meshes => "meshes",
        MemoryKind::RenderTargets => "render_targets",
        MemoryKind::Readback => "readback",
        MemoryKind::PerFrame => "per_frame",
        MemoryKind::Staging => "staging",
        MemoryKind::ShadowMaps => "shadow_maps",
        MemoryKind::PostTargets => "post_targets",
        MemoryKind::Textures => "textures",
    }
}

fn cache_json(c: PipelineCacheLoad) -> Json {
    match c {
        PipelineCacheLoad::NotProvided => Json::from("not_provided"),
        PipelineCacheLoad::Loaded { bytes } => Json::obj([("loaded_bytes", Json::from(bytes))]),
        PipelineCacheLoad::Rejected(r) => Json::obj([(
            "rejected",
            Json::from(match r {
                CacheRejection::TooShort { .. } => "too_short",
                CacheRejection::BadHeader { .. } => "bad_header",
                CacheRejection::OtherVendor { .. } => "other_vendor",
                CacheRejection::OtherDevice { .. } => "other_device",
                CacheRejection::OtherDriver => "other_driver",
                CacheRejection::Driver { .. } => "driver",
            }),
        )]),
    }
}

/// One frame's statistics as compact JSON.
/// The last frame's render graph for `perf_frame`: a summary, and with `detail` every pass with
/// the resources it uses and the barriers recorded before it (bounded: a frame has a few dozen
/// passes and resources at most). `null` before the first frame.
pub fn graph_json(info: Option<&GraphInfo>, detail: bool) -> Json {
    let Some(g) = info else { return Json::Null };
    let mut out = Json::obj([
        ("passes", Json::from(g.passes.len())),
        ("resources", Json::from(g.resources.len())),
        ("barriers", Json::from(g.barrier_count())),
        ("compiles", Json::from(g.compiles)),
        ("pass_names", Json::arr(g.passes.iter().map(|p| Json::from(p.name)))),
    ]);
    if detail {
        let passes = g.passes.iter().map(|p| {
            let uses = p.uses.iter().map(|u| {
                Json::obj([
                    ("resource", Json::from(u.resource)),
                    ("access", Json::from(format!("{:?}", u.access))),
                    ("load", Json::from(u.load_op)),
                    ("store", Json::from(u.store_op)),
                ])
            });
            Json::obj([
                ("name", Json::from(p.name)),
                ("inline", Json::from(p.inline)),
                ("uses", Json::arr(uses)),
                ("barriers", Json::arr(p.barriers.iter().map(|b| Json::from(b.to_string())))),
            ])
        });
        let resources = g.resources.iter().map(|r| {
            Json::obj([
                ("name", Json::from(r.name)),
                ("kind", Json::from(r.kind)),
                ("size", Json::arr([Json::from(r.width), Json::from(r.height), Json::from(r.layers)])),
                ("format", Json::from(r.format)),
                ("bytes", Json::from(r.bytes)),
            ])
        });
        out.set("pass_list", Json::arr(passes));
        out.set("resource_list", Json::arr(resources));
        out.set("final_barriers", Json::arr(g.final_barriers.iter().map(|b| Json::from(b.to_string()))));
    }
    out
}

pub fn stats_json(s: &FrameStats) -> Json {
    let gpu = match &s.gpu {
        Some(g) => Json::obj([
            ("frame", Json::from(g.frame)),
            ("total_ms", ms(g.total_ms)),
            ("passes", Json::obj(g.passes.iter().map(|p| (p.name, ms(p.gpu_ms))))),
        ]),
        None => Json::Null,
    };
    let d = &s.draws;
    let m = &s.memory;
    let heaps = m.heaps.iter().map(|h| {
        Json::obj([
            ("index", Json::from(h.index)),
            ("device_local", Json::from(h.device_local)),
            ("size_mb", mb(h.size)),
            ("budget_mb", h.budget.map(mb).unwrap_or(Json::Null)),
            ("usage_mb", h.usage.map(mb).unwrap_or(Json::Null)),
        ])
    });
    let mut out = Json::obj([
        ("frame", Json::from(s.frame)),
        ("path", Json::from(path_name(s.path))),
        ("size", Json::arr([Json::from(s.width), Json::from(s.height)])),
        ("gpu", gpu),
        (
            "cpu",
            Json::obj([
                ("wait_ms", ms(s.cpu.wait_ms)),
                ("record_ms", ms(s.cpu.record_ms)),
                ("submit_ms", ms(s.cpu.submit_ms)),
            ]),
        ),
        (
            "draws",
            Json::obj([
                ("submitted", Json::from(d.submitted)),
                ("culled", Json::from(d.culled)),
                ("draw_calls", Json::from(d.draw_calls)),
                ("instances", Json::from(d.instances)),
                ("triangles", Json::from(d.triangles)),
                ("pipeline_binds", Json::from(d.pipeline_binds)),
                ("unbatched_draw_calls", Json::from(d.unbatched_draw_calls)),
            ]),
        ),
        (
            "pipelines",
            Json::obj([
                ("created", Json::from(s.pipelines.created)),
                ("created_after_warm", Json::from(s.pipelines.created_after_warm)),
                ("warm", Json::from(s.pipelines.warm)),
                ("create_ms", ms(s.pipelines.create_ms)),
                ("created_background", Json::from(s.pipelines.created_background)),
                ("background_ms", ms(s.pipelines.background_ms)),
                ("background_pending", Json::from(s.pipelines.background_pending)),
                ("background_failed", Json::from(s.pipelines.background_failed)),
                ("cache", cache_json(s.pipelines.cache)),
            ]),
        ),
        (
            "memory",
            Json::obj([
                ("total_mb", mb(m.total_bytes)),
                ("allocations", Json::from(m.allocations)),
                ("by_kind_mb", Json::obj(m.by_kind.iter().map(|(k, b)| (kind_name(*k), mb(*b))))),
                ("draw_buffer_kb", Json::from((s.draw_buffer_bytes as f64 / 1024.0).round())),
                ("heaps", Json::arr(heaps)),
            ]),
        ),
    ]);
    if !s.timestamps_supported {
        out.set("gpu_timing", Json::from("unsupported on this GPU"));
    }
    if let Some(code) = s.timestamp_error {
        out.set("timestamp_error", Json::from(i64::from(code)));
    }
    out
}

fn mb(bytes: u64) -> Json {
    Json::from((bytes as f64 / 1_048_576.0 * 10.0).round() / 10.0)
}

fn bytes_mb(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}

// ---------------------------------------------------------------------------------------------
// Budgets: perf.toml

/// Frame budgets for one adapter (see `perf.example.toml`). Every miss is a structured error.
#[derive(Clone, Debug, PartialEq)]
pub struct Budgets {
    pub frame_ms: f64,
    /// GPU time of a frame (first pass start to last pass end).
    pub gpu_ms: f64,
    /// Engine-thread frame time excluding `present` (which waits for vsync), p95.
    pub cpu_frame_ms: f64,
    /// Window frame-to-frame time, p95.
    pub frame_pacing_p95_ms: f64,
    pub draw_calls: Option<u64>,
    pub triangles: Option<u64>,
    pub gpu_memory_mb: Option<f64>,
    /// Heap in use (needs the counting allocator).
    pub heap_mb: Option<f64>,
    /// Heap allocations per frame, whole process, p95.
    pub allocations_per_frame: Option<u64>,
    /// One simulation tick, p95.
    pub sim_tick_ms: Option<f64>,
    /// The game's per-frame update, p95.
    pub game_frame_ms: Option<f64>,
    /// Pipelines built after warm-up (each one is a visible hitch).
    pub pipelines_after_warm: u64,
    /// GPU ms per pass, by pass name.
    pub gpu_passes: Vec<(String, f64)>,
    /// CPU ms (p95 per frame) per zone: `(thread, path, ms)`.
    pub zones: Vec<(String, String, f64)>,
}

impl Default for Budgets {
    fn default() -> Budgets {
        Budgets::for_frame(DEFAULT_BUDGET_MS)
    }
}

impl Budgets {
    fn for_frame(frame_ms: f64) -> Budgets {
        Budgets {
            frame_ms,
            gpu_ms: frame_ms,
            cpu_frame_ms: frame_ms,
            frame_pacing_p95_ms: frame_ms * 1.25,
            draw_calls: None,
            triangles: None,
            gpu_memory_mb: None,
            heap_mb: None,
            allocations_per_frame: None,
            sim_tick_ms: None,
            game_frame_ms: None,
            pipelines_after_warm: 0,
            gpu_passes: Vec::new(),
            zones: Vec::new(),
        }
    }

    /// perf_frame's `budget_ms`: checks frame, GPU, CPU and pacing against this frame time.
    pub fn with_frame(mut self, frame_ms: f64) -> Budgets {
        self.frame_ms = frame_ms;
        self.gpu_ms = frame_ms;
        self.cpu_frame_ms = frame_ms;
        self.frame_pacing_p95_ms = frame_ms * 1.25;
        self
    }

    /// Reads a merged `[budgets]` table.
    fn from_json(table: &Json, section: &str) -> Result<Budgets, String> {
        let pairs = table.as_object().ok_or_else(|| format!("[{section}] must be a table"))?;
        let num = |key: &str, v: &Json, max: f64| -> Result<f64, String> {
            match v.as_f64() {
                Some(x) if x.is_finite() && x > 0.0 && x <= max => Ok(x),
                _ => Err(format!("[{section}] {key} must be a number above 0 and at most {max}")),
            }
        };
        let int = |key: &str, v: &Json| -> Result<u64, String> {
            v.as_u64().ok_or_else(|| format!("[{section}] {key} must be a whole number >= 0"))
        };
        let frame_ms = match table.get("frame_ms") {
            Some(v) => num("frame_ms", v, 10_000.0)?,
            None => DEFAULT_BUDGET_MS,
        };
        let mut b = Budgets::for_frame(frame_ms);
        for (key, v) in pairs {
            match key.as_str() {
                "frame_ms" => {}
                "gpu_ms" => b.gpu_ms = num(key, v, 10_000.0)?,
                "cpu_frame_ms" => b.cpu_frame_ms = num(key, v, 10_000.0)?,
                "frame_pacing_p95_ms" => b.frame_pacing_p95_ms = num(key, v, 10_000.0)?,
                "draw_calls" => b.draw_calls = Some(int(key, v)?),
                "triangles" => b.triangles = Some(int(key, v)?),
                "gpu_memory_mb" => b.gpu_memory_mb = Some(num(key, v, 1e7)?),
                "heap_mb" => b.heap_mb = Some(num(key, v, 1e7)?),
                "allocations_per_frame" => b.allocations_per_frame = Some(int(key, v)?),
                "sim_tick_ms" => b.sim_tick_ms = Some(num(key, v, 10_000.0)?),
                "game_frame_ms" => b.game_frame_ms = Some(num(key, v, 10_000.0)?),
                "pipelines_after_warm" => b.pipelines_after_warm = int(key, v)?,
                "gpu_passes" => {
                    let passes = v.as_object().ok_or_else(|| format!("[{section}.gpu_passes] must be a table"))?;
                    for (pass, ms) in passes {
                        b.gpu_passes.push((pass.clone(), num(&format!("gpu_passes.{pass}"), ms, 10_000.0)?));
                    }
                }
                "zones" => {
                    let zones = v.as_object().ok_or_else(|| format!("[{section}.zones] must be a table"))?;
                    for (key, ms) in zones {
                        let (thread, path) = match key.split_once(':') {
                            Some((t, p)) => (t, p),
                            None => (ENGINE_THREAD, key.as_str()),
                        };
                        if thread.is_empty() || path.is_empty() || path.split('/').any(str::is_empty) {
                            return Err(format!(
                                "[{section}.zones] '{key}' must be a zone path like \"frame/sim\" (optionally \"thread:path\")"
                            ));
                        }
                        b.zones.push((thread.to_owned(), path.to_owned(), num(&format!("zones.\"{key}\""), ms, 10_000.0)?));
                    }
                }
                other => {
                    return Err(format!("unknown budget '{other}' in [{section}] (known: {})", BUDGET_KEYS.join(", ")));
                }
            }
        }
        Ok(b)
    }

    pub fn to_json(&self) -> Json {
        let opt_u = |v: Option<u64>| v.map_or(Json::Null, Json::from);
        let opt_f = |v: Option<f64>| v.map_or(Json::Null, ms);
        Json::obj([
            ("frame_ms", ms(self.frame_ms)),
            ("gpu_ms", ms(self.gpu_ms)),
            ("cpu_frame_ms", ms(self.cpu_frame_ms)),
            ("frame_pacing_p95_ms", ms(self.frame_pacing_p95_ms)),
            ("draw_calls", opt_u(self.draw_calls)),
            ("triangles", opt_u(self.triangles)),
            ("gpu_memory_mb", opt_f(self.gpu_memory_mb)),
            ("heap_mb", opt_f(self.heap_mb)),
            ("allocations_per_frame", opt_u(self.allocations_per_frame)),
            ("sim_tick_ms", opt_f(self.sim_tick_ms)),
            ("game_frame_ms", opt_f(self.game_frame_ms)),
            ("pipelines_after_warm", Json::from(self.pipelines_after_warm)),
            ("gpu_passes", Json::from(self.gpu_passes.len())),
            ("zones", Json::from(self.zones.len())),
        ])
    }
}

const BUDGET_KEYS: [&str; 14] = [
    "frame_ms",
    "gpu_ms",
    "cpu_frame_ms",
    "frame_pacing_p95_ms",
    "draw_calls",
    "triangles",
    "gpu_memory_mb",
    "heap_mb",
    "allocations_per_frame",
    "sim_tick_ms",
    "game_frame_ms",
    "pipelines_after_warm",
    "gpu_passes",
    "zones",
];

/// Adapter kinds a `[adapter.<kind>]` section may name.
const ADAPTER_KINDS: [&str; 5] = ["integrated", "discrete", "virtual", "cpu", "other"];

/// `perf.toml`: profiler settings and budgets, with per-adapter overrides.
#[derive(Clone, Debug, PartialEq)]
pub struct PerfConfig {
    /// The file the budgets came from (`None` = built-in defaults).
    pub source: Option<PathBuf>,
    /// `[profiler] enabled` (default true).
    pub profiler: bool,
    /// `[profiler] history_frames`: frames of zone history (default 120).
    pub history_frames: usize,
    base: Json,
    adapters: Vec<(String, Json)>,
}

impl Default for PerfConfig {
    fn default() -> PerfConfig {
        PerfConfig { source: None, profiler: true, history_frames: HISTORY, base: Json::obj::<&str>([]), adapters: Vec::new() }
    }
}

impl PerfConfig {
    /// Parses and checks a perf.toml (every adapter section included).
    pub fn parse(text: &str) -> Result<PerfConfig, String> {
        let doc = specula_core::toml::parse(text).map_err(|e| e.to_string())?;
        let mut cfg = PerfConfig::default();
        for (key, v) in doc.as_object().unwrap_or_default() {
            match key.as_str() {
                "profiler" => {
                    for (k, x) in v.as_object().ok_or("[profiler] must be a table")? {
                        match k.as_str() {
                            "enabled" => cfg.profiler = x.as_bool().ok_or("[profiler] enabled must be true or false")?,
                            "history_frames" => {
                                cfg.history_frames = x
                                    .as_u64()
                                    .filter(|n| (1..=1000).contains(n))
                                    .ok_or("[profiler] history_frames must be a whole number from 1 to 1000")?
                                    as usize;
                            }
                            other => return Err(format!("unknown key '{other}' in [profiler] (known: enabled, history_frames)")),
                        }
                    }
                }
                "budgets" => cfg.base = v.clone(),
                "adapter" => {
                    for (kind, table) in v.as_object().ok_or("[adapter] must hold tables like [adapter.integrated]")? {
                        if !ADAPTER_KINDS.contains(&kind.as_str()) {
                            return Err(format!("unknown adapter kind '{kind}' (known: {})", ADAPTER_KINDS.join(", ")));
                        }
                        cfg.adapters.push((kind.clone(), table.clone()));
                    }
                }
                other => return Err(format!("unknown section '{other}' (known: profiler, budgets, adapter)")),
            }
        }
        Budgets::from_json(&cfg.base, "budgets")?;
        for (kind, _) in &cfg.adapters {
            Budgets::from_json(&cfg.merged(kind), &format!("adapter.{kind}"))?;
        }
        Ok(cfg)
    }

    /// `--perf FILE` (must exist) or `./perf.toml` (optional).
    pub fn load(explicit: Option<&Path>, cwd: &Path) -> Result<PerfConfig, String> {
        let path = explicit.map_or_else(|| cwd.join(DEFAULT_CONFIG_FILE), Path::to_path_buf);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if explicit.is_none() && e.kind() == std::io::ErrorKind::NotFound => return Ok(PerfConfig::default()),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let mut cfg = PerfConfig::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        cfg.source = Some(path);
        Ok(cfg)
    }

    /// `[budgets]` with `[adapter.<kind>]` laid over it.
    fn merged(&self, kind: &str) -> Json {
        let mut out = self.base.clone();
        if let Some((_, over)) = self.adapters.iter().find(|(k, _)| k == kind) {
            for (key, v) in over.as_object().unwrap_or_default() {
                match (out.get(key), v) {
                    (Some(Json::Obj(_)), Json::Obj(inner)) => {
                        let mut table = out.get(key).cloned().unwrap_or(Json::Null);
                        for (k, x) in inner {
                            table.set(k.clone(), x.clone());
                        }
                        out.set(key.clone(), table);
                    }
                    _ => {
                        out.set(key.clone(), v.clone());
                    }
                }
            }
        }
        out
    }

    /// The budgets for an adapter kind (`integrated`, `discrete`, ...).
    pub fn budgets(&self, kind: &str) -> Budgets {
        Budgets::from_json(&self.merged(kind), "budgets").unwrap_or_default()
    }

    pub fn has_adapter_section(&self, kind: &str) -> bool {
        self.adapters.iter().any(|(k, _)| k == kind)
    }
}

/// The `[adapter.<kind>]` name of a GPU.
pub fn adapter_kind(kind: specula_gpu::AdapterKind) -> &'static str {
    match kind {
        specula_gpu::AdapterKind::Integrated => "integrated",
        specula_gpu::AdapterKind::Discrete => "discrete",
        specula_gpu::AdapterKind::Virtual => "virtual",
        specula_gpu::AdapterKind::Cpu => "cpu",
        specula_gpu::AdapterKind::Other => "other",
    }
}

/// What budgets are checked against.
#[derive(Default)]
pub struct Sample<'a> {
    /// The latest frame's renderer statistics.
    pub stats: Option<&'a FrameStats>,
    pub pacing_p95_ms: Option<f64>,
    pub cpu_frame_p95_ms: Option<f64>,
    pub tick_p95_ms: Option<f64>,
    pub game_frame_p95_ms: Option<f64>,
    /// Rolling zone statistics (p95 per frame is checked).
    pub zones: &'a [RollingZone],
    pub heap_bytes: Option<u64>,
    pub allocations_per_frame_p95: Option<f64>,
}

/// Budget misses as structured errors (empty = within budget). Each has `error` (the code),
/// `metric`, `value`, `budget`, `unit` and `adapter`.
pub fn evaluate(b: &Budgets, s: &Sample, adapter: &str) -> Vec<Json> {
    let mut errors = Vec::new();
    let mut check = |code: &str, metric: &str, value: f64, budget: f64, unit: &str| {
        if value > budget {
            let round: fn(f64) -> Json = if unit == "ms" { ms } else { one_decimal };
            errors.push(Json::obj([
                ("error", Json::from(code)),
                ("metric", Json::from(metric)),
                ("value", round(value)),
                ("budget", round(budget)),
                ("unit", Json::from(unit)),
                ("adapter", Json::from(adapter)),
            ]));
        }
    };
    if let Some(st) = s.stats {
        let after = f64::from(st.pipelines.created_after_warm);
        check("pipeline_created_after_warm", "pipelines_after_warm", after, b.pipelines_after_warm as f64, "count");
        if let Some(g) = &st.gpu {
            check("gpu_over_budget", "gpu_ms", g.total_ms, b.gpu_ms, "ms");
            for (pass, budget) in &b.gpu_passes {
                if let Some(p) = g.passes.iter().find(|p| p.name == pass) {
                    check("gpu_pass_over_budget", &format!("gpu_pass.{pass}"), p.gpu_ms, *budget, "ms");
                }
            }
        }
        if let Some(max) = b.draw_calls {
            check("draw_calls_over_budget", "draw_calls", f64::from(st.draws.draw_calls), max as f64, "count");
        }
        if let Some(max) = b.triangles {
            check("triangles_over_budget", "triangles", st.draws.triangles as f64, max as f64, "count");
        }
        if let Some(max) = b.gpu_memory_mb {
            check("gpu_memory_over_budget", "gpu_memory_mb", bytes_mb(st.memory.total_bytes), max, "MB");
        }
    }
    if let Some(p95) = s.pacing_p95_ms {
        // Present waits for vsync, so pacing above budget means frames are being missed.
        check("frame_pacing_over_budget", "frame_pacing_p95_ms", p95, b.frame_pacing_p95_ms, "ms");
    }
    if let Some(v) = s.cpu_frame_p95_ms {
        check("cpu_frame_over_budget", "cpu_frame_p95_ms", v, b.cpu_frame_ms, "ms");
    }
    if let (Some(v), Some(max)) = (s.tick_p95_ms, b.sim_tick_ms) {
        check("sim_tick_over_budget", "sim_tick_p95_ms", v, max, "ms");
    }
    if let (Some(v), Some(max)) = (s.game_frame_p95_ms, b.game_frame_ms) {
        check("game_frame_over_budget", "game_frame_p95_ms", v, max, "ms");
    }
    if let (Some(v), Some(max)) = (s.heap_bytes, b.heap_mb) {
        check("heap_over_budget", "heap_mb", bytes_mb(v), max, "MB");
    }
    if let (Some(v), Some(max)) = (s.allocations_per_frame_p95, b.allocations_per_frame) {
        check("allocations_over_budget", "allocations_per_frame_p95", v, max as f64, "count");
    }
    for (thread, path, budget) in &b.zones {
        if let Some(z) = s.zones.iter().find(|z| &z.thread == thread && &z.path == path) {
            let metric = if thread == ENGINE_THREAD { format!("zone.{path}") } else { format!("zone.{thread}:{path}") };
            check("zone_over_budget", &metric, z.p95_ms, *budget, "ms");
        }
    }
    errors
}

/// Engine-thread CPU time per frame without `present` (vsync and GPU waits), per frame.
fn cpu_frame_ms(fp: &FrameProfile) -> Option<f64> {
    let t = fp.thread(ENGINE_THREAD)?;
    let frame = t.find("frame")?;
    let present = t.find("frame/present").map_or(0, |z| z.total_ns);
    Some(frame.total_ns.saturating_sub(present) as f64 / 1e6)
}

// ---------------------------------------------------------------------------------------------
// The profiler as the engine runs it

/// perf_zones' arguments.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ZonesQuery {
    /// Only this thread (`"main"` is the engine thread).
    pub thread: Option<String>,
    /// Hide zones whose rolling p95 is below this.
    pub min_ms: f64,
    pub limit: usize,
    /// Rows to skip (from `next_cursor`).
    pub offset: usize,
    /// Turn recording on or off first.
    pub enabled: Option<bool>,
}

/// perf_capture's arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureSpec {
    pub frames: u64,
    /// Also write a Chrome trace-event timeline.
    pub trace: bool,
    /// Added to the file names.
    pub label: Option<String>,
}

/// A frame recorded by perf_capture.
struct CapturedFrame {
    profile: FrameProfile,
    gpu: Option<FrameStats>,
    /// Since the previous collect.
    dt_ms: f64,
}

struct PendingCapture {
    spec: CaptureSpec,
    reply: Sender<ToolOutput>,
    /// Reply by then whatever was recorded (the caller gives up soon after).
    deadline: Instant,
    started: Instant,
    dir: PathBuf,
    budgets: Budgets,
    adapter: String,
    frames: Vec<CapturedFrame>,
    /// Profiling was off when the capture began (switched back off at the end).
    was_disabled: bool,
}

/// The engine's CPU profiler: collects zones once per loop iteration and runs perf_capture.
pub struct Profiler {
    collector: Collector,
    frame: u64,
    last_collect: Instant,
    capture: Option<PendingCapture>,
    captures: u32,
}

impl Profiler {
    pub fn new(history_frames: usize) -> Profiler {
        Profiler { collector: Collector::new(history_frames), frame: 0, last_collect: Instant::now(), capture: None, captures: 0 }
    }

    pub fn capturing(&self) -> bool {
        self.capture.is_some()
    }

    pub fn latest(&self) -> Option<&FrameProfile> {
        self.collector.latest()
    }

    pub fn rolling(&self) -> Vec<RollingZone> {
        self.collector.rolling()
    }

    /// p95 of [`cpu_frame_ms`] over the history.
    pub fn cpu_frame_p95(&self) -> Option<f64> {
        Stats::of(self.collector.history().filter_map(cpu_frame_ms)).map(|s| s.p95)
    }

    /// p95 of allocations per frame over the history (when the counting allocator is in).
    pub fn allocations_p95(&self) -> Option<f64> {
        if !alloc::installed() {
            return None;
        }
        Stats::of(self.collector.history().map(|f| f.memory.allocations as f64)).map(|s| s.p95)
    }

    /// Ends a frame: collects every thread's zones, and records them for a capture in progress
    /// (replying once it has its frames or runs out of time).
    pub fn end_frame(&mut self, gpu: &Gpu) {
        self.end_frame_with(|| {
            gpu.last_frame_stats_of(FramePath::Window).or_else(|| gpu.last_frame_stats_of(FramePath::Offscreen))
        });
    }

    fn end_frame_with(&mut self, gpu_stats: impl FnOnce() -> Option<FrameStats>) {
        let now = Instant::now();
        let dt_ms = (now - self.last_collect).as_secs_f64() * 1000.0;
        self.last_collect = now;
        self.frame += 1;
        let fp = self.collector.collect(self.frame);
        let Some(cap) = &mut self.capture else { return };
        // The capture's own copies are not the frames' allocations.
        let before = alloc::thread_counts();
        cap.frames.push(CapturedFrame { profile: fp.clone(), gpu: gpu_stats(), dt_ms });
        let after = alloc::thread_counts();
        self.collector.discount_allocations(after.0 - before.0, after.1 - before.1);
        let done = cap.frames.len() as u64 >= cap.spec.frames;
        if done || now >= cap.deadline {
            let cap = self.capture.take().expect("checked above");
            self.captures += 1;
            finish_capture(cap, self.captures, (!done).then_some("time_limit"));
        }
    }

    /// The engine is stopping: a capture in progress answers now with the frames it has (and
    /// drops its reply channel), so neither its caller nor the server's connection thread waits
    /// out the timeout.
    pub fn abort_capture(&mut self) {
        if let Some(cap) = self.capture.take() {
            self.captures += 1;
            finish_capture(cap, self.captures, Some("shutting_down"));
        }
    }

    /// Starts perf_capture; the reply is sent when it finishes. Refused while one runs.
    pub fn start_capture(&mut self, spec: CaptureSpec, reply: Sender<ToolOutput>, deadline: Instant, ctx: CaptureContext) {
        if self.capture.is_some() {
            let _ = reply.send(ToolOutput::error(
                Json::obj([
                    ("error", Json::from("capture_in_progress")),
                    ("message", Json::from("another perf_capture is recording; try again when it has answered")),
                ])
                .to_pretty(),
            ));
            return;
        }
        let was_disabled = !profile::enabled();
        profile::set_enabled(true);
        if spec.trace {
            profile::set_tracing(true);
        }
        self.capture = Some(PendingCapture {
            spec,
            reply,
            // Leave time to write the files and answer before the caller's timeout.
            deadline: deadline.checked_sub(Duration::from_secs(2)).unwrap_or(deadline),
            started: Instant::now(),
            dir: ctx.dir,
            budgets: ctx.budgets,
            adapter: ctx.adapter,
            frames: Vec::new(),
            was_disabled,
        });
    }
}

/// Where and against what a capture is written and judged.
pub struct CaptureContext {
    pub dir: PathBuf,
    pub budgets: Budgets,
    pub adapter: String,
}

/// perf_frame's CPU section and the zone budget sample, from the profiler.
pub fn cpu_json(p: &Profiler) -> Json {
    let latest = p.latest();
    let engine = latest.and_then(|f| f.thread(ENGINE_THREAD));
    let frame_ms = engine.and_then(|t| t.find("frame")).map(|z| z.total_ms());
    Json::obj([
        ("profiler", Json::from(profile::enabled())),
        ("frame_zone_ms", frame_ms.map_or(Json::Null, ms)),
        ("cpu_frame_p95_ms", p.cpu_frame_p95().map_or(Json::Null, ms)),
        ("frames_collected", Json::from(p.frame)),
    ])
}

/// perf_zones' answer.
pub fn zones_json(p: &Profiler, q: &ZonesQuery) -> Json {
    let rolling = p.rolling();
    let latest = p.latest();
    let mut rows: Vec<&RollingZone> = rolling
        .iter()
        .filter(|z| q.thread.as_deref().is_none_or(|t| z.thread == t))
        .filter(|z| z.p95_ms >= q.min_ms)
        .collect();
    // The engine thread first, then threads by name; paths sort parents before children.
    rows.sort_by(|a, b| (a.thread != ENGINE_THREAD, &a.thread, &a.path).cmp(&(b.thread != ENGINE_THREAD, &b.thread, &b.path)));
    let total = rows.len();
    let page: Vec<Json> = rows
        .iter()
        .skip(q.offset)
        .take(q.limit)
        .map(|z| {
            let now = latest.and_then(|f| f.thread(&z.thread)).and_then(|t| t.find(&z.path));
            Json::obj([
                ("thread", Json::from(z.thread.as_str())),
                ("path", Json::from(z.path.as_str())),
                ("calls", now.map_or(Json::from(0), |n| Json::from(n.calls))),
                ("ms", now.map_or(Json::from(0), |n| ms(n.total_ms()))),
                ("self_ms", now.map_or(Json::from(0), |n| ms(n.self_ms()))),
                ("allocs", now.map_or(Json::from(0), |n| Json::from(n.self_alloc_count))),
                ("mean_ms", ms(z.mean_ms)),
                ("p95_ms", ms(z.p95_ms)),
                ("max_call_ms", ms(z.max_call_ms)),
                ("self_mean_ms", ms(z.self_mean_ms)),
                ("allocs_per_frame", one_decimal(z.self_allocs_mean)),
                ("frames", Json::from(z.frames)),
            ])
        })
        .collect();
    let end = q.offset + page.len();
    let dropped: u64 = latest.map_or(0, |f| f.threads.iter().map(|t| t.dropped).sum());
    Json::obj([
        ("enabled", Json::from(profile::enabled())),
        ("frame", Json::from(latest.map_or(0, |f| f.frame))),
        ("history_frames", Json::from(p.collector.history().len())),
        (
            "columns",
            Json::from(
                "calls/ms/self_ms/allocs: latest frame; mean_ms/p95_ms: per-frame total over the history; \
                 allocs = heap allocations outside child zones",
            ),
        ),
        ("total", Json::from(total)),
        ("zones", Json::Arr(page)),
        ("dropped_samples", Json::from(dropped)),
        ("next_cursor", if end < total { Json::from(end.to_string()) } else { Json::Null }),
    ])
}

/// perf_memory's answer.
pub fn memory_json(p: &Profiler, gpu: Option<&FrameStats>, limit: usize) -> Json {
    let s = alloc::stats();
    if !s.installed {
        return Json::obj([
            ("tracking", Json::from(false)),
            (
                "message",
                Json::from("this binary does not use specula_core::alloc::Counting as its global allocator, so heap use is unknown"),
            ),
            ("gpu", gpu.map_or(Json::Null, gpu_memory_json)),
        ]);
    }
    let history: Vec<&FrameProfile> = p.collector.history().collect();
    let allocs = Stats::of(history.iter().map(|f| f.memory.allocations as f64));
    let kb = Stats::of(history.iter().map(|f| f.memory.allocated_bytes as f64 / 1024.0));
    let mut zones = p.rolling();
    zones.sort_by(|a, b| b.self_alloc_bytes_mean.total_cmp(&a.self_alloc_bytes_mean));
    let top = zones.iter().filter(|z| z.self_allocs_mean > 0.0).take(limit).map(|z| {
        Json::obj([
            ("thread", Json::from(z.thread.as_str())),
            ("path", Json::from(z.path.as_str())),
            ("allocs_per_frame", one_decimal(z.self_allocs_mean)),
            ("alloc_kb_per_frame", one_decimal(z.self_alloc_bytes_mean / 1024.0)),
        ])
    });
    Json::obj([
        ("tracking", Json::from(true)),
        ("heap_mb", Json::from((bytes_mb(s.current_bytes) * 100.0).round() / 100.0)),
        ("peak_mb", Json::from((bytes_mb(s.peak_bytes) * 100.0).round() / 100.0)),
        ("allocations_total", Json::from(s.allocations)),
        ("frees_total", Json::from(s.frees)),
        ("allocated_mb_total", one_decimal(bytes_mb(s.allocated_bytes))),
        (
            "per_frame",
            Json::obj([
                ("frames", Json::from(history.len())),
                ("allocations", allocs.map_or(Json::Null, |a| a.json(one_decimal))),
                ("allocated_kb", kb.map_or(Json::Null, |k| k.json(one_decimal))),
            ]),
        ),
        ("top_zones", Json::arr(top)),
        ("note", Json::from("per-zone numbers count allocations made by the zone's own thread outside its child zones")),
        ("gpu", gpu.map_or(Json::Null, gpu_memory_json)),
    ])
}

fn gpu_memory_json(s: &FrameStats) -> Json {
    Json::obj([
        ("total_mb", mb(s.memory.total_bytes)),
        ("allocations", Json::from(s.memory.allocations)),
        ("by_kind_mb", Json::obj(s.memory.by_kind.iter().map(|(k, b)| (kind_name(*k), mb(*b))))),
    ])
}

/// Writes a finished capture and answers its caller.
fn finish_capture(cap: PendingCapture, n: u32, stopped_early: Option<&'static str>) {
    if cap.spec.trace {
        profile::set_tracing(false);
    }
    if cap.was_disabled {
        profile::set_enabled(false);
    }
    let label = cap.spec.label.as_deref().map(|l| format!("-{l}")).unwrap_or_default();
    let base = cap.dir.join(format!("perf-capture-{n:03}{label}"));
    let data_path = base.with_extension("json");
    let trace_path = base.with_extension("trace.json");
    let frames = &cap.frames;
    let mut warnings = Vec::new();
    let write = |path: &Path, text: String| -> Result<(), String> {
        std::fs::create_dir_all(&cap.dir).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    };
    if let Err(e) = write(&data_path, capture_json(&cap).to_string()) {
        warnings.push(Json::from(format!("capture not saved to {}: {e}", data_path.display())));
    }
    let mut trace_events = 0;
    if cap.spec.trace {
        let trace = chrome_trace(frames);
        trace_events = trace.get("traceEvents").and_then(Json::as_array).map_or(0, <[Json]>::len);
        if let Err(e) = write(&trace_path, trace.to_string()) {
            warnings.push(Json::from(format!("trace not saved to {}: {e}", trace_path.display())));
        }
    }
    let summary = capture_summary(&cap, stopped_early);
    let mut out = summary;
    out.set("path", Json::from(data_path.display().to_string()));
    out.set("trace_path", if cap.spec.trace { Json::from(trace_path.display().to_string()) } else { Json::Null });
    out.set("trace_events", Json::from(trace_events));
    out.set("warnings", Json::Arr(warnings));
    let _ = cap.reply.send(ToolOutput::text(out.to_string()));
}

/// A capture's numbers for the budget check.
struct CaptureParts {
    zones: Vec<RollingZone>,
    pacing: Option<f64>,
    cpu: Option<f64>,
    tick: Option<f64>,
    game: Option<f64>,
    allocs: Option<f64>,
}

fn capture_parts(frames: &[CapturedFrame]) -> CaptureParts {
    let zones = profile::rolling(frames.iter().map(|f| &f.profile));
    // Loop iterations are paced by the display only with a window (headless they are wake-ups).
    let windowed = frames.iter().any(|f| f.gpu.as_ref().is_some_and(|s| s.path == FramePath::Window));
    let pacing = Stats::of(frames.iter().skip(1).map(|f| f.dt_ms)).map(|s| s.p95).filter(|_| windowed);
    let cpu = Stats::of(frames.iter().filter_map(|f| cpu_frame_ms(&f.profile))).map(|s| s.p95);
    let engine = |f: &CapturedFrame, path: &str| f.profile.thread(ENGINE_THREAD).and_then(|t| t.find(path)).copied();
    let tick = Stats::of(frames.iter().filter_map(|f| engine(f, "frame/sim/tick")).map(|z| z.max_ms())).map(|s| s.p95);
    let game = Stats::of(frames.iter().filter_map(|f| engine(f, "frame/game_frame")).map(|z| z.total_ms())).map(|s| s.p95);
    let allocs = if alloc::installed() { Stats::of(frames.iter().map(|f| f.profile.memory.allocations as f64)).map(|s| s.p95) } else { None };
    CaptureParts { zones, pacing, cpu, tick, game, allocs }
}

fn capture_summary(cap: &PendingCapture, stopped_early: Option<&'static str>) -> Json {
    let frames = &cap.frames;
    let CaptureParts { zones, pacing, cpu, tick, game, allocs } = capture_parts(frames);
    let last = frames.last();
    let stats = capture_stats(frames);
    let heap = last.filter(|f| f.profile.memory.tracking).map(|f| f.profile.memory.current_bytes);
    let sample = Sample {
        stats: stats.as_ref(),
        pacing_p95_ms: pacing,
        cpu_frame_p95_ms: cpu,
        tick_p95_ms: tick,
        game_frame_p95_ms: game,
        zones: &zones,
        heap_bytes: heap,
        allocations_per_frame_p95: allocs,
    };
    let errors = evaluate(&cap.budgets, &sample, &cap.adapter);
    let dt = Stats::of(frames.iter().skip(1).map(|f| f.dt_ms));
    let gpu = Stats::of(frames.iter().filter_map(|f| f.gpu.as_ref()?.gpu.as_ref()).map(|g| g.total_ms));
    let passes = pass_times(frames);
    let draws = |f: fn(&FrameStats) -> f64| Stats::of(frames.iter().filter_map(|c| c.gpu.as_ref()).map(f));
    let draw_calls = draws(|s| f64::from(s.draws.draw_calls));
    let triangles = draws(|s| s.draws.triangles as f64);
    let gpu_memory = draws(|s| bytes_mb(s.memory.total_bytes));
    let mut top: Vec<&RollingZone> = zones.iter().collect();
    top.sort_by(|a, b| b.self_mean_ms.total_cmp(&a.self_mean_ms));
    let top = top.iter().take(10).map(|z| {
        Json::obj([
            ("thread", Json::from(z.thread.as_str())),
            ("path", Json::from(z.path.as_str())),
            ("self_mean_ms", ms(z.self_mean_ms)),
            ("mean_ms", ms(z.mean_ms)),
            ("p95_ms", ms(z.p95_ms)),
            ("max_call_ms", ms(z.max_call_ms)),
        ])
    });
    let dropped: u64 = frames.iter().flat_map(|f| f.profile.threads.iter().map(|t| t.dropped + t.trace_overwritten)).sum();
    let alloc_stats = Stats::of(frames.iter().map(|f| f.profile.memory.allocations as f64));
    Json::obj([
        ("frames", Json::from(frames.len())),
        ("requested_frames", Json::from(cap.spec.frames)),
        ("stopped_early", stopped_early.map_or(Json::Null, Json::from)),
        ("seconds", ms(cap.started.elapsed().as_secs_f64())),
        ("adapter", Json::from(cap.adapter.as_str())),
        ("frame_ms", dt.map_or(Json::Null, |s| s.json(ms))),
        ("gpu_ms", gpu.map_or(Json::Null, |s| s.json(ms))),
        (
            "gpu_passes_ms",
            Json::obj(passes.iter().filter_map(|(n, v)| Some((*n, Stats::of(v.iter().copied())?.json(ms))))),
        ),
        ("draw_calls", draw_calls.map_or(Json::Null, |s| s.json(one_decimal))),
        ("triangles", triangles.map_or(Json::Null, |s| s.json(one_decimal))),
        ("gpu_memory_mb", gpu_memory.map_or(Json::Null, |s| s.json(one_decimal))),
        ("top_zones_by_self_ms", Json::arr(top)),
        (
            "memory",
            Json::obj([
                ("tracking", Json::from(alloc::installed())),
                ("allocations_per_frame", alloc_stats.map_or(Json::Null, |s| s.json(one_decimal))),
                ("heap_mb_end", heap.map_or(Json::Null, |b| one_decimal(bytes_mb(b)))),
            ]),
        ),
        ("dropped_samples", Json::from(dropped)),
        (
            "budgets_checked_on",
            Json::from("p95 over the captured frames (GPU memory and pipelines built after warm-up: their maximum)"),
        ),
        ("budget_errors", Json::Arr(errors)),
    ])
}

/// GPU ms per pass over a capture's frames, in pass order.
fn pass_times(frames: &[CapturedFrame]) -> Vec<(&'static str, Vec<f64>)> {
    let mut passes: Vec<(&'static str, Vec<f64>)> = Vec::new();
    for g in frames.iter().filter_map(|f| f.gpu.as_ref()?.gpu.as_ref()) {
        for p in &g.passes {
            match passes.iter_mut().find(|(n, _)| *n == p.name) {
                Some((_, v)) => v.push(p.gpu_ms),
                None => passes.push((p.name, vec![p.gpu_ms])),
            }
        }
    }
    passes
}

/// A capture's renderer statistics for the budget check: the p95 over its frames of GPU time,
/// each pass, draw calls and triangles, and the maximum of GPU memory and pipelines built after
/// warm-up (the rest from the last frame).
fn capture_stats(frames: &[CapturedFrame]) -> Option<FrameStats> {
    let all: Vec<&FrameStats> = frames.iter().filter_map(|f| f.gpu.as_ref()).collect();
    let mut out = (*all.last()?).clone();
    let p95 = |f: &dyn Fn(&FrameStats) -> f64| Stats::of(all.iter().map(|s| f(s))).map_or(0.0, |s| s.p95);
    out.draws.draw_calls = p95(&|s| f64::from(s.draws.draw_calls)).round() as u32;
    out.draws.triangles = p95(&|s| s.draws.triangles as f64).round() as u64;
    out.memory.total_bytes = all.iter().map(|s| s.memory.total_bytes).max().unwrap_or(0);
    out.pipelines.created_after_warm = all.iter().map(|s| s.pipelines.created_after_warm).max().unwrap_or(0);
    let timed: Vec<&specula_gpu::GpuTiming> = all.iter().filter_map(|s| s.gpu.as_ref()).collect();
    if let Some(g) = out.gpu.as_mut() {
        g.total_ms = Stats::of(timed.iter().map(|t| t.total_ms)).map_or(g.total_ms, |s| s.p95);
        let passes = pass_times(frames);
        for p in &mut g.passes {
            if let Some(st) = passes.iter().find(|(n, _)| *n == p.name).and_then(|(_, v)| Stats::of(v.iter().copied())) {
                p.gpu_ms = st.p95;
            }
        }
    }
    Some(out)
}

/// The capture file: per frame, every thread's zones as rows plus GPU passes and memory.
fn capture_json(cap: &PendingCapture) -> Json {
    let frames = cap.frames.iter().map(|f| {
        let threads = f.profile.threads.iter().map(|t| {
            let rows = t.ordered().into_iter().map(|i| {
                let z = &t.zones()[i];
                Json::arr([
                    Json::from(t.path(i)),
                    Json::from(z.calls),
                    ms(z.total_ms()),
                    ms(z.self_ms()),
                    ms(z.max_ms()),
                    Json::from(z.self_alloc_count),
                    Json::from(z.self_alloc_bytes),
                ])
            });
            (t.thread.as_str(), Json::arr(rows))
        });
        let gpu = f.gpu.as_ref().map_or(Json::Null, |s| {
            Json::obj([
                ("frame", Json::from(s.frame)),
                ("path", Json::from(path_name(s.path))),
                (
                    "gpu_ms",
                    s.gpu.as_ref().map_or(Json::Null, |g| {
                        Json::obj(std::iter::once(("total", ms(g.total_ms))).chain(g.passes.iter().map(|p| (p.name, ms(p.gpu_ms)))))
                    }),
                ),
                ("draw_calls", Json::from(s.draws.draw_calls)),
                ("triangles", Json::from(s.draws.triangles)),
                ("gpu_memory_mb", mb(s.memory.total_bytes)),
            ])
        });
        let m = &f.profile.memory;
        Json::obj([
            ("frame", Json::from(f.profile.frame)),
            ("dt_ms", ms(f.dt_ms)),
            ("zones", Json::obj(threads)),
            ("gpu", gpu),
            (
                "memory",
                Json::obj([
                    ("heap_bytes", Json::from(m.current_bytes)),
                    ("allocations", Json::from(m.allocations)),
                    ("allocated_bytes", Json::from(m.allocated_bytes)),
                    ("frees", Json::from(m.frees)),
                ]),
            ),
        ])
    });
    Json::obj([
        ("schema", Json::from("specula.perf_capture/v1")),
        ("label", cap.spec.label.as_deref().map_or(Json::Null, Json::from)),
        ("adapter", Json::from(cap.adapter.as_str())),
        ("budgets", cap.budgets.to_json()),
        ("memory_tracking", Json::from(alloc::installed())),
        ("zone_columns", Json::arr(["path", "calls", "ms", "self_ms", "max_call_ms", "self_allocs", "self_alloc_bytes"].map(Json::from))),
        ("frames", Json::arr(frames)),
    ])
}

/// Chrome trace-event JSON (open in chrome://tracing or Perfetto): CPU zones per thread, GPU
/// passes on their own row (laid end to end from each frame's start: GPU and CPU clocks are not
/// synchronised, so their placement is approximate), and the heap as a counter.
fn chrome_trace(frames: &[CapturedFrame]) -> Json {
    const GPU_TID: u32 = 0;
    let us = |ns: u64| Json::from(ns as f64 / 1000.0);
    let mut events = Vec::new();
    let mut named: Vec<u32> = Vec::new();
    let mut name_thread = |events: &mut Vec<Json>, tid: u32, name: &str| {
        if !named.contains(&tid) {
            named.push(tid);
            events.push(Json::obj([
                ("name", Json::from("thread_name")),
                ("ph", Json::from("M")),
                ("pid", Json::from(1)),
                ("tid", Json::from(tid)),
                ("args", Json::obj([("name", Json::from(name))])),
            ]));
        }
    };
    let epoch = profile::epoch();
    for f in frames {
        let fp = &f.profile;
        for e in &fp.events {
            name_thread(&mut events, e.tid, fp.thread_of(e.tid).unwrap_or("?"));
            events.push(Json::obj([
                ("name", Json::from(e.name)),
                ("cat", Json::from("cpu")),
                ("ph", Json::from("X")),
                ("ts", us(e.start_ns)),
                ("dur", us(e.dur_ns)),
                ("pid", Json::from(1)),
                ("tid", Json::from(e.tid)),
            ]));
        }
        // GPU passes, from the start of this frame's engine "frame" zone (else its first event,
        // else when it was collected).
        let frame_start = fp
            .thread(ENGINE_THREAD)
            .and_then(|t| fp.events.iter().find(|e| e.depth == 0 && e.name == "frame" && t.tids.contains(&e.tid)))
            .or_else(|| fp.events.iter().min_by_key(|e| e.start_ns))
            .map(|e| e.start_ns)
            .or_else(|| fp.at.map(|t| t.saturating_duration_since(epoch).as_nanos() as u64));
        if let (Some(start), Some(g)) = (frame_start, f.gpu.as_ref().and_then(|s| s.gpu.as_ref())) {
            name_thread(&mut events, GPU_TID, "GPU (passes end to end; approximate placement)");
            let mut at = start;
            for p in &g.passes {
                let dur = (p.gpu_ms * 1e6) as u64;
                events.push(Json::obj([
                    ("name", Json::from(p.name)),
                    ("cat", Json::from("gpu")),
                    ("ph", Json::from("X")),
                    ("ts", us(at)),
                    ("dur", us(dur)),
                    ("pid", Json::from(1)),
                    ("tid", Json::from(GPU_TID)),
                    ("args", Json::obj([("gpu_frame", Json::from(g.frame))])),
                ]));
                at += dur;
            }
        }
        if fp.memory.tracking {
            let at = fp.at.map_or(0, |t| t.saturating_duration_since(epoch).as_nanos() as u64);
            events.push(Json::obj([
                ("name", Json::from("heap")),
                ("ph", Json::from("C")),
                ("ts", us(at)),
                ("pid", Json::from(1)),
                ("args", Json::obj([("MB", Json::from((bytes_mb(fp.memory.current_bytes) * 100.0).round() / 100.0))])),
            ]));
        }
    }
    Json::obj([("traceEvents", Json::Arr(events)), ("displayTimeUnit", Json::from("ms"))])
}

/// Where the pipeline cache for an `--adapter` choice lives. The GPU crate checks the blob's
/// vendor, device and driver before using it, so a stale or foreign file is only a cache miss.
pub fn pipeline_cache_path(dir: &Path, adapter_label: &str) -> PathBuf {
    let safe: String =
        adapter_label.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    dir.join(format!("pipelines-{safe}.bin"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_gpu::{Access, GpuTiming, GraphBarrier, GraphPass, GraphResource, GraphUse, Load, PassTiming};

    #[test]
    fn render_graph_summary_and_detail() {
        assert_eq!(graph_json(None, true), Json::Null);
        let barrier = GraphBarrier {
            resource: "hdr",
            old_layout: "undefined",
            new_layout: "color_attachment",
            src_stage: 0,
            src_access: 0,
            dst_stage: 0,
            dst_access: 0,
        };
        let g = GraphInfo {
            resources: vec![GraphResource {
                name: "hdr",
                kind: "transient",
                width: 64,
                height: 32,
                layers: 1,
                format: 97,
                usage: 0,
                bytes: 0,
            }],
            passes: vec![GraphPass {
                name: "scene",
                inline: false,
                uses: vec![GraphUse { resource: "hdr", access: Access::Color(Load::Clear), load_op: "clear", store_op: "store" }],
                barriers: vec![barrier],
            }],
            final_barriers: vec![],
            compiles: 2,
        };
        let short = graph_json(Some(&g), false);
        assert_eq!(short.get("passes").and_then(Json::as_f64), Some(1.0));
        assert_eq!(short.get("barriers").and_then(Json::as_f64), Some(1.0));
        assert_eq!(short.get("compiles").and_then(Json::as_f64), Some(2.0));
        assert!(short.get("pass_list").is_none(), "detail only when asked");
        let full = graph_json(Some(&g), true);
        let text = full.to_string();
        assert!(text.contains("\"Color(Clear)\"") && text.contains("undefined->color_attachment"), "{text}");
        assert!(full.get("resource_list").is_some() && full.get("final_barriers").is_some());
    }

    #[test]
    fn series_statistics() {
        let mut s = Series::default();
        assert_eq!(s.to_json(), Json::Null);
        for i in 1..=200u64 {
            s.push(Duration::from_millis(i));
        }
        let j = s.to_json();
        assert_eq!(j.get("samples").and_then(Json::as_u64), Some(HISTORY as u64));
        assert_eq!(j.get("max_ms").and_then(Json::as_f64), Some(200.0));
        assert_eq!(j.get("median_ms").and_then(Json::as_f64), Some(141.0));
        assert_eq!(s.p95(), Some(194.0));
    }

    fn codes(errors: &[Json]) -> Vec<String> {
        errors.iter().map(|e| e.get("error").and_then(Json::as_str).unwrap().to_owned()).collect()
    }

    fn zone(path: &str, p95: f64) -> RollingZone {
        RollingZone {
            thread: ENGINE_THREAD.into(),
            path: path.into(),
            depth: 0,
            frames: 10,
            calls: 10,
            mean_ms: p95,
            p50_ms: p95,
            p95_ms: p95,
            max_frame_ms: p95,
            max_call_ms: p95,
            self_mean_ms: p95,
            self_allocs_mean: 0.0,
            self_alloc_bytes_mean: 0.0,
        }
    }

    #[test]
    fn default_budgets_keep_the_old_checks() {
        let mut s = FrameStats::default();
        let b = Budgets::default().with_frame(16.0);
        assert!(evaluate(&b, &Sample { stats: Some(&s), pacing_p95_ms: Some(16.0), ..Sample::default() }, "x").is_empty());
        s.pipelines.created_after_warm = 2;
        s.gpu = Some(GpuTiming { frame: 1, total_ms: 20.0, passes: Vec::new() });
        let errors = evaluate(&b, &Sample { stats: Some(&s), pacing_p95_ms: Some(30.0), ..Sample::default() }, "integrated");
        assert_eq!(codes(&errors), ["pipeline_created_after_warm", "gpu_over_budget", "frame_pacing_over_budget"]);
        let gpu = &errors[1];
        assert_eq!(gpu.get("metric"), Some(&Json::from("gpu_ms")));
        assert_eq!(gpu.get("value"), Some(&Json::from(20.0)));
        assert_eq!(gpu.get("budget"), Some(&Json::from(16.0)));
        assert_eq!(gpu.get("unit"), Some(&Json::from("ms")));
        assert_eq!(gpu.get("adapter"), Some(&Json::from("integrated")));
    }

    /// Every metric: just under its budget passes, just over it is reported.
    #[test]
    fn each_budget_metric_misses_and_passes() {
        let cfg = PerfConfig::parse(
            "[budgets]\nframe_ms = 16\ngpu_ms = 10\ncpu_frame_ms = 8\nframe_pacing_p95_ms = 20\ndraw_calls = 100\n\
             triangles = 1000\ngpu_memory_mb = 64\nheap_mb = 32\nallocations_per_frame = 50\nsim_tick_ms = 2\n\
             game_frame_ms = 1\npipelines_after_warm = 1\n[budgets.gpu_passes]\nscene = 6\n[budgets.zones]\n\
             \"frame/sim\" = 3\n\"specula-autosave:autosave.write\" = 50\n",
        )
        .unwrap();
        let b = cfg.budgets("integrated");
        let stats = |scale: f64| {
            let mut s = FrameStats::default();
            s.pipelines.created_after_warm = (1.0 * scale).round() as u32;
            s.gpu = Some(GpuTiming {
                frame: 1,
                total_ms: 10.0 * scale,
                passes: vec![PassTiming { name: "scene", gpu_ms: 6.0 * scale }, PassTiming { name: "shadow", gpu_ms: 99.0 }],
            });
            s.draws.draw_calls = (100.0 * scale) as u32;
            s.draws.triangles = (1000.0 * scale) as u64;
            s.memory.total_bytes = (64.0 * scale * 1_048_576.0) as u64;
            s
        };
        let zones_at = |scale: f64| {
            let mut z = vec![zone("frame/sim", 3.0 * scale), zone("autosave.write", 50.0 * scale)];
            z[1].thread = "specula-autosave".into();
            z
        };
        let sample = |s: &FrameStats, z: &'static [RollingZone], scale: f64| -> Vec<Json> {
            let sample = Sample {
                stats: Some(s),
                pacing_p95_ms: Some(20.0 * scale),
                cpu_frame_p95_ms: Some(8.0 * scale),
                tick_p95_ms: Some(2.0 * scale),
                game_frame_p95_ms: Some(1.0 * scale),
                zones: z,
                heap_bytes: Some((32.0 * scale * 1_048_576.0) as u64),
                allocations_per_frame_p95: Some(50.0 * scale),
            };
            evaluate(&b, &sample, "integrated")
        };
        let at = stats(1.0);
        let z_at: &'static [RollingZone] = Box::leak(zones_at(1.0).into_boxed_slice());
        assert!(sample(&at, z_at, 1.0).is_empty(), "{:?}", sample(&at, z_at, 1.0));
        let over = stats(2.0);
        let z_over: &'static [RollingZone] = Box::leak(zones_at(2.0).into_boxed_slice());
        let errors = sample(&over, z_over, 2.0);
        assert_eq!(
            codes(&errors),
            [
                "pipeline_created_after_warm",
                "gpu_over_budget",
                "gpu_pass_over_budget",
                "draw_calls_over_budget",
                "triangles_over_budget",
                "gpu_memory_over_budget",
                "frame_pacing_over_budget",
                "cpu_frame_over_budget",
                "sim_tick_over_budget",
                "game_frame_over_budget",
                "heap_over_budget",
                "allocations_over_budget",
                "zone_over_budget",
                "zone_over_budget",
            ]
        );
        let metrics: Vec<&str> = errors.iter().map(|e| e.get("metric").and_then(Json::as_str).unwrap()).collect();
        assert!(metrics.contains(&"gpu_pass.scene") && !metrics.contains(&"gpu_pass.shadow"), "{metrics:?}");
        assert!(metrics.contains(&"zone.frame/sim") && metrics.contains(&"zone.specula-autosave:autosave.write"));
    }

    #[test]
    fn adapter_sections_override_the_base() {
        let cfg = PerfConfig::parse(
            "[profiler]\nenabled = false\nhistory_frames = 30\n[budgets]\nframe_ms = 16.7\ndraw_calls = 500\n\
             [budgets.gpu_passes]\nscene = 9\nshadow = 2\n[adapter.discrete]\ngpu_ms = 6\n[adapter.discrete.gpu_passes]\nscene = 4\n",
        )
        .unwrap();
        assert!(!cfg.profiler);
        assert_eq!(cfg.history_frames, 30);
        let i = cfg.budgets("integrated");
        assert_eq!((i.frame_ms, i.gpu_ms, i.draw_calls), (16.7, 16.7, Some(500)));
        assert_eq!(i.gpu_passes, [("scene".to_string(), 9.0), ("shadow".to_string(), 2.0)]);
        let d = cfg.budgets("discrete");
        assert_eq!((d.frame_ms, d.gpu_ms, d.draw_calls), (16.7, 6.0, Some(500)));
        assert_eq!(d.gpu_passes, [("scene".to_string(), 4.0), ("shadow".to_string(), 2.0)]);
        assert!(cfg.has_adapter_section("discrete") && !cfg.has_adapter_section("integrated"));
        assert_eq!(PerfConfig::default().budgets("integrated"), Budgets::default());
    }

    #[test]
    fn bad_config_is_rejected_with_its_reason() {
        let err = |text: &str| PerfConfig::parse(text).unwrap_err();
        assert!(err("[budgets]\nframe_ms = 0").contains("frame_ms"));
        assert!(err("[budgets]\nframe_mss = 16").contains("unknown budget 'frame_mss'"));
        assert!(err("[budgets]\ndraw_calls = -1").contains("draw_calls"));
        assert!(err("[adapter.igpu]\nframe_ms = 16").contains("unknown adapter kind 'igpu'"));
        assert!(err("[adapter.integrated]\nframe_ms = 'x'").contains("[adapter.integrated] frame_ms"));
        assert!(err("[budgets.zones]\n\"frame//sim\" = 1").contains("zone path"));
        assert!(err("[profiler]\nhistory_frames = 0").contains("history_frames"));
        assert!(err("[tools]\nx = 1").contains("unknown section 'tools'"));
        assert!(err("[budgets]\nframe_ms = 16\nframe_ms = 17").contains("duplicate_key"));
        assert!(err("[budgets]\nframe_ms = 16\nframe_ms = 17").contains("line 3"));
    }

    #[test]
    fn the_example_file_is_valid() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../perf.example.toml");
        let cfg = PerfConfig::load(Some(&path), Path::new(".")).unwrap();
        assert_eq!(cfg.source.as_deref(), Some(path.as_path()));
        let i = cfg.budgets("integrated");
        assert!((i.frame_ms - 16.7).abs() < 1e-9, "{i:?}");
        assert!(!i.gpu_passes.is_empty() && !i.zones.is_empty());
        assert!(cfg.has_adapter_section("integrated"));
    }

    #[test]
    fn missing_default_file_means_defaults() {
        let dir = std::env::temp_dir().join(format!("specula-perf-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(PerfConfig::load(None, &dir).unwrap(), PerfConfig::default());
        assert!(PerfConfig::load(Some(&dir.join("nope.toml")), &dir).is_err());
        std::fs::write(dir.join(DEFAULT_CONFIG_FILE), "[budgets]\nframe_ms = 33.3\n").unwrap();
        let cfg = PerfConfig::load(None, &dir).unwrap();
        assert_eq!(cfg.budgets("discrete").frame_ms, 33.3);
        std::fs::write(dir.join(DEFAULT_CONFIG_FILE), "[budgets\n").unwrap();
        assert!(PerfConfig::load(None, &dir).is_err(), "a broken default file is an error, not ignored");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cache_paths_are_file_safe() {
        assert_eq!(pipeline_cache_path(Path::new("c"), "integrated"), Path::new("c").join("pipelines-integrated.bin"));
        assert_eq!(pipeline_cache_path(Path::new("c"), "../x"), Path::new("c").join("pipelines-___x.bin"));
    }

    /// Tests that collect the process-wide profiler run one at a time.
    static PROFILER: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        PROFILER.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn capture_records_frames_writes_files_and_answers() {
        let _s = serial();
        let dir = std::env::temp_dir().join(format!("specula-perf-capture-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ctx = || CaptureContext { dir: dir.clone(), budgets: Budgets::default(), adapter: "integrated".into() };
        let later = Instant::now() + Duration::from_secs(60);
        let mut p = Profiler::new(8);
        let (tx, rx) = std::sync::mpsc::channel();
        assert!(!profile::enabled());
        p.start_capture(CaptureSpec { frames: 3, trace: true, label: Some("t".into()) }, tx, later, ctx());
        assert!(profile::enabled() && profile::tracing() && p.capturing());
        let (tx2, rx2) = std::sync::mpsc::channel();
        p.start_capture(CaptureSpec { frames: 3, trace: false, label: None }, tx2, later, ctx());
        let refused = rx2.try_recv().unwrap();
        assert!(refused.is_error && format!("{:?}", refused.content).contains("capture_in_progress"));
        let slow_gpu = || {
            let passes = vec![PassTiming { name: "scene", gpu_ms: 20.0 }];
            Some(FrameStats { gpu: Some(GpuTiming { frame: 1, total_ms: 20.0, passes }), ..FrameStats::default() })
        };
        for _ in 0..3 {
            assert!(rx.try_recv().is_err(), "answered before its frames were recorded");
            {
                let _f = profile::zone("frame");
                let _s = profile::zone("sim");
            }
            p.end_frame_with(slow_gpu);
        }
        let reply = rx.try_recv().expect("answered after the last frame");
        let specula_protocol::Content::Text(text) = &reply.content[0] else { panic!("text reply") };
        let out = Json::parse(text).unwrap();
        assert_eq!(out.get("frames"), Some(&Json::from(3)));
        assert_eq!(out.get("stopped_early"), Some(&Json::Null));
        let errors = out.get("budget_errors").and_then(Json::as_array).unwrap();
        assert_eq!(codes(errors), ["gpu_over_budget"]);
        // Profiling goes back to how it was.
        assert!(!profile::enabled() && !profile::tracing() && !p.capturing());
        let data = Json::parse(&std::fs::read_to_string(out.get("path").and_then(Json::as_str).unwrap()).unwrap()).unwrap();
        assert_eq!(data.get("schema"), Some(&Json::from("specula.perf_capture/v1")));
        assert_eq!(data.get("frames").and_then(Json::as_array).map(<[Json]>::len), Some(3));
        let trace_path = out.get("trace_path").and_then(Json::as_str).unwrap();
        assert!(trace_path.ends_with("perf-capture-001-t.trace.json"), "{trace_path}");
        let trace = Json::parse(&std::fs::read_to_string(trace_path).unwrap()).unwrap();
        let events = trace.get("traceEvents").and_then(Json::as_array).unwrap();
        assert_eq!(out.get("trace_events"), Some(&Json::from(events.len())));
        assert!(events.iter().any(|e| e.get("name") == Some(&Json::from("sim"))));
        assert_eq!(events.iter().filter(|e| e.get("name") == Some(&Json::from("scene"))).count(), 3);

        // Out of time: answers with what it has.
        let (tx, rx) = std::sync::mpsc::channel();
        p.start_capture(CaptureSpec { frames: 100, trace: false, label: None }, tx, Instant::now(), ctx());
        p.end_frame_with(|| None);
        let reply = rx.try_recv().expect("answered at the deadline");
        let specula_protocol::Content::Text(text) = &reply.content[0] else { panic!("text reply") };
        let out = Json::parse(text).unwrap();
        assert_eq!((out.get("frames"), out.get("stopped_early")), (Some(&Json::from(1)), Some(&Json::from("time_limit"))));
        assert_eq!(out.get("trace_path"), Some(&Json::Null));
        assert!(out.get("path").and_then(Json::as_str).unwrap().ends_with("perf-capture-002.json"));

        // Shutting down: answers with what it has, then the reply channel is gone.
        let (tx, rx) = std::sync::mpsc::channel();
        p.start_capture(CaptureSpec { frames: 100, trace: true, label: None }, tx, later, ctx());
        p.end_frame_with(|| None);
        p.abort_capture();
        let reply = rx.try_recv().expect("answered on abort");
        let specula_protocol::Content::Text(text) = &reply.content[0] else { panic!("text reply") };
        let out = Json::parse(text).unwrap();
        assert_eq!((out.get("frames"), out.get("stopped_early")), (Some(&Json::from(1)), Some(&Json::from("shutting_down"))));
        assert!(matches!(rx.recv(), Err(std::sync::mpsc::RecvError)), "the reply channel disconnects");
        assert!(!p.capturing() && !profile::tracing() && !profile::enabled());
        p.abort_capture(); // nothing to abort: no-op
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Budgets are checked against the capture's p95, not its last frame: one slow frame in 40
    /// is reported in the maximum but is no budget error; a slow last frame isn't hidden either.
    #[test]
    fn capture_budgets_use_the_p95_over_its_frames() {
        let frame = |gpu_ms: f64, draws: u32| CapturedFrame {
            profile: FrameProfile::default(),
            gpu: Some(FrameStats {
                gpu: Some(GpuTiming { frame: 1, total_ms: gpu_ms, passes: vec![PassTiming { name: "scene", gpu_ms }] }),
                draws: specula_gpu::DrawStats { draw_calls: draws, ..Default::default() },
                ..FrameStats::default()
            }),
            dt_ms: 16.0,
        };
        let budgets = Budgets { gpu_passes: vec![("scene".into(), 10.0)], draw_calls: Some(100), ..Budgets::default().with_frame(16.0) };
        let errors_for = |frames: Vec<CapturedFrame>| {
            let s = capture_stats(&frames).unwrap();
            evaluate(&budgets, &Sample { stats: Some(&s), ..Sample::default() }, "integrated")
        };
        // One spike, not last: within budget at p95.
        let mut frames: Vec<CapturedFrame> = (0..40).map(|_| frame(5.0, 50)).collect();
        frames[10] = frame(30.0, 500);
        assert!(errors_for(frames).is_empty());
        // Every frame slow except the last: the old last-frame check missed this.
        let mut frames: Vec<CapturedFrame> = (0..40).map(|_| frame(30.0, 500)).collect();
        frames.push(frame(5.0, 50));
        assert_eq!(codes(&errors_for(frames)), ["gpu_over_budget", "gpu_pass_over_budget", "draw_calls_over_budget"]);
    }

    #[test]
    fn chrome_trace_has_threads_zones_and_gpu() {
        use specula_core::profile::{zone as z, set_enabled, set_tracing};
        let _s = serial();
        set_enabled(true);
        set_tracing(true);
        let mut c = Collector::new(4);
        c.collect(0);
        {
            let _f = z("frame");
            let _s = z("sim");
        }
        let fp = c.collect(1).clone();
        set_tracing(false);
        set_enabled(false);
        let passes = vec![PassTiming { name: "scene", gpu_ms: 2.0 }, PassTiming { name: "post_fxaa", gpu_ms: 1.0 }];
        let s = FrameStats { gpu: Some(GpuTiming { frame: 1, total_ms: 3.0, passes }), ..FrameStats::default() };
        let frames = [CapturedFrame { profile: fp, gpu: Some(s), dt_ms: 16.0 }];
        let trace = chrome_trace(&frames);
        let events = trace.get("traceEvents").and_then(Json::as_array).unwrap();
        let named = |n: &str| events.iter().filter(|e| e.get("name").and_then(Json::as_str) == Some(n)).count();
        // Other tests' threads may have recorded zones too; this thread's are there.
        assert!(named("frame") >= 1 && named("sim") >= 1, "{trace}");
        assert_eq!((named("scene"), named("post_fxaa")), (1, 1));
        let scene = events.iter().find(|e| e.get("name").and_then(Json::as_str) == Some("scene")).unwrap();
        assert_eq!(scene.get("dur").and_then(Json::as_f64), Some(2000.0));
        assert!(named("thread_name") >= 2);
    }
}

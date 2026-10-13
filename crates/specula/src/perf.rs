//! Performance agents can see (foundation F3): `perf_frame` reports the renderer's own statistics
//! (GPU time per pass, draws, memory, pipelines) plus frame pacing and simulation cost measured
//! here, and turns budget misses into errors an agent can act on.

use specula_core::Json;
use specula_gpu::{CacheRejection, FramePath, FrameStats, MemoryKind, PipelineCacheLoad};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Frames of history for pacing statistics (about two seconds at 60 Hz).
const HISTORY: usize = 120;

/// Default frame budget: 60 fps on the reference iGPU.
pub const DEFAULT_BUDGET_MS: f64 = 1000.0 / 60.0;

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
        if self.samples.is_empty() {
            return Json::Null;
        }
        let mut sorted: Vec<f64> = self.samples.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        let mean = sorted.iter().sum::<f64>() / n as f64;
        let pick = |q: f64| sorted[((q * (n - 1) as f64).round() as usize).min(n - 1)];
        Json::obj([
            ("samples", Json::from(n)),
            ("mean_ms", ms(mean)),
            ("median_ms", ms(pick(0.5))),
            ("p95_ms", ms(pick(0.95))),
            ("max_ms", ms(sorted[n - 1])),
        ])
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

/// Budget misses for a frame, as structured errors (empty = within budget).
pub fn budget_errors(s: &FrameStats, budget_ms: f64, pacing_p95_ms: Option<f64>) -> Vec<Json> {
    let mut errors = Vec::new();
    if s.pipelines.created_after_warm > 0 {
        errors.push(Json::obj([
            ("error", Json::from("pipeline_created_after_warm")),
            ("count", Json::from(s.pipelines.created_after_warm)),
            ("message", Json::from("pipelines were built during play; each one is a visible hitch")),
        ]));
    }
    if let Some(g) = &s.gpu {
        if g.total_ms > budget_ms {
            errors.push(Json::obj([
                ("error", Json::from("gpu_over_budget")),
                ("gpu_ms", ms(g.total_ms)),
                ("budget_ms", ms(budget_ms)),
            ]));
        }
    }
    if let Some(p95) = pacing_p95_ms {
        // Present waits for vsync, so pacing above budget means frames are being missed.
        if p95 > budget_ms * 1.25 {
            errors.push(Json::obj([
                ("error", Json::from("frame_pacing_over_budget")),
                ("p95_ms", ms(p95)),
                ("budget_ms", ms(budget_ms)),
            ]));
        }
    }
    errors
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
    }

    #[test]
    fn budget_errors_are_structured() {
        let mut s = FrameStats::default();
        assert!(budget_errors(&s, 16.0, Some(16.0)).is_empty());
        s.pipelines.created_after_warm = 2;
        s.gpu = Some(specula_gpu::GpuTiming { frame: 1, total_ms: 20.0, passes: Vec::new() });
        let codes: Vec<String> = budget_errors(&s, 16.0, Some(30.0))
            .iter()
            .map(|e| e.get("error").and_then(Json::as_str).unwrap().to_owned())
            .collect();
        assert_eq!(codes, ["pipeline_created_after_warm", "gpu_over_budget", "frame_pacing_over_budget"]);
    }

    #[test]
    fn cache_paths_are_file_safe() {
        assert_eq!(pipeline_cache_path(Path::new("c"), "integrated"), Path::new("c").join("pipelines-integrated.bin"));
        assert_eq!(pipeline_cache_path(Path::new("c"), "../x"), Path::new("c").join("pipelines-___x.bin"));
    }
}

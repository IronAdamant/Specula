# Performance baselines

`python scripts/perf_baseline.py --label <what changed>` measures the engine on every GPU and the agent
iteration loop, and saves `perf/baselines/<timestamp>-<label>.json` (schema `specula.baseline/v1`).
Compare a run with an earlier one: `--compare perf/baselines/<file>.json` (exit code 2 when a timing
regresses by more than `--threshold`, default 15%).

What it measures:
- **Agent loop:** clean and incremental debug builds, the workspace test run, the release engine build.
- **Engine, per GPU** (release build, headless, its own port 7495): start-up until MCP answers, MCP round
  trip, cost of one `world_spawn`, a 1000-op `world_batch` (when the engine has it), `sight_frame` time
  (render + readback + PNG) for a 500-cube grid and the lava lamps at 720p and 1080p, `perf_frame` GPU
  statistics and budget errors against `perf.example.toml` (when the engine has it), CPU zone p95s
  (`perf_zones`) and heap use (`perf_memory`), working-set memory, validation errors and a clean shutdown.

**Take baselines on an idle machine.** The script records whole-machine CPU use before and after and
warns above 20%: with parallel builds running, single calls show 15–25 ms scheduler delays instead of
under 1 ms, and build timings double.

# Budgets: perf.toml

The engine reads budgets from `--perf FILE`, else `./perf.toml` if it exists, else built-in defaults
(16.67 ms frame, no pipelines built after warm-up). `perf.example.toml` at the repository root documents
every key with Sepulcra's numbers on the reference iGPU; `[adapter.integrated]`, `[adapter.discrete]`, ...
override the base `[budgets]` key by key for that kind of GPU. A malformed file stops the engine at
start-up with the line and column. `perf_frame` (and `perf_capture`'s summary) report every miss as
`{error, metric, value, budget, unit, adapter}`, for example
`{"error":"gpu_pass_over_budget","metric":"gpu_pass.scene","value":9.1,"budget":8,"unit":"ms","adapter":"integrated"}`.

**Shape budgets (world limits).** `[budgets]` also sets how many vertices the distinct `shape2d` shapes
of a world may need (their parse-time upper bounds; identical shapes count once), in two pools:
`shape2d_vertices` (default 2,000,000) for persistent entities and `transient_shape2d_vertices` (default
250,000) for transient ones, which is what a hosted game spawns for effects and presentation. A spawn,
set, undo or world load that would pass its pool's budget is refused with
`{"error":"limit","field":"shape2d","budget":"transient_shape2d_vertices","limit":…,"value":…,"shape_vertices":…}`
(a load is refused whole). Why two pools:
- effects can never crowd out authored content, and authored content never starves the game's effects;
- a refusal tells the game designer exactly what an effect costs, against a budget they set;
- the small transient default keeps effects cheap on the reference iGPU.

They are world limits, not frame budgets, so they may only appear in `[budgets]`, never in
`[adapter.*]`: a world must load the same on every machine.

# Profiling tools

- **CPU zones** (`specula_core::profile`): `let _z = profile::zone("sim");` around a phase. The engine
  thread's frame is `frame` with children `requests` (agents' tool calls; `sight.render`, `sight.encode`,
  `sight.save` inside), `autosave` (`autosave.plan`), `sim` (`tick`, `tick_hash`), `game_frame`,
  `draw_list` (with `sync_meshes`), `parked_captures` and `present` (which waits for vsync). The autosave worker records
  `autosave.write` on thread `specula-autosave`. `perf_zones` lists them (latest frame plus rolling
  mean/p95; paged; `enabled` switches recording).
- **Memory** (`specula_core::alloc::Counting`): the `specula` and `sepulcra` binaries install the counting
  allocator; `perf_memory` gives heap, peak, allocations per frame and the zones that allocate most.
  Vulkan driver memory is not on the Rust heap: see `perf_frame`'s GPU memory.
- **Frame capture**: `perf_capture {frames: 120}` records the next frames (zones of every thread, GPU
  passes, draws, memory) to `captures/run-*/perf-capture-NNN.json` and a Chrome trace-event file
  (`.trace.json`; open it in `chrome://tracing` or https://ui.perfetto.dev).
- **Live check**: `python scripts/perf_profile_check.py --adapter integrated` runs the release engine
  windowed with `perf.example.toml` and the lava lamps, calls every tool, checks the files, and compares
  `sight_frame` time with the profiler on and off.
- **Zone cost**: `cargo test --release -p specula-core --lib zone_cost -- --ignored --nocapture`
  (measured 2026-10-14: about 3 ns disabled, 130–160 ns enabled, of which two `Instant::now` calls are
  about 120 ns on this machine; an engine frame records about 10 zones).

# Sight captures (ID buffer)

Every capture (`sight_frame`, `sight_orbit`, `sight_compare`, `sight_query`) renders the ID buffer
for per-subject `visible_px`, `screen_box_exact`, `no_pixels` and `occluded`: an `id_buffer` GPU pass
after the frame's own passes, a copy, and a CPU count of the opaque ids (run-length per row, in
parallel row bands for big images). `sight_frame {ids: false}` skips all of it. Overlays are drawn in
the same render; only the `bounds` overlay renders the frame a second time (its hidden boxes are red).
The packet's `id_buffer` gives each capture's cost (`gpu_ms`, `readback_count_ms`).

Measured 2026-10-15, release build, Intel RaptorLake iGPU (`--adapter integrated`), 400 cubes:

| Capture | GPU (`id_buffer` pass + copy) | Readback + count (CPU) |
|---|---|---|
| 1280×720 | 0.43 ms | 1.4 ms |
| 2048×2048 | 1.47 ms | 1.8 ms |

Memory: the id image, its depth and the readback are shared by every capture size (sized to the
largest capture so far, in 256 px steps): four cached 2048² targets went from 200.6 to 250.7 MB of
GPU memory with the ID buffer on (it was 529 MB with per-target ID images).

**Blended (glass-like) draws cost by their coverage and layers.** Every blended fragment puts its
id into its pixel's set of the 4 smallest ids (reads plus at most one successful compare-and-swap
per id and pixel), and a full-screen pass resolves the sets (atomics only on each id's region
edges). Ids pushed out of a set (more than 4 blended ids at a pixel) are only flagged: their counts
are lower bounds (`visible_px_approx: true`). Nothing counts per fragment, so nothing contends on
one address: the cost grows linearly with layers × pixels, like the scene's own blended overdraw
(16 full-view panes at 2048² on Intel: `id_buffer` 170 ms next to the scene's own 177 ms).
Measured 2026-10-15, release, `id_buffer` GPU ms (copy included); 7 opaque cubes plus:

| Scene | Intel iGPU 720p | Intel iGPU 2048² | RTX 4090 Laptop 720p | RTX 4090 Laptop 2048² |
|---|---|---|---|---|
| no blended draws | 0.30 | 0.93 | 0.16 | 0.67 |
| 7 overlapping glass spheres (≤ 4 per pixel) | 1.23 | 5.65 | 0.19 | 0.88 |
| 7 concentric glass spheres (3 ids approximate) | 1.61 | 8.97 | 0.21 | 0.96 |

Full-view glass panes (each a thin box: 2 layers), `id_buffer` GPU ms:

| Panes | Intel iGPU 720p | Intel iGPU 2048² | RTX 4090 Laptop 720p | RTX 4090 Laptop 2048² |
|---|---|---|---|---|
| 4 | 24.6 | - | 0.5 | - |
| 8 | 36.6 | 101 | - | 3.1 |
| 16 | 43.3 | 147 | 1.1 | 5.0 |
| 32 | 70.6 | 286 | 2.0 | 8.9 |

(Round 4's per-fragment overflow recount took 624 ms for 12 panes at 720p and lost the device with
16 at 2048²; the first, order-dependent scheme took 2.67 / 16.5 ms on Intel for the 7 overlapping
spheres.) CPU readback + count stays 0.8-1.7 ms. A lost device (`VK_ERROR_DEVICE_LOST`) comes back
from a capture as `{"error": "gpu_device_lost", ...}`; the engine can't recover in that run.

Memory: the per-pixel sets take exactly 16 bytes per pixel of the shared bucket (64 MB at 2048²,
15 MB for a 1280×720 capture's bucket), counted as render targets, allocated once a capture has
blended draws and never cleared between captures (only when new or every 4095 captures). The
stats buffer takes 28 bytes per id.

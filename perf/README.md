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

# M2 exit proof: 10k instances on the iGPU

`python scripts/m2_exit.py --adapter both` (release build) also measures two 10,000-entity worlds in
the window at 1280x720 with the Sepulcra night effects (shadows, SSAO, bloom, fog, grading, FXAA)
against `perf.example.toml`, through `perf_capture` (120 frames each, budgets on the p95): "mixed"
(8,000 cubes + 2,000 tombstone models) and "tombstones" (10,000 tombstone models). The frame's GPU ms,
pacing, draw calls, triangles, GPU memory, pipelines after warm-up and the CPU zone `frame/draw_list`
decide it; per-pass and other CPU misses are printed as notes. `--criteria 3` runs only that part.
Timings vary between runs (other engines on the machine share the GPU; the script warns when it sees
them). On 2026-10-16, Intel iGPU: GPU p95 7.1 / 9.2 ms, draw list p95 0.36 / 0.53 ms.

`cargo test --release -p specula --test draw_list_10k -- --nocapture` times the window's CPU draw work
for a 10k-entity world (the full walk captures use vs the window's cache, warm and with cold caches,
with 0, 1 and 100 transient enemies or persistent markers moved per frame), checks that a warm frame
allocates nothing and that the cache is never slower than the walk. `perf_frame`'s `draw_cache`
counts frames that rebuilt the cache vs patched it; `tests/draw_cache_fuzz.rs` checks the cache
draws exactly what the full walk does.

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

# M3 exit proof: Sepulcra on the iGPU

`python scripts/m3_exit.py --adapter integrated [--max-fps N]` plays a whole game of Sepulcra (Lex rules)
windowed at 1280x720 on the reference iGPU and checks the frame-level budgets over 600 real-time frames
in the busiest wave and in the boss wave. Each capture records the desktop apps running and, on Windows,
every process's share of the engine's GPU while it measured (`gpu_3d_percent_mean_max`, from the OS's GPU
counters): the criterion must hold with typical apps open (the owner's machine runs Discord, Brave,
Obsidian and Steam), so a failure there is a failure, and the data says who else used the GPU.

Per-pass GPU ms, boss wave, same view, Intel RaptorLake-S iGPU (2026-10-22, quiet GPU):

| pass | before | after | what changed |
|---|---|---|---|
| scene | 2.73 | 1.88 | point lights out of range skipped; 4 rim shadow taps first (12 only in penumbrae); sky ray from the vertex stage; fixed powers multiplied |
| post_fxaa | 0.37 | 0.26 | luma written into alpha by the tonemap pass, neighbours gathered (FXAA 3.11 `FXAA_GATHER4_ALPHA`) |
| ssao (3 passes) | 0.47 | 0.47 | already half resolution, 8 samples, separable depth-aware blur; the cheaper spiral saved nothing measurable |
| post_tonemap | 0.78 | 0.79 | grading is 0.26 of it (saturation 0.08, lift/gain 0.05) |
| bloom, shadow, overlay | 0.42, 0.19, 0.19 | the same | |
| **GPU frame** | **5.1** | **4.2** | |

FXAA's p95 of 1.6-2.7 ms in earlier runs was never FXAA's own cost (0.26-0.37 ms; 0.23 offscreen):
spikes land in the passes that write the swapchain image (FXAA and the overlay drawn in it) when other
GPU work (DWM composing at 240 Hz, other engines) runs at the end of the frame. Pass p95s under
contention are a contention measure; the frame's GPU p95 and pacing decide the criterion.

Results (`perf/drills/20261022-m3-exit-igpu-render-budget.json`): uncapped, no other engine, 4 of 4
pass (busy wave GPU p95 6.8-7.9 ms, pacing p95 14.3-17.6 ms; boss wave GPU p95 5.0-5.9, pacing p95
11.4-11.7). With a second Sepulcra engine left running at 60 fps (a third of the iGPU) 3 of 4 uncapped
runs still pass (the miss: boss pacing p95 23.5 ms while DWM took 12%).

**The frame cap** (`--max-fps`, `[window] max_fps`) halves the iGPU's load (about 31% vs 60-85% at
240 Hz) and steadies GPU times (p95 4.2-4.4 ms), but 1 of 2 capped clean runs missed pacing (24.2 ms,
DWM bursts to 40%): a capped frame's present finds a free image and returns at once, so a CPU stall
reaches the frame interval undamped, where an uncapped FIFO present absorbs it while it blocks for the
vblank. So it is off by default. Presenting at a target time (submit early, present on time; or
`VK_KHR_present_wait`) would remove that.

# Agent throughput (windowed)

The window serves agents' calls between presents (`crates/specula/src/schedule.rs`): each serve window is the
display's refresh period minus the frame's own work and a margin; under a backlog (two or more calls still queued
when windows end) it presents less often but keeps at least 60 fps (`BUSY_FRAME`). `perf_frame.serve` reports the
window, period and its source, frame work, margin, backlog, held calls, late presents and `call_cost_ms` per call
class.

- `python scripts/call_rate.py`: one sequential agent's calls/s with the window open, per adapter.
- `python scripts/swarm_drill.py --windowed --agents 16|64`: swarm throughput and frame pacing with the window open.
- Results: `perf/drills/*-windowed-{before,after}.json` (2026-10-17: one agent 500-1,270 calls/s, was about
  110-230; 64 agents pacing p95 about 19 ms, was 0.4-2.5 s stalls, at about 20% less total throughput).
- The real-time versions of the schedule tests: `cargo test -p specula --lib schedule -- --ignored --nocapture`.

# Profiling tools

- **CPU zones** (`specula_core::profile`): `let _z = profile::zone("sim");` around a phase. The engine
  thread's frame is `frame` with children `autosave` (`autosave.plan`), `sim` (`tick`, `tick_hash`), `game_frame`,
  `draw_list` (with `sync_meshes`), `parked_captures` and `present` (which waits for vsync). The autosave worker records
  `autosave.write` on thread `specula-autosave`. `perf_zones` lists them (latest frame plus rolling
  mean/p95; paged; `enabled` switches recording). Agents' tool calls are top-level `requests` zones outside `frame`
  (`sight.render`, `sight.encode`, `sight.save` inside), since the window serves them between presents, so CPU frame
  budgets don't include them.
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

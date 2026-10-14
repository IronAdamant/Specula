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

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
  statistics (when the engine has it), working-set memory, validation errors and a clean shutdown.

**Take baselines on an idle machine.** The script records whole-machine CPU use before and after and
warns above 20%: with parallel builds running, single calls show 15–25 ms scheduler delays instead of
under 1 ms, and build timings double.

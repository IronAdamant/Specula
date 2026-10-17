# Headless playtests and playtest swarms

Status: built 2026-10-18 (M3: headless playtests, headless playtest swarms, the exit proof's 20+ replays). Code:
`crates/specula/src/playtest.rs` (spec, runner, replay format), `crates/specula/src/playtest_engine.rs` (tools,
engine glue), `scripts/playtest_swarm.py` (the coordinator), `scripts/playtest_suite.py` (Sepulcra's suite).

## Why

QA is where swarms beat studios: one agent per path through the game, many engines at once, one report. A playtest
has to be **data** (an agent writes it, another reruns it), **deterministic** (the same spec in a fresh engine makes
the same calls at the same ticks, F2) and **verifiable** (a replay checks every hash, not a final screenshot). Before
this, only Sepulcra could do it, through its own `play_*` tools and a Python script that stepped time call by call.

## A playtest is a spec

A spec calls ordinary tools (engine or game) at ticks, asserts on their answers, and ends after a number of ticks or
when a condition holds:

```json
{"name": "centre_s7", "game": "sepulcra",
 "steps": [{"at": 0, "call": "play_new", "args": {"seed": "7", "layout": "centre"}},
           {"at": 9000, "call": "sight_frame", "args": {"width": 640, "height": 360}, "label": "mid"},
           {"at": 9001, "call": "play_command", "args": {"action": "place", "stone": "warding", "cell": "top:10,7"},
            "expect_error": "not_buildable"}],
 "until": {"call": "play_state", "expect": {"outcome": {"ne": "running"}}, "every": 30},
 "ticks": 40000,
 "end": [{"call": "play_state", "expect": {"outcome": "won", "wave": 10, "tombstones": {"gte": 1}}}]}
```

- **Inputs over time** are tool calls: `play_command`, `logic_input`, `world_set`, `physics_impulse`, anything an agent
  calls. They run as the agent that started the playtest, through the same path (claims, journal, undo).
- **Assertions**: `expect` maps a dotted path into the answer (`enemies.items.0.hp`, `#` for a length) to a value or
  to `{eq, ne, lt, lte, gt, gte, in, exists, contains, near + tol}`. `expect_error: true | "code"` expects a refusal
  (matched against the error's `error` or `code`).
- **Captures**: any answer with `image.path` (`sight_frame`) is listed with its label and tick.
- **Time**: the playtest owns the clock. It pauses it, steps tick by tick to each `at`, and refuses other agents'
  `time_control` (except `info`) until it ends; ticks queued by an earlier `step` are dropped.
- **Bounds**: a spec is at most 256 KB of JSON, with 256 steps, 64 end steps, 32 expectations a step, each matcher
  at most 2 KB and each path 256 characters. A run is at most 1,000,000 ticks, `max_wall_s` (default 600, at most
  3600) and 64 file-writing calls (`sight_frame`, `sight_orbit`, `sight_compare`, `perf_capture`, and the game's
  own, which it declares with `Game::file_tools`: Sepulcra's `play_record`). Past that a call fails with
  `capture_limit`, and the engine never sees it. Captures made by `until` calls are listed too. Reports quote values cut to 240 characters and have
  byte budgets: failures 6 KB concise and 48 KB detailed, the whole report 128 KB. The engine keeps 16 finished
  reports, and at most 32 callers wait on a run.
- **Ownership**: only the agent that started a playtest (by identity key) cancels it. Another agent needs
  `force: true`, and the cancel is logged on the board's event feed.
- **Replay files** are written only under `playtests/` in the engine's working directory, only as `.json`, and never
  over an existing file unless `overwrite: true`. A link or junction inside `playtests/` can't carry the write
  elsewhere: the resolved folder must still be under `playtests/`. Sepulcra's `play_record` follows the same rule: `.json` only, and
  no overwrite unasked.

`playtest_run {spec | path | replay, record?, wait_s?}` starts one (one at a time per engine) and answers when it ends
or after `wait_s` (at most 20) with its progress; `playtest_report {id?, wait_s?, cancel?}` polls, waits or cancels.
Both take `response_format`. A headless engine runs a playtest flat out between calls (Sepulcra: about 50,000 ticks a
second, a whole ten-wave run in under a second); a windowed one gives it 4 ms a frame, so the window stays fluid.

## Replays verify by hash

At the start, every `checkpoint_every` ticks (default 300; it grows so a run keeps at most 1,024) and at the end, the
run records a checkpoint: the world's state hash, the physics hash and the game's subsystem hashes (what the desync
detector sees), combined. `record` writes them with the spec into a **replay file** (`format: specula-playtest`).
`playtest_run {replay}` runs that spec again and compares every checkpoint; the first that differs stops the run as
`diverged` and names the parts that differ (then `desync_trace` and `desync_compare` find the component and field).

However a run stops (a failure with `stop_on_failure`, an `until` call that errs, a cancel, `max_wall_s`), its end
is hashed, so the replay of a failed run verifies up to where it stopped and reproduces the failure.

The start checkpoint is compared but not required to match: a spec that begins with `play_new` may start anywhere. The
world part, though, includes the id counter and every persistent entity, so a replay only verifies in an engine in
the same starting state: start engines with **`--paused`** (nothing ticks before the playtest) and use a fresh engine
per run. Measured: the same replay in a used engine diverges at the first checkpoint, on `world` only, while every
game part matches.

## Playtest swarms

`scripts/playtest_swarm.py` coordinates through a lead engine:

1. one task per playtest on the **task board** (`task_post`, tag `playtest`);
2. each worker (a thread, `agent_hello` as `playtester-N`) **leases** two ports (`lease_take {kind: port, count: 2}`,
   pool 7500-7599) and a scratch folder (`lease_take {kind: scratch}`);
3. a worker takes a ready task, starts a **fresh** headless engine (`--headless --paused`, working directory and
   pipeline cache in its scratch folder) on one port, runs the spec with `record`, stops it, starts another fresh
   engine on its other port and runs the replay; it finishes the task with a one-line summary (`task_done`);
4. at the end the workers release their leases (scratch folders deleted), engines are stopped by PID (Ctrl+Break,
   then kill), and one report gathers every playtest's status, failures, ticks, wall time and verification; replays
   and captures are copied next to it.

## The M3 exit proof

`python scripts/playtest_suite.py --serial` runs Sepulcra's 26 playtests: every built-in layout over several seeds,
asserted to end the way the design says (centre wins all ten waves, no anti-air falls at wave 6, walling every gate
falls at wave 4-5, no defence falls at wave 2), scripted strategies (early wave calls, selling mid-game, walls without
stones, the game's pause and speed 4, HUD buttons, dry runs, refused moves with their codes, the game's own recording
replayed, captures) and two engine-only playtests (a crate settling under physics checks, a world edit undone).

Result (2026-10-18, RTX 4090 laptop, a machine shared with other sessions): **26 of 26 passed, 26 replays verified by
hash in separate fresh engines**. On the lane as merged with Lex (`perf/playtests/20261018-053843-sepulcra-suite.json`)
the suite took 46.2 s wall with 8 engines in parallel against 313.9 s with one, 6.8 times faster. An earlier run before
that merge (`20261018-045650`) took 58.6 s against 241.9 s, 4.1 times faster. After review round 1, on main
`715e55f` (`20261018-064836`), it was again 26 of 26 passed and verified, with the negative control diverging, in
75.9 s against 491.2 s (6.5 times faster) on a busier machine. Load from other sessions moves both
figures. A replay recorded in a **windowed engine on the Intel iGPU** also verifies in a headless engine on the
discrete GPU, with the same end hash as a headless run (`af131660c03bdebc`, centre layout, seed 7). With the window
open, the playtest ran at about 8,400 ticks a second and the window's frame median stayed at 10.5-11.5 ms. The negative control (the centre seed-7 replay with its seed
changed and its hashes kept) diverged at tick 300 on `game.sim`, as it must. A playtest itself takes 4 ms to 1.3 s;
engine start-up (2-5 s, Vulkan with the validation layer) and shut-down dominate each job.

## Follow-ups

- **Engine reuse**: a reset to a fresh world (and the game restarted) inside one engine would cut the per-run start-up
  and let a worker record and verify without restarting. The world hash's dependence on history is the obstacle.
- **Balance lab**: sweeps are playtests with parameters; the swarm script is its runner.
- **Agent evals**: an eval is a playtest whose spec checks the world an agent built; the same runner and report.
- **Lex**: `lex_*` tools are refused inside a playtest for now. They validate and simulate on the caller's
  connection thread and then wait for the engine thread, which the playtest is running on, so the call would
  deadlock. Edit rule sets before the run. To let specs set rule parameters per run (the balance lab), `lex_edit`
  needs an engine-thread path that a step can take.

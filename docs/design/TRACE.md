# Tracing: a structured, correlated log for agents and humans

**Owner, 2026-10-29:** "It won't hurt to have a debug logger inbuilt, so you and your agents can monitor and trace
everything. I believe in game engines, traceability is everything. Especially to you and other AI agents."
And, the same day: "I think, autonomous debugger could be used, perhaps? Not just have it be inbuilt and it could
be in operation alongside the agents." The second half is the debugger role ([DEBUGGER.md](DEBUGGER.md)); this page
is the engine side.

Before this, the engine had the change journal (`world_history`), profile zones (`perf_zones`, `perf_capture`),
`desync_trace`, per-system explainers, and about 260 unstructured `eprintln!` lines that agents grep out of
engine.log. Nothing tied a symptom (a slow frame, a refused write, a wrong packet) to the agent call that caused it.

## What it gives an agent

- **"Everything my call 1234 caused"**: `trace_call {call: 1234}`. Every MCP call gets a call id; the engine-thread
  work, worker jobs, write-policy decisions, journal entries and GPU errors it causes carry the id.
- **"Why was this write refused?"**: the refusal is an event (`write.refused`) with the rule, the error code, the
  path and the agent; non-routine refusals become an incident.
- **"What stalled frame 5120?"**: `trace_query {frame: [5120, 5120]}`; stalls and over-budget frames are events
  (`engine.stall`, `frame.over_budget`) with the call, the tool or zone and the top passes.
- **Incidents, not just events**: the watchdogs group related events into deduplicated incidents
  (`trace_incidents`), which a debugger agent triages and turns into critiques or tasks.
- **Crash reports**: the last events before a panic, a module panic, a fatal engine error or device loss are
  persisted, and `trace_crash` reads them after a restart.

## Where it lives (and why)

| Part | Where | Why |
|---|---|---|
| Events, levels, subsystems, spans, call context, per-thread buffers, the central ring, filters | `specula_core::trace` | Every engine and simulation crate already depends on `specula-core`, so any of them can emit without a new dependency in shared manifests (five lanes edit them at once). Spans open the profiler's zones (same names), which live next door in `specula_core::profile`. |
| Call table, agent names, sinks (JSONL file, stderr), crash reports, incidents, monitors | `crates/specula/src/trace_*.rs`, `incidents.rs` | Engine concerns: files, sessions, budgets from `perf.toml`, the window loop. |
| Agent tools `trace_*` | `crates/specula/src/trace_tools.rs` | Answered **on the caller's connection thread**, never the engine thread: a stalled engine can still be traced. |
| Module API | `specula_sdk::Cx::trace` | Modules are cdylibs with their own copy of `specula-core`, so their events cross the C ABI as a request; the host emits them under `module:<name>`. |

A separate crate (`specula-vestigium`) was considered: it would have to be added to every emitting crate's manifest,
and still depend on `specula-core` for zones and JSON. A module in core costs nothing to adopt.

## Events

```rust
use specula_core::{trace, trace_event};
static SUB: trace::Subsystem = trace::Subsystem::new("write_policy");
trace_event!(&SUB, Info, "write.refused", rule = "world", code = e.code, agent = w.name);
```

- **Level**: `error`, `warn`, `info`, `debug`, `trace` (and `off` as a setting).
- **Subsystem**: a static name (`engine`, `mcp`, `write_policy`, `sight`, `gpu`, `jobs`, `module:ambulo`).
- **Code**: a static snake_case/dotted machine code (`engine.stall`, `write.refused`), like error codes: agents
  filter on it, nobody parses text.
- **Fields**: typed (`bool`, integers, floats, strings), at most 16, strings cut at 256 characters.
- **Stamps** added by the trace layer, never passed by the caller: `call`, `span`, the engine's `tick` and `frame`
  (global, set by the engine loop), `time_us` since the trace epoch, `thread`.

**Cost.** Each subsystem holds its maximum level in one atomic: a disabled event is one relaxed load and a
compare; its fields are never evaluated and nothing allocates (a test pins this with the counting allocator). An
enabled event builds its fields (one small allocation) and is pushed into its thread's bounded buffer (512 events,
a `try_lock` only the drainer contends). Buffers drain into the central ring when full, when a reader asks, and
every 100 ms from the sink thread. Measured numbers are in "Cost" below.

**Determinism (F2).** Simulation crates may emit, but nothing in tracing feeds back: `emit` returns nothing, the
module request always answers `null`, and the level check guards only building the fields. Simulation code never
reads or passes wall time; the trace layer stamps it. A headless simulation on another thread (a `lex_simulate`, a
balance sweep) can stamp its own tick with `trace::set_thread_tick`; nothing calls it yet (phase 2: those crates emit
nothing in phase 1). `world_hash` and replays are unchanged at full verbosity (checked by `scripts/trace_check.py`,
which also traces every tick's hash at `trace` level).

## Spans

`trace::span(&SUB, Level::Info, "studio.render")` opens a profile zone of the same name (so `perf_zones` and the
trace line up) and, when the subsystem is enabled at that level, a span: events inside carry its id, and its close
is one event with its duration, its parent span and any fields attached. Spans nest per thread. A job queued for a
worker captures `trace::Ctx::current()` (call and span) and enters it on the worker, so the worker's spans hang
under the call's tree.

## Correlation

Every MCP call: `Tools::call` takes a call id (`trace::new_call`), records `{id, session, agent, tool}` in the call
table (4,096 calls kept), and enters the call's context for the call's duration. The context then travels:

- **to the engine thread**: `Request` carries it; `Engine::handle` (and parked captures when they're served) enter it;
- **to jobs**: model builds (`models::Job`, a `model.build` span on the worker), studio renders (`RenderJob`, a
  `studio_render` span) and save writers (`save.write`) capture it when queued and enter it where they run;
- **to write-policy decisions**: `write_policy::commit`, `undo`, `game_commit` emit `write.refused` (info: rule,
  code, message, path) and `write.committed` (debug: rule, journal id, changes; debug so a swarm's thousands of
  writes a second don't push stalls out of the ring). A writer's own error (a missing entity) is `write.failed` at
  debug: only the policy's refusals are decisions;
- **to the journal**: each `journal::Entry` stores `call` (0 for the simulation's own writers), shown by
  `world_history {response_format: "detailed"}`, and the trace keeps a `(call, entry)` index so `trace_call` lists
  the entries without the engine thread;
- **to the GPU**: validation messages taken by a capture are `gpu.validation` events under the capture's call.

When the call ends its record gets the duration and the error code, if any. Agent filters resolve through the
call table (agent → call ids).

Not yet covered (phase 2): module builds on the build service's thread, vfx preview encoders (stepped from the
studio's pump, outside the call), streaming uploads (no call causes them), the board's save thread, workers that
`lex_simulate` and the balance lab start inside other crates.

## Sinks

- **The ring**: in memory, 8,192 events (oldest overwritten, counted). Everything below reads from it.
- **JSONL files**: `<trace dir>/trace-<start ms>-<pid>-<part>.jsonl` (default `.specula-trace/` in the working
  directory, `--trace DIR`), one event per line, each line naming its engine (`pid`, `port`, `run`: engines started
  from one folder share it), a new part past 4 MB, the newest 8 files kept in the folder (32 MB at most). On by
  default at `info` for the engine (`--trace-file off`); `trace_settings {file}` changes it. Written through the disk
  book (`disk::book().append`, category `cache`: the trace folder is a second cache root, measured by the walker
  but never pruned by it, because the sink bounds itself and a crash report must survive until it is read).
- **stderr** for humans: `warn` and up by default, one line per event (`specula: warn engine.stall call=12 ...`).
- **Crash reports**: `<trace dir>/crash-<unix ms>.json`, the newest 8 kept: the reason (`panic`, `module_panic`,
  `engine_error`, `device_lost`), the message, the location, the engine (pid, port, run), the context, and the last
  256 events. A panic hook (chained before the previous one) writes them. It never blocks: it only `try_lock`s the
  trace's state, and writes through `disk::book().write_reserved` (not admitted, accounted if the book is free that
  instant): a reserved budget the count bounds. Panics inside `specula_core::trace::catch_unwind` (every engine
  `catch_unwind` goes through it) are caught, structured errors: they are `caught-<unix ms>.json` reports, reason
  `caught_panic`, 4 kept, so they never push a real crash out. `trace_crash {caught: true}` reads them.
- **Device loss** (window or headless): one crash report and a `gpu.device_lost` incident per run.

**Arguments are never kept verbatim.** A call's digest records each top-level key with its value's type and size
(`name:str(4)`, `ops:[300]`, `props:{2}`); only numbers and booleans keep their values; keys naming secrets
(`token`, `secret`, `password`, `auth`, `key`...) are always `<redacted>`. Keys are cut at 32 characters and the
digest at 160, built before any lock. Holding no text, a digest is **public metadata**: every agent sees it (in
`trace_call`, events, incidents, `perf_frame`'s recent stalls, crash reports), and `trace_query {text}` searches it
like any field. A review probe (2026-10-29) read another agent's `resume_token` out of the old digest, which kept short
strings, and took over its identity: never again (tests check no string value ever reaches a digest). M6's per-agent
auth tokens travel in a request header, never as a tool argument.

## Watchdogs and incidents

Monitors run continuously, each with a budget, and raise **incidents**: deduplicated by `(code, key)`, with first
and last seen (time, tick, frame), a count, the related calls, agents, frames and ticks (8 of each kept), the latest
evidence (numbers, capture and packet paths), and a status: `open`, `acknowledged`, `resolved` or `recurring` (seen
again after it was resolved). At most 256 are kept (resolved ones go first). An incident's events are rate-limited
to one a second; its count still goes up.

### Stalls: the structural check

The scheduler (`schedule.rs`) only decides when a call *starts*; every call then runs to completion on the engine
thread. So every request is timed where it is handled (`Engine::handle_traced`, called by both loops' serve step;
parked captures when they are served; the window frame's own work after it). One past its **class budget** is
`engine.stall` with the call id, the tool, its class, the milliseconds and an **argument digest** (each top-level key
with a short scalar, a list's length or an object's size, at most 160 characters, built in O(keys) when the call
starts: `{width:2048, height:2048, max_subjects:1}`). `perf_frame.trace.recent_stalls` shows the last 8.

| Class | Budget | Why |
|---|---|---|
| read, edit | 4 ms | They normally cost tens of microseconds (the schedule's floor is 50 µs). A quarter of a 60 Hz frame is already more than the schedule's 0.5-3 ms margin before the present: one such call pushes the present past its vblank. |
| render, disk, big, sim, game | 30 ms | They legitimately cost about a frame. Two 60 Hz frames is where one dropped frame becomes a hitch a human sees. |
| a window frame's own work | 30 ms | Past it the window visibly stutters, whatever caused it. |

The budgets are data (`trace_settings {budgets_ms}`, lead only). **`--strict-budget`** makes a stall an `error`
event and fails the run: the engine exits with an error naming the count, so live scripts and evals can assert on
it (the integrator turns it on in scripts later). Instances found on main by the stall investigation (2026-10-29),
which this catches: `lease_release {delete: true}` on 20k files (8.5 s), `anim_check` with 256 duplicate clips
(1.2 s), `anim_filmstrip` at 16×320 px (322 ms), `sight_frame` at 2048² (290 ms of PNG encoding),
`world_save` with 20k entities (310 ms). `scripts/trace_check.py` confirms the `sight_frame` one. Fixing them is a
separate lane's (a standard worker API, bounded arguments).

### Monitors

| Monitor | Code | Budget | Key |
|---|---|---|---|
| A call on the engine thread past its class budget | `engine.stall` (warn; error with `--strict-budget`) | one comparison per call | the tool |
| A window frame's CPU work past the frame budget | `engine.stall` (warn) | one comparison per frame | `frame` |
| A change of the window's size (the swapchain's rebuild) past the frame budget | `engine.stall` (warn), and always `window.resized` (info) with its `ms` | one comparison per resize | `window_resize` |
| The window's GPU time past `perf.toml`'s `gpu_ms` (sampled twice a second), with the top passes | `frame.over_budget` (warn) | one stats read per 500 ms | the cause: `pass:<name>` when one pass alone takes more than the whole budget, else `spread` (load or contention: one incident, not one per pass) |
| A refused write: routine conflicts (`claimed`, `claims_overlap`, `stale_version`) only past 20 in 10 s; anything else (`reserved`, `transient`, ...) at once | `write.refused` / `swarm.refusal_spike` (info) | per refusal | code and rule |
| Sight packets: a subject with `visible_px > 0` that this frame's colour passes never recorded (`pixels_not_drawn`). The record is the GPU's own (`Gpu::last_colour_draws`: the mesh and 2D draws the colour passes actually recorded, after every pipeline and variant readiness check), independent of the ID buffer's counts; `DrawnRecord` maps it to entities, and composes with the authority lane's `sight::FrameRecord` when that merges. Also an exact box outside the image or the projected box, or more pixels than the box holds (up to 64 subjects a packet) | `sight.packet_inconsistent` (error) | O(subjects + draws) per packet | the tool and the problem |
| World replay spot-check: started after 2 s without calls (at most once a minute, only if the world changed), then **a step of about 2 ms per loop pass**, calls or not: changes applied 64 at a time (a 1,000-spawn entry is split across steps), then the replayed world hashed 64 entities at a time. The live hash and the journal's newest entry are taken at the start, so the game may tick on meanwhile. Only journals whose base world has at most 5,000 entities (copying the base is one step; a world built in this run has an empty base); a replay estimated (from the last one's cost per entry) to need over 10 s in all is skipped for 10 minutes. Each step is timed against the frame budget like any engine work, and is never a `--strict-budget` failure (the engine's own background work) | `world.replay_mismatch` (error) | about 2 ms per loop pass while one runs | `world` |
| GPU validation messages taken by captures | `gpu.validation` (error/warn) | per message | the message id |
| Jobs that stay in flight without finishing for 60 s (model builds, parked captures, save writers; studio queues are not sampled yet) | `jobs.stuck` (warn) | one sample a second | the job kind |
| GPU memory past `perf.toml`'s budget | `memory.over_budget` (warn) | one sample a second | `gpu` |
| A module disabled after a panic or a runaway | `module.disabled` (error) | per event | the module |

Disk budget crossings join once `disk::book()` merges.

## Agent tools (family `trace`)

All answer concisely by default (counts and the first events), are paged, byte-capped (32 KB), and run on the
caller's thread. They don't wait for the engine thread, but they do take the short locks it also takes (the ring,
the call table, the incidents): reading costs the engine a little, so poll every few seconds, not in a loop.

| Tool | Does |
|---|---|
| `trace_query` | Filter by level, subsystem, code (prefix), agent, call, tick, frame or time range, or text; newest first, paged by `before`. |
| `trace_follow` | Events after a cursor (the session's own by default), like `world_changes`. |
| `trace_call` | One call's record and causal tree: its spans, events, jobs and journal ids. |
| `trace_settings` | Levels per subsystem, the default level, the sinks and the stall budget. Global changes: the lead only (`trace_tools::authority`, today the lead or director role; replaced by `authority::authority(swarm, session)` with operator grants when the authority lane merges); `scope: "session"` sets this session's own view (the default filter of its queries). Names: at most 64 characters of `a-z 0-9 _ . : * -`. |
| `trace_crash` | The newest crash report (or the list of them), readable after a restart. |
| `trace_incidents` | Incidents, filtered by status or code. |
| `trace_incident` | One incident in detail; `action: ack / note / link` (anyone; an ack confers nothing) or `resolve / reopen` (an agent the incident names, a reviewer or the debugger role, or the lead; an incident naming no agent, only a reviewer or the lead), linking a `critique` or `task` id. |

Catalog lines (for the scoped-tools lane's catalog, when it merges):

- `trace_query`: search the engine's structured log.
- `trace_follow`: new log events since your cursor.
- `trace_call`: everything one call caused.
- `trace_settings`: log levels and sinks.
- `trace_crash`: the last crash report.
- `trace_incidents`: open problems the watchdogs found.
- `trace_incident`: one incident; acknowledge, resolve or link it.

## Games and modules

- **Modules** call `cx.trace(TraceLevel::Info, "door.opened", Value::map().with("door", id.0))`; the event's
  subsystem is `module:<name>`. At most 64 events a call; codes and field names must match `^[a-z0-9_.]{1,64}$`,
  and the engine's code prefixes (`engine.`, `write.`, `frame.`, `gpu.`, `module.`, ...: `ENGINE_CODE_PREFIXES`)
  are refused, so a module's event never passes for a watchdog's. A module names at most 64 distinct codes and
  fields. `cx.log` lines are also `module.log` events.
- **A game linked into the engine** (Sepulcra) uses `specula_core::trace` directly.
- **A shipped game** keeps the core (it costs one atomic per disabled event) with every subsystem at `warn`, no file
  sink, and the crash report on: a player's crash leaves a JSON file a developer (or an agent) can read. A game that
  wants none of it sets the default level to `off`; the macros then cost a load and a branch. Stripping at compile
  time is not offered: the check is already cheap, and a stripped build can't explain its own crash.

## Phase 2 (after the current lanes merge)

Migrate the remaining `eprintln!` lines to events (subsystem and code each). A guard test
(`crates/specula/tests/print_guard.rs`) fails when an engine source file gains a `println!` or `eprintln!`, with
an allow-list of today's counts, so the count only goes down.

## Cost

Measured 2026-10-29 on the reference laptop while three other lanes compiled (`cargo run --release -p specula-core
--example trace_bench` repeats it):

| What | Cost |
|---|---|
| A disabled event (fields not evaluated) | 0.8-1.1 ns |
| A disabled span (its profile zone included) | 9-11 ns |
| An enabled event with 3 fields | 630-730 ns: about 240 ns builds and frees the fields (the system allocator, under load), about 90 ns reads the clock |
| An enabled span | 730-910 ns |
| An enabled event, 4 threads at once | 2.1-2.6 µs each (allocator and ring contention) |

At `info` the engine emits a handful of events per agent call and none per frame unless a monitor fires, so the
frame cost is the disabled path's. Sepulcra on the Intel iGPU (window, dev build, `trace_check.py --cost`, 240-frame
`perf_capture`s alternating `info` and `off`): the quietest pair was frame p95 15.4 ms / GPU p95 9.4 ms at `info`
against 16.7 / 9.1 ms `off`; the other pairs moved by tens of milliseconds both ways with the compilers, so tracing
is below what this machine can resolve under load. A quiet-machine run with a release build is worth repeating.

If enabled events ever matter (a swarm at `debug`), the next step is inline fields (no allocation) and a coarser
clock.

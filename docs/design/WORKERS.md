# Workers and step jobs: keeping the engine thread free

Status: built 2026-10-30 (the bounds lane). Code: `crates/specula/src/workers.rs` (the worker pool and
`Engine::offload`), `crates/specula/src/steps.rs` (`Engine::step_job`). Guard:
`crates/specula/tests/engine_thread_guard.rs`. Measured by the strict-budget harness
(`scripts/budget_check.py`).

## Why

One engine thread owns the world, the GPU and the journal. Agents' calls are served between the
window's presents (`schedule.rs`), and the tracing lane times every call against its class budget
(`engine.stall`: 4 ms for reads and edits, 30 ms for renders, disk work, big batches; see
`trace_sink::Budgets`). A root-cause investigation (2026-10-29) found that the budgets were being
broken by the same mistake over and over (recurring bug class C):

| Instance | Measured | What held the engine thread |
|---|---|---|
| `lease_release {delete: true}` | 8.5 s for 20k files | deleting a scratch folder |
| `anim_check` with 256 copies of a clip | 1.2 s | every check in one call |
| `anim_filmstrip` | 322 ms | every tile rendered, read back and encoded in one call |
| `sight_frame` / `sight_orbit` / `sight_compare` at 2048² | 290 ms | the PNG encode |
| an explicit `world_save` at 20k entities | 310 ms | serialising and writing the world |
| `world_hash {verify: true}` | 150 ms | replaying the whole journal |
| project mirroring | per file | `fsync` of each written file |

The root causes:

1. The scheduler gates when a call *starts*; nothing bounds a call once it runs.
2. There was no standard way to move work off the thread: fourteen hand-rolled threads, each with
   its own answer path, slot counting and failure handling (or none).
3. A blocking operation looks like an ordinary function call, so nothing stops the next one.

## The rule

> Engine-thread code never touches the disk, encodes an image, deletes a folder or loops over
> unbounded work. It snapshots what it needs and hands the rest to a worker (`offload`), or splits
> work that needs the engine (the GPU, the world) into budgeted steps (`step_job`).

The guard test makes the rule mechanical (below); the strict-budget harness proves it at the
largest arguments every schema allows.

## `Engine::offload`: one-shot work off the thread

```rust
let job = Offload::agent(JobKind::CaptureEncode, owner, &session.id).lane(Lane::Any);
self.offload(job, move |cancel| encode_and_write(sheet, path), |engine, result| {
    // On the engine thread, once the work is done (or failed, or was cancelled).
    let _ = reply.send(answer_from(result));
})?;
```

- **work** (`FnOnce(&Cancel) -> R + Send`) runs on a worker thread. It may not touch the engine:
  it owns what it was given (pixels, a write plan, paths). It may poll `cancel.is_set()` in loops.
- **then** (`FnOnce(&mut Engine, Result<R, Failed>)`) runs on the engine thread when the result
  is back (`Engine::pump_workers`, once per loop pass in both loops). It must be cheap: it applies
  a result (stamps, the saved-world cache) and answers. It is timed like any engine-thread work
  (`engine_work_done`, zone `worker_then`).
- **The answer** goes through `jobs::Reply` (the only way to make an answer channel): `then`
  usually owns the reply and sends it; a job that needs no engine follow-up answers from the worker
  (`offload_answer`). Either way every deferred answer has one owner and one sender.

### Bounded slots

- Two worker threads (`WORKERS`), started on first use and named `specula-worker-N`. The
  reference machine (Intel iGPU laptop) has four cores: the engine thread, the window thread and
  the protocol's connection threads need the rest.
- Agent jobs: at most `MAX_AGENT_JOBS` (64: one per agent of a 64-agent swarm, each waiting for its
  answer) queued or running, at most `MAX_PER_OWNER` (4) per agent (by identity key, which a resume
  keeps), and at most `MAX_PENDING_BYTES` (1 GiB) of pixels held between them. Past any of these,
  `offload` refuses with `job_busy` (naming whose jobs fill the queue, `retry_after_s`), and the
  caller answers with it *before* doing any expensive engine-thread work (a capture checks
  `workers_room` before it renders).
- A playtest's own steps (call 0) never count against agents' slots: one playtest runs at a time and
  holds the clock while it waits (review of 998af3d: a flood of agents' calls failed a playtest).
- Engine jobs (writes the engine owes: project mirroring, the trash, world files, replay and perf
  files, drops) are never refused and never run inline: an inline 20,000-file delete is the stall
  this pool exists to prevent (review of 998af3d). Their sources are bounded instead: one world
  write, one project sync and one trash emptying at a time (the trash is one job however many
  folders are released), a replay or a perf capture at a time. Work runs inline only when no
  worker thread could be started.

### Lanes

Writes to the same place must not overlap. A job names a `Lane`: `Any` (encodes), or a serial lane
(`Lane::World` for world files, `Lane::Project` for project mirroring, `Lane::Trash` for deletes):
jobs of one serial lane run one at a time, in the order they were offloaded; other lanes and `Any`
jobs run beside them.

### Cancel, drop and failure

- **Session end**: `pump_workers` checks the live sessions about four times a second; an agent
  job whose session closed is cancelled. A queued one never starts (`then` gets
  `Err(Failed::Cancelled)`); a running one sees `cancel.is_set()` and may stop early.
- **Drop guard**: `then` owns its reply. If the engine shuts down with jobs pending, they are
  drained first (`Engine::workers_drain`, up to 10 s: writes the engine owes complete); anything
  still pending is dropped, and dropping a reply answers its caller "shutting down" (the reply
  channel closes). A pending job is never silently lost.
- **Panics**: the work runs under `catch_unwind`; a panic becomes `Err(Failed::Panicked(message))`
  and `then` answers `worker_failed`. The worker thread survives.
- **No thread**: if the pool can't start a thread, work runs inline (it still completes).

### What the trace sees

Each job carries its call's trace context (`trace::Ctx`): the worker enters it, so events the work
raises follow the call (`trace_call`), and `then` is timed against the call's class.

## `Engine::step_job`: work that needs the engine, in slices

Some work can't leave the thread: it renders on the GPU, reads the world or the journal. It runs as
a `StepJob`:

```rust
trait StepJob {
    fn kind(&self) -> JobKind;
    /// One slice: at most about a millisecond or one GPU render and readback.
    fn step(&mut self, engine: &mut Engine, reply: &Reply) -> Step; // More, Done(answer), Handed
}
```

- Started by `Engine::step_job(start, job)`: its first slices run at once, for up to 2 ms, so a
  small job answers in its call and is never refused; one that must queue is refused (`job_busy`)
  past `MAX_STEP_JOBS` (64) agents' jobs or `MAX_STEPS_PER_OWNER` (4) per agent. A playtest's
  steps always queue. Journal replays (`world_hash {verify}`) also take one of `MAX_REPLAYS` (2)
  slots (each holds a copy of the journal's base world); the rest wait their turn cheaply, and
  replay towards the journal as it was at their call.
- Pumped once per loop pass (`pump_steps`), round-robin over jobs, within a credit like the
  studio's: earned at a share of wall time (`WINDOW_SHARE` 0.25 with a window, `HEADLESS_SHARE`
  0.8 headless), spent by each step's measured time. In debt nothing runs, so a slow step on a
  busy iGPU is followed by frames without step work.
- A step returns `Step::Done(answer)`, `Step::More`, or `Step::Handed` (it offloaded its finish,
  an encode, with a clone of the reply).
- Past its caller's deadline a job answers `timed_out`; when its session closes it is dropped
  (answered `session_closed`).

This generalises what the studio's renders and `vfx_preview` already did (a frame per step within
a render credit). The studio keeps its own queue (its renders answer the studio's caller-thread
waits, not a tool reply), and `vfx_preview`'s sheet encode now goes to the pool instead of its own
thread.

## What moved (C1-C7)

| | Before | Now |
|---|---|---|
| C1 `lease_release {delete}` | `remove_tree_inside` on the engine thread | the folder is **renamed** in place to `.<name>.trash.<pid>.<n>` (one rename in the same folder, never through a link; a shared trash folder made renames wait behind the delete), then the whole trash is emptied by one coalesced job on `Lane::Trash` through `disk::remove_tree_inside` and accounted in `disk::book`; leftovers from a crash are swept at start |
| C2 `anim_check` | every check in one call | a step job: one subject (clip, state or transition) per step; duplicate clips are refused by the schema (`uniqueItems`) |
| C3 `anim_filmstrip` | every tile, readback and the encode in one call | a step job: a row's poses and checks per step, one tile render per step, the sheet's encode and writes offloaded |
| C4 `sight_frame`, `sight_orbit`, `sight_compare` | PNG encode, base64 and file writes inline | render and readback stay (they wait on the GPU, see below); encode, base64 and the PNG and packet writes are offloaded; the answer is sent from the worker |
| C5 explicit `world_save` | `persist::save_for_agent` inline (and a blocking wait for an autosave in flight) | the engine plans (takes the changed subtrees, as the autosave always did), the write runs on `Lane::World`, `then` updates the saved-world cache and answers; an autosave in flight is waited for by queueing behind it on the same lane, never by blocking |
| C6 `world_hash {verify}` | a full journal replay inline | a step job like the trace's replay spot-check: the replay a few entries per step, then the replayed world hashed in steps |
| C7 project mirroring | `write_entry` (temporary file, `fsync`, rename, read-back) on the engine thread | the sync is planned on the engine thread (`Disk::sync_plan`: which paths, which texts), written on `Lane::Project` (`SyncPlan::execute`, which stamps each file as it writes it, so the watcher never mistakes the engine's write for an outside edit), and applied when it returns (`Disk::sync_apply`: pending writes, held files, races imported, hot reload); `project_write` and `project_patch` answer once their file is on disk, as before |

### Ordering: project calls wait for a sync in progress

When syncs ran on the engine thread, the engine's view of the disk (stamps, pending writes, held files) only
changed between whole syncs. To keep that, the project calls that read or write the disk (`Snapshot`, `Commit`,
`ModuleTexts`, `CommitFixes`) are held in order while a sync writes (`project_deferred`) and run when it ends;
edits the watcher found wait in its channel the same way. A `project_mount`, which answers in its own call, waits
for the sync (rare). Otherwise a commit's `refresh` could read a file the worker had just renamed into place but
not yet stamped, and import the engine's own older text as an outside edit.

Autosaves and explicit saves share `Lane::World`; an explicit save asked for while one is writing waits in a
short queue (4), and `world_load` writes the waiting saves first.

### What stays on the engine thread, and why

- **A capture's render and readback.** `Gpu::read_rgba8` waits on the GPU fence: the engine
  thread owns the Vulkan queue and the offscreen targets. Making it asynchronous (a readback
  ring, answered a frame later) is a GPU-side change for a later lane; the strict-budget harness
  records its cost at 2048² (render class, 30 ms budget).
- **The plan of a world save** (taking the changed subtrees off the world) and **a project sync's
  plan**: they read the world, which only the engine thread may.
- **Loading** (`world_load`, `save_read`'s apply): a load replaces the world; its read and parse
  already run on a worker (`save_tools`), and `world_load` is a lead-gated, rare call.

## The guard

`tests/engine_thread_guard.rs` scans the engine crate's non-test code (the shared lexer of the
write-policy and disk guards: comments, literals and test items blanked) for blocking calls in
engine-thread modules: `fs::` calls, `sync_all`, `remove_dir_all`, PNG encodes, `read_rgba8`,
`thread::Builder` / `thread::spawn`, and since the review of 998af3d disk work through the engine's
own helpers (`disk::book()` writes, appends, copies and folders, `remove_tree_inside`,
`remove_file_inside`, `persist` reads and writes) and file-system stats (`.exists()`,
`.metadata()`, `.is_file()`, `.is_dir()`, `.canonicalize()`, `.read_dir()`). Imports that would hide
`fs` calls (`use std::fs as x`, `use std::fs::{..}`, `use std::{fs, ..}`) are refused everywhere.
Moved to workers in that review: `desync_trace`'s render and write, a playtest's replay file, and
`perf_capture`'s files. Each file may make only the calls its allow-list entry
counts, with the reason (a worker-side module, a caller-thread tool, start-up, a bounded small
read); a new one anywhere else fails, as does a count that grows. Moving a call to a worker
lowers the count, and the test then asks for the lower number, so the list only shrinks.

## The strict-budget harness

`scripts/budget_check.py` starts a headless engine with `--strict-budget`, builds a large world
(20k entities, a 20k-file scratch lease, a long journal), then calls every tool twice: with
schema-maximal fuzz (the largest string and array each property allows: the bounds must refuse or
bound it cheaply), and the heavy tools with their largest *valid* arguments (2048² captures, the
largest orbits and filmstrips, 256 distinct clips, a 1000-op batch, a 20k-entity save, a verify
over the whole journal, a 20k-file delete). It reads `trace_incidents` and fails on any
`engine.stall`, and the engine's exit status (a strict run fails at shutdown when a stall
happened). The integrator runs it at milestone audits.

### What it found (2026-10-30, Intel iGPU, a dev-profile build, 20,000 cubes)

The C1-C6 instances no longer stall: `lease_release {delete}` of 20,000 files answers in 32-47 ms of
wall time with nothing on the engine thread past budget, `world_hash {verify}` runs in about 100
one-millisecond steps, the explicit `world_save` no longer blocks on an autosave. What the harness still
reports, each a finding for a later lane (none is a regression; each was there before, hidden behind
the bigger stalls):

| Stall | Engine-thread time | Why |
|---|---|---|
| `sight_frame`, `sight_compare` at 20k entities | 250-500 ms | the subject list (`sight::subjects`, about 90 ms for 20k entities), the render's fence wait and the readback: O(entities) per capture, at any image size (256x256 costs nearly as much) |
| `sight_orbit` (12 views) | 2.5-4 s | 12 such renders in one call: an orbit should become a step job (a view per step), once a view itself fits a budget |
| `world_query` / `world_list` at 20k | 11-40 ms (read budget 4 ms) | filters scan every entity |
| `vfx_get` after big world changes | 35-45 ms | the effect library's rescan of every entity |
| `world_save` plan at 20k loose entities | 36-49 ms (disk budget 30 ms) | the plan copies every changed top-level entity |
| `anim_check` / `anim_filmstrip` single steps | 30-60 ms | one state check (3 cycles) or one tile on a loaded iGPU: the smallest slice `specula-anim` offers |
| `desync_trace` of 600 ticks at 20k entities | 230-270 ms | gathering the ticks from the journal (its 3-12 MB file is rendered and written on a worker since the review of 998af3d) |

`budget_check.py` marks these KNOWN and any other stall NEW (with its zone); `--known-ok` passes a run
whose only stalls are known ones, which is how the milestone audit runs it until the follow-up lane
lands.

### Window frames under heavy calls (`scripts/stall_load.py`)

Sepulcra windowed on the Intel iGPU (1280x720, seed 7), one agent looping the heavy tools (2048x2048
`sight_frame`, a 12-view orbit, the largest filmstrip, `anim_check`, `world_save`, `world_hash
{verify}`, `lease_release {delete}` of 20,000 files, `project_write`), a poller reading the window's
frame pacing every second; release builds of main (d679814) and this lane, alternating; no builds or
other engines running (the usual desktop apps open):

| Run | Worst window frame | Median of per-second p95 |
|---|---|---|
| main, 1 | 4,407 ms | 8.9 ms |
| bounds, 1 | 170 ms | 9.0 ms |
| main, 2 | 6,378 ms | 9.2 ms |
| bounds, 2 | 188 ms | 9.2 ms |

The multi-second frames were the scratch delete; what is left (150-190 ms) is the orbit and the
2048x2048 capture's render and readback (above). The cost: an `anim_filmstrip` of 128 tiles now takes
about 6 s of wall time with a window (its steps get a quarter of the engine thread) instead of 0.4 s
in one stall.


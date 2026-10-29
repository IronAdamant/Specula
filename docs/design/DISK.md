# Design: the disk footprint budget

Status: built 2026-10-29 (M4 lane "disk footprint budget"). Owner, 2026-10-22: sort it out "before it impacts
someone else's system ... especially when they have even less space". Foundations **F1** (swarm-first: a swarm must
not fill the operator's disk) and **F3** (low-end machines have small disks too). Roadmap item: "Disk footprint
budget" in the swarm track of [ROADMAP.md](../ROADMAP.md). Code: `crates/specula/src/disk.rs` (the book),
`disk_tools.rs` (`disk_status`, `disk_prune`), `blobs.rs` (the content store), `specula_sys::disk` (free space);
guard: `crates/specula/tests/disk_guard.rs`; live check: `scripts/disk_check.py`.

**Why.** On 2026-10-22 about 30 build folders (one per lane and reviewer, each 5-15 GB) took a 1 TB drive from
340 GB free to full, and a lane's link failed with "No space left on device". Operators with small disks shouldn't
find out the hard way, and cleanup done under pressure is how real work gets deleted.

## One accounting point

Everything the engine writes goes through `disk::book()`, one per process:

- `book().write(category, source, path, bytes, need)`: admitted, written (folders made), accounted.
- `book().admit(category, bytes, need)` before, and `wrote` / `replaced` after, for writers that write their own
  way: atomic world saves, save games, the task board and review queue (temporary file, then rename), the content
  store, project files (`project::write_entry`), module builds (Cargo; admitted before the build, measured after).
- `book().make_dir` and `book().copy` for folders and copies.

The guard test fails when engine code, the hosted game's, the module host's (`specula-logic`) or any other crate
the engine calls writes or deletes files outside the files listed in it: `fs::write`, `File::create`,
`File::options`, `OpenOptions::new`, `fs::copy`, `fs::rename`, links, `create_dir_all`, `fs::create_dir`,
`fs::remove_file`, `fs::remove_dir(_all)`, `Command::new` (another program writes where it likes), and
`use std::fs::...` imports or `use std::{.. fs ..}` groups that would hide them. Patterns are matched as bare paths
in each file's code with all whitespace removed across lines, so a function value, a turbofish, a qualified form
or a call split over lines is seen too (the round-2 review's nine dodges are a test).
Each listed file must itself use the book. `specula-logic` can't depend on the engine, so its writes (loaded
library copies, fixes written into a module's sources, a restored `Cargo.lock`) go through
`specula_logic::disk`, a hook the engine installs at start. Folders are removed only by
`disk::remove_tree_inside` (below).

**Who writes what** (the categories, each with its folders):

| Category | Folders | Writers | Pruned |
|---|---|---|---|
| `captures` | `--captures` (every run's `run-<secs>-<pid>`) | sight frames and packets, orbit and compare sheets, studio sheets, VFX previews, anim filmstrips, procgen images, perf captures, desync traces, Lex exports | yes: oldest first |
| `cache` | `--cache` (minus the two below) | the pipeline cache | yes (via pruners) |
| `content` | `<cache>/content` | the content store: binary project files' versions and uploads in progress | yes: what nothing needs |
| `modules` | `<cache>/modules` | module builds' Cargo targets and loaded library copies | `logic_build` drops stale artefacts |
| `scratch` | `[swarm] scratch_root` (default `<captures>/scratch`) | scratch-folder leases | expired leases, after a grace period |
| `saves` | `--saves` | save games | never |
| `worlds` | `--worlds` | world files, the task board, the review queue | never |
| `project` | the assets root and mounted modules | project files (the store's size is reported; the folders are never walked) | never |
| `recordings` | `playtests/` and `recordings/` in the engine's working directory | playtest replays (`playtest_run {record}`), the hosted game's recordings (Sepulcra's `play_record`, which writes only under `recordings/`) | never: they are evidence and replays |

A walk of one category never counts another's folder inside it (the default scratch root is inside the captures
root; the content store and module targets are inside the cache).

## Measuring: bounded and cheap

- **Writes move the numbers at once** (the book's per-category deltas), and are tallied per writer for
  `disk_status {detailed}` (`written_this_run`: sight, studio, vfx, procgen, playtest, ...).
- **A background walker** (`specula-disk` thread) measures each category's folders a slice at a time: 2,000
  entries, then a quarter-second pause, at most 250,000 entries and 32 folders deep per walk (past that the numbers
  are marked `partial`), each category again 30 s after its last walk. The disk is read outside the book's lock,
  so writers never wait on a walk. No tool call ever walks a tree, except `disk_status {measure: true}`, which
  runs at most once per 10 s.
- **Free space** comes from the OS (`GetDiskFreeSpaceExW`, what this process may use under quotas, and
  `GetVolumePathNameW` so folders on one drive count once), read again after 2 s or for a write of 64 MB or more,
  and lowered by what was admitted since. Other platforms report `free_space: {unsupported}` instead of guessing
  (`std` has no `statvfs`); the floor is then not enforced there.

## The budget (`perf.toml [disk]`)

```toml
[disk]
total_mb = 16384        # the engine's own output, all categories (default 16 GiB)
free_floor_mb = 2048    # agents' writes are refused below this much free space (0 turns it off)
warn_percent = 90       # the soft line: a share of the total and of each cap
captures_mb = 4096      # per-category caps: captures, cache, content, modules, scratch (saves_mb... too)
content_mb = 2048
cache_mb = 1024
modules_mb = 8192
scratch_mb = 8192
scratch_grace_s = 3600  # an ended scratch lease's folder is removed this long after it ended
capture_min_age_s = 300 # captures younger than this are never pruned
```

- **The soft line.** A write that takes the total or a category past `warn_percent` of its line, or leaves free
  space under twice the floor, is answered with a structured `disk_warning` (`code: disk_budget`, `message`,
  `crossed`, `biggest`: the largest categories with their folders and what to do about each, `hint`). Every session
  hears it once (in the answer to the write that crossed it, as a rule; another session on its next call), and
  again when it gets worse (a new line crossed). It rides on the answer's first text item (a sight tool's packet,
  after its image). Answers to sessions already told are passed through untouched
  (no parse), and free space is read from the OS outside the book's lock. `engine_info.disk` and `disk_status` carry it while it lasts.
- **Over a line, the walker prunes**, at most every 30 s, down to 85% of the line it crossed, in this order: the
  oldest captures, cache entries (content nothing needs), expired scratch. A lead can free more with
  `disk_prune {free_mb}`; anyone can prune what the budget needs or `dry_run`. A real prune by an agent runs at
  most once per 10 s per engine (`too_soon` with `retry_after_s`; only a prune that ran starts the wait, never a
  dry run or a `busy` answer); one prune or dry run runs at a time (`busy`).
- **Only what helps.** A prune first works out what is actually prunable now (captures past `capture_min_age_s`
  in this run's and finished runs' folders, content a pruner can give back, expired scratch; never live scratch or
  the pipeline cache) and frees at most that: `need = min(total - line, prunable)`, likewise for a cap or the
  free-space line. A prune that leaves the total over budget is remembered; the walker doesn't prune again until
  use grows past it (review findings: fresh captures were wiped every 30 s for nothing, live scratch counted as
  prunable, and a prune that would have helped partly didn't run at all).
- **Never what was just written:** captures younger than `capture_min_age_s` (default 5 minutes) are never
  pruned; an agent is still looking at them, or about to name them in a review.
- **Reviews keep their own copies** (below), so pruning captures never takes a reviewer's evidence.
- **Below the floor, agents' writes are refused** (`disk_floor`): `category`, `volume`, `needed_bytes`,
  `available_mb`, `floor_mb`, `engine_use_mb`, `what_to_delete` (the biggest categories and what to do) and how to
  prune. Refused: `project_upload` (at its start, before any bytes move), `project_write` / `project_patch` (before
  the commit), `lease_take {kind: scratch}`, `save_write`, `logic_build` (before Cargo runs), the content store,
  captures. A sight tool still answers its image inline: only the file isn't written, and the answer says why.
- **Essential writes go through anyway**, counted (`essential_writes_below_floor`): the engine's autosave of the
  current world (and at shutdown), the task board and the review queue beside the world. Losing them costs more
  than their bytes. An agent's `world_save` is an agent's write: refused below the floor (review finding: saves to
  new names went through it).
  A project file committed before space ran out waits on disk as `pending` (retried every second), never lost.

## Review evidence

Round-2 review: protecting reviewed captures by reference kept being fooled (a `world_load` didn't refresh the
set, another world's queue wasn't known at all, rebuilding the set cost a second per critique with thousands of
them, a relative path pointed at the wrong run after a restart). So the risky dependency is gone instead:
`review_submit` and `critique_add` copy each capture and packet into the queue's own evidence folder,
`<world>/evidence/` beside `review.json` (code: `evidence.rs`), and the queue names the copy. World files are never
pruned, so evidence lives exactly as long as the queue names it, whatever happens to the captures; no protected set
exists any more, and nothing the disk budget prunes is ever named by a review.

- **One confinement** (round 3): every path a review names goes through `safe_path::review_file`, for
  `review_submit`, `critique_add`, the review screen and every resolution of a review's paths. Lexical checks come
  first (no UNC, verbatim, drive-relative or `..` name ever reaches the disk: no credentials sent, no existence
  answered); then only a capture (`capture_file`) or a file directly in a world's `evidence` folder, never a world's
  own files or anything else under `--worlds`; no link on the way.
- **Relative references** (round 3): the queue names `evidence/<hash>-<len>.<ext>`, relative to its folder, never an
  absolute path: a copied or moved world keeps its evidence, and no home path lands in world files. `review_list`
  shows those references, which `review_file` resolves.
- **Off the engine thread** (round 3): the copies are made on the calling agent's connection thread before the
  engine sees the call; the engine only checks they are there. Within a call each file is read once, an existing
  copy is read only when its name and length match, and one call copies at most 64 MiB (`evidence_budget`).
  `world_save` under a new name copies the queue's evidence on a worker thread. A call that doesn't come through
  a connection thread (a playtest step's `review_submit` or `critique_add`, run on the engine thread in tick order)
  reads at most 4 MiB of evidence in all, refused before the read (`evidence_budget` with `engine_thread: true`):
  a few milliseconds of the engine's time at most, and plenty for a playtest's frames (integration check,
  2026-10-29: such a step could copy 64 MiB on the engine thread).
- **Refused calls leave nothing behind** (integration check, 2026-10-29: a refused `review_submit` left its copies
  for good, and the 2 GiB cap's advice couldn't free them). The arguments are checked before anything is copied;
  a call refused after copying (a journal entry, a claim, a later file of the same call) hands what it copied back
  to the engine, and collection removes those copies like dropped evidence, unless the queue names the same content.
  A copy a new call finds already there is marked as just used, so collection can't take it from under that call.
- **Cost.** One copy per distinct file (the same PNG submitted twice, or critiqued, is one file). Captures are
  typically 50-500 KB and contact sheets a few MB; each evidence file is at most 16 MiB and a world's evidence at most
  2 GiB (`evidence_full`), counted under `worlds`. Copies, not hard links: a hard link shares the capture's bytes, so
  pruning the capture would free nothing while the budget counted the bytes twice, and links are what the deletion
  rules refuse to follow.
- **Collection never guesses** (round 3): only files this run's queue named and then dropped (decided submissions
  and resolved critiques it lets go at its caps), or a refused call copied, are removed, at most once a minute (again
  a minute on while some wait to be old enough), only files older than a minute;
  nothing at all while the load reported a problem, the queue isn't being saved, or an `.unreadable-*` queue sits in
  the folder (an unreadable queue set aside keeps its evidence).
- **Unsaved worlds** keep their queue's evidence in this run's `<worlds>/.review-unsaved-run-<secs>-<pid>/evidence/`,
  with a heartbeat file (`.alive`, every minute). `world_save` under a name copies the evidence into the world.
  Folders whose heartbeat is ten minutes old are removed at start, on every platform and whatever their process id
  (a reused pid can't keep one alive). They are gitignored.
- **Loading** (`--world`, `world_load`) adopts the queue's evidence: old absolute references (queues saved before
  this) are matched against `evidence/<file name>` with the same bytes first, then copied in while their files
  exist. A reference whose file is gone is reported: `missing: true` on the capture (`capture_missing` /
  `packet_missing` on critiques) in `review_list` and `critique_list`, the review screen's image note, and a warning
  in the `world_load` answer.

## Never pruned

Saves, world files and journals (review evidence among them), project files, playtest replays and game
recordings, captures younger than
`capture_min_age_s`, another running engine's captures (only this
run's and finished runs' folders are candidates), scratch folders still leased or released with `keep: true`, and
content a current file, a kept journal entry or an upload still needs.

## Deleting safely

Deleting is the dangerous part, so every delete:

- is built from the root's **canonical** path and must lie strictly inside it (never the root itself);
- checks every step from the root down with `symlink_metadata`: **no symlink, junction or other reparse point**
  anywhere on the way (`disk::check_inside`);
- removes a folder with `remove_tree_inside`: after those checks, `std::fs::remove_dir_all`, which removes links
  met inside **as links** and, on Windows, works from open folder handles, so a folder swapped for a junction
  while it is being emptied can't lead the delete outside (a test races exactly that);
- is never handed a path from an agent: candidates come from the engine's own bounded walks, which skip links.

Tests: a junction inside a capture run pointing at a folder outside is not counted by the walk, not followed by
pruning, refused as a delete target, and removing the scratch folder that holds one removes only the link
(`disk::tests::junctions_out_of_a_root_are_never_followed`); `..` and paths outside a root are refused. The live
check does the same with a real engine. The old capture-run pruning at start-up now uses the same helpers.

## Module builds and scratch leases

- **One Cargo target per module**, shared by every agent's builds of it: `<cache>/modules/<folder>-<hash>`
  (`logic::tools::module_target`). Cargo locks it, so concurrent builds take turns instead of making copies.
- **`logic_build` drops stale artefacts** after each successful build: in each profile's `deps`, `.fingerprint`
  and `incremental`, only the newest two of each crate and file kind stay (a check build and a full build each keep
  theirs). The answer says what it dropped (`stale_dropped`).
- **Scratch leases expire and are cleaned up.** Each lease folder carries a marker (`.specula-lease`: lease,
  owner, `ends_unix`, `keep`), rewritten on renew and release. A folder whose lease ended more than
  `scratch_grace_s` ago is removed by the walker (checked every minute), unless released with `keep: true`.
  Only a folder named as a lease names it (`<owner>-<lease>-<secs>-<pid>`) whose marker says the same lease,
  owner and process is a candidate, and never one whose lease is live in this engine: a marker dropped into any
  other folder gets nothing removed. Folders without a marker are never touched. `lease_release {delete: true}`
  removes one at once.

## Tools

| Tool | What it does |
|---|---|
| `engine_info.disk` | The state (`ok`, `warn`, `over_budget`, `below_floor`), the budget, use per category, volumes, the warning, pruned and refused counts, the journal's binary history. Concise: state, budget, warning, refused. |
| `disk_status` | The same, with `detailed`: each category's folders, when it was measured, bytes written this run per writer, the prune log; the content store and open uploads. `measure: true` re-walks now (at most once per 10 s). |
| `disk_prune` | Prunes what the budget needs now (and expired scratch); `dry_run` lists what would go; `free_mb` frees at least that much (lead only). |

## A swarm guide: keeping lanes off the operator's disk

What filled the 1 TB drive was build output, not the engine: every lane and reviewer building the whole workspace
into its own `target/` (5-15 GB each, about 30 of them). The engine's budget covers what the engine writes; for the
builds around it:

- **Share build folders.** Give each lane one `CARGO_TARGET_DIR` (on a disk with room) and reuse it for that lane's
  reviewers; never one per agent. Linux clippy (`--target x86_64-unknown-linux-gnu`) can share the lane's folder.
- **Limit jobs.** Build and test with `-j 4` when several lanes build at once: past that the machine runs out of
  commit memory ("The paging file is too small") before it runs out of cores. `CARGO_INCREMENTAL=0` saves several
  GB per folder when space is short.
- **Clean merged lanes.** When a lane or review is merged or finished, delete its target folder (the integrator
  does; reviewers delete their own scratch builds). Delete only folders you made: never anything outside your own
  target or scratch folder.
- **Check before you start.** `disk_status` (or the operator's free space) before deploying a swarm; set
  `[disk] free_floor_mb` to what the operator needs left for their own work.
- **Scratch through leases.** Engines a swarm starts take scratch folders with `lease_take {kind: scratch}`; they
  are cleaned up after their grace period, so a crashed agent's leftovers don't accumulate.

## Later

- Free space on macOS and Linux (`statfs` through the platform layer, with the native ports).
- A per-agent share of the budget (who filled it), from `written_this_run` per session.
- Budgets for the content store's history per file (today: one journal budget for all binary history, 1 GiB).

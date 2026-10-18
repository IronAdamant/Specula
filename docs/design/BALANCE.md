# Design: the balance lab

Status: designed and built 2026-10-18 (M3 lane "save games and the balance lab"): the `specula-balance` crate and the
engine's `balance_*` tools; the same day, several play styles at once and a check of tuned results by playtests in the
game itself. Foundations **F1** (swarm-first) and **F2** (determinism). Roadmap item: "Balance lab" in
[ROADMAP.md](../ROADMAP.md); research: [AGENT_GAMEMAKING.md](../research/AGENT_GAMEMAKING.md) ("Balance by simulation
works, but heuristics mislead"); the rules it tunes: [LEX.md](LEX.md) (`param` items, `lex_simulate`'s `params`).

**Goal:** a designer (person or agent) says what the game should feel like **in numbers**; the lab measures a rule set
against them over many seeded headless runs, sweeps param values, and tunes params toward the targets, then checks the
result on seeds the tuner never saw. Results are data. Every result is flagged `needs_human_playtest`, because a
scripted policy isn't people: simulated balance is a hypothesis to test, not a verdict.

## Targets as data

A targets document (JSON, `*.targets.json`; the format version is `balance`):

```json
{ "balance": 1, "name": "slice_close_call", "ticks": "12m",
  "seeds": { "train": 24, "holdout": 24, "base": 1 },
  "policy": { "every": "1s", "wait_on": ["not_enough_dust"], "steps": [
      { "command": "place", "args": { "stone": "warding_1", "site": "top:8,14" } },
      { "command": "place", "args": { "stone": "rune_pillar_1", "site": "top:8,16" }, "when": "res.dust >= 75" },
      { "command": "upgrade", "args": { "stone": "top:8,14" }, "after_wave": 2 } ] },
  "targets": [
      { "id": "wins", "metric": "win_rate", "target": 1, "tolerance": 0.1 },
      { "id": "tombstones_left", "metric": "expr", "expr": "res.tombstones", "target": 7, "tolerance": 0.75 },
      { "id": "runner_time_to_kill", "metric": "time_to_kill", "archetype": "runner", "target": 5, "tolerance": 2, "weight": 0.25 } ] }
```

| Metric | Fields | Per run | Unit |
|---|---|---|---|
| `win_rate` | | 1 if the outcome is `won` | fraction of runs |
| `outcome_rate` | `result` | 1 if the outcome is `result` | fraction of runs |
| `wave_cleared` | `wave` (1-based) | 1 if that wave was cleared: **win rate per wave** | fraction of runs |
| `leaks` | `wave?` | wave entities removed without dying (reached the goal), in that wave or all | entities |
| `kills` | `wave?` | wave entities that died | entities |
| `resource` | `resource`, `at?` | the resource then (or at the end): several targets at several times make a **curve** (the dust curve) | the resource's unit |
| `time_to_kill` | `archetype?` or `tag?` | mean seconds from spawn to death | seconds |
| `wave_clear_time` | `wave` | seconds from the wave's start to its clearing | seconds |
| `end_time` | | seconds until the outcome | seconds |
| `expr` | `expr` | any Lex global expression at the end (`res.tombstones`, `count(enemy)`) | its human unit |

A target's value is the **mean over the seeds** where the metric is defined (`time_to_kill` with no kills isn't),
with stdev, min and max. It **hits** when `|mean − target| ≤ tolerance`; its loss is
`weight × ((mean − target) / tolerance)²` (undefined everywhere costs nine, as if three tolerances off); a point's loss
is the sum. Tolerances make targets in different units comparable.

**The policy** is who plays: a build order as data, generic over rule sets. Every `every`, steps are tried in order:
a step is due when its `at` time has passed, wave `after_wave` has started and its `when` condition (a Lex expression)
holds; a due step's command is applied; refused with a code in `wait_on` it waits for the next try (saving up dust);
refused otherwise it is skipped and counted (`policy_steps_skipped`); a step not yet due stops the try. Gating on
resources (`when: "res.dust >= 75"`) makes the build's timing follow each seed's drops, so seeds matter.

**Seeds:** `train` (tuning sees these) and `holdout` (only validation does); a seed in both is an error.

## Several play styles

One build order is one player. `"policies"` (instead of `"policy"`) lists up to eight, each with a `name` and an
optional `weight`:

```json
"policies": [
  { "name": "dust_gated", "every": "1s", "wait_on": ["not_enough_dust"], "steps": [ ... ] },
  { "name": "braziers", "steps": [ ... ] },
  { "name": "rusher", "weight": 0.5, "steps": [ { "command": "call_wave", "args": {}, "at": "5s" }, ... ] } ]
```

Every point is played by **each** policy on the same seeds (points × policies × seeds runs). A target holds for all
policies unless it lists the ones it means (`"policies": ["rusher"]`); it **hits only when it hits under each**, and
its loss is the policies' weighted sum, so tuning looks for rules that feel right however people play rather than
for one script. Results keep each style's own numbers (`policies: [{policy, loss, hit, targets, outcomes}]`), so a
point that is perfect for one build and broken for another says which. Names are checked (unknown ones get the closest
name), and one unnamed `policy` reads and writes as before.

**Validation, twice.** `Targets::parse` checks the document's shape (unknown fields with the closest name, missing
fields, ranges, `win_rate` targets above 1 with "write 0.6"); `Targets::bind` checks it against a compiled rule set
(resources, archetypes, tags, outcome results and waves exist; commands take their args; expressions compile; times
become ticks at the rules' rate). Both report every problem at once as Lex diagnostics (`code`, `item` = the target's
id, `path`, `expected`, `got`, `suggestion`).

## Runs, sweeps and determinism

A run is the rule set compiled with a point's params (`lex_simulate`'s overrides, range-checked, folded into the
rules hash), one seed, the policy, until an outcome or `ticks`. Lex simulation is pure, so a sweep runs its (point,
seed) jobs **in-process across threads**: workers take job indices from a counter, results are stored by index, and
every mean is summed in seed order. The numbers are therefore the same on 1 thread or 16 and on every machine
(determinism rule 7; `sweeps_do_not_depend_on_the_thread_count`). A wall-clock deadline stops workers between runs (a
run cut inside a tick is never counted): a point is reported only when all its runs finished, and the answer says
where to continue.

## Tuning

Bayesian optimisation over chosen `param` items, zero-dependency and deterministic:

- **Space.** Each param over its declared range or a narrower `min`/`max`, on a grid in its stored unit (about a
  thousand 1-2-5 steps by default, or the caller's `step`: `1` for whole points), normalised to `[0, 1]`.
- **Model.** A Gaussian process on `ln(1 + loss)` (standardised), squared-exponential kernel, a small fixed noise
  term. The length scale is **not** fitted by gradient steps or random restarts: it is the most likely of a fixed grid
  (3 % to 100 % of the range) by marginal likelihood, so the model follows how bumpy the objective is (step-like
  metrics such as leaks call for short scales) and stays a pure function of the data.
- **Design.** The rules' current values first, then a Halton sequence, `2 × params + 1` points (at least 3).
- **Proposals.** The point with the most expected improvement among 512 candidates: a Halton set (offset by the
  evaluation count) and perturbations of the best point from a PCG stream seeded by `(seed, evaluations so far)`.
  Points already evaluated are skipped; a small exhausted grid steps to the best point's nearest unseen neighbour.
- **Resumable.** A proposal depends only on the evaluations so far and the seed, so a job is its data
  (`TuneJob::to_json`: params, grid, budget, seed, every evaluation with its exact loss bits): stop after any call,
  read it back and carry on, and get exactly the evaluations of an uninterrupted job (tested).
- **Validation.** The best point is run on the held-out seeds and reported beside its training score.

Why not TPE or CMA-ES: with a few params and expensive, low-noise evaluations (each is 16-24 runs averaged), a GP uses
evaluations best; TPE shines with many cheap, noisy ones. The kernel grid keeps it deterministic without the fitting
machinery that makes GP libraries non-reproducible.

## Tools

Like `lex_simulate`, the tools run on the calling agent's protocol thread plus worker threads, never on the engine
thread, which only hands out a snapshot of the rule set (`lex_*` stores). All balance calls share a cap on worker
threads (the machine's threads less two, at most 16): a call gets what is free up to its `threads`, and when every
worker is taken it answers `busy` with `retry_after_ms` instead of starting, so a swarm of tuners can't starve the
window or each other. A missing `targets_file` is `not_found`.

| Tool | What it does |
|---|---|
| `balance_targets {targets \| targets_file, set? \| document?}` | Validates a targets document; with a rule set, binds it and lists the tunable params. No arguments: the metric catalogue and the format. |
| `balance_sweep {set \| document, targets \| targets_file, points \| grid, seeds?, start?, threads?, runs?}` | Scores param points (a list, or a grid: every combination of `{param: [values] \| {from, to, steps}}`) over the train, holdout or listed seeds. ≤ 64 points and 16,384 runs per call, 25 s wall budget; continue with `start: next_start`. `runs: true` also lists every run (≤ 128 a call): seed, policy, outcome and its tick, end resources, each target's value, and the policy steps it applied with their ticks. |
| `balance_tune {set \| document, targets \| targets_file, tune, budget?, seed?, job?, evals?, state?, validate?}` | Tunes; `job` keeps it in the engine (≤ 4 per session, 16 in all; only its session may continue it) and a later call continues it; `state` resumes from data. Refuses to continue a job when the rules or targets changed (`job_changed`). Validates on held-out seeds when done; `held_out_status` says `done`, `not_requested` or `unfinished` (the wall budget ran out first: call again, `finished` is true only when both the tuning and the held-out check are done). |

Every answer carries `needs_human_playtest: true` and a note.

## Proof: the Sepulcra slice

`games/sepulcra/lex/slice.targets.json` asks the Lex slice for a close call under a dust-gated build order: still won,
about seven of ten tombstones left, runners taking a few seconds to kill. As written (`runner_hp` 26) the build wins
with all ten tombstones (loss 16.7). `games/sepulcra/tests/balance.rs` tunes `runner_hp` over
40..160 (step 1, budget 14, seed 7) on 24 training seeds: the best point is **118 pt** (train loss 0.42, every target
hit), and on 24 held-out seeds it hits every target too (tombstones left 6.50 ± 0.88 against 7 ± 0.75, runner time to
kill 5.9 s, win rate 1.0; loss 0.49). `scripts/balance_check.py` (port 7602) repeats it through the engine's tools:
the tune runs as a kept job over two calls, and the bystander agent's `world_list` keeps answering during sweeps.

**Three play styles** (`games/sepulcra/lex/slice_styles.targets.json`: the same close call asked of the dust-gated
build, a brazier-first build and a rusher who calls waves early; 16 + 16 seeds). The `runner_hp` 118 tuned above
for one build order is a close call for that build (6.75 tombstones left) but **loses every brazier-first run** and
leaves the rusher two tombstones: one policy hid it (`targets_must_hold_across_play_styles`). No `runner_hp` alone
fits all three (the brazier build falls behind against runners first). Tuning `runner_hp` and `shambler_hp` together
(budget 30, seed 7; `tune_styles`, ignored, 3 s in release) finds **61 / 139**, which hits every target under every
style on the training seeds (loss 0.91); on the held-out seeds the dust-gated and brazier builds still hit (win rate
1.0 and 0.94) and the rusher misses its win rate (0.875: two of sixteen runs lost). That is the lab's answer: the
rusher's risk is real, and a human playtest should decide whether calling every wave early ought to be that risky.

## Checked by playtests in the game

The lab runs Lex in-process. `scripts/balance_playtests.py` checks a result where players meet it: **Sepulcra's own
game** (its hand-written rules, its tools, fresh engines).

1. A lead engine imports the slice and calls `balance_sweep {runs: true}` at the point: every run with its seed,
   policy, outcome, end tick, dust and tombstones, and the steps it applied with their ticks (a run's trace replays it
   with no policy at all: `a_runs_trace_replays_it_without_the_policy`).
2. Each run becomes a playtest ([PLAYTESTS.md](PLAYTESTS.md)): `play_new` with the seed and game data carrying the point's values
   (`runner_hp` → `enemies.json`, `start_dust` → `waves.json`, ...; a param with no counterpart is refused), each
   applied step as a `play_command` at its tick, until the outcome; the end asserts the outcome, the tick, dust and
   tombstones the lab measured. A step the game refuses fails the playtest at that step.
3. `scripts/playtest_swarm.py` runs them through the lead's task board on parallel fresh engines (workers lease one
   port each from `--pool` when replays aren't verified).
4. The report compares, per style, the lab's win rate and tombstones with the game's, and lists every run that
   disagrees with what differed. Any disagreement exits 1.

`--out` is the script's own folder: it empties it only when it holds the script's marker file and refuses any other
non-empty folder. Every engine it starts is listed in `engines.pid` from the moment its process exists until it has
stopped cleanly (PID, start time and full exe path). Ctrl+C and Ctrl+Break stop them all; after a hard kill the next
run against the same `--out` stops whichever are still running before it starts, a PID only when the process with it
has the same start time and exe path (a reused PID is another program and is left alone). If the folder still can't
be emptied, the script says which PIDs may still be running instead of failing with a traceback.

**Measured** (2026-10-18, release, 3 engines on ports 7601-7603): runner_hp 61 / shambler_hp 139, 16 held-out seeds
× 3 styles = **48 playtests, 48 agree** (dust-gated win rate 1.0 and 6.69 tombstones left in both, braziers 0.94 /
6.56, rusher 0.875 / 6.5), in 141 s wall (415 s of playtests). The comparison can fail: with `--skew shambler_hp=40`
(the game's data only) every run disagrees at once (an upgrade refused for lack of dust, then the run lost). A skew
of +1 or +5 `runner_hp` changes nothing in either sim (runners still die to the same number of bolts), so it isn't
flagged: the check compares what a player sees (outcome, timing, dust, tombstones, refusals), not hit points.

**Throughput** (release, i9-13900HX shared with other sessions, the slice's 12-minute game, about 8,000-10,000 ticks a
run): 82 runs/s on one thread, 249 on 4, 664 on 8, 1,075 on 16 (`cargo test -p sepulcra --release --test balance --
--ignored --nocapture throughput`). Through the tools (120 runs, so start-up shows): 38-102 runs/s on 1 thread,
202-349 on 16.

## Honest limits

- A policy is a script, so balance found against it is balance for that script. Several policies narrow that, and
  playtests in the game check the port, but neither is people: a human's recorded build as a policy (from Sepulcra's
  `play_record`) is the next step, and human playtests are the check.
- Playtests in the game need a mapping from the Lex slice to the game (params to data files, commands to
  `play_command`), written for Sepulcra in the script; another game needs its own.
- The slice has little randomness (drops only), so seeds vary little; Sepulcra's full port (lane jumps, flyers'
  circling) will make held-out checks stricter.
- Step-like metrics (whole leaks and tombstones) make plateaus that slow any optimiser; the kernel grid helps, and a
  `step` keeps the grid coarse where the game is.
- Tuning runs headless Lex only: rule sets with native hooks are refused (`native_headless`), as in `lex_simulate`.

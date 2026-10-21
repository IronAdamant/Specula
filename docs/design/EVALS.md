# Design: Specula agent evals

Status: designed and built 2026-10-18 (M3, the agent-evals lane). Code: `scripts/evals.py` (the runner),
`scripts/evals_test.py` (its unit tests), `evals/` (the tasks), `crates/specula/tests/evals_suite.rs` (the task files
and the shared matcher checked in `cargo test`). Results: `perf/evals/`. Roadmap item: "Specula agent evals" in
[ROADMAP.md](../ROADMAP.md); research: [AGENT_GAMEMAKING.md](../research/AGENT_GAMEMAKING.md) ("Tool design for
agents": evaluate tools with agents on realistic, verifiable multi-step tasks and study the transcripts, including what
agents leave out). Siblings: [PLAYTESTS.md](PLAYTESTS.md) (the runner verifiers reuse), [TOOLS.md](TOOLS.md) (the
audit whose open items the evals feed).

## Why

Tools are a product, and a product needs a regression suite that measures what users actually do. Specula's users are
agents, so the suite is a fixed set of **realistic tasks** ("build a room that passes these checks", "make wave 3 10 %
harder without breaking wave 4"), each with an outcome **the engine verifies from its own data**: physics rays,
queries, ID-buffer pixels, motion checks, simulated games, playtests. Not a model's opinion of a screenshot, and not
the agent's word. Three uses:

1. **Tool regression** (`--reference`): every task has a scripted reference solution that must keep passing after
   every tool change, and a scripted broken solution that must keep failing (so a verifier that stops checking
   anything is caught too).
2. **Agent benchmark** (`--agent`): a real agent gets the brief and a port; the engine verifies what it built, and
   the transcript says how (calls, bytes, refusals, what it left out). Studying those transcripts is how the tools
   get better ([TOOLS.md](TOOLS.md) gets the findings).
3. **Public benchmark**: the public set with its scores is the honest answer to "can agents build games with it?",
   with a held-out set so the tools aren't tuned to the questions.

## A task is data

One JSON file per task in `evals/public/` or `evals/heldout/` (format version `eval: 1`):

```json
{"eval": 1, "id": "room_build", "set": "public", "area": "world", "title": "...", "summary": "...",
 "game": null,                         // or "sepulcra": which binary hosts the task
 "engine_cwd": "run",                  // or "repo" (module builds: path crates must be under the engine's folder)
 "needs": ["world_place"],             // tools the task can't run without: skipped (not failed) where missing
 "files": [{"from": "evals/fixtures/...", "to": "saltus/Cargo.toml"}],   // copied into the task's folder
 "setup": [{"as": "keeper", "call": "claim_take", "args": {...}, "save": "claim"}],
 "actors": [{"name": "keeper", "watch": {"call": "task_list", "select": {...}, "expect": {...}}, "do": [...]}],
 "brief": "the text the agent receives ({work} is the task's folder)",
 "tools": ["world_*", "sight_frame"],  // what the agent may call (patterns); agent_hello and engine_info always
 "limits": {"calls": 40, "wall_s": 600},
 "verify": [{"name": "the doorway is open", "call": "physics_raycast", "args": {...}, "expect": {"total": 0}},
            {"name": "...", "playtest": {...spec...}},
            {"name": "...", "file": "playtests/noair_s1.json", "expect": {...}},
            {"name": "...", "transcript": {"tool": "save_read", "args": {...}, "answer": {...}}},
            {"name": "...", "geometry": {"kind": "facing", "query": {...}, "target": "/table", "max_deg": 30}}],
 "reference": [...steps that pass...],
 "broken": [{"note": "a plausible mistake", ...steps that must fail...}],
 "cheats": [{"id": "fake_lanterns", "note": "how it games the verifiers", "steps": [...]}]}
```

- **Steps** (setup, reference, broken, actors' `do`) are tool calls (`call`, `args`, `args_files` to load a JSON
  file into an argument, `as` an agent name, `expect` / `expect_error` / `select` to check the answer, `save` to keep
  it), file writes (`write` + `text` or `text_file`, inside the task's folder: module source) or polls
  (`wait_until {call, expect}` with `timeout_s`). Each agent name is its own MCP session that says `agent_hello`
  once, so claims, the journal and the board see who did what.
- **Variables**: `save: "h"` keeps an answer; `"${h.entries.0.id}"` in later arguments, expectations or selects is
  replaced by that value (a whole-string reference keeps its type). `${work}` is the task's folder relative to the
  engine's working directory. Verifiers see setup's variables, never the solution's: a verifier can't depend on how
  the work was done.
- **Expectations** are the playtest runner's: a dotted path (`#` is a length) to a value or to
  `{eq, ne, lt, lte, gt, gte, in, exists, contains, near + tol}`. `select {from, where, count?}` first picks the array
  element(s) that match `where`, so a verifier needn't know the order of an answer (`subjects` of a capture, `items`
  of a query). Objects compare as maps (key order doesn't matter), arrays in order. The runner mirrors the engine's
  matcher in Python; `evals/matcher_cases.json` holds both to the same 50 cases (`cargo test --test evals_suite` and `scripts/evals_test.py`), so they can't drift.
- **`world_diff {allow: [{path, ops?, fields?}]}`: nothing changed but what the task allows.** Right after setup the
  runner snapshots every persistent entity (whole, as `world_list` gives it) and the environment; the verifier
  compares the end state with it entity by entity and field by field (props, components and Lex items down to their
  leaves; ids too, so a delete-and-recreate shows), and fails on any difference no rule allows, naming the path and the
  field (`/garden/bench: position changed`). Each task lists what it may change: room_build may add under `/room`,
  recolour_claimed may change `color` on `/street/lantern_*`, wave3_harder may change only
  `props.lex.waves.2.scale.hp` on `/lex/sepulcra/waves` (so the same verifier is the rules diff against the shipped
  set), undo_vandal may re-add the beam and change the posts' colour and scale. 20 of the 21 tasks use it, as their
  first verifier (before any verifier moves time); playtest_noair doesn't, since playing a whole game changes the world
  by design. Bookkeeping fields (`version`, authors, derived `world_position` and `bounds`) and transient entities (a
  game's presentation) aren't compared.
- **Other verifiers**: `expect_files {answer path: {file, at}}` compares part of an answer with a repository file;
  `transcript` checks the solution's own calls: `{tool, args, answer}` (it made this call successfully), `sequence`
  (these calls in this order), `equal` (two of the sequence's answers agree), `none` (calls it must not have made) and
  `count` (exactly n of a call): save_reload needs the save at 90, a hash at 180, the load, a step back to 180 and an
  equal hash, one save and no other load, none of which an end state can show; `series {setup, ticks, call, value,
  expect: {max, min, last}}` steps one tick at a time and judges the largest, smallest and last reading (a jump's
  peak, not one sample); `geometry` computes what a matcher can't from the engine's own answers: `min_gap` (pairwise
  X/Z gap between bounds), `facing` (the front, M * Z with M the product of R * diag(scale) over every ancestor),
  `world_size` (the edges of the entity's unit shape through M), `size` (bounds extents).
- **Cheats** are further broken solutions: ways to game the verifiers that a reviewer found passing once. `--broken`
  runs each as its own case (`<task>~<cheat>`), so the suite keeps proving its verifiers catch them.
- **Actors** are scripted participants: another agent holding a claim that shares it when asked on the task board or
  in a note (`recolour_claimed`). They poll a tool and run their steps when the watched answer matches (`${match}` is
  the matching element).

## Verifiable outcomes, checked by the engine

| Kind of check | Tools a verifier uses | Tasks |
|---|---|---|
| Geometry by physics | `physics_raycast`, `physics_overlap` after a tick | room_build, furniture_relations, task_board_handoff |
| Placement and state | `world_query` (positions, bounds, tags, layers, versions, authors) with `select` | lantern_row, recolour_claimed, hud_marker |
| Sight | `sight_frame` packets: `visible_px` from the ID buffer, with and without `include_ui`, through an agent's own camera (`camera_of`) | sight_hidden, camera_overview, hud_marker |
| Physics over time | a playtest of N ticks ending in `physics_check` (rest, penetration, tunnelling, energy, inside) | crate_stack, ball_launch |
| Motion | `anim_check` (foot sliding, pops, loop seams), `anim_get`, `anim_state` on a live entity | anim_fix_graph, anim_idle_clip |
| Rules (Lex) | `lex_validate`, `lex_simulate` with commands, `lex_session inspect` of a saved mid-game state | patrol_enemy, wave3_harder, lex_fix_errors, lex_tower |
| Balance | whole simulated games of the reference build (`lex_simulate` with its 22 commands; `balance_*` allowed to the agent) | wave3_harder |
| Code modules | `logic_status` and a playtest that presses the new action (`logic_input`) and measures the jump | module_jump |
| Saves and determinism | the verifier loads the agent's save itself, steps, and compares `world_hash` with the hash the agent reported | save_reload |
| Journal and swarm | `world_history` (who, `undone_by`), `claim_list`, `task_get` (owner, result), notes | undo_vandal, recolour_claimed, task_board_handoff |
| Playtests the agent wrote | `playtest_report`, the replay file's checkpoints, and the game's own state after the run | playtest_noair |
| Navigation and AI | `nav_info`, positions sampled over 20 s, `ai_status` | guard_patrol |

Verifiers check **outcomes, not methods**: an agent may place furniture with `world_place` or by hand, fix a graph
with a patch or a whole new graph, as long as the engine measures the result as right. Where a task's point is the
method (asking for a claim instead of forcing it), the outcome is chosen so that only the method reaches it (the
keeper's claim can't be bypassed, and must still be held at the end).

Gaming the verifier: each brief states the success criteria the verifier checks, verifiers measure from the engine
(a note of a hash is compared with the verifier's own replay of the save), solutions' variables are invisible to
verifiers, and `ball_launch` replays the agent's answer from the start instead of trusting the final world. Review
(2026-10-22, round 1) still found six cheap cheats that passed: fake amber lanterns outside the claim, other waves changed
along with wave 3, a playtest with no end assertions, a doorway 1.05 m tall that dodged every ray, a "reload" that
never loaded, and a wall added behind the bin. Each verifier was tightened (and three smaller gaps closed: chair
spacing, facing and table size; a second sentry after the 10 s sample; an outcome deleted and re-created under a new
id), and each cheat is now a `cheats` entry `--broken` must see fail. Round 2 found four more: chairs under a parent
group turned 180 degrees (the facing check read parent-relative rotations; it now composes every ancestor's), a save
loaded straight after writing it (the transcript check now requires the calls in order: save at 90, a hash at 180, the
load, a hash at 180), a static pillar with the crates laid out elsewhere (the crates themselves must now stand over the
spot, and nothing else may be built), and a filled square as the ring (it must be a stroked circle with no fill); and
sight_hidden now checks all six crates' positions. Round 3 found six more and named the pattern: verifiers checked
what should be there, not that nothing else changed. `world_diff` closes that class for every task that has a world
(another agent's bench moved, a lamp re-created elsewhere, a second save, a stray wall all show as differences), and
the six were fixed one by one too: a save written at 180 and loaded after the checkpoint (the transcript now needs the
step back to 180, equal hashes, exactly one save and no other load), a filled square plus a tiny stroked circle and a
fat-stroked disc (exactly one 2D shape, and that same item is a circle of radius about 1.5 m with a stroke of at most
0.25 m and no fill), a tall 1.8 m "crate" (each crate must be a 0.6 m cube of 20 kg in the world, one centre 1.4-1.6 m
up), a low-gravity jump right at tick 10 but peaking at 1.7 m (a `series` reads the height every tick), and chairs
under a parent stretched 4x (facing and size go through every ancestor's scale). The round also found that two module
tasks running at once loaded each other's library: they shared one Cargo target folder, so each module task's engine
now builds into its own.

## Scoring

Per task: each verifier passes or fails; the task passes when all do (`score: {passed, of, fraction}`). Per run, from
the transcript of the solving phase: tool calls, request and response bytes, an estimate of tokens (bytes / 4; real
counts come from the agent harness when it reports them), errors by code (refusals such as `claimed`,
`stale_version`, `tool_not_allowed`), wall time, and calls by tool. A run's `report.json` sums them; `--broken`
passes when every broken solution fails at least one verifier.

## Transcripts

Each run writes, per task: `transcript.jsonl` (every MCP call: time, agent, phase, tool, arguments, bytes, ms, error
code and the first 600 characters of the answer), `journal.json` (the engine's `world_history` at the end: who
changed what), `brief.txt` and the files the agent edited (`files/`, module tasks) in agent mode, and the report
entry. Captures stay in the engine's work folder (`<target>/evals-runs/<run>/<task>/captures`). Reports and
transcripts are committed (`perf/evals/`), so every string is scrubbed **before** anything truncates it: the known
folders become `<target>`, `<repo>` and `~`, then any remaining absolute path, Claude project folder, UUID or session
id is masked (`<abs>`, `<folder>`, `<id>`, `<session>`). `cargo test --test evals_suite` and `evals_test.py` scan
`evals/` and `perf/evals/` and fail on any drive or home path, app-data or Claude temp folder, project folder name, or an id cut short (`tests/private_paths.rs` scans every tracked file for the rest).

## Sets

- **Public** (`evals/public/`, 15 tasks): the benchmark's published face and the regression suite's core.
- **Held-out** (`evals/heldout/`, 6 tasks): run for regression like the rest, but **tool changes are never tuned
  against their transcripts**, so a score gap between the sets shows over-fitting. This repository is public, so the
  in-repo held-out set guards against our own tuning, not against outsiders; a benchmark claim made in marketing
  should also be scored on a private held-out set kept outside the repository (`--tasks-dir` takes any folder with
  `public/` and `heldout/`). Whether to keep one, and who writes it, is an owner decision.

## Determinism and isolation

- **A fresh engine per task**, `--headless --paused`, its own working folder, saves folder, captures folder and
  pipeline cache, so tasks can't see each other and every run starts from the same tick and state. Module builds use
  the checkout as working folder (path crates must be under it) with the task's crate in
  `target/evals-work/<run>-<pid>-<task>`, so two runs from one checkout never share it.
- **Leased ports**: in parallel, the runner reuses the playtest swarm (`scripts/playtest_swarm.py`): a lead engine
  on `--lead-port` posts one board task per eval, and each worker leases a port from the lead's pool (set with a
  `perf.toml` `[swarm] lease_ports` the runner writes, default 7493-7495) and a scratch folder. Without Vulkan an
  engine can't start, so the task is `skipped` and the run still exits 0 (CI without a GPU stays green; the task files
  and the matcher are checked in `cargo test` regardless).
- Reference solutions are deterministic: the same calls in a fresh engine give the same answers. Real agents aren't,
  so agent scores are reported per run with their transcripts, not as single numbers.

## Running it

```
cargo build --release -p specula -p sepulcra-app
python scripts/evals.py --check                       # task files only (no engine)
python scripts/evals.py --reference --workers 3       # every reference must pass   (lead 7492, pool 7493-7495)
python scripts/evals.py --broken --workers 3          # every broken solution must fail
python scripts/evals.py --agent --task room_build --engine-port 7492 --port 7493 --timeout 1800
python scripts/evals_test.py                          # the runner's unit tests
```

`--agent` starts the task's engine and a **logging proxy** on `--port`: it forwards MCP to the engine, shows only the
task's tools in `tools/list` (plus `eval_done`), refuses the others with `tool_not_allowed`, enforces the brief's call
limit (`call_limit`), logs every call, and ends the run when the agent calls `eval_done` (or at `--timeout`), then
verifies. An agent with an MCP client connects to `http://127.0.0.1:<port>/mcp`; one without uses `evals.py tools
--port P` and `evals.py call --port P --as NAME TOOL '{json}'` (or `@file.json`, or `-` for stdin: PowerShell strips
the quotes of a single-quoted argument). Its session is kept per name between calls.

The proxy checks what it forwards: a request with **duplicate keys** is refused (Python keeps the last duplicate, the
engine's JSON the first, so `"name": "note_add", "name": "world_query"` would show the proxy one tool and run another),
so are batches, non-string methods and tool names; what reaches the engine is the checked message re-serialised, never
the raw bytes; and a `playtest_run`'s own calls (steps, end, until) are held to the task's tools (specs from files are
refused, since their calls can't be checked first). **The proxy is a boundary for honest agents, not a sandbox:** an
agent with a shell can reach the engine's own port (on loopback, printed in the runner's log) and call anything. Agent
runs should give agents only the proxy's port, and their transcripts show every call the proxy saw; a hostile-agent
benchmark would need the engine itself to scope tools per session (TOOLS.md's role-scoped tool lists).

## The suite (21 tasks)

| Set | Task | Area | What it asks |
|---|---|---|---|
| public | room_build | world | a room from a written spec: walls at exact faces, a doorway, a lamp |
| public | lantern_row | world | five lit lanterns 2 m apart, standing on the ground |
| public | sight_hidden | sight | tag the crate a camera can't see, and the partly visible ones |
| public | recolour_claimed | swarm | recolour lanterns, half of them in another agent's claim (ask, wait for `claim_share`) |
| public | undo_vandal | journal | undo exactly one agent's entries, keep everyone's later work |
| public | crate_stack | physics | three dynamic crates that stay stacked for 10 s |
| public | save_reload | saves | save, run on, reload, show the hashes agree |
| public | patrol_enemy | lex | an enemy that patrols a loop, spawned once, rules otherwise intact |
| public | wave3_harder | lex, balance | wave 3 +10 % hit points, waves 1-2 and 4-10 untouched, the reference build still wins |
| public | anim_fix_graph | animation | repair a graph that foot-sliding and pop checks flag |
| public | module_jump | modules | add a jump to a hot-reloaded Rust module, ground-only |
| public | playtest_noair | playtests | write and record a playtest proving a layout loses at wave 6 |
| public | hud_marker | interface | a flat ring and a floating marker in the `ui` layer |
| public | furniture_relations | Locus | a table against a wall and chairs around it (needs `world_place`) |
| public | guard_patrol | AI | a navmesh and a mind that patrols waypoints (needs `nav_*`, `ai_*`) |
| held-out | ball_launch | physics | find a launch velocity that lands a ball in a bin |
| held-out | camera_overview | sight | a named camera from which five pillars each show 300+ px |
| held-out | lex_fix_errors | lex | repair a draft rule set with three validation errors, no deletions |
| held-out | lex_tower | lex | a tower archetype and a build command with a cost and a refusal code |
| held-out | task_board_handoff | swarm | take the builder's task (not the painter's), build to spec, finish it |
| held-out | anim_idle_clip | animation | generate a 4 s idle and make a graph state play it |

The two Locus and AI tasks need the navigation lane's tools (`needs`), which were unmerged when the suite was
written: they were first proven against that lane's build, and run on main since it merged; an engine without those
tools reports them `skipped`. A
**HUD widget** task in the full sense (screen-space widgets laid out by `specula-ui`, checked with `ui_tree` /
`ui_check`) waits for M5's agent-facing UI tools ([UI.md](UI.md)); `hud_marker` covers the interface layer that exists.

## Results (2026-10-22, RTX 4090 laptop, a machine shared with other sessions)

**Tool regression** (main `bb6b750` merged, after review round 3). `--reference --workers 3`, twice
(`perf/evals/20261022-083345-reference` and a first run kept out of the repo): **21 of 21 passed** both times with the
same verdicts (388 s wall on a busy machine; the module task now builds its own Cargo target, about a minute). The
solutions made 88 tool calls. `--broken --workers 3` (`20261022-084017-broken`): **44 of 44 failed**: the 21 broken
solutions and the 23 cheats from three review rounds, each at the verifier meant to catch it.

**Real agents** (re-run after review round 2, judged by the round-2 verifiers; round 3 added `world_diff`, the series and the world-space checks after these runs, so they are not re-scored). Six public tasks, each run by a fresh Claude
subagent (the same model as this lane) that saw only the brief and drove the engine through the logging proxy with
`evals.py tools` / `call`:

| Task | Result | Engine calls | Bytes back | Wall | Agent tokens | Refusals and slips |
|---|---|---:|---:|---:|---:|---|
| room_build | 16/16 | 16 | 3.2 KB | 131 s | 64.4k | none |
| recolour_claimed | 5/5 (second run) | 13 | 6.7 KB | 126 s | 61.0k | `path` vs `path_prefix`, `tag` vs `tags` |
| patrol_enemy | 9/9 | 18 | 28.4 KB | 171 s | 70.3k | `lex_session {run}` (needs `action`, `name`) |
| wave3_harder | 14/14 | 15 | 44.2 KB | 180 s | 78.5k | `lex_list {kind: wave}`, `lex_schema {topic: waves}` |
| undo_vandal | 6/6 | 7 | 4.2 KB | 109 s | 55.1k | `not_yours` (needed `force`) |
| module_jump | 3/3 | 20 | 11.1 KB | 228 s | 72.5k | `path` vs `path_prefix`; built twice (assumed 60 Hz) |

The first recolour_claimed run (`20261022-agent2-recolour_claimed`, 4/5) is **not a fair score**: half way through,
Claude Code's own permission check (auto mode) blocked the agent's bridge calls, a recolour inside the claim and then a
read-only `claim_list`; the engine refused nothing. The operator ended that run with `eval_done` (the transcript says
`operator`), so the verifiers judged the state it reached; the second run passed. Each run's transcript, journal, brief
and the agent's own account (`agent.json`) are in `perf/evals/20261022-agent2-*` and `20261022-agent3-*`. Wall time
includes the agents' thinking and the bridge's process start per call; one `not_run` error in most transcripts is the
bridge retrying a session left from an earlier run on that port.

**What this says.** All six passed, with between 7 and 20 calls, so this model finds the public tasks easy: as a
benchmark they separate working from broken tools (the reference/broken pairs and the cheats), not strong agents from
stronger ones. Harder tasks are the next step (multi-step game changes, several agents at once, tasks whose brief
leaves the method open). The value of these runs is in the transcripts: most agents hit an argument-name slip that the
hints recovered from (a refusal names only the first bad argument, so two slips cost two calls), several never found
`fields` and paid for detailed answers instead, and each named a specific gap (listed in [TOOLS.md](TOOLS.md)). What
agents **left out** is as telling: no agent checked the verifier-relevant edge it was unsure about (a held jump across a
landing, corner rays), two left state behind (an open task, saved Lex runs), and the undo agent used `force`, which
also switches off the conflict check that protects later work.

**Runner defects the agent runs found and fixed:** the path scrubber's regex backtracked exponentially on doubly
escaped Windows paths (a module task's transcript hung the runner; now one character class, with a test), agent edits
to module source were deleted with the work folder (now kept under `files/`), the brief's call limit wasn't enforced in
agent mode (now `call_limit`), and an actor's polls flooded transcripts (now counted).

## Later

- Agent runs on every release: a scheduled job that runs the public set with a fixed agent and model, keeps the
  transcripts, and charts scores, calls and bytes per task over time (the benchmark's history).
- More tasks as features land: A4 ragdolls, streaming, Sepulcra on live Lex, M4's asset studio (style checks as
  verifiers), M5's UI tools. A task per new tool family is the rule of thumb.
- Multi-agent tasks: two or more real agents on one engine (a builder and a lighter sharing a claim), scored together.
- Engine reuse: a reset to a fresh world inside one engine would cut start-up (2-5 s) from every task
  ([PLAYTESTS.md](PLAYTESTS.md) has the same follow-up).

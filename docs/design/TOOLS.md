# Agent tools: an audit, and response formats

> **For script and tool authors:** heavy tools (`sight_frame`, `perf_frame`, `engine_info`, `world_list`,
> `world_query`, `world_history`, `play_state`, `logic_status`, `playtest_*`) answer **concisely by default**. A
> script that checks engine data in depth must either use `scripts/specula_mcp.py` (its `Mcp` asks for detailed
> answers; a script with its own client wraps its arguments in `specula_mcp.detailed(tool, args)`) or pass
> `response_format: "detailed"` itself. Other ways to get detailed answers by default: `agent_hello
> {response_format: "detailed"}` (for one session) or start the engine with `--response-format detailed` (for every
> session). New heavy tools should take `response_format` too (see "Changed").

Status: audit of 2026-10-18 (M3, tool ergonomics), against the agent-tool guidance summarised in
[research/AGENT_GAMEMAKING.md](../research/AGENT_GAMEMAKING.md) ("Tool design for agents"). The changes listed under
"Changed" are in `crates/specula/src/response.rs`, `tools.rs` (the call path), `games/sepulcra-app/src/tools.rs`
(`play_state`) and `scripts/specula_mcp.py`. Measurements: `perf/tools/20261018-response-format.json`.

## Inventory

An engine hosting Sepulcra lists **82 tools**: 74 of the engine's and 8 of the game's. By namespace: `world_` 14,
`physics_` 9, `play_` 8 (the game's), `task_` 6, `logic_` 6, `claim_` 5, `sight_` 4, `perf_` 4, `lease_` 4,
`camera_` 3, `intent_` 3, `note_` 3, `tilemap_` 2, `model_` 2, `desync_` 2, `playtest_` 2, and one each of
`agent_hello`, `engine_info`, `time_control`, `window_view`, `swarm_settings`.

`tools/list` is **96.5 KB** of compact JSON, very roughly 24,000 tokens that every agent's context carries before its
first call. Since Lex merged (nine `lex_*` tools) it is **91 tools and 105.5 KB**; the counts below are from the
audit's 82. The biggest definitions: `world_spawn` 9.3 KB, `world_set` 8.6 KB (they describe the same components
twice), `sight_frame` 5.0 KB, `world_env` 3.5 KB, `sight_compare` 3.3 KB, `sight_orbit` 3.0 KB.

## Against the guidance

| Guidance | Where Specula stands |
|---|---|
| Consolidate around workflows, not raw API calls | Mostly yes: `world_batch` (atomic edits), `time_control` (pause, step, scale), `sight_frame` (image + packet + overlays in one), `playtest_run` (a whole scripted session in one call). Overlaps remain (below). |
| Namespace tool names | Yes: every name is `namespace_action` (five namespaces have a single tool: `agent_hello`, `engine_info`, `time_control`, `window_view`, `swarm_settings`); names match `^[a-z_]+$` and stay under 64 characters. `world_` is overloaded (14 tools, of which `world_history`, `world_changes`, `world_undo`, `world_hash`, `world_rollup` are the journal's). |
| High-signal context, not opaque ids | Yes: entities answer with paths as well as ids; refusals name the claim holder, the stale version and a `conflict` object; game rejections carry the rule's code (`not_enough_dust`, `occupied`). |
| Concise and detailed response formats | **Was missing; now added** (below). |
| Paginate and truncate by default, with guidance | Yes: every listing has `limit` with a default and a maximum, and a cursor; byte caps (`MAX_LISTING_BYTES`, batch summaries) hand back `next_cursor`. Paging names are mostly `cursor` + `limit`; feeds use `since` (`world_changes`, `world_rollup`), and three outliers remain (below). |
| Actionable errors | Mostly: argument errors name the field and suggest the closest valid key (`'rotation' is not an argument of world_spawn; did you mean 'rotation_deg'?`), world errors carry codes and details. **Some errors were plain text**, without a code; now all are coded (below). |
| Evaluate tools with agents on realistic tasks | Not yet: the agent-evals item (M3). The playtest runner is the harness it needs (an eval is a spec that checks what an agent built). |
| Every property described | 1 of the 82 tools' properties had no description (`play_state.include`); fixed. |

## Changed

**`response_format: "concise" | "detailed"`** on the heavy tools; concise is the default, detailed is the full answer
as before. The engine still builds the detailed answer and the connection's thread cuts it, so the engine thread pays
nothing. Each tool's schema says what its concise answer leaves out.

| Tool | Concise keeps | Detailed adds |
|---|---|---|
| `sight_frame` | image paths, camera, subject counts, warnings, problems, overlays; per subject: id, name, path, on_screen, screen boxes, visible_px, occlusion, distance, layer (ui), transient | each subject's shape, material, colour, position, author; `id_buffer`, `models` (kept when models are missing), `lights`, `time_s`, `schema`. The full packet is always saved at `image.packet_path`. |
| `perf_frame` | errors, problems, per render path GPU ms, CPU wait/record/submit, draw calls and triangles, memory total; pacing, serve schedule, sim and game times, physics totals, heap, entity counts | every pass, pipeline counters (kept when pipelines were built after warm-up), heaps, budget tables, render graph, model library, shape budgets |
| `engine_info` | engine, GPU, validation, world summary, agents, clock, window, game status, module summary, swarm counts | the model library, capture and world paths, journal internals, shape budgets |
| `world_list` | whole entities with what defaults imply left out (`name` = the path's last segment, `rotation_deg` [0,0,0], `scale` [1,1,1], `modified_by` = `created_by`): lossless | those fields |
| `world_query` | the fields asked for (default id, path, shape, position, tags, version), as before | `fields: ["all"]` |
| `world_history` | one line per entry, as before | `detail: true` |
| `play_state` (Sepulcra) | the run's numbers, counts, layout, hashes; enemies by kind when present; list items without max_hp/remaining_m | presentation (HUD, audio, scene), content hash, projectiles, pending spawns, those item fields |
| `playtest_run`, `playtest_report` | status, ticks, step counts, the first 5 failures, 8 captures, hashes | every call's result, all failures and captures, checkpoints |
| `logic_status` | as before | `response_format: "detailed"` is the same knob as its `detailed` |

**Compact JSON everywhere.** Many answers were pretty-printed (`engine_info`, `perf_frame`, `sight_frame`, every
entity answer, every error): indentation cost about a third of their bytes (`sight_frame`'s full packet: 7,604 bytes
pretty, 4,528 compact). Every answer now leaves compact (`response::tidy` re-serialises an answer that has
newlines; text that isn't JSON, such as `play_map`'s map, is untouched).

**Every error coded.** Plain-text errors now arrive as `{"error": code, "message": ...}`: `shutting_down`,
`engine_busy` (with `"retry": true`), `no_game`, `unknown_tool`, and the generic `error` for the rest (a failed render,
a module host's timeout), so an agent can branch on a code everywhere.

**Scripts keep detailed answers.** `scripts/specula_mcp.py` asks for `response_format: "detailed"` on the tools with a
format unless a call passes its own, and the scripts with their own MCP clients (`call_rate.py`, `swarm_drill.py`,
`world_roundtrip.py`, `window_check.py`, `lex_check.py` and the raw calls in `anim_graph_check.py`, `model_gallery.py`, `physics_check.py`,
`two_d_check.py`, `sepulcra_play.py`) wrap their arguments in `specula_mcp.detailed()`, so the verification scripts
read what they always read. An agent (and a client made with `detailed=False`) gets the concise defaults. Review round
1 found the gap: concise `engine_info` had left out a null `last_autosave_error`, which `world_roundtrip.py` reads.
Concise `engine_info` now always carries it, null or not.

**Choosing the default.** The order of precedence is: a call's own `response_format`, then the agent's default
(`agent_hello {response_format}`), then the engine's (`--response-format concise|detailed`, concise unless set). An
agent's default belongs to its identity (its resume token), so a later session that resumes it with `agent_hello
{name, resume_token}` keeps it. The engine remembers 4,096 identities and sessions, dropping the least recently used.
The defaults apply to the shaped tools above and to the tools that take `response_format` themselves (a game's
`play_state`, `logic_status`, `playtest_*`), and to nothing else: a tool without a format never receives the
argument. They also apply inside playtests, whose calls go out as the session that started them.

### Measured (bytes, headless Sepulcra at tick 10,000, 40 agent cubes)

| Call | Concise | Detailed (compact) | Saved |
|---|---:|---:|---:|
| `sight_frame` (640x360, 12 subjects) | 2,814 | 4,528 | 38% |
| `perf_frame` | 698 | 3,479 | 80% |
| `engine_info` | 768 | 2,104 | 64% |
| `world_list` (50) | 8,701 | 12,536 | 31% |
| `world_query` (50) vs `fields: all` | 4,746 | 12,533 | 62% |
| `world_history` (20) vs `detail` | 2,082 | 7,914 | 74% |
| `play_state` | 282 | 637 | 56% |
| `play_state` with enemies and stones | 1,939 | 2,613 | 26% |

A typical builder session of 20 calls (hello, info, a new run, a step, state, a claim, spawns, a set, a query, a
listing, a task, a capture, perf, history, physics info, two moves, the hash, a release) went from **27,475 bytes on
main to 12,900 bytes** with the new defaults and compact JSON: **53% less** for the same information an agent acts on.

## Not changed (recommendations)

1. **The tool list itself is the biggest cost** (96.5 KB, about 24,000 tokens per agent). Options, roughly by payoff:
   - **Role-scoped tool lists**: `agent_hello {role}` (or the MCP client name) selects the tools `tools/list` shows,
     e.g. a playtester sees `play_*`, `playtest_*`, `sight_*`, `world_query`, `world_hash`, the board; the rest stay
     callable. This needs `tools/list_changed` notifications per session in `specula-protocol`.
   - **Share the component schemas** of `world_spawn` and `world_set` (about 8 KB duplicated): describe components
     once (in `world_spawn`) and refer to them from `world_set`.
   - **Shorter descriptions** for the longest (`sight_frame` 1.5 KB, `world_env` 1.3 KB, `playtest_run` 1.1 KB).
2. **Overlaps that could be one tool each** (keep the old names as aliases if merged): `world_list` is `world_query`
   with `response_format: detailed`; `claim_renew` / `claim_release` / `claim_share`, `lease_renew` / `lease_release`,
   `intent_clear`, `note_delete` and `camera_delete` are each an action on one resource; `tilemap_set` is a `world_set`.
3. **The journal tools under `world_`** (`world_history`, `world_changes`, `world_undo`, `world_hash`, `world_rollup`)
   would read better as `journal_*`; renaming needs aliases, since scripts and other lanes use the names.
4. **Paging outliers**: `physics_events` pages with `after`, `play_state` with `offset`, `task_list`'s feed with
   `events_since`; the rest use `cursor` (listings) or `since` (feeds).
5. **`detail` means two things**: a boolean on `world_history` (now also `response_format`), a text field on
   `task_post` / `task_update`; `logic_build` has `detailed`. New tools should use `response_format`.
6. **Engine-side pretty-printing** is now wasted work (the answer is re-serialised compact): switch `to_pretty()` to
   `to_string()` in tool answers as files are touched (about 60 call sites in the engine and Sepulcra).
7. **Still heavy, not shaped**: `sight_orbit` and `sight_compare` (contact sheets with compact subjects already),
   `physics_contacts`, `task_list` with events. Shape them the same way when measurements show agents paying for them.
8. **Agent evals** (M3): run realistic tasks through the tools and read the transcripts for what agents misuse or
   leave out. The playtest runner checks outcomes; the evals lane adds the tasks. *Built 2026-10-18/22:*
   [EVALS.md](EVALS.md); its findings are below.

## Findings from the agent evals (2026-10-18 to 10-22)

From writing 21 tasks' reference solutions and verifiers, and from real agents' transcripts
(`perf/evals/*-agent-*`). "Agents" means Claude subagents that saw only the brief and the tools. Each item says who hit
it; the fix is a suggestion, not done here.

1. **`tag` vs `tags`** (every agent run that queried by tag hit it): `world_query` takes `tags` (an array) while
   `task_list` takes `tag`. The refusal's hint ("did you mean 'tags'?") recovered every time, at the cost of a call.
   Accept `tag` as a one-element `tags` on `world_query` (and `tags` on `task_list`), or rename to match.
2. **Agents don't find `fields`**: two agents fetched `response_format: detailed` (about 2 KB for 6 entities) or gave
   up checking a component (`light`, `color`) because the concise `world_query` leaves it out, and said "a way to ask
   for only some fields would help". `fields` exists; the concise description should lead with it ("add `fields:
   [\"color\"]` for more").
3. **Physics queries see nothing until a tick runs**: on a paused world, `physics_raycast` / `physics_overlap` after a
   `world_spawn` return no hits (they read "the colliders as of the last physics tick"). One agent stepped a tick
   because it read the description; the reference solutions had to. Sync physics with the world before a query (as
   `save_write` already does), or answer with a `stale: true` and the fix.
4. **Partial occlusion is invisible in sight packets**: `occluded` is set only when no pixel shows, so a pillar 99 %
   behind a wall reads `visible_px: 10` with no flag (`camera_overview`). An `occluded_fraction` (visible pixels vs the
   subject drawn alone) or `occluded_px` would let agents ask "is it fully in view?".
5. **Another agent's cameras can't be listed**: `camera_list` shows only the caller's; a verifier (or a reviewer) has
   to find the owner in `world_history {kinds: [camera]}` before `sight_frame {camera_of}`. Add `owner` / `all` to
   `camera_list`.
6. **Names that change between write and read**: `task_done {summary}` is read back as `result` in `task_get`;
   `lex_simulate` counts `commands.refused` but lists them under `rejections`; `task_get` events quote notes cut
   mid-word next to the full `notes` array (one agent called it misleading).
7. **Units as strings**: `lex_session inspect` gives attributes as `"66.55pt"`, `"0.8m/s"`: readable, but numeric
   checks need string matching. Add the number in the human unit alongside (`{"hp": "66.55pt", "hp_value": 66.55}` or
   a `numeric: true` option).
8. **`world_query` on a missing `path_prefix` is an error** (`not_found`) where a filter would answer `total: 0`;
   checks like "this was deleted" need `expect_error`.
9. **`shape2d` bounds are loose**: a circle of radius 1.45 m with a 0.1 m stroke reports bounds of radius 1.65 m (the
   stroke counted on both sides plus the antialiasing margin), so "is the ring 1.5 m?" needs a 0.2 m tolerance.
10. **Collider units**: an agent wasn't sure whether `collider.size` is metres or scaled units until a ray told it; the
    schema says "local (scaled) units" but an example ("size 1 = the cube's own size") would settle it.
11. **The anim asset path** is `/anims/recipes:shambler.json/graphs/creature` where the docs say
    `/anims/<recipe stem>/...`.
12. **`world_undo {force}` means two things** (undo_vandal agent): "undo another agent's entry" and "override later
    conflicting changes". Undoing a vandal's entries needs the first, and silently turns off the second, the check
    that protects everyone's later work. Split it (`allow_others: true` keeps the conflict check; `force` stays the
    override).
13. **`lex_session` without `action`** lists runs even when `name` and `archetype` are given (patrol agent: a wasted
    call). Infer `inspect` from `name`, or refuse with the fix.
14. **Lex reads are coarse** (wave3 agent): `lex_get` returns a whole 5 KB `waves` item when one wave is wanted (a path
    like `waves[2]` would do); `lex_edit` doesn't echo the patched path's old and new value; nothing shows the stats an
    enemy will spawn with in a wave (it took a saved run and an inspect); `waves[i].scale` has no doc saying it
    multiplies base attributes.
15. **`lex_simulate` watch** never samples tick 0, and `count(tag)` can't count one archetype over time.
16. **Measuring motion costs three calls a sample** (module agent): `logic_input`, `time_control step`, `world_query`.
    A "step and report these entities" option on `time_control` (or a watch like `lex_simulate`'s) would halve a
    tuning loop. The tick rate (30 Hz) appears only in `time_control`'s answer.
17. **`world_history {detail}`** lists ids and paths but not values, so what an entry changed (and what an undo
    restores) needs further reads.
18. **A refusal names only the first bad argument** (re-runs, 2026-10-22): `{path, tag}` on `world_query` took two
    refusals to fix. Name every unknown argument with its suggestion in one answer; `world_query`'s description could
    also name its filters (`path_prefix`, `tags`, `region`) up front.
19. **`lex_schema` topics** don't take kind names (`{topic: waves}` is refused; the waves kind is under `kinds`), and
    `lex_list {kind: wave}` needs the plural.
20. **No "inside claim N" filter**: an agent worked out which entities a region claim covers by comparing bounds by
    hand; a `world_query {claim}` filter would answer it.
21. **`world_env` with no arguments is journaled as a change** (round 3): reading the environment (the tool's only
    way to read it) adds an `env` journal entry by the reader, so "nobody touched the environment" can't be checked
    from the journal. Don't journal a call that changes nothing (or add a read-only form).

Engine defects the evals ran into (reported to their lanes):

- **Physics (P3):** a sphere that had gone to sleep inside the `ball_launch` bin, teleported back to the start with
  `world_set {position, body: {velocity}}` and launched again, stops against the bin wall's outer face 0.55 m up and
  hangs there while its velocity integrates to -70 m/s over 8 s (`physics_check energy` fails at 23 J a tick; the
  tunnelling guard stops it every tick, `guard_stops: 206`). The same launch from a fresh engine bounces off the wall
  normally. Warm-start or sleep state surviving a teleport looks like the cause.
- **Navigation:** with `nav_agent {speed: 3}` the patrolling guard covers about 5.3 m/s of path between
  samples while `ai_status` reports a velocity of 3.0 (`guard_patrol`); worth checking (not measured further here).

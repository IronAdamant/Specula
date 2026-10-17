# Agent tools: an audit, and response formats

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
first call. The biggest definitions: `world_spawn` 9.3 KB, `world_set` 8.6 KB (they describe the same components
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
| `sight_frame` | image paths, camera, subject counts, warnings, problems, overlays; per subject: id, name, path, on_screen, screen boxes, visible_px, occlusion, distance | each subject's shape, material, colour, position, author; `id_buffer`, `models` (kept when models are missing), `lights`, `time_s`, `schema`. The full packet is always saved at `image.packet_path`. |
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

**Scripts keep detailed answers.** `scripts/specula_mcp.py` asks for `response_format: "detailed"` on the shaped tools
unless a call passes its own, so the verification scripts read what they always read. An agent (and a client made
with `detailed=False`) gets the concise defaults.

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
   leave out. The playtest runner checks outcomes; the evals lane adds the tasks.

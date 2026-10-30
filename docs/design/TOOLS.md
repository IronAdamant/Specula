# Agent tools: an audit, and response formats

> **For script and tool authors:** heavy tools (`sight_frame`, `perf_frame`, `engine_info`, `world_list`,
> `world_query`, `world_history`, `play_state`, `logic_status`, `playtest_*`) answer **concisely by default**. A
> script that checks engine data in depth must either use `scripts/specula_mcp.py` (its `Mcp` asks for detailed
> answers; a script with its own client wraps its arguments in `specula_mcp.detailed(tool, args)`) or pass
> `response_format: "detailed"` itself. Other ways to get detailed answers by default: `agent_hello
> {response_format: "detailed"}` (for one session) or start the engine with `--response-format detailed` (for every
> session). New heavy tools should take `response_format` too (see "Changed").
>
> **Scoped tool lists (2026-10-29):** a client that says nothing now lists only the **core** tools (`agent_hello`
> and `tools_*`); every other tool is still callable. Scripts that read `tools/list` connect to `/mcp?tools=all`
> (`specula_mcp.Mcp` does it for you; `.mcp.json` does it for Claude Code). A new tool needs a family (its name
> prefix) and a one-line catalog entry in `crates/specula/src/toolscope.rs`, or
> `every_tool_has_a_family_and_a_brief` fails. See "Scoped tool lists" below.

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
audit's 82.

**After M4's merges (measured 2026-10-29):** the engine alone lists **143 tools, 194.2 KB** of compact JSON; hosting
Sepulcra, **151 tools, 200.3 KB** (very roughly 50,000 tokens). By family (tools, KB): `world_` 15, 37.3; `sight_` 4,
13.9; `lex_` 10, 13.1; `physics_` 11, 12.0; `studio_` 5, 10.3; `anim_` 5, 8.7; `task_` 8, 8.0; `project_` 6, 7.8;
`nav_` 5, 7.8; `vfx_` 5, 7.2; `play_` 8, 6.1 (the game's); `procgen_` 3, 5.6; `review_` 4, 5.3; `balance_` 3, 5.1;
`logic_` 6, 5.0; `stream_` 3, 3.8; `ai_` 3, 3.4; `perf_` 4, 3.4; `ragdoll_` 2, 3.3; `playtest_` 2, 3.3; and under 3 KB
each: `claim_`, `save_`, `model_`, `camera_`, `critique_`, `lease_`, `intent_`, `tilemap_`, `style_check`, `desync_`,
`note_`, `agent_hello`, `window_view`, `merge_suggest`, `swarm_settings`, `time_control`, `engine_info`. The biggest
definitions: `world_spawn` 10.4 KB, `world_set` 9.7 KB, `sight_frame` 5.5 KB, `sight_compare` 3.6 KB, `world_env`
3.5 KB, `lex_live` 3.2 KB, `sight_orbit` 3.2 KB, `world_place` 3.1 KB. The list has doubled since the audit: tool
search or per-role tool sets (an agent loads the families its role uses) is the lever, not shorter descriptions.
(`world_spawn` and `world_set` describe the same components twice.)

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

**`disk_warning` rides on answers.** When the engine's output crosses the disk budget's soft line, every session
hears it once: a structured `disk_warning` object (`code: disk_budget`, what crossed, the biggest categories with
what to do about each, a hint) is added to the next answer it gets, any tool's, in the answer's first text item (a
sight tool's packet, after its image). Answers to sessions already told pass through untouched. The `disk` family
(`disk_status`, `disk_prune`) reports and frees space; `project_upload` brings binary project files (PNG, GLB/glTF,
WAV) in through the protocol in chunks ([DISK.md](DISK.md), [PROJECT_DATA.md](PROJECT_DATA.md)).

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

## Scoped tool lists (2026-10-29)

By the M4 audit the engine had 143 tools (151 with Sepulcra's), and `tools/list` was about 200 KB of compact JSON,
roughly 50,000 tokens that every agent read before its first call. A swarm pays that once per agent (F1), so the
list is now **scoped per session**. Code: `crates/specula/src/toolscope.rs` (families, roles, the catalog, each
session's scope), `toolscope_tools.rs` (the `tools_*` tools and the call gate), and `specula-protocol` (connect
options and `notifications/tools/list_changed`). Live check: `scripts/toolscope_check.py`; numbers in
`perf/tools/20261029-toolscope.json`.

**The owner's model (2026-10-29), kept as it was given.** The lead agent knows the engine and picks the tools for
each subagent, or tells it to activate only the tools its task needs. Subagents see only names with one-line
descriptions until they activate something. If no tool exists for a need, the request goes to the lead, who builds
or composes one. Claude Code already defers MCP tool schemas, loading names first and schemas on demand, but other
vendors' clients may not, so the engine does it itself.

### What a session lists

A session lists the **core** plus its **scope**. The core is `agent_hello`, `tools_catalog`, `tools_activate`,
`tools_deactivate`, `tools_invoke` and `tools_request`: about 5 KB (5,260 bytes measured), always listed, never capped. The scope is an
**active** set, and optionally a **cap** set by the lead. A session lists core + (active ∩ cap).

**Calls are not limited to the list.** A session may call any tool within its cap, whether it is listed or not.
Without a cap, that means every tool. So scripts, older clients and agents that know a tool's name keep working.
Calls beyond a cap are refused with `not_in_scope`, which names the tool and points to `tools_request`. The cap
holds on direct calls, inside `tools_invoke` and `tools_activate`, and for a playtest's calls. `playtest_run` is
refused when its spec calls a tool beyond its starter's cap, and each step is checked again when it runs, so a
cap the lead sets mid-run applies (the step fails with `not_in_scope`). A playtest can't call `tools_*` at all:
they answer on the caller's connection (`CallerThread::Scope`, refused with a reason like the other caller-thread
tools).

**A hello never widens a cap.** A session capped as `will` that says `agent_hello` as `bob` keeps will's cap,
narrowed further by bob's cap if bob has one. Only the lead's own `tools_scope` (or `clear`) on the session's
agent can widen it.

**Bounds.** A selection names at most 256 entries. An entry over 64 characters names nothing and gets no
suggestion. An error names at most 8 unknown entries, then gives a count. Echoed names and labels are cut to 64
and 200 characters. The lead's label is built from the deduplicated entries.

**Who chooses the list:**

| Source | How | Notes |
|---|---|---|
| Engine default | `--tools-default core\|all` | `core` unless set (see below) |
| Connect URL | `/mcp?tools=world,sight,anim_check`, `/mcp?role=builder`, or both (their union) | Applied at `initialize`, so the client's first `tools/list` is already right. An unknown option or entry refuses the `initialize` and suggests the closest name (`'wrld' (did you mean 'world'?)`). |
| `agent_hello {tools}` | Families or tool names | Replaces the active set |
| `agent_hello {role}` | A template's name, given explicitly (a hello with only a name keeps the core) | Only while nothing explicit chose: the default, or an earlier role. A URL's choice stands. |
| `tools_activate` / `tools_deactivate` | At runtime | Within the cap |
| The lead's `tools_scope {agent, tools\|role, cap}` | Overrides all of the above | Applies now to the agent's open sessions. It is also kept by name for any session that later says `agent_hello` as that agent. A hello's own `tools` are then added within the cap. `clear: true` removes it. |

**Families** come from the name prefix (`world_`, `sight_`, `camera_`, ...), with a few tools placed by name: the
journal's `world_history`, `world_undo`, `world_changes`, `world_hash` and `world_rollup` form `journal`, and
`window_view` belongs to `sight`. There are 25 engine families, plus `game` for the hosted game's tools (the
newest, `disk`: `disk_status` and `disk_prune`, the disk footprint budget, [DISK.md](DISK.md); and `project` gained
`project_upload`, binary project files in chunks, [PROJECT_DATA.md](PROJECT_DATA.md)). A new tool
joins its family by its prefix. A new prefix, or a tool without a one-line catalog entry (`BRIEFS`), fails
`every_tool_has_a_family_and_a_brief`. That test also catches stale entries, empty families and roles that name
nothing.

**Role templates** are data (`ROLES`); `tools_catalog {roles: true}` shows them with their sizes:

- `lead` and `director`: everything.
- `builder`: world, sight, claim, board, journal, model, engine, terrain.
- `reviewer`: sight, journal, board, review, studio, plus `world_query` and `world_list`.
- `playtester`: game, playtest, sight, board, save, engine, plus `world_query` and `world_hash`.
- `art_director`: sight, studio, review, model, procgen, vfx, terrain, project (so it can `project_upload` the
  textures it directs), board, plus `world_env` and `world_query`.
- `sight_skeptic`: sight, plus `world_query`, `world_list` and `critique_add`.
- `performance`: perf, engine, stream, sight, disk, plus the world reads.
- `designer`: lex, balance, playtest, game, board, save.
- `animator`: anim, physics, sight, world, claim, board.
- `coder`: logic, project, world, sight, engine, claim, board.

**The default is core-only**, so a client that says nothing gets the core. That is the point of this work: an
unconfigured agent in a swarm pays about 5 KB instead of about 255 KB. The engine's `instructions` (sent at `initialize`)
and the core tools' descriptions say how to find and activate the rest. Backward compatibility holds in three
ways:

- Calls to unlisted tools still work.
- `scripts/specula_mcp.py` connects with `?tools=all`. The evals proxy forwards to `?tools=all` and asks
  upstream for JSON answers only, because it filters the list itself and relays every answer as
  `application/json`. A stream it gets anyway is relayed as the response's JSON.
- `.mcp.json` asks for `?tools=all`. Claude Code defers MCP schemas, so the full list costs it only names, and
  Claude Code subagents share their parent's MCP session, so per-subagent scopes there need separate sessions.

`--tools-default all` restores the old behaviour for an operator who wants it.

### The tools

- **`tools_catalog {filter, family, roles, cursor}`**: every tool by family with one line each. The whole catalog
  is 9.6 KB in one page (capped at 16 KB a page, paged by family). It also gives your scope: how many tools you
  list, their bytes, where the scope came from, and the cap. `filter` matches words against names, lines and full
  descriptions. When nothing matches, the answer gives the closest partial matches and points to `tools_request`.
- **`tools_activate {tools, role, schemas}`**: adds tools to your list. It answers with what was added and the full
  definitions of the tools asked for, up to 32 KB; any others are named. `schemas: false` leaves the definitions
  out, for clients that refresh their list anyway.
- **`tools_deactivate {tools | all}`**: removes tools from your list. The core stays.
- **`tools_invoke {tool, arguments}`**: calls any tool by name. It is for clients that don't re-read `tools/list`,
  or can only call tools they list. The cap applies.
- **`tools_request {need, why}`**: searches the catalog for `need`, then posts a board task tagged `tool_request`
  for the lead (priority 4). The task carries the need, the reason, what the search found (marking matches beyond
  the requester's cap) and the requester's scope. The answer gives the task id and the matches. Gaps become data
  the lead works through. It needs `agent_hello`, like every board write. Because it is core and uncapped, a
  looping agent must not be able to fill the board, so:
  - A request the agent already has open (same need, ignoring case and spacing) answers with that task and
    `duplicate: true`.
  - An agent may have at most 8 requests open; the next is refused with `too_many_requests`, which lists the
    open ones.
  - Requests are serialised, so two identical requests sent at once still make one task.
- **`tools_scope {agent, role, tools, cap, clear}`**: for the lead only (role `lead` or `director` after
  `agent_hello`). Anyone else gets `lead_only`. It is not core: an agent capped without it can't call it.

### list_changed: how it reaches clients, verified

The server now declares `capabilities.tools.listChanged: true`. Each session remembers the list version it last
saw. When the version has changed, the answer to that session's next request is a short `text/event-stream`:
first `notifications/tools/list_changed`, then the response. Streamable HTTP allows a POST to be answered this way.
The session's next request is usually the call that changed the list (`tools_activate`, or an `agent_hello` that
applied a role). For a change the lead made, it is whatever the session sends next. Only clients whose `Accept`
names `text/event-stream` get an event stream; others get plain JSON and see the change at their next
`tools/list`. There is still no standalone `GET` stream, so a session that sends nothing is told when it next
does.

**Claude Code 2.1.312 re-reads the list on the notification.** In a live run (`claude -p` through a logging proxy,
`perf/tools/20261029-toolscope-claude-code.txt`):

1. It listed the 6 core tools.
2. It called `tools_activate {tools: ["world_hash"]}`, and the answer came back as an event stream.
3. It re-read `tools/list` within the same second, and now saw 7 tools.
4. It called `world_hash` natively; its tool search found the newly listed tool.

Grok Build, OpenAI's clients and others aren't verified yet (M6 cloud swarms). For a client that doesn't refresh,
this is exactly what still works:

- The activation answer carries the schemas.
- `tools_invoke {tool, arguments}` calls any tool within the cap.
- A direct call to an unlisted tool works if the client allows calling a name it doesn't list.

The repo's own MCP clients (`specula_mcp.py`, `call_rate.py`, `swarm_drill.py`, `window_check.py` and
`world_roundtrip.py`) accepted `text/event-stream` but parsed only JSON. They now read both
(`specula_mcp.reply_of`). The evals proxy asks for JSON only (above); `evals_test.py ProxyStreams` covers it.

### Not a security boundary (yet)

Until per-agent auth tokens exist (M6), anyone can open a session with `?tools=all`, and anyone can say they are
the lead. Scopes save tokens and keep agents focused; they don't contain a hostile agent. The design leaves room
for tokens: the lead's assignment is kept by **agent name** and applied at `agent_hello`. In M6 a token will carry
it instead, so a session authenticated with that token gets that cap whatever its URL or name.

**The scope store holds open sessions only.** The protocol tells the handler when a session ends, by `DELETE` or
by its own eviction past 1,024 sessions (`ToolHandler::session_closed`). So the store never holds more than the
protocol's 1,024 sessions, well under its own bound of 4,096. If that bound is ever reached, uncapped scopes go
first, then the least recently used capped one, and never the entry just stored.

**A session the store doesn't know** (none from the protocol; internal callers and tests) has the engine's
default scope: core or all per `--tools-default`, never a cap. This is deliberate. Caps come only from a lead's
assignment, and the protocol's sessions are always in the store while they are open, so there is no capped session
to fall back from.

### Measured (Sepulcra hosted, 160 tools; compact `tools/list` JSON; tokens are roughly bytes / 4)

Re-measured with `scripts/toolscope_check.py` after the disk budget merged (2026-10-29, the `disk` family,
`project_upload`, and `project` in the art director's role); the engine alone lists 152 tools.

| List | Tools | Bytes | ~Tokens |
|---|---:|---:|---:|
| all (`?tools=all`, the old default) | 160 | 214,679 | 53,700 |
| **core (the new default)** | 6 | **4,674** | **1,170** |
| role builder | 51 | 79,674 | 19,900 |
| role reviewer | 45 | 62,326 | 15,600 |
| role playtester | 40 | 47,588 | 11,900 |
| role art_director | 56 | 84,488 | 21,100 |
| role sight_skeptic | 17 | 26,508 | 6,600 |
| role performance | 27 | 36,611 | 9,200 |
| role designer | 42 | 46,062 | 11,500 |
| role animator | 57 | 93,652 | 23,400 |
| role coder | 54 | 86,345 | 21,600 |
| `tools_catalog` (every tool, one line each) | 160 | 9,862 | 2,500 |

So an agent that starts from the core and reads the catalog pays about 14 KB before it activates anything: 7% of
the old list. The role templates are still 25-95 KB, because `world_spawn` (10.7 KB) and `world_set` (10 KB) are
in most of them. Sharing their component schemas (below) is now the biggest remaining saving.

Each tool call used to rebuild all of the engine's tool definitions, about 200 KB of JSON, just to check that the
name exists. That check is now a map lookup (`ToolHandler::has_tool`), and the definitions are built once.

**64-agent drill**, headless, on the Intel iGPU, release builds, run back to back on the same machine
(`perf/drills/20261029-*-toolscope-*`):

| Build | Calls/s | World ops/s |
|---|---:|---:|
| main (15ea288), run 1 | 209.5 | 658 |
| main (15ea288), run 2 | 190.5 | 598 |
| this lane, run 1 | 426.3 | 1,332 |
| this lane, run 2 | 435.4 | 1,359 |

Scoping added nothing measurable, and the drill runs **about 2.2 times faster**. The drill's own sessions say
`agent_hello {role: "builder"}`, so they also exercise the role path and its event-stream answers.

## Bounds (2026-10-30)

Recurring bug class D (root-cause investigation, 2026-10-29): input reached loops and answers unbounded.

| Instance | Measured |
|---|---|
| `world_query {fields: [...] * 1M}` | 3.8 s, a 21 MB answer |
| `check_keys`' "did you mean" on a 4 MB key | 4.4 s (an edit distance over every character) |
| errors that echo their input | a 4 MB error for a 4 MB input (unknown keys, names, values; `Unknown tool: ...`) |

The root causes: argument helpers had no default bounds, nothing capped answers, and errors echoed input
verbatim. Each tool's parser had its own caps, or none. The fix is one choke point per direction, in
`crates/specula/src/bounds.rs`, so a new tool is bounded without anyone remembering to.

### In: arguments are checked against the tool's own schema

- **Every string and array a schema describes declares its limit.** `bounds::add_to_schemas` runs once over every
  definition (engine and game tools) and writes `maxLength` / `maxItems` where a schema has none: the defaults are
  **4096 characters per string (`maxLength` counts characters, as JSON Schema does) and 256 items per array**, or
  the tool's own override from `bounds::OVERRIDES` (each with its reason: `world_batch` ops up to 1000, a project
  file's text and a plan document up to 1 MiB, an upload chunk, `tilemap_set` cells, `lex_simulate` commands and
  `lex_edit` ops). A property without a type takes the free-form data limits below. Clients read the limits in
  `tools/list`, which costs about 16 bytes per property.
- **Arrays that mean sets carry `uniqueItems`**, and repeats are refused (`duplicate_items`, naming the repeated
  item): `world_query` fields, `anim_check` clips (256 copies of one clip took 1.2 s), journal ids, overlay and tool
  names.
- **`bounds::check_args` checks a call's arguments against the schema before any parser runs**, in
  `toolscope_tools::call`, which every call passes through (`tools_invoke`'s inner call and playtest steps too).
  Lengths are compared before anything is read, so a 1 MB string or a million items is refused in microseconds:
  `argument_too_large {field, limit, got}`.
- **Keys:** an object the schema describes has keys of at most 64 characters (no argument name is longer), refused
  as `argument_key_too_long` before any "did you mean". `check_keys`, which parsers also use on nested objects,
  does the same before it calls `closest()`.
- **Free-form data** (an object schema without `properties`: entity props, recipe params, a playtest step's
  arguments, a Lex document's items, a property without a type) is the consumer's to validate; it is bounded more
  loosely (1 MiB strings, 65,536 items, 256-character keys and 16,384 keys per object, nesting at most 64) and by the
  request body (16 MiB).

Why the schema, rather than limits in each parser's getters: the schema is what the agent reads, so declaring the
limit there and enforcing that declaration in one place means they can't drift apart, and no parser has to be
remembered. Parsers keep their own semantic checks (ranges, names, shapes) and their own smaller caps.

### Out: errors and answers

- **Echoes are cut**: `bounds::echo` keeps 128 characters and the value's size (`'kkkk...' (4194304 bytes)`). It is
  used where errors echo keys, names and values (`check_keys`, unknown tools in the engine and the protocol's
  `Unknown tool`), and the choke point also cuts any quoted run without spaces past 128 characters in an error's
  message, whoever wrote it.
- **Errors are at most 2 KiB of message**, other strings in an error 8 KiB (a rendered compiler diagnostic fits),
  nesting 16 levels, and a whole error at most its tool's answer ceiling (below): past it the largest arrays are
  halved, each ending in `{"omitted": n}`, until it fits, so nothing the message names disappears; only an error
  that can't be cut keeps just `error` and `message` (with `details_omitted`). `bounds::cap_answer` applies this to
  every answer in `Tools::call`, after `response::tidy`; `tools_invoke` is capped as the tool it invoked.
- **Answers have a ceiling**: 256 KiB of text by default (images apart; a capture's PNG is bounded by its size
  limit), more for the few tools whose answer is a document (`project_read` 1.2 MiB, `tools_activate` 1 MiB,
  `tools_catalog`, `lex_get`, `lex_export` 512 KiB). Past it the answer is `answer_too_large {completed: true, bytes,
  limit}`: the call itself completed and its changes stand (never retry a write because of it), only the answer was
  withheld; ask for less or page.

### Guard tests

- `bounds::tests`: the limits the schema shows are the limits enforced (a value at the limit passes, one past it is
  refused, for every string and array property of every tool), every override names a real property, and every
  string and array property of every tool declares a limit.
- `tools::tests::every_tool_survives_oversized_arguments` (the fuzz): for every tool in `definitions()`, each
  property in turn gets a 1 MB string, a 100,000-item array and an object with a 1 MB key. Every answer must be under
  64 KiB and each call must return within a bound (on a stub engine: the engine side is the strict-budget harness's,
  see [WORKERS.md](WORKERS.md)).

## Not changed (recommendations)

1. **The tool list itself is the biggest cost** (96.5 KB, about 24,000 tokens per agent). Options, roughly by payoff:
   - **Role-scoped tool lists**: *done 2026-10-29*, see "Scoped tool lists" above.
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

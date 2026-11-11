# Design: authority, jobs and provenance

Status: built 2026-10-29 (the owner asked that recurring bugs be "reproduced and fixed at the root, not instance by
instance"). Foundations **F1** (swarm-first: a second agent must never undo, cancel or approve the first's work
behind its back) and **F5** (the human stays the director: the owner can't be impersonated). Code:
`crates/specula/src/authority.rs` (authority, roles, grants, refusal sites, the per-tool access table),
`crates/specula/src/jobs.rs` (deferred jobs), `crates/specula/src/sight.rs` (`FrameRecord`), `review.rs`
(critique ownership, provenance). Guards: `crates/specula/tests/authority_guard.rs`.

The model is the [write policy](WRITE_POLICY.md): make the class impossible or loud by construction, then add a guard
test that fails when someone adds a new instance.

## The two classes

A root-cause investigation on main (eb354d1) confirmed two recurring classes of review findings.

**Class A: claims and ownership skipped on agent paths.** The write policy covers the world; every other shared
store (job slots, the review and critique queues, the board, leases, cameras, the clock, settings, the profiler) was
mutated directly. Ownership was spelled about ten ways, three deferred-reply jobs (`PendingCapture`, `Fly`,
`BuildWaiter`) had no owner, and authority was a self-asserted string: carl said `role: "owner"` and approved bob's
submission; dave said `role: "lead"` and set the frame floor; any agent could take the name `physics`.

**Class B: data that says something untrue.** Answers started from intent and were annotated with measurements, so
an unmeasured field kept the intent (`sight_query` said `drawn: true` for an entity behind the camera, because its
subjects came from the world, not from what was drawn). Agent-supplied facts sat next to engine facts, and concise
shaping dropped provenance first (a critique's free-text `source`, with `by` hidden in the concise listing; checks a
submitter typed in counted as "all N passed").

## Class A: authority and ownership

### One authority function; roles are never self-declared

`authority::authority(swarm, session) -> Authority` is the only answer to "what may this caller do":
`Agent < Reviewer < Lead < Owner`, each including the ones below it.

- `agent_hello {role}` still names a job ("builder", "playtester", "artist"; task_plan assigns by it). The four roles
  that carry authority, `owner`, `lead`, `director` (the lead's other name) and `reviewer`, are admitted only when
  granted. Asking without a grant is refused (`role_not_granted`), loudly, never quietly downgraded.
- **The owner** is the human: the window (`window:owner`, the review screen) or an operator grant (`--owner NAME`
  when the engine starts, for scripts that act for the owner). Never self-declared over MCP.
- **The lead** comes from the operator (`--lead NAME`, repeatable). **The documented fallback:** when the operator
  granted nothing at all, the first identity whose hello says `lead` (or `director`) holds the lead for the run;
  anyone else asking is refused with `lead_taken`, naming the holder. engine_info's `swarm.grants.fallback_lead` says
  who. **Any operator grant turns the fallback off** (coordinator, 2026-10-29): with `--reviewer rev` and no
  `--lead`, nobody becomes lead by asking first, so a granted reviewer is never outranked by a first-come lead.
- **Reviewers** come from the operator (`--reviewer NAME`); a lead reviews too.
- **A granted name is bound** to the identity that first says hello with it successfully (a refused hello binds
  nothing; its resume token carries it to later sessions). Another session can't take that name, or a look-alike
  of it, for the rest of the run (`grant_bound`). Grant holders and the fallback lead are never pruned from the
  identity table, so they can always come back.
- **Engine writer names are reserved**: every `Exempt` writer's name (`physics`, `nav`, `anim_reload`, the hosted
  game's), `disk`, `engine:stray`, `window:owner`, and the prefixes `module:`, `lex:`, `engine:`, `window:`
  (`name_reserved`); also `taste`, the galleries lane's writer. `world_history` can no longer show an agent as
  `by: "physics"`. **So are the role words**
  (`owner`, `lead`, `director`, `reviewer`) unless the operator granted that name.
- **Agent names are a small, documented class** (coordinator, 2026-10-29): 1..64 characters of ASCII letters and
  digits, space, `_`, `.` and `-`, with no space at either end; anything else is `name_invalid`. No look-alike
  letters, colons (so no writer prefix such as `module:`), combining marks, Roman-numeral or mathematical letters
  or invisible characters can reach a name, so names compare as they read; case aside (`authority::skeleton`):
  `Physics` and `DISK` are the reserved names, `ANN` is `ann`'s (`name_taken`). Roles and critique sources are free
  text, compared by the skeleton's look-alike mapping too (`Lead`, `OWNER`, a Cyrillic `о`).
- **Caller-thread tools** (the tracing, galleries, procgen and studio tools: no `&Swarm` there) read the same
  answer with `authority::of_session(session_id)`, which the swarm publishes on every hello, resume and prune.
- The role literals appear only in `authority.rs` (guard test).

**Groundwork for M6.** Grants by name are as strong as names: before the granted agent first connects, anyone could
say hello as it. M6's per-agent tokens close that: a grant then names a token, and `authority()` stays the one place
that answers. Nothing else reads roles.

Who needs what:

| Action | Needs |
|---|---|
| `swarm_settings {frame_floor_fps}`, `stream_settings` changes | Lead (`lead_only`) |
| `review_decide` (never your own submission), `review_screen` open/next/prev/close | Reviewer (`not_reviewer`) |
| `critique_update` on someone else's critique | Reviewer and at least the author's authority (`not_yours`) |
| `critique_add {source: "owner"}` (or another authority role as the source) | that authority (`source_reserved`) |
| A forced cancel of another agent's job | Lead |
| `force` on another agent's object: `note_delete`, `save_write {overwrite}`, `save_delete`, `playtest_report {cancel}`, `world_undo` of others' entries or over their later changes | Lead (`force_needs_lead`); the owner of the object may always force their own (owner, 2026-10-29) |
| `world_save` under another name (it renames the live world: its autosave target, evidence and board files), or with `overwrite` over an existing world folder the caller didn't save in this run (another agent's, or an earlier run's) | Lead (`lead_only`); saving under the current name is open |
| `world_load {force}`, `save_read {force}` over another agent's unsaved work (journal entries since the last write to the world's files or a save, by another identity or on its behalf) | Lead (`force_needs_lead`, naming `unsaved_by`); the simulation's own changes and the caller's are anyone's to discard; the game's own Load menu is the player's |
| `tools_scope`, `disk_prune {free_mb}` | Lead (`lead_only`) |

### Refusals: one function, stable codes, a call site

Every authority and ownership refusal is made by `authority::refusal(Site, message)`: the code comes from the
`Site` (`role_not_granted`, `lead_only`, `not_reviewer`, `not_yours`, `job_busy`, `not_job_owner`,
`capture_in_progress`...) and the answer carries `refused_at` (the site's name, such as `swarm_settings.frame_floor_fps`).
The tracing lane hooks `refusal` to emit an event per refusal.

### Ownership inside the stores: critiques

`Review::check_critique(writer, authority, id, action, expect)` returns a `CritiqueCheck`, and `apply_critique` takes
one; only `check_critique` can make one (a `compile_fail` doctest pins it). So no critique changes without the
ownership check. The rule: the author may do anything; the assignee may resolve; anyone else needs Reviewer and at
least the author's authority (each critique records the authority it was made with, so an owner's critique is closed
by the owner or resolved by its assignee). A forged `source: "owner"` is refused, in any case or look-alike, and
so is `window:owner`.

**After a restart** a critique's or submission's author is known by name alone (identity keys are per run). A
namesake is then the author only with the authority the critique was made with: an owner's critique isn't closed by
an agent who says hello as the owner's name. A loaded submission is withdrawn by a namesake only with reviewer
authority, and can't be superseded at all (submit anew).

### Jobs

Every request to the engine carries a `jobs::Reply` (`Request::reply`), and every deferred job keeps it. Only
`jobs::channel` makes one (its field is private), and no other module may hold a `Sender` of a tool answer: the
guard fails on any `Sender<..ToolOutput..>` outside `jobs.rs` (fields, parameters, a `Result` around it), on a
generic `type` alias of a `Sender`, and on `Sender` imported under another name. A job that runs one at a time lives
in a `jobs::Slot`, which records its `JobOwner` (name and identity key):

- **Busy is refused, naming the owner** (`job_busy`, with `owner` and `job`), never silently replaced. The owner's
  own new start replaces its old job, and the old one is answered.
- **Cancel is owner-only** (`not_job_owner`); a lead's or the owner's forced cancel is allowed, and the caller logs it
  on the board.
- **`start` runs the caller's check first, always** (claims, as the caller), even when the job changes nothing.

On a `Slot`: `perf_capture` (and `perf_zones {enabled}` is refused while another agent's capture records:
`capture_in_progress`), `stream_fly` (another agent's flight is refused instead of replaced; the answer names
`flown_by`). Queues of jobs that each belong to their caller (none runs one at a time, and no tool cancels or
replaces another's), holding a `Reply`: model build waiters, parked captures, playtest waiters, background saves,
the studio's renders (`Reply<Result<Rendered, ToolOutput>>`), and vfx previews, which are counted per agent
(identity key) and refused naming whose previews fill the queue (`job_busy`, `queued_by`). **For the GI and disk lanes** (bakes, uploads): a bake is a `Slot<Bake>` (or a `Vec<Job<_>>`); `start` with the
caller's `JobOwner` and a `check` of the caller's claims on what the bake writes; `cancel` for a cancel tool;
`Job::answer` to reply. Their `Sender<ToolOutput>` fields become `Reply`s, or the guard fails at merge.

### The access table: every tool declares its authority

`authority::access(tool) -> Access` has an entry for every engine tool:
`ReadOnly | World(Rule) | Owned(Store) | Lead | Reviewer | Owner | Open(reason)`.

- `World(Rule)`: writes the world through the write policy with that rule (claims as the caller).
- `Owned(Store)`: changes items that have owners; the store checks the caller (claims, tasks, notes, intents,
  leases, cameras, review submissions, critiques, jobs, saves, playtests, uploads, saved Lex runs). Module
  loads (`logic_build`, `logic_reload`, `logic_unload`) are `Lead`, and more: native code runs with the owner's user
  rights, so they need the owner or an operator-granted lead (`may_load_native`; the fallback lead isn't enough), and
  a builder's build becomes a proposal the lead approves (MODULES.md, *Who may load a module*). Hooks stay `Open`,
  refused where another agent claims the module's root.
- `Open(reason)`: anyone may, and the reason says why that's right (the clock is shared on purpose, say).

A guard test fails if any tool in `tools/list` has no entry, and the live drill (`scripts/authority_drill.py`) has a
second agent call every `Owned` tool on the first agent's objects: each call must be refused, or the table must name
the Open reason it was allowed.

### One tool call, one commit

Every tool call gets an id on its connection thread (`tools::current_call`), carried by every request it sends to the
engine (`Request::call`), so a caller-thread tool that sends two world edits is counted as one call. The swarm counts
journal entry the request's own dispatch makes on the caller's behalf (`Swarm::begin_dispatch` / `end_dispatch`):
its commits and undos, and what a game, module or live Lex run committed because it asked, **inside sessions too**
(`Live::session`: the Lex and module tools). Ticks the request runs (`time_control step`) are the simulation's:
counting pauses for them (`Engine::advance_clock`), so held input firing as its holder never counts as the
holder's call (review round 2: ten ticks of held input were ten commits, a debug panic). Exempt writers, disk
imports and other agents' commits aren't the caller's. `authority::CallCommits` keeps the totals per call; a call past one fails a debug assertion,
and is logged (`multi_commit`) and counted (`swarm.multi_commit_calls`, engine_info detailed) in release builds. A
tool that commits twice can fail halfway, its first commit standing: the terrain_edit class.

Exempt by name (`commits_independently`), because they decide independent items and answer each on its own:
`review_decide` (each submission decided, promoted or reverted by itself) and logic_build's fix rounds (each round
one commit; the answer lists what was applied and skipped). Found and fixed: `lex_live {restart}` committed twice (a
delete, then start's batch), and a start refused after the delete left the run half gone. Now start's one batch
replaces the group, and a refused start leaves the old run as it was.

## Class B: truthful data

### Drawn facts come from a frame record

`render_view` returns a `FrameRecord`: the entity ids the frame's draw list actually submitted, the ID buffer's
per-entity pixel counts (when the ID pass ran), and streaming's HLOD and not-resident marks. **HLOD proxies** are
chosen by distance, not by view, so a cell behind the camera is still drawn as its proxy (and culled): each of a
capture's proxies gets an ID-pass owner of its own (`sight::proxy_owner`, past every entity id), and a member's
facts come from its proxy's pixels (`drawn: false` behind the camera, `measured_by: "id_buffer"`). The bounds
overlay's "fully hidden" colour comes from the record too (an HLOD member has no pixels of its own). Facts about drawing
(`drawn`, `drawn_as`, `measured_by`) are a `DrawnFact`, which only `FrameRecord::drawn` makes. `sight_query` answers
from it: an entity behind the camera (submitted, culled, no fragment rasterized) is `drawn: false` with the reason; a
frame rendered without the ID pass says `drawn: null, measured_by: "draw_list"` instead of guessing.

GPU tests (skipped without Vulkan) check the record against an independent readback of the ID image:
`capture::frame_record_engine_tests` goes through the engine's real `render_view` (behind the camera, hidden, an
unbuilt model, occluded, a sprite with a missing texture, the ID pass off, an HLOD cell in view and behind the
camera, a model streaming left off the GPU), and `tests/frame_record_gpu.rs` drives the draw list and ID pass
directly. **For the terrain lane:** water and terrain passes add their entity ids to the record when they submit a
draw (or record `drawn_as: "terrain"`), never in the packet directly.

### Provenance by type

- `review::Fact<T>` is either `Engine(T)` (measured) or `Claimed(Claimed<T> { by, value })` (what an agent said).
  A check's `passed` is a `Fact<bool>`: `paths_exist` and `captures_exist` are the engine's, everything typed into
  `review_submit {checks}` is claimed by the submitter (and saved with `reported_by`). Renderers count them apart:
  the review screen shows two lines, "Engine checks: 2 passed" in green (amber when one failed) and "Reported by bob
  (not checked by the engine): 2 passed" in the neutral small text (STYLE.md: an agent's claim never looks like an
  engine pass), instead of "all 4 passed"; listings say "2 engine check(s) passed; 2 reported by bob passed"; and
  `review_list`'s `checks` has `engine {passed, failed, recorded}` and `reported {passed, failed, by}`.
- **Results the engine recorded** (M5, `crate::records`): `style_check`, `studio_check` and every saved capture
  keep their result in memory under an id and the run's epoch, with the session that ran them. They vouch by
  SHA-256 from the OS (`specula_sys::digest`, never hand-rolled): for a capture, of the PNG and packet bytes the engine
  wrote; for a check, per subject, of everything judged (recipe, includes, params, seed, `style.json`, `style.toml`).
  A name an agent can reproduce never vouches for anything (the lane review forged the unkeyed evidence name with one
  solved word). `review_submit {results: [{id, epoch}]}` turns them into engine facts that name their record and its
  subjects (`Check.recorded`, `Capture.recorded`); only `Fact::Engine` carries a record, in memory and in the file.
  The references are verified on the connection thread (inputs re-read and digested, evidence copies digested) and
  leave a short-lived token the engine thread requires. Refused: `foreign_result` (another identity's record, by
  `authority`'s keys, like `captured_by_other`), `stale_result`, `unrelated_result` (a check of none of the submitted
  models), `result_unverifiable`, `result_not_verified`, `result_not_found`. A typed check or capture can't carry
  `recorded` or `engine` (unknown fields). Probes in `scripts/authority_drill.py` (`recorded_results`, the lane
  review's forged PNG, edited packet, loosened `style.toml` and unrelated record among them).
- A critique's `source` is the author's claim, always listed with `by` beside it (`by` and `resolved_by` are in the
  concise listing too).
- `response.rs` has a never-drop list of provenance keys (`by`, `source`, `resolved_by`, `engine`, `submitted_by`,
  `decided_by`, `set_by`, `flown_by`): concise shaping keeps them wherever they appear.
- Guard: for every listing tool, a provenance key in an item of the detailed answer is also in the concise one.

### The two-fresh-engines hash fuzz

`scripts/hash_fuzz.py` replays a corpus (each mutating tool once, with generated arguments, plus the eval and
playtest corpora) on two fresh engines that differ in what must not matter (a cold vs a warm `--cache`, physics
thread counts, injected delays between calls) and compares world, physics and navigation hashes per tick. Any
difference is a determinism leak (wall time, cache state or thread timing in world data). It belongs in the checks the
integrator runs at milestone audits.

## Other instances fixed

- `project_mount` imported a folder as the claims-exempt `disk` writer: an agent's (re)mount now checks the
  caller's claims on `/project/<name>` first (`claimed`), so it can't overwrite files another agent claimed.
- `time_control` stays open to every agent on purpose (agents pause and step for their checks; a playtest owns the
  clock while one runs), and its answer now says who moved the clock last (`set_by: {agent, op}`).
- `perf_zones {enabled}` during another agent's capture is refused (`capture_in_progress`, naming the owner).
- Two new instances of class A arrived with the merge of main (ea192b7), both caught by the role-literal guard:
  `tools_scope` and `disk_prune {free_mb}` checked a self-declared `role: "lead"`. Both now ask
  `authority::require` (`lead_only`, `refused_at` `tools_scope` / `disk_prune.free_mb`). The tool-list templates named
  `lead`, `director` and `reviewer` (`?role=`, `tools_activate {role}`) choose tools, not authority; they keep the
  names through the constants in `authority.rs`. A hello can no longer pick those templates by declaring the role
  without a grant: use `?role=` or `tools_activate {role}`.
- Overrides with `force` on another agent's object need lead authority (owner, 2026-10-29): notes, saves, playtest
  cancels, `world_undo` of others' entries. Without it, `world_undo {force}` runs unforced, so a caller's own clean
  undo still works and one that needed force is refused (`force_needs_lead`).
- `world_save` renamed the live world for anyone (`world_save {name: "bobs_world"}` moved its autosave target, review
  evidence and board files): another name, or `overwrite` over a world folder the caller didn't save in this run,
  needs the lead.
- Scripts that load with force or overwrite earlier runs' worlds say role `lead` (the first-lead fallback; their
  engines grant nothing). The `undo_vandal` eval's solver acts as the lead (task field `solver_role: "lead"`, which
  `evals.py` turns into `--lead solver`), so its reference undoes the vandal's entries with force and its cheat still
  fails on the world diff.

## Tests

- `authority::tests`: roles admitted only when granted, the fallback lead, grants bound to the first identity,
  writer names reserved, `require`, and **every engine tool has an access entry** (Open entries need their reason).
- `jobs::tests`: busy refused naming the owner (never replaced or answered), own restart replaces, owner-only cancel,
  a forced cancel needs a lead, the check runs on every start.
- `swarm::tests`: the M4 findings as rows: carl says owner, dave says lead after the first lead, bob's second
  identity says reviewer (all refused); operator grants, bound names, resumes keep them; every `Exempt` writer's name
  refused (an exhaustive match: a new variant doesn't compile until it is listed).
- `review::tests`: critique ownership (another agent refused on every action; a reviewer may close an agent's but not
  the owner's; the assignee resolves but never dismisses; a stale check refused at apply), a forged `owner` source
  refused, authority through the file; provenance kept in the concise critique and submission listings; the checks
  line never counts reported checks as the engine's; the check token (`compile_fail` doctest on `check_critique`).
- `perf` tests: another agent's capture refused naming the owner; `captured_by_other`.
- `tests/authority_guard.rs`: role literals only in `authority.rs`; `Sender<ToolOutput>` fields only in `jobs.rs`
  (and `tools::Request`); a test of its own scanners.
- `tests/frame_record_gpu.rs` (skips without Vulkan): drawn facts against an independent ID readback.
- Live: `scripts/authority_drill.py` (roles, names, and bob on every owned tool of ann's; it fails when the engine's
  table has an owned tool without a probe), `scripts/hash_fuzz.py`.

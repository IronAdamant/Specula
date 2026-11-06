# Design: the one write policy

Status: built 2026-10-28 (the first process change the owner approved after M3: "a structural fix, not a
checklist"). Foundations **F1** (swarm-first: claims must hold on every path) and **F2** (every change journaled, so
replay stays exact). Code: `crates/specula/src/write_policy.rs`; tests: `write_policy/tests.rs` (the regression table)
and `crates/specula/tests/write_policy_guard.rs` (the guard).

**Why.** In M3, "X bypasses claims" was the most common major review finding. Claims, the reserved-data rules (`/lex`,
`/lex_live`, `/nav`, `/project`, `props.joint`) and the joint rules were re-checked separately on each write path:
`Swarm::commit_with`, `Swarm::undo`, `check_game_changes`, the module handler's `check_module_changes`, game tools,
live Lex hooks, the physics, navigation and anim writers, world loads. Every new path was a fresh chance to forget one.

**Goal.** One policy every change to the live world passes through, with the compiler refusing anything else.

## How bypassing is prevented

What the **compiler** enforces (points 1, 4 and 5) and what is a **run-time backstop** (point 6) are kept apart here
on purpose: review rounds 2 to 4 each fooled the previous round's detection, so the structure carries the weight.

1. **The live world is a `write_policy::Live`.** `Engine.world` is a `Live`: it reads like a `World` (`Deref`), but
   it has no `DerefMut` and its field is private, so no engine code can get a `&mut World` of the live world. This is
   a compile error, not a convention: changing `Engine.world` to `Live` produced 70 errors, one per write site, and
   each was migrated to an entry point.
2. **Every entry point names its writer**, and the writer decides the rules (the table below). An exemption is a named
   variant with its reason (`Exempt::Physics`, `Exempt::Game {..}`...), never a missing check.
3. **Nothing is left behind.** Every entry point ends with no change pending (`Live::settle`): a writer that changed
   the world without journaling or rolling it back is a bug, and the entry point fails closed (point 6). The live
   world always records full changes between entry points.
4. **No writer ever holds a `World`** (review round 4). Every writer's code, at every entry point (agents' commands
   in `Live::commit`, disk imports, game tools, exempt writers, the module host's and live Lex's sessions, and the
   physics, navigation, animation, character, Locus and Lex code they call), gets a handle: `&mut dyn WorldWrite`.
   It reads like a `World` (`Deref` to `&World`) and has only recorded writes: `spawn`, `update`, `delete`,
   `apply_batch`, `set_env`, `set_motion`, and the narrow recorded operations subsystems need (`set_external_pose`
   for ragdolls, `set_camera` / `delete_cameras`, `put_file` for the project tools and disk imports, `rollback_to`
   for the module host's call transactions). It has no way to switch recording, take recorded changes, move
   epochs, set the id counter, take or put raw project files, change the shape budgets, the model library
   or the clock, mark or seal the world, or reach the `World` behind it: `policy_world` takes a `PolicyKey`, whose
   field is private to `write_policy`. Replacing the world through a handle doesn't compile either (`*w = ..` on an
   unsized `dyn`, `mem::swap`). Each is pinned by a `compile_fail` doctest on `WorldWrite`. The hosted game gets a
   `game::GameWorld`, a wrapper over the same handle (doctests on `GameWorld`), so Sepulcra's code is unchanged.
5. **Only the policy writes the live journal.** `Swarm`'s journal field is private; `Swarm::journal()` reads it, and
   `journal_mut` and `claims_and_journal` take the `PolicyKey`. So a session can't append an entry under another
   agent's name, undo one or start the journal over (probes P5, P6; `compile_fail` doctests on `Swarm::journal`).
   A detached `Journal` (a test's, a replay's) is ordinary data.
6. **The run-time backstop.** Points 4 and 5 make the known evasions unspellable. In case a future change opens one
   (a new `&mut World` path, `unsafe`), every entry point still checks, on the live world, in release builds too:
   - **Sealed.** The live world is sealed except while the policy itself journals: `set_recording` (to another mode)
     and `take_changes` change nothing, so every write stays recorded and reaches the checks. Inside agents'
     commands, disk imports and game tools, recording also stays full even through `recording_as` (P1b: Snapshot hid
     a write from the step-by-step claim check). Each attempt is printed and counted.
   - **Counted moves.** The world counts persistent changes made while recording was off, and while it wasn't full,
     in counters nothing ever clears (journaling doesn't: round 3's acknowledgement was cleared by a later
     `journal_exempt`, probe P1d). Rollbacks and the step re-check, which replay recorded changes, don't count. The
     policy compares them around each writer, independently of what is pending (P1e): a change made unrecorded, or
     in a checked writer's code one per entity, is a *hidden write*. `take_files` counts as one.
   - **Marked.** The live world carries a mark that neither `Default` nor `Clone` copies, so a world put in its place
     is noticed. Project files are lent out while writers that don't use them run; the project tools, disk imports
     and undo keep them, and a copy is taken to put back.
   - **Settings.** The id counter (never lowered; a jump is noted in the journal so replay ends past it), the shape
     budgets, the model library and the clock are compared around each writer and put back if moved (P4, P4b).
   - **Fail closed** (`Live::settle`): unjournaled full changes are rolled back; changes recorded one per entity
     (Snapshot) can't be, so they are journaled as `engine:stray`; a hidden write or a replaced world can't be undone,
     so the journal starts over from the world as it is and replay stays exact. Each is a structured log line
     (`unjournaled_write`, `unrecorded_write`, `world_replaced`, `settings_moved`), counted in engine_info
     (`swarm.unjournaled_writes`, `swarm.world_replacements`) and a debug assertion. Agents' commits, disk imports,
     undo, game tools and sessions answer `unjournaled_write` (sessions too: the Lex and logic tools no longer answer
     success after a rollback). Ticks can't be refused; theirs are reported.
   - **Recovered.** A replaced world is made live consistently: marked, sealed and recording, with the clock, the model
     library, the shape budgets and the project files back (lent, or the copy kept for the project tools, disk imports
     and undo: never the empty store of the world put in its place, which the disk sync would take for deletions,
     probe P3). Ids, revisions, subtree stamps, the files revision and the transient, draw, layout and mesh epochs all
     move past the old world's. The engine resets its caches as after a load, and the next autosave rewrites every
     world file. A hidden write in the project tools or a disk import puts the files copy back too (P2).
   - **Known loss.** What a replaced world held is gone (entities, cameras, the journal's history and undo). That is
     the price of a writer's bug, and it is reported, never silent. The backstop can't see a hidden write that moves
     no counter and no setting; that is what point 4 is for.
7. **Replacing the world needs a proof.** `Live::replace` takes a `MayReplace`, which only `may_replace` makes (no
   playtest running, read from the engine's playtests rather than a caller's flag, and no other agent's claim).
   `Live::new` appears once, in `Engine::new` (a guard test). world_load, save_read (at start and again at the swap) and `--world`
   at startup all get one, so a new way to load a world can't skip the check.
8. **The simulation's writers live apart.** The hosted game's exemptions, anim reloads and the module host's and
   Lex tools' sessions are opened only in `sim_writes.rs`, a short file of named functions (`sim_writes::game`,
   `game_start`, `module_tick`, `anim_reload`, `logic_tools`, `lex_tools`). The agent tools' dispatcher (`lib.rs`)
   names no exemption itself. The hosted game's own calls are a `GameCall` (tick, frame, overlay,
   before_capture, assets_changed, apply_state, request_done, the repair after a refused tool), so only the game's
   own methods run as its exempt writes, never a closure. Their reserved-data writes are reported and counted
   (`swarm.game_reserved_writes`), not refused: a tick can't be refused halfway.
9. **Presentation is checked.** A live Lex run's presentation is journaled as the run's own writes, exempt from the
   reserved-data and joint rules. So its `present` rules are checked when the run starts, and again when a save
   brings it back: no `props.joint`, no navmesh settings, no Lex rule data. (Round 3: a present rule tied a joint
   onto a claimed box.)
10. **Ticks tell claim holders.** Module ticks and live Lex runs' ticks stay exempt from claims, which keeps them
   deterministic. A write one makes inside another agent's claim is a *notice* for the claim's holder: by (the module
   or run), started_by (the agent who loaded or started it), path, op, claim and tick. The holder sees them in
   `claim_list` (`notices`), and engine_info counts them (`swarm.claim_notices`). They are bounded per holder (the
   newest 64 each, for at most 64 holders), so one holder's flood never pushes another's out; at most 16 per call.
   `claim_list` reports what was dropped (`notices_dropped`: past the 64 kept, and changes past the per-call cap,
   never examined). Nothing is examined while nobody holds a claim. Whether such writes should ever be
   refused is the owner's decision.
11. **The guard test** (`crates/specula/tests/write_policy_guard.rs`) catches what the compiler can't see, so a
   regression fails even if visibility is loosened later. It is a backstop: points 1 to 6 don't depend on it. It
   lexes every source file the engine compiles. A file is skipped only when a `#[cfg(test)] mod` declares it, never
   because of its name. Comments and literals are blanked, and `#[cfg(test)]` items are skipped by brace depth.
   Matching ignores whitespace, and hatch names match bare (called, in path form, named in a macro, passed as a
   function). It fails if:
   - an **escape hatch** appears outside the files that own it: taking recorded changes (`take_changes`), switching
     recording off, raw puts and removes that skip a write's validation, rollbacks, the step re-check, pruning,
     journal appends, rebases and id notes, swapping project files;
   - an **exemption or reserved rule** is used outside its owner (`Exempt::Physics` only in `physics.rs`,
     `Rule::Nav` only in `nav_tools.rs`, `Opener::LexLive` only in `lex_live.rs`...);
   - engine code uses **Swarm's write methods** (`swarm.commit`, `game_changes`...: they remain for detached worlds
     and tests);
   - any struct outside the listed files gets a **field holding a `World`** (`World`, `Box<World>`...), `Live` gains
     `DerefMut` or a public field, or a function hands out a mutable world (`-> &mut World`, `-> &'a mut World`,
     `-> Option<&mut World>`);
   - variants are imported bare (`use ...::Exempt::*`), or the policy's names, `World` or `mem::swap/take/replace`
     are imported under another name;
   - a `#[path]` module appears (one allowed: `physics_save.rs`);
   - the new hatches (sealing, marking, lending files, recovering, `clone_from`, `recording_as`, the policy's key and
     `policy_world`, `journal_mut`, `claims_and_journal`, `drop_changes`, the handle's `put_file`) appear outside
     their owners;
   - code outside `write_policy.rs` replaces a world wholesale (`*world = ..`, `*wd = ..`, `mem::swap/take/replace`
     of one), or writes project files outside the project tools (`put_file_raw`).

   **Its limits, honestly.** It is a text scan at file granularity: an owner file may use its exemption anywhere in
   that file, so a new write path in an owner file (`sim_writes.rs`, `lex_live.rs`, `physics.rs`, `nav_tools.rs`)
   needs a reviewer's eye. Picking the wrong *named* exemption is the one thing only review catches. Aliases no
   longer matter for the journal (point 5: the field is private). `unsafe` code could forge the policy's key (a
   zero-sized value); AGENTS.md rule 8 keeps `unsafe` to FFI boundaries, and review checks it.

   **What the `compile_fail` doctests prove.** Those on public methods (`take_changes`, `set_recording`,
   `set_shape_limits`, `adopt_clock`, `set_model_source`, the journal's `rebase`) prove the handle doesn't reach
   them. Methods that are `pub(crate)` (`take_files`, `replay_next_id`, `put_file_raw`, `mark_live`) would fail in a
   doctest anyway; inside the crate they are off the handle because `WorldWrite` doesn't declare them and its
   `Deref` gives `&World`. Controls in the guard test spell the allowed uses of the same paths, so the doctests fail
   for the reason they state.

Detached worlds (the journal's base and replays, a save being read off the engine thread, a world read from its files
before it goes live, tests, the hosted game's offline builds) are plain `World`s: they are data, not the shared world
agents build in, so their `World` methods stay public.

## The writers and their rules

| Writer (entry point) | Reserved data | Transient entities | Claims | Joint rules | Journaled |
|---|---|---|---|---|---|
| An agent's tool (`Live::commit`, `Rule::World`) | all refused | refused | step by step, as the agent | on the result | under the agent, undoable |
| ... owning one kind (`Rule::Joint`, `Nav`, `Lex`, `LexLive`, `Project`, `Vfx`, `Taste`, `Gi`) | its kind allowed | refused | as the agent (`Taste`: never; nobody claims taste state) | as above; `Joint`: the tool checks its bodies itself | under the agent |
| A taste decision's results (`Rule::TasteResult`: gallery_apply, gallery_decide with `apply_on_accept`, taste_suggest `apply`, an approved configurator proposal) | project files only, taste state included | refused | as the agent on the result files, never on the taste state | | under the agent, **one** entry for the results and the taste state that records them (`Engine::project_put_all`; one tool call, one commit) |
| `world_undo` (`Live::undo`) | (restores what was) | (as recorded) | step by step, as the agent; `force` never overrides them | step by step | under the agent, kind `undo` |
| What an agent asked the hosted game, a module or a live Lex run to do (`Live::game_tool`, `game_commit`, `module_agent_commit`) | all refused (joints allowed) | the game's own | step by step, as the agent | step by step | under the game or module, `via` the agent; on refusal the world rolls back and the caller rolls its own state back |
| A module's own request (`module_request`) | all refused | | no (the call's commit decides) | no | the call is journaled by its commit |
| Files read from disk (`Live::disk`) | | | **exempt:** a human's edit can't be refused (F5) | | under `disk` |
| The hosted game's own calls (`Exempt::Game`, `Exempt::GameStart`) | **exempt** | its own | **exempt** | **exempt** | under the game, replay-only, coalesced |
| A module's tick (`Exempt::Module` via `module_tick_commit`) | per request (above) | | **exempt** | **exempt** | under `module:<name>` |
| A live Lex run's ticks, presentation and local input (`Exempt::LexLive`) | | | **exempt** | **exempt** | under `lex:<set>` |
| Physics (`Exempt::Physics`), navigation (`Exempt::Nav`), anim reloads (`Exempt::AnimReload`) | | | **exempt** | **exempt** | under `physics`, `nav`, `anim_reload` |
| Replacing the world (`Live::replace`, needs `MayReplace`) | | | refused while another agent holds a claim; never during a playtest | | the journal starts over from the new world |

**Why the exemptions.** The simulation's own writers are the game's rules: claims hold agents back, not the game being
made (PHYSICS.md "Joints and claims, every write path"), and a tick can't be refused halfway without the game's state
and the world parting. They still journal every change (replay-only, coalesced per tick), so history, the feed,
replay and `undo_conflict` all see them. The hosted game is code linked into the engine, trusted as the engine is, so
its own calls skip the reserved-data rules too; what an agent asks it to do never does.

**Versions** (`expect_version`, versions that only go up, restores past a removed entity's last version) are the
world's own invariants: `World::update`, `World::delete` and the journal's undo enforce them for every world, live or
detached, so the policy relies on them instead of re-checking. **Pruning** of removed versions (`prune_retired`)
runs after every journaled write. The joint index keeps the joint rules at an index lookup per change, and with no
claim that could refuse the writer only the reserved-data and transient rules run (O(changes); tested).

## The audit (2026-10-28, main at 8de7d62)

Every code path that mutated the live world before this change, who triggers it and what it checked, and the entry
point it goes through now. Behaviour is unchanged on every path (same checks, same order, same journal entries).

| Path | Triggered by | Checked before (M3) | Now |
|---|---|---|---|
| world_spawn, world_set, world_delete, world_batch, world_env | agent | `Swarm::commit`: reserved, joints, transient, claims | `Live::commit`, `Rule::World` |
| camera_set, camera_delete | agent | `Swarm::commit` | `Live::commit`, `Rule::World` |
| world_place (Locus), physics component sets, ragdoll_set, nav_agent and ai_set, anim batches | agent | `Swarm::commit` | `Live::commit`, `Rule::World` |
| nav_build (and remove) | agent | `Swarm::commit_nav` | `Live::commit`, `Rule::Nav` |
| procgen_scatter `spawn`, procgen_terrain `collider` (M4) | agent | a world_batch / world_set the tool sends as the caller | `Live::commit`, `Rule::World`: the scatter's spawns are ONE batch, all or nothing (`procgen_tools::spawn_command`, `collider_command`; table rows) |
| physics_joint | agent | `Swarm::commit_joint` (+ its own body checks) | `Live::commit`, `Rule::Joint` |
| lex_edit, lex_import | agent | `Swarm::commit_lex` | session (`Opener::LexTools`) + `commit`, `Rule::Lex` |
| lex_live start and stop | agent | `Swarm::commit_lex_live` | session (`Opener::LexLive`) + `commit`, `Rule::LexLive` |
| project_write, project_patch, project_delete, logic_build `fix: true` | agent | `Swarm::commit_project` | `Live::commit`, `Rule::Project` |
| vfx_set (the effect library, `/vfx`; M4) | agent | (new) | `Live::commit`, `Rule::Vfx` |
| project files read from disk (mounts, the watcher) | a human (disk) | `Swarm::commit_disk`: no claims | `Live::disk` (exempt) |
| world_undo | agent | `Swarm::undo`: claims + joint rules per step | `Live::undo` |
| play_* game tools | agent via the game | `Swarm::game_tool` → `check_game_changes`; on refusal checkpoint restore, or `world_rolled_back` journaled by hand | `Live::game_tool`; the repair is `Exempt::Game` (kind `game_tool`) |
| logic_build/logic_reload loads, logic_unload, logic_hook | agent via a module | `check_game_changes` (agent_commit); `check_module_changes` per request | session (`Opener::Modules`) + `module_agent_commit`; `module_request` |
| lex_live commands and held input | agent via a run | `check_game_changes`, hooks staged | session (`Opener::LexLive`) + `game_commit` |
| lex_live local input (window, gamepad) | the local player | `game_changes` | `journal_exempt`, `Exempt::LexLive` |
| the game's start | game | `game_changes` (full recording) | `sim_writes::game_start` (`Exempt::GameStart`) |
| the game's tick, frame, overlay, before_capture, request_done, assets_changed, apply_state | game | 7 call sites each setting Snapshot and calling `game_changes` | `sim_writes::game` (`Exempt::Game`; via `Engine::game_call`), sealed |
| module ticks | module | `game_changes` per call; `check_module_changes` per request | session (`Opener::Modules`) + `module_tick_commit` (`Exempt::Module`); `module_request` |
| live Lex ticks, presentation, hooks a run's tick calls | live Lex run | `game_changes` | `journal_exempt`, `Exempt::LexLive` |
| physics tick (pose write-back, ragdoll external poses) | physics | Snapshot + `game_changes` | `Live::exempt`, `Exempt::Physics` |
| navigation tick | nav | Snapshot + `game_changes` | `Live::exempt`, `Exempt::Nav` |
| anim sets changed on disk | engine (disk) | `game_changes` (`anim_reload`) | `Live::exempt`, `Exempt::AnimReload` |
| world_load | agent | playtest refusal and `check_replace_world` (lib.rs), then `replace_world` | `may_replace` → `Live::replace` |
| save_read (tool or the game's menu) | agent, player | `check_load_allowed` (save_tools) at start and at the swap | `may_replace` at both; its proof goes to `Live::replace` |
| `--world` at startup | operator | none (no agents yet) | `may_replace` (always allowed then) → `Live::replace` |
| playtests | agent | the paths above as the agent that started them; world_load and save_read refused while one runs | unchanged (the paths above) |
| module load/reload/unload/disable records | module host | `journal.append_module` in lib.rs | `write_policy::module_event` |
| review_decide approve with `promote` (and the review screen's Approve) | agent, or the owner at the window as `window:owner` | (M4, new) | `Live::commit`, `Rule::World`, one batch as the decider |
| review_decide reject with `revert` | agent | (M4, new) | the queue's own refusals (`revert_request`), then `Live::undo` as the decider |
| model builds landing, recipes reloaded | engine | `refresh_model_handles` (derived data, never recorded; after file changes also the unresolved model entities) | `Live::refresh_model_handles` |
| the clock, shape budgets | engine | `set_clock_s`, `set_shape_limits` (not world state) | `Live::set_clock_s`, `Live::set_shape_limits` |

Project files share the journal: `Change::File` writes (`put_file_raw` inside a command) are checked like entities
(reserved for `Rule::Project`, claims on `/project/...` paths). Reads (sight, queries, saves being written, streaming,
desync traces) need nothing: `Live` derefs to `&World`.

## Tests

- **The regression table** (`write_policy/tests.rs`, `every_write_path_holds_the_policy`): 50 rows, every *writer*
  the policy knows (each `Rule`, `Exempt` and `Opener`, undo, game commits, module requests, disk, replacement), tried
  against alice's path- and region-claimed boxes and against reserved data, from one fixture. It drives the policy's
  entry points directly. The engine's call sites (which `Rule` each tool passes, that `Command::Load` asks
  `may_replace`) need the GPU-backed `Engine`. They are covered by the compiler (every site had to pick an entry
  point), the guard, the subsystems' own tests (lex_live's rig drives `apply_command` against claimed hooks) and the
  live scripts below. Each
  refused row must leave the world exactly as it was (only the id counter moves on), nothing pending and no change
  journaled; each exempt row must journal under its own name; and the journal must replay to the live world after
  every row. The M3 bypass findings are rows marked `M3`: undo of a joint's removal, a module hook tying a joint onto a
  claimed body, deleting or restoring a jointed body, deleting the group holding one, giving it another kind of body,
  reserved `/project`, `/lex` and `/lex_live` writes by game tools, module requests and live Lex hooks, world_batch
  mixed ops (claimed and reserved), world_load under a playtest.
- `the_table_names_every_writer`: a new `Rule`, `Exempt` or `Opener` variant doesn't compile until the table has a
  row for it.
- **The compiler** (point 4, 5): `compile_fail` doctests on `WorldWrite` (taking changes, recording off, `*w = ..`,
  `mem::swap`, the shape budgets and clock, the model library, making the key), `GameWorld` and `Swarm::journal`
  (rebasing through the read-only journal, the private field), with controls in the guard test.
- **The backstop** (point 6), with writers that hold the world itself (`Live::raw_writer_for_test`,
  `raw_commit_for_test`, `raw_disk_for_test`: what no handle allows):
  `hiding_or_replacing_inside_a_commit_is_caught_however_spelled` (review round 3's probes as rows: path form,
  macro, recording off, `mem::swap`, an aliased reference, `clone_from`, a renamed swap, a modified clone), and the
  rows marked *replaced* (`*w = World::default()`, `mem::swap`, `mem::take`, a clone). Each must be reported, keep the
  clock, project files and ids, and replay.
- **Review round 4's probes**: `p1_a_command_cant_switch_recording_to_hide_a_write` (P1, P1b),
  `p1_hidden_writes_in_exempt_writers_and_sessions_are_caught` (P1c, P1d, P1e),
  `p1f_starting_over_after_a_hidden_write_is_deterministic`, `p2_files_taken_inside_a_project_command_are_put_back`,
  `p3_a_world_replaced_in_a_project_command_or_disk_import_keeps_the_files` (the data-loss case: files and files
  revision), `p4_engine_settings_moved_in_a_command_are_put_back` (P4, P4b); P5 and P6 are `compile_fail` doctests.
  `claim_notices_are_bounded_per_holder`.
- `lex_live::presentation_tests` covers presentation. `ticks_writing_inside_a_claim_notify_its_holder` and
  `the_games_own_reserved_writes_are_reported` cover points 10 and 8.
- `a_writer_that_leaves_changes_unjournaled_is_caught`, `a_sealed_writer_cant_hide_its_writes`,
  `exempt_writers_record_as_documented`,
  `with_no_blocking_claim_a_batch_does_no_claim_work` (the fast path stays O(changes)).
- **The guard** (`tests/write_policy_guard.rs`), above, with a test of its own scanner (`#[cfg(test)]` fields,
  literals, path forms, spacing).
- Engine-level checks on every path stay where they were: `swarm.rs`, `claims_steps.rs`, `joint_claims.rs`,
  `journal_replay.rs`, Sepulcra's `tests/claims.rs`, and the live scripts (`swarm_drill.py`'s attacks,
  `joints_check.py`, `project_check.py`, `save_check.py`, `world_load_check.py`).

## Adding a write path (the only way: through the policy)

1. **Pick the writer.** An agent's tool is `self.world.commit(&mut self.swarm, tick, w, kind, Rule::World, |wd| ...)`.
   Something the agent asked a game or module to do goes through `game_commit` (or `Live::game_tool`). The
   simulation's own per-tick writer is `self.world.exempt(&mut self.swarm, tick, Exempt::..., |wd| ...)`. A writer
   that commits several times in one call opens a `self.world.session(...)` and commits through this module's
   functions inside it.
2. **New reserved data** (an M4 lane's `/vfx` library, say): add a `Rule` variant for its tools, a clause in
   `reserved()`, rows in the regression table (refused for everyone else, allowed for its tools, claims still apply)
   and its owner in the guard's `OWNERS`.
3. **A new exemption** needs a reason the owner would accept (the simulation's own rules), an `Exempt` variant with
   that reason in its doc comment, its owner file in the guard, and a table row.
4. **Never** add a `World` field to an engine struct, a `&mut World` accessor, or a hatch call (the guard lists them):
   if the policy lacks what you need, add a narrow, named entry point here.

For the parked M4 lanes: review promotion writes project files and world entities as the approving agent
(`Rule::Project` / `Rule::World` commits); the `/vfx` library is new reserved data (step 2: built, `Rule::Vfx`, with
the effect rules, `crate::vfx::write_rules`, on every checked path; VFX.md "Photosensitivity"); procedural scatter spawns
are an agent's batch (`Rule::World`, one `apply_batch`, all or nothing).

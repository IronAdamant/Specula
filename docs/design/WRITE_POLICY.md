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

1. **The live world is a `write_policy::Live`.** `Engine.world` is a `Live`: it reads like a `World` (`Deref`), but
   it has no `DerefMut` and its field is private, so no engine code can get a `&mut World` of the live world except
   inside one of `Live`'s entry points. This is a compile error, not a convention: changing `Engine.world` to `Live`
   produced 70 errors, one per write site, and each was migrated to an entry point.
2. **Every entry point names its writer**, and the writer decides the rules (the table below). An exemption is a named
   variant with its reason (`Exempt::Physics`, `Exempt::Game {..}`...), never a missing check.
3. **Nothing is left behind.** Every entry point ends with no change pending (`Live::settle`): a writer that changed
   the world without journaling or rolling it back is a bug. Debug builds assert; release builds journal the changes
   as `engine:stray` (so replay stays exact), print a line and count them (`engine_info` `swarm.unjournaled_writes`).
   The live world always records full changes between entry points.
4. **The hosted game never holds a `World`.** Its `Ctx` carries a `game::GameWorld`: it reads like a `World` and
   writes entities and the environment (`spawn`, `update`, `delete`, `apply_batch`, `set_env`, `set_motion`). It has
   no `DerefMut` and no accessor, so a game can't replace the world (`*ctx.world = World::default()`), stop it
   recording or take its recorded changes. Each is a compile error, pinned by `compile_fail` doctests on
   `GameWorld`. Review round 1 found a game tool switching to `Recording::Snapshot` (no step-by-step trace) to move
   another agent's claimed box; round 2 found one replacing the whole world. Both are now impossible from a game.
   Sepulcra's change was mechanical: `&mut World` became `&mut GameWorld<'_>` in its helpers.
5. **Writers in the crate can't hide either.** The physics, nav and anim writers, the module host and live Lex do
   still hold `&mut World`.
   - **Sealed.** While an exempt writer runs, the world is sealed: `set_recording` and `take_changes` change nothing,
     so its writes stay journaled. Each attempt is printed and counted.
   - **Marked.** Exempt writers and sessions run *guarded*. The live world carries a mark that neither `Default` nor
     `Clone` copies, and the project files are lent out while the writer runs (it doesn't use them).
   - **Recovered.** If the world the writer leaves isn't the marked one (`*w = World::default()`, `mem::swap`,
     `mem::take`, a clone put in its place), the policy reports an error: a structured log line, engine_info
     `swarm.world_replacements` and `last_world_replacement`, a debug assertion. It then makes the new world live
     consistently: marked, unsealed, recording, with the clock, the model library, the shape budgets and the project
     files put back, and no id or revision repeated. The journal starts over from it (so replay stays exact), and
     the engine resets its own caches as after a load.
   - **Known loss.** What the old world held is gone (entities, cameras, the journal's history and undo). That is the
     price of a writer's bug, and it is reported, never silent. The guard test also refuses the usual spellings of
     a wholesale replacement in the crate's code outside `write_policy.rs`.
6. **Replacing the world needs a proof.** `Live::replace` takes a `MayReplace`, which only `may_replace` makes (no
   playtest running, no other agent's claim). world_load, save_read (at start and again at the swap) and `--world`
   at startup all get one, so a new way to load a world can't skip the check.
7. **The simulation's writers live apart.** The hosted game's exemptions, anim reloads and the module host's and
   Lex tools' sessions are opened only in `sim_writes.rs`, a short file of named functions (`sim_writes::game`,
   `game_start`, `module_tick`, `anim_reload`, `logic_tools`, `lex_tools`). The agent tools' dispatcher (`lib.rs`)
   names no exemption itself.
8. **The guard test** (`crates/specula/tests/write_policy_guard.rs`) catches what the compiler can't see, so a
   regression fails even if visibility is loosened later. It lexes the crate's non-test sources: comments and
   literals are blanked, and `#[cfg(test)]` items are skipped by brace depth. Matching ignores whitespace, and method
   patterns also match their path form (`World::take_changes(w)`). It fails if:
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
   - variants are imported bare (`use ...::Exempt::*`), which would hide them from the owner check;
   - code outside `write_policy.rs` replaces a world wholesale (`*world = ..`, `*wd = ..`, `mem::swap/take/replace`
     of one), or writes project files outside the project tools (`put_file_raw`).

   **Its limits, honestly.** It is a text scan at file granularity. It can't see an alias (`let j = &mut
   swarm.journal; j.undo(..)`, `use Exempt as E`), and an owner file may use its exemption anywhere in that file. A
   new write path in an owner file (`sim_writes.rs`, `lex_live.rs`, `physics.rs`, `nav_tools.rs`) needs a reviewer's
   eye. The compiler (`Live`), the sealed world and the settle check don't depend on the scan.

Detached worlds (the journal's base and replays, a save being read off the engine thread, a world read from its files
before it goes live, tests, the hosted game's offline builds) are plain `World`s: they are data, not the shared world
agents build in, so their `World` methods stay public.

## The writers and their rules

| Writer (entry point) | Reserved data | Transient entities | Claims | Joint rules | Journaled |
|---|---|---|---|---|---|
| An agent's tool (`Live::commit`, `Rule::World`) | all refused | refused | step by step, as the agent | on the result | under the agent, undoable |
| ... owning one kind (`Rule::Joint`, `Nav`, `Lex`, `LexLive`, `Project`) | its kind allowed | refused | as the agent | as above; `Joint`: the tool checks its bodies itself | under the agent |
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
| physics_joint | agent | `Swarm::commit_joint` (+ its own body checks) | `Live::commit`, `Rule::Joint` |
| lex_edit, lex_import | agent | `Swarm::commit_lex` | session (`Opener::LexTools`) + `commit`, `Rule::Lex` |
| lex_live start and stop | agent | `Swarm::commit_lex_live` | session (`Opener::LexLive`) + `commit`, `Rule::LexLive` |
| project_write, project_patch, project_delete, logic_build `fix: true` | agent | `Swarm::commit_project` | `Live::commit`, `Rule::Project` |
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
| model builds landing, recipes reloaded | engine | `refresh_model_handles`, `refresh_models` (derived data, never recorded) | `Live::refresh_model_handles`, `Live::refresh_models` |
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
- Rows marked *replaced*: an exempt writer doing `*w = World::default()`, `mem::swap` and `mem::take`, and a session
  putting a clone in the live world's place. Each must be reported, keep the clock, project files and ids, and replay.
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
(`Rule::Project` / `Rule::World` commits); the `/vfx` library is new reserved data (step 2); procedural scatter spawns
are an agent's batch (`Rule::World`, one `apply_batch`, all or nothing).

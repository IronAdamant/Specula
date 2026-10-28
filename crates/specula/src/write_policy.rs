//! The one write policy: every change to the engine's live world goes through here.
//!
//! In M3 the claim, reserved-data and joint rules were checked separately on each write path
//! (agents' commits, undo, game tools, module requests and hooks, live Lex, ticks, loads), and
//! every new path was a fresh chance to forget one: "X bypasses claims" was the most common major
//! review finding. This module makes that class of bug structural (docs/design/WRITE_POLICY.md):
//!
//! - **The live world is a [`Live`].** The engine holds its world only as a `Live`, which reads
//!   like a [`World`] (`Deref`) but has no `DerefMut` and a private field: the compiler refuses
//!   any `&mut World` of the live world outside this module's entry points. Detached worlds (the
//!   journal's base, replays, a save being read, tests, the hosted game's offline builds) are
//!   plain [`World`]s: they are data, not the shared world agents build in.
//! - **Every entry point names its writer**, and the writer decides the rules: an agent's tool
//!   ([`Live::commit`] with a [`Rule`]), an undo ([`Live::undo`]), a game tool or module call an
//!   agent asked for ([`Live::game_tool`], [`game_commit`]), files read from disk
//!   ([`Live::disk`]), the simulation's own writers ([`Exempt`]: the hosted game, module and live
//!   Lex ticks, physics, navigation, anim reloads), a world replacement ([`Live::replace`]) and
//!   writers that commit as they go ([`Live::session`]: the module host, live Lex, the logic
//!   tools). An exemption is a named variant with its reason, never a missing check.
//! - **The decisions, in one place.** [`commit`] (agents' tools), [`undo`], [`game_commit`]
//!   (what an agent asked a game, a module or a live Lex run to do), [`module_request`] and
//!   [`journal_exempt`] compose the same rules for their writers: reserved data ([`reserved`]),
//!   transient entities, claims (path and region, step by step) and the joint rules
//!   ([`crate::claims::joint_rule_changes`]). Journaling, attribution and the removed-version
//!   pruning ([`prune_retired`]) follow from the writer too.
//! - **Nothing is left behind.** Every entry point ends with no change pending: a writer that
//!   changed the world without journaling it (or rolling it back) is a bug, caught by a debug
//!   assertion, and journaled as `engine:stray` in release builds so replay stays exact.
//! - **Nothing can hide.** While the hosted game or an exempt writer holds the live world it is
//!   sealed ([`World::set_sealed`]): switching recording or taking the recorded changes does
//!   nothing (and is reported), so their writes stay checked and journaled.
//! - **A guard test** (`crates/specula/tests/write_policy_guard.rs`) fails if the escape hatches
//!   (taking or hiding recorded changes, raw puts, journal appends) or the exempt writers appear
//!   outside the files that own them, or if anything else gets a field of type `World`.
//!
//! Versions (`expect_version`, versions that only go up, restores past a removed entity's last
//! version) are the world's own invariants: [`World::update`], [`World::delete`] and the journal's
//! undo enforce them for every world, so the policy relies on them rather than re-checking.

use crate::claims::Claims;
use crate::game::GameWorld;
use crate::journal::{self, Kind, UndoRequest, Undone, Writer};
use crate::logic::Phase;
use crate::swarm::Swarm;
use crate::world::{Change, Mark, ModelSource, Recording, World, WorldError};
use std::ops::Deref;
use std::sync::Arc;
use std::time::Instant;

/// The reserved data an agent's write may touch: each tool family owns its own (everything else
/// is refused with `reserved` whoever writes it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// world_* tools, cameras, Locus, physics, ragdoll and nav-agent components, anim batches:
    /// no reserved data.
    World,
    /// physics_joint: joint data (`props.joint`). It checks the joint's bodies against claims
    /// itself, so the joint rules don't run again on its changes.
    Joint,
    /// The nav tools: `/nav` and `props.navmesh`.
    Nav,
    /// The Lex tools: `/lex`, Lex rule data.
    Lex,
    /// lex_live start and stop: entities in `/lex_live` (runs' groups).
    LexLive,
    /// The project tools: project files (`/project`, [`Change::File`]).
    Project,
}

/// Writers exempt from claims: the simulation's own rules, journaled under their own names. Claims
/// hold agents back, not the game being made (PHYSICS.md "every write path"), and a tick can't be
/// refused halfway without the game's state and the world's parting. Each still journals every
/// change (replay-only, coalesced), so history, the feed and replay see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exempt<'a> {
    /// The hosted game's own calls (`tick`, `frame`, `overlay`, `before_capture`, `apply_state`,
    /// `request_done`, `assets_changed`, and the repair after a refused game tool without a
    /// checkpoint), journaled as `kind` under its name, one change per entity per call. Not
    /// checked for reserved data either: the game is code linked into the engine, trusted as the
    /// engine is.
    Game { name: &'a str, kind: Kind },
    /// The hosted game's `start` (as [`Exempt::Game`], but it builds the scene once and its
    /// entry keeps every change as made, full states included).
    GameStart { name: &'a str },
    /// A module's own call outside any agent's request (its ticks, a hook a live Lex run's tick
    /// calls), under `module:<name>`. Each request it made was already checked for reserved data
    /// ([`module_request`]).
    Module { agent: &'a str, kind: Kind },
    /// A live Lex run's own ticks, its presentation and its local input (window, gamepad),
    /// under `lex:<set>`. Agents' commands to a run are checked as theirs ([`game_commit`]).
    LexLive { agent: &'a str },
    /// Physics moving bodies (pose write-back, the hot path): under `physics`.
    Physics,
    /// Navigation steering agents: under `nav`.
    Nav,
    /// Anim sets changed on disk resettle the graphs that use them: under `anim_reload`.
    AnimReload,
}

impl Exempt<'_> {
    /// The name the changes are journaled under.
    pub fn by(&self) -> &str {
        match self {
            Exempt::Game { name, .. } | Exempt::GameStart { name } => name,
            Exempt::Module { agent, .. } | Exempt::LexLive { agent } => agent,
            Exempt::Physics => crate::physics::AGENT,
            Exempt::Nav => crate::nav::AGENT,
            Exempt::AnimReload => crate::anim::RELOAD_AGENT,
        }
    }

    /// The journal kind of its entries.
    pub fn kind(&self) -> Kind {
        match self {
            Exempt::Game { kind, .. } | Exempt::Module { kind, .. } => *kind,
            Exempt::GameStart { .. } => Kind::GameStart,
            Exempt::LexLive { .. } => Kind::GameTick,
            Exempt::Physics => Kind::Physics,
            Exempt::Nav => Kind::Nav,
            Exempt::AnimReload => Kind::Set,
        }
    }

    /// How [`Live::exempt`] records while the writer runs: one change per entity per call
    /// (cheap, replay-only) for the per-tick writers; full states for the game's start and the
    /// rare anim resettle.
    fn recording(&self) -> Recording {
        match self {
            Exempt::GameStart { .. } | Exempt::AnimReload => Recording::Full,
            _ => Recording::Snapshot,
        }
    }
}

/// Who opens a [`Live::session`]: writers that commit as they go, through this module's
/// functions ([`commit`], [`game_commit`], [`journal_exempt`], [`rolled_back`]), because one call
/// holds several commits (a module tick commits each module's call; a live Lex command commits
/// with its hooks' changes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opener {
    /// The module host: ticks ([`module_tick_commit`]) and an agent's load, unload or hook
    /// ([`module_agent_commit`]).
    Modules,
    /// Live Lex: runs' ticks, input and agents' commands, start and stop.
    LexLive,
    /// The Lex tools (lex_edit, lex_import...): they commit with [`Rule::Lex`].
    LexTools,
}

/// What a write may do with reserved data: the reserved kind it owns (if any), and whether joint
/// data counts (agents only; games and modules may make joints, whose bodies the joint rules
/// check).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reserved {
    owns: Option<Rule>,
    joints: bool,
}

impl Reserved {
    /// An agent's tool under `rule`.
    fn agent(rule: Rule) -> Reserved {
        Reserved { owns: Some(rule), joints: true }
    }
    /// Hosted games' tools, modules' requests and the calls agents ask them for.
    const GAME: Reserved = Reserved { owns: None, joints: false };
}

/// The reserved-data rules, for every writer that runs them: `/lex` (and Lex rule data anywhere)
/// only through the Lex tools, entities in `/lex_live` only from lex_live (deleting there is
/// anyone's: a run shows what it needs again), `/project` only through the project tools (and no
/// entity there), `/nav` and navmesh settings only through the nav tools, and (for agents) joint
/// data only through physics_joint.
fn reserved(changes: &[Change], r: Reserved) -> Result<(), WorldError> {
    let owns = |rule| r.owns == Some(rule);
    // The /lex subtree is Lex rule data, written only through the lex_* tools, and /lex_live
    // holds live runs' groups and output, spawned only by lex_live (so both can be exempt from
    // region claims without letting anything hide there); joint data only through physics_joint
    // (a joint moves the bodies it links).
    if !owns(Rule::Lex) {
        if let Some(c) = changes.iter().find(|c| crate::claims::touches_rule_data(c)) {
            return Err(WorldError::new("reserved", format!("'{}' is in /lex, which holds Lex rule data: change it with lex_edit / lex_import (lex_get and lex_list read it)", c.path()))
                .with("path", c.path().to_owned()));
        }
    }
    if !owns(Rule::LexLive) {
        if let Some(c) = changes.iter().find(|c| crate::claims::touches_lex_live(c)) {
            return Err(WorldError::new(
                "reserved",
                format!("'{}' is in /lex_live, where live Lex runs show their entities: lex_live starts and stops runs (world_delete works there; a run shows what it needs again)", c.path()),
            )
            .with("path", c.path().to_owned()));
        }
    }
    if r.joints && !owns(Rule::Joint) {
        if let Some(c) = changes.iter().find(|c| crate::claims::touches_joint_data(c) && !matches!(c, Change::Delete { .. })) {
            return Err(WorldError::new(
                "reserved",
                format!("'{}' is a joint (props.joint): make, change or remove joints with physics_joint, which checks the right to move both bodies", c.path()),
            )
            .with("path", c.path().to_owned())
            .with("field", "props.joint"));
        }
    }
    // /project holds project files, written only through the project tools; no entity may live
    // there (a claim on a file must never cover something else).
    if let Some(c) = changes.iter().find(|c| (!owns(Rule::Project) && matches!(c, Change::File { .. })) || (!matches!(c, Change::File { .. }) && crate::project::under_root(c.path()))) {
        return Err(WorldError::new(
            "reserved",
            format!("'{}' is in /project, which holds project files: change them with project_write / project_patch / project_delete (project_list and project_read read them)", c.path()),
        )
        .with("path", c.path().to_owned()));
    }
    // Likewise /nav and navmesh settings: only the nav tools write them.
    if !owns(Rule::Nav) {
        if let Some(c) = changes.iter().find(|c| crate::claims::touches_nav_data(c)) {
            return Err(WorldError::new(
                "reserved",
                format!("'{}' is navmesh data (/nav, props.navmesh): change it with nav_build (nav_build remove: true deletes one; nav_info reads them)", c.path()),
            )
            .with("path", c.path().to_owned()));
        }
    }
    Ok(())
}

/// The joint rules for one change ([`crate::claims::joint_rule_changes`]): every body it moves
/// through a joint must be one `w` may change. An index lookup per change (nothing in a world
/// without joints).
fn joint_rules(claims: &Claims, world: &World, change: &Change, w: Writer, now: Instant) -> Result<(), WorldError> {
    for body in crate::claims::joint_rule_changes(world, change) {
        claims.check(world, std::slice::from_ref(&body), w, now).map_err(|e| {
            e.with("note", "this change ties, lets go of or pulls bodies linked by a joint (props.joint), so it needs the right to change them").with("joint_change", change.path().to_owned())
        })?;
    }
    Ok(())
}

/// Claims and the joint rules for one step of a write by `w` (see [`check_changes`]).
fn claims_step(claims: &Claims, world: &World, change: &Change, w: Writer, now: Instant, joints: bool) -> Result<(), WorldError> {
    claims.check(world, std::slice::from_ref(change), w, now)?;
    if joints {
        joint_rules(claims, world, change, w, now)?;
    }
    Ok(())
}

/// The decision for what an agent caused through the game, a module or live Lex
/// ([`game_commit`]): the changes recorded since `mark`, made for `w`. Reserved data first (the
/// whole set), then claims and the joint rules step by step (each change against the world as it
/// was right after it: a later step can't hide what an earlier one did). Claims only run when one
/// could refuse `w` (a world without other agents' claims examines nothing more). On refusal
/// everything since `mark` is rolled back.
fn check_game(world: &mut World, claims: &Claims, mark: Mark, w: Writer) -> Result<(), WorldError> {
    let start = world.mark_changes(mark);
    if let Err(e) = reserved(&world.pending_changes()[start..], Reserved::GAME) {
        world.rollback_to(mark);
        return Err(e);
    }
    let now = Instant::now();
    if !claims.could_block(w, now) {
        return Ok(());
    }
    world.recheck_steps(mark, &mut |world: &World, change: &Change| claims_step(claims, world, change, w, now, true))
}

/// An agent's tool writing the world. The decision, in order: `command` runs; its changes pass
/// the reserved-data rules for `rule`; the joint rules on the result (every body a change ties,
/// lets go of or pulls, unless `rule` is [`Rule::Joint`], which checks its bodies itself);
/// transient entities are refused (they belong to the hosted game); then claims, step by step.
/// Journaled under `w` (undoable). A failing command or check changes nothing. Returns the
/// command's result and the journal id (`None` when nothing changed). With no claim that could
/// refuse `w`, only the reserved-data and transient rules run (O(changes)).
pub fn commit<T>(
    world: &mut World,
    swarm: &mut Swarm,
    tick: u64,
    w: Writer,
    kind: Kind,
    rule: Rule,
    command: impl FnOnce(&mut World) -> Result<T, WorldError>,
) -> Result<(T, Option<u64>), WorldError> {
    let now = Instant::now();
    let claims = &swarm.claims;
    let blocks = claims.could_block(w, now);
    let command = |wd: &mut World| {
        let mark = wd.mark();
        let v = command(wd)?;
        let start = wd.mark_changes(mark);
        let changes = &wd.pending_changes()[start..];
        reserved(changes, Reserved::agent(rule))?;
        // Removing a joint, or a body a joint links, lets go of the joint's other bodies: allowed
        // with the right to change them (physics_joint checks its own writes).
        if rule != Rule::Joint && blocks {
            for c in changes {
                joint_rules(claims, wd, c, w, now)?;
            }
        }
        Ok(v)
    };
    let mut check = |world: &World, change: &Change| claims.check(world, std::slice::from_ref(change), w, now);
    let check: Option<&mut journal::Check> = if blocks { Some(&mut check) } else { None };
    // `journal::commit` refuses transient writes, runs `check` step by step, rolls a failed
    // command back and journals the rest.
    let result = journal::commit(world, &mut swarm.journal, tick, w, kind, check, command);
    prune_retired(world, swarm);
    match &result {
        Err(e) => swarm.count_rejection(w, e),
        Ok((_, Some(seq))) => {
            let changes = swarm.journal.get(*seq).map_or(0, |e| e.changes.len());
            swarm.board.note_entry(w, *seq, changes);
        }
        Ok(_) => {}
    }
    result
}

/// Project files read from disk (edits made outside the engine, mounts), journaled as
/// [`crate::project::DISK_AGENT`]'s. **Exempt from claims:** a human's edit can't be refused
/// (F5), so it is recorded instead, and agents see it in history and the feed.
pub fn disk<T>(world: &mut World, swarm: &mut Swarm, tick: u64, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
    journal::commit(world, &mut swarm.journal, tick, crate::project::DISK_AGENT, Kind::File, None, command)
}

/// `w` undoes its entries (or forced, others'): checked against claims and the joint rules step
/// by step like any write (undoing a joint's removal ties its bodies again, restoring a jointed
/// body pulls the others), journaled as `w`'s undo.
pub fn undo(world: &mut World, swarm: &mut Swarm, tick: u64, w: Writer, req: &UndoRequest) -> Result<Undone, WorldError> {
    let now = Instant::now();
    let claims = &swarm.claims;
    let mut check = |world: &World, change: &Change| claims_step(claims, world, change, w, now, true);
    let check: Option<&mut journal::Check> = if claims.could_block(w, now) { Some(&mut check) } else { None };
    let result = swarm.journal.undo(world, tick, w, req, check);
    prune_retired(world, swarm);
    match &result {
        Err(e) => swarm.count_rejection(w, e),
        Ok(u) => swarm.board.note_entry(w, u.seq, u.changes),
    }
    result
}

/// The changes since `mark` that `game` (the hosted game, `module:<name>`, `lex:<set>`) made
/// because agent `w` asked (a game tool, a module load, unload or hook, a live Lex command):
/// reserved data (joints allowed: their bodies are checked), then claims and the joint rules as
/// `w`'s, step by step; journaled under `game` via `w`. On refusal the world is back at `mark`
/// (ids it used are never handed out again; the journal notes them) and the caller rolls its own
/// state back too.
pub fn game_commit(world: &mut World, swarm: &mut Swarm, mark: Mark, tick: u64, game: &str, w: Writer) -> Result<(), WorldError> {
    if let Err(e) = check_game(world, &swarm.claims, mark, w) {
        swarm.journal.note_next_id(world.next_id());
        swarm.count_rejection(w, &e);
        return Err(e.with("game", game));
    }
    let changes = world.take_changes();
    if changes.is_empty() {
        // Nothing to journal, but ids may have been used (and rolled back): replays must hand
        // out the same ones next.
        swarm.journal.note_next_id(world.next_id());
    } else {
        let count = changes.len();
        if let Some(seq) = swarm.journal.append_game(tick, game, Some(w.name), Kind::GameTool, changes, world.next_id()) {
            swarm.board.note_entry(w, seq, count);
        }
    }
    prune_retired(world, swarm);
    Ok(())
}

/// A game tool agent `w` called: `call` runs inside the game, and its world changes pass
/// [`game_commit`]. On refusal only the **world** is rolled back: the caller must roll the game
/// back too ([`crate::game::Game::restore`] from a checkpoint taken before the call, or
/// [`crate::game::Game::world_rolled_back`] for games without checkpoints).
pub fn game_tool<T>(world: &mut World, swarm: &mut Swarm, tick: u64, game: &str, w: Writer, call: impl FnOnce(&mut GameWorld) -> T) -> Result<T, WorldError> {
    let mark = world.mark();
    let out = call(&mut GameWorld::new(world));
    game_commit(world, swarm, mark, tick, game, w).map(|()| out)
}

/// A module's own request (spawn, set, delete, move_character) inside a call: the reserved-data
/// rules on the changes since `mark`; on refusal they are rolled back (the ids they used are
/// never handed out again) and the module gets the error. The call as a whole is committed by
/// the module host's commit ([`module_tick_commit`], [`module_agent_commit`]).
pub fn module_request(world: &mut World, mark: Mark) -> Result<(), WorldError> {
    let start = world.mark_changes(mark);
    if let Err(e) = reserved(&world.pending_changes()[start..], Reserved::GAME) {
        world.rollback_to(mark);
        return Err(e);
    }
    Ok(())
}

/// Journals what an [`Exempt`] writer changed (pending since its last journal): replay-only,
/// under its name. A game's per-tick entries are coalesced to one change per entity by the
/// [`Recording::Snapshot`] they were recorded with.
pub fn journal_exempt(world: &mut World, swarm: &mut Swarm, tick: u64, why: Exempt) {
    let changes = world.take_changes();
    swarm.journal.append_snapshot(tick, why.by(), None, why.kind(), changes, world.next_id());
    prune_retired(world, swarm);
}

/// After a writer rolled the world back itself (to a mark it took): ids the rolled-back changes
/// used are never handed out again, and the journal notes it so replays agree.
pub fn rolled_back(world: &mut World, swarm: &mut Swarm, mark: Mark) {
    world.rollback_to(mark);
    swarm.journal.note_next_id(world.next_id());
}

/// What a module call commits during a tick: journaled under the module, exempt from claims
/// ([`Exempt::Module`]).
pub fn module_tick_commit<'a>(swarm: &'a mut Swarm, tick: u64) -> impl FnMut(&mut World, Mark, &str, Phase) -> Result<(), WorldError> + 'a {
    move |world: &mut World, _: Mark, agent: &str, phase: Phase| {
        journal_exempt(world, swarm, tick, Exempt::Module { agent, kind: crate::logic::tools::kind(phase) });
        Ok(())
    }
}

/// What an agent's module load, unload or hook commits: the module's changes checked as `w`'s
/// and journaled under the module's name via `w` ([`game_commit`]).
pub fn module_agent_commit<'a>(swarm: &'a mut Swarm, tick: u64, w: Writer<'a>) -> impl FnMut(&mut World, Mark, &str, Phase) -> Result<(), WorldError> + 'a {
    move |world: &mut World, mark: Mark, agent: &str, _: Phase| game_commit(world, swarm, mark, tick, agent, w)
}

/// Module events (loads, reloads, unloads, disables) are journaled as records of code
/// ([`Kind::Module`]), with no world changes.
pub fn module_event(world: &World, swarm: &mut Swarm, by: Writer, via: Option<&str>, event: journal::ModuleEvent) -> u64 {
    swarm.journal.append_module(by, via, event, world.next_id())
}

/// Keeps the world's record of removed entities' versions (what restores go past, see
/// [`World::version_floor`]) to what the journal can still restore: once it has grown past twice
/// what was kept last time (and [`crate::swarm::RETIRED_PRUNE_MIN`]), ids no entry in the ring
/// touches go. Amortized: each prune costs about the ring's ids, and runs only after as many
/// deletes. Runs after every journaled write.
pub fn prune_retired(world: &mut World, swarm: &mut Swarm) {
    if world.retired_len() > swarm.retired_prune_at.max(crate::swarm::RETIRED_PRUNE_MIN) {
        prune_retired_now(world, swarm);
    }
}

/// [`prune_retired`] at once.
pub fn prune_retired_now(world: &mut World, swarm: &mut Swarm) {
    let keep = swarm.journal.ids_in_ring();
    world.retain_retired(|id| keep.contains(&id));
    swarm.retired_prune_at = 2 * world.retired_len();
}

/// Proof that the live world may be replaced now ([`may_replace`]): [`Live::replace`] takes one,
/// so no replacement (world_load, save_read, `--world` at startup) skips the check.
pub(crate) struct MayReplace(());

/// Who may replace the whole world (world_load, save_read): nobody while a playtest runs (it owns
/// the clock and the world until it ends; not even with force: its replay would record a run
/// nobody can reproduce), and `w` only while no other agent holds a live claim (a load replaces
/// everything they claimed). Checked when the request comes in, and again when a save game
/// swaps in.
pub(crate) fn may_replace(swarm: &Swarm, w: Writer, playtest_running: bool) -> Result<MayReplace, WorldError> {
    if playtest_running {
        return Err(WorldError::new("playtest_running", "a playtest is running (it owns the clock and the world); load after it ends (playtest_report waits for it) or cancel it")
            .with("retry_after_ms", 1000));
    }
    swarm.check_replace_world(w)?;
    Ok(MayReplace(()))
}

/// The engine's live world: read it like a [`World`]; write it only through the entry points
/// below (see the module docs).
pub struct Live {
    world: World,
    /// Changes found pending after an entry point (each a bug), journaled as `engine:stray`.
    strays: u64,
    /// The model library and shape budgets the live world uses: put back on a world that took
    /// its place ([`Live::recover`]).
    models: Option<Arc<dyn ModelSource>>,
    limits: crate::two_d::ShapeLimits,
    /// Times a writer replaced the live world wholesale (each a bug), and the last one.
    replaced: u64,
    last_replaced: Option<(String, u64)>,
    /// A replacement the engine hasn't reset its own caches for yet ([`Live::take_replaced`]).
    unseen_replacement: bool,
}

impl Deref for Live {
    type Target = World;
    fn deref(&self) -> &World {
        &self.world
    }
}

/// Runs `f` with `world` sealed ([`World::set_sealed`]): code the policy hands the live world to
/// (the hosted game, exempt writers) can't change how it records or take its recorded changes,
/// so nothing it writes escapes the checks and the journal. The policy's own journaling runs
/// after, unsealed.
fn sealed<T>(world: &mut World, f: impl FnOnce(&mut World) -> T) -> T {
    world.set_sealed(true);
    let out = f(world);
    world.set_sealed(false);
    out
}

/// Attribution of changes a writer left unjournaled (see [`Live::settle`]).
pub const STRAY_AGENT: &str = "engine:stray";

impl Live {
    /// The live world, starting as `world` (recording every change).
    pub(crate) fn new(mut world: World) -> Live {
        world.set_recording(Recording::Full);
        world.mark_live();
        let (models, limits) = (world.model_source().cloned(), world.shape_limits());
        Live { world, strays: 0, models, limits, replaced: 0, last_replaced: None, unseen_replacement: false }
    }

    /// Times a writer replaced the live world wholesale (bugs; 0 in a healthy run), and the
    /// entry point and tick of the last time, for engine_info.
    pub(crate) fn replacements(&self) -> (u64, Option<&(String, u64)>) {
        (self.replaced, self.last_replaced.as_ref())
    }

    /// Whether a writer replaced the live world since the last call: the engine then resets
    /// what it keyed on the old one (as after a load).
    pub(crate) fn take_replaced(&mut self) -> bool {
        std::mem::take(&mut self.unseen_replacement)
    }

    /// Runs `f`, which holds the live world, so that replacing the world wholesale (`*w =
    /// World::default()`, `mem::swap`, `mem::take`) can't go unnoticed: the project files are
    /// lent out meanwhile (`f`'s writers don't use them), and if the world `f` leaves isn't the
    /// one marked live, [`Live::recover`] takes over.
    fn guarded<T>(&mut self, swarm: &mut Swarm, tick: u64, entry: &str, f: impl FnOnce(&mut World, &mut Swarm) -> T) -> T {
        let (next_id, revision, clock, files_revision) = (self.world.next_id(), self.world.revision(), self.world.clock_parts(), self.world.files_revision());
        let files = self.world.lend_files();
        let out = f(&mut self.world, swarm);
        if self.world.is_marked_live() {
            self.world.return_files(files);
        } else {
            self.recover(swarm, tick, entry, next_id, revision, clock, (files, files_revision));
        }
        out
    }

    /// A writer replaced the live world wholesale: what it did can't be undone (the old world is
    /// gone), so the world it left becomes the live one, consistently. It is marked live and
    /// unsealed, records again, gets back the clock, the model library, the shape budgets and the
    /// project files, never repeats an id or a revision, and the journal starts over from it (so
    /// replay stays exact). Reported as an error (engine_info `swarm.world_replacements`, a log
    /// line, a debug assertion) and counted with the strays.
    #[allow(clippy::too_many_arguments)]
    fn recover(&mut self, swarm: &mut Swarm, tick: u64, entry: &str, next_id: u64, revision: u64, clock: crate::world::ClockParts, files: (crate::world::ProjectFiles, u64)) {
        self.strays += 1;
        self.replaced += 1;
        self.last_replaced = Some((entry.to_owned(), tick));
        self.unseen_replacement = true;
        let w = &mut self.world;
        w.set_sealed(false);
        w.mark_live();
        w.set_recording(Recording::Full);
        w.set_clock_parts(clock);
        let _ = w.set_model_source(self.models.clone());
        w.set_shape_limits(self.limits);
        w.adopt_files(files.0, files.1);
        w.set_next_id(next_id.max(w.next_id()));
        w.revision_past(revision);
        // What it recorded since belongs to a world that is gone: the journal starts here.
        w.set_sealed(false);
        let _ = w.take_changes();
        swarm.journal.rebase(w, &format!("engine:replaced_by:{entry}"));
        swarm.journal.note_next_id(w.next_id());
        eprintln!(
            "specula: write_policy: {{\"error\":\"world_replaced\",\"entry\":\"{entry}\",\"tick\":{tick},\"message\":\"a writer replaced the live world wholesale; it is live as left (clock, models, budgets and project files kept) and the journal starts over from it\"}}"
        );
        debug_assert!(false, "write_policy: {entry} replaced the live world wholesale");
    }

    /// Changes a writer left unjournaled (bugs; 0 in a healthy run).
    pub(crate) fn strays(&self) -> u64 {
        self.strays
    }

    /// Every entry point ends here: the live world records full changes again, and nothing is
    /// pending (every writer journals or rolls back what it changed). A writer that left changes
    /// is a bug: a debug assertion, and in release they are journaled as [`STRAY_AGENT`]'s so
    /// replay stays exact.
    fn settle(&mut self, swarm: &mut Swarm, tick: u64, entry: &str) {
        if self.world.recording() != Recording::Full {
            self.world.set_recording(Recording::Full);
        }
        if self.world.has_unjournaled() {
            self.strays += 1;
            eprintln!("specula: write_policy: {entry} left world changes unjournaled (journaled as {STRAY_AGENT})");
            let changes = self.world.take_changes();
            swarm.journal.append_snapshot(tick, STRAY_AGENT, None, Kind::GameFrame, changes, self.world.next_id());
            debug_assert!(false, "write_policy: {entry} left world changes unjournaled");
        }
    }

    /// [`commit`] on the live world: an agent's tool, under `rule`.
    pub(crate) fn commit<T>(
        &mut self,
        swarm: &mut Swarm,
        tick: u64,
        w: Writer,
        kind: Kind,
        rule: Rule,
        command: impl FnOnce(&mut World) -> Result<T, WorldError>,
    ) -> Result<(T, Option<u64>), WorldError> {
        let out = commit(&mut self.world, swarm, tick, w, kind, rule, command);
        self.settle(swarm, tick, "commit");
        out
    }

    /// [`disk`] on the live world.
    pub(crate) fn disk<T>(&mut self, swarm: &mut Swarm, tick: u64, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        let out = disk(&mut self.world, swarm, tick, command);
        self.settle(swarm, tick, "disk");
        out
    }

    /// [`undo`] on the live world.
    pub(crate) fn undo(&mut self, swarm: &mut Swarm, tick: u64, w: Writer, req: &UndoRequest) -> Result<Undone, WorldError> {
        let out = undo(&mut self.world, swarm, tick, w, req);
        self.settle(swarm, tick, "undo");
        out
    }

    /// [`game_tool`] on the live world. The world is [sealed](sealed) while the game runs.
    pub(crate) fn game_tool<T>(&mut self, swarm: &mut Swarm, tick: u64, game: &str, w: Writer, call: impl FnOnce(&mut GameWorld) -> T) -> Result<T, WorldError> {
        // The game gets a GameWorld: it can't replace the world or hide what it records.
        let out = game_tool(&mut self.world, swarm, tick, game, w, call);
        self.settle(swarm, tick, "game_tool");
        out
    }

    /// The writer tried to change how the world records, or to take its recorded changes, while
    /// it held the sealed world: refused (its writes stayed recorded, checked and journaled), and
    /// a bug in that writer: reported and counted with the strays.
    fn report_breaks(&mut self, before: u64, entry: &str) {
        if self.world.seal_breaks() != before {
            self.strays += 1;
            eprintln!("specula: write_policy: {entry}: the writer tried to change recording or take the world's recorded changes; refused (its writes are checked and journaled as usual)");
        }
    }

    /// An [`Exempt`] writer: `f` runs with the world recording as the writer does (and
    /// [sealed](sealed)), and what it changed is journaled under the writer's name
    /// ([`journal_exempt`]).
    pub(crate) fn exempt<T>(&mut self, swarm: &mut Swarm, tick: u64, why: Exempt, f: impl FnOnce(&mut World) -> T) -> T {
        self.world.set_recording(why.recording());
        let breaks = self.world.seal_breaks();
        let out = self.guarded(swarm, tick, "exempt", |w, _| sealed(w, f));
        self.world.set_recording(Recording::Full);
        journal_exempt(&mut self.world, swarm, tick, why);
        self.settle(swarm, tick, "exempt");
        self.report_breaks(breaks, "exempt");
        out
    }

    /// A writer that commits as it goes ([`Opener`]): `f` gets the world and the swarm, and must
    /// journal or roll back everything it changes through this module's functions before it
    /// returns ([`Live::settle`] checks).
    pub(crate) fn session<T>(&mut self, swarm: &mut Swarm, tick: u64, opener: Opener, f: impl FnOnce(&mut World, &mut Swarm) -> T) -> T {
        let entry = match opener {
            Opener::Modules => "session(modules)",
            Opener::LexLive => "session(lex_live)",
            Opener::LexTools => "session(lex_tools)",
        };
        let out = self.guarded(swarm, tick, entry, f);
        self.settle(swarm, tick, entry);
        out
    }

    /// Makes `world` the live one (world_load, save_read, `--world` at startup): it records
    /// changes, joins the engine clock at `clock_s` (world time resumes where it was saved),
    /// builds its models with `models`, keeps the current project files (they are the project's,
    /// not the world's), and the journal starts over from it. Who may replace the world (claims,
    /// a running playtest) was checked: `_may` proves it ([`may_replace`]). Returns the model
    /// entities whose recipe failed.
    pub(crate) fn replace(&mut self, _may: MayReplace, swarm: &mut Swarm, mut world: World, label: &str, clock_s: f64, models: Option<Arc<dyn ModelSource>>) -> Vec<(u64, WorldError)> {
        world.adopt_clock(clock_s);
        world.set_recording(Recording::Full);
        let failed = world.set_model_source(models);
        let files_revision = self.world.files_revision();
        world.adopt_files(self.world.take_files(), files_revision);
        swarm.journal.rebase(&world, label);
        world.mark_live();
        self.models = world.model_source().cloned();
        self.limits = world.shape_limits();
        self.world = world;
        failed
    }

    // Engine state that is not world state: never recorded, journaled or hashed.

    /// The engine clock as of its last tick (world time follows; see [`World::set_clock_s`]).
    pub(crate) fn set_clock_s(&mut self, seconds: f64) {
        self.world.set_clock_s(seconds);
    }

    /// Re-resolves every model entity after recipes changed (derived data only).
    pub(crate) fn refresh_models(&mut self) -> Vec<(u64, WorldError)> {
        self.world.refresh_models()
    }

    /// Re-resolves the model entities whose builds finished (derived data only).
    pub(crate) fn refresh_model_handles(&mut self, handles: &std::collections::BTreeSet<u32>) {
        self.world.refresh_model_handles(handles);
    }

    /// The vector-shape budgets (perf.toml), checked by later writes.
    pub(crate) fn set_shape_limits(&mut self, limits: crate::two_d::ShapeLimits) {
        self.limits = limits;
        self.world.set_shape_limits(limits);
    }
}

/// Swarm's write methods: the policy for a world the caller holds itself (tests and detached
/// worlds; the engine's live world is a [`Live`] and goes through its entry points). Each is
/// this module's function of the same rules.
impl Swarm {
    /// [`commit`] with [`Rule::World`].
    pub fn commit<T>(&mut self, world: &mut World, tick: u64, w: Writer, kind: Kind, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        commit(world, self, tick, w, kind, Rule::World, command)
    }

    /// [`commit`] with [`Rule::Joint`].
    pub fn commit_joint<T>(&mut self, world: &mut World, tick: u64, w: Writer, kind: Kind, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        commit(world, self, tick, w, kind, Rule::Joint, command)
    }

    /// [`commit`] with [`Rule::Nav`].
    pub fn commit_nav<T>(&mut self, world: &mut World, tick: u64, w: Writer, kind: Kind, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        commit(world, self, tick, w, kind, Rule::Nav, command)
    }

    /// [`commit`] with [`Rule::Lex`].
    pub fn commit_lex<T>(&mut self, world: &mut World, tick: u64, w: Writer, kind: Kind, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        commit(world, self, tick, w, kind, Rule::Lex, command)
    }

    /// [`commit`] with [`Rule::Project`] (always [`Kind::File`]).
    pub fn commit_project<T>(&mut self, world: &mut World, tick: u64, w: Writer, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        commit(world, self, tick, w, Kind::File, Rule::Project, command)
    }

    /// [`commit`] with [`Rule::LexLive`].
    pub fn commit_lex_live<T>(&mut self, world: &mut World, tick: u64, w: Writer, kind: Kind, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        commit(world, self, tick, w, kind, Rule::LexLive, command)
    }

    /// [`disk`].
    pub fn commit_disk<T>(&mut self, world: &mut World, tick: u64, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        disk(world, self, tick, command)
    }

    /// [`undo`].
    pub fn undo(&mut self, world: &mut World, tick: u64, w: Writer, req: &UndoRequest) -> Result<Undone, WorldError> {
        undo(world, self, tick, w, req)
    }

    /// [`game_tool`].
    pub fn game_tool<T>(&mut self, world: &mut World, tick: u64, game: &str, w: Writer, call: impl FnOnce(&mut GameWorld) -> T) -> Result<T, WorldError> {
        game_tool(world, self, tick, game, w, call)
    }

    /// [`game_commit`].
    pub fn check_game_changes(&mut self, world: &mut World, mark: Mark, tick: u64, game: &str, w: Writer) -> Result<(), WorldError> {
        game_commit(world, self, mark, tick, game, w)
    }

    /// [`journal_exempt`] for the hosted game `game`'s own changes, journaled as `kind`.
    pub fn game_changes(&mut self, world: &mut World, tick: u64, game: &str, kind: Kind) {
        journal_exempt(world, self, tick, Exempt::Game { name: game, kind });
    }

    /// [`prune_retired_now`].
    pub fn prune_retired_now(&mut self, world: &mut World) {
        prune_retired_now(world, self);
    }
}

#[cfg(test)]
mod tests;

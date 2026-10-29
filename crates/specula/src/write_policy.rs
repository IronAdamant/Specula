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
//! - **No writer holds a `World`.** Every writer's code gets a handle, `&mut dyn` [`WorldWrite`]:
//!   recorded writes only, no way to hide one, move the engine's settings or replace the world
//!   (compile errors, pinned by `compile_fail` doctests). The hosted game holds a [`GameWorld`]
//!   over the same handle. Only this module writes the live journal (`Swarm::journal_mut` takes
//!   its [`PolicyKey`]).
//! - **A backstop, in case that ever opens.** The live world is sealed except while the policy
//!   journals ([`World::set_sealed`]), counts changes made unrecorded in counters nothing clears
//!   ([`World::moves`]), and carries a mark a world put in its place lacks; the engine's settings
//!   are compared around each writer. Every entry point fails closed ([`Live::settle`]):
//!   unjournaled changes are rolled back, hidden ones or a replaced world start the journal over
//!   (so replay stays exact; the project files are kept); each is reported, counted and refused
//!   where the caller can be told.
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

/// Drops what a world of your own recorded: a "journal" that keeps nothing, for tests and tools
/// that drive the module host or live Lex on a world that isn't the engine's. Refused
/// (`live_world`, nothing dropped) on the live world, whose changes are the journal's.
pub fn drop_changes(world: &mut dyn WorldWrite) -> Result<(), WorldError> {
    let world = world.policy_world(&KEY);
    if world.is_marked_live() {
        world.count_seal_break();
        return Err(WorldError::new("live_world", "the live world's changes belong to the journal: only a world of your own can drop them"));
    }
    world.take_changes();
    Ok(())
}

/// Proof of being the write policy: only this module can make one. The journal's writes
/// ([`Swarm::journal_mut`]) and the world behind a writer's handle
/// ([`WorldWrite::policy_world`]) take it, so no other code can reach either (review round 4:
/// the journal aliased, and writers holding `&mut World` kept finding new spellings).
pub struct PolicyKey(());

const KEY: PolicyKey = PolicyKey(());

/// **What every writer gets: a handle on the world that can only make recorded writes.** It reads
/// like a [`World`] (`Deref`) and offers the writes each subsystem needs, each one recorded (so
/// it reaches the checks and the journal): no way to switch recording, take the recorded changes,
/// move the id counter, replace the world, or touch the engine's own settings (shape budgets, the
/// model library, the clock). Agents' commands, undo, disk imports, game tools, exempt writers
/// and sessions all receive `&mut dyn WorldWrite`, never `&mut World`, so none of that compiles
/// in a writer (`compile_fail` doctests below). A detached `World` is one too (tests, offline
/// tools); the live world is reachable only as this.
///
/// ```compile_fail
/// fn hide(w: &mut dyn specula::write_policy::WorldWrite) {
///     w.take_changes(); // not a recorded write: not on the handle
/// }
/// ```
///
/// ```compile_fail
/// fn off(w: &mut dyn specula::write_policy::WorldWrite) {
///     w.set_recording(specula::world::Recording::Off);
/// }
/// ```
///
/// ```compile_fail
/// fn replace(w: &mut dyn specula::write_policy::WorldWrite) {
///     *w = specula::world::World::default(); // the world itself is out of reach
/// }
/// ```
///
/// ```compile_fail
/// fn swap(w: &mut dyn specula::write_policy::WorldWrite, other: &mut specula::world::World) {
///     std::mem::swap(w, other);
/// }
/// ```
///
/// ```compile_fail
/// fn settings(w: &mut dyn specula::write_policy::WorldWrite) {
///     w.set_shape_limits(Default::default());
///     w.adopt_clock(1.0);
/// }
/// ```
///
/// ```compile_fail
/// fn models(w: &mut dyn specula::write_policy::WorldWrite) {
///     w.set_model_source(None);
/// }
/// ```
///
/// ```compile_fail
/// fn world(w: &mut dyn specula::write_policy::WorldWrite) -> &mut specula::world::World {
///     w.policy_world(&specula::write_policy::PolicyKey(())) // the key is the policy's own
/// }
/// ```
pub trait WorldWrite {
    /// The world, to read.
    fn as_world(&self) -> &World;
    /// [`World::spawn`].
    fn spawn(&mut self, spec: crate::world::SpawnSpec, by: &str) -> Result<&crate::world::Entity, WorldError>;
    /// [`World::update`].
    fn update(&mut self, key: &crate::world::EntityKey, change: crate::world::UpdateSpec, by: &str) -> Result<&crate::world::Entity, WorldError>;
    /// [`World::delete`].
    fn delete(&mut self, key: &crate::world::EntityKey, expect_version: Option<u64>) -> Result<crate::world::Deleted, WorldError>;
    /// [`World::apply_batch`].
    fn apply_batch(&mut self, ops: Vec<crate::world::Op>, by: &str) -> Result<Vec<crate::world::OpResult>, (usize, WorldError)>;
    /// [`World::set_env`].
    fn set_env(&mut self, patch: &crate::world::EnvPatch) -> Result<(), WorldError>;
    /// [`World::set_motion`]: physics' pose fast path (recorded as a pose change).
    fn set_motion(&mut self, id: u64, position: Option<specula_core::math::Vec3>, rotation_deg: Option<specula_core::math::Vec3>, body: crate::world::Body, by: &str) -> Result<&crate::world::Entity, WorldError>;
    /// [`World::set_external_pose`]: a ragdoll's pose for the animation (derived each physics
    /// tick; not world state, never journaled).
    fn set_external_pose(&mut self, id: u64, pose: Option<crate::world::ExternalPose>);
    /// [`World::set_camera`].
    fn set_camera(&mut self, owner: &str, spec: crate::cameras::CameraSpec, persisted: bool) -> Result<&crate::cameras::NamedCamera, WorldError>;
    /// [`World::delete_cameras`].
    fn delete_cameras(&mut self, owner: &str, name: Option<&str>) -> Result<usize, WorldError>;
    /// A project file written (or removed, `None`), recorded as a file change: only the project
    /// tools' commits and disk imports may make one (the reserved-data rules refuse it anywhere
    /// else).
    fn put_file(&mut self, path: &str, file: Option<crate::project::ProjectFile>, import: bool);
    /// Rolls back what was recorded since `mark` (a writer taking back its own failed step; the
    /// module host's call transactions).
    fn rollback_to(&mut self, mark: Mark);
    /// The world itself, for the write policy only.
    #[doc(hidden)]
    fn policy_world(&mut self, key: &PolicyKey) -> &mut World;
}

/// A handle reads like the world it writes (no `DerefMut`: the world itself stays out of reach).
impl<'a> Deref for dyn WorldWrite + 'a {
    type Target = World;
    fn deref(&self) -> &World {
        self.as_world()
    }
}

impl WorldWrite for World {
    fn as_world(&self) -> &World {
        self
    }
    fn spawn(&mut self, spec: crate::world::SpawnSpec, by: &str) -> Result<&crate::world::Entity, WorldError> {
        World::spawn(self, spec, by)
    }
    fn update(&mut self, key: &crate::world::EntityKey, change: crate::world::UpdateSpec, by: &str) -> Result<&crate::world::Entity, WorldError> {
        World::update(self, key, change, by)
    }
    fn delete(&mut self, key: &crate::world::EntityKey, expect_version: Option<u64>) -> Result<crate::world::Deleted, WorldError> {
        World::delete(self, key, expect_version)
    }
    fn apply_batch(&mut self, ops: Vec<crate::world::Op>, by: &str) -> Result<Vec<crate::world::OpResult>, (usize, WorldError)> {
        World::apply_batch(self, ops, by)
    }
    fn set_env(&mut self, patch: &crate::world::EnvPatch) -> Result<(), WorldError> {
        World::set_env(self, patch)
    }
    fn set_motion(&mut self, id: u64, position: Option<specula_core::math::Vec3>, rotation_deg: Option<specula_core::math::Vec3>, body: crate::world::Body, by: &str) -> Result<&crate::world::Entity, WorldError> {
        World::set_motion(self, id, position, rotation_deg, body, by)
    }
    fn set_external_pose(&mut self, id: u64, pose: Option<crate::world::ExternalPose>) {
        World::set_external_pose(self, id, pose)
    }
    fn set_camera(&mut self, owner: &str, spec: crate::cameras::CameraSpec, persisted: bool) -> Result<&crate::cameras::NamedCamera, WorldError> {
        World::set_camera(self, owner, spec, persisted)
    }
    fn delete_cameras(&mut self, owner: &str, name: Option<&str>) -> Result<usize, WorldError> {
        World::delete_cameras(self, owner, name)
    }
    fn put_file(&mut self, path: &str, file: Option<crate::project::ProjectFile>, import: bool) {
        World::put_file_raw(self, path, file, import)
    }
    fn rollback_to(&mut self, mark: Mark) {
        World::rollback_to(self, mark)
    }
    fn policy_world(&mut self, _: &PolicyKey) -> &mut World {
        self
    }
}

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
    /// vfx_set: the effect library (`/vfx` and its children, the `vfx_effect` tag).
    Vfx,
    /// gi_bake: baked GI volumes (`/gi` and its children, `props.gi_volume` anywhere).
    Gi,
}

impl Rule {
    /// Its name in trace events and incidents (docs/design/TRACE.md).
    pub fn name(self) -> &'static str {
        match self {
            Rule::World => "world",
            Rule::Joint => "joint",
            Rule::Nav => "nav",
            Rule::Lex => "lex",
            Rule::LexLive => "lex_live",
            Rule::Project => "project",
            Rule::Vfx => "vfx",
            Rule::Gi => "gi",
        }
    }
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
/// entity there), `/nav` and navmesh settings only through the nav tools, the effect library
/// (`/vfx`, the `vfx_effect` tag) only through vfx_set, and (for agents) joint data only through
/// physics_joint.
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
    // The effect library: /vfx's children tagged vfx_effect (a claim on /vfx covers every world
    // effect), written only through vfx_set; the tag marks an effect, so it goes nowhere else.
    if !owns(Rule::Vfx) {
        if let Some(c) = changes.iter().find(|c| crate::vfx::touches_library(c)) {
            return Err(WorldError::new(
                "reserved",
                format!("'{}' is the effect library (/vfx and its children, the vfx_effect tag): change effects with vfx_set (vfx_get reads them; world_spawn {{vfx: {{effect}}}} plays one)", c.path()),
            )
            .with("path", c.path().to_owned()));
        }
    }
    // Baked GI volumes: /gi and props.gi_volume, written only through gi_bake (a volume's
    // settings and the hash of its bake; the probes are derived data in the store).
    if !owns(Rule::Gi) {
        if let Some(c) = changes.iter().find(|c| crate::gi::touches_gi_data(c)) {
            return Err(WorldError::new(
                "reserved",
                format!("'{}' is baked GI data (/gi, props.gi_volume): change volumes with gi_bake (gi_info reads them)", c.path()),
            )
            .with("path", c.path().to_owned()));
        }
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
/// ([`game_commit`]): the changes recorded since `mark`, made for `w`. Reserved data and the
/// effect rules first (the whole set), then claims and the joint rules step by step (each change against the world as it
/// was right after it: a later step can't hide what an earlier one did). Claims only run when one
/// could refuse `w` (a world without other agents' claims examines nothing more). On refusal
/// everything since `mark` is rolled back.
fn check_game(world: &mut World, claims: &Claims, mark: Mark, w: Writer) -> Result<(), WorldError> {
    let start = world.mark_changes(mark);
    let rules = reserved(&world.pending_changes()[start..], Reserved::GAME).and_then(|()| crate::vfx::write_rules(world, &world.pending_changes()[start..]));
    if let Err(e) = rules {
        world.rollback_to(mark);
        return Err(e);
    }
    let now = Instant::now();
    if !claims.could_block(w, now) {
        return Ok(());
    }
    world.recheck_steps(mark, &mut |world: &World, change: &Change| claims_step(claims, world, change, w, now, true))
}

/// Runs `f` (a writer's code) with `world` sealed ([`World::set_sealed`]): it can't switch how
/// the world records or take its recorded changes, so everything it writes reaches the checks
/// and the journal. The caller's seal is restored after.
fn sealed<T>(world: &mut World, f: impl FnOnce(&mut World) -> T) -> T {
    let prev = world.is_sealed();
    world.set_sealed(true);
    let out = f(world);
    world.set_sealed(prev);
    out
}

/// [`sealed`], for writers whose changes are checked (agents' commands, disk imports, game
/// tools): recording also stays full ([`World::set_full_only`]).
fn checked<T>(world: &mut World, f: impl FnOnce(&mut World) -> T) -> T {
    let prev = world.full_only();
    world.set_full_only(true);
    let out = sealed(world, f);
    world.set_full_only(prev);
    out
}

/// Runs `f` (the policy's own journaling: taking the recorded changes, switching recording) with
/// `world` unsealed, restoring the seal after.
fn journaling<T>(world: &mut World, f: impl FnOnce(&mut World) -> T) -> T {
    let prev = world.is_sealed();
    world.set_sealed(false);
    let out = f(world);
    world.set_sealed(prev);
    out
}

/// Switches how `world` records for `f` (live Lex's presentation records one change per entity),
/// back after: the only way to change recording while the live world is sealed. `f` itself runs
/// sealed; its writes are journaled by the caller.
pub(crate) fn recording_as<T>(world: &mut dyn WorldWrite, mode: Recording, f: impl FnOnce(&mut dyn WorldWrite) -> T) -> T {
    let world = world.policy_world(&KEY);
    if world.full_only() && mode != world.recording() {
        // Inside an agent's command, a disk import or a game tool: their checks need full
        // changes, so recording stays as it is (review round 4, P1b: Snapshot hid a write).
        world.count_seal_break();
        return f(world);
    }
    let (prev_mode, prev_seal) = (world.recording(), world.is_sealed());
    world.set_sealed(false);
    world.set_recording(mode);
    world.set_sealed(true);
    let out = f(&mut *world);
    world.set_sealed(false);
    world.set_recording(prev_mode);
    world.set_sealed(prev_seal);
    out
}

/// Runs `f` with `world` recording full changes (what the module host's call transactions roll
/// back exactly), then as before. Only ever to full, so it can't hide a write: inside an entry
/// point the live world already records full changes, except in exempt writers recording one
/// change per entity, where full changes are only more detail.
pub(crate) fn recording_full<T>(world: &mut dyn WorldWrite, f: impl FnOnce(&mut dyn WorldWrite) -> T) -> T {
    let w = world.policy_world(&KEY);
    if w.recording() == Recording::Full {
        return f(w);
    }
    let (prev_mode, prev_seal, prev_full) = (w.recording(), w.is_sealed(), w.full_only());
    w.set_sealed(false);
    w.set_full_only(false);
    w.set_recording(Recording::Full);
    w.set_sealed(prev_seal);
    let out = f(&mut *w);
    w.set_sealed(false);
    w.set_recording(prev_mode);
    w.set_sealed(prev_seal);
    w.set_full_only(prev_full);
    out
}

/// An agent's tool writing the world. The decision, in order: `command` runs (sealed); its
/// changes pass the reserved-data rules for `rule` and the effect rules
/// ([`crate::vfx::write_rules`]: effects named exist and nothing flashes); the joint rules on the result (every body a
/// change ties, lets go of or pulls, unless `rule` is [`Rule::Joint`], which checks its bodies
/// itself); transient entities are refused (they belong to the hosted game); then claims, step by
/// step. Journaled under `w` (undoable). A failing command or check changes nothing. Returns the
/// command's result and the journal id (`None` when nothing changed). With no claim that could
/// refuse `w`, only the reserved-data and transient rules run (O(changes)).
pub fn commit<T>(
    world: &mut dyn WorldWrite,
    swarm: &mut Swarm,
    tick: u64,
    w: Writer,
    kind: Kind,
    rule: Rule,
    command: impl FnOnce(&mut dyn WorldWrite) -> Result<T, WorldError>,
) -> Result<(T, Option<u64>), WorldError> {
    let world = world.policy_world(&KEY);
    let now = Instant::now();
    let (claims, journal) = swarm.claims_and_journal(&KEY);
    let blocks = claims.could_block(w, now);
    let command = |wd: &mut World| {
        let mark = wd.mark();
        let v = checked(wd, |w| command(w))?;
        let start = wd.mark_changes(mark);
        let changes = &wd.pending_changes()[start..];
        reserved(changes, Reserved::agent(rule))?;
        crate::vfx::write_rules(wd, changes)?;
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
    let result = journaling(world, |world| journal::commit(world, journal, tick, w, kind, check, command));
    prune_retired(world, swarm);
    match &result {
        Err(e) => {
            swarm.count_rejection(w, e);
            crate::trace_engine::write_refused(w, crate::trace_engine::rule_name(rule), e);
        }
        Ok((_, Some(seq))) => {
            let changes = swarm.journal().get(*seq).map_or(0, |e| e.changes.len());
            swarm.board.note_entry(w, *seq, changes);
            swarm.note_agent_commit(w);
            crate::trace_engine::write_committed(w, crate::trace_engine::rule_name(rule), Some(*seq), changes);
        }
        Ok(_) => {}
    }
    result
}

/// Project files read from disk (edits made outside the engine, mounts), journaled as
/// [`crate::project::DISK_AGENT`]'s. **Exempt from claims:** a human's edit can't be refused
/// (F5), so it is recorded instead, and agents see it in history and the feed.
pub fn disk<T>(world: &mut dyn WorldWrite, swarm: &mut Swarm, tick: u64, command: impl FnOnce(&mut dyn WorldWrite) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
    let world = world.policy_world(&KEY);
    journaling(world, |world| journal::commit(world, swarm.journal_mut(&KEY), tick, crate::project::DISK_AGENT, Kind::File, None, |wd| checked(wd, |w| command(w))))
}

/// `w` undoes its entries (or forced, others'): checked against claims and the joint rules step
/// by step like any write (undoing a joint's removal ties its bodies again, restoring a jointed
/// body pulls the others), journaled as `w`'s undo. The journal itself puts the old states back:
/// no writer's code runs.
pub fn undo(world: &mut dyn WorldWrite, swarm: &mut Swarm, tick: u64, w: Writer, req: &UndoRequest) -> Result<Undone, WorldError> {
    let world = world.policy_world(&KEY);
    let now = Instant::now();
    let (claims, journal) = swarm.claims_and_journal(&KEY);
    let mut check = |world: &World, change: &Change| claims_step(claims, world, change, w, now, true);
    let check: Option<&mut journal::Check> = if claims.could_block(w, now) { Some(&mut check) } else { None };
    let result = journaling(world, |world| journal.undo(world, tick, w, req, check));
    prune_retired(world, swarm);
    match &result {
        Err(e) => {
            swarm.count_rejection(w, e);
            crate::trace_engine::write_refused(w, "undo", e);
        }
        Ok(u) => {
            swarm.board.note_entry(w, u.seq, u.changes);
            swarm.note_agent_commit(w);
            crate::trace_engine::write_committed(w, "undo", Some(u.seq), u.changes);
        }
    }
    result
}

/// The changes since `mark` that `game` (the hosted game, `module:<name>`, `lex:<set>`) made
/// because agent `w` asked (a game tool, a module load, unload or hook, a live Lex command):
/// reserved data (joints allowed: their bodies are checked), then claims and the joint rules as
/// `w`'s, step by step; journaled under `game` via `w`. On refusal the world is back at `mark`
/// (ids it used are never handed out again; the journal notes them) and the caller rolls its own
/// state back too.
pub fn game_commit(world: &mut dyn WorldWrite, swarm: &mut Swarm, mark: Mark, tick: u64, game: &str, w: Writer) -> Result<(), WorldError> {
    let world = world.policy_world(&KEY);
    if let Err(e) = journaling(world, |world| check_game(world, &swarm.claims, mark, w)) {
        swarm.journal_mut(&KEY).note_next_id(world.next_id());
        swarm.count_rejection(w, &e);
        crate::trace_engine::write_refused(w, "game", &e);
        return Err(e.with("game", game));
    }
    let changes = journaling(world, World::take_changes);
    if changes.is_empty() {
        // Nothing to journal, but ids may have been used (and rolled back): replays must hand
        // out the same ones next.
        swarm.journal_mut(&KEY).note_next_id(world.next_id());
    } else {
        let count = changes.len();
        if let Some(seq) = swarm.journal_mut(&KEY).append_game(tick, game, Some(w.name), Kind::GameTool, changes, world.next_id()) {
            swarm.board.note_entry(w, seq, count);
            swarm.note_agent_commit(w);
        }
    }
    prune_retired(world, swarm);
    Ok(())
}

/// A game tool agent `w` called: `call` runs inside the game (with a [`GameWorld`], sealed), and
/// its world changes pass [`game_commit`]. On refusal only the **world** is rolled back: the
/// caller must roll the game back too ([`crate::game::Game::restore`] from a checkpoint taken
/// before the call, or [`crate::game::Game::world_rolled_back`] for games without checkpoints).
pub fn game_tool<T>(world: &mut dyn WorldWrite, swarm: &mut Swarm, tick: u64, game: &str, w: Writer, call: impl FnOnce(&mut GameWorld) -> T) -> Result<T, WorldError> {
    let world = world.policy_world(&KEY);
    let mark = world.mark();
    let out = checked(world, |wd| call(&mut GameWorld::new(wd)));
    game_commit(world, swarm, mark, tick, game, w).map(|()| out)
}

/// A module's own request (spawn, set, delete, move_character) inside a call: the reserved-data
/// and effect rules on the changes since `mark`; on refusal they are rolled back (the ids they used are
/// never handed out again) and the module gets the error. The call as a whole is committed by
/// the module host's commit ([`module_tick_commit`], [`module_agent_commit`]).
pub fn module_request(world: &mut dyn WorldWrite, mark: Mark) -> Result<(), WorldError> {
    let world = world.policy_world(&KEY);
    let start = world.mark_changes(mark);
    let rules = reserved(&world.pending_changes()[start..], Reserved::GAME).and_then(|()| crate::vfx::write_rules(world, &world.pending_changes()[start..]));
    if let Err(e) = rules {
        world.rollback_to(mark);
        return Err(e);
    }
    Ok(())
}

/// Journals what an [`Exempt`] writer changed (pending since its last journal): replay-only,
/// under its name. A game's per-tick entries are coalesced to one change per entity by the
/// [`Recording::Snapshot`] they were recorded with. Returns the reserved-data rule the writer's
/// changes broke, if any (exempt writers aren't refused: the caller reports it). Writes a module
/// or a live Lex run made to another agent's claimed entities are reported to the claim's
/// holder ([`Swarm::note_claim_crossings`]).
pub fn journal_exempt(world: &mut dyn WorldWrite, swarm: &mut Swarm, tick: u64, why: Exempt) -> Option<WorldError> {
    let world = world.policy_world(&KEY);
    let changes = journaling(world, World::take_changes);
    let broke = match why {
        Exempt::Game { .. } | Exempt::GameStart { .. } => reserved(&changes, Reserved::GAME).err(),
        _ => None,
    };
    if matches!(why, Exempt::Module { .. } | Exempt::LexLive { .. }) && !swarm.claims.is_empty() && !changes.is_empty() {
        swarm.note_claim_crossings(world, &changes, why.by(), tick);
    }
    swarm.journal_mut(&KEY).append_snapshot(tick, why.by(), None, why.kind(), changes, world.next_id());
    prune_retired(world, swarm);
    broke
}

/// After a writer rolled the world back itself (to a mark it took): ids the rolled-back changes
/// used are never handed out again, and the journal notes it so replays agree.
pub fn rolled_back(world: &mut dyn WorldWrite, swarm: &mut Swarm, mark: Mark) {
    let world = world.policy_world(&KEY);
    journaling(world, |world| world.rollback_to(mark));
    swarm.journal_mut(&KEY).note_next_id(world.next_id());
}

/// What a module call commits during a tick: journaled under the module, exempt from claims
/// ([`Exempt::Module`]).
pub fn module_tick_commit<'a>(swarm: &'a mut Swarm, tick: u64) -> impl FnMut(&mut dyn WorldWrite, Mark, &str, Phase) -> Result<(), WorldError> + 'a {
    move |world: &mut dyn WorldWrite, _: Mark, agent: &str, phase: Phase| {
        journal_exempt(world, swarm, tick, Exempt::Module { agent, kind: crate::logic::tools::kind(phase) });
        Ok(())
    }
}

/// What an agent's module load, unload or hook commits: the module's changes checked as `w`'s
/// and journaled under the module's name via `w` ([`game_commit`]).
pub fn module_agent_commit<'a>(swarm: &'a mut Swarm, tick: u64, w: Writer<'a>) -> impl FnMut(&mut dyn WorldWrite, Mark, &str, Phase) -> Result<(), WorldError> + 'a {
    move |world: &mut dyn WorldWrite, mark: Mark, agent: &str, _: Phase| game_commit(world, swarm, mark, tick, agent, w)
}

/// Module events (loads, reloads, unloads, disables) are journaled as records of code
/// ([`Kind::Module`]), with no world changes.
pub fn module_event(world: &World, swarm: &mut Swarm, by: Writer, via: Option<&str>, event: journal::ModuleEvent) -> u64 {
    swarm.journal_mut(&KEY).append_module(by, via, event, world.next_id())
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
    let keep = swarm.journal().ids_in_ring();
    world.retain_retired(|id| keep.contains(&id));
    swarm.retired_prune_at = 2 * world.retired_len();
}

/// What a live Lex run may present (its `present` rules' spawn specs, checked when the run
/// starts and when a save brings it back): the presentation is journaled as the run's own writes
/// ([`Exempt::LexLive`], with no reserved-data or joint checks), so nothing reserved may be in
/// it: no joint (`props.joint`, which would tie or pull bodies others claimed), no navmesh
/// settings, no Lex rule data (review round 3: a present rule tied a joint onto a claimed box).
pub(crate) fn presentation_allowed(spec: &crate::world::SpawnSpec) -> Result<(), String> {
    for key in [crate::physics::joints::PROP, crate::nav::MESH_KEY, "lex", "lex_set", "lex_live_run"] {
        if spec.props.get(key).is_some() {
            return Err(format!("props.{key} is reserved (joints are physics_joint's, navmesh settings nav_build's, Lex rule data the Lex tools'): a live run's presentation can't carry it"));
        }
    }
    Ok(())
}

/// Proof that the live world may be replaced now ([`may_replace`]): [`Live::replace`] takes one,
/// so no replacement (world_load, save_read, `--world` at startup) skips the check.
pub(crate) struct MayReplace(());

/// Who may replace the whole world (world_load, save_read): nobody while a playtest runs (it owns
/// the clock and the world until it ends; not even with force: its replay would record a run
/// nobody can reproduce), and `w` only while no other agent holds a live claim (a load replaces
/// everything they claimed). Checked when the request comes in, and again when a save game
/// swaps in. The playtests are read here, not told.
pub(crate) fn may_replace(swarm: &Swarm, w: Writer, playtests: &crate::playtest_engine::Playtests) -> Result<MayReplace, WorldError> {
    if playtests.running() {
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
    /// Changes found unjournaled after an entry point, made unrecorded, or engine settings a
    /// writer moved (each a bug).
    strays: u64,
    /// The model library and shape budgets the live world uses: put back when a writer moved or
    /// replaced them ([`Live::guarded`]).
    models: Option<Arc<dyn ModelSource>>,
    limits: crate::two_d::ShapeLimits,
    /// Times a writer replaced the live world wholesale (each a bug), and the last one.
    replaced: u64,
    last_replaced: Option<(String, u64)>,
    /// A replacement the engine hasn't reset its own caches for yet ([`Live::take_replaced`]).
    unseen_replacement: bool,
    /// Reserved data the hosted game's own calls wrote (reported, not refused).
    game_reserved: u64,
}

impl Deref for Live {
    type Target = World;
    fn deref(&self) -> &World {
        &self.world
    }
}

/// Whether two model libraries are the same one.
fn same_models(a: Option<&Arc<dyn ModelSource>>, b: Option<&Arc<dyn ModelSource>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => std::ptr::eq(Arc::as_ptr(x) as *const (), Arc::as_ptr(y) as *const ()),
        _ => false,
    }
}

/// Attribution of changes a writer left unjournaled (see [`Live::settle`]).
pub const STRAY_AGENT: &str = "engine:stray";

/// How a [`Live::guarded`] writer uses the project files.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Files {
    /// It doesn't: they are lent out meanwhile (so a world put in its place keeps them).
    Lent,
    /// It writes them (the project tools, disk imports): they stay, and a copy is kept to put
    /// back if the world is replaced (never an empty store reaching the disk sync).
    Kept,
}

/// Whether a writer may record one change per entity (exempt writers and sessions: Snapshot is
/// how they record) or must record full changes (agents' commits, disk imports, game tools:
/// their checks run on full changes, so a write recorded otherwise skipped them).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Records {
    Full,
    AsTheyDo,
}

/// What a guarded writer did that it shouldn't have (each a bug the policy recovered from).
#[derive(Default)]
struct Found {
    /// It replaced the world wholesale.
    replaced: bool,
    /// It changed the world unrecorded (or, where full changes are required, recorded one per
    /// entity).
    hidden: bool,
    /// It moved the engine's settings (the id counter back, the shape budgets, the model
    /// library, the clock): put back.
    settings: bool,
}

impl Live {
    /// The live world, starting as `world` (recording every change, sealed). Made once, at
    /// startup (`Engine::new`; the guard keeps it there): later worlds come in through
    /// [`Live::replace`].
    pub(crate) fn new(mut world: World) -> Live {
        world.set_recording(Recording::Full);
        world.mark_live();
        world.set_sealed(true);
        let (models, limits) = (world.model_source().cloned(), world.shape_limits());
        Live { world, strays: 0, models, limits, replaced: 0, last_replaced: None, unseen_replacement: false, game_reserved: 0 }
    }

    /// Times a writer replaced the live world wholesale (bugs; 0 in a healthy run), and the
    /// entry point and tick of the last time, for engine_info.
    pub(crate) fn replacements(&self) -> (u64, Option<&(String, u64)>) {
        (self.replaced, self.last_replaced.as_ref())
    }

    /// Reserved data the hosted game's own calls wrote (reported, never refused).
    pub(crate) fn game_reserved_writes(&self) -> u64 {
        self.game_reserved
    }

    /// Whether a writer replaced the live world since the last call: the engine then resets
    /// what it keyed on the old one (as after a load).
    pub(crate) fn take_replaced(&mut self) -> bool {
        std::mem::take(&mut self.unseen_replacement)
    }

    /// Runs `f` with a handle on the live world ([`WorldWrite`]: recorded writes only, so it
    /// can't hide a write or replace the world). As a backstop, whatever the spelling, `f` is
    /// also checked here: a world left in place of the marked one is recovered
    /// ([`Live::recover`]); a change made unrecorded (or, for [`Records::Full`] writers, one per
    /// entity) is a hidden write; the engine's settings it moved are put back.
    fn guarded<T>(&mut self, swarm: &mut Swarm, tick: u64, entry: &str, files: Files, records: Records, f: impl FnOnce(&mut dyn WorldWrite, &mut Swarm) -> T) -> (T, Found) {
        let (epochs, clock, files_revision, moves) = (self.world.epochs(), self.world.clock_parts(), self.world.files_revision(), self.world.moves());
        let keep = match files {
            Files::Lent => self.world.lend_files(),
            Files::Kept => self.world.files_snapshot(),
        };
        let out = f(&mut self.world, swarm);
        let mut found = Found::default();
        if !self.world.is_marked_live() {
            found.replaced = true;
            self.recover(swarm, tick, entry, epochs, clock, (keep, files_revision));
            return (out, found);
        }
        let (off, unfull) = self.world.moves();
        found.hidden = off != moves.0 || (records == Records::Full && unfull != moves.1);
        match files {
            Files::Lent => self.world.return_files(keep),
            // A hidden write may have emptied or changed the files (taken them): they go back as
            // they were, so the disk sync never takes a missing file for a deletion.
            Files::Kept if found.hidden => {
                let revision = self.world.files_revision().max(files_revision);
                self.world.adopt_files(keep, revision);
            }
            Files::Kept => {}
        }
        // The engine's settings: no writer's handle moves them, but put them back if one did.
        let w = &mut self.world;
        if w.next_id() < epochs.next_id() || w.shape_limits() != self.limits || !w.clock_is(&clock) || !same_models(w.model_source(), self.models.as_ref()) {
            found.settings = true;
            journaling(w, |w| {
                w.set_next_id(epochs.next_id().max(w.next_id()));
                w.set_shape_limits(self.limits);
                w.set_clock_parts(clock);
                if !same_models(w.model_source(), self.models.as_ref()) {
                    let _ = w.set_model_source(self.models.clone());
                }
            });
            eprintln!("specula: write_policy: {{\"error\":\"settings_moved\",\"entry\":\"{entry}\",\"tick\":{tick},\"message\":\"a writer moved the engine's settings (id counter, shape budgets, clock or model library); put back\"}}");
        }
        // Ids used without an entry (a refused command, or a counter moved on): replay ends past
        // them too.
        swarm.journal_mut(&KEY).note_next_id(self.world.next_id());
        (out, found)
    }

    /// A writer replaced the live world wholesale: what it did can't be undone (the old world is
    /// gone), so the world it left becomes the live one, consistently. It is marked live and
    /// sealed, records again, gets back the clock, the model library, the shape budgets and the
    /// project files (lent, or the copy kept for the project tools and disk imports: never an
    /// empty store, which the disk sync would take for deletions), never repeats an id, a
    /// revision, a subtree stamp, an epoch or a files revision, and the journal starts over from
    /// it (so replay stays exact). Reported as an error (engine_info `swarm.world_replacements`,
    /// a log line, a debug assertion) and counted with the strays.
    fn recover(&mut self, swarm: &mut Swarm, tick: u64, entry: &str, epochs: crate::world::Epochs, clock: crate::world::ClockParts, files: (crate::world::ProjectFiles, u64)) {
        self.strays += 1;
        self.replaced += 1;
        self.last_replaced = Some((entry.to_owned(), tick));
        self.unseen_replacement = true;
        let w = &mut self.world;
        journaling(w, |w| {
            w.mark_live();
            w.set_recording(Recording::Full);
            w.set_clock_parts(clock);
            let _ = w.set_model_source(self.models.clone());
            w.set_shape_limits(self.limits);
            w.adopt_files(files.0, files.1.max(w.files_revision()));
            w.advance_past(epochs);
            // What it recorded belongs to a world that is gone: the journal starts here.
            let _ = w.take_changes();
            swarm.journal_mut(&KEY).rebase(w, &format!("engine:replaced_by:{entry}"));
            swarm.journal_mut(&KEY).note_next_id(w.next_id());
        });
        w.set_sealed(true);
        eprintln!(
            "specula: write_policy: {{\"error\":\"world_replaced\",\"entry\":\"{entry}\",\"tick\":{tick},\"message\":\"a writer replaced the live world wholesale; it is live as left (clock, models, budgets and project files kept) and the journal starts over from it\"}}"
        );
        debug_assert!(false, "write_policy: {entry} replaced the live world wholesale");
    }

    /// Tests: a writer that holds the world itself (what no writer's handle allows), run as an
    /// entry point runs one: the backstop's cases (hidden writes, a replaced world, moved
    /// settings). With `agent`, as that agent's command in [`Live::commit`] (claims checked,
    /// full changes required); else as an exempt writer. Whether it was clean and allowed.
    #[cfg(test)]
    pub(crate) fn raw_writer_for_test(&mut self, swarm: &mut Swarm, tick: u64, agent: Option<Writer>, f: impl FnOnce(&mut World)) -> bool {
        match agent {
            Some(w) => self.raw_commit_for_test(swarm, tick, w, Rule::World, f),
            None => self.run(swarm, tick, "raw writer (test)", Files::Lent, Records::AsTheyDo, |w, _| sealed(w.policy_world(&KEY), f)).1,
        }
    }

    /// Tests: [`Live::raw_writer_for_test`] as an agent's command under `rule` (the project
    /// tools keep the files), or (`None`) as a disk import.
    #[cfg(test)]
    pub(crate) fn raw_commit_for_test(&mut self, swarm: &mut Swarm, tick: u64, w: Writer, rule: Rule, f: impl FnOnce(&mut World)) -> bool {
        let files = if rule == Rule::Project { Files::Kept } else { Files::Lent };
        let kind = if rule == Rule::Project { Kind::File } else { Kind::Set };
        let (out, clean) = self.run(swarm, tick, "raw writer (test)", files, Records::Full, |world, swarm| {
            commit(world, swarm, tick, w, kind, rule, |h| {
                f(h.policy_world(&KEY));
                Ok(())
            })
        });
        clean && out.is_ok()
    }

    /// Tests: [`Live::raw_writer_for_test`] as a disk import.
    #[cfg(test)]
    pub(crate) fn raw_disk_for_test(&mut self, swarm: &mut Swarm, tick: u64, f: impl FnOnce(&mut World)) -> bool {
        let (out, clean) = self.run(swarm, tick, "raw writer (test)", Files::Kept, Records::Full, |world, swarm| {
            disk(world, swarm, tick, |h| {
                f(h.policy_world(&KEY));
                Ok(())
            })
        });
        clean && out.is_ok()
    }

    /// Changes a writer left unjournaled or unrecorded, settings it moved, and worlds replaced
    /// (bugs; 0 in a healthy run).
    pub(crate) fn strays(&self) -> u64 {
        self.strays
    }

    /// Every entry point ends here, and fails closed. The live world records full changes again
    /// and is sealed. Then:
    /// - **Unjournaled** changes (still recorded, not taken): rolled back when they can be (full
    ///   changes carry their state before), else (a Snapshot writer's) journaled as
    ///   [`STRAY_AGENT`]'s so replay stays exact.
    /// - **Hidden** writes (`found`: made unrecorded, or where full changes were required, one
    ///   per entity; or a world replaced): they can't be undone, so the journal starts over from
    ///   the world as it is.
    ///
    /// Each is a bug: reported, counted, a debug assertion; the entry point refuses
    /// (`unjournaled_write`) when its caller can be told. Returns whether all was clean.
    fn settle(&mut self, swarm: &mut Swarm, tick: u64, entry: &str, found: &Found) -> bool {
        let w = &mut self.world;
        let pending = w.has_unjournaled();
        let hidden = found.hidden && !found.replaced;
        journaling(w, |w| {
            w.set_recording(Recording::Full);
            if pending {
                if !w.can_roll_back_pending() {
                    eprintln!("specula: write_policy: {{\"error\":\"unjournaled_write\",\"entry\":\"{entry}\",\"tick\":{tick},\"message\":\"a writer left changes unjournaled; they can't be rolled back (recorded one per entity), so they are journaled as {STRAY_AGENT}\"}}");
                    let changes = w.take_changes();
                    swarm.journal_mut(&KEY).append_snapshot(tick, STRAY_AGENT, None, Kind::GameFrame, changes, w.next_id());
                } else {
                    eprintln!("specula: write_policy: {{\"error\":\"unjournaled_write\",\"entry\":\"{entry}\",\"tick\":{tick},\"message\":\"a writer left changes unjournaled; they were rolled back\"}}");
                    w.rollback_pending();
                    swarm.journal_mut(&KEY).note_next_id(w.next_id());
                }
            }
            if hidden {
                eprintln!("specula: write_policy: {{\"error\":\"unrecorded_write\",\"entry\":\"{entry}\",\"tick\":{tick},\"message\":\"the world changed with nothing recorded (or recorded one per entity where full changes are checked); the journal starts over from it\"}}");
                swarm.journal_mut(&KEY).rebase(w, &format!("engine:unrecorded_by:{entry}"));
                swarm.journal_mut(&KEY).note_next_id(w.next_id());
            }
        });
        w.set_sealed(true);
        let clean = !pending && !hidden && !found.replaced && !found.settings;
        if !clean {
            if pending || hidden || found.settings {
                self.strays += 1;
            }
            if pending || hidden {
                debug_assert!(false, "write_policy: {entry} left world changes unjournaled or unrecorded");
            }
        }
        clean
    }

    /// The error an entry point answers with when [`Live::settle`] or [`Live::guarded`] found
    /// a writer's bug: what the writer did was rolled back, or the journal started over.
    fn refusal(entry: &str) -> WorldError {
        WorldError::new("unjournaled_write", format!("{entry}: a writer changed the world outside the write policy (unjournaled, unrecorded, the world replaced or the engine's settings moved); refused and reported (engine_info swarm.unjournaled_writes)"))
    }

    /// Runs a writer guarded, settles, reports seal breaks: whether it was all clean.
    fn run<T>(&mut self, swarm: &mut Swarm, tick: u64, entry: &str, files: Files, records: Records, f: impl FnOnce(&mut dyn WorldWrite, &mut Swarm) -> T) -> (T, bool) {
        let breaks = self.world.seal_breaks();
        let (out, found) = self.guarded(swarm, tick, entry, files, records, f);
        self.report_breaks(breaks, entry);
        let clean = self.settle(swarm, tick, entry, &found);
        (out, clean)
    }

    /// [`commit`] on the live world: an agent's tool, under `rule`.
    pub(crate) fn commit<T>(
        &mut self,
        swarm: &mut Swarm,
        tick: u64,
        w: Writer,
        kind: Kind,
        rule: Rule,
        command: impl FnOnce(&mut dyn WorldWrite) -> Result<T, WorldError>,
    ) -> Result<(T, Option<u64>), WorldError> {
        let files = if rule == Rule::Project { Files::Kept } else { Files::Lent };
        let (out, clean) = self.run(swarm, tick, "commit", files, Records::Full, |world, swarm| commit(world, swarm, tick, w, kind, rule, command));
        if !clean {
            return Err(Live::refusal("commit"));
        }
        out
    }

    /// [`disk`] on the live world.
    pub(crate) fn disk<T>(&mut self, swarm: &mut Swarm, tick: u64, command: impl FnOnce(&mut dyn WorldWrite) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        let (out, clean) = self.run(swarm, tick, "disk", Files::Kept, Records::Full, |world, swarm| disk(world, swarm, tick, command));
        if !clean {
            return Err(Live::refusal("disk"));
        }
        out
    }

    /// [`undo`] on the live world (the journal puts states back; no writer's code runs).
    pub(crate) fn undo(&mut self, swarm: &mut Swarm, tick: u64, w: Writer, req: &UndoRequest) -> Result<Undone, WorldError> {
        let (out, clean) = self.run(swarm, tick, "undo", Files::Kept, Records::Full, |world, swarm| undo(world, swarm, tick, w, req));
        if !clean {
            return Err(Live::refusal("undo"));
        }
        out
    }

    /// [`game_tool`] on the live world. The game gets a GameWorld: it can't replace the world or
    /// hide what it records.
    pub(crate) fn game_tool<T>(&mut self, swarm: &mut Swarm, tick: u64, game: &str, w: Writer, call: impl FnOnce(&mut GameWorld) -> T) -> Result<T, WorldError> {
        let (out, clean) = self.run(swarm, tick, "game_tool", Files::Lent, Records::Full, |world, swarm| game_tool(world, swarm, tick, game, w, call));
        if !clean {
            return Err(Live::refusal("game_tool"));
        }
        out
    }

    /// The writer tried to change how the world records, or to take its recorded changes, while
    /// it held the sealed world: refused (its writes stayed recorded, checked and journaled), and
    /// a bug in that writer: reported and counted with the strays.
    fn report_breaks(&mut self, before: u64, entry: &str) {
        let broke = self.world.seal_breaks() != before;
        if broke {
            self.strays += 1;
            eprintln!("specula: write_policy: {{\"error\":\"seal_break\",\"entry\":\"{entry}\",\"message\":\"the writer tried to change recording or take the world's recorded changes; refused (its writes are checked and journaled as usual)\"}}");
        }
    }

    /// An [`Exempt`] writer: `f` runs with the world recording as the writer does (sealed and
    /// guarded), and what it changed is journaled under the writer's name ([`journal_exempt`]).
    /// The hosted game's own reserved-data writes are reported and counted (not refused: a tick
    /// can't be refused halfway).
    pub(crate) fn exempt<T>(&mut self, swarm: &mut Swarm, tick: u64, why: Exempt, f: impl FnOnce(&mut dyn WorldWrite) -> T) -> T {
        let ((out, broke), _) = self.run(swarm, tick, "exempt", Files::Lent, Records::AsTheyDo, |world, swarm| {
            let out = recording_as(world, why.recording(), f);
            (out, journal_exempt(world, swarm, tick, why))
        });
        if let Some(e) = broke {
            self.game_reserved += 1;
            eprintln!("specula: write_policy: {{\"error\":\"game_reserved_write\",\"by\":\"{}\",\"tick\":{tick},\"message\":{}}}", why.by(), specula_core::Json::from(e.message.as_str()));
            debug_assert!(false, "write_policy: the hosted game wrote reserved data: {}", e.message);
        }
        out
    }

    /// A writer that commits as it goes ([`Opener`]): `f` gets a handle on the world (sealed and
    /// guarded) and the swarm, and must journal or roll back everything it changes through this
    /// module's functions before it returns ([`Live::settle`] checks, and fails closed). `Err`
    /// when it didn't (what it left was rolled back, or the journal started over): callers that
    /// answer an agent refuse with it.
    pub(crate) fn session<T>(&mut self, swarm: &mut Swarm, tick: u64, opener: Opener, f: impl FnOnce(&mut dyn WorldWrite, &mut Swarm) -> T) -> Result<T, WorldError> {
        let entry = match opener {
            Opener::Modules => "session(modules)",
            Opener::LexLive => "session(lex_live)",
            Opener::LexTools => "session(lex_tools)",
        };
        let (out, clean) = self.run(swarm, tick, entry, Files::Lent, Records::AsTheyDo, f);
        if clean {
            Ok(out)
        } else {
            Err(Live::refusal(entry))
        }
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
        swarm.journal_mut(&KEY).rebase(&world, label);
        world.mark_live();
        world.set_sealed(true);
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
        commit(world, self, tick, w, kind, Rule::World, |wd| command(wd.policy_world(&KEY)))
    }

    /// [`commit`] with [`Rule::Joint`].
    pub fn commit_joint<T>(&mut self, world: &mut World, tick: u64, w: Writer, kind: Kind, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        commit(world, self, tick, w, kind, Rule::Joint, |wd| command(wd.policy_world(&KEY)))
    }

    /// [`commit`] with [`Rule::Nav`].
    pub fn commit_nav<T>(&mut self, world: &mut World, tick: u64, w: Writer, kind: Kind, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        commit(world, self, tick, w, kind, Rule::Nav, |wd| command(wd.policy_world(&KEY)))
    }

    /// [`commit`] with [`Rule::Lex`].
    pub fn commit_lex<T>(&mut self, world: &mut World, tick: u64, w: Writer, kind: Kind, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        commit(world, self, tick, w, kind, Rule::Lex, |wd| command(wd.policy_world(&KEY)))
    }

    /// [`commit`] with [`Rule::Project`] (always [`Kind::File`]).
    pub fn commit_project<T>(&mut self, world: &mut World, tick: u64, w: Writer, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        commit(world, self, tick, w, Kind::File, Rule::Project, |wd| command(wd.policy_world(&KEY)))
    }

    /// [`commit`] with [`Rule::LexLive`].
    pub fn commit_lex_live<T>(&mut self, world: &mut World, tick: u64, w: Writer, kind: Kind, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        commit(world, self, tick, w, kind, Rule::LexLive, |wd| command(wd.policy_world(&KEY)))
    }

    /// [`disk`].
    pub fn commit_disk<T>(&mut self, world: &mut World, tick: u64, command: impl FnOnce(&mut World) -> Result<T, WorldError>) -> Result<(T, Option<u64>), WorldError> {
        disk(world, self, tick, |wd| command(wd.policy_world(&KEY)))
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

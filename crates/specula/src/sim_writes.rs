//! The simulation's own writers, and the sessions the engine opens for the module host and the
//! Lex tools: the only code outside `write_policy` that names the hosted game's exemptions
//! ([`Exempt::Game`], [`Exempt::GameStart`], [`Exempt::AnimReload`]) and these openers
//! (docs/design/WRITE_POLICY.md). Kept apart from the agent tools' dispatcher (`lib.rs`), so a
//! new agent tool can't reach an exemption without naming one of these functions, each of which
//! says whose write it is.

use crate::game::{Clock, Ctx, Frame, Game, GameRequest, GameWorld, Overlay};
use crate::journal::{Kind, Writer};
use crate::logic::LogicHost;
use crate::swarm::Swarm;
use crate::write_policy::{self, Exempt, Live, Opener};
use specula_protocol::ToolOutput;
use specula_core::Json;
use std::any::Any;
use std::collections::BTreeSet;

/// One of the hosted game's own calls ([`Game`]'s methods), the only things that run as the
/// game's exempt writes: no closure, so nothing but the game's own code can be journaled as the
/// game's (review round 3: any closure could run there).
pub(crate) enum GameCall<'c, 'f> {
    Tick,
    Frame(&'c mut Frame<'f>),
    Overlay(&'c mut Overlay<'f>),
    BeforeCapture,
    AssetsChanged(&'c [String]),
    /// The project's game file changed (its new text, from the project store).
    GameFile(&'c str),
    ApplyState(Box<dyn Any>),
    RequestDone(&'c GameRequest, Result<Json, Json>),
    /// The repair after a refused game tool, for games without checkpoints.
    RolledBack,
    /// An engine screen took (`true`) or gave back the window's input.
    ModalChanged(bool),
    /// A playing timeline's start, end, event, cue or subtitle line (docs/design/TIMELINE.md).
    Timeline(&'c crate::game::TimelineEvent),
    /// After a tick's modules ran: what they keep now (docs/design/FIRST_HOUR.md).
    AfterModules(&'c crate::game::ModuleStates<'f>),
}

impl GameCall<'_, '_> {
    /// The journal kind of its entry.
    fn kind(&self) -> Kind {
        match self {
            GameCall::Tick | GameCall::Timeline(_) | GameCall::AfterModules(_) => Kind::GameTick,
            GameCall::ApplyState(_) => Kind::GameStart,
            GameCall::RolledBack => Kind::GameTool,
            GameCall::Frame(_) | GameCall::Overlay(_) | GameCall::BeforeCapture | GameCall::AssetsChanged(_) | GameCall::GameFile(_) | GameCall::RequestDone(..) | GameCall::ModalChanged(_) => Kind::GameFrame,
        }
    }
}

/// The hosted game's own call: what it changes is journaled under `name`, exempt from claims
/// ([`Exempt::Game`]), as the call's kind.
pub(crate) fn game(world: &mut Live, swarm: &mut Swarm, game: &mut dyn Game, clock: &Clock, name: &str, windowed: bool, call: GameCall) {
    let why = Exempt::Game { name, kind: call.kind() };
    world.exempt(swarm, clock.tick(), why, |w| {
        let ctx = &mut Ctx { world: GameWorld::over(w), clock, agent: name, windowed };
        match call {
            GameCall::Tick => game.tick(ctx),
            GameCall::Frame(frame) => game.frame(ctx, frame),
            GameCall::Overlay(overlay) => game.overlay(ctx, overlay),
            GameCall::BeforeCapture => game.before_capture(ctx),
            GameCall::AssetsChanged(paths) => game.assets_changed(ctx, paths),
            GameCall::GameFile(text) => game.game_file_changed(ctx, text),
            GameCall::ApplyState(state) => game.apply_state(ctx, state),
            GameCall::RequestDone(request, result) => game.request_done(ctx, request, result),
            GameCall::RolledBack => game.world_rolled_back(ctx),
            GameCall::ModalChanged(open) => game.modal_changed(ctx, open),
            GameCall::Timeline(event) => game.timeline_event(ctx, event),
            GameCall::AfterModules(modules) => game.after_modules(ctx, modules),
        }
    });
}

/// The hosted game's `start` ([`Exempt::GameStart`]: full states, as made).
pub(crate) fn game_start(world: &mut Live, swarm: &mut Swarm, game: &mut dyn Game, clock: &Clock, name: &str, windowed: bool) {
    let why = Exempt::GameStart { name };
    world.exempt(swarm, clock.tick(), why, |w| game.start(&mut Ctx { world: GameWorld::over(w), clock, agent: name, windowed }));
}

/// One tick of every running module: each call journaled under its module, exempt from claims
/// (each request it makes is still checked for reserved data).
pub(crate) fn module_tick(world: &mut Live, swarm: &mut Swarm, logic: &mut LogicHost, tick: u64, hz: u32) {
    // A tick can't be refused: what a module left unjournaled was rolled back and reported.
    let _ = world.session(swarm, tick, Opener::Modules, |w, swarm| logic.tick(w, tick, hz, &mut write_policy::module_tick_commit(swarm, tick)));
}

/// Anim sets changed on disk resettle the graphs that use them ([`Exempt::AnimReload`]).
pub(crate) fn anim_reload(world: &mut Live, swarm: &mut Swarm, tick: u64, handles: &BTreeSet<u32>) {
    world.exempt(swarm, tick, Exempt::AnimReload, |w| crate::anim::resettle(w, handles));
}

/// The logic_* tools an agent `w` calls: loads, unloads and hooks commit as `w`'s
/// (`module_agent_commit`, claims checked).
pub(crate) fn logic_tools(world: &mut Live, swarm: &mut Swarm, logic: &mut LogicHost, clock: &Clock, call: crate::logic::tools::LogicCall, w: Writer) -> ToolOutput {
    world.session(swarm, clock.tick(), Opener::Modules, |wd, swarm| crate::logic::tools::handle(logic, call, wd, clock, swarm, w)).unwrap_or_else(|e| ToolOutput::error(e.to_json().to_pretty()))
}

/// The lex_* tools an agent `w` calls: they commit with `Rule::Lex` as `w`'s.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lex_tools(world: &mut Live, swarm: &mut Swarm, tick: u64, call: crate::lex_tools::LexCall, w: Writer, by: &str, hooks: &[String], in_process: &[String]) -> ToolOutput {
    world.session(swarm, tick, Opener::LexTools, |wd, swarm| crate::lex_tools::handle(call, wd, swarm, tick, w, by, hooks, in_process)).unwrap_or_else(|e| ToolOutput::error(e.to_json().to_pretty()))
}

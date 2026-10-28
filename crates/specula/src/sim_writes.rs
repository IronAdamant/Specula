//! The simulation's own writers, and the sessions the engine opens for the module host and the
//! Lex tools: the only code outside `write_policy` that names the hosted game's exemptions
//! ([`Exempt::Game`], [`Exempt::GameStart`], [`Exempt::AnimReload`]) and these openers
//! (docs/design/WRITE_POLICY.md). Kept apart from the agent tools' dispatcher (`lib.rs`), so a
//! new agent tool can't reach an exemption without naming one of these functions, each of which
//! says whose write it is.

use crate::game::{Clock, Ctx, Game, GameWorld};
use crate::journal::{Kind, Writer};
use crate::logic::LogicHost;
use crate::swarm::Swarm;
use crate::write_policy::{self, Exempt, Live, Opener};
use specula_protocol::ToolOutput;
use std::collections::BTreeSet;

/// The hosted game's own call (a tick, a frame, its overlay, `before_capture`, `apply_state`,
/// `request_done`, `assets_changed`, or the repair after a refused game tool): `call` gets the
/// game and its [`Ctx`]; what it changes is journaled as `kind` under `name`, exempt from
/// claims ([`Exempt::Game`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn game<T>(world: &mut Live, swarm: &mut Swarm, game: &mut dyn Game, clock: &Clock, name: &str, windowed: bool, kind: Kind, call: impl FnOnce(&mut dyn Game, &mut Ctx) -> T) -> T {
    let why = Exempt::Game { name, kind };
    world.exempt(swarm, clock.tick(), why, |w| call(game, &mut Ctx { world: GameWorld::new(w), clock, agent: name, windowed }))
}

/// The hosted game's `start` ([`Exempt::GameStart`]: full states, as made).
pub(crate) fn game_start(world: &mut Live, swarm: &mut Swarm, game: &mut dyn Game, clock: &Clock, name: &str, windowed: bool) {
    let why = Exempt::GameStart { name };
    world.exempt(swarm, clock.tick(), why, |w| game.start(&mut Ctx { world: GameWorld::new(w), clock, agent: name, windowed }));
}

/// One tick of every running module: each call journaled under its module, exempt from claims
/// (each request it makes is still checked for reserved data).
pub(crate) fn module_tick(world: &mut Live, swarm: &mut Swarm, logic: &mut LogicHost, tick: u64, hz: u32) {
    world.session(swarm, tick, Opener::Modules, |w, swarm| logic.tick(w, tick, hz, &mut write_policy::module_tick_commit(swarm, tick)));
}

/// Anim sets changed on disk resettle the graphs that use them ([`Exempt::AnimReload`]).
pub(crate) fn anim_reload(world: &mut Live, swarm: &mut Swarm, tick: u64, handles: &BTreeSet<u32>) {
    world.exempt(swarm, tick, Exempt::AnimReload, |w| crate::anim::resettle(w, handles));
}

/// The logic_* tools an agent `w` calls: loads, unloads and hooks commit as `w`'s
/// (`module_agent_commit`, claims checked).
pub(crate) fn logic_tools(world: &mut Live, swarm: &mut Swarm, logic: &mut LogicHost, clock: &Clock, call: crate::logic::tools::LogicCall, w: Writer) -> ToolOutput {
    world.session(swarm, clock.tick(), Opener::Modules, |wd, swarm| crate::logic::tools::handle(logic, call, wd, clock, swarm, w))
}

/// The lex_* tools an agent `w` calls: they commit with `Rule::Lex` as `w`'s.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lex_tools(world: &mut Live, swarm: &mut Swarm, tick: u64, call: crate::lex_tools::LexCall, w: Writer, by: &str, hooks: &[String], in_process: &[String]) -> ToolOutput {
    world.session(swarm, tick, Opener::LexTools, |wd, swarm| crate::lex_tools::handle(call, wd, swarm, tick, w, by, hooks, in_process))
}

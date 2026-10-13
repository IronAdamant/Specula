//! The swarm layer: who agents are, what they claimed, and the journal of what they did.
//!
//! The engine routes every agent write through here: identity ([`Swarm::hello`],
//! [`Swarm::agent_name`]), claims ([`crate::claims`]) and the journal ([`crate::journal`]). It
//! holds no GPU or window state, so all of it is tested headless.
//!
//! **Identity rules.** An agent's name is what `agent_hello` set, unique among open sessions
//! (idle ones included: a name another open session uses is refused with `name_taken` and a free
//! suggestion). The hosted game's name (and other in-process names) are reserved. Without
//! `agent_hello` a session is `<MCP client name>-<first 6 characters of its session id>`, so two
//! unnamed Claude Code sessions never share an identity. Claims and "your own changes" (undo) go
//! by name *and* session: a name taken again after its session ended adopts nothing (undoing the
//! old session's work needs `force`). A session can't change its name while it holds claims.

use crate::claims::{Claim, ClaimSpec, Claims};
use crate::journal::{self, Journal, Kind, UndoRequest, Undone, Writer};
use crate::world::{Change, World, WorldError};
use specula_core::Json;
use specula_protocol::SessionInfo;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// Agent identities remembered (sessions are capped at 1024 by the protocol).
const MAX_AGENTS: usize = 1024;

pub struct Agent {
    pub name: String,
    pub role: String,
}

/// See the module docs.
#[derive(Default)]
pub struct Swarm {
    pub journal: Journal,
    pub claims: Claims,
    /// Session id → identity given with agent_hello.
    agents: BTreeMap<String, Agent>,
    /// Names agents may not take: the hosted game's, in-process writers'.
    reserved: BTreeSet<String>,
}

/// A session's name before (or without) agent_hello.
pub fn default_name(s: &SessionInfo) -> String {
    let client = if s.client_name.is_empty() { "anonymous" } else { s.client_name.as_str() };
    let short: String = s.id.chars().take(6).collect();
    format!("{client}-{short}")
}

impl Swarm {
    /// Who a session is in attribution, claims and undo.
    pub fn agent_name(&self, session: &SessionInfo) -> String {
        match self.agents.get(&session.id) {
            Some(a) => a.name.clone(),
            None => default_name(session),
        }
    }

    pub fn role(&self, session_id: &str) -> &str {
        self.agents.get(session_id).map_or("builder", |a| a.role.as_str())
    }

    /// Whether the session said agent_hello.
    pub fn introduced(&self, session_id: &str) -> bool {
        self.agents.contains_key(session_id)
    }

    /// Keeps agents from calling themselves `name` (the hosted game's name, say).
    pub fn reserve(&mut self, name: &str) {
        self.reserved.insert(name.to_owned());
    }

    /// Names `session` (see the module docs); `live` are the protocol's open sessions.
    pub fn hello(&mut self, session: &SessionInfo, name: String, role: String, live: &[SessionInfo]) -> Result<Json, WorldError> {
        if self.reserved.contains(&name) {
            return Err(WorldError::new(
                "name_reserved",
                format!("'{name}' is reserved (the hosted game or the engine writes under it); pick another name"),
            )
            .with("name", name.as_str()));
        }
        let taken = |n: &str| self.reserved.contains(n) || live.iter().any(|s| s.id != session.id && self.agent_name(s) == n);
        if taken(&name) {
            let free = (2..).map(|i| format!("{name}-{i}")).find(|n| !taken(n)).expect("some suffix is free");
            return Err(WorldError::new(
                "name_taken",
                format!("another live session is already '{name}'; agent names are unique (try '{free}')"),
            )
            .with("name", name.as_str())
            .with("suggestion", free));
        }
        let current = self.agent_name(session);
        let held = self.claims.owned_by(Writer::new(&current, Some(&session.id)));
        if current != name && !held.is_empty() {
            return Err(WorldError::new(
                "holds_claims",
                format!("this session holds {} claim(s) as '{current}'; release them before changing its name", held.len()),
            )
            .with("name", current.as_str())
            .with("claims", Json::arr(held.into_iter().map(Json::from))));
        }
        if self.agents.len() >= MAX_AGENTS && !self.agents.contains_key(&session.id) {
            let ids: BTreeSet<&str> = live.iter().map(|s| s.id.as_str()).collect();
            self.agents.retain(|id, _| ids.contains(id.as_str()));
        }
        let reply = Json::obj([("agent", Json::from(name.as_str())), ("role", Json::from(role.as_str()))]);
        self.agents.insert(session.id.clone(), Agent { name, role });
        Ok(reply)
    }

    /// Drops claims (and shares) whose session is no longer live.
    pub fn sweep(&mut self, live: &[SessionInfo], now: Instant) {
        if self.claims.is_empty() {
            return;
        }
        let ids: BTreeSet<String> = live.iter().map(|s| s.id.clone()).collect();
        self.claims.expire(now, Some(&ids));
    }

    fn count_rejection(&mut self, e: &WorldError) {
        if matches!(e.code, "claimed" | "claims_overlap") {
            self.claims.rejected += 1;
        }
    }

    /// Applies an agent's world command: rejected (and rolled back) when it wrote inside another
    /// agent's claim, journaled otherwise. Returns the result and the journal id.
    pub fn commit<T>(
        &mut self,
        world: &mut World,
        tick: u64,
        w: Writer,
        kind: Kind,
        command: impl FnOnce(&mut World) -> Result<T, WorldError>,
    ) -> Result<(T, Option<u64>), WorldError> {
        let now = Instant::now();
        let claims = &self.claims;
        let mut check = |world: &World, change: &Change| claims.check(world, std::slice::from_ref(change), w, now);
        let result = journal::commit(world, &mut self.journal, tick, w, kind, &mut check, command);
        if let Err(e) = &result {
            self.count_rejection(e);
        }
        result
    }

    /// Undoes `w`'s entries, checked against claims like any write.
    pub fn undo(&mut self, world: &mut World, tick: u64, w: Writer, req: &UndoRequest) -> Result<Undone, WorldError> {
        let now = Instant::now();
        let claims = &self.claims;
        let mut check = |world: &World, change: &Change| claims.check(world, std::slice::from_ref(change), w, now);
        let result = self.journal.undo(world, tick, w, req, &mut check);
        if let Err(e) = &result {
            self.count_rejection(e);
        }
        result
    }

    /// Runs a game tool agent `w` called: the game's changes are checked against claims as `w`'s,
    /// step by step, and journaled under the game's name, `via` the agent.
    ///
    /// On rejection only the **world** is rolled back (ids it used are never handed out again).
    /// The tool has still run inside the game, so the caller must roll the game back too:
    /// [`crate::game::Game::restore`] from a checkpoint taken before the call, or
    /// [`crate::game::Game::world_rolled_back`] for games without checkpoints.
    pub fn game_tool<T>(
        &mut self,
        world: &mut World,
        tick: u64,
        game: &str,
        w: Writer,
        call: impl FnOnce(&mut World) -> T,
    ) -> Result<T, WorldError> {
        let mark = world.mark();
        let out = call(world);
        let now = Instant::now();
        let claims = &self.claims;
        let mut check = |world: &World, change: &Change| claims.check(world, std::slice::from_ref(change), w, now);
        if let Err(e) = world.recheck_steps(mark, &mut check) {
            self.journal.note_next_id(world.next_id());
            self.count_rejection(&e);
            return Err(e.with("game", game));
        }
        let changes = world.take_changes();
        self.journal.append_game(tick, game, Some(w.name), Kind::GameTool, changes, world.next_id());
        Ok(out)
    }

    /// Journals the hosted game's own changes (ticks, frames, start): exempt from claims.
    pub fn game_changes(&mut self, world: &mut World, tick: u64, game: &str, kind: Kind) {
        let changes = world.take_changes();
        self.journal.append_snapshot(tick, game, None, kind, changes, world.next_id());
    }

    /// Whether `w` may replace the whole world (world_load).
    pub fn check_replace_world(&self, w: Writer) -> Result<(), WorldError> {
        self.claims.check_replace_world(w, Instant::now())
    }

    /// claim_take; needs agent_hello first.
    pub fn take_claim(&mut self, world: &World, session: &SessionInfo, spec: ClaimSpec) -> Result<Json, WorldError> {
        if !self.introduced(&session.id) {
            return Err(WorldError::new(
                "hello_required",
                "claims belong to agent names: call agent_hello with a unique name first",
            ));
        }
        let name = self.agent_name(session);
        let now = Instant::now();
        self.claims.take(world, &name, &session.id, spec, now).map(|c| c.to_json(now))
    }

    /// claim_renew: one claim, or all of `w`'s.
    pub fn renew(&mut self, w: Writer, id: Option<u64>, lease: Duration) -> Result<Json, WorldError> {
        let now = Instant::now();
        let ids = id.map_or_else(|| self.claims.owned_by(w), |id| vec![id]);
        let mut renewed = Vec::new();
        let mut failed = None;
        for id in ids {
            match self.claims.renew(id, w, lease, now) {
                Ok(c) => renewed.push(c.to_json(now)),
                Err(e) => failed = Some(e),
            }
        }
        match failed {
            Some(e) if renewed.is_empty() => Err(e),
            _ => Ok(Json::obj([("renewed", Json::Arr(renewed))])),
        }
    }

    /// claim_release: one claim, or all of `w`'s.
    pub fn release(&mut self, w: Writer, id: Option<u64>) -> Result<Json, WorldError> {
        let ids = match id {
            Some(id) => self.claims.release(id, w, Instant::now()).map(|()| vec![id])?,
            None => self.claims.release_all(w),
        };
        Ok(Json::obj([("released", Json::arr(ids.into_iter().map(Json::from)))]))
    }

    /// claim_share: `with` must be a live, introduced agent; the share binds to its session.
    pub fn share(&mut self, w: Writer, id: u64, with: &str, revoke: bool, live: &[SessionInfo]) -> Result<Json, WorldError> {
        let target = live.iter().find(|s| self.introduced(&s.id) && self.agent_name(s) == with).map(|s| s.id.clone());
        let session = match (target, revoke) {
            (Some(s), _) => s,
            (None, true) => String::new(),
            (None, false) => {
                return Err(WorldError::new(
                    "unknown_agent",
                    format!("no live session has said agent_hello as '{with}'; shares go to a connected, named agent"),
                )
                .with("with", with))
            }
        };
        let now = Instant::now();
        self.claims.share(id, w, with, &session, revoke, now).map(|c| c.to_json(now))
    }

    pub fn claim(&self, id: u64) -> Option<&Claim> {
        self.claims.get(id)
    }

    /// Sizes for engine_info.
    pub fn stats_json(&self) -> Json {
        Json::obj([
            ("journal", self.journal.stats_json()),
            ("claims", Json::from(self.claims.len())),
            ("claim_rejections", Json::from(self.claims.rejected)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claims::{Scope, DEFAULT_LEASE};
    use crate::world::{EntityKey, Recording, Shape, SpawnSpec, UpdateSpec};

    fn session(id: &str, client: &str) -> SessionInfo {
        SessionInfo { id: id.into(), client_name: client.into(), client_version: "1".into(), protocol_version: "x".into() }
    }

    fn world() -> World {
        let mut w = World::default();
        w.set_recording(Recording::Full);
        w
    }

    fn spec(path: &str) -> SpawnSpec {
        let mut s = SpawnSpec::new(Shape::Group);
        let (parent, name) = path.rsplit_once('/').unwrap();
        s.name = Some(name.into());
        s.parent = (!parent.is_empty()).then(|| EntityKey::Path(parent.into()));
        s
    }

    fn claim(path: &str) -> ClaimSpec {
        ClaimSpec { scope: Scope::path(path).unwrap(), lease: DEFAULT_LEASE, note: String::new() }
    }

    /// Reviewer finding: a third session saying hello as "ann" could act as ann; unnamed Claude
    /// Code sessions all shared one name.
    #[test]
    fn names_are_unique_and_sessions_adopt_nothing() {
        let (s1, s2, s3) = (session("aaaaaa11", "claude-code"), session("bbbbbb22", "claude-code"), session("cccccc33", "x"));
        let live = vec![s1.clone(), s2.clone(), s3.clone()];
        let mut sw = Swarm::default();
        assert_ne!(sw.agent_name(&s1), sw.agent_name(&s2), "unnamed sessions don't collide");
        assert_eq!(sw.agent_name(&s1), "claude-code-aaaaaa");
        sw.hello(&s1, "ann".into(), "builder".into(), &live).unwrap();
        let err = sw.hello(&s2, "ann".into(), "builder".into(), &live).unwrap_err();
        assert_eq!((err.code, err.detail("suggestion")), ("name_taken", Some(&Json::from("ann-2"))));
        let err = sw.hello(&s3, "claude-code-bbbbbb".into(), "builder".into(), &live).unwrap_err();
        assert_eq!(err.code, "name_taken", "default names are taken too");
        // Once ann's session is gone her name is free again, but her claims and undo aren't.
        let mut w = world();
        sw.take_claim(&w, &s1, claim("/q")).unwrap();
        let ann = Writer::new("ann", Some("aaaaaa11"));
        sw.commit(&mut w, 0, ann, Kind::Spawn, |w| w.spawn(spec("/q"), "ann").map(|_| ())).unwrap();
        let later = vec![s2.clone(), s3.clone()];
        sw.hello(&s2, "ann".into(), "builder".into(), &later).unwrap();
        let impostor = Writer::new("ann", Some("bbbbbb22"));
        assert_eq!(sw.release(impostor, Some(1)).unwrap_err().code, "not_owner");
        let write = sw.commit(&mut w, 0, impostor, Kind::Spawn, |w| w.spawn(spec("/q/x"), "ann").map(|_| ()));
        assert_eq!(write.unwrap_err().code, "claimed");
        let undo = sw.undo(&mut w, 0, impostor, &UndoRequest { ids: None, count: 1, force: false });
        assert_eq!(undo.unwrap_err().code, "nothing_to_undo");
        let undo = sw.undo(&mut w, 0, impostor, &UndoRequest { ids: Some(vec![1]), count: 1, force: false });
        assert_eq!(undo.unwrap_err().code, "not_yours");
        // Renaming while holding claims is refused.
        let err = sw.hello(&s1, "annie".into(), "builder".into(), &live).unwrap_err();
        assert_eq!(err.code, "holds_claims");
        // Shares need a live, named agent and bind to its session.
        assert_eq!(sw.share(ann, 1, "nobody", false, &live).unwrap_err().code, "unknown_agent");
        sw.hello(&s3, "carl".into(), "builder".into(), &live).unwrap();
        sw.share(ann, 1, "carl", false, &live).unwrap();
        let carl = Writer::new("carl", Some("cccccc33"));
        sw.commit(&mut w, 0, carl, Kind::Spawn, |w| w.spawn(spec("/q/c"), "carl").map(|_| ())).unwrap();
        assert_eq!(sw.take_claim(&w, &session("dddd", "y"), claim("/z")).unwrap_err().code, "hello_required");
    }

    /// Reviewer finding (blocker): an ancestor's rename carried claimed entities out of their
    /// claim; the same through world_batch and through undo (which puts an old state back).
    #[test]
    fn ancestor_moves_are_checked_through_batches_and_undo() {
        use crate::world::Op;
        let (s1, s2) = (session("aaaaaa11", "a"), session("bbbbbb22", "b"));
        let live = vec![s1.clone(), s2.clone()];
        let mut sw = Swarm::default();
        let mut w = world();
        sw.hello(&s1, "ann".into(), "builder".into(), &live).unwrap();
        sw.hello(&s2, "bob".into(), "builder".into(), &live).unwrap();
        let (ann, bob) = (Writer::new("ann", Some("aaaaaa11")), Writer::new("bob", Some("bbbbbb22")));
        for p in ["/a", "/a/b", "/a/b/wall", "/lane", "/lane/c"] {
            sw.commit(&mut w, 0, bob, Kind::Spawn, |w| w.spawn(spec(p), "bob").map(|_| ())).unwrap();
        }
        sw.take_claim(&w, &s1, claim("/a/b")).unwrap();
        let rename = |n: &str| UpdateSpec { name: Some(n.into()), ..Default::default() };
        let ops = vec![Op::Set(EntityKey::Path("/a".into()), rename("z"))];
        let batch = sw.commit(&mut w, 0, bob, Kind::Batch, |w| w.apply_batch(ops, "bob").map_err(|(_, e)| e));
        assert_eq!(batch.unwrap_err().code, "claimed");
        assert!(w.find_path("/a/b/wall").is_ok(), "nothing moved");
        // Undo: bob renames his /lane to /road, ann then claims /lane/c, bob undoes the rename.
        sw.commit(&mut w, 0, bob, Kind::Set, |w| w.update(&EntityKey::Path("/lane".into()), rename("road"), "bob").map(|_| ()))
            .unwrap();
        sw.take_claim(&w, &s1, claim("/lane/c")).unwrap();
        let undo = sw.undo(&mut w, 0, bob, &UndoRequest { ids: None, count: 1, force: true });
        assert_eq!(undo.unwrap_err().code, "claimed", "force doesn't override claims");
        assert!(w.find_path("/road/c").is_ok(), "rolled back");
        // The owner can still work inside.
        sw.commit(&mut w, 0, ann, Kind::Spawn, |w| w.spawn(spec("/a/b/gate"), "ann").map(|_| ())).unwrap();
        assert_eq!(sw.journal.replay().unwrap().state_hash(), w.state_hash());
    }

    /// Reviewer finding: world_load ignored claims.
    #[test]
    fn replacing_the_world_respects_others_claims() {
        let s1 = session("aaaaaa11", "a");
        let live = vec![s1.clone()];
        let mut sw = Swarm::default();
        let w = world();
        sw.hello(&s1, "ann".into(), "builder".into(), &live).unwrap();
        sw.take_claim(&w, &s1, claim("/q")).unwrap();
        assert!(sw.check_replace_world(Writer::new("ann", Some("aaaaaa11"))).is_ok());
        let err = sw.check_replace_world(Writer::new("bob", Some("bbbb"))).unwrap_err();
        assert_eq!((err.code, err.detail("count")), ("claimed", Some(&Json::from(1))));
        sw.sweep(&[], Instant::now());
        assert!(sw.check_replace_world(Writer::new("bob", Some("bbbb"))).is_ok(), "claims end with their session");
    }

    /// Reviewer finding: game tools an agent called skipped claims.
    #[test]
    fn game_tools_are_checked_as_the_agent_that_called_them() {
        let (s1, s2) = (session("aaaaaa11", "a"), session("bbbbbb22", "b"));
        let live = vec![s1.clone(), s2.clone()];
        let mut sw = Swarm::default();
        let mut w = world();
        sw.hello(&s1, "ann".into(), "builder".into(), &live).unwrap();
        sw.take_claim(&w, &s1, claim("/q")).unwrap();
        let ann = Writer::new("ann", Some("aaaaaa11"));
        sw.commit(&mut w, 0, ann, Kind::Spawn, |w| w.spawn(spec("/q"), "ann").map(|_| ())).unwrap();
        let shape = |w: &World| w.entities().map(|e| (e.id, e.version, w.path_of(e.id))).collect::<Vec<_>>();
        let before = shape(&w);
        let bob = Writer::new("bob", Some("bbbbbb22"));
        let tool = |w: &mut World| {
            w.spawn(spec("/tower"), "game").unwrap();
            w.update(&EntityKey::Path("/q".into()), UpdateSpec { name: Some("razed".into()), ..Default::default() }, "game").unwrap();
            "built"
        };
        let err = sw.game_tool(&mut w, 1, "sepulcra", bob, tool).unwrap_err();
        assert_eq!((err.code, err.detail("game")), ("claimed", Some(&Json::from("sepulcra"))));
        assert_eq!(shape(&w), before, "the game's changes were rolled back");
        assert!(w.pending_changes().is_empty());
        let ok = sw.game_tool(&mut w, 1, "sepulcra", bob, |w| w.spawn(spec("/tower"), "game").map(|_| "ok").unwrap());
        assert_eq!(ok.unwrap(), "ok");
        let last = sw.journal.entries().last().unwrap();
        assert_eq!((last.kind, &*last.by, last.via.as_deref()), (Kind::GameTool, "sepulcra", Some("bob")));
        assert_eq!(sw.journal.replay().unwrap().state_hash(), w.state_hash());
    }
}

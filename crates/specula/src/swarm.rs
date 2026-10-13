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
//!
//! **Resuming an identity.** Scripted agents open a new MCP session per run, so a session is not
//! an agent's lifetime. `agent_hello` returns a `resume_token`; a later session that says
//! `agent_hello {name, resume_token}` becomes that agent: its claims, shares and undo history move
//! with it, and the earlier session (if still open) falls back to its default name. Ownership is
//! keyed by an identity key that the token carries over, not by the protocol session id, and an
//! introduced identity's claims last until their lease ends even while no session is open.

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
    /// What claims and undo ownership are keyed by (carried over by a resume).
    pub key: String,
    /// Proves a later session is this agent (`agent_hello {resume_token}`).
    pub token: String,
}

/// 32 hex characters from randomly keyed SipHash (std only), like protocol session ids.
fn random_hex() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut out = String::with_capacity(32);
    for lane in 0u8..2 {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(nanos);
        h.write_u64(count);
        h.write_u8(lane);
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
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

    /// The identity key `session` writes and claims under: its agent's key after agent_hello
    /// (kept across resumes), else the session id itself.
    pub fn key(&self, session: &SessionInfo) -> String {
        match self.agents.get(&session.id) {
            Some(a) => a.key.clone(),
            None => session.id.clone(),
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
    pub fn hello(
        &mut self,
        session: &SessionInfo,
        name: String,
        role: String,
        resume: Option<String>,
        live: &[SessionInfo],
    ) -> Result<Json, WorldError> {
        if let Some(token) = resume {
            return self.resume(session, name, role, &token);
        }
        if self.reserved.contains(&name) {
            return Err(WorldError::new(
                "name_reserved",
                format!("'{name}' is reserved (the hosted game or the engine writes under it); pick another name"),
            )
            .with("name", name.as_str()));
        }
        // A name is taken by an open session using it, or by a dormant identity that still holds
        // live claims (its owner may come back with the resume token).
        let my_key = self.key(session);
        let now = Instant::now();
        let taken = |n: &str| {
            self.reserved.contains(n)
                || live.iter().any(|s| s.id != session.id && self.agent_name(s) == n)
                || self.claims.held_by_name_elsewhere(n, &my_key, now)
        };
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
        let current_key = self.key(session);
        let held = self.claims.owned_by(Writer::new(&current, Some(&current_key)));
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
        // Saying hello again with the same name keeps the identity (and its token).
        let (key, token) = match self.agents.get(&session.id) {
            Some(a) if a.name == name => (a.key.clone(), a.token.clone()),
            // A new identity is keyed by the session that introduced it (unique, and what an
            // unnamed session was already keyed by); a resume carries the key to later sessions.
            _ => (session.id.clone(), random_hex()),
        };
        let reply = Json::obj([
            ("agent", Json::from(name.as_str())),
            ("role", Json::from(role.as_str())),
            ("resume_token", Json::from(token.as_str())),
            ("note", Json::from("keep resume_token private; a later session continues as this agent (claims and undo included) with agent_hello {name, resume_token}")),
        ]);
        self.agents.insert(session.id.clone(), Agent { name, role, key, token });
        Ok(reply)
    }

    /// `agent_hello {name, resume_token}`: this session becomes the agent the token belongs to.
    fn resume(&mut self, session: &SessionInfo, name: String, role: String, token: &str) -> Result<Json, WorldError> {
        let found = self.agents.iter().find(|(_, a)| a.token == token).map(|(sid, a)| (sid.clone(), a.name.clone()));
        let Some((old_sid, old_name)) = found else {
            return Err(WorldError::new(
                "bad_resume_token",
                "no agent has this resume_token (it is from another engine run, or was mistyped); say agent_hello without it to start a new identity",
            ));
        };
        if old_name != name {
            return Err(WorldError::new(
                "bad_resume_token",
                format!("this resume_token belongs to '{old_name}', not '{name}'"),
            )
            .with("name", name.as_str()));
        }
        // This session may itself hold claims under another identity: it can't switch then.
        let current = self.agent_name(session);
        let current_key = self.key(session);
        let held = self.claims.owned_by(Writer::new(&current, Some(&current_key)));
        if old_sid != session.id && !held.is_empty() {
            return Err(WorldError::new(
                "holds_claims",
                format!("this session holds {} claim(s) as '{current}'; release them before resuming another identity", held.len()),
            )
            .with("name", current.as_str()));
        }
        let mut agent = self.agents.remove(&old_sid).expect("found above");
        agent.role = role;
        let reply = Json::obj([
            ("agent", Json::from(agent.name.as_str())),
            ("role", Json::from(agent.role.as_str())),
            ("resumed", Json::from(true)),
            ("claims", Json::arr(self.claims.owned_by(Writer::new(&agent.name, Some(&agent.key))).into_iter().map(Json::from))),
        ]);
        self.agents.insert(session.id.clone(), agent);
        Ok(reply)
    }

    /// Drops claims (and shares) whose session is no longer live.
    pub fn sweep(&mut self, live: &[SessionInfo], now: Instant) {
        if self.claims.is_empty() {
            return;
        }
        // Claims are keyed by identity. An introduced identity can come back with its resume
        // token, so its claims last until their lease ends even with no session open (scripts
        // run in short sessions); only an unnamed session's claims die with it.
        let mut ids: BTreeSet<String> = live.iter().map(|s| self.key(s)).collect();
        ids.extend(self.agents.values().map(|a| a.key.clone()));
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
        let check: Option<&mut journal::Check> = if claims.could_block(w, now) { Some(&mut check) } else { None };
        let result = journal::commit(world, &mut self.journal, tick, w, kind, check, command);
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
        let check: Option<&mut journal::Check> = if claims.could_block(w, now) { Some(&mut check) } else { None };
        let result = self.journal.undo(world, tick, w, req, check);
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
        let checked = if claims.could_block(w, now) { world.recheck_steps(mark, &mut check) } else { Ok(()) };
        if let Err(e) = checked {
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
        let key = self.key(session);
        let now = Instant::now();
        self.claims.take(world, &name, &key, spec, now).map(|c| c.to_json(now))
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
        let target = live.iter().find(|s| self.introduced(&s.id) && self.agent_name(s) == with).map(|s| self.key(s));
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
        sw.hello(&s1, "ann".into(), "builder".into(), None, &live).unwrap();
        let err = sw.hello(&s2, "ann".into(), "builder".into(), None, &live).unwrap_err();
        assert_eq!((err.code, err.detail("suggestion")), ("name_taken", Some(&Json::from("ann-2"))));
        let err = sw.hello(&s3, "claude-code-bbbbbb".into(), "builder".into(), None, &live).unwrap_err();
        assert_eq!(err.code, "name_taken", "default names are taken too");
        // Once ann's session is gone her name is free again, but her claims and undo aren't.
        let mut w = world();
        sw.take_claim(&w, &s1, claim("/q")).unwrap();
        let k1 = sw.key(&s1);
        let ann = Writer::new("ann", Some(&k1));
        sw.commit(&mut w, 0, ann, Kind::Spawn, |w| w.spawn(spec("/q"), "ann").map(|_| ())).unwrap();
        let later = vec![s2.clone(), s3.clone()];
        // While ann's claim lives her name stays hers (she can come back with her token).
        let err = sw.hello(&s2, "ann".into(), "builder".into(), None, &later).unwrap_err();
        assert_eq!(err.code, "name_taken");
        // Once the claim has lapsed the name is free, but the newcomer adopts nothing.
        sw.sweep(&later, Instant::now() + Duration::from_secs(301));
        sw.hello(&s2, "ann".into(), "builder".into(), None, &later).unwrap();
        let k2 = sw.key(&s2);
        let impostor = Writer::new("ann", Some(&k2));
        let undo = sw.undo(&mut w, 0, impostor, &UndoRequest { ids: None, count: 1, force: false });
        assert_eq!(undo.unwrap_err().code, "nothing_to_undo");
        let undo = sw.undo(&mut w, 0, impostor, &UndoRequest { ids: Some(vec![1]), count: 1, force: false });
        assert_eq!(undo.unwrap_err().code, "not_yours");
        // Renaming while holding claims is refused (the original ann takes a fresh claim).
        sw.commit(&mut w, 0, ann, Kind::Spawn, |w| w.spawn(spec("/r"), "ann").map(|_| ())).unwrap();
        let r = sw.take_claim(&w, &s1, claim("/r")).unwrap();
        let rid = r.get("id").and_then(Json::as_u64).unwrap();
        let err = sw.hello(&s1, "annie".into(), "builder".into(), None, &live).unwrap_err();
        assert_eq!(err.code, "holds_claims");
        // Shares need a live, named agent and bind to its identity.
        assert_eq!(sw.share(ann, rid, "nobody", false, &live).unwrap_err().code, "unknown_agent");
        sw.hello(&s3, "carl".into(), "builder".into(), None, &live).unwrap();
        sw.share(ann, rid, "carl", false, &live).unwrap();
        let k3 = sw.key(&s3);
        let carl = Writer::new("carl", Some(&k3));
        sw.commit(&mut w, 0, carl, Kind::Spawn, |w| w.spawn(spec("/r/c"), "carl").map(|_| ())).unwrap();
        assert_eq!(sw.take_claim(&w, &session("dddd", "y"), claim("/z")).unwrap_err().code, "hello_required");
    }

    /// Dogfooding (M1 exit proof): scripted agents open a session per run and locked themselves
    /// out of their own claims. A resume_token carries the identity, claims and undo over.
    #[test]
    fn a_resume_token_carries_identity_claims_and_undo() {
        let (s1, s2, s3) = (session("aaaaaa11", "py"), session("bbbbbb22", "py"), session("cccccc33", "py"));
        let live = vec![s1.clone(), s2.clone(), s3.clone()];
        let mut sw = Swarm::default();
        let mut w = world();
        let hi = sw.hello(&s1, "mason".into(), "builder".into(), None, &live).unwrap();
        let token = hi.get("resume_token").and_then(Json::as_str).unwrap().to_owned();
        assert_eq!(token.len(), 32);
        sw.take_claim(&w, &s1, claim("/d")).unwrap();
        let k1 = sw.key(&s1);
        sw.commit(&mut w, 0, Writer::new("mason", Some(&k1)), Kind::Spawn, |w| w.spawn(spec("/d"), "mason").map(|_| ())).unwrap();
        // A wrong token, or the right token with another name, is refused.
        let bad = sw.hello(&s3, "mason".into(), "builder".into(), Some("0".repeat(32)), &live).unwrap_err();
        assert_eq!(bad.code, "bad_resume_token");
        let bad = sw.hello(&s3, "other".into(), "builder".into(), Some(token.clone()), &live).unwrap_err();
        assert_eq!(bad.code, "bad_resume_token");
        // While mason's claim lives, nobody else may become 'mason' without the token, even with
        // no session of mason's open.
        assert_eq!(sw.hello(&s3, "mason".into(), "builder".into(), None, std::slice::from_ref(&s3)).unwrap_err().code, "name_taken");
        // The next run resumes: same name, the claim and undo history come along.
        let back = sw.hello(&s2, "mason".into(), "builder".into(), Some(token.clone()), &live).unwrap();
        assert_eq!((back.get("resumed"), back.get("claims")), (Some(&Json::from(true)), Some(&Json::arr([Json::from(1)]))));
        assert_eq!((sw.agent_name(&s2), sw.key(&s2)), ("mason".to_string(), k1.clone()));
        assert_ne!(sw.agent_name(&s1), "mason", "the old session falls back to its default name");
        let me = Writer::new("mason", Some(&k1));
        sw.commit(&mut w, 0, me, Kind::Spawn, |w| w.spawn(spec("/d/more"), "mason").map(|_| ())).unwrap();
        assert!(sw.undo(&mut w, 0, me, &UndoRequest { ids: None, count: 2, force: false }).is_ok(), "undo spans both runs");
        // The claim outlives the sessions (the token can bring the owner back) until its lease ends.
        sw.sweep(&[], Instant::now());
        assert_eq!(sw.claims.owned_by(me), vec![1]);
        sw.sweep(&[], Instant::now() + Duration::from_secs(301));
        assert!(sw.claims.owned_by(me).is_empty(), "the lease still ends it");
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
        sw.hello(&s1, "ann".into(), "builder".into(), None, &live).unwrap();
        sw.hello(&s2, "bob".into(), "builder".into(), None, &live).unwrap();
        let (k1, k2) = (sw.key(&s1), sw.key(&s2));
        let (ann, bob) = (Writer::new("ann", Some(&k1)), Writer::new("bob", Some(&k2)));
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
        sw.hello(&s1, "ann".into(), "builder".into(), None, &live).unwrap();
        sw.take_claim(&w, &s1, claim("/q")).unwrap();
        assert!(sw.check_replace_world(Writer::new("ann", Some(&sw.key(&s1)))).is_ok());
        let err = sw.check_replace_world(Writer::new("bob", Some("bbbb"))).unwrap_err();
        assert_eq!((err.code, err.detail("count")), ("claimed", Some(&Json::from(1))));
        sw.sweep(&[], Instant::now() + Duration::from_secs(301));
        assert!(sw.check_replace_world(Writer::new("bob", Some("bbbb"))).is_ok(), "claims end with their lease");
    }

    /// Reviewer finding: game tools an agent called skipped claims.
    #[test]
    fn game_tools_are_checked_as_the_agent_that_called_them() {
        let (s1, s2) = (session("aaaaaa11", "a"), session("bbbbbb22", "b"));
        let live = vec![s1.clone(), s2.clone()];
        let mut sw = Swarm::default();
        let mut w = world();
        sw.hello(&s1, "ann".into(), "builder".into(), None, &live).unwrap();
        sw.take_claim(&w, &s1, claim("/q")).unwrap();
        let k1 = sw.key(&s1);
        let ann = Writer::new("ann", Some(&k1));
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

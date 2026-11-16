//! The swarm layer: who agents are, what they claimed, and the journal of what they did.
//!
//! It holds identity ([`Swarm::hello`], [`Swarm::agent_name`]), claims ([`crate::claims`]) and
//! the journal ([`crate::journal`]); the rules every world write passes (claims, reserved data,
//! joints, journaling) are [`crate::write_policy`]'s. It holds no GPU or window state, so all of
//! it is tested headless.
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

use crate::board::Board;
use crate::claims::{Claim, ClaimSpec, Claims};
use crate::intents::Intents;
use crate::leases::Leases;
use crate::journal::{Journal, Writer};
use crate::world::{Change, World, WorldError};

/// Attribution prefix of game modules' world changes (`module:<name>`): no agent may take a name
/// starting with it.
pub const MODULE_PREFIX: &str = "module:";
use specula_core::Json;
use specula_protocol::SessionInfo;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

/// Agent identities remembered (sessions are capped at 1024 by the protocol).
const MAX_AGENTS: usize = 1024;

pub struct Agent {
    pub name: String,
    pub role: String,
    /// What the role carries, as admitted (never self-declared: [`crate::authority::Grants`]).
    pub authority: crate::authority::Authority,
    /// What claims and undo ownership are keyed by (carried over by a resume).
    pub key: String,
    /// Proves a later session is this agent (`agent_hello {resume_token}`).
    pub token: String,
}

/// 32 hex characters from randomly keyed SipHash (std only), like protocol session ids.
pub(crate) fn random_hex() -> String {
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


#[derive(Default)]
pub struct Swarm {
    /// Read with [`Swarm::journal`]; only the write policy writes it ([`Swarm::journal_mut`]).
    journal: Journal,
    pub claims: Claims,
    /// Session id → identity given with agent_hello.
    agents: BTreeMap<String, Agent>,
    /// Names agents may not take: the hosted game's, in-process writers'.
    reserved: BTreeSet<String>,
    /// The task board and notes (see [`crate::board`]).
    pub board: Board,
    /// Declared intents (see [`crate::intents`]).
    pub intents: Intents,
    /// Ports, GPU time and scratch folders leased to agents (see [`crate::leases`]).
    pub leases: Leases,
    /// The review queue and critiques, saved with the board (see [`crate::review`]).
    pub review: crate::review::Review,
    /// The authority roles the operator granted, and who holds them (see [`crate::authority`]).
    pub grants: crate::authority::Grants,
    /// Agent name → refused writes by error code, for roll-ups (at most [`MAX_AGENTS`] names).
    refused: BTreeMap<String, BTreeMap<&'static str, u64>>,
    /// Prune the world's removed-entity versions once there are more than this (twice what the
    /// last prune kept; see [`crate::write_policy::prune_retired`]).
    pub(crate) retired_prune_at: usize,
    /// Who started each module (`module:<name>`) and live Lex run (`lex:<set>`), for claim
    /// notices (at most [`MAX_AGENTS`]).
    starters: BTreeMap<String, String>,
    /// Writes a module's or live Lex run's own ticks made to entities another agent claimed
    /// (exempt from claims, so never refused), by the claim's holder, newest last, and how many
    /// of the holder's were dropped past [`MAX_CLAIM_NOTICES`]: told to the holder in claim_list.
    /// One holder's notices never push another's out (at most [`MAX_AGENTS`] holders).
    claim_notices: BTreeMap<String, (VecDeque<Json>, u64)>,
    /// Claim notices ever made.
    claim_notices_total: u64,
    /// Changes a call made past [`NOTICES_PER_CALL`] notices, never examined for claims.
    claim_notices_unexamined: u64,
    /// The request being dispatched: its caller's identity key, and the journal entries the
    /// dispatch's own code made, **whoever wrote them** (its commits and undos, what a game,
    /// module or live Lex run committed because it asked, sessions included, and what an engine
    /// writer such as `taste` committed on its behalf: the M4 final review found a configurator
    /// step written as `taste` beside the owner's decision, uncounted). Ticks a request
    /// runs (`time_control step`) are the simulation's: counting pauses for them, so held input
    /// firing as its holder is never the holder's call (round 2 of the review). One call, one
    /// commit (`docs/design/AUTHORITY.md`).
    dispatch: Option<(String, u64)>,
    /// The newest journal entry [`Swarm::note_deletes`] examined.
    deletes_seen: u64,
    /// Agent name → calls the engine handled and commits charged to them, for the editor's
    /// cost dashboard (at most [`MAX_AGENTS`] names).
    usage: BTreeMap<String, Usage>,
    /// Counts the cost view's updates ([`Usage::last`]).
    usage_clock: u64,
}

/// One agent's use of the engine since it started (the editor's cost dashboard).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    /// Tool calls the engine handled for it.
    pub calls: u64,
    /// Journal entries charged to its calls (one call, one commit).
    pub commits: u64,
    /// Tokens, only if a harness reports them (none does yet): never measured by the engine.
    pub tokens: Option<u64>,
    /// When it was last counted (a counter): the name heard from longest ago makes room.
    last: u64,
}

/// A paused dispatch count ([`Swarm::pause_dispatch`]).
pub(crate) struct PausedDispatch(Option<(String, u64)>);

/// Claim notices kept per holder (see `Swarm::claim_notices`), and how many one journaled call
/// may make.
pub const MAX_CLAIM_NOTICES: usize = 64;
const NOTICES_PER_CALL: usize = 16;

/// The world's removed-entity versions are pruned once there are at least this many.
pub const RETIRED_PRUNE_MIN: usize = 1024;

/// A session's name before (or without) agent_hello.
pub fn default_name(s: &SessionInfo) -> String {
    // At most 48 characters of the client's name, and only the characters an agent's own name
    // may have (`authority::name_char`: ASCII letters, digits, space, _ . -). Clients choose
    // their name freely, and this one is shown to the owner (the security toast, the window
    // title, the journal): no bidirectional overrides, no look-alikes, nothing that reads as
    // the engine's own words in another script (review R2 of m5-inputguard, major 5).
    let client: String = s.client_name.chars().filter(|c| crate::authority::name_char(*c)).take(48).collect();
    let client = client.trim().to_owned();
    let client = if client.is_empty() { "anonymous".to_owned() } else { client };
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

    /// Notes one journal entry made by an agent-facing entry point: counted against the request
    /// being dispatched, whoever wrote it (see the field).
    pub(crate) fn note_commit(&mut self) {
        if let Some((_, n)) = &mut self.dispatch {
            *n += 1;
        }
    }

    /// Starts counting the commits of a request by the identity `key`.
    pub(crate) fn begin_dispatch(&mut self, key: &str) {
        self.dispatch = Some((key.to_owned(), 0));
    }

    /// Ends the count: the journal entries made by the dispatch.
    pub(crate) fn end_dispatch(&mut self) -> u64 {
        self.dispatch.take().map_or(0, |(_, n)| n)
    }

    /// Pauses the count while the simulation runs ticks inside a request.
    pub(crate) fn pause_dispatch(&mut self) -> PausedDispatch {
        PausedDispatch(self.dispatch.take())
    }

    pub(crate) fn resume_dispatch(&mut self, paused: PausedDispatch) {
        self.dispatch = paused.0;
    }

    /// The authority of the identity `key` (its session's, when one is open; else none).
    pub(crate) fn authority_of_key(&self, key: &str) -> crate::authority::Authority {
        self.agents.values().filter(|a| a.key == key).map(|a| a.authority).max().unwrap_or_default()
    }

    /// Whether the identity `key` may load native module code ([`crate::authority::may_load_native`]):
    /// `Ok`, or the refusal naming `what`.
    pub(crate) fn may_load_native(&self, key: &str, what: &str) -> Result<(), crate::world::WorldError> {
        crate::authority::may_load_native(self.authority_of_key(key), self.grants.grant_of_key(key), what)
    }

    /// The session's authority as the operator's grants alone give it (read through
    /// [`crate::authority::trusted`]): a lead nobody granted would be a plain agent here.
    pub(crate) fn trusted_authority_of(&self, session_id: &str) -> crate::authority::Authority {
        self.agents.get(session_id).map_or(crate::authority::Authority::Agent, |a| crate::authority::trust(a.authority, self.grants.granted_key(&a.key)))
    }

    /// Whether the identity `key` holds the operator's lead (or owner) grant, whatever role its
    /// hello said (the security monitor never freezes such an identity for a refused try).
    pub(crate) fn holds_lead_grant(&self, key: &str) -> bool {
        self.grants.grant_of_key(key).is_some_and(|g| g >= crate::authority::Authority::Lead)
    }

    /// [`Swarm::authority_of_key`] as the operator's grants alone give it.
    pub(crate) fn trusted_authority_of_key(&self, key: &str) -> crate::authority::Authority {
        crate::authority::trust(self.authority_of_key(key), self.grants.granted_key(key))
    }

    /// The identity a resume token belongs to: its key and the role it has now.
    pub(crate) fn identity_of_token(&self, token: &str) -> Option<(String, String)> {
        self.agents.values().find(|a| a.token == token).map(|a| (a.key.clone(), a.role.clone()))
    }

    /// What the session may do (read through [`crate::authority::authority`]).
    pub(crate) fn authority_of(&self, session_id: &str) -> crate::authority::Authority {
        self.agents.get(session_id).map_or(crate::authority::Authority::Agent, |a| a.authority)
    }

    /// Whether the session said agent_hello.
    pub fn introduced(&self, session_id: &str) -> bool {
        self.agents.contains_key(session_id)
    }

    /// Admits the window's human as an identity (docs/design/EDITOR.md, "The window's operator"):
    /// session `session_id` (never a protocol session id) is `window:owner`, keyed by that name,
    /// with the owner's authority, so the editor's edits go through `Engine::handle` like any
    /// agent's and are journaled, claimed and undone as the human's. Never pruned with dead
    /// sessions; the name stays reserved (no agent may say hello as it).
    pub(crate) fn admit_window_operator(&mut self, session_id: &str) {
        let name = crate::authority::WINDOW_OWNER.to_owned();
        self.reserve(&name);
        let authority = crate::authority::Authority::Owner;
        crate::authority::publish_as(session_id, authority, Some(authority));
        let agent = Agent { name: name.clone(), role: crate::authority::ROLE_OWNER.to_owned(), authority, key: name, token: random_hex() };
        self.agents.insert(session_id.to_owned(), agent);
    }

    /// Admits the operator's own start-up build as an identity (`--run`: the operator said on
    /// the command line that the project's code is theirs to run; [`crate::run_modules`]):
    /// session `session_id` (never a protocol session id) is `operator:run`, keyed by that
    /// name, with the owner's authority, so its builds and loads pass the same gates as an
    /// owner's `logic_build` and are journaled under that name. Admitted only by `--run`. The
    /// name stays reserved (no agent may say hello as it).
    pub(crate) fn admit_run_operator(&mut self, session_id: &str) {
        let name = crate::authority::RUN_OPERATOR.to_owned();
        self.reserve(&name);
        let authority = crate::authority::Authority::Owner;
        crate::authority::publish_as(session_id, authority, Some(authority));
        let agent = Agent { name: name.clone(), role: crate::authority::ROLE_OWNER.to_owned(), authority, key: name, token: random_hex() };
        self.agents.insert(session_id.to_owned(), agent);
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
        if let Some(e) = crate::authority::name_problem(&name) {
            return Err(e);
        }
        if let Some(token) = resume {
            return self.resume(session, name, role, &token);
        }
        // Compared as they look (`authority::skeleton`): `Physics` or `DISK` is the writer's
        // name, and a role word is the role's unless the operator granted that name.
        let sk = crate::authority::skeleton(&name);
        let reserved = self.reserved.iter().any(|r| crate::authority::skeleton(r) == sk) || sk.starts_with(MODULE_PREFIX) || crate::authority::reserved_writer(&name) || self.grants.role_word(&name);
        if reserved {
            return Err(crate::authority::refusal(
                crate::authority::Site::HelloReservedName,
                format!("'{name}' is reserved (the hosted game or the engine writes under it); pick another name"),
            )
            .with("name", name.as_str()));
        }
        // A name is taken by an open session using it, or by a dormant identity that still holds
        // live claims (its owner may come back with the resume token).
        let my_key = self.key(session);
        let now = Instant::now();
        let taken = |n: &str| {
            let sk = crate::authority::skeleton(n);
            self.reserved.contains(n)
                || live.iter().any(|s| s.id != session.id && crate::authority::skeleton(&self.agent_name(s)) == sk)
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
            // Never a grant holder: their bindings outlive their sessions
            // (they come back with their resume tokens).
            let grants = &self.grants;
            self.agents.retain(|id, a| {
                let keep = ids.contains(id.as_str()) || grants.holds(&a.key) || a.key == crate::authority::WINDOW_OWNER || a.key == crate::authority::RUN_OPERATOR;
                if !keep {
                    crate::authority::unpublish(id);
                }
                keep
            });
        }
        // Saying hello again with the same name keeps the identity (and its token).
        let (key, token) = match self.agents.get(&session.id) {
            Some(a) if a.name == name => (a.key.clone(), a.token.clone()),
            // A new identity is keyed by the session that introduced it (unique, and what an
            // unnamed session was already keyed by); a resume carries the key to later sessions.
            _ => (session.id.clone(), random_hex()),
        };
        let authority = self.grants.admit(&name, &key, &role)?;
        let reply = Json::obj([
            ("agent", Json::from(name.as_str())),
            ("role", Json::from(role.as_str())),
            ("authority", Json::from(authority.name())),
            ("resume_token", Json::from(token.as_str())),
            ("note", Json::from("keep resume_token private; a later session continues as this agent (claims and undo included) with agent_hello {name, resume_token}")),
        ]);
        crate::authority::publish_as(&session.id, authority, self.grants.grant_of_key(&key));
        self.agents.insert(session.id.clone(), Agent { name, role, authority, key, token });
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
        let authority = self.grants.admit(&name, &self.agents[&old_sid].key.clone(), &role)?;
        let mut agent = self.agents.remove(&old_sid).expect("found above");
        agent.role = role;
        agent.authority = authority;
        crate::authority::unpublish(&old_sid);
        crate::authority::publish_as(&session.id, authority, self.grants.grant_of_key(&agent.key));
        let reply = Json::obj([
            ("agent", Json::from(agent.name.as_str())),
            ("role", Json::from(agent.role.as_str())),
            ("authority", Json::from(agent.authority.name())),
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

    pub(crate) fn count_rejection(&mut self, w: Writer, e: &WorldError) {
        if matches!(e.code, "claimed" | "claims_overlap") {
            self.claims.rejected += 1;
        }
        // Conflicts a parent wants to hear about (not argument mistakes like a missing entity).
        if matches!(e.code, "claimed" | "claims_overlap" | "stale_version" | "undo_conflict" | "name_taken") {
            if self.refused.len() >= MAX_AGENTS && !self.refused.contains_key(w.name) {
                self.refused.pop_first();
            }
            *self.refused.entry(w.name.to_owned()).or_default().entry(e.code).or_default() += 1;
            self.board.note_refused(w);
        }
    }

    /// Calls and commits per agent name (see [`Usage`]).
    pub fn usage(&self) -> &BTreeMap<String, Usage> {
        &self.usage
    }

    fn usage_of(&mut self, name: &str) -> &mut Usage {
        if self.usage.len() >= MAX_AGENTS && !self.usage.contains_key(name) {
            // The name heard from longest ago makes room (it was the first in the alphabet:
            // review R1 of m5-edswarm, minor 4). A scan of at most MAX_AGENTS names, only when a
            // new name arrives at the cap.
            if let Some(oldest) = self.usage.iter().min_by_key(|(_, u)| u.last).map(|(n, _)| n.clone()) {
                self.usage.remove(&oldest);
            }
        }
        self.usage_clock += 1;
        let at = self.usage_clock;
        let u = self.usage.entry(name.to_owned()).or_default();
        u.last = at;
        u
    }

    /// A tool call by `name` reached the engine.
    pub(crate) fn count_call(&mut self, name: &str) {
        self.usage_of(name).calls += 1;
    }

    /// `n` commits were charged to a call by `name`.
    pub(crate) fn count_commits(&mut self, name: &str, n: u64) {
        if n > 0 {
            self.usage_of(name).commits += n;
        }
    }

    /// Refused writes of `name` by error code (since the engine started).
    pub fn refused_of(&self, name: &str) -> Option<&BTreeMap<&'static str, u64>> {
        self.refused.get(name)
    }

    /// The journal of every world change, to read (history, the change feed, replay). Only the
    /// write policy writes it (its `journal_mut` takes a key nothing else can make): a writer
    /// can't append an entry under another agent's name, undo one, or start the journal over
    /// (review round 4, P5 and P6).
    ///
    /// ```compile_fail
    /// fn forge(sw: &mut specula::swarm::Swarm, w: &specula::world::World) {
    ///     sw.journal().rebase(w, "forged"); // read-only
    /// }
    /// ```
    ///
    /// ```compile_fail
    /// fn forge(sw: &mut specula::swarm::Swarm) {
    ///     let _ = &mut sw.journal; // private
    /// }
    /// ```
    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Before the live world is replaced (a save read keeps the board and the review queue):
    /// every note's, task's and critique's entity records its path as of now, so one the new
    /// world doesn't have shows where its entity last was. References are identities
    /// ([`crate::entity_ref::EntityRef`]), so nothing else needs doing, here or per loop.
    /// O(references x (log n + depth)), once per replacement. Returns how many changed.
    pub fn remember_entity_paths(&mut self, world: &World) -> usize {
        self.board.remember_entity_paths(world) + self.review.remember_entity_paths(world)
    }

    /// After a journaled write (the write policy calls it with every one): references to
    /// entities the new entries deleted take the path each had when it was deleted, so a note on
    /// an entity renamed and then deleted shows where it last was (docs/design/EDITOR.md, "Stable
    /// entity identity"). Cost: one pass over the new entries' changes (a discriminant check
    /// each); only when one deleted a persistent entity, a lookup per delete in the board's and
    /// the queue's own indexes of who names each identity (`entity_ref::Referrers`, kept as
    /// references come and go: nothing is rebuilt here), then a visit to each note, task,
    /// critique and submission that names a deleted one. Returns how many of them changed.
    pub(crate) fn note_deletes(&mut self) -> usize {
        let last = self.journal.last_seq();
        let from = std::mem::replace(&mut self.deletes_seen, last);
        if last <= from {
            return 0;
        }
        let mut deleted: Vec<(crate::world::EntityUid, &str)> = Vec::new();
        for e in self.journal.entries_after(from, true) {
            for c in &e.changes {
                if let Change::Delete { before, path } = c {
                    if !before.transient {
                        deleted.push((before.uid(), path.as_str()));
                    }
                }
            }
        }
        if deleted.is_empty() {
            return 0;
        }
        // A later delete of the same identity (an undo brought it back between) wins.
        let gone: crate::entity_ref::Gone<'_> = deleted.into_iter().filter(|(uid, _)| self.board.names(uid) || self.review.names(uid)).collect();
        if gone.is_empty() {
            return 0;
        }
        self.board.note_deleted(&gone) + self.review.note_deleted(&gone)
    }

    /// The journal to write: the write policy's only (its key, [`crate::write_policy::PolicyKey`],
    /// is one only it can make), so no other code appends, undoes or rebases the engine's journal.
    pub(crate) fn journal_mut(&mut self, _: &crate::write_policy::PolicyKey) -> &mut Journal {
        &mut self.journal
    }

    /// The claims to read and the journal to write at once (the write policy's checks run while
    /// it journals).
    pub(crate) fn claims_and_journal(&mut self, _: &crate::write_policy::PolicyKey) -> (&Claims, &mut Journal) {
        (&self.claims, &mut self.journal)
    }

    /// Tests: the journal to adjust (budgets).
    #[cfg(test)]
    pub(crate) fn journal_for_test(&mut self) -> &mut Journal {
        &mut self.journal
    }

    /// A swarm whose journal starts from `journal` (tests, tools building a detached world).
    pub fn with_journal(journal: Journal) -> Swarm {
        Swarm { journal, ..Swarm::default() }
    }

    /// Refuses `w` acting on what `path` names (a module's root, `/<name>`) while another agent
    /// holds a live claim covering it.
    pub fn check_claimed_path(&self, path: &str, w: Writer, what: &str) -> Result<(), WorldError> {
        self.claims.check_path(path, w, Instant::now(), what)
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
    /// Notes that agent `agent` started `writer` (`module:<name>` loaded, `lex:<set>` started),
    /// for claim notices.
    pub(crate) fn note_starter(&mut self, writer: &str, agent: &str) {
        if self.starters.len() >= MAX_AGENTS && !self.starters.contains_key(writer) {
            self.starters.pop_first();
        }
        self.starters.insert(writer.to_owned(), agent.to_owned());
    }

    /// A module's or live Lex run's own tick (`by`) changed entities another agent holds a claim
    /// on. Ticks are exempt from claims (the game's rules, deterministic), so nothing is refused;
    /// the claim's holder is told instead: a notice per change (at most 16 per call; the changes
    /// past them are counted, unexamined; the newest 64 kept per holder, the rest counted)
    /// naming the writer and the agent who started it, shown in the holder's claim_list. Costs
    /// nothing while nobody holds a claim.
    pub(crate) fn note_claim_crossings(&mut self, world: &World, changes: &[Change], by: &str, tick: u64) {
        let now = Instant::now();
        let w = Writer::new(by, None);
        let started_by = self.starters.get(by).cloned();
        let mut added = 0;
        for (i, c) in changes.iter().enumerate() {
            if added == NOTICES_PER_CALL {
                self.claim_notices_unexamined += (changes.len() - i) as u64;
                break;
            }
            let Err(e) = self.claims.check(world, std::slice::from_ref(c), w, now) else { continue };
            if e.code != "claimed" {
                continue;
            }
            let notice = Json::obj([
                ("tick", Json::from(tick)),
                ("by", Json::from(by)),
                ("started_by", started_by.as_deref().map_or(Json::Null, Json::from)),
                ("path", Json::from(c.path())),
                ("op", Json::from(c.op())),
                ("claim", e.detail("claim").cloned().unwrap_or(Json::Null)),
                ("holder", e.detail("owner").cloned().unwrap_or(Json::Null)),
            ]);
            let holder = e.detail("owner").and_then(Json::as_str).unwrap_or_default().to_owned();
            if self.claim_notices.len() >= MAX_AGENTS && !self.claim_notices.contains_key(&holder) {
                self.claim_notices.pop_first();
            }
            let (kept, dropped) = self.claim_notices.entry(holder).or_default();
            if kept.len() == MAX_CLAIM_NOTICES {
                kept.pop_front();
                *dropped += 1;
            }
            kept.push_back(notice);
            self.claim_notices_total += 1;
            added += 1;
        }
    }

    /// The claim notices for `holder` (everyone's for `None`), newest last, at most `limit`.
    pub fn claim_notices(&self, holder: Option<&str>, limit: usize) -> Json {
        let mine: Vec<&Json> = self.claim_notices.iter().filter(|(h, _)| holder.is_none_or(|x| *h == x)).flat_map(|(_, (kept, _))| kept.iter()).collect();
        Json::arr(mine[mine.len().saturating_sub(limit)..].iter().map(|n| (*n).clone()))
    }

    /// Notices dropped for `holder` past the 64 kept, and changes never examined past the
    /// per-call cap (everyone's).
    pub fn claim_notices_dropped(&self, holder: &str) -> (u64, u64) {
        (self.claim_notices.get(holder).map_or(0, |(_, d)| *d), self.claim_notices_unexamined)
    }

    pub fn stats_json(&self) -> Json {
        Json::obj([
            ("journal", self.journal.stats_json()),
            ("claims", Json::from(self.claims.len())),
            ("claim_rejections", Json::from(self.claims.rejected)),
            ("claim_notices", Json::from(self.claim_notices_total)),
            ("tasks", Json::from(self.board.len())),
            ("notes", Json::from(self.board.notes_len())),
            ("intents", Json::from(self.intents.len())),
            ("leases", Json::from(self.leases.len())),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claims::{Scope, DEFAULT_LEASE};
    use crate::journal::{Kind, UndoRequest};
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

    /// Integrator, round 4: the world's record of removed entities' versions stays bounded by
    /// what the journal can still restore (it held every id ever deleted: 10M deletes, about
    /// 0.5 GB), while an undo within the ring still restores past the entity's last version.
    #[test]
    fn removed_versions_stay_bounded_by_the_journal() {
        let mut w = world();
        let mut sw = Swarm { journal: Journal::with_limits(&w, "empty", 400, 64 << 20), ..Swarm::default() };
        let me = Writer { name: "churn", session: None, via: None };
        let churn = |sw: &mut Swarm, w: &mut World, n: usize, tag: &str| {
            for i in 0..n {
                let id = sw.commit(w, 0, me, Kind::Spawn, |w| w.spawn(spec(&format!("/{tag}{i}")), "churn").map(|e| e.id)).unwrap().0;
                sw.commit(w, 0, me, Kind::Delete, |w| w.delete(&EntityKey::Id(id), None).map(|_| ())).unwrap();
            }
        };
        // 20,000 deletes through a ring of 400 entries (200 spawn + delete pairs).
        let mut most = 0;
        for round in 0..20 {
            churn(&mut sw, &mut w, 1000, &format!("c{round}_"));
            most = most.max(w.retired_len());
        }
        let ring_ids = sw.journal.ids_in_ring().len();
        assert!(ring_ids <= 200, "{ring_ids}");
        assert!(most <= 2 * RETIRED_PRUNE_MIN.max(2 * ring_ids) + 1, "kept {most} removed versions for {ring_ids} ids in the ring");
        // An undo within the ring: a box set twice, deleted, then its first set force-undone.
        let id = sw.commit(&mut w, 0, me, Kind::Spawn, |w| w.spawn(spec("/a"), "churn").map(|e| e.id)).unwrap().0;
        let set = |sw: &mut Swarm, w: &mut World, x: f32| {
            sw.commit(w, 0, me, Kind::Set, |w| w.update(&EntityKey::Id(id), UpdateSpec { position: Some(specula_core::math::Vec3::new(x, 0.0, 0.0)), ..UpdateSpec::default() }, "churn").map(|e| e.version)).unwrap()
        };
        let (_, first) = set(&mut sw, &mut w, 1.0);
        let (last, _) = set(&mut sw, &mut w, 2.0);
        sw.commit(&mut w, 0, me, Kind::Delete, |w| w.delete(&EntityKey::Id(id), None).map(|_| ())).unwrap();
        // Other churn, still within the ring, then a prune: the box's version is kept.
        churn(&mut sw, &mut w, 50, "late");
        sw.prune_retired_now(&mut w);
        assert!(w.retired_len() <= sw.journal.ids_in_ring().len());
        assert!(w.version_floor(id) >= last, "the box's last version survived the prune");
        sw.undo(&mut w, 0, me, &UndoRequest { ids: Some(vec![first.unwrap()]), count: 1, force: true }).unwrap();
        assert!(w.get(id).unwrap().version > last, "restored past its last version");
    }

    /// Review R1 of m5-leverage: the security monitor has to know that a session's identity is
    /// the lead the operator named even when its hello said a job (it froze such a lead for its
    /// first refused call). The hello publishes the grant its name holds, apart from the
    /// authority it was admitted with; a reviewer's grant is not a lead's, and a name nobody
    /// granted has none.
    #[test]
    fn a_hello_publishes_the_grant_its_name_holds_whatever_role_it_said() {
        use crate::authority::Authority;
        let (lee, rev, ann) = (session("lg-lee-1", "x"), session("lg-rev-1", "x"), session("lg-ann-1", "x"));
        let live = vec![lee.clone(), rev.clone(), ann.clone()];
        let mut sw = Swarm::default();
        sw.grants.grant("lee", Authority::Lead).unwrap();
        sw.grants.grant("rev", Authority::Reviewer).unwrap();
        for (s, name) in [(&lee, "lee"), (&rev, "rev"), (&ann, "ann")] {
            sw.hello(s, name.into(), "builder".into(), None, &live).unwrap();
            assert_eq!(crate::authority::authority(&sw, &s.id), Authority::Agent, "{name} said a job: a plain agent in what it may do");
        }
        assert!(sw.holds_lead_grant(&sw.key(&lee)) && crate::authority::lead_grant_of_session(&lee.id));
        assert!(!sw.holds_lead_grant(&sw.key(&rev)) && !crate::authority::lead_grant_of_session(&rev.id), "a reviewer's grant is not a lead's");
        assert!(!sw.holds_lead_grant(&sw.key(&ann)) && !crate::authority::lead_grant_of_session(&ann.id));
        // The monitor's actor for each carries it.
        let actor = |s: &SessionInfo| crate::inputguard::Actor::of(&sw, s);
        assert_eq!(actor(&lee), crate::inputguard::Actor::keyed(&lee.id, &sw.key(&lee), "lee", Authority::Agent).granted_lead(true));
        assert_eq!(actor(&ann), crate::inputguard::Actor::keyed(&ann.id, &sw.key(&ann), "ann", Authority::Agent));
        // Saying the role: the lead's authority, and it may load native code; before, it may not.
        assert!(sw.may_load_native(&sw.key(&lee), "logic_build").unwrap_err().message.contains("--role lead"));
        sw.hello(&lee, "lee".into(), "lead".into(), None, &live).unwrap();
        assert_eq!(crate::authority::authority(&sw, &lee.id), Authority::Lead);
        assert!(sw.may_load_native(&sw.key(&lee), "logic_build").is_ok());
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
        let tool = |w: &mut crate::game::GameWorld| {
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

    /// Audit: game tools (and module loads and hooks, through the same check) skipped the
    /// reserved-data rules; they hold for them as for agents, with no claims in play.
    #[test]
    fn game_tools_cannot_write_reserved_data() {
        let mut sw = Swarm::default();
        let mut w = world();
        let bob = Writer::new("bob", Some("bbbbbb22"));
        for path in ["/lex", "/lex_live", "/project", "/nav"] {
            let err = sw.game_tool(&mut w, 1, "sepulcra", bob, |w| w.spawn(spec(path), "game").map(|_| ()).unwrap()).unwrap_err();
            assert_eq!((err.code, err.detail("game")), ("reserved", Some(&Json::from("sepulcra"))), "{path}");
            assert!(w.find_path(path).is_err() && w.pending_changes().is_empty(), "{path} rolled back");
        }
        assert!(sw.game_tool(&mut w, 1, "sepulcra", bob, |w| w.spawn(spec("/tower"), "game").map(|_| ()).unwrap()).is_ok());
        assert_eq!(sw.journal.replay().unwrap().state_hash(), w.state_hash(), "ids the refused spawns used are noted");
    }

    /// The M4 authority findings, as rows: each must stay refused.
    #[test]
    fn authority_is_never_self_declared() {
        use crate::authority::{authority, Authority, ROLE_DIRECTOR, ROLE_LEAD, ROLE_OWNER, ROLE_REVIEWER};
        let (sa, sb, sc, sd, sb2) = (session("a", "c"), session("b", "c"), session("c", "c"), session("d", "c"), session("b2", "c"));
        let live = vec![sa.clone(), sb.clone(), sc.clone(), sd.clone(), sb2.clone()];
        let mut sw = Swarm::default();
        // carl said role owner (and approved bob's submission).
        let e = sw.hello(&sc, "carl".into(), ROLE_OWNER.into(), None, &live).unwrap_err();
        assert_eq!((e.code, e.detail("refused_at")), ("role_not_granted", Some(&Json::from("agent_hello.role"))));
        assert!(!sw.introduced("c"), "a refused hello names nobody");
        // Nobody becomes lead by asking, first or not (owner, 2026-11-12: no fallback lead).
        assert_eq!(sw.hello(&sa, "ann".into(), ROLE_LEAD.into(), None, &live).unwrap_err().code, "role_not_granted");
        assert_eq!(authority(&sw, "a"), Authority::Agent);
        assert_eq!(sw.hello(&sd, "dave".into(), ROLE_DIRECTOR.into(), None, &live).unwrap_err().code, "role_not_granted");
        sw.hello(&sa, "ann".into(), "builder".into(), None, &live).unwrap();
        // bob approved his own submission from a second identity with role reviewer.
        sw.hello(&sb, "bob".into(), "builder".into(), None, &live).unwrap();
        assert_eq!(sw.hello(&sb2, "bob2".into(), ROLE_REVIEWER.into(), None, &live).unwrap_err().code, "role_not_granted");
        assert_eq!(authority(&sw, "b"), Authority::Agent);
        // A job label carries nothing.
        let reply = sw.hello(&sd, "dave".into(), "playtester".into(), None, &live).unwrap();
        assert_eq!(reply.get("authority"), Some(&Json::from("agent")));
    }

    #[test]
    fn operator_grants_are_the_way_in_and_resumes_keep_them() {
        use crate::authority::{authority, Authority, ROLE_LEAD, ROLE_OWNER, ROLE_REVIEWER};
        let (sa, sa2, sb, sx) = (session("a", "c"), session("a2", "c"), session("b", "c"), session("x", "c"));
        let mut sw = Swarm::default();
        sw.grants.grant("rev", Authority::Reviewer).unwrap();
        sw.grants.grant("boss", Authority::Owner).unwrap();
        let live = vec![sa.clone(), sb.clone()];
        let token = sw.hello(&sa, "rev".into(), ROLE_REVIEWER.into(), None, &live).unwrap().get("resume_token").and_then(Json::as_str).unwrap().to_owned();
        assert_eq!(authority(&sw, "a"), Authority::Reviewer);
        sw.hello(&sb, "boss".into(), ROLE_OWNER.into(), None, &live).unwrap();
        assert_eq!(authority(&sw, "b"), Authority::Owner);
        // Another session can't take the granted name, even after its session ends.
        let later = vec![sx.clone(), sa2.clone()];
        assert_eq!(sw.hello(&sx, "rev".into(), "builder".into(), None, &later).unwrap_err().code, "grant_bound");
        // Its resume token carries the grant.
        sw.hello(&sa2, "rev".into(), ROLE_REVIEWER.into(), Some(token), &later).unwrap();
        assert_eq!(authority(&sw, "a2"), Authority::Reviewer);
        // With a lead granted, someone else asking still gets nothing.
        sw.grants.grant("lee", Authority::Lead).unwrap();
        assert_eq!(sw.hello(&sx, "dave".into(), ROLE_LEAD.into(), None, &later).unwrap_err().code, "role_not_granted");
    }

    #[test]
    fn engine_writer_names_are_refused() {
        use crate::write_policy::Exempt;
        let s = session("a", "c");
        let live = vec![s.clone()];
        let mut sw = Swarm::default();
        sw.reserve("sepulcra");
        // Every Exempt writer's name (a new variant doesn't compile until it is listed here).
        let all = [
            Exempt::Game { name: "sepulcra", kind: Kind::GameTick },
            Exempt::GameStart { name: "sepulcra" },
            Exempt::Module { agent: "module:ambulo", kind: Kind::GameTick },
            Exempt::LexLive { agent: "lex:waves" },
            Exempt::Physics,
            Exempt::Nav,
            Exempt::AnimReload,
        ];
        for why in &all {
            match why {
                Exempt::Game { .. } | Exempt::GameStart { .. } | Exempt::Module { .. } | Exempt::LexLive { .. } | Exempt::Physics | Exempt::Nav | Exempt::AnimReload => {}
            }
            let e = sw.hello(&s, why.by().to_owned(), "builder".into(), None, &live).unwrap_err();
            assert!(matches!(e.code, "name_reserved" | "name_invalid"), "{}: {}", why.by(), e.code);
        }
        for n in ["disk", "engine:stray", "engine:x", "window:owner", "lex:anything", "module:x"] {
            assert!(matches!(sw.hello(&s, n.into(), "builder".into(), None, &live).unwrap_err().code, "name_reserved" | "name_invalid"), "{n}");
        }
        assert!(sw.hello(&s, "physicist".into(), "builder".into(), None, &live).is_ok());
    }

    /// Review of the authority lane (bf57c7f), probes as rows.
    #[test]
    fn grants_survive_pruning_and_bind_only_on_success() {
        use crate::authority::{authority, Authority, ROLE_LEAD, ROLE_OWNER, ROLE_REVIEWER};
        // Pruning at MAX_AGENTS kept the binding but dropped the granted identity: locked out.
        let mut sw = Swarm::default();
        sw.grants.grant("boss", Authority::Owner).unwrap();
        let sb = session("boss-session", "c");
        let tok = sw.hello(&sb, "boss".into(), ROLE_OWNER.into(), None, std::slice::from_ref(&sb)).unwrap().get("resume_token").and_then(Json::as_str).unwrap().to_owned();
        for i in 0..(MAX_AGENTS + 80) {
            let s = session(&format!("s{i:05}"), "c");
            let _ = sw.hello(&s, format!("a{i}"), "builder".into(), None, std::slice::from_ref(&s));
        }
        let sb2 = session("boss-later", "c");
        sw.hello(&sb2, "boss".into(), ROLE_OWNER.into(), Some(tok), std::slice::from_ref(&sb2)).expect("the granted identity survives pruning");
        assert_eq!(authority(&sw, "boss-later"), Authority::Owner);
        // The same for a granted lead.
        let mut sw = Swarm::default();
        sw.grants.grant("lee", Authority::Lead).unwrap();
        let sl = session("lead-session", "c");
        let tok = sw.hello(&sl, "lee".into(), ROLE_LEAD.into(), None, std::slice::from_ref(&sl)).unwrap().get("resume_token").and_then(Json::as_str).unwrap().to_owned();
        for i in 0..(MAX_AGENTS + 80) {
            let s = session(&format!("s{i:05}"), "c");
            let _ = sw.hello(&s, format!("a{i}"), "builder".into(), None, std::slice::from_ref(&s));
        }
        let s2 = session("lead-later", "c");
        sw.hello(&s2, "lee".into(), ROLE_LEAD.into(), Some(tok), std::slice::from_ref(&s2)).expect("the granted lead survives pruning");
        // A refused hello bound the granted name to the refused session.
        let mut sw = Swarm::default();
        sw.grants.grant("rev", Authority::Reviewer).unwrap();
        sw.grants.grant("lee", Authority::Lead).unwrap();
        let sx = session("rev-first-try", "c");
        assert_eq!(sw.hello(&sx, "rev".into(), ROLE_LEAD.into(), None, std::slice::from_ref(&sx)).unwrap_err().code, "role_not_granted");
        assert!(!sw.introduced("rev-first-try"));
        let sr = session("real-rev", "c");
        sw.hello(&sr, "rev".into(), ROLE_REVIEWER.into(), None, std::slice::from_ref(&sr)).expect("the refused hello bound nothing");
        assert_eq!(authority(&sw, "real-rev"), Authority::Reviewer);
    }

    #[test]
    fn look_alike_and_role_word_names_are_refused() {
        let mut sw = Swarm::default();
        sw.reserve("sepulcra");
        for (i, n) in ["Physics", "DISK", "NAV", "Anim_Reload", "Sepulcra", "owner", "Lead", "REVIEWER", "director"].iter().enumerate() {
            let s = session(&format!("la{i}"), "c");
            let e = sw.hello(&s, (*n).into(), "builder".into(), None, std::slice::from_ref(&s)).unwrap_err();
            assert_eq!(e.code, "name_reserved", "{n:?}");
        }
        // Outside the name class (ASCII letters, digits, space _ . -): look-alikes, colons
        // (so no writer prefix), marks, invisible characters (review round 2's probes).
        let outside = [
            "phys\u{0456}cs", "\u{0440}hysics", "\u{ff50}hysics", "Engine:stray", "W\u{0456}ndow:owner", "Module:x", "LEX:waves", "s\u{0435}pulcra",
            "module\u{2236}x", "engine\u{A789}stray", "lex\u{02D0}waves", "engine\u{FE55}x", "module\u{FF1A}x", "lex\u{FE13}x", "engine\u{02F8}x",
            "\u{1D429}\u{1D421}\u{1D432}\u{1D42C}\u{1D422}\u{1D41C}\u{1D42C}", "phys\u{0131}cs", "physi\u{0307}cs", "na\u{0301}v", "\u{217E}isk", "physic\u{A731}",
            "nav\u{0336}", "na\u{200D}v", "nav\u{2067}", "\u{202E}van", "n\u{0251}v", "l\u{0435}ad", "\u{217C}ead", "\u{24DB}ead", "jose\u{301}",
            "physics ", " nav", "disk\u{200b}", "an\u{2060}n", "bo\u{feff}b", "x\u{202e}y", "a/b", "",
        ];
        for (i, n) in outside.iter().enumerate() {
            let s = session(&format!("inv{i}"), "c");
            let e = sw.hello(&s, (*n).into(), "builder".into(), None, std::slice::from_ref(&s)).unwrap_err();
            assert_eq!(e.code, "name_invalid", "{n:?}");
            assert!(crate::authority::name_problem(n).is_some());
        }
        for n in ["ann", "agent_00", "pt-starter", "builder b", "v1.2"] {
            assert!(crate::authority::name_problem(n).is_none(), "{n:?}");
        }
        // A look-alike of a live agent's name is that name.
        let (sa, sb) = (session("a", "c"), session("b", "c"));
        let live = vec![sa.clone(), sb.clone()];
        sw.hello(&sa, "ann".into(), "builder".into(), None, &live).unwrap();
        assert_eq!(sw.hello(&sb, "\u{0430}nn".into(), "builder".into(), None, &live).unwrap_err().code, "name_invalid");
        assert_eq!(sw.hello(&sb, "ANN".into(), "builder".into(), None, &live).unwrap_err().code, "name_taken");
        // A granted role word is fine.
        let mut sw = Swarm::default();
        sw.grants.grant("lead", crate::authority::Authority::Lead).unwrap();
        let s = session("l", "c");
        assert!(sw.hello(&s, "lead".into(), crate::authority::ROLE_LEAD.into(), None, std::slice::from_ref(&s)).is_ok());
        // Role words in any case ask for the role.
        let s2 = session("c2", "c");
        assert_eq!(sw.hello(&s2, "casey".into(), "LEAD".into(), None, std::slice::from_ref(&s2)).unwrap_err().code, "role_not_granted");
    }

    #[test]
    fn asking_for_lead_gets_nothing_beside_another_grant() {
        use crate::authority::{authority, Authority, ROLE_LEAD, ROLE_REVIEWER};
        let mut sw = Swarm::default();
        sw.grants.grant("rev", Authority::Reviewer).unwrap();
        let (sa, sr) = (session("a", "c"), session("r", "c"));
        let live = vec![sa.clone(), sr.clone()];
        // Coordinator, 2026-10-29: with --reviewer and no --lead, the first lead hello isn't lead.
        assert_eq!(sw.hello(&sa, "ann".into(), ROLE_LEAD.into(), None, &live).unwrap_err().code, "role_not_granted");
        sw.hello(&sr, "rev".into(), ROLE_REVIEWER.into(), None, &live).unwrap();
        assert_eq!(authority(&sw, "r"), Authority::Reviewer);
    }

    /// Caller-thread tools read the same authority (review round 2: the tracing and galleries
    /// tools can't reach `&Swarm`).
    #[test]
    fn caller_threads_read_the_admitted_authority() {
        use crate::authority::{of_session, Authority, ROLE_REVIEWER};
        let mut sw = Swarm::default();
        sw.grants.grant("rev", Authority::Reviewer).unwrap();
        let (s1, s2) = (session("pub-rev-1", "c"), session("pub-rev-2", "c"));
        let tok = sw.hello(&s1, "rev".into(), ROLE_REVIEWER.into(), None, std::slice::from_ref(&s1)).unwrap().get("resume_token").and_then(Json::as_str).unwrap().to_owned();
        assert_eq!(of_session("pub-rev-1"), Authority::Reviewer);
        assert_eq!(of_session("pub-nobody"), Authority::Agent);
        assert_eq!(crate::authority::trusted_of_session("pub-rev-1"), Authority::Reviewer, "a grant is trusted");
        sw.hello(&s2, "rev".into(), ROLE_REVIEWER.into(), Some(tok), std::slice::from_ref(&s2)).unwrap();
        assert_eq!((of_session("pub-rev-1"), of_session("pub-rev-2")), (Authority::Agent, Authority::Reviewer), "a resume moves it");
        sw.hello(&s2, "rev".into(), "builder".into(), None, std::slice::from_ref(&s2)).unwrap();
        assert_eq!(of_session("pub-rev-2"), Authority::Agent, "a plain role carries none");
    }

    /// Review R1 of m5-edswarm, M1: one delete commit walked every note, task, critique and
    /// submission whenever the board had changed since the last delete (and every commit by a
    /// task's holder changes it): 69-77 ms on the engine thread at the board's own caps. A
    /// delete now visits only what names the deleted entity. The measure is a count of holders
    /// visited, not a time: it is the same on any machine, and whatever the board holds.
    #[test]
    fn a_delete_visits_only_what_names_the_deleted_entity_whatever_the_board_holds() {
        use crate::board::{NoteTarget, TaskSpec};
        use crate::entity_ref::EntityRef;
        use crate::world::EntityUid;
        let mut world = World::default();
        world.set_recording(Recording::Full);
        let mut swarm = Swarm::default();
        let w = Writer::new("ann", None);
        // A board full of references to other entities: 600 tasks, 16 each.
        let mut n = 0u64;
        for t in 0..600 {
            let entities = (0..16)
                .map(|_| {
                    n += 1;
                    EntityRef { uid: EntityUid { lineage: 7, id: 1_000_000 + n }, path: format!("/x/e{n}") }
                })
                .collect();
            swarm.board.post(w, 0, TaskSpec { title: format!("t{t}"), entities, ..TaskSpec::default() }).unwrap();
        }
        let spawn = |swarm: &mut Swarm, world: &mut World, name: &str| {
            let mut s = SpawnSpec::new(Shape::Group);
            s.name = Some(name.into());
            swarm.commit(world, 0, w, Kind::Spawn, |wd| wd.spawn(s, "ann").map(|e| e.id)).unwrap().0
        };
        let (a, b) = (spawn(&mut swarm, &mut world, "a"), spawn(&mut swarm, &mut world, "b"));
        let on_a = EntityRef::of(&world, a).unwrap();
        let task = swarm.board.post(w, 0, TaskSpec { title: "about a".into(), entities: vec![on_a.clone()], ..TaskSpec::default() }).unwrap().id;
        swarm.board.add_note(w, 0, NoteTarget::Entity(on_a), "look".into()).unwrap();
        assert_eq!(swarm.board.len(), 601);
        swarm.board.take_ref_visits();
        // The board changes before each delete, as every commit by a task's holder changes it.
        swarm.board.log("ann", 0, "link", Some(task), "x".into());
        swarm.commit(&mut world, 1, w, Kind::Delete, |wd| wd.delete(&EntityKey::Id(b), None).map(|_| ())).unwrap();
        assert_eq!(swarm.board.take_ref_visits() + swarm.review.take_ref_visits(), 0, "an entity nothing names: nothing is visited");
        swarm.board.log("ann", 0, "link", Some(task), "x".into());
        swarm.commit(&mut world, 2, w, Kind::Delete, |wd| wd.delete(&EntityKey::Id(a), None).map(|_| ())).unwrap();
        assert_eq!((swarm.board.take_ref_visits(), swarm.review.take_ref_visits()), (2, 0), "its task and its note, of 601 tasks");
        // Dropped holders leave the index: nothing names `a` once its task and note are gone...
        assert!(swarm.board.names(&EntityUid { lineage: 7, id: 1_000_001 }), "a task still names this one");
    }

    /// Review R1 of m5-edswarm, minor 4: past its cap the cost view dropped the first name in
    /// the alphabet, however busy it was. The name heard from longest ago makes room.
    #[test]
    fn the_cost_views_oldest_name_makes_room() {
        let mut sw = Swarm::default();
        sw.count_call("aaa");
        for i in 0..MAX_AGENTS - 1 {
            sw.count_call(&format!("n{i:05}"));
        }
        // Heard from again: the newest now, though first in the alphabet.
        sw.count_call("aaa");
        sw.count_call("zzz");
        assert_eq!(sw.usage().len(), MAX_AGENTS);
        assert!(sw.usage().contains_key("aaa") && sw.usage().contains_key("zzz"), "the busy name and the new one stay");
        assert!(!sw.usage().contains_key("n00000"), "the one heard from longest ago went");
        assert_eq!(sw.usage()["aaa"].calls, 2);
    }
}

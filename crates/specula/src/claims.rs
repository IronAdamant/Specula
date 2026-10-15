//! Claims: leases agents take on part of the world so a swarm can build side by side safely.
//!
//! A claim covers a **subtree path** (`/district_3` and everything below it; `/` is the whole
//! world, environment included) or a **world-space region** (an axis-aligned box; an entity is
//! inside when its world origin is, `min <= p < max`). It belongs to an agent **name and the
//! session** that took it (see [`Writer`]): names are unique among live sessions, and a name
//! taken again after its session ended adopts nothing. Leases run for a time limit the owner
//! renews.
//!
//! - **Writes inside another agent's claim are rejected** with a structured `claimed` error naming
//!   the owner, the claim and when it expires — unless the owner shared the claim with the writer
//!   (`claim_share`, bound to the sharee's current session). Unclaimed space is open to everyone.
//!   A write is checked on everything it changed: the entity's path before and after (renames and
//!   moves), its world origin before and after, and — when the entity has descendants — every
//!   claim *below* its old and new path (renaming or moving `/a` would carry the claimed `/a/b`
//!   out of its claim, or carry foreign entities into one) and the descendants' origins before
//!   and after a move. A delete touches every entity it removes, so deleting `/a` is rejected
//!   while someone holds `/a/b`, and so is moving, rotating or scaling `/a` (it would move or
//!   shrink away what `/a/b` holds). Undo and game tools an agent calls are checked the same way.
//!   Commands of several steps (a batch, an undo of several entries, a game tool) are checked
//!   **step by step**, each change against the world as it was right after it (see
//!   `World::recheck_steps`): a later step can't hide what an earlier one did, such as a rename
//!   followed by a delete, or two moves in a row. A rejected command is rolled back whole; the
//!   ids it used are never handed out again.
//! - **Claims don't overlap** between different owners (even co-claimed ones), so an owner is
//!   never locked out of its own claim. *Limitation:* a path claim and a region claim are
//!   compared through the entities that exist when the second is taken; if they meet later (the
//!   path owner builds into the region, or the region holds the path's subtree), writes where
//!   they meet are rejected for both owners with `claims_overlap`, naming both claims: resolve it
//!   with `claim_share` or by releasing one.
//! - **Expiry.** A claim ends when its lease runs out or its owner releases it. A claim taken by an
//!   introduced agent survives its session (the agent can resume with its `resume_token`, see
//!   `swarm`); shares bound to a session end with it.
//!   Expired claims never block.
//! - **Live coordination only:** claims are not saved with the world. `world_load` is refused
//!   while another agent holds a live claim (it would replace their work). Wall-clock time is fine
//!   here: claims never affect simulation state.
//! - **The hosted game's own ticks are exempt**: they are the simulation's rules and are never
//!   checked, and the game holds no claims. Game tools an agent calls are checked as that agent.

use crate::journal::{path_under, Writer};
use crate::world::{validate_name, vec3_json, Change, World, WorldError};
use specula_core::math::Vec3;
use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// Lease when none is given, and the range allowed.
pub const DEFAULT_LEASE: Duration = Duration::from_secs(300);
pub const MIN_LEASE_S: u64 = 5;
pub const MAX_LEASE_S: u64 = 3600;
/// Claims one agent may hold, and claims in all.
pub const MAX_PER_AGENT: usize = 64;
pub const MAX_CLAIMS: usize = 4096;
/// Agents a claim may be shared with.
pub const MAX_SHARED: usize = 32;
/// claim_list stops adding claims past this many bytes and hands back a cursor.
const MAX_LIST_BYTES: usize = 64 * 1024;
/// Claims named in an error.
const MAX_NAMED: usize = 5;

/// What a claim covers.
#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    /// A subtree: this path and everything below it (`/` = the whole world).
    Path(String),
    /// World-space box, `min <= p < max` on every axis.
    Region { min: Vec3, max: Vec3 },
}

impl Scope {
    /// Validates a claimed path: `/`, or `/a/b` with valid names.
    pub fn path(path: &str) -> Result<Scope, String> {
        if path == "/" {
            return Ok(Scope::Path("/".into()));
        }
        let rest = path.strip_prefix('/').ok_or("'path' must start with '/', like \"/district_3\"")?;
        for name in rest.split('/') {
            validate_name(name).map_err(|e| format!("'path': {} (in {path:?})", e.message))?;
        }
        Ok(Scope::Path(path.to_owned()))
    }

    pub fn region(min: Vec3, max: Vec3) -> Result<Scope, String> {
        let finite = |v: Vec3| [v.x, v.y, v.z].iter().all(|c| c.is_finite());
        if !finite(min) || !finite(max) || !(min.x < max.x && min.y < max.y && min.z < max.z) {
            return Err("'region' must be {min: [x,y,z], max: [x,y,z]} with min < max on every axis".into());
        }
        Ok(Scope::Region { min, max })
    }

    fn contains_point(&self, p: Vec3) -> bool {
        match self {
            Scope::Region { min, max } => {
                p.x >= min.x && p.x < max.x && p.y >= min.y && p.y < max.y && p.z >= min.z && p.z < max.z
            }
            Scope::Path(_) => false,
        }
    }

    fn json(&self) -> (&'static str, Json) {
        match self {
            Scope::Path(p) => ("path", Json::from(p.as_str())),
            Scope::Region { min, max } => ("region", Json::obj([("min", vec3_json(*min)), ("max", vec3_json(*max))])),
        }
    }

    fn describe(&self) -> String {
        match self {
            Scope::Path(p) => p.clone(),
            Scope::Region { min, max } => {
                format!("region [{}, {}, {}]..[{}, {}, {}]", min.x, min.y, min.z, max.x, max.y, max.z)
            }
        }
    }
}

/// What [`Claims::take`] asks for.
#[derive(Clone, Debug)]
pub struct ClaimSpec {
    pub scope: Scope,
    pub lease: Duration,
    /// What the owner is doing there (shown in listings).
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct Claim {
    pub id: u64,
    /// Agent name.
    pub owner: String,
    /// The protocol session that took it: the claim is that session's alone.
    pub session: String,
    pub scope: Scope,
    /// Agents allowed to write inside it too: name → the session it was shared with.
    pub shared: BTreeMap<String, String>,
    pub expires: Instant,
    pub note: String,
}

fn secs_left(expires: Instant, now: Instant) -> Json {
    Json::from((expires.saturating_duration_since(now).as_secs_f64() * 10.0).round() / 10.0)
}

impl Claim {
    pub fn to_json(&self, now: Instant) -> Json {
        let mut f = vec![("id", Json::from(self.id)), ("owner", Json::from(self.owner.as_str())), self.scope.json()];
        f.push(("expires_in_s", secs_left(self.expires, now)));
        if !self.shared.is_empty() {
            f.push(("shared", Json::arr(self.shared.keys().map(|s| Json::from(s.as_str())))));
        }
        if !self.note.is_empty() {
            f.push(("note", Json::from(self.note.as_str())));
        }
        Json::obj(f)
    }

    /// Whether `w` holds this claim (same name and session).
    pub fn held_by(&self, w: Writer) -> bool {
        self.owner == w.name && w.session == Some(self.session.as_str())
    }

    fn lets(&self, w: Writer, now: Instant) -> bool {
        self.expires <= now
            || self.held_by(w)
            || self.shared.get(w.name).is_some_and(|s| w.session == Some(s.as_str()))
    }
}

/// Every live claim.
#[derive(Default)]
pub struct Claims {
    claims: BTreeMap<u64, Claim>,
    /// Path → claims on exactly that path.
    paths: BTreeMap<String, BTreeSet<u64>>,
    regions: BTreeSet<u64>,
    next_id: u64,
    /// Writes rejected so far (for engine_info and drills).
    pub rejected: u64,
}

fn claim_error(code: &'static str, c: &Claim, now: Instant, message: String) -> WorldError {
    let (key, scope) = c.scope.json();
    WorldError::new(code, message)
        .with("claim", c.id)
        .with("owner", c.owner.as_str())
        .with(key, scope)
        .with("expires_in_s", secs_left(c.expires, now))
}

impl Claims {
    pub fn len(&self) -> usize {
        self.claims.len()
    }

    pub fn is_empty(&self) -> bool {
        self.claims.is_empty()
    }

    pub fn get(&self, id: u64) -> Option<&Claim> {
        self.claims.get(&id)
    }

    /// Claims not yet expired at `now`, in id order (for sight's claims overlay).
    pub fn live(&self, now: Instant) -> impl Iterator<Item = &Claim> + '_ {
        self.claims.values().filter(move |c| c.expires > now)
    }

    fn by_ids<'a>(&'a self, ids: &'a BTreeSet<u64>) -> impl Iterator<Item = &'a Claim> + 'a {
        ids.iter().filter_map(|id| self.claims.get(id))
    }

    /// Path claims covering `path` (on it or an ancestor, or `/`).
    fn covering<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a Claim> + 'a {
        let mut prefixes: Vec<&str> = vec!["/"];
        if path != "/" {
            prefixes.extend(path.char_indices().filter(|&(i, c)| c == '/' && i > 0).map(|(i, _)| &path[..i]));
            prefixes.push(path);
        }
        prefixes.into_iter().filter_map(|p| self.paths.get(p)).flat_map(|ids| self.by_ids(ids))
    }

    /// Path claims strictly below `path`.
    fn below<'a>(&'a self, path: &str) -> impl Iterator<Item = &'a Claim> + 'a {
        let prefix = if path == "/" { "/".to_owned() } else { format!("{path}/") };
        self.paths
            .range(prefix.clone()..)
            .take_while(move |(k, _)| k.starts_with(&prefix))
            .flat_map(|(_, ids)| self.by_ids(ids))
    }

    fn regions(&self) -> impl Iterator<Item = &Claim> + '_ {
        self.by_ids(&self.regions)
    }

    /// Whether `a` and `b` (different owners' claims) cover any of the same world.
    fn overlap(&self, world: &World, a: &Scope, b: &Scope) -> bool {
        match (a, b) {
            (Scope::Path(x), Scope::Path(y)) => path_under(x, y) || path_under(y, x),
            (Scope::Region { min: a0, max: a1 }, Scope::Region { min: b0, max: b1 }) => {
                a0.x < b1.x && b0.x < a1.x && a0.y < b1.y && b0.y < a1.y && a0.z < b1.z && b0.z < a1.z
            }
            (Scope::Path(p), r @ Scope::Region { .. }) | (r @ Scope::Region { .. }, Scope::Path(p)) => {
                // Overlap where an entity under the path already sits in the region.
                let root = if p == "/" { None } else { world.find_path(p).ok() };
                if p != "/" && root.is_none() {
                    return false;
                }
                let mut hit = false;
                world.walk(root, |e, _| hit = hit || r.contains_point(e.world_position()));
                hit
            }
        }
    }

    /// Takes a claim for `owner` in `session`. Rejected (`claim_conflict`) when it overlaps
    /// another agent's live claim.
    pub fn take(&mut self, world: &World, owner: &str, session: &str, spec: ClaimSpec, now: Instant) -> Result<&Claim, WorldError> {
        let ClaimSpec { scope, lease, note } = spec;
        self.expire(now, None);
        let me = Writer::new(owner, Some(session));
        let mine = self.claims.values().filter(|c| c.held_by(me)).count();
        if mine >= MAX_PER_AGENT || self.claims.len() >= MAX_CLAIMS {
            return Err(WorldError::new(
                "too_many_claims",
                format!("at most {MAX_PER_AGENT} claims per agent and {MAX_CLAIMS} in all; release some first"),
            )
            .with("yours", mine)
            .with("total", self.claims.len()));
        }
        if let Some(other) = self.claims.values().find(|c| !c.held_by(me) && self.overlap(world, &scope, &c.scope)) {
            return Err(claim_error(
                "claim_conflict",
                other,
                now,
                format!(
                    "{} overlaps {}'s claim {} on {}; pick another area, or ask them to claim_share theirs with you",
                    scope.describe(),
                    other.owner,
                    other.id,
                    other.scope.describe()
                ),
            ));
        }
        self.next_id += 1;
        let id = self.next_id;
        match &scope {
            Scope::Path(p) => {
                self.paths.entry(p.clone()).or_default().insert(id);
            }
            Scope::Region { .. } => {
                self.regions.insert(id);
            }
        }
        let claim = Claim {
            id,
            owner: owner.to_owned(),
            session: session.to_owned(),
            scope,
            shared: BTreeMap::new(),
            expires: now + lease,
            note,
        };
        self.claims.insert(id, claim);
        Ok(&self.claims[&id])
    }

    fn owned_mut(&mut self, id: u64, owner: Writer, now: Instant) -> Result<&mut Claim, WorldError> {
        let Some(c) = self.claims.get_mut(&id).filter(|c| c.expires > now) else {
            return Err(WorldError::new("claim_not_found", format!("there is no live claim {id} (expired or released?)"))
                .with("claim", id));
        };
        if !c.held_by(owner) {
            let message = if c.owner == owner.name {
                format!("claim {id} belongs to {} in another session; a new session adopts no claims", c.owner)
            } else {
                format!("claim {id} belongs to {}, not {}", c.owner, owner.name)
            };
            return Err(claim_error("not_owner", c, now, message));
        }
        Ok(c)
    }

    /// Extends a claim's lease to `lease` from now.
    pub fn renew(&mut self, id: u64, owner: Writer, lease: Duration, now: Instant) -> Result<&Claim, WorldError> {
        let c = self.owned_mut(id, owner, now)?;
        c.expires = now + lease;
        Ok(c)
    }

    /// Releases a claim.
    pub fn release(&mut self, id: u64, owner: Writer, now: Instant) -> Result<(), WorldError> {
        self.owned_mut(id, owner, now)?;
        self.remove(id);
        Ok(())
    }

    /// Ids of the claims `owner` holds.
    /// Whether some live claim is owned by `name` under an identity other than `key`.
    pub fn held_by_name_elsewhere(&self, name: &str, key: &str, now: Instant) -> bool {
        self.claims.values().any(|c| c.owner == name && c.session != key && c.expires > now)
    }

    pub fn owned_by(&self, owner: Writer) -> Vec<u64> {
        self.claims.values().filter(|c| c.held_by(owner)).map(|c| c.id).collect()
    }

    /// Releases every claim `owner` holds; returns their ids.
    pub fn release_all(&mut self, owner: Writer) -> Vec<u64> {
        let ids = self.owned_by(owner);
        for &id in &ids {
            self.remove(id);
        }
        ids
    }

    /// Lets agent `with` (in its current session `with_session`) write inside the claim too, or
    /// stops letting it (`revoke`, any session).
    pub fn share(
        &mut self,
        id: u64,
        owner: Writer,
        with: &str,
        with_session: &str,
        revoke: bool,
        now: Instant,
    ) -> Result<&Claim, WorldError> {
        let c = self.owned_mut(id, owner, now)?;
        if revoke {
            c.shared.remove(with);
        } else if with != owner.name {
            if c.shared.len() >= MAX_SHARED && !c.shared.contains_key(with) {
                return Err(WorldError::new("too_many_shares", format!("a claim is shared with at most {MAX_SHARED} agents"))
                    .with("claim", id));
            }
            c.shared.insert(with.to_owned(), with_session.to_owned());
        }
        Ok(c)
    }

    fn remove(&mut self, id: u64) -> Option<Claim> {
        let c = self.claims.remove(&id)?;
        match &c.scope {
            Scope::Path(p) => {
                if let Some(ids) = self.paths.get_mut(p) {
                    ids.remove(&id);
                    if ids.is_empty() {
                        self.paths.remove(p);
                    }
                }
            }
            Scope::Region { .. } => {
                self.regions.remove(&id);
            }
        }
        Some(c)
    }

    /// Drops claims whose lease ran out, and (given the live session ids) claims whose session is
    /// gone; shares with gone sessions lapse too. Returns how many claims were dropped.
    pub fn expire(&mut self, now: Instant, live_sessions: Option<&BTreeSet<String>>) -> usize {
        let dead: Vec<u64> = self
            .claims
            .values()
            .filter(|c| c.expires <= now || live_sessions.is_some_and(|live| !live.contains(&c.session)))
            .map(|c| c.id)
            .collect();
        for &id in &dead {
            self.remove(id);
        }
        if let Some(live) = live_sessions {
            for c in self.claims.values_mut() {
                c.shared.retain(|_, s| live.contains(s));
            }
        }
        dead.len()
    }

    /// Whether any live claim could reject a write by `w` (when none can, commands skip the
    /// step-by-step check entirely).
    pub fn could_block(&self, w: Writer, now: Instant) -> bool {
        self.claims.values().any(|c| !c.lets(w, now))
    }

    /// Live claims held by anyone but `w`.
    pub fn others(&self, w: Writer, now: Instant) -> Vec<&Claim> {
        self.claims.values().filter(|c| c.expires > now && !c.held_by(w)).collect()
    }

    /// Refuses replacing the whole world (`world_load`) while another agent holds a live claim:
    /// a load would wipe their claimed work. `force` doesn't override claims.
    pub fn check_replace_world(&self, w: Writer, now: Instant) -> Result<(), WorldError> {
        let others = self.others(w, now);
        if others.is_empty() {
            return Ok(());
        }
        let first = others[0];
        let mut err = claim_error(
            "claimed",
            first,
            now,
            format!(
                "{} other agent claim(s) are live (first: {}'s claim {} on {}); loading would replace their work. \
                 Ask them to release, or wait for the claims to end",
                others.len(),
                first.owner,
                first.id,
                first.scope.describe()
            ),
        );
        err = err.with("claims", Json::arr(others.iter().take(MAX_NAMED).map(|c| c.to_json(now)))).with("count", others.len());
        Err(err)
    }

    /// Claims in id order, filtered, one page at a time (at most 64 KiB).
    pub fn list(&self, owner: Option<&str>, path_prefix: Option<&str>, after: Option<u64>, limit: usize, now: Instant) -> Json {
        let matching = self.claims.values().filter(|c| {
            c.expires > now
                && owner.is_none_or(|o| c.owner == o)
                && path_prefix.is_none_or(|p| match &c.scope {
                    Scope::Path(cp) => path_under(cp, p) || path_under(p, cp),
                    Scope::Region { .. } => false,
                })
        });
        let total = matching.clone().count();
        let (mut items, mut bytes, mut next) = (Vec::new(), 0, None);
        for c in matching.filter(|c| after.is_none_or(|a| c.id > a)) {
            let item = c.to_json(now);
            let size = item.to_string().len();
            if items.len() == limit || (!items.is_empty() && bytes + size > MAX_LIST_BYTES) {
                next = items.last().and_then(|i: &Json| i.get("id")).and_then(Json::as_u64);
                break;
            }
            bytes += size;
            items.push(item);
        }
        Json::obj([("total", Json::from(total)), ("claims", Json::Arr(items)), ("next_cursor", Json::from(next))])
    }

    /// Rejects `changes` (just made by `w`) when any of them lies inside another agent's live
    /// claim not shared with `w` (see the module docs for what a change touches).
    pub fn check(&self, world: &World, changes: &[Change], w: Writer, now: Instant) -> Result<(), WorldError> {
        if self.claims.is_empty() {
            return Ok(());
        }
        let blocked = |c: &&Claim| !c.lets(w, now);
        // Points are needed for blocking region claims, and for naming the writer's own region
        // when a path claim blocks it (claims_overlap).
        let any_regions = !self.regions.is_empty();
        for change in changes {
            let points = if any_regions { change_points(world, change) } else { Vec::new() };
            let mut paths = vec![change.path()];
            if let Change::Set { prev_path: Some(p), .. } = change {
                paths.push(p);
            }
            for p in &paths {
                if let Some(c) = self.covering(p).find(blocked) {
                    return Err(self.rejection(c, change, p, w, &points, now));
                }
            }
            // A renamed or moved entity carries its descendants: claims below its old path would
            // lose them, claims below its new one would gain someone else's entities.
            if let Change::Set { prev_path: Some(prev), after, .. } = change {
                if world.has_children(after.id) {
                    for p in [prev.as_str(), change.path()] {
                        if let Some(c) = self.below(p).find(blocked) {
                            let (claimed, msg) = (c.scope.describe(), format!("it has descendants, which {p} carries"));
                            let message = format!(
                                "moving or renaming {p} would carry entities in or into {}'s claim {} on {claimed} ({msg}); \
                                 nothing was changed",
                                c.owner, c.id
                            );
                            return Err(claim_error("claimed", c, now, message).with("entity_path", p).with("op", change.op()));
                        }
                    }
                }
            }
            // Moving, rotating or scaling an entity moves (or shrinks away) everything below it,
            // claimed subtrees included.
            if let Change::Set { before: Some(before), after, .. } = change {
                if before.model() != after.model() && world.has_children(after.id) {
                    let p = change.path();
                    if let Some(c) = self.below(p).find(blocked) {
                        let message = format!(
                            "moving, rotating or scaling {p} would move entities inside {}'s claim {} on {}; nothing was \
                             changed",
                            c.owner,
                            c.id,
                            c.scope.describe()
                        );
                        return Err(claim_error("claimed", c, now, message).with("entity_path", p).with("op", change.op()));
                    }
                }
            }
            for q in &points {
                if let Some(c) = self.regions().filter(blocked).find(|c| c.scope.contains_point(*q)) {
                    return Err(self.rejection(c, change, change.path(), w, &points, now));
                }
            }
        }
        Ok(())
    }

    /// The error for `change` blocked by `c`: `claims_overlap` when `w`'s own claim covers the
    /// same spot (a path claim and a region claim meeting), `claimed` otherwise.
    fn rejection(&self, c: &Claim, change: &Change, at: &str, w: Writer, points: &[Vec3], now: Instant) -> WorldError {
        let mine = self
            .covering(change.path())
            .chain(self.regions().filter(|r| points.iter().any(|p| r.scope.contains_point(*p))))
            .find(|m| m.held_by(w) && m.expires > now);
        let error = match mine {
            Some(m) => claim_error(
                "claims_overlap",
                c,
                now,
                format!(
                    "{at} is inside both your claim {} on {} and {}'s claim {} on {}: a path claim and a region claim \
                     meet here, so neither of you can write it. Resolve it with claim_share or by releasing one",
                    m.id,
                    m.scope.describe(),
                    c.owner,
                    c.id,
                    c.scope.describe()
                ),
            )
            .with("your_claim", m.id),
            None => claim_error(
                "claimed",
                c,
                now,
                format!(
                    "{at} is inside {}'s claim {} on {} (expires in {} s); nothing was changed. Work elsewhere, or ask \
                     {} to claim_share it with you",
                    c.owner,
                    c.id,
                    c.scope.describe(),
                    c.expires.saturating_duration_since(now).as_secs(),
                    c.owner
                ),
            ),
        };
        error.with("entity_path", at).with("op", change.op())
    }
}

/// World origins a change touches: the entity's before and after, and when it moved with
/// descendants, theirs now and before the move.
fn change_points(world: &World, change: &Change) -> Vec<Vec3> {
    let mut points = Vec::new();
    match change {
        Change::Spawn { after, .. } => points.push(after.world_position()),
        Change::Delete { before, .. } => points.push(before.world_position()),
        Change::Set { before, after, .. } => {
            points.push(after.world_position());
            if let Some(b) = before {
                points.push(b.world_position());
            }
            let moved = before.as_ref().is_none_or(|b| b.model() != after.model());
            if moved && world.get(after.id).is_some() && world.has_children(after.id) {
                let delta = before.as_ref().and_then(|b| after.model().inverse().map(|inv| b.model() * inv));
                world.walk(Some(after.id), |e, _| {
                    if e.id != after.id {
                        let p = e.world_position();
                        points.push(p);
                        if let Some(d) = delta {
                            points.push(d.transform_point(p));
                        }
                    }
                });
            }
        }
        // Only the hosted game's own ticks record poses, and they are never checked.
        // Cameras belong to their agent name, not to any claimed region.
        Change::Env { .. } | Change::Pose { .. } | Change::Camera { .. } => {}
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{EntityKey, Recording, Shape, SpawnSpec, UpdateSpec};

    const ANN: Writer = Writer { name: "ann", session: Some("s1") };
    const BOB: Writer = Writer { name: "bob", session: Some("s2") };

    fn spawn(w: &mut World, path: &str, pos: Vec3, by: &str) -> Vec<Change> {
        let mut s = SpawnSpec::new(Shape::Cube);
        let (parent, name) = path.rsplit_once('/').unwrap();
        s.name = Some(name.into());
        s.parent = (!parent.is_empty()).then(|| EntityKey::Path(parent.into()));
        s.position = pos;
        w.spawn(s, by).unwrap();
        w.take_changes()
    }

    fn world() -> World {
        let mut w = World::default();
        w.set_recording(Recording::Full);
        w
    }

    fn spec(scope: Scope) -> ClaimSpec {
        ClaimSpec { scope, lease: DEFAULT_LEASE, note: String::new() }
    }

    fn take(c: &mut Claims, w: &World, who: Writer, scope: Scope, now: Instant) -> Result<u64, WorldError> {
        c.take(w, who.name, who.session.unwrap(), spec(scope), now).map(|c| c.id)
    }

    fn set(w: &mut World, path: &str, change: UpdateSpec) -> Vec<Change> {
        w.update(&EntityKey::Path(path.into()), change, "x").unwrap();
        w.take_changes()
    }

    #[test]
    fn path_claims_block_other_agents_only() {
        let now = Instant::now();
        let mut w = world();
        let mut c = Claims::default();
        take(&mut c, &w, ANN, Scope::path("/d1").unwrap(), now).unwrap();
        let changes = spawn(&mut w, "/d1", Vec3::ZERO, "ann");
        assert!(c.check(&w, &changes, ANN, now).is_ok());
        let err = c.check(&w, &changes, BOB, now).unwrap_err();
        assert_eq!(err.code, "claimed");
        assert_eq!(err.detail("owner"), Some(&Json::from("ann")));
        assert_eq!(err.detail("path"), Some(&Json::from("/d1")));
        let below = spawn(&mut w, "/d1/house", Vec3::ZERO, "x");
        assert_eq!(c.check(&w, &below, BOB, now).unwrap_err().code, "claimed");
        // Siblings with a common prefix are not inside.
        let other = spawn(&mut w, "/d10", Vec3::ZERO, "bob");
        assert!(c.check(&w, &other, BOB, now).is_ok());
        // Shared → allowed; expired → allowed.
        c.share(1, ANN, "bob", "s2", false, now).unwrap();
        assert!(c.check(&w, &below, BOB, now).is_ok());
        c.share(1, ANN, "bob", "", true, now).unwrap();
        assert!(c.check(&w, &below, BOB, now + DEFAULT_LEASE).is_ok(), "expired claims never block");
        assert_eq!(c.share(1, BOB, "carl", "s3", false, now).unwrap_err().code, "not_owner");
    }

    /// Reviewer finding (blocker): renaming or moving an ancestor carried a claimed subtree out
    /// of its claim, or foreign entities into one.
    #[test]
    fn moving_an_ancestor_cannot_carry_entities_out_of_or_into_a_claim() {
        let now = Instant::now();
        let mut w = world();
        spawn(&mut w, "/a", Vec3::ZERO, "x");
        spawn(&mut w, "/a/b", Vec3::ZERO, "x");
        spawn(&mut w, "/a/b/wall", Vec3::ZERO, "x");
        spawn(&mut w, "/x", Vec3::ZERO, "x");
        spawn(&mut w, "/x/b", Vec3::ZERO, "x");
        spawn(&mut w, "/lone", Vec3::ZERO, "x");
        let mut c = Claims::default();
        take(&mut c, &w, ANN, Scope::path("/a/b").unwrap(), now).unwrap();
        // Out: rename /a → /z, or move /a under /x.
        let rename = set(&mut w, "/a", UpdateSpec { name: Some("z".into()), ..Default::default() });
        let err = c.check(&w, &rename, BOB, now).unwrap_err();
        assert_eq!((err.code, err.detail("claim")), ("claimed", Some(&Json::from(1))));
        assert!(c.check(&w, &rename, ANN, now).is_ok(), "the owner may");
        let back = set(&mut w, "/z", UpdateSpec { name: Some("a".into()), ..Default::default() });
        assert!(c.check(&w, &back, ANN, now).is_ok());
        let reparent = set(&mut w, "/a", UpdateSpec { parent: Some(Some(EntityKey::Path("/x".into()))), ..Default::default() });
        assert_eq!(c.check(&w, &reparent, BOB, now).unwrap_err().code, "claimed");
        set(&mut w, "/x/a", UpdateSpec { parent: Some(None), ..Default::default() });
        // In: renaming /x (which has a child b) to /a would put a foreign /a/b... (name taken
        // here, so use a fresh claim on a path that doesn't exist yet).
        take(&mut c, &w, ANN, Scope::path("/camp/b").unwrap(), now).unwrap();
        let into = set(&mut w, "/x", UpdateSpec { name: Some("camp".into()), ..Default::default() });
        assert_eq!(c.check(&w, &into, BOB, now).unwrap_err().code, "claimed");
        // A leaf with no descendants may be renamed freely next to claims.
        let leaf = set(&mut w, "/lone", UpdateSpec { name: Some("solo".into()), ..Default::default() });
        assert!(c.check(&w, &leaf, BOB, now).is_ok());
        // A change by a game snapshot (no previous path) is never treated as a move.
        assert!(c.below("/a").any(|cl| cl.id == 1));
        assert!(c.below("/ab").next().is_none(), "prefix siblings are not below");
    }

    #[test]
    fn deletes_and_moves_out_of_a_claim_are_caught() {
        let now = Instant::now();
        let mut w = world();
        spawn(&mut w, "/a", Vec3::ZERO, "x");
        spawn(&mut w, "/a/b", Vec3::ZERO, "x");
        spawn(&mut w, "/c", Vec3::ZERO, "x");
        let mut c = Claims::default();
        take(&mut c, &w, ANN, Scope::path("/a/b").unwrap(), now).unwrap();
        w.delete(&EntityKey::Path("/a".into()), None).unwrap();
        let del = w.take_changes();
        assert_eq!(c.check(&w, &del, BOB, now).unwrap_err().detail("entity_path"), Some(&Json::from("/a/b")));
        // Moving the claimed entity away is a change at its old path.
        let mut w = world();
        spawn(&mut w, "/a", Vec3::ZERO, "x");
        spawn(&mut w, "/a/b", Vec3::ZERO, "x");
        spawn(&mut w, "/c", Vec3::ZERO, "x");
        let moved = set(&mut w, "/a/b", UpdateSpec { parent: Some(Some(EntityKey::Path("/c".into()))), ..Default::default() });
        assert_eq!(c.check(&w, &moved, BOB, now).unwrap_err().code, "claimed");
    }

    #[test]
    fn region_claims_use_world_origins_including_moved_children() {
        let now = Instant::now();
        let mut w = world();
        spawn(&mut w, "/g", Vec3::new(-10.0, 0.0, 0.0), "x");
        spawn(&mut w, "/g/kid", Vec3::new(1.0, 0.0, 0.0), "x");
        let mut c = Claims::default();
        let region = Scope::region(Vec3::new(0.0, -1.0, -1.0), Vec3::new(5.0, 1.0, 1.0)).unwrap();
        take(&mut c, &w, ANN, region, now).unwrap();
        let inside = spawn(&mut w, "/p", Vec3::new(1.0, 0.0, 0.0), "bob");
        assert_eq!(c.check(&w, &inside, BOB, now).unwrap_err().code, "claimed");
        let edge = spawn(&mut w, "/q", Vec3::new(5.0, 0.0, 0.0), "bob");
        assert!(c.check(&w, &edge, BOB, now).is_ok(), "max is exclusive");
        // Moving /g to x = 0 puts its child at x = 1: inside.
        let moved = set(&mut w, "/g", UpdateSpec { position: Some(Vec3::ZERO), ..Default::default() });
        assert_eq!(c.check(&w, &moved, BOB, now).unwrap_err().code, "claimed");
    }

    /// Review round 3 (R4): moving, rotating or scaling an unclaimed ancestor moved (or shrank
    /// away) a path-claimed subtree.
    #[test]
    fn transforming_an_ancestor_cannot_move_a_claimed_subtree() {
        let now = Instant::now();
        let mut w = world();
        spawn(&mut w, "/a", Vec3::ZERO, "x");
        spawn(&mut w, "/a/b", Vec3::ZERO, "x");
        spawn(&mut w, "/a/b/wall", Vec3::new(1.0, 0.0, 0.0), "x");
        spawn(&mut w, "/lone", Vec3::ZERO, "x");
        let mut c = Claims::default();
        take(&mut c, &w, ANN, Scope::path("/a/b").unwrap(), now).unwrap();
        let shrink = UpdateSpec { position: Some(Vec3::new(-9999.0, 0.0, 0.0)), scale: Some(Vec3::new(0.001, 0.001, 0.001)), ..Default::default() };
        let moved = set(&mut w, "/a", shrink);
        assert_eq!(c.check(&w, &moved, BOB, now).unwrap_err().code, "claimed");
        assert!(c.check(&w, &moved, ANN, now).is_ok(), "the owner may");
        let turned = set(&mut w, "/a", UpdateSpec { rotation_deg: Some(Vec3::new(0.0, 90.0, 0.0)), ..Default::default() });
        assert_eq!(c.check(&w, &turned, BOB, now).unwrap_err().code, "claimed");
        let recoloured = set(&mut w, "/a", UpdateSpec { color: Some([1.0, 0.0, 0.0]), ..Default::default() });
        assert!(c.check(&w, &recoloured, BOB, now).is_ok(), "a colour change moves nothing");
        let leaf = set(&mut w, "/lone", UpdateSpec { position: Some(Vec3::ONE), ..Default::default() });
        assert!(c.check(&w, &leaf, BOB, now).is_ok());
    }

    /// Reviewer finding: a path claim and a region claim that meet later lock each other out;
    /// the error must say so.
    #[test]
    fn a_path_claim_meeting_a_region_claim_reports_the_overlap() {
        let now = Instant::now();
        let mut w = world();
        let mut c = Claims::default();
        take(&mut c, &w, ANN, Scope::path("/pr").unwrap(), now).unwrap();
        let r = Scope::region(Vec3::new(10.0, -1.0, -1.0), Vec3::new(20.0, 1.0, 1.0)).unwrap();
        take(&mut c, &w, BOB, r, now).unwrap();
        spawn(&mut w, "/pr", Vec3::ZERO, "ann");
        let into = spawn(&mut w, "/pr/x", Vec3::new(12.0, 0.0, 0.0), "ann");
        let err = c.check(&w, &into, ANN, now).unwrap_err();
        assert_eq!((err.code, err.detail("your_claim"), err.detail("claim")), ("claims_overlap", Some(&Json::from(1)), Some(&Json::from(2))));
        let err = c.check(&w, &into, BOB, now).unwrap_err();
        assert_eq!((err.code, err.detail("your_claim")), ("claims_overlap", Some(&Json::from(2))));
        assert_eq!(c.check(&w, &into, Writer::new("carl", Some("s3")), now).unwrap_err().code, "claimed");
    }

    #[test]
    fn claims_of_different_owners_never_overlap() {
        let now = Instant::now();
        let mut w = world();
        spawn(&mut w, "/d", Vec3::ZERO, "x");
        spawn(&mut w, "/d/h", Vec3::new(2.0, 0.0, 0.0), "x");
        let mut c = Claims::default();
        take(&mut c, &w, ANN, Scope::path("/d").unwrap(), now).unwrap();
        assert_eq!(take(&mut c, &w, BOB, Scope::path("/d/h").unwrap(), now).unwrap_err().code, "claim_conflict");
        assert!(take(&mut c, &w, ANN, Scope::path("/d/h").unwrap(), now).is_ok(), "own claims may nest");
        let r = Scope::region(Vec3::new(1.0, -1.0, -1.0), Vec3::new(3.0, 1.0, 1.0)).unwrap();
        assert_eq!(take(&mut c, &w, BOB, r, now).unwrap_err().code, "claim_conflict");
        let r = Scope::region(Vec3::new(10.0, -1.0, -1.0), Vec3::new(13.0, 1.0, 1.0)).unwrap();
        take(&mut c, &w, BOB, r, now).unwrap();
        let touching = Scope::region(Vec3::new(13.0, -1.0, -1.0), Vec3::new(14.0, 1.0, 1.0)).unwrap();
        assert!(take(&mut c, &w, Writer::new("carl", Some("s3")), touching, now).is_ok(), "touching faces don't overlap");
        assert!(Scope::path("d").is_err() && Scope::path("/a//b").is_err() && Scope::path("/").is_ok());
        assert!(Scope::region(Vec3::ONE, Vec3::ONE).is_err());
    }

    /// Reviewer finding: the same name in another session could renew, release, write inside
    /// and share someone's claims.
    #[test]
    fn a_name_in_another_session_adopts_nothing() {
        let now = Instant::now();
        let mut w = world();
        let mut c = Claims::default();
        take(&mut c, &w, ANN, Scope::path("/q").unwrap(), now).unwrap();
        let impostor = Writer::new("ann", Some("s9"));
        assert_eq!(c.renew(1, impostor, DEFAULT_LEASE, now).unwrap_err().code, "not_owner");
        assert_eq!(c.release(1, impostor, now).unwrap_err().code, "not_owner");
        assert_eq!(c.share(1, impostor, "bob", "s2", false, now).unwrap_err().code, "not_owner");
        assert!(c.release_all(impostor).is_empty());
        let inside = spawn(&mut w, "/q", Vec3::ZERO, "ann");
        assert_eq!(c.check(&w, &inside, impostor, now).unwrap_err().code, "claimed");
        // Shares bind to the sharee's session too.
        c.share(1, ANN, "bob", "s2", false, now).unwrap();
        assert!(c.check(&w, &inside, BOB, now).is_ok());
        assert!(c.check(&w, &inside, Writer::new("bob", Some("s8")), now).is_err());
        let live: BTreeSet<String> = ["s1".to_string()].into();
        c.expire(now, Some(&live));
        assert!(c.get(1).unwrap().shared.is_empty(), "a share lapses with its session");
    }

    /// Reviewer finding: world_load replaced claimed work.
    #[test]
    fn replacing_the_world_is_refused_while_others_hold_claims() {
        let now = Instant::now();
        let w = world();
        let mut c = Claims::default();
        assert!(c.check_replace_world(BOB, now).is_ok());
        take(&mut c, &w, ANN, Scope::path("/q").unwrap(), now).unwrap();
        assert!(c.check_replace_world(ANN, now).is_ok(), "only your own claims");
        let err = c.check_replace_world(BOB, now).unwrap_err();
        assert_eq!((err.code, err.detail("count")), ("claimed", Some(&Json::from(1))));
        assert!(c.check_replace_world(BOB, now + DEFAULT_LEASE).is_ok(), "expired claims don't count");
    }

    #[test]
    fn expiry_release_and_listing() {
        let now = Instant::now();
        let w = world();
        let mut c = Claims::default();
        let short = ClaimSpec { scope: Scope::path("/a").unwrap(), lease: Duration::from_secs(10), note: "walls".into() };
        c.take(&w, "ann", "s1", short, now).unwrap();
        take(&mut c, &w, BOB, Scope::path("/b").unwrap(), now).unwrap();
        take(&mut c, &w, BOB, Scope::path("/c").unwrap(), now).unwrap();
        let page = c.list(None, None, None, 2, now);
        assert_eq!(page.get("total"), Some(&Json::from(3)));
        assert_eq!(page.get("next_cursor"), Some(&Json::from(2)));
        assert_eq!(c.list(None, None, Some(2), 2, now).get("claims").unwrap().as_array().unwrap().len(), 1);
        assert_eq!(c.list(None, None, Some(2), 2, now).get("next_cursor"), Some(&Json::Null));
        c.renew(1, ANN, DEFAULT_LEASE, now + Duration::from_secs(5)).unwrap();
        assert_eq!(c.expire(now + Duration::from_secs(20), None), 0, "renewed");
        let live: BTreeSet<String> = ["s1".to_string()].into();
        assert_eq!(c.expire(now, Some(&live)), 2, "bob's session is gone");
        assert_eq!(c.release(1, BOB, now).unwrap_err().code, "not_owner");
        c.release(1, ANN, now).unwrap();
        assert!(c.is_empty());
        assert_eq!(c.renew(1, ANN, DEFAULT_LEASE, now).unwrap_err().code, "claim_not_found");
    }

    #[test]
    fn listings_stay_under_their_byte_cap() {
        let now = Instant::now();
        let w = world();
        let mut c = Claims::default();
        for (agent, letter) in [("ann", "p"), ("bob", "q"), ("carl", "r")] {
            for i in 0..MAX_PER_AGENT {
                let note = "n".repeat(200);
                let scope = Scope::path(&format!("/{}{i}", letter.repeat(120))).unwrap();
                c.take(&w, agent, agent, ClaimSpec { scope, lease: DEFAULT_LEASE, note }, now).unwrap();
            }
        }
        let page = c.list(None, None, None, 200, now);
        assert!(page.to_string().len() <= MAX_LIST_BYTES + 1024, "{} bytes", page.to_string().len());
        assert!(page.get("next_cursor").and_then(Json::as_u64).is_some());
    }

    #[test]
    fn environment_changes_are_blocked_only_by_a_whole_world_claim() {
        let now = Instant::now();
        let mut w = world();
        let mut c = Claims::default();
        take(&mut c, &w, ANN, Scope::path("/a").unwrap(), now).unwrap();
        w.set_env(&crate::world::EnvPatch { exposure: Some(2.0), ..Default::default() }).unwrap();
        let env = w.take_changes();
        assert!(c.check(&w, &env, BOB, now).is_ok());
        c.release(1, ANN, now).unwrap();
        take(&mut c, &w, ANN, Scope::path("/").unwrap(), now).unwrap();
        assert_eq!(c.check(&w, &env, BOB, now).unwrap_err().code, "claimed");
    }
}

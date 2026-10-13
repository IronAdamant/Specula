//! Claims: leases agents take on part of the world so a swarm can build side by side safely.
//!
//! A claim covers a **subtree path** (`/district_3` and everything below it; `/` is the whole
//! world, environment included) or a **world-space region** (an axis-aligned box; an entity is
//! inside when its world origin is, `min <= p < max`). It is owned by an agent *name* (the
//! `agent_hello` name, so attribution, undo and claims agree on who is who) and leased for a time
//! limit the owner renews.
//!
//! - **Writes inside another agent's claim are rejected** with a structured `claimed` error naming
//!   the owner, the claim and when it expires — unless the owner shared the claim with the writer
//!   (`claim_share`). Unclaimed space is open to everyone. A write is checked on everything it
//!   changed: the entity's path before and after (renames and moves), its world origin before and
//!   after, and when a moved entity has children, their origins too. A delete touches every
//!   entity it removes, so deleting `/a` is rejected while someone holds `/a/b`.
//! - **Claims don't overlap** between different owners (even co-claimed ones), so an owner is
//!   never locked out of its own claim.
//! - **Expiry.** A claim ends when its lease runs out, when its owner releases it, or when the
//!   session that took (or last renewed) it disappears from the protocol (ended, or idle past the
//!   protocol's listing timeout). Expired claims never block.
//! - **Live coordination only:** claims are not saved with the world and survive `world_load`
//!   (they are about who works where, not what the world holds). Wall-clock time is fine here:
//!   claims never affect simulation state.
//! - **The hosted game is exempt**: its changes are the simulation's rules and are never checked
//!   against claims, and it holds none. Agents editing the game's own subtree should claim it
//!   like any other area.

use crate::journal::path_under;
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
    /// The protocol session that took or last renewed it.
    pub session: String,
    pub scope: Scope,
    /// Agent names allowed to write inside it too.
    pub shared: BTreeSet<String>,
    pub expires: Instant,
    pub note: String,
}

impl Claim {
    pub fn to_json(&self, now: Instant) -> Json {
        let mut f = vec![("id", Json::from(self.id)), ("owner", Json::from(self.owner.as_str())), self.scope.json()];
        let left = self.expires.saturating_duration_since(now).as_secs_f64();
        f.push(("expires_in_s", Json::from((left * 10.0).round() / 10.0)));
        if !self.shared.is_empty() {
            f.push(("shared", Json::arr(self.shared.iter().map(|s| Json::from(s.as_str())))));
        }
        if !self.note.is_empty() {
            f.push(("note", Json::from(self.note.as_str())));
        }
        Json::obj(f)
    }

    fn lets(&self, agent: &str, now: Instant) -> bool {
        self.owner == agent || self.shared.contains(agent) || self.expires <= now
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
        .with("expires_in_s", Json::from((c.expires.saturating_duration_since(now).as_secs_f64() * 10.0).round() / 10.0))
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

    /// Path claims covering `path` (on it or an ancestor, or `/`).
    fn covering<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a Claim> + 'a {
        let mut prefixes: Vec<&str> = vec!["/"];
        if path != "/" {
            prefixes.extend(path.char_indices().filter(|&(i, c)| c == '/' && i > 0).map(|(i, _)| &path[..i]));
            prefixes.push(path);
        }
        prefixes
            .into_iter()
            .filter_map(|p| self.paths.get(p))
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.claims.get(id))
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

    /// Takes a claim for `owner`. Rejected (`claim_conflict`) when it overlaps another agent's
    /// live claim.
    pub fn take(&mut self, world: &World, owner: &str, session: &str, spec: ClaimSpec, now: Instant) -> Result<&Claim, WorldError> {
        let ClaimSpec { scope, lease, note } = spec;
        self.expire(now, None);
        let mine = self.claims.values().filter(|c| c.owner == owner).count();
        if mine >= MAX_PER_AGENT || self.claims.len() >= MAX_CLAIMS {
            return Err(WorldError::new(
                "too_many_claims",
                format!("at most {MAX_PER_AGENT} claims per agent and {MAX_CLAIMS} in all; release some first"),
            )
            .with("yours", mine)
            .with("total", self.claims.len()));
        }
        if let Some(other) = self.claims.values().find(|c| c.owner != owner && self.overlap(world, &scope, &c.scope)) {
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
            shared: BTreeSet::new(),
            expires: now + lease,
            note,
        };
        self.claims.insert(id, claim);
        Ok(&self.claims[&id])
    }

    fn owned_mut(&mut self, id: u64, owner: &str, now: Instant) -> Result<&mut Claim, WorldError> {
        let Some(c) = self.claims.get_mut(&id).filter(|c| c.expires > now) else {
            return Err(WorldError::new("claim_not_found", format!("there is no live claim {id} (expired or released?)"))
                .with("claim", id));
        };
        if c.owner != owner {
            let owner_now = c.owner.clone();
            return Err(claim_error("not_owner", c, now, format!("claim {id} belongs to {owner_now}, not {owner}")));
        }
        Ok(c)
    }

    /// Extends a claim's lease to `lease` from now (and moves it to `session`).
    pub fn renew(&mut self, id: u64, owner: &str, session: &str, lease: Duration, now: Instant) -> Result<&Claim, WorldError> {
        let c = self.owned_mut(id, owner, now)?;
        c.expires = now + lease;
        c.session = session.to_owned();
        Ok(c)
    }

    /// Releases a claim.
    pub fn release(&mut self, id: u64, owner: &str, now: Instant) -> Result<(), WorldError> {
        self.owned_mut(id, owner, now)?;
        self.remove(id);
        Ok(())
    }

    /// Releases every claim `owner` holds; returns their ids.
    pub fn release_all(&mut self, owner: &str) -> Vec<u64> {
        let ids: Vec<u64> = self.claims.values().filter(|c| c.owner == owner).map(|c| c.id).collect();
        for &id in &ids {
            self.remove(id);
        }
        ids
    }

    /// Lets `with` write inside the claim too (or stops letting it, with `revoke`).
    pub fn share(&mut self, id: u64, owner: &str, with: &str, revoke: bool, now: Instant) -> Result<&Claim, WorldError> {
        let c = self.owned_mut(id, owner, now)?;
        if revoke {
            c.shared.remove(with);
        } else if with != owner {
            if c.shared.len() >= MAX_SHARED && !c.shared.contains(with) {
                return Err(WorldError::new("too_many_shares", format!("a claim is shared with at most {MAX_SHARED} agents"))
                    .with("claim", id));
            }
            c.shared.insert(with.to_owned());
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
    /// gone. Returns how many were dropped.
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
        dead.len()
    }

    /// Claims in id order, filtered, one page at a time.
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
        let page: Vec<&Claim> = matching.filter(|c| after.is_none_or(|a| c.id > a)).take(limit + 1).collect();
        let more = page.len() > limit;
        let items: Vec<Json> = page.iter().take(limit).map(|c| c.to_json(now)).collect();
        let next = if more { page.get(limit - 1).map(|c| c.id) } else { None };
        Json::obj([("total", Json::from(total)), ("claims", Json::Arr(items)), ("next_cursor", Json::from(next))])
    }

    /// Rejects `changes` (just made by `writer`) when any of them lies inside another agent's
    /// live claim not shared with `writer`.
    pub fn check(&self, world: &World, changes: &[Change], writer: &str, now: Instant) -> Result<(), WorldError> {
        if self.claims.is_empty() {
            return Ok(());
        }
        let blocked = |c: &&Claim| !c.lets(writer, now);
        let rejection = |c: &Claim, change: &Change, at: &str| {
            claim_error(
                "claimed",
                c,
                now,
                format!(
                    "{at} is inside {}'s claim {} on {} (expires in {} s); nothing was changed. Work elsewhere, or ask {} \
                     to claim_share it with you",
                    c.owner,
                    c.id,
                    c.scope.describe(),
                    c.expires.saturating_duration_since(now).as_secs(),
                    c.owner
                ),
            )
            .with("entity_path", at)
            .with("op", change.op())
        };
        let regions: Vec<&Claim> = self.regions.iter().filter_map(|id| self.claims.get(id)).filter(blocked).collect();
        for change in changes {
            let mut paths = vec![change.path()];
            if let Change::Set { prev_path: Some(p), .. } = change {
                paths.push(p);
            }
            for p in paths {
                if let Some(c) = self.covering(p).find(blocked) {
                    return Err(rejection(c, change, p));
                }
            }
            if regions.is_empty() {
                continue;
            }
            let mut points: Vec<Vec3> = Vec::new();
            match change {
                Change::Spawn { after, .. } => points.push(after.world_position()),
                Change::Delete { before, .. } => points.push(before.world_position()),
                Change::Set { before, after, .. } => {
                    points.push(after.world_position());
                    if let Some(b) = before {
                        points.push(b.world_position());
                    }
                    // A moved parent moves its children: check where they are now and were.
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
                Change::Env { .. } => {}
            }
            for p in points {
                if let Some(c) = regions.iter().find(|c| c.scope.contains_point(p)) {
                    return Err(rejection(c, change, change.path()));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{EntityKey, Recording, Shape, SpawnSpec, UpdateSpec};

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

    #[test]
    fn path_claims_block_other_agents_only() {
        let now = Instant::now();
        let mut w = world();
        let mut c = Claims::default();
        c.take(&w, "ann", "s1", ClaimSpec { scope: Scope::path("/d1").unwrap(), lease: DEFAULT_LEASE, note: String::new() }, now).unwrap();
        let changes = spawn(&mut w, "/d1", Vec3::ZERO, "ann");
        assert!(c.check(&w, &changes, "ann", now).is_ok());
        let err = c.check(&w, &changes, "bob", now).unwrap_err();
        assert_eq!(err.code, "claimed");
        assert_eq!(err.detail("owner"), Some(&Json::from("ann")));
        assert_eq!(err.detail("path"), Some(&Json::from("/d1")));
        let below = spawn(&mut w, "/d1/house", Vec3::ZERO, "x");
        assert_eq!(c.check(&w, &below, "bob", now).unwrap_err().code, "claimed");
        // Siblings with a common prefix are not inside.
        let other = spawn(&mut w, "/d10", Vec3::ZERO, "bob");
        assert!(c.check(&w, &other, "bob", now).is_ok());
        // Shared → allowed; expired → allowed.
        c.share(1, "ann", "bob", false, now).unwrap();
        assert!(c.check(&w, &below, "bob", now).is_ok());
        c.share(1, "ann", "bob", true, now).unwrap();
        assert!(c.check(&w, &below, "bob", now + DEFAULT_LEASE).is_ok(), "expired claims never block");
        assert_eq!(c.share(1, "bob", "carl", false, now).unwrap_err().code, "not_owner");
    }

    #[test]
    fn deletes_and_moves_out_of_a_claim_are_caught() {
        let now = Instant::now();
        let mut w = world();
        spawn(&mut w, "/a", Vec3::ZERO, "x");
        spawn(&mut w, "/a/b", Vec3::ZERO, "x");
        spawn(&mut w, "/c", Vec3::ZERO, "x");
        let mut c = Claims::default();
        c.take(&w, "ann", "s", ClaimSpec { scope: Scope::path("/a/b").unwrap(), lease: DEFAULT_LEASE, note: String::new() }, now).unwrap();
        w.delete(&EntityKey::Path("/a".into()), None).unwrap();
        let del = w.take_changes();
        assert_eq!(c.check(&w, &del, "bob", now).unwrap_err().detail("entity_path"), Some(&Json::from("/a/b")));
        // Moving the claimed entity away is a change at its old path.
        let mut w = world();
        spawn(&mut w, "/a", Vec3::ZERO, "x");
        spawn(&mut w, "/a/b", Vec3::ZERO, "x");
        spawn(&mut w, "/c", Vec3::ZERO, "x");
        let change = UpdateSpec { parent: Some(Some(EntityKey::Path("/c".into()))), ..Default::default() };
        w.update(&EntityKey::Path("/a/b".into()), change, "bob").unwrap();
        let moved = w.take_changes();
        assert_eq!(c.check(&w, &moved, "bob", now).unwrap_err().code, "claimed");
    }

    #[test]
    fn region_claims_use_world_origins_including_moved_children() {
        let now = Instant::now();
        let mut w = world();
        spawn(&mut w, "/g", Vec3::new(-10.0, 0.0, 0.0), "x");
        spawn(&mut w, "/g/kid", Vec3::new(1.0, 0.0, 0.0), "x");
        w.take_changes();
        let mut c = Claims::default();
        let region = Scope::region(Vec3::new(0.0, -1.0, -1.0), Vec3::new(5.0, 1.0, 1.0)).unwrap();
        c.take(&w, "ann", "s", ClaimSpec { scope: region, lease: DEFAULT_LEASE, note: String::new() }, now).unwrap();
        let inside = spawn(&mut w, "/p", Vec3::new(1.0, 0.0, 0.0), "bob");
        assert_eq!(c.check(&w, &inside, "bob", now).unwrap_err().code, "claimed");
        let edge = spawn(&mut w, "/q", Vec3::new(5.0, 0.0, 0.0), "bob");
        assert!(c.check(&w, &edge, "bob", now).is_ok(), "max is exclusive");
        // Moving /g to x = 0 puts its child at x = 1: inside.
        w.update(&EntityKey::Path("/g".into()), UpdateSpec { position: Some(Vec3::ZERO), ..Default::default() }, "bob")
            .unwrap();
        let moved = w.take_changes();
        assert_eq!(c.check(&w, &moved, "bob", now).unwrap_err().code, "claimed");
    }

    #[test]
    fn claims_of_different_owners_never_overlap() {
        let now = Instant::now();
        let mut w = world();
        spawn(&mut w, "/d", Vec3::ZERO, "x");
        spawn(&mut w, "/d/h", Vec3::new(2.0, 0.0, 0.0), "x");
        let mut c = Claims::default();
        let lease = DEFAULT_LEASE;
        c.take(&w, "ann", "s", ClaimSpec { scope: Scope::path("/d").unwrap(), lease, note: String::new() }, now).unwrap();
        let err = c.take(&w, "bob", "t", ClaimSpec { scope: Scope::path("/d/h").unwrap(), lease, note: String::new() }, now).unwrap_err();
        assert_eq!(err.code, "claim_conflict");
        assert!(c.take(&w, "ann", "s", ClaimSpec { scope: Scope::path("/d/h").unwrap(), lease, note: String::new() }, now).is_ok(), "own claims may nest");
        let r = Scope::region(Vec3::new(1.0, -1.0, -1.0), Vec3::new(3.0, 1.0, 1.0)).unwrap();
        assert_eq!(c.take(&w, "bob", "t", ClaimSpec { scope: r, lease, note: String::new() }, now).unwrap_err().code, "claim_conflict");
        let r = Scope::region(Vec3::new(10.0, -1.0, -1.0), Vec3::new(13.0, 1.0, 1.0)).unwrap();
        c.take(&w, "bob", "t", ClaimSpec { scope: r, lease, note: String::new() }, now).unwrap();
        let touching = Scope::region(Vec3::new(13.0, -1.0, -1.0), Vec3::new(14.0, 1.0, 1.0)).unwrap();
        assert!(c.take(&w, "carl", "u", ClaimSpec { scope: touching, lease, note: String::new() }, now).is_ok(), "touching faces don't overlap");
        assert!(Scope::path("d").is_err() && Scope::path("/a//b").is_err() && Scope::path("/").is_ok());
        assert!(Scope::region(Vec3::ONE, Vec3::ONE).is_err());
    }

    #[test]
    fn expiry_release_and_listing() {
        let now = Instant::now();
        let w = world();
        let mut c = Claims::default();
        let short = Duration::from_secs(10);
        c.take(&w, "ann", "s1", ClaimSpec { scope: Scope::path("/a").unwrap(), lease: short, note: "walls".into() }, now).unwrap();
        c.take(&w, "bob", "s2", ClaimSpec { scope: Scope::path("/b").unwrap(), lease: DEFAULT_LEASE, note: String::new() }, now).unwrap();
        c.take(&w, "bob", "s2", ClaimSpec { scope: Scope::path("/c").unwrap(), lease: DEFAULT_LEASE, note: String::new() }, now).unwrap();
        let page = c.list(None, None, None, 2, now);
        assert_eq!(page.get("total"), Some(&Json::from(3)));
        assert_eq!(page.get("next_cursor"), Some(&Json::from(2)));
        assert_eq!(c.list(None, None, Some(2), 2, now).get("claims").unwrap().as_array().unwrap().len(), 1);
        c.renew(1, "ann", "s1", DEFAULT_LEASE, now + Duration::from_secs(5)).unwrap();
        assert_eq!(c.expire(now + Duration::from_secs(20), None), 0, "renewed");
        let live: BTreeSet<String> = ["s1".to_string()].into();
        assert_eq!(c.expire(now, Some(&live)), 2, "bob's session is gone");
        assert_eq!(c.release(1, "bob", now).unwrap_err().code, "not_owner");
        c.release(1, "ann", now).unwrap();
        assert!(c.is_empty());
        assert_eq!(c.renew(1, "ann", "s1", DEFAULT_LEASE, now).unwrap_err().code, "claim_not_found");
    }

    #[test]
    fn environment_changes_are_blocked_only_by_a_whole_world_claim() {
        let now = Instant::now();
        let mut w = world();
        let mut c = Claims::default();
        c.take(&w, "ann", "s", ClaimSpec { scope: Scope::path("/a").unwrap(), lease: DEFAULT_LEASE, note: String::new() }, now).unwrap();
        w.set_env(&crate::world::EnvPatch { exposure: Some(2.0), ..Default::default() }).unwrap();
        let env = w.take_changes();
        assert!(c.check(&w, &env, "bob", now).is_ok());
        c.release(1, "ann", now).unwrap();
        c.take(&w, "ann", "s", ClaimSpec { scope: Scope::path("/").unwrap(), lease: DEFAULT_LEASE, note: String::new() }, now).unwrap();
        assert_eq!(c.check(&w, &env, "bob", now).unwrap_err().code, "claimed");
    }
}

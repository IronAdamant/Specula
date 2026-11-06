//! Intents: an agent says what it is about to work on *before* it edits, so overlaps surface
//! while they are still cheap to resolve.
//!
//! `intent_declare {paths, regions, text}` records the planned scope (subtree paths and world
//! boxes, like claims) with a lease. The answer lists every other agent's live intent and claim
//! that overlaps it (`overlaps`), so the agent can coordinate, pick another area or claim what it
//! needs. Intents never block anything: claims do that. `claim_take` also reports other agents'
//! intents it overlaps (`intent_warnings`).
//!
//! Live coordination like claims: kept in memory, not saved, ended by their lease (renewed by
//! declaring again with `replace`) or by `intent_clear`. They belong to an identity key, so a
//! resumed agent keeps its intents.

use crate::claims::{scopes_overlap, Claims, Scope};
use crate::journal::Writer;
use crate::world::{World, WorldError};
use specula_core::Json;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub const DEFAULT_LEASE: Duration = Duration::from_secs(600);
/// Intents one agent may hold, and in all.
pub const MAX_PER_AGENT: usize = 16;
pub const MAX_INTENTS: usize = 2048;
/// Paths plus regions in one intent.
pub const MAX_SCOPES: usize = 8;
pub const MAX_TEXT: usize = 500;
/// Overlaps named in one answer.
const MAX_OVERLAPS: usize = 10;

#[derive(Clone, Debug)]
pub struct Intent {
    pub id: u64,
    pub owner: String,
    pub key: String,
    pub scopes: Vec<Scope>,
    pub text: String,
    /// The task it is for, if any.
    pub task: Option<u64>,
    pub tick: u64,
    pub expires: Instant,
}

fn secs_left(expires: Instant, now: Instant) -> Json {
    Json::from((expires.saturating_duration_since(now).as_secs_f64() * 10.0).round() / 10.0)
}

fn scopes_json(scopes: &[Scope]) -> (Json, Json) {
    let mut paths = Vec::new();
    let mut regions = Vec::new();
    for s in scopes {
        match s.json() {
            ("path", p) => paths.push(p),
            (_, r) => regions.push(r),
        }
    }
    (Json::Arr(paths), Json::Arr(regions))
}

impl Intent {
    pub fn held_by(&self, w: Writer) -> bool {
        self.owner == w.name && w.session == Some(self.key.as_str())
    }

    pub fn to_json(&self, now: Instant) -> Json {
        let (paths, regions) = scopes_json(&self.scopes);
        let mut f = vec![("id", Json::from(self.id)), ("owner", Json::from(self.owner.as_str())), ("paths", paths), ("regions", regions)];
        f.push(("text", Json::from(self.text.as_str())));
        if let Some(t) = self.task {
            f.push(("task", Json::from(t)));
        }
        f.push(("tick", Json::from(self.tick)));
        f.push(("expires_in_s", secs_left(self.expires, now)));
        Json::obj(f)
    }

    fn overlaps(&self, world: &World, scopes: &[Scope]) -> bool {
        self.scopes.iter().any(|a| scopes.iter().any(|b| scopes_overlap(world, a, b)))
    }
}

/// What `intent_declare` asks for.
#[derive(Clone, Debug)]
pub struct IntentSpec {
    pub scopes: Vec<Scope>,
    pub text: String,
    pub task: Option<u64>,
    pub lease: Duration,
    /// Replace this intent of yours (renewing it) instead of adding one.
    pub replace: Option<u64>,
}

#[derive(Default)]
pub struct Intents {
    intents: BTreeMap<u64, Intent>,
    next: u64,
}

impl Intents {
    pub fn len(&self) -> usize {
        self.intents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.intents.is_empty()
    }

    pub fn expire(&mut self, now: Instant) {
        self.intents.retain(|_, i| i.expires > now);
    }

    /// Other agents' live intents that overlap `scopes`.
    pub fn overlapping<'a>(&'a self, world: &'a World, w: Writer<'a>, scopes: &'a [Scope], now: Instant) -> impl Iterator<Item = &'a Intent> + 'a {
        self.intents.values().filter(move |i| i.expires > now && !i.held_by(w) && i.overlaps(world, scopes))
    }

    /// `intent_declare`: records it and reports what it overlaps (intents and claims of others).
    pub fn declare(&mut self, world: &World, claims: &Claims, w: Writer, tick: u64, spec: IntentSpec, now: Instant) -> Result<Json, WorldError> {
        self.expire(now);
        let key = w.session.unwrap_or_default().to_owned();
        if let Some(r) = spec.replace {
            match self.intents.get(&r) {
                Some(i) if i.held_by(w) => {}
                Some(i) => {
                    return Err(WorldError::new("not_yours", format!("intent {r} belongs to {}", i.owner)).with("intent", r).with("owner", i.owner.as_str()))
                }
                None => return Err(WorldError::new("intent_not_found", format!("there is no live intent {r}")).with("intent", r)),
            }
        } else {
            let mine = self.intents.values().filter(|i| i.held_by(w)).count();
            if mine >= MAX_PER_AGENT || self.intents.len() >= MAX_INTENTS {
                return Err(WorldError::new(
                    "too_many_intents",
                    format!("at most {MAX_PER_AGENT} intents per agent and {MAX_INTENTS} in all; clear some (intent_clear) or replace one"),
                )
                .with("yours", mine));
            }
        }
        let others: Vec<Json> = self.overlapping(world, w, &spec.scopes, now).map(|i| {
            let mut j = i.to_json(now);
            j.set("kind", Json::from("intent"));
            j
        })
        .collect();
        let claimed: Vec<Json> = claims
            .live(now)
            .filter(|c| !c.held_by(w) && spec.scopes.iter().any(|s| scopes_overlap(world, s, &c.scope)))
            .map(|c| {
                let mut j = c.to_json(now);
                j.set("kind", Json::from("claim"));
                j
            })
            .collect();
        let count = others.len() + claimed.len();
        let id = spec.replace.unwrap_or_else(|| {
            self.next += 1;
            self.next
        });
        let intent = Intent { id, owner: w.name.to_owned(), key, scopes: spec.scopes, text: spec.text, task: spec.task, tick, expires: now + spec.lease };
        let mut out = Json::obj([("intent", intent.to_json(now))]);
        self.intents.insert(id, intent);
        let mut overlaps: Vec<Json> = claimed.into_iter().chain(others).collect();
        overlaps.truncate(MAX_OVERLAPS);
        if count > 0 {
            out.set(
                "warning",
                Json::from(format!(
                    "your scope overlaps {count} other agent intent(s)/claim(s): coordinate with them (note_add, task board), \
                     narrow your scope, or claim_take what you need before editing"
                )),
            );
        }
        out.set("overlap_count", Json::from(count));
        out.set("overlaps", Json::Arr(overlaps));
        Ok(out)
    }

    /// `intent_clear`: one intent of `w`'s, or all of them. Returns the ids cleared.
    pub fn clear(&mut self, w: Writer, id: Option<u64>) -> Result<Vec<u64>, WorldError> {
        match id {
            Some(id) => match self.intents.get(&id) {
                Some(i) if i.held_by(w) => {
                    self.intents.remove(&id);
                    Ok(vec![id])
                }
                Some(i) => Err(WorldError::new("not_yours", format!("intent {id} belongs to {}", i.owner)).with("intent", id)),
                None => Err(WorldError::new("intent_not_found", format!("there is no live intent {id}")).with("intent", id)),
            },
            None => {
                let ids: Vec<u64> = self.intents.values().filter(|i| i.held_by(w)).map(|i| i.id).collect();
                for id in &ids {
                    self.intents.remove(id);
                }
                Ok(ids)
            }
        }
    }

    /// Live intents of `owner` (or everyone), at or around `path_prefix`, one page in id order.
    pub fn list(&self, owner: Option<&str>, path_prefix: Option<&str>, after: Option<u64>, limit: usize, now: Instant) -> Json {
        use crate::journal::path_under;
        let matching = self.intents.values().filter(|i| {
            i.expires > now
                && owner.is_none_or(|o| i.owner == o)
                && path_prefix.is_none_or(|p| {
                    i.scopes.iter().any(|s| matches!(s, Scope::Path(ip) if path_under(ip, p) || path_under(p, ip)))
                })
        });
        let total = matching.clone().count();
        let (items, next) = crate::board::page(matching.filter(|i| after.is_none_or(|a| i.id > a)).map(|i| (i.id, i.to_json(now))), limit);
        Json::obj([("total", Json::from(total)), ("intents", Json::Arr(items)), ("next_cursor", Json::from(next))])
    }

    /// Ids and owners of others' intents overlapping `scope`, for claim_take's warnings.
    pub fn warnings(&self, world: &World, w: Writer, scope: &Scope, now: Instant) -> Vec<Json> {
        let scopes = std::slice::from_ref(scope);
        self.overlapping(world, w, scopes, now).take(MAX_OVERLAPS).map(|i| i.to_json(now)).collect()
    }

    /// Live intents held by `name`, for roll-ups.
    pub fn of(&self, name: &str, now: Instant) -> impl Iterator<Item = &Intent> + '_ {
        let name = name.to_owned();
        self.intents.values().filter(move |i| i.owner == name && i.expires > now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claims::ClaimSpec;
    use crate::world::{EntityKey, Recording, Shape, SpawnSpec};
    use specula_core::math::Vec3;

    const ANN: Writer = Writer { name: "ann", session: Some("k1"), via: None };
    const BOB: Writer = Writer { name: "bob", session: Some("k2"), via: None };

    fn spec(paths: &[&str]) -> IntentSpec {
        IntentSpec {
            scopes: paths.iter().map(|p| Scope::path(p).unwrap()).collect(),
            text: "rework".into(),
            task: None,
            lease: DEFAULT_LEASE,
            replace: None,
        }
    }

    #[test]
    fn overlaps_with_intents_and_claims_are_flagged_before_any_edit() {
        let now = Instant::now();
        let mut w = World::default();
        w.set_recording(Recording::Full);
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("gate".into());
        s.position = Vec3::new(3.0, 0.0, 0.0);
        w.spawn(s, "x").unwrap();
        let mut claims = Claims::default();
        let region = Scope::region(Vec3::new(0.0, -1.0, -1.0), Vec3::new(5.0, 1.0, 1.0)).unwrap();
        claims.take(&w, "carl", "k3", ClaimSpec { scope: region, lease: DEFAULT_LEASE, note: String::new() }, now).unwrap();
        let mut i = Intents::default();
        let first = i.declare(&w, &claims, ANN, 0, spec(&["/east"]), now).unwrap();
        assert_eq!(first.get("overlap_count"), Some(&Json::from(0)));
        // Bob plans the east gate (under ann's /east): flagged; and /gate sits in carl's region.
        let second = i.declare(&w, &claims, BOB, 0, spec(&["/east/gate", "/gate"]), now).unwrap();
        assert_eq!(second.get("overlap_count"), Some(&Json::from(2)));
        let kinds: Vec<&str> = second.get("overlaps").unwrap().as_array().unwrap().iter().map(|o| o.get("kind").unwrap().as_str().unwrap()).collect();
        assert_eq!(kinds, ["claim", "intent"]);
        assert!(second.get("warning").is_some());
        // Your own don't count; claim_take's warnings name others' intents.
        assert_eq!(i.warnings(&w, ANN, &Scope::path("/east/gate").unwrap(), now).len(), 1);
        assert_eq!(i.warnings(&w, ANN, &Scope::path("/west").unwrap(), now).len(), 0);
        assert_eq!(i.list(None, Some("/east"), None, 10, now).get("total"), Some(&Json::from(2)));
        // Clearing and expiry.
        assert_eq!(i.clear(BOB, Some(1)).unwrap_err().code, "not_yours");
        assert_eq!(i.clear(BOB, None).unwrap(), vec![2]);
        i.expire(now + DEFAULT_LEASE);
        assert_eq!(i.len(), 0);
        let _ = EntityKey::Id(1);
    }

    #[test]
    fn replacing_renews_and_limits_hold() {
        let now = Instant::now();
        let w = World::default();
        let claims = Claims::default();
        let mut i = Intents::default();
        for _ in 0..MAX_PER_AGENT {
            i.declare(&w, &claims, ANN, 0, spec(&["/a"]), now).unwrap();
        }
        assert_eq!(i.declare(&w, &claims, ANN, 0, spec(&["/a"]), now).unwrap_err().code, "too_many_intents");
        let renewed = i.declare(&w, &claims, ANN, 0, IntentSpec { replace: Some(1), ..spec(&["/b"]) }, now).unwrap();
        assert_eq!(renewed.get("intent").and_then(|j| j.get("id")), Some(&Json::from(1)));
        assert_eq!(i.declare(&w, &claims, BOB, 0, IntentSpec { replace: Some(1), ..spec(&["/b"]) }, now).unwrap_err().code, "not_yours");
        assert_eq!(i.len(), MAX_PER_AGENT);
    }
}

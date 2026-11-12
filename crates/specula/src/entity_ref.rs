//! References to entities that outlive world replacements: board notes, critiques and task-board
//! `entities` (docs/design/EDITOR.md, "Stable entity identity").
//!
//! A reference holds the entity's identity ([`EntityUid`]: the spawn lineage it was born in and
//! its id), never a bare id or a path. It is **attached** while the world has that very entity
//! (one map lookup) and **detached** otherwise: deleted, or absent from the world loaded since.
//! A detached reference shows its last known path and no id, and never moves to whatever later
//! takes the path or the id; an undone delete or a later load that has the entity attaches it
//! again.
//!
//! Paths are only shown: an attached reference's current path is looked up when it is listed
//! (the page's items only), and the stored *last known* path is refreshed then, when it is made,
//! and once before the world is replaced ([`crate::swarm::Swarm::remember_entity_paths`]).
//! Nothing runs per loop, and renames (by anyone: agents, the human, modules, the hosted game)
//! can't race a reference, since they don't change identity.
//!
//! **Deletes** are the one write that matters: a deleted entity can't be looked up any more, so
//! its references take the path it had when it was deleted ([`Gone`],
//! [`crate::swarm::Swarm::note_deletes`], after each journaled write that deleted something a
//! reference names). A note on `/dd` renamed to `/ee` and then deleted shows `/ee`.

use crate::journal::path_under;
use crate::world::{EntityUid, World, WorldError};
use specula_core::Json;
use std::collections::{BTreeMap, BTreeSet};

/// Entities just deleted that a reference names: identity → its path when it was deleted.
pub type Gone<'a> = BTreeMap<EntityUid, &'a str>;

/// Who names each entity identity: a store's index of its references, kept as they come and go
/// (`K`: the kind of holder, with the holder's id). A delete then costs a lookup per deleted
/// entity and a visit per holder that names it, whatever the store holds. (Review R1 of
/// m5-edswarm, M1: the set of named identities was rebuilt from every note, task, critique and
/// submission whenever the board's revision had moved, which every commit by a task's holder
/// does: 69-77 ms a delete at the board's own caps, on the engine thread.)
#[derive(Clone, Debug, PartialEq)]
pub struct Referrers<K: Ord + Copy> {
    by_uid: BTreeMap<EntityUid, BTreeSet<(K, u64)>>,
}

impl<K: Ord + Copy> Default for Referrers<K> {
    fn default() -> Self {
        Referrers { by_uid: BTreeMap::new() }
    }
}

impl<K: Ord + Copy> Referrers<K> {
    /// Holder (`kind`, `id`) names `uid` (once, however often it is said).
    pub fn add(&mut self, uid: EntityUid, kind: K, id: u64) {
        self.by_uid.entry(uid).or_default().insert((kind, id));
    }

    /// Holder (`kind`, `id`) no longer names `uid` (it was dropped).
    pub fn forget(&mut self, uid: EntityUid, kind: K, id: u64) {
        if let Some(holders) = self.by_uid.get_mut(&uid) {
            holders.remove(&(kind, id));
            if holders.is_empty() {
                self.by_uid.remove(&uid);
            }
        }
    }

    /// Whether any holder names `uid`.
    pub fn names(&self, uid: &EntityUid) -> bool {
        self.by_uid.contains_key(uid)
    }

    /// The holders that name an identity in `gone`, each once.
    pub fn holders(&self, gone: &Gone<'_>) -> BTreeSet<(K, u64)> {
        gone.keys().filter_map(|uid| self.by_uid.get(uid)).flatten().copied().collect()
    }
}

/// See the module docs. Equal when they name the same identity (paths are only shown).
#[derive(Clone, Debug)]
pub struct EntityRef {
    pub uid: EntityUid,
    /// The entity's path when last seen (shown while detached; may be empty for a reference
    /// read from a file that didn't record it).
    pub path: String,
}

impl PartialEq for EntityRef {
    fn eq(&self, other: &Self) -> bool {
        self.uid == other.uid
    }
}

impl std::fmt::Display for EntityRef {
    /// Its id (board events: "entity 5").
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.uid.id)
    }
}

impl EntityRef {
    /// A reference to entity `id` of `world` as it is now (`None`: no such entity).
    pub fn of(world: &World, id: u64) -> Option<EntityRef> {
        let uid = world.uid_of(id)?;
        Some(EntityRef { uid, path: world.path_of(id) })
    }

    /// References to `ids` (in order, duplicates kept); the first id with no entity is a
    /// `not_found` error naming it and its index in `field`.
    pub fn of_ids(world: &World, ids: &[u64], field: &str) -> Result<Vec<EntityRef>, WorldError> {
        ids.iter()
            .enumerate()
            .map(|(i, &id)| {
                EntityRef::of(world, id).ok_or_else(|| {
                    WorldError::new("not_found", format!("{field}[{i}]: no entity with id {id} (references name entities that exist)"))
                        .with("field", field)
                        .with("index", i)
                        .with("id", id)
                })
            })
            .collect()
    }

    /// The entity's id while `world` has this very entity (attached), else `None` (detached).
    pub fn attached(&self, world: &World) -> Option<u64> {
        world.resolve_uid(self.uid)
    }

    /// Brings the last known path up to date while attached (checked in place: it allocates
    /// only when the entity moved). Whether it changed.
    pub fn refresh(&mut self, world: &World) -> bool {
        let Some(id) = self.attached(world) else { return false };
        if world.path_is(id, &self.path) {
            return false;
        }
        self.path = world.path_of(id);
        true
    }

    /// Takes the path its entity had when it was deleted, if `gone` has its identity. Whether it
    /// changed.
    pub fn deleted(&mut self, gone: &Gone<'_>) -> bool {
        match gone.get(&self.uid) {
            Some(&p) if p != self.path => {
                p.clone_into(&mut self.path);
                true
            }
            _ => false,
        }
    }

    /// As shown to agents (call [`EntityRef::refresh`] first): `{id, path}` while attached,
    /// `{path, detached: true}` (no id: it may name another entity now) otherwise.
    pub fn json(&self, world: &World) -> Json {
        match self.attached(world) {
            Some(id) => Json::obj([("id", Json::from(id)), ("path", Json::from(self.path.as_str()))]),
            None => {
                let mut f = Vec::with_capacity(2);
                if !self.path.is_empty() {
                    f.push(("path", Json::from(self.path.as_str())));
                }
                f.push(("detached", Json::from(true)));
                Json::obj(f)
            }
        }
    }

    /// As saved (board and review files): `{id, born, path}`.
    pub fn saved_json(&self) -> Json {
        Json::obj([
            ("id", Json::from(self.uid.id)),
            ("born", Json::from(format!("{:016x}", self.uid.lineage))),
            ("path", Json::from(self.path.as_str())),
        ])
    }

    /// Reads [`EntityRef::saved_json`], or what older files kept: `{id, path}` or a bare id.
    /// Without `born` (or with one that doesn't parse) the lineage is unknown (0), so the
    /// reference stays detached. `None`: not a reference at all (no id).
    pub fn from_saved(j: &Json) -> Option<EntityRef> {
        if let Some(id) = j.as_u64() {
            return Some(EntityRef { uid: EntityUid { lineage: 0, id }, path: String::new() });
        }
        let id = j.get("id").and_then(Json::as_u64)?;
        let lineage = j
            .get("born")
            .and_then(Json::as_str)
            .filter(|h| h.len() == 16 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .and_then(|h| u64::from_str_radix(h, 16).ok())
            .unwrap_or(0);
        let path = j.get("path").and_then(Json::as_str).unwrap_or_default().to_owned();
        Some(EntityRef { uid: EntityUid { lineage, id }, path })
    }

    /// Whether it is at or below `prefix`: an attached reference by where its entity is now
    /// (an ancestor walk, no allocation), a detached one by its last known path.
    pub fn under(&self, world: &World, prefix: &Prefix<'_>) -> bool {
        if prefix.path == "/" {
            return true;
        }
        match self.attached(world) {
            Some(id) => prefix.entity.is_some_and(|p| world.is_ancestor_or_self(p, id)),
            None => !self.path.is_empty() && path_under(&self.path, prefix.path),
        }
    }
}

/// A `path_prefix` filter resolved once per listing: the entity at the prefix, if any (an
/// attached reference is under the prefix exactly when that entity is its ancestor or itself).
pub struct Prefix<'a> {
    pub path: &'a str,
    entity: Option<u64>,
}

impl<'a> Prefix<'a> {
    pub fn new(world: &World, path: &'a str) -> Prefix<'a> {
        Prefix { path, entity: if path == "/" { None } else { world.find_path(path).ok() } }
    }
}

#[cfg(test)]
mod swarm_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{EntityKey, Shape, SpawnSpec, UpdateSpec};

    fn spawn(w: &mut World, name: &str, parent: Option<u64>) -> u64 {
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some(name.into());
        s.parent = parent.map(EntityKey::Id);
        w.spawn(s, "t").unwrap().id
    }

    #[test]
    fn a_reference_follows_its_entity_by_identity_and_detaches_when_it_goes() {
        let mut w = World::default();
        let a = spawn(&mut w, "a", None);
        let x = spawn(&mut w, "x", Some(a));
        let mut r = EntityRef::of(&w, x).unwrap();
        assert_eq!(r.path, "/a/x");
        w.update(&EntityKey::Id(a), UpdateSpec { name: Some("b".into()), ..UpdateSpec::default() }, "t").unwrap();
        assert_eq!(r.attached(&w), Some(x));
        assert!(r.refresh(&w));
        assert_eq!(r.path, "/b/x");
        assert!(!r.refresh(&w), "nothing new");
        assert!(w.path_is(x, "/b/x") && !w.path_is(x, "/a/x") && !w.path_is(x, "b/x") && !w.path_is(x, "/x") && !w.path_is(x, "//b/x") && !w.path_is(99, ""));
        assert_eq!(r.json(&w).to_string(), r#"{"id":2,"path":"/b/x"}"#);
        // Under: by the entity's place now.
        assert!(r.under(&w, &Prefix::new(&w, "/b")) && !r.under(&w, &Prefix::new(&w, "/a")) && r.under(&w, &Prefix::new(&w, "/")));
        w.delete(&EntityKey::Id(x), None).unwrap();
        assert_eq!(r.attached(&w), None, "a delete detaches it");
        let newcomer = spawn(&mut w, "x", Some(a));
        assert_ne!(newcomer, x);
        assert_eq!(r.attached(&w), None, "never handed to what takes its path");
        assert_eq!(r.json(&w).to_string(), r#"{"path":"/b/x","detached":true}"#);
        assert!(r.under(&w, &Prefix::new(&w, "/b")), "a detached reference filters by its last known path");
    }

    #[test]
    fn saved_references_round_trip_and_older_forms_stay_detached() {
        let mut w = World::default();
        let a = spawn(&mut w, "a", None);
        let r = EntityRef::of(&w, a).unwrap();
        let back = EntityRef::from_saved(&Json::parse(&r.saved_json().to_string()).unwrap()).unwrap();
        assert_eq!((back.uid, back.path.as_str()), (r.uid, "/a"));
        assert_eq!(back.attached(&w), Some(a));
        for old in [r#"{"id":1,"path":"/a"}"#, "1", r#"{"id":1,"born":"zz","path":"/a"}"#, r#"{"id":1,"born":"0000000000000000","path":"/a"}"#] {
            let o = EntityRef::from_saved(&Json::parse(old).unwrap()).unwrap();
            assert_eq!(o.attached(&w), None, "{old}: no identity recorded, so detached");
        }
        assert!(EntityRef::from_saved(&Json::parse(r#"{"path":"/a"}"#).unwrap()).is_none());
        // Probatio: 16 characters that parse as a number but aren't 16 hex digits stay unknown.
        let signed = EntityRef::from_saved(&Json::parse(r#"{"id":1,"born":"+000000000000001","path":"/a"}"#).unwrap()).unwrap();
        assert_eq!(signed.uid.lineage, 0);
    }

    /// Probatio: refreshing a detached reference changes nothing and says so.
    #[test]
    fn a_detached_reference_refreshes_to_nothing() {
        let mut w = World::default();
        let a = spawn(&mut w, "a", None);
        let mut r = EntityRef::of(&w, a).unwrap();
        w.delete(&EntityKey::Id(a), None).unwrap();
        assert!(!r.refresh(&w));
        assert_eq!(r.path, "/a");
    }

    #[test]
    fn missing_ids_are_refused_by_index() {
        let mut w = World::default();
        let a = spawn(&mut w, "a", None);
        let e = EntityRef::of_ids(&w, &[a, 99], "entities").unwrap_err();
        assert_eq!((e.code, e.detail("index").and_then(Json::as_u64)), ("not_found", Some(1)));
    }
}

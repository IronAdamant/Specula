//! Conflict help: a refused write carries what changed, as data, so the agent can rebase instead
//! of re-reading everything and guessing.
//!
//! A `stale_version` error (the entity changed since the agent read it) gets a `conflict` object:
//!
//! - `base`: the entity at the version the agent expected (found in the journal, or in the world
//!   it was loaded from), limited to the fields that changed since;
//! - `theirs`: who changed it since (journal ids, agents, ticks) and those fields' values now;
//! - `yours`: the fields the refused write sets;
//! - `overlap`: fields both sides change. Empty means the agent can retry at once with
//!   `expect_version` = the current version (`rebase` says so); otherwise it must decide.
//!
//! A `claimed` or `claims_overlap` error gets the same shape: `theirs` names the claim's owner and
//! the entity's last change, `base` is its current version (nothing was applied), `yours` the
//! fields the write sets.
//!
//! Values bigger than [`MAX_VALUE_BYTES`] are replaced by their size, so a conflict on a tile map
//! stays small. M4 adds suggested three-way merges for text data.

use crate::journal::Journal;
use crate::world::{Change, Entity, UpdateSpec, World, WorldError};
use specula_core::Json;
use std::collections::BTreeSet;

/// Journal entries examined to find the base version.
const SCAN: usize = 50_000;
/// Later changes listed under `theirs`.
const MAX_THEIRS: usize = 8;
/// A field value larger than this (compact JSON) is summarised.
pub const MAX_VALUE_BYTES: usize = 2048;
/// Entity fields that never count as a change (derived or bookkeeping).
const IGNORED: [&str; 6] = ["id", "version", "created_by", "modified_by", "model_state", "world_position"];

/// What a refused write was going to change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Yours {
    /// Entity fields (`props.<key>` for props keys), or `["(delete)"]`.
    pub fields: Vec<String>,
}

impl Yours {
    pub fn delete() -> Yours {
        Yours { fields: vec!["(delete)".into()] }
    }

    pub fn spawn() -> Yours {
        Yours { fields: vec!["(spawn)".into()] }
    }

    /// The entity fields an update sets.
    pub fn of_update(u: &UpdateSpec) -> Yours {
        let mut f: Vec<String> = Vec::new();
        let mut add = |s: &str| {
            if !f.iter().any(|x| x == s) {
                f.push(s.to_owned());
            }
        };
        if u.name.is_some() {
            add("name");
            add("path");
        }
        if u.parent.is_some() {
            add("parent");
            add("path");
        }
        for (set, name) in [(u.position.is_some(), "position"), (u.rotation_deg.is_some(), "rotation_deg"), (u.scale.is_some(), "scale"), (u.color.is_some(), "color")] {
            if set {
                add(name);
            }
        }
        let l = &u.look;
        for (set, name) in [
            (l.material.is_some(), "material"),
            (l.opacity.is_some(), "opacity"),
            (l.emissive.is_some(), "emissive"),
            (l.metallic.is_some(), "metallic"),
            (l.roughness.is_some(), "roughness"),
            (l.specular.is_some(), "specular"),
            (l.color2.is_some(), "color2"),
            (l.seed.is_some(), "seed"),
        ] {
            if set {
                add(name);
            }
        }
        for (set, name) in [
            (u.light.is_some(), "light"),
            (u.profile.is_some(), "profile"),
            (u.tags.is_some() || !u.add_tags.is_empty() || !u.remove_tags.is_empty(), "tags"),
            (u.collider.is_some(), "collider"),
            (u.body.is_some(), "body"),
            (u.recipe.is_some(), "recipe"),
            (u.params.is_some(), "params"),
            (u.anim.is_some(), "anim"),
            (u.visible.is_some(), "visible"),
            (u.layer.is_some(), "layer"),
            (u.tiles.is_some(), "tilemap"),
        ] {
            if set {
                add(name);
            }
        }
        if let Some(t) = &u.two_d {
            add(t.key());
        }
        match u.props.as_ref().and_then(Json::as_object) {
            Some(pairs) => pairs.iter().for_each(|(k, _)| add(&format!("props.{k}"))),
            None if u.props.is_some() => add("props"),
            None => {}
        }
        Yours { fields: f }
    }
}

/// A field value, or its size when too big.
fn value(v: &Json) -> Json {
    let text = v.to_string();
    if text.len() <= MAX_VALUE_BYTES {
        v.clone()
    } else {
        Json::obj([("omitted_bytes", Json::from(text.len()))])
    }
}

/// `entity.field` (or `props.key`) of an entity listing.
fn field<'a>(e: &'a Json, name: &str) -> Option<&'a Json> {
    match name.strip_prefix("props.") {
        Some(k) => e.get("props").and_then(|p| p.get(k)),
        None => e.get(name),
    }
}

/// Fields that differ between two entity listings (`props` compared key by key).
pub fn changed_fields(a: &Json, b: &Json) -> Vec<String> {
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for e in [a, b] {
        for (k, v) in e.as_object().unwrap_or_default() {
            if IGNORED.contains(&k.as_str()) {
                continue;
            }
            if k == "props" {
                for (pk, _) in v.as_object().unwrap_or_default() {
                    keys.insert(format!("props.{pk}"));
                }
            } else {
                keys.insert(k.clone());
            }
        }
    }
    keys.into_iter().filter(|k| field(a, k) != field(b, k)).collect()
}

/// Whether two field names name the same thing (`props` covers `props.x`).
fn same_field(a: &str, b: &str) -> bool {
    a == b || a.strip_prefix("props.").is_some_and(|_| b == "props") || b.strip_prefix("props.").is_some_and(|_| a == "props")
}

/// The entity at `version` and the path it had then, from the journal (newest first) or its
/// base world; with the journal entry that recorded it.
fn at_version(journal: &Journal, id: u64, version: u64) -> Option<(Entity, String, Option<u64>)> {
    for e in journal.touching(id, SCAN) {
        for c in e.changes.iter().rev().filter(|c| c.id() == id) {
            match c {
                Change::Spawn { after, path } | Change::Set { after, path, .. } if after.version == version => {
                    return Some(((**after).clone(), path.clone(), Some(e.seq)));
                }
                Change::Set { before: Some(b), path, prev_path, .. } if b.version == version => {
                    return Some(((**b).clone(), prev_path.clone().unwrap_or_else(|| path.clone()), Some(e.seq)));
                }
                Change::Delete { before, path } if before.version == version => return Some(((**before).clone(), path.clone(), Some(e.seq))),
                _ => {}
            }
            // Gone past it (game updates skip versions).
            if c.version().is_some_and(|v| v < version) {
                return None;
            }
        }
    }
    let base = journal.base();
    base.get(id).filter(|e| e.version == version).map(|e| (e.clone(), base.path_of(id), None))
}

/// Later changes to `id` (after `version`), newest first.
fn theirs(journal: &Journal, id: u64, version: u64) -> Vec<Json> {
    journal
        .touching(id, SCAN)
        .take_while(|e| e.changes.iter().filter(|c| c.id() == id).any(|c| c.version().is_none_or(|v| v > version)))
        .take(MAX_THEIRS)
        .map(|e| {
            let mut f = vec![("journal", Json::from(e.seq)), ("by", Json::from(&*e.by)), ("tick", Json::from(e.tick)), ("kind", Json::from(e.kind.name()))];
            if let Some(v) = &e.via {
                f.push(("via", Json::from(&**v)));
            }
            Json::obj(f)
        })
        .collect()
}

fn overlap_and_advice(changed: &[String], yours: &Yours, current_version: u64) -> (Json, String) {
    let both: Vec<&String> = yours.fields.iter().filter(|y| changed.iter().any(|c| same_field(c, y)) || y.as_str() == "(delete)").collect();
    let advice = if both.is_empty() {
        format!(
            "none of the fields you set were changed by them: retry with expect_version {current_version} to apply your change on top of theirs"
        )
    } else {
        format!(
            "you and they both change {}: look at theirs (values now) and base, decide, then retry with expect_version {current_version}",
            both.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        )
    };
    (Json::arr(both.into_iter().map(|s| Json::from(s.as_str()))), advice)
}

/// Adds the `conflict` detail to a `stale_version`, `claimed` or `claims_overlap` error (others
/// pass through unchanged). `world` is the live world (the refused write already rolled back).
pub fn help(world: &World, journal: &Journal, e: WorldError, yours: &Yours) -> WorldError {
    match e.code {
        "stale_version" => stale(world, journal, e, yours),
        "claimed" | "claims_overlap" => claimed(world, journal, e, yours),
        _ => e,
    }
}

fn stale(world: &World, journal: &Journal, e: WorldError, yours: &Yours) -> WorldError {
    let (Some(id), Some(expected)) = (e.detail("id").and_then(Json::as_u64), e.detail("expected_version").and_then(Json::as_u64)) else {
        return e;
    };
    let Some(now) = world.get(id) else { return e };
    if now.version == expected {
        // Nothing outside this command changed it: an earlier op of the same batch did (and was
        // rolled back with the rest).
        let advice = "an earlier op in this same batch changed it (each op bumps the version): in the later op expect the \
                      version after your earlier ops on it, or leave expect_version out of the later op"
            .to_owned();
        return e.with(
            "conflict",
            Json::obj([
                ("base", Json::obj([("version", Json::from(expected))])),
                ("theirs", Json::obj([("version", Json::from(expected)), ("changes", Json::arr([]))])),
                ("yours", Json::obj([("fields", Json::arr(yours.fields.iter().map(|f| Json::from(f.as_str()))))])),
                ("same_batch", Json::from(true)),
                ("rebase", Json::from(advice)),
            ]),
        );
    }
    let current = now.to_json(&world.path_of(id));
    let mut conflict = vec![];
    let (changed, base) = match at_version(journal, id, expected) {
        Some((then, path, seq)) => {
            let base_json = then.to_json(&path);
            let changed = changed_fields(&base_json, &current);
            let fields = Json::obj(changed.iter().map(|k| (k.clone(), field(&base_json, k).map_or(Json::Null, value))));
            (changed, Json::obj([("version", Json::from(expected)), ("journal", Json::from(seq)), ("fields", fields)]))
        }
        None => (
            Vec::new(),
            Json::obj([
                ("version", Json::from(expected)),
                ("missing", Json::from("that version is no longer in the journal (evicted, or older than the loaded world): compare with 'current'")),
            ]),
        ),
    };
    let values = Json::obj(changed.iter().map(|k| (k.clone(), field(&current, k).map_or(Json::Null, value))));
    conflict.push(("base", base));
    conflict.push((
        "theirs",
        Json::obj([("version", Json::from(now.version)), ("changes", Json::arr(theirs(journal, id, expected))), ("fields", values)]),
    ));
    conflict.push(("yours", Json::obj([("fields", Json::arr(yours.fields.iter().map(|f| Json::from(f.as_str()))))])));
    let (overlap, advice) = overlap_and_advice(&changed, yours, now.version);
    conflict.push(("overlap", overlap));
    conflict.push(("rebase", Json::from(advice)));
    e.with("conflict", Json::obj(conflict))
}

fn claimed(world: &World, journal: &Journal, e: WorldError, yours: &Yours) -> WorldError {
    let at = e.detail("entity_path").and_then(Json::as_str).map(str::to_owned);
    let entity = at.as_deref().and_then(|p| world.find_path(p).ok()).and_then(|id| world.get(id));
    let mut theirs = vec![
        ("owner", e.detail("owner").cloned().unwrap_or(Json::Null)),
        ("claim", e.detail("claim").cloned().unwrap_or(Json::Null)),
        ("expires_in_s", e.detail("expires_in_s").cloned().unwrap_or(Json::Null)),
    ];
    let mut base = vec![];
    if let Some(ent) = entity {
        theirs.push(("version", Json::from(ent.version)));
        theirs.push(("last_changes", Json::arr(journal.touching(ent.id, SCAN).take(3).map(|j| {
            Json::obj([("journal", Json::from(j.seq)), ("by", Json::from(&*j.by)), ("tick", Json::from(j.tick)), ("kind", Json::from(j.kind.name()))])
        }))));
        base.push(("version", Json::from(ent.version)));
        base.push(("path", Json::from(world.path_of(ent.id))));
    } else {
        base.push(("missing", Json::from("nothing exists there yet (your write would have created it)")));
    }
    let owner = e.detail("owner").and_then(Json::as_str).unwrap_or("its owner").to_owned();
    let advice = format!(
        "nothing was applied: {owner} holds that area. Work elsewhere, ask {owner} to claim_share it with you, or leave a note \
         (note_add) / post a task for them; retry once the claim is shared or released"
    );
    let conflict = Json::obj([
        ("base", Json::obj(base)),
        ("theirs", Json::obj(theirs)),
        ("yours", Json::obj([("fields", Json::arr(yours.fields.iter().map(|f| Json::from(f.as_str()))))])),
        ("rebase", Json::from(advice)),
    ]);
    e.with("conflict", conflict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{self, Kind, Writer};
    use crate::world::{EntityKey, Recording, Shape, SpawnSpec};
    use specula_core::math::Vec3;

    fn world() -> World {
        let mut w = World::default();
        w.set_recording(Recording::Full);
        w
    }

    fn set(w: &mut World, j: &mut Journal, by: &str, u: UpdateSpec) -> Result<u64, WorldError> {
        journal::commit(w, j, 0, Writer::new(by, Some(by)), Kind::Set, None, |w| w.update(&EntityKey::Path("/gate".into()), u, by).map(|e| e.version))
            .map(|(v, _)| v)
    }

    #[test]
    fn a_stale_write_carries_base_theirs_and_yours() {
        let mut w = world();
        let mut j = Journal::default();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("gate".into());
        journal::commit(&mut w, &mut j, 0, Writer::new("ann", Some("ann")), Kind::Spawn, None, |w| w.spawn(s, "ann").map(|_| ())).unwrap();
        // Ann read version 1. Bob moves it and sets a prop; then Ann recolours with expect_version 1.
        let props = Json::parse(r#"{"hp": 3}"#).unwrap();
        set(&mut w, &mut j, "bob", UpdateSpec { position: Some(Vec3::new(4.0, 0.0, 0.0)), props: Some(props), ..Default::default() }).unwrap();
        let mine = UpdateSpec { color: Some([1.0, 0.0, 0.0]), expect_version: Some(1), ..Default::default() };
        let yours = Yours::of_update(&mine);
        let e = set(&mut w, &mut j, "ann", mine).unwrap_err();
        let e = help(&w, &j, e, &yours);
        let c = e.detail("conflict").expect("conflict data");
        assert_eq!(c.get("base").and_then(|b| b.get("version")), Some(&Json::from(1)));
        let base = c.get("base").unwrap().get("fields").unwrap();
        assert_eq!(base.get("position"), Some(&Json::parse("[0,0,0]").unwrap()));
        assert_eq!(base.get("props.hp"), Some(&Json::Null), "the prop didn't exist at the base");
        let theirs = c.get("theirs").unwrap();
        assert_eq!(theirs.get("version"), Some(&Json::from(2)));
        assert_eq!(theirs.get("changes").unwrap().as_array().unwrap()[0].get("by"), Some(&Json::from("bob")));
        assert_eq!(theirs.get("fields").unwrap().get("props.hp"), Some(&Json::from(3)));
        assert_eq!(c.get("overlap"), Some(&Json::arr([])));
        assert!(c.get("rebase").unwrap().as_str().unwrap().contains("expect_version 2"));
        // Moving it too overlaps.
        let mine = UpdateSpec { position: Some(Vec3::ONE), expect_version: Some(1), ..Default::default() };
        let yours = Yours::of_update(&mine);
        let refused = set(&mut w, &mut j, "ann", mine).unwrap_err();
        let e = help(&w, &j, refused, &yours);
        assert_eq!(e.detail("conflict").unwrap().get("overlap"), Some(&Json::arr([Json::from("position")])));
    }

    /// Review finding: an earlier op of the same batch bumped the version; the help must say so
    /// rather than advise retrying with the version that just failed.
    #[test]
    fn a_stale_op_after_an_earlier_op_of_its_batch_says_so() {
        use crate::world::Op;
        let mut w = world();
        let mut j = Journal::default();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("gate".into());
        journal::commit(&mut w, &mut j, 0, Writer::new("ann", Some("ann")), Kind::Spawn, None, |w| w.spawn(s, "ann").map(|_| ())).unwrap();
        let key = || EntityKey::Path("/gate".into());
        let ops = vec![
            Op::Set(key(), UpdateSpec { position: Some(Vec3::ONE), ..Default::default() }),
            Op::Set(key(), UpdateSpec { color: Some([1.0, 0.0, 0.0]), expect_version: Some(1), ..Default::default() }),
        ];
        let e = journal::commit(&mut w, &mut j, 0, Writer::new("ann", Some("ann")), Kind::Batch, None, |w| w.apply_batch(ops, "ann").map_err(|(_, e)| e))
            .unwrap_err();
        let e = help(&w, &j, e, &Yours { fields: vec!["color".into()] });
        let c = e.detail("conflict").unwrap();
        assert_eq!(c.get("same_batch"), Some(&Json::from(true)));
        let advice = c.get("rebase").unwrap().as_str().unwrap();
        assert!(advice.contains("after your earlier ops") && !advice.contains("  "), "{advice}");
    }

    #[test]
    fn update_fields_are_named_like_the_entity_listing() {
        let props = Json::parse(r#"{"a": 1, "b": null}"#).unwrap();
        let u = UpdateSpec { name: Some("x".into()), props: Some(props), add_tags: vec!["t".into()], ..Default::default() };
        assert_eq!(Yours::of_update(&u).fields, ["name", "path", "tags", "props.a", "props.b"]);
        let a = Json::parse(r#"{"id": 1, "version": 3, "position": [0,0,0], "props": {"k": 1}}"#).unwrap();
        let b = Json::parse(r#"{"id": 1, "version": 4, "position": [1,0,0], "props": {"k": 1, "n": 2}}"#).unwrap();
        assert_eq!(changed_fields(&a, &b), ["position", "props.n"]);
        assert!(same_field("props", "props.k") && !same_field("position", "scale"));
        let big = Json::from("x".repeat(MAX_VALUE_BYTES + 10));
        assert!(value(&big).get("omitted_bytes").is_some());
    }
}

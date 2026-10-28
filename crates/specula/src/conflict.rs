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
//! stays small.
//!
//! **M4: a suggested merge.** A `stale_version` refusal whose base version is still known also
//! carries `merge` (see [`crate::merge`]): `props` keys both sides changed are merged three ways
//! (game data is JSON), other fields you set are kept unless they changed and you set them
//! differently (a conflict, which keeps theirs). `merge.apply` is the `world_set` call that
//! writes the merge at the current version: send it as is to accept, or adjust it. Fields the
//! answer can't repeat (materials, colliders...) are listed under `merge.resend`.

use crate::journal::Journal;
use crate::merge::{self, Kind as MergeKind, Prefer};
use crate::world::{Change, Entity, EntityKey, UpdateSpec, World, WorldError};
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
    /// The `props` patch it sets (a merge offer merges it key by key).
    pub props: Option<Json>,
    /// Other fields it sets, as `world_set` arguments (for the merge's `apply`).
    pub values: Vec<(String, Json)>,
    /// Fields it sets that `values` can't repeat (the merge says to send them again).
    pub resend: Vec<String>,
}

impl Yours {
    pub fn delete() -> Yours {
        Yours { fields: vec!["(delete)".into()], ..Yours::default() }
    }

    pub fn spawn() -> Yours {
        Yours { fields: vec!["(spawn)".into()], ..Yours::default() }
    }

    /// Only the fields' names (a refusal that has nothing to merge).
    pub fn named(fields: Vec<String>) -> Yours {
        Yours { fields, ..Yours::default() }
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
            (u.vfx.is_some(), "vfx"),
            (u.decal.is_some(), "decal"),
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
        // The values a merge's apply can repeat.
        let v3 = |v: specula_core::math::Vec3| Json::arr([Json::from(f64::from(v.x)), Json::from(f64::from(v.y)), Json::from(f64::from(v.z))]);
        let mut values: Vec<(String, Json)> = Vec::new();
        if let Some(n) = &u.name {
            values.push(("rename".into(), Json::from(n.as_str())));
        }
        match &u.parent {
            Some(None) => values.push(("parent".into(), Json::Null)),
            Some(Some(EntityKey::Path(p))) => values.push(("parent".into(), Json::from(p.as_str()))),
            Some(Some(EntityKey::Id(id))) => values.push(("parent".into(), Json::from(*id))),
            _ => {}
        }
        for (k, v) in [("position", u.position), ("rotation_deg", u.rotation_deg), ("scale", u.scale)] {
            if let Some(v) = v {
                values.push((k.into(), v3(v)));
            }
        }
        if let Some(c) = u.color {
            values.push(("color".into(), Json::arr(c.iter().map(|x| Json::from(f64::from(*x))))));
        }
        if let Some(t) = &u.tags {
            values.push(("tags".into(), Json::arr(t.iter().map(|x| Json::from(x.as_str())))));
        }
        for (k, list) in [("add_tags", &u.add_tags), ("remove_tags", &u.remove_tags)] {
            if !list.is_empty() {
                values.push((k.into(), Json::arr(list.iter().map(|x| Json::from(x.as_str())))));
            }
        }
        if let Some(v) = u.visible {
            values.push(("visible".into(), Json::Bool(v)));
        }
        let parent_rendered = values.iter().any(|(k, _)| k == "parent");
        let repeated = |name: &str| {
            name.starts_with("props")
                || matches!(name, "name" | "position" | "rotation_deg" | "scale" | "color" | "tags" | "visible")
                || (matches!(name, "parent" | "path") && (parent_rendered || u.name.is_some()))
        };
        let resend = f.iter().filter(|n| !repeated(n)).cloned().collect();
        Yours { fields: f, props: u.props.clone(), values, resend }
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
pub(crate) fn at_version(journal: &Journal, id: u64, version: u64) -> Option<(Entity, String, Option<u64>)> {
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
    let mut offer = None;
    let (changed, base) = match at_version(journal, id, expected) {
        Some((then, path, seq)) => {
            let base_json = then.to_json(&path);
            offer = merge_offer(&base_json, &current, id, now.version, &parent_as_path(world, yours));
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
    if let Some(m) = offer {
        conflict.push(("merge", m));
    }
    e.with("conflict", Json::obj(conflict))
}

/// A merge offered for op `index` of a `world_batch`: its apply is one `world_set`, so it says to
/// send the merged op back inside the batch (sending it alone would drop the batch's other ops).
pub fn in_batch(mut e: WorldError, index: u64) -> WorldError {
    for (k, v) in e.details.iter_mut() {
        if *k != "conflict" {
            continue;
        }
        if let Some(mut m) = v.get("merge").cloned() {
            m.set("batch_op", Json::from(index));
            let next = format!(
                "this was ops[{index}] of a world_batch, and none of the batch was applied: put merge.apply's args back as ops[{index}] (op: set) and send the whole batch again"
            );
            m.set("next", Json::from(next));
            v.set("merge", m);
        }
    }
    e
}

/// `yours` with a parent given by id written as that parent's path, so it compares with the
/// listing (which shows paths) and the merge's apply names it the same way.
fn parent_as_path(world: &World, yours: &Yours) -> Yours {
    let mut y = yours.clone();
    for (k, v) in y.values.iter_mut() {
        if k == "parent" {
            if let Some(id) = v.as_u64().filter(|id| world.get(*id).is_some()) {
                *v = Json::from(world.path_of(id));
            }
        }
    }
    y
}

/// The suggested merge of a stale `world_set` (see the module docs), when there is one to offer.
fn merge_offer(base: &Json, current: &Json, id: u64, now_version: u64, yours: &Yours) -> Option<Json> {
    if yours.fields.is_empty() || yours.fields.iter().any(|f| f.starts_with('(')) {
        return None;
    }
    let mut args = vec![("id".to_owned(), Json::from(id)), ("expect_version".to_owned(), Json::from(now_version))];
    let mut conflicts: Vec<Json> = Vec::new();
    let mut total = 0usize;
    match &yours.props {
        Some(Json::Obj(pairs)) => {
            let mut merged: Vec<(String, Json)> = Vec::new();
            for (k, v) in pairs {
                let name = format!("props.{k}");
                let (b, t) = (field(base, &name), field(current, &name));
                let m = merge::merge3_under(k, b, t, (!v.is_null()).then_some(v), MergeKind::Props, Prefer::Theirs);
                total += m.conflicts_total;
                for c in m.conflicts.iter().take(merge::MAX_CONFLICTS.saturating_sub(conflicts.len())) {
                    let side = |x: &Option<Json>| x.as_ref().map_or(Json::obj([("absent", Json::Bool(true))]), value);
                    let mut j = Json::obj([("base", side(&c.base)), ("theirs", side(&c.theirs)), ("yours", side(&c.yours))]);
                    let path = if c.path.is_empty() {
                        name.clone()
                    } else if c.path.starts_with('[') {
                        format!("{name}{}", c.path)
                    } else {
                        format!("{name}.{}", c.path)
                    };
                    j.set("path", Json::from(path));
                    conflicts.push(j);
                }
                // Write only what differs from theirs (null removes the key).
                if m.merged.as_ref() != t {
                    merged.push((k.clone(), m.merged.unwrap_or(Json::Null)));
                }
            }
            if !merged.is_empty() {
                args.push(("props".to_owned(), Json::Obj(merged)));
            }
        }
        Some(_) => return None,
        None => {}
    }
    for (arg, v) in &yours.values {
        // The listing field the argument sets.
        let listing = match arg.as_str() {
            "rename" => "name",
            "parent" => "path",
            "add_tags" | "remove_tags" => {
                // Adding or removing a tag composes with anything they did.
                args.push((arg.clone(), v.clone()));
                continue;
            }
            other => other,
        };
        let (b, t) = (field(base, listing), field(current, listing));
        let theirs_changed = !match (b, t) {
            (Some(x), Some(y)) => merge::same(x, y),
            (None, None) => true,
            _ => false,
        };
        // Your value as the listing shows it (colours are hex there; a parent shows as the path).
        let as_listed = match (arg.as_str(), v) {
            ("color", Json::Arr(c)) if c.len() == 3 => {
                let f = |i: usize| c[i].as_f64().unwrap_or(0.0) as f32;
                Json::from(crate::world::color_hex([f(0), f(1), f(2)]))
            }
            ("parent", Json::Str(p)) => {
                let renamed = yours.values.iter().find(|(k, _)| k == "rename").and_then(|(_, v)| v.as_str());
                let name = renamed.or_else(|| field(current, "name").and_then(Json::as_str)).unwrap_or("");
                Json::from(format!("{p}/{name}"))
            }
            _ => v.clone(),
        };
        if !theirs_changed {
            args.push((arg.clone(), v.clone()));
        } else if t.is_some_and(|t| merge::same(t, &as_listed)) {
            // They set what you set.
        } else {
            total += 1;
            if conflicts.len() < merge::MAX_CONFLICTS {
                let side = |x: Option<&Json>| x.map_or(Json::obj([("absent", Json::Bool(true))]), value);
                conflicts.push(Json::obj([("path", Json::from(listing)), ("base", side(b)), ("theirs", side(t)), ("yours", value(v))]));
            }
        }
    }
    let mut out = vec![
        ("clean", Json::Bool(total == 0)),
        ("conflicts", Json::Arr(conflicts)),
        ("apply", Json::obj([("tool", Json::from("world_set")), ("args", Json::Obj(args))])),
    ];
    if !yours.resend.is_empty() {
        out.push(("resend", Json::arr(yours.resend.iter().map(|f| Json::from(f.as_str())))));
    }
    let next = if total == 0 {
        "clean: send merge.apply (world_set) to write your change on top of theirs"
    } else {
        "conflicts keep theirs in merge.apply: decide each (conflicts lists base, theirs and yours), adjust apply's args, then send it"
    };
    out.push(("next", Json::from(next)));
    Some(Json::obj(out))
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
        let e = help(&w, &j, e, &Yours::named(vec!["color".into()]));
        let c = e.detail("conflict").unwrap();
        assert_eq!(c.get("same_batch"), Some(&Json::from(true)));
        let advice = c.get("rebase").unwrap().as_str().unwrap();
        assert!(advice.contains("after your earlier ops") && !advice.contains("  "), "{advice}");
    }

    /// M4: a stale write carries a suggested merge whose apply can be sent as is.
    #[test]
    fn a_stale_write_carries_a_merge_to_accept() {
        let mut w = world();
        let mut j = Journal::default();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("gate".into());
        s.props = Json::parse(r#"{"stats": {"hp": 1, "speed": 1}, "tags": ["iron"]}"#).unwrap();
        journal::commit(&mut w, &mut j, 0, Writer::new("ann", Some("ann")), Kind::Spawn, None, |w| w.spawn(s, "ann").map(|_| ())).unwrap();
        let props = |t: &str| Some(Json::parse(t).unwrap());
        set(&mut w, &mut j, "bob", UpdateSpec { props: props(r#"{"stats": {"hp": 3, "speed": 1}, "tags": ["iron", "old"]}"#), position: Some(Vec3::ONE), ..Default::default() }).unwrap();
        // Ann read version 1: she speeds it up, adds a tag and recolours it.
        let mine = UpdateSpec { props: props(r#"{"stats": {"hp": 1, "speed": 2}, "tags": ["iron", "bolted"]}"#), color: Some([1.0, 0.0, 0.0]), expect_version: Some(1), ..Default::default() };
        let yours = Yours::of_update(&mine);
        let refused = set(&mut w, &mut j, "ann", mine).unwrap_err();
        let e = help(&w, &j, refused, &yours);
        let m = e.detail("conflict").unwrap().get("merge").expect("a merge offer");
        assert_eq!(m.get("clean"), Some(&Json::Bool(true)), "{m}");
        let args = m.get("apply").unwrap().get("args").unwrap();
        assert_eq!(args.get("expect_version"), Some(&Json::from(2)));
        let merged = args.get("props").unwrap();
        assert!(crate::merge::same(merged, &Json::parse(r#"{"stats": {"hp": 3, "speed": 2}, "tags": ["iron", "old", "bolted"]}"#).unwrap()), "{merged}");
        assert!(args.get("color").is_some() && args.get("position").is_none());
        // Accepting it: the apply's props and colour on top of Bob's change.
        let accept = UpdateSpec { props: Some(merged.clone()), color: Some([1.0, 0.0, 0.0]), expect_version: Some(2), ..Default::default() };
        assert_eq!(set(&mut w, &mut j, "ann", accept).unwrap(), 3);
        // Both moving it: a conflict that keeps theirs (apply leaves position out).
        set(&mut w, &mut j, "bob", UpdateSpec { position: Some(Vec3::new(5.0, 0.0, 0.0)), ..Default::default() }).unwrap();
        let mine = UpdateSpec { position: Some(Vec3::new(9.0, 0.0, 0.0)), expect_version: Some(3), ..Default::default() };
        let yours = Yours::of_update(&mine);
        let refused = set(&mut w, &mut j, "ann", mine).unwrap_err();
        let e = help(&w, &j, refused, &yours);
        let m = e.detail("conflict").unwrap().get("merge").unwrap();
        assert_eq!(m.get("clean"), Some(&Json::Bool(false)));
        assert_eq!(m.get("conflicts").unwrap().as_array().unwrap()[0].get("path"), Some(&Json::from("position")));
        assert!(m.get("apply").unwrap().get("args").unwrap().get("position").is_none());
        // Fields the answer can't repeat are named.
        let u = UpdateSpec { look: crate::world::LookPatch { roughness: Some(0.5), ..Default::default() }, ..Default::default() };
        assert_eq!(Yours::of_update(&u).resend, ["roughness"]);
        // Integrator follow-up: a parent given by id compares with the listing's path. Bob moves
        // the gate under /yard; Ann (reading an older version) moves it there too, by id.
        let mut yard = SpawnSpec::new(Shape::Group);
        yard.name = Some("yard".into());
        let yard_id = journal::commit(&mut w, &mut j, 0, Writer::new("bob", Some("bob")), Kind::Spawn, None, |w| w.spawn(yard, "bob").map(|e| e.id)).unwrap().0;
        let v = w.get(w.find_path("/gate").unwrap()).unwrap().version;
        set(&mut w, &mut j, "bob", UpdateSpec { parent: Some(Some(EntityKey::Path("/yard".into()))), ..Default::default() }).unwrap_or_else(|_| panic!("bob moves it"));
        let key = EntityKey::Path("/yard/gate".into());
        let mine = UpdateSpec { parent: Some(Some(EntityKey::Id(yard_id))), color: Some([0.0, 0.0, 1.0]), expect_version: Some(v), ..Default::default() };
        let yours = Yours::of_update(&mine);
        let refused = journal::commit(&mut w, &mut j, 0, Writer::new("ann", Some("ann")), Kind::Set, None, |w| w.update(&key, mine, "ann").map(|e| e.version)).unwrap_err();
        let e = help(&w, &j, refused, &yours);
        let m = e.detail("conflict").unwrap().get("merge").unwrap();
        assert_eq!(m.get("clean"), Some(&Json::Bool(true)), "the same parent, by id and by path: no conflict ({m})");
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

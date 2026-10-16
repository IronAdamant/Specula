//! Thin helpers over the engine's in-process [`World`] API, so the presentation reads as "spawn
//! this model here" rather than spec plumbing. World errors here are presentation bugs, not
//! player errors: they are logged and the frame goes on.

use crate::looks::Part;
use specula::world::{EntityKey, Layer, ModelSpec, Shape, SpawnSpec, UpdateSpec, World};
use specula_core::math::Vec3;
use specula_core::Json;
use std::collections::BTreeSet;

fn log(what: &str, e: impl std::fmt::Display) {
    eprintln!("sepulcra: {what}: {e}");
}

/// Spawns anything; returns its id, or 0 when the world refused it (logged).
pub fn spawn(world: &mut World, agent: &str, spec: SpawnSpec) -> u64 {
    let name = spec.name.clone().unwrap_or_default();
    match world.spawn(spec, agent) {
        Ok(e) => e.id,
        Err(e) => {
            log(&format!("spawn '{name}'"), e);
            0
        }
    }
}

/// A model built from a recipe (see `looks`), white so it shows as authored.
#[allow(clippy::too_many_arguments)]
pub fn model_spec(parent: u64, name: &str, model: ModelSpec, pos: Vec3, rot: Vec3, scale: f32, tag_list: &[&str], props: Json) -> SpawnSpec {
    let mut spec = SpawnSpec::model(model);
    spec.name = Some(name.to_owned());
    spec.parent = (parent != 0).then_some(EntityKey::Id(parent));
    spec.position = pos;
    spec.rotation_deg = rot;
    spec.scale = Vec3::new(scale, scale, scale);
    spec.tags = tags(tag_list);
    spec.props = props;
    spec
}

/// A group for game presentation that is never saved, journaled or hashed (moving things:
/// enemies, bolts, flashes, the cursor). Everything spawned below it is transient too.
pub fn transient_group(world: &mut World, agent: &str, name: &str, layer: Layer, tag_list: &[&str]) -> u64 {
    let mut spec = SpawnSpec::new(Shape::Group);
    spec.name = Some(name.to_owned());
    spec.transient = true;
    spec.layer = layer;
    spec.tags = tags(tag_list);
    spawn(world, agent, spec)
}

/// Shows or hides an entity and its subtree.
pub fn show(world: &mut World, agent: &str, id: u64, visible: bool) {
    update(world, agent, id, UpdateSpec { visible: Some(visible), ..UpdateSpec::default() });
}

pub fn tags(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|t| (*t).to_owned()).collect()
}

/// A group (a transform with no mesh). Returns its id, or 0 when the world refused it.
#[allow(clippy::too_many_arguments)]
pub fn group(world: &mut World, agent: &str, parent: Option<u64>, name: &str, pos: Vec3, rot: Vec3, tag_list: &[&str], props: Json) -> u64 {
    let mut spec = SpawnSpec::new(Shape::Group);
    spec.name = Some(name.to_owned());
    spec.parent = parent.map(EntityKey::Id);
    spec.position = pos;
    spec.rotation_deg = rot;
    spec.tags = tags(tag_list);
    spec.props = props;
    match world.spawn(spec, agent) {
        Ok(e) => e.id,
        Err(e) => {
            log(&format!("group '{name}'"), e);
            0
        }
    }
}

/// One part as a child of `parent`.
pub fn part(world: &mut World, agent: &str, parent: u64, p: &Part) -> u64 {
    part_named(world, agent, parent, p.name, p)
}

pub fn part_named(world: &mut World, agent: &str, parent: u64, name: &str, p: &Part) -> u64 {
    let mut spec = SpawnSpec::new(p.shape);
    spec.name = Some(name.to_owned());
    spec.parent = (parent != 0).then_some(EntityKey::Id(parent));
    spec.position = p.pos;
    spec.rotation_deg = p.rot;
    spec.scale = p.scale;
    spec.color = p.color;
    spec.look = p.look;
    spec.light = p.light;
    spec.profile = p.profile.clone();
    match world.spawn(spec, agent) {
        Ok(e) => e.id,
        Err(e) => {
            log(&format!("part '{name}'"), e);
            0
        }
    }
}

/// Moves, turns and scales an entity (fields left `None` stay).
pub fn place(world: &mut World, agent: &str, id: u64, pos: Option<Vec3>, rot: Option<Vec3>, scale: Option<Vec3>) {
    if id == 0 {
        return;
    }
    let change = UpdateSpec { position: pos, rotation_deg: rot, scale, ..UpdateSpec::default() };
    if let Err(e) = world.update(&EntityKey::Id(id), change, agent) {
        log("move", e);
    }
}

/// Any other change.
pub fn update(world: &mut World, agent: &str, id: u64, change: UpdateSpec) {
    if id == 0 {
        return;
    }
    if let Err(e) = world.update(&EntityKey::Id(id), change, agent) {
        log("update", e);
    }
}

pub fn delete(world: &mut World, id: u64) {
    if id == 0 {
        return;
    }
    if let Err(e) = world.delete(&EntityKey::Id(id), None) {
        log("delete", e);
    }
}

/// The id of an existing top-level entity by name, if any.
pub fn top_level(world: &World, name: &str) -> Option<u64> {
    world.find_path(&format!("/{name}")).ok()
}

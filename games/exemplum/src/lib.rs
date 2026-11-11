//! Exemplum (Latin "example"): the gameplay-module demo scene. It builds a small graveyard yard
//! from `scene.json` beside ambulo's slab and drifts its wisps; the four gameplay modules do the
//! rest (docs/design/GAMEPLAY_MODULES.md):
//!
//! - walk with ambulo (WASD / left stick);
//! - talk to the Sexton with colloquium (E, then 1-4): he offers the quest "The Sexton's Bones"
//!   (quaestio) and hands over a lantern (inventarium);
//! - pick up bones with inventarium (E);
//! - banish wisps with the lantern flare from facultas (Q): cyan light, a sound, damage;
//! - bring three bones back: the quest completes, a silver key and the "ward" ability (R) are
//!   the rewards.
//!
//! Load order: `ambulo`, `inventarium`, `facultas`, `colloquium`, `quaestio`, then `exemplum`
//! (`scripts/gmods_check.py` does it with `logic_build`). Everything is in the world under
//! `/exemplum`, so a save, a reload or a replay brings it back as it was.

#![deny(unsafe_code)]

use specula_sdk::{dmath, Cx, HookDef, Id, Module, ModuleInfo, Patch, SdkError, Spawn, StopReason, Value, Vec3};

pub const MAX_ENTITIES: usize = 64;
pub const MAX_WISPS: usize = 16;
const ROOT: &str = "/exemplum";
const SCENE: &str = include_str!("../scene.json");
/// The fields a scene entity may set (passed to spawn as `world_spawn` takes them).
const FIELDS: &[&str] = &["name", "shape", "position", "rotation_deg", "scale", "color", "tags", "props", "emissive", "light", "material", "roughness", "specular"];

fn bad(at: &str, what: &str) -> SdkError {
    SdkError::new("bad_data", format!("scene.json: {at} {what}"))
}

/// The scene's entities, checked (each a `world_spawn` object without a parent).
pub fn scene() -> Result<Vec<Value>, SdkError> {
    let v = Value::parse_json(SCENE)?;
    if v.get("format").and_then(Value::as_str) != Some("specula.exemplum.v1") {
        return Err(bad("format", "must be \"specula.exemplum.v1\""));
    }
    let list = v.get("entities").and_then(Value::as_list).filter(|l| !l.is_empty() && l.len() <= MAX_ENTITIES).ok_or_else(|| bad("entities", &format!("must be a list of 1..{MAX_ENTITIES}")))?;
    let mut names = std::collections::BTreeSet::new();
    for (i, e) in list.iter().enumerate() {
        let m = e.as_map().ok_or_else(|| bad(&format!("entities[{i}]"), "must be an object"))?;
        if let Some(k) = m.keys().find(|k| !FIELDS.contains(&k.as_str())) {
            return Err(bad(&format!("entities[{i}].{k}"), &format!("is not a scene field ({})", FIELDS.join(", "))));
        }
        let name = e.get("name").and_then(Value::as_str).filter(|n| specula_sdk::valid_name(n, 64)).ok_or_else(|| bad(&format!("entities[{i}].name"), "must be a name"))?;
        if !names.insert(name.to_owned()) {
            return Err(bad(&format!("entities[{i}].name"), "is used twice"));
        }
    }
    Ok(list.to_vec())
}

#[derive(Default)]
pub struct Exemplum;

#[cfg(not(feature = "linked"))]
specula_sdk::export_module!(Exemplum);

impl Module for Exemplum {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo {
            name: "exemplum",
            version: env!("CARGO_PKG_VERSION"),
            description: "The gameplay-module demo: a graveyard yard with the Sexton, bones and wisps (scene.json); load after ambulo, inventarium, facultas, colloquium and quaestio.",
            order: 50,
            actions: "",
            hooks: vec![
                HookDef { name: "reset", doc: "Builds the scene again from scene.json (bones and wisps back; the player's own state stays).", args: "" },
                HookDef { name: "status", doc: "Reads the scene: bones still lying about, wisps and their hp (changes nothing).", args: "" },
            ],
        }
    }

    fn start(&self, cx: &mut Cx) -> Result<(), SdkError> {
        let entities = scene()?;
        if cx.find(ROOT)?.is_none() {
            build(cx, &entities)?;
        }
        Ok(())
    }

    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        drift(cx)
    }

    fn hook(&self, cx: &mut Cx, name: &str, _args: &Value) -> Result<Value, SdkError> {
        match name {
            "reset" => {
                let entities = scene()?;
                if let Some(root) = cx.find(ROOT)? {
                    cx.delete(root)?;
                }
                build(cx, &entities)?;
                status(cx)
            }
            "status" => status(cx),
            other => Err(SdkError::new("unknown_hook", format!("exemplum has no hook '{}' (it has: reset, status)", other.chars().take(64).collect::<String>()))),
        }
    }

    fn stop(&self, cx: &mut Cx, reason: StopReason) -> Result<(), SdkError> {
        if reason == StopReason::Unload {
            if let Some(root) = cx.find(ROOT)? {
                cx.delete(root)?;
            }
            cx.set_state("wisps", Value::Null)?;
        }
        Ok(())
    }
}

fn build(cx: &mut Cx, entities: &[Value]) -> Result<(), SdkError> {
    let root = cx.spawn(Spawn::new("group").name("exemplum"))?;
    let mut wisps = Vec::new();
    for e in entities {
        let mut spec = Spawn(e.clone()).field("parent", root.0);
        if spec.0.get("tags").is_none() {
            spec = spec.field("tags", Vec::<Value>::new());
        }
        let id = cx.spawn(spec)?;
        let is_wisp = e.get("tags").and_then(Value::as_list).is_some_and(|t| t.iter().any(|t| t.as_str() == Some("wisp")));
        if is_wisp && wisps.len() < MAX_WISPS {
            let home = e.get("position").and_then(Value::as_vec3).unwrap_or(Vec3::ZERO);
            let phase = e.get("props").and_then(|p| p.get("phase")).and_then(Value::as_f32).unwrap_or(0.0);
            wisps.push(Value::from(vec![Value::from(id.0), Value::from(home), Value::from(phase)]));
        }
    }
    cx.set_state("wisps", wisps)
}

/// Wisps bob and circle their home: a pure function of the tick (dmath), so every run, replay
/// and reload puts them in the same place. Defeated ones stay where they fell (hidden).
fn drift(cx: &mut Cx) -> Result<(), SdkError> {
    let wisps = cx.state("wisps")?;
    let Some(list) = wisps.as_list().filter(|l| !l.is_empty()) else { return Ok(()) };
    let t = cx.tick() as f32 / cx.tick_hz() as f32;
    for w in list {
        let Some([id, home, phase]) = w.as_list() else { continue };
        let (Some(id), Some(home)) = (id.as_u64(), home.as_vec3()) else { continue };
        let phase = phase.as_f32().unwrap_or(0.0);
        let Some(e) = cx.entity(Id(id))? else { continue };
        if e.prop("defeated").as_bool() == Some(true) {
            continue;
        }
        let a = t * 0.7 + phase;
        let at = home + Vec3::new(0.8 * dmath::sin(a), 0.18 * dmath::sin(t * 2.3 + phase), 0.8 * dmath::cos(a));
        cx.set(Id(id), Patch::new().position(at))?;
    }
    Ok(())
}

fn status(cx: &mut Cx) -> Result<Value, SdkError> {
    let mut pickups = Value::map();
    for id in cx.query(Some("pickup"), Some(ROOT), MAX_ENTITIES as u32)? {
        if let Some(e) = cx.entity(id)? {
            let name = e.path().rsplit('/').next().unwrap_or_default().to_owned();
            pickups.insert(&name, e.prop("pickup").clone());
        }
    }
    let mut wisps = Value::map();
    for id in cx.query(Some("wisp"), Some(ROOT), MAX_ENTITIES as u32)? {
        if let Some(e) = cx.entity(id)? {
            let name = e.path().rsplit('/').next().unwrap_or_default().to_owned();
            wisps.insert(&name, e.prop("hp").clone());
        }
    }
    Ok(Value::map().with("pickups", pickups).with("wisps_hp", wisps))
}

#[cfg(test)]
mod tests;

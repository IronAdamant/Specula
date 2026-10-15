//! Per-agent named cameras for sight (`camera_set`, `camera_list`, `camera_delete`,
//! `sight_frame {camera}` and `window_view {camera}`).
//!
//! **Ownership.** A camera belongs to an agent *name*: the name its owner holds (agent names are
//! unique among live sessions; `agent_hello {name, resume_token}` keeps one across sessions).
//! Only the agent holding the name changes or deletes its cameras; any agent may *look through*
//! another's (`sight_frame {camera, camera_of: "<agent name>"}`), which is how a reviewer watches
//! what a builder watches, and a human may look through one in the window
//! (`window_view {camera, of}`).
//!
//! **Lifetime (owner decision, 2026-10-15: "both human developers and AI developers can
//! corroborate easily").** Cameras are **saved with the world**: they live in the [`World`]
//! (`World::cameras`) beside the entities, are written to `cameras.<generation>.json` in the
//! world folder (listed by `world.json`, so they commit with it, crash-safe), and come back with
//! `world_load` / `--world`, keyed by agent name: after a restart a camera belongs to whoever
//! holds that name, and an agent that says `agent_hello` with its name again gets its cameras
//! back. They are **not** world state for determinism: [`World::state_hash`] leaves them out, so
//! moving a camera never changes a replay or a desync check. Every `camera_set` / `camera_delete`
//! is journaled (kind `camera`, a `Change::Camera` at path `@camera/<owner>/<name>`), so
//! `world_changes` shows other agents what moved and `world_undo` puts a camera back.
//!
//! **Following.** A camera may follow an entity by path: eye = entity origin + `offset`, target =
//! entity origin + `look_offset` (world axes, or the entity's own axes with `local`). The path is
//! re-resolved at every use, so a re-spawned unit at the same path is followed again; when it is
//! gone the camera stays where it last saw it (with a warning).

use crate::sight::Camera;
use crate::world::{vec3_json, World};
use specula_core::math::Vec3;
use specula_core::Json;
use std::collections::BTreeMap;

/// Cameras one agent may hold.
pub const MAX_CAMERAS_PER_AGENT: usize = 32;
/// Agent names that may hold cameras in one world.
pub const MAX_OWNERS: usize = 1024;

/// What `camera_set` asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraSpec {
    pub name: String,
    pub eye: Option<Vec3>,
    pub target: Option<Vec3>,
    pub fov_y_deg: f32,
    pub follow: Option<Follow>,
}

/// Follow an entity (see the module docs).
#[derive(Clone, Debug, PartialEq)]
pub struct Follow {
    pub path: String,
    pub offset: Vec3,
    pub look_offset: Vec3,
    /// Offsets turn with the entity (its rotation, not its scale).
    pub local: bool,
}

impl Follow {
    pub const DEFAULT_OFFSET: Vec3 = Vec3 { x: 0.0, y: 4.0, z: 6.0 };
}

/// A camera as the world keeps (and saves) it.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedCamera {
    pub spec: CameraSpec,
    /// Bumped on every `camera_set`.
    pub version: u64,
    /// Saved with the world: cameras of agents that never said `agent_hello` (no name to come
    /// back as) live for the engine run only.
    pub persisted: bool,
}

impl NamedCamera {
    /// The form `camera_set` takes (plus `version`).
    pub fn to_json(&self) -> Json {
        let s = &self.spec;
        let mut fields = vec![("name", Json::from(s.name.as_str())), ("fov_y_deg", Json::from(s.fov_y_deg))];
        match &s.follow {
            Some(f) => fields.extend([
                ("follow", Json::from(f.path.as_str())),
                ("offset", vec3_json(f.offset)),
                ("look_offset", vec3_json(f.look_offset)),
                ("local", Json::from(f.local)),
            ]),
            None => {
                if let (Some(e), Some(t)) = (s.eye, s.target) {
                    fields.extend([("eye", vec3_json(e)), ("target", vec3_json(t))]);
                }
            }
        }
        fields.push(("version", Json::from(self.version)));
        Json::obj(fields)
    }

    /// From [`NamedCamera::to_json`]'s form (a saved camera), validated like `camera_set`.
    pub fn from_json(j: &Json) -> Result<NamedCamera, String> {
        let version = j.get("version").and_then(Json::as_u64).filter(|v| *v >= 1).ok_or("'version' must be a positive integer")?;
        let mut args = j.clone();
        if let Json::Obj(o) = &mut args {
            o.retain(|(k, _)| k != "version");
        }
        let spec = crate::sight_tools::parse_camera_set(&args)?;
        Ok(NamedCamera { spec, version, persisted: true })
    }
}

/// Error from the camera store: `code` is machine-readable.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraError {
    pub code: &'static str,
    pub message: String,
}

impl CameraError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> CameraError {
        CameraError { code, message: message.into() }
    }

    pub fn to_json(&self) -> Json {
        Json::obj([("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))])
    }
}

/// What the engine keeps about cameras beyond the world's: for each follow camera, the entity
/// its path resolved to (re-resolved only when the world's layout changes, so a window following
/// one every frame allocates nothing) and its last view (shown when the entity is gone). Not
/// saved: a reloaded follow camera resolves afresh.
#[derive(Default)]
pub struct Cameras {
    follows: BTreeMap<String, BTreeMap<String, Followed>>,
}

/// A follow camera's resolution (see [`Cameras`]).
#[derive(Clone, Copy, Debug)]
struct Followed {
    /// The entity at its path as of `epoch` (`None`: no entity there).
    entity: Option<u64>,
    /// [`World::layout_epoch`] when `entity` was looked up (`u64::MAX`: never).
    epoch: u64,
    /// The last view resolved through it.
    last: Option<Camera>,
}

/// A camera resolved for one frame.
pub struct Resolved {
    pub camera: Camera,
    /// e.g. the followed entity is gone.
    pub warning: Option<String>,
    /// The camera's version (bumped by every `camera_set`).
    pub version: u64,
}

/// The cameras agent `owner` holds in `world`, in name order.
pub fn list<'w>(world: &'w World, owner: &str) -> Vec<&'w NamedCamera> {
    world.cameras().get(owner).map(|c| c.values().collect()).unwrap_or_default()
}

/// Owners and their camera names, at most `limit` owners (name order): `(json, omitted)`.
pub fn list_all(world: &World, limit: usize) -> (Vec<Json>, usize) {
    let owners: Vec<(&String, &BTreeMap<String, NamedCamera>)> = world.cameras().iter().filter(|(_, c)| !c.is_empty()).collect();
    let omitted = owners.len().saturating_sub(limit);
    let out = owners
        .into_iter()
        .take(limit)
        .map(|(owner, cams)| Json::obj([("owner", Json::from(owner.as_str())), ("cameras", Json::arr(cams.keys().map(|k| Json::from(k.as_str()))))]))
        .collect();
    (out, omitted)
}

impl Cameras {
    /// The view through camera `name` of agent `owner` in `world` now (`of`: the owner's name
    /// for messages, when it isn't the caller). Follow cameras remember what they resolved to.
    pub fn resolve(&mut self, owner: &str, name: &str, of: Option<&str>, world: &World) -> Result<Resolved, CameraError> {
        let cam = world.cameras().get(owner).and_then(|c| c.get(name)).ok_or_else(|| not_found(name, of))?;
        let version = cam.version;
        let spec = &cam.spec;
        let Some(f) = &spec.follow else {
            let (eye, target) = (spec.eye.expect("checked at set"), spec.target.expect("checked at set"));
            return Ok(Resolved { camera: Camera::new(eye, target, spec.fov_y_deg), warning: None, version });
        };
        // Borrowed lookups; an entry is made (allocating) only the first time.
        if !self.follows.get(owner).is_some_and(|m| m.contains_key(name)) {
            let fresh = Followed { entity: None, epoch: u64::MAX, last: None };
            self.follows.entry(owner.to_owned()).or_default().insert(name.to_owned(), fresh);
        }
        let slot = self.follows.get_mut(owner).and_then(|m| m.get_mut(name)).expect("made above");
        if slot.epoch != world.layout_epoch() {
            slot.entity = world.find_path(&f.path).ok();
            slot.epoch = world.layout_epoch();
        }
        match slot.entity.and_then(|id| world.get(id)) {
            Some(e) => {
                let m = e.model();
                let origin = m.transform_point(Vec3::ZERO);
                let turn = |v: Vec3| -> Vec3 {
                    if !f.local {
                        return v;
                    }
                    // The rotation part of the world matrix: each axis column normalised.
                    let c = &m.cols;
                    let axis = |i: usize| Vec3::new(c[i][0], c[i][1], c[i][2]).normalize();
                    axis(0) * v.x + axis(1) * v.y + axis(2) * v.z
                };
                let (eye, target) = (origin + turn(f.offset), origin + turn(f.look_offset));
                let target = if (target - eye).length() < 1e-4 { target + Vec3::new(0.0, 0.0, -1.0) } else { target };
                let camera = Camera::new(eye, target, spec.fov_y_deg);
                slot.last = Some(camera);
                Ok(Resolved { camera, warning: None, version })
            }
            None => match slot.last {
                Some(camera) => Ok(Resolved {
                    version,
                    camera,
                    warning: Some(format!(
                        "camera '{name}' follows {} which no longer exists; it shows where it last saw it",
                        f.path
                    )),
                }),
                None => Err(CameraError::new(
                    "follow_target_missing",
                    format!("camera '{name}' follows {}, which does not exist (and never did while followed)", f.path),
                )),
            },
        }
    }

    /// Forgets remembered views of cameras the world no longer has (after a delete, an undo or a
    /// load), so the cache stays bounded by the world's cameras.
    pub fn prune(&mut self, world: &World) {
        let cams = world.cameras();
        self.follows.retain(|owner, m| {
            m.retain(|name, _| cams.get(owner).is_some_and(|c| c.contains_key(name)));
            !m.is_empty()
        });
    }
}

pub(crate) fn not_found(name: &str, of: Option<&str>) -> CameraError {
    let whose = of.map_or("you have".to_owned(), |a| format!("agent '{a}' has"));
    CameraError::new("camera_not_found", format!("{whose} no camera named '{name}' (camera_list shows them)"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{EntityKey, Recording, Shape, SpawnSpec, UpdateSpec};

    trait SetT {
        fn set_camera_t(&mut self, owner: &str, spec: CameraSpec) -> Result<&NamedCamera, crate::world::WorldError>;
    }

    impl SetT for World {
        fn set_camera_t(&mut self, owner: &str, spec: CameraSpec) -> Result<&NamedCamera, crate::world::WorldError> {
            self.set_camera(owner, spec, true)
        }
    }

    fn fixed(name: &str, eye: [f32; 3]) -> CameraSpec {
        CameraSpec {
            name: name.into(),
            eye: Some(Vec3::new(eye[0], eye[1], eye[2])),
            target: Some(Vec3::ZERO),
            fov_y_deg: 50.0,
            follow: None,
        }
    }

    #[test]
    fn cameras_belong_to_their_owner() {
        let mut world = World::default();
        let mut c = Cameras::default();
        world.set_camera_t("ann", fixed("overview", [0.0, 10.0, 10.0])).unwrap();
        world.set_camera_t("bob", fixed("overview", [5.0, 5.0, 5.0])).unwrap();
        let ann = c.resolve("ann", "overview", None, &world).unwrap().camera;
        let bob = c.resolve("bob", "overview", None, &world).unwrap().camera;
        assert_eq!(ann.eye, Vec3::new(0.0, 10.0, 10.0));
        assert_eq!(bob.eye, Vec3::new(5.0, 5.0, 5.0), "same name, separate cameras");
        // Anyone may look through another's camera (by that agent's name).
        let through = c.resolve("ann", "overview", Some("ann"), &world).unwrap();
        assert_eq!((through.camera.eye, through.version), (ann.eye, 1));
        assert_eq!(world.delete_cameras("bob", Some("overview")).unwrap(), 1);
        assert!(c.resolve("ann", "overview", None, &world).is_ok(), "bob deleted his own, not ann's");
        assert_eq!(c.resolve("bob", "overview", None, &world).err().unwrap().code, "camera_not_found");
        let e = c.resolve("nobody", "x", Some("nobody"), &world).err().unwrap();
        assert!(e.code == "camera_not_found" && e.message.contains("agent 'nobody'"), "{e:?}");
        assert_eq!(world.delete_cameras("bob", Some("gone")).unwrap_err().code, "camera_not_found");
        // Replacing bumps the version; the list is in name order and round-trips camera_set's form.
        world.set_camera_t("ann", fixed("overview", [1.0, 2.0, 3.0])).unwrap();
        world.set_camera_t("ann", fixed("a_close", [1.0, 1.0, 1.0])).unwrap();
        let listed = list(&world, "ann");
        assert_eq!(listed.iter().map(|n| n.spec.name.as_str()).collect::<Vec<_>>(), ["a_close", "overview"]);
        assert_eq!(listed[1].version, 2);
        assert_eq!(listed[1].to_json().get("eye"), Some(&vec3_json(Vec3::new(1.0, 2.0, 3.0))));
        assert_eq!(NamedCamera::from_json(&listed[1].to_json()).unwrap(), *listed[1], "saved form reads back exactly");
        assert_eq!(list(&world, "nobody").len(), 0);
        let (all, omitted) = list_all(&world, 10);
        assert_eq!((all.len(), omitted), (1, 0), "ann only; bob has none left");
        assert_eq!(world.delete_cameras("ann", None).unwrap(), 2);
    }

    #[test]
    fn cameras_are_bounded_and_not_world_state() {
        let mut world = World::default();
        let hash = world.state_hash();
        let revision = world.revision();
        for i in 0..MAX_CAMERAS_PER_AGENT {
            world.set_camera_t("ann", fixed(&format!("c{i}"), [1.0, 1.0, 1.0])).unwrap();
        }
        assert_eq!(world.set_camera_t("ann", fixed("one_more", [1.0, 1.0, 1.0])).err().unwrap().code, "camera_limit");
        assert!(world.set_camera_t("ann", fixed("c3", [2.0, 1.0, 1.0])).is_ok(), "replacing is always allowed");
        assert_eq!(world.state_hash(), hash, "cameras never change the state hash (replays, desync checks)");
        assert!(world.revision() > revision, "but they are changes to save");
    }

    #[test]
    fn camera_changes_are_recorded_and_revert() {
        let mut world = World::default();
        world.set_recording(Recording::Full);
        world.set_camera_t("ann", fixed("overview", [0.0, 10.0, 10.0])).unwrap();
        world.set_camera_t("ann", fixed("overview", [0.0, 5.0, 5.0])).unwrap();
        world.delete_cameras("ann", Some("overview")).unwrap();
        let changes = world.take_changes();
        let ops: Vec<&str> = changes.iter().map(|c| c.op()).collect();
        assert_eq!(ops, ["camera_set", "camera_set", "camera_delete"]);
        assert_eq!(changes[0].path(), "@camera/ann/overview");
        assert_eq!(changes[1].version(), Some(2));
        // The second set's before is the first's after: putting it back restores version 1.
        let mut replayed = World::default();
        for c in &changes[..2] {
            replayed.apply_after(c, "ann");
        }
        assert_eq!(list(&replayed, "ann")[0].spec.eye, Some(Vec3::new(0.0, 5.0, 5.0)));
        replayed.apply_after(&changes[2], "ann");
        assert!(list(&replayed, "ann").is_empty());
    }

    #[test]
    fn follow_cameras_track_and_remember() {
        let mut world = World::default();
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some("runner".into());
        s.position = Vec3::new(10.0, 0.0, 0.0);
        world.spawn(s, "t").unwrap();
        let mut c = Cameras::default();
        let follow = Follow { path: "/runner".into(), offset: Vec3::new(0.0, 2.0, 5.0), look_offset: Vec3::ZERO, local: false };
        world.set_camera_t("ann", CameraSpec { name: "chase".into(), eye: None, target: None, fov_y_deg: 60.0, follow: Some(follow) }).unwrap();
        let r = c.resolve("ann", "chase", None, &world).unwrap();
        assert_eq!((r.camera.eye, r.camera.target), (Vec3::new(10.0, 2.0, 5.0), Vec3::new(10.0, 0.0, 0.0)));
        // It moves: the camera moves with it.
        let key = EntityKey::Path("/runner".into());
        let turned = UpdateSpec { position: Some(Vec3::new(20.0, 0.0, 0.0)), rotation_deg: Some(Vec3::new(0.0, 90.0, 0.0)), ..Default::default() };
        world.update(&key, turned, "t").unwrap();
        let r = c.resolve("ann", "chase", None, &world).unwrap();
        assert_eq!(r.camera.eye, Vec3::new(20.0, 2.0, 5.0));
        // Local offsets turn with it: +Z behind becomes +X after a 90 degree yaw.
        let local = Follow { path: "/runner".into(), offset: Vec3::new(0.0, 0.0, 5.0), look_offset: Vec3::ZERO, local: true };
        world.set_camera_t("ann", CameraSpec { name: "chase".into(), eye: None, target: None, fov_y_deg: 60.0, follow: Some(local) }).unwrap();
        let eye = c.resolve("ann", "chase", None, &world).unwrap().camera.eye;
        assert!((eye - Vec3::new(25.0, 0.0, 0.0)).length() < 1e-4, "{eye:?}");
        // Gone: the last view, with a warning.
        world.delete(&key, None).unwrap();
        let r = c.resolve("ann", "chase", None, &world).unwrap();
        assert!((r.camera.eye - eye).length() < 1e-6 && r.warning.unwrap().contains("no longer exists"));
        let never = Follow { path: "/ghost".into(), offset: Vec3::ONE, look_offset: Vec3::ZERO, local: false };
        world.set_camera_t("ann", CameraSpec { name: "g".into(), eye: None, target: None, fov_y_deg: 60.0, follow: Some(never) }).unwrap();
        assert_eq!(c.resolve("ann", "g", None, &world).err().unwrap().code, "follow_target_missing");
        // Deleted cameras' remembered views go.
        world.delete_cameras("ann", None).unwrap();
        c.prune(&world);
        assert!(c.follows.is_empty());
    }
}

//! Per-agent named cameras for sight (`camera_set`, `camera_list`, `camera_delete`, and
//! `sight_frame {camera}`).
//!
//! **Ownership.** A camera belongs to the identity that set it: the agent's identity key (see
//! [`crate::swarm::Swarm::key`]), which `agent_hello {resume_token}` carries over, so a resumed
//! agent keeps its cameras. Only the owner may change or delete a camera; any agent may *look
//! through* another's camera (`sight_frame {camera, camera_of: "<agent name>"}`), which is how a
//! reviewer watches what a builder watches.
//!
//! **Lifetime (decided).** Cameras live for the engine run and are **not** saved with the world:
//! world files hold the world's state (they feed the state hash and the journal), cameras are an
//! agent's tooling, and identity keys don't outlive the engine anyway. `camera_list` returns
//! each camera in the exact form `camera_set` takes, so an agent re-creates its rig in one batch
//! after a restart.
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
/// Identities whose cameras are kept (the least recently changed is forgotten first).
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

#[derive(Clone, Debug)]
pub struct NamedCamera {
    pub spec: CameraSpec,
    /// A follow camera's last resolved view (used when its entity is gone).
    pub last: Option<Camera>,
    /// Bumped on every `camera_set`.
    pub version: u64,
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
}

/// One identity's cameras.
struct Owned {
    /// The owner's agent name when it last set a camera (for `camera_of` and listings).
    name: String,
    cameras: BTreeMap<String, NamedCamera>,
    /// Ordering stamp for forgetting owners (`MAX_OWNERS`).
    touched: u64,
}

/// Error from the camera store: `code` is machine-readable.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraError {
    pub code: &'static str,
    pub message: String,
}

impl CameraError {
    fn new(code: &'static str, message: impl Into<String>) -> CameraError {
        CameraError { code, message: message.into() }
    }

    pub fn to_json(&self) -> Json {
        Json::obj([("error", Json::from(self.code)), ("message", Json::from(self.message.as_str()))])
    }
}

/// Every agent's cameras, keyed by identity key.
#[derive(Default)]
pub struct Cameras {
    owners: BTreeMap<String, Owned>,
    stamp: u64,
}

/// A camera resolved for one frame.
pub struct Resolved {
    pub camera: Camera,
    /// e.g. the followed entity is gone.
    pub warning: Option<String>,
    /// The camera's version (bumped by every `camera_set`).
    pub version: u64,
}

impl Cameras {
    /// Creates or replaces `spec.name` for `key` (agent `name`).
    pub fn set(&mut self, key: &str, name: &str, spec: CameraSpec) -> Result<&NamedCamera, CameraError> {
        self.stamp += 1;
        let stamp = self.stamp;
        if !self.owners.contains_key(key) && self.owners.len() >= MAX_OWNERS {
            // Forget the identity that changed its cameras longest ago.
            if let Some(oldest) = self.owners.iter().min_by_key(|(_, o)| o.touched).map(|(k, _)| k.clone()) {
                self.owners.remove(&oldest);
            }
        }
        let owned = self
            .owners
            .entry(key.to_owned())
            .or_insert_with(|| Owned { name: name.to_owned(), cameras: BTreeMap::new(), touched: stamp });
        owned.name = name.to_owned();
        owned.touched = stamp;
        if !owned.cameras.contains_key(&spec.name) && owned.cameras.len() >= MAX_CAMERAS_PER_AGENT {
            return Err(CameraError::new(
                "camera_limit",
                format!("at most {MAX_CAMERAS_PER_AGENT} cameras per agent; camera_delete one first"),
            ));
        }
        let version = owned.cameras.get(&spec.name).map_or(1, |c| c.version + 1);
        let cam_name = spec.name.clone();
        owned.cameras.insert(cam_name.clone(), NamedCamera { spec, last: None, version });
        Ok(&owned.cameras[&cam_name])
    }

    /// Deletes `name` (or every camera with `name: None`); returns how many went.
    pub fn delete(&mut self, key: &str, name: Option<&str>) -> Result<usize, CameraError> {
        let Some(owned) = self.owners.get_mut(key) else {
            return match name {
                Some(n) => Err(not_found(n, None)),
                None => Ok(0),
            };
        };
        match name {
            Some(n) => owned.cameras.remove(n).map(|_| 1).ok_or_else(|| not_found(n, None)),
            None => {
                let n = owned.cameras.len();
                owned.cameras.clear();
                Ok(n)
            }
        }
    }

    /// The identity key of the agent called `agent` (the one that most recently set a camera).
    /// The identity `key`'s cameras, in name order.
    pub fn list(&self, key: &str) -> Vec<&NamedCamera> {
        self.owners.get(key).map(|o| o.cameras.values().collect()).unwrap_or_default()
    }

    /// Owners and their camera names, at most `limit` owners: `(json, omitted)`.
    pub fn list_all(&self, limit: usize) -> (Vec<Json>, usize) {
        let mut owners: Vec<&Owned> = self.owners.values().filter(|o| !o.cameras.is_empty()).collect();
        owners.sort_by(|a, b| a.name.cmp(&b.name));
        let omitted = owners.len().saturating_sub(limit);
        let out = owners
            .into_iter()
            .take(limit)
            .map(|o| {
                Json::obj([
                    ("owner", Json::from(o.name.as_str())),
                    ("cameras", Json::arr(o.cameras.keys().map(|k| Json::from(k.as_str())))),
                ])
            })
            .collect();
        (out, omitted)
    }

    /// The view through camera `name` of the identity `key` in `world` now (`of`: that agent's
    /// name, for messages, when it isn't the caller). Callers resolve another agent's name to its
    /// *current* identity (the swarm's), never to whoever used the name before. Follow cameras
    /// remember what they resolved to.
    pub fn resolve(&mut self, key: &str, name: &str, of: Option<&str>, world: &World) -> Result<Resolved, CameraError> {
        let cam = self.owners.get_mut(key).and_then(|o| o.cameras.get_mut(name)).ok_or_else(|| not_found(name, of))?;
        let version = cam.version;
        let spec = &cam.spec;
        let Some(f) = &spec.follow else {
            let (eye, target) = (spec.eye.expect("checked at set"), spec.target.expect("checked at set"));
            return Ok(Resolved { camera: Camera { eye, target, fov_y_deg: spec.fov_y_deg }, warning: None, version });
        };
        match world.find_path(&f.path).ok().and_then(|id| world.get(id)) {
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
                let camera = Camera { eye, target, fov_y_deg: spec.fov_y_deg };
                cam.last = Some(camera);
                Ok(Resolved { camera, warning: None, version })
            }
            None => match cam.last {
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
}

fn not_found(name: &str, of: Option<&str>) -> CameraError {
    let whose = of.map_or("you have".to_owned(), |a| format!("agent '{a}' has"));
    CameraError::new("camera_not_found", format!("{whose} no camera named '{name}' (camera_list shows them)"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Shape, SpawnSpec, UpdateSpec, EntityKey};

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
        let mut c = Cameras::default();
        let world = World::default();
        c.set("key-ann", "ann", fixed("overview", [0.0, 10.0, 10.0])).unwrap();
        c.set("key-bob", "bob", fixed("overview", [5.0, 5.0, 5.0])).unwrap();
        let ann = c.resolve("key-ann", "overview", None, &world).unwrap().camera;
        let bob = c.resolve("key-bob", "overview", None, &world).unwrap().camera;
        assert_eq!(ann.eye, Vec3::new(0.0, 10.0, 10.0));
        assert_eq!(bob.eye, Vec3::new(5.0, 5.0, 5.0), "same name, separate cameras");
        // Anyone may look through another's camera (by that agent's identity); nobody may
        // delete it.
        let through = c.resolve("key-ann", "overview", Some("ann"), &world).unwrap();
        assert_eq!((through.camera.eye, through.version), (ann.eye, 1));
        assert_eq!(c.delete("key-bob", Some("overview")).unwrap(), 1);
        assert!(c.resolve("key-ann", "overview", None, &world).is_ok(), "bob deleted his own, not ann's");
        assert_eq!(c.resolve("key-bob", "overview", None, &world).err().unwrap().code, "camera_not_found");
        let e = c.resolve("key-nobody", "x", Some("nobody"), &world).err().unwrap();
        assert!(e.code == "camera_not_found" && e.message.contains("agent 'nobody'"), "{e:?}");
        // Reviewer finding: a later session taking the name "ann" is a different identity; its
        // camera never answers for the old ann's (callers resolve names through the swarm).
        c.set("key-impostor", "ann", fixed("overview", [99.0, 5.0, 5.0])).unwrap();
        assert_eq!(c.resolve("key-ann", "overview", Some("ann"), &world).unwrap().camera.eye, ann.eye);
        // Replacing bumps the version; the list is in name order and round-trips camera_set's form.
        c.set("key-ann", "ann", fixed("overview", [1.0, 2.0, 3.0])).unwrap();
        c.set("key-ann", "ann", fixed("a_close", [1.0, 1.0, 1.0])).unwrap();
        let list = c.list("key-ann");
        assert_eq!(list.iter().map(|n| n.spec.name.as_str()).collect::<Vec<_>>(), ["a_close", "overview"]);
        assert_eq!(list[1].version, 2);
        assert_eq!(list[1].to_json().get("eye"), Some(&vec3_json(Vec3::new(1.0, 2.0, 3.0))));
        assert_eq!(c.list("key-nobody").len(), 0);
        let (all, omitted) = c.list_all(10);
        assert_eq!((all.len(), omitted), (2, 0), "ann and the impostor; bob has none left");
        assert_eq!(c.delete("key-ann", None).unwrap(), 2);
    }

    #[test]
    fn cameras_per_agent_are_capped() {
        let mut c = Cameras::default();
        for i in 0..MAX_CAMERAS_PER_AGENT {
            c.set("k", "ann", fixed(&format!("c{i}"), [1.0, 1.0, 1.0])).unwrap();
        }
        assert_eq!(c.set("k", "ann", fixed("one_more", [1.0, 1.0, 1.0])).err().unwrap().code, "camera_limit");
        assert!(c.set("k", "ann", fixed("c3", [2.0, 1.0, 1.0])).is_ok(), "replacing is always allowed");
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
        c.set("k", "ann", CameraSpec { name: "chase".into(), eye: None, target: None, fov_y_deg: 60.0, follow: Some(follow) }).unwrap();
        let r = c.resolve("k", "chase", None, &world).unwrap();
        assert_eq!((r.camera.eye, r.camera.target), (Vec3::new(10.0, 2.0, 5.0), Vec3::new(10.0, 0.0, 0.0)));
        // It moves: the camera moves with it.
        let key = EntityKey::Path("/runner".into());
        let turned = UpdateSpec { position: Some(Vec3::new(20.0, 0.0, 0.0)), rotation_deg: Some(Vec3::new(0.0, 90.0, 0.0)), ..Default::default() };
        world.update(&key, turned, "t").unwrap();
        let r = c.resolve("k", "chase", None, &world).unwrap();
        assert_eq!(r.camera.eye, Vec3::new(20.0, 2.0, 5.0));
        // Local offsets turn with it: +Z behind becomes +X after a 90 degree yaw.
        let local = Follow { path: "/runner".into(), offset: Vec3::new(0.0, 0.0, 5.0), look_offset: Vec3::ZERO, local: true };
        c.set("k", "ann", CameraSpec { name: "chase".into(), eye: None, target: None, fov_y_deg: 60.0, follow: Some(local) }).unwrap();
        let eye = c.resolve("k", "chase", None, &world).unwrap().camera.eye;
        assert!((eye - Vec3::new(25.0, 0.0, 0.0)).length() < 1e-4, "{eye:?}");
        // Gone: the last view, with a warning.
        world.delete(&key, None).unwrap();
        let r = c.resolve("k", "chase", None, &world).unwrap();
        assert!((r.camera.eye - eye).length() < 1e-6 && r.warning.unwrap().contains("no longer exists"));
        let never = Follow { path: "/ghost".into(), offset: Vec3::ONE, look_offset: Vec3::ZERO, local: false };
        c.set("k", "ann", CameraSpec { name: "g".into(), eye: None, target: None, fov_y_deg: 60.0, follow: Some(never) }).unwrap();
        assert_eq!(c.resolve("k", "g", None, &world).err().unwrap().code, "follow_target_missing");
    }
}

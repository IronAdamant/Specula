//! Joints in the engine (P4; docs/design/PHYSICS.md): joint entities and their sync into the
//! space.
//!
//! - **A joint is an entity**: usually a Group under one of its bodies, whose `props.joint` holds
//!   the joint as data ([`JointSpec`]): its kind, the two bodies (entity ids; `a: null` is the
//!   world), a frame on each (an anchor, an axis and a reference normal, in each body's own
//!   local, scaled coordinates, like collider offsets), limits, a motor and whether the two still
//!   collide. So joints are journaled, undoable, claimable, saved and hashed like everything
//!   else. `physics_joint` writes them from world-space anchors and axes; `world_set {props}`
//!   can too (physics checks them and lists bad ones under `not_simulated` with why).
//! - **Sync.** Each tick, joint entities that changed (and the joints of bodies that changed:
//!   their frames follow a body's scale) are rebuilt into the space; unchanged ones keep their
//!   warm start. A joint whose body is missing waits (inactive, `missing_body`).
//! - Joint keys at and above [`SYNTHETIC`] are the engine's own (ragdoll bones), never entities.

use super::*;
use specula_physics::joint::{frame, Drive, JointDesc, JointKind, Motor};

/// The props key that makes an entity a joint.
pub const PROP: &str = "joint";
/// Joint (and body) keys at and above this are made by the engine (ragdolls), not entities.
pub const SYNTHETIC: u64 = 1 << 63;
/// Largest motor force (N or N·m).
pub const MAX_FORCE: f32 = 1.0e7;

/// A joint motor as data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotorDrive {
    /// Hinge: degrees per second; prismatic: m/s.
    Speed(f32),
    /// Hinge: degrees; prismatic: metres along the axis.
    Position(f32),
    /// Ball and fixed joints: B's frame in A's frame (degrees, X then Y then Z).
    Rotation(Vec3),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotorSpec {
    pub drive: MotorDrive,
    pub max_force: f32,
    pub hertz: f32,
    pub damping: f32,
}

/// A joint, as `props.joint` holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct JointSpec {
    pub kind: JointKind,
    /// Entity ids; `None` is the world.
    pub a: Option<u64>,
    pub b: Option<u64>,
    /// Frames in each body's local (scaled) coordinates, or the world's for a world side.
    pub anchor_a: Vec3,
    pub axis_a: Vec3,
    pub normal_a: Vec3,
    pub anchor_b: Vec3,
    pub axis_b: Vec3,
    pub normal_b: Vec3,
    /// Degrees (hinge angle, ball twist) or metres (prismatic offset).
    pub limit: Option<[f32; 2]>,
    /// Ball: swing half-angles about the frame's y and z axes, degrees.
    pub swing: Option<[f32; 2]>,
    /// Distance: `[min, max]` metres.
    pub length: Option<[f32; 2]>,
    /// Distance: a spring towards `rest_length` (Hz, damping ratio).
    pub spring: Option<(f32, f32, f32)>,
    pub motor: Option<MotorSpec>,
    pub collide_connected: bool,
}

fn num(v: &Json, field: &str) -> Result<f32, String> {
    match v.as_f64() {
        Some(f) if f.is_finite() && f.abs() <= 1.0e6 => Ok(f as f32),
        _ => Err(format!("'joint.{field}' must be a finite number")),
    }
}

fn vec(o: &Json, key: &str) -> Result<Option<Vec3>, String> {
    match o.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => match v.as_array() {
            Some([x, y, z]) => Ok(Some(Vec3::new(num(x, key)?, num(y, key)?, num(z, key)?))),
            _ => Err(format!("'joint.{key}' must be [x, y, z]")),
        },
    }
}

fn pair(o: &Json, key: &str) -> Result<Option<[f32; 2]>, String> {
    match o.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => match v.as_array() {
            Some([lo, hi]) => {
                let (lo, hi) = (num(lo, key)?, num(hi, key)?);
                if lo > hi {
                    return Err(format!("'joint.{key}' is [low, high] with low <= high"));
                }
                Ok(Some([lo, hi]))
            }
            _ => match v.as_f64() {
                // One number: a symmetric range (a round cone for `swing`).
                Some(_) if key == "swing" => {
                    let a = num(v, key)?;
                    Ok(Some([a, a]))
                }
                _ => Err(format!("'joint.{key}' must be [low, high]")),
            },
        },
    }
}

fn body_ref(o: &Json, key: &str) -> Result<Option<u64>, String> {
    match o.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v.as_u64().filter(|id| *id < SYNTHETIC).map(Some).ok_or_else(|| format!("'joint.{key}' must be an entity id or null (the world)")),
    }
}

const KEYS: &[&str] = &[
    "kind", "a", "b", "anchor_a", "axis_a", "normal_a", "anchor_b", "axis_b", "normal_b", "limit", "swing", "length", "spring", "motor", "collide_connected",
];

fn v3j(v: Vec3) -> Json {
    crate::world::vec3_json(v)
}

impl JointSpec {
    /// Parses `props.joint` (strict: unknown fields are errors that name the valid ones).
    pub fn from_json(o: &Json) -> Result<JointSpec, String> {
        let pairs = o.as_object().ok_or("'joint' must be an object")?;
        if let Some((k, _)) = pairs.iter().find(|(k, _)| !KEYS.contains(&k.as_str())) {
            return Err(format!("'joint.{k}' is not a field; valid: {}", KEYS.join(", ")));
        }
        let kind = o.get("kind").and_then(Json::as_str).and_then(JointKind::parse).ok_or("'joint.kind' must be fixed, hinge, ball, distance or prismatic")?;
        let (a, b) = (body_ref(o, "a")?, body_ref(o, "b")?);
        if a.is_none() && b.is_none() {
            return Err("a joint links at least one body: give 'joint.b' (and 'joint.a', or null for the world)".into());
        }
        let spring = match o.get("spring") {
            None | Some(Json::Null) => None,
            Some(s) => {
                let hz = s.get("hertz").map(|v| num(v, "spring.hertz")).transpose()?.unwrap_or(0.0);
                let damping = s.get("damping").map(|v| num(v, "spring.damping")).transpose()?.unwrap_or(0.5);
                let rest = s.get("rest_length").map(|v| num(v, "spring.rest_length")).transpose()?.ok_or("'joint.spring' needs 'rest_length' (m)")?;
                Some((hz, damping, rest))
            }
        };
        let motor = match o.get("motor") {
            None | Some(Json::Null) => None,
            Some(m) => Some(motor_from(m)?),
        };
        let collide_connected = match o.get("collide_connected") {
            None | Some(Json::Null) => false,
            Some(v) => v.as_bool().ok_or("'joint.collide_connected' must be true or false")?,
        };
        Ok(JointSpec {
            kind,
            a,
            b,
            anchor_a: vec(o, "anchor_a")?.unwrap_or(Vec3::ZERO),
            axis_a: vec(o, "axis_a")?.unwrap_or(Vec3::X),
            normal_a: vec(o, "normal_a")?.unwrap_or(Vec3::Y),
            anchor_b: vec(o, "anchor_b")?.unwrap_or(Vec3::ZERO),
            axis_b: vec(o, "axis_b")?.unwrap_or(Vec3::X),
            normal_b: vec(o, "normal_b")?.unwrap_or(Vec3::Y),
            limit: pair(o, "limit")?,
            swing: pair(o, "swing")?,
            length: match o.get("length") {
                Some(v) if v.as_f64().is_some() => {
                    let l = num(v, "length")?;
                    Some([l, l])
                }
                _ => pair(o, "length")?,
            },
            spring,
            motor,
            collide_connected,
        })
    }

    pub fn to_json(&self) -> Json {
        let id = |k: Option<u64>| k.map_or(Json::Null, Json::from);
        let mut f = vec![
            ("kind", Json::from(self.kind.name())),
            ("a", id(self.a)),
            ("b", id(self.b)),
            ("anchor_a", v3j(self.anchor_a)),
            ("axis_a", v3j(self.axis_a)),
            ("normal_a", v3j(self.normal_a)),
            ("anchor_b", v3j(self.anchor_b)),
            ("axis_b", v3j(self.axis_b)),
            ("normal_b", v3j(self.normal_b)),
        ];
        let two = |v: [f32; 2]| Json::arr(v.map(Json::from));
        if let Some(l) = self.limit {
            f.push(("limit", two(l)));
        }
        if let Some(s) = self.swing {
            f.push(("swing", two(s)));
        }
        if let Some(l) = self.length {
            f.push(("length", two(l)));
        }
        if let Some((hz, damping, rest)) = self.spring {
            f.push(("spring", Json::obj([("hertz", Json::from(hz)), ("damping", Json::from(damping)), ("rest_length", Json::from(rest))])));
        }
        if let Some(m) = self.motor {
            f.push(("motor", motor_json(&m)));
        }
        if self.collide_connected {
            f.push(("collide_connected", Json::from(true)));
        }
        Json::obj(f)
    }

    /// The physics joint, with each side's frame scaled by its body's scale (`None` for the world).
    pub fn desc(&self, scale_a: Vec3, scale_b: Vec3) -> JointDesc {
        let rad = |d: f32| d.to_radians();
        let side = |anchor: Vec3, axis: Vec3, normal: Vec3, scale: Vec3| frame(mul3(anchor, scale), axis, normal);
        let mut d = JointDesc::new(
            self.kind,
            self.a,
            self.b,
            side(self.anchor_a, self.axis_a, self.normal_a, scale_a),
            side(self.anchor_b, self.axis_b, self.normal_b, scale_b),
        );
        let angular = matches!(self.kind, JointKind::Hinge | JointKind::Ball);
        d.limit = self.limit.map(|[lo, hi]| if angular { [rad(lo), rad(hi)] } else { [lo, hi] });
        d.swing = self.swing.map(|[y, z]| [rad(y), rad(z)]);
        if let Some(l) = self.length {
            d.length = l;
        }
        if let Some((hz, damping, rest)) = self.spring {
            d.spring_hertz = hz;
            d.spring_damping = damping;
            d.rest_length = rest;
        }
        d.motor = self.motor.map(|m| Motor {
            drive: match m.drive {
                MotorDrive::Speed(s) => Drive::Speed(if angular { rad(s) } else { s }),
                MotorDrive::Position(p) => Drive::Position(if angular { rad(p) } else { p }),
                MotorDrive::Rotation(r) => Drive::Rotation(Quat::from_euler_xyz_deg(r.x, r.y, r.z)),
            },
            max_force: m.max_force,
            hertz: m.hertz,
            damping_ratio: m.damping,
        });
        d.collide_connected = self.collide_connected;
        d
    }

    /// Checks the joint as physics will (ranges, which motors fit which kinds).
    pub fn check(&self) -> Result<(), String> {
        if self.a.is_some() && self.a == self.b {
            return Err("'joint.a' and 'joint.b' must be different bodies".into());
        }
        if let (Some([lo, hi]), JointKind::Hinge | JointKind::Ball) = (self.limit, self.kind) {
            if lo < -180.0 || hi > 180.0 {
                return Err("'joint.limit' angles lie within -180..180 degrees".into());
            }
        }
        if self.kind == JointKind::Distance && self.length.is_none() {
            return Err("a distance joint needs 'joint.length' (m, or [min, max]; a rope is [0, L])".into());
        }
        if let Some(m) = &self.motor {
            if !(0.0..=MAX_FORCE).contains(&m.max_force) {
                return Err(format!("'joint.motor.max_force' must be 0..{MAX_FORCE} (N or N·m)"));
            }
        }
        specula_physics::joint::validate(&self.desc(Vec3::ONE, Vec3::ONE)).map_err(|e| e.message)
    }
}

fn motor_from(m: &Json) -> Result<MotorSpec, String> {
    const KEYS: &[&str] = &["speed", "position", "rotation_deg", "max_force", "hertz", "damping"];
    let pairs = m.as_object().ok_or("'joint.motor' must be an object")?;
    if let Some((k, _)) = pairs.iter().find(|(k, _)| !KEYS.contains(&k.as_str())) {
        return Err(format!("'joint.motor.{k}' is not a field; valid: {}", KEYS.join(", ")));
    }
    let given = ["speed", "position", "rotation_deg"].iter().filter(|k| m.get(k).is_some_and(|v| !v.is_null())).count();
    if given != 1 {
        return Err("'joint.motor' takes one of speed (hinge deg/s, prismatic m/s), position (deg or m) or rotation_deg (ball, fixed)".into());
    }
    let drive = if let Some(v) = m.get("speed").filter(|v| !v.is_null()) {
        MotorDrive::Speed(num(v, "motor.speed")?)
    } else if let Some(v) = m.get("position").filter(|v| !v.is_null()) {
        MotorDrive::Position(num(v, "motor.position")?)
    } else {
        MotorDrive::Rotation(vec(m, "rotation_deg")?.expect("counted"))
    };
    let spring = !matches!(drive, MotorDrive::Speed(_));
    let max_force = m.get("max_force").map(|v| num(v, "motor.max_force")).transpose()?.ok_or("'joint.motor.max_force' is required (N, or N·m for angular motors)")?;
    Ok(MotorSpec {
        drive,
        max_force,
        hertz: m.get("hertz").map(|v| num(v, "motor.hertz")).transpose()?.unwrap_or(if spring { 5.0 } else { 0.0 }),
        damping: m.get("damping").map(|v| num(v, "motor.damping")).transpose()?.unwrap_or(1.0),
    })
}

fn motor_json(m: &MotorSpec) -> Json {
    let mut f = match m.drive {
        MotorDrive::Speed(s) => vec![("speed", Json::from(s))],
        MotorDrive::Position(p) => vec![("position", Json::from(p))],
        MotorDrive::Rotation(r) => vec![("rotation_deg", v3j(r))],
    };
    f.push(("max_force", Json::from(m.max_force)));
    if !matches!(m.drive, MotorDrive::Speed(_)) {
        f.push(("hertz", Json::from(m.hertz)));
        f.push(("damping", Json::from(m.damping)));
    }
    Json::obj(f)
}

/// Whether an entity is a joint (a persistent entity with `props.joint`).
pub fn is_joint(e: &world::Entity) -> bool {
    !e.transient && e.props.get(PROP).is_some()
}

/// The scale physics gives a side's body (its world scale; the world's is one).
fn scale_of(world: &World, id: Option<u64>) -> Vec3 {
    match id.and_then(|i| world.get(i)) {
        Some(e) => world_pose(world, e).1,
        None => Vec3::ONE,
    }
}

impl Physics {
    /// Brings the joint held by entity `e` (if it is one) up to date.
    pub(super) fn upsert_joint(&mut self, world: &World, e: &world::Entity) {
        let id = e.id;
        if !is_joint(e) {
            if self.space.remove_joint(id) {
                self.problems.remove(&id);
            }
            return;
        }
        let spec = match JointSpec::from_json(e.props.get(PROP).expect("is_joint")).and_then(|s| s.check().map(|()| s)) {
            Ok(s) => s,
            Err(m) => {
                self.space.remove_joint(id);
                return self.note_joint_problem(world, id, &m);
            }
        };
        let desc = spec.desc(scale_of(world, spec.a), scale_of(world, spec.b));
        if self.space.joint(id).is_some_and(|j| *j.desc == desc) {
            return;
        }
        match self.space.insert_joint(id, desc) {
            Ok(()) => {
                self.problems.remove(&id);
            }
            Err(err) => self.note_joint_problem(world, id, &err.message),
        }
    }

    fn note_joint_problem(&mut self, world: &World, id: u64, error: &str) {
        if self.problems.len() < MAX_PROBLEMS || self.problems.contains_key(&id) {
            self.problems.insert(id, Json::obj([("id", Json::from(id)), ("path", Json::from(world.path_of(id))), ("error", Json::from(format!("joint: {error}")))]));
        }
    }

    /// Entity `id` is gone: its joint goes, and joints on it as a body wait for it.
    pub(super) fn forget_joint(&mut self, id: u64) {
        if self.space.remove_joint(id) {
            self.problems.remove(&id);
        }
    }

    /// The joints on body `id` (entity joints only) follow its scale.
    pub(super) fn refresh_joints_of(&mut self, world: &World, id: u64) {
        for jk in self.space.joints_of(id) {
            if jk >= SYNTHETIC {
                continue;
            }
            if let Some(je) = world.get(jk) {
                self.upsert_joint(world, je);
            }
        }
    }

    /// A physics key as data: an entity's `{id, path}`, or an engine-made part's.
    pub fn key_json(&self, world: &World, key: u64) -> Json {
        if key < SYNTHETIC {
            return Json::obj([("id", Json::from(key)), ("path", Json::from(world.path_of(key)))]);
        }
        Json::obj([("key", Json::from(key))])
    }

    /// After a full compare: joints whose entities are gone go too.
    pub(super) fn drop_stale_joints(&mut self, live: &BTreeSet<u64>) {
        let stale: Vec<u64> = self.space.joints().map(|j| j.key).filter(|k| *k < SYNTHETIC && !live.contains(k)).collect();
        for k in stale {
            self.space.remove_joint(k);
            self.problems.remove(&k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Shape as WShape, SpawnSpec};

    fn spec_json(text: &str) -> Json {
        Json::parse(text).unwrap()
    }

    #[test]
    fn joint_specs_round_trip_and_explain_mistakes() {
        let j = spec_json(
            r#"{"kind":"hinge","a":3,"b":4,"anchor_a":[0,1,0],"axis_a":[0,0,1],"normal_a":[0,1,0],"anchor_b":[0,-1,0],"axis_b":[0,0,1],"normal_b":[0,1,0],
                "limit":[-90,0],"motor":{"speed":45,"max_force":20}}"#,
        );
        let s = JointSpec::from_json(&j).unwrap();
        s.check().unwrap();
        assert_eq!(JointSpec::from_json(&s.to_json()).unwrap(), s);
        let d = s.desc(Vec3::ONE, Vec3::new(2.0, 2.0, 2.0));
        assert_eq!(d.frame_b.p, Vec3::new(0.0, -2.0, 0.0), "frames scale like collider offsets");
        assert!((d.limit.unwrap()[0] + std::f32::consts::FRAC_PI_2).abs() < 1e-6, "degrees become radians");
        let bad = |t: &str| JointSpec::from_json(&spec_json(t)).and_then(|s| s.check()).unwrap_err();
        assert!(bad(r#"{"kind":"hinge","b":4,"wobble":1}"#).contains("valid:"));
        assert!(bad(r#"{"kind":"distance","b":4}"#).contains("length"));
        assert!(bad(r#"{"kind":"hinge","b":4,"motor":{"rotation_deg":[0,0,0],"max_force":1}}"#).contains("motor"));
        assert!(bad(r#"{"kind":"ball","b":4,"limit":[10,-10]}"#).contains("low <= high"));
        assert!(bad(r#"{"kind":"ball","a":4,"b":4}"#).contains("different"));
    }

    #[test]
    fn joint_entities_hold_bodies_together_and_follow_their_changes() {
        let mut w = World::default();
        let mut s = SpawnSpec::new(WShape::Cube);
        s.name = Some("bar".into());
        s.position = Vec3::new(0.5, 2.0, 0.0);
        s.scale = Vec3::new(1.0, 0.1, 0.1);
        s.collider = Some(Collider::default());
        s.body = Some(Body::new(BodyKind::Dynamic, 2.0));
        let bar = w.spawn(s, "t").unwrap().id;
        let joint = Json::obj([(
            "joint",
            Json::parse(&format!(r#"{{"kind":"hinge","a":null,"b":{bar},"anchor_a":[0,2,0],"axis_a":[0,0,1],"anchor_b":[-0.5,0,0],"axis_b":[0,0,1]}}"#)).unwrap(),
        )]);
        let mut j = SpawnSpec::new(WShape::Group);
        j.name = Some("hinge".into());
        j.parent = Some(EntityKey::Id(bar));
        j.props = joint;
        let jid = w.spawn(j, "t").unwrap().id;
        let mut p = Physics::default();
        let settings = w.env().physics;
        for tick in 0..120 {
            w.set_recording(Recording::Snapshot);
            p.tick(&mut w, 0, &settings, tick, 1.0 / 60.0);
            w.set_recording(Recording::Full);
            w.take_changes();
        }
        let view = p.space.joint(jid).expect("synced");
        assert!(view.state.active && view.state.linear_error < 0.01, "{:?}", view.state);
        let e = w.get(bar).unwrap();
        assert!((e.position - Vec3::new(0.0, 2.0, 0.0)).length() > 0.4 && e.position.y < 2.0, "it swung: {:?}", e.position);
        // A broken joint is listed with why, and removing props.joint removes the joint.
        w.update(&EntityKey::Id(jid), UpdateSpec { props: Some(Json::parse(r#"{"joint":{"kind":"spring"}}"#).unwrap()), ..Default::default() }, "t").unwrap();
        p.tick(&mut w, 0, &settings, 200, 1.0 / 60.0);
        assert!(p.space.joint(jid).is_none());
        assert!(p.problems.get(&jid).and_then(|j| j.get("error")).and_then(Json::as_str).unwrap().contains("joint.kind"));
        w.delete(&EntityKey::Id(jid), None).unwrap();
        p.tick(&mut w, 0, &settings, 201, 1.0 / 60.0);
        assert!(!p.problems.contains_key(&jid));
    }
}

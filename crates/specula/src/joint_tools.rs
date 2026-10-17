//! The joint tools (P4): `physics_joint` (make, change or remove a joint from world-space
//! anchors and axes), `physics_joints` (joints and their measured state as data), the
//! `joint_error` and `joint_limits` checks of `physics_check`, and the `joints` sight overlay.
//!
//! A joint is an entity holding `props.joint` (see [`crate::physics::joints`]), so every write
//! here is journaled, undoable and claim-checked like `world_spawn` / `world_set` / `world_delete`,
//! and also needs the writer to be allowed to change both bodies (a joint moves them).

use crate::journal::{Kind as JKind, Writer};
use crate::physics::joints::{JointSpec, PROP, SYNTHETIC};
use crate::physics::{self, world_pose};
use crate::sight_overlay::{self as ov, Painter};
use crate::tools::{bool_arg, check_keys, int_arg, key_value, string_arg, vec3_arg};
use crate::world::{Change, EntityKey, Shape, SpawnSpec, UpdateSpec, World, WorldError};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_physics::joint::JointKind;
use specula_physics::Pose;
use specula_protocol::{ToolDef, ToolOutput};
use std::time::Instant;

/// Most joints one listing returns.
pub const MAX_LIST: usize = 256;
/// Joints the overlay draws at most.
const MAX_DRAWN: usize = 512;

/// A `physics_joint` call.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JointWrite {
    /// An existing joint entity to change or remove.
    pub target: Option<EntityKey>,
    pub remove: bool,
    pub kind: Option<JointKind>,
    /// `Some(None)`: the world.
    pub a: Option<Option<EntityKey>>,
    pub b: Option<EntityKey>,
    pub anchor: Option<Vec3>,
    pub anchor_b: Option<Vec3>,
    pub axis: Option<Vec3>,
    pub normal: Option<Vec3>,
    /// The fields `props.joint` takes as they are (limit, swing, length, spring, motor,
    /// collide_connected): `null` removes one.
    pub fields: Vec<(String, Json)>,
    pub name: Option<String>,
    pub parent: Option<EntityKey>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JointCall {
    Write(Box<JointWrite>),
    List { entity: Option<EntityKey>, limit: usize, after: Option<u64>, ragdolls: bool },
}

impl JointCall {
    pub fn is_read(&self) -> bool {
        matches!(self, JointCall::List { .. })
    }
}

const PASS_THROUGH: [&str; 6] = ["limit", "swing", "length", "spring", "motor", "collide_connected"];

fn key_or_world(v: &Json, field: &str) -> Result<Option<EntityKey>, String> {
    match v {
        Json::Null => Ok(None),
        v => key_value(v, field).map(Some),
    }
}

/// Parses `physics_joint` / `physics_joints` (`None`: not a joint tool).
pub fn parse(name: &str, args: &Json) -> Option<Result<JointCall, String>> {
    let call = match name {
        "physics_joint" => (|| {
            let mut allowed = vec!["id", "path", "remove", "kind", "a", "b", "anchor", "anchor_b", "axis", "normal", "name", "parent"];
            allowed.extend(PASS_THROUGH);
            check_keys(args, name, &[&allowed])?;
            let target = match (args.get("id").filter(|v| !v.is_null()), args.get("path").filter(|v| !v.is_null())) {
                (Some(_), Some(_)) => return Err("physics_joint: give 'id' or 'path' of the joint, not both".to_string()),
                (Some(v), None) => Some(key_value(v, "id")?),
                (None, Some(v)) => Some(key_value(v, "path")?),
                (None, None) => None,
            };
            let kind = match string_arg(args, "kind")? {
                None => None,
                Some(k) => Some(JointKind::parse(k).ok_or_else(|| format!("'kind' must be fixed, hinge, ball, distance or prismatic (got '{k}')"))?),
            };
            let point = |k: &str| -> Result<Option<Vec3>, String> {
                let v = vec3_arg(args, k)?;
                if let Some(p) = v {
                    crate::world::check_point(k, p).map_err(|e| e.message)?;
                }
                Ok(v)
            };
            let dir = |k: &str| -> Result<Option<Vec3>, String> {
                let v = vec3_arg(args, k)?;
                if v.is_some_and(|d| d.length() < 1.0e-6) {
                    return Err(format!("'{k}' must not be zero"));
                }
                Ok(v)
            };
            let w = JointWrite {
                remove: bool_arg(args, "remove")?.unwrap_or(false),
                kind,
                a: args.get("a").map(|v| key_or_world(v, "a")).transpose()?,
                b: match args.get("b") {
                    None | Some(Json::Null) => None,
                    Some(v) => Some(key_value(v, "b")?),
                },
                anchor: point("anchor")?,
                anchor_b: point("anchor_b")?,
                axis: dir("axis")?,
                normal: dir("normal")?,
                fields: PASS_THROUGH.iter().filter_map(|k| args.get(k).map(|v| (k.to_string(), v.clone()))).collect(),
                name: string_arg(args, "name")?.map(str::to_owned),
                parent: args.get("parent").filter(|v| !v.is_null()).map(|v| key_value(v, "parent")).transpose()?,
                target,
            };
            if w.remove && w.target.is_none() {
                return Err("'remove' needs the joint's 'id' or 'path'".into());
            }
            if w.target.is_none() && (w.kind.is_none() || w.b.is_none()) {
                return Err("a new joint needs 'kind' and 'b' (the body it holds; 'a' is the other body, or null for the world)".into());
            }
            Ok(JointCall::Write(Box::new(w)))
        })(),
        "physics_joints" => (|| {
            check_keys(args, name, &[&["entity", "limit", "cursor", "ragdolls"]])?;
            let after = match string_arg(args, "cursor")? {
                None => None,
                Some(c) => Some(c.parse::<u64>().map_err(|_| "'cursor' must be the next_cursor of an earlier physics_joints answer")?),
            };
            Ok(JointCall::List {
                entity: args.get("entity").filter(|v| !v.is_null()).map(|v| key_value(v, "entity")).transpose()?,
                limit: int_arg(args, "limit", 1, MAX_LIST as u64)?.unwrap_or(32) as usize,
                after,
                ragdolls: bool_arg(args, "ragdolls")?.unwrap_or(false),
            })
        })(),
        _ => return None,
    };
    Some(call)
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let any = |d: &str| Json::obj([("description", Json::from(d))]);
    let vec3 = |d: &str| {
        Json::obj([
            ("type", Json::from("array")),
            ("items", Json::obj([("type", Json::from("number"))])),
            ("minItems", Json::from(3)),
            ("maxItems", Json::from(3)),
            ("description", Json::from(d)),
        ])
    };
    let schema = |props: Vec<(&str, Json)>| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props))]);
    vec![
        ToolDef {
            name: "physics_joint".into(),
            description: "Make, change or remove a joint between two bodies (or a body and the world). A joint is an entity \
                          (a group, by default under body b) whose props.joint holds it, so it is journaled, undoable, \
                          claimable and saved; you also need to be allowed to change both bodies. Give world-space 'anchor' \
                          and 'axis': the frames on each body are worked out from where the bodies are now (at rest the \
                          joint holds that relative pose). Kinds: fixed (welds), hinge (turns about 'axis': doors, wheels, \
                          knees), ball (turns any way about the anchor; 'axis' is its twist axis: shoulders, chains), \
                          distance (keeps the anchors' distance in 'length': rods, ropes [0, L], springs), prismatic \
                          (slides along 'axis' without turning). Jointed bodies don't collide with each other unless \
                          collide_connected. To change one, give its id or path and only what changes (null removes a \
                          field); remove: true deletes it. physics_joints shows how well each one holds."
                .into(),
            input_schema: schema(vec![
                ("id", t("integer", "An existing joint entity to change or remove")),
                ("path", t("string", "An existing joint entity's path")),
                ("remove", t("boolean", "Delete the joint (with id or path)")),
                ("kind", t("string", "fixed | hinge | ball | distance | prismatic")),
                ("a", any("The first body (id or path), or null for the world (default: the world)")),
                ("b", any("The body the joint holds (id or path)")),
                ("anchor", vec3("Where the joint is, world [x,y,z] (default: b's position)")),
                ("anchor_b", vec3("Distance joints: b's end, world (default: anchor)")),
                ("axis", vec3("Hinge/prismatic axis, ball twist axis, world (default [1,0,0])")),
                ("normal", vec3("Reference direction across the axis that hinge angles are measured from (default: up, or x)")),
                (
                    "limit",
                    any("[low, high]: hinge angle or ball twist in degrees (-180..180), prismatic offset in metres along the axis; null removes"),
                ),
                ("swing", any("Ball joints: cone half-angle in degrees, or [about y, about z] for an ellipse")),
                ("length", any("Distance joints: metres, or [min, max] (a rope is [0, L]); default: the anchors' distance now")),
                ("spring", any("Distance joints: {hertz, damping (ratio, default 0.5), rest_length m}")),
                (
                    "motor",
                    any("{speed (hinge deg/s, prismatic m/s) | position (deg or m) | rotation_deg [x,y,z] (ball, fixed: b's frame in a's), max_force (N or N·m, required), hertz (target drives, default 5), damping (default 1)}"),
                ),
                ("collide_connected", t("boolean", "The two bodies still collide with each other (default false)")),
                ("name", t("string", "The joint entity's name (default joint_<kind>)")),
                ("parent", any("Where the joint entity goes (default: under b)")),
            ]),
        },
        ToolDef {
            name: "physics_joints".into(),
            description: "Joints as data, in id order: kind, the bodies (id, path), whether physics solved it last tick (and if \
                          not, why: missing_body, asleep, no_dynamic_body, cosmetic_link), how well it holds (linear_error_m: \
                          anchor gap; angular_error_deg: misalignment), where it is (angle_deg for hinges and ball twist, \
                          swing_deg, offset_m for sliders, length_m for distance joints), how far past its limits, and the \
                          force and torque it carries. Filter by a joint or a body (its joints); ragdolls: true adds ragdoll \
                          bones' joints. Page with cursor."
                .into(),
            input_schema: schema(vec![
                ("entity", any("A joint entity, or a body (id or path): only its joints")),
                ("limit", t("integer", "Joints per page, 1..256 (default 32)")),
                ("cursor", t("string", "next_cursor of an earlier answer")),
                ("ragdolls", t("boolean", "Include ragdoll bones' joints (default false)")),
            ]),
        },
    ]
}

fn world_error(e: &WorldError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_pretty())
}

fn round(v: f32) -> Json {
    Json::from((f64::from(v) * 1.0e4).round() / 1.0e4)
}

/// An anchor, axis and normal given in the world, in body `id`'s local (scaled) frame (the
/// world's for `None`).
fn local_frame(world: &World, id: Option<u64>, anchor: Vec3, axis: Vec3, normal: Vec3) -> (Vec3, Vec3, Vec3) {
    match id.and_then(|i| world.get(i)) {
        None => (anchor, axis, normal),
        Some(e) => {
            let (pose, scale): (Pose, Vec3) = world_pose(world, e);
            let p = pose.inv_point(anchor);
            let unscale = |v: f32, s: f32| if s.abs() > 1.0e-9 { v / s } else { v };
            (Vec3::new(unscale(p.x, scale.x), unscale(p.y, scale.y), unscale(p.z, scale.z)), pose.inv_vector(axis), pose.inv_vector(normal))
        }
    }
}

/// Body `id`'s anchor `local` (scaled) in the world.
fn world_point(world: &World, id: Option<u64>, local: Vec3) -> Vec3 {
    match id.and_then(|i| world.get(i)) {
        None => local,
        Some(e) => {
            let (pose, scale) = world_pose(world, e);
            pose.point(Vec3::new(local.x * scale.x, local.y * scale.y, local.z * scale.z))
        }
    }
}

/// Body `id`'s local direction `v` in the world.
fn world_vector(world: &World, id: Option<u64>, v: Vec3) -> Vec3 {
    match id.and_then(|i| world.get(i)) {
        None => v,
        Some(e) => world_pose(world, e).0.vector(v),
    }
}

fn entity_json(world: &World, id: u64) -> Json {
    Json::obj([("id", Json::from(id)), ("path", Json::from(world.path_of(id)))])
}

impl crate::Engine {
    pub(crate) fn joint_call(&mut self, call: JointCall, w: Writer, tick: u64) -> ToolOutput {
        match call {
            JointCall::Write(write) => self.joint_write(*write, w, tick),
            JointCall::List { entity, limit, after, ragdolls } => self.joint_list(entity, limit, after, ragdolls),
        }
    }

    fn joint_write(&mut self, jw: JointWrite, w: Writer, tick: u64) -> ToolOutput {
        let world = &self.world;
        let resolve = |k: &EntityKey, field: &str| world.resolve(k).map_err(|e| e.with("field", field));
        // The joint as it is (when changing one).
        let (target, old) = match &jw.target {
            None => (None, None),
            Some(k) => {
                let id = match resolve(k, "id") {
                    Ok(id) => id,
                    Err(e) => return world_error(&e),
                };
                let e = world.get(id).expect("resolved");
                match e.props.get(PROP) {
                    Some(j) => (Some(id), Some(j.clone())),
                    None => {
                        return world_error(
                            &WorldError::new("not_a_joint", format!("{} has no props.joint: it isn't a joint (make one without id/path)", world.path_of(id))).with("id", id),
                        )
                    }
                }
            }
        };
        if jw.remove {
            let id = target.expect("checked at parse");
            let by = w.name.to_owned();
            return match self.swarm.commit(&mut self.world, tick, w, JKind::Delete, |wd| wd.delete(&EntityKey::Id(id), None).map(|d| d.path)) {
                Ok((path, seq)) => ToolOutput::text(Json::obj([("removed", Json::from(id)), ("path", Json::from(path)), ("by", Json::from(by)), ("journal", Json::from(seq))]).to_string()),
                Err(e) => world_error(&e),
            };
        }
        let old_spec = old.as_ref().and_then(|j| JointSpec::from_json(j).ok());
        // The bodies.
        let a = match &jw.a {
            Some(None) => None,
            Some(Some(k)) => match resolve(k, "a") {
                Ok(id) => Some(id),
                Err(e) => return world_error(&e),
            },
            None => old_spec.as_ref().and_then(|s| s.a),
        };
        let b = match &jw.b {
            Some(k) => match resolve(k, "b") {
                Ok(id) => Some(id),
                Err(e) => return world_error(&e),
            },
            None => old_spec.as_ref().and_then(|s| s.b),
        };
        let Some(b) = b else {
            return world_error(&WorldError::new("invalid_field", "the joint needs a body 'b'").with("field", "b"));
        };
        for (field, id) in [("a", a), ("b", Some(b))] {
            let Some(id) = id else { continue };
            let e = world.get(id).expect("resolved");
            let why = if e.transient {
                Some("is transient (a game's presentation): physics doesn't simulate it")
            } else if e.collider.is_none() && e.body.is_none() {
                Some("has no body or collider: give it body {kind: dynamic} and a collider first")
            } else {
                None
            };
            if let Some(why) = why {
                return world_error(&WorldError::new("not_a_body", format!("'{field}' ({}) {why}", world.path_of(id))).with("field", field).with("id", id));
            }
        }
        // props.joint: the old one (when changing), then what was given.
        let mut j = old.unwrap_or_else(|| Json::obj::<&str>([]));
        let kind = jw.kind.or(old_spec.as_ref().map(|s| s.kind)).expect("checked at parse");
        j.set("kind", Json::from(kind.name()));
        j.set("a", a.map_or(Json::Null, Json::from));
        j.set("b", Json::from(b));
        for (k, v) in &jw.fields {
            if v.is_null() {
                if let Json::Obj(pairs) = &mut j {
                    pairs.retain(|(key, _)| key != k);
                }
            } else {
                j.set(k, v.clone());
            }
        }
        // Frames: from the world anchor and axis, on a new joint or when any of them (or a
        // body) changed.
        let reframe = old_spec.is_none() || jw.anchor.is_some() || jw.anchor_b.is_some() || jw.axis.is_some() || jw.normal.is_some() || jw.a.is_some() || jw.b.is_some();
        if reframe {
            let b_pos = world.get(b).expect("resolved").world_position();
            let anchor = jw.anchor.or(old_spec.as_ref().map(|s| world_point(world, s.a, s.anchor_a))).unwrap_or(b_pos);
            let anchor_b = jw.anchor_b.or(if kind == JointKind::Distance { old_spec.as_ref().map(|s| world_point(world, s.b, s.anchor_b)) } else { None }).unwrap_or(anchor);
            let axis = jw.axis.or(old_spec.as_ref().map(|s| world_vector(world, s.a, s.axis_a))).unwrap_or(Vec3::X).normalize();
            let normal = jw.normal.unwrap_or(if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X });
            let (pa, xa, na) = local_frame(world, a, anchor, axis, normal);
            let (pb, xb, nb) = local_frame(world, Some(b), anchor_b, axis, normal);
            for (k, v) in [("anchor_a", pa), ("axis_a", xa), ("normal_a", na), ("anchor_b", pb), ("axis_b", xb), ("normal_b", nb)] {
                j.set(k, crate::world::vec3_json(v));
            }
            if kind == JointKind::Distance && j.get("length").is_none() {
                j.set("length", Json::from((anchor_b - anchor).length()));
            }
        }
        let spec = match JointSpec::from_json(&j).and_then(|s| s.check().map(|()| s)) {
            Ok(s) => s,
            Err(m) => return world_error(&WorldError::new("invalid_joint", m)),
        };
        // A joint moves both bodies: the writer must be allowed to change them.
        let now = Instant::now();
        for id in spec.a.into_iter().chain(spec.b) {
            let e = self.world.get(id).expect("resolved");
            let change = Change::Pose { path: self.world.path_of(id), pose: crate::world::Pose::of(e) };
            if let Err(err) = self.swarm.claims.check(&self.world, &[change], w, now) {
                return world_error(&err.with("note", "a joint moves the bodies it links, so it needs the same right to change them"));
            }
        }
        let props = Json::obj([(PROP, spec.to_json())]);
        let by = w.name.to_owned();
        let result = match target {
            Some(id) => {
                let change = UpdateSpec { props: Some(props), ..Default::default() };
                self.swarm.commit(&mut self.world, tick, w, JKind::Set, |wd| wd.update(&EntityKey::Id(id), change, &by).map(|e| e.id))
            }
            None => {
                let parent = match &jw.parent {
                    Some(k) => match self.world.resolve(k) {
                        Ok(p) => p,
                        Err(e) => return world_error(&e.with("field", "parent")),
                    },
                    None => b,
                };
                let base = jw.name.clone().unwrap_or_else(|| format!("joint_{}", kind.name()));
                let mut name = base.clone();
                let mut n = 1;
                while self.world.find_path(&format!("{}/{name}", self.world.path_of(parent))).is_ok() {
                    if jw.name.is_some() || n >= 100 {
                        return world_error(&WorldError::new("name_taken", format!("{}/{name} already exists", self.world.path_of(parent))).with("field", "name"));
                    }
                    n += 1;
                    name = format!("{base}_{n}");
                }
                let mut s = SpawnSpec::new(Shape::Group);
                s.name = Some(name);
                s.parent = Some(EntityKey::Id(parent));
                s.props = props;
                s.tags = ["joint".to_string()].into_iter().collect();
                self.swarm.commit(&mut self.world, tick, w, JKind::Spawn, |wd| wd.spawn(s, &by).map(|e| e.id))
            }
        };
        match result {
            Ok((id, seq)) => {
                let mut out = entity_json(&self.world, id);
                out.set("joint", spec.to_json());
                let dynamic = spec.a.into_iter().chain(spec.b).any(|k| self.world.get(k).and_then(|e| e.body).is_some_and(|b| b.kind == physics::BodyKind::Dynamic));
                if !dynamic {
                    out.set("note", Json::from("neither body is dynamic: the joint holds nothing until one is"));
                }
                out.set("journal", Json::from(seq));
                ToolOutput::text(out.to_string())
            }
            Err(e) => world_error(&e),
        }
    }

    fn joint_list(&self, entity: Option<EntityKey>, limit: usize, after: Option<u64>, ragdolls: bool) -> ToolOutput {
        let only = match entity.map(|k| self.world.resolve(&k)).transpose() {
            Ok(o) => o,
            Err(e) => return world_error(&e),
        };
        let space = &self.physics.space;
        let mut items = Vec::new();
        let mut total = 0usize;
        let mut next = None;
        for j in space.joints() {
            if j.key >= SYNTHETIC && !ragdolls {
                continue;
            }
            if let Some(o) = only {
                if j.key != o && j.desc.a != Some(o) && j.desc.b != Some(o) {
                    continue;
                }
            }
            total += 1;
            if after.is_some_and(|a| j.key <= a) {
                continue;
            }
            if items.len() == limit {
                if next.is_none() {
                    next = items.last().and_then(|i: &Json| i.get("id")).and_then(Json::as_u64).map(|k| k.to_string());
                }
                continue;
            }
            items.push(self.joint_json(j.key));
        }
        let mut problems: Vec<Json> = self.physics.problems.iter().filter(|(k, v)| space.joint(**k).is_none() && v.get("error").and_then(Json::as_str).is_some_and(|e| e.starts_with("joint:"))).map(|(_, v)| v.clone()).take(16).collect();
        problems.truncate(16);
        ToolOutput::text(Json::obj([("joints", Json::Arr(items)), ("total", Json::from(total)), ("next_cursor", Json::from(next)), ("not_simulated", Json::Arr(problems))]).to_string())
    }

    /// One joint as data (an entity's, or a ragdoll bone's).
    pub(crate) fn joint_json(&self, key: u64) -> Json {
        let world = &self.world;
        let Some(j) = self.physics.space.joint(key) else { return Json::Null };
        let side = |k: Option<u64>| k.map_or(Json::Null, |k| self.physics.key_json(world, k));
        let s = j.state;
        let deg = |r: f32| round(r.to_degrees());
        let angular = matches!(j.desc.kind, JointKind::Hinge | JointKind::Ball);
        let mut out = if key >= SYNTHETIC { self.physics.key_json(world, key) } else { entity_json(world, key) };
        out.set("kind", Json::from(j.desc.kind.name()));
        out.set("a", side(j.desc.a));
        out.set("b", side(j.desc.b));
        out.set("active", Json::from(s.active));
        if let Some(why) = s.inactive {
            out.set("inactive", Json::from(why));
        }
        out.set("linear_error_m", round(s.linear_error));
        if matches!(j.desc.kind, JointKind::Fixed | JointKind::Hinge | JointKind::Prismatic) {
            out.set("angular_error_deg", deg(s.angular_error));
        }
        match j.desc.kind {
            JointKind::Hinge => {
                out.set("angle_deg", deg(s.position));
            }
            JointKind::Ball => {
                out.set("twist_deg", deg(s.position));
                out.set("swing_deg", deg(s.swing));
            }
            JointKind::Prismatic => {
                out.set("offset_m", round(s.position));
            }
            JointKind::Distance => {
                out.set("length_m", round(s.position));
            }
            JointKind::Fixed => {}
        }
        if let Some([lo, hi]) = j.desc.limit {
            out.set("limit", if angular { Json::arr([deg(lo), deg(hi)]) } else { Json::arr([round(lo), round(hi)]) });
        }
        if let Some([y, z]) = j.desc.swing {
            out.set("swing", Json::arr([deg(y), deg(z)]));
        }
        if j.desc.kind == JointKind::Distance {
            out.set("length", Json::arr([round(j.desc.length[0]), round(j.desc.length[1])]));
        }
        if j.desc.limit.is_some() || j.desc.swing.is_some() || j.desc.kind == JointKind::Distance {
            out.set(if angular { "past_limit_deg" } else { "past_limit_m" }, if angular { deg(s.limit_excess) } else { round(s.limit_excess) });
        }
        out.set("force_n", round(s.force));
        out.set("torque_nm", round(s.torque));
        if j.desc.motor.is_some() {
            out.set("motor", Json::from(true));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// The `joint_error` and `joint_limits` checks

/// `physics_check {check: joint_error | joint_limits}` over the recorded window.
pub fn run_check(engine: &crate::Engine, name: &str, c: &Json, since: Option<u64>) -> Json {
    let world = &engine.world;
    let window = || engine.physics.window.iter().filter(move |r| since.is_none_or(|s| r.tick >= s));
    let num = |k: &str, d: f32| c.get(k).and_then(Json::as_f64).map_or(d, |v| v as f32);
    let key = |k: u64| if k >= SYNTHETIC { engine.physics.key_json(world, k) } else { entity_json(world, k) };
    match name {
        "joint_error" => {
            let (max_m, max_deg) = (num("max_m", 0.01), num("max_deg", 2.0));
            let worst_m = window().filter_map(|r| r.joint.map(|j| (j, r.tick))).fold(None, |m: Option<((f32, f32, u64), u64)>, x| match m {
                Some(y) if y.0 .0 >= x.0 .0 => Some(y),
                _ => Some(x),
            });
            let worst_a = window().filter_map(|r| r.joint.map(|j| (j, r.tick))).fold(None, |m: Option<((f32, f32, u64), u64)>, x| match m {
                Some(y) if y.0 .1 >= x.0 .1 => Some(y),
                _ => Some(x),
            });
            let lin = worst_m.map_or(0.0, |((l, _, _), _)| l);
            let ang = worst_a.map_or(0.0, |((_, a, _), _)| a).to_degrees();
            let mut j = Json::obj([
                ("check", Json::from("joint_error")),
                ("pass", Json::from(lin <= max_m && ang <= max_deg)),
                ("worst_m", round(lin)),
                ("worst_deg", round(ang)),
                ("joints_solved", Json::from(window().filter(|r| r.joint.is_some()).count())),
            ]);
            if let Some(((_, _, k), t)) = worst_m.filter(|_| lin > 0.0) {
                j.set("worst_m_joint", key(k));
                j.set("worst_m_tick", Json::from(t));
            }
            if let Some(((_, _, k), t)) = worst_a.filter(|_| ang > 0.0) {
                j.set("worst_deg_joint", key(k));
                j.set("worst_deg_tick", Json::from(t));
            }
            j
        }
        _ => {
            let (max_deg, max_m) = (num("max_deg", 2.0), num("max_m", 0.01));
            let worst = |pick: fn(&physics::TickRecord) -> Option<(f32, u64)>| {
                window().filter_map(|r| pick(r).map(|x| (x, r.tick))).fold(None, |m: Option<((f32, u64), u64)>, x| match m {
                    Some(y) if y.0 .0 >= x.0 .0 => Some(y),
                    _ => Some(x),
                })
            };
            let (angle, offset) = (worst(|r| r.limit), worst(|r| r.offset));
            let (deg, m) = (angle.map_or(0.0, |((e, _), _)| e.to_degrees()), offset.map_or(0.0, |((e, _), _)| e));
            let mut j = Json::obj([("check", Json::from("joint_limits")), ("pass", Json::from(deg <= max_deg && m <= max_m)), ("past_limit_deg", round(deg)), ("past_limit_m", round(m))]);
            if let Some(((_, k), t)) = angle {
                j.set("worst_angle_joint", key(k));
                j.set("worst_angle_tick", Json::from(t));
            }
            if let Some(((_, k), t)) = offset {
                j.set("worst_offset_joint", key(k));
                j.set("worst_offset_tick", Json::from(t));
            }
            j
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The `joints` overlay

const OK: [f32; 4] = [0.35, 1.0, 0.85, 1.0];
const STRAINED: [f32; 4] = [1.0, 0.7, 0.2, 1.0];
const PAST: [f32; 4] = [1.0, 0.25, 0.3, 1.0];
const IDLE: [f32; 4] = [0.6, 0.62, 0.7, 0.85];
const AXIS: [f32; 4] = [1.0, 0.95, 0.4, 1.0];

/// Draws the `joints` overlay: anchors, axes, the arms from each body's centre, limit arcs;
/// coloured by how the joint holds. Returns the packet's details.
pub fn paint(physics: &physics::Physics, world: &World, p: &mut Painter, spec: &ov::Spec) -> Json {
    let space = &physics.space;
    let selected = |k: u64| spec.entities.is_empty() || spec.selects(&world.path_of(crate::physics::ragdoll::owner_of(k)));
    let (mut drawn, mut omitted) = (0usize, 0usize);
    let mut counts = [0usize; 4];
    let mut worst = 0.0f32;
    for j in space.joints() {
        let mine = j.desc.bodies().any(selected) || (j.key < SYNTHETIC && selected(j.key));
        if !mine {
            continue;
        }
        if drawn >= MAX_DRAWN {
            omitted += 1;
            continue;
        }
        let pose = |k: Option<u64>| match k {
            None => Some(Pose::IDENTITY),
            Some(k) => space.body(k).map(|b| b.pose),
        };
        let (Some(pa), Some(pb)) = (pose(j.desc.a), pose(j.desc.b)) else { continue };
        let fa = pa.mul(&j.desc.frame_a);
        let fb = pb.mul(&j.desc.frame_b);
        let s = j.state;
        let (color, slot) = if !s.active {
            (IDLE, 3)
        } else if s.limit_excess > 0.035 || s.linear_error > 0.05 {
            (PAST, 2)
        } else if s.linear_error > 0.01 || s.angular_error > 0.035 {
            (STRAINED, 1)
        } else {
            (OK, 0)
        };
        counts[slot] += 1;
        worst = worst.max(s.linear_error);
        // The arms from each body's centre to its anchor.
        for (k, f) in [(j.desc.a, fa), (j.desc.b, fb)] {
            if let Some(b) = k.and_then(|k| space.body(k)) {
                p.line(b.center, f.p, [color[0], color[1], color[2], 0.55], 1.0);
            }
        }
        p.dot(fa.p, 4.0, color);
        if (fb.p - fa.p).length() > 1.0e-3 {
            p.dot(fb.p, 3.0, color);
            p.line(fa.p, fb.p, PAST, 2.0);
        }
        let size = j.desc.bodies().filter_map(|k| space.body(k)).filter_map(|b| b.aabb).map(|bb| (bb.max - bb.min).length()).fold(0.0f32, f32::max).clamp(0.1, 1.0) * 0.35;
        let x = fa.q.rotate(Vec3::X);
        match j.desc.kind {
            JointKind::Hinge | JointKind::Prismatic | JointKind::Ball => p.line(fa.p - x * size, fa.p + x * size, AXIS, 2.0),
            _ => false,
        };
        if let (JointKind::Hinge, Some([lo, hi])) = (j.desc.kind, j.desc.limit) {
            // The allowed arc, from A's reference direction, and B's reference now.
            let (y, z) = (fa.q.rotate(Vec3::Y), fa.q.rotate(Vec3::Z));
            let steps = 16;
            let mut prev = None;
            for i in 0..=steps {
                let t = lo + (hi - lo) * i as f32 / steps as f32;
                let (sn, cs) = specula_core::dmath::sin_cos(t);
                let q = fa.p + (y * cs + z * sn) * size;
                if let Some(pr) = prev {
                    p.line(pr, q, [color[0], color[1], color[2], 0.8], 1.5);
                }
                prev = Some(q);
            }
            p.line(fb.p, fb.p + fb.q.rotate(Vec3::Y) * size, color, 2.0);
        }
        drawn += 1;
    }
    Json::obj([
        ("drawn", Json::from(drawn)),
        ("omitted", Json::from(omitted)),
        ("holding", Json::from(counts[0])),
        ("strained", Json::from(counts[1])),
        ("past_limit_or_apart", Json::from(counts[2])),
        ("inactive", Json::from(counts[3])),
        ("worst_gap_m", round(worst)),
        (
            "legend",
            Json::obj([
                ("holding", Json::from(ov::hex(OK))),
                ("strained (gap > 1 cm or 2 deg)", Json::from(ov::hex(STRAINED))),
                ("past a limit or apart (> 2 deg past, > 5 cm gap)", Json::from(ov::hex(PAST))),
                ("inactive", Json::from(ov::hex(IDLE))),
                ("axis", Json::from(ov::hex(AXIS))),
            ]),
        ),
    ])
}

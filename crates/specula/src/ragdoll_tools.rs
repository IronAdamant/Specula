//! The ragdoll tools (A4): `ragdoll_set` (limp, powered or off, hits) and `ragdoll_info` (its
//! parts, joints and motion checks as data). Ragdolls themselves are in
//! [`crate::physics::ragdoll`].

use crate::journal::{Kind as JKind, Writer};
use crate::physics::ragdoll::{self, Hit, Mode, Spec, PARAM, PROP};
use crate::tools::{bool_arg, check_keys, entity_key, num_in, string_arg, vec3_arg};
use crate::world::{AnimSpec, EntityKey, Shape, UpdateSpec, WorldError};
use specula_anim::graph::Value;
use specula_anim::play::GraphPlay;
use specula_core::math::Vec3;
use specula_core::Json;
use specula_protocol::{ToolDef, ToolOutput};

/// What `ragdoll_set {fit}` measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    Capsules,
    Hulls,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RagdollCall {
    Set { entity: EntityKey, mode: Option<Mode>, strength: Option<f32>, mass: Option<f32>, relocate: Option<bool>, hit: Option<(String, Vec3)>, param: bool, fit: Option<Fit> },
    Info { entity: EntityKey, checks: bool },
}

impl RagdollCall {
    pub fn is_read(&self) -> bool {
        matches!(self, RagdollCall::Info { .. })
    }
}

/// Parses `ragdoll_set` / `ragdoll_info` (`None`: not a ragdoll tool).
pub fn parse(name: &str, args: &Json) -> Option<Result<RagdollCall, String>> {
    let call = match name {
        "ragdoll_set" => (|| {
            check_keys(args, name, &[&["id", "path", "name", "mode", "strength", "mass", "relocate", "hit", "param", "fit"]])?;
            let mode = match string_arg(args, "mode")? {
                None => None,
                Some(m) => Some(Mode::parse(m).ok_or_else(|| format!("'mode' must be limp, powered or off (got '{m}')"))?),
            };
            let hit = match args.get("hit") {
                None | Some(Json::Null) => None,
                Some(h) => {
                    check_keys(h, "hit", &[&["bone", "impulse"]])?;
                    let bone = string_arg(h, "bone")?.ok_or("'hit.bone' is required (a bone name)")?.to_owned();
                    let impulse = vec3_arg(h, "impulse")?.ok_or("'hit.impulse' is required: [x, y, z] N·s, world")?;
                    if [impulse.x, impulse.y, impulse.z].iter().any(|c| c.abs() > 1.0e5) {
                        return Err("'hit.impulse' must be within ±1e5 N·s".into());
                    }
                    Some((bone, impulse))
                }
            };
            let fit = match args.get("fit") {
                None | Some(Json::Null) | Some(Json::Bool(false)) => None,
                Some(Json::Bool(true)) => Some(Fit::Capsules),
                Some(v) => match v.as_str() {
                    Some("capsules") => Some(Fit::Capsules),
                    Some("hulls") => Some(Fit::Hulls),
                    _ => return Err("'fit' must be true or \"capsules\" (capsule radii) or \"hulls\" (convex hull parts)".into()),
                },
            };
            if mode.is_none() && hit.is_none() && fit.is_none() && args.get("strength").is_none() && args.get("mass").is_none() && args.get("relocate").is_none() {
                return Err("ragdoll_set needs 'mode' (limp, powered, off), a 'hit', 'fit', or a setting to change".into());
            }
            Ok(RagdollCall::Set {
                entity: entity_key(args, name)?,
                mode,
                strength: num_in(args, "strength", 0.0..=10.0)?,
                mass: num_in(args, "mass", 0.01..=1.0e5)?,
                relocate: bool_arg(args, "relocate")?,
                hit,
                param: bool_arg(args, "param")?.unwrap_or(true),
                fit,
            })
        })(),
        "ragdoll_info" => (|| {
            check_keys(args, name, &[&["id", "path", "name", "checks"]])?;
            Ok(RagdollCall::Info { entity: entity_key(args, name)?, checks: bool_arg(args, "checks")?.unwrap_or(true) })
        })(),
        _ => return None,
    };
    Some(call)
}

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let schema = |props: Vec<(&str, Json)>| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props))]);
    let id = || vec![("id", t("integer", "The model entity's id")), ("path", t("string", "Its path")), ("name", t("string", "A top-level entity's name"))];
    let mut set = id();
    set.extend([
        (
            "mode",
            t(
                "string",
                "limp (a death or fall: physics takes the body from the animation), powered (joint motors follow the animation; hits knock it about and it recovers), off (back to the animation: the last pose stays 2 s for the graph's transition out)",
            ),
        ),
        ("strength", t("number", "Powered: motor strength 0..10 (1 = holds the pose firmly; 0.3-0.6 for visible hit reactions)")),
        ("mass", t("number", "Total kg (default 40 kg at scale 1, times scale cubed)")),
        ("relocate", t("boolean", "Off: move the entity under the pelvis first, so the next animation starts there (default true)")),
        ("hit", Json::obj([("type", Json::from("object")), ("description", Json::from("{bone, impulse [x,y,z] N·s world}: knock a bone (a bone without a body passes it to its parent's)"))])),
        ("param", t("boolean", "Also set the anim graph's 'ragdoll' bool parameter (if it has one): true when limp, false otherwise (default true)")),
        (
            "fit",
            Json::obj([
                ("description", Json::from("Measure the parts from the built mesh and store them as data, so the body lies on its surface, not its bones: true or \"capsules\" measures capsule radii (how far each bone's vertices lie from it) into props.ragdoll.radii; \"hulls\" measures convex hull parts (the vertices reaching furthest in 14 directions, at most 16 points, in each bone's rest frame) into props.ragdoll.hulls, and radii for the rest. Copy them into the graph's ragdoll.radius / ragdoll.hull to share them")),
                ("oneOf", Json::arr([Json::obj([("type", Json::from("boolean"))]), Json::obj([("type", Json::from("string")), ("enum", Json::arr([Json::from("capsules"), Json::from("hulls")]))])])),
            ]),
        ),
    ]);
    let mut info = id();
    info.push(("checks", t("boolean", "Run the motion checks on the recorded poses (default true)")));
    vec![
        ToolDef {
            name: "ragdoll_set".into(),
            description: "Turn a rigged model entity into a ragdoll, or back. Capsules along its bones, joined by joints whose \
                          limits come from its anim graph's 'limits' (hinges for one-axis limits such as knees, ball joints with \
                          a cone otherwise), start from the animation at this tick and fall on from how it moved. The pose \
                          plays through the graph's {external: true} nodes: give the graph a 'ragdoll' state (any -> ragdoll \
                          when ragdoll; ragdoll -> get_up when not ragdoll) and a bool parameter 'ragdoll', which this tool \
                          sets. A graph without an external node shows the ragdoll as it is. Writes props.ragdoll (journaled, \
                          undoable, claims apply). Transient entities' ragdolls are cosmetic: they land on static colliders \
                          but never touch the simulation. ragdoll_info shows parts, joints and checks."
                .into(),
            input_schema: schema(set),
        },
        ToolDef {
            name: "ragdoll_info".into(),
            description: "A ragdoll as data: mode, whether it is cosmetic, its parts (bone, part index used in physics answers, \
                          mass, capsule, position, velocity, sleeping), its joints (bone, kind, limits, how far apart and past \
                          its limits, angle), and the motion checks run on the poses it took since it started (ground \
                          penetration, where it came to rest, joint limits, pops where physics took over), with the game's \
                          style.toml thresholds. Not a ragdoll: why (no props.ragdoll, not simulated and why, off)."
                .into(),
            input_schema: schema(info),
        },
    ]
}

fn world_error(e: &WorldError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_pretty())
}

fn round(v: f32) -> Json {
    Json::from((f64::from(v) * 1.0e4).round() / 1.0e4)
}

fn rv3(v: Vec3) -> Json {
    Json::arr([round(v.x), round(v.y), round(v.z)])
}

impl crate::Engine {
    pub(crate) fn ragdoll_call(&mut self, call: RagdollCall, w: Writer, tick: u64) -> ToolOutput {
        match call {
            RagdollCall::Set { entity, mode, strength, mass, relocate, hit, param, fit } => {
                let id = match self.world.resolve(&entity) {
                    Ok(id) => id,
                    Err(e) => return world_error(&e),
                };
                let e = self.world.get(id).expect("resolved");
                if e.shape != Shape::Model {
                    return world_error(&WorldError::new("not_a_model", format!("{} is a {}: only rigged model entities have ragdolls", self.world.path_of(id), e.shape.name())).with("id", id));
                }
                let Some(set) = crate::anim::write_set_of(&self.world, e) else {
                    return world_error(&WorldError::new("no_rig", "its model has no rig (or its recipe or anims file doesn't load): nothing to build a ragdoll from").with("id", id));
                };
                let old = e.props.get(PROP).and_then(|j| Spec::from_json(j).ok());
                let mut spec = old.clone().unwrap_or(Spec { mode: Mode::Off, strength: 1.0, mass: None, relocate: true, hit: None, radii: Vec::new(), hulls: Vec::new() });
                if let Some(fit) = fit {
                    let recipe = e.model.as_ref().map(|m| m.recipe.clone()).unwrap_or_default();
                    let built = {
                        let lib = self.library.lock();
                        lib.handle_for_source(&recipe).and_then(|h| lib.built(h).cloned())
                    };
                    let Some(mesh) = built.as_ref().and_then(|b| b.model.lods.first()).filter(|m| m.is_skinned()) else {
                        return world_error(&WorldError::new("model_building", format!("{recipe}'s skinned mesh isn't built yet: model_build it, then fit")).with("field", "fit"));
                    };
                    spec.radii = ragdoll::fit_radii(&set, mesh).into_iter().map(|(b, r)| (set.skeleton.names[b].clone(), r)).collect();
                    spec.radii.sort_by(|a, b| a.0.cmp(&b.0));
                    spec.hulls = match fit {
                        Fit::Capsules => Vec::new(),
                        Fit::Hulls => ragdoll::fit_hulls(&set, mesh).into_iter().map(|(b, p)| (set.skeleton.names[b].clone(), p)).collect(),
                    };
                    spec.hulls.sort_by(|a, b| a.0.cmp(&b.0));
                }
                if let Some(m) = mode {
                    spec.mode = m;
                }
                if let Some(s) = strength {
                    spec.strength = s;
                }
                if mass.is_some() {
                    spec.mass = mass;
                }
                if let Some(r) = relocate {
                    spec.relocate = r;
                }
                if let Some((bone, impulse)) = hit {
                    if set.skeleton.find(&bone).is_none() {
                        return world_error(&WorldError::new("unknown_bone", format!("no bone '{bone}' (bones: {})", set.skeleton.names.join(", "))).with("field", "hit.bone"));
                    }
                    let seq = old.as_ref().and_then(|s| s.hit.as_ref()).map_or(0, |h| h.seq) + 1;
                    spec.hit = Some(Hit { bone, impulse, seq });
                }
                let mut change = UpdateSpec { props: Some(Json::obj([(PROP, spec.to_json())])), ..UpdateSpec::default() };
                // The graph's `ragdoll` parameter follows: true while limp.
                let mut param_set = Json::Null;
                if param {
                    if let Some((a, gp)) = e.anim.as_ref().and_then(|a| a.graph.as_ref().map(|g| (a, g))) {
                        if set.graph(&gp.graph).is_some_and(|g| g.param(PARAM).is_some()) {
                            let on = spec.mode == Mode::Limp;
                            if gp.param(PARAM) != Some(&Value::Bool(on)) {
                                let mut play = GraphPlay::new(gp.graph.clone());
                                play.set_param(PARAM, Value::Bool(on));
                                change.anim = Some(Some(AnimSpec { speed: a.speed, ..AnimSpec::graph(play) }));
                            }
                            param_set = Json::from(on);
                        }
                    }
                }
                let by = w.name.to_owned();
                match self.swarm.commit(&mut self.world, tick, w, JKind::Set, |wd| wd.update(&EntityKey::Id(id), change, &by).map(|e| e.id)) {
                    Ok((id, seq)) => {
                        let mut out = Json::obj([
                            ("id", Json::from(id)),
                            ("path", Json::from(self.world.path_of(id))),
                            ("ragdoll", spec.to_json()),
                            ("graph_param_ragdoll", param_set.clone()),
                            ("journal", Json::from(seq)),
                        ]);
                        if param && param_set.is_null() {
                            out.set("note", Json::from("its anim graph has no bool parameter 'ragdoll': give it one and a state playing {external: true} to blend into and out of the ragdoll (without one the ragdoll shows as it is)"));
                        }
                        ToolOutput::text(out.to_string())
                    }
                    Err(e) => world_error(&e),
                }
            }
            RagdollCall::Info { entity, checks } => {
                let id = match self.world.resolve(&entity) {
                    Ok(id) => id,
                    Err(e) => return world_error(&e),
                };
                ToolOutput::text(self.ragdoll_info(id, checks).to_string())
            }
        }
    }

    fn ragdoll_info(&self, id: u64, checks: bool) -> Json {
        let world = &self.world;
        let e = world.get(id).expect("resolved");
        let mut out = Json::obj([("id", Json::from(id)), ("path", Json::from(world.path_of(id)))]);
        out.set("props", e.props.get(PROP).cloned().unwrap_or(Json::Null));
        let Some(live) = self.physics.ragdolls.live.get(&id) else {
            out.set("active", Json::from(false));
            let why = match (e.props.get(PROP), self.physics.problems.get(&id)) {
                (None, _) => Json::from("no props.ragdoll (ragdoll_set {mode: limp} makes one)"),
                (Some(_), Some(p)) => p.get("error").cloned().unwrap_or(Json::Null),
                (Some(_), None) => Json::from("off, or it starts at the next physics tick"),
            };
            out.set("why", why);
            return out;
        };
        let skel = &live.set.skeleton;
        out.set("active", Json::from(live.off_since.is_none()));
        out.set("mode", Json::from(live.mode.name()));
        out.set("cosmetic", Json::from(live.cosmetic));
        out.set("started_tick", Json::from(live.started_tick));
        if let Some(t) = live.off_since {
            out.set("off_since_s", Json::from(t));
        }
        out.set("frames_recorded", Json::from(live.frames.len()));
        let parts: Vec<Json> = live
            .parts
            .iter()
            .map(|p| {
                let mut j = Json::obj([
                    ("bone", Json::from(skel.names[p.bone].as_str())),
                    ("part", Json::from(p.bone as u64)),
                    ("parent", p.parent.map_or(Json::Null, |b| Json::from(skel.names[b].as_str()))),
                    ("mass_kg", round(p.mass)),
                ]);
                if p.hull.is_empty() {
                    j.set("shape", Json::from("capsule"));
                    j.set("radius_m", round(p.radius));
                    j.set("length_m", round(2.0 * p.half + 2.0 * p.radius));
                } else {
                    j.set("shape", Json::from("hull"));
                    j.set("points", Json::from(p.hull.len() as u64));
                }
                if let Some(b) = self.physics.space.body(ragdoll::key(id, p.bone)) {
                    j.set("position", rv3(b.center));
                    j.set("velocity", rv3(b.velocity));
                    j.set("sleeping", Json::from(b.sleeping));
                    j.set("kind", Json::from(b.kind.name()));
                }
                j
            })
            .collect();
        out.set("parts", Json::Arr(parts));
        let joints: Vec<Json> = live
            .parts
            .iter()
            .filter(|p| p.joint.is_some())
            .map(|p| {
                let mut j = self.joint_json(ragdoll::key(id, p.bone));
                j.set("bone", Json::from(skel.names[p.bone].as_str()));
                j
            })
            .collect();
        out.set("joints", Json::Arr(joints));
        if checks {
            let rules = e.model.as_ref().map(|m| self.motion_rules(&m.recipe)).and_then(Result::ok).unwrap_or_default();
            let limits = e
                .anim
                .as_ref()
                .and_then(|a| a.graph.as_ref())
                .and_then(|g| live.set.graph(&g.graph))
                .map(|g| g.limits.clone())
                .unwrap_or_else(|| specula_anim::checks::limits_of(&live.set));
            let stem = e.model.as_ref().map(|m| crate::anim_tools::stem_of(&m.recipe)).unwrap_or_default();
            // The skinned mesh (when its build is in) measures penetration and rest by the
            // surface, not the bones: a body lying down rests on its flesh.
            let built = e.model.as_ref().and_then(|m| {
                let lib = self.library.lock();
                lib.handle_for_source(&m.recipe).and_then(|h| lib.built(h).cloned())
            });
            let mesh = built.as_ref().and_then(|b| b.model.lods.first()).filter(|m| m.is_skinned());
            let report = specula_anim::checks::check_frames(&live.set, &format!("{stem}/ragdoll"), &live.frames, live.frame_dt, &limits, &rules, mesh);
            let mut checks = report.to_json(16);
            checks.set("measured_by", Json::from(if mesh.is_some() { "skinned mesh" } else { "bones (the mesh isn't built yet)" }));
            out.set("checks", checks);
        }
        out
    }
}

//! The baked GI tools (`gi_*`, docs/design/GI.md): bake a volume off the frame thread
//! (`gi_bake`, journaled and claim-aware as the agent who asked), read volumes, jobs and
//! residency (`gi_info`), check a bake and sample it at points (`gi_check`), and the runtime
//! quality (`gi_settings`, lead only). Answers are bounded.

use super::*;
use crate::gi::{self, Settings};
use crate::world::{EntityKey, Op, Shape, SpawnSpec, UpdateSpec};
use specula_core::math::Vec3;
use specula_lux as lux;
use specula_protocol::ToolDef;

/// Points `gi_check` samples at most.
const MAX_POINTS: usize = 32;

pub enum GiCall {
    Bake { volume: String, args: Json, around: Option<EntityKey>, threads: Option<usize>, cancel: bool, force: bool, remove: bool, expect_version: Option<u64> },
    Info { volume: Option<String> },
    Check { volume: Option<String>, points: Vec<(Vec3, Vec3)> },
    Settings(Json),
}

pub fn call_class(call: &GiCall) -> schedule::Class {
    match call {
        GiCall::Bake { .. } => schedule::Class::Edit,
        GiCall::Check { .. } => schedule::Class::Big,
        GiCall::Info { .. } | GiCall::Settings(_) => schedule::Class::Read,
    }
}

fn error(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

fn name_arg(args: &Json, required: bool) -> Result<Option<String>, String> {
    match tools::string_arg(args, "volume")? {
        Some(n) if (1..=64).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)) => Ok(Some(n.to_owned())),
        Some(_) => Err("'volume' must be 1..64 characters of A-Z a-z 0-9 _ - .".into()),
        None if required => Ok(Some("main".into())),
        None => Ok(None),
    }
}

fn unit(v: Vec3, what: &str) -> Result<Vec3, String> {
    let l = v.length();
    if !(l > 1e-6 && l.is_finite()) {
        return Err(format!("'{what}' must be a non-zero direction"));
    }
    Ok(v * (1.0 / l))
}

pub fn parse(name: &str, args: &Json) -> Option<Result<GiCall, String>> {
    let r = match name {
        "gi_bake" => (|| {
            tools::check_keys(args, name, &[&["volume", "min", "max", "around", "spacing", "rays", "bounces", "intensity", "sky_below", "margin", "exclude", "threads", "cancel", "force", "remove", "expect_version"]])?;
            let cancel = tools::bool_arg(args, "cancel")?.unwrap_or(false);
            let force = tools::bool_arg(args, "force")?.unwrap_or(false);
            let remove = tools::bool_arg(args, "remove")?.unwrap_or(false);
            let around = match args.get("around").filter(|v| !v.is_null()) {
                Some(v) => Some(tools::key_value(v, "around")?),
                None => None,
            };
            // Checked now (structured errors before anything is written); applied on the engine.
            Settings::patched(&Settings::default(), args)?;
            Ok(GiCall::Bake {
                volume: name_arg(args, true)?.expect("defaulted"),
                args: args.clone(),
                around,
                threads: tools::int_arg(args, "threads", 1, 16)?.map(|n| n as usize),
                cancel,
                force,
                remove,
                expect_version: tools::int_arg(args, "expect_version", 1, u64::MAX >> 11)?,
            })
        })(),
        "gi_info" => (|| {
            tools::check_keys(args, name, &[&["volume"]])?;
            Ok(GiCall::Info { volume: name_arg(args, false)? })
        })(),
        "gi_check" => (|| {
            tools::check_keys(args, name, &[&["volume", "points"]])?;
            let mut points = Vec::new();
            if let Some(list) = args.get("points").filter(|v| !v.is_null()) {
                let list = list.as_array().filter(|l| l.len() <= MAX_POINTS).ok_or(format!("'points' must be a list of at most {MAX_POINTS} {{p: [x, y, z], n: [x, y, z]}}"))?;
                for pt in list {
                    tools::check_keys(pt, "a point", &[&["p", "n"]])?;
                    let p = tools::vec3_arg(pt, "p")?.ok_or("each point needs 'p': [x, y, z]")?;
                    let n = unit(tools::vec3_arg(pt, "n")?.unwrap_or(Vec3::Y), "n")?;
                    points.push((p, n));
                }
            }
            Ok(GiCall::Check { volume: name_arg(args, false)?, points })
        })(),
        "gi_settings" => (|| {
            tools::check_keys(args, name, &[&["quality", "radius_m", "memory_mb"]])?;
            if let Some(q) = tools::string_arg(args, "quality")? {
                gi::Quality::parse(q).ok_or("'quality' must be off, low or high")?;
            }
            tools::num_in(args, "radius_m", 8.0..=4096.0)?;
            tools::num_in(args, "memory_mb", 1.0..=gi::MAX_MEMORY_MB)?;
            Ok(GiCall::Settings(args.clone()))
        })(),
        _ => return None,
    };
    Some(r)
}

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))])
    };
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    vec![
        ToolDef {
            name: "gi_bake".into(),
            description: "Bakes global illumination for a box: irradiance probe volumes (L1 spherical harmonics on a world-aligned \
                          grid, each cell with a visibility mask that hides probes behind walls), baked on the CPU on worker \
                          threads off the frame thread, deterministic (the same scene and settings give the same bytes on every \
                          machine). The probes carry only what bounces and the sky: the sun, the moon and point lights are still \
                          drawn directly. Sky = the world's ambient colour x intensity, so open ground keeps its tuned fill and \
                          enclosed spaces fall dark except where light gets in. Writes the volume's settings to /gi/<volume> \
                          (a group at the box's centre holding props.gi_volume: reserved data, only gi_bake writes it; claims on \
                          /gi or the region hold), answers at once with the estimate; watch gi_info for progress. When done, \
                          the result (hash, scene keys, counts) is committed into props.gi_volume.baked as you (journaled, \
                          undoable). An unchanged scene with unchanged settings comes from the store within a frame or two \
                          (gi_info's last_job says cached). Geometry: \
                          shown, persistent, opaque lit cubes, quads, spheres, lathes and models in the box plus 'margin' \
                          (moving bodies, animated models, transient game presentation, glass and 'exclude' subtrees are left \
                          out). Bounded: at most 65,536 probes, 2^30 ray casts and a 256 MB hit cache per bake (structured \
                          errors name what to change). One bake at a time; cancel: true stops it (its starter; another \
                          agent with force: true, which needs lead authority and the right to write /gi/<volume>; name the \
                          volume). At most 64 volumes. 'threads' past the default needs lead authority. Not inside playtests (a bake lands by wall clock)."
                .into(),
            input_schema: schema(
                vec![
                    ("volume", t("string", "The volume's name (default \"main\"): /gi/<volume>")),
                    ("min", t("array", "The box's low corner [x, y, z] (metres)")),
                    ("max", t("array", "The box's high corner [x, y, z]")),
                    ("around", Json::obj([("description", Json::from("Instead of min/max: the box of this entity's subtree (id, \"/path\" or unique name), 1 m larger"))])),
                    ("spacing", t("number", "Metres between probes, 0.25..8 (default 1; 2 for big open areas, 0.5 for interiors)")),
                    ("rays", t("integer", "Rays per probe and pass, 16..1024 (default 256)")),
                    ("bounces", t("integer", "Indirect bounces after the first, 0..4 (default 2)")),
                    ("intensity", t("number", "Multiplies the volume's light at runtime, 0..16 (default 1; no rebake)")),
                    ("sky_below", t("number", "The sky below the horizon as a share of above, 0..1 (default 1; lower for a floating slab over dark mist)")),
                    ("margin", t("number", "Geometry this far outside the box still occludes and bounces, 0..64 m (default 8)")),
                    ("exclude", t("array", "Subtrees left out (paths, at most 16), e.g. what the player builds")),
                    ("threads", t("integer", "Worker threads, 1..16 (default: all cores but one, at most 4)")),
                    ("cancel", t("boolean", "Stop the running bake")),
                    ("force", t("boolean", "With cancel: stop another agent's bake (needs lead authority)")),
                    ("remove", t("boolean", "Delete the volume (/gi/<volume>; undoable): its area falls back to the flat ambient")),
                    ("expect_version", t("integer", "Only if /gi/<volume> is still at this version")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "gi_info".into(),
            description: "Baked GI as data: each volume's settings and status (fresh; stale with reasons geometry, lights, \
                          environment, settings; unbaked; missing: its file isn't in the store, rebake to get it back; baking), \
                          the bake's result (hash, probes valid / relocated / invalid, triangles, lights), chunks resident; the \
                          running job's progress (phase prepare/validity/visibility/trace/bounce, probes done of total, elapsed) and \
                          the last job's outcome (with its timings); the runtime quality and residency (chunks, bytes on the GPU, \
                          buffer rebuilds, and gpu_error when the GPU couldn't take the set: frames then draw the flat ambient); \
                          whether the GI pipelines are ready. GPU time of the sampling is part of perf_frame's scene pass."
                .into(),
            input_schema: schema(vec![("volume", t("string", "Only this volume"))], &[]),
        },
        ToolDef {
            name: "gi_check".into(),
            description: "Checks of a baked volume: invalid and relocated probe shares (probes inside geometry), cells with walls \
                          through them (their masks hide probes), how many cell octants would leak light without the masks, mean and peak irradiance, staleness. With 'points' ([{p: [x, y, z], n: [x, y, z]}], at most \
                          32): the irradiance the shaders compute there (linear, times the volume's intensity; luminance), the \
                          volume and fade, and each probe's share (trilinear, backface, visible, weight)."
                .into(),
            input_schema: schema(
                vec![("volume", t("string", "One volume (default: all, at most 8)")), ("points", t("array", "Points to sample: [{p: [x, y, z], n: [x, y, z] (default up)}]"))],
                &[],
            ),
        },
        ToolDef {
            name: "gi_settings".into(),
            description: format!(
                "The runtime GI tier for this machine (not world data): off (the flat ambient); low (an ambient cube per \
                 draw, each face of its box sampled once on the CPU; draws wider than 2 probe spacings and terrain read the \
                 probes' lean copy with the cells' visibility masks, terrain per vertex; water its draw's cube: the \
                 integrated GPU's default, measured {low} ms of its scene pass in Sepulcra); high (every lit pixel samples its \
                 eight probes with masks, reaches and backface weights: other GPUs' default; measured {high} ms on an \
                 integrated GPU, so choosing it there answers with a warning). Both tiers keep the masks: a sealed room stays \
                 dark at either (tested); low is coarser on small props and has no reaches. The answer says the active tier \
                 and the adapter's default. radius_m (chunks resident around the window's and captures' cameras, default 96) \
                 and memory_mb (default 16). Changing them needs lead authority; anyone may read them.",
                low = gi::LOW_COST_IGPU_MS,
                high = gi::HIGH_COST_IGPU_MS
            ),
            input_schema: schema(
                vec![
                    ("quality", t("string", "off | low | high")),
                    ("radius_m", t("number", "8..4096")),
                    ("memory_mb", t("number", "1..32 (the GPU side holds 524,288 probes at most, 64 bytes each)")),
                ],
                &[],
            ),
        },
    ]
}

/// Whether `w` may write `/gi/<volume>` (claims on it, its ancestors and its region), without
/// writing anything: a bake whose settings are unchanged writes nothing at its start, yet it
/// occupies the baker and lands its result there.
pub(crate) fn may_write_volume(swarm: &crate::swarm::Swarm, world: &crate::world::World, volume: &str, w: Writer) -> Result<(), WorldError> {
    let path = format!("/{}/{volume}", gi::GROUP);
    let Some(e) = world.find_path(&path).ok().and_then(|id| world.get(id)) else { return Ok(()) };
    let change = crate::world::Change::Set { path: path.clone(), prev_path: None, before: Some(Box::new(e.clone())), after: Box::new(e.clone()) };
    swarm.claims.check(world, &[change], w, std::time::Instant::now())
}

/// Whether `w` may cancel the running bake of `job_volume`, started by session `job_session`:
/// `volume` must name it; another agent's bake needs `force`, force on another agent's object
/// needs the lead (`authority::may_force`), and `w` must still be allowed to write
/// `/gi/<volume>` (claims).
#[allow(clippy::too_many_arguments)]
pub(crate) fn may_cancel(swarm: &crate::swarm::Swarm, world: &crate::world::World, job_volume: &str, job_session: &str, volume: &str, w: Writer, force: bool) -> Result<(), WorldError> {
    if job_volume != volume {
        return Err(WorldError::new("wrong_volume", format!("the running bake is of /{}/{job_volume} (not {volume}): name it to cancel it", gi::GROUP)));
    }
    if w.session == Some(job_session) {
        return Ok(());
    }
    if !force {
        return Err(WorldError::new("not_job_owner", "another agent started this bake: cancel it with force: true (needs lead authority), or ask them").with("volume", volume));
    }
    crate::authority::may_force(w.session.map_or(crate::authority::Authority::Agent, |s| crate::authority::authority(swarm, s)), false, "gi_bake cancel")?;
    may_write_volume(swarm, world, volume, w)
}

fn round(x: f64, places: i32) -> Json {
    let k = 10f64.powi(places);
    Json::from((x * k).round() / k)
}

fn v3(v: Vec3) -> Json {
    Json::arr([round(v.x as f64, 3), round(v.y as f64, 3), round(v.z as f64, 3)])
}

fn rgb(c: [f32; 3]) -> Json {
    Json::arr(c.map(|x| round(x as f64, 4)))
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

impl Engine {
    pub(crate) fn gi_call(&mut self, call: GiCall, w: Writer, tick: u64, session: &SessionInfo) -> ToolOutput {
        let _z = profile::zone("gi_tool");
        match call {
            GiCall::Bake { volume, args, around, threads, cancel, force, remove: true, expect_version } => {
                let _ = (args, around, threads, cancel, force);
                self.gi_remove(&volume, expect_version, w, tick)
            }
            GiCall::Bake { volume, args, around, threads, cancel, force, remove: false, expect_version } => self.gi_bake(&volume, &args, around, threads, (cancel, force), expect_version, w, tick),
            GiCall::Info { volume } => self.gi_info(volume.as_deref()),
            GiCall::Check { volume, points } => self.gi_check(volume.as_deref(), &points),
            GiCall::Settings(patch) => self.gi_settings(&patch, session),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn gi_bake(&mut self, volume: &str, args: &Json, around: Option<EntityKey>, threads: Option<usize>, (cancel, force): (bool, bool), expect_version: Option<u64>, w: Writer, tick: u64) -> ToolOutput {
        if cancel {
            let Some(job) = &self.gi.job else { return error("no_bake", "no bake is running") };
            if let Err(e) = may_cancel(&self.swarm, &self.world, &job.volume, &job.key, volume, w, force) {
                return world_error(&e);
            }
            if job.control.progress().phase == lux::Phase::Done {
                return ToolOutput::error(
                    Json::obj([("error", Json::from("too_late")), ("message", Json::from("the bake has finished: its result lands this frame (undo it with world_undo, or bake again)")), ("job", job.progress_json())]).to_string(),
                );
            }
            job.control.cancel();
            self.gi.cancelled_by = Some(w.name.to_owned());
            return ToolOutput::text(Json::obj([("cancelled", Json::from(true)), ("job", job.progress_json())]).to_string());
        }
        if let Some(job) = &self.gi.job {
            return ToolOutput::error(Json::obj([("error", Json::from("busy")), ("message", Json::from("a bake is running (one at a time): wait for it (gi_info) or cancel it (gi_bake cancel: true)")), ("job", job.progress_json())]).to_string());
        }
        let path = format!("/{}/{volume}", gi::GROUP);
        let existing = self.world.find_path(&path).ok();
        // Volumes past MAX_VOLUMES would bake and never show: refused.
        if existing.is_none() && self.gi.volume_names().len() >= gi::MAX_VOLUMES {
            return error("over_limit", format!("at most {} GI volumes: remove one (gi_bake remove: true) or bake a bigger box", gi::MAX_VOLUMES));
        }
        // More threads than the default slow the frame thread and other agents: the lead's call.
        if threads.is_some_and(|t| t > gi::default_threads()) {
            let have = w.session.map_or(crate::authority::Authority::Agent, |s| crate::authority::authority(&self.swarm, s));
            let what = format!("'threads' past the default ({})", gi::default_threads());
            if let Err(e) = crate::authority::require(have, crate::authority::Authority::Lead, crate::authority::Site::GiThreads, &what) {
                return world_error(&e);
            }
        }
        let base = existing.and_then(|id| self.world.get(id)).and_then(|e| e.props.get(gi::PROP)).and_then(|p| Settings::parse(p).ok());
        let mut args = args.clone();
        if let Some(key) = &around {
            if args.get("min").is_some_and(|v| !v.is_null()) || args.get("max").is_some_and(|v| !v.is_null()) {
                return error("invalid_value", "give 'around' or 'min'/'max', not both");
            }
            let id = match self.world.resolve(key) {
                Ok(id) => id,
                Err(e) => return world_error(&e),
            };
            let Some((lo, hi)) = capture::subtree_bounds(&self.world, id) else {
                return error("no_bounds", format!("{} has no geometry to bake around", self.world.path_of(id)));
            };
            args.set("min", Json::arr([lo.x - 1.0, lo.y - 1.0, lo.z - 1.0].map(Json::from)));
            args.set("max", Json::arr([hi.x + 1.0, hi.y + 1.0, hi.z + 1.0].map(Json::from)));
        }
        let settings = match &base {
            Some(b) => Settings::patched(b, &args),
            None => Settings::parse(&args).map_err(|e| if e.contains("needs 'min'") { format!("{e} (or 'around'): /gi/{volume} doesn't exist yet") } else { e }),
        };
        let settings = match settings {
            Ok(s) => s,
            Err(e) => return error("invalid_value", e),
        };
        let plan = match lux::plan(&settings.request()) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(gi::bake_error_json(&e).to_string()),
        };
        let input = {
            let lib = self.library.lock();
            match gi::gather(&self.world, &lib, &settings) {
                Ok(i) => i,
                Err(e) => return error(e.code, e.message),
            }
        };
        if !input.building.is_empty() {
            return ToolOutput::error(
                Json::obj([
                    ("error", Json::from("models_building")),
                    ("message", Json::from("models in the box are still building: bake once they are ready (model_build waits for them)")),
                    ("entities", Json::arr(input.building.iter().map(|p| Json::from(p.as_str())))),
                ])
                .to_string(),
            );
        }
        // The lights' shadow rays count too (refused before anything is written).
        if let Err(e) = lux::check_work(&plan, settings.rays, input.lights.len()) {
            return ToolOutput::error(gi::bake_error_json(&e).to_string());
        }
        // The settings, as the agent (claims on /gi and the region hold): only when they changed.
        let old_baked = existing.and_then(|id| self.world.get(id)).and_then(|e| e.props.get(gi::PROP)).and_then(|p| p.get("baked")).cloned();
        let mut prop = settings.to_json();
        prop.set("baked", old_baked.clone().unwrap_or(Json::Null));
        // Props come back with their keys sorted: compare what they mean.
        let unchanged = base.as_ref() == Some(&settings);
        // Unchanged settings write nothing, but the bake still occupies the baker and lands a result
        // there: claims on the volume (path or region) hold all the same.
        if unchanged && expect_version.is_none() {
            if let Err(e) = may_write_volume(&self.swarm, &self.world, volume, w) {
                return world_error(&e);
            }
        }
        // At most one commit in this call (the settings); the result lands later in its own, when
        // the worker hands it back, from the store (a scene baked before: the file is read on the
        // worker, not the engine thread) or freshly baked.
        let mut seq = None;
        if !unchanged || expect_version.is_some() {
            let mut ops = Vec::new();
            if self.world.find_path(&format!("/{}", gi::GROUP)).is_err() {
                let mut g = SpawnSpec::new(Shape::Group);
                g.name = Some(gi::GROUP.into());
                ops.push(Op::Spawn(g));
            }
            let props = Json::obj([(gi::PROP, prop)]);
            match existing {
                Some(id) => ops.push(Op::Set(EntityKey::Id(id), UpdateSpec { props: Some(props), position: Some(settings.centre()), expect_version, ..UpdateSpec::default() })),
                None => {
                    if expect_version.is_some() {
                        return error("version_conflict", format!("{path} doesn't exist yet, so it has no version to expect"));
                    }
                    let mut s = SpawnSpec::new(Shape::Group);
                    s.name = Some(volume.to_owned());
                    s.parent = Some(EntityKey::Path(format!("/{}", gi::GROUP)));
                    s.position = settings.centre();
                    s.props = props;
                    ops.push(Op::Spawn(s));
                }
            }
            let by = w.name.to_string();
            match self.world.commit(&mut self.swarm, tick, w, Kind::Batch, crate::write_policy::Rule::Gi, |wd| wd.apply_batch(ops, &by).map_err(|(_, e)| e)) {
                Ok((_, s)) => seq = s,
                Err(e) => return world_error(&self.conflict_help(e, Some(&crate::conflict::Yours::named(vec![format!("props.{}", gi::PROP)])))),
            }
        }
        let threads = threads.unwrap_or_else(gi::default_threads);
        let mut out = Json::obj([("volume", Json::from(volume)), ("path", Json::from(path.as_str())), ("journal", seq.map_or(Json::Null, Json::from))]);
        let l = plan.lattice;
        out.set(
            "estimate",
            Json::obj([
                ("probes", Json::from(l.count())),
                ("dims", Json::arr(l.dims.map(Json::from))),
                ("instances", Json::from(input.instances.len())),
                ("lights", Json::from(input.lights.len())),
                ("ray_casts", Json::from(lux::work_with_lights(&plan, settings.rays, input.lights.len()))),
                ("cache_mb", round(plan.cache_bytes as f64 / 1_048_576.0, 1)),
                ("threads", Json::from(threads)),
            ]),
        );
        // The GI pipelines (the set this tier draws with) start building now, on a worker, so the
        // bake's first frames and captures don't wait for them.
        if self.gi.quality != gi::Quality::Off {
            self.gpu.request_gi_variants(self.gi.quality.gpu());
        }
        let session = w.session.map(str::to_owned).unwrap_or_default();
        match gi::Job::start(volume.to_owned(), w.name.to_owned(), session, settings, input, plan, self.gi.store.clone(), threads) {
            Ok(job) => {
                out.set("status", Json::from("baking"));
                out.set("job", job.progress_json());
                self.gi.job = Some(job);
                ToolOutput::text(out.to_string())
            }
            Err(e) => error("no_thread", e),
        }
    }

    /// gi_bake remove: deletes the volume as the agent (`Rule::Gi`); a bake of it stops.
    fn gi_remove(&mut self, volume: &str, expect_version: Option<u64>, w: Writer, tick: u64) -> ToolOutput {
        let path = format!("/{}/{volume}", gi::GROUP);
        let Ok(id) = self.world.find_path(&path) else {
            return error("unknown_volume", format!("no volume at {path} (there are: {})", self.gi.volume_names().join(", ")));
        };
        let deleted = self.world.commit(&mut self.swarm, tick, w, Kind::Delete, crate::write_policy::Rule::Gi, |wd| wd.delete(&EntityKey::Id(id), expect_version).map(|_| ()));
        match deleted {
            Ok((_, seq)) => {
                if let Some(job) = self.gi.job.as_ref().filter(|j| j.volume == volume) {
                    job.control.cancel();
                }
                ToolOutput::text(Json::obj([("volume", Json::from(volume)), ("removed", Json::from(true)), ("journal", seq.map_or(Json::Null, Json::from))]).to_string())
            }
            Err(e) => world_error(&self.conflict_help(e, Some(&crate::conflict::Yours::named(vec!["(delete)".into()])))),
        }
    }

    /// Commits a finished bake into its volume as the agent who asked (`Rule::Gi`).
    #[allow(clippy::too_many_arguments)]
    fn gi_commit_result(&mut self, volume: &str, by: &str, session: &str, settings: &Settings, keys: gi::SceneKeys, hash: u64, stats: &lux::Stats, cached: bool, tick: u64) -> Json {
        let path = format!("/{}/{volume}", gi::GROUP);
        let Some(id) = self.world.find_path(&path).ok() else {
            return Json::obj([("ok", Json::from(false)), ("error", Json::from("volume_gone")), ("message", Json::from(format!("{path} was deleted while it baked: the result is kept in the store only")))]);
        };
        // Settings changed meanwhile (another gi_bake, an undo): this result is for the old ones.
        let now = self.world.get(id).and_then(|e| e.props.get(gi::PROP)).and_then(|p| Settings::parse(p).ok());
        if now.as_ref().map(Settings::key) != Some(settings.key()) {
            return Json::obj([("ok", Json::from(false)), ("error", Json::from("settings_changed")), ("message", Json::from(format!("{path}'s settings changed while it baked: bake again")))]);
        }
        let baked = gi::BakedInfo::of(hash, keys, settings.key(), gi::counts_of(stats), stats.probes);
        let mut prop = now.map_or_else(|| settings.to_json(), |s| s.to_json());
        prop.set("baked", baked.to_json());
        let recorded = self.world.get(id).and_then(|e| e.props.get(gi::PROP)).and_then(|p| p.get("baked")).and_then(gi::BakedInfo::parse);
        if recorded.as_ref().is_some_and(|r| r.hash == baked.hash && r.keys == baked.keys && r.settings == baked.settings) {
            // Already recorded (the same bake again): nothing to journal.
            return Json::obj([("ok", Json::from(true)), ("baked", baked.to_json()), ("journal", Json::Null), ("cached", Json::from(cached)), ("unchanged", Json::from(true))]);
        }
        let w = Writer::new(by, (!session.is_empty()).then_some(session));
        let update = UpdateSpec { props: Some(Json::obj([(gi::PROP, prop)])), ..UpdateSpec::default() };
        let committed = self.world.commit(&mut self.swarm, tick, w, Kind::Set, crate::write_policy::Rule::Gi, |wd| wd.update(&EntityKey::Id(id), update, by).map(|_| ()));
        match committed {
            Ok((_, seq)) => Json::obj([("ok", Json::from(true)), ("baked", baked.to_json()), ("journal", seq.map_or(Json::Null, Json::from)), ("cached", Json::from(cached))]),
            Err(e) => {
                let mut j = e.to_json();
                j.set("ok", Json::from(false));
                j.set("note", Json::from("the result is in the store: gi_bake again (it finishes at once) once the write is allowed"));
                j
            }
        }
    }

    /// Lands a finished bake (between frames).
    pub(crate) fn gi_pump(&mut self) {
        let Some(job) = &self.gi.job else { return };
        let Some(result) = job.poll() else { return };
        let job = self.gi.job.take().expect("checked");
        let tick = self.clock.tick();
        let mut json = Json::obj([("volume", Json::from(job.volume.as_str())), ("by", Json::from(job.by.as_str())), ("elapsed_ms", Json::from(job.started.elapsed().as_millis() as u64))]);
        let cancelled_by = self.gi.cancelled_by.take();
        let ok = match result {
            // Cancelled after it finished computing: kept in the store, not committed (the
            // cancel said it would stop).
            Ok(_) if cancelled_by.is_some() => {
                json.set("cancelled_by", Json::from(cancelled_by.unwrap_or_default()));
                json.set("note", Json::from("cancelled: the result is kept in the store only (gi_bake again lands it at once)"));
                false
            }
            Ok(done) => {
                let s = &done.stats;
                json.set(
                    "stats",
                    Json::obj([
                        ("probes", Json::from(s.probes)),
                        ("valid", Json::from(s.valid)),
                        ("relocated", Json::from(s.relocated)),
                        ("invalid", Json::from(s.invalid)),
                        ("triangles", Json::from(s.triangles)),
                        ("lights", Json::from(s.lights)),
                        ("rays", Json::from(s.rays)),
                        ("ms", Json::obj([("bvh", round(s.ms[0], 1)), ("validity", round(s.ms[1], 1)), ("trace", round(s.ms[2], 1)), ("bounces", round(s.ms[3], 1)), ("total", round(s.ms[4], 1))])),
                        ("mrays_per_s", round(s.rays as f64 / (s.ms[4].max(1e-3) * 1e3), 2)),
                        ("threads", Json::from(job.threads)),
                    ]),
                );
                // Lights inside a solid light nothing: say so (a bake from the store repeats
                // what its original found this run).
                let inside: Vec<String> = if done.cached {
                    self.gi.lights_inside.get(&done.hash).cloned().unwrap_or_default()
                } else {
                    let names: Vec<String> = done.lights_inside.iter().filter_map(|&i| job.light_names.get(i).cloned()).collect();
                    self.gi.lights_inside.insert(done.hash, names.clone());
                    names
                };
                let warnings = gi::light_warnings(&inside);
                for n in &inside {
                    specula_core::trace_event!(&crate::trace_sink::JOBS, Warn, "gi.light_inside_geometry", volume = job.volume.as_str(), light = n.as_str());
                }
                if !warnings.is_empty() {
                    json.set("warnings", Json::Arr(warnings.clone()));
                }
                self.gi.warnings.insert(job.volume.clone(), warnings);
                let r = self.gi_commit_result(&job.volume, &job.by, &job.key, &job.settings, job.keys, done.hash, &done.stats, done.cached, tick);
                let ok = r.get("ok").and_then(Json::as_bool).unwrap_or(false);
                json.set("result", r);
                ok
            }
            Err(e) => {
                json.set("error", gi::bake_error_json(&e));
                false
            }
        };
        json.set("ok", Json::from(ok));
        if !ok {
            specula_core::trace_event!(&crate::trace_sink::JOBS, Warn, "gi.bake_failed", volume = job.volume.as_str(), result = json.to_string());
        }
        self.gi.last = Some(gi::Outcome { json });
    }

    fn gi_info(&mut self, volume: Option<&str>) -> ToolOutput {
        let vols = {
            let lib = self.library.lock();
            self.gi.volumes_json(&self.world, &lib)
        };
        let vols: Vec<Json> = vols.into_iter().filter(|v| volume.is_none_or(|n| v.get("name").and_then(Json::as_str) == Some(n))).collect();
        if let (Some(n), true) = (volume, vols.is_empty()) {
            return error("unknown_volume", format!("no volume '{n}' under /gi (there are: {})", self.gi.volume_names().join(", ")));
        }
        let (gpu_bytes, gpu_rebuilds) = self.gpu.gi_stats();
        let variants = self.gpu.pipeline_variants();
        // The set this tier draws with (lean for low, per pixel for high).
        let ready = if self.gi.quality.gpu().per_pixel() { variants.gi_pixel } else { variants.gi };
        let gpu_error = self.gpu.gi_failure().map_or(Json::Null, Json::from);
        let mut runtime = Json::obj([
            ("gpu_error", gpu_error),
            ("quality", Json::from(self.gi.quality.name())),
            ("tier", Json::from(self.gi.quality.describe())),
            ("radius_m", Json::from(self.gi.radius_m)),
            ("memory_mb", Json::from(self.gi.memory_mb)),
            ("pipelines_ready", Json::from(ready)),
            ("gpu_bytes", Json::from(gpu_bytes)),
            ("gpu_buffer_rebuilds", Json::from(gpu_rebuilds)),
        ]);
        runtime.set("residency", self.gi.residency_json());
        let out = Json::obj([
            ("volumes", Json::Arr(vols)),
            ("job", self.gi.job.as_ref().map_or(Json::Null, gi::Job::progress_json)),
            ("last_job", self.gi.last.as_ref().map_or(Json::Null, |o| o.json.clone())),
            ("runtime", runtime),
            ("store", Json::from(self.gi.store.dir.display().to_string())),
        ]);
        ToolOutput::text(out.to_string())
    }

    fn gi_check(&mut self, volume: Option<&str>, points: &[(Vec3, Vec3)]) -> ToolOutput {
        // Bakes load on a worker: a check waits for a load that is running (at most 30 s).
        self.gi.wait_loads(&self.world, std::time::Duration::from_secs(30));
        let vols = {
            let lib = self.library.lock();
            self.gi.volumes_json(&self.world, &lib)
        };
        let names: Vec<String> = self.gi.volume_names().into_iter().filter(|n| volume.is_none_or(|v| v == n)).take(8).collect();
        if let (Some(n), true) = (volume, names.is_empty()) {
            return error("unknown_volume", format!("no volume '{n}' under /gi"));
        }
        let mut checks = Vec::new();
        for name in &names {
            let status = vols.iter().find(|v| v.get("name").and_then(Json::as_str) == Some(name.as_str())).cloned().unwrap_or(Json::Null);
            let Some((field, _)) = self.gi.field_of(name) else {
                checks.push(Json::obj([("volume", Json::from(name.as_str())), ("status", status.get("status").cloned().unwrap_or(Json::Null)), ("note", Json::from("not baked or not loaded: nothing to check"))]));
                continue;
            };
            let mut c = check_field(name, field, &status);
            // What its last bake this run found (a light inside a solid lights nothing).
            if let Some(w) = self.gi.warnings.get(name).filter(|w| !w.is_empty()) {
                c.set("warnings", Json::Arr(w.clone()));
            }
            checks.push(c);
        }
        let mut samples = Vec::new();
        for (p, n) in points {
            let mut hit = None;
            for name in &names {
                let Some((field, vol)) = self.gi.field_of(name) else { continue };
                let s = field.sample(*p, *n, Vec3::ZERO, true);
                if s.fade > 0.0 && hit.as_ref().is_none_or(|(f, ..): &(f32, String, Json)| s.fade > *f) {
                    let k = vol.settings.as_ref().map_or(1.0, |s| s.intensity);
                    let mut shares = Vec::new();
                    field.sample_with(*p, *n, Vec3::ZERO, true, |c| {
                        shares.push(Json::obj([
                            ("probe", Json::from(c.index)),
                            ("at", v3(field.lattice.position(c.index) + Vec3::new(field.probes[c.index].offset[0], field.probes[c.index].offset[1], field.probes[c.index].offset[2]))),
                            ("trilinear", round(c.trilinear as f64, 3)),
                            ("backface", round(c.backface as f64, 3)),
                            ("visible", Json::from(c.visible)),
                            ("weight", round(c.weight as f64, 4)),
                        ]))
                    });
                    let e = s.irradiance.map(|x| x * k);
                    let j = Json::obj([
                        ("irradiance", rgb(e)),
                        ("luminance", round(luma(e) as f64, 4)),
                        ("fade", round(s.fade as f64, 3)),
                        ("probes", Json::Arr(shares)),
                    ]);
                    hit = Some((s.fade, name.clone(), j));
                }
            }
            let mut j = Json::obj([("p", v3(*p)), ("n", v3(*n))]);
            match hit {
                Some((_, name, s)) => {
                    j.set("volume", Json::from(name));
                    if let Json::Obj(fields) = s {
                        for (k, v) in fields {
                            j.set(k.as_str(), v);
                        }
                    }
                }
                None => {
                    let env = &self.world.env().light;
                    j.set("volume", Json::Null);
                    j.set("note", Json::from(format!("outside every baked volume: the flat ambient ({:?} x {})", env.ambient_color, env.ambient_intensity)));
                }
            }
            samples.push(j);
        }
        let mut out = Json::obj([("volumes", Json::Arr(checks))]);
        if !points.is_empty() {
            out.set("points", Json::Arr(samples));
        }
        ToolOutput::text(out.to_string())
    }

    fn gi_settings(&mut self, patch: &Json, session: &SessionInfo) -> ToolOutput {
        let changing = patch.as_object().is_some_and(|o| !o.is_empty());
        if changing {
            let have = crate::authority::authority(&self.swarm, &session.id);
            if let Err(e) = crate::authority::require(have, crate::authority::Authority::Lead, crate::authority::Site::GiSettings, "changing gi_settings (the GI tier shapes the human's window too; anyone may read it)") {
                return crate::world_error(&e);
            }
        }
        if let Ok(Some(q)) = tools::string_arg(patch, "quality") {
            if let Some(q) = gi::Quality::parse(q) {
                self.gi.quality = q;
                // The tier's pipeline set starts building now, on a worker (each tier builds only
                // its own; `gi_info`'s pipelines_ready says when it is there).
                if q != gi::Quality::Off && self.gi.wants_pipelines() {
                    self.gpu.request_gi_variants(q.gpu());
                }
            }
        }
        if let Ok(Some(r)) = tools::num_in(patch, "radius_m", 8.0..=4096.0) {
            self.gi.radius_m = r;
        }
        if let Ok(Some(m)) = tools::num_in(patch, "memory_mb", 1.0..=gi::MAX_MEMORY_MB) {
            self.gi.memory_mb = m;
        }
        self.gi.dirty = true;
        let integrated = self.gpu.adapter().kind == specula_gpu::AdapterKind::Integrated;
        let mut out = Json::obj([
            ("quality", Json::from(self.gi.quality.name())),
            ("tier", Json::from(self.gi.quality.describe())),
            ("adapter_default", Json::from(gi::Quality::default_for(integrated).name())),
            ("radius_m", Json::from(self.gi.radius_m)),
            ("memory_mb", Json::from(self.gi.memory_mb)),
        ]);
        if let Some(w) = gi::tier_warning(self.gi.quality, integrated) {
            out.set("warning", w);
        }
        ToolOutput::text(out.to_string())
    }

    /// Small spheres at the resident probes (`sight_frame gi: "probes"`), shaded by the probes'
    /// own irradiance (the capture renders in the irradiance view); invalid ones aren't drawn.
    pub(crate) fn gi_probe_draws(&self, eye: Vec3, out: &mut Vec<specula_gpu::DrawItem>) -> (usize, usize) {
        let Some(sphere) = self.meshes.get(Shape::Sphere) else { return (0, 0) };
        let marks = self.gi.probe_marks(eye, 4096);
        let mut drawn = 0;
        let mut invalid = 0;
        for (p, valid, s) in marks {
            if !valid {
                invalid += 1;
                continue;
            }
            let r = 0.12 * s;
            let m = specula_core::math::Mat4::from_trs(p, specula_core::math::Quat::IDENTITY, Vec3::new(r, r, r));
            out.push(specula_gpu::DrawItem::new(sphere, m.to_cols_array(), [1.0; 4]));
            drawn += 1;
        }
        (drawn, invalid)
    }
}

/// Checks of one loaded field (bounded: at most 65,536 cells, eight octant samples each).
fn check_field(name: &str, f: &lux::Field, status: &Json) -> Json {
    let l = f.lattice;
    let n = f.probes.len().max(1);
    let invalid = f.probes.iter().filter(|p| p.weight <= 0.0).count();
    let relocated = f.probes.iter().filter(|p| p.offset != [0.0; 3]).count();
    let mut walled = 0usize;
    let mut leaks = 0usize;
    let mut cells = 0usize;
    let mut peak = 0.0f32;
    let mut sum = 0.0f64;
    for (i, p) in f.probes.iter().enumerate() {
        if p.weight > 0.0 {
            let y = p.sh.luma();
            peak = peak.max(y);
            sum += f64::from(y);
        }
        let c = l.coords(i);
        if (0..3).any(|a| c[a] + 1 >= l.dims[a]) {
            continue;
        }
        cells += 1;
        if p.mask != lux::field::ALL_VISIBLE {
            walled += 1;
            // Each octant's centre, sampled facing up with and without the masks.
            let lo = l.at(c);
            for o in 0..8u32 {
                let q = lo + Vec3::new(0.25 + 0.5 * (o & 1) as f32, 0.25 + 0.5 * ((o >> 1) & 1) as f32, 0.25 + 0.5 * (o >> 2) as f32) * l.spacing;
                // The sampler biases along the normal: start a quarter spacing below.
                let at = q - Vec3::Y * (lux::field::NORMAL_BIAS * l.spacing);
                let masked = luma(f.sample(at, Vec3::Y, Vec3::ZERO, true).irradiance);
                let open = luma(f.sample(at, Vec3::Y, Vec3::ZERO, false).irradiance);
                if open > masked * 1.5 + 0.01 {
                    leaks += 1;
                }
            }
        }
    }
    let valid = (n - invalid).max(1);
    Json::obj([
        ("volume", Json::from(name)),
        ("status", status.get("status").cloned().unwrap_or(Json::Null)),
        ("stale", status.get("stale").cloned().unwrap_or(Json::Null)),
        ("probes", Json::from(f.probes.len())),
        ("dims", Json::arr(l.dims.map(Json::from))),
        ("spacing", Json::from(l.spacing)),
        ("invalid_share", round(invalid as f64 / n as f64, 3)),
        ("relocated_share", round(relocated as f64 / n as f64, 3)),
        ("cells", Json::from(cells)),
        ("cells_with_walls", Json::from(walled)),
        ("octants_leaking_without_masks", Json::from(leaks)),
        ("mean_luminance", round(sum / valid as f64, 4)),
        ("peak_luminance", round(f64::from(peak), 4)),
    ])
}

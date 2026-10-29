//! The effect tools (`vfx_*`, docs/design/VFX.md): the effect library as data (`vfx_get`,
//! `vfx_set`: journaled, claim-aware world entities under `/vfx`), filmstrips of an effect over
//! time with facts per frame (`vfx_preview`), budget and readability checks (`vfx_check`), and
//! what effects cost now (`vfx_info`). Decals and effects on entities are `world_set` components.
//! Answers are bounded; previews render offscreen like sight captures.

use super::*;
use crate::sight_overlay::Painter;
use crate::vfx::{View, EFFECT_PROP, EFFECT_TAG, LIBRARY_GROUP};
use crate::world::{EntityKey, Op, OpResult, Shape, SpawnSpec, UpdateSpec};
use specula_core::math::{Mat4, Quat, Vec3};
use specula_protocol::ToolDef;
use specula_scintilla::check::{self, Limits};
use specula_scintilla::effect::Effect;
use specula_scintilla::eval::{self, Instance, Placement, Track};
use specula_scintilla::{component, library, Params};
use std::sync::Arc;

/// Effects `vfx_get` lists per page.
const LIST_LIMITS: (usize, usize) = (40, 200);
/// Frames of a preview.
pub const MAX_PREVIEW_FRAMES: u32 = 16;
/// Particles one preview frame draws at most (the frame budget, `[vfx] max_particles`, applies
/// first: previews are bounded like window frames).
const PREVIEW_LIMIT: usize = 20_000;
/// Longest preview span (seconds).
const MAX_PREVIEW_S: f32 = 30.0;
/// Palette colours `vfx_check` takes.
const MAX_PALETTE: usize = 32;
/// Largest effect JSON `vfx_get` returns whole.
const MAX_EFFECT_BYTES: usize = 24 * 1024;

/// What a tool looks at: a library effect, an inline one, or an entity's.
pub enum Subject {
    Name(String),
    Inline(Box<Effect>),
    Entity(EntityKey),
}

pub enum VfxCall {
    Get { name: Option<String>, cursor: usize, limit: usize },
    Set { name: String, effect: Option<Box<Effect>>, expect_version: Option<u64> },
    Preview { subject: Subject, params: Option<Params>, frames: u32, duration: Option<f32>, start: Option<f32>, tile: u32, motion: [f32; 3] },
    Check { subject: Subject, params: Option<Params>, palette: Vec<[f32; 3]> },
    Info,
}

pub fn call_class(call: &VfxCall) -> schedule::Class {
    match call {
        VfxCall::Preview { .. } => schedule::Class::Render,
        VfxCall::Set { .. } => schedule::Class::Edit,
        VfxCall::Check { .. } | VfxCall::Get { .. } | VfxCall::Info => schedule::Class::Read,
    }
}

fn error(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

fn subject_arg(args: &Json) -> Result<Subject, String> {
    let given = ["effect", "definition", "entity"].iter().filter(|k| args.get(k).is_some_and(|v| !v.is_null())).count();
    if given != 1 {
        return Err("give exactly one of 'effect' (a library name), 'definition' (an inline effect) or 'entity' (id, \"/path\" or unique name)".into());
    }
    if let Some(n) = tools::string_arg(args, "effect")? {
        component::check_name(n, "effect").map_err(|e| crate::vfx::error_text(&e))?;
        return Ok(Subject::Name(n.to_owned()));
    }
    if let Some(d) = args.get("definition").filter(|v| !v.is_null()) {
        return Effect::parse(d).map(|e| Subject::Inline(Box::new(e))).map_err(|e| crate::vfx::error_text(&e));
    }
    let e = args.get("entity").expect("counted");
    Ok(Subject::Entity(match e {
        Json::Num(_) => EntityKey::Id(e.as_u64().ok_or("'entity' must be an id, a path or a unique name")?),
        Json::Str(s) if s.starts_with('/') => EntityKey::Path(s.clone()),
        Json::Str(s) => EntityKey::Name(s.clone()),
        _ => return Err("'entity' must be an id, a path or a unique name".into()),
    }))
}

fn params_arg(args: &Json) -> Result<Option<Params>, String> {
    match args.get("params").filter(|v| !v.is_null()) {
        None => Ok(None),
        Some(p) => Params::parse(p, &specula_scintilla::error::Path::root("params")).map(Some).map_err(|e| crate::vfx::error_text(&e)),
    }
}

pub fn parse(name: &str, args: &Json) -> Option<Result<VfxCall, String>> {
    let r = match name {
        "vfx_get" => (|| {
            tools::check_keys(args, name, &[&["name", "cursor", "limit"]])?;
            let n = tools::string_arg(args, "name")?.map(str::to_owned);
            let cursor = tools::int_arg(args, "cursor", 0, 1 << 20)?.unwrap_or(0) as usize;
            let limit = tools::int_arg(args, "limit", 1, LIST_LIMITS.1 as u64)?.map_or(LIST_LIMITS.0, |l| l as usize);
            Ok(VfxCall::Get { name: n, cursor, limit })
        })(),
        "vfx_set" => (|| {
            tools::check_keys(args, name, &[&["name", "effect", "expect_version"]])?;
            let n = tools::string_arg(args, "name")?.ok_or("'name' is required: the effect's name (what vfx components play)")?;
            component::check_name(n, "name").map_err(|e| crate::vfx::error_text(&e))?;
            let effect = match args.get("effect") {
                None => return Err("'effect' is required: the effect (see vfx_get for examples), or null to delete the world's effect".into()),
                Some(Json::Null) => None,
                Some(e) => Some(Box::new(Effect::parse(e).map_err(|err| crate::vfx::error_text(&err))?)),
            };
            let expect_version = tools::int_arg(args, "expect_version", 1, u64::MAX >> 11)?;
            Ok(VfxCall::Set { name: n.to_owned(), effect, expect_version })
        })(),
        "vfx_preview" => (|| {
            tools::check_keys(args, name, &[&["effect", "definition", "entity", "params", "frames", "duration", "start", "tile", "move"]])?;
            let subject = subject_arg(args)?;
            let frames = tools::int_arg(args, "frames", 2, u64::from(MAX_PREVIEW_FRAMES))?.unwrap_or(8) as u32;
            let duration = tools::num_arg(args, "duration")?;
            if duration.is_some_and(|d| !(0.0..=MAX_PREVIEW_S).contains(&d)) {
                return Err(format!("'duration' is 0 to {MAX_PREVIEW_S} seconds"));
            }
            let start = tools::num_arg(args, "start")?;
            if start.is_some_and(|s| !(0.0..=3600.0).contains(&s)) {
                return Err("'start' is 0 to 3600 seconds after the effect starts".into());
            }
            let tile = tools::int_arg(args, "tile", 96, 320)?.unwrap_or(192) as u32;
            let motion = tools::vec3_arg(args, "move")?.unwrap_or(Vec3::ZERO);
            if motion.length() > 50.0 {
                return Err("'move' is at most 50 m/s".into());
            }
            Ok(VfxCall::Preview { subject, params: params_arg(args)?, frames, duration, start, tile, motion: [motion.x, motion.y, motion.z] })
        })(),
        "vfx_check" => (|| {
            tools::check_keys(args, name, &[&["effect", "definition", "entity", "params", "palette"]])?;
            let subject = subject_arg(args)?;
            let mut palette = Vec::new();
            if let Some(p) = args.get("palette").filter(|v| !v.is_null()) {
                let list = p.as_array().filter(|l| l.len() <= MAX_PALETTE).ok_or(format!("'palette' must be a list of at most {MAX_PALETTE} \"#rrggbb\" colours"))?;
                for c in list {
                    palette.push(c.as_str().and_then(crate::world::color_from_hex).ok_or("'palette' colours are \"#rrggbb\"")?);
                }
            }
            Ok(VfxCall::Check { subject, params: params_arg(args)?, palette })
        })(),
        "vfx_info" => tools::check_keys(args, name, &[]).map(|()| VfxCall::Info),
        _ => return None,
    };
    Some(r)
}

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))])
    };
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let subject = || {
        vec![
            ("effect", t("string", "A library effect by name (built-in or the world's)")),
            ("definition", t("object", "An effect given inline, not yet in the library (the vfx_set form)")),
            ("entity", Json::obj([("description", Json::from("An entity with a vfx component: id, \"/path\" or a unique name (its effect and params)"))])),
        ]
    };
    let params = t(
        "object",
        "Overrides as on the vfx component: {rate, size, speed, alpha, intensity (multipliers), tint (sRGB), light (true/false), seed}",
    );
    vec![
        ToolDef {
            name: "vfx_get".into(),
            description: "The effect library: built-in effects (fire, embers, smoke, motes, trail, dust_puff, impact, sparks) and \
                          the world's own (vfx_set). Without 'name': a page of {name, source, emitters, peak particles, reach m, \
                          finished_after s (one-shots), light, description}. With 'name': the effect as JSON (the form vfx_set \
                          takes: copy a built-in, change it, save it under a new name). Effects are cosmetic: they never change \
                          the simulation."
                .into(),
            input_schema: schema(
                vec![
                    ("name", t("string", "One effect to return whole")),
                    ("cursor", t("integer", "next_cursor from the previous page")),
                    ("limit", t("integer", "Effects per page, 1..200 (default 40)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "vfx_set".into(),
            description: "Writes an effect into the world's library, validated (structured errors name the field and what is \
                          allowed): a Group entity /vfx/<name> tagged vfx_effect holding it in props.vfx_effect, so it is \
                          journaled, claim-aware, undoable and saved with the world; every vfx component naming it plays the new \
                          version at once (only /vfx's children are effects: a claim on /vfx covers the library; the tag is refused elsewhere, and an effect moved out of /vfx stops being one). Refused (photosensitive): an effect that would flash more than 3 times a second on its own (WCAG 2.3.1, one instance at 3 m: bursts, streams and its light summed; brightening, darkening, red), and a change that would make an instance already playing it flash with its params. The flash watch checks the final image (perf_frame, vfx_info). effect = {description?, emitters: [1..8 of {name?, rate (particles/s) and/or bursts \
                          [{time, count, cycles (0 = forever), interval}], delay, duration (s; none = loops), lifetime [min, max] \
                          s, shape (point | {kind: sphere radius surface | box size | disc radius | cone radius angle_deg | line \
                          length}), direction ([x,y,z] or \"radial\"), spread_deg, speed [min, max] m/s, gravity [x,y,z], drag \
                          (1/s), noise {strength, frequency, speed} (curl-like swirl), size (m, or curve [[u, m]...] over life u \
                          0..1), size_random [min, max], color (\"#rrggbb\" or [[u, colour]...], sRGB), alpha (curve), emissive \
                          (HDR x; above 1 blooms), rotation_deg [min, max], spin_deg [min, max], texture (soft, spark, smoke, \
                          flame, ring, rune, dust, or a .png under the assets folder), flipbook {cols, rows, fps | over_life: \
                          true, random_start, frames}, blend (\"additive\" | \"alpha\" | 0..1 mix), soft (m of depth fade), lit, \
                          stretch (s: sparks streak along velocity), space (\"local\" | \"world\": trails), inherit_velocity, \
                          collide {plane: m above the emitter | ground: true, mode: kill | slide | stick, radius}, lod {start, \
                          end, min}, priority, cull_px}], light?: {color, intensity, range, offset, flicker 0..1, flicker_hz \
                          (<= 3 at visible flicker: photosensitivity), duration (a flash)}}. effect: null deletes it. Built-in \
                          names can't be replaced. The answer includes vfx_check's failures."
                .into(),
            input_schema: schema(
                vec![
                    ("name", t("string", "The effect's name: 1..64 of A-Z a-z 0-9 _ - .")),
                    ("effect", Json::obj([("description", Json::from("The effect (see the description), or null to delete the world's"))])),
                    ("expect_version", t("integer", "Only if the effect's entity is still at this version")),
                ],
                &["name", "effect"],
            ),
        },
        ToolDef {
            name: "vfx_preview".into(),
            description: "Effect sight: a filmstrip of an effect over time on a neutral night stage (ground at y = 0, moonlight, \
                          bloom), auto-framed on everything it does, returned like sight captures (PNG plus a packet saved in \
                          the captures folder). Facts per frame: time, live and drawn particles, world bounds, screen box in the \
                          tile, estimated overdraw (covered pixels / tile pixels). One-shots span their whole life; looping \
                          effects start warmed up (after their longest lifetime) and span 'duration' (default 2 s). 'move' \
                          moves the emitter (m/s) to see trails. Particles are a function of time: the same call gives the same \
                          images. Frames are bounded like window frames ([vfx] max_particles and max_work; each frame reports its \
                          work). The packet carries vfx_check's failures."
                .into(),
            input_schema: schema(
                {
                    let mut p = subject();
                    p.extend([
                        ("params", params.clone()),
                        ("frames", t("integer", "Frames, 2..16 (default 8)")),
                        ("duration", t("number", "Seconds the frames span, 0..30")),
                        ("start", t("number", "Seconds after the effect starts of the first frame")),
                        ("tile", t("integer", "Tile size in pixels, 96..320 (default 192)")),
                        ("move", t("array", "Emitter velocity [x, y, z] m/s (trails; default still)")),
                    ]);
                    p
                },
                &[],
            ),
        },
        ToolDef {
            name: "vfx_check".into(),
            description: "Budget and readability checks of an effect, as data: peak_particles (an instance's most particles \
                          alive, against perf.toml [vfx] max_per_effect), overdraw (estimated covered screens at 8 m, against \
                          fill_cap), reach (m travelled), visible (each emitter's particles at least about a pixel), blowout \
                          (additive colour far above white loses its hue), trail_track (world-space lifetimes the track covers), \
                          flash_rate (at most 3 flashes a second, WCAG 2.3.1, for one instance at 3 m: bursts, streams and the \
                          light summed, each particle's curves and built-in flipbooks; brightening, darkening, red; several \
                          instances, scale and closer cameras are the flash watch's), flash_unchecked (asset flipbooks), and with a \
                          palette, palette (every colour, tinted, near one of the palette's: a style family). Errors are budget \
                          or safety failures; warnings are for a look with vfx_preview."
                .into(),
            input_schema: schema(
                {
                    let mut p = subject();
                    p.extend([("params", params), ("palette", t("array", "The style family's colours, \"#rrggbb\" (at most 32)"))]);
                    p
                },
                &[],
            ),
        },
        ToolDef {
            name: "vfx_info".into(),
            description: "What effects cost now: on/off (--vfx), the [vfx] budgets for this GPU, the library (built-ins, the \
                          world's effects and problems with them), entities with vfx and decal components, and the last window \
                          frame's and the last capture's counts: instances drawn, culled and finished, emitters, particles \
                          (expected, live, drawn, thinned by LOD/budget/fill cap, dropped, killed by collision, offscreen, \
                          fill_thinned), CPU work (expected, done, over [vfx] max_work), fill against the cap (after evaluation), \
                          decals drawn and skipped, lights added, CPU ms, problems. GPU time per pass is in \
                          perf_frame (decals, particles)."
                .into(),
            input_schema: schema(vec![], &[]),
        },
    ]
}

fn round(x: f64, places: i32) -> Json {
    let k = 10f64.powi(places);
    Json::from((x * k).round() / k)
}

fn v3(v: Vec3) -> Json {
    Json::arr([round(v.x as f64, 3), round(v.y as f64, 3), round(v.z as f64, 3)])
}

/// A library listing line.
fn summary(name: &str, e: &Effect, source: &str) -> Json {
    let mut f = vec![
        ("name", Json::from(name)),
        ("source", Json::from(source)),
        ("emitters", Json::from(e.emitters.len())),
        ("peak", Json::from(e.peak().round())),
        ("reach_m", round(e.reach() as f64, 2)),
    ];
    if let Some(end) = e.finished_after() {
        f.push(("finished_after", round(end as f64, 3)));
    }
    if e.light.is_some() {
        f.push(("light", Json::from(true)));
    }
    if e.world_space() {
        f.push(("world_space", Json::from(true)));
    }
    if !e.description.is_empty() {
        f.push(("description", Json::from(e.description.as_str())));
    }
    Json::obj(f)
}

/// The night stage previews render on (STYLE.md's moonlight, brightened for small tiles).
fn stage() -> specula_gpu::Environment {
    let rgb = |r: u8, g: u8, b: u8| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
    let mut env = specula_gpu::Environment {
        sun_direction: [0.45, 0.55, 0.7],
        sun_color: rgb(0xc9, 0xd6, 0xff),
        sun_intensity: 1.2,
        ambient_color: rgb(0x2a, 0x34, 0x48),
        ambient_intensity: 2.2,
        exposure: 1.1,
        tonemap: true,
        ..specula_gpu::Environment::default()
    };
    env.bloom.enabled = true;
    env.bloom.threshold = 1.2;
    env.bloom.intensity = 0.6;
    env
}

impl Engine {
    /// Builds the frame's effects (and effect lights) for `view` at `t`.
    pub(crate) fn build_effects(&mut self, view: &View, t: f64, include_ui: bool, lights: &mut Vec<specula_gpu::PointLight>, window: bool) {
        let _z = profile::zone("vfx");
        let ground = self.models.ground().cloned();
        self.vfx.build(&mut self.gpu, &self.world, self.world_generation, ground.as_deref(), view, t, include_ui, lights, window);
    }

    /// Effect and decal textures still loading (a capture waits for them, as for sprites).
    /// `fresh`: the capture's first check (looks for changed asset PNGs now).
    pub(crate) fn vfx_textures_waiting(&mut self, fresh: bool) -> usize {
        if !self.vfx.enabled {
            return 0;
        }
        self.vfx.library.refresh(&self.world, self.world_generation);
        self.vfx.request_textures(&mut self.gpu, &self.world, fresh);
        self.vfx.textures.pending()
    }

    pub(crate) fn vfx_call(&mut self, call: VfxCall, w: Writer, tick: u64) -> ToolOutput {
        let _z = profile::zone("vfx_tool");
        self.vfx.library.refresh(&self.world, self.world_generation);
        match call {
            VfxCall::Get { name, cursor, limit } => self.vfx_get(name.as_deref(), cursor, limit),
            VfxCall::Set { name, effect, expect_version } => self.vfx_set(&name, effect, expect_version, w, tick),
            // Queued by `Engine::handle` (`vfx_preview_start`): rendered within the studio's budget.
            VfxCall::Preview { .. } => error("internal", "vfx_preview is answered through the studio's render queue"),
            VfxCall::Check { subject, params, palette } => self.vfx_check(subject, params, &palette),
            VfxCall::Info => self.vfx_info(),
        }
    }

    fn vfx_get(&self, name: Option<&str>, cursor: usize, limit: usize) -> ToolOutput {
        let lib = &self.vfx.library;
        if let Some(n) = name {
            let (effect, mut out) = match (library::builtin(n), lib.effects.get(n)) {
                (Some(e), _) => (e, Json::obj([("name", Json::from(n)), ("source", Json::from("builtin"))])),
                (None, Some(w)) => (&w.effect, Json::obj([("name", Json::from(n)), ("source", Json::from("world")), ("path", Json::from(w.path.as_str())), ("id", Json::from(w.id)), ("version", Json::from(w.version))])),
                (None, None) => {
                    let names: Vec<&str> = lib.names().take(64).collect();
                    return error("unknown_effect", format!("no effect named '{n}' (there are: {})", names.join(", ")));
                }
            };
            let json = effect.to_json();
            if json.to_string().len() > MAX_EFFECT_BYTES {
                out.set("effect_omitted", Json::from("larger than 24 KiB; see the entity's props.vfx_effect"));
            } else {
                out.set("effect", json);
            }
            out.set("summary", summary(n, effect, ""));
            return ToolOutput::text(out.to_string());
        }
        let all: Vec<(&str, &Arc<Effect>, &str)> = library::builtins()
            .iter()
            .map(|(n, e)| (*n, e, "builtin"))
            .chain(lib.effects.iter().map(|(n, w)| (n.as_str(), &w.effect, "world")))
            .collect();
        let page: Vec<Json> = all.iter().skip(cursor).take(limit).map(|(n, e, s)| summary(n, e, s)).collect();
        let next = (cursor + page.len() < all.len()).then_some(cursor + page.len());
        let mut out = Json::obj([("total", Json::from(all.len())), ("effects", Json::Arr(page)), ("next_cursor", next.map_or(Json::Null, Json::from))]);
        if !lib.problems.is_empty() {
            out.set("problems", Json::arr(lib.problems.iter().take(16).map(|p| Json::from(p.as_str()))));
        }
        ToolOutput::text(out.to_string())
    }

    fn vfx_set(&mut self, name: &str, effect: Option<Box<Effect>>, expect_version: Option<u64>, w: Writer, tick: u64) -> ToolOutput {
        if library::builtin(name).is_some() {
            return error("builtin_effect", format!("'{name}' is a built-in effect: save your version under another name (vfx_get '{name}' to copy it)"));
        }
        let path = format!("/{LIBRARY_GROUP}/{name}");
        let existing = self.world.find_path(&path).ok();
        let mut ops = Vec::new();
        let mut checks_json = Json::Null;
        match &effect {
            Some(e) => {
                if self.world.find_path(&format!("/{LIBRARY_GROUP}")).is_err() {
                    let mut g = SpawnSpec::new(Shape::Group);
                    g.name = Some(LIBRARY_GROUP.into());
                    ops.push(Op::Spawn(g));
                }
                let prop = Json::obj([(EFFECT_PROP, e.to_json())]);
                match existing {
                    Some(id) => ops.push(Op::Set(EntityKey::Id(id), UpdateSpec { props: Some(prop), add_tags: vec![EFFECT_TAG.into()], expect_version, ..UpdateSpec::default() })),
                    None => {
                        if expect_version.is_some() {
                            return error("version_conflict", format!("{path} doesn't exist yet, so it has no version to expect"));
                        }
                        let mut s = SpawnSpec::new(Shape::Group);
                        s.name = Some(name.to_owned());
                        s.parent = Some(EntityKey::Path(format!("/{LIBRARY_GROUP}")));
                        s.tags.insert(EFFECT_TAG.into());
                        s.props = prop;
                        ops.push(Op::Spawn(s));
                    }
                }
                let limits = self.check_limits();
                let failed: Vec<Json> = check::check(e, &Params::default(), &limits, None).iter().filter(|c| !c.ok).map(check::Check::to_json).collect();
                checks_json = Json::Arr(failed);
            }
            None => match existing {
                Some(id) => ops.push(Op::Delete(EntityKey::Id(id), expect_version)),
                None => return error("not_found", format!("no world effect '{name}' at {path}")),
            },
        }
        let by = w.name.to_string();
        // A policy commit under the library's own rule: claims on /vfx hold, and the effect rules
        // (crate::vfx::write_rules) refuse a change that would make an instance playing it flash.
        let result = self.world.commit(&mut self.swarm, tick, w, Kind::Batch, crate::write_policy::Rule::Vfx, |wd| wd.apply_batch(ops, &by).map_err(|(_, e)| e));
        let (results, seq) = match result {
            Ok(r) => r,
            Err(mut e) => {
                if e.message.contains("props") {
                    e.message.push_str(" (an effect lives in props, at most 16 KiB as JSON)");
                }
                // Conflict help, as world_set gives: base, theirs, yours, overlap.
                let yours = crate::conflict::Yours::named(vec![if effect.is_some() { format!("props.{EFFECT_PROP}") } else { "(delete)".into() }]);
                return world_error(&self.conflict_help(e, Some(&yours)));
            }
        };
        self.vfx.library.refresh(&self.world, self.world_generation);
        let mut out = Json::obj([("name", Json::from(name)), ("path", Json::from(path.as_str())), ("journal", Json::from(seq))]);
        match results.last() {
            Some(OpResult::Spawned { id, .. }) => {
                out.set("id", Json::from(*id));
                out.set("version", Json::from(self.world.get(*id).map_or(0, |e| e.version)));
            }
            Some(OpResult::Updated { id, version, .. }) => {
                out.set("id", Json::from(*id));
                out.set("version", Json::from(*version));
            }
            _ => {
                out.set("deleted", Json::from(true));
            }
        }
        if let Some(e) = &effect {
            out.set("summary", summary(name, e, "world"));
            out.set("checks_failed", checks_json);
            let users = self.world.fx_entities().filter(|x| x.vfx.as_ref().is_some_and(|v| v.effect == name)).count();
            out.set("playing_on", Json::from(users));
        }
        ToolOutput::text(out.to_string())
    }

    fn check_limits(&self) -> Limits {
        let b = self.vfx.budget;
        Limits { max_per_effect: b.max_per_effect, fill_cap: b.fill_cap, tick_hz: self.clock.hz(), ..Limits::default() }
    }

    /// The effect and params a subject names.
    fn subject(&self, subject: Subject, params: Option<Params>) -> Result<(Arc<Effect>, Params, String), ToolOutput> {
        match subject {
            Subject::Name(n) => match self.vfx.library.get(&n) {
                Some(e) => Ok((Arc::clone(e), params.unwrap_or_default(), n)),
                None => {
                    let names: Vec<&str> = self.vfx.library.names().take(64).collect();
                    Err(error("unknown_effect", format!("no effect named '{n}' (there are: {})", names.join(", "))))
                }
            },
            Subject::Inline(e) => Ok((Arc::new(*e), params.unwrap_or_default(), "(definition)".into())),
            Subject::Entity(key) => {
                let id = self.world.resolve(&key).map_err(|e| world_error(&e))?;
                let ent = self.world.get(id).expect("resolved");
                let Some(spec) = &ent.vfx else {
                    return Err(error("no_vfx", format!("{} has no vfx component", self.world.path_of(id))));
                };
                match self.vfx.library.get(&spec.effect) {
                    Some(e) => Ok((Arc::clone(e), params.unwrap_or(spec.params), spec.effect.clone())),
                    None => Err(error("unknown_effect", format!("{} plays '{}', which isn't in the library", self.world.path_of(id), spec.effect))),
                }
            }
        }
    }

    fn vfx_check(&self, subject: Subject, params: Option<Params>, palette: &[[f32; 3]]) -> ToolOutput {
        let (effect, params, name) = match self.subject(subject, params) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let checks = check::check(&effect, &params, &self.check_limits(), (!palette.is_empty()).then_some(palette));
        let errors = checks.iter().filter(|c| !c.ok && c.severity == "error").count();
        let warnings = checks.iter().filter(|c| !c.ok && c.severity == "warning").count();
        // Failures first; passing checks compactly.
        let failed: Vec<Json> = checks.iter().filter(|c| !c.ok).map(check::Check::to_json).collect();
        let passed: Vec<Json> = checks.iter().filter(|c| c.ok).map(|c| Json::obj([("check", Json::from(c.check)), ("value", round(c.value, 3)), ("limit", round(c.limit, 3))])).collect();
        let out = Json::obj([
            ("effect", Json::from(name.as_str())),
            ("ok", Json::from(errors == 0)),
            ("errors", Json::from(errors)),
            ("warnings", Json::from(warnings)),
            ("failed", Json::Arr(failed)),
            ("passed", Json::Arr(passed)),
            ("summary", summary(&name, &effect, "")),
        ]);
        ToolOutput::text(out.to_string())
    }

    fn vfx_info(&self) -> ToolOutput {
        let (mut vfx, mut decals) = (0usize, 0usize);
        for e in self.world.fx_entities() {
            vfx += usize::from(e.vfx.is_some());
            decals += usize::from(e.decal.is_some());
        }
        let lib = &self.vfx.library;
        let out = Json::obj([
            ("enabled", Json::from(self.vfx.enabled)),
            ("budget", self.vfx.budget.to_json()),
            ("library", Json::obj([
                ("builtin", Json::from(library::BUILTIN.len())),
                ("world", Json::from(lib.effects.len())),
                ("problems", Json::arr(lib.problems.iter().take(16).map(|p| Json::from(p.as_str())))),
            ])),
            ("entities", Json::obj([("vfx", Json::from(vfx)), ("decal", Json::from(decals))])),
            ("textures", Json::obj([("loaded", Json::from(self.vfx.textures.loaded())), ("loading", Json::from(self.vfx.textures.pending()))])),
            ("window", self.vfx.window.to_json()),
            ("capture", self.vfx.capture.to_json()),
            ("flash", self.flash.to_json(self.gpu.last_frame_stats_of(specula_gpu::FramePath::Window).and_then(|w| w.flash.unsupported))),
        ]);
        ToolOutput::text(out.to_string())
    }

    /// `vfx_preview`: resolves the effect and queues the filmstrip; it is evaluated and rendered
    /// a frame per step within the studio's render budget (`Engine::pump_studio`), and the answer
    /// goes to `reply` when the last frame is done.
    /// `from_playtest`: a playtest's step (it keeps running after its starter's session closes,
    /// so the session's liveness doesn't drop it; the deadline still does).
    pub(crate) fn vfx_preview_start(&mut self, call: VfxCall, session: &SessionInfo, reply: crate::jobs::Reply, deadline: Instant, from_playtest: bool) {
        let VfxCall::Preview { subject, params, frames, duration, start, tile, motion } = call else {
            let _ = reply.send(error("invalid_argument", "not a vfx_preview"));
            return;
        };
        self.vfx.library.refresh(&self.world, self.world_generation);
        // Each preview is its caller's job (identity key, which a resume keeps): counted per agent,
        // and a refusal names whose previews fill the queue.
        let owner = crate::jobs::JobOwner { name: self.swarm.agent_name(session), key: self.swarm.key(session) };
        let queued = |o: &crate::jobs::JobOwner| self.studio.previews.iter().filter(|j| j.owner.is(o)).count();
        // Sheets still encoding count too (each holds its pixels).
        let encoding = self.studio.encoders.load(std::sync::atomic::Ordering::Relaxed);
        if self.studio.previews.len() + encoding >= MAX_PREVIEWS_QUEUED || queued(&owner) >= MAX_PREVIEWS_PER_AGENT {
            let owners: std::collections::BTreeSet<&str> = self.studio.previews.iter().map(|j| j.owner.name.as_str()).collect();
            let e = crate::authority::refusal(
                crate::authority::Site::JobBusy,
                format!("{} previews are queued ({MAX_PREVIEWS_PER_AGENT} per agent, {MAX_PREVIEWS_QUEUED} in all; yours: {}): call again when one has answered", self.studio.previews.len(), queued(&owner)),
            )
            .with("queued_by", Json::arr(owners.into_iter().map(Json::from)))
            .with("retry_after_s", 1);
            let _ = reply.send(ToolOutput::error(e.to_json().to_string()));
            return;
        }
        let (effect, params, name) = match self.subject(subject, params) {
            Ok(s) => s,
            Err(e) => {
                let _ = reply.send(e);
                return;
            }
        };
        let one_shot = effect.finished_after();
        let start = start.unwrap_or(if one_shot.is_some() { 0.0 } else { effect.max_lifetime() }) as f64;
        let span = duration.unwrap_or_else(|| one_shot.map_or(2.0, |f| (f - start as f32).max(0.1))).clamp(0.0, MAX_PREVIEW_S) as f64;
        let n = frames.max(2) as usize;
        let times: Vec<f64> = (0..n).map(|i| start + span * i as f64 / (n - 1) as f64).collect();
        let seed = eval::seed_of(1, 0.0, params.seed);
        let job = PreviewJob {
            session: session.id.clone(),
            owner,
            from_playtest,
            reply,
            deadline,
            effect,
            params,
            name,
            seed,
            motion: Vec3::new(motion[0], motion[1], motion[2]),
            times,
            tile,
            probed: 0,
            union: None,
            stage: None,
            frame: crate::vfx::Frame::default(),
            out_frames: Vec::new(),
            problems: Vec::new(),
            failed: None,
        };
        self.studio.previews.push_back(Box::new(job));
    }

    /// One step of a queued preview: a frame evaluated (to find what to frame), one rendered into
    /// the sheet, the effect's checks, or the hand-off of the sheet to an encoder thread.
    pub(crate) fn vfx_preview_step(&mut self, job: &mut PreviewJob) -> PreviewStep {
        let _z = profile::zone("vfx_preview");
        let n = job.times.len();
        let tile = job.tile;
        if job.out_frames.len() == n {
            if job.failed.is_none() {
                let failed = check::check(&job.effect, &job.params, &self.check_limits(), None).iter().filter(|c| !c.ok).map(check::Check::to_json).collect();
                job.failed = Some(failed);
                return PreviewStep::More;
            }
            return self.vfx_preview_finish(job);
        }
        // A first pass without a camera finds what to frame.
        if job.probed < n {
            let t = job.times[job.probed];
            let track = job.track_at(t);
            let p = job.where_at(t);
            let probe_cam = crate::sight::Camera::new(Vec3::new(0.0, 2.0, 10.0), Vec3::ZERO, 40.0);
            let probe_view = View::of(&probe_cam, tile, tile);
            let inst = Instance { effect: &job.effect, params: job.params, seed: job.seed, start: 0.0, placement: Placement::at([p.x, p.y, p.z]), track: Some(&track), ground: None };
            let f = self.vfx.preview_frame(&mut self.gpu, &inst, t, &probe_view, PREVIEW_LIMIT, &mut job.frame);
            let b = f.bounds.unwrap_or((p - Vec3::new(0.2, 0.0, 0.2), p + Vec3::new(0.2, 0.4, 0.2)));
            job.union = Some(job.union.map_or(b, |(lo, hi)| (lo.min(b.0), hi.max(b.1))));
            job.probed += 1;
            return PreviewStep::More;
        }
        let union = job.union;
        let st = job.stage.get_or_insert_with(|| {
            let (lo, hi) = union.unwrap_or((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5)));
            // Include the ground under it, so a scale is always in view.
            let lo = lo.min(Vec3::new(lo.x, 0.0, lo.z));
            let center = (lo + hi) * 0.5;
            let radius = ((hi - lo).length() * 0.5).max(0.4);
            let fov = 40.0f32;
            let dist = radius / (fov.to_radians() * 0.5).sin() * 1.1;
            let dir = Vec3::new(0.5, 0.35, 0.8).normalize();
            let camera = crate::sight::Camera::new(center + dir * dist, center, fov);
            let cols = (n as f64).sqrt().ceil() as u32;
            let rows = (n as u32).div_ceil(cols);
            let mut sheet = vec![0u8; (cols * tile * rows * tile * 4) as usize];
            for px in sheet.chunks_exact_mut(4) {
                px.copy_from_slice(&[24, 26, 30, 255]);
            }
            PreviewStage { camera, center, radius, cols, rows, sheet }
        });
        let view = View::of(&st.camera, tile, tile);
        let (cols, sw) = (st.cols, st.cols * tile);
        let (center, radius) = (st.center, st.radius);
        let i = job.out_frames.len();
        let t = job.times[i];
        let track = job.track_at(t);
        let p = job.where_at(t);
        let ground = self.meshes.get(Shape::Quad);
        let slab = [0x5b as f32 / 255.0, 0x64 as f32 / 255.0, 0x70 as f32 / 255.0, 1.0];
        let draws: Vec<specula_gpu::DrawItem> = ground
            .map(|m| specula_gpu::DrawItem::new(m, Mat4::from_trs(Vec3::new(center.x, 0.0, center.z), Quat::IDENTITY, Vec3::new(radius * 8.0, 1.0, radius * 8.0)).to_cols_array(), slab))
            .into_iter()
            .collect();
        let (effect, params, seed) = (&job.effect, job.params, job.seed);
        let inst = Instance { effect, params, seed, start: 0.0, placement: Placement::at([p.x, p.y, p.z]), track: Some(&track), ground: None };
        let facts = self.vfx.preview_frame(&mut self.gpu, &inst, t, &view, PREVIEW_LIMIT, &mut job.frame);
        for m in facts.problems {
            if !job.problems.contains(&m) && job.problems.len() < 8 {
                job.problems.push(m);
            }
        }
        let mut lights = Vec::new();
        if let (Some(l), true) = (&effect.light, params.light) {
            let level = l.level(t, seed);
            if level > 0.0 {
                let tint = params.tint;
                lights.push(specula_gpu::PointLight {
                    position: [p.x + l.offset[0], p.y + l.offset[1], p.z + l.offset[2]],
                    color: [l.color[0] * tint[0], l.color[1] * tint[1], l.color[2] * tint[2]],
                    intensity: l.intensity * level * params.intensity,
                    range: l.range,
                });
            }
        }
        let target = match self.offscreen_target(tile, tile) {
            Ok(t) => t,
            Err(e) => return PreviewStep::Done(ToolOutput::error(e)),
        };
        let srgb = |r: u8, g: u8, b: u8| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0];
        let desc = FrameDesc {
            view_proj: view.view_proj.to_cols_array(),
            clear_color: srgb(0x0b, 0x10, 0x20),
            draws: &draws,
            reverse_z: true,
            environment: stage(),
            lights: &lights,
            time_s: t as f32,
            effects: job.frame.effects(),
            ..Default::default()
        };
        if let Err(e) = self.gpu.render(target, &desc) {
            return PreviewStep::Done(ToolOutput::error(capture::gpu_failure("render", &e, self.windowed)));
        }
        let px = match self.gpu.read_rgba8(target) {
            Ok(p) => p,
            Err(e) => return PreviewStep::Done(ToolOutput::error(capture::gpu_failure("readback", &e, self.windowed))),
        };
        let stage_ = job.stage.as_mut().expect("made above");
        let (x0, y0) = ((i as u32 % cols) * tile, (i as u32 / cols) * tile);
        for row in 0..tile {
            let src = (row * tile * 4) as usize;
            let dst = (((y0 + row) * sw + x0) * 4) as usize;
            stage_.sheet[dst..dst + (tile * 4) as usize].copy_from_slice(&px[src..src + (tile * 4) as usize]);
        }
        let mut fj = Json::obj([
            ("frame", Json::from(i + 1)),
            ("t", round(t, 3)),
            ("live", Json::from(facts.stats.live)),
            ("drawn", Json::from(facts.stats.drawn)),
            ("work", Json::from(facts.stats.work)),
            ("overdraw", round(f64::from(facts.fill_px) / f64::from(tile * tile), 3)),
            ("tile", Json::arr([x0, y0, tile, tile].map(Json::from))),
        ]);
        if let Some((lo, hi)) = facts.bounds {
            fj.set("bounds", Json::obj([("min", v3(lo)), ("max", v3(hi))]));
            fj.set("screen", view.screen_rect(lo, hi).map_or(Json::Null, |r| Json::arr(r.map(Json::from))));
        }
        if facts.stats.dropped > 0 {
            fj.set("dropped", Json::from(facts.stats.dropped));
        }
        job.out_frames.push(fj);
        PreviewStep::More
    }

    /// A preview whose frames are rendered and checked: its capture number, paths and packet facts
    /// are settled here, and the sheet goes to an encoder thread, which writes the PNG and the
    /// packet and answers. (Review of the M4 fixes: encoding a 1280 px sheet, its base64 and the
    /// writes took about 36 ms of the engine thread in one step.)
    fn vfx_preview_finish(&mut self, job: &mut PreviewJob) -> PreviewStep {
        let Some(stage) = job.stage.take() else { return PreviewStep::Done(error("render_unavailable", "the preview has no frames")) };
        let (sw, sh) = (stage.cols * job.tile, stage.rows * job.tile);
        let mut warnings: Vec<Json> = std::mem::take(&mut job.problems).into_iter().map(Json::from).collect();
        for m in self.gpu.take_validation_messages() {
            if m.severity >= Severity::Warning {
                warnings.push(Json::from(format!("vulkan {:?} {}: {}", m.severity, m.id_name, m.message)));
                crate::trace_engine::gpu_message(m.severity == Severity::Error, &m.id_name, &m.message);
            }
        }
        self.frames_captured += 1;
        let png_path = self.capture_dir.join(format!("frame_{:05}.png", self.frames_captured));
        let m = job.motion;
        let packet = Json::obj([
            ("schema", Json::from(sight::SCHEMA)),
            ("kind", Json::from("vfx_preview")),
            ("frame", Json::from(self.frames_captured)),
            ("effect", Json::from(job.name.as_str())),
            ("params", job.params.to_json()),
            // (Filled in by the encoder.)
            ("image", Json::Null),
            ("camera", stage.camera.to_json()),
            ("view", Json::from("night stage: ground at y = 0, moonlight, bloom; the emitter starts at the origin")),
            ("move", Json::arr([m.x, m.y, m.z].map(Json::from))),
            ("frames", Json::Arr(std::mem::take(&mut job.out_frames))),
            ("checks_failed", Json::Arr(job.failed.take().unwrap_or_default())),
            ("warnings", Json::Null),
        ]);
        let encode = PreviewEncode { sheet: stage.sheet, width: sw, height: sh, grid: [stage.cols, stage.rows], png_path, warnings, packet };
        // On the worker pool (docs/design/WORKERS.md); with no slot free for its agent, here.
        let call = u64::from(!job.from_playtest);
        let bytes = encode.sheet.len() as u64;
        if self.workers_room(&job.owner, call, bytes).is_err() {
            return PreviewStep::Done(encode.finish());
        }
        // Gives the preview's slot back however the job ends: run, panicked, or dropped unrun
        // (a cancelled job's closure is dropped with the guard in it; review of 998af3d: a guard
        // made inside the closure never ran for a cancelled job, and four leaks disabled previews).
        struct Slot(Arc<std::sync::atomic::AtomicUsize>);
        impl Drop for Slot {
            fn drop(&mut self) {
                self.0.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        self.studio.encoders.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let slot = Slot(std::sync::Arc::clone(&self.studio.encoders));
        let (reply, theirs) = (job.reply.clone(), job.reply.clone());
        let work = move |_: &crate::workers::Cancel| {
            let _slot = slot;
            let _ = theirs.send(encode.finish());
        };
        let kind = crate::workers::JobKind::PreviewEncode;
        // A playtest's preview outlives its starter's session (call 0: never cancelled with it).
        let o = crate::workers::Offload::agent(kind, job.owner.clone(), &job.session, call).class(crate::schedule::Class::Render).bytes(bytes);
        let refused = job.reply.clone();
        match self.offload(o, work, move |_, r| {
            if let Err(f) = r {
                let _ = reply.send(f.to_output(kind));
            }
        }) {
            Ok(()) => PreviewStep::Handed,
            Err(e) => {
                refused.refuse(&e);
                PreviewStep::Handed
            }
        }
    }
}

/// What a preview step did.
pub(crate) enum PreviewStep {
    /// More steps to go.
    More,
    /// The answer (or a failure), for the caller.
    Done(ToolOutput),
    /// An encoder thread answers.
    Handed,
}

/// A finished preview's sheet and packet, encoded and written off the engine thread.
struct PreviewEncode {
    sheet: Vec<u8>,
    width: u32,
    height: u32,
    grid: [u32; 2],
    png_path: PathBuf,
    warnings: Vec<Json>,
    /// Every field but `image` and `warnings` (set once the files are written), in order.
    packet: Json,
}

impl PreviewEncode {
    /// Encodes the sheet, writes the PNG and its packet (paths only for what was written) and
    /// makes the answer.
    fn finish(self) -> ToolOutput {
        let PreviewEncode { sheet, width, height, grid, png_path, mut warnings, mut packet } = self;
        let png_bytes = png::encode_rgba8(width, height, &sheet, Level::Fastest);
        // Through the disk budget (refused below the free-space floor, like every capture).
        let written = crate::disk::book().write(crate::disk::Category::Captures, "vfx", &png_path, &png_bytes, crate::disk::Need::Agent);
        let packet_path = png_path.with_extension("json");
        let (path, packet_at) = match &written {
            Ok(()) => (Json::from(png_path.display().to_string()), Json::from(packet_path.display().to_string())),
            Err(e) => {
                warnings.push(Json::from(format!("capture not saved to disk: {e}")));
                (Json::Null, Json::Null)
            }
        };
        packet.set(
            "image",
            Json::obj([("width", Json::from(width)), ("height", Json::from(height)), ("grid", Json::arr(grid.map(Json::from))), ("path", path), ("packet_path", packet_at)]),
        );
        packet.set("warnings", Json::Arr(warnings));
        if written.is_ok() {
            if let Err(e) = crate::disk::book().write(crate::disk::Category::Captures, "vfx", &packet_path, packet.to_pretty().as_bytes(), crate::disk::Need::Agent) {
                packet.set("packet_error", Json::from(e.to_string()));
            }
        }
        ToolOutput { content: vec![Content::Image { mime_type: "image/png".into(), data_base64: base64::encode(&png_bytes) }, Content::Text(packet.to_string())], is_error: false }
    }
}

/// Queued previews at most, in all and per agent (each holds its sheet: up to 1280 x 1280 px).
pub(crate) const MAX_PREVIEWS_QUEUED: usize = 4;
pub(crate) const MAX_PREVIEWS_PER_AGENT: usize = 2;

/// A `vfx_preview` in progress: its frames are evaluated (to frame the effect), rendered a frame
/// per step and checked, each step within the asset studio's render budget, round-robin with the
/// studio's renders (`Engine::pump_studio`); the sheet is then encoded and written on a thread of
/// its own. A step starts while there is credit and is charged afterwards, so one step (a tile:
/// about 1-6 ms on the Intel iGPU) can overrun a frame's share, never the whole sheet. (M4 audit:
/// rendered in one go on the engine thread, 16 tiles of 320 px took Sepulcra's iGPU frame p95
/// from 8.5 to 43.5 ms; the review of the fix: encoding the sheet in one step still took 36 ms.)
pub(crate) struct PreviewJob {
    pub(crate) session: String,
    /// Who asked for it (counted per agent; named when the queue is full).
    pub(crate) owner: crate::jobs::JobOwner,
    /// A playtest's step: kept when the session closes (see [`Engine::vfx_preview_start`]).
    pub(crate) from_playtest: bool,
    pub(crate) reply: crate::jobs::Reply,
    pub(crate) deadline: Instant,
    effect: Arc<Effect>,
    params: Params,
    name: String,
    seed: u32,
    motion: Vec3,
    times: Vec<f64>,
    tile: u32,
    /// Frames evaluated so far, and the bounds they found.
    probed: usize,
    union: Option<(Vec3, Vec3)>,
    /// Made once every frame is evaluated.
    stage: Option<PreviewStage>,
    frame: crate::vfx::Frame,
    out_frames: Vec<Json>,
    problems: Vec<String>,
    /// The effect's failed checks, run as a step of their own once every frame is rendered.
    failed: Option<Vec<Json>>,
}

/// A preview's camera and sheet.
struct PreviewStage {
    camera: crate::sight::Camera,
    center: Vec3,
    radius: f32,
    cols: u32,
    rows: u32,
    sheet: Vec<u8>,
}

impl PreviewJob {
    fn where_at(&self, t: f64) -> Vec3 {
        self.motion * t as f32
    }

    /// The trail an emitter moving at `motion` left up to `t`.
    fn track_at(&self, t: f64) -> Track {
        let mut tr = Track::new();
        let hz = 30.0;
        let first = ((t - Track::CAPACITY as f64 / hz).max(0.0) * hz).ceil() as u64;
        let last = (t * hz).floor() as u64;
        for k in first..=last {
            let tk = k as f64 / hz;
            let p = self.where_at(tk);
            tr.push(tk, [p.x, p.y, p.z]);
        }
        tr
    }
}

/// The `effects` overlay: the capture's effect boxes (orange) and decal boxes (violet), labelled.
pub fn paint(vfx: &crate::vfx::Vfx, world: &World, p: &mut Painter) -> Json {
    const EFFECT: [f32; 4] = [1.0, 0.62, 0.2, 1.0];
    const DECAL: [f32; 4] = [0.72, 0.5, 1.0, 1.0];
    let corners = |lo: Vec3, hi: Vec3| -> [Vec3; 8] {
        std::array::from_fn(|i| Vec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z }))
    };
    let r = &vfx.capture;
    let mut boxes = 0;
    for s in r.shown.iter().take(crate::sight_overlay::MAX_BOXES) {
        let Some((lo, hi)) = s.bounds else { continue };
        if !p.sees(lo, hi) {
            continue;
        }
        p.box_edges(&corners(lo, hi), EFFECT, 1.5);
        if let Some(at) = p.project(Vec3::new((lo.x + hi.x) * 0.5, hi.y, (lo.z + hi.z) * 0.5)) {
            let name = world.get(s.entity).and_then(|e| e.vfx.as_ref()).map_or("?", |v| v.effect.as_str());
            p.label(&format!("{name} {}", s.drawn), at, EFFECT, u64::from(s.drawn));
        }
        boxes += 1;
    }
    let mut decal_boxes = 0;
    for d in r.shown_decals.iter().take(crate::sight_overlay::MAX_BOXES) {
        p.box_edges(&corners(d.lo, d.hi), DECAL, 1.0);
        decal_boxes += 1;
    }
    Json::obj([
        ("effects", Json::from(boxes)),
        ("decals", Json::from(decal_boxes)),
        ("legend", Json::obj([("effect", Json::from("#ff9e33")), ("decal", Json::from("#b880ff"))])),
    ])
}

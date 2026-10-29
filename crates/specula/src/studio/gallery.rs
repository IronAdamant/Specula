//! Gallery rounds rendered through the studio (docs/design/GALLERIES.md): each variant is one
//! render queued like the studio's own (round-robin with other agents' studio calls, within the
//! render budget), composed into one sheet with numbered tiles, measured, and saved with its
//! packet in the captures folder. Runs on the gallery worker's thread, never the engine's.
//!
//! Subjects: a recipe on the stage (one camera fitted to every variant, so proportions compare
//! at true scale, with the 1.8 m figure), the live world through a camera (read only), a `ui`
//! panel drawn on the CPU (over a colour or a scene render), or the stage itself.

use super::*;
use crate::sight::Camera;
use specula_gustus::gallery::Gallery;
use specula_gustus::presets::{self, Preset};
use specula_gustus::space::{Target, Value};

/// A gallery variant's render extras.
#[derive(Clone, Debug, Default)]
pub(crate) struct Extra {
    pub fog: Option<specula_gpu::FogSettings>,
    /// The live world as the subject, through this camera, with the environment patched.
    pub scene: Option<SceneLook>,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneLook {
    pub camera: Camera,
    pub patch: Patch,
    /// The studio call it belongs to: every tile of a call is drawn at the same moment, so
    /// flames, embers and trails don't make tiles differ (or alike-checks miss).
    pub call: u64,
}

/// What a variant changes in the light (each `None` keeps the base's).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Patch {
    pub preset: Option<&'static Preset>,
    pub key_color: Option<[f32; 3]>,
    pub key_intensity: Option<f32>,
    pub key_elevation: Option<f32>,
    pub key_azimuth: Option<f32>,
    pub ambient_color: Option<[f32; 3]>,
    pub ambient_intensity: Option<f32>,
    pub exposure: Option<f32>,
    pub fog_density: Option<f32>,
    pub fog_color: Option<[f32; 3]>,
    pub backdrop: Option<[f32; 3]>,
    pub ground: Option<[f32; 3]>,
}

/// Which light a stage subject starts from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Base {
    Neutral,
    Game,
    Preset(&'static Preset),
}

/// One line of a `ui` subject: its text and colour (a fixed one, or a parameter's `$name`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LineColour {
    Fixed([f64; 3]),
    Param(String),
}

#[derive(Clone, Debug)]
pub(crate) enum Subject {
    Recipe { spec: ModelSpec, az: f32, el: f32, references: bool, base: Base },
    Scene { camera: Camera, look: Option<&'static Preset> },
    Ui { panel: [f64; 3], background: [f64; 3], lines: Vec<(String, LineColour)>, camera: Option<Camera> },
    Stage { base: Base },
}

/// The largest sheet tile.
pub(crate) const MAX_TILE: u32 = 384;
pub(crate) const DEFAULT_TILE: u32 = 256;
/// Lines of a `ui` subject.
const MAX_LINES: usize = 6;

fn hex3(s: &str) -> Option<[f64; 3]> {
    specula_gustus::colour::parse_hex(s)
}

fn f32x3(c: [f64; 3]) -> [f32; 3] {
    c.map(|v| v as f32)
}

fn base_of(j: &Json) -> Result<Base, String> {
    if let Some(l) = j.get("look").and_then(Json::as_str) {
        return presets::find(l).map(Base::Preset).ok_or_else(|| format!("look '{l}' is not a base style ({})", presets::names().join(", ")));
    }
    match j.get("rig").and_then(Json::as_str).unwrap_or("game") {
        "neutral" => Ok(Base::Neutral),
        "game" => Ok(Base::Game),
        other => Err(format!("rig '{other}': neutral or game (or give a look: one of {})", presets::names().join(", "))),
    }
}

/// A camera stored in a subject (`{eye, target, fov_y_deg}`; named cameras are resolved to
/// these when the gallery is made).
pub(crate) fn camera_of(j: &Json) -> Option<Camera> {
    let v = |k: &str| -> Option<Vec3> {
        let a = j.get(k)?.as_array()?;
        Some(Vec3::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32))
    };
    let (eye, target) = (v("eye")?, v("target")?);
    ((eye - target).length() > 1e-3).then(|| Camera::new(eye, target, j.get("fov_y_deg").and_then(Json::as_f64).unwrap_or(45.0).clamp(5.0, 120.0) as f32))
}

pub(crate) fn camera_json(c: &Camera) -> Json {
    let v = |p: Vec3| Json::arr([r3(p.x), r3(p.y), r3(p.z)]);
    Json::obj([("eye", v(c.eye)), ("target", v(c.target)), ("fov_y_deg", r3(c.fov_y_deg))])
}

/// Parses a gallery subject (scene cameras already resolved).
pub(crate) fn parse_subject(j: &Json) -> Result<Subject, String> {
    if let Some(r) = j.get("recipe").and_then(Json::as_str) {
        let mut spec = ModelSpec::new(r);
        if let Some(p) = j.get("params").and_then(Json::as_object) {
            for (k, v) in p {
                spec.set_param(k, v.as_f64().ok_or_else(|| format!("params.{k} is a number"))?);
            }
        }
        if let Some(s) = j.get("seed").and_then(Json::as_u64) {
            spec.seed = Some(s);
        }
        let az = j.get("az").and_then(Json::as_f64).unwrap_or(35.0) as f32;
        let el = j.get("el").and_then(Json::as_f64).unwrap_or(18.0).clamp(-10.0, 89.0) as f32;
        return Ok(Subject::Recipe { spec, az, el, references: j.get("references").and_then(Json::as_bool).unwrap_or(true), base: base_of(j)? });
    }
    if let Some(s) = j.get("scene") {
        let look = match j.get("look").and_then(Json::as_str) {
            Some(l) => Some(presets::find(l).ok_or_else(|| format!("look '{l}' is not a base style ({})", presets::names().join(", ")))?),
            None => None,
        };
        return Ok(Subject::Scene { camera: camera_of(s).ok_or("scene needs a camera (a named camera, {eye, target, fov_y_deg}, or \"window\")")?, look });
    }
    if let Some(u) = j.get("ui") {
        let panel = u.get("panel").and_then(Json::as_str).map_or(Some([0.17, 0.19, 0.22]), hex3).ok_or("ui.panel is #rrggbb")?;
        let background = u.get("background").and_then(Json::as_str).map_or(Some([0.06, 0.07, 0.09]), hex3).ok_or("ui.background is #rrggbb")?;
        let lines = u.get("lines").and_then(Json::as_array).ok_or("ui.lines: [{text, color}] (color #rrggbb or $param)")?;
        if lines.is_empty() || lines.len() > MAX_LINES {
            return Err(format!("ui.lines: 1..={MAX_LINES} lines"));
        }
        let mut out = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            let text = l.get("text").and_then(Json::as_str).filter(|t| !t.is_empty() && t.chars().count() <= 40).ok_or(format!("ui.lines[{i}].text: 1-40 characters"))?;
            let c = l.get("color").and_then(Json::as_str).unwrap_or("#ece6d6");
            let colour = match c.strip_prefix('$') {
                Some(p) => LineColour::Param(p.to_owned()),
                None => LineColour::Fixed(hex3(c).ok_or(format!("ui.lines[{i}].color: #rrggbb or $param"))?),
            };
            out.push((text.to_owned(), colour));
        }
        let camera = match u.get("camera") {
            None | Some(Json::Null) => None,
            Some(c) => Some(camera_of(c).ok_or("ui.camera: a resolved camera {eye, target}")?),
        };
        return Ok(Subject::Ui { panel, background, lines: out, camera });
    }
    // (Keys the engine adds to a stored subject don't make it something else: a review probe's
    // `{look}` subject passed create and failed at render once `tile` was added.)
    if j.get("stage").is_some() || j.as_object().is_some_and(|o| o.iter().all(|(k, _)| matches!(k.as_str(), "look" | "rig" | "tile" | "start" | "onboarding_step"))) {
        return Ok(Subject::Stage { base: base_of(j)? });
    }
    Err("a subject is {recipe}, {scene: {camera}}, {ui: {panel, lines}} or {stage: {}}".into())
}

/// Checks a space against its subject: every target means something for it.
pub(crate) fn check_targets(subject: &Subject, space: &specula_gustus::space::Space) -> Result<(), String> {
    for p in &space.params {
        let ok = match (&p.target, subject) {
            (Target::Recipe(_) | Target::Scale(_), Subject::Recipe { .. }) => true,
            (Target::Recipe(_) | Target::Scale(_), _) => false,
            (Target::Ui(name), Subject::Ui { lines, .. }) => lines.iter().any(|(_, c)| *c == LineColour::Param(name.clone())),
            (Target::Ui(_), _) => false,
            (Target::Ground, Subject::Scene { .. }) => false,
            (_, Subject::Ui { camera: None, .. }) if !matches!(p.target, Target::Ui(_) | Target::Backdrop) => false,
            _ => true,
        };
        if !ok {
            return Err(format!("parameter '{}' targets {}, which this subject doesn't have (recipe.* and scale.* need a recipe; ui.<name> needs a ui line coloured $<name>; ground needs the stage)", p.name, p.target.name()));
        }
    }
    Ok(())
}

/// What a variant's values change.
/// A variant's light patch, scale, recipe parameters and ui colours.
type Look = (Patch, Vec3, Vec<(String, f64)>, BTreeMap<String, [f64; 3]>);

fn variant_look(space: &specula_gustus::space::Space, values: &[Value]) -> Look {
    let mut patch = Patch::default();
    let mut scale = Vec3::ONE;
    let mut params = Vec::new();
    let mut ui = BTreeMap::new();
    for (p, v) in space.params.iter().zip(values) {
        let n = v.as_num().map(|x| x as f32);
        let c = v.as_colour();
        match &p.target {
            Target::KeyColor => patch.key_color = c.map(f32x3),
            Target::KeyIntensity => patch.key_intensity = n,
            Target::KeyElevation => patch.key_elevation = n,
            Target::KeyAzimuth => patch.key_azimuth = n,
            Target::AmbientColor => patch.ambient_color = c.map(f32x3),
            Target::AmbientIntensity => patch.ambient_intensity = n,
            Target::Exposure => patch.exposure = n,
            Target::FogDensity => patch.fog_density = n,
            Target::FogColor => patch.fog_color = c.map(f32x3),
            Target::Backdrop => patch.backdrop = c.map(f32x3),
            Target::Ground => patch.ground = c.map(f32x3),
            Target::Scale(a) => {
                let s = n.unwrap_or(1.0).clamp(0.05, 20.0);
                match a {
                    0 => scale.x = s,
                    1 => scale.y = s,
                    2 => scale.z = s,
                    _ => scale = Vec3::new(scale.x * s, scale.y * s, scale.z * s),
                }
            }
            Target::Recipe(name) => params.push((name.clone(), v.as_num().unwrap_or(0.0))),
            Target::Ui(name) => {
                if let Some(c) = c {
                    ui.insert(name.clone(), c);
                }
            }
            Target::Preset => {
                if let Value::Choice(_, s) = v {
                    patch.preset = presets::find(s);
                }
            }
        }
    }
    (patch, scale, params, ui)
}

/// A preset's light as a studio rig.
fn preset_rig(p: &Preset) -> Rig {
    let h = |s: &str| f32x3(hex3(s).unwrap_or([0.5; 3]));
    Rig {
        key_color: h(p.key_color),
        key_intensity: p.key_intensity as f32,
        key_elevation_deg: p.key_elevation_deg as f32,
        key_azimuth_deg: p.key_azimuth_deg as f32,
        ambient_color: h(p.ambient_color),
        ambient_intensity: p.ambient_intensity as f32,
        exposure: p.exposure as f32,
        bloom: p.bloom,
        shadows: p.shadows,
        background: h(p.background),
        ground: h(p.ground),
    }
}

fn fog_of(density: f32, colour: [f32; 3]) -> Option<specula_gpu::FogSettings> {
    (density > 0.0).then_some(specula_gpu::FogSettings { enabled: true, color: colour, density, falloff: 0.08, height: 0.0, start_distance: 2.0, sun_scatter: 0.3 })
}

/// The rig and fog of a stage render: the base, its preset if the variant picks one, then the
/// variant's values.
fn stage_light(base: Base, game: &Rig, patch: &Patch) -> (Rig, Option<specula_gpu::FogSettings>) {
    let preset = patch.preset.or(match base {
        Base::Preset(p) => Some(p),
        _ => None,
    });
    let mut rig = match (preset, base) {
        (Some(p), _) => preset_rig(p),
        (None, Base::Game) => game.clone(),
        _ => neutral_rig(),
    };
    let mut fog = preset.map(|p| (p.fog_density as f32, f32x3(hex3(p.fog_color).unwrap_or([0.2; 3]))));
    if let Some(c) = patch.key_color {
        rig.key_color = c;
    }
    if let Some(v) = patch.key_intensity {
        rig.key_intensity = v;
    }
    if let Some(v) = patch.key_elevation {
        rig.key_elevation_deg = v;
    }
    if let Some(v) = patch.key_azimuth {
        rig.key_azimuth_deg = v;
    }
    if let Some(c) = patch.ambient_color {
        rig.ambient_color = c;
    }
    if let Some(v) = patch.ambient_intensity {
        rig.ambient_intensity = v;
    }
    if let Some(v) = patch.exposure {
        rig.exposure = v;
    }
    if let Some(c) = patch.backdrop {
        rig.background = c;
    }
    if let Some(c) = patch.ground {
        rig.ground = c;
    }
    if patch.fog_density.is_some() || patch.fog_color.is_some() {
        let (d, c) = fog.unwrap_or((0.0, rig.background));
        fog = Some((patch.fog_density.unwrap_or(d), patch.fog_color.unwrap_or(c)));
    }
    (rig, fog.and_then(|(d, c)| fog_of(d, c)))
}

/// One round to render.
pub(crate) struct RoundJob {
    pub gallery: Gallery,
    /// The round's number and its points.
    pub n: usize,
    pub points: Vec<Vec<f64>>,
    pub incumbent: Option<usize>,
    pub tile: u32,
    /// Whether tiles that render alike may be replaced (the engine planned the round, and no
    /// chips steer it).
    pub replace_alike: bool,
    /// Render exactly these points (a sheet made again): no palette rules, nothing replaced.
    pub exact: bool,
}

/// A rendered round.
pub(crate) struct RoundOut {
    /// The sheet's file name in the captures folder, and its full path.
    pub sheet: String,
    pub path: PathBuf,
    pub packet: PathBuf,
    pub measures: Vec<Option<Json>>,
    /// Each tile's box on the sheet (x, y, w, h).
    pub tiles: Vec<[u32; 4]>,
    /// The round's points as rendered (a colour the game's palette rules forbid is pulled back
    /// towards its base first: [`palette_guard`]).
    pub points: Vec<Vec<f64>>,
    pub size: (u32, u32),
    /// What the renders cost: count, the most and the total engine-thread time, wall time.
    pub cost: Json,
    /// Tiles that render alike although their values differ ([`rendered_alike`]).
    pub warning: Option<Json>,
    /// The incumbent's index among the rendered points (guarded look-alikes may be dropped).
    pub incumbent: Option<usize>,
    /// How many variants fit the style rules, when fewer than planned ([`settle`]).
    pub fit: Option<usize>,
}

/// The gallery worker's session id: its renders queue under it (no MCP session has it).
pub(crate) const WORKER_SESSION: &str = "gallery-worker";

/// The gallery worker's session (studio admission and round-robin).
pub(crate) fn worker_session() -> SessionInfo {
    SessionInfo { id: WORKER_SESSION.into(), client_name: "specula".into(), client_version: env!("CARGO_PKG_VERSION").into(), protocol_version: String::new() }
}

/// The sheet's file name for a round.
pub(crate) fn sheet_name(id: &str, n: usize) -> String {
    format!("gallery-{id}-r{n}.png")
}

/// Renders one round: admission (retried while the studio is busy, within the call limit),
/// one render per variant, the sheet, its measures and packet.
pub(crate) fn render_round(tools: &crate::tools::Tools, shared: &Shared, session: &SessionInfo, job: &RoundJob) -> Result<RoundOut, Json> {
    let started = Instant::now();
    let subject = parse_subject(&job.gallery.subject).map_err(|m| Json::obj([("error", Json::from("bad_subject")), ("message", Json::from(m))]))?;
    let renders = match &subject {
        Subject::Ui { camera: None, .. } => 0,
        Subject::Ui { .. } => 1,
        _ => job.points.len(),
    };
    let id = loop {
        match shared.admit(&session.id, renders) {
            Ok(id) => break id,
            Err(e) => {
                if started.elapsed() > CALL_LIMIT {
                    return Err(Json::parse(&text_of(&e)).unwrap_or_else(|_| Json::obj([("error", Json::from("studio_busy"))])));
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    };
    let mut ctx = Ctx { shared, tools, session, call: id, deadline: Instant::now() + CALL_LIMIT, renders: 0, render_scale: 1.0, timing: (0, RenderCost::default()) };
    let tile = job.tile.clamp(96, MAX_TILE);
    let (tw, th) = match subject {
        Subject::Scene { .. } | Subject::Ui { .. } => (tile, (tile * 9 / 16).max(64)),
        _ => (tile, tile),
    };
    let root = shared.library.lock().root().to_path_buf();
    let render_err = |e: ToolOutput| Json::parse(&text_of(&e)).unwrap_or_else(|_| Json::obj([("error", Json::from("render_failed")), ("message", Json::from(text_of(&e)))]));
    // A sheet made again shows exactly what the round holds (taste review 2: a re-render after
    // style.toml changed guarded the colours anew, so its packet no longer matched the round).
    let Settled { mut points, notes: mut palette_notes, incumbent, dropped, refilled } = if job.exact {
        Settled { points: job.points.clone(), notes: Vec::new(), incumbent: job.incumbent, dropped: Vec::new(), refilled: 0 }
    } else {
        settle(&root, &job.gallery, &job.points, job.incumbent, job.replace_alike, job.n)
    };
    let mut values: Vec<Vec<Value>> = points.iter().map(|p| job.gallery.space.values(p)).collect();
    // Fewer than planned: the owner is told how many fit, not left to wonder.
    let fit = (!dropped.is_empty() && points.len() < job.points.len()).then_some(points.len());
    let mut tiles = ctx.gallery_tiles(&subject, &job.gallery, &values, tw, th).map_err(render_err)?;
    // Tiles whose values differ but whose renders don't (a sight-skeptic finding: near-white
    // moonlights at full strength, tone-mapped alike) waste the owner's look: once, the later of
    // each such pair is replaced by a point far from everything shown and rendered again. Not for
    // an agent's own variants, a breadcrumb's, or a round steered by chips (whose side a fresh
    // point might leave).
    let mut replaced = Vec::new();
    if job.replace_alike {
        for [i, j] in rendered_alike(&tiles) {
            if replaced.contains(&i) || replaced.contains(&j) {
                continue;
            }
            let k = if incumbent == Some(j) { i } else { j };
            if incumbent != Some(k) {
                replaced.push(k);
            }
        }
    }
    if !replaced.is_empty() {
        let mut rng = specula_core::rng::Rng::new(job.gallery.seed ^ 0x616c_696b_6500, job.n as u64);
        let region = specula_gustus::spread::Region { centre: None, radius: 1.0 };
        let fresh = specula_gustus::spread::dispersed(replaced.len(), job.gallery.space.dims(), &[], &points, &region, &mut rng);
        let (fresh, notes) = palette_guard(&root, &job.gallery, &fresh);
        let fresh_values: Vec<Vec<Value>> = fresh.iter().map(|p| job.gallery.space.values(p)).collect();
        // Admitted like the round's own renders (taste review 2: they weren't counted).
        if !matches!(subject, Subject::Ui { .. }) {
            shared.more_renders(id, fresh.len());
        }
        let fresh_tiles = ctx.gallery_tiles(&subject, &job.gallery, &fresh_values, tw, th).map_err(render_err)?;
        for (((&k, p), v), t) in replaced.iter().zip(fresh).zip(fresh_values).zip(fresh_tiles) {
            (points[k], values[k], tiles[k]) = (p, v, t);
        }
        palette_notes.retain(|n| n.get("variant").and_then(Json::as_u64).is_none_or(|v| !replaced.contains(&(v as usize))));
        for mut n in notes {
            if let Some(k) = n.get("variant").and_then(Json::as_u64).and_then(|v| replaced.get(v as usize)) {
                n.set("variant", Json::from(*k));
                palette_notes.push(n);
            }
        }
    }
    // The sheet: tiles in a grid, numbered, the incumbent marked.
    let n = tiles.len() as u32;
    let (cols, rows) = sheet_grid(n);
    // The header: the gallery's title in the UI font (the bitmap font was 6 px tall at window
    // scale and showed the gallery's id, not its question: a sight-skeptic finding).
    let (gap, head) = (6u32, if ui_font().is_some() { 26 } else { film::CHAR_H + 8 });
    let (w, h) = (cols * tw + (cols + 1) * gap, head + rows * th + (rows + 1) * gap);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    film::fill(&mut rgba, w, 0, 0, w, h, [14, 17, 26, 255]);
    label(&mut rgba, w, gap, 2, &format!("{}, round {}", specula_gustus::cut(&job.gallery.title, 90), job.n), 15.0, [0.8, 0.8, 0.75]);
    let mut boxes = Vec::new();
    let mut measures = Vec::new();
    for (i, px) in tiles.iter().enumerate() {
        let (c, r) = (i as u32 % cols, i as u32 / cols);
        let (x0, y0) = (gap + c * (tw + gap), head + gap + r * (th + gap));
        film::blit(&mut rgba, w, x0, y0, px, tw, th);
        // Numbers in the UI font, large enough to read at window scale (the bitmap font's 8
        // read as a 0 there).
        label(&mut rgba, w, x0 + 4, y0 + 4, &format!("{}", i + 1), 20.0, [0.95, 0.93, 0.86]);
        if incumbent == Some(i) {
            label(&mut rgba, w, x0 + 4, y0 + th - 22, "current", 14.0, [0.47, 0.9, 0.86]);
        }
        boxes.push([x0, y0, tw, th]);
        measures.push(Some(variant_measures(px, tw, th, &subject, &job.gallery, &values[i])));
    }
    let alike = rendered_alike(&tiles);
    let warning = (!alike.is_empty()).then(|| {
        Json::obj([
            ("rendered_alike", Json::arr(alike.iter().map(|p| Json::arr(p.map(Json::from))))),
            (
                "message",
                Json::from("some tiles render (almost) the same although their values differ: a parameter may make no visible difference here (fog far from the camera, a light the camera can't see); show it from another camera or take it out of the space"),
            ),
        ])
    });
    let sheet = sheet_name(&job.gallery.id, job.n);
    let dir = &shared.capture_dir;
    let path = dir.join(&sheet);
    let png_bytes = png::encode_rgba8(w, h, &rgba, Level::Fastest);
    // Through the disk book (captures: prunable once old, never while a review names them).
    crate::disk::book().write(crate::disk::Category::Captures, "gallery", &path, &png_bytes, crate::disk::Need::Agent).map_err(|e| {
        let mut j = e.to_json();
        j.set("error", Json::from(if e.code == "io" { "sheet_not_saved" } else { e.code }));
        j
    })?;
    let (count, cost) = ctx.timing;
    let cost_json = Json::obj([("renders", Json::from(count)), ("max_engine_ms", r1(cost.ms)), ("max_gpu_ms", r1(cost.gpu_ms)), ("wall_ms", r1(started.elapsed().as_secs_f64() * 1000.0))]);
    let packet = path.with_extension("json");
    let p = Json::obj([
        ("schema", Json::from(sight::SCHEMA)),
        ("kind", Json::from("gallery_round")),
        ("gallery", Json::from(job.gallery.id.as_str())),
        ("round", Json::from(job.n)),
        ("subject", job.gallery.subject.clone()),
        ("size", Json::arr([Json::from(w), Json::from(h)])),
        (
            "tiles",
            Json::arr(boxes.iter().enumerate().map(|(i, b)| {
                Json::obj([("variant", Json::from(i)), ("box", Json::arr(b.map(Json::from))), ("values", job.gallery.space.values_json(&values[i])), ("measures", measures[i].clone().unwrap_or(Json::Null))])
            })),
        ),
        ("incumbent", incumbent.map_or(Json::Null, Json::from)),
        ("renders", cost_json.clone()),
        ("palette", Json::Arr(palette_notes)),
        ("warning", warning.clone().unwrap_or(Json::Null)),
        ("replaced_alike", Json::arr(replaced.iter().map(|&k| Json::from(k)))),
        ("dropped_alike", Json::arr(dropped.iter().map(|&k| Json::from(k)))),
        ("refilled", Json::from(refilled)),
        ("fit", fit.map_or(Json::Null, Json::from)),
        ("fit_note", fit.map_or(Json::Null, |n| Json::from(fit_note(n)))),
    ]);
    let _ = crate::disk::book().write(crate::disk::Category::Captures, "gallery", &packet, p.to_pretty().as_bytes(), crate::disk::Need::Agent);
    Ok(RoundOut { sheet, path, packet, measures, tiles: boxes, size: (w, h), cost: cost_json, points, warning, incumbent, fit })
}

/// What the owner and agents are told when the style rules left room for fewer variants.
pub(crate) fn fit_note(n: usize) -> String {
    format!("{n} option{} fit the style rules", if n == 1 { "" } else { "s" })
}

/// Pairs of tiles that render (almost) alike: fewer than one pixel in [`ALIKE_SHARE`] differs by
/// more than [`ALIKE_STEP`] in any channel (a label's colour changes few pixels, but changes them
/// a lot). Values that differ but look the same mean a parameter shows nothing from where the
/// round is seen (a sight-skeptic finding: fog density from a camera above Sepulcra's mist made
/// four identical tiles).
fn rendered_alike(tiles: &[Vec<u8>]) -> Vec<[usize; 2]> {
    let mut out = Vec::new();
    for i in 0..tiles.len() {
        for j in i + 1..tiles.len() {
            let (a, b) = (&tiles[i], &tiles[j]);
            if a.len() != b.len() || a.is_empty() {
                continue;
            }
            let px = a.len() / 4;
            let differ = a.chunks_exact(4).zip(b.chunks_exact(4)).filter(|(p, q)| (0..3).any(|k| p[k].abs_diff(q[k]) > ALIKE_STEP)).count();
            if differ * ALIKE_SHARE < px {
                out.push([i, j]);
            }
        }
    }
    out
}

const ALIKE_STEP: u8 = 6;
const ALIKE_SHARE: usize = 500;

/// The sheet's grid for `n` tiles: one row up to 4, else two rows (the window's panel is wide:
/// a 3 x 3 sheet filled only two thirds of it).
pub(crate) fn sheet_grid(n: u32) -> (u32, u32) {
    let rows = if n <= 4 { 1 } else { 2 };
    (n.div_ceil(rows).max(1), rows)
}

/// Text in the system UI font on a dark plate (the bitmap font where there is none).
fn label(px: &mut [u8], w: u32, x: u32, y: u32, text: &str, size: f32, col: [f64; 3]) {
    let h = (px.len() / 4) as u32 / w.max(1);
    let Some(font) = ui_font() else {
        film::fill(px, w, x, y, (text.len() as u32 + 1) * film::CHAR_W + 2, film::CHAR_H + 4, [0, 0, 0, 200]);
        film::draw_text(px, w, x + 2, y + 2, &text.to_uppercase(), [(col[0] * 255.0) as u8, (col[1] * 255.0) as u8, (col[2] * 255.0) as u8]);
        return;
    };
    let layout = specula_text::layout(font, text, &specula_text::TextStyle::new(size));
    let (bw, bh) = ((layout.width + 8.0) as u32, (layout.height + 4.0) as u32);
    for yy in y..(y + bh).min(h) {
        for xx in x..(x + bw).min(w) {
            let o = ((yy * w + xx) * 4) as usize;
            for k in 0..3 {
                px[o + k] = (f32::from(px[o + k]) * 0.3) as u8;
            }
        }
    }
    draw_glyphs(px, w, h, font, &layout, x as f32 + 4.0, y as f32 + 2.0 + layout.ascent, size, col);
}

/// Blends a laid-out text's glyphs into `px` at pen `x0`, baseline `baseline`.
#[allow(clippy::too_many_arguments)]
fn draw_glyphs(px: &mut [u8], w: u32, h: u32, font: &specula_text::Font, layout: &specula_text::TextLayout, x0: f32, baseline: f32, size: f32, col: [f64; 3]) {
    for g in &layout.glyphs {
        let fx = x0 + g.x;
        let Ok(bm) = font.rasterize_glyph(g.glyph, size, fx - fx.floor()) else { continue };
        let (gx, gy) = (fx.floor() as i64 + i64::from(bm.left), baseline.floor() as i64 + i64::from(bm.top));
        for y in 0..bm.height {
            for x in 0..bm.width {
                let a = f64::from(bm.get(x, y)) / 255.0;
                let (tx, ty) = (gx + i64::from(x), gy + i64::from(y));
                if a <= 0.0 || tx < 0 || ty < 0 || tx >= i64::from(w) || ty >= i64::from(h) {
                    continue;
                }
                let o = ((ty as u32 * w + tx as u32) * 4) as usize;
                for k in 0..3 {
                    let dst = f64::from(px[o + k]) / 255.0;
                    px[o + k] = ((col[k] * a + dst * (1.0 - a)) * 255.0) as u8;
                }
            }
        }
    }
}

/// A round's points after the palette rules ([`settle`]).
pub(crate) struct Settled {
    pub points: Vec<Vec<f64>>,
    pub notes: Vec<Json>,
    pub incumbent: Option<usize>,
    /// The planned tiles dropped (indices into the planned points).
    pub dropped: Vec<usize>,
    /// How many tiles were planned again in their place.
    pub refilled: usize,
}

/// The palette rules on a round's points ([`palette_guard`]), then the look-alikes they leave.
/// A guarded colour that found no room of its own is dropped (never the incumbent): the owner
/// sees fewer tiles rather than two they can't tell apart (a sight-skeptic finding). Where the
/// engine planned the round (`refill`), dropped tiles are planned again among points the rules
/// leave as they are and that look like no tile shown: a "can't afford" spread the rules leave
/// little more than a line of greys still fills, from where its greys actually are.
pub(crate) fn settle(root: &Path, g: &Gallery, planned: &[Vec<f64>], incumbent: Option<usize>, refill: bool, round: usize) -> Settled {
    let (mut points, mut notes) = palette_guard(root, g, planned);
    let values: Vec<Vec<Value>> = points.iter().map(|p| g.space.values(p)).collect();
    let guarded: std::collections::BTreeSet<usize> = notes.iter().filter_map(|n| n.get("variant").and_then(Json::as_u64)).map(|v| v as usize).collect();
    let mut dropped = Vec::new();
    for &i in &guarded {
        if Some(i) != incumbent && (0..values.len()).any(|j| j != i && !dropped.contains(&j) && g.space.same_shown(&values[i], &values[j])) {
            dropped.push(i);
        }
    }
    let mut incumbent = incumbent;
    if dropped.is_empty() {
        return Settled { points, notes, incumbent, dropped, refilled: 0 };
    }
    let keep: Vec<usize> = (0..points.len()).filter(|i| !dropped.contains(i)).collect();
    incumbent = incumbent.and_then(|inc| keep.iter().position(|&k| k == inc));
    points = keep.iter().map(|&k| points[k].clone()).collect();
    for n in &mut notes {
        let v = n.get("variant").and_then(Json::as_u64).map(|v| v as usize);
        let _ = match v.and_then(|v| keep.iter().position(|&k| k == v)) {
            Some(now) => n.set("variant", Json::from(now)),
            None => n.set("dropped", Json::Bool(true)),
        };
    }
    let mut refilled = 0;
    if refill {
        let mut rng = specula_core::rng::Rng::new(g.seed ^ 0x7265_6669_6c6c, round as u64);
        let region = specula_gustus::spread::Region { centre: None, radius: 1.0 };
        let cands = specula_gustus::spread::dispersed(REFILL_CANDIDATES, g.space.dims(), &[], &points, &region, &mut rng);
        let (cands, flagged) = palette_guard(root, g, &cands);
        let flagged: Vec<usize> = flagged.iter().filter_map(|n| n.get("variant").and_then(Json::as_u64)).map(|v| v as usize).collect();
        let mut shown: Vec<Vec<Value>> = points.iter().map(|p| g.space.values(p)).collect();
        for (k, c) in cands.into_iter().enumerate() {
            if refilled == dropped.len() {
                break;
            }
            let v = g.space.values(&c);
            if !flagged.contains(&k) && !shown.iter().any(|s| g.space.same_shown(s, &v)) {
                shown.push(v);
                points.push(c);
                refilled += 1;
            }
        }
    }
    Settled { points, notes, incumbent, dropped, refilled }
}

/// Candidates a refill chooses from (farthest-first, so the first that pass are spread out).
const REFILL_CANDIDATES: usize = 96;

/// The game's palette rules on interface colours (the integrator's review: a "can't afford"
/// spread offered fire orange and grave green, which STYLE.md reserves for fire and the dead).
/// A `ui.*` colour that reads as a family `style.toml` reserves (`[palette] reserve`) or forbids
/// to the interface (`forbid = { ui = [...] }`) loses its colour (lightness and hue kept, chroma
/// under the neutral limit), and steps its lightness apart if that makes it a look-alike of
/// another tile (a live round's guarded tiles had all collapsed onto the base). Returns the
/// points and a note per change.
pub(crate) fn palette_guard(root: &Path, g: &Gallery, points: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<Json>) {
    let mut out = points.to_vec();
    let mut notes = Vec::new();
    let has_ui = g.space.params.iter().any(|p| matches!(p.target, Target::Ui(_)));
    if !has_ui {
        return (out, notes);
    }
    let style = std::fs::read_to_string(root.join("style.json")).ok().and_then(|s| specula_model::Style::parse(&s).ok()).unwrap_or_default();
    let Ok(bible) = Bible::load(&style, std::fs::read_to_string(root.join("style.toml")).ok().as_deref(), "style.toml") else { return (out, notes) };
    let rules = &bible.rules.palette;
    let banned: Vec<String> = rules.reserve.iter().map(|(f, _)| f.clone()).chain(rules.forbid.iter().filter(|(c, _)| c == "ui").flat_map(|(_, fs)| fs.iter().cloned())).collect();
    if banned.is_empty() || bible.palette.is_empty() {
        return (out, notes);
    }
    // Each banned family's hue arc: the members' Oklab hues (colourful ones), widened by
    // HUE_MARGIN. Nearest-colour alone lets an olive pass as the floor's moss, and a peach as no
    // one's; on the interface, any colourful hue in a family's arc reads as that family. 30
    // degrees: at 15, sage and mint accents (hue 156-161) passed as no one's green.
    // Hue and chroma come from the deterministic Oklch of specula-gustus (dmath, no platform
    // trigonometry: the guarded values reach stored rounds the planner reads; taste review 2).
    const HUE_MARGIN: f64 = 30.0;
    let lch = |c: [f64; 3]| specula_gustus::colour::srgb_to_oklch(c);
    let wide = |c: [f32; 3]| [f64::from(c[0]), f64::from(c[1]), f64::from(c[2])];
    let nmc = f64::from(rules.neutral_max_chroma);
    let gap = |a: f64, b: f64| specula_gustus::colour::hue_delta(a, b).abs();
    let arcs: Vec<(String, f64, f64)> = rules.families.iter().filter(|(f, _)| banned.contains(f)).filter_map(|(f, members)| {
        let hues: Vec<f64> = members.iter().filter_map(|m| bible.palette.colour(m)).map(|c| lch(wide(c))).filter(|l| l[1] > nmc).map(|l| l[2]).collect();
        let first = *hues.first()?;
        // The arc's middle: the first hue moved by the members' mean signed difference from it.
        let mid = first + hues.iter().map(|&h| specula_gustus::colour::hue_delta(first, h)).sum::<f64>() / hues.len() as f64;
        let half = hues.iter().map(|&h| gap(h, mid)).fold(0.0, f64::max) + HUE_MARGIN;
        Some((f.clone(), mid, half))
    }).collect();
    let reads_as = |c: [f64; 3]| -> Option<String> {
        let l = lch(c);
        if l[1] <= nmc {
            return None;
        }
        if let Some((f, _, _)) = arcs.iter().find(|(_, mid, half)| gap(l[2], *mid) <= *half) {
            return Some(f.clone());
        }
        let c = [c[0] as f32, c[1] as f32, c[2] as f32];
        let n = bible.palette.nearest(c, &[])?;
        let fam = n.family.or(n.touches)?;
        (n.delta_e <= 0.12 && banned.contains(&fam)).then_some(fam)
    };
    use specula_gustus::colour as gc;
    let mut at = 0;
    for p in &g.space.params {
        let d = p.dims();
        if matches!(p.target, Target::Ui(_)) {
            let shown = |u: &[f64]| p.value_at(u).as_colour().unwrap_or([0.5; 3]);
            let flagged: Vec<bool> = out.iter().map(|pt| reads_as(shown(&pt[at..at + d])).is_some()).collect();
            for i in 0..out.len() {
                if !flagged[i] {
                    continue;
                }
                let c = shown(&out[i][at..at + d]);
                let fam = reads_as(c).unwrap_or_default();
                let before = p.value_at(&out[i][at..at + d]).to_json();
                // The tiles it must stay apart from: those already settled (earlier, or never moved).
                let others: Vec<[f64; 3]> = (0..out.len()).filter(|&j| j != i && (j < i || !flagged[j])).map(|j| shown(&out[j][at..at + d])).collect();
                let lch = gc::srgb_to_oklch(c);
                let base = gc::srgb_to_oklch(shown(&p.default_unit()));
                // First its own hue with the colour taken out (half the neutral limit: at 0.8 of
                // it a guarded olive still read as sage green); where the space can't go that grey
                // (a saturated accent's spread), its hue and chroma turned towards the base's.
                // Then the base's own hue at a few greys: a spread the rules leave little more
                // than a line of greys still holds several that read apart (a live round dropped
                // six of nine with only the first look and lightness steps to try).
                let nmc = rules.neutral_max_chroma as f64;
                let mut looks = vec![(lch[1].min(nmc * 0.5), lch[2]), (nmc * 0.5, base[2]), (nmc * 0.9, base[2]), (nmc * 0.2, lch[2])];
                for t in [0.25, 0.5, 0.75, 1.0] {
                    looks.push((lch[1] + (base[1] - lch[1]) * t, lch[2] + gc::hue_delta(lch[2], base[2]) * t));
                }
                let (mut best, mut fallback) = (None, None);
                'search: for (chroma, h) in looks {
                    for k in 0..=14 {
                        for s in [1.0, -1.0] {
                            if k == 0 && s < 0.0 {
                                continue;
                            }
                            let l = (lch[0] + s * k as f64 * GUARD_STEP_L).clamp(0.2, 0.97);
                            let u = p.unit_of(&Value::Colour(gc::oklch_to_srgb([l, chroma, h])));
                            let got = shown(&u);
                            if reads_as(got).is_some() {
                                continue;
                            }
                            let apart = others.iter().map(|o| gc::delta_e(*o, got)).fold(f64::INFINITY, f64::min);
                            if apart >= p.min_shown_de() {
                                best = Some(u);
                                break 'search;
                            }
                            // Short of that, the look farthest from every other tile.
                            if fallback.as_ref().is_none_or(|(d, _)| apart > *d) {
                                fallback = Some((apart, u));
                            }
                        }
                    }
                }
                let u = best.or(fallback.map(|(_, u)| u)).unwrap_or_else(|| p.default_unit());
                out[i][at..at + d].copy_from_slice(&u);
                notes.push(Json::obj([("variant", Json::from(i)), ("param", Json::from(p.name.as_str())), ("was", before), ("read_as", Json::from(fam)), ("now", p.value_at(&out[i][at..at + d]).to_json())]));
            }
        }
        at += d;
    }
    (out, notes)
}

/// A guarded interface colour keeps its lightness and hue and loses its chroma (to under the
/// palette's neutral limit); if that makes it look like another tile, its lightness steps away by
/// this much (Oklab L) until it is an interface look-alike of no other tile
/// ([`specula_gustus::space::MIN_SHOWN_DE_UI`]).
const GUARD_STEP_L: f64 = 0.025;

fn text_of(o: &ToolOutput) -> String {
    o.content.iter().find_map(|c| match c {
        Content::Text(t) => Some(t.clone()),
        Content::Image { .. } => None,
    }).unwrap_or_default()
}

/// A tile's measures: lightness, contrast and chroma (Oklab, at the measuring size); a ui
/// tile's text contrast against its panel (WCAG ratio and Oklab lightness difference).
fn variant_measures(px: &[u8], w: u32, h: u32, subject: &Subject, g: &Gallery, values: &[Value]) -> Json {
    let mut j = match specula_gustus::board::measure(px, w, h) {
        Ok(m) => Json::obj([("value_mean", r3(m.value_mean as f32)), ("contrast", r3(m.contrast as f32)), ("chroma_mean", r3(m.chroma_mean as f32)), ("edge_density", r3(m.edge_density as f32))]),
        Err(_) => Json::obj::<String>([]),
    };
    if let Subject::Ui { panel, lines, .. } = subject {
        let (_, _, _, ui) = variant_look(&g.space, values);
        let lum = |c: [f64; 3]| {
            let l = c.map(specula_gustus::colour::to_linear);
            0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2]
        };
        let mut texts = Vec::new();
        for (text, c) in lines {
            let col = match c {
                LineColour::Fixed(c) => *c,
                LineColour::Param(p) => ui.get(p).copied().unwrap_or([0.9; 3]),
            };
            let (a, b) = (lum(col), lum(*panel));
            let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            let dl = specula_gustus::colour::srgb_to_oklab(col)[0] - specula_gustus::colour::srgb_to_oklab(*panel)[0];
            texts.push(Json::obj([("text", Json::from(text.as_str())), ("color", Json::from(specula_gustus::colour::hex(col))), ("contrast_ratio", r3(ratio as f32)), ("delta_l", r3(dl as f32))]));
        }
        j.set("text", Json::Arr(texts));
    }
    j
}

impl Ctx<'_> {
    /// Every variant's tile pixels (`tw`×`th`).
    fn gallery_tiles(&mut self, subject: &Subject, g: &Gallery, values: &[Vec<Value>], tw: u32, th: u32) -> Result<Vec<Vec<u8>>, ToolOutput> {
        let looks: Vec<_> = values.iter().map(|v| variant_look(&g.space, v)).collect();
        match subject {
            Subject::Recipe { spec, az, el, references, base } => {
                // One build per distinct parameter set.
                let mut bibles = Bibles::default();
                let mut staged: Vec<Staged> = Vec::new();
                let mut index = Vec::new();
                for (_, scale, params, _) in &looks {
                    let mut s = spec.clone();
                    for (k, v) in params {
                        s.set_param(k, *v);
                    }
                    let _ = scale;
                    let i = match staged.iter().position(|x| x.spec == s) {
                        Some(i) => i,
                        None => {
                            let mut one = self.studio_staged(&[SubjectSpec { spec: s, label: None }], &mut bibles)?;
                            staged.push(one.remove(0));
                            staged.len() - 1
                        }
                    };
                    index.push(i);
                }
                // One camera for every variant: the union of their (scaled) boxes and the figure.
                let (mut lo, mut hi) = (Vec3::new(f32::MAX, 0.0, f32::MAX), Vec3::new(f32::MIN, 0.0, f32::MIN));
                for (k, (_, scale, ..)) in looks.iter().enumerate() {
                    let s = &staged[index[k]];
                    lo = lo.min(Vec3::new(s.lo.x * scale.x, s.lo.y * scale.y, s.lo.z * scale.z));
                    hi = hi.max(Vec3::new(s.hi.x * scale.x, s.hi.y * scale.y, s.hi.z * scale.z));
                }
                let fig = references.then(|| {
                    let f = Vec3::new(lo.x - 0.6 - 0.35, 0.0, 0.0);
                    let (a, b) = reference_box(f);
                    lo = lo.min(a);
                    hi = hi.max(b);
                    f
                });
                let cam = fit(lo, hi, *az, *el, FIT_FOV, tw as f32 / th as f32, 1.08);
                let mut out = Vec::new();
                for (k, (patch, scale, ..)) in looks.iter().enumerate() {
                    let s = &staged[index[k]];
                    let (rig, fog) = stage_light(*base, &s.rules().stage.game, patch);
                    let req = RenderReq {
                        subjects: vec![PlacedOwned { built: Arc::clone(&s.built), skin: s.skin.clone(), at: Vec3::ZERO, yaw: 0.0, lod: 0, radius: s.radius() * scale.x.max(scale.y).max(scale.z), scale: *scale }],
                        refs: fig,
                        cam,
                        w: tw,
                        h: th,
                        aspect: tw as f32 / th as f32,
                        light: Some((rig, false)),
                        extra: fog.map(|f| Box::new(Extra { fog: Some(f), scene: None })),
                    };
                    out.push(self.gallery_render(req, tw, th)?);
                }
                Ok(out)
            }
            Subject::Stage { base } => {
                let at = Vec3::ZERO;
                let (lo, hi) = reference_box(at);
                let cam = fit(lo, hi, 30.0, 14.0, FIT_FOV, tw as f32 / th as f32, 1.15);
                let game = self.shared.library.lock().root().join("style.toml");
                let game_rig = std::fs::read_to_string(game).ok().and_then(|t| Bible::load(&specula_model::Style::default(), Some(&t), "style.toml").ok()).map_or_else(neutral_rig, |b| b.rules.stage.game);
                let mut out = Vec::new();
                for (patch, ..) in &looks {
                    let (rig, fog) = stage_light(*base, &game_rig, patch);
                    let req = RenderReq { subjects: Vec::new(), refs: Some(at), cam, w: tw, h: th, aspect: tw as f32 / th as f32, light: Some((rig, false)), extra: fog.map(|f| Box::new(Extra { fog: Some(f), scene: None })) };
                    out.push(self.gallery_render(req, tw, th)?);
                }
                Ok(out)
            }
            Subject::Scene { camera, look } => {
                let mut out = Vec::new();
                for (patch, ..) in &looks {
                    // The subject's base style (the configurator's later steps), unless the
                    // variant picks one.
                    let mut p = patch.clone();
                    if p.preset.is_none() {
                        p.preset = *look;
                    }
                    out.push(self.scene_render(camera, p, tw, th)?);
                }
                Ok(out)
            }
            Subject::Ui { panel, background, lines, camera } => {
                let back = match camera {
                    Some(c) => Some(self.scene_render(c, Patch::default(), tw, th)?),
                    None => None,
                };
                Ok(looks.iter().map(|(_, _, _, ui)| ui_tile(back.as_deref(), *background, *panel, lines, ui, tw, th)).collect())
            }
        }
    }

    /// The live world through `camera` with `patch` applied (read only), `w`×`h`.
    pub(crate) fn scene_render(&mut self, camera: &Camera, patch: Patch, w: u32, h: u32) -> Result<Vec<u8>, ToolOutput> {
        let cam = Cam { eye: camera.eye, target: camera.target, fov: camera.fov_y_deg };
        let req = RenderReq { subjects: Vec::new(), refs: None, cam, w, h, aspect: w as f32 / h as f32, light: Some((neutral_rig(), false)), extra: Some(Box::new(Extra { fog: None, scene: Some(SceneLook { camera: *camera, patch, call: self.call }) })) };
        self.gallery_render(req, w, h)
    }

    fn gallery_render(&mut self, mut req: RenderReq, w: u32, h: u32) -> Result<Vec<u8>, ToolOutput> {
        if self.renders >= MAX_RENDERS {
            return Err(error("too_many_renders", format!("a round needs more than {MAX_RENDERS} renders")));
        }
        self.renders += 1;
        let (rw, rh) = render_size(w, h);
        req.w = rw;
        req.h = rh;
        self.send_render(req, w, h)
    }
}

/// Opens a studio context for one scene render outside a gallery round (`board_check`).
pub(crate) fn render_scene_once(tools: &crate::tools::Tools, shared: &Shared, session: &SessionInfo, camera: &Camera, w: u32, h: u32) -> Result<Vec<u8>, ToolOutput> {
    let id = shared.admit(&session.id, 1)?;
    let mut ctx = Ctx { shared, tools, session, call: id, deadline: Instant::now() + CALL_LIMIT, renders: 0, render_scale: 1.0, timing: (0, RenderCost::default()) };
    ctx.scene_render(camera, Patch::default(), w, h)
}

/// The system UI font, loaded once (galleries' ui tiles).
fn ui_font() -> Option<&'static specula_text::Font> {
    static FONT: std::sync::OnceLock<Option<specula_text::Font>> = std::sync::OnceLock::new();
    FONT.get_or_init(|| specula_text::find_system_ui_font().map(|(_, f)| f)).as_ref()
}

/// A `ui` tile: the backdrop (a scene render or a colour), the panel, and its lines of text in
/// their colours, drawn with the system font.
fn ui_tile(back: Option<&[u8]>, background: [f64; 3], panel: [f64; 3], lines: &[(String, LineColour)], ui: &BTreeMap<String, [f64; 3]>, w: u32, h: u32) -> Vec<u8> {
    let b8 = |c: [f64; 3]| [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255];
    let mut px = match back {
        Some(b) if b.len() == (w * h * 4) as usize => b.to_vec(),
        _ => {
            let mut v = vec![0u8; (w * h * 4) as usize];
            film::fill(&mut v, w, 0, 0, w, h, b8(background));
            v
        }
    };
    let (px0, py0, pw, ph) = (w / 12, h / 6, w - w / 6, h - h / 3);
    film::fill(&mut px, w, px0.saturating_sub(1), py0.saturating_sub(1), pw + 2, ph + 2, [8, 9, 12, 255]);
    film::fill(&mut px, w, px0, py0, pw, ph, b8(panel));
    let Some(font) = ui_font() else {
        // No system font: the bitmap font, so the colours still show.
        for (i, (t, c)) in lines.iter().enumerate() {
            let col = match c {
                LineColour::Fixed(c) => *c,
                LineColour::Param(p) => ui.get(p).copied().unwrap_or([0.9; 3]),
            };
            let rgb = b8(col);
            film::draw_text(&mut px, w, px0 + 8, py0 + 8 + i as u32 * (film::CHAR_H + 6), &t.to_uppercase(), [rgb[0], rgb[1], rgb[2]]);
        }
        return px;
    };
    let size = (ph as f32 / (lines.len() as f32 + 0.8) * 0.62).clamp(9.0, 40.0);
    let line_h = ph as f32 / (lines.len() as f32 + 0.8);
    for (i, (t, c)) in lines.iter().enumerate() {
        let col = match c {
            LineColour::Fixed(c) => *c,
            LineColour::Param(p) => ui.get(p).copied().unwrap_or([0.9; 3]),
        };
        let layout = specula_text::layout(font, t, &specula_text::TextStyle::new(size));
        let baseline = py0 as f32 + line_h * (i as f32 + 0.4) + layout.ascent;
        let x0 = px0 as f32 + (pw as f32 - layout.width).max(0.0) * 0.5;
        draw_glyphs(&mut px, w, h, font, &layout, x0, baseline, size, col);
    }
    px
}

impl Engine {
    /// A gallery variant of the live world: the window's draw path through the variant's camera,
    /// the world's environment with the variant's patch, into the studio's own target. Nothing is
    /// written (no ID buffer, no flash watch, no effects: the look, not the moment).
    pub(super) fn render_scene_variant(&mut self, req: &RenderReq, scene: &SceneLook) -> Result<Vec<u8>, ToolOutput> {
        let (w, h) = (req.w, req.h);
        let camera = scene.camera;
        let view_proj = camera.view_proj(w, h);
        let mut draws = Vec::new();
        let time_s = match self.taste.scene_time {
            Some((call, t)) if call == scene.call => t,
            _ => {
                let t = self.window_time_s();
                self.taste.scene_time = Some((scene.call, t));
                t
            }
        };
        let _ = self.build_draws(camera, h, false, None, false, time_s, &mut draws, None);
        let mut lights = Vec::new();
        sight::lights(&self.world, false, &mut lights);
        let bones = self.models.take_bones();
        let mut frame = FrameDesc { bones: &bones, ..sight::frame(&self.world, &view_proj, &draws, &lights, time_s as f32) };
        self.luts.apply(&mut self.gpu, self.world.env(), &mut frame, true);
        let p = &scene.patch;
        let env = &mut frame.environment;
        if let Some(pr) = p.preset {
            let r = preset_rig(pr);
            env.sun_color = r.key_color;
            env.sun_intensity = r.key_intensity;
            env.ambient_color = r.ambient_color;
            env.ambient_intensity = r.ambient_intensity;
            env.exposure = r.exposure;
            env.fog = fog_of(pr.fog_density as f32, r.background).unwrap_or(env.fog);
            frame.clear_color = [r.background[0], r.background[1], r.background[2], 1.0];
        }
        if let Some(c) = p.key_color {
            env.sun_color = c;
        }
        if let Some(v) = p.key_intensity {
            env.sun_intensity = v;
        }
        if p.key_elevation.is_some() || p.key_azimuth.is_some() {
            let d = Vec3::new(env.sun_direction[0], env.sun_direction[1], env.sun_direction[2]).normalize();
            let el0 = d.y.clamp(-1.0, 1.0).asin().to_degrees();
            let cam_yaw = Cam { eye: camera.eye, target: camera.target, fov: camera.fov_y_deg }.yaw();
            let az0 = d.x.atan2(d.z).to_degrees() - cam_yaw;
            let (az, el) = ((cam_yaw + p.key_azimuth.unwrap_or(az0)).to_radians(), p.key_elevation.unwrap_or(el0).to_radians());
            env.sun_direction = [el.cos() * az.sin(), el.sin(), el.cos() * az.cos()];
        }
        if let Some(c) = p.ambient_color {
            env.ambient_color = c;
        }
        if let Some(v) = p.ambient_intensity {
            env.ambient_intensity = v;
        }
        if let Some(v) = p.exposure {
            env.exposure = v;
        }
        if p.fog_density.is_some() || p.fog_color.is_some() {
            let base = if env.fog.enabled { env.fog } else { specula_gpu::FogSettings { color: [frame.clear_color[0], frame.clear_color[1], frame.clear_color[2]], ..specula_gpu::FogSettings::default() } };
            let density = p.fog_density.unwrap_or(if env.fog.enabled { base.density } else { 0.0 });
            env.fog = specula_gpu::FogSettings { enabled: density > 0.0, density, color: p.fog_color.unwrap_or(base.color), ..base };
        }
        if let Some(c) = p.backdrop {
            frame.clear_color = [c[0], c[1], c[2], 1.0];
        }
        let target = self.studio_target(w, h, 1);
        let out = target.map_err(ToolOutput::error).and_then(|t| {
            self.gpu.render(t, &frame).map_err(|e| ToolOutput::error(capture::gpu_failure("render", &e, self.windowed)))?;
            self.gpu.read_rgba8(t).map_err(|e| ToolOutput::error(capture::gpu_failure("readback", &e, self.windowed)))
        });
        self.models.put_bones(bones);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subjects_parse_and_targets_are_checked() {
        let sp = |s: &str| specula_gustus::space::Space::parse(&Json::parse(s).unwrap()).unwrap();
        let rec = parse_subject(&Json::parse(r#"{"recipe": "recipes/tombstone.json", "params": {"tilt": 0.5}}"#).unwrap()).unwrap();
        assert!(matches!(&rec, Subject::Recipe { spec, .. } if spec.params == vec![("tilt".to_owned(), 0.5)]));
        assert!(check_targets(&rec, &sp(r#"[{"name": "h", "target": "scale.y", "min": 0.8, "max": 1.3}]"#)).is_ok());
        let ui = parse_subject(&Json::parse(r##"{"ui": {"panel": "#2b3038", "lines": [{"text": "Not enough dust", "color": "$cant_afford"}]}}"##).unwrap()).unwrap();
        assert!(check_targets(&ui, &sp(r##"[{"name": "c", "target": "ui.cant_afford", "base": "#ffd27a"}]"##)).is_ok());
        assert!(check_targets(&ui, &sp(r##"[{"name": "c", "target": "ui.accent", "base": "#ffd27a"}]"##)).is_err(), "no line is coloured $accent");
        assert!(check_targets(&ui, &sp(r#"[{"name": "h", "target": "scale.y", "min": 0.8, "max": 1.3}]"#)).is_err());
        assert!(parse_subject(&Json::parse(r#"{"scene": {"camera": "overview"}}"#).unwrap()).is_err(), "named cameras are resolved first");
        assert!(parse_subject(&Json::parse(r#"{"scene": {"eye": [0, 5, 10], "target": [0, 0, 0]}}"#).unwrap()).is_ok());
        assert!(matches!(parse_subject(&Json::parse(r#"{"stage": {}, "look": "noir"}"#).unwrap()).unwrap(), Subject::Stage { base: Base::Preset(p) } if p.name == "noir"));
        assert!(parse_subject(&Json::parse(r#"{"look": "neon"}"#).unwrap()).is_err());
    }

    /// Review probes: a `{look}` subject with the keys the engine adds still parses (it failed
    /// at render), and sheets fill a wide panel (two rows).
    #[test]
    fn stored_subjects_and_sheet_grids() {
        assert!(matches!(parse_subject(&Json::parse(r#"{"look": "noir", "tile": 256, "start": {"x": 1}, "onboarding_step": "base"}"#).unwrap()), Ok(Subject::Stage { .. })));
        assert_eq!(sheet_grid(9), (5, 2));
        assert_eq!(sheet_grid(4), (4, 1));
        assert_eq!(sheet_grid(12), (6, 2));
        assert_eq!(sheet_grid(5), (3, 2));
    }

    /// The integrator's review: a "can't afford" spread offered fire orange and grave green.
    /// With Sepulcra's palette rules, no variant of that spread reads as fire or the dead.
    #[test]
    fn interface_colours_keep_to_the_palette_rules() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
        let space = specula_gustus::space::Space::parse(&Json::parse(r##"[{"name": "cant_afford", "target": "ui.cant_afford", "kind": "colour", "base": "#b3a78f", "spread": {"l": 0.25, "c": 0.15, "h": 120}}]"##).unwrap()).unwrap();
        let g = specula_gustus::gallery::Gallery::new("ca", "t", Json::parse(r##"{"ui": {"panel": "#2b3038", "lines": [{"text": "Not enough dust", "color": "$cant_afford"}]}}"##).unwrap(), space, specula_gustus::gallery::Mode::Tune, 3, 9, "maker").unwrap();
        // A wide spread: hues all round, the most saturated.
        let points: Vec<Vec<f64>> = (0..12).map(|i| vec![0.6, 1.0, i as f64 / 11.0]).collect();
        let (out, notes) = palette_guard(&root, &g, &points);
        assert!(!notes.is_empty(), "some of a full hue circle reads as fire or grave green");
        let style = specula_model::Style::parse(&std::fs::read_to_string(root.join("style.json")).unwrap()).unwrap();
        let bible = Bible::load(&style, Some(&std::fs::read_to_string(root.join("style.toml")).unwrap()), "style.toml").unwrap();
        for p in &out {
            let c = g.space.values(p)[0].as_colour().unwrap();
            let c = [c[0] as f32, c[1] as f32, c[2] as f32];
            if specula_studio::colour::chroma(c) > bible.rules.palette.neutral_max_chroma {
                let n = bible.palette.nearest(c, &[]).unwrap();
                let fam = n.family.or(n.touches);
                assert!(!(n.delta_e <= 0.12 && matches!(fam.as_deref(), Some("fire" | "enemy"))), "{:?} still reads as {fam:?}", g.space.values(p)[0].to_json());
            }
        }
        // Two that got through a live round on nearest-colour alone: an olive nearest the floor's
        // moss (still the dead's green on a label) and a peach nearest no family (still fire's
        // warmth). The families' hue arcs catch both; the dim, desaturated base stays as it is.
        // Accents: sage and mint (hue 156-161) are the dead's green to a player and are guarded;
        // the runes' cyan (hue 185) is the player's and is kept.
        let accent = specula_gustus::space::Space::parse(&Json::parse(r##"[{"name": "accent", "target": "ui.accent", "kind": "colour", "base": "#3fe0d0", "spread": {"l": 0.2, "c": 0.1, "h": 60}}]"##).unwrap()).unwrap();
        let ga = specula_gustus::gallery::Gallery::new("acc", "t", Json::parse(r##"{"ui": {"panel": "#2b3038", "lines": [{"text": "Selected", "color": "$accent"}]}}"##).unwrap(), accent, specula_gustus::gallery::Mode::Tune, 3, 9, "maker").unwrap();
        let pa = &ga.space.params[0];
        let pts: Vec<Vec<f64>> = ["#79bf96", "#57ffb6", "#3fe0d0"].iter().map(|h| pa.unit_of(&pa.parse_value(&Json::from(*h)).unwrap())).collect();
        let (out_a, notes_a) = palette_guard(&root, &ga, &pts);
        let flagged: Vec<u64> = notes_a.iter().filter_map(|n| n.get("variant").and_then(Json::as_u64)).collect();
        assert_eq!(flagged, vec![0, 1], "{notes_a:?}");
        assert_eq!(out_a[2], pts[2], "the runes' cyan stays");
        // And the guarded accents don't all collapse onto the base (a live onboarding round's
        // five green accents all became the base's cyan).
        let sa: Vec<[f64; 3]> = out_a.iter().map(|p| pa.value_at(p).as_colour().unwrap()).collect();
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            let de = specula_gustus::colour::delta_e(sa[i], sa[j]);
            assert!(de >= specula_gustus::space::MIN_SHOWN_DE_UI, "accents {i} {:?} and {j} {:?} read alike (dE {de:.3})", pa.value_at(&out_a[i]).to_json(), pa.value_at(&out_a[j]).to_json());
        }
        // That live round, whole: once guarded, its four warm and green tiles must not collapse
        // onto the base either (the first fix pulled all four to within 0.02 of it).
        let space = specula_gustus::space::Space::parse(&Json::parse(r##"[{"name": "cant_afford", "target": "ui.cant_afford", "kind": "colour", "base": "#b3a78f", "spread": {"l": 0.14, "c": 0.06, "h": 50}}]"##).unwrap()).unwrap();
        let g = specula_gustus::gallery::Gallery::new("ca", "t", Json::parse(r##"{"ui": {"panel": "#2b3038", "lines": [{"text": "Not enough dust", "color": "$cant_afford"}]}}"##).unwrap(), space, specula_gustus::gallery::Mode::Tune, 3, 9, "maker").unwrap();
        let param = &g.space.params[0];
        let at = |hex: &str| param.unit_of(&param.parse_value(&Json::from(hex)).unwrap());
        let live =["#b3a78f", "#b9de9d", "#d2d5d0", "#858180", "#b77259", "#c9c8c8", "#ffbf9e", "#728a58", "#e4d1bc"];
        let points: Vec<Vec<f64>> = live.iter().map(|h| at(h)).collect();
        let (out, notes) = palette_guard(&root, &g, &points);
        let read: Vec<(usize, String)> = notes.iter().map(|n| (n.get("variant").and_then(Json::as_u64).unwrap() as usize, n.get("read_as").and_then(Json::as_str).unwrap().to_string())).collect();
        assert_eq!(read, vec![(1, "enemy".into()), (4, "fire".into()), (6, "fire".into()), (7, "enemy".into())], "{notes:?}");
        for i in [0, 2, 3, 5, 8] {
            assert_eq!(out[i], points[i], "tile {i} reads as no family and is never moved");
        }
        // Planned rounds of that space (the interface gap applies), settled: guarded tiles still
        // alike are dropped and planned again among the greys the rules allow; no two left read
        // alike.
        for seed in 0..6u64 {
            let gs = specula_gustus::gallery::Gallery::new("ca", "t", g.subject.clone(), g.space.clone(), specula_gustus::gallery::Mode::Tune, seed, 9, "maker").unwrap();
            let planned = gs.first_round(None).points;
            let s = settle(&root, &gs, &planned, None, true, 1);
            let vals: Vec<Vec<Value>> = s.points.iter().map(|p| gs.space.values(p)).collect();
            let shown = || vals.iter().map(|v| gs.space.values_json(v)).collect::<Vec<_>>();
            // The rules leave this spread a wedge of greys about 0.28 tall and 0.08 wide: room for
            // five or six labels 0.06 apart, not nine.
            assert!(vals.len() >= 5, "seed {seed}: {} tiles left ({} dropped, {} refilled): {:?}", vals.len(), s.dropped.len(), s.refilled, shown());
            for i in 0..vals.len() {
                for j in i + 1..vals.len() {
                    assert!(!gs.space.same_shown(&vals[i], &vals[j]), "seed {seed}: tiles {i} and {j} read alike: {:?}", shown());
                }
            }
            // Without a refill (an agent's own round), the round only shrinks.
            let s2 = settle(&root, &gs, &planned, None, false, 1);
            assert_eq!(s2.points.len() + s2.dropped.len(), planned.len());
        }
        let shown: Vec<[f64; 3]> = out.iter().map(|p| param.value_at(p).as_colour().unwrap()).collect();
        for &i in &[1, 4, 6, 7] {
            let c = [shown[i][0] as f32, shown[i][1] as f32, shown[i][2] as f32];
            assert!(specula_studio::colour::chroma(c) <= bible.rules.palette.neutral_max_chroma, "tile {i} {:?} still has colour", param.value_at(&out[i]).to_json());
            for j in 0..shown.len() {
                if j != i {
                    let de = specula_gustus::colour::delta_e(shown[i], shown[j]);
                    // (This live round predates the interface gap: tiles 2 and 5 are themselves
                    // 0.04 apart, so the guard can't keep 0.06 from all; it keeps the most it can.)
                    assert!(de >= 1.5 * specula_gustus::space::MIN_SHOWN_DE, "tile {i} {:?} looks like tile {j} {:?} (dE {de:.3})", param.value_at(&out[i]).to_json(), param.value_at(&out[j]).to_json());
                }
            }
        }
    }

    #[test]
    fn tiles_that_render_alike_are_reported() {
        let n = 64 * 36 * 4;
        let base: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
        // Render noise of a step or two: alike.
        let noisy: Vec<u8> = base.iter().enumerate().map(|(i, &v)| if i % 7 == 0 { v.saturating_add(2) } else { v }).collect();
        // A label in another colour: few pixels, changed a lot. Not alike.
        let mut label = base.clone();
        for p in 0..(64 * 36 / 50) {
            label[p * 4] = label[p * 4].wrapping_add(90);
        }
        assert_eq!(rendered_alike(&[base, noisy, label]), vec![[0, 1]]);
    }

    #[test]
    fn variant_values_become_light() {
        let space = specula_gustus::space::Space::parse(&Json::parse(r##"[{"name": "moon", "target": "light.key_color", "base": "#d0d9f0"}, {"name": "fog", "target": "fog.density", "min": 0, "max": 0.04}, {"name": "w", "target": "scale.all", "min": 0.5, "max": 2}]"##).unwrap()).unwrap();
        let values = space.values(&[0.5, 0.5, 0.5, 1.0, 0.0]);
        let (patch, scale, _, _) = variant_look(&space, &values);
        assert_eq!(patch.fog_density, Some(0.04));
        assert!((scale.x - 0.5).abs() < 1e-6 && (scale.y - 0.5).abs() < 1e-6);
        let (rig, fog) = stage_light(Base::Neutral, &neutral_rig(), &patch);
        assert!((rig.key_color[2] - 0.941).abs() < 0.01, "{:?}", rig.key_color);
        assert!(fog.is_some_and(|f| f.enabled && (f.density - 0.04).abs() < 1e-6));
        // A preset gives its whole light; values on top.
        let p = Patch { preset: presets::find("noir"), ..Patch::default() };
        let (rig, fog) = stage_light(Base::Game, &neutral_rig(), &p);
        assert!(rig.key_intensity > 1.5 && fog.is_some());
        // A ui tile draws its panel.
        let px = ui_tile(None, [0.0; 3], [0.5, 0.0, 0.0], &[("A".into(), LineColour::Fixed([1.0; 3]))], &BTreeMap::new(), 120, 68);
        assert_eq!(&px[((34 * 120 + 12) * 4) as usize..((34 * 120 + 12) * 4 + 3) as usize], &[127, 0, 0]);
    }
}

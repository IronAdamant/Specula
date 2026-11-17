//! Arguments and definitions of the sight tools beyond `sight_frame`: named cameras
//! (`camera_set`, `camera_list`, `camera_delete`), `sight_orbit`, `sight_compare` and
//! `sight_query`, plus `sight_frame`'s camera, overlay and subject-filter arguments.

use crate::cameras::{CameraSpec, Follow};
use crate::orbit;
use crate::sight_overlay::{self, Kind};
use crate::tools::{
    bool_arg, check_keys, entity_key, int_arg, key_value, num_in, parse_frame, position_arg, string_arg, FrameSpec, FRAME_EXTRA_KEYS,
};
use crate::world::EntityKey;
use specula_core::math::Vec3;
use specula_core::Json;
use specula_protocol::ToolDef;

/// A named camera to look through: one's own, or another agent's (`of`).
#[derive(Clone, Debug, PartialEq)]
pub struct CameraRef {
    pub name: String,
    pub of: Option<String>,
}

/// Which subjects a response lists (the saved packet always has all of them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubjectFilter {
    #[default]
    All,
    /// Some of it inside the image (by its projected box).
    OnScreen,
    /// At least one pixel actually drawn (by the ID buffer); falls back to on-screen when the
    /// ID buffer is unavailable.
    Visible,
}

impl SubjectFilter {
    pub fn name(self) -> &'static str {
        match self {
            SubjectFilter::All => "all",
            SubjectFilter::OnScreen => "on_screen",
            SubjectFilter::Visible => "visible",
        }
    }
}

pub enum SightCall {
    CameraSet(CameraSpec),
    /// `of`: another agent's cameras; `all`: every agent's camera names.
    CameraList { of: Option<String>, all: bool },
    /// `None`: every camera of yours.
    CameraDelete { name: Option<String> },
    Orbit(OrbitSpec),
    Compare(CompareCall),
    Query(QuerySpec),
}

pub enum OrbitTarget {
    Entity(EntityKey),
    Point(Vec3),
}

/// A capture's form view (`form`, `form_scale`): the world as it is, or its lit surfaces as
/// shape alone ([`specula_gpu::FormView`]). `scale` is the curvature view's, in metres, when
/// the caller gave one; without it the capture takes a twentieth of the view's half height at
/// its target ([`Form::scale_for`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Form {
    pub view: specula_gpu::FormView,
    pub scale: Option<f32>,
}

impl Form {
    /// The curvature view's scale limits (m).
    pub const SCALE: std::ops::RangeInclusive<f32> = 0.001..=1000.0;

    pub fn is_world(&self) -> bool {
        self.view == specula_gpu::FormView::World
    }

    /// The scale a capture through `camera` draws curvature at: the caller's, or a twentieth
    /// of the view's half height where the camera looks (a figure that fills the frame shows
    /// its trunk a little lighter than flat, a limb halfway to white and a finger nearly white).
    pub fn scale_for(&self, camera: &crate::sight::Camera) -> f32 {
        self.scale.unwrap_or_else(|| {
            let half = match camera.projection {
                crate::sight::Projection::Ortho { height_m } => 0.5 * height_m,
                crate::sight::Projection::Perspective => (camera.target - camera.eye).length() * (0.5 * camera.fov_y_deg).to_radians().tan(),
            };
            (0.05 * half).clamp(*Self::SCALE.start(), *Self::SCALE.end())
        })
    }

    /// What a packet says of it (`form`), with the scale the frame was drawn at.
    pub fn packet_json(&self, scale: f32) -> Json {
        let mut j = Json::obj([
            ("view", Json::from(self.view.name())),
            (
                "shows",
                Json::from(match self.view {
                    specula_gpu::FormView::World => "the world as it is",
                    specula_gpu::FormView::Clay => "every lit surface as the same matte clay under a studio light that comes with the camera (key from the upper left): no colour, texture or light of the world's",
                    specula_gpu::FormView::Normals => "each surface's facing as colour: world x, y, z as red, green, blue (128 is 0)",
                    specula_gpu::FormView::Curvature => "how fast each surface turns: mid grey flat, lighter turning away (a ridge), darker turning in (a groove); a ball of scale_m's radius is halfway to white (191), never cut off",
                    specula_gpu::FormView::Stripes => "the level bands a mirror of this shape would show: even on an even surface, kinked where the surface is",
                }),
            ),
            ("not_drawn", Json::from("the world's light, sky, fog, post effects, particles and decals; colours in this image say nothing about materials")),
        ]);
        if self.view == specula_gpu::FormView::Curvature {
            j.set("scale_m", Json::from((f64::from(scale) * 1e4).round() / 1e4));
        }
        j
    }
}

/// `form` and `form_scale` (`sight_frame`, `sight_orbit`).
pub fn form_arg(args: &Json) -> Result<Form, String> {
    let view = match string_arg(args, "form")? {
        None => specula_gpu::FormView::World,
        Some(name) => specula_gpu::FormView::parse(name).ok_or_else(|| format!("'form' must be one of {}", specula_gpu::FormView::NAMES.join(", ")))?,
    };
    let scale = num_in(args, "form_scale", Form::SCALE)?;
    if scale.is_some() && view != specula_gpu::FormView::Curvature {
        return Err("'form_scale' is the curvature view's: give it with form: \"curvature\"".into());
    }
    Ok(Form { view, scale })
}

/// The schema of [`form_arg`]'s two arguments.
pub fn form_schema() -> Vec<(&'static str, Json)> {
    vec![
        (
            "form",
            Json::obj([
                ("type", Json::from("string")),
                ("enum", Json::arr(specula_gpu::FormView::NAMES.map(Json::from))),
                (
                    "description",
                    Json::from(
                        "Show shape alone (default world: the frame as it is). clay: every lit surface the same matte clay \
                         under a studio light that comes with the camera, whatever the world's light; normals: facing as \
                         colour (x, y, z as r, g, b); curvature: grey where flat, lighter on ridges, darker in grooves; \
                         stripes: the level bands a mirror of the shape would show, which kink where the surface does. \
                         Use these to judge form: roundness, faceting, seams, ledges. No sky, fog, post effects, particles \
                         or decals are drawn, and the packet's form says so",
                    ),
                ),
            ]),
        ),
        (
            "form_scale",
            Json::obj([
                ("type", Json::from("number")),
                ("minimum", Json::from(0.001)),
                ("maximum", Json::from(1000.0)),
                (
                    "description",
                    Json::from(
                        "With form: curvature, the radius in metres (0.001..1000) of the ball that shows halfway from mid grey to white (default: a \
                         twentieth of the view's half height at its target). Smaller shows fine detail, larger the big forms",
                    ),
                ),
            ]),
        ),
    ]
}

pub struct OrbitSpec {
    pub target: OrbitTarget,
    pub views: u32,
    pub elevation_deg: f32,
    /// `None`: fit the target's bounds.
    pub distance: Option<f32>,
    pub start_deg: f32,
    pub tile: (u32, u32),
    pub fov_y_deg: f32,
    /// Per view.
    pub max_subjects: usize,
    pub time_s: Option<f32>,
    pub include_ui: bool,
    pub lod: Option<usize>,
    pub overlays: sight_overlay::Spec,
    /// The world, or a form view of it, in every tile.
    pub form: Form,
}

pub enum CompareCall {
    /// Capture "before" through the frame's camera.
    Mark { name: String, frame: Box<FrameSpec> },
    /// Render now through the mark's camera and compare.
    Diff { name: String, threshold: u8, max_subjects: usize },
    List,
    /// `None`: all of yours.
    Delete { name: Option<String> },
}

/// `sight_query`: one entity's on-screen facts, without an image.
pub struct QuerySpec {
    pub key: EntityKey,
    pub frame: FrameSpec,
}

/// Camera and mark names: 1..64 of `A-Z a-z 0-9 _ - .`.
fn label_arg(args: &Json, key: &str, required: bool) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        Some(n) if (1..=64).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)) => Ok(Some(n.to_owned())),
        Some(_) => Err(format!("'{key}' must be 1..64 characters of A-Z a-z 0-9 _ - .")),
        None if required => Err(format!("'{key}' is required")),
        None => Ok(None),
    }
}

/// An agent name (as given to agent_hello).
fn agent_arg(args: &Json, key: &str) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        Some(n) if !n.trim().is_empty() && n.chars().count() <= 64 => Ok(Some(n.trim().to_owned())),
        Some(_) => Err(format!("'{key}' must be an agent name (1..64 characters)")),
        None => Ok(None),
    }
}

fn fov_arg(args: &Json) -> Result<f32, String> {
    Ok(num_in(args, "fov_y_deg", 10.0..=120.0)?.unwrap_or(50.0))
}

/// A named camera excludes `eye` and `target` (either one).
pub fn check_camera_conflict(args: &Json) -> Result<(), String> {
    let given = |k: &str| args.get(k).is_some_and(|v| !v.is_null());
    if given("camera") && (given("eye") || given("target") || given("ortho") || given("view_2d")) {
        return Err("give 'camera' (a named camera, which has its own projection) or 'eye' and 'target' / 'view_2d', not both".into());
    }
    Ok(())
}

/// `sight_frame`'s `camera`, `camera_of`, `overlays`, `overlay_entities`, `max_labels`, `ids` and
/// `subjects` (also used by `sight_compare` and `sight_query`).
pub fn frame_extras(args: &Json, spec: &mut FrameSpec) -> Result<(), String> {
    check_camera_conflict(args)?;
    spec.ids = bool_arg(args, "ids")?.unwrap_or(true);
    spec.camera = match label_arg(args, "camera", false)? {
        Some(name) => {
            Some(CameraRef { name, of: agent_arg(args, "camera_of")? })
        }
        None if args.get("camera_of").is_some_and(|v| !v.is_null()) => {
            return Err("'camera_of' needs 'camera' (the name of that agent's camera)".into())
        }
        None => None,
    };
    spec.overlays = overlays_arg(args)?;
    spec.subjects = match string_arg(args, "subjects")? {
        None | Some("all") => SubjectFilter::All,
        Some("on_screen") => SubjectFilter::OnScreen,
        Some("visible") => SubjectFilter::Visible,
        Some(other) => return Err(format!("'subjects' must be \"all\", \"on_screen\" or \"visible\", not '{other}'")),
    };
    Ok(())
}

fn overlays_arg(args: &Json) -> Result<sight_overlay::Spec, String> {
    let names = || Kind::ALL.map(Kind::name).join(", ");
    let mut kinds = match args.get("overlays").filter(|v| !v.is_null()) {
        None => Vec::new(),
        Some(v) => {
            let list: Option<Vec<&str>> = v.as_array().and_then(|a| a.iter().map(Json::as_str).collect());
            let list = list.ok_or_else(|| format!("'overlays' must be an array of: {}", names()))?;
            list.into_iter()
                .map(|n| Kind::parse(n).ok_or_else(|| format!("'overlays': unknown overlay '{n}' (one of: {})", names())))
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    kinds.sort();
    kinds.dedup();
    let entities = match args.get("overlay_entities").filter(|v| !v.is_null()) {
        None => Vec::new(),
        Some(v) => {
            let list: Option<Vec<&str>> = v.as_array().and_then(|a| a.iter().map(Json::as_str).collect());
            match list {
                Some(l) if l.len() <= 32 && l.iter().all(|p| p.starts_with('/')) => l.into_iter().map(str::to_owned).collect(),
                _ => return Err("'overlay_entities' must be an array of at most 32 paths like \"/district_3\"".into()),
            }
        }
    };
    let (default, max) = sight_overlay::LABELS;
    let max_labels = int_arg(args, "max_labels", 0, max as u64)?.map_or(default, |n| n as usize);
    Ok(sight_overlay::Spec { kinds, entities, max_labels, ui_display: None })
}

/// The sight tools' arguments; `None` when `name` isn't one of them.
pub fn parse(name: &str, args: &Json) -> Option<Result<SightCall, String>> {
    let call = match name {
        "camera_set" => parse_camera_set(args).map(SightCall::CameraSet),
        "camera_list" => check_keys(args, name, &[&["of", "all"]]).and_then(|()| {
            let all = bool_arg(args, "all")?.unwrap_or(false);
            let of = agent_arg(args, "of")?;
            if all && of.is_some() {
                return Err("camera_list: give 'of' or 'all', not both".into());
            }
            Ok(SightCall::CameraList { of, all })
        }),
        "camera_delete" => check_keys(args, name, &[&["name", "all"]]).and_then(|()| {
            let name = label_arg(args, "name", false)?;
            match (name, bool_arg(args, "all")?.unwrap_or(false)) {
                (Some(_), true) => Err("camera_delete: give 'name' or 'all', not both".into()),
                (None, false) => Err("camera_delete needs 'name' (a camera) or 'all': true (every camera of yours)".into()),
                (name, _) => Ok(SightCall::CameraDelete { name }),
            }
        }),
        "sight_orbit" => parse_orbit(args).map(SightCall::Orbit),
        "sight_compare" => parse_compare(args).map(SightCall::Compare),
        "sight_query" => parse_query(args).map(SightCall::Query),
        _ => return None,
    };
    Some(call)
}

pub(crate) fn parse_camera_set(args: &Json) -> Result<CameraSpec, String> {
    check_keys(args, "camera_set", &[&["name", "eye", "target", "fov_y_deg", "follow", "offset", "look_offset", "local", "ortho"]])?;
    let name = label_arg(args, "name", true)?.expect("required");
    let fov_y_deg = fov_arg(args)?;
    let ortho = match args.get("ortho").filter(|v| !v.is_null()) {
        None => None,
        Some(o) => {
            check_keys(o, "'ortho'", &[&["height_m"]])?;
            Some(num_in(o, "height_m", 0.01..=1.0e5)?.ok_or("'ortho' needs 'height_m': metres of the view's height")?)
        }
    };
    let (eye, target) = (position_arg(args, "eye")?, position_arg(args, "target")?);
    let offset = position_arg(args, "offset")?;
    let look_offset = position_arg(args, "look_offset")?;
    let local = bool_arg(args, "local")?;
    match string_arg(args, "follow")? {
        Some(path) => {
            if eye.is_some() || target.is_some() {
                return Err("a follow camera takes 'offset' and 'look_offset' (from the entity), not 'eye'/'target'".into());
            }
            if !path.starts_with('/') {
                return Err("'follow' must be an entity path like \"/wave/runner_3\"".into());
            }
            let follow = Follow {
                path: path.to_owned(),
                offset: offset.unwrap_or(Follow::DEFAULT_OFFSET),
                look_offset: look_offset.unwrap_or(Vec3::ZERO),
                local: local.unwrap_or(false),
            };
            if (follow.offset - follow.look_offset).length() < 1e-4 {
                return Err("'offset' and 'look_offset' must differ (the camera would look at itself)".into());
            }
            Ok(CameraSpec { name, eye: None, target: None, fov_y_deg, follow: Some(follow), ortho })
        }
        None => {
            if offset.is_some() || look_offset.is_some() || local.is_some() {
                return Err("'offset', 'look_offset' and 'local' are for follow cameras (give 'follow')".into());
            }
            match (eye, target) {
                (Some(e), Some(t)) if (t - e).length() >= 1e-4 => Ok(CameraSpec { name, eye, target, fov_y_deg, follow: None, ortho }),
                (Some(_), Some(_)) => Err("'eye' and 'target' must be different points".into()),
                _ => Err("camera_set needs 'eye' and 'target', or 'follow' (an entity path)".into()),
            }
        }
    }
}

const ORBIT_KEYS: &[&str] = &[
    "target", "point", "views", "elevation_deg", "distance", "start_deg", "tile_width", "tile_height", "fov_y_deg",
    "max_subjects", "time_s", "include_ui", "lod", "overlays", "overlay_entities", "max_labels", "form", "form_scale",
];

fn parse_orbit(args: &Json) -> Result<OrbitSpec, String> {
    check_keys(args, "sight_orbit", &[ORBIT_KEYS])?;
    let target = match (args.get("target").filter(|v| !v.is_null()), position_arg(args, "point")?) {
        (Some(_), Some(_)) => return Err("sight_orbit: give 'target' (an entity) or 'point', not both".into()),
        (Some(t), None) => OrbitTarget::Entity(key_value(t, "target")?),
        (None, Some(p)) => OrbitTarget::Point(p),
        (None, None) => return Err("sight_orbit needs 'target' (an entity path, id or unique name) or 'point' ([x, y, z])".into()),
    };
    let (lo, hi) = (*orbit::VIEWS.start(), *orbit::VIEWS.end());
    let views = int_arg(args, "views", u64::from(lo), u64::from(hi))?.map_or(8, |n| n as u32);
    let size = |key: &str, default: u32| -> Result<u32, String> { Ok(int_arg(args, key, 64, 1024)?.map_or(default, |n| n as u32)) };
    Ok(OrbitSpec {
        target,
        views,
        elevation_deg: num_in(args, "elevation_deg", -89.0..=89.0)?.unwrap_or(25.0),
        distance: num_in(args, "distance", 0.05..=100_000.0)?,
        start_deg: num_in(args, "start_deg", -360.0..=360.0)?.unwrap_or(0.0),
        tile: (size("tile_width", 320)?, size("tile_height", 240)?),
        fov_y_deg: fov_arg(args)?,
        max_subjects: int_arg(args, "max_subjects", 0, 20)?.map_or(8, |n| n as usize),
        time_s: num_in(args, "time_s", 0.0..=1.0e6)?,
        include_ui: bool_arg(args, "include_ui")?.unwrap_or(false),
        lod: int_arg(args, "lod", 0, 7)?.map(|l| l as usize),
        overlays: overlays_arg(args)?,
        form: form_arg(args)?,
    })
}

fn parse_compare(args: &Json) -> Result<CompareCall, String> {
    let action = string_arg(args, "action")?.unwrap_or("");
    match action {
        "mark" => {
            // A mark takes sight_frame's view arguments.
            let frame_keys: &[&str] = &["width", "height", "eye", "target", "fov_y_deg", "time_s", "include_ui", "lod"];
            check_keys(args, "sight_compare mark", &[&["action", "name"], frame_keys, FRAME_EXTRA_KEYS])?;
            let name = label_arg(args, "name", true)?.expect("required");
            let frame = parse_frame(args)?;
            Ok(CompareCall::Mark { name, frame: Box::new(frame) })
        }
        "diff" => {
            check_keys(args, "sight_compare diff", &[&["action", "name", "threshold", "max_subjects"]])?;
            Ok(CompareCall::Diff {
                name: label_arg(args, "name", true)?.expect("required"),
                threshold: int_arg(args, "threshold", 0, 254)?.map_or(16, |t| t as u8),
                max_subjects: int_arg(args, "max_subjects", 0, 50)?.map_or(25, |n| n as usize),
            })
        }
        "list" => check_keys(args, "sight_compare list", &[&["action"]]).map(|()| CompareCall::List),
        "delete" => {
            check_keys(args, "sight_compare delete", &[&["action", "name", "all"]])?;
            match (label_arg(args, "name", false)?, bool_arg(args, "all")?.unwrap_or(false)) {
                (Some(_), true) => Err("sight_compare delete: give 'name' or 'all', not both".into()),
                (None, false) => Err("sight_compare delete needs 'name' or 'all': true".into()),
                (name, _) => Ok(CompareCall::Delete { name }),
            }
        }
        "" => Err("'action' is required: mark, diff, list or delete".into()),
        other => Err(format!("'action' must be mark, diff, list or delete, not '{other}'")),
    }
}

fn parse_query(args: &Json) -> Result<QuerySpec, String> {
    let view: &[&str] = &["width", "height", "eye", "target", "fov_y_deg", "time_s", "include_ui", "lod", "camera", "camera_of"];
    check_keys(args, "sight_query", &[&["id", "path", "name"], view])?;
    let key = entity_key(args, "sight_query")?;
    // sight_query's 'name' is the entity's; a camera is named with 'camera'.
    let mut frame = parse_frame(&strip(args, &["id", "path", "name"]))?;
    frame.max_subjects = 1;
    Ok(QuerySpec { key, frame })
}

/// `args` without `keys`.
fn strip(args: &Json, keys: &[&str]) -> Json {
    match args {
        Json::Obj(pairs) => Json::Obj(pairs.iter().filter(|(k, _)| !keys.contains(&k.as_str())).cloned().collect()),
        other => other.clone(),
    }
}

/// `sight_frame`'s extra schema properties.
pub fn frame_extra_schema() -> Vec<(&'static str, Json)> {
    let described = |t: &str, d: &str| Json::obj([("type", Json::from(t)), ("description", Json::from(d))]);
    vec![
        ("camera", described("string", "Look through your named camera (camera_set) instead of eye/target")),
        ("camera_of", described("string", "With camera: look through this agent's camera of that name instead (read-only)")),
        (
            "overlays",
            Json::obj([
                ("type", Json::from("array")),
                ("items", Json::obj([("type", Json::from("string")), ("enum", Json::arr(Kind::ALL.map(|k| Json::from(k.name()))))])),
                (
                    "description",
                    Json::from(
                        "Debug overlays drawn on top of the image (x-ray: they ignore occlusion): grid (ground grid at y=0, \
                         metre labels), axes (world axes at the origin; also each overlay_entities root's own axes), \
                         bounds (oriented boxes of on-screen subjects: cyan visible, red fully hidden), labels (entity \
                         names at their screen centres, no overlaps, max_labels), claims (claimed regions and subtrees \
                         tinted per owner, with owner names), skeletons (bones and joints of rigged models, rest pose), \
                         lods (each model's LOD level and triangles), colliders (physics collider wireframes: grey \
                         static, yellow kinematic, green awake, blue asleep, magenta triggers), contacts (physics contact \
                         points with their normals: red penetrating, orange touching, green speculative), joints (physics \
                         joints: anchors, yellow axes, hinge limit arcs; teal holding, orange strained, red past a limit \
                         or apart, grey inactive), effects (orange boxes around each effect's particles and violet decal \
                         boxes, labelled; particles and decals never count in the ID buffer). The packet's 'overlays' \
                         says what was drawn.",
                    ),
                ),
            ]),
        ),
        (
            "overlay_entities",
            Json::obj([
                ("type", Json::from("array")),
                ("items", Json::obj([("type", Json::from("string"))])),
                ("description", Json::from("Only these subtrees get boxes, labels, axes, skeletons and LOD tags (paths; default all on screen)")),
            ]),
        ),
        ("max_labels", described("integer", "Most entity labels, 0..64 (default 32)")),
        (
            "ids",
            described(
                "boolean",
                "Render the ID buffer for visible_px / screen_box_exact / no_pixels / occluded (default true; \
                 false saves its GPU pass and readback)",
            ),
        ),
        (
            "subjects",
            described(
                "string",
                "Which subjects the response lists: all (default), on_screen, or visible (at least one pixel drawn, \
                 by the ID buffer)",
            ),
        ),
        (
            "gi",
            described(
                "string",
                "Baked GI in this capture: lit (default, as the window draws it), off (the flat ambient: before/after \
                 pairs), irradiance (the indirect light alone) or probes (that, with the probes as small spheres). \
                 The packet's gi lists the volumes and whether they are stale (see gi_info)",
            ),
        ),
    ]
}

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([
            ("type", Json::from("object")),
            ("properties", Json::obj(props)),
            ("required", Json::arr(required.iter().map(|r| Json::from(*r)))),
        ])
    };
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let vec3 = |d: &str| {
        Json::obj([
            ("type", Json::from("array")),
            ("items", Json::obj([("type", Json::from("number"))])),
            ("minItems", Json::from(3)),
            ("maxItems", Json::from(3)),
            ("description", Json::from(d)),
        ])
    };
    let overlay_props = || {
        frame_extra_schema().into_iter().filter(|(k, _)| matches!(*k, "overlays" | "overlay_entities" | "max_labels")).collect::<Vec<_>>()
    };
    let view_props = || {
        vec![
            ("width", t("integer", "Image width 64..2048 (default 960)")),
            ("height", t("integer", "Image height 64..2048 (default 540)")),
            ("eye", vec3("Camera position (give with target)")),
            ("target", vec3("Point the camera looks at")),
            ("fov_y_deg", t("number", "Vertical field of view, 10..120 (default 50)")),
            ("camera", t("string", "Look through your named camera instead of eye/target")),
            ("camera_of", t("string", "With camera: that agent's camera of that name")),
            ("time_s", t("number", "Animation time in seconds (default: now)")),
            ("include_ui", t("boolean", "Also draw ui-layer entities and a hosted game's interface (its opaque panels hide what is under them)")),
            ("lod", t("integer", "Draw every model at this LOD")),
        ]
    };
    vec![
        ToolDef {
            name: "camera_set".into(),
            description: "Create or replace one of your named cameras (at most 32): fixed (eye + target) or following \
                          an entity (follow: path; eye = its origin + offset, looking at its origin + look_offset; \
                          local: offsets turn with the entity). Use it with sight_frame {camera}, sight_compare and \
                          sight_query, and show it to a human with window_view {camera}; other agents may look \
                          through it (camera_of) but only you change it. Cameras belong to your agent name and are \
                          saved with the world (world_save / --world autosave; back after world_load or a restart \
                          for whoever holds the name), journaled (kind camera: world_changes shows them, world_undo \
                          reverts them), and never part of the world hash."
                .into(),
            input_schema: schema(
                vec![
                    ("name", t("string", "Camera name, 1..64 of A-Z a-z 0-9 _ - .")),
                    ("eye", vec3("Camera position (fixed camera)")),
                    ("target", vec3("Point it looks at (fixed camera)")),
                    ("fov_y_deg", t("number", "Vertical field of view, 10..120 (default 50)")),
                    ("follow", t("string", "Entity path to follow, e.g. \"/wave/runner_3\"")),
                    ("offset", vec3("Follow: eye offset from the entity's origin (default [0, 4, 6])")),
                    ("look_offset", vec3("Follow: look-at offset from the entity's origin (default [0, 0, 0])")),
                    ("local", t("boolean", "Follow: offsets in the entity's own axes (turn with it); default false (world axes)")),
                    ("ortho", t("object", "{height_m}: orthographic, this many metres tall (a 2D game's camera); default perspective")),
                ],
                &["name"],
            ),
        },
        ToolDef {
            name: "camera_list".into(),
            description: "Your named cameras (or another agent's with of, or every agent's camera names with all), \
                          each in camera_set's form plus its view right now."
                .into(),
            input_schema: schema(
                vec![("of", t("string", "Another agent's name")), ("all", t("boolean", "Every agent's camera names"))],
                &[],
            ),
        },
        ToolDef {
            name: "camera_delete".into(),
            description: "Delete one of your cameras (name), or all of them (all: true); journaled (undo brings it back).".into(),
            input_schema: schema(vec![("name", t("string", "Camera name")), ("all", t("boolean", "Every camera of yours"))], &[]),
        },
        ToolDef {
            name: "sight_orbit".into(),
            description: "A turntable: 4..12 views around an entity (or a point), evenly spaced in azimuth at one \
                          elevation and distance (default: fit its bounds), returned as ONE contact-sheet PNG (tiles \
                          row by row, each captioned with its azimuth) plus each view's camera, tile rectangle and \
                          biggest on-screen subjects. Azimuth 0 looks from +Z (south), 90 from +X (east). Cheaper \
                          than N sight_frame calls. The sheet is at most 2048 px a side."
                .into(),
            input_schema: schema(
                [
                    vec![
                        ("target", t("string", "Entity to orbit: path, id or unique name (its bounds' centre)")),
                        ("point", vec3("Or a world point to orbit")),
                        ("views", t("integer", "Number of views, 4..12 (default 8)")),
                        ("elevation_deg", t("number", "Camera angle above the horizontal, -89..89 (default 25)")),
                        ("distance", t("number", "Metres from the centre (default: fits the target's bounds)")),
                        ("start_deg", t("number", "Azimuth of the first view (default 0 = from +Z)")),
                        ("tile_width", t("integer", "Tile width 64..1024 (default 320)")),
                        ("tile_height", t("integer", "Tile height 64..1024 (default 240)")),
                        ("fov_y_deg", t("number", "Vertical field of view, 10..120 (default 50)")),
                        ("max_subjects", t("integer", "Subjects listed per view, 0..20 (default 8)")),
                        ("time_s", t("number", "Animation time (default: now)")),
                        ("include_ui", t("boolean", "Also draw ui-layer entities and a hosted game's interface (its opaque panels hide what is under them)")),
                        ("lod", t("integer", "Draw every model at this LOD")),
                    ],
                    overlay_props(),
                    form_schema(),
                ]
                .concat(),
                &[],
            ),
        },
        ToolDef {
            name: "sight_compare".into(),
            description: "Before/after. action mark {name, + sight_frame's view arguments or camera}: capture and \
                          remember a \"before\". action diff {name, threshold}: render the world now through the \
                          mark's exact camera and settings and return one image (before | after | heat; heat shows \
                          changed pixels from red to white over the after image in grey) with numbers: changed_pct, \
                          mean_diff and max_diff (0..255), boxes of changed regions, and which subjects were added, \
                          removed, moved or changed visibility. list and delete manage your marks (at most 8, least \
                          recently used dropped first)."
                .into(),
            input_schema: schema(
                [
                    vec![
                        (
                            "action",
                            Json::obj([("type", Json::from("string")), ("enum", Json::arr(["mark", "diff", "list", "delete"].map(Json::from)))]),
                        ),
                        ("name", t("string", "Mark name, 1..64 of A-Z a-z 0-9 _ - .")),
                        ("threshold", t("integer", "diff: a pixel changed when a channel differs by more than this, 0..254 (default 16)")),
                        ("max_subjects", t("integer", "diff: subject changes listed per kind, 0..50 (default 25)")),
                        ("all", t("boolean", "delete: every mark of yours")),
                    ],
                    view_props(),
                    overlay_props(),
                ]
                .concat(),
                &["action"],
            ),
        },
        ToolDef {
            name: "sight_query".into(),
            description: "One entity's on-screen facts through a camera, without an image (cheap for checks): \
                          on_screen, screen (projected box), distance_m and, from the ID buffer, visible_px, \
                          screen_box_exact, no_pixels (in view, nothing shows) and occluded (hidden by something in front); \
                          visible_px_approx: true when visible_px and screen_box_exact are lower bounds (more than \
                          4 blended entities overlapped at some of its pixels)."
                .into(),
            input_schema: schema(
                [
                    vec![
                        ("id", t("integer", "Entity id")),
                        ("path", t("string", "Entity path")),
                        ("name", t("string", "Entity name (unique in the world)")),
                    ],
                    view_props(),
                ]
                .concat(),
                &[],
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Json {
        Json::parse(text).unwrap()
    }

    fn call(name: &str, a: &str) -> Result<SightCall, String> {
        parse(name, &args(a)).expect("a sight tool")
    }

    /// `form` and `form_scale`: sight_frame's and sight_orbit's, bounded, and refused elsewhere.
    #[test]
    fn form_arguments() {
        use specula_gpu::FormView;
        let frame = |a: &str| crate::tools::parse("sight_frame", &args(a)).map(|c| match c {
            crate::tools::Command::Frame(f) => f.form,
            _ => panic!("a frame"),
        });
        assert_eq!(frame("{}").unwrap(), Form::default());
        assert!(Form::default().is_world());
        assert_eq!(frame(r#"{"form":"clay"}"#).unwrap(), Form { view: FormView::Clay, scale: None });
        assert_eq!(frame(r#"{"form":"world"}"#).unwrap(), Form { view: FormView::World, scale: None });
        assert_eq!(frame(r#"{"form":"curvature","form_scale":0.2}"#).unwrap(), Form { view: FormView::Curvature, scale: Some(0.2) });
        for bad in [r#"{"form":"wireframe"}"#, r#"{"form":3}"#, r#"{"form":"clay","form_scale":0.2}"#, r#"{"form_scale":0.2}"#, r#"{"form":"curvature","form_scale":0}"#, r#"{"form":"curvature","form_scale":5000}"#] {
            assert!(frame(bad).is_err(), "{bad}");
        }
        let SightCall::Orbit(o) = call("sight_orbit", r#"{"point":[0,0,0],"form":"stripes"}"#).unwrap() else { panic!("an orbit") };
        assert_eq!(o.form.view, FormView::Stripes);
        assert!(call("sight_orbit", r#"{"point":[0,0,0],"form":"mesh"}"#).is_err());
        // The other tools that take a frame's view do not take a form view, and say so as an unknown argument.
        assert!(call("sight_compare", r#"{"action":"mark","name":"a","form":"clay"}"#).is_err());
        assert!(call("sight_query", r#"{"name":"a","form":"clay"}"#).is_err());
        // Its scale without one given: a twentieth of the view's half height where the camera looks.
        let cam = crate::sight::Camera::new(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, 90.0);
        let auto = Form { view: FormView::Curvature, scale: None }.scale_for(&cam);
        assert!((auto - 0.5).abs() < 1e-4, "{auto}");
        let ortho = crate::sight::Camera { projection: crate::sight::Projection::Ortho { height_m: 2.0 }, ..cam };
        assert!((Form::default().scale_for(&ortho) - 0.05).abs() < 1e-6);
        assert_eq!(Form { view: FormView::Curvature, scale: Some(3.0) }.scale_for(&cam), 3.0);
        // The schema lists every view and bounds the scale.
        let schema = form_schema();
        let names = schema[0].1.get("enum").and_then(Json::as_array).unwrap();
        assert_eq!(names.len(), FormView::NAMES.len());
        assert!(schema[1].1.get("minimum").is_some() && schema[1].1.get("maximum").is_some());
    }

    #[test]
    fn camera_arguments() {
        let SightCall::CameraSet(c) = call("camera_set", r#"{"name":"top","eye":[0,20,0],"target":[0,0,0.1]}"#).unwrap() else { panic!() };
        assert_eq!((c.name.as_str(), c.fov_y_deg, c.follow.is_none()), ("top", 50.0, true));
        let SightCall::CameraSet(c) = call("camera_set", r#"{"name":"chase","follow":"/wave/r1","local":true}"#).unwrap() else { panic!() };
        let f = c.follow.unwrap();
        assert_eq!((f.path.as_str(), f.offset, f.local), ("/wave/r1", Follow::DEFAULT_OFFSET, true));
        for bad in [
            r#"{"eye":[0,1,0],"target":[0,0,0]}"#,
            r#"{"name":"a b","eye":[0,1,0],"target":[0,0,0]}"#,
            r#"{"name":"x","eye":[0,1,0]}"#,
            r#"{"name":"x","eye":[0,1,0],"target":[0,1,0]}"#,
            r#"{"name":"x","follow":"/a","eye":[0,1,0]}"#,
            r#"{"name":"x","follow":"a"}"#,
            r#"{"name":"x","eye":[0,1,0],"target":[0,0,0],"offset":[1,1,1]}"#,
            r#"{"name":"x","follow":"/a","offset":[0,0,0]}"#,
            r#"{"name":"x","eye":[0,1,0],"target":[0,0,0],"fov_y_deg":200}"#,
        ] {
            assert!(call("camera_set", bad).is_err(), "{bad}");
        }
        assert!(matches!(call("camera_list", r#"{"of":"ann"}"#), Ok(SightCall::CameraList { of: Some(_), all: false })));
        assert!(call("camera_list", r#"{"of":"ann","all":true}"#).is_err());
        assert!(matches!(call("camera_delete", r#"{"all":true}"#), Ok(SightCall::CameraDelete { name: None })));
        assert!(call("camera_delete", "{}").is_err());
        assert!(parse("world_spawn", &args("{}")).is_none(), "not a sight tool");
    }

    #[test]
    fn frame_extras_parse() {
        let f = parse_frame(&args(r#"{"camera":"top","overlays":["labels","grid","grid"],"subjects":"visible","max_labels":5}"#)).unwrap();
        assert_eq!(f.camera, Some(CameraRef { name: "top".into(), of: None }));
        assert_eq!(f.overlays.kinds, vec![Kind::Grid, Kind::Labels], "sorted, no repeats");
        assert_eq!((f.overlays.max_labels, f.subjects), (5, SubjectFilter::Visible));
        assert_eq!(parse_frame(&args("{}")).unwrap().overlays.max_labels, sight_overlay::LABELS.0);
        for bad in [
            r#"{"camera":"top","eye":[0,1,0],"target":[0,0,0]}"#,
            r#"{"camera":"top","target":[0,0,0]}"#,
            r#"{"camera":"top","eye":[0,0,1]}"#,
            r#"{"ids":"no"}"#,
            r#"{"camera_of":"ann"}"#,
            r#"{"overlays":["heatmap"]}"#,
            r#"{"overlays":"grid"}"#,
            r#"{"overlay_entities":["district"]}"#,
            r#"{"subjects":"some"}"#,
            r#"{"max_labels":65}"#,
        ] {
            assert!(parse_frame(&args(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn orbit_compare_and_query_arguments() {
        let SightCall::Orbit(o) = call("sight_orbit", r#"{"target":"/tower"}"#).unwrap() else { panic!() };
        assert!(matches!(o.target, OrbitTarget::Entity(EntityKey::Path(_))));
        assert_eq!((o.views, o.elevation_deg, o.tile, o.max_subjects), (8, 25.0, (320, 240), 8));
        assert!(matches!(call("sight_orbit", r#"{"point":[1,2,3],"views":12}"#), Ok(SightCall::Orbit(_))));
        for bad in [r#"{}"#, r#"{"target":"/a","point":[0,0,0]}"#, r#"{"target":"/a","views":3}"#, r#"{"target":"/a","views":13}"#, r#"{"target":"/a","elevation_deg":90}"#, r#"{"target":"/a","tile_width":2000}"#] {
            assert!(call("sight_orbit", bad).is_err(), "{bad}");
        }
        let SightCall::Compare(CompareCall::Mark { name, frame }) = call("sight_compare", r#"{"action":"mark","name":"before","camera":"top"}"#).unwrap() else { panic!() };
        assert_eq!((name.as_str(), frame.camera.map(|c| c.name)), ("before", Some("top".into())));
        assert!(matches!(call("sight_compare", r#"{"action":"diff","name":"before"}"#), Ok(SightCall::Compare(CompareCall::Diff { threshold: 16, .. }))));
        for bad in [r#"{}"#, r#"{"action":"diff"}"#, r#"{"action":"diff","name":"x","eye":[0,0,1]}"#, r#"{"action":"undo","name":"x"}"#, r#"{"action":"delete"}"#] {
            assert!(call("sight_compare", bad).is_err(), "{bad}");
        }
        let SightCall::Query(q) = call("sight_query", r#"{"path":"/box","camera":"top"}"#).unwrap() else { panic!() };
        assert!(matches!(q.key, EntityKey::Path(_)) && q.frame.camera.is_some());
        assert!(matches!(call("sight_query", r#"{"name":"box"}"#), Ok(SightCall::Query(_))));
        assert!(call("sight_query", r#"{}"#).is_err());
        assert!(call("sight_query", r#"{"path":"/a","overlays":["grid"]}"#).is_err());
    }

    #[test]
    fn definitions_are_well_formed() {
        for d in definitions() {
            assert!(d.name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') && d.name.len() <= 64);
            assert!(d.input_schema.get("properties").is_some());
        }
    }
}

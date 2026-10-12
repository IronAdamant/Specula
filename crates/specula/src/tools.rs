//! MCP tools agents use to drive the engine.
//!
//! Tool calls arrive on protocol threads; each is parsed and validated here, sent to the main
//! thread as a [`Command`], and answered with the main thread's [`ToolOutput`]. The world is only
//! ever touched by the main thread, between frames.

use crate::world::{EntityKey, Shape, SpawnSpec, UpdateSpec};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolDef, ToolHandler, ToolOutput};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

pub const INSTRUCTIONS: &str = "Specula is a live game engine you build in. Add things with world_spawn, \
change them with world_set, inspect with world_list, and LOOK with sight_frame: it returns the rendered image \
plus a subject list giving each entity's on-screen rectangle (its projected bounding box, computed by the \
engine). Coordinates: metres, right-handed, +Y up; a top-down view puts north (-Z) at the top. Other agents may \
be building in the same world at the same time; each entity records who created it.";

/// How long a tool call waits for the main thread before giving up.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
/// The engine discards a command this long after it was sent: comfortably before the caller's
/// timeout, so a call reported as timed out has definitely not been applied.
const APPLY_DEADLINE: Duration = Duration::from_secs(25);

/// Largest accepted |coordinate| (1000 km): keeps every transform and camera finite.
const MAX_COORD: f32 = 1.0e6;
const MIN_SCALE: f32 = 1.0e-3;
const MAX_SCALE: f32 = 1.0e5;
const MAX_NAME_CHARS: usize = 128;

pub enum Command {
    Spawn(SpawnSpec),
    Set(EntityKey, UpdateSpec),
    List,
    Delete(EntityKey),
    Frame(FrameSpec),
    Info,
}

pub struct FrameSpec {
    pub width: u32,
    pub height: u32,
    /// Explicit camera; `None` frames the whole world automatically.
    pub eye: Option<Vec3>,
    pub target: Option<Vec3>,
    pub fov_y_deg: f32,
    /// Subjects returned in the response (the saved packet always has all of them).
    pub max_subjects: usize,
}

pub struct Request {
    pub session: SessionInfo,
    pub command: Command,
    pub reply: Sender<ToolOutput>,
    /// Past this instant the caller has given up; the engine drops the command unapplied.
    pub deadline: Instant,
}

pub struct Tools {
    to_engine: Sender<Request>,
}

impl Tools {
    pub fn new(to_engine: Sender<Request>) -> Tools {
        Tools { to_engine }
    }
}

impl ToolHandler for Tools {
    fn tools(&self) -> Vec<ToolDef> {
        definitions()
    }

    fn call(&self, session: &SessionInfo, name: &str, arguments: &Json) -> ToolOutput {
        let command = match parse(name, arguments) {
            Ok(c) => c,
            Err(e) => return ToolOutput::error(e),
        };
        let (reply, answer) = mpsc::channel();
        let deadline = Instant::now() + APPLY_DEADLINE;
        let request = Request { session: session.clone(), command, reply, deadline };
        if self.to_engine.send(request).is_err() {
            return ToolOutput::error("the engine is shutting down");
        }
        answer.recv_timeout(REPLY_TIMEOUT).unwrap_or_else(|e| match e {
            mpsc::RecvTimeoutError::Timeout => {
                ToolOutput::error("the engine was too busy to answer within 30 s; the command was not applied")
            }
            mpsc::RecvTimeoutError::Disconnected => {
                ToolOutput::error("the engine is shutting down or skipped the command; it was not applied")
            }
        })
    }
}

fn parse(name: &str, args: &Json) -> Result<Command, String> {
    match name {
        "world_spawn" => parse_spawn(args).map(Command::Spawn),
        "world_set" => parse_set(args),
        "world_list" => Ok(Command::List),
        "world_delete" => entity_key(args, "world_delete").map(Command::Delete),
        "sight_frame" => parse_frame(args).map(Command::Frame),
        "engine_info" => Ok(Command::Info),
        other => Err(format!("unknown tool '{other}'")),
    }
}

/// `id` (integer) or `name` (string) identifying an existing entity.
fn entity_key(args: &Json, tool: &str) -> Result<EntityKey, String> {
    let present = |k: &str| args.get(k).is_some_and(|v| !v.is_null());
    if present("id") && present("name") {
        return Err(format!("{tool}: give 'id' or 'name', not both (to rename an entity use 'rename')"));
    }
    match (args.get("id"), args.get("name")) {
        (Some(id), _) if !id.is_null() => {
            id.as_u64().map(EntityKey::Id).ok_or_else(|| "'id' must be a non-negative integer".into())
        }
        (_, Some(Json::Str(n))) => Ok(EntityKey::Name(n.clone())),
        (_, Some(n)) if !n.is_null() => Err("'name' must be a string".into()),
        _ => Err(format!("{tool} needs 'id' (integer) or 'name' (string)")),
    }
}

fn parse_spawn(args: &Json) -> Result<SpawnSpec, String> {
    let shape_name = match args.get("shape") {
        None | Some(Json::Null) => return Err("'shape' is required".into()),
        Some(Json::Str(s)) => s,
        Some(_) => return Err("'shape' must be a string: cube, quad or sphere".into()),
    };
    let shape = Shape::parse(shape_name).ok_or_else(|| format!("unknown shape '{shape_name}' (cube, quad or sphere)"))?;
    Ok(SpawnSpec {
        shape,
        name: name_arg(args.get("name"))?,
        position: position_arg(args, "position")?.unwrap_or(Vec3::ZERO),
        rotation_deg: vec3_arg(args, "rotation_deg")?.unwrap_or(Vec3::ZERO),
        scale: scale_arg(args)?.unwrap_or(Vec3::ONE),
        color: color_arg(args.get("color"))?.unwrap_or([0.8, 0.8, 0.8]),
    })
}

fn parse_set(args: &Json) -> Result<Command, String> {
    let key = entity_key(args, "world_set")?;
    let change = UpdateSpec {
        name: name_arg(args.get("rename"))?,
        position: position_arg(args, "position")?,
        rotation_deg: vec3_arg(args, "rotation_deg")?,
        scale: scale_arg(args)?,
        color: color_arg(args.get("color"))?,
    };
    let nothing = change.name.is_none()
        && change.position.is_none()
        && change.rotation_deg.is_none()
        && change.scale.is_none()
        && change.color.is_none();
    if nothing {
        return Err("world_set needs at least one of: rename, position, rotation_deg, scale, color".into());
    }
    Ok(Command::Set(key, change))
}

fn parse_frame(args: &Json) -> Result<FrameSpec, String> {
    let int = |key: &str, default: u32, range: std::ops::RangeInclusive<u32>| -> Result<u32, String> {
        match args.get(key) {
            None | Some(Json::Null) => Ok(default),
            Some(v) => match v.as_u64() {
                Some(n) if n <= u64::from(*range.end()) && range.contains(&(n as u32)) => Ok(n as u32),
                _ => Err(format!("'{key}' must be an integer from {} to {}", range.start(), range.end())),
            },
        }
    };
    let fov_y_deg = match args.get("fov_y_deg") {
        None | Some(Json::Null) => 50.0,
        Some(v) => match v.as_f64() {
            Some(f) if (10.0..=120.0).contains(&f) => f as f32,
            _ => return Err("'fov_y_deg' must be a number from 10 to 120".into()),
        },
    };
    let eye = position_arg(args, "eye")?;
    let target = position_arg(args, "target")?;
    match (eye, target) {
        (Some(_), None) | (None, Some(_)) => {
            return Err("give both 'eye' and 'target', or neither (auto-framing)".into());
        }
        (Some(e), Some(t)) if (t - e).length() < 1e-4 => {
            return Err("'eye' and 'target' must be different points".into());
        }
        _ => {}
    }
    Ok(FrameSpec {
        width: int("width", 960, 64..=2048)?,
        height: int("height", 540, 64..=2048)?,
        eye,
        target,
        fov_y_deg,
        max_subjects: int("max_subjects", 50, 1..=500)? as usize,
    })
}

fn name_arg(v: Option<&Json>) -> Result<Option<String>, String> {
    match v {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) => {
            let n = s.trim();
            if n.is_empty() {
                Err("names must not be empty".into())
            } else if n.chars().count() > MAX_NAME_CHARS {
                Err(format!("names must be at most {MAX_NAME_CHARS} characters"))
            } else if n.chars().any(char::is_control) {
                Err("names must not contain control characters".into())
            } else {
                Ok(Some(n.to_owned()))
            }
        }
        Some(_) => Err("names must be strings".into()),
    }
}

/// `[x, y, z]` of finite numbers, or `None` when absent / null.
fn vec3_arg(args: &Json, key: &str) -> Result<Option<Vec3>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => {
            let nums: Option<Vec<f64>> = v.as_array().and_then(|a| a.iter().map(Json::as_f64).collect());
            match nums.as_deref() {
                Some(&[x, y, z]) if [x, y, z].iter().all(|c| c.is_finite() && c.abs() <= f32::MAX as f64) => {
                    Ok(Some(Vec3::new(x as f32, y as f32, z as f32)))
                }
                _ => Err(format!("'{key}' must be an array of 3 finite numbers")),
            }
        }
    }
}

/// A point in the world: within ±[`MAX_COORD`] metres on every axis.
fn position_arg(args: &Json, key: &str) -> Result<Option<Vec3>, String> {
    let p = vec3_arg(args, key)?;
    if let Some(p) = p {
        if [p.x, p.y, p.z].iter().any(|c| c.abs() > MAX_COORD) {
            return Err(format!("'{key}' components must be within ±{MAX_COORD} m"));
        }
    }
    Ok(p)
}

/// Number (uniform) or `[x, y, z]`, each within [`MIN_SCALE`]..=[`MAX_SCALE`].
fn scale_arg(args: &Json) -> Result<Option<Vec3>, String> {
    let scale = match args.get("scale") {
        Some(Json::Num(s)) => Some(Vec3::new(*s as f32, *s as f32, *s as f32)),
        _ => vec3_arg(args, "scale")?,
    };
    if let Some(s) = scale {
        if [s.x, s.y, s.z].iter().any(|c| !(MIN_SCALE..=MAX_SCALE).contains(c)) {
            return Err(format!("'scale' components must be between {MIN_SCALE} and {MAX_SCALE}"));
        }
    }
    Ok(scale)
}

/// `"#rrggbb"`, `"#rgb"`, or `[r, g, b]` with components in 0..1 (sRGB).
fn color_arg(v: Option<&Json>) -> Result<Option<[f32; 3]>, String> {
    const ERR: &str = "'color' must be \"#rrggbb\", \"#rgb\" or [r, g, b] with components 0..1";
    match v {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) => {
            let hex = s.strip_prefix('#').ok_or(ERR)?;
            let digits: Vec<u32> = hex.chars().map(|c| c.to_digit(16)).collect::<Option<_>>().ok_or(ERR)?;
            let channel = |hi: u32, lo: u32| (hi * 16 + lo) as f32 / 255.0;
            match *digits.as_slice() {
                [r, g, b] => Ok(Some([channel(r, r), channel(g, g), channel(b, b)])),
                [r1, r2, g1, g2, b1, b2] => Ok(Some([channel(r1, r2), channel(g1, g2), channel(b1, b2)])),
                _ => Err(ERR.into()),
            }
        }
        Some(Json::Arr(a)) => match a.iter().map(Json::as_f64).collect::<Option<Vec<_>>>().as_deref() {
            Some(&[r, g, b]) if [r, g, b].iter().all(|c| (0.0..=1.0).contains(c)) => {
                Ok(Some([r as f32, g as f32, b as f32]))
            }
            _ => Err(ERR.into()),
        },
        Some(_) => Err(ERR.into()),
    }
}

fn definitions() -> Vec<ToolDef> {
    let vec3 = |desc: &str| {
        Json::obj([
            ("type", Json::from("array")),
            ("items", Json::obj([("type", Json::from("number"))])),
            ("minItems", Json::from(3)),
            ("maxItems", Json::from(3)),
            ("description", Json::from(desc)),
        ])
    };
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([
            ("type", Json::from("object")),
            ("properties", Json::obj(props)),
            ("required", Json::arr(required.iter().map(|r| Json::from(*r)))),
        ])
    };
    let string = |desc: &str| Json::obj([("type", Json::from("string")), ("description", Json::from(desc))]);
    let integer = |desc: &str| Json::obj([("type", Json::from("integer")), ("description", Json::from(desc))]);
    let described = |desc: &str| Json::obj([("description", Json::from(desc))]);
    let position = "World position in metres [x, y, z]; +Y is up; each within ±1,000,000";
    let rotation = "Rotation in degrees about X, then Y, then Z";
    let scale = "Number (uniform) or [x, y, z], each 0.001..100000";
    let color = "sRGB \"#rrggbb\" or [r,g,b] in 0..1";
    vec![
        ToolDef {
            name: "world_spawn".into(),
            description: "Add a shape to the live world. Returns the new entity (id, name, transform).".into(),
            input_schema: schema(
                vec![
                    (
                        "shape",
                        Json::obj([
                            ("type", Json::from("string")),
                            ("enum", Json::arr(Shape::ALL.iter().map(|s| Json::from(s.name())))),
                            ("description", Json::from("cube and sphere are 1 m across; quad is a 1×1 m floor tile in XZ facing up")),
                        ]),
                    ),
                    ("name", string("Unique name, up to 128 characters (default: <shape>_<id>)")),
                    ("position", vec3(&format!("{position}. Default [0,0,0]"))),
                    ("rotation_deg", vec3(&format!("{rotation}. Default [0,0,0]"))),
                    ("scale", described(&format!("{scale}. Default 1"))),
                    ("color", described(&format!("{color}. Default light grey"))),
                ],
                &["shape"],
            ),
        },
        ToolDef {
            name: "world_set".into(),
            description: "Change an existing entity (by id or name) in place; its id stays the same. Give any of: \
                          rename, position, rotation_deg, scale, color."
                .into(),
            input_schema: schema(
                vec![
                    ("id", integer("Entity id")),
                    ("name", string("Entity name (used when id is not given)")),
                    ("rename", string("New unique name")),
                    ("position", vec3(position)),
                    ("rotation_deg", vec3(rotation)),
                    ("scale", described(scale)),
                    ("color", described(color)),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "world_list".into(),
            description: "List every entity in the world with its transform, colour and creator.".into(),
            input_schema: schema(vec![], &[]),
        },
        ToolDef {
            name: "world_delete".into(),
            description: "Remove an entity by id or name.".into(),
            input_schema: schema(vec![("id", integer("Entity id")), ("name", string("Entity name"))], &[]),
        },
        ToolDef {
            name: "sight_frame".into(),
            description: "Render the world from a camera and return the PNG image plus JSON with each entity's \
                          on-screen rectangle [x, y, w, h] (pixels, origin top-left): the engine's projection of its \
                          bounding box, clipped to the view; occlusion is not considered. Without eye/target the camera \
                          frames the whole world from a three-quarter view. Large worlds return the biggest on-screen \
                          subjects first (max_subjects); the full list is saved at packet_path."
                .into(),
            input_schema: schema(
                vec![
                    ("width", integer("Image width 64..2048 (default 960)")),
                    ("height", integer("Image height 64..2048 (default 540)")),
                    ("eye", vec3("Camera position (give with target)")),
                    ("target", vec3("Point the camera looks at (give with eye; must differ from eye)")),
                    ("fov_y_deg", Json::obj([("type", Json::from("number")), ("description", Json::from("Vertical field of view, 10..120 (default 50)"))])),
                    ("max_subjects", integer("Subjects to include in the response, 1..500 (default 50)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "engine_info".into(),
            description: "Engine status: GPU, validation layer state, entity count, connected agents.".into(),
            input_schema: schema(vec![], &[]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Json {
        Json::parse(text).unwrap()
    }

    #[test]
    fn spawn_defaults_and_values() {
        let s = parse_spawn(&args(r##"{"shape":"cube"}"##)).unwrap();
        assert_eq!(s.shape, Shape::Cube);
        assert_eq!(s.scale, Vec3::ONE);
        let s = parse_spawn(&args(r##"{"shape":"sphere","name":"sun","position":[1,2,3],"scale":2,"color":"#f80"}"##))
            .unwrap();
        assert_eq!(s.name.as_deref(), Some("sun"));
        assert_eq!(s.position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(s.scale, Vec3::new(2.0, 2.0, 2.0));
        assert_eq!(s.color, [1.0, 136.0 / 255.0, 0.0]);
    }

    #[test]
    fn spawn_rejects_bad_input() {
        let long = format!(r#"{{"shape":"cube","name":"{}"}}"#, "n".repeat(129));
        for bad in [
            r#"{}"#,
            r#"{"shape":"cone"}"#,
            r#"{"shape":"cube","position":[1,2]}"#,
            r#"{"shape":"cube","position":[1e30,0,0]}"#,
            r#"{"shape":"cube","position":[1e300,0,0]}"#,
            r#"{"shape":"cube","scale":0}"#,
            r#"{"shape":"cube","scale":1e39}"#,
            r#"{"shape":"cube","scale":[1,1e-9,1]}"#,
            r#"{"shape":"cube","color":"red"}"#,
            r#"{"shape":"cube","color":[2,0,0]}"#,
            r#"{"shape":"cube","name":"  "}"#,
            r#"{"shape":"cube","name":"a\nb"}"#,
            &long,
        ] {
            assert!(parse_spawn(&args(bad)).is_err(), "{bad} should be rejected");
        }
        assert_eq!(parse_spawn(&args(r#"{"shape":1}"#)).err().unwrap(), "'shape' must be a string: cube, quad or sphere");
    }

    #[test]
    fn set_needs_a_key_and_a_change() {
        assert!(matches!(parse_set(&args(r#"{"id":3,"position":[1,2,3]}"#)), Ok(Command::Set(EntityKey::Id(3), _))));
        assert!(matches!(parse_set(&args(r#"{"name":"a","rename":"b"}"#)), Ok(Command::Set(EntityKey::Name(_), _))));
        assert!(parse_set(&args(r#"{"id":3}"#)).is_err(), "no change");
        let both = parse_set(&args(r#"{"id":1,"name":"b2","position":[0,0,0]}"#)).err().unwrap();
        assert!(both.contains("not both") && both.contains("rename"), "{both}");
        assert!(parse_set(&args(r#"{"position":[0,0,0]}"#)).is_err(), "no key");
        assert!(parse_set(&args(r##"{"id":-1,"color":"#fff"}"##)).is_err());
        assert!(parse_set(&args(r#"{"id":1,"scale":0}"#)).is_err());
    }

    #[test]
    fn frame_arguments() {
        let f = parse_frame(&args("{}")).unwrap();
        assert_eq!((f.width, f.height, f.eye.is_none(), f.max_subjects), (960, 540, true, 50));
        assert!(parse_frame(&args(r#"{"eye":[0,1,2]}"#)).is_err());
        assert!(parse_frame(&args(r#"{"width":10}"#)).is_err());
        assert!(parse_frame(&args(r#"{"width":4294967360}"#)).is_err(), "u32 wrap-around");
        assert!(parse_frame(&args(r#"{"eye":[1,1,1],"target":[1,1,1]}"#)).is_err(), "eye == target");
        assert!(parse_frame(&args(r#"{"max_subjects":0}"#)).is_err());
        let f = parse_frame(&args(r#"{"eye":[0,10,0],"target":[0,0,0],"fov_y_deg":70,"max_subjects":5}"#)).unwrap();
        assert_eq!((f.eye, f.max_subjects), (Some(Vec3::new(0.0, 10.0, 0.0)), 5));
    }

    #[test]
    fn tool_names_are_claude_code_safe() {
        for t in definitions() {
            assert!(t.name.len() <= 64 && t.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        }
    }
}

//! MCP tools agents use to drive the engine.
//!
//! Tool calls arrive on protocol threads; each is parsed here, sent to the main thread as a
//! [`Command`], and answered with the main thread's [`ToolOutput`]. The world is only ever touched
//! by the main thread, between frames.

use crate::world::{EntityKey, Shape, SpawnSpec};
use specula_core::math::Vec3;
use specula_core::Json;
use specula_protocol::{SessionInfo, ToolDef, ToolHandler, ToolOutput};
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

pub const INSTRUCTIONS: &str = "Specula is a live game engine you build in. Add things with world_spawn, \
inspect with world_list, and LOOK with sight_frame: it returns the rendered image plus a subject list giving \
every entity's exact on-screen rectangle, computed by the engine. Coordinates: metres, right-handed, +Y up. \
Other agents may be building in the same world at the same time; each entity records who created it.";

/// How long a tool call waits for the main thread before reporting a stall.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

pub enum Command {
    Spawn(SpawnSpec),
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
}

pub struct Request {
    pub session: SessionInfo,
    pub command: Command,
    pub reply: Sender<ToolOutput>,
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
        let request = Request { session: session.clone(), command, reply };
        if self.to_engine.send(request).is_err() {
            return ToolOutput::error("the engine is shutting down");
        }
        answer.recv_timeout(REPLY_TIMEOUT).unwrap_or_else(|e| match e {
            mpsc::RecvTimeoutError::Timeout => ToolOutput::error("the engine did not answer within 30 s"),
            mpsc::RecvTimeoutError::Disconnected => ToolOutput::error("the engine is shutting down"),
        })
    }
}

fn parse(name: &str, args: &Json) -> Result<Command, String> {
    match name {
        "world_spawn" => parse_spawn(args).map(Command::Spawn),
        "world_list" => Ok(Command::List),
        "world_delete" => match (args.get("id").and_then(Json::as_u64), args.get("name").and_then(Json::as_str)) {
            (Some(id), _) => Ok(Command::Delete(EntityKey::Id(id))),
            (None, Some(n)) => Ok(Command::Delete(EntityKey::Name(n.to_owned()))),
            (None, None) => Err("world_delete needs 'id' (integer) or 'name' (string)".into()),
        },
        "sight_frame" => parse_frame(args).map(Command::Frame),
        "engine_info" => Ok(Command::Info),
        other => Err(format!("unknown tool '{other}'")),
    }
}

fn parse_spawn(args: &Json) -> Result<SpawnSpec, String> {
    let shape_name = args.get("shape").and_then(Json::as_str).ok_or("'shape' is required")?;
    let shape = Shape::parse(shape_name).ok_or_else(|| format!("unknown shape '{shape_name}'"))?;
    let name = match args.get("name") {
        None | Some(Json::Null) => None,
        Some(v) => {
            let n = v.as_str().ok_or("'name' must be a string")?.trim();
            if n.is_empty() {
                return Err("'name' must not be empty".into());
            }
            Some(n.to_owned())
        }
    };
    let scale = match args.get("scale") {
        Some(Json::Num(s)) => Vec3::new(*s as f32, *s as f32, *s as f32),
        _ => vec3_arg(args, "scale", Vec3::ONE)?,
    };
    if [scale.x, scale.y, scale.z].iter().any(|c| *c <= 0.0) {
        return Err("'scale' components must be > 0".into());
    }
    Ok(SpawnSpec {
        shape,
        name,
        position: vec3_arg(args, "position", Vec3::ZERO)?,
        rotation_deg: vec3_arg(args, "rotation_deg", Vec3::ZERO)?,
        scale,
        color: color_arg(args.get("color"))?,
    })
}

fn parse_frame(args: &Json) -> Result<FrameSpec, String> {
    let dim = |key: &str, default: u32| -> Result<u32, String> {
        match args.get(key) {
            None | Some(Json::Null) => Ok(default),
            Some(v) => match v.as_u64() {
                Some(n) if (64..=2048).contains(&n) => Ok(n as u32),
                _ => Err(format!("'{key}' must be an integer from 64 to 2048")),
            },
        }
    };
    let opt_vec3 = |key: &str| -> Result<Option<Vec3>, String> {
        match args.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(_) => vec3_arg(args, key, Vec3::ZERO).map(Some),
        }
    };
    let fov_y_deg = match args.get("fov_y_deg") {
        None | Some(Json::Null) => 50.0,
        Some(v) => match v.as_f64() {
            Some(f) if (10.0..=120.0).contains(&f) => f as f32,
            _ => return Err("'fov_y_deg' must be a number from 10 to 120".into()),
        },
    };
    let eye = opt_vec3("eye")?;
    let target = opt_vec3("target")?;
    if eye.is_some() != target.is_some() {
        return Err("give both 'eye' and 'target', or neither (auto-framing)".into());
    }
    Ok(FrameSpec { width: dim("width", 960)?, height: dim("height", 540)?, eye, target, fov_y_deg })
}

fn vec3_arg(args: &Json, key: &str, default: Vec3) -> Result<Vec3, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(default),
        Some(v) => {
            let nums: Option<Vec<f32>> =
                v.as_array().and_then(|a| a.iter().map(|c| c.as_f64().map(|f| f as f32)).collect());
            match nums.as_deref() {
                Some(&[x, y, z]) if [x, y, z].iter().all(|c| c.is_finite()) => Ok(Vec3::new(x, y, z)),
                _ => Err(format!("'{key}' must be an array of 3 numbers")),
            }
        }
    }
}

/// `"#rrggbb"`, `"#rgb"`, or `[r, g, b]` with components in 0..1 (sRGB). Default light grey.
fn color_arg(v: Option<&Json>) -> Result<[f32; 3], String> {
    const ERR: &str = "'color' must be \"#rrggbb\", \"#rgb\" or [r, g, b] with components 0..1";
    match v {
        None | Some(Json::Null) => Ok([0.8, 0.8, 0.8]),
        Some(Json::Str(s)) => {
            let hex = s.strip_prefix('#').ok_or(ERR)?;
            let digits: Vec<u32> = hex.chars().map(|c| c.to_digit(16)).collect::<Option<_>>().ok_or(ERR)?;
            let channel = |hi: u32, lo: u32| (hi * 16 + lo) as f32 / 255.0;
            match digits.as_slice() {
                &[r, g, b] => Ok([channel(r, r), channel(g, g), channel(b, b)]),
                &[r1, r2, g1, g2, b1, b2] => Ok([channel(r1, r2), channel(g1, g2), channel(b1, b2)]),
                _ => Err(ERR.into()),
            }
        }
        Some(Json::Arr(a)) => match a.iter().map(Json::as_f64).collect::<Option<Vec<_>>>().as_deref() {
            Some(&[r, g, b]) if [r, g, b].iter().all(|c| (0.0..=1.0).contains(c)) => Ok([r as f32, g as f32, b as f32]),
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
                    ("name", string("Unique name (default: <shape>_<id>)")),
                    ("position", vec3("World position in metres [x, y, z]; +Y is up. Default [0,0,0]")),
                    ("rotation_deg", vec3("Rotation in degrees about X, then Y, then Z. Default [0,0,0]")),
                    ("scale", Json::obj([("description", Json::from("Number (uniform) or [x, y, z]. Default 1"))])),
                    ("color", Json::obj([("description", Json::from("sRGB \"#rrggbb\" or [r,g,b] in 0..1. Default light grey"))])),
                ],
                &["shape"],
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
            description: "Render the world from a camera and return the PNG image plus a JSON subject list with each \
                          entity's exact on-screen rectangle [x, y, w, h] (pixels, origin top-left). Without eye/target \
                          the camera frames the whole world from a three-quarter view."
                .into(),
            input_schema: schema(
                vec![
                    ("width", integer("Image width 64..2048 (default 960)")),
                    ("height", integer("Image height 64..2048 (default 540)")),
                    ("eye", vec3("Camera position (give with target)")),
                    ("target", vec3("Point the camera looks at (give with eye)")),
                    ("fov_y_deg", Json::obj([("type", Json::from("number")), ("description", Json::from("Vertical field of view, 10..120 (default 50)"))])),
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
        for bad in [
            r#"{}"#,
            r#"{"shape":"cone"}"#,
            r#"{"shape":"cube","position":[1,2]}"#,
            r#"{"shape":"cube","scale":0}"#,
            r#"{"shape":"cube","color":"red"}"#,
            r#"{"shape":"cube","color":[2,0,0]}"#,
            r#"{"shape":"cube","name":"  "}"#,
        ] {
            assert!(parse_spawn(&args(bad)).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn frame_arguments() {
        let f = parse_frame(&args("{}")).unwrap();
        assert_eq!((f.width, f.height, f.eye.is_none()), (960, 540, true));
        assert!(parse_frame(&args(r#"{"eye":[0,1,2]}"#)).is_err());
        assert!(parse_frame(&args(r#"{"width":10}"#)).is_err());
        let f = parse_frame(&args(r#"{"eye":[0,1,2],"target":[0,0,0],"fov_y_deg":70}"#)).unwrap();
        assert_eq!(f.eye, Some(Vec3::new(0.0, 1.0, 2.0)));
    }

    #[test]
    fn tool_names_are_claude_code_safe() {
        for t in definitions() {
            assert!(t.name.len() <= 64 && t.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        }
    }
}

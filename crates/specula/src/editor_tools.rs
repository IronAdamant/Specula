//! The editor's agent tools (docs/design/EDITOR.md, "Agents driving the editor"):
//! `editor_state` (read) and `editor_input` (simulated window input as the window's human, owner
//! only). Parsed here on the caller's thread; answered by the engine (`editor_engine`).

use specula_core::Json;
use specula_platform::{keys, Event, MouseButton, PointerKind};
use specula_protocol::ToolDef;

/// Steps in one `editor_input` call.
pub const MAX_STEPS: usize = 64;
/// Window frames one call's steps may expand to.
pub const MAX_FRAMES: usize = 512;
/// Pointer coordinates (pixels) are within this.
const MAX_COORD: f64 = 16_384.0;
/// Frames of motion one drag may take, and frames one wait may last.
const MAX_DRAG_STEPS: u64 = 60;
const MAX_WAIT: u64 = 120;
/// Characters one text step types.
const MAX_TEXT: usize = 64;

#[derive(Debug)]
pub enum EditorCall {
    State { detailed: bool },
    /// The steps expanded to window frames, each frame's events.
    Input(Vec<Vec<Event>>),
}

pub fn parse(name: &str, args: &Json) -> Option<Result<EditorCall, String>> {
    Some(match name {
        "editor_state" => crate::tools::check_keys(args, "editor_state", &[&["detailed"]]).and_then(|()| {
            let detailed = match args.get("detailed") {
                None | Some(Json::Null) => false,
                Some(Json::Bool(b)) => *b,
                Some(_) => return Err("'detailed' must be true or false".into()),
            };
            Ok(EditorCall::State { detailed })
        }),
        "editor_input" => crate::tools::check_keys(args, "editor_input", &[&["steps"]]).and_then(|()| parse_steps(args).map(EditorCall::Input)),
        _ => return None,
    })
}

fn point(v: Option<&Json>, what: &str) -> Result<(i32, i32), String> {
    let err = || format!("'{what}' must be [x, y] in window pixels (0..{MAX_COORD})");
    let a = v.and_then(Json::as_array).ok_or_else(err)?;
    match a {
        [x, y] => {
            let (x, y) = (x.as_f64().ok_or_else(err)?, y.as_f64().ok_or_else(err)?);
            if !(0.0..=MAX_COORD).contains(&x) || !(0.0..=MAX_COORD).contains(&y) {
                return Err(err());
            }
            Ok((x.round() as i32, y.round() as i32))
        }
        _ => Err(err()),
    }
}

fn button(v: Option<&Json>) -> Result<MouseButton, String> {
    match v.map(|j| j.as_str()) {
        None | Some(Some("left")) => Ok(MouseButton::Left),
        Some(Some("right")) => Ok(MouseButton::Right),
        Some(Some("middle")) => Ok(MouseButton::Middle),
        _ => Err("'button' must be \"left\", \"right\" or \"middle\"".into()),
    }
}

fn flag(o: &Json, key: &str) -> Result<bool, String> {
    match o.get(key) {
        None | Some(Json::Null) => Ok(false),
        Some(Json::Bool(b)) => Ok(*b),
        Some(_) => Err(format!("'{key}' must be true or false")),
    }
}

/// Modifier key codes held for a step (Shift, Ctrl).
fn mods(o: &Json) -> Result<Vec<u32>, String> {
    let mut m = Vec::new();
    if flag(o, "shift")? {
        m.push(0x10);
    }
    if flag(o, "ctrl")? {
        m.push(0x11);
    }
    Ok(m)
}

fn down(codes: &[u32]) -> impl Iterator<Item = Event> + '_ {
    codes.iter().map(|&code| Event::KeyDown { code, repeat: false })
}

fn up(codes: &[u32]) -> impl Iterator<Item = Event> + '_ {
    codes.iter().rev().map(|&code| Event::KeyUp { code })
}

fn parse_steps(args: &Json) -> Result<Vec<Vec<Event>>, String> {
    let steps = args.get("steps").and_then(Json::as_array).ok_or("'steps' must be an array of steps (see the description)")?;
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err(format!("'steps' must hold 1..{MAX_STEPS} steps (got {})", steps.len()));
    }
    let mut frames: Vec<Vec<Event>> = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        step(s, &mut frames).map_err(|e| format!("steps[{i}]: {e}"))?;
        if frames.len() > MAX_FRAMES {
            return Err(format!("the steps expand to more than {MAX_FRAMES} window frames; split them over calls"));
        }
    }
    Ok(frames)
}

const KINDS: &str = "move, press, release, click, drag, tap, key, text, wheel or wait";
const KIND_NAMES: [&str; 10] = ["move", "press", "release", "click", "drag", "tap", "key", "text", "wheel", "wait"];

fn step(s: &Json, frames: &mut Vec<Vec<Event>>) -> Result<(), String> {
    let Json::Obj(fields) = s else { return Err(format!("a step is an object with one of {KINDS}")) };
    let kind = fields.iter().map(|(k, _)| k.as_str()).find(|k| KIND_NAMES.contains(k)).ok_or_else(|| format!("a step needs one of {KINDS}"))?;
    let allowed: &[&str] = match kind {
        "move" => &["move"],
        "press" | "release" => &[kind, "button"],
        "click" => &["click", "button", "shift", "ctrl"],
        "drag" => &["drag", "to", "frames", "button", "shift", "ctrl"],
        "tap" => &["tap"],
        "key" => &["key"],
        "text" => &["text"],
        "wheel" => &["wheel", "at"],
        "wait" => &["wait"],
        _ => return Err(format!("a step needs one of {KINDS}")),
    };
    crate::tools::check_keys(s, &format!("a {kind} step"), &[allowed])?;
    let mv = |(x, y): (i32, i32)| Event::MouseMove { x, y };
    match kind {
        "move" => frames.push(vec![mv(point(s.get("move"), "move")?)]),
        "press" => {
            let (x, y) = point(s.get("press"), "press")?;
            frames.push(vec![mv((x, y)), Event::MouseDown { button: button(s.get("button"))?, x, y }]);
        }
        "release" => {
            let (x, y) = point(s.get("release"), "release")?;
            frames.push(vec![mv((x, y)), Event::MouseUp { button: button(s.get("button"))?, x, y }]);
        }
        "click" => {
            let (x, y) = point(s.get("click"), "click")?;
            let b = button(s.get("button"))?;
            let m = mods(s)?;
            frames.push(down(&m).chain([mv((x, y))]).collect());
            frames.push(vec![Event::MouseDown { button: b, x, y }]);
            frames.push(std::iter::once(Event::MouseUp { button: b, x, y }).chain(up(&m)).collect());
        }
        "drag" => {
            let (x0, y0) = point(s.get("drag"), "drag")?;
            let (x1, y1) = point(s.get("to"), "to")?;
            let n = match s.get("frames") {
                None | Some(Json::Null) => 8,
                Some(v) => v.as_u64().filter(|n| (1..=MAX_DRAG_STEPS).contains(n)).ok_or(format!("'frames' must be 1..{MAX_DRAG_STEPS}"))?,
            };
            let b = button(s.get("button"))?;
            let m = mods(s)?;
            frames.push(down(&m).chain([mv((x0, y0))]).collect());
            frames.push(vec![Event::MouseDown { button: b, x: x0, y: y0 }]);
            for k in 1..=n {
                let t = k as f64 / n as f64;
                let x = (f64::from(x0) + f64::from(x1 - x0) * t).round() as i32;
                let y = (f64::from(y0) + f64::from(y1 - y0) * t).round() as i32;
                frames.push(vec![mv((x, y))]);
            }
            frames.push(std::iter::once(Event::MouseUp { button: b, x: x1, y: y1 }).chain(up(&m)).collect());
        }
        "tap" => {
            let (x, y) = point(s.get("tap"), "tap")?;
            frames.push(vec![Event::PointerDown { id: 9001, kind: PointerKind::Touch, x, y }]);
            frames.push(vec![Event::PointerUp { id: 9001, kind: PointerKind::Touch, x, y }]);
        }
        "key" => {
            let spec = s.get("key").and_then(Json::as_str).ok_or("'key' must be a key name such as \"W\", \"F7\", \"Ctrl+Z\", \"Enter\"")?;
            if spec.len() > 32 {
                return Err("'key' is at most 32 characters".into());
            }
            let mut codes = Vec::new();
            for part in spec.split('+') {
                let code = keys::from_name(part.trim()).ok_or_else(|| format!("'{}' is not a key name (e.g. W, F7, Ctrl, Shift, Enter, Escape, Delete, Backspace)", crate::bounds::echo(part)))?;
                codes.push(code);
            }
            frames.push(down(&codes).collect());
            frames.push(up(&codes).collect());
        }
        "text" => {
            let t = s.get("text").and_then(Json::as_str).ok_or("'text' must be a string")?;
            if t.chars().count() > MAX_TEXT {
                return Err(format!("'text' is at most {MAX_TEXT} characters"));
            }
            frames.push(t.chars().filter(|c| !c.is_control()).map(Event::Char).collect());
        }
        "wheel" => {
            let notches = s.get("wheel").and_then(Json::as_f64).filter(|n| n.is_finite() && n.abs() <= 20.0).ok_or("'wheel' must be notches, -20..20 (+ = zoom in)")?;
            let mut f = Vec::new();
            if s.get("at").is_some() {
                f.push(mv(point(s.get("at"), "at")?));
            }
            frames.push(f);
            frames.push(vec![Event::MouseWheel { delta: notches as f32 }]);
        }
        "wait" => {
            let n = s.get("wait").and_then(Json::as_u64).filter(|n| (1..=MAX_WAIT).contains(n)).ok_or(format!("'wait' must be 1..{MAX_WAIT} frames"))?;
            frames.extend((0..n).map(|_| Vec::new()));
        }
        _ => unreachable!("kinds checked above"),
    }
    Ok(())
}

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(Vec::<Json>::new()))]);
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    vec![
        ToolDef {
            name: "editor_state".into(),
            description: "The window's editor as data (docs/design/EDITOR.md): open, mode (translate, rotate, scale), space, snap \
                          steps, the selection (id, path, version), the gizmo's handles on screen (pixel grab points: axis_x, \
                          plane_xz, ring_y, uniform, ...), the outliner page (rows with path, depth, who last changed each, \
                          claimed_by), the inspector (position, rotation_deg, scale, colour, visible, claim), the status line \
                          (the last edit, or why it was refused and whose claim it is), the human's undo and redo journal ids, \
                          what is still in flight (pick, edits, input frames), the window and viewport boxes and the camera. \
                          detailed: also the panels' widget boxes ([x, y, w, h]: buttons, fields, outliner rows) to aim \
                          editor_input at. Every edit the editor makes is a world_set / world_batch / world_undo as window:owner: \
                          read them in world_history and world_changes."
                .into(),
            input_schema: schema(vec![("detailed", t("boolean", "Also the panels' widget boxes"))]),
        },
        ToolDef {
            name: "editor_input".into(),
            description: "Drives the window's editor with simulated window input, as the window's human: its edits are \
                          window:owner's (journaled, claimed, undoable), so it needs owner authority (an operator grant, --owner \
                          NAME); it is traced (editor.input) and editor_state names who drove it. Each step runs on window frames \
                          of its own, in order; the call answers once they have all run (with editor_state). Steps (one key \
                          each): {move: [x, y]}, {press: [x, y], button?}, {release: [x, y], button?}, {click: [x, y], button?, \
                          shift?, ctrl?}, {drag: [x, y], to: [x, y], frames? 1..60, button?, shift?, ctrl? (Ctrl inverts \
                          snapping)}, {tap: [x, y]} (touch), {key: \"F7\" | \"W\" | \"Ctrl+Z\" | \"Enter\" ...}, {text: \"1.5\"} \
                          (typed characters, at most 64), {wheel: notches, at?: [x, y]}, {wait: frames 1..120}. Window pixels \
                          from the top left (editor_state's window, viewport and widgets). F7 opens the editor; click selects \
                          (the ID buffer), a drag from empty viewport selects by box, a drag from a gizmo handle moves, turns or \
                          scales (one journaled edit on release); at most one edit per call. Its edit is journaled with you as \
                          `via` and this call as `call` (world_history, trace_call). At most 64 steps and 512 frames a call; \
                          refused headless (no_window), at once while the review or gallery screen holds the window or it is \
                          minimized (window_held: held_by, retry), and frames not run by the call's deadline are dropped, never \
                          run later (input_dropped: frames_run, frames_dropped)."
                .into(),
            input_schema: schema(vec![(
                "steps",
                Json::obj([
                    ("type", Json::from("array")),
                    ("maxItems", Json::from(MAX_STEPS)),
                    ("description", Json::from("Steps, 1..64: objects with one of move, press, release, click, drag, tap, key, text, wheel, wait")),
                    ("items", Json::obj([("type", Json::from("object"))])),
                ]),
            )]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: &str) -> Result<EditorCall, String> {
        parse(name, &Json::parse(args).unwrap()).unwrap()
    }

    fn frames(args: &str) -> Vec<Vec<Event>> {
        match call("editor_input", args).unwrap() {
            EditorCall::Input(f) => f,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn steps_expand_to_frames() {
        let f = frames(r#"{"steps": [{"click": [10, 20]}]}"#);
        assert_eq!(f.len(), 3);
        assert!(matches!(f[1][0], Event::MouseDown { button: MouseButton::Left, x: 10, y: 20 }));
        let f = frames(r#"{"steps": [{"drag": [0, 0], "to": [100, 50], "frames": 4, "ctrl": true}]}"#);
        assert_eq!(f.len(), 1 + 1 + 4 + 1);
        assert!(matches!(f[0][0], Event::KeyDown { code: 0x11, .. }));
        assert!(matches!(f[5][0], Event::MouseMove { x: 100, y: 50 }));
        assert!(matches!(f.last().unwrap()[1], Event::KeyUp { code: 0x11 }));
        let f = frames(r#"{"steps": [{"key": "Ctrl+Z"}]}"#);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].len(), 2);
        let f = frames(r#"{"steps": [{"text": "1.5"}, {"wait": 3}]}"#);
        assert_eq!(f.len(), 4);
        assert_eq!(f[0], vec![Event::Char('1'), Event::Char('.'), Event::Char('5')]);
        let f = frames(r#"{"steps": [{"tap": [5, 5]}]}"#);
        assert!(matches!(f[0][0], Event::PointerDown { kind: PointerKind::Touch, .. }));
    }

    #[test]
    fn bad_steps_are_refused_with_what_to_change() {
        for (args, want) in [
            (r#"{"steps": []}"#, "1..64"),
            (r#"{"steps": [{"click": [1]}]}"#, "[x, y]"),
            (r#"{"steps": [{"click": [-1, 5]}]}"#, "[x, y]"),
            (r#"{"steps": [{"click": [1, 2], "button": "thumb"}]}"#, "button"),
            (r#"{"steps": [{"key": "Hyper"}]}"#, "not a key name"),
            (r#"{"steps": [{"wait": 0}]}"#, "1..120"),
            (r#"{"steps": [{"jump": 1}]}"#, "one of"),
            (r#"{"steps": [{"click": [1, 2], "speed": 3}]}"#, "speed"),
            (r#"{"steps": [{"drag": [1, 2], "to": [3, 4], "frames": 999}]}"#, "1..60"),
            (r#"{"steps": [{"wheel": 1e9}]}"#, "-20..20"),
            (r#"{"bogus": 1}"#, "bogus"),
        ] {
            let e = call("editor_input", args).unwrap_err();
            assert!(e.contains(want), "{args}: {e}");
        }
        let many = format!(r#"{{"steps": [{}]}}"#, [r#"{"wait": 120}"#; 5].join(","));
        assert!(call("editor_input", &many).unwrap_err().contains("512"));
        let long = format!(r#"{{"steps": [{{"text": "{}"}}]}}"#, "x".repeat(65));
        assert!(call("editor_input", &long).unwrap_err().contains("64"));
    }

    #[test]
    fn state_takes_detailed_only() {
        assert!(matches!(call("editor_state", "{}"), Ok(EditorCall::State { detailed: false })));
        assert!(matches!(call("editor_state", r#"{"detailed": true}"#), Ok(EditorCall::State { detailed: true })));
        assert!(call("editor_state", r#"{"detailed": 1}"#).is_err());
        assert!(parse("world_set", &Json::Null).is_none());
    }
}

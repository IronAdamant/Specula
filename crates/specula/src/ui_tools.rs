//! Arguments and definitions of the UI sight tools (docs/design/UI.md, "Agent-facing tools"):
//! `ui_tree` (the laid-out widget tree as paged data), `ui_frame` (a capture with every widget's
//! outline and id, the UI version of `sight_frame`), `ui_act` (a player's input, synthesised:
//! gamepad, keyboard, mouse or touch, through the same frames a player's input goes through) and
//! `ui_check` (measured checks at several displays and UI scales). The engine side is
//! [`crate::ui_engine`].
//!
//! The tools work on whatever interface is showing: the hosted game's (`surface: "game"`) and
//! the engine's own screens (`review`, `gallery`). Every argument has a limit and every answer a
//! ceiling (docs/design/TOOLS.md).

use crate::tools::{bool_arg, check_keys, int_arg, num_in, string_arg, FrameSpec};
use specula_core::Json;
use specula_protocol::ToolDef;
use specula_ui::check::{Target, PRESETS};
use specula_ui::Only;

/// The interfaces the tools can look at.
pub const SURFACES: [&str; 4] = ["game", "review", "gallery", "editor"];
/// The `ui_check` rules (for `rules` filters).
pub const RULES: [&str; 17] = [
    "legibility", "contrast", "non_text_contrast", "overlap", "offscreen", "overflow", "text_overflow", "missing_glyph", "touch_target", "focus_reachability",
    "focus_scope", "gamepad_access", "unbound", "unknown_texture", "untranslated", "missing_key", "hardcoded",
];

/// Most screens one `ui_check` checks together.
pub const MAX_SCREENS: usize = 16;
/// Most targets, scales, and their product, per `ui_check`.
pub const MAX_TARGETS: usize = 8;
pub const MAX_SCALES: usize = 4;
pub const MAX_CHECKS: usize = 16;
/// Most findings a `ui_check` answer quotes.
pub const MAX_FINDINGS: usize = 200;
/// Most widgets per `ui_tree` page.
pub const MAX_TREE_PAGE: usize = 200;
/// Most steps per `ui_act`, and synthetic frames one call may run in all.
pub const MAX_STEPS: usize = 16;
pub const MAX_FRAMES: usize = 160;
/// Engine-thread time one `ui_act` may spend on its frames: it stops before a frame (or a press,
/// which can't be cut in half) that its frames so far say would end past this, and answers with
/// the steps still to run (`rest`). 75 ms at 160 frames during a busy wave was the review's
/// stall (R1 of m5-uitools, finding 5); the class budget is 30 ms.
pub const BUDGET_MS: f64 = 20.0;
/// Longest text one `type` step sends.
pub const MAX_TYPE_CHARS: usize = 256;
/// Most frames a press is held, and an explicit wait.
pub const MAX_HOLD: u32 = 30;
pub const MAX_WAIT: u32 = 120;
/// Most wheel notches (or stick frames) per scroll step.
pub const MAX_SCROLL: i64 = 20;
/// Widget ids and screen names: at most this many characters.
pub const MAX_ID: usize = 64;
/// Largest `ui_frame` side and pixel count (4K).
pub const MAX_FRAME_SIDE: u32 = 4096;
pub const MAX_FRAME_PIXELS: u64 = 3840 * 2160;
/// Most outlines one `ui_frame` lists.
pub const MAX_OUTLINES: usize = 150;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Game,
    Review,
    Gallery,
    /// The window's editor (F7): its panels (toolbar, outliner, inspector, the swarm panels).
    /// Its buttons act as the window's owner, so `ui_act` on it is owner-only; never the
    /// "Ask the agents" box, whose words are the human's alone.
    Editor,
}

impl Surface {
    pub fn name(self) -> &'static str {
        match self {
            Surface::Game => "game",
            Surface::Review => "review",
            Surface::Gallery => "gallery",
            Surface::Editor => "editor",
        }
    }

    fn parse(s: &str) -> Option<Surface> {
        match s {
            "game" => Some(Surface::Game),
            "review" => Some(Surface::Review),
            "gallery" => Some(Surface::Gallery),
            "editor" => Some(Surface::Editor),
            _ => None,
        }
    }
}

/// `display`: the virtual display headless engines lay the interface out for (and, with a
/// window, the safe area and OS scale to simulate). Unset fields keep their value.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayArg {
    pub size: Option<[u32; 2]>,
    /// `Some(None)`: back to the platform's (or none).
    pub os_scale: Option<Option<f32>>,
    pub safe_area: Option<[f32; 4]>,
    pub touch: Option<bool>,
    /// A physical panel the size of the display, this many inches across (a handheld: the
    /// game's handheld profile reads it). `Some(None)`: none.
    pub diagonal_in: Option<Option<f32>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Gamepad,
    Keyboard,
    Mouse,
    Touch,
}

impl Device {
    pub fn name(self) -> &'static str {
        match self {
            Device::Gamepad => "gamepad",
            Device::Keyboard => "keyboard",
            Device::Mouse => "mouse",
            Device::Touch => "touch",
        }
    }

    fn parse(s: &str) -> Option<Device> {
        match s {
            "gamepad" => Some(Device::Gamepad),
            "keyboard" => Some(Device::Keyboard),
            "mouse" => Some(Device::Mouse),
            "touch" => Some(Device::Touch),
            _ => None,
        }
    }

    /// Points at things (clicks, taps) rather than moving a focus.
    pub fn pointer(self) -> bool {
        matches!(self, Device::Mouse | Device::Touch)
    }
}

/// Where a click lands.
#[derive(Clone, Debug, PartialEq)]
pub enum At {
    Widget(String),
    /// Physical pixels, origin top-left.
    Point([f32; 2]),
}

/// One `ui_act` step.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// A UI direction (`up`, `down`, `left`, `right`, `accept`, `back`), an input action of the
    /// game (`pause`: its binding on the device), or a raw source (`Pad.Start`, `Key.F`).
    Press { name: String, device: Device, hold: u32 },
    /// Move the focus to a widget with the d-pad (or arrow keys), along the shortest route.
    Focus { id: String, device: Device },
    /// Press a widget the way the device does: focus it and accept (gamepad, keyboard; a HUD
    /// button's `shortcut` action), or click / tap its centre (mouse, touch).
    Activate { id: String, device: Device },
    Click { at: At, device: Device },
    Type { text: String },
    /// Wheel notches (+ toward the player: down) at a widget (mouse), or the right stick for
    /// that many frames (gamepad).
    Scroll { amount: i64, at: Option<String>, device: Device },
    Wait { frames: u32 },
}

impl Step {
    /// The step as `ui_act` takes it (`rest` in an answer that stopped early: send it back).
    pub fn to_json(&self) -> Json {
        let dev = |d: &Device| ("device", Json::from(d.name()));
        match self {
            Step::Press { name, device, hold } => {
                let mut v = vec![("press", Json::from(name.as_str())), dev(device)];
                if *hold != 1 {
                    v.push(("hold", Json::from(*hold)));
                }
                Json::obj(v)
            }
            Step::Focus { id, device } => Json::obj([("focus", Json::from(id.as_str())), dev(device)]),
            Step::Activate { id, device } => Json::obj([("activate", Json::from(id.as_str())), dev(device)]),
            Step::Click { at, device } => {
                let at = match at {
                    At::Widget(id) => Json::from(id.as_str()),
                    At::Point(p) => Json::arr(p.map(|v| Json::from(f64::from(v)))),
                };
                Json::obj([("click", at), dev(device)])
            }
            Step::Type { text } => Json::obj([("type", Json::from(text.as_str()))]),
            Step::Scroll { amount, at, device } => {
                let mut v = vec![("scroll", Json::from(*amount)), dev(device)];
                if let Some(a) = at {
                    v.push(("at", Json::from(a.as_str())));
                }
                Json::obj(v)
            }
            Step::Wait { frames } => Json::obj([("wait", Json::from(*frames))]),
        }
    }
}

pub struct TreeArgs {
    pub surface: Option<Surface>,
    pub screen: Option<String>,
    pub only: Only,
    pub offset: usize,
    pub limit: usize,
    /// Lay out now for the current display, instead of reading the last frame's layout.
    pub fresh: bool,
    /// Longest text a widget carries, in characters (`specula_ui::agent::MAX_ENTRY_TEXT`
    /// unless asked; a page's long texts together are at most `MAX_PAGE_TEXT` bytes).
    pub text_chars: usize,
}

pub struct CheckArgs {
    pub surface: Option<Surface>,
    /// `None`: the screens showing now.
    pub screens: Option<Vec<String>>,
    pub targets: Vec<Target>,
    pub pseudo: bool,
    pub rules: Option<Vec<String>>,
    pub max_findings: usize,
    /// `locale`: check in this language (`crate::l10n_tools` makes `strings` from the project's
    /// string tables on the caller's connection before the call reaches the engine).
    pub locale: Option<String>,
    pub strings: Option<std::sync::Arc<specula_ui::Strings>>,
}

pub struct ActArgs {
    pub surface: Option<Surface>,
    pub display: Option<DisplayArg>,
    pub steps: Vec<Step>,
    /// Idle frames after each step, so what it opened is laid out.
    pub settle: u32,
}

pub struct FrameArgs {
    pub frame: FrameSpec,
    /// A display for this capture alone (merged over the caller's): nothing persists.
    pub display: Option<DisplayArg>,
    /// No camera given: look through the window's (or the game's) camera.
    pub player_camera: bool,
    /// `None`: the display's size.
    pub size: Option<[u32; 2]>,
}

pub enum UiCall {
    Tree(TreeArgs),
    Check(CheckArgs),
    Act(ActArgs),
    Frame(Box<FrameArgs>),
    /// The hosted game's language and the string-table keys its code uses (`l10n_*`).
    Locale(std::sync::mpsc::Sender<crate::game::GameLocale>),
}

impl UiCall {
    pub fn class(&self) -> crate::schedule::Class {
        use crate::schedule::Class;
        match self {
            // It may lay the interface out now (fresh, or a display the last frame didn't lay
            // out for): a few milliseconds, past a read's 4 ms budget (ui_check.py's probes).
            UiCall::Tree(_) => Class::Big,
            UiCall::Check(_) => Class::Big,
            // Up to BUDGET_MS of game frames: scheduled like other big work, not like a game
            // tool's half millisecond (review R1 of m5-uitools, finding 5).
            UiCall::Act(_) => Class::Big,
            UiCall::Frame(_) => Class::Render,
            UiCall::Locale(_) => Class::Read,
        }
    }
}

fn surface_arg(args: &Json) -> Result<Option<Surface>, String> {
    match string_arg(args, "surface")? {
        None => Ok(None),
        Some(s) => Surface::parse(s).map(Some).ok_or_else(|| format!("'surface' must be one of {}, not '{}'", SURFACES.join(", "), crate::bounds::echo(s))),
    }
}

fn id_arg(v: &Json, key: &str, what: &str) -> Result<String, String> {
    match v.as_str() {
        Some(s) if !s.is_empty() && s.chars().count() <= MAX_ID => Ok(s.to_owned()),
        _ => Err(format!("'{key}' must be {what}: 1..{MAX_ID} characters")),
    }
}

fn names_arg(args: &Json, key: &str, max: usize) -> Result<Option<Vec<String>>, String> {
    match args.get(key).filter(|v| !v.is_null()) {
        None => Ok(None),
        Some(v) => {
            let a = v.as_array().filter(|a| !a.is_empty() && a.len() <= max).ok_or_else(|| format!("'{key}' must be an array of 1..{max} names"))?;
            let mut out: Vec<String> = Vec::with_capacity(a.len());
            for x in a {
                let s = id_arg(x, key, "a name")?;
                if out.contains(&s) {
                    return Err(format!("'{key}' names '{}' twice", crate::bounds::echo(&s)));
                }
                out.push(s);
            }
            Ok(Some(out))
        }
    }
}

fn nums<const N: usize>(v: &Json, key: &str, lo: f64, hi: f64) -> Result<[f64; N], String> {
    let a: Option<Vec<f64>> = v.as_array().and_then(|a| a.iter().map(Json::as_f64).collect());
    match a {
        Some(a) if a.len() == N && a.iter().all(|x| x.is_finite() && (lo..=hi).contains(x)) => Ok(std::array::from_fn(|i| a[i])),
        _ => Err(format!("'{key}' must be {N} numbers from {lo} to {hi}")),
    }
}

fn size_arg(v: &Json, key: &str) -> Result<[u32; 2], String> {
    let [w, h] = nums::<2>(v, key, 64.0, f64::from(MAX_FRAME_SIDE))?;
    if w.fract() != 0.0 || h.fract() != 0.0 {
        return Err(format!("'{key}' must be whole pixels [width, height]"));
    }
    Ok([w as u32, h as u32])
}

/// `display {size, os_scale, safe_area, touch, diagonal_in}`.
fn display_arg(args: &Json) -> Result<Option<DisplayArg>, String> {
    let Some(d) = args.get("display").filter(|v| !v.is_null()) else { return Ok(None) };
    check_keys(d, "'display'", &[&["size", "os_scale", "safe_area", "touch", "diagonal_in"]])?;
    let mut out = DisplayArg::default();
    if let Some(s) = d.get("size").filter(|v| !v.is_null()) {
        out.size = Some(size_arg(s, "display.size")?);
    }
    match d.get("os_scale") {
        None => {}
        Some(Json::Null) => out.os_scale = Some(None),
        Some(v) => out.os_scale = Some(Some(v.as_f64().filter(|x| (0.5..=5.0).contains(x)).ok_or("'display.os_scale' must be a number from 0.5 to 5 (Windows: DPI / 96), or null")? as f32)),
    }
    if let Some(s) = d.get("safe_area").filter(|v| !v.is_null()) {
        out.safe_area = Some(nums::<4>(s, "display.safe_area", 0.0, 2048.0)?.map(|v| v as f32));
    }
    out.touch = bool_arg(d, "touch")?;
    match d.get("diagonal_in") {
        None => {}
        Some(Json::Null) => out.diagonal_in = Some(None),
        Some(v) => out.diagonal_in = Some(Some(v.as_f64().filter(|x| (3.0..=100.0).contains(x)).ok_or("'display.diagonal_in' must be a number of inches from 3 to 100 (the panel's diagonal: 7 is a Steam Deck), or null")? as f32)),
    }
    Ok(Some(out))
}

/// `targets` (presets or `{name, size, ppi, os_scale, safe_area, touch}`) times `scales`.
fn targets_arg(args: &Json) -> Result<Vec<Target>, String> {
    let base: Vec<Target> = match args.get("targets").filter(|v| !v.is_null()) {
        None => vec![Target::deck(), Target::desktop_1080p(), Target::desktop_4k()],
        Some(v) => {
            let a = v.as_array().filter(|a| !a.is_empty() && a.len() <= MAX_TARGETS).ok_or_else(|| format!("'targets' must be an array of 1..{MAX_TARGETS} targets"))?;
            let mut out: Vec<Target> = Vec::new();
            for (i, t) in a.iter().enumerate() {
                let target = match t {
                    Json::Str(p) => Target::preset(p).ok_or_else(|| format!("'targets': unknown preset '{}' (one of {})", crate::bounds::echo(p), PRESETS.join(", ")))?,
                    Json::Obj(_) => {
                        check_keys(t, "a target", &[&["name", "size", "ppi", "os_scale", "safe_area", "touch"]])?;
                        let size = size_arg(t.get("size").ok_or_else(|| format!("'targets[{i}].size' is required: [width, height]"))?, "size")?;
                        let name = match t.get("name") {
                            Some(n) => id_arg(n, "name", "a target name")?,
                            None => format!("custom_{}x{}", size[0], size[1]),
                        };
                        let mut tg = Target::new(&name, size, num_in(t, "ppi", 30.0..=1000.0)?);
                        tg.os_scale = num_in(t, "os_scale", 0.5..=5.0)?;
                        if let Some(s) = t.get("safe_area").filter(|v| !v.is_null()) {
                            tg.safe_area = nums::<4>(s, "safe_area", 0.0, 2048.0)?.map(|v| v as f32);
                        }
                        tg.touch = bool_arg(t, "touch")?.unwrap_or(false);
                        tg
                    }
                    _ => return Err(format!("'targets[{i}]' must be a preset name or {{name, size, ppi, os_scale, safe_area, touch}}")),
                };
                if out.iter().any(|o| o.name == target.name) {
                    return Err(format!("'targets' has '{}' twice", target.name));
                }
                out.push(target);
            }
            out
        }
    };
    let scales: Vec<f32> = match args.get("scales").filter(|v| !v.is_null()) {
        None => vec![1.0],
        Some(v) => {
            let a: Option<Vec<f64>> = v.as_array().filter(|a| !a.is_empty() && a.len() <= MAX_SCALES).and_then(|a| a.iter().map(Json::as_f64).collect());
            let a = a.filter(|a| a.iter().all(|s| (0.5..=3.0).contains(s))).ok_or_else(|| format!("'scales' must be an array of 1..{MAX_SCALES} UI scales from 0.5 to 3"))?;
            let mut out: Vec<f32> = Vec::new();
            for s in a {
                let s = s as f32;
                if out.iter().any(|o| (o - s).abs() < 1e-4) {
                    return Err(format!("'scales' has {s} twice"));
                }
                out.push(s);
            }
            out
        }
    };
    if base.len() * scales.len() > MAX_CHECKS {
        return Err(format!("{} targets x {} scales is {} layouts; at most {MAX_CHECKS} per call", base.len(), scales.len(), base.len() * scales.len()));
    }
    Ok(scales.iter().flat_map(|&s| base.iter().map(move |t| t.at_scale(s))).collect())
}

fn device_of(v: &Json, default: Device) -> Result<Device, String> {
    match v.get("device").filter(|d| !d.is_null()) {
        None => Ok(default),
        Some(d) => d.as_str().and_then(Device::parse).ok_or_else(|| "'device' must be gamepad, keyboard, mouse or touch".into()),
    }
}

fn parse_step(v: &Json, i: usize, default: Device) -> Result<Step, String> {
    let kinds = ["press", "focus", "activate", "click", "type", "scroll", "wait"];
    let Some(pairs) = v.as_object() else { return Err(format!("'steps[{i}]' must be an object with one of: {}", kinds.join(", "))) };
    let given: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).filter(|k| kinds.contains(k)).collect();
    if given.len() != 1 {
        return Err(format!("'steps[{i}]' needs exactly one of: {} (has {})", kinds.join(", "), if given.is_empty() { "none".to_owned() } else { given.join(", ") }));
    }
    let what = given[0];
    let extra: &[&str] = match what {
        "press" => &["device", "hold"],
        "focus" | "activate" => &["device"],
        "click" => &["device"],
        "scroll" => &["device", "at"],
        _ => &[],
    };
    check_keys(v, &format!("steps[{i}]"), &[&[what], extra])?;
    let device = device_of(v, default)?;
    let val = v.get(what).expect("present");
    Ok(match what {
        "press" => {
            let name = id_arg(val, "press", "a UI direction (up, down, left, right, accept, back), an input action or a source (Pad.Start)")?;
            let hold = int_arg(v, "hold", 1, u64::from(MAX_HOLD))?.map_or(1, |h| h as u32);
            if device.pointer() {
                return Err(format!("'steps[{i}]': press needs a gamepad or keyboard device (mouse and touch click or activate)"));
            }
            Step::Press { name, device, hold }
        }
        "focus" => {
            if device.pointer() {
                return Err(format!("'steps[{i}]': focus moves with the d-pad or arrow keys: device gamepad or keyboard"));
            }
            Step::Focus { id: id_arg(val, "focus", "a widget id")?, device }
        }
        "activate" => Step::Activate { id: id_arg(val, "activate", "a widget id")?, device },
        "click" => {
            let at = match val {
                Json::Str(_) => At::Widget(id_arg(val, "click", "a widget id")?),
                _ => {
                    let p = nums::<2>(val, "click", 0.0, f64::from(MAX_FRAME_SIDE))?;
                    At::Point([p[0] as f32, p[1] as f32])
                }
            };
            let device = if v.get("device").is_some() { device } else { Device::Mouse };
            if !device.pointer() {
                return Err(format!("'steps[{i}]': click needs device mouse or touch"));
            }
            Step::Click { at, device }
        }
        "type" => match val.as_str() {
            Some(t) if !t.is_empty() && t.chars().count() <= MAX_TYPE_CHARS && !t.chars().any(char::is_control) => Step::Type { text: t.to_owned() },
            _ => return Err(format!("'type' must be 1..{MAX_TYPE_CHARS} characters of text (no control characters)")),
        },
        "scroll" => {
            let amount = val.as_i64().filter(|a| *a != 0 && a.abs() <= MAX_SCROLL).ok_or_else(|| format!("'scroll' must be a whole number of notches, -{MAX_SCROLL}..{MAX_SCROLL}, not 0 (+ scrolls down)"))?;
            let at = match v.get("at").filter(|a| !a.is_null()) {
                Some(a) => Some(id_arg(a, "at", "a widget id")?),
                None => None,
            };
            if device == Device::Touch || device == Device::Keyboard {
                return Err(format!("'steps[{i}]': scroll with the mouse wheel or the gamepad's right stick (device mouse or gamepad)"));
            }
            Step::Scroll { amount, at, device }
        }
        _ => Step::Wait { frames: val.as_u64().filter(|f| (1..=u64::from(MAX_WAIT)).contains(f)).ok_or_else(|| format!("'wait' must be 1..{MAX_WAIT} frames"))? as u32 },
    })
}

/// The UI tools' arguments; `None` when `name` isn't one of them.
pub fn parse(name: &str, args: &Json) -> Option<Result<UiCall, String>> {
    Some(match name {
        "ui_tree" => parse_tree(args).map(UiCall::Tree),
        "ui_check" => parse_check(args).map(UiCall::Check),
        "ui_act" => parse_act(args).map(UiCall::Act),
        "ui_frame" => parse_ui_frame(args).map(|f| UiCall::Frame(Box::new(f))),
        _ => return None,
    })
}

fn parse_tree(args: &Json) -> Result<TreeArgs, String> {
    check_keys(args, "ui_tree", &[&["surface", "screen", "only", "offset", "limit", "fresh", "text_chars"]])?;
    let only = match string_arg(args, "only")? {
        None => Only::Named,
        Some(s) => Only::parse(s).ok_or("'only' must be named, interactive or all")?,
    };
    let screen = match args.get("screen").filter(|v| !v.is_null()) {
        Some(s) => Some(id_arg(s, "screen", "a screen name")?),
        None => None,
    };
    Ok(TreeArgs {
        surface: surface_arg(args)?,
        screen,
        only,
        offset: int_arg(args, "offset", 0, 100_000)?.unwrap_or(0) as usize,
        limit: int_arg(args, "limit", 1, MAX_TREE_PAGE as u64)?.unwrap_or(100) as usize,
        fresh: bool_arg(args, "fresh")?.unwrap_or(false),
        text_chars: int_arg(args, "text_chars", 1, specula_ui::state::MAX_TEXT as u64)?.map_or(specula_ui::agent::MAX_ENTRY_TEXT, |n| n as usize),
    })
}

fn parse_check(args: &Json) -> Result<CheckArgs, String> {
    check_keys(args, "ui_check", &[&["surface", "screens", "targets", "scales", "pseudo", "rules", "max_findings", "locale"]])?;
    let rules = names_arg(args, "rules", RULES.len())?;
    if let Some(r) = rules.as_ref().and_then(|r| r.iter().find(|r| !RULES.contains(&r.as_str()))) {
        return Err(format!("'rules': unknown rule '{}' (one of {})", crate::bounds::echo(r), RULES.join(", ")));
    }
    Ok(CheckArgs {
        surface: surface_arg(args)?,
        screens: names_arg(args, "screens", MAX_SCREENS)?,
        targets: targets_arg(args)?,
        pseudo: bool_arg(args, "pseudo")?.unwrap_or(false),
        rules,
        max_findings: int_arg(args, "max_findings", 1, MAX_FINDINGS as u64)?.unwrap_or(50) as usize,
        locale: match string_arg(args, "locale")? {
            Some(l) => Some(specula_lingua::Locale::parse(l).map_err(|e| format!("'locale': {}", e.message))?.to_string()),
            None => None,
        },
        strings: None,
    })
}

fn parse_act(args: &Json) -> Result<ActArgs, String> {
    check_keys(args, "ui_act", &[&["surface", "display", "device", "steps", "settle"]])?;
    let device = match string_arg(args, "device")? {
        None => Device::Gamepad,
        Some(d) => Device::parse(d).ok_or("'device' must be gamepad, keyboard, mouse or touch")?,
    };
    let list = args.get("steps").and_then(Json::as_array).filter(|a| a.len() <= MAX_STEPS).ok_or_else(|| format!("'steps' is required: an array of 0..{MAX_STEPS} steps"))?;
    let steps = list.iter().enumerate().map(|(i, s)| parse_step(s, i, device)).collect::<Result<Vec<_>, _>>()?;
    Ok(ActArgs {
        surface: surface_arg(args)?,
        display: display_arg(args)?,
        steps,
        settle: int_arg(args, "settle", 0, 10)?.map_or(1, |s| s as u32),
    })
}

fn parse_ui_frame(args: &Json) -> Result<FrameArgs, String> {
    check_keys(args, "ui_frame", &[&["size", "display", "eye", "target", "fov_y_deg", "camera", "camera_of", "max_labels", "ids", "time_s"]])?;
    let display = display_arg(args)?;
    if let Some(s) = display.and_then(|d| d.size) {
        if u64::from(s[0]) * u64::from(s[1]) > MAX_FRAME_PIXELS {
            return Err(format!("'display.size' {}x{} is past 4K ({MAX_FRAME_PIXELS} pixels)", s[0], s[1]));
        }
    }
    let size = match args.get("size").filter(|v| !v.is_null()) {
        Some(s) => {
            let s = size_arg(s, "size")?;
            if u64::from(s[0]) * u64::from(s[1]) > MAX_FRAME_PIXELS {
                return Err(format!("'size' {}x{} is past 4K ({MAX_FRAME_PIXELS} pixels)", s[0], s[1]));
            }
            Some(s)
        }
        None => None,
    };
    // sight_frame's own parser for the camera, with the interface drawn and its outlines on.
    let mut pass = Json::obj::<&str>([]);
    for k in ["eye", "target", "fov_y_deg", "camera", "camera_of", "max_labels", "time_s"] {
        if let Some(v) = args.get(k) {
            pass.set(k, v.clone());
        }
    }
    pass.set("include_ui", Json::Bool(true));
    pass.set("max_subjects", Json::from(1));
    pass.set("overlays", Json::arr([Json::from("ui")]));
    // Every outline labelled up to the overlay's most (a busy HUD has 35: the default 32 left some
    // unlabelled, which reads like a widget under a modal; sight-skeptic, run 3).
    if args.get("max_labels").is_none_or(Json::is_null) {
        pass.set("max_labels", Json::from(crate::sight_overlay::LABELS.1));
    }
    pass.set("ids", Json::Bool(bool_arg(args, "ids")?.unwrap_or(false)));
    let mut frame = crate::tools::parse_frame(&pass)?;
    let player_camera = frame.eye.is_none() && frame.camera.is_none();
    if let Some([w, h]) = size {
        frame.width = w;
        frame.height = h;
    }
    Ok(FrameArgs { frame, display, player_camera, size })
}

/// The tools' schemas.
pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let surface = || {
        Json::obj([
            ("type", Json::from("string")),
            ("enum", Json::arr(SURFACES.map(Json::from))),
            ("description", Json::from("Which interface: the hosted game's, or the engine's review (F9) or gallery (F8) screen, or the editor (F7: its panels, the swarm panels included; ui_act on it is owner-only, one edit per call, refused while the human types into its Ask box). Default: the engine screen that is open, else the editor if open, else the game's.")),
        ])
    };
    let arr = |items: Json, max: usize, d: &str| Json::obj([("type", Json::from("array")), ("items", items), ("maxItems", Json::from(max)), ("description", Json::from(d))]);
    let short = |d: &str| Json::obj([("type", Json::from("string")), ("maxLength", Json::from(MAX_ID)), ("description", Json::from(d))]);
    let int = |lo: u64, hi: u64, d: &str| Json::obj([("type", Json::from("integer")), ("minimum", Json::from(lo)), ("maximum", Json::from(hi)), ("description", Json::from(d))]);
    let pair = |d: &str| arr(Json::obj([("type", Json::from("integer")), ("minimum", Json::from(64)), ("maximum", Json::from(MAX_FRAME_SIDE))]), 2, d);
    let obj = |props: Vec<(&str, Json)>| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::Arr(Vec::new()))]);
    let display = obj(vec![
        ("size", pair("[width, height] physical pixels (headless only: a window's size is the window's).")),
        ("os_scale", t("number", "The OS's scale for the monitor (Windows: DPI / 96), 0.5..5, or null for the platform's: a floor for the UI scale.")),
        ("safe_area", arr(Json::obj([("type", Json::from("number")), ("minimum", Json::from(0)), ("maximum", Json::from(2048))]), 4, "Insets [top, right, bottom, left] in physical px (notches, TV overscan); each at most 40% of its side.")),
        ("touch", t("boolean", "Touch is a primary input.")),
        ("diagonal_in", t("number", "A physical panel the size of the display, this many inches across (3..100, or null for none): a handheld (7 with 1280x800 is a Steam Deck) whose profile the game reads.")),
    ]);
    let step = Json::obj([
        ("type", Json::from("object")),
        (
            "description",
            Json::from(
                "Exactly one of: press (up, down, left, right, accept, back; an input action of the game such as \"pause\", pressed through its binding on the device; or a source such as \"Pad.Start\"), \
                 focus (a widget id: the d-pad route there), activate (a widget id: focus and accept with a gamepad or keyboard, or the HUD button's shortcut action; click or tap its centre with mouse or touch), \
                 click (a widget id or [x, y] px; mouse unless device is touch), type (text, at most 256 characters), scroll (notches, + is down; at a widget with the mouse, the right stick with a gamepad), wait (1..120 idle frames). \
                 Optional: device (overrides the call's), hold (press: 1..30 frames), at (scroll).",
            ),
        ),
        ("maxProperties", Json::from(3)),
    ]);
    let target = Json::obj([
        ("description", Json::from(format!("A preset ({}) or {{name, size: [w, h], ppi, os_scale, safe_area: [t, r, b, l], touch}}.", PRESETS.join(", ")))),
        ("anyOf", Json::arr([Json::obj([("type", Json::from("string")), ("enum", Json::arr(PRESETS.map(Json::from)))]), Json::obj([("type", Json::from("object"))])])),
    ]);
    vec![
        ToolDef {
            name: "ui_tree".into(),
            description: "The interface as data: the shown screens (layer, input menu/hud/none, modal, which one has the gamepad focus, which a modal blocks) and their laid-out widgets, a page at a time: \
                 id, type, rect [x, y, w, h] in physical px, text (labels), enabled / focused / action (buttons), clipped (+ visible_rect), overflow (text that doesn't fit its box), blocked (a modal above takes its input), path. \
                 Read from the last frame's layout when it was made for the current display (layout: \"last_frame\"), else laid out now with no input (layout: \"now\"; the player's interface is put back exactly). \
                 Pages: total, next_offset."
                .into(),
            input_schema: obj(vec![
                ("surface", surface()),
                ("screen", short("Only this screen.")),
                ("only", Json::obj([("type", Json::from("string")), ("enum", Json::arr(["named", "interactive", "all"].map(Json::from))), ("description", Json::from("named (default): widgets with an id, text, or that take input; interactive: buttons; all: layout boxes too."))])),
                ("offset", int(0, 100_000, "Skip this many widgets (a page's next_offset).")),
                ("limit", int(1, MAX_TREE_PAGE as u64, "Widgets per page (default 100).")),
                ("fresh", t("boolean", "Lay out now for the current display even if the last frame did.")),
                ("text_chars", int(1, specula_ui::state::MAX_TEXT as u64, "Longest text a widget carries, in characters (default 160; a page's longer texts are 64 KiB at most together, the rest cut at 160).")),
            ]),
        },
        ToolDef {
            name: "ui_check".into(),
            description: "Measured checks of an interface at several displays and UI scales (the screens laid out at each, the player's interface put back after): \
                 legibility (cap height in physical px against the Steam Deck rule, readable at 30 cm on an 800p screen), text_overflow and missing_glyph, contrast (WCAG: 4.5 body, 3 large) and non_text_contrast, \
                 overlap, offscreen (outside the safe area), overflow (out of its panel), touch_target (touch displays), focus_reachability (every menu widget reachable with the d-pad from the default focus), \
                 focus_scope (a modal that traps the gamepad with nothing to press, a menu with no way out), gamepad_access (HUD buttons need a shortcut with a gamepad binding, or a menu offering the same action), \
                 unbound (template keys the state lacks), unknown_texture, missing_key (a @key text the string table lacks), hardcoded (with a string table: words a screen writes itself), and with pseudo: true untranslated (labels whose words come from game state) while every rule measures text about 35% longer. \
                 Answer: errors, warnings, by_rule, findings (errors first; rule, severity, target or \"all\", screen, widget, message, measured, required)."
                .into(),
            input_schema: obj(vec![
                ("surface", surface()),
                ("screens", arr(short("A screen name."), MAX_SCREENS, "Screens shown together (in their layers) for the check. Default: the ones showing now.")),
                ("targets", arr(target, MAX_TARGETS, "Displays to lay out for. Default: deck (1280x800), 1080p, 4k.")),
                ("scales", arr(Json::obj([("type", Json::from("number")), ("minimum", Json::from(0.5)), ("maximum", Json::from(3))]), MAX_SCALES, "Player UI-scale settings to check each target at (default [1]). targets x scales at most 16.")),
                ("pseudo", t("boolean", "Lay text out in a pseudo-locale: the screens' own words accented and about 35% longer (overflow before anyone translates), and untranslated findings.")),
                ("locale", Json::obj([("type", Json::from("string")), ("maxLength", Json::from(specula_lingua::locale::MAX_LOCALE_BYTES)), ("description", Json::from("Check in this language (a locale with a string table: en, de, qps-ploc for the pseudo-locale; l10n_get lists them), with its words, numbers and faces; the player's language is put back after."))])),
                ("rules", arr(Json::obj([("type", Json::from("string")), ("enum", Json::arr(RULES.map(Json::from)))]), RULES.len(), "Only these rules' findings.")),
                ("max_findings", int(1, MAX_FINDINGS as u64, "Findings quoted (default 50; counts cover all).")),
            ]),
        },
        ToolDef {
            name: "ui_act".into(),
            description: "Drives the interface as a player would: each step becomes synthetic input (gamepad buttons and sticks, keys, mouse, touch) fed through the same frames a player's input goes through \
                 (the hosted game's frame, its controls and its UI; the engine's own screens for surface review or gallery, which only the owner may drive). focus and activate walk the d-pad route the focus rules give; \
                 a HUD button is activated on a gamepad by its shortcut action. The world changes the input causes (the commands a press sends) commit as the caller's, like a game tool's (claims are checked; refused, nothing changes); \
                 synthetic frames don't animate (the game's own presentation isn't the caller's). \
                 display: headless, this session's own virtual display (seen by this session's ui_* calls and ui_frame; other agents and sight_frame's include_ui keep the default; dropped when the session closes); with a window, only the owner may change the window's display (others simulate displays with ui_check targets or ui_frame's display). A failed or refused call keeps the display it had. \
                 Answer per step: did (the input sent), frames, actions (what the UI sent), refused (disabled widgets pressed), focused, screens shown; it stops at the first step that fails (error with what to change). \
                 Frames are budgeted by time (about 20 ms, at most 160 a call): past it the answer has stopped (time_budget or frame_cap), frames_left (of an interrupted wait) and rest, the steps still to run: call again with steps: rest."
                .into(),
            input_schema: obj(vec![
                ("surface", surface()),
                ("display", display.clone()),
                ("device", Json::obj([("type", Json::from("string")), ("enum", Json::arr(["gamepad", "keyboard", "mouse", "touch"].map(Json::from))), ("description", Json::from("The steps' device unless a step names its own (default gamepad)."))])),
                ("steps", arr(step, MAX_STEPS, "What to do, in order.")),
                ("settle", int(0, 10, "Idle frames after each step so what it opened is laid out (default 1).")),
            ]),
        },
        ToolDef {
            name: "ui_frame".into(),
            description: "A capture of what the player sees with every widget that has an id or takes input outlined and labelled (cyan: takes input; yellow, thick: has the gamepad focus; red: disabled; grey: has an id; faint and unlabelled: under a modal), \
                 the UI version of sight_frame. Size defaults to the display's (the window's, or this session's virtual one; up to 4K), the camera to the player's. display simulates another display for this capture alone (os_scale, safe_area, touch, size): nothing persists. \
                 overlays.ui lists the outlines with their rects, which come from the layout the capture drew (its frame record): surface, screen, id, type, rect, interactive, enabled, focused. The rest of the packet is sight_frame's."
                .into(),
            input_schema: obj(vec![
                ("size", pair("[width, height] physical pixels, at most 4K. Default: the display's.")),
                ("display", display.clone()),
                ("eye", t("array", "[x, y, z] metres: an explicit camera (with target).")),
                ("target", t("array", "[x, y, z] metres.")),
                ("fov_y_deg", t("number", "Vertical field of view, 10..120 (with eye and target).")),
                ("camera", short("A named camera (camera_set) instead of the player's.")),
                ("camera_of", short("Whose named camera.")),
                ("max_labels", int(0, 64, "Id labels drawn (default 64, the most); every outline is listed either way, and overlays.text.skipped counts ids left unlabelled.")),
                ("ids", t("boolean", "Also render the ID buffer (per-subject visible pixels). Default false.")),
                ("time_s", t("number", "Animation time to render.")),
            ]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, args: &str) -> Result<UiCall, String> {
        parse(name, &Json::parse(args).unwrap()).unwrap()
    }

    #[test]
    fn steps_parse_and_refuse_with_reasons() {
        let Ok(UiCall::Act(a)) = p("ui_act", r#"{"steps": [{"press": "accept"}, {"focus": "resume"}, {"activate": "speed_button", "device": "mouse"}, {"click": [10, 20]}, {"type": "hi"}, {"scroll": 3, "at": "list"}, {"wait": 5}]}"#) else { panic!() };
        assert_eq!(a.steps.len(), 7);
        assert_eq!(a.steps[0], Step::Press { name: "accept".into(), device: Device::Gamepad, hold: 1 });
        assert_eq!(a.steps[3], Step::Click { at: At::Point([10.0, 20.0]), device: Device::Mouse });
        for (bad, says) in [
            (r#"{"steps": [{"press": "accept", "focus": "x"}]}"#, "exactly one"),
            (r#"{"steps": [{"press": "accept", "device": "mouse"}]}"#, "gamepad or keyboard"),
            (r#"{"steps": [{"focus": "x", "device": "touch"}]}"#, "d-pad"),
            (r#"{"steps": [{"type": "a\nb"}]}"#, "control"),
            (r#"{"steps": [{"scroll": 0}]}"#, "not 0"),
            (r#"{"steps": [{"wait": 1000}]}"#, "1..120"),
            (r#"{"steps": [{"press": "accept", "hold": 99}]}"#, "hold"),
            (r#"{"display": {"safe_area": [1, 2, 3]}, "steps": []}"#, "4 numbers"),
            (r#"{"display": {"size": [10, 10]}, "steps": []}"#, "from 64"),
            (r#"{"steps": [{"click": "x", "device": "gamepad"}]}"#, "mouse or touch"),
        ] {
            let e = p("ui_act", bad).err().unwrap_or_else(|| panic!("{bad} parsed"));
            assert!(e.contains(says), "{bad}: {e}");
        }
        let many: Vec<String> = (0..=MAX_STEPS).map(|_| r#"{"wait": 1}"#.to_owned()).collect();
        assert!(p("ui_act", &format!(r#"{{"steps": [{}]}}"#, many.join(","))).is_err());
        let long = "x".repeat(MAX_TYPE_CHARS + 1);
        assert!(p("ui_act", &format!(r#"{{"steps": [{{"type": "{long}"}}]}}"#)).is_err());
    }

    #[test]
    fn check_targets_and_scales_are_bounded() {
        let Ok(UiCall::Check(c)) = p("ui_check", r#"{}"#) else { panic!() };
        assert_eq!(c.targets.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["deck_1280x800", "desktop_1920x1080", "desktop_3840x2160"]);
        let Ok(UiCall::Check(c)) = p("ui_check", r#"{"targets": ["deck", {"name": "tv", "size": [1920, 1080], "safe_area": [54, 96, 54, 96]}], "scales": [1, 1.5]}"#) else { panic!() };
        assert_eq!(c.targets.len(), 4);
        assert_eq!(c.targets[3].name, "tv@1.5x");
        assert_eq!(c.targets[1].safe_area, [54.0, 96.0, 54.0, 96.0]);
        for (bad, says) in [
            (r#"{"targets": ["8k"]}"#, "unknown preset"),
            (r#"{"targets": ["deck", "deck"]}"#, "twice"),
            (r#"{"scales": [1, 1]}"#, "twice"),
            (r#"{"scales": [9]}"#, "0.5 to 3"),
            (r#"{"targets": ["deck", "1080p", "4k", "4:3", "16:10"], "scales": [1, 1.25, 1.5, 2]}"#, "at most 16"),
            (r#"{"rules": ["vibes"]}"#, "unknown rule"),
            (r#"{"screens": ["hud", "hud"]}"#, "twice"),
        ] {
            let e = p("ui_check", bad).err().unwrap_or_else(|| panic!("{bad} parsed"));
            assert!(e.contains(says), "{bad}: {e}");
        }
    }

    #[test]
    fn frames_go_to_4k_and_no_further() {
        let Ok(UiCall::Frame(f)) = p("ui_frame", r#"{"size": [3840, 2160]}"#) else { panic!() };
        assert_eq!((f.frame.width, f.frame.height, f.frame.include_ui, f.player_camera), (3840, 2160, true, true));
        assert!(f.frame.overlays.has(crate::sight_overlay::Kind::Ui));
        assert_eq!(f.frame.overlays.max_labels, crate::sight_overlay::LABELS.1, "every outline labelled by default");
        assert!(p("ui_frame", r#"{"size": [4096, 4096]}"#).is_err());
        let Ok(UiCall::Frame(f)) = p("ui_frame", r#"{"eye": [0, 5, 5], "target": [0, 0, 0]}"#) else { panic!() };
        assert!(!f.player_camera);
        // A display for this capture alone (review R1 of m5-uitools, finding 1).
        let Ok(UiCall::Frame(f)) = p("ui_frame", r#"{"display": {"os_scale": 1.5, "safe_area": [40, 0, 40, 0], "touch": true}}"#) else { panic!() };
        assert_eq!(f.display, Some(DisplayArg { os_scale: Some(Some(1.5)), safe_area: Some([40.0, 0.0, 40.0, 0.0]), touch: Some(true), size: None, diagonal_in: None }));
        assert!(p("ui_frame", r#"{"display": {"size": [4096, 4096]}}"#).err().unwrap().contains("past 4K"));
        assert!(p("ui_frame", r#"{"display": {"dpi": 2}}"#).is_err());
    }

    #[test]
    fn every_tool_has_a_schema_with_limits() {
        let defs = definitions();
        assert_eq!(defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["ui_tree", "ui_check", "ui_act", "ui_frame"]);
        for d in &defs {
            assert!(parse(&d.name, &Json::obj::<&str>([])).is_some());
            // Arrays carry maxItems.
            for (k, v) in d.input_schema.get("properties").and_then(Json::as_object).unwrap() {
                if v.get("type").and_then(Json::as_str) == Some("array") && !["eye", "target"].contains(&k.as_str()) {
                    assert!(v.get("maxItems").is_some(), "{}.{k}", d.name);
                }
            }
        }
    }
}

//! The UI sight tools on the engine thread ([`crate::ui_tools`] has their arguments): which
//! interface a call looks at (the hosted game's through [`Game::ui`], the engine's review and
//! gallery screens), the display the game lays out for (the window's, or a virtual one headless),
//! and the synthetic input frames `ui_act` runs.
//!
//! **The display.** With a window it is the window's size and the OS's scale for its monitor
//! (`specula_platform::os_scale_of`, re-read a few times a second); headless it is a virtual
//! display (1280x800, the Deck). Agents may also set a size (headless), a safe area, an OS scale
//! and touch, to see the interface as on a TV or a phone: headless each session's own, seen only
//! by its `ui_*` calls and `ui_frame` captures; with a window only the owner's ([`UiState`]). The
//! game is told whenever it changes ([`Game::display`]).
//!
//! **ui_act's frames.** Each step becomes input for one or more frames: platform events (keys,
//! the mouse, touch contacts) and gamepad states, given to the game's `frame` exactly as the
//! window's are (the game's controls read them, its interface lays out and sends actions, the
//! game acts on them). They run inside one `Live::game_tool` as the caller, so their world
//! changes commit as the caller's, checked against claims like a game tool's (one call, one
//! commit). The engine's own screens get them through `review_frame` / `gallery_frame`, with
//! the gamepads the frames hold standing in for the polled ones (owner only: the review screen
//! decides as the window's owner).

use super::*;
use std::collections::BTreeMap;
use crate::authority::Authority;
use crate::game::{GameUi, UiPick};
use crate::sight_overlay::{self as ov, Painter};
use crate::ui_tools::{ActArgs, At, CheckArgs, Device, DisplayArg, FrameArgs, Step, Surface, TreeArgs, UiCall, MAX_FRAMES, MAX_OUTLINES};
use specula_input::{ActionMap, BindingInput, Source};
use specula_platform::{GamepadButton, GamepadState, MouseButton, PointerKind, RawGamepad};
use specula_ui::check::{check_with, report_json, CheckOpts, Severity};
use specula_ui::geom::Dir;
use specula_ui::screen::{InputMode, Kind as WidgetKind};
use specula_ui::{Display, StateMap, TreeOpts, Ui, WidgetRef};

/// The virtual display of a headless engine until an agent sets one: the Steam Deck's.
pub(crate) const HEADLESS_SIZE: [u32; 2] = [1280, 800];
/// How often a window's OS scale is read again.
const OS_SCALE_EVERY: Duration = Duration::from_millis(250);
/// Most actions a step's answer lists.
const MAX_STEP_ACTIONS: usize = 16;

/// How often closed sessions' display overrides are dropped.
const SWEEP_EVERY: Duration = Duration::from_secs(1);

/// The display state ([`Engine::ui_display_for`]).
///
/// **Who may change the display** (review R1 of m5-uitools, finding 1: any agent's `ui_act
/// {display}` reshaped the human's HUD for good, unjournaled, and headless one agent's display
/// became everyone's). Every override belongs to the session that set it and goes when that
/// session closes; nothing persists unattributed.
/// - **Headless:** each session has its own virtual display; the game is told the caller's
///   before each of its calls lays out.
/// - **With a window:** only the owner may override the window's display (the human sees it);
///   other agents simulate displays with `ui_check` targets and `ui_frame {display}`.
#[derive(Default)]
pub(crate) struct UiState {
    /// The display the game was last told.
    told: Option<Display>,
    /// A capture told the game its own display: the one to tell again after it.
    restore: Option<Display>,
    /// Headless: what each session set (`ui_act {display}`), with its agent's name.
    sessions: BTreeMap<String, (String, DisplayArg)>,
    /// With a window: the owner's override of the window's display (session, name, what it set).
    owner: Option<(String, String, DisplayArg)>,
    /// The window: its size and OS scale, and when the scale was read.
    window: Option<([u32; 2], Option<f32>, Instant)>,
    /// Headless: the camera the game's last synthetic frame chose (what `ui_frame` looks through).
    camera: Option<Camera>,
    last_sweep: Option<Instant>,
}

/// An override's fields over `d` (all but the size, which only a headless display takes).
fn overlay(d: &mut Display, a: &DisplayArg) {
    if let Some(o) = a.os_scale {
        d.os_scale = o;
    }
    if let Some(s) = a.safe_area {
        d.safe_area = s;
    }
    if let Some(t) = a.touch {
        d.touch = t;
    }
}

/// A display as answers show it, with who set what overrides it.
fn display_json(d: &Display, by: Option<&str>) -> Json {
    let mut j = d.to_json();
    if let Some(by) = by {
        j.set("set_by", Json::from(by));
    }
    j
}

/// What an agent's `display` changes in what agents set: given fields replace, absent ones stay;
/// `os_scale: null` drops the override (the platform's scale again).
fn apply_display(set: &mut DisplayArg, d: DisplayArg) {
    if d.size.is_some() {
        set.size = d.size;
    }
    if let Some(o) = d.os_scale {
        set.os_scale = o.map(Some);
    }
    if d.safe_area.is_some() {
        set.safe_area = d.safe_area;
    }
    if d.touch.is_some() {
        set.touch = d.touch;
    }
}

/// The display overrides before a `ui_act` changed them (put back if it fails).
struct DisplaySaved {
    session: Option<(String, DisplayArg)>,
    owner: Option<(String, String, DisplayArg)>,
}

/// Outlines under an engine screen that holds the window's input (review R1 of m5-uitools,
/// finding 3: with the review screen open the pause menu's buttons were listed as reachable,
/// labelled over the sheet, and focused beside the review's own focus): out of reach, so
/// blocked, and none of them has the gamepad focus.
fn block_under_modal(outlines: &mut [(&'static str, specula_ui::Outline)]) {
    for (_, o) in outlines {
        o.blocked = true;
        o.focused = false;
    }
}

/// Replaces, in a `ui_tree` answer, every label showing `shown` (what the human is typing, as the
/// screen draws it; clipped labels by their start) with a redaction.
fn redact_typing(tree: &mut Json, shown: &str) {
    let Some(Json::Arr(widgets)) = tree.get("widgets").cloned() else { return };
    let mut out = Vec::with_capacity(widgets.len());
    for mut w in widgets {
        let hit = w.get("text").and_then(Json::as_str).is_some_and(|t| {
            let t = t.trim_end_matches('\u{2026}');
            t == shown || (t.chars().count() >= 8 && shown.starts_with(t))
        });
        if hit {
            w.set("text", Json::from("(redacted: the owner's typing)"));
            w.set("redacted", Json::Bool(true));
        }
        out.push(w);
    }
    tree.set("widgets", Json::Arr(out));
}

/// One frame of synthetic input.
#[derive(Default)]
struct Synth {
    events: Vec<Event>,
    pad: Option<GamepadState>,
}

/// A pad holding `buttons` (and the right stick's y).
fn pad(buttons: u32, right_y: f32) -> GamepadState {
    let raw = RawGamepad { buttons, right_y: (right_y * 32767.0) as i16, ..RawGamepad::default() };
    GamepadState { connected: true, buttons, right_stick: [0.0, right_y], raw, ..GamepadState::default() }
}

fn key(name: &str) -> Source {
    Source::Key(specula_platform::keys::from_name(name).unwrap_or(0))
}

/// The inputs a press of `name` is on `device`: the sources held together (modifiers first).
fn press_sources(name: &str, device: Device, map: Option<&ActionMap>) -> Result<Vec<Source>, String> {
    let ui = |pad: GamepadButton, k: &str| if device == Device::Gamepad { Source::Pad(pad) } else { key(k) };
    let direct = match name {
        "up" => Some(ui(GamepadButton::DpadUp, "Up")),
        "down" => Some(ui(GamepadButton::DpadDown, "Down")),
        "left" => Some(ui(GamepadButton::DpadLeft, "Left")),
        "right" => Some(ui(GamepadButton::DpadRight, "Right")),
        "accept" => Some(ui(GamepadButton::South, "Enter")),
        "back" => Some(ui(GamepadButton::East, "Escape")),
        _ => None,
    };
    if let Some(s) = direct {
        return Ok(vec![s]);
    }
    if name.contains('.') {
        let s = Source::parse(name).ok_or_else(|| format!("'{}' is not a source (Pad.Start, Key.F)", crate::bounds::echo(name)))?;
        let fits = matches!((s, device), (Source::Pad(_), Device::Gamepad) | (Source::Key(_), Device::Keyboard));
        if !fits {
            return Err(format!("{name} isn't a {} button: a gamepad presses Pad.* buttons, a keyboard Key.* keys", device.name()));
        }
        return Ok(vec![s]);
    }
    let map = map.ok_or_else(|| format!("'{}' isn't up, down, left, right, accept or back, and this interface has no input map to find the action in", crate::bounds::echo(name)))?;
    let action = map.action(name).ok_or_else(|| format!("no input action '{}' (UI directions: up, down, left, right, accept, back; or a source such as Pad.Start)", crate::bounds::echo(name)))?;
    let button = |s: &Source| matches!((s, device), (Source::Pad(_), Device::Gamepad) | (Source::Key(_), Device::Keyboard));
    for b in &action.bindings {
        if let BindingInput::Source(s) = b.input {
            if button(&s) && b.modifiers.iter().all(button) {
                let mut out = b.modifiers.clone();
                out.push(s);
                return Ok(out);
            }
        }
    }
    Err(format!("action '{name}' has no {} button binding (it can't be pressed with that device)", device.name()))
}

/// Buttons and key events for holding `sources` down (`down`) or letting them go.
fn hold(sources: &[Source], down: bool, frame: &mut Synth) {
    let mut bits = frame.pad.map_or(0, |p| p.buttons);
    for s in sources {
        match *s {
            Source::Pad(b) => {
                if down {
                    bits |= b.bit();
                } else {
                    bits &= !b.bit();
                }
            }
            Source::Key(code) => frame.events.push(if down { Event::KeyDown { code, repeat: false } } else { Event::KeyUp { code } }),
            _ => {}
        }
    }
    if let Some(p) = &mut frame.pad {
        *p = pad(bits, p.right_stick[1]);
    }
}

/// What a driver gives the step runner: frames, and the interface between them.
trait Driver {
    /// Runs one frame with `input` (events in order, the pad if the device is a gamepad).
    fn frame(&mut self, input: &Synth);
    /// The live interface (laid out by the last frame) and its input map.
    fn ui(&mut self) -> Option<(&mut Ui, Option<&ActionMap>)>;
}

/// The hosted game, inside a `game_tool`.
struct GameDriver<'a, 'w> {
    game: &'a mut dyn Game,
    world: &'a mut game::GameWorld<'w>,
    clock: &'a Clock,
    agent: &'a str,
    windowed: bool,
    size: (u32, u32),
    camera: &'a mut Option<Camera>,
    /// The interface's state was brought up to date (by a frame, or the first look).
    filled: bool,
}

impl Driver for GameDriver<'_, '_> {
    fn frame(&mut self, input: &Synth) {
        let pads: &[GamepadState] = match &input.pad {
            Some(p) => std::slice::from_ref(p),
            None => &[],
        };
        let mut title = None;
        // Synthetic: the game makes only the writes this input causes (they are the caller's);
        // presentation waits for the window's next frame or a capture.
        let mut frame = game::Frame { input: &input.events, size: self.size, alpha: self.clock.alpha(), dt_s: 1.0 / 60.0, camera: self.camera, agent_view: false, title: &mut title, pads, synthetic: true };
        self.game.frame(&mut Ctx { world: self.world.reborrow(), clock: self.clock, agent: self.agent, windowed: self.windowed }, &mut frame);
        self.filled = true;
    }

    fn ui(&mut self) -> Option<(&mut Ui, Option<&ActionMap>)> {
        // A frame just filled the interface's state: read it as it left it.
        let pick = if self.filled { UiPick::Peek } else { UiPick::Live };
        self.filled = true;
        let g = self.game.ui(pick)?;
        Some((g.ui, g.input_map))
    }
}

/// The engine's review or gallery screen.
struct ScreenDriver<'a> {
    engine: &'a mut Engine,
    surface: Surface,
    size: (u32, u32),
}

impl Driver for ScreenDriver<'_> {
    fn frame(&mut self, input: &Synth) {
        let e = &mut *self.engine;
        e.input.clear();
        e.input.extend_from_slice(&input.events);
        e.logic.set_pad_override(Some(input.pad.into_iter().collect()));
        let modal = e.review_frame(self.size);
        if self.surface == Surface::Gallery || !modal {
            e.gallery_frame(self.size, modal);
        }
        e.logic.set_pad_override(None);
        e.input.clear();
    }

    fn ui(&mut self) -> Option<(&mut Ui, Option<&ActionMap>)> {
        let (ui, _, map) = match self.surface {
            Surface::Review => self.engine.review_ui.ui_parts()?,
            Surface::Gallery => self.engine.taste.screen.ui_parts()?,
            Surface::Game => return None,
        };
        Some((ui, map))
    }
}

/// Why the step runner stopped before the end.
#[derive(Debug)]
enum Halt {
    /// A step failed: its error code and what to change.
    Fail(String, String),
    /// Out of time ("time_budget") or frames ("frame_cap"): the rest runs in the next call.
    Stop(&'static str),
}

impl From<(String, String)> for Halt {
    fn from((code, message): (String, String)) -> Halt {
        Halt::Fail(code, message)
    }
}

/// Runs `ui_act`'s steps through a driver; the answer's `steps`, or the first failure (with the
/// steps before it). Frames are budgeted by time ([`crate::ui_tools::BUDGET_MS`]): before each
/// unit that can't be cut (a frame, a press and its release, a click), the frames so far say
/// what it would cost, and past the budget the runner stops there.
struct Runner<'d> {
    d: &'d mut dyn Driver,
    gamepad: bool,
    frames: usize,
    pad_now: GamepadState,
    started: Instant,
    budget: Duration,
    /// An interrupted wait's frames still to run.
    wait_left: Option<u32>,
    /// A step's frame ran in this call. Until one has, the next unit always runs (whatever a
    /// settle frame before it cost), so every call that answers `rest` made progress.
    stepped: bool,
}

impl Runner<'_> {
    /// Whether `n` more frames fit: within [`MAX_FRAMES`], and within the budget at the mean
    /// cost of this call's frames so far. The first step unit of a call always runs, even after
    /// a settle frame (review R2 of m5-uitools, finding 3: a slow settle frame used to stop the
    /// call before any step ran, answering a `rest` that made no progress). It always fits the
    /// cap: a unit is at most `MAX_HOLD + 1` frames, and a settle frame is one.
    fn room(&self, n: usize) -> Result<(), Halt> {
        if !self.stepped {
            return Ok(());
        }
        if self.frames + n > MAX_FRAMES {
            return Err(Halt::Stop("frame_cap"));
        }
        let spent = self.started.elapsed();
        let per = spent.as_secs_f64() / self.frames as f64;
        if spent.as_secs_f64() + per * n as f64 > self.budget.as_secs_f64() {
            return Err(Halt::Stop("time_budget"));
        }
        Ok(())
    }

    fn run(&mut self, input: Synth) -> Result<(Vec<String>, Vec<String>), Halt> {
        self.frames += 1;
        self.stepped = true;
        let mut input = input;
        if self.gamepad && input.pad.is_none() {
            input.pad = Some(self.pad_now);
        }
        if let Some(p) = input.pad {
            self.pad_now = p;
        }
        self.d.frame(&input);
        let mut actions = Vec::new();
        let mut refused = Vec::new();
        if let Some((ui, _)) = self.d.ui() {
            for e in &ui.last_events().events {
                match e {
                    specula_ui::UiEvent::Action { action, .. } => actions.push(action.to_string()),
                    specula_ui::UiEvent::Refused { widget } => refused.push(name_of(ui, *widget)),
                    specula_ui::UiEvent::Back { .. } => {}
                }
            }
        }
        Ok((actions, refused))
    }

    /// Idle frames (nothing pressed; the pad, if any, stays connected and released). Stopped by
    /// the budget, `wait_left` says how many were still to run.
    fn idle(&mut self, n: u32, acc: &mut Acc) -> Result<(), Halt> {
        for i in 0..n {
            if let Err(h) = self.room(1) {
                self.wait_left = Some(n - i);
                return Err(h);
            }
            let mut s = Synth::default();
            if self.gamepad {
                s.pad = Some(pad(0, 0.0));
            }
            acc.add(self.run(s)?);
        }
        Ok(())
    }

    /// Runs `frames` (a unit that can't be cut: a click, typed text, a stick held), if it fits.
    fn unit(&mut self, frames: Vec<Synth>, acc: &mut Acc) -> Result<(), Halt> {
        self.room(frames.len())?;
        for f in frames {
            acc.add(self.run(f)?);
        }
        Ok(())
    }

    /// Holds `sources` for `frames` frames, then lets them go (if all of it fits).
    fn press(&mut self, sources: &[Source], frames: u32, acc: &mut Acc) -> Result<(), Halt> {
        self.room(frames as usize + 1)?;
        for f in 0..frames {
            let mut s = Synth { events: Vec::new(), pad: self.gamepad.then_some(self.pad_now) };
            if f == 0 {
                hold(sources, true, &mut s);
            }
            acc.add(self.run(s)?);
        }
        let mut s = Synth { events: Vec::new(), pad: self.gamepad.then_some(self.pad_now) };
        hold(sources, false, &mut s);
        acc.add(self.run(s)?);
        Ok(())
    }
}

/// Actions and refusals across a step's frames.
#[derive(Default)]
struct Acc {
    actions: Vec<String>,
    refused: Vec<String>,
    frames: usize,
}

impl Acc {
    fn add(&mut self, (a, r): (Vec<String>, Vec<String>)) {
        self.frames += 1;
        for x in a {
            if self.actions.len() < MAX_STEP_ACTIONS {
                self.actions.push(x);
            }
        }
        for x in r {
            if self.refused.len() < MAX_STEP_ACTIONS {
                self.refused.push(x);
            }
        }
    }
}

/// A widget's id, or its screen and path.
fn name_of(ui: &Ui, w: WidgetRef) -> String {
    match ui.widget_id(w) {
        Some(id) => id.to_owned(),
        None => format!("{}:{}", ui.screen_name(w.screen), ui.widget_path(w).unwrap_or("?")),
    }
}

fn dir_name(d: Dir) -> &'static str {
    match d {
        Dir::Up => "up",
        Dir::Down => "down",
        Dir::Left => "left",
        Dir::Right => "right",
    }
}

fn ui_err(e: specula_ui::UiError) -> (String, String) {
    (e.code.as_str().to_owned(), e.message)
}

/// What the interface shows after a step: the focused widget and the screens, top first.
fn after(ui: &mut Ui) -> (Json, Json, Json) {
    let focused = ui.focused().map_or(Json::Null, |w| Json::from(name_of(ui, w)));
    let mut screens: Vec<Json> = ui.visible_screens().map(Json::from).collect();
    screens.reverse();
    (focused, Json::Arr(screens), ui.top_modal().map_or(Json::Null, Json::from))
}

fn find(ui: &Ui, id: &str) -> Result<WidgetRef, (String, String)> {
    ui.widget_shown(id).ok_or_else(|| {
        let shown: Vec<&str> = ui.visible_screens().collect();
        ("unknown_widget".to_owned(), format!("no widget '{}' on the screens showing ({})", crate::bounds::echo(id), shown.join(", ")))
    })
}

/// The centre of a widget as it shows (clipped by its list), and whether the pointer there hits it.
fn centre(ui: &Ui, w: WidgetRef) -> Result<[f32; 2], (String, String)> {
    ui.reachable(w).map_err(ui_err)?;
    let r = ui.rect(w).ok_or_else(|| ("not_visible".to_owned(), "the widget isn't laid out (hidden)".to_owned()))?;
    let c = r.center();
    match ui.hit_test(c) {
        Some(h) if h == w => Ok(c),
        Some(h) => Err(("covered".into(), format!("its centre [{:.0}, {:.0}] hits '{}' instead", c[0], c[1], name_of(ui, h)))),
        None => Err(("not_interactive".into(), format!("nothing takes a press at its centre [{:.0}, {:.0}] (not a button, or disabled)", c[0], c[1]))),
    }
}

fn pointer_frames(p: [f32; 2], device: Device) -> [Synth; 2] {
    let (x, y) = (p[0].round() as i32, p[1].round() as i32);
    match device {
        Device::Touch => [
            Synth { events: vec![Event::PointerDown { id: 1, kind: PointerKind::Touch, x, y }], pad: None },
            Synth { events: vec![Event::PointerUp { id: 1, kind: PointerKind::Touch, x, y }], pad: None },
        ],
        _ => [
            Synth { events: vec![Event::MouseMove { x, y }, Event::MouseDown { button: MouseButton::Left, x, y }], pad: None },
            Synth { events: vec![Event::MouseUp { button: MouseButton::Left, x, y }], pad: None },
        ],
    }
}

/// One step: what it did, or why it couldn't.
fn step(r: &mut Runner, s: &Step, acc: &mut Acc) -> Result<String, Halt> {
    match s {
        Step::Press { name, device, hold } => {
            let sources = {
                let map = r.d.ui().and_then(|(_, m)| m.cloned());
                press_sources(name, *device, map.as_ref()).map_err(|m| ("not_bound".to_owned(), m))?
            };
            r.press(&sources, *hold, acc)?;
            Ok(format!("pressed {}", sources.iter().map(|s| s.name()).collect::<Vec<_>>().join("+")))
        }
        Step::Focus { id, device } => focus(r, id, *device, acc).map(|n| format!("{n} d-pad presses")),
        Step::Activate { id, device } if device.pointer() => {
            let p = {
                let (ui, _) = r.d.ui().ok_or_else(no_ui)?;
                let w = find(ui, id)?;
                centre(ui, w)?
            };
            r.unit(pointer_frames(p, *device).into_iter().collect(), acc)?;
            Ok(format!("{} at [{:.0}, {:.0}]", if *device == Device::Touch { "tapped" } else { "clicked" }, p[0], p[1]))
        }
        Step::Activate { id, device } => {
            // A widget on the screen the d-pad moves in: focus it, then accept. A HUD button: its
            // declared gamepad path, the shortcut action.
            let shortcut = {
                let (ui, _) = r.d.ui().ok_or_else(no_ui)?;
                let w = find(ui, id)?;
                ui.reachable(w).map_err(ui_err)?;
                let screen = ui.screen(specula_ui::ScreenId(w.screen));
                let in_scope = ui.focus_screen() == Some(w.screen);
                match &screen.nodes[w.node as usize].kind {
                    WidgetKind::Button { shortcut: Some(a), .. } if !in_scope && screen.input == InputMode::Hud => Some(a.clone()),
                    _ if in_scope => None,
                    _ => {
                        return Err(Halt::Fail(
                            "not_reachable".into(),
                            format!("'{id}' is on \"{}\", which the d-pad doesn't move in, and has no shortcut action: activate it with device mouse or touch, or give it a \"shortcut\"", screen.name),
                        ))
                    }
                }
            };
            match shortcut {
                Some(a) => {
                    let sources = {
                        let map = r.d.ui().and_then(|(_, m)| m.cloned());
                        press_sources(&a, *device, map.as_ref()).map_err(|m| ("not_bound".to_owned(), format!("its shortcut: {m}")))?
                    };
                    r.press(&sources, 1, acc)?;
                    Ok(format!("pressed its shortcut '{a}' ({})", sources.iter().map(|s| s.name()).collect::<Vec<_>>().join("+")))
                }
                None => {
                    let n = focus(r, id, *device, acc)?;
                    let accept = press_sources("accept", *device, None).map_err(|m| ("not_bound".to_owned(), m))?;
                    r.press(&accept, 1, acc)?;
                    Ok(format!("{n} d-pad presses, then accept"))
                }
            }
        }
        Step::Click { at, device } => {
            let p = match at {
                At::Point(p) => *p,
                At::Widget(id) => {
                    let (ui, _) = r.d.ui().ok_or_else(no_ui)?;
                    let w = find(ui, id)?;
                    centre(ui, w)?
                }
            };
            r.unit(pointer_frames(p, *device).into_iter().collect(), acc)?;
            Ok(format!("{} at [{:.0}, {:.0}]", if *device == Device::Touch { "tapped" } else { "clicked" }, p[0], p[1]))
        }
        Step::Type { text } => {
            r.unit(vec![Synth { events: text.chars().map(Event::Char).collect(), pad: None }], acc)?;
            Ok(format!("typed {} characters", text.chars().count()))
        }
        Step::Scroll { amount, at, device: Device::Gamepad } => {
            let _ = at;
            let y = if *amount > 0 { -1.0 } else { 1.0 };
            let mut frames: Vec<Synth> = (0..amount.unsigned_abs()).map(|_| Synth { events: Vec::new(), pad: Some(pad(r.pad_now.buttons, y)) }).collect();
            frames.push(Synth { events: Vec::new(), pad: Some(pad(r.pad_now.buttons, 0.0)) });
            r.unit(frames, acc)?;
            Ok(format!("right stick {} for {} frames", if *amount > 0 { "down" } else { "up" }, amount.unsigned_abs()))
        }
        Step::Scroll { amount, at, .. } => {
            let p = match at {
                Some(id) => {
                    let (ui, _) = r.d.ui().ok_or_else(no_ui)?;
                    let w = find(ui, id)?;
                    let r = ui.rect(w).ok_or_else(|| ("not_visible".to_owned(), format!("'{id}' isn't laid out")))?;
                    r.center()
                }
                None => {
                    let (ui, _) = r.d.ui().ok_or_else(no_ui)?;
                    ui.last_view().map_or([0.0, 0.0], |v| v.safe().center())
                }
            };
            let (x, y) = (p[0].round() as i32, p[1].round() as i32);
            r.unit(vec![Synth { events: vec![Event::MouseMove { x, y }, Event::MouseWheel { delta: -(*amount as f32) }], pad: None }], acc)?;
            Ok(format!("wheel {amount} notches at [{x}, {y}]"))
        }
        Step::Wait { frames } => {
            r.idle(*frames, acc)?;
            Ok(format!("waited {frames} frames"))
        }
    }
}

fn no_ui() -> (String, String) {
    ("no_ui".into(), "the interface is gone (its screens failed to load?)".into())
}

/// The d-pad (or arrow keys) to widget `id`, re-planning after each press (a list scrolls, a
/// layout moves). Returns the presses made.
fn focus(r: &mut Runner, id: &str, device: Device, acc: &mut Acc) -> Result<usize, Halt> {
    let mut presses = 0;
    loop {
        let next = {
            let (ui, _) = r.d.ui().ok_or_else(no_ui)?;
            let w = find(ui, id)?;
            ui.focus_route(w).map_err(ui_err)?.first().copied()
        };
        let Some(d) = next else { return Ok(presses) };
        if presses >= specula_ui::agent::MAX_ROUTE {
            return Err(Halt::Fail("not_reachable".into(), format!("still not on '{id}' after {presses} presses")));
        }
        let sources = press_sources(dir_name(d), device, None).map_err(|m| ("not_bound".to_owned(), m))?;
        r.press(&sources, 1, acc)?;
        presses += 1;
    }
}

/// What the step runner did: the per-step answers, the failure that stopped it, why it stopped
/// early (out of time or frames) with the steps still to run, and its frames.
struct Ran {
    steps: Vec<Json>,
    failed: Option<(String, String)>,
    stopped: Option<&'static str>,
    rest: Vec<Step>,
    wait_left: u32,
    frames: usize,
}

/// Runs every step within the budget (see [`Runner`]).
fn run_steps(d: &mut dyn Driver, args: &ActArgs, first_settle: bool, budget: Duration) -> Ran {
    let gamepad = args.steps.iter().any(|s| matches!(s, Step::Press { device: Device::Gamepad, .. } | Step::Focus { device: Device::Gamepad, .. } | Step::Activate { device: Device::Gamepad, .. } | Step::Scroll { device: Device::Gamepad, .. }));
    let mut r = Runner { d, gamepad, frames: 0, pad_now: pad(0, 0.0), started: Instant::now(), budget, wait_left: None, stepped: false };
    let mut ran = Ran { steps: Vec::new(), failed: None, stopped: None, rest: Vec::new(), wait_left: 0, frames: 0 };
    if first_settle {
        let mut acc = Acc::default();
        if let Err(h) = r.idle(1, &mut acc) {
            if let Halt::Fail(c, m) = h {
                ran.failed = Some((c, m));
            }
            ran.frames = r.frames;
            return ran;
        }
        // The settle frame isn't a step: the first step unit still runs.
        r.stepped = false;
    }
    for (i, s) in args.steps.iter().enumerate() {
        let mut acc = Acc::default();
        r.wait_left = None;
        // The step, then its settle frames (a settle cut short leaves the step done).
        let did = step(&mut r, s, &mut acc);
        let (did, settled) = match did {
            Ok(d) => (Ok(d), r.idle(args.settle, &mut acc)),
            Err(h) => (Err(h), Ok(())),
        };
        let (focused, screens, modal) = match r.d.ui() {
            Some((ui, _)) => after(ui),
            None => (Json::Null, Json::Arr(Vec::new()), Json::Null),
        };
        let mut j = vec![("step", Json::from(i)), ("frames", Json::from(acc.frames))];
        let mut halt = None;
        match did {
            Ok(d) => {
                j.push(("did", Json::from(d)));
                if let Err(h) = settled {
                    halt = Some((h, false));
                }
            }
            Err(h) => halt = Some((h, true)),
        }
        j.push(("actions", Json::arr(acc.actions.into_iter().map(Json::from))));
        if !acc.refused.is_empty() {
            j.push(("refused", Json::arr(acc.refused.into_iter().map(Json::from))));
        }
        j.push(("focused", focused));
        j.push(("screens", screens));
        if !modal.is_null() {
            j.push(("modal", modal));
        }
        match halt {
            None => ran.steps.push(Json::obj(j)),
            Some((Halt::Fail(code, message), _)) => {
                j.push(("error", Json::from(code.as_str())));
                j.push(("message", Json::from(message.as_str())));
                ran.steps.push(Json::obj(j));
                ran.failed = Some((code, message));
                break;
            }
            Some((Halt::Stop(why), in_step)) => {
                j.push(("stopped", Json::from(why)));
                ran.steps.push(Json::obj(j));
                ran.stopped = Some(why);
                // What of this step is left: an interrupted wait's frames; a focus or activate
                // again (its route is planned afresh); any other step whole (it hadn't begun:
                // presses, clicks and scrolls are never cut). A cut settle leaves it done.
                if in_step {
                    match (s, r.wait_left) {
                        (Step::Wait { .. }, Some(left)) => {
                            ran.wait_left = left;
                            ran.rest.push(Step::Wait { frames: left });
                        }
                        _ => ran.rest.push(s.clone()),
                    }
                }
                ran.rest.extend(args.steps[i + 1..].iter().cloned());
                break;
            }
        }
    }
    ran.frames = r.frames;
    ran
}

/// `ui_act`'s game frames as [`Engine::ui_act_frames`] runs them inside its `game_tool` (the
/// write-policy table runs them on its own world): the failure that stopped the steps, if any.
#[cfg(test)]
pub(crate) fn act_in_game(game: &mut dyn Game, gw: &mut game::GameWorld, clock: &Clock, agent: &str, a: &ActArgs) -> Option<(String, String)> {
    let mut camera = None;
    let mut d = GameDriver { game, world: gw, clock, agent, windowed: false, size: (1280, 800), camera: &mut camera, filled: false };
    run_steps(&mut d, a, false, Duration::from_secs(1)).failed
}

/// `ui_check` in slices: one target per step.
struct CheckJob {
    a: CheckArgs,
    surface: Surface,
    /// Resolved at the first step (the screens showing then, unless named).
    screens: Option<Vec<String>>,
    next: usize,
    /// Per-target findings, in order.
    found: Vec<specula_ui::Finding>,
    /// Size-independent findings and the targets they held at.
    global: Vec<(specula_ui::Finding, Vec<String>)>,
    spent: Duration,
    /// The caller's session (its display).
    session: String,
}

impl steps::StepJob for CheckJob {
    fn kind(&self) -> &'static str {
        "ui_check"
    }

    fn step(&mut self, engine: &mut Engine, _reply: &jobs::Reply) -> steps::Step {
        let started = Instant::now();
        let target = self.a.targets[self.next].clone();
        if let (Some(l), None) = (&self.a.locale, &self.a.strings) {
            // The string tables are read on the caller's connection (crate::l10n_tools); a
            // playtest step runs on the engine thread, which can't wait for them.
            return steps::Step::Done(world_error(&WorldError::new(
                "locale_unavailable",
                format!("ui_check {{locale: \"{}\"}} reads the project's string tables on your connection, which a playtest step can't: run it outside the playtest (or check pseudo: true inside one)", crate::bounds::echo(l)),
            )));
        }
        let opts = CheckOpts { pseudo: self.a.pseudo, strings: self.a.strings.clone() };
        let want = self.screens.clone().or_else(|| self.a.screens.clone());
        let display = engine.ui_display_for(Some(&self.session)).0;
        let r = engine.with_ui(self.surface, display, |ui, state, _, map, _| {
            let screens: Vec<String> = match want {
                Some(v) => v,
                None => ui.visible_screens().map(str::to_owned).collect(),
            };
            if screens.is_empty() {
                return Err(WorldError::new("nothing_showing", "no screen is showing: name the screens to check ('screens')"));
            }
            let names: Vec<&str> = screens.iter().map(String::as_str).collect();
            let f = check_with(ui, &names, state, map, std::slice::from_ref(&target), &opts).map_err(|e| {
                let loaded: Vec<String> = (0..ui.screen_count()).map(|i| ui.screen_name(i).to_owned()).collect();
                WorldError::new("unknown_screen", e.message).with("loaded", Json::arr(loaded.into_iter().map(Json::from)))
            })?;
            Ok((f, screens))
        });
        let (findings, screens) = match r.and_then(|x| x) {
            Ok(x) => x,
            Err(e) => return steps::Step::Done(world_error(&e)),
        };
        self.screens = Some(screens);
        for f in findings {
            if f.target == "all" {
                match self.global.iter_mut().find(|(g, _)| g.rule == f.rule && g.screen == f.screen && g.widget == f.widget && g.message == f.message) {
                    Some((_, at)) => at.push(target.name.clone()),
                    None => self.global.push((f, vec![target.name.clone()])),
                }
            } else {
                self.found.push(f);
            }
        }
        self.spent += started.elapsed();
        self.next += 1;
        if self.next < self.a.targets.len() {
            return steps::Step::More;
        }
        steps::Step::Done(self.answer())
    }
}

impl CheckJob {
    fn answer(&mut self) -> ToolOutput {
        let a = &self.a;
        let n = a.targets.len();
        let mut findings = std::mem::take(&mut self.found);
        for (mut g, at) in std::mem::take(&mut self.global) {
            if at.len() < n {
                g.target = at.join(",");
            }
            findings.push(g);
        }
        findings.sort_by(|x, y| y.severity.cmp(&x.severity).then_with(|| x.rule.cmp(y.rule)));
        let kept: Vec<_> = findings.into_iter().filter(|f| a.rules.as_ref().is_none_or(|r| r.iter().any(|x| x == f.rule))).collect();
        let mut by_rule: std::collections::BTreeMap<&str, (usize, usize)> = std::collections::BTreeMap::new();
        for f in &kept {
            let e = by_rule.entry(f.rule).or_default();
            if f.severity == Severity::Error {
                e.0 += 1;
            } else {
                e.1 += 1;
            }
        }
        let shown = &kept[..kept.len().min(a.max_findings)];
        let mut j = report_json(shown);
        let errors = kept.iter().filter(|f| f.severity == Severity::Error).count();
        j.set("errors", Json::from(errors));
        j.set("warnings", Json::from(kept.len() - errors));
        if kept.len() > shown.len() {
            j.set("findings_left_out", Json::from(kept.len() - shown.len()));
        }
        j.set("by_rule", Json::obj(by_rule.into_iter().map(|(r, (e, w))| (r, Json::obj([("errors", Json::from(e)), ("warnings", Json::from(w))])))));
        j.set("surface", Json::from(self.surface.name()));
        j.set("screens", Json::arr(self.screens.take().unwrap_or_default().into_iter().map(Json::from)));
        j.set("targets", Json::arr(a.targets.iter().map(|t| Json::obj([("name", Json::from(t.name.as_str())), ("display", t.display().to_json()), ("ui_scale", Json::from(f64::from(t.user_scale)))]))));
        if a.pseudo {
            j.set("pseudo_locale", Json::Bool(true));
        }
        if let Some(s) = &a.strings {
            j.set("locale", Json::from(s.locale().as_str()));
        }
        // Engine-thread time across the slices.
        j.set("ms", Json::from((self.spent.as_secs_f64() * 1e4).round() / 10.0));
        ToolOutput::text(j.to_string())
    }
}

impl Engine {
    /// The display `session` sees ([`UiState`]), and the agent whose override is in it: with a
    /// window, the window's with the owner's override; headless, the session's own virtual
    /// display (`None`: the default one, the Deck's).
    pub(crate) fn ui_display_for(&self, session: Option<&str>) -> (Display, Option<&str>) {
        if self.windowed {
            let mut d = match self.ui_state.window {
                Some((size, os, _)) => Display { os_scale: os, ..Display::new(size) },
                None => Display::new(HEADLESS_SIZE),
            };
            let by = self.ui_state.owner.as_ref().map(|(_, name, a)| {
                overlay(&mut d, a);
                name.as_str()
            });
            return (d, by);
        }
        let set = session.and_then(|s| self.ui_state.sessions.get(s));
        let mut d = Display::new(set.and_then(|(_, a)| a.size).unwrap_or(HEADLESS_SIZE));
        if let Some((_, a)) = set {
            overlay(&mut d, a);
        }
        (d, set.map(|(name, _)| name.as_str()))
    }

    /// Tells the hosted game `d`, if it isn't what it was last told.
    fn ui_tell(&mut self, d: Display) {
        if self.ui_state.told == Some(d) {
            return;
        }
        self.ui_state.told = Some(d);
        if let Some(g) = self.game.as_mut() {
            g.display(&d);
        }
    }

    /// Tells the hosted game the display (the window's; headless the default one), if it changed
    /// since it was last told.
    pub(crate) fn ui_tell_display(&mut self) {
        let d = self.ui_display_for(None).0;
        self.ui_tell(d);
    }

    /// The display a capture's interface is drawn for, told the game for this capture: its own
    /// (`ui_frame`: the caller's, or one for this capture alone), else the default one (the
    /// window's; headless the Deck's). Never whatever the game was told last: headless, that was
    /// the last `ui_act` caller's display (review R2 of m5-uitools, finding 2: one session's safe
    /// area squeezed the HUD in other agents' `sight_frame {include_ui}`).
    /// Returns whether it was told (then [`Engine::ui_capture_display_done`] tells the last one again).
    pub(crate) fn ui_capture_display(&mut self, d: Option<Display>) -> bool {
        let d = d.unwrap_or_else(|| self.ui_display_for(None).0);
        if self.ui_state.told == Some(d) {
            return false;
        }
        self.ui_state.restore = self.ui_state.told;
        self.ui_tell(d);
        true
    }

    pub(crate) fn ui_capture_display_done(&mut self, told: bool) {
        if !told {
            return;
        }
        match self.ui_state.restore.take() {
            Some(d) => self.ui_tell(d),
            None => self.ui_tell_display(),
        }
    }

    /// Drops the display overrides of sessions that closed (at most once a second).
    pub(crate) fn ui_sweep(&mut self, server: &Server) {
        if self.ui_state.sessions.is_empty() && self.ui_state.owner.is_none() {
            return;
        }
        let now = Instant::now();
        if self.ui_state.last_sweep.is_some_and(|t| now - t < SWEEP_EVERY) {
            return;
        }
        self.ui_state.last_sweep = Some(now);
        let alive: BTreeSet<String> = server.all_sessions().into_iter().map(|s| s.id).collect();
        self.ui_forget_sessions(&alive);
    }

    /// Keeps only the overrides of sessions in `alive`; the window's display is told again when
    /// the owner's override went.
    pub(crate) fn ui_forget_sessions(&mut self, alive: &BTreeSet<String>) {
        self.ui_state.sessions.retain(|s, _| alive.contains(s));
        if self.ui_state.owner.as_ref().is_some_and(|(s, _, _)| !alive.contains(s)) {
            self.ui_state.owner = None;
            self.ui_tell_display();
        }
    }

    /// A window frame: the window's size (not 0x0) and, now and then, its monitor's OS scale.
    pub(crate) fn ui_window(&mut self, raw: specula_platform::RawWindow, size: (u32, u32)) {
        if size.0 == 0 || size.1 == 0 {
            return;
        }
        let now = Instant::now();
        let os = match self.ui_state.window {
            Some((_, os, at)) if now - at < OS_SCALE_EVERY => (os, at),
            _ => (specula_platform::os_scale_of(raw), now),
        };
        self.ui_state.window = Some(([size.0, size.1], os.0, os.1));
        self.ui_tell_display();
    }

    /// The surface a call means: the one named, else the engine screen that is open, else the game's.
    fn ui_surface_of(&self, s: Option<Surface>) -> Surface {
        s.unwrap_or(if self.review_ui.is_open() {
            Surface::Review
        } else if self.taste.screen.is_open() {
            Surface::Gallery
        } else {
            Surface::Game
        })
    }

    /// Runs `f` on a surface's live interface, its state, the player's UI scale, its input map
    /// and `display` (the caller's).
    fn with_ui<R>(&mut self, s: Surface, display: Display, f: impl FnOnce(&mut Ui, &StateMap, f32, Option<&ActionMap>, Display) -> R) -> Result<R, WorldError> {
        match s {
            Surface::Game => {
                let Some(game) = self.game.as_mut() else {
                    return Err(WorldError::new("no_ui", "no game is running: the engine's own screens are surface review (F9) and gallery (F8)"));
                };
                let Some(GameUi { ui, state, user_scale, input_map }) = game.ui(UiPick::Live) else {
                    return Err(WorldError::new("no_ui", "the hosted game has no specula-ui interface (or it is off: no font?)"));
                };
                Ok(f(ui, state, user_scale, input_map, display))
            }
            Surface::Review | Surface::Gallery => {
                // The engine's screens lay out for the image's size alone (no OS scale or insets).
                let d = Display::new(display.size);
                let parts = if s == Surface::Review { self.review_ui.ui_parts() } else { self.taste.screen.ui_parts() };
                let Some((ui, state, map)) = parts else {
                    return Err(WorldError::new("no_ui", format!("the {} screen hasn't loaded yet (it loads when first opened)", s.name())));
                };
                Ok(f(ui, state, 1.0, map, d))
            }
        }
    }

    pub(crate) fn ui_call(&mut self, call: UiCall, w: Writer, tick: u64, session: &SessionInfo) -> ToolOutput {
        let out = match call {
            UiCall::Tree(a) => self.ui_tree(a, session),
            UiCall::Check(_) => unreachable!("ui_check is a step job (start_ui_check)"),
            UiCall::Locale(reply) => {
                let mut l = self.game.as_ref().map(|g| g.locale()).unwrap_or_default();
                if let Some(k) = &mut l.keys {
                    k.truncate(crate::game::MAX_GAME_KEYS);
                }
                let _ = reply.send(l);
                Ok(ToolOutput::text("{}"))
            }
            UiCall::Act(a) => self.ui_act(a, w, tick, session),
            UiCall::Frame(_) => unreachable!("ui_frame goes through handle_sight"),
        };
        out.unwrap_or_else(|e| world_error(&e))
    }

    fn ui_tree(&mut self, a: TreeArgs, session: &SessionInfo) -> Result<ToolOutput, WorldError> {
        let s = self.ui_surface_of(a.surface);
        let opts = TreeOpts { screen: a.screen.clone(), only: a.only, offset: a.offset, limit: a.limit };
        let (shown, by) = self.ui_display_for(Some(&session.id));
        let by = by.map(str::to_owned);
        let mut tree = self.with_ui(s, shown, |ui, state, scale, _, display| {
            if let Some(n) = &opts.screen {
                if ui.screen_id(n).is_none() {
                    return Err(WorldError::new("unknown_screen", format!("no screen '{}' in the {} interface", crate::bounds::echo(n), s.name())));
                }
            }
            let view = display.viewport(ui.theme(), scale);
            let (mut t, how) = if !a.fresh && ui.last_view() == Some(view) { (ui.tree_page(&opts), "last_frame") } else { (ui.inspect(state, &view, |u| u.tree_page(&opts)), "now") };
            t.set("layout", Json::from(how));
            t.set("display", display_json(&display, by.as_deref()));
            t.set("ui_scale", Json::from(f64::from(scale)));
            Ok(t)
        })??;
        tree.set("surface", Json::from(s.name()));
        // The note the human is typing on the gallery screen is theirs alone (input sweep R3).
        if s == Surface::Gallery && authority::authority(&self.swarm, &session.id) < Authority::Owner {
            if let Some(shown) = self.taste.screen.note_shown() {
                redact_typing(&mut tree, &shown);
            }
        }
        Ok(ToolOutput::text(tree.to_string()))
    }

    /// `ui_check` as a step job: one target's layout and rules per slice (a 3-screen set takes a
    /// few milliseconds per layout; 16 layouts in one call would hold the engine thread past its
    /// budget). Findings that hold whatever the size ("all") are merged across the slices.
    pub(crate) fn start_ui_check(&mut self, request: Request) {
        let Request { command: Command::Ui(UiCall::Check(a)), session, reply, deadline, call, .. } = request else { unreachable!("only ui_check comes here") };
        let surface = self.ui_surface_of(a.surface);
        let job = CheckJob { a, surface, screens: None, next: 0, found: Vec::new(), global: Vec::new(), spent: Duration::ZERO, session: session.id.clone() };
        let owner = jobs::JobOwner { name: self.swarm.agent_name(&session), key: self.swarm.key(&session) };
        self.step_job(steps::StepStart { owner, session: session.id.clone(), call, reply, deadline, class: schedule::Class::Big }, Box::new(job));
    }

    /// `ui_act {display}`: the caller's override changes ([`UiState`]). With a window only the
    /// owner may (the human sees it), and not the size (the window's). Returns what to put back
    /// if the call fails.
    fn ui_set_display(&mut self, d: DisplayArg, session: &SessionInfo, owner: bool) -> Result<DisplaySaved, WorldError> {
        let saved = DisplaySaved { session: self.ui_state.sessions.get(&session.id).cloned(), owner: self.ui_state.owner.clone() };
        let name = self.swarm.agent_name(session);
        if self.windowed {
            if !owner {
                return Err(WorldError::new(
                    "owner_only",
                    "the window's display is the human's: only the owner may override it. Simulate other displays with ui_check targets (size, os_scale, safe_area, touch) or a capture's own display (ui_frame {display})",
                ));
            }
            if let (Some(size), Some((win, _, _))) = (d.size, self.ui_state.window) {
                if size != win {
                    return Err(WorldError::new("window_size", format!("the window is {}x{}: its interface lays out for that (resize the window, or run headless for another size)", win[0], win[1])).with("size", Json::arr(win.map(Json::from))));
                }
            }
            let mut arg = self.ui_state.owner.take().map(|(_, _, a)| a).unwrap_or_default();
            apply_display(&mut arg, d);
            arg.size = None;
            self.ui_state.owner = (arg != DisplayArg::default()).then(|| (session.id.clone(), name, arg));
        } else {
            let mut arg = self.ui_state.sessions.remove(&session.id).map(|(_, a)| a).unwrap_or_default();
            apply_display(&mut arg, d);
            if arg != DisplayArg::default() {
                self.ui_state.sessions.insert(session.id.clone(), (name, arg));
            }
        }
        Ok(saved)
    }

    /// Puts the display overrides back as they were before a call that failed.
    fn ui_restore_display(&mut self, saved: DisplaySaved, session: &SessionInfo) {
        match saved.session {
            Some(v) => {
                self.ui_state.sessions.insert(session.id.clone(), v);
            }
            None => {
                self.ui_state.sessions.remove(&session.id);
            }
        }
        self.ui_state.owner = saved.owner;
        // (With a window the owner's override is the window's display: tell it again.)
        self.ui_tell_display();
    }

    fn ui_act(&mut self, a: ActArgs, w: Writer, tick: u64, session: &SessionInfo) -> Result<ToolOutput, WorldError> {
        let started = Instant::now();
        let s = self.ui_surface_of(a.surface);
        let owner = authority::authority(&self.swarm, &session.id) == Authority::Owner;
        // No surface named while an engine screen is open (the owner pressed F8 or F9, a reviewer
        // opened one): the call meant the game, and the screen takes the window's input. That is
        // state the agent didn't choose: refused as engine_screen_open, seen, never counted
        // (review R1 of m5-inputguard, major 2: a human pressing F9 froze every playtester).
        if a.surface.is_none() && s != Surface::Game && !owner {
            let e = WorldError::new("engine_screen_open", format!("the {} screen is open on the window and takes all its input: a player couldn't reach the game now (the owner closes it)", s.name())).with("screen", s.name());
            crate::inputguard::declined(crate::inputguard::Channel::ScreenInput, "ui_act", &crate::inputguard::Actor::of(&self.swarm, session), e.code, s.name());
            return Err(e);
        }
        if s != Surface::Game && !owner {
            let e = authority::refusal(
                authority::Site::ScreenAct,
                format!("the {} screen is the owner's (its buttons decide as the window's owner): only an agent the operator granted --owner may drive it; a refused try freezes your session (docs/design/SECURITY.md)", s.name()),
            );
            // A refused try at driving the human's screen: a security incident.
            crate::inputguard::refused(crate::inputguard::Channel::ScreenInput, "ui_act", &crate::inputguard::Actor::of(&self.swarm, session), e.code, s.name());
            return Err(e);
        }
        if s != Surface::Game {
            // The owner's own (an --owner agent): seen and recorded, fine.
            let _seen = crate::inputguard::done(crate::inputguard::Channel::ScreenInput, "ui_act", &crate::inputguard::Actor::of(&self.swarm, session), s.name());
        }
        if s == Surface::Game {
            if let Some(open) = [(self.review_ui.is_open(), "review"), (self.taste.screen.is_open(), "gallery")].iter().find(|(o, _)| *o).map(|(_, n)| *n) {
                return Err(WorldError::new("engine_screen_open", format!("the {open} screen is open on the window and takes all its input: a player couldn't reach the game now (the owner closes it)")).with("screen", open));
            }
        }
        let saved = match a.display {
            Some(d) => Some(self.ui_set_display(d, session, owner)?),
            None => None,
        };
        // The caller's display, for this call's frames; the default one is told again after them
        // (headless, nothing else may see a session's display: finding 2 of review R2).
        let (display, by) = self.ui_display_for(Some(&session.id));
        let by = by.map(str::to_owned);
        self.ui_tell(display);
        let out = self.ui_act_frames(&a, s, display, w, tick, started);
        self.ui_tell_display();
        let (ran, failed) = match out {
            Ok(r) => {
                let failed = r.failed.is_some();
                (r, failed)
            }
            Err(e) => {
                if let Some(saved) = saved {
                    self.ui_restore_display(saved, session);
                }
                return Err(e);
            }
        };
        if failed {
            // A failed call changes no display either.
            if let Some(saved) = saved {
                self.ui_restore_display(saved, session);
            }
        }
        let (focused, screens, modal) = self.with_ui(s, display, |ui, _, _, _, _| after(ui))?;
        let mut j = vec![
            ("surface", Json::from(s.name())),
            ("display", display_json(&display, by.as_deref())),
            ("frames", Json::from(ran.frames)),
            ("ms", Json::from((started.elapsed().as_secs_f64() * 1e4).round() / 10.0)),
            ("steps", Json::Arr(ran.steps)),
            ("focused", focused),
            ("screens", screens),
        ];
        if !modal.is_null() {
            j.push(("modal", modal));
        }
        if let Some(why) = ran.stopped {
            j.push(("stopped", Json::from(why)));
            j.push(("frames_left", Json::from(ran.wait_left)));
            j.push(("steps_left", Json::from(ran.rest.len())));
            j.push(("rest", Json::arr(ran.rest.iter().map(Step::to_json))));
            j.push(("budget_ms", Json::from(crate::ui_tools::BUDGET_MS)));
        }
        if let Some((code, message)) = ran.failed {
            j.insert(0, ("error", Json::from(code)));
            j.insert(1, ("message", Json::from(message)));
        }
        let text = Json::obj(j).to_string();
        Ok(if failed { ToolOutput::error(text) } else { ToolOutput::text(text) })
    }

    /// `ui_act`'s frames on surface `s`, laid out for `display`: the game's inside one
    /// `game_tool` as the caller (refused by a claim, the game is rolled back), the engine
    /// screens' directly.
    fn ui_act_frames(&mut self, a: &ActArgs, s: Surface, display: Display, w: Writer, tick: u64, started: Instant) -> Result<Ran, WorldError> {
        let size = (display.size[0], display.size[1]);
        // A first frame lays the interface out for this display when the last one didn't.
        let laid = self.with_ui(s, display, |ui, _, scale, _, d| ui.last_view() == Some(d.viewport(ui.theme(), scale)))?;
        // What the budget leaves after the checks above.
        let budget = Duration::from_secs_f64(crate::ui_tools::BUDGET_MS / 1000.0).saturating_sub(started.elapsed());
        match s {
            Surface::Game => {
                let Some(game) = self.game.as_mut() else { return Err(WorldError::new("no_ui", "no game is running")) };
                // A claim can only refuse the frames when claims exist: checkpoint only then.
                let checkpoint = if self.swarm.claims.is_empty() { None } else { game.checkpoint() };
                let (clock, agent, windowed) = (&self.clock, self.game_name.as_str(), self.windowed);
                let mut camera = if self.windowed { self.window_camera } else { self.ui_state.camera };
                let cam = &mut camera;
                let run = |gw: &mut game::GameWorld| {
                    let mut d = GameDriver { game: game.as_mut(), world: gw, clock, agent, windowed, size, camera: cam, filled: false };
                    run_steps(&mut d, a, !laid, budget)
                };
                match self.world.game_tool(&mut self.swarm, tick, agent, w, run) {
                    Ok(r) => {
                        if !self.windowed {
                            self.ui_state.camera = camera;
                        }
                        Ok(r)
                    }
                    Err(e) => {
                        self.game_rolled_back(checkpoint);
                        Err(self.conflict_help(e, Some(&conflict::Yours::named(vec!["(ui_act frames)".into()]))))
                    }
                }
            }
            _ => {
                let mut d = ScreenDriver { engine: self, surface: s, size };
                Ok(run_steps(&mut d, a, !laid, budget))
            }
        }
    }

    /// `ui_frame` as a capture: the caller's display (with the call's own `display` over it, for
    /// this capture alone), its size, the player's camera, the interface drawn and outlined.
    pub(crate) fn ui_frame_spec(&mut self, f: FrameArgs, session: &SessionInfo) -> FrameSpec {
        let mut spec = f.frame;
        let mut d = self.ui_display_for(Some(&session.id)).0;
        if let Some(a) = f.display {
            if let Some(size) = a.size {
                d.size = size;
            }
            overlay(&mut d, &a);
        }
        if let Some([w, h]) = f.size {
            d.size = [w, h];
        } else {
            let (mut w, mut h) = (d.size[0].clamp(64, crate::ui_tools::MAX_FRAME_SIDE), d.size[1].clamp(64, crate::ui_tools::MAX_FRAME_SIDE));
            // A window past 4K captures at 4K's pixel count, same aspect.
            let px = u64::from(w) * u64::from(h);
            if px > crate::ui_tools::MAX_FRAME_PIXELS {
                let k = (crate::ui_tools::MAX_FRAME_PIXELS as f64 / px as f64).sqrt();
                (w, h) = (((f64::from(w) * k) as u32).max(64), ((f64::from(h) * k) as u32).max(64));
            }
            spec.width = w;
            spec.height = h;
        }
        spec.overlays.ui_display = Some(Box::new(d));
        if f.player_camera {
            let cam = if self.windowed { self.window_camera } else { self.ui_state.camera };
            if let Some(c) = cam {
                spec.eye = Some(c.eye);
                spec.target = Some(c.target);
                spec.fov_y_deg = c.fov_y_deg.clamp(10.0, 120.0);
                if let sight::Projection::Ortho { height_m } = c.projection {
                    spec.ortho = Some(height_m);
                }
            }
        }
        spec
    }

    /// The `ui` overlay: every surface the capture drew, outlined from the layout it drew
    /// (`ui_frame`). Only an interface laid out for this capture's size counts.
    /// Before a capture with the `ui` overlay draws its interfaces: each forgets its last
    /// layout, so afterwards only those the capture laid out claim its size.
    pub(crate) fn ui_forget_capture_views(&mut self) {
        if let Some(g) = self.game.as_mut().and_then(|g| g.ui(UiPick::Capture)) {
            g.ui.forget_view();
        }
        if let Some((ui, _, _)) = self.taste.screen.ui_parts() {
            ui.forget_view();
        }
        if let Some((ui, _, _)) = self.review_ui.ui_parts() {
            ui.forget_view();
        }
    }

    pub(crate) fn paint_ui_outlines(&mut self, p: &mut Painter, max_labels: usize, drawn: bool) -> Json {
        if !drawn {
            return Json::obj([("outlines", Json::Arr(Vec::new())), ("skipped", Json::from("include_ui is off: the capture drew no interface"))]);
        }
        let size = p.size();
        let drawn_here = |ui: &Ui| ui.last_view().is_some_and(|v| v.size == [size[0] as u32, size[1] as u32]);
        let mut all: Vec<(&'static str, specula_ui::Outline)> = Vec::new();
        let mut left_out = 0;
        let mut surfaces = Vec::new();
        let mut take = |name: &'static str, ui: &Ui, all: &mut Vec<(&'static str, specula_ui::Outline)>| {
            if !drawn_here(ui) {
                return;
            }
            surfaces.push(name);
            let (o, more) = ui.outlines(MAX_OUTLINES.saturating_sub(all.len()));
            left_out += more;
            all.extend(o.into_iter().map(|o| (name, o)));
        };
        // The engine's screens are modal while open (as review_frame and gallery_frame decide:
        // they take all the window's input), the review screen over the gallery over the game.
        let (review_modal, gallery_modal) = (self.review_ui.is_open(), self.taste.screen.is_open());
        if let Some(g) = self.game.as_mut().and_then(|g| g.ui(UiPick::Capture)) {
            take("game", g.ui, &mut all);
        }
        let game_end = all.len();
        if gallery_modal {
            if let Some((ui, _, _)) = self.taste.screen.ui_parts() {
                take("gallery", ui, &mut all);
            }
        }
        let gallery_end = all.len();
        if review_modal || self.swarm.review.pending_count() > 0 {
            if let Some((ui, _, _)) = self.review_ui.ui_parts() {
                take("review", ui, &mut all);
            }
        }
        let under = if review_modal { gallery_end } else if gallery_modal { game_end } else { 0 };
        block_under_modal(&mut all[..under]);
        const INPUT: [f32; 4] = [0.25, 0.9, 1.0, 0.95];
        const FOCUS: [f32; 4] = [1.0, 0.86, 0.2, 1.0];
        const DISABLED: [f32; 4] = [1.0, 0.36, 0.36, 0.9];
        const NAMED: [f32; 4] = [0.8, 0.82, 0.86, 0.7];
        const BLOCKED: [f32; 4] = [0.55, 0.58, 0.62, 0.35];
        let mut list = Vec::with_capacity(all.len());
        for (surface, o) in &all {
            let (color, width) = if o.blocked {
                // Under a modal: shown, out of reach. Faint, and unlabelled (the modal's ids read).
                (BLOCKED, 1.0)
            } else if o.focused {
                (FOCUS, 3.0)
            } else if o.interactive && !o.enabled {
                (DISABLED, 1.5)
            } else if o.interactive {
                (INPUT, 1.5)
            } else {
                (NAMED, 1.0)
            };
            let r = o.rect;
            let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
            p.line2d([x0, y0], [x1, y0], color, width);
            p.line2d([x1, y0], [x1, y1], color, width);
            p.line2d([x1, y1], [x0, y1], color, width);
            p.line2d([x0, y1], [x0, y0], color, width);
            if let (Some(id), false) = (&o.id, o.blocked) {
                let priority = if o.focused { u64::MAX / 2 } else { (u64::from(o.interactive) << 40) + (r.w * r.h) as u64 };
                p.label(id, [x0, y0], color, priority);
            }
            let mut e = vec![("surface", Json::from(*surface)), ("screen", Json::from(o.screen.as_str()))];
            if let Some(id) = &o.id {
                e.push(("id", Json::from(id.as_str())));
            }
            e.push(("type", Json::from(o.kind)));
            e.push(("rect", Json::arr(r.to_array().map(|v| Json::from((f64::from(v) * 10.0).round() / 10.0)))));
            if o.interactive {
                e.push(("enabled", Json::Bool(o.enabled)));
            }
            if o.focused {
                e.push(("focused", Json::Bool(true)));
            }
            if o.blocked {
                e.push(("blocked", Json::Bool(true)));
            }
            list.push(Json::obj(e));
        }
        Json::obj([
            ("surfaces", Json::arr(surfaces.into_iter().map(Json::from))),
            ("outlines", Json::Arr(list)),
            ("left_out", Json::from(left_out)),
            ("labels_at_most", Json::from(max_labels)),
            ("legend", Json::obj([("takes_input", Json::from(ov::hex(INPUT))), ("gamepad_focus", Json::from(ov::hex(FOCUS))), ("disabled", Json::from(ov::hex(DISABLED))), ("has_id", Json::from(ov::hex(NAMED))), ("under_a_modal", Json::from(ov::hex(BLOCKED)))])),
            ("rects_from", Json::from("the layout this capture drew (physical px, origin top-left)")),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presses_resolve_through_the_input_map() {
        let map = ActionMap::from_json_str(
            r#"{"format": "specula.input.v1", "actions": [
                {"name": "pause", "kind": "button", "bindings": ["Pad.Start", "Key.P"]},
                {"name": "speed", "kind": "button", "bindings": ["Key.F"]},
                {"name": "gallery", "kind": "button", "bindings": [{"source": "Pad.West", "modifiers": ["Pad.LeftBumper"]}]}
            ]}"#,
        )
        .unwrap();
        assert_eq!(press_sources("accept", Device::Gamepad, None).unwrap(), [Source::Pad(GamepadButton::South)]);
        assert_eq!(press_sources("back", Device::Keyboard, None).unwrap(), [key("Escape")]);
        assert_eq!(press_sources("pause", Device::Gamepad, Some(&map)).unwrap(), [Source::Pad(GamepadButton::Start)]);
        assert_eq!(press_sources("pause", Device::Keyboard, Some(&map)).unwrap(), [key("P")]);
        assert_eq!(press_sources("gallery", Device::Gamepad, Some(&map)).unwrap(), [Source::Pad(GamepadButton::LeftBumper), Source::Pad(GamepadButton::West)], "modifiers first");
        assert!(press_sources("speed", Device::Gamepad, Some(&map)).unwrap_err().contains("no gamepad button"));
        assert!(press_sources("nope", Device::Gamepad, Some(&map)).unwrap_err().contains("no input action"));
        assert!(press_sources("Key.F", Device::Gamepad, None).unwrap_err().contains("isn't a gamepad button"));
        assert_eq!(press_sources("Pad.Back", Device::Gamepad, None).unwrap(), [Source::Pad(GamepadButton::Back)]);
    }

    #[test]
    fn a_null_os_scale_goes_back_to_the_platforms() {
        let mut set = DisplayArg::default();
        apply_display(&mut set, DisplayArg { os_scale: Some(Some(1.0)), size: Some([1280, 800]), ..DisplayArg::default() });
        assert_eq!((set.os_scale, set.size), (Some(Some(1.0)), Some([1280, 800])));
        apply_display(&mut set, DisplayArg { touch: Some(true), ..DisplayArg::default() });
        assert_eq!(set.os_scale, Some(Some(1.0)), "absent fields stay");
        apply_display(&mut set, DisplayArg { os_scale: Some(None), ..DisplayArg::default() });
        assert_eq!(set.os_scale, None, "null drops the override (it was an override to 'unknown' before)");
        assert_eq!((set.size, set.touch), (Some([1280, 800]), Some(true)));
    }

    /// Frames that each cost `cost` (no interface).
    struct Slow {
        cost: Duration,
        frames: usize,
    }

    impl Driver for Slow {
        fn frame(&mut self, _input: &Synth) {
            self.frames += 1;
            std::thread::sleep(self.cost);
        }
        fn ui(&mut self) -> Option<(&mut Ui, Option<&ActionMap>)> {
            None
        }
    }

    /// Review R2 of m5-uitools, finding 3: a call that lays out first (a settle frame, because
    /// another session's display came in between) still runs its first step unit when the settle
    /// frame alone used the budget; every `rest` answer made progress, so continuing ends.
    #[test]
    fn the_first_step_unit_runs_even_after_a_slow_settle_frame() {
        let Ok(UiCall::Act(a)) = crate::ui_tools::parse("ui_act", &Json::parse(r#"{"steps": [{"press": "accept", "device": "gamepad"}, {"wait": 5}], "settle": 0}"#).unwrap()).unwrap() else { panic!() };
        let mut d = Slow { cost: Duration::from_millis(25), frames: 0 };
        let ran = run_steps(&mut d, &a, true, Duration::from_millis(20));
        assert!(ran.failed.is_none(), "{:?}", ran.failed);
        assert_eq!(ran.stopped, Some("time_budget"));
        assert_eq!(ran.frames, 3, "the settle frame, then the press and its release");
        assert_eq!(d.frames, 3);
        assert!(ran.steps[0].get("did").and_then(Json::as_str).is_some_and(|d| d.starts_with("pressed ")), "{:?}", ran.steps);
        assert_eq!(Json::arr(ran.rest.iter().map(Step::to_json)).to_string(), r#"[{"wait":5}]"#, "only what didn't run is left");
        // A wait first: its first frame runs after the settle frame, then the budget stops it.
        let Ok(UiCall::Act(w)) = crate::ui_tools::parse("ui_act", &Json::parse(r#"{"steps": [{"wait": 4}], "settle": 0}"#).unwrap()).unwrap() else { panic!() };
        let mut d = Slow { cost: Duration::from_millis(25), frames: 0 };
        let ran = run_steps(&mut d, &w, true, Duration::from_millis(20));
        assert_eq!((ran.frames, ran.wait_left), (2, 3), "the settle frame and one wait frame");
    }

    #[test]
    fn holding_sets_and_clears_buttons_and_keys() {
        let mut f = Synth { events: Vec::new(), pad: Some(pad(0, 0.0)) };
        hold(&[Source::Pad(GamepadButton::South), key("Enter")], true, &mut f);
        assert_eq!(f.pad.unwrap().buttons, GamepadButton::South.bit());
        assert_eq!(f.pad.unwrap().raw.buttons, GamepadButton::South.bit(), "the raw state agrees (the pad gate reads it)");
        assert!(matches!(f.events[..], [Event::KeyDown { .. }]));
        hold(&[Source::Pad(GamepadButton::South)], false, &mut f);
        assert_eq!(f.pad.unwrap().buttons, 0);
    }
}

#[cfg(test)]
mod engine_tests;

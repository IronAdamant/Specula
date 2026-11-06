//! The window's editor: presentation and input (docs/design/EDITOR.md).
//!
//! A `specula-ui` screen (`crates/specula/ui/editor.json`, theme `editor_theme.json`: data like
//! any game's screens) docked around the viewport: toolbar, outliner, inspector, status bar.
//! F7 (or LB + RB + Start) toggles it; mouse, touch, keys and gamepad focus navigation all work
//! (F5, F6). While it is open the hosted game gets no window input.
//!
//! This module holds presentation state only and turns input into [`UiAct`]s and a
//! [`ViewIn`] (the viewport's pointer and buttons); `editor_engine` applies them, because
//! selection, gizmos and every edit need the world and the protocol (`Engine::handle`).

use crate::editor_gizmo::{Handle, Mode, Snap, Space};
use specula_core::Json;
use specula_gpu::{OverlayQuad, OverlayTextures};
use specula_input::{ActionMap, Input, Source};
use specula_platform::gamepad::GamepadState;
use specula_platform::{Event, MouseButton, PointerKind};
use specula_ui::gpu::GpuOverlay;
use specula_ui::{FontSet, StateMap, Theme, Ui, Viewport};
use std::collections::VecDeque;

const THEME: &str = include_str!("../ui/editor_theme.json");
const SCREEN: &str = include_str!("../ui/editor.json");
pub const SCREEN_NAME: &str = "editor";

/// Outliner rows on one page (the screen's `repeat.max`).
pub const PAGE_ROWS: usize = 18;
/// The most entities selected at once (a box over a huge world keeps the first ones).
pub const MAX_SELECTION: usize = 256;
/// The longest text typed into a field or the search.
pub const MAX_TYPED: usize = 64;
/// The longest text typed into a text entity's Text field: the text component's own limit
/// (bytes), so a text of any length starts whole and is never cut (review R4 of m5-text).
pub const MAX_TYPED_TEXT_BYTES: usize = crate::text_world::MAX_TEXT_BYTES;
/// The most characters of a text entity's words the inspector shows (the rest is cut).
const SHOWN_TEXT: usize = 96;
/// Edits the human can undo (and redo) from the toolbar, newest last.
pub const MAX_UNDO: usize = 128;
/// Simulated input frames waiting (`editor_input`).
pub const MAX_INJECTED: usize = 1024;

/// The editor's own actions (beside the UI's focus, accept and back).
const ACTIONS: &str = r#"{
  "format": "specula.input.v1",
  "actions": [
    { "name": "editor_toggle", "kind": "button", "group": "editor", "description": "Open or close the editor.", "bindings": ["Key.F7", "Pad.LB+Pad.RB+Pad.Start"] },
    { "name": "editor_translate", "kind": "button", "group": "editor", "description": "The translate gizmo.", "bindings": ["Key.W"] },
    { "name": "editor_rotate", "kind": "button", "group": "editor", "description": "The rotate gizmo.", "bindings": ["Key.E"] },
    { "name": "editor_scale", "kind": "button", "group": "editor", "description": "The scale gizmo.", "bindings": ["Key.R"] },
    { "name": "editor_space", "kind": "button", "group": "editor", "description": "World or local axes.", "bindings": ["Key.L"] },
    { "name": "editor_snap", "kind": "button", "group": "editor", "description": "Snapping on or off.", "bindings": ["Key.G"] },
    { "name": "editor_undo", "kind": "button", "group": "editor", "description": "Undo your last edit.", "bindings": ["Key.Ctrl+Key.Z"] },
    { "name": "editor_redo", "kind": "button", "group": "editor", "description": "Redo.", "bindings": ["Key.Ctrl+Key.Y", "Key.Ctrl+Key.Shift+Key.Z"] },
    { "name": "editor_delete", "kind": "button", "group": "editor", "description": "Delete the selection.", "bindings": ["Key.Delete"] },
    { "name": "editor_frame_sel", "kind": "button", "group": "editor", "description": "Frame the selection.", "bindings": ["Key.F"] },
    { "name": "editor_orbit", "kind": "axis2", "output": "delta", "group": "editor", "description": "Orbit the editor camera (radians this frame).", "bindings": [{ "source": "Pad.RightStick", "scale": [2.2, 1.6] }, { "source": "Touch.TwoFingerDrag", "scale": [-0.006, -0.006] }] },
    { "name": "editor_zoom", "kind": "axis", "output": "delta", "group": "editor", "description": "Zoom the editor camera (+ = in).", "bindings": [{ "source": "Pad.RightTrigger", "scale": 1.5 }, { "source": "Pad.LeftTrigger", "scale": -1.5 }, { "source": "Touch.Pinch", "scale": 1.0 }] }
  ]
}"#;

/// A typed inspector field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKey {
    Name,
    /// A text entity's words (the human's way to type into world text: a click in the world
    /// never points the window's keyboard at one; docs/design/TEXT.md).
    Text,
    /// Position, rotation, scale, colour: component 0..3.
    Pos(usize),
    Rot(usize),
    Scale(usize),
    Color(usize),
}

impl FieldKey {
    pub fn parse(s: &str) -> Option<FieldKey> {
        let c = |ch: u8| match ch {
            b'x' | b'r' => Some(0),
            b'y' | b'g' => Some(1),
            b'z' | b'b' => Some(2),
            _ => None,
        };
        let b = s.as_bytes();
        match (s, b.len()) {
            ("name", _) => Some(FieldKey::Name),
            ("text", _) => Some(FieldKey::Text),
            (_, 2) => match b[0] {
                b'p' => c(b[1]).filter(|_| b[1] != b'r' && b[1] != b'g' && b[1] != b'b').map(FieldKey::Pos),
                b'r' => c(b[1]).filter(|_| b[1] != b'r' && b[1] != b'g' && b[1] != b'b').map(FieldKey::Rot),
                b's' => c(b[1]).filter(|_| b[1] != b'r' && b[1] != b'g' && b[1] != b'b').map(FieldKey::Scale),
                b'c' => c(b[1]).filter(|_| b[1] == b'r' || b[1] == b'g' || b[1] == b'b').map(FieldKey::Color),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn key(self) -> String {
        match self {
            FieldKey::Name => "name".into(),
            FieldKey::Text => "text".into(),
            FieldKey::Pos(i) => format!("p{}", ["x", "y", "z"][i % 3]),
            FieldKey::Rot(i) => format!("r{}", ["x", "y", "z"][i % 3]),
            FieldKey::Scale(i) => format!("s{}", ["x", "y", "z"][i % 3]),
            FieldKey::Color(i) => format!("c{}", ["r", "g", "b"][i % 3]),
        }
    }

    pub fn label(self) -> String {
        match self {
            FieldKey::Name => "Name".into(),
            FieldKey::Text => "Text".into(),
            FieldKey::Pos(i) => format!("Position {}", ["X", "Y", "Z"][i % 3]),
            FieldKey::Rot(i) => format!("Rotation {} (degrees)", ["X", "Y", "Z"][i % 3]),
            FieldKey::Scale(i) => format!("Scale {}", ["X", "Y", "Z"][i % 3]),
            FieldKey::Color(i) => format!("Colour {} (0..1)", ["R", "G", "B"][i % 3]),
        }
    }

    /// A number (stepped as well as typed).
    pub fn numeric(self) -> bool {
        !matches!(self, FieldKey::Name | FieldKey::Text)
    }

    /// Whether `c` still fits after `buf`: a text's words up to the component's bytes, other
    /// fields up to [`MAX_TYPED`] characters.
    pub fn fits(self, buf: &str, c: char) -> bool {
        if self == FieldKey::Text {
            buf.len() + c.len_utf8() <= MAX_TYPED_TEXT_BYTES
        } else {
            buf.chars().count() < MAX_TYPED
        }
    }

    pub const NUMBERS: [FieldKey; 12] = [
        FieldKey::Pos(0),
        FieldKey::Pos(1),
        FieldKey::Pos(2),
        FieldKey::Rot(0),
        FieldKey::Rot(1),
        FieldKey::Rot(2),
        FieldKey::Scale(0),
        FieldKey::Scale(1),
        FieldKey::Scale(2),
        FieldKey::Color(0),
        FieldKey::Color(1),
        FieldKey::Color(2),
    ];
}

/// What a press on the panels (or a hotkey) asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum UiAct {
    Close,
    Mode(Mode),
    SpaceToggle,
    SnapToggle,
    Undo,
    Redo,
    FrameSelection,
    Delete,
    /// Escape: cancels a drag, else clears the selection.
    Escape,
    OlTop,
    OlPrev,
    OlNext,
    /// Expand or collapse the row's entity.
    OlExpand(usize),
    /// Select the row's entity (`true`: add to the selection, Shift held).
    OlSelect(usize, bool),
    /// The search text changed.
    Search,
    /// A typed value for a field, with the inspector as it was when typing began (its version
    /// is the edit's `expect_version`: a change made meanwhile refuses it, never lost).
    Commit(FieldKey, String, Option<Box<Inspect>>),
    /// Step the active field by the snap step (-1 or +1).
    Step(FieldKey, f32),
    ToggleVisible,
}

/// A mouse button (or the primary touch contact, as left) this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Btn {
    pub pressed: bool,
    pub held: bool,
    pub released: bool,
}

/// The viewport's input this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct ViewIn {
    pub pointer: Option<[f32; 2]>,
    /// The pointer is over a panel, or a press that began on one is held: the viewport ignores it.
    pub over_ui: bool,
    pub left: Btn,
    pub right: Btn,
    pub middle: Btn,
    /// Wheel notches (+ = away from the user: zoom in).
    pub wheel: f32,
    pub shift: bool,
    pub ctrl: bool,
    /// Gamepad or two-finger orbit (radians this frame) and zoom.
    pub orbit: [f32; 2],
    pub zoom: f32,
    /// The window lost focus this frame: a drag in progress is cancelled, never committed.
    pub focus_lost: bool,
}

/// One outliner row, filled by the engine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub id: u64,
    pub depth: usize,
    pub name: String,
    pub path: String,
    pub has_children: bool,
    pub expanded: bool,
    /// Another agent's claim covering it: the holder.
    pub claim: Option<String>,
    /// Who last changed it.
    pub by: String,
    /// The hosted game's (transient): no tool may change it.
    pub game: bool,
}

/// The inspector's entity, filled by the engine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inspect {
    pub id: u64,
    pub path: String,
    pub name: String,
    pub version: u64,
    pub created_by: String,
    pub modified_by: String,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub color: [f32; 3],
    pub visible: bool,
    pub game: bool,
    /// Another agent's claim covering it, in words.
    pub claim: Option<String>,
    pub components: String,
    /// A text entity's words (its `text`, markup included): `None` for other entities.
    pub text: Option<String>,
}

impl Inspect {
    pub fn value(&self, f: FieldKey) -> f32 {
        match f {
            FieldKey::Name | FieldKey::Text => 0.0,
            FieldKey::Pos(i) => self.position[i % 3],
            FieldKey::Rot(i) => self.rotation[i % 3],
            FieldKey::Scale(i) => self.scale[i % 3],
            FieldKey::Color(i) => self.color[i % 3],
        }
    }
}

/// The outliner's page as the engine filled it.
#[derive(Clone, Debug, Default)]
pub struct OutlinerView {
    pub rows: Vec<Row>,
    pub page: usize,
    pub has_next: bool,
    /// The world's entity count.
    pub total: usize,
    /// Search matches so far (searching only), and whether the scan finished.
    pub matches: usize,
    pub scanning: bool,
    pub note: String,
}

/// A number as the inspector shows it: at most 3 decimals, no trailing zeros.
pub fn num(v: f32) -> String {
    if !v.is_finite() {
        return "nan".into();
    }
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_owned()
    }
}

/// A typed number (`1.5`, `-2`, `3e-1`): finite, within ±1e6.
pub fn parse_number(s: &str) -> Result<f32, String> {
    let t = s.trim();
    let v: f32 = t.parse().map_err(|_| format!("'{t}' is not a number"))?;
    if !v.is_finite() || v.abs() > 1e6 {
        return Err(format!("{t} is out of range (at most 1e6 either way)"));
    }
    Ok(v)
}

pub struct EditorUi {
    open: bool,
    ui: Option<Ui>,
    overlay: GpuOverlay,
    /// Why the panels can't draw (no font), if so.
    pub(crate) problem: Option<String>,
    loaded: bool,
    input: Option<Input>,
    /// Layout-only passes (captures, a changed size) read no input.
    idle: Option<Input>,
    state: StateMap,
    dirty: bool,
    laid_out: Option<(u32, u32)>,
    started: std::time::Instant,
    // Tools.
    pub(crate) mode: Mode,
    pub(crate) space: Space,
    pub(crate) snap: Snap,
    /// Selected entities, the primary last.
    pub(crate) selection: Vec<u64>,
    /// The gizmo handle under the pointer (for its highlight).
    pub(crate) hover: Option<Handle>,
    // Panels.
    pub(crate) outliner: OutlinerView,
    pub(crate) inspect: Option<Inspect>,
    /// The outliner's search text (`#tag` filters by tag).
    pub(crate) search: String,
    /// Typing into the search box.
    searching: bool,
    /// The inspector field the steppers and typing act on, and the text typed so far.
    pub(crate) field: Option<FieldKey>,
    typing: Option<String>,
    /// The inspector when typing began (the edit's base: its version and values).
    pub(crate) edit_base: Option<Inspect>,
    // Feedback.
    pub(crate) status: (String, bool),
    // Undo: the human's own journal ids, newest last; redo: the undos' ids.
    pub(crate) undo: Vec<u64>,
    pub(crate) redo: Vec<u64>,
    // Input state.
    buttons: [bool; 3],
    /// The touch contact acting as the left button.
    contact: Option<u32>,
    /// A press that began on the panels: its whole press is theirs.
    ui_press: bool,
    /// Simulated input frames (`editor_input`), each fed on a window frame of its own with the
    /// tool call that sent it, and the agent that sent the latest.
    pub(crate) injected: VecDeque<(u64, Vec<Event>)>,
    pub(crate) driven_by: Option<String>,
    /// The widget to focus at the end of the next frame.
    focus: Option<&'static str>,
    /// Where the pointer was at the last frame.
    pointer: Option<[f32; 2]>,
}

impl Default for EditorUi {
    fn default() -> Self {
        EditorUi {
            open: false,
            ui: None,
            overlay: GpuOverlay::default(),
            problem: None,
            loaded: false,
            input: None,
            idle: None,
            state: StateMap::new(),
            dirty: true,
            laid_out: None,
            started: std::time::Instant::now(),
            mode: Mode::Translate,
            space: Space::World,
            snap: Snap::default(),
            selection: Vec::new(),
            hover: None,
            outliner: OutlinerView::default(),
            inspect: None,
            search: String::new(),
            searching: false,
            field: None,
            typing: None,
            edit_base: None,
            status: ("Click to select; drag the gizmo to move; every edit is journaled as window:owner.".into(), true),
            undo: Vec::new(),
            redo: Vec::new(),
            buttons: [false; 3],
            contact: None,
            ui_press: false,
            injected: VecDeque::new(),
            driven_by: None,
            focus: None,
            pointer: None,
        }
    }
}

fn actions() -> Result<ActionMap, String> {
    let mut map = ActionMap::from_json_str(ACTIONS).map_err(|e| format!("editor actions: {e}"))?;
    specula_ui::add_ui_actions(&mut map).map_err(|e| format!("ui actions: {e}"))?;
    Ok(map)
}

/// The UI with the editor's theme and screen.
fn load(fonts: FontSet) -> Result<Ui, String> {
    let theme = Theme::from_json_str(THEME).map_err(|e| format!("editor_theme.json: {e}"))?;
    let mut ui = Ui::new(theme, fonts);
    ui.load_screen(SCREEN).map_err(|e| format!("editor.json: {e}"))?;
    Ok(ui)
}

fn system_fonts() -> Result<FontSet, String> {
    let (path, font) = specula_text::find_system_ui_font().ok_or("no system UI font found")?;
    let mut fonts = FontSet::new().with("ui", font);
    if let Some(dir) = path.parent() {
        if let Some(f) = ["segoeuib.ttf", "arialbd.ttf", "DejaVuSans-Bold.ttf"].iter().find_map(|f| specula_text::load_font_file(dir.join(f)).ok()) {
            fonts.add("ui_bold", f);
        }
    }
    if fonts.index("ui_bold").is_none() {
        let (_, f) = specula_text::find_system_ui_font().ok_or("no system UI font found")?;
        fonts.add("ui_bold", f);
    }
    Ok(fonts)
}

fn viewport(ui: &Ui, size: (u32, u32)) -> Viewport {
    let size = [size.0.max(1), size.1.max(1)];
    Viewport::new(size, ui.theme().scale.scale(size, None, 1.0))
}

const VK_BACK: u32 = 0x08;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;

impl EditorUi {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The editor's UI (for `editor_state` and the UI tools), once loaded.
    pub fn ui(&self) -> Option<&Ui> {
        self.ui.as_ref()
    }

    pub fn typing(&self) -> Option<(FieldKey, &str)> {
        match (&self.typing, self.field) {
            (Some(t), Some(f)) => Some((f, t.as_str())),
            _ => None,
        }
    }

    /// The pointer at the last frame.
    pub fn last_pointer(&self) -> Option<[f32; 2]> {
        self.pointer
    }

    /// Whether the UI was loaded (fonts, screen): until then the closed editor reads only its toggle.
    pub fn loaded(&self) -> bool {
        self.loaded
    }

    pub fn searching(&self) -> bool {
        self.searching
    }

    /// Loads the UI on first use (fonts, theme, screen). `false` when it can't be drawn.
    fn ensure(&mut self) -> bool {
        if !self.loaded {
            self.loaded = true;
            match system_fonts().and_then(load) {
                Ok(ui) => self.ui = Some(ui),
                Err(e) => {
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "editor.no_ui", reason = e.as_str());
                    self.problem = Some(e);
                }
            }
            match actions() {
                Ok(map) => {
                    self.input = Some(Input::new(map.clone()));
                    self.idle = Some(Input::new(map));
                }
                Err(e) => self.problem = Some(e),
            }
        }
        self.ui.is_some() && self.input.is_some()
    }

    /// Opens the editor (the engine starts its camera).
    pub fn open(&mut self) {
        if !self.open {
            self.open = true;
            self.dirty = true;
            self.focus = Some("tb_move");
        }
    }

    pub fn close(&mut self) {
        if self.open {
            self.open = false;
            self.typing = None;
            self.searching = false;
            self.ui_press = false;
            self.buttons = [false; 3];
            self.contact = None;
            self.dirty = true;
            if let Some(ui) = self.ui.as_mut() {
                let _ = ui.hide(SCREEN_NAME);
            }
        }
    }

    /// The panels' data changed: filled again at the next layout.
    pub fn changed(&mut self) {
        self.dirty = true;
    }

    pub fn set_status(&mut self, text: impl Into<String>, good: bool) {
        let mut t: String = text.into();
        if t.len() > 400 {
            let cut = (0..=400).rev().find(|i| t.is_char_boundary(*i)).unwrap_or(0);
            t.truncate(cut);
            t.push_str("...");
        }
        self.status = (t, good);
        self.dirty = true;
    }

    /// Selects `ids` (the last is the primary), at most [`MAX_SELECTION`].
    pub fn select(&mut self, ids: &[u64], add: bool) {
        if add {
            for id in ids {
                if let Some(i) = self.selection.iter().position(|s| s == id) {
                    // Shift on a selected entity takes it out (one id), or makes it primary (a box).
                    self.selection.remove(i);
                    if ids.len() > 1 {
                        self.selection.push(*id);
                    }
                } else {
                    self.selection.push(*id);
                }
            }
        } else {
            self.selection.clear();
            self.selection.extend_from_slice(ids);
        }
        if self.selection.len() > MAX_SELECTION {
            let extra = self.selection.len() - MAX_SELECTION;
            self.selection.drain(..extra);
        }
        self.field = None;
        self.typing = None;
        self.edit_base = None;
        self.dirty = true;
    }

    /// The world was replaced (a load, a save read): the selection, the inspector, the outliner
    /// and the undo history name the old world's entities and journal, so they go.
    pub fn world_replaced(&mut self) {
        self.selection.clear();
        self.hover = None;
        self.inspect = None;
        self.outliner = OutlinerView::default();
        self.field = None;
        self.typing = None;
        self.edit_base = None;
        self.undo.clear();
        self.redo.clear();
        self.dirty = true;
    }

    pub fn primary(&self) -> Option<u64> {
        self.selection.last().copied()
    }

    /// Records an edit the human can undo (a new edit clears redo).
    pub fn did(&mut self, journal: u64) {
        self.undo.push(journal);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    /// The next simulated input frame and the call that sent it, if any.
    pub fn next_injected(&mut self) -> Option<(u64, Vec<Event>)> {
        self.injected.pop_front()
    }

    /// Drops the frames of `call` still waiting (its caller gave up on them).
    pub fn drop_injected(&mut self, call: u64) {
        self.injected.retain(|(c, _)| *c != call);
    }

    /// One window frame: reads input (the toggle, hotkeys, the panels, typing) and returns what
    /// it asks for and the viewport's input. `blocked`: an engine screen above holds the input.
    pub fn frame(&mut self, events: &[Event], pads: &[GamepadState], size: (u32, u32), blocked: bool) -> (Vec<UiAct>, ViewIn) {
        let mut acts = Vec::new();
        let mut view = ViewIn::default();
        if size.0 == 0 || size.1 == 0 || !self.ensure() {
            return (acts, view);
        }
        let now = self.started.elapsed().as_secs_f64();
        // While typing, the keys that edit text are the field's, not the UI's (Enter would press
        // the focused button again, Escape go back).
        let typing = self.open && (self.typing.is_some() || self.searching);
        let filtered: Vec<Event>;
        let ui_events: &[Event] = if typing {
            filtered = events.iter().filter(|e| !matches!(e, Event::KeyDown { .. } | Event::KeyUp { .. } | Event::Char(_))).cloned().collect();
            &filtered
        } else {
            events
        };
        let empty: [Event; 0] = [];
        let input = self.input.as_mut().expect("ensured");
        input.update(now, if blocked { &empty } else { ui_events }, if blocked { &[] } else { pads });
        if blocked {
            return (acts, view);
        }
        if !typing && input.pressed("editor_toggle") {
            if self.open {
                acts.push(UiAct::Close);
                return (acts, view);
            }
            self.open();
        }
        if !self.open {
            return (acts, view);
        }
        if typing {
            self.type_keys(events, &mut acts);
        } else {
            let i = self.input.as_ref().expect("ensured");
            for (name, act) in [
                ("editor_translate", UiAct::Mode(Mode::Translate)),
                ("editor_rotate", UiAct::Mode(Mode::Rotate)),
                ("editor_scale", UiAct::Mode(Mode::Scale)),
                ("editor_space", UiAct::SpaceToggle),
                ("editor_snap", UiAct::SnapToggle),
                ("editor_undo", UiAct::Undo),
                ("editor_redo", UiAct::Redo),
                ("editor_delete", UiAct::Delete),
                ("editor_frame_sel", UiAct::FrameSelection),
            ] {
                if i.pressed(name) {
                    acts.push(act);
                }
            }
        }
        // The panels.
        if self.dirty {
            self.fill();
            self.dirty = false;
        }
        let ui = self.ui.as_mut().expect("ensured");
        if !ui.is_visible(SCREEN_NAME) {
            let _ = ui.show(SCREEN_NAME);
        }
        let input = self.input.as_ref().expect("ensured");
        let vp = viewport(ui, size);
        let events_out = ui.frame(input, &self.state, &vp).events.clone();
        self.laid_out = Some(size);
        let shift = input.source_held(Source::Key(VK_SHIFT));
        let ctrl = input.source_held(Source::Key(VK_CONTROL));
        let pressed_actions: Vec<(std::rc::Rc<str>, Option<usize>)> = events_out
            .events
            .iter()
            .filter_map(|e| match e {
                specula_ui::UiEvent::Action { action, widget } => Some((action.clone(), ui.widget_id(*widget).and_then(row_of))),
                _ => None,
            })
            .collect();
        for (action, row) in pressed_actions {
            self.ui_action(&action, row, shift, &mut acts);
        }
        // The viewport.
        let input = self.input.as_ref().expect("ensured");
        view.pointer = input.pointer().map(|p| [p.x, p.y]);
        view.shift = shift;
        view.ctrl = ctrl;
        view.orbit = input.value2("editor_orbit");
        view.zoom = input.value("editor_zoom");
        let mut pressed = [false; 3];
        let mut released = [false; 3];
        for e in events {
            match *e {
                Event::MouseDown { button, .. } => {
                    if let Some(b) = button_index(button) {
                        pressed[b] = true;
                        self.buttons[b] = true;
                    }
                }
                Event::MouseUp { button, .. } => {
                    if let Some(b) = button_index(button) {
                        released[b] = true;
                        self.buttons[b] = false;
                    }
                }
                Event::PointerDown { id, kind, .. } if kind != PointerKind::Mouse && self.contact.is_none() => {
                    self.contact = Some(id);
                    pressed[0] = true;
                    self.buttons[0] = true;
                }
                Event::PointerUp { id, .. } if self.contact == Some(id) => {
                    self.contact = None;
                    released[0] = true;
                    self.buttons[0] = false;
                }
                Event::MouseWheel { delta } => view.wheel += delta,
                Event::Focused(false) => {
                    view.focus_lost = true;
                    for (b, held) in self.buttons.iter_mut().enumerate() {
                        if *held {
                            released[b] = true;
                        }
                        *held = false;
                    }
                    self.contact = None;
                }
                _ => {}
            }
        }
        if let (Some(id), Some(t)) = (self.contact, input.touches().iter().find(|t| Some(t.id) == self.contact)) {
            let _ = id;
            view.pointer = Some([t.x, t.y]);
        }
        self.pointer = view.pointer;
        let any_pressed = pressed.iter().any(|p| *p);
        if any_pressed {
            self.ui_press = events_out.consumed.pointer;
        }
        view.over_ui = events_out.consumed.pointer || self.ui_press;
        if !self.buttons.iter().any(|b| *b) && !any_pressed {
            self.ui_press = false;
        }
        let b = |i: usize| Btn { pressed: pressed[i], held: self.buttons[i], released: released[i] };
        view.left = b(0);
        view.right = b(1);
        view.middle = b(2);
        if events_out.events.iter().any(|e| matches!(e, specula_ui::UiEvent::Back { .. })) && !acts.contains(&UiAct::Escape) {
            acts.push(UiAct::Escape);
        }
        if let Some(id) = self.focus.take() {
            let ui = self.ui.as_mut().expect("ensured");
            if let Some(w) = ui.widget(SCREEN_NAME, id) {
                let _ = ui.focus(w);
            }
        }
        (acts, view)
    }

    /// Keys while typing into a field or the search box.
    fn type_keys(&mut self, events: &[Event], acts: &mut Vec<UiAct>) {
        for e in events {
            match *e {
                Event::Char(c) if !c.is_control() => {
                    let field = if self.searching { FieldKey::Name } else { self.field.unwrap_or(FieldKey::Name) };
                    let buf = if self.searching { &mut self.search } else { self.typing.get_or_insert_with(String::new) };
                    if field.fits(buf, c) {
                        buf.push(c);
                        if self.searching {
                            acts.push(UiAct::Search);
                        }
                    }
                    self.dirty = true;
                }
                Event::KeyDown { code: VK_BACK, .. } => {
                    let buf = if self.searching { &mut self.search } else { self.typing.get_or_insert_with(String::new) };
                    buf.pop();
                    if self.searching {
                        acts.push(UiAct::Search);
                    }
                    self.dirty = true;
                }
                Event::KeyDown { code: VK_RETURN, repeat: false } => {
                    if self.searching {
                        self.searching = false;
                    } else {
                        self.commit_typed(acts);
                    }
                    self.dirty = true;
                }
                Event::KeyDown { code: VK_ESCAPE, repeat: false } => {
                    if self.searching {
                        self.searching = false;
                        self.search.clear();
                        acts.push(UiAct::Search);
                    } else {
                        self.typing = None;
                    }
                    self.dirty = true;
                }
                _ => {}
            }
        }
    }

    /// Enter (or the OK button) on the field being typed: its value is committed, except a
    /// text's words left as they were (nothing is sent: an edit that changes nothing never
    /// writes; review R4 of m5-text).
    fn commit_typed(&mut self, acts: &mut Vec<UiAct>) {
        let (Some(f), Some(t)) = (self.field, self.typing.take()) else { return };
        let base = self.edit_base.take();
        if f == FieldKey::Text && base.as_ref().and_then(|b| b.text.as_deref()) == Some(t.as_str()) {
            self.set_status("The text is unchanged: nothing was sent.", true);
            return;
        }
        acts.push(UiAct::Commit(f, t, base.map(Box::new)));
    }

    fn ui_action(&mut self, action: &str, row: Option<usize>, shift: bool, acts: &mut Vec<UiAct>) {
        match action {
            "editor_close" => acts.push(UiAct::Close),
            "mode_translate" => acts.push(UiAct::Mode(Mode::Translate)),
            "mode_rotate" => acts.push(UiAct::Mode(Mode::Rotate)),
            "mode_scale" => acts.push(UiAct::Mode(Mode::Scale)),
            "space_toggle" => acts.push(UiAct::SpaceToggle),
            "snap_toggle" => acts.push(UiAct::SnapToggle),
            "editor_undo" => acts.push(UiAct::Undo),
            "editor_redo" => acts.push(UiAct::Redo),
            "editor_frame" => acts.push(UiAct::FrameSelection),
            "editor_delete" => acts.push(UiAct::Delete),
            "editor_escape" => {
                if self.typing.is_some() || self.searching {
                    self.typing = None;
                    self.searching = false;
                    self.dirty = true;
                } else {
                    acts.push(UiAct::Escape);
                }
            }
            "ol_top" => acts.push(UiAct::OlTop),
            "ol_prev" => acts.push(UiAct::OlPrev),
            "ol_next" => acts.push(UiAct::OlNext),
            "ol_expand" => acts.extend(row.map(UiAct::OlExpand)),
            "ol_select" => acts.extend(row.map(|r| UiAct::OlSelect(r, shift))),
            "ol_search" => {
                self.searching = true;
                self.typing = None;
                self.dirty = true;
            }
            "ol_search_clear" => {
                self.searching = false;
                self.search.clear();
                acts.push(UiAct::Search);
                self.dirty = true;
            }
            "step_down" | "step_up" => {
                if let Some(f) = self.field.filter(|f| f.numeric()) {
                    acts.push(UiAct::Step(f, if action == "step_up" { 1.0 } else { -1.0 }));
                }
            }
            "field_commit" => {
                self.commit_typed(acts);
                self.dirty = true;
            }
            "toggle_visible" => acts.push(UiAct::ToggleVisible),
            other => {
                if let Some(f) = other.strip_prefix("field:").and_then(FieldKey::parse) {
                    // A text's words start as they are (Backspace edits from the end); other
                    // fields are typed afresh.
                    let start = match (f, self.inspect.as_ref().and_then(|i| i.text.as_ref())) {
                        (FieldKey::Text, Some(t)) => t.clone(),
                        (FieldKey::Text, None) => return,
                        _ => String::new(),
                    };
                    self.field = Some(f);
                    self.typing = Some(start);
                    // The edit starts now: the entity's version and values as the human sees them.
                    self.edit_base = self.inspect.clone();
                    self.searching = false;
                    self.dirty = true;
                }
            }
        }
    }

    /// The screen's state from the editor's data.
    fn fill(&mut self) {
        let s = &mut self.state;
        s.reset();
        s.set_bool("mode_t", self.mode == Mode::Translate);
        s.set_bool("mode_r", self.mode == Mode::Rotate);
        s.set_bool("mode_s", self.mode == Mode::Scale);
        s.set_str("space_label", if self.space == Space::World { "World" } else { "Local" });
        s.set_bool("snap_on", self.snap.on);
        let snap = format!("Snap {} m {} deg x{}", num(self.snap.translate), num(self.snap.rotate), num(self.snap.scale));
        s.set_str("snap_label", &snap);
        s.set_bool("can_undo", !self.undo.is_empty());
        s.set_bool("can_redo", !self.redo.is_empty());
        s.set_bool("has_sel", !self.selection.is_empty());
        let who = match &self.driven_by {
            Some(a) => format!("window:owner (driven by {a})"),
            None => "window:owner".into(),
        };
        s.set_str("who", &who);
        // Outliner.
        let o = &self.outliner;
        let searching = !self.search.is_empty() || self.searching;
        s.set_bool("ol_searching", searching);
        let search_text = if self.searching {
            format!("Search: {}_", self.search)
        } else if self.search.is_empty() {
            "Search...".into()
        } else {
            format!("Search: {}", self.search)
        };
        s.set_str("ol_search_text", &search_text);
        s.set_str("ol_count", &format!("{} entities", o.total));
        s.set_str("ol_page", &format!("page {}", o.page + 1));
        s.set_bool("ol_has_prev", o.page > 0);
        s.set_bool("ol_has_next", o.has_next);
        let note = if searching {
            if o.scanning {
                format!("{} found so far, still searching...", o.matches)
            } else if o.matches == 0 {
                "Nothing matches.".into()
            } else {
                format!("{} found", o.matches)
            }
        } else {
            o.note.clone()
        };
        s.set_str("ol_note", &note);
        let selected: Vec<u64> = self.selection.clone();
        let rows = s.list_mut("rows");
        rows.clear();
        for r in self.outliner.rows.iter().take(PAGE_ROWS) {
            let row = rows.push();
            let indent = if searching { String::new() } else { "  ".repeat(r.depth.saturating_sub(1).min(12)) };
            let label = if searching { r.path.clone() } else { format!("{indent}{}", r.name) };
            row.set_str("label", &label);
            row.set_str("arrow", if r.expanded { "-" } else { "+" });
            row.set_bool("has_children", r.has_children && !searching);
            row.set_bool("selected", selected.contains(&r.id));
            let badge = match (&r.claim, r.game) {
                (Some(c), _) => format!("claimed: {c}"),
                (None, true) => "game".into(),
                _ => String::new(),
            };
            row.set_str("badge", &badge);
            row.set_str("tip", &format!("{} (id {}), last changed by {}", r.path, r.id, r.by));
        }
        // Inspector.
        match &self.inspect {
            Some(i) => {
                s.set_bool("has_sel", true);
                let name = if self.field == Some(FieldKey::Name) && self.typing.is_some() {
                    format!("Name: {}_", self.typing.as_deref().unwrap_or(""))
                } else {
                    i.name.clone()
                };
                s.set_str("in_name", &name);
                s.set_bool("in_has_text", i.text.is_some());
                let text = match (self.field, &self.typing, &i.text) {
                    (Some(FieldKey::Text), Some(t), _) => format!("Text: {}_", tail(t, SHOWN_TEXT)),
                    (_, _, Some(t)) => format!("Text: {}", head(t, SHOWN_TEXT)),
                    _ => String::new(),
                };
                s.set_str("in_text", &text);
                s.set_str("in_path", &i.path);
                s.set_str("in_meta", &format!("id {} - version {} - made by {} - last changed by {}", i.id, i.version, i.created_by, i.modified_by));
                s.set_bool("in_claimed", i.claim.is_some() || i.game);
                let claim = match (&i.claim, i.game) {
                    (Some(c), _) => format!("Claimed: {c}. Your edits here are refused until it is released or shared."),
                    (None, true) => "The hosted game's own entity (transient): no tool may change it.".into(),
                    _ => "Not claimed by another agent: you may edit it.".into(),
                };
                s.set_str("in_claim", &claim);
                let more = if self.selection.len() > 1 { format!("{} selected; the inspector shows the last, the gizmo moves them all.", self.selection.len()) } else { String::new() };
                s.set_str("in_more", &more);
                s.set_str("in_components", &i.components);
                s.set_str("visible_label", if i.visible { "Shown (click to hide)" } else { "Hidden (click to show)" });
                for f in FieldKey::NUMBERS {
                    let shown = match (self.field, &self.typing) {
                        (Some(a), Some(t)) if a == f => format!("{t}_"),
                        (Some(a), None) if a == f => format!("> {}", num(i.value(f))),
                        _ => num(i.value(f)),
                    };
                    s.set_str(&format!("f_{}", f.key()), &shown);
                }
            }
            None => {
                s.set_bool("has_sel", !self.selection.is_empty());
            }
        }
        s.set_bool("has_field", self.field.is_some_and(FieldKey::numeric));
        s.set_bool("typing", self.typing.is_some());
        let field_label = match (self.field, &self.typing) {
            (Some(f), Some(_)) => format!("{}: type a value, Enter", f.label()),
            (Some(f), None) => format!("{}: step", f.label()),
            (None, _) => "Pick a field to type or step it".into(),
        };
        s.set_str("field_label", &field_label);
        // Status.
        s.set_str("status_line", &self.status.0);
        s.set_bool("status_good", self.status.1);
        let sel = match self.selection.len() {
            0 => "nothing selected".to_owned(),
            1 => "1 selected".to_owned(),
            n => format!("{n} selected"),
        };
        s.set_str("sel_line", &format!("{sel} - {} - {}", self.mode.name(), self.space.name()));
    }

    /// Where the panels leave the viewport, as `[x, y, w, h]` in pixels (the whole window before
    /// the first layout).
    pub fn viewport_rect(&self, size: (u32, u32)) -> [f32; 4] {
        let full = [0.0, 0.0, size.0 as f32, size.1 as f32];
        let Some(ui) = self.ui.as_ref().filter(|_| self.laid_out.is_some()) else { return full };
        match ui.widget(SCREEN_NAME, "ed_viewport").and_then(|w| ui.rect(w)) {
            Some(r) if r.w > 0.0 && r.h > 0.0 => [r.x, r.y, r.w, r.h],
            _ => full,
        }
    }

    /// Named widgets' boxes (`[x, y, w, h]`), for agents (at most `max`).
    pub fn widget_boxes(&self, max: usize) -> Json {
        let Some(ui) = self.ui.as_ref().filter(|_| self.open) else { return Json::Obj(Vec::new()) };
        let mut out: Vec<(String, Json)> = Vec::new();
        let mut names: Vec<String> = WIDGETS.iter().map(|s| (*s).to_owned()).collect();
        for r in 0..self.outliner.rows.len().min(PAGE_ROWS) {
            names.push(format!("ol_row[{r}]"));
            names.push(format!("ol_row_on[{r}]"));
            names.push(format!("ol_x[{r}]"));
        }
        for n in names {
            if out.len() >= max {
                break;
            }
            let Some(w) = ui.widget(SCREEN_NAME, &n) else { continue };
            let Some(r) = ui.rect(w) else { continue };
            if r.w <= 0.0 || r.h <= 0.0 {
                continue;
            }
            out.push((n, Json::arr([r.x, r.y, r.w, r.h].map(|v| Json::from((v * 10.0).round() / 10.0)))));
        }
        Json::Obj(out)
    }

    /// The panels' quads for one window frame after `scene` (the gizmo, marks: under the
    /// panels), within `room` quads.
    pub fn overlay<G: OverlayTextures + ?Sized>(&mut self, gpu: &mut G, size: (u32, u32), room: usize, scene: &[OverlayQuad], out: &mut Vec<OverlayQuad>) {
        if !self.open || size.0 == 0 || size.1 == 0 || !self.ensure() {
            return;
        }
        let room = room.min(specula_gpu::MAX_OVERLAY_QUADS.saturating_sub(out.len()));
        let take = scene.len().min(room / 2);
        out.extend_from_slice(&scene[..take]);
        if self.laid_out != Some(size) {
            // A size the last frame didn't lay out: laid out here, with no input. Changed data
            // waits for the next frame's own layout (`frame`): a layout pass without the pointer
            // here, between a press and its release, would drop the press (the review screen's
            // race).
            if self.dirty {
                self.fill();
                self.dirty = false;
            }
            let ui = self.ui.as_mut().expect("ensured");
            if !ui.is_visible(SCREEN_NAME) {
                let _ = ui.show(SCREEN_NAME);
            }
            let vp = viewport(ui, size);
            let idle = self.idle.as_mut().expect("ensured");
            idle.update(self.started.elapsed().as_secs_f64(), &[], &[]);
            ui.frame(idle, &self.state, &vp);
            self.laid_out = Some(size);
        }
        let ui = self.ui.as_mut().expect("ensured");
        if let Err(e) = self.overlay.sync_atlas(gpu, ui.atlas_mut()) {
            self.problem = Some(format!("glyph atlas: {e}"));
            return;
        }
        let quads = match self.overlay.build(ui) {
            Ok(q) => q,
            Err(e) => {
                self.problem = Some(format!("overlay: {e}"));
                return;
            }
        };
        let left = room.saturating_sub(take);
        out.extend_from_slice(&quads[..quads.len().min(left)]);
    }
}

/// Widgets agents may need the boxes of (rows are added per page).
const WIDGETS: &[&str] = &[
    "tb_move", "tb_move_on", "tb_rotate", "tb_rotate_on", "tb_scale", "tb_scale_on", "tb_space", "tb_snap", "tb_snap_on", "tb_undo", "tb_redo", "tb_frame",
    "tb_delete", "tb_close", "ol_search", "ol_search_clear", "ol_top", "ol_prev", "ol_next", "in_name", "in_text", "f_px", "f_py", "f_pz", "f_rx", "f_ry", "f_rz", "f_sx",
    "f_sy", "f_sz", "f_cr", "f_cg", "f_cb", "f_down", "f_up", "f_ok", "f_visible", "ed_viewport", "ed_outliner", "ed_inspector", "ed_toolbar", "ed_status",
];

fn button_index(b: MouseButton) -> Option<usize> {
    match b {
        MouseButton::Left => Some(0),
        MouseButton::Right => Some(1),
        MouseButton::Middle => Some(2),
    }
}

/// The first `n` characters of `s` (an ellipsis when cut), on one line.
fn head(s: &str, n: usize) -> String {
    let one: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    if one.chars().count() > n {
        one.chars().take(n.saturating_sub(1)).chain(['\u{2026}']).collect()
    } else {
        one
    }
}

/// The last `n` characters of `s` (an ellipsis when cut), on one line.
fn tail(s: &str, n: usize) -> String {
    let one: Vec<char> = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    if one.len() > n {
        std::iter::once('\u{2026}').chain(one[one.len() - (n - 1)..].iter().copied()).collect()
    } else {
        one.into_iter().collect()
    }
}

/// The row of a repeated widget id (`ol_row[3]` → 3).
fn row_of(id: &str) -> Option<usize> {
    let open = id.rfind('[')?;
    id[open + 1..].strip_suffix(']')?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screen_theme_and_actions_load() {
        let theme = Theme::from_json_str(THEME).expect("theme");
        let mut ui = Ui::new(theme, FontSet::new().with("ui", specula_ui::testfont::box_font_parsed()).with("ui_bold", specula_ui::testfont::box_font_parsed()));
        ui.load_screen(SCREEN).expect("screen");
        for w in WIDGETS {
            assert!(ui.widget(SCREEN_NAME, w).is_some(), "{w} is in editor.json");
        }
        for r in 0..PAGE_ROWS {
            assert!(ui.widget(SCREEN_NAME, &format!("ol_row[{r}]")).is_some());
        }
        assert!(ui.widget(SCREEN_NAME, &format!("ol_row[{PAGE_ROWS}]")).is_none(), "PAGE_ROWS is the repeat's max");
        actions().expect("actions");
    }

    #[test]
    fn fields_parse_and_name_themselves() {
        for f in FieldKey::NUMBERS.into_iter().chain([FieldKey::Name, FieldKey::Text]) {
            assert_eq!(FieldKey::parse(&f.key()), Some(f), "{f:?}");
        }
        assert_eq!(FieldKey::parse("pr"), None);
        assert_eq!(FieldKey::parse("cx"), None);
        assert_eq!(FieldKey::parse("zz"), None);
        assert_eq!(row_of("ol_row[12]"), Some(12));
        assert_eq!(row_of("tb_move"), None);
    }

    #[test]
    fn numbers_show_and_parse() {
        assert_eq!(num(1.25), "1.25");
        assert_eq!(num(2.0), "2");
        assert_eq!(num(-0.0001), "0");
        assert_eq!(num(0.1 + 0.2), "0.3");
        assert_eq!(parse_number(" -2.5 "), Ok(-2.5));
        assert!(parse_number("abc").is_err());
        assert!(parse_number("1e9").is_err());
        assert!(parse_number("NaN").is_err());
        assert!(parse_number("inf").is_err());
    }

    #[test]
    fn selection_adds_toggles_and_caps() {
        let mut e = EditorUi::default();
        e.select(&[1, 2], false);
        assert_eq!(e.primary(), Some(2));
        e.select(&[3], true);
        assert_eq!(e.selection, vec![1, 2, 3]);
        e.select(&[2], true);
        assert_eq!(e.selection, vec![1, 3], "shift-click takes a selected one out");
        let many: Vec<u64> = (0..1000).collect();
        e.select(&many, false);
        assert_eq!(e.selection.len(), MAX_SELECTION);
        assert_eq!(e.primary(), Some(999));
    }

    #[test]
    fn undo_history_is_bounded_and_a_new_edit_clears_redo() {
        let mut e = EditorUi::default();
        e.redo.push(7);
        for j in 0..(MAX_UNDO as u64 + 10) {
            e.did(j);
        }
        assert_eq!(e.undo.len(), MAX_UNDO);
        assert!(e.redo.is_empty());
    }

    fn test_ui() -> EditorUi {
        let fonts = FontSet::new().with("ui", specula_ui::testfont::box_font_parsed()).with("ui_bold", specula_ui::testfont::box_font_parsed());
        let map = actions().unwrap();
        EditorUi { ui: Some(load(fonts).expect("the editor's screen loads")), input: Some(Input::new(map.clone())), idle: Some(Input::new(map)), loaded: true, ..EditorUi::default() }
    }

    /// A probe from the live check (editor_check run 9): the panels' data changed between a press
    /// and its release, and the overlay's re-layout without the pointer dropped the press.
    #[test]
    fn a_data_change_between_press_and_release_keeps_the_press() {
        use specula_gpu::{TextureFormat, TextureId};
        struct NoGpu;
        impl OverlayTextures for NoGpu {
            fn upload_texture(&mut self, _: u32, _: u32, _: TextureFormat, _: &[u8]) -> Result<TextureId, specula_gpu::GpuError> {
                Ok(TextureId(1))
            }
            fn update_texture(&mut self, _: TextureId, _: u32, _: u32, _: u32, _: u32, _: &[u8]) -> Result<(), specula_gpu::GpuError> {
                Ok(())
            }
            fn destroy_texture(&mut self, _: TextureId) {}
        }
        let size = (1280, 800);
        let mut e = test_ui();
        e.open();
        e.inspect = Some(Inspect { id: 1, path: "/a".into(), name: "a".into(), version: 1, scale: [1.0; 3], ..Inspect::default() });
        e.selection = vec![1];
        e.frame(&[], &[], size, false);
        e.frame(&[], &[], size, false);
        let ui = e.ui.as_ref().unwrap();
        let r = ui.widget(SCREEN_NAME, "f_py").and_then(|w| ui.rect(w)).expect("the Y field is laid out");
        let (x, y) = ((r.x + r.w / 2.0) as i32, (r.y + r.h / 2.0) as i32);
        e.frame(&[Event::MouseMove { x, y }, Event::MouseDown { button: MouseButton::Left, x, y }], &[], size, false);
        // A status line or the outliner changed, then the overlay drew.
        e.set_status("something else happened", true);
        let mut quads = Vec::new();
        e.overlay(&mut NoGpu, size, 60_000, &[], &mut quads);
        e.frame(&[Event::MouseUp { button: MouseButton::Left, x, y }], &[], size, false);
        assert_eq!(e.typing().map(|(f, _)| f), Some(FieldKey::Pos(1)), "the click on the Y field went through");
    }

    #[test]
    fn typed_text_is_capped() {
        let mut e = EditorUi { field: Some(FieldKey::Pos(0)), typing: Some(String::new()), ..EditorUi::default() };
        let chars: Vec<Event> = std::iter::repeat_n(Event::Char('9'), 500).collect();
        let mut acts = Vec::new();
        e.type_keys(&chars, &mut acts);
        assert_eq!(e.typing.as_ref().unwrap().len(), MAX_TYPED);
        e.type_keys(&[Event::KeyDown { code: VK_RETURN, repeat: false }], &mut acts);
        assert!(matches!(acts.last(), Some(UiAct::Commit(FieldKey::Pos(0), t, _)) if t.len() == MAX_TYPED));
    }

    /// The human's way to type into world text (review R3 of m5-text: a click in the world never
    /// points the window's keyboard at a text): the inspector's Text field starts from the text's
    /// words, Backspace edits from the end, Enter commits with the inspector it began from, and
    /// it holds up to the text component's bytes. Entities that aren't texts have no Text field.
    #[test]
    fn a_text_entitys_words_are_typed_in_the_inspector() {
        let inspect = Inspect { id: 4, path: "/epitaph".into(), version: 9, text: Some("Here lies".into()), ..Inspect::default() };
        let mut e = EditorUi { inspect: Some(inspect.clone()), open: true, ..EditorUi::default() };
        let mut acts = Vec::new();
        e.ui_action("field:text", None, false, &mut acts);
        assert_eq!(e.typing(), Some((FieldKey::Text, "Here lies")));
        e.type_keys(&[Event::KeyDown { code: VK_BACK, repeat: false }, Event::KeyDown { code: VK_BACK, repeat: false }, Event::KeyDown { code: VK_BACK, repeat: false }, Event::KeyDown { code: VK_BACK, repeat: false }], &mut acts);
        e.type_keys(&"rests".chars().map(Event::Char).collect::<Vec<_>>(), &mut acts);
        e.type_keys(&[Event::KeyDown { code: VK_RETURN, repeat: false }], &mut acts);
        match acts.last() {
            Some(UiAct::Commit(FieldKey::Text, t, Some(base))) => assert_eq!((t.as_str(), base.version), ("Here rests", 9)),
            other => panic!("{other:?}"),
        }
        // Capped; never stepped.
        e.ui_action("field:text", None, false, &mut acts);
        e.type_keys(&std::iter::repeat_n(Event::Char('x'), 3000).collect::<Vec<_>>(), &mut acts);
        assert_eq!(e.typing().map(|(_, t)| t.len()), Some(MAX_TYPED_TEXT_BYTES));
        let n = acts.len();
        e.ui_action("step_up", None, false, &mut acts);
        assert_eq!(acts.len(), n, "a text isn't stepped");
        // Not a text: no field.
        let mut e = EditorUi { inspect: Some(Inspect { text: None, ..inspect }), open: true, ..EditorUi::default() };
        e.ui_action("field:text", None, false, &mut acts);
        assert_eq!(e.typing(), None);
        assert_eq!(FieldKey::parse("text"), Some(FieldKey::Text));
        assert!(!FieldKey::Text.numeric() && FieldKey::Pos(0).numeric());
    }

    /// Review R4 of m5-text (the reviewer's probe_inspector): a 1,500-character text starts
    /// whole in the Text field; Enter with nothing changed sends nothing; one character typed
    /// commits the whole text with it, never a cut copy.
    #[test]
    fn a_long_text_is_never_cut_and_an_unchanged_one_never_sent() {
        let long = "epitaph ".repeat(187) + "abcd";
        assert_eq!(long.chars().count(), 1500);
        let inspect = Inspect { id: 4, version: 2, text: Some(long.clone()), ..Inspect::default() };
        let mut e = EditorUi { inspect: Some(inspect), open: true, ..EditorUi::default() };
        let mut acts = Vec::new();
        e.ui_action("field:text", None, false, &mut acts);
        assert_eq!(e.typing().map(|(_, t)| t.len()), Some(1500), "starts whole");
        e.type_keys(&[Event::KeyDown { code: VK_RETURN, repeat: false }], &mut acts);
        assert!(acts.is_empty(), "unchanged: nothing sent ({acts:?})");
        e.ui_action("field:text", None, false, &mut acts);
        e.type_keys(&[Event::Char('!')], &mut acts);
        e.ui_action("field_commit", None, false, &mut acts);
        match acts.last() {
            Some(UiAct::Commit(FieldKey::Text, t, _)) => assert_eq!(*t, format!("{long}!"), "the whole text and the key"),
            other => panic!("{other:?}"),
        }
    }

    /// The inspector's text shown on one line: exactly `n` characters stay whole, one more is
    /// cut to `n` with an ellipsis (Probatio's survivors at the boundary).
    #[test]
    fn shown_text_is_cut_only_past_its_length() {
        assert_eq!(head("abcd", 4), "abcd");
        assert_eq!(head("abcde", 4), "abc\u{2026}");
        assert_eq!(tail("abcd", 4), "abcd");
        assert_eq!(tail("abcde", 4), "\u{2026}cde");
        assert_eq!(head("a\nb", 4), "a b", "one line");
    }
}

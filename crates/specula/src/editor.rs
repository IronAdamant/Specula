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

use crate::editor_brush::{Brush, BrushKind};
use crate::editor_gizmo::{Handle, Mode, Snap, Space};
use crate::editor_swarm::SwAct;
use specula_core::Json;
use specula_gpu::{OverlayQuad, OverlayTextures};
use specula_input::{ActionMap, Input, Source};
use specula_platform::gamepad::GamepadState;
use specula_platform::{Event, MouseButton, PointerKind};
use specula_ui::gpu::GpuOverlay;
use specula_ui::{FontSet, StateMap, Theme, Ui, Viewport};
use std::collections::VecDeque;

pub(crate) const THEME: &str = include_str!("../ui/editor_theme.json");
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
    { "name": "editor_brush_narrower", "kind": "button", "group": "editor", "description": "A narrower brush (the draw tools).", "bindings": ["Key.O"] },
    { "name": "editor_brush_wider", "kind": "button", "group": "editor", "description": "A wider brush (the draw tools).", "bindings": ["Key.P"] },
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
    /// The swarm panels (`editor_swarm`).
    Swarm(crate::editor_swarm::SwAct),
    /// "Ask the agents…" (`editor_ask`): the human's own press only.
    AskOpen,
    /// The draw tools (`editor_brush`).
    Brush(BrushAct),
}

/// A press on the Draw bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BrushAct {
    /// Opens or closes the Draw bar (closing it puts the brush down).
    Bar,
    /// The sculpt brush (pressed again: its next op).
    Sculpt,
    /// The splat brush (pressed again: the next layer).
    Splat,
    /// The paint brush (pressed again: the next colour).
    Paint,
    /// The path tool (pressed again: the model's next sweep part).
    Path,
    /// A wider (`true`) or narrower brush.
    Radius(bool),
    /// A stronger (`true`) or weaker brush.
    Amount(bool),
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
    /// The other side's hold ended this frame because it had gone quiet (with `focus_lost`, so
    /// what it held is cancelled). See [`Holds`].
    pub hold_lapsed: bool,
    /// Presses in this frame's events that were not taken: the other side holds the viewport
    /// (or this frame took it back from a quiet hold: press again).
    pub presses_refused: u8,
}

/// A hold whose side has been quiet this long, seconds, yields to the other side's press: the
/// human takes the viewport back from an agent that pressed and went away, and an agent takes it
/// from a human hold whose release never came (it went to another window).
pub const AGENT_HOLD_S: f64 = 5.0;
/// And a hold lapses by itself after this long without anything from its side.
pub const AGENT_HOLD_MAX_S: f64 = 120.0;

/// Who holds the viewport's buttons: the human's own hand, or an agent's injected frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Human,
    Agent,
}

/// The viewport's mouse buttons (and the touch or pen contact that acts as the left one), and
/// who holds them.
///
/// - **All held buttons belong to one side at a time.** A press is taken only when nothing is
///   held, or by the side that holds; a release only from the side that holds. So the human's
///   focus change, mouse, touch or pen never ends an agent's drag, and an agent's frames never
///   take over or end the human's (a live run lost an agent's stroke to a click in another
///   window; reviews R1 and R2 of m5-eddraw found the mixed states a per-button rule left: two
///   sides each holding a button and neither getting the viewport, a contact outliving its
///   button).
/// - **Nobody is locked out.** A hold whose side has been quiet for [`AGENT_HOLD_S`] yields to
///   the other side's press (that press ends the hold; the next one is taken), and any hold
///   lapses after [`AGENT_HOLD_MAX_S`] of quiet. A window that takes no input for the human
///   (another screen holds it, the Ask box, minimized) lets the human's holds go: their
///   release would never be read.
/// - **A press that is not taken is counted**, so an agent's call can say its drag did not
///   start.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Holds {
    buttons: [bool; 3],
    side: Option<Side>,
    /// The touch or pen contact holding the left button.
    contact: Option<u32>,
    /// When each side's pointer last did anything (`[human, agent]`, the editor's clock).
    last: [f64; 2],
    /// Where each side's own pointer is (from its own events), and which side's acted last.
    /// The viewport's pointer is its holder's own; with nothing held, the one that acted last.
    /// (The window's pointer state is one for both sides: an agent's injected moves bent a
    /// stroke the human was drawing, and it was sent as the human's own. Found by the live
    /// check, after review R2.)
    pointers: [Option<[f32; 2]>; 2],
    last_active: Option<Side>,
    /// Each side's own Shift and Ctrl (from its own key events): while a side holds, its drag
    /// reads its own. (The window's key state is one for both sides, and the window's focus
    /// loss clears it: review R3 of m5-eddraw, F2: the human's focus change dropped the Ctrl an
    /// agent's drag was holding, and its snapping flipped mid-drag.)
    mods: [[bool; 2]; 2],
}

impl Holds {
    fn at(side: Side) -> usize {
        match side {
            Side::Human => 0,
            Side::Agent => 1,
        }
    }

    pub(crate) fn holder(&self) -> Option<Side> {
        self.side
    }

    /// Shift and Ctrl as the viewport reads them: the holder's own while a side holds; with
    /// nothing held, the window's (`shared`).
    fn modifiers(&self, shared: (bool, bool)) -> (bool, bool) {
        match self.side {
            Some(s) => (self.mods[Self::at(s)][0], self.mods[Self::at(s)][1]),
            None => shared,
        }
    }

    /// The viewport's pointer: the holder's own; with nothing held, the pointer of the side
    /// that acted last. `None`: neither side's pointer has been anywhere yet.
    fn pointer(&self) -> Option<[f32; 2]> {
        self.side.or(self.last_active).and_then(|s| self.pointers[Self::at(s)])
    }

    /// Nothing held means nobody holds, and no contact outlives the left button.
    fn settle(&mut self) {
        if !self.buttons.iter().any(|b| *b) {
            self.side = None;
        }
        if !self.buttons[0] {
            self.contact = None;
        }
    }

    /// Lets go of everything `side` holds (marking each in `released`). Whether it held anything.
    pub(crate) fn drop_side(&mut self, side: Side, released: &mut [bool; 3]) -> bool {
        if self.side != Some(side) {
            return false;
        }
        for (b, held) in self.buttons.iter_mut().enumerate() {
            if std::mem::take(held) {
                released[b] = true;
            }
        }
        self.settle();
        true
    }

    /// The start of `side`'s frame at time `now`. `active`: its pointer did something this
    /// frame; `presses`: it pressed. The other side's hold ends here when it has been quiet
    /// long enough ([`AGENT_HOLD_S`] and a press, or [`AGENT_HOLD_MAX_S`]): returns whether one
    /// did (its buttons are marked in `released`).
    fn begin(&mut self, side: Side, now: f64, active: bool, presses: bool, released: &mut [bool; 3]) -> bool {
        if active {
            self.last[Self::at(side)] = now;
            self.last_active = Some(side);
        }
        let Some(holder) = self.side.filter(|h| *h != side) else { return false };
        let quiet = now - self.last[Self::at(holder)];
        if quiet > AGENT_HOLD_MAX_S || (quiet > AGENT_HOLD_S && presses) {
            return self.drop_side(holder, released);
        }
        false
    }

    /// `side` presses button `b`: taken when nothing is held or `side` holds.
    fn press(&mut self, side: Side, b: usize) -> bool {
        if self.side.is_some_and(|h| h != side) {
            return false;
        }
        self.buttons[b] = true;
        self.side = Some(side);
        true
    }

    /// `side` releases button `b`: read unless the other side holds.
    fn release(&mut self, side: Side, b: usize) -> bool {
        if self.side.is_some_and(|h| h != side) {
            return false;
        }
        self.buttons[b] = false;
        self.settle();
        true
    }

    /// A touch or pen contact goes down for `side`: it acts as the left button. `None`: there is
    /// a contact already (a second finger: not a press at all).
    fn contact_down(&mut self, side: Side, id: u32) -> Option<bool> {
        if self.contact.is_some() {
            return None;
        }
        let taken = self.press(side, 0);
        if taken {
            self.contact = Some(id);
        }
        Some(taken)
    }

    fn contact_up(&mut self, side: Side, id: u32) -> bool {
        self.contact == Some(id) && self.release(side, 0)
    }
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
    // Input state: the viewport's buttons, and who holds them ([`Holds`]).
    holds: Holds,
    /// Whether the frame being read is an agent's injected one (the engine sets it around
    /// [`EditorUi::frame`]).
    pub(crate) injected_pass: bool,
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
    /// The swarm panels' state (the right dock's Swarm tab, the claim map).
    pub(crate) swarm: crate::editor_swarm::Panel,
    /// The "Ask the agents…" box (its words are private to it).
    pub(crate) ask: crate::editor_ask::AskBox,
    /// The brush in hand (the draw tools: while one is, the viewport's left button draws).
    pub brush: Option<Brush>,
    /// The Draw bar is open; and the brush as it was last held (its size and strength are kept).
    pub draw_bar: bool,
    brush_kept: Brush,
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
            holds: Holds::default(),
            injected_pass: false,
            ui_press: false,
            injected: VecDeque::new(),
            driven_by: None,
            focus: None,
            pointer: None,
            swarm: crate::editor_swarm::Panel::default(),
            ask: crate::editor_ask::AskBox::default(),
            brush: None,
            draw_bar: false,
            brush_kept: Brush::default(),
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

pub(crate) fn system_fonts() -> Result<FontSet, String> {
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

    /// The UI tools' view of the editor (`surface: "editor"`): its UI, state and action map.
    /// Never the Ask box's (its words are the human's alone).
    pub(crate) fn ui_parts(&mut self) -> Option<(&mut Ui, &StateMap, Option<&ActionMap>)> {
        let map = self.input.as_ref().map(Input::map);
        Some((self.ui.as_mut()?, &self.state, map))
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
            self.holds = Holds::default();
            self.dirty = true;
            self.ask.close("editor_closed");
            if let Some(ui) = self.ui.as_mut() {
                let _ = ui.hide(SCREEN_NAME);
            }
        }
    }

    /// The human is typing a request into the "Ask the agents…" box.
    pub fn asking(&self) -> bool {
        self.ask.is_open()
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
        self.swarm.world_replaced();
        // Its selection was the old world's.
        self.ask.close("world_replaced");
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
            // (Minimized: the human's release would never be read.)
            self.holds.drop_side(Side::Human, &mut [false; 3]);
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
            // Another screen holds the window: the human's release would never be read here
            // (review R2 of m5-eddraw, N1: their hold then kept every agent out of the
            // viewport, silently, until their next click).
            self.holds.drop_side(Side::Human, &mut [false; 3]);
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
            // The brush's size, without leaving the stroke for the Draw bar (letters: the
            // bracket keys have no code that holds on every keyboard layout).
            if self.brush.is_some() {
                for (name, wider) in [("editor_brush_narrower", false), ("editor_brush_wider", true)] {
                    if i.pressed(name) {
                        acts.push(UiAct::Brush(BrushAct::Radius(wider)));
                    }
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
        let side = if self.injected_pass { Side::Agent } else { Side::Human };
        // (An agent's frame is its pointer acting, whatever it holds; the human's is when their
        // pointer does something.)
        let pointer_event = |e: &Event| matches!(e, Event::MouseMove { .. } | Event::MouseDown { .. } | Event::MouseUp { .. } | Event::MouseWheel { .. } | Event::PointerDown { .. } | Event::PointerMove { .. } | Event::PointerUp { .. });
        let presses = events.iter().any(|e| matches!(e, Event::MouseDown { .. } | Event::PointerDown { .. }));
        let active = self.injected_pass || events.iter().any(pointer_event);
        // The other side's quiet hold ends with this side's press (or by itself, after
        // minutes): what it held is cancelled, and this frame's press is the one that ended it,
        // not a press of its own (press again).
        for e in events {
            if let Event::MouseMove { x, y } | Event::MouseDown { x, y, .. } | Event::MouseUp { x, y, .. } | Event::PointerDown { x, y, .. } | Event::PointerMove { x, y, .. } | Event::PointerUp { x, y, .. } = *e {
                self.holds.pointers[Holds::at(side)] = Some([x as f32, y as f32]);
            }
            // (This side's own Shift and Ctrl; its own focus loss lets them go.)
            let mods = &mut self.holds.mods[Holds::at(side)];
            match *e {
                Event::KeyDown { code: VK_SHIFT, .. } => mods[0] = true,
                Event::KeyUp { code: VK_SHIFT } => mods[0] = false,
                Event::KeyDown { code: VK_CONTROL, .. } => mods[1] = true,
                Event::KeyUp { code: VK_CONTROL } => mods[1] = false,
                Event::Focused(false) => *mods = [false; 2],
                _ => {}
            }
        }
        let took_back = self.holds.begin(side, now, active, presses, &mut released);
        if took_back {
            view.focus_lost = true;
            view.hold_lapsed = true;
        }
        let mut refused = 0u8;
        for e in events {
            match *e {
                Event::MouseDown { button, .. } => {
                    if let Some(b) = button_index(button) {
                        if !took_back && self.holds.press(side, b) {
                            pressed[b] = true;
                        } else {
                            refused = refused.saturating_add(1);
                        }
                    }
                }
                Event::MouseUp { button, .. } => {
                    if let Some(b) = button_index(button).filter(|b| self.holds.release(side, *b)) {
                        released[b] = true;
                    }
                }
                Event::PointerDown { id, kind, .. } if kind != PointerKind::Mouse => match self.holds.contact_down(side, id).filter(|_| !took_back) {
                    Some(true) => pressed[0] = true,
                    Some(false) => refused = refused.saturating_add(1),
                    None if took_back => refused = refused.saturating_add(1),
                    None => {}
                },
                Event::PointerUp { id, .. } => {
                    if self.holds.contact_up(side, id) {
                        released[0] = true;
                    }
                }
                Event::MouseWheel { delta } => view.wheel += delta,
                // A side's focus loss ends what that side holds (the human's release would go to
                // another window): said to the viewport when it let something go, or when
                // nobody holds anything.
                Event::Focused(false) if self.holds.drop_side(side, &mut released) || self.holds.holder().is_none() => view.focus_lost = true,
                _ => {}
            }
        }
        // (Its holder's own pointer and modifiers, not the window's shared ones: see `Holds`.)
        if let Some(at) = self.holds.pointer() {
            view.pointer = Some(at);
        }
        (view.shift, view.ctrl) = self.holds.modifiers((view.shift, view.ctrl));
        if let (Some(id), Some(t)) = (self.holds.contact, input.touches().iter().find(|t| Some(t.id) == self.holds.contact)) {
            let _ = id;
            view.pointer = Some([t.x, t.y]);
        }
        self.pointer = view.pointer;
        let any_pressed = pressed.iter().any(|p| *p);
        if any_pressed {
            self.ui_press = events_out.consumed.pointer;
        }
        view.over_ui = events_out.consumed.pointer || self.ui_press;
        // A press that landed on a panel was the panel's (and it acted): only a press meant
        // for the viewport is one that was "not taken". (Review R3 of m5-eddraw, F1: an agent
        // told to retry a toolbar click pressed it twice.)
        view.presses_refused = if view.over_ui { 0 } else { refused };
        if self.holds.holder().is_none() && !any_pressed {
            self.ui_press = false;
        }
        let b = |i: usize| Btn { pressed: pressed[i], held: self.holds.buttons[i], released: released[i] };
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
            "draw_bar" => acts.push(UiAct::Brush(BrushAct::Bar)),
            "brush_sculpt" => acts.push(UiAct::Brush(BrushAct::Sculpt)),
            "brush_splat" => acts.push(UiAct::Brush(BrushAct::Splat)),
            "brush_paint" => acts.push(UiAct::Brush(BrushAct::Paint)),
            "brush_path" => acts.push(UiAct::Brush(BrushAct::Path)),
            "brush_radius_up" => acts.push(UiAct::Brush(BrushAct::Radius(true))),
            "brush_radius_down" => acts.push(UiAct::Brush(BrushAct::Radius(false))),
            "brush_amount_up" => acts.push(UiAct::Brush(BrushAct::Amount(true))),
            "brush_amount_down" => acts.push(UiAct::Brush(BrushAct::Amount(false))),
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
            "ask_open" => acts.push(UiAct::AskOpen),
            "tab_inspector" | "tab_swarm" => acts.push(UiAct::Swarm(SwAct::Shown(action == "tab_swarm"))),
            "sw_select" => acts.extend(row.map(|r| UiAct::Swarm(SwAct::Select(r)))),
            "sw_newest" => acts.push(UiAct::Swarm(SwAct::Newest)),
            "sw_newer" => acts.push(UiAct::Swarm(SwAct::Newer)),
            "sw_older" => acts.push(UiAct::Swarm(SwAct::Older)),
            "sw_agent" => acts.push(UiAct::Swarm(SwAct::CycleAgent)),
            "sw_place" => acts.push(UiAct::Swarm(SwAct::CyclePlace)),
            "sw_game" => acts.push(UiAct::Swarm(SwAct::ToggleGame)),
            "sw_show" => acts.push(UiAct::Swarm(SwAct::CycleBoard)),
            "claim_map" => acts.push(UiAct::Swarm(SwAct::ClaimMap)),
            "sw_open_review" => acts.push(UiAct::Swarm(SwAct::OpenReview)),
            other if other.starts_with("sw_") => {
                if let Some(v) = crate::editor_swarm::View::parse(&other[3..]) {
                    acts.push(UiAct::Swarm(SwAct::View(v)));
                }
            }
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
        let kind = self.brush.as_ref().map(|b| b.kind);
        let sculpting = matches!(kind, Some(BrushKind::Raise | BrushKind::Lower | BrushKind::Flatten | BrushKind::Smooth));
        s.set_bool("draw_bar", self.draw_bar);
        s.set_bool("brush_on", self.brush.is_some());
        s.set_bool("brush_sculpt", sculpting);
        s.set_bool("brush_splat", matches!(kind, Some(BrushKind::Splat(_))));
        s.set_bool("brush_paint", kind == Some(BrushKind::Paint));
        s.set_bool("brush_path", matches!(kind, Some(BrushKind::Path(_))));
        s.set_str("path_label", &match kind {
            Some(BrushKind::Path(k)) => format!("Path: sweep {}", k + 1),
            _ => "Path".to_owned(),
        });
        s.set_str("sculpt_label", &kind.filter(|_| sculpting).map_or("Sculpt".to_owned(), |k| format!("Sculpt: {}", k.name())));
        s.set_str("splat_label", &match kind {
            Some(BrushKind::Splat(layer)) => format!("Splat: layer {layer}"),
            _ => "Splat".to_owned(),
        });
        s.set_str("paint_label", &self.brush.as_ref().filter(|b| b.kind == BrushKind::Paint).map_or("Paint".to_owned(), |b| format!("Paint: {}", b.color)));
        s.set_str("brush_label", &self.brush.as_ref().map_or("Pick a brush; then drag on the selected terrain or model.".to_owned(), Brush::label));
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
        // The swarm panels (the right dock's Swarm tab, the claim map toggle).
        self.swarm.fill(s);
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
    pub fn widget_boxes(&self, max: usize, rows: bool) -> Json {
        let Some(ui) = self.ui.as_ref().filter(|_| self.open) else { return Json::Obj(Vec::new()) };
        let mut out: Vec<(String, Json)> = Vec::new();
        let mut names: Vec<String> = WIDGETS.iter().map(|s| (*s).to_owned()).collect();
        // Without `rows` the outliner's row boxes are left out (how many there are says how many
        // entities the human's search found).
        for r in 0..if rows { self.outliner.rows.len().min(PAGE_ROWS) } else { 0 } {
            names.push(format!("ol_row[{r}]"));
            names.push(format!("ol_row_on[{r}]"));
            names.push(format!("ol_x[{r}]"));
        }
        for r in 0..self.swarm.rows.len().min(crate::editor_swarm::ROWS) {
            names.push(format!("sw_row[{r}]"));
            names.push(format!("sw_row_on[{r}]"));
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
    // The swarm panels and "highlight and ask".
    // The draw tools.
    "tb_draw", "tb_draw_on", "ed_draw", "br_sculpt", "br_sculpt_on", "br_splat", "br_splat_on", "br_paint", "br_paint_on", "br_path", "br_path_on", "br_r_down", "br_r_up", "br_a_down", "br_a_up",
    "tb_claims", "tb_claims_on", "tb_ask", "in_ask", "tab_inspector", "tab_inspector_on", "tab_swarm", "tab_swarm_on", "sw_panel", "sw_journal", "sw_journal_on",
    "sw_claims", "sw_claims_on", "sw_board", "sw_board_on", "sw_review", "sw_review_on", "sw_cost", "sw_cost_on", "sw_agent", "sw_place", "sw_game", "sw_show",
    "sw_list", "sw_open_review", "sw_newest", "sw_newer", "sw_older",
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

impl EditorUi {
    /// A press on the Draw bar. The bar closed means no brush; a brush button pressed again
    /// steps to its next op, layer or colour; size and strength are kept from brush to brush.
    pub fn brush_act(&mut self, a: BrushAct) {
        let held = self.brush.as_ref().map(|b| b.kind);
        let with = |kind: BrushKind, kept: &Brush| Brush { kind, ..kept.clone() };
        match a {
            BrushAct::Bar => {
                self.draw_bar = !self.draw_bar;
                if !self.draw_bar {
                    self.brush = None;
                }
            }
            BrushAct::Sculpt => {
                let kind = match held {
                    Some(k @ (BrushKind::Raise | BrushKind::Lower | BrushKind::Flatten | BrushKind::Smooth)) => k.next_sculpt(),
                    _ => BrushKind::Raise,
                };
                self.brush = Some(with(kind, &self.brush_kept));
            }
            BrushAct::Splat => {
                let layer = match held {
                    Some(BrushKind::Splat(l)) => (l + 1) % 4,
                    _ => 0,
                };
                self.brush = Some(with(BrushKind::Splat(layer), &self.brush_kept));
            }
            BrushAct::Paint => {
                let mut b = with(BrushKind::Paint, &self.brush_kept);
                if held == Some(BrushKind::Paint) {
                    b.color = b.next_color().to_owned();
                }
                self.brush = Some(b);
            }
            BrushAct::Path => {
                let sweep = match held {
                    Some(BrushKind::Path(k)) => (k + 1) % crate::editor_brush::MAX_PATH_SWEEPS,
                    _ => 0,
                };
                self.brush = Some(with(BrushKind::Path(sweep), &self.brush_kept));
            }
            BrushAct::Radius(up) => {
                if let Some(b) = self.brush.as_mut() {
                    b.radius = b.stepped_radius(up);
                }
            }
            BrushAct::Amount(up) => {
                if let Some(b) = self.brush.as_mut() {
                    b.amount = b.stepped_amount(up);
                }
            }
        }
        if let Some(b) = &self.brush {
            self.draw_bar = true;
            self.brush_kept = b.clone();
        }
        self.dirty = true;
    }

    /// Whether an agent's injected frames hold the viewport's buttons now (a drag or a stroke of
    /// theirs is in progress): the viewport is theirs until they release, or their hold ends.
    pub fn agent_holds(&self) -> bool {
        self.holds.holder() == Some(Side::Agent)
    }

    /// Whether the human's own hand holds them: the viewport is theirs, and an agent's injected
    /// frames move nothing in it meanwhile.
    pub fn human_holds(&self) -> bool {
        self.holds.holder() == Some(Side::Human)
    }

    /// The human's holds are let go (the window takes no input for them: another screen holds
    /// it, the Ask box, minimized). Whether they held anything.
    pub fn release_human_holds(&mut self) -> bool {
        self.holds.drop_side(Side::Human, &mut [false; 3])
    }

    /// Puts the brush down (Escape, or the editor closing); the bar stays as it is.
    pub fn brush_down(&mut self) -> bool {
        let had = self.brush.take().is_some();
        self.dirty |= had;
        had
    }
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
        let rows = crate::editor_swarm::ROWS;
        assert!(ui.widget(SCREEN_NAME, &format!("sw_row[{}]", rows - 1)).is_some() && ui.widget(SCREEN_NAME, &format!("sw_row[{rows}]")).is_none(), "the swarm panel's ROWS is its repeat's max");
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

    /// The rule itself ([`Holds`]): all held buttons belong to one side; the other side's
    /// press and release are not read; a quiet hold yields to a press, and lapses by itself
    /// after minutes; no contact outlives its button. (A live run lost an agent's stroke to a
    /// click in another window; reviews R1 and R2 of m5-eddraw found the mixed states.)
    #[test]
    fn all_held_buttons_belong_to_one_side_and_a_quiet_hold_yields() {
        let mut none = [false; 3];
        for (holder, other) in [(Side::Agent, Side::Human), (Side::Human, Side::Agent)] {
            let mut h = Holds::default();
            assert!(!h.begin(holder, 10.0, true, true, &mut none) && h.press(holder, 0));
            assert_eq!(h.holder(), Some(holder));
            // The other side: no press of any button, no release, no contact.
            assert!(!h.press(other, 0) && !h.press(other, 1) && !h.release(other, 0));
            assert_eq!(h.contact_down(other, 7), Some(false));
            assert!(h.contact.is_none() && h.buttons == [true, false, false] && h.holder() == Some(holder));
            assert!(!h.drop_side(other, &mut none), "its focus loss lets go of nothing");
            // The holder presses a second button and releases both: nobody holds.
            assert!(h.press(holder, 1) && h.release(holder, 0) && h.holder() == Some(holder));
            assert!(h.release(holder, 1) && h.holder().is_none());
            // Quiet for under the limit: the other's press is still refused. Past it: the press
            // ends the hold (and is not itself a press). With no press, it holds for minutes.
            assert!(h.press(holder, 0));
            h.last[Holds::at(holder)] = 10.0;
            assert!(!h.begin(other, 10.0 + AGENT_HOLD_S - 0.1, true, true, &mut none));
            assert!(!h.begin(other, 10.0 + AGENT_HOLD_S + 1.0, true, false, &mut none), "quiet, but nobody presses");
            let mut released = [false; 3];
            assert!(h.begin(other, 10.0 + AGENT_HOLD_S + 1.0, true, true, &mut released));
            assert!(released == [true, false, false] && h.holder().is_none());
            assert!(h.press(holder, 2));
            h.last[Holds::at(holder)] = 10.0;
            assert!(!h.begin(other, 10.0 + AGENT_HOLD_MAX_S - 1.0, false, false, &mut none));
            assert!(h.begin(other, 10.0 + AGENT_HOLD_MAX_S + 1.0, false, false, &mut none) && h.holder().is_none());
            // The holder's own activity keeps its hold (it is not quiet).
            assert!(h.press(holder, 0));
            h.last[Holds::at(holder)] = 10.0;
            assert!(!h.begin(holder, 10.0 + AGENT_HOLD_S + 100.0, true, false, &mut none));
            assert!(!h.begin(other, 10.0 + AGENT_HOLD_S + 101.0, true, true, &mut none), "it moved a second ago");
        }
        // A contact is the left button: it never outlives it (review R2, N2: a lapse cleared the
        // button and left the contact, and every later touch read as not pressed).
        let mut h = Holds::default();
        assert_eq!(h.contact_down(Side::Agent, 9), Some(true));
        assert_eq!(h.contact_down(Side::Agent, 10), None, "a second finger is no press");
        h.last[1] = 0.0;
        assert!(h.begin(Side::Human, AGENT_HOLD_S + 1.0, true, true, &mut none) && h.contact.is_none());
        assert_eq!(h.contact_down(Side::Human, 3), Some(true), "the human's next touch is theirs");
        assert!(!h.contact_up(Side::Human, 4) && h.contact_up(Side::Human, 3) && h.holder().is_none());
        // A touch down then the same side's mouse-up of the left button: the contact goes too.
        assert_eq!(h.contact_down(Side::Agent, 5), Some(true));
        assert!(h.release(Side::Agent, 0) && h.contact.is_none() && h.holder().is_none());
        // A focus loss ends what its own side holds.
        assert!(h.press(Side::Human, 0) && h.press(Side::Human, 2));
        let mut released = [false; 3];
        assert!(h.drop_side(Side::Human, &mut released) && released == [true, false, true] && h.holder().is_none());
    }

    /// The same through a frame's events: what the viewport is told.
    #[test]
    fn the_viewport_is_told_whose_hold_it_is_and_what_was_refused() {
        use specula_platform::PointerKind;
        let size = (1280, 800);
        let (x, y) = (640, 400);
        let down = Event::MouseDown { button: MouseButton::Left, x, y };
        let up = Event::MouseUp { button: MouseButton::Left, x, y };
        let mut e = test_ui();
        e.open();
        e.frame(&[], &[], size, false);
        // The human's own drag: a focus loss releases it and says so.
        let (_, v) = e.frame(&[Event::MouseMove { x, y }, down.clone()], &[], size, false);
        assert!(v.left.pressed && v.left.held && e.human_holds() && v.presses_refused == 0);
        // (An agent's press meanwhile is refused and counted; its release and its own focus
        // loss touch nothing of the human's.)
        e.injected_pass = true;
        let (_, v) = e.frame(&[down.clone(), Event::MouseDown { button: MouseButton::Right, x, y }], &[], size, false);
        assert!(!v.left.pressed && !v.right.pressed && v.presses_refused == 2 && e.human_holds(), "{v:?}");
        let (_, v) = e.frame(&[up.clone(), Event::Focused(false)], &[], size, false);
        assert!(!v.left.released && !v.focus_lost && v.left.held && e.human_holds(), "{v:?}");
        e.injected_pass = false;
        let (_, v) = e.frame(&[Event::Focused(false)], &[], size, false);
        assert!(v.focus_lost && v.left.released && !v.left.held && !e.human_holds());
        // An agent's hold: the window's focus loss, the human's mouse, touch and pen leave it.
        e.injected_pass = true;
        let (_, v) = e.frame(&[Event::MouseMove { x, y }, down.clone()], &[], size, false);
        assert!(v.left.pressed && e.agent_holds());
        e.injected_pass = false;
        let (_, v) = e.frame(&[Event::Focused(false), up.clone(), down.clone()], &[], size, false);
        assert!(!v.focus_lost && !v.left.released && !v.left.pressed && v.left.held && v.presses_refused == 1 && e.agent_holds(), "{v:?}");
        for kind in [PointerKind::Touch, PointerKind::Pen] {
            let (_, v) = e.frame(&[Event::PointerDown { id: 5, kind, x: x + 30, y }], &[], size, false);
            assert!(!v.left.pressed && v.left.held && v.presses_refused == 1, "{kind:?}");
            let (_, v) = e.frame(&[Event::PointerUp { id: 5, kind, x: x + 30, y }], &[], size, false);
            assert!(!v.left.released && v.left.held && e.agent_holds(), "{kind:?}");
        }
        // The agent's own release ends it; the human's next press is theirs.
        e.injected_pass = true;
        let (_, v) = e.frame(std::slice::from_ref(&up), &[], size, false);
        assert!(v.left.released && !e.agent_holds());
        // An agent that pressed and went quiet: the human's press takes the viewport back (that
        // press ends the hold and is not taken itself; their next one is).
        e.frame(std::slice::from_ref(&down), &[], size, false);
        e.injected_pass = false;
        e.holds.last[1] -= AGENT_HOLD_S + 1.0;
        let (_, v) = e.frame(&[Event::MouseMove { x, y }], &[], size, false);
        assert!(!v.hold_lapsed && v.left.held && e.agent_holds(), "quiet, but the human only moved");
        let (_, v) = e.frame(std::slice::from_ref(&down), &[], size, false);
        assert!(v.hold_lapsed && v.focus_lost && v.left.released && !v.left.pressed && !v.left.held && v.presses_refused == 1, "{v:?}");
        let (_, v) = e.frame(std::slice::from_ref(&down), &[], size, false);
        assert!(v.left.pressed && e.human_holds() && !v.hold_lapsed);
        // The other way (review R2, N1): a human hold whose release never came yields to an
        // agent's press once it has been quiet.
        e.holds.last[0] -= AGENT_HOLD_S + 1.0;
        e.injected_pass = true;
        let (_, v) = e.frame(std::slice::from_ref(&down), &[], size, false);
        assert!(v.hold_lapsed && v.focus_lost && v.presses_refused == 1 && !e.human_holds() && !e.agent_holds(), "{v:?}");
        let (_, v) = e.frame(std::slice::from_ref(&down), &[], size, false);
        assert!(v.left.pressed && e.agent_holds());
        e.frame(std::slice::from_ref(&up), &[], size, false);
        e.injected_pass = false;
        // A window that takes no input for the human lets their hold go: blocked by another
        // screen, minimized, or told so by the engine (the Ask box); and closing forgets all.
        for case in 0..4 {
            let (_, v) = e.frame(std::slice::from_ref(&down), &[], size, false);
            assert!(v.left.pressed && e.human_holds(), "case {case}");
            match case {
                0 => drop(e.frame(&[], &[], size, true)),
                1 => drop(e.frame(&[], &[], (0, 0), false)),
                2 => assert!(e.release_human_holds()),
                _ => {
                    e.close();
                    e.open();
                }
            }
            assert!(!e.human_holds(), "case {case}");
        }
        // The viewport's pointer is its holder's own: an agent's injected moves don't bend a
        // stroke the human is drawing (found by the live check: the human's stroke was sent
        // with the agent's points in it), nor the human's mouse an agent's.
        let (_, v) = e.frame(&[Event::MouseMove { x: 300, y: 300 }, Event::MouseDown { button: MouseButton::Left, x: 300, y: 300 }], &[], size, false);
        assert_eq!(v.pointer, Some([300.0, 300.0]));
        e.injected_pass = true;
        e.frame(&[Event::MouseMove { x: 900, y: 500 }], &[], size, false);
        e.injected_pass = false;
        let (_, v) = e.frame(&[], &[], size, false);
        assert_eq!(v.pointer, Some([300.0, 300.0]), "the human holds: their own pointer");
        let (_, v) = e.frame(&[Event::MouseUp { button: MouseButton::Left, x: 310, y: 300 }], &[], size, false);
        assert_eq!(v.pointer, Some([310.0, 300.0]));
        // With nothing held the pointer is whichever side's acted last (an agent's hover shows).
        e.injected_pass = true;
        e.frame(&[Event::MouseMove { x: 900, y: 500 }, Event::MouseDown { button: MouseButton::Left, x: 900, y: 500 }], &[], size, false);
        e.injected_pass = false;
        let (_, v) = e.frame(&[Event::MouseMove { x: 20, y: 20 }], &[], size, false);
        assert_eq!(v.pointer, Some([900.0, 500.0]), "the agent holds: its pointer, whatever the human's mouse does");
        e.injected_pass = true;
        e.frame(&[Event::MouseUp { button: MouseButton::Left, x: 900, y: 500 }], &[], size, false);
        e.injected_pass = false;
        let (_, v) = e.frame(&[], &[], size, false);
        assert_eq!(v.pointer, Some([900.0, 500.0]), "nothing held: the agent's pointer acted last");
        let (_, v) = e.frame(&[Event::MouseMove { x: 20, y: 20 }], &[], size, false);
        assert_eq!(v.pointer, Some([20.0, 20.0]), "then the human's");
        // A side's own Shift and Ctrl while it holds (review R3, F2: the window's focus loss
        // dropped the Ctrl an agent's drag held, and its snapping flipped mid-drag).
        e.injected_pass = true;
        e.frame(&[Event::MouseUp { button: MouseButton::Left, x: 900, y: 500 }], &[], size, false);
        let (_, v) = e.frame(&[Event::KeyDown { code: 0x11, repeat: false }, Event::MouseMove { x: 700, y: 400 }, Event::MouseDown { button: MouseButton::Left, x: 700, y: 400 }], &[], size, false);
        assert!(v.ctrl && !v.shift && v.left.pressed && e.agent_holds(), "{v:?}");
        e.injected_pass = false;
        e.frame(&[Event::Focused(false), Event::KeyDown { code: 0x10, repeat: false }], &[], size, false);
        e.injected_pass = true;
        let (_, v) = e.frame(&[Event::MouseMove { x: 720, y: 400 }], &[], size, false);
        assert!(v.ctrl && !v.shift && v.left.held, "the agent's Ctrl is its own; the human's Shift is not its: {v:?}");
        let (_, v) = e.frame(&[Event::KeyUp { code: 0x11 }, Event::MouseUp { button: MouseButton::Left, x: 720, y: 400 }], &[], size, false);
        assert!(v.left.released && !e.agent_holds());
        e.injected_pass = false;
        // (An agent's hold is not the blocked window's to end: its frames wait and come back.)
        e.injected_pass = true;
        e.frame(std::slice::from_ref(&down), &[], size, false);
        e.injected_pass = false;
        e.frame(&[], &[], size, true);
        assert!(e.agent_holds() && !e.release_human_holds());
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

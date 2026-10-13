//! The runtime: screens + state + input in, a draw list + events out, every frame.
//!
//! ```text
//! let mut ui = Ui::new(theme, FontSet::new().with("ui", font));
//! ui.load_screen(&std::fs::read_to_string("hud.json")?)?;
//! ui.show("hud")?;
//! // every frame:
//! let view = Viewport::new([w, h], ui.theme().scale.scale([w, h], ppi, user_scale));
//! let frame = ui.frame(&input, &state, &view);
//! if !frame.events.consumed.pointer && input.pressed("place") { /* the game's click */ }
//! for a in frame.events.actions() { /* "resume", "build:warding", ... */ }
//! renderer.draw(frame.draw, ui.atlas());
//! ```

use crate::color::{mix, with_alpha, Rgba, TRANSPARENT};
use crate::draw::{DrawList, Rect, RectCmd};
use crate::error::{ErrorCode, UiError};
use crate::geom::{neighbour, radial_pick, threat_arrow, Dir};
use crate::layout::{layout, Computed, Ctx, Measure};
use crate::screen::{ColorRef, IconSource, InputMode, Kind, Node, Num, Screen, TextAlign, TextSource};
use crate::shapes::{self, Scratch};
use crate::state::{Scope, StateMap, Value};
use crate::theme::{BoxStyle, ButtonLook, Theme};
use specula_core::json::Json;
use specula_input::{DeviceFamily, Input, PadStyle, Source};
use specula_platform::{keys, GamepadButton, MouseButton};
use specula_text::{layout_into, AtlasConfig, Font, GlyphAtlas, TextLayout, TextStyle};
use std::rc::Rc;

/// Named fonts for the theme's roles (`"ui"`, `"title"`). A role whose font slot is missing uses
/// the first font.
#[derive(Default)]
pub struct FontSet {
    fonts: Vec<(String, Font)>,
}

impl FontSet {
    pub fn new() -> FontSet {
        FontSet::default()
    }

    pub fn with(mut self, name: &str, font: Font) -> FontSet {
        self.add(name, font);
        self
    }

    pub fn add(&mut self, name: &str, font: Font) {
        match self.fonts.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = font,
            None => self.fonts.push((name.to_owned(), font)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// Index of the font for a slot name (falls back to the first font).
    pub fn index(&self, name: &str) -> Option<usize> {
        if self.fonts.is_empty() {
            return None;
        }
        Some(self.fonts.iter().position(|(n, _)| n == name).unwrap_or(0))
    }

    pub fn font(&self, i: usize) -> &Font {
        &self.fonts[i].1
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.fonts.iter().map(|(n, _)| n.as_str())
    }
}

/// The target: size and scale (physical px per logical px), safe-area insets in physical px
/// `[top, right, bottom, left]` (notches, rounded corners, TV overscan).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub size: [u32; 2],
    pub scale: f32,
    pub safe_area: [f32; 4],
}

impl Viewport {
    pub fn new(size: [u32; 2], scale: f32) -> Viewport {
        Viewport { size, scale: if scale.is_finite() && scale > 0.0 { scale } else { 1.0 }, safe_area: [0.0; 4] }
    }

    pub fn with_safe_area(mut self, insets: [f32; 4]) -> Viewport {
        self.safe_area = insets.map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 });
        self
    }

    pub fn full(&self) -> Rect {
        Rect::new(0.0, 0.0, self.size[0] as f32, self.size[1] as f32)
    }

    pub fn safe(&self) -> Rect {
        let s = self.safe_area;
        self.full().inset(s[0], s[1], s[2], s[3])
    }
}

/// Identifies a loaded screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScreenId(pub usize);

/// A widget: screen and node index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WidgetRef {
    pub screen: usize,
    pub node: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    /// A button or radial item was activated (click, tap, accept).
    Action { action: Rc<str>, widget: WidgetRef },
    /// A disabled button or item was activated (play a "no" sound, say why).
    Refused { widget: WidgetRef },
    /// "Back" on a menu screen (its `back` action, if any, is also sent as an `Action`).
    Back { screen: usize },
}

/// What the UI took this frame, so the game doesn't also act on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Consumed {
    /// The pointer is over UI, or a press that started on UI is still held: ignore mouse /
    /// touch gameplay actions (placing, orbit by drag).
    pub pointer: bool,
    /// A menu screen has gamepad / keyboard focus: ignore pad and key gameplay actions.
    pub navigation: bool,
    /// A modal screen is open: ignore all gameplay input.
    pub all: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiEvents {
    pub events: Vec<UiEvent>,
    pub consumed: Consumed,
    pub hovered: Option<WidgetRef>,
    pub focused: Option<WidgetRef>,
}

impl UiEvents {
    /// Action names sent this frame.
    pub fn actions(&self) -> impl Iterator<Item = &str> {
        self.events.iter().filter_map(|e| match e {
            UiEvent::Action { action, .. } => Some(&**action),
            _ => None,
        })
    }
}

/// One frame's output, borrowed from the [`Ui`] (no allocation).
pub struct Frame<'a> {
    pub draw: &'a DrawList,
    pub events: &'a UiEvents,
}

/// Kinds of toast message (colour of the accent stripe).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Good,
    Warn,
    Threat,
}

#[derive(Default)]
pub(crate) struct NodeRt {
    pub enabled: bool,
    pub text: String,
    pub laid: TextLayout,
    laid_text: String,
    laid_wrap: f32,
    laid_size: f32,
    laid_font: usize,
    laid_ok: bool,
    pub font: Option<usize>,
}

pub(crate) struct ScreenRt {
    pub screen: Screen,
    pub visible: bool,
    pub comp: Vec<Computed>,
    pub rt: Vec<NodeRt>,
    pub focus: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Hit {
    pub rect: Rect,
    pub w: WidgetRef,
    pub interactive: bool,
}

#[derive(Default)]
struct Toast {
    text: String,
    kind: Option<ToastKind>,
    age: f32,
    laid: TextLayout,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct PointerSt {
    pos: Option<[f32; 2]>,
    down: bool,
    press: Option<WidgetRef>,
    press_pos: [f32; 2],
    /// The press began on UI: the whole press belongs to the UI.
    captured: bool,
    dragging_list: Option<WidgetRef>,
    hover: Option<WidgetRef>,
    /// The touch / pen contact that made the press (`None`: the mouse).
    press_contact: Option<u32>,
}

/// Names of the input actions the UI reads (with raw fallbacks when the game's map lacks them).
pub const UI_ACTIONS: [&str; 8] = ["ui_up", "ui_down", "ui_left", "ui_right", "ui_accept", "ui_back", "ui_point", "ui_scroll"];

/// The UI's input actions as an action map file, for games to merge into theirs
/// (`specula_ui::add_ui_actions`). Gamepad: D-pad / left stick navigate, South accepts, East goes
/// back; keyboard: arrows, Tab, Enter / Space, Escape.
pub const UI_ACTIONS_JSON: &str = r#"{
  "format": "specula.input.v1",
  "actions": [
    { "name": "ui_up", "kind": "button", "group": "ui", "description": "Move UI focus up.", "bindings": ["Key.Up", "Pad.DpadUp", { "source": "Pad.LeftY", "threshold": 0.6 }] },
    { "name": "ui_down", "kind": "button", "group": "ui", "description": "Move UI focus down.", "bindings": ["Key.Down", "Key.Tab", "Pad.DpadDown", { "source": "Pad.LeftY", "scale": -1, "threshold": 0.6 }] },
    { "name": "ui_left", "kind": "button", "group": "ui", "description": "Move UI focus left.", "bindings": ["Key.Left", "Pad.DpadLeft", { "source": "Pad.LeftX", "scale": -1, "threshold": 0.6 }] },
    { "name": "ui_right", "kind": "button", "group": "ui", "description": "Move UI focus right.", "bindings": ["Key.Right", "Pad.DpadRight", { "source": "Pad.LeftX", "threshold": 0.6 }] },
    { "name": "ui_accept", "kind": "button", "group": "ui", "description": "Press the focused button.", "bindings": ["Key.Enter", "Key.Space", "Pad.South"] },
    { "name": "ui_back", "kind": "button", "group": "ui", "description": "Close the menu / go back.", "text_mode": true, "bindings": ["Key.Escape", "Pad.East"] },
    { "name": "ui_point", "kind": "axis2", "output": "value", "group": "ui", "description": "Point at a radial menu item (+y down).", "bindings": [{ "source": "Pad.LeftStick", "scale": [1, -1] }] },
    { "name": "ui_scroll", "kind": "axis", "output": "value", "group": "ui", "description": "Scroll lists (+ = down).", "bindings": [{ "source": "Mouse.Wheel", "scale": -1 }, { "source": "Pad.RightY", "scale": -0.25 }] }
  ]
}"#;

/// Add any of the UI's actions the map lacks (existing actions of the same name are kept).
pub fn add_ui_actions(map: &mut specula_input::ActionMap) -> Result<(), specula_input::InputError> {
    let ui = specula_input::ActionMap::from_json_str(UI_ACTIONS_JSON)?;
    for a in ui.actions() {
        if map.action(&a.name).is_none() {
            map.add_action(a.clone())?;
        }
    }
    Ok(())
}

#[derive(Default, Clone, Copy)]
struct NavIn {
    dirs: [(bool, bool); 4],
    accept: bool,
    back: bool,
    point: [f32; 2],
    scroll: f32,
}

fn read_nav(input: &Input) -> NavIn {
    let has = |a: &str| input.map().index_of(a).is_some();
    let mut n = NavIn::default();
    let raw = |srcs: &[Source]| -> (bool, bool) {
        (srcs.iter().any(|&s| input.source_pressed(s)), srcs.iter().any(|&s| input.source_held(s)))
    };
    // Key code 0 is never pressed, so a missing key name simply never fires.
    let key = |name: &str| Source::Key(keys::from_name(name).unwrap_or(0));
    let fallback: [[Source; 2]; 4] = [
        [key("Up"), Source::Pad(GamepadButton::DpadUp)],
        [key("Down"), Source::Pad(GamepadButton::DpadDown)],
        [key("Left"), Source::Pad(GamepadButton::DpadLeft)],
        [key("Right"), Source::Pad(GamepadButton::DpadRight)],
    ];
    for (d, name) in UI_ACTIONS[..4].iter().enumerate() {
        n.dirs[d] = if has(name) {
            (input.pressed(name), input.held(name))
        } else {
            raw(&fallback[d])
        };
    }
    n.accept = if has("ui_accept") {
        input.pressed("ui_accept")
    } else {
        [key("Enter"), key("Space"), Source::Pad(GamepadButton::South)].iter().any(|&s| input.source_pressed(s))
    };
    n.back = if has("ui_back") {
        input.pressed("ui_back")
    } else {
        [key("Escape"), Source::Pad(GamepadButton::East)].iter().any(|&s| input.source_pressed(s))
    };
    if has("ui_point") {
        n.point = input.value2("ui_point");
    }
    n.scroll = if has("ui_scroll") {
        input.value("ui_scroll")
    } else {
        let up = input.source_held(Source::WheelUp);
        let down = input.source_held(Source::WheelDown);
        f32::from(u8::from(down)) - f32::from(u8::from(up))
    };
    n
}

/// The UI toolkit instance: theme, fonts, glyph atlas, loaded screens, and per-frame output.
pub struct Ui {
    pub(crate) theme: Theme,
    pub(crate) fonts: FontSet,
    pub(crate) atlas: GlyphAtlas,
    pub(crate) screens: Vec<ScreenRt>,
    order: Vec<usize>,
    pub(crate) draw: DrawList,
    pub(crate) hits: Vec<Hit>,
    events: UiEvents,
    time: f64,
    fixed_time: Option<f64>,
    toasts: Vec<Toast>,
    nav_held: [f32; 4],
    pointer: PointerSt,
    focus_visible: bool,
    prompt_key: Option<(Option<DeviceFamily>, PadStyle)>,
    prompts: Vec<(String, Option<String>)>,
    scratch: Scratch,
    pending: Vec<(WidgetRef, bool)>,
    tip: Option<WidgetRef>,
    tip_since: f64,
    tip_text: String,
    tip_laid: TextLayout,
    /// Threat-arrow labels: text, layout, and the scale it was laid out at.
    threat_text: Vec<(String, TextLayout, f32)>,
    pub(crate) last_view: Option<Viewport>,
    /// Image textures the game registered (`None`: none yet). See [`Ui::set_textures`].
    pub(crate) textures: Option<Vec<u32>>,
    /// The `ui.*` keys conditions and templates can read.
    pub(crate) env: StateMap,
}

impl Ui {
    pub fn new(theme: Theme, fonts: FontSet) -> Ui {
        Ui {
            theme,
            fonts,
            atlas: GlyphAtlas::new(AtlasConfig::default()),
            screens: Vec::new(),
            order: Vec::new(),
            draw: DrawList::default(),
            hits: Vec::new(),
            events: UiEvents::default(),
            time: 0.0,
            fixed_time: None,
            toasts: Vec::new(),
            nav_held: [0.0; 4],
            pointer: PointerSt::default(),
            focus_visible: false,
            prompt_key: None,
            prompts: Vec::new(),
            scratch: Scratch::default(),
            pending: Vec::new(),
            tip: None,
            tip_since: 0.0,
            tip_text: String::new(),
            tip_laid: TextLayout::default(),
            threat_text: Vec::new(),
            last_view: None,
            env: StateMap::new(),
            textures: None,
        }
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn fonts(&self) -> &FontSet {
        &self.fonts
    }

    /// The glyph atlas the draw list's glyph quads sample (upload `take_dirty()` rects each frame).
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    pub fn atlas_mut(&mut self) -> &mut GlyphAtlas {
        &mut self.atlas
    }

    /// Replace the theme (high contrast, colour-vision variants). Every screen is re-validated
    /// against it; on error nothing changes.
    pub fn set_theme(&mut self, theme: Theme) -> Result<(), UiError> {
        let mut parsed = Vec::with_capacity(self.screens.len());
        for s in &self.screens {
            parsed.push(Screen::parse(&s.screen.source, &theme)?);
        }
        for (s, p) in self.screens.iter_mut().zip(parsed) {
            s.screen = p;
            s.rt.iter_mut().for_each(|r| r.laid_ok = false);
        }
        self.theme = theme;
        Ok(())
    }

    /// Load (or hot-reload, by name) a screen. A reloaded screen keeps its visibility.
    pub fn load_screen(&mut self, json: &str) -> Result<ScreenId, UiError> {
        let screen = Screen::parse(json, &self.theme)?;
        let n = screen.nodes.len();
        let rt = ScreenRt {
            focus: screen.focus,
            screen,
            visible: false,
            comp: vec![Computed::default(); n],
            rt: (0..n).map(|_| NodeRt::default()).collect(),
        };
        let id = match self.screens.iter().position(|s| s.screen.name == rt.screen.name) {
            Some(i) => {
                let visible = self.screens[i].visible;
                self.forget_screen(i);
                self.screens[i] = rt;
                self.screens[i].visible = visible;
                i
            }
            None => {
                self.screens.push(rt);
                self.screens.len() - 1
            }
        };
        self.reorder();
        Ok(ScreenId(id))
    }

    pub fn screen_id(&self, name: &str) -> Option<ScreenId> {
        self.screens.iter().position(|s| s.screen.name == name).map(ScreenId)
    }

    fn need(&self, name: &str) -> Result<usize, UiError> {
        self.screen_id(name).map(|s| s.0).ok_or_else(|| {
            let names: Vec<&str> = self.screens.iter().map(|s| s.screen.name.as_str()).collect();
            UiError::new(ErrorCode::UnknownScreen, "", format!("no screen \"{name}\"; loaded: {}", names.join(", ")))
        })
    }

    pub fn screen(&self, id: ScreenId) -> &Screen {
        &self.screens[id.0].screen
    }

    pub fn show(&mut self, name: &str) -> Result<(), UiError> {
        let i = self.need(name)?;
        if !self.screens[i].visible {
            self.screens[i].visible = true;
            self.screens[i].focus = self.screens[i].screen.focus;
            self.reorder();
        }
        Ok(())
    }

    pub fn hide(&mut self, name: &str) -> Result<(), UiError> {
        let i = self.need(name)?;
        self.screens[i].visible = false;
        // Nothing of a hidden screen is laid out, hit, pressed or hovered.
        for c in &mut self.screens[i].comp {
            c.visible = false;
        }
        self.forget_screen(i);
        self.reorder();
        Ok(())
    }

    /// Drop every reference to widgets of screen `i` (it was reloaded or hidden): a press in
    /// progress must not activate a different widget that now has the same index.
    fn forget_screen(&mut self, i: usize) {
        let of = |w: Option<WidgetRef>| w.filter(|w| w.screen != i);
        self.pointer.press = of(self.pointer.press);
        self.pointer.hover = of(self.pointer.hover);
        self.pointer.dragging_list = of(self.pointer.dragging_list);
        self.tip = of(self.tip);
        self.events.hovered = of(self.events.hovered);
        self.events.focused = of(self.events.focused);
        self.pending.retain(|(w, _)| w.screen != i);
        self.hits.retain(|h| h.w.screen != i);
    }

    /// Can input reach widget `w` now? (exists, its screen is shown, no modal screen above it)
    pub fn reachable(&self, w: WidgetRef) -> Result<(), UiError> {
        let Some(sr) = self.screens.get(w.screen) else {
            return Err(UiError::new(ErrorCode::UnknownWidget, "", format!("no screen #{}", w.screen)));
        };
        if w.node as usize >= sr.screen.nodes.len() {
            return Err(UiError::new(ErrorCode::UnknownWidget, "", format!("screen \"{}\" has no widget #{}", sr.screen.name, w.node)));
        }
        if !sr.visible {
            return Err(UiError::new(ErrorCode::NotVisible, "", format!("screen \"{}\" is hidden", sr.screen.name)));
        }
        let pos = self.order.iter().position(|&s| s == w.screen).unwrap_or(0);
        if let Some(m) = self.order[pos + 1..].iter().find(|&&s| self.screens[s].screen.modal) {
            let name = &self.screens[*m].screen.name;
            return Err(UiError::new(ErrorCode::ModalBlocks, "", format!("modal screen \"{name}\" is open above \"{}\"", sr.screen.name)));
        }
        Ok(())
    }

    pub fn is_visible(&self, name: &str) -> bool {
        self.screen_id(name).is_some_and(|s| self.screens[s.0].visible)
    }

    /// Visible screens, bottom to top.
    pub fn visible_screens(&self) -> impl Iterator<Item = &str> {
        self.order.iter().map(|&i| self.screens[i].screen.name.as_str())
    }

    pub(crate) fn reorder(&mut self) {
        self.order.clear();
        self.order.extend((0..self.screens.len()).filter(|&i| self.screens[i].visible));
        let screens = &self.screens;
        self.order.sort_by_key(|&i| screens[i].screen.layer);
    }

    /// Widget by screen name and id.
    pub fn widget(&self, screen: &str, id: &str) -> Option<WidgetRef> {
        let s = self.screen_id(screen)?.0;
        self.screens[s].screen.index_of(id).map(|node| WidgetRef { screen: s, node })
    }

    pub fn widget_id(&self, w: WidgetRef) -> Option<&str> {
        self.screens.get(w.screen)?.screen.nodes.get(w.node as usize)?.id.as_deref()
    }

    /// Number of loaded screens.
    pub fn screen_count(&self) -> usize {
        self.screens.len()
    }

    /// A widget's JSON path in its screen (`root.children[2]`), for widgets without an id.
    pub fn widget_path(&self, w: WidgetRef) -> Option<&str> {
        Some(self.screens.get(w.screen)?.screen.nodes.get(w.node as usize)?.path.as_str())
    }

    pub fn screen_name(&self, screen: usize) -> &str {
        &self.screens[screen].screen.name
    }

    /// Layout result of a widget from the last frame (physical px).
    pub fn rect(&self, w: WidgetRef) -> Option<Rect> {
        let s = self.screens.get(w.screen)?;
        let c = s.comp.get(w.node as usize)?;
        c.visible.then_some(c.rect)
    }

    /// Focus a widget (agents, tests, or game logic), applied at the next frame. Refused with a
    /// structured error (`unknown_widget`, `not_visible`, `modal_blocks` naming the modal) when
    /// input couldn't reach it, so agents can't do what a player couldn't.
    pub fn focus(&mut self, w: WidgetRef) -> Result<(), UiError> {
        self.reachable(w)?;
        self.pending.push((w, false));
        Ok(())
    }

    /// Activate a widget as if clicked (agents and headless tests), at the next frame. Refused
    /// like [`Ui::focus`].
    pub fn activate(&mut self, w: WidgetRef) -> Result<(), UiError> {
        self.reachable(w)?;
        self.pending.push((w, true));
        Ok(())
    }

    /// Show a toast message.
    pub fn toast(&mut self, text: &str, kind: ToastKind) {
        let slot = match self.toasts.iter().position(|t| t.kind.is_none()) {
            Some(i) => i,
            None if self.toasts.len() < 8 => {
                self.toasts.push(Toast::default());
                self.toasts.len() - 1
            }
            None => {
                // Replace the oldest.
                let mut oldest = 0;
                for (i, t) in self.toasts.iter().enumerate() {
                    if t.age > self.toasts[oldest].age {
                        oldest = i;
                    }
                }
                oldest
            }
        };
        let t = &mut self.toasts[slot];
        t.text.clear();
        t.text.push_str(text);
        t.kind = Some(kind);
        t.age = 0.0;
    }

    /// Register the texture ids image icons may use (`specula_gpu::TextureId` numbers the game
    /// uploaded). `ui_check` flags images outside this set and `gpu::GpuOverlay` skips them
    /// (instead of the whole GPU frame failing on an unknown texture).
    pub fn set_textures(&mut self, ids: &[u32]) {
        let mut v = ids.to_vec();
        v.sort_unstable();
        v.dedup();
        self.textures = Some(v);
    }

    /// The registered image textures, sorted (`None` before [`Ui::set_textures`]).
    pub fn textures(&self) -> Option<&[u32]> {
        self.textures.as_deref()
    }

    /// Remove every toast.
    pub fn clear_toasts(&mut self) {
        for t in &mut self.toasts {
            t.kind = None;
        }
    }

    /// Fix the UI clock (seconds) for reproducible previews; `None` follows `input.dt()`.
    pub fn set_time(&mut self, t: Option<f64>) {
        self.fixed_time = t;
        if let Some(t) = t {
            self.time = t;
        }
    }

    /// Hit-test the last frame: the topmost interactive widget under `p` (physical px).
    pub fn hit_test(&self, p: [f32; 2]) -> Option<WidgetRef> {
        self.hits.iter().rev().find(|h| h.interactive && h.rect.contains(p)).map(|h| h.w)
    }

    /// The hit-test map of the last frame: `(rect, widget, interactive)` bottom to top.
    pub fn hits(&self) -> impl Iterator<Item = (Rect, WidgetRef, bool)> + '_ {
        self.hits.iter().map(|h| (h.rect, h.w, h.interactive))
    }

    pub fn draw_list(&self) -> &DrawList {
        &self.draw
    }

    // ---- the frame ----

    /// Build one frame: evaluate bindings, lay out, handle input, draw.
    pub fn frame(&mut self, input: &Input, state: &StateMap, view: &Viewport) -> Frame<'_> {
        let dt = input.dt().clamp(0.0, 0.25);
        match self.fixed_time {
            Some(t) => self.time = t,
            None => self.time += f64::from(dt),
        }
        self.events.events.clear();
        self.events.consumed = Consumed::default();
        self.draw.clear();
        self.draw.size = view.size;
        self.draw.scale = view.scale;
        self.atlas.begin_frame();
        self.last_view = Some(*view);
        for t in &mut self.toasts {
            if t.kind.is_some() {
                t.age += dt;
                if t.age > self.theme.toast.duration_s + self.theme.toast.fade_s {
                    t.kind = None;
                }
            }
        }
        self.update_prompts(input);
        let safe = view.safe();
        let (lw, lh) = (safe.w / view.scale, safe.h / view.scale);
        let device = input.last_device().map(|d| d.family());
        self.env
            .set_num("ui.width", f64::from(lw.round()))
            .set_num("ui.height", f64::from(lh.round()))
            .set_bool("ui.portrait", lh > lw)
            .set_bool("ui.narrow", lw < 760.0)
            .set_bool("ui.gamepad", device == Some(DeviceFamily::Gamepad))
            .set_bool("ui.touch", device == Some(DeviceFamily::Touch));
        for k in 0..self.order.len() {
            let s = self.order[k];
            self.prepare(s, state, view);
        }
        self.collect_hits();
        let relayout = self.handle_input(input, dt);
        for k in 0..self.order.len() {
            let s = self.order[k];
            if relayout & (1u64 << (s.min(63))) != 0 {
                self.prepare(s, state, view);
            }
        }
        if relayout != 0 {
            self.collect_hits();
        }
        self.render(state, view);
        Frame { draw: &self.draw, events: &self.events }
    }

    fn update_prompts(&mut self, input: &Input) {
        let key = (input.last_device().map(|d| d.family()), input.pad_style());
        if self.prompt_key == Some(key) {
            return;
        }
        self.prompt_key = Some(key);
        // Collect every prompt action once; glyphs come from the input map (rebinding-aware).
        let mut actions: Vec<String> = Vec::new();
        for s in &self.screens {
            for n in &s.screen.nodes {
                if let Kind::Label { text: TextSource::Prompt(a), .. } = &n.kind {
                    if !actions.contains(a) {
                        actions.push(a.clone());
                    }
                }
            }
        }
        self.prompts = actions.into_iter().map(|a| { let g = input.prompt(&a); (a, g) }).collect();
    }

    /// Visibility, enabled, texts and layout of one screen.
    fn prepare(&mut self, s: usize, state: &StateMap, view: &Viewport) {
        // Texts can depend on focus (the radial centre): if the fix-up moved it, go again.
        if self.prepare_once(s, state, view) {
            self.prepare_once(s, state, view);
        }
    }

    /// One pass of [`Ui::prepare`]; returns true if it had to move the focus.
    fn prepare_once(&mut self, s: usize, state: &StateMap, view: &Viewport) -> bool {
        let theme = &self.theme;
        let fonts = &self.fonts;
        let prompts = &self.prompts;
        let env = &self.env;
        let sr = &mut self.screens[s];
        let screen = &sr.screen;
        let n = screen.nodes.len();
        for i in 0..n {
            let node = &screen.nodes[i];
            let (pv, pe) = match node.parent {
                Some(p) => (sr.comp[p as usize].visible, sr.rt[p as usize].enabled),
                None => (true, true),
            };
            let scope = scope_for(screen, node, state, env);
            let mut vis = pv && scope.is_some();
            let scope = scope.unwrap_or(Scope::new(state).with_env(env));
            if vis {
                vis = node.visible.eval(&scope);
            }
            sr.rt[i].enabled = pe && node.enabled.eval(&scope);
            if vis {
                match &node.kind {
                    Kind::Label { text, role, .. } => {
                        let rt = &mut sr.rt[i];
                        rt.font = fonts.index(&theme.role(*role).font);
                        match text {
                            TextSource::Template(t) => t.render(&scope, &mut rt.text),
                            TextSource::Prompt(a) => {
                                rt.text.clear();
                                match prompts.iter().find(|(n, _)| n == a).and_then(|(_, g)| g.as_deref()) {
                                    Some(g) => rt.text.push_str(g),
                                    None => vis = false,
                                }
                            }
                            TextSource::RadialDetail(r) => {
                                rt.text.clear();
                                let focus = sr.focus.filter(|f| screen.nodes[*f as usize].parent == Some(*r));
                                if let Some(f) = focus {
                                    if let Kind::RadialItem { detail, .. } = &screen.nodes[f as usize].kind {
                                        let fs = scope_for(screen, &screen.nodes[f as usize], state, env).unwrap_or(Scope::new(state).with_env(env));
                                        detail.render(&fs, &mut rt.text);
                                    }
                                }
                            }
                        }
                        // A prompt with no glyph for this device hides its whole prompt widget.
                        if !vis {
                            if let Some(p) = node.parent.and_then(|cap| screen.nodes[cap as usize].parent) {
                                sr.comp[p as usize].visible = false;
                            }
                        }
                    }
                    Kind::Radial { .. } => {}
                    _ => {}
                }
            }
            sr.comp[i].visible = vis;
        }
        // Hidden prompt widgets hide their descendants too.
        for i in 0..n {
            if let Some(p) = screen.nodes[i].parent {
                if !sr.comp[p as usize].visible {
                    sr.comp[i].visible = false;
                }
            }
        }
        let root = if screen.full_bleed { view.full() } else { view.safe() };
        let ctx = Ctx { scale: view.scale, em: theme.em() };
        let mut m = Meas { nodes: &screen.nodes, rt: &mut sr.rt, theme, fonts, scale: view.scale };
        layout(screen, &mut sr.comp, root, &ctx, &mut m);
        // Keep focus on something focusable.
        if screen.input == InputMode::Menu {
            let ok = sr.focus.is_some_and(|f| focusable(sr, f as usize));
            if !ok {
                let before = sr.focus;
                sr.focus = screen.focus.filter(|&f| focusable(sr, f as usize)).or_else(|| (0..n).find(|&i| focusable(sr, i)).map(|i| i as u32));
                return sr.focus != before;
            }
        }
        false
    }

    fn collect_hits(&mut self) {
        self.hits.clear();
        for &s in &self.order {
            let sr = &self.screens[s];
            if sr.screen.input == InputMode::None {
                continue;
            }
            for (i, node) in sr.screen.nodes.iter().enumerate() {
                let c = &sr.comp[i];
                if !c.visible {
                    continue;
                }
                let blocks = match &node.kind {
                    Kind::Panel { boxed } => self.theme.boxed(*boxed).is_visible(),
                    Kind::Button { .. } | Kind::RadialItem { .. } | Kind::List { .. } | Kind::Bar { .. } => true,
                    Kind::Radial { .. } => true,
                    _ => false,
                };
                if !blocks {
                    continue;
                }
                let mut rect = match c.clip {
                    Some(cl) => c.rect.intersect(&cl),
                    None => c.rect,
                };
                if let Kind::RadialItem { .. } = node.kind {
                    // The whole wedge is the target: approximate with the item's square cell.
                    let parent = node.parent.map(|p| sr.comp[p as usize].rect).unwrap_or(rect);
                    let _ = parent;
                    rect = rect.grow(4.0 * self.last_view.map_or(1.0, |v| v.scale));
                }
                if rect.is_empty() {
                    continue;
                }
                self.hits.push(Hit { rect, w: WidgetRef { screen: s, node: i as u32 }, interactive: node.kind.interactive() });
            }
        }
    }

    /// Input → focus, hover, presses, events. Returns a bit mask of screens to lay out again.
    fn handle_input(&mut self, input: &Input, dt: f32) -> u64 {
        let mut relayout = 0u64;
        // Screens that take input: from the top down to (and including) the topmost modal.
        let top_modal = self.order.iter().rposition(|&s| self.screens[s].screen.modal);
        let lowest = top_modal.unwrap_or(0);
        self.events.consumed.all = top_modal.is_some();
        // The menu screen with keyboard/gamepad focus: the topmost visible Menu screen.
        let menu = self.order[lowest..].iter().rev().copied().find(|&s| self.screens[s].screen.input == InputMode::Menu);
        self.events.consumed.navigation = menu.is_some() || top_modal.is_some();

        // ---- agent requests ----
        let pending = std::mem::take(&mut self.pending);
        for &(w, activate) in &pending {
            if self.reachable(w).is_ok() {
                if self.screens[w.screen].screen.nodes[w.node as usize].kind.interactive() {
                    self.screens[w.screen].focus = Some(w.node);
                    relayout |= 1u64 << w.screen.min(63);
                }
                if activate {
                    self.activate_now(w);
                }
            }
        }
        self.pending = pending;
        self.pending.clear();

        // ---- pointer ----
        let pos = input.pointer().map(|p| [p.x, p.y]);
        let touch_down = !input.touches().is_empty();
        let mouse_down = input.source_held(Source::Mouse(MouseButton::Left));
        let down = touch_down || mouse_down;
        // A touch that went down and up within one frame shows only as the tap gesture.
        let quick_tap = !down && !self.pointer.down && input.source_pressed(Source::Tap);
        let pressed = (down && !self.pointer.down) || input.source_pressed(Source::Mouse(MouseButton::Left)) || quick_tap;
        let released = (!down && self.pointer.down) || quick_tap;
        // An OS-cancelled touch abandons the press: never an activation.
        // Only the contact that made the press can cancel it: a stray cancelled finger must not
        // drop a press made by the mouse or another finger.
        let cancelled = self.pointer.press_contact.is_some_and(|id| input.cancelled_contacts().contains(&id));
        // A finger that isn't touching doesn't hover: a touch pointer only counts while in
        // contact (and in the frame it lifts).
        let is_touch = input.pointer().is_some_and(|p| p.kind != specula_platform::PointerKind::Mouse && p.kind != specula_platform::PointerKind::Pen);
        let pos = if is_touch && !down && !released && !cancelled { None } else { pos };
        let moved = pos.is_some() && pos != self.pointer.pos;
        self.pointer.pos = pos;
        let in_scope = |order: &[usize], s: usize| order[lowest..].contains(&s);
        let hit_any = pos.and_then(|p| self.hits.iter().rev().find(|h| h.rect.contains(p) && in_scope(&self.order, h.w.screen)).copied());
        let hot = pos.and_then(|p| {
            self.hits.iter().rev().find(|h| h.rect.contains(p) && in_scope(&self.order, h.w.screen)).filter(|h| h.interactive).map(|h| h.w)
        });
        // Radial menus: the pointer picks by angle anywhere outside the dead zone, so that whole
        // area is the UI's (clicks there must not also reach the game).
        let mut hot = hot;
        let mut over_radial = false;
        if let Some(p) = pos {
            for &s in self.order[lowest..].iter().rev() {
                let sr = &self.screens[s];
                if sr.screen.input == InputMode::None {
                    continue;
                }
                if let Some((_, item)) = radial_under(sr, p) {
                    over_radial = true;
                    // The whole sector (inside and around the ring) is the item's target; the
                    // dead zone in the middle picks nothing. Focus follows only when the pointer
                    // moves, so a resting mouse doesn't fight the stick.
                    hot = item.map(|node| WidgetRef { screen: s, node });
                    break;
                }
            }
        }
        if moved && hot.is_some() {
            self.focus_visible = false;
        }
        if matches!(input.last_device(), Some(specula_input::Device::Gamepad(_))) {
            self.focus_visible = true;
        }
        let on_ui = hit_any.is_some() || over_radial;
        if pressed {
            self.pointer.press_contact = if touch_down { input.touches().first().map(|t| t.id) } else { None };
            self.pointer.press = hot;
            self.pointer.press_pos = pos.unwrap_or([0.0; 2]);
            self.pointer.captured = on_ui;
            self.pointer.dragging_list = None;
        }
        // Touch-drag scrolling of lists.
        if down && self.pointer.captured {
            if let (Some(p), Some(h)) = (pos, hit_any) {
                let _ = h;
                let dy = p[1] - self.pointer.press_pos[1];
                if self.pointer.dragging_list.is_none() && dy.abs() > 10.0 * self.draw.scale {
                    self.pointer.dragging_list = self.list_under(self.pointer.press_pos, lowest);
                    if self.pointer.dragging_list.is_some() {
                        self.pointer.press = None;
                    }
                }
            }
        }
        if let (Some(l), Some(p)) = (self.pointer.dragging_list, pos) {
            if moved {
                let prev = self.pointer.press_pos;
                if let Some(c) = self.screens.get_mut(l.screen).and_then(|s| s.comp.get_mut(l.node as usize)) {
                    c.scroll -= p[1] - prev[1];
                    relayout |= 1u64 << l.screen.min(63);
                }
                self.pointer.press_pos = p;
            }
        }
        if released || cancelled {
            if let (Some(pw), Some(h), false) = (self.pointer.press, hot, cancelled) {
                if pw == h {
                    self.activate_now(h);
                }
            }
            self.pointer.press = None;
            self.pointer.press_contact = None;
        }
        self.events.consumed.pointer = on_ui || (self.pointer.captured && (down || released || cancelled));
        if !down && !released {
            self.pointer.captured = false;
            self.pointer.dragging_list = None;
        }
        self.pointer.down = down;
        if self.pointer.hover != hot {
            self.pointer.hover = hot;
        }
        self.events.hovered = hot;
        // Hover moves menu focus (so mouse and pad agree on what "accept" presses).
        if let Some(h) = hot {
            if moved && self.screens[h.screen].focus != Some(h.node) {
                self.screens[h.screen].focus = Some(h.node);
                relayout |= 1u64 << h.screen.min(63);
            }
        }

        // ---- wheel scrolling ----
        let nav = read_nav(input);
        if nav.scroll != 0.0 {
            if let Some(p) = pos {
                if let Some(l) = self.list_under(p, lowest) {
                    let step = 40.0 * self.draw.scale;
                    self.screens[l.screen].comp[l.node as usize].scroll += nav.scroll * step;
                    relayout |= 1u64 << l.screen.min(63);
                }
            }
        }

        // ---- gamepad / keyboard ----
        if let Some(s) = menu {
            let mut moved_dir = None;
            for d in 0..4 {
                let (p, h) = nav.dirs[d];
                if p {
                    self.nav_held[d] = 0.0;
                    moved_dir = Some(Dir::ALL[d]);
                } else if h {
                    let before = self.nav_held[d];
                    self.nav_held[d] += dt;
                    // Repeat after 0.4 s, then every 0.12 s.
                    let first = 0.4;
                    let every = 0.12;
                    if self.nav_held[d] >= first {
                        let k0 = ((before - first) / every).floor();
                        let k1 = ((self.nav_held[d] - first) / every).floor();
                        if before < first || k1 > k0 {
                            moved_dir = Some(Dir::ALL[d]);
                        }
                    }
                } else {
                    self.nav_held[d] = 0.0;
                }
            }
            let has_radial = self.screens[s].screen.nodes.iter().any(|n| matches!(n.kind, Kind::Radial { .. }));
            if has_radial {
                // Stick picks by angle; left / right step around the ring.
                if let Some((items, count)) = radial_items(&self.screens[s]) {
                    let items = &items[..count];
                    let len = (nav.point[0] * nav.point[0] + nav.point[1] * nav.point[1]).sqrt();
                    if len > 0.5 {
                        if let Some(k) = radial_pick(nav.point, items.len(), 0.5) {
                            if self.screens[s].focus != Some(items[k]) {
                                self.screens[s].focus = Some(items[k]);
                                relayout |= 1u64 << s.min(63);
                            }
                            self.focus_visible = true;
                        }
                    } else if let Some(d) = moved_dir {
                        let cur = self.screens[s].focus.and_then(|f| items.iter().position(|&i| i == f)).unwrap_or(0);
                        let n = items.len();
                        let next = match d {
                            Dir::Right | Dir::Down => (cur + 1) % n,
                            Dir::Left | Dir::Up => (cur + n - 1) % n,
                        };
                        self.screens[s].focus = Some(items[next]);
                        relayout |= 1u64 << s.min(63);
                        self.focus_visible = true;
                    }
                }
            } else if let Some(d) = moved_dir {
                self.focus_visible = true;
                let sr = &self.screens[s];
                if let Some(f) = sr.focus {
                    let next = sr.screen.nodes[f as usize].nav[d.index()].filter(|&t| focusable(sr, t as usize)).or_else(|| {
                        let cands = (0..sr.screen.nodes.len()).filter(|&i| focusable(sr, i)).map(|i| (i as u32, sr.comp[i].rect));
                        neighbour(f, sr.comp[f as usize].rect, d, cands)
                    });
                    if let Some(t) = next {
                        self.screens[s].focus = Some(t);
                        self.scroll_into_view(s, t);
                        relayout |= 1u64 << s.min(63);
                    }
                }
            }
            if nav.accept {
                self.focus_visible = true;
                if let Some(f) = self.screens[s].focus {
                    self.activate_now(WidgetRef { screen: s, node: f });
                }
            }
            if nav.back {
                self.events.events.push(UiEvent::Back { screen: s });
                if let Some(a) = self.screens[s].screen.back.clone() {
                    self.events.events.push(UiEvent::Action { action: a, widget: WidgetRef { screen: s, node: 0 } });
                }
            }
            self.events.focused = self.screens[s].focus.map(|node| WidgetRef { screen: s, node });
        } else {
            self.events.focused = None;
        }
        relayout
    }

    fn list_under(&self, p: [f32; 2], lowest: usize) -> Option<WidgetRef> {
        for &s in self.order[lowest..].iter().rev() {
            let sr = &self.screens[s];
            if sr.screen.input == InputMode::None {
                continue;
            }
            for (i, n) in sr.screen.nodes.iter().enumerate().rev() {
                if let Kind::List { .. } = n.kind {
                    let c = &sr.comp[i];
                    if c.visible && c.rect.contains(p) && c.extent > c.content.h + 0.5 {
                        return Some(WidgetRef { screen: s, node: i as u32 });
                    }
                }
            }
        }
        None
    }

    fn scroll_into_view(&mut self, s: usize, node: u32) {
        let sr = &mut self.screens[s];
        let mut p = sr.screen.nodes[node as usize].parent;
        while let Some(l) = p {
            if let Kind::List { .. } = sr.screen.nodes[l as usize].kind {
                let content = sr.comp[l as usize].content;
                let r = sr.comp[node as usize].rect;
                let c = &mut sr.comp[l as usize];
                if r.y < content.y {
                    c.scroll -= content.y - r.y;
                } else if r.bottom() > content.bottom() {
                    c.scroll += r.bottom() - content.bottom();
                }
                return;
            }
            p = sr.screen.nodes[l as usize].parent;
        }
    }

    fn activate_now(&mut self, w: WidgetRef) {
        let Some(sr) = self.screens.get(w.screen) else { return };
        let Some(node) = sr.screen.nodes.get(w.node as usize) else { return };
        if !sr.visible || !sr.comp[w.node as usize].visible {
            return;
        }
        let action = match &node.kind {
            Kind::Button { action, .. } | Kind::RadialItem { action, .. } => action.clone(),
            _ => return,
        };
        if !sr.rt[w.node as usize].enabled {
            self.events.events.push(UiEvent::Refused { widget: w });
            return;
        }
        if let Some(action) = action {
            self.events.events.push(UiEvent::Action { action, widget: w });
        }
    }

    // ---- drawing ----

    fn render(&mut self, state: &StateMap, view: &Viewport) {
        let top_modal = self.order.iter().rposition(|&s| self.screens[s].screen.modal);
        let mut clip: Option<Rect> = None;
        for k in 0..self.order.len() {
            let s = self.order[k];
            if Some(k) == top_modal {
                if clip.is_some() {
                    clip = None;
                    self.draw.clip(None);
                }
                self.draw.rect(RectCmd {
                    rect: view.full(),
                    radius: 0.0,
                    fill: [self.theme.dim; 2],
                    border: 0.0,
                    border_color: [TRANSPARENT; 2],
                    feather: 1.0,
                });
            }
            let n = self.screens[s].screen.nodes.len();
            for i in 0..n {
                if !self.screens[s].comp[i].visible {
                    continue;
                }
                let want = self.screens[s].comp[i].clip;
                // Rows scrolled out of a list cost nothing to draw.
                if want.is_some_and(|c| c.intersect(&self.screens[s].comp[i].rect).is_empty()) {
                    continue;
                }
                if want != clip {
                    clip = want;
                    self.draw.clip(clip);
                }
                self.draw_node(s, i, state, view);
            }
        }
        if clip.is_some() {
            self.draw.clip(None);
        }
        self.draw_tooltip(view, state);
    }

    /// A text role's drawing parameters (no strings, so no allocation).
    fn role_look(&self, i: usize) -> RoleLook {
        let r = self.theme.role(i);
        RoleLook { color: r.color, shadow: r.shadow, size: r.size, font: self.fonts.index(&r.font) }
    }

    fn color_ref(&self, c: &ColorRef) -> Rgba {
        match c {
            ColorRef::Hex(c) => *c,
            ColorRef::Named(n) => self.theme.color(n).unwrap_or([1.0, 0.0, 1.0, 1.0]),
        }
    }

    fn draw_box(&mut self, b: &BoxStyle, r: Rect, scale: f32) {
        draw_box(&mut self.draw, b, r, scale);
    }

    fn button_look(&self, w: WidgetRef, style: usize) -> ButtonLook {
        let bs = self.theme.button(style);
        let sr = &self.screens[w.screen];
        if !sr.rt[w.node as usize].enabled {
            return bs.disabled.clone();
        }
        let pressed = self.pointer.press == Some(w) && self.pointer.hover == Some(w) && self.pointer.down;
        if pressed {
            return bs.pressed.clone();
        }
        let focused = sr.screen.input == InputMode::Menu && sr.focus == Some(w.node) && self.focus_visible;
        if focused {
            return bs.focused.clone();
        }
        if self.pointer.hover == Some(w) {
            return bs.hover.clone();
        }
        bs.normal.clone()
    }

    fn text_color_for(&self, s: usize, i: usize) -> Option<Rgba> {
        let sr = &self.screens[s];
        let parent = sr.screen.nodes[i].parent? as usize;
        match &sr.screen.nodes[parent].kind {
            Kind::Button { style, .. } => Some(self.button_look(WidgetRef { screen: s, node: parent as u32 }, *style).text),
            Kind::RadialItem { .. } => Some(if sr.rt[parent].enabled { self.theme.radial.text } else { self.theme.radial.text_disabled }),
            _ => None,
        }
    }

    fn draw_node(&mut self, s: usize, i: usize, state: &StateMap, view: &Viewport) {
        let scale = view.scale;
        let rect = self.screens[s].comp[i].rect;
        let content = self.screens[s].comp[i].content;
        let w = WidgetRef { screen: s, node: i as u32 };
        let kind = self.screens[s].screen.nodes[i].kind.clone_shallow();
        match kind {
            Shallow::Panel(boxed) => {
                let b = self.theme.boxed(boxed).clone();
                self.draw_box(&b, rect, scale);
            }
            Shallow::Button(style) => {
                let look = self.button_look(w, style);
                let b = self.theme.boxed(look.boxed).clone();
                self.draw_box(&b, rect, scale);
                self.focus_ring(s, i, rect, b.radius * scale, view);
            }
            Shallow::Label => self.draw_label(s, i, content, scale),
            Shallow::Icon => {
                let (icon, c, rotation) = match &self.screens[s].screen.nodes[i].kind {
                    Kind::Icon { icon, color, rotation } => {
                        let c = match color {
                            Some(c) => self.color_ref(c),
                            None => self.text_color_for(s, i).unwrap_or(self.theme.role(self.theme.role_index("body").unwrap_or(0)).color),
                        };
                        (*icon, c, *rotation)
                    }
                    _ => return,
                };
                match icon {
                    IconSource::Shape(shape) => {
                        let start = self.draw.begin_mesh();
                        shapes::draw_shape(&mut self.draw.vertices, &mut self.scratch, shape, content, rotation.to_radians(), c);
                        self.draw.end_mesh(start);
                    }
                    IconSource::Image { texture, uv } => {
                        let owner = crate::draw::Owner { screen: s as u32, node: i as u32 };
                        self.draw.cmds.push(crate::draw::DrawCmd::Image { rect: content, uv, texture, color: c, owner });
                    }
                }
            }
            Shallow::Bar => {
                let (frac, fill_c) = {
                    let node = &self.screens[s].screen.nodes[i];
                    let Kind::Bar { value, max, color } = &node.kind else { return };
                    let scope = scope_for(&self.screens[s].screen, node, state, &self.env).unwrap_or(Scope::new(state).with_env(&self.env));
                    let get = |n: &Num| match n {
                        Num::Const(v) => f64::from(*v),
                        Num::Key(k) => scope.num(k).unwrap_or(0.0),
                    };
                    let (v, m) = (get(value), get(max));
                    let frac = if m > 0.0 { (v / m).clamp(0.0, 1.0) as f32 } else { 0.0 };
                    let fill_c = match color {
                        Some(c) => self.color_ref(c),
                        None if frac <= self.theme.bar.low_below => self.theme.bar.low,
                        None => self.theme.bar.fill,
                    };
                    (frac, fill_c)
                };
                let track = self.theme.boxed(self.theme.bar.track).clone();
                self.draw_box(&track, rect, scale);
                let inner = content;
                if frac > 0.0 && inner.w > 0.0 {
                    let fw = (inner.w * frac).max(inner.h.min(inner.w));
                    let r = (track.radius * scale - track.border * scale).max(0.0).min(inner.h * 0.5);
                    self.draw.rect(RectCmd {
                        rect: Rect::new(inner.x, inner.y, fw, inner.h),
                        radius: r,
                        fill: [mix(fill_c, [1.0, 1.0, 1.0, fill_c[3]], 0.18), fill_c],
                        border: 0.0,
                        border_color: [TRANSPARENT; 2],
                        feather: 1.0,
                    });
                }
            }
            Shallow::Radial(radius, inner) => self.draw_radial(s, i, rect, radius * scale, inner * scale, scale),
            Shallow::Threats => self.draw_threats(s, i, rect, state, scale),
            Shallow::Toasts => self.draw_toasts(rect, scale, self.screens[s].screen.nodes[i].anchor.0[1] > 0.5),
            Shallow::List => {
                let c = &self.screens[s].comp[i];
                let (extent, content, scroll) = (c.extent, c.content, c.scroll);
                if extent > content.h + 0.5 && content.h > 0.0 {
                    // A scroll bar at the right edge of the list (the list's right padding is its gutter).
                    let ink = self.theme.role(self.theme.role_index("body").unwrap_or(0)).color;
                    let w = 4.0 * scale;
                    let track = Rect::new(rect.right() - w, content.y, w, content.h);
                    let thumb_h = (content.h * content.h / extent).max(16.0 * scale).min(content.h);
                    let t = scroll / (extent - content.h);
                    let thumb = Rect::new(track.x, track.y + (track.h - thumb_h) * t, w, thumb_h);
                    for (r, a) in [(track, 0.12), (thumb, 0.55)] {
                        self.draw.rect(RectCmd { rect: r, radius: w * 0.5, fill: [with_alpha(ink, a); 2], border: 0.0, border_color: [TRANSPARENT; 2], feather: 1.0 });
                    }
                }
            }
            Shallow::None => {}
            Shallow::RadialItem => {}
        }
    }

    fn focus_ring(&mut self, s: usize, i: usize, rect: Rect, radius: f32, view: &Viewport) {
        let sr = &self.screens[s];
        if !(self.focus_visible && sr.screen.input == InputMode::Menu && sr.focus == Some(i as u32)) {
            return;
        }
        if matches!(sr.screen.nodes[i].kind, Kind::RadialItem { .. }) {
            return;
        }
        let f = self.theme.focus;
        let g = (f.gap + f.width) * view.scale;
        self.draw.rect(RectCmd {
            rect: rect.grow(g),
            radius: radius + g,
            fill: [TRANSPARENT; 2],
            border: f.width * view.scale,
            border_color: [f.color; 2],
            feather: 1.0,
        });
    }

    fn draw_label(&mut self, s: usize, i: usize, content: Rect, scale: f32) {
        let (role, c, align, wrap) = match &self.screens[s].screen.nodes[i].kind {
            Kind::Label { role, color, align, wrap, .. } => {
                let role = self.role_look(*role);
                let c = match color {
                    Some(c) => self.color_ref(c),
                    None => self.text_color_for(s, i).unwrap_or(role.color),
                };
                (role, c, *align, *wrap)
            }
            _ => return,
        };
        let Some(font_i) = self.screens[s].rt[i].font else { return };
        let tsize = self.screens[s].comp[i].text;
        let fx = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => 0.5,
            TextAlign::Right => 1.0,
        };
        let x = if wrap { content.x } else { content.x + (content.w - tsize[0]) * fx };
        let y = content.y + ((content.h - tsize[1]) * 0.5).max(0.0);
        let font = self.fonts.font(font_i);
        let laid = &self.screens[s].rt[i].laid;
        let start = self.draw.glyphs.len() as u32;
        if let Some(sh) = role.shadow {
            let _ = laid.append_quads(font, &mut self.atlas, [x + scale.round().max(1.0), y + scale.round().max(1.0)], sh, &mut self.draw.glyphs);
        }
        let _ = laid.append_quads(font, &mut self.atlas, [x, y], c, &mut self.draw.glyphs);
        self.draw.glyph_run(start);
    }

    fn draw_radial(&mut self, s: usize, i: usize, rect: Rect, radius: f32, inner: f32, scale: f32) {
        let centre = rect.center();
        let (children, count) = radial_children(&self.screens[s], i as u32);
        if count == 0 {
            return;
        }
        let st = self.theme.radial.clone();
        let sector = std::f32::consts::TAU / count as f32;
        let gap_a = (st.gap * scale) / radius.max(1.0);
        // Soft shadow disc under the ring.
        self.draw.rect(RectCmd {
            rect: Rect::new(centre[0] - radius, centre[1] - radius + 3.0 * scale, radius * 2.0, radius * 2.0).grow(2.0 * scale),
            radius: radius + 2.0 * scale,
            fill: [[0.0, 0.0, 0.0, 0.45]; 2],
            border: 0.0,
            border_color: [TRANSPARENT; 2],
            feather: 10.0 * scale,
        });
        // An opaque base so the world never shows through the gaps between wedges and the hub.
        self.draw.rect(RectCmd {
            rect: Rect::new(centre[0] - radius, centre[1] - radius, radius * 2.0, radius * 2.0),
            radius,
            fill: [st.base; 2],
            border: 0.0,
            border_color: [TRANSPARENT; 2],
            feather: 1.0,
        });
        let start = self.draw.begin_mesh();
        for (k, &c) in children.iter().take(count).enumerate() {
            let sr = &self.screens[s];
            let a = crate::layout::radial_angle(k, count);
            let hot = sr.focus == Some(c);
            let enabled = sr.rt[c as usize].enabled;
            let fill = if !enabled {
                st.wedge_disabled
            } else if hot {
                st.wedge_hot
            } else {
                st.wedge
            };
            let a0 = a - sector * 0.5 + gap_a * 0.5;
            let a1 = a + sector * 0.5 - gap_a * 0.5;
            shapes::band(&mut self.draw.vertices, &mut self.scratch, centre, inner, radius, a0, a1, fill);
            let edge = if hot && enabled { st.edge_hot } else { st.edge };
            let ew = if hot { 3.0 } else { 1.5 } * scale;
            shapes::band(&mut self.draw.vertices, &mut self.scratch, centre, radius - ew, radius, a0, a1, edge);
            if hot {
                shapes::band(&mut self.draw.vertices, &mut self.scratch, centre, inner, inner + 2.0 * scale, a0, a1, edge);
            }
        }
        self.draw.end_mesh(start);
        // The centre disc (a box style drawn as a circle).
        let mut b = self.theme.boxed(st.centre).clone();
        let d = (inner - 4.0 * scale).max(0.0);
        b.radius = d / scale;
        self.draw_box(&b, Rect::new(centre[0] - d, centre[1] - d, d * 2.0, d * 2.0), scale);
    }

    fn draw_threats(&mut self, s: usize, i: usize, bounds: Rect, state: &StateMap, scale: f32) {
        let list = match &self.screens[s].screen.nodes[i].kind {
            Kind::Threats { source: crate::state::Key::Global(key) } => state.list(key),
            _ => None,
        };
        let Some(list) = list else { return };
        let t = self.theme.threat.clone();
        let size = t.size * scale;
        let margin = t.inset * scale + size * 0.5;
        let phase = (self.time * f64::from(t.pulse_hz)).fract() as f32;
        let pulse = 0.5 + 0.5 * specula_core::dmath::cos(phase * std::f32::consts::TAU);
        while self.threat_text.len() < list.len() {
            self.threat_text.push((String::new(), TextLayout::default(), 0.0));
        }
        for (k, row) in list.rows().iter().enumerate() {
            let num = |name: &str| match row.get(name) {
                Some(Value::Num(v)) => *v as f32,
                Some(Value::Int(v)) => *v as f32,
                _ => f32::NAN,
            };
            let p = [num("x"), num("y")];
            let behind = row.get("behind").is_some_and(Value::truthy);
            let Some((pos, angle)) = threat_arrow(bounds, margin, p, behind) else { continue };
            // Keep clear of HUD panels and buttons (other screens' and this screen's widgets).
            let inner = bounds.inset(margin, margin, margin, margin);
            let mut obstacles = [Rect::default(); 32];
            let mut n_obs = 0;
            for hit in &self.hits {
                if n_obs < obstacles.len() && !matches!(self.screens[hit.w.screen].screen.nodes[hit.w.node as usize].kind, Kind::Radial { .. } | Kind::RadialItem { .. }) {
                    obstacles[n_obs] = hit.rect;
                    n_obs += 1;
                }
            }
            let pos = crate::geom::avoid_on_edge(pos, [size * 0.62, size * 0.62], inner, 4.0 * scale, &obstacles[..n_obs]);
            let grow = 1.0 + 0.12 * pulse;
            let alpha = 0.7 + 0.3 * pulse;
            let r = Rect::new(pos[0] - size * 0.5 * grow, pos[1] - size * 0.5 * grow, size * grow, size * grow);
            // Dark halo for contrast over bright scenery, then the arrow.
            let start = self.draw.begin_mesh();
            shapes::draw_shape(&mut self.draw.vertices, &mut self.scratch, crate::screen::Shape::Circle, r.grow(size * 0.12), 0.0, t.halo);
            shapes::draw_shape(&mut self.draw.vertices, &mut self.scratch, crate::screen::Shape::Arrow, r, angle, with_alpha(t.color, alpha));
            self.draw.end_mesh(start);
            // Optional label inside the arrow (towards the screen centre).
            if let Some(Value::Str(label)) = row.get("label") {
                let role = self.role_look(t.label);
                let Some(fi) = role.font else { continue };
                let font = self.fonts.font(fi);
                let (text, laid, laid_scale) = &mut self.threat_text[k];
                if text != label || laid.glyphs.is_empty() || *laid_scale != scale {
                    text.clear();
                    text.push_str(label);
                    layout_into(font, text, &TextStyle::new(role.size).with_scale(scale), laid);
                    *laid_scale = scale;
                }
                let tw = laid.lines.iter().map(|l| l.width).fold(0.0f32, f32::max);
                let th = laid.height;
                let (sa, ca) = specula_core::dmath::sin_cos(angle);
                let back = [-ca, -sa];
                let pad_l = self.theme.boxed(t.label_box).padding;
                let (bw, bh) = (tw + (pad_l[1] + pad_l[3]) * scale, th + (pad_l[0] + pad_l[2]) * scale);
                // Clear of the arrow's halo (size × 0.62) plus a gap, along the way back in.
                let reach = size * 0.62 * grow + 6.0 * scale + (bw * back[0].abs() + bh * back[1].abs()) * 0.5;
                let c = [pos[0] + back[0] * reach, pos[1] + back[1] * reach];
                let lb = self.theme.boxed(t.label_box).clone();
                let pad = lb.padding.map(|v| v * scale);
                let br = Rect::new(c[0] - tw * 0.5 - pad[3], c[1] - th * 0.5 - pad[0], tw + pad[1] + pad[3], th + pad[0] + pad[2]);
                draw_box(&mut self.draw, &lb, br, scale);
                let start = self.draw.glyphs.len() as u32;
                let laid = &self.threat_text[k].1;
                let _ = laid.append_quads(font, &mut self.atlas, [br.x + pad[3], br.y + pad[0]], t.color, &mut self.draw.glyphs);
                self.draw.glyph_run(start);
            }
        }
    }

    fn draw_toasts(&mut self, rect: Rect, scale: f32, upward: bool) {
        let st = self.theme.toast.clone();
        let role = self.role_look(st.role);
        let Some(fi) = role.font else { return };
        let b = self.theme.boxed(st.boxed).clone();
        let pad = b.padding.map(|v| v * scale);
        let gap = 6.0 * scale;
        // Newest first nearest the anchor.
        let mut idx: [usize; 8] = [usize::MAX; 8];
        let mut n = 0;
        for (k, t) in self.toasts.iter().enumerate() {
            if t.kind.is_some() && n < 8 {
                idx[n] = k;
                n += 1;
            }
        }
        idx[..n].sort_by(|&a, &b| self.toasts[a].age.total_cmp(&self.toasts[b].age));
        let mut y = if upward { rect.bottom() } else { rect.y };
        for &k in &idx[..n] {
            let font = self.fonts.font(fi);
            let t = &mut self.toasts[k];
            let max_w = (rect.w - pad[1] - pad[3] - 6.0 * scale).max(10.0);
            let mut style = TextStyle::new(role.size).with_scale(scale).with_max_width(max_w / scale);
            style.align = specula_text::Align::Center;
            layout_into(font, &t.text, &style, &mut t.laid);
            let th = t.laid.height;
            let fade = st.fade_s.max(1e-3);
            let a = if t.age < 0.15 { t.age / 0.15 } else { (1.0 - (t.age - st.duration_s) / fade).clamp(0.0, 1.0) };
            let h = th + pad[0] + pad[2];
            let r = if upward {
                y -= h;
                Rect::new(rect.x, y, rect.w, h)
            } else {
                Rect::new(rect.x, y, rect.w, h)
            };
            if upward {
                y -= gap;
            } else {
                y += h + gap;
            }
            let accent = match t.kind {
                Some(ToastKind::Good) => st.good,
                Some(ToastKind::Warn) => st.warn,
                Some(ToastKind::Threat) => st.threat,
                _ => st.info,
            };
            let mut bb = b.clone();
            for c in bb.fill.iter_mut().chain(bb.border_color.iter_mut()) {
                c[3] *= a;
            }
            if let Some(sh) = &mut bb.shadow {
                sh.color[3] *= a;
            }
            if let Some(bv) = &mut bb.bevel {
                bv.light[3] *= a;
                bv.dark[3] *= a;
            }
            draw_box(&mut self.draw, &bb, r, scale);
            // Accent stripe on the left.
            self.draw.rect(RectCmd {
                rect: Rect::new(r.x + 3.0 * scale, r.y + 4.0 * scale, 3.0 * scale, (r.h - 8.0 * scale).max(0.0)),
                radius: 1.5 * scale,
                fill: [with_alpha(accent, a); 2],
                border: 0.0,
                border_color: [TRANSPARENT; 2],
                feather: 1.0,
            });
            let start = self.draw.glyphs.len() as u32;
            let tx = r.x + pad[3] + 6.0 * scale;
            let laid = &self.toasts[k].laid;
            let font = self.fonts.font(fi);
            let _ = laid.append_quads(font, &mut self.atlas, [tx, r.y + pad[0]], with_alpha(role.color, a), &mut self.draw.glyphs);
            self.draw.glyph_run(start);
        }
    }

    fn draw_tooltip(&mut self, view: &Viewport, state: &StateMap) {
        let target = self.pointer.hover.or(if self.focus_visible { self.events.focused } else { None });
        let target = target.filter(|w| self.screens[w.screen].screen.nodes[w.node as usize].tooltip.is_some());
        if target != self.tip {
            self.tip = target;
            self.tip_since = self.time;
        }
        let Some(w) = target else { return };
        if self.time - self.tip_since < f64::from(self.theme.tooltip.delay_s) {
            return;
        }
        let st = self.theme.tooltip.clone();
        let role = self.role_look(st.role);
        let Some(fi) = role.font else { return };
        let scale = view.scale;
        let sr = &self.screens[w.screen];
        let node = &sr.screen.nodes[w.node as usize];
        let Some(tpl) = &node.tooltip else { return };
        let scope = scope_for(&sr.screen, node, state, &self.env).unwrap_or(Scope::new(state).with_env(&self.env));
        tpl.render(&scope, &mut self.tip_text);
        let font = self.fonts.font(fi);
        let style = TextStyle::new(role.size).with_scale(scale).with_max_width(st.max_width);
        layout_into(font, &self.tip_text, &style, &mut self.tip_laid);
        let tw = self.tip_laid.lines.iter().map(|l| l.width).fold(0.0f32, f32::max);
        let th = self.tip_laid.height;
        let b = self.theme.boxed(st.boxed).clone();
        let pad = b.padding.map(|v| v * scale);
        let (bw, bh) = (tw + pad[1] + pad[3], th + pad[0] + pad[2]);
        let target_r = sr.comp[w.node as usize].rect;
        let safe = view.safe();
        let gap = 6.0 * scale;
        let mut y = target_r.bottom() + gap;
        if y + bh > safe.bottom() {
            y = target_r.y - gap - bh;
        }
        let x = (target_r.center()[0] - bw * 0.5).clamp(safe.x, (safe.right() - bw).max(safe.x));
        let y = y.clamp(safe.y, (safe.bottom() - bh).max(safe.y));
        let r = Rect::new(x, y, bw, bh);
        draw_box(&mut self.draw, &b, r, scale);
        let start = self.draw.glyphs.len() as u32;
        let _ = self.tip_laid.append_quads(font, &mut self.atlas, [r.x + pad[3], r.y + pad[0]], role.color, &mut self.draw.glyphs);
        self.draw.glyph_run(start);
    }

    /// Everything a `ui_check` run disturbs, so it can be put back: interaction state, the last
    /// layout, draw list, hit map and events.
    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            pointer: self.pointer,
            focus_visible: self.focus_visible,
            nav_held: self.nav_held,
            tip: self.tip,
            tip_since: self.tip_since,
            time: self.time,
            last_view: self.last_view,
            screens: self.screens.iter().map(|s| (s.comp.clone(), s.rt.iter().map(|r| r.text.clone()).collect(), s.focus)).collect(),
            draw: self.draw.clone(),
            hits: self.hits.clone(),
            events: self.events.clone(),
            prompt_key: self.prompt_key,
            prompts: self.prompts.clone(),
        }
    }

    pub(crate) fn restore(&mut self, s: Snapshot) {
        self.pointer = s.pointer;
        self.focus_visible = s.focus_visible;
        self.nav_held = s.nav_held;
        self.tip = s.tip;
        self.tip_since = s.tip_since;
        self.time = s.time;
        self.last_view = s.last_view;
        for (sr, (comp, texts, focus)) in self.screens.iter_mut().zip(s.screens) {
            if sr.comp.len() == comp.len() {
                sr.comp = comp;
                for (r, t) in sr.rt.iter_mut().zip(texts) {
                    r.text = t;
                }
            }
            sr.focus = focus;
        }
        self.draw = s.draw;
        self.hits = s.hits;
        self.events = s.events;
        self.prompt_key = s.prompt_key;
        self.prompts = s.prompts;
    }

    /// The UI tree as JSON (`ui_tree`): every visible widget with id, type, rect, text and state.
    pub fn tree(&self) -> Json {
        let mut screens = Vec::new();
        for &s in &self.order {
            let sr = &self.screens[s];
            let mut widgets = Vec::new();
            for (i, n) in sr.screen.nodes.iter().enumerate() {
                let c = &sr.comp[i];
                if !c.visible {
                    continue;
                }
                let mut pairs = vec![
                    ("type", Json::Str(n.kind.name().into())),
                    ("rect", Json::arr(c.rect.to_array().map(|v| Json::Num(f64::from((v * 10.0).round() / 10.0))))),
                ];
                if let Some(id) = &n.id {
                    pairs.insert(0, ("id", Json::Str(id.clone())));
                }
                if let Kind::Label { .. } = n.kind {
                    pairs.push(("text", Json::Str(sr.rt[i].text.clone())));
                }
                if n.kind.interactive() {
                    pairs.push(("enabled", Json::Bool(sr.rt[i].enabled)));
                    pairs.push(("focused", Json::Bool(sr.focus == Some(i as u32))));
                }
                if let Some(cl) = c.clip {
                    if !c.rect.inside(&cl) {
                        pairs.push(("clipped", Json::Bool(true)));
                    }
                }
                pairs.push(("path", Json::Str(n.path.clone())));
                widgets.push(Json::obj(pairs));
            }
            screens.push(Json::obj([
                ("name", Json::Str(sr.screen.name.clone())),
                ("layer", Json::Num(f64::from(sr.screen.layer))),
                ("widgets", Json::Arr(widgets)),
            ]));
        }
        Json::obj([("screens", Json::Arr(screens))])
    }
}

/// See [`Ui::snapshot`].
pub(crate) struct Snapshot {
    pointer: PointerSt,
    focus_visible: bool,
    nav_held: [f32; 4],
    tip: Option<WidgetRef>,
    tip_since: f64,
    time: f64,
    last_view: Option<Viewport>,
    screens: Vec<(Vec<Computed>, Vec<String>, Option<u32>)>,
    draw: DrawList,
    hits: Vec<Hit>,
    events: UiEvents,
    prompt_key: Option<(Option<DeviceFamily>, PadStyle)>,
    prompts: Vec<(String, Option<String>)>,
}

/// Draw a box style into `r` (physical px).
pub(crate) fn draw_box(d: &mut DrawList, b: &BoxStyle, r: Rect, scale: f32) {
    let radius = (b.radius * scale).min(r.w.min(r.h) * 0.5);
    if let Some(sh) = b.shadow {
        let blur = (sh.blur * scale).max(1.0);
        d.rect(RectCmd {
            rect: Rect::new(r.x + sh.offset[0] * scale, r.y + sh.offset[1] * scale, r.w, r.h),
            radius,
            fill: [sh.color; 2],
            border: 0.0,
            border_color: [TRANSPARENT; 2],
            feather: blur,
        });
    }
    d.rect(RectCmd { rect: r, radius, fill: b.fill, border: b.border * scale, border_color: b.border_color, feather: 1.0 });
    if let Some(bv) = b.bevel {
        let inset = b.border * scale;
        let ir = r.inset(inset, inset, inset, inset);
        d.rect(RectCmd {
            rect: ir,
            radius: (radius - inset).max(0.0),
            fill: [TRANSPARENT; 2],
            border: bv.width * scale,
            border_color: [bv.light, bv.dark],
            feather: 1.0,
        });
    }
}

/// The scope a node's bindings read: its list row inside a repeat, or the plain state. `None`
/// when the node belongs to a repeat row that doesn't exist this frame.
pub(crate) fn scope_for<'a>(screen: &Screen, node: &Node, state: &'a StateMap, env: &'a StateMap) -> Option<Scope<'a>> {
    match node.item {
        None => Some(Scope::new(state).with_env(env)),
        Some((list, row)) => {
            let Kind::List { repeat: Some(rep) } = &screen.nodes[list as usize].kind else { return Some(Scope::new(state).with_env(env)) };
            let crate::state::Key::Global(k) = &rep.source else { return None };
            let r = state.list(k)?.row(row)?;
            Some(Scope { state, item: Some((row, r)), env: Some(env) })
        }
    }
}

pub(crate) fn focusable(sr: &ScreenRt, i: usize) -> bool {
    sr.screen.nodes[i].kind.interactive() && sr.comp[i].visible && sr.rt[i].enabled && !sr.comp[i].rect.is_empty()
}

/// Visible items of radial node `r` (at most 12), in ring order.
fn radial_children(sr: &ScreenRt, r: u32) -> ([u32; 12], usize) {
    let mut a = [u32::MAX; 12];
    let mut n = 0;
    for c in sr.screen.children(r) {
        if n < 12 && sr.comp[c as usize].visible && matches!(sr.screen.nodes[c as usize].kind, Kind::RadialItem { .. }) {
            a[n] = c;
            n += 1;
        }
    }
    (a, n)
}

fn radial_items(sr: &ScreenRt) -> Option<([u32; 12], usize)> {
    let r = sr.screen.nodes.iter().position(|n| matches!(n.kind, Kind::Radial { .. }))? as u32;
    if !sr.comp[r as usize].visible {
        return None;
    }
    let (items, n) = radial_children(sr, r);
    (n > 0).then_some((items, n))
}

/// If `p` is over a visible radial menu: (radial node, picked item).
fn radial_under(sr: &ScreenRt, p: [f32; 2]) -> Option<(u32, Option<u32>)> {
    for (i, n) in sr.screen.nodes.iter().enumerate() {
        let Kind::Radial { inner, .. } = n.kind else { continue };
        let c = &sr.comp[i];
        if !c.visible {
            continue;
        }
        let centre = c.rect.center();
        let v = [p[0] - centre[0], p[1] - centre[1]];
        let dist = (v[0] * v[0] + v[1] * v[1]).sqrt();
        let radius = c.rect.w * 0.5;
        if dist > radius * 1.6 {
            continue;
        }
        let scale = c.rect.w / (2.0 * match n.kind {
            Kind::Radial { radius, .. } => radius,
            _ => 1.0,
        });
        let (items, count) = radial_children(sr, i as u32);
        let pick = radial_pick(v, count, inner * scale * 0.5).map(|k| items[k]);
        return Some((i as u32, pick));
    }
    None
}

/// Text measuring with per-node layout caches.
struct Meas<'a> {
    nodes: &'a [Node],
    rt: &'a mut [NodeRt],
    theme: &'a Theme,
    fonts: &'a FontSet,
    scale: f32,
}

impl Measure for Meas<'_> {
    fn text(&mut self, node: u32, wrap: Option<f32>) -> [f32; 2] {
        let i = node as usize;
        let Kind::Label { role, align, .. } = &self.nodes[i].kind else { return [0.0, 0.0] };
        let role = self.theme.role(*role);
        let Some(fi) = self.fonts.index(&role.font) else { return [0.0, role.size * self.scale * 1.2] };
        let rt = &mut self.rt[i];
        let wrap_key = wrap.map_or(-1.0, |w| w);
        let size = role.size * self.scale;
        if !rt.laid_ok || rt.laid_text != rt.text || rt.laid_wrap != wrap_key || rt.laid_size != size || rt.laid_font != fi {
            let mut style = TextStyle::new(role.size).with_scale(self.scale);
            style.line_height = role.line_height;
            style.align = *align;
            if let Some(w) = wrap {
                style.max_width = Some((w / self.scale).max(1.0));
            }
            layout_into(self.fonts.font(fi), &rt.text, &style, &mut rt.laid);
            rt.laid_text.clear();
            rt.laid_text.push_str(&rt.text);
            rt.laid_wrap = wrap_key;
            rt.laid_size = size;
            rt.laid_font = fi;
            rt.laid_ok = true;
        }
        let w = rt.laid.lines.iter().map(|l| l.width).fold(0.0f32, f32::max);
        [w.ceil(), rt.laid.height.ceil()]
    }
}

#[derive(Clone, Copy)]
struct RoleLook {
    color: Rgba,
    shadow: Option<Rgba>,
    size: f32,
    font: Option<usize>,
}

/// A shallow copy of a node kind's variant (avoids cloning templates while drawing).
enum Shallow {
    Panel(usize),
    Button(usize),
    Label,
    Icon,
    Bar,
    Radial(f32, f32),
    RadialItem,
    Threats,
    Toasts,
    List,
    None,
}

impl Kind {
    fn clone_shallow(&self) -> Shallow {
        match self {
            Kind::Panel { boxed } => Shallow::Panel(*boxed),
            Kind::Button { style, .. } => Shallow::Button(*style),
            Kind::Label { .. } => Shallow::Label,
            Kind::Icon { .. } => Shallow::Icon,
            Kind::Bar { .. } => Shallow::Bar,
            Kind::Radial { radius, inner } => Shallow::Radial(*radius, *inner),
            Kind::RadialItem { .. } => Shallow::RadialItem,
            Kind::Threats { .. } => Shallow::Threats,
            Kind::Toasts { .. } => Shallow::Toasts,
            Kind::List { .. } => Shallow::List,
            Kind::Box | Kind::Prompt { .. } | Kind::Spacer => Shallow::None,
        }
    }
}

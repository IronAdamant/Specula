//! The editor's "Ask the agents…" box (docs/design/EDITOR.md, "Highlight and ask"): the human
//! highlights something, types a request, and Send posts a board task (`task_post` as
//! `window:owner`, a protocol command like every editor edit) carrying the request, the selection
//! by identity and part descriptors ([`crate::ask::Part`]).
//!
//! **The box is the human's alone** (TEXT.md, "The window's keyboard"):
//! - Only the human's own window input opens it, types into it or presses its buttons:
//!   `editor_input`'s injected frames are refused while it is open (`window_held`) and can't open
//!   it; `ui_act` never reaches it (it is not a UI-tools surface).
//! - **No agent reads it while the human types.** The words live only in this module's private
//!   fields and its own `specula-ui` instance (never the editor screen's state, so `ui_tree` and
//!   `editor_state` can't see them; `editor_state` says only that it is open and what it is
//!   about). They leave only on Send, as the task the human sent.
//! - **Agents can't create or move it:** it opens from the human's press, beside where the
//!   selection was on screen then (above or below it), and stays there; a leader line points at
//!   the selection.
//! - **A screen taking the window closes it** (the review or gallery screen, opened by a lead or
//!   the owner while the human types; others are refused, `window_typing`): what was typed is
//!   dropped, never sent, and the window's title says why.
//! - The window's title names it while it is open (the cue world content can't fake).

use crate::ask::{Part, PartKind, MAX_REQUEST};
use crate::entity_ref::EntityRef;
use specula_core::Json;
use specula_gpu::{OverlayQuad, OverlayTextures};
use specula_input::{ActionMap, Input};
use specula_platform::Event;
use specula_ui::gpu::GpuOverlay;
use specula_ui::{StateMap, Theme, Ui, Viewport};

const SCREEN: &str = include_str!("../ui/editor_ask.json");
pub const SCREEN_NAME: &str = "editor_ask";
/// The most entities one ask names (the board's link cap).
pub const MAX_ENTITIES: usize = crate::board::MAX_LINKS;
/// Words shown in the box (the rest scrolls off the front).
const SHOWN_CHARS: usize = 400;

const VK_BACK: u32 = 0x08;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_SHIFT: u32 = 0x10;
const VK_TAB: u32 = 0x09;

/// What a request can be about, beside the whole selection.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    Whole,
    /// A named part of the primary's model recipe.
    Recipe(String),
    /// All of a text entity's words (bytes `[0, len)`).
    Words(u32),
    /// The ground around the primary (a terrain or grass patch).
    Area { center: [f32; 3], radius: f32 },
    /// The spot the human's brush last touched on the primary (the draw tools): a place they
    /// pointed at, not one worked out from the entity's bounds.
    Spot { center: [f32; 3], radius: f32 },
}

impl Choice {
    pub fn describe(&self) -> String {
        match self {
            Choice::Whole => "the whole selection".into(),
            Choice::Recipe(n) => format!("its part \"{n}\""),
            Choice::Words(_) => "its words".into(),
            Choice::Area { radius, .. } => format!("the ground around it ({radius:.1} m)"),
            Choice::Spot { radius, .. } => format!("the spot under your brush ({:.1} m wide)", radius * 2.0),
        }
    }

    /// The part descriptor (the primary is entity `primary` of the task).
    pub fn part(&self, primary: usize) -> Option<Part> {
        let kind = match self {
            Choice::Whole => return None,
            Choice::Recipe(n) => PartKind::RecipePart { name: n.clone() },
            Choice::Words(len) => PartKind::TextSpan { start: 0, end: *len },
            Choice::Area { center, radius } | Choice::Spot { center, radius } => return Some(Part { entity: None, kind: PartKind::Area { center: *center, radius: *radius } }),
        };
        Some(Part { entity: Some(primary), kind })
    }
}

/// What the box asks for this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum AskOut {
    /// The human sent it: the words, the selection (identities, the primary last) and the part.
    Send { text: String, entities: Vec<EntityRef>, choice: Choice },
    Cancel,
}

/// See the module docs.
#[derive(Default)]
pub struct AskBox {
    ui: Option<Ui>,
    overlay: GpuOverlay,
    input: Option<Input>,
    state: StateMap,
    problem: Option<String>,
    open: bool,
    /// Private: the words typed, and an IME composition in progress.
    text: String,
    composing: String,
    /// What it is about (public: the selection, which agents can see in the editor anyway).
    about: String,
    entities: Vec<EntityRef>,
    choices: Vec<Choice>,
    choice: usize,
    /// Drawn below the selection (it was in the top half of the window) or above it.
    below: bool,
    /// Where the selection was on screen when it opened (the leader line's end).
    target: Option<[f32; 2]>,
    dirty: bool,
    laid_out: Option<(u32, u32)>,
    started: Option<std::time::Instant>,
    /// The request was sent and the board hasn't answered: the box stays, with the words, and
    /// sends nothing more until it has.
    sending: bool,
}

fn load(theme: &str, fonts: specula_ui::FontSet) -> Result<Ui, String> {
    let theme = Theme::from_json_str(theme).map_err(|e| format!("editor_theme.json: {e}"))?;
    let mut ui = Ui::new(theme, fonts);
    ui.load_screen(SCREEN).map_err(|e| format!("editor_ask.json: {e}"))?;
    Ok(ui)
}

fn actions() -> Result<ActionMap, String> {
    let mut map = ActionMap::from_json_str(r#"{"format": "specula.input.v1", "actions": []}"#).map_err(|e| format!("ask actions: {e}"))?;
    specula_ui::add_ui_actions(&mut map).map_err(|e| format!("ui actions: {e}"))?;
    Ok(map)
}

impl AskBox {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// For `editor_state` and traces: whether it is open and what it is about. Never the words,
    /// nor how many there are.
    pub fn status_json(&self) -> Json {
        if !self.open {
            return Json::obj([("open", Json::from(false))]);
        }
        Json::obj([
            ("open", Json::from(true)),
            ("about", Json::from(self.about.as_str())),
            ("part", Json::from(self.choices.get(self.choice).map_or_else(String::new, Choice::describe))),
            ("words", Json::from("private to the human until sent")),
            // Where its buttons are (window pixels; only the human's own pointer presses them).
            ("buttons", self.button_rects()),
        ])
    }

    /// The selection's screen point it was opened beside (the leader line's end).
    pub fn target(&self) -> Option<[f32; 2]> {
        self.target.filter(|_| self.open)
    }

    /// Opens it (the human's press): about `entities` (the primary last), with `choices` beside
    /// the whole selection, beside `target` (the selection on screen) in a window of `size`.
    pub fn open(&mut self, about: String, entities: Vec<EntityRef>, choices: Vec<Choice>, target: Option<[f32; 2]>, size: (u32, u32)) {
        self.open = true;
        self.text.clear();
        self.composing.clear();
        self.about = about;
        self.entities = entities;
        self.choices = std::iter::once(Choice::Whole).chain(choices).collect();
        self.choice = 0;
        self.target = target;
        self.below = target.is_none_or(|t| t[1] < size.1 as f32 * 0.5);
        self.dirty = true;
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.show(SCREEN_NAME);
        }
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.ask_open", by = crate::authority::WINDOW_OWNER, about = self.about.as_str());
    }

    /// Starts the open box on choice `i` of the list `open` was given (0: the first after "the
    /// whole selection"): what the human pointed at before asking. Tab still steps through all.
    pub fn choose(&mut self, i: usize) {
        if self.open && i + 1 < self.choices.len() {
            self.choice = i + 1;
            self.dirty = true;
        }
    }

    /// Closes it; what was typed is forgotten (`why` is traced).
    pub fn close(&mut self, why: &str) {
        if !self.open {
            return;
        }
        self.open = false;
        self.sending = false;
        self.text.clear();
        self.composing.clear();
        self.entities.clear();
        self.target = None;
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.hide(SCREEN_NAME);
        }
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "editor.ask_closed", by = crate::authority::WINDOW_OWNER, why = why);
    }

    fn ensure(&mut self, theme: &str, fonts: impl FnOnce() -> Result<specula_ui::FontSet, String>) -> bool {
        if self.ui.is_none() && self.problem.is_none() {
            match fonts().and_then(|f| load(theme, f)).and_then(|ui| actions().map(|a| (ui, a))) {
                Ok((ui, map)) => {
                    self.ui = Some(ui);
                    self.input = Some(Input::new(map));
                }
                Err(e) => self.problem = Some(e),
            }
        }
        self.ui.is_some()
    }

    /// One window frame of the human's own input while it is open (never an agent's injected
    /// frames). Keys type; Enter sends, Shift+Enter is a new line, Escape cancels; the pointer
    /// presses its buttons.
    #[cfg(test)]
    pub fn frame(&mut self, events: &[Event], size: (u32, u32), theme: &str, fonts: impl FnOnce() -> Result<specula_ui::FontSet, String>) -> Option<AskOut> {
        self.frame_with_pads(events, &[], size, theme, fonts)
    }

    /// Whether the request was sent and its answer is awaited.
    pub fn sending(&self) -> bool {
        self.sending
    }

    /// The request went to the board (`true`), or its answer came back refused (`false`: the
    /// words are still here to send again).
    pub fn set_sending(&mut self, on: bool) {
        self.sending = on;
        self.dirty = true;
    }

    /// [`AskBox::frame`] with the window's pads: they press the box's buttons, so a human with
    /// only a pad in hand can send or cancel (review R1 of m5-edswarm, minor 6: the box was
    /// given no pads, and the only way out was the review screen's chord).
    pub fn frame_with_pads(
        &mut self,
        events: &[Event],
        pads: &[specula_platform::gamepad::GamepadState],
        size: (u32, u32),
        theme: &str,
        fonts: impl FnOnce() -> Result<specula_ui::FontSet, String>,
    ) -> Option<AskOut> {
        if !self.open || size.0 == 0 || size.1 == 0 {
            return None;
        }
        let mut out = None;
        let mut shift = false;
        for e in events {
            match e {
                Event::KeyDown { code: VK_SHIFT, .. } => shift = true,
                Event::KeyUp { code: VK_SHIFT } => shift = false,
                Event::Char(c) if !c.is_control() => {
                    if self.text.chars().count() < MAX_REQUEST {
                        self.text.push(*c);
                    }
                    self.dirty = true;
                }
                Event::ImeComposition { text, .. } => {
                    self.composing = text.chars().take(64).collect();
                    self.dirty = true;
                }
                Event::ImeCommit(_) | Event::ImeEnd => {
                    // The committed characters follow as Char events.
                    self.composing.clear();
                    self.dirty = true;
                }
                Event::KeyDown { code: VK_BACK, .. } => {
                    self.text.pop();
                    self.dirty = true;
                }
                Event::KeyDown { code: VK_RETURN, repeat: false } if shift => {
                    if self.text.chars().count() < MAX_REQUEST {
                        self.text.push('\n');
                    }
                    self.dirty = true;
                }
                Event::KeyDown { code: VK_RETURN, repeat: false } => out = out.or_else(|| self.send()),
                // (While the request is with the board, Cancel waits: it can't be taken back,
                // and its answer closes the box or gives the words back.)
                Event::KeyDown { code: VK_ESCAPE, repeat: false } if !self.sending => out = Some(AskOut::Cancel),
                // Tab: the next part (Shift+Tab the one before), from the keyboard alone.
                Event::KeyDown { code: VK_TAB, .. } => {
                    let n = self.choices.len().max(1);
                    self.choice = if shift { (self.choice + n - 1) % n } else { (self.choice + 1) % n };
                    self.dirty = true;
                }
                _ => {}
            }
        }
        if out.is_some() || !self.ensure(theme, fonts) {
            return out;
        }
        // The pointer and pad press its buttons (keys are the words', not the UI's).
        let pointer: Vec<Event> = events.iter().filter(|e| !matches!(e, Event::KeyDown { .. } | Event::KeyUp { .. } | Event::Char(_) | Event::ImeComposition { .. } | Event::ImeCommit(_) | Event::ImeEnd)).cloned().collect();
        let now = self.started.get_or_insert_with(std::time::Instant::now).elapsed().as_secs_f64();
        let input = self.input.as_mut().expect("ensured");
        input.update(now, &pointer, pads);
        self.fill_if_dirty();
        let ui = self.ui.as_mut().expect("ensured");
        if !ui.is_visible(SCREEN_NAME) {
            let _ = ui.show(SCREEN_NAME);
        }
        let vp = Viewport::new([size.0.max(1), size.1.max(1)], ui.theme().scale.scale([size.0.max(1), size.1.max(1)], None, 1.0));
        let acts: Vec<std::rc::Rc<str>> = ui
            .frame(self.input.as_ref().expect("ensured"), &self.state, &vp)
            .events
            .events
            .iter()
            .filter_map(|e| match e {
                specula_ui::UiEvent::Action { action, .. } => Some(action.clone()),
                _ => None,
            })
            .collect();
        self.laid_out = Some(size);
        for a in acts {
            match &*a {
                "ask_send" => out = out.or_else(|| self.send()),
                "ask_cancel" if !self.sending => out = Some(AskOut::Cancel),
                "ask_part" => {
                    self.choice = (self.choice + 1) % self.choices.len().max(1);
                    self.dirty = true;
                }
                _ => {}
            }
        }
        out
    }

    /// Send: the words and the selection, if there are words.
    fn send(&mut self) -> Option<AskOut> {
        let text = self.text.trim().to_owned();
        // (One request at a time: while the board's answer is awaited, nothing more is sent.)
        if text.is_empty() || self.sending {
            return None;
        }
        Some(AskOut::Send { text, entities: self.entities.clone(), choice: self.choices.get(self.choice).cloned().unwrap_or(Choice::Whole) })
    }

    fn fill_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let s = &mut self.state;
        s.reset();
        s.set_bool("ask_below", self.below);
        s.set_bool("ask_above", !self.below);
        s.set_str("ask_about", &format!("About {}", self.about));
        let part = self.choices.get(self.choice).map_or_else(String::new, Choice::describe);
        s.set_str("ask_part", &format!("Part: {part}"));
        s.set_bool("ask_has_parts", self.choices.len() > 1);
        // The words, the end of them when long, with the composition and a caret.
        let all: Vec<char> = self.text.chars().chain(self.composing.chars()).collect();
        let shown: String = if all.len() > SHOWN_CHARS { std::iter::once('\u{2026}').chain(all[all.len() - SHOWN_CHARS..].iter().copied()).collect() } else { all.into_iter().collect() };
        s.set_str("ask_text", &format!("{shown}_"));
        s.set_bool("ask_can_send", !self.text.trim().is_empty());
    }

    /// Its quads, after the editor's (within `room`).
    pub fn overlay<G: OverlayTextures + ?Sized>(&mut self, gpu: &mut G, size: (u32, u32), room: usize, out: &mut Vec<OverlayQuad>) {
        if !self.open || self.ui.is_none() || size.0 == 0 || size.1 == 0 {
            return;
        }
        if self.laid_out != Some(size) || self.dirty {
            self.fill_if_dirty();
            let ui = self.ui.as_mut().expect("loaded");
            let vp = Viewport::new([size.0, size.1], ui.theme().scale.scale([size.0, size.1], None, 1.0));
            if let Some(input) = self.input.as_mut() {
                input.update(self.started.get_or_insert_with(std::time::Instant::now).elapsed().as_secs_f64(), &[], &[]);
                ui.frame(input, &self.state, &vp);
            }
            self.laid_out = Some(size);
        }
        let ui = self.ui.as_mut().expect("loaded");
        if self.overlay.sync_atlas(gpu, ui.atlas_mut()).is_err() {
            return;
        }
        if let Ok(q) = self.overlay.build(ui) {
            let room = room.min(specula_gpu::MAX_OVERLAY_QUADS.saturating_sub(out.len()));
            out.extend_from_slice(&q[..q.len().min(room)]);
        }
    }

    fn button_rects(&self) -> Json {
        let Some(ui) = self.ui.as_ref() else { return Json::Null };
        let suffix = if self.below { "_b" } else { "_a" };
        let mut f: Vec<(&str, Json)> = Vec::new();
        for name in ["ask_send", "ask_cancel", "ask_part"] {
            let r = ui.widget(SCREEN_NAME, &format!("{name}{suffix}")).and_then(|w| ui.rect(w));
            if let Some(r) = r.filter(|r| r.w > 0.0 && r.h > 0.0) {
                f.push((name, Json::arr([r.x, r.y, r.w, r.h].map(|v| Json::from((f64::from(v) * 10.0).round() / 10.0)))));
            }
        }
        Json::obj(f)
    }

    /// The box's rectangle on screen, once laid out (the leader line starts at its edge).
    pub fn rect(&self) -> Option<[f32; 4]> {
        let ui = self.ui.as_ref().filter(|_| self.open)?;
        let w = ui.widget(SCREEN_NAME, if self.below { "ask_panel_below" } else { "ask_panel_above" })?;
        let r = ui.rect(w)?;
        Some([r.x, r.y, r.w, r.h])
    }

    #[cfg(test)]
    pub(crate) fn typed_for_test(&self) -> &str {
        &self.text
    }
}

impl crate::Engine {
    /// The human is typing: into a world text the operator pointed the window's keyboard at, or
    /// into the editor's Ask box. Only the owner or a granted lead then puts a screen on the
    /// window (`window_typing`; TEXT.md, "Screens agents open").
    pub(crate) fn human_typing(&self) -> bool {
        self.text_typing.focus.is_some() || self.editor.ui.asking()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(id: u64) -> EntityRef {
        EntityRef { uid: crate::world::EntityUid { lineage: 1, id }, path: format!("/e{id}") }
    }

    #[test]
    fn typing_sends_the_words_and_the_part() {
        let mut a = AskBox::default();
        a.open("/statue".into(), vec![r(4)], vec![Choice::Recipe("head".into())], Some([10.0, 10.0]), (800, 600));
        let fonts = || Err::<specula_ui::FontSet, String>("no fonts in tests".into());
        let keys: Vec<Event> = "Make it bigger".chars().map(Event::Char).collect();
        assert_eq!(a.frame(&keys, (800, 600), "", fonts), None);
        assert_eq!(a.typed_for_test(), "Make it bigger");
        // Backspace, then the part: cycled by its button (here: directly).
        a.frame(&[Event::KeyDown { code: VK_BACK, repeat: false }], (800, 600), "", fonts);
        a.choice = 1;
        match a.frame(&[Event::KeyDown { code: VK_RETURN, repeat: false }], (800, 600), "", fonts) {
            Some(AskOut::Send { text, entities, choice }) => {
                assert_eq!(text, "Make it bigge");
                assert_eq!(entities, vec![r(4)]);
                assert_eq!(choice.part(0), Some(Part { entity: Some(0), kind: PartKind::RecipePart { name: "head".into() } }));
            }
            other => panic!("{other:?}"),
        }
        // Escape cancels; nothing typed is kept when it closes.
        assert_eq!(a.frame(&[Event::KeyDown { code: VK_ESCAPE, repeat: false }], (800, 600), "", fonts), Some(AskOut::Cancel));
        a.close("test");
        assert_eq!(a.typed_for_test(), "");
    }

    /// The box's screen loads with the editor's theme, with both placements and their buttons;
    /// Tab picks the part from the keyboard alone.
    #[test]
    fn the_screen_loads_and_tab_picks_the_part() {
        let fonts = specula_ui::FontSet::new().with("ui", specula_ui::testfont::box_font_parsed()).with("ui_bold", specula_ui::testfont::box_font_parsed());
        let ui = load(crate::editor::THEME, fonts).expect("editor_ask.json loads");
        for w in ["ask_panel_below", "ask_panel_above", "ask_send_a", "ask_send_b", "ask_cancel_a", "ask_cancel_b", "ask_part_a", "ask_part_b"] {
            assert!(ui.widget(SCREEN_NAME, w).is_some(), "{w}");
        }
        actions().expect("actions");
        let mut a = AskBox::default();
        a.open("/s".into(), vec![r(1)], vec![Choice::Recipe("head".into()), Choice::Words(4)], None, (800, 600));
        let fonts = || Err::<specula_ui::FontSet, String>("none".into());
        a.frame(&[Event::KeyDown { code: VK_TAB, repeat: false }], (800, 600), "", fonts);
        assert_eq!(a.choices[a.choice], Choice::Recipe("head".into()));
        a.frame(&[Event::KeyDown { code: VK_SHIFT, repeat: false }, Event::KeyDown { code: VK_TAB, repeat: false }, Event::KeyUp { code: VK_SHIFT }], (800, 600), "", fonts);
        assert_eq!(a.choices[a.choice], Choice::Whole);
        // Shift+Enter is a new line; Enter alone sends.
        a.frame(&[Event::Char('a'), Event::KeyDown { code: VK_SHIFT, repeat: false }, Event::KeyDown { code: VK_RETURN, repeat: false }, Event::KeyUp { code: VK_SHIFT }, Event::Char('b')], (800, 600), "", fonts);
        assert_eq!(a.typed_for_test(), "a
b");
    }

    /// The words never show in what agents can read; empty words never send; caps hold.
    #[test]
    fn the_words_stay_private_and_capped() {
        let mut a = AskBox::default();
        a.open("/statue".into(), vec![r(1)], vec![], None, (800, 600));
        let fonts = || Err::<specula_ui::FontSet, String>("no fonts".into());
        a.frame(&"secret plan".chars().map(Event::Char).collect::<Vec<_>>(), (800, 600), "", fonts);
        let j = a.status_json().to_string();
        assert!(!j.contains("secret") && !j.contains("11"), "{j}");
        let mut b = AskBox::default();
        b.open("/x".into(), vec![], vec![], None, (800, 600));
        assert_eq!(b.frame(&[Event::KeyDown { code: VK_RETURN, repeat: false }], (800, 600), "", fonts), None, "nothing to send");
        b.frame(&std::iter::repeat_n(Event::Char('x'), MAX_REQUEST + 50).collect::<Vec<_>>(), (800, 600), "", fonts);
        assert_eq!(b.typed_for_test().chars().count(), MAX_REQUEST);
    }
}

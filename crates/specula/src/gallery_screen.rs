//! The window's gallery screen (docs/design/GALLERIES.md, "The pick flow"): the open round's
//! sheet with its numbered variants, "This one", "This one, with edits" (a note and chips),
//! "None of these", "Not this one" and the breadcrumb of earlier rounds.
//!
//! A `specula-ui` screen (`crates/specula/ui/gallery.json`) over the game, like the review
//! screen. Every input works (F5, F6): the mouse and touch (the buttons; a click on a tile
//! selects it, a right-click marks the point disliked), keys (F8 toggles, digits 1-9 select,
//! arrows and Enter, typing into the note), gamepads (LB + RB + X toggles, focus navigation, A).
//! Chips alone finish a round, so nothing needs a keyboard. Modal while open. The sheet is
//! decoded on a worker thread and drawn inside `gl_image`; one texture is held.

use crate::taste::Decision;
use specula_core::Json;
use specula_gpu::{OverlayKind, OverlayQuad, OverlayTextures, TextureFormat, TextureId};
use specula_gustus::gallery::{Gallery, Kind, Status};
use specula_input::{ActionMap, Input};
use specula_platform::gamepad::GamepadState;
use specula_platform::{Event, MouseButton};
use specula_ui::gpu::GpuOverlay;
use specula_ui::{FontSet, StateMap, Theme, Ui, Viewport};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

const THEME: &str = include_str!("../ui/review_theme.json");
const SCREEN: &str = include_str!("../ui/gallery.json");
/// Seconds a newly shown round must be on screen before a press decides it.
const SETTLE_S: f64 = 0.4;
/// Seconds after the human closes the screen during which agents can't reopen it.
const HUMAN_CLOSE_HOLD: std::time::Duration = std::time::Duration::from_secs(30);
/// Longest side the sheet is shown at.
const MAX_IMAGE_SIDE: u32 = 1400;
const MAX_CHIPS: usize = 6;
const MAX_CRUMBS: usize = 8;
const VK_BACK: u32 = 0x08;
const VK_F8: u32 = 0x77;

const ACTIONS: &str = r#"{
  "format": "specula.input.v1",
  "actions": [
    { "name": "gallery_toggle", "kind": "button", "group": "gallery", "description": "Open or close the gallery screen.", "bindings": ["Key.F8", "Pad.LB+Pad.RB+Pad.West"] }
  ]
}"#;

/// A decision pressed on the screen, for the engine to apply.
pub struct Act {
    pub gallery: String,
    pub round: usize,
    pub decision: Decision,
}

/// Which sheet: the gallery, the round, the file.
type ImageKey = (String, usize, PathBuf);

struct Image {
    key: ImageKey,
    texture: Option<TextureId>,
    size: (u32, u32),
    problem: Option<String>,
}

type Loader = (ImageKey, Receiver<Result<specula_core::png::Image, String>>);

pub struct GalleryUi {
    open: bool,
    gallery: Option<String>,
    user: String,
    /// The round shown and when it appeared (input clock).
    round: usize,
    shown_at: f64,
    selected: Option<usize>,
    editing: bool,
    note: String,
    chips: Vec<&'static str>,
    chips_on: Vec<bool>,
    /// The first chip shown (chips page by [`MAX_CHIPS`]: every chip is reachable by hand).
    chip_page: usize,
    /// The last round the breadcrumb shows (`None`: the newest; it pages back by [`MAX_CRUMBS`]).
    crumb_end: Option<usize>,
    ui: Option<Ui>,
    overlay: GpuOverlay,
    problem: Option<String>,
    loaded: bool,
    input: Option<Input>,
    idle: Option<Input>,
    state: StateMap,
    image: Option<Image>,
    want: Option<ImageKey>,
    loader: Option<Loader>,
    /// The shown round's tiles (sheet pixels) and the sheet's size.
    tiles: Vec<[u32; 4]>,
    sheet_size: (u32, u32),
    /// Where the sheet is drawn (window pixels), from the last layout.
    image_rect: Option<[f32; 4]>,
    /// The crumbs' rounds, by button.
    crumbs: Vec<usize>,
    status: (String, bool),
    laid_out: Option<(u32, u32)>,
    started: std::time::Instant,
    human_closed: Option<std::time::Instant>,
    dirty: bool,
    focus: Option<String>,
    /// An agent put what is shown on the window (its `gallery_open`): the first round the
    /// screen shows keeps its focus on Close.
    by_agent: bool,
    retired: Vec<TextureId>,
    /// LB + RB + X held at the last closed frame (edge detection while closed).
    pad_combo: bool,
    /// Widget rects (window pixels) of the last layout, for agents and scripts.
    rects: Vec<(String, [f32; 4])>,
    /// Input that came after a selecting click in the same frame, held for the next frame (see
    /// [`GalleryUi::frame`]).
    deferred: Vec<Event>,
    /// Mouse presses the screen has seen, and the last action its widgets sent (agents and
    /// scripts read them to tell a lost click from a refused one).
    presses_seen: u64,
    last_ui_action: Option<String>,
}

impl Default for GalleryUi {
    fn default() -> Self {
        GalleryUi {
            open: false,
            gallery: None,
            user: crate::authority::ROLE_OWNER.into(),
            round: 0,
            shown_at: f64::NEG_INFINITY,
            selected: None,
            editing: false,
            note: String::new(),
            chips: Vec::new(),
            chips_on: Vec::new(),
            chip_page: 0,
            crumb_end: None,
            ui: None,
            overlay: GpuOverlay::new(),
            problem: None,
            loaded: false,
            input: None,
            idle: None,
            state: StateMap::new(),
            image: None,
            want: None,
            loader: None,
            tiles: Vec::new(),
            sheet_size: (0, 0),
            image_rect: None,
            crumbs: Vec::new(),
            status: (String::new(), true),
            laid_out: None,
            started: std::time::Instant::now(),
            human_closed: None,
            dirty: true,
            focus: None,
            by_agent: false,
            retired: Vec::new(),
            pad_combo: false,
            rects: Vec::new(),
            deferred: Vec::new(),
            presses_seen: 0,
            last_ui_action: None,
        }
    }
}

fn actions() -> Result<ActionMap, String> {
    let mut map = ActionMap::from_json_str(ACTIONS).map_err(|e| format!("gallery actions: {e}"))?;
    specula_ui::add_ui_actions(&mut map).map_err(|e| format!("ui actions: {e}"))?;
    Ok(map)
}

fn load(fonts: FontSet) -> Result<Ui, String> {
    let theme = Theme::from_json_str(THEME).map_err(|e| format!("review_theme.json: {e}"))?;
    let mut ui = Ui::new(theme, fonts);
    ui.load_screen(SCREEN).map_err(|e| format!("gallery.json: {e}"))?;
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
    Ok(fonts)
}

fn viewport(ui: &Ui, size: (u32, u32)) -> Viewport {
    let size = [size.0.max(1), size.1.max(1)];
    Viewport::new(size, ui.theme().scale.scale(size, None, 1.0))
}

fn decode(path: &std::path::Path) -> Result<specula_core::png::Image, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        if u64::from(w) * u64::from(h) > 1 << 24 {
            return Err(format!("{w}x{h}: too large to show"));
        }
    }
    let img = specula_core::png::decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(crate::review_screen::shrink(img, MAX_IMAGE_SIDE))
}

/// Keys that reach the screen's actions while the note is being typed into (the rest are text).
fn editing_key(code: u32) -> bool {
    matches!(code, 0x0D | 0x1B | 0x09 | 0x25..=0x28 | VK_F8 | 0x21 | 0x22)
}

impl GalleryUi {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The screen's interface for the engine's `ui_*` tools: the UI, the state it binds, its
    /// input map (`None` before the screen first loaded).
    pub(crate) fn ui_parts(&mut self) -> Option<(&mut Ui, &StateMap, Option<&ActionMap>)> {
        let map = self.input.as_ref().map(Input::map);
        Some((self.ui.as_mut()?, &self.state, map))
    }

    pub fn gallery(&self) -> Option<&str> {
        self.gallery.as_deref()
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    pub fn has_textures(&self) -> bool {
        self.image.as_ref().is_some_and(|i| i.texture.is_some()) || !self.retired.is_empty()
    }

    /// Something changed (a round arrived, a decision): the screen fills again.
    pub fn changed(&mut self) {
        self.dirty = true;
    }

    pub fn closed_by_human_recently(&self) -> bool {
        self.human_closed.is_some_and(|t| t.elapsed() < HUMAN_CLOSE_HOLD)
    }

    /// Puts gallery `id` on the window (the input monitor saw it first: `_seen`).
    pub fn open(&mut self, id: &str, user: &str, seen: crate::inputguard::Seen) {
        if self.gallery.as_deref() != Some(id) {
            self.round = 0;
            self.selected = None;
            self.editing = false;
        }
        self.gallery = Some(id.to_owned());
        self.user = user.to_owned();
        self.open = true;
        self.dirty = true;
        self.shown_at = self.started.elapsed().as_secs_f64();
        // The human's own F8, and the next onboarding step after their own acceptance, open on
        // the first variant. A gallery an agent puts on the window starts on Close (owner,
        // 2026-11-13), as the review screen does: a key meant for something else picks nothing.
        self.by_agent = seen.by_agent();
        self.focus = Some(if seen.by_agent() { "gl_close".into() } else { "gl_v0".into() });
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.show("gallery");
        }
    }

    /// As [`crate::review_screen::ReviewUi::defuse`]: when the gallery took the window from
    /// under the human's typing, its focus goes to Close, so the Enter or Space meant for their
    /// words picks nothing (review R2 of m5-edswarm, N1).
    pub fn defuse(&mut self) {
        if self.open {
            self.focus = Some("gl_close".into());
        }
    }

    /// The focus the next frame will set (tests).
    #[cfg(test)]
    pub fn pending_focus(&self) -> Option<&str> {
        self.focus.as_deref()
    }

    /// Opens with no gallery to show (it says so; agents put galleries here).
    pub fn open_empty(&mut self, _seen: crate::inputguard::Seen) {
        self.gallery = None;
        self.round = 0;
        self.open = true;
        self.dirty = true;
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.show("gallery");
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.editing = false;
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.hide("gallery");
        }
    }

    fn close_by_human(&mut self) {
        self.close();
        self.human_closed = Some(std::time::Instant::now());
    }

    /// While closed: whether F8 or LB + RB + X was just pressed (the engine then opens it).
    pub fn toggle_pressed(&mut self, events: &[Event], pads: &[GamepadState]) -> bool {
        let key = events.iter().any(|e| matches!(e, Event::KeyDown { code: VK_F8, repeat: false }));
        use specula_platform::gamepad::GamepadButton as B;
        let need = B::LeftBumper.bit() | B::RightBumper.bit() | B::West.bit();
        let combo = pads.iter().any(|p| p.connected && p.buttons & need == need);
        let pressed = key || (combo && !self.pad_combo);
        self.pad_combo = combo;
        pressed
    }

    fn ensure(&mut self) -> bool {
        if !self.loaded {
            self.loaded = true;
            match system_fonts().and_then(load) {
                Ok(ui) => self.ui = Some(ui),
                Err(e) => {
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "gallery.no_screen", error = e.as_str());
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

    /// What the screen shows, as data (with the widgets' places in window pixels, so scripts
    /// press what a human presses).
    /// The note as the screen shows it while the human types one (`None`: not typing, or empty):
    /// `ui_tree` redacts it for anyone but the owner (input sweep R3).
    pub fn note_shown(&self) -> Option<String> {
        (self.editing && !self.note.is_empty()).then(|| format!("{}|", self.note))
    }

    /// `owner`: the caller is the owner. The note the human is typing is theirs alone (input sweep
    /// R3): anyone else gets `note_redacted` and nothing more (not even its length).
    pub fn status_json(&self, owner: bool) -> Json {
        let mut f = vec![
            ("open", Json::Bool(self.open)),
            ("gallery", self.gallery.as_deref().map_or(Json::Null, Json::from)),
            ("round", Json::from(self.round)),
            ("selected", self.selected.map_or(Json::Null, Json::from)),
            ("editing", Json::Bool(self.editing)),
            ("user", Json::from(self.user.as_str())),
            ("presses_seen", Json::from(self.presses_seen)),
            ("last_ui_action", self.last_ui_action.as_deref().map_or(Json::Null, Json::from)),
        ];
        if self.editing && owner {
            f.push(("note", Json::from(self.note.as_str())));
        } else if self.editing {
            // Not even its length: polled per key it gives the typing's rhythm away.
            f.push(("note_redacted", Json::Bool(true)));
        }
        if self.editing {
            f.push(("chips_on", Json::arr(self.chips.iter().zip(&self.chips_on).filter(|(_, on)| **on).map(|(c, _)| Json::from(*c)))));
        }
        if !self.status.0.is_empty() {
            f.push(("last_action", Json::from(self.status.0.as_str())));
        }
        if let Some(p) = &self.problem {
            f.push(("problem", Json::from(p.as_str())));
        }
        f.push(("sheet_shown", Json::Bool(self.image.as_ref().is_some_and(|i| i.texture.is_some() && self.want.as_ref() == Some(&i.key)))));
        let r = |v: [f32; 4]| Json::arr(v.map(|x| Json::from(x.round() as i64)));
        if self.open {
            let mut rects: Vec<(String, Json)> = self.rects.iter().map(|(k, v)| (k.clone(), r(*v))).collect();
            for i in 0..self.tiles.len() {
                if let Some(t) = self.tile_rect(i) {
                    rects.push((format!("tile{i}"), r(t)));
                }
            }
            f.push(("rects", Json::obj(rects)));
        }
        Json::obj(f)
    }

    /// The outcome of a decision, in the status line.
    pub fn report(&mut self, result: &Result<Json, crate::world::WorldError>) {
        self.status = match result {
            Ok(j) => {
                let st = j.get("status").and_then(Json::as_str).unwrap_or("");
                let mut s = match st {
                    "accepted" => "Accepted. Thank you.".to_owned(),
                    "rendering" => "Got it: making the next round\u{2026}".to_owned(),
                    _ => "Noted.".to_owned(),
                };
                if j.get("applied").is_some() {
                    s.push_str(" Its values were written to the project.");
                }
                if j.get("onboarding").and_then(|o| o.get("next_step")).is_some() {
                    s.push_str(" Next step of the style configurator\u{2026}");
                }
                (s, true)
            }
            Err(e) => (format!("Not done: {}", e.message), false),
        };
        self.dirty = true;
    }

    /// A tile's rect in window pixels (where the sheet is drawn).
    fn tile_rect(&self, i: usize) -> Option<[f32; 4]> {
        let r = self.image_rect?;
        let t = self.tiles.get(i)?;
        let (sw, sh) = (self.sheet_size.0.max(1) as f32, self.sheet_size.1.max(1) as f32);
        let (kx, ky) = (r[2] / sw, r[3] / sh);
        Some([r[0] + t[0] as f32 * kx, r[1] + t[1] as f32 * ky, t[2] as f32 * kx, t[3] as f32 * ky])
    }

    /// Which tile (and where on it, 0..1) a window point is on.
    fn tile_at(&self, x: f32, y: f32) -> Option<(usize, [f64; 2])> {
        (0..self.tiles.len()).find_map(|i| {
            let t = self.tile_rect(i)?;
            (x >= t[0] && y >= t[1] && x < t[0] + t[2] && y < t[1] + t[3]).then(|| (i, [f64::from((x - t[0]) / t[2]), f64::from((y - t[1]) / t[3])]))
        })
    }

    fn fill(&mut self, g: Option<&Gallery>, busy: bool, failure: Option<&Json>) {
        let st = &mut self.state;
        st.reset();
        st.set_bool("editing", self.editing);
        for k in ["tip_accept", "tip_edit", "tip_dislike"] {
            st.set_str(k, "Select a variant first: click it, or press its number.");
        }
        st.set_str("status_line", &self.status.0);
        st.set_bool("status_good", self.status.1);
        let Some(g) = g else {
            st.set_str("title", "Galleries");
            st.set_str("image_note", "No gallery is waiting for you. Agents put one here when they want you to choose.");
            return;
        };
        st.set_str("title", &g.title);
        let r = g.rounds.last();
        let open = g.status() == Status::Open && !busy;
        let line = match (r, g.status()) {
            (None, _) => "making the first round\u{2026}".to_owned(),
            (Some(r), Status::Accepted) => format!("round {} \u{b7} accepted", r.n),
            (Some(r), _) if !open => format!("round {} \u{b7} making the next round\u{2026}", r.n),
            // Fewer than planned: say why (the coordinator: "6 options fit the style rules").
            (Some(r), _) if r.fit.is_some() => format!("round {} \u{b7} {} \u{b7} pick one", r.n, crate::studio::gallery::fit_note(r.variants.len())),
            (Some(r), _) => format!("round {} \u{b7} {} variants \u{b7} pick one", r.n, r.variants.len()),
        };
        st.set_str("round_line", &line);
        let crumbs: Vec<usize> = g.rounds.iter().map(|r| r.n).collect();
        let end = self.crumb_end.map_or(crumbs.len(), |e| e.clamp(MAX_CRUMBS.min(crumbs.len()), crumbs.len()));
        let first = end.saturating_sub(MAX_CRUMBS);
        self.crumbs = crumbs[first..end].to_vec();
        st.set_bool("crumbs", crumbs.len() > 1);
        st.set_bool("crumbs_earlier", first > 0);
        st.set_bool("crumbs_later", end < crumbs.len());
        for (k, n) in self.crumbs.iter().enumerate() {
            let rd = &g.rounds[n - 1];
            let mark = match rd.decision.as_ref().map(|d| &d.kind) {
                Some(Kind::Edit { .. }) => " >",
                Some(Kind::None { .. }) => " x",
                Some(Kind::Accept { .. }) => " ok",
                _ => "",
            };
            st.set_str(&format!("c{k}"), &format!("{n}{mark}"));
            st.set_bool(&format!("c{k}_back"), open && *n < r.map_or(0, |r| r.n));
        }
        if let Some(f) = failure {
            let why = f.get("message").and_then(Json::as_str).or_else(|| f.get("error").and_then(Json::as_str)).unwrap_or("it failed");
            st.set_str("failure_line", &format!("The round couldn't be made: {}", specula_gustus::cut(why, 160)));
            st.set_bool("failed", true);
        }
        let Some(r) = r else {
            st.set_str("image_note", if failure.is_some() { "The first round couldn't be made." } else { "Making the first round\u{2026}" });
            return;
        };
        if r.chips_exhausted {
            st.set_str("chips_line", "Those chips can't go any further from your pick: the other parameters vary instead.");
        }
        st.set_bool("has_round", true);
        for (i, _) in r.variants.iter().enumerate().take(12) {
            // (ASCII marks: every UI font has them.)
            let cur = if r.incumbent == Some(i) { " now" } else { "" };
            let label = if self.selected == Some(i) { format!("[{}]{cur}", i + 1) } else { format!("{}{cur}", i + 1) };
            st.set_str(&format!("v{i}"), &label);
        }
        let can_pick = open && self.selected.is_some();
        st.set_bool("can_pick", can_pick);
        // A disabled button's tooltip says what it waits for (a sight-skeptic note: "Accept the
        // selected variant" floated beside a disabled button with nothing selected).
        let first = "Select a variant first: click it, or press its number.";
        st.set_str("tip_accept", if can_pick { "Accept the selected variant as it is." } else { first });
        st.set_str("tip_edit", if can_pick { "Start from it: say what to change (chips or a note); a new round follows." } else { first });
        st.set_str("tip_dislike", if can_pick { "Remember that you dislike the selected variant (the round stays open)." } else { first });
        st.set_bool("can_decide", open);
        if let Some(i) = self.selected.filter(|i| *i < r.variants.len()) {
            let v = &r.variants[i];
            let vals: Vec<String> = g.space.params.iter().zip(&v.values).map(|(p, x)| format!("{} {}", p.name, x.to_json().to_string().trim_matches('"'))).collect();
            st.set_str("selected_line", &format!("Selected {}: {}", i + 1, vals.join(", ")));
        } else if open {
            st.set_str("selected_line", "Select the variant closest to what you want (click it, or press its number).");
        }
        let applies: Vec<String> = g.space.params.iter().filter_map(|p| p.apply.as_ref().map(|a| a.describe())).collect();
        if !applies.is_empty() {
            st.set_str("applies_line", &format!("This one decides: {}", applies.join("; ")));
        }
        let note = match (&self.image, &self.want) {
            (_, None) => "Making this round's sheet\u{2026}".to_owned(),
            (Some(img), Some(w)) if &img.key == w => match (&img.problem, img.texture) {
                (Some(p), _) => format!("The sheet isn't shown: {p}"),
                (None, Some(_)) => String::new(),
                (None, None) => "Loading\u{2026}".into(),
            },
            _ => "Loading\u{2026}".into(),
        };
        st.set_str("image_note", &note);
        if self.editing {
            let shown = if self.note.is_empty() { "(type, or use the chips below)".to_owned() } else { format!("{}|", self.note) };
            st.set_str("note_display", &shown);
            st.set_bool("note", !self.note.is_empty());
            let first = self.chip_page.min(self.chips.len().saturating_sub(1));
            for (k, c) in self.chips.iter().enumerate().skip(first).take(MAX_CHIPS) {
                let on = self.chips_on.get(k).copied().unwrap_or(false);
                st.set_str(&format!("k{}", k - first), &if on { format!("+ {c}") } else { (*c).to_owned() });
            }
            st.set_bool("chips_more", self.chips.len() > MAX_CHIPS);
        }
    }

    fn may_decide(&mut self, g: &Gallery, busy: bool) -> bool {
        let now = self.started.elapsed().as_secs_f64();
        let seen = self.image.as_ref().is_some_and(|i| self.want.as_ref() == Some(&i.key) && (i.texture.is_some() || i.problem.is_some()));
        if g.status() != Status::Open || busy {
            self.status = ("Not done: the next round is still being made".into(), false);
        } else if now - self.shown_at < SETTLE_S || !seen {
            self.status = ("Not done: look at it first (it has only just appeared)".into(), false);
        } else {
            return true;
        }
        self.dirty = true;
        false
    }

    /// One window frame: input, and the decisions pressed. Returns whether it took the input.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, events: &[Event], pads: &[GamepadState], size: (u32, u32), g: Option<&Gallery>, sheet: Option<crate::taste::SheetLayout>, busy: bool, failure: Option<Json>) -> (Vec<Act>, bool) {
        let mut acts = Vec::new();
        let was_open = self.open;
        if !self.ensure() {
            // No font: it can't be drawn, so it never holds the window (humans are never
            // locked in).
            self.open = false;
            return (acts, false);
        }
        if size.0 == 0 || size.1 == 0 {
            return (acts, was_open);
        }
        if self.gallery.is_none() {
            self.gallery = g.map(|g| g.id.clone());
        }
        // The round shown: the last one.
        let n = g.map_or(0, |g| g.rounds.len());
        if n != self.round {
            let first_shown = self.round == 0;
            self.round = n;
            self.crumb_end = None;
            self.selected = None;
            self.editing = false;
            // "Making the next round..." and the like were about the round before.
            if self.status.1 {
                self.status = (String::new(), true);
            }
            self.shown_at = self.started.elapsed().as_secs_f64();
            self.dirty = true;
            // A round that appears starts on its first variant when it is the human's: one
            // their own decision led to (the engine made it from their pick), or the first
            // they see of a gallery they opened. One an agent put on the window leaves the
            // focus on Close: the first round of a gallery an agent opened, and any round an
            // agent made itself (`Made::Agent`). The round says who made it, so nothing has to
            // be remembered between a decision and its round. (Review R1 of m5-eddraw, M2: an
            // agent's open set Close and this line moved it to the first variant on the first
            // frame; R2, N6: a flag set by a decision outlived it.)
            let agents_round = g.and_then(|g| g.rounds.last()).is_some_and(|r| r.made == specula_gustus::gallery::Made::Agent);
            let humans = !agents_round && !(first_shown && self.by_agent);
            self.focus = Some(if humans { "gl_v0".into() } else { "gl_close".into() });
        }
        if let Some(g) = g {
            let chips = g.space.chips();
            if chips != self.chips {
                self.chips_on = vec![false; chips.len()];
                self.chips = chips;
            }
        }
        // The sheet wanted, and its layout.
        let want = sheet.as_ref().zip(g).map(|((p, _, _), g)| (g.id.clone(), n, p.clone()));
        if want != self.want {
            self.want = want;
            self.dirty = true;
        }
        if let Some((_, tiles, size)) = &sheet {
            self.tiles = tiles.clone();
            self.sheet_size = *size;
        } else {
            self.tiles.clear();
        }
        self.start_load();
        // Input held from the last frame comes first.
        let held;
        let events: &[Event] = if self.deferred.is_empty() {
            events
        } else {
            let mut all = std::mem::take(&mut self.deferred);
            all.extend_from_slice(events);
            held = all;
            &held
        };
        // While typing, only navigation keys reach the actions; characters go to the note.
        let mut fed: Vec<Event> = Vec::with_capacity(events.len());
        let selected_before = self.selected;
        for (k, e) in events.iter().enumerate() {
            // A selection changes which buttons are enabled, and the UI reads presses against
            // the last layout: what follows a selecting click waits for the next frame, so a
            // click on a tile and a press on "This one" that arrive in one frame (a stalled
            // window) both count.
            if self.selected != selected_before {
                self.deferred = events[k..].to_vec();
                break;
            }
            match e {
                Event::KeyDown { code, .. } | Event::KeyUp { code } if self.editing && !editing_key(*code) => {
                    if matches!(e, Event::KeyDown { code: VK_BACK, .. }) {
                        self.note.pop();
                        self.dirty = true;
                    }
                }
                Event::Char(c) if self.editing => {
                    if !c.is_control() && self.note.chars().count() < specula_gustus::gallery::MAX_NOTE {
                        self.note.push(*c);
                        self.dirty = true;
                    }
                }
                Event::KeyDown { code: c @ 0x31..=0x39, repeat: false } if !self.editing && self.open => {
                    let i = (*c - 0x31) as usize;
                    if g.and_then(|g| g.rounds.last()).is_some_and(|r| i < r.variants.len()) {
                        self.selected = Some(i);
                        self.dirty = true;
                    }
                    fed.push(e.clone());
                }
                Event::MouseDown { button, x, y } if self.open => {
                    if let Some((i, at)) = self.tile_at(*x as f32, *y as f32) {
                        match button {
                            MouseButton::Right => {
                                // A Dislike waits the same settle as every decision (input sweep R1:
                                // a right-click the moment a screen appeared counted).
                                if let Some(g) = g.filter(|g| g.status() == Status::Open && !busy) {
                                    if self.may_decide(g, busy) {
                                        acts.push(Act { gallery: g.id.clone(), round: n, decision: Decision::Dislike { variant: i, at: Some(at), note: String::new() } });
                                    }
                                }
                            }
                            _ => {
                                self.selected = Some(i);
                                self.dirty = true;
                            }
                        }
                    }
                    fed.push(e.clone());
                }
                Event::PointerDown { x, y, .. } if self.open => {
                    if let Some((i, _)) = self.tile_at(*x as f32, *y as f32) {
                        self.selected = Some(i);
                        self.dirty = true;
                    }
                    fed.push(e.clone());
                }
                _ => fed.push(e.clone()),
            }
        }
        self.presses_seen += fed.iter().filter(|e| matches!(e, Event::MouseDown { .. } | Event::PointerDown { .. })).count() as u64;
        let now = self.started.elapsed().as_secs_f64();
        let input = self.input.as_mut().expect("ensured");
        input.update(now, &fed, pads);
        if input.pressed("gallery_toggle") && self.open {
            self.close_by_human();
            return (acts, true);
        }
        if self.dirty {
            self.fill(g, busy, failure.as_ref());
            self.dirty = false;
        }
        let ui = self.ui.as_mut().expect("ensured");
        let input = self.input.as_ref().expect("ensured");
        let view = viewport(ui, size);
        let events = ui.frame(input, &self.state, &view).events.clone();
        // A press on a disabled button says why, rather than nothing.
        let refused = events.events.iter().find_map(|e| match e {
            specula_ui::UiEvent::Refused { widget } => ui.widget_id(*widget).map(str::to_owned),
            _ => None,
        });
        if let Some(id) = refused {
            let why = if self.selected.is_none() { "select a variant first (click it, or press its number)" } else if busy { "the next round is still being made" } else { "that isn't open to a decision now" };
            self.status = (format!("Not done ({id}): {why}"), false);
            self.dirty = true;
        }
        self.laid_out = Some(size);
        self.remember_rects();
        let mut close = false;
        if let Some(a) = events.actions().last() {
            self.last_ui_action = Some(a.to_owned());
        }
        for a in events.actions() {
            let Some(g) = g else {
                close |= a == "gallery_close";
                continue;
            };
            let id = g.id.clone();
            match a {
                "gallery_close" => close = true,
                "gallery_retry" => acts.push(Act { gallery: id, round: n, decision: Decision::Retry }),
                "gallery_chips_more" => {
                    self.chip_page = if self.chip_page + MAX_CHIPS < self.chips.len() { self.chip_page + MAX_CHIPS } else { 0 };
                    self.dirty = true;
                }
                "gallery_crumbs_earlier" => {
                    let end = self.crumb_end.unwrap_or(g.rounds.len());
                    self.crumb_end = Some(end.saturating_sub(MAX_CRUMBS).max(MAX_CRUMBS.min(g.rounds.len())));
                    self.dirty = true;
                }
                "gallery_crumbs_later" => {
                    let end = self.crumb_end.unwrap_or(g.rounds.len()) + MAX_CRUMBS;
                    self.crumb_end = (end < g.rounds.len()).then_some(end);
                    self.dirty = true;
                }
                "gallery_accept" => {
                    if let Some(i) = self.selected {
                        if self.may_decide(g, busy) {
                            acts.push(Act { gallery: id, round: n, decision: Decision::Kind(Kind::Accept { variant: i }) });
                        }
                    }
                }
                "gallery_edit" => {
                    if self.selected.is_some() && g.status() == Status::Open && !busy {
                        self.editing = true;
                        self.note.clear();
                        self.chips_on = vec![false; self.chips.len()];
                        self.chip_page = 0;
                        self.focus = Some(if self.chips.is_empty() { "gl_send".into() } else { "gl_k0".into() });
                        self.dirty = true;
                    }
                }
                "gallery_none" => {
                    if self.may_decide(g, busy) {
                        acts.push(Act { gallery: id, round: n, decision: Decision::Kind(Kind::None { note: String::new() }) });
                    }
                }
                "gallery_dislike" => {
                    if let (Some(i), true) = (self.selected, g.status() == Status::Open && !busy) {
                        if self.may_decide(g, busy) {
                            acts.push(Act { gallery: id, round: n, decision: Decision::Dislike { variant: i, at: None, note: String::new() } });
                        }
                    }
                }
                "gallery_note_clear" => {
                    self.note.clear();
                    self.dirty = true;
                }
                "gallery_cancel" => {
                    self.editing = false;
                    self.focus = Some("gl_edit".into());
                    self.dirty = true;
                }
                "gallery_send" => {
                    if let Some(i) = self.selected {
                        if self.may_decide(g, busy) {
                            let chips: Vec<String> = self.chips.iter().zip(&self.chips_on).filter(|(_, on)| **on).map(|(c, _)| (*c).to_owned()).collect();
                            acts.push(Act { gallery: id, round: n, decision: Decision::Kind(Kind::Edit { variant: i, note: self.note.trim().to_owned(), chips }) });
                            self.editing = false;
                        }
                    }
                }
                other => {
                    if let Some(k) = other.strip_prefix("gallery_pick:").and_then(|k| k.parse::<usize>().ok()) {
                        self.selected = Some(k);
                        self.dirty = true;
                    } else if let Some(k) = other.strip_prefix("gallery_chip:").and_then(|k| k.parse::<usize>().ok()) {
                        if let Some(on) = self.chips_on.get_mut(k + self.chip_page) {
                            *on = !*on;
                            self.dirty = true;
                        }
                    } else if let Some(k) = other.strip_prefix("gallery_crumb:").and_then(|k| k.parse::<usize>().ok()) {
                        if let Some(to) = self.crumbs.get(k).copied().filter(|to| *to < n) {
                            if self.may_decide(g, busy) {
                                acts.push(Act { gallery: id, round: n, decision: Decision::Kind(Kind::Back { to }) });
                            }
                        }
                    }
                }
            }
        }
        if acts.iter().any(|a| !matches!(a.decision, Decision::Dislike { .. })) {
            // After a decision a new round appears: focus leaves the decision buttons.
            self.focus = Some("gl_v0".into());
        }
        let back = self.open && events.events.iter().any(|e| matches!(e, specula_ui::UiEvent::Back { .. }));
        if (close || back) && self.editing {
            // Esc or a pad's B while typing a note leaves the note, not the screen.
            self.editing = false;
            self.focus = Some("gl_edit".into());
            self.dirty = true;
        } else if close || back {
            self.close_by_human();
        }
        if let Some(ui) = self.ui.as_mut() {
            if self.open != ui.is_visible("gallery") {
                let _ = if self.open { ui.show("gallery") } else { ui.hide("gallery") };
                self.dirty = true;
            }
            if let Some(id) = self.focus.take().filter(|_| self.open) {
                if let Some(w) = ui.widget("gallery", &id) {
                    let _ = ui.focus(w);
                }
            }
        }
        (acts, was_open || self.open)
    }

    fn remember_rects(&mut self) {
        let Some(ui) = self.ui.as_ref() else { return };
        self.rects.clear();
        const IDS: [&str; 15] = ["gl_accept", "gl_edit", "gl_none", "gl_dislike", "gl_send", "gl_cancel", "gl_close", "gl_clear", "gl_image", "gl_note", "gl_retry", "gl_kmore", "gl_cprev", "gl_cnext", "gl_sheet"];
        let extra = (0..12).map(|i| format!("gl_v{i}")).chain((0..MAX_CHIPS).map(|i| format!("gl_k{i}"))).chain((0..MAX_CRUMBS).map(|i| format!("gl_c{i}")));
        for id in IDS.iter().map(|s| (*s).to_owned()).chain(extra) {
            if let Some(r) = ui.widget("gallery", &id).and_then(|w| ui.rect(w)) {
                if r.w > 0.0 && r.h > 0.0 {
                    self.rects.push((id, [r.x, r.y, r.w, r.h]));
                }
            }
        }
        // Where the sheet goes: fitted inside the panel's padding, centred.
        self.image_rect = self.rects.iter().find(|(k, _)| k == "gl_image").and_then(|(_, r)| {
            let (w, h) = self.sheet_size;
            (w > 0 && h > 0 && !self.tiles.is_empty()).then(|| {
                let pad = 6.0;
                let (bw, bh) = ((r[2] - 2.0 * pad).max(1.0), (r[3] - 2.0 * pad).max(1.0));
                let s = (bw / w as f32).min(bh / h as f32);
                let (dw, dh) = (w as f32 * s, h as f32 * s);
                [r[0] + pad + (bw - dw) / 2.0, r[1] + pad + (bh - dh) / 2.0, dw, dh]
            })
        });
    }

    fn start_load(&mut self) {
        if self.loader.is_some() {
            return;
        }
        let Some(key) = self.want.clone() else { return };
        if self.image.as_ref().is_some_and(|i| i.key == key) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let path = key.2.clone();
        match std::thread::Builder::new().name("specula-gallery-image".into()).spawn(move || {
            let _ = tx.send(decode(&path));
        }) {
            Ok(_) => self.loader = Some((key, rx)),
            Err(e) => {
                let old = self.image.replace(Image { key, texture: None, size: (0, 0), problem: Some(format!("could not start the image loader: {e}")) });
                self.retired.extend(old.and_then(|i| i.texture));
            }
        }
    }

    fn poll_load<G: OverlayTextures + ?Sized>(&mut self, gpu: &mut G) {
        let Some((key, rx)) = &self.loader else { return };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("the image loader stopped".into()),
        };
        let key = key.clone();
        self.loader = None;
        if self.want.as_ref() == Some(&key) {
            let mut img = Image { key, texture: None, size: (0, 0), problem: None };
            match result {
                Ok(px) => match gpu.upload_texture(px.width, px.height, TextureFormat::Rgba8Srgb, &px.rgba) {
                    Ok(t) => {
                        img.texture = Some(t);
                        img.size = (px.width, px.height);
                    }
                    Err(e) => img.problem = Some(format!("upload: {e}")),
                },
                Err(e) => img.problem = Some(e),
            }
            let old = self.image.replace(img);
            self.retired.extend(old.and_then(|i| i.texture));
            // The round counts as seen from when its sheet is drawn.
            self.shown_at = self.started.elapsed().as_secs_f64();
            self.dirty = true;
        }
        self.start_load();
    }

    /// The overlay for one image, after the game's, within `room` quads.
    pub fn overlay<G: OverlayTextures + ?Sized>(&mut self, gpu: &mut G, size: (u32, u32), capture: bool, room: usize, out: &mut Vec<OverlayQuad>) {
        for t in self.retired.drain(..) {
            gpu.destroy_texture(t);
        }
        self.poll_load(gpu);
        if !self.open || size.0 == 0 || size.1 == 0 || !self.ensure() {
            return;
        }
        if capture || self.laid_out != Some(size) || self.dirty {
            self.relayout(size, capture);
        }
        let ui = self.ui.as_mut().expect("ensured");
        if let Err(e) = self.overlay.sync_atlas(gpu, ui.atlas_mut()) {
            self.problem = Some(format!("glyph atlas: {e}"));
            return;
        }
        self.overlay_quads(room, out);
    }

    /// Layout without new input (captures, a size the window frame didn't lay out, or state
    /// changed since).
    fn relayout(&mut self, size: (u32, u32), capture: bool) {
        // A capture never shows the note the human is typing (input sweep R3: any agent's capture
        // with the interface drew it); the window's next frame fills it in again.
        let masked = capture && self.editing && !self.note.is_empty();
        if masked {
            self.state.set_str("note_display", "(the owner is typing a note: not shown in captures)");
            self.dirty = true;
        }
        {
            let ui = self.ui.as_mut().expect("ensured");
            if !ui.is_visible("gallery") {
                let _ = ui.show("gallery");
            }
            let view = viewport(ui, size);
            // The layout pass sees the pointer where it is, and a held button still held: a
            // fresh input there read as "released, nowhere", and a press made in the frame
            // before was dropped (taste review 2: gallery_check's restart step lost "This one"
            // about one run in six, whenever a re-layout fell between the press and the release).
            let idle = self.idle.as_mut().expect("ensured");
            let mut held = Vec::with_capacity(2);
            if let Some(live) = self.input.as_ref() {
                held.extend(pointer_held(live, idle));
            }
            idle.update(self.started.elapsed().as_secs_f64(), &held, &[]);
            ui.frame(idle, &self.state, &view);
        }
        self.laid_out = if capture { None } else { Some(size) };
        self.remember_rects();
    }

    /// The screen's quads, the sheet and the selection frame.
    fn overlay_quads(&mut self, room: usize, out: &mut Vec<OverlayQuad>) {
        let ui = self.ui.as_mut().expect("ensured");
        let quads = match self.overlay.build(ui) {
            Ok(q) => q,
            Err(e) => {
                self.problem = Some(format!("overlay: {e}"));
                return;
            }
        };
        let room = room.saturating_sub(6);
        out.extend_from_slice(&quads[..quads.len().min(room)]);
        let img = self.image.as_ref().filter(|i| self.want.as_ref() == Some(&i.key));
        if let (Some(r), Some(Image { texture: Some(t), .. })) = (self.image_rect, img) {
            out.push(OverlayQuad::textured(r, [0.0, 0.0, 1.0, 1.0], *t, OverlayKind::Rgba, [1.0; 4]));
            // The selected tile: a cyan frame (STYLE.md's highlight colour).
            if let Some(tr) = self.selected.and_then(|i| self.tile_rect(i)) {
                let (c, k) = ([0.25, 0.88, 0.82, 1.0], 3.0);
                out.push(OverlayQuad::solid([tr[0] - k, tr[1] - k, tr[2] + 2.0 * k, k], c));
                out.push(OverlayQuad::solid([tr[0] - k, tr[1] + tr[3], tr[2] + 2.0 * k, k], c));
                out.push(OverlayQuad::solid([tr[0] - k, tr[1], k, tr[3]], c));
                out.push(OverlayQuad::solid([tr[0] + tr[2], tr[1], k, tr[3]], c));
            }
        }
    }
}

/// What the layout-only input must hear so its pointer agrees with the live one: where the mouse
/// is, and the left button going down or up to match.
fn pointer_held(live: &Input, idle: &Input) -> Vec<Event> {
    use specula_input::Source;
    let mut out = Vec::new();
    let Some(p) = live.pointer().filter(|p| p.kind == specula_platform::PointerKind::Mouse) else { return out };
    let (x, y) = (p.x as i32, p.y as i32);
    out.push(Event::MouseMove { x, y });
    let left = Source::Mouse(MouseButton::Left);
    match (live.source_held(left), idle.source_held(left)) {
        (true, false) => out.push(Event::MouseDown { button: MouseButton::Left, x, y }),
        (false, true) => out.push(Event::MouseUp { button: MouseButton::Left, x, y }),
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_gustus::gallery::Mode;
    use specula_gustus::space::Space;

    fn test_ui() -> GalleryUi {
        let mut s = GalleryUi::default();
        let fonts = FontSet::new().with("ui", specula_ui::testfont::box_font_parsed());
        s.ui = Some(load(fonts).expect("the gallery screen loads"));
        let map = actions().unwrap();
        s.input = Some(Input::new(map.clone()));
        s.idle = Some(Input::new(map));
        s.loaded = true;
        s
    }

    fn gallery() -> Gallery {
        let space = Space::parse(&Json::parse(r##"[{"name": "moon", "target": "light.key_color", "base": "#d0d9f0"}]"##).unwrap()).unwrap();
        let mut g = Gallery::new("moon", "Moonlight", Json::obj([("stage", Json::obj::<String>([]))]), space, Mode::Tune, 1, 9, "maker").unwrap();
        let p = g.first_round(None);
        g.push_round(p, Some("gallery-moon-r1.png".into()), vec![]).unwrap();
        g
    }

    fn sheet() -> Option<crate::taste::SheetLayout> {
        let tiles = (0..9).map(|i| [6 + (i % 3) * 262, 22 + (i / 3) * 262, 256, 256]).collect();
        Some((PathBuf::from("missing-sheet.png"), tiles, (792, 814)))
    }

    #[test]
    fn the_screen_loads_and_lays_out() {
        let mut s = test_ui();
        let g = gallery();
        s.open("moon", crate::authority::ROLE_OWNER, crate::inputguard::seen_for_test());
        let (acts, modal) = s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        assert!(acts.is_empty() && modal);
        let ui = s.ui.as_ref().unwrap();
        for id in ["gl_image", "gl_accept", "gl_edit", "gl_none", "gl_v0", "gl_v8"] {
            assert!(ui.widget("gallery", id).is_some(), "{id}");
        }
        let st = s.status_json(false);
        assert!(st.get("rects").and_then(|r| r.get("gl_accept")).is_some(), "{st}");
        assert!(st.get("rects").and_then(|r| r.get("tile4")).is_some(), "{st}");
    }

    /// Review probes (minor 2): Esc while typing leaves the note, not the screen; every chip is
    /// reachable (paged); without a font the screen never holds the window.
    /// Taste review 2 (gallery_check's restart flake, found with the screen's press counter): a
    /// re-layout between a button's press and its release (the overlay's, when anything changed)
    /// ran with an input that had no pointer, which the UI read as "released, nowhere": the press
    /// was dropped. The layout pass now sees the pointer and the held button.
    /// Review R1 of m5-eddraw, M2: the rule holds once frames run, not only as the open leaves
    /// it. An agent's open stays on Close through the first frames; a round an agent makes
    /// unasked puts the focus on Close; the human's own open, and a round their decision asked
    /// for, start on the first variant.
    #[test]
    fn an_agents_gallery_stays_on_close_once_its_frames_run() {
        let focused = |s: &GalleryUi| {
            let ui = s.ui.as_ref().unwrap();
            ui.focused().and_then(|w| ui.widget_id(w)).map(str::to_owned)
        };
        let size = (1280, 800);
        let mut g = gallery();
        let mut s = test_ui();
        s.open("moon", crate::authority::ROLE_OWNER, crate::inputguard::seen_by_agent_for_test());
        for _ in 0..3 {
            s.frame(&[], &[], size, Some(&g), sheet(), false, None);
        }
        assert_eq!(focused(&s).as_deref(), Some("gl_close"), "an agent's open, after its frames");
        // The human's own open of the same gallery: the first variant.
        s.close();
        s.open("moon", crate::authority::ROLE_OWNER, crate::inputguard::seen_for_test());
        for _ in 0..3 {
            s.frame(&[], &[], size, Some(&g), sheet(), false, None);
        }
        assert_eq!(focused(&s).as_deref(), Some("gl_v0"), "the human's own open");
        // An agent makes a round nobody asked for: the screen turns to it, on Close.
        let mut next = g.first_round(None);
        next.after = g.rounds.len();
        next.made = specula_gustus::gallery::Made::Agent;
        g.push_round(next, Some("gallery-moon-r2.png".into()), vec![]).unwrap();
        for _ in 0..3 {
            s.frame(&[], &[], size, Some(&g), sheet(), false, None);
        }
        assert_eq!(focused(&s).as_deref(), Some("gl_close"), "a round made unasked");
        // A round the human's own decision led to (the engine makes it from their pick): the
        // first variant again. The round says who made it: nothing is remembered in between
        // (review R2 of m5-eddraw, N6).
        let last = g.rounds.len();
        g.decide(last, specula_gustus::gallery::Kind::None { note: "colder".into() }, "window:owner", crate::authority::ROLE_OWNER).unwrap();
        let next = g.plan_next().expect("a decision that asks for another round");
        assert!(next.made != specula_gustus::gallery::Made::Agent);
        g.push_round(next, Some("gallery-moon-r3.png".into()), vec![]).unwrap();
        for _ in 0..3 {
            s.frame(&[], &[], size, Some(&g), sheet(), false, None);
        }
        assert_eq!(focused(&s).as_deref(), Some("gl_v0"), "a round the human's decision led to");
        // And an agent's own round after that: Close, whatever was decided before it.
        let mut own = g.first_round(None);
        own.after = g.rounds.len();
        own.made = specula_gustus::gallery::Made::Agent;
        g.push_round(own, Some("gallery-moon-r4.png".into()), vec![]).unwrap();
        for _ in 0..3 {
            s.frame(&[], &[], size, Some(&g), sheet(), false, None);
        }
        assert_eq!(focused(&s).as_deref(), Some("gl_close"), "an agent's round after the human's decision");
    }

    /// Owner, 2026-11-13: a gallery an agent puts on the window starts on Close; the human's
    /// own open starts on the first variant.
    #[test]
    fn a_gallery_an_agent_opens_starts_on_close() {
        let mut s = test_ui();
        s.open("moon", crate::authority::ROLE_OWNER, crate::inputguard::seen_by_agent_for_test());
        assert_eq!(s.pending_focus(), Some("gl_close"), "an agent's open");
        s.open("moon", crate::authority::ROLE_OWNER, crate::inputguard::seen_for_test());
        assert_eq!(s.pending_focus(), Some("gl_v0"), "the human's own");
        s.defuse();
        assert_eq!(s.pending_focus(), Some("gl_close"), "taken from under their typing");
    }

    #[test]
    fn a_relayout_between_press_and_release_keeps_the_press() {
        let mut s = test_ui();
        let g = gallery();
        s.open("moon", crate::authority::ROLE_OWNER, crate::inputguard::seen_for_test());
        s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        s.selected = Some(1);
        s.dirty = true;
        s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        let e = s.rects.iter().find(|(k, _)| k == "gl_edit").map(|(_, r)| *r).unwrap();
        let (x, y) = ((e[0] + e[2] / 2.0) as i32, (e[1] + e[3] / 2.0) as i32);
        s.frame(&[Event::MouseMove { x, y }, Event::MouseDown { button: MouseButton::Left, x, y }], &[], (1280, 800), Some(&g), sheet(), false, None);
        // Something changed: the overlay lays the screen out again before the release comes.
        s.dirty = true;
        s.relayout((1280, 800), false);
        s.frame(&[Event::MouseUp { button: MouseButton::Left, x, y }], &[], (1280, 800), Some(&g), sheet(), false, None);
        assert!(s.editing, "the press survived the re-layout and \"This one, with edits\" opened the note");
    }

    /// A click on a tile and a press on a button
    /// that needs a selection, arriving in one frame (a window stalled while it renders), both
    /// count: the press is held for the next frame, when the button is enabled.
    #[test]
    fn a_tile_click_and_a_press_in_one_frame_both_count() {
        let mut s = test_ui();
        let g = gallery();
        s.open("moon", crate::authority::ROLE_OWNER, crate::inputguard::seen_for_test());
        s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        let t = s.tile_rect(2).unwrap();
        let e = s.rects.iter().find(|(k, _)| k == "gl_edit").map(|(_, r)| *r).expect("the edit button is laid out");
        let at = |r: [f32; 4]| ((r[0] + r[2] / 2.0) as i32, (r[1] + r[3] / 2.0) as i32);
        let ((tx, ty), (ex, ey)) = (at(t), at(e));
        let batch = [
            Event::MouseMove { x: tx, y: ty },
            Event::MouseDown { button: MouseButton::Left, x: tx, y: ty },
            Event::MouseUp { button: MouseButton::Left, x: tx, y: ty },
            Event::MouseMove { x: ex, y: ey },
            Event::MouseDown { button: MouseButton::Left, x: ex, y: ey },
            Event::MouseUp { button: MouseButton::Left, x: ex, y: ey },
        ];
        s.frame(&batch, &[], (1280, 800), Some(&g), sheet(), false, None);
        assert_eq!(s.selected, Some(2));
        for _ in 0..3 {
            s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        }
        assert!(s.editing, "the press after the selecting click counted (\"This one, with edits\" opened the note)");
        assert!(s.deferred.is_empty());
    }

    /// A sight-skeptic finding: editing on a later round (the tallest view: rounds row, note,
    /// chips) ran off a 1280x800 window. The panel fits.
    #[test]
    fn the_tallest_view_fits_a_small_window() {
        let mut g = gallery();
        g.decide(1, Kind::None { note: String::new() }, "owner-proxy", crate::authority::ROLE_OWNER).unwrap();
        let p = g.plan_next().unwrap();
        g.push_round(p, Some("x.png".into()), vec![]).unwrap();
        let mut s = test_ui();
        s.open("moon", crate::authority::ROLE_OWNER, crate::inputguard::seen_for_test());
        s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        s.selected = Some(0);
        s.editing = true;
        s.dirty = true;
        s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        let panel = s.rects.iter().find(|(k, _)| k == "gl_sheet").map(|(_, r)| *r).expect("the panel is laid out");
        assert!(panel[1] + panel[3] <= 800.0, "the panel ends at y {} in an 800 px window", panel[1] + panel[3]);
    }

    /// A round the style rules trimmed says how many fit, on screen and in the file.
    #[test]
    fn a_trimmed_round_says_how_many_fit() {
        let space = Space::parse(&Json::parse(r##"[{"name": "moon", "target": "light.key_color", "base": "#d0d9f0"}]"##).unwrap()).unwrap();
        let mut g = Gallery::new("moon", "Moonlight", Json::obj([("stage", Json::obj::<String>([]))]), space, Mode::Tune, 1, 9, "maker").unwrap();
        let mut p = g.first_round(None);
        p.points.truncate(6);
        p.fit = Some(6);
        g.push_round(p, Some("x.png".into()), vec![]).unwrap();
        assert_eq!(Gallery::parse(&g.to_text()).unwrap().rounds[0].fit, Some(6));
        let mut s = test_ui();
        s.open("moon", crate::authority::ROLE_OWNER, crate::inputguard::seen_for_test());
        s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        assert_eq!(s.state.get("round_line"), Some(&specula_ui::Value::Str("round 1 \u{b7} 6 options fit the style rules \u{b7} pick one".into())));
    }

    #[test]
    fn typing_paging_and_no_font() {
        let mut s = test_ui();
        let g = gallery();
        s.open("moon", crate::authority::ROLE_OWNER, crate::inputguard::seen_for_test());
        s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        s.selected = Some(0);
        s.editing = true;
        s.dirty = true;
        s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        // The key hints say what Esc does (a sight-skeptic finding: "Esc Close" while typing).
        let tree = s.ui.as_ref().unwrap().tree().to_string();
        assert!(tree.contains("Leave the note"), "while typing, Esc leaves the note");
        s.frame(&[Event::KeyDown { code: 0x1B, repeat: false }, Event::KeyUp { code: 0x1B }], &[], (1280, 800), Some(&g), sheet(), false, None);
        s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        assert!(s.is_open() && !s.editing, "Esc cancels the note, the screen stays");
        let tree = s.ui.as_ref().unwrap().tree().to_string();
        assert!(!tree.contains("Leave the note"), "and the hint goes back to Close");
        // Chips page: the moon space offers 6 chips; a space with more offers "More".
        let space = Space::parse(&Json::parse(r##"[{"name": "moon", "target": "light.key_color", "base": "#d0d9f0"}, {"name": "fog", "target": "fog.density", "min": 0, "max": 1}, {"name": "w", "target": "scale.all", "min": 0.5, "max": 2}]"##).unwrap()).unwrap();
        assert!(space.chips().len() > MAX_CHIPS);
        let mut big = Gallery::new("big", "t", Json::obj([("recipe", Json::from("r.json"))]), space, Mode::Tune, 1, 9, "maker").unwrap();
        let p = big.first_round(None);
        big.push_round(p, Some("x.png".into()), vec![]).unwrap();
        let mut s2 = test_ui();
        s2.open("big", crate::authority::ROLE_OWNER, crate::inputguard::seen_for_test());
        s2.frame(&[], &[], (1280, 800), Some(&big), sheet(), false, None);
        s2.editing = true;
        s2.dirty = true;
        s2.frame(&[], &[], (1280, 800), Some(&big), sheet(), false, None);
        assert_eq!(s2.state.get("chips_more"), Some(&specula_ui::Value::Bool(true)));
        // No font: closed, never modal.
        let mut s3 = GalleryUi { loaded: true, ..GalleryUi::default() };
        s3.open = true;
        let (_, modal) = s3.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        assert!(!modal && !s3.is_open());
    }

    #[test]
    fn selection_edits_and_chips_need_no_keyboard() {
        let mut s = test_ui();
        let g = gallery();
        s.open("moon", crate::authority::ROLE_OWNER, crate::inputguard::seen_for_test());
        s.frame(&[], &[], (1280, 800), Some(&g), sheet(), false, None);
        // A click on tile 5 selects it.
        let t = s.tile_rect(4).unwrap();
        let click = [Event::MouseDown { button: MouseButton::Left, x: (t[0] + 10.0) as i32, y: (t[1] + 10.0) as i32 }, Event::MouseUp { button: MouseButton::Left, x: (t[0] + 10.0) as i32, y: (t[1] + 10.0) as i32 }];
        s.frame(&click, &[], (1280, 800), Some(&g), sheet(), false, None);
        assert_eq!(s.selected, Some(4));
        // A digit selects too; typed text goes to the note only while editing.
        s.frame(&[Event::KeyDown { code: 0x33, repeat: false }], &[], (1280, 800), Some(&g), sheet(), false, None);
        assert_eq!(s.selected, Some(2));
        s.editing = true;
        s.frame(&[Event::Char('c'), Event::Char('o'), Event::KeyDown { code: 0x20, repeat: false }, Event::Char(' '), Event::KeyDown { code: VK_BACK, repeat: false }], &[], (1280, 800), Some(&g), sheet(), false, None);
        assert_eq!(s.note, "co", "space was text (not a press), backspace removed it");
        assert!(s.chips.contains(&"darker"));
        // A right-click marks a dislike with the point, once the round has settled (input sweep
        // R1: a right-click the moment the screen appeared counted).
        s.editing = false;
        let r = s.tile_rect(1).unwrap();
        let right = [Event::MouseDown { button: MouseButton::Right, x: (r[0] + r[2] / 2.0) as i32, y: (r[1] + r[3] / 4.0) as i32 }];
        let (acts, _) = s.frame(&right, &[], (1280, 800), Some(&g), sheet(), false, None);
        assert!(acts.is_empty(), "not before the settle: {} acts", acts.len());
        // A press right after the round appeared (and with no sheet drawn) decides nothing.
        assert!(!s.may_decide(&g, false));
        let key = s.want.clone().expect("a sheet is wanted");
        s.image = Some(Image { key, texture: None, size: (0, 0), problem: Some("test: shown".into()) });
        s.shown_at -= 2.0 * SETTLE_S;
        let (acts, _) = s.frame(&right, &[], (1280, 800), Some(&g), sheet(), false, None);
        assert!(matches!(acts.as_slice(), [Act { decision: Decision::Dislike { variant: 1, at: Some(at), .. }, .. }] if (at[0] - 0.5).abs() < 0.05 && (at[1] - 0.25).abs() < 0.05));
    }
}

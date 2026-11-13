//! The start screen (docs/design/LAUNCH.md): the projects a person worked on, the last one
//! first, when the executable is opened with no project. They open one, make a new one, open a
//! folder as a project, or take one off the list; closing it leaves a plain engine.
//!
//! A `specula-ui` screen (`crates/specula/ui/launch.json`) with a UI instance of its own, as the
//! editor's Ask box has: it hears the window's own input only (never an agent's injected
//! frames), is no UI-tools surface and is drawn in the window's frames only, never in a
//! capture. The list of a person's projects is theirs: no tool serves it, and this screen does
//! not leak it. Mouse, touch, keys and gamepads all work (F5, F6); buttons keep the touch height.
//!
//! This module holds presentation state only. What a press asks for comes back as an [`Out`]
//! for the engine, which does the disk work on a worker (`launch_window`).

use specula_gpu::{OverlayQuad, OverlayTextures};
use specula_input::{ActionMap, Input};
use specula_platform::gamepad::GamepadState;
use specula_platform::Event;
use specula_ui::gpu::GpuOverlay;
use specula_ui::{FontSet, StateMap, Theme, Ui, Viewport};
use std::path::PathBuf;

const THEME: &str = include_str!("../ui/launch_theme.json");
const SCREEN: &str = include_str!("../ui/launch.json");
const SCREEN_NAME: &str = "start";
/// Row slots on the screen (`launch.json` has exactly these).
pub const ROWS: usize = 6;
/// Longest folder path typed or pasted.
pub const MAX_PATH_CHARS: usize = 512;
/// Characters of a typed text shown (the end of a longer one).
const SHOWN_CHARS: usize = 96;
/// Characters of a row's path shown (the end of a longer one).
const PATH_CHARS: usize = 62;

const VK_BACK: u32 = 0x08;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_CONTROL: u32 = 0x11;
const VK_V: u32 = 0x56;

/// The screen's own actions (beside the UI's focus, accept and back).
const ACTIONS: &str = r#"{
  "format": "specula.input.v1",
  "actions": [
    { "name": "start_page_next", "kind": "button", "group": "start", "description": "The next page of projects.", "bindings": ["Key.PageDown", "Pad.RB"] },
    { "name": "start_page_prev", "kind": "button", "group": "start", "description": "The previous page of projects.", "bindings": ["Key.PageUp", "Pad.LB"] }
  ]
}"#;

/// One listed project, as the screen shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub name: String,
    pub path: PathBuf,
    /// Seconds since the Unix epoch it was last opened.
    pub last_opened: u64,
    /// Its folder is gone.
    pub missing: bool,
    /// The port of an engine that has it open now.
    pub running: Option<u16>,
}

/// What a press asks the engine to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// Open this listed project.
    Open(PathBuf),
    /// Make a project of this name (in the default projects folder) and open it.
    Create(String),
    /// Open the folder typed, as a project.
    Folder(String),
    /// Take this project off the list.
    Forget(PathBuf),
    /// Ctrl+V in the folder field: the clipboard's text is wanted ([`StartScreen::pasted`]).
    Paste,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    List,
    New,
    Folder,
    /// Asking before the row at this index (of all rows) is forgotten.
    Forget(usize),
}

pub struct StartScreen {
    open: bool,
    rows: Vec<Row>,
    page: usize,
    mode: Mode,
    text: String,
    composing: String,
    /// The last action's outcome, and whether it went well.
    status: (String, bool),
    /// A request is with the engine (a folder being made or checked): nothing more is sent.
    busy: bool,
    /// Where a new project is made, for the note under the name.
    parent: Option<PathBuf>,
    /// What this engine holds that is in no project (said before it is left behind).
    world_note: String,
    /// Seconds since the Unix epoch, for "3 days ago".
    now: u64,
    ui: Option<Ui>,
    input: Option<Input>,
    state: StateMap,
    overlay: GpuOverlay,
    /// Why the screen can't draw (no font), if so.
    problem: Option<String>,
    dirty: bool,
    laid_out: Option<(u32, u32)>,
    started: Option<std::time::Instant>,
    /// The widget to focus at the end of the next frame.
    focus: Option<String>,
    ctrl: bool,
}

impl Default for StartScreen {
    fn default() -> Self {
        StartScreen {
            open: false,
            rows: Vec::new(),
            page: 0,
            mode: Mode::List,
            text: String::new(),
            composing: String::new(),
            status: (String::new(), true),
            busy: false,
            parent: None,
            world_note: String::new(),
            now: 0,
            ui: None,
            input: None,
            state: StateMap::new(),
            overlay: GpuOverlay::new(),
            problem: None,
            dirty: true,
            laid_out: None,
            started: None,
            focus: None,
            ctrl: false,
        }
    }
}

fn actions() -> Result<ActionMap, String> {
    let mut map = ActionMap::from_json_str(ACTIONS).map_err(|e| format!("start actions: {e}"))?;
    specula_ui::add_ui_actions(&mut map).map_err(|e| format!("ui actions: {e}"))?;
    Ok(map)
}

fn load(fonts: FontSet) -> Result<Ui, String> {
    let theme = Theme::from_json_str(THEME).map_err(|e| format!("launch_theme.json: {e}"))?;
    let mut ui = Ui::new(theme, fonts);
    ui.load_screen(SCREEN).map_err(|e| format!("launch.json: {e}"))?;
    Ok(ui)
}

/// "3 days ago": how long before `now` something happened (both seconds since the epoch).
pub fn ago(now: u64, then: u64) -> String {
    let s = now.saturating_sub(then);
    let plural = |n: u64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match s {
        _ if then == 0 => "never opened".to_owned(),
        0..=89 => "just now".to_owned(),
        90..=5_399 => plural((s + 30) / 60, "minute"),
        5_400..=86_399 => plural((s + 1_800) / 3_600, "hour"),
        86_400..=172_799 => "yesterday".to_owned(),
        172_800..=1_209_599 => plural(s / 86_400, "day"),
        1_209_600..=5_183_999 => plural(s / 604_800, "week"),
        5_184_000..=31_535_999 => plural(s / 2_592_000, "month"),
        _ => "over a year ago".to_owned(),
    }
}

/// The end of `text` when it is longer than `max` characters, after an ellipsis.
fn tail(text: &str, max: usize) -> String {
    let all: Vec<char> = text.chars().collect();
    if all.len() > max {
        std::iter::once('\u{2026}').chain(all[all.len() - max..].iter().copied()).collect()
    } else {
        text.to_owned()
    }
}

/// A folder's path as a row shows it: whole when it fits `max` characters, else its end, from
/// a folder's name on (never from the middle of one), after an ellipsis.
fn tail_path(text: &str, max: usize) -> String {
    let all: Vec<char> = text.chars().collect();
    if all.len() <= max {
        return text.to_owned();
    }
    let end = &all[all.len() - max..];
    // The first separator in the kept part, unless that would drop most of it.
    let from = end.iter().position(|c| matches!(c, '/' | '\\')).filter(|at| *at < max / 2).unwrap_or(0);
    std::iter::once('\u{2026}').chain(end[from..].iter().copied()).collect()
}

/// What a paste gives the folder field: the clipboard's first line, without the quotes
/// Explorer's "Copy as path" puts round it, at most [`MAX_PATH_CHARS`] characters and no
/// control characters.
pub fn pasted_path(clip: &str) -> String {
    let line = clip.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let line = line.strip_prefix('"').and_then(|l| l.strip_suffix('"')).unwrap_or(line);
    line.chars().filter(|c| !c.is_control()).take(MAX_PATH_CHARS).collect()
}

impl StartScreen {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens it with the listed projects (most recent first), `parent` where a new one is made
    /// and `now` (seconds since the epoch).
    pub fn open(&mut self, rows: Vec<Row>, parent: Option<PathBuf>, now: u64) {
        self.open = true;
        self.mode = Mode::List;
        self.page = 0;
        self.parent = parent;
        self.now = now;
        self.busy = false;
        self.status = (String::new(), true);
        self.set_rows(rows);
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.show(SCREEN_NAME);
        }
        // (Not how many projects, nor which: agents in this engine read traces. Review R1-7.)
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "launch.screen_open");
    }

    /// The list changed (one was forgotten): shown from the next frame.
    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.rows = rows;
        self.page = self.page.min(self.pages().saturating_sub(1));
        self.focus = Some(self.home_focus());
        self.dirty = true;
    }

    /// The listed folders were checked (which are gone, which another engine has open): the
    /// rows say so. The person's place is kept: the page, the question being asked and the
    /// focus stay where they are, unless the list itself is another one now.
    pub fn checked(&mut self, rows: Vec<Row>) {
        let same = rows.len() == self.rows.len() && rows.iter().zip(&self.rows).all(|(a, b)| a.path == b.path);
        if same {
            self.rows = rows;
            self.dirty = true;
        } else {
            // (A name or a folder being typed stays; a question about a row is dropped.)
            let typing = matches!(self.mode, Mode::New | Mode::Folder);
            self.rows = rows;
            self.page = self.page.min(self.pages().saturating_sub(1));
            if !typing {
                self.mode = Mode::List;
                self.focus = Some(self.home_focus());
            }
            self.dirty = true;
        }
    }

    /// Closes it: the engine goes on with no project (`why` is traced).
    pub fn close(&mut self, why: &str) {
        if !self.open {
            return;
        }
        self.open = false;
        self.mode = Mode::List;
        self.text.clear();
        self.composing.clear();
        self.busy = false;
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.hide(SCREEN_NAME);
        }
        specula_core::trace_event!(&crate::trace_sink::ENGINE, Info, "launch.screen_closed", why = why);
    }

    /// A request the engine took came back: `Err` is said on the screen and the person goes on
    /// where they were (their typed text is kept); `Ok` is said too (the engine is on its way
    /// to the project).
    pub fn answered(&mut self, result: Result<String, String>) {
        self.busy = false;
        self.status = match result {
            Ok(said) => (said, true),
            Err(why) => (why, false),
        };
        self.dirty = true;
    }

    /// The engine is on its way to a project: said, and nothing more is taken.
    pub fn leaving(&mut self, said: String) {
        self.busy = true;
        self.status = (said, true);
        self.dirty = true;
    }

    /// The clipboard's text for the folder field (asked with [`Out::Paste`]).
    pub fn pasted(&mut self, clip: &str) {
        if self.open && self.mode == Mode::Folder {
            let room = MAX_PATH_CHARS.saturating_sub(self.text.chars().count());
            self.text.extend(pasted_path(clip).chars().take(room));
            self.dirty = true;
        }
    }

    /// What this engine holds that is in no project ("" for nothing): shown as a warning, since
    /// opening a project leaves it behind.
    pub fn set_world_note(&mut self, note: String) {
        if self.world_note != note {
            self.world_note = note;
            self.dirty = true;
        }
    }

    fn pages(&self) -> usize {
        self.rows.len().div_ceil(ROWS).max(1)
    }

    /// Where focus rests in the list: the first project of the page, else "New project".
    fn home_focus(&self) -> String {
        if self.rows.is_empty() {
            "st_new".to_owned()
        } else if self.page == 0 {
            "st_row_0".to_owned()
        } else {
            "st_row_0b".to_owned()
        }
    }

    fn back_to_list(&mut self) {
        self.mode = Mode::List;
        self.text.clear();
        self.composing.clear();
        self.focus = Some(self.home_focus());
        self.dirty = true;
    }

    fn turn(&mut self, by: i32) {
        let last = self.pages() - 1;
        let page = if by > 0 { (self.page + 1).min(last) } else { self.page.saturating_sub(1) };
        if page != self.page {
            self.page = page;
            self.focus = Some(self.home_focus());
            self.dirty = true;
        }
    }

    /// Confirm in a typing mode: the name or the folder, if there is one.
    fn confirm(&mut self) -> Option<Out> {
        let text = self.text.trim().to_owned();
        if text.is_empty() || self.busy {
            return None;
        }
        match self.mode {
            Mode::New if !crate::launch_list::valid_name(&text) => {
                self.status = ("A project's name is 1 to 64 characters, with none of / \\ : * ? \" < > | and no dot or space at its ends.".into(), false);
                self.dirty = true;
                None
            }
            Mode::New => {
                self.busy = true;
                Some(Out::Create(text))
            }
            Mode::Folder => {
                self.busy = true;
                Some(Out::Folder(text))
            }
            _ => None,
        }
    }

    fn ensure(&mut self, fonts: impl FnOnce() -> Result<FontSet, String>) -> bool {
        if self.ui.is_none() && self.problem.is_none() {
            match fonts().and_then(load).and_then(|ui| actions().map(|a| (ui, a))) {
                Ok((mut ui, map)) => {
                    let _ = ui.show(SCREEN_NAME);
                    self.ui = Some(ui);
                    self.input = Some(Input::new(map));
                }
                Err(e) => {
                    specula_core::trace_event!(&crate::trace_sink::ENGINE, Warn, "launch.screen_unavailable", error = e.as_str());
                    self.problem = Some(e);
                }
            }
        }
        self.ui.is_some()
    }

    /// Why the screen can't be drawn, if so (no system font): the engine closes it and says so.
    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    /// One window frame of the person's own input while it is open. Returns what they asked
    /// for, if anything.
    pub fn frame(&mut self, events: &[Event], pads: &[GamepadState], size: (u32, u32), fonts: impl FnOnce() -> Result<FontSet, String>) -> Option<Out> {
        if !self.open || size.0 == 0 || size.1 == 0 {
            return None;
        }
        let mut out = None;
        let typing = matches!(self.mode, Mode::New | Mode::Folder);
        for e in events {
            match e {
                Event::KeyDown { code: VK_CONTROL, .. } => self.ctrl = true,
                Event::KeyUp { code: VK_CONTROL } => self.ctrl = false,
                Event::Focused(false) => self.ctrl = false,
                _ if !typing => {}
                Event::Char(c) if !c.is_control() && !self.busy => {
                    let max = if self.mode == Mode::New { crate::launch_list::MAX_NAME_CHARS } else { MAX_PATH_CHARS };
                    if self.text.chars().count() < max {
                        self.text.push(*c);
                    }
                    self.dirty = true;
                }
                Event::ImeComposition { text, .. } => {
                    self.composing = text.chars().take(64).collect();
                    self.dirty = true;
                }
                Event::ImeCommit(_) | Event::ImeEnd => {
                    self.composing.clear();
                    self.dirty = true;
                }
                Event::KeyDown { code: VK_BACK, .. } if !self.busy => {
                    self.text.pop();
                    self.dirty = true;
                }
                Event::KeyDown { code: VK_V, repeat: false } if self.ctrl && self.mode == Mode::Folder && !self.busy => out = out.or(Some(Out::Paste)),
                Event::KeyDown { code: VK_RETURN, repeat: false } => out = out.or_else(|| self.confirm()),
                Event::KeyDown { code: VK_ESCAPE, repeat: false } if !self.busy => self.back_to_list(),
                _ => {}
            }
        }
        if !self.ensure(fonts) {
            return out;
        }
        // While a name or a folder is typed the keys are its words, not the UI's (Enter would
        // press the focused button too): the pointer and pads press the buttons. A key's release
        // always reaches the UI: it presses nothing, and the Enter that opened the field would
        // otherwise stay held there and not be heard as a press again.
        let passed: Vec<Event> = if typing {
            events.iter().filter(|e| !matches!(e, Event::KeyDown { .. } | Event::Char(_) | Event::ImeComposition { .. } | Event::ImeCommit(_) | Event::ImeEnd)).cloned().collect()
        } else {
            events.to_vec()
        };
        let now = self.started.get_or_insert_with(std::time::Instant::now).elapsed().as_secs_f64();
        let input = self.input.as_mut().expect("ensured");
        input.update(now, &passed, pads);
        if self.mode == Mode::List {
            let (next, prev) = (input.pressed("start_page_next"), input.pressed("start_page_prev"));
            if next {
                self.turn(1);
            }
            if prev {
                self.turn(-1);
            }
        }
        self.fill_if_dirty();
        let ui = self.ui.as_mut().expect("ensured");
        if !ui.is_visible(SCREEN_NAME) {
            let _ = ui.show(SCREEN_NAME);
        }
        let view = Viewport::new([size.0.max(1), size.1.max(1)], ui.theme().scale.scale([size.0.max(1), size.1.max(1)], None, 1.0));
        let events = ui.frame(self.input.as_ref().expect("ensured"), &self.state, &view).events.clone();
        self.laid_out = Some(size);
        let back = events.events.iter().any(|e| matches!(e, specula_ui::UiEvent::Back { .. }));
        let acts: Vec<String> = events.actions().map(str::to_owned).collect();
        for a in acts.iter().map(String::as_str).chain(back.then_some("start_back")) {
            match a {
                "start_close" => self.close("closed by the person"),
                // Back (Esc, a pad's B): out of a question first, then out of the screen.
                "start_back" | "start_cancel" if self.mode != Mode::List => {
                    if !self.busy {
                        self.back_to_list();
                    }
                }
                "start_back" => self.close("closed by the person"),
                "start_new" => {
                    self.mode = Mode::New;
                    self.text.clear();
                    self.status = (String::new(), true);
                    self.focus = Some("st_cancel".to_owned());
                    self.dirty = true;
                }
                "start_folder" => {
                    self.mode = Mode::Folder;
                    self.text.clear();
                    self.status = (String::new(), true);
                    self.focus = Some("st_cancel".to_owned());
                    self.dirty = true;
                }
                "start_ok" => out = out.or_else(|| self.confirm()),
                "start_prev" => self.turn(-1),
                "start_next" => self.turn(1),
                "start_forget_yes" => {
                    if let Mode::Forget(i) = self.mode {
                        if let Some(row) = self.rows.get(i) {
                            out = out.or(Some(Out::Forget(row.path.clone())));
                        }
                        self.back_to_list();
                    }
                }
                other => {
                    let slot = |prefix: &str| other.strip_prefix(prefix).and_then(|n| n.parse::<usize>().ok()).filter(|n| *n < ROWS).map(|n| self.page * ROWS + n);
                    if let Some(i) = slot("start_open:") {
                        match self.rows.get(i) {
                            Some(row) if row.missing => {
                                self.status = (format!("{}'s folder is gone ({}). Forget takes it off the list.", row.name, row.path.display()), false);
                                self.dirty = true;
                            }
                            // One engine a project: two would write the same worlds and caches.
                            Some(row) if row.running.is_some() => {
                                let port = row.running.unwrap_or(0);
                                self.status = (format!("{} is open now in an engine on port {port}: that engine's window is the one to use.", row.name), false);
                                self.dirty = true;
                            }
                            Some(row) if !self.busy => {
                                self.busy = true;
                                out = out.or(Some(Out::Open(row.path.clone())));
                            }
                            _ => {}
                        }
                    } else if let Some(i) = slot("start_forget:").filter(|i| *i < self.rows.len()) {
                        // Asked first; focus starts on "Keep it".
                        self.mode = Mode::Forget(i);
                        self.focus = Some("st_keep".to_owned());
                        self.dirty = true;
                    }
                }
            }
        }
        if let Some(id) = self.focus.take().filter(|_| self.open) {
            if let Some(ui) = self.ui.as_mut() {
                let refused = ui.widget(SCREEN_NAME, &id).is_none_or(|w| ui.focus(w).is_err());
                if refused {
                    if let Some(w) = ui.widget(SCREEN_NAME, "st_close") {
                        let _ = ui.focus(w);
                    }
                }
            }
        }
        out
    }

    fn fill_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let s = &mut self.state;
        s.reset();
        s.set_str("version", concat!("version ", env!("CARGO_PKG_VERSION")));
        s.set_bool("listing", self.mode == Mode::List);
        s.set_bool("typing", matches!(self.mode, Mode::New | Mode::Folder));
        s.set_bool("forgetting", matches!(self.mode, Mode::Forget(_)));
        s.set_bool("has_rows", !self.rows.is_empty());
        s.set_bool("first_page", self.page == 0);
        s.set_str("list_title", if self.rows.is_empty() { "Projects" } else { "Projects, the last one worked on first" });
        let pages = self.rows.len().div_ceil(ROWS).max(1);
        s.set_bool("paged", pages > 1);
        s.set_bool("has_prev", self.page > 0);
        s.set_bool("has_next", self.page + 1 < pages);
        s.set_str("page_line", &format!("{} of {pages}", self.page + 1));
        for n in 0..ROWS {
            let i = self.page * ROWS + n;
            let row = self.rows.get(i);
            s.set_bool(&format!("row_{n}"), row.is_some());
            let Some(row) = row else { continue };
            s.set_str(&format!("row_{n}_name"), &format!("{}. {}", i + 1, tail(&row.name, 28)));
            s.set_str(&format!("row_{n}_path"), &tail_path(&row.path.display().to_string(), PATH_CHARS));
            let when = ago(self.now, row.last_opened);
            let line = match (row.missing, row.running) {
                (true, _) => "its folder is gone".to_owned(),
                (false, Some(port)) => format!("open now in an engine on port {port}"),
                // (Every row says it the same way; the order and the first row's button say
                // which was last.)
                (false, None) if row.last_opened == 0 => "not opened yet".to_owned(),
                (false, None) => format!("last opened {when}"),
            };
            s.set_str(&format!("row_{n}_when"), &line);
            s.set_bool(&format!("row_{n}_warn"), row.missing);
        }
        let caret = if self.busy { "" } else { "_" };
        s.set_str("typed", &format!("{}{}{caret}", tail(&self.text, SHOWN_CHARS), self.composing));
        s.set_bool("can_ok", !self.text.trim().is_empty() && !self.busy);
        match self.mode {
            Mode::New => {
                s.set_str("type_title", "New project: its name");
                s.set_str("type_ok", "Make it and open it");
                let parent = self.parent.as_ref().map_or_else(|| "(this machine names no home folder: use 'specula new NAME --in FOLDER')".to_owned(), |p| p.display().to_string());
                s.set_str("type_note", &format!("It is made as a folder of that name in {parent}, with an assets folder. Enter makes it, Esc goes back."));
            }
            Mode::Folder => {
                s.set_str("type_title", "Open a folder as a project: its path");
                s.set_str("type_ok", "Open it");
                s.set_str("type_note", "Type the folder's path, or paste it with Ctrl+V. A folder that is not a project yet gets one small marker file (specula.project.json); nothing else in it is touched. Enter opens it, Esc goes back.");
            }
            Mode::Forget(i) => {
                let (name, path) = self.rows.get(i).map_or((String::new(), String::new()), |r| (r.name.clone(), r.path.display().to_string()));
                s.set_str("forget_line", &format!("Take {name} off this list? Its folder ({path}) and everything in it stay as they are; opening the folder again lists it again."));
            }
            Mode::List => {}
        }
        s.set_str("world_note", &self.world_note);
        s.set_str("status_line", &self.status.0);
        s.set_bool("status_good", self.status.1);
    }

    /// Its quads for one window frame, over everything else of the engine's (within `room`).
    pub fn overlay<G: OverlayTextures + ?Sized>(&mut self, gpu: &mut G, size: (u32, u32), room: usize, out: &mut Vec<OverlayQuad>) {
        if !self.open || self.ui.is_none() || size.0 == 0 || size.1 == 0 {
            return;
        }
        if self.laid_out != Some(size) || self.dirty {
            self.fill_if_dirty();
            let ui = self.ui.as_mut().expect("loaded");
            let view = Viewport::new([size.0, size.1], ui.theme().scale.scale([size.0, size.1], None, 1.0));
            if let Some(input) = self.input.as_mut() {
                input.update(self.started.get_or_insert_with(std::time::Instant::now).elapsed().as_secs_f64(), &[], &[]);
                ui.frame(input, &self.state, &view);
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

    #[cfg(test)]
    pub(crate) fn folder_mode_for_test(&mut self) {
        self.mode = Mode::Folder;
        self.text.clear();
        self.dirty = true;
    }

    #[cfg(test)]
    pub(crate) fn typed_for_test(&self) -> &str {
        &self.text
    }

    #[cfg(test)]
    pub(crate) fn status_for_test(&self) -> &str {
        &self.status.0
    }

    /// Where a widget is on screen (window pixels), once laid out: the live check presses the
    /// screen's buttons by posting pointer messages to the engine's own window.
    #[cfg(test)]
    fn rect_of(&self, id: &str) -> Option<[f32; 4]> {
        let ui = self.ui.as_ref()?;
        let r = ui.rect(ui.widget(SCREEN_NAME, id)?)?;
        (r.w > 0.0 && r.h > 0.0).then_some([r.x, r.y, r.w, r.h])
    }

    #[cfg(test)]
    fn state_str(&self, key: &str) -> String {
        match self.state.get(key) {
            Some(specula_ui::Value::Str(s)) => s.clone(),
            Some(specula_ui::Value::Bool(b)) => b.to_string(),
            Some(specula_ui::Value::Int(i)) => i.to_string(),
            _ => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_platform::MouseButton;

    /// The UI crate's built-in test font: these tests need no font of the machine's.
    fn fonts() -> Result<FontSet, String> {
        Ok(FontSet::new().with("ui", specula_ui::testfont::box_font_parsed()))
    }

    fn rows(n: usize) -> Vec<Row> {
        (0..n).map(|i| Row { name: format!("game{i}"), path: PathBuf::from(format!("D:/games/game{i}")), last_opened: 1_000_000u64.saturating_sub(i as u64 * 100_000).max(1), missing: false, running: None }).collect()
    }

    /// A screen with `n` projects, laid out once.
    fn screen(n: usize) -> StartScreen {
        let mut s = StartScreen::default();
        s.open(rows(n), Some(PathBuf::from("C:/people/Specula Projects")), 1_000_050);
        s.frame(&[], &[], (1280, 800), fonts);
        assert_eq!(s.problem(), None, "the built-in screen loads");
        s
    }

    fn click(s: &mut StartScreen, id: &str) -> Option<Out> {
        let r = s.rect_of(id).unwrap_or_else(|| panic!("{id} is not on screen"));
        let (x, y) = ((r[0] + r[2] / 2.0) as i32, (r[1] + r[3] / 2.0) as i32);
        s.frame(&[Event::MouseMove { x, y }], &[], (1280, 800), fonts);
        s.frame(&[Event::MouseDown { button: MouseButton::Left, x, y }], &[], (1280, 800), fonts);
        let out = s.frame(&[Event::MouseUp { button: MouseButton::Left, x, y }], &[], (1280, 800), fonts);
        s.frame(&[], &[], (1280, 800), fonts);
        out
    }

    fn key(code: u32) -> Event {
        Event::KeyDown { code, repeat: false }
    }

    fn typed(s: &mut StartScreen, text: &str) {
        s.frame(&text.chars().map(Event::Char).collect::<Vec<_>>(), &[], (1280, 800), fonts);
    }

    #[test]
    fn times_read_as_a_person_says_them() {
        let now = 100_000_000;
        let said: Vec<String> = [0u64, 60, 100, 3_000, 7_200, 80_000, 100_000, 3 * 86_400, 20 * 86_400, 90 * 86_400, 800 * 86_400].iter().map(|s| ago(now, now - s)).collect();
        assert_eq!(said, ["just now", "just now", "2 minutes ago", "50 minutes ago", "2 hours ago", "22 hours ago", "yesterday", "3 days ago", "2 weeks ago", "3 months ago", "over a year ago"]);
        assert_eq!(ago(now, 0), "never opened");
        assert_eq!(ago(5, 50), "just now", "a clock that went back");
        assert_eq!((ago(now, now - 90), ago(now, now - 5_400)), ("2 minutes ago".to_owned(), "2 hours ago".to_owned()));
    }

    #[test]
    fn a_long_path_is_shown_from_a_folders_name_on() {
        assert_eq!(tail_path("D:/games/crypt", 62), "D:/games/crypt");
        let long = "D:\\Work\\someone\\Documents\\Specula Projects\\A Very Long Studio Name\\Frozen Wasteland";
        let shown = tail_path(long, 44);
        assert!(shown.starts_with("\u{2026}\\") && shown.ends_with("\\Frozen Wasteland") && shown.chars().count() <= 45, "{shown}");
        // One very long folder name: its end, rather than nothing.
        let shown = tail_path(&format!("D:/{}", "x".repeat(100)), 20);
        assert_eq!(shown, format!("\u{2026}{}", "x".repeat(20)));
        // A row that was never opened says so.
        let mut s = screen(0);
        s.set_rows(vec![Row { name: "new".into(), path: PathBuf::from("D:/games/new"), last_opened: 0, missing: false, running: None }]);
        s.frame(&[], &[], (1280, 800), fonts);
        assert_eq!(s.state_str("row_0_when"), "not opened yet");
    }

    #[test]
    fn a_pasted_path_is_one_clean_line() {
        assert_eq!(pasted_path("\"D:\\Games\\My Crypt\"\r\n"), "D:\\Games\\My Crypt");
        assert_eq!(pasted_path("\n  D:/games/crypt  \nsecond line"), "D:/games/crypt");
        assert_eq!(pasted_path("a\u{7}b\tc"), "abc");
        assert_eq!(pasted_path(&"x".repeat(MAX_PATH_CHARS + 40)).chars().count(), MAX_PATH_CHARS);
        assert_eq!(pasted_path(""), "");
    }

    #[test]
    fn the_last_project_is_first_and_enter_resumes_it() {
        let mut s = screen(3);
        assert_eq!(s.state_str("row_0_name"), "1. game0");
        assert_eq!(s.state_str("row_0_when"), "last opened just now");
        assert_eq!(s.state_str("row_1_when"), "last opened yesterday");
        assert_eq!(s.state_str("list_title"), "Projects, the last one worked on first");
        // Focus starts on the first project: Enter opens it with no pointer at all.
        assert_eq!(s.frame(&[key(VK_RETURN)], &[], (1280, 800), fonts), Some(Out::Open(PathBuf::from("D:/games/game0"))));
        // One request at a time: a second press while the engine is on its way sends nothing.
        assert_eq!(s.frame(&[Event::KeyUp { code: VK_RETURN }], &[], (1280, 800), fonts), None);
        assert_eq!(s.frame(&[key(VK_RETURN)], &[], (1280, 800), fonts), None);
        // Refused (its folder went meanwhile): said, and the person can go on.
        s.answered(Err("the folder is gone".into()));
        s.frame(&[Event::KeyUp { code: VK_RETURN }], &[], (1280, 800), fonts);
        assert_eq!(s.state_str("status_line"), "the folder is gone");
        assert_eq!(click(&mut s, "st_row_1"), Some(Out::Open(PathBuf::from("D:/games/game1"))));
    }

    #[test]
    fn an_empty_list_says_so_and_offers_a_new_project() {
        let mut s = screen(0);
        assert_eq!(s.state_str("has_rows"), "false");
        assert!(s.rect_of("st_row_0").is_none() && s.rect_of("st_new").is_some());
        // Focus is on "New project": Enter asks for its name; a name is typed; Enter makes it.
        assert_eq!(s.frame(&[key(VK_RETURN)], &[], (1280, 800), fonts), None);
        s.frame(&[Event::KeyUp { code: VK_RETURN }], &[], (1280, 800), fonts);
        assert_eq!(s.state_str("typing"), "true");
        typed(&mut s, "Frozen Wastelanx");
        s.frame(&[key(VK_BACK)], &[], (1280, 800), fonts);
        typed(&mut s, "d");
        assert_eq!(s.state_str("typed"), "Frozen Wasteland_");
        assert_eq!(s.frame(&[key(VK_RETURN)], &[], (1280, 800), fonts), Some(Out::Create("Frozen Wasteland".into())));
        // Refused by the disk: the name is still there to change.
        s.answered(Err("D:/x/Frozen Wasteland is not empty".into()));
        s.frame(&[], &[], (1280, 800), fonts);
        assert_eq!((s.state_str("typed"), s.state_str("status_good")), ("Frozen Wasteland_".to_owned(), "false".to_owned()));
        // A name no folder can have is refused here, before the disk is asked.
        typed(&mut s, "?");
        assert_eq!(s.frame(&[key(VK_RETURN)], &[], (1280, 800), fonts), None);
        assert!(s.state_str("status_line").contains("A project's name"));
        // Esc goes back to the list (not out of the screen). The Enter that opened the field
        // was released while it was open: it is heard as a press again (it opens the field).
        s.frame(&[key(VK_ESCAPE)], &[], (1280, 800), fonts);
        s.frame(&[], &[], (1280, 800), fonts);
        assert!(s.is_open() && s.state_str("listing") == "true", "Esc while typing goes back to the list");
        s.frame(&[Event::KeyUp { code: VK_ESCAPE }], &[], (1280, 800), fonts);
        s.frame(&[key(VK_RETURN)], &[], (1280, 800), fonts);
        s.frame(&[Event::KeyUp { code: VK_RETURN }], &[], (1280, 800), fonts);
        assert_eq!(s.state_str("typing"), "true", "Enter on New project is heard again");
        s.frame(&[key(VK_ESCAPE)], &[], (1280, 800), fonts);
        s.frame(&[], &[], (1280, 800), fonts);
        assert_eq!(s.state_str("listing"), "true");
        // Esc again closes the screen.
        s.frame(&[Event::KeyUp { code: VK_ESCAPE }], &[], (1280, 800), fonts);
        s.frame(&[key(VK_ESCAPE)], &[], (1280, 800), fonts);
        assert!(!s.is_open(), "Esc in the list closes the screen: the engine goes on with no project");
    }

    #[test]
    fn a_folder_is_typed_or_pasted_and_keys_typed_never_press_buttons() {
        let mut s = screen(2);
        assert_eq!(click(&mut s, "st_folder"), None);
        // Ctrl+V asks for the clipboard; what comes back is one clean line.
        assert_eq!(s.frame(&[key(VK_CONTROL), key(VK_V)], &[], (1280, 800), fonts), Some(Out::Paste));
        s.pasted("\"D:\\Games\\crypt\"\n");
        s.frame(&[Event::KeyUp { code: VK_V }, Event::KeyUp { code: VK_CONTROL }], &[], (1280, 800), fonts);
        assert_eq!(s.state_str("typed"), "D:\\Games\\crypt_");
        // A space and an Enter are the path's and the confirm's: no button under focus is
        // pressed by them (the folder is sent exactly once).
        typed(&mut s, " 2");
        assert_eq!(s.frame(&[key(VK_RETURN)], &[], (1280, 800), fonts), Some(Out::Folder("D:\\Games\\crypt 2".into())));
        assert_eq!(s.frame(&[Event::KeyUp { code: VK_RETURN }, key(VK_RETURN)], &[], (1280, 800), fonts), None, "busy: nothing more is sent");
        // While the engine checks the folder, typing and Esc wait.
        typed(&mut s, "zzz");
        s.frame(&[key(VK_ESCAPE)], &[], (1280, 800), fonts);
        assert_eq!(s.state_str("typing"), "true");
        s.answered(Err("there is no such folder".into()));
        s.frame(&[], &[], (1280, 800), fonts);
        assert_eq!(s.state_str("typed"), "D:\\Games\\crypt 2_");
        // Ctrl+V in the name field asks nothing (a name is typed).
        assert_eq!(click(&mut s, "st_cancel"), None);
        assert_eq!(click(&mut s, "st_new"), None);
        assert_eq!(s.frame(&[key(VK_CONTROL), key(VK_V)], &[], (1280, 800), fonts), None);
        // A path is cut at its limit.
        s.frame(&[Event::KeyUp { code: VK_CONTROL }], &[], (1280, 800), fonts);
        assert_eq!(click(&mut s, "st_cancel"), None);
        assert_eq!(click(&mut s, "st_folder"), None);
        s.pasted(&"p".repeat(MAX_PATH_CHARS + 9));
        s.pasted("more");
        assert_eq!(s.text.chars().count(), MAX_PATH_CHARS);
    }

    #[test]
    fn forgetting_asks_first_and_a_missing_folder_is_never_opened() {
        let mut s = screen(3);
        let mut list = rows(3);
        list[1].missing = true;
        list[2].running = Some(7480);
        s.set_rows(list);
        s.frame(&[], &[], (1280, 800), fonts);
        assert_eq!((s.state_str("row_1_when"), s.state_str("row_1_warn")), ("its folder is gone".to_owned(), "true".to_owned()));
        assert_eq!(s.state_str("row_2_when"), "open now in an engine on port 7480");
        // A project another engine has open is not opened a second time: said instead.
        assert_eq!(click(&mut s, "st_row_2"), None);
        assert!(s.state_str("status_line").contains("open now in an engine on port 7480") && s.state_str("status_good") == "false");
        assert_eq!(click(&mut s, "st_row_1"), None);
        assert!(s.state_str("status_line").contains("folder is gone"));
        // Forget asks; "Keep it" has the focus, so an Enter keeps it.
        assert_eq!(click(&mut s, "st_forget_1"), None);
        assert_eq!(s.state_str("forgetting"), "true");
        assert!(s.state_str("forget_line").contains("Take game1 off this list"));
        assert_eq!(s.frame(&[key(VK_RETURN)], &[], (1280, 800), fonts), None);
        s.frame(&[Event::KeyUp { code: VK_RETURN }], &[], (1280, 800), fonts);
        assert_eq!(s.state_str("listing"), "true");
        // Asked again and confirmed: the engine is told which.
        assert_eq!(click(&mut s, "st_forget_1"), None);
        assert_eq!(click(&mut s, "st_forget_yes"), Some(Out::Forget(PathBuf::from("D:/games/game1"))));
        assert_eq!(s.state_str("listing"), "true");
    }

    #[test]
    fn folders_checked_later_change_what_rows_say_and_not_where_the_person_is() {
        let mut s = screen(ROWS + 2);
        assert_eq!(click(&mut s, "st_next"), None);
        assert_eq!(click(&mut s, "st_forget_1"), None);
        assert_eq!(s.state_str("forgetting"), "true");
        // The check comes back while a question is on screen: same list, new facts.
        let mut list = rows(ROWS + 2);
        list[0].missing = true;
        list[ROWS].running = Some(7480);
        s.checked(list);
        s.frame(&[], &[], (1280, 800), fonts);
        assert_eq!((s.state_str("forgetting"), s.state_str("page_line")), ("true".to_owned(), "2 of 2".to_owned()), "the question and the page stay");
        assert!(s.state_str("forget_line").contains(&format!("game{}", ROWS + 1)), "and it is still about the same project");
        assert_eq!(click(&mut s, "st_keep"), None);
        assert_eq!(s.state_str("row_0_when"), "open now in an engine on port 7480");
        // Another list (a project was forgotten from a terminal meanwhile): a question about a
        // row that may be another project now is dropped, and the list is shown.
        assert_eq!(click(&mut s, "st_forget_1"), None);
        s.checked(rows(ROWS));
        s.frame(&[], &[], (1280, 800), fonts);
        assert_eq!((s.state_str("listing"), s.state_str("paged")), ("true".to_owned(), "false".to_owned()));
        // A name being typed is never thrown away by it.
        assert_eq!(click(&mut s, "st_new"), None);
        typed(&mut s, "abc");
        s.checked(rows(2));
        s.frame(&[], &[], (1280, 800), fonts);
        assert_eq!((s.state_str("typing"), s.state_str("typed")), ("true".to_owned(), "abc_".to_owned()));
    }

    #[test]
    fn a_long_list_is_paged_and_a_slot_means_the_project_shown_in_it() {
        let mut s = screen(ROWS * 2 + 1);
        assert_eq!((s.state_str("paged"), s.state_str("page_line")), ("true".to_owned(), "1 of 3".to_owned()));
        assert_eq!(click(&mut s, "st_next"), None);
        assert_eq!(s.state_str("row_0_name"), format!("{}. game{ROWS}", ROWS + 1));
        // The first slot of a later page is not the last project worked on: an ordinary button.
        assert!(s.rect_of("st_row_0").is_none() && s.rect_of("st_row_0b").is_some());
        assert_eq!(click(&mut s, "st_row_0b"), Some(Out::Open(PathBuf::from(format!("D:/games/game{ROWS}")))));
        s.answered(Err("no".into()));
        // PageDown turns the page; the last page shows one project and no empty slot opens one.
        s.frame(&[key(0x22)], &[], (1280, 800), fonts);
        s.frame(&[Event::KeyUp { code: 0x22 }], &[], (1280, 800), fonts);
        assert_eq!((s.state_str("page_line"), s.state_str("row_1")), ("3 of 3".to_owned(), "false".to_owned()));
        assert!(s.rect_of("st_row_1").is_none());
        // The list shrank under the page shown: it steps back to a page that exists.
        s.set_rows(rows(2));
        s.frame(&[], &[], (1280, 800), fonts);
        assert_eq!((s.state_str("paged"), s.state_str("row_0_name")), ("false".to_owned(), "1. game0".to_owned()));
    }

    #[test]
    fn close_leaves_the_engine_with_no_project_and_a_closed_screen_hears_nothing() {
        let mut s = screen(2);
        assert_eq!(click(&mut s, "st_close"), None);
        assert!(!s.is_open());
        assert_eq!(s.frame(&[key(VK_RETURN)], &[], (1280, 800), fonts), None);
        let mut quads = Vec::new();
        struct NoGpu;
        impl OverlayTextures for NoGpu {
            fn upload_texture(&mut self, _: u32, _: u32, _: specula_gpu::TextureFormat, _: &[u8]) -> Result<specula_gpu::TextureId, specula_gpu::GpuError> {
                unreachable!("a closed screen draws nothing")
            }
            fn update_texture(&mut self, _: specula_gpu::TextureId, _: u32, _: u32, _: u32, _: u32, _: &[u8]) -> Result<(), specula_gpu::GpuError> {
                unreachable!()
            }
            fn destroy_texture(&mut self, _: specula_gpu::TextureId) {}
        }
        s.overlay(&mut NoGpu, (1280, 800), 1000, &mut quads);
        assert!(quads.is_empty());
    }
}
